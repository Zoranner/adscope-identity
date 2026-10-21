use adscope_protocol::{
    CredentialBatch, DirectoryBatch, SyncChannel, SyncerConfirmRequest, SyncerSyncRequest,
};
use adscope_syncer::{
    ConfiguredDirectoryClient, ControlPlaneClient, HttpControlPlaneClient, LdapDirectoryConfig,
    SyncerProcessConfig,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(1);

#[test]
fn http_control_plane_client_builds_endpoints_from_trailing_slash_base_url() {
    let client =
        HttpControlPlaneClient::new("http://127.0.0.1:8080/", "syncer-a-key", HTTP_TIMEOUT)
            .unwrap();

    assert_eq!(
        client.endpoint("/api/syncer/sync"),
        "http://127.0.0.1:8080/api/syncer/sync"
    );
    assert_eq!(
        client.endpoint("api/syncer/confirm"),
        "http://127.0.0.1:8080/api/syncer/confirm"
    );
}

#[tokio::test]
async fn http_control_plane_client_posts_sync_request_with_syncer_key() {
    let server = OneShotHttpServer::start(
        r#"{"directory":{"server_revision":0,"batch_revision":0,"organizational_units":[],"organizational_unit_dns":{},"users":[],"groups":[],"has_more":false},"credentials":{"server_revision":0,"batch_revision":0,"credentials":[],"has_more":false},"directory_config":{"domain_id":"domain-a","mirror_root_dn":"OU=Mirror,DC=example,DC=com","quarantine_ou_dn":"OU=Quarantine,DC=example,DC=com","upn_suffix":"example.com","employee_id_attribute":"employeeID","managed_group_id_attribute":"adminDescription"}}"#,
    )
    .await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let response = client
        .sync(SyncerSyncRequest {
            domain_id: "domain-a".to_string(),
            applied_directory_revision: 3,
            applied_credential_revision: 7,
            rebuild_directory: true,
            rebuild_credentials: false,
        })
        .await
        .unwrap();

    let request = server.request().await;
    assert!(request.starts_with("POST /api/syncer/sync HTTP/1.1"));
    assert!(request.contains("x-adscope-syncer-key: syncer-a-key"));
    assert!(!request.contains("x-adss-connector-key:"));
    assert!(request.contains(r#""applied_directory_revision":3"#));
    assert!(request.contains(r#""applied_credential_revision":7"#));
    assert!(request.contains(r#""rebuild_directory":true"#));
    assert!(request.contains(r#""rebuild_credentials":false"#));
    assert_eq!(response.directory, empty_directory_batch());
    assert_eq!(response.credentials, empty_credential_batch());
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_sync_non_success_status() {
    let server = OneShotHttpServer::start_with_status(503, r#"{"error":"unavailable"}"#).await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let result = client.sync(sync_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_sync_bad_json() {
    let server = OneShotHttpServer::start("not-json").await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let result = client.sync(sync_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_posts_confirm_request_with_syncer_key() {
    let server = OneShotHttpServer::start(r#"{"accepted":true}"#).await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let response = client
        .confirm(SyncerConfirmRequest {
            domain_id: "domain-a".to_string(),
            channel: SyncChannel::Credential,
            target_revision: 11,
            success: true,
            error_code: None,
        })
        .await
        .unwrap();

    let request = server.request().await;
    assert!(request.starts_with("POST /api/syncer/confirm HTTP/1.1"));
    assert!(request.contains("x-adscope-syncer-key: syncer-a-key"));
    assert!(!request.contains("x-adss-connector-key:"));
    assert!(request.contains(r#""channel":"credential""#));
    assert!(request.contains(r#""target_revision":11"#));
    assert!(request.contains(r#""success":true"#));
    assert!(response.accepted);
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_confirm_non_success_status() {
    let server = OneShotHttpServer::start_with_status(401, r#"{"error":"unauthorized"}"#).await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let result = client.confirm(confirm_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_confirm_bad_json() {
    let server = OneShotHttpServer::start("not-json").await;
    let client =
        HttpControlPlaneClient::new(server.base_url(), "syncer-a-key", HTTP_TIMEOUT).unwrap();

    let result = client.confirm(confirm_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_times_out_when_server_does_not_respond() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let client = HttpControlPlaneClient::new(
        format!("http://{address}"),
        "syncer-a-key",
        Duration::from_millis(50),
    )
    .unwrap();

    let error = client.sync(sync_request()).await.unwrap_err();

    assert!(
        error
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_timeout)
    );
    server.abort();
}

#[test]
fn syncer_process_config_uses_explicit_values_and_defaults() {
    let config = SyncerProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "syncer-a-key",
        "syncer-state.json",
        15,
        LdapDirectoryConfig {
            url: "ldap://dc-a.example.com:389".to_string(),
            server_fqdn: "dc-a.example.com".to_string(),
            adopt_existing_users_by_username: false,
        },
    );

    assert_eq!(config.core_url, "https://sync.example.com");
    assert_eq!(config.domain_id, "domain-a");
    assert_eq!(config.syncer_key, "syncer-a-key");
    assert_eq!(config.state_path, "syncer-state.json");
    assert_eq!(config.interval_seconds, 15);
    assert_eq!(config.http_timeout_seconds, 60);
    assert_eq!(config.operation_timeout_seconds, 60);
    assert_eq!(config.ldap.url, "ldap://dc-a.example.com:389");
}

#[test]
fn syncer_process_config_debug_redacts_secrets() {
    let config = SyncerProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "syncer-a-key",
        "syncer-state.json",
        30,
        LdapDirectoryConfig {
            url: "ldap://dc-a.example.com:389".to_string(),
            server_fqdn: "dc-a.example.com".to_string(),
            adopt_existing_users_by_username: false,
        },
    );

    let debug = format!("{config:?}");

    assert!(!debug.contains("syncer-a-key"));
    assert!(debug.matches("[redacted]").count() >= 1);
}

#[test]
fn syncer_process_config_rejects_zero_timeouts_from_env() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in [
        "CORE_REQUEST_TIMEOUT_SECONDS",
        "DIRECTORY_OPERATION_TIMEOUT_SECONDS",
    ] {
        clear_syncer_env();
        unsafe {
            std::env::set_var("CORE_URL", "https://sync.example.com");
            std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
            std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
            std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "ldap://dc-a.example.com:389");
            std::env::set_var(name, "0");
        }

        let error = SyncerProcessConfig::from_env().unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("{name} must be greater than 0"))
        );
    }
    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_retired_adss_environment_variables() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("ADSS_CENTER_URL", "https://sync.example.com");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert_eq!(
        error.to_string(),
        "ADSS_CENTER_URL is retired; use CORE_URL"
    );
    clear_syncer_env();
}

#[test]
fn syncer_process_config_uses_aligned_environment_variable_names() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("CORE_URL", "https://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
        std::env::set_var("SYNC_INTERVAL_SECONDS", "15");
        std::env::set_var("CORE_REQUEST_TIMEOUT_SECONDS", "20");
        std::env::set_var("DIRECTORY_OPERATION_TIMEOUT_SECONDS", "30");
        std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "ldap://dc-a.example.com:389");
        std::env::set_var("MATCH_LEGACY_USERS_BY_USERNAME", "true");
    }

    let config = SyncerProcessConfig::from_env().unwrap();

    assert_eq!(config.core_url, "https://sync.example.com");
    assert_eq!(config.domain_id, "domain-a");
    assert_eq!(config.syncer_key, "syncer-a-key");
    assert_eq!(config.interval_seconds, 15);
    assert_eq!(config.http_timeout_seconds, 20);
    assert_eq!(config.operation_timeout_seconds, 30);
    assert_eq!(config.ldap.url, "ldap://dc-a.example.com:389");
    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_retired_adscope_environment_variables() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("ADSCOPE_CENTER_URL", "https://sync.example.com");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert_eq!(
        error.to_string(),
        "ADSCOPE_CENTER_URL is retired; use CORE_URL"
    );
    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_removed_dry_run_variable() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("ADSCOPE_CONNECTOR_DRY_RUN", "1");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert_eq!(
        error.to_string(),
        "ADSCOPE_CONNECTOR_DRY_RUN is retired; no replacement is available"
    );
    clear_syncer_env();
}

#[test]
fn syncer_process_config_requires_https_core_url() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("CORE_URL", "http://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
        std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "ldap://dc-a.example.com:389");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert!(error.to_string().contains("CORE_URL must use https://"));
    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_zero_interval_from_env() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    unsafe {
        std::env::set_var("CORE_URL", "https://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
        std::env::set_var("SYNC_INTERVAL_SECONDS", "0");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("SYNC_INTERVAL_SECONDS must be greater than 0")
    );

    unsafe {
        std::env::remove_var("CORE_URL");
        std::env::remove_var("DIRECTORY_DOMAIN_ID");
        std::env::remove_var("SYNCER_ACCESS_KEY");
        std::env::remove_var("SYNC_INTERVAL_SECONDS");
        std::env::remove_var("CORE_REQUEST_TIMEOUT_SECONDS");
        std::env::remove_var("DIRECTORY_OPERATION_TIMEOUT_SECONDS");
    }
}

#[test]
fn syncer_process_config_requires_domain_controller_ldap_url() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("CORE_URL", "https://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("DOMAIN_CONTROLLER_LDAP_URL is required")
    );

    clear_syncer_env();
}

#[test]
fn syncer_process_config_accepts_domain_controller_ldap_url() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("CORE_URL", "https://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
        std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "ldap://dc-a.example.com:389");
    }

    let config = SyncerProcessConfig::from_env().unwrap();
    assert_eq!(config.ldap.url, "ldap://dc-a.example.com:389");

    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_invalid_domain_controller_ldap_url() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_syncer_env();
    unsafe {
        std::env::set_var("CORE_URL", "https://sync.example.com");
        std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
        std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
        std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "http://dc-a.example.com");
    }

    let error = SyncerProcessConfig::from_env().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("DOMAIN_CONTROLLER_LDAP_URL must be ldap://<FQDN>:389")
    );

    clear_syncer_env();
}

#[test]
fn syncer_process_config_rejects_ldap_downgrade_and_legacy_credentials() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for url in [
        "ldaps://dc-a.example.com:636",
        "ldap://192.168.2.6:389",
        "ldap://dc-a.example.com:1389",
        "ldap://dc-a.example.com:389/ou=Mirror",
        "ldap://dc-a.example.com:389?starttls=1",
        "ldap://user@dc-a.example.com:389",
        "ldap://dc-a.example.com:389#fragment",
    ] {
        clear_syncer_env();
        unsafe {
            std::env::set_var("CORE_URL", "https://sync.example.com");
            std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
            std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
            std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", url);
        }
        assert!(SyncerProcessConfig::from_env().is_err(), "accepted {url}");
    }

    for legacy_name in [
        "ADSCOPE_LDAP_BIND_DN",
        "ADSCOPE_LDAP_BIND_PASSWORD",
        "ADSCOPE_LDAP_ACCEPT_INVALID_CERTS",
    ] {
        clear_syncer_env();
        unsafe {
            std::env::set_var("CORE_URL", "https://sync.example.com");
            std::env::set_var("DIRECTORY_DOMAIN_ID", "domain-a");
            std::env::set_var("SYNCER_ACCESS_KEY", "syncer-a-key");
            std::env::set_var("DOMAIN_CONTROLLER_LDAP_URL", "ldap://dc-a.example.com:389");
            std::env::set_var(legacy_name, "legacy");
        }
        let error = SyncerProcessConfig::from_env().unwrap_err();
        assert!(error.to_string().contains(legacy_name));
    }

    clear_syncer_env();
}

#[test]
fn configured_directory_client_selects_ldap() {
    let config = SyncerProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "syncer-a-key",
        "syncer-state.json",
        30,
        LdapDirectoryConfig {
            url: "ldap://dc-a.example.com:389".to_string(),
            server_fqdn: "dc-a.example.com".to_string(),
            adopt_existing_users_by_username: false,
        },
    );

    let _client = ConfiguredDirectoryClient::from_process_config(&config).unwrap();
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clear_syncer_env() {
    unsafe {
        std::env::remove_var("ADSS_CENTER_URL");
        std::env::remove_var("CORE_URL");
        std::env::remove_var("ADSCOPE_CENTER_URL");
        std::env::remove_var("ADSCOPE_DOMAIN_ID");
        std::env::remove_var("DIRECTORY_DOMAIN_ID");
        std::env::remove_var("ADSCOPE_CONNECTOR_KEY");
        std::env::remove_var("SYNCER_ACCESS_KEY");
        std::env::remove_var("ADSCOPE_CONNECTOR_INTERVAL_SECONDS");
        std::env::remove_var("SYNC_INTERVAL_SECONDS");
        std::env::remove_var("ADSCOPE_CONNECTOR_HTTP_TIMEOUT_SECONDS");
        std::env::remove_var("CORE_REQUEST_TIMEOUT_SECONDS");
        std::env::remove_var("ADSCOPE_CONNECTOR_OPERATION_TIMEOUT_SECONDS");
        std::env::remove_var("DIRECTORY_OPERATION_TIMEOUT_SECONDS");
        std::env::remove_var("ADSCOPE_CONNECTOR_DRY_RUN");
        std::env::remove_var("ADSCOPE_LDAP_URL");
        std::env::remove_var("DOMAIN_CONTROLLER_LDAP_URL");
        std::env::remove_var("ADSCOPE_LDAP_BIND_DN");
        std::env::remove_var("ADSCOPE_LDAP_BIND_PASSWORD");
        std::env::remove_var("ADSCOPE_LDAP_ACCEPT_INVALID_CERTS");
        std::env::remove_var("ADSCOPE_ADOPT_EXISTING_USERS_BY_USERNAME");
        std::env::remove_var("MATCH_LEGACY_USERS_BY_USERNAME");
    }
}

struct OneShotHttpServer {
    base_url: String,
    request: tokio::task::JoinHandle<String>,
}

impl OneShotHttpServer {
    async fn start(response_body: &'static str) -> Self {
        Self::start_with_status(200, response_body).await
    }

    async fn start_with_status(status: u16, response_body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 8192];
            let bytes_read = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
            let response = format!(
                "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                status,
                reason_phrase(status),
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });

        Self {
            base_url: format!("http://{address}"),
            request,
        }
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    async fn request(self) -> String {
        self.request.await.unwrap()
    }
}

fn sync_request() -> SyncerSyncRequest {
    SyncerSyncRequest {
        domain_id: "domain-a".to_string(),
        applied_directory_revision: 3,
        applied_credential_revision: 7,
        rebuild_directory: false,
        rebuild_credentials: false,
    }
}

fn confirm_request() -> SyncerConfirmRequest {
    SyncerConfirmRequest {
        domain_id: "domain-a".to_string(),
        channel: SyncChannel::Directory,
        target_revision: 5,
        success: true,
        error_code: None,
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn empty_directory_batch() -> DirectoryBatch {
    DirectoryBatch {
        server_revision: 0,
        batch_revision: 0,
        organizational_units: Vec::new(),
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: Vec::new(),
        groups: Vec::new(),
        deleted_users: Vec::new(),
        has_more: false,
    }
}

fn empty_credential_batch() -> CredentialBatch {
    CredentialBatch {
        server_revision: 0,
        batch_revision: 0,
        credentials: Vec::new(),
        has_more: false,
    }
}

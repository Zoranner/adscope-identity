use adss_agent::{
    AgentProcessConfig, ConfiguredDirectoryClient, ControlPlaneClient, HttpControlPlaneClient,
    LdapDirectoryConfig,
};
use adss_contract::{
    AgentConfirmRequest, AgentSyncRequest, CredentialBatch, DirectoryBatch, SyncChannel,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn http_control_plane_client_builds_endpoints_from_trailing_slash_base_url() {
    let client = HttpControlPlaneClient::new("http://127.0.0.1:8080/", "agent-a-key");

    assert_eq!(
        client.endpoint("/api/agent/sync"),
        "http://127.0.0.1:8080/api/agent/sync"
    );
    assert_eq!(
        client.endpoint("api/agent/confirm"),
        "http://127.0.0.1:8080/api/agent/confirm"
    );
}

#[tokio::test]
async fn http_control_plane_client_posts_sync_request_with_agent_key() {
    let server = OneShotHttpServer::start(
        r#"{"directory":{"server_revision":0,"batch_revision":0,"organizational_units":[],"users":[],"groups":[],"has_more":false},"credentials":{"server_revision":0,"batch_revision":0,"credentials":[],"has_more":false},"directory_config":{"domain_id":"domain-a","mirror_root_dn":"OU=Mirror,DC=example,DC=com","quarantine_ou_dn":"OU=Quarantine,DC=example,DC=com","upn_suffix":"example.com","employee_id_attribute":"employeeID","managed_group_id_attribute":"adminDescription"}}"#,
    )
    .await;
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let response = client
        .sync(AgentSyncRequest {
            domain_id: "domain-a".to_string(),
            applied_directory_revision: 3,
            applied_credential_revision: 7,
            rebuild_directory: true,
            rebuild_credentials: false,
        })
        .await
        .unwrap();

    let request = server.request().await;
    assert!(request.starts_with("POST /api/agent/sync HTTP/1.1"));
    assert!(request.contains("x-adss-agent-key: agent-a-key"));
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
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let result = client.sync(sync_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_sync_bad_json() {
    let server = OneShotHttpServer::start("not-json").await;
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let result = client.sync(sync_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_posts_confirm_request_with_agent_key() {
    let server = OneShotHttpServer::start(r#"{"accepted":true}"#).await;
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let response = client
        .confirm(AgentConfirmRequest {
            domain_id: "domain-a".to_string(),
            channel: SyncChannel::Credential,
            target_revision: 11,
            success: true,
            error_code: None,
        })
        .await
        .unwrap();

    let request = server.request().await;
    assert!(request.starts_with("POST /api/agent/confirm HTTP/1.1"));
    assert!(request.contains("x-adss-agent-key: agent-a-key"));
    assert!(request.contains(r#""channel":"credential""#));
    assert!(request.contains(r#""target_revision":11"#));
    assert!(request.contains(r#""success":true"#));
    assert!(response.accepted);
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_confirm_non_success_status() {
    let server = OneShotHttpServer::start_with_status(401, r#"{"error":"unauthorized"}"#).await;
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let result = client.confirm(confirm_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[tokio::test]
async fn http_control_plane_client_returns_err_for_confirm_bad_json() {
    let server = OneShotHttpServer::start("not-json").await;
    let client = HttpControlPlaneClient::new(server.base_url(), "agent-a-key");

    let result = client.confirm(confirm_request()).await;

    assert!(result.is_err());
    let _ = server.request().await;
}

#[test]
fn agent_process_config_uses_explicit_values_and_defaults() {
    let config = AgentProcessConfig::new(
        "http://127.0.0.1:8080",
        "domain-a",
        "agent-a-key",
        "agent-state.json",
        15,
        true,
        None,
    );

    assert_eq!(config.server_url, "http://127.0.0.1:8080");
    assert_eq!(config.domain_id, "domain-a");
    assert_eq!(config.agent_key, "agent-a-key");
    assert_eq!(config.state_path, "agent-state.json");
    assert_eq!(config.interval_seconds, 15);
    assert!(config.dry_run);
    assert!(config.ldap.is_none());
}

#[test]
fn agent_process_config_rejects_zero_interval_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("ADSS_DOMAIN_ID", "domain-a");
        std::env::set_var("ADSS_AGENT_KEY", "agent-a-key");
        std::env::set_var("ADSS_AGENT_INTERVAL_SECONDS", "0");
        std::env::set_var("ADSS_AGENT_DRY_RUN", "1");
    }

    let error = AgentProcessConfig::from_env().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("ADSS_AGENT_INTERVAL_SECONDS must be greater than 0")
    );

    unsafe {
        std::env::remove_var("ADSS_DOMAIN_ID");
        std::env::remove_var("ADSS_AGENT_KEY");
        std::env::remove_var("ADSS_AGENT_INTERVAL_SECONDS");
        std::env::remove_var("ADSS_AGENT_DRY_RUN");
    }
}

#[test]
fn agent_process_config_requires_ldap_settings_without_dry_run() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_agent_env();
    unsafe {
        std::env::set_var("ADSS_DOMAIN_ID", "domain-a");
        std::env::set_var("ADSS_AGENT_KEY", "agent-a-key");
        std::env::set_var("ADSS_AGENT_DRY_RUN", "0");
    }

    let error = AgentProcessConfig::from_env().unwrap_err();

    assert!(error.to_string().contains("ADSS_LDAP_URL is required"));

    clear_agent_env();
}

#[test]
fn agent_process_config_accepts_ldap_url_without_dry_run() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_agent_env();
    unsafe {
        std::env::set_var("ADSS_DOMAIN_ID", "domain-a");
        std::env::set_var("ADSS_AGENT_KEY", "agent-a-key");
        std::env::set_var("ADSS_AGENT_DRY_RUN", "0");
        std::env::set_var("ADSS_LDAP_URL", "ldap://dc-a.example.com:389");
        std::env::set_var(
            "ADSS_LDAP_BIND_DN",
            "CN=adss-agent,OU=Svc,DC=example,DC=com",
        );
        std::env::set_var("ADSS_LDAP_BIND_PASSWORD", "BindSecret123!");
    }

    let config = AgentProcessConfig::from_env().unwrap();
    let ldap = config.ldap.expect("LDAP settings should be parsed");

    assert_eq!(ldap.url, "ldap://dc-a.example.com:389");

    clear_agent_env();
}

#[test]
fn agent_process_config_rejects_non_ldap_url_without_dry_run() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_agent_env();
    unsafe {
        std::env::set_var("ADSS_DOMAIN_ID", "domain-a");
        std::env::set_var("ADSS_AGENT_KEY", "agent-a-key");
        std::env::set_var("ADSS_AGENT_DRY_RUN", "0");
        std::env::set_var("ADSS_LDAP_URL", "http://dc-a.example.com");
        std::env::set_var(
            "ADSS_LDAP_BIND_DN",
            "CN=adss-agent,OU=Svc,DC=example,DC=com",
        );
        std::env::set_var("ADSS_LDAP_BIND_PASSWORD", "BindSecret123!");
    }

    let error = AgentProcessConfig::from_env().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("ADSS_LDAP_URL must use ldap:// or ldaps://")
    );

    clear_agent_env();
}

#[test]
fn agent_process_config_parses_ldap_settings_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_agent_env();
    unsafe {
        std::env::set_var("ADSS_SERVER_URL", "https://sync.example.com");
        std::env::set_var("ADSS_DOMAIN_ID", "domain-a");
        std::env::set_var("ADSS_AGENT_KEY", "agent-a-key");
        std::env::set_var("ADSS_AGENT_INTERVAL_SECONDS", "30");
        std::env::set_var("ADSS_AGENT_DRY_RUN", "false");
        std::env::set_var("ADSS_LDAP_URL", "ldaps://dc-a.example.com:636");
        std::env::set_var(
            "ADSS_LDAP_BIND_DN",
            "CN=adss-agent,OU=Svc,DC=example,DC=com",
        );
        std::env::set_var("ADSS_LDAP_BIND_PASSWORD", "BindSecret123!");
        std::env::set_var("ADSS_LDAP_ACCEPT_INVALID_CERTS", "true");
    }

    let config = AgentProcessConfig::from_env().unwrap();
    let ldap = config
        .ldap
        .expect("non-dry-run config must include LDAP settings");

    assert!(!config.dry_run);
    assert_eq!(config.state_path, "adss-agent-state.json");
    assert_eq!(ldap.url, "ldaps://dc-a.example.com:636");
    assert_eq!(ldap.bind_dn, "CN=adss-agent,OU=Svc,DC=example,DC=com");
    assert_eq!(ldap.bind_password, "BindSecret123!");
    assert!(ldap.accept_invalid_certs);

    clear_agent_env();
}

#[test]
fn configured_directory_client_selects_ldap_without_dry_run() {
    let config = AgentProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "agent-a-key",
        "agent-state.json",
        30,
        false,
        Some(LdapDirectoryConfig {
            url: "ldaps://dc-a.example.com:636".to_string(),
            bind_dn: "CN=adss-agent,OU=Svc,DC=example,DC=com".to_string(),
            bind_password: "BindSecret123!".to_string(),
            accept_invalid_certs: false,
        }),
    );

    assert!(matches!(
        ConfiguredDirectoryClient::from_process_config(&config).unwrap(),
        ConfiguredDirectoryClient::Ldap(_)
    ));
}

#[test]
fn configured_directory_client_keeps_explicit_dry_run() {
    let config = AgentProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "agent-a-key",
        "agent-state.json",
        30,
        true,
        None,
    );

    assert!(matches!(
        ConfiguredDirectoryClient::from_process_config(&config).unwrap(),
        ConfiguredDirectoryClient::DryRun(_)
    ));
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clear_agent_env() {
    unsafe {
        std::env::remove_var("ADSS_SERVER_URL");
        std::env::remove_var("ADSS_DOMAIN_ID");
        std::env::remove_var("ADSS_AGENT_KEY");
        std::env::remove_var("ADSS_AGENT_INTERVAL_SECONDS");
        std::env::remove_var("ADSS_AGENT_DRY_RUN");
        std::env::remove_var("ADSS_LDAP_URL");
        std::env::remove_var("ADSS_LDAP_BIND_DN");
        std::env::remove_var("ADSS_LDAP_BIND_PASSWORD");
        std::env::remove_var("ADSS_LDAP_ACCEPT_INVALID_CERTS");
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

fn sync_request() -> AgentSyncRequest {
    AgentSyncRequest {
        domain_id: "domain-a".to_string(),
        applied_directory_revision: 3,
        applied_credential_revision: 7,
        rebuild_directory: false,
        rebuild_credentials: false,
    }
}

fn confirm_request() -> AgentConfirmRequest {
    AgentConfirmRequest {
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
        users: Vec::new(),
        groups: Vec::new(),
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

use adss_contract::{
    AgentConfirmRequest, AgentSyncRequest, AgentSyncResponse, MvpPasswordChangeRequest,
    MvpUserStatus, SyncChannel, UserLoginRequest,
};
use adss_persistence::{DomainRecord, MvpRepository, UserCredentialInput, UserDirectoryPatch};
use adss_server::{AppState, build_router};
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const AGENT_KEY: &str = "test-agent-key";
const TEST_ENVELOPE_KEY: &str = "test-envelope-key";

struct TestApp {
    app: axum::Router,
    repository: MvpRepository,
}

#[tokio::test]
async fn user_directory_update_returns_only_changed_user() {
    let TestApp { app, .. } = test_app().await;

    patch_user(&app, "1001", "first@example.com").await;
    confirm(&app, SyncChannel::Directory, 1, true).await;
    patch_user(&app, "1002", "second@example.com").await;

    let response = agent_sync(&app, 1, 0, false, false).await;

    assert_eq!(response.directory.server_revision, 2);
    assert_eq!(response.directory.batch_revision, 2);
    assert_eq!(response.directory.users.len(), 1);
    assert_eq!(response.directory.users[0].employee_id, "1002");
    assert_eq!(
        response.directory.users[0].email.as_deref(),
        Some("second@example.com")
    );
    assert!(response.directory.organizational_units.is_empty());
    assert!(response.directory.groups.is_empty());
    assert!(response.credentials.credentials.is_empty());
}

#[tokio::test]
async fn directory_confirm_advances_only_directory_channel() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 1, true).await;
    let response = agent_sync(&app, 1, 0, false, false).await;

    assert!(response.directory.users.is_empty());
    assert_eq!(response.directory.server_revision, 1);
    assert_eq!(response.credentials.credentials.len(), 1);
    assert_eq!(
        response.credentials.credentials[0].plaintext_password,
        "OldPass123!"
    );
}

#[tokio::test]
async fn failed_confirm_does_not_advance_revision() {
    let TestApp { app, .. } = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 1, false).await;
    let response = agent_sync(&app, 0, 0, false, false).await;

    assert_eq!(response.directory.server_revision, 1);
    assert_eq!(response.directory.batch_revision, 1);
    assert_eq!(response.directory.users.len(), 1);
}

#[tokio::test]
async fn login_and_password_change_returns_plaintext_credential_to_agent_sync() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    login(&app, "1001", "OldPass123!").await;
    change_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let response = agent_sync(&app, 0, 0, false, false).await;

    assert!(response.directory.users.is_empty());
    assert_eq!(response.credentials.server_revision, 2);
    assert_eq!(response.credentials.credentials.len(), 1);
    assert_eq!(response.credentials.credentials[0].employee_id, "1001");
    assert_eq!(
        response.credentials.credentials[0].plaintext_password,
        "NewPass123!"
    );
}

#[tokio::test]
async fn login_rejects_same_length_wrong_password() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    let response = app
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                employee_id: "1001".to_string(),
                password: "BadPass123!".to_string(),
            },
        ))
        .await
        .expect("login response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn password_change_rejects_same_length_wrong_current_password() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    let response = app
        .clone()
        .oneshot(json_request(
            "/api/users/1001/password",
            &MvpPasswordChangeRequest {
                current_password: "BadPass123!".to_string(),
                new_password: "NewPass123!".to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let sync = agent_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.credentials.credentials.len(), 1);
    assert_eq!(
        sync.credentials.credentials[0].plaintext_password,
        "OldPass123!"
    );
}

#[tokio::test]
async fn sync_requires_matching_agent_key() {
    let TestApp { app, .. } = test_app().await;

    let response = request_with_agent_key(&app, "wrong-key").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn confirm_requires_matching_agent_key_and_does_not_advance_revision() {
    let TestApp { app, .. } = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    let response = app
        .clone()
        .oneshot(agent_json_request(
            "/api/agent/confirm",
            "wrong-key",
            &AgentConfirmRequest {
                domain_id: "domain-a".to_string(),
                channel: SyncChannel::Directory,
                target_revision: 1,
                success: true,
                error_code: None,
            },
        ))
        .await
        .expect("confirm response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let sync = agent_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.directory.users.len(), 1);
    assert_eq!(sync.directory.batch_revision, 1);
}

#[tokio::test]
async fn stored_agent_key_hash_is_not_plaintext_agent_key() {
    let repository = MvpRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository
        .seed_domain(DomainRecord {
            agent_key_hash: AGENT_KEY.to_string(),
            ..domain(true)
        })
        .await
        .unwrap();
    let app = build_router(AppState::new_for_tests(repository, TEST_ENVELOPE_KEY));

    let response = request_with_agent_key(&app, AGENT_KEY).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn password_change_stores_ciphertext_without_plaintext_password() {
    let TestApp { app, repository } = test_app_with_seeded_credential("OldPass123!").await;

    change_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let credential = repository
        .get_credential_record("1001")
        .await
        .unwrap()
        .unwrap();

    assert!(!credential.password_ciphertext.contains("NewPass123!"));
}

#[tokio::test]
async fn disabled_domain_with_wrong_key_returns_unauthorized() {
    let TestApp { app, .. } = test_app_with_domain_enabled(false).await;

    let response = request_with_agent_key(&app, "wrong-key").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabled_domain_with_correct_key_returns_forbidden() {
    let TestApp { app, .. } = test_app_with_domain_enabled(false).await;

    let response = request_with_agent_key(&app, AGENT_KEY).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn password_error_response_does_not_include_submitted_passwords() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let response = app
        .oneshot(json_request(
            "/api/users/1001/password",
            &MvpPasswordChangeRequest {
                current_password: "BadPass123!".to_string(),
                new_password: "NewPass123!".to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(!text.contains("BadPass123!"));
    assert!(!text.contains("NewPass123!"));
}

async fn test_app() -> TestApp {
    test_app_with_domain_enabled(true).await
}

async fn test_app_with_domain_enabled(enabled: bool) -> TestApp {
    let repository = MvpRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(enabled)).await.unwrap();
    let app = build_router(AppState::new_for_tests(
        repository.clone(),
        TEST_ENVELOPE_KEY,
    ));
    TestApp { app, repository }
}

async fn test_app_with_seeded_credential(password: &str) -> TestApp {
    let repository = MvpRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: seal_password_for_storage(password),
            password_verifier: password_verifier(password),
        })
        .await
        .unwrap();
    let app = build_router(AppState::new_for_tests(
        repository.clone(),
        TEST_ENVELOPE_KEY,
    ));
    TestApp { app, repository }
}

async fn patch_user(app: &axum::Router, employee_id: &str, email: &str) {
    let request = json!({
        "username": employee_id,
        "display_name": employee_id,
        "email": email,
        "mobile": null,
        "telephone": null,
        "organizational_unit_id": "ou-root",
        "status": "active"
    });
    let response = app
        .clone()
        .oneshot(method_json_request(
            Method::PATCH,
            &format!("/api/users/{employee_id}"),
            &request,
        ))
        .await
        .expect("patch response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn login(app: &axum::Router, employee_id: &str, password: &str) {
    let response = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                employee_id: employee_id.to_string(),
                password: password.to_string(),
            },
        ))
        .await
        .expect("login response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn change_password(
    app: &axum::Router,
    employee_id: &str,
    current_password: &str,
    new_password: &str,
) {
    let response = app
        .clone()
        .oneshot(json_request(
            &format!("/api/users/{employee_id}/password"),
            &MvpPasswordChangeRequest {
                current_password: current_password.to_string(),
                new_password: new_password.to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn agent_sync(
    app: &axum::Router,
    applied_directory_revision: u64,
    applied_credential_revision: u64,
    rebuild_directory: bool,
    rebuild_credentials: bool,
) -> AgentSyncResponse {
    let response = app
        .clone()
        .oneshot(agent_json_request(
            "/api/agent/sync",
            AGENT_KEY,
            &AgentSyncRequest {
                domain_id: "domain-a".to_string(),
                applied_directory_revision,
                applied_credential_revision,
                rebuild_directory,
                rebuild_credentials,
            },
        ))
        .await
        .expect("sync response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn confirm(app: &axum::Router, channel: SyncChannel, target_revision: u64, success: bool) {
    let response = app
        .clone()
        .oneshot(agent_json_request(
            "/api/agent/confirm",
            AGENT_KEY,
            &AgentConfirmRequest {
                domain_id: "domain-a".to_string(),
                channel,
                target_revision,
                success,
                error_code: (!success).then(|| "directory_failed".to_string()),
            },
        ))
        .await
        .expect("confirm response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn request_with_agent_key(app: &axum::Router, agent_key: &str) -> Response<Body> {
    app.clone()
        .oneshot(agent_json_request(
            "/api/agent/sync",
            agent_key,
            &AgentSyncRequest {
                domain_id: "domain-a".to_string(),
                applied_directory_revision: 0,
                applied_credential_revision: 0,
                rebuild_directory: false,
                rebuild_credentials: false,
            },
        ))
        .await
        .expect("sync response")
}

fn domain(enabled: bool) -> DomainRecord {
    DomainRecord {
        id: "domain-a".to_string(),
        name: "Domain A".to_string(),
        enabled,
        mirror_root_dn: "OU=Mirror,DC=a,DC=example,DC=com".to_string(),
        quarantine_ou_dn: "OU=Quarantine,DC=a,DC=example,DC=com".to_string(),
        upn_suffix: "a.example.com".to_string(),
        employee_id_attribute: "employeeID".to_string(),
        agent_key_hash: agent_key_hash(AGENT_KEY),
        applied_directory_revision: 0,
        applied_credential_revision: 0,
    }
}

fn json_request<T: serde::Serialize>(uri: &str, value: &T) -> Request<Body> {
    method_json_request(Method::POST, uri, value)
}

fn agent_json_request<T: serde::Serialize>(uri: &str, agent_key: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-adss-agent-key", agent_key)
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn method_json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:mvp-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("verifier:v1:{}", hex::encode(hasher.finalize()))
}

fn agent_key_hash(agent_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(agent_key.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn seal_password_for_storage(password: &str) -> String {
    format!(
        "mvp-envelope:v1:{}",
        hex::encode(xor_with_password_stream(password.as_bytes()))
    )
}

fn xor_with_password_stream(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut counter = 0_u64;

    while output.len() < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(b"adss:mvp-password-envelope:v1");
        hasher.update(TEST_ENVELOPE_KEY.as_bytes());
        hasher.update(counter.to_be_bytes());
        let block = hasher.finalize();

        for byte in block {
            if output.len() == input.len() {
                break;
            }
            output.push(input[output.len()] ^ byte);
        }

        counter += 1;
    }

    output
}

#[allow(dead_code)]
fn user_patch(employee_id: &str, email: &str) -> UserDirectoryPatch {
    UserDirectoryPatch {
        employee_id: employee_id.to_string(),
        username: employee_id.to_string(),
        display_name: employee_id.to_string(),
        email: Some(email.to_string()),
        mobile: None,
        telephone: None,
        organizational_unit_id: "ou-root".to_string(),
        status: MvpUserStatus::Active,
    }
}

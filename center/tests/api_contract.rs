use adss_center::{AppState, build_router};
use adss_protocol::{
    ConnectorConfirmRequest, ConnectorSyncRequest, ConnectorSyncResponse, PasswordChangeRequest,
    SyncChannel, UserLoginRequest, UserStatus,
};
use adss_store::{DomainRecord, Repository, UserCredentialInput, UserDirectoryPatch};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const CONNECTOR_KEY: &str = "test-connector-key";
const MANAGEMENT_TOKEN: &str = "test-management-token";
const TEST_ENCRYPTION_KEY: &str = "test-password-encryption-key";
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TestApp {
    app: axum::Router,
    repository: Repository,
}

#[tokio::test]
async fn user_directory_update_returns_changed_user_with_ou_context() {
    let TestApp { app, repository } = test_app().await;

    repository
        .upsert_directory(
            vec![adss_protocol::OrganizationalUnit {
                id: "ou-root".to_string(),
                name: "Root".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    confirm(&app, SyncChannel::Directory, 1, true).await;
    patch_user(&app, "1001", "first@example.com").await;
    confirm(&app, SyncChannel::Directory, 2, true).await;
    patch_user(&app, "1002", "second@example.com").await;

    let response = connector_sync(&app, 2, 0, false, false).await;

    assert_eq!(response.directory.server_revision, 3);
    assert_eq!(response.directory.batch_revision, 3);
    assert_eq!(response.directory.users.len(), 1);
    assert_eq!(response.directory.users[0].employee_id, "1002");
    assert_eq!(
        response.directory.users[0].email.as_deref(),
        Some("second@example.com")
    );
    assert_eq!(response.directory.organizational_units.len(), 1);
    assert_eq!(response.directory.organizational_units[0].id, "ou-root");
    assert!(response.directory.groups.is_empty());
    assert!(response.credentials.credentials.is_empty());
}

#[tokio::test]
async fn admin_routes_require_management_token_not_user_token() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let user_token = login_token(&app, "1001", "OldPass123!").await;

    let missing = app
        .clone()
        .oneshot(method_json_request(
            Method::POST,
            "/api/admin/users",
            &json!({
                "employee_id": "1002",
                "username": "lisi",
                "display_name": "李四",
                "email": null,
                "mobile": null,
                "telephone": null,
                "organizational_unit_id": "ou-root",
                "status": "active",
                "initial_password": "InitialPass123!"
            }),
        ))
        .await
        .expect("admin response");
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let user_token_response = app
        .clone()
        .oneshot(auth_json_request(
            Method::POST,
            "/api/admin/users",
            &user_token,
            &json!({
                "employee_id": "1002",
                "username": "lisi",
                "display_name": "李四",
                "email": null,
                "mobile": null,
                "telephone": null,
                "organizational_unit_id": "ou-root",
                "status": "active",
                "initial_password": "InitialPass123!"
            }),
        ))
        .await
        .expect("admin response");
    assert_eq!(user_token_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_routes_do_not_expose_connector_key_rotation() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/admin/domains/domain-a/connector-key")
                .header("authorization", format!("Bearer {MANAGEMENT_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("connector key response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_routes_manage_domains_directory_users_groups_and_sync_status() {
    let TestApp { app, repository } = test_app().await;

    let created_domain = admin_json(
        &app,
        Method::POST,
        "/api/admin/domains",
        &json!({
            "id": "domain-b",
            "name": "Domain B",
            "enabled": true,
            "mirror_root_dn": "OU=Mirror,DC=b,DC=example,DC=com",
            "quarantine_ou_dn": "OU=Quarantine,DC=b,DC=example,DC=com",
            "upn_suffix": "b.example.com",
            "employee_id_attribute": "employeeID",
            "managed_group_id_attribute": "adminDescription",
            "connector_key": "domain-b-connector-key"
        }),
    )
    .await;
    assert_eq!(created_domain["id"], "domain-b");
    assert!(created_domain.get("connector_key").is_none());
    assert!(created_domain.get("connector_key_hash").is_none());

    let domains = admin_empty(&app, Method::GET, "/api/admin/domains").await;
    assert_eq!(domains["domains"].as_array().unwrap().len(), 2);

    let patched_domain = admin_json(
        &app,
        Method::PATCH,
        "/api/admin/domains/domain-b",
        &json!({
            "name": "Domain B Updated",
            "enabled": false,
            "mirror_root_dn": "OU=Mirror2,DC=b,DC=example,DC=com",
            "quarantine_ou_dn": "OU=Quarantine2,DC=b,DC=example,DC=com",
            "upn_suffix": "b2.example.com",
            "employee_id_attribute": "employeeNumber",
            "managed_group_id_attribute": "extensionAttribute10",
            "connector_key_hash": "must-not-be-accepted"
        }),
    )
    .await;
    assert_eq!(patched_domain["name"], "Domain B Updated");
    let stored_domain = repository.get_domain("domain-b").await.unwrap().unwrap();
    assert_eq!(
        stored_domain.connector_key_hash,
        connector_key_hash("domain-b-connector-key")
    );

    let root_ou = admin_json(
        &app,
        Method::POST,
        "/api/admin/ous",
        &json!({
            "id": "ou-root",
            "name": "Root",
            "parent_id": null
        }),
    )
    .await;
    assert_eq!(root_ou["directory_revision"], 1);
    let child_ou = admin_json(
        &app,
        Method::POST,
        "/api/admin/ous",
        &json!({
            "id": "ou-child",
            "name": "Child",
            "parent_id": "ou-root"
        }),
    )
    .await;
    assert_eq!(child_ou["directory_revision"], 2);
    let renamed_ou = admin_json(
        &app,
        Method::PATCH,
        "/api/admin/ous/ou-child",
        &json!({
            "name": "Child Updated",
            "parent_id": "ou-root"
        }),
    )
    .await;
    assert_eq!(renamed_ou["organizational_unit"]["name"], "Child Updated");
    let ou_tree = admin_empty(&app, Method::GET, "/api/admin/ous/tree").await;
    assert_eq!(ou_tree["organizational_units"].as_array().unwrap().len(), 2);

    let created_user = admin_json(
        &app,
        Method::POST,
        "/api/admin/users",
        &json!({
            "employee_id": "1001",
            "username": "zhangsan",
            "display_name": "张三",
            "email": "zhangsan@example.com",
            "mobile": "13800000000",
            "telephone": "021-10000000",
            "organizational_unit_id": "ou-child",
            "status": "active",
            "initial_password": "InitialPass123!"
        }),
    )
    .await;
    assert_eq!(created_user["directory_revision"], 4);
    assert_eq!(created_user["credential_revision"], 1);
    assert!(created_user.get("initial_password").is_none());

    let users = admin_empty(
        &app,
        Method::GET,
        "/api/admin/users?organizational_unit_id=ou-child&status=active",
    )
    .await;
    assert_eq!(users["users"].as_array().unwrap().len(), 1);
    assert_eq!(users["users"][0]["employee_id"], "1001");

    let patched_user = admin_json(
        &app,
        Method::PATCH,
        "/api/admin/users/1001",
        &json!({
            "username": "zhangsan",
            "display_name": "张三 Updated",
            "email": "zhangsan.updated@example.com",
            "mobile": "13900000000",
            "telephone": "021-20000000",
            "organizational_unit_id": "ou-root",
            "status": "active"
        }),
    )
    .await;
    assert_eq!(patched_user["user"]["display_name"], "张三 Updated");
    admin_empty(&app, Method::POST, "/api/admin/users/1001/disable").await;
    let disabled = admin_empty(&app, Method::GET, "/api/admin/users/1001").await;
    assert_eq!(disabled["status"], "disabled");
    admin_empty(&app, Method::POST, "/api/admin/users/1001/enable").await;
    let reset = admin_json(
        &app,
        Method::POST,
        "/api/admin/users/1001/password-reset",
        &json!({ "new_password": "ResetPass123!" }),
    )
    .await;
    assert_eq!(reset["credential_revision"], 2);
    login(&app, "1001", "ResetPass123!").await;

    let created_group = admin_json(
        &app,
        Method::POST,
        "/api/admin/groups",
        &json!({
            "id": "group-rd",
            "name": "研发组"
        }),
    )
    .await;
    assert_eq!(
        created_group["group"]["member_employee_ids"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let members = admin_json(
        &app,
        Method::PUT,
        "/api/admin/groups/group-rd/members",
        &json!({ "member_employee_ids": ["1001"] }),
    )
    .await;
    assert_eq!(members["group"]["member_employee_ids"][0], "1001");
    let renamed_group = admin_json(
        &app,
        Method::PATCH,
        "/api/admin/groups/group-rd",
        &json!({ "name": "研发组 Updated" }),
    )
    .await;
    assert_eq!(renamed_group["group"]["name"], "研发组 Updated");
    let group = admin_empty(&app, Method::GET, "/api/admin/groups/group-rd").await;
    assert_eq!(group["member_employee_ids"][0], "1001");
    let groups = admin_empty(&app, Method::GET, "/api/admin/groups").await;
    assert_eq!(groups["groups"].as_array().unwrap().len(), 1);

    let status = admin_empty(&app, Method::GET, "/api/admin/sync/domains").await;
    let domain_a = status["domains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|domain| domain["domain_id"] == "domain-a")
        .unwrap();
    assert_eq!(domain_a["applied_directory_revision"], 0);
    assert_eq!(domain_a["applied_credential_revision"], 0);
    assert_eq!(domain_a["directory_lag"], 10);
    assert_eq!(domain_a["credential_lag"], 2);
}

#[tokio::test]
async fn directory_confirm_advances_only_directory_channel() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 2, true).await;
    let response = connector_sync(&app, 2, 0, false, false).await;

    assert!(response.directory.users.is_empty());
    assert_eq!(response.directory.server_revision, 2);
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
    let response = connector_sync(&app, 0, 0, false, false).await;

    assert_eq!(response.directory.server_revision, 1);
    assert_eq!(response.directory.batch_revision, 1);
    assert_eq!(response.directory.users.len(), 1);
}

#[tokio::test]
async fn login_and_password_change_returns_plaintext_credential_to_connector_sync() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    login(&app, "1001", "OldPass123!").await;
    confirm(&app, SyncChannel::Directory, 1, true).await;
    change_password_with_current_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let response = connector_sync(&app, 1, 0, false, false).await;

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
async fn login_returns_user_access_token_for_self_service() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    let token = login_token(&app, "1001", "OldPass123!").await;

    assert!(token.starts_with("adss-user-session:v1:1001:"));
}

#[tokio::test]
async fn me_requires_user_access_token_and_returns_own_profile() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("me response");
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    let token = login_token(&app, "1001", "OldPass123!").await;

    let response = app
        .clone()
        .oneshot(auth_empty_request(Method::GET, "/api/me", &token))
        .await
        .expect("me response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let profile: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(profile["employee_id"], "1001");
    assert_eq!(profile["username"], "1001");
    assert_eq!(profile["display_name"], "User 1001");
    assert_eq!(profile["email"], "old@example.com");
    assert_eq!(profile["mobile"], "13800000000");
    assert_eq!(profile["telephone"], "021-10000000");
    assert_eq!(profile["organizational_unit_id"], "ou-root");
    assert_eq!(profile["status"], "active");
}

#[tokio::test]
async fn user_self_contact_patch_updates_only_contact_fields() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let token = login_token(&app, "1001", "OldPass123!").await;
    confirm(&app, SyncChannel::Directory, 1, true).await;
    let request = json!({
        "email": "new@example.com",
        "mobile": "13900000000",
        "telephone": "021-20000000",
        "username": "must-not-change",
        "display_name": "Must Not Change",
        "organizational_unit_id": "ou-other",
        "status": "disabled"
    });

    let response = app
        .clone()
        .oneshot(auth_json_request(
            Method::PATCH,
            "/api/me/contact",
            &token,
            &request,
        ))
        .await
        .expect("contact response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["directory_revision"], 2);
    assert_eq!(payload["profile"]["email"], "new@example.com");
    assert_eq!(payload["profile"]["mobile"], "13900000000");
    assert_eq!(payload["profile"]["telephone"], "021-20000000");
    assert_eq!(payload["profile"]["username"], "1001");
    assert_eq!(payload["profile"]["display_name"], "User 1001");
    assert_eq!(payload["profile"]["organizational_unit_id"], "ou-root");
    assert_eq!(payload["profile"]["status"], "active");

    let sync = connector_sync(&app, 1, 0, false, false).await;
    assert_eq!(sync.directory.users.len(), 1);
    assert_eq!(sync.directory.users[0].username, "1001");
    assert_eq!(sync.directory.users[0].display_name, "User 1001");
    assert_eq!(
        sync.directory.users[0].email.as_deref(),
        Some("new@example.com")
    );
    assert_eq!(
        sync.directory.users[0].mobile.as_deref(),
        Some("13900000000")
    );
    assert_eq!(
        sync.directory.users[0].telephone.as_deref(),
        Some("021-20000000")
    );
    assert_eq!(sync.directory.users[0].organizational_unit_id, "ou-root");
    assert_eq!(sync.directory.users[0].status, UserStatus::Active);
}

#[tokio::test]
async fn user_self_password_change_uses_access_token_identity() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let token = login_token(&app, "1001", "OldPass123!").await;

    let response = app
        .clone()
        .oneshot(auth_json_request(
            Method::POST,
            "/api/me/password",
            &token,
            &PasswordChangeRequest {
                current_password: "OldPass123!".to_string(),
                new_password: "NewPass123!".to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::OK);
    login(&app, "1001", "NewPass123!").await;
    let sync = connector_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.credentials.credentials.len(), 1);
    assert_eq!(
        sync.credentials.credentials[0].plaintext_password,
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
        .oneshot(auth_json_request(
            Method::POST,
            "/api/me/password",
            &login_token(&app, "1001", "OldPass123!").await,
            &PasswordChangeRequest {
                current_password: "BadPass123!".to_string(),
                new_password: "NewPass123!".to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let sync = connector_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.credentials.credentials.len(), 1);
    assert_eq!(
        sync.credentials.credentials[0].plaintext_password,
        "OldPass123!"
    );
}

#[tokio::test]
async fn sync_requires_matching_connector_key() {
    let TestApp { app, .. } = test_app().await;

    let response = request_with_connector_key(&app, "wrong-key").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn confirm_requires_matching_connector_key_and_does_not_advance_revision() {
    let TestApp { app, .. } = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    let response = app
        .clone()
        .oneshot(connector_json_request(
            "/api/connector/confirm",
            "wrong-key",
            &ConnectorConfirmRequest {
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

    let sync = connector_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.directory.users.len(), 1);
    assert_eq!(sync.directory.batch_revision, 1);
}

#[tokio::test]
async fn stored_connector_key_hash_is_not_plaintext_connector_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository
        .seed_domain(DomainRecord {
            connector_key_hash: CONNECTOR_KEY.to_string(),
            ..domain(true)
        })
        .await
        .unwrap();
    let app = build_router(AppState::new_for_tests(repository, TEST_ENCRYPTION_KEY));

    let response = request_with_connector_key(&app, CONNECTOR_KEY).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn password_change_stores_ciphertext_without_plaintext_password() {
    let TestApp { app, repository } = test_app_with_seeded_credential("OldPass123!").await;

    change_password_with_current_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let credential = repository
        .get_credential_record("1001")
        .await
        .unwrap()
        .unwrap();

    assert!(!credential.password_ciphertext.contains("NewPass123!"));
    assert!(credential.password_ciphertext.starts_with("pw:v1:"));
}

#[tokio::test]
async fn built_in_password_encryption_uses_randomized_ciphertext() {
    let TestApp { app, repository } = test_app_with_seeded_credential("OldPass123!").await;

    change_password_with_current_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let first_ciphertext = repository
        .get_credential_record("1001")
        .await
        .unwrap()
        .unwrap()
        .password_ciphertext;

    change_password_with_current_password(&app, "1001", "NewPass123!", "OldPass123!").await;
    change_password_with_current_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let second_ciphertext = repository
        .get_credential_record("1001")
        .await
        .unwrap()
        .unwrap()
        .password_ciphertext;

    assert_ne!(first_ciphertext, second_ciphertext);
    let sync = connector_sync(&app, 0, 0, false, false).await;
    assert_eq!(
        sync.credentials.credentials[0].plaintext_password,
        "NewPass123!"
    );
}

#[tokio::test]
async fn disabled_domain_with_wrong_key_returns_unauthorized() {
    let TestApp { app, .. } = test_app_with_domain_enabled(false).await;

    let response = request_with_connector_key(&app, "wrong-key").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabled_domain_with_correct_key_returns_forbidden() {
    let TestApp { app, .. } = test_app_with_domain_enabled(false).await;

    let response = request_with_connector_key(&app, CONNECTOR_KEY).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn password_error_response_does_not_include_submitted_passwords() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let response = app
        .clone()
        .oneshot(auth_json_request(
            Method::POST,
            "/api/me/password",
            &login_token(&app, "1001", "OldPass123!").await,
            &PasswordChangeRequest {
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

#[tokio::test]
async fn app_state_from_env_requires_password_encryption_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing password encryption key must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSS_PASSWORD_ENCRYPTION_KEY is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_accepts_password_encryption_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    unsafe {
        std::env::set_var("ADSS_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSS_PASSWORD_HASH_PROVIDER", "argon2id");
        std::env::set_var("ADSS_USER_SESSION_KEY", "test-user-session-key");
        std::env::set_var("ADSS_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
    }

    AppState::from_env(repository).unwrap();

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_password_hash_provider() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    unsafe {
        std::env::set_var("ADSS_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing password hash provider must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSS_PASSWORD_HASH_PROVIDER is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_user_session_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    unsafe {
        std::env::set_var("ADSS_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSS_PASSWORD_HASH_PROVIDER", "argon2id");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing user session key must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSS_USER_SESSION_KEY is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_management_token() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    unsafe {
        std::env::set_var("ADSS_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSS_PASSWORD_HASH_PROVIDER", "argon2id");
        std::env::set_var("ADSS_USER_SESSION_KEY", "test-user-session-key");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing management token must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSS_MANAGEMENT_TOKEN is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_uses_argon2id_password_hash_provider_for_login_and_change() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: seal_password_for_storage("OldPass123!"),
            password_verifier: argon2id_password_verifier("OldPass123!"),
        })
        .await
        .unwrap();

    let state = {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_server_env();
        unsafe {
            std::env::set_var("ADSS_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
            std::env::set_var("ADSS_PASSWORD_HASH_PROVIDER", "argon2id");
            std::env::set_var("ADSS_USER_SESSION_KEY", "test-user-session-key");
            std::env::set_var("ADSS_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
        }
        let state = AppState::from_env(repository.clone()).unwrap();
        clear_server_env();
        state
    };
    let app = build_router(state);

    login(&app, "1001", "OldPass123!").await;
    change_password_with_current_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    login(&app, "1001", "NewPass123!").await;

    let credential = repository
        .get_credential_record("1001")
        .await
        .unwrap()
        .unwrap();
    assert!(credential.password_verifier.starts_with("$argon2id$"));
    assert!(!credential.password_verifier.contains("NewPass123!"));
    assert_ne!(
        credential.password_verifier,
        password_verifier("NewPass123!")
    );
}

async fn test_app() -> TestApp {
    test_app_with_domain_enabled(true).await
}

async fn test_app_with_domain_enabled(enabled: bool) -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(enabled)).await.unwrap();
    let app = build_router(AppState::new_for_tests(
        repository.clone(),
        TEST_ENCRYPTION_KEY,
    ));
    TestApp { app, repository }
}

async fn test_app_with_seeded_credential(password: &str) -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    seed_user(&repository, "1001", "old@example.com")
        .await
        .unwrap();
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
        TEST_ENCRYPTION_KEY,
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
        .oneshot(admin_json_request(
            Method::PATCH,
            &format!("/api/admin/users/{employee_id}"),
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

async fn login_token(app: &axum::Router, employee_id: &str, password: &str) -> String {
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    payload["access_token"].as_str().unwrap().to_string()
}

async fn admin_json(
    app: &axum::Router,
    method: Method,
    uri: &str,
    value: &serde_json::Value,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(admin_json_request(method, uri, value))
        .await
        .expect("admin response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn admin_empty(app: &axum::Router, method: Method, uri: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {MANAGEMENT_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("admin response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn admin_json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {MANAGEMENT_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

async fn change_password_with_current_password(
    app: &axum::Router,
    employee_id: &str,
    current_password: &str,
    new_password: &str,
) {
    let token = login_token(app, employee_id, current_password).await;
    let response = app
        .clone()
        .oneshot(auth_json_request(
            Method::POST,
            "/api/me/password",
            &token,
            &PasswordChangeRequest {
                current_password: current_password.to_string(),
                new_password: new_password.to_string(),
            },
        ))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn connector_sync(
    app: &axum::Router,
    applied_directory_revision: u64,
    applied_credential_revision: u64,
    rebuild_directory: bool,
    rebuild_credentials: bool,
) -> ConnectorSyncResponse {
    let response = app
        .clone()
        .oneshot(connector_json_request(
            "/api/connector/sync",
            CONNECTOR_KEY,
            &ConnectorSyncRequest {
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
        .oneshot(connector_json_request(
            "/api/connector/confirm",
            CONNECTOR_KEY,
            &ConnectorConfirmRequest {
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

async fn request_with_connector_key(app: &axum::Router, connector_key: &str) -> Response<Body> {
    app.clone()
        .oneshot(connector_json_request(
            "/api/connector/sync",
            connector_key,
            &ConnectorSyncRequest {
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
        managed_group_id_attribute: "adminDescription".to_string(),
        connector_key_hash: connector_key_hash(CONNECTOR_KEY),
        applied_directory_revision: 0,
        applied_credential_revision: 0,
    }
}

fn json_request<T: serde::Serialize>(uri: &str, value: &T) -> Request<Body> {
    method_json_request(Method::POST, uri, value)
}

fn connector_json_request<T: serde::Serialize>(
    uri: &str,
    connector_key: &str,
    value: &T,
) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-adss-connector-key", connector_key)
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn auth_empty_request(method: Method, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn auth_json_request<T: serde::Serialize>(
    method: Method,
    uri: &str,
    token: &str,
    value: &T,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
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

async fn seed_user(repository: &Repository, employee_id: &str, email: &str) -> anyhow::Result<u64> {
    repository
        .upsert_directory(
            vec![adss_protocol::OrganizationalUnit {
                id: "ou-root".to_string(),
                name: "Root".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                employee_id: employee_id.to_string(),
                username: employee_id.to_string(),
                display_name: format!("User {employee_id}"),
                email: Some(email.to_string()),
                mobile: Some("13800000000".to_string()),
                telephone: Some("021-10000000".to_string()),
                organizational_unit_id: "ou-root".to_string(),
                status: UserStatus::Active,
            }],
            Vec::new(),
        )
        .await
}

fn password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:test-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("test-verifier:v1:{}", hex::encode(hasher.finalize()))
}

fn argon2id_password_verifier(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn connector_key_hash(connector_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(connector_key.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn seal_password_for_storage(password: &str) -> String {
    let cipher = XChaCha20Poly1305::new_from_slice(&test_password_encryption_key()).unwrap();
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, password.as_bytes()).unwrap();
    format!("pw:v1:{}:{}", hex::encode(nonce), hex::encode(ciphertext))
}

fn test_password_encryption_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:password-encryption:v1");
    hasher.update(TEST_ENCRYPTION_KEY.as_bytes());
    hasher.finalize().into()
}

fn clear_server_env() {
    unsafe {
        std::env::remove_var("ADSS_PASSWORD_ENCRYPTION_KEY");
        std::env::remove_var("ADSS_PASSWORD_HASH_PROVIDER");
        std::env::remove_var("ADSS_USER_SESSION_KEY");
        std::env::remove_var("ADSS_USER_SESSION_TTL_SECONDS");
        std::env::remove_var("ADSS_MANAGEMENT_TOKEN");
    }
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
        status: UserStatus::Active,
    }
}

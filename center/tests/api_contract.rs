use adscope_center::{AppState, build_router, build_router_with_web_root};
use adscope_protocol::{
    ConnectorConfirmRequest, ConnectorSyncRequest, ConnectorSyncResponse, PasswordChangeRequest,
    SyncChannel, UserLoginRequest, UserStatus,
};
use adscope_store::{DomainRecord, Repository, UserCredentialInput, UserDirectoryPatch};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const CONNECTOR_KEY: &str = "test-connector-key";
const MANAGEMENT_TOKEN: &str = "test-management-token";
const MANAGEMENT_CSRF_TOKEN: &str = "test-management-csrf-token";
const TEST_ENCRYPTION_KEY: &str = "test-password-encryption-key";
const TEST_OIDC_ISSUER: &str = "https://center.example.test";
const TEST_OIDC_PRIVATE_KEY: &[u8] = include_bytes!("fixtures/oidc-private-key.pem");
const TEST_OIDC_PRIVATE_KEY_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/oidc-private-key.pem"
);
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TestApp {
    app: axum::Router,
    repository: Repository,
}

#[tokio::test]
async fn health_reports_database_readiness() {
    let ready_repository = Repository::connect("sqlite::memory:").await.unwrap();
    ready_repository.initialize_schema().await.unwrap();
    let ready_app = build_router(AppState::new_for_tests(
        ready_repository,
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    ));

    let ready = ready_app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("health response");
    assert_eq!(ready.status(), StatusCode::OK);
    let ready_body = ready.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(ready_body.as_ref(), br#"{"status":"ok"}"#);

    let unavailable_app = build_router(AppState::new_for_tests(
        Repository::from_connection(Default::default()),
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    ));
    let unavailable = unavailable_app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("health response");
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    let unavailable_body = unavailable.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(unavailable_body.as_ref(), br#"{"status":"unavailable"}"#);
}

#[tokio::test]
async fn user_directory_update_returns_changed_user_with_ou_context() {
    let TestApp { app, repository } = test_app().await;

    repository
        .upsert_directory(
            vec![adscope_protocol::OrganizationalUnit {
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
    assert!(response.directory.organizational_units.is_empty());
    assert_eq!(
        response.directory.organizational_unit_dns.get("ou-root"),
        Some(&"OU=Root,OU=Mirror,DC=a,DC=example,DC=com".to_string())
    );
    assert!(response.directory.groups.is_empty());
    assert!(response.credentials.credentials.is_empty());
}

#[tokio::test]
async fn connector_sync_honors_the_configured_directory_batch_limit() {
    let TestApp { app, repository } = test_app_with_batch_limit(2).await;

    repository
        .upsert_directory(
            vec![
                adscope_protocol::OrganizationalUnit {
                    id: "ou-root".to_string(),
                    name: "Root".to_string(),
                    parent_id: None,
                    changed_revision: 0,
                },
                adscope_protocol::OrganizationalUnit {
                    id: "ou-unused".to_string(),
                    name: "Unused".to_string(),
                    parent_id: None,
                    changed_revision: 0,
                },
            ],
            vec![user_patch("1001", "first@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1002", "second@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .upsert_directory(
            vec![adscope_protocol::OrganizationalUnit {
                id: "ou-child".to_string(),
                name: "Child".to_string(),
                parent_id: Some("ou-root".to_string()),
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                organizational_unit_id: "ou-child".to_string(),
                ..user_patch("1003", "third@example.com")
            }],
            Vec::new(),
        )
        .await
        .unwrap();

    let first_batch = connector_sync(&app, 0, 0, false, false).await.directory;
    assert_eq!(first_batch.server_revision, 3);
    assert_eq!(first_batch.batch_revision, 2);
    assert!(first_batch.has_more);
    assert_eq!(first_batch.users.len(), 2);

    confirm(
        &app,
        SyncChannel::Directory,
        first_batch.batch_revision,
        true,
    )
    .await;
    let second_batch = connector_sync(&app, 2, 0, false, false).await.directory;
    assert_eq!(second_batch.batch_revision, 3);
    assert!(!second_batch.has_more);
    assert_eq!(second_batch.organizational_units.len(), 1);
    assert_eq!(second_batch.organizational_units[0].id, "ou-child");
    assert_eq!(
        second_batch.organizational_unit_dns.get("ou-child"),
        Some(&"OU=Child,OU=Root,OU=Mirror,DC=a,DC=example,DC=com".to_string())
    );
    assert!(
        !second_batch
            .organizational_unit_dns
            .contains_key("ou-unused")
    );
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
async fn management_session_exchanges_token_for_protected_cookie() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(json_request(
            "/api/admin/session",
            &json!({ "token": MANAGEMENT_TOKEN }),
        ))
        .await
        .expect("management session response");

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("management session cookie");
    assert!(set_cookie.starts_with("adscope_management="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("SameSite=Strict"));
    assert!(set_cookie.contains("Path=/api/admin"));
    assert!(set_cookie.contains("Max-Age=28800"));
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let raw_body = String::from_utf8_lossy(&body);
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["csrf_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    assert!(!raw_body.contains(MANAGEMENT_TOKEN));
}

#[tokio::test]
async fn management_session_errors_always_disable_caching() {
    let TestApp { app, .. } = test_app().await;

    let missing_content_type = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/admin/session")
                .body(Body::from(r#"{"token":"wrong-management-token"}"#))
                .unwrap(),
        )
        .await
        .expect("missing content type response");
    assert!(missing_content_type.status().is_client_error());
    assert_eq!(
        missing_content_type.headers().get("cache-control").unwrap(),
        "no-store"
    );

    let invalid_token = app
        .clone()
        .oneshot(json_request(
            "/api/admin/session",
            &json!({ "token": "wrong-management-token" }),
        ))
        .await
        .expect("invalid management token response");
    assert_eq!(invalid_token.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        invalid_token.headers().get("cache-control").unwrap(),
        "no-store"
    );

    let missing_cookie = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("missing management cookie response");
    assert_eq!(missing_cookie.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        missing_cookie.headers().get("cache-control").unwrap(),
        "no-store"
    );

    let created = app
        .clone()
        .oneshot(json_request(
            "/api/admin/session",
            &json!({ "token": MANAGEMENT_TOKEN }),
        ))
        .await
        .expect("management session creation response");
    let management_cookie = created
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("management cookie")
        .to_string();
    let missing_csrf = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/admin/session")
                .header("cookie", management_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("missing csrf response");
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        missing_csrf.headers().get("cache-control").unwrap(),
        "no-store"
    );
}

#[tokio::test]
async fn admin_routes_reject_normal_user_cookie() {
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;
    let login = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                username: "1001".to_string(),
                password: "OldPass123!".to_string(),
            },
        ))
        .await
        .expect("user login response");
    assert_eq!(login.status(), StatusCode::OK);
    let user_cookie = login
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("user cookie");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/domains")
                .header("cookie", user_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("admin response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn management_cookie_session_restores_protects_writes_and_logs_out() {
    let TestApp { app, .. } = test_app().await;

    let created = app
        .clone()
        .oneshot(json_request(
            "/api/admin/session",
            &json!({ "token": MANAGEMENT_TOKEN }),
        ))
        .await
        .expect("management session creation response");
    assert_eq!(created.status(), StatusCode::OK);
    let management_cookie = created
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("management cookie")
        .to_string();
    let body = created.into_body().collect().await.unwrap().to_bytes();
    let csrf_token = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["csrf_token"]
        .as_str()
        .expect("csrf token")
        .to_string();

    let restored = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/session")
                .header("cookie", &management_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("management session restore response");
    assert_eq!(restored.status(), StatusCode::OK);
    assert_eq!(restored.headers().get("cache-control").unwrap(), "no-store");
    let restored_body = restored.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&restored_body).unwrap()["csrf_token"].as_str(),
        Some(csrf_token.as_str())
    );

    let bearer = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/domains")
                .header("authorization", format!("Bearer {MANAGEMENT_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("legacy bearer response");
    assert_eq!(bearer.status(), StatusCode::UNAUTHORIZED);

    let read = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/domains")
                .header("cookie", &management_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("management cookie read response");
    assert_eq!(read.status(), StatusCode::OK);

    let missing_csrf = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/admin/ous")
                .header("cookie", &management_cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"id":"ou-csrf","name":"CSRF","parent_id":null}"#,
                ))
                .unwrap(),
        )
        .await
        .expect("missing csrf response");
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

    let write = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/admin/ous")
                .header("cookie", &management_cookie)
                .header("x-adscope-csrf-token", &csrf_token)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"id":"ou-csrf","name":"CSRF","parent_id":null}"#,
                ))
                .unwrap(),
        )
        .await
        .expect("csrf-protected write response");
    assert_eq!(write.status(), StatusCode::OK);

    let logged_out = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/admin/session")
                .header("cookie", &management_cookie)
                .header("x-adscope-csrf-token", &csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("management session delete response");
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        logged_out.headers().get("cache-control").unwrap(),
        "no-store"
    );
    let deleted_cookie = logged_out
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("deleted management cookie");
    assert!(deleted_cookie.starts_with("adscope_management="));
    assert!(deleted_cookie.contains("Path=/api/admin"));
    assert!(deleted_cookie.contains("Max-Age=0"));
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
async fn creating_domain_generates_one_time_connector_key() {
    let TestApp { app, repository } = test_app().await;

    let response = app
        .clone()
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/domains",
            &json!({
                "id": "domain-generated-key",
                "name": "Generated Key Domain",
                "enabled": true,
                "mirror_root_dn": "OU=Mirror,DC=generated,DC=example,DC=com",
                "quarantine_ou_dn": "OU=Quarantine,DC=generated,DC=example,DC=com",
                "upn_suffix": "generated.example.com",
                "employee_id_attribute": "employeeID",
                "managed_group_id_attribute": "adminDescription"
            }),
        ))
        .await
        .expect("create domain response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["domain"]["id"], "domain-generated-key");
    assert!(body["domain"].get("connector_key").is_none());
    assert!(body["domain"].get("connector_key_hash").is_none());

    let connector_key = body["connector_key"].as_str().unwrap();
    assert_eq!(connector_key.len(), 64);
    assert!(
        connector_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );

    let sync_response =
        request_domain_with_connector_key(&app, "domain-generated-key", connector_key).await;
    assert_eq!(sync_response.status(), StatusCode::OK);

    let stored_domain = repository
        .get_domain("domain-generated-key")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_domain.connector_key_hash,
        connector_key_hash(connector_key)
    );
    assert_ne!(stored_domain.connector_key_hash, connector_key);

    let domains = admin_empty(&app, Method::GET, "/api/admin/domains").await;
    let listed_domain = domains["domains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|domain| domain["id"] == "domain-generated-key")
        .unwrap();
    assert!(listed_domain.get("connector_key").is_none());
    assert!(listed_domain.get("connector_key_hash").is_none());
}

#[tokio::test]
async fn creating_domain_rejects_client_supplied_connector_key_fields() {
    let TestApp { app, repository } = test_app().await;

    for (domain_id, injected_field) in [
        (
            "domain-manual-key",
            json!({ "connector_key": "client-supplied-key" }),
        ),
        (
            "domain-manual-hash",
            json!({ "connector_key_hash": "client-supplied-hash" }),
        ),
    ] {
        let mut request = json!({
                "id": domain_id,
                "name": "Manual Key Domain",
                "enabled": true,
                "mirror_root_dn": "OU=Mirror,DC=manual,DC=example,DC=com",
                "quarantine_ou_dn": "OU=Quarantine,DC=manual,DC=example,DC=com",
                "upn_suffix": "manual.example.com",
                "employee_id_attribute": "employeeID",
                "managed_group_id_attribute": "adminDescription"
        });
        request
            .as_object_mut()
            .unwrap()
            .extend(injected_field.as_object().unwrap().clone());

        let response = app
            .clone()
            .oneshot(admin_json_request(
                Method::POST,
                "/api/admin/domains",
                &request,
            ))
            .await
            .expect("create domain response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(repository.get_domain(domain_id).await.unwrap().is_none());
    }
}

#[tokio::test]
async fn creating_duplicate_domain_returns_conflict_without_changing_existing_domain() {
    let TestApp { app, repository } = test_app().await;
    let existing_domain = DomainRecord {
        name: "Existing Domain".to_string(),
        connector_key_hash: connector_key_hash("existing-connector-key"),
        applied_directory_revision: 12,
        applied_credential_revision: 34,
        ..domain(true)
    };
    repository
        .upsert_domain(existing_domain.clone())
        .await
        .unwrap();

    let response = app
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/domains",
            &json!({
                "id": "domain-a",
                "name": "Replacement Domain",
                "enabled": false,
                "mirror_root_dn": "OU=Replacement,DC=a,DC=example,DC=com",
                "quarantine_ou_dn": "OU=ReplacementQuarantine,DC=a,DC=example,DC=com",
                "upn_suffix": "replacement.example.com",
                "employee_id_attribute": "employeeNumber",
                "managed_group_id_attribute": "extensionAttribute10"
            }),
        ))
        .await
        .expect("create domain response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        repository.get_domain("domain-a").await.unwrap().unwrap(),
        existing_domain
    );
}

#[tokio::test]
async fn updating_domain_regenerates_connector_key_and_preserves_applied_revisions() {
    let TestApp { app, repository } = test_app().await;
    let old_connector_key = "domain-rekey-old-connector-key";
    repository
        .upsert_domain(DomainRecord {
            id: "domain-rekey".to_string(),
            name: "Domain Rekey".to_string(),
            enabled: true,
            mirror_root_dn: "OU=Mirror,DC=rekey,DC=example,DC=com".to_string(),
            quarantine_ou_dn: "OU=Quarantine,DC=rekey,DC=example,DC=com".to_string(),
            upn_suffix: "rekey.example.com".to_string(),
            employee_id_attribute: "employeeID".to_string(),
            managed_group_id_attribute: "adminDescription".to_string(),
            connector_key_hash: connector_key_hash(old_connector_key),
            applied_directory_revision: 12,
            applied_credential_revision: 34,
        })
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(admin_json_request(
            Method::PATCH,
            "/api/admin/domains/domain-rekey",
            &json!({
                "name": "Domain Rekey Updated",
                "enabled": true,
                "mirror_root_dn": "OU=Mirror2,DC=rekey,DC=example,DC=com",
                "quarantine_ou_dn": "OU=Quarantine2,DC=rekey,DC=example,DC=com",
                "upn_suffix": "rekey2.example.com",
                "employee_id_attribute": "employeeNumber",
                "managed_group_id_attribute": "extensionAttribute10"
            }),
        ))
        .await
        .expect("update domain response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["domain"]["name"], "Domain Rekey Updated");
    assert_eq!(body["domain"]["applied_directory_revision"], 12);
    assert_eq!(body["domain"]["applied_credential_revision"], 34);

    let new_connector_key = body["connector_key"].as_str().unwrap();
    assert_eq!(new_connector_key.len(), 64);
    assert!(
        new_connector_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_ne!(new_connector_key, old_connector_key);

    let old_key_sync =
        request_domain_with_connector_key(&app, "domain-rekey", old_connector_key).await;
    assert_eq!(old_key_sync.status(), StatusCode::UNAUTHORIZED);
    let new_key_sync =
        request_domain_with_connector_key(&app, "domain-rekey", new_connector_key).await;
    assert_eq!(new_key_sync.status(), StatusCode::OK);

    let stored_domain = repository
        .get_domain("domain-rekey")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_domain.connector_key_hash,
        connector_key_hash(new_connector_key)
    );
    assert_ne!(
        stored_domain.connector_key_hash,
        connector_key_hash(old_connector_key)
    );
    assert_eq!(stored_domain.applied_directory_revision, 12);
    assert_eq!(stored_domain.applied_credential_revision, 34);
}

#[tokio::test]
async fn updating_domain_rejects_client_supplied_connector_key_fields() {
    let TestApp { app, repository } = test_app().await;

    for injected_field in [
        json!({ "connector_key": "client-supplied-key" }),
        json!({ "connector_key_hash": "client-supplied-hash" }),
    ] {
        let mut request = json!({
            "name": "Domain A",
            "enabled": true,
            "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
            "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
            "upn_suffix": "a.example.com",
            "employee_id_attribute": "employeeID",
            "managed_group_id_attribute": "adminDescription"
        });
        request
            .as_object_mut()
            .unwrap()
            .extend(injected_field.as_object().unwrap().clone());

        let response = app
            .clone()
            .oneshot(admin_json_request(
                Method::PATCH,
                "/api/admin/domains/domain-a",
                &request,
            ))
            .await
            .expect("update domain response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let stored_domain = repository.get_domain("domain-a").await.unwrap().unwrap();
    assert_eq!(
        stored_domain.connector_key_hash,
        connector_key_hash(CONNECTOR_KEY)
    );
}

#[tokio::test]
async fn updating_missing_domain_returns_not_found() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(admin_json_request(
            Method::PATCH,
            "/api/admin/domains/missing-domain",
            &json!({
                "name": "Missing Domain",
                "enabled": true,
                "mirror_root_dn": "OU=Mirror,DC=missing,DC=example,DC=com",
                "quarantine_ou_dn": "OU=Quarantine,DC=missing,DC=example,DC=com",
                "upn_suffix": "missing.example.com",
                "employee_id_attribute": "employeeID",
                "managed_group_id_attribute": "adminDescription"
            }),
        ))
        .await
        .expect("update missing domain response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn web_static_routes_do_not_capture_unknown_api_paths() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    let web_root = test_web_root();
    std::fs::create_dir_all(web_root.join("_nuxt")).unwrap();
    std::fs::write(
        web_root.join("index.html"),
        "<main>Adscope Web Shell</main>",
    )
    .unwrap();
    std::fs::write(
        web_root.join("_nuxt").join("app.js"),
        "console.log('adscope')",
    )
    .unwrap();
    let app = build_router_with_web_root(
        AppState::new_for_tests(
            repository,
            TEST_ENCRYPTION_KEY,
            TEST_OIDC_ISSUER,
            TEST_OIDC_PRIVATE_KEY,
        ),
        web_root.clone(),
    );

    let index = app
        .clone()
        .oneshot(empty_request(Method::GET, "/"))
        .await
        .expect("index response");
    assert_eq!(index.status(), StatusCode::OK);
    assert_body_contains(index, "Adscope Web Shell").await;

    let frontend_route = app
        .clone()
        .oneshot(empty_request(Method::GET, "/admin/domains"))
        .await
        .expect("frontend route response");
    assert_eq!(frontend_route.status(), StatusCode::OK);
    assert_body_contains(frontend_route, "Adscope Web Shell").await;

    let asset = app
        .clone()
        .oneshot(empty_request(Method::GET, "/_nuxt/app.js"))
        .await
        .expect("asset response");
    assert_eq!(asset.status(), StatusCode::OK);
    assert_body_contains(asset, "console.log('adscope')").await;

    let api_get = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/not-found"))
        .await
        .expect("api response");
    assert_eq!(api_get.status(), StatusCode::NOT_FOUND);
    assert_body_not_contains(api_get, "Adscope Web Shell").await;

    let api_post = app
        .oneshot(empty_request(Method::POST, "/api/not-found"))
        .await
        .expect("api response");
    assert_eq!(api_post.status(), StatusCode::NOT_FOUND);
    assert_body_not_contains(api_post, "Adscope Web Shell").await;

    std::fs::remove_dir_all(web_root).unwrap();
}

#[tokio::test]
async fn admin_create_user_returns_conflict_for_duplicate_employee_id_or_username() {
    let TestApp { app, .. } = test_app().await;
    let initial_user = json!({
        "employee_id": "1001",
        "username": "zhangsan",
        "display_name": "张三",
        "email": null,
        "mobile": null,
        "telephone": null,
        "organizational_unit_id": "ou-root",
        "status": "active",
        "initial_password": "InitialPass123!"
    });
    admin_json(&app, Method::POST, "/api/admin/users", &initial_user).await;

    let duplicate_employee_id = app
        .clone()
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/users",
            &json!({
                "employee_id": "1001",
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
        .expect("duplicate employee ID response");
    assert_eq!(duplicate_employee_id.status(), StatusCode::CONFLICT);

    let duplicate_username = app
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/users",
            &json!({
                "employee_id": "1002",
                "username": "zhangsan",
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
        .expect("duplicate username response");
    assert_eq!(duplicate_username.status(), StatusCode::CONFLICT);
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
            "managed_group_id_attribute": "adminDescription"
        }),
    )
    .await;
    assert_eq!(created_domain["domain"]["id"], "domain-b");
    let created_connector_key = created_domain["connector_key"].as_str().unwrap();
    assert!(created_domain["domain"].get("connector_key").is_none());
    assert!(created_domain["domain"].get("connector_key_hash").is_none());

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
            "managed_group_id_attribute": "extensionAttribute10"
        }),
    )
    .await;
    assert_eq!(patched_domain["domain"]["name"], "Domain B Updated");
    let patched_connector_key = patched_domain["connector_key"].as_str().unwrap();
    assert_ne!(patched_connector_key, created_connector_key);
    let stored_domain = repository.get_domain("domain-b").await.unwrap().unwrap();
    assert_eq!(
        stored_domain.connector_key_hash,
        connector_key_hash(patched_connector_key)
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
    assert_eq!(created_user["user"]["employee_id"], "1001");
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
    login(&app, "zhangsan", "ResetPass123!").await;

    let created_group = admin_json(
        &app,
        Method::POST,
        "/api/admin/groups",
        &json!({
            "id": "group-rd",
            "name": "研发组",
            "organizational_unit_id": "ou-rd"
        }),
    )
    .await;
    assert_eq!(created_group["group"]["organizational_unit_id"], "ou-rd");
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
        &json!({
            "name": "研发组 Updated",
            "organizational_unit_id": "ou-root"
        }),
    )
    .await;
    assert_eq!(renamed_group["group"]["name"], "研发组 Updated");
    assert_eq!(renamed_group["group"]["organizational_unit_id"], "ou-root");
    let group = admin_empty(&app, Method::GET, "/api/admin/groups/group-rd").await;
    assert_eq!(group["organizational_unit_id"], "ou-root");
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
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 2, false).await;
    let response = connector_sync(&app, 0, 0, false, false).await;

    assert_eq!(response.directory.server_revision, 2);
    assert_eq!(response.directory.batch_revision, 2);
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

    assert!(token.starts_with("adscope-user-session:v2."));
}

#[tokio::test]
async fn login_uses_username_not_employee_id() {
    let TestApp { app, .. } = test_app_with_seeded_user("1001", "zhangsan", "OldPass123!").await;

    let token = login_token(&app, "zhangsan", "OldPass123!").await;
    let employee_id_login = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                username: "1001".to_string(),
                password: "OldPass123!".to_string(),
            },
        ))
        .await
        .expect("login response");

    assert!(token.starts_with("adscope-user-session:v2."));
    assert_eq!(employee_id_login.status(), StatusCode::UNAUTHORIZED);
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
                username: "1001".to_string(),
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
    let TestApp { app, .. } = test_app_with_seeded_credential("OldPass123!").await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    let response = app
        .clone()
        .oneshot(connector_json_request(
            "/api/connector/confirm",
            "wrong-key",
            &ConnectorConfirmRequest {
                domain_id: "domain-a".to_string(),
                channel: SyncChannel::Directory,
                target_revision: 2,
                success: true,
                error_code: None,
            },
        ))
        .await
        .expect("confirm response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let sync = connector_sync(&app, 0, 0, false, false).await;
    assert_eq!(sync.directory.users.len(), 1);
    assert_eq!(sync.directory.batch_revision, 2);
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
    let app = build_router(AppState::new_for_tests(
        repository,
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    ));

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
    set_oidc_env();

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing password encryption key must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSCOPE_PASSWORD_ENCRYPTION_KEY is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_retired_adss_environment_variables() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    unsafe {
        std::env::set_var("ADSS_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("retired ADSS configuration must not configure AppState"),
        Err(error) => error,
    };

    assert_eq!(
        error.to_string(),
        "ADSS_MANAGEMENT_TOKEN is retired; use ADSCOPE_MANAGEMENT_TOKEN"
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_accepts_password_encryption_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_oidc_env();
    unsafe {
        std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSCOPE_PASSWORD_HASH_PROVIDER", "argon2id");
        std::env::set_var("ADSCOPE_USER_SESSION_KEY", "test-user-session-key");
        std::env::set_var("ADSCOPE_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
    }

    let state = AppState::from_env(repository).unwrap();
    assert_eq!(state.oidc.config().issuer(), TEST_OIDC_ISSUER);
    assert!(!state.oidc.config().allow_insecure_web_loopback_redirects());

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_password_hash_provider() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_oidc_env();
    unsafe {
        std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing password hash provider must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSCOPE_PASSWORD_HASH_PROVIDER is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_user_session_key() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_oidc_env();
    unsafe {
        std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSCOPE_PASSWORD_HASH_PROVIDER", "argon2id");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing user session key must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSCOPE_USER_SESSION_KEY is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_management_token() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_oidc_env();
    unsafe {
        std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSCOPE_PASSWORD_HASH_PROVIDER", "argon2id");
        std::env::set_var("ADSCOPE_USER_SESSION_KEY", "test-user-session-key");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing management token must not configure AppState"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("ADSCOPE_MANAGEMENT_TOKEN is required")
    );

    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_invalid_oidc_issuer() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_valid_server_env();
    unsafe {
        std::env::set_var("ADSCOPE_OIDC_ISSUER", "http://center.example.test");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("HTTP OIDC issuer must not configure AppState"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("ADSCOPE_OIDC_ISSUER must use HTTPS")
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_requires_oidc_issuer() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_valid_server_env();
    unsafe {
        std::env::remove_var("ADSCOPE_OIDC_ISSUER");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing OIDC issuer must not configure AppState"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("ADSCOPE_OIDC_ISSUER is required")
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_requires_oidc_private_key_file() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_valid_server_env();
    unsafe {
        std::env::remove_var("ADSCOPE_OIDC_PRIVATE_KEY_FILE");
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing OIDC private key setting must not configure AppState"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("ADSCOPE_OIDC_PRIVATE_KEY_FILE is required")
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_missing_oidc_private_key_file() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_valid_server_env();
    unsafe {
        std::env::set_var(
            "ADSCOPE_OIDC_PRIVATE_KEY_FILE",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/missing.pem"),
        );
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("missing OIDC private key file must not configure AppState"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("failed to read ADSCOPE_OIDC_PRIVATE_KEY_FILE")
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_rejects_invalid_oidc_private_key_contents() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    let _guard = ENV_LOCK.lock().unwrap();
    clear_server_env();
    set_valid_server_env();
    unsafe {
        std::env::set_var(
            "ADSCOPE_OIDC_PRIVATE_KEY_FILE",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        );
    }

    let error = match AppState::from_env(repository) {
        Ok(_) => panic!("invalid OIDC PEM must not configure AppState"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("OIDC private key must be valid RSA")
    );
    clear_server_env();
}

#[tokio::test]
async fn app_state_from_env_uses_argon2id_password_hash_provider_for_login_and_change() {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    seed_user(&repository, "1001", "old@example.com")
        .await
        .unwrap();
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
        set_oidc_env();
        unsafe {
            std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
            std::env::set_var("ADSCOPE_PASSWORD_HASH_PROVIDER", "argon2id");
            std::env::set_var("ADSCOPE_USER_SESSION_KEY", "test-user-session-key");
            std::env::set_var("ADSCOPE_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
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

async fn test_app_with_batch_limit(batch_limit: usize) -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    let app = build_router(AppState::with_batch_limit_for_tests(
        repository.clone(),
        batch_limit,
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    ));
    TestApp { app, repository }
}

async fn test_app_with_domain_enabled(enabled: bool) -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(enabled)).await.unwrap();
    let app = build_router(AppState::new_for_tests(
        repository.clone(),
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    ));
    TestApp { app, repository }
}

async fn test_app_with_seeded_credential(password: &str) -> TestApp {
    test_app_with_seeded_user("1001", "1001", password).await
}

async fn test_app_with_seeded_user(employee_id: &str, username: &str, password: &str) -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain(true)).await.unwrap();
    seed_user_with_username(&repository, employee_id, username, "old@example.com")
        .await
        .unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: employee_id.to_string(),
            password_ciphertext: seal_password_for_storage(password),
            password_verifier: password_verifier(password),
        })
        .await
        .unwrap();
    let app = build_router(AppState::new_for_tests(
        repository.clone(),
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
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

async fn login(app: &axum::Router, username: &str, password: &str) {
    let response = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                username: username.to_string(),
                password: password.to_string(),
            },
        ))
        .await
        .expect("login response");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn login_token(app: &axum::Router, username: &str, password: &str) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            &UserLoginRequest {
                username: username.to_string(),
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
                .header("cookie", management_session_cookie())
                .header("x-adscope-csrf-token", MANAGEMENT_CSRF_TOKEN)
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
        .header("cookie", management_session_cookie())
        .header("x-adscope-csrf-token", MANAGEMENT_CSRF_TOKEN)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn management_session_cookie() -> String {
    let payload = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "auth_time": 0,
            "expires_at": u64::MAX,
            "csrf_nonce": MANAGEMENT_CSRF_TOKEN,
        }))
        .unwrap(),
    );
    let signed = format!("adscope-management-session:v1.{payload}");
    let mut key_derivation =
        <Hmac<Sha256> as Mac>::new_from_slice(MANAGEMENT_TOKEN.as_bytes()).unwrap();
    key_derivation.update(b"adscope:management-session:v1");
    let key = key_derivation.finalize().into_bytes();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).unwrap();
    mac.update(signed.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("adscope_management={signed}.{signature}")
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
    request_domain_with_connector_key(app, "domain-a", connector_key).await
}

async fn request_domain_with_connector_key(
    app: &axum::Router,
    domain_id: &str,
    connector_key: &str,
) -> Response<Body> {
    app.clone()
        .oneshot(connector_json_request(
            "/api/connector/sync",
            connector_key,
            &ConnectorSyncRequest {
                domain_id: domain_id.to_string(),
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
        .header("x-adscope-connector-key", connector_key)
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

fn empty_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
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

async fn assert_body_contains(response: Response<Body>, expected: &str) {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body.contains(expected),
        "body did not contain {expected}: {body}"
    );
}

async fn assert_body_not_contains(response: Response<Body>, unexpected: &str) {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        !body.contains(unexpected),
        "body contained {unexpected}: {body}"
    );
}

fn test_web_root() -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("adscope-web-test-{suffix}"))
}

async fn seed_user(repository: &Repository, employee_id: &str, email: &str) -> anyhow::Result<u64> {
    seed_user_with_username(repository, employee_id, employee_id, email).await
}

async fn seed_user_with_username(
    repository: &Repository,
    employee_id: &str,
    username: &str,
    email: &str,
) -> anyhow::Result<u64> {
    repository
        .upsert_directory(
            vec![adscope_protocol::OrganizationalUnit {
                id: "ou-root".to_string(),
                name: "Root".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                employee_id: employee_id.to_string(),
                username: username.to_string(),
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
    hasher.update(b"adscope:test-password-verifier:v1");
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
    hasher.update(b"adscope:password-encryption:v1");
    hasher.update(TEST_ENCRYPTION_KEY.as_bytes());
    hasher.finalize().into()
}

fn clear_server_env() {
    unsafe {
        std::env::remove_var("ADSS_MANAGEMENT_TOKEN");
        std::env::remove_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY");
        std::env::remove_var("ADSCOPE_PASSWORD_HASH_PROVIDER");
        std::env::remove_var("ADSCOPE_USER_SESSION_KEY");
        std::env::remove_var("ADSCOPE_USER_SESSION_TTL_SECONDS");
        std::env::remove_var("ADSCOPE_MANAGEMENT_TOKEN");
        std::env::remove_var("ADSCOPE_OIDC_ISSUER");
        std::env::remove_var("ADSCOPE_OIDC_PRIVATE_KEY_FILE");
        std::env::remove_var("ADSCOPE_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS");
    }
}

fn set_oidc_env() {
    unsafe {
        std::env::set_var("ADSCOPE_OIDC_ISSUER", TEST_OIDC_ISSUER);
        std::env::set_var("ADSCOPE_OIDC_PRIVATE_KEY_FILE", TEST_OIDC_PRIVATE_KEY_PATH);
    }
}

fn set_valid_server_env() {
    unsafe {
        std::env::set_var("ADSCOPE_PASSWORD_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY);
        std::env::set_var("ADSCOPE_PASSWORD_HASH_PROVIDER", "argon2id");
        std::env::set_var("ADSCOPE_USER_SESSION_KEY", "test-user-session-key");
        std::env::set_var("ADSCOPE_MANAGEMENT_TOKEN", MANAGEMENT_TOKEN);
    }
    set_oidc_env();
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

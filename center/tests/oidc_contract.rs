use std::time::{SystemTime, UNIX_EPOCH};

use adss_center::{
    AppState, build_router,
    oidc::{AccessTokenClaims, OidcService, config::OidcConfig},
};
use adss_protocol::{OrganizationalUnit, UserLoginRequest, UserStatus};
use adss_store::{
    OAuthClientRecord, OAuthClientType, Repository, UserCredentialInput, UserDirectoryPatch,
};
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, pkcs8::DecodePrivateKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const MANAGEMENT_TOKEN: &str = "test-management-token";
const TEST_ENCRYPTION_KEY: &str = "test-password-encryption-key";
const TEST_OIDC_ISSUER: &str = "https://center.example.test";
const TEST_OIDC_PRIVATE_KEY: &[u8] = include_bytes!("fixtures/oidc-private-key.pem");
const OIDC_CLIENT_ID: &str = "client_oidc_contract";

struct TestApp {
    app: axum::Router,
    repository: Repository,
    oidc: OidcService,
}

#[tokio::test]
async fn discovery_endpoint_is_public() {
    let TestApp { app, .. } = test_app().await;

    let request = Request::builder()
        .method(Method::GET)
        .uri("/.well-known/openid-configuration")
        .header("host", "attacker.example.test")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=300"
    );
    assert_eq!(
        json_body(response).await,
        json!({
            "issuer": TEST_OIDC_ISSUER,
            "authorization_endpoint": "https://center.example.test/oauth2/authorize",
            "token_endpoint": "https://center.example.test/oauth2/token",
            "userinfo_endpoint": "https://center.example.test/oauth2/userinfo",
            "jwks_uri": "https://center.example.test/oauth2/jwks",
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "scopes_supported": ["openid", "profile", "email", "phone"],
            "token_endpoint_auth_methods_supported": ["client_secret_basic", "none"],
            "code_challenge_methods_supported": ["S256"]
        })
    );
}

#[tokio::test]
async fn jwks_endpoint_is_public() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(empty_request(Method::GET, "/oauth2/jwks"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=300"
    );
    let body = json_body(response).await;
    let keys = body["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
    let key = keys[0].as_object().unwrap();
    assert_eq!(key.len(), 6);
    assert_eq!(key["kty"], "RSA");
    assert_eq!(key["use"], "sig");
    assert_eq!(key["alg"], "RS256");
    assert!(key["kid"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(key["n"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(key["e"].as_str().is_some_and(|value| !value.is_empty()));
    for private_name in ["d", "p", "q", "dp", "dq", "qi"] {
        assert!(!key.contains_key(private_name));
    }
}

#[tokio::test]
async fn userinfo_without_bearer_token_returns_invalid_token() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(empty_request(Method::GET, "/oauth2/userinfo"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_invalid_token(response).await;
}

#[test]
fn access_token_has_fixed_claims_scope_and_lifetime() {
    let oidc = test_oidc_service();
    let issued_at = unix_seconds();
    let token = oidc
        .issue_access_token_at(
            "1001",
            OIDC_CLIENT_ID,
            "openid profile email phone",
            issued_at,
        )
        .unwrap();
    let claims = oidc.verify_access_token(&token).unwrap();

    assert_eq!(claims.iss, TEST_OIDC_ISSUER);
    assert_eq!(claims.sub, "1001");
    assert_eq!(claims.aud, "https://center.example.test/oauth2/userinfo");
    assert_eq!(claims.client_id, OIDC_CLIENT_ID);
    assert_eq!(claims.scope, "openid profile email phone");
    assert_eq!(claims.iat, issued_at);
    assert_eq!(claims.exp - claims.iat, 300);
    assert!(
        oidc.issue_access_token("1001", OIDC_CLIENT_ID, "profile")
            .is_err()
    );
    assert!(
        oidc.issue_access_token("1001", OIDC_CLIENT_ID, "openid  profile")
            .is_err()
    );
}

#[test]
fn access_token_enforces_clock_skew_audience_and_fixed_ttl() {
    let oidc = test_oidc_service();
    let now = unix_seconds();
    let within_clock_skew = oidc
        .issue_access_token_at("1001", OIDC_CLIENT_ID, "openid", now + 30)
        .unwrap();
    assert!(oidc.verify_access_token(&within_clock_skew).is_ok());

    let future = oidc
        .issue_access_token_at("1001", OIDC_CLIENT_ID, "openid", now + 61)
        .unwrap();
    assert!(oidc.verify_access_token(&future).is_err());

    let expired = oidc
        .issue_access_token_at("1001", OIDC_CLIENT_ID, "openid", now - 331)
        .unwrap();
    assert!(oidc.verify_access_token(&expired).is_err());

    let valid = oidc
        .issue_access_token_at("1001", OIDC_CLIENT_ID, "openid", now)
        .unwrap();
    let mut claims = oidc.verify_access_token(&valid).unwrap();
    claims.aud = "https://center.example.test/oauth2/not-userinfo".to_string();
    assert!(
        oidc.verify_access_token(&signed_access_token(&oidc, &claims))
            .is_err()
    );

    claims.aud = "https://center.example.test/oauth2/userinfo".to_string();
    claims.exp += 1;
    assert!(
        oidc.verify_access_token(&signed_access_token(&oidc, &claims))
            .is_err()
    );
}

#[tokio::test]
async fn userinfo_rejects_non_oidc_invalid_and_expired_tokens() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;

    let user_session = login_token(&app).await;
    let valid = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid")
        .unwrap();
    let bad_signature = corrupt_signature(&valid);
    let mut wrong_audience_claims = oidc.verify_access_token(&valid).unwrap();
    wrong_audience_claims.aud = "https://center.example.test/oauth2/not-userinfo".to_string();
    let wrong_audience = signed_access_token(&oidc, &wrong_audience_claims);
    let expired = oidc
        .issue_access_token_at("1001", OIDC_CLIENT_ID, "openid", unix_seconds() - 331)
        .unwrap();

    for token in [user_session, bad_signature, wrong_audience, expired] {
        let response = app
            .clone()
            .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
            .await
            .unwrap();
        assert_invalid_token(response).await;
    }
}

#[tokio::test]
async fn userinfo_returns_only_claims_granted_by_scope() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user_profile(
        &repository,
        Some("user@example.test"),
        Some("13800000000"),
        Some("010-12345678"),
        UserStatus::Active,
    )
    .await;
    seed_oidc_client(&repository).await;

    let openid_token = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid")
        .unwrap();
    let openid_response = app
        .clone()
        .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &openid_token))
        .await
        .unwrap();
    assert_eq!(openid_response.status(), StatusCode::OK);
    assert_eq!(openid_response.headers()["cache-control"], "no-store");
    assert_eq!(json_body(openid_response).await, json!({ "sub": "1001" }));

    let full_token = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid profile email phone")
        .unwrap();
    let full_response = app
        .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &full_token))
        .await
        .unwrap();
    assert_eq!(full_response.status(), StatusCode::OK);
    assert_eq!(full_response.headers()["cache-control"], "no-store");
    assert_eq!(
        json_body(full_response).await,
        json!({
            "sub": "1001",
            "preferred_username": "test-user",
            "name": "Test User",
            "email": "user@example.test",
            "phone_number": "13800000000"
        })
    );
}

#[tokio::test]
async fn userinfo_omits_empty_optional_values_and_falls_back_to_telephone() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user_profile(
        &repository,
        Some(""),
        Some(""),
        Some("010-12345678"),
        UserStatus::Active,
    )
    .await;
    seed_oidc_client(&repository).await;
    let token = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid email phone")
        .unwrap();

    let response = app
        .clone()
        .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({ "sub": "1001", "phone_number": "010-12345678" })
    );

    seed_user_profile(
        &repository,
        Some(""),
        Some(""),
        Some(""),
        UserStatus::Active,
    )
    .await;
    let empty_response = app
        .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
        .await
        .unwrap();
    assert_eq!(empty_response.status(), StatusCode::OK);
    assert_eq!(json_body(empty_response).await, json!({ "sub": "1001" }));
}

#[tokio::test]
async fn userinfo_rechecks_current_user_and_client_state() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user(&repository).await;
    let mut client = seed_oidc_client(&repository).await;
    let token = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid")
        .unwrap();

    client.enabled = false;
    repository
        .update_oauth_client(client.clone())
        .await
        .unwrap()
        .unwrap();
    assert_invalid_token(
        app.clone()
            .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
            .await
            .unwrap(),
    )
    .await;

    client.enabled = true;
    repository
        .update_oauth_client(client)
        .await
        .unwrap()
        .unwrap();
    seed_user_profile(&repository, None, None, None, UserStatus::Disabled).await;
    assert_invalid_token(
        app.clone()
            .oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
            .await
            .unwrap(),
    )
    .await;

    repository
        .delete_oauth_client(OIDC_CLIENT_ID)
        .await
        .unwrap();
    assert_invalid_token(
        app.oneshot(auth_request(Method::GET, "/oauth2/userinfo", &token))
            .await
            .unwrap(),
    )
    .await;
}

#[tokio::test]
async fn userinfo_rejects_missing_user_and_api_me_rejects_oidc_access_token() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_oidc_client(&repository).await;
    let missing_user_token = oidc
        .issue_access_token("missing", OIDC_CLIENT_ID, "openid")
        .unwrap();
    assert_invalid_token(
        app.clone()
            .oneshot(auth_request(
                Method::GET,
                "/oauth2/userinfo",
                &missing_user_token,
            ))
            .await
            .unwrap(),
    )
    .await;

    seed_user(&repository).await;
    let oidc_token = oidc
        .issue_access_token("1001", OIDC_CLIENT_ID, "openid")
        .unwrap();
    let response = app
        .oneshot(auth_request(Method::GET, "/api/me", &oidc_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_oauth_client_routes_are_available() {
    let TestApp { app, .. } = test_app().await;

    let response = app
        .oneshot(admin_request(Method::GET, "/api/admin/oauth-clients"))
        .await
        .expect("OAuth client list response");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_oauth_client_routes_require_management_token() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    let user_token = login_token(&app).await;

    let missing = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/admin/oauth-clients"))
        .await
        .unwrap();
    let user = app
        .oneshot(auth_request(
            Method::GET,
            "/api/admin/oauth-clients",
            &user_token,
        ))
        .await
        .unwrap();

    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(user.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_oauth_client_creates_web_with_one_time_secret_and_digest() {
    let TestApp {
        app, repository, ..
    } = test_app().await;

    let response = app
        .clone()
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/oauth-clients",
            &web_client_request("Web Portal"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = json_body(response).await;
    let client = &body["client"];
    let client_id = client["client_id"].as_str().unwrap();
    let secret = body["client_secret"].as_str().unwrap();
    assert!(client_id.starts_with("client_"));
    assert_eq!(client_id.len(), 50);
    assert!(secret.len() >= 43);
    assert_eq!(client["client_type"], "web");
    assert!(client.get("client_secret_hash").is_none());

    let stored = repository
        .get_oauth_client(client_id)
        .await
        .unwrap()
        .unwrap();
    let expected_hash = sha256_token(secret);
    assert_eq!(
        stored.client_secret_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert_ne!(stored.client_secret_hash.as_deref(), Some(secret));

    let listed = admin_json(&app, Method::GET, "/api/admin/oauth-clients", None).await;
    let serialized = serde_json::to_string(&listed).unwrap();
    assert!(!serialized.contains(secret));
    assert!(!serialized.contains("client_secret"));
    assert!(!serialized.contains("client_secret_hash"));
}

#[tokio::test]
async fn admin_oauth_client_creates_desktop_without_secret() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    let request = json!({
        "name": "Desktop Client",
        "client_type": "desktop",
        "redirect_uris": ["http://127.0.0.1/callback"],
        "allowed_scopes": ["openid", "profile"],
        "enabled": true
    });

    let response = app
        .oneshot(admin_json_request(
            Method::POST,
            "/api/admin/oauth-clients",
            &request,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = json_body(response).await;
    assert!(body["client_secret"].is_null());
    assert_eq!(body["client"]["client_type"], "desktop");
    let stored = repository
        .get_oauth_client(body["client"]["client_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(stored.client_secret_hash.is_none());
}

#[tokio::test]
async fn admin_oauth_client_list_is_sorted_and_redacted() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    for client_id in ["client_z", "client_a"] {
        repository
            .create_oauth_client(OAuthClientRecord {
                client_id: client_id.to_string(),
                name: client_id.to_string(),
                client_type: OAuthClientType::Web,
                client_secret_hash: Some(format!("secret-hash-{client_id}")),
                redirect_uris: vec!["https://client.example.com/callback".to_string()],
                allowed_scopes: vec!["openid".to_string()],
                enabled: true,
            })
            .await
            .unwrap()
            .unwrap();
    }

    let body = admin_json(&app, Method::GET, "/api/admin/oauth-clients", None).await;
    let clients = body["clients"].as_array().unwrap();
    assert_eq!(clients[0]["client_id"], "client_a");
    assert_eq!(clients[1]["client_id"], "client_z");
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains("secret-hash"));
    assert!(!serialized.contains("client_secret"));
}

#[tokio::test]
async fn admin_oauth_client_create_rejects_invalid_contract_fields() {
    let TestApp { app, .. } = test_app().await;
    let invalid_requests = [
        json!({
            "name": "",
            "client_type": "web",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["openid"],
            "enabled": true
        }),
        json!({
            "name": "Unsupported Type",
            "client_type": "service",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["openid"],
            "enabled": true
        }),
        json!({
            "name": "Missing OpenID",
            "client_type": "web",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["profile"],
            "enabled": true
        }),
        json!({
            "name": "Spaced Scope",
            "client_type": "web",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["openid", "profile email"],
            "enabled": true
        }),
        json!({
            "name": "Too Many Scopes",
            "client_type": "web",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["openid", "profile", "email", "phone", "extra"],
            "enabled": true
        }),
        json!({
            "name": "Insecure Redirect",
            "client_type": "web",
            "redirect_uris": ["http://client.example.com/callback"],
            "allowed_scopes": ["openid"],
            "enabled": true
        }),
        json!({
            "name": "Injected ID",
            "client_id": "client_supplied",
            "client_type": "web",
            "redirect_uris": ["https://client.example.com/callback"],
            "allowed_scopes": ["openid"],
            "enabled": true
        }),
    ];

    for request in invalid_requests {
        let response = app
            .clone()
            .oneshot(admin_json_request(
                Method::POST,
                "/api/admin/oauth-clients",
                &request,
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_client_error(),
            "request unexpectedly accepted: {request}"
        );
    }
}

#[tokio::test]
async fn admin_oauth_client_patch_preserves_identity_type_and_secret_hash() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    let created = create_web_client(&app, "Patch Original").await;
    let client_id = created["client"]["client_id"].as_str().unwrap();
    let original = repository
        .get_oauth_client(client_id)
        .await
        .unwrap()
        .unwrap();

    let response = app
        .clone()
        .oneshot(admin_json_request(
            Method::PATCH,
            &format!("/api/admin/oauth-clients/{client_id}"),
            &json!({
                "name": "Patch Updated",
                "redirect_uris": ["https://updated.example.com/callback"],
                "allowed_scopes": ["openid", "email"],
                "enabled": false
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["client_id"], client_id);
    assert_eq!(body["client_type"], "web");
    assert_eq!(body["enabled"], false);
    let updated = repository
        .get_oauth_client(client_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.client_id, original.client_id);
    assert_eq!(updated.client_type, original.client_type);
    assert_eq!(updated.client_secret_hash, original.client_secret_hash);

    let unknown = app
        .clone()
        .oneshot(admin_json_request(
            Method::PATCH,
            &format!("/api/admin/oauth-clients/{client_id}"),
            &json!({
                "name": "Injected",
                "client_type": "desktop",
                "redirect_uris": ["https://updated.example.com/callback"],
                "allowed_scopes": ["openid"],
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert!(unknown.status().is_client_error());

    let missing = app
        .oneshot(admin_json_request(
            Method::PATCH,
            "/api/admin/oauth-clients/client_missing",
            &json!({
                "name": "Missing",
                "redirect_uris": ["https://missing.example.com/callback"],
                "allowed_scopes": ["openid"],
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_oauth_client_regenerates_web_secret_and_rejects_desktop() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    let web = create_web_client(&app, "Regenerate Web").await;
    let web_id = web["client"]["client_id"].as_str().unwrap();
    let old_secret = web["client_secret"].as_str().unwrap();
    let old_hash = repository
        .get_oauth_client(web_id)
        .await
        .unwrap()
        .unwrap()
        .client_secret_hash
        .unwrap();

    let regenerated = app
        .clone()
        .oneshot(admin_request(
            Method::POST,
            &format!("/api/admin/oauth-clients/{web_id}/secret"),
        ))
        .await
        .unwrap();
    assert_eq!(regenerated.status(), StatusCode::OK);
    assert_eq!(
        regenerated.headers().get("cache-control").unwrap(),
        "no-store"
    );
    let regenerated = json_body(regenerated).await;
    assert_eq!(regenerated["client_id"], web_id);
    assert_eq!(regenerated.as_object().unwrap().len(), 2);
    let new_secret = regenerated["client_secret"].as_str().unwrap();
    assert_ne!(new_secret, old_secret);
    let new_hash = repository
        .get_oauth_client(web_id)
        .await
        .unwrap()
        .unwrap()
        .client_secret_hash
        .unwrap();
    assert_ne!(new_hash, old_hash);
    assert_eq!(new_hash, sha256_token(new_secret));

    let desktop = repository
        .create_oauth_client(OAuthClientRecord {
            client_id: "client_desktop".to_string(),
            name: "Desktop".to_string(),
            client_type: OAuthClientType::Desktop,
            client_secret_hash: None,
            redirect_uris: vec!["http://127.0.0.1/callback".to_string()],
            allowed_scopes: vec!["openid".to_string()],
            enabled: true,
        })
        .await
        .unwrap()
        .unwrap();
    let desktop_response = app
        .clone()
        .oneshot(admin_request(
            Method::POST,
            &format!("/api/admin/oauth-clients/{}/secret", desktop.client_id),
        ))
        .await
        .unwrap();
    assert!(matches!(
        desktop_response.status(),
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT
    ));
    let missing = app
        .oneshot(admin_request(
            Method::POST,
            "/api/admin/oauth-clients/client_missing/secret",
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_oauth_client_delete_returns_no_content_and_not_found() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    let created = create_web_client(&app, "Delete Web").await;
    let client_id = created["client"]["client_id"].as_str().unwrap();

    let deleted = app
        .clone()
        .oneshot(admin_request(
            Method::DELETE,
            &format!("/api/admin/oauth-clients/{client_id}"),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(
        deleted
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
    assert!(
        repository
            .get_oauth_client(client_id)
            .await
            .unwrap()
            .is_none()
    );

    let missing = app
        .oneshot(admin_request(
            Method::DELETE,
            "/api/admin/oauth-clients/client_missing",
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

async fn test_app() -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    let state = AppState::new_for_tests(
        repository.clone(),
        TEST_ENCRYPTION_KEY,
        TEST_OIDC_ISSUER,
        TEST_OIDC_PRIVATE_KEY,
    );
    let oidc = state.oidc.clone();
    let app = build_router(state);
    TestApp {
        app,
        repository,
        oidc,
    }
}

async fn seed_user(repository: &Repository) {
    seed_user_profile(repository, None, None, None, UserStatus::Active).await;
}

async fn seed_user_profile(
    repository: &Repository,
    email: Option<&str>,
    mobile: Option<&str>,
    telephone: Option<&str>,
    status: UserStatus,
) {
    repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-root".to_string(),
                name: "Root".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                employee_id: "1001".to_string(),
                username: "test-user".to_string(),
                display_name: "Test User".to_string(),
                email: email.map(str::to_string),
                mobile: mobile.map(str::to_string),
                telephone: telephone.map(str::to_string),
                organizational_unit_id: "ou-root".to_string(),
                status,
            }],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "not-used-for-login".to_string(),
            password_verifier: password_verifier("UserPass123!"),
        })
        .await
        .unwrap();
}

async fn seed_oidc_client(repository: &Repository) -> OAuthClientRecord {
    let client = OAuthClientRecord {
        client_id: OIDC_CLIENT_ID.to_string(),
        name: "OIDC Contract Client".to_string(),
        client_type: OAuthClientType::Web,
        client_secret_hash: Some("not-used-by-userinfo".to_string()),
        redirect_uris: vec!["https://client.example.test/callback".to_string()],
        allowed_scopes: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "phone".to_string(),
        ],
        enabled: true,
    };
    repository
        .create_oauth_client(client.clone())
        .await
        .unwrap()
        .unwrap()
}

fn test_oidc_service() -> OidcService {
    OidcService::new(
        OidcConfig::new(TEST_OIDC_ISSUER, TEST_OIDC_PRIVATE_KEY.to_vec(), false).unwrap(),
    )
    .unwrap()
}

fn signed_access_token(oidc: &OidcService, claims: &AccessTokenClaims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(oidc.key_id().to_string());
    let private_key =
        RsaPrivateKey::from_pkcs8_pem(std::str::from_utf8(TEST_OIDC_PRIVATE_KEY).unwrap()).unwrap();
    let private_key_der = private_key.to_pkcs1_der().unwrap();
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_der(private_key_der.as_bytes()),
    )
    .unwrap()
}

fn corrupt_signature(token: &str) -> String {
    let mut parts = token.split('.').map(str::to_string).collect::<Vec<_>>();
    assert_eq!(parts.len(), 3);
    let replacement = if parts[2].starts_with('A') { "B" } else { "A" };
    parts[2].replace_range(..1, replacement);
    parts.join(".")
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn assert_invalid_token(response: Response<Body>) {
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.headers()["www-authenticate"],
        "Bearer error=\"invalid_token\""
    );
    assert_eq!(
        json_body(response).await,
        json!({ "error": "invalid_token" })
    );
}

async fn login_token(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            &UserLoginRequest {
                username: "test-user".to_string(),
                password: "UserPass123!".to_string(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn create_web_client(app: &axum::Router, name: &str) -> Value {
    admin_json(
        app,
        Method::POST,
        "/api/admin/oauth-clients",
        Some(&web_client_request(name)),
    )
    .await
}

fn web_client_request(name: &str) -> Value {
    json!({
        "name": name,
        "client_type": "web",
        "redirect_uris": ["https://client.example.com/callback"],
        "allowed_scopes": ["openid", "profile"],
        "enabled": true
    })
}

async fn admin_json(app: &axum::Router, method: Method, uri: &str, value: Option<&Value>) -> Value {
    let request = match value {
        Some(value) => admin_json_request(method, uri, value),
        None => admin_request(method, uri),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn json_body(response: Response<Body>) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn admin_request(method: Method, uri: &str) -> Request<Body> {
    auth_request(method, uri, MANAGEMENT_TOKEN)
}

fn auth_request(method: Method, uri: &str, token: &str) -> Request<Body> {
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

fn admin_json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    json_request_with_token(method, uri, MANAGEMENT_TOKEN, value)
}

fn json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn json_request_with_token<T: serde::Serialize>(
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

fn sha256_token(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}

fn password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:test-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("test-verifier:v1:{}", hex::encode(hasher.finalize()))
}

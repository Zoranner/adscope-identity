use std::time::{SystemTime, UNIX_EPOCH};

use adscope_center::{
    AppState, build_router,
    oidc::{AccessTokenClaims, OidcService, config::OidcConfig},
};
use adscope_protocol::{OrganizationalUnit, UserLoginRequest, UserStatus};
use adscope_store::{
    OAuthClientRecord, OAuthClientType, Repository, UserCredentialInput, UserDirectoryPatch,
};
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode, header},
};
use axum_extra::extract::cookie::Cookie;
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, decode_header, encode};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, pkcs8::DecodePrivateKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const MANAGEMENT_TOKEN: &str = "test-management-token";
const MANAGEMENT_CSRF_TOKEN: &str = "test-management-csrf-token";
const TEST_ENCRYPTION_KEY: &str = "test-password-encryption-key";
const TEST_OIDC_ISSUER: &str = "https://center.example.test";
const TEST_OIDC_PRIVATE_KEY: &[u8] = include_bytes!("fixtures/oidc-private-key.pem");
const OIDC_CLIENT_ID: &str = "client_oidc_contract";
const OIDC_REDIRECT_URI: &str = "https://client.example.test/callback?source=adss";
const OIDC_STATE: &str = "state-original";
const OIDC_NONCE: &str = "nonce-original";
const OIDC_CODE_CHALLENGE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const TOKEN_WEB_CLIENT_ID: &str = "client_token_web";
const TOKEN_WEB_SECRET: &str = "token-web-secret";
const TOKEN_DESKTOP_CLIENT_ID: &str = "client_token_desktop";
const TOKEN_DESKTOP_REDIRECT_URI: &str = "http://127.0.0.1:51000/callback";
const TOKEN_CODE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const TOKEN_CODE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

struct TestApp {
    app: axum::Router,
    repository: Repository,
    oidc: OidcService,
}

#[tokio::test]
async fn token_endpoint_exchanges_web_code_and_issues_bound_rs256_tokens() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user_profile(
        &repository,
        Some("user@example.test"),
        Some("13800000000"),
        None,
        UserStatus::Active,
    )
    .await;
    seed_token_client(&repository, OAuthClientType::Web).await;
    seed_token_code(
        &repository,
        "web-code",
        TOKEN_WEB_CLIENT_ID,
        OIDC_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;

    let response = app
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "web-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();

    assert_token_headers(&response, StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 300);
    assert_eq!(body["scope"], "openid profile email phone");
    assert!(body.get("refresh_token").is_none());
    let access_token = body["access_token"].as_str().unwrap();
    let id_token = body["id_token"].as_str().unwrap();
    for token in [access_token, id_token] {
        let header = decode_header(token).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some(oidc.key_id()));
    }
    let access_claims = oidc.verify_access_token(access_token).unwrap();
    assert_eq!(access_claims.sub, "1001");
    assert_eq!(access_claims.client_id, TOKEN_WEB_CLIENT_ID);
    assert_eq!(access_claims.scope, "openid profile email phone");
    assert_eq!(
        access_claims.aud,
        format!("{TEST_OIDC_ISSUER}/oauth2/userinfo")
    );
    assert_eq!(access_claims.exp - access_claims.iat, 300);
    let id_claims = oidc.verify_id_token(id_token, TOKEN_WEB_CLIENT_ID).unwrap();
    assert_eq!(id_claims.sub, "1001");
    assert_eq!(id_claims.nonce, OIDC_NONCE);
    assert_eq!(id_claims.auth_time, 1_700_000_000);
    assert_eq!(id_claims.exp - id_claims.iat, 300);
    let id_payload = jwt_payload(id_token);
    assert_eq!(id_payload["preferred_username"], "test-user");
    assert_eq!(id_payload["name"], "Test User");
    assert_eq!(id_payload["email"], "user@example.test");
    assert_eq!(id_payload["phone_number"], "13800000000");
}

#[tokio::test]
async fn token_endpoint_exchanges_public_desktop_code_without_client_secret() {
    let TestApp {
        app,
        repository,
        oidc,
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;
    seed_token_code(
        &repository,
        "desktop-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;

    let response = app
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "desktop-code",
            None,
        ))
        .await
        .unwrap();

    assert_token_headers(&response, StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 300);
    assert_eq!(body["scope"], "openid profile email phone");
    let id_token = body["id_token"].as_str().unwrap();
    oidc.verify_id_token(id_token, TOKEN_DESKTOP_CLIENT_ID)
        .unwrap();
    let id_payload = jwt_payload(id_token);
    assert!(id_payload.get("email").is_none());
    assert!(id_payload.get("phone_number").is_none());
}

#[tokio::test]
async fn token_endpoint_rejects_request_shape_and_unsupported_grant_without_consuming_code() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;

    let invalid_shapes = [
        format!(
            "grant_type=authorization_code&client_id={TOKEN_DESKTOP_CLIENT_ID}&redirect_uri={TOKEN_DESKTOP_REDIRECT_URI}&code=shape-code&code_verifier={TOKEN_CODE_VERIFIER}&extra=value"
        ),
        format!(
            "grant_type=authorization_code&client_id={TOKEN_DESKTOP_CLIENT_ID}&client_id=duplicate&redirect_uri={TOKEN_DESKTOP_REDIRECT_URI}&code=shape-code&code_verifier={TOKEN_CODE_VERIFIER}"
        ),
        format!(
            "grant_type=authorization_code&client_id={TOKEN_DESKTOP_CLIENT_ID}&redirect_uri={TOKEN_DESKTOP_REDIRECT_URI}&code=shape-code&code_verifier=%FF"
        ),
    ];
    for body in invalid_shapes {
        let response = app
            .clone()
            .oneshot(raw_token_request(
                Body::from(body),
                "application/x-www-form-urlencoded",
                None,
            ))
            .await
            .unwrap();
        assert_token_error(response, StatusCode::BAD_REQUEST, "invalid_request", None).await;
    }
    let wrong_content_type = app
        .clone()
        .oneshot(raw_token_request(
            Body::from("{}"),
            "application/json",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        wrong_content_type,
        StatusCode::BAD_REQUEST,
        "invalid_request",
        None,
    )
    .await;
    let oversized = app
        .clone()
        .oneshot(raw_token_request(
            Body::from("x".repeat(16 * 1024 + 1)),
            "application/x-www-form-urlencoded",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        oversized,
        StatusCode::PAYLOAD_TOO_LARGE,
        "invalid_request",
        None,
    )
    .await;

    seed_token_code(
        &repository,
        "grant-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let unsupported = app
        .clone()
        .oneshot(raw_token_request(
            Body::from(token_form(
                "refresh_token",
                TOKEN_DESKTOP_CLIENT_ID,
                TOKEN_DESKTOP_REDIRECT_URI,
                "grant-code",
                TOKEN_CODE_VERIFIER,
            )),
            "application/x-www-form-urlencoded",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        unsupported,
        StatusCode::BAD_REQUEST,
        "unsupported_grant_type",
        None,
    )
    .await;
    let success = app
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "grant-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_headers(&success, StatusCode::OK);
}

#[tokio::test]
async fn token_endpoint_requires_client_authentication_before_grant_validation() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Web).await;

    for authorization in [
        None,
        Some(web_basic(TOKEN_WEB_CLIENT_ID, "wrong-secret")),
        Some("Basic !!!".to_string()),
    ] {
        let response = app
            .clone()
            .oneshot(raw_token_request(
                Body::from(token_form(
                    "refresh_token",
                    TOKEN_WEB_CLIENT_ID,
                    OIDC_REDIRECT_URI,
                    "unknown-priority-code",
                    TOKEN_CODE_VERIFIER,
                )),
                "application/x-www-form-urlencoded",
                authorization,
            ))
            .await
            .unwrap();
        assert_token_error(
            response,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            Some("Basic"),
        )
        .await;
    }

    seed_token_code(
        &repository,
        "unsupported-web-code",
        TOKEN_WEB_CLIENT_ID,
        OIDC_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let unsupported = app
        .clone()
        .oneshot(raw_token_request(
            Body::from(token_form(
                "refresh_token",
                TOKEN_WEB_CLIENT_ID,
                OIDC_REDIRECT_URI,
                "unsupported-web-code",
                TOKEN_CODE_VERIFIER,
            )),
            "application/x-www-form-urlencoded",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_error(
        unsupported,
        StatusCode::BAD_REQUEST,
        "unsupported_grant_type",
        None,
    )
    .await;
    let success = app
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "unsupported-web-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_headers(&success, StatusCode::OK);
}

#[tokio::test]
async fn token_endpoint_rejects_unknown_and_authenticates_disabled_clients() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_token_client(&repository, OAuthClientType::Web).await;

    let unknown = app
        .clone()
        .oneshot(raw_token_request(
            Body::from(token_form(
                "refresh_token",
                "client_unknown",
                OIDC_REDIRECT_URI,
                "unknown-code",
                TOKEN_CODE_VERIFIER,
            )),
            "application/x-www-form-urlencoded",
            Some(web_basic("client_unknown", "unknown-secret")),
        ))
        .await
        .unwrap();
    assert_token_error(
        unknown,
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        Some("Basic"),
    )
    .await;

    let mut client = repository
        .get_oauth_client(TOKEN_WEB_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    client.enabled = false;
    repository
        .update_oauth_client(client.clone())
        .await
        .unwrap()
        .unwrap();
    let missing_auth = app
        .clone()
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "unknown-disabled-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        missing_auth,
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        Some("Basic"),
    )
    .await;

    seed_token_code(
        &repository,
        "disabled-client-priority-code",
        TOKEN_WEB_CLIENT_ID,
        OIDC_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let unsupported = app
        .clone()
        .oneshot(raw_token_request(
            Body::from(token_form(
                "refresh_token",
                TOKEN_WEB_CLIENT_ID,
                OIDC_REDIRECT_URI,
                "disabled-client-priority-code",
                TOKEN_CODE_VERIFIER,
            )),
            "application/x-www-form-urlencoded",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_error(
        unsupported,
        StatusCode::BAD_REQUEST,
        "unsupported_grant_type",
        None,
    )
    .await;
    let disabled = app
        .clone()
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "disabled-client-priority-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_error(disabled, StatusCode::BAD_REQUEST, "invalid_grant", None).await;

    client.enabled = true;
    repository
        .update_oauth_client(client)
        .await
        .unwrap()
        .unwrap();
    let retry = app
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "disabled-client-priority-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_error(retry, StatusCode::BAD_REQUEST, "invalid_grant", None).await;
}

#[tokio::test]
async fn token_endpoint_returns_controlled_json_for_non_post_methods() {
    let TestApp { app, .. } = test_app().await;
    for method in [Method::GET, Method::PUT, Method::PATCH, Method::DELETE] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/oauth2/token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_token_error(
            response,
            StatusCode::METHOD_NOT_ALLOWED,
            "invalid_request",
            None,
        )
        .await;
    }
}

#[tokio::test]
async fn token_endpoint_structurally_validates_one_content_type_header() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;
    seed_token_code(
        &repository,
        "charset-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;

    let charset = app
        .clone()
        .oneshot(raw_token_request(
            Body::from(token_form(
                "authorization_code",
                TOKEN_DESKTOP_CLIENT_ID,
                TOKEN_DESKTOP_REDIRECT_URI,
                "charset-code",
                TOKEN_CODE_VERIFIER,
            )),
            "application/x-www-form-urlencoded; charset=UTF-8",
            None,
        ))
        .await
        .unwrap();
    assert_token_headers(&charset, StatusCode::OK);

    let form = token_form(
        "authorization_code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        "content-type-code",
        TOKEN_CODE_VERIFIER,
    );
    for content_type in [
        "application/x-www-form-urlencoded; charset",
        "application/x-www-form-urlencoded-extra",
    ] {
        let response = app
            .clone()
            .oneshot(raw_token_request(
                Body::from(form.clone()),
                content_type,
                None,
            ))
            .await
            .unwrap();
        assert_token_error(response, StatusCode::BAD_REQUEST, "invalid_request", None).await;
    }
    let duplicate = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/oauth2/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(
                    header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded; charset=UTF-8",
                )
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_token_error(duplicate, StatusCode::BAD_REQUEST, "invalid_request", None).await;
}

#[tokio::test]
async fn token_endpoint_enforces_web_basic_and_rejects_desktop_authentication() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Web).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;

    let malformed_basic = [
        "Basic !!!".to_string(),
        format!("Basic {}", STANDARD.encode([0xff, b':', b'x'])),
        format!("Basic {}", STANDARD.encode("missing-colon")),
        format!("Basic {}", STANDARD.encode(":secret")),
        format!("Basic {}", STANDARD.encode("client:")),
    ];
    for (index, authorization) in malformed_basic.into_iter().enumerate() {
        let code = format!("malformed-basic-{index}");
        seed_token_code(
            &repository,
            &code,
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            unix_seconds() as i64 + 120,
        )
        .await;
        let response = app
            .clone()
            .oneshot(token_request(
                TOKEN_WEB_CLIENT_ID,
                OIDC_REDIRECT_URI,
                &code,
                Some(authorization),
            ))
            .await
            .unwrap();
        assert_token_error(
            response,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            Some("Basic"),
        )
        .await;
    }

    for (code, authorization) in [
        ("missing-basic", None),
        (
            "wrong-secret",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, "wrong-secret")),
        ),
        (
            "wrong-basic-client",
            Some(web_basic("other-client", TOKEN_WEB_SECRET)),
        ),
    ] {
        seed_token_code(
            &repository,
            code,
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            unix_seconds() as i64 + 120,
        )
        .await;
        let response = app
            .clone()
            .oneshot(token_request(
                TOKEN_WEB_CLIENT_ID,
                OIDC_REDIRECT_URI,
                code,
                authorization,
            ))
            .await
            .unwrap();
        assert_token_error(
            response,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            Some("Basic"),
        )
        .await;
    }

    seed_token_code(
        &repository,
        "desktop-basic",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let desktop_basic = app
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "desktop-basic",
            Some(web_basic(TOKEN_DESKTOP_CLIENT_ID, "not-allowed")),
        ))
        .await
        .unwrap();
    assert_token_error(
        desktop_basic,
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        Some("Basic"),
    )
    .await;
}

#[tokio::test]
async fn token_endpoint_authenticates_web_before_grant_result_and_burns_failed_code() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Web).await;

    let missing_auth_unknown_code = app
        .clone()
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "unknown-web-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        missing_auth_unknown_code,
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        Some("Basic"),
    )
    .await;

    seed_token_code(
        &repository,
        "failed-auth-code",
        TOKEN_WEB_CLIENT_ID,
        OIDC_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let failed_auth = app
        .clone()
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "failed-auth-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, "wrong-secret")),
        ))
        .await
        .unwrap();
    assert_token_error(
        failed_auth,
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        Some("Basic"),
    )
    .await;
    let retry = app
        .oneshot(token_request(
            TOKEN_WEB_CLIENT_ID,
            OIDC_REDIRECT_URI,
            "failed-auth-code",
            Some(web_basic(TOKEN_WEB_CLIENT_ID, TOKEN_WEB_SECRET)),
        ))
        .await
        .unwrap();
    assert_token_error(retry, StatusCode::BAD_REQUEST, "invalid_grant", None).await;
}

#[tokio::test]
async fn token_endpoint_maps_grant_failures_and_consumes_failed_redemptions() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;

    let now = unix_seconds() as i64;
    for (
        code,
        client_id,
        redirect_uri,
        verifier,
        expires_at,
        expected_status,
        expected_error,
        authenticate,
    ) in [
        (
            "expired-code",
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            TOKEN_CODE_VERIFIER,
            now,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            None,
        ),
        (
            "wrong-client-code",
            "other-client",
            TOKEN_DESKTOP_REDIRECT_URI,
            TOKEN_CODE_VERIFIER,
            now + 120,
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            Some("Basic"),
        ),
        (
            "wrong-redirect-code",
            TOKEN_DESKTOP_CLIENT_ID,
            "http://127.0.0.1:52000/other",
            TOKEN_CODE_VERIFIER,
            now + 120,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            None,
        ),
        (
            "wrong-pkce-code",
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            &"x".repeat(43),
            now + 120,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            None,
        ),
    ] {
        seed_token_code(
            &repository,
            code,
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            expires_at,
        )
        .await;
        let response = app
            .clone()
            .oneshot(raw_token_request(
                Body::from(token_form(
                    "authorization_code",
                    client_id,
                    redirect_uri,
                    code,
                    verifier,
                )),
                "application/x-www-form-urlencoded",
                None,
            ))
            .await
            .unwrap();
        assert_token_error(response, expected_status, expected_error, authenticate).await;
    }

    seed_token_code(
        &repository,
        "destroyed-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        now + 120,
    )
    .await;
    let wrong_redirect = app
        .clone()
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            "http://127.0.0.1:52000/callback",
            "destroyed-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        wrong_redirect,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        None,
    )
    .await;
    let retry = app
        .clone()
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "destroyed-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(retry, StatusCode::BAD_REQUEST, "invalid_grant", None).await;

    let missing = app
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "unknown-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(missing, StatusCode::BAD_REQUEST, "invalid_grant", None).await;
}

#[tokio::test]
async fn token_endpoint_rechecks_client_and_user_state_after_code_issue() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;

    seed_token_code(
        &repository,
        "disabled-client-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let mut client = repository
        .get_oauth_client(TOKEN_DESKTOP_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    client.enabled = false;
    repository
        .update_oauth_client(client.clone())
        .await
        .unwrap()
        .unwrap();
    let disabled_client = app
        .clone()
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "disabled-client-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        disabled_client,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        None,
    )
    .await;

    client.enabled = true;
    repository
        .update_oauth_client(client)
        .await
        .unwrap()
        .unwrap();
    seed_user_profile(&repository, None, None, None, UserStatus::Disabled).await;
    seed_token_code(
        &repository,
        "disabled-user-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;
    let disabled_user = app
        .oneshot(token_request(
            TOKEN_DESKTOP_CLIENT_ID,
            TOKEN_DESKTOP_REDIRECT_URI,
            "disabled-user-code",
            None,
        ))
        .await
        .unwrap();
    assert_token_error(
        disabled_user,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        None,
    )
    .await;
}

#[tokio::test]
async fn token_endpoint_allows_only_one_concurrent_exchange() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;
    seed_token_code(
        &repository,
        "concurrent-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
    )
    .await;

    let first = app.clone().oneshot(token_request(
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        "concurrent-code",
        None,
    ));
    let second = app.oneshot(token_request(
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        "concurrent-code",
        None,
    ));
    let (first, second) = tokio::join!(first, second);
    let first = first.unwrap();
    let second = second.unwrap();
    let statuses = [first.status(), second.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let failure = if first.status() == StatusCode::BAD_REQUEST {
        first
    } else {
        second
    };
    assert_token_error(failure, StatusCode::BAD_REQUEST, "invalid_grant", None).await;
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

#[tokio::test]
async fn login_cookie_matches_json_token_and_logout_expires_it() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;

    let login = app
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
    assert_eq!(login.status(), StatusCode::OK);
    let set_cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    let cookie_token = session_cookie_value(&set_cookie);
    let body = json_body(login).await;
    assert_eq!(body["access_token"], cookie_token);
    for attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=3600",
    ] {
        assert!(
            set_cookie.contains(attribute),
            "missing {attribute}: {set_cookie}"
        );
    }

    let cookie_only_me = app
        .clone()
        .oneshot(cookie_request(Method::GET, "/api/me", &cookie_token))
        .await
        .unwrap();
    assert_eq!(cookie_only_me.status(), StatusCode::UNAUTHORIZED);

    let logout = app
        .clone()
        .oneshot(cookie_request(
            Method::POST,
            "/api/auth/logout",
            &cookie_token,
        ))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let cleared = logout.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cleared.starts_with("adscope_sso="));
    assert!(cleared.contains("HttpOnly"));
    assert!(cleared.contains("Secure"));
    assert!(cleared.contains("SameSite=Lax"));
    assert!(cleared.contains("Path=/"));
    assert!(cleared.contains("Max-Age=0"));
    let cleared_cookie = Cookie::parse(cleared.to_string()).unwrap();
    assert_eq!(cleared_cookie.max_age().unwrap().whole_seconds(), 0);
    assert!(cleared_cookie.expires_datetime().unwrap().unix_timestamp() < unix_seconds() as i64);

    let after_browser_clear = app
        .oneshot(empty_request(Method::GET, &authorization_uri()))
        .await
        .unwrap();
    assert_eq!(after_browser_clear.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        local_url(
            after_browser_clear.headers()[header::LOCATION]
                .to_str()
                .unwrap()
        )
        .path(),
        "/login"
    );
}

#[tokio::test]
async fn authorization_rejects_untrusted_redirects_and_redirects_trusted_protocol_errors() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_oidc_client(&repository).await;

    for uri in [
        authorization_uri_with(
            "client_missing",
            "https://evil.example/callback",
            "openid",
            None,
        ),
        authorization_uri_with(
            OIDC_CLIENT_ID,
            "https://evil.example/callback",
            "openid",
            None,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(empty_request(Method::GET, &uri))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response.headers().get(header::LOCATION).is_none());
    }

    let invalid_scope = app
        .clone()
        .oneshot(empty_request(
            Method::GET,
            &authorization_uri_with(OIDC_CLIENT_ID, OIDC_REDIRECT_URI, "openid groups", None),
        ))
        .await
        .unwrap();
    assert_redirect_error(invalid_scope, "invalid_scope", Some(OIDC_STATE));

    let prompt_none = app
        .clone()
        .oneshot(empty_request(
            Method::GET,
            &authorization_uri_with(OIDC_CLIENT_ID, OIDC_REDIRECT_URI, "openid", Some("none")),
        ))
        .await
        .unwrap();
    assert_redirect_error(prompt_none, "interaction_required", Some(OIDC_STATE));

    for (uri, error) in [
        (
            authorization_uri().replace("response_type=code", "response_type=token"),
            "unsupported_response_type",
        ),
        (
            authorization_uri().replace("response_mode=query", "response_mode=fragment"),
            "invalid_request",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(empty_request(Method::GET, &uri))
            .await
            .unwrap();
        assert_redirect_error(response, error, Some(OIDC_STATE));
    }

    for uri in [
        format!("{}&scope=openid", authorization_uri()),
        format!("{}&unknown={}", authorization_uri(), "x".repeat(16 * 1024)),
        authorization_uri().replace("state=state-original", "state=%FF"),
    ] {
        let response = app
            .clone()
            .oneshot(empty_request(Method::GET, &uri))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response.headers().get(header::LOCATION).is_none());
    }

    let mut client = repository
        .get_oauth_client(OIDC_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    client.allowed_scopes = vec!["openid".to_string()];
    repository
        .update_oauth_client(client.clone())
        .await
        .unwrap();
    let disallowed_scope = app
        .clone()
        .oneshot(empty_request(
            Method::GET,
            &authorization_uri_with(OIDC_CLIENT_ID, OIDC_REDIRECT_URI, "openid profile", None),
        ))
        .await
        .unwrap();
    assert_redirect_error(disallowed_scope, "invalid_scope", Some(OIDC_STATE));

    client.enabled = false;
    repository.update_oauth_client(client).await.unwrap();
    let disabled = app
        .oneshot(empty_request(Method::GET, &authorization_uri()))
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::BAD_REQUEST);
    assert!(disabled.headers().get(header::LOCATION).is_none());
}

#[tokio::test]
async fn authorization_routes_only_to_server_rebuilt_internal_pages() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;
    let authorization = authorization_uri();

    let anonymous = app
        .clone()
        .oneshot(empty_request(Method::GET, &authorization))
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::SEE_OTHER);
    let login_location = anonymous.headers()[header::LOCATION].to_str().unwrap();
    let login_url = local_url(login_location);
    assert_eq!(login_url.path(), "/login");
    let continue_value = query_value(&login_url, "continue").unwrap();
    assert_eq!(continue_value, authorization);
    assert!(continue_value.starts_with("/oauth2/authorize?"));

    let cookie = login_cookie(&app).await;
    let authenticated = app
        .clone()
        .oneshot(cookie_request(Method::GET, &authorization, &cookie))
        .await
        .unwrap();
    assert_eq!(authenticated.status(), StatusCode::SEE_OTHER);
    let authorize_location = authenticated.headers()[header::LOCATION].to_str().unwrap();
    assert_eq!(
        authorize_location,
        authorization.replacen("/oauth2/authorize", "/authorize", 1)
    );

    seed_user_profile(&repository, None, None, None, UserStatus::Disabled).await;
    let inactive = app
        .clone()
        .oneshot(cookie_request(Method::GET, &authorization, &cookie))
        .await
        .unwrap();
    assert_eq!(inactive.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        local_url(inactive.headers()[header::LOCATION].to_str().unwrap()).path(),
        "/login"
    );

    let injected = format!("{authorization}&return_to=https%3A%2F%2Fevil.example%2F");
    let response = app
        .oneshot(empty_request(Method::GET, &injected))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get(header::LOCATION).is_none());
}

#[tokio::test]
async fn authorization_context_requires_session_and_returns_confirmed_claims_and_csrf() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user_profile(
        &repository,
        Some("user@example.test"),
        Some("13800000000"),
        None,
        UserStatus::Active,
    )
    .await;
    seed_oidc_client(&repository).await;
    let context_uri = format!(
        "/api/oauth2/authorize/context?{}",
        authorization_uri().split_once('?').unwrap().1
    );

    let anonymous = app
        .clone()
        .oneshot(empty_request(Method::GET, &context_uri))
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(anonymous).await["error"], "invalid_session");

    let cookie = login_cookie(&app).await;
    let prompt_none_context = format!(
        "/api/oauth2/authorize/context?{}",
        authorization_uri_with(OIDC_CLIENT_ID, OIDC_REDIRECT_URI, "openid", Some("none"))
            .split_once('?')
            .unwrap()
            .1
    );
    let prompt_none = app
        .clone()
        .oneshot(cookie_request(Method::GET, &prompt_none_context, &cookie))
        .await
        .unwrap();
    assert_redirect_error(prompt_none, "interaction_required", Some(OIDC_STATE));
    let response = app
        .clone()
        .oneshot(cookie_request(Method::GET, &context_uri, &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["client_name"], "OIDC Contract Client");
    assert_eq!(body["user"]["employee_id"], "1001");
    assert_eq!(body["user"]["username"], "test-user");
    assert_eq!(body["user"]["display_name"], "Test User");
    assert_eq!(body["claims"]["sub"], "1001");
    assert_eq!(body["claims"]["preferred_username"], "test-user");
    assert_eq!(body["claims"]["name"], "Test User");
    assert_eq!(body["claims"]["email"], "user@example.test");
    assert_eq!(body["claims"]["phone_number"], "13800000000");
    assert!(
        body["csrf_token"]
            .as_str()
            .unwrap()
            .starts_with("adss-csrf:v1.")
    );
    assert_eq!(body["authorization"]["redirect_uri"], OIDC_REDIRECT_URI);
    assert_eq!(body["authorization"]["state"], OIDC_STATE);

    seed_user_profile(&repository, None, None, None, UserStatus::Disabled).await;
    let disabled_user = app
        .clone()
        .oneshot(cookie_request(Method::GET, &context_uri, &cookie))
        .await
        .unwrap();
    assert_eq!(disabled_user.status(), StatusCode::UNAUTHORIZED);

    seed_user(&repository).await;
    let mut client = repository
        .get_oauth_client(OIDC_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    client.enabled = false;
    repository
        .update_oauth_client(client)
        .await
        .unwrap()
        .unwrap();
    let disabled_client = app
        .oneshot(cookie_request(Method::GET, &context_uri, &cookie))
        .await
        .unwrap();
    assert_eq!(disabled_client.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_approve_stores_hash_bound_record_and_allows_new_confirmations() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;
    let cookie = login_cookie(&app).await;

    let first_csrf = authorization_csrf(&app, &cookie).await;
    let first = app
        .clone()
        .oneshot(form_request(
            "/oauth2/authorize",
            &authorization_form("approve", &first_csrf),
            &cookie,
        ))
        .await
        .unwrap();
    let first_code = approved_code(first);
    let stored = repository
        .consume_authorization_code(&sha256_token(&first_code), unix_seconds() as i64)
        .await
        .unwrap()
        .unwrap();
    assert!(
        repository
            .consume_authorization_code(&sha256_token(&first_code), unix_seconds() as i64)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(stored.client_id, OIDC_CLIENT_ID);
    assert_eq!(stored.employee_id, "1001");
    assert_eq!(stored.redirect_uri, OIDC_REDIRECT_URI);
    assert_eq!(stored.scopes, ["openid", "profile", "email", "phone"]);
    assert_eq!(stored.nonce, OIDC_NONCE);
    assert_eq!(stored.code_challenge, OIDC_CODE_CHALLENGE);
    assert!(stored.auth_time <= unix_seconds() as i64);
    assert!((1..=120).contains(&(stored.expires_at - unix_seconds() as i64)));

    let second_csrf = authorization_csrf(&app, &cookie).await;
    let second = app
        .oneshot(form_request(
            "/oauth2/authorize",
            &authorization_form("approve", &second_csrf),
            &cookie,
        ))
        .await
        .unwrap();
    let second_code = approved_code(second);
    assert_ne!(first_code, second_code);
}

#[tokio::test]
async fn authorization_cancel_and_invalid_confirmations_never_return_codes() {
    let TestApp {
        app, repository, ..
    } = test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;
    let cookie = login_cookie(&app).await;
    let csrf = authorization_csrf(&app, &cookie).await;

    let cancel = app
        .clone()
        .oneshot(form_request(
            "/oauth2/authorize",
            &authorization_form("cancel", &csrf),
            &cookie,
        ))
        .await
        .unwrap();
    assert_redirect_error(cancel, "access_denied", Some(OIDC_STATE));

    let mut tampered_csrf = authorization_form("approve", &format!("{csrf}x"));
    let mut tampered_request = authorization_form("approve", &csrf);
    tampered_request = tampered_request.replace("nonce=nonce-original", "nonce=nonce-tampered");
    let wrong_decision = authorization_form("later", &csrf);
    let duplicate = format!("{}&decision=approve", authorization_form("approve", &csrf));
    for form in [
        &tampered_csrf,
        &tampered_request,
        &wrong_decision,
        &duplicate,
    ] {
        let response = app
            .clone()
            .oneshot(form_request("/oauth2/authorize", form, &cookie))
            .await
            .unwrap();
        assert!(response.status().is_client_error() || response.status().is_redirection());
        assert!(
            response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .is_none_or(|location| local_or_absolute_query_value(location, "code").is_none())
        );
    }
    tampered_csrf.push_str(&"x".repeat(16 * 1024));
    let oversized = app
        .oneshot(form_request("/oauth2/authorize", &tampered_csrf, &cookie))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

fn authorization_uri() -> String {
    authorization_uri_with(
        OIDC_CLIENT_ID,
        OIDC_REDIRECT_URI,
        "openid profile email phone",
        None,
    )
}

fn authorization_uri_with(
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    prompt: Option<&str>,
) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope)
        .append_pair("state", OIDC_STATE)
        .append_pair("nonce", OIDC_NONCE)
        .append_pair("code_challenge", OIDC_CODE_CHALLENGE)
        .append_pair("code_challenge_method", "S256")
        .append_pair("response_mode", "query");
    if let Some(prompt) = prompt {
        serializer.append_pair("prompt", prompt);
    }
    format!("/oauth2/authorize?{}", serializer.finish())
}

fn authorization_form(decision: &str, csrf_token: &str) -> String {
    let query = authorization_uri();
    let mut form = query.split_once('?').unwrap().1.to_string();
    form.push('&');
    form.push_str(
        &url::form_urlencoded::Serializer::new(String::new())
            .append_pair("decision", decision)
            .append_pair("csrf_token", csrf_token)
            .finish(),
    );
    form
}

fn form_request(uri: &str, form: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, format!("adscope_sso={cookie}"))
        .body(Body::from(form.to_string()))
        .unwrap()
}

fn token_request(
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    authorization: Option<String>,
) -> Request<Body> {
    raw_token_request(
        Body::from(token_form(
            "authorization_code",
            client_id,
            redirect_uri,
            code,
            TOKEN_CODE_VERIFIER,
        )),
        "application/x-www-form-urlencoded",
        authorization,
    )
}

fn raw_token_request(
    body: Body,
    content_type: &str,
    authorization: Option<String>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/oauth2/token")
        .header(header::CONTENT_TYPE, content_type);
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    request.body(body).unwrap()
}

fn token_form(
    grant_type: &str,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", grant_type)
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code", code)
        .append_pair("code_verifier", code_verifier)
        .finish()
}

fn web_basic(client_id: &str, secret: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{client_id}:{secret}")))
}

fn jwt_payload(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

fn assert_token_headers(response: &Response<Body>, expected_status: StatusCode) {
    assert_eq!(response.status(), expected_status);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::PRAGMA], "no-cache");
    assert!(response.headers().get(header::LOCATION).is_none());
}

async fn assert_token_error(
    response: Response<Body>,
    expected_status: StatusCode,
    expected_error: &str,
    authenticate: Option<&str>,
) {
    assert_token_headers(&response, expected_status);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        authenticate
    );
    assert_eq!(
        json_body(response).await,
        json!({ "error": expected_error })
    );
}

fn cookie_request(method: Method, uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("adscope_sso={cookie}"))
        .body(Body::empty())
        .unwrap()
}

fn session_cookie_value(set_cookie: &str) -> String {
    let cookie = Cookie::parse_encoded(set_cookie.to_string()).unwrap();
    assert_eq!(cookie.name(), "adscope_sso");
    cookie.value().to_string()
}

async fn login_cookie(app: &axum::Router) -> String {
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
    session_cookie_value(response.headers()[header::SET_COOKIE].to_str().unwrap())
}

async fn authorization_csrf(app: &axum::Router, cookie: &str) -> String {
    let context_uri = format!(
        "/api/oauth2/authorize/context?{}",
        authorization_uri().split_once('?').unwrap().1
    );
    let response = app
        .clone()
        .oneshot(cookie_request(Method::GET, &context_uri, cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["csrf_token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn approved_code(response: Response<Body>) -> String {
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    let url = url::Url::parse(location).unwrap();
    assert_eq!(
        url.as_str().split('?').next().unwrap(),
        "https://client.example.test/callback"
    );
    assert_eq!(query_value(&url, "source").as_deref(), Some("adss"));
    assert_eq!(query_value(&url, "state").as_deref(), Some(OIDC_STATE));
    query_value(&url, "code").unwrap()
}

fn assert_redirect_error(response: Response<Body>, error: &str, state: Option<&str>) {
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    let url = url::Url::parse(location).unwrap();
    assert_eq!(query_value(&url, "error").as_deref(), Some(error));
    assert_eq!(query_value(&url, "state").as_deref(), state);
    assert!(query_value(&url, "code").is_none());
}

fn local_url(location: &str) -> url::Url {
    url::Url::parse("https://center.example.test")
        .unwrap()
        .join(location)
        .unwrap()
}

fn local_or_absolute_query_value(location: &str, name: &str) -> Option<String> {
    query_value(&local_url(location), name)
}

fn query_value(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
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

async fn seed_token_client(repository: &Repository, client_type: OAuthClientType) {
    let (client_id, client_secret_hash, redirect_uris) = match client_type {
        OAuthClientType::Web => (
            TOKEN_WEB_CLIENT_ID,
            Some(sha256_token(TOKEN_WEB_SECRET)),
            vec![OIDC_REDIRECT_URI.to_string()],
        ),
        OAuthClientType::Desktop => (
            TOKEN_DESKTOP_CLIENT_ID,
            None,
            vec!["http://127.0.0.1:41000/callback".to_string()],
        ),
    };
    repository
        .create_oauth_client(OAuthClientRecord {
            client_id: client_id.to_string(),
            name: format!("Token {client_type:?} Client"),
            client_type,
            client_secret_hash,
            redirect_uris,
            allowed_scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ],
            enabled: true,
        })
        .await
        .unwrap()
        .unwrap();
}

async fn seed_token_code(
    repository: &Repository,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    expires_at: i64,
) {
    repository
        .store_authorization_code(adscope_store::AuthorizationCodeRecord {
            code_hash: sha256_token(code),
            client_id: client_id.to_string(),
            employee_id: "1001".to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ],
            nonce: OIDC_NONCE.to_string(),
            code_challenge: TOKEN_CODE_CHALLENGE.to_string(),
            auth_time: 1_700_000_000,
            expires_at,
        })
        .await
        .unwrap();
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
        redirect_uris: vec![OIDC_REDIRECT_URI.to_string()],
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
    Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", management_session_cookie())
        .header("x-adscope-csrf-token", MANAGEMENT_CSRF_TOKEN)
        .body(Body::empty())
        .unwrap()
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
    let signed = format!("adss-management-session:v1.{payload}");
    let mut key_derivation =
        <Hmac<Sha256> as Mac>::new_from_slice(MANAGEMENT_TOKEN.as_bytes()).unwrap();
    key_derivation.update(b"adscope:management-session:v1");
    let key = key_derivation.finalize().into_bytes();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).unwrap();
    mac.update(signed.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("adscope_management={signed}.{signature}")
}

fn json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn sha256_token(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}

fn password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adscope:test-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("test-verifier:v1:{}", hex::encode(hasher.finalize()))
}

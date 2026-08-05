use adss_protocol::UserStatus;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;

use crate::state::AppState;

const PUBLIC_METADATA_CACHE_CONTROL: &str = "public, max-age=300";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        .route("/oauth2/jwks", get(jwks))
        .route("/oauth2/userinfo", get(userinfo))
}

async fn openid_configuration(
    State(state): State<AppState>,
) -> (HeaderMap, Json<OpenIdConfiguration>) {
    let issuer = state.oidc.config().issuer();
    public_metadata_json(OpenIdConfiguration {
        issuer: issuer.to_string(),
        authorization_endpoint: format!("{issuer}/oauth2/authorize"),
        token_endpoint: format!("{issuer}/oauth2/token"),
        userinfo_endpoint: format!("{issuer}/oauth2/userinfo"),
        jwks_uri: format!("{issuer}/oauth2/jwks"),
        response_types_supported: ["code"],
        grant_types_supported: ["authorization_code"],
        subject_types_supported: ["public"],
        id_token_signing_alg_values_supported: ["RS256"],
        scopes_supported: ["openid", "profile", "email", "phone"],
        token_endpoint_auth_methods_supported: ["client_secret_basic", "none"],
        code_challenge_methods_supported: ["S256"],
    })
}

async fn jwks(State(state): State<AppState>) -> impl IntoResponse {
    public_metadata_json(state.oidc.jwks().clone())
}

async fn userinfo(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<UserInfoResponse>), OidcRouteError> {
    let token = bearer_token(&headers).ok_or(OidcRouteError::InvalidToken)?;
    let claims = state
        .oidc
        .verify_access_token(token)
        .map_err(|_| OidcRouteError::InvalidToken)?;
    let client = state
        .repository
        .get_oauth_client(&claims.client_id)
        .await
        .map_err(|_| OidcRouteError::ServerError)?
        .ok_or(OidcRouteError::InvalidToken)?;
    if !client.enabled || client.client_id != claims.client_id {
        return Err(OidcRouteError::InvalidToken);
    }
    let user = state
        .repository
        .get_user(&claims.sub)
        .await
        .map_err(|_| OidcRouteError::ServerError)?
        .ok_or(OidcRouteError::InvalidToken)?;
    if user.status != UserStatus::Active {
        return Err(OidcRouteError::InvalidToken);
    }

    let scopes = claims.scope.split(' ').collect::<Vec<_>>();
    let profile = scopes.contains(&"profile");
    let email = if scopes.contains(&"email") {
        non_empty(user.email.as_deref())
    } else {
        None
    };
    let phone_number = if scopes.contains(&"phone") {
        non_empty(user.mobile.as_deref()).or_else(|| non_empty(user.telephone.as_deref()))
    } else {
        None
    };
    Ok(no_store_json(UserInfoResponse {
        sub: user.employee_id,
        preferred_username: profile.then_some(user.username),
        name: profile.then_some(user.display_name),
        email,
        phone_number,
    }))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn public_metadata_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PUBLIC_METADATA_CACHE_CONTROL),
    );
    (headers, Json(value))
}

fn no_store_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, Json(value))
}

#[derive(Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    jwks_uri: String,
    response_types_supported: [&'static str; 1],
    grant_types_supported: [&'static str; 1],
    subject_types_supported: [&'static str; 1],
    id_token_signing_alg_values_supported: [&'static str; 1],
    scopes_supported: [&'static str; 4],
    token_endpoint_auth_methods_supported: [&'static str; 2],
    code_challenge_methods_supported: [&'static str; 1],
}

#[derive(Serialize)]
struct UserInfoResponse {
    sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phone_number: Option<String>,
}

#[derive(Clone, Copy)]
enum OidcRouteError {
    InvalidToken,
    ServerError,
}

impl IntoResponse for OidcRouteError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            Self::ServerError => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        };
        let mut response = (status, Json(OidcErrorResponse { error })).into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if matches!(self, Self::InvalidToken) {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer error=\"invalid_token\""),
            );
        }
        response
    }
}

#[derive(Serialize)]
struct OidcErrorResponse {
    error: &'static str,
}

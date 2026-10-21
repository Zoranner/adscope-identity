use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware,
    response::Response,
    routing::post,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::{auth::constant_time_eq, error::ApiError, state::AppState};

use super::admin::{authorize_management, authorize_management_write};

pub(super) const MANAGEMENT_COOKIE: &str = "adscope_management";
pub(super) const MANAGEMENT_CSRF_HEADER: &str = "x-adscope-csrf-token";

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/session",
            post(create_session).get(get_session).delete(delete_session),
        )
        .route_layer(middleware::map_response(no_store))
}

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn get_session(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<
    (
        [(header::HeaderName, &'static str); 1],
        Json<ManagementSessionResponse>,
    ),
    ApiError,
> {
    let session = authorize_management(&headers, &state)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ManagementSessionResponse {
            csrf_token: session.csrf_nonce,
        }),
    ))
}

async fn delete_session(
    headers: HeaderMap,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<
    (
        CookieJar,
        ([(header::HeaderName, &'static str); 1], StatusCode),
    ),
    ApiError,
> {
    authorize_management_write(&headers, &state)?;
    let cookie = management_cookie(String::new(), 0)?;
    Ok((
        jar.remove(cookie),
        (
            [(header::CACHE_CONTROL, "no-store")],
            StatusCode::NO_CONTENT,
        ),
    ))
}

async fn create_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<ManagementSessionRequest>,
) -> Result<
    (
        CookieJar,
        (
            [(header::HeaderName, &'static str); 1],
            Json<ManagementSessionResponse>,
        ),
    ),
    ApiError,
> {
    if !constant_time_eq(request.token.as_bytes(), state.management_token.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }

    let token = state
        .management_sessions
        .issue()
        .map_err(|_| ApiError::Persistence)?;
    let session = state
        .management_sessions
        .verify(&token)
        .ok_or(ApiError::Persistence)?;
    let cookie = management_cookie(token, state.management_sessions.ttl_seconds())?;

    Ok((
        jar.add(cookie),
        (
            [(header::CACHE_CONTROL, "no-store")],
            Json(ManagementSessionResponse {
                csrf_token: session.csrf_nonce,
            }),
        ),
    ))
}

fn management_cookie(value: String, ttl_seconds: u64) -> Result<Cookie<'static>, ApiError> {
    let max_age = std::time::Duration::from_secs(ttl_seconds)
        .try_into()
        .map_err(|_| ApiError::Persistence)?;
    Ok(Cookie::build((MANAGEMENT_COOKIE, value))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .path("/api/admin")
        .max_age(max_age)
        .build())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagementSessionRequest {
    token: String,
}

#[derive(Debug, Serialize)]
struct ManagementSessionResponse {
    csrf_token: String,
}

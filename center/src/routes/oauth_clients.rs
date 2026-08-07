use adss_store::{OAuthClientRecord, OAuthClientType};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    oidc::{
        crypto::{random_urlsafe, sha256_token},
        validation::{
            validate_client_id, validate_client_name, validate_redirect_uris, validate_scopes,
        },
    },
    state::AppState,
};

use super::admin::{authorize_management, authorize_management_write};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/oauth-clients",
            get(list_oauth_clients).post(create_oauth_client),
        )
        .route(
            "/admin/oauth-clients/{client_id}",
            patch(update_oauth_client).delete(delete_oauth_client),
        )
        .route(
            "/admin/oauth-clients/{client_id}/secret",
            post(regenerate_oauth_client_secret),
        )
}

async fn list_oauth_clients(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<OAuthClientListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let clients = state
        .repository
        .list_oauth_clients()
        .await
        .map_err(|_| ApiError::Persistence)?
        .into_iter()
        .map(OAuthClientResponse::from)
        .collect();
    Ok(Json(OAuthClientListResponse { clients }))
}

async fn create_oauth_client(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateOAuthClientRequest>,
) -> Result<(HeaderMap, Json<OAuthClientCreateResponse>), OAuthClientApiError> {
    authorize_management_write(&headers, &state)?;
    validate_client_fields(
        &request.name,
        request.client_type,
        &request.redirect_uris,
        &request.allowed_scopes,
        state.oidc.config().allow_insecure_web_loopback_redirects(),
    )?;

    let client_id = format!(
        "client_{}",
        random_urlsafe(32).map_err(|_| OAuthClientApiError::Internal)?
    );
    validate_client_id(&client_id).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    let client_secret = match request.client_type {
        OAuthClientType::Web => {
            Some(random_urlsafe(32).map_err(|_| OAuthClientApiError::Internal)?)
        }
        OAuthClientType::Desktop => None,
    };
    let client = state
        .repository
        .create_oauth_client(OAuthClientRecord {
            client_id,
            name: request.name,
            client_type: request.client_type,
            client_secret_hash: client_secret.as_ref().map(sha256_token),
            redirect_uris: request.redirect_uris,
            allowed_scopes: request.allowed_scopes,
            enabled: request.enabled,
        })
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Conflict)?;

    Ok(no_store_json(OAuthClientCreateResponse {
        client: OAuthClientResponse::from(client),
        client_secret,
    }))
}

async fn update_oauth_client(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<UpdateOAuthClientRequest>,
) -> Result<Json<OAuthClientResponse>, OAuthClientApiError> {
    authorize_management_write(&headers, &state)?;
    validate_client_id(&client_id).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    let existing = state
        .repository
        .get_oauth_client(&client_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    validate_client_fields(
        &request.name,
        existing.client_type,
        &request.redirect_uris,
        &request.allowed_scopes,
        state.oidc.config().allow_insecure_web_loopback_redirects(),
    )?;
    ensure_secret_invariant(existing.client_type, &existing.client_secret_hash)?;

    let updated = state
        .repository
        .update_oauth_client(OAuthClientRecord {
            client_id: existing.client_id,
            name: request.name,
            client_type: existing.client_type,
            client_secret_hash: existing.client_secret_hash,
            redirect_uris: request.redirect_uris,
            allowed_scopes: request.allowed_scopes,
            enabled: request.enabled,
        })
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(OAuthClientResponse::from(updated)))
}

async fn delete_oauth_client(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Result<StatusCode, OAuthClientApiError> {
    authorize_management_write(&headers, &state)?;
    validate_client_id(&client_id).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    if !state
        .repository
        .delete_oauth_client(&client_id)
        .await
        .map_err(|_| ApiError::Persistence)?
    {
        return Err(ApiError::NotFound.into());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn regenerate_oauth_client_secret(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Result<(HeaderMap, Json<OAuthClientSecretResponse>), OAuthClientApiError> {
    authorize_management_write(&headers, &state)?;
    validate_client_id(&client_id).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    let mut client = state
        .repository
        .get_oauth_client(&client_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    if client.client_type != OAuthClientType::Web {
        return Err(OAuthClientApiError::InvalidClientType);
    }

    let client_secret = random_urlsafe(32).map_err(|_| OAuthClientApiError::Internal)?;
    client.client_secret_hash = Some(sha256_token(&client_secret));
    state
        .repository
        .update_oauth_client(client)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(no_store_json(OAuthClientSecretResponse {
        client_id,
        client_secret,
    }))
}

fn validate_client_fields(
    name: &str,
    client_type: OAuthClientType,
    redirect_uris: &[String],
    allowed_scopes: &[String],
    allow_insecure_web_loopback_redirects: bool,
) -> Result<(), OAuthClientApiError> {
    validate_client_name(name).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    if !(1..=4).contains(&allowed_scopes.len())
        || allowed_scopes
            .iter()
            .any(|scope| scope.is_empty() || scope.chars().any(char::is_whitespace))
    {
        return Err(OAuthClientApiError::InvalidRequest);
    }
    validate_scopes(&allowed_scopes.join(" ")).map_err(|_| OAuthClientApiError::InvalidRequest)?;
    validate_redirect_uris(
        client_type,
        redirect_uris,
        allow_insecure_web_loopback_redirects,
    )
    .map_err(|_| OAuthClientApiError::InvalidRequest)?;
    Ok(())
}

fn ensure_secret_invariant(
    client_type: OAuthClientType,
    client_secret_hash: &Option<String>,
) -> Result<(), OAuthClientApiError> {
    match (client_type, client_secret_hash.is_some()) {
        (OAuthClientType::Web, true) | (OAuthClientType::Desktop, false) => Ok(()),
        _ => Err(OAuthClientApiError::Internal),
    }
}

fn no_store_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, Json(value))
}

#[derive(Debug)]
enum OAuthClientApiError {
    Api(ApiError),
    InvalidRequest,
    InvalidClientType,
    Internal,
}

impl From<ApiError> for OAuthClientApiError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl IntoResponse for OAuthClientApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(error) => error.into_response(),
            Self::InvalidRequest => StatusCode::BAD_REQUEST.into_response(),
            Self::InvalidClientType => StatusCode::CONFLICT.into_response(),
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Debug, Serialize)]
struct OAuthClientResponse {
    client_id: String,
    name: String,
    client_type: OAuthClientType,
    redirect_uris: Vec<String>,
    allowed_scopes: Vec<String>,
    enabled: bool,
}

impl From<OAuthClientRecord> for OAuthClientResponse {
    fn from(client: OAuthClientRecord) -> Self {
        Self {
            client_id: client.client_id,
            name: client.name,
            client_type: client.client_type,
            redirect_uris: client.redirect_uris,
            allowed_scopes: client.allowed_scopes,
            enabled: client.enabled,
        }
    }
}

#[derive(Debug, Serialize)]
struct OAuthClientListResponse {
    clients: Vec<OAuthClientResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateOAuthClientRequest {
    name: String,
    client_type: OAuthClientType,
    redirect_uris: Vec<String>,
    allowed_scopes: Vec<String>,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateOAuthClientRequest {
    name: String,
    redirect_uris: Vec<String>,
    allowed_scopes: Vec<String>,
    enabled: bool,
}

#[derive(Serialize)]
struct OAuthClientCreateResponse {
    client: OAuthClientResponse,
    client_secret: Option<String>,
}

#[derive(Serialize)]
struct OAuthClientSecretResponse {
    client_id: String,
    client_secret: String,
}

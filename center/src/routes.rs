mod admin;
mod oauth_clients;

use adss_protocol::{
    ConnectorConfirmRequest, ConnectorConfirmResponse, ConnectorSyncRequest, ConnectorSyncResponse,
    CredentialBatch, CredentialEntry, DomainDirectoryConfig, PasswordChangeRequest,
    PasswordChangeResponse, SyncChannel, User, UserLoginRequest, UserLoginResponse, UserStatus,
};
use adss_store::{CredentialCiphertextBatch, UserContactPatch, UserCredentialInput};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{authorize_domain_connector, connector_key_from_headers},
    error::ApiError,
    oidc,
    password::PasswordEncryption,
    state::AppState,
    web,
};

pub fn build_router(state: AppState) -> Router {
    build_router_with_web_root(state, web::default_web_root())
}

pub fn build_router_with_web_root(
    state: AppState,
    web_root: impl Into<std::path::PathBuf>,
) -> Router {
    Router::new()
        .nest("/api", api_routes())
        .merge(oidc::routes::routes())
        .fallback_service(web::static_service(web_root))
        .with_state(state)
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/auth/login", post(login))
        .route("/me", get(get_me))
        .route("/me/contact", patch(update_me_contact))
        .route("/me/password", post(change_me_password))
        .merge(admin::routes())
        .merge(oauth_clients::routes())
        .route("/connector/sync", post(connector_sync))
        .route("/connector/confirm", post(connector_confirm))
        .fallback(api_not_found)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match state.repository.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ok" })),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unavailable",
            }),
        ),
    }
}

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<UserLoginRequest>,
) -> Result<Json<UserLoginResponse>, ApiError> {
    let user = state
        .repository
        .get_user_by_username(&request.username)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;
    let credential = state
        .repository
        .get_credential_record(&user.employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;

    if !state
        .password_hash
        .verify(&request.password, &credential.password_verifier)
    {
        return Err(ApiError::Unauthorized);
    }

    Ok(Json(UserLoginResponse {
        employee_id: user.employee_id,
        access_token: state
            .user_sessions
            .issue(&credential.employee_id)
            .map_err(|_| ApiError::Persistence)?,
    }))
}

async fn get_me(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<UserProfileResponse>, ApiError> {
    let employee_id = authorize_user_session(&headers, &state)?;
    let user = state
        .repository
        .get_user(&employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;

    Ok(Json(UserProfileResponse::from(user)))
}

async fn update_me_contact(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<UserContactPatchRequest>,
) -> Result<Json<UserContactUpdateResponse>, ApiError> {
    let employee_id = authorize_user_session(&headers, &state)?;
    let (user, directory_revision) = state
        .repository
        .update_user_contact(
            &employee_id,
            UserContactPatch {
                email: request.email,
                mobile: request.mobile,
                telephone: request.telephone,
            },
        )
        .await
        .map_err(|_| ApiError::Persistence)?;

    Ok(Json(UserContactUpdateResponse {
        profile: UserProfileResponse::from(user),
        directory_revision,
    }))
}

async fn change_me_password(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<PasswordChangeRequest>,
) -> Result<Json<PasswordChangeResponse>, ApiError> {
    let employee_id = authorize_user_session(&headers, &state)?;
    change_password_by_employee_id(&state, employee_id, request).await
}

async fn change_password_by_employee_id(
    state: &AppState,
    employee_id: String,
    request: PasswordChangeRequest,
) -> Result<Json<PasswordChangeResponse>, ApiError> {
    let credential = state
        .repository
        .get_credential_record(&employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;

    if !state
        .password_hash
        .verify(&request.current_password, &credential.password_verifier)
    {
        return Err(ApiError::Unauthorized);
    }

    let credential_revision = state
        .repository
        .change_user_password(UserCredentialInput {
            employee_id: employee_id.clone(),
            password_ciphertext: state
                .password_encryption
                .seal(&request.new_password)
                .map_err(|_| ApiError::Persistence)?,
            password_verifier: state
                .password_hash
                .hash(&request.new_password)
                .map_err(|_| ApiError::Persistence)?,
        })
        .await
        .map_err(|_| ApiError::Persistence)?;

    Ok(Json(PasswordChangeResponse {
        employee_id,
        credential_revision,
    }))
}

async fn connector_sync(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<ConnectorSyncRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let domain = authorize_domain_connector(
        &state.repository,
        &request.domain_id,
        connector_key_from_headers(&headers),
    )
    .await?;

    let directory = state
        .repository
        .list_directory_changed_after(
            &request.domain_id,
            request.applied_directory_revision,
            request.rebuild_directory,
            state.batch_limit,
        )
        .await
        .map_err(|_| ApiError::Persistence)?;
    let credential_ciphertexts = state
        .repository
        .list_credentials_changed_after(
            &request.domain_id,
            request.applied_credential_revision,
            request.rebuild_credentials,
            state.batch_limit,
        )
        .await
        .map_err(|_| ApiError::Persistence)?;
    let credentials = open_credential_batch_for_connector(
        credential_ciphertexts,
        state.password_encryption.as_ref(),
    )?;

    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ConnectorSyncResponse {
            directory,
            credentials,
            directory_config: DomainDirectoryConfig {
                domain_id: domain.id,
                mirror_root_dn: domain.mirror_root_dn,
                quarantine_ou_dn: domain.quarantine_ou_dn,
                upn_suffix: domain.upn_suffix,
                employee_id_attribute: domain.employee_id_attribute,
                managed_group_id_attribute: domain.managed_group_id_attribute,
            },
        }),
    ))
}

async fn connector_confirm(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<ConnectorConfirmRequest>,
) -> Result<Json<ConnectorConfirmResponse>, ApiError> {
    authorize_domain_connector(
        &state.repository,
        &request.domain_id,
        connector_key_from_headers(&headers),
    )
    .await?;

    if request.success {
        match request.channel {
            SyncChannel::Directory => {
                state
                    .repository
                    .confirm_directory_revision(&request.domain_id, request.target_revision)
                    .await
                    .map_err(|_| ApiError::Persistence)?;
            }
            SyncChannel::Credential => {
                state
                    .repository
                    .confirm_credential_revision(&request.domain_id, request.target_revision)
                    .await
                    .map_err(|_| ApiError::Persistence)?;
            }
        }
    }

    Ok(Json(ConnectorConfirmResponse { accepted: true }))
}

fn open_credential_batch_for_connector(
    batch: CredentialCiphertextBatch,
    password_encryption: &dyn PasswordEncryption,
) -> Result<CredentialBatch, ApiError> {
    let credentials = batch
        .credentials
        .into_iter()
        .map(|credential| {
            Ok(CredentialEntry {
                employee_id: credential.employee_id,
                plaintext_password: open_password_for_connector(
                    &credential.password_ciphertext,
                    password_encryption,
                )
                .ok_or(ApiError::Persistence)?,
                status: credential.status,
                changed_revision: credential.changed_revision,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    Ok(CredentialBatch {
        server_revision: batch.server_revision,
        batch_revision: batch.batch_revision,
        credentials,
        has_more: batch.has_more,
    })
}

fn open_password_for_connector(
    ciphertext: &str,
    password_encryption: &dyn PasswordEncryption,
) -> Option<String> {
    password_encryption.open(ciphertext).ok()
}

fn authorize_user_session(headers: &HeaderMap, state: &AppState) -> Result<String, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    state
        .user_sessions
        .verify(token)
        .map(|session| session.employee_id)
        .ok_or(ApiError::Unauthorized)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct UserProfileResponse {
    employee_id: String,
    username: String,
    display_name: String,
    email: Option<String>,
    mobile: Option<String>,
    telephone: Option<String>,
    organizational_unit_id: String,
    status: UserStatus,
}

impl From<User> for UserProfileResponse {
    fn from(user: User) -> Self {
        Self {
            employee_id: user.employee_id,
            username: user.username,
            display_name: user.display_name,
            email: user.email,
            mobile: user.mobile,
            telephone: user.telephone,
            organizational_unit_id: user.organizational_unit_id,
            status: user.status,
        }
    }
}

#[derive(Debug, Deserialize)]
struct UserContactPatchRequest {
    email: Option<String>,
    mobile: Option<String>,
    telephone: Option<String>,
}

#[derive(Debug, Serialize)]
struct UserContactUpdateResponse {
    profile: UserProfileResponse,
    directory_revision: u64,
}

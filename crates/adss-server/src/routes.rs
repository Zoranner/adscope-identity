use adss_contract::{
    AgentConfirmRequest, AgentConfirmResponse, AgentSyncRequest, AgentSyncResponse,
    CredentialBatch, CredentialEntry, DomainDirectoryConfig, PasswordChangeRequest,
    PasswordChangeResponse, SyncChannel, User, UserLoginRequest, UserLoginResponse, UserStatus,
};
use adss_persistence::{
    CredentialCiphertextBatch, UserContactPatch, UserCredentialInput, UserDirectoryPatch,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{agent_key_from_headers, authorize_domain_agent},
    error::ApiError,
    password::PasswordEnvelope,
    state::AppState,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/me", get(get_me))
        .route("/api/me/contact", patch(update_me_contact))
        .route("/api/me/password", post(change_me_password))
        .route("/api/users/{employee_id}", patch(update_user))
        .route("/api/users/{employee_id}/password", post(change_password))
        .route("/api/agent/sync", post(agent_sync))
        .route("/api/agent/confirm", post(agent_confirm))
        .with_state(state)
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<UserLoginRequest>,
) -> Result<Json<UserLoginResponse>, ApiError> {
    let credential = state
        .repository
        .get_credential_record(&request.employee_id)
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
        employee_id: request.employee_id,
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

async fn update_user(
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<UpdateUserDirectoryRequest>,
) -> Result<Json<DirectoryUpdateResponse>, ApiError> {
    let revision = state
        .repository
        .upsert_directory(
            Vec::new(),
            vec![UserDirectoryPatch {
                employee_id: employee_id.clone(),
                username: request.username,
                display_name: request.display_name,
                email: request.email,
                mobile: request.mobile,
                telephone: request.telephone,
                organizational_unit_id: request.organizational_unit_id,
                status: request.status,
            }],
            Vec::new(),
        )
        .await
        .map_err(|_| ApiError::Persistence)?;

    Ok(Json(DirectoryUpdateResponse {
        employee_id,
        directory_revision: revision,
    }))
}

async fn change_password(
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<PasswordChangeRequest>,
) -> Result<Json<PasswordChangeResponse>, ApiError> {
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
                .password_envelope
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

async fn agent_sync(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<AgentSyncRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let domain = authorize_domain_agent(
        &state.repository,
        &request.domain_id,
        agent_key_from_headers(&headers),
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
    let credentials =
        open_credential_batch_for_agent(credential_ciphertexts, state.password_envelope.as_ref())?;

    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(AgentSyncResponse {
            directory,
            credentials,
            directory_config: DomainDirectoryConfig {
                domain_id: domain.id,
                mirror_root_dn: domain.mirror_root_dn,
                quarantine_ou_dn: domain.quarantine_ou_dn,
                upn_suffix: domain.upn_suffix,
                employee_id_attribute: domain.employee_id_attribute,
            },
        }),
    ))
}

async fn agent_confirm(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<AgentConfirmRequest>,
) -> Result<Json<AgentConfirmResponse>, ApiError> {
    authorize_domain_agent(
        &state.repository,
        &request.domain_id,
        agent_key_from_headers(&headers),
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

    Ok(Json(AgentConfirmResponse { accepted: true }))
}

fn open_credential_batch_for_agent(
    batch: CredentialCiphertextBatch,
    password_envelope: &dyn PasswordEnvelope,
) -> Result<CredentialBatch, ApiError> {
    let credentials = batch
        .credentials
        .into_iter()
        .map(|credential| {
            Ok(CredentialEntry {
                employee_id: credential.employee_id,
                plaintext_password: open_password_for_agent(
                    &credential.password_ciphertext,
                    password_envelope,
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

fn open_password_for_agent(
    ciphertext: &str,
    password_envelope: &dyn PasswordEnvelope,
) -> Option<String> {
    password_envelope.open(ciphertext).ok()
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
        .ok_or(ApiError::Unauthorized)
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

#[derive(Debug, Deserialize)]
struct UpdateUserDirectoryRequest {
    username: String,
    display_name: String,
    email: Option<String>,
    mobile: Option<String>,
    telephone: Option<String>,
    organizational_unit_id: String,
    status: UserStatus,
}

#[derive(Debug, Serialize)]
struct DirectoryUpdateResponse {
    employee_id: String,
    directory_revision: u64,
}

use adss_protocol::{Group, OrganizationalUnit, PasswordChangeResponse, User, UserStatus};
use adss_store::{
    DomainPatch, DomainRecord, UserCredentialInput, UserDirectoryPatch, UserListFilter,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    routing::{get, patch, post, put},
};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{KeyInit, OsRng},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{connector_key_hash, constant_time_eq},
    error::ApiError,
    state::AppState,
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/domains", get(list_domains).post(create_domain))
        .route("/admin/domains/{domain_id}", patch(update_domain))
        .route("/admin/ous/tree", get(list_organizational_units))
        .route("/admin/ous", post(create_organizational_unit))
        .route("/admin/ous/{ou_id}", patch(update_organizational_unit))
        .route("/admin/users", get(list_users).post(create_user))
        .route(
            "/admin/users/{employee_id}",
            get(get_user).patch(update_user),
        )
        .route("/admin/users/{employee_id}/disable", post(disable_user))
        .route("/admin/users/{employee_id}/enable", post(enable_user))
        .route(
            "/admin/users/{employee_id}/password-reset",
            post(reset_user_password),
        )
        .route("/admin/groups", get(list_groups).post(create_group))
        .route(
            "/admin/groups/{group_id}",
            get(get_group).patch(update_group),
        )
        .route(
            "/admin/groups/{group_id}/members",
            put(replace_group_members),
        )
        .route("/admin/sync/domains", get(list_sync_domains))
}

async fn list_domains(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<DomainListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let domains = state
        .repository
        .list_domains()
        .await
        .map_err(|_| ApiError::Persistence)?
        .into_iter()
        .map(DomainResponse::from)
        .collect();

    Ok(Json(DomainListResponse { domains }))
}

async fn create_domain(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateDomainRequest>,
) -> Result<(HeaderMap, Json<DomainMutationResponse>), ApiError> {
    authorize_management(&headers, &state)?;
    let connector_key = generate_connector_key();
    let domain = state
        .repository
        .create_domain(DomainRecord {
            id: request.id,
            name: request.name,
            enabled: request.enabled,
            mirror_root_dn: request.mirror_root_dn,
            quarantine_ou_dn: request.quarantine_ou_dn,
            upn_suffix: request.upn_suffix,
            employee_id_attribute: request.employee_id_attribute,
            managed_group_id_attribute: request.managed_group_id_attribute,
            connector_key_hash: connector_key_hash(&connector_key),
            applied_directory_revision: 0,
            applied_credential_revision: 0,
        })
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Conflict)?;

    Ok(domain_mutation_response(domain, connector_key))
}

async fn update_domain(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(domain_id): Path<String>,
    Json(request): Json<DomainPatchRequest>,
) -> Result<(HeaderMap, Json<DomainMutationResponse>), ApiError> {
    authorize_management(&headers, &state)?;
    let connector_key = generate_connector_key();
    let domain = state
        .repository
        .update_domain(
            &domain_id,
            DomainPatch {
                name: Some(request.name),
                enabled: Some(request.enabled),
                mirror_root_dn: Some(request.mirror_root_dn),
                quarantine_ou_dn: Some(request.quarantine_ou_dn),
                upn_suffix: Some(request.upn_suffix),
                employee_id_attribute: Some(request.employee_id_attribute),
                managed_group_id_attribute: Some(request.managed_group_id_attribute),
                connector_key_hash: Some(connector_key_hash(&connector_key)),
            },
        )
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(domain_mutation_response(domain, connector_key))
}

async fn list_organizational_units(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<OrganizationalUnitListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let organizational_units = state
        .repository
        .list_organizational_units()
        .await
        .map_err(|_| ApiError::Persistence)?;

    Ok(Json(OrganizationalUnitListResponse {
        organizational_units,
    }))
}

async fn create_organizational_unit(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<OrganizationalUnitRequest>,
) -> Result<Json<OrganizationalUnitUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    write_organizational_unit(&state, request.id, request.name, request.parent_id).await
}

async fn update_organizational_unit(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(ou_id): Path<String>,
    Json(request): Json<OrganizationalUnitPatchRequest>,
) -> Result<Json<OrganizationalUnitUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    write_organizational_unit(&state, ou_id, request.name, request.parent_id).await
}

async fn list_users(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<UserListQuery>,
) -> Result<Json<UserListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let users = state
        .repository
        .list_users(UserListFilter {
            employee_id: query.employee_id,
            username: query.username,
            organizational_unit_id: query.organizational_unit_id,
            status: query.status,
        })
        .await
        .map_err(|_| ApiError::Persistence)?
        .into_iter()
        .map(UserProfileResponse::from)
        .collect();

    Ok(Json(UserListResponse { users }))
}

async fn create_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserCreateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let user_patch = UserDirectoryPatch {
        employee_id: request.employee_id.clone(),
        username: request.username,
        display_name: request.display_name,
        email: request.email,
        mobile: request.mobile,
        telephone: request.telephone,
        organizational_unit_id: request.organizational_unit_id,
        status: request.status,
    };
    let directory_revision = state
        .repository
        .upsert_directory(Vec::new(), vec![user_patch], Vec::new())
        .await
        .map_err(|_| ApiError::Persistence)?;
    let credential_revision =
        write_user_password(&state, &request.employee_id, &request.initial_password).await?;
    let user = state
        .repository
        .get_user(&request.employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(UserCreateResponse {
        user: UserProfileResponse::from(user),
        directory_revision,
        credential_revision,
    }))
}

async fn get_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
) -> Result<Json<UserProfileResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let user = state
        .repository
        .get_user(&employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(UserProfileResponse::from(user)))
}

async fn update_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<UpdateUserDirectoryRequest>,
) -> Result<Json<UserDirectoryUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let directory_revision = state
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
    let user = state
        .repository
        .get_user(&employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(UserDirectoryUpdateResponse {
        user: UserProfileResponse::from(user),
        directory_revision,
    }))
}

async fn disable_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
) -> Result<Json<UserDirectoryUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    set_user_status(&state, &employee_id, UserStatus::Disabled).await
}

async fn enable_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
) -> Result<Json<UserDirectoryUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    set_user_status(&state, &employee_id, UserStatus::Active).await
}

async fn reset_user_password(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<PasswordResetRequest>,
) -> Result<Json<PasswordChangeResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let credential_revision =
        write_user_password(&state, &employee_id, &request.new_password).await?;

    Ok(Json(PasswordChangeResponse {
        employee_id,
        credential_revision,
    }))
}

async fn list_groups(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<GroupListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let groups = state
        .repository
        .list_groups()
        .await
        .map_err(|_| ApiError::Persistence)?;

    Ok(Json(GroupListResponse { groups }))
}

async fn create_group(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<GroupCreateRequest>,
) -> Result<Json<GroupUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    write_group(
        &state,
        Group {
            id: request.id,
            name: request.name,
            organizational_unit_id: request.organizational_unit_id,
            member_employee_ids: Vec::new(),
            changed_revision: 0,
        },
    )
    .await
}

async fn get_group(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(group_id): Path<String>,
) -> Result<Json<Group>, ApiError> {
    authorize_management(&headers, &state)?;
    let group = state
        .repository
        .get_group(&group_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(group))
}

async fn update_group(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    Json(request): Json<GroupPatchRequest>,
) -> Result<Json<GroupUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let existing = state
        .repository
        .get_group(&group_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    write_group(
        &state,
        Group {
            id: existing.id,
            name: request.name,
            organizational_unit_id: request.organizational_unit_id,
            member_employee_ids: existing.member_employee_ids,
            changed_revision: 0,
        },
    )
    .await
}

async fn replace_group_members(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    Json(request): Json<GroupMembersRequest>,
) -> Result<Json<GroupUpdateResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let existing = state
        .repository
        .get_group(&group_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    write_group(
        &state,
        Group {
            id: existing.id,
            name: existing.name,
            organizational_unit_id: existing.organizational_unit_id,
            member_employee_ids: request.member_employee_ids,
            changed_revision: 0,
        },
    )
    .await
}

async fn list_sync_domains(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<SyncDomainListResponse>, ApiError> {
    authorize_management(&headers, &state)?;
    let current_directory_revision = state
        .repository
        .current_directory_revision()
        .await
        .map_err(|_| ApiError::Persistence)?;
    let current_credential_revision = state
        .repository
        .current_credential_revision()
        .await
        .map_err(|_| ApiError::Persistence)?;
    let domains = state
        .repository
        .list_domains()
        .await
        .map_err(|_| ApiError::Persistence)?
        .into_iter()
        .map(|domain| SyncDomainResponse {
            domain_id: domain.id,
            enabled: domain.enabled,
            applied_directory_revision: domain.applied_directory_revision,
            applied_credential_revision: domain.applied_credential_revision,
            directory_lag: current_directory_revision
                .saturating_sub(domain.applied_directory_revision),
            credential_lag: current_credential_revision
                .saturating_sub(domain.applied_credential_revision),
        })
        .collect();

    Ok(Json(SyncDomainListResponse { domains }))
}

async fn write_organizational_unit(
    state: &AppState,
    id: String,
    name: String,
    parent_id: Option<String>,
) -> Result<Json<OrganizationalUnitUpdateResponse>, ApiError> {
    let directory_revision = state
        .repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: id.clone(),
                name,
                parent_id,
                changed_revision: 0,
            }],
            Vec::new(),
            Vec::new(),
        )
        .await
        .map_err(|_| ApiError::Persistence)?;
    let organizational_unit = state
        .repository
        .get_organizational_unit(&id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(OrganizationalUnitUpdateResponse {
        organizational_unit,
        directory_revision,
    }))
}

async fn set_user_status(
    state: &AppState,
    employee_id: &str,
    status: UserStatus,
) -> Result<Json<UserDirectoryUpdateResponse>, ApiError> {
    let existing = state
        .repository
        .get_user(employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    let directory_revision = state
        .repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch_with_status(existing, status)],
            Vec::new(),
        )
        .await
        .map_err(|_| ApiError::Persistence)?;
    let user = state
        .repository
        .get_user(employee_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(UserDirectoryUpdateResponse {
        user: UserProfileResponse::from(user),
        directory_revision,
    }))
}

async fn write_group(
    state: &AppState,
    group: Group,
) -> Result<Json<GroupUpdateResponse>, ApiError> {
    let group_id = group.id.clone();
    let directory_revision = state
        .repository
        .upsert_directory(Vec::new(), Vec::new(), vec![group])
        .await
        .map_err(|_| ApiError::Persistence)?;
    let group = state
        .repository
        .get_group(&group_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(GroupUpdateResponse {
        group,
        directory_revision,
    }))
}

async fn write_user_password(
    state: &AppState,
    employee_id: &str,
    password: &str,
) -> Result<u64, ApiError> {
    state
        .repository
        .change_user_password(UserCredentialInput {
            employee_id: employee_id.to_string(),
            password_ciphertext: state
                .password_encryption
                .seal(password)
                .map_err(|_| ApiError::Persistence)?,
            password_verifier: state
                .password_hash
                .hash(password)
                .map_err(|_| ApiError::Persistence)?,
        })
        .await
        .map_err(|_| ApiError::Persistence)
}

fn user_patch_with_status(user: User, status: UserStatus) -> UserDirectoryPatch {
    UserDirectoryPatch {
        employee_id: user.employee_id,
        username: user.username,
        display_name: user.display_name,
        email: user.email,
        mobile: user.mobile,
        telephone: user.telephone,
        organizational_unit_id: user.organizational_unit_id,
        status,
    }
}

pub(super) fn authorize_management(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    if constant_time_eq(token.as_bytes(), state.management_token.as_bytes()) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

#[derive(Debug, Serialize)]
struct DomainResponse {
    id: String,
    name: String,
    enabled: bool,
    mirror_root_dn: String,
    quarantine_ou_dn: String,
    upn_suffix: String,
    employee_id_attribute: String,
    managed_group_id_attribute: String,
    applied_directory_revision: u64,
    applied_credential_revision: u64,
}

impl From<DomainRecord> for DomainResponse {
    fn from(domain: DomainRecord) -> Self {
        Self {
            id: domain.id,
            name: domain.name,
            enabled: domain.enabled,
            mirror_root_dn: domain.mirror_root_dn,
            quarantine_ou_dn: domain.quarantine_ou_dn,
            upn_suffix: domain.upn_suffix,
            employee_id_attribute: domain.employee_id_attribute,
            managed_group_id_attribute: domain.managed_group_id_attribute,
            applied_directory_revision: domain.applied_directory_revision,
            applied_credential_revision: domain.applied_credential_revision,
        }
    }
}

#[derive(Debug, Serialize)]
struct DomainListResponse {
    domains: Vec<DomainResponse>,
}

#[derive(Debug, Serialize)]
struct DomainMutationResponse {
    domain: DomainResponse,
    connector_key: String,
}

fn generate_connector_key() -> String {
    hex::encode(XChaCha20Poly1305::generate_key(&mut OsRng))
}

fn domain_mutation_response(
    domain: DomainRecord,
    connector_key: String,
) -> (HeaderMap, Json<DomainMutationResponse>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        headers,
        Json(DomainMutationResponse {
            domain: DomainResponse::from(domain),
            connector_key,
        }),
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateDomainRequest {
    id: String,
    name: String,
    enabled: bool,
    mirror_root_dn: String,
    quarantine_ou_dn: String,
    upn_suffix: String,
    employee_id_attribute: String,
    managed_group_id_attribute: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainPatchRequest {
    name: String,
    enabled: bool,
    mirror_root_dn: String,
    quarantine_ou_dn: String,
    upn_suffix: String,
    employee_id_attribute: String,
    managed_group_id_attribute: String,
}

#[derive(Debug, Deserialize)]
struct OrganizationalUnitRequest {
    id: String,
    name: String,
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrganizationalUnitPatchRequest {
    name: String,
    parent_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OrganizationalUnitListResponse {
    organizational_units: Vec<OrganizationalUnit>,
}

#[derive(Debug, Serialize)]
struct OrganizationalUnitUpdateResponse {
    organizational_unit: OrganizationalUnit,
    directory_revision: u64,
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
struct UserListQuery {
    employee_id: Option<String>,
    username: Option<String>,
    organizational_unit_id: Option<String>,
    status: Option<UserStatus>,
}

#[derive(Debug, Serialize)]
struct UserListResponse {
    users: Vec<UserProfileResponse>,
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    employee_id: String,
    username: String,
    display_name: String,
    email: Option<String>,
    mobile: Option<String>,
    telephone: Option<String>,
    organizational_unit_id: String,
    status: UserStatus,
    initial_password: String,
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
struct UserCreateResponse {
    user: UserProfileResponse,
    directory_revision: u64,
    credential_revision: u64,
}

#[derive(Debug, Serialize)]
struct UserDirectoryUpdateResponse {
    user: UserProfileResponse,
    directory_revision: u64,
}

#[derive(Debug, Deserialize)]
struct PasswordResetRequest {
    new_password: String,
}

#[derive(Debug, Deserialize)]
struct GroupCreateRequest {
    id: String,
    name: String,
    organizational_unit_id: String,
}

#[derive(Debug, Deserialize)]
struct GroupPatchRequest {
    name: String,
    organizational_unit_id: String,
}

#[derive(Debug, Deserialize)]
struct GroupMembersRequest {
    member_employee_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GroupListResponse {
    groups: Vec<Group>,
}

#[derive(Debug, Serialize)]
struct GroupUpdateResponse {
    group: Group,
    directory_revision: u64,
}

#[derive(Debug, Serialize)]
struct SyncDomainListResponse {
    domains: Vec<SyncDomainResponse>,
}

#[derive(Debug, Serialize)]
struct SyncDomainResponse {
    domain_id: String,
    enabled: bool,
    applied_directory_revision: u64,
    applied_credential_revision: u64,
    directory_lag: u64,
    credential_lag: u64,
}

use adss_contract::{
    AgentConfirmRequest, AgentConfirmResponse, AgentSyncRequest, AgentSyncResponse,
    CredentialBatch, CredentialEntry, DomainDirectoryConfig, PasswordChangeRequest,
    PasswordChangeResponse, SyncChannel, UserLoginRequest, UserLoginResponse, UserStatus,
};
use adss_persistence::{
    CredentialCiphertextBatch, Repository, UserCredentialInput, UserDirectoryPatch,
};
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, Version,
    password_hash::{
        PasswordHasher, PasswordVerifier as Argon2PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{patch, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    io::Write,
    path::Path as FsPath,
    process::{Command, Stdio},
    sync::Arc,
};

const AGENT_KEY_HEADER: &str = "x-adss-agent-key";
const PASSWORD_ENVELOPE_PROVIDER_ENV: &str = "ADSS_PASSWORD_ENVELOPE_PROVIDER";
const PASSWORD_ENVELOPE_LOCAL_KEY_ENV: &str = "ADSS_PASSWORD_ENVELOPE_LOCAL_KEY";
const PASSWORD_ENVELOPE_COMMAND_ENV: &str = "ADSS_PASSWORD_ENVELOPE_COMMAND";
const PASSWORD_HASH_PROVIDER_ENV: &str = "ADSS_PASSWORD_HASH_PROVIDER";
const DEFAULT_BATCH_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub database_url: Option<String>,
}

impl ServerConfig {
    pub fn from_bind_addr(bind_addr: Option<String>) -> Self {
        Self {
            bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
            database_url: None,
        }
    }

    pub fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("ADSS_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: std::env::var("ADSS_DATABASE_URL").ok(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    repository: Repository,
    batch_limit: usize,
    password_envelope: Arc<dyn PasswordEnvelope>,
    password_hash: Arc<dyn PasswordHashProvider>,
}

impl AppState {
    pub fn from_env(repository: Repository) -> anyhow::Result<Self> {
        Ok(Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            password_envelope_from_env()?,
            password_hash_from_env()?,
        ))
    }

    pub fn new_for_tests(repository: Repository, password_envelope_key: impl Into<String>) -> Self {
        Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            Arc::new(LocalPasswordEnvelope::new(password_envelope_key)),
            Arc::new(DeterministicPasswordHash),
        )
    }

    pub fn with_batch_limit_for_tests(
        repository: Repository,
        batch_limit: usize,
        password_envelope_key: impl Into<String>,
    ) -> Self {
        Self::with_password_providers(
            repository,
            batch_limit,
            Arc::new(LocalPasswordEnvelope::new(password_envelope_key)),
            Arc::new(DeterministicPasswordHash),
        )
    }

    fn with_password_providers(
        repository: Repository,
        batch_limit: usize,
        password_envelope: Arc<dyn PasswordEnvelope>,
        password_hash: Arc<dyn PasswordHashProvider>,
    ) -> Self {
        Self {
            repository,
            batch_limit: batch_limit.max(1),
            password_envelope,
            password_hash,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
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
    }))
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

async fn authorize_domain_agent(
    repository: &Repository,
    domain_id: &str,
    agent_key: Option<&str>,
) -> Result<adss_persistence::DomainRecord, ApiError> {
    let domain = repository
        .get_domain(domain_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;

    let Some(agent_key) = agent_key else {
        return Err(ApiError::Unauthorized);
    };

    let provided_hash = agent_key_hash(agent_key);
    if !constant_time_eq(domain.agent_key_hash.as_bytes(), provided_hash.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }

    if !domain.enabled {
        return Err(ApiError::Forbidden);
    }

    Ok(domain)
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

fn agent_key_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AGENT_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
}

fn agent_key_hash(agent_key: &str) -> String {
    format!("sha256:{}", sha256_hex(agent_key.as_bytes()))
}

fn open_password_for_agent(
    ciphertext: &str,
    password_envelope: &dyn PasswordEnvelope,
) -> Option<String> {
    password_envelope.open(ciphertext).ok()
}

trait PasswordEnvelope: Send + Sync {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String>;
    fn open(&self, ciphertext: &str) -> anyhow::Result<String>;
}

#[derive(Debug)]
struct LocalPasswordEnvelope {
    key: Vec<u8>,
}

impl LocalPasswordEnvelope {
    fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into().into_bytes(),
        }
    }
}

impl PasswordEnvelope for LocalPasswordEnvelope {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("local password envelope key must not be empty");
        }

        Ok(format!(
            "local-envelope:v1:{}",
            hex::encode(xor_with_password_stream(plaintext.as_bytes(), &self.key))
        ))
    }

    fn open(&self, ciphertext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("local password envelope key must not be empty");
        }

        let encoded = ciphertext
            .strip_prefix("local-envelope:v1:")
            .ok_or_else(|| anyhow::anyhow!("unsupported password envelope"))?;
        let ciphertext = hex::decode(encoded)?;
        Ok(String::from_utf8(xor_with_password_stream(
            &ciphertext,
            &self.key,
        ))?)
    }
}

#[derive(Debug)]
struct CommandPasswordEnvelope {
    command: String,
}

impl CommandPasswordEnvelope {
    fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    fn run(&self, action: &str, input: &str) -> anyhow::Result<String> {
        let mut child = Command::new(&self.command)
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("password envelope command stdin unavailable"))?;
        stdin.write_all(input.as_bytes())?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            anyhow::bail!("password envelope command failed");
        }

        let output = String::from_utf8(output.stdout)?;
        Ok(output.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl PasswordEnvelope for CommandPasswordEnvelope {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        self.run("seal", plaintext)
    }

    fn open(&self, ciphertext: &str) -> anyhow::Result<String> {
        self.run("open", ciphertext)
    }
}

fn password_envelope_from_env() -> anyhow::Result<Arc<dyn PasswordEnvelope>> {
    let provider = env::var(PASSWORD_ENVELOPE_PROVIDER_ENV)
        .map_err(|_| anyhow::anyhow!("{PASSWORD_ENVELOPE_PROVIDER_ENV} is required"))?;

    match provider.as_str() {
        "local" => {
            let key = env::var(PASSWORD_ENVELOPE_LOCAL_KEY_ENV)
                .map_err(|_| anyhow::anyhow!("{PASSWORD_ENVELOPE_LOCAL_KEY_ENV} is required"))?;
            if key.is_empty() {
                anyhow::bail!("{PASSWORD_ENVELOPE_LOCAL_KEY_ENV} must not be empty");
            }
            Ok(Arc::new(LocalPasswordEnvelope::new(key)))
        }
        "command" => {
            let command = env::var(PASSWORD_ENVELOPE_COMMAND_ENV)
                .map_err(|_| anyhow::anyhow!("{PASSWORD_ENVELOPE_COMMAND_ENV} is required"))?;
            if command.is_empty() {
                anyhow::bail!("{PASSWORD_ENVELOPE_COMMAND_ENV} must not be empty");
            }
            if !FsPath::new(&command).is_file() {
                anyhow::bail!("{PASSWORD_ENVELOPE_COMMAND_ENV} must point to a file");
            }
            Ok(Arc::new(CommandPasswordEnvelope::new(command)))
        }
        _ => anyhow::bail!("unsupported password envelope provider: {provider}"),
    }
}

trait PasswordHashProvider: Send + Sync {
    fn hash(&self, password: &str) -> anyhow::Result<String>;
    fn verify(&self, password: &str, verifier: &str) -> bool;
}

#[derive(Debug)]
struct Argon2idPasswordHash;

impl PasswordHashProvider for Argon2idPasswordHash {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(argon2id()
            .hash_password(password.as_bytes(), &salt)?
            .to_string())
    }

    fn verify(&self, password: &str, verifier: &str) -> bool {
        let Ok(parsed_hash) = PasswordHash::new(verifier) else {
            return false;
        };

        argon2id()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }
}

#[derive(Debug)]
struct DeterministicPasswordHash;

impl PasswordHashProvider for DeterministicPasswordHash {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        Ok(deterministic_password_verifier(password))
    }

    fn verify(&self, password: &str, verifier: &str) -> bool {
        constant_time_eq(
            deterministic_password_verifier(password).as_bytes(),
            verifier.as_bytes(),
        )
    }
}

fn password_hash_from_env() -> anyhow::Result<Arc<dyn PasswordHashProvider>> {
    let provider = env::var(PASSWORD_HASH_PROVIDER_ENV)
        .map_err(|_| anyhow::anyhow!("{PASSWORD_HASH_PROVIDER_ENV} is required"))?;

    match provider.as_str() {
        "argon2id" => Ok(Arc::new(Argon2idPasswordHash)),
        _ => anyhow::bail!("unsupported password hash provider: {provider}"),
    }
}

fn argon2id() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

fn deterministic_password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:test-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("test-verifier:v1:{}", hex::encode(hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();

    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        diff |= usize::from(left_byte ^ right_byte);
    }

    diff == 0
}

fn xor_with_password_stream(input: &[u8], password_envelope_key: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut counter = 0_u64;

    while output.len() < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(b"adss:local-password-envelope:v1");
        hasher.update(password_envelope_key);
        hasher.update(counter.to_be_bytes());
        let block = hasher.finalize();

        for byte in block {
            if output.len() == input.len() {
                break;
            }
            output.push(input[output.len()] ^ byte);
        }

        counter += 1;
    }

    output
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

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    Forbidden,
    Persistence,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::Persistence => StatusCode::INTERNAL_SERVER_ERROR,
        };
        status.into_response()
    }
}

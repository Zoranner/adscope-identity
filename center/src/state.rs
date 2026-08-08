use std::sync::Arc;

use adscope_store::Repository;

use crate::oidc::{OidcService, config::OidcConfig, crypto::CsrfSigner};
use crate::password::{
    BuiltInPasswordEncryption, DeterministicPasswordHash, PasswordEncryption, PasswordHashProvider,
    password_encryption_from_env, password_hash_from_env,
};
use crate::session::{ManagementSessionIssuer, UserSessionIssuer};

const DEFAULT_BATCH_LIMIT: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub(crate) repository: Repository,
    pub(crate) batch_limit: usize,
    pub(crate) password_encryption: Arc<dyn PasswordEncryption>,
    pub(crate) password_hash: Arc<dyn PasswordHashProvider>,
    pub(crate) user_sessions: UserSessionIssuer,
    pub(crate) csrf_signer: CsrfSigner,
    pub(crate) management_token: String,
    pub(crate) management_sessions: ManagementSessionIssuer,
    pub oidc: OidcService,
}

impl AppState {
    pub fn from_env(repository: Repository) -> anyhow::Result<Self> {
        let password_encryption = password_encryption_from_env()?;
        let password_hash = password_hash_from_env()?;
        let user_sessions = UserSessionIssuer::from_env()?;
        let management_token = management_token_from_env()?;
        let oidc = OidcService::from_env()?;
        Ok(Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            password_encryption,
            password_hash,
            user_sessions,
            management_token,
            oidc,
        ))
    }

    pub fn new_for_tests(
        repository: Repository,
        password_encryption_key: impl Into<String>,
        oidc_issuer: &str,
        oidc_private_key_pem: &[u8],
    ) -> Self {
        Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            Arc::new(BuiltInPasswordEncryption::new(password_encryption_key)),
            Arc::new(DeterministicPasswordHash),
            UserSessionIssuer::for_tests("test-user-session-key"),
            "test-management-token".to_string(),
            oidc_service_for_tests(oidc_issuer, oidc_private_key_pem),
        )
    }

    pub fn with_batch_limit_for_tests(
        repository: Repository,
        batch_limit: usize,
        password_encryption_key: impl Into<String>,
        oidc_issuer: &str,
        oidc_private_key_pem: &[u8],
    ) -> Self {
        Self::with_password_providers(
            repository,
            batch_limit,
            Arc::new(BuiltInPasswordEncryption::new(password_encryption_key)),
            Arc::new(DeterministicPasswordHash),
            UserSessionIssuer::for_tests("test-user-session-key"),
            "test-management-token".to_string(),
            oidc_service_for_tests(oidc_issuer, oidc_private_key_pem),
        )
    }

    fn with_password_providers(
        repository: Repository,
        batch_limit: usize,
        password_encryption: Arc<dyn PasswordEncryption>,
        password_hash: Arc<dyn PasswordHashProvider>,
        user_sessions: UserSessionIssuer,
        management_token: String,
        oidc: OidcService,
    ) -> Self {
        let csrf_signer = user_sessions.csrf_signer();
        let management_sessions = ManagementSessionIssuer::from_management_token(&management_token);
        Self {
            repository,
            batch_limit: batch_limit.max(1),
            password_encryption,
            password_hash,
            user_sessions,
            csrf_signer,
            management_token,
            management_sessions,
            oidc,
        }
    }
}

fn oidc_service_for_tests(issuer: &str, private_key_pem: &[u8]) -> OidcService {
    let config = OidcConfig::new(issuer, private_key_pem.to_vec(), false)
        .expect("test OIDC configuration must be valid");
    OidcService::new(config).expect("test OIDC private key must be valid")
}

fn management_token_from_env() -> anyhow::Result<String> {
    let token = std::env::var("ADSS_MANAGEMENT_TOKEN")
        .map_err(|_| anyhow::anyhow!("ADSS_MANAGEMENT_TOKEN is required"))?;
    if token.trim().is_empty() {
        anyhow::bail!("ADSS_MANAGEMENT_TOKEN must not be empty");
    }
    Ok(token)
}

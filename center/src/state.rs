use std::sync::Arc;

use adss_store::Repository;

use crate::password::{
    BuiltInPasswordEncryption, DeterministicPasswordHash, PasswordEncryption, PasswordHashProvider,
    password_encryption_from_env, password_hash_from_env,
};
use crate::session::UserSessionIssuer;

const DEFAULT_BATCH_LIMIT: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub(crate) repository: Repository,
    pub(crate) batch_limit: usize,
    pub(crate) password_encryption: Arc<dyn PasswordEncryption>,
    pub(crate) password_hash: Arc<dyn PasswordHashProvider>,
    pub(crate) user_sessions: UserSessionIssuer,
    pub(crate) management_token: String,
}

impl AppState {
    pub fn from_env(repository: Repository) -> anyhow::Result<Self> {
        Ok(Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            password_encryption_from_env()?,
            password_hash_from_env()?,
            UserSessionIssuer::from_env()?,
            management_token_from_env()?,
        ))
    }

    pub fn new_for_tests(
        repository: Repository,
        password_encryption_key: impl Into<String>,
    ) -> Self {
        Self::with_password_providers(
            repository,
            DEFAULT_BATCH_LIMIT,
            Arc::new(BuiltInPasswordEncryption::new(password_encryption_key)),
            Arc::new(DeterministicPasswordHash),
            UserSessionIssuer::for_tests("test-user-session-key"),
            "test-management-token".to_string(),
        )
    }

    pub fn with_batch_limit_for_tests(
        repository: Repository,
        batch_limit: usize,
        password_encryption_key: impl Into<String>,
    ) -> Self {
        Self::with_password_providers(
            repository,
            batch_limit,
            Arc::new(BuiltInPasswordEncryption::new(password_encryption_key)),
            Arc::new(DeterministicPasswordHash),
            UserSessionIssuer::for_tests("test-user-session-key"),
            "test-management-token".to_string(),
        )
    }

    fn with_password_providers(
        repository: Repository,
        batch_limit: usize,
        password_encryption: Arc<dyn PasswordEncryption>,
        password_hash: Arc<dyn PasswordHashProvider>,
        user_sessions: UserSessionIssuer,
        management_token: String,
    ) -> Self {
        Self {
            repository,
            batch_limit: batch_limit.max(1),
            password_encryption,
            password_hash,
            user_sessions,
            management_token,
        }
    }
}

fn management_token_from_env() -> anyhow::Result<String> {
    let token = std::env::var("ADSS_MANAGEMENT_TOKEN")
        .map_err(|_| anyhow::anyhow!("ADSS_MANAGEMENT_TOKEN is required"))?;
    if token.trim().is_empty() {
        anyhow::bail!("ADSS_MANAGEMENT_TOKEN must not be empty");
    }
    Ok(token)
}

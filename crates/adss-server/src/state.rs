use std::sync::Arc;

use adss_persistence::Repository;

use crate::password::{
    DeterministicPasswordHash, LocalPasswordEnvelope, PasswordEnvelope, PasswordHashProvider,
    password_envelope_from_env, password_hash_from_env,
};

const DEFAULT_BATCH_LIMIT: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub(crate) repository: Repository,
    pub(crate) batch_limit: usize,
    pub(crate) password_envelope: Arc<dyn PasswordEnvelope>,
    pub(crate) password_hash: Arc<dyn PasswordHashProvider>,
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

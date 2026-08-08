mod encryption;
mod hash;

use std::{env, sync::Arc};

pub(crate) use encryption::BuiltInPasswordEncryption;
pub(crate) use hash::DeterministicPasswordHash;

use hash::Argon2idPasswordHash;

const PASSWORD_ENCRYPTION_KEY_ENV: &str = "ADSCOPE_PASSWORD_ENCRYPTION_KEY";
const PASSWORD_HASH_PROVIDER_ENV: &str = "ADSCOPE_PASSWORD_HASH_PROVIDER";

pub(crate) trait PasswordEncryption: Send + Sync {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String>;
    fn open(&self, ciphertext: &str) -> anyhow::Result<String>;
}

pub(crate) trait PasswordHashProvider: Send + Sync {
    fn hash(&self, password: &str) -> anyhow::Result<String>;
    fn verify(&self, password: &str, verifier: &str) -> bool;
}

pub(crate) fn password_encryption_from_env() -> anyhow::Result<Arc<dyn PasswordEncryption>> {
    let key = env::var(PASSWORD_ENCRYPTION_KEY_ENV)
        .map_err(|_| anyhow::anyhow!("{PASSWORD_ENCRYPTION_KEY_ENV} is required"))?;
    if key.is_empty() {
        anyhow::bail!("{PASSWORD_ENCRYPTION_KEY_ENV} must not be empty");
    }
    Ok(Arc::new(BuiltInPasswordEncryption::new(key)))
}

pub(crate) fn password_hash_from_env() -> anyhow::Result<Arc<dyn PasswordHashProvider>> {
    let provider = env::var(PASSWORD_HASH_PROVIDER_ENV)
        .map_err(|_| anyhow::anyhow!("{PASSWORD_HASH_PROVIDER_ENV} is required"))?;

    match provider.as_str() {
        "argon2id" => Ok(Arc::new(Argon2idPasswordHash)),
        _ => anyhow::bail!("unsupported password hash provider: {provider}"),
    }
}

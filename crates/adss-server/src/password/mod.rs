mod envelope;
mod hash;

use std::{env, path::Path as FsPath, sync::Arc};

pub(crate) use envelope::LocalPasswordEnvelope;
pub(crate) use hash::DeterministicPasswordHash;

use envelope::CommandPasswordEnvelope;
use hash::Argon2idPasswordHash;

const PASSWORD_ENVELOPE_PROVIDER_ENV: &str = "ADSS_PASSWORD_ENVELOPE_PROVIDER";
const PASSWORD_ENVELOPE_LOCAL_KEY_ENV: &str = "ADSS_PASSWORD_ENVELOPE_LOCAL_KEY";
const PASSWORD_ENVELOPE_COMMAND_ENV: &str = "ADSS_PASSWORD_ENVELOPE_COMMAND";
const PASSWORD_HASH_PROVIDER_ENV: &str = "ADSS_PASSWORD_HASH_PROVIDER";

pub(crate) trait PasswordEnvelope: Send + Sync {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String>;
    fn open(&self, ciphertext: &str) -> anyhow::Result<String>;
}

pub(crate) trait PasswordHashProvider: Send + Sync {
    fn hash(&self, password: &str) -> anyhow::Result<String>;
    fn verify(&self, password: &str, verifier: &str) -> bool;
}

pub(crate) fn password_envelope_from_env() -> anyhow::Result<Arc<dyn PasswordEnvelope>> {
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

pub(crate) fn password_hash_from_env() -> anyhow::Result<Arc<dyn PasswordHashProvider>> {
    let provider = env::var(PASSWORD_HASH_PROVIDER_ENV)
        .map_err(|_| anyhow::anyhow!("{PASSWORD_HASH_PROVIDER_ENV} is required"))?;

    match provider.as_str() {
        "argon2id" => Ok(Arc::new(Argon2idPasswordHash)),
        _ => anyhow::bail!("unsupported password hash provider: {provider}"),
    }
}

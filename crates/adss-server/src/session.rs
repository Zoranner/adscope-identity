use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const TOKEN_PREFIX: &str = "adss-user-session:v1";
const DEFAULT_SESSION_TTL_SECONDS: u64 = 3600;
const SESSION_KEY_ENV: &str = "ADSS_USER_SESSION_KEY";
const SESSION_TTL_ENV: &str = "ADSS_USER_SESSION_TTL_SECONDS";

#[derive(Debug, Clone)]
pub(crate) struct UserSessionIssuer {
    key: String,
    ttl: Duration,
}

impl UserSessionIssuer {
    pub(crate) fn new(key: impl Into<String>, ttl: Duration) -> Self {
        Self {
            key: key.into(),
            ttl,
        }
    }

    pub(crate) fn for_tests(key: impl Into<String>) -> Self {
        Self::new(key, Duration::from_secs(DEFAULT_SESSION_TTL_SECONDS))
    }

    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var(SESSION_KEY_ENV)
            .map_err(|_| anyhow::anyhow!("{SESSION_KEY_ENV} is required"))?;
        if key.is_empty() {
            anyhow::bail!("{SESSION_KEY_ENV} must not be empty");
        }
        let ttl_seconds = std::env::var(SESSION_TTL_ENV)
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("{SESSION_TTL_ENV} must be a positive integer"))?
            .unwrap_or(DEFAULT_SESSION_TTL_SECONDS);
        if ttl_seconds == 0 {
            anyhow::bail!("{SESSION_TTL_ENV} must be greater than 0");
        }
        Ok(Self::new(key, Duration::from_secs(ttl_seconds)))
    }

    pub(crate) fn issue(&self, employee_id: &str) -> anyhow::Result<String> {
        if employee_id.contains(':') {
            anyhow::bail!("employee_id must not contain ':' for session tokens");
        }
        let expires_at = current_unix_seconds()? + self.ttl.as_secs();
        let signature = self.signature(employee_id, expires_at);
        Ok(format!(
            "{TOKEN_PREFIX}:{employee_id}:{expires_at}:{signature}"
        ))
    }

    pub(crate) fn verify(&self, token: &str) -> Option<String> {
        let mut parts = token.split(':');
        let prefix = format!("{}:{}", parts.next()?, parts.next()?);
        if prefix != TOKEN_PREFIX {
            return None;
        }
        let employee_id = parts.next()?;
        let expires_at = parts.next()?.parse::<u64>().ok()?;
        let signature = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if current_unix_seconds().ok()? > expires_at {
            return None;
        }
        let expected = self.signature(employee_id, expires_at);
        crate::auth::constant_time_eq(signature.as_bytes(), expected.as_bytes())
            .then(|| employee_id.to_string())
    }

    fn signature(&self, employee_id: &str, expires_at: u64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"adss:user-session:v1");
        hasher.update(self.key.as_bytes());
        hasher.update(employee_id.as_bytes());
        hasher.update(expires_at.to_be_bytes());
        hex::encode(hasher.finalize())
    }
}

fn current_unix_seconds() -> anyhow::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

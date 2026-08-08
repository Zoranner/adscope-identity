use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::{TryRngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::oidc::crypto::CsrfSigner;

const TOKEN_PREFIX: &str = "adscope-user-session:v2";
const MANAGEMENT_TOKEN_PREFIX: &str = "adscope-management-session:v1";
const MANAGEMENT_SESSION_TTL_SECONDS: u64 = 8 * 60 * 60;
const DEFAULT_SESSION_TTL_SECONDS: u64 = 3600;
const SESSION_KEY_ENV: &str = "ADSCOPE_USER_SESSION_KEY";
const SESSION_TTL_ENV: &str = "ADSCOPE_USER_SESSION_TTL_SECONDS";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UserSession {
    pub(crate) employee_id: String,
    pub(crate) auth_time: u64,
    pub(crate) expires_at: u64,
}

#[derive(Clone)]
pub(crate) struct UserSessionIssuer {
    key: Vec<u8>,
    ttl: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManagementSession {
    pub(crate) auth_time: u64,
    pub(crate) expires_at: u64,
    pub(crate) csrf_nonce: String,
}

#[derive(Clone)]
pub(crate) struct ManagementSessionIssuer {
    key: Vec<u8>,
    ttl: Duration,
}

impl UserSessionIssuer {
    pub(crate) fn new(key: impl Into<String>, ttl: Duration) -> Self {
        Self {
            key: key.into().into_bytes(),
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
        self.issue_at(employee_id, current_unix_seconds()?)
    }

    pub(crate) fn verify(&self, token: &str) -> Option<UserSession> {
        self.verify_at(token, current_unix_seconds().ok()?)
    }

    pub(crate) fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }

    pub(crate) fn csrf_signer(&self) -> CsrfSigner {
        CsrfSigner::new(&self.key).expect("user session key must be a valid HMAC key")
    }

    fn issue_at(&self, employee_id: &str, auth_time: u64) -> anyhow::Result<String> {
        let expires_at = auth_time
            .checked_add(self.ttl.as_secs())
            .ok_or_else(|| anyhow::anyhow!("session expiration overflow"))?;
        let session = UserSession {
            employee_id: employee_id.to_string(),
            auth_time,
            expires_at,
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&session)?);
        let signed = format!("{TOKEN_PREFIX}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.key)?;
        mac.update(signed.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signed}.{signature}"))
    }

    fn verify_at(&self, token: &str, now: u64) -> Option<UserSession> {
        let (signed, signature) = token.rsplit_once('.')?;
        let payload = signed.strip_prefix(&format!("{TOKEN_PREFIX}."))?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&signature).ok()?;

        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let session: UserSession = serde_json::from_slice(&payload).ok()?;
        (session.auth_time <= session.expires_at && now <= session.expires_at).then_some(session)
    }
}

impl ManagementSessionIssuer {
    pub(crate) fn from_management_token(token: &str) -> Self {
        let mut mac = HmacSha256::new_from_slice(token.as_bytes())
            .expect("management token must be a valid HMAC key");
        mac.update(b"adscope:management-session:v1");
        Self {
            key: mac.finalize().into_bytes().to_vec(),
            ttl: Duration::from_secs(MANAGEMENT_SESSION_TTL_SECONDS),
        }
    }

    pub(crate) fn issue(&self) -> anyhow::Result<String> {
        self.issue_at(current_unix_seconds()?)
    }

    pub(crate) fn verify(&self, token: &str) -> Option<ManagementSession> {
        self.verify_at(token, current_unix_seconds().ok()?)
    }

    pub(crate) fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }

    fn issue_at(&self, auth_time: u64) -> anyhow::Result<String> {
        let expires_at = auth_time
            .checked_add(self.ttl.as_secs())
            .ok_or_else(|| anyhow::anyhow!("management session expiration overflow"))?;
        let mut csrf_nonce = [0_u8; 32];
        OsRng.try_fill_bytes(&mut csrf_nonce).map_err(|error| {
            anyhow::anyhow!("operating system random source unavailable: {error}")
        })?;
        let session = ManagementSession {
            auth_time,
            expires_at,
            csrf_nonce: URL_SAFE_NO_PAD.encode(csrf_nonce),
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&session)?);
        let signed = format!("{MANAGEMENT_TOKEN_PREFIX}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.key)?;
        mac.update(signed.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signed}.{signature}"))
    }

    fn verify_at(&self, token: &str, now: u64) -> Option<ManagementSession> {
        let (signed, signature) = token.rsplit_once('.')?;
        let payload = signed.strip_prefix(&format!("{MANAGEMENT_TOKEN_PREFIX}."))?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&signature).ok()?;

        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let session: ManagementSession = serde_json::from_slice(&payload).ok()?;
        (session.auth_time <= session.expires_at && now <= session.expires_at).then_some(session)
    }
}

fn current_unix_seconds() -> anyhow::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_session_round_trips_structured_payload() {
        let issuer = UserSessionIssuer::new("session-secret", Duration::from_secs(60));

        let token = issuer.issue_at("employee:1001", 1_000).unwrap();
        let session = issuer.verify_at(&token, 1_030).unwrap();

        assert!(token.starts_with("adscope-user-session:v2."));
        assert_eq!(session.employee_id, "employee:1001");
        assert_eq!(session.auth_time, 1_000);
        assert_eq!(session.expires_at, 1_060);
    }

    #[test]
    fn v2_session_rejects_tampering_malformed_payload_and_expiration() {
        let issuer = UserSessionIssuer::new("session-secret", Duration::from_secs(60));
        let token = issuer.issue_at("1001", 1_000).unwrap();
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let tampered = format!("{signed}.A{}", &signature[1..]);

        assert!(issuer.verify_at(&tampered, 1_001).is_none());
        assert!(
            issuer
                .verify_at("adscope-user-session:v2.not-json.signature", 1_001)
                .is_none()
        );
        assert!(issuer.verify_at(&token, 1_061).is_none());
        assert!(issuer.verify_at(&token, 1_060).is_some());
    }
}

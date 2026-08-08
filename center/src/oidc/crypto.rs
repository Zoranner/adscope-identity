use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::{TryRngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CSRF_PREFIX: &str = "adscope-csrf:v1";

type HmacSha256 = Hmac<Sha256>;

pub fn random_urlsafe(bytes: usize) -> anyhow::Result<String> {
    let mut random = vec![0_u8; bytes];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(|error| anyhow::anyhow!("operating system random source unavailable: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(random))
}

pub fn sha256_token(value: impl AsRef<[u8]>) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_ref())))
}

pub fn pkce_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[derive(Clone)]
pub struct CsrfSigner {
    key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CsrfPayload {
    employee_id: String,
    authorization_request_digest: String,
    expires_at: u64,
}

impl CsrfSigner {
    pub fn new(key: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        let key = key.as_ref();
        anyhow::ensure!(!key.is_empty(), "CSRF signing key must not be empty");
        HmacSha256::new_from_slice(key)?;
        Ok(Self { key: key.to_vec() })
    }

    pub fn issue(
        &self,
        employee_id: &str,
        authorization_request_digest: &str,
        expires_at: u64,
    ) -> anyhow::Result<String> {
        let payload = CsrfPayload {
            employee_id: employee_id.to_string(),
            authorization_request_digest: authorization_request_digest.to_string(),
            expires_at,
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?);
        let signed = format!("{CSRF_PREFIX}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.key)?;
        mac.update(signed.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signed}.{signature}"))
    }

    pub fn verify(
        &self,
        token: &str,
        employee_id: &str,
        authorization_request_digest: &str,
        now: u64,
    ) -> bool {
        self.verify_inner(token, employee_id, authorization_request_digest, now)
            .is_some()
    }

    fn verify_inner(
        &self,
        token: &str,
        employee_id: &str,
        authorization_request_digest: &str,
        now: u64,
    ) -> Option<()> {
        let (signed, signature) = token.rsplit_once('.')?;
        let payload = signed.strip_prefix(&format!("{CSRF_PREFIX}."))?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&signature).ok()?;

        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let payload: CsrfPayload = serde_json::from_slice(&payload).ok()?;
        (payload.employee_id == employee_id
            && payload.authorization_request_digest == authorization_request_digest
            && now <= payload.expires_at)
            .then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_matches_rfc7636_vector() {
        assert_eq!(
            pkce_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_urlsafe_has_expected_encoded_length_and_alphabet() {
        let first = random_urlsafe(32).unwrap();
        let second = random_urlsafe(32).unwrap();

        assert_eq!(first.len(), 43);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
        assert_ne!(first, second);
    }

    #[test]
    fn sha256_token_is_stable_and_namespaced() {
        assert_eq!(
            sha256_token("authorization-code"),
            "sha256:595614278d7adc57bb92e28fa203a8a18d797a07684fdff75cbbf2a96ebf8577"
        );
    }

    #[test]
    fn csrf_token_binds_subject_request_and_expiration() {
        let signer = CsrfSigner::new("csrf-secret").unwrap();
        let token = signer.issue("1001", "sha256:request-a", 1_060).unwrap();

        assert!(signer.verify(&token, "1001", "sha256:request-a", 1_000));
        assert!(!signer.verify(&token, "1002", "sha256:request-a", 1_000));
        assert!(!signer.verify(&token, "1001", "sha256:request-b", 1_000));
        assert!(!signer.verify(&token, "1001", "sha256:request-a", 1_061));
    }

    #[test]
    fn csrf_token_rejects_tampering_and_malformed_payload() {
        let signer = CsrfSigner::new("csrf-secret").unwrap();
        let token = signer.issue("1001", "sha256:request-a", 1_060).unwrap();
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let tampered = format!("{signed}.A{}", &signature[1..]);

        assert!(!signer.verify(&tampered, "1001", "sha256:request-a", 1_000));
        assert!(!signer.verify(
            "adscope-csrf:v1.not-json.signature",
            "1001",
            "sha256:request-a",
            1_000
        ));
    }
}

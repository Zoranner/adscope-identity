pub mod config;
pub mod crypto;
pub(crate) mod routes;
pub mod validation;

use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey},
    pkcs8::DecodePrivateKey,
    traits::PublicKeyParts,
};
use serde::{Deserialize, Serialize};

use self::{config::OidcConfig, crypto::sha256_token};

const MAX_CLOCK_SKEW_SECONDS: u64 = 30;

#[derive(Clone)]
pub struct OidcService {
    config: OidcConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    jwks: JwkSetResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub iat: u64,
    pub exp: u64,
    pub auth_time: u64,
    pub nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
}

#[derive(Default)]
pub(crate) struct IdTokenUserClaims {
    pub preferred_username: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub client_id: String,
    pub scope: String,
    pub iat: u64,
    pub exp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwkSetResponse {
    pub keys: Vec<RsaJwk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsaJwk {
    pub kty: String,
    #[serde(rename = "use")]
    pub key_use: String,
    #[serde(rename = "alg")]
    pub algorithm: String,
    pub kid: String,
    pub n: String,
    pub e: String,
}

impl OidcService {
    pub fn new(config: OidcConfig) -> anyhow::Result<Self> {
        let pem = std::str::from_utf8(config.private_key_pem())?;
        let private_key = RsaPrivateKey::from_pkcs8_pem(pem)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
            .map_err(|_| {
                anyhow::anyhow!("OIDC private key must be valid RSA PKCS#8 or PKCS#1 PEM")
            })?;
        let public_key = RsaPublicKey::from(&private_key);
        anyhow::ensure!(
            public_key.n().bits() >= 2048,
            "OIDC RSA private key must be at least 2048 bits"
        );
        let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());
        let kid = sha256_token(format!("{n}.{e}"));
        let private_key_der = private_key.to_pkcs1_der()?;
        let encoding_key = EncodingKey::from_rsa_der(private_key_der.as_bytes());
        let decoding_key = DecodingKey::from_rsa_components(&n, &e)?;
        let jwks = JwkSetResponse {
            keys: vec![RsaJwk {
                kty: "RSA".to_string(),
                key_use: "sig".to_string(),
                algorithm: "RS256".to_string(),
                kid,
                n,
                e,
            }],
        };
        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            jwks,
        })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Self::new(OidcConfig::from_env()?)
    }

    pub(crate) fn from_env_with_private_key_path(private_key_path: &Path) -> anyhow::Result<Self> {
        Self::new(OidcConfig::from_env_with_private_key_path(
            private_key_path,
        )?)
    }

    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    pub fn key_id(&self) -> &str {
        &self.jwks.keys[0].kid
    }

    pub fn jwks(&self) -> &JwkSetResponse {
        &self.jwks
    }

    pub fn issue_id_token(
        &self,
        subject: &str,
        audience: &str,
        auth_time: u64,
        nonce: &str,
    ) -> anyhow::Result<String> {
        self.issue_id_token_with_user_claims(
            subject,
            audience,
            auth_time,
            nonce,
            IdTokenUserClaims::default(),
        )
    }

    pub(crate) fn issue_id_token_with_user_claims(
        &self,
        subject: &str,
        audience: &str,
        auth_time: u64,
        nonce: &str,
        user_claims: IdTokenUserClaims,
    ) -> anyhow::Result<String> {
        let iat = current_unix_seconds()?;
        let exp = iat
            .checked_add(self.config.token_ttl().as_secs())
            .ok_or_else(|| anyhow::anyhow!("ID token expiration overflow"))?;
        let claims = IdTokenClaims {
            iss: self.config.issuer().to_string(),
            aud: audience.to_string(),
            sub: subject.to_string(),
            iat,
            exp,
            auth_time,
            nonce: nonce.to_string(),
            preferred_username: user_claims.preferred_username,
            name: user_claims.name,
            email: user_claims.email,
            phone_number: user_claims.phone_number,
        };
        self.encode_claims(&claims)
    }

    pub fn verify_id_token(&self, token: &str, audience: &str) -> anyhow::Result<IdTokenClaims> {
        let header = decode_header(token)?;
        anyhow::ensure!(header.alg == Algorithm::RS256, "ID token must use RS256");
        anyhow::ensure!(
            header.kid.as_deref() == Some(self.key_id()),
            "ID token kid is not active"
        );
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = MAX_CLOCK_SKEW_SECONDS;
        validation.set_issuer(&[self.config.issuer()]);
        validation.set_audience(&[audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<IdTokenClaims>(token, &self.decoding_key, &validation)?.claims;
        let now = current_unix_seconds()?;
        anyhow::ensure!(
            claims.exp.checked_sub(claims.iat) == Some(self.config.token_ttl().as_secs()),
            "ID token TTL is invalid"
        );
        anyhow::ensure!(
            claims.iat <= now.saturating_add(MAX_CLOCK_SKEW_SECONDS),
            "ID token was issued too far in the future"
        );
        anyhow::ensure!(
            claims.auth_time <= claims.iat.saturating_add(MAX_CLOCK_SKEW_SECONDS),
            "ID token auth_time is invalid"
        );
        Ok(claims)
    }

    pub fn issue_access_token(
        &self,
        subject: &str,
        client_id: &str,
        scope: &str,
    ) -> anyhow::Result<String> {
        self.issue_access_token_at(subject, client_id, scope, current_unix_seconds()?)
    }

    pub fn issue_access_token_at(
        &self,
        subject: &str,
        client_id: &str,
        scope: &str,
        issued_at: u64,
    ) -> anyhow::Result<String> {
        validation::validate_scopes(scope)?;
        let exp = issued_at
            .checked_add(self.config.token_ttl().as_secs())
            .ok_or_else(|| anyhow::anyhow!("OIDC access token expiration overflow"))?;
        self.encode_claims(&AccessTokenClaims {
            iss: self.config.issuer().to_string(),
            sub: subject.to_string(),
            aud: self.userinfo_audience(),
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            iat: issued_at,
            exp,
        })
    }

    pub fn verify_access_token(&self, token: &str) -> anyhow::Result<AccessTokenClaims> {
        let header = decode_header(token)?;
        anyhow::ensure!(
            header.alg == Algorithm::RS256,
            "OIDC access token must use RS256"
        );
        anyhow::ensure!(
            header.kid.as_deref() == Some(self.key_id()),
            "OIDC access token kid is not active"
        );
        let audience = self.userinfo_audience();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = MAX_CLOCK_SKEW_SECONDS;
        validation.set_issuer(&[self.config.issuer()]);
        validation.set_audience(&[audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<AccessTokenClaims>(token, &self.decoding_key, &validation)?.claims;
        let now = current_unix_seconds()?;
        anyhow::ensure!(
            claims.exp.checked_sub(claims.iat) == Some(self.config.token_ttl().as_secs()),
            "OIDC access token TTL is invalid"
        );
        anyhow::ensure!(
            claims.iat <= now.saturating_add(MAX_CLOCK_SKEW_SECONDS),
            "OIDC access token was issued too far in the future"
        );
        validation::validate_scopes(&claims.scope)?;
        Ok(claims)
    }

    fn userinfo_audience(&self) -> String {
        format!("{}/oauth2/userinfo", self.config.issuer())
    }

    fn encode_claims<T: Serialize>(&self, claims: &T) -> anyhow::Result<String> {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id().to_string());
        Ok(encode(&header, claims, &self.encoding_key)?)
    }
}

fn current_unix_seconds() -> anyhow::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
    use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, pkcs8::DecodePrivateKey};

    use super::{IdTokenClaims, OidcService, config::OidcConfig};

    const PRIVATE_KEY: &[u8] = include_bytes!("../../tests/fixtures/oidc-private-key.pem");

    #[test]
    fn invalid_private_key_is_rejected() {
        let config = OidcConfig::new(
            "https://center.example.test",
            b"not a private key".to_vec(),
            false,
        )
        .unwrap();
        assert!(OidcService::new(config).is_err());
    }

    #[test]
    fn pkcs1_private_key_is_accepted() {
        let private_key =
            RsaPrivateKey::from_pkcs8_pem(std::str::from_utf8(PRIVATE_KEY).unwrap()).unwrap();
        let pkcs1_pem = private_key.to_pkcs1_pem(Default::default()).unwrap();
        let config = OidcConfig::new(
            "https://center.example.test",
            pkcs1_pem.as_bytes().to_vec(),
            false,
        )
        .unwrap();

        assert!(OidcService::new(config).is_ok());
    }

    #[test]
    fn rsa_private_key_rejects_2047_bits_and_accepts_2048_bits() {
        let error = match service_with_generated_rsa_key(2047) {
            Ok(_) => panic!("RSA private keys below 2048 bits must be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("at least 2048 bits"));
        assert!(service_with_generated_rsa_key(2048).is_ok());
    }

    #[test]
    fn jwks_contains_public_rsa_parameters_and_no_private_fields() {
        let service = test_service();
        let jwks = service.jwks();
        assert_eq!(jwks.keys.len(), 1);
        let key = &jwks.keys[0];
        assert_eq!(key.kty, "RSA");
        assert_eq!(key.algorithm, "RS256");
        assert_eq!(key.key_use, "sig");
        assert_eq!(key.kid, service.key_id());
        assert!(!key.n.is_empty());
        assert!(!key.e.is_empty());
        assert_eq!(test_service().key_id(), service.key_id());

        let serialized = serde_json::to_value(jwks).unwrap();
        let key = &serialized["keys"][0];
        for private_name in ["d", "p", "q", "dp", "dq", "qi"] {
            assert!(key.get(private_name).is_none());
        }
    }

    #[test]
    fn id_token_verifies_with_jwks_and_has_fixed_contract() {
        let service = test_service();
        let token = service
            .issue_id_token("1001", "client-web", 1_700_000_000, "nonce-value")
            .unwrap();
        let header = decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some(service.key_id()));

        let jwk = &service.jwks().keys[0];
        let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = false;
        validation.set_issuer(&["https://center.example.test"]);
        validation.set_audience(&["client-web"]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<IdTokenClaims>(&token, &decoding_key, &validation)
            .unwrap()
            .claims;

        assert_eq!(claims.iss, "https://center.example.test");
        assert_eq!(claims.aud, "client-web");
        assert_eq!(claims.sub, "1001");
        assert_eq!(claims.auth_time, 1_700_000_000);
        assert_eq!(claims.nonce, "nonce-value");
        assert_eq!(claims.exp - claims.iat, 300);
        assert_eq!(
            service.verify_id_token(&token, "client-web").unwrap(),
            claims
        );
        assert!(service.verify_id_token(&token, "other-client").is_err());
    }

    fn test_service() -> OidcService {
        OidcService::new(
            OidcConfig::new("https://center.example.test", PRIVATE_KEY.to_vec(), false).unwrap(),
        )
        .unwrap()
    }

    fn service_with_generated_rsa_key(bits: usize) -> anyhow::Result<OidcService> {
        let private_key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, bits)?;
        let pkcs1_pem = private_key.to_pkcs1_pem(Default::default())?;
        let config = OidcConfig::new(
            "https://center.example.test",
            pkcs1_pem.as_bytes().to_vec(),
            false,
        )?;
        OidcService::new(config)
    }
}

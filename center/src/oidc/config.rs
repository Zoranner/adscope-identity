use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, ensure};
use url::Url;

const ISSUER_ENV: &str = "OIDC_ISSUER";
const ALLOW_INSECURE_WEB_LOOPBACK_ENV: &str = "OIDC_LOOPBACK_HTTP";
const AUTHORIZATION_CODE_TTL_SECONDS: u64 = 120;
const TOKEN_TTL_SECONDS: u64 = 300;
const CONTAINER_PRIVATE_KEY_PATH: &str = "/run/secrets/oidc-private-key.pem";
const LOCAL_PRIVATE_KEY_PATH: &str = "secrets/oidc-private-key.pem";

#[derive(Clone)]
pub struct OidcConfig {
    issuer: Url,
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
    authorization_code_ttl: Duration,
    token_ttl: Duration,
}

impl OidcConfig {
    pub fn new(
        issuer: &str,
        private_key_pem: Vec<u8>,
        allow_insecure_web_loopback_redirects: bool,
    ) -> anyhow::Result<Self> {
        let issuer = Url::parse(issuer).context("OIDC_ISSUER must be an absolute URL")?;
        ensure!(issuer.scheme() == "https", "OIDC_ISSUER must use HTTPS");
        ensure!(issuer.host().is_some(), "OIDC_ISSUER must include a host");
        ensure!(
            issuer.username().is_empty() && issuer.password().is_none(),
            "OIDC_ISSUER must not include userinfo"
        );
        ensure!(
            issuer.query().is_none(),
            "OIDC_ISSUER must not include a query"
        );
        ensure!(
            issuer.fragment().is_none(),
            "OIDC_ISSUER must not include a fragment"
        );
        ensure!(
            issuer.path() == "/",
            "OIDC_ISSUER path must be '/' or empty"
        );

        Ok(Self {
            issuer,
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            authorization_code_ttl: Duration::from_secs(AUTHORIZATION_CODE_TTL_SECONDS),
            token_ttl: Duration::from_secs(TOKEN_TTL_SECONDS),
        })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let working_directory =
            std::env::current_dir().context("failed to determine working directory")?;
        Self::from_env_with_private_key_path(&Self::private_key_path_from(&working_directory))
    }

    pub(crate) fn from_env_with_private_key_path(private_key_path: &Path) -> anyhow::Result<Self> {
        let issuer =
            std::env::var(ISSUER_ENV).map_err(|_| anyhow::anyhow!("{ISSUER_ENV} is required"))?;
        let private_key_pem = std::fs::read(private_key_path).with_context(|| {
            format!(
                "failed to read OIDC private key at {}",
                private_key_path.display()
            )
        })?;
        let allow_insecure_web_loopback_redirects =
            match std::env::var(ALLOW_INSECURE_WEB_LOOPBACK_ENV) {
                Ok(value) => value.parse::<bool>().map_err(|_| {
                    anyhow::anyhow!("{ALLOW_INSECURE_WEB_LOOPBACK_ENV} must be true or false")
                })?,
                Err(std::env::VarError::NotPresent) => false,
                Err(error) => return Err(error.into()),
            };

        Self::new(
            &issuer,
            private_key_pem,
            allow_insecure_web_loopback_redirects,
        )
    }

    pub(crate) fn private_key_path_from(working_directory: &Path) -> PathBuf {
        let local_private_key_path = working_directory.join(LOCAL_PRIVATE_KEY_PATH);
        if local_private_key_path.is_file() {
            local_private_key_path
        } else {
            PathBuf::from(CONTAINER_PRIVATE_KEY_PATH)
        }
    }

    pub fn issuer(&self) -> &str {
        self.issuer.as_str().trim_end_matches('/')
    }

    pub fn allow_insecure_web_loopback_redirects(&self) -> bool {
        self.allow_insecure_web_loopback_redirects
    }

    pub fn authorization_code_ttl(&self) -> Duration {
        self.authorization_code_ttl
    }

    pub fn token_ttl(&self) -> Duration {
        self.token_ttl
    }

    pub(crate) fn private_key_pem(&self) -> &[u8] {
        &self.private_key_pem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIVATE_KEY: &[u8] = include_bytes!("../../tests/fixtures/oidc-private-key.pem");

    #[test]
    fn issuer_must_be_an_https_origin() {
        assert!(
            OidcConfig::new("https://center.example.test", PRIVATE_KEY.to_vec(), false).is_ok()
        );
        for issuer in [
            "http://center.example.test",
            "https://center.example.test/path",
            "https://center.example.test?query=value",
            "https://center.example.test#fragment",
            "https://user@center.example.test",
        ] {
            assert!(
                OidcConfig::new(issuer, PRIVATE_KEY.to_vec(), false).is_err(),
                "unexpectedly accepted {issuer}"
            );
        }
    }

    #[test]
    fn fixed_protocol_ttls_are_exposed() {
        let config =
            OidcConfig::new("https://center.example.test", PRIVATE_KEY.to_vec(), false).unwrap();
        assert_eq!(config.authorization_code_ttl().as_secs(), 120);
        assert_eq!(config.token_ttl().as_secs(), 300);
    }

    #[test]
    fn local_private_key_path_is_preferred_from_working_directory() {
        let temporary_directory = std::env::temp_dir().join(format!(
            "adscope-oidc-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let local_private_key = temporary_directory.join("secrets/oidc-private-key.pem");
        std::fs::create_dir_all(local_private_key.parent().unwrap()).unwrap();
        std::fs::write(&local_private_key, PRIVATE_KEY).unwrap();

        assert_eq!(
            OidcConfig::private_key_path_from(&temporary_directory),
            local_private_key
        );

        std::fs::remove_dir_all(temporary_directory).unwrap();
    }

    #[test]
    fn container_private_key_path_is_used_without_local_key() {
        assert_eq!(
            OidcConfig::private_key_path_from(Path::new("missing-working-directory")),
            Path::new("/run/secrets/oidc-private-key.pem")
        );
    }
}

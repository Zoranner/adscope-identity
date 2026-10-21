use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail, ensure};
use url::Url;

const ISSUERS_ENV: &str = "OIDC_ISSUERS";
const LEGACY_ISSUER_ENV: &str = "OIDC_ISSUER";
const ALLOW_INSECURE_WEB_LOOPBACK_ENV: &str = "OIDC_LOOPBACK_HTTP";
const AUTHORIZATION_CODE_TTL_SECONDS: u64 = 120;
const TOKEN_TTL_SECONDS: u64 = 300;
const CONTAINER_PRIVATE_KEY_PATH: &str = "/run/secrets/oidc-private-key.pem";
const LOCAL_PRIVATE_KEY_PATH: &str = "secrets/oidc-private-key.pem";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OidcIssuer {
    issuer: String,
    host: String,
}

impl OidcIssuer {
    fn parse(value: &str) -> anyhow::Result<Self> {
        let value = value.trim();
        let url = Url::parse(value).context("OIDC issuer must be an absolute URL")?;
        ensure!(url.scheme() == "https", "OIDC issuer must use HTTPS");
        ensure!(url.host().is_some(), "OIDC issuer must include a host");
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "OIDC issuer must not include userinfo"
        );
        ensure!(
            url.query().is_none(),
            "OIDC issuer must not include a query"
        );
        ensure!(
            url.fragment().is_none(),
            "OIDC issuer must not include a fragment"
        );
        ensure!(url.path() == "/", "OIDC issuer path must be '/' or empty");

        let host = url
            .host_str()
            .expect("validated issuer has a host")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        ensure!(!host.is_empty(), "OIDC issuer host must not be empty");

        let mut normalized = url;
        normalized
            .set_host(Some(&host))
            .expect("validated issuer host must remain valid");
        normalized.set_path("");
        let issuer = normalized.as_str().trim_end_matches('/').to_string();
        let host = issuer
            .strip_prefix("https://")
            .expect("validated issuer uses HTTPS")
            .to_string();

        Ok(Self { issuer, host })
    }

    fn as_str(&self) -> &str {
        &self.issuer
    }
}

fn normalize_host(value: &str) -> Option<String> {
    let url = Url::parse(&format!("https://{value}")).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let mut normalized = url;
    normalized.set_host(Some(&host)).ok()?;
    normalized.set_path("");
    Some(
        normalized
            .as_str()
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string(),
    )
}

fn ensure_unique_hosts(issuers: &[OidcIssuer]) -> anyhow::Result<()> {
    let mut hosts = HashSet::new();
    for issuer in issuers {
        ensure!(
            hosts.insert(issuer.host.as_str()),
            "OIDC_ISSUERS must not contain duplicate hosts"
        );
    }
    Ok(())
}

fn parse_issuer_list(value: &str) -> anyhow::Result<Vec<OidcIssuer>> {
    let values = value.split(',').map(str::trim).collect::<Vec<_>>();
    ensure!(
        !values.is_empty() && values.iter().all(|value| !value.is_empty()),
        "OIDC_ISSUERS must contain at least one issuer"
    );
    let issuers = values
        .iter()
        .map(|value| OidcIssuer::parse(value))
        .collect::<anyhow::Result<Vec<_>>>()?;
    ensure_unique_hosts(&issuers)?;
    Ok(issuers)
}

fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone)]
pub struct OidcConfig {
    issuers: Vec<OidcIssuer>,
    legacy_single_issuer: bool,
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
    authorization_code_ttl: Duration,
    token_ttl: Duration,
}

impl OidcConfig {
    pub fn new_single_issuer_legacy(
        issuer: &str,
        private_key_pem: Vec<u8>,
        allow_insecure_web_loopback_redirects: bool,
    ) -> anyhow::Result<Self> {
        Self::build(
            vec![OidcIssuer::parse(issuer)?],
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            true,
        )
    }

    pub fn from_issuers(
        issuers: &[&str],
        private_key_pem: Vec<u8>,
        allow_insecure_web_loopback_redirects: bool,
    ) -> anyhow::Result<Self> {
        ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
        let issuers = issuers
            .iter()
            .map(|issuer| OidcIssuer::parse(issuer))
            .collect::<anyhow::Result<Vec<_>>>()?;
        ensure_unique_hosts(&issuers)?;
        Self::build(
            issuers,
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            false,
        )
    }

    fn build(
        issuers: Vec<OidcIssuer>,
        private_key_pem: Vec<u8>,
        allow_insecure_web_loopback_redirects: bool,
        legacy_single_issuer: bool,
    ) -> anyhow::Result<Self> {
        ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
        Ok(Self {
            issuers,
            legacy_single_issuer,
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
        let issuers_env = optional_env(ISSUERS_ENV)?;
        let legacy_issuer_env = optional_env(LEGACY_ISSUER_ENV)?;
        let (issuers, legacy_single_issuer) = match (issuers_env, legacy_issuer_env) {
            (Some(_), Some(_)) => {
                bail!("{ISSUERS_ENV} and {LEGACY_ISSUER_ENV} must not both be set")
            }
            (Some(value), None) => (parse_issuer_list(&value)?, false),
            (None, Some(value)) => (vec![OidcIssuer::parse(&value)?], true),
            (None, None) => bail!("{ISSUERS_ENV} or {LEGACY_ISSUER_ENV} is required"),
        };
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

        Self::build(
            issuers,
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            legacy_single_issuer,
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

    pub fn issuer_for_host(&self, host: Option<&str>) -> Option<&str> {
        if self.legacy_single_issuer {
            return self.issuers.first().map(OidcIssuer::as_str);
        }
        let host = normalize_host(host?)?;
        self.issuers
            .iter()
            .find(|issuer| issuer.host == host)
            .map(OidcIssuer::as_str)
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
            OidcConfig::new_single_issuer_legacy(
                "https://center.example.test",
                PRIVATE_KEY.to_vec(),
                false,
            )
            .is_ok()
        );
        for issuer in [
            "http://center.example.test",
            "https://center.example.test/path",
            "https://center.example.test?query=value",
            "https://center.example.test#fragment",
            "https://user@center.example.test",
        ] {
            assert!(
                OidcConfig::new_single_issuer_legacy(issuer, PRIVATE_KEY.to_vec(), false).is_err(),
                "unexpectedly accepted {issuer}"
            );
        }
    }

    #[test]
    fn fixed_protocol_ttls_are_exposed() {
        let config = OidcConfig::new_single_issuer_legacy(
            "https://center.example.test",
            PRIVATE_KEY.to_vec(),
            false,
        )
        .unwrap();
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

    #[test]
    fn multiple_issuers_are_normalized_and_selected_by_host() {
        let config = OidcConfig::from_issuers(
            &[
                "https://auth-a.example.com.:443",
                "https://auth-b.example.com",
                "https://auth-c.example.com:8443",
            ],
            PRIVATE_KEY.to_vec(),
            false,
        )
        .unwrap();

        assert_eq!(
            config.issuer_for_host(Some("auth-a.example.com.:443")),
            Some("https://auth-a.example.com")
        );
        assert_eq!(
            config.issuer_for_host(Some("auth-b.example.com")),
            Some("https://auth-b.example.com")
        );
        assert_eq!(
            config.issuer_for_host(Some("auth-c.example.com:8443")),
            Some("https://auth-c.example.com:8443")
        );
        assert_eq!(config.issuer_for_host(Some("core.example.com")), None);
        assert_eq!(config.issuer_for_host(None), None);
    }

    #[test]
    fn issuer_lists_reject_invalid_duplicate_and_empty_origins() {
        for issuer in [
            "http://auth-a.example.com",
            "https://auth-a.example.com/path",
            "https://auth-a.example.com?query=value",
            "https://auth-a.example.com#fragment",
            "https://user@auth-a.example.com",
        ] {
            assert!(
                OidcConfig::from_issuers(&[issuer], PRIVATE_KEY.to_vec(), false).is_err(),
                "unexpectedly accepted {issuer}"
            );
        }

        assert!(
            OidcConfig::from_issuers(
                &["https://auth-a.example.com", "https://auth-a.example.com.:443"],
                PRIVATE_KEY.to_vec(),
                false,
            )
            .is_err()
        );
        assert!(OidcConfig::from_issuers(&[], PRIVATE_KEY.to_vec(), false).is_err());
    }

    #[test]
    fn legacy_single_issuer_ignores_request_host() {
        let config = OidcConfig::new_single_issuer_legacy(
            "https://center.example.test",
            PRIVATE_KEY.to_vec(),
            false,
        )
        .unwrap();

        assert_eq!(
            config.issuer_for_host(Some("attacker.example.test")),
            Some("https://center.example.test")
        );
        assert_eq!(
            config.issuer_for_host(None),
            Some("https://center.example.test")
        );
    }
}

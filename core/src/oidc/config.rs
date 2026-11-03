use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail, ensure};
use url::Url;

use crate::auth::constant_time_eq;

const ISSUERS_ENV: &str = "OIDC_ISSUERS";
const ALLOW_INSECURE_WEB_LOOPBACK_ENV: &str = "OIDC_LOOPBACK_HTTP";
const PORTAL_ACCESS_KEYS_ENV: &str = "PORTAL_ACCESS_KEYS";
const MIN_PORTAL_ACCESS_KEY_LEN: usize = 32;
const AUTHORIZATION_CODE_TTL_SECONDS: u64 = 120;
const TOKEN_TTL_SECONDS: u64 = 300;
const CONTAINER_PRIVATE_KEY_PATH: &str = "/run/secrets/oidc-private-key.pem";
const LOCAL_PRIVATE_KEY_PATH: &str = "secrets/oidc-private-key.pem";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OidcIssuer {
    issuer: String,
    host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PortalAccessKey {
    issuer: String,
    key: String,
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

fn parse_portal_access_keys(
    value: &str,
    issuers: &[OidcIssuer],
) -> anyhow::Result<Vec<PortalAccessKey>> {
    let mut access_keys = Vec::new();
    for entry in value.split(',').map(str::trim) {
        ensure!(
            !entry.is_empty(),
            "{PORTAL_ACCESS_KEYS_ENV} must not contain empty entries"
        );
        let (issuer, key) = entry.split_once('=').with_context(|| {
            format!("{PORTAL_ACCESS_KEYS_ENV} entries must use issuer=key: {entry}")
        })?;
        let issuer = OidcIssuer::parse(issuer.trim())?;
        ensure!(
            issuers
                .iter()
                .any(|configured| configured.issuer == issuer.issuer),
            "{PORTAL_ACCESS_KEYS_ENV} issuer is not listed in {ISSUERS_ENV}: {}",
            issuer.issuer
        );
        ensure!(
            access_keys
                .iter()
                .all(|existing: &PortalAccessKey| existing.issuer != issuer.issuer),
            "{PORTAL_ACCESS_KEYS_ENV} must not repeat an issuer: {}",
            issuer.issuer
        );
        let key = key.trim();
        ensure!(
            key.chars().count() >= MIN_PORTAL_ACCESS_KEY_LEN,
            "{PORTAL_ACCESS_KEYS_ENV} key must be at least {MIN_PORTAL_ACCESS_KEY_LEN} characters"
        );
        ensure!(
            key.chars()
                .all(|character| character.is_ascii_graphic() && character != ','),
            "{PORTAL_ACCESS_KEYS_ENV} key must be printable ASCII without commas"
        );
        ensure!(
            access_keys.iter().all(|existing| existing.key != key),
            "{PORTAL_ACCESS_KEYS_ENV} must not repeat a key"
        );
        access_keys.push(PortalAccessKey {
            issuer: issuer.issuer,
            key: key.to_string(),
        });
    }
    ensure!(
        !access_keys.is_empty(),
        "{PORTAL_ACCESS_KEYS_ENV} must contain at least one entry"
    );
    Ok(access_keys)
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
    portal_access_keys: Vec<PortalAccessKey>,
    /// 仅供测试配置：单 issuer 且忽略 Host。生产配置不再支持这种模式。
    host_independent_issuer: bool,
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
    authorization_code_ttl: Duration,
    token_ttl: Duration,
}

impl OidcConfig {
    #[doc(hidden)]
    pub fn fixed_issuer_for_tests(issuer: &str, private_key_pem: Vec<u8>) -> anyhow::Result<Self> {
        Self::build(
            vec![OidcIssuer::parse(issuer)?],
            Vec::new(),
            private_key_pem,
            false,
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
            Vec::new(),
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            false,
        )
    }

    #[doc(hidden)]
    pub fn from_issuers_with_portal_access_keys(
        issuers: &[&str],
        portal_access_keys: &[(&str, &str)],
        private_key_pem: Vec<u8>,
    ) -> anyhow::Result<Self> {
        ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
        let issuers = issuers
            .iter()
            .map(|issuer| OidcIssuer::parse(issuer))
            .collect::<anyhow::Result<Vec<_>>>()?;
        ensure_unique_hosts(&issuers)?;
        let portal_access_keys = portal_access_keys
            .iter()
            .map(|(issuer, key)| {
                Ok(PortalAccessKey {
                    issuer: OidcIssuer::parse(issuer)?.issuer,
                    key: (*key).to_string(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Self::build(issuers, portal_access_keys, private_key_pem, false, false)
    }

    fn build(
        issuers: Vec<OidcIssuer>,
        portal_access_keys: Vec<PortalAccessKey>,
        private_key_pem: Vec<u8>,
        allow_insecure_web_loopback_redirects: bool,
        host_independent_issuer: bool,
    ) -> anyhow::Result<Self> {
        ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
        if !portal_access_keys.is_empty() {
            for issuer in &issuers {
                ensure!(
                    portal_access_keys
                        .iter()
                        .any(|entry| entry.issuer == issuer.issuer),
                    "{PORTAL_ACCESS_KEYS_ENV} must include a key for every issuer in {ISSUERS_ENV}: {}",
                    issuer.issuer
                );
            }
        }
        Ok(Self {
            issuers,
            portal_access_keys,
            host_independent_issuer,
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
        let Some(issuers_env) = optional_env(ISSUERS_ENV)? else {
            bail!("{ISSUERS_ENV} is required");
        };
        let issuers = parse_issuer_list(&issuers_env)?;
        let portal_access_keys = match optional_env(PORTAL_ACCESS_KEYS_ENV)? {
            Some(value) if value.trim().is_empty() => {
                bail!("{PORTAL_ACCESS_KEYS_ENV} must not be empty when set")
            }
            Some(value) => parse_portal_access_keys(&value, &issuers)?,
            None => Vec::new(),
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
            portal_access_keys,
            private_key_pem,
            allow_insecure_web_loopback_redirects,
            false,
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
        if self.host_independent_issuer {
            return self.issuers.first().map(OidcIssuer::as_str);
        }
        let host = normalize_host(host?)?;
        self.issuers
            .iter()
            .find(|issuer| issuer.host == host)
            .map(OidcIssuer::as_str)
    }

    pub fn requires_portal_key(&self) -> bool {
        !self.portal_access_keys.is_empty()
    }

    pub fn issuer_for_portal_key(&self, key: &str) -> Option<&str> {
        let mut matched = None;
        for entry in &self.portal_access_keys {
            if constant_time_eq(entry.key.as_bytes(), key.as_bytes()) {
                matched = Some(entry.issuer.as_str());
            }
        }
        matched
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
    const ACCESS_KEY_A: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const ACCESS_KEY_B: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn portal_access_keys_bind_each_issuer_to_one_key() {
        let config = OidcConfig::from_issuers_with_portal_access_keys(
            &["https://auth-a.example.com", "https://auth-b.example.com"],
            &[
                ("https://auth-a.example.com", ACCESS_KEY_A),
                ("https://auth-b.example.com", ACCESS_KEY_B),
            ],
            PRIVATE_KEY.to_vec(),
        )
        .unwrap();

        assert!(config.requires_portal_key());
        assert_eq!(
            config.issuer_for_portal_key(ACCESS_KEY_A),
            Some("https://auth-a.example.com")
        );
        assert_eq!(
            config.issuer_for_portal_key(ACCESS_KEY_B),
            Some("https://auth-b.example.com")
        );
        assert_eq!(config.issuer_for_portal_key("unknown"), None);
    }

    #[test]
    fn portal_access_keys_must_cover_every_issuer() {
        assert!(
            OidcConfig::from_issuers_with_portal_access_keys(
                &["https://auth-a.example.com", "https://auth-b.example.com"],
                &[("https://auth-a.example.com", ACCESS_KEY_A)],
                PRIVATE_KEY.to_vec(),
            )
            .is_err()
        );
    }

    #[test]
    fn portal_access_keys_reject_unlisted_duplicate_and_short_entries() {
        let issuers = parse_issuer_list("https://auth-a.example.com").unwrap();
        assert!(
            parse_portal_access_keys(
                &format!("https://auth-b.example.com={ACCESS_KEY_A}"),
                &issuers
            )
            .is_err()
        );
        assert!(
            parse_portal_access_keys(
                &format!(
                    "https://auth-a.example.com={ACCESS_KEY_A},https://auth-a.example.com={ACCESS_KEY_B}"
                ),
                &issuers
            )
            .is_err()
        );
        assert!(
            parse_portal_access_keys(
                &format!("https://auth-a.example.com={ACCESS_KEY_A},https://auth-b.example.com={ACCESS_KEY_A}"),
                &issuers
            )
            .is_err()
        );
        assert!(
            parse_portal_access_keys("https://auth-a.example.com=short-key", &issuers).is_err()
        );
    }

    #[test]
    fn issuer_must_be_an_https_origin() {
        assert!(
            OidcConfig::fixed_issuer_for_tests("https://core.example.test", PRIVATE_KEY.to_vec())
                .is_ok()
        );
        for issuer in [
            "http://core.example.test",
            "https://core.example.test/path",
            "https://core.example.test?query=value",
            "https://core.example.test#fragment",
            "https://user@core.example.test",
        ] {
            assert!(
                OidcConfig::fixed_issuer_for_tests(issuer, PRIVATE_KEY.to_vec()).is_err(),
                "unexpectedly accepted {issuer}"
            );
        }
    }

    #[test]
    fn fixed_protocol_ttls_are_exposed() {
        let config =
            OidcConfig::fixed_issuer_for_tests("https://core.example.test", PRIVATE_KEY.to_vec())
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
                "https://AUTH-A.EXAMPLE.COM.:443",
                "https://auth-b.example.com",
                "https://auth-c.example.com:18086",
            ],
            PRIVATE_KEY.to_vec(),
            false,
        )
        .unwrap();

        assert_eq!(
            config.issuer_for_host(Some("AUTH-A.EXAMPLE.COM.:443")),
            Some("https://auth-a.example.com")
        );
        assert_eq!(
            config.issuer_for_host(Some("auth-b.example.com")),
            Some("https://auth-b.example.com")
        );
        assert_eq!(
            config.issuer_for_host(Some("auth-c.example.com:18086")),
            Some("https://auth-c.example.com:18086")
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
                &[
                    "https://auth-a.example.com",
                    "https://AUTH-A.EXAMPLE.COM.:443",
                ],
                PRIVATE_KEY.to_vec(),
                false,
            )
            .is_err()
        );
        assert!(OidcConfig::from_issuers(&[], PRIVATE_KEY.to_vec(), false).is_err());
    }

    #[test]
    fn fixed_test_issuer_ignores_request_host() {
        let config =
            OidcConfig::fixed_issuer_for_tests("https://core.example.test", PRIVATE_KEY.to_vec())
                .unwrap();

        assert_eq!(
            config.issuer_for_host(Some("attacker.example.test")),
            Some("https://core.example.test")
        );
        assert_eq!(
            config.issuer_for_host(None),
            Some("https://core.example.test")
        );
    }
}

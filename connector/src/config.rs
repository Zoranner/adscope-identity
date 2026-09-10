use std::fmt;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;
use url::Url;

#[derive(Clone, PartialEq, Eq)]
pub struct ConnectorProcessConfig {
    pub center_url: String,
    pub domain_id: String,
    pub connector_key: String,
    pub state_path: String,
    pub interval_seconds: u64,
    pub http_timeout_seconds: u64,
    pub operation_timeout_seconds: u64,
    pub ldap: LdapDirectoryConfig,
}

#[derive(Clone, PartialEq, Eq)]
pub struct LdapDirectoryConfig {
    pub url: String,
    pub server_fqdn: String,
    pub adopt_existing_users_by_username: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorInstallConfig {
    pub center_url: String,
    pub directory_domain_id: String,
    pub connector_access_key: String,
    pub domain_controller_ldap_url: String,
    pub match_legacy_users_by_username: bool,
}

const DEFAULT_STATE_PATH: &str = "adscope-connector-state.json";
const DEFAULT_TIMEOUT_SECONDS: u64 = 60;

pub fn write_install_config(
    runtime_dir: impl AsRef<Path>,
    config: &ConnectorInstallConfig,
) -> anyhow::Result<()> {
    validate_install_config(config)?;
    let runtime_dir = runtime_dir.as_ref();
    fs::create_dir_all(runtime_dir)?;

    let environment = format!(
        "CENTER_URL={}\nCENTER_REQUEST_TIMEOUT_SECONDS={DEFAULT_TIMEOUT_SECONDS}\nDIRECTORY_DOMAIN_ID={}\nCONNECTOR_ACCESS_KEY={}\nSYNC_INTERVAL_SECONDS={DEFAULT_TIMEOUT_SECONDS}\nDIRECTORY_OPERATION_TIMEOUT_SECONDS={DEFAULT_TIMEOUT_SECONDS}\nDOMAIN_CONTROLLER_LDAP_URL={}\nMATCH_LEGACY_USERS_BY_USERNAME={}\n",
        config.center_url,
        config.directory_domain_id,
        config.connector_access_key,
        config.domain_controller_ldap_url,
        u8::from(config.match_legacy_users_by_username),
    );
    let environment_path = runtime_dir.join(".env");
    let temporary_path = runtime_dir.join(".env.new");
    fs::write(&temporary_path, environment)?;
    fs::rename(temporary_path, environment_path)?;
    Ok(())
}

pub fn protect_runtime_config(runtime_dir: impl AsRef<Path>) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let runtime_dir = runtime_dir.as_ref();
        let environment_path = runtime_dir.join(".env");
        for (path, arguments) in [
            (
                runtime_dir,
                vec![
                    "/inheritance:r".to_string(),
                    "/grant:r".to_string(),
                    "*S-1-5-18:(OI)(CI)F".to_string(),
                    "*S-1-5-32-544:(OI)(CI)F".to_string(),
                    "*S-1-5-20:(OI)(CI)M".to_string(),
                ],
            ),
            (
                environment_path.as_path(),
                vec![
                    "/inheritance:r".to_string(),
                    "/grant:r".to_string(),
                    "*S-1-5-18:F".to_string(),
                    "*S-1-5-32-544:F".to_string(),
                    "*S-1-5-20:R".to_string(),
                ],
            ),
        ] {
            let status = Command::new("icacls.exe")
                .arg(path)
                .args(arguments)
                .status()?;
            if !status.success() {
                anyhow::bail!("icacls.exe failed while protecting {}", path.display());
            }
        }
    }
    Ok(())
}

impl ConnectorProcessConfig {
    pub fn new(
        center_url: impl Into<String>,
        domain_id: impl Into<String>,
        connector_key: impl Into<String>,
        state_path: impl Into<String>,
        interval_seconds: u64,
        ldap: LdapDirectoryConfig,
    ) -> Self {
        Self {
            center_url: center_url.into(),
            domain_id: domain_id.into(),
            connector_key: connector_key.into(),
            state_path: state_path.into(),
            interval_seconds,
            http_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            operation_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            ldap,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        reject_retired_environment_variables()?;
        let center_url = required_env("CENTER_URL")?;
        let domain_id = required_env("DIRECTORY_DOMAIN_ID")?;
        let connector_key = required_env("CONNECTOR_ACCESS_KEY")?;
        let interval_seconds = positive_seconds("SYNC_INTERVAL_SECONDS", 60)?;
        let http_timeout_seconds =
            positive_seconds("CENTER_REQUEST_TIMEOUT_SECONDS", DEFAULT_TIMEOUT_SECONDS)?;
        let operation_timeout_seconds = positive_seconds(
            "DIRECTORY_OPERATION_TIMEOUT_SECONDS",
            DEFAULT_TIMEOUT_SECONDS,
        )?;
        if !has_https_scheme(&center_url) {
            anyhow::bail!("CENTER_URL must use https://");
        }
        let url = required_env("DOMAIN_CONTROLLER_LDAP_URL")?;
        let server_fqdn = parse_ldap_server_fqdn(&url)?;
        let ldap = LdapDirectoryConfig {
            url,
            server_fqdn,
            adopt_existing_users_by_username: std::env::var("MATCH_LEGACY_USERS_BY_USERNAME")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        };

        let mut config = Self::new(
            center_url,
            domain_id,
            connector_key,
            DEFAULT_STATE_PATH,
            interval_seconds,
            ldap,
        );
        config.http_timeout_seconds = http_timeout_seconds;
        config.operation_timeout_seconds = operation_timeout_seconds;
        Ok(config)
    }
}

impl fmt::Debug for ConnectorProcessConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectorProcessConfig")
            .field("center_url", &self.center_url)
            .field("domain_id", &self.domain_id)
            .field("connector_key", &"[redacted]")
            .field("state_path", &self.state_path)
            .field("interval_seconds", &self.interval_seconds)
            .field("http_timeout_seconds", &self.http_timeout_seconds)
            .field("operation_timeout_seconds", &self.operation_timeout_seconds)
            .field("ldap", &self.ldap)
            .finish()
    }
}

impl fmt::Debug for LdapDirectoryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LdapDirectoryConfig")
            .field("url", &self.url)
            .field("server_fqdn", &self.server_fqdn)
            .field(
                "adopt_existing_users_by_username",
                &self.adopt_existing_users_by_username,
            )
            .finish()
    }
}

fn positive_seconds(name: &'static str, default: u64) -> anyhow::Result<u64> {
    let value = std::env::var(name)
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(default);
    if value == 0 {
        anyhow::bail!("{name} must be greater than 0");
    }
    Ok(value)
}

fn has_https_scheme(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

fn reject_retired_environment_variables() -> anyhow::Result<()> {
    for (retired, replacement) in [
        ("ADSS_CENTER_URL", Some("CENTER_URL")),
        ("ADSCOPE_CENTER_URL", Some("CENTER_URL")),
        ("ADSS_DOMAIN_ID", Some("DIRECTORY_DOMAIN_ID")),
        ("ADSCOPE_DOMAIN_ID", Some("DIRECTORY_DOMAIN_ID")),
        ("ADSS_CONNECTOR_KEY", Some("CONNECTOR_ACCESS_KEY")),
        ("ADSCOPE_CONNECTOR_KEY", Some("CONNECTOR_ACCESS_KEY")),
        (
            "ADSS_CONNECTOR_INTERVAL_SECONDS",
            Some("SYNC_INTERVAL_SECONDS"),
        ),
        (
            "ADSCOPE_CONNECTOR_INTERVAL_SECONDS",
            Some("SYNC_INTERVAL_SECONDS"),
        ),
        (
            "ADSS_CONNECTOR_HTTP_TIMEOUT_SECONDS",
            Some("CENTER_REQUEST_TIMEOUT_SECONDS"),
        ),
        (
            "ADSCOPE_CONNECTOR_HTTP_TIMEOUT_SECONDS",
            Some("CENTER_REQUEST_TIMEOUT_SECONDS"),
        ),
        (
            "ADSS_CONNECTOR_OPERATION_TIMEOUT_SECONDS",
            Some("DIRECTORY_OPERATION_TIMEOUT_SECONDS"),
        ),
        (
            "ADSCOPE_CONNECTOR_OPERATION_TIMEOUT_SECONDS",
            Some("DIRECTORY_OPERATION_TIMEOUT_SECONDS"),
        ),
        ("ADSS_CONNECTOR_DRY_RUN", None),
        ("ADSCOPE_CONNECTOR_DRY_RUN", None),
        ("ADSS_LDAP_URL", Some("DOMAIN_CONTROLLER_LDAP_URL")),
        ("ADSCOPE_LDAP_URL", Some("DOMAIN_CONTROLLER_LDAP_URL")),
        (
            "ADSS_ADOPT_EXISTING_USERS_BY_USERNAME",
            Some("MATCH_LEGACY_USERS_BY_USERNAME"),
        ),
        (
            "ADSCOPE_ADOPT_EXISTING_USERS_BY_USERNAME",
            Some("MATCH_LEGACY_USERS_BY_USERNAME"),
        ),
        ("ADSS_LDAP_BIND_DN", None),
        ("ADSCOPE_LDAP_BIND_DN", None),
        ("ADSS_LDAP_BIND_PASSWORD", None),
        ("ADSCOPE_LDAP_BIND_PASSWORD", None),
        ("ADSS_LDAP_ACCEPT_INVALID_CERTS", None),
        ("ADSCOPE_LDAP_ACCEPT_INVALID_CERTS", None),
    ] {
        if std::env::var_os(retired).is_some() {
            match replacement {
                Some(replacement) => anyhow::bail!("{retired} is retired; use {replacement}"),
                None => anyhow::bail!("{retired} is retired; no replacement is available"),
            }
        }
    }
    Ok(())
}

fn parse_ldap_server_fqdn(raw_url: &str) -> anyhow::Result<String> {
    let url = Url::parse(raw_url)
        .map_err(|_| anyhow::anyhow!("DOMAIN_CONTROLLER_LDAP_URL must be ldap://<FQDN>:389"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("DOMAIN_CONTROLLER_LDAP_URL must be ldap://<FQDN>:389"))?;
    let is_fqdn = host.contains('.')
        && host.parse::<IpAddr>().is_err()
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    if url.scheme() != "ldap"
        || url.port() != Some(389)
        || !url.username().is_empty()
        || url.password().is_some()
        || !(url.path().is_empty() || url.path() == "/")
        || url.query().is_some()
        || url.fragment().is_some()
        || !is_fqdn
    {
        anyhow::bail!("DOMAIN_CONTROLLER_LDAP_URL must be ldap://<FQDN>:389");
    }
    Ok(host.to_ascii_lowercase())
}

fn required_env(name: &'static str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required"))
}

fn validate_install_config(config: &ConnectorInstallConfig) -> anyhow::Result<()> {
    for (name, value) in [
        ("CENTER_URL", config.center_url.as_str()),
        ("DIRECTORY_DOMAIN_ID", config.directory_domain_id.as_str()),
        ("CONNECTOR_ACCESS_KEY", config.connector_access_key.as_str()),
        (
            "DOMAIN_CONTROLLER_LDAP_URL",
            config.domain_controller_ldap_url.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            anyhow::bail!("{name} must not be empty");
        }
        if value.contains(['\r', '\n']) {
            anyhow::bail!("{name} must not contain a line break");
        }
    }
    if !has_https_scheme(&config.center_url) {
        anyhow::bail!("CENTER_URL must use https://");
    }
    parse_ldap_server_fqdn(&config.domain_controller_ldap_url)?;
    Ok(())
}

pub(crate) fn validate_ldap_attribute_name(name: &str) -> anyhow::Result<()> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        anyhow::bail!("LDAP attribute name contains unsupported characters: {name}")
    }
}

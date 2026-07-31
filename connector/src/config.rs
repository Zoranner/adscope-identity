#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorProcessConfig {
    pub center_url: String,
    pub domain_id: String,
    pub connector_key: String,
    pub state_path: String,
    pub interval_seconds: u64,
    pub dry_run: bool,
    pub ldap: Option<LdapDirectoryConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapDirectoryConfig {
    pub url: String,
    pub bind_dn: String,
    pub bind_password: String,
    pub accept_invalid_certs: bool,
    pub adopt_existing_users_by_username: bool,
}

const DEFAULT_STATE_PATH: &str = "adss-connector-state.json";

impl ConnectorProcessConfig {
    pub fn new(
        center_url: impl Into<String>,
        domain_id: impl Into<String>,
        connector_key: impl Into<String>,
        state_path: impl Into<String>,
        interval_seconds: u64,
        dry_run: bool,
        ldap: Option<LdapDirectoryConfig>,
    ) -> Self {
        Self {
            center_url: center_url.into(),
            domain_id: domain_id.into(),
            connector_key: connector_key.into(),
            state_path: state_path.into(),
            interval_seconds,
            dry_run,
            ldap,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let center_url = std::env::var("ADSS_CENTER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let domain_id = std::env::var("ADSS_DOMAIN_ID")?;
        let connector_key = std::env::var("ADSS_CONNECTOR_KEY")?;
        let interval_seconds = std::env::var("ADSS_CONNECTOR_INTERVAL_SECONDS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(60);
        if interval_seconds == 0 {
            anyhow::bail!("ADSS_CONNECTOR_INTERVAL_SECONDS must be greater than 0");
        }
        let dry_run = std::env::var("ADSS_CONNECTOR_DRY_RUN")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let ldap = if dry_run {
            None
        } else {
            let url = required_env("ADSS_LDAP_URL")?;
            if !has_ldap_scheme(&url) {
                anyhow::bail!("ADSS_LDAP_URL must use ldap:// or ldaps://");
            }
            Some(LdapDirectoryConfig {
                url,
                bind_dn: required_env("ADSS_LDAP_BIND_DN")?,
                bind_password: required_env("ADSS_LDAP_BIND_PASSWORD")?,
                accept_invalid_certs: std::env::var("ADSS_LDAP_ACCEPT_INVALID_CERTS")
                    .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                    .unwrap_or(false),
                adopt_existing_users_by_username: std::env::var(
                    "ADSS_ADOPT_EXISTING_USERS_BY_USERNAME",
                )
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            })
        };

        Ok(Self::new(
            center_url,
            domain_id,
            connector_key,
            DEFAULT_STATE_PATH,
            interval_seconds,
            dry_run,
            ldap,
        ))
    }
}

fn has_ldap_scheme(url: &str) -> bool {
    url.get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("ldap://"))
        || url
            .get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("ldaps://"))
}

fn required_env(name: &'static str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required"))
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

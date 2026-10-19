pub const DEFAULT_DATABASE_URL: &str = "sqlite://./data/adscope.db?mode=rwc";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CenterConfig {
    pub bind_addr: String,
}

impl CenterConfig {
    pub fn from_bind_addr(bind_addr: Option<String>) -> Self {
        Self {
            bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        reject_retired_environment_variables()?;
        Ok(Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
        })
    }
}

pub(crate) fn reject_retired_environment_variables() -> anyhow::Result<()> {
    for (retired, replacement) in [
        ("ADSS_BIND_ADDR", "BIND_ADDR"),
        ("ADSCOPE_BIND_ADDR", "BIND_ADDR"),
        ("ADSS_PASSWORD_ENCRYPTION_KEY", "PASSWORD_ENCRYPTION_KEY"),
        ("ADSCOPE_PASSWORD_ENCRYPTION_KEY", "PASSWORD_ENCRYPTION_KEY"),
        ("ADSS_PASSWORD_HASH_PROVIDER", "PASSWORD_HASH_PROVIDER"),
        ("ADSCOPE_PASSWORD_HASH_PROVIDER", "PASSWORD_HASH_PROVIDER"),
        ("ADSS_USER_SESSION_KEY", "SESSION_KEY"),
        ("ADSCOPE_USER_SESSION_KEY", "SESSION_KEY"),
        ("ADSS_USER_SESSION_TTL_SECONDS", "SESSION_TTL_SECONDS"),
        ("ADSCOPE_USER_SESSION_TTL_SECONDS", "SESSION_TTL_SECONDS"),
        ("ADSS_MANAGEMENT_TOKEN", "MANAGEMENT_TOKEN"),
        ("ADSCOPE_MANAGEMENT_TOKEN", "MANAGEMENT_TOKEN"),
        ("ADSS_OIDC_ISSUER", "OIDC_ISSUER"),
        ("ADSCOPE_OIDC_ISSUER", "OIDC_ISSUER"),
        (
            "ADSS_OIDC_PRIVATE_KEY_FILE",
            "/run/secrets/oidc-private-key.pem",
        ),
        (
            "ADSCOPE_OIDC_PRIVATE_KEY_FILE",
            "/run/secrets/oidc-private-key.pem",
        ),
        (
            "ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS",
            "OIDC_LOOPBACK_HTTP",
        ),
        (
            "ADSCOPE_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS",
            "OIDC_LOOPBACK_HTTP",
        ),
        (
            "OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS",
            "OIDC_LOOPBACK_HTTP",
        ),
    ] {
        if std::env::var_os(retired).is_some() {
            anyhow::bail!("{retired} is retired; use {replacement}");
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::DEFAULT_DATABASE_URL;

    #[test]
    fn default_database_is_kept_in_the_runtime_data_directory() {
        assert_eq!(DEFAULT_DATABASE_URL, "sqlite://./data/adscope.db?mode=rwc");
    }
}

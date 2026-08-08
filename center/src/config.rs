#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CenterConfig {
    pub bind_addr: String,
    pub database_url: Option<String>,
}

impl CenterConfig {
    pub fn from_bind_addr(bind_addr: Option<String>) -> Self {
        Self {
            bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
            database_url: None,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        reject_retired_environment_variables()?;
        Ok(Self {
            bind_addr: std::env::var("ADSCOPE_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: std::env::var("ADSCOPE_DATABASE_URL").ok(),
        })
    }
}

pub(crate) fn reject_retired_environment_variables() -> anyhow::Result<()> {
    for (retired, replacement) in [
        ("ADSS_BIND_ADDR", "ADSCOPE_BIND_ADDR"),
        ("ADSS_DATABASE_URL", "ADSCOPE_DATABASE_URL"),
        ("ADSS_WEB_ROOT", "ADSCOPE_WEB_ROOT"),
        (
            "ADSS_PASSWORD_ENCRYPTION_KEY",
            "ADSCOPE_PASSWORD_ENCRYPTION_KEY",
        ),
        (
            "ADSS_PASSWORD_HASH_PROVIDER",
            "ADSCOPE_PASSWORD_HASH_PROVIDER",
        ),
        ("ADSS_USER_SESSION_KEY", "ADSCOPE_USER_SESSION_KEY"),
        (
            "ADSS_USER_SESSION_TTL_SECONDS",
            "ADSCOPE_USER_SESSION_TTL_SECONDS",
        ),
        ("ADSS_MANAGEMENT_TOKEN", "ADSCOPE_MANAGEMENT_TOKEN"),
        ("ADSS_OIDC_ISSUER", "ADSCOPE_OIDC_ISSUER"),
        (
            "ADSS_OIDC_PRIVATE_KEY_FILE",
            "ADSCOPE_OIDC_PRIVATE_KEY_FILE",
        ),
        (
            "ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS",
            "ADSCOPE_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS",
        ),
    ] {
        if std::env::var_os(retired).is_some() {
            anyhow::bail!("{retired} is retired; use {replacement}");
        }
    }
    Ok(())
}

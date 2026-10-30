const DATABASE_URL_ENV: &str = "DATABASE_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreConfig {
    pub bind_addr: String,
    pub database_url: String,
}

impl CoreConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        reject_retired_environment_variables()?;
        Ok(Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: parse_database_url(std::env::var(DATABASE_URL_ENV).ok().as_deref())?,
        })
    }
}

fn parse_database_url(value: Option<&str>) -> anyhow::Result<String> {
    let value = value.map(str::trim).unwrap_or_default();
    if value.is_empty() {
        anyhow::bail!(
            "{DATABASE_URL_ENV} is required; set it to a PostgreSQL connection URL such as \
             postgres://adscope:<password>@postgres:5432/adscope"
        );
    }
    if !(value.starts_with("postgres://") || value.starts_with("postgresql://")) {
        anyhow::bail!("{DATABASE_URL_ENV} must use the postgres:// or postgresql:// scheme");
    }
    Ok(value.to_string())
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
    use super::parse_database_url;

    #[test]
    fn database_url_must_be_a_postgres_url() {
        assert_eq!(
            parse_database_url(Some(" postgres://adscope:secret@postgres:5432/adscope ")).unwrap(),
            "postgres://adscope:secret@postgres:5432/adscope"
        );
        assert_eq!(
            parse_database_url(Some("postgresql://adscope:secret@postgres:5432/adscope")).unwrap(),
            "postgresql://adscope:secret@postgres:5432/adscope"
        );
        assert!(parse_database_url(Some("sqlite://./data/adscope.db?mode=rwc")).is_err());
        assert!(parse_database_url(Some("   ")).is_err());
        assert!(parse_database_url(None).is_err());
    }
}

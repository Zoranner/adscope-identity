#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub database_url: Option<String>,
}

impl ServerConfig {
    pub fn from_bind_addr(bind_addr: Option<String>) -> Self {
        Self {
            bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
            database_url: None,
        }
    }

    pub fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("ADSS_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: std::env::var("ADSS_DATABASE_URL").ok(),
        }
    }
}

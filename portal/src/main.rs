use adscope_portal::{AppState, Config, ListenerTls, router};
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install rustls crypto provider"))?;

    let config = Config::from_env()?;
    let state = AppState::new(&config)?;
    let listen_addr = config.listen_addr;
    let tls = config.tls.clone();

    tracing::info!(
        %listen_addr,
        core_url = %config.core_url,
        public_issuer = config.public_issuer.origin(),
        login_enabled = config.login_enabled,
        admin_enabled = config.admin_enabled,
        "starting Adscope Portal"
    );

    match tls {
        ListenerTls::Https {
            cert_file,
            key_file,
        } => {
            let tls = RustlsConfig::from_pem_file(&cert_file, &key_file)
                .await
                .context("failed to load TLS certificate or key")?;
            axum_server::bind_rustls(listen_addr, tls)
                .serve(router(state).into_make_service())
                .await?;
        }
        ListenerTls::Http => {
            axum_server::bind(listen_addr)
                .serve(router(state).into_make_service())
                .await?;
        }
    }
    Ok(())
}

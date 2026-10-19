use adscope_gateway::{AppState, Config, router};
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
    let tls = RustlsConfig::from_pem_file(&config.tls_cert_file, &config.tls_key_file)
        .await
        .context("failed to load TLS certificate or key")?;
    let state = AppState::new(&config)?;
    let listen_addr = config.listen_addr;

    tracing::info!(
        %listen_addr,
        center_url = %config.center_url,
        login_enabled = config.login_enabled,
        admin_enabled = config.admin_enabled,
        "starting Adscope gateway"
    );

    axum_server::bind_rustls(listen_addr, tls)
        .serve(router(state).into_make_service())
        .await?;
    Ok(())
}

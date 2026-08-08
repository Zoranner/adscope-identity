use adscope_center::{AppState, CenterConfig, build_router, load_env_file};
use adscope_store::Repository;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env_file(".env")?;
    let config = CenterConfig::from_env();
    let database_url = config
        .database_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("ADSS_DATABASE_URL is required for the center service"))?;
    let repository = Repository::connect(database_url).await?;
    repository.initialize_schema().await?;
    let state = AppState::from_env(repository)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

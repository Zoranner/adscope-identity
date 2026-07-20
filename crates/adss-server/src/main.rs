use adss_persistence::Repository;
use adss_server::{AppState, ServerConfig, build_router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();
    let database_url = config
        .database_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("ADSS_DATABASE_URL is required for the MVP server"))?;
    let repository = Repository::connect(database_url).await?;
    repository.initialize_schema().await?;
    let state = AppState::from_env(repository)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

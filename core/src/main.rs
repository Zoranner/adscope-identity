use adscope_core::{AppState, CoreConfig, build_router, load_env_file};
use adscope_store::Repository;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env_file(".env")?;
    let config = CoreConfig::from_env()?;
    let repository = Repository::connect(&config.database_url).await?;
    repository.initialize_schema().await?;
    let state = AppState::from_env(repository)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

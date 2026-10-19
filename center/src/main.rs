use adscope_center::{AppState, CenterConfig, DEFAULT_DATABASE_URL, build_router, load_env_file};
use adscope_store::Repository;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env_file(".env")?;
    let config = CenterConfig::from_env()?;
    std::fs::create_dir_all("data")?;
    let repository = Repository::connect(DEFAULT_DATABASE_URL).await?;
    repository.initialize_schema().await?;
    let state = AppState::from_env(repository)?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

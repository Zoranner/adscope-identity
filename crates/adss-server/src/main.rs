use adss_persistence::OrmRepository;
use adss_server::{AppState, ServerConfig, build_router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();
    let state = if let Some(database_url) = &config.database_url {
        let repository = OrmRepository::connect(database_url).await?;
        repository.initialize_schema().await?;
        AppState::seeded_with_repository(repository).await?
    } else {
        AppState::seeded()
    };
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

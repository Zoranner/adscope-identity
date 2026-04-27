use adss_server::{AppState, ServerConfig, build_router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_bind_addr(std::env::var("ADSS_BIND_ADDR").ok());
    let listener = TcpListener::bind(&config.bind_addr).await?;
    axum::serve(listener, build_router(AppState::seeded())).await?;
    Ok(())
}

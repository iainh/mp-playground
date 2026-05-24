use mp_playground::build_app;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = build_app()?;
    let address: SocketAddr = format!("{}:{}", app.config.server.host, app.config.server.port)
        .parse()
        .map_err(|error| format!("invalid server address in configuration: {error}"))?;
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!("listening on http://{address}");

    axum::serve(listener, app.router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

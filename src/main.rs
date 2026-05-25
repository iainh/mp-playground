use mp_config_tracing::TracingConfig;
use mp_playground::{build_app_from_config, load_config_source};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_source = load_config_source()?;
    TracingConfig::from_config(&config_source)?.init()?;
    let app = build_app_from_config(config_source).await?;
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

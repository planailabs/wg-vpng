//! wg-vpng-node: a dumb WireGuard switch. It exposes a tiny HTTP control API
//! and applies whatever interface + peer state the main server pushes, using a
//! local backend (self-managed `wg`+`ip`, or NetworkManager). It stores no
//! policy and makes no decisions of its own.

use std::sync::Arc;

use wg_vpng_node::config::NodeConfig;
use wg_vpng_node::app;
use wg_backend::WireguardBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let path = std::env::args().nth(1).unwrap_or_else(|| "config.toml".to_string());
    let cfg = NodeConfig::load(&path)?;
    let backend: Arc<dyn WireguardBackend> =
        Arc::from(wg_backend::local_backend(&cfg.backend).map_err(|e| anyhow::anyhow!(e))?);

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, backend = %cfg.backend, "wg-vpng-node listening");
    axum::serve(listener, app(backend, Arc::from(cfg.api_key.as_str()))).await?;
    Ok(())
}

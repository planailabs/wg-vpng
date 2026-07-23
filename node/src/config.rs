//! Node config: the *only* things a node knows are where to listen, its API
//! key, and which local backend to apply with. It holds no interface state —
//! the server pushes that over HTTP.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    /// Address:port for the control API (e.g. `0.0.0.0:8787`).
    pub bind: String,
    /// Bearer key the server must present. Required and non-empty.
    pub api_key: String,
    /// Local backend to apply interfaces with: `self-managed` or
    /// `network-manager`.
    #[serde(default = "default_backend")]
    pub backend: String,
}

fn default_backend() -> String {
    "self-managed".to_string()
}

impl NodeConfig {
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read config {path}"))?;
        let cfg: NodeConfig = toml::from_str(&text).context("parse config")?;
        if cfg.api_key.trim().is_empty() {
            anyhow::bail!("api_key must be set and non-empty");
        }
        // Validate the backend kind up front so a typo fails at startup, not on
        // the first request.
        wg_backend::local_backend(&cfg.backend).map_err(|e| anyhow::anyhow!(e))?;
        Ok(cfg)
    }
}

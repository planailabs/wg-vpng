//! Server configuration, loaded once from `CONFIG_PATH` (default
//! `./config.toml`) into a process-global.

use std::sync::OnceLock;

use crate::backend::BackendConfig;

static CONFIG: OnceLock<ServerConfig> = OnceLock::new();

#[derive(Debug, serde::Deserialize)]
pub struct ServerConfig {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub web: WebConfig,
    /// OIDC auth. Optional so `DEV_ONLY_NO_AUTH=1` debug runs need no IdP.
    pub auth: Option<plan_ai_auth::AuthConfig>,
    pub wireguard: WireguardConfig,
}

#[derive(Debug, serde::Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_web_port")]
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self { port: default_web_port() }
    }
}

fn default_web_port() -> u16 {
    8080
}

/// The VPN the app manages. On first boot a `wg_interfaces` row is seeded from
/// these values (with a freshly generated keypair); afterwards the stored row
/// is authoritative so keys stay stable across restarts.
#[derive(Debug, serde::Deserialize)]
pub struct WireguardConfig {
    /// Which backend applies the config to the real world.
    pub backend: BackendConfig,
    #[serde(default = "default_iface_name")]
    pub interface_name: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    /// Server tunnel address with prefix, e.g. `10.8.0.1/24`.
    pub address: String,
    /// Public `host:port` clients dial.
    pub endpoint: String,
    /// DNS pushed to clients (optional).
    #[serde(default)]
    pub dns: Option<String>,
    /// Networks routed through the tunnel (client `AllowedIPs`).
    #[serde(default = "default_allowed_ips")]
    pub allowed_ips: String,
    #[serde(default = "default_keepalive")]
    pub keepalive: u16,
}

fn default_iface_name() -> String {
    "wg0".to_string()
}
fn default_listen_port() -> u16 {
    51820
}
fn default_allowed_ips() -> String {
    "10.8.0.0/24".to_string()
}
fn default_keepalive() -> u16 {
    25
}

pub fn load() -> &'static ServerConfig {
    CONFIG.get_or_init(|| {
        let path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "./config.toml".to_string());
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read config from {path}: {e}"));
        toml::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse config from {path}: {e}"))
    })
}

pub fn config() -> &'static ServerConfig {
    CONFIG.get().expect("config not loaded — call config::load() first")
}

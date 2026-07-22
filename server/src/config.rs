//! Server configuration, loaded once from `CONFIG_PATH` (default
//! `./config.toml`). Interfaces (and their backends) are managed entirely in
//! the admin UI, so config only carries the database, web, auth, and the app
//! encryption key.

use std::sync::OnceLock;

static CONFIG: OnceLock<ServerConfig> = OnceLock::new();

#[derive(Debug, serde::Deserialize)]
pub struct ServerConfig {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub web: WebConfig,
    /// OIDC auth. Optional so `DEV_ONLY_NO_AUTH=1` debug runs need no IdP.
    pub auth: Option<plan_ai_auth::AuthConfig>,
    /// App secrets (encryption key for stored credentials).
    pub secrets: Option<SecretsConfig>,
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

#[derive(Debug, serde::Deserialize)]
pub struct SecretsConfig {
    /// 32-byte key (base64 or hex) encrypting credentials stored in the DB.
    /// Generate with `openssl rand -base64 32`.
    pub encryption_key: String,
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

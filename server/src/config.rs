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
    /// Accept signed-in users' Zitadel access tokens on the API / MCP.
    pub zitadel_bearer: Option<ZitadelBearerConfig>,
}

/// `[zitadel_bearer]` — a plan.ai user's own Zitadel access token (e.g. the
/// chat acting as them by token exchange) as an API bearer. Zitadel tokens are
/// opaque: each is checked at `{issuer}/oauth/v2/introspect` (cached a
/// minute), must carry `project_id` in its audience, and acts as the wg-vpng
/// user with its email — exactly what that user may do in the web UI.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZitadelBearerConfig {
    /// e.g. `https://id.plan.ai`.
    pub issuer: String,
    /// The Zitadel project the token must be meant for.
    pub project_id: String,
    /// Client id + secret of that project's API application (introspection).
    pub client_id: String,
    pub client_secret: String,
    /// Create the user of a token with a verified email that has none yet, as
    /// a first web sign-in would (the `[[auth.providers]]` entry with this
    /// issuer decides who may). `false` → unknown users get 403.
    #[serde(default = "yes")]
    pub auto_create: bool,
}

fn yes() -> bool {
    true
}

impl std::fmt::Debug for ZitadelBearerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZitadelBearerConfig")
            .field("issuer", &self.issuer)
            .field("project_id", &self.project_id)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("auto_create", &self.auto_create)
            .finish()
    }
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

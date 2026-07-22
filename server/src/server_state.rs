//! Process-global handles shared by the Dioxus `#[server]` functions and the
//! plan-ai-api-mcp endpoints: the database pool and the selected WireGuard
//! backend.

use std::sync::OnceLock;

use sqlx::PgPool;

use crate::backend::WireguardBackend;

static POOL: OnceLock<PgPool> = OnceLock::new();
static BACKEND: OnceLock<Box<dyn WireguardBackend>> = OnceLock::new();

pub fn set_pool(pool: PgPool) {
    POOL.set(pool).expect("pool already initialized");
}

pub fn set_backend(backend: Box<dyn WireguardBackend>) {
    let _ = BACKEND.set(backend);
}

pub fn pool() -> Result<PgPool, dioxus::prelude::ServerFnError> {
    POOL.get()
        .cloned()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("database pool not initialized"))
}

/// Raw pool for non-Dioxus callers (api-mcp, background tasks).
pub fn pool_raw() -> Option<PgPool> {
    POOL.get().cloned()
}

pub fn backend() -> &'static dyn WireguardBackend {
    BACKEND
        .get()
        .map(|b| b.as_ref())
        .expect("backend not initialized")
}

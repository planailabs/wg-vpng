//! Process-global database pool, shared by the Dioxus `#[server]` functions and
//! the plan-ai-api-mcp endpoints. Backends are per-interface (built from the
//! interface row at sync time), so no backend is stored here.

use std::sync::OnceLock;

use sqlx::PgPool;

static POOL: OnceLock<PgPool> = OnceLock::new();

pub fn set_pool(pool: PgPool) {
    POOL.set(pool).expect("pool already initialized");
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

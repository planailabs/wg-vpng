//! Server-side helpers for reading the authenticated user inside `#[server]`
//! functions.

#![cfg(feature = "server")]

use dioxus::prelude::ServerFnError;
use plan_ai_auth::WebUser;

/// Extract the current authenticated user from the request extensions.
///
/// When compiled in debug with `DEV_ONLY_NO_AUTH=1` there is no OIDC layer and
/// thus no `WebUser` extension; fall back to a synthetic dev admin so the UI is
/// usable without an IdP. This path can never exist in a release binary.
pub async fn current_user() -> Result<WebUser, ServerFnError> {
    use dioxus::fullstack::axum::extract::Extension;
    match dioxus::fullstack::FullstackContext::extract::<Extension<WebUser>, _>().await {
        Ok(Extension(user)) => Ok(user),
        Err(e) => {
            if cfg!(debug_assertions) && std::env::var("DEV_ONLY_NO_AUTH").as_deref() == Ok("1") {
                return Ok(dev_user());
            }
            Err(ServerFnError::new(format!("not authenticated: {e}")))
        }
    }
}

fn dev_user() -> WebUser {
    WebUser {
        id: uuid::Uuid::nil(),
        email: "dev@localhost".into(),
        name: "Dev".into(),
        is_admin: true,
        org_memberships: vec![],
        impersonating_from: None,
    }
}

/// Ensure the current user exists in the DB and return its row id (peers are
/// keyed by user id). Idempotent upsert by email.
pub async fn current_user_id(pool: &sqlx::PgPool, user: &WebUser) -> Result<uuid::Uuid, ServerFnError> {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO users (email, name, is_admin) VALUES ($1,$2,$3) \
         ON CONFLICT (email) DO UPDATE SET name = EXCLUDED.name RETURNING id",
    )
    .bind(&user.email)
    .bind(&user.name)
    .bind(user.is_admin)
    .fetch_one(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))
}

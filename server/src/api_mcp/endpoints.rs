//! Endpoint handlers shared by REST + MCP. Each takes `(pool, principal,
//! input)` and returns a serializable output or an `ApiError`.
//!
//! wg-vpng's API is an admin/automation surface: reads require any valid
//! token, mutations require an admin token.

use plan_ai_api_mcp::{ApiError, Principal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::store;

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::internal(e.to_string())
}

async fn resolve_interface(pool: &PgPool, id: Option<Uuid>) -> Result<store::Interface, ApiError> {
    let iface = match id {
        Some(id) => store::get_interface(pool, id).await.map_err(internal)?,
        None => store::list_interfaces(pool)
            .await
            .map_err(internal)?
            .into_iter()
            .next(),
    };
    iface.ok_or_else(|| ApiError::not_found("interface not found"))
}

/// Reconcile the backend after a mutation (best-effort; surfaced as an error).
async fn sync(pool: &PgPool, interface_id: Uuid) -> Result<(), ApiError> {
    store::sync_interface(pool, crate::server_state::backend(), interface_id)
        .await
        .map_err(internal)
}

// ── Interfaces ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceListInput {}

pub async fn interface_list(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    _i: InterfaceListInput,
) -> Result<Vec<store::Interface>, ApiError> {
    store::list_interfaces(&pool).await.map_err(internal)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceGetInput {
    /// Interface id; omit to use the default (first) interface.
    #[serde(default)]
    pub id: Option<Uuid>,
}

pub async fn interface_get(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: InterfaceGetInput,
) -> Result<store::Interface, ApiError> {
    resolve_interface(&pool, i.id).await
}

// ── Peers ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerListInput {
    /// Filter to one interface; omit for the default interface.
    #[serde(default)]
    pub interface_id: Option<Uuid>,
}

pub async fn peer_list(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: PeerListInput,
) -> Result<Vec<store::Peer>, ApiError> {
    let iface = resolve_interface(&pool, i.interface_id).await?;
    store::list_peers(&pool, iface.id).await.map_err(internal)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerGetInput {
    pub id: Uuid,
}

pub async fn peer_get(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: PeerGetInput,
) -> Result<store::Peer, ApiError> {
    store::get_peer(&pool, i.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("peer not found"))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerCreateInput {
    /// Interface to attach to; omit for the default interface.
    #[serde(default)]
    pub interface_id: Option<Uuid>,
    /// Owner's email (upserted as a user). Omit for an unowned peer.
    #[serde(default)]
    pub user_email: Option<String>,
    /// Display label.
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn peer_create(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: PeerCreateInput,
) -> Result<store::Peer, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.interface_id).await?;
    let user_id = match &i.user_email {
        Some(email) => Some(upsert_user(&pool, email).await?),
        None => None,
    };
    let name = i.name.unwrap_or_default();
    let peer = store::create_peer(&pool, iface.id, user_id, &name)
        .await
        .map_err(internal)?;
    sync(&pool, iface.id).await?;
    Ok(peer)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerDeleteInput {
    pub id: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteOutput {
    pub deleted: bool,
}

pub async fn peer_delete(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: PeerDeleteInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    let peer = store::get_peer(&pool, i.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("peer not found"))?;
    store::delete_peer(&pool, i.id).await.map_err(internal)?;
    sync(&pool, peer.interface_id).await?;
    Ok(DeleteOutput { deleted: true })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerRegenerateInput {
    pub id: Uuid,
}

pub async fn peer_regenerate(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: PeerRegenerateInput,
) -> Result<store::Peer, ApiError> {
    p.require_admin()?;
    let peer = store::regenerate_peer(&pool, i.id).await.map_err(internal)?;
    sync(&pool, peer.interface_id).await?;
    Ok(peer)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerConfigInput {
    pub id: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConfigOutput {
    /// The rendered WireGuard client `.conf`.
    pub config: String,
}

pub async fn peer_config(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: PeerConfigInput,
) -> Result<ConfigOutput, ApiError> {
    let peer = store::get_peer(&pool, i.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("peer not found"))?;
    let iface = store::get_interface(&pool, peer.interface_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("interface not found"))?;
    Ok(ConfigOutput {
        config: store::render_peer_config(&iface, &peer),
    })
}

async fn upsert_user(pool: &PgPool, email: &str) -> Result<Uuid, ApiError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email) VALUES ($1) \
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(internal)
}

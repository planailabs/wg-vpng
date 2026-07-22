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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceUpdateInput {
    /// Interface id; omit to update the default (first) interface.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Public host:port clients dial.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// DNS pushed to clients; empty string clears it.
    #[serde(default)]
    pub dns: Option<String>,
    /// Networks routed through the tunnel (client AllowedIPs).
    #[serde(default)]
    pub allowed_ips: Option<String>,
    /// PersistentKeepalive seconds.
    #[serde(default)]
    pub keepalive: Option<i32>,
}

pub async fn interface_update(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: InterfaceUpdateInput,
) -> Result<store::Interface, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.id).await?;
    // Empty dns string clears; absent leaves unchanged.
    let dns = i.dns.as_ref().map(|s| if s.is_empty() { None } else { Some(s.as_str()) });
    store::update_interface(
        &pool,
        iface.id,
        i.endpoint.as_deref(),
        dns,
        i.allowed_ips.as_deref(),
        i.keepalive,
    )
    .await
    .map_err(internal)
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatusOutput {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub last_handshake: Option<i64>,
}

pub async fn interface_status(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: InterfaceGetInput,
) -> Result<Vec<StatusOutput>, ApiError> {
    let iface = resolve_interface(&pool, i.id).await?;
    let statuses = crate::server_state::backend()
        .status(&iface.name)
        .await
        .map_err(internal)?;
    Ok(statuses
        .into_iter()
        .map(|s| StatusOutput {
            public_key: s.public_key,
            endpoint: s.endpoint,
            last_handshake: s.last_handshake,
        })
        .collect())
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
pub struct PeerUpdateInput {
    pub id: Uuid,
    /// New device name.
    pub name: String,
}

pub async fn peer_update(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: PeerUpdateInput,
) -> Result<store::Peer, ApiError> {
    p.require_admin()?;
    store::rename_peer(&pool, i.id, &i.name).await.map_err(internal)
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

// ── Users (admin) ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
pub struct UserOutput {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub banned: bool,
    pub access_revoked: bool,
    pub device_limit: Option<i32>,
    pub device_count: i64,
}

async fn sync_all(pool: &PgPool) -> Result<(), ApiError> {
    crate::store::sync_all(pool, crate::server_state::backend())
        .await
        .map_err(internal)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserListInput {}

pub async fn user_list(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    _i: UserListInput,
) -> Result<Vec<UserOutput>, ApiError> {
    p.require_admin()?;
    let rows = crate::store::list_users_with_counts(&pool).await.map_err(internal)?;
    Ok(rows
        .into_iter()
        .map(|r| UserOutput {
            id: r.user.id,
            email: r.user.email,
            name: r.user.name,
            is_admin: r.user.is_admin,
            banned: r.user.banned,
            access_revoked: r.user.access_revoked,
            device_limit: r.user.device_limit,
            device_count: r.device_count,
        })
        .collect())
}

async fn user_output(pool: &PgPool, id: Uuid) -> Result<UserOutput, ApiError> {
    let user = crate::store::get_user(pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    let device_count = crate::store::count_user_devices(pool, id).await.map_err(internal)?;
    Ok(UserOutput {
        id: user.id,
        email: user.email,
        name: user.name,
        is_admin: user.is_admin,
        banned: user.banned,
        access_revoked: user.access_revoked,
        device_limit: user.device_limit,
        device_count,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserIdInput {
    pub id: Uuid,
}

pub async fn user_get(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserIdInput,
) -> Result<UserOutput, ApiError> {
    p.require_admin()?;
    user_output(&pool, i.id).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserCreateInput {
    pub email: String,
}

pub async fn user_create(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserCreateInput,
) -> Result<UserOutput, ApiError> {
    p.require_admin()?;
    let id = crate::store::upsert_user_by_email(&pool, &i.email).await.map_err(internal)?;
    user_output(&pool, id).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserBanInput {
    pub id: Uuid,
    pub banned: bool,
}

pub async fn user_ban(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserBanInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    crate::store::set_banned(&pool, i.id, i.banned).await.map_err(internal)?;
    sync_all(&pool).await?;
    Ok(DeleteOutput { deleted: i.banned })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserRevokeInput {
    pub id: Uuid,
    pub revoked: bool,
}

pub async fn user_revoke(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserRevokeInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    crate::store::set_access_revoked(&pool, i.id, i.revoked).await.map_err(internal)?;
    sync_all(&pool).await?;
    Ok(DeleteOutput { deleted: i.revoked })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserLimitInput {
    pub id: Uuid,
    /// New per-user device limit; omit/null to fall back to the global default.
    #[serde(default)]
    pub limit: Option<i32>,
}

pub async fn user_set_limit(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserLimitInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    crate::store::set_device_limit(&pool, i.id, i.limit).await.map_err(internal)?;
    Ok(DeleteOutput { deleted: true })
}

pub async fn user_delete(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserIdInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    crate::store::delete_user(&pool, i.id).await.map_err(internal)?;
    sync_all(&pool).await?;
    Ok(DeleteOutput { deleted: true })
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

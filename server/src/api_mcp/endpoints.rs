//! Endpoint handlers shared by REST + MCP. Each takes `(pool, principal,
//! input)` and returns a serializable output or an `ApiError`.
//!
//! wg-vpng's API is an admin/automation surface: reads require any valid
//! token, mutations require an admin token. Interfaces are admin-managed, each
//! with its own backend + pattern ACL + device limit.

use plan_ai_api_mcp::{ApiError, Principal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::backend::BackendConfig;
use crate::store;

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::internal(e.to_string())
}

/// Resolve an interface by id, or the sole interface when `id` is omitted.
async fn resolve_interface(pool: &PgPool, id: Option<Uuid>) -> Result<store::Interface, ApiError> {
    match id {
        Some(id) => store::get_interface(pool, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::not_found("interface not found")),
        None => {
            let mut all = store::list_interfaces(pool).await.map_err(internal)?;
            match all.len() {
                1 => Ok(all.remove(0)),
                0 => Err(ApiError::not_found("no interfaces configured")),
                _ => Err(ApiError::bad_request("multiple interfaces; specify interface_id")),
            }
        }
    }
}

async fn sync(pool: &PgPool, interface_id: Uuid) -> Result<(), ApiError> {
    store::sync_interface(pool, interface_id).await.map_err(internal)
}

async fn sync_all(pool: &PgPool) -> Result<(), ApiError> {
    store::sync_all(pool).await.map_err(internal)
}

// ── Backend input ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BackendInput {
    /// `self-managed`, `network-manager`, `mikrotik`, or `node`.
    pub kind: String,
    #[serde(default)]
    pub mikrotik_url: Option<String>,
    #[serde(default)]
    pub mikrotik_username: Option<String>,
    #[serde(default)]
    pub mikrotik_password: Option<String>,
    #[serde(default)]
    pub mikrotik_insecure: bool,
    /// Node control-API URL (node kind only).
    #[serde(default)]
    pub node_url: Option<String>,
    /// Node API key (node kind only); encrypted at rest.
    #[serde(default)]
    pub node_key: Option<String>,
}

impl BackendInput {
    fn build(self) -> Result<BackendConfig, ApiError> {
        // mikrotik + node both persist an encrypted secret.
        if (self.kind == "mikrotik" || self.kind == "node") && !crate::crypto::has_key() {
            return Err(ApiError::bad_request(
                "storing backend credentials requires [secrets] encryption_key in config",
            ));
        }
        BackendConfig::from_parts(
            &self.kind,
            self.mikrotik_url,
            self.mikrotik_username,
            self.mikrotik_password,
            self.mikrotik_insecure,
            self.node_url,
            self.node_key,
        )
        .map_err(ApiError::bad_request)
    }
}

// ── Interface output ──────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
pub struct InterfaceOutput {
    pub id: Uuid,
    /// WireGuard interface id (e.g. wg0) — unique, immutable.
    pub name: String,
    /// Human-friendly label; empty falls back to `name`.
    pub display_name: String,
    pub listen_port: i32,
    pub address: String,
    pub public_key: String,
    pub endpoint: String,
    pub dns: Option<String>,
    pub allowed_ips: String,
    pub keepalive: i32,
    pub device_limit: Option<i32>,
    pub group_ids: Vec<Uuid>,
    pub backend_kind: String,
    /// MikroTik url (no password), when applicable.
    pub mikrotik_url: Option<String>,
    pub mikrotik_username: Option<String>,
    pub mikrotik_insecure: Option<bool>,
    /// Node control-API url (no key), when applicable.
    pub node_url: Option<String>,
}

fn iface_output(i: &store::Interface) -> InterfaceOutput {
    let (url, user, insecure) = match i.backend.mikrotik_display() {
        Some((u, n, s)) => (Some(u), Some(n), Some(s)),
        None => (None, None, None),
    };
    InterfaceOutput {
        id: i.id,
        name: i.name.clone(),
        display_name: i.display_name.clone(),
        listen_port: i.listen_port,
        address: i.address.clone(),
        public_key: i.public_key.clone(),
        endpoint: i.endpoint.clone(),
        dns: i.dns.clone(),
        allowed_ips: i.allowed_ips.clone(),
        keepalive: i.keepalive,
        device_limit: i.device_limit,
        group_ids: i.group_ids.clone(),
        backend_kind: i.backend.kind().to_string(),
        mikrotik_url: url,
        mikrotik_username: user,
        mikrotik_insecure: insecure,
        node_url: i.backend.node_display(),
    }
}

// ── Interfaces ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceListInput {}

pub async fn interface_list(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    _i: InterfaceListInput,
) -> Result<Vec<InterfaceOutput>, ApiError> {
    Ok(store::list_interfaces(&pool)
        .await
        .map_err(internal)?
        .iter()
        .map(iface_output)
        .collect())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceGetInput {
    #[serde(default)]
    pub id: Option<Uuid>,
}

pub async fn interface_get(
    pool: PgPool,
    _p: std::sync::Arc<Principal>,
    i: InterfaceGetInput,
) -> Result<InterfaceOutput, ApiError> {
    Ok(iface_output(&resolve_interface(&pool, i.id).await?))
}

fn default_listen_port() -> i32 {
    51820
}
fn default_allowed_ips() -> String {
    "10.8.0.0/24, fd00:8::/64".to_string()
}
fn default_keepalive() -> i32 {
    25
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceCreateInput {
    /// WireGuard interface id (e.g. wg0) — unique, immutable.
    pub name: String,
    /// Human-friendly label; empty falls back to `name`.
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: i32,
    /// Server tunnel address(es), comma-separated (dual-stack; IPv6 enforced).
    pub address: String,
    /// Public host:port clients dial.
    pub endpoint: String,
    #[serde(default)]
    pub dns: Option<String>,
    #[serde(default = "default_allowed_ips")]
    pub allowed_ips: String,
    #[serde(default = "default_keepalive")]
    pub keepalive: i32,
    /// Max devices per user on this interface; null = unlimited.
    #[serde(default)]
    pub device_limit: Option<i32>,
    /// Groups whose members may use this interface (by id).
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
    pub backend: BackendInput,
}

pub async fn interface_create(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: InterfaceCreateInput,
) -> Result<InterfaceOutput, ApiError> {
    p.require_admin()?;
    let backend = i.backend.build()?;
    let iface = store::create_interface(
        &pool,
        &i.name,
        &i.display_name,
        i.listen_port,
        &i.address,
        &i.endpoint,
        i.dns.as_deref(),
        &i.allowed_ips,
        i.keepalive,
        i.device_limit,
        &i.group_ids,
        backend,
    )
    .await
    .map_err(internal)?;
    sync(&pool, iface.id).await?;
    Ok(iface_output(&iface))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfaceUpdateInput {
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Human-friendly label; empty falls back to `name`.
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Empty string clears DNS.
    #[serde(default)]
    pub dns: Option<String>,
    #[serde(default)]
    pub allowed_ips: Option<String>,
    #[serde(default)]
    pub keepalive: Option<i32>,
    #[serde(default)]
    pub device_limit: Option<i32>,
    /// Replace the assigned groups (real-time grant/revoke on sync).
    #[serde(default)]
    pub group_ids: Option<Vec<Uuid>>,
    /// Replace the backend.
    #[serde(default)]
    pub backend: Option<BackendInput>,
}

pub async fn interface_update(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: InterfaceUpdateInput,
) -> Result<InterfaceOutput, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.id).await?;
    let dns = i.dns.as_ref().map(|s| if s.is_empty() { None } else { Some(s.as_str()) });
    let backend = match i.backend {
        Some(b) => Some(b.build()?),
        None => None,
    };
    let updated = store::update_interface(
        &pool,
        iface.id,
        i.display_name.as_deref(),
        i.endpoint.as_deref(),
        dns,
        i.allowed_ips.as_deref(),
        i.keepalive,
        i.device_limit.map(Some),
        i.group_ids.as_deref(),
        backend,
    )
    .await
    .map_err(internal)?;
    // Patterns/backend may have changed access — re-sync now (real-time).
    sync(&pool, updated.id).await?;
    Ok(iface_output(&updated))
}

pub async fn interface_delete(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: InterfaceGetInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.id).await?;
    store::delete_interface(&pool, iface.id).await.map_err(internal)?;
    Ok(DeleteOutput { deleted: true })
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
    Ok(store::interface_status(&pool, iface.id)
        .await
        .map_err(internal)?
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
    #[serde(default)]
    pub interface_id: Option<Uuid>,
    /// Owner's email (upserted as a user). Omit for an unowned peer.
    #[serde(default)]
    pub user_email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Explicit address/subnet CIDR(s) (admin). Any prefix — grants a device a
    /// whole routed subnet (e.g. an IPv6 `/64`). Omit to auto-allocate.
    #[serde(default)]
    pub address: Option<String>,
    /// When false, create an *unconfigured* device (no key) so the user
    /// generates it themselves. Defaults to true.
    #[serde(default = "default_true")]
    pub generate_key: bool,
}

fn default_true() -> bool {
    true
}

/// A created/regenerated peer, including the private key + rendered config —
/// returned once (the private key is not stored).
#[derive(Debug, Serialize, JsonSchema)]
pub struct CreatedPeer {
    pub id: Uuid,
    pub interface_id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub public_key: String,
    pub address: String,
    /// Shown once; never stored server-side. Empty for an unconfigured device.
    pub private_key: String,
    pub config: String,
    /// False for an unconfigured device (no key yet).
    pub configured: bool,
}

fn created_peer(peer: &store::Peer, private_key: String, iface: &store::Interface) -> CreatedPeer {
    CreatedPeer {
        id: peer.id,
        interface_id: peer.interface_id,
        user_id: peer.user_id,
        name: peer.name.clone(),
        public_key: peer.public_key.clone(),
        address: peer.address.clone(),
        config: store::render_peer_config(iface, peer, &private_key),
        private_key,
        configured: true,
    }
}

pub async fn peer_create(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: PeerCreateInput,
) -> Result<CreatedPeer, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.interface_id).await?;
    let user_id = match &i.user_email {
        Some(email) => Some(store::upsert_user_by_email(&pool, email).await.map_err(internal)?),
        None => None,
    };
    let name = i.name.unwrap_or_default();
    let addr = i.address.as_deref().filter(|a| !a.is_empty());

    if !i.generate_key {
        let peer = store::create_peer_unconfigured(&pool, iface.id, user_id, &name, addr)
            .await
            .map_err(internal)?;
        return Ok(CreatedPeer {
            id: peer.id,
            interface_id: peer.interface_id,
            user_id: peer.user_id,
            name: peer.name,
            public_key: String::new(),
            address: peer.address,
            private_key: String::new(),
            config: String::new(),
            configured: false,
        });
    }

    let (peer, private_key) = match addr {
        Some(a) => store::create_peer_with_address(&pool, iface.id, user_id, &name, a).await,
        None => store::create_peer(&pool, iface.id, user_id, &name, false).await,
    }
    .map_err(internal)?;
    sync(&pool, iface.id).await?;
    Ok(created_peer(&peer, private_key, &iface))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerUpdateInput {
    pub id: Uuid,
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
) -> Result<CreatedPeer, ApiError> {
    p.require_admin()?;
    let (peer, private_key) = store::regenerate_peer(&pool, i.id).await.map_err(internal)?;
    sync(&pool, peer.interface_id).await?;
    let iface = store::get_interface(&pool, peer.interface_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("interface not found"))?;
    Ok(created_peer(&peer, private_key, &iface))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerConfigInput {
    pub id: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConfigOutput {
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
    // The private key isn't stored; re-rendered config uses a placeholder.
    Ok(ConfigOutput {
        config: store::render_peer_config(&iface, &peer, store::PRIVATE_KEY_PLACEHOLDER),
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
    pub device_count: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserListInput {}

pub async fn user_list(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    _i: UserListInput,
) -> Result<Vec<UserOutput>, ApiError> {
    p.require_admin()?;
    Ok(store::list_users_with_counts(&pool)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|r| UserOutput {
            id: r.user.id,
            email: r.user.email,
            name: r.user.name,
            is_admin: r.user.is_admin,
            banned: r.user.banned,
            access_revoked: r.user.access_revoked,
            device_count: r.device_count,
        })
        .collect())
}

async fn user_output(pool: &PgPool, id: Uuid) -> Result<UserOutput, ApiError> {
    let user = store::get_user(pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    let device_count: i64 = sqlx::query_scalar("SELECT count(*) FROM wg_peers WHERE user_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(internal)?;
    Ok(UserOutput {
        id: user.id,
        email: user.email,
        name: user.name,
        is_admin: user.is_admin,
        banned: user.banned,
        access_revoked: user.access_revoked,
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
    let id = store::upsert_user_by_email(&pool, &i.email).await.map_err(internal)?;
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
    store::set_banned(&pool, i.id, i.banned).await.map_err(internal)?;
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
    store::set_access_revoked(&pool, i.id, i.revoked).await.map_err(internal)?;
    sync_all(&pool).await?;
    Ok(DeleteOutput { deleted: i.revoked })
}

pub async fn user_delete(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: UserIdInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    store::delete_user(&pool, i.id).await.map_err(internal)?;
    sync_all(&pool).await?;
    Ok(DeleteOutput { deleted: true })
}

// ── Groups ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
pub struct GroupOutput {
    pub id: Uuid,
    pub name: String,
    /// Email globs / literal emails; `*` = everyone.
    pub patterns: Vec<String>,
    /// Values matched against a user's OIDC group claim.
    pub claim_values: Vec<String>,
}

fn group_output(g: &store::Group) -> GroupOutput {
    GroupOutput {
        id: g.id,
        name: g.name.clone(),
        patterns: g.patterns.clone(),
        claim_values: g.claim_values.clone(),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupListInput {}

pub async fn group_list(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    _i: GroupListInput,
) -> Result<Vec<GroupOutput>, ApiError> {
    p.require_admin()?;
    Ok(store::list_groups(&pool).await.map_err(internal)?.iter().map(group_output).collect())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupIdInput {
    pub id: Uuid,
}

pub async fn group_get(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: GroupIdInput,
) -> Result<GroupOutput, ApiError> {
    p.require_admin()?;
    let g = store::get_group(&pool, i.id).await.map_err(internal)?.ok_or_else(|| ApiError::not_found("group not found"))?;
    Ok(group_output(&g))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupCreateInput {
    pub name: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub claim_values: Vec<String>,
}

pub async fn group_create(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: GroupCreateInput,
) -> Result<GroupOutput, ApiError> {
    p.require_admin()?;
    let g = store::create_group(&pool, i.name.trim(), &i.patterns, &i.claim_values).await.map_err(internal)?;
    Ok(group_output(&g))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupUpdateInput {
    pub id: Uuid,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub patterns: Option<Vec<String>>,
    #[serde(default)]
    pub claim_values: Option<Vec<String>>,
}

pub async fn group_update(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: GroupUpdateInput,
) -> Result<GroupOutput, ApiError> {
    p.require_admin()?;
    let cur = store::get_group(&pool, i.id).await.map_err(internal)?.ok_or_else(|| ApiError::not_found("group not found"))?;
    let name = i.name.unwrap_or(cur.name);
    let patterns = i.patterns.unwrap_or(cur.patterns);
    let claim_values = i.claim_values.unwrap_or(cur.claim_values);
    let g = store::update_group(&pool, i.id, name.trim(), &patterns, &claim_values).await.map_err(internal)?;
    Ok(group_output(&g))
}

pub async fn group_delete(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: GroupIdInput,
) -> Result<DeleteOutput, ApiError> {
    p.require_admin()?;
    store::delete_group(&pool, i.id).await.map_err(internal)?;
    Ok(DeleteOutput { deleted: true })
}

/// Re-sync an interface to its backend now (re-assert desired state).
pub async fn interface_resync(
    pool: PgPool,
    p: std::sync::Arc<Principal>,
    i: InterfaceGetInput,
) -> Result<InterfaceOutput, ApiError> {
    p.require_admin()?;
    let iface = resolve_interface(&pool, i.id).await?;
    sync(&pool, iface.id).await?;
    let iface = resolve_interface(&pool, Some(iface.id)).await?;
    Ok(iface_output(&iface))
}

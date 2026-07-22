//! Dioxus server functions the UI calls. Signatures compile on both targets;
//! bodies run server-side only and return the plain DTOs from `super::dto`.

use dioxus::prelude::*;

use super::dto::{CurrentUser, DeviceQuota, InterfaceView, PeerView, UserAdminView};
use uuid::Uuid;

#[server]
pub async fn get_current_user() -> Result<CurrentUser, ServerFnError> {
    let u = crate::web::user::current_user().await?;
    Ok(CurrentUser { email: u.email, name: u.name, is_admin: u.is_admin })
}

#[server]
pub async fn get_interface() -> Result<InterfaceView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let iface = crate::store::list_interfaces(&pool)
        .await
        .map_err(err)?
        .into_iter()
        .next()
        .ok_or_else(|| ServerFnError::new("no interface configured"))?;
    Ok(iface_view(iface))
}

#[server]
pub async fn my_peers() -> Result<Vec<PeerView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;
    let peers = crate::store::list_peers_for_user(&pool, uid).await.map_err(err)?;
    Ok(peers.into_iter().map(|p| peer_view(p, Some(user.email.clone()))).collect())
}

#[server]
pub async fn all_peers() -> Result<Vec<PeerView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    if !user.is_admin {
        return Err(ServerFnError::new("admin access required"));
    }
    let iface = default_interface_id(&pool).await?;
    let peers = crate::store::list_peers(&pool, iface).await.map_err(err)?;
    // Attach owner email per peer.
    let mut out = Vec::with_capacity(peers.len());
    for p in peers {
        let email = match p.user_id {
            Some(uid) => sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
                .bind(uid)
                .fetch_optional(&pool)
                .await
                .map_err(err)?,
            None => None,
        };
        out.push(peer_view(p, email));
    }
    Ok(out)
}

/// Current user's device usage against their configured limit.
#[server]
pub async fn device_quota() -> Result<DeviceQuota, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;
    let used = crate::store::count_user_devices(&pool, uid).await.map_err(err)?;
    Ok(DeviceQuota { used, limit: effective_limit_for(&pool, uid).await? })
}

#[server]
pub async fn create_my_peer(name: String) -> Result<PeerView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;

    // Enforce access + device limit for self-service creation.
    let db_user = crate::store::get_user(&pool, uid).await.map_err(err)?;
    if db_user.as_ref().is_some_and(|u| u.banned || u.access_revoked) {
        return Err(ServerFnError::new("your access has been revoked"));
    }
    let used = crate::store::count_user_devices(&pool, uid).await.map_err(err)?;
    let limit = effective_limit_for(&pool, uid).await?;
    if used >= limit as i64 {
        return Err(ServerFnError::new(format!("device limit reached ({limit})")));
    }

    let iface = default_interface_id(&pool).await?;
    let peer = crate::store::create_peer(&pool, iface, Some(uid), &name).await.map_err(err)?;
    sync(&pool, iface).await?;
    Ok(peer_view(peer, Some(user.email)))
}

// ── Admin ─────────────────────────────────────────────────────────────

#[server]
pub async fn admin_list_users() -> Result<Vec<UserAdminView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let global = crate::config::config().wireguard.device_limit;
    let rows = crate::store::list_users_with_counts(&pool).await.map_err(err)?;
    Ok(rows
        .into_iter()
        .map(|r| UserAdminView {
            id: r.user.id,
            email: r.user.email.clone(),
            name: r.user.name.clone(),
            is_admin: r.user.is_admin,
            banned: r.user.banned,
            access_revoked: r.user.access_revoked,
            device_limit: r.user.device_limit,
            device_count: r.device_count,
            effective_limit: crate::store::effective_device_limit(&r.user, global),
        })
        .collect())
}

#[server]
pub async fn admin_user_devices(user_id: Uuid) -> Result<Vec<PeerView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(err)?;
    let peers = crate::store::list_peers_for_user(&pool, user_id).await.map_err(err)?;
    Ok(peers.into_iter().map(|p| peer_view(p, email.clone())).collect())
}

/// Admin creates a device for a user (bypasses the per-user limit). An optional
/// `address` assigns an explicit CIDR/subnet (any prefix — e.g. an IPv6 `/64`);
/// empty auto-allocates a host address in each interface subnet.
#[server]
pub async fn admin_create_device(
    user_id: Uuid,
    name: String,
    address: String,
) -> Result<PeerView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let iface = default_interface_id(&pool).await?;
    let addr = address.trim();
    let peer = if addr.is_empty() {
        crate::store::create_peer(&pool, iface, Some(user_id), &name).await
    } else {
        crate::store::create_peer_with_address(&pool, iface, Some(user_id), &name, addr).await
    }
    .map_err(err)?;
    sync(&pool, iface).await?;
    Ok(peer_view(peer, None))
}

#[server]
pub async fn admin_set_device_limit(user_id: Uuid, limit: Option<i32>) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    crate::store::set_device_limit(&pool, user_id, limit).await.map_err(err)
}

#[server]
pub async fn admin_set_banned(user_id: Uuid, banned: bool) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    crate::store::set_banned(&pool, user_id, banned).await.map_err(err)?;
    sync_all(&pool).await
}

#[server]
pub async fn admin_set_revoked(user_id: Uuid, revoked: bool) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    crate::store::set_access_revoked(&pool, user_id, revoked).await.map_err(err)?;
    sync_all(&pool).await
}

#[server]
pub async fn admin_delete_user(user_id: Uuid) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    crate::store::delete_user(&pool, user_id).await.map_err(err)?;
    sync_all(&pool).await
}

#[server]
pub async fn create_peer_for(email: String, name: String) -> Result<PeerView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let uid = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email) VALUES ($1) ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .map_err(err)?;
    let iface = default_interface_id(&pool).await?;
    let peer = crate::store::create_peer(&pool, iface, Some(uid), &name).await.map_err(err)?;
    sync(&pool, iface).await?;
    Ok(peer_view(peer, Some(email)))
}

#[server]
pub async fn regenerate_peer(id: Uuid) -> Result<PeerView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    authorize_peer(&pool, id).await?;
    let peer = crate::store::regenerate_peer(&pool, id).await.map_err(err)?;
    sync(&pool, peer.interface_id).await?;
    Ok(peer_view(peer, None))
}

#[server]
pub async fn delete_peer(id: Uuid) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    let peer = authorize_peer(&pool, id).await?;
    crate::store::delete_peer(&pool, id).await.map_err(err)?;
    sync(&pool, peer.interface_id).await?;
    Ok(())
}

#[server]
pub async fn peer_config_text(id: Uuid) -> Result<String, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let peer = authorize_peer(&pool, id).await?;
    let iface = crate::store::get_interface(&pool, peer.interface_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("interface not found"))?;
    Ok(crate::store::render_peer_config(&iface, &peer))
}

// ── server-only helpers ───────────────────────────────────────────────

#[cfg(feature = "server")]
fn err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[cfg(feature = "server")]
fn iface_view(i: crate::store::Interface) -> InterfaceView {
    InterfaceView {
        id: i.id,
        name: i.name,
        address: i.address,
        endpoint: i.endpoint,
        public_key: i.public_key,
        listen_port: i.listen_port,
        dns: i.dns,
        allowed_ips: i.allowed_ips,
    }
}

#[cfg(feature = "server")]
fn peer_view(p: crate::store::Peer, owner_email: Option<String>) -> PeerView {
    PeerView { id: p.id, name: p.name, address: p.address, public_key: p.public_key, owner_email }
}

#[cfg(feature = "server")]
async fn default_interface_id(pool: &sqlx::PgPool) -> Result<Uuid, ServerFnError> {
    crate::store::list_interfaces(pool)
        .await
        .map_err(err)?
        .into_iter()
        .next()
        .map(|i| i.id)
        .ok_or_else(|| ServerFnError::new("no interface configured"))
}

#[cfg(feature = "server")]
async fn require_admin(pool: &sqlx::PgPool) -> Result<(), ServerFnError> {
    let user = crate::web::user::current_user().await?;
    let _ = pool;
    if user.is_admin { Ok(()) } else { Err(ServerFnError::new("admin access required")) }
}

/// Load a peer and check the caller may act on it (owner or admin).
#[cfg(feature = "server")]
async fn authorize_peer(pool: &sqlx::PgPool, id: Uuid) -> Result<crate::store::Peer, ServerFnError> {
    let user = crate::web::user::current_user().await?;
    let peer = crate::store::get_peer(pool, id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("peer not found"))?;
    if user.is_admin {
        return Ok(peer);
    }
    let uid = crate::web::user::current_user_id(pool, &user).await?;
    if peer.user_id == Some(uid) {
        Ok(peer)
    } else {
        Err(ServerFnError::new("not your peer"))
    }
}

#[cfg(feature = "server")]
async fn sync(pool: &sqlx::PgPool, interface_id: Uuid) -> Result<(), ServerFnError> {
    crate::store::sync_interface(pool, crate::server_state::backend(), interface_id)
        .await
        .map_err(err)
}

#[cfg(feature = "server")]
async fn sync_all(pool: &sqlx::PgPool) -> Result<(), ServerFnError> {
    crate::store::sync_all(pool, crate::server_state::backend()).await.map_err(err)
}

/// A user's effective device limit (their override, else the global default).
#[cfg(feature = "server")]
async fn effective_limit_for(pool: &sqlx::PgPool, user_id: Uuid) -> Result<i32, ServerFnError> {
    let global = crate::config::config().wireguard.device_limit;
    let user = crate::store::get_user(pool, user_id).await.map_err(err)?;
    Ok(user.map(|u| crate::store::effective_device_limit(&u, global)).unwrap_or(global))
}

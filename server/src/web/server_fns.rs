//! Dioxus server functions the UI calls. Signatures compile on both targets;
//! bodies run server-side only and return the plain DTOs from `super::dto`.

use dioxus::prelude::*;

use super::dto::{
    CurrentUser, InterfaceAccessView, InterfaceAdminView, NewDeviceView, PeerView, UserAdminView,
};
use uuid::Uuid;

#[server]
pub async fn get_current_user() -> Result<CurrentUser, ServerFnError> {
    let u = crate::web::user::current_user().await?;
    Ok(CurrentUser { email: u.email, name: u.name, is_admin: u.is_admin })
}

// ── User-facing ───────────────────────────────────────────────────────

/// Interfaces the current user may use (per the pattern ACL), with usage.
#[server]
pub async fn list_my_interfaces() -> Result<Vec<InterfaceAccessView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;
    let ifaces = crate::store::interfaces_for_user(&pool, &user.email).await.map_err(err)?;
    let mut out = Vec::with_capacity(ifaces.len());
    for i in ifaces {
        let used = crate::store::count_user_devices_on_interface(&pool, i.id, uid).await.map_err(err)?;
        out.push(InterfaceAccessView { id: i.id, name: i.name, display_name: i.display_name, endpoint: i.endpoint, used, limit: i.device_limit });
    }
    Ok(out)
}

#[server]
pub async fn my_devices() -> Result<Vec<PeerView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;
    let peers = crate::store::list_peers_for_user(&pool, uid).await.map_err(err)?;
    let mut out = Vec::with_capacity(peers.len());
    for p in peers {
        out.push(peer_view(&pool, p, Some(user.email.clone())).await?);
    }
    Ok(out)
}

#[server]
pub async fn create_my_device(interface_id: Uuid, name: String) -> Result<NewDeviceView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;

    let iface = crate::store::get_interface(&pool, interface_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("interface not found"))?;
    // Access is pattern-derived.
    if !crate::store::email_matches(&iface.access_patterns, &user.email) {
        return Err(ServerFnError::new("you do not have access to this interface"));
    }
    if let Some(limit) = iface.device_limit {
        let used = crate::store::count_user_devices_on_interface(&pool, interface_id, uid).await.map_err(err)?;
        if used >= limit as i64 {
            return Err(ServerFnError::new(format!("device limit reached ({limit})")));
        }
    }
    let (peer, private_key) =
        crate::store::create_peer(&pool, interface_id, Some(uid), &name).await.map_err(err)?;
    sync(&pool, interface_id).await?;
    let config = crate::store::render_peer_config(&iface, &peer, &private_key);
    let qr_svg = crate::store::config_qr_svg(&config);
    Ok(NewDeviceView { peer: peer_view(&pool, peer, Some(user.email)).await?, config, qr_svg })
}

#[server]
pub async fn regenerate_peer(id: Uuid) -> Result<NewDeviceView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    authorize_peer(&pool, id).await?;
    let (peer, private_key) = crate::store::regenerate_peer(&pool, id).await.map_err(err)?;
    sync(&pool, peer.interface_id).await?;
    let iface = crate::store::get_interface(&pool, peer.interface_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("interface not found"))?;
    let config = crate::store::render_peer_config(&iface, &peer, &private_key);
    let qr_svg = crate::store::config_qr_svg(&config);
    Ok(NewDeviceView { peer: peer_view(&pool, peer, None).await?, config, qr_svg })
}

#[server]
pub async fn delete_peer(id: Uuid) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    let peer = authorize_peer(&pool, id).await?;
    crate::store::delete_peer(&pool, id).await.map_err(err)?;
    sync(&pool, peer.interface_id).await
}

/// Re-render an existing device's config. The private key is NOT stored, so it
/// appears as a placeholder — regenerate to get a usable config.
#[server]
pub async fn peer_config_text(id: Uuid) -> Result<String, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let peer = authorize_peer(&pool, id).await?;
    let iface = crate::store::get_interface(&pool, peer.interface_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("interface not found"))?;
    Ok(crate::store::render_peer_config(&iface, &peer, crate::store::PRIVATE_KEY_PLACEHOLDER))
}

// ── Admin: interfaces ─────────────────────────────────────────────────

#[server]
pub async fn admin_list_interfaces() -> Result<Vec<InterfaceAdminView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let ifaces = crate::store::list_interfaces(&pool).await.map_err(err)?;
    Ok(ifaces.iter().map(iface_admin_view).collect())
}

/// Create an interface. Backend/patterns take effect immediately; a bring-up
/// failure is returned (and also persisted on the interface's `last_error`).
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn admin_create_interface(
    name: String,
    display_name: String,
    listen_port: i32,
    address: String,
    endpoint: String,
    dns: String,
    allowed_ips: String,
    keepalive: i32,
    device_limit: Option<i32>,
    access_patterns: Vec<String>,
    backend_kind: String,
    mikrotik_url: String,
    mikrotik_username: String,
    mikrotik_password: String,
    mikrotik_insecure: bool,
) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let backend = build_backend_config(&backend_kind, &mikrotik_url, &mikrotik_username, &mikrotik_password, mikrotik_insecure)?;
    let patterns = clean_patterns(access_patterns);
    let iface = crate::store::create_interface(
        &pool,
        &name,
        &display_name,
        listen_port,
        &address,
        &endpoint,
        opt(&dns),
        &allowed_ips,
        keepalive,
        device_limit,
        &patterns,
        backend,
    )
    .await
    .map_err(err)?;
    // Bring it up now; surface (and persist) any backend failure.
    sync(&pool, iface.id).await
}

#[server]
#[allow(clippy::too_many_arguments)]
pub async fn admin_update_interface(
    id: Uuid,
    display_name: String,
    endpoint: String,
    dns: String,
    allowed_ips: String,
    keepalive: i32,
    device_limit: Option<i32>,
    access_patterns: Vec<String>,
    backend_kind: String,
    mikrotik_url: String,
    mikrotik_username: String,
    mikrotik_password: String,
    mikrotik_insecure: bool,
) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    // The backend *type* is immutable (enforced in the store); credentials may be
    // updated. A blank MikroTik password keeps the stored one (store handles it).
    let backend = Some(build_backend_config(&backend_kind, &mikrotik_url, &mikrotik_username, &mikrotik_password, mikrotik_insecure)?);
    let patterns = clean_patterns(access_patterns);
    crate::store::update_interface(
        &pool,
        id,
        Some(&display_name),
        Some(&endpoint),
        Some(opt(&dns)),
        Some(&allowed_ips),
        Some(keepalive),
        Some(device_limit),
        Some(&patterns),
        backend,
    )
    .await
    .map_err(err)?;
    sync(&pool, id).await
}

#[server]
pub async fn admin_delete_interface(id: Uuid) -> Result<(), ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    crate::store::delete_interface(&pool, id).await.map_err(err)
}

// ── Admin: users + devices ────────────────────────────────────────────

#[server]
pub async fn admin_list_users() -> Result<Vec<UserAdminView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let rows = crate::store::list_users_with_counts(&pool).await.map_err(err)?;
    Ok(rows
        .into_iter()
        .map(|r| UserAdminView {
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
    let mut out = Vec::with_capacity(peers.len());
    for p in peers {
        out.push(peer_view(&pool, p, email.clone()).await?);
    }
    Ok(out)
}

/// Admin creates a device for a user on an interface (bypasses limit + ACL; the
/// device just stays inactive until the user matches the interface patterns).
/// Optional `address` assigns an explicit CIDR/subnet.
/// Admin creates a device for a user. When `generate_key` is false the device
/// is left *unconfigured* (no key) so the user generates it themselves — this
/// returns `None`. When true it generates the key and returns the config once.
#[server]
pub async fn admin_create_device(
    interface_id: Uuid,
    user_id: Uuid,
    name: String,
    address: String,
    generate_key: bool,
) -> Result<Option<NewDeviceView>, ServerFnError> {
    let pool = crate::server_state::pool()?;
    require_admin(&pool).await?;
    let addr = address.trim();
    let addr_opt = if addr.is_empty() { None } else { Some(addr) };

    if !generate_key {
        crate::store::create_peer_unconfigured(&pool, interface_id, Some(user_id), &name, addr_opt)
            .await
            .map_err(err)?;
        return Ok(None);
    }

    let (peer, private_key) = match addr_opt {
        Some(a) => crate::store::create_peer_with_address(&pool, interface_id, Some(user_id), &name, a).await,
        None => crate::store::create_peer(&pool, interface_id, Some(user_id), &name).await,
    }
    .map_err(err)?;
    sync(&pool, interface_id).await?;
    let iface = crate::store::get_interface(&pool, interface_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ServerFnError::new("interface not found"))?;
    let config = crate::store::render_peer_config(&iface, &peer, &private_key);
    let qr_svg = crate::store::config_qr_svg(&config);
    Ok(Some(NewDeviceView { peer: peer_view(&pool, peer, None).await?, config, qr_svg }))
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

// ── server-only helpers ───────────────────────────────────────────────

#[cfg(feature = "server")]
fn err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[cfg(feature = "server")]
fn opt(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
}

#[cfg(feature = "server")]
/// Trim each pattern row from the UI and drop blanks (empty rows the admin
/// added but didn't fill in).
fn clean_patterns(rows: Vec<String>) -> Vec<String> {
    rows.into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[cfg(feature = "server")]
fn build_backend_config(
    kind: &str,
    url: &str,
    username: &str,
    password: &str,
    insecure: bool,
) -> Result<crate::backend::BackendConfig, ServerFnError> {
    if kind == "mikrotik" && !crate::crypto::has_key() {
        return Err(ServerFnError::new(
            "storing MikroTik credentials requires [secrets] encryption_key in config",
        ));
    }
    crate::backend::BackendConfig::from_parts(
        kind,
        Some(url.to_string()),
        Some(username.to_string()),
        Some(password.to_string()),
        insecure,
    )
    .map_err(ServerFnError::new)
}

#[cfg(feature = "server")]
fn iface_admin_view(i: &crate::store::Interface) -> InterfaceAdminView {
    let (url, user, insecure) = match i.backend.mikrotik_display() {
        Some((u, n, s)) => (Some(u), Some(n), s),
        None => (None, None, false),
    };
    InterfaceAdminView {
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
        access_patterns: i.access_patterns.clone(),
        backend_kind: i.backend.kind().to_string(),
        mikrotik_url: url,
        mikrotik_username: user,
        mikrotik_insecure: insecure,
        backend_error: i.last_error.clone(),
    }
}

#[cfg(feature = "server")]
async fn peer_view(
    pool: &sqlx::PgPool,
    p: crate::store::Peer,
    owner_email: Option<String>,
) -> Result<PeerView, ServerFnError> {
    let interface_name = sqlx::query_scalar::<_, String>("SELECT name FROM wg_interfaces WHERE id = $1")
        .bind(p.interface_id)
        .fetch_optional(pool)
        .await
        .map_err(err)?
        .unwrap_or_default();
    let configured = !p.public_key.is_empty();
    Ok(PeerView {
        id: p.id,
        interface_id: p.interface_id,
        interface_name,
        name: p.name,
        address: p.address,
        public_key: p.public_key,
        owner_email,
        configured,
    })
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
        Err(ServerFnError::new("not your device"))
    }
}

#[cfg(feature = "server")]
async fn sync(pool: &sqlx::PgPool, interface_id: Uuid) -> Result<(), ServerFnError> {
    crate::store::sync_interface(pool, interface_id).await.map_err(err)
}

#[cfg(feature = "server")]
async fn sync_all(pool: &sqlx::PgPool) -> Result<(), ServerFnError> {
    crate::store::sync_all(pool).await.map_err(err)
}

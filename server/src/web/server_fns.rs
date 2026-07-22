//! Dioxus server functions the UI calls. Signatures compile on both targets;
//! bodies run server-side only and return the plain DTOs from `super::dto`.

use dioxus::prelude::*;

use super::dto::{CurrentUser, InterfaceView, PeerView};
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

#[server]
pub async fn create_my_peer(name: String) -> Result<PeerView, ServerFnError> {
    let pool = crate::server_state::pool()?;
    let user = crate::web::user::current_user().await?;
    let uid = crate::web::user::current_user_id(&pool, &user).await?;
    let iface = default_interface_id(&pool).await?;
    let peer = crate::store::create_peer(&pool, iface, Some(uid), &name).await.map_err(err)?;
    sync(&pool, iface).await?;
    Ok(peer_view(peer, Some(user.email)))
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

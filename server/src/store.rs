//! Database layer for interfaces and peers, plus per-interface backend
//! reconciliation and client-config rendering. Shared by the web UI and the API.
//!
//! Interfaces are fully admin-managed (no config seeding). Each interface
//! carries its own backend (self-managed / NetworkManager / MikroTik, secrets
//! encrypted) and a pattern-based ACL: a user may use an interface iff their
//! email matches one of the interface's `access_patterns` (globs; `*` = all;
//! literal emails allowed). Access is enforced live at sync time, so editing
//! patterns and re-syncing grants/revokes devices in real time.

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::backend::{BackendConfig, InterfaceSpec, PeerSpec, PeerStatus};
use crate::wg;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema)]
pub struct Interface {
    pub id: Uuid,
    /// WireGuard interface id (e.g. wg0) — unique, immutable, the system name.
    pub name: String,
    /// Human-friendly label; empty falls back to `name` in the UI.
    pub display_name: String,
    pub listen_port: i32,
    pub address: String,
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub private_key: String,
    pub public_key: String,
    pub endpoint: String,
    pub dns: Option<String>,
    pub allowed_ips: String,
    pub keepalive: i32,
    /// Max devices per user on this interface; None = unlimited.
    pub device_limit: Option<i32>,
    /// Access patterns (globs / literal emails; `*` = everyone). Stored as jsonb.
    #[sqlx(json)]
    pub access_patterns: Vec<String>,
    /// Per-interface backend (secrets encrypted). Not exposed via serialize.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    #[sqlx(json)]
    pub backend: BackendConfig,
    /// Last backend reconcile error (None = last sync ok).
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema)]
pub struct Peer {
    pub id: Uuid,
    pub interface_id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub public_key: String,
    /// Server-side secret used to configure the peer (kept). The client's
    /// PRIVATE key is never stored — it is generated + returned once.
    pub preshared_key: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub banned: bool,
    pub access_revoked: bool,
}

const IFACE_COLS: &str = "id, name, display_name, listen_port, address, private_key, public_key, endpoint, \
    dns, allowed_ips, keepalive, device_limit, access_patterns, backend, last_error";
const PEER_COLS: &str = "id, interface_id, user_id, name, public_key, preshared_key, address";
const USER_COLS: &str = "id, email, name, is_admin, banned, access_revoked";

/// Default IPv6 pools appended when an interface lacks IPv6 (IPv6 is
/// non-optional).
const DEFAULT_V6_ADDR: &str = "fd00:8::1/64";
const DEFAULT_V6_NET: &str = "fd00:8::/64";

// ── ACL matching ──────────────────────────────────────────────────────

/// Whether `email` matches any access pattern (case-insensitive globs with `*`;
/// a literal email matches only itself; `*` matches everyone). Empty patterns
/// match nobody (grant-required).
pub fn email_matches(patterns: &[String], email: &str) -> bool {
    let e = email.to_ascii_lowercase();
    patterns.iter().any(|p| glob_match(&p.trim().to_ascii_lowercase(), &e))
}

fn glob_match(pat: &str, s: &str) -> bool {
    let (pat, s) = (pat.as_bytes(), s.as_bytes());
    let (mut pi, mut si) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while si < s.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

// ── Interfaces ────────────────────────────────────────────────────────

/// Create an interface (admin). Generates a keypair, enforces IPv6, and
/// encrypts backend secrets before storing.
#[allow(clippy::too_many_arguments)]
pub async fn create_interface(
    pool: &PgPool,
    name: &str,
    display_name: &str,
    listen_port: i32,
    address: &str,
    endpoint: &str,
    dns: Option<&str>,
    allowed_ips: &str,
    keepalive: i32,
    device_limit: Option<i32>,
    access_patterns: &[String],
    mut backend: BackendConfig,
) -> Result<Interface> {
    backend.encrypt_secrets();
    let address = wg::ensure_ipv6(address, DEFAULT_V6_ADDR);
    let allowed_ips = wg::ensure_ipv6(allowed_ips, DEFAULT_V6_NET);
    let kp = wg::generate_keypair();
    let iface = sqlx::query_as::<_, Interface>(&format!(
        "INSERT INTO wg_interfaces \
         (name, display_name, listen_port, address, private_key, public_key, endpoint, dns, allowed_ips, \
          keepalive, device_limit, access_patterns, backend) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING {IFACE_COLS}"
    ))
    .bind(name)
    .bind(display_name)
    .bind(listen_port)
    .bind(&address)
    .bind(&kp.private_key)
    .bind(&kp.public_key)
    .bind(endpoint)
    .bind(dns)
    .bind(&allowed_ips)
    .bind(keepalive)
    .bind(device_limit)
    .bind(sqlx::types::Json(access_patterns))
    .bind(sqlx::types::Json(backend))
    .fetch_one(pool)
    .await
    .context("insert interface")?;
    Ok(iface)
}

pub async fn list_interfaces(pool: &PgPool) -> Result<Vec<Interface>> {
    Ok(sqlx::query_as::<_, Interface>(&format!(
        "SELECT {IFACE_COLS} FROM wg_interfaces ORDER BY created_at"
    ))
    .fetch_all(pool)
    .await?)
}

pub async fn get_interface(pool: &PgPool, id: Uuid) -> Result<Option<Interface>> {
    Ok(sqlx::query_as::<_, Interface>(&format!(
        "SELECT {IFACE_COLS} FROM wg_interfaces WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// Interfaces a user (by email) may use, per the pattern ACL.
pub async fn interfaces_for_user(pool: &PgPool, email: &str) -> Result<Vec<Interface>> {
    Ok(list_interfaces(pool)
        .await?
        .into_iter()
        .filter(|i| email_matches(&i.access_patterns, email))
        .collect())
}

/// Update an interface's policy + presentation (not its name/keys/address).
#[allow(clippy::too_many_arguments)]
pub async fn update_interface(
    pool: &PgPool,
    id: Uuid,
    display_name: Option<&str>,
    endpoint: Option<&str>,
    dns: Option<Option<&str>>,
    allowed_ips: Option<&str>,
    keepalive: Option<i32>,
    device_limit: Option<Option<i32>>,
    access_patterns: Option<&[String]>,
    backend: Option<BackendConfig>,
) -> Result<Interface> {
    // Server addresses are immutable after creation (changing them strands
    // existing peers). The backend *kind* is also immutable, but its credentials
    // (e.g. MikroTik URL/username/password) may be updated in place.
    let cur = get_interface(pool, id).await?.context("interface not found")?;
    let new_display = display_name.map(str::to_string).unwrap_or(cur.display_name);
    let new_endpoint = endpoint.map(str::to_string).unwrap_or(cur.endpoint);
    let new_dns = match dns {
        Some(d) => d.map(str::to_string),
        None => cur.dns,
    };
    let new_allowed = allowed_ips
        .map(|a| wg::ensure_ipv6(a, DEFAULT_V6_NET))
        .unwrap_or(cur.allowed_ips);
    let new_keepalive = keepalive.unwrap_or(cur.keepalive);
    let new_limit = match device_limit {
        Some(v) => v,
        None => cur.device_limit,
    };
    let new_patterns = access_patterns.map(<[String]>::to_vec).unwrap_or(cur.access_patterns);
    let new_backend = match backend {
        Some(mut b) => {
            if b.kind() != cur.backend.kind() {
                anyhow::bail!("backend type cannot be changed after creation");
            }
            b.encrypt_secrets();
            // A blank secret means "keep the stored one", so admins can edit the
            // URL/username/etc. without re-typing the MikroTik password / node key.
            match (&mut b, &cur.backend) {
                (
                    BackendConfig::Mikrotik { password: newp, .. },
                    BackendConfig::Mikrotik { password: oldp, .. },
                ) if newp.is_empty() => *newp = oldp.clone(),
                (
                    BackendConfig::Node { key: newk, .. },
                    BackendConfig::Node { key: oldk, .. },
                ) if newk.is_empty() => *newk = oldk.clone(),
                _ => {}
            }
            b
        }
        None => cur.backend,
    };

    let iface = sqlx::query_as::<_, Interface>(&format!(
        "UPDATE wg_interfaces SET display_name=$2, endpoint=$3, dns=$4, allowed_ips=$5, keepalive=$6, \
         device_limit=$7, access_patterns=$8, backend=$9 WHERE id=$1 RETURNING {IFACE_COLS}"
    ))
    .bind(id)
    .bind(&new_display)
    .bind(&new_endpoint)
    .bind(&new_dns)
    .bind(&new_allowed)
    .bind(new_keepalive)
    .bind(new_limit)
    .bind(sqlx::types::Json(&new_patterns))
    .bind(sqlx::types::Json(new_backend))
    .fetch_one(pool)
    .await
    .context("update interface")?;
    Ok(iface)
}

/// Delete an interface: tear it down on its backend (best-effort), then drop
/// the row (peers cascade).
pub async fn delete_interface(pool: &PgPool, id: Uuid) -> Result<()> {
    if let Some(iface) = get_interface(pool, id).await? {
        if let Ok(backend) = build_backend(&iface) {
            let _ = backend.remove(&iface.name).await;
        }
    }
    sqlx::query("DELETE FROM wg_interfaces WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Build (and decrypt) the backend for an interface.
fn build_backend(iface: &Interface) -> Result<Box<dyn crate::backend::WireguardBackend>> {
    let mut b = iface.backend.clone();
    b.decrypt_secrets().context("decrypt backend secrets")?;
    b.build().map_err(|e| anyhow::anyhow!("build backend: {e}"))
}

/// Live peer status for an interface, via its backend.
pub async fn interface_status(pool: &PgPool, id: Uuid) -> Result<Vec<PeerStatus>> {
    let iface = get_interface(pool, id).await?.context("interface not found")?;
    let backend = build_backend(&iface)?;
    backend.status(&iface.name).await.map_err(|e| anyhow::anyhow!("backend status: {e}"))
}

// ── Peers ─────────────────────────────────────────────────────────────

pub async fn list_peers(pool: &PgPool, interface_id: Uuid) -> Result<Vec<Peer>> {
    Ok(sqlx::query_as::<_, Peer>(&format!(
        "SELECT {PEER_COLS} FROM wg_peers WHERE interface_id = $1 ORDER BY created_at"
    ))
    .bind(interface_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_peers_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Peer>> {
    Ok(sqlx::query_as::<_, Peer>(&format!(
        "SELECT {PEER_COLS} FROM wg_peers WHERE user_id = $1 ORDER BY created_at"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_peer(pool: &PgPool, id: Uuid) -> Result<Option<Peer>> {
    Ok(sqlx::query_as::<_, Peer>(&format!(
        "SELECT {PEER_COLS} FROM wg_peers WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// Count a user's devices on a specific interface (for per-interface limits).
pub async fn count_user_devices_on_interface(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Uuid,
) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM wg_peers WHERE interface_id = $1 AND user_id = $2")
            .bind(interface_id)
            .bind(user_id)
            .fetch_one(pool)
            .await?,
    )
}

/// Create a peer on `interface_id`: generate a keypair + PSK and allocate the
/// next free tunnel address in each of the interface's subnets (dual-stack).
/// Returns `(peer, private_key)` — the private key is NOT stored; the caller
/// shows it to the user once.
pub async fn create_peer(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
) -> Result<(Peer, String)> {
    let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
    let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
    let taken: Vec<String> =
        list_peers(pool, interface_id).await?.into_iter().map(|p| p.address).collect();
    let address = wg::allocate_addresses(&subnets, &taken).map_err(anyhow::Error::msg)?;
    insert_peer(pool, interface_id, user_id, name, &address).await
}

/// Create a peer with an explicit address/subnet spec (admin only). Returns
/// `(peer, private_key)`.
pub async fn create_peer_with_address(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
    address_spec: &str,
) -> Result<(Peer, String)> {
    let address = wg::validate_address_spec(address_spec).map_err(anyhow::Error::msg)?;
    insert_peer(pool, interface_id, user_id, name, &address).await
}

/// Create an *unconfigured* device (admin): no key is generated, so the user
/// generates it themselves later (via regenerate). `public_key` stays empty
/// until then. Allocates an address (or uses `address_spec`).
pub async fn create_peer_unconfigured(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
    address_spec: Option<&str>,
) -> Result<Peer> {
    let address = match address_spec {
        Some(s) => wg::validate_address_spec(s).map_err(anyhow::Error::msg)?,
        None => {
            let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
            let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
            let taken: Vec<String> =
                list_peers(pool, interface_id).await?.into_iter().map(|p| p.address).collect();
            wg::allocate_addresses(&subnets, &taken).map_err(anyhow::Error::msg)?
        }
    };
    sqlx::query_as::<_, Peer>(&format!(
        "INSERT INTO wg_peers (interface_id, user_id, name, public_key, preshared_key, address) \
         VALUES ($1,$2,$3,'',NULL,$4) RETURNING {PEER_COLS}"
    ))
    .bind(interface_id)
    .bind(user_id)
    .bind(name)
    .bind(&address)
    .fetch_one(pool)
    .await
    .context("insert unconfigured peer")
}

/// Insert a peer, storing only its PUBLIC key + PSK. Returns `(peer,
/// private_key)`; the private key is generated here and never persisted.
async fn insert_peer(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
    address: &str,
) -> Result<(Peer, String)> {
    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();
    let peer = sqlx::query_as::<_, Peer>(&format!(
        "INSERT INTO wg_peers \
         (interface_id, user_id, name, public_key, preshared_key, address) \
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING {PEER_COLS}"
    ))
    .bind(interface_id)
    .bind(user_id)
    .bind(name)
    .bind(&kp.public_key)
    .bind(&psk)
    .bind(address)
    .fetch_one(pool)
    .await
    .context("insert peer")?;
    Ok((peer, kp.private_key))
}

/// Replace a peer's keypair (and preshared key), keeping its address. Returns
/// `(peer, new_private_key)` — the new private key is not stored.
pub async fn regenerate_peer(pool: &PgPool, peer_id: Uuid) -> Result<(Peer, String)> {
    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();
    let peer = sqlx::query_as::<_, Peer>(&format!(
        "UPDATE wg_peers SET public_key=$2, preshared_key=$3 \
         WHERE id=$1 RETURNING {PEER_COLS}"
    ))
    .bind(peer_id)
    .bind(&kp.public_key)
    .bind(&psk)
    .fetch_one(pool)
    .await
    .context("regenerate peer")?;
    Ok((peer, kp.private_key))
}

/// Rename a peer (device). Cosmetic — no backend change needed.
pub async fn rename_peer(pool: &PgPool, peer_id: Uuid, name: &str) -> Result<Peer> {
    sqlx::query_as::<_, Peer>(&format!(
        "UPDATE wg_peers SET name=$2 WHERE id=$1 RETURNING {PEER_COLS}"
    ))
    .bind(peer_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .context("rename peer")
}

pub async fn delete_peer(pool: &PgPool, peer_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM wg_peers WHERE id = $1")
        .bind(peer_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Upsert a user by email, returning its id.
pub async fn upsert_user_by_email(pool: &PgPool, email: &str) -> Result<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email) VALUES ($1) \
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .context("upsert user")
}

// ── Users (admin) ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UserWithCount {
    pub user: User,
    pub device_count: i64,
}

pub async fn get_user(pool: &PgPool, id: Uuid) -> Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn list_users_with_counts(pool: &PgPool) -> Result<Vec<UserWithCount>> {
    let users =
        sqlx::query_as::<_, User>(&format!("SELECT {USER_COLS} FROM users ORDER BY email"))
            .fetch_all(pool)
            .await?;
    let mut out = Vec::with_capacity(users.len());
    for user in users {
        let device_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wg_peers WHERE user_id = $1")
                .bind(user.id)
                .fetch_one(pool)
                .await?;
        out.push(UserWithCount { user, device_count });
    }
    Ok(out)
}

pub async fn set_banned(pool: &PgPool, user_id: Uuid, banned: bool) -> Result<()> {
    sqlx::query("UPDATE users SET banned = $2 WHERE id = $1")
        .bind(user_id)
        .bind(banned)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_access_revoked(pool: &PgPool, user_id: Uuid, revoked: bool) -> Result<()> {
    sqlx::query("UPDATE users SET access_revoked = $2 WHERE id = $1")
        .bind(user_id)
        .bind(revoked)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_user(pool: &PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Active peers + reconcile ──────────────────────────────────────────

/// Peers currently permitted on `interface_id`: unowned, or owned by a user who
/// is not banned/revoked AND whose email matches the interface's access
/// patterns. This is what gets pushed to the backend, so pattern edits + a sync
/// grant/revoke devices in real time.
pub async fn list_active_peers(pool: &PgPool, interface_id: Uuid) -> Result<Vec<Peer>> {
    let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
    let peers = list_peers(pool, interface_id).await?;
    let mut out = Vec::with_capacity(peers.len());
    for p in peers {
        // Unconfigured devices (no key yet) can't be applied.
        if p.public_key.is_empty() {
            continue;
        }
        match p.user_id {
            None => out.push(p),
            Some(uid) => {
                if let Some((email, banned, revoked)) =
                    sqlx::query_as::<_, (String, bool, bool)>(
                        "SELECT email, banned, access_revoked FROM users WHERE id = $1",
                    )
                    .bind(uid)
                    .fetch_optional(pool)
                    .await?
                {
                    if !banned && !revoked && email_matches(&iface.access_patterns, &email) {
                        out.push(p);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Reconcile one interface to its backend (active peers only).
pub async fn sync_interface(pool: &PgPool, interface_id: Uuid) -> Result<()> {
    let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
    let backend = build_backend(&iface)?;
    let peers = list_active_peers(pool, interface_id).await?;

    let ispec = InterfaceSpec {
        name: iface.name.clone(),
        listen_port: iface.listen_port as u16,
        address: iface.address.clone(),
        private_key: iface.private_key.clone(),
    };
    let pspecs: Vec<PeerSpec> = peers
        .iter()
        .map(|p| PeerSpec {
            public_key: p.public_key.clone(),
            address: p.address.clone(),
            preshared_key: p.preshared_key.clone(),
        })
        .collect();

    // Record the outcome on the interface so bring-up failures surface in the UI.
    let result = backend.apply(&ispec, &pspecs).await;
    let err_text = result.as_ref().err().map(|e| e.to_string());
    let _ = sqlx::query("UPDATE wg_interfaces SET last_error = $2 WHERE id = $1")
        .bind(interface_id)
        .bind(&err_text)
        .execute(pool)
        .await;
    result.map_err(|e| anyhow::anyhow!("backend apply: {e}"))
}

/// Reconcile every interface (used on boot and after user-level changes).
pub async fn sync_all(pool: &PgPool) -> Result<()> {
    for iface in list_interfaces(pool).await? {
        sync_interface(pool, iface.id).await?;
    }
    Ok(())
}

/// Best-effort sync of every interface — logs failures, never errors. For boot
/// and background reconciles where one bad backend shouldn't block the rest.
pub async fn sync_all_best_effort(pool: &PgPool) {
    let ifaces = match list_interfaces(pool).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("sync_all: listing interfaces failed: {e:#}");
            return;
        }
    };
    for iface in ifaces {
        if let Err(e) = sync_interface(pool, iface.id).await {
            tracing::warn!("sync of interface {} failed: {e:#}", iface.name);
        }
    }
}

/// Placeholder for the private key when re-rendering an existing device's
/// config (the real private key was shown once at creation and isn't stored).
pub const PRIVATE_KEY_PLACEHOLDER: &str =
    "<not stored — regenerate this device to get a new key>";

/// Render a scannable QR code (SVG) of a WireGuard client config. WireGuard's
/// mobile apps import the whole `.conf` text from the QR, so we encode the
/// config verbatim.
pub fn config_qr_svg(config: &str) -> String {
    match qrcode::QrCode::new(config.as_bytes()) {
        Ok(code) => code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .dark_color(qrcode::render::svg::Color("#111827"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
        // Config too large for a QR (very unusual) — return nothing; UI hides it.
        Err(_) => String::new(),
    }
}

/// Render the client `.conf` for a peer. `private_key` is supplied by the
/// caller — the freshly generated key at create/regenerate time, or
/// [`PRIVATE_KEY_PLACEHOLDER`] when re-showing an existing device.
pub fn render_peer_config(iface: &Interface, peer: &Peer, private_key: &str) -> String {
    wg::render_client_config(&wg::ClientConfig {
        private_key,
        address: &peer.address,
        dns: iface.dns.as_deref(),
        server_public_key: &iface.public_key,
        preshared_key: peer.preshared_key.as_deref(),
        endpoint: &iface.endpoint,
        allowed_ips: &iface.allowed_ips,
        persistent_keepalive: iface.keepalive as u16,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{PeerStatus, WireguardBackend};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[test]
    fn glob_and_email_matching() {
        assert!(email_matches(&["*".into()], "anyone@x.com"));
        assert!(email_matches(&["*@corp.com".into()], "alice@corp.com"));
        assert!(!email_matches(&["*@corp.com".into()], "alice@other.com"));
        assert!(email_matches(&["cto@x.com".into()], "CTO@X.com")); // case-insensitive
        assert!(!email_matches(&[], "nobody@x.com")); // empty = grant-required
        assert!(email_matches(&["a@x.com".into(), "*@y.com".into()], "bob@y.com"));
    }

    #[derive(Default)]
    struct MockBackend {
        applied: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WireguardBackend for MockBackend {
        async fn apply(&self, _i: &InterfaceSpec, peers: &[PeerSpec]) -> crate::backend::Result<()> {
            *self.applied.lock().unwrap() = peers.iter().map(|p| p.public_key.clone()).collect();
            Ok(())
        }
        async fn status(&self, _n: &str) -> crate::backend::Result<Vec<PeerStatus>> {
            Ok(vec![])
        }
        async fn remove(&self, _n: &str) -> crate::backend::Result<()> {
            Ok(())
        }
    }

    async fn mk_iface(pool: &PgPool, patterns: &[String], addr: &str) -> Interface {
        create_interface(
            pool,
            "wg0",
            "",
            51820,
            addr,
            "vpn.example.com:51820",
            Some("10.8.0.1"),
            "10.8.0.0/24",
            25,
            Some(5),
            patterns,
            BackendConfig::SelfManaged,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn dual_stack_alloc_and_subnet_device() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = mk_iface(&pool, &["*".into()], "10.8.0.1/24, fd00:8::1/64").await;
        let (p, privkey) = create_peer(&pool, iface.id, None, "dual").await.unwrap();
        assert_eq!(p.address, "10.8.0.2/32, fd00:8::2/128");
        assert_eq!(privkey.len(), 44); // returned once, not stored

        let (sub, _) = create_peer_with_address(&pool, iface.id, None, "site", "fd00:beef::/64")
            .await
            .unwrap();
        assert_eq!(sub.address, "fd00:beef::/64");

        let cfg = render_peer_config(&iface, &p, &privkey);
        assert!(cfg.contains(&format!("PrivateKey = {privkey}")));
        assert!(cfg.contains("Address = 10.8.0.2/32, fd00:8::2/128"));
    }

    #[tokio::test]
    async fn pattern_acl_gates_active_peers() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        // Interface only grants *@corp.com.
        let iface = mk_iface(&pool, &["*@corp.com".into()], "10.8.0.1/24").await;

        let alice: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('alice@corp.com') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let mallory: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('mallory@evil.com') RETURNING id")
            .fetch_one(&pool).await.unwrap();

        create_peer(&pool, iface.id, Some(alice), "a").await.unwrap();
        create_peer(&pool, iface.id, Some(mallory), "m").await.unwrap();

        // Only alice's device is active (mallory doesn't match the pattern).
        let active = list_active_peers(&pool, iface.id).await.unwrap();
        assert_eq!(active.len(), 1);

        let backend = MockBackend::default();
        // sync_interface builds a self-managed backend which would shell out;
        // instead assert active-set directly against the mock.
        let peers = list_active_peers(&pool, iface.id).await.unwrap();
        backend
            .apply(
                &InterfaceSpec { name: iface.name.clone(), listen_port: 51820, address: iface.address.clone(), private_key: iface.private_key.clone() },
                &peers.iter().map(|p| PeerSpec { public_key: p.public_key.clone(), address: p.address.clone(), preshared_key: p.preshared_key.clone() }).collect::<Vec<_>>(),
            )
            .await
            .unwrap();
        assert_eq!(backend.applied.lock().unwrap().len(), 1);

        // Widen the pattern -> mallory now active.
        update_interface(&pool, iface.id, None, None, None, None, None, None, Some(&["*".into()]), None)
            .await
            .unwrap();
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 2);

        // Ban alice -> back to 1.
        set_banned(&pool, alice, true).await.unwrap();
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn per_interface_device_count() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let a = mk_iface(&pool, &["*".into()], "10.8.0.1/24").await;
        // A second interface with a different name/subnet.
        let b = create_interface(&pool, "wg1", "", 51821, "10.9.0.1/24", "vpn:51821", None, "10.9.0.0/24", 25, Some(2), &["*".into()], BackendConfig::SelfManaged).await.unwrap();

        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        create_peer(&pool, a.id, Some(u), "d1").await.unwrap();
        create_peer(&pool, a.id, Some(u), "d2").await.unwrap();
        create_peer(&pool, b.id, Some(u), "d3").await.unwrap();

        assert_eq!(count_user_devices_on_interface(&pool, a.id, u).await.unwrap(), 2);
        assert_eq!(count_user_devices_on_interface(&pool, b.id, u).await.unwrap(), 1);

        // interfaces_for_user sees both (both are `*`).
        assert_eq!(interfaces_for_user(&pool, "u@x").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn unconfigured_device_activates_on_regenerate() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = mk_iface(&pool, &["*".into()], "10.8.0.1/24").await;
        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();

        // Two unconfigured devices coexist (empty public keys don't collide).
        let d1 = create_peer_unconfigured(&pool, iface.id, Some(u), "a", None).await.unwrap();
        let _d2 = create_peer_unconfigured(&pool, iface.id, Some(u), "b", None).await.unwrap();
        assert!(d1.public_key.is_empty());
        assert_eq!(count_user_devices_on_interface(&pool, iface.id, u).await.unwrap(), 2);
        // Neither is active yet (no key).
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());

        // The user generates a key -> device becomes configured + active.
        let (d1b, privkey) = regenerate_peer(&pool, d1.id).await.unwrap();
        assert!(!d1b.public_key.is_empty());
        assert_eq!(d1b.address, d1.address);
        assert_eq!(privkey.len(), 44);
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 1);
    }
}

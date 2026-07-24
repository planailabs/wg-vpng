//! Database layer for interfaces and peers, plus per-interface backend
//! reconciliation and client-config rendering. Shared by the web UI and the API.
//!
//! Interfaces are fully admin-managed (no config seeding). Each interface
//! carries its own backend (secrets encrypted) and references groups
//! (`wg_groups`). A user may use an interface iff they belong to one of its
//! groups — email matches a group pattern (globs; `*` = all; empty = nobody) or
//! one of their captured OIDC claim groups is in the group's `claim_values` —
//! and is currently *live* (logged in within the provider's TTL). Access is
//! enforced live at sync time, so group/interface edits + a re-sync grant/revoke
//! devices in real time.

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
    /// Base name for downloaded device configs; empty falls back to `name`.
    /// The downloaded file is `<download_filename>-<device name>.conf`.
    pub download_filename: Option<String>,
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
    /// Groups whose members may use this interface (union). Stored as jsonb.
    #[sqlx(json)]
    pub group_ids: Vec<Uuid>,
    /// Per-interface backend (secrets encrypted). Not exposed via serialize.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    #[sqlx(json)]
    pub backend: BackendConfig,
    /// Last backend reconcile error (None = last sync ok).
    pub last_error: Option<String>,
}

/// A reusable named set of access rules. A user is a member if their email
/// matches one of `patterns` OR one of their OIDC claim groups is in
/// `claim_values`.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    /// Email globs / literal emails; `*` = everyone. Stored as jsonb.
    #[sqlx(json)]
    pub patterns: Vec<String>,
    /// Values matched against a user's OIDC group claim. Stored as jsonb.
    #[sqlx(json)]
    pub claim_values: Vec<String>,
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
    /// Whether the end user created this device (vs an admin). Users may rename
    /// only their own user-created devices.
    pub user_created: bool,
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

const IFACE_COLS: &str = "id, name, display_name, download_filename, listen_port, address, private_key, public_key, endpoint, \
    dns, allowed_ips, keepalive, device_limit, group_ids, backend, last_error";
const GROUP_COLS: &str = "id, name, patterns, claim_values";
const PEER_COLS: &str = "id, interface_id, user_id, name, public_key, preshared_key, address, user_created";
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

/// Whether a group grants a user: their email matches a pattern, or one of their
/// OIDC claim groups is listed in the group's `claim_values`.
pub fn group_grants(group: &Group, email: &str, claim_groups: &[String]) -> bool {
    email_matches(&group.patterns, email)
        || group.claim_values.iter().any(|cv| claim_groups.iter().any(|cg| cg == cv))
}

/// Whether any of `groups` grants the user.
pub fn access_granted(groups: &[Group], email: &str, claim_groups: &[String]) -> bool {
    groups.iter().any(|g| group_grants(g, email, claim_groups))
}

// ── Groups ────────────────────────────────────────────────────────────

pub async fn list_groups(pool: &PgPool) -> Result<Vec<Group>> {
    Ok(sqlx::query_as::<_, Group>(&format!("SELECT {GROUP_COLS} FROM wg_groups ORDER BY name"))
        .fetch_all(pool)
        .await?)
}

pub async fn get_group(pool: &PgPool, id: Uuid) -> Result<Option<Group>> {
    Ok(sqlx::query_as::<_, Group>(&format!("SELECT {GROUP_COLS} FROM wg_groups WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Load the groups referenced by `ids` (order/absence tolerant).
pub async fn get_groups(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Group>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(sqlx::query_as::<_, Group>(&format!(
        "SELECT {GROUP_COLS} FROM wg_groups WHERE id = ANY($1)"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?)
}

pub async fn create_group(pool: &PgPool, name: &str, patterns: &[String], claim_values: &[String]) -> Result<Group> {
    sqlx::query_as::<_, Group>(&format!(
        "INSERT INTO wg_groups (name, patterns, claim_values) VALUES ($1,$2,$3) RETURNING {GROUP_COLS}"
    ))
    .bind(name)
    .bind(sqlx::types::Json(patterns))
    .bind(sqlx::types::Json(claim_values))
    .fetch_one(pool)
    .await
    .context("create group")
}

pub async fn update_group(pool: &PgPool, id: Uuid, name: &str, patterns: &[String], claim_values: &[String]) -> Result<Group> {
    let g = sqlx::query_as::<_, Group>(&format!(
        "UPDATE wg_groups SET name=$2, patterns=$3, claim_values=$4 WHERE id=$1 RETURNING {GROUP_COLS}"
    ))
    .bind(id)
    .bind(name)
    .bind(sqlx::types::Json(patterns))
    .bind(sqlx::types::Json(claim_values))
    .fetch_one(pool)
    .await
    .context("update group")?;
    // Membership may have changed → re-sync interfaces using this group.
    sync_groups_interfaces(pool, id).await;
    Ok(g)
}

pub async fn delete_group(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM wg_groups WHERE id = $1").bind(id).execute(pool).await?;
    // Interfaces referencing it lose that grant on the next sync; re-sync now.
    sync_groups_interfaces(pool, id).await;
    Ok(())
}

/// Re-sync (best-effort) every interface that references `group_id`.
async fn sync_groups_interfaces(pool: &PgPool, group_id: Uuid) {
    let ifaces = list_interfaces(pool).await.unwrap_or_default();
    for iface in ifaces {
        if iface.group_ids.contains(&group_id) {
            if let Err(e) = sync_interface(pool, iface.id).await {
                tracing::warn!("sync of interface {} after group change failed: {e:#}", iface.name);
            }
        }
    }
}

// ── OIDC claim groups (per user, per provider) ────────────────────────

/// Replace the captured group set for `(user, provider)` (called on login).
pub async fn set_provider_groups(pool: &PgPool, user_id: Uuid, provider: &str, groups: &[String]) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_provider_groups (user_id, provider, groups) VALUES ($1,$2,$3) \
         ON CONFLICT (user_id, provider) DO UPDATE SET groups = EXCLUDED.groups",
    )
    .bind(user_id)
    .bind(provider)
    .bind(sqlx::types::Json(groups))
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a login: push the liveliness deadline out by the provider's configured
/// TTL (default 30d; None = never deactivate). A user whose `live_until` has
/// passed is deactivated — their devices drop until they log in again.
pub async fn record_login(pool: &PgPool, user_id: Uuid, ttl_days: Option<u32>) -> Result<()> {
    let live_sql = match ttl_days {
        Some(d) => format!("now() + interval '{d} days'"),
        None => "'infinity'::timestamptz".to_string(),
    };
    sqlx::query(&format!("UPDATE users SET live_until = {live_sql} WHERE id = $1"))
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A user's effective OIDC claim groups: the union across every provider they've
/// logged in through.
pub async fn user_claim_groups(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, sqlx::types::Json<Vec<String>>>(
        "SELECT groups FROM user_provider_groups WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for r in rows {
        set.extend(r.0);
    }
    Ok(set.into_iter().collect())
}

// ── Interfaces ────────────────────────────────────────────────────────

/// Create an interface (admin). Generates a keypair, enforces IPv6, and
/// encrypts backend secrets before storing.
#[allow(clippy::too_many_arguments)]
pub async fn create_interface(
    pool: &PgPool,
    name: &str,
    display_name: &str,
    download_filename: Option<&str>,
    listen_port: i32,
    address: &str,
    endpoint: &str,
    dns: Option<&str>,
    allowed_ips: &str,
    keepalive: i32,
    device_limit: Option<i32>,
    group_ids: &[Uuid],
    mut backend: BackendConfig,
) -> Result<Interface> {
    backend.encrypt_secrets();
    let address = wg::ensure_ipv6(address, DEFAULT_V6_ADDR);
    let allowed_ips = wg::ensure_ipv6(allowed_ips, DEFAULT_V6_NET);
    let kp = wg::generate_keypair();
    let iface = sqlx::query_as::<_, Interface>(&format!(
        "INSERT INTO wg_interfaces \
         (name, display_name, download_filename, listen_port, address, private_key, public_key, endpoint, dns, allowed_ips, \
          keepalive, device_limit, group_ids, backend) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING {IFACE_COLS}"
    ))
    .bind(name)
    .bind(display_name)
    .bind(download_filename.map(str::trim).filter(|s| !s.is_empty()))
    .bind(listen_port)
    .bind(&address)
    .bind(&kp.private_key)
    .bind(&kp.public_key)
    .bind(endpoint)
    .bind(dns)
    .bind(&allowed_ips)
    .bind(keepalive)
    .bind(device_limit)
    .bind(sqlx::types::Json(group_ids))
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

/// Interfaces a user may use: those assigned a group the user belongs to (by
/// email pattern or OIDC claim group).
pub async fn interfaces_for_user(pool: &PgPool, user_id: Uuid, email: &str) -> Result<Vec<Interface>> {
    if !user_is_live(pool, user_id).await? {
        return Ok(vec![]);
    }
    let claim_groups = user_claim_groups(pool, user_id).await?;
    let mut out = Vec::new();
    for iface in list_interfaces(pool).await? {
        let groups = get_groups(pool, &iface.group_ids).await?;
        if access_granted(&groups, email, &claim_groups) {
            out.push(iface);
        }
    }
    Ok(out)
}

/// Whether a user is currently live (logged in within the provider TTL).
pub async fn user_is_live(pool: &PgPool, user_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>("SELECT live_until > now() FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(false))
}

/// Whether a user may use an interface: live AND a member of one of its groups.
pub async fn can_access_interface(pool: &PgPool, user_id: Uuid, email: &str, interface_id: Uuid) -> Result<bool> {
    if !user_is_live(pool, user_id).await? {
        return Ok(false);
    }
    let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
    let groups = get_groups(pool, &iface.group_ids).await?;
    let claim_groups = user_claim_groups(pool, user_id).await?;
    Ok(access_granted(&groups, email, &claim_groups))
}

/// Update an interface's policy + presentation (not its name/keys/address).
#[allow(clippy::too_many_arguments)]
pub async fn update_interface(
    pool: &PgPool,
    id: Uuid,
    display_name: Option<&str>,
    download_filename: Option<Option<&str>>,
    endpoint: Option<&str>,
    dns: Option<Option<&str>>,
    allowed_ips: Option<&str>,
    keepalive: Option<i32>,
    device_limit: Option<Option<i32>>,
    group_ids: Option<&[Uuid]>,
    backend: Option<BackendConfig>,
) -> Result<Interface> {
    // Server addresses are immutable after creation (changing them strands
    // existing peers). The backend *kind* is also immutable, but its credentials
    // (e.g. MikroTik URL/username/password) may be updated in place.
    let cur = get_interface(pool, id).await?.context("interface not found")?;
    let new_display = display_name.map(str::to_string).unwrap_or(cur.display_name);
    let new_download = match download_filename {
        Some(v) => v.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        None => cur.download_filename,
    };
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
    let new_groups = group_ids.map(<[Uuid]>::to_vec).unwrap_or(cur.group_ids);
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
        "UPDATE wg_interfaces SET display_name=$2, download_filename=$3, endpoint=$4, dns=$5, allowed_ips=$6, keepalive=$7, \
         device_limit=$8, group_ids=$9, backend=$10 WHERE id=$1 RETURNING {IFACE_COLS}"
    ))
    .bind(id)
    .bind(&new_display)
    .bind(&new_download)
    .bind(&new_endpoint)
    .bind(&new_dns)
    .bind(&new_allowed)
    .bind(new_keepalive)
    .bind(new_limit)
    .bind(sqlx::types::Json(&new_groups))
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
    user_created: bool,
) -> Result<(Peer, String)> {
    let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
    let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
    let taken: Vec<String> =
        list_peers(pool, interface_id).await?.into_iter().map(|p| p.address).collect();
    let address = wg::allocate_addresses(&subnets, &taken).map_err(anyhow::Error::msg)?;
    insert_peer(pool, interface_id, user_id, name, &address, user_created).await
}

/// Resolve an admin-supplied device address spec to a concrete address. A bare
/// prefix (e.g. `/64`) auto-allocates a free routed subnet of that size from the
/// interface's range; anything else is validated as an explicit CIDR spec.
async fn resolve_admin_address(pool: &PgPool, interface_id: Uuid, spec: &str) -> Result<String> {
    match wg::parse_bare_prefix(spec) {
        Some(prefix) => {
            let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
            let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
            let taken: Vec<String> =
                list_peers(pool, interface_id).await?.into_iter().map(|p| p.address).collect();
            wg::allocate_subnet(&subnets, &taken, prefix).map_err(anyhow::Error::msg)
        }
        None => wg::validate_address_spec(spec).map_err(anyhow::Error::msg),
    }
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
    let address = resolve_admin_address(pool, interface_id, address_spec).await?;
    insert_peer(pool, interface_id, user_id, name, &address, false).await
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
        Some(s) => resolve_admin_address(pool, interface_id, s).await?,
        None => {
            let iface = get_interface(pool, interface_id).await?.context("interface not found")?;
            let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
            let taken: Vec<String> =
                list_peers(pool, interface_id).await?.into_iter().map(|p| p.address).collect();
            wg::allocate_addresses(&subnets, &taken).map_err(anyhow::Error::msg)?
        }
    };
    sqlx::query_as::<_, Peer>(&format!(
        "INSERT INTO wg_peers (interface_id, user_id, name, public_key, preshared_key, address, user_created) \
         VALUES ($1,$2,$3,'',NULL,$4,false) RETURNING {PEER_COLS}"
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
    user_created: bool,
) -> Result<(Peer, String)> {
    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();
    let peer = sqlx::query_as::<_, Peer>(&format!(
        "INSERT INTO wg_peers \
         (interface_id, user_id, name, public_key, preshared_key, address, user_created) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING {PEER_COLS}"
    ))
    .bind(interface_id)
    .bind(user_id)
    .bind(name)
    .bind(&kp.public_key)
    .bind(&psk)
    .bind(address)
    .bind(user_created)
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
    /// Liveliness lapsed (login TTL passed) — access is suspended until re-login.
    pub deactivated: bool,
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
        let deactivated = !user_is_live(pool, user.id).await?;
        out.push(UserWithCount { user, device_count, deactivated });
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
    let groups = get_groups(pool, &iface.group_ids).await?;
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
                if let Some((email, banned, revoked, live)) =
                    sqlx::query_as::<_, (String, bool, bool, bool)>(
                        "SELECT email, banned, access_revoked, live_until > now() FROM users WHERE id = $1",
                    )
                    .bind(uid)
                    .fetch_optional(pool)
                    .await?
                {
                    let claim_groups = user_claim_groups(pool, uid).await?;
                    if !banned && !revoked && live && access_granted(&groups, &email, &claim_groups) {
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

/// The `.conf` filename offered on download: `<base>-<device>.conf`, where the
/// base is the interface's `download_filename` (falling back to its `name`).
/// Both parts are sanitised to filename-safe characters.
pub fn download_filename(iface: &Interface, device_name: &str) -> String {
    let base = iface
        .download_filename
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&iface.name);
    format!("{}-{}.conf", sanitize_filename(base), sanitize_filename(device_name))
}

/// Reduce an arbitrary label to a safe filename component (alphanumerics plus
/// `-`/`_`/`.`; other runs collapse to a single `-`). Never empty.
fn sanitize_filename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "device".to_string() } else { trimmed }
}

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

    /// Create an interface whose access comes from a single group named `acl`
    /// carrying `patterns`. Returns the interface; the group is fetchable by name.
    async fn mk_iface(pool: &PgPool, patterns: &[String], addr: &str) -> Interface {
        let g = create_group(pool, "acl", patterns, &[]).await.unwrap();
        create_interface(
            pool,
            "wg0",
            "",
            None,
            51820,
            addr,
            "vpn.example.com:51820",
            Some("10.8.0.1"),
            "10.8.0.0/24",
            25,
            Some(5),
            &[g.id],
            BackendConfig::SelfManaged,
        )
        .await
        .unwrap()
    }

    async fn group_id_by_name(pool: &PgPool, name: &str) -> Uuid {
        list_groups(pool).await.unwrap().into_iter().find(|g| g.name == name).unwrap().id
    }

    #[tokio::test]
    async fn dual_stack_alloc_and_subnet_device() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = mk_iface(&pool, &["*".into()], "10.8.0.1/24, fd00:8::1/64").await;
        let (p, privkey) = create_peer(&pool, iface.id, None, "dual", false).await.unwrap();
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

        create_peer(&pool, iface.id, Some(alice), "a", false).await.unwrap();
        create_peer(&pool, iface.id, Some(mallory), "m", false).await.unwrap();

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

        // Widen the group's patterns -> mallory now active. (Raw SQL keeps the
        // test hermetic; update_group would trigger a real backend sync.)
        let gid = group_id_by_name(&pool, "acl").await;
        sqlx::query("UPDATE wg_groups SET patterns = '[\"*\"]'::jsonb WHERE id = $1")
            .bind(gid).execute(&pool).await.unwrap();
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
        // A second interface sharing the same "everyone" group.
        let gid = group_id_by_name(&pool, "acl").await;
        let b = create_interface(&pool, "wg1", "", None, 51821, "10.9.0.1/24", "vpn:51821", None, "10.9.0.0/24", 25, Some(2), &[gid], BackendConfig::SelfManaged).await.unwrap();

        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        create_peer(&pool, a.id, Some(u), "d1", true).await.unwrap();
        create_peer(&pool, a.id, Some(u), "d2", true).await.unwrap();
        create_peer(&pool, b.id, Some(u), "d3", true).await.unwrap();

        assert_eq!(count_user_devices_on_interface(&pool, a.id, u).await.unwrap(), 2);
        assert_eq!(count_user_devices_on_interface(&pool, b.id, u).await.unwrap(), 1);

        // interfaces_for_user sees both (both reference the `*` group).
        assert_eq!(interfaces_for_user(&pool, u, "u@x").await.unwrap().len(), 2);
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

    #[test]
    fn group_grants_by_pattern_or_claim() {
        let g = Group {
            id: Uuid::nil(),
            name: "eng".into(),
            patterns: vec!["*@corp.com".into()],
            claim_values: vec!["engineering".into()],
        };
        // Email pattern.
        assert!(group_grants(&g, "alice@corp.com", &[]));
        // Claim group.
        assert!(group_grants(&g, "bob@other.com", &["engineering".into()]));
        // Neither.
        assert!(!group_grants(&g, "bob@other.com", &["sales".into()]));
        assert!(access_granted(&[g.clone()], "x@corp.com", &[]));
        assert!(!access_granted(&[], "x@corp.com", &["engineering".into()]));
    }

    #[tokio::test]
    async fn claim_groups_union_across_providers_and_gate_access() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        // Interface access via a claim-only group.
        let g = create_group(&pool, "eng", &[], &["engineering".into()]).await.unwrap();
        let iface = create_interface(&pool, "wg0", "", None, 51820, "10.8.0.1/24, fd00:8::1/64",
            "vpn:51820", None, "10.8.0.0/24", 25, Some(5), &[g.id], BackendConfig::SelfManaged).await.unwrap();

        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        create_peer(&pool, iface.id, Some(u), "d", true).await.unwrap();

        // No claims yet -> not active.
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());
        assert!(interfaces_for_user(&pool, u, "u@x").await.unwrap().is_empty());

        // Google login has no groups; plan.ai login carries "engineering".
        set_provider_groups(&pool, u, "google", &[]).await.unwrap();
        set_provider_groups(&pool, u, "planai", &["engineering".into()]).await.unwrap();
        assert_eq!(user_claim_groups(&pool, u).await.unwrap(), vec!["engineering".to_string()]);
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 1);
        assert_eq!(interfaces_for_user(&pool, u, "u@x").await.unwrap().len(), 1);

        // Re-login via plan.ai without the group revokes (that provider's set is replaced).
        set_provider_groups(&pool, u, "planai", &[]).await.unwrap();
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn liveliness_deactivates_stale_accounts() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = mk_iface(&pool, &["*".into()], "10.8.0.1/24").await;
        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        create_peer(&pool, iface.id, Some(u), "d", true).await.unwrap();

        // Fresh users default to live (infinity) -> active.
        assert!(user_is_live(&pool, u).await.unwrap());
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 1);

        // Liveliness lapses -> account deactivated -> devices drop.
        sqlx::query("UPDATE users SET live_until = now() - interval '1 hour' WHERE id = $1")
            .bind(u).execute(&pool).await.unwrap();
        assert!(!user_is_live(&pool, u).await.unwrap());
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());
        assert!(interfaces_for_user(&pool, u, "u@x").await.unwrap().is_empty());

        // Logging in again pushes the deadline out -> reactivated.
        record_login(&pool, u, Some(30)).await.unwrap();
        assert!(user_is_live(&pool, u).await.unwrap());
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn download_filename_defaults_to_name_and_sanitizes() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let g = create_group(&pool, "acl", &["*".into()], &[]).await.unwrap();

        // No override -> base falls back to the interface name.
        let a = create_interface(&pool, "wg0", "plan.ai", None, 51820, "10.8.0.1/24",
            "vpn:51820", None, "10.8.0.0/24", 25, None, &[g.id], BackendConfig::SelfManaged).await.unwrap();
        assert_eq!(download_filename(&a, "My Laptop"), "wg0-My-Laptop.conf");

        // Explicit override is used and sanitised.
        let b = create_interface(&pool, "wg1", "", Some("plan ai vpn"), 51821, "10.9.0.1/24",
            "vpn:51821", None, "10.9.0.0/24", 25, None, &[g.id], BackendConfig::SelfManaged).await.unwrap();
        assert_eq!(b.download_filename.as_deref(), Some("plan ai vpn"));
        assert_eq!(download_filename(&b, "phone/2"), "plan-ai-vpn-phone-2.conf");

        // Blank override is stored as NULL (falls back to name).
        let c = create_interface(&pool, "wg2", "", Some("  "), 51822, "10.10.0.1/24",
            "vpn:51822", None, "10.10.0.0/24", 25, None, &[g.id], BackendConfig::SelfManaged).await.unwrap();
        assert_eq!(c.download_filename, None);
        assert_eq!(download_filename(&c, "x"), "wg2-x.conf");
    }

    #[tokio::test]
    async fn user_created_flag_tracks_origin() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = mk_iface(&pool, &["*".into()], "10.8.0.1/24").await;
        let u: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool).await.unwrap();

        let (mine, _) = create_peer(&pool, iface.id, Some(u), "mine", true).await.unwrap();
        let (admins, _) = create_peer_with_address(&pool, iface.id, Some(u), "admins", "fd00:1::/64").await.unwrap();
        assert!(mine.user_created);
        assert!(!admins.user_created);
        // Rename keeps the flag.
        let renamed = rename_peer(&pool, mine.id, "laptop").await.unwrap();
        assert_eq!(renamed.name, "laptop");
        assert!(renamed.user_created);
    }
}

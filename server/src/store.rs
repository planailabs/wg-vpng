//! Database layer for interfaces and peers, plus backend reconciliation and
//! client-config rendering. Shared by the web UI and the API.

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::backend::{InterfaceSpec, PeerSpec, WireguardBackend};
use crate::config::WireguardConfig;
use crate::wg;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema)]
pub struct Interface {
    pub id: Uuid,
    pub name: String,
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
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema)]
pub struct Peer {
    pub id: Uuid,
    pub interface_id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub private_key: String,
    pub public_key: String,
    pub preshared_key: Option<String>,
    pub address: String,
}

/// A user with access-control + device-limit fields (admin view).
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub banned: bool,
    pub access_revoked: bool,
    /// Per-user override; None → global default.
    pub device_limit: Option<i32>,
}

const USER_COLS: &str = "id, email, name, is_admin, banned, access_revoked, device_limit";

const IFACE_COLS: &str =
    "id, name, listen_port, address, private_key, public_key, endpoint, dns, allowed_ips, keepalive";
const PEER_COLS: &str =
    "id, interface_id, user_id, name, private_key, public_key, preshared_key, address";

// ── Interfaces ────────────────────────────────────────────────────────

/// Seed the configured interface on first boot (generating a keypair), or
/// return the existing stored row. The stored row is authoritative thereafter.
pub async fn ensure_default_interface(pool: &PgPool, cfg: &WireguardConfig) -> Result<Interface> {
    if let Some(existing) = get_interface_by_name(pool, &cfg.interface_name).await? {
        return Ok(existing);
    }
    let kp = wg::generate_keypair();
    let iface = sqlx::query_as::<_, Interface>(&format!(
        "INSERT INTO wg_interfaces \
         (name, listen_port, address, private_key, public_key, endpoint, dns, allowed_ips, keepalive) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING {IFACE_COLS}"
    ))
    .bind(&cfg.interface_name)
    .bind(cfg.listen_port as i32)
    .bind(&cfg.address)
    .bind(&kp.private_key)
    .bind(&kp.public_key)
    .bind(&cfg.endpoint)
    .bind(&cfg.dns)
    .bind(&cfg.allowed_ips)
    .bind(cfg.keepalive as i32)
    .fetch_one(pool)
    .await
    .context("insert default interface")?;
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

pub async fn get_interface_by_name(pool: &PgPool, name: &str) -> Result<Option<Interface>> {
    Ok(sqlx::query_as::<_, Interface>(&format!(
        "SELECT {IFACE_COLS} FROM wg_interfaces WHERE name = $1"
    ))
    .bind(name)
    .fetch_optional(pool)
    .await?)
}

/// Update an interface's client-facing settings (endpoint, DNS, routed
/// networks, keepalive). Keys/address/port are immutable here so existing
/// peers keep working.
pub async fn update_interface(
    pool: &PgPool,
    id: Uuid,
    endpoint: Option<&str>,
    dns: Option<Option<&str>>,
    allowed_ips: Option<&str>,
    keepalive: Option<i32>,
) -> Result<Interface> {
    let mut iface = get_interface(pool, id).await?.context("interface not found")?;
    if let Some(e) = endpoint {
        iface.endpoint = e.to_string();
    }
    if let Some(d) = dns {
        iface.dns = d.map(|s| s.to_string());
    }
    if let Some(a) = allowed_ips {
        iface.allowed_ips = a.to_string();
    }
    if let Some(k) = keepalive {
        iface.keepalive = k;
    }
    let updated = sqlx::query_as::<_, Interface>(&format!(
        "UPDATE wg_interfaces SET endpoint=$2, dns=$3, allowed_ips=$4, keepalive=$5 \
         WHERE id=$1 RETURNING {IFACE_COLS}"
    ))
    .bind(id)
    .bind(&iface.endpoint)
    .bind(&iface.dns)
    .bind(&iface.allowed_ips)
    .bind(iface.keepalive)
    .fetch_one(pool)
    .await
    .context("update interface")?;
    Ok(updated)
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

/// Create a peer on `interface_id` owned by `user_id`: generate a keypair +
/// preshared key and allocate the next free tunnel address in each of the
/// interface's subnets (so dual-stack interfaces yield a v4 + v6 address).
pub async fn create_peer(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
) -> Result<Peer> {
    let iface = get_interface(pool, interface_id)
        .await?
        .context("interface not found")?;
    let subnets = wg::parse_subnets(&iface.address).map_err(anyhow::Error::msg)?;
    let taken: Vec<String> = list_peers(pool, interface_id)
        .await?
        .into_iter()
        .map(|p| p.address)
        .collect();
    let address = wg::allocate_addresses(&subnets, &taken).map_err(anyhow::Error::msg)?;
    insert_peer(pool, interface_id, user_id, name, &address).await
}

/// Create a peer with an explicit address/subnet spec (admin only). The spec is
/// one or more CIDRs of any prefix length — this is how a device is granted a
/// whole routed subnet (e.g. an IPv6 `/64`) rather than a single host address.
pub async fn create_peer_with_address(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
    address_spec: &str,
) -> Result<Peer> {
    let address = wg::validate_address_spec(address_spec).map_err(anyhow::Error::msg)?;
    insert_peer(pool, interface_id, user_id, name, &address).await
}

async fn insert_peer(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
    address: &str,
) -> Result<Peer> {
    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();
    sqlx::query_as::<_, Peer>(&format!(
        "INSERT INTO wg_peers \
         (interface_id, user_id, name, private_key, public_key, preshared_key, address) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING {PEER_COLS}"
    ))
    .bind(interface_id)
    .bind(user_id)
    .bind(name)
    .bind(&kp.private_key)
    .bind(&kp.public_key)
    .bind(&psk)
    .bind(address)
    .fetch_one(pool)
    .await
    .context("insert peer")
}

/// Replace a peer's keypair (and preshared key), keeping its address. Returns
/// the updated peer; the caller must re-sync the backend.
pub async fn regenerate_peer(pool: &PgPool, peer_id: Uuid) -> Result<Peer> {
    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();
    let peer = sqlx::query_as::<_, Peer>(&format!(
        "UPDATE wg_peers SET private_key=$2, public_key=$3, preshared_key=$4 \
         WHERE id=$1 RETURNING {PEER_COLS}"
    ))
    .bind(peer_id)
    .bind(&kp.private_key)
    .bind(&kp.public_key)
    .bind(&psk)
    .fetch_one(pool)
    .await
    .context("regenerate peer")?;
    Ok(peer)
}

/// Rename a peer (device). Cosmetic — no backend change needed.
pub async fn rename_peer(pool: &PgPool, peer_id: Uuid, name: &str) -> Result<Peer> {
    Ok(sqlx::query_as::<_, Peer>(&format!(
        "UPDATE wg_peers SET name=$2 WHERE id=$1 RETURNING {PEER_COLS}"
    ))
    .bind(peer_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .context("rename peer")?)
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
    Ok(sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email) VALUES ($1) \
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .context("upsert user")?)
}

// ── Users / devices ───────────────────────────────────────────────────

/// A user plus their current device count (admin listing).
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

pub async fn count_user_devices(pool: &PgPool, user_id: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM wg_peers WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?)
}

/// Effective device limit for a user: their override, else the global default.
pub fn effective_device_limit(user: &User, global_default: i32) -> i32 {
    user.device_limit.unwrap_or(global_default)
}

pub async fn set_device_limit(pool: &PgPool, user_id: Uuid, limit: Option<i32>) -> Result<()> {
    sqlx::query("UPDATE users SET device_limit = $2 WHERE id = $1")
        .bind(user_id)
        .bind(limit)
        .execute(pool)
        .await?;
    Ok(())
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

/// Delete a user; their devices cascade (and must be dropped from the backend
/// afterwards via a sync).
pub async fn delete_user(pool: &PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Peers whose owner may currently connect: unowned, or owned by a user that
/// is neither banned nor access-revoked. This is what gets pushed to the
/// backend, so revoking/banning immediately cuts a user off.
pub async fn list_active_peers(pool: &PgPool, interface_id: Uuid) -> Result<Vec<Peer>> {
    Ok(sqlx::query_as::<_, Peer>(&format!(
        "SELECT {} FROM wg_peers p LEFT JOIN users u ON p.user_id = u.id \
         WHERE p.interface_id = $1 AND (u.id IS NULL OR (NOT u.banned AND NOT u.access_revoked)) \
         ORDER BY p.created_at",
        PEER_COLS.split(", ").map(|c| format!("p.{c}")).collect::<Vec<_>>().join(", ")
    ))
    .bind(interface_id)
    .fetch_all(pool)
    .await?)
}

// ── Reconcile + render ────────────────────────────────────────────────

/// Push the interface + all its *active* peers to the backend.
pub async fn sync_interface(
    pool: &PgPool,
    backend: &dyn WireguardBackend,
    interface_id: Uuid,
) -> Result<()> {
    let iface = get_interface(pool, interface_id)
        .await?
        .context("interface not found")?;
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

    backend
        .apply(&ispec, &pspecs)
        .await
        .context("backend apply")?;
    Ok(())
}

/// Reconcile every interface (used after a change that can affect any
/// interface's active peer set, e.g. banning or deleting a user).
pub async fn sync_all(pool: &PgPool, backend: &dyn WireguardBackend) -> Result<()> {
    for iface in list_interfaces(pool).await? {
        sync_interface(pool, backend, iface.id).await?;
    }
    Ok(())
}

/// Render the client `.conf` for a peer against its interface.
pub fn render_peer_config(iface: &Interface, peer: &Peer) -> String {
    wg::render_client_config(&wg::ClientConfig {
        private_key: &peer.private_key,
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
    use crate::config::WireguardConfig;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Records the peer set of the most recent `apply`.
    #[derive(Default)]
    struct MockBackend {
        applied: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WireguardBackend for MockBackend {
        async fn apply(&self, _iface: &InterfaceSpec, peers: &[PeerSpec]) -> crate::backend::Result<()> {
            *self.applied.lock().unwrap() = peers.iter().map(|p| p.public_key.clone()).collect();
            Ok(())
        }
        async fn status(&self, _iface_name: &str) -> crate::backend::Result<Vec<PeerStatus>> {
            Ok(vec![])
        }
    }

    fn test_wg_config() -> WireguardConfig {
        WireguardConfig {
            backend: crate::backend::BackendConfig::SelfManaged,
            interface_name: "wg0".into(),
            listen_port: 51820,
            address: "10.8.0.1/24".into(),
            endpoint: "vpn.example.com:51820".into(),
            dns: Some("10.8.0.1".into()),
            allowed_ips: "10.8.0.0/24".into(),
            keepalive: 25,
            device_limit: 5,
        }
    }

    // Full peer lifecycle against a throwaway postgres, asserting the backend
    // is reconciled to the DB at each step.
    #[tokio::test]
    async fn peer_lifecycle_reconciles_backend() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = ensure_default_interface(&pool, &test_wg_config()).await.unwrap();
        // Idempotent: second call returns the same row (same keys).
        let again = ensure_default_interface(&pool, &test_wg_config()).await.unwrap();
        assert_eq!(iface.public_key, again.public_key);

        let backend = MockBackend::default();

        // Create two peers -> .2 and .3 (server holds .1).
        let p1 = create_peer(&pool, iface.id, None, "a").await.unwrap();
        let p2 = create_peer(&pool, iface.id, None, "b").await.unwrap();
        assert_eq!(p1.address, "10.8.0.2/32");
        assert_eq!(p2.address, "10.8.0.3/32");

        sync_interface(&pool, &backend, iface.id).await.unwrap();
        assert_eq!(backend.applied.lock().unwrap().len(), 2);

        // Regenerate p1: new key, same address; backend follows.
        let old_pub = p1.public_key.clone();
        let p1b = regenerate_peer(&pool, p1.id).await.unwrap();
        assert_ne!(p1b.public_key, old_pub);
        assert_eq!(p1b.address, p1.address);
        sync_interface(&pool, &backend, iface.id).await.unwrap();
        {
            let applied = backend.applied.lock().unwrap();
            assert!(applied.contains(&p1b.public_key));
            assert!(!applied.contains(&old_pub));
        }

        // Config renders with the regenerated private key.
        let cfg = render_peer_config(&iface, &p1b);
        assert!(cfg.contains(&format!("PrivateKey = {}", p1b.private_key)));
        assert!(cfg.contains("Endpoint = vpn.example.com:51820"));

        // Delete p2: backend left with one peer.
        delete_peer(&pool, p2.id).await.unwrap();
        sync_interface(&pool, &backend, iface.id).await.unwrap();
        assert_eq!(backend.applied.lock().unwrap().len(), 1);
    }

    // Revoking / banning a user drops their devices from the backend's active
    // set; deleting the user removes them entirely.
    #[tokio::test]
    async fn access_control_gates_active_peers() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let iface = ensure_default_interface(&pool, &test_wg_config()).await.unwrap();
        let uid: Uuid = sqlx::query_scalar("INSERT INTO users (email) VALUES ('u@x') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();

        create_peer(&pool, iface.id, Some(uid), "d1").await.unwrap();
        create_peer(&pool, iface.id, Some(uid), "d2").await.unwrap();
        assert_eq!(count_user_devices(&pool, uid).await.unwrap(), 2);
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 2);

        // Revoke → no active peers, but rows retained.
        set_access_revoked(&pool, uid, true).await.unwrap();
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());
        assert_eq!(count_user_devices(&pool, uid).await.unwrap(), 2);

        // Restore → active again.
        set_access_revoked(&pool, uid, false).await.unwrap();
        assert_eq!(list_active_peers(&pool, iface.id).await.unwrap().len(), 2);

        // Ban → inactive.
        set_banned(&pool, uid, true).await.unwrap();
        assert!(list_active_peers(&pool, iface.id).await.unwrap().is_empty());

        // Delete user → devices cascade away.
        delete_user(&pool, uid).await.unwrap();
        assert_eq!(list_peers(&pool, iface.id).await.unwrap().len(), 0);

        // Per-user limit override beats the global default.
        let u2: User = sqlx::query_as::<_, User>(&format!(
            "INSERT INTO users (email, device_limit) VALUES ('u2@x', 2) RETURNING {USER_COLS}"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(effective_device_limit(&u2, 5), 2);
    }

    // Dual-stack interface yields v4+v6 device addresses; admins can assign a
    // whole routed subnet to a device.
    #[tokio::test]
    async fn dual_stack_and_subnet_device() {
        let db = pgtemp::PgTempDB::async_new().await;
        let pool = sqlx::PgPool::connect(&db.connection_uri()).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let mut cfg = test_wg_config();
        cfg.address = "10.8.0.1/24, fd00:8::1/64".into();
        cfg.allowed_ips = "10.8.0.0/24, fd00:8::/64".into();
        let iface = ensure_default_interface(&pool, &cfg).await.unwrap();

        let p = create_peer(&pool, iface.id, None, "dual").await.unwrap();
        assert_eq!(p.address, "10.8.0.2/32, fd00:8::2/128");

        // Admin assigns an IPv6 /64 subnet to a device (a routed network).
        let sub = create_peer_with_address(&pool, iface.id, None, "site", "fd00:beef::/64")
            .await
            .unwrap();
        assert_eq!(sub.address, "fd00:beef::/64");

        // The rendered client config carries both the dual-stack address and
        // the dual-stack routed networks.
        let cfg_text = render_peer_config(&iface, &p);
        assert!(cfg_text.contains("Address = 10.8.0.2/32, fd00:8::2/128"));
        assert!(cfg_text.contains("AllowedIPs = 10.8.0.0/24, fd00:8::/64"));
    }
}

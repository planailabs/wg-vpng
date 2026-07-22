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
/// preshared key and allocate the next free tunnel address.
pub async fn create_peer(
    pool: &PgPool,
    interface_id: Uuid,
    user_id: Option<Uuid>,
    name: &str,
) -> Result<Peer> {
    let iface = get_interface(pool, interface_id)
        .await?
        .context("interface not found")?;
    let taken: Vec<String> = list_peers(pool, interface_id)
        .await?
        .into_iter()
        .map(|p| p.address)
        .collect();
    let address = wg::allocate_address(&iface.address, &iface.address, &taken)
        .map_err(anyhow::Error::msg)?;

    let kp = wg::generate_keypair();
    let psk = wg::generate_preshared_key();

    let peer = sqlx::query_as::<_, Peer>(&format!(
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
    .bind(&address)
    .fetch_one(pool)
    .await
    .context("insert peer")?;
    Ok(peer)
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

pub async fn delete_peer(pool: &PgPool, peer_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM wg_peers WHERE id = $1")
        .bind(peer_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Reconcile + render ────────────────────────────────────────────────

/// Push the interface + its full peer set to the backend.
pub async fn sync_interface(
    pool: &PgPool,
    backend: &dyn WireguardBackend,
    interface_id: Uuid,
) -> Result<()> {
    let iface = get_interface(pool, interface_id)
        .await?
        .context("interface not found")?;
    let peers = list_peers(pool, interface_id).await?;

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
}

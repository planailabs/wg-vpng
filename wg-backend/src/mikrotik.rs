//! MikroTik RouterOS backend: reconcile the interface + peer set through the
//! REST API (via the `mikrotik-api` crate). Full-state apply = ensure the
//! interface, then add/update desired peers and delete any the router still
//! has that we no longer want.

use async_trait::async_trait;

use crate::{InterfaceSpec, PeerSpec, PeerStatus, Result, WireguardBackend};

pub struct MikrotikBackend {
    client: mikrotik_api::Client,
}

impl MikrotikBackend {
    pub fn new(url: &str, username: &str, password: &str, insecure: bool) -> Result<Self> {
        Ok(Self {
            client: mikrotik_api::Client::new(url, username, password, insecure)?,
        })
    }
}

#[async_trait]
impl WireguardBackend for MikrotikBackend {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        self.client
            .ensure_interface(&iface.name, iface.listen_port, &iface.private_key)
            .await?;

        let existing = self.client.list_peers(&iface.name).await?;
        let desired: std::collections::HashSet<&str> = peers.iter().map(|p| p.public_key.as_str()).collect();

        // Remove peers the router still has that we no longer want.
        for e in &existing {
            if !desired.contains(e.public_key.as_str()) {
                self.client.remove_peer(&iface.name, &e.public_key).await?;
            }
        }
        // Add / update desired peers.
        for p in peers {
            self.client
                .upsert_peer(&iface.name, &p.public_key, &p.address, p.preshared_key.as_deref())
                .await?;
        }
        Ok(())
    }

    async fn remove(&self, iface_name: &str) -> Result<()> {
        self.client.remove_interface(iface_name).await?;
        Ok(())
    }

    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        let peers = self.client.list_peers(iface_name).await?;
        Ok(peers
            .into_iter()
            .map(|p| PeerStatus {
                public_key: p.public_key,
                endpoint: p.endpoint,
                // RouterOS reports handshake as a human duration string, not a
                // unix ts; surface presence only.
                last_handshake: p.last_handshake.and(Some(1)),
            })
            .collect())
    }
}

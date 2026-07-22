//! NetworkManager backend: render a keyfile connection profile for the
//! interface + all peers, drop it in NetworkManager's system-connections dir,
//! then `nmcli connection reload` + `up`. NM owns the whole profile, so a
//! full-state rewrite on every `apply` is the natural fit.

use async_trait::async_trait;
use tokio::process::Command;

use super::{BackendError, InterfaceSpec, PeerSpec, PeerStatus, Result, WireguardBackend};

const DEFAULT_KEYFILE_DIR: &str = "/etc/NetworkManager/system-connections";

pub struct NetworkManagerBackend {
    keyfile_dir: String,
}

impl NetworkManagerBackend {
    pub fn new() -> Self {
        // Overridable for tests / non-standard installs.
        let keyfile_dir = std::env::var("WG_VPNG_NM_KEYFILE_DIR")
            .unwrap_or_else(|_| DEFAULT_KEYFILE_DIR.to_string());
        Self { keyfile_dir }
    }
}

impl Default for NetworkManagerBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a NetworkManager keyfile (INI) for the interface + peers.
pub fn render_keyfile(iface: &InterfaceSpec, peers: &[PeerSpec]) -> String {
    let mut s = String::new();
    s.push_str("[connection]\n");
    s.push_str(&format!("id={}\n", iface.name));
    s.push_str("type=wireguard\n");
    s.push_str(&format!("interface-name={}\n\n", iface.name));

    s.push_str("[wireguard]\n");
    s.push_str(&format!("private-key={}\n", iface.private_key));
    s.push_str(&format!("listen-port={}\n\n", iface.listen_port));

    for p in peers {
        s.push_str(&format!("[wireguard-peer.{}]\n", p.public_key));
        s.push_str(&format!("allowed-ips={};\n", p.address));
        if let Some(psk) = p.preshared_key.as_deref().filter(|k| !k.is_empty()) {
            s.push_str(&format!("preshared-key={psk}\n"));
            s.push_str("preshared-key-flags=0\n");
        }
        s.push('\n');
    }

    s.push_str("[ipv4]\n");
    s.push_str(&format!("address1={}\n", iface.address));
    s.push_str("method=manual\n\n");
    s.push_str("[ipv6]\n");
    s.push_str("method=ignore\n");
    s
}

async fn nmcli(args: &[&str]) -> Result<()> {
    let out = Command::new("nmcli").args(args).output().await?;
    if !out.status.success() {
        return Err(BackendError::Command(format!(
            "nmcli {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[async_trait]
impl WireguardBackend for NetworkManagerBackend {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let content = render_keyfile(iface, peers);
        let path = std::path::Path::new(&self.keyfile_dir).join(format!("{}.nmconnection", iface.name));
        tokio::fs::write(&path, content).await?;
        // Keyfiles hold private keys — NM refuses to load world-readable ones.
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;

        nmcli(&["connection", "reload"]).await?;
        nmcli(&["connection", "up", &iface.name]).await?;
        Ok(())
    }

    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        // NM creates a real kernel wg interface, so `wg show` still works.
        match Command::new("wg").args(["show", iface_name, "dump"]).output().await {
            Ok(out) if out.status.success() => {
                Ok(super::selfmanaged::parse_wg_dump(&String::from_utf8_lossy(&out.stdout)))
            }
            _ => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyfile_contains_interface_and_peer_sections() {
        let iface = InterfaceSpec {
            name: "wg0".into(),
            listen_port: 51820,
            address: "10.8.0.1/24".into(),
            private_key: "SRVPRIV".into(),
            public_key: "SRVPUB".into(),
        };
        let peers = vec![PeerSpec {
            public_key: "PEERPUB".into(),
            address: "10.8.0.2/32".into(),
            preshared_key: Some("PSK".into()),
        }];
        let k = render_keyfile(&iface, &peers);
        assert!(k.contains("type=wireguard"));
        assert!(k.contains("private-key=SRVPRIV"));
        assert!(k.contains("listen-port=51820"));
        assert!(k.contains("[wireguard-peer.PEERPUB]"));
        assert!(k.contains("allowed-ips=10.8.0.2/32;"));
        assert!(k.contains("preshared-key=PSK"));
        assert!(k.contains("address1=10.8.0.1/24"));
    }
}

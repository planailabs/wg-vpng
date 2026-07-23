//! systemd-networkd backend: render a `.netdev` (the WireGuard device + peers)
//! and a `.network` (its addresses) into networkd's config dir, then
//! `networkctl reload`. networkd owns the whole device, so a full-state rewrite
//! on every `apply` is the natural fit — same shape as the NetworkManager
//! backend, different config format.

use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;

use crate::{parse_wg_dump, BackendError, InterfaceSpec, PeerSpec, PeerStatus, Result, WireguardBackend};

const DEFAULT_DIR: &str = "/etc/systemd/network";

pub struct SystemdNetworkdBackend {
    dir: String,
}

impl SystemdNetworkdBackend {
    pub fn new() -> Self {
        // Overridable for tests / non-standard installs.
        let dir = std::env::var("WG_VPNG_NETWORKD_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_string());
        Self { dir }
    }
}

impl Default for SystemdNetworkdBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Split a comma/space-separated CIDR spec into individual trimmed CIDRs.
fn cidrs(spec: &str) -> impl Iterator<Item = &str> {
    spec.split([',', ' ']).map(str::trim).filter(|a| !a.is_empty())
}

/// Render the `.netdev`: the WireGuard device, its key/port, and every peer.
pub fn render_netdev(iface: &InterfaceSpec, peers: &[PeerSpec]) -> String {
    let mut s = String::new();
    s.push_str("[NetDev]\n");
    s.push_str(&format!("Name={}\n", iface.name));
    s.push_str("Kind=wireguard\n\n");

    s.push_str("[WireGuard]\n");
    s.push_str(&format!("PrivateKey={}\n", iface.private_key));
    s.push_str(&format!("ListenPort={}\n", iface.listen_port));

    for p in peers {
        s.push_str("\n[WireGuardPeer]\n");
        s.push_str(&format!("PublicKey={}\n", p.public_key));
        if let Some(psk) = p.preshared_key.as_deref().filter(|k| !k.is_empty()) {
            s.push_str(&format!("PresharedKey={psk}\n"));
        }
        let allowed = cidrs(&p.address).collect::<Vec<_>>().join(",");
        s.push_str(&format!("AllowedIPs={allowed}\n"));
    }
    s
}

/// Render the `.network`: match the device by name, assign its addresses.
pub fn render_network(iface: &InterfaceSpec) -> String {
    let mut s = String::new();
    s.push_str("[Match]\n");
    s.push_str(&format!("Name={}\n\n", iface.name));
    s.push_str("[Network]\n");
    for addr in cidrs(&iface.address) {
        s.push_str(&format!("Address={addr}\n"));
    }
    s
}

async fn networkctl(args: &[&str]) -> Result<()> {
    let out = Command::new("networkctl")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !out.status.success() {
        return Err(BackendError::Command(format!(
            "networkctl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[async_trait]
impl WireguardBackend for SystemdNetworkdBackend {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::path::Path::new(&self.dir);
        let netdev = dir.join(format!("{}.netdev", iface.name));
        let network = dir.join(format!("{}.network", iface.name));

        tokio::fs::write(&netdev, render_netdev(iface, peers)).await?;
        // The .netdev holds the private key — networkd refuses world-readable
        // secrets. 0640 root:systemd-network; the chgrp is best-effort (the
        // group may be absent on non-networkd hosts).
        tokio::fs::set_permissions(&netdev, std::fs::Permissions::from_mode(0o640)).await?;
        let _ = Command::new("chgrp")
            .args(["systemd-network", &netdev.to_string_lossy()])
            .status()
            .await;
        tokio::fs::write(&network, render_network(iface)).await?;

        networkctl(&["reload"]).await?;
        // reload creates new devices but won't reconfigure an existing one's
        // peers; nudge it. Best-effort — the device may not exist yet.
        let _ = networkctl(&["reconfigure", &iface.name]).await;
        Ok(())
    }

    async fn remove(&self, iface_name: &str) -> Result<()> {
        let dir = std::path::Path::new(&self.dir);
        let _ = tokio::fs::remove_file(dir.join(format!("{iface_name}.netdev"))).await;
        let _ = tokio::fs::remove_file(dir.join(format!("{iface_name}.network"))).await;
        let _ = networkctl(&["reload"]).await;
        // Reloading doesn't delete an already-created device; tear it down.
        let _ = Command::new("ip").args(["link", "del", "dev", iface_name]).status().await;
        Ok(())
    }

    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        // networkd creates a real kernel wg interface, so `wg show` works.
        match Command::new("wg").args(["show", iface_name, "dump"]).output().await {
            Ok(out) if out.status.success() => Ok(parse_wg_dump(&String::from_utf8_lossy(&out.stdout))),
            _ => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface() -> InterfaceSpec {
        InterfaceSpec {
            name: "wg0".into(),
            listen_port: 51820,
            address: "10.8.0.1/24, fd00:8::1/64".into(),
            private_key: "SRVPRIV".into(),
        }
    }

    #[test]
    fn netdev_has_device_and_peers() {
        let peers = vec![
            PeerSpec { public_key: "P1".into(), address: "10.8.0.2/32, fd00:8::2/128".into(), preshared_key: Some("K1".into()) },
            PeerSpec { public_key: "P2".into(), address: "10.8.0.3/32".into(), preshared_key: None },
        ];
        let n = render_netdev(&iface(), &peers);
        assert!(n.contains("Kind=wireguard"));
        assert!(n.contains("PrivateKey=SRVPRIV"));
        assert!(n.contains("ListenPort=51820"));
        assert!(n.contains("PublicKey=P1"));
        assert!(n.contains("PresharedKey=K1"));
        assert!(n.contains("AllowedIPs=10.8.0.2/32,fd00:8::2/128"));
        assert!(n.contains("PublicKey=P2"));
        assert_eq!(n.matches("PresharedKey").count(), 1);
    }

    #[test]
    fn network_has_dual_stack_addresses() {
        let net = render_network(&iface());
        assert!(net.contains("Name=wg0"));
        assert!(net.contains("Address=10.8.0.1/24"));
        assert!(net.contains("Address=fd00:8::1/64"));
    }
}

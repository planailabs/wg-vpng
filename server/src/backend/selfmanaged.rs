//! Self-managed backend: a kernel WireGuard interface driven by `wg` + `ip`.
//!
//! `apply` is a full reconcile: ensure the link exists, is addressed and up,
//! then hand `wg syncconf` a freshly rendered server config so peers converge
//! to the desired set (syncconf adds/removes/updates without dropping the
//! interface). We shell out to `wg`/`ip` because they *are* the interface to
//! the kernel WireGuard module — there is no in-process equivalent, and this
//! is exactly the "the subprocess is the thing being configured" exception.

use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{BackendError, InterfaceSpec, PeerSpec, PeerStatus, Result, WireguardBackend};

pub struct SelfManagedBackend;

impl SelfManagedBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SelfManagedBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the server-side `wg` config consumed by `wg syncconf`. Only the
/// `[Interface]` key material + `[Peer]` sections matter to syncconf (address
/// and link state are handled separately via `ip`).
pub fn render_server_config(iface: &InterfaceSpec, peers: &[PeerSpec]) -> String {
    let mut s = String::new();
    s.push_str("[Interface]\n");
    s.push_str(&format!("ListenPort = {}\n", iface.listen_port));
    s.push_str(&format!("PrivateKey = {}\n", iface.private_key));
    for p in peers {
        s.push_str("\n[Peer]\n");
        s.push_str(&format!("PublicKey = {}\n", p.public_key));
        if let Some(psk) = p.preshared_key.as_deref().filter(|k| !k.is_empty()) {
            s.push_str(&format!("PresharedKey = {psk}\n"));
        }
        s.push_str(&format!("AllowedIPs = {}\n", p.address));
    }
    s
}

async fn run(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output().await?;
    if !out.status.success() {
        return Err(BackendError::Command(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run a command that reads its stdin (used to feed `wg syncconf /dev/stdin`).
async fn run_stdin(cmd: &str, args: &[&str], stdin_data: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(stdin_data.as_bytes())
        .await?;
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(BackendError::Command(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[async_trait]
impl WireguardBackend for SelfManagedBackend {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        let name = &iface.name;

        // Create the link if it doesn't exist yet (ignore "exists").
        if run("ip", &["link", "show", name]).await.is_err() {
            run("ip", &["link", "add", "dev", name, "type", "wireguard"]).await?;
        }

        // Ensure each server address is present (dual-stack; ignore "exists").
        for addr in iface.address.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let _ = run("ip", &["address", "add", addr, "dev", name]).await;
        }

        // Bring it up.
        run("ip", &["link", "set", "up", "dev", name]).await?;

        // Reconcile key material + peers in one shot.
        let cfg = render_server_config(iface, peers);
        run_stdin("wg", &["syncconf", name, "/dev/stdin"], &cfg).await?;
        Ok(())
    }

    async fn remove(&self, iface_name: &str) -> Result<()> {
        // Best-effort: ignore "does not exist".
        let _ = run("ip", &["link", "del", "dev", iface_name]).await;
        Ok(())
    }

    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        let dump = match run("wg", &["show", iface_name, "dump"]).await {
            Ok(d) => d,
            Err(_) => return Ok(vec![]),
        };
        Ok(parse_wg_dump(&dump))
    }
}

/// Parse `wg show <if> dump`. The first line is the interface; each following
/// line is a peer: pubkey, psk, endpoint, allowed-ips, latest-handshake, ...
pub fn parse_wg_dump(dump: &str) -> Vec<PeerStatus> {
    dump.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 5 {
                return None;
            }
            let endpoint = match f[2] {
                "(none)" | "" => None,
                e => Some(e.to_string()),
            };
            let last_handshake = f[4].parse::<i64>().ok().filter(|&t| t > 0);
            Some(PeerStatus {
                public_key: f[0].to_string(),
                endpoint,
                last_handshake,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface() -> InterfaceSpec {
        InterfaceSpec {
            name: "wg0".into(),
            listen_port: 51820,
            address: "10.8.0.1/24".into(),
            private_key: "SRVPRIV".into(),
        }
    }

    #[test]
    fn server_config_has_interface_and_peers() {
        let peers = vec![
            PeerSpec { public_key: "P1".into(), address: "10.8.0.2/32".into(), preshared_key: Some("K1".into()) },
            PeerSpec { public_key: "P2".into(), address: "10.8.0.3/32".into(), preshared_key: None },
        ];
        let c = render_server_config(&iface(), &peers);
        assert!(c.contains("ListenPort = 51820"));
        assert!(c.contains("PrivateKey = SRVPRIV"));
        assert!(c.contains("PublicKey = P1"));
        assert!(c.contains("PresharedKey = K1"));
        assert!(c.contains("AllowedIPs = 10.8.0.2/32"));
        assert!(c.contains("PublicKey = P2"));
        // P2 has no PSK line right after it.
        assert_eq!(c.matches("PresharedKey").count(), 1);
    }

    #[test]
    fn parses_wg_dump() {
        let dump = "SRVPRIV\tSRVPUB\t51820\toff\n\
                    PEERPUB\t(none)\t1.2.3.4:5678\t10.8.0.2/32\t1700000000\t0\t0\toff";
        let peers = parse_wg_dump(dump);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].public_key, "PEERPUB");
        assert_eq!(peers[0].endpoint.as_deref(), Some("1.2.3.4:5678"));
        assert_eq!(peers[0].last_handshake, Some(1700000000));
    }

    #[test]
    fn dump_zero_handshake_is_none() {
        let dump = "a\tb\tc\td\nPUB\t(none)\t(none)\t10.8.0.2/32\t0\t0\t0\toff";
        let peers = parse_wg_dump(dump);
        assert_eq!(peers[0].last_handshake, None);
        assert_eq!(peers[0].endpoint, None);
    }
}

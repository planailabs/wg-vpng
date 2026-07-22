//! WireGuard primitives: Curve25519 keypairs, preshared keys, client-config
//! rendering, and address allocation. No handrolled crypto — keys come from
//! `x25519-dalek` (the same X25519 WireGuard itself uses).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

/// A base64-encoded Curve25519 keypair in WireGuard's wire format.
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

/// Generate a fresh WireGuard keypair.
pub fn generate_keypair() -> KeyPair {
    use rand::rngs::OsRng;
    let secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let public = x25519_dalek::PublicKey::from(&secret);
    KeyPair {
        private_key: B64.encode(secret.to_bytes()),
        public_key: B64.encode(public.as_bytes()),
    }
}

/// Derive the public key for an existing base64 private key (used when
/// importing a key or validating stored state).
pub fn public_from_private(private_b64: &str) -> Result<String, String> {
    let bytes = B64
        .decode(private_b64.trim())
        .map_err(|e| format!("invalid base64 private key: {e}"))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "private key must be 32 bytes".to_string())?;
    let secret = x25519_dalek::StaticSecret::from(arr);
    let public = x25519_dalek::PublicKey::from(&secret);
    Ok(B64.encode(public.as_bytes()))
}

/// Generate a 32-byte preshared key (base64).
pub fn generate_preshared_key() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    B64.encode(buf)
}

/// Inputs for rendering a peer's client-side `.conf` file.
pub struct ClientConfig<'a> {
    /// The peer's own private key.
    pub private_key: &'a str,
    /// The address assigned to the peer, e.g. `10.8.0.5/32`.
    pub address: &'a str,
    /// Optional DNS server(s) pushed to the client.
    pub dns: Option<&'a str>,
    /// The server interface's public key.
    pub server_public_key: &'a str,
    /// Optional preshared key shared with the server peer entry.
    pub preshared_key: Option<&'a str>,
    /// Public `host:port` the client dials.
    pub endpoint: &'a str,
    /// Networks routed through the tunnel, e.g. `10.8.0.0/24`.
    pub allowed_ips: &'a str,
    /// Keepalive seconds (0 disables).
    pub persistent_keepalive: u16,
}

/// Render a WireGuard client configuration file.
pub fn render_client_config(c: &ClientConfig) -> String {
    let mut s = String::new();
    s.push_str("[Interface]\n");
    s.push_str(&format!("PrivateKey = {}\n", c.private_key));
    s.push_str(&format!("Address = {}\n", c.address));
    if let Some(dns) = c.dns.filter(|d| !d.is_empty()) {
        s.push_str(&format!("DNS = {dns}\n"));
    }
    s.push('\n');
    s.push_str("[Peer]\n");
    s.push_str(&format!("PublicKey = {}\n", c.server_public_key));
    if let Some(psk) = c.preshared_key.filter(|p| !p.is_empty()) {
        s.push_str(&format!("PresharedKey = {psk}\n"));
    }
    s.push_str(&format!("Endpoint = {}\n", c.endpoint));
    s.push_str(&format!("AllowedIPs = {}\n", c.allowed_ips));
    if c.persistent_keepalive > 0 {
        s.push_str(&format!("PersistentKeepalive = {}\n", c.persistent_keepalive));
    }
    s
}

/// Pick the lowest free host address in `cidr` (e.g. `10.8.0.0/24`), skipping
/// the network address and any address already in `taken` (each `taken` entry
/// may carry a `/xx` suffix, which is ignored). Returns `10.8.0.N/32`.
pub fn allocate_address(cidr: &str, server_address: &str, taken: &[String]) -> Result<String, String> {
    use std::net::Ipv4Addr;
    let net: ipnet::Ipv4Net = cidr
        .parse()
        .map_err(|e| format!("invalid interface cidr {cidr}: {e}"))?;

    let strip = |a: &str| -> Option<Ipv4Addr> { a.split('/').next()?.trim().parse().ok() };
    let mut used: std::collections::HashSet<Ipv4Addr> = taken.iter().filter_map(|a| strip(a)).collect();
    if let Some(sa) = strip(server_address) {
        used.insert(sa);
    }

    for host in net.hosts() {
        if host == net.network() || host == net.broadcast() {
            continue;
        }
        if !used.contains(&host) {
            return Ok(format!("{host}/32"));
        }
    }
    Err(format!("no free addresses left in {cidr}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_roundtrips_public() {
        let kp = generate_keypair();
        // 32 bytes base64 => 44 chars with padding.
        assert_eq!(kp.private_key.len(), 44);
        assert_eq!(kp.public_key.len(), 44);
        assert_eq!(public_from_private(&kp.private_key).unwrap(), kp.public_key);
    }

    #[test]
    fn keypairs_are_unique() {
        assert_ne!(generate_keypair().private_key, generate_keypair().private_key);
    }

    #[test]
    fn preshared_key_is_32_bytes() {
        let psk = generate_preshared_key();
        assert_eq!(B64.decode(psk).unwrap().len(), 32);
    }

    #[test]
    fn renders_expected_config() {
        let cfg = render_client_config(&ClientConfig {
            private_key: "PRIV",
            address: "10.8.0.5/32",
            dns: Some("10.8.0.1"),
            server_public_key: "SRVPUB",
            preshared_key: Some("PSK"),
            endpoint: "vpn.example.com:51820",
            allowed_ips: "10.8.0.0/24",
            persistent_keepalive: 25,
        });
        assert!(cfg.contains("PrivateKey = PRIV"));
        assert!(cfg.contains("Address = 10.8.0.5/32"));
        assert!(cfg.contains("DNS = 10.8.0.1"));
        assert!(cfg.contains("PublicKey = SRVPUB"));
        assert!(cfg.contains("PresharedKey = PSK"));
        assert!(cfg.contains("Endpoint = vpn.example.com:51820"));
        assert!(cfg.contains("AllowedIPs = 10.8.0.0/24"));
        assert!(cfg.contains("PersistentKeepalive = 25"));
    }

    #[test]
    fn omits_optional_fields() {
        let cfg = render_client_config(&ClientConfig {
            private_key: "P",
            address: "10.8.0.5/32",
            dns: None,
            server_public_key: "S",
            preshared_key: None,
            endpoint: "e:1",
            allowed_ips: "0.0.0.0/0",
            persistent_keepalive: 0,
        });
        assert!(!cfg.contains("DNS ="));
        assert!(!cfg.contains("PresharedKey ="));
        assert!(!cfg.contains("PersistentKeepalive ="));
    }

    #[test]
    fn allocates_lowest_free_skipping_server_and_taken() {
        // .1 is the server; .2 taken -> next is .3
        let a = allocate_address("10.8.0.0/24", "10.8.0.1/24", &["10.8.0.2/32".into()]).unwrap();
        assert_eq!(a, "10.8.0.3/32");
    }

    #[test]
    fn allocation_exhaustion_errors() {
        // /30 has hosts .1 and .2; server takes .1, peer takes .2 -> none left.
        let r = allocate_address("10.9.0.0/30", "10.9.0.1/30", &["10.9.0.2/32".into()]);
        assert!(r.is_err());
    }
}

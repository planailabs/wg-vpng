//! Minimal RouterOS v7 REST client for managing WireGuard interfaces and peers.
//!
//! RouterOS exposes `/rest/...` over HTTPS with HTTP Basic auth. We only touch
//! the two paths wg-vpng needs:
//!   - `/rest/interface/wireguard`        (the server interface)
//!   - `/rest/interface/wireguard/peers`  (one entry per user)
//!
//! The URL/body construction lives in pure functions (unit-tested); the async
//! methods just perform the HTTP round-trips.

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("router returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("peer with public key {0} not found")]
    PeerNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// An `/ip/address` or `/ipv6/address` entry as returned by RouterOS.
#[derive(Debug, Clone, Deserialize)]
pub struct Address {
    #[serde(rename = ".id")]
    pub id: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub comment: String,
}

/// A WireGuard peer as returned by RouterOS (`.id` is RouterOS's internal id).
#[derive(Debug, Clone, Deserialize)]
pub struct Peer {
    #[serde(rename = ".id")]
    pub id: String,
    #[serde(rename = "public-key", default)]
    pub public_key: String,
    #[serde(rename = "allowed-address", default)]
    pub allowed_address: String,
    #[serde(rename = "current-endpoint-address", default)]
    pub endpoint: Option<String>,
    #[serde(rename = "last-handshake", default)]
    pub last_handshake: Option<String>,
}

pub struct Client {
    base: String,
    user: String,
    password: String,
    http: reqwest::Client,
}

impl Client {
    /// `base_url` is the router's REST root, e.g. `https://10.0.0.1`.
    /// `insecure` skips TLS verification (RouterOS ships a self-signed cert).
    pub fn new(base_url: impl Into<String>, user: impl Into<String>, password: impl Into<String>, insecure: bool) -> Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(insecure)
            .build()?;
        Ok(Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            user: user.into(),
            password: password.into(),
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/rest/{}", self.base, path.trim_start_matches('/'))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let resp = req.basic_auth(&self.user, Some(&self.password)).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Status { status: status.as_u16(), body: text });
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Create the interface if missing, otherwise update its port/key.
    pub async fn ensure_interface(&self, name: &str, listen_port: u16, private_key: &str) -> Result<()> {
        let existing = self.send(self.http.get(self.url(&format!("interface/wireguard?name={name}")))).await?;
        let body = interface_body(name, listen_port, private_key);
        if let Some(id) = first_id(&existing) {
            self.send(self.http.patch(self.url(&format!("interface/wireguard/{id}"))).json(&body)).await?;
        } else {
            self.send(self.http.put(self.url("interface/wireguard")).json(&body)).await?;
        }
        Ok(())
    }

    /// Add the peer, or update its allowed-address / preshared-key if the
    /// public key already exists on the interface.
    pub async fn upsert_peer(&self, interface: &str, public_key: &str, allowed_address: &str, preshared_key: Option<&str>) -> Result<()> {
        let body = peer_body(interface, public_key, allowed_address, preshared_key);
        if let Some(id) = self.find_peer_id(interface, public_key).await? {
            self.send(self.http.patch(self.url(&format!("interface/wireguard/peers/{id}"))).json(&body)).await?;
        } else {
            self.send(self.http.put(self.url("interface/wireguard/peers")).json(&body)).await?;
        }
        Ok(())
    }

    pub async fn remove_peer(&self, interface: &str, public_key: &str) -> Result<()> {
        let id = self
            .find_peer_id(interface, public_key)
            .await?
            .ok_or_else(|| Error::PeerNotFound(public_key.to_string()))?;
        self.send(self.http.delete(self.url(&format!("interface/wireguard/peers/{id}")))).await?;
        Ok(())
    }

    /// Reconcile the interface's IP addresses to `addresses` (comma/space-free
    /// list of CIDRs; v4 and v6 mixed). Each address we manage is tagged with
    /// comment `wg-vpng:<interface>`, and we own exactly the set carrying that
    /// comment — so stale ones are removed and missing ones added.
    pub async fn reconcile_addresses(&self, interface: &str, addresses: &[String]) -> Result<()> {
        let comment = wg_comment(interface);
        let (v6, v4): (Vec<&String>, Vec<&String>) =
            addresses.iter().partition(|a| a.contains(':'));
        self.reconcile_addr_family("ip/address", interface, &comment, &v4, false).await?;
        self.reconcile_addr_family("ipv6/address", interface, &comment, &v6, true).await?;
        Ok(())
    }

    /// Reconcile one address family (`ip/address` or `ipv6/address`).
    async fn reconcile_addr_family(
        &self,
        path: &str,
        interface: &str,
        comment: &str,
        desired: &[&String],
        v6: bool,
    ) -> Result<()> {
        let v = self.send(self.http.get(self.url(&format!("{path}?interface={interface}")))).await?;
        let existing: Vec<Address> = serde_json::from_value(v).unwrap_or_default();
        // We only touch addresses we own (by comment).
        let owned: Vec<&Address> = existing.iter().filter(|a| a.comment == comment).collect();

        let want: std::collections::HashSet<&str> = desired.iter().map(|s| s.as_str()).collect();
        for a in &owned {
            if !want.contains(a.address.as_str()) {
                self.send(self.http.delete(self.url(&format!("{path}/{}", a.id)))).await?;
            }
        }
        let have: std::collections::HashSet<&str> = owned.iter().map(|a| a.address.as_str()).collect();
        for d in desired {
            if !have.contains(d.as_str()) {
                let body = address_body(d, interface, comment, v6);
                self.send(self.http.put(self.url(path)).json(&body)).await?;
            }
        }
        Ok(())
    }

    /// Delete the interface (and its peers + managed addresses) by name. No-op
    /// if absent.
    pub async fn remove_interface(&self, name: &str) -> Result<()> {
        // Drop the addresses we own, then the peers, then the interface.
        let _ = self.reconcile_addresses(name, &[]).await;
        for p in self.list_peers(name).await? {
            let _ = self.remove_peer(name, &p.public_key).await;
        }
        let existing = self.send(self.http.get(self.url(&format!("interface/wireguard?name={name}")))).await?;
        if let Some(id) = first_id(&existing) {
            self.send(self.http.delete(self.url(&format!("interface/wireguard/{id}")))).await?;
        }
        Ok(())
    }

    /// List the addresses on an interface for one family (`ip`/`ipv6`).
    pub async fn list_addresses(&self, v6: bool, interface: &str) -> Result<Vec<Address>> {
        let path = if v6 { "ipv6/address" } else { "ip/address" };
        let v = self.send(self.http.get(self.url(&format!("{path}?interface={interface}")))).await?;
        Ok(serde_json::from_value(v).unwrap_or_default())
    }

    pub async fn list_peers(&self, interface: &str) -> Result<Vec<Peer>> {
        let v = self.send(self.http.get(self.url(&format!("interface/wireguard/peers?interface={interface}")))).await?;
        Ok(serde_json::from_value(v).unwrap_or_default())
    }

    async fn find_peer_id(&self, interface: &str, public_key: &str) -> Result<Option<String>> {
        Ok(self
            .list_peers(interface)
            .await?
            .into_iter()
            .find(|p| p.public_key == public_key)
            .map(|p| p.id))
    }
}

/// Body for creating/updating a WireGuard interface.
pub fn interface_body(name: &str, listen_port: u16, private_key: &str) -> Value {
    json!({
        "name": name,
        "listen-port": listen_port.to_string(),
        "private-key": private_key,
        "disabled": "false",
    })
}

/// Body for creating/updating a peer.
pub fn peer_body(interface: &str, public_key: &str, allowed_address: &str, preshared_key: Option<&str>) -> Value {
    let mut m = json!({
        "interface": interface,
        "public-key": public_key,
        "allowed-address": allowed_address,
    });
    if let Some(psk) = preshared_key {
        m["preshared-key"] = Value::String(psk.to_string());
    }
    m
}

/// The comment wg-vpng stamps on the addresses it manages for an interface,
/// used to find/sync exactly its own set.
pub fn wg_comment(interface: &str) -> String {
    format!("wg-vpng:{interface}")
}

/// Body for creating an `/ip/address` or `/ipv6/address` entry. IPv6 addresses
/// are set non-advertising (a WG tunnel shouldn't send RAs).
pub fn address_body(address: &str, interface: &str, comment: &str, v6: bool) -> Value {
    let mut m = json!({
        "address": address,
        "interface": interface,
        "comment": comment,
    });
    if v6 {
        m["advertise"] = Value::String("no".to_string());
    }
    m
}

/// RouterOS list responses are arrays of objects each carrying `.id`.
fn first_id(v: &Value) -> Option<String> {
    v.as_array()?.first()?.get(".id")?.as_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_body_shape() {
        let b = interface_body("wg0", 51820, "PRIV");
        assert_eq!(b["name"], "wg0");
        assert_eq!(b["listen-port"], "51820");
        assert_eq!(b["private-key"], "PRIV");
    }

    #[test]
    fn peer_body_omits_psk_when_absent() {
        let b = peer_body("wg0", "PUB", "10.8.0.2/32", None);
        assert_eq!(b["public-key"], "PUB");
        assert_eq!(b["allowed-address"], "10.8.0.2/32");
        assert!(b.get("preshared-key").is_none());
    }

    #[test]
    fn peer_body_includes_psk_when_present() {
        let b = peer_body("wg0", "PUB", "10.8.0.2/32", Some("PSK"));
        assert_eq!(b["preshared-key"], "PSK");
    }

    #[test]
    fn first_id_reads_dot_id() {
        let v = json!([{ ".id": "*1", "name": "wg0" }]);
        assert_eq!(first_id(&v), Some("*1".to_string()));
        assert_eq!(first_id(&json!([])), None);
    }

    #[test]
    fn wg_comment_tags_interface() {
        assert_eq!(wg_comment("wg0"), "wg-vpng:wg0");
    }

    #[test]
    fn address_body_v4_and_v6() {
        let v4 = address_body("10.8.0.1/24", "wg0", "wg-vpng:wg0", false);
        assert_eq!(v4["address"], "10.8.0.1/24");
        assert_eq!(v4["interface"], "wg0");
        assert_eq!(v4["comment"], "wg-vpng:wg0");
        assert!(v4.get("advertise").is_none());

        let v6 = address_body("fd00:8::1/64", "wg0", "wg-vpng:wg0", true);
        assert_eq!(v6["address"], "fd00:8::1/64");
        assert_eq!(v6["advertise"], "no");
    }
}

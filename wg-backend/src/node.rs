//! Node backend: drive a remote `wg-vpng-node` agent over HTTP. The node is a
//! dumb switch — it owns no policy, it just applies whatever interface + peer
//! state we POST it, using its own local backend (self-managed / NetworkManager).
//!
//! This is the *client* half; the node binary serves the matching routes
//! (`POST /v1/apply`, `POST /v1/remove`, `GET /v1/status/{name}`), authenticated
//! with a bearer key.

use async_trait::async_trait;

use crate::{ApplyRequest, BackendError, InterfaceSpec, PeerSpec, PeerStatus, RemoveRequest, Result, WireguardBackend};

pub struct NodeBackend {
    base: String,
    key: String,
    http: reqwest::Client,
}

impl NodeBackend {
    pub fn new(url: &str, key: &str) -> Self {
        Self {
            base: url.trim_end_matches('/').to_string(),
            key: key.to_string(),
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

fn net(e: reqwest::Error) -> BackendError {
    BackendError::Node(e.to_string())
}

#[async_trait]
impl WireguardBackend for NodeBackend {
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()> {
        let body = ApplyRequest { interface: iface.clone(), peers: peers.to_vec() };
        let resp = self
            .http
            .post(self.url("/v1/apply"))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await
    }

    async fn remove(&self, iface_name: &str) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/v1/remove"))
            .bearer_auth(&self.key)
            .json(&RemoveRequest { name: iface_name.to_string() })
            .send()
            .await
            .map_err(net)?;
        check(resp).await
    }

    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>> {
        let resp = self
            .http
            .get(self.url(&format!("/v1/status/{iface_name}")))
            .bearer_auth(&self.key)
            .send()
            .await
            .map_err(net)?;
        if !resp.status().is_success() {
            // Status is best-effort; a node hiccup shouldn't error the caller.
            return Ok(vec![]);
        }
        resp.json::<Vec<PeerStatus>>().await.map_err(net)
    }
}

/// Turn a non-2xx node response into a `BackendError::Node` with its body.
async fn check(resp: reqwest::Response) -> Result<()> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    Err(BackendError::Node(format!("{status}: {}", body.trim())))
}

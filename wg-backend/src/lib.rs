//! Pluggable WireGuard backends + the wire types shared between the main server
//! and the remote `node` agent.
//!
//! A backend takes the *desired* interface + peer set and reconciles the real
//! world to match (`apply`). Full-state reconciliation (rather than incremental
//! add/remove) is the one shape that fits every target: `wg syncconf` for the
//! self-managed kernel interface, a rewritten NetworkManager keyfile, a diff
//! against RouterOS's peer list, and a single POST to a node.

use async_trait::async_trait;

mod mikrotik;
mod networkmanager;
mod node;
mod selfmanaged;
mod systemd_networkd;

pub use mikrotik::MikrotikBackend;
pub use networkmanager::{render_keyfile, NetworkManagerBackend};
pub use node::NodeBackend;
pub use selfmanaged::{parse_wg_dump, render_server_config, SelfManagedBackend};
pub use systemd_networkd::{render_netdev, render_network, SystemdNetworkdBackend};

/// Desired state of the server-side WireGuard interface. Serializable so it can
/// cross the wire to a node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InterfaceSpec {
    pub name: String,
    pub listen_port: u16,
    /// Server address with prefix, e.g. `10.8.0.1/24` (comma-separated for
    /// dual-stack).
    pub address: String,
    pub private_key: String,
}

/// Desired state of one peer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerSpec {
    pub public_key: String,
    /// Peer's tunnel address, e.g. `10.8.0.5/32`.
    pub address: String,
    pub preshared_key: Option<String>,
}

/// Runtime status of a peer as reported by the backend.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct PeerStatus {
    pub public_key: String,
    pub endpoint: Option<String>,
    /// Unix seconds of last handshake, if any.
    pub last_handshake: Option<i64>,
}

/// Body of a node `apply` request: the full desired state of one interface.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyRequest {
    pub interface: InterfaceSpec,
    pub peers: Vec<PeerSpec>,
}

/// Body of a node `remove` request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveRequest {
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend command failed: {0}")]
    Command(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("mikrotik: {0}")]
    Mikrotik(#[from] mikrotik_api::Error),
    #[error("node: {0}")]
    Node(String),
}

pub type Result<T> = std::result::Result<T, BackendError>;

#[async_trait]
pub trait WireguardBackend: Send + Sync {
    /// Reconcile the interface and its full peer set to the desired state.
    /// Must be idempotent.
    async fn apply(&self, iface: &InterfaceSpec, peers: &[PeerSpec]) -> Result<()>;

    /// Best-effort runtime status (handshakes/endpoints). Empty is acceptable
    /// for backends that don't expose it.
    async fn status(&self, iface_name: &str) -> Result<Vec<PeerStatus>>;

    /// Tear the interface down (on interface deletion). Best-effort.
    async fn remove(&self, iface_name: &str) -> Result<()>;
}

/// Build a *local* backend by kind name (`self-managed` / `network-manager`).
/// Used by the node agent, whose config only ever selects a local mechanism.
pub fn local_backend(kind: &str) -> std::result::Result<Box<dyn WireguardBackend>, String> {
    match kind {
        "self-managed" => Ok(Box::new(SelfManagedBackend::new())),
        "network-manager" => Ok(Box::new(NetworkManagerBackend::new())),
        "systemd-networkd" => Ok(Box::new(SystemdNetworkdBackend::new())),
        other => Err(format!("unknown local backend kind: {other}")),
    }
}

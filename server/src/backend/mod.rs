//! Pluggable WireGuard backends.
//!
//! A backend takes the *desired* interface + peer set and reconciles the real
//! world to match (`apply`). Full-state reconciliation (rather than
//! incremental add/remove) is the one shape that fits all three targets: `wg
//! syncconf` for the self-managed kernel interface, a rewritten NetworkManager
//! keyfile, and a diff against RouterOS's peer list.

use async_trait::async_trait;

mod mikrotik;
mod networkmanager;
mod selfmanaged;

pub use mikrotik::MikrotikBackend;
pub use networkmanager::NetworkManagerBackend;
pub use selfmanaged::SelfManagedBackend;

/// Desired state of the server-side WireGuard interface.
#[derive(Debug, Clone)]
pub struct InterfaceSpec {
    pub name: String,
    pub listen_port: u16,
    /// Server address with prefix, e.g. `10.8.0.1/24`.
    pub address: String,
    pub private_key: String,
    pub public_key: String,
}

/// Desired state of one peer.
#[derive(Debug, Clone)]
pub struct PeerSpec {
    pub public_key: String,
    /// Peer's tunnel address, e.g. `10.8.0.5/32`.
    pub address: String,
    pub preshared_key: Option<String>,
}

/// Runtime status of a peer as reported by the backend.
#[derive(Debug, Clone, Default)]
pub struct PeerStatus {
    pub public_key: String,
    pub endpoint: Option<String>,
    /// Unix seconds of last handshake, if any.
    pub last_handshake: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend command failed: {0}")]
    Command(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("mikrotik: {0}")]
    Mikrotik(#[from] mikrotik_api::Error),
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
}

/// Backend selection, mirrored from `[wireguard] backend = "..."` in config.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendConfig {
    /// Manage a kernel WireGuard interface locally with `wg` + `ip`.
    SelfManaged,
    /// Manage the interface through NetworkManager (`nmcli` + keyfile).
    NetworkManager,
    /// Manage a MikroTik RouterOS device over its REST API.
    Mikrotik {
        url: String,
        username: String,
        password: String,
        #[serde(default)]
        insecure: bool,
    },
}

impl BackendConfig {
    pub fn build(&self) -> Result<Box<dyn WireguardBackend>> {
        Ok(match self {
            BackendConfig::SelfManaged => Box::new(SelfManagedBackend::new()),
            BackendConfig::NetworkManager => Box::new(NetworkManagerBackend::new()),
            BackendConfig::Mikrotik { url, username, password, insecure } => {
                Box::new(MikrotikBackend::new(url, username, password, *insecure)?)
            }
        })
    }
}

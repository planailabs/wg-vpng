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
#[derive(Debug, Clone, Default, serde::Serialize, schemars::JsonSchema)]
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

    /// Tear the interface down (on interface deletion). Best-effort.
    async fn remove(&self, iface_name: &str) -> Result<()>;
}

/// The backend that applies a single interface. Configured per interface and
/// stored (serialized) on the interface row. Secrets (the MikroTik password)
/// are encrypted at rest via [`BackendConfig::encrypt_secrets`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
        /// Encrypted at rest (see `encrypt_secrets`); plaintext only in memory
        /// after `decrypt_secrets`.
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

    /// Short kind tag for display / API (`self-managed`, `network-manager`,
    /// `mikrotik`).
    pub fn kind(&self) -> &'static str {
        match self {
            BackendConfig::SelfManaged => "self-managed",
            BackendConfig::NetworkManager => "network-manager",
            BackendConfig::Mikrotik { .. } => "mikrotik",
        }
    }

    /// Build from UI/API parts. `mikrotik_*` are required only for the mikrotik
    /// kind.
    pub fn from_parts(
        kind: &str,
        mikrotik_url: Option<String>,
        mikrotik_username: Option<String>,
        mikrotik_password: Option<String>,
        mikrotik_insecure: bool,
    ) -> std::result::Result<Self, String> {
        match kind {
            "self-managed" => Ok(BackendConfig::SelfManaged),
            "network-manager" => Ok(BackendConfig::NetworkManager),
            "mikrotik" => Ok(BackendConfig::Mikrotik {
                url: mikrotik_url.filter(|s| !s.is_empty()).ok_or("mikrotik url required")?,
                username: mikrotik_username.unwrap_or_default(),
                password: mikrotik_password.unwrap_or_default(),
                insecure: mikrotik_insecure,
            }),
            other => Err(format!("unknown backend kind: {other}")),
        }
    }

    /// Encrypt secrets in place before storing.
    pub fn encrypt_secrets(&mut self) {
        if let BackendConfig::Mikrotik { password, .. } = self {
            if !password.is_empty() {
                *password = crate::crypto::encrypt(password);
            }
        }
    }

    /// Decrypt secrets in place after loading (before `build`).
    pub fn decrypt_secrets(&mut self) -> anyhow::Result<()> {
        if let BackendConfig::Mikrotik { password, .. } = self {
            if !password.is_empty() {
                *password = crate::crypto::decrypt(password)?;
            }
        }
        Ok(())
    }

    /// MikroTik connection params for display (url, username, insecure) — never
    /// the password.
    pub fn mikrotik_display(&self) -> Option<(String, String, bool)> {
        match self {
            BackendConfig::Mikrotik { url, username, insecure, .. } => {
                Some((url.clone(), username.clone(), *insecure))
            }
            _ => None,
        }
    }
}

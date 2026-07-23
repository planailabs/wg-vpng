//! Per-interface backend selection.
//!
//! The reconcile backends themselves (self-managed, NetworkManager, MikroTik,
//! and the remote `node`) live in the shared `wg-backend` crate so the node
//! agent can reuse them. This module keeps only [`BackendConfig`] — the
//! serialized, per-interface choice stored on the interface row, with its
//! secrets encrypted at rest.

pub use wg_backend::{
    BackendError, InterfaceSpec, NodeBackend, PeerSpec, PeerStatus, Result, WireguardBackend,
};

/// The backend that applies a single interface. Configured per interface and
/// stored (serialized) on the interface row. Secrets (the MikroTik password,
/// the node key) are encrypted at rest via [`BackendConfig::encrypt_secrets`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BackendConfig {
    /// Manage a kernel WireGuard interface locally with `wg` + `ip`.
    SelfManaged,
    /// Manage the interface through NetworkManager (`nmcli` + keyfile).
    NetworkManager,
    /// Manage the interface through systemd-networkd (`.netdev`/`.network`).
    SystemdNetworkd,
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
    /// Drive a remote `wg-vpng-node` agent over HTTP.
    Node {
        url: String,
        /// API key; encrypted at rest (see `encrypt_secrets`).
        key: String,
    },
}

impl BackendConfig {
    pub fn build(&self) -> Result<Box<dyn WireguardBackend>> {
        Ok(match self {
            BackendConfig::SelfManaged => Box::new(wg_backend::SelfManagedBackend::new()),
            BackendConfig::NetworkManager => Box::new(wg_backend::NetworkManagerBackend::new()),
            BackendConfig::SystemdNetworkd => Box::new(wg_backend::SystemdNetworkdBackend::new()),
            BackendConfig::Mikrotik { url, username, password, insecure } => {
                Box::new(wg_backend::MikrotikBackend::new(url, username, password, *insecure)?)
            }
            BackendConfig::Node { url, key } => Box::new(NodeBackend::new(url, key)),
        })
    }

    /// Short kind tag for display / API (`self-managed`, `network-manager`,
    /// `mikrotik`, `node`).
    pub fn kind(&self) -> &'static str {
        match self {
            BackendConfig::SelfManaged => "self-managed",
            BackendConfig::NetworkManager => "network-manager",
            BackendConfig::SystemdNetworkd => "systemd-networkd",
            BackendConfig::Mikrotik { .. } => "mikrotik",
            BackendConfig::Node { .. } => "node",
        }
    }

    /// Build from UI/API parts. `mikrotik_*` are required only for the mikrotik
    /// kind; `node_*` only for the node kind.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        kind: &str,
        mikrotik_url: Option<String>,
        mikrotik_username: Option<String>,
        mikrotik_password: Option<String>,
        mikrotik_insecure: bool,
        node_url: Option<String>,
        node_key: Option<String>,
    ) -> std::result::Result<Self, String> {
        match kind {
            "self-managed" => Ok(BackendConfig::SelfManaged),
            "network-manager" => Ok(BackendConfig::NetworkManager),
            "systemd-networkd" => Ok(BackendConfig::SystemdNetworkd),
            "mikrotik" => Ok(BackendConfig::Mikrotik {
                url: mikrotik_url.filter(|s| !s.is_empty()).ok_or("mikrotik url required")?,
                username: mikrotik_username.unwrap_or_default(),
                password: mikrotik_password.unwrap_or_default(),
                insecure: mikrotik_insecure,
            }),
            "node" => Ok(BackendConfig::Node {
                url: node_url.filter(|s| !s.is_empty()).ok_or("node url required")?,
                key: node_key.unwrap_or_default(),
            }),
            other => Err(format!("unknown backend kind: {other}")),
        }
    }

    /// Encrypt secrets in place before storing.
    pub fn encrypt_secrets(&mut self) {
        match self {
            BackendConfig::Mikrotik { password, .. } if !password.is_empty() => {
                *password = crate::crypto::encrypt(password);
            }
            BackendConfig::Node { key, .. } if !key.is_empty() => {
                *key = crate::crypto::encrypt(key);
            }
            _ => {}
        }
    }

    /// Decrypt secrets in place after loading (before `build`).
    pub fn decrypt_secrets(&mut self) -> anyhow::Result<()> {
        match self {
            BackendConfig::Mikrotik { password, .. } if !password.is_empty() => {
                *password = crate::crypto::decrypt(password)?;
            }
            BackendConfig::Node { key, .. } if !key.is_empty() => {
                *key = crate::crypto::decrypt(key)?;
            }
            _ => {}
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

    /// Node URL for display — never the key.
    pub fn node_display(&self) -> Option<String> {
        match self {
            BackendConfig::Node { url, .. } => Some(url.clone()),
            _ => None,
        }
    }
}

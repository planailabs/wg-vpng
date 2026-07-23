//! Plain data-transfer types crossing the client/server boundary. These must
//! compile on both wasm and native, so they reference no server-only crates.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentUser {
    pub email: String,
    pub name: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerView {
    pub id: Uuid,
    pub interface_id: Uuid,
    pub interface_name: String,
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub owner_email: Option<String>,
    /// False for an "unconfigured" device (no key yet — the user must generate).
    pub configured: bool,
    /// Whether the end user created this device. Users may rename only their own
    /// user-created devices; admins may rename any.
    pub user_created: bool,
}

/// A freshly created/regenerated device plus its full config (with the private
/// key) — shown once; the private key is never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewDeviceView {
    pub peer: PeerView,
    pub config: String,
    /// Scannable QR (SVG) of the config for the WireGuard mobile app.
    pub qr_svg: String,
}

/// An interface a user can access, with their usage on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceAccessView {
    pub id: Uuid,
    pub name: String,
    /// Human-friendly label; empty falls back to `name`.
    pub display_name: String,
    pub endpoint: String,
    pub used: i64,
    /// None = unlimited.
    pub limit: Option<i32>,
}

/// Full interface view for the admin console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceAdminView {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub listen_port: i32,
    pub address: String,
    pub public_key: String,
    pub endpoint: String,
    pub dns: Option<String>,
    pub allowed_ips: String,
    pub keepalive: i32,
    pub device_limit: Option<i32>,
    /// Groups whose members may use this interface.
    pub group_ids: Vec<Uuid>,
    pub backend_kind: String,
    pub mikrotik_url: Option<String>,
    pub mikrotik_username: Option<String>,
    pub mikrotik_insecure: bool,
    pub node_url: Option<String>,
    /// Populated when the last backend sync/status failed (surfaced in the UI).
    pub backend_error: Option<String>,
}

/// A group (reusable access rule set) for the admin console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupView {
    pub id: Uuid,
    pub name: String,
    /// Email globs / literal emails; `*` = everyone.
    pub patterns: Vec<String>,
    /// Values matched against a user's OIDC group claim.
    pub claim_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserAdminView {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub banned: bool,
    pub access_revoked: bool,
    pub device_count: i64,
    /// Liveliness lapsed (login TTL passed) — access suspended until re-login.
    pub deactivated: bool,
}

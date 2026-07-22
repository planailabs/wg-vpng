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
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub owner_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserAdminView {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub banned: bool,
    pub access_revoked: bool,
    pub device_limit: Option<i32>,
    pub device_count: i64,
    /// Effective limit (override or global default), for display.
    pub effective_limit: i32,
}

/// Current user's device usage against their limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceQuota {
    pub used: i64,
    pub limit: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceView {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub endpoint: String,
    pub public_key: String,
    pub listen_port: i32,
    pub dns: Option<String>,
    pub allowed_ips: String,
}

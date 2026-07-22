//! Web UI: Dioxus fullstack app, its server functions, and (server-only) the
//! OIDC user resolver.

pub mod app;
pub mod components;
pub mod dto;
pub mod server_fns;

#[cfg(feature = "server")]
pub mod auth;
#[cfg(feature = "server")]
pub mod user;

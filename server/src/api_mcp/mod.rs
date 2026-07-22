//! plan-ai-api-mcp registry: declares the `interfaces` and `peers` resources
//! once and serves them as REST (`/api/v1/*`), MCP (`/mcp`), and OpenAPI.

pub mod auth;
pub mod endpoints;

use std::sync::{Arc, OnceLock};

use plan_ai_api_mcp::{OnItem, Registry, Risk};
use sqlx::PgPool;

static REGISTRY: OnceLock<Arc<Registry<PgPool>>> = OnceLock::new();

/// Build (once) and share the registry.
pub fn shared_registry(pool: PgPool) -> Arc<Registry<PgPool>> {
    REGISTRY
        .get_or_init(|| Arc::new(build_registry(pool)))
        .clone()
}

fn build_registry(pool: PgPool) -> Registry<PgPool> {
    use endpoints::*;

    let authn = Arc::new(auth::TokenAuthenticator::new(pool));
    let mut reg = Registry::new(authn)
        .info("wg-vpng API", env!("CARGO_PKG_VERSION"))
        .instructions(
            "WireGuard VPN generator API. Authenticate with a Bearer token \
             (kind='admin' for mutations). Tools are <entity>_<action>, e.g. \
             peer_create, peer_regenerate, peer_config.",
        );

    {
        let mut r = reg.resource("interfaces", "interface", "Interfaces");
        r.list("List managed WireGuard interfaces.", |pool, p, i: InterfaceListInput| async move {
            interface_list(pool, p, i).await
        });
        r.get("Get an interface (default interface when id omitted).", |pool, p, i: InterfaceGetInput| async move {
            interface_get(pool, p, i).await
        });
    }

    {
        let mut r = reg.resource("peers", "peer", "Peers");
        r.list("List peers on an interface.", |pool, p, i: PeerListInput| async move {
            peer_list(pool, p, i).await
        });
        r.get("Get a peer.", |pool, p, i: PeerGetInput| async move {
            peer_get(pool, p, i).await
        });
        r.create(
            "Create a peer (generates a keypair, allocates an address, syncs the backend).",
            |pool, p, i: PeerCreateInput| async move { peer_create(pool, p, i).await },
        );
        r.delete("Delete a peer and sync the backend.", |pool, p, i: PeerDeleteInput| async move {
            peer_delete(pool, p, i).await
        });
        r.custom(
            "regenerate",
            Risk::Mutating,
            OnItem::Yes,
            "Replace a peer's private key (and preshared key), then sync the backend.",
            |pool, p, i: PeerRegenerateInput| async move { peer_regenerate(pool, p, i).await },
        );
        r.custom(
            "config",
            Risk::ReadOnly,
            OnItem::Yes,
            "Render a peer's WireGuard client configuration.",
            |pool, p, i: PeerConfigInput| async move { peer_config(pool, p, i).await },
        );
    }

    reg
}

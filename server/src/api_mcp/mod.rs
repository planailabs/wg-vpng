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
        r.update(
            "Update an interface's client-facing settings (endpoint, DNS, routed networks, keepalive).",
            |pool, p, i: InterfaceUpdateInput| async move { interface_update(pool, p, i).await },
        );
        r.custom(
            "status",
            Risk::ReadOnly,
            OnItem::Yes,
            "Live peer status (handshakes / endpoints) as reported by the backend.",
            |pool, p, i: InterfaceGetInput| async move { interface_status(pool, p, i).await },
        );
    }

    {
        let mut r = reg.resource("peers", "peer", "Peers");
        r.list("List peers on an interface.", |pool, p, i: PeerListInput| async move {
            peer_list(pool, p, i).await
        });
        r.get("Get a peer.", |pool, p, i: PeerGetInput| async move {
            peer_get(pool, p, i).await
        });
        r.update("Rename a device (peer).", |pool, p, i: PeerUpdateInput| async move {
            peer_update(pool, p, i).await
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

    {
        let mut r = reg.resource("users", "user", "Users");
        r.list("List users with device counts (admin).", |pool, p, i: UserListInput| async move {
            user_list(pool, p, i).await
        });
        r.get("Get a user with device count (admin).", |pool, p, i: UserIdInput| async move {
            user_get(pool, p, i).await
        });
        r.create("Create (or upsert) a user by email (admin).", |pool, p, i: UserCreateInput| async move {
            user_create(pool, p, i).await
        });
        r.custom(
            "ban",
            Risk::Destructive,
            OnItem::Yes,
            "Ban or unban a user (banned users cannot log in and their devices are dropped).",
            |pool, p, i: UserBanInput| async move { user_ban(pool, p, i).await },
        );
        r.custom(
            "revoke",
            Risk::Mutating,
            OnItem::Yes,
            "Revoke or restore a user's VPN access (devices retained, dropped from the backend).",
            |pool, p, i: UserRevokeInput| async move { user_revoke(pool, p, i).await },
        );
        r.custom(
            "set_limit",
            Risk::Mutating,
            OnItem::Yes,
            "Set a user's device limit (null = global default).",
            |pool, p, i: UserLimitInput| async move { user_set_limit(pool, p, i).await },
        );
        r.delete("Delete a user and all their devices (admin).", |pool, p, i: UserIdInput| async move {
            user_delete(pool, p, i).await
        });
    }

    reg
}

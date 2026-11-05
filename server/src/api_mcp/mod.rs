//! plan-ai-api-mcp registry: declares the `interfaces` and `peers` resources
//! once and serves them as REST (`/api/v1/*`), MCP (`/mcp`), and OpenAPI.

pub mod auth;
pub mod endpoints;

use std::sync::{Arc, OnceLock};

use plan_ai_api_mcp::{Authenticator, OnItem, Principal, Registry, Risk};
use sqlx::PgPool;

static REGISTRY: OnceLock<Arc<Registry<PgPool>>> = OnceLock::new();

/// Build (once) and share the registry.
pub fn shared_registry(pool: PgPool) -> Arc<Registry<PgPool>> {
    REGISTRY
        .get_or_init(|| Arc::new(build_registry(pool)))
        .clone()
}

fn build_registry(pool: PgPool) -> Registry<PgPool> {
    let authn = Arc::new(auth::BearerAuthenticator::new(pool, crate::config::config().zitadel_bearer.as_ref()));
    registry_with(authn)
}

/// Admins, and signed-in users acting on their own devices; not the
/// non-admin `tokens` rows (API tokens), which the peer mutations refuse.
fn admin_or_user(p: &Principal) -> bool {
    p.admin || auth::acting_user(p).is_some()
}

fn registry_with(authn: Arc<dyn Authenticator>) -> Registry<PgPool> {
    use endpoints::*;

    let mut reg = Registry::new(authn)
        .info("wg-vpng API", env!("CARGO_PKG_VERSION"))
        .instructions(
            "WireGuard VPN generator API. Authenticate with a Bearer token \
             (kind='admin' for everything) or a signed-in user's Zitadel token \
             (their own devices on the interfaces they may use). Tools are \
             <entity>_<action>, e.g. peer_create, peer_regenerate, peer_config.",
        );

    {
        let mut r = reg.resource("interfaces", "interface", "Interfaces");
        r.list("List managed WireGuard interfaces.", |pool, p, i: InterfaceListInput| async move {
            interface_list(pool, p, i).await
        });
        r.get("Get an interface (the sole interface when id omitted).", |pool, p, i: InterfaceGetInput| async move {
            interface_get(pool, p, i).await
        });
        r.create(
            "Create an interface (name, address, endpoint, per-interface backend, device limit, access patterns).",
            |pool, p, i: InterfaceCreateInput| async move { interface_create(pool, p, i).await },
        )
        .admin_only();
        r.update(
            "Update an interface's endpoint/DNS/routed-nets/keepalive, device limit, access patterns, or backend.",
            |pool, p, i: InterfaceUpdateInput| async move { interface_update(pool, p, i).await },
        )
        .admin_only();
        r.delete(
            "Delete an interface (tears it down on its backend; devices cascade).",
            |pool, p, i: InterfaceGetInput| async move { interface_delete(pool, p, i).await },
        )
        .admin_only();
        r.custom(
            "status",
            Risk::ReadOnly,
            OnItem::Yes,
            "Live peer status (handshakes / endpoints) as reported by the backend.",
            |pool, p, i: InterfaceGetInput| async move { interface_status(pool, p, i).await },
        );
        r.custom(
            "resync",
            Risk::Mutating,
            OnItem::Yes,
            "Re-sync the interface to its backend now (re-assert desired state).",
            |pool, p, i: InterfaceGetInput| async move { interface_resync(pool, p, i).await },
        )
        .admin_only();
    }

    {
        let mut r = reg.resource("groups", "group", "Groups");
        r.list("List access groups (admin).", |pool, p, i: GroupListInput| async move {
            group_list(pool, p, i).await
        })
        .admin_only();
        r.get("Get a group (admin).", |pool, p, i: GroupIdInput| async move {
            group_get(pool, p, i).await
        })
        .admin_only();
        r.create(
            "Create a group (name, email patterns, OIDC claim values).",
            |pool, p, i: GroupCreateInput| async move { group_create(pool, p, i).await },
        )
        .admin_only();
        r.update(
            "Update a group's name / patterns / claim values (re-syncs interfaces using it).",
            |pool, p, i: GroupUpdateInput| async move { group_update(pool, p, i).await },
        )
        .admin_only();
        r.delete("Delete a group.", |pool, p, i: GroupIdInput| async move {
            group_delete(pool, p, i).await
        })
        .admin_only();
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
        })
        .visible_if(admin_or_user);
        r.create(
            "Create a peer (generates a keypair, allocates an address, syncs the backend).",
            |pool, p, i: PeerCreateInput| async move { peer_create(pool, p, i).await },
        )
        .visible_if(admin_or_user);
        r.delete("Delete a peer and sync the backend.", |pool, p, i: PeerDeleteInput| async move {
            peer_delete(pool, p, i).await
        })
        .visible_if(admin_or_user);
        r.custom(
            "regenerate",
            Risk::Mutating,
            OnItem::Yes,
            "Replace a peer's private key (and preshared key), then sync the backend.",
            |pool, p, i: PeerRegenerateInput| async move { peer_regenerate(pool, p, i).await },
        )
        .visible_if(admin_or_user);
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
        })
        .admin_only();
        r.get("Get a user with device count (admin).", |pool, p, i: UserIdInput| async move {
            user_get(pool, p, i).await
        })
        .admin_only();
        r.create("Create (or upsert) a user by email (admin).", |pool, p, i: UserCreateInput| async move {
            user_create(pool, p, i).await
        })
        .admin_only();
        r.custom(
            "ban",
            Risk::Destructive,
            OnItem::Yes,
            "Ban or unban a user (banned users cannot log in and their devices are dropped).",
            |pool, p, i: UserBanInput| async move { user_ban(pool, p, i).await },
        )
        .admin_only();
        r.custom(
            "revoke",
            Risk::Mutating,
            OnItem::Yes,
            "Revoke or restore a user's VPN access (devices retained, dropped from the backend).",
            |pool, p, i: UserRevokeInput| async move { user_revoke(pool, p, i).await },
        )
        .admin_only();
        r.delete("Delete a user and all their devices (admin).", |pool, p, i: UserIdInput| async move {
            user_delete(pool, p, i).await
        })
        .admin_only();
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(is_admin: bool) -> Principal {
        auth::user_principal(
            &crate::store::User {
                id: uuid::Uuid::from_u128(7),
                email: "a@plan.ai".into(),
                name: "A".into(),
                is_admin,
                banned: false,
                access_revoked: false,
            },
            None,
        )
    }

    #[tokio::test]
    async fn admin_tools_are_hidden_from_users_and_peer_mutations_from_api_tokens() {
        let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap();
        let reg = registry_with(Arc::new(auth::TokenAuthenticator::new(pool)));
        let tool = |name: &str| reg.endpoints().iter().find(|e| e.tool_name() == name).unwrap().clone();
        let api_token = Principal::scoped("token:api", Default::default(), Default::default());

        let ban = tool("user_ban");
        assert!(!ban.visible_to(&user(false)) && !ban.visible_to(&api_token));
        assert!(ban.visible_to(&user(true)) && ban.visible_to(&Principal::admin("token:admin")));

        let create = tool("peer_create");
        assert!(create.visible_to(&user(false)) && create.visible_to(&user(true)));
        assert!(!create.visible_to(&api_token));

        // Reads stay visible to everyone; the handler narrows what they see.
        assert!(tool("peer_list").visible_to(&api_token) && tool("peer_config").visible_to(&user(false)));
    }
}

//! wg-vpng server: Dioxus fullstack UI + plan-ai-api-mcp REST/MCP surface,
//! OIDC auth (plan-ai-auth), plan-ai-html error/login pages, and a pluggable
//! WireGuard backend (self-managed / NetworkManager / MikroTik).

#[cfg(feature = "server")]
mod api_mcp;
#[cfg(feature = "server")]
mod backend;
#[cfg(feature = "server")]
mod config;
#[cfg(feature = "server")]
mod db;
#[cfg(feature = "server")]
mod server_state;
#[cfg(feature = "server")]
mod store;
#[cfg(feature = "server")]
mod wg;

#[cfg(feature = "webui")]
mod web;

#[cfg(feature = "server")]
async fn init_server() {
    let cfg = config::load();
    let pool = db::connect(&cfg.database.url).await;

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("failed to run migrations");

    server_state::set_pool(pool.clone());

    // Select + install the WireGuard backend.
    let backend = cfg
        .wireguard
        .backend
        .build()
        .expect("failed to build wireguard backend");
    server_state::set_backend(backend);

    // Seed the interface (keypair generated on first boot) and push current
    // state to the backend so restarts converge the real world to the DB.
    let iface = store::ensure_default_interface(&pool, &cfg.wireguard)
        .await
        .expect("failed to ensure default interface");
    if let Err(e) = store::sync_interface(&pool, server_state::backend(), iface.id).await {
        tracing::warn!("initial backend sync failed (continuing): {e:#}");
    }

    // Install the OIDC user resolver.
    web::auth::install_resolver(pool.clone());
}

/// plan-ai-html 404 for browser requests the app + API routers don't match.
#[cfg(all(feature = "server", feature = "webui"))]
fn not_found(lang: plan_ai_html::Lang) -> axum::response::Response {
    use axum::response::IntoResponse;
    let html = plan_ai_html::error_page(lang, "not-found-title", "not-found-body");
    (
        axum::http::StatusCode::NOT_FOUND,
        [("content-type", "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

fn main() {
    #[cfg(feature = "server")]
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls ring crypto provider");

    #[cfg(all(feature = "server", feature = "webui"))]
    {
        use tracing_subscriber::EnvFilter;
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .init();
    }

    #[cfg(all(feature = "server", feature = "webui"))]
    {
        use dioxus::server::{DioxusRouterExt, ServeConfig, axum};
        use std::sync::OnceLock;

        static INIT: OnceLock<Option<Vec<plan_ai_auth::AuthLayer>>> = OnceLock::new();

        // Dioxus reads PORT for its listen address.
        if std::env::var("PORT").is_err() {
            let cfg = config::load();
            unsafe { std::env::set_var("PORT", cfg.web.port.to_string()) };
        }

        dioxus::serve(move || async move {
            let dev_no_auth =
                cfg!(debug_assertions) && std::env::var("DEV_ONLY_NO_AUTH").as_deref() == Ok("1");

            let auth_layers = if let Some(layers) = INIT.get() {
                layers.clone()
            } else {
                init_server().await;
                let cfg = config::load();
                let layers = if dev_no_auth {
                    tracing::warn!("DEV_ONLY_NO_AUTH=1 — OIDC disabled, all users are admin dev@localhost");
                    None
                } else if let Some(auth) = &cfg.auth {
                    let (layers, _cache) =
                        plan_ai_auth::build_auth_layers(auth, &cfg.database.url).await;
                    Some(layers)
                } else {
                    tracing::warn!("[auth] not configured — web authentication disabled");
                    None
                };
                let _ = INIT.set(layers.clone());
                layers
            };

            // The Dioxus fullstack UI, OIDC-gated when auth is configured.
            let mut web_router =
                axum::Router::new().serve_dioxus_application(ServeConfig::new(), web::app::App);
            if let Some(auth_layers) = auth_layers {
                web_router = web_router
                    .route("/auth/login", axum::routing::get(plan_ai_auth::login_page))
                    .route("/auth/logout", axum::routing::get(plan_ai_auth::logout_handler))
                    .layer(axum::middleware::from_fn(plan_ai_auth::require_auth));
                for layer in auth_layers {
                    web_router = web_router.layer(layer);
                }
            }

            // The plan-ai-api-mcp surface (Bearer-token auth, not OIDC):
            // REST at /api/v1/*, MCP at /mcp.
            let pool = server_state::pool_raw().expect("pool initialized in init_server");
            let registry = api_mcp::shared_registry(pool.clone());
            let api_http = registry.http_router(pool.clone());
            let mcp_service = registry.mcp_service(pool);

            // Merge the API into the Dioxus router so its SSR fallback stays
            // intact (deep-linked client routes must render). A dispatch
            // wrapper renders a plan-ai-html 404 for browser requests the app
            // genuinely doesn't match.
            let app_router = web_router
                .merge(api_http)
                .route_service("/mcp", mcp_service.clone())
                .route_service("/mcp/", mcp_service);

            use tower::ServiceExt;
            let dispatch = tower::service_fn(move |req: axum::extract::Request| {
                let app = app_router.clone();
                async move {
                    let wants_html = req
                        .headers()
                        .get(axum::http::header::ACCEPT)
                        .and_then(|v| v.to_str().ok())
                        .is_some_and(|a| a.contains("text/html"));
                    let lang = plan_ai_html::Lang::from_accept_language(
                        req.headers()
                            .get("accept-language")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or(""),
                    );
                    let resp = app.oneshot(req).await.expect("router is infallible");
                    if wants_html && resp.status() == axum::http::StatusCode::NOT_FOUND {
                        return Ok::<_, std::convert::Infallible>(not_found(lang));
                    }
                    Ok(resp)
                }
            });

            Ok(axum::Router::new().fallback_service(dispatch))
        });
    }

    #[cfg(all(not(feature = "server"), feature = "webui"))]
    {
        dioxus::launch(web::app::App);
    }
}

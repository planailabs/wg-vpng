//! wg-vpng-node library: the control-API router + state. Kept separate from the
//! binary so integration tests can serve the real router in-process.
//!
//! Routes (all bearer-authenticated with the configured `api_key`):
//!   POST /v1/apply          { interface, peers }  -> 204
//!   POST /v1/remove         { name }              -> 204
//!   GET  /v1/status/{name}                        -> [PeerStatus]
//!   GET  /healthz                                 -> 200 (no auth)

pub mod config;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use wg_backend::{ApplyRequest, PeerStatus, RemoveRequest, WireguardBackend};

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn WireguardBackend>,
    pub api_key: Arc<str>,
}

/// Build the control-API router.
pub fn app(backend: Arc<dyn WireguardBackend>, api_key: Arc<str>) -> Router {
    let state = AppState { backend, api_key };
    Router::new()
        .route("/v1/apply", post(apply))
        .route("/v1/remove", post(remove))
        .route("/v1/status/{name}", get(status))
        .route("/healthz", get(|| async { StatusCode::OK }))
        .with_state(state)
}

/// Reject requests without a matching `Authorization: Bearer <api_key>`.
fn authorized(state: &AppState, headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|tok| tok == &*state.api_key)
        .unwrap_or(false)
}

async fn apply(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ApplyRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.backend.apply(&req.interface, &req.peers).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn remove(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RemoveRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.backend.remove(&req.name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn status(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.backend.status(&name).await {
        Ok(peers) => Json(peers).into_response(),
        // Status is best-effort; report an empty set rather than erroring.
        Err(_) => Json(Vec::<PeerStatus>::new()).into_response(),
    }
}

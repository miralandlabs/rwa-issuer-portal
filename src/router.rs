//! Shared request dispatch used by both the Vercel binary (`portal`) and the
//! local dev server (`dev-server`). One route table, two transports.

use std::sync::Arc;

use http::HeaderMap;
use vercel_runtime::{Body, Response, StatusCode};

use crate::{api, state::AppState};

pub fn cors_preflight() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, POST, PATCH, OPTIONS")
        .header(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, Date, X-Date",
        )
        .header("Access-Control-Max-Age", "86400")
        .body(Body::Empty)
        .unwrap()
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Text("Not found".to_string()))
        .unwrap()
}

/// Dispatch one request to the matching handler. `method` should already have
/// HEAD folded into GET by the caller if desired.
pub async fn dispatch(
    headers: &HeaderMap,
    method: &http::Method,
    path: &str,
    query: &str,
    body: &Body,
    state: Arc<AppState>,
) -> Response<Body> {
    let m = if method == http::Method::HEAD {
        &http::Method::GET
    } else {
        method
    };
    match (m, path) {
        (&http::Method::OPTIONS, _) => cors_preflight(),

        (&http::Method::GET, "/") | (&http::Method::GET, "/health") => {
            api::handle_health(state).await
        }

        // Public
        (&http::Method::POST, "/api/v1/issuers") => api::register_issuer(body, state).await,
        (&http::Method::GET, "/api/v1/issuers") => api::get_issuer(query, state).await,
        (&http::Method::POST, "/api/v1/kyc") => api::submit_kyc(body, state).await,

        // Operator (bearer-gated)
        (&http::Method::PATCH, "/api/v1/issuers") => {
            api::set_issuer_status(headers, body, state).await
        }
        (&http::Method::POST, "/api/v1/kyc/review") => api::review_kyc(headers, body, state).await,
        (&http::Method::GET, "/api/v1/sync/feed") => api::sync_feed(headers, query, state).await,
        (&http::Method::POST, "/api/v1/sync/mark") => api::mark_synced(headers, body, state).await,

        _ => not_found(),
    }
}

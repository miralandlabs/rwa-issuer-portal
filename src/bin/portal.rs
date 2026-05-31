//! `rwa-issuer-portal` serverless entrypoint.
//!
//! Records-only off-chain system of record for the rwa-kyc-hook tenant model.
//! Public routes: issuer self-registration, issuer read, investor KYC submit.
//! Operator routes (bearer-gated): KYC review, issuer status, on-chain sync
//! feed, mark-synced. The portal NEVER signs or writes on-chain — a separate
//! ops sync consumes the feed and drives the kyc-hook CLI.

use {
    rwa_issuer_portal::{
        api, config::Config, route_handler::run_server, state::AppState,
    },
    std::{future::Future, pin::Pin, sync::Arc},
    vercel_runtime::{Body, Response, StatusCode},
};

fn cors_preflight() -> Response<Body> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rwa_issuer_portal=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let state = match AppState::cold_start(config).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!(error = %e, "cold-start failed; aborting");
            return Err(format!("cold-start failed: {e}").into());
        }
    };

    let routes = |headers: http::HeaderMap,
                  method: http::Method,
                  path: String,
                  query: String,
                  body: Body,
                  state: Arc<AppState>|
     -> Pin<Box<dyn Future<Output = Response<Body>> + Send>> {
        Box::pin(async move {
            let m = if method == http::Method::HEAD {
                http::Method::GET
            } else {
                method
            };
            match (&m, path.as_str()) {
                (&http::Method::OPTIONS, _) => cors_preflight(),

                (&http::Method::GET, "/") | (&http::Method::GET, "/health") => {
                    api::handle_health(state).await
                }

                // Public
                (&http::Method::POST, "/api/v1/issuers") => {
                    api::register_issuer(&body, state).await
                }
                (&http::Method::GET, "/api/v1/issuers") => {
                    api::get_issuer(&query, state).await
                }
                (&http::Method::POST, "/api/v1/kyc") => api::submit_kyc(&body, state).await,

                // Operator (bearer-gated)
                (&http::Method::PATCH, "/api/v1/issuers") => {
                    api::set_issuer_status(&headers, &body, state).await
                }
                (&http::Method::POST, "/api/v1/kyc/review") => {
                    api::review_kyc(&headers, &body, state).await
                }
                (&http::Method::GET, "/api/v1/sync/feed") => {
                    api::sync_feed(&headers, &query, state).await
                }
                (&http::Method::POST, "/api/v1/sync/mark") => {
                    api::mark_synced(&headers, &body, state).await
                }

                _ => not_found(),
            }
        })
    };

    run_server(state, routes).await
}

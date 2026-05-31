//! `rwa-issuer-portal` serverless entrypoint (Vercel lambda runtime).
//!
//! Records-only off-chain system of record for the rwa-kyc-hook tenant model.
//! Public routes: issuer self-registration, issuer read, investor KYC submit.
//! Operator routes (bearer-gated): KYC review, issuer status, on-chain sync
//! feed, mark-synced. The portal NEVER signs or writes on-chain — a separate
//! ops sync (`rwa-kyc-hook/scripts/sync-worker.sh`) consumes the feed and
//! drives the kyc-hook CLI.

use {
    rwa_issuer_portal::{config::Config, route_handler::run_server, router, state::AppState},
    std::{future::Future, pin::Pin, sync::Arc},
    vercel_runtime::{Body, Response},
};

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
            router::dispatch(&headers, &method, &path, &query, &body, state).await
        })
    };

    run_server(state, routes).await
}

//! HTTP handlers. Public: issuer registration, issuer read, KYC submit.
//! Portal-admin (bearer-gated): KYC review, issuer status, sync feed, mark-synced.

use std::sync::Arc;

use http::HeaderMap;
use serde_json::json;
use vercel_runtime::{Body, Response, StatusCode};

use crate::{
    error::{into_vercel_response, Error},
    issuer_id::parse_issuer_id,
    models::{MarkSyncedRequest, RegisterIssuerRequest, ReviewKycRequest, SubmitKycRequest},
    repo,
    state::AppState,
};

fn parse_body<T: serde::de::DeserializeOwned>(body: &Body) -> Result<T, Error> {
    let bytes = match body {
        Body::Text(s) => s.as_bytes(),
        Body::Binary(b) => b.as_slice(),
        Body::Empty => return Err(Error::BadRequest("empty request body".into())),
    };
    serde_json::from_slice(bytes).map_err(|e| Error::BadRequest(format!("invalid JSON: {e}")))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get("authorization").and_then(|v| v.to_str().ok())
}

fn query_param(query: &str, key: &str) -> Option<String> {
    serde_qs::from_str::<std::collections::HashMap<String, String>>(query)
        .ok()
        .and_then(|m| m.get(key).cloned())
}

pub async fn handle_health(state: Arc<AppState>) -> Response<Body> {
    // Cheap DB round-trip via a no-op query keeps health honest.
    let db_ok = repo::sync_feed(&state.db, 1).await.is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Text(
            json!({
                "service": "rwa-issuer-portal",
                "status": if db_ok { "ok" } else { "degraded" },
                "db": db_ok,
            })
            .to_string(),
        ))
        .unwrap()
}

// ---- Public: issuer self-registration ----
pub async fn register_issuer(body: &Body, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        let req: RegisterIssuerRequest = parse_body(body)?;
        let issuer = repo::create_issuer(
            &state.db,
            req.name.trim(),
            req.ops_authority.as_deref(),
            req.identity.as_deref(),
            req.contact_email.as_deref(),
        )
        .await?;
        Ok(issuer)
    }
    .await;
    into_vercel_response(result)
}

// ---- Public: read an issuer ----
pub async fn get_issuer(query: &str, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        let raw = query_param(query, "issuer_id")
            .ok_or_else(|| Error::BadRequest("issuer_id query param required".into()))?;
        let id = parse_issuer_id(&raw).map_err(Error::BadRequest)?;
        repo::get_issuer(&state.db, &id).await
    }
    .await;
    into_vercel_response(result)
}

// ---- Public: investor KYC submission ----
pub async fn submit_kyc(body: &Body, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        let req: SubmitKycRequest = parse_body(body)?;
        let id = parse_issuer_id(&req.issuer_id).map_err(Error::BadRequest)?;
        repo::validate_scope(&req.scope, req.offering_id.as_deref())?;
        repo::submit_kyc(
            &state.db,
            &id,
            req.wallet.trim(),
            &req.scope,
            req.offering_id.as_deref(),
        )
        .await
    }
    .await;
    into_vercel_response(result)
}

// ---- Portal admin: review a KYC submission ----
pub async fn review_kyc(headers: &HeaderMap, body: &Body, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        state.config.verify_portal_admin_token(bearer(headers))?;
        let req: ReviewKycRequest = parse_body(body)?;
        repo::review_kyc(&state.db, req.id, &req.decision, req.review_note.as_deref()).await
    }
    .await;
    into_vercel_response(result)
}

// ---- Portal admin: set issuer status ----
pub async fn set_issuer_status(
    headers: &HeaderMap,
    body: &Body,
    state: Arc<AppState>,
) -> Response<Body> {
    #[derive(serde::Deserialize)]
    struct Req {
        issuer_id: String,
        status: String,
    }
    let result = async {
        state.config.verify_portal_admin_token(bearer(headers))?;
        let req: Req = parse_body(body)?;
        let id = parse_issuer_id(&req.issuer_id).map_err(Error::BadRequest)?;
        repo::set_issuer_status(&state.db, &id, &req.status).await
    }
    .await;
    into_vercel_response(result)
}

// ---- Portal admin: on-chain sync feed ----
pub async fn sync_feed(headers: &HeaderMap, query: &str, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        state.config.verify_portal_admin_token(bearer(headers))?;
        let limit = query_param(query, "limit")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(100)
            .clamp(1, 1000);
        let items = repo::sync_feed(&state.db, limit).await?;
        Ok(json!({ "items": items, "count": items.len() }))
    }
    .await;
    into_vercel_response(result)
}

// ---- Portal admin: mark a KYC row synced on-chain ----
pub async fn mark_synced(headers: &HeaderMap, body: &Body, state: Arc<AppState>) -> Response<Body> {
    let result = async {
        state.config.verify_portal_admin_token(bearer(headers))?;
        let req: MarkSyncedRequest = parse_body(body)?;
        repo::mark_synced(&state.db, req.id).await
    }
    .await;
    into_vercel_response(result)
}

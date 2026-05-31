//! Local plain-HTTP dev server (feature `dev-server`). NOT used by the Vercel
//! deployment — that drives the lambda runtime via `portal`. This exists so
//! `npm run dev` can proxy to 127.0.0.1:8080 and so the ops sync worker can be
//! tested end-to-end without Vercel.
//!
//! Minimal HTTP/1.1 (no keep-alive, no chunked) — sufficient for the portal's
//! small JSON request/response shapes. Run:
//!   cargo run --features dev-server --bin dev-server

use std::sync::Arc;

use http::{HeaderMap, HeaderName, HeaderValue, Method};
use rwa_issuer_portal::{config::Config, router, state::AppState};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use vercel_runtime::Body;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rwa_issuer_portal=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let state = Arc::new(AppState::cold_start(config).await?);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("dev-server listening on http://{addr}");

    loop {
        let (mut socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(&mut socket, state).await {
                tracing::debug!(error = %e, "connection error");
            }
        });
    }
}

async fn handle_conn(
    socket: &mut tokio::net::TcpStream,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read until headers complete (\r\n\r\n), then the declared Content-Length.
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > 1 << 20 {
            return Ok(()); // 1 MiB header guard
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method_str = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");

    let mut headers = HeaderMap::new();
    let mut content_length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                headers.insert(name, val);
            }
        }
    }

    // Read the rest of the body.
    let mut body_bytes = buf[header_end + 4..].to_vec();
    while body_bytes.len() < content_length {
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body_bytes.extend_from_slice(&tmp[..n]);
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    let method = Method::from_bytes(method_str.as_bytes()).unwrap_or(Method::GET);
    let body = if body_bytes.is_empty() {
        Body::Empty
    } else {
        Body::Binary(body_bytes)
    };

    let resp = router::dispatch(&headers, &method, &path, &query, &body, state).await;

    // Serialize the http::Response back onto the wire.
    let status = resp.status();
    let mut out = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    let body_out: Vec<u8> = match resp.body() {
        Body::Empty => Vec::new(),
        Body::Text(s) => s.clone().into_bytes(),
        Body::Binary(b) => b.clone(),
    };
    for (k, v) in resp.headers() {
        if k.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        out.push_str(&format!("{}: {}\r\n", k, v.to_str().unwrap_or("")));
    }
    out.push_str(&format!("Content-Length: {}\r\n", body_out.len()));
    out.push_str("Connection: close\r\n\r\n");

    socket.write_all(out.as_bytes()).await?;
    socket.write_all(&body_out).await?;
    socket.flush().await?;
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

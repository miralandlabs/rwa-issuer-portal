use crate::error::Error;
use sha2::{Digest, Sha256};
use std::env;

/// Portal configuration. The portal is records-only: it never holds a Solana
/// keypair and never writes on-chain. It needs a shared Postgres DB and an
/// operator bearer token to gate review/sync endpoints.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    /// SHA-256 (hex) of the operator bearer token. Operator-only endpoints
    /// (KYC review, issuer activation, sync feed) require
    /// `Authorization: Bearer <token>` whose SHA-256 matches this.
    pub operator_token_sha256: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, Error> {
        let database_url = env::var("DATABASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Internal("DATABASE_URL is required".into()))?;

        // Accept either the precomputed hash or a raw token (hashed here).
        let operator_token_sha256 = env::var("PORTAL_OPERATOR_TOKEN_SHA256")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .or_else(|| {
                env::var("PORTAL_OPERATOR_TOKEN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(|t| sha256_hex(&t))
            });

        Ok(Self {
            database_url,
            operator_token_sha256,
        })
    }

    /// Constant-time-ish check that the presented bearer token matches the
    /// configured operator token hash. Returns `Unauthorized` when no token is
    /// configured (fail-closed: operator endpoints are disabled until set).
    pub fn verify_operator_token(&self, bearer: Option<&str>) -> Result<(), Error> {
        let want = self
            .operator_token_sha256
            .as_deref()
            .ok_or_else(|| Error::Unauthorized("operator token not configured".into()))?;
        let token = bearer
            .and_then(|h| h.strip_prefix("Bearer ").or(Some(h)))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Unauthorized("missing bearer token".into()))?;
        let got = sha256_hex(token);
        if constant_time_eq(got.as_bytes(), want.as_bytes()) {
            Ok(())
        } else {
            Err(Error::Unauthorized("invalid operator token".into()))
        }
    }
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_matches_and_rejects() {
        let cfg = Config {
            database_url: "x".into(),
            operator_token_sha256: Some(sha256_hex("s3cret")),
        };
        assert!(cfg.verify_operator_token(Some("Bearer s3cret")).is_ok());
        assert!(cfg.verify_operator_token(Some("s3cret")).is_ok());
        assert!(cfg.verify_operator_token(Some("Bearer wrong")).is_err());
        assert!(cfg.verify_operator_token(None).is_err());
    }

    #[test]
    fn unconfigured_token_fails_closed() {
        let cfg = Config {
            database_url: "x".into(),
            operator_token_sha256: None,
        };
        assert!(cfg.verify_operator_token(Some("Bearer anything")).is_err());
    }
}

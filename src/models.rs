//! Wire + row types for the issuer portal.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::issuer_id::uuid_to_issuer_id_hex;

/// An issuer (tenant) record.
#[derive(Debug, Clone, Serialize)]
pub struct Issuer {
    pub id: Uuid,
    /// The on-chain `issuer_id` (32-char hex) derived from `id`. Included in
    /// responses so the portal admin can copy it into the kyc-hook CLI.
    pub issuer_id_hex: String,
    pub name: String,
    pub status: String,
    pub ops_authority: Option<String>,
    pub identity: Option<String>,
    pub contact_email: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Issuer {
    pub fn from_row(row: &tokio_postgres::Row) -> Self {
        let id: Uuid = row.get("id");
        Self {
            issuer_id_hex: uuid_to_issuer_id_hex(&id),
            id,
            name: row.get("name"),
            status: row.get("status"),
            ops_authority: row.get("ops_authority"),
            identity: row.get("identity"),
            contact_email: row.get("contact_email"),
            created_at: row.get("created_at"),
        }
    }
}

/// Self-service issuer registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterIssuerRequest {
    pub name: String,
    #[serde(default)]
    pub ops_authority: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
}

/// An investor KYC record.
#[derive(Debug, Clone, Serialize)]
pub struct KycRecord {
    pub id: i64,
    pub issuer_id: Uuid,
    pub issuer_id_hex: String,
    pub wallet: String,
    pub scope: String,
    pub offering_id: Option<String>,
    pub status: String,
    pub is_verified: bool,
    pub synced_on_chain: bool,
    pub review_note: Option<String>,
}

impl KycRecord {
    pub fn from_row(row: &tokio_postgres::Row) -> Self {
        let issuer_id: Uuid = row.get("issuer_id");
        Self {
            issuer_id_hex: uuid_to_issuer_id_hex(&issuer_id),
            issuer_id,
            id: row.get("id"),
            wallet: row.get("wallet"),
            scope: row.get("scope"),
            offering_id: row.get("offering_id"),
            status: row.get("status"),
            is_verified: row.get("is_verified"),
            synced_on_chain: row.get("synced_on_chain"),
            review_note: row.get("review_note"),
        }
    }
}

/// Investor KYC submission.
#[derive(Debug, Deserialize)]
pub struct SubmitKycRequest {
    /// Issuer this KYC is for (dashed UUID or 32-char hex).
    pub issuer_id: String,
    pub wallet: String,
    /// "global" or "offering".
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub offering_id: Option<String>,
}

fn default_scope() -> String {
    "global".to_string()
}

/// Portal-admin KYC review decision.
#[derive(Debug, Deserialize)]
pub struct ReviewKycRequest {
    pub id: i64,
    /// "approved" or "rejected".
    pub decision: String,
    #[serde(default)]
    pub review_note: Option<String>,
}

/// Portal admin: mark a KYC row synced on-chain after the ops CLI ran.
#[derive(Debug, Deserialize)]
pub struct MarkSyncedRequest {
    pub id: i64,
}

/// A row the ops sync still needs to push on-chain. Mirrors exactly what the
/// kyc-hook `create-kyc-record` / `update-kyc-verified` scripts need.
#[derive(Debug, Clone, Serialize)]
pub struct SyncFeedItem {
    pub id: i64,
    pub issuer_id_hex: String,
    pub wallet: String,
    pub scope: String,
    pub offering_id: Option<String>,
    pub is_verified: bool,
}

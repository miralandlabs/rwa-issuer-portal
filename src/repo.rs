//! Data-access for issuers and investor KYC records.

use uuid::Uuid;

use crate::{
    db::Db,
    error::Error,
    issuer_id::uuid_to_issuer_id_hex,
    models::{Issuer, KycRecord, SyncFeedItem},
};

const OFFERING_MAX_LEN: usize = 31;

/// Validate the (scope, offering_id) pair against the on-chain seed rules.
pub fn validate_scope(scope: &str, offering_id: Option<&str>) -> Result<(), Error> {
    match scope {
        "global" => {
            if offering_id.is_some() {
                return Err(Error::BadRequest(
                    "offering_id must be omitted for global scope".into(),
                ));
            }
            Ok(())
        }
        "offering" => {
            let oid = offering_id.ok_or_else(|| {
                Error::BadRequest("offering_id is required for offering scope".into())
            })?;
            if oid.is_empty() || oid.len() > OFFERING_MAX_LEN {
                return Err(Error::BadRequest(format!(
                    "offering_id must be 1..={OFFERING_MAX_LEN} bytes"
                )));
            }
            Ok(())
        }
        other => Err(Error::BadRequest(format!("invalid scope '{other}'"))),
    }
}

pub async fn create_issuer(
    db: &Db,
    name: &str,
    ops_authority: Option<&str>,
    identity: Option<&str>,
    contact_email: Option<&str>,
) -> Result<Issuer, Error> {
    if name.trim().is_empty() {
        return Err(Error::BadRequest("name is required".into()));
    }
    let row = db
        .query_opt(
            "INSERT INTO issuers (name, ops_authority, identity, contact_email)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name, status, ops_authority, identity, contact_email, created_at",
            &[&name, &ops_authority, &identity, &contact_email],
            "create_issuer",
        )
        .await?
        .ok_or_else(|| Error::Internal("insert returned no row".into()))?;
    Ok(Issuer::from_row(&row))
}

pub async fn get_issuer(db: &Db, id: &Uuid) -> Result<Issuer, Error> {
    let row = db
        .query_opt(
            "SELECT id, name, status, ops_authority, identity, contact_email, created_at
             FROM issuers WHERE id = $1",
            &[id],
            "get_issuer",
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("issuer {id} not found")))?;
    Ok(Issuer::from_row(&row))
}

/// Operator: set issuer status (active / paused / closed).
pub async fn set_issuer_status(db: &Db, id: &Uuid, status: &str) -> Result<Issuer, Error> {
    if !matches!(status, "pending" | "active" | "paused" | "closed") {
        return Err(Error::BadRequest(format!("invalid status '{status}'")));
    }
    let row = db
        .query_opt(
            "UPDATE issuers SET status = $2, updated_at = NOW()
             WHERE id = $1
             RETURNING id, name, status, ops_authority, identity, contact_email, created_at",
            &[id, &status],
            "set_issuer_status",
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("issuer {id} not found")))?;
    Ok(Issuer::from_row(&row))
}

/// Investor submits KYC (idempotent on the unique key — re-submission resets a
/// rejected/pending row back to pending, but never downgrades an approved one).
pub async fn submit_kyc(
    db: &Db,
    issuer_id: &Uuid,
    wallet: &str,
    scope: &str,
    offering_id: Option<&str>,
) -> Result<KycRecord, Error> {
    if wallet.trim().is_empty() {
        return Err(Error::BadRequest("wallet is required".into()));
    }
    // Issuer must exist (FK would error, but we give a clean 404).
    get_issuer(db, issuer_id).await?;

    let row = db
        .query_opt(
            "INSERT INTO issuer_kyc_records (issuer_id, wallet, scope, offering_id)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (issuer_id, wallet, scope, offering_id) DO UPDATE
               SET updated_at = NOW(),
                   status = CASE WHEN issuer_kyc_records.status = 'rejected'
                                 THEN 'pending' ELSE issuer_kyc_records.status END
             RETURNING id, issuer_id, wallet, scope, offering_id, status,
                       is_verified, synced_on_chain, review_note",
            &[issuer_id, &wallet, &scope, &offering_id],
            "submit_kyc",
        )
        .await?
        .ok_or_else(|| Error::Internal("upsert returned no row".into()))?;
    Ok(KycRecord::from_row(&row))
}

pub async fn get_kyc(db: &Db, id: i64) -> Result<KycRecord, Error> {
    let row = db
        .query_opt(
            "SELECT id, issuer_id, wallet, scope, offering_id, status,
                    is_verified, synced_on_chain, review_note
             FROM issuer_kyc_records WHERE id = $1",
            &[&id],
            "get_kyc",
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("kyc record {id} not found")))?;
    Ok(KycRecord::from_row(&row))
}

/// Operator review. Approve sets is_verified=true and re-arms the sync flag so
/// the ops worker pushes it on-chain; reject clears verification (and, if it
/// was previously verified, re-arms the sync so the on-chain flag is revoked).
pub async fn review_kyc(
    db: &Db,
    id: i64,
    decision: &str,
    review_note: Option<&str>,
) -> Result<KycRecord, Error> {
    let (status, is_verified) = match decision {
        "approved" => ("approved", true),
        "rejected" => ("rejected", false),
        other => return Err(Error::BadRequest(format!("invalid decision '{other}'"))),
    };
    let row = db
        .query_opt(
            "UPDATE issuer_kyc_records
                SET status = $2,
                    is_verified = $3,
                    review_note = $4,
                    synced_on_chain = FALSE,
                    decided_at = NOW(),
                    updated_at = NOW()
              WHERE id = $1
              RETURNING id, issuer_id, wallet, scope, offering_id, status,
                        is_verified, synced_on_chain, review_note",
            &[&id, &status, &is_verified, &review_note],
            "review_kyc",
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("kyc record {id} not found")))?;
    Ok(KycRecord::from_row(&row))
}

/// The ops-sync feed: approved decisions whose on-chain state is stale.
pub async fn sync_feed(db: &Db, limit: i64) -> Result<Vec<SyncFeedItem>, Error> {
    let rows = db
        .query(
            "SELECT id, issuer_id, wallet, scope, offering_id, is_verified
             FROM issuer_kyc_records
             WHERE synced_on_chain = FALSE
               AND status IN ('approved', 'rejected')
             ORDER BY updated_at ASC
             LIMIT $1",
            &[&limit],
            "sync_feed",
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let issuer_id: Uuid = r.get("issuer_id");
            SyncFeedItem {
                id: r.get("id"),
                issuer_id_hex: uuid_to_issuer_id_hex(&issuer_id),
                wallet: r.get("wallet"),
                scope: r.get("scope"),
                offering_id: r.get("offering_id"),
                is_verified: r.get("is_verified"),
            }
        })
        .collect())
}

/// Operator marks a row synced after running the kyc-hook ops CLI.
pub async fn mark_synced(db: &Db, id: i64) -> Result<KycRecord, Error> {
    let n = db
        .execute(
            "UPDATE issuer_kyc_records SET synced_on_chain = TRUE, updated_at = NOW()
             WHERE id = $1",
            &[&id],
            "mark_synced",
        )
        .await?;
    if n == 0 {
        return Err(Error::NotFound(format!("kyc record {id} not found")));
    }
    get_kyc(db, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_validation() {
        assert!(validate_scope("global", None).is_ok());
        assert!(validate_scope("global", Some("series-a")).is_err());
        assert!(validate_scope("offering", Some("series-a")).is_ok());
        assert!(validate_scope("offering", None).is_err());
        assert!(validate_scope("offering", Some("")).is_err());
        assert!(validate_scope("offering", Some(&"a".repeat(32))).is_err());
        assert!(validate_scope("offering", Some(&"a".repeat(31))).is_ok());
        assert!(validate_scope("bogus", None).is_err());
    }
}

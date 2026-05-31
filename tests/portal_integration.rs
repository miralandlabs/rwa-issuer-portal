//! End-to-end portal tests against a real Postgres database.
//!
//! Gated on `TEST_DATABASE_URL` and marked `#[ignore]` by default so `cargo test`
//! stays hermetic. Run manually:
//!
//! ```bash
//! TEST_DATABASE_URL='postgres://...' cargo test --test portal_integration -- --ignored
//! ```

use std::env;

use rwa_issuer_portal::{
    config::{sha256_hex, Config},
    error::Error,
    repo,
    state::AppState,
};

const TEST_WALLET: &str = "11111111111111111111111111111111";
const ADMIN_TOKEN: &str = "integration-test-token";

async fn test_state() -> AppState {
    let database_url =
        env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set for integration tests");
    let config = Config {
        database_url,
        portal_admin_token_sha256: Some(sha256_hex(ADMIN_TOKEN)),
    };
    AppState::cold_start(config)
        .await
        .expect("cold_start failed")
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn kyc_lifecycle_and_sync_feed() {
    let state = test_state().await;
    let db = &state.db;

    let issuer = repo::create_issuer(db, "Integration Issuer", None, None, None)
        .await
        .expect("create issuer");
    repo::set_issuer_status(db, &issuer.id, "active")
        .await
        .expect("activate issuer");

    let kyc = repo::submit_kyc(db, &issuer.id, TEST_WALLET, "global", None)
        .await
        .expect("submit kyc");
    assert_eq!(kyc.status, "pending");

    let reviewed = repo::review_kyc(db, kyc.id, "approved", Some("ok"))
        .await
        .expect("review kyc");
    assert!(reviewed.is_verified);
    assert!(!reviewed.synced_on_chain);

    let feed = repo::sync_feed(db, 100).await.expect("sync feed");
    assert!(feed.iter().any(|i| i.id == kyc.id));

    let synced = repo::mark_synced(db, kyc.id).await.expect("mark synced");
    assert!(synced.synced_on_chain);

    let feed_after = repo::sync_feed(db, 100).await.expect("sync feed after");
    assert!(!feed_after.iter().any(|i| i.id == kyc.id));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn global_kyc_resubmit_is_idempotent() {
    let state = test_state().await;
    let db = &state.db;

    let issuer = repo::create_issuer(db, "Idempotent Issuer", None, None, None)
        .await
        .expect("create issuer");
    repo::set_issuer_status(db, &issuer.id, "active")
        .await
        .expect("activate issuer");

    let first = repo::submit_kyc(db, &issuer.id, TEST_WALLET, "global", None)
        .await
        .expect("first submit");
    let second = repo::submit_kyc(db, &issuer.id, TEST_WALLET, "global", None)
        .await
        .expect("second submit");
    assert_eq!(first.id, second.id);

    let listed = repo::list_kyc(db, None, Some(&issuer.id), 100)
        .await
        .expect("list kyc");
    let global_rows: Vec<_> = listed
        .iter()
        .filter(|r| r.wallet == TEST_WALLET && r.scope == "global")
        .collect();
    assert_eq!(global_rows.len(), 1);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn mark_synced_returns_conflict_when_already_synced() {
    let state = test_state().await;
    let db = &state.db;

    let issuer = repo::create_issuer(db, "Conflict Issuer", None, None, None)
        .await
        .expect("create issuer");
    repo::set_issuer_status(db, &issuer.id, "active")
        .await
        .expect("activate issuer");

    let kyc = repo::submit_kyc(db, &issuer.id, TEST_WALLET, "global", None)
        .await
        .expect("submit kyc");
    repo::review_kyc(db, kyc.id, "approved", None)
        .await
        .expect("review kyc");
    repo::mark_synced(db, kyc.id)
        .await
        .expect("first mark synced");

    let err = repo::mark_synced(db, kyc.id)
        .await
        .expect_err("second mark synced should fail");
    assert!(matches!(err, Error::Conflict(_)));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn submit_kyc_rejects_non_active_issuer() {
    let state = test_state().await;
    let db = &state.db;

    let issuer = repo::create_issuer(db, "Pending Issuer", None, None, None)
        .await
        .expect("create issuer");
    assert_eq!(issuer.status, "pending");

    let err = repo::submit_kyc(db, &issuer.id, TEST_WALLET, "global", None)
        .await
        .expect_err("submit on pending issuer should fail");
    assert!(matches!(err, Error::BadRequest(_)));
}

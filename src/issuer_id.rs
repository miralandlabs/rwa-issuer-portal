//! The issuer-id binding contract.
//!
//! The portal's `issuers.id` is a UUID. The on-chain `rwa-kyc-hook` program
//! seeds its `IssuerConfig` / `KycRecord` PDAs with a raw 16-byte `issuer_id`
//! and the kyc-hook CLI accepts it as a **32-char lowercase hex string with no
//! dashes** (see `rwa-kyc-hook` `parse_issuer_id_hex`). This module is the
//! single place that converts between the two so the off-chain record and the
//! on-chain tenant can never drift.
//!
//!   UUID `550e8400-e29b-41d4-a716-446655440000`
//!     ⇄ on-chain `RWA_KYC_HOOK_ISSUER_ID=550e8400e29b41d4a716446655440000`

use uuid::Uuid;

/// A freshly generated issuer id (UUID v4).
pub fn new_issuer_uuid() -> Uuid {
    Uuid::new_v4()
}

/// Render a UUID as the on-chain `issuer_id` hex (32 lowercase hex chars, no
/// dashes) — exactly the value passed to the kyc-hook CLI as
/// `--issuer-id` / `RWA_KYC_HOOK_ISSUER_ID`.
pub fn uuid_to_issuer_id_hex(id: &Uuid) -> String {
    // `simple` = 32 hex chars, no hyphens, lowercase.
    id.simple().to_string()
}

/// Parse a user-supplied issuer id into a canonical UUID, accepting either the
/// dashed UUID form or the 32-char on-chain hex form.
pub fn parse_issuer_id(input: &str) -> Result<Uuid, String> {
    let s = input.trim();
    Uuid::parse_str(s).map_err(|e| format!("invalid issuer id '{s}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_32_lowercase_no_dashes() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let hex = uuid_to_issuer_id_hex(&id);
        assert_eq!(hex, "550e8400e29b41d4a716446655440000");
        assert_eq!(hex.len(), 32);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn round_trips_through_on_chain_hex() {
        let id = new_issuer_uuid();
        let hex = uuid_to_issuer_id_hex(&id);
        // The on-chain hex (32 chars) must parse back to the same UUID.
        let back = Uuid::parse_str(&hex).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn parse_accepts_both_forms() {
        let dashed = "550e8400-e29b-41d4-a716-446655440000";
        let hex = "550e8400e29b41d4a716446655440000";
        assert_eq!(
            parse_issuer_id(dashed).unwrap(),
            parse_issuer_id(hex).unwrap()
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_issuer_id("not-a-uuid").is_err());
        assert!(parse_issuer_id("550e8400").is_err());
    }
}

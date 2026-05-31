-- rwa-issuer-portal schema. Shares the same Supabase database as aethervane /
-- spl-token-balance (one DB for preview, one for production); these tables are
-- namespaced by name and do not collide with the shared `parameters` table.
--
-- Binding contract (see rwa-kyc-hook/docs/SYNC_RUNBOOK.md):
--   issuers.id (UUID)  ==  on-chain issuer_id (32-char hex, no dashes)
-- The portal is the off-chain system of record. A separate ops sync reads the
-- `kyc_sync_feed` view and drives the on-chain PDA writes with the ops keypair;
-- the portal itself never signs or writes on-chain.

-- ---- Issuers (one row per tenant) ------------------------------------------
CREATE TABLE IF NOT EXISTS issuers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    -- Lifecycle: pending (self-registered) -> active (platform-approved) ->
    -- paused | closed. Mirrors the on-chain IssuerStatus but is advisory here;
    -- the on-chain RegisterIssuer is a separate step (ops keypair).
    status          TEXT NOT NULL DEFAULT 'pending'
        CONSTRAINT issuers_status_check
        CHECK (status IN ('pending', 'active', 'paused', 'closed')),
    -- Base58 Solana pubkeys, recorded so the portal admin can RegisterIssuer
    -- on-chain with the matching ops/identity authorities.
    ops_authority   TEXT,
    identity        TEXT,
    contact_email   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_issuers_status ON issuers (status);

-- ---- Investor KYC records (one row per issuer × wallet × scope) ------------
CREATE TABLE IF NOT EXISTS issuer_kyc_records (
    id              BIGSERIAL PRIMARY KEY,
    issuer_id       UUID NOT NULL REFERENCES issuers(id) ON DELETE CASCADE,
    -- Base58 investor wallet (the KYC subject / transfer recipient owner).
    wallet          TEXT NOT NULL,
    -- 'global' or 'offering'; offering rows carry a non-null offering_id
    -- (max 31 UTF-8 bytes to match the on-chain KycRecord seed limit).
    scope           TEXT NOT NULL
        CONSTRAINT issuer_kyc_scope_check CHECK (scope IN ('global', 'offering')),
    offering_id     TEXT
        CONSTRAINT issuer_kyc_offering_len CHECK (offering_id IS NULL OR length(offering_id) <= 31),
    -- KYC review lifecycle (off-chain decision).
    status          TEXT NOT NULL DEFAULT 'pending'
        CONSTRAINT issuer_kyc_status_check
        CHECK (status IN ('pending', 'approved', 'rejected')),
    -- The flag the ops sync must mirror on-chain via update-kyc-verified.
    is_verified     BOOLEAN NOT NULL DEFAULT FALSE,
    -- Has the on-chain KycRecord been synced to match `is_verified`?
    synced_on_chain BOOLEAN NOT NULL DEFAULT FALSE,
    review_note     TEXT,
    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at      TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Offering scope: a NULL offering_id and a present one are distinct keys.
    -- COALESCE keeps the global row unique per (issuer, wallet) while allowing
    -- one row per (issuer, wallet, offering).
    CONSTRAINT issuer_kyc_unique UNIQUE (issuer_id, wallet, scope, offering_id)
);

CREATE INDEX IF NOT EXISTS idx_kyc_issuer_wallet
    ON issuer_kyc_records (issuer_id, wallet);

-- Rows the ops sync still needs to push on-chain: an approved+verified
-- decision whose on-chain KycRecord has not yet been updated. The sync worker
-- (portal-admin bearer gated) reads this, calls create/update-kyc-verified, then
-- marks `synced_on_chain = true`.
CREATE INDEX IF NOT EXISTS idx_kyc_sync_pending
    ON issuer_kyc_records (issuer_id)
    WHERE synced_on_chain = FALSE;

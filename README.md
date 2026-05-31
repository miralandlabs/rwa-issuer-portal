# rwa-issuer-portal

Off-chain **system of record** for the x402 [`rwa-kyc-hook`](../rwa-kyc-hook)
tenant model: issuer registration + investor KYC. The portal records decisions
in Postgres; it **never signs or writes on-chain**. A separate ops sync reads
the sync feed and drives the kyc-hook CLI with the ops keypair.

```
investor / issuer ──HTTP──▶ rwa-issuer-portal ──Postgres──┐
                                                          │  (records only)
operator ──review/approve──▶                              │
                                                          ▼
                              GET /api/v1/sync/feed ──▶ ops sync ──▶ rwa-kyc-hook CLI ──▶ on-chain KycRecord
```

## The binding contract

`issuers.id` (a UUID) **is** the on-chain `issuer_id`. The portal renders it as
the 32-char, dash-free hex string the kyc-hook CLI expects:

| Portal `issuers.id` (UUID) | On-chain `issuer_id` (`RWA_KYC_HOOK_ISSUER_ID`) |
| -------------------------- | ----------------------------------------------- |
| `550e8400-e29b-41d4-a716-446655440000` | `550e8400e29b41d4a716446655440000` |

Every issuer/KYC response includes `issuer_id_hex` so operators can copy it
straight into `./scripts/*.sh` in the kyc-hook repo. See
[`rwa-kyc-hook/docs/SYNC_RUNBOOK.md`](../rwa-kyc-hook/docs/SYNC_RUNBOOK.md).

## Shared database

Reuses the **same Supabase Postgres** as `aethervane` and
`spl-token-balance` (one DB for preview deployments, one for production). The
portal adds its own `issuers` / `issuer_kyc_records` tables (no collision with
the shared `parameters` table); the idempotent migration runs on cold start.

## HTTP surface

Public:

| Method | Path | Body / query | Purpose |
| ------ | ---- | ------------ | ------- |
| `POST` | `/api/v1/issuers` | `{name, ops_authority?, identity?, contact_email?}` | Self-register an issuer |
| `GET`  | `/api/v1/issuers?issuer_id=<uuid\|hex>` | — | Read an issuer |
| `POST` | `/api/v1/kyc` | `{issuer_id, wallet, scope, offering_id?}` | Submit investor KYC |
| `GET`  | `/health` | — | Liveness + DB probe |

Operator (require `Authorization: Bearer <PORTAL_OPERATOR_TOKEN>`):

| Method | Path | Body | Purpose |
| ------ | ---- | ---- | ------- |
| `POST`  | `/api/v1/kyc/review` | `{id, decision: approved\|rejected, review_note?}` | Approve / reject a KYC record |
| `PATCH` | `/api/v1/issuers` | `{issuer_id, status}` | Set issuer status |
| `GET`   | `/api/v1/sync/feed?limit=` | — | Decisions whose on-chain state is stale |
| `POST`  | `/api/v1/sync/mark` | `{id}` | Mark a record synced after running the CLI |

## KYC lifecycle

```
submit ─▶ pending ─review─▶ approved (is_verified=true,  synced_on_chain=false)
                        └──▶ rejected (is_verified=false, synced_on_chain=false)
ops sync: read feed ─▶ run kyc-hook create/update-kyc-verified ─▶ POST /sync/mark ─▶ synced_on_chain=true
```

`scope` is `global` or `offering`; offering rows carry an `offering_id`
(≤ 31 bytes, matching the on-chain KycRecord seed limit).

## Run locally

```bash
cp env.example .env   # set DATABASE_URL + PORTAL_OPERATOR_TOKEN

# backend (records-only)
cargo run --bin portal           # serves on the vercel_runtime local port

# storefront (emerald light/dark, proxies /api to the backend)
cd storefront && npm install && npm run dev   # http://localhost:5173
```

`cargo test` covers the issuer-id binding, operator-token auth, and scope
validation. `npm run build` emits the storefront into `../public` for Vercel.

## Sync worker (example)

The ops side is deliberately out of this repo (it holds the keypair). A minimal
worker, per the kyc-hook runbook:

```bash
for row in $(curl -s -H "Authorization: Bearer $TOKEN" "$PORTAL/api/v1/sync/feed" | jq -c '.items[]'); do
  ISSUER=$(echo "$row" | jq -r .issuer_id_hex)
  WALLET=$(echo "$row" | jq -r .wallet)
  SCOPE=$(echo "$row" | jq -r .scope)
  VERIFIED=$(echo "$row" | jq -r .is_verified)
  ID=$(echo "$row" | jq -r .id)
  export RWA_KYC_HOOK_ISSUER_ID="$ISSUER" OPS_KEYPAIR=/secure/ops.json
  ( cd ../rwa-kyc-hook/scripts && ./create-kyc-record.sh "$SCOPE" "$WALLET" 2>/dev/null || true
    ./update-kyc-verified.sh "$SCOPE" "$WALLET" "$VERIFIED" )
  curl -s -X POST -H "Authorization: Bearer $TOKEN" "$PORTAL/api/v1/sync/mark" -d "{\"id\":$ID}"
done
```

## Theme

Layout + emerald light/dark theme reuse the
[`x402-buy-spl-token`](../x402-buy-spl-token) storefront scheme (`theme.css`
verbatim, JetBrains Mono, `data-theme` toggle persisted to `localStorage`).

# rwa-issuer-portal

Off-chain **system of record** for the x402 [`rwa-kyc-hook`](https://github.com/miralandlabs/rwa-kyc-hook)
tenant model: issuer registration + investor KYC. The portal records decisions
in Postgres; it **never signs or writes on-chain**. A separate ops sync reads
the sync feed and drives the kyc-hook CLI with the ops keypair.

```
investor / issuer ──HTTP──▶ rwa-issuer-portal ──Postgres──┐
                                                          │  (records only)
portal admin ──review/approve──▶                          │
                                                          ▼
                              GET /api/v1/sync/feed ──▶ ops sync ──▶ rwa-kyc-hook CLI ──▶ on-chain KycRecord
```

One **portal admin** bearer token per deployment (not per issuer). On-chain signing stays in the separate ops sync + `ops_authority` keypair.

## The binding contract

`issuers.id` (a UUID) **is** the on-chain `issuer_id`. The portal renders it as
the 32-char, dash-free hex string the kyc-hook CLI expects:

| Portal `issuers.id` (UUID) | On-chain `issuer_id` (`RWA_KYC_HOOK_ISSUER_ID`) |
| -------------------------- | ----------------------------------------------- |
| `550e8400-e29b-41d4-a716-446655440000` | `550e8400e29b41d4a716446655440000` |

Every issuer/KYC response includes `issuer_id_hex` for the kyc-hook CLI. See
[`rwa-kyc-hook/docs/SYNC_RUNBOOK.md`](https://github.com/miralandlabs/rwa-kyc-hook/blob/main/docs/SYNC_RUNBOOK.md).

## Required environment

| Variable | Required | Purpose |
| -------- | -------- | ------- |
| `DATABASE_URL` | Yes | Shared Supabase Postgres (same DB as aethervane / spl-token-balance) |
| `PORTAL_ADMIN_TOKEN` or `PORTAL_ADMIN_TOKEN_SHA256` | Yes | One portal admin; gates review + sync feed |
| `PORTAL_ADMIN_TOKEN_OPTIONAL` | No | Set to `1` for local dev without admin token (not for production) |

Copy `env.example` → `.env` for local dev. The portal adds `issuers` /
`issuer_kyc_records` tables via idempotent migration on cold start.

## HTTP surface

**Public**

| Method | Path | Purpose |
| ------ | ---- | ------- |
| `POST` | `/api/v1/issuers` | Self-register an issuer |
| `GET`  | `/api/v1/issuers?issuer_id=` | Read an issuer |
| `POST` | `/api/v1/kyc` | Submit investor KYC |
| `GET`  | `/health` | Liveness + DB probe |

**Portal admin** (`Authorization: Bearer <PORTAL_ADMIN_TOKEN>`)

| Method | Path | Purpose |
| ------ | ---- | ------- |
| `GET`   | `/api/v1/kyc` | List KYC records (`?status=`, `?issuer_id=`) |
| `POST`  | `/api/v1/kyc/review` | Approve / reject KYC |
| `PATCH` | `/api/v1/issuers` | Set issuer status |
| `GET`   | `/api/v1/sync/feed` | Rows stale vs on-chain |
| `POST`  | `/api/v1/sync/mark` | Mark synced after CLI run |

## KYC lifecycle

```
submit → pending → review → approved/rejected (synced_on_chain=false)
ops sync: feed → kyc-hook create/update → POST /sync/mark → synced_on_chain=true
```

`scope` is `global` or `offering` (offering id ≤ 31 UTF-8 bytes, matching on-chain seeds).

## Run locally

```bash
cp env.example .env

# Backend (records-only; plain HTTP on :8080)
cargo run --bin dev-server --features dev-server

# Storefront (proxies /api → backend)
cd storefront && npm install && npm run dev   # http://localhost:5173
```

`cargo test` covers issuer-id binding, portal-admin auth, scope validation, and wallet pubkey checks.

Integration tests (real Postgres) live in `tests/portal_integration.rs` and are `#[ignore]` by default:

```bash
TEST_DATABASE_URL='postgres://...' cargo test --test portal_integration -- --ignored
```

Build the Vite storefront into `public/` (required before Vercel deploy):

```bash
npm run build:storefront
```

## Deploy to Vercel

This repo matches the [`x402-buy-spl-token`](https://github.com/miralandlabs/x402-buy-spl-token) pattern:

1. **Rust API** — `vercel-rust` serverless binary (`src/bin/portal.rs`)
2. **Static UI** — Vite build output in `public/` (not committed; built in CI)
3. **CI deploy** — GitHub Actions runs tests, builds storefront, then `vercel build` + `vercel deploy --prebuilt`

Do **not** rely on the Vercel dashboard “Import Git Repository” alone — the Rust
builder and empty `public/` on clone will fail. Use the workflow instead.

**One-time setup**

1. Create a Vercel project (empty or linked manually once).
2. Add GitHub repo secrets: `VERCEL_TOKEN`, `ORG_ID`, `PROJECT_ID`.
3. Set Vercel env vars: `DATABASE_URL`, `PORTAL_ADMIN_TOKEN_SHA256` (or `PORTAL_ADMIN_TOKEN`).
4. Push to `main` — workflow deploys production; other branches get previews.

Pin Vercel CLI to **52.x** in CI (newer CLI breaks legacy `builds` + `vercel-rust`).

## Sync worker (example)

The ops side is out of this repo (it holds the keypair):

```bash
for row in $(curl -s -H "Authorization: Bearer $PORTAL_ADMIN_TOKEN" "$PORTAL/api/v1/sync/feed" | jq -c '.items[]'); do
  ISSUER=$(echo "$row" | jq -r .issuer_id_hex)
  WALLET=$(echo "$row" | jq -r .wallet)
  SCOPE=$(echo "$row" | jq -r .scope)
  VERIFIED=$(echo "$row" | jq -r .is_verified)
  ID=$(echo "$row" | jq -r .id)
  export RWA_KYC_HOOK_ISSUER_ID="$ISSUER" OPS_KEYPAIR=/secure/ops.json
  # run rwa-kyc-hook scripts (create if needed, then update verified)
  curl -s -X POST -H "Authorization: Bearer $PORTAL_ADMIN_TOKEN" "$PORTAL/api/v1/sync/mark" -d "{\"id\":$ID}"
done
```

## Theme

Emerald light/dark layout reused from
[`x402-buy-spl-token`](https://github.com/miralandlabs/x402-buy-spl-token) (`theme.css`, JetBrains Mono, `data-theme` toggle).

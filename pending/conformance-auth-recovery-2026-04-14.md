# Conformance auth recovery blocker (2026-04-14)

## Decision / blocker

Live Shopify conformance is currently blocked because the stored Shopify CLI-backed Admin GraphQL bearer token is expired, and the repo's current refresh instructions were only partially correct.

## What was learned

1. `corepack pnpm conformance:probe` and `corepack pnpm conformance:capture-products` both fail with:
   - `401`
   - `[API] Service is not valid for authentication`
2. Refreshing `https://accounts.shopify.com/oauth/token` does **not** accept the payload as JSON here.
   - `Content-Type: application/json` returns `400 invalid_request` complaining that `client_id` / `grant_type` are missing.
   - `Content-Type: application/x-www-form-urlencoded` works for the refresh request format.
3. Shopify appears to rotate or invalidate the previous refresh grant after a successful refresh response.
   - A one-off manual refresh succeeded during this run, but because the returned token material was not persisted in the same step, the old refresh token then started returning `invalid_grant`.
4. The currently stored identity/application tokens in `~/.config/shopify-cli-kit-nodejs/config.json` no longer authenticate against Admin GraphQL in any tested header combination.

## Options considered

### Option A — recover from another persisted Shopify CLI/session source

- Search for another persisted copy of the newly rotated refresh token or a separate valid store-scoped session.
- Best if such a source exists, because it avoids interactive login.
- Not completed in this run; no obvious backup config file was found.

### Option B — re-establish Shopify CLI auth non-interactively on this host

- If the Shopify CLI binary or equivalent auth helper can be installed/restored, use it to mint a fresh session and then mirror the new bearer into repo `.env`.
- Most likely durable fix if host automation is expected to keep running unattended.

### Option C — switch conformance to a dedicated Admin API token

- Provision a dev-store token not coupled to the volatile Shopify CLI account session.
- Best long-term operational choice for unattended conformance jobs, but requires app/token provisioning work.

## Blocked downstream work

- Refreshing conformance fixtures this run
- Any implementation pass that requires new live Shopify shape evidence beyond existing checked-in fixtures

## Recommended next step

Prefer **Option B** or **Option C**:

1. restore/install a working Shopify CLI auth path on this host (or otherwise obtain a fresh valid bearer)
2. immediately persist the returned `accessToken` and rotated `refreshToken` to both:
   - `~/.config/shopify-cli-kit-nodejs/config.json`
   - `/home/airhorns/code/shopify-draft-proxy/.env`
3. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-products`

## Follow-up note

The `shopify-draft-proxy-live-conformance` skill was patched during this run to record the important correction that the refresh request must be **form-encoded**, not JSON.

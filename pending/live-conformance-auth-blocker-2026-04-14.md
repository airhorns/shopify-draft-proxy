# Live conformance auth blocker — 2026-04-14

## Decision to make

How should unattended cron runs recover Shopify live conformance when the host's Shopify CLI account session is no longer refreshable?

## What happened

During this run (originally, then reconfirmed on the later `productChangeStatus` pass, reconfirmed again on the subsequent timestamp-search surface-expansion pass the same day, and reconfirmed again on the product-media mutation pass):

- `corepack pnpm typecheck` passed
- `corepack pnpm test` passed
- `corepack pnpm run worklist:check` passed
- `corepack pnpm conformance:probe` failed with:
  - `401`
  - `{"errors":"[API] Service is not valid for authentication"}`
- `corepack pnpm conformance:capture-products` also failed with the same `401` auth error
- refreshing against `https://accounts.shopify.com/oauth/token` with Shopify CLI client id `fbdb2649-e327-4907-8f67-908d24cfd7e3` returned:
  - HTTP `400`
  - payload shape indicating `invalid_grant`

## Current state

- repo `.env` still points `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` at the expired CLI-derived bearer token
- `~/.config/shopify-cli-kit-nodejs/config.json` still contains the same unrecoverable access/refresh token pair
- because the refresh call returned `invalid_grant`, this host cannot recover the current CLI-backed grant non-interactively

## Options considered

### 1. Keep relying on Shopify CLI account session refresh
- Pros:
  - matches the current host-specific workflow
  - avoids introducing a new store token right now
- Cons:
  - brittle refresh-token rotation can strand unattended runs
  - once the stored pair is spent, cron cannot recover autonomously

### 2. Switch conformance to a dedicated custom-app Admin API token
- Pros:
  - stable credential for unattended probe/capture runs
  - avoids Shopify account-session expiry and rotation behavior
  - simplifies future conformance automation
- Cons:
  - requires provisioning and storing a safe dev-store token

## Blocked downstream work

Any increment that needs fresh live conformance verification or refreshed fixtures is blocked until auth is restored.

## Recommended next step

Provision a dedicated dev-store Admin API token for `very-big-test-store.myshopify.com` and wire `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` to that stable credential.

If the project must stay on the Shopify CLI-session path, a human needs to re-authenticate Shopify CLI on this host and immediately persist the refreshed token pair back into both:

- `~/.config/shopify-cli-kit-nodejs/config.json`
- `/home/airhorns/code/shopify-draft-proxy/.env`

# Live conformance auth blocker — 2026-04-15

## Decision to make

How should unattended conformance runs recover once the host's Shopify CLI account session is no longer refreshable non-interactively?

## What happened on this run

- `corepack pnpm conformance:probe` failed with:
  - HTTP `401`
  - payload: `{"errors":"[API] Service is not valid for authentication"}`
- `corepack pnpm conformance:capture-products` failed with the same `401` auth error
- a fresh re-check on `2026-04-15T01:14:41Z` after the publication-status increment still reproduced the same probe/capture `401` failures
- another re-check on `2026-04-15T01:39Z` after the `productSet` increment still reproduced the same probe/capture `401` failures
- another re-check on `2026-04-15T02:16Z` after the collection-membership replacement increment still reproduced the same probe `401` failure
- another re-check on `2026-04-15T02:36Z` after the top-level `productVariant` / `inventoryItem` read increment still reproduced the same probe `401` failure
- another re-check on `2026-04-15T03:18Z` after the collection membership mutation increment still reproduced the same probe `401` failure
- a fresh re-check on `2026-04-15T03:31Z` before the handle/status sort-key increment still reproduced the same probe `401` failure
- another verification re-check on `2026-04-15T03:35Z` after the handle/status sort-key increment still reproduced the same probe `401` failure
- `corepack pnpm conformance:capture-products` on the same run also failed with the same `401` / `[API] Service is not valid for authentication` response
- another verification run of `corepack pnpm conformance:capture-products` on `2026-04-15T03:35Z` failed with the same `401` / `[API] Service is not valid for authentication` response
- another verification re-check on `2026-04-15T03:54Z` after the product tag-mutation increment still reproduced the same probe `401` failure
- another verification run of `corepack pnpm conformance:capture-products` on `2026-04-15T03:54Z` failed with the same `401` / `[API] Service is not valid for authentication` response
- another verification re-check on `2026-04-15T04:16:24Z` after the `inventoryAdjustQuantities` increment still reproduced the same probe `401` failure
- another verification run of `corepack pnpm conformance:capture-products` on `2026-04-15T04:16:24Z` failed with the same `401` / `[API] Service is not valid for authentication` response
- a fresh non-interactive refresh attempt on `2026-04-15T03:19Z` against `https://accounts.shopify.com/oauth/token` using Shopify CLI client id `fbdb2649-e327-4907-8f67-908d24cfd7e3` still returned:
  - HTTP `400`
  - OAuth payload: `{"error":"invalid_grant","error_description":"The provided access grant is invalid, expired, or revoked ..."}`
- another non-interactive refresh attempt on `2026-04-15T03:55Z` against the same Shopify Accounts endpoint and client id returned the same unrecoverable result:
  - HTTP `400`
  - OAuth payload: `{"error":"invalid_grant","error_description":"The provided access grant is invalid, expired, or revoked ..."}`
- another non-interactive refresh attempt on `2026-04-15T04:16:24Z` against the same Shopify Accounts endpoint and client id again returned the same unrecoverable result:
  - HTTP `400`
  - OAuth payload: `{"error":"invalid_grant","error_description":"The provided access grant is invalid, expired, or revoked ..."}`

## Current state

- historical note: repo `.env` in the original checkout pointed `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` at the expired CLI-derived bearer token, but current mainline conformance scripts no longer use repo `.env` as the canonical credential source
- current canonical conformance credential path is `~/.shopify-draft-proxy/conformance-admin-auth.json`
- `~/.config/shopify-cli-kit-nodejs/config.json` still contains the same unrecoverable access/refresh token pair
- the active CLI session is still present structurally, but the persisted grant is no longer usable for unattended refresh

## Why this blocks conformance

This run touched supported `products` overlay behavior, so fresh live probe/capture verification should have been rerun before stopping. That is currently blocked by unrecoverable auth.

## Recommended next step

Switch conformance to a dedicated dev-store Admin API token or fresh expiring offline token pair for `very-big-test-store.myshopify.com` instead of the rotating Shopify CLI account bearer token, and persist it into `~/.shopify-draft-proxy/conformance-admin-auth.json`.

If the project must continue using the CLI-session path, a human needs to re-authenticate Shopify CLI and then persist the new token pair into both:

- `~/.config/shopify-cli-kit-nodejs/config.json`
- `~/.shopify-draft-proxy/conformance-admin-auth.json` (or regenerate the repo's manual store-auth link and exchange flow so the shared home credential is replaced atomically)

# Fulfillment lifecycle conformance blocker

## What this run checked

Refreshed the next fulfillment lifecycle probes on `very-big-test-store.myshopify.com` using the current repo conformance credential.

- `fulfillmentTrackingInfoUpdate` — the first merchant-facing fulfillment lifecycle root for updating tracking details after a fulfillment exists
- `fulfillmentCancel` — the adjacent cancellation root for reversing a fulfillment lifecycle step
- `corepack pnpm conformance:capture-orders`

## Current refresh blocker

The old missing-repo-local-app-`.env` failure from a worktree-specific run is historical only. Current `main` now falls back to `/tmp/shopify-conformance-app/<handle>/.env` when the checked-in app directory lacks its own `.env`.

Current blocker on the main checkout:

- shared credential path: `~/.shopify-draft-proxy/conformance-admin-auth.json`
- app secret path used for refresh: `/tmp/shopify-conformance-app/hermes-conformance-products/.env`
- failing command: `corepack pnpm conformance:capture-orders`
- exact blocker: `Stored Shopify conformance access token is invalid and refresh failed: This request requires an active refresh_token`
- interpretation: this is a dead saved grant above the fulfillment lifecycle probes; it does not invalidate the last verified fulfillment access-denied evidence below.

## Current credential summary

- credential family: `shpca`
- header mode: `raw-x-shopify-access-token`
- the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host

## Shared store auth token on disk

- path: `~/.shopify-draft-proxy/conformance-admin-auth.json`
- status: `present-shpca-user-token-not-offline-capable`
- token family: `shpca`
- cached scopes: `read_product_listings`, `read_themes`, `write_assigned_fulfillment_orders`, `write_content`, `write_customers`, `write_discounts`, `write_draft_orders`, `write_files`, `write_fulfillments`, `write_inventory`, `write_locations`, `write_markets`, `write_merchant_managed_fulfillment_orders`, `write_metaobject_definitions`, `write_metaobjects`, `write_order_edits`, `write_orders`, `write_products`, `write_publications`, `write_returns`, `write_shipping`, `write_third_party_fulfillment_orders`, `write_translations`
- associated user scopes: none recorded
- interpretation: The shared home-folder conformance credential still caches a `shpca` user token, so it does not satisfy Shopify's offline-token requirement for `orderCreate` even though its cached scope strings include order scopes.

Legacy note:

- repo-local `.manual-store-auth-token.json` and repo `.env` can still exist in older checkouts on this host, but they are legacy artifacts and must not be treated as the live conformance credential source.

## Current run summary

### Captured pre-access validation slices

- `fulfillmentTrackingInfoUpdate` inline missing `fulfillmentId`
  - exact message: missing error payload
- `fulfillmentTrackingInfoUpdate` inline `fulfillmentId: null`
  - exact message: missing error payload
- `fulfillmentTrackingInfoUpdate` missing `$fulfillmentId`
  - exact message: missing error payload
- `fulfillmentCancel` inline missing `id`
  - exact message: missing error payload
- `fulfillmentCancel` inline `id: null`
  - exact message: missing error payload
- `fulfillmentCancel` missing `$id`
  - exact message: missing error payload

### Remaining live happy-path blockers

### `fulfillmentTrackingInfoUpdate`

- result: access denied on the current repo credential
- exact message: Access denied for fulfillmentTrackingInfoUpdate field. Required access: One of `write_assigned_fulfillment_orders`, `write_merchant_managed_fulfillment_orders`, or `write_third_party_fulfillment_orders` access scopes. Also: The user must have permission to fulfill and ship orders.
- required access summary: `write_assigned_fulfillment_orders`, `write_merchant_managed_fulfillment_orders`, `write_third_party_fulfillment_orders`; required permissions: `fulfill_and_ship_orders`

### `fulfillmentCancel`

- result: access denied on the current repo credential
- exact message: Access denied for fulfillmentCancel field.
- required access summary: Shopify did not return a narrower required-scope string in the current payload

## Practical interpretation

The first fulfillment-domain increment now includes evidence-backed GraphQL validation slices for both `fulfillmentTrackingInfoUpdate` and `fulfillmentCancel`, alongside the earlier captured `fulfillmentCreate` invalid-id branch. The broader fulfillment lifecycle happy paths remain blocked on live access under the current repo credential.

Practical next step for fulfillment lifecycle parity:

1. provision a credential/install that can write the relevant fulfillment family
2. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-orders`
3. once the roots are reachable, capture the smallest safe fulfillment lifecycle sequence in order:
   - `fulfillmentTrackingInfoUpdate`
   - `fulfillmentCancel`
4. only after live write evidence exists should the proxy start staging tracking-update/cancel semantics or downstream fulfillment read effects locally

# Order creation conformance blocker

## What this run checked

Refreshed the current orders-domain creation probes on `very-big-test-store.myshopify.com` using the repo conformance credential.

- `orderCreate`
- `draftOrderCreate`
- `draftOrderComplete`
- `corepack pnpm conformance:capture-orders`

## Current credential summary

- credential family: `shpca`
- header mode: `raw-x-shopify-access-token`
- the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host

## Saved manual store auth token on disk

- path: `.manual-store-auth-token.json`
- status: `present-shpca-user-token-not-offline-capable`
- token family: `shpca`
- cached scopes: `read_product_listings`, `read_themes`, `write_assigned_fulfillment_orders`, `write_content`, `write_customers`, `write_discounts`, `write_draft_orders`, `write_files`, `write_fulfillments`, `write_inventory`, `write_locations`, `write_markets`, `write_merchant_managed_fulfillment_orders`, `write_metaobject_definitions`, `write_metaobjects`, `write_order_edits`, `write_orders`, `write_products`, `write_publications`, `write_returns`, `write_shipping`, `write_third_party_fulfillment_orders`, `write_translations`
- associated user scopes: none recorded
- interpretation: The saved manual store-auth artifact still caches a `shpca` user token, so it does not satisfy Shopify's offline-token requirement for `orderCreate` even though its cached scope strings include order scopes.

## Current run summary

- current run is auth-regressed before the family-specific creation roots can be reprobed
- live probe failure: `401` / `[API] Invalid API key or access token (unrecognized login or wrong password)`
- the checked-in fixtures are the last verified live references and should not be overwritten with `401` payloads during this regression

### `orderCreate`
- result on this run: auth-regressed before the root-specific create path could be reprobed
- last verified happy-path fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json`
- the checked-in fixture preserves immediate `order(id:)` read-after-write visibility

### `draftOrderCreate`
- result on this run: auth-regressed before the root-specific create path could be reprobed
- last verified happy-path fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json`
- immediate downstream detail fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json`

### `draftOrderComplete`
- result on this run: auth-regressed before the root-specific completion blocker could be reprobed
- last verified family-specific access-denied evidence: Access denied for draftOrderComplete field. Required access: `write_draft_orders` access scope. Also: The user must have access to mark as paid, or set payment terms.
- required access summary: `write_draft_orders`; required permissions: `mark-as-paid`, `set-payment-terms`

## Practical interpretation

- this auth regression does **not** invalidate the last verified merchant-facing create fixtures or the existing local runtime slices they back
- the remaining creation-family live blocker after auth is repaired is still `draftOrderComplete` requiring write access that can mark as paid or set payment terms

## Recommended next step

1. run `corepack pnpm conformance:refresh-auth`
2. if refresh returns `invalid_request` / `This request requires an active refresh_token`, stop retrying the dead saved grant and generate a fresh manual store-auth link before continuing
3. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-orders`

Refresh this note with `corepack pnpm conformance:capture-orders` after any credential or store-state change.

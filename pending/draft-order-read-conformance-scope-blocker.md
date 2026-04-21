# Draft-order read conformance blocker

## What this run checked

Refreshed the first draft-order read probes on `very-big-test-store.myshopify.com` using the current repo conformance credential.

- `draftOrder(id: ...)` — direct draft-order detail read surface that downstream local staging would need immediately after a safe draft-order create/edit flow
- `draftOrders(first: ...)` — draft-order catalog surface for merchant list views and local overlay replay
- `draftOrdersCount(query:)` — draft-order count surface that needs to stay aligned with draft-order catalog filtering later
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

- current run is auth-regressed before the draft-order read roots could be reprobed
- live probe failure: `401` / `[API] Invalid API key or access token (unrecognized login or wrong password)`
- the checked-in fixtures are the last verified live references and should not be overwritten with `401` payloads during this regression

## Direct-order read baseline that remains safe

The current repo still keeps the last verified direct-order empty-state baseline in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-empty-state.json`:

- `order(id: "gid://shopify/Order/0")` -> `null`
- `orders(first: 1, sortKey: CREATED_AT, reverse: true)` -> empty connection with null cursors
- `ordersCount` -> `{ count: 0, precision: EXACT }`

## Last verified draft-order catalog/count evidence

### `draftOrders`

- result on this run: auth-regressed before the root could be reprobed
- last verified captured fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json`

### `draftOrdersCount`

- result on this run: auth-regressed before the root could be reprobed
- last verified captured fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-count.json`

## Practical interpretation

- local proxy/runtime already supports the narrow unfiltered staged synthetic `draftOrders` / `draftOrdersCount` slice in snapshot mode and live-hybrid mode
- the remaining gap is auth repair for rerunning the now-captured live baseline, not missing draft-order catalog/count fixtures

## Recommended next step

1. run `corepack pnpm conformance:refresh-auth`
2. if refresh returns `invalid_request` / `This request requires an active refresh_token`, stop retrying the dead saved grant and generate a fresh manual store-auth link before continuing
3. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-orders`
4. once auth is healthy again, refresh `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json` and `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-count.json`

Refresh this note with `corepack pnpm conformance:capture-orders` after any credential or store-state change.

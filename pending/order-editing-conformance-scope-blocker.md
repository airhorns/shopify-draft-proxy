# Order editing conformance blocker

## What this run checked

Refreshed the first order-editing mutation probes on `very-big-test-store.myshopify.com` using the current repo conformance credential.

- `orderEditBegin` — the session-start root for Shopify's order-edit flow
- `orderEditAddVariant` — the first merchant-realistic edit step for adding sellable items to a calculated order
- `orderEditSetQuantity` — the quantity-adjustment root for calculated order line items
- `orderEditCommit` — the commit/apply root that would eventually need local downstream order visibility after staged edits
- `corepack pnpm conformance:capture-orders`

## Current credential summary

- credential family: `shpca`
- header mode: `raw-x-shopify-access-token`
- the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host

## Saved manual store auth token on disk

- path: `.manual-store-auth-token.json`
- status: `missing`
- token family: `missing`
- cached scopes: none recorded
- associated user scopes: none recorded
- interpretation: No saved manual store-auth artifact is currently available for this run.

## Live blocker evidence for the order-edit family

### `orderEditBegin`

- exact message: Access denied for orderEditBegin field. Required access: Requires `write_order_edits` access scope.
- required access summary: `write_order_edits`

### `orderEditAddVariant`

- exact message: Access denied for orderEditAddVariant field. Required access: `write_order_edits` access scope.
- required access summary: `write_order_edits`

### `orderEditSetQuantity`

- exact message: Access denied for orderEditSetQuantity field. Required access: `write_order_edits` access scope.
- required access summary: `write_order_edits`

### `orderEditCommit`

- exact message: Access denied for orderEditCommit field. Required access: Requires `write_order_edits` access scope.
- required access summary: `write_order_edits`

## Practical interpretation

- the proxy already supports a first local calculated-order edit flow for synthetic/local orders in snapshot mode and live-hybrid mode
- safe missing-`$id` GraphQL validation coverage is now captured for `orderEditBegin`, `orderEditAddVariant`, `orderEditSetQuantity`, and `orderEditCommit`
- the remaining gap is live Shopify parity for non-local orders; happy-path Shopify probes for all four initial roots still hit `write_order_edits` on this host before the resolver reveals broader session-shape semantics

## Practical next step for order-edit parity

1. keep the checked-in first local calculated-order edit flow for synthetic/local orders as-is
2. provision a credential/install with `write_order_edits`
3. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-orders`
4. once the roots are writable, capture the smallest safe sequence in order:
   - `orderEditBegin`
   - `orderEditAddVariant`
   - `orderEditSetQuantity`
   - `orderEditCommit`
5. only after live evidence exists for non-local orders should the proxy broaden the calculated-order runtime beyond the current synthetic/local slice

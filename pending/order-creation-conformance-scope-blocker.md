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
- status: `missing`
- token family: `missing`
- cached scopes: none recorded
- associated user scopes: none recorded
- interpretation: No saved manual store-auth artifact is currently available for this run.

## Current run summary

### `orderCreate`

- result: not freshly captured on this run
- checked-in happy-path fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json`
- the checked-in fixture preserves immediate `order(id:)` read-after-write visibility
- exact message: missing error payload
- required access summary: missing requiredAccess payload

### `draftOrderCreate`

- result: captured success on the current repo credential
- checked-in happy-path fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json`
- immediate downstream detail fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json`
- the checked-in fixture preserves immediate `draftOrder(id:)` read-after-write visibility

### `draftOrderComplete`

- result: access denied
- exact message: Access denied for draftOrderComplete field. Required access: `write_draft_orders` access scope. Also: The user must have access to mark as paid, or set payment terms.
- required access summary: `write_draft_orders`; required permissions: `mark-as-paid`, `set-payment-terms`

## Repo impact

- `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json` and `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json` remain the live references for the current happy-path creation slices
- the checked-in fixtures continue to back immediate `order(id:)` and `draftOrder(id:)` read-after-write visibility
- the creation family still keeps `draftOrderComplete` blocked until write access can mark as paid or set payment terms

Refresh this note with `corepack pnpm conformance:capture-orders` after any credential or store-state change.

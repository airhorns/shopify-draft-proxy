# Fulfillment lifecycle conformance blocker

## What this run checked

Refreshed the next fulfillment lifecycle probes on `very-big-test-store.myshopify.com` using the current repo conformance credential.

- `fulfillmentTrackingInfoUpdate` — the first merchant-facing fulfillment lifecycle root for updating tracking details after a fulfillment exists
- `fulfillmentCancel` — the adjacent cancellation root for reversing a fulfillment lifecycle step
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

### Captured pre-access validation slices

- `fulfillmentTrackingInfoUpdate` inline missing `fulfillmentId`
  - exact message: Field 'fulfillmentTrackingInfoUpdate' is missing required arguments: fulfillmentId
- `fulfillmentTrackingInfoUpdate` inline `fulfillmentId: null`
  - exact message: Argument 'fulfillmentId' on Field 'fulfillmentTrackingInfoUpdate' has an invalid value (null). Expected type 'ID!'.
- `fulfillmentTrackingInfoUpdate` missing `$fulfillmentId`
  - exact message: Variable $fulfillmentId of type ID! was provided invalid value
- `fulfillmentCancel` inline missing `id`
  - exact message: Field 'fulfillmentCancel' is missing required arguments: id
- `fulfillmentCancel` inline `id: null`
  - exact message: Argument 'id' on Field 'fulfillmentCancel' has an invalid value (null). Expected type 'ID!'.
- `fulfillmentCancel` missing `$id`
  - exact message: Variable $id of type ID! was provided invalid value

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

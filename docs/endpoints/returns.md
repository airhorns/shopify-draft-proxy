# Returns Endpoint Group

The returns group is modeled as an order-backed reverse-logistics slice. The local source of truth is the normalized
`Order.returns` array, so supported return writes update order state and downstream reads without sending runtime
mutations to Shopify.

## Supported roots

Overlay reads:

- `return`

Local staged mutations:

- `returnCreate`
- `returnRequest`
- `returnCancel`
- `returnClose`
- `returnReopen`

## Behavior notes

- `return(id:)` resolves returns already present on the local order graph and returns `null` for missing IDs in snapshot
  mode. It serializes the supported Return detail slice from the same record used by nested `Order.returns`.
- `returnCreate` stages a local `OPEN` return for a known order and known fulfillment line items. The local return stores a
  stable synthetic Return ID, ReturnLineItem IDs, status, name, timestamps, quantity, reason, reason note, and order
  linkage. The original raw mutation is retained in the meta log for explicit commit replay.
- `returnRequest` stages the same order-backed shape with status `REQUESTED`.
- `returnCancel`, `returnClose`, and `returnReopen` update the status of known local returns. `returnClose` records a local
  `closedAt` timestamp; `returnReopen` clears it.
- Supported return mutations are handled locally in snapshot mode and for local/synthetic orders in live-hybrid mode.
  They do not call upstream Shopify at runtime.
- Validation branches for unknown orders, unknown fulfillment line items, invalid quantities, and unknown returns return
  local `userErrors` and do not append staged commit-log entries.

## Blocked roots

- `returnCalculate` is blocked on calculation parity for restocking fees, exchange lines, return shipping fees, taxes,
  discounts, and error behavior.
- `returnableFulfillment` and `returnableFulfillments` are blocked on returnability eligibility and line-item quantity
  parity over fulfilled orders.
- `removeFromReturn` is blocked on captured removal semantics for return and exchange line items, quantity recomputation,
  and downstream read effects.
- `returnProcess` is blocked on processing semantics for returned/exchanged items, refunds, duties, shipping, financial
  transfers, notification behavior, and downstream order/refund/return effects.
- `reverseDelivery`, `reverseFulfillmentOrder`, `reverseDeliveryCreateWithShipping`,
  `reverseDeliveryShippingUpdate`, and `reverseFulfillmentOrderDispose` are blocked on a normalized reverse fulfillment
  order and reverse delivery graph with captured tracking, label, disposition, and inventory/location effects.
- `returnApprove` and `returnDecline` are not exposed by live 2025-01 or 2026-04 Admin GraphQL root introspection on the
  current conformance shop.

## Validation anchors

- Runtime behavior: `tests/integration/order-return-flow.test.ts`
- No-side-effect schema evidence: live 2025-01 and 2026-04 conformance introspection captured root signatures for
  `return`, `returnCalculate`, `returnableFulfillment(s)`, `reverseDelivery`, `reverseFulfillmentOrder`, and the listed
  mutation payloads.

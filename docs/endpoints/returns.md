# Returns Endpoint Group

The returns group is modeled as an order-backed reverse-logistics slice. The local source of truth is the normalized
`Order.returns` array, so supported return writes update order state and downstream reads without sending runtime
mutations to Shopify.

## Current support and limitations

### Supported roots

Overlay reads:

- `return`

Local staged mutations:

- `returnCreate`
- `returnRequest`
- `returnApproveRequest`
- `returnDeclineRequest`
- `returnCancel`
- `returnClose`
- `returnReopen`
- `removeFromReturn`
- `returnProcess`

### Behavior notes

- `return(id:)` resolves returns already present on the local order graph and returns `null` for missing IDs in snapshot
  mode. It serializes the supported Return detail slice from the same record used by nested `Order.returns`.
- `returnCreate` stages a local `OPEN` return for a known order and known fulfillment line items. The local return stores a
  stable synthetic Return ID, ReturnLineItem IDs, status, name, timestamps, quantity, reason, reason note, order linkage,
  and reverse fulfillment order work for the returned quantities. The original raw mutation is retained in the meta log
  for explicit commit replay.
- `returnRequest` stages the same order-backed shape with status `REQUESTED`.
- `returnApproveRequest` transitions a local `REQUESTED` return to `OPEN`, clears any decline metadata, and creates a
  reverse fulfillment order with line work for the approved return line quantities.
- `returnDeclineRequest` transitions a local `REQUESTED` return to `DECLINED` and stores the selected decline reason/note.
  Customer notification side effects are not sent.
- `returnCancel`, `returnClose`, and `returnReopen` update the status of known local returns. `returnClose` records a local
  `closedAt` timestamp; `returnReopen` clears it.
- `removeFromReturn` reduces or removes return line quantities, recomputes `totalQuantity`, and syncs the associated
  reverse fulfillment order line quantities. Exchange-line removal remains explicitly unsupported until exchange fixtures
  exist.
- `returnProcess` updates processed quantities for local return line items, closes the return when all lines are processed,
  and syncs reverse fulfillment order remaining quantities. Refund duties, refund shipping, financial transfers, exchange
  processing, and notification behavior are not emulated beyond local metadata and validation boundaries.
- `reverseDeliveryCreateWithShipping` treats an empty `reverseDeliveryLineItems` input as Shopify documents it: the proxy
  creates one local reverse delivery line for each line item on the reverse fulfillment order. `ReverseDeliveryLabelInput`
  accepts Shopify's `fileUrl` field and preserves it as the downstream `label.publicFileUrl`; legacy local fixture aliases
  `publicFileUrl` and `url` are still accepted for older recorded runtime fixtures.
- Supported return mutations are handled locally in snapshot mode and for local/synthetic orders in live-hybrid mode.
  They do not call upstream Shopify at runtime.
- Validation branches for unknown orders, unknown fulfillment line items, invalid quantities, and unknown returns return
  local `userErrors` and do not append staged commit-log entries.
- HAR-353 promotes `return-lifecycle-local-staging` from fixture-only evidence to generic strict parity. The parity replay
  seeds a fulfilled order graph, then compares `returnCreate`, `returnClose`, `returnReopen`, `returnCancel`,
  downstream `return(id:)` and `Order.returns` reads, `returnRequest`, and a missing fulfillment-line-item validation
  branch against an explicit local-runtime fixture. The live reverse-logistics introspection fixture remains schema
  evidence for root availability and blocked roots; it is not the behavior payload for the strict local lifecycle replay.
- HAR-370 adds executable local-runtime parity for `returnApproveRequest`, `returnDeclineRequest`, `removeFromReturn`,
  `returnProcess`, reverse delivery creation/update, reverse fulfillment disposal, and downstream reverse logistics reads.
  The current checked-in evidence uses the local parity harness plus live 2026-04 root/type introspection; success-path live
  return/reverse-logistics mutation captures still need disposable order setup and cleanup before claiming carrier,
  refund-transfer, exchange, notification, or inventory movement fidelity.
- HAR-442 reviews the return/reverse-logistics slice against current Shopify docs and public examples. It adds executable
  coverage for documented empty reverse-delivery line expansion and `ReverseDeliveryLabelInput.fileUrl` handling while
  keeping live success-path captures, exchange processing, carrier label creation, notification sends, refund transfers,
  duties, and inventory/location movement as explicit unsupported fidelity gaps.

### Blocked roots

- `returnCalculate` is blocked on calculation parity for restocking fees, exchange lines, return shipping fees, taxes,
  discounts, and error behavior.
- `returnableFulfillment` and `returnableFulfillments` are blocked on returnability eligibility and line-item quantity
  parity over fulfilled orders.
- Exchange-line removal/processing is blocked on captured exchange item semantics and downstream order/return effects.
- Refund duties, refund shipping, financial transfers, notification sends, carrier labels, and inventory/location movement
  remain local-only boundaries until live success-path captures prove the side effects and cleanup path.
- `returnApprove` and `returnDecline` are not exposed by live 2025-01 or 2026-04 Admin GraphQL root introspection on the
  current conformance shop.

## Historical and developer notes

### Validation anchors

- Runtime behavior: `tests/integration/order-return-flow.test.ts`
- Executable parity: `config/parity-specs/orders/return-lifecycle-local-staging.json`
- HAR-370 executable parity:
  `config/parity-specs/orders/return-reverse-logistics-local-staging.json`,
  `config/parity-specs/orders/return-request-decline-local-staging.json`, and
  `config/parity-specs/orders/removeFromReturn-local-staging.json`
- HAR-442 extends `config/parity-specs/orders/return-reverse-logistics-local-staging.json` to exercise empty
  `reverseDeliveryLineItems` replay and `fileUrl` label input normalization.
- No-side-effect schema evidence: live 2025-01 and 2026-04 conformance introspection captured root signatures for
  `return`, `returnCalculate`, `returnableFulfillment(s)`, `reverseDelivery`, `reverseFulfillmentOrder`, and the listed
  mutation payloads.

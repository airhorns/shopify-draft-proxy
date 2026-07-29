---
title: 'Returns Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Returns Endpoint Group.'
---

The returns group is modeled as an order-backed reverse-logistics slice. The local source of truth is staged return,
reverse-delivery, and reverse-fulfillment-order records linked back to orders, so supported return writes update local
state and downstream reads without sending runtime mutations to Shopify.

## Current support and limitations

Return roots are documented as their own endpoint group, but the current registry snapshot does not have a separate `returns` domain. The support claims below are based on Rust runtime handlers in `src/proxy.rs` plus order-backed parity specs, tests, and fixtures.

### Supported roots

Overlay reads:

- `return`
- `returnableFulfillments`
- `returnCalculate`

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

### Local behavior

- `return(id:)` resolves staged return records and returns `null` for missing IDs in snapshot mode. Nested
  `Order.returns` reads are derived from the same staged return records through the order-to-return index, so top-level
  and nested reads observe the same staged status, quantity, shipping-fee, and line-item state.
- Generic `node(id:)` and `nodes(ids:)` resolve staged `Return`, `ReturnableFulfillment`, `ReturnLineItem`,
  `UnverifiedReturnLineItem`, `ReverseDelivery`, `ReverseDeliveryLineItem`, `ReverseFulfillmentOrder`, and
  `ReverseFulfillmentOrderLineItem` records from the order-backed return/reverse-logistics graph. Return creation,
  close/reopen, reverse delivery create/update, reverse fulfillment disposal, dump/restore, and order deletion are
  reflected immediately where they affect the selected node. Missing, consumed, or deleted IDs return `null`, and
  `nodes(ids:)` preserves input order and duplicates.
- `Order.returnStatus` is derived from the effective return lifecycle attached to the order and is projected consistently
  through singular `order`, `orders`, generic `node`, and nested mutation-payload order selections. Captured 2026-04
  behavior maps zero returns to `NO_RETURN`, any `REQUESTED` return to `RETURN_REQUESTED`, otherwise any `OPEN` return
  to `IN_PROGRESS`, otherwise any `CLOSED` return to `RETURNED`. Declined-only and canceled-only return sets report
  `NO_RETURN`; a return whose last line is removed is `CLOSED` with `totalQuantity: 0` and contributes `RETURNED`.
- `returnableFulfillments(orderId:)` derives returnable fulfillment lines from the order's staged fulfillment graph and
  subtracts quantities already claimed by staged returns. For locally staged orders it stays fully local even in
  live-hybrid mode; for cold live-hybrid orders the return hydrator may fetch the order fulfillment graph before
  projecting the returnable lines. The current supported slice covers fulfilled line items and quantity reduction after
  staged returns; broader Shopify eligibility rules remain a fidelity boundary.
- `returnCalculate(input:)` derives calculated return line items from the order's fulfilled line item prices, per-line tax
  lines, requested quantities, and submitted restocking-fee percentages. Captured 2026-04 evidence shows refund
  subtotals and proportional line tax as negative money amounts, percentage restocking fees as calculated positive fee
  amounts, and `returnShippingFee: null` when no return shipping fee is submitted. The proxy mirrors that shape for
  staged fulfilled orders and uses a narrow live-hybrid order hydrate only when the local order graph lacks the price or
  tax fields needed for calculation. Exchange lines, shipping-fee calculations, discounts, and error branches remain
  explicit partial-fidelity boundaries.
- `returnCreate` stages a local `OPEN` return for the submitted order and fulfillment line items. The local return stores
  a stable synthetic Return ID, ReturnLineItem IDs, status, name, quantity, reason, reason note, order linkage, return
  shipping fee, and reverse-fulfillment-order references for returned quantities. The original raw mutation is retained
  in the meta log for explicit commit replay.
- `returnRequest` stages the same order-backed shape with status `REQUESTED` and uses the same already-returned quantity
  cap as `returnCreate`. Public `notifyCustomer` input is accepted, but the public Admin schema does not expose the
  non-public `tmp_notify_customer` payload; variable-bound requests that include it fail with top-level
  `INVALID_VARIABLE` before local staging. The proxy does not send notification side effects.
- `returnCreate` and `returnRequest` validate the complete line batch and build an allocation-free plan before creating
  Return or ReturnLineItem IDs, reading the local clock, staging return/reverse-fulfillment records, or appending commit
  replay entries. A rejected batch therefore leaves staged state, synthetic identity allocation, timestamps, and the
  commit log unchanged; a successful retry observes the same identities and timestamps as a clean first attempt.
- Cold live-hybrid return mutations resolve their order prerequisites with a bounded order query stored as observed base
  evidence rather than as a staged write. If one or more requested fulfillment lines are outside that bounded order
  slice, the proxy sends one batched `nodes(ids:)` query for only those line IDs. Authoritative `null` nodes produce the
  captured root-specific `NOT_FOUND` line errors. Existing nodes whose relationship to the requested order is not proven,
  transport failures, GraphQL errors, and malformed responses remain unresolved and abort without caching partial
  evidence or changing mutation state. Snapshot mode never hydrates upstream.
- `returnCreate` / `returnRequest` validate return-line reasons before order hydration, return staging, or mutation-log
  append. Public 2026-04 capture shows root-specific missing-reason shapes: `returnCreate` returns `NOT_FOUND` on
  `["returnInput", "returnLineItems", "0"]`, while `returnRequest` returns `BLANK` on
  `["input", "returnLineItems", "0", "returnReason"]`. `returnCreate` rejects legacy `returnReason: OTHER` without a
  note on `["returnInput", "returnLineItems", "0", "returnReasonNote"]`; captured public `returnRequest` accepts legacy
  `OTHER` and the public `other-reason` `returnReasonDefinitionId` without a note on this shop/API version. Invalid
  legacy `returnReason` variable values are rejected at the GraphQL variable-coercion layer with `INVALID_VARIABLE`.
- `returnApproveRequest` transitions a local `REQUESTED` return to `OPEN`, clears any decline metadata, and creates a
  reverse fulfillment order with line work for the approved return line quantities. Approving a return whose status is no
  longer `REQUESTED` returns `INVALID_STATE` on `["input", "id"]` with Shopify's rendered
  `Return is not approvable. Only returns with status REQUESTED can be approved.` message, does not change the return,
  and does not create additional reverse fulfillment order work. Unknown Return IDs return `NOT_FOUND` on
  `["input", "id"]` with `Return not found.` Public `notifyCustomer` input is accepted, while non-public
  `tmp_notify_customer` payloads are rejected by public-schema input validation before the handler runs. The proxy does
  not send notification side effects.
- `returnDeclineRequest` transitions a local `REQUESTED` return to `DECLINED` and stores the selected decline reason/note.
  Variable-bound decline reasons must be public `ReturnDeclineReason` enum values (`RETURN_PERIOD_ENDED`, `FINAL_SALE`,
  or `OTHER`); out-of-set values fail public GraphQL variable coercion with top-level `INVALID_VARIABLE` errors and no
  `data` payload before local staging. Decline notes longer than 500 characters return `TOO_LONG` on
  `["input", "declineNote"]`. Public `notifyCustomer` input is accepted, while non-public `tmp_notify_customer`
  payloads are rejected by public-schema input validation before the handler runs. The proxy does not send notification
  side effects.
  Declining a non-`REQUESTED` return returns `INVALID_STATE` on `["input", "id"]` with Shopify's rendered
  `Return is not declinable. Only non-refunded returns with status REQUESTED can be declined.` message and leaves local
  return state unchanged. Declining an already `DECLINED` return returns `INVALID_STATE` with
  `The return is already declined.` Unknown Return IDs return `NOT_FOUND` on `["input", "id"]` with
  `Return not found.`
- `returnClose` transitions `OPEN` returns to `CLOSED` and records a local `closedAt` timestamp. Already `CLOSED`
  returns are returned unchanged without re-stamping `closedAt` or order `updatedAt`. Other statuses, except
  line-item-empty `REQUESTED` returns, return `INVALID_STATE` on `["id"]` with Shopify's captured
  `Return status is invalid.` message and do not mutate local state.
- `returnReopen` transitions `CLOSED` returns back to `OPEN` and clears `closedAt`. Already `OPEN` returns are returned
  unchanged without re-stamping order `updatedAt`; non-`CLOSED` / non-`OPEN` returns produce the same captured
  `INVALID_STATE` status error without staging changes.
- `returnCancel` transitions cancelable `OPEN` returns to `CANCELED`; already `CANCELED` returns are returned unchanged.
  Returns with processed or refunded return-line quantities produce `INVALID_STATE` on `["id"]` with Shopify's captured
  `Return is not cancelable.` message and do not mutate local state.
- `removeFromReturn` reduces or removes return line quantities, recomputes `totalQuantity`, and syncs the associated
  reverse fulfillment order line quantities only while the return is `OPEN` or `REQUESTED`. Closed, canceled, declined,
  and processed returns return `INVALID_STATE` on `["returnId"]` with Shopify's captured `Return status is invalid.`
  message and leave return lines, totals, and reverse fulfillment order work unchanged. Return line removal quantities
  must be positive and no greater than the removable quantity for that return line. Exchange-line removal remains
  explicitly unsupported until exchange fixtures exist.
- `returnProcess` updates processed quantities for local return line items and keeps the return `OPEN` in both mutation
  payloads and immediate downstream reads. Refund duties, refund shipping, financial transfers, exchange processing, and
  notification behavior are not emulated beyond local metadata and validation boundaries.
- `reverseDeliveryCreateWithShipping` builds staged reverse delivery lines from `reverseDeliveryLineItems`. Explicit
  entries preserve input order, quantity, and the requested reverse fulfillment order line item; an empty input follows
  Shopify's documented expansion rule and creates one local reverse delivery line for each reverse fulfillment order line
  at that line's total quantity. `ReverseDeliveryLabelInput` accepts Shopify's `fileUrl` field and preserves it as the
  downstream `label.publicFileUrl`; legacy local fixture aliases `publicFileUrl` and `url` are still accepted for older
  recorded runtime fixtures. In live-hybrid mode, a cold reverse fulfillment order is hydrated with a query before the
  mutation is staged. Typed-but-missing orders return `NOT_FOUND`; wrong resource types return Shopify's top-level
  `RESOURCE_NOT_FOUND`; and missing, unrelated, duplicate, or over-quantity line references return the captured payload
  errors without allocating IDs, changing staged state, or appending a commit-log entry. Submitted GIDs are never used to
  fabricate relationship records.
- `reverseDeliveryShippingUpdate` query-hydrates a cold existing reverse delivery in live-hybrid mode, stages the tracking
  update locally, and makes the delivery and its reverse fulfillment order available through top-level and generic node
  reads. Missing typed delivery IDs return the captured `NOT_FOUND` payload; wrong resource types return a top-level
  `RESOURCE_NOT_FOUND`. Rejections leave staged state and the ordered commit log unchanged.
- `reverseFulfillmentOrderDispose` resolves every referenced line and location before staging. Cold inputs are fetched in
  one deduplicated, query-only `nodes(ids:)` request. Missing locations return `NOT_FOUND`, wrong resource types return a
  top-level `RESOURCE_NOT_FOUND`, custom-line `RESTOCKED` remains invalid, and duplicate, over-quantity, or unprovable
  multi-order inputs fail atomically. A valid cold line is stored as an authoritative local overlay, including disposition
  and location data, so `node(id:)` reads and state dump/restore preserve the disposal.
- Supported return mutations are handled locally in snapshot mode and for local/synthetic orders in live-hybrid mode.
  They do not call upstream Shopify at runtime.
- Validation branches for unknown orders, unknown fulfillment line items, invalid quantities, and unknown returns return
  local `userErrors` and do not append staged commit-log entries.
- Return lifecycle staging is covered by Rust runtime tests rather than local-runtime parity evidence. The runtime tests
  create fulfilled local order graphs, then cover `returnCreate`, `returnClose`, `returnReopen`, `returnCancel`,
  downstream `return(id:)` and `Order.returns` reads, `returnRequest`, and missing fulfillment-line-item validation. The
  live reverse-logistics introspection fixture remains schema evidence for root availability and blocked roots; it is not
  behavior payload evidence for the local lifecycle replay.
- Executable parity covers `returnableFulfillments`, `returnCalculate`, `returnApproveRequest`, `returnDeclineRequest`,
  `removeFromReturn`, `returnProcess`, reverse delivery creation/update, reverse fulfillment disposal, and downstream
  reverse logistics reads. Public 2026-04 evidence covers cold valid create/update/dispose hydration; missing and wrong-type
  reverse fulfillment orders, delivery lines, deliveries, and locations; unrelated and duplicate delivery lines;
  over-quantity delivery creation; empty `reverseFulfillmentOrderDispose` inputs; custom-line `RESTOCKED` rejection;
  multiple reverse fulfillment order rejection; valid `NOT_RESTOCKED` disposal; and downstream disposition readback.
  The stricter local over-disposal guard remains runtime-tested because the public custom-line capture accepted that probe.
- `return-decline-request-validation` covers public-schema `returnDeclineRequest` validation for invalid
  `ReturnDeclineReason` variables and non-public `tmp_notify_customer` payloads, comparing proxy responses against the
  live `return-decline-request-validation.json` fixture. `return-request-decline-local-staging` covers live-backed
  request-to-decline staging aliases and downstream state with the `returnApprove-decline-state-preconditions.json`
  fixture.
- Rust runtime tests cover invalid `returnDeclineRequest` decline reasons and invalid
  `tmp_notify_customer.email_address` notification payloads against local staged return state. Public Admin GraphQL
  evidence for the exposed `ReturnDeclineReason` enum and the current public-schema `tmp_notify_customer` boundary is
  recorded separately in `return-decline-request-validation.json`.
- Live 2026-04 parity covers return quantity validation: existing `OPEN` returns consuming part of the fulfilled
  quantity make over-cap `returnRequest` and `returnCreate` calls return `Return line item has an invalid quantity.`
  with root-specific field paths, while zero and over-line `removeFromReturn` quantities return Shopify's captured
  `GREATER_THAN`/`INVALID` userError shapes.
- Executable parity covers request approval, empty reverse-delivery line expansion, `ReverseDeliveryLabelInput.fileUrl`,
  shipping update, reverse-fulfillment disposal, return processing, and downstream reads from staged return and
  reverse-logistics records. Exchange processing, carrier label creation, notification sends, refund transfers, duties,
  and inventory/location movement remain explicit unsupported fidelity gaps.
- Executable parity covers `returnClose`, `returnReopen`, `returnCancel`, and `removeFromReturn` status preconditions,
  success transitions, idempotent no-op branches, remove-from-closed-return rejection/readback, and processed-return
  cancel rejection in
  `config/parity-specs/orders/returnClose-Reopen-Cancel-state-preconditions.json`.
- Executable parity covers `Order.returnStatus` aggregation across zero, requested, open, mixed requested/open/declined,
  closed/reopened, processed, canceled-only, declined-only, and removed-line states in
  `config/parity-specs/orders/order-return-status-lifecycle.json`.
- Executable parity covers public 2026-04 `returnCreate` with `ReturnInput.returnShippingFee` and deprecated
  `unprocessed`, plus read-after-write `Return.returnShippingFees` / `Order.returns.returnShippingFees`, in
  `config/parity-specs/orders/return-shipping-fee-recorded.json`. The same fixture records the current public-schema
  boundary: `returnDelivery`, `note`, `refundIntent`, `locationId`, and `retailAttribution` are rejected during GraphQL
  variable coercion on this Admin API version, so those hidden/internal fields are backed by local runtime tests rather
  than public success-path parity.

### Boundaries

- The singular `returnableFulfillment` root remains registry-only/unsupported.
- Broader `returnCalculate` fidelity for exchange lines, return shipping fee calculations, discounts, and error behavior
  remains unsupported beyond the captured fulfilled-line subtotal, tax, and percentage-restocking-fee slice.
- Exchange-line removal/processing is blocked on captured exchange item semantics and downstream order/return effects.
- Refund duties, refund shipping, financial transfers, notification sends, carrier labels, and inventory/location movement
  remain local-only boundaries until live success-path captures prove the side effects and cleanup path.
- `returnApprove` and `returnDecline` are not exposed by live 2025-01 or 2026-04 Admin GraphQL root introspection on the
  current conformance shop.

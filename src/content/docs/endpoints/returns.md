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

- `return(id:)` resolves staged return records and returns `null` for missing IDs in snapshot mode. Nested
  `Order.returns` reads are derived from the same staged return records through the order-to-return index, so top-level
  and nested reads observe the same staged status, quantity, shipping-fee, and line-item state.
- `returnCreate` stages a local `OPEN` return for the submitted order and fulfillment line items. The local return stores
  a stable synthetic Return ID, ReturnLineItem IDs, status, name, quantity, reason, reason note, order linkage, return
  shipping fee, and reverse-fulfillment-order references for returned quantities. The original raw mutation is retained
  in the meta log for explicit commit replay.
- `returnRequest` stages the same order-backed shape with status `REQUESTED` and uses the same already-returned quantity
  cap as `returnCreate`. Public `notifyCustomer` input is accepted, but the public Admin schema does not expose the
  non-public `tmp_notify_customer` payload; variable-bound requests that include it fail with top-level
  `INVALID_VARIABLE` before local staging. The proxy does not send notification side effects.
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
- `returnProcess` updates processed quantities for local return line items and closes the return for subsequent reads when
  all lines are processed. Captured 2026-04 behavior returns the mutation payload with status `OPEN`, then exposes
  `CLOSED` on immediate downstream `return(id:)` / `Order.returns` reads; local staging mirrors that split. Refund duties,
  refund shipping, financial transfers, exchange processing, and notification behavior are not emulated beyond local
  metadata and validation boundaries.
- `reverseDeliveryCreateWithShipping` builds staged reverse delivery lines from `reverseDeliveryLineItems`. Explicit
  entries preserve input order, quantity, and the requested reverse fulfillment order line item; an empty input follows
  Shopify's documented expansion rule and creates one local reverse delivery line for each reverse fulfillment order line
  at that line's total quantity. `ReverseDeliveryLabelInput` accepts Shopify's `fileUrl` field and preserves it as the
  downstream `label.publicFileUrl`; legacy local fixture aliases `publicFileUrl` and `url` are still accepted for older
  recorded runtime fixtures.
- Supported return mutations are handled locally in snapshot mode and for local/synthetic orders in live-hybrid mode.
  They do not call upstream Shopify at runtime.
- Validation branches for unknown orders, unknown fulfillment line items, invalid quantities, and unknown returns return
  local `userErrors` and do not append staged commit-log entries.
- `return-lifecycle-local-staging` is generic strict parity. The parity replay
  seeds a fulfilled order graph, then compares `returnCreate`, `returnClose`, `returnReopen`, `returnCancel`,
  downstream `return(id:)` and `Order.returns` reads, `returnRequest`, and a missing fulfillment-line-item validation
  branch against an explicit local-runtime fixture. The live reverse-logistics introspection fixture remains schema
  evidence for root availability and blocked roots; it is not the behavior payload for the strict local lifecycle replay.
- Executable parity covers `returnApproveRequest`, `returnDeclineRequest`, `removeFromReturn`, `returnProcess`, reverse
  delivery creation/update, reverse fulfillment disposal, and downstream reverse logistics reads. Public 2026-04 evidence
  covers empty `reverseFulfillmentOrderDispose` inputs, custom-line `RESTOCKED` rejection, multiple reverse fulfillment
  order rejection, valid `NOT_RESTOCKED` disposal, and downstream disposition readback. Unknown-line and over-disposal
  guardrails remain covered by focused runtime tests because the public custom-line capture did not reject those probes.
- `return-decline-request-validation` covers public-schema `returnDeclineRequest` validation for invalid
  `ReturnDeclineReason` variables and non-public `tmp_notify_customer` payloads, comparing proxy responses against the
  live `return-decline-request-validation.json` fixture. `return-request-decline-local-staging` covers successful
  request-to-decline local staging and downstream state.
- Executable local-runtime parity covers return quantity validation:
  `config/parity-specs/orders/returnRequest-quantity-cap.json` hydrates an order with an existing `OPEN` return consuming
  part of the fulfilled quantity and verifies over-cap `returnRequest` and `returnCreate` calls return a quantity userError
  instead of staging a second return, while `config/parity-specs/orders/removeFromReturn-quantity-validation.json` verifies
  zero and over-line removal quantities return `INVALID` quantity userErrors.
- Executable parity covers request approval, empty reverse-delivery line expansion, `ReverseDeliveryLabelInput.fileUrl`,
  shipping update, reverse-fulfillment disposal, return processing, and downstream reads from staged return and
  reverse-logistics records. Exchange processing, carrier label creation, notification sends, refund transfers, duties,
  and inventory/location movement remain explicit unsupported fidelity gaps.
- Executable parity covers `returnClose`, `returnReopen`, `returnCancel`, and `removeFromReturn` status preconditions,
  success transitions, idempotent no-op branches, remove-from-closed-return rejection/readback, and processed-return
  cancel rejection in
  `config/parity-specs/orders/returnClose-Reopen-Cancel-state-preconditions.json`.
- Executable parity covers public 2026-04 `returnCreate` with `ReturnInput.returnShippingFee` and deprecated
  `unprocessed`, plus read-after-write `Return.returnShippingFees` / `Order.returns.returnShippingFees`, in
  `config/parity-specs/orders/return-shipping-fee-recorded.json`. The same fixture records the current public-schema
  boundary: `returnDelivery`, `note`, `refundIntent`, `locationId`, and `retailAttribution` are rejected during GraphQL
  variable coercion on this Admin API version, so those hidden/internal fields are backed by local runtime tests rather
  than public success-path parity.

### Unsupported, registry-only, and validation-only coverage

- `returnCalculate` is blocked on calculation parity for restocking fees, exchange lines, return shipping fees, taxes,
  discounts, and error behavior.
- `returnableFulfillment` and `returnableFulfillments` are blocked on returnability eligibility and line-item quantity
  parity over fulfilled orders.
- Exchange-line removal/processing is blocked on captured exchange item semantics and downstream order/return effects.
- Refund duties, refund shipping, financial transfers, notification sends, carrier labels, and inventory/location movement
  remain local-only boundaries until live success-path captures prove the side effects and cleanup path.
- `returnApprove` and `returnDecline` are not exposed by live 2025-01 or 2026-04 Admin GraphQL root introspection on the
  current conformance shop.

### Evidence and validation

- Executable parity: `config/parity-specs/orders/return-lifecycle-local-staging.json`
- Executable parity:
  `config/parity-specs/orders/return-reverse-logistics-local-staging.json`,
  `config/parity-specs/orders/return-request-decline-local-staging.json`, and
  `config/parity-specs/orders/removeFromReturn-local-staging.json`
- Return decline/request validation parity:
  `config/parity-specs/orders/return-request-decline-local-staging.json`, backed by
  `fixtures/conformance/local-runtime/2026-04/orders/return-lifecycle-local-staging.json` and public schema evidence in
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/return-decline-request-validation.json`.
- Quantity validation parity:
  `config/parity-specs/orders/returnRequest-quantity-cap.json` and
  `config/parity-specs/orders/removeFromReturn-quantity-validation.json`
- `config/parity-specs/orders/return-reverse-logistics-local-staging.json` exercises empty
  `reverseDeliveryLineItems` replay and `fileUrl` label input normalization.
  `config/parity-specs/orders/return-reverse-logistics-non-recording-operation-name.json`
  replays the same store-backed mutation/read flow with unrelated client operation names to guard against
  document-marker dispatch. It also adds live recorded parity in
  `config/parity-specs/orders/return-reverse-logistics-recorded.json` backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-recorded.json`; the live
  recorder creates one two-line return for empty-array expansion and a second two-line return for explicit multi-line
  input so both reverse delivery payloads are captured against real reverse fulfillment order state.
- Reverse fulfillment disposal validation parity:
  `config/parity-specs/orders/return-reverse-logistics-dispose-validation.json`, backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-dispose-validation.json`
  and captured with `scripts/capture-return-reverse-logistics-dispose-validation-conformance.mts`.
- Return status precondition parity:
  `config/parity-specs/orders/returnClose-Reopen-Cancel-state-preconditions.json`, backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/returnClose-Reopen-Cancel-state-preconditions.json`.
- Return approve/decline state-precondition parity:
  `config/parity-specs/orders/returnApprove-decline-state-preconditions-live.json`, backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/returnApprove-decline-state-preconditions.json`,
  captures invalid-state and not-found userError shapes for `returnApproveRequest` and `returnDeclineRequest`.
- Return shipping fee parity:
  `config/parity-specs/orders/return-shipping-fee-recorded.json`, backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-shipping-fee-recorded.json`.
- Return reason validation parity:
  `config/parity-specs/orders/return-reason-validation.json`, backed by
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reason-validation.json`.
- No-side-effect schema evidence: live 2025-01 and 2026-04 conformance introspection captured root signatures for
  `return`, `returnCalculate`, `returnableFulfillment(s)`, `reverseDelivery`, `reverseFulfillmentOrder`, and the listed
  mutation payloads.

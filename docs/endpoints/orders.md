# Orders Endpoint Group

The orders group is fully implemented in the operation registry. It covers orders, draft orders, order lifecycle mutations, fulfillment slices, refunds, and order editing.

## Supported roots

Overlay reads:

- `order`
- `orders`
- `ordersCount`
- `draftOrder`
- `draftOrders`
- `draftOrdersCount`

Local staged mutations:

- `orderUpdate`
- `orderClose`
- `orderOpen`
- `orderMarkAsPaid`
- `orderCreateManualPayment`
- `orderCapture`
- `transactionVoid`
- `orderCreateMandatePayment`
- `orderCustomerSet`
- `orderCustomerRemove`
- `orderInvoiceSend`
- `taxSummaryCreate`
- `orderCancel`
- `fulfillmentCreate`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`
- `fulfillmentOrderHold`
- `fulfillmentOrderReleaseHold`
- `fulfillmentOrderMove`
- `fulfillmentOrderReportProgress`
- `fulfillmentOrderOpen`
- `fulfillmentOrderCancel`
- `orderCreate`
- `refundCreate`
- `draftOrderCreate`
- `draftOrderComplete`
- `draftOrderUpdate`
- `draftOrderDuplicate`
- `draftOrderDelete`
- `draftOrderInvoiceSend`
- `draftOrderCreateFromOrder`
- `orderEditBegin`
- `orderEditAddVariant`
- `orderEditSetQuantity`
- `orderEditCommit`

## Behavior notes

- Order and draft-order reads use the shared Shopify-style search parser for catalog, count, invalid-query, and pagination slices covered by parity fixtures.
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus order-backed creation from local fulfillment orders, while `fulfillmentEventCreate`, `fulfillmentTrackingInfoUpdate`, and `fulfillmentCancel` update seeded or staged fulfillment records locally. Fulfillment-order request/cancellation roots stage against order-backed fulfillment orders: submit can split partial quantities into submitted/unsubmitted fulfillment orders, accept/reject fulfillment requests update request status, and cancellation submit/accept/reject preserves merchant request history. HAR-234 fulfillment-order lifecycle support stages held, released, moved, progress-reported, reopened, and cancelled fulfillment orders from the same order-owned fulfillment-order graph and keeps downstream nested/top-level reads consistent without upstream writes.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Staged fulfillment events are visible through both top-level `fulfillment(id:)` and nested `Order.fulfillments.events`, and tracking/cancel updates preserve event history and shipment milestone fields. Staged fulfillment-order request statuses and merchant request messages are visible through `fulfillmentOrder`, `fulfillmentOrders`, `assignedFulfillmentOrders`, and nested `Order.fulfillmentOrders`; no fulfillment-service notification callbacks are invoked. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- `draftOrderInvoiceSend` is treated as an outbound email side-effect root. Runtime support never sends the mutation upstream or emails a customer; it appends the original raw mutation to the meta log for explicit commit replay. Safe captured 2026-04 branches are mirrored locally for missing/unknown/deleted draft IDs, no-recipient drafts (`To can't be blank`), and completed no-recipient drafts (`To can't be blank` plus the already-paid error). For open local drafts with a recipient, the proxy returns an explicit local userError instead of pretending the invoice email was delivered.
- `draftOrder(id:)` returns `null` for absent IDs. The `draft-order-by-id-not-found-read` parity scenario captures this missing-id behavior without relying on live upstream passthrough.
- Draft-order detail parity now compares the captured `draftOrder(id:)` payload as a strict object for the selected phone, timestamp, subtotal/total, line-item unit-price, SKU/nullability, address, shipping-line, custom-attribute, discount, tax-exemption, and payment-terms fields. The current live detail capture returns `paymentTerms: null` for the merchant-realistic draft without terms and preserves empty line-item structures such as `customAttributes: []`, `appliedDiscount: null`, and variant-backed SKU/title nullability.
- Shopify normalizes draft-order shipping lines created with `priceWithCurrency` to `code: "custom"`, `custom: true`, and matching `originalPriceSet` / `discountedPriceSet` shop-money amounts. The local serializer mirrors that shape and uses `null` for absent shipping lines after duplicate/create-from-order flows.
- The captured DraftOrder detail read surface does not select `note`; local mutation payloads and downstream local reads still preserve staged note values, but live detail parity keeps note out of the strict object contract until Shopify exposes a selectable note field for this surface.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.
- Shipping refunds staged through `refundCreate(input.shipping)` are retained on the refund record and rolled into downstream `Order.totalRefundedShippingSet`; the broader refund amount still follows the captured transaction total / line-item plus shipping fallback behavior.
- Order shipping-line tax lines contribute to total tax calculations for staged `orderCreate`, and staged shipping lines remain visible through downstream `Order.shippingLines` reads.
- State-specific lifecycle/customer validation is modeled locally for the staged order roots covered by HAR-278. Repeated `orderClose`, repeated `orderOpen`, `orderOpen` after cancellation, repeated `orderMarkAsPaid`, unknown or duplicate `orderCustomerSet`, empty `orderCustomerRemove`, and repeated `orderCancel` return concrete `userErrors` and do not mutate downstream order reads, meta state, or the mutation log.
- `orderCustomerSet` and `orderCustomerRemove` own the order-domain relationship on `OrderRecord.customer`. Customer reads consume that normalized relationship only for the immediate `Customer.orders` connection; captured HAR-288 evidence showed the customer-owned `numberOfOrders`, `amountSpent`, and `lastOrder` fields do not update in the immediate read-after-set/remove slice.
- HAR-278 order lifecycle/payment guardrails append mutation-log entries only when the handler stages a successful local effect. The scoped validation branches with `userErrors` or top-level GraphQL `errors`, including access-denied branches such as `orderCreateManualPayment` and `taxSummaryCreate`, leave the mutation log unchanged; other established safety handlers such as draft-order invoice send still retain their existing observability log entries.
- Create-time validation coverage now includes executable parity specs for `orderCreate` no-line-items and a grouped `draftOrderCreate` validation matrix. Rejected create requests return captured mutation-scoped `userErrors` locally without staging orders/draft orders or appending staged-write log entries.
- Captured `draftOrderCreate` validation branches include no line items, unknown variant, missing custom title, zero quantity, payment terms without a template id, negative custom line price, past reserve-inventory timestamp, and invalid email. Fresh 2026-04 probes also showed Shopify accepts variant-backed draft lines even when custom title/originalUnitPrice fields are present, accepts missing custom originalUnitPrice as a zero-price line, and accepts shippingLine without a title; those combinations are intentionally not local validation failures.
- Broader direct `orderCreate` create-time validation remains partially blocked on this host by Shopify's `Too many attempts. Please try again later.` order-create throttle under 2026-04. Keep the existing no-line-items parity fixture as executable evidence, and do not expand direct-order business validation branches without a fresh successful capture.
- Order payment transaction flows stage locally for in-memory orders. `orderCapture` turns successful authorization transactions into `CAPTURE` transactions, updates `capturable`, `totalCapturable`, `totalCapturableSet`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `displayFinancialStatus`, `paymentGatewayNames`, and records synthetic `paymentId` / `paymentReferenceId` values. Partial captures keep the remaining authorization capturable; final captures close the remaining capturable balance.
- `orderCapture` validation for over-capture, non-positive amounts, missing transactions, and no-longer-capturable authorizations returns local `userErrors` without mutating order financial state or logs.
- `transactionVoid` creates a `VOID` transaction for uncaptured authorization transactions and clears downstream capturable state. Missing, invalid, already-voided, and already-captured authorization requests return local `userErrors` without passthrough, downstream order changes, or mutation-log entries.
- `orderCreateMandatePayment` creates a completed local `Job`, a stable session-scoped `paymentReferenceId`, and a `MANDATE_PAYMENT` transaction. Reusing the same order/idempotency-key pair returns the original job/reference result and does not duplicate the transaction. Missing idempotency keys and non-positive amounts return local `userErrors` without contacting payment services.
- The local payment implementation does not contact real payment gateways and intentionally limits itself to local/synthetic orders and transaction branches covered by runtime tests or safe documentation evidence. Broader Plus-only and permission-specific mandate/capture branches still require live conformance evidence before they should be expanded.
- `orderInvoiceSend` is handled locally for existing orders and does not send upstream invoice email. Safe live success recapture is side-effect-heavy and remains blocked unless a no-recipient disposable capture path is available; local runtime coverage verifies no upstream/email call is made. `taxSummaryCreate` mirrors the captured access-denied branch without invoking tax calculation services until tax-app semantics can be safely captured.

## Validation anchors

- Order reads: `tests/integration/order-query-shapes.test.ts`
- Order lifecycle, payment, and customer changes: `tests/integration/order-lifecycle-payment-customer-flow.test.ts`
- Order payment transaction changes: `tests/integration/order-payment-transaction-flow.test.ts`
- Order create/update flows: `tests/integration/order-creation-flow.test.ts`, `tests/integration/order-draft-flow.test.ts`
- Draft-order mutation family: `tests/integration/draft-order-mutation-family-flow.test.ts`
- Fulfillments: `tests/integration/order-fulfillment-flow.test.ts`, `tests/integration/order-query-shapes.test.ts`
- Fulfillment-order lifecycle capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/fulfillment-order-lifecycle.json`
- Order editing: `tests/integration/order-edit-flow.test.ts`
- Refunds and shipping-refund aggregates: `tests/integration/order-refund-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/order*.json`, `config/parity-specs/draftOrder*.json`, `config/parity-specs/draftOrders*.json`, `config/parity-specs/fulfillment*.json`, `config/parity-specs/refund*.json`, and matching files under `config/parity-requests/`

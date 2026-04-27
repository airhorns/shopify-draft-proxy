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
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus order-backed creation from local fulfillment orders, while `fulfillmentEventCreate`, `fulfillmentTrackingInfoUpdate`, and `fulfillmentCancel` update seeded or staged fulfillment records locally.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Staged fulfillment events are visible through both top-level `fulfillment(id:)` and nested `Order.fulfillments.events`, and tracking/cancel updates preserve event history and shipment milestone fields. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- `draftOrderInvoiceSend` is treated as an outbound email side-effect root. Runtime support never sends the mutation upstream or emails a customer; it appends the original raw mutation to the meta log for explicit commit replay. Safe captured 2026-04 branches are mirrored locally for missing/unknown/deleted draft IDs, no-recipient drafts (`To can't be blank`), and completed no-recipient drafts (`To can't be blank` plus the already-paid error). For open local drafts with a recipient, the proxy returns an explicit local userError instead of pretending the invoice email was delivered.
- `draftOrder(id:)` returns `null` for absent IDs. The `draft-order-by-id-not-found-read` parity scenario captures this missing-id behavior without relying on live upstream passthrough.
- Draft-order detail parity now compares the captured `draftOrder(id:)` payload as a strict object for the selected phone, timestamp, subtotal/total, line-item unit-price, SKU/nullability, address, shipping-line, custom-attribute, discount, tax-exemption, and payment-terms fields. The current live detail capture returns `paymentTerms: null` for the merchant-realistic draft without terms and preserves empty line-item structures such as `customAttributes: []`, `appliedDiscount: null`, and variant-backed SKU/title nullability.
- Local `Order.paymentTerms` and `DraftOrder.paymentTerms` reads preserve `null` for orders/drafts without terms. When normalized payment terms are present in the local graph, the serializer exposes selected scalar fields plus the nested `paymentSchedules` connection with shared cursor/window/pageInfo handling and schedule money fields (`amount`, `balanceDue`, `totalBalance`). The current runtime tests seed that normalized graph directly because live set-payment-terms success remains blocked by merchant permission on the conformance shop.
- Shopify normalizes draft-order shipping lines created with `priceWithCurrency` to `code: "custom"`, `custom: true`, and matching `originalPriceSet` / `discountedPriceSet` shop-money amounts. The local serializer mirrors that shape and uses `null` for absent shipping lines after duplicate/create-from-order flows.
- The captured DraftOrder detail read surface does not select `note`; local mutation payloads and downstream local reads still preserve staged note values, but live detail parity keeps note out of the strict object contract until Shopify exposes a selectable note field for this surface.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.
- Shipping refunds staged through `refundCreate(input.shipping)` are retained on the refund record and rolled into downstream `Order.totalRefundedShippingSet`; the broader refund amount still follows the captured transaction total / line-item plus shipping fallback behavior.
- Order shipping-line tax lines contribute to total tax calculations for staged `orderCreate`, and staged shipping lines remain visible through downstream `Order.shippingLines` reads.
- Create-time validation coverage now includes executable parity specs for `orderCreate` no-line-items and a grouped `draftOrderCreate` validation matrix. Rejected create requests return captured mutation-scoped `userErrors` locally without staging orders/draft orders or appending staged-write log entries.
- Captured `draftOrderCreate` validation branches include no line items, unknown variant, missing custom title, zero quantity, payment terms without a template id, payment terms with a template id blocked by merchant permission, negative custom line price, past reserve-inventory timestamp, and invalid email. Fresh 2026-04 probes also showed Shopify accepts variant-backed draft lines even when custom title/originalUnitPrice fields are present, accepts missing custom originalUnitPrice as a zero-price line, and accepts shippingLine without a title; those combinations are intentionally not local validation failures.
- Broader direct `orderCreate` create-time validation remains partially blocked on this host by Shopify's `Too many attempts. Please try again later.` order-create throttle under 2026-04. Keep the existing no-line-items parity fixture as executable evidence, and do not expand direct-order business validation branches without a fresh successful capture.
- Order payment transaction flows stage locally for in-memory orders. `orderCapture` turns successful authorization transactions into `CAPTURE` transactions, updates `capturable`, `totalCapturable`, `totalCapturableSet`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `displayFinancialStatus`, `paymentGatewayNames`, and records synthetic `paymentId` / `paymentReferenceId` values. Partial captures keep the remaining authorization capturable; final captures close the remaining capturable balance.
- `transactionVoid` creates a `VOID` transaction for uncaptured authorization transactions and clears downstream capturable state. Invalid, already-voided, and already-captured authorization requests return local `userErrors` without passthrough.
- `orderCreateMandatePayment` creates a completed local `Job`, a stable session-scoped `paymentReferenceId`, and a `MANDATE_PAYMENT` transaction. Reusing the same order/idempotency-key pair returns the original job/reference result and does not duplicate the transaction.
- The local payment implementation does not contact real payment gateways and intentionally limits itself to local/synthetic orders and transaction branches covered by runtime tests or safe documentation evidence. Broader Plus-only and permission-specific mandate/capture branches still require live conformance evidence before they should be expanded.

## Validation anchors

- Order reads: `tests/integration/order-query-shapes.test.ts`
- Order lifecycle, payment, and customer changes: `tests/integration/order-lifecycle-payment-customer-flow.test.ts`
- Order payment transaction changes: `tests/integration/order-payment-transaction-flow.test.ts`
- Order create/update flows: `tests/integration/order-creation-flow.test.ts`, `tests/integration/order-draft-flow.test.ts`
- Payment terms reads: `tests/integration/payment-terms-query-shapes.test.ts`
- Draft-order mutation family: `tests/integration/draft-order-mutation-family-flow.test.ts`
- Fulfillments: `tests/integration/order-fulfillment-flow.test.ts`, `tests/integration/order-query-shapes.test.ts`
- Order editing: `tests/integration/order-edit-flow.test.ts`
- Refunds and shipping-refund aggregates: `tests/integration/order-refund-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/order*.json`, `config/parity-specs/draftOrder*.json`, `config/parity-specs/draftOrders*.json`, `config/parity-specs/fulfillment*.json`, `config/parity-specs/refund*.json`, and matching files under `config/parity-requests/`

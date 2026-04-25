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
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus order-backed creation from local fulfillment orders, while `fulfillmentEventCreate`, `fulfillmentTrackingInfoUpdate`, and `fulfillmentCancel` update seeded or staged fulfillment records locally. HAR-234 fulfillment-order lifecycle support stages held, released, moved, progress-reported, reopened, and cancelled fulfillment orders from the order-owned fulfillment-order graph and keeps downstream nested/top-level reads consistent without upstream writes.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Staged fulfillment events are visible through both top-level `fulfillment(id:)` and nested `Order.fulfillments.events`, and tracking/cancel updates preserve event history and shipment milestone fields. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- `draftOrder(id:)` returns `null` for absent IDs. The `draft-order-by-id-not-found-read` parity scenario captures this missing-id behavior without relying on live upstream passthrough.
- Draft-order detail parity now compares the captured `draftOrder(id:)` payload as a strict object for the selected phone, timestamp, subtotal/total, line-item unit-price, SKU/nullability, address, shipping-line, custom-attribute, discount, tax-exemption, and payment-terms fields. The current live detail capture returns `paymentTerms: null` for the merchant-realistic draft without terms and preserves empty line-item structures such as `customAttributes: []`, `appliedDiscount: null`, and variant-backed SKU/title nullability.
- Shopify normalizes draft-order shipping lines created with `priceWithCurrency` to `code: "custom"`, `custom: true`, and matching `originalPriceSet` / `discountedPriceSet` shop-money amounts. The local serializer mirrors that shape and uses `null` for absent shipping lines after duplicate/create-from-order flows.
- The captured DraftOrder detail read surface does not select `note`; local mutation payloads and downstream local reads still preserve staged note values, but live detail parity keeps note out of the strict object contract until Shopify exposes a selectable note field for this surface.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.
- Shipping refunds staged through `refundCreate(input.shipping)` are retained on the refund record and rolled into downstream `Order.totalRefundedShippingSet`; the broader refund amount still follows the captured transaction total / line-item plus shipping fallback behavior.
- Order shipping-line tax lines contribute to total tax calculations for staged `orderCreate`, and staged shipping lines remain visible through downstream `Order.shippingLines` reads.
- Order payment transaction flows stage locally for in-memory orders. `orderCapture` turns successful authorization transactions into `CAPTURE` transactions, updates `capturable`, `totalCapturable`, `totalCapturableSet`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `displayFinancialStatus`, `paymentGatewayNames`, and records synthetic `paymentId` / `paymentReferenceId` values. Partial captures keep the remaining authorization capturable; final captures close the remaining capturable balance.
- `transactionVoid` creates a `VOID` transaction for uncaptured authorization transactions and clears downstream capturable state. Invalid, already-voided, and already-captured authorization requests return local `userErrors` without passthrough.
- `orderCreateMandatePayment` creates a completed local `Job`, a stable session-scoped `paymentReferenceId`, and a `MANDATE_PAYMENT` transaction. Reusing the same order/idempotency-key pair returns the original job/reference result and does not duplicate the transaction.
- The local payment implementation does not contact real payment gateways and intentionally limits itself to local/synthetic orders and transaction branches covered by runtime tests or safe documentation evidence. Broader Plus-only and permission-specific mandate/capture branches still require live conformance evidence before they should be expanded.

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

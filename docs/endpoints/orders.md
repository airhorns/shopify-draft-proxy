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
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus the happy path, while `fulfillmentTrackingInfoUpdate` and `fulfillmentCancel` update seeded fulfillment records locally.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`. The executable conformance anchors for the first begin/add/set/commit lifecycle are the captured workflow specs `orderEditExistingOrder-happy-path`, `orderEditExistingOrder-validation`, and `orderEditExistingOrder-zero-removal`; do not reintroduce the older single-root access-scope parity plans as discovery blockers.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.
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
- Order editing: `tests/integration/order-edit-flow.test.ts`
- Refunds: `tests/integration/order-refund-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/order*.json`, `config/parity-specs/draftOrder*.json`, `config/parity-specs/draftOrders*.json`, `config/parity-specs/fulfillment*.json`, `config/parity-specs/refund*.json`, and matching files under `config/parity-requests/`. For order editing, prefer the `orderEditExistingOrder-*` workflow specs plus the missing-id validation slices over single-root planned placeholders.

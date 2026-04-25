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
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.

## Validation anchors

- Order reads: `tests/integration/order-query-shapes.test.ts`
- Order lifecycle, payment, and customer changes: `tests/integration/order-lifecycle-payment-customer-flow.test.ts`
- Order create/update flows: `tests/integration/order-creation-flow.test.ts`, `tests/integration/order-draft-flow.test.ts`
- Draft-order mutation family: `tests/integration/draft-order-mutation-family-flow.test.ts`
- Fulfillments: `tests/integration/order-fulfillment-flow.test.ts`
- Order editing: `tests/integration/order-edit-flow.test.ts`
- Refunds: `tests/integration/order-refund-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/order*.json`, `config/parity-specs/draftOrder*.json`, `config/parity-specs/draftOrders*.json`, `config/parity-specs/fulfillment*.json`, `config/parity-specs/refund*.json`, and matching files under `config/parity-requests/`

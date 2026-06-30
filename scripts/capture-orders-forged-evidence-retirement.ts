/* oxlint-disable no-console -- CLI registry script reports retired evidence paths. */

const retiredEvidencePaths = [
  'config/parity-specs/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.json',
  'config/parity-specs/orders/draftOrderBulkTag-validation.json',
  'config/parity-specs/orders/draftOrderComplete-non-recording-operation-name.json',
  'config/parity-specs/orders/draftOrderComplete-paymentGateway-paths.json',
  'config/parity-specs/orders/draftOrderComplete-stages-resulting-order.json',
  'config/parity-specs/orders/draftOrderInvoiceSend-invoice-errors.json',
  'config/parity-specs/orders/money-bag-presentment-parity.json',
  'config/parity-specs/orders/order-edit-residual-local-staging.json',
  'config/parity-specs/orders/order-payment-mandate-local-staging.json',
  'config/parity-specs/orders/order-payment-transaction-local-staging.json',
  'config/parity-specs/orders/order-payment-transaction-non-recording-operation-name.json',
  'config/parity-specs/orders/order-payment-transaction-void-local-staging.json',
  'config/parity-specs/orders/orderCancel-state-transitions.json',
  'config/parity-specs/orders/orderDelete-cascade-and-deletability.json',
  'config/parity-specs/orders/removeFromReturn-local-staging.json',
  'config/parity-specs/orders/removeFromReturn-quantity-validation.json',
  'config/parity-specs/orders/return-lifecycle-local-staging.json',
  'config/parity-specs/orders/return-request-decline-local-staging.json',
  'config/parity-specs/orders/return-reverse-logistics-local-staging.json',
  'config/parity-specs/orders/return-reverse-logistics-non-recording-operation-name.json',
  'config/parity-specs/orders/returnApprove-decline-state-preconditions.json',
  'config/parity-specs/orders/returnRequest-quantity-cap.json',
  'config/parity-specs/payments/order_create_mandate_payment_auto_capture_false.json',
  'config/parity-specs/payments/order_create_mandate_payment_idempotency.json',
  'config/parity-specs/payments/order_create_mandate_payment_missing_mandate.json',
  'config/parity-specs/payments/order_create_mandate_payment_reference_id_format.json',
  'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-full-parity.json',
  'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-over-refund-user-errors.json',
  'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-partial-shipping-restock-parity.json',
];

console.log(
  JSON.stringify(
    {
      ok: true,
      action: 'retired-forged-orders-parity-evidence',
      retiredEvidencePaths,
    },
    null,
    2,
  ),
);

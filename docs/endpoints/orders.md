# Orders Endpoint Group

The orders group is broadly implemented in the operation registry, with explicit blockers documented for roots that still lack enough Shopify evidence. It covers orders, draft orders, order lifecycle mutations, fulfillment slices, refunds, and order editing.

## Current support and limitations

### Supported roots

Overlay reads:

- `order`
- `return`
- `orders`
- `ordersCount`
- `abandonedCheckouts`
- `abandonedCheckoutsCount`
- `abandonment`
- `abandonmentByAbandonedCheckoutId`
- `draftOrder`
- `draftOrders`
- `draftOrdersCount`
- `draftOrderAvailableDeliveryOptions`
- `draftOrderSavedSearches`

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
- `orderDelete`
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
- `returnCreate`
- `returnRequest`
- `returnCancel`
- `returnClose`
- `returnReopen`
- `abandonmentUpdateActivitiesDeliveryStatuses`
- `draftOrderCreate`
- `draftOrderComplete`
- `draftOrderUpdate`
- `draftOrderDuplicate`
- `draftOrderDelete`
- `draftOrderBulkAddTags`
- `draftOrderBulkRemoveTags`
- `draftOrderBulkDelete`
- `draftOrderCalculate`
- `draftOrderInvoicePreview`
- `draftOrderInvoiceSend`
- `draftOrderCreateFromOrder`
- `orderEditBegin`
- `orderEditAddVariant`
- `orderEditAddCustomItem`
- `orderEditAddLineItemDiscount`
- `orderEditRemoveDiscount`
- `orderEditSetQuantity`
- `orderEditCommit`

### Declared Gaps

- `orderRiskAssessmentCreate` is a registry-only HAR-316 scaffold. The 2025-01 `finance-risk-access-read` capture records only an unknown-order validation branch returning `userErrors[{ field: ["orderRiskAssessmentInput", "orderId"], code: "NOT_FOUND" }]`. Do not mark this mutation supported until local risk assessment staging, downstream order risk reads, Shopify-like userErrors, and raw commit replay are modeled end to end.

### Behavior notes

- Order and draft-order reads use the shared Shopify-style search parser for catalog, count, invalid-query, and pagination slices covered by parity fixtures.
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus order-backed creation from local fulfillment orders, while `fulfillmentEventCreate`, `fulfillmentTrackingInfoUpdate`, and `fulfillmentCancel` update seeded or staged fulfillment records locally. Fulfillment-order request/cancellation roots stage against order-backed fulfillment orders: submit can split partial quantities into submitted/unsubmitted fulfillment orders, accept/reject fulfillment requests update request status, and cancellation submit/accept/reject preserves merchant request history. HAR-234 fulfillment-order lifecycle support stages held, released, moved, progress-reported, reopened, and cancelled fulfillment orders from the same order-owned fulfillment-order graph and keeps downstream nested/top-level reads consistent without upstream writes.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Staged fulfillment events are visible through both top-level `fulfillment(id:)` and nested `Order.fulfillments.events`, and tracking/cancel updates preserve event history and shipment milestone fields. Staged fulfillment-order request statuses and merchant request messages are visible through `fulfillmentOrder`, `fulfillmentOrders`, `assignedFulfillmentOrders`, and nested `Order.fulfillmentOrders`; no fulfillment-service notification callbacks are invoked. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay.
- `draftOrderSavedSearches` mirrors the captured default draft-order saved searches as a local connection. Those saved-search IDs can drive local `draftOrderBulk*` target selection through their captured query strings.
- `draftOrderAvailableDeliveryOptions` currently mirrors the captured no-data helper shape: empty shipping/local-delivery/local-pickup arrays and empty `pageInfo`. Non-empty delivery-rate modeling remains future work until delivery-profile-backed draft-order evidence exists.
- `draftOrderBulkAddTags`, `draftOrderBulkRemoveTags`, and `draftOrderBulkDelete` stage against the effective local draft-order set selected by `ids`, `search`, or captured draft-order saved-search query. Downstream `draftOrder(id:)` and draft-order catalog reads observe tag changes and deletions immediately; the returned `Job` keeps Shopify's captured async `done: false` payload shape even though the proxy applies the local effect synchronously.
- `draftOrderCalculate` evaluates a `DraftOrderInput` through the local draft-order pricing model without staging a draft order or sending a mutation upstream. It covers captured-safe totals, line item prices, discounts, shipping totals, empty shipping-rate lists, and selected `CalculatedDraftOrder` scalar/list fields.
- `draftOrderInvoicePreview` returns deterministic local preview subject/html for staged draft orders and never sends email or writes upstream. It mirrors the safe preview contract enough for tests that need a payload before deciding whether to send an invoice.
- The `draft-order-invoice-send-safety` parity fixture is executable generic parity coverage rather than capture-only evidence. The runner replays the captured unknown-id, deleted-draft, open no-recipient, and completed no-recipient validation branches through the local proxy with strict JSON comparison while seeding only the disposable captured setup draft states; recipient-backed invoice sends remain runtime-blocked to avoid customer-visible email.
- Abandoned checkout reads are modeled for snapshot/local state. Empty `abandonedCheckouts` returns an empty connection with false/null `pageInfo`, `abandonedCheckoutsCount` returns `{ count: 0, precision: "EXACT" }`, and missing `abandonment` / `abandonmentByAbandonedCheckoutId` lookups return `null`, matching the 2026-04-27 live capture against `harry-test-heelo.myshopify.com` on Admin GraphQL `2025-01`.
- Representative non-empty abandoned checkout and abandonment reads serialize from seeded normalized records. Local `abandonedCheckouts(query:)` and `abandonedCheckoutsCount(query:)` use the shared Shopify search helpers for the documented `id`, `created_at`, `updated_at`, `status`, `recovery_state`, `email_state`, and default text/title slices. The live conformance store had no abandoned checkout records during HAR-300, so non-empty runtime coverage is schema/introspection-backed rather than a live non-empty fixture. Future work should replace or supplement that seeded proof when a disposable store can produce real abandoned checkout data.
- `abandonmentUpdateActivitiesDeliveryStatuses` is local-only for seeded/snapshot abandonment records. Unknown IDs mirror the captured safe payload `abandonment: null` plus `userErrors[{ field: ["abandonmentId"], message: "abandonment_not_found" }]`. Known local records update the in-memory delivery activity map, surface `emailState` / `emailSentAt` changes on downstream local reads, append the original raw mutation to the meta log, and never send the runtime mutation to Shopify.
- `draftOrderInvoiceSend` is treated as an outbound email side-effect root. Runtime support never sends the mutation upstream or emails a customer; it appends the original raw mutation to the meta log for explicit commit replay. Safe captured 2026-04 branches are mirrored locally for missing/unknown/deleted draft IDs, no-recipient drafts (`To can't be blank`), and completed no-recipient drafts (`To can't be blank` plus the already-paid error). For open local drafts with a recipient, the proxy returns an explicit local userError instead of pretending the invoice email was delivered.
- `draftOrderTag` remains an explicit blocker rather than implemented support. HAR-318 live probing showed raw tag strings fail ID validation and guessed `gid://shopify/DraftOrderTag/<tag>` IDs return `null`; no exposed catalog in the current evidence produced a valid `DraftOrderTag` ID. The runtime can synthesize local staged tag IDs for internal helper reads, but the registry keeps the root unimplemented until a valid-ID capture exists.
- `draftOrder(id:)` returns `null` for absent IDs. The `draft-order-by-id-not-found-read` parity scenario captures this missing-id behavior without relying on live upstream passthrough.
- Draft-order detail parity now compares the captured `draftOrder(id:)` payload as a strict object for the selected phone, timestamp, subtotal/total, line-item unit-price, SKU/nullability, address, shipping-line, custom-attribute, discount, tax-exemption, and payment-terms fields. The current live detail capture returns `paymentTerms: null` for the merchant-realistic draft without terms and preserves empty line-item structures such as `customAttributes: []`, `appliedDiscount: null`, and variant-backed SKU/title nullability.
- Local `Order.paymentTerms` and `DraftOrder.paymentTerms` reads preserve `null` for orders/drafts without terms. When normalized payment terms are present in the local graph, the serializer exposes selected scalar fields plus the nested `paymentSchedules` connection with shared cursor/window/pageInfo handling and schedule money fields (`amount`, `balanceDue`, `totalBalance`). The standalone `paymentTermsCreate`, `paymentTermsUpdate`, and `paymentTermsDelete` roots now stage against this same order/draft-order graph, so downstream reads observe creates, updates, and deletes immediately without runtime Shopify writes. The executable 2026-04 parity fixture uses a disposable draft order and confirms NET `dueAt` derivation, replacement schedule IDs on update, and null downstream terms after delete.
- Shopify normalizes draft-order shipping lines created with `priceWithCurrency` to `code: "custom"`, `custom: true`, and matching `originalPriceSet` / `discountedPriceSet` shop-money amounts. The local serializer mirrors that shape and uses `null` for absent shipping lines after duplicate/create-from-order flows.
- The captured DraftOrder detail read surface does not select `note`; local mutation payloads and downstream local reads still preserve staged note values, but live detail parity keeps note out of the strict object contract until Shopify exposes a selectable note field for this surface.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`. Current local staging covers variant additions, custom item additions, line-item discount add/remove, quantity edits, and shipping-line add/update/remove. The order-edit conformance anchors are the captured existing-order workflow specs, executable single-root begin/add/set/commit parity slices backed by those workflow fixtures, and the HAR-369 local-runtime residual edit/delete spec for roots that must not write to Shopify during runtime.
- `orderDelete` stages an order tombstone locally. Downstream `order(id:)` returns `null`, and local `orders` / `ordersCount` omit the deleted order immediately. Repeated deletes return an `orderId` userError and do not append another staged-write log entry.
- `refundCreate` stages refund records for downstream order reads and covers over-refund user-error behavior through parity fixtures.
- Return staging is order-backed: `returnCreate` and `returnRequest` create local Return rows for known fulfilled order
  line items, while `returnCancel`, `returnClose`, and `returnReopen` update local return status. Top-level
  `return(id:)` and nested `Order.returns` read from the same order graph. Broader calculation, returnable fulfillment,
  processing, removal, reverse-delivery, and reverse-fulfillment-order roots are tracked in `docs/endpoints/returns.md`
  until conformance-backed local models exist.
- Shipping refunds staged through `refundCreate(input.shipping)` are retained on the refund record and rolled into downstream `Order.totalRefundedShippingSet`; the broader refund amount still follows the captured transaction total / line-item plus shipping fallback behavior.
- Order shipping-line tax lines contribute to total tax calculations for staged `orderCreate`, and staged shipping lines remain visible through downstream `Order.shippingLines` reads.
- State-specific lifecycle/customer validation is modeled locally for the staged order roots covered by HAR-278. Repeated `orderClose`, repeated `orderOpen`, `orderOpen` after cancellation, repeated `orderMarkAsPaid`, unknown or duplicate `orderCustomerSet`, empty `orderCustomerRemove`, and repeated `orderCancel` return concrete `userErrors` and do not mutate downstream order reads, meta state, or the mutation log.
- `orderCustomerSet` and `orderCustomerRemove` own the order-domain relationship on `OrderRecord.customer`. Customer reads consume that normalized relationship only for the immediate `Customer.orders` connection; captured HAR-288 evidence showed the customer-owned `numberOfOrders`, `amountSpent`, and `lastOrder` fields do not update in the immediate read-after-set/remove slice.
- HAR-278 order lifecycle/payment guardrails append mutation-log entries only when the handler stages a successful local effect. The scoped validation branches with `userErrors` or top-level GraphQL `errors`, including access-denied branches such as `orderCreateManualPayment` and `taxSummaryCreate`, leave the mutation log unchanged; other established safety handlers such as draft-order invoice send still retain their existing observability log entries.
- Create-time validation coverage now includes executable parity specs for `orderCreate` no-line-items and a grouped `draftOrderCreate` validation matrix. Rejected create requests return captured mutation-scoped `userErrors` locally without staging orders/draft orders or appending staged-write log entries.
- Captured `draftOrderCreate` validation branches include no line items, unknown variant, missing custom title, zero quantity, payment terms without a template id, payment terms with a template id blocked by merchant permission, negative custom line price, past reserve-inventory timestamp, and invalid email. Fresh 2026-04 probes also showed Shopify accepts variant-backed draft lines even when custom title/originalUnitPrice fields are present, accepts missing custom originalUnitPrice as a zero-price line, and accepts shippingLine without a title; those combinations are intentionally not local validation failures.
- Broader direct `orderCreate` create-time validation remains partially blocked on this host by Shopify's `Too many attempts. Please try again later.` order-create throttle under 2026-04. Keep the existing no-line-items parity fixture as executable evidence, and do not expand direct-order business validation branches without a fresh successful capture.
- Order payment transaction flows stage locally for in-memory orders. `orderCapture` turns successful authorization transactions into `CAPTURE` transactions, updates `capturable`, `totalCapturable`, `totalCapturableSet`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `displayFinancialStatus`, `paymentGatewayNames`, and records synthetic `paymentId` / `paymentReferenceId` values. Partial captures keep the remaining authorization capturable; final captures close the remaining capturable balance.
- `orderCapture` validation for over-capture, non-positive amounts, missing transactions, and no-longer-capturable authorizations returns local `userErrors` without mutating order financial state or logs.
- `transactionVoid` creates a `VOID` transaction for uncaptured authorization transactions and clears downstream capturable state. Missing, invalid, already-voided, and already-captured authorization requests return local `userErrors` without passthrough, downstream order changes, or mutation-log entries.
- `orderCreateMandatePayment` creates a completed local `Job`, a stable session-scoped `paymentReferenceId`, and a `MANDATE_PAYMENT` transaction. Reusing the same order/idempotency-key pair returns the original job/reference result and does not duplicate the transaction. Missing idempotency keys and non-positive amounts return local `userErrors` without contacting payment services.
- The local payment implementation does not contact real payment gateways and intentionally limits itself to local/synthetic orders and transaction branches covered by runtime tests or safe documentation evidence. HAR-353 promotes the local order payment fixture to executable strict parity: `order-payment-transaction-local-staging` replays order creation, over-capture validation, partial/final capture, downstream order reads, void-after-capture validation, and missing mandate idempotency-key validation; sibling specs replay successful `transactionVoid` and idempotent `orderCreateMandatePayment` branches because those require mutually exclusive order payment state. Broader Plus-only and permission-specific mandate/capture branches still require live conformance evidence before they should be expanded.
- `orderInvoiceSend` is handled locally for existing orders and does not send upstream invoice email. Safe live success recapture is side-effect-heavy and remains blocked unless a no-recipient disposable capture path is available; local runtime coverage verifies no upstream/email call is made. `taxSummaryCreate` mirrors the captured access-denied branch without invoking tax calculation services until tax-app semantics can be safely captured.

## Historical and developer notes

### Validation anchors

- Order reads: `tests/integration/order-query-shapes.test.ts`
- Abandoned checkouts and abandonments: `tests/integration/abandoned-checkout-query-shapes.test.ts`
- Order lifecycle, payment, and customer changes: `tests/integration/order-lifecycle-payment-customer-flow.test.ts`
- Order payment transaction changes: `tests/integration/order-payment-transaction-flow.test.ts`
- Order create/update flows: `tests/integration/order-creation-flow.test.ts`, `tests/integration/order-draft-flow.test.ts`
- Payment terms reads: `tests/integration/payment-terms-query-shapes.test.ts`
- Draft-order mutation family: `tests/integration/draft-order-mutation-family-flow.test.ts`
- Fulfillments: `tests/integration/order-fulfillment-flow.test.ts`, `tests/integration/order-query-shapes.test.ts`
- Fulfillment-order lifecycle capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-lifecycle.json`
- Order editing: `tests/integration/order-edit-flow.test.ts`
- Refunds and shipping-refund aggregates: `tests/integration/order-refund-flow.test.ts`
- Returns: `tests/integration/order-return-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/orders/order*.json`, `config/parity-specs/orders/draftOrder*.json`, `config/parity-specs/orders/draftOrders*.json`, `config/parity-specs/shipping-fulfillments/fulfillment*.json`, `config/parity-specs/orders/refund*.json`, and matching files under `config/parity-requests/orders/` or `config/parity-requests/shipping-fulfillments/`. For order editing, prefer the `orderEditExistingOrder-*` workflow specs plus the missing-id validation slices over single-root planned placeholders.
- Residual draft-order helper capture: `corepack pnpm conformance:capture-draft-order-residual-helpers`

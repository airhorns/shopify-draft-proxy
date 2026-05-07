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
- `orderEditAddShippingLine`
- `orderEditRemoveDiscount`
- `orderEditRemoveShippingLine`
- `orderEditSetQuantity`
- `orderEditUpdateShippingLine`
- `orderEditCommit`

### HAR-439 order lifecycle/payment/refund fidelity review

The HAR-439 review rechecked the order catalog, lifecycle, payment, refund,
abandoned checkout, and abandonment roots against Shopify Admin GraphQL docs,
public GraphQL examples, existing parity specs, and the integration tests listed
below. The strongest executable coverage is still the local-runtime order graph:
`orderCreate` seeds realistic staged orders, lifecycle mutations update the same
order rows, payment mutations derive downstream financial fields from staged
transactions, and `refundCreate` projects refund records, refund transactions,
shipping refunds, and downstream order totals without runtime Shopify writes.

High-risk paths with executable evidence:

- `orderCancel` now mirrors Shopify's asynchronous cancellation payload more
  closely when `job` is selected: successful local cancellation returns a
  synthetic `Job` with `done: false`, exposes both `orderCancelUserErrors` and
  `userErrors`, and still stages `closed`, `closedAt`, `cancelledAt`, and
  `cancelReason` locally without upstream calls.
- `orderCapture`, `transactionVoid`, `orderCreateManualPayment`, and
  `orderCreateMandatePayment` remain local/synthetic payment models. They cover
  downstream order financial fields, transaction reads, idempotent mandate
  payment behavior, validation branches, mutation-log preservation, and no
  payment-service calls.
- `refundCreate` covers staged line-item and shipping refunds, refund
  transactions, downstream `Order.refunds`, `Order.transactions`,
  `totalRefundedSet`, `totalRefundedShippingSet`, and over-refund validation.
  The public Admin GraphQL 2026-04 `RefundInput` schema does not expose the
  retail attribution keys `pointOfSaleDeviceId`, `locationId`, `userId`, or
  `transactionGroupId`; the proxy mirrors the captured top-level coercion
  errors for those fields before refund staging can run.
- Abandoned checkout empty/no-data reads and local seeded reads are executable.
  `abandonmentUpdateActivitiesDeliveryStatuses` stages known local forward
  delivery-status transitions and preserves raw mutation order; unknown
  abandonments mirror the captured `abandonment_not_found` branch. Focused
  local runtime tests cover same-status no-ops, unknown activity references,
  backwards transitions, and future `deliveredAt` rejection.

Remaining gaps that should not be overclaimed:

- Non-empty abandoned checkout capture is still not live-fixture-backed because
  the conformance store had no abandoned checkout records during capture.
- Direct `orderCreate` has a representative rich local/create parity slice, but
  broad live business-validation expansion remains constrained by observed
  Shopify throttling and should be added only with fresh captures.
- `orderCreateManualPayment` success with amount remains Plus/permission
  sensitive on the current conformance store; local success coverage is a
  synthetic-order runtime model, not live Plus-store evidence.
- `orderCreateMandatePayment` does not validate real mandate ownership or
  payment-service behavior; it models only local order/idempotency/amount
  effects.
- `orderInvoiceSend` and other customer-visible email roots remain local intent
  only unless explicitly committed later.

### Declared Gaps

- `orderRiskAssessmentCreate` is a registry-only HAR-316 scaffold. The 2025-01 `finance-risk-access-read` capture records only an unknown-order validation branch returning `userErrors[{ field: ["orderRiskAssessmentInput", "orderId"], code: "NOT_FOUND" }]`. Do not mark this mutation supported until local risk assessment staging, downstream order risk reads, Shopify-like userErrors, and raw commit replay are modeled end to end.
- `taxSummaryCreate` has captured 2026-04 access-denied parity on the current conformance store, and the runtime mirrors that branch locally without passthrough. It is not marked as implemented operation support because tax-app enqueue/result semantics and downstream tax summary behavior still need conformance-backed modeling.

### Behavior notes

- Order and draft-order reads use the shared Shopify-style search parser for catalog, count, invalid-query, and pagination slices covered by parity fixtures.
- HAR-538 migrated the orders parity suite to cassette-backed LiveHybrid execution. Cold order and draft-order reads use gated Pattern 1 passthrough only when no staged local order state can affect the result; supported mutations keep staging locally and use targeted Pattern 2 cassette hydration for prior order, draft-order, calculated-order, refund, fulfillment, return, customer, and product-variant context before applying local effects.
- Order fulfillment mutations stage locally in snapshot mode. `fulfillmentCreate` covers validation slices plus order-backed creation from local fulfillment orders, while `fulfillmentEventCreate`, `fulfillmentTrackingInfoUpdate`, and `fulfillmentCancel` update seeded or staged fulfillment records locally. `fulfillmentCreate` mirrors public Admin API 2026-04 preconditions for closed fulfillment orders and over-quantity requests before staging any fulfillment or mutation-log entry: both return `userErrors` on `field: ["fulfillment"]`, and this root's public `UserError` shape exposes `field` / `message` but no selectable `code`. The same capture records that `fulfillmentOrderReportProgress` can leave a fulfillment order `IN_PROGRESS` and public `fulfillmentCreate` still accepts it; the proxy follows that public behavior. Fulfillment-order request/cancellation roots stage against order-backed fulfillment orders: submit can split partial quantities into submitted/unsubmitted fulfillment orders, accept/reject fulfillment requests update request status, and cancellation submit/accept/reject preserves merchant request history. HAR-234 fulfillment-order lifecycle support stages held, released, moved, progress-reported, reopened, and cancelled fulfillment orders from the same order-owned fulfillment-order graph and keeps downstream nested/top-level reads consistent without upstream writes. HAR-573 adds cancel preconditions for already-cancelled/otherwise non-cancellable states plus manually progress-reported orders, returning Shopify-captured `field` / `message` values and locally selected code-shaped `userErrors` without staging a replacement when the cancel is rejected.
- Nested `Order.fulfillments` and `Order.fulfillmentOrders` remain the order-owned source for top-level fulfillment reads. The shipping/fulfillments endpoint docs describe the top-level `fulfillment(id:)`, `fulfillmentOrder(id:)`, and fulfillment-order catalog roots that now serialize from the same local order graph.
- Fulfillment flows return Shopify-shaped `userErrors` and expose staged state through immediate downstream order fulfillment reads without sending supported mutations to Shopify at runtime. Staged fulfillment events are visible through both top-level `fulfillment(id:)` and nested `Order.fulfillments.events`, and tracking/cancel updates preserve event history and shipment milestone fields. Staged fulfillment-order request statuses and merchant request messages are visible through `fulfillmentOrder`, `fulfillmentOrders`, `assignedFulfillmentOrders`, and nested `Order.fulfillmentOrders`; no fulfillment-service notification callbacks are invoked. Broader shipping/fulfillment roots and coverage boundaries are tracked in `docs/endpoints/shipping-fulfillments.md`.
- `orderUpdate` rejects locally known orders before staging when the input has no mutable attributes beyond `id`, when `phone` fails the same local E.164-shaped guard used by customer mutations, or when `shippingAddress` fails the shared country/province resolution used for mailing-address-style inputs. These branches return mutation-scoped `userErrors` with `order: null` and do not append mutation-log entries. Shopify's phone/email churn throttle is not modeled locally; treat throttle parity as captured-only fidelity until a session-scoped `Throttle::Counter` equivalent is deliberately added.
- Draft-order create/complete/update/duplicate/delete/invoice/create-from-order flows preserve staged state for downstream reads and commit replay. `draftOrderComplete(paymentGatewayId:)` consults the effective shop payment gateway fixture (`ShopRecord.paymentSettings.paymentGateways`) instead of hardcoding every gateway to invalid: active gateways stage a resulting order with the gateway name, a synthetic `SALE` transaction and `displayFinancialStatus: PAID`, while `paymentPending: true` stages an `AUTHORIZATION` transaction and `displayFinancialStatus: AUTHORIZED`. Unknown gateways return `userErrors[{ field: ["paymentGatewayId"], message: "payment_gateway_not_found", code: "INVALID" }]`; disabled gateways return `payment_gateway_disabled` with the same field/code and do not mutate draft/order state or append a staged-write log entry. Omitting `paymentGatewayId` preserves the existing manual/no-gateway behavior, including the `paymentPending: true` `PENDING` baseline with no transaction.
- `draftOrderDuplicate` stages a new `OPEN` draft for both open and completed source drafts. The duplicate keeps cloneable draft content but clears lifecycle-owned fields (`completedAt`, `invoiceSentAt`, `orderId`, and nested `order`), allocates fresh local IDs/name/timestamps, and regenerates the local invoice URL. Current 2025-01 live evidence returns `ready: true` for ready source drafts, so the proxy preserves that behavior rather than forcing the duplicate to `ready: false`.
- `draftOrderSavedSearches` mirrors the captured default draft-order saved searches as a local connection. Those saved-search IDs can drive local `draftOrderBulk*` target selection through their captured query strings.
- `draftOrderAvailableDeliveryOptions` currently mirrors the captured no-data helper shape: empty shipping/local-delivery/local-pickup arrays and empty `pageInfo`. Non-empty delivery-rate modeling remains future work until delivery-profile-backed draft-order evidence exists.
- `draftOrderBulkAddTags`, `draftOrderBulkRemoveTags`, and `draftOrderBulkDelete` stage against the effective local draft-order set selected by `ids`, `search`, or captured draft-order saved-search query. HAR-440 executable parity covers the safe unique `search` branch for bulk tag job payloads, while runtime tests cover saved-search targeting because Shopify's captured default `Open` saved search is too broad to use safely in a live bulk mutation against a shared disposable store. Downstream `draftOrder(id:)` and draft-order catalog reads observe tag changes and deletions immediately; the returned `Job` keeps Shopify's captured async `done: false` payload shape even though the proxy applies the local effect synchronously.
- Draft-order bulk tag mutations trim tag input, deduplicate by case-insensitive identity, and remove tags by that same identity. Tags longer than 255 characters return `INVALID` with `tag_too_long` on the indexed `["input", "tags", n]` path and are omitted from staging while valid tags in the same request can still apply. Requests that exceed the 250-tag guardrail return `INVALID` with `too_many_tags` on `["input", "tags"]`. Unknown draft-order IDs in `ids` return per-index `NOT_FOUND` userErrors without aborting valid IDs in the same batch.
- The `draftOrderBulkTag-validation` local-runtime parity scenario replays those bulk tag guardrails through the generic parity runner, including partial success with an unknown ID, long-tag rejection, too-many-tag rejection, normalized removal, and downstream `draftOrder(id:)` readback. Public Admin 2025-01 currently accepts the long-tag/unknown-ID bulk add as an async job with no synchronous userErrors, so this executable fixture documents the proxy's internal Tagging guardrail contract rather than a live public Admin response.
- `draftOrderCalculate` validates the supported `DraftOrderInput` slices locally before evaluating the draft-order pricing model, without staging a draft order or sending a mutation upstream. The HAR-580 2026-04 capture records Shopify returning `calculatedDraftOrder: null` for empty `lineItems` and invalid `email` branches, `availableShippingRates: []` when a shipping-required variant has no `shippingAddress`, and a successful valid `paymentTermsTemplateId` calculation on the conformance merchant. The proxy mirrors those captured branches, covers selected totals, line item prices, discounts, shipping totals, and `CalculatedDraftOrder` scalar/list fields, but does not compute delivery-profile-backed non-empty shipping-rate options locally.
- `draftOrderInvoicePreview` returns deterministic local preview subject/html for staged draft orders and never sends email or writes upstream. It mirrors the safe preview contract enough for tests that need a payload before deciding whether to send an invoice.
- The `draft-order-invoice-send-safety` parity fixture is executable generic parity coverage rather than capture-only evidence. The runner replays the captured unknown-id, deleted-draft, open no-recipient, and completed no-recipient validation branches through the local proxy with strict JSON comparison while seeding only the disposable captured setup draft states; recipient-backed invoice sends remain runtime-blocked to avoid customer-visible email.
- Abandoned checkout reads are modeled for snapshot/local state. Empty `abandonedCheckouts` returns an empty connection with false/null `pageInfo`, `abandonedCheckoutsCount` returns `{ count: 0, precision: "EXACT" }`, and missing `abandonment` / `abandonmentByAbandonedCheckoutId` lookups return `null`, matching the 2026-04-27 live capture against `harry-test-heelo.myshopify.com` on Admin GraphQL `2025-01`.
- Representative non-empty abandoned checkout and abandonment reads serialize from seeded normalized records. Local `abandonedCheckouts(query:)` and `abandonedCheckoutsCount(query:)` use the shared Shopify search helpers for the documented `id`, `created_at`, `updated_at`, `status`, `recovery_state`, `email_state`, and default text/title slices. The live conformance store had no abandoned checkout records during HAR-300, so non-empty runtime coverage is schema/introspection-backed rather than a live non-empty fixture. Future work should replace or supplement that seeded proof when a disposable store can produce real abandoned checkout data.
- `abandonmentUpdateActivitiesDeliveryStatuses` is local-only for seeded/snapshot abandonment records and cassette-backed local-runtime abandonment hydration. Public `Abandonment` reads expose `emailState` / `emailSentAt` but not the internal marketing-activity delivery map, so the edge-case parity fixture supplies that map through the cassette rather than claiming live public Admin discovery. Unknown abandonment IDs mirror the captured safe payload `abandonment: null` plus `userErrors[{ field: ["abandonmentId"], message: "abandonment_not_found" }]`. For known local abandonments, the marketing activity must already be referenced by that abandonment's delivery activity map; unknown activity IDs return `userErrors[{ field: ["deliveryStatuses", "0", "marketingActivityId"], message: "invalid", code: "NOT_FOUND" }]` without staging state. Same-status updates return silent success without changing the staged abandonment record. Backwards transitions such as `DELIVERED` to `SENDING` return `userErrors[{ field: ["deliveryStatuses", "0", "deliveryStatus"], message: "invalid_transition", code: "INVALID" }]`. `DELIVERED` updates reject future `deliveredAt` values on `["deliveryStatuses", "0", "deliveredAt"]`. Forward transitions update the in-memory delivery activity map, surface `emailState` / `emailSentAt` changes on downstream local reads, append the original raw mutation to the meta log, and never send the runtime mutation to Shopify.
- `draftOrderInvoiceSend` is treated as an outbound email side-effect root. Runtime support never sends the mutation upstream or emails a customer; it appends the original raw mutation to the meta log for explicit commit replay. Safe captured 2026-04 branches are mirrored locally for missing/unknown/deleted draft IDs, no-recipient drafts (`To can't be blank`), and completed no-recipient drafts (`To can't be blank` plus the already-paid error). For open local drafts with a recipient, the proxy returns an explicit local userError instead of pretending the invoice email was delivered.
- `draftOrderTag` remains an explicit blocker rather than implemented support. HAR-318 live probing showed raw tag strings fail ID validation and guessed `gid://shopify/DraftOrderTag/<tag>` IDs return `null`; no exposed catalog in the current evidence produced a valid `DraftOrderTag` ID. The runtime can synthesize local staged tag IDs for internal helper reads, but the registry keeps the root unimplemented until a valid-ID capture exists.
- `draftOrder(id:)` returns `null` for absent IDs. The `draft-order-by-id-not-found-read` parity scenario captures this missing-id behavior without relying on live upstream passthrough.
- Draft-order detail parity now compares the captured `draftOrder(id:)` payload as a strict object for the selected phone, timestamp, subtotal/total, line-item unit-price, SKU/nullability, address, shipping-line, custom-attribute, discount, tax-exemption, and payment-terms fields. The current live detail capture returns `paymentTerms: null` for the merchant-realistic draft without terms and preserves empty line-item structures such as `customAttributes: []`, `appliedDiscount: null`, and variant-backed SKU/title nullability.
- Local `Order.paymentTerms` and `DraftOrder.paymentTerms` reads preserve `null` for orders/drafts without terms. When normalized payment terms are present in the local graph, the serializer exposes selected scalar fields plus the nested `paymentSchedules` connection with shared cursor/window/pageInfo handling and schedule money fields (`amount`, `balanceDue`, `totalBalance`). The standalone `paymentTermsCreate`, `paymentTermsUpdate`, and `paymentTermsDelete` roots now stage against this same order/draft-order graph, so downstream reads observe creates, updates, and deletes immediately without runtime Shopify writes. The executable 2026-04 parity fixture uses a disposable draft order and confirms NET `dueAt` derivation, replacement schedule IDs on update, and null downstream terms after delete.
- Shopify normalizes draft-order shipping lines created with `priceWithCurrency` to `code: "custom"`, `custom: true`, and matching `originalPriceSet` / `discountedPriceSet` shop-money amounts. The local serializer mirrors that shape and uses `null` for absent shipping lines after duplicate/create-from-order flows.
- The captured DraftOrder detail read surface does not select `note`; local mutation payloads and downstream local reads still preserve staged note values, but live detail parity keeps note out of the strict object contract until Shopify exposes a selectable note field for this surface.
- Order edit operations use calculated-order state during the edit session and materialize changes on `orderEditCommit`. Current local staging covers variant additions, custom item additions, line-item discount add/remove, quantity edits, shipping-line add/update/remove, non-null payload user-error branches for unknown begin/add/set/commit targets, not-editable begin guards for refunded/voided/cancelled orders, and single-open-session begin rejection. `orderEditAddVariant` keeps newly added variant rows under `CalculatedOrder.addedLineItems` while leaving `CalculatedOrder.lineItems` scoped to the original calculated line items. `orderEditAddCustomItem` mirrors the captured Shopify validation branches for missing/blank/oversized title, non-positive quantity, negative price, missing inline `MoneyInput.currencyCode`, currency mismatch, and the happy-path custom-item payload; rejected branches return `userErrors` or top-level GraphQL errors without staging line items or consuming synthetic IDs. The custom-item price currency falls back to the host order/session currency, not a hardcoded shop currency. The local channel-policy guard is explicit-data-only: if hydrated or seeded order JSON carries `addCustomItemAllowed`, `add_custom_item_allowed`, or `__draftProxyAddCustomItemAllowed: false`, the proxy returns `not_supported` without staging; absent policy data is not guessed from source/channel names. Successful commits append local `OrderEdit` history and event nodes, propagate added/decremented line items onto local fulfillment-order line items, and recompute current subtotal, total, tax-total, and tax-line values from the calculated-order session plus captured order tax evidence. The order-edit conformance anchors are the captured existing-order workflow specs, `orderEdit-lifecycle-userErrors` for missing-resource payload roots, executable single-root begin/add/set/commit parity slices backed by workflow fixtures, the residual calculated-edit parity spec for custom item/discount/shipping mutation payloads, the `orderEditAddCustomItem-validation` live parity spec, and the local-runtime residual edit/delete spec for roots that must not write to Shopify during runtime.
- HAR-441 state-machine review: local order editing now has executable coverage for begin -> calculated edits -> commit, zero-quantity removal, validation, line-discount staged-change payloads, shipping-line add/update/remove calculated-order state, committed shipping-line downstream reads, mutation-log preservation, calculated-order cleanup after commit, begin rejection when an order already has an open edit session, and non-null payload userErrors for unknown calculated-order/line/variant targets. Remaining gaps that should not be overclaimed are existing-order shipping-line update/remove behavior when Shopify starts with non-empty shipping lines, tax/duty/refund recalculation beyond the captured simple money totals, and customer notification/email side effects from `orderEditCommit(notifyCustomer: true)`.
- `orderDelete` stages an order tombstone locally only for orders without financial, fulfillment, open-return, or open-fulfillment-order state. Non-deletable orders return an `OrderDeleteUserError` on `orderId` with code `INVALID` and leave the local graph unchanged; missing orders return code `NOT_FOUND`. Successful deletes cascade local cleanup so downstream `order(id:)`, attached fulfillment-order and return lookups, order payment terms, and abandoned-checkout order linkage no longer expose the deleted order. Local `orders` / `ordersCount` omit the deleted order immediately, and repeated deletes do not append another staged-write log entry.
- `refundCreate` stages refund records for downstream order reads and covers
  over-refund user-error behavior through parity fixtures. The
  `refundCreate-attribution-validation` parity spec records the public
  2026-04 schema behavior for unsupported retail attribution keys: variable
  inputs return one `INVALID_VARIABLE` error with all four problems, inline
  inputs return per-field `argumentNotAccepted` errors, `UserError.code` is not
  selectable for the payload, and downstream order reads remain unchanged.
- Return staging is order-backed: `returnCreate` and `returnRequest` create local Return rows for known fulfilled order
  line items, while `returnCancel`, `returnClose`, and `returnReopen` enforce captured status-machine preconditions and
  idempotent no-op behavior before updating local return status. Top-level `return(id:)` and nested `Order.returns` read
  from the same order graph. Broader calculation, returnable fulfillment, processing, removal, reverse-delivery, and
  reverse-fulfillment-order roots are tracked in `docs/endpoints/returns.md` until conformance-backed local models exist.
- Shipping refunds staged through `refundCreate(input.shipping)` are retained on the refund record and rolled into downstream `Order.totalRefundedShippingSet`; the broader refund amount still follows the captured transaction total / line-item plus shipping fallback behavior.
- Order shipping-line tax lines contribute to total tax calculations for staged `orderCreate`, and staged shipping lines remain visible through downstream `Order.shippingLines` reads.
- State-specific lifecycle/customer validation is modeled locally for the staged order roots covered by HAR-278. Redundant `orderClose` and `orderOpen` return Shopify's silent-success payload (`userErrors: []`) without changing `closedAt`/`updatedAt`, consuming a synthetic timestamp, staging local state, or appending a mutation-log entry. Other repeated or invalid lifecycle/customer branches such as `orderOpen` after cancellation, repeated `orderMarkAsPaid`, unknown or duplicate `orderCustomerSet`, empty `orderCustomerRemove`, and repeated `orderCancel` return concrete `userErrors` and do not mutate downstream order reads, meta state, or the mutation log. Successful local `orderCancel` returns a synthetic async `Job` when selected, with `done: false`, matching the captured shape used by Shopify cleanup flows while keeping the actual cancellation state local.
- `orderCustomerSet` and `orderCustomerRemove` own the order-domain relationship on `OrderRecord.customer`. Customer reads consume that normalized relationship only for the immediate `Customer.orders` connection; captured HAR-288 evidence showed the customer-owned `numberOfOrders`, `amountSpent`, and `lastOrder` fields do not update in the immediate read-after-set/remove slice.
- `orderCustomerSet` returns field-specific error codes for relationship failures: missing orders return `NOT_FOUND` on `orderId`, missing customers return `NOT_FOUND` on `customerId`, and B2B contacts without an ordering role return `NOT_PERMITTED` on `customerId` with `no_customer_role_error`. `orderCustomerRemove` returns missing-order `NOT_FOUND` on `orderId` and refuses cancelled orders with `INVALID` / `customer_cannot_be_removed` without staging a relationship change.
- HAR-278 order lifecycle/payment guardrails append mutation-log entries only when the handler stages a successful local effect. The scoped validation branches with `userErrors` or top-level GraphQL `errors`, including the `taxSummaryCreate` access-denied branch and the unhydrated-order `orderCreateManualPayment` access-denied branch, leave the mutation log unchanged. Other established safety handlers such as draft-order invoice send still retain their existing observability log entries.
- Create-time validation coverage now includes executable parity specs for `orderCreate` no-line-items, HAR-556's extended `orderCreate` validation matrix, and a grouped `draftOrderCreate` validation matrix. Rejected create requests return mutation-scoped `userErrors` locally without staging orders/draft orders or appending staged-write log entries. The extended direct-order matrix covers future `processedAt`, redundant `customerId` + `customer`, missing/empty line-item tax-line `rate`, and missing/empty shipping-line tax-line `rate`, including Shopify-style `code` projection and indexed `field` paths.
- Captured `draftOrderCreate` validation branches include no line items, unknown variant, missing custom title, zero quantity, payment terms without a template id, payment terms with a template id blocked by merchant permission, negative custom line price, past reserve-inventory timestamp, and invalid email. Fresh 2026-04 probes also showed Shopify accepts variant-backed draft lines even when custom title/originalUnitPrice fields are present, accepts missing custom originalUnitPrice as a zero-price line, and accepts shippingLine without a title; those combinations are intentionally not local validation failures.
- Broader direct `orderCreate` create-time validation remains partially constrained on this host by Shopify order-create throttles and schema coercion. HAR-556 live probing under the current 2025-01 conformance store confirmed the store/auth path is usable, but omitted or empty inline tax-line `rate` is rejected by GraphQL input coercion before the mutation resolver; the local guardrail still models the ticket's cited mutation-level `TAX_LINE_RATE_MISSING` behavior for resolved inputs that reach the proxy handler.
- Order payment transaction flows stage locally for in-memory orders. `orderCapture` turns successful authorization transactions into `CAPTURE` transactions, updates `capturable`, `totalCapturable`, `totalCapturableSet`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `displayFinancialStatus`, `paymentGatewayNames`, and records synthetic `paymentId` / `paymentReferenceId` values. Partial captures keep the remaining authorization capturable; final captures close the remaining capturable balance. `orderCreateManualPayment` uses the same downstream payment derivation path for direct manual `SALE` transactions without requiring a prior authorization.
- `orderMarkAsPaid` stages only valid non-cancelled orders whose financial status is not `PAID`, `REFUNDED`, `PARTIALLY_REFUNDED`, or `VOIDED`. Rejected state transitions return `userErrors` on `["id"]` without appending transactions, changing `paymentGatewayNames`, or recording a staged mutation. Successful local mark-as-paid transactions and outstanding totals emit complete MoneyBag payloads with both `shopMoney` and `presentmentMoney`; single-currency orders mirror shop money, while multi-currency orders use the order's `presentmentCurrencyCode` when available. The public 2026-04 live `orderMarkAsPaid.userErrors` shape is `UserError` without a selectable `code` field, so the executable parity scenario compares field/message and focused runtime tests cover the local `INVALID` code projection.
- `orderCapture` validation for multi-currency capture currency, over-capture, non-positive amounts, missing transactions, and no-longer-capturable authorizations returns local `userErrors` without mutating order financial state or logs. Local capture validation follows the internal `OrderCaptureUserError` code contract for these branches: `CURRENCY_REQUIRED` / `CURRENCY_MISMATCH` on `["currency"]`, `TRANSACTION_NOT_FOUND` / `INVALID_TRANSACTION_STATE` on `["parent_transaction_id"]`, and `INVALID_AMOUNT` / `OVER_CAPTURE` on `["amount"]`. A 2026-04 live public Admin probe against `harry-test-heelo.myshopify.com` confirmed the public `OrderCapturePayload` currently exposes only `transaction` and plain `UserError` (`field`, `message`) without selectable `order` or `code`; public live validation messages for manual test authorizations also differ from the internal code contract, so focused runtime tests cover the local code projection while the existing local-runtime parity fixture covers staged payment effects.
- `transactionVoid` accepts the current `parentTransactionId` argument shape and retains legacy `id` / input-object compatibility for older local fixtures. It creates a `VOID` transaction for uncaptured authorization transactions and clears downstream capturable state. Missing, invalid, already-voided, and already-captured authorization requests return local `userErrors` without passthrough, downstream order changes, or mutation-log entries.
- `orderCreateManualPayment` accepts `id`, optional `amount`, optional `paymentMethodName`, and optional `processedAt` for orders already present in proxy state. A successful local manual payment creates a synthetic successful `SALE` transaction, uses `paymentMethodName` as the transaction gateway and `Order.paymentGatewayNames` entry, preserves the provided `processedAt` on the transaction, updates `displayFinancialStatus`, `totalOutstandingSet`, `totalReceivedSet`, `netPaymentSet`, `capturable`, and downstream transaction reads, and appends the original raw mutation request for commit replay. Non-positive amounts, amounts above the outstanding balance, and already-paid orders return local `userErrors` without mutating state or logs. The captured 2026-04 access-denied branch remains executable parity for unhydrated/non-local order IDs and is returned without passthrough. Live success capture remains blocked on the current conformance credential because `harry-test-heelo.myshopify.com` reports `shop.plan.shopifyPlus: false`; Shopify's captured access-denied message requires the API client to be installed on a Shopify Plus store when the `amount` field is used.
- `orderCreateMandatePayment` accepts Shopify's current `mandateId` argument but does not validate real mandate ownership or contact payment services; the executable local evidence uses a synthetic PaymentMandate GID and models only the order/idempotency/amount/auto-capture effects. It creates a completed local `Job`, returns the Shopify-style `<order_gid>/<idempotency_key>` `paymentReferenceId`, and stages either a `SALE` transaction when `autoCapture` is omitted/true or an `AUTHORIZATION` transaction when `autoCapture: false`. Reusing the same order/idempotency-key pair returns the original job/reference result and does not duplicate the transaction. Missing `mandateId` is rejected by GraphQL schema validation before resolver execution; missing idempotency keys and non-positive amounts return local `userErrors` without contacting payment services.
- The local payment implementation does not contact real payment gateways and intentionally limits itself to local/synthetic orders and transaction branches covered by runtime tests or safe documentation evidence. HAR-353 promotes the local order payment fixture to executable strict parity: `order-payment-transaction-local-staging` replays order creation, over-capture validation, partial/final capture, downstream order reads, void-after-capture validation, and missing mandate idempotency-key validation; sibling specs replay successful `transactionVoid` and idempotent `orderCreateMandatePayment` branches because those require mutually exclusive order payment state. Broader Plus-only and permission-specific mandate/capture branches still require live conformance evidence before they should be expanded.
- `orderInvoiceSend` is handled locally for existing orders and does not send upstream invoice email. Safe live success recapture is side-effect-heavy and remains blocked unless a no-recipient disposable capture path is available; local runtime coverage verifies no upstream/email call is made. `taxSummaryCreate` mirrors the captured access-denied branch without invoking tax calculation services, but remains a declared gap rather than implemented support until tax-app semantics can be safely captured.

## Historical and developer notes

### Validation anchors

- Fulfillment-order lifecycle capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-lifecycle.json`
- Conformance fixtures and requests: `config/parity-specs/orders/order*.json`, `config/parity-specs/orders/draftOrder*.json`, `config/parity-specs/orders/draftOrders*.json`, `config/parity-specs/shipping-fulfillments/fulfillment*.json`, `config/parity-specs/orders/refund*.json`, and matching files under `config/parity-requests/orders/` or `config/parity-requests/shipping-fulfillments/`. For order editing, prefer `orderEdit-lifecycle-userErrors`, the `orderEditExistingOrder-*` workflow specs, and the missing-id validation slices over single-root planned placeholders.
- Residual draft-order helper capture: `corepack pnpm conformance:capture-draft-order-residual-helpers`

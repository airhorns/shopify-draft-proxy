# Payments

This endpoint group tracks Admin GraphQL payment-area roots whose behavior is sensitive because they can expose payment settings, payment methods, payment status, or Shopify Functions-backed checkout configuration.

Order payment transaction mutations such as `orderCapture`, `transactionVoid`, and `orderCreateMandatePayment` are modeled with the order graph because their downstream reads are `Order` financial and transaction fields. Their local validation and payment-service safety notes live in `docs/endpoints/orders.md`.

## Current support and limitations

### Supported roots

- `paymentCustomization(id:)`
- `paymentCustomizations(...)`
- `paymentCustomizationCreate`
- `paymentCustomizationUpdate`
- `paymentCustomizationDelete`
- `paymentCustomizationActivation`
- `customerPaymentMethodCreditCardCreate`
- `customerPaymentMethodCreditCardUpdate`
- `customerPaymentMethodRemoteCreate`
- `customerPaymentMethodPaypalBillingAgreementCreate`
- `customerPaymentMethodPaypalBillingAgreementUpdate`
- `customerPaymentMethodGetDuplicationData`
- `customerPaymentMethodCreateFromDuplicationData`
- `customerPaymentMethodGetUpdateUrl`
- `customerPaymentMethodRevoke`
- `paymentReminderSend`
- `paymentTermsTemplates(paymentTermsType:)`
- `paymentTermsCreate`
- `paymentTermsUpdate`
- `paymentTermsDelete`

Payment customization writes are local-only once supported. They must not invoke Shopify Functions or mutate checkout payment behavior at runtime; commit replay keeps the original raw mutation for an explicit later commit.

### Payment customizations

Payment customization records live in normalized state as `PaymentCustomizationRecord` rows keyed by Admin GID. Snapshot-mode reads return:

- `paymentCustomization(id:)`: the modeled record when present, otherwise `null`
- `paymentCustomizations(...)`: a non-null connection with `nodes`, `edges`, and selected `pageInfo`; an empty normalized graph returns empty arrays and false/null pageInfo values

Catalog reads support local cursor pagination through `first`, `last`, `after`, and `before`, plus `reverse`. Search query support is intentionally limited to captured-safe filters:

- `enabled:true|false`
- `function_id:<gid-or-tail>`
- `id:<gid-or-tail>`
- default/title text matching over captured `title`

Local cursors use the proxy's synthetic `cursor:<gid>` form. Shopify's opaque cursor encoding is not a contract clients should depend on.

Selected scalar detail fields currently include `id`, `legacyResourceId`, `title`, `enabled`, and `functionId`. `shopifyFunction` and `errorHistory` are replayed from captured normalized JSON only; when those slices are absent the serializer returns `null` rather than inventing Function ownership or failure history. Owner-scoped `metafield(namespace:, key:)` and `metafields(...)` selections serialize from the payment customization's captured metafield rows using the shared metafield serializer.

Lifecycle mutations stage against the same normalized records:

- `paymentCustomizationCreate(paymentCustomization:)` creates a synthetic local `PaymentCustomization` record with selected metafields when required `title`, `enabled`, and a Function identifier are present. The current Admin API uses `functionHandle`; deprecated `functionId` remains accepted for captured 2025-01 parity branches.
- `paymentCustomizationUpdate(id:, paymentCustomization:)` merges scalar/metafield input over an existing normalized record. Updating either Function identifier clears the other local identifier and does not invent a `shopifyFunction` object.
- `paymentCustomizationActivation(ids:, enabled:)` toggles `enabled` on existing normalized records and returns the updated ids.
- `paymentCustomizationDelete(id:)` marks a normalized record deleted so downstream detail reads return `null` and catalog reads omit it.

Captured validation branches are modeled locally for missing create fields, missing Function id `gid://shopify/ShopifyFunction/0`, unknown update/delete ids, unknown activation ids, and empty activation id lists. The latest 2026-04 docs also expose `functionHandle`, `MULTIPLE_FUNCTION_IDENTIFIERS`, `FUNCTION_NOT_FOUND`, and `INVALID_METAFIELDS`; the local model accepts Function handles, rejects the captured invalid handle sentinels, rejects requests that provide both `functionId` and `functionHandle`, and rejects structurally invalid owner metafields. The current local model does not maintain a full Shopify Function catalog; successful create/update paths preserve the provided Function identifier and return `shopifyFunction` only if that field was present in normalized state.

### Access Scopes And Capture Notes

HAR-219 recorded that the refreshed 2026-04-25 conformance app can safely read payment customization empty/null behavior with `read_payment_customizations`, and HAR-223 captured that current empty/null slice in `payment-customization-empty-read`. The same conformance credential has `write_payment_customizations`; HAR-223 captured validation/error branches for missing Function ownership and unknown ids in `payment-customization-validation`.

The current test store had no released `ShopifyFunction` nodes at capture time, so non-empty live happy-path create/update/delete/activation remains local-runtime evidence rather than a live Shopify parity contract. HAR-223 added and deployed the repo-local `conformance-payment-customization` Function extension to the conformance app, but the existing store install still returned an empty `shopifyFunctions` catalog afterward; a future live happy-path capture likely needs a refreshed app install/grant that can see the released Function. Non-empty detail, Function ownership, and error-history behavior should be promoted into fixtures/parity specs only after real interactions exist and the comparison contract is ready.

### Finance, Risk, Disputes, And POS Cash

HAR-316 records coverage scaffolds for the sensitive finance/risk roots `cashTrackingSession`, `cashTrackingSessions`, `financeAppAccessPolicy`, `financeKycInformation`, `pointOfSaleDevice`, `dispute`, `disputes`, `disputeEvidence`, `disputeEvidenceUpdate`, `shopPayPaymentRequestReceipt`, `shopPayPaymentRequestReceipts`, `shopifyPaymentsPayoutAlternateCurrencyCreate`, and `tenderTransactions`.

The checked-in capture `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/finance-risk-access-read.json` deliberately avoids creating or exposing financial records. It records only root introspection, unknown-id or unknown-token reads, type-only connection nodes, access-denied credential blockers, an unknown-order `orderRiskAssessmentCreate` validation branch, and a non-executing missing-currency validation request for `shopifyPaymentsPayoutAlternateCurrencyCreate`.

Current 2025-01 implemented no-data coverage:

- `cashTrackingSession(id:)`, `pointOfSaleDevice(id:)`, `dispute(id:)`, and `shopPayPaymentRequestReceipt(token:)` return `null` for unknown identifiers.
- `cashTrackingSessions(first: 1)` and `shopPayPaymentRequestReceipts(first: 1)` return empty connections with false `pageInfo` flags on the current store.
- `disputes(first: 1)` returns an empty connection.

Still-blocked sensitive coverage:

- `disputeEvidence(id:)` is denied without `read_shopify_payments_dispute_evidences`.
- `financeAppAccessPolicy` is denied without the required valid finance app user session/client; `financeKycInformation` is denied without `read_financial_kyc_information` plus finance-app permission.
- `disputeEvidenceUpdate` is denied without `write_shopify_payments_dispute_evidences` plus staff dispute/order permission.
- `tenderTransactions(first: 1)` may expose real transaction rows on the current store, so the capture selects only `__typename` and page flags. Do not add IDs, amounts, payment methods, remote references, users, or transaction details unless the fixture is deliberately scrubbed and justified.
- `shopifyPaymentsPayoutAlternateCurrencyCreate` must remain unsupported until a local payout model exists; live happy paths can alter payout configuration or money movement.

The `finance-risk-no-data-read` parity scenario enforces the implemented empty/null local overlay slice. Future support needs disposable-store fixtures, sensitive-data minimization, Shopify-like no-data behavior, local read-after-write modeling for mutations, and raw mutation preservation for commit replay.

Do not add planned-only parity specs for payment roots. Keep unsupported payment-area reads and writes as registry/workpad gaps until captured evidence can back local behavior.

### Customer payment method lifecycle and reminders

HAR-365 implements a scrubbed local staging slice for customer payment-method lifecycle roots and `paymentReminderSend`. These roots are sensitive because live success paths can involve PCI card sessions, PayPal billing-agreement IDs, remote gateway identifiers, encrypted duplication data, expiring customer-facing update URLs, destructive revocation, and customer-visible reminder email.

Runtime support is local-only:

- Credit-card create/update stages a normalized `CustomerPaymentMethod` with a `CustomerCreditCard` instrument shell and selected card fields set to `null`. The proxy does not store cardserver session IDs, billing addresses, card digits, expiry values, names, or masked numbers.
- PayPal create/update stages a `CustomerPaypalBillingAgreement` shell with `paypalAccountEmail: null` and the safe `inactive` flag where applicable. The proxy does not store PayPal billing-agreement IDs or billing addresses.
- Remote create stages an initially incomplete payment method with `instrument: null`, matching Shopify's documented asynchronous processing posture without retaining gateway customer IDs, payment method IDs, or tokens.
- `customerPaymentMethodGetDuplicationData` returns a non-secret `shopify-draft-proxy:` duplication token for an existing local payment method and target customer. Only `customerPaymentMethodCreateFromDuplicationData` in this proxy accepts that token; invalid or foreign material returns local userErrors.
- `customerPaymentMethodGetUpdateUrl` returns a non-deliverable `https://shopify-draft-proxy.local/...` URL and records the staged URL in meta state. The proxy never requests or stores Shopify's real expiring payment update URL at runtime.
- `customerPaymentMethodRevoke` sets local `revokedAt` and `revokedReason: CUSTOMER_REVOKED`; revoked methods stay hidden from `customerPaymentMethod` and `Customer.paymentMethods` unless `showRevoked: true` is selected.
- `paymentReminderSend` records a local reminder intent for `PaymentSchedule` GIDs and returns `success: true`; it does not send customer email at runtime.

Supported calls append the original raw GraphQL request to the meta log for eventual explicit commit replay. Local validation covers unknown customers, unknown/revoked payment methods, invalid synthetic duplication data, invalid remote-reference cardinality, and invalid payment schedule IDs. Validation behavior is intentionally narrow where live Shopify success/error captures are blocked by missing payment-method scopes or unsafe customer-visible side effects.

The current conformance credential probe on 2026-04-28 against `harry-test-heelo.myshopify.com` succeeded for general Admin access and confirmed these roots exist on API `2025-01`, but `currentAppInstallation.accessScopes` lacks both `read_customer_payment_methods` and `write_customer_payment_methods`. It has `write_orders`, but live `paymentReminderSend` success remains unsafe without a no-recipient or disposable-customer email plan. The executable parity evidence is therefore a local-runtime fixture, `customer-payment-method-local-staging`, which compares stable mutation payloads plus downstream payment-method reads without live payment credentials, vaulted data, real update URLs, or delivered reminders.

### Payment terms templates

`paymentTermsTemplates(paymentTermsType:)` is modeled as a read-only local catalog in snapshot and live-hybrid modes. The normalized default catalog is based on the 2025-01 `harry-test-heelo.myshopify.com` capture from 2026-04-27 and preserves Shopify's order and scalar values for:

- `Due on receipt` (`RECEIPT`, `dueInDays: null`)
- `Due on fulfillment` (`FULFILLMENT`, `dueInDays: null`)
- `Net 7`, `Net 15`, `Net 30`, `Net 45`, `Net 60`, and `Net 90` (`NET`)
- `Fixed` (`FIXED`, `dueInDays: null`)

The optional `paymentTermsType` argument filters by exact enum value. Selected template fields currently include `id`, `name`, `description`, `dueInDays`, `paymentTermsType`, `translatedName`, and `__typename`.

### Payment terms lifecycle

`paymentTermsCreate(referenceId:, paymentTermsAttributes:)`, `paymentTermsUpdate(input:)`, and `paymentTermsDelete(input:)` are modeled as local-only order/draft-order graph updates. The payment root owns the Admin API entrypoint, but the staged state lives on `Order.paymentTerms` and `DraftOrder.paymentTerms` so immediate downstream reads observe the same normalized payment terms graph and payment schedule connection serializers used by order reads.

Create supports eligible local `Order` and `DraftOrder` IDs as `referenceId`, builds a stable local `PaymentTerms` GID, and creates schedule GIDs for supplied schedules. Update locates existing local terms by `paymentTermsId`, preserves the terms ID and same-index schedule IDs, and reprojects template name/type/due-day fields from the local `paymentTermsTemplates` catalog. Delete locates terms by `paymentTermsId`, clears the owning order/draft-order field, and returns `deletedId`.

Validation is local and does not append staged-write log entries for rejected branches. Captured evidence currently covers the draft-order create-time missing-template branch (`Payment terms template id can not be empty.`) and merchant permission blocker (`The user must have access to set payment terms.`). The standalone lifecycle mutations use Shopify-documented 2026-04 argument/input shapes plus local guardrails for unknown order/draft targets, missing or unknown template IDs, invalid NET/FIXED schedule requirements, missing update IDs, and duplicate deletes. Full live happy-path parity remains future work because these mutations alter real payment schedules.

## Historical and developer notes

### Validation

- `tests/integration/payment-customization-query-shapes.test.ts`
- `tests/integration/customer-payment-method-flow.test.ts`
- `tests/integration/payment-terms-query-shapes.test.ts`
- `tests/integration/payment-terms-lifecycle-flow.test.ts`
- `config/parity-specs/payments/finance-risk-no-data-read.json`
- `config/parity-specs/payments/customer-payment-method-local-staging.json`
- `corepack pnpm conformance:capture-finance-risk`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`

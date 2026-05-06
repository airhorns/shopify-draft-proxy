# Payments

This endpoint group tracks Admin GraphQL payment-area roots whose behavior is sensitive because they can expose payment settings, payment methods, payment status, or Shopify Functions-backed checkout configuration.

Order payment transaction mutations such as `orderCapture`, `transactionVoid`, and `orderCreateMandatePayment` are modeled with the order graph because their downstream reads are `Order` financial and transaction fields. Their local validation and payment-service safety notes live in `docs/endpoints/orders.md`.

## Current support and limitations

### HAR-439 payment-sensitive order review

HAR-439 reviewed the order-adjacent payment roots in this ticket alongside the
payments endpoint group. `orderCapture`, `transactionVoid`,
`orderCreateManualPayment`, and `orderCreateMandatePayment` remain intentionally
owned by the order graph because their observable effects are order financial
status, capturable balance, received/outstanding/net payment totals, payment
gateway names, transactions, and mutation-log replay. The current executable
evidence is local runtime coverage plus order/payment parity specs, including
the mandate payment composite reference, idempotency, missing-`mandateId`, and
`autoCapture: false` authorization branches; it is not real gateway or
mandate-service execution.

The review also kept `paymentReminderSend` in the sensitive side-effect bucket:
the proxy records local reminder intent for `PaymentSchedule` IDs and never
sends customer email at runtime. Live success capture still needs a safe
disposable-customer/no-recipient plan before expanding beyond local intent and
validation behavior.

### HAR-456 fidelity review

The HAR-456 audit reviewed the scoped payment terms, payment customization,
Shopify Payments, dispute, POS cash, and Shop Pay payment-request roots against
the checked-in registry, executable parity specs, integration tests, Shopify
Admin docs/examples, and public query examples. Existing coverage is strongest
where the proxy has either scrubbed local lifecycle modeling or captured
empty/no-data fixtures:

- Payment terms now have live captured lifecycle evidence for
  `paymentTermsCreate`, `paymentTermsUpdate`, and `paymentTermsDelete` on a
  disposable draft order, plus downstream `draftOrder.paymentTerms` reads.
- Payment customizations have local lifecycle tests and captured validation plus
  empty read parity, but full live happy-path parity still depends on an
  install/grant that exposes a released `payment_customization` Shopify
  Function to the conformance store.
- Shopify Payments account support is intentionally limited to access-denied or
  no-account parity plus fixture-backed safe scalar fields and empty account
  activity connections. Balance, bank-account, payout, statement descriptor,
  dispute, payout, and balance-transaction details remain sensitive
  scrubbed-fixture gaps.
- Dispute, POS cash tracking, and Shop Pay payment-request receipt reads are
  supported only for captured null/empty behavior. Non-empty support requires
  disposable-store fixtures with minimized financial/customer-visible data and
  a normalized local state model before the roots can be treated as lifecycle
  complete.
- Public examples for POS, Shopify Payments, Shop Pay receipts, and disputes are
  mostly shape/access examples rather than safe lifecycle recipes. Do not
  promote them into parity specs unless they are backed by a real captured
  interaction and an executable proxy comparison.

External processor, POS, and customer-visible side effects remain intentionally
unemulated at runtime: Shopify Functions are not executed, POS cash sessions and
hardware state are not invented, Shopify Payments balances/payouts/bank details
are not synthesized, disputes/evidence are not fabricated, Shop Pay payment
requests are not sent, and payment-reminder emails are recorded only as local
intent unless the original raw mutation is later committed explicitly.

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
- `shopifyPaymentsAccount`

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
- `paymentCustomizationUpdate(id:, paymentCustomization:)` merges mutable scalar/metafield input over an existing normalized record. Function identifiers are immutable on update: equivalent `functionId`/`functionHandle` input is accepted as a no-op, replacement identifiers return `FUNCTION_ID_CANNOT_BE_CHANGED`, and missing Function handles return `FUNCTION_NOT_FOUND` when the local Function catalog can prove the handle is unknown. Update does not invent a `shopifyFunction` object.
- `paymentCustomizationActivation(ids:, enabled:)` toggles `enabled` on existing normalized records and returns the updated ids.
- `paymentCustomizationDelete(id:)` marks a normalized record deleted so downstream detail reads return `null` and catalog reads omit it.

Captured validation branches are modeled locally for missing create fields, missing Function id `gid://shopify/ShopifyFunction/0`, immutable Function replacement on update, unknown update/delete ids, unknown activation ids, and empty activation id lists. The latest 2026-04 docs also expose `functionHandle`, `MULTIPLE_FUNCTION_IDENTIFIERS`, `FUNCTION_NOT_FOUND`, and `INVALID_METAFIELDS`; the local model accepts Function handles, rejects the captured invalid handle sentinels, rejects requests that provide both `functionId` and `functionHandle`, rejects structurally invalid owner metafields, and validates update Function handles against the local Function catalog when one is available. The current local model does not maintain a full Shopify Function catalog; successful create paths preserve the provided Function identifier, update paths preserve the existing Function identifier, and `shopifyFunction` is returned only if that field was present in normalized state.

### Access Scopes And Capture Notes

HAR-219 recorded that the refreshed 2026-04-25 conformance app can safely read payment customization empty/null behavior with `read_payment_customizations`, and HAR-223 captured that current empty/null slice in `payment-customization-empty-read`. The same conformance credential has `write_payment_customizations`; HAR-223 captured validation/error branches for missing Function ownership and unknown ids in `payment-customization-validation`. HAR-629 captured a visible `payment_customization` Function in the 2026-04 test shop and recorded that `paymentCustomizationUpdate` rejects replacement `functionId` input with `FUNCTION_ID_CANNOT_BE_CHANGED` while downstream readback keeps the original Function identifier.

Earlier HAR-223 captures did not see released `ShopifyFunction` nodes, so broad non-empty happy-path create/update/delete/activation behavior remains local-runtime evidence unless a later scenario captures that specific branch. Non-empty detail, Function ownership, and error-history behavior should be promoted into fixtures/parity specs only after real interactions exist and the comparison contract is ready.

### Finance, Risk, Disputes, And POS Cash

HAR-316 records coverage scaffolds for the sensitive finance/risk roots `cashTrackingSession`, `cashTrackingSessions`, `financeAppAccessPolicy`, `financeKycInformation`, `pointOfSaleDevice`, `dispute`, `disputes`, `disputeEvidence`, `disputeEvidenceUpdate`, `shopPayPaymentRequestReceipt`, `shopPayPaymentRequestReceipts`, `shopifyPaymentsAccount`, `shopifyPaymentsPayoutAlternateCurrencyCreate`, and `tenderTransactions`.

The checked-in capture `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/finance-risk-access-read.json` deliberately avoids creating or exposing financial records. It records only root introspection, unknown-id or unknown-token reads, type-only connection nodes, access-denied credential blockers, an unknown-order `orderRiskAssessmentCreate` validation branch, and a non-executing missing-currency validation request for `shopifyPaymentsPayoutAlternateCurrencyCreate`.

Current 2025-01 implemented no-data coverage:

- `cashTrackingSession(id:)`, `pointOfSaleDevice(id:)`, `dispute(id:)`, and `shopPayPaymentRequestReceipt(token:)` return `null` for unknown identifiers.
- Generic `node(id:)` / `nodes(ids:)` dispatch also returns Shopify-like `null` entries for unknown `CashTrackingSession`, `PointOfSaleDevice`, and `ShopifyPaymentsDispute` GIDs. This is no-data behavior only; non-empty finance, POS, and dispute Node payloads remain unsupported until scrubbed fixtures and local state models exist.
- `cashTrackingSessions(first: 1)` and `shopPayPaymentRequestReceipts(first: 1)` return empty connections with false `pageInfo` flags on the current store.
- `disputes(first: 1)` returns an empty connection.
- `shopifyPaymentsAccount` returns `null` in the captured access-denied/no-account branch. When a normalized Business Entity fixture includes a Shopify Payments account, the direct root serializes only captured-safe account scalars and empty no-data account connections shared with `BusinessEntity.shopifyPaymentsAccount`.

Still-blocked sensitive coverage:

- `disputeEvidence(id:)` is denied without `read_shopify_payments_dispute_evidences`.
- `financeAppAccessPolicy` is denied without the required valid finance app user session/client; `financeKycInformation` is denied without `read_financial_kyc_information` plus finance-app permission.
- `disputeEvidenceUpdate` is denied without `write_shopify_payments_dispute_evidences` plus staff dispute/order permission.
- `shopifyPaymentsAccount` non-empty balance, bank account, payout schedule, statement descriptor, dispute, payout, and balance-transaction details require `read_shopify_payments` / `read_shopify_payments_accounts` style access and scrubbed disposable-store fixtures before local support expands beyond captured-safe scalar/no-data behavior.
- `tenderTransactions(first: 1)` may expose real transaction rows on the current store, so the capture selects only `__typename` and page flags. Do not add IDs, amounts, payment methods, remote references, users, or transaction details unless the fixture is deliberately scrubbed and justified.
- `shopifyPaymentsPayoutAlternateCurrencyCreate` must remain unsupported until a local payout model exists; live happy paths can alter payout configuration or money movement.

The `finance-risk-no-data-read` parity scenario enforces the implemented empty/null local overlay slice. Future support needs disposable-store fixtures, sensitive-data minimization, Shopify-like no-data behavior, local read-after-write modeling for mutations, and raw mutation preservation for commit replay.

Do not add planned-only parity specs for payment roots. Keep unsupported payment-area reads and writes as registry/workpad gaps until captured evidence can back local behavior.

### Customer payment method lifecycle and reminders

HAR-365 implements a scrubbed local staging slice for customer payment-method lifecycle roots and `paymentReminderSend`. These roots are sensitive because live success paths can involve PCI card sessions, PayPal billing-agreement IDs, remote gateway identifiers, encrypted duplication data, expiring customer-facing update URLs, destructive revocation, and customer-visible reminder email.

Runtime support is local-only:

- Credit-card create/update validates `sessionId` and `billingAddress`, requiring `address1`, `city`, `zip`, and `countryCode`/`provinceCode` with `country`/`province` fallbacks. Blank billing fields return Shopify-like `BLANK` user errors on `billing_address.<field>`.
- Successful credit-card create/update stages a normalized `CustomerPaymentMethod` with a `CustomerCreditCard` instrument shell, selected card fields set to `null`, and the scrubbed billing address needed for downstream `instrument.billingAddress` reads. The proxy does not store cardserver session IDs, card digits, expiry values, names, or masked numbers.
- Credit-card create/update normally returns `processing: false`. Local-runtime fixtures can opt into Shopify's asynchronous 3DS-like branch by using the non-secret `shopify-draft-proxy:processing` session sentinel, which returns `processing: true` and `customerPaymentMethod: null` without staging a payment method.
- PayPal create/update stages a `CustomerPaypalBillingAgreement` shell with `paypalAccountEmail: null` and the safe `inactive` flag where applicable. The proxy does not store PayPal billing-agreement IDs or billing addresses.
- Remote create stages an initially incomplete payment method with `instrument: null`, matching Shopify's documented asynchronous processing posture without retaining gateway customer IDs, payment method IDs, or tokens.
- `customerPaymentMethodGetDuplicationData` returns a non-secret `shopify-draft-proxy:` duplication token only for a Shop Pay agreement instrument and an eligible target customer/shop. Non-Shop-Pay instruments return `INVALID_INSTRUMENT` at `customerPaymentMethodId`; a target equal to the hydrated source shop returns `SAME_SHOP` at `targetShopId`. Only `customerPaymentMethodCreateFromDuplicationData` in this proxy accepts the proxy-local token; invalid or foreign material returns local userErrors.
- `customerPaymentMethodCreateFromDuplicationData` requires a non-blank billing address slice for the local duplication path. Blank `address1`, `city`, `zip`, `countryCode`, or `provinceCode` inputs return `BLANK` userErrors at Shopify-style `billing_address.*` field paths before any duplicated method is staged.
- `customerPaymentMethodGetUpdateUrl` returns a non-deliverable `https://shopify-draft-proxy.local/...` URL only for a Shop Pay agreement instrument and records the staged URL in meta state. Non-Shop-Pay instruments return `INVALID_INSTRUMENT`; the proxy never requests or stores Shopify's real expiring payment update URL at runtime.
- `customerPaymentMethodRevoke` finds active and already-revoked normalized methods. Methods with local `subscriptionContracts` links return `ACTIVE_CONTRACT` without changing local state. Already-revoked methods return the normalized `gid://shopify/CustomerPaymentMethod/<token>` id without replacing existing revoke metadata. Active methods without contracts set local `revokedAt` and `revokedReason: CUSTOMER_REVOKED`; revoked methods stay hidden from `customerPaymentMethod` and `Customer.paymentMethods` unless `showRevoked: true` is selected.
- `paymentReminderSend` records a local reminder intent only when the
  `PaymentSchedule` exists in the effective payment-terms store, is overdue,
  has no `completedAt`, and belongs to an open unpaid order. Invalid schedule
  ID formats return `INVALID_PAYMENT_SCHEDULE_ID`; unknown or ineligible
  schedules return `success: null` with `PAYMENT_REMINDER_SEND_UNSUCCESSFUL`
  and do not stage reminder intent. It does not send customer email at runtime.

Supported calls append the original raw GraphQL request to the meta log for eventual explicit commit replay. Local validation covers unknown customers, unknown/revoked payment methods, invalid synthetic duplication data, Shop Pay-only duplication/update-url guards, same-shop duplication rejection, blank duplication billing-address fields, remote-reference cardinality and required gateway fields, and invalid payment schedule IDs. `customerPaymentMethodRemoteCreate` rejects blank Stripe customer IDs, PayPal billing-agreement IDs, Braintree customer IDs/payment-method tokens, Authorize.Net customer profile IDs, and Adyen shopper/stored-payment-method IDs with `CustomerPaymentMethodRemoteUserError`-compatible `field`/`code`/`message` payloads before staging any local payment method. Throttling, remote-payment-method-id feature gating, broader address normalization, organization-shop membership beyond the same-shop guard, and live gateway eligibility checks such as Authorize.Net/Adyen subscriptions enablement remain expected differences for this local-only scrubbed slice.

The current conformance credential probe on 2026-04-28 against `harry-test-heelo.myshopify.com` succeeded for general Admin access and confirmed these roots exist on API `2025-01`, but `currentAppInstallation.accessScopes` lacks both `read_customer_payment_methods` and `write_customer_payment_methods`. The payment-method lifecycle evidence is therefore local-runtime backed: `customer-payment-method-local-staging` compares stable mutation payloads plus downstream payment-method reads without live payment credentials, vaulted data, real update URLs, or delivered reminders, and `customer-payment-method-shop-pay-guards` strictly compares the Shop Pay-only and blank billing-address guard payloads. `payment-reminder-send-eligibility` is a separate live 2025-01 parity fixture captured against disposable order-owned Net 30 payment schedules; it strictly compares success, unknown-schedule failure, and paid-schedule failure payloads while the runtime replay uses only cassette hydrate reads.

In cassette-backed LiveHybrid parity, customer payment-method mutations use a narrow Pattern 2 hydrate query before local staging. The hydrate path imports only the customer shell and selected payment-method shell required by the captured local-runtime scenario, so `customerPaymentMethod(id:)` and `Customer.paymentMethods` can observe staged changes without calling Shopify for supported mutations at runtime.

### Payment terms templates

`paymentTermsTemplates(paymentTermsType:)` is modeled as a read-only local catalog in snapshot and live-hybrid modes. The normalized default catalog is based on the 2025-01 `harry-test-heelo.myshopify.com` capture from 2026-04-27 and preserves Shopify's order and scalar values for:

- `Due on receipt` (`RECEIPT`, `dueInDays: null`)
- `Due on fulfillment` (`FULFILLMENT`, `dueInDays: null`)
- `Net 7`, `Net 15`, `Net 30`, `Net 45`, `Net 60`, and `Net 90` (`NET`)
- `Fixed` (`FIXED`, `dueInDays: null`)

The optional `paymentTermsType` argument filters by exact enum value. Selected template fields currently include `id`, `name`, `description`, `dueInDays`, `paymentTermsType`, `translatedName`, and `__typename`.

### Payment terms lifecycle

`paymentTermsCreate(referenceId:, paymentTermsAttributes:)`, `paymentTermsUpdate(input:)`, and `paymentTermsDelete(input:)` are modeled as local-only order/draft-order graph updates. The payment root owns the Admin API entrypoint, but the staged state lives on `Order.paymentTerms` and `DraftOrder.paymentTerms` so immediate downstream reads observe the same normalized payment terms graph and payment schedule connection serializers used by order reads.

Create supports eligible local or upstream-hydrated `Order` and `DraftOrder` IDs as `referenceId`, builds a stable local `PaymentTerms` GID, and creates one schedule GID for NET/FIXED schedules that materialize a due date. RECEIPT/FULFILLMENT event terms with no due date stage terms with an empty payment schedule connection, matching captured 2026-04 behavior for RECEIPT plus `issuedAt`. Schedule `amount`, `balanceDue`, and `totalBalance` derive from the owner's presentment money when available, then shop money, using `totalOutstandingSet`, `currentTotalPriceSet`, `totalPriceSet`, or `subtotalPriceSet` in that order. If no owner money is available, the local fallback is `0.0 CAD` and should be covered by an explicit parity expected difference before claiming amount fidelity for that scenario. Update locates existing local terms by `paymentTermsId`, preserves the terms ID, mints a replacement schedule ID when a materialized schedule exists, and reprojects template name/type/due-day fields from the local `paymentTermsTemplates` catalog. Delete locates terms by `paymentTermsId`, accepts either the full `gid://shopify/PaymentTerms/<id>` or the numeric tail for local records, clears the owning order/draft-order field, and returns `deletedId` as a built PaymentTerms GID.

Validation is local and does not append staged-write log entries for rejected branches. Multiple payment schedules are rejected with `PAYMENT_TERMS_CREATION_UNSUCCESSFUL` on create and `PAYMENT_TERMS_UPDATE_UNSUCCESSFUL` on update plus the Shopify message `Cannot create payment terms with multiple schedules.` Missing create references use `PAYMENT_TERMS_CREATION_UNSUCCESSFUL`; missing update/delete IDs use `PAYMENT_TERMS_UPDATE_UNSUCCESSFUL` / `payment_terms_deletion_unsuccessful`. Captured 2026-04 evidence in `payment-terms-create-template-and-schedule-validation` covers the template catalog lookup, unknown template IDs (`Could not find payment terms template.`), FIXED schedules missing `dueAt` (`A due date is required with fixed or net payment terms.`), RECEIPT schedules with `dueAt` (`A due date cannot be set with event payment terms.`), and RECEIPT `issuedAt` success with no schedule nodes. Shopify rejects omitted `paymentTermsTemplateId` during variable coercion before `paymentTermsCreate` runs; the local handler rejects inline omissions with `REQUIRED` rather than defaulting to the first template. The standalone lifecycle mutations use Shopify-documented 2026-04 argument/input shapes plus local guardrails for unknown order/draft targets, missing or unknown template IDs, invalid NET/FIXED/event schedule requirements, missing update IDs, and duplicate deletes.

For `paymentTermsCreate` against an existing captured order or draft order, the Gleam handler uses Pattern 2 hydration to read the owner from the cassette before staging local payment terms. Follow-up update/delete and downstream `Order.paymentTerms` / `DraftOrder.paymentTerms` reads then run from local state, preserving the supported-mutation rule while matching the captured owner context.

## Historical and developer notes

### Validation

- `config/parity-specs/payments/finance-risk-no-data-read.json`
- `config/parity-specs/payments/customer-payment-method-local-staging.json`
- `config/parity-specs/payments/customer-payment-method-remote-create-validation.json`
- `corepack pnpm conformance:capture-finance-risk`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`

---
title: 'Payments'
description: 'Coverage notes and fidelity boundaries for Payments.'
---

This endpoint group tracks Admin GraphQL payment-area roots whose behavior is sensitive because they can expose payment settings, payment methods, payment status, or Shopify Functions-backed checkout configuration.

Order payment transaction mutations such as `orderCapture`, `transactionVoid`, and `orderCreateMandatePayment` are modeled with the order graph because their downstream reads are `Order` financial and transaction fields. Their local validation and payment-service safety notes live in `/endpoints/orders/`.

For `orderCapture`, checked-in live public parity must use the public Admin
GraphQL payload shape: `OrderCapturePayload.userErrors` is plain `UserError`
with selectable `field` and `message` only, and the payload does not expose an
`order` field. The draft proxy still keeps a local/internal code-bearing
contract for validation branches such as currency mismatch, missing parent
transaction, invalid amount, and final-capture lock behavior; that contract is
covered by focused runtime tests rather than public live parity until a
captured schema exposes `OrderCaptureUserError.code`.

## Current support and limitations

`src/operation_registry.rs` marks selected payment roots as locally implemented
when their canonical registry entries are answered by Rust runtime handlers. The
support claims below are based on those handlers plus checked-in parity specs,
tests, and fixtures; registry presence alone is not support.

### Order-owned payment roots

Order-adjacent payment roots are tracked alongside the payments endpoint group
but remain owned by the order graph. `orderCapture`, `transactionVoid`,
`orderCreateManualPayment`, and `orderCreateMandatePayment` remain intentionally
owned by the order graph because their observable effects are order financial
status, capturable balance, received/outstanding/net payment totals, payment
gateway names, transactions, and mutation-log replay. The current executable
evidence is local runtime coverage plus order/payment parity specs, including
the mandate payment composite reference, idempotency, missing-`mandateId`, and
`autoCapture: false` authorization branches; it is not real gateway or
mandate-service execution.

`paymentReminderSend` remains in the sensitive side-effect bucket: the proxy
records local reminder intent for `PaymentSchedule` IDs and never sends customer
email at runtime. Live success capture still needs a safe
disposable-customer/no-recipient plan before expanding beyond local intent and
validation behavior.

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

In LiveHybrid mode, detail reads, catalog reads, update, activation, and delete hydrate Shopify payment customization rows through upstream reads before applying the local overlay. Effective state is the hydrated base catalog plus staged creates/updates minus staged deletes, so mutation-first clients can update, activate, or delete real Shopify IDs without issuing a preceding read. Supported mutations still stage locally and keep the original raw mutation for explicit commit replay.

Catalog reads support local cursor pagination through `first`, `last`, `after`, and `before`, plus `reverse`. Search query support is intentionally limited to captured-safe filters:

- `enabled:true|false`
- `function_id:<gid-or-tail>`
- `id:<gid-or-tail>`
- default/title text matching over captured `title`

Local cursors use the proxy's synthetic `cursor:<gid>` form. Shopify's opaque cursor encoding is not a contract clients should depend on.

Selected scalar detail fields currently include `id`, `legacyResourceId`, `title`, `enabled`, and `functionId`. `shopifyFunction` and `errorHistory` are replayed from captured normalized JSON only; when those slices are absent the serializer returns `null` rather than inventing Function ownership or failure history. Owner-scoped `metafield(namespace:, key:)` and `metafields(...)` selections serialize from the payment customization's captured metafield rows using the shared metafield serializer.

Lifecycle mutations stage against the same effective records:

- `paymentCustomizationCreate(paymentCustomization:)` creates a synthetic local `PaymentCustomization` record with selected metafields when required `title`, `enabled`, and exactly one Function identifier are present. Public 2026-04 Admin behavior accepts omitted `metafields` and accepts a sixth enabled payment customization in the captured test shop, so the draft proxy treats those public Admin semantics as authoritative. Empty `metafields: []` is also accepted. The current Admin API uses `functionHandle`; deprecated `functionId` remains accepted for captured 2025-01 parity branches. Function handles resolve through the observed Shopify Function catalog and LiveHybrid hydration; unresolved handles return `FUNCTION_NOT_FOUND`, while resolved handles are stored as the canonical `functionId`.
- `paymentCustomizationUpdate(id:, paymentCustomization:)` merges mutable scalar/metafield input over an existing effective record, hydrating the target row first in LiveHybrid mode when needed. Blank `title` input is rejected with `REQUIRED_INPUT_FIELD` and leaves the stored title unchanged. Function identifiers are immutable on update: equivalent catalog/GID `functionId`/`functionHandle` input is accepted as a no-op, replacement identifiers return `FUNCTION_ID_CANNOT_BE_CHANGED`, and unresolved Function handles return `FUNCTION_NOT_FOUND`. Update does not invent a `shopifyFunction` object.
- `paymentCustomizationActivation(ids:, enabled:)` hydrates the catalog in LiveHybrid mode before setting `enabled` on existing effective records, then returns every submitted id that resolves to an existing eligible record, including records that were already in the requested state. Missing ids are omitted from `ids` and reported through the `ids` userError bucket.
- `paymentCustomizationDelete(id:)` hydrates the target row first in LiveHybrid mode when needed, then marks the normalized record deleted so downstream detail reads return `null` and catalog reads omit it.

Captured validation branches are modeled locally for missing create fields, missing Function identifiers, missing Function id `gid://shopify/ShopifyFunction/0`, multiple Function identifiers, immutable Function replacement on update, unknown update/delete ids, unknown activation ids, mixed valid/missing activation ids, and empty activation id lists. The latest 2026-04 docs also expose `functionHandle`, `MULTIPLE_FUNCTION_IDENTIFIERS`, `FUNCTION_NOT_FOUND`, `INVALID_METAFIELDS`, and `MAXIMUM_ACTIVE_PAYMENT_CUSTOMIZATIONS`; the public 2026-04 capture for `paymentCustomizationCreate` accepted the missing-`metafields` branch and more than five active customizations, so those internal guardrails are not enforced on the public Admin draft-proxy path. The local model accepts resolved Function handles, rejects arbitrary unresolved handles, rejects requests that provide both `functionId` and `functionHandle`, validates update Function identifiers against the observed Function catalog/GID identity, and rejects structurally invalid owner metafields without staging or mutating the payment customization. Invalid metafield input returns one `INVALID_METAFIELDS` userError per invalid field at `paymentCustomization.metafields.<index>.<field>`, including `may not be empty` for missing `key`/`value`, `can't be blank` for blank `type`, and `is too short (minimum is 3 characters)` for too-short non-empty namespaces. `shopifyFunction` is returned only if that field was present in normalized state.

### Access scopes and capture notes

The 2026-04 conformance app can safely read payment customization empty/null behavior with `read_payment_customizations`, captured in `payment-customization-empty-read`. The same conformance credential has `write_payment_customizations`; `payment-customization-validation` captures validation/error branches for missing Function ownership and unknown ids. The 2026-04 test shop exposes a visible `payment_customization` Function, and checked-in captures record that `paymentCustomizationUpdate` rejects replacement `functionId` input with `FUNCTION_ID_CANNOT_BE_CHANGED` while downstream readback keeps the original Function identifier. `payment-customization-metafields-and-handle-update` captures successful metafield create/update/readback plus invalid metafield userErrors and proves the proxy's no-upstream local replay. `payment-customization-mutation-first-hydration` creates disposable real payment customizations, then captures update, activation, and delete as first proxy operations against those IDs with upstream read hydration and downstream readback comparisons. A 2026-04 public Admin capture against `harry-test-heelo.myshopify.com` recorded `paymentCustomizationCreate` accepting omitted `metafields` and a sixth enabled customization after cleaning up existing active rows; the same fixture confirms required-field errors for missing/blank `title` and missing `enabled`, plus identifier arbitration errors for both `functionId` plus `functionHandle` and for missing Function identifiers.

Earlier payment-customization captures did not see released `ShopifyFunction` nodes, so broad non-empty happy-path create/update/delete/activation behavior remains local-runtime evidence unless a later scenario captures that specific branch. Non-empty detail, Function ownership, and error-history behavior should be promoted into fixtures/parity specs only after real interactions exist and the comparison contract is ready.

### Unsupported, registry-only, and validation-only coverage

The registry tracks the sensitive finance/risk roots `cashTrackingSession`, `cashTrackingSessions`, `financeAppAccessPolicy`, `financeKycInformation`, `pointOfSaleDevice`, `dispute`, `disputes`, `disputeEvidence`, `disputeEvidenceUpdate`, `shopPayPaymentRequestReceipt`, `shopPayPaymentRequestReceipts`, `shopifyPaymentsAccount`, `shopifyPaymentsPayoutAlternateCurrencyCreate`, and `tenderTransactions`. Most of this surface is no-data, access-denied, registry-only, or validation-only coverage rather than local lifecycle support.

`cashTrackingSession`, `cashTrackingSessions`, `pointOfSaleDevice`, `dispute`, `disputes`, `shopPayPaymentRequestReceipt`, and `shopPayPaymentRequestReceipts` have a narrow local implementation only for captured-safe Snapshot no-data behavior. Cold LiveHybrid reads forward the caller's complete operation once and preserve the authoritative upstream values, GraphQL errors, HTTP status, headers, and extensions. The proxy does not normalize or overlay those finance values because it has no modeled financial records to merge.

`disputeEvidence` and `shopifyPaymentsAccount` remain unimplemented roots. LiveHybrid therefore passes them through to Shopify, including scope/access errors and any non-empty values available to the caller's credential. Snapshot rejects them as unsupported instead of turning missing local modeling or access denial into a fabricated `null` result.

The checked-in capture `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/finance-risk-access-read.json` deliberately avoids creating or exposing financial records. It records only root introspection, unknown-id or unknown-token reads, type-only connection nodes, access-denied credential blockers, an unknown-order `orderRiskAssessmentCreate` validation branch, and a non-executing missing-currency validation request for `shopifyPaymentsPayoutAlternateCurrencyCreate`.

Current 2025-01 fixture-backed no-data coverage:

- `cashTrackingSession(id:)`, `pointOfSaleDevice(id:)`, `dispute(id:)`, and `shopPayPaymentRequestReceipt(token:)` return `null` for unknown identifiers.
- Generic `node(id:)` / `nodes(ids:)` dispatch also returns Shopify-like `null` entries for unknown `CashTrackingSession`, `PointOfSaleDevice`, and `ShopifyPaymentsDispute` GIDs. This is no-data behavior only; non-empty finance, POS, and dispute Node payloads remain unsupported until scrubbed fixtures and local state models exist.
- `cashTrackingSessions(first: 1)` and `shopPayPaymentRequestReceipts(first: 1)` return empty connections with false `pageInfo` flags on the current store.
- `disputes(first: 1)` returns an empty connection.
- `disputeEvidence(id:)` and `shopifyPaymentsAccount` return `null` data alongside captured `ACCESS_DENIED` errors for the current credential; those are access results, not no-data evidence, and are not synthesized in Snapshot.

Still-blocked sensitive coverage:

- `disputeEvidence(id:)` is denied without `read_shopify_payments_dispute_evidences`.
- `financeAppAccessPolicy` is denied without the required valid finance app user session/client; `financeKycInformation` is denied without `read_financial_kyc_information` plus finance-app permission.
- `disputeEvidenceUpdate` is denied without `write_shopify_payments_dispute_evidences` plus staff dispute/order permission.
- `shopifyPaymentsAccount` non-empty balance, bank account, payout schedule, statement descriptor, dispute, payout, and balance-transaction details require `read_shopify_payments` / `read_shopify_payments_accounts` style access and scrubbed disposable-store fixtures before local modeling can be claimed.
- `tenderTransactions(first: 1)` may expose real transaction rows on the current store, so the capture selects only `__typename` and page flags. Do not add IDs, amounts, payment methods, remote references, users, or transaction details unless the fixture is deliberately scrubbed and justified.
- `shopifyPaymentsPayoutAlternateCurrencyCreate` must remain unsupported until a local payout model exists; live happy paths can alter payout configuration or money movement.

The `finance-risk-no-data-read` parity scenario replays the exact combined request through a cold LiveHybrid cassette and compares the safe no-data fields plus the captured `disputeEvidence` access-denied error. `shopify-payments-account-read` separately compares the direct account root's captured null data and access-denied error. Snapshot no-data behavior is limited to the roots with explicit captured empty/unknown-identifier evidence.

Do not add planned-only parity specs for payment roots. Keep unsupported payment-area reads and writes as registry/workpad gaps until captured evidence can back local behavior.

### Customer payment method lifecycle and reminders

The scrubbed local staging slice for customer payment-method lifecycle roots and `paymentReminderSend` is intentionally narrow. These roots are sensitive because live success paths can involve PCI card sessions, PayPal billing-agreement IDs, remote gateway identifiers, encrypted duplication data, expiring customer-facing update URLs, destructive revocation, and customer-visible reminder email.

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
  has no `completedAt`, and belongs to an open unpaid order with a contact
  email. It rejects order-owned schedules with any observed selling-plan line,
  and a second reminder for the same order inside the synthetic 24-hour window.
  Empty and non-GID
  `paymentScheduleId` variables are rejected before resolver execution with
  Shopify's top-level `INVALID_VARIABLE` GraphQL coercion envelope and no
  `paymentReminderSend` payload; Shopify GIDs for a non-`PaymentSchedule`
  resource return top-level `RESOURCE_NOT_FOUND` with
  `data.paymentReminderSend: null`. Well-formed `PaymentSchedule` GIDs that are
  unknown or ineligible return `success: null` with
  `PAYMENT_REMINDER_SEND_UNSUCCESSFUL` and do not stage reminder intent. It
  does not send customer email at runtime.

Supported calls append the original raw GraphQL request to the meta log for eventual explicit commit replay. Local validation covers unknown customers, unknown/revoked payment methods, invalid synthetic duplication data, Shop Pay-only duplication/update-url guards, same-shop duplication rejection, blank duplication billing-address fields, remote-reference cardinality and required gateway fields, and invalid payment schedule IDs. `customerPaymentMethodRemoteCreate` rejects blank Stripe customer IDs, PayPal billing-agreement IDs, Braintree customer IDs/payment-method tokens, Authorize.Net customer profile IDs, and Adyen shopper/stored-payment-method IDs with `CustomerPaymentMethodRemoteUserError`-compatible `field`/`code`/`message` payloads before staging any local payment method. These payment-method branches are covered by focused Rust runtime tests rather than parity specs until they can be captured from Shopify with the required scopes and scrubbed payment material. Throttling, remote-payment-method-id feature gating, broader address normalization, organization-shop membership beyond the same-shop guard, and live gateway eligibility checks such as Authorize.Net/Adyen subscriptions enablement remain expected differences for this local-only scrubbed slice.

The current conformance credential probe on 2026-07-02 against `harry-test-heelo.myshopify.com` succeeded for general Admin access, but the active install still lacks both `read_customer_payment_methods` and `write_customer_payment_methods`. The checked-in conformance app config now requests those scopes, and `customer-payment-method-access-probe` records the unattended `shopify app deploy --allow-updates` attempt being blocked by interactive device-code login plus the active token's missing scopes and representative access-denied payment-method probes. The former customer-payment-method local-runtime parity specs were removed from parity evidence for that reason; the runtime tests in `tests/graphql_routes/orders.rs` are the executable coverage for this local-only scrubbed slice. `payment-reminder-send-shape` captures Shopify's live schema rejection for selecting `customerPaymentMethod` on `PaymentReminderSendPayload`; `payment-reminder-send-malformed-gid` captures the live top-level coercion/resource-not-found envelopes for empty, non-GID, and wrong-resource `paymentScheduleId` variables. `payment-reminder-send-eligibility` is a separate live 2025-01 parity fixture captured against disposable order-owned Net 30 payment schedules; it strictly compares success, unknown-schedule failure, and paid-schedule failure payloads while the runtime replay uses exact GraphQL hydrate cassettes. `payment-reminder-send-additional-guards` captures the public Admin-reproducible blank-email and one-per-24h rate-limit branches with exact GraphQL hydrate cassettes. The selling-plan guard is covered by local runtime tests using a created order whose staged line item includes the same `sellingPlan` shape that live schedule hydration returns. Capture-at-fulfillment and unsent PaymentCollection reminder guards are not claimed until those states are modeled from the local order/payment graph.

In live-hybrid runtime, customer payment-method mutations can use narrow Pattern 2 hydration before local staging when upstream context is available. That path must not be promoted back into parity evidence until a real Shopify capture can provide exact GraphQL cassette reads for customer and payment-method ownership.

### Payment terms templates

`paymentTermsTemplates(paymentTermsType:)` is modeled as a read-only local catalog in snapshot and live-hybrid modes. The normalized default catalog is based on the 2025-01 `harry-test-heelo.myshopify.com` capture from 2026-04-27 and preserves Shopify's order and scalar values for:

- `Due on receipt` (`RECEIPT`, `dueInDays: null`)
- `Due on fulfillment` (`FULFILLMENT`, `dueInDays: null`)
- `Net 7`, `Net 15`, `Net 30`, `Net 45`, `Net 60`, and `Net 90` (`NET`)
- `Fixed` (`FIXED`, `dueInDays: null`)

The optional `paymentTermsType` argument filters by exact enum value. Selected template fields currently include `id`, `name`, `description`, `dueInDays`, `paymentTermsType`, `translatedName`, and `__typename`.

### Payment terms lifecycle

`paymentTermsCreate(referenceId:, paymentTermsAttributes:)`, `paymentTermsUpdate(input:)`, and `paymentTermsDelete(input:)` are modeled as local-only order/draft-order graph updates. The payment root owns the Admin API entrypoint, but the staged state lives on `Order.paymentTerms` and `DraftOrder.paymentTerms` so immediate downstream reads observe the same normalized payment terms graph and payment schedule connection serializers used by order reads.

Create supports eligible local or upstream-hydrated `Order` and `DraftOrder` IDs as `referenceId`, builds a stable local `PaymentTerms` GID, and creates one schedule GID for NET/FIXED schedules that materialize a due date. Successful create/update payloads reproject `paymentTermsName`, `paymentTermsType`, `dueInDays`, and `translatedName` from the resolved local `paymentTermsTemplates` catalog, including FIXED, non-30 NET templates, and FULFILLMENT. RECEIPT/FULFILLMENT event terms with no due date stage terms with an empty payment schedule connection, matching captured 2026-04 behavior. For materialized, non-completed schedules, `PaymentSchedule.due` is computed from `dueAt <=` the current proxy clock; `PaymentTerms.due` and `PaymentTerms.overdue` roll up to true when any local schedule is due, and stay false for future-due schedule sets. Mutation payloads and downstream `Order.paymentTerms` / `DraftOrder.paymentTerms` reads reproject those due flags from the staged schedule dates so long-lived staged terms continue to transition as time advances. Schedule `amount`, `balanceDue`, and `totalBalance` derive from the owner's presentment money when available, then shop money, using `totalOutstandingSet`, `currentTotalPriceSet`, `totalPriceSet`, or `subtotalPriceSet` in that order. The payment-terms-local `orderCreate` path computes `currentTotalPriceSet`, `totalPriceSet`, and `totalOutstandingSet` from submitted line-item unit prices times quantities; if no line price is present, it falls back to neutral zero in the order currencies. If no owner money is available for standalone schedule materialization, the local fallback is `0.0 CAD` and should be covered by an explicit parity expected difference before claiming amount fidelity for that scenario. Update locates existing local or upstream-hydrated terms by `paymentTermsId`, preserves the terms ID, mints a replacement schedule ID when a materialized schedule exists, and reprojects template name/type/due-day fields from the local `paymentTermsTemplates` catalog. Delete locates terms by `paymentTermsId`, accepts either the full `gid://shopify/PaymentTerms/<id>` or the numeric tail for local records, clears the owning order/draft-order field, and returns `deletedId` as a built PaymentTerms GID.

Validation is local and does not append staged-write log entries for rejected branches. Multiple payment schedules are rejected with `field: null`, `PAYMENT_TERMS_CREATION_UNSUCCESSFUL` on create, `PAYMENT_TERMS_UPDATE_UNSUCCESSFUL` on update, and the Shopify message `Cannot create payment terms with multiple payment schedules.` Missing `Order` and `DraftOrder` create references use `PAYMENT_TERMS_CREATION_UNSUCCESSFUL`, `field: null`, and the Shopify messages `Cannot find the specific Order with id <numeric id>.` / `Cannot find the specific Draft order with id <numeric id>.`; `payment-terms-create-reference-not-found` captures this 2026-04 behavior. Missing update IDs use `field: null`, `Could not find payment terms.`, and `PAYMENT_TERMS_UPDATE_UNSUCCESSFUL`; `payment-terms-create-on-order` and the retained `payment-terms-update-missing-local-runtime` scenario capture this live 2026-04 payload. Missing delete IDs use `field: null`, `Could not find payment terms.`, and `PAYMENT_TERMS_DELETE_UNSUCCESSFUL`; `payment-terms-delete-not-found` captures this 2026-04 behavior.

Template schedule validation is derived from the resolved template `paymentTermsType`: FIXED requires `dueAt`, NET requires an issued or explicit due date, and RECEIPT/FULFILLMENT reject supplied `dueAt` with the event-terms message. Captured 2026-04 evidence in `payment-terms-create-template-and-schedule-validation` covers the template catalog lookup, unknown template IDs (`Could not find payment terms template.`), FIXED schedules missing `dueAt` (`A due date is required with fixed or net payment terms.`), RECEIPT and FULFILLMENT schedules with `dueAt` (`A due date cannot be set with event payment terms.`), and RECEIPT `issuedAt` success with no schedule nodes. `payment-terms-create-template-reprojection` covers successful FIXED, Net 7, and FULFILLMENT create reprojection, materialized vs empty schedule-node shape, and multi-line `Order.currentTotalPriceSet` computation from line-item prices times quantities; the Net 7 branch uses Shopify's accepted `issuedAt` input because the current 2026-04 target rejects dueAt-only NET schedules.

The `payment-terms-create-order-eligibility` capture covers Order-owner eligibility: paid Orders reject with `field: null`, `PAYMENT_TERMS_CREATION_UNSUCCESSFUL`, and `Cannot create payment terms on an Order that has already been paid in full.` before staging; unpaid closed and cancelled Orders were accepted by the public 2026-04 Admin API and remain accepted locally. `payment-terms-create-on-order` captures disposable Order-owned create, multiple-schedule create validation, missing update, and missing delete branches against live Shopify. `payment-terms-delete-owner-cascade` captures disposable DraftOrder and Order owners through public Admin GraphQL, deletes created payment terms, and verifies owner reads return `paymentTerms: null` after deletion. `payment-terms-update-order-eligibility` captures the update-side paid-Order guard: after an existing Order-owned `PaymentTerms` record is hydrated by `paymentTermsId`, a fully paid owner returns `field: null`, `PAYMENT_TERMS_UPDATE_UNSUCCESSFUL`, and the same paid-in-full message without staging an update. Local create and update also honor explicit channel-policy hints on Order data (`paymentTermsAllowed`, `payment_terms_allowed`, `__draftProxyPaymentTermsAllowed: false`, or matching `customAttributes`) and return the Shopify channel-policy message with the create/update unsuccessful code; the current conformance shop did not expose a public order-create path for a channel-disallowed sales channel, so that branch is runtime-test-backed until a live fixture can be captured. DraftOrder owners skip these Order-only paid-status and channel-policy guards.

Shopify rejects omitted standalone `paymentTermsCreate.paymentTermsAttributes.paymentTermsTemplateId` during GraphQL variable coercion before the local lifecycle handler runs, so the proxy returns the same top-level `INVALID_VARIABLE` envelope without staging or logging a create. Public 2026-04 introspection shows `PaymentTermsInput.paymentTermsTemplateId` is nullable for `paymentTermsUpdate`; a live disposable-draft capture confirms omitting it on an existing Net 30 payment term succeeds and recomputes the schedule from the supplied `issuedAt`. The standalone lifecycle mutations use Shopify-documented 2026-04 argument/input shapes plus local guardrails for unknown order/draft targets, unknown template IDs, invalid NET/FIXED/event schedule requirements, missing update IDs, and duplicate deletes.

For `paymentTermsCreate` against an existing captured order or draft order, the Rust runtime uses narrow cassette-backed owner hydration before staging local payment terms. For `paymentTermsUpdate` and `paymentTermsDelete`, the local path can hydrate an existing `PaymentTerms` node by `paymentTermsId`, including owner and schedule context, before deciding whether to reject or stage locally. A cold LiveHybrid delete performs one query-only node hydrate; an authoritative `paymentTerms: null` result receives the captured missing-delete user error and is retained across dump/restore so repeated operations do not query upstream again, while transport or GraphQL failures remain top-level errors and are never cached as missing. A successful delete records PaymentTerms and PaymentSchedule tombstones, stages the hydrated Order or DraftOrder with `paymentTerms: null`, and keeps the original delete mutation for explicit commit replay. Snapshot mode never hydrates upstream. Downstream owner reads and generic Node reads therefore remain local and return the captured null results after deletion without sending the delete mutation to Shopify.

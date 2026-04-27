# Customers Endpoint Group

The customers group has implemented local slices, but the whole registry domain is not complete yet. Keep new customer-specific quirks here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `customer`
- `customers`
- `customersCount`
- `customerByIdentifier`
- `customerMergePreview`
- `customerMergeJobStatus`
- `customerPaymentMethod` from seeded/snapshot or locally hydrated state only; formal live conformance support remains blocked on `read_customer_payment_methods`
- `storeCreditAccount` from seeded/snapshot or locally staged state only

Local staged mutations:

- `customerCreate`
- `customerUpdate`
- `customerDelete`
- `customerAddressCreate`
- `customerAddressUpdate`
- `customerAddressDelete`
- `customerUpdateDefaultAddress`
- `customerEmailMarketingConsentUpdate`
- `customerSmsMarketingConsentUpdate`
- `customerGenerateAccountActivationUrl`
- `customerSendAccountInviteEmail`
- `customerPaymentMethodSendUpdateEmail`
- `customerSet`
- `customerMerge`
- `customerAddTaxExemptions`
- `customerRemoveTaxExemptions`
- `customerReplaceTaxExemptions`
- `storeCreditAccountCredit`
- `storeCreditAccountDebit`

## Unsupported roots still tracked by the registry

- `customerPaymentMethodGetUpdateUrl`

## Behavior notes

- Customer-domain state deliberately stays narrower than the product model, but it is still normalized.
- `CustomerRecord` carries scalar/detail fields plus `taxExemptions` as a separate list from the boolean `taxExempt`, and `dataSaleOptOut` for the privacy-domain `dataSaleOptOut` mutation's downstream read effect.
- Customer-owned metafields live in `customerMetafields` instead of reusing product-domain metafield storage or broadening shared `metafieldsSet` owner support without separate customer-domain evidence.
- Staged `customerUpdate(input.metafields)` computes against the effective customer metafield set and replaces the staged customer-owned set, so downstream `customer.metafield(...)` and `customer.metafields(...)` reads stay consistent.
- `customerByIdentifier(identifier:)` resolves from the same effective normalized customer graph as `customer(id:)` and `customers`, including staged customer creates/updates and hydrated live-hybrid customers.
- In Admin GraphQL 2026-04, customer marketing consent readback is conformance-backed through `Customer.defaultEmailAddress` and `Customer.defaultPhoneNumber` marketing fields. Staged email/SMS consent updates keep those default contact fields and the compatibility `emailMarketingConsent` / `smsMarketingConsent` serializers aligned for downstream reads.
- `customerEmailMarketingConsentUpdate` and `customerSmsMarketingConsentUpdate` now model the HAR-287 validation matrix: GraphQL-variable failures for missing/null nested consent payloads, null required `marketingState`, invalid enum values, invalid `DateTime`, and unsupported input fields; resolver-level rejection for `NOT_SUBSCRIBED` / `REDACTED` input states; `PENDING` requiring `CONFIRMED_OPT_IN`; future timestamp userErrors; and the SMS no-default-phone userError branch. Validation failures do not mutate normalized customer consent state.
- Email and SMS validation payload shapes intentionally differ where Shopify differs: unknown email customers report `field: ["input", "customerId"]` / `code: "INVALID"`, unknown SMS customers report null field/code, future timestamp email failures return the unchanged customer payload, and future timestamp/SMS no-phone failures return `customer: null`.
- Customer-owned addresses live in normalized `customerAddresses` state. Staged `customerAddressCreate`, `customerAddressUpdate`, `customerAddressDelete`, and `customerUpdateDefaultAddress` mutate that address graph locally and keep `Customer.defaultAddress` synchronized with the selected default row.
- `Customer.addresses` and `Customer.addressesV2` serialize from the effective normalized address graph; `addressesV2` preserves hydrated connection cursors where captured and returns Shopify-like empty connections when no address records exist.
- Captured Admin GraphQL 2025-01 evidence for customer address lifecycle uses `MailingAddressInput`, payload field `address`, and delete payload field `deletedAddressId`. Unknown customers return payload `userErrors` with `field: ["customerId"]`; unknown address IDs on update/delete/default roots return top-level `RESOURCE_NOT_FOUND` GraphQL errors with `data.<root>: null`. Address IDs that exist but belong to a different customer return payload `userErrors` with `field: ["addressId"]` instead: update/delete return `address: null` or `deletedAddressId: null`, while default-address selection returns the unchanged customer plus the userError.
- `MailingAddressInput` blank and normalization behavior is fixture-backed for the local slice: `{}` creates a blank address, inherited customer `firstName` / `lastName` populate missing or blank address names, empty address strings normalize to `null`, invalid `countryCode` / country-specific `provinceCode` values return payload userErrors from the generated Shopify Atlas country/zone metadata, and arbitrary postal text is accepted.
- Duplicate `customerAddressCreate` payloads are rejected with `field: ["address"]` and `Address already exists`. Duplicate entries submitted through `customerSet(input.addresses)` are coalesced during replacement without a userError, matching the captured replacement-list behavior.
- Deleting the current default address promotes the next remaining normalized address to `Customer.defaultAddress`; `setAsDefault: false` and omitted/null `setAsDefault` do not replace an existing default. The capture did not find a maximum-address failure through 105 created addresses, so local staging does not impose a smaller artificial limit.
- Customer payment method reads are modeled from normalized `customerPaymentMethods` state only. The local serializer supports `customerPaymentMethod(id:, showRevoked:)` and `Customer.paymentMethods(showRevoked:)`; revoked rows are hidden unless `showRevoked: true` is selected.
- `CustomerPaymentMethod.instrument` is stored as a selected union payload keyed by `__typename`, so seeded fixtures can serialize credit-card and PayPal billing-agreement fragments without local vaulting. `subscriptionContracts` on the payment method is serialized as a normal connection from seeded link rows; the customer-level `subscriptionContracts` field remains empty/no-data until separately modeled.
- Customer payment method writes remain unsupported scaffolds except for `customerPaymentMethodSendUpdateEmail`, which is buffered locally and retained for commit replay rather than delivered at runtime. Credit-card, PayPal billing-agreement, remote-create, duplication-data, update-url, and revoke roots require `write_customers` plus `write_customer_payment_methods` and are sensitive because they can involve vaulted instruments, expiring payment links, destructive revocation, asynchronous gateway polling, or customer-visible flows.
- Store credit accounts are modeled as sensitive balance records. Snapshot mode never creates a store credit account merely because a customer exists: `Customer.storeCreditAccounts` remains an empty connection until normalized account state is seeded or a local mutation updates an existing account, and `storeCreditAccount(id:)` returns `null` for unknown IDs.
- `storeCreditAccountCredit` and `storeCreditAccountDebit` stage locally only for existing normalized store credit accounts. They update the account balance and append local `StoreCreditAccountTransaction` rows so direct account reads and nested customer account reads observe the changed balance without runtime Shopify writes. Debit staging rejects insufficient local funds; both roots reject currency mismatches and unknown account IDs locally.
- HAR-317 live evidence on 2026-04-27 used the canonical conformance auth helper against `harry-test-heelo.myshopify.com`: schema introspection captured the account fields (`id`, `balance`, `owner`, `transactions`), transaction fields (`account`, `amount`, `balanceAfterTransaction`, `createdAt`, `event`, `origin`), credit/debit input shapes, and payload fields. The recorder now creates a disposable customer, calls `storeCreditAccountCredit` with that customer ID so Shopify creates a real store credit account, captures account-ID credit/debit mutations plus downstream direct/nested balance reads, debits the remaining captured balance back to zero, and deletes the disposable customer.
- Captured Admin GraphQL 2025-01 evidence for `customerCreate` and `customerUpdate` now covers the first CustomerInput validation long tail. The local model reproduces payload `userErrors` for invalid email (`["email"]`, `Email is invalid`), invalid phone (`["phone"]`, `Phone is invalid`), duplicate email/phone (`Email has already been taken`, `Phone has already been taken`), invalid locale (`["locale"]`, `Locale is invalid`), oversized tags (`["tags"]`, 255-character limit), oversized first/last name (255-character limit), and oversized note (5000-character limit). Failed validations return `customer: null` and do not mutate normalized customer rows or customer-owned metafields.
- Captured successful CustomerInput normalization trims blank `firstName`/`lastName`/`phone` to `null`, preserves blank `note` as an empty string, preserves explicit `null` scalar inputs as `null` on update, defaults created-customer `locale` to `en` when omitted, and trims, de-duplicates, and lexicographically sorts tags. These normalized values are reflected in mutation payloads and downstream `customer`, `customerByIdentifier`, and `customers` reads.
- Updating a deleted customer or a customer that was merged away returns the same supported unknown-id branch as other missing customers: `customer: null` plus `userErrors[{ field: ["id"], message: "Customer does not exist" }]`.
- Customer order-summary reads have a deliberately narrow order-domain bridge. Captured Admin GraphQL 2026-04 evidence for `orderCustomerSet` / `orderCustomerRemove` shows immediate customer reads expose the linked order through `Customer.orders`, then return to an empty order connection after removal, but `numberOfOrders`, `amountSpent`, and `lastOrder` remain at the customer record's existing summary values in the immediate read. The proxy therefore derives `Customer.orders` from normalized effective `OrderRecord.customer` relationships and leaves customer-owned summary scalars on `CustomerRecord` unless they were hydrated from Shopify customer data.
- Captured Admin GraphQL 2026-04 evidence for `customerSet` supports a local slice: create without `identifier`, update by `identifier.id`, and upsert/update by `identifier.email` or `identifier.phone`. The staged input slice is `email`, `firstName`, `lastName`, `locale`, `note`, `phone`, `tags`, `taxExempt`, `taxExemptions`, and address-list replacement when the identifier resolves an existing customer.
- No-identifier `customerSet` creates reject duplicate native contact identifiers locally: existing email returns `field: ["input", "email"]` / `Email has already been taken`, and existing phone returns `field: ["input", "phone"]` / `Phone has already been taken`.
- `identifier.email` and `identifier.phone` require the corresponding input field. Missing values return `The input field corresponding to the identifier is required.` and mismatched values return `The identifier value does not match the value of the corresponding field in the input.` at `field: ["input"]`.
- `customerSet(input.addresses)` is treated as a replacement list for an existing customer: current normalized addresses are removed, input mailing addresses are staged, duplicate replacement rows are coalesced, and `Customer.defaultAddress` follows the first replacement address or `null` for an empty list. A nullable address list (`addresses: null`) is a no-op. Local replay uses stable synthetic address cursors; live Shopify cursors are opaque and are only parity-comparable as expected differences.
- Captured nullable input behavior remains narrow: `taxExempt: null` returns `field: ["input", "taxExempt"]` / `Tax exempt is of unexpected type NilClass`; `taxExemptions: null` is tolerated by the captured branch when `taxExempt` is the only reported validation error.
- `customerSet(identifier.customId)` remains unsupported without modeled unique metafield definitions. The local response mirrors the captured top-level `NOT_FOUND` error instead of proxying the supported root upstream.
- Unsupported `customerSet` input or identifier fields return local `userErrors`, keeping the root in the supported local-staging path without silently claiming unmodeled behavior.
- Staged `customerMerge` updates the normalized resulting customer row, marks the source customer deleted, records the source-to-result redirect in `mergedCustomerIds`, and records the observed merge job/result shape in `customerMergeRequests`.
- `customerMergePreview` and `customerMergeJobStatus` resolve from normalized customer/merge-request state. The first local merge slice supports customers already present in staged state or hydrated base state and does not fetch unknown customer ids during the supported mutation path.
- HAR-291 attached-resource capture is a dedicated scenario separate from the base customer merge parity fixture. It extends local `customerMerge` staging for normalized customer-owned addresses, customer-owned metafields, and normalized orders. Shopify selected `customerTwoId` as the resulting customer, kept the result customer's default address, appended the source customer's address to `addressesV2`, preserved result-side metafield conflicts, copied source-only metafields under a new metafield id, and moved the source order to the result customer with the result customer's email. `numberOfOrders` and `lastOrder` stayed at the result customer's existing summary values in the captured immediate post-completion reads.
- HAR-291 polling evidence observed `customerMergeJobStatus` as `IN_PROGRESS` immediately after the mutation and `COMPLETED` on the next poll. The local merge request is still recorded as completed immediately after staging because the local mutation has no asynchronous worker, but the fixture keeps the live polling samples for future asynchronous fidelity work.
- Draft-order setup is present in the HAR-291 capture, but downstream draft-order transfer was not captured through a customer/draft-order read. Local `customerMerge` therefore deliberately does not claim draft-order transfer support yet; keep draft orders, gift cards, discounts, and other unmodeled attached resources deferred until a fixture captures their non-empty downstream behavior.
- Captured Admin GraphQL 2025-01 evidence for `customerAddTaxExemptions`, `customerRemoveTaxExemptions`, and `customerReplaceTaxExemptions` stages against `Customer.taxExemptions` only; it does not flip the separate `taxExempt` boolean. Add/remove preserve the existing exemption order and de-duplicate inputs; empty add/remove lists are no-ops; replace de-duplicates inputs and an empty replace clears the list. Unknown customers return payload `userErrors` at `["customerId"]` with `Customer does not exist.` Invalid enum variables are top-level `INVALID_VARIABLE` GraphQL errors before payload execution.
- The `dataSaleOptOut` root remains documented under the privacy endpoint group, but its local read-after-write state is stored on `CustomerRecord`. Existing-email opt-out flips `Customer.dataSaleOptOut` to `true`; unknown valid emails create a local opted-out customer; invalid email strings return the captured `FAILED` userError shape.

## Outbound email and activation buffering

The customer surface includes roots that look related but have different safety profiles:

- `customerEmailMarketingConsentUpdate` and `customerSmsMarketingConsentUpdate` are customer state changes, not outbound notification sends. They are safe to stage locally once captured behavior is modeled because downstream reads can observe the changed consent fields.
- `customerGenerateAccountActivationUrl` returns a sensitive, expiring customer-facing link. Runtime support synthesizes a non-deliverable local activation URL and keeps the original raw mutation for commit replay instead of asking Shopify for a live URL.
- `customerSendAccountInviteEmail` and `customerPaymentMethodSendUpdateEmail` are explicit outbound email side effects. These can still be supported because the proxy buffers the original mutations locally and does not deliver email at runtime; delivery happens only if the staged mutation log is committed to Shopify.
- `customerPaymentMethodSendUpdateEmail` validates that the payment method is present in the local customer-payment graph before buffering. It returns the associated customer when that ownership edge is known locally, and otherwise mirrors Shopify's not-found userError instead of claiming delivery for an unknown payment method.
- `customerPaymentMethodGetUpdateUrl` remains unsupported because the proxy does not yet model customer payment-method ownership or synthesize payment-method update URLs.

Do not mark outbound email roots implemented by proxying them upstream. Support means local validation/buffering plus original raw mutation retention for commit-time replay; it does not require pretending the runtime email or URL side effect already happened in Shopify.

## Validation anchors

- Customer reads: `tests/integration/customer-query-shapes.test.ts`
- Customer mutations, `customerSet`, and merge slices: `tests/integration/customer-draft-flow.test.ts`
- Store credit account read and local balance staging: `tests/integration/store-credit-flow.test.ts`
- Store credit account success-path capture: `corepack pnpm conformance:capture-store-credit`, writing `store-credit-account-parity.json`
- Customer address lifecycle capture: `corepack pnpm conformance:capture-customer-addresses`, writing `fixtures/conformance/<store>/<version>/customer-address-lifecycle.json`
- CustomerSet capture: `corepack pnpm conformance:capture-customer-set`, writing `fixtures/conformance/<store>/<version>/customer-set-parity.json`
- CustomerInput validation capture: `corepack pnpm conformance:capture-customer-input-validation`, writing `fixtures/conformance/<store>/<version>/customer-input-validation-parity.json`
- Customer consent capture: `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:capture-customer-consent`, writing `customer-email-marketing-consent-update-parity.json` and `customer-sms-marketing-consent-update-parity.json` with strict parity branches plus the HAR-287 validation matrix.
- Customer tax exemption capture: `corepack pnpm conformance:capture-customer-tax-exemptions`, writing `customer-add-tax-exemptions-parity.json`, `customer-remove-tax-exemptions-parity.json`, and `customer-replace-tax-exemptions-parity.json`
- Data sale opt-out capture: `corepack pnpm conformance:capture-data-sale-opt-out`, writing `data-sale-opt-out-parity.json`
- Customer order-summary capture: `corepack pnpm conformance:capture-customer-order-summary`, writing `customer-order-summary-read-effects.json`
- Customer merge base capture: `corepack pnpm conformance:capture-customer-merge`, writing `customer-merge-parity.json`
- Customer merge attached-resource capture: `corepack pnpm conformance:capture-customer-merge-attached-resources`, writing `customer-merge-attached-resources-parity.json`
- Conformance fixtures and requests: `config/parity-specs/customer*.json`, `config/parity-specs/customers*.json`, and matching files under `config/parity-requests/`

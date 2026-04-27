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

## Unsupported roots still tracked by the registry

- `customerPaymentMethodGetUpdateUrl`

## Behavior notes

- Customer-domain state deliberately stays narrower than the product model, but it is still normalized.
- `CustomerRecord` carries scalar/detail fields plus `taxExemptions` as a separate list from the boolean `taxExempt`, and `dataSaleOptOut` for the privacy-domain `dataSaleOptOut` mutation's downstream read effect.
- Customer-owned metafields live in `customerMetafields` instead of reusing product-domain metafield storage or broadening shared `metafieldsSet` owner support without separate customer-domain evidence.
- Staged `customerUpdate(input.metafields)` computes against the effective customer metafield set and replaces the staged customer-owned set, so downstream `customer.metafield(...)` and `customer.metafields(...)` reads stay consistent.
- `customerByIdentifier(identifier:)` resolves from the same effective normalized customer graph as `customer(id:)` and `customers`, including staged customer creates/updates and hydrated live-hybrid customers.
- Customer-owned addresses live in normalized `customerAddresses` state. Staged `customerAddressCreate`, `customerAddressUpdate`, `customerAddressDelete`, and `customerUpdateDefaultAddress` mutate that address graph locally and keep `Customer.defaultAddress` synchronized with the selected default row.
- `Customer.addresses` and `Customer.addressesV2` serialize from the effective normalized address graph; `addressesV2` preserves hydrated connection cursors where captured and returns Shopify-like empty connections when no address records exist.
- Captured Admin GraphQL 2025-01 evidence for customer address lifecycle uses `MailingAddressInput`, payload field `address`, and delete payload field `deletedAddressId`. Unknown customers return payload `userErrors` with `field: ["customerId"]`; unknown address IDs on update/delete/default roots return top-level `RESOURCE_NOT_FOUND` GraphQL errors with `data.<root>: null`. Address IDs that exist but belong to a different customer return payload `userErrors` with `field: ["addressId"]` instead: update/delete return `address: null` or `deletedAddressId: null`, while default-address selection returns the unchanged customer plus the userError.
- `MailingAddressInput` blank and normalization behavior is fixture-backed for the local slice: `{}` creates a blank address, inherited customer `firstName` / `lastName` populate missing or blank address names, empty address strings normalize to `null`, invalid `countryCode` / country-specific `provinceCode` values return payload userErrors from the generated Shopify Atlas country/zone metadata, and arbitrary postal text is accepted.
- Duplicate `customerAddressCreate` payloads are rejected with `field: ["address"]` and `Address already exists`. Duplicate entries submitted through `customerSet(input.addresses)` are coalesced during replacement without a userError, matching the captured replacement-list behavior.
- Deleting the current default address promotes the next remaining normalized address to `Customer.defaultAddress`; `setAsDefault: false` and omitted/null `setAsDefault` do not replace an existing default. The capture did not find a maximum-address failure through 105 created addresses, so local staging does not impose a smaller artificial limit.
- Customer payment method reads are modeled from normalized `customerPaymentMethods` state only. The local serializer supports `customerPaymentMethod(id:, showRevoked:)` and `Customer.paymentMethods(showRevoked:)`; revoked rows are hidden unless `showRevoked: true` is selected.
- `CustomerPaymentMethod.instrument` is stored as a selected union payload keyed by `__typename`, so seeded fixtures can serialize credit-card and PayPal billing-agreement fragments without local vaulting. `subscriptionContracts` on the payment method is serialized as a normal connection from seeded link rows; the customer-level `subscriptionContracts` field remains empty/no-data until separately modeled.
- Customer payment method writes remain unsupported scaffolds except for `customerPaymentMethodSendUpdateEmail`, which is buffered locally and retained for commit replay rather than delivered at runtime. Credit-card, PayPal billing-agreement, remote-create, duplication-data, update-url, and revoke roots require `write_customers` plus `write_customer_payment_methods` and are sensitive because they can involve vaulted instruments, expiring payment links, destructive revocation, asynchronous gateway polling, or customer-visible flows.
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
- HAR-291 attached-resource capture extends local `customerMerge` staging for normalized customer-owned addresses, customer-owned metafields, and normalized orders. Shopify selected `customerTwoId` as the resulting customer, kept the result customer's default address, appended the source customer's address to `addressesV2`, preserved result-side metafield conflicts, copied source-only metafields under a new metafield id, and moved the source order to the result customer with the result customer's email. `numberOfOrders` and `lastOrder` stayed at the result customer's existing summary values in the captured immediate post-completion reads.
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
- Customer address lifecycle capture: `corepack pnpm conformance:capture-customer-addresses`, writing `fixtures/conformance/<store>/<version>/customer-address-lifecycle.json`
- CustomerSet capture: `corepack pnpm conformance:capture-customer-set`, writing `fixtures/conformance/<store>/<version>/customer-set-parity.json`
- Customer tax exemption capture: `corepack pnpm conformance:capture-customer-tax-exemptions`, writing `customer-add-tax-exemptions-parity.json`, `customer-remove-tax-exemptions-parity.json`, and `customer-replace-tax-exemptions-parity.json`
- Data sale opt-out capture: `corepack pnpm conformance:capture-data-sale-opt-out`, writing `data-sale-opt-out-parity.json`
- Customer order-summary capture: `corepack pnpm conformance:capture-customer-order-summary`, writing `customer-order-summary-read-effects.json`
- Customer merge capture: `corepack pnpm conformance:capture-customer-merge`, writing `customer-merge-parity.json`
- Conformance fixtures and requests: `config/parity-specs/customer*.json`, `config/parity-specs/customers*.json`, and matching files under `config/parity-requests/`

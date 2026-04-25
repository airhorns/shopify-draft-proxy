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
- `customerMerge`

## Unsupported roots still tracked by the registry

- `customerAddTaxExemptions`
- `customerRemoveTaxExemptions`
- `customerReplaceTaxExemptions`
- `customerSet`
- `customerGenerateAccountActivationUrl`
- `customerPaymentMethodGetUpdateUrl`
- `customerSendAccountInviteEmail`
- `customerPaymentMethodSendUpdateEmail`

## Behavior notes

- Customer-domain state deliberately stays narrower than the product model, but it is still normalized.
- `CustomerRecord` carries scalar/detail fields plus `taxExemptions` as a separate list from the boolean `taxExempt`.
- Customer-owned metafields live in `customerMetafields` instead of reusing product-domain metafield storage or broadening shared `metafieldsSet` owner support without separate customer-domain evidence.
- Staged `customerUpdate(input.metafields)` computes against the effective customer metafield set and replaces the staged customer-owned set, so downstream `customer.metafield(...)` and `customer.metafields(...)` reads stay consistent.
- `customerByIdentifier(identifier:)` resolves from the same effective normalized customer graph as `customer(id:)` and `customers`, including staged customer creates/updates and hydrated live-hybrid customers.
- Customer-owned addresses live in normalized `customerAddresses` state. Staged `customerAddressCreate`, `customerAddressUpdate`, `customerAddressDelete`, and `customerUpdateDefaultAddress` mutate that address graph locally and keep `Customer.defaultAddress` synchronized with the selected default row.
- `Customer.addressesV2` serializes from the effective normalized address graph, preserving hydrated connection cursors where captured and returning Shopify-like empty connections when no address records exist.
- Captured Admin GraphQL 2025-01 evidence for customer address lifecycle uses `MailingAddressInput`, payload field `address`, and delete payload field `deletedAddressId`. Unknown customers return payload `userErrors` with `field: ["customerId"]`; unknown address IDs on update/delete/default roots return top-level `RESOURCE_NOT_FOUND` GraphQL errors with `data.<root>: null`.
- Staged `customerMerge` updates the normalized resulting customer row, marks the source customer deleted, records the source-to-result redirect in `mergedCustomerIds`, and records the observed merge job/result shape in `customerMergeRequests`.
- `customerMergePreview` and `customerMergeJobStatus` resolve from normalized customer/merge-request state. The first local merge slice supports customers already present in staged state or hydrated base state and does not fetch unknown customer ids during the supported mutation path.

## Outbound email and activation safety

The customer surface includes roots that look related but have different safety profiles:

- `customerEmailMarketingConsentUpdate` and `customerSmsMarketingConsentUpdate` are customer state changes, not outbound notification sends. They are safe to stage locally once captured behavior is modeled because downstream reads can observe the changed consent fields.
- `customerGenerateAccountActivationUrl` and `customerPaymentMethodGetUpdateUrl` return sensitive, expiring customer-facing links. They can be represented safely only as synthetic non-deliverable local URLs after captured payload/error shape evidence exists; until then they stay unimplemented registry entries.
- `customerSendAccountInviteEmail` and `customerPaymentMethodSendUpdateEmail` are explicit outbound email side effects. They stay unsupported because a successful local response would claim customer-visible delivery that the proxy cannot observe, undo, or validate through downstream Admin reads.

Do not mark outbound email roots implemented by proxying them upstream. Future support needs a product decision between a blocking response and a non-delivering local outbox/audit model, plus conformance evidence for the payload and user-error shapes.

## Validation anchors

- Customer reads: `tests/integration/customer-query-shapes.test.ts`
- Customer mutations and merge slices: `tests/integration/customer-draft-flow.test.ts`
- Customer address lifecycle capture: `corepack pnpm conformance:capture-customer-addresses`, writing `fixtures/conformance/<store>/<version>/customer-address-lifecycle.json`
- Conformance fixtures and requests: `config/parity-specs/customer*.json`, `config/parity-specs/customers*.json`, and matching files under `config/parity-requests/`

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
- `customerSet`
- `customerMerge`

## Unsupported roots still tracked by the registry

- `customerAddTaxExemptions`
- `customerRemoveTaxExemptions`
- `customerReplaceTaxExemptions`
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
- Captured Admin GraphQL 2026-04 evidence for `customerSet` supports a narrow local slice: create without `identifier`, update by `identifier.id`, and upsert/update by `identifier.email` or `identifier.phone`. The staged input slice is `email`, `firstName`, `lastName`, `locale`, `note`, `phone`, `tags`, `taxExempt`, `taxExemptions`, and address-list replacement when the identifier resolves an existing customer.
- `customerSet(input.addresses)` is treated as a replacement list for an existing customer: current normalized addresses are removed, input mailing addresses are staged, and `Customer.defaultAddress` follows the first replacement address or `null` for an empty list. Local replay uses stable synthetic address cursors; live Shopify cursors are opaque and are only parity-comparable as expected differences.
- `customerSet(identifier.customId)` remains unsupported without modeled unique metafield definitions. The local response mirrors the captured top-level `NOT_FOUND` error instead of proxying the supported root upstream.
- Unsupported `customerSet` input or identifier fields return local `userErrors`, keeping the root in the supported local-staging path without silently claiming unmodeled behavior.
- Staged `customerMerge` updates the normalized resulting customer row, marks the source customer deleted, records the source-to-result redirect in `mergedCustomerIds`, and records the observed merge job/result shape in `customerMergeRequests`.
- `customerMergePreview` and `customerMergeJobStatus` resolve from normalized customer/merge-request state. The first local merge slice supports customers already present in staged state or hydrated base state and does not fetch unknown customer ids during the supported mutation path.

## Validation anchors

- Customer reads: `tests/integration/customer-query-shapes.test.ts`
- Customer mutations, `customerSet`, and merge slices: `tests/integration/customer-draft-flow.test.ts`
- Customer address lifecycle capture: `corepack pnpm conformance:capture-customer-addresses`, writing `fixtures/conformance/<store>/<version>/customer-address-lifecycle.json`
- CustomerSet capture: `corepack pnpm conformance:capture-customer-set`, writing `fixtures/conformance/<store>/<version>/customer-set-parity.json`
- Conformance fixtures and requests: `config/parity-specs/customer*.json`, `config/parity-specs/customers*.json`, and matching files under `config/parity-requests/`

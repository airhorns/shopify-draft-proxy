---
title: 'Store Properties Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Store Properties Endpoint Group.'
---

This endpoint group covers shop-level Admin GraphQL roots for shop metadata,
locations, business entities, legal shop policies, generic publishable
operations, and related store-property utility reads.

## Current support and limitations

### Implemented roots

The following read roots have local model or fixture-backed behavior:

- `shop`
- `location`
- `locationByIdentifier`
- `locations`
- `businessEntities`
- `businessEntity`
- `cashManagementLocationSummary`

The following mutation roots are handled locally rather than sent to Shopify at
runtime:

- `locationAdd`
- `locationEdit`
- `locationActivate`
- `locationDeactivate`
- `locationDelete`
- `publishablePublish`
- `publishablePublishToCurrentChannel`
- `publishableUnpublish`
- `publishableUnpublishToCurrentChannel`
- `shopPolicyUpdate`

Registry presence means the proxy has a local dispatcher for the root. Support
for each root is limited to the local lifecycle behavior and downstream
read-after-write effects described below and backed by executable evidence.

### Local behavior

Shop reads return modeled metadata for selected shop fields, including shop
policies, publication aggregates, primary domain, and Shopify-like empty or null
fallbacks. `shopPolicyUpdate` stages policy body, title, URL, migrated HTML,
blank subscription-policy validation, user-error payloads, downstream
`shop.shopPolicies` reads, and generic `node(id:)` / `nodes(ids:)` policy
dispatch.

Location reads resolve staged and observed local locations through `location`,
`locationByIdentifier(identifier: { id })`, and `locations(first:)`. Deleted
locations are tombstoned so those reads return `null` or omit the node, and
inventory-level projections also hide deleted locations.

`locationAdd` stages synthetic Location records with deterministic timestamps,
address data, Location-owned metafields, the captured `fulfillsOnlineOrders`
default of `true`, blank/duplicate/too-long name userErrors, public schema-style
address and country-code validation, and the captured 200-location create guard.
Rejected adds do not append mutation-log entries.

`locationEdit` stages edits for generic public Admin GraphQL documents by root
field. Successful edits update the local Location record, preserve the original
raw mutation for commit replay, and are visible through downstream
`location`, `locationByIdentifier`, `locations`, and inventory location
projections. The modeled validation surface includes unknown ID, duplicate,
blank, and too-long names; invalid address country codes; too-long city and ZIP
values; unsupported metafield types; and the only-online-fulfillment guard.

`locationActivate` and `locationDeactivate` stage local state-machine changes
and preserve successful raw mutations for commit replay. Activation guard
branches cover location limit, ongoing relocation, and fulfillment-service
managed scope. Deactivation with a destination relocates source-location
inventory into the destination in the modeled slice, merges same-name quantity
rows when needed, removes source levels from downstream inventory reads, and
leaves guard/userError branches unstaged.

`locationDelete` stages successful deletes by tombstoning the Location record,
removing observed and fulfillment-service location overlays, and cascading
dependent inventory levels and quantity timestamps. It returns
`locationDeleteUserErrors` with the modeled Core code set, including
`LOCATION_NOT_FOUND`, `LOCATION_IS_ACTIVE`, `LOCATION_HAS_INVENTORY`,
`LOCATION_HAS_PENDING_ORDERS`, and `LOCATION_NOT_DELETABLE`. It does not return
a synthetic `LOCATION_IS_PRIMARY` code. Successful deletes preserve the original
raw mutation for commit replay.

Generic publishable mutation slices cover Product and Collection publish and
unpublish behavior where backed by parity specs. Product-scoped
`PublicationInput` validation locally rejects duplicate publication IDs, blank
or empty `publicationId`, unknown publication IDs, and pre-1970 `publishDate`
values with captured Shopify field paths and messages. Current-channel helpers
update modeled publication aggregates such as `shop.publicationCount`.

Business entity reads have safe fixture-backed catalog and fallback behavior,
including ordered `businessEntities`, primary `businessEntity` fallback,
known/unknown ID lookup, empty structures, and Shopify Payments account fields
where captured.

### Boundaries

- Store-properties roots outside the implemented list follow the configured
  unsupported path and remain visible in logs/observability.
- Location lifecycle support is bounded to the captured local state-machine and
  validation branches. Open purchase order, transfer, temporary block, retail
  subscription, external document, and additional fulfillment-service branches
  require separate captured evidence before they should be relied on.
- `locationByIdentifier` is modeled for ID-based lookup. Additional identifier
  forms remain unsupported unless a scenario explicitly covers them.
- Validation-only store-properties specs prove guardrail payloads and no-stage
  behavior for those inputs only.
- Shipping package and local pickup behavior are documented under
  `/endpoints/shipping-fulfillments/` because their caller-visible effects live
  in shipping and delivery settings.

### Evidence

- Runtime coverage: `tests/graphql_routes.rs`
- Registry status: `src/operation_registry.rs` and
  `src/operation_registry_data.rs`
- Location parity specs:
  `config/parity-specs/store-properties/location-add-edit-uniqueness-and-required-fields.json`,
  `config/parity-specs/store-properties/location-edit-fields-and-state-machine.json`,
  `config/parity-specs/store-properties/location-edit-unknown-id-validation.json`,
  `config/parity-specs/store-properties/location-delete-active-location-validation.json`,
  `config/parity-specs/store-properties/location-delete-inventory-level-cascade.json`,
  `config/parity-specs/store-properties/location-delete-primary-location.json`,
  and
  `config/parity-specs/store-properties/location-delete-state-and-scope.json`
- Store-properties fixtures:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/*.json`
  and
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/*.json`

### Validation

- `corepack pnpm conformance:fixture-invariants`
- `corepack pnpm rust:fmt`
- `corepack pnpm rust:clippy`
- `corepack pnpm rust:test`
- `corepack pnpm conformance:check`
- `corepack pnpm lint`

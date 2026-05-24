# Store Properties Endpoint Group

This endpoint group covers shop-level Admin GraphQL roots for shop metadata,
locations, business entities, legal shop policies, generic publishable
operations, and related store-property utility reads.

## Current support and limitations

### Supported roots

The current Rust operation registry does not mark any store-properties root as
fully implemented. Registry presence is a local-model commitment only; it is
not a supported-runtime claim for the whole store-properties domain.

The registry-only read roots are:

- `shop`
- `location`
- `locationByIdentifier`
- `businessEntities`
- `businessEntity`
- `cashManagementLocationSummary`

The registry-only mutation roots are:

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

### Local behavior

The Rust runtime has scenario-backed store-properties slices for ported parity
requests and runtime tests. These slices are not general registry support for
every store-property document.

Shop reads have a baseline fixture-backed slice for selected shop metadata,
including shop policies, publication aggregates, primary domain, and safe empty
or null shapes. `shopPolicyUpdate` has local staging evidence for policy body,
title, URL, migrated-HTML behavior, user-error codes, blank subscription-policy
validation, downstream `shop.shopPolicies` reads, and generic `node(id:)` /
`nodes(ids:)` policy dispatch.

Location reads and lifecycle mutations have fixture-backed local slices for
detail reads, unknown-ID null behavior, `locationByIdentifier` selected cases,
address/country/province derivation, create/edit validation, metafields on
location add/edit, activate/deactivate state transitions, delete tombstones,
idempotency directives, resource-limit validation, and selected lifecycle guard
errors. Successful location mutation slices stage local state, preserve the raw
GraphQL request for commit replay, and expose read-after-write behavior through
`location`, `locationByIdentifier`, `locations`, inventory-level location
projection, and meta state/log inspection when those surfaces are part of the
checked-in scenario. Successful `locationDeactivate` calls with a
`destinationLocationId` relocate source-location inventory levels into the
destination in the modeled slice, merge same-name quantity rows when a
destination level already exists, remove the source level from downstream
inventory reads, and leave guard/userError branches without relocation.

Generic publishable mutation slices cover Product and Collection publish/unpublish
behavior where backed by parity specs. Product current-channel helpers update
publication aggregates such as `shop.publicationCount` for the modeled
publication catalog. Unsupported publishable target types return local
userErrors in the documented scenarios instead of being treated as full support
for every publishable object.

Business entity reads have safe fixture-backed catalog and fallback behavior,
including ordered `businessEntities`, primary `businessEntity` fallback,
known/unknown ID lookup, empty structures, and safe Shopify Payments account
fields only where captured.

### Boundaries

- Store-properties roots remain `implemented: false` in the current operation
  registry. Scenario-backed Rust helpers should not be described as broad root
  support.
- Location lifecycle support is bounded to the captured local state-machine and
  validation branches. Open purchase order, transfer, temporary block, retail
  subscription, external document, and fulfillment-service branches remain
  fixture or runtime-slice evidence unless separately captured for a public
  setup path.
- Validation-only store-properties specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make the corresponding mutation
  roots generally supported.
- Shipping package and local pickup behavior are documented under
  `docs/endpoints/shipping-fulfillments.md` because their caller-visible effects
  live in shipping and delivery settings.
- Unsupported mutation documents outside the ported local slices follow the
  configured unsupported path and must remain visible in logs/observability.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes.rs`
- Store-properties parity specs: `config/parity-specs/store-properties/*.json`
- Store-properties fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/*.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`

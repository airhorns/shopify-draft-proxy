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

The Rust runtime has scenario-backed store-properties slices for parity requests
and runtime tests. These slices are not general registry support for every
store-property document.

Shop reads have a local store-backed slice for selected shop metadata,
including staged shop policies, publication aggregates, primary domain, and safe
empty or null shapes. LiveHybrid reads hydrate the connected shop when upstream
or cassette data is available; pure snapshot/cold fallback uses a neutral
synthetic `Shopify Draft Proxy` shop identity instead of a captured real store.
`shopPolicyUpdate` is dispatched by root field, stages
policy body/title/URL/timestamps in the Rust store, preserves the original raw
mutation for commit replay, and exposes read-after-write behavior through
`shop.shopPolicies` plus generic `node(id:)` / `nodes(ids:)` policy dispatch.
The local model uses Shopify's deprecated policy title map (`Privacy Policy`,
`Refund Policy`, `Terms of Service`, `Shipping Policy`, `Subscription Policy`,
`Contact Information`, `Legal Notice`, and `Terms of Sale`), derives URLs from
the effective shop domain fallback, accepts bodies up to 524,287 bytes, returns
`TOO_BIG` above that cap, rejects blank subscription-policy bodies with
`field: ["shopPolicy", "body"]`, rejects privacy-policy-only Liquid syntax
errors with `field: ["shopPolicy", "body"]`, and returns top-level
`INVALID_VARIABLE` errors for invalid policy enum values or missing/null
required bodies.

`locationAdd` now has a generic Rust staging path for public Admin GraphQL
documents, not only fixture-named parity documents. It stages a synthetic
Location ID, deterministic timestamps, address data, Location-owned metafields,
the captured `fulfillsOnlineOrders` default of `true`, blank/duplicate/too-long
name userErrors, public schema-style address/country-code validation, and the
captured 200-location create guard. In LiveHybrid replay, the guard derives cap
state from the recorded `StorePropertiesLocationLimitStatus` upstream read
instead of a synthetic local seed. Rejected adds do not append mutation-log
entries.

`locationActivate` now has a generic Rust staging path for public Admin GraphQL
documents. Successful activations flip the local Location `isActive` state,
stage the changed record, preserve the raw mutation for commit replay, and are
visible through downstream location reads. Guard branches for location limit,
ongoing relocation, fulfillment-service managed scope, and duplicate active
location names return field paths, codes, and messages without staging
activation. The `LOCATION_LIMIT` branch is backed by live 2026-04 evidence; the
internal/transient `HAS_ONGOING_RELOCATION` branch remains runtime-test-only
because public Admin GraphQL relocation completed synchronously in the
disposable shop.

Location reads and lifecycle mutations have local slices for detail reads,
unknown-ID null behavior, `locationByIdentifier` selected cases,
address/country/province derivation including the captured GB, AU, AE, and CA
branches, create/edit validation, metafields on
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
Captured guard slices include same-destination rejection, inactive-destination
rejection, active-inventory relocation requirements, only-online-fulfillment
protection, and permanent deactivation blocks with Shopify field paths and
codes.

Generic publishable mutation slices cover Product and Collection publish/unpublish
behavior where backed by parity specs. Product-scoped `PublicationInput`
validation locally rejects duplicate publication IDs, blank or empty
`publicationId`, unknown publication IDs, and pre-1970 `publishDate` values with
the captured Shopify field paths/messages. The top-level publishable `id` must
resolve to a known Product or Collection from staged/base state, or from a
LiveHybrid hydrate read, before the mutation stages; missing resources return a
local `Resource does not exist` userError on `field: ["id"]` and leave the
mutation log unchanged. Product current-channel helpers stage an internal
current-channel publication membership when a current channel is available,
return `Channel does not exist` without staging when the local shop context has
no current channel, and project `publishedOnCurrentPublication` plus
`resourcePublications(first:)` from the staged membership set. Unsupported
publishable target types return local userErrors in the documented scenarios
instead of being treated as full support for every publishable object.

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

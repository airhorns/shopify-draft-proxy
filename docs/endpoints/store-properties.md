# Store Properties Endpoint Group

The store-properties group has implemented local slices, but the whole registry domain is not complete yet. Keep shop, location, business-entity, policy, and generic publishable minutia here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `shop`
- `location`
- `locationByIdentifier`
- `businessEntities`
- `businessEntity`

Local staged mutations:

- `publishablePublish`
- `publishablePublishToCurrentChannel`
- `publishableUnpublish`
- `publishableUnpublishToCurrentChannel`
- `shopPolicyUpdate`

## Unsupported roots still tracked by the registry

- `cashManagementLocationSummary`
- `locationAdd`
- `locationEdit`
- `locationActivate`
- `locationDeactivate`
- `locationDelete`

## Behavior notes

- `baseState` includes a nullable normalized `shop` slice. Snapshot mode returns `shop: null` when no shop slice is present instead of inventing store identity; live-hybrid can serve a locally staged shop overlay when one exists.
- `shopPolicyUpdate` stages legal policy writes into the shop slice by `ShopPolicyType`. Downstream `shop.shopPolicies` reads in snapshot and live-hybrid modes observe staged body, identity, URL, title, timestamps, and empty translations shape without sending supported policy mutations upstream at runtime.
- Snapshot `location` and `locationByIdentifier` detail reads use the Store properties overlay. They combine narrow normalized location metadata for captured address/lifecycle scalars with nested inventory-level connections derived from the effective inventory-level graph.
- The first location detail slice supports primary-location fallback when `location(id:)` omits `id`, identifier lookup by `LocationIdentifierInput.id`, unknown-location `null` behavior, address and lifecycle scalar shapes, empty metafield/suggested-address structures, and nested `inventoryLevel` / `inventoryLevels` selections.
- Location creation/update/deletion remains unsupported runtime scope. Location metadata is read-only baseline state and staged inventory remains the source of truth for read-after-write inventory visibility.
- Generic `publishablePublish` and `publishableUnpublish` stage Product and Collection publishables locally. `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` currently cover Product publishables. Unsupported publishable target types return local userErrors instead of proxying upstream as supported behavior.

## Validation anchors

- Shop reads: `tests/integration/shop-query-shapes.test.ts`
- Shop policy mutation flow: `tests/integration/shop-policy-update-flow.test.ts`
- Location reads: `tests/integration/location-query-shapes.test.ts`
- Business entity reads: `tests/integration/business-entity-query-shapes.test.ts`
- Generic publishable slices: `tests/integration/product-draft-flow.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/shop*.json`, `config/parity-specs/location*.json`, `config/parity-specs/locations*.json`, `config/parity-specs/business*.json`, `config/parity-specs/publishable*.json`, and matching files under `config/parity-requests/`

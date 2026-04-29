# Store Properties Endpoint Group

The store-properties group has implemented local slices, but the whole registry domain is not complete yet. Keep shop, location, business-entity, policy, and generic publishable minutia here instead of in `docs/architecture.md`.

## Current support and limitations

### Implemented roots

Overlay reads:

- `shop`
- `location`
- `locationByIdentifier`
- `businessEntities`
- `businessEntity`
- `shopifyPaymentsAccount`

Local staged mutations:

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

### Unsupported roots still tracked by the registry

- `cashManagementLocationSummary`

### Behavior notes

- `baseState` includes a nullable normalized `shop` slice. Snapshot mode returns `shop: null` when no shop slice is present instead of inventing store identity; live-hybrid can serve a locally staged shop overlay when one exists.
- `shop.fulfillmentServices` is serialized from the normalized fulfillment-service graph owned by `docs/endpoints/shipping-fulfillments.md`; the field returns an empty list when no services are staged or snapshotted.
- `shopPolicyUpdate` stages legal policy writes into the shop slice by `ShopPolicyType`. Downstream `shop.shopPolicies` reads in snapshot and live-hybrid modes observe staged body, identity, URL, title, timestamps, and empty translations shape without sending supported policy mutations upstream at runtime.
- Generic Admin `node(id:)` / `nodes(ids:)` dispatch resolves `ShopAddress` from the effective `shop.shopAddress` row and `ShopPolicy` from the effective `shop.shopPolicies` list. Missing address/policy IDs still return `null`; this does not broaden unsupported store-property roots beyond the already modeled shop/policy state.
- Snapshot `location` and `locationByIdentifier` detail reads use the Store properties overlay. They combine narrow normalized location metadata for captured address/lifecycle scalars with nested inventory-level connections derived from the effective inventory-level graph.
- Top-level `locations` is still dispatched through the product/inventory read path, but it serializes each `Location` node with the Store properties location serializer. Selected lifecycle, address, metafield, local-pickup, and inventory-level fields therefore stay aligned with `location`, `locationByIdentifier`, and staged location lifecycle mutations instead of being limited to catalog `id` / `name`.
- The first location detail slice supports primary-location fallback when `location(id:)` omits `id`, identifier lookup by `LocationIdentifierInput.id`, unknown-location `null` behavior, address and lifecycle scalar shapes, empty metafield/suggested-address structures, and nested `inventoryLevel` / `inventoryLevels` selections.
- `locationAdd` stages new normalized locations with proxy-synthetic `Location` IDs, stable timestamps, address metadata, `fulfillsOnlineOrders`, and owner-scoped metafields. Downstream `location`, `locationByIdentifier`, top-level `locations`, and meta state/log inspection observe the staged location without sending the write upstream at runtime.
- `locationEdit` stages updates against base or synthetic locations, preserving unspecified address fields and updating inventory-level location name serialization through the effective location record. Captured validation branches include blank-name `userErrors` (`input.name` / `Add a location name`) and missing-location `userErrors` (`id` / `Location not found.`). Fulfillment-service locations are blocked locally unless future conformance proves an app-owned editable fulfillment-service branch.
- `locationActivate` and `locationDeactivate` stage lifecycle state locally and require the Admin GraphQL 2026-04 `@idempotent(key: "...")` directive. Missing directive validation is backed by safe live captures and returns Shopify's top-level `BAD_REQUEST` GraphQL error plus `data.<root>: null`.
- `locationDeactivate` sets `isActive: false`, records a synthetic `deactivatedAt`, flips activation/deactivation/deletion flags, and clears local stockability flags. If inventory is stocked at the source location, local staging requires a valid active `destinationLocationId` and transfers the effective inventory-level quantities there before deactivating. Downstream top-level `locations`, singular location reads, and inventory item level reads all observe the same staged lifecycle and inventory-transfer effects.
- `locationDelete` stages a tombstone after deactivation. Active stocked locations return captured `LOCATION_IS_ACTIVE` and `LOCATION_HAS_INVENTORY` userErrors; successful deletes remove the location from `location`, `locationByIdentifier`, top-level `locations`, and inventory-level reads while keeping the raw mutation in the commit log.
- Fulfillment-service lifecycle support can create, update, keep, or delete service-managed locations as a side effect. Direct `locationEdit` continues to reject existing fulfillment-service locations unless a separate app-owned editable branch is captured.
- Snapshot `shopifyPaymentsAccount` reads are backed by the same normalized safe account fixture used by `BusinessEntity.shopifyPaymentsAccount`. When no account fixture is present, the direct root returns `null`, matching the current access-denied capture's data shape. When a safe account fixture is present, scalar identity/setup fields are exposed and `payouts`, `disputes`, and `balanceTransactions` return empty no-data connections with selected `edges`, `nodes`, and `pageInfo`.
- Shopify Payments fields that can reveal balances, bank accounts, statement descriptors, payout schedules, or other account-specific financial data remain unavailable unless captured and modeled explicitly; snapshot reads return `null` for those selections with `UNSUPPORTED_FIELD` diagnostics.
- Generic `publishablePublish` and `publishableUnpublish` stage Product and Collection publishables locally. `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` currently cover Product publishables. Product publishable mutation payloads can select `shop.publicationCount`, which is derived from the same normalized publication catalog used by product and publication reads. Current-channel product placeholders are excluded from the shop publication catalog so `shop.publicationCount` does not grow just because a local current-channel membership marker was staged. Unsupported publishable target types return local userErrors instead of proxying upstream as supported behavior.

### HAR-460 fidelity review summary

The April 2026 review compared the implemented Store properties slice with the Admin GraphQL docs/examples and public usage examples for shop, location, business entity, generic publishable, and shop policy roots. The current executable evidence is concentrated in strict store-properties parity specs plus targeted integration tests rather than broad schema-shape assertions:

- Shop and policy evidence covers baseline `shop` reads, snapshot no-shop `null`, live-hybrid staged overlays, `shopPolicyUpdate` local staging, oversized policy body `TOO_BIG` userErrors, and downstream `shop.shopPolicies` read-after-write.
- Location evidence covers top-level empty `locations` behavior, `location` primary fallback, `location(id:)`, `locationByIdentifier(identifier: { id })`, no-data `locationByIdentifier(identifier: { customId })`, invalid empty identifier errors, captured address/lifecycle/metafield/suggested-address shapes, local add/edit/activate/deactivate/delete staging, missing idempotency guardrails, active stocked delete guardrails, and downstream inventory-level/location reads after lifecycle writes.
- Business entity evidence covers captured ordered `businessEntities`, primary `businessEntity` fallback, known/unknown ID lookup, empty/no-data structures, and safe Shopify Payments account fields only when explicitly fixture-backed.
- Publishable evidence covers Product generic publish/unpublish, Product current-channel publish/unpublish, Collection generic publish/unpublish, downstream publication aggregates, `shop.publicationCount`, and local userErrors for unsupported publishable target types instead of supported-runtime passthrough.

Remaining gaps are intentional rather than silent support claims:

- `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` stay Product-scoped until Collection current-channel behavior has separate captured evidence.
- `LocationIdentifierInput.customId` currently returns `null` unless a future fixture-backed custom-ID index is modeled; do not synthesize custom identifier matches from arbitrary metafields.
- Location lifecycle happy paths are covered by runtime tests; strict live parity is currently strongest for safe validation/idempotency branches plus read/detail fixtures. Fresh success-path captures should use disposable location setup and cleanup evidence before replacing those runtime-test-backed assertions.
- Business entity reads must not expose or synthesize Shopify Payments balances, bank accounts, payout schedules, statement descriptors, or other financial data unless those exact fields are captured and modeled safely.

## Historical and developer notes

### Validation anchors

- Shop reads: `tests/integration/shop-query-shapes.test.ts`
- Shop policy mutation flow: `tests/integration/shop-policy-update-flow.test.ts`
- Generic Node coverage for `ShopAddress` / `ShopPolicy`: `config/parity-specs/admin-platform/admin-platform-store-property-node-reads.json` and `tests/integration/admin-platform-query-shapes.test.ts`
- Location reads: `tests/integration/location-query-shapes.test.ts`
- Fulfillment-service location linkage: `tests/integration/fulfillment-service-flow.test.ts`
- Business entity and Shopify Payments account reads: `tests/integration/business-entity-query-shapes.test.ts`
- Generic publishable slices: `tests/integration/product-draft-flow.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/store-properties/shop*.json`, `config/parity-specs/store-properties/location*.json`, `config/parity-specs/store-properties/locations*.json`, `config/parity-specs/store-properties/business*.json`, `config/parity-specs/store-properties/publishable*.json`, and matching files under `config/parity-requests/store-properties/`

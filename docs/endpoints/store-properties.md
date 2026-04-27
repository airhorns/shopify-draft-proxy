# Store Properties Endpoint Group

The store-properties group has implemented local slices, but the whole registry domain is not complete yet. Keep shop, location, business-entity, policy, and generic publishable minutia here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `shop`
- `location`
- `locationByIdentifier`
- `businessEntities`
- `businessEntity`
- `shopifyPaymentsAccount`
- `cashManagementLocationSummary` access-denied branch

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

## Unsupported roots still tracked by the registry

- None in the narrow Store properties registry slice currently tracked here.

## Behavior notes

- `baseState` includes a nullable normalized `shop` slice. Snapshot mode returns `shop: null` when no shop slice is present instead of inventing store identity; live-hybrid can serve a locally staged shop overlay when one exists.
- `shop.fulfillmentServices` is serialized from the normalized fulfillment-service graph owned by `docs/endpoints/shipping-fulfillments.md`; the field returns an empty list when no services are staged or snapshotted.
- `shopPolicyUpdate` stages legal policy writes into the shop slice by `ShopPolicyType`. Downstream `shop.shopPolicies` reads in snapshot and live-hybrid modes observe staged body, identity, URL, title, timestamps, and empty translations shape without sending supported policy mutations upstream at runtime.
- Snapshot `location` and `locationByIdentifier` detail reads use the Store properties overlay. They combine narrow normalized location metadata for captured address/lifecycle scalars with nested inventory-level connections derived from the effective inventory-level graph.
- The first location detail slice supports primary-location fallback when `location(id:)` omits `id`, identifier lookup by `LocationIdentifierInput.id`, unknown-location `null` behavior, address and lifecycle scalar shapes, empty metafield/suggested-address structures, and nested `inventoryLevel` / `inventoryLevels` selections.
- `locationAdd` stages new normalized locations with proxy-synthetic `Location` IDs, stable timestamps, address metadata, `fulfillsOnlineOrders`, and owner-scoped metafields. Downstream `location`, `locationByIdentifier`, top-level `locations`, and meta state/log inspection observe the staged location without sending the write upstream at runtime.
- `locationEdit` stages updates against base or synthetic locations, preserving unspecified address fields and updating inventory-level location name serialization through the effective location record. Captured validation branches include blank-name `userErrors` (`input.name` / `Add a location name`) and missing-location `userErrors` (`id` / `Location not found.`). Fulfillment-service locations are blocked locally unless future conformance proves an app-owned editable fulfillment-service branch.
- `locationActivate` and `locationDeactivate` stage lifecycle state locally and require the Admin GraphQL 2026-04 `@idempotent(key: "...")` directive. Missing directive validation is backed by safe live captures and returns Shopify's top-level `BAD_REQUEST` GraphQL error plus `data.<root>: null`.
- `locationDeactivate` sets `isActive: false`, records a synthetic `deactivatedAt`, flips activation/deactivation/deletion flags, and clears local stockability flags. If inventory is stocked at the source location, local staging requires a valid active `destinationLocationId` and transfers the effective inventory-level quantities there before deactivating.
- `locationDelete` stages a tombstone after deactivation. Active stocked locations return captured `LOCATION_IS_ACTIVE` and `LOCATION_HAS_INVENTORY` userErrors; successful deletes remove the location from `location`, `locationByIdentifier`, top-level `locations`, and inventory-level reads while keeping the raw mutation in the commit log.
- Fulfillment-service lifecycle support can create, update, keep, or delete service-managed locations as a side effect. Direct `locationEdit` continues to reject existing fulfillment-service locations unless a separate app-owned editable branch is captured.
- Snapshot `shopifyPaymentsAccount` reads are backed by the same normalized safe account fixture used by `BusinessEntity.shopifyPaymentsAccount`. When no account fixture is present, the direct root returns `null`, matching the current access-denied capture's data shape. When a safe account fixture is present, scalar identity/setup fields are exposed and `payouts`, `disputes`, and `balanceTransactions` return empty no-data connections with selected `edges`, `nodes`, and `pageInfo`.
- Shopify Payments fields that can reveal balances, bank accounts, statement descriptors, payout schedules, or other account-specific financial data remain unavailable unless captured and modeled explicitly; snapshot reads return `null` for those selections with `UNSUPPORTED_FIELD` diagnostics.
- Generic `publishablePublish` and `publishableUnpublish` stage Product and Collection publishables locally. `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` currently cover Product publishables. Unsupported publishable target types return local userErrors instead of proxying upstream as supported behavior.
- `cashManagementLocationSummary` snapshot support mirrors the captured Admin 2026-04 access-denied branch for `harry-test-heelo.myshopify.com`: top-level `data: null`, `ACCESS_DENIED`, and the required `read_cash_tracking` plus POS/retail role permission message. The credential is denied before known-location, unknown-location, or no-data summary behavior can be observed; do not synthesize `CashManagementSummary` balances or session counts until a fixture-backed readable branch exists.

## Validation anchors

- Shop reads: `tests/integration/shop-query-shapes.test.ts`
- Shop policy mutation flow: `tests/integration/shop-policy-update-flow.test.ts`
- Location reads: `tests/integration/location-query-shapes.test.ts`
- Fulfillment-service location linkage: `tests/integration/fulfillment-service-flow.test.ts`
- Business entity and Shopify Payments account reads: `tests/integration/business-entity-query-shapes.test.ts`
- Cash-management access-denied reads: `tests/integration/cash-management-location-summary-query-shapes.test.ts`
- Generic publishable slices: `tests/integration/product-draft-flow.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/shop*.json`, `config/parity-specs/location*.json`, `config/parity-specs/locations*.json`, `config/parity-specs/business*.json`, `config/parity-specs/cash-management*.json`, `config/parity-specs/publishable*.json`, and matching files under `config/parity-requests/`
- Cash-management access-denied evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/cash-management-location-summary-access-denied.json`

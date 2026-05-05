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
- `shopPolicyUpdate` stages legal policy writes into the shop slice by `ShopPolicyType`. New staged policies use Shopify's deprecated title casing (`Privacy Policy`, `Terms of Service`, etc.), enforce Shopify's 524287-byte body limit, are tagged as migrated HTML, and therefore return their body verbatim. Downstream `shop.shopPolicies` reads in snapshot and live-hybrid modes observe staged body, identity, URL, title, timestamps, and empty translations shape without sending supported policy mutations upstream at runtime.
- Shop policy URLs are built from the effective shop domain instead of a literal `checkout.shopify.com` host. The proxy prefers `shop.primaryDomain.url`, then `shop.primaryDomain.host`, then `shop.url`, and finally `https://<myshopifyDomain>` when no richer checkout host is available; with numeric shop/policy GIDs it keeps Shopify's `/shop-id/policies/policy-id.html?locale=en` path shape.
- `ShopPolicyRecord.migratedToHtml` is local state metadata, not a GraphQL field. Snapshot or state fixtures can set it to `false` for legacy plain-text policies, in which case `ShopPolicy.body` is rendered with a narrow `simple_format` equivalent on `shop.shopPolicies` and Admin `node` / `nodes` reads. Public Shopify capture hydration defaults this flag to `true` because the Admin response body has already passed through Shopify's resolver.
- Generic Admin `node(id:)` / `nodes(ids:)` dispatch resolves `ShopAddress` from the effective `shop.shopAddress` row and `ShopPolicy` from the effective `shop.shopPolicies` list. Missing address/policy IDs still return `null`; this does not broaden unsupported store-property roots beyond the already modeled shop/policy state.
- Snapshot `location` and `locationByIdentifier` detail reads use the Store properties overlay. They combine narrow normalized location metadata for captured address/lifecycle scalars with nested inventory-level connections derived from the effective inventory-level graph.
- Top-level `locations` is still dispatched through the product/inventory read path, but it serializes each `Location` node with the Store properties location serializer. Selected lifecycle, address, metafield, local-pickup, and inventory-level fields therefore stay aligned with `location`, `locationByIdentifier`, and staged location lifecycle mutations instead of being limited to catalog `id` / `name`.
- The first location detail slice supports primary-location fallback when `location(id:)` omits `id`, identifier lookup by `LocationIdentifierInput.id`, unknown-location `null` behavior, address and lifecycle scalar shapes, empty metafield/suggested-address structures, and nested `inventoryLevel` / `inventoryLevels` selections.
- `locationAdd` stages new normalized locations with proxy-synthetic `Location` IDs, stable timestamps, address metadata, `fulfillsOnlineOrders`, owner-scoped metafields, and a local capabilities collection when `capabilitiesToAdd` / `capabilitiesToRemove` are supplied. Inline inputs missing required `LocationAddInput.address` return Shopify's top-level parser error shape; blank names return mutation `userErrors` with Shopify's captured `BLANK` code; obvious non-ISO country codes are rejected locally with an `INVALID` userError guardrail. Downstream `location`, `locationByIdentifier`, top-level `locations`, and meta state/log inspection observe the staged location without sending the write upstream at runtime.
- `locationEdit` stages updates against base or synthetic locations for `name`, partial `address`, `fulfillsOnlineOrders`, and owner-scoped `metafields`, preserving unspecified address fields and updating inventory-level location name serialization through the effective location record. Location-owned `metafield(...)` / `metafields(...)` reads observe staged edit metafields after the mutation. Captured validation branches include blank-name `LocationEditUserError` payloads, invalid `CountryCode` variable errors, invalid metafield type `LocationEditUserError` payloads, missing-location `userErrors`, and online-order fulfillment state-machine blockers for the only active online-fulfilling location, pending-order locations, fulfillment-service locations, and delivery-profile-bound locations represented in local state.
- `locationActivate` and `locationDeactivate` stage lifecycle state locally. Numeric Admin API routes `>= 2026-04` require an `@idempotent(key: "...")` directive on the lifecycle field and return Shopify's top-level `BAD_REQUEST` GraphQL error plus `data.<root>: null` when it is absent; numeric routes before `2026-04` keep the pre-required behavior and stage locally without the directive. Live 2026-04 evidence rejects operation-level `@idempotent`, so the local required-directive check treats it as absent. The `location-activate-deactivate-with-idempotency-directive` parity spec replays both lifecycle roots through captured field-level `@idempotent(key:)` documents, and sibling inventory specs cover the same directive shape across product inventory mutation roots.
- `locationDeactivate` sets `isActive: false`, records a synthetic `deactivatedAt`, preserves Shopify's `activatable`/`deactivatable` lifecycle flags for the captured unstocked disposable-location branch, and clears local stockability flags. `locationActivate` restores `isActive: true`, clears `deactivatedAt`, preserves the captured lifecycle flags, and does not invent stockability. Downstream top-level `locations` and singular location reads observe the staged lifecycle state.
- `locationDelete` stages a tombstone after deactivation. Active stocked locations return captured `LOCATION_IS_ACTIVE` and `LOCATION_HAS_INVENTORY` userErrors; successful deletes remove the location from `location`, `locationByIdentifier`, top-level `locations`, and inventory-level reads while keeping the raw mutation in the commit log.
- Fulfillment-service lifecycle support can create, update, keep, or delete service-managed locations as a side effect. Direct `locationEdit` continues to reject existing fulfillment-service locations unless a separate app-owned editable branch is captured.
- Snapshot `shopifyPaymentsAccount` reads are backed by the same normalized safe account fixture used by `BusinessEntity.shopifyPaymentsAccount`. When no account fixture is present, the direct root returns `null`, matching the current access-denied capture's data shape. When a safe account fixture is present, scalar identity/setup fields are exposed and `payouts`, `disputes`, and `balanceTransactions` return empty no-data connections with selected `edges`, `nodes`, and `pageInfo`.
- Shopify Payments fields that can reveal balances, bank accounts, statement descriptors, payout schedules, or other account-specific financial data remain unavailable unless captured and modeled explicitly; snapshot reads return `null` for those selections with `UNSUPPORTED_FIELD` diagnostics.
- Generic `publishablePublish` and `publishableUnpublish` stage Product and Collection publishables locally. `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` currently cover Product publishables. Product publishable mutation payloads can select `shop.publicationCount`, which is derived from the same normalized publication catalog used by product and publication reads. Current-channel product placeholders are excluded from the shop publication catalog so `shop.publicationCount` does not grow just because a local current-channel membership marker was staged. Unsupported publishable target types return local userErrors instead of proxying upstream as supported behavior.

### HAR-460 fidelity review summary

The April 2026 review compared the implemented Store properties slice with the Admin GraphQL docs/examples and public usage examples for shop, location, business entity, generic publishable, and shop policy roots. The current executable evidence is concentrated in strict store-properties parity specs plus targeted integration tests rather than broad schema-shape assertions:

- Shop and policy evidence covers baseline `shop` reads, snapshot no-shop `null`, live-hybrid staged overlays, `shopPolicyUpdate` local staging, oversized policy body `TOO_BIG` userErrors, and downstream `shop.shopPolicies` read-after-write.
- Location evidence covers top-level empty `locations` behavior, `location` primary fallback, `location(id:)`, `locationByIdentifier(identifier: { id })`, `locationByIdentifier(identifier: { customId })` missing-definition `NOT_FOUND` behavior, invalid empty identifier errors, captured address/lifecycle/metafield/suggested-address shapes, local add/edit/activate/deactivate/delete staging, missing idempotency guardrails, active stocked delete guardrails, and downstream inventory-level/location reads after lifecycle writes.
- Business entity evidence covers captured ordered `businessEntities`, primary `businessEntity` fallback, known/unknown ID lookup, empty/no-data structures, and safe Shopify Payments account fields only when explicitly fixture-backed.
- Publishable evidence covers Product generic publish/unpublish, Product current-channel publish/unpublish, Collection generic publish/unpublish, downstream publication aggregates, `shop.publicationCount`, and local userErrors for unsupported publishable target types instead of supported-runtime passthrough.

Remaining gaps are intentional rather than silent support claims:

- `publishablePublishToCurrentChannel` and `publishableUnpublishToCurrentChannel` stay Product-scoped until Collection current-channel behavior has separate captured evidence.
- `LocationIdentifierInput.customId` currently returns `null` plus Shopify's captured top-level `NOT_FOUND` error when there is no id-typed location metafield definition; do not synthesize custom identifier matches from arbitrary metafields.
- Location lifecycle happy paths are covered by runtime tests; strict live parity is currently strongest for safe validation/idempotency branches plus read/detail fixtures. Fresh success-path captures should use disposable location setup and cleanup evidence before replacing those runtime-test-backed assertions.
- Business entity reads must not expose or synthesize Shopify Payments balances, bank accounts, payout schedules, statement descriptors, or other financial data unless those exact fields are captured and modeled safely.

## Historical and developer notes

### Validation anchors

- Conformance fixtures and requests: `config/parity-specs/store-properties/shop*.json`, `config/parity-specs/store-properties/location*.json`, `config/parity-specs/store-properties/locations*.json`, `config/parity-specs/store-properties/business*.json`, `config/parity-specs/store-properties/publishable*.json`, and matching files under `config/parity-requests/store-properties/`

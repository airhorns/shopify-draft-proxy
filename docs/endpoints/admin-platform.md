# Admin Platform Utility Roots

This endpoint group covers Admin GraphQL platform/utility roots that do not belong to a merchant resource family yet:

- queries: `publicApiVersions`, `node`, `nodes`, `job`, `taxonomy`, `domain`, `backupRegion`, `staffMember`, `staffMembers`
- mutations: `backupRegionUpdate`, `flowGenerateSignature`, `flowTriggerReceive`

HAR-315/HAR-418 conformance evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json`.

## Current support and limitations

### Snapshot Read Behavior

The local snapshot handler is intentionally conservative and only models shapes backed by the HAR-315 capture:

- `publicApiVersions` returns the captured 2026-04 version window. Refresh the fixture and constant when Shopify rotates supported Admin API versions.
- `node(id:)` resolves locally modeled resource GIDs by dispatching the GID type to the existing local detail handler or direct serializer for that resource family. This includes product/catalog/inventory records, customer and payment-method records, B2B Company records, store/shop/location/business-entity records, Files API records, saved searches, payment terms templates/customizations, BulkOperation, metafield/metaobject definitions, order/fulfillment/return/draft-order records, GiftCard, DeliveryProfile, discount owner nodes, marketing/event/webhook/segment records, Market/MarketCatalog/PriceList records, and supported online-store content/integration records when those records are already present in local state.
- `nodes(ids:)` applies the same GID-type dispatch per input id, preserves input order, and returns `null` entries for malformed, missing, or unsupported ids.
- HAR-418 records the live Shopify `Node` interface `possibleTypes` in the admin-platform conformance fixture. `tests/unit/admin-platform-node-coverage.test.ts` verifies every locally supported resolver maps to a live Node implementor and snapshots unsupported implementors plus implemented singular roots that still intentionally return `null` through generic Node dispatch. HAR-424 tracks reducing that unsupported snapshot.
- `job(id:)` mirrors the captured arbitrary-job behavior: a requested Job GID returns a completed job payload with that id and a selected `query { __typename }` QueryRoot link. The proxy does not model async job lifecycle state yet.
- `domain(id:)` resolves the effective snapshot shop `primaryDomain` by id when one is present; unknown ids return `null`.
- `backupRegion` first returns any explicitly staged or snapshot `backupRegion`, then derives a store-specific `MarketRegionCountry` from the effective shop's `myshopifyDomain` and `shopAddress.countryCodeV2` when that domain/country pair is backed by checked-in conformance evidence. The captured `MarketRegionCountry` ids are treated as shop-domain-scoped evidence, not as platform-global ids for a country. The current backed domain/country boundary is `harry-test-heelo.myshopify.com` for `CA`, `AE`, `AT`, `AU`, `BE`, `CH`, `CZ`, `DE`, `DK`, `ES`, `FI`, and `MX`, plus `very-big-test-store.myshopify.com` for `CA` and `US`. Snapshot parity requests that do not include shop state still preserve the HAR-315 captured Canada fallback, while an effective shop with an unbacked domain/country returns `null` instead of receiving the captured Canada region.
- `taxonomy.categories(...)` returns an empty connection shape for the captured unmatched search/no-data branch. The global non-empty taxonomy catalog is not modeled.
- `staffMember` and `staffMembers` return the captured field-level `ACCESS_DENIED` blocker locally. Authorized staff identity/catalog reads require an eligible app/store and a separate local staff model before support can broaden.

### Access-Scoped Behavior

- `staffMember` requires `read_users` access and additional Shopify app/store eligibility. The checked-in conformance fixture captures the current credential's `ACCESS_DENIED` response, so snapshot mode mirrors that blocker instead of inventing staff identities.
- `staffMembers` is treated as the same restricted staff surface. The local handler returns `null` plus the captured access error until authorized staff catalog evidence and a staff state model exist.
- Generic `node` / `nodes` dispatch is intentionally limited to resource families whose serializers already project local state through the requested selection set. The admin-platform handler does not create new domain support by itself; unsupported GID families return Shopify-like `null` entries rather than partially fabricated objects. The unsupported list is executable test evidence, not a permanent support claim.

### Mutation Behavior

`backupRegionUpdate` stages the selected fallback region in the in-memory admin platform state and updates downstream snapshot `backupRegion` reads without mutating Shopify at runtime. HAR-374 conformance covers the current conformance shop's idempotent `CA` success branch and `REGION_NOT_FOUND` validation for an unknown country code. Mutation support intentionally remains `CA`-only until more `backupRegionUpdate` success captures exist; the broader read mapping above is derived from read-only market-region evidence and does not broaden mutation support.

`flowGenerateSignature` is locally short-circuited for proxy-local Flow trigger IDs. The proxy returns a deterministic local signature, stores only payload/signature SHA-256 hashes in meta state, and keeps the original raw mutation in the mutation log for eventual commit replay. Unknown Flow trigger IDs mirror the captured Shopify `RESOURCE_NOT_FOUND` top-level error.

`flowTriggerReceive` records proxy-local trigger receipts for handles with the `local-` / HAR-374 local prefix. It does not deliver any external Flow trigger at runtime. The staged meta state records only the handle, payload byte count, and payload hash; the raw payload remains in the mutation log request body for commit replay. Captured validation branches include unknown handle and payloads whose JSON representation exceeds 50000 bytes.

## Historical and developer notes

- Conformance evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json`, plus market-region country evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/markets-baseline.json` and `fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets/markets-baseline.json`.
- HAR-400 expanded executable runtime coverage for local Product and primary Domain resolution through the generic `Node` interface.
- HAR-418 expanded generic Node dispatch to existing supported local detail handlers/direct serializers, added executable parity for captured Product, Collection, Customer, and Location `nodes(ids:)` reads, and added an introspection-backed unsupported Node implementor snapshot. Follow-up HAR-424 tracks reducing the remaining unsupported Node list.
- Executable parity specs: `admin-platform-supported-node-reads.json`, `admin-platform-utility-reads.json`, `admin-platform-backup-region-update.json`, `admin-platform-flow-generate-signature.json`, and `admin-platform-flow-trigger-receive.json`.

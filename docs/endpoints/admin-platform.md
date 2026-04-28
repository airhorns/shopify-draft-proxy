# Admin Platform Utility Roots

This endpoint group covers Admin GraphQL platform/utility roots that do not belong to a merchant resource family yet:

- queries: `publicApiVersions`, `node`, `nodes`, `job`, `taxonomy`, `domain`, `backupRegion`, `staffMember`, `staffMembers`
- mutations: `backupRegionUpdate`, `flowGenerateSignature`, `flowTriggerReceive`

HAR-315/HAR-418 conformance evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json`.

## Current support and limitations

### Snapshot Read Behavior

The local snapshot handler is intentionally conservative and only models shapes backed by the HAR-315 capture:

- `publicApiVersions` returns the captured 2026-04 version window. Refresh the fixture and constant when Shopify rotates supported Admin API versions.
- `node(id:)` resolves locally modeled resource GIDs by dispatching the GID type to the existing local detail handler or direct serializer for that resource family. This includes product/catalog/inventory records, product options and option values, effective owner-scoped metafields, customer and payment-method records, B2B Company records, store/shop/location/business-entity records, Files API records, saved searches, payment terms templates/customizations, captured-safe finance/POS/dispute no-data roots, BulkOperation, metafield/metaobject definitions, order/fulfillment/return/draft-order records, GiftCard, DeliveryProfile, discount owner and `DiscountNode` wrapper reads, marketing/event/webhook/segment records, Market/MarketRegionCountry/MarketCatalog/MarketWebPresence/PriceList records, and supported online-store content/integration records when those records are already present in local state.
- `nodes(ids:)` applies the same GID-type dispatch per input id, preserves input order, and returns `null` entries for malformed, missing, or unsupported ids.
- HAR-418 records the live Shopify `Node` interface `possibleTypes` in the admin-platform conformance fixture. `tests/unit/admin-platform-node-coverage.test.ts` verifies every locally supported resolver maps to a live Node implementor and snapshots unsupported implementors plus implemented singular roots that still intentionally return `null` through generic Node dispatch. HAR-424 tracks reducing that unsupported snapshot.
- `job(id:)` mirrors the captured arbitrary-job behavior: a requested Job GID returns a completed job payload with that id and a selected `query { __typename }` QueryRoot link. The proxy does not model async job lifecycle state yet.
- `domain(id:)` resolves the effective snapshot shop `primaryDomain` by id when one is present; unknown ids return `null`.
- `backupRegion` returns the captured or locally staged `MarketRegionCountry` slice for the current conformance shop. Generic Node dispatch resolves that same effective backup-region record by GID; broader shop-country-to-region id mapping remains a gap.
- `taxonomy.categories(...)` is backed by normalized taxonomy category records in snapshot/local state. HAR-414 captures representative 2026-04 taxonomy catalog pages and an `apparel` search slice, including hierarchy fields (`fullName`, `isRoot`, `isLeaf`, `level`, `parentId`, `ancestorIds`, `childrenIds`, `isArchived`), raw Shopify cursors, and selected `pageInfo`. Search support is intentionally limited to simple term matching over captured `id`, `name`, and `fullName` values; unmatched searches return the captured empty connection shape. The proxy does not invent taxonomy categories and does not claim exhaustive global catalog coverage beyond records present in snapshot/local state.
- `staffMember` and `staffMembers` return the captured field-level `ACCESS_DENIED` blocker locally. Authorized staff identity/catalog reads require an eligible app/store and a separate local staff model before support can broaden.

### Access-Scoped Behavior

- `staffMember` requires `read_users` access and additional Shopify app/store eligibility. The checked-in conformance fixture captures the current credential's `ACCESS_DENIED` response, so snapshot mode mirrors that blocker instead of inventing staff identities.
- `staffMembers` is treated as the same restricted staff surface. The local handler returns `null` plus the captured access error until authorized staff catalog evidence and a staff state model exist.
- Generic `node` / `nodes` dispatch is intentionally limited to resource families whose serializers already project local state through the requested selection set. The admin-platform handler does not create new domain support by itself; unsupported GID families return Shopify-like `null` entries rather than partially fabricated objects. The unsupported list is executable test evidence, not a permanent support claim.

### Mutation Behavior

`backupRegionUpdate` stages the selected fallback region in the in-memory admin platform state and updates downstream snapshot `backupRegion` reads without mutating Shopify at runtime. HAR-374 conformance covers the current conformance shop's idempotent `CA` success branch and `REGION_NOT_FOUND` validation for an unknown country code. The local country mapping is intentionally narrow until more shop-country captures exist.

`flowGenerateSignature` is locally short-circuited for proxy-local Flow trigger IDs. The proxy returns a deterministic local signature, stores only payload/signature SHA-256 hashes in meta state, and keeps the original raw mutation in the mutation log for eventual commit replay. Unknown Flow trigger IDs mirror the captured Shopify `RESOURCE_NOT_FOUND` top-level error.

`flowTriggerReceive` records proxy-local trigger receipts for handles with the `local-` / HAR-374 local prefix. It does not deliver any external Flow trigger at runtime. The staged meta state records only the handle, payload byte count, and payload hash; the raw payload remains in the mutation log request body for commit replay. Captured validation branches include unknown handle and payloads whose JSON representation exceeds 50000 bytes.

## Historical and developer notes

- Conformance evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json`.
- HAR-400 expanded executable runtime coverage for local Product and primary Domain resolution through the generic `Node` interface.
- HAR-413 added generic `node` / `nodes` dispatch for the discount `DiscountNode` wrapper when a requested `DiscountCodeNode` or `DiscountAutomaticNode` id already exists in normalized discount state; missing discount ids and unrelated unsupported GID families still return `null`.
- HAR-414 expanded taxonomy category coverage from empty/no-data only to captured non-empty catalog/search slices with hierarchy-field, cursor, and `pageInfo` parity.
- HAR-418 expanded generic Node dispatch to existing supported local detail handlers/direct serializers, added executable parity for captured Product, Collection, Customer, and Location `nodes(ids:)` reads, and added an introspection-backed unsupported Node implementor snapshot. HAR-424 adds ProductOption/ProductOptionValue Node dispatch on top of the existing product option lifecycle model; rework also adds owner-scoped Metafield Node reads, effective backup-region `MarketRegionCountry`, staged/captured `MarketWebPresence`, and captured-safe no-data dispatch for `CashTrackingSession`, `PointOfSaleDevice`, and `ShopifyPaymentsDispute`. Product taxonomy, product delete operation, product variant component, quantity price break, selling-plan, non-empty finance/POS/dispute, and broader market-region catalog Node implementors remain unsupported until their owning lifecycle/read models have executable Node evidence.
- Executable parity specs: `admin-platform-supported-node-reads.json`, `admin-platform-product-option-node-reads.json`, `admin-platform-metafield-node-reads.json`, `admin-platform-market-region-node-read.json`, `admin-platform-market-web-presence-node-read.json`, `admin-platform-finance-risk-node-no-data.json`, `admin-platform-utility-reads.json`, `admin-platform-backup-region-update.json`, `admin-platform-flow-generate-signature.json`, and `admin-platform-flow-trigger-receive.json`.

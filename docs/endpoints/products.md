# Products Endpoint Group

The products group is fully implemented in the operation registry. It covers product roots plus the directly related inventory, metafield, collection, publication, tag, and product-media roots that are modeled as product-owned behavior.

## Supported roots

Overlay reads:

- `product`
- `products`
- `productsCount`
- `productFeed`
- `productFeeds`
- `productVariant`
- `inventoryItem`
- `inventoryLevel`
- `collection`
- `collectionByIdentifier`
- `collectionByHandle`
- `collections`
- `locations`
- `publications`

Local staged mutations:

- `tagsAdd`
- `tagsRemove`
- `productCreate`
- `productUpdate`
- `productDelete`
- `productDuplicate`
- `productSet`
- `productChangeStatus`
- `productPublish`
- `productUnpublish`
- `productOptionsCreate`
- `productOptionUpdate`
- `productOptionsDelete`
- `productVariantsBulkCreate`
- `productVariantsBulkUpdate`
- `productVariantsBulkDelete`
- `productVariantsBulkReorder`
- `productVariantCreate`
- `productVariantUpdate`
- `productVariantDelete`
- `productCreateMedia`
- `productUpdateMedia`
- `productDeleteMedia`
- `productReorderMedia`
- `inventoryItemUpdate`
- `inventoryAdjustQuantities`
- `inventoryActivate`
- `inventoryDeactivate`
- `inventoryBulkToggleActivation`
- `metafieldsSet`
- `metafieldsDelete`
- `metafieldDelete`
- `collectionCreate`
- `collectionUpdate`
- `collectionDelete`
- `collectionAddProducts`
- `collectionRemoveProducts`
- `collectionReorderProducts`

## Registered product merchandising gaps

These product merchandising roots are registered in the operation registry as product-domain gaps, but are not local mutation support yet. They still proxy as unsupported mutations at runtime and must not be treated as supported until success-path staging and downstream read-after-write behavior are modeled:

- `productFeedCreate`
- `productFeedDelete`
- `productFullSync`
- `productBundleCreate`
- `productBundleUpdate`
- `combinedListingUpdate`

## Behavior notes

- Product feed reads currently support Shopify-like no-data behavior in snapshot mode. Captured 2025-01 `harry-test-heelo` evidence returns `productFeed(id:)` as `null` for an absent feed id and `productFeeds(first:)` as an empty connection with empty `nodes`/`edges`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors. Live-hybrid `productFeed` / `productFeeds` requests continue to proxy upstream because staged product mutations do not currently model feed-channel membership.
- Product feed mutations remain unsupported. The 2025-01 `harry-test-heelo` probe for `productFeedCreate(country: US, language: EN)` returned a top-level `NOT_FOUND` error, `Unable to find channel for product feed`; `productFeedDelete` and `productFullSync` unknown feed ids returned payload userErrors with `field: ["id"]` and `ProductFeed does not exist`. Local staging needs channel-backed success evidence and downstream feed read effects before these roots can become supported.
- Product bundle and combined-listing mutations remain unsupported. Captured guardrails cover `productBundleCreate` with empty components (`productBundleOperation: null`, user error `At least one component is required.`), `productBundleUpdate` with an unknown product (`productBundleOperation: null`, user error `Product does not exist`), and `combinedListingUpdate` with an unknown parent product (`product: null`, code `PARENT_PRODUCT_NOT_FOUND`). Local staging needs component-backed bundle success evidence, `ProductBundleOperation` lifecycle/status behavior, combined-listing child relationship evidence, and downstream product reads before these roots can become supported.
- Product-domain metafields are normalized as owner-scoped records for `Product`, `ProductVariant`, and `Collection` owners. Besides `id`, `namespace`, `key`, `type`, and `value`, hydrated and staged records carry `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`, and `ownerType` for owner-scoped parity. Metafield `definition` serializes as `null` until fixture evidence justifies modeling definition linkage.
- Local `metafieldsSet` support covers product, product variant, and collection owners only. It validates the full input batch before replacing each affected owner metafield set, supports compare-and-set through `compareDigest`, treats `compareDigest: null` as a create-only guard, and preserves Shopify-like atomic no-write behavior when any modeled resolver error is returned. Customer, order, draft-order, shop, discount, and other owner families remain scoped to their own endpoint groups or future issues.
- `metafieldsDelete` uses the same product-domain owner scope and returns ordered `deletedMetafields` entries, including `null` for missing namespace/key rows. Downstream `product(id:)`, `productVariant(id:)`, and `collection(id:)` reads expose staged owner-specific singular `metafield(namespace:, key:)` and `metafields` connection results without live writes.
- Product search uses the shared Shopify-style search parser. Endpoint-specific product behavior includes boolean grouping, quoted values, field comparators, simple term-list searches, variant search terms, sort keys, and captured connection cursor/pageInfo baselines.
- Live schema introspection confirms product reorder roots for variant order (`productVariantsBulkReorder`), media order (`productReorderMedia`), option/value order (`productOptionsReorder`), and collection product order (`collectionReorderProducts`). Local support currently covers variant list reordering and media list reordering because the normalized state already has ordered variant/media connections; `productOptionsReorder` remains registry-only until option/value reorder semantics and Shopify's resulting variant order recalculation are backed by conformance evidence.
- `productVariantsBulkReorder` stages an ordered effective variant list from `ProductVariantPositionInput.position` and exposes that ordering through downstream `product.variants(...)` and `productVariant(id:)` reads without runtime Shopify writes.
- `productReorderMedia` stages `MoveInput` media moves, returns an async-style `Job`, and exposes reordered `product.media(...)` and `product.images(...)` connections without runtime Shopify writes.
- Collection records carry aggregate publication target ids alongside product publication ids. A staged `collectionCreate` starts unpublished; collection publication counts and `publishedOnPublication(publicationId:)` remain unpublished until a local publish mutation adds a target.
- `publishedOnCurrentPublication` is not inferred from aggregate collection publication count. Captured Online Store publishable writes leave it false when the app current publication is not the target.
- Local `publishablePublish` and `publishableUnpublish` currently stage Product and Collection publishables. Broader publishable implementers remain unsupported in their own groups.
- Product handle generation and validation follows the captured product mutation slice: duplicate title-generated handles are de-duplicated, explicit handles are normalized before uniqueness checks, Unicode letters/numbers are preserved, punctuation-only explicit handles fall back into the `product` handle family, explicit collisions return `['input', 'handle']` userErrors, and explicit handles longer than 255 characters return `['handle']` userErrors without staging partial state. The HAR-22 live probe found no product reserved-word rejection for handles such as `admin`, `products`, `collections`, `cart`, `checkout`, or `new`.
- Product option lifecycle staging is fixture-backed for `productOptionsCreate`, `productOptionUpdate`, and `productOptionsDelete`. The current conformance fixtures cover replacing Shopify's default `Title` option with created options, keeping non-variant option values in `optionValues` but out of `values`, renaming and repositioning options, adding/updating/deleting option values, reordering variant `selectedOptions` after option repositioning, and restoring Shopify's default option/variant graph when all custom options are deleted. Expected parity differences are limited to generated `ProductOption` and `ProductOptionValue` GIDs.
- Captured option lifecycle validation branches include `productOptionsCreate` with an unknown product (`field: ["productId"]`, `Product does not exist`), `productOptionUpdate` with an unknown option (`field: ["option"]`, `Option does not exist`), and `productOptionsDelete` with an unknown option id (`field: ["options", "0"]`, `Option does not exist`). These branches stage no upstream Shopify writes.
- Top-level `products(query: "published_status:...")` and `productsCount(query: "published_status:...")` apply locally modeled aggregate product publication visibility. A product is treated as published for this filter only when it is `ACTIVE` and has at least one staged or hydrated publication target; `DRAFT` and `ARCHIVED` products remain unpublished even when publication targets are staged. Richer publication graph/detail parity remains limited to the aggregate product fields listed in the validation fixtures.
- Top-level `collections(query: "published_status:...")` applies locally modeled aggregate collection publication state for staged and snapshot reads.
- Product-side collection membership effects are modeled as normalized product collection rows. `productSet(input.collections)` replaces the product's effective memberships, while `productDuplicate` copies the source product's effective memberships onto the staged duplicate; downstream `product.collections`, `collection(id:)`, collection `products`, `productsCount`, `hasProduct`, and top-level `collections(query: "product_id:...")` reads resolve from the same staged membership rows.
- `productSet(input.variants[].inventoryQuantities[])` accepts the live Shopify shape with `locationId`, `name`, and `quantity`. Staged create and update flows store those entries as inventory item `inventoryLevels` rows instead of only collapsing them onto the variant. Downstream `product`, `productVariant`, and `inventoryItem` reads expose the location-level `inventoryLevels`, selected `quantities(names: ...)`, aggregate variant `inventoryQuantity`, and product `totalInventory` from the staged graph. Current live evidence uses `name: "available"`; the local row mirrors that quantity into `on_hand` for read parity and leaves `incoming` at `0` unless separately hydrated.
- Product-level `totalInventory` intentionally follows the captured `productSet` timing rather than the generic variant mutation summary path: synchronous create counted the tracked variant's available quantity, while a follow-up `productSet` variant inventory update changed variant and inventory-item quantities immediately but left `product.totalInventory` at the prior aggregate in both the mutation payload and immediate downstream reads.
- `collectionByIdentifier` supports id and handle identifier branches against effective local collection state. `customId` returns `null` until collection unique-metafield evidence exists.
- `collectionByHandle` is a deprecated Shopify root but is supported as a handle lookup over effective local collection state.
- Missing product-adjacent by-id roots return `null` without inventing records. The `product-related-by-id-not-found-read` parity scenario captures this for `collection(id:)`, `productVariant(id:)`, `inventoryItem(id:)`, and `inventoryLevel(id:)`.
- Product media validation follows the captured Shopify branches in `product-media-validation-branches`, which is replayed by `pnpm conformance:parity` against the local proxy. Unknown product IDs return `Product does not exist` media errors with null media/delete payload slots; invalid image `originalSource` values return indexed `media.<index>.originalSource` errors; invalid `CreateMediaInput.mediaContentType` enum values return top-level `INVALID_VARIABLE` errors. Empty media/update/delete lists are accepted as empty successes. Mixed create batches stage valid media and report invalid entries, while mixed update/delete batches with unknown media IDs are rejected atomically and leave staged media unchanged.

## Validation anchors

- Runtime flows: `tests/integration/product-draft-flow.test.ts`
- Product reads: `tests/integration/product-query-shapes.test.ts`
- Collection reads and mutations: `tests/integration/collection-query-shapes.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Location and publication reads: `tests/integration/location-query-shapes.test.ts`, `tests/integration/publication-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/product*.json`, `config/parity-specs/products*.json`, `config/parity-specs/collection*.json`, `config/parity-specs/metafieldsSet-owner-expansion.json`, and matching files under `config/parity-requests/`
- Product merchandising read fixture: `config/parity-specs/product-feeds-empty-read.json`, `config/parity-requests/product-feeds-empty-read.graphql`, and `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-feeds-empty-read.json`
- Product merchandising mutation guardrail fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-merchandising-mutation-probes.json`
- Product handle validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-handle-validation-parity.json`

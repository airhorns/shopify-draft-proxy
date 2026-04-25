# Products Endpoint Group

The products group is fully implemented in the operation registry. It covers product roots plus the directly related inventory, metafield, collection, publication, tag, and product-media roots that are modeled as product-owned behavior.

## Supported roots

Overlay reads:

- `product`
- `products`
- `productsCount`
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
- `productVariantCreate`
- `productVariantUpdate`
- `productVariantDelete`
- `productCreateMedia`
- `productUpdateMedia`
- `productDeleteMedia`
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

## Behavior notes

- Product-domain metafields are normalized as owner-scoped records for `Product`, `ProductVariant`, and `Collection` owners. Besides `id`, `namespace`, `key`, `type`, and `value`, hydrated and staged records carry `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`, and `ownerType` for owner-scoped parity. Metafield `definition` serializes as `null` until fixture evidence justifies modeling definition linkage.
- Local `metafieldsSet` support covers product, product variant, and collection owners only. It validates the full input batch before replacing each affected owner metafield set, supports compare-and-set through `compareDigest`, treats `compareDigest: null` as a create-only guard, and preserves Shopify-like atomic no-write behavior when any modeled resolver error is returned. Customer, order, draft-order, shop, discount, and other owner families remain scoped to their own endpoint groups or future issues.
- `metafieldsDelete` uses the same product-domain owner scope and returns ordered `deletedMetafields` entries, including `null` for missing namespace/key rows. Downstream `product(id:)`, `productVariant(id:)`, and `collection(id:)` reads expose staged owner-specific singular `metafield(namespace:, key:)` and `metafields` connection results without live writes.
- Product search uses the shared Shopify-style search parser. Endpoint-specific product behavior includes boolean grouping, quoted values, field comparators, simple term-list searches, variant search terms, sort keys, and captured connection cursor/pageInfo baselines.
- Collection records carry aggregate publication target ids alongside product publication ids. A staged `collectionCreate` starts unpublished; collection publication counts and `publishedOnPublication(publicationId:)` remain unpublished until a local publish mutation adds a target.
- `publishedOnCurrentPublication` is not inferred from aggregate collection publication count. Captured Online Store publishable writes leave it false when the app current publication is not the target.
- Local `publishablePublish` and `publishableUnpublish` currently stage Product and Collection publishables. Broader publishable implementers remain unsupported in their own groups.
- Top-level `products(query: "published_status:...")` and `productsCount(query: "published_status:...")` apply locally modeled aggregate product publication visibility. A product is treated as published for this filter only when it is `ACTIVE` and has at least one staged or hydrated publication target; `DRAFT` and `ARCHIVED` products remain unpublished even when publication targets are staged. Richer publication graph/detail parity remains limited to the aggregate product fields listed in the validation fixtures.
- Top-level `collections(query: "published_status:...")` applies locally modeled aggregate collection publication state for staged and snapshot reads.
- `collectionByIdentifier` supports id and handle identifier branches against effective local collection state. `customId` returns `null` until collection unique-metafield evidence exists.
- `collectionByHandle` is a deprecated Shopify root but is supported as a handle lookup over effective local collection state.
- Missing product-adjacent by-id roots return `null` without inventing records. The `product-related-by-id-not-found-read` parity scenario captures this for `collection(id:)`, `productVariant(id:)`, `inventoryItem(id:)`, and `inventoryLevel(id:)`.

## Validation anchors

- Runtime flows: `tests/integration/product-draft-flow.test.ts`
- Product reads: `tests/integration/product-query-shapes.test.ts`
- Collection reads and mutations: `tests/integration/collection-query-shapes.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Location and publication reads: `tests/integration/location-query-shapes.test.ts`, `tests/integration/publication-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/product*.json`, `config/parity-specs/products*.json`, `config/parity-specs/collection*.json`, `config/parity-specs/metafieldsSet-owner-expansion.json`, and matching files under `config/parity-requests/`

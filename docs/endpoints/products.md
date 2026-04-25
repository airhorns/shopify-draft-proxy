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

- Product-owned metafields are normalized as product-scoped records. Besides `id`, `namespace`, `key`, `type`, and `value`, hydrated and staged product metafields carry `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`, and `ownerType` for owner-scoped parity. Product metafield `definition` serializes as `null` until fixture evidence justifies modeling definition linkage.
- Local `metafieldsSet` support is product-owned only. It validates the full input batch before replacing the staged product metafield set, supports compare-and-set through `compareDigest`, treats `compareDigest: null` as a create-only guard, and preserves Shopify-like atomic no-write behavior when any modeled resolver error is returned.
- Product search uses the shared Shopify-style search parser. Endpoint-specific product behavior includes boolean grouping, quoted values, field comparators, simple term-list searches, variant search terms, sort keys, and captured connection cursor/pageInfo baselines.
- Collection records carry aggregate publication target ids alongside product publication ids. A staged `collectionCreate` starts unpublished; collection publication counts and `publishedOnPublication(publicationId:)` remain unpublished until a local publish mutation adds a target.
- `publishedOnCurrentPublication` is not inferred from aggregate collection publication count. Captured Online Store publishable writes leave it false when the app current publication is not the target.
- Local `publishablePublish` and `publishableUnpublish` currently stage Product and Collection publishables. Broader publishable implementers remain unsupported in their own groups.
- Top-level `collections(query: "published_status:...")` applies locally modeled aggregate collection publication state for staged and snapshot reads.
- `collectionByIdentifier` supports id and handle identifier branches against effective local collection state. `customId` returns `null` until collection unique-metafield evidence exists.
- `collectionByHandle` is a deprecated Shopify root but is supported as a handle lookup over effective local collection state.

## Validation anchors

- Runtime flows: `tests/integration/product-draft-flow.test.ts`
- Product reads: `tests/integration/product-query-shapes.test.ts`
- Collection reads and mutations: `tests/integration/collection-query-shapes.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Location and publication reads: `tests/integration/location-query-shapes.test.ts`, `tests/integration/publication-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/product*.json`, `config/parity-specs/products*.json`, `config/parity-specs/collection*.json`, and matching files under `config/parity-requests/`

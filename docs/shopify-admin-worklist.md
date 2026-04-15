# Shopify Admin GraphQL Worklist

This document is the long-term execution map for the proxy. The intent is to track **every relevant Admin GraphQL query and mutation** as a concrete work item.

Status legend:

- `[ ]` not started
- `[-]` planned / partially scoped
- `[~]` in progress
- `[x]` implemented
- `[c]` conformance-covered

## Rules

1. Do not mark an operation implemented unless it has runtime tests.
2. Do not mark an operation conformance-covered unless it is compared against real Shopify behavior.
3. Breadth comes after product-domain depth.
4. If Shopify behavior is uncertain, add a conformance scenario instead of guessing.

---

## Product domain

### Queries
- [x] `product` (id-based reads, staged overlay, snapshot empty/null behavior)
  - [x] richer detail slice on product reads (`descriptionHtml`, `onlineStorePreviewUrl`, `templateSuffix`, `seo { title description }`, `category { id fullName }`)
- [x] `products` (nodes/edges/pageInfo with simple first slicing, staged overlay)
  - [x] richer catalog merchandising slice on list reads (`legacyResourceId`, `vendor`, `productType`, `tags`, `totalInventory`, `tracksInventory`)
- [x] `productsCount` (top-level catalog counts with alias support and overlay-aware query filtering)
- [x] product publications / publication status read paths
  - [x] aggregate publication-status slice on product reads (`publishedOnCurrentPublication`, `availablePublicationsCount`, `resourcePublicationsCount`)
- [x] product collections membership read paths (hybrid upstream hydration + empty staged connection serialization)
  - [x] nested collections connection pagination (`first`/`after`) with overlay-aware `pageInfo`
- [x] product metafields read paths (owner-scoped `metafield(namespace:, key:)` + `metafields` connection for `id`, `namespace`, `key`, `type`, and `value`)
  - [x] nested metafields connection pagination (`first`/`after`) with overlay-aware `pageInfo`
- [x] product media read paths (connection serialization for `mediaContentType`, `alt`, and `preview.image.url`; staged empty connection behavior)
  - [x] richer media node selection on staged/overlay reads (`id`, `status`, and `... on MediaImage { image { url } }`)
  - [x] nested media connection pagination (`first`/`after`) with overlay-aware `pageInfo`
- [x] product options read paths (array serialization, staged default Title option, hybrid upstream option hydration)
- [x] product variants read paths (default staged variant + hybrid variant hydration + connection serialization)
  - [x] nested variants connection pagination (`first`/`after`) with selection-aware edges and `pageInfo`
  - [x] variant merchandising + inventory slice on product reads (`sku`, `barcode`, `price`, `compareAtPrice`, `taxable`, `inventoryPolicy`, `inventoryQuantity`, `selectedOptions`, `inventoryItem { id tracked requiresShipping }`)
  - [x] richer inventory item detail on product variant reads (`inventoryItem.measurement.weight { unit value }`, `countryCodeOfOrigin`, `provinceCodeOfOrigin`, `harmonizedSystemCode`)
- [ ] product images compatibility read paths if needed for older versions
- [-] search/filter/sort semantics for products
  - [x] initial overlay-read support for `query:` terms `vendor`, `status`, and `inventory_total`, plus `sortKey: TITLE|INVENTORY_TOTAL`
  - [x] additional overlay-read query terms `title`, `handle`, `tag`, and `product_type`, plus `sortKey: UPDATED_AT` with `reverse: true`
  - [x] variant-backed overlay-read query terms `sku` and `barcode`, with counts sharing the same filtered product set
  - [x] timestamp overlay-read query terms `created_at` and `updated_at` with comparator support (`<`, `<=`, `>`, `>=`, `=`) and counts sharing the same filtered product set
  - [x] a first broader grammar slice: quoted phrases, bare text terms, and leading `-` negation across overlay `products` / `productsCount`
  - [x] additional sort keys `VENDOR` and `PRODUCT_TYPE`
  - [x] additional overlay sort keys `HANDLE` and `STATUS` across `products` and derived `collection.products`
  - [-] broader Shopify grammar beyond that first slice and more sort keys
    - [x] grouped OR expressions and trailing `*` prefix matching across overlay `products` / `productsCount`
    - [ ] additional grammar parity beyond that (for example richer grouping precedence/details and more sort keys)
- [x] pagination semantics for product connections
  - [x] top-level `products` forward cursor pagination with `after` across default and filtered/sorted overlay reads
  - [x] top-level `products` backward cursor pagination with `before`/`last` across default and filtered/sorted overlay reads

### Mutations
- [x] `productCreate` (staged locally with synthetic id/timestamps and simple Shopify-like payload)
  - [x] staged merchandising/detail input slice (`vendor`, `productType`, `tags`, `descriptionHtml`, `templateSuffix`, `seo`) with downstream read/filter visibility
- [x] `productUpdate` (staged locally with merge-over-existing behavior)
  - [x] hydrated rich-field overlay updates preserve untouched upstream detail fields while applying staged merchandising/detail edits
- [x] `productDelete` (staged locally with downstream null/list removal behavior)
- [x] `productDuplicate`
  - [x] staged duplicate of the effective product graph (`options`, `variants`, `collections`, `media`, `metafields`) with new synthetic product/variant/option/inventory/metafield ids and downstream read/count visibility
- [x] `productChangeStatus` (staged locally with downstream status visibility on `product`, `products`, and `productsCount`)
- [x] `productPublish`
- [x] `productUnpublish`
  - [x] staged aggregate publication-count/status visibility on downstream `product` reads
- [x] `productSet`
  - [x] first staged sync/async pass for scalar/detail fields plus list-field replacement of `productOptions`, `variants`, `collections`, and `metafields`, with downstream `product` / `products` / `productsCount` visibility
- [x] `productOptionsCreate`
- [x] `productOptionUpdate`
- [x] `productOptionsDelete`
  - [x] local LEAVE_AS_IS-style option list staging for name/position/value edits with downstream `product.options` visibility
- [x] `productVariantsBulkCreate`
- [x] `productVariantsBulkUpdate`
- [x] `productVariantsBulkDelete`
  - [x] staged variant merchandising/inventory slice (`sku`, `barcode`, `price`, `compareAtPrice`, `taxable`, `inventoryPolicy`, `inventoryQuantity`, `inventoryItem { tracked requiresShipping }`) with downstream `product.variants`, variant-backed search, and derived `totalInventory` / `tracksInventory` visibility
- [x] `productVariantCreate` (version-relevant compatibility path staged through the local variant overlay model)
- [x] `productVariantUpdate`
- [x] `productVariantDelete`
- [x] single-variant compatibility mutation family (`productVariantCreate` / `productVariantUpdate` / `productVariantDelete`) with downstream `product.variants`, `products`, and `productsCount` visibility
- [x] product media create/update/delete family
  - [x] local image-media staging for `productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia` with downstream `product.media` visibility, `mediaUserErrors`, and delete id payloads
- [ ] product reorder / sort operations if supported in Admin GraphQL
- [x] product tags side-effect mutations (`tagsAdd`, `tagsRemove` for product resources)
  - [x] downstream `product`, `products(query: "tag:...")`, and `productsCount(query: "tag:...")` visibility stays aligned after staged tag edits
- [ ] product collections side-effect mutations as applicable beyond the dedicated collection membership family

### Product-specific fidelity work items
- [x] stable synthetic GID generation for products and variants (product-only so far)
- [x] stable synthetic timestamps (product-only so far)
- [-] handle generation / uniqueness behavior (slugified titles only; no uniqueness conflict modeling yet)
- [-] status transitions and publication visibility semantics (`productChangeStatus` now stages status-only transitions; aggregate publication count/current-publication visibility now stages locally, while richer publication graph/detail parity still pending)
- [-] userErrors parity for invalid product inputs (missing id covered for update/delete/changeStatus only)
- [x] list-query visibility after create/update/delete
- [x] top-level count visibility after create/update/delete
- [x] downstream variant count / option consistency
  - [x] staged single/bulk variant mutations now keep `product.options` aligned by flipping `optionValues.hasVariants` and auto-extending option values when selected options introduce new values
- [x] metafield visibility after staged writes (`metafieldsSet` + `metafieldDelete` with downstream `product.metafield` / `product.metafields` overlay reads)
- [x] collection membership read behavior after staged mutations
  - [x] repeated staged collection replacement now drops superseded memberships cleanly so `product.collections`, top-level `collection(id:)`, and derived `collections` stay aligned after successive `productSet` writes

---

## Metafields domain

### Queries
- [x] metafield owner-scoped read coverage (product owner via `product.metafield(namespace:, key:)`)
- [x] metafields connections on supported owner types (product owner via `product.metafields`)

### Mutations
- [x] `metafieldsSet`
- [x] `metafieldDelete`

---

## Collections domain

### Queries
- [x] `collection`
- [x] `collections`
- [x] collection products connection
  - [x] top-level collection surface is currently derived from the known product↔collection membership graph (staged state and hydrated product reads), with cursor pagination on `collections` and the usual overlay product list semantics on `collection.products`

### Mutations
- [x] `collectionCreate`
- [x] `collectionUpdate`
- [x] `collectionDelete`
- [x] first standalone collection staging slice with downstream `collection`, `collections`, and nested `product.collections` visibility
- [x] collection membership mutations
  - [x] `collectionAddProducts` with atomic duplicate-member rejection and downstream `collection.products` / `product.collections` visibility
  - [x] `collectionRemoveProducts` with immediate local membership removal plus a synthetic done `job` payload for the first async pass

---

## Inventory / variants domain

### Queries
- [-] variant inventory read paths
  - [x] top-level `productVariant` read path for the common merchandising + inventory slice (`sku`, `barcode`, `price`, `compareAtPrice`, `taxable`, `inventoryPolicy`, `inventoryQuantity`, `selectedOptions`, `inventoryItem`) plus nested parent `product`
- [-] inventory item read paths
  - [x] top-level `inventoryItem` read path for `tracked`, `requiresShipping`, `measurement.weight`, `countryCodeOfOrigin`, `provinceCodeOfOrigin`, and `harmonizedSystemCode`, plus nested `variant { ... product { ... } }`
- [ ] inventory levels read paths

### Mutations
- [-] inventory adjustment family relevant to products
  - [x] `inventoryAdjustQuantities` first pass for product-backed `available` quantity deltas by `inventoryItemId` / `locationId`, with downstream `product`, `productVariant`, `inventoryItem`, `products`, and `productsCount` inventory visibility
  - [ ] broader inventory adjustment parity beyond `available` (additional quantity names, richer userErrors, and fuller payload/detail parity)
- [ ] variant inventory linkage mutations

---

## Media domain

### Queries
- [ ] media-on-product read coverage

### Mutations
- [ ] media create family
- [ ] media update family
- [ ] media delete family

---

## Generic platform work items

### Proxy/runtime
- [x] versioned Shopify path handling
- [x] upstream auth header pass-through
- [x] query vs mutation classifier
- [x] unsupported-mutation passthrough marker
- [-] capability registry for overlay-read vs stage-locally vs passthrough routing
- [ ] snapshot mode
- [ ] live-hybrid mode
- [ ] pure passthrough mode

### Meta API
- [ ] `POST /__meta/reset`
- [ ] `POST /__meta/commit`
- [ ] `GET /__meta/log`
- [ ] `GET /__meta/state`
- [ ] `GET /__meta/config`
- [ ] `GET /__meta/health`

### State engine
- [ ] normalized object graph
- [ ] staged overlay engine
- [ ] raw mutation log retention
- [ ] original-order commit replay
- [ ] stop-on-first-error commit semantics

### Conformance
- [ ] scenario fixture format
- [ ] real Shopify recorder
- [ ] normalized snapshot compiler
- [ ] proxy parity runner
- [ ] operation coverage matrix
- [ ] product scenario pack
- [ ] mutation userErrors parity harness
- [ ] empty/null behavior parity harness

---

## Initial execution order

1. product query + mutation skeletons
2. normalized product state model
3. `product` + `products` query behavior
4. `productCreate`
5. `productUpdate`
6. `productDelete`
7. variant / option / metafield depth
8. conformance recorder and parity tests

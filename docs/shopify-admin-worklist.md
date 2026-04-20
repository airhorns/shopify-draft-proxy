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
5. Every implemented root operation must stay in sync with `config/operation-registry.json`; `corepack pnpm conformance:check` is the structural gate for runtime-test coverage plus required scenario manifests and parity specs.
6. Do not hand-maintain `[c]` status for root operations; use `docs/generated/worklist-conformance-status.md` and `docs/generated/conformance-status.json` as the generated source of truth.

---

## Product domain

### Queries
- [x] `product` (id-based reads, staged overlay, snapshot empty/null behavior)
  - [x] richer detail slice on product reads (`descriptionHtml`, `onlineStorePreviewUrl`, `templateSuffix`, `seo { title description }`, `category { id fullName }`)
  - [x] live empty/null conformance evidence now includes both a real product with empty `collections` / `media` connections and a dedicated missing-product / unmatched-query fixture (`product-empty-state.json`) covering `product -> null`, `productsCount -> 0`, and an empty `products` connection
- [x] `products` (nodes/edges/pageInfo with simple first slicing, staged overlay)
  - [x] richer catalog merchandising slice on list reads (`legacyResourceId`, `vendor`, `productType`, `tags`, `totalInventory`, `tracksInventory`)
- [x] `productsCount` (top-level catalog counts with alias support and overlay-aware query filtering)
- [x] product publications / publication status read paths
  - [x] aggregate publication-status slice on product reads (`publishedOnCurrentPublication`, `availablePublicationsCount`, `resourcePublicationsCount`)
  - [x] top-level `publications` catalog read path (`id`, `name`, connection pagination) with local replay from the effective publication graph plus hydrated publication names when a live publication catalog probe has been seen
  - [x] hydrated publication catalog cursors now replay through the local overlay serializer in live-hybrid mode instead of collapsing back to synthetic `cursor:<gid>` cursors when staged product state is present
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
  - [x] live search-grammar conformance capture now covers that earlier local-only slice explicitly via `products-search-grammar.json` (`"flat peak cap" accessories -vendor:VANS -tag:vans`) so quoted-phrase/bare-term/term-local-negation parity no longer rides only on integration tests
  - [x] additional sort keys `VENDOR` and `PRODUCT_TYPE`
  - [x] additional overlay sort keys `HANDLE` and `STATUS` across `products` and derived `collection.products`
  - [-] broader Shopify grammar beyond that first slice and more sort keys
    - [x] grouped OR expressions and trailing `*` prefix matching across overlay `products` / `productsCount`
    - [x] live advanced-search conformance capture now covers trailing `*` prefix matching, grouped vendor `OR`, grouped negation, and `UPDATED_AT` descending ordering with `pageInfo` on `products` plus paired `productsCount`
    - [x] live filtered-pagination conformance capture now preserves a concrete `UPDATED_AT`-desc search window plus forward `after` and backward `before`/`last` traversal for `tag:egnition-sample-data product_type:ACCESSORIES`, so cursor-window parity no longer rides only on local integration tests
    - [x] live OR-precedence conformance capture now settles one easy-to-guess parser detail: unparenthesized product queries on this host keep Shopify's current AND-before-OR behavior (`vendor:NIKE OR vendor:VANS tag:egnition-sample-data product_type:ACCESSORIES` stayed limited to the two accessory matches rather than broadening to every NIKE product)
    - [x] live sort-key conformance capture now covers the current schema-backed `ProductSortKeys` slice `TITLE`, `VENDOR`, `PRODUCT_TYPE`, `PUBLISHED_AT`, and `ID`; local `HANDLE` / `STATUS` overlay sorting remains intentionally documented as a proxy-only convenience because the live 2025-01 schema on this host rejects those enum values
    - [x] live relevance-search conformance capture now preserves a concrete `sortKey: RELEVANCE` window with opaque Shopify cursors, and staged overlay reads reuse that hydrated baseline instead of inventing a local synthetic relevance order/cursor scheme
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
  - [x] live safe-write conformance capture now covers `productDuplicate`; on this host the duplicate preserved collection memberships + product metafields immediately, but the duplicate's immediate `media` connection stayed empty even when the source product already had ready image media
- [x] `productChangeStatus` (staged locally with downstream status visibility on `product`, `products`, and `productsCount`)
- [x] `productPublish`
- [x] `productUnpublish`
  - [x] staged aggregate publication-count/status visibility on downstream `product` reads
  - [x] minimal live publish/unpublish mutation payload capture now succeeds against a real publication id on this host, and the checked-in fixtures preserve the remaining app-scoped aggregate publication-field blocker (`publishedOnCurrentPublication`, `availablePublicationsCount`, `resourcePublicationsCount`) explicitly instead of leaving the root operations as declared gaps
- [x] `productSet`
  - [x] first staged sync/async pass for scalar/detail fields plus list-field replacement of `productOptions`, `variants`, `collections`, and `metafields`, with downstream `product` / `products` / `productsCount` visibility
  - [x] live safe-write conformance capture now covers the synchronous create slice of `productSet`; on this host Shopify requires `variants[].inventoryQuantities[]` to include both `locationId` and quantity `name` (`available` in the capture), not just a bare quantity
- [x] `productOptionsCreate`
- [x] `productOptionUpdate`
- [x] `productOptionsDelete`
  - [x] local LEAVE_AS_IS-style option list staging for name/position/value edits with downstream `product.options` visibility
  - [x] live safe-write conformance captures now cover the full product option mutation family (`productOptionsCreate`, `productOptionUpdate`, `productOptionsDelete`) against the dev store
  - [x] local option behavior now mirrors the captured Shopify quirks: creating options on a default-only product replaces the synthetic `Title` option, deleting the last custom option restores a fresh default `Title` option, and `product.options[].values` reflects only option values currently used by variants
- [x] `productVariantsBulkCreate`
- [x] `productVariantsBulkUpdate`
- [x] `productVariantsBulkDelete`
  - [x] staged variant merchandising/inventory slice (`sku`, `barcode`, `price`, `compareAtPrice`, `taxable`, `inventoryPolicy`, `inventoryQuantity`, `inventoryItem { tracked requiresShipping }`) with downstream `product.variants`, variant-backed search, and derived `totalInventory` / `tracksInventory` visibility
  - [x] concrete parity-plan proxy requests now mirror Shopify's current 2025-01 bulk-input shape (`inventoryItem.sku`, `optionValues`, and create-only `inventoryQuantities`)
  - [x] live safe-write conformance captures now cover the bulk family on the current dev-store token
  - [x] local bulk-update behavior now mirrors two captured Shopify quirks: `inventoryQuantities` is rejected on update (inventory changes belong in `inventoryAdjustQuantities`), and immediate sku-filtered `products` / `productsCount` reads can still lag behind `product.variants`
- [x] `productVariantCreate` (version-relevant compatibility path staged through the local variant overlay model)
- [x] `productVariantUpdate`
- [x] `productVariantDelete`
- [x] single-variant compatibility mutation family (`productVariantCreate` / `productVariantUpdate` / `productVariantDelete`) with downstream `product.variants`, `products`, and `productsCount` visibility
  - [x] concrete parity-plan proxy requests now exist for single-variant create/update/delete
  - [x] compatibility-only conformance closure now uses the covered bulk variant family plus the dedicated live schema blocker note as explicit evidence when the current 2025-01 store/API pair does not expose these legacy roots
- [x] product media create/update/delete family
  - [x] local image-media staging for `productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia` with downstream `product.media` visibility, `mediaUserErrors`, and delete id payloads
  - [x] live safe-write conformance captures now cover the full product media mutation family (`productCreateMedia`, `productUpdateMedia`, `productDeleteMedia`) against the dev store
  - [x] local media staging now mirrors the captured Shopify quirks: create payloads return `UPLOADED` image media with null image urls, the immediate downstream read can shift the asset into `PROCESSING` before it later becomes `READY`, pre-ready updates return `mediaUserErrors`, and `productDeleteMedia.deletedProductImageIds` stays empty for MediaImage deletes
- [ ] product reorder / sort operations if supported in Admin GraphQL
- [x] product tags side-effect mutations (`tagsAdd`, `tagsRemove` for product resources)
  - [x] downstream `product.tags` updates immediately after staged tag edits
  - [x] hydrated-product tag search/count parity mirrors live Shopify indexing lag: `products(query: "tag:...")` / `productsCount(query: "tag:...")` only reflect hydrated/base tags minus immediate removals, not freshly added-only tags
- [ ] product collections side-effect mutations as applicable beyond the dedicated collection membership family

### Product-specific fidelity work items
- [x] stable synthetic GID generation for products and variants (product-only so far)
- [x] stable synthetic timestamps (product-only so far)
- [-] handle generation / uniqueness behavior
  - [x] first live-backed uniqueness slice now mirrors Shopify across the staged product graph family: auto-generated create/duplicate/productSet handles de-duplicate on collision (for example `foo` -> `foo-1`, or trailing numeric handles increment in place), while explicit colliding handles on `productCreate`, `productUpdate`, and synchronous `productSet` return the captured `['input', 'handle']` userError instead of mutating the losing product
  - [x] explicit handle normalization now mirrors the current live create/update/productSet slice on this host: merchant-supplied handles are trimmed, lowercased, punctuation-collapsed, and hyphenated before storage (for example `"  Weird Handle / 100%  "` -> `weird-handle-100`), while punctuation-only explicit handles normalize into Shopify's fallback `product` handle family (`product`, `product-1`, ...) instead of the proxy-only `untitled-product`
  - [x] live product mutation capture now also preserves the title-only `productUpdate` slice where Shopify keeps the current explicit handle stable instead of regenerating it from the new title
  - [ ] broader handle parity beyond that first slice (for example reserved/invalid handles and exact normalization quirks outside the currently captured collision + explicit-normalization + title-only-update cases)
- [-] status transitions and publication visibility semantics (`productChangeStatus` now stages status-only transitions; aggregate publication count/current-publication visibility now stages locally, while richer publication graph/detail parity still pending)
  - [x] live status-mutation validation now also captures `productChangeStatus(productId: <unknown>) -> product: null + userErrors[{ field: ['productId'], message: 'Product does not exist' }]` and the null-literal `productId: null` GraphQL argument error, and local runtime mirrors those slices instead of staging phantom status rows
- [-] userErrors parity for invalid product inputs
  - [x] live-backed create/update/delete validation slice now covers `productCreate(title: "") -> Title can't be blank`, `productUpdate(title: "") -> unchanged product + Title can't be blank`, plus `productUpdate` / `productDelete` unknown-id userErrors (`field: ['id']`, `message: 'Product does not exist'`)
  - [x] live-backed missing-id validation now distinguishes two easy-to-guess-wrong branches: `productUpdate(product: { title: ... })` still returns mutation-scoped `['id'] / 'Product does not exist'`, while `productDelete` missing/null `input.id` fails at the GraphQL layer (`INVALID_VARIABLE`, `missingRequiredInputObjectAttribute`, or `argumentLiteralsIncompatible` depending on variable-vs-inline shape)
  - [x] snapshot/live-hybrid local parity now mirrors that captured product delete GraphQL-validation split instead of collapsing missing delete ids into local mutation `userErrors`
  - [x] live-backed status validation now also covers `productChangeStatus` unknown-id userErrors plus the separate null-literal GraphQL argument-validation path
  - [x] snapshot/live-hybrid local parity now mirrors the current captured `productChangeStatus` invalid-input slices without staging phantom status changes
  - [ ] broader invalid-input parity beyond the current slice (for example richer field validation on other product roots and exact missing-id semantics where local staging intentionally diverges before hydration)
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
  - [x] live safe-write conformance capture now covers `metafieldsSet` plus immediate downstream `product.metafield(...)` / `product.metafields` visibility on the dev store
- [x] `metafieldsDelete`
  - [x] live safe-write conformance capture now covers Shopify's current plural delete root plus immediate downstream `product.metafield(...)` / `product.metafields` removal parity
- [x] `metafieldDelete`
  - [x] singular compatibility alias is now closed against the live `metafieldsDelete` evidence instead of remaining a schema-drift gap

---

## Collections domain

### Queries
- [x] `collection`
- [x] `collections`
- [x] collection products connection
  - [x] top-level collection surface is currently derived from the known product↔collection membership graph (staged state and hydrated product reads), with cursor pagination on `collections` and the usual overlay product list semantics on `collection.products`
  - [x] live conformance captures now cover both `collection(id:)` detail reads and a top-level `collections` catalog slice with nested product connections

### Mutations
- [x] `collectionCreate`
- [x] `collectionUpdate`
- [x] `collectionDelete`
- [x] first standalone collection staging slice with downstream `collection`, `collections`, and nested `product.collections` visibility
- [x] collection membership mutations
  - [x] `collectionAddProducts` with atomic duplicate-member rejection and downstream `collection.products` / `product.collections` visibility
  - [x] `collectionRemoveProducts` with immediate local membership removal plus a synthetic async-shaped `job` payload (`done: false`) for the first async pass
  - [x] live safe-write conformance captures now cover the full collection mutation family (`collectionCreate`, `collectionUpdate`, `collectionDelete`, `collectionAddProducts`, `collectionRemoveProducts`) against the dev store

---

## Inventory / variants domain

### Queries
- [-] variant inventory read paths
  - [x] top-level `productVariant` read path for the common merchandising + inventory slice (`sku`, `barcode`, `price`, `compareAtPrice`, `taxable`, `inventoryPolicy`, `inventoryQuantity`, `selectedOptions`, `inventoryItem`) plus nested parent `product`
  - [x] nested `inventoryItem.inventoryLevels(first:)` slice on variant reads for `edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } }` plus selection-aware `pageInfo`
- [-] inventory item read paths
  - [x] top-level `inventoryItem` read path for `tracked`, `requiresShipping`, `measurement.weight`, `countryCodeOfOrigin`, `provinceCodeOfOrigin`, and `harmonizedSystemCode`, plus nested `variant { ... product { ... } }`
  - [x] top-level `inventoryItem.inventoryLevels(first:)` slice with location-aware level nodes and quantity-name filtering for `available`, `on_hand`, and `incoming`
- [x] top-level `locations` read path
  - [x] first location catalog slice now replays `locations(first:)` with `edges` / `pageInfo` plus the merchant-facing location fields (`id`, `name`) derived from the effective inventory-level graph
  - [x] live conformance capture now records `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/locations-catalog.json` for the current dev store location catalog
- [-] inventory levels read paths
  - [x] first inventory-level connection slice now replays from hydrated Shopify fixtures when available and otherwise synthesizes a product-scoped level set for staged reads, including `location { id name }`, `quantities(names: ...)`, and selection-aware `pageInfo`
  - [x] staged multi-location inventory-level replay now keeps per-location `available` / `on_hand` / non-available quantity rows scoped to the touched location instead of flattening every level back to the variant-wide total

### Mutations
- [x] `inventoryItemUpdate`
  - [x] staged inventory-item metadata updates for the first merchant-facing slice (`tracked`, `requiresShipping`, `countryCodeOfOrigin`, `provinceCodeOfOrigin`, `harmonizedSystemCode`, and `measurement.weight`) with immediate downstream `inventoryItem` / `productVariant.inventoryItem` visibility
  - [x] live safe-write conformance capture now covers `inventoryItemUpdate`, including the unknown-id `The product couldn't be updated because it does not exist.` validation path
- [-] inventory adjustment family relevant to products
  - [x] `inventoryAdjustQuantities` live parity captured and structurally covered for product-backed `available` quantity deltas by `inventoryItemId` / `locationId`
  - [x] local runtime now mirrors the captured Shopify lag pattern: mutation payload mirrors each `available` delta into an additional `on_hand` change with `quantityAfterChange: null`, while immediate `productVariant` / `inventoryItem.variant` quantities update before product-level `totalInventory` and `inventory_total:` catalog filtering catch up
  - [x] non-available quantity-name support now covers the current captured write-capable slice (`incoming`, `reserved`, `damaged`, `quality_control`, `safety_stock`) with Shopify-like `ledgerDocumentUri` validation per change while leaving `productVariant.inventoryQuantity` / product-level aggregates untouched
  - [x] invalid quantity names like `on_hand` now return the captured valid-values error plus the same per-change ledger-document requirement instead of a generic local-only message
  - [x] missing required nested change fields (`inventoryItemId`, `locationId`, `delta`) now mirror Shopify's top-level `INVALID_VARIABLE` GraphQL errors, while unknown inventory item ids return the captured fielded `userErrors` path/message instead of generic local errors
  - [x] unknown `locationId` inventory-adjust writes now return the captured `['input', 'changes', '<index>', 'locationId'] / 'The specified location could not be found.'` userError instead of silently synthesizing a new location row
  - [-] broader inventory adjustment parity beyond the current slice
    - [x] staged multi-location quantity replay now preserves location-scoped `available` / `on_hand` / non-available rows while `productVariant.inventoryQuantity` still reflects the aggregate available total
    - [x] richer first-pass `InventoryAdjustmentGroup` detail now includes the group `id` plus per-change `ledgerDocumentUri` echo parity instead of incorrectly collapsing every change back to the group-level `referenceDocumentUri`
    - [~] broader ledger semantics beyond that first richer payload slice
      - [x] live capture now preserves `inventoryAdjustmentGroup.app { id title apiKey handle }` plus `changes.location { id name }`, and the local proxy replays those selected fields on staged inventory adjustments when the corresponding conformance app metadata is configured locally
      - [ ] `inventoryAdjustmentGroup.staffMember` remains scope-gated on this host: Shopify returned a top-level `ACCESS_DENIED` error requiring `read_users` while still returning the rest of the mutation payload
- [x] variant inventory linkage mutations beyond the now-covered `inventoryItemUpdate` metadata root
  - [x] `inventoryActivate` now mirrors the current host-backed live slice: already-active primary-location calls stay no-op successes, unknown locations return the captured `['locationId'] / "The product couldn't be stocked because the location wasn't found."` validation path, and activating a known second location creates a new zero-quantity inventory level while ignoring the provided `available` seed
  - [x] `inventoryDeactivate` now mirrors both captured branches: single-location calls keep the minimum-one-location blocker (`field: null`, exact Shopify message), while deactivating one of multiple active levels succeeds and removes that level from downstream reads
  - [x] `inventoryBulkToggleActivation` now mirrors the broader live slice on this host: `activate: true` on an already-active location is a no-op success, `activate: true` on a known second location stages a new zero-quantity level, `activate: false` on the last remaining level returns the captured `CANNOT_DEACTIVATE_FROM_ONLY_LOCATION` userError, successful multi-location deactivation removes the target level and returns an empty `inventoryLevels` payload, and unknown locations return the captured `LOCATION_NOT_FOUND` validation error

---

## Media domain

### Queries
- [x] media-on-product read coverage
  - [x] product-scoped `media` connection reads are covered through the product-domain overlay serializer, nested pagination, and the live `product-detail-read` / media mutation parity fixtures

### Mutations
- [x] media create family
- [x] media update family
- [x] media delete family
  - [x] live safe-write conformance captures now cover `productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia`, including the observed upload lifecycle (`UPLOADED` → `PROCESSING` → `READY`) and the pre-ready `mediaUserErrors` path

---

## Customers domain

### Queries
- [x] `customer`
  - [x] first snapshot-mode null/empty increment for direct `customer(id:)` lookups without hitting upstream
  - [x] live-hybrid customer detail hydration/serialization now replays locally from normalized customer state after upstream reads
  - [x] live customer detail capture now records a real Shopify fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-detail.json`) for the merchant-facing read slice used by the proxy
  - [x] richer customer detail replay now preserves merchant-facing admin fields learned from live Shopify capture: `legacyResourceId`, `locale`, `note`, `canDelete`, `verifiedEmail`, `taxExempt`, `defaultPhoneNumber { phoneNumber }`, and `defaultAddress { address1 city province country zip formattedArea }`
- [x] `customers`
  - [x] first snapshot-mode empty connection increment for top-level `customers` with selection-aware `edges` / `pageInfo` handling
  - [x] live-hybrid customer catalog hydration/serialization now replays locally from normalized customer state after upstream reads
  - [x] live customer catalog capture now records a real Shopify fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-catalog.json`) for the current paginated customer list slice
  - [x] hydrated customer catalog replays now preserve Shopify's captured opaque cursors/pageInfo baseline and string-shaped `numberOfOrders` values instead of collapsing back to synthetic cursors or null counts
  - [x] richer customer card/search replay now preserves the live-captured admin slice for `legacyResourceId`, `verifiedEmail`, masked `defaultPhoneNumber`, and `defaultAddress` alongside the earlier email/state/count fields
  - [x] overlay customer list search/sort growth: local `customers` replays now support a useful first read slice for `query:` (`state`, `tag`, `email`, `first_name`, `last_name`, bare text), explicit deterministic `sortKey: UPDATED_AT|CREATED_AT|NAME|ID|LOCATION`, `reverse`, and backward pagination via `before`/`last`
  - [x] advanced customer search grammar now covers prefix terms, grouped `OR`, and grouped negation for overlay `customers(query:, sortKey:, reverse:)` replay without regressing cursor-stable filtered windows
  - [x] customer relevance-ranked replay (`sortKey: RELEVANCE`) now preserves hydrated Shopify search order, opaque cursors, and baseline pageInfo for exact captured relevance queries instead of inventing deterministic score semantics locally
  - [x] live customer search capture now records both a concrete filtered/sorted fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-search.json`) for `customers(query: "state:DISABLED", sortKey: UPDATED_AT, reverse: true)`, an advanced grammar fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-advanced-search.json`) covering `How*`, `(tag:VIP OR tag:referral) state:DISABLED`, a dedicated deterministic sort-key fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-sort-keys.json`) covering `NAME`, `ID`, and `LOCATION`, and a relevance-ranked fixture (`fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-relevance-search.json`) for `customers(query: "egnition", sortKey: RELEVANCE)`
- [x] `customersCount`
  - [x] top-level customer count reads now replay locally in snapshot/live-hybrid mode with alias support and exact count/precision payloads
  - [x] live customer count capture now records `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-count.json`
  - [x] current live parity trap on this host: `customersCount(query:)` did **not** reuse `customers` search-field semantics for the captured `email:` / `state:` slices; Shopify returned exact total counts plus `invalid_field` warnings, so the proxy intentionally keeps a conservative no-op count-query model instead of guessing richer count filtering
  - [x] local `customersCount` replay now also mirrors the captured `extensions.search[]` warning slice for those evidence-backed invalid `email:` / `state:` count queries instead of dropping Shopify's warning metadata entirely

### Mutations
- [x] `customerCreate`
- [x] `customerUpdate`
- [x] `customerDelete`
  - [x] snapshot-mode customer CRUD now stages locally with downstream `customer`, `customers`, and `customersCount` visibility instead of falling back to upstream passthrough
  - [x] local mutation payloads now preserve the current live merchant-facing customer slice (`displayName`, `locale`, `note`, `verifiedEmail`, `taxExempt`, `tags`, `state`, `canDelete`, `defaultEmailAddress`, and masked `defaultPhoneNumber`)
  - [x] live safe-write conformance captures now cover the customer CRUD family (`customerCreate`, `customerUpdate`, `customerDelete`) against the current dev-store token, including validation slices for missing create identity and unknown-id update/delete

---

## Orders domain

### Queries
- [x] `order`
- [x] `orders`
- [x] `ordersCount`
  - [x] snapshot-mode overlay still preserves the conservative local empty-state baseline when no staged orders exist (`order(id: "gid://shopify/Order/0") -> null`, `orders ->` empty connection, `ordersCount -> { count: 0, precision: EXACT }`)
  - [x] the first staged direct-order happy path now also replays locally in both snapshot mode and live-hybrid mode after `orderCreate`: immediate `order(id:)`, `orders(first: ...)`, and `ordersCount` reads can see the newly staged synthetic order without hitting upstream
  - [-] broader live Shopify catalog/count parity remains separate from that first staged read-after-write slice because the real store can already contain other merchant orders, so `orders` / `ordersCount` parity after live `orderCreate` is not the same problem as local single-order replay
- [x] `draftOrder`
  - [x] the first live-backed draft-order detail slice is now captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json`: reading `draftOrder(id:)` immediately after a safe `draftOrderCreate` returns the same merchant-facing open-draft detail payload on this host
  - [x] snapshot-mode local staging now replays that same detail slice after `draftOrderCreate` without hitting upstream, so the proxy has its first evidence-backed draft-order read-after-write path
  - [x] live-hybrid now preserves that same supported `draftOrderCreate` -> immediate `draftOrder(id:)` read-after-write path locally for staged synthetic draft ids instead of proxying the supported create/detail roots upstream
- [x] `draftOrders`
- [x] `draftOrdersCount`
  - [x] the narrow local staged-synthetic replay slice now honors connection windows instead of only `first`: unfiltered `draftOrders` in snapshot/live-hybrid mode preserves forward `after` and backward `before`/`last` pagination with cursor-stable `pageInfo` whenever staged drafts exist locally
  - [x] the first non-empty live Shopify catalog/count baseline is now captured on the healthy repo credential: `corepack pnpm conformance:capture-orders` refreshes `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json` and `draft-orders-count.json` with newest-first catalog cursors plus `{ count, precision }`
  - [x] the first evidence-backed filtered-query warning slice is now captured and replayed locally: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-invalid-email-query.json` preserves Shopify's current `query: "email:..."` behavior where `draftOrders` / `draftOrdersCount` keep the same catalog/count window and return `extensions.search[].warnings[{ code: 'invalid_field' }]` instead of a narrowed result set
  - [-] practical consequence: keep that captured invalid-field branch narrow and evidence-backed rather than treating it as proof that broader draft-order `query:` semantics are now supported

### Mutations
- [x] `orderUpdate`
  - [x] first safe unknown-id validation slice is now evidence-backed and mirrored locally in snapshot mode: `orderUpdate(input: { id: "gid://shopify/Order/0", ... })` returns `order: null` plus `userErrors[{ field: ['id'], message: 'Order does not exist' }]` without hitting upstream
  - [x] adjacent missing-id variable payload is now also evidence-backed and mirrored locally in snapshot mode: omitting `input.id` from the variables payload fails earlier with top-level GraphQL `INVALID_VARIABLE` (`Expected value to not be null`) instead of reaching mutation-scoped `userErrors`
  - [x] inline-literal missing/null id validation is now separately evidence-backed and mirrored locally in snapshot mode: `orderUpdate(input: { ... })` without `id` fails with `missingRequiredInputObjectAttribute`, while `orderUpdate(input: { id: null, ... })` fails with `argumentLiteralsIncompatible` instead of reusing the variables-path `INVALID_VARIABLE` shape
  - [x] snapshot mode and live-hybrid now also support a first happy-path local `orderUpdate` slice for already-known orders, not just freshly synthetic ones: updating `note` and `tags` mutates the effective order in place, returns `userErrors: []`, bumps `updatedAt`, and preserves the edited values through immediate downstream `order(id:)` / `orders(first: ...)` reads without hitting upstream
  - [x] first live happy-path `orderUpdate` slice is now captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-parity.json`: updating `note` and `tags` on a freshly created order returned `userErrors: []`, preserved the same order id/name, and the immediate downstream `order(id:)` read kept the edited values visible on this host
  - [x] live-hybrid still short-circuits the captured unknown-id `orderUpdate` branch locally instead of proxying that obviously invalid supported edit upstream; broader live Shopify `orderUpdate` parity for non-local orders remains separate from this first synthetic/local edit slice
- [x] `orderCreate`
  - [x] safe direct-order GraphQL validation coverage now includes the inline argument-literal branches too: omitting the inline `order` argument is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-inline-missing-order.json` with top-level `missingRequiredArguments` (`Field 'orderCreate' is missing required arguments: order`), and `orderCreate(order: null)` is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-inline-null-order.json` with `argumentLiteralsIncompatible`
  - [x] first safe direct-order validation slice is now captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-missing-order.json`: omitting the required `$order` variable fails at GraphQL coercion time with top-level `INVALID_VARIABLE`, and local parity mirrors that branch without hitting upstream in both snapshot mode and live-hybrid mode
  - [x] the first merchant-realistic direct-order happy path is now live-captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json`: Shopify accepted custom line items, one shipping line, billing + shipping addresses, order `customAttributes`, and a successful manual transaction, returning a paid/unfulfilled order payload plus immediate `order(id:)` read-after-write visibility
  - [x] local `orderCreate` now stages that same merchant-facing slice locally in both snapshot mode and live-hybrid mode, so supported direct-order creates no longer proxy upstream during normal runtime and immediate `order(id:)` / `orders(first: ...)` / `ordersCount` replay stays available for the staged synthetic order
  - [-] broader live Shopify `orders` / `ordersCount` parity after direct-order create remains separate because the real store catalog can already contain other merchant orders; the current direct-order happy-path capture is intentionally anchored on the mutation payload plus immediate `order(id:)` read-after-write slice instead of claiming full catalog/count parity

  - [x] safe draft-order GraphQL validation coverage now includes the inline argument-literal branches too: omitting the inline `input` argument is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-inline-missing-input.json` with top-level `missingRequiredArguments` (`Field 'draftOrderCreate' is missing required arguments: input`), and `draftOrderCreate(input: null)` is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-inline-null-input.json` with top-level `argumentLiteralsIncompatible` (`Argument 'input' on Field 'draftOrderCreate' has an invalid value (null). Expected type 'DraftOrderInput!'.`); snapshot-mode local parity now mirrors both branches without hitting upstream
  - [x] first safe draft-order validation slice is now captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-missing-input.json`: omitting the required `$input` variable fails at GraphQL coercion time with top-level `INVALID_VARIABLE`, and snapshot-mode local parity now mirrors that branch without hitting upstream
  - [x] the first merchant-realistic happy-path draft-order create slice is now captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json`, and snapshot-mode local staging replays that same open-draft payload without hitting upstream
  - [x] the current repo credential can now also recapture that same happy-path slice directly, so `corepack pnpm conformance:capture-orders` refreshes `draft-order-create-parity.json` and `draft-order-detail.json` from a healthy live credential instead of treating `draftOrderCreate` as the remaining creation blocker
  - [x] immediate `draftOrder(id:)` read-after-write visibility from that same create flow is now evidence-backed via `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json`
  - [x] the checked-in draft-order parity request now mirrors the same merchant-facing address/shipping metadata slice in Shopify's draft-order shape (`billingAddress`, `shippingAddress`, singular `shippingLine`, and `customAttributes`)
  - [x] a live quirk from that first happy-path slice is now explicit in docs/runtime expectations: `DraftOrderInput.note` is still sent, but the immediate `draftOrder` payload on this host must **not** select a top-level `note` field, and sending `input.shippingLine` did **not** guarantee a non-null immediate `draftOrder.shippingLine`
- [x] `draftOrderComplete`
  - [x] safe draft-order completion GraphQL validation coverage now includes all three first evidence-backed pre-access branches: omitting the inline `id` argument is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-inline-missing-id.json` with top-level `missingRequiredArguments`, inline `id: null` is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-inline-null-id.json` with top-level `argumentLiteralsIncompatible`, and omitting the required `$id` variable remains captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-missing-id.json` with top-level `INVALID_VARIABLE`
  - [x] local parity now mirrors those same validation branches without hitting upstream, and the obviously invalid missing-`id` completion requests stay short-circuited locally before the separate live access blocker is even relevant
  - [x] snapshot mode and live-hybrid now both support a first narrow synthetic/local `draftOrderComplete` runtime slice: completing a locally staged draft flips the staged draft payload to `status: COMPLETED`, `ready: true`, preserves the current `invoiceUrl`, and replays the same completed draft detail locally without hitting upstream
  - [-] first draft-to-order completion happy-path parity scaffold still exists separately (`config/parity-specs/draftOrderComplete-parity-plan.json` + `config/parity-requests/draftOrderComplete-parity-plan.{graphql,variables.json}`), but the current host credential is blocked on `write_draft_orders` plus Shopify's extra mark-as-paid / set-payment-terms permission gate
  - [x] a new live schema correction is baked into that scaffold: Shopify's `DraftOrderCompletePayload` on this host does **not** expose a top-level `order` field, and probing `draftOrderComplete { order { ... } }` fails with `Field 'order' doesn't exist on type 'DraftOrderCompletePayload'`
  - [-] the checked-in completion request now keeps the first merchant-facing completion payload narrow and schema-valid: `draftOrder { id name status ready invoiceUrl totalPriceSet lineItems(first: 5) { nodes { ... } } }` plus `userErrors`, so later live capture does not start from a broken draft-to-order bridge query or overclaim the downstream order bridge before Shopify happy-path evidence exists
- [x] `orderEditBegin`
  - [x] covered safely by the captured missing-`$id` GraphQL validation branch in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-begin-missing-id.json`: `mutation OrderEditBeginMissingId($id: ID!)` fails with top-level `INVALID_VARIABLE` before Shopify reaches the root's separate `write_order_edits` blocker
  - [x] snapshot mode and live-hybrid now mirror that captured missing-`$id` branch locally without hitting upstream, and the same local runtime still opens a calculated-order session for synthetic/local orders when a staged order id is present
  - [-] first live happy-path parity still remains blocked separately in `config/parity-specs/orderEditBegin-parity-plan.json`: the current host credential still hits `write_order_edits` before Shopify reveals broader non-local/session semantics
- [x] `orderEditAddVariant`
  - [x] covered safely by the captured missing-`$id` GraphQL validation branch in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-add-variant-missing-id.json`: `mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!)` fails with top-level `INVALID_VARIABLE` before Shopify reaches the root's separate `write_order_edits` blocker
  - [x] snapshot mode and live-hybrid now mirror that captured missing-`$id` branch locally without hitting upstream, and the same local runtime still stages variant-derived calculated line items for synthetic/local calculated orders when a staged calculated-order id is present
  - [-] first live happy-path parity still remains blocked separately in `config/parity-specs/orderEditAddVariant-parity-plan.json`: the current host credential still hits `write_order_edits` before Shopify reveals broader non-local/session semantics
- [x] `orderEditSetQuantity`
  - [x] covered safely by the captured missing-`$id` GraphQL validation branch in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-set-quantity-missing-id.json`: `mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!)` fails with top-level `INVALID_VARIABLE` before Shopify reaches the root's separate `write_order_edits` blocker
  - [x] snapshot mode and live-hybrid now mirror that captured missing-`$id` branch locally without hitting upstream, and the same local runtime still stages calculated line-item quantity edits for synthetic/local calculated orders when a staged calculated-order id is present
  - [-] first live happy-path parity still remains blocked separately in `config/parity-specs/orderEditSetQuantity-parity-plan.json`: the current host credential still hits `write_order_edits` before Shopify reveals broader non-local/session semantics
- [x] `orderEditCommit`
  - [x] covered safely by the captured missing-`$id` GraphQL validation branch in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-commit-missing-id.json`: `mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String)` fails with top-level `INVALID_VARIABLE` before Shopify reaches the root's separate `write_order_edits` blocker
  - [x] snapshot mode and live-hybrid now mirror that captured missing-`$id` branch locally without hitting upstream, and the same local runtime still commits a first synthetic/local calculated-order session back onto the staged order when a staged calculated-order id is present
  - [-] first live happy-path parity still remains blocked separately in `config/parity-specs/orderEditCommit-parity-plan.json`: the current host credential still hits `write_order_edits` before Shopify reveals broader non-local/session semantics
- [x] first calculated-order mutation family local runtime slice (`orderEditSetQuantity`, `orderEditCommit`)
  - [x] snapshot mode and live-hybrid now stage a first local calculated-order edit flow for synthetic/local orders: `orderEditSetQuantity` updates a staged calculated line item quantity, and `orderEditCommit` applies the edited line set plus optional `staffNote` back onto the staged synthetic order without hitting upstream
  - [-] current live blocker on this host is still consistent across the remaining non-local/live parity roots even after the safe validation slices landed: the happy-path Shopify probes for `orderEditSetQuantity` and `orderEditCommit` remain gated on `write_order_edits` before Shopify reveals broader unknown-id or session-shape semantics
- [x] first fulfillment-domain validation slice is evidence-backed (`fulfillmentCreate` invalid fulfillment-order id)
  - [x] snapshot-mode and live-hybrid local parity now mirror the captured top-level GraphQL `RESOURCE_NOT_FOUND` / `invalid id` branch for `fulfillmentCreate(fulfillment: { lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: "gid://shopify/FulfillmentOrder/0" }] })` without hitting upstream
  - [x] live conformance capture now preserves that branch in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-create-invalid-id.json` so the first fulfillment increment stays evidence-backed instead of guessed
- [x] `fulfillmentTrackingInfoUpdate`
- [x] `fulfillmentCancel`
  - [x] safe pre-access GraphQL validation coverage is now evidence-backed and mirrored locally in snapshot/live-hybrid mode for both roots: inline missing-id, inline null-id, and missing-required-variable requests short-circuit locally without hitting upstream, matching the new captured fixtures under `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/`
  - [-] the broader live happy paths remain blocked and tracked explicitly in the registry/scenario/parity scaffolds rather than only as prose: `fulfillmentTrackingInfoUpdate` is still gated on one of Shopify's fulfillment-write scopes plus the `fulfill_and_ship_orders` permission, while `fulfillmentCancel` still returns a generic `ACCESS_DENIED` payload on this host once a real id is present
  - [-] `corepack pnpm conformance:capture-orders` now refreshes both the new validation fixtures and `pending/fulfillment-lifecycle-conformance-scope-blocker.md` alongside the other order-domain blocker notes instead of leaving the broader fulfillment lifecycle as an unstructured TODO

Current orders-domain blocker notes:
- the current probe is auth-regressed, but `corepack pnpm conformance:capture-orders` still refreshes `pending/order-creation-conformance-scope-blocker.md` without overwriting the checked-in `orderCreate` / `draftOrderCreate` happy-path fixtures or the last verified `draftOrderComplete` access-denied evidence with `401` payloads
- the same command also refreshes `pending/order-editing-conformance-scope-blocker.md` from the current auth-regressed context while preserving the last verified `write_order_edits` blocker details for `orderEditBegin`, `orderEditAddVariant`, `orderEditSetQuantity`, and `orderEditCommit`
- `pending/draft-order-read-conformance-scope-blocker.md` is recreated on the current auth-regressed run while the checked-in `draft-orders-catalog.json` / `draft-orders-count.json` fixtures remain the last verified live baseline
- the same command also refreshes `pending/fulfillment-lifecycle-conformance-scope-blocker.md` from the current auth-regressed context while preserving the last verified live `ACCESS_DENIED` split for `fulfillmentTrackingInfoUpdate` vs `fulfillmentCancel`
- the remaining creation blocker after auth repair is still `draftOrderComplete`, not `orderCreate`; direct-order creation is already captured and staged locally, while completion is still gated on `write_draft_orders` plus Shopify's mark-as-paid / set-payment-terms permission gate

---

## Generic platform work items

### Proxy/runtime
- [x] versioned Shopify path handling
- [x] upstream auth header pass-through
- [x] query vs mutation classifier
- [x] unsupported-mutation passthrough marker
- [-] capability registry for overlay-read vs stage-locally vs passthrough routing
- [x] snapshot mode
  - [x] startup `snapshotPath` loading now accepts normalized state snapshot files, seeds base products/customers plus customer catalog/search baselines, and `POST /__meta/reset` restores that startup snapshot instead of dropping back to an empty store
- [x] live-hybrid mode
- [x] pure passthrough mode

### Meta API
- [x] `POST /__meta/reset`
- [x] `POST /__meta/commit`
- [x] `GET /__meta/log`
- [x] `GET /__meta/state`
- [x] `GET /__meta/config`
- [x] `GET /__meta/health`
  - [x] meta config now exposes the active upstream origin, read mode, and snapshot path for the current process
  - [x] meta commit now replays pending logged mutations against upstream Shopify in original order, persists per-entry `committed` / `failed` statuses, and stops on the first upstream failure while returning the ordered attempt list

### State engine
- [x] normalized object graph
- [x] staged overlay engine
- [x] raw mutation log retention
- [x] original-order commit replay
- [x] stop-on-first-error commit semantics

### Conformance
- [ ] scenario fixture format
- [ ] real Shopify recorder
- [ ] normalized snapshot compiler
- [ ] proxy parity runner
- [x] operation coverage matrix
  - [x] `corepack pnpm conformance:check` now generates `docs/generated/operation-coverage-matrix.{json,md}` with operation-level scenario states, blocker summaries, and assertion-kind coverage
- [ ] product scenario pack
- [ ] mutation userErrors parity harness
- [ ] empty/null behavior parity harness

Current live-conformance status note:
- the current orders-domain conformance probe on this host is auth-regressed
- `corepack pnpm conformance:probe` currently fails with `401` / `Invalid API key or access token` against the repo credential for `very-big-test-store.myshopify.com`
- the checked-in fixtures are the last verified live references and `corepack pnpm conformance:capture-orders` refreshes blocker notes without overwriting those safe fixtures with `401` payloads
- `pending/draft-order-read-conformance-scope-blocker.md` is recreated on the current auth-regressed run while the checked-in `draft-orders-catalog.json` / `draft-orders-count.json` fixtures remain the last verified live baseline
- the auth regression does not invalidate the checked-in `draftOrders` / `draftOrdersCount` baseline, and a failed repo-local refresh now has a concrete meaning on this host: `corepack pnpm conformance:refresh-auth` can return `invalid_request` / `This request requires an active refresh_token` once the saved grant is no longer refreshable
- the remaining creation blocker after auth repair is still `draftOrderComplete`, while `pending/order-editing-conformance-scope-blocker.md` and `pending/fulfillment-lifecycle-conformance-scope-blocker.md` preserve the current blocker details for the edit and fulfillment families
- if `corepack pnpm conformance:refresh-auth` now fails with `invalid_request` / `This request requires an active refresh_token`, stop retrying the dead saved grant and generate a fresh manual store-auth link before rerunning `corepack pnpm conformance:probe` plus `corepack pnpm conformance:capture-orders`

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

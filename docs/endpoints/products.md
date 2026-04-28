# Products Endpoint Group

The products group is product-first and deep, with registry entries for supported local behavior and explicit unsupported gaps. It covers product roots plus directly related inventory, metafield, collection, publication, tag, helper, feedback, and product-media roots that are modeled as product-owned behavior.

## Current support and limitations

### Supported roots

Overlay reads:

- `product`
- `productByIdentifier`
- `products`
- `productsCount`
- `productFeed`
- `productFeeds`
- `sellingPlanGroup`
- `sellingPlanGroups`
- `productVariant`
- `productVariantByIdentifier`
- `productVariants`
- `productVariantsCount`
- `productTags`
- `productTypes`
- `productVendors`
- `productSavedSearches`
- `productOperation`
- `productDuplicateJob`
- `productResourceFeedback`
- `inventoryItem`
- `inventoryItems`
- `inventoryShipment`
- `inventoryLevel`
- `inventoryProperties`
- `inventoryTransfer`
- `inventoryTransfers`
- `collection`
- `collectionByIdentifier`
- `collectionByHandle`
- `collections`
- `locations`
- `channel`
- `channels`
- `publication`
- `publications`
- `publicationsCount`
- `publishedProductsCount`

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
- `productOptionsReorder`
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
- `productVariantAppendMedia`
- `productVariantDetachMedia`
- `inventoryItemUpdate`
- `inventoryAdjustQuantities`
- `inventorySetQuantities`
- `inventoryMoveQuantities`
- `inventoryActivate`
- `inventoryDeactivate`
- `inventoryBulkToggleActivation`
- `inventoryTransferCreate`
- `inventoryTransferCreateAsReadyToShip`
- `inventoryTransferEdit`
- `inventoryTransferSetItems`
- `inventoryTransferRemoveItems`
- `inventoryTransferMarkAsReadyToShip`
- `inventoryTransferDuplicate`
- `inventoryTransferCancel`
- `inventoryTransferDelete`
- `inventoryShipmentCreate`
- `inventoryShipmentCreateInTransit`
- `inventoryShipmentAddItems`
- `inventoryShipmentRemoveItems`
- `inventoryShipmentUpdateItemQuantities`
- `inventoryShipmentSetTracking`
- `inventoryShipmentMarkInTransit`
- `inventoryShipmentReceive`
- `inventoryShipmentDelete`
- `metafieldsSet`
- `metafieldsDelete`
- `metafieldDelete`
- `collectionCreate`
- `collectionUpdate`
- `collectionDelete`
- `collectionAddProducts`
- `collectionAddProductsV2`
- `collectionRemoveProducts`
- `collectionReorderProducts`
- `sellingPlanGroupCreate`
- `sellingPlanGroupUpdate`
- `sellingPlanGroupDelete`
- `sellingPlanGroupAddProducts`
- `sellingPlanGroupRemoveProducts`
- `sellingPlanGroupAddProductVariants`
- `sellingPlanGroupRemoveProductVariants`
- `productJoinSellingPlanGroups`
- `productLeaveSellingPlanGroups`
- `productVariantJoinSellingPlanGroups`
- `productVariantLeaveSellingPlanGroups`
- `publicationCreate`
- `publicationUpdate`
- `publicationDelete`

### Registered product helper and merchandising gaps

These product-adjacent roots are registered in the operation registry as product-domain gaps, but are not local mutation support yet. They still proxy as unsupported mutations at runtime and must not be treated as supported until success-path staging and downstream read-after-write behavior are modeled:

- `bulkProductResourceFeedbackCreate`
- `shopResourceFeedbackCreate`
- `productFeedCreate`
- `productFeedDelete`
- `productFullSync`
- `productBundleCreate`
- `productBundleUpdate`
- `productVariantRelationshipBulkUpdate`
- `combinedListingUpdate`

### Behavior notes

- Product feed reads currently support Shopify-like no-data behavior in snapshot mode. Captured 2025-01 `harry-test-heelo` evidence returns `productFeed(id:)` as `null` for an absent feed id and `productFeeds(first:)` as an empty connection with empty `nodes`/`edges`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors. Live-hybrid `productFeed` / `productFeeds` requests continue to proxy upstream because staged product mutations do not currently model feed-channel membership.
- Product feed mutations remain unsupported. The 2025-01 `harry-test-heelo` probe for `productFeedCreate(country: US, language: EN)` returned a top-level `NOT_FOUND` error, `Unable to find channel for product feed`; `productFeedDelete` and `productFullSync` unknown feed ids returned payload userErrors with `field: ["id"]` and `ProductFeed does not exist`. Local staging needs channel-backed success evidence and downstream feed read effects before these roots can become supported.
- Selling-plan group lifecycle and membership details are documented in `docs/endpoints/selling-plans.md`.
- `Product.sellingPlanGroups`, `Product.sellingPlanGroupsCount`, `ProductVariant.sellingPlanGroups`, and `ProductVariant.sellingPlanGroupsCount` resolve from the staged selling-plan group membership model. Product and variant memberships are tracked separately, matching captured 2026-04 behavior where a group created with `resources.productIds` applies to the product but not automatically to the product variant.
- Product bundle, product variant relationship, and combined-listing mutations remain unsupported. Captured guardrails cover `productBundleCreate` with empty components (`productBundleOperation: null`, user error `At least one component is required.`), `productBundleUpdate` with an unknown product (`productBundleOperation: null`, user error `Product does not exist`), and `combinedListingUpdate` with an unknown parent product (`product: null`, code `PARENT_PRODUCT_NOT_FOUND`). `productVariantRelationshipBulkUpdate` is registered as a HAR-299 gap, but it is not implemented because full support requires normalized `ProductVariantComponent` state, bundle-specific validation, and downstream `productVariantComponents` reads. Local staging needs component-backed bundle success evidence, `ProductBundleOperation` lifecycle/status behavior, combined-listing child relationship evidence, and downstream product reads before these roots can become supported.
- Product-domain metafields are normalized as owner-scoped records for `Product`, `ProductVariant`, and `Collection` owners. Besides `id`, `namespace`, `key`, `type`, and `value`, hydrated and staged records carry `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`, and `ownerType` for owner-scoped parity. Metafield `definition` still serializes as `null` for product metafield node selections until a product-owned fixture returns nested definition data; definition records themselves are modeled through the metafields endpoint group.
- Local `metafieldsSet` support covers product, product variant, and collection owners only. It validates the full input batch before replacing each affected owner metafield set, supports compare-and-set through `compareDigest`, treats `compareDigest: null` as a create-only guard, and preserves Shopify-like atomic no-write behavior when any modeled resolver error is returned. For matching staged/effective metafield definitions, it infers omitted input `type`, rejects explicit type mismatches, and applies the fixture-backed `max` and `regex` validations represented in the local definition model. Customer, order, draft-order, shop, discount, and other owner families remain scoped to their own endpoint groups or future issues.
- `metafieldsDelete` uses the same product-domain owner scope and returns ordered `deletedMetafields` entries, including `null` for missing namespace/key rows. Downstream `product(id:)`, `productVariant(id:)`, and `collection(id:)` reads expose staged owner-specific singular `metafield(namespace:, key:)` and `metafields` connection results without live writes.
- Product search uses the shared Shopify-style search parser. Endpoint-specific product behavior includes boolean grouping, quoted values, field comparators, simple term-list searches, variant search terms, sort keys, and captured connection cursor/pageInfo baselines.
- Live schema introspection confirms product reorder roots for variant order (`productVariantsBulkReorder`), media order (`productReorderMedia`), option/value order (`productOptionsReorder`), and collection product order (`collectionReorderProducts`). Local support covers all four reorder roots with ordered normalized variants, media, options, option values, and collection membership rows.
- `productVariantsBulkReorder` stages an ordered effective variant list from `ProductVariantPositionInput.position` and exposes that ordering through downstream `product.variants(...)` and `productVariant(id:)` reads without runtime Shopify writes.
- `productOptionsReorder` stages the submitted option order and optional option-value order. It remaps variant `selectedOptions`, derives variant titles from the reordered selected-option tuple, and sorts downstream `product.variants(...)` by the reordered option/value sequence.
- `productReorderMedia` stages `MoveInput` media moves, returns an async-style `Job`, and exposes reordered `product.media(...)` and `product.images(...)` connections without runtime Shopify writes.
- `productVariantAppendMedia` and `productVariantDetachMedia` stage variant-specific media ID associations without duplicating or deleting product-level media. Downstream `productVariant.media(...)` resolves from the variant's ordered media IDs against the effective product media set.
- `productByIdentifier` supports `identifier.id` and `identifier.handle` against effective local product state. `identifier.customId` resolves product metafields backed by an effective PRODUCT metafield definition whose type is `id`; without that definition, snapshot/local reads return Shopify's captured top-level `NOT_FOUND` error, `Metafield definition of type 'id' is required when using custom ids.`
- `productVariantByIdentifier` supports `identifier.id` against effective local variant state. `identifier.customId` returns `null` until unique variant metafield identifier evidence and indexing are modeled.
- Top-level `productVariants` and `productVariantsCount` resolve from effective local product variants. Supported local query terms are `id`, `product_id`, `title`, `sku`, `barcode`, `vendor`, `product_type`, and `tag`, with the shared Shopify-style boolean parser. Local sorting supports the common `ID`, `TITLE`, `SKU`, `POSITION`, and `INVENTORY_QUANTITY` paths, with connection serialization delegated to `src/proxy/graphql-helpers.ts`.
- `productTags`, `productTypes`, and `productVendors` serialize distinct sorted `StringConnection` values from effective local products. Empty snapshot state returns empty `nodes`/`edges` with false `pageInfo` booleans and null cursors.
- `productSavedSearches` is handled by the saved-search domain model. It returns staged product saved searches with downstream read-after-write behavior and otherwise preserves the captured empty connection shape for product saved searches.
- `productSet(input:, synchronous: false)` now records a local `ProductSetOperation` and `productOperation(id:)` can read it back with `status`, `product`, and `userErrors`. Unknown operation IDs return `null`, matching the captured helper-root no-data behavior.
- `productDuplicateJob(id:)` models the captured unknown-job read shape as `{ id, done: true }`. Local `productDuplicate` remains synchronous and does not create a long-running duplicate job.
- `productResourceFeedback(id:)` returns `null` for absent local feedback. The HAR-297 live capture recorded Shopify returning `data.productResourceFeedback: null` plus an `ACCESS_DENIED` error without `read_resource_feedbacks` and sales-channel configuration; local mutation-created feedback is not claimed.
- `bulkProductResourceFeedbackCreate` and `shopResourceFeedbackCreate` are intentionally registry-only unsupported gaps. They may proxy as the unknown/unsupported escape hatch and are logged as proxied, but they are not supported local staging paths until a feedback state model can preserve raw mutation order for commit and expose downstream `productResourceFeedback`/shop feedback reads without runtime Shopify writes.
- Collection records carry aggregate publication target ids alongside product publication ids. A staged `collectionCreate` starts unpublished; collection publication counts and `publishedOnPublication(publicationId:)` remain unpublished until a local publish mutation adds a target.
- `publishedOnCurrentPublication` is not inferred from aggregate collection publication count. Captured Online Store publishable writes leave it false when the app current publication is not the target.
- Local `publishablePublish` and `publishableUnpublish` currently stage Product and Collection publishables. Product-scoped generic publishable payloads also serialize the selected `shop.publicationCount` from the effective local publication catalog, so explicit publication targets can add derived catalog rows when no publication fixture was seeded. The internal current-channel placeholder used by `publishablePublishToCurrentChannel` updates product aggregate/current-publication state but does not become a shop-level publication row; captured parity shows Shopify keeps `shop.publicationCount` tied to the shop publication catalog. Broader publishable implementers remain unsupported in their own groups.
- Shopify's current docs mark `productPublish` as deprecated in favor of `publishablePublish`; the local product-specific roots remain supported because captured parity fixtures still exercise the deprecated product payload shape used by existing apps. `PublicationInput.publishDate` is accepted as a target field but only online-store channels support future publishing in Shopify, and the field has no effect for unpublish mutations. Subscription-only products (`requiresSellingPlan: true`) are publication-restricted to online-store targets in Shopify; the local aggregate model should not generalize non-online-store success without additional live evidence.
- Top-level `publication(id:)`, `publications(...)`, `publicationsCount(...)`, `publishedProductsCount(publicationId:)`, and deprecated `channel` / `channels` roots resolve from the normalized publication catalog in snapshot/local overlay paths. Empty snapshot state returns `publication: null`, `channel: null`, empty channel/publication connections, and exact zero counts without upstream access. The existing live publication catalog fixture captures non-empty `publications` id/name/cursor shape; HAR-319 adds runtime-test-backed local evidence for the remaining root family and lifecycle flow.
- `publicationCreate`, `publicationUpdate`, and `publicationDelete` stage normalized Publication rows locally and never perform runtime Shopify writes. This support is intentionally catalog-level: product and collection publication membership still flows through the existing product-specific or generic publishable roots. Deleting a publication strips that target from locally modeled Product and Collection publication IDs so downstream `publishedOnPublication`, publication counts, and `published_status` filters stop seeing the removed target.
- Product handle generation and validation follows the captured product mutation slice: duplicate title-generated handles are de-duplicated, explicit handles are normalized before uniqueness checks, Unicode letters/numbers are preserved, punctuation-only explicit handles fall back into the `product` handle family, explicit collisions return `['input', 'handle']` userErrors, and explicit handles longer than 255 characters return `['handle']` userErrors without staging partial state. The HAR-22 live probe found no product reserved-word rejection for handles such as `admin`, `products`, `collections`, `cart`, `checkout`, or `new`.
- Product option lifecycle staging is fixture-backed for `productOptionsCreate`, `productOptionUpdate`, and `productOptionsDelete`. The current conformance fixtures cover replacing Shopify's default `Title` option with created options, keeping non-variant option values in `optionValues` but out of `values`, renaming and repositioning options, adding/updating/deleting option values, reordering variant `selectedOptions` after option repositioning, and restoring Shopify's default option/variant graph when all custom options are deleted. Local `productOptionsCreate(variantStrategy: CREATE)` also stages variants for the effective option-value Cartesian product, which keeps `values`, `optionValues.hasVariants`, and downstream `product.variants(...)` reads aligned with Shopify's documented CREATE strategy behavior. Captured 2025-01 `harry-test-heelo` evidence for a second CREATE option shows Shopify keeps the existing variants first after filling the new option's first value, then appends the remaining fanout combinations. That shop accepted 110 variants with no `productOptionsCreate.userErrors` (`field`, `message`, and `code` were all absent because the array was empty), which is above Shopify's historical default 100-variant limit and indicates Extended Variants-relevant behavior on this test shop rather than a default-limit rejection. Expected parity differences are limited to generated `ProductOption`, `ProductOptionValue`, and locally generated variant/inventory-item GIDs.
- Explicit `productOptionsCreate(variantStrategy: LEAVE_AS_IS)` and an explicit null `variantStrategy` both matched Shopify's default branch in the 2025-01 capture: the standalone existing variant was updated to the first created option value, additional option values remained in `optionValues` with `hasVariants: false`, `values` included only the variant-backed value, and `userErrors` was empty.
- Captured option lifecycle validation branches include `productOptionsCreate` with an unknown product (`field: ["productId"]`, `Product does not exist`), `productOptionUpdate` with an unknown option (`field: ["option"]`, `Option does not exist`), and `productOptionsDelete` with an unknown option id (`field: ["options", "0"]`, `Option does not exist`). These branches stage no upstream Shopify writes.
- Top-level `products(query: "published_status:...")` and `productsCount(query: "published_status:...")` apply locally modeled aggregate product publication visibility. A product is treated as published for this filter only when it is `ACTIVE` and has at least one staged or hydrated publication target; `DRAFT` and `ARCHIVED` products remain unpublished even when publication targets are staged. Richer publication graph/detail parity remains limited to the aggregate product fields listed in the validation fixtures.
- Top-level `collections(query: "published_status:...")` applies locally modeled aggregate collection publication state for staged and snapshot reads.
- Product-side collection membership effects are modeled as normalized product collection rows. `collectionCreate(input.products)` stages initial manual collection memberships without upstream writes. A 2025-01 live capture confirms the create payload immediately returns the selected `products` nodes and `hasProduct: true`, but its selected `productsCount` aggregate remains `0` until a downstream collection read returns the actual count. `collectionAddProducts` appends memberships in submitted product order; `collectionAddProductsV2` returns an async-style `Job` and preserves submitted order for explicitly `MANUAL` collections, while retaining the captured non-manual/default insertion behavior from the 2026-04 product-relationship fixture. Manual `collectionReorderProducts` applies moves sequentially, including insertion into the middle of the sort order. `productSet(input.collections)` replaces the product's effective memberships, while `productDuplicate` copies the source product's effective memberships onto the staged duplicate; downstream `product.collections`, `collection(id:)`, collection `products`, `productsCount`, `hasProduct`, and top-level `collections(query: "product_id:...")` reads resolve from the same staged membership rows.
- `productSet(input.variants[].inventoryQuantities[])` accepts the live Shopify shape with `locationId`, `name`, and `quantity`. Staged create and update flows store those entries as inventory item `inventoryLevels` rows instead of only collapsing them onto the variant. Downstream `product`, `productVariant`, and `inventoryItem` reads expose the location-level `inventoryLevels`, selected `quantities(names: ...)`, aggregate variant `inventoryQuantity`, and product `totalInventory` from the staged graph. Current live evidence uses `name: "available"`; the local row mirrors that quantity into `on_hand` for read parity and leaves `incoming` at `0` unless separately hydrated.
- Product-level `totalInventory` intentionally follows the captured `productSet` timing rather than the generic variant mutation summary path: synchronous create counted the tracked variant's available quantity, while a follow-up `productSet` variant inventory update changed variant and inventory-item quantities immediately but left `product.totalInventory` at the prior aggregate in both the mutation payload and immediate downstream reads.
- `inventoryItems(...)` lists inventory items from the same product-variant-backed inventory graph as `inventoryItem(id:)`; snapshot no-data reads return an empty connection with false page booleans and null cursors. The local search slice covers captured-safe `id`, `sku`, and `tracked` terms and otherwise stays permissive like other early product search slices.
- `inventoryProperties` returns the captured 2025-01 inventory quantity-name catalog: `available`, `committed`, `damaged`, `incoming`, `on_hand`, `quality_control`, `reserved`, and `safety_stock`, including `belongsTo` / `comprises` relationships used by local quantity staging.
- `inventoryAdjustQuantities` stages incremental quantity changes over effective product-backed inventory levels without runtime Shopify writes. Captured 2025-01 evidence covers available deltas, non-available `incoming` adjustments with per-change ledger document URIs, invalid quantity names, missing required nested fields, unknown inventory items/locations, app metadata, and the `staffMember` access-denied companion error. Available deltas mirror into `on_hand` changes and update `inventoryItem.variant.inventoryQuantity` immediately while leaving `product.totalInventory` and product inventory search/count surfaces stale; non-available deltas are visible through downstream `inventoryItem.inventoryLevels(...).quantities(names: ...)` reads without changing available/on_hand totals. The executable parity target for the non-available branch compares the whole selected downstream product/variant/item payload and only carves out volatile timestamps plus reconstructed inventory-level GIDs.
- `inventorySetQuantities` stages absolute quantity writes over effective inventory item levels without runtime Shopify writes. Captured 2025-01 evidence showed `ignoreCompareQuantity: true` accepting available set writes, returning an `InventoryAdjustmentGroup`, mirroring available deltas into `on_hand` changes, immediately updating `inventoryItem.variant.inventoryQuantity`, and leaving `product.totalInventory` stale in immediate downstream reads. The local model also applies the same `on_hand` relationship for other component quantity names and rejects `on_hand` as a directly staged quantity name.
- `inventoryMoveQuantities` stages same-location quantity moves over effective inventory item levels. Captured 2025-01 evidence showed an available-to-damaged move returning two `InventoryChange` rows, keeping `on_hand` unchanged because both names belong to `on_hand`, updating variant inventory quantity from available totals, and preserving stale product-level `totalInventory`. Different-location moves, same-name moves, and unsupported ledger-document branches return visible local `userErrors` and do not contact Shopify.
- Version drift to watch: Shopify's current 2026-04 Admin GraphQL inventory examples require `@idempotent` keys for `inventoryAdjustQuantities` and show `changeFromQuantity` instead of the older `compareQuantity` / `ignoreCompareQuantity` shape for `inventorySetQuantities`; current local inventory parity is still anchored to the checked-in 2025-01 captures. Do not claim 2026-04 inventory quantity mutation fidelity until a dedicated capture updates the route-version-specific input contract and validation branches.
- `inventoryTransfer` and `inventoryTransfers` are modeled for locally staged transfer records. Empty snapshot state returns `inventoryTransfer(id:)` as `null` and `inventoryTransfers(first:)` as an empty connection with false page booleans and null cursors. The current search/sort slice is intentionally narrow: locally staged transfers are listed in staging order with shared cursor pagination.
- Inventory transfer lifecycle mutations stage records in memory and never proxy supported roots at runtime. Draft create/edit/set-items/remove-items/duplicate/delete preserve downstream transfer reads and raw mutation log order. `inventoryTransferCreateAsReadyToShip` and `inventoryTransferMarkAsReadyToShip` mirror the captured 2025-01 inventory effect by moving origin-level `available` quantity into `reserved`; `inventoryTransferCancel` releases that local reservation when canceling a ready transfer. Product-level `totalInventory` remains stale, matching the existing inventory quantity timing rule.
- Transfer conformance probes on `harry-test-heelo.myshopify.com` confirmed `InventoryTransferLineItem` exposes `totalQuantity`, `shippableQuantity`, `shippedQuantity`, `processableQuantity`, and `pickedForShipmentQuantity`, not a generic `quantity` field. A draft create with an untracked inventory item returns `userErrors[{ field: ["input","lineItems","0","inventoryItemId"], message: "The inventory item does not track inventory." }]`. Marking a stocked draft transfer ready returned `READY_TO_SHIP`, made the line item shippable, and changed immediate downstream inventory quantities from `available: 5, reserved: 0, on_hand: 5` to `available: 3, reserved: 2, on_hand: 5`.
- Inventory shipment lifecycle support is bounded to product-backed inventory items. Live 2025-01 schema introspection confirmed `inventoryShipment(id:)`, the shipment payload fields (`lineItems`, `lineItemsCount`, status totals, and `tracking`), and the mutation argument names for create/create-in-transit, add/remove/update items, set tracking, mark in transit, receive, and delete. Direct live payload capture for these roots is currently blocked because the conformance grant lacks `read_inventory_shipments` / `write_inventory_shipments`; the executable parity fixture records that blocker and compares the local proxy's create-in-transit, shipment detail read, and downstream inventory read behavior against the staged recording. Local staging preserves `movementId` as metadata but does not yet model transfer routing, origin/destination movement topology, or Shopify shipment name allocation.
- Staged shipment quantities update the same effective inventory-level graph used by `inventoryItem` reads: create-in-transit and mark-in-transit add unreceived quantities to `incoming`; add/remove/update item mutations adjust `incoming` atomically for in-transit shipments; receive with `ACCEPTED` moves quantities from `incoming` into `available` and `on_hand`; receive with `REJECTED` only reduces `incoming`; delete reverses remaining unreceived `incoming` and tombstones the local shipment. Rejected shipment mutations leave shipment and inventory state unchanged and return local `userErrors` without upstream writes.
- `collectionByIdentifier` supports id and handle identifier branches against effective local collection state. `customId` returns `null` until collection unique-metafield evidence exists.
- `collectionByHandle` is a deprecated Shopify root but is supported as a handle lookup over effective local collection state.
- Missing product-adjacent by-id roots return `null` without inventing records. The `product-related-by-id-not-found-read` parity scenario captures this for `collection(id:)`, `productVariant(id:)`, `inventoryItem(id:)`, and `inventoryLevel(id:)`.
- Bulk product variant mutations validate the full submitted batch before writing staged state. Executable conformance coverage for `productVariantsBulkCreate`, `productVariantsBulkUpdate`, and `productVariantsBulkDelete` lives in `config/parity-specs/product-variants-bulk-validation-atomicity.json`, replaying the capture at `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-variants-bulk-validation-atomicity.json`. It covers unknown product ids, missing and unknown variant ids, empty batches, duplicate option names, unknown product options, missing required option values, invalid inventory locations on create, `inventoryQuantities` on update, and mixed valid/invalid batches.
- `productVariantsBulkCreate.strategy` standalone-variant behavior is fixture-backed by 2025-01 `harry-test-heelo` captures. With `strategy: DEFAULT`, Shopify deletes the standalone default `Default Title` variant before creating the submitted variant, but preserves a standalone custom variant created through `productOptionsCreate(variantStrategy: LEAVE_AS_IS)`. With `strategy: REMOVE_STANDALONE_VARIANT`, Shopify deletes either standalone default or standalone custom variant before creating the submitted variant. All four captured branches returned empty `userErrors` arrays, so there were no `field`, `message`, or `code` values to model for those success paths.
- Rejected bulk variant create and delete payloads follow the captured null/empty payload shape: create returns `product: null` with `productVariants: []`, update returns the current `product` with `productVariants: null` for variant-level errors and `product: null` for empty/unknown-product errors, and delete returns `product: null` for unknown or non-member variants. Empty create and delete batches are no-op successes, while empty update returns Shopify's generic "Something went wrong, please try again." user error. All rejected branches leave staged options, variants, inventory items, inventory quantities, and product inventory summaries unchanged.
- Remaining bulk variant validation gaps: local responses currently model the captured `field` and `message` user error surface used by app runtime tests, but not every Shopify `code` value is selected/serialized across product-domain mutation payloads. GraphQL variable-shape errors for missing non-null fields in `inventoryQuantities` are still left to the GraphQL input contract rather than hand-built by the proxy.
- Product media validation follows the captured Shopify branches in `product-media-validation-branches`, which is replayed by `pnpm conformance:parity` against the local proxy. Unknown product IDs return `Product does not exist` media errors with null media/delete payload slots; invalid image `originalSource` values return indexed `media.<index>.originalSource` errors; invalid `CreateMediaInput.mediaContentType` enum values return top-level `INVALID_VARIABLE` errors. Empty media/update/delete lists are accepted as empty successes. Mixed create batches stage valid media and report invalid entries, while mixed update/delete batches with unknown media IDs are rejected atomically and leave staged media unchanged.

## Historical and developer notes

### Validation anchors

- Runtime flows: `tests/integration/product-draft-flow.test.ts`
- Inventory adjustment and quantity roots: `tests/integration/product-draft-flow.test.ts`, `tests/integration/inventory-quantity-roots.test.ts`, `config/parity-specs/inventoryAdjustQuantities-parity-plan.json`, and `config/parity-specs/inventory-quantity-roots-parity.json`
- Inventory transfer lifecycle roots: `tests/integration/inventory-transfer-flow.test.ts`
- Inventory shipment lifecycle roots: `tests/integration/inventory-shipment-flow.test.ts`, `config/parity-specs/inventory-shipment-lifecycle-local-staging.json`, and `fixtures/conformance/local-runtime/2026-04/inventory-shipment-lifecycle-local-staging.json`
- Product reads: `tests/integration/product-query-shapes.test.ts`
- Collection reads and mutations: `tests/integration/collection-query-shapes.test.ts`, `tests/integration/collection-draft-flow.test.ts`
- Location and publication reads: `tests/integration/location-query-shapes.test.ts`, `tests/integration/publication-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/product*.json`, `config/parity-specs/products*.json`, `config/parity-specs/collection*.json`, `config/parity-specs/metafieldsSet-owner-expansion.json`, and matching files under `config/parity-requests/`
- Product helper roots parity: `config/parity-specs/product-helper-roots-read.json`, `config/parity-requests/product-helper-roots-read.graphql`, and `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-helper-roots-read.json`, captured by `corepack pnpm conformance:capture-product-helper-reads`
- Product merchandising read fixture: `config/parity-specs/product-feeds-empty-read.json`, `config/parity-requests/product-feeds-empty-read.graphql`, and `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-feeds-empty-read.json`
- Product merchandising mutation guardrail fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-merchandising-mutation-probes.json`
- Selling-plan group lifecycle fixture: `config/parity-specs/selling-plan-group-lifecycle.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/selling-plan-group-lifecycle.json`
- Product handle validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-handle-validation-parity.json`
- Bulk variant validation/atomicity parity: `config/parity-specs/product-variants-bulk-validation-atomicity.json`, `config/parity-requests/productVariantsBulk*-validation-atomicity.graphql`, and `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/product-variants-bulk-validation-atomicity.json`
- Product media reorder parity: `config/parity-specs/productReorderMedia-parity.json` replays the captured `productReorderMedia` mutation and downstream `product.media`/`product.images` reads through the generic parity runner. The downstream media comparison keeps order, IDs, alt text, and media type strict; it excludes one async-processing status transition that Shopify changed after setup independently of the reorder.
- Legacy single-variant parity: `config/parity-specs/productVariantCreate-parity-plan.json`, `productVariantUpdate-parity-plan.json`, and `productVariantDelete-parity-plan.json` execute the local legacy roots against strict comparison targets derived from equivalent live-supported bulk variant captures while HAR-189 tracks direct legacy-root schema availability.

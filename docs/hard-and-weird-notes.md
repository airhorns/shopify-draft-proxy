# Hard and Weird Notes

This file records implementation surprises and fidelity traps discovered while building the first supported product queries/mutations.

## 1. GraphQL operation names are not reliable routing keys

A big early trap: the GraphQL **operation name** (`query Foo`, `mutation Bar`) is user-controlled and often omitted entirely.

The proxy cannot route by operation name alone.

Instead, routing should key primarily off the **root field name**:

- `product`
- `products`
- `productCreate`
- `productUpdate`
- `productDelete`

The code now captures both operation name and root field names, and capability routing checks both.

## 2. Query overlay fidelity is much harder than mutation staging

Staging mutations locally is relatively straightforward compared with answering reads afterward.

The hard part is not `productCreate` itself — it is making all the following reads behave as if Shopify had truly materialized that product, including:

- list ordering
- filtering
- pagination
- derived counts
- nested relationships
- search semantics
- status/publication visibility

Current implementation only supports a narrow product subset:

- scalar product fields: `id`, `title`, `handle`, `status`, `createdAt`, `updatedAt`
- `products { nodes }`
- `products { edges { cursor node } }`
- basic `pageInfo`
- simple `first` slicing

## 3. Shopify empty-data behavior is field-specific, not generic

"Return empty data when missing" sounds simple, but in practice Shopify behavior depends on the field shape:

- singular lookup fields often return `null`
- connections usually return a non-null object with empty `nodes`/`edges`
- some mutations return `userErrors`
- some paths may surface authorization or validation failures before nullability matters

So snapshot-mode fidelity cannot be implemented as a single generic fallback rule. It has to be modeled per field family.

## 4. Hybrid mode needs normalization, not blind JSON patching

For supported product reads, the proxy now normalizes a small upstream product shape into in-memory state before overlaying staged edits.

This is a clue for the future architecture: if we try to patch raw GraphQL JSON blobs directly, fidelity will become brittle very quickly. The long-term path is:

1. normalize upstream into domain entities
2. apply staged overlay at the entity layer
3. re-serialize into requested GraphQL shape

## 5. Handle generation is already weird

Right now handles are just slugified from title. That is intentionally simplistic.

What is still missing:

- uniqueness conflicts
- reserved/invalid handles
- how Shopify mutates handles after updates
- interaction with duplicate titles

This will matter a lot once we have conformance tests.

## 6. Product update semantics are under-modeled

Current `productUpdate` behavior is best-effort:

- if the product exists in effective state, merge over it
- if it does not, synthesize a skeletal product record

That is useful for tests, but probably not truly Shopify-like. Real parity will need:

- better missing-record behavior
- better validation/userErrors
- understanding which fields are required vs optional vs server-derived

## 7. Pagination and sorting are going to get gnarly fast

Current `products(first: N)` support no longer stops at the default `createdAt desc` order. Overlay reads now also cover an explicit sort-key slice including:

- `TITLE`
- `UPDATED_AT`
- `INVENTORY_TOTAL`
- `CREATED_AT`
- `VENDOR`
- `PRODUCT_TYPE`
- `HANDLE`
- `STATUS`

Real Shopify behavior still likely depends on more than that:

- stable default sort keys
- reverse behavior
- explicit sort keys beyond the current slice
- query/search filters
- cursors derived from the server's ordering model

This is still one of the biggest fidelity cliffs for list queries.

### 7a. Derived collection product lists must inherit product sort-key growth

Top-level `collection.products` reuses the same overlay-aware product connection serializer as top-level `products`.
That means every useful new product sort key should be treated as shared list semantics work, not as a top-level-only feature. If a sort key only works on `products` but not on derived `collection.products`, merchant-facing collection views drift immediately.

## 8. Supported mutation passthrough must stay off by default

The new supported product mutations are now staged locally and do not hit Shopify.

That is good — but it raises a design pressure point:

- supported mutations should stay local
- unsupported mutations still passthrough today

That means a mixed-fidelity test suite can still accidentally mutate real Shopify if it wanders outside covered operations. Observability and conformance coverage matter a lot here.

## 9. The serializer surface will expand quickly

Even a tiny amount of product support already needed custom serialization for:

- direct product objects
- product connections
- `nodes`
- `edges`
- `pageInfo`
- alias support via GraphQL field aliases

As soon as variants, options, metafields, collections, media, and publications show up, this serializer layer will become a major subsystem.

## 10. Conformance tests are the only way to settle many of these questions

Several behaviors are still guessed or simplified on purpose:

- exact `userErrors` contents
- handle uniqueness behavior
- default ordering
- update/delete error semantics
- exact mutation payload shape beyond a minimal useful subset

The right next step is not endless speculation. It is conformance capture against a real Shopify dev store.

## 11. On this host, Shopify account bearer tokens work directly against Admin GraphQL

A useful discovery for conformance work on this host class:

- the Shopify CLI account session stored in `~/.config/shopify-cli-kit-nodejs/config.json`
- can be used against store Admin GraphQL endpoints
- when sent as `Authorization: Bearer ***`
- and, for compatibility with current probing scripts, mirrored into `X-Shopify-Access-Token: Bearer <token>`

Using the raw token only as `X-Shopify-Access-Token` failed with `Invalid API key or access token`, but the bearer-header form succeeded.

This is important because it means early read-focused conformance capture can proceed even before a separate custom-app token flow is fully automated.

### 11a. Those Shopify CLI bearer tokens are brittle for unattended conformance

A later failure mode on this same host class is worth recording explicitly:

- `corepack pnpm conformance:probe` can fail with `401` / `[API] Service is not valid for authentication`
- attempting the documented non-interactive refresh flow can then return OAuth `400 invalid_grant`
- when that happens, the persisted access/refresh pair in both the CLI config and repo `.env` is no longer recoverable non-interactively

Takeaway:

- do **not** keep retrying the same refresh request after `invalid_grant`
- for unattended cron conformance, a dedicated dev-store Admin API token is more durable than mirroring a rotating Shopify CLI account bearer token
- if a human repairs CLI auth, they must persist the fresh token pair into both `~/.config/shopify-cli-kit-nodejs/config.json` and repo `.env`

### 11b. Dedicated Admin API tokens need different header handling than Shopify CLI bearer tokens

Once conformance was switched from the expired CLI session to a store-auth / Admin API token, another auth trap showed up:

- store-auth tokens here are shaped like `shpat_...`
- sending them as `Authorization: Bearer <token>` fails with `401` / `Invalid API key or access token`
- sending them as raw `X-Shopify-Access-Token: <token>` succeeds
- sending both headers can also succeed as long as `X-Shopify-Access-Token` carries the raw token value

So the conformance scripts cannot unconditionally coerce every token into bearer form.

Practical rule now used by the repo:

- `shpat_...` token → send raw `X-Shopify-Access-Token`
- Shopify CLI account token / explicit `Bearer ...` token → keep bearer-header behavior

This is the durable fix that lets live conformance run on either credential type instead of assuming only the older CLI-session path.

### 11c. Probe success does not imply product-write scope for live mutation capture

A later conformance run showed another important distinction:

- `corepack pnpm conformance:probe` can succeed against the target store
- the same token can still fail immediately on `productCreate` with GraphQL `ACCESS_DENIED`
- Shopify reports required access `write_products` plus product-create permission requirements

That means read-capable conformance credentials are not automatically sufficient for closing staged product mutation gaps. For the `productCreate` / `productUpdate` / `productDelete` family, the repo now has a dedicated live-write capture harness, but it cannot produce captured fixtures until the conformance credential can safely perform product writes on the dev store.

Practical rule:

- treat this as a **scope blocker**, not a proxy/runtime bug
- keep the safe write pack explicit and ready to rerun
- switch to a safe dev-store token with `write_products` before trying to promote product mutation scenarios from planned to captured/covered

## 12. Product variants are the first real nested product fidelity cliff

Once `variants` were added, the product serializer stopped being a scalar-only object mapper and became a nested connection serializer.

That introduces a few durable design lessons:

- staged product creation should synthesize a stable default variant
- upstream hydration must normalize nested variant nodes, not just top-level products
- product updates must preserve variant state unless a variant-specific mutation changes it
- the serializer layer will need the same pattern again for options, media, metafields, and collections

## 13. Product options are a plain array, not a connection

The first options increment exposed an important Shopify shape distinction:

- `product.variants` is returned as a connection
- `product.options` is returned as a plain array
- option values are nested arrays under each option, not their own connection in this slice

That means the serializer cannot treat every nested product child as a connection. It needs field-specific shape handling.

Current modeled behavior:

- staged `productCreate` synthesizes a default `Title` option
- that default option contains one `optionValues` entry: `Default Title`
- the merchant-facing `product.options` slice also needs a plain `values` array derived from `optionValues[].name`; Shopify examples routinely request both in the same payload
- live-hybrid hydration preserves upstream option ids, names, positions, and `hasVariants`
- local option mutations are split across three real root fields, not one family name: `productOptionsCreate`, `productOptionUpdate` (singular), and `productOptionsDelete`
- a useful first staged-mutation slice is LEAVE_AS_IS-style option list editing: insert/reorder options, rename them, and add/update/delete option values while leaving variant fanout semantics for a later increment
- `productUpdate` preserves option state unless a future option-specific mutation changes it

## 14. Variant detail reads already mix merchandising and inventory concepts

The `product-variants-matrix.json` conformance fixture shows that even a single `product.variants` read quickly crosses subsystem boundaries. A commonly-inspected merchant slice already includes:

- merchandising fields: `sku`, `barcode`, `price`, `compareAtPrice`
- sellability fields: `taxable`, `inventoryPolicy`
- stock state: `inventoryQuantity`
- option realization: `selectedOptions { name value }`
- nested inventory state: `inventoryItem { id tracked requiresShipping }`

That means variant serialization cannot stay at `id` + `title` for long. The normalized variant record needs to preserve nested `selectedOptions` and the lightweight inventory-item summary so hybrid reads can survive a staged product overlay without losing upstream fidelity.

## 15. Collection memberships are another nested connection, but they are not variant-like

The `product-detail.json` conformance fixture shows product collections as a connection under `product.collections`, but the merchant-useful slice here is much lighter than variants:

- collection node fields commonly inspected first are `id`, `title`, and `handle`
- staged products with no memberships should still return a non-null empty connection object, not `null`
- hybrid reads need to preserve upstream memberships even after a staged `productUpdate`

That makes collections a good next serializer increment after variants/options, but it also reinforces that nested product children need field-specific modeling.

### 15a. Top-level collection reads can be bootstrapped from the product membership graph

The first top-level `collection` / `collections` increment did **not** introduce a separate normalized collection entity table yet. Instead, it derives the visible collection surface from the known product↔collection memberships already stored for `product.collections`.

Useful consequences:

- `collection(id: ...)` can be answered immediately from any product that already carries that membership
- `collection.products` can reuse the same overlay-aware product connection serializer as top-level `products`
- `collections` needs deduplication by collection id because the same collection appears once per member product in storage

### 15b. Dedicated top-level collection captures expose opaque cursors and a richer nested product slice

The first live top-level collection fixtures (`collection-detail.json` and `collections-catalog.json`) settled two important shape questions:

- Shopify uses opaque server cursors for both top-level `collections` and nested `collection.products`; they are not derivable from plain ids
- merchant-facing collection reads quickly ask for catalog-style product fields such as `vendor`, `productType`, `tags`, `totalInventory`, and `tracksInventory`, not just `id`/`title`

That means collection-read conformance cannot be inferred only from the lighter `product.collections` membership fixture. It needs its own captured top-level scenarios so cursor shape and nested catalog fields stay visible as the overlay serializer grows.

Important limitation to remember:

- in live-hybrid mode, top-level collection visibility is only as complete as the known membership graph
- today that means staged collection memberships and memberships learned from prior hydrated product reads are visible, but independently-hydrated collection entities are not modeled yet

So this is a useful merchant-facing read slice, but it is still a membership-derived view rather than a fully independent collections domain model.

### 15b. Product-scoped collection storage needs product-scoped deletion keys too

Once collection memberships were stored with a product-scoped internal key (`<productId>::<collectionId>`), replacement semantics needed the same storage key discipline on deletion paths.

A subtle bug showed up on repeated staged `productSet` collection replacements:

- the store correctly inserted staged collection memberships under the product-scoped storage key
- but replacement/deletion loops were deleting by raw `collection.id`
- that left superseded staged memberships resident in state
- downstream `product.collections`, top-level `collection(id:)`, and derived `collections` reads then leaked old memberships after a later replacement

Takeaway:

- once collection membership storage is product-scoped, every cleanup path (`replaceBaseCollectionsForProduct`, `replaceStagedCollectionsForProduct`, delete/reset flows) must delete by the same storage key, not by bare collection id
- repeated replacement semantics are especially important for `productSet`, because its `collections` input is a product-scoped replacement list rather than an additive patch

### 15c. Standalone collection staging has to overlay — not replace — membership-derived visibility
The first `collectionCreate` / `collectionUpdate` / `collectionDelete` pass added standalone collection state so the proxy can represent empty collections before any products are attached.

That uncovered an easy trap:

- top-level `collection` / `collections` reads need standalone collection rows so newly-created empty collections are visible immediately
- nested `product.collections` still comes from the product↔collection membership graph
- `collectionDelete` must hide the collection from both surfaces without reviving stale base memberships

A naive collection lookup over raw base membership rows leaked superseded collections after staged replacement flows like repeated `productSet(... collections: ...)` writes.

Takeaway:

- standalone collection rows and membership-derived collection visibility need one effective collection view keyed by collection id
- fallback membership lookup must walk the **effective** product collection surface, not raw base/staged membership tables, or old replaced memberships leak back into `collection(id:)`
- nested `product.collections` should overlay standalone staged `title` / `handle` edits onto membership rows instead of inventing a second membership mutation path prematurely

### 15d. Collection membership mutations need product-scoped empty-family replacement markers
The first `collectionAddProducts` / `collectionRemoveProducts` pass exposed a subtle overlay bug in the normalized store.

What went wrong:

- collection memberships are stored product-scoped (`product.collections` is the normalized source of truth)
- staged replacement helpers previously treated "has staged state" as `stagedCollections.length > 0`
- that works for additive writes, but it fails for removal-to-empty writes
- after removing the last collection membership from a product, the staged set is intentionally empty, so the old base memberships leaked back into downstream reads

Takeaway:

- product-scoped collection replacement needs an explicit staged-family marker separate from the staged rows themselves
- once a product's collection family has been staged, overlay reads must treat that staged family as authoritative even when it contains zero memberships
- this is the collection-membership version of the broader child-family replacement rule already learned for `productSet`

### 15e. Collection add/remove parity is asymmetric in a useful way
Public Shopify docs for the real mutations already force two different local semantics:

- `collectionAddProducts` is atomic for duplicate membership: if any requested product is already in the collection, return a `userErrors` entry and add none of them
- `collectionRemoveProducts` is async in Shopify and explicitly does **not** validate product existence or prior membership, so a pragmatic first local pass can remove known memberships immediately, ignore unknown product ids, and return a synthetic done `job`

That asymmetry is worth preserving in the local model instead of forcing one generic membership-mutation helper.

## 16. Edge objects should only include selected fields

While adding `product.collections`, a fidelity trap surfaced: connection edges must not blindly include `cursor` just because the runtime knows how to compute it.

If the GraphQL selection is:

- `edges { node { ... } }`

then the response edge object should not grow an unsolicited `cursor` field. This sounds obvious, but it is easy to violate when hand-serializing connection objects.

Takeaway: nested connection serializers should build edge payloads from the edge selection set itself, not from a hard-coded `{ cursor, node }` template.

## 17. Product media is a connection, but the merchant-useful first slice is presentation metadata

The `product-detail.json` conformance fixture shows `product.media` as another connection, but the first useful parity slice is not media mutation behavior or polymorphic subtype detail. It is the read shape merchants commonly inspect in product editors and merchandising tools:

- `mediaContentType`
- `alt`
- `preview { image { url } }`

A few durable takeaways from the first media increment:

- staged products with no media should still return a non-null empty `media` connection object, not `null`
- hybrid reads need to preserve upstream media ordering even though this initial slice has no stable node `id`
- a lightweight normalized media record can key off product id + position until richer conformance data forces a stronger identity model

That makes media similar to collections in connection structure, but different in identity and nested preview-shape requirements.

## 18. Variant mutations and product options cannot drift apart

The first option-mutation increment left a fidelity gap on the read-after-write side: variant mutations were updating `product.variants`, but `product.options` could stay stale.

A few durable lessons from closing that gap:

- staged `productVariantCreate` / `productVariantUpdate` / `productVariantDelete` and the bulk variant family need to re-sync option state, not just inventory summaries
- `optionValues.hasVariants` should be recomputed from the effective variant set after each staged variant mutation
- selected-option writes can introduce option values that were never explicitly staged through `productOptionsCreate`; for practical local parity, those missing values should be synthesized into the matching option rather than dropped
- preserving the older default single-variant shape still needs one special case: a default variant with no `selectedOptions` should keep the synthetic `Title` / `Default Title` option value marked as variant-backed
- this is another sign that product child state is coupled: variants drive catalog counts and inventory summaries, but they also drive option-value availability and merchandising affordances

## 19. Product media mutations are their own image-heavy compatibility slice

The first staged media-write increment surfaced a few durable modeling constraints:

- the real root fields are `productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia`
- mutation payloads commonly ask for media node fields that the earlier read slice did not expose yet, especially `id`, `status`, and `... on MediaImage { image { url } }`
- inline fragments on media nodes matter immediately because Shopify examples routinely request image URLs through `MediaImage.image.url`, not only through `preview.image.url`
- for the first worthwhile local pass, image media can reuse the staged source URL for both `preview.image.url` and `MediaImage.image.url` while keeping ordering product-scoped and stable in local state
- `productDeleteMedia` carries two delete-id lists (`deletedMediaIds` and `deletedProductImageIds`); for staged image deletes they should stay aligned instead of inventing a second image-only identity path

That means media writes are not just three new mutation cases — they also force the media serializer to understand richer node identity/state and inline-fragment selection semantics.

## 19. Product metafields split into singular lookup and connection shapes

The first metafields increment surfaced another field-family distinction under `product`:

- `product.metafield(namespace:, key:)` is a singular owner-scoped lookup and should return `null` when the requested key is absent
- `product.metafields(first: ...)` is a connection and should still return a non-null empty connection object for staged products with no metafields
- hybrid reads may hydrate both fields in one upstream payload, so normalization should deduplicate them into one stored metafield set instead of treating the singular lookup and connection as separate sources of truth

For the initial merchant-useful slice, preserving these fields has been enough:

- `id`
- `namespace`
- `key`
- `type`
- `value`

This is another case where the serializer layer needs field-specific handling rather than one generic nested-object rule.

## 18a. Staged metafield writes need product-scoped replacement semantics, not id-wise merge

Adding `metafieldsSet` / `metafieldDelete` exposed a subtle state-model trap:

- base product metafields often come from upstream hydration
- staged metafield writes are product-scoped edits against that effective set
- a delete cannot be represented by simply omitting the deleted metafield from a staged-by-id overlay if reads still union base + staged records

The workable first pass is:

- treat staged metafields as a **full replacement set per product** once any staged metafield state exists for that product
- compute `metafieldsSet` against the current effective product metafield set using `(namespace, key)` as the practical upsert identity
- compute `metafieldDelete` by replacing the staged product metafield set with the remaining effective rows

That keeps downstream `product.metafield(...)` and `product.metafields(...)` reads consistent after staged deletes without needing a separate metafield tombstone model yet.

## 19. Variant bulk mutations are useful before full option/variant fanout parity

A worthwhile intermediate step before true Shopify-grade variant fanout is supporting the real bulk mutation family with a narrower merchandising/inventory slice:

- `productVariantsBulkCreate`
- `productVariantsBulkUpdate`
- `productVariantsBulkDelete`

Even without full option-value combinatorics, this already unlocks common merchant flows:

- editing variant `sku`, `barcode`, `price`, and `compareAtPrice`
- staging `inventoryQuantity`, `inventoryPolicy`, and lightweight `inventoryItem` shipping/tracking state
- verifying variant-backed product search terms like `sku:` continue to reflect staged local state
- keeping top-level catalog fields like `totalInventory` and `tracksInventory` aligned with the effective staged variant set

The key design lesson is that variant mutations should update the normalized variant records first, then recompute product-level derived inventory summary from the effective variant set. If that derivation is skipped, `product.variants` may look correct while top-level `products` reads and counts still show stale inventory state.

## 19a. Single-variant compatibility mutations can reuse the bulk-variant staging model

## 20. `productDuplicate` needs product-scoped copies of the normalized child graph

The first `productDuplicate` pass surfaced a few durable state-model lessons:

- duplicating only the top-level product row is not enough; merchants expect downstream reads on the duplicate to immediately include its nested product-domain state too
- the useful first duplicate slice is the **effective** local graph, not just the raw base snapshot row:
  - product core fields
  - options
  - variants
  - collections
  - media
  - metafields
- product-scoped children split into two identity families:
  - some nodes should get fresh synthetic ids on the duplicate (`Product`, `ProductVariant`, `ProductOption`, `ProductOptionValue`, duplicated `InventoryItem`, duplicated `Metafield`)
  - some references should keep their real upstream ids because they still point at the same shared entity (`Collection` ids)
- that in turn exposed a storage-key trap for collection memberships: keying normalized product collections only by collection id cannot represent the same collection attached to two products at once. The store now needs a **product-scoped storage key** internally even though the serialized collection node `id` stays the real Shopify collection id.

Current pragmatic duplicate semantics are intentionally narrow until live conformance is healthy again:

- the duplicate stages locally and never hits upstream Shopify
- duplicated products are forced to `DRAFT`
- handles are regenerated from the new title (or `Copy of …` fallback title)
- exact Shopify payload extras / job fields / preview-url semantics should be settled with live conformance later rather than guessed further

Adding version-relevant `productVariantCreate`, `productVariantUpdate`, and `productVariantDelete` support did not require a second variant state engine. The useful first pass is a compatibility layer over the same normalized variant overlay used by the bulk family:

- `productVariantCreate(input: ...)` can synthesize one variant record, append it to the product's effective variant set, and then reuse the same inventory-summary recomputation path as bulk create
- `productVariantUpdate(input: { id, ... })` needs a variant-id lookup across the effective product set before it can stage the updated normalized variant record
- `productVariantDelete(id: ...)` similarly needs variant-id-to-product resolution before replacing the staged variant set for that product
- downstream read-after-write behavior should stay aligned with the existing overlay model, so the single-variant family should update `product.variants`, variant-backed `products(query: "sku:...")`, and `productsCount` immediately without hitting upstream

This is a good example of the digital-twin architecture paying off: once variant reads and the bulk mutation family exist, the older single-variant surface can ride the same normalized state model instead of adding a parallel ad hoc code path.

## 20. Nested product connections need the same cursor semantics as top-level products

Once product detail reads started carrying richer child connections, it was tempting to treat nested connections as "small arrays" and skip real connection semantics. That breaks quickly in real admin flows.

The new nested connection pagination work reinforced a few durable rules:

- `product.variants`, `product.collections`, `product.media`, and `product.metafields` should all honor connection args like `first` and `after`, not just dump the full hydrated child set
- nested `pageInfo` should be computed from the sliced child window, not from the unsliced backing array
- GraphQL argument resolution for nested fields must use the operation variables, not an empty variable bag, or queries like `variants(first: $first, after: $after)` silently degrade to the full connection
- selection-aware edge serialization matters here too: `edges { node { ... } }` should not leak `cursor` unless it was actually requested

This is another sign that child serializers are effectively mini connection runtimes, not just JSON mappers.

## 20. Products list reads need their own merchandising field slice

The `products-catalog-page.json` conformance fixture exposed a useful distinction between product detail reads and catalog/list reads. A merchant-facing products table often inspects fields that are not always part of the first detail increment:

- `legacyResourceId`
- `vendor`
- `productType`
- `tags`
- `totalInventory`
- `tracksInventory`

These should be preserved through overlay reads, not dropped to `null`, because merchants use them for search, filtering, and list rendering long before deeper write coverage exists.

## 20. Sparse staged product updates may arrive before upstream hydration

A subtle hybrid-mode trap showed up while expanding `products` search semantics: `productUpdate` can be staged before the proxy has ever hydrated the upstream product. In that case, title-only updates do not actually know the current upstream handle/tags/product type yet.

If the staging layer eagerly synthesizes replacements for those unknown fields, overlay reads drift from Shopify semantics:

- title-only updates can accidentally regenerate the handle
- `UPDATED_AT` sorting can put freshly staged updates behind older upstream rows because the synthetic timestamp is older than the hydrated base row

Current direction:

- treat unknown update-only fields as sparse until base hydration arrives
- preserve the upstream handle when no explicit handle update was requested
- ensure merged staged `updatedAt` sorts after the hydrated base row so local updates behave like materialized writes in merchant list views

## 21. Top-level `products` overlay reads need edge hydration, not just `nodes`

While expanding product search/filter/sort behavior, another fidelity trap showed up in live-hybrid mode:

- Shopify list reads often return `products { edges { node { ... } } }`, not `nodes`
- if the proxy only hydrates base state from `nodes`, then any later overlay read with staged product state can accidentally drop the upstream catalog slice entirely
- this is especially easy to miss because the proxy only reserializes from local state once staged writes exist

Takeaway: upstream list hydration for `products` must normalize both `nodes` and `edges[].node` forms before the overlay layer runs.

## 22. Top-level product connections should respect the selected edge/pageInfo fields

A related serializer trap surfaced on `products` list reads:

- if a query asks for `edges { node { ... } }`, the proxy must not inject `cursor`
- if a query asks for `pageInfo { hasNextPage }`, the proxy must not eagerly add `hasPreviousPage`, `startCursor`, or `endCursor`

This mirrors the earlier nested-connection lesson for collections/media/variants, but now at the top-level `products` connection too. Connection serializers should always build edge and `pageInfo` payloads from the actual selection set, not from a hard-coded full object template.

## 23. Cursor pagination has to run after overlay filtering/sorting, not before

The first `products(after: ...)` increment exposed another overlay-read ordering trap. Cursor pagination cannot slice the raw upstream or merged product set before query and sort rules are applied.

The correct order for top-level `products` reads is:

1. build effective products from base + staged state
2. apply supported `query:` filtering
3. apply sort ordering (`UPDATED_AT`, `TITLE`, `INVENTORY_TOTAL`, default order, `reverse`)
4. interpret the `after` cursor against **that ordered result**
5. slice `first: N`
6. derive `pageInfo` from the paginated window

If pagination runs before filtering/sorting, the same cursor can point at the wrong row once local overlays reorder products. Current support only covers forward pagination via `after` on top-level `products`, but that slice is already enough to show why pagination belongs at the end of the overlay pipeline.

## 24. Backward cursor pagination belongs in the same overlay pipeline as forward pagination

Adding top-level `products(before:, last:)` support reinforced that backward pagination cannot be a separate shortcut bolted onto the serializer. The same ordered effective catalog has to drive both directions:

1. build effective products from base + staged state
2. apply supported `query:` filtering
3. apply sort ordering (`UPDATED_AT`, `TITLE`, `INVENTORY_TOTAL`, default order, `reverse`)
4. apply cursor windowing (`after` and `before`) against that ordered result
5. apply `first` / `last`
6. derive `pageInfo` from the final slice and the excluded rows on each side

A few fidelity takeaways from this increment:

- `before` must be interpreted against the post-filter, post-sort overlay order, not raw upstream order
- `last` changes `hasPreviousPage` semantics because trimming from the tail means rows were excluded on the left even when no `after` cursor was provided
- `hasNextPage` can also be true for backward windows because `before` intentionally excludes later rows from the ordered result

That means forward and backward pagination should share one cursor-window model rather than growing two unrelated code paths.

## 25. Product detail fields need stable null defaults even before dedicated editors/mutations exist

Adding the first richer `product` detail slice exposed a useful modeling rule for scalar/object detail fields that are common in merchant UIs but not yet mutation-managed by the proxy:

- upstream-hydrated reads should preserve detail fields like `descriptionHtml`, `onlineStorePreviewUrl`, `templateSuffix`, `seo`, and `category` even after a staged `productUpdate`
- staged products created with no upstream hydration still need deterministic read shapes
- for this slice, a safe default is:
  - nullable scalar detail fields → `null`
  - `seo` → non-null object with nullable children
  - `category` → `null`

That gives downstream product-editor reads a more realistic baseline without pretending we already model all write semantics for SEO/category/detail editing.

## 26. `inventoryItem` is already a nested subtree, not just a small metadata blob

The richer `product-variants-matrix.json` conformance fixture shows that even the first useful `inventoryItem` slice needs its own nested shape handling:

- lightweight summary fields: `id`, `tracked`, `requiresShipping`
- origin/classification fields: `countryCodeOfOrigin`, `provinceCodeOfOrigin`, `harmonizedSystemCode`
- measurement subtree: `measurement { weight { unit value } }`

A few durable takeaways:

- hybrid variant hydration must normalize `inventoryItem` as a structured object, not flatten it to a couple of booleans
- serializer logic needs to stay selection-aware at each nested level (`inventoryItem` → `measurement` → `weight`)
- null origin/classification fields should still round-trip explicitly as `null` when selected, matching the conformance fixture shape

## 27. Product search/conformance queries often include multiple supported top-level root fields

The captured Shopify catalog queries do not always ask for a single root field. A realistic product admin query can request multiple supported product-domain roots in the same operation, for example:

- `productsCount { ... }`
- `products(first: ...) { ... }`

Two important fidelity consequences fell out of adding `productsCount` support:

- handling only the first root field silently drops aliased siblings like `total: productsCount(...)`
- overlay reads need to recompute all supported root fields from the same effective local state once staged mutations exist

Takeaway: supported query handling should iterate every supported root field in the operation, not assume a single-root happy path just because early tests did.

## 28. Rich product detail writes only preserve untouched fields after hydration

Expanding staged `productCreate` / `productUpdate` input support for common merchant fields (`vendor`, `productType`, `tags`, `descriptionHtml`, `templateSuffix`, `seo`) reinforced an earlier sparse-update lesson:

- staged create can safely synthesize these fields directly from mutation input because the proxy owns the whole staged row
- staged update can only preserve untouched fields like `onlineStorePreviewUrl` if the upstream/base product has already been hydrated into local state
- without prior hydration, the proxy should keep unknown fields sparse/null rather than inventing Shopify state

Practical takeaway: richer staged updates work best as merge-over-hydrated-base behavior, not as a license to backfill unknown upstream product detail fields during mutation handling.

## 29. Partial upstream product nodes must not erase hydrated nested children

Adding variant-backed `products(query: "sku:...")` / `products(query: "barcode:...")` support exposed a hybrid-mode trap:

- a detailed `product(id: ...)` read may hydrate variants into normalized base state
- a later top-level `products(...)` read often omits the `variants` subtree entirely
- treating that omission as "replace variants with empty list" destroys previously learned child state and breaks later overlay filtering/counts that depend on variants

Current direction:

- normalize whatever nested children are actually present in the upstream payload
- only replace the corresponding normalized child collection when that field was present on the upstream node
- preserve previously hydrated children when a later upstream payload simply omits them

This matters beyond variants. The same omission-vs-empty distinction applies to options, collections, media, and metafields whenever hybrid overlay reads rehydrate products from narrower catalog queries.

## 30. Hydrated status-only mutations should serialize the effective merged product, not the raw staged row

Adding `productChangeStatus` surfaced a subtle read-after-write trap for hydrated products:

- the staged mutation record gets a fresh synthetic `updatedAt`
- the effective product view may normalize that timestamp to sort after the hydrated base row (`ensureUpdatedAtAfterBase`)
- if the mutation response serializes the raw staged row but later reads serialize the merged effective row, the same product shows two different `updatedAt` values immediately after the mutation

Current direction:

- for hydrated product mutations that return a `product` payload, stage the local row first
- then serialize `store.getEffectiveProductById(...)` rather than the raw staged record
- keep mutation responses aligned with the downstream overlay reads merchants will do next (`product`, `products`, `productsCount`)

This is another sign that mutation payload fidelity depends on the same merge/overlay model as subsequent reads, not just on writing a plausible staged record.

## 31. Timestamp search terms should compare effective overlay timestamps, not raw staged clocks

Adding `created_at` / `updated_at` product search support exposed another hybrid-mode timestamp trap:

- staged mutation timestamps come from the synthetic clock, which starts far earlier than the real conformance fixtures on a fresh test run
- if a product was hydrated from Shopify first, the effective overlay view may normalize a staged `updatedAt` forward to `base.updatedAt + 1s`
- filtering against the raw staged timestamp would miss the very read-after-write rows merchants expect to find next

Current direction:

- evaluate timestamp search terms against the effective merged product set, just like `products` / `productsCount` already do for status, tags, and variant-backed fields
- support the same comparator slice on `created_at` and `updated_at` (`<`, `<=`, `>`, `>=`, `=`)
- keep `products` and `productsCount` aligned by running both through the same filtered effective product list

Practical takeaway: timestamp search semantics in hybrid mode are another place where the merged overlay view is the source of truth, not the raw staged record alone.

## 32. Broader search grammar only matters on the overlay path once local state exists

Adding a first broader `products` search-grammar slice (quoted phrases, bare text terms, and leading `-` negation) exposed another live-hybrid subtlety:

- when there is no staged local state, supported `products` / `productsCount` reads still passthrough the upstream body as-is
- that means local overlay-only behavior like extra search grammar and local sort keys is invisible until at least one staged mutation has put the request on the serializer/overlay path
- tests that expect aliased root fields or overlay-local query semantics therefore need to stage a product mutation first, even if that mutation is semantically neutral for the final assertion

The grammar slice itself also reinforced a useful modeling boundary:

- bare text terms are most useful as a lightweight merchant-facing full-text match across the effective product view (`title`, `handle`, `vendor`, `productType`, `tags`)
- quoted phrases need tokenization that preserves internal whitespace instead of naïvely splitting on spaces
- leading `-` negation composes cleanly when it wraps the same positive term matcher used by fielded filters
- `products` and `productsCount` should still share the exact same filtered effective product set so list reads and counts remain aligned

Related sort lesson:

- adding `sortKey: VENDOR` and `sortKey: PRODUCT_TYPE` needs a deterministic tiebreaker (`title`, then `id`) so reverse ordering stays stable within a shared vendor/product type bucket

## 33. OR/grouping/prefix search semantics need a real parser, not token-by-token hacks

The next search-grammar increment (grouped `OR` expressions plus trailing `*` prefixes) pushed the old whitespace token splitter past its limit.

A few durable takeaways:

- `OR` only becomes meaningful once the query layer can parse grouped boolean expressions like `(vendor:NI* OR vendor:CON*) status:active`
- prefix matching is more merchant-useful when it works across word boundaries inside a field value (`swoo*` should match `SWOOSH`, not only strings that begin with `swoo`)
- grouped search has to feed the same effective filtered product set into both `products` and `productsCount`, or list/count drift reappears immediately
- unary negation now exists in two useful forms: term-local `-tag:vans` and group-level `-(vendor:VANS OR tag:vans)`
- even with this parser in place, the overlay grammar is still intentionally partial; richer Shopify precedence/details should be conformance-settled rather than guessed

## 34. Aggregate publication staging needs placeholder identities before full publication-graph parity exists

The first `productPublish` / `productUnpublish` pass exposed a publication-state modeling trap similar to earlier metafield work, but with much thinner conformance data:

- the first merchant-useful read slice is often aggregate state on `product` itself (`publishedOnCurrentPublication`, `availablePublicationsCount`, `resourcePublicationsCount`)
- real publish/unpublish inputs target specific publication identities, but live-hybrid upstream hydration may only give us aggregate counts/booleans, not a full publication list to diff against later mutations
- a plain boolean flag is too weak because read-after-write parity needs counts to move coherently across publish and unpublish calls

Current pragmatic direction:

- keep the first staged publication model aggregate-first on the normalized product record
- represent hydrated-but-anonymous upstream publications with placeholder ids so later local publish/unpublish calls can still bump or decrement counts deterministically
- treat deprecated `channelId` / `channelHandle` inputs as alternate local target keys for this first pass instead of inventing a second state path
- defer richer publication graph/detail parity until conformance capture settles which publication/read surfaces matter most

This keeps the proxy useful for common merchant publication-status checks without pretending full `resourcePublicationsV2` / publication-edge fidelity already exists.

## 35. ProductSet list-field semantics need product-scoped staged child shadowing, not base+staged unions

The first `productSet` increment exposed a state-layer trap across every nested product child collection:

- `productSet` treats included list fields as replacement sets (`productOptions`, `variants`, `collections`, `metafields`)
- the old overlay getters for variants/options/collections/media were simple base+staged unions
- that union model cannot represent "delete the hydrated/base child records that were omitted from this staged replacement"

The practical consequence showed up immediately on read-after-write queries:

- omitted base variants could still appear after a staged `productSet`
- removed collection memberships leaked back into downstream `product.collections`
- option values from omitted base variants could survive through the option synchronizer and keep stale `hasVariants` / `values` output alive

Current direction:

- once a product has any staged child records for a given family, treat that staged set as the source of truth for that family on overlay reads
- keep the older base+staged union only for families that have no staged rows yet
- apply the same shadowing rule consistently to variants, options, collections, and media (metafields already had product-scoped staged replacement semantics)

This is broader than `productSet`: any staged mutation family that replaces a whole child slice needs the store to understand product-scoped replacement, not just additive overlay.

## 36. Top-level `productVariant` / `inventoryItem` reads can ride the normalized variant graph

The first top-level variant/inventory increment did **not** add a separate inventory-item table.
That was intentional.

The useful merchant slice was already living on the normalized variant records we hydrate and stage for product detail reads:

- variant merchandising fields (`sku`, `barcode`, `price`, `compareAtPrice`)
- variant inventory summary (`inventoryPolicy`, `inventoryQuantity`)
- selected options
- lightweight inventory-item detail (`tracked`, `requiresShipping`, `measurement.weight`, origin codes, harmonized code)

A simpler and more faithful first pass is therefore:

- answer `productVariant(id:)` by resolving the effective variant from the product-scoped variant overlay model
- answer `inventoryItem(id:)` by finding the effective variant that owns that inventory item
- derive nested back-references like `inventoryItem.variant.product` by walking the effective variant → effective product relationship at read time

Why this matters:

- staged product-level edits (for example a later `productUpdate` title change) then show up automatically under top-level variant/inventory reads without duplicating parent product state onto the inventory item itself
- product-scoped staged variant replacement semantics still apply, so top-level reads do not leak hydrated/base variants once a staged variant family becomes the source of truth for that product
- we can defer a separate inventory-domain state model until real conformance or inventory-level mutations require it

## 37. Generic tag mutations need product-scoped staging and inline-fragment-aware node serialization

`tagsAdd` and `tagsRemove` look deceptively small, but they surface two durable fidelity traps.

First, the root fields are generic across owner types. A product-domain digital twin should not claim broad generic support just because those mutation names exist. The safe first slice is:

- support `tagsAdd` / `tagsRemove` only for product ids
- stage onto the normalized `Product.tags` field rather than inventing a generic tag-owner table prematurely
- keep downstream `product`, `products(query: "tag:...")`, and `productsCount(query: "tag:...")` aligned by computing all of them from the same effective post-mutation product set

Second, the payload returns `node`, and callers often ask for product fields through `... on Product` inline fragments. That means mutation-payload serialization cannot assume plain field selections only. Product serialization now needs to honor inline fragments with `typeCondition: Product` inside `node` selections or useful payload fields like `tags` silently disappear even though the staged state is correct.

A small Shopify-specific shape wrinkle worth preserving:

- `tagsAdd` usefully accepts either `[String!]!` values or a comma-separated string in practice, so local staging should normalize both into one trimmed, deduplicated tag list
- `tagsRemove` is narrower; a good first pass can require an explicit array and ignore unknown/non-member tags rather than fabricating errors for no-op removals

## 38. First-pass inventory adjustments are product-backed, not true location-ledger parity

The first `inventoryAdjustQuantities` increment is intentionally narrow and rides on the existing product-scoped variant model rather than introducing a separate inventory ledger.

That yields a useful merchant-facing slice, but with explicit limits:

- only the `available` quantity name is staged locally for now
- mutation inputs still accept `locationId`, and the mutation payload echoes it back on `changes.location { id }`, but the staged quantity is currently collapsed into the variant's single `inventoryQuantity`
- downstream read-after-write fidelity stays aligned because `product.totalInventory`, top-level `productVariant`, top-level `inventoryItem`, and `products` / `productsCount` inventory filters all recompute from that same effective variant set
- this is **not** yet true inventory-level parity; distinct per-location quantities, additional quantity names, and richer `InventoryAdjustmentGroup` / `InventoryChange` detail will need a broader inventory-domain state model later

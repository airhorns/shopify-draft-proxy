# Metafields

## Current support and limitations

### Metafield definition reads

The product-owner definition slice supports the Admin GraphQL read roots:

- `metafieldDefinition(identifier:)`
- `metafieldDefinitions(ownerType:, first:, after:, reverse:, sortKey:, query:, namespace:, key:, pinnedStatus:, constraintStatus:, constraintSubtype:)`

The implementation is snapshot/local only for definition reads. In snapshot mode, missing singular definitions return `null`, and catalog misses return a non-null empty connection with empty `nodes` / `edges` and falsey `pageInfo`.

Normalized state stores definition records separately from product metafields. The supported owner type is `PRODUCT`; definition-scoped `metafields` and `metafieldsCount` are derived from the effective product-owned metafield set by matching `namespace` and `key`. Staged definition lifecycle mutations update this same normalized catalog, so downstream definition reads and definition-backed `metafieldsSet` validation use the effective staged definition state.

The serializer currently covers these selected definition fields:

- `id`, `name`, `namespace`, `key`, `ownerType`
- `type { name category }`
- `description`
- `validations { name value }`
- `access`
- `capabilities`
- `constraints`
- `pinnedPosition`
- `validationStatus`
- `metafieldsCount`
- `metafields`

Catalog filters are intentionally limited to the fixture-backed product-owner slice: owner type, namespace, key, pinned status, constraint status/subtype, and search query terms for `id`, `namespace`, `key`, `owner_type`, and `type`. `sortKey: PINNED_POSITION` follows the captured Shopify ordering where higher pinned positions sort before lower pinned positions.

### Metafield definition lifecycle mutations

The product-owner lifecycle slice stages these roots locally without runtime Shopify writes:

- `metafieldDefinitionCreate(definition:)`
- `metafieldDefinitionUpdate(definition:)`
- `metafieldDefinitionDelete(id:|identifier:, deleteAllAssociatedMetafields:)`

Create supports the normalized fields represented by `MetafieldDefinitionRecord`: identity (`ownerType`, `namespace`, `key`), `name`, `description`, `type`, `validations`, selected `access`, selected `capabilities`, optional `pin`, empty constraints, and `validationStatus: ALL_VALID`.

Update resolves the existing definition by immutable identity (`ownerType`, `namespace`, `key`). It preserves `type`, `ownerType`, `namespace`, and `key`, and locally updates `name`, `description`, `validations`, selected `access`, and selected `capabilities`. The local `validationJob` payload is currently `null`.

Delete resolves by Shopify's preferred `identifier` input or by global `id`, hides the definition from singular and catalog reads with a staged tombstone, and compacts owner-type pin positions when deleting a pinned definition. When `deleteAllAssociatedMetafields: true`, the local effect conservatively removes matching product-owned metafields from the in-memory graph; it does not invent broad async job state.

Definition-backed `metafieldsSet` support now consults effective staged definitions for product, product variant, and collection owners. When the input omits `type`, the matching definition supplies it. When the input supplies a mismatched type, local validation rejects the write. Fixture-backed basic validations currently cover `max` string length and `regex`.

Live evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafield-definition-lifecycle-mutations.json`, captured with `corepack pnpm conformance:capture-metafield-definition-lifecycle`, covers product-owner create, downstream definition/metafield reads, update, delete with `deleteAllAssociatedMetafields: true`, and immediate downstream no-data reads after delete.

HAR-351 promotes that fixture from runtime-test-backed fixture evidence into `config/parity-specs/metafield-definition-lifecycle-mutations.json` as a strict generic proxy-vs-recording parity scenario. The parity runner seeds the recorded setup product, replays create, definition-backed `metafieldsSet`, downstream definition/product reads, update, delete, and post-delete no-data reads against the local proxy harness. Accepted differences are limited to local synthetic GIDs and the pinned-position offset caused by unrelated pinned definitions already present in the live capture shop.

### Standard metafield definition enablement

`standardMetafieldDefinitionEnable` stages a normalized metafield definition locally from the HAR-257 captured standard template slice. Supported selectors are the fixture-backed template IDs/namespaces in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/standard-metafield-definition-enable-validation.json`.

Successful local enablement:

- creates or replaces a staged `MetafieldDefinition` record without sending the mutation to Shopify
- supports `id` or `namespace` / `key` template selection for the captured template slice
- applies `ownerType`, selected `access`, selected `capabilities`, and `pin`
- when `pin: true`, assigns the next owner-type pinned position after any existing pinned definitions, matching the local pinning/create rule instead of reusing position `1`
- returns a Shopify-like `createdDefinition` payload
- makes downstream `metafieldDefinition(identifier:)` and `metafieldDefinitions(...)` reads observe the staged definition

HAR-257 captured validation behavior in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/standard-metafield-definition-enable-validation.json`:

- no `id` and no `namespace` / `key` returns `createdDefinition: null` with `TEMPLATE_NOT_FOUND`
- an unknown template ID returns `field: ["id"]`, `TEMPLATE_NOT_FOUND`
- an unknown namespace/key selector returns `field: null`, `TEMPLATE_NOT_FOUND`
- template ID `1` with incompatible owner type `CUSTOMER` returns the same invalid-template-ID branch

That fixture scope is not a rule against live success captures. Normal supported proxy runtime handling must not send this mutation to Shopify, but explicit conformance recording may create and clean up real standard definitions in a disposable test shop, and `__meta/commit` replay should let the queued raw mutation create its Shopify-side schema effect.

### Metafield definition pinning

The product-owner pinning slice supports local staging for existing normalized definition records:

- `metafieldDefinitionPin(definitionId:)`
- `metafieldDefinitionPin(identifier:)`
- `metafieldDefinitionUnpin(definitionId:)`
- `metafieldDefinitionUnpin(identifier:)`

Captured 2025-01 live behavior shows pinning an unpinned product definition assigns the next owner-type pinned position after the highest existing product definition position. Pinned definition catalogs sorted with `sortKey: PINNED_POSITION` return higher pinned positions first. Unpinning clears the target definition's `pinnedPosition` and compacts any higher pinned positions down by one, so downstream `metafieldDefinition` detail reads plus `metafieldDefinitions(... pinnedStatus: PINNED|UNPINNED)` catalogs reflect the staged change.

The local implementation intentionally covers pin/unpin for definitions already present in normalized snapshot, hydrated state, or staged lifecycle state. It does not create missing definitions through pin/unpin and does not model app-configuration-managed / unsupported-owner error branches yet.

## Historical and developer notes

Validation entry points:

- `tests/integration/metafield-definition-query-shapes.test.ts`
- `tests/integration/metafield-definition-draft-flow.test.ts`
- `config/parity-specs/metafield-definition-pinning-parity.json`
- `corepack pnpm conformance:capture-metafield-definition-pinning`
- `corepack pnpm conformance:capture-metafield-definition-lifecycle`

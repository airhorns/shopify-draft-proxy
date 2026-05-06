# Metafields

## Current support and limitations

### Metafield definition reads

The metafield definition slice supports the Admin GraphQL read roots:

- `metafieldDefinition(identifier:)`
- `metafieldDefinitions(ownerType:, first:, after:, reverse:, sortKey:, query:, namespace:, key:, pinnedStatus:, constraintStatus:, constraintSubtype:)`

In LiveHybrid, cold definition detail/catalog reads use passthrough when the local definition model has no staged or deleted definition state to overlay. Once a scenario stages, hydrates, or deletes metafield definitions, downstream reads stay local so read-after-write and read-after-delete behavior does not leak back to Shopify. In snapshot mode, missing singular definitions return `null`, and catalog misses return a non-null empty connection with empty `nodes` / `edges` and falsey `pageInfo`.

Normalized state stores definition records separately from metafield values. Definition records are owner-type scoped and can be staged for non-product owner types. Definition-scoped `metafields` and `metafieldsCount` read staged `metafieldsSet` values whose owner type, namespace, and key match the effective definition. Staged definition lifecycle mutations update this same normalized catalog, so downstream definition reads use the effective staged definition state.

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

Catalog filters cover owner type, namespace, key, pinned status, constraint status/subtype, and search query terms for `id`, `namespace`, `key`, `owner_type`, and `type`. `sortKey: PINNED_POSITION` follows the captured Shopify ordering where higher pinned positions sort before lower pinned positions.

### Metafield definition lifecycle mutations

The definition lifecycle slice stages these roots locally without runtime Shopify writes:

- `metafieldDefinitionCreate(definition:)`
- `metafieldDefinitionUpdate(definition:)`
- `metafieldDefinitionDelete(id:|identifier:, deleteAllAssociatedMetafields:)`

Create supports the normalized fields represented by `MetafieldDefinitionRecord`: identity (`ownerType`, `namespace`, `key`), `name`, `description`, `type`, `validations`, selected `access`, selected `capabilities`, optional `pin`, selected `constraints`, and `validationStatus: ALL_VALID`. Product-owner creates reject Shopify-incompatible namespace/key lengths and characters, overlong `name` / `description`, unsupported custom-data type names, protected or Shopify-reserved namespaces, constrained `pin: true` inputs, and owner-type pin-cap violations before staging any local definition.

Update resolves the existing definition by immutable identity (`ownerType`, `namespace`, `key`). It preserves `type`, `ownerType`, `namespace`, and `key`, and locally updates `name`, `description`, `validations`, selected `access`, selected `capabilities`, and selected constraint inputs. Public Admin 2026-04 exposes `constraintsUpdates`, which can set the constraint key, create/delete values, and clear all constraints with `key: null, values: []`; the proxy also handles the legacy/internal `constraints` mixed-operation shape and `constraintsSet` replace-all shape for staged update fidelity. The local `validationJob` payload is currently `null`.

App-owned namespace forms follow Shopify's canonicalization rule. Mutation inputs, identifier lookups, catalog namespace filters, pin/unpin selectors, and standard-definition namespace selectors resolve `$app:<suffix>` through the request's `x-shopify-draft-proxy-api-client-id` identity before validation, persistence, lookup, and serialization. Stored and returned definitions use the canonical `app--<api_client_id>--<suffix>` namespace. Canonical `app--<other_id>--<suffix>` create/update inputs from another API client return Shopify's top-level `ACCESS_DENIED` error shape instead of staging a definition.

Delete resolves by Shopify's preferred `identifier` input or by global `id`, hides the definition from singular and catalog reads with a staged tombstone, and compacts owner-type pin positions when deleting a pinned definition. When `deleteAllAssociatedMetafields: true`, the local effect conservatively removes matching product-owned metafields from the in-memory graph for `PRODUCT` definitions only; it does not invent broad async job state for other owner families.

Shopify Admin 2026-04 does not reject product-owned definition deletion when associated product metafields exist and `deleteAllAssociatedMetafields` is omitted or explicitly `false`: it deletes the definition, returns empty `userErrors`, and leaves the product metafields in place without a definition. The `true` flag removes those associated product metafields. 2026-04 `MetafieldDefinitionUpdateInput` is identifier-shaped (`namespace`, `key`, `ownerType`) and does not accept `id` or `type`; namespace/key/owner-type changes therefore resolve as `NOT_FOUND` for the supplied identifier rather than an immutable-field user error. Live 2026-04 introspection also does not expose the legacy/internal `constraints` or `constraintsSet` update inputs, so capture evidence for update constraint persistence focuses on public `constraintsUpdates` while runtime tests cover the additional local legacy branches.

Definition-backed `metafieldsSet` support now consults effective staged definitions for product, product variant, collection, customer, order, and company owners. When the input omits `type`, the matching definition supplies it. When the input supplies a mismatched type, local validation rejects the write. Fixture-backed basic validations currently cover `max` string length and `regex` for product-owned values; CUSTOMER, ORDER, and COMPANY value success paths are covered for definition create, `metafieldsSet`, and owner read-after-set.

Live evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-lifecycle-mutations.json`, captured with `corepack pnpm conformance:capture-metafield-definition-lifecycle`, covers product-owner create, downstream definition/metafield reads, update, delete with `deleteAllAssociatedMetafields: true`, and immediate downstream no-data reads after delete. `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-update-delete-preconditions.json` covers the 2026-04 delete flag and update identifier preconditions. The Gleam port preserves a minimal product shell when deleting associated product metafields through a definition delete, so a downstream `product(id:) { metafield(...) }` read returns the product object with `metafield: null` rather than collapsing the product root to `null`.

HAR-351 promotes that fixture from runtime-test-backed fixture evidence into `config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json` as a strict generic proxy-vs-recording parity scenario. The parity runner seeds the recorded setup product, replays create, definition-backed `metafieldsSet`, downstream definition/product reads, update, delete, and post-delete no-data reads against the local proxy harness. Accepted differences are limited to local synthetic GIDs and the pinned-position offset caused by unrelated pinned definitions already present in the live capture shop.

HAR-691 expands definition create/update/delete beyond `PRODUCT` because definitions are owner-type scoped records. Its required parity scenario covers CUSTOMER, ORDER, and COMPANY definition create plus CUSTOMER update, read-by-id, and delete. Rework evidence also covers creating definitions for CUSTOMER, ORDER, and COMPANY, setting matching metafield values with `metafieldsSet`, and reading those values back through the owner roots. `deleteAllAssociatedMetafields: true` remains scoped to product-owned metafields matching a deleted product definition's namespace/key and must not remove same-key product-variant, collection, customer, or other owner metafields without separate conformance evidence for those owner families.

### Metafield value type matrix

HAR-294 adds executable product-owned `metafieldsSet` set/read parity for 96 Shopify custom-data value types in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json`, replayed by `config/parity-specs/metafields/custom-data-metafield-type-matrix.json`.

The matrix covers scalar text, number, boolean, date/date-time, URL/color/language, JSON/rich text/link/money/rating, measurement types, supported `list.*` variants, and product/variant/collection reference values. The local model now normalizes captured Shopify value behavior for this slice: date-time values gain an explicit `+00:00` offset, decimal `jsonValue` stays string-shaped, measurement `value` JSON serializes uppercase units and integer measurement numbers as `.0`, list measurement `jsonValue` uses Shopify's lowercase or abbreviated units, and rating value strings use Shopify's key order.

The fixture documents excluded product-owned metafield types instead of adding placeholders. Exclusions are types that require separate definition-backed or resource-specific setup outside this disposable product matrix: `id`, `list.id`, metaobject/mixed references, company/customer/file/page/article/order/product-taxonomy references, and their list variants. Metaobject-owned `id`, metaobject reference, and mixed reference field values are covered by the HAR-294 metaobject matrix.

### Standard metafield definition enablement

`standardMetafieldDefinitionEnable` stages a normalized metafield definition locally from the captured standard template slice. Supported selectors are the fixture-backed template IDs/namespaces in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/standard-metafield-definition-enable-validation.json`, the 2026-04 read-only `standardMetafieldDefinitionTemplates(first: 100, excludeActivated: false)` probe noted in `fixtures/conformance/local-runtime/2026-04/metafields/standard-metafield-definition-enable-error-branches.json`, and the constrained-template evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-create-with-pin-guards.json`.

Successful local enablement:

- creates or replaces a staged `MetafieldDefinition` record without sending the mutation to Shopify
- supports `id` or `namespace` / `key` template selection for the captured template slice
- applies `ownerType`, selected `access`, selected `capabilities`, and `pin`
- translates the deprecated local inputs `useAsCollectionCondition`, `useAsAdminFilter`, and `visibleToStorefrontApi` into the corresponding capability/access records before staging
- rejects matching existing unstructured metafields with `UNSTRUCTURED_ALREADY_EXISTS` unless `forceEnable: true` is provided
- returns `INVALID_CAPABILITY` for ineligible capability input, and returns `TYPE_NOT_ALLOWED_FOR_CONDITIONS` for the deprecated collection-condition argument on an ineligible type
- returns `ADMIN_ACCESS_INPUT_NOT_ALLOWED` when merchant admin access is supplied for non-app-owned standard templates
- returns `LIMIT_EXCEEDED` when the product owner-type definition cap is already reached
- when `pin: true`, uses the same local pin validation as definition create/pin so constrained templates and owner-type cap violations return `createdDefinition: null` before staging
- when pin validation passes, assigns the next owner-type pinned position after any existing pinned definitions, matching the local pinning/create rule instead of reusing position `1`
- returns a Shopify-like `createdDefinition` payload
- makes downstream `metafieldDefinition(identifier:)` and `metafieldDefinitions(...)` reads observe the staged definition

HAR-257 captured validation behavior in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/standard-metafield-definition-enable-validation.json`:

- no `id` and no `namespace` / `key` returns `createdDefinition: null` with `TEMPLATE_NOT_FOUND`
- an unknown template ID returns `field: ["id"]`, `TEMPLATE_NOT_FOUND`
- an unknown namespace/key selector returns `field: null`, `TEMPLATE_NOT_FOUND`
- template ID `1` with incompatible owner type `CUSTOMER` returns the same invalid-template-ID branch

That fixture scope is not a rule against live success captures. Normal supported proxy runtime handling must not send this mutation to Shopify, but explicit conformance recording may create and clean up real standard definitions in a disposable test shop, and `__meta/commit` replay should let the queued raw mutation create its Shopify-side schema effect.

The public Admin GraphQL 2026-04 schema on the current conformance shop exposes `capabilities` and `access`, but no longer exposes `forceEnable`, `useAsCollectionCondition`, `useAsAdminFilter`, or `visibleToStorefrontApi` on `standardMetafieldDefinitionEnable`. `config/parity-specs/metafields/standard-metafield-definition-enable-error-branches.json` therefore combines live public captures for schema-exposed capability errors with local-runtime-backed contract targets for the deprecated/internal inputs required by this proxy fidelity slice.

### Metafield definition pinning

The product-owner pinning slice supports local staging for existing normalized definition records:

- `metafieldDefinitionPin(definitionId:)`
- `metafieldDefinitionPin(identifier:)`
- `metafieldDefinitionUnpin(definitionId:)`
- `metafieldDefinitionUnpin(identifier:)`

Captured live behavior shows pinning an unpinned product definition, or creating a product definition with `pin: true`, assigns the next owner-type pinned position after the highest existing product definition position. Pinned definition catalogs sorted with `sortKey: PINNED_POSITION` return higher pinned positions first. Unpinning clears the target definition's `pinnedPosition` and compacts any higher pinned positions down by one, so downstream `metafieldDefinition` detail reads plus `metafieldDefinitions(... pinnedStatus: PINNED|UNPINNED)` catalogs reflect the staged change.

HAR-699 captured the default 2025-01 product-owner pin cap as 20 pinned definitions. The 21st pin returns `pinnedDefinition: null` with `field: null`, message `Limit of 20 pinned definitions.`, and code `PINNED_LIMIT_REACHED`. Constrained definitions, represented by populated `constraints.key` or `constraints.values`, cannot be pinned and return `pinnedDefinition: null` with code `UNSUPPORTED_PINNING`.

The 2026-04 create-with-pin guard capture records the corresponding create-time branches: after 20 product definitions have been created with `pin: true`, the next pinned create returns `createdDefinition: null`, `field: ["definition"]`, message `Limit of 20 pinned definitions.`, and code `PINNED_LIMIT_REACHED`; constrained create with `pin: true` returns `createdDefinition: null`, `field: ["definition"]`, and code `UNSUPPORTED_PINNING`. A constrained standard template enable with `pin: true` returns `createdDefinition: null`, `field: null`, and code `UNSUPPORTED_PINNING`.

The local implementation intentionally covers pin/unpin for definitions already present in normalized snapshot, hydrated state, or staged lifecycle state. In LiveHybrid, a cold pin/unpin first hydrates the product-owner definition catalog through `upstream_query.fetch_sync`, then stages only the pin or unpin effect locally; parity cassettes provide that read deterministically. It does not create missing definitions through pin/unpin when no upstream definition can be hydrated, and it does not model app-configuration-managed / unsupported-owner error branches yet.

## Historical and developer notes

Validation entry points:

- `config/parity-specs/metafields/metafield-definition-create-input-validation.json`
- `config/parity-specs/metafields/metafield-definition-create-with-pin-guards.json`
- `config/parity-specs/metafields/metafield-definition-update-constraints.json`
- `config/parity-specs/metafields/metafield-definition-pinning-parity.json`
- `config/parity-specs/metafields/metafield-definition-pin-limit-and-constraint-guard.json`
- `config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json`
- `config/parity-specs/metafields/metafield-definition-non-product-owner-types.json`
- `config/parity-specs/metafields/metafield-definition-non-product-metafields.json`
- `config/parity-specs/products/metafieldsSet-*.json`
- `config/parity-specs/products/metafieldDelete-parity-plan.json`
- `config/parity-specs/products/metafieldsDelete-parity-plan.json`
- `corepack pnpm conformance:capture -- --run metafield-definition-pinning`
- `corepack pnpm conformance:capture -- --run metafield-definition-create-with-pin-guards`
- `corepack pnpm conformance:capture -- --run metafield-definition-lifecycle`
- `corepack pnpm conformance:capture -- --run metafield-definition-non-product-owner-types`
- `corepack pnpm conformance:capture -- --run metafield-definition-non-product-metafields`

### HAR-450 coverage review gaps

- `standardMetafieldDefinitionTemplates` remains registry-only declaration coverage; enabling a bounded template slice is modeled, but the catalog root itself should not be treated as locally supported until it has executable behavior and fixture-backed shape evidence.
- Product, product variant, collection, customer, order, and company are the current fixture-backed shared `metafieldsSet` owner surface for definition-backed success paths. Additional owner families still need capture-backed set/read evidence before being claimed beyond definition lifecycle staging.
- Definition lifecycle parity covers product-owner behavior plus HAR-691 non-product owner create/update/delete/read evidence. App-owned definitions, owner-specific access/capability quirks, non-product delete cascade behavior, and non-product owner families outside CUSTOMER/ORDER/COMPANY still need fresh conformance before support expands beyond normalized definition records.
- CAS/userError coverage for `metafieldsSet` is product-owned fixture evidence. Reuse the atomic validation and downstream-read expectations, but do not assume other owner families have identical validation branches without capture.

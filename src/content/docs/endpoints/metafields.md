---
title: 'Metafields'
description: 'Coverage notes and fidelity boundaries for Metafields.'
---

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

Create supports the normalized fields represented by `MetafieldDefinitionRecord`: identity (`ownerType`, `namespace`, `key`), `name`, `description`, `type`, `validations`, selected `access`, selected `capabilities`, optional `pin`, selected `constraints`, and `validationStatus: ALL_VALID`. The identity is owner-type scoped: a duplicate create for the same `(ownerType, namespace, key)` returns `createdDefinition: null` with field `["definition", "key"]`, code `TAKEN`, and Shopify's captured `Key is in use for Product metafields on the 'custom' namespace.` message shape, while the same namespace/key can coexist across distinct owner types. Product-owner creates reject Shopify-incompatible namespace/key lengths and characters, the literal business-reserved namespaces `shopify_standard` and `protected`, overlong `name` / `description`, unsupported custom-data type names, constrained `pin: true` inputs, owner-type pin-cap violations, and the captured 256 non-standard-definition resource-type cap before staging any local definition. The current 2025-01 conformance app accepts `shopify_standard` and `protected` namespace creates and the recorder preserves those exact live cases in the fixture, but strict parity excludes those two branches because the proxy intentionally keeps the local `RESERVED` guard as runtime-test-backed business logic. The resource-type cap is scoped separately for merchant-owned namespaces and app-reserved `app--<api_client_id>--...` namespaces, matching Shopify's app-owned versus merchant-owned count split.

Update resolves the existing definition by immutable identity (`ownerType`, `namespace`, `key`). It preserves `type`, `ownerType`, `namespace`, and `key`, and locally updates `name`, `description`, `validations`, selected `access`, selected `capabilities`, and selected constraint inputs. Public Admin 2026-04 exposes `constraintsUpdates`, which can set the constraint key, create/delete values, and clear all constraints with `key: null, values: []`; `key: <string>, values: []` is rejected with field `["definition"]`, code `INVALID_INPUT`, and message `Cannot change the constraint key without providing values.`. The proxy also handles the legacy/internal `constraints` mixed-operation shape and `constraintsSet` replace-all shape for staged update fidelity, applying the same keyed empty-values guard to `constraintsSet` even though public Admin 2026-04 rejects `constraintsSet` at GraphQL validation before resolver execution. When a product-owned validation update changes validations and matching product metafields exist, the update payload returns a synthetic `validationJob` with a Shopify-like `Job` GID, `done: false`, and `query: null`; downstream `job(id:)` and local `node(id:)` resolve the staged job with the same pending shape. Validation-unchanged updates continue to return `validationJob: null`. Live 2026-04 introspection exposes `validationJob` on `MetafieldDefinitionUpdatePayload`, but not on `MetafieldDefinitionCreatePayload` or `MetafieldDefinition`.

Definitions created through `standardMetafieldDefinitionEnable` retain their standard-template marker in local state. Updates that would change `name`, `description`, or `validations` reject before staging with field-specific `INVALID_INPUT` userErrors (`["definition", "name"]`, `["definition", "description"]`, or `["definition", "validations"]`) and keep the staged definition unchanged. Non-immutable update fields such as `pin`, `access`, and `capabilities` continue to stage normally on the same definition when their ordinary validation passes.

Capability handling is type- and owner-aware for the modeled capability slice. `uniqueValues` is eligible only for `id`, `number_integer`, `single_line_text_field`, and `url` definitions; `smartCollectionCondition` is eligible for `PRODUCT` `single_line_text_field` definitions; and `adminFilterable` is eligible only for modeled filterable owner/type combinations. Ineligible capability inputs on create/update return Shopify's captured `INVALID_CAPABILITY` user error without staging. `id` definitions auto-enable `uniqueValues` unless the input explicitly disables it, matching Shopify's required-capability behavior. Enabled `adminFilterable` definitions are capped at 50 per owner type; the 51st PRODUCT definition returns `OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS`. Serialized `capabilities.*.eligible` and `adminFilterable.status` are derived from this same local eligibility model rather than defaulting to eligible.

Access input handling follows the public Admin GraphQL 2026-04 surface captured in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-access-validation.json`. The public schema does not expose `access.grants`, so inline grants are rejected as a top-level `argumentNotAccepted` schema error before resolver execution, and variable-bound grants are rejected by variable validation. Merchant-owned create/update access requires writable admin access; `access.admin: MERCHANT_READ` returns Shopify's captured `INVALID` create user error and `INVALID_INPUT` update user error at `field: ["definition"]` without staging. `MERCHANT_READ_WRITE` is accepted as the public input spelling for the stored/default `PUBLIC_READ_WRITE` access record.

App-owned namespace forms follow Shopify's canonicalization rule. Mutation inputs, identifier lookups, catalog namespace filters, pin/unpin selectors, standard-definition namespace selectors, and namespace-taking value mutation inputs resolve `$app:<suffix>` through the request's `x-shopify-draft-proxy-api-client-id` identity before validation, persistence, lookup, and serialization. Stored and returned definitions and metafield values use the canonical `app--<api_client_id>--<suffix>` namespace. `metafieldsSet` also defaults an omitted or blank namespace to the current app's canonical namespace when that request identity is present. If a local value mutation needs app identity for `$app:` or the default app namespace and the header is absent, the proxy returns an explicit app-authorization user error instead of assuming the conformance app id. `metafieldsDelete` resolves `$app:<suffix>` before constructing delete identifiers, so it can delete canonical values written through the shorthand. The compatibility `metafieldDelete(input: { id })` path deletes the canonical staged record by id; modern public Shopify schemas no longer expose a namespace-taking singular value-delete input. Canonical `app--<other_id>--<suffix>` create/update inputs from another API client return Shopify's top-level `ACCESS_DENIED` error shape instead of staging a definition, while value mutation roots return Shopify's captured payload-level user error shape.

Delete resolves by Shopify's preferred `identifier` input or by global `id`, hides the definition from singular and catalog reads with a staged tombstone, and compacts owner-type pin positions when deleting a pinned definition. When `deleteAllAssociatedMetafields: true`, the local effect conservatively removes matching product-owned metafields from the in-memory graph for `PRODUCT` definitions only; it does not invent broad async job state for other owner families. Hydrated or restored definitions marked `appConfigManaged` reject delete with `APP_CONFIG_MANAGED`, and definitions marked `standardTemplateAppDependent` reject delete with `STANDARD_METAFIELD_DEFINITION_DEPENDENT_ON_APP`; public Admin GraphQL exposes neither internal flag directly, so the live delete-preconditions fixture records the current schema/candidate-discovery limitation while runtime tests cover the guarded local branches.

Definitions in app-reserved namespaces such as `app--<api_client_id>--<suffix>` reject `metafieldDefinitionDelete` when `deleteAllAssociatedMetafields` is omitted or false. The payload returns `deletedDefinition: null`, `field: null`, code `RESERVED_NAMESPACE_ORPHANED_METAFIELDS`, and message `Deleting a definition in a reserved namespace must have deleteAllAssociatedMetafields set to true.`, leaving the definition available to downstream reads. With `deleteAllAssociatedMetafields: true`, the delete succeeds and matching staged product metafields are removed. Existing `id` and reference-type delete guards keep their stricter type-specific errors when those types are deleted without the flag.

Shopify Admin 2026-04 does not reject non-reference, non-`id` product-owned definition deletion when associated product metafields exist and `deleteAllAssociatedMetafields` is omitted or explicitly `false`: it deletes the definition, returns empty `userErrors`, and leaves the product metafields in place without a definition. The `true` flag removes those associated product metafields. 2026-04 `MetafieldDefinitionUpdateInput` is identifier-shaped (`namespace`, `key`, `ownerType`) and does not accept `id` or `type`; namespace/key/owner-type changes therefore resolve as `NOT_FOUND` for the supplied identifier rather than an immutable-field user error. Live 2026-04 introspection also does not expose the legacy/internal `constraints` or `constraintsSet` update inputs, so capture evidence for update constraint persistence focuses on public `constraintsUpdates` while runtime tests cover the additional local legacy branches.

Reference-type and `id` product-owned definitions are stricter regardless of whether associated metafields exist. Shopify Admin returns a resolver `userErrors` payload instead of deleting when `deleteAllAssociatedMetafields` is omitted or false, with `REFERENCE_TYPE_DELETION_ERROR` for reference types, including `list.*_reference`, and `ID_TYPE_DELETION_ERROR` for `id`; the local handler follows that guard and emits the per-mutation `MetafieldDefinitionDeleteUserError` typename with `field: null` when selected. `deleteAllAssociatedMetafields: true` still deletes these definitions and removes matching product-owned metafields from local staged state.

Definition-backed `metafieldsSet` support consults effective staged definitions for product, product variant, collection, customer, order, and company owners. When a staged definition validation changes through `metafieldDefinitionUpdate(validations:)`, later local value writes are checked against the effective definition before mutating owner metafield state. Captured 2026-04 evidence proves a product-owned text definition accepts an initially long value, then rejects a later too-long `metafieldsSet` value after `max: 5`, leaves the existing value untouched, accepts a short replacement, and exposes that replacement through downstream product metafield reads. Captured 2025-01 parity also covers definition-backed `metafieldsSet` rejection for numeric min/max, regex, choices, rating scale, date/date_time bounds, and metaobject reference target-definition violations. When the input omits `type`, the matching definition supplies it. When the input supplies a mismatched type, local validation rejects the write. Date-time values accept Shopify's offset and fractional-second forms and serialize back with an explicit offset and no fractional seconds. CUSTOMER, ORDER, and COMPANY value success paths are covered for definition create, `metafieldsSet`, and owner read-after-set; PAGE, LOCATION, MARKET, and ARTICLE owner payloads are covered for owner type resolution in `metafieldsSet`.

Live evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-lifecycle-mutations.json`, captured with `corepack pnpm conformance:capture-metafield-definition-lifecycle`, covers product-owner create, downstream definition/metafield reads, update, delete with `deleteAllAssociatedMetafields: true`, and immediate downstream no-data reads after delete. `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-update-delete-preconditions.json` covers the 2026-04 non-reference delete flag and update identifier preconditions. `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-protected-guards.json` and `config/parity-specs/metafields/metafield-definition-protected-guards.json` cover standard-template immutable update fields, app-reserved namespace delete without the flag, retained-definition readback after the guarded delete, and delete-with-flag success. `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-delete-type-guard-no-metafields.json` and `config/parity-specs/metafields/metafield-definition-delete-type-guard-no-metafields.json` cover `id`, `product_reference`, and `list.product_reference` delete guards when no associated metafields exist. The Rust runtime preserves a minimal product shell when deleting associated product metafields through a definition delete, so a downstream `product(id:) { metafield(...) }` read returns the product object with `metafield: null` rather than collapsing the product root to `null`.

`config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json` promotes that fixture into a strict generic proxy-vs-recording parity scenario. The parity runner seeds the recorded setup product, replays create, definition-backed `metafieldsSet`, downstream definition/product reads, update, delete, and post-delete no-data reads against the local proxy harness. Accepted differences are limited to local synthetic GIDs and the pinned-position offset caused by unrelated pinned definitions already present in the live capture shop.

Definition create/update/delete support extends beyond `PRODUCT` because definitions are owner-type scoped records. Parity coverage includes CUSTOMER, ORDER, and COMPANY definition create plus CUSTOMER update, read-by-id, and delete. The owner-scoped duplicate fixture covers PRODUCT duplicate create returning `TAKEN`, CUSTOMER create with the same namespace/key succeeding, and both owner-type catalogs reading their own definition. Evidence also covers creating definitions for CUSTOMER, ORDER, and COMPANY, setting matching metafield values with `metafieldsSet`, and reading those values back through the owner roots. `deleteAllAssociatedMetafields: true` remains scoped to product-owned metafields matching a deleted product definition's namespace/key and must not remove same-key product-variant, collection, customer, or other owner metafields without separate conformance evidence for those owner families.

### Metafield value type matrix

Executable product-owned `metafieldsSet` set/read parity covers 96 Shopify custom-data value types in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json`, replayed by `config/parity-specs/metafields/custom-data-metafield-type-matrix.json`.

Custom namespace keys that coincide with type names are ordinary merchant keys, not type-shape sentinels. `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafieldsSet-custom-namespace-typed-keys.json` and `config/parity-specs/metafields/metafieldsSet-custom-namespace-typed-keys.json` cover non-empty `custom.json`, `custom.rating`, and `custom.money` writes plus immediate product read-back. The local write path stages those records through the normal owner-metafield model so selected `value`, `jsonValue`, identity, owner type, timestamps, and non-empty compare digests are derived from the submitted value.

The matrix covers scalar text, number, boolean, date/date-time, URL/color/language, JSON/rich text/link/money/rating, measurement types, supported `list.*` variants, and product/variant/collection reference values. The local model now normalizes captured Shopify value behavior for this slice: date-time values gain an explicit `+00:00` offset, decimal `jsonValue` stays string-shaped, measurement `value` JSON serializes uppercase units and integer measurement numbers as `.0`, list measurement `jsonValue` uses Shopify's lowercase or abbreviated units, and rating value strings use Shopify's key order.

`metafieldsSet` value validation rejects invalid scalar and structured values before staging any input in the batch. Fixture-backed branches cover `number_decimal` bounds, `money` JSON object shape, URL/link scheme validation, date format validation, single-line/multi-line blank text and single-line newline rejection, rating bounds, measurement non-negative value and supported-unit checks, `list.*` array shape/128-element cap, and per-element coercion for list values such as `list.number_integer`. Product reference values, including `list.product_reference`, are checked against staged/base/hydrated resource state rather than a fixed sentinel GID. Missing references return Shopify's `INVALID_VALUE` field path and `Value references non-existent resource ...` message without staging the mutation.

The fixture documents excluded product-owned metafield types instead of adding placeholders. Exclusions are types that require separate definition-backed or resource-specific setup outside this disposable product matrix: `id`, `list.id`, metaobject/mixed references, company/customer/file/page/article/order/product-taxonomy references, and their list variants. Metaobject-owned `id`, metaobject reference, and mixed reference field values are covered by the metaobject matrix.

### Standard metafield definition enablement

`standardMetafieldDefinitionEnable` stages a normalized metafield definition locally from the checked-in standard template catalog in `src/proxy/standard_metafield_definition_templates.json`. The catalog is captured from `standardMetafieldDefinitionTemplates(first: 250, excludeActivated: false)` on the 2025-01 conformance shop and supports both template `id` selectors and `namespace` / `key` selectors across the captured catalog.

Successful local enablement:

- creates a staged `MetafieldDefinition` record without sending the mutation to Shopify when no definition exists for the template's owner/namespace/key
- re-enabling an already-present template returns the existing definition id and merges only supplied update params into that staged record
- supports `id` or `namespace` / `key` template selection for the captured template catalog
- applies `ownerType`, selected `access`, selected `capabilities`, and `pin`
- derives standard-template validations and constraints from template metadata; Shopify product-attribute templates such as `shopify.material` stage a `list.metaobject_reference` definition with a synthetic `metaobject_definition_id` validation plus category constraints instead of a per-key hardcode
- rejects ineligible capability inputs before staging, using the same captured `INVALID_CAPABILITY` branch as definition create/update but with standard-enable field paths
- translates the public-hidden `useAsCollectionCondition` and `visibleToStorefrontApi` arguments into the corresponding capability/access records before staging; the older local-compatibility `useAsAdminFilter` argument remains runtime-test-backed because current public Admin GraphQL rejects it before resolver execution
- rejects matching existing unstructured metafields with `UNSTRUCTURED_ALREADY_EXISTS` unless `forceEnable: true` is provided; current public Admin GraphQL rejects `forceEnable` before resolver execution, so that override remains runtime-test-backed compatibility rather than live payload parity
- returns `INVALID_CAPABILITY` for ineligible capability input, including the hidden collection-condition argument on an ineligible type
- returns `INVALID` with the captured public-read-write access message when merchant read-only admin access is supplied for non-app-owned standard templates
- returns `INVALID` when any explicit access controls are supplied while enabling a reserved `shopify` standard template
- on first-time enable with `pin: true`, uses the same local pin validation as definition create/pin so constrained templates and owner-type cap violations return `createdDefinition: null` before staging
- on re-enable with `pin: true`, suppresses the ordinary owner-type pin-cap error and updates the existing definition in place; the 2026-04 capture records `pinnedPosition: 21` after 20 disposable pinned definitions
- when pin validation passes, assigns the next owner-type pinned position after any existing pinned definitions, matching the local pinning/create rule instead of reusing position `1`
- returns a Shopify-like `createdDefinition` payload
- makes downstream `metafieldDefinition(identifier:)` and `metafieldDefinitions(...)` reads observe the staged definition

`fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/standard-metafield-definition-enable-validation.json` captures standard metafield definition enablement validation:

- no `id` and no `namespace` / `key` returns `createdDefinition: null` with `TEMPLATE_NOT_FOUND`
- an unknown template ID returns `field: ["id"]`, `TEMPLATE_NOT_FOUND`
- an unknown namespace/key selector returns `field: null`, `TEMPLATE_NOT_FOUND`
- template ID `1` with incompatible owner type `CUSTOMER` returns the same invalid-template-ID branch

That fixture scope is not a rule against live success captures. Normal supported proxy runtime handling must not send this mutation to Shopify, but explicit conformance recording may create and clean up real standard definitions in a disposable test shop, and `__meta/commit` replay should let the queued raw mutation create its Shopify-side schema effect.

The public Admin GraphQL 2026-04 introspection result on the current conformance shop lists `ownerType`, `id`, `namespace`, `key`, `pin`, `capabilities`, and `access` on `standardMetafieldDefinitionEnable`. Live execution still accepts hidden `visibleToStorefrontApi` and `useAsCollectionCondition` arguments, while `forceEnable` and `useAsAdminFilter` are rejected by public schema validation before resolver execution. `config/parity-specs/metafields/standard-metafield-definition-enable-error-branches.json` captures the live capability, access, visible-storefront, and hidden collection-condition branches; runtime tests cover the local-only compatibility branches for `forceEnable` and `useAsAdminFilter`.

`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/standard-metafield-definition-enable-reenable-idempotent.json` captures successful re-enable behavior for template `1` (`descriptors` / `subtitle`): enabling, re-enabling, and re-enabling with `pin: true` all return the same definition id. After the capture creates 20 disposable pinned definitions, `pin: true` re-enable still returns empty `userErrors` and assigns pinned position `21`; a downstream `metafieldDefinition(id:)` read by the original id resolves the same definition.

`config/parity-specs/metafields/standard-metafield-definition-enable-material.json` captures successful enablement for the PRODUCT `shopify.material` template. The strict target compares the mutation payload shape, access defaults, `list.metaobject_reference` type, `metaobject_definition_id` validation presence, category constraint key, and the first five category constraint values while carving out live-store definition IDs.

### Metafield definition pinning

The product-owner pinning slice supports local staging for existing normalized definition records:

- `metafieldDefinitionPin(definitionId:)`
- `metafieldDefinitionPin(identifier:)`
- `metafieldDefinitionUnpin(definitionId:)`
- `metafieldDefinitionUnpin(identifier:)`

Captured live behavior shows pinning an unpinned product definition, or creating a product definition with `pin: true`, assigns the next owner-type pinned position after the highest existing product definition position. Pinned definition catalogs sorted with `sortKey: PINNED_POSITION` return higher pinned positions first. Pinning an already-pinned definition returns `pinnedDefinition: null`, `field: null`, message `Definition already pinned.`, and code `ALREADY_PINNED`. Unpinning clears the target definition's `pinnedPosition` and compacts any higher pinned positions down by one, so downstream `metafieldDefinition` detail reads plus `metafieldDefinitions(... pinnedStatus: PINNED|UNPINNED)` catalogs reflect the staged change. Unpinning an unpinned definition returns `unpinnedDefinition: null`, `field: null`, a definition-id-specific message, and code `NOT_PINNED`.

The current product-owner pin cap for ordinary pinning is 20 pinned definitions. The 21st pin returns `pinnedDefinition: null` with `field: null`, message `Limit of 20 pinned definitions.`, and code `PINNED_LIMIT_REACHED`. Constrained definitions, represented by populated `constraints.key` or `constraints.values`, cannot be pinned and return `pinnedDefinition: null` with code `UNSUPPORTED_PINNING`.

The 2026-04 create-with-pin guard capture records the corresponding create-time branches: after 20 product definitions have been created with `pin: true`, the next pinned create returns `createdDefinition: null`, `field: ["definition"]`, message `Limit of 20 pinned definitions.`, and code `PINNED_LIMIT_REACHED`; constrained create with `pin: true` returns `createdDefinition: null`, `field: ["definition"]`, and code `UNSUPPORTED_PINNING`. A constrained standard template enable with `pin: true` returns `createdDefinition: null`, `field: null`, and code `UNSUPPORTED_PINNING`.

The local implementation intentionally covers pin/unpin for definitions already present in normalized snapshot, hydrated state, or staged lifecycle state. In LiveHybrid, a cold pin/unpin first hydrates the product-owner definition catalog through `upstream_query.fetch_sync`, then stages only the pin or unpin effect locally; parity cassettes provide that read deterministically. It does not create missing definitions through pin/unpin when no upstream definition can be hydrated. Definitions marked `appConfigManaged` reject pin and unpin with `APP_CONFIG_MANAGED`; full app-config lifecycle discovery remains out of scope until public or fixture-backed metadata can populate that flag automatically.

### Evidence

- `config/parity-specs/metafields/metafield-definition-create-input-validation.json`
- `config/parity-specs/metafields/standard-metafield-definition-enable-error-branches.json`
- `config/parity-specs/metafields/metafield-definition-create-with-pin-guards.json`
- `config/parity-specs/metafields/metafield-definition-capability-eligibility.json`
- `config/parity-specs/metafields/metafield-definition-update-constraints.json`
- `config/parity-specs/metafields/metafield-definition-pinning-parity.json`
- `config/parity-specs/metafields/metafield-definition-pin-limit-and-constraint-guard.json`
- `config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json`
- `config/parity-specs/metafields/metafield-definition-owner-scoped-duplicates.json`
- `config/parity-specs/metafields/metafield-definition-non-product-owner-types.json`
- `config/parity-specs/metafields/metafield-definition-non-product-metafields.json`
- `config/parity-specs/metafields/metafields-set-validation-gaps.json`
- `config/parity-specs/metafields/metafieldsSet-custom-namespace-typed-keys.json`
- `config/parity-specs/metafields/metafields-set-input-validation.json`
- `config/parity-specs/metafield-definitions/access-validation.json`
- `config/parity-specs/metafield-definitions/validation-affects-values.json`
- `config/parity-specs/metafield-definitions/metafield-delete-not-found.json`
- `config/parity-specs/metafield-definitions/metafields-set-delete-app-namespace-resolution.json`
- `config/parity-specs/products/metafieldsSet-*.json`
- `config/parity-specs/products/metafieldsDelete-parity-plan.json`
- `corepack pnpm conformance:capture -- --run metafield-definition-pinning`
- `corepack pnpm conformance:capture -- --run metafield-definition-create-with-pin-guards`
- `corepack pnpm conformance:capture -- --run metafield-definition-lifecycle`
- `corepack pnpm conformance:capture -- --run metafield-definition-non-product-owner-types`
- `corepack pnpm conformance:capture -- --run metafield-definition-non-product-metafields`
- `corepack pnpm conformance:capture -- --run metafield-definition-validation-affects-values`
- `corepack pnpm conformance:capture -- --run metafields-delete-not-found`
- `corepack pnpm conformance:capture -- --run metafields-parity-provenance-replacements`
- `corepack pnpm conformance:capture -- --run metafields-custom-namespace-typed-keys`
- `corepack pnpm conformance:capture -- --run metafields-set-delete-app-namespace-resolution`

### Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`

### Unsupported and registry-only boundaries

- `standardMetafieldDefinitionTemplates` remains registry-only declaration coverage; `standardMetafieldDefinitionEnable` consumes the checked-in captured catalog and models a bounded template slice, but the catalog query root itself should not be treated as locally supported until it has executable read behavior and fixture-backed shape evidence.
- Product, product variant, collection, customer, order, and company are the current fixture-backed shared `metafieldsSet` owner surface for definition-backed set/read success paths. PAGE, LOCATION, MARKET, and ARTICLE owner type payloads are captured for shared `metafieldsSet`; additional owner-family read-after-set behavior still needs capture-backed evidence before being claimed beyond definition lifecycle staging.
- Definition lifecycle parity covers product-owner behavior plus non-product owner create/update/delete/read evidence. App-owned definitions, owner-specific access/capability quirks, non-product delete cascade behavior, and non-product owner families outside CUSTOMER/ORDER/COMPANY still need fresh conformance before support expands beyond normalized definition records.
- CAS/userError coverage for `metafieldsSet` is product-owned fixture evidence. Reuse the atomic validation and downstream-read expectations, but do not assume other owner families have identical validation branches without capture.

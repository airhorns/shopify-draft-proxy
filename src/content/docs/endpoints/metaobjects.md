---
title: 'Metaobjects Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Metaobjects Endpoint Group.'
---

The metaobjects group covers Shopify Admin GraphQL custom data roots. Runtime support now models definition reads/lifecycle mutations plus the core entry row lifecycle locally.

## Current support and limitations

### Supported definition read roots

Supported definition reads resolve normalized definition state in snapshot and live-hybrid modes:

- `metaobjectDefinition(id:)`
- `metaobjectDefinitionByType(type:)`
- `metaobjectDefinitions(first:, after:, last:, before:, reverse:)`

The supported fields are limited to the captured 2026-04 definition payload:

- definition identity and display metadata: `id`, `type`, `name`, `description`, `displayNameKey`
- `access.admin`, `access.storefront`, and `access.customerAccount`
- `capabilities.publishable.enabled`, `translatable.enabled`, `renderable.enabled`, and `onlineStore.enabled`
- ordered `fieldDefinitions` with `key`, `name`, `description`, `required`, `type.name`, `type.category`, and `validations`
- `hasThumbnailField`, `metaobjectsCount`, and `standardTemplate.type` / `standardTemplate.name`

Snapshot mode reads these roots from the normalized `metaobjectDefinitions` state bucket and returns `null` for absent singular ID/type lookups. Empty catalogs return non-null connections with empty `edges` / `nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.

Live-hybrid mode uses cassette-backed passthrough for cold definition reads. When local staged, deleted, or hydrated definition state exists, the proxy serves definitions from local state so supported mutations preserve read-after-write behavior; when no local definition exists, upstream no-data/null responses are returned unchanged rather than replaced with fabricated definitions.

Local catalog cursors use the proxy's stable `cursor:<definition gid>` form. Shopify's captured live catalog cursors are opaque and should not be treated as client-visible semantics.

### Supported entry read roots

Supported entry reads resolve normalized metaobject entry state:

- `metaobject`
- `metaobjectByHandle`
- `metaobjects`

The supported entry field slice is based on the Admin GraphQL 2026-04 capture:

- entry identity and display metadata: `id`, `handle`, `type`, `displayName`, `createdAt`, and `updatedAt`
- entry `capabilities.publishable.status` and nullable `capabilities.onlineStore.templateSuffix`
- ordered `fields` with `key`, `type`, `value`, `jsonValue`, and the captured field-definition reference (`key`, `name`, `required`, `type.name`, `type.category`)
- `field(key:)`, including `null` for unknown field keys while preserving aliases
- `definition` when the matching normalized definition is present
- `MetaobjectField.reference` for `metaobject_reference` fields and `MetaobjectField.references` for `list.metaobject_reference` fields
- `referencedBy` connections for reverse relationships created by modeled metaobject reference fields

Snapshot mode reads entries from normalized `metaobjects` state and returns `null` for absent ID or `(type, handle)` lookups. Empty or absent type catalogs return non-null empty connections with empty `edges` / `nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.

`metaobjects(type:, first:, after:, before:, last:, reverse:, sortKey:, query:)` is type-scoped and never invents entries outside normalized state. Local catalog cursors use stable `cursor:<metaobject gid>` values. Supported local sort keys are `id`, `type`, `updated_at`, and `display_name`; `reverse` flips the sorted list before cursor windowing. Query filtering supports general text search plus documented field-value filters such as `fields.title:Alpha` against normalized field `value` / `jsonValue` data.

Live-hybrid mode uses cassette-backed passthrough for cold entry reads. Once local staged, deleted, or hydrated entry state exists, reads stay local so supported mutations preserve read-after-write and read-after-delete behavior; when no local entry exists, upstream no-data/null responses are returned unchanged.

### Reference relationship behavior

Metaobject field relationships are modeled for the fixture-backed local subset. The live 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-reference-lifecycle.json`, recorded by `corepack pnpm conformance:capture-metaobject-references`, confirms these shapes:

- `metaobject_reference` field definitions accept a `metaobject_definition_id` validation that points at the target definition.
- A single-reference field serializes `value` and `jsonValue` as the referenced metaobject GID, returns a selected `reference` object, and returns `references: null`.
- A list-reference field serializes `value` as Shopify's JSON-encoded ID list, `jsonValue` as an array of IDs, returns `reference: null`, and returns a `references` connection of referenced metaobjects.
- Target metaobjects expose `referencedBy` as a `MetafieldRelationConnection`; relation nodes include the parent field `key`, field definition `name`, parent type as `namespace`, and the parent metaobject as `referencer`.

The local model derives relationships from effective staged/snapshot metaobject field values at read time. Create, update, upsert, delete, and schema projection therefore affect downstream `reference`, `references`, and `referencedBy` reads without runtime Shopify writes. When no fields reference a target metaobject, `referencedBy` remains a Shopify-like empty connection.

Reference connection cursors are intentionally stable synthetic cursor values in local mode. Cold live-hybrid reference reads passthrough verbatim, so their cassette parity specs no longer carry cursor expected-difference rules.

### Supported definition mutation roots

These Admin GraphQL 2026-04 definition mutation roots stage locally:

- `metaobjectDefinitionCreate(definition:)`
- `metaobjectDefinitionUpdate(id:, definition:, resetFieldOrder:)`
- `metaobjectDefinitionDelete(id:)`
- captured-safe branches of `standardMetaobjectDefinitionEnable(type:)`

Supported definition mutations never proxy to Shopify at runtime. They append the original GraphQL request body to the meta mutation log for later `POST /__meta/commit` replay, stage changes in the normalized `metaobjectDefinitions` state bucket, and make downstream `metaobjectDefinition`, `metaobjectDefinitionByType`, and `metaobjectDefinitions` reads observe the effective staged schema immediately.

Create support models the captured merchant-owned definition shape:

- `type`, `name`, `description`, and `displayNameKey`
- default merchant access `admin: PUBLIC_READ_WRITE`, `storefront: NONE`, and `customerAccount: NONE`
- `capabilities.publishable`, `translatable`, `renderable`, and `onlineStore` with false defaults when omitted
- ordered field definitions with `key`, `name`, `description`, `required`, scalar type name/category, and validations
- `metaobjectsCount: 0`, `hasThumbnailField: false`, and `standardTemplate: null`

Create and update persist `access.customerAccount` for the public 2026-04 `MetaobjectCustomerAccountAccess` enum values `NONE` and `READ`; downstream definition reads project the effective value after local writes. Invalid literal values are rejected by GraphQL enum coercion before resolver side effects, matching the captured Shopify top-level error shape.

Captured guardrail: merchant-owned create input that specifies `access.admin` returns a local `ADMIN_ACCESS_INPUT_NOT_ALLOWED` userError with Shopify's captured message instead of staging or proxying.

Create and update now normalize definition types through Shopify's app-reserved namespace rules before storage and duplicate checks. `$app:<rest>` resolves with the request-owned `x-shopify-draft-proxy-api-client-id` value (`app--347082227713--<rest>` in the captured conformance shop) and the resolved type is lowercased before downstream reads and uniqueness checks. If that request identity is absent, local create validation returns an explicit authorization user error instead of assuming the captured conformance app id. Definition create/update validation returns Shopify-like `BLANK` for blank names, `TOO_LONG` for names/descriptions over 255 characters, `TOO_SHORT`/`TOO_LONG` for type length guardrails, and `INVALID` for type character-set guardrails. Public Admin GraphQL 2026-04 does not expose `type` on `MetaobjectDefinitionUpdateInput`, so live parity evidence covers create type validation while local runtime tests keep the update type guardrail executable for locally parsed update inputs. Create/update field-definition keys are locally guarded to 2-64 ASCII alphanumeric characters, underscores, and dashes, including uppercase letters. Update `fieldDefinitions[].create` operations also reject Shopify-reserved field keys, duplicate create keys in the same input, and post-operation field counts above the definition limit before staging; `displayNameKey` is resolved against the field set after create/update/delete operations.

Field-definition `type` input is allowlisted against the supported custom-data definition type set before any local definition is staged. Captured create parity accepts current merchant-owned public types such as `jurisdiction`, `list.jurisdiction`, and `product_taxonomy_disclosure_reference`, rejects standard-definition-only `disclosure_reference` / `list.disclosure_reference` values with Shopify's captured `INVALID` userError, and rejects unknown values such as `garbage_type` or unsupported metaobject list values such as `list.boolean` with `INCLUSION` at `field: ["definition", "fieldDefinitions", "<index>"]`, the field key in `elementKey`, and Shopify's `Type name <type> is not a valid type. Valid types are: ...` message. The allowlist intentionally excludes metaobject-definition-incompatible list types including `list.boolean` and `list.multi_line_text_field`. Update field-creation uses the same allowlist; existing field type changes are outside the supported public 2026-04 update surface and the local handler rejects them rather than silently mutating the stored field type.

Definition limit handling uses Shopify's default entitlement caps where the proxy cannot know shop-specific overrides locally. Merchant-owned definition creates are capped at 128 non-standard-template definitions; app-owned definitions are capped at 128 per resolved API client. Shop Pay's 256-definition override and custom entitlement plans are not modeled, so the proxy prefers conservative over-rejection to silently staging writes that would fail at commit. Definition create/update also rejects post-operation field sets with more than 40 `adminFilterable` field definitions, returning Shopify's `INVALID` userError at `field: ["definition", "fieldDefinitions"]`.

Update support stages scalar definition changes, access/capability merges, field definition create/update/delete operations, and `resetFieldOrder` ordering. Field definition updates preserve existing values for omitted fields; created fields append unless `resetFieldOrder` is set, in which case fields touched by the update input lead the resulting order and untouched fields follow in their previous relative order.

Capability changes are validated as one atomic update set before any definition changes are staged. Disabling `publishable` rejects when local rows of the definition type still have `DRAFT` publishable status. Disabling `onlineStore` conservatively rejects when any local row of the type exists because route and redirect dependencies are upstream-only state the proxy does not model. Disabling `renderable` conservatively rejects when rows exist because the local definition record does not persist Shopify's SEO field-reference dependency graph. Disabling `translatable` rejects when local base or staged translations still exist for rows of the type. If any of these guardrails fires, the mutation returns capability-scoped `INVALID` userErrors and leaves all other scalar, access, capability, and field-definition changes from the same input unpersisted.

Enabling `renderable` with `data.metaTitleKey` or `data.metaDescriptionKey` validates the referenced field definitions before staging. Missing field keys return Shopify's `INVALID` message at `field: ["definition", "capabilities", "renderable"]`; non-text field references return `FIELD_TYPE_INVALID` with Shopify's captured renderable capability message. Text-compatible metaobject field types are `single_line_text_field`, `multi_line_text_field`, and `rich_text_field`.

Online-store capability data retains `data.urlHandle` and the current `data.createRedirects` input in local definition state. When an effective definition's `onlineStore.data.urlHandle` changes with `createRedirects: true`, the proxy stages one local `UrlRedirect` for each effective row of that definition type whose publishable status is `ACTIVE`. Captured Admin GraphQL 2026-04 behavior creates `/pages/<old-url-handle>/<row-handle>` to `/pages/<new-url-handle>/<row-handle>` redirects; the local staged redirects use that `/pages` path shape and are visible through `urlRedirects(query: "path:/pages/...")` and `urlRedirect(id:)`. `createRedirects: false`, unchanged handles, disabled online-store capability, and non-`ACTIVE` rows do not stage definition-level redirects.

Definition reads project `capabilities.onlineStore.data.canCreateRedirects` for local and staged definitions when the online-store capability is enabled. Shopify also checks shop redirect quota, but the local model uses the captured published-entry constraint conservatively: `canCreateRedirects` is `true` when the effective definition has no more than 1000 `ACTIVE` published rows, and `false` above that cap. Disabled online-store capabilities continue to serialize `data: null`.

Create rejects reserved definition types before staging. If `implementStandardTemplate` is omitted or false, both checked-in standard-template types such as `shopify--qa-pair` and unresolved `shopify--*` namespace types return the live-captured public Admin GraphQL 2026-04 shape: a single `NOT_AUTHORIZED` userError at `field: ["definition"]` with message `Not authorized. This type is reserved for use by another application.` The internal service names this as reserved-name handling, but the public 2026-04 GraphQL payload does not expose `RESERVED_NAME` for the captured shop.

Update rejects standard and Shopify-reserved definitions before any scalar, capability, access, or field-definition operation runs. A definition is treated as immutable when its resolved type is present in the checked-in standard template catalog, when its local `standardTemplate` metadata is populated, or when its resolved namespace uses Shopify's reserved `shopify--` prefix. These branches return a single `IMMUTABLE` userError at `field: ["definition"]` with message `Standard metaobject definitions can't be updated`, and downstream reads continue to show the original definition. Definitions linked to product options are represented when local state can connect `productOptionsCreate.linkedMetafield` to a PRODUCT metafield definition whose validations include `metaobject_definition_id`; linked option value IDs are retained from `linkedMetafield.values` / `linkedMetafieldValue`, and hydrated product options retain upstream `linkedMetafield` metadata. For those linked definitions, changing `displayNameKey` returns Shopify's captured `IMMUTABLE` userError at `field: ["definition", "displayNameKey"]` with message `Cannot change display name field when metaobject is used in product options`, and leaves the definition unchanged.

When an effective definition changes, downstream row reads project existing row values through the current effective definition instead of returning stale field metadata. Existing row values whose field definitions still exist remain visible in the updated definition order, removed field definitions are omitted from `fields` and return `null` from `field(key:)`, changed field types update the serialized `type` / field-definition reference, and `displayName` is recomputed from the current `displayNameKey`. Rows that predate a newly required field remain readable; subsequent create/update/upsert requests are validated against the current effective definition, so missing required fields and writes to removed field keys return local userErrors.

Delete support stages deletion for definitions regardless of effective `metaobjectsCount` unless explicit local or hydrated metadata marks the definition as protected. Definitions marked `appConfigManaged` return `APP_CONFIG_MANAGED` at `field: ["id"]` with message `App-managed metaobject definitions cannot be deleted by other apps.` Definitions with both `standardTemplateId` and `standardTemplateDependentOnApp` return `STANDARD_METAOBJECT_DEFINITION_DEPENDENT_ON_APP` at `field: ["id"]` with message `Standard metaobject definition is in use by an installed app.` In both rejected branches, no cascade is staged and downstream `metaobjectDefinition(id:)` / `metaobjectDefinitionByType(type:)` reads keep returning the definition. Public Admin GraphQL 2026-04 does not expose `app_config_managed?`, `standard_template_id`, or `dependent_on_app?` fields on `MetaobjectDefinition` or `StandardMetaobjectDefinitionTemplate`; the live lifecycle-invariants fixture records that the current conformance shop has no safe app-managed or dependent-standard delete candidate, so these delete guard branches remain runtime-test-backed until an eligible shop/app credential exists.

For unprotected definitions, the local cascade records a tombstone for the definition and for every effective metaobject of that definition type, then downstream `metaobjectDefinition`, `metaobjectDefinitionByType`, `metaobject`, `metaobjectByHandle`, and `metaobjects(type:)` reads observe Shopify-like null or empty results. The mutation response returns the input definition GID as `deletedId`; unknown or stale definition ids continue to return `RECORD_NOT_FOUND`.

`standardMetaobjectDefinitionEnable` stages definitions from the checked-in standard template catalog captured from the 2026-04 conformance shop. Known templates stage a standard definition locally with captured `standardTemplate`, access, capabilities, and field-definition metadata; unknown template types return Shopify's `RECORD_NOT_FOUND` userError at `field: ["type"]` with message `Record not found`. Re-enabling an already staged standard definition mirrors the captured Shopify branch by returning the existing definition with no userErrors.

### Supported entry mutation roots

The core Admin GraphQL 2026-04 metaobject row lifecycle roots stage locally:

- `metaobjectCreate`
- `metaobjectUpdate`
- `metaobjectUpsert`
- `metaobjectDelete`
- `metaobjectBulkDelete`

Supported entry mutations never proxy to Shopify at runtime. They append the original GraphQL request body to the meta mutation log for later `POST /__meta/commit` replay, stage changes in the normalized `metaobjects` state bucket or `deletedMetaobjectIds` tombstone map, and make downstream `metaobject`, `metaobjectByHandle`, and `metaobjects` reads observe staged row writes immediately.

Create support requires an existing effective definition. In live-hybrid mode, cold creates first hydrate the matching upstream definition by type through `MetaobjectDefinitionHydrateByType`, then stage the create locally; snapshot mode remains local-only. The create path stages a synthetic `Metaobject` ID, explicit or generated handle, selected field values projected through the effective definition's ordered field definitions, null-valued placeholders for omitted field definitions, `displayName` from the definition's nonblank `displayNameKey` value or from the handle fallback, default/selected publishable status, online-store capability data (`templateSuffix` is `null` when omitted and preserved exactly when supplied), and an incremented effective definition `metaobjectsCount`. Captured 2026-04 behavior defaults omitted publishable status to `DRAFT` while the definition's publishable capability is enabled. Repeated `fields[].key` inputs return `DUPLICATE_FIELD_INPUT` at the second occurrence and do not stage a row. Non-empty explicit handles that fail Shopify's handle format or 255-character length guards return `INVALID` or `TOO_LONG` at `field: ["metaobject", "handle"]`, leave `metaobject: null`, and do not stage a row. If a requested create handle is already taken, Shopify auto-suffixes the handle instead of returning a duplicate-handle userError; the local model mirrors that for create while preserving duplicate-handle errors for update.

When `metaobjectCreate` omits `handle`, captured 2026-04 behavior generates a handle shaped as `<type-dasherized>-<8 lowercase alphanumeric>` regardless of other field values. If the definition has no `displayNameKey`, or the keyed value is blank, fallback `displayName` is derived from the handle: generated random-shaped handles render as the titleized base plus `#SUFFIX`, while explicit handles render as the titleized submitted handle. Explicit mixed-case handles are lowercased for storage on the public Admin API path, but the no-conflict fallback display name is titleized from the submitted handle; a subsequent lower-case duplicate is compared case-insensitively and auto-suffixed. `metaobjectUpsert` never auto-generates the handle because its `handle` argument is required, but its create and update branches use the same handle-derived `displayName` fallback when no nonblank display field value is available.

`metaobjectCreate` and the create branch of `metaobjectUpsert` reject non-standard definitions whose cached effective `metaobjectsCount` is already at the default 1,000,000 per-type cap. The benchmark override of 16,777,216 and custom entitlement overrides are not modeled. Standard-template definitions skip this cap. Cold live-hybrid creates hydrate the definition once and then use the stored count for subsequent local staging, so the proxy does not ask Shopify for a fresh count for every staged row.

Entry capability input is rejected unless the effective definition has the corresponding capability enabled. `metaobjectCreate`, `metaobjectUpdate`, and `metaobjectUpsert` return `CAPABILITY_NOT_ENABLED` at `field: ["metaobject", "capabilities", capabilityKey]` with message `Capability is not enabled: publishable` or `Capability is not enabled: online_store` for disabled `publishable` or `onlineStore` input, and the rejected mutation does not stage partial row changes.

Update support resolves the effective row by ID, patches selected fields while preserving omitted field values, supports handle changes with same-type uniqueness checks, merges publishable/online-store capability input, and keeps the row visible under the new handle while the old handle returns `null`. For online-store-enabled definitions, `metaobjectUpdate` and the update branch of `metaobjectUpsert` preserve the stored `templateSuffix` on unrelated edits and update it only when input supplies `capabilities.onlineStore.templateSuffix`. `displayName` is recomputed only when the input includes the definition's display field key and that field value changes; when the definition has no `displayNameKey`, handle changes derive the next display name from the handle. Non-display-field updates preserve the stored display name in the mutation payload and downstream reads. Required field validation mirrors the captured `OBJECT_FIELD_REQUIRED` shape, duplicate `fields[].key` inputs return `DUPLICATE_FIELD_INPUT` at the second occurrence and leave required duplicate keys blank, updates to removed fields return the captured `UNDEFINED_OBJECT_FIELD` shape, and missing definition types return `UNDEFINED_OBJECT_TYPE`. Field value validation runs for create, update, upsert, and merged existing update values against the effective definition. It covers numeric/date/JSON/rating/color/URL/text constraints, money/link/language/custom-ID uniqueness, classic and extended measurements, product/variant/collection/customer/company/metaobject/file/page/order/article/product-taxonomy references, and `mixed_reference` definition constraints from local state. Invalid field values return Shopify-like `INVALID_VALUE` at `field: ["metaobject", "fields", "<index>"]` for submitted fields; duplicate `id` type values return `TAKEN`.

`metaobjectUpdate` and the update branch of `metaobjectUpsert` run a local `DISPLAY_NAME_CONFLICT` guard when the effective display name changes. The guard checks other effective metaobjects of the same type and only rejects a duplicate display name when the competing row is referenced by a product option through a `productOptionsCreate.linkedMetafield` association recorded on the metaobject definition. Captured public Admin GraphQL 2026-04 evidence covers display-field changes and returns `DISPLAY_NAME_CONFLICT` at `field: ["metaobject", "fields", "<index>"]` with message `The display name you have chosen is already in use as an option value. Choose a different name to avoid conflicts.` Rejected rows are not staged. `metaobjectCreate` keeps Shopify's create behavior and continues to auto-suffix duplicate handles without running this update/upsert validator. Handle-derived display-name changes remain runtime-test-backed from internal validator behavior; a public 2026-04 live probe accepted the tested `one-a`/`one_a` handle-derived collision shape rather than returning `DISPLAY_NAME_CONFLICT`.

`metaobjectUpdate` reads `redirectNewHandle` from `MetaobjectUpdateInput`. Captured 2026-04 behavior rejects a top-level mutation argument, so checked-in parity uses the nested input shape. When a handle changes with `redirectNewHandle: true`, the effective definition has both `onlineStore` and `renderable` enabled, and the row has `capabilities.onlineStore`, the proxy stages a local `UrlRedirect` from `/pages/<onlineStore.urlHandle>/<old-handle>` to `/pages/<onlineStore.urlHandle>/<new-handle>`. Definitions created locally retain `onlineStore.data.urlHandle` in state for that path computation; hydrated definitions without captured URL-handle data fall back to the display key. `redirectNewHandle: false` and non-renderable definitions are silent no-ops for redirects, matching the live capture. The staged redirect is visible through `urlRedirects(query: "path:/...")` and `urlRedirect(id:)`; Admin API 2026-04 `UrlRedirect` exposes `id`, `path`, and `target` for this read surface.

Upsert support resolves by `MetaobjectHandleInput`. Existing rows are updated in place; missing rows are created against the effective definition with the requested handle. Non-empty invalid or over-255 locator handles return the same handle validation codes at `field: ["handle", "handle"]` before staging, while blank locator handles keep Shopify's captured generated-handle success behavior on the create branch. Definition misses and missing handle data return local userErrors rather than proxying.

Delete support stages a tombstone for base or staged rows, returns the selected `deletedId` on success, decrements the effective definition `metaobjectsCount`, and hides the row from ID, handle, and catalog reads. In live-hybrid mode, cold deletes hydrate the upstream row before local tombstone staging. Missing or stale/deleted rows return Shopify's 2026-04 `RECORD_NOT_FOUND` userError with `deletedId: null`.

Bulk delete support accepts the current 2026-04 `where.ids` and `where.type` branches, with the older local `ids` branch retained only for already-checked-in local replay evidence. It stages tombstones for found rows, preserves ordered `elementIndex` `RECORD_NOT_FOUND` userErrors for missing IDs, updates definition counts per type, and keeps all effects local. Empty `where.ids` and type selections that find a known definition with no rows return a no-work `Job` payload with no userErrors. Unknown `where.type` returns `RECORD_NOT_FOUND` on `where.type` with `job: null`. Supplying both `type` and `ids`, or neither selector, is modeled as Shopify's top-level `INVALID_FIELD_ARGUMENTS` validation error rather than a payload userError. `where.ids` selection is capped to the first 250 IDs with no truncation error. In live-hybrid mode, type-scoped bulk delete hydrates the upstream selected rows and definition through `MetaobjectBulkDeleteHydrateByType` before staging local tombstones. Live 2026-04 introspection shows `where` is the required argument and direct `ids` is not accepted by Shopify.

### Metaobject field value type matrix

Executable set/read parity covers 99 metaobject field value types in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json`, replayed by `config/parity-specs/metaobjects/custom-data-metaobject-field-type-matrix.json`.

The recorder splits the field set across three disposable definitions because Shopify caps a metaobject definition at 40 fields. It uses `custom_id` as the field key for Shopify's `id` type because `id` itself is reserved as a metaobject field key. The matrix covers scalar custom-data values, measurement values, supported lists, product/variant/collection references, `metaobject_reference`, `list.metaobject_reference`, `mixed_reference`, and `list.mixed_reference`. Shopify rejected `list.boolean` and `list.multi_line_text_field` for this metaobject definition path, so those are not represented as working metaobject field fixtures.

Metaobject field-definition `type.category` derives `list.*` categories from the list element type. Scalar list fields such as `list.single_line_text_field`, `list.number_integer`, and `list.date` serialize as `TEXT`, `NUMBER`, and `DATE_TIME`; only list reference element types serialize as `REFERENCE`.

The local entry model shares the metafield custom-data normalization helper for field `value` / `jsonValue` projection. Metaobject `displayName` for measurement display keys follows the captured Shopify behavior by formatting the measurement `jsonValue` form rather than the stored canonical `value` string.

Strict create/update userError parity for invalid metaobject field values lives in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-field-validation-matrix.json`, replayed by `config/parity-specs/metaobjects/metaobject-field-validation-matrix.json`. The matrix covers scalar numbers, boolean create/update behavior, date/date-time, classic measurement, rating, color, URL, money, link, power, text max, product/variant/collection/customer/company/metaobject/file/mixed references, and list variants for the original scalar/reference slice. Captured 2026-04 behavior is asymmetric for scalar boolean input: `metaobjectCreate` coerces `"hello"` to `true`, while `metaobjectUpdate` rejects the same value with `INVALID_VALUE`. Runtime route tests cover additional value-validation branches that are not strict parity targets in the 40-field live matrix, including link `allowed_domains`, `id` uniqueness, language, page/order/article/product-taxonomy references, and stale stored values revalidated after definition validation changes.

### Coverage boundaries

- Registry entries in this group are declared gaps unless they are marked implemented and have executable runtime tests, parity inventory, and documented field behavior.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance/runtime evidence, and documented field behavior. Definition reads, definition mutation roots, entry reads, and entry row mutation roots satisfy that bar for the supported slices documented above.
- Unsupported metaobjects mutations must not be registered as permanent passthrough support. The generic unknown-operation passthrough path can still handle unsupported runtime requests outside snapshot-only parity execution, but that is not a support commitment for any declared root.
- Do not add planned-only parity specs or request placeholders for this group. Add parity specs only after a captured Shopify interaction can run as evidence.
- The current metaobject reference value validator covers the modeled GID families and local `mixed_reference` definition constraints. Relationship-edge reads remain modeled only for metaobject-owned `metaobject_reference` and `list.metaobject_reference` fields. Do not infer support for metafield-backed references, file/page/order/article/product-taxonomy relationship edges, mixed-reference relationship edges, or broader cross-owner relationship traversal from value-validation evidence alone.

### Schema-change lifecycle behavior

The live 2026-04 schema-change fixture (`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-schema-change-lifecycle.json`) is replayed by `config/parity-specs/metaobjects/metaobject-schema-change-lifecycle.json`.

The captured update sequence creates a definition and rows, deletes a row before the schema edit, then updates the definition with `resetFieldOrder: true` inside `MetaobjectDefinitionUpdateInput`, an added required field, a removed field, display-name key change, validation change, and publishable capability disable. Shopify 2026-04 rejects `resetFieldOrder` as a top-level `metaobjectDefinitionUpdate` argument, and `MetaobjectFieldDefinitionUpdateInput` does not expose a `type` field, so the local model treats type changes as outside the captured supported update surface.

Rows created before the schema edit continue to resolve by ID and handle after the definition update. Missing newly required fields serialize as selected field objects with `value: null`, and `displayName` falls back to a titleized handle until the row is updated with the new display field. Immediate type catalog reads omit rows that fail the new required-field validation. After the row is updated with the new display field, it returns to the catalog.

Rows created after publishable capability is disabled serialize `capabilities.publishable: null`; singular ID/handle reads observe them immediately, while the captured immediate catalog read did not include the newly created post-disable row. The local catalog model preserves the captured distinction between rows that had an active publishable status before capability disable and rows created after publishable is disabled.

### Unsupported and validation-only boundaries

- Metaobject relationship edges are modeled only for metaobject-owned `metaobject_reference` and `list.metaobject_reference` fields. Broader owners, generic metafield-backed relations, and mixed/file/page/order/article/product-taxonomy reference edges need separate conformance evidence before support is widened.
- Broader bulk delete selection semantics still need additional live conformance before widening beyond the local ids/type branches. Captured evidence covers the `where.type` branch and confirms Shopify returns an async job while immediate downstream reads already hide selected rows and report the definition's `metaobjectsCount` as zero. Edge-case evidence covers empty `where.ids`, unknown `where.type`, known-empty `where.type`, and invalid combined selectors.
- Upsert support covers handle-scoped create/update behavior in the local model; additional conflict/userError branches should be expanded when captured.

### Empty and no-data expectations

- Singular entry and definition lookup misses should match Shopify null behavior once captured, including ID, type, and handle lookup branches.
- Connection roots should return Shopify-like empty `edges`, `nodes`, and `pageInfo` structures for known empty datasets instead of inventing records.
- Type-scoped entry reads must not synthesize arbitrary metaobjects when the snapshot or staged state lacks that type.
- Definition reads must not invent field definitions, capabilities, access settings, standard-template metadata, or associated entry counts without captured or staged state. When an observed entry record carries a nested `definition`, the proxy reuses that definition metadata; when it can only infer a minimal shell from entry field definitions, unknown definition access stays `null` rather than defaulting to writable access. The serializer only projects normalized definition records and returns Shopify-like null/empty structures when no record exists.

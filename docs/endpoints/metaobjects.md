# Metaobjects Endpoint Group

The metaobjects group covers Shopify Admin GraphQL custom data roots. Runtime support now models definition reads/lifecycle mutations plus the core entry row lifecycle locally.

HAR-131 is the source related issue for the metaobjects area.

## Current support and limitations

### Supported definition read roots

HAR-241 promotes the first metaobject runtime slice from registry-only coverage to executable snapshot and live-hybrid reads for normalized definition state:

- `metaobjectDefinition(id:)`
- `metaobjectDefinitionByType(type:)`
- `metaobjectDefinitions(first:, after:, last:, before:, reverse:)`

The supported fields are limited to the captured 2026-04 definition payload:

- definition identity and display metadata: `id`, `type`, `name`, `description`, `displayNameKey`
- `access.admin` and `access.storefront`
- `capabilities.publishable.enabled`, `translatable.enabled`, `renderable.enabled`, and `onlineStore.enabled`
- ordered `fieldDefinitions` with `key`, `name`, `description`, `required`, `type.name`, `type.category`, and `validations`
- `hasThumbnailField`, `metaobjectsCount`, and `standardTemplate.type` / `standardTemplate.name`

Snapshot mode reads these roots from the normalized `metaobjectDefinitions` state bucket and returns `null` for absent singular ID/type lookups. Empty catalogs return non-null connections with empty `edges` / `nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.

Live-hybrid mode uses cassette-backed passthrough for cold definition reads. When local staged, deleted, or hydrated definition state exists, the proxy serves definitions from local state so supported mutations preserve read-after-write behavior; when no local definition exists, upstream no-data/null responses are returned unchanged rather than replaced with fabricated definitions.

Local catalog cursors use the proxy's stable `cursor:<definition gid>` form. Shopify's captured live catalog cursors are opaque and should not be treated as client-visible semantics.

### Supported entry read roots

HAR-243 promotes normalized metaobject entry reads for:

- `metaobject`
- `metaobjectByHandle`
- `metaobjects`

The supported entry field slice is based on the HAR-240 Admin GraphQL 2026-04 capture:

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

HAR-384 promotes metaobject field relationships from documentation-only gap to modeled runtime behavior. The live 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-reference-lifecycle.json`, recorded by `corepack pnpm conformance:capture-metaobject-references`, confirms these shapes:

- `metaobject_reference` field definitions accept a `metaobject_definition_id` validation that points at the target definition.
- A single-reference field serializes `value` and `jsonValue` as the referenced metaobject GID, returns a selected `reference` object, and returns `references: null`.
- A list-reference field serializes `value` as Shopify's JSON-encoded ID list, `jsonValue` as an array of IDs, returns `reference: null`, and returns a `references` connection of referenced metaobjects.
- Target metaobjects expose `referencedBy` as a `MetafieldRelationConnection`; relation nodes include the parent field `key`, field definition `name`, parent type as `namespace`, and the parent metaobject as `referencer`.

The local model derives relationships from effective staged/snapshot metaobject field values at read time. Create, update, upsert, delete, and schema projection therefore affect downstream `reference`, `references`, and `referencedBy` reads without runtime Shopify writes. When no fields reference a target metaobject, `referencedBy` remains a Shopify-like empty connection.

Reference connection cursors are intentionally stable synthetic cursor values in local mode. Cold live-hybrid reference reads passthrough verbatim, so their cassette parity specs no longer carry cursor expected-difference rules.

### Supported definition mutation roots

HAR-242 adds local staging for these Admin GraphQL 2026-04 definition mutation roots:

- `metaobjectDefinitionCreate(definition:)`
- `metaobjectDefinitionUpdate(id:, definition:, resetFieldOrder:)`
- `metaobjectDefinitionDelete(id:)`
- captured-safe branches of `standardMetaobjectDefinitionEnable(type:)`

Supported definition mutations never proxy to Shopify at runtime. They append the original GraphQL request body to the meta mutation log for later `POST /__meta/commit` replay, stage changes in the normalized `metaobjectDefinitions` state bucket, and make downstream `metaobjectDefinition`, `metaobjectDefinitionByType`, and `metaobjectDefinitions` reads observe the effective staged schema immediately.

Create support models the captured merchant-owned definition shape:

- `type`, `name`, `description`, and `displayNameKey`
- default merchant access `admin: PUBLIC_READ_WRITE` and `storefront: NONE`
- `capabilities.publishable`, `translatable`, `renderable`, and `onlineStore` with false defaults when omitted
- ordered field definitions with `key`, `name`, `description`, `required`, scalar type name/category, and validations
- `metaobjectsCount: 0`, `hasThumbnailField: false`, and `standardTemplate: null`

Captured guardrail: merchant-owned create input that specifies `access.admin` returns a local `ADMIN_ACCESS_INPUT_NOT_ALLOWED` userError with Shopify's captured message instead of staging or proxying.

Update support stages scalar definition changes, access/capability merges, field definition create/update/delete operations, and `resetFieldOrder` ordering. Field definition updates preserve existing values for omitted fields; created fields append unless `resetFieldOrder` is set, in which case fields touched by the update input lead the resulting order and untouched fields follow in their previous relative order.

When an effective definition changes, downstream row reads project existing row values through the current effective definition instead of returning stale field metadata. Existing row values whose field definitions still exist remain visible in the updated definition order, removed field definitions are omitted from `fields` and return `null` from `field(key:)`, changed field types update the serialized `type` / field-definition reference, and `displayName` is recomputed from the current `displayNameKey`. Rows that predate a newly required field remain readable; subsequent create/update/upsert requests are validated against the current effective definition, so missing required fields and writes to removed field keys return local userErrors.

Delete support stages deletion for definitions whose effective `metaobjectsCount` is zero and hides deleted base/staged definitions from downstream reads through a staged tombstone. Definitions with associated entries return an explicit local `UNSUPPORTED` userError because entry records and cascade semantics are not modeled yet; this keeps the known branch local and visible instead of pretending Shopify's destructive cascade has been faithfully emulated.

`standardMetaobjectDefinitionEnable` is limited to the bounded local template catalog currently represented by runtime tests. Known templates stage a standard definition locally with `standardTemplate` metadata; unknown template types return `TEMPLATE_NOT_FOUND` locally.

### Supported entry mutation roots

HAR-244 adds local staging for the core Admin GraphQL 2026-04 metaobject row lifecycle roots:

- `metaobjectCreate`
- `metaobjectUpdate`
- `metaobjectUpsert`
- `metaobjectDelete`
- `metaobjectBulkDelete`

Supported entry mutations never proxy to Shopify at runtime. They append the original GraphQL request body to the meta mutation log for later `POST /__meta/commit` replay, stage changes in the normalized `metaobjects` state bucket or `deletedMetaobjectIds` tombstone map, and make downstream `metaobject`, `metaobjectByHandle`, and `metaobjects` reads observe staged row writes immediately.

Create support requires an existing effective definition. In live-hybrid mode, cold creates first hydrate the matching upstream definition by type through `MetaobjectDefinitionHydrateByType`, then stage the create locally; snapshot mode remains local-only. The create path stages a synthetic `Metaobject` ID, explicit or generated handle, selected field values projected through the effective definition's ordered field definitions, null-valued placeholders for omitted field definitions, `displayName` from the definition's `displayNameKey`, default/selected publishable status, nullable online-store capability shape, and an incremented effective definition `metaobjectsCount`. Captured 2026-04 behavior defaults omitted publishable status to `DRAFT` while the definition's publishable capability is enabled. If a requested create handle is already taken, Shopify auto-suffixes the handle instead of returning a duplicate-handle userError; the local model mirrors that for create while preserving duplicate-handle errors for update.

Update support resolves the effective row by ID, patches selected fields while preserving omitted field values, supports handle changes with same-type uniqueness checks, merges publishable/online-store capability input, updates `displayName`, and keeps the row visible under the new handle while the old handle returns `null`. Required field validation now mirrors the captured `OBJECT_FIELD_REQUIRED` shape, updates to removed fields return the captured `UNDEFINED_OBJECT_FIELD` shape, missing definition types return `UNDEFINED_OBJECT_TYPE`, and invalid values for the currently modeled scalar slice return `INVALID_VALUE` for captured `max` length and JSON parsing failures.

Upsert support resolves by `MetaobjectHandleInput`. Existing rows are updated in place; missing rows are created against the effective definition with the requested handle. Definition misses and missing handle data return local userErrors rather than proxying.

Delete support stages a tombstone for base or staged rows, returns the selected `deletedId` on success, decrements the effective definition `metaobjectsCount`, and hides the row from ID, handle, and catalog reads. In live-hybrid mode, cold deletes hydrate the upstream row before local tombstone staging. Missing or stale/deleted rows return Shopify's 2026-04 `RECORD_NOT_FOUND` userError with `deletedId: null`.

Bulk delete support accepts the current 2026-04 `where.ids` and `where.type` branches, with the older local `ids` branch retained only for already-checked-in local replay evidence. It stages tombstones for found rows, returns a completed local `Job` payload when at least one row is deleted, preserves ordered `elementIndex` `RECORD_NOT_FOUND` userErrors for missing IDs, updates definition counts per type, and keeps all effects local. In live-hybrid mode, type-scoped bulk delete hydrates the upstream selected rows and definition through `MetaobjectBulkDeleteHydrateByType` before staging local tombstones. Live 2026-04 introspection shows `where` is the required argument and direct `ids` is not accepted by Shopify.

### Metaobject field value type matrix

HAR-294 adds executable set/read parity for 99 metaobject field value types in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json`, replayed by `config/parity-specs/metaobjects/custom-data-metaobject-field-type-matrix.json`.

The recorder splits the field set across three disposable definitions because Shopify caps a metaobject definition at 40 fields. It uses `custom_id` as the field key for Shopify's `id` type because `id` itself is reserved as a metaobject field key. The matrix covers scalar custom-data values, measurement values, supported lists, product/variant/collection references, `metaobject_reference`, `list.metaobject_reference`, `mixed_reference`, and `list.mixed_reference`. Shopify rejected `list.boolean` and `list.multi_line_text_field` for this metaobject definition path, so those are not represented as working metaobject field fixtures.

The local entry model shares the metafield custom-data normalization helper for field `value` / `jsonValue` projection. Metaobject `displayName` for measurement display keys follows the captured Shopify behavior by formatting the measurement `jsonValue` form rather than the stored canonical `value` string.

### Coverage boundaries

- Registry entries in this group are declared gaps unless they are marked implemented and have executable runtime tests, parity inventory, and documented field behavior.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance/runtime evidence, and documented field behavior. HAR-241 satisfies that bar for definition reads; HAR-242 satisfies that bar for definition mutation roots; HAR-243 satisfies that bar for entry reads; HAR-244 satisfies that bar for entry row mutation roots.
- Unsupported metaobjects mutations must not be registered as permanent passthrough support. The generic unknown-operation passthrough path can still handle unsupported runtime requests outside snapshot-only parity execution, but that is not a support commitment for any declared root.
- Do not add planned-only parity specs or request placeholders for this group. Add parity specs only after a captured Shopify interaction can run as evidence.
- HAR-450 review note: the current metaobject reference model covers only metaobject-owned `metaobject_reference` and `list.metaobject_reference` fields. Do not infer support for metafield-backed references, `mixed_reference`, generic file/product/page references, or cross-owner relationship edges from this evidence.

### Schema-change lifecycle behavior

HAR-245's live 2026-04 schema-change fixture (`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-schema-change-lifecycle.json`) is replayed by `config/parity-specs/metaobjects/metaobject-schema-change-lifecycle.json`.

The captured update sequence creates a definition and rows, deletes a row before the schema edit, then updates the definition with `resetFieldOrder: true` inside `MetaobjectDefinitionUpdateInput`, an added required field, a removed field, display-name key change, validation change, and publishable capability disable. Shopify 2026-04 rejects `resetFieldOrder` as a top-level `metaobjectDefinitionUpdate` argument, and `MetaobjectFieldDefinitionUpdateInput` does not expose a `type` field, so the local model treats type changes as outside the captured supported update surface.

Rows created before the schema edit continue to resolve by ID and handle after the definition update. Missing newly required fields serialize as selected field objects with `value: null`, and `displayName` falls back to a titleized handle until the row is updated with the new display field. Immediate type catalog reads omit rows that fail the new required-field validation. After the row is updated with the new display field, it returns to the catalog.

Rows created after publishable capability is disabled serialize `capabilities.publishable: null`; singular ID/handle reads observe them immediately, while the captured immediate catalog read did not include the newly created post-disable row. The local catalog model preserves the captured distinction between rows that had an active publishable status before capability disable and rows created after publishable is disabled.

### Planned local-staging posture

- Definition delete support deliberately stops short of destructive associated-entry cascades. The local proxy returns an explicit `UNSUPPORTED` userError when effective `metaobjectsCount` is nonzero; future cascade behavior needs conformance-backed migration, deletion ordering, and downstream-read evidence before support widens.
- Metaobject relationship edges are modeled only for metaobject-owned `metaobject_reference` and `list.metaobject_reference` fields. Broader owners, generic metafield-backed relations, and `mixed_reference` need separate conformance evidence before support is widened.
- Broader bulk delete selection semantics still need additional live conformance before widening beyond the local ids/type branches. HAR-450 captures the `where.type` branch and confirms Shopify returns an async job while immediate downstream reads already hide selected rows and report the definition's `metaobjectsCount` as zero.
- Upsert support covers handle-scoped create/update behavior in the local model; additional conflict/userError branches should be expanded when captured.

### Empty and no-data expectations

- Singular entry and definition lookup misses should match Shopify null behavior once captured, including ID, type, and handle lookup branches.
- Connection roots should return Shopify-like empty `edges`, `nodes`, and `pageInfo` structures for known empty datasets instead of inventing records.
- Type-scoped entry reads must not synthesize arbitrary metaobjects when the snapshot or staged state lacks that type.
- Definition reads must not invent field definitions, capabilities, access settings, standard-template metadata, or associated entry counts without captured or staged state. HAR-241's serializer only projects normalized definition records and returns Shopify-like null/empty structures when no record exists.

### Conformance evidence still needed before widening support

- Capture associated-entry `metaobjectDefinitionDelete` cascade behavior before replacing the current local `UNSUPPORTED` guardrail for definitions with entries.
- Capture additional `metaobjectBulkDelete` selector branches and partial-result behavior before widening beyond the current `where.ids` / captured `where.type` local branches.
- Capture more `metaobjectUpdate` / `metaobjectUpsert` conflict and validation branches, especially field type families beyond the current scalar/JSON/reference slice.
- Expand `standardMetaobjectDefinitionEnable` success captures for additional standard templates before broadening the bounded local template catalog.
- Capture generic metafield-backed references, `mixed_reference`, and non-metaobject owner relationship edges before claiming broader reference support.
- Promote any new parity specs only after comparison targets can verify Shopify payload shape, userErrors, nullability, empty connections, cursor treatment, and downstream read-after-write or read-after-delete behavior.

## Historical and developer notes

### Captured read fixture slice

HAR-240 adds a live 2026-04 read fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobjects-read.json`, recorded by `corepack pnpm conformance:capture-metaobjects`.

The recorder captures no-data behavior before setup:

- `metaobjectDefinitionByType(type:)` returns `null` for an unknown type.
- `metaobjectDefinition(id:)` returns `null` for an unknown definition GID.
- `metaobjects(type:, first:)` returns a non-null empty connection for an unknown type, with empty `edges`/`nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.
- `metaobjectByHandle(handle:)` returns `null` for an unknown type/handle pair.
- `metaobject(id:)` returns `null` for an unknown entry GID.

The seeded branch creates one disposable merchant-owned metaobject definition and one entry, reads them, then deletes both. Definition reads cover catalog/detail/type lookup with `access`, `capabilities`, `displayNameKey`, ordered `fieldDefinitions`, `metaobjectsCount`, and connection cursors. Entry reads cover type catalog, ID lookup, handle lookup, `handle`, `type`, `displayName`, `updatedAt`, entry `capabilities`, ordered `fields`, and `field(key: "title")`.

HAR-351 promotes the HAR-241 definition-read subset into `config/parity-specs/metaobjects/metaobject-definitions-read.json` as a strict generic proxy-vs-recording parity scenario. Under the cassette runner, cold live-hybrid definition catalog, ID lookup, type lookup, and missing ID/type requests passthrough to the captured upstream responses verbatim. `gleam/test/parity_test.gleam` continues to cover aliases/fragments, live-hybrid overlay, field order, and no-runtime-live-access behavior.

HAR-243 adds `config/parity-specs/metaobjects/metaobjects-read.json` and `config/parity-requests/metaobjects/metaobjects-read.graphql` for the entry-read subset. Under the cassette runner, cold live-hybrid `metaobjects`, `metaobject`, and `metaobjectByHandle` reads passthrough to synthesized upstream cassettes assembled from the captured entry catalog/detail/handle responses, with strict comparison targets for seeded and no-data branches.

HAR-242 adds `config/parity-specs/metaobjects/metaobject-definition-lifecycle-local-staging.json`, backed by `fixtures/conformance/local-runtime/2026-04/metaobjects/metaobject-definition-draft-flow.json`, `config/parity-requests/metaobjects/metaobject-definition-*.graphql`, and `gleam/test/parity_test.gleam`. The convention-driven parity runner executes the create/update/delete/read-after-write and bounded standard-enable flow against the local proxy harness with strict JSON comparison targets. The runtime test also covers meta API log/state visibility, no runtime Shopify writes, the captured merchant-owned access.admin guardrail, and explicit unsupported handling for associated-entry delete cascades.

HAR-244 adds `config/parity-specs/metaobjects/metaobject-entry-lifecycle-local-staging.json` and `gleam/test/parity_test.gleam` for local entry row lifecycle staging. The test covers create/update/upsert/delete/bulk delete, downstream ID/handle/catalog reads, definition count updates, meta API state/log visibility, ordered missing-row bulk errors, and no runtime Shopify writes. HAR-246 extends that runtime coverage for GraphQL variable validation versus resolver `userErrors`, missing definition type, invalid field key/value, duplicate create/update handle behavior, stale row update/delete, blank upsert handle generation, and `where.ids` bulk partial-result behavior. The captured create/delete branches in `metaobjects-read.json` now run through cassette-backed hydration for the upstream definition/entry preconditions; additional live captures are still needed before promoting broader update/upsert/bulk delete parity scenarios.

HAR-450 adds `config/parity-specs/metaobjects/metaobject-bulk-delete-type-lifecycle.json`, `config/parity-requests/metaobjects/metaobject-bulk-delete-type-*.graphql`, and a live 2026-04 fixture for `metaobjectBulkDelete(where: { type })`. The recorder creates a disposable definition with two rows, captures the seeded read, bulk deletes by type, records downstream ID/catalog/definition-count reads, and then cleans up the definition. The cassette runner hydrates the proxy from the captured seeded read through `MetaobjectBulkDeleteHydrateByType`, replays the type-scoped bulk delete locally, compares mutation `userErrors` and downstream reads strictly, and records Shopify's async job id/`done` timing as the only accepted payload volatility.

HAR-245 adds `gleam/test/parity_test.gleam` for the combined definition/row lifecycle matrix and promotes the live schema-change sequence through `config/parity-specs/metaobjects/metaobject-schema-change-lifecycle.json`. The fixture-backed local scenario creates a definition, creates/updates/deletes rows before a schema edit, updates the definition with an added required field, removed field, reordered fields, display-name key change, validation change, and capability changes, then validates pre-existing and post-change row reads plus post-change create/update/delete behavior. It also checks singular ID/handle lookups, catalog reads, meta state/log visibility, and no runtime Shopify writes.

HAR-384 adds `config/parity-specs/metaobjects/metaobject-reference-lifecycle.json`, `config/parity-requests/metaobjects/metaobject-reference-read.graphql`, and a live 2026-04 fixture for metaobject reference relationships. Cold live-hybrid reference reads now passthrough to the captured cassette response, while `gleam/test/parity_test.gleam` covers staged create/update/delete reference effects and no runtime upstream writes.

### Validation anchors

- Registry and coverage tests: `tests/unit/operation-registry.test.ts`, `gleam/test/parity_test.gleam`
- Definition read runtime tests: `gleam/test/parity_test.gleam`
- Entry read runtime tests: `gleam/test/parity_test.gleam`
- Definition mutation runtime tests: `gleam/test/parity_test.gleam`
- Entry mutation runtime tests: `gleam/test/parity_test.gleam`
- Definition/entry schema-change runtime tests: `gleam/test/parity_test.gleam`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`
- Read fixture recorder: `scripts/capture-metaobject-read-conformance.mts`
- Schema-change fixture recorder: `scripts/capture-metaobject-schema-change-conformance.ts`
- Reference relationship fixture recorder: `scripts/capture-metaobject-reference-conformance.ts`
- Bulk-delete fixture recorder: `scripts/capture-metaobject-bulk-delete-conformance.ts`

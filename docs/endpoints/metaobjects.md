# Metaobjects Endpoint Group

The metaobjects group covers Shopify Admin GraphQL custom data roots. Runtime support now models definition reads/lifecycle mutations plus the core entry row lifecycle locally.

HAR-131 is the source related issue for the metaobjects area.

## Supported definition read roots

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

Live-hybrid mode still fetches upstream first. When local staged or snapshot definition state exists, the proxy overlays local definitions onto the effective read result; when no local definition exists, upstream no-data/null responses are returned unchanged rather than replaced with fabricated definitions.

Local catalog cursors use the proxy's stable `cursor:<definition gid>` form. Shopify's captured live catalog cursors are opaque and should not be treated as client-visible semantics.

## Supported entry read roots

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
- `referencedBy` as a Shopify-like empty connection until relation evidence exists

Snapshot mode reads entries from normalized `metaobjects` state and returns `null` for absent ID or `(type, handle)` lookups. Empty or absent type catalogs return non-null empty connections with empty `edges` / `nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.

`metaobjects(type:, first:, after:, before:, last:, reverse:, sortKey:, query:)` is type-scoped and never invents entries outside normalized state. Local catalog cursors use stable `cursor:<metaobject gid>` values. Supported local sort keys are `id`, `type`, `updated_at`, and `display_name`; `reverse` flips the sorted list before cursor windowing. Query filtering supports general text search plus documented field-value filters such as `fields.title:Alpha` against normalized field `value` / `jsonValue` data.

Live-hybrid mode fetches upstream first. When local snapshot or staged entry state exists, the proxy overlays normalized entries onto the selected roots; when no local entry exists, upstream no-data/null responses are returned unchanged.

## Supported definition mutation roots

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

Delete support stages deletion for definitions whose effective `metaobjectsCount` is zero and hides deleted base/staged definitions from downstream reads through a staged tombstone. Definitions with associated entries return an explicit local `UNSUPPORTED` userError because entry records and cascade semantics are not modeled yet; this keeps the known branch local and visible instead of pretending Shopify's destructive cascade has been faithfully emulated.

`standardMetaobjectDefinitionEnable` is limited to the bounded local template catalog currently represented by runtime tests. Known templates stage a standard definition locally with `standardTemplate` metadata; unknown template types return `TEMPLATE_NOT_FOUND` locally.

## Supported entry mutation roots

HAR-244 adds local staging for the core Admin GraphQL 2026-04 metaobject row lifecycle roots:

- `metaobjectCreate`
- `metaobjectUpdate`
- `metaobjectUpsert`
- `metaobjectDelete`
- `metaobjectBulkDelete`

Supported entry mutations never proxy to Shopify at runtime. They append the original GraphQL request body to the meta mutation log for later `POST /__meta/commit` replay, stage changes in the normalized `metaobjects` state bucket or `deletedMetaobjectIds` tombstone map, and make downstream `metaobject`, `metaobjectByHandle`, and `metaobjects` reads observe staged row writes immediately.

Create support requires an existing effective definition. It stages a synthetic `Metaobject` ID, explicit or generated handle, selected field values projected through the effective definition's ordered field definitions, `displayName` from the definition's `displayNameKey`, default/selected publishable status, nullable online-store capability shape, and an incremented effective definition `metaobjectsCount`.

Update support resolves the effective row by ID, patches selected fields while preserving omitted field values, supports handle changes with same-type uniqueness checks, merges publishable/online-store capability input, updates `displayName`, and keeps the row visible under the new handle while the old handle returns `null`.

Upsert support resolves by `MetaobjectHandleInput`. Existing rows are updated in place; missing rows are created against the effective definition with the requested handle. Definition misses and missing handle data return local userErrors rather than proxying.

Delete support stages a tombstone for base or staged rows, returns the selected `deletedId` on success, decrements the effective definition `metaobjectsCount`, and hides the row from ID, handle, and catalog reads. Missing rows return a local `NOT_FOUND` userError with `deletedId: null`.

Bulk delete support accepts the local `ids` branch used by runtime tests and a type-scoped `where.type` branch for local cleanup-style selection. It stages tombstones for found rows, returns a completed local `Job` payload when at least one row is deleted, preserves ordered `elementIndex` userErrors for missing IDs, updates definition counts per type, and keeps all effects local.

## Coverage boundaries

- Registry entries in this group are declared gaps unless they are marked implemented and have executable runtime tests, parity inventory, and documented field behavior.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance/runtime evidence, and documented field behavior. HAR-241 satisfies that bar for definition reads; HAR-242 satisfies that bar for definition mutation roots; HAR-243 satisfies that bar for entry reads; HAR-244 satisfies that bar for entry row mutation roots.
- Unsupported metaobjects mutations must not be registered as permanent passthrough support. The generic unknown-operation passthrough path can still handle unsupported runtime requests outside snapshot-only parity execution, but that is not a support commitment for any declared root.
- Do not add planned-only parity specs or request placeholders for this group. Add parity specs only after a captured Shopify interaction can run as evidence.

## Planned local-staging posture

- Definition mutation support does not yet migrate modeled entries or cascade definition deletes into entry state. Future definition/entry coupling needs conformance-backed migration and cascade behavior.
- Broader bulk delete selection semantics and Shopify async job timing need additional live conformance before widening beyond the local ids/type branches.
- Upsert support covers handle-scoped create/update behavior in the local model; additional conflict/userError branches should be expanded when captured.

## Empty and no-data expectations

- Singular entry and definition lookup misses should match Shopify null behavior once captured, including ID, type, and handle lookup branches.
- Connection roots should return Shopify-like empty `edges`, `nodes`, and `pageInfo` structures for known empty datasets instead of inventing records.
- Type-scoped entry reads must not synthesize arbitrary metaobjects when the snapshot or staged state lacks that type.
- Definition reads must not invent field definitions, capabilities, access settings, standard-template metadata, or associated entry counts without captured or staged state. HAR-241's serializer only projects normalized definition records and returns Shopify-like null/empty structures when no record exists.

## Conformance evidence needed before support

- Capture baseline definition catalog and definition detail reads, including empty catalog behavior and missing ID/type lookup behavior.
- Capture entry catalog reads by type, singular ID lookup, handle lookup, empty type behavior, pagination, reverse ordering, supported sort keys, and field-value query filters.
- Capture additional update, upsert, delete-missing, and bulk delete entry behavior before widening HAR-244's local branches beyond the tested/captured-safe slice.
- Expand definition mutation live captures for update, associated-entry delete cascades, and additional standard templates before broadening the HAR-242 local support boundaries.
- Promote parity specs only after comparison targets can verify Shopify payload shape, userErrors, nullability, empty connections, cursor treatment, and downstream read-after-write or read-after-delete behavior.

## Captured read fixture slice

HAR-240 adds a live 2026-04 read fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects-read.json`, recorded by `corepack pnpm conformance:capture-metaobjects`.

The recorder captures no-data behavior before setup:

- `metaobjectDefinitionByType(type:)` returns `null` for an unknown type.
- `metaobjectDefinition(id:)` returns `null` for an unknown definition GID.
- `metaobjects(type:, first:)` returns a non-null empty connection for an unknown type, with empty `edges`/`nodes`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.
- `metaobjectByHandle(handle:)` returns `null` for an unknown type/handle pair.
- `metaobject(id:)` returns `null` for an unknown entry GID.

The seeded branch creates one disposable merchant-owned metaobject definition and one entry, reads them, then deletes both. Definition reads cover catalog/detail/type lookup with `access`, `capabilities`, `displayNameKey`, ordered `fieldDefinitions`, `metaobjectsCount`, and connection cursors. Entry reads cover type catalog, ID lookup, handle lookup, `handle`, `type`, `displayName`, `updatedAt`, entry `capabilities`, ordered `fields`, and `field(key: "title")`.

HAR-241 adds `config/parity-specs/metaobject-definitions-read.json` for the definition-read subset of this fixture, enforced by `tests/integration/metaobject-definition-query-shapes.test.ts`.

HAR-243 adds `config/parity-specs/metaobjects-read.json` and `config/parity-requests/metaobjects-read.graphql` for the entry-read subset. The parity runner seeds the local proxy from the captured definition and entry read payloads, then replays `metaobjects`, `metaobject`, and `metaobjectByHandle` with strict comparison targets for seeded and no-data branches. Opaque Shopify connection cursors remain an expected difference because snapshot mode emits stable synthetic cursors.

HAR-242 adds `config/parity-specs/metaobject-definition-lifecycle-local-staging.json`, backed by `fixtures/conformance/local-runtime/2026-04/metaobject-definition-draft-flow.json`, `config/parity-requests/metaobject-definition-*.graphql`, and `tests/integration/metaobject-definition-draft-flow.test.ts`. The convention-driven parity runner executes the create/update/delete/read-after-write and bounded standard-enable flow against the local proxy harness with strict JSON comparison targets. The runtime test also covers meta API log/state visibility, no runtime Shopify writes, the captured merchant-owned access.admin guardrail, and explicit unsupported handling for associated-entry delete cascades.

HAR-244 adds `config/parity-specs/metaobject-entry-lifecycle-local-staging.json` and `tests/integration/metaobject-draft-flow.test.ts` for local entry row lifecycle staging. The test covers create/update/upsert/delete/bulk delete, downstream ID/handle/catalog reads, definition count updates, meta API state/log visibility, ordered missing-row bulk errors, and no runtime Shopify writes. The captured create/delete branches in `metaobjects-read.json` are used as shape evidence; additional live captures are still needed before promoting broader update/upsert/bulk delete parity scenarios.

## Validation anchors

- Registry and coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Definition read runtime tests: `tests/integration/metaobject-definition-query-shapes.test.ts`
- Entry read runtime tests: `tests/integration/metaobject-query-shapes.test.ts`
- Definition mutation runtime tests: `tests/integration/metaobject-definition-draft-flow.test.ts`
- Entry mutation runtime tests: `tests/integration/metaobject-draft-flow.test.ts`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`
- Read fixture recorder: `scripts/capture-metaobject-read-conformance.mts`

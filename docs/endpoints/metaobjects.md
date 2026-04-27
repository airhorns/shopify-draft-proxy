# Metaobjects Endpoint Group

The metaobjects group is a registry-only coverage map for Shopify Admin GraphQL custom data roots. It intentionally declares known roots before runtime support so future slices can add conformance-backed local modeling without treating known unsupported operations as supported passthrough.

HAR-131 is the source related issue for the metaobjects area. HAR-239 blocks the implementation slices that depend on this registry coverage map.

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

## Unsupported roots tracked by the registry

Planned local-staging mutations:

- `metaobjectCreate`
- `metaobjectUpdate`
- `metaobjectUpsert`
- `metaobjectDelete`
- `metaobjectBulkDelete`
- `metaobjectDefinitionCreate`
- `metaobjectDefinitionUpdate`
- `metaobjectDefinitionDelete`
- `standardMetaobjectDefinitionEnable`

## Coverage boundaries

- Unimplemented registry entries in this group are declared gaps. They make known Admin GraphQL roots discoverable but do not claim runtime support.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance evidence, and documented field behavior. HAR-241 satisfies that bar for definition reads and HAR-243 satisfies it for entry reads; entry mutations and definition mutations remain declared gaps.
- Unsupported metaobjects mutations must not be registered as permanent passthrough support. The generic unknown-operation passthrough path can still handle unsupported runtime requests outside snapshot-only parity execution, but that is not a support commitment for any declared root.
- Do not add planned-only parity specs or request placeholders for this group. Add parity specs only after a captured Shopify interaction can run as evidence.

## Planned local-staging posture

- Entry mutations must eventually stage locally without mutating Shopify at runtime, preserve the original raw mutation for commit replay, and make staged entries visible through `metaobject`, `metaobjectByHandle`, and `metaobjects`.
- Definition mutations must eventually stage schema changes locally, including access settings, capabilities, field definition ordering, standard-template enablement, and downstream effects on entry reads.
- Bulk delete support needs captured evidence for selection semantics, partial failure behavior, async payload shape, and read-after-delete visibility before it can be promoted.
- Upsert support needs captured evidence for create-vs-update identity, handle conflicts, and userErrors before it can be promoted.

## Empty and no-data expectations

- Singular entry and definition lookup misses should match Shopify null behavior once captured, including ID, type, and handle lookup branches.
- Connection roots should return Shopify-like empty `edges`, `nodes`, and `pageInfo` structures for known empty datasets instead of inventing records.
- Type-scoped entry reads must not synthesize arbitrary metaobjects when the snapshot or staged state lacks that type.
- Definition reads must not invent field definitions, capabilities, access settings, standard-template metadata, or associated entry counts without captured or staged state. HAR-241's serializer only projects normalized definition records and returns Shopify-like null/empty structures when no record exists.

## Conformance evidence needed before support

- Capture baseline definition catalog and definition detail reads, including empty catalog behavior and missing ID/type lookup behavior.
- Capture entry catalog reads by type, singular ID lookup, handle lookup, empty type behavior, pagination, reverse ordering, supported sort keys, and field-value query filters.
- Capture create, update, upsert, delete, bulk delete, definition create/update/delete, and standard definition enable userErrors before local staging is marked implemented.
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

HAR-241 adds `config/parity-specs/metaobject-definitions-read.json` for the definition-read subset of this fixture, enforced by `tests/integration/metaobject-definition-query-shapes.test.ts`. Entry reads and mutation lifecycles still need implementation before the full `metaobjects-read.json` capture can be promoted as a strict end-to-end parity scenario without expected gaps.

## Validation anchors

- Registry and coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Definition read runtime tests: `tests/integration/metaobject-definition-query-shapes.test.ts`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`
- Read fixture recorder: `scripts/capture-metaobject-read-conformance.mts`

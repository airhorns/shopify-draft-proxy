# Metaobjects Endpoint Group

The metaobjects group covers Shopify Admin GraphQL custom data roots. The current runtime support is intentionally definition-first: definition reads and definition lifecycle mutations have local modeling, while entry reads/mutations remain declared gaps until entry state is modeled.

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

## Unsupported roots tracked by the registry

Planned overlay reads:

- `metaobject`
- `metaobjectByHandle`
- `metaobjects`

Planned local-staging mutations:

- `metaobjectCreate`
- `metaobjectUpdate`
- `metaobjectUpsert`
- `metaobjectDelete`
- `metaobjectBulkDelete`

## Coverage boundaries

- Registry entries in this group are declared gaps unless they are marked implemented and have executable runtime tests, parity inventory, and documented field behavior.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance/runtime evidence, and documented field behavior. HAR-241 satisfies that bar for definition reads; HAR-242 satisfies that bar for the definition mutation roots listed above. Entry reads and entry mutations remain declared gaps.
- Unsupported metaobjects mutations must not be registered as permanent passthrough support. The generic unknown-operation passthrough path can still handle unsupported runtime requests outside snapshot-only parity execution, but that is not a support commitment for any declared root.
- Do not add planned-only parity specs or request placeholders for this group. Add parity specs only after a captured Shopify interaction can run as evidence.

## Planned local-staging posture

- Entry mutations must eventually stage locally without mutating Shopify at runtime, preserve the original raw mutation for commit replay, and make staged entries visible through `metaobject`, `metaobjectByHandle`, and `metaobjects`.
- Definition mutation support does not yet update modeled entries because entry state is still absent. When entry support lands, definition updates/deletes need conformance-backed entry migration and cascade behavior.
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
- Capture create, update, upsert, delete, and bulk delete entry behavior before entry local staging is marked implemented.
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

HAR-242 adds `config/parity-specs/metaobject-definition-lifecycle-local-staging.json`, backed by `fixtures/conformance/local-runtime/2026-04/metaobject-definition-draft-flow.json` and enforced by `tests/integration/metaobject-definition-draft-flow.test.ts`. This runtime fixture covers local definition create/update/delete, bounded standard enablement, downstream definition reads, meta API log/state visibility, no runtime Shopify writes, and explicit unsupported handling for associated-entry delete cascades.

Entry reads and entry mutation lifecycles still need implementation before the full `metaobjects-read.json` capture can be promoted as a strict end-to-end parity scenario without expected gaps.

## Validation anchors

- Registry and coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Definition read runtime tests: `tests/integration/metaobject-definition-query-shapes.test.ts`
- Definition mutation runtime tests: `tests/integration/metaobject-definition-draft-flow.test.ts`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`
- Read fixture recorder: `scripts/capture-metaobject-read-conformance.mts`

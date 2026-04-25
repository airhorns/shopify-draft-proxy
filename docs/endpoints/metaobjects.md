# Metaobjects Endpoint Group

The metaobjects group is a registry-only coverage map for Shopify Admin GraphQL custom data roots. It intentionally declares known roots before runtime support so future slices can add conformance-backed local modeling without treating known unsupported operations as supported passthrough.

HAR-131 is the source related issue for the metaobjects area. HAR-239 blocks the implementation slices that depend on this registry coverage map.

## Unsupported roots tracked by the registry

Planned overlay reads:

- `metaobject`
- `metaobjectByHandle`
- `metaobjects`
- `metaobjectDefinition`
- `metaobjectDefinitionByType`
- `metaobjectDefinitions`

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

- Registry entries in this group are declared gaps. They make known Admin GraphQL roots discoverable but do not claim runtime support.
- `implemented` must remain `false` until a root has executable runtime behavior, targeted tests, captured conformance evidence, and documented field behavior.
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
- Definition reads must not invent field definitions, capabilities, access settings, standard-template metadata, or associated entry counts without captured or staged state.

## Conformance evidence needed before support

- Capture baseline definition catalog and definition detail reads, including empty catalog behavior and missing ID/type lookup behavior.
- Capture entry catalog reads by type, singular ID lookup, handle lookup, empty type behavior, pagination, reverse ordering, supported sort keys, and field-value query filters.
- Capture create, update, upsert, delete, bulk delete, definition create/update/delete, and standard definition enable userErrors before local staging is marked implemented.
- Promote parity specs only after comparison targets can verify Shopify payload shape, userErrors, nullability, empty connections, cursor treatment, and downstream read-after-write or read-after-delete behavior.

## Validation anchors

- Registry and coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`

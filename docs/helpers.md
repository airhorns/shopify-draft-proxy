# Helper Modules

This document catalogs shared helper surfaces that future work should reuse before adding resource-local utility functions.

## `src/proxy/graphql-helpers.ts`

Shared helpers for GraphQL Admin proxy serializers.

- `getFieldResponseKey(field)` returns the Shopify GraphQL response key for a field, preserving aliases.
- `isPlainObject(value)` narrows unknown values before proxy serializers read or hydrate object-shaped Shopify payloads.
- `getDocumentFragments(document)` parses reusable fragment definitions from a GraphQL document for serializers that project stored snapshot data through the requested selection set.
- `readGraphqlDataResponsePayload(payload, responseKey)` reads a root `data` payload by response key, returning `null` for malformed or absent upstream payloads so hydrate paths keep Shopify-like no-data behavior.
- `projectGraphqlValue(value, selections, fragments, options)` and `projectGraphqlObject(source, selections, fragments, options)` project stored objects, arrays, fragment spreads, inline fragments, aliases, `__typename`, and connection `nodes` fallback behavior through a selected GraphQL shape. Use `options.projectFieldValue` only when a resource needs field-specific projection such as nested connection filtering.
- `defaultGraphqlTypeConditionApplies(source, typeCondition)` applies the default projection rule for fragments: no type condition always applies, and missing `__typename` keeps snapshot projection permissive.
- `getSelectedChildFields(field, options)` returns selected child `FieldNode`s. By default it preserves the direct-selection behavior used by product/customer serializers. Pass `{ includeInlineFragments: true }` for serializers that intentionally flatten inline fragments, as order serializers do.
- `readNullableIntArgument(field, name, variables)` and `readNullableStringArgument(field, name, variables)` read literal or variable-backed field arguments with the same null-on-missing/null-on-unsupported behavior used by existing proxy serializers.
- `buildSyntheticCursor(id)` creates the local `cursor:<id>` cursor form used by snapshot/local connection responses.
- `paginateConnectionItems(items, field, variables, getCursorValue, options)` applies `first`, `last`, `after`, and `before` cursor-window pagination for in-memory connection lists whose cursors can be derived from each item. Pass `parseCursor` when the resource needs to preserve raw upstream cursors instead of interpreting the local `cursor:<value>` form.
- `serializeConnection(field, options)` serializes selected `nodes`, `edges`, and `pageInfo` fields for a connection from an already-paginated item window. It supports aliases, unknown selected connection/edge fields as `null`, index-aware cursor values for synthetic ordinal cursors, optional raw cursor output via `pageInfoOptions.prefixCursors: false`, optional `pageInfo` cursor suppression via `pageInfoOptions.includeCursors: false`, inline-fragment child selection handling through `selectedFieldOptions`, and an opt-in `serializeUnknownField` hook for projected upstream connection payloads.
- `serializeConnectionPageInfo(selection, items, hasNextPage, hasPreviousPage, getCursorValue, options)` serializes a selected `pageInfo` object, including aliases, unknown selected fields as `null`, optional cursor prefixing, optional cursor suppression, and optional fallback cursors for preserved snapshot baselines.
- `serializeEmptyConnectionPageInfo(selection, options)` serializes an empty no-data `pageInfo` shape with false booleans and null cursors.

Use this module for GraphQL selection, projection, cursor, pagination, and PageInfo behavior shared across proxy resource files. New connection implementations should compose `paginateConnectionItems(...)` with `serializeConnection(...)` unless the payload must remain unprojected for a later serializer stage. Resource-specific domain decisions, such as how to sort products, derive ordinal cursors, preserve captured customer baseline cursors, apply additional fragment type-condition compatibility, or project Shopify-specific node fields, should stay in the resource module and pass explicit values into these helpers.

## `src/proxy/metafields.ts`

Shared helpers for owner-scoped metafield serializers and staging input handling.

- `normalizeOwnerMetafield(ownerKey, ownerId, raw)` normalizes a selected upstream metafield object into an owner-scoped record such as product, customer, or order metafields.
- `readMetafieldInputObjects(raw)` filters mutation input arrays down to object inputs before owner-specific validation or staging.
- `upsertOwnerMetafields(ownerKey, ownerId, inputs, existingMetafields, options)` applies Shopify-like `(namespace, key)` owner-scoped replacement semantics, with optional id lookup and identity trimming for input styles such as customer metafields.
- `serializeMetafieldSelection(...)`, `serializeMetafieldSelectionSet(...)`, and `serializeMetafieldsConnection(...)` serialize singular `metafield(...)` and connection-style `metafields(...)` selections, including aliases, stable synthetic cursors, PageInfo, and optional inline-fragment flattening for order serializers.
- `mergeMetafieldRecords(existing, next)` merges hydrated singular and connection metafields by `(namespace, key)` when upstream payloads provide both shapes.

Use this module before adding product-, customer-, or order-local metafield serializer/upsert helpers. Owner-specific validation, store placement, and captured Shopify quirks should remain in the resource module that owns them.

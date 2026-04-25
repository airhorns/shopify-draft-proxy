# Helper Modules

This document catalogs shared helper surfaces that future work should reuse before adding resource-local utility functions.

## `src/proxy/graphql-helpers.ts`

Shared helpers for GraphQL Admin proxy serializers.

- `getFieldResponseKey(field)` returns the Shopify GraphQL response key for a field, preserving aliases.
- `getSelectedChildFields(field, options)` returns selected child `FieldNode`s. By default it preserves the direct-selection behavior used by product/customer serializers. Pass `{ includeInlineFragments: true }` for serializers that intentionally flatten inline fragments, as order serializers do.
- `readNullableIntArgument(field, name, variables)` and `readNullableStringArgument(field, name, variables)` read literal or variable-backed field arguments with the same null-on-missing/null-on-unsupported behavior used by existing proxy serializers.
- `buildSyntheticCursor(id)` creates the local `cursor:<id>` cursor form used by snapshot/local connection responses.
- `paginateConnectionItems(items, field, variables, getCursorValue)` applies `first`, `last`, `after`, and `before` cursor-window pagination for in-memory connection lists whose cursors can be derived from each item.
- `serializeConnectionPageInfo(selection, items, hasNextPage, hasPreviousPage, getCursorValue, options)` serializes a selected `pageInfo` object, including aliases, unknown selected fields as `null`, optional cursor prefixing, and optional fallback cursors for preserved snapshot baselines.
- `serializeEmptyConnectionPageInfo(selection, options)` serializes an empty no-data `pageInfo` shape with false booleans and null cursors.

Use this module for GraphQL selection, cursor, pagination, and PageInfo behavior shared across proxy resource files. Resource-specific domain decisions, such as how to sort products or preserve captured customer baseline cursors, should stay in the resource module and pass explicit values into these helpers.

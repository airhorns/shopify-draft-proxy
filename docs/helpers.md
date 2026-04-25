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

## `src/proxy/metafields.ts`

Shared helpers for owner-scoped metafield serializers and staging input handling.

- `normalizeOwnerMetafield(ownerKey, ownerId, raw)` normalizes a selected upstream metafield object into an owner-scoped record such as product, customer, or order metafields.
- `readMetafieldInputObjects(raw)` filters mutation input arrays down to object inputs before owner-specific validation or staging.
- `upsertOwnerMetafields(ownerKey, ownerId, inputs, existingMetafields, options)` applies Shopify-like `(namespace, key)` owner-scoped replacement semantics, with optional id lookup and identity trimming for input styles such as customer metafields.
- `serializeMetafieldSelection(...)`, `serializeMetafieldSelectionSet(...)`, and `serializeMetafieldsConnection(...)` serialize singular `metafield(...)` and connection-style `metafields(...)` selections, including aliases, stable synthetic cursors, PageInfo, and optional inline-fragment flattening for order serializers.
- `mergeMetafieldRecords(existing, next)` merges hydrated singular and connection metafields by `(namespace, key)` when upstream payloads provide both shapes.

Use this module before adding product-, customer-, or order-local metafield serializer/upsert helpers. Owner-specific validation, store placement, and captured Shopify quirks should remain in the resource module that owns them.

## `src/search-query-parser.ts`

Shared helpers for Shopify Admin `query:` parsing, query execution, and common term matching.

- `parseSearchQuery(raw, options)` parses boolean-style Shopify search syntax into `SearchQueryNode` trees with implicit `AND`, `OR`, grouped expressions, leading `-` negation, optional `NOT`, field names, comparators, and quote handling.
- `applySearchQuery(items, rawQuery, options, matchesPositiveTerm)` is the preferred helper for endpoints that support boolean/grouped search. Resource modules provide only the domain-specific positive term matcher; the shared helper handles raw-query guards, parsing, AST traversal, and term/group negation.
- `parseSearchQueryTermList(rawQuery, options)` and `applySearchQueryTerms(items, rawQuery, options, matchesPositiveTerm)` cover endpoints whose captured behavior is still a simple term-list/implicit-AND subset. Use options such as `ignoredKeywords`, `preserveQuotesInTerms`, and `dropEmptyValues` to mirror endpoint-specific evidence without duplicating raw-query guards.
- `searchQueryTermValue(term)` reconstructs comparator-prefixed values such as `>=2026-01-01` for endpoint matchers.
- `stripSearchQueryValueQuotes(value)`, `normalizeSearchQueryValue(value)`, `matchesSearchQueryString(...)`, `matchesSearchQueryNumber(...)`, and `matchesSearchQueryDate(...)` provide reusable primitive matching behavior for field filters.

New or expanded endpoint search support should use this module for parsing, execution, and primitive matching. Keep endpoint-specific Shopify semantics in the resource module's positive term matcher, especially unsupported fields, known no-op warning behavior, and domain-specific search-index lag.

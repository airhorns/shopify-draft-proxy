# Helper Modules

This document catalogs shared Gleam helper surfaces that future runtime work should reuse before adding resource-local utility functions. TypeScript helper modules from the retired runtime have been removed; remaining TypeScript under `scripts/` is conformance, capture, and repository tooling.

## `src/shopify_draft_proxy/proxy/graphql_helpers.gleam`

Shared helpers for GraphQL Admin proxy serializers.

- cursor windowing, `nodes` / `edges` serialization, and selected `pageInfo` fields
- selected-field lookup, alias-aware response keys, and connection envelope helpers
- synthetic cursor generation for local and snapshot-backed connection responses
- resolved argument readers for common scalar, object, and string-list input shapes

Use this module for pagination and connection envelopes. Resource-specific sorting, filtering, cursor derivation, and node projection should stay in the owning domain module and pass explicit decisions into these helpers.

## `src/shopify_draft_proxy/proxy/app_identity.gleam`

Shared helper for request-owned app identity.

- reads the `x-shopify-draft-proxy-api-client-id` header case-insensitively
- trims blank values to `None`

Use this module when local Shopify behavior depends on the requesting app's API client ID, such as `$app:` namespace resolution or app-scoped callback validation. Do not hardcode a conformance app ID in domain code.

## `src/shopify_draft_proxy/proxy/metafields.gleam`

Shared helpers for owner-scoped metafield serializers and staging input handling.

- owner-scoped metafield normalization
- metafield input parsing and `(namespace, key)` replacement semantics
- singular `metafield(...)` and connection-style `metafields(...)` serialization
- captured Admin metafield type-name list/message used by mutation validators that need Shopify-like `INVALID_TYPE` payloads

Use this module before adding product-, customer-, order-, or metaobject-local metafield helpers. Owner-specific validation, store placement, and captured Shopify quirks belong in the resource module that owns them.

## `src/shopify_draft_proxy/proxy/metafield_values.gleam`

Shared custom-data value normalization helpers for metafields and metaobject fields.

- Shopify-like `jsonValue` projection for scalar, JSON, measurement, rating, date-time, decimal, reference-list, and list custom-data field types
- canonicalization for staged custom-data value strings where Shopify rewrites input values
- Shopify-like metaobject field input validation for scalar, measurement, rating, URL/color/date/time, reference, text min/max, and list custom-data field values
- measurement type predicates for serializers that need measurement-specific display behavior

Use this module before adding resource-local custom-data parsers or serializers.

## `src/shopify_draft_proxy/search_query_parser.gleam`

Shared helpers for Shopify Admin `query:` parsing, query execution, AST traversal, term-list guards, and primitive term matching.

Endpoint modules should provide only the domain-specific positive term matcher and documented Shopify quirks. Do not add new resource-local query parsers or duplicated query-tree traversal helpers.

## `src/shopify_draft_proxy/shopify/resource_ids.gleam`

Shared helpers for Shopify resource ID handling.

- canonical Shopify GID construction from full GIDs, numeric tails, and opaque tails
- GID tail extraction with query suffixes ignored
- stable Shopify resource ID sorting that compares numeric tails when available

Use this module before adding resource-local GID tail parsers, canonical ID builders, or Shopify resource ID comparators.

## `src/shopify_draft_proxy/proxy/upstream_query.gleam`

Shared chokepoint for runtime reads that need upstream Shopify data.

- in parity tests, reads from the installed cassette transport
- in live-hybrid runtime, forwards through the configured upstream client
- keeps unsupported passthrough and explicit commit replay distinct from supported local staging

Supported mutation branches must still synthesize responses from local state without runtime Shopify writes.

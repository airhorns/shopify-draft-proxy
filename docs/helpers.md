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
- reads the `x-shopify-draft-proxy-internal-visibility` header case-insensitively for local branches that emulate Shopify internal Admin visibility

Use this module when local Shopify behavior depends on the requesting app's API client ID, such as `$app:` namespace resolution or app-scoped callback validation. Do not hardcode a conformance app ID in domain code.

## `src/shopify_draft_proxy/proxy/phone_numbers.gleam`

Shared helper for Shopify-like phone number normalization.

- normalizes formatted international and national phone inputs to E.164 strings
- accepts common separators such as spaces, parentheses, dashes, and dots
- applies compatibility-style handling for full-width plus signs and digits
- uses the effective shop country as the default territory, falling back to `US`

Use this module when Admin API domain input accepts phone numbers and should stage or compare the normalized value instead of raw input text.

## `src/shopify_draft_proxy/proxy/admin_api_versions.gleam`

Shared helper for versioned Shopify Admin API route parsing.

- extracts year/month API versions from Shopify-like `/admin/api/<version>/graphql.json` request paths
- compares a request path version against a minimum Admin API version

Use this module when local behavior needs to follow an API-version-specific Shopify contract. Do not add resource-local request-path parsers for Admin API version checks.

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

## `src/shopify_draft_proxy/state/store.gleam`

Shared in-memory store helpers for cross-domain shop capability reads.

- `shop_sells_subscriptions` reads the effective staged/base `ShopRecord.features.sellsSubscriptions` capability and defaults missing synthetic shop state to `False`
- `set_shop_sells_subscriptions` configures the effective shop capability for tests and local-runtime parity scenarios without introducing ambient/global shop state
- `shop_discounts_by_market_enabled` reads the effective staged/base `ShopRecord.features.discountsByMarketEnabled` capability and defaults missing synthetic shop state to `False`
- `set_shop_discounts_by_market_enabled` configures the effective shop discount-market capability for tests and local-runtime scenarios without introducing ambient/global shop state
- `shop_markets_home_enabled` reads the effective staged/base `ShopRecord.features.unifiedMarkets` capability for Markets Home behavior and defaults missing synthetic shop state to `True`, matching the modern conformance shop posture
- `shop_market_plan_limit` reads the effective staged/base `ShopRecord.features.marketsGranted` limit used by legacy Markets plan-limit checks and defaults missing synthetic shop state to `50`
- `payment_gateway_by_id` reads an opt-in synthetic shop payment gateway by ID from effective staged/base `ShopRecord.paymentSettings.paymentGateways`
- `set_shop_payment_gateways` configures the effective shop payment gateway catalog for tests and local-runtime scenarios without introducing ambient/global shop state

Use these helpers when validation depends on synthetic shop capabilities, Markets plan posture, or installed payment-provider fixtures. Endpoint handlers should not add resource-local copies of shop capability defaults or gateway catalog lookups.

## `src/shopify_draft_proxy/proxy/upstream_query.gleam`

Shared chokepoint for runtime reads that need upstream Shopify data.

- in parity tests, reads from the installed cassette transport
- in live-hybrid runtime, forwards through the configured upstream client
- keeps unsupported passthrough and explicit commit replay distinct from supported local staging

Supported mutation branches must still synthesize responses from local state without runtime Shopify writes.

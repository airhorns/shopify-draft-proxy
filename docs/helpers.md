# Helper Surfaces

This document points at shared Rust helper surfaces to check before adding resource-local parsers, serializers, scalar readers, projection helpers, metafield/search/connection utilities, or Shopify ID helpers.

The proxy is currently implemented in Rust. New runtime behavior belongs in `src/`, and TypeScript under `scripts/` / `js/` should stay limited to conformance tooling and the embeddable package shim.

## `src/graphql.rs`

Use the GraphQL helpers here before adding resource-local document parsing or argument readers.

- `parse_operation(...)` identifies operation type and top-level roots without depending on operation names.
- `root_fields(...)` preserves aliases, response keys, selections, and resolved arguments for each root field.
- `root_field_arguments(...)` resolves literals, enums, lists, objects, variables, and missing variables into `ResolvedValue`.
- `root_field_selection(...)`, `nested_root_field_selection(...)`, and `nested_root_field_path_selection(...)` expose selected fields for serializers.

Route behavior by actual root fields and resolved arguments from these helpers, not by raw query string checks, unless a narrowly documented fixture compatibility branch already exists.

## `src/operation_registry.rs`

Use the registry helpers here before adding capability metadata or support discovery logic.

- `default_registry()` is the executable Rust registry.
- `implemented_entries(...)` filters only locally modeled roots.
- `operation_capability(...)` returns passthrough for unknown or unimplemented roots, even when metadata exists.

Do not mark a root implemented until the Rust runtime models its supported local lifecycle and downstream read-after-write behavior.

## Selection And Connection Helpers

Several generic serializers live under `src/proxy/` and should be reused before adding local copies.

- `src/proxy/selection.rs` owns alias-aware selected-field projection helpers such as `selected_json(...)`, `nullable_selected_json(...)`, `nested_selected_fields(...)`, `selected_child_selection(...)`, and `selected_fields_named(...)`.
- `src/proxy/product_helpers.rs` owns typed resource serializers and connection helpers such as `selected_typed_connection(...)` and product/saved-search JSON builders.
- `src/proxy/markets_online_inventory.rs` owns generic Shopify connection envelope helpers such as `connection_json(...)`, `connection_json_with_cursor(...)`, `selected_empty_connection_json(...)`, and `connection_page_info(...)`.

Prefer passing domain-specific sort/filter/cursor decisions into these helpers rather than duplicating connection envelope construction.

## `src/proxy/resource_ids.rs` Resource Identity Helpers

Check these helpers before adding GID tail extraction, resource-type parsing, or synthetic GID construction.

- `shopify_gid(...)` and `synthetic_shopify_gid(...)` build canonical Shopify Admin GIDs.
- `resource_id_path_tail(...)` preserves the raw final path segment; use it only when existing Shopify-like behavior keeps query suffixes.
- `resource_id_tail(...)` returns the final path segment without a query suffix for legacy-resource-id/token comparisons.
- `shopify_gid_resource_type(...)` parses the resource type from complete `gid://shopify/...` IDs.
- `next_proxy_synthetic_gid(...)` allocates stable per-instance synthetic IDs and delegates formatting to the shared resource-ID helpers.

Handle and slug behavior remains separate because Shopify semantics vary by domain: `src/proxy/app_shipping_helpers.rs` owns `slugify_handle(...)` and `fulfillment_service_handle(...)`; product and saved-search handle lookup helpers live in `src/proxy/product_helpers.rs`.

When a new domain needs ID behavior, extend `src/proxy/resource_ids.rs` instead of creating another resource-local parser. When a new domain needs handle behavior, first inspect the existing domain-specific handle helpers and extract only when semantics are genuinely shared.

## `src/proxy/resolved_values.rs` Resolved Argument Serialization

Use these helpers before adding resource-local `ResolvedValue` serializers.

- `resolved_value_json(...)` converts GraphQL `ResolvedValue` trees into JSON while preserving strings, numbers, booleans, nulls, lists, and objects.
- `resolved_variables_json(...)` serializes resolved variable maps for log metadata and validation payloads.

Input readers that need the inverse conversion from JSON into resolved GraphQL values should use `resolved_value_from_json(...)` in `src/proxy/routing.rs`.

## Metafields And Custom Data

Check the existing metafield helpers before adding metafield-specific parsing or projection.

- `src/proxy/media_products_saved_searches.rs` owns `owner_metafields_set(...)`, `owner_metafields_read(...)`, and metafield definition pinning/lifecycle read handlers.
- `src/proxy/metafields_orders_payments.rs` owns `metafield_json_value(...)`, `custom_data_metafield_type_matrix_record(...)`, `canonical_app_metafield_namespace(...)`, and shared `metafield_definition_*` value/ID helpers.

Owner-specific validation and storage should stay in the owning domain branch, but scalar parsing and projection should reuse shared helpers when possible.

## Search And Query Helpers

Search behavior is currently domain-local where Shopify semantics differ by resource, but shared patterns already exist.

- Product search helpers live around `product_catalog_search_read_data(...)`, product cursor/page-info helpers, and tag search helpers.
- Saved-search parsing and projection helpers live around `saved_search_query_tokens(...)`, `saved_search_filters(...)`, `saved_search_query_user_errors(...)`, and `canonical_saved_search_query(...)`.

Before adding a new search parser, inspect these functions and `docs/hard-and-weird-notes.md` for existing Shopify query grammar decisions.

## Version And Route Helpers

Use the existing route/version helpers before adding local request-path parsing.

- `admin_graphql_version(...)` extracts Shopify Admin API versions from Admin GraphQL paths.
- The route classifier in `DraftProxy::process_request(...)` preserves Shopify-like versioned routes and meta API boundaries.

Endpoint handlers should not add ad hoc Admin path parsing unless the behavior is tightly scoped and documented.

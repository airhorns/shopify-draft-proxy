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

## `src/proxy.rs` Selection And Connection Helpers

Several generic serializers live near the bottom of `src/proxy.rs` and should be reused before adding local copies.

- `selected_json(...)`, `nullable_selected_json(...)`, `selected_child_selection(...)`, and `selected_fields_named(...)` handle alias-aware selected-field projection.
- `connection_json(...)`, `connection_json_with_cursor(...)`, `selected_connection_json(...)`, `selected_empty_connection_json(...)`, `connection_edges_with_cursor(...)`, and `connection_page_info(...)` provide common Shopify connection envelopes.
- `selected_typed_connection(...)` is available when a domain has typed records but needs standard selected connection output.

Prefer passing domain-specific sort/filter/cursor decisions into these helpers rather than duplicating connection envelope construction.

## Resource Identity And Handle Helpers

Check existing helpers before adding GID, cursor, handle, or slug code.

- `next_proxy_synthetic_gid(...)` allocates stable per-instance synthetic IDs.
- `owner_type_from_gid(...)`, `is_safe_no_data_node_gid(...)`, and resource-specific ID helpers cover common GID handling.
- `slugify_handle(...)`, `normalize_localized_handle(...)`, `fulfillment_service_handle(...)`, product handle lookup helpers, and saved-search ID helpers cover existing Shopify-like handle and ID behavior.

When a new domain needs ID or handle behavior, prefer extracting a shared helper from the current Rust code over creating another resource-local variant.

## Metafields And Custom Data

Check the existing metafield helpers in `src/proxy.rs` before adding metafield-specific parsing or projection.

- `owner_metafields_set(...)` and `owner_metafields_read(...)` handle owner-scoped metafield staging and reads.
- `metafield_json_value(...)`, `custom_data_metafield_type_matrix_record(...)`, and `canonical_app_metafield_namespace(...)` cover common custom-data value and app-namespace behavior.
- Metafield definition helpers around `metafield_definition_*` handle definition IDs, defaults, pinning, and read serialization.

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

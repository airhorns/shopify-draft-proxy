# Helper Surfaces

This document points at shared Rust helper surfaces to check before adding resource-local parsers, serializers, scalar readers, projection helpers, metafield/search/connection utilities, or Shopify ID helpers.

The proxy is currently implemented in Rust. New runtime behavior belongs in `src/`, and TypeScript under `scripts/` / `js/` should stay limited to conformance tooling and the embeddable package shim.

## `src/admin_graphql.rs`

Use the executable-schema registry before adding another schema parser or checked-in schema projection.

- `schema(...)` returns the lazily built `async-graphql` schema for a captured `AdminApiVersion`.
- `root_field_arguments(...)`, `input_field_at_path(...)`, `input_owner_at_path(...)`, `input_object_fields(...)`, and `enum_values(...)` expose metadata from the same executable registry that validates requests.
- `output_field_named_type(...)` supports nested output planning without a second output-schema model.
- `output_type_condition_applies(...)` lets transitional JSON projectors use captured interface/union relationships instead of maintaining handwritten implementor lists; the executable engine remains the final projection authority.
- Custom scalar codecs live beside schema construction. Extend that explicit codec table when a captured schema adds a scalar; do not make unknown scalars permissive. `invalid_url_scalar_message(...)` is shared with the Shopify error adapter so engine validation and wire-envelope text cannot drift.

Full schema captures live at `config/admin-graphql/<version>/schema.graphql`, the executable/default version inventory lives in `config/admin-graphql/manifest.json`, and captures are produced by `scripts/capture-admin-graphql-schema.mts`. Do not introduce another partial mutation/input/output schema source or a second TypeScript version list.

## `src/graphql.rs`

Use the compatibility document helpers here before adding resource-local argument readers. `async-graphql`, through `src/admin_graphql.rs`, is the executable parser, validator, projector, and null-propagation engine; these helpers provide the normalized domain-handler view and Shopify-specific error locations.

- `parse_operation(...)` identifies operation type and top-level roots without depending on operation names.
- `root_fields(...)` preserves aliases, response keys, selections, and resolved arguments for each root field.
- `root_field_arguments(...)` resolves literals, enums, lists, objects, variables, and missing variables into `ResolvedValue`.
- Selection data is exposed on `RootFieldSelection` and `SelectedField`; use the projection helpers in `src/proxy/selection.rs` for selected/nested field serializers.

Route behavior by actual root fields and resolved arguments from these helpers, not by raw query string checks, unless a narrowly documented fixture compatibility branch already exists.

## Root And Node Resolver Registries

Use `src/operation_registry.rs` and `src/resolver_registry.rs` before adding capability metadata, support discovery logic, or another root routing table.

- `default_registry()` is the executable Rust registry.
- `implemented_entries(...)` filters only locally modeled roots.
- `operation_capability(...)` returns passthrough for unknown or unimplemented roots, even when metadata exists.
- `ResolverRegistry::new(...)` derives the instance-owned local root inventory from those implemented entries.

For generic IDs, update `src/node_resolver_inventory.rs` and its matching loader in `src/proxy/node_registry.rs` rather than adding another `node`/`nodes` switch. The inventory is exported for coverage audits; the executable loader reads the owning domain's effective store state.

Node loaders return store evidence through `NodeLoadState`: `Found`, `KnownMissing`, `NeedsHydration`, or `UnsupportedType`. Return `Some(Value::Null)` from an inventory loader when a tombstone or modeled safe-null makes absence authoritative; return `None` only when live-hybrid may need hydration. Do not add a parallel loader-name enum or per-call domain switch.

Do not mark a root implemented until the Rust runtime models its supported local lifecycle and downstream read-after-write behavior.

## `src/proxy/validation_helpers.rs` UserError Builders

Use these builders before adding inline `json!` userError objects with `field`, `message`, and optional `code` keys.

- `user_error(...)` emits the standard three-key userError shape and converts absent codes to JSON null.
- `user_error_omit_code(...)` emits field/message-only shapes, adding `code` only when Shopify includes it.
- `user_error_with_code_value(...)` handles computed or non-string code values.
- `presence_user_error(...)`, `length_user_error(...)`, typed variants, `user_error_with_extra_info(...)`, and `user_error_with_element_index(...)` cover common specialized userError shapes.
- `UserErrorField` accepts static paths, dynamic string paths, and JSON values, so prefer passing the field path directly instead of rebuilding arrays locally.

Do not use these helpers for top-level GraphQL `errors`/`extensions` envelopes; those are a different response shape.

Top-level parse, validation, variable/scalar coercion, location, path, and extension-code compatibility belongs in `src/proxy/graphql_error_compat.rs`. Keep resolver/business validation in domain modules and payload `userErrors`; do not add Shopify engine-envelope rewriting to `graphql_runtime.rs` or a resource handler.

## Selection, Connection, And Count Helpers

Several generic serializers live under `src/proxy/` and should be reused before adding local copies.

- `src/proxy/selection.rs` owns alias-aware selected-field projection helpers such as `selected_json(...)`, `nullable_selected_json(...)`, `nested_selected_fields(...)`, `selected_child_selection(...)`, and `selected_fields_named(...)`.
- `src/proxy/connection.rs` owns generic Shopify connection envelope helpers such as `connection_json(...)`, `connection_json_with_cursor(...)`, `selected_connection_json(...)`, `selected_empty_connection_json(...)`, `selected_typed_connection(...)`, `connection_window(...)`, `connection_page_info(...)`, and `empty_page_info(...)`.
- `src/proxy/connection.rs` also owns `count_object(...)` and `count_object_with_precision(...)` for Shopify `Count` objects. Do not rebuild `{ count, precision }` envelopes inline.
- `src/proxy/connection.rs` owns the staged-resource query path `staged_connection_query(...)` / `selected_staged_connection_with_args(...)`. Use it for staged reads that need resource-specific search decisions, sort-key mapping, `reverse`, cursor windowing, and filtered counts to stay in one order of operations.
- `src/proxy/product_helpers.rs` owns product/saved-search JSON builders and product-specific serializers; generic connection and Count envelopes belong in `connection.rs`.

Prefer passing domain-specific sort/filter/cursor decisions into these helpers rather than duplicating connection envelope construction.

## `src/proxy/resource_ids.rs` Resource Identity Helpers

Check these helpers before adding GID tail extraction, resource-type parsing, or synthetic GID construction.

- `shopify_gid(...)` and `synthetic_shopify_gid(...)` build canonical Shopify Admin GIDs.
- `resource_id_path_tail(...)` preserves the raw final path segment; use it only when existing Shopify-like behavior keeps query suffixes.
- `resource_id_tail(...)` returns the final path segment without a query suffix for legacy-resource-id/token comparisons.
- `shopify_gid_tail_for_type(...)` and `is_shopify_gid_of_type(...)` parse typed Shopify GID tails without open-coding `strip_prefix(...)` or `starts_with(...)`.
- `shopify_gid_resource_type(...)` parses the resource type from complete `gid://shopify/...` IDs.
- `next_proxy_synthetic_gid(...)` allocates stable per-instance synthetic IDs and delegates formatting to the shared resource-ID helpers.

Handle and slug behavior remains separate because Shopify semantics vary by domain: `src/proxy/app_shipping_helpers.rs` owns `slugify_handle(...)` and `fulfillment_service_handle(...)`; product and saved-search handle lookup helpers live in `src/proxy/product_helpers.rs`.

When a new domain needs ID behavior, extend `src/proxy/resource_ids.rs` instead of creating another resource-local parser. When a new domain needs handle behavior, first inspect the existing domain-specific handle helpers and extract only when semantics are genuinely shared.

## `src/proxy/resolved_values.rs` Resolved Argument Serialization

Use these helpers before adding resource-local `ResolvedValue` serializers.

- `resolved_value_json(...)` converts GraphQL `ResolvedValue` trees into JSON while preserving strings, numbers, booleans, nulls, lists, and objects.
- `resolved_variables_json(...)` serializes resolved variable maps for log metadata and validation payloads.
- `resolved_value_string(...)` reads a string from a single `ResolvedValue`.
- `resolved_string_field(...)` and `resolved_int_field(...)` read string and integer scalars from resolved argument/input maps.
- `resolved_object_list_field(...)` and `list_string_field(...)` read object and string lists from resolved argument/input maps.

Input readers that need the inverse conversion from JSON into resolved GraphQL values should use `resolved_value_from_json(...)` in `src/proxy/routing.rs`.

## Scalar, String, And Money Helpers

Use the canonical scalar/string helpers before adding one-off parsers.

- `src/proxy/money.rs` owns money scalar helpers such as `money_value(...)`, `money_set(...)`, `money_set_pair(...)`, and `normalize_money_amount(...)`.
- `src/proxy/scalar_helpers.rs` owns domain-agnostic token/file helpers such as `token_char(...)`, `token_chars_valid(...)`, `graphql_name_char(...)`, `graphql_name_byte(...)`, and `file_extension(...)`.

Keep domain-specific scalar semantics close to the owning domain, but extract repeated domain-agnostic one-liners into these shared homes.

## Metafields And Custom Data

Check the existing metafield helpers before adding metafield-specific parsing or projection.

- `src/proxy/media_products_saved_searches.rs` owns `owner_metafields_set(...)`, `owner_metafields_read(...)`, and metafield definition pinning/lifecycle read handlers.
- `src/proxy/metafields_orders_payments.rs` owns `metafield_json_value(...)`, `canonical_app_metafield_namespace(...)`, and shared `metafield_definition_*` value/ID helpers.

Owner-specific validation and storage should stay in the owning domain branch, but scalar parsing and projection should reuse shared helpers when possible.

## Search And Query Helpers

Search behavior is currently domain-local where Shopify semantics differ by resource, but shared patterns already exist.

- `src/proxy/search.rs` owns shared search-query primitives: the boolean query grammar used by product reads, flat whitespace token splitting used by saved-search/media/metaobject filters, common text matching, and comparator parsing. Keep resource-specific predicate semantics in domain modules and adapt them through these helpers instead of adding another local lexer/parser.
- Product search helpers live around `products_connection_value(...)`, product cursor/page-info helpers, and tag search helpers.
- Saved-search parsing and projection helpers live around `saved_search_query_tokens(...)`, `saved_search_filters(...)`, `saved_search_query_user_errors(...)`, and `canonical_saved_search_query(...)`.

Before adding a new search parser, inspect these functions and `docs/hard-and-weird-notes.md` for existing Shopify query grammar decisions.

## Version And Route Helpers

Use the existing route/version helpers before adding local request-path parsing.

- `admin_graphql_version(...)` extracts Shopify Admin API versions from Admin GraphQL paths.
- `version_at_least(...)` compares Shopify Admin API year-month versions.
- The route classifier in `DraftProxy::process_request(...)` preserves Shopify-like versioned routes and meta API boundaries.

Endpoint handlers should not add ad hoc Admin path parsing unless the behavior is tightly scoped and documented.

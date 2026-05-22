# Rust port Gleam proxy test crosswalk

Machine-readable source: `docs/rust-port-gleam-proxy-test-crosswalk.json`.

- Scope: `origin/main test/shopify_draft_proxy/proxy/**/*.gleam functions ending in _test`
- Total old Gleam proxy tests enumerated: **2162** across **45** files.
- Coverage status counts:
  - `implementation_unit_replaced_needs_review`: 171
  - `needs_manual_review`: 516
  - `represented_by_parity_and_rust_handler`: 4
  - `represented_by_rust_root_tests`: 1471
- Kind counts:
  - `domain_processor_behavior`: 275
  - `http_proxy_request`: 414
  - `proxy_integration_helper`: 390
  - `pure_helper_or_parser`: 423
  - `state_or_helper_behavior`: 660

## Domains with unresolved/manual-review entries

- `markets_mutation`: 78
- `online_store`: 52
- `payments`: 49
- `products_mutation`: 45
- `b2b`: 36
- `localization_mutation`: 35
- `draft_proxy`: 33
- `store_properties`: 33
- `metaobject_definitions`: 26
- `mutation_helpers`: 22
- `commit`: 20
- `graphql_helpers`: 18
- `orders`: 18
- `shipping_fulfillments`: 18
- `apps_mutation`: 15
- `admin_platform`: 13
- `customers`: 13
- `discounts`: 13
- `webhooks`: 12
- `segments`: 10
- `metafields`: 9
- `pagination`: 9
- `passthrough`: 9
- `delivery_settings`: 8
- `events`: 8
- `functions`: 8
- `media`: 8
- `operation_registry`: 8
- `privacy`: 8
- `localization`: 7
- `bulk_operations`: 6
- `draft_proxy_async`: 5
- `metafield_definitions`: 5
- `segments_mutation`: 5
- `capabilities`: 4
- `marketing`: 4
- `upstream_query`: 4
- `gift_cards`: 3
- `saved_searches`: 3
- `apps`: 2
- `operation_coverage`: 2
- `products`: 2
- `gift_cards_mutation`: 1

## Manual Rust-port updates

- Webhook endpoint/URI conversion, connection filter/sort/count behavior, validation edge cases, and dedicated Pub/Sub update field-path behavior are now represented by Rust integration tests in `tests/graphql_routes.rs`.
- Draft-proxy state dump/restore malformed-input behavior from `draft_proxy_test.gleam` is now represented by `tests/meta_routes.rs::ported_gleam_restore_state_rejects_malformed_rust_dumps`, covering unsupported schemas, missing state buckets, missing log entries, invalid synthetic identity, and malformed JSON.

## Notes

- `represented_by_rust_root_tests` means at least one extracted GraphQL root appears in Rust integration tests or a manual Rust-port override mapped an old helper/processor behavior to a Rust integration test; it is not a one-for-one proof.
- `represented_by_parity_and_rust_handler` means checked-in parity specs plus Rust handler source mention the extracted root; add Rust tests for endpoint families before calling them fully ported.
- `rust_handler_no_obvious_test` is the highest-priority bucket for new Rust integration coverage.

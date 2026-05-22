# Rust port Gleam proxy test crosswalk

Machine-readable source: `docs/rust-port-gleam-proxy-test-crosswalk.json`.

- Scope: `origin/main test/shopify_draft_proxy/proxy/**/*.gleam functions ending in _test`
- Total old Gleam proxy tests enumerated: **2162** across **45** files.
- Coverage status counts:
  - `implementation_unit_replaced_needs_review`: 124
  - `needs_manual_review`: 428
  - `represented_by_parity_and_rust_handler`: 4
  - `represented_by_rust_root_tests`: 1606
- Kind counts:
  - `domain_processor_behavior`: 275
  - `http_proxy_request`: 414
  - `proxy_integration_helper`: 390
  - `pure_helper_or_parser`: 423
  - `state_or_helper_behavior`: 660

## Domains with unresolved/manual-review entries

- `online_store`: 32
- `payments`: 49
- `products_mutation`: 45
- `b2b`: 36
- `localization_mutation`: 35
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
- `draft_proxy`: 4
- `marketing`: 4
- `upstream_query`: 4
- `gift_cards`: 3
- `saved_searches`: 3
- `apps`: 2
- `operation_coverage`: 2
- `products`: 2
- `gift_cards_mutation`: 1

## Manual Rust-port updates

- Online-store storefront-token, web-pixel, server-pixel endpoint, and mobile-platform update edge cases from `online_store_test.gleam` are now backed by `tests/graphql_routes.rs::online_store_storefront_access_token_edges_ported_from_gleam`, `tests/graphql_routes.rs::online_store_pixel_endpoint_edges_ported_from_gleam`, and `tests/graphql_routes.rs::online_store_mobile_platform_application_lifecycle_and_validation_are_local`, covering token scope filtering/blank/limit behavior, web-pixel duplicate/no-settings/update JSON/readback behavior, and server-pixel endpoint not-found/validation/success branches.
- Online-store theme local-runtime behavior from `online_store_test.gleam` is now backed by `tests/graphql_routes.rs::online_store_theme_lifecycle_tail_helpers_ported_from_gleam` and `tests/graphql_routes.rs::online_store_theme_file_lifecycle_tail_helpers_ported_from_gleam`, covering theme publish demotion/rejection, delete guards, locked/blank/successful themeUpdate branches, and theme-file upsert/copy/delete readback while leaving the schema-layer `role` input rejection as a distinct unresolved item.
- Market/catalog relation tail helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::market_catalog_relation_tail_helpers_ported_from_gleam`, covering the remaining plan-limit skip cases, unknown marketUpdate catalog/web-presence additions, catalogDelete price-list detachment, and catalogCreate/catalogUpdate taken or missing price-list/publication relation guards.
- Web-presence helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::market_web_presence_ported_gleam_helpers_stage_and_validate`, covering subfolder/domain root URL construction, Shopify locale-code normalization, aggregate invalid-locale errors, subfolder validation ordering, duplicate/taken suffix guards, create/update readback, unknown-domain create errors, empty-update no-ops, and update-domain-id non-validation.
- Fixed-price helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::price_list_fixed_prices_ported_gleam_helpers_stage_and_validate`, covering by-product bulk no-op/input-set/missing-product/price-limit guards, fixed-price add/update/delete lifecycle, duplicate variant last-wins behavior, missing price-list/variant/currency guards, missing fixed-price deletion errors, and selected downstream `priceList.prices` readback.
- Price-list update helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::price_list_create_update_delete_ported_gleam_helpers_stage_and_validate`, covering update name uniqueness/preservation, catalog/currency mismatch tolerance, invalid parent-adjustment revalidation, staged readback, catalog attachment, and null-catalog detachment for locally created price lists.
- Quantity-rule add helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::markets_quantity_pricing_and_web_presence_local_staging_match_captured_shapes`, covering numeric validation, duplicate variant IDs, unknown price lists, and valid rule staging/read payloads.
- Price-list helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::price_list_create_update_delete_ported_gleam_helpers_stage_and_validate`, covering create validation, parent adjustment bounds, typed `PriceListUserError` / `QuantityRuleUserError` / `MarketUserError` shapes, exact staged create/readback, catalog attachment, and null-catalog detachment for locally created price lists.
- Catalog create/delete/context helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::catalog_create_and_context_update_ported_gleam_helpers_stage_and_validate`, covering required/invalid status, required context and market IDs, country-context validation, typed `CatalogUserError` shapes, market-context staging/readback, unknown price-list/publication guards, missing-catalog delete, and `catalogContextUpdate` add/remove validation/readback.
- Market create validation/staging helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::market_create_ported_gleam_validation_and_staging_helpers_match_old_proxy_tests`, covering the old status/enabled guards, partial-input defaults, price-inclusion projection and location-condition rejection, currency settings validation/read-after-write, region duplicate/unsupported guards, generated handle slugification/deduplication, and duplicate name/handle errors.
- Market localization register/remove helper behavior from `markets_mutation_test.gleam` is now represented by `tests/graphql_routes.rs::market_localizations_register_remove_ported_gleam_helpers_stage_and_validate`, covering the old >100-key cap, missing resource register/remove errors, market/key/digest/value validation, seeded-content staging, no-op remove filters, returned removed rows, and read-after-remove state.
- Webhook endpoint/URI conversion, connection filter/sort/count behavior, validation edge cases, and dedicated Pub/Sub update field-path behavior are now represented by Rust integration tests in `tests/graphql_routes.rs`.
- Draft-proxy state dump/restore malformed-input behavior from `draft_proxy_test.gleam` is now represented by `tests/meta_routes.rs::ported_gleam_restore_state_rejects_malformed_rust_dumps`, covering unsupported schemas, missing state buckets, missing log entries, invalid synthetic identity, and malformed JSON.
- Events empty-read behavior from `events_test.gleam` and the draft-proxy event route helpers is now represented by `tests/graphql_routes.rs::ported_gleam_event_empty_read_shapes_match_draft_proxy_tests`, covering `event`, `events`, `eventsCount`, aliases, unknown event subfields, unknown mixed event roots, and the data envelope.
- Draft-proxy route/config/snapshot helper behavior from `draft_proxy_test.gleam` is now represented by `tests/meta_routes.rs::ported_gleam_draft_proxy_route_and_snapshot_helpers_match_old_proxy_tests`, covering default/config snapshot parity, snapshot-path config, meta route/getter equality, method/path guards, invalid GraphQL bodies, unsupported mutation envelopes, reset, empty commit, and default Rust state dump envelopes. This pass also fixed `Config::default()` to preserve the old proxy default port `4000`.

## Notes

- `represented_by_rust_root_tests` means at least one extracted GraphQL root appears in Rust integration tests or a manual Rust-port override mapped an old helper/processor behavior to a Rust integration test; it is not a one-for-one proof.
- `represented_by_parity_and_rust_handler` means checked-in parity specs plus Rust handler source mention the extracted root; add Rust tests for endpoint families before calling them fully ported.
- `rust_handler_no_obvious_test` is the highest-priority bucket for new Rust integration coverage.

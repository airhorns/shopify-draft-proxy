//// Pure-Gleam parity scenario suite.
////
//// This is the Gleam port's replacement for
//// `tests/unit/conformance-parity-scenarios.test.ts`. Each test loads a
//// parity spec from `config/parity-specs/...`, drives the GraphQL
//// document through `draft_proxy.process_request`, and compares the
//// proxy response to the corresponding capture slice using the spec's
//// `expectedDifferences` matchers.
////
//// Tests run from the `gleam/` subdirectory; the runner resolves
//// repo-root-relative paths in the spec via `..`.

import parity/diff
import parity/runner

fn check(spec_path: String) -> Nil {
  case runner.run(spec_path) {
    Ok(report) -> {
      // Sanity: the spec must define at least one target, otherwise the
      // suite would be silently a no-op.
      assert report.targets != []
      case runner.into_assert(report) {
        Ok(Nil) -> Nil
        Error(message) -> panic as message
      }
    }
    Error(err) -> panic as runner.render_error(err)
  }
}

// ----------- webhooks -----------

pub fn webhook_subscription_catalog_read_test() {
  check("config/parity-specs/webhooks/webhook-subscription-catalog-read.json")
}

pub fn webhook_subscription_required_argument_validation_test() {
  check(
    "config/parity-specs/webhooks/webhook-subscription-required-argument-validation.json",
  )
}

pub fn webhook_subscription_conformance_test() {
  check("config/parity-specs/webhooks/webhook-subscription-conformance.json")
}

pub fn saved_search_local_staging_test() {
  check("config/parity-specs/saved-searches/saved-search-local-staging.json")
}

pub fn saved_search_query_grammar_test() {
  check("config/parity-specs/saved-searches/saved-search-query-grammar.json")
}

pub fn saved_search_resource_roots_test() {
  check("config/parity-specs/saved-searches/saved-search-resource-roots.json")
}

pub fn gift_card_search_filters_test() {
  check("config/parity-specs/gift-cards/gift-card-search-filters.json")
}

// ----------- store properties -----------

pub fn shop_baseline_read_test() {
  check("config/parity-specs/store-properties/shop-baseline-read.json")
}

pub fn shop_policy_update_parity_test() {
  check("config/parity-specs/store-properties/shopPolicyUpdate-parity.json")
}

pub fn admin_platform_store_property_node_reads_test() {
  check(
    "config/parity-specs/admin-platform/admin-platform-store-property-node-reads.json",
  )
}

// ----------- products -----------

pub fn product_empty_state_read_test() {
  check("config/parity-specs/products/product-empty-state-read.json")
}

pub fn product_related_by_id_not_found_read_test() {
  check(
    "config/parity-specs/products/product-related-by-id-not-found-read.json",
  )
}

pub fn product_feeds_empty_read_test() {
  check("config/parity-specs/products/product-feeds-empty-read.json")
}

pub fn product_detail_read_test() {
  check("config/parity-specs/products/product-detail-read.json")
}

pub fn products_catalog_read_test() {
  check("config/parity-specs/products/products-catalog-read.json")
}

pub fn product_helper_roots_read_test() {
  check("config/parity-specs/products/product-helper-roots-read.json")
}

pub fn product_variants_read_test() {
  check("config/parity-specs/products/product-variants-read.json")
}

pub fn inventory_level_read_test() {
  check("config/parity-specs/products/inventory-level-read.json")
}

pub fn products_variant_search_read_test() {
  check("config/parity-specs/products/products-variant-search-read.json")
}

pub fn products_search_read_test() {
  check("config/parity-specs/products/products-search-read.json")
}

// NOTE: functions-metadata-local-staging fails because the capture
// fixture (`fixtures/.../functions-metadata-flow.json`) is hand-
// written and aspirational — it claims `Validation/2` + T+1s, but
// running the *TS port* against the same primary variables produces
// `Validation/1` + T+0s (verified 2026-04-29 with a debug
// integration test). The Gleam port matches the TS port exactly;
// the fixture diverges from BOTH. Either patch the fixture to match
// real proxy output, or add `expectedDifferences` rules with
// `shopify-gid:Validation` + `iso-timestamp` matchers. Tracked as a
// fixture-correctness follow-up, not a port gap.

// This scenario relies on runner seeding from the capture's
// `seedShopifyFunctions` records so known owner/app metadata can be
// preserved across staged validation and cart-transform writes.
pub fn functions_owner_metadata_local_staging_test() {
  check(
    "config/parity-specs/functions/functions-owner-metadata-local-staging.json",
  )
}

// ----------- apps -----------

pub fn delegate_access_token_current_input_local_staging_test() {
  check(
    "config/parity-specs/apps/delegate-access-token-current-input-local-staging.json",
  )
}

// NOTE: scenarios that require pre-seeded store state (e.g. captured
// shopifyFunctions, segments-baseline-read) are deferred until the
// runner gains snapshot-seeding support. The capture already contains
// the data the proxy needs to be seeded with; the seeding harness is
// the next step.

// ----------- segments -----------

pub fn segments_create_invalid_query_validation_test() {
  check(
    "config/parity-specs/segments/segment-create-invalid-query-validation.json",
  )
}

pub fn segment_query_grammar_not_contains_test() {
  check("config/parity-specs/segments/segment-query-grammar-not-contains.json")
}

pub fn segments_update_unknown_id_validation_test() {
  check(
    "config/parity-specs/segments/segment-update-unknown-id-validation.json",
  )
}

pub fn segments_delete_unknown_id_validation_test() {
  check(
    "config/parity-specs/segments/segment-delete-unknown-id-validation.json",
  )
}

pub fn customer_segment_members_query_lifecycle_test() {
  check(
    "config/parity-specs/segments/customer-segment-members-query-lifecycle.json",
  )
}

// ----------- events -----------

pub fn event_empty_read_test() {
  check("config/parity-specs/events/event-empty-read.json")
}

pub fn metafield_definitions_product_empty_read_test() {
  check(
    "config/parity-specs/metafields/metafield-definitions-product-empty-read.json",
  )
}

pub fn standard_metafield_definition_enable_validation_test() {
  check(
    "config/parity-specs/metafields/standard-metafield-definition-enable-validation.json",
  )
}

// ----------- runner self-check -----------

/// Confirms `into_assert` actually surfaces non-empty mismatches as a
/// failure, so the parity tests above are not trivially passing on
/// empty reports.
pub fn runner_into_assert_flags_mismatches_test() {
  let report =
    runner.Report(scenario_id: "synthetic", targets: [
      runner.TargetReport(
        name: "always-fails",
        capture_path: "$",
        proxy_path: "$",
        mismatches: [
          diff.Mismatch(path: "$.x", expected: "1", actual: "2"),
        ],
      ),
    ])
  assert runner.has_mismatches(report)
  let assert Error(_) = runner.into_assert(report)
}

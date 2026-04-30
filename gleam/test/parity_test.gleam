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

// ----------- customers -----------

pub fn customer_by_identifier_read_test() {
  check("config/parity-specs/customers/customer-by-identifier-read.json")
}

pub fn customer_account_page_data_erasure_test() {
  check("config/parity-specs/customers/customer-account-page-data-erasure.json")
}

pub fn customer_detail_parity_plan_test() {
  check("config/parity-specs/customers/customer-detail-parity-plan.json")
}

pub fn customer_nested_subresources_read_test() {
  check("config/parity-specs/customers/customer-nested-subresources-read.json")
}

pub fn customer_order_summary_read_effects_test() {
  check(
    "config/parity-specs/customers/customer-order-summary-read-effects.json",
  )
}

pub fn customer_outbound_side_effect_validation_test() {
  check(
    "config/parity-specs/customers/customer-outbound-side-effect-validation.json",
  )
}

pub fn customer_add_tax_exemptions_parity_test() {
  check("config/parity-specs/customers/customerAddTaxExemptions-parity.json")
}

pub fn customer_address_lifecycle_parity_test() {
  check("config/parity-specs/customers/customerAddressLifecycle-parity.json")
}

pub fn customer_create_live_parity_test() {
  check("config/parity-specs/customers/customerCreate-parity-plan.json")
}

pub fn customer_delete_parity_plan_test() {
  check("config/parity-specs/customers/customerDelete-parity-plan.json")
}

pub fn customer_email_marketing_consent_update_parity_test() {
  check(
    "config/parity-specs/customers/customerEmailMarketingConsentUpdate-parity.json",
  )
}

pub fn customer_input_addresses_parity_test() {
  check("config/parity-specs/customers/customerInputAddresses-parity.json")
}

pub fn customer_input_inline_consent_parity_test() {
  check("config/parity-specs/customers/customerInputInlineConsent-parity.json")
}

pub fn customer_input_validation_parity_test() {
  check("config/parity-specs/customers/customerInputValidation-parity.json")
}

pub fn customer_merge_attached_resources_parity_test() {
  check(
    "config/parity-specs/customers/customerMerge-attached-resources-parity.json",
  )
}

pub fn customer_merge_parity_test() {
  check("config/parity-specs/customers/customerMerge-parity.json")
}

pub fn customer_remove_tax_exemptions_parity_test() {
  check("config/parity-specs/customers/customerRemoveTaxExemptions-parity.json")
}

pub fn customer_replace_tax_exemptions_parity_test() {
  check(
    "config/parity-specs/customers/customerReplaceTaxExemptions-parity.json",
  )
}

pub fn customer_set_parity_test() {
  check("config/parity-specs/customers/customerSet-parity.json")
}

pub fn customer_sms_marketing_consent_update_parity_test() {
  check(
    "config/parity-specs/customers/customerSmsMarketingConsentUpdate-parity.json",
  )
}

pub fn customer_update_parity_plan_test() {
  check("config/parity-specs/customers/customerUpdate-parity-plan.json")
}

pub fn customers_advanced_search_read_test() {
  check("config/parity-specs/customers/customers-advanced-search-read.json")
}

pub fn customers_catalog_parity_plan_test() {
  check("config/parity-specs/customers/customers-catalog-parity-plan.json")
}

pub fn customers_count_read_test() {
  check("config/parity-specs/customers/customers-count-read.json")
}

pub fn customers_relevance_search_read_test() {
  check("config/parity-specs/customers/customers-relevance-search-read.json")
}

pub fn customers_search_read_test() {
  check("config/parity-specs/customers/customers-search-read.json")
}

pub fn customers_sort_keys_read_test() {
  check("config/parity-specs/customers/customers-sort-keys-read.json")
}

pub fn store_credit_account_local_staging_test() {
  check("config/parity-specs/customers/store-credit-account-local-staging.json")
}

// ----------- store properties -----------

pub fn shop_baseline_read_test() {
  check("config/parity-specs/store-properties/shop-baseline-read.json")
}

pub fn shop_policy_update_parity_test() {
  check("config/parity-specs/store-properties/shopPolicyUpdate-parity.json")
}

pub fn business_entities_catalog_read_test() {
  check(
    "config/parity-specs/store-properties/business-entities-catalog-read.json",
  )
}

pub fn business_entity_fallbacks_read_test() {
  check(
    "config/parity-specs/store-properties/business-entity-fallbacks-read.json",
  )
}

pub fn location_detail_read_test() {
  check("config/parity-specs/store-properties/location-detail-read.json")
}

pub fn location_custom_id_miss_read_test() {
  check(
    "config/parity-specs/store-properties/location-custom-id-miss-read.json",
  )
}

pub fn location_add_blank_name_validation_test() {
  check(
    "config/parity-specs/store-properties/location-add-blank-name-validation.json",
  )
}

pub fn location_edit_unknown_id_validation_test() {
  check(
    "config/parity-specs/store-properties/location-edit-unknown-id-validation.json",
  )
}

pub fn location_activate_missing_idempotency_validation_test() {
  check(
    "config/parity-specs/store-properties/location-activate-missing-idempotency-validation.json",
  )
}

pub fn location_deactivate_missing_idempotency_validation_test() {
  check(
    "config/parity-specs/store-properties/location-deactivate-missing-idempotency-validation.json",
  )
}

pub fn location_delete_active_location_validation_test() {
  check(
    "config/parity-specs/store-properties/location-delete-active-location-validation.json",
  )
}

pub fn publishable_publish_product_parity_test() {
  check(
    "config/parity-specs/store-properties/publishablePublish-product-parity.json",
  )
}

pub fn publishable_publish_shop_count_parity_test() {
  check(
    "config/parity-specs/store-properties/publishablePublish-shop-count-parity.json",
  )
}

pub fn publishable_publish_to_current_channel_product_parity_test() {
  check(
    "config/parity-specs/store-properties/publishablePublishToCurrentChannel-product-parity.json",
  )
}

pub fn publishable_publish_to_current_channel_shop_count_parity_test() {
  check(
    "config/parity-specs/store-properties/publishablePublishToCurrentChannel-shop-count-parity.json",
  )
}

pub fn publishable_unpublish_product_parity_test() {
  check(
    "config/parity-specs/store-properties/publishableUnpublish-product-parity.json",
  )
}

pub fn publishable_unpublish_shop_count_parity_test() {
  check(
    "config/parity-specs/store-properties/publishableUnpublish-shop-count-parity.json",
  )
}

pub fn publishable_unpublish_to_current_channel_product_parity_test() {
  check(
    "config/parity-specs/store-properties/publishableUnpublishToCurrentChannel-product-parity.json",
  )
}

pub fn publishable_unpublish_to_current_channel_shop_count_parity_test() {
  check(
    "config/parity-specs/store-properties/publishableUnpublishToCurrentChannel-shop-count-parity.json",
  )
}

pub fn collection_publishable_publication_parity_test() {
  check(
    "config/parity-specs/store-properties/collectionPublishablePublication-parity.json",
  )
}

pub fn admin_platform_store_property_node_reads_test() {
  check(
    "config/parity-specs/admin-platform/admin-platform-store-property-node-reads.json",
  )
}

pub fn functions_metadata_local_staging_test() {
  check("config/parity-specs/functions/functions-metadata-local-staging.json")
}

// This scenario relies on runner seeding from the capture's
// `seedShopifyFunctions` records so known owner/app metadata can be
// preserved across staged validation and cart-transform writes.
pub fn functions_owner_metadata_local_staging_test() {
  check(
    "config/parity-specs/functions/functions-owner-metadata-local-staging.json",
  )
}

pub fn functions_live_owner_metadata_read_test() {
  check("config/parity-specs/functions/functions-live-owner-metadata-read.json")
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

pub fn metafield_definitions_product_read_test() {
  check(
    "config/parity-specs/metafields/metafield-definitions-product-read.json",
  )
}

pub fn metafield_definition_pinning_parity_test() {
  check(
    "config/parity-specs/metafields/metafield-definition-pinning-parity.json",
  )
}

pub fn metafield_definition_lifecycle_mutations_test() {
  check(
    "config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json",
  )
}

pub fn custom_data_metafield_type_matrix_test() {
  check("config/parity-specs/metafields/custom-data-metafield-type-matrix.json")
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

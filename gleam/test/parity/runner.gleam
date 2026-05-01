//// Pure-Gleam parity runner.
////
//// Replaces the legacy vitest harness in
//// `tests/unit/conformance-parity-scenarios.test.ts`. Reads a parity
//// spec, loads the capture and GraphQL document referenced by the
//// spec, drives them through `draft_proxy.process_request`, and
//// compares each target's `capturePath` slice of the capture against
//// the same `proxyPath` slice of the proxy response — applying the
//// spec's `expectedDifferences` matchers.
////
//// Per-target `proxyRequest` overrides are supported. State (store,
//// synthetic identity) is threaded forward across requests, so a
//// target can read back records the primary mutation created.
////
//// File-system paths in the spec are repo-root relative. Tests run
//// from the `gleam/` subdirectory; the runner resolves paths via `..`
//// (configurable via `RunnerConfig.repo_root`).

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/diff.{type Mismatch}
import parity/json_value.{
  type JsonValue, JArray, JBool, JFloat, JInt, JNull, JObject, JString,
}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, NoVariables, OverrideRequest, ProxyLog, ProxyResponse,
  ProxyState, ReusePrimary, VariablesFromCapture, VariablesFromFile,
  VariablesInline,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type B2BCompanyContactRecord, type B2BCompanyContactRoleRecord,
  type B2BCompanyLocationRecord, type B2BCompanyRecord, type CapturedJsonValue,
  type CarrierServiceRecord, type CatalogRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type CustomerAccountPageRecord, type CustomerAddressRecord,
  type CustomerCatalogConnectionRecord, type CustomerCatalogPageInfoRecord,
  type CustomerDefaultAddressRecord, type CustomerDefaultEmailAddressRecord,
  type CustomerDefaultPhoneNumberRecord,
  type CustomerEmailMarketingConsentRecord, type CustomerEventSummaryRecord,
  type CustomerMetafieldRecord, type CustomerOrderSummaryRecord,
  type CustomerPaymentMethodInstrumentRecord, type CustomerPaymentMethodRecord,
  type CustomerRecord, type CustomerSmsMarketingConsentRecord,
  type DeliveryProfileRecord, type DiscountRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type FulfillmentOrderRecord,
  type FulfillmentRecord, type GiftCardConfigurationRecord,
  type GiftCardRecipientAttributesRecord, type GiftCardRecord,
  type GiftCardTransactionRecord, type InventoryItemRecord,
  type InventoryLevelRecord, type InventoryLocationRecord,
  type InventoryMeasurementRecord, type InventoryQuantityRecord,
  type InventoryWeightRecord, type LocationRecord, type MarketRecord,
  type MarketingRecord, type MarketingValue,
  type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord, type MetaobjectCapabilitiesRecord,
  type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectDefinitionTypeRecord, type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectJsonValue, type MetaobjectRecord,
  type MetaobjectStandardTemplateRecord, type Money,
  type OnlineStoreContentRecord, type OrderRecord, type PaymentSettingsRecord,
  type PriceListRecord, type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductMediaRecord, type ProductMetafieldRecord, type ProductOptionRecord,
  type ProductOptionValueRecord, type ProductRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SegmentRecord, type SellingPlanGroupRecord,
  type SellingPlanRecord, type ShippingOrderRecord,
  type ShippingPackageDimensionsRecord, type ShippingPackageRecord,
  type ShippingPackageWeightRecord, type ShopAddressRecord,
  type ShopDomainRecord, type ShopFeaturesRecord, type ShopPlanRecord,
  type ShopPolicyRecord, type ShopRecord, type ShopResourceLimitsRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord,
  type StoreCreditAccountRecord, type StorePropertyRecord,
  type StorePropertyValue, type TranslationRecord, type WebPresenceRecord,
  B2BCompanyContactRecord, B2BCompanyContactRoleRecord, B2BCompanyLocationRecord,
  B2BCompanyRecord, CapturedArray, CapturedBool, CapturedFloat, CapturedInt,
  CapturedNull, CapturedObject, CapturedString, CarrierServiceRecord,
  CatalogRecord, CollectionImageRecord, CollectionRecord, CollectionRuleRecord,
  CollectionRuleSetRecord, CustomerAccountPageRecord, CustomerAddressRecord,
  CustomerCatalogConnectionRecord, CustomerCatalogPageInfoRecord,
  CustomerDefaultAddressRecord, CustomerDefaultEmailAddressRecord,
  CustomerDefaultPhoneNumberRecord, CustomerEmailMarketingConsentRecord,
  CustomerEventSummaryRecord, CustomerMetafieldRecord,
  CustomerOrderSummaryRecord, CustomerPaymentMethodInstrumentRecord,
  CustomerPaymentMethodRecord, CustomerRecord, CustomerSmsMarketingConsentRecord,
  DeliveryProfileRecord, DiscountRecord, DraftOrderRecord,
  DraftOrderVariantCatalogRecord, FulfillmentOrderRecord, FulfillmentRecord,
  GiftCardConfigurationRecord, GiftCardRecipientAttributesRecord, GiftCardRecord,
  GiftCardTransactionRecord, InventoryItemRecord, InventoryLevelRecord,
  InventoryLocationRecord, InventoryMeasurementRecord, InventoryQuantityRecord,
  InventoryWeightFloat, InventoryWeightInt, InventoryWeightRecord, LocaleRecord,
  LocationRecord, MarketRecord, MarketingBool, MarketingFloat, MarketingInt,
  MarketingList, MarketingNull, MarketingObject, MarketingRecord,
  MarketingString, MetafieldDefinitionCapabilitiesRecord,
  MetafieldDefinitionCapabilityRecord, MetafieldDefinitionConstraintValueRecord,
  MetafieldDefinitionConstraintsRecord, MetafieldDefinitionRecord,
  MetafieldDefinitionTypeRecord, MetafieldDefinitionValidationRecord,
  MetaobjectBool, MetaobjectCapabilitiesRecord,
  MetaobjectDefinitionCapabilitiesRecord, MetaobjectDefinitionCapabilityRecord,
  MetaobjectDefinitionRecord, MetaobjectDefinitionTypeRecord,
  MetaobjectFieldDefinitionRecord, MetaobjectFieldDefinitionReferenceRecord,
  MetaobjectFieldDefinitionValidationRecord, MetaobjectFieldRecord,
  MetaobjectFloat, MetaobjectInt, MetaobjectList, MetaobjectNull,
  MetaobjectObject, MetaobjectOnlineStoreCapabilityRecord,
  MetaobjectPublishableCapabilityRecord, MetaobjectRecord,
  MetaobjectStandardTemplateRecord, MetaobjectString, Money,
  OnlineStoreContentRecord, OrderRecord, PaymentSettingsRecord, PriceListRecord,
  ProductCategoryRecord, ProductCollectionRecord, ProductMediaRecord,
  ProductMetafieldRecord, ProductOptionRecord, ProductOptionValueRecord,
  ProductRecord, ProductSeoRecord, ProductVariantRecord,
  ProductVariantSelectedOptionRecord, PublicationRecord, SegmentRecord,
  SellingPlanGroupRecord, SellingPlanRecord, ShippingOrderRecord,
  ShippingPackageDimensionsRecord, ShippingPackageRecord,
  ShippingPackageWeightRecord, ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopPolicyRecord,
  ShopRecord, ShopResourceLimitsRecord, ShopifyFunctionAppRecord,
  ShopifyFunctionRecord, StoreCreditAccountRecord, StorePropertyBool,
  StorePropertyFloat, StorePropertyInt, StorePropertyList,
  StorePropertyMutationPayloadRecord, StorePropertyNull, StorePropertyObject,
  StorePropertyRecord, StorePropertyString, TranslationRecord, WebPresenceRecord,
}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromPreviousProxyPath` substitution path didn't resolve.
  PreviousRefUnresolved(path: String)
  /// `fromProxyResponse` substitution target/path didn't resolve.
  ProxyResponseRefUnresolved(target: String, path: String)
  /// `fromCapturePath` substitution path didn't resolve.
  CaptureRefUnresolved(path: String)
  /// Capture JSONPath did not resolve for a target.
  CaptureUnresolved(target: String, path: String)
  /// Proxy response JSONPath did not resolve for a target.
  ProxyUnresolved(target: String, path: String)
  /// Proxy returned a non-200 status.
  ProxyStatus(target: String, status: Int, body: String)
}

pub type TargetReport {
  TargetReport(
    name: String,
    capture_path: String,
    proxy_path: String,
    mismatches: List(Mismatch),
  )
}

pub type Report {
  Report(scenario_id: String, targets: List(TargetReport))
}

pub type RunnerConfig {
  RunnerConfig(repo_root: String)
}

type SeedMarketingRecords {
  SeedMarketingRecords(
    activities: List(MarketingRecord),
    events: List(MarketingRecord),
  )
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..")
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  use capture <- result.try(load_capture(config, parsed))
  use primary_doc <- result.try(
    read_file(resolve(config, parsed.proxy_request.document_path)),
  )
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    None,
    dict.new(),
    "<primary>",
  ))
  let primary_vars = replace_customer_one_variables(capture, primary_vars)
  let proxy = draft_proxy.new()
  let proxy = seed_capture_preconditions(parsed, capture, proxy)
  use #(primary_response, proxy) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
    parsed.proxy_request.api_version,
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  Ok(Report(scenario_id: parsed.scenario_id, targets: target_reports))
}

/// Capture-driven seeding dispatch.
///
/// Each helper in this chain inspects the capture for its own marker
/// paths and either seeds or no-ops. Helpers MUST self-gate on capture
/// shape — never on `parsed.scenario_id`. This lets new parity specs
/// land without touching runner code: if a capture exposes the markers
/// a helper expects, that helper fires.
fn seed_capture_preconditions(
  parsed: Spec,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let helpers = [
    seed_captured_products_preconditions,
    seed_discount_preconditions,
    seed_selling_plan_group_preconditions,
    seed_product_media_preconditions,
    seed_file_delete_product_media_preconditions,
    seed_gift_card_lifecycle_preconditions,
    seed_shopify_function_preconditions,
    seed_shop_preconditions,
    seed_business_entity_preconditions,
    seed_location_detail_preconditions,
    seed_location_lifecycle_preconditions,
    seed_publishable_preconditions,
    seed_product_preconditions,
    seed_pre_mutation_product_preconditions,
    seed_product_publication_preconditions,
    seed_product_feedback_preconditions,
    seed_product_metafields_preconditions,
    seed_product_delete_preconditions,
    seed_product_update_preconditions,
    seed_product_duplicate_async_preconditions,
    seed_product_duplicate_preconditions,
    seed_product_set_preconditions,
    seed_tags_remove_preconditions,
    seed_product_relationship_roots_preconditions,
    seed_product_create_media_plan_preconditions,
    seed_product_update_media_plan_preconditions,
    seed_product_delete_media_plan_preconditions,
    seed_product_reorder_media_preconditions,
    seed_product_variant_create_preconditions,
    seed_product_variants_bulk_create_preconditions,
    seed_product_variants_bulk_validation_atomicity_preconditions,
    seed_product_variant_update_preconditions,
    seed_product_variants_bulk_update_preconditions,
    seed_product_variant_delete_preconditions,
    seed_product_variants_bulk_reorder_preconditions,
    seed_product_variants_read_preconditions,
    seed_products_catalog_preconditions,
    fn(c, p) { seed_markets_preconditions(parsed.scenario_id, c, p) },
    seed_products_search_read_preconditions,
    seed_captured_product_connections_preconditions,
    seed_products_sort_keys_preconditions,
    seed_products_search_pagination_preconditions,
    seed_collection_detail_preconditions,
    seed_collections_catalog_preconditions,
    seed_collection_add_products_preconditions,
    seed_collection_remove_products_preconditions,
    seed_collection_reorder_products_preconditions,
    seed_collection_update_preconditions,
    seed_collection_delete_preconditions,
    seed_collection_create_initial_products_preconditions,
    seed_locations_catalog_preconditions,
    seed_publications_catalog_preconditions,
    seed_publication_roots_preconditions,
    seed_inventory_quantity_roots_preconditions,
    seed_inventory_quantity_contracts_preconditions,
    seed_inventory_adjust_quantities_preconditions,
    seed_inventory_activate_preconditions,
    seed_inventory_inactive_lifecycle_preconditions,
    seed_inventory_item_update_preconditions,
    seed_inventory_shipment_preconditions,
    seed_inventory_transfer_preconditions,
    seed_metafields_set_preconditions,
    seed_metafields_delete_preconditions,
    seed_metafield_definition_preconditions,
    seed_metaobject_preconditions,
    seed_marketing_baseline_preconditions,
    seed_online_store_content_preconditions,
    fn(c, p) {
      case parsed.scenario_id {
        "shipping-package-default-lifecycle-local-runtime" ->
          seed_shipping_package_preconditions(c, p)
        "shipping-settings-package-pickup-constraints" ->
          seed_shipping_settings_package_pickup_preconditions(c, p)
        "delivery-profile-read" ->
          seed_delivery_profile_read_preconditions(c, p)
        "delivery-profile-lifecycle" ->
          seed_delivery_profile_lifecycle_preconditions(c, p)
        "fulfillment-top-level-reads" | "fulfillment-detail-events-lifecycle" ->
          seed_fulfillment_read_preconditions(c, p)
        "assigned-fulfillment-orders-filtering-local-runtime" ->
          seed_assigned_fulfillment_orders_preconditions(c, p)
        "fulfillment-order-request-lifecycle" ->
          seed_fulfillment_order_request_preconditions(c, p)
        "fulfillment-order-lifecycle-local-staging" ->
          seed_fulfillment_order_lifecycle_preconditions(c, p)
        _ -> p
      }
    },
    fn(c, p) {
      case parsed.scenario_id {
        "segments-baseline-read" -> seed_segments_baseline_preconditions(c, p)
        _ -> p
      }
    },
    seed_b2b_company_roots_preconditions,
    seed_localization_disable_cleanup_preconditions,
    seed_localization_locale_translation_preconditions,
    seed_store_credit_preconditions,
    seed_customer_payment_method_preconditions,
    seed_payment_terms_preconditions,
    seed_customer_count_baseline,
    seed_customer_delete_preconditions,
    seed_customer_input_validation_preconditions,
    seed_customer_consent_preconditions,
    seed_customer_merge_preconditions,
    seed_customer_order_summary_preconditions,
    fn(c, p) { seed_customer_preconditions(parsed, c, p) },
    fn(c, p) { seed_orders_capture_preconditions(parsed.scenario_id, c, p) },
  ]
  list.fold(helpers, proxy, fn(p, helper) { helper(capture, p) })
}

fn seed_online_store_content_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    list.append(
      list.append(
        online_store_connection_nodes(
          capture,
          "$.interactions[0].response.data.blogs.edges",
        )
          |> list.filter_map(make_online_store_seed_content(_, "blog")),
        online_store_connection_nodes(
          capture,
          "$.interactions[0].response.data.pages.edges",
        )
          |> list.filter_map(make_online_store_seed_content(_, "page")),
      ),
      online_store_payload_records(capture, [
        #("$.interactions[0].response.data.blogCreate.blog", "blog"),
        #("$.interactions[1].response.data.pageCreate.page", "page"),
        #("$.interactions[2].response.data.articleCreate.article", "article"),
      ]),
    )

  case records {
    [] -> proxy
    records -> {
      let seeded_store =
        store_mod.upsert_base_online_store_content(proxy.store, records)
      draft_proxy.DraftProxy(..proxy, store: seeded_store)
    }
  }
}

fn online_store_payload_records(
  capture: JsonValue,
  paths: List(#(String, String)),
) -> List(OnlineStoreContentRecord) {
  list.filter_map(paths, fn(pair) {
    let #(path, kind) = pair
    case jsonpath.lookup(capture, path) {
      Some(source) -> make_online_store_seed_content(source, kind)
      None -> Error(Nil)
    }
  })
}

fn online_store_connection_nodes(
  capture: JsonValue,
  path: String,
) -> List(JsonValue) {
  case jsonpath.lookup(capture, path) {
    Some(JArray(edges)) ->
      list.filter_map(edges, fn(edge) {
        case json_value.field(edge, "node") {
          Some(node) -> Ok(node)
          None -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn make_online_store_seed_content(
  source: JsonValue,
  kind: String,
) -> Result(OnlineStoreContentRecord, Nil) {
  case read_string_field(source, "id") {
    Some(id) ->
      Ok(OnlineStoreContentRecord(
        id: id,
        kind: kind,
        cursor: None,
        parent_id: online_store_parent_id(source, kind),
        created_at: read_string_field(source, "createdAt"),
        updated_at: read_string_field(source, "updatedAt"),
        data: captured_json_from_parity(source),
      ))
    None -> Error(Nil)
  }
}

fn online_store_parent_id(source: JsonValue, kind: String) -> Option(String) {
  case kind {
    "article" ->
      case read_object_field(source, "blog") {
        Some(blog) -> read_string_field(blog, "id")
        None -> None
      }
    _ -> None
  }
}

fn seed_orders_capture_preconditions(
  scenario_id: String,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case scenario_id {
    "order-edit-begin-live-parity"
    | "order-edit-add-variant-live-parity"
    | "order-edit-set-quantity-live-parity"
    | "order-edit-existing-order-validation"
    | "order-edit-commit-live-parity"
    | "order-edit-existing-order-happy-path"
    | "order-edit-existing-order-zero-removal"
    | "order-edit-residual-workflow-calculated-edits" ->
      seed_order_edit_existing_order_preconditions(capture, proxy)
    "draft-order-detail-read" ->
      seed_draft_order_detail_preconditions(capture, proxy)
    "draft-order-create-live-parity" ->
      seed_draft_order_create_preconditions(capture, proxy)
    "draft-order-create-from-order-live-parity" ->
      seed_draft_order_create_from_order_preconditions(capture, proxy)
    "draft-order-complete-live-parity" ->
      seed_draft_order_complete_preconditions(capture, proxy)
    "draft-order-delete-live-parity" ->
      seed_draft_order_delete_preconditions(capture, proxy)
    "draft-order-duplicate-live-parity" ->
      seed_draft_order_duplicate_preconditions(capture, proxy)
    "draft-order-invoice-send-safety" ->
      seed_draft_order_invoice_send_preconditions(capture, proxy)
    "draft-order-update-live-parity" ->
      seed_draft_order_update_preconditions(capture, proxy)
    "draft-orders-catalog-read"
    | "draft-orders-count-read"
    | "draft-orders-invalid-email-query-read" ->
      seed_draft_orders_catalog_preconditions(capture, proxy)
    "order-merchant-detail-read" ->
      seed_order_merchant_detail_preconditions(capture, proxy)
    "order-empty-state-read" -> seed_order_catalog_preconditions(capture, proxy)
    "order-catalog-count-read" ->
      seed_order_catalog_count_preconditions(capture, proxy)
    "fulfillment-cancel-live-parity"
    | "fulfillment-tracking-info-update-live-parity" ->
      seed_order_downstream_preconditions(capture, proxy)
    "refund-create-full-parity"
    | "refund-create-live-parity"
    | "refund-create-over-refund-user-errors" ->
      seed_order_create_setup_preconditions(capture, proxy)
    "orderCancel-live-parity" ->
      seed_order_downstream_preconditions(capture, proxy)
    "orderOpen-live-parity" ->
      seed_order_management_preconditions(capture, proxy, "orderOpen")
    "orderClose-live-parity" ->
      seed_order_management_preconditions(capture, proxy, "orderClose")
    "orderInvoiceSend-live-parity" ->
      seed_order_management_preconditions(capture, proxy, "orderInvoiceSend")
    "orderMarkAsPaid-live-parity" ->
      seed_order_management_preconditions(capture, proxy, "orderMarkAsPaid")
    "order-update-live-parity" | "order-update-expanded-live-parity" ->
      seed_order_downstream_preconditions(capture, proxy)
    "orderCustomerSet-live-parity" | "orderCustomerRemove-live-parity" ->
      seed_order_customer_preconditions(capture, proxy)
    "return-lifecycle-local-staging"
    | "removeFromReturn-local-staging"
    | "return-request-decline-local-staging"
    | "return-reverse-logistics-local-staging"
    | "return-reverse-logistics-recorded" ->
      seed_return_lifecycle_preconditions(capture, proxy)
    _ -> proxy
  }
}

/// Broad walker — collects every customer-shaped object in the capture
/// and seeds them. Fires for any capture in a customer-flavoured scenario
/// (per spec `operationNames`) EXCEPT captures that own the lifecycle of
/// a fresh customer (create, set, address-lifecycle, input-addresses,
/// input-inline-consent, input-validation matrices, order-summary
/// effects). For those, the scenario-specific helper above does the
/// right narrow seeding — pre-seeding the response customer here would
/// either duplicate it in the store or cause the proxy's create to land
/// on top of a pre-existing record.
///
/// The `operationNames` gate keeps the broad walker from firing for
/// non-customer captures that nonetheless contain `precision`+`count`
/// pairs (e.g. productsCount: 13552), which would otherwise produce
/// thousands of placeholder customers.
///
/// Helpers above (consent, merge, input-validation, …) call the
/// `_unchecked` variant on a narrowed subtree, bypassing this gate.
fn seed_localization_disable_cleanup_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.disableCleanupLifecycle") {
    Some(source) -> {
      case
        read_string_field(source, "resourceId"),
        read_string_field(source, "titleDigest")
      {
        Some(resource_id), Some(title_digest) -> {
          let #(_, seeded_store) =
            store_mod.stage_translation(
              proxy.store,
              TranslationRecord(
                resource_id: resource_id,
                key: "title",
                locale: "__source",
                value: "",
                translatable_content_digest: title_digest,
                market_id: None,
                updated_at: read_string_field(capture, "capturedAt")
                  |> option.unwrap("1970-01-01T00:00:00Z"),
                outdated: False,
              ),
            )
          draft_proxy.DraftProxy(..proxy, store: seeded_store)
        }
        _, _ -> proxy
      }
    }
    None -> proxy
  }
}

fn seed_localization_locale_translation_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let store_with_locales = case
    jsonpath.lookup(
      capture,
      "$.readCapture.response.data.availableLocalesExcerpt",
    )
  {
    Some(JArray(locales)) -> {
      let records =
        locales
        |> list.filter_map(fn(locale) {
          case
            read_string_field(locale, "isoCode"),
            read_string_field(locale, "name")
          {
            Some(iso_code), Some(name) ->
              Ok(LocaleRecord(iso_code: iso_code, name: name))
            _, _ -> Error(Nil)
          }
        })
      store_mod.replace_base_available_locales(proxy.store, records)
    }
    _ -> proxy.store
  }
  let seeded_store = case
    jsonpath.lookup(capture, "$.readCapture.response.data.resources.nodes")
  {
    Some(JArray(resources)) ->
      resources
      |> list.flat_map(localization_source_markers)
      |> list.fold(store_with_locales, fn(current_store, marker) {
        let #(_, next_store) =
          store_mod.stage_translation(current_store, marker)
        next_store
      })
    _ -> store_with_locales
  }
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn localization_source_markers(resource: JsonValue) -> List(TranslationRecord) {
  case read_string_field(resource, "resourceId") {
    Some(resource_id) -> {
      let content = case read_array_field(resource, "translatableContent") {
        Some(entries) -> entries
        None -> []
      }
      content
      |> list.filter_map(fn(entry) {
        case
          read_string_field(entry, "key"),
          read_string_field(entry, "value"),
          read_string_field(entry, "digest")
        {
          Some(key), Some(value), Some(digest) ->
            Ok(TranslationRecord(
              resource_id: resource_id,
              key: key,
              locale: "__source",
              value: value,
              translatable_content_digest: digest,
              market_id: None,
              updated_at: "1970-01-01T00:00:00Z",
              outdated: False,
            ))
          _, _, _ -> Error(Nil)
        }
      })
    }
    None -> []
  }
}

fn seed_customer_preconditions(
  parsed: Spec,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    operation_names_indicate_customer_scenario(parsed.operation_names)
    && !capture_creates_fresh_customer(capture)
  {
    True -> seed_customer_preconditions_unchecked(capture, proxy)
    False -> proxy
  }
}

/// True if any operation name on the spec is a customer-flavoured root
/// (`customer*` or `customers*`). The TS suite's denylist of
/// non-customer scenarios maps directly onto "no operation name starts
/// with customer".
fn operation_names_indicate_customer_scenario(names: List(String)) -> Bool {
  list.any(names, fn(name) {
    string.starts_with(name, "customer")
    || string.starts_with(name, "customers")
  })
}

/// True if the capture's *primary* mutation/stage produces a freshly
/// created customer whose response payload must NOT be pre-seeded into
/// the store. The path list mirrors the scenarios that the original
/// `is_customer_seeded_scenario` denylist excluded.
fn capture_creates_fresh_customer(capture: JsonValue) -> Bool {
  capture_has_any_path(capture, [
    // customer-create-live-parity, customer-input-inline-consent-parity
    "$.mutation.response.data.customerCreate.customer.id",
    // customer-set-live-parity
    "$.mutation.response.data.customerSet.customer.id",
    // customer-address-lifecycle-parity
    "$.createCustomer.response.data.customerCreate.customer.id",
    // customer-input-addresses-parity
    "$.create.response.data.customerCreate.customer.id",
    // customer-input-validation-parity
    "$.createScenarios",
    // customer-order-summary-read-effects
    "$.seedOrder",
    // consent / merge / delete have dedicated helpers — broad walker would
    // double-seed against post-mutation state and corrupt the baseline.
    "$.mutation.response.data.customerEmailMarketingConsentUpdate",
    "$.mutation.response.data.customerSmsMarketingConsentUpdate",
    "$.mutation.response.data.customerMerge",
    "$.preview.response.data.customerMergePreview",
    "$.mutation.response.data.customerDelete",
  ])
}

fn seed_customer_preconditions_unchecked(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let customers = collect_seed_customers(capture)
  let customer_count_baseline = max_customer_count_baseline(capture)
  let placeholder_customers = case customer_count_baseline {
    Some(count) ->
      make_placeholder_customers(int.max(0, count - list.length(customers)), 1)
    None -> []
  }
  let addresses = collect_seed_customer_addresses(capture)
  let order_summaries = collect_seed_customer_order_summaries(capture)
  let order_page_infos = collect_seed_customer_order_page_infos(capture)
  let event_summaries = collect_seed_customer_event_summaries(capture)
  let event_page_infos = collect_seed_customer_event_page_infos(capture)
  let last_orders = collect_seed_customer_last_orders(capture)
  let metafields = collect_seed_customer_metafields(capture)
  let pages = collect_seed_customer_account_pages(capture)
  let connections = collect_seed_customer_connections(capture)
  let store =
    proxy.store
    |> store_mod.upsert_base_customers(list.append(
      customers,
      placeholder_customers,
    ))
    |> store_mod.upsert_base_customer_addresses(addresses)
    |> store_mod.upsert_base_customer_order_summaries(order_summaries)
    |> seed_customer_order_page_infos(order_page_infos)
    |> store_mod.upsert_base_customer_event_summaries(event_summaries)
    |> seed_customer_event_page_infos(event_page_infos)
    |> store_mod.upsert_base_customer_last_orders(last_orders)
    |> seed_customer_metafields(metafields)
    |> store_mod.upsert_base_customer_account_pages(pages)
    |> seed_customer_connections(connections)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_customer_payment_method_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let customers =
    jsonpath.lookup(capture, "$.seedCustomers")
    |> json_array_values
    |> list.filter_map(make_seed_customer)
  let payment_methods =
    jsonpath.lookup(capture, "$.seedCustomerPaymentMethods")
    |> json_array_values
    |> list.filter_map(make_seed_customer_payment_method)
  case customers, payment_methods {
    [], [] -> proxy
    _, _ -> {
      let store =
        proxy.store
        |> store_mod.upsert_base_customers(customers)
        |> store_mod.upsert_base_customer_payment_methods(payment_methods)
      let #(_, identity_after_seed) =
        synthetic_identity.make_synthetic_gid(
          proxy.synthetic_identity,
          "CustomerPaymentMethodSeed",
        )
      draft_proxy.DraftProxy(
        ..proxy,
        store: store,
        synthetic_identity: identity_after_seed,
      )
    }
  }
}

fn seed_payment_terms_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let owner_id =
    jsonpath.lookup(capture, "$.seedDraftOrder.id")
    |> option.then(json_string_value)
  let store = case owner_id {
    Some(id) -> store_mod.register_payment_terms_owner(proxy.store, id)
    None -> proxy.store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn json_array_values(value: Option(JsonValue)) -> List(JsonValue) {
  case value {
    Some(JArray(items)) -> items
    _ -> []
  }
}

fn seed_draft_order_create_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let proxy = seed_customer_preconditions_unchecked(capture, proxy)
  let catalog = collect_draft_order_variant_catalog(capture)
  let store =
    proxy.store |> store_mod.upsert_base_draft_order_variant_catalog(catalog)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_draft_order_detail_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.response.data.draftOrder") {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> {
          let store =
            proxy.store |> store_mod.upsert_base_draft_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_draft_orders_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = collect_draft_order_catalog_records(capture)
  let desired_count = draft_order_catalog_count(capture, list.length(records))
  let padded_records =
    list.append(
      records,
      draft_order_placeholder_records(
        desired_count - list.length(records),
        list.length(records),
      ),
    )
  let store = proxy.store |> store_mod.upsert_base_draft_orders(padded_records)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn collect_draft_order_catalog_records(
  capture: JsonValue,
) -> List(DraftOrderRecord) {
  case jsonpath.lookup(capture, "$.response.data.draftOrders.edges") {
    Some(JArray(edges)) ->
      edges
      |> list.filter_map(fn(edge) {
        case make_seed_draft_order_edge(edge) {
          Ok(record) -> Ok(record)
          Error(_) -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn make_seed_draft_order_edge(
  edge: JsonValue,
) -> Result(DraftOrderRecord, Nil) {
  use node <- result.try(read_object_field(edge, "node") |> option_to_result())
  use id <- result.try(required_gid(node, "id", "DraftOrder"))
  Ok(DraftOrderRecord(
    id: id,
    cursor: read_string_field(edge, "cursor"),
    data: captured_json_from_parity(node),
  ))
}

fn draft_order_catalog_count(capture: JsonValue, edge_count: Int) -> Int {
  case jsonpath.lookup(capture, "$.response.data.draftOrdersCount.count") {
    Some(JInt(count)) -> count
    _ -> {
      let has_next =
        jsonpath.lookup(
          capture,
          "$.response.data.draftOrders.pageInfo.hasNextPage",
        )
      case has_next {
        Some(JBool(True)) -> edge_count + 1
        _ -> edge_count
      }
    }
  }
}

fn draft_order_placeholder_records(
  count: Int,
  offset: Int,
) -> List(DraftOrderRecord) {
  case count <= 0 {
    True -> []
    False ->
      int_sequence(1, count)
      |> list.map(fn(index) {
        let id_number = 9_900_000_000_000 + offset + index
        let id = "gid://shopify/DraftOrder/" <> int.to_string(id_number)
        DraftOrderRecord(
          id: id,
          cursor: Some(id),
          data: CapturedObject([
            #("id", CapturedString(id)),
            #("name", CapturedString("#D" <> int.to_string(offset + index))),
            #("status", CapturedString("OPEN")),
            #("ready", CapturedBool(True)),
            #(
              "email",
              CapturedString(
                "placeholder-draft-order-"
                <> int.to_string(offset + index)
                <> "@example.test",
              ),
            ),
            #("tags", CapturedArray([])),
            #("createdAt", CapturedString("2026-01-01T00:00:00Z")),
            #("updatedAt", CapturedString("2026-01-01T00:00:00Z")),
            #(
              "totalPriceSet",
              CapturedObject([
                #(
                  "shopMoney",
                  CapturedObject([
                    #("amount", CapturedString("0.0")),
                    #("currencyCode", CapturedString("CAD")),
                  ]),
                ),
              ]),
            ),
          ]),
        )
      })
  }
}

fn int_sequence(current: Int, last: Int) -> List(Int) {
  case current > last {
    True -> []
    False -> [current, ..int_sequence(current + 1, last)]
  }
}

fn seed_draft_order_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.variables.input.id") {
    Some(JString(id)) -> {
      let record =
        DraftOrderRecord(
          id: id,
          cursor: None,
          data: CapturedObject([#("id", CapturedString(id))]),
        )
      let store = proxy.store |> store_mod.upsert_base_draft_orders([record])
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _ -> proxy
  }
}

fn seed_draft_order_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder",
    )
  {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> {
          let store =
            proxy.store |> store_mod.upsert_base_draft_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_draft_order_complete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder",
    )
  {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> {
          let record = draft_order_with_setup_note(capture, record)
          let store =
            proxy.store |> store_mod.upsert_base_draft_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_draft_order_create_from_order_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder",
    )
  {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> {
          let record = draft_order_with_create_from_order_setup(capture, record)
          let store =
            proxy.store |> store_mod.upsert_base_draft_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn draft_order_with_create_from_order_setup(
  capture: JsonValue,
  record: DraftOrderRecord,
) -> DraftOrderRecord {
  let complete_path =
    "$.setup.draftOrderComplete.mutation.response.data.draftOrderComplete.draftOrder"
  let data =
    record.data
    |> upsert_captured_json_field_from_path(
      capture,
      complete_path <> ".status",
      "status",
    )
    |> upsert_captured_json_field_from_path(
      capture,
      complete_path <> ".completedAt",
      "completedAt",
    )
    |> upsert_captured_json_field_from_path(
      capture,
      complete_path <> ".order",
      "order",
    )
  DraftOrderRecord(..record, data: data)
}

fn upsert_captured_json_field_from_path(
  value: CapturedJsonValue,
  capture: JsonValue,
  path: String,
  name: String,
) -> CapturedJsonValue {
  case jsonpath.lookup(capture, path) {
    Some(replacement) ->
      upsert_captured_json_field(
        value,
        name,
        captured_json_from_parity(replacement),
      )
    None -> value
  }
}

fn seed_draft_order_duplicate_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder",
    )
  {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> {
          let store =
            proxy.store |> store_mod.upsert_base_draft_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_draft_order_invoice_send_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    []
    |> list.append(seed_draft_orders_at_path(
      capture,
      "$.recipient.openNoRecipient.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder",
    ))
    |> list.append(seed_draft_orders_at_path(
      capture,
      "$.lifecycle.completedNoRecipient.setup.draftOrderComplete.mutation.response.data.draftOrderComplete.draftOrder",
    ))
  let store = proxy.store |> store_mod.upsert_base_draft_orders(records)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_draft_orders_at_path(
  capture: JsonValue,
  path: String,
) -> List(DraftOrderRecord) {
  case jsonpath.lookup(capture, path) {
    Some(source) ->
      case make_seed_draft_order(source) {
        Ok(record) -> [record]
        Error(_) -> []
      }
    None -> []
  }
}

fn make_seed_draft_order(source: JsonValue) -> Result(DraftOrderRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "DraftOrder"))
  Ok(DraftOrderRecord(
    id: id,
    cursor: None,
    data: captured_json_from_parity(source),
  ))
}

fn seed_order_merchant_detail_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.response.data.order") {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn make_seed_order(source: JsonValue) -> Result(OrderRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "Order"))
  Ok(OrderRecord(id: id, cursor: None, data: captured_json_from_parity(source)))
}

fn seed_order_management_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
  root_name: String,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data." <> root_name <> ".order",
    )
  {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_order_downstream_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.response.data.order") {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_order_create_setup_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.orderCreate.response.data.orderCreate.order",
    )
  {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> seed_order_downstream_preconditions(capture, proxy)
  }
}

fn seed_order_edit_existing_order_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.seedOrder") {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_return_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.seedOrder") {
    Some(source) ->
      case make_seed_order(source) {
        Ok(record) -> {
          let store = proxy.store |> store_mod.upsert_base_orders([record])
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_order_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = collect_order_catalog_records(capture)
  let desired_count = order_catalog_count(capture, list.length(records))
  let padded_records =
    list.append(
      records,
      order_placeholder_records(
        desired_count - list.length(records),
        list.length(records),
      ),
    )
  let store = proxy.store |> store_mod.upsert_base_orders(padded_records)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn collect_order_catalog_records(capture: JsonValue) -> List(OrderRecord) {
  case jsonpath.lookup(capture, "$.response.data.orders.edges") {
    Some(JArray(edges)) ->
      edges
      |> list.filter_map(fn(edge) {
        case make_seed_order_edge(edge) {
          Ok(record) -> Ok(record)
          Error(_) -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn make_seed_order_edge(edge: JsonValue) -> Result(OrderRecord, Nil) {
  use node <- result.try(read_object_field(edge, "node") |> option_to_result())
  use id <- result.try(required_gid(node, "id", "Order"))
  Ok(OrderRecord(
    id: id,
    cursor: read_string_field(edge, "cursor"),
    data: captured_json_from_parity(node),
  ))
}

fn order_catalog_count(capture: JsonValue, edge_count: Int) -> Int {
  case jsonpath.lookup(capture, "$.response.data.ordersCount.count") {
    Some(JInt(count)) -> count
    _ -> {
      let has_next =
        jsonpath.lookup(capture, "$.response.data.orders.pageInfo.hasNextPage")
      case has_next {
        Some(JBool(True)) -> edge_count + 1
        _ -> edge_count
      }
    }
  }
}

fn order_placeholder_records(count: Int, offset: Int) -> List(OrderRecord) {
  case count <= 0 {
    True -> []
    False ->
      int_sequence(1, count)
      |> list.map(fn(index) {
        let id_number = 9_800_000_000_000 + offset + index
        let id = "gid://shopify/Order/" <> int.to_string(id_number)
        OrderRecord(
          id: id,
          cursor: Some(id),
          data: CapturedObject([
            #("id", CapturedString(id)),
            #("name", CapturedString("#" <> int.to_string(offset + index))),
            #("tags", CapturedArray([])),
            #("createdAt", CapturedString("2026-01-01T00:00:00Z")),
            #("updatedAt", CapturedString("2026-01-01T00:00:00Z")),
            #("displayFinancialStatus", CapturedString("PAID")),
            #("displayFulfillmentStatus", CapturedString("UNFULFILLED")),
            #("note", CapturedNull),
            #(
              "currentTotalPriceSet",
              CapturedObject([
                #(
                  "shopMoney",
                  CapturedObject([
                    #("amount", CapturedString("0.0")),
                    #("currencyCode", CapturedString("CAD")),
                  ]),
                ),
              ]),
            ),
          ]),
        )
      })
  }
}

fn seed_order_catalog_count_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    []
    |> list.append(order_records_from_nodes_path(
      capture,
      "$.response.data.seedCatalog.nodes",
    ))
    |> list.append(order_records_from_nodes_path(
      capture,
      "$.response.data.byStatus.nodes",
    ))
    |> list.append(order_records_from_nodes_path(
      capture,
      "$.nextPage.response.data.nextPage.nodes",
    ))
    |> dedupe_seed_orders()
  let store = proxy.store |> store_mod.upsert_base_orders(records)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn order_records_from_nodes_path(
  capture: JsonValue,
  path: String,
) -> List(OrderRecord) {
  case jsonpath.lookup(capture, path) {
    Some(JArray(nodes)) ->
      nodes
      |> list.filter_map(fn(node) {
        case make_seed_order_node(capture, node) {
          Ok(record) -> Ok(record)
          Error(_) -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn make_seed_order_node(
  capture: JsonValue,
  node: JsonValue,
) -> Result(OrderRecord, Nil) {
  use id <- result.try(required_gid(node, "id", "Order"))
  Ok(OrderRecord(
    id: id,
    cursor: order_catalog_cursor_for_id(capture, id),
    data: captured_json_from_parity(node),
  ))
}

fn order_catalog_cursor_for_id(
  capture: JsonValue,
  id: String,
) -> Option(String) {
  [
    "$.response.data.recent",
    "$.response.data.oldest",
    "$.response.data.byName",
    "$.response.data.byStatus",
    "$.nextPage.response.data.nextPage",
  ]
  |> list.find_map(fn(path) {
    order_connection_cursor_for_id(capture, path, id)
  })
  |> option.from_result
}

fn order_connection_cursor_for_id(
  capture: JsonValue,
  path: String,
  id: String,
) -> Result(String, Nil) {
  case jsonpath.lookup(capture, path <> ".nodes") {
    Some(JArray(nodes)) -> {
      let matching_index =
        nodes
        |> list.index_map(fn(node, index) { #(node, index) })
        |> list.find_map(fn(pair) {
          let #(node, index) = pair
          case required_gid(node, "id", "Order") {
            Ok(node_id) if node_id == id -> Ok(index)
            _ -> Error(Nil)
          }
        })
      case matching_index {
        Ok(0) -> read_string_jsonpath(capture, path <> ".pageInfo.startCursor")
        Ok(index) ->
          case index == list.length(nodes) - 1 {
            True -> read_string_jsonpath(capture, path <> ".pageInfo.endCursor")
            False -> Error(Nil)
          }
        _ -> Error(Nil)
      }
    }
    _ -> Error(Nil)
  }
}

fn read_string_jsonpath(
  capture: JsonValue,
  path: String,
) -> Result(String, Nil) {
  case jsonpath.lookup(capture, path) {
    Some(JString(value)) -> Ok(value)
    _ -> Error(Nil)
  }
}

fn dedupe_seed_orders(records: List(OrderRecord)) -> List(OrderRecord) {
  let initial: List(OrderRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn draft_order_with_setup_note(
  capture: JsonValue,
  record: DraftOrderRecord,
) -> DraftOrderRecord {
  case
    jsonpath.lookup(capture, "$.setup.draftOrderCreate.variables.input.note")
  {
    Some(JString(note)) ->
      DraftOrderRecord(
        ..record,
        data: upsert_captured_json_field(
          record.data,
          "note",
          CapturedString(note),
        ),
      )
    _ -> record
  }
}

fn upsert_captured_json_field(
  value: CapturedJsonValue,
  name: String,
  replacement: CapturedJsonValue,
) -> CapturedJsonValue {
  case value {
    CapturedObject(fields) ->
      CapturedObject(upsert_captured_json_fields(fields, name, replacement))
    _ -> CapturedObject([#(name, replacement)])
  }
}

fn upsert_captured_json_fields(
  fields: List(#(String, CapturedJsonValue)),
  name: String,
  replacement: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  case list.any(fields, fn(pair) { pair.0 == name }) {
    True ->
      list.map(fields, fn(pair) {
        case pair.0 == name {
          True -> #(name, replacement)
          False -> pair
        }
      })
    False -> list.append(fields, [#(name, replacement)])
  }
}

fn collect_draft_order_variant_catalog(
  capture: JsonValue,
) -> List(DraftOrderVariantCatalogRecord) {
  collect_objects(capture)
  |> list.filter_map(make_draft_order_variant_catalog)
  |> dedupe_draft_order_variant_catalog([])
}

fn make_draft_order_variant_catalog(
  source: JsonValue,
) -> Result(DraftOrderVariantCatalogRecord, Nil) {
  use variant <- result.try(
    read_object_field(source, "variant") |> option_to_result(),
  )
  use variant_id <- result.try(required_gid(variant, "id", "ProductVariant"))
  let title = read_string_field(source, "title") |> option.unwrap("Variant")
  let name = read_string_field(source, "name") |> option.unwrap(title)
  let shop_money =
    read_object_field(source, "originalUnitPriceSet")
    |> option.then(read_object_field(_, "shopMoney"))
  let unit_price =
    read_string_field_from_option(shop_money, "amount")
    |> option.unwrap("0.0")
  let currency_code =
    read_string_field_from_option(shop_money, "currencyCode")
    |> option.unwrap("CAD")
  Ok(DraftOrderVariantCatalogRecord(
    variant_id: variant_id,
    title: title,
    name: name,
    variant_title: read_string_field(variant, "title"),
    sku: read_string_field(source, "sku"),
    requires_shipping: read_bool_field(source, "requiresShipping")
      |> option.unwrap(True),
    taxable: read_bool_field(source, "taxable") |> option.unwrap(True),
    unit_price: unit_price,
    currency_code: currency_code,
  ))
}

fn dedupe_draft_order_variant_catalog(
  records: List(DraftOrderVariantCatalogRecord),
  seen: List(String),
) -> List(DraftOrderVariantCatalogRecord) {
  case records {
    [] -> []
    [record, ..rest] ->
      case list.contains(seen, record.variant_id) {
        True -> dedupe_draft_order_variant_catalog(rest, seen)
        False -> [
          record,
          ..dedupe_draft_order_variant_catalog(rest, [record.variant_id, ..seen])
        ]
      }
  }
}

fn seed_customer_connections(
  store: store_mod.Store,
  connections: List(#(String, CustomerCatalogConnectionRecord)),
) -> store_mod.Store {
  list.fold(connections, store, fn(acc, pair) {
    let #(key, connection) = pair
    store_mod.set_base_customer_catalog_connection(acc, key, connection)
  })
}

fn seed_customer_order_page_infos(
  store: store_mod.Store,
  page_infos: List(#(String, CustomerCatalogPageInfoRecord)),
) -> store_mod.Store {
  list.fold(page_infos, store, fn(acc, pair) {
    let #(customer_id, page_info) = pair
    store_mod.set_base_customer_order_connection_page_info(
      acc,
      customer_id,
      page_info,
    )
  })
}

fn seed_customer_event_page_infos(
  store: store_mod.Store,
  page_infos: List(#(String, CustomerCatalogPageInfoRecord)),
) -> store_mod.Store {
  list.fold(page_infos, store, fn(acc, pair) {
    let #(customer_id, page_info) = pair
    store_mod.set_base_customer_event_connection_page_info(
      acc,
      customer_id,
      page_info,
    )
  })
}

/// Walks the capture for the maximum `count` paired with a `precision`
/// field. Only meaningful in customer scenarios — the dispatcher gates
/// the call with `operationNames` so non-customer captures (productsCount,
/// ordersCount) never reach this walker. Iterative to avoid blowing the
/// JS stack on deep capture trees.
fn max_customer_count_baseline(capture: JsonValue) -> Option(Int) {
  do_max_customers_count_baseline([capture], None)
}

fn do_max_customers_count_baseline(
  stack: List(JsonValue),
  acc: Option(Int),
) -> Option(Int) {
  case stack {
    [] -> acc
    [JObject(entries) as obj, ..rest] -> {
      let acc = case
        read_string_field(obj, "precision"),
        json_value.field(obj, "count")
      {
        Some(_), Some(JInt(n)) ->
          case acc {
            Some(existing) -> Some(int.max(existing, n))
            None -> Some(n)
          }
        _, _ -> acc
      }
      let next =
        list.fold(list.reverse(entries), rest, fn(s, pair) { [pair.1, ..s] })
      do_max_customers_count_baseline(next, acc)
    }
    [JArray(items), ..rest] -> {
      let next =
        list.fold(list.reverse(items), rest, fn(s, item) { [item, ..s] })
      do_max_customers_count_baseline(next, acc)
    }
    [_, ..rest] -> do_max_customers_count_baseline(rest, acc)
  }
}

/// Gated on the `setup.createAccountCredit` block that store-credit
/// captures use to record the account creation that must exist before
/// the proxy executes any debit/credit mutation.
fn seed_store_credit_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.setup.createAccountCredit.response.data.storeCreditAccountCredit.storeCreditAccountTransaction.account",
    ])
  {
    False -> proxy
    True -> {
      let proxy = seed_customer_preconditions_unchecked(capture, proxy)
      let account =
        jsonpath.lookup(
          capture,
          "$.setup.createAccountCredit.response.data.storeCreditAccountCredit.storeCreditAccountTransaction.account",
        )
      let store = case account {
        Some(source) ->
          case make_seed_store_credit_account(source) {
            Ok(record) ->
              store_mod.stage_store_credit_account(proxy.store, record)
            Error(_) -> proxy.store
          }
        None -> proxy.store
      }
      draft_proxy.DraftProxy(..proxy, store: store)
    }
  }
}

/// Gated on the customerDelete mutation marker — the only scenario that
/// needs this helper pre-seeds a placeholder customer matching the
/// mutation input id so the delete can find it.
fn seed_customer_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, ["$.mutation.response.data.customerDelete"])
  {
    False -> proxy
    True -> {
      let seeded = seed_customer_preconditions_unchecked(capture, proxy)
      let delete_customer =
        jsonpath.lookup(capture, "$.mutation.variables.input.id")
        |> option.then(json_string_value)
        |> option.map(fn(id) {
          CustomerRecord(..make_placeholder_customer(0), id: id)
        })
      let store = case delete_customer {
        Some(customer) ->
          store_mod.upsert_base_customers(seeded.store, [customer])
        None -> seeded.store
      }
      draft_proxy.DraftProxy(..seeded, store: store)
    }
  }
}

/// Self-gated via the fold: only subtrees that exist contribute. The
/// per-path lookup acts as the marker check, so this function is a
/// no-op on captures that lack all four paths.
fn seed_customer_input_validation_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  [
    "$.preconditions",
    "$.updateScenarios",
    "$.deletedCustomerUpdate",
    "$.mergedCustomerUpdate",
  ]
  |> list.fold(proxy, fn(acc, path) {
    case jsonpath.lookup(capture, path) {
      Some(value) -> seed_customer_preconditions_unchecked(value, acc)
      None -> acc
    }
  })
}

/// Gated on the consent mutation markers. When present, walks
/// `$.precondition` (the existing customer pre-update) so the proxy can
/// project the consent change against the seeded base record.
fn seed_customer_consent_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.customerEmailMarketingConsentUpdate",
      "$.mutation.response.data.customerSmsMarketingConsentUpdate",
    ])
  {
    False -> proxy
    True ->
      case jsonpath.lookup(capture, "$.precondition") {
        Some(value) -> seed_customer_preconditions_unchecked(value, proxy)
        None -> seed_customer_preconditions_unchecked(capture, proxy)
      }
  }
}

/// Gated on the customerMerge mutation marker. Adds placeholder
/// customer 999_001 alongside the broad walker; the proxy's merge
/// implementation expects an extra unrelated customer in the store.
fn seed_customer_merge_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.customerMerge",
      "$.preview.response.data.customerMergePreview",
    ])
  {
    False -> proxy
    True -> {
      let seeded = seed_customer_preconditions_unchecked(capture, proxy)
      let extra = make_placeholder_customer(999_001)
      draft_proxy.DraftProxy(
        ..seeded,
        store: store_mod.upsert_base_customers(seeded.store, [extra]),
      )
    }
  }
}

/// Gated on the seedOrder block that customer-order-summary captures
/// expose. Without it the proxy's order summary effects can't be
/// recreated.
fn seed_customer_order_summary_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, ["$.seedOrder.response.data.orders.nodes"])
  {
    False -> proxy
    True -> {
      let orders = collect_seed_customer_order_summaries(capture)
      draft_proxy.DraftProxy(
        ..proxy,
        store: store_mod.upsert_base_customer_order_summaries(
          proxy.store,
          orders,
        ),
      )
    }
  }
}

fn seed_discount_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let discount_sources = case jsonpath.lookup(capture, "$.seedDiscounts") {
    Some(JArray(nodes)) -> nodes
    _ -> discount_seed_sources_from_response(capture)
  }
  let discounts =
    discount_sources
    |> list.filter_map(make_seed_discount)
    |> dedupe_discount_records
  case discounts {
    [] -> proxy
    _ ->
      draft_proxy.DraftProxy(
        ..proxy,
        store: store_mod.upsert_base_discounts(proxy.store, discounts),
      )
  }
}

fn discount_seed_sources_from_response(capture: JsonValue) -> List(JsonValue) {
  case jsonpath.lookup(capture, "$.response") {
    Some(response) -> collect_objects(response)
    None -> []
  }
}

fn dedupe_discount_records(
  records: List(DiscountRecord),
) -> List(DiscountRecord) {
  let #(reversed_ids, by_id) =
    list.fold(records, #([], dict.new()), fn(acc, record) {
      let #(ids, records_by_id) = acc
      case dict.get(records_by_id, record.id) {
        Ok(existing) -> #(
          ids,
          dict.insert(
            records_by_id,
            record.id,
            merge_seed_discount_record(existing, record),
          ),
        )
        Error(_) -> #(
          [record.id, ..ids],
          dict.insert(records_by_id, record.id, record),
        )
      }
    })
  reversed_ids
  |> list.reverse
  |> list.filter_map(fn(id) { dict.get(by_id, id) })
}

fn merge_seed_discount_record(
  existing: DiscountRecord,
  candidate: DiscountRecord,
) -> DiscountRecord {
  DiscountRecord(
    ..existing,
    title: candidate.title |> option.or(existing.title),
    status: candidate.status,
    code: candidate.code |> option.or(existing.code),
    payload: merge_captured_objects(existing.payload, candidate.payload),
    cursor: existing.cursor |> option.or(candidate.cursor),
  )
}

fn merge_captured_objects(
  left: CapturedJsonValue,
  right: CapturedJsonValue,
) -> CapturedJsonValue {
  case left, right {
    CapturedObject(left_fields), CapturedObject(right_fields) ->
      CapturedObject(list.append(
        left_fields
          |> list.filter(fn(pair) {
            !list.any(right_fields, fn(right_pair) { right_pair.0 == pair.0 })
          }),
        right_fields,
      ))
    _, _ -> right
  }
}

fn make_seed_discount(source: JsonValue) -> Result(DiscountRecord, Nil) {
  case read_object_field(source, "node"), read_string_field(source, "cursor") {
    Some(node), Some(cursor) -> {
      use record <- result.try(make_seed_discount(node))
      Ok(DiscountRecord(..record, cursor: Some(cursor)))
    }
    _, _ -> make_seed_discount_owner(source)
  }
}

fn make_seed_discount_owner(source: JsonValue) -> Result(DiscountRecord, Nil) {
  use id <- result.try(read_string_field(source, "id") |> option_to_result())
  let owner_kind = case
    string.starts_with(id, "gid://shopify/DiscountAutomaticNode/")
  {
    True -> "automatic"
    False ->
      case string.starts_with(id, "gid://shopify/DiscountCodeNode/") {
        True -> "code"
        False -> ""
      }
  }
  case owner_kind {
    "" -> Error(Nil)
    _ -> {
      let discount_field = case owner_kind {
        "automatic" -> "automaticDiscount"
        _ -> "codeDiscount"
      }
      let discount =
        read_object_field(source, discount_field)
        |> option.or(read_object_field(source, "discount"))
      use discount <- result.try(discount |> option_to_result())
      let payload =
        normalize_seed_discount_payload(source, discount_field, discount)
      Ok(DiscountRecord(
        id: id,
        owner_kind: owner_kind,
        discount_type: seed_discount_type(discount),
        title: read_string_field(discount, "title"),
        status: read_string_field(discount, "status") |> option.unwrap("ACTIVE"),
        code: seed_discount_code(discount),
        payload: payload,
        cursor: seed_discount_cursor(source),
      ))
    }
  }
}

fn normalize_seed_discount_payload(
  source: JsonValue,
  discount_field: String,
  discount: JsonValue,
) -> CapturedJsonValue {
  case source {
    JObject(fields) ->
      CapturedObject(
        fields
        |> list.filter(fn(pair) { pair.0 != "discount" })
        |> list.append([#(discount_field, discount)])
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, captured_json_from_parity(value))
        }),
      )
    _ ->
      CapturedObject([
        #("id", CapturedString("")),
        #(discount_field, captured_json_from_parity(discount)),
      ])
  }
}

fn seed_discount_type(discount: JsonValue) -> String {
  case read_string_field(discount, "__typename") {
    Some("DiscountCodeApp") | Some("DiscountAutomaticApp") -> "app"
    Some("DiscountCodeBxgy") | Some("DiscountAutomaticBxgy") -> "bxgy"
    Some("DiscountCodeFreeShipping") | Some("DiscountAutomaticFreeShipping") ->
      "free_shipping"
    _ -> "basic"
  }
}

fn seed_discount_code(discount: JsonValue) -> Option(String) {
  case read_object_field(discount, "codes") {
    Some(codes) ->
      case read_array_field(codes, "nodes") {
        Some([first, ..]) -> read_string_field(first, "code")
        _ -> None
      }
    None -> None
  }
}

fn seed_discount_cursor(source: JsonValue) -> Option(String) {
  read_string_field(source, "cursor")
}

fn seed_order_customer_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let seeded = seed_customer_preconditions_unchecked(capture, proxy)
  seed_customer_order_summary_preconditions(capture, seeded)
}

fn seed_segments_baseline_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let segments =
    collect_objects(capture)
    |> list.filter_map(make_seed_segment)
    |> dedupe_seed_segments([])
  let root_payload_paths = [
    #("segments", "$.data.segments"),
    #("segmentsCount", "$.data.segmentsCount"),
    #("segmentFilters", "$.data.segmentFilters"),
    #("segmentFilterSuggestions", "$.data.segmentFilterSuggestions"),
    #("segmentValueSuggestions", "$.data.segmentValueSuggestions"),
    #("segmentMigrations", "$.data.segmentMigrations"),
  ]
  let store =
    root_payload_paths
    |> list.fold(
      store_mod.upsert_base_segments(proxy.store, segments),
      fn(acc, pair) {
        let #(root_name, path) = pair
        case jsonpath.lookup(capture, path) {
          Some(payload) ->
            store_mod.set_base_segment_root_payload(
              acc,
              root_name,
              store_property_value(payload),
            )
          None -> acc
        }
      },
    )
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn make_seed_segment(value: JsonValue) -> Result(SegmentRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  case string.starts_with(id, "gid://shopify/Segment/") {
    False -> Error(Nil)
    True ->
      Ok(SegmentRecord(
        id: id,
        name: read_string_field(value, "name"),
        query: read_string_field(value, "query"),
        creation_date: read_string_field(value, "creationDate"),
        last_edit_date: read_string_field(value, "lastEditDate"),
      ))
  }
}

fn dedupe_seed_segments(
  segments: List(SegmentRecord),
  seen: List(String),
) -> List(SegmentRecord) {
  case segments {
    [] -> []
    [first, ..rest] ->
      case list.contains(seen, first.id) {
        True -> dedupe_seed_segments(rest, seen)
        False -> [first, ..dedupe_seed_segments(rest, [first.id, ..seen])]
      }
  }
}

fn seed_customer_metafields(
  store: store_mod.Store,
  records: List(CustomerMetafieldRecord),
) -> store_mod.Store {
  records
  |> group_metafields_by_customer
  |> list.fold(store, fn(acc, pair) {
    let #(customer_id, customer_metafields) = pair
    store_mod.stage_customer_metafields(acc, customer_id, customer_metafields)
  })
}

fn group_metafields_by_customer(
  records: List(CustomerMetafieldRecord),
) -> List(#(String, List(CustomerMetafieldRecord))) {
  records
  |> list.fold(dict.new(), fn(acc, record) {
    let existing = dict.get(acc, record.customer_id) |> result.unwrap([])
    dict.insert(acc, record.customer_id, [record, ..existing])
  })
  |> dict.to_list
  |> list.map(fn(pair) {
    let #(customer_id, items) = pair
    #(customer_id, list.reverse(items))
  })
}

fn collect_seed_customers(value: JsonValue) -> List(CustomerRecord) {
  collect_objects(value)
  |> list.filter_map(make_seed_customer)
  |> list.fold(dict.new(), fn(acc, customer) {
    case dict.get(acc, customer.id) {
      Ok(existing) ->
        dict.insert(acc, customer.id, merge_seed_customer(existing, customer))
      Error(_) -> dict.insert(acc, customer.id, customer)
    }
  })
  |> dict.values
}

fn merge_seed_customer(
  existing: CustomerRecord,
  candidate: CustomerRecord,
) -> CustomerRecord {
  CustomerRecord(
    ..existing,
    first_name: candidate.first_name |> option.or(existing.first_name),
    last_name: candidate.last_name |> option.or(existing.last_name),
    display_name: candidate.display_name |> option.or(existing.display_name),
    email: candidate.email |> option.or(existing.email),
    legacy_resource_id: candidate.legacy_resource_id
      |> option.or(existing.legacy_resource_id),
    locale: candidate.locale |> option.or(existing.locale),
    note: candidate.note |> option.or(existing.note),
    can_delete: candidate.can_delete |> option.or(existing.can_delete),
    verified_email: candidate.verified_email
      |> option.or(existing.verified_email),
    tax_exempt: candidate.tax_exempt |> option.or(existing.tax_exempt),
    tax_exemptions: case candidate.tax_exemptions {
      [] -> existing.tax_exemptions
      values -> values
    },
    state: candidate.state |> option.or(existing.state),
    tags: normalize_seed_string_list(list.append(existing.tags, candidate.tags)),
    number_of_orders: candidate.number_of_orders
      |> option.or(existing.number_of_orders),
    amount_spent: candidate.amount_spent |> option.or(existing.amount_spent),
    default_email_address: candidate.default_email_address
      |> option.or(existing.default_email_address),
    default_phone_number: candidate.default_phone_number
      |> option.or(existing.default_phone_number),
    email_marketing_consent: candidate.email_marketing_consent
      |> option.or(existing.email_marketing_consent),
    sms_marketing_consent: candidate.sms_marketing_consent
      |> option.or(existing.sms_marketing_consent),
    default_address: merge_seed_default_address(
      existing.default_address,
      candidate.default_address,
    ),
    created_at: candidate.created_at |> option.or(existing.created_at),
    updated_at: candidate.updated_at |> option.or(existing.updated_at),
  )
}

fn merge_seed_default_address(
  existing: Option(CustomerDefaultAddressRecord),
  candidate: Option(CustomerDefaultAddressRecord),
) -> Option(CustomerDefaultAddressRecord) {
  case existing, candidate {
    Some(left), Some(right) ->
      Some(CustomerDefaultAddressRecord(
        id: right.id |> option.or(left.id),
        first_name: right.first_name |> option.or(left.first_name),
        last_name: right.last_name |> option.or(left.last_name),
        address1: right.address1 |> option.or(left.address1),
        address2: right.address2 |> option.or(left.address2),
        city: right.city |> option.or(left.city),
        company: right.company |> option.or(left.company),
        province: right.province |> option.or(left.province),
        province_code: right.province_code |> option.or(left.province_code),
        country: right.country |> option.or(left.country),
        country_code_v2: right.country_code_v2
          |> option.or(left.country_code_v2),
        zip: right.zip |> option.or(left.zip),
        phone: right.phone |> option.or(left.phone),
        name: right.name |> option.or(left.name),
        formatted_area: right.formatted_area |> option.or(left.formatted_area),
      ))
    None, Some(value) -> Some(value)
    Some(value), None -> Some(value)
    None, None -> None
  }
}

fn normalize_seed_string_list(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

fn collect_seed_customer_addresses(
  value: JsonValue,
) -> List(CustomerAddressRecord) {
  collect_objects(value)
  |> list.flat_map(seed_addresses_from_customer_object)
  |> dedupe_addresses([])
}

fn collect_seed_customer_order_summaries(
  value: JsonValue,
) -> List(CustomerOrderSummaryRecord) {
  list.append(
    collect_objects(value) |> list.flat_map(seed_orders_from_customer_object),
    collect_objects(value) |> list.filter_map(make_seed_unowned_order_summary),
  )
  |> dedupe_order_summaries([])
}

fn collect_seed_customer_order_page_infos(
  value: JsonValue,
) -> List(#(String, CustomerCatalogPageInfoRecord)) {
  collect_objects(value)
  |> list.filter_map(fn(object) {
    use customer <- result.try(make_seed_customer(object))
    use orders <- result.try(
      read_object_field(object, "orders") |> option_to_result(),
    )
    Ok(#(
      customer.id,
      make_seed_customer_page_info(read_object_field(orders, "pageInfo")),
    ))
  })
}

fn collect_seed_customer_event_summaries(
  value: JsonValue,
) -> List(CustomerEventSummaryRecord) {
  collect_objects(value)
  |> list.flat_map(seed_events_from_customer_object)
  |> dedupe_event_summaries([])
}

fn collect_seed_customer_event_page_infos(
  value: JsonValue,
) -> List(#(String, CustomerCatalogPageInfoRecord)) {
  collect_objects(value)
  |> list.filter_map(fn(object) {
    use customer <- result.try(make_seed_customer(object))
    use events <- result.try(
      read_object_field(object, "events") |> option_to_result(),
    )
    Ok(#(
      customer.id,
      make_seed_customer_page_info(read_object_field(events, "pageInfo")),
    ))
  })
}

fn collect_seed_customer_last_orders(
  value: JsonValue,
) -> List(#(String, CustomerOrderSummaryRecord)) {
  collect_objects(value)
  |> list.filter_map(fn(object) {
    use customer <- result.try(make_seed_customer(object))
    use last_order <- result.try(
      read_object_field(object, "lastOrder") |> option_to_result(),
    )
    use order <- result.try(make_seed_order_summary(
      last_order,
      Some(customer.id),
      None,
    ))
    Ok(#(customer.id, order))
  })
}

fn collect_seed_customer_metafields(
  value: JsonValue,
) -> List(CustomerMetafieldRecord) {
  collect_objects(value)
  |> list.flat_map(seed_metafields_from_customer_object)
  |> list.reverse
  |> dedupe_metafields([])
  |> list.reverse
}

fn collect_seed_customer_account_pages(
  value: JsonValue,
) -> List(CustomerAccountPageRecord) {
  list.append(
    collect_account_pages_from_connections(value),
    collect_objects(value)
      |> list.filter_map(make_seed_customer_account_page),
  )
  |> dedupe_account_pages([])
}

fn collect_seed_customer_connections(
  value: JsonValue,
) -> List(#(String, CustomerCatalogConnectionRecord)) {
  collect_objects(value)
  |> list.flat_map(customer_connections_from_object)
}

fn customer_connections_from_object(
  value: JsonValue,
) -> List(#(String, CustomerCatalogConnectionRecord)) {
  case value {
    JObject(entries) ->
      entries
      |> list.filter_map(fn(pair) {
        let #(key, candidate) = pair
        use connection <- result.try(make_seed_customer_connection(candidate))
        Ok(#(key, connection))
      })
    _ -> []
  }
}

fn make_seed_customer_connection(
  value: JsonValue,
) -> Result(CustomerCatalogConnectionRecord, Nil) {
  let edges = read_array_field(value, "edges") |> option.unwrap([])
  let edge_records =
    edges
    |> list.filter_map(fn(edge) {
      use node <- result.try(
        read_object_field(edge, "node") |> option_to_result(),
      )
      use customer <- result.try(make_seed_customer(node))
      let cursor = read_string_field(edge, "cursor")
      Ok(#(customer.id, cursor))
    })
  let node_records = case edge_records {
    [] ->
      read_array_field(value, "nodes")
      |> option.unwrap([])
      |> list.index_map(fn(node, index) {
        use customer <- result.try(make_seed_customer(node))
        Ok(#(
          customer.id,
          read_object_field(value, "pageInfo")
            |> option.then(fn(info) { page_info_cursor_for_index(info, index) }),
        ))
      })
      |> list.filter_map(fn(item) { item })
    _ -> []
  }
  let records = list.append(edge_records, node_records)
  case records {
    [] -> Error(Nil)
    _ -> {
      let cursor_by_customer_id =
        records
        |> list.fold(dict.new(), fn(acc, pair) {
          let #(customer_id, cursor) = pair
          case cursor {
            Some(value) -> dict.insert(acc, customer_id, value)
            None -> acc
          }
        })
      Ok(CustomerCatalogConnectionRecord(
        ordered_customer_ids: list.map(records, fn(pair) { pair.0 }),
        cursor_by_customer_id: cursor_by_customer_id,
        page_info: make_seed_customer_page_info(read_object_field(
          value,
          "pageInfo",
        )),
      ))
    }
  }
}

fn make_seed_customer_page_info(
  value: Option(JsonValue),
) -> CustomerCatalogPageInfoRecord {
  case value {
    Some(info) ->
      CustomerCatalogPageInfoRecord(
        has_next_page: read_bool_field(info, "hasNextPage")
          |> option.unwrap(False),
        has_previous_page: read_bool_field(info, "hasPreviousPage")
          |> option.unwrap(False),
        start_cursor: read_string_field(info, "startCursor"),
        end_cursor: read_string_field(info, "endCursor"),
      )
    None ->
      CustomerCatalogPageInfoRecord(
        has_next_page: False,
        has_previous_page: False,
        start_cursor: None,
        end_cursor: None,
      )
  }
}

fn collect_account_pages_from_connections(
  value: JsonValue,
) -> List(CustomerAccountPageRecord) {
  collect_objects(value)
  |> list.flat_map(account_pages_from_connection)
}

fn account_pages_from_connection(
  value: JsonValue,
) -> List(CustomerAccountPageRecord) {
  let edge_pages =
    read_array_field(value, "edges")
    |> option.unwrap([])
    |> list.filter_map(fn(edge) {
      let cursor = read_string_field(edge, "cursor")
      use node <- result.try(
        read_object_field(edge, "node") |> option_to_result(),
      )
      use page <- result.try(make_seed_customer_account_page(node))
      Ok(CustomerAccountPageRecord(..page, cursor: cursor))
    })
  case edge_pages {
    [_, ..] -> edge_pages
    [] -> {
      let nodes = read_array_field(value, "nodes") |> option.unwrap([])
      let page_info = read_object_field(value, "pageInfo")
      nodes
      |> list.index_map(fn(node, index) {
        let cursor =
          account_page_node_cursor(page_info, index, list.length(nodes))
        case make_seed_customer_account_page(node) {
          Ok(page) -> Ok(CustomerAccountPageRecord(..page, cursor: cursor))
          Error(_) -> Error(Nil)
        }
      })
      |> list.filter_map(fn(item) { item })
    }
  }
}

fn account_page_node_cursor(
  page_info: Option(JsonValue),
  index: Int,
  length: Int,
) -> Option(String) {
  case page_info {
    Some(info) ->
      case index == 0, index == length - 1 {
        True, _ -> read_string_field(info, "startCursor")
        _, True -> read_string_field(info, "endCursor")
        _, _ -> None
      }
    None -> None
  }
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(v) -> Ok(v)
    None -> Error(Nil)
  }
}

/// Gated on the customerCreate mutation marker. The customer-create
/// scenario uses `$.downstreamRead.data.customersCount.count` to
/// reproduce the post-create count assertion; pre-seeding `count - 1`
/// placeholder customers leaves room for the freshly-created customer.
/// Other scenarios that happen to expose a customersCount path are
/// handled by `seed_customer_preconditions` and friends.
fn seed_customer_count_baseline(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.customerCreate.customer.id",
    ])
  {
    False -> proxy
    True -> {
      let target_count =
        jsonpath.lookup(capture, "$.downstreamRead.data.customersCount.count")
        |> option.then(json_int_value)
        |> option.unwrap(0)
      let records = make_placeholder_customers(int.max(0, target_count - 1), 1)
      draft_proxy.DraftProxy(
        ..proxy,
        store: store_mod.upsert_base_customers(proxy.store, records),
      )
    }
  }
}

fn make_placeholder_customers(count: Int, next: Int) -> List(CustomerRecord) {
  do_make_placeholder_customers(count, next, [])
  |> list.reverse
}

fn do_make_placeholder_customers(
  count: Int,
  next: Int,
  acc: List(CustomerRecord),
) -> List(CustomerRecord) {
  case count <= 0 {
    True -> acc
    False ->
      do_make_placeholder_customers(count - 1, next + 1, [
        make_placeholder_customer(next),
        ..acc
      ])
  }
}

fn make_placeholder_customer(index: Int) -> CustomerRecord {
  let id =
    "gid://shopify/Customer/customer-parity-baseline-" <> int.to_string(index)
  CustomerRecord(
    id: id,
    first_name: None,
    last_name: None,
    display_name: Some("Customer parity baseline " <> int.to_string(index)),
    email: Some(
      "customer-parity-baseline-" <> int.to_string(index) <> "@example.test",
    ),
    legacy_resource_id: Some(
      "customer-parity-baseline-" <> int.to_string(index),
    ),
    locale: None,
    note: None,
    can_delete: Some(False),
    verified_email: Some(False),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("DISABLED"),
    tags: ["customer-parity-baseline"],
    number_of_orders: Some("0"),
    amount_spent: Some(Money(amount: "0.0", currency_code: "CAD")),
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    created_at: None,
    updated_at: None,
  )
}

fn json_int_value(value: JsonValue) -> Option(Int) {
  case value {
    JInt(i) -> Some(i)
    _ -> None
  }
}

fn json_string_value(value: JsonValue) -> Option(String) {
  case value {
    JString(s) -> Some(s)
    _ -> None
  }
}

/// Pre-order iterative walker. The previous self-recursive implementation
/// blew the JS call stack on deep capture trees once
/// `seed_customer_preconditions` started running for every non-fresh-
/// customer scenario.
fn collect_objects(value: JsonValue) -> List(JsonValue) {
  do_collect_objects([value], []) |> list.reverse
}

fn do_collect_objects(
  stack: List(JsonValue),
  acc: List(JsonValue),
) -> List(JsonValue) {
  case stack {
    [] -> acc
    [JObject(entries) as obj, ..rest] -> {
      let next =
        list.fold(list.reverse(entries), rest, fn(s, pair) { [pair.1, ..s] })
      do_collect_objects(next, [obj, ..acc])
    }
    [JArray(items), ..rest] -> {
      let next =
        list.fold(list.reverse(items), rest, fn(s, item) { [item, ..s] })
      do_collect_objects(next, acc)
    }
    [_, ..rest] -> do_collect_objects(rest, acc)
  }
}

fn replace_customer_one_variables(
  capture: JsonValue,
  variables: JsonValue,
) -> JsonValue {
  case first_customer_gid(capture) {
    Some(customer_id) -> replace_customer_one_value(variables, customer_id)
    None -> variables
  }
}

fn first_customer_gid(value: JsonValue) -> Option(String) {
  let found =
    collect_objects(value)
    |> list.find_map(fn(object) {
      case read_string_field(object, "id") {
        Some(id) ->
          case string.contains(id, "gid://shopify/Customer/") {
            True -> Ok(id)
            False -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case found {
    Ok(id) -> Some(id)
    Error(_) -> None
  }
}

fn replace_customer_one_value(
  value: JsonValue,
  customer_id: String,
) -> JsonValue {
  case value {
    JString("gid://shopify/Customer/1") -> JString(customer_id)
    JObject(entries) ->
      JObject(
        list.map(entries, fn(pair) {
          #(pair.0, replace_customer_one_value(pair.1, customer_id))
        }),
      )
    JArray(items) ->
      JArray(
        list.map(items, fn(item) {
          replace_customer_one_value(item, customer_id)
        }),
      )
    other -> other
  }
}

fn make_seed_customer(source: JsonValue) -> Result(CustomerRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "Customer"))
  use _ <- result.try(require_customer_seed_payload(source))
  let email =
    read_string_field(source, "email")
    |> option.or(read_string_field_from_option(
      read_object_field(source, "defaultEmailAddress"),
      "emailAddress",
    ))
  let first_name = read_string_field(source, "firstName")
  let last_name = read_string_field(source, "lastName")
  let display_name = read_string_field(source, "displayName")
  let default_email =
    make_seed_default_email(
      read_object_field(source, "defaultEmailAddress"),
      email,
    )
  let default_phone =
    make_seed_default_phone(read_object_field(source, "defaultPhoneNumber"))
  Ok(CustomerRecord(
    id: id,
    first_name: first_name,
    last_name: last_name,
    display_name: display_name,
    email: email,
    legacy_resource_id: read_string_field(source, "legacyResourceId")
      |> option.or(Some(generic_gid_tail(id))),
    locale: read_string_field(source, "locale"),
    note: read_string_field(source, "note"),
    can_delete: read_bool_field(source, "canDelete"),
    verified_email: read_bool_field(source, "verifiedEmail"),
    data_sale_opt_out: read_bool_field(source, "dataSaleOptOut")
      |> option.unwrap(False),
    tax_exempt: read_bool_field(source, "taxExempt"),
    tax_exemptions: read_string_array_field(source, "taxExemptions"),
    state: read_string_field(source, "state"),
    tags: read_string_array_field(source, "tags"),
    number_of_orders: read_scalar_string_field(source, "numberOfOrders"),
    amount_spent: make_seed_money(read_object_field(source, "amountSpent")),
    default_email_address: default_email,
    default_phone_number: default_phone,
    email_marketing_consent: make_seed_email_consent(read_object_field(
      source,
      "emailMarketingConsent",
    )),
    sms_marketing_consent: make_seed_sms_consent(read_object_field(
      source,
      "smsMarketingConsent",
    )),
    default_address: make_seed_default_address(read_object_field(
      source,
      "defaultAddress",
    )),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
  ))
}

fn make_seed_customer_payment_method(
  source: JsonValue,
) -> Result(CustomerPaymentMethodRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use customer_id <- result.try(required_string_field(source, "customerId"))
  Ok(
    CustomerPaymentMethodRecord(
      id: id,
      customer_id: customer_id,
      cursor: read_string_field(source, "cursor"),
      instrument: make_seed_customer_payment_method_instrument(
        read_object_field(source, "instrument"),
      ),
      revoked_at: read_string_field(source, "revokedAt"),
      revoked_reason: read_string_field(source, "revokedReason"),
      subscription_contracts: [],
    ),
  )
}

fn make_seed_customer_payment_method_instrument(
  source: Option(JsonValue),
) -> Option(CustomerPaymentMethodInstrumentRecord) {
  case source {
    Some(value) -> {
      let type_name =
        read_string_field(value, "typeName")
        |> option.or(read_string_field(value, "__typename"))
      case type_name {
        Some(name) ->
          Some(CustomerPaymentMethodInstrumentRecord(
            type_name: name,
            data: json_object_to_string_dict(read_object_field(value, "data")),
          ))
        None -> None
      }
    }
    None -> None
  }
}

fn json_object_to_string_dict(
  value: Option(JsonValue),
) -> Dict(String, String) {
  case value {
    Some(JObject(entries)) ->
      list.fold(entries, dict.new(), fn(acc, pair) {
        let #(key, item) = pair
        case item {
          JString(text) -> dict.insert(acc, key, text)
          JBool(True) -> dict.insert(acc, key, "true")
          JBool(False) -> dict.insert(acc, key, "false")
          JInt(number) -> dict.insert(acc, key, int.to_string(number))
          JFloat(number) -> dict.insert(acc, key, float.to_string(number))
          JNull -> dict.insert(acc, key, "__null")
          _ -> acc
        }
      })
    _ -> dict.new()
  }
}

fn require_customer_seed_payload(source: JsonValue) -> Result(Nil, Nil) {
  let has_payload =
    has_field(source, "email")
    || has_field(source, "displayName")
    || has_field(source, "legacyResourceId")
    || has_field(source, "defaultEmailAddress")
    || has_field(source, "defaultPhoneNumber")
    || has_field(source, "defaultAddress")
    || has_field(source, "addresses")
    || has_field(source, "addressesV2")
    || has_field(source, "metafield")
    || has_field(source, "metafields")
    || has_field(source, "orders")
    || has_field(source, "events")
    || has_field(source, "lastOrder")
    || has_field(source, "tags")
    || has_field(source, "state")
    || has_field(source, "createdAt")
    || has_field(source, "updatedAt")
  case has_payload {
    True -> Ok(Nil)
    False -> Error(Nil)
  }
}

fn has_field(source: JsonValue, name: String) -> Bool {
  case json_value.field(source, name) {
    Some(_) -> True
    None -> False
  }
}

fn required_gid(
  source: JsonValue,
  field: String,
  type_name: String,
) -> Result(String, Nil) {
  case read_string_field(source, field) {
    Some(id) ->
      case string.contains(id, "gid://shopify/" <> type_name <> "/") {
        True -> Ok(id)
        False -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

fn make_seed_default_email(
  source: Option(JsonValue),
  fallback_email: Option(String),
) -> Option(CustomerDefaultEmailAddressRecord) {
  case source, fallback_email {
    Some(value), _ ->
      Some(CustomerDefaultEmailAddressRecord(
        email_address: read_string_field(value, "emailAddress")
          |> option.or(fallback_email),
        marketing_state: read_string_field(value, "marketingState"),
        marketing_opt_in_level: read_string_field(value, "marketingOptInLevel"),
        marketing_updated_at: read_string_field(value, "marketingUpdatedAt"),
      ))
    None, Some(email) ->
      Some(CustomerDefaultEmailAddressRecord(
        email_address: Some(email),
        marketing_state: None,
        marketing_opt_in_level: None,
        marketing_updated_at: None,
      ))
    None, None -> None
  }
}

fn make_seed_default_phone(
  source: Option(JsonValue),
) -> Option(CustomerDefaultPhoneNumberRecord) {
  case source {
    Some(value) ->
      Some(CustomerDefaultPhoneNumberRecord(
        phone_number: read_string_field(value, "phoneNumber"),
        marketing_state: read_string_field(value, "marketingState"),
        marketing_opt_in_level: read_string_field(value, "marketingOptInLevel"),
        marketing_updated_at: read_string_field(value, "marketingUpdatedAt"),
        marketing_collected_from: read_string_field(
          value,
          "marketingCollectedFrom",
        ),
      ))
    None -> None
  }
}

fn make_seed_email_consent(
  source: Option(JsonValue),
) -> Option(CustomerEmailMarketingConsentRecord) {
  case source {
    Some(value) ->
      Some(CustomerEmailMarketingConsentRecord(
        marketing_state: read_string_field(value, "marketingState"),
        marketing_opt_in_level: read_string_field(value, "marketingOptInLevel"),
        consent_updated_at: read_string_field(value, "consentUpdatedAt")
          |> option.or(read_string_field(value, "marketingUpdatedAt")),
      ))
    None -> None
  }
}

fn make_seed_sms_consent(
  source: Option(JsonValue),
) -> Option(CustomerSmsMarketingConsentRecord) {
  case source {
    Some(value) ->
      Some(CustomerSmsMarketingConsentRecord(
        marketing_state: read_string_field(value, "marketingState"),
        marketing_opt_in_level: read_string_field(value, "marketingOptInLevel"),
        consent_updated_at: read_string_field(value, "consentUpdatedAt")
          |> option.or(read_string_field(value, "marketingUpdatedAt")),
        consent_collected_from: read_string_field(value, "consentCollectedFrom")
          |> option.or(read_string_field(value, "marketingCollectedFrom")),
      ))
    None -> None
  }
}

fn make_seed_default_address(
  source: Option(JsonValue),
) -> Option(CustomerDefaultAddressRecord) {
  case source {
    Some(value) ->
      Some(CustomerDefaultAddressRecord(
        id: read_string_field(value, "id"),
        first_name: read_string_field(value, "firstName"),
        last_name: read_string_field(value, "lastName"),
        address1: read_string_field(value, "address1"),
        address2: read_string_field(value, "address2"),
        city: read_string_field(value, "city"),
        company: read_string_field(value, "company"),
        province: read_string_field(value, "province"),
        province_code: read_string_field(value, "provinceCode"),
        country: read_string_field(value, "country"),
        country_code_v2: read_string_field(value, "countryCodeV2"),
        zip: read_string_field(value, "zip"),
        phone: read_string_field(value, "phone"),
        name: read_string_field(value, "name"),
        formatted_area: read_string_field(value, "formattedArea"),
      ))
    None -> None
  }
}

fn seed_addresses_from_customer_object(
  source: JsonValue,
) -> List(CustomerAddressRecord) {
  case make_seed_customer(source) {
    Ok(customer) -> {
      let addresses_v2_edges =
        read_object_field(source, "addressesV2")
        |> option.then(fn(connection) { read_array_field(connection, "edges") })
        |> option.unwrap([])
      let default_address = case read_object_field(source, "defaultAddress") {
        Some(address) -> [
          make_seed_customer_address(address, customer.id, 0, "default", None),
        ]
        None -> []
      }
      let addresses =
        read_array_field(source, "addresses")
        |> option.unwrap([])
        |> list.index_map(fn(address, index) {
          make_seed_customer_address(
            address,
            customer.id,
            index + 1,
            "address",
            cursor_at(addresses_v2_edges, index),
          )
        })
      let addresses_v2 = case addresses {
        [] ->
          addresses_v2_edges
          |> list.index_map(fn(edge, index) {
            case read_object_field(edge, "node") {
              Some(address) ->
                make_seed_customer_address(
                  address,
                  customer.id,
                  index + 100,
                  "node",
                  read_string_field(edge, "cursor"),
                )
              None -> Error(Nil)
            }
          })
        _ -> []
      }
      list.append(default_address, list.append(addresses, addresses_v2))
      |> list.filter_map(fn(item) { item })
    }
    Error(_) -> []
  }
}

fn make_seed_customer_address(
  source: JsonValue,
  customer_id: String,
  position: Int,
  fallback_key: String,
  cursor: Option(String),
) -> Result(CustomerAddressRecord, Nil) {
  let fallback_id =
    customer_id
    <> "/MailingAddress/"
    <> fallback_key
    <> "-"
    <> int.to_string(position)
  let id =
    read_string_field(source, "id")
    |> option.unwrap(fallback_id)
  Ok(CustomerAddressRecord(
    id: id,
    customer_id: customer_id,
    cursor: cursor,
    position: position,
    first_name: read_string_field(source, "firstName"),
    last_name: read_string_field(source, "lastName"),
    address1: read_string_field(source, "address1"),
    address2: read_string_field(source, "address2"),
    city: read_string_field(source, "city"),
    company: read_string_field(source, "company"),
    province: read_string_field(source, "province"),
    province_code: read_string_field(source, "provinceCode"),
    country: read_string_field(source, "country"),
    country_code_v2: read_string_field(source, "countryCodeV2"),
    zip: read_string_field(source, "zip"),
    phone: read_string_field(source, "phone"),
    name: read_string_field(source, "name"),
    formatted_area: read_string_field(source, "formattedArea"),
  ))
}

fn cursor_at(edges: List(JsonValue), index: Int) -> Option(String) {
  case list.drop(edges, index) {
    [edge, ..] -> read_string_field(edge, "cursor")
    [] -> None
  }
}

fn seed_orders_from_customer_object(
  source: JsonValue,
) -> List(CustomerOrderSummaryRecord) {
  case make_seed_customer(source) {
    Ok(customer) -> {
      let connection = read_object_field(source, "orders")
      let edge_orders =
        connection
        |> option.then(fn(c) { read_array_field(c, "edges") })
        |> option.unwrap([])
        |> list.filter_map(fn(edge) {
          let cursor = read_string_field(edge, "cursor")
          use node <- result.try(
            read_object_field(edge, "node") |> option_to_result(),
          )
          make_seed_order_summary(node, Some(customer.id), cursor)
        })
      let node_orders = case edge_orders {
        [] ->
          connection
          |> option.then(fn(c) { read_array_field(c, "nodes") })
          |> option.unwrap([])
          |> list.index_map(fn(node, index) {
            make_seed_order_summary(
              node,
              Some(customer.id),
              connection
                |> option.then(fn(c) { read_object_field(c, "pageInfo") })
                |> option.then(fn(info) {
                  page_info_cursor_for_index(info, index)
                }),
            )
          })
          |> list.filter_map(fn(item) { item })
        _ -> []
      }
      list.append(edge_orders, node_orders)
    }
    Error(_) -> []
  }
}

fn page_info_cursor_for_index(
  page_info: JsonValue,
  index: Int,
) -> Option(String) {
  case index {
    0 -> read_string_field(page_info, "startCursor")
    _ -> read_string_field(page_info, "endCursor")
  }
}

fn make_seed_unowned_order_summary(
  source: JsonValue,
) -> Result(CustomerOrderSummaryRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "Order"))
  let customer_id =
    read_object_field(source, "customer")
    |> option.then(fn(customer) { read_string_field(customer, "id") })
  Ok(CustomerOrderSummaryRecord(
    id: id,
    customer_id: customer_id,
    cursor: None,
    name: read_string_field(source, "name"),
    email: read_string_field(source, "email"),
    created_at: read_string_field(source, "createdAt"),
    current_total_price: read_object_field(source, "currentTotalPriceSet")
      |> option.then(fn(set) {
        make_seed_money(read_object_field(set, "shopMoney"))
      }),
  ))
}

fn make_seed_order_summary(
  source: JsonValue,
  customer_id: Option(String),
  cursor: Option(String),
) -> Result(CustomerOrderSummaryRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "Order"))
  Ok(CustomerOrderSummaryRecord(
    id: id,
    customer_id: customer_id,
    cursor: cursor,
    name: read_string_field(source, "name"),
    email: read_string_field(source, "email"),
    created_at: read_string_field(source, "createdAt"),
    current_total_price: read_object_field(source, "currentTotalPriceSet")
      |> option.then(fn(set) {
        make_seed_money(read_object_field(set, "shopMoney"))
      }),
  ))
}

fn seed_events_from_customer_object(
  source: JsonValue,
) -> List(CustomerEventSummaryRecord) {
  case make_seed_customer(source) {
    Ok(customer) -> {
      read_object_field(source, "events")
      |> option.then(fn(connection) { read_array_field(connection, "edges") })
      |> option.unwrap([])
      |> list.filter_map(fn(edge) {
        let cursor = read_string_field(edge, "cursor")
        use node <- result.try(
          read_object_field(edge, "node") |> option_to_result(),
        )
        use id <- result.try(required_gid(node, "id", "BasicEvent"))
        Ok(CustomerEventSummaryRecord(
          id: id,
          customer_id: customer.id,
          cursor: cursor,
        ))
      })
    }
    Error(_) -> []
  }
}

fn seed_metafields_from_customer_object(
  source: JsonValue,
) -> List(CustomerMetafieldRecord) {
  case make_seed_customer(source) {
    Ok(customer) -> {
      let direct = case read_object_field(source, "metafield") {
        Some(value) -> [value]
        None -> []
      }
      let nodes =
        read_object_field(source, "metafields")
        |> option.then(fn(connection) { read_array_field(connection, "nodes") })
        |> option.unwrap([])
      list.append(direct, nodes)
      |> list.index_map(fn(value, index) {
        make_seed_customer_metafield(value, customer.id, index)
      })
      |> list.filter_map(fn(item) { item })
    }
    Error(_) -> []
  }
}

fn make_seed_customer_metafield(
  source: JsonValue,
  customer_id: String,
  index: Int,
) -> Result(CustomerMetafieldRecord, Nil) {
  use namespace <- result.try(required_string_field(source, "namespace"))
  use key <- result.try(required_string_field(source, "key"))
  use value <- result.try(required_string_field(source, "value"))
  let id =
    read_string_field(source, "id")
    |> option.unwrap(
      "gid://shopify/Metafield/"
      <> generic_gid_tail(customer_id)
      <> "-"
      <> int.to_string(index + 1),
    )
  Ok(CustomerMetafieldRecord(
    id: id,
    customer_id: customer_id,
    namespace: namespace,
    key: key,
    type_: read_string_field(source, "type")
      |> option.unwrap("single_line_text_field"),
    value: value,
    compare_digest: read_string_field(source, "compareDigest"),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
  ))
}

fn make_seed_customer_account_page(
  source: JsonValue,
) -> Result(CustomerAccountPageRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "CustomerAccountPage"))
  use title <- result.try(required_string_field(source, "title"))
  use handle <- result.try(required_string_field(source, "handle"))
  use default_cursor <- result.try(required_string_field(
    source,
    "defaultCursor",
  ))
  Ok(CustomerAccountPageRecord(
    id: id,
    title: title,
    handle: handle,
    default_cursor: default_cursor,
    cursor: None,
  ))
}

fn make_seed_store_credit_account(
  source: JsonValue,
) -> Result(StoreCreditAccountRecord, Nil) {
  use id <- result.try(required_gid(source, "id", "StoreCreditAccount"))
  case make_seed_money(read_object_field(source, "balance")) {
    Some(balance) -> {
      use customer_id <- result.try(required_string_field(
        read_object_field(source, "owner") |> option.unwrap(JObject([])),
        "id",
      ))
      Ok(StoreCreditAccountRecord(
        id: id,
        customer_id: customer_id,
        cursor: None,
        balance: balance,
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_money(source: Option(JsonValue)) -> Option(Money) {
  case source {
    Some(value) -> {
      let amount = read_scalar_string_field(value, "amount")
      let currency = read_string_field(value, "currencyCode")
      case amount, currency {
        Some(a), Some(c) -> Some(Money(amount: a, currency_code: c))
        _, _ -> None
      }
    }
    None -> None
  }
}

fn read_scalar_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    Some(JInt(i)) -> Some(int.to_string(i))
    Some(JFloat(f)) -> Some(float.to_string(f))
    _ -> None
  }
}

fn dedupe_addresses(
  items: List(CustomerAddressRecord),
  _seen: List(String),
) -> List(CustomerAddressRecord) {
  items
  |> list.fold([], fn(acc, item) { upsert_seed_address(acc, item) })
}

fn upsert_seed_address(
  items: List(CustomerAddressRecord),
  item: CustomerAddressRecord,
) -> List(CustomerAddressRecord) {
  case items {
    [] -> [item]
    [existing, ..rest] ->
      case existing.id == item.id {
        True -> [merge_seed_address(existing, item), ..rest]
        False -> [existing, ..upsert_seed_address(rest, item)]
      }
  }
}

fn merge_seed_address(
  existing: CustomerAddressRecord,
  candidate: CustomerAddressRecord,
) -> CustomerAddressRecord {
  CustomerAddressRecord(
    ..existing,
    cursor: candidate.cursor |> option.or(existing.cursor),
    first_name: candidate.first_name |> option.or(existing.first_name),
    last_name: candidate.last_name |> option.or(existing.last_name),
    address1: candidate.address1 |> option.or(existing.address1),
    address2: candidate.address2 |> option.or(existing.address2),
    city: candidate.city |> option.or(existing.city),
    company: candidate.company |> option.or(existing.company),
    province: candidate.province |> option.or(existing.province),
    province_code: candidate.province_code |> option.or(existing.province_code),
    country: candidate.country |> option.or(existing.country),
    country_code_v2: candidate.country_code_v2
      |> option.or(existing.country_code_v2),
    zip: candidate.zip |> option.or(existing.zip),
    phone: candidate.phone |> option.or(existing.phone),
    name: candidate.name |> option.or(existing.name),
    formatted_area: candidate.formatted_area
      |> option.or(existing.formatted_area),
  )
}

fn dedupe_order_summaries(
  items: List(CustomerOrderSummaryRecord),
  _seen: List(String),
) -> List(CustomerOrderSummaryRecord) {
  items
  |> list.fold([], fn(acc, item) { upsert_seed_order_summary(acc, item) })
  |> list.reverse
}

fn upsert_seed_order_summary(
  items: List(CustomerOrderSummaryRecord),
  item: CustomerOrderSummaryRecord,
) -> List(CustomerOrderSummaryRecord) {
  case items {
    [] -> [item]
    [existing, ..rest] ->
      case existing.id == item.id {
        True -> [merge_seed_order_summary(existing, item), ..rest]
        False -> [existing, ..upsert_seed_order_summary(rest, item)]
      }
  }
}

fn merge_seed_order_summary(
  existing: CustomerOrderSummaryRecord,
  candidate: CustomerOrderSummaryRecord,
) -> CustomerOrderSummaryRecord {
  CustomerOrderSummaryRecord(
    ..existing,
    customer_id: candidate.customer_id |> option.or(existing.customer_id),
    cursor: candidate.cursor |> option.or(existing.cursor),
    name: candidate.name |> option.or(existing.name),
    email: candidate.email |> option.or(existing.email),
    created_at: candidate.created_at |> option.or(existing.created_at),
    current_total_price: candidate.current_total_price
      |> option.or(existing.current_total_price),
  )
}

fn dedupe_event_summaries(
  items: List(CustomerEventSummaryRecord),
  seen: List(String),
) -> List(CustomerEventSummaryRecord) {
  case items {
    [] -> []
    [item, ..rest] ->
      case list.contains(seen, item.id) {
        True -> dedupe_event_summaries(rest, seen)
        False -> [item, ..dedupe_event_summaries(rest, [item.id, ..seen])]
      }
  }
}

fn dedupe_metafields(
  items: List(CustomerMetafieldRecord),
  _seen: List(String),
) -> List(CustomerMetafieldRecord) {
  items
  |> list.fold([], fn(acc, item) { upsert_seed_metafield(acc, item) })
  |> list.reverse
}

fn upsert_seed_metafield(
  items: List(CustomerMetafieldRecord),
  item: CustomerMetafieldRecord,
) -> List(CustomerMetafieldRecord) {
  let key = customer_metafield_seed_key(item)
  case items {
    [] -> [item]
    [existing, ..rest] ->
      case customer_metafield_seed_key(existing) == key {
        True ->
          case metafield_seed_score(item) >= metafield_seed_score(existing) {
            True -> [item, ..rest]
            False -> items
          }
        False -> [existing, ..upsert_seed_metafield(rest, item)]
      }
  }
}

fn customer_metafield_seed_key(metafield: CustomerMetafieldRecord) -> String {
  metafield.customer_id <> "::" <> metafield.namespace <> "::" <> metafield.key
}

fn metafield_seed_score(metafield: CustomerMetafieldRecord) -> Int {
  case
    string.contains(
      metafield.id,
      generic_gid_tail(metafield.customer_id) <> "-",
    )
  {
    True -> 0
    False -> 1
  }
}

fn dedupe_account_pages(
  items: List(CustomerAccountPageRecord),
  seen: List(String),
) -> List(CustomerAccountPageRecord) {
  case items {
    [] -> []
    [item, ..rest] ->
      case list.contains(seen, item.id) {
        True -> dedupe_account_pages(rest, seen)
        False -> [item, ..dedupe_account_pages(rest, [item.id, ..seen])]
      }
  }
}

fn seed_captured_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let single_product_nodes = case read_object_field(capture, "seedProduct") {
    Some(node) -> [node]
    None -> []
  }
  let product_nodes = case read_array_field(capture, "seedProducts") {
    Some(nodes) -> nodes
    None -> []
  }
  let seed_id_nodes = case jsonpath.lookup(capture, "$.seed.productId") {
    Some(JString(id)) -> [JObject([#("id", JString(id))])]
    _ -> []
  }
  let product_nodes =
    list.append(single_product_nodes, list.append(product_nodes, seed_id_nodes))
  let products = case product_nodes {
    [] -> []
    nodes -> list.filter_map(nodes, make_seed_product_relaxed)
  }
  let variants = case product_nodes {
    [] -> []
    nodes ->
      list.flat_map(nodes, fn(product_json) {
        seed_variants_for_product(product_json)
      })
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
  let store = list.fold(product_nodes, store, seed_options_for_product)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_selling_plan_group_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let groups = case read_array_field(capture, "seedSellingPlanGroups") {
    Some(nodes) -> list.filter_map(nodes, make_seed_selling_plan_group)
    None -> []
  }
  case groups {
    [] -> proxy
    _ ->
      draft_proxy.DraftProxy(
        ..proxy,
        store: store_mod.upsert_base_selling_plan_groups(proxy.store, groups),
      )
  }
}

fn make_seed_selling_plan_group(
  source: JsonValue,
) -> Result(SellingPlanGroupRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use merchant_code <- result.try(required_string_field(source, "merchantCode"))
  Ok(SellingPlanGroupRecord(
    id: id,
    app_id: read_string_field(source, "appId"),
    name: name,
    merchant_code: merchant_code,
    description: read_string_field(source, "description"),
    options: read_string_array_field(source, "options"),
    position: read_int_field(source, "position"),
    summary: read_string_field(source, "summary"),
    created_at: read_string_field(source, "createdAt"),
    product_ids: read_connection_node_ids(source, "products"),
    product_variant_ids: read_connection_node_ids(source, "productVariants"),
    selling_plans: read_connection_nodes(source, "sellingPlans")
      |> list.filter_map(make_seed_selling_plan),
    cursor: None,
  ))
}

fn make_seed_selling_plan(source: JsonValue) -> Result(SellingPlanRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(SellingPlanRecord(id: id, data: captured_json_from_parity(source)))
}

fn read_connection_node_ids(source: JsonValue, field: String) -> List(String) {
  read_connection_nodes(source, field)
  |> list.filter_map(fn(node) { required_string_field(node, "id") })
}

fn read_connection_nodes(source: JsonValue, field: String) -> List(JsonValue) {
  case read_object_field(source, field) {
    Some(connection) ->
      read_array_field(connection, "nodes") |> option.unwrap([])
    None -> []
  }
}

fn seed_product_media_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let media = case read_array_field(capture, "seedProductMedia") {
    Some(nodes) ->
      nodes
      |> enumerate_json_values()
      |> list.filter_map(fn(entry) {
        let #(node, index) = entry
        make_seed_product_media(node, index)
      })
    None -> []
  }
  let store =
    media
    |> group_product_media_by_product_id
    |> list.fold(proxy.store, fn(current_store, entry) {
      let #(product_id, records) = entry
      let store_with_product = case
        store_mod.get_effective_product_by_id(current_store, product_id)
      {
        Some(_) -> current_store
        None -> {
          let products = case
            make_seed_product_relaxed(
              JObject([
                #("id", JString(product_id)),
                #("title", JString("Seed product")),
              ]),
            )
          {
            Ok(product) -> [product]
            Error(_) -> []
          }
          store_mod.upsert_base_products(current_store, products)
        }
      }
      store_mod.replace_base_media_for_product(
        store_with_product,
        product_id,
        records,
      )
    })
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_file_delete_product_media_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath_string(
      capture,
      "$.setup.productCreate.response.data.productCreate.product.id",
    )
  {
    Some(product_id) -> {
      let product_title =
        jsonpath_string(
          capture,
          "$.setup.productCreate.response.data.productCreate.product.title",
        )
        |> option.unwrap("Seed product")
      let products = case
        make_seed_product_relaxed(
          JObject([
            #("id", JString(product_id)),
            #("title", JString(product_title)),
          ]),
        )
      {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let media =
        seed_media_nodes_at(
          capture,
          "$.setup.productReadBeforeDelete.data.product.media.nodes",
          product_id,
        )
      let store =
        store_mod.upsert_base_products(proxy.store, products)
        |> store_mod.replace_base_media_for_product(product_id, media)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_product_create_media_plan_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.productCreateMedia",
    ])
  {
    False -> proxy
    True ->
      case jsonpath_string(capture, "$.mutation.variables.productId") {
        Some(product_id) -> seed_media_plan_product(product_id, proxy)
        None -> proxy
      }
  }
}

fn seed_product_update_media_plan_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.productUpdateMedia",
    ])
  {
    False -> proxy
    True ->
      case jsonpath_string(capture, "$.mutation.variables.productId") {
        Some(product_id) -> {
          let proxy = seed_media_plan_product(product_id, proxy)
          let media =
            seed_media_nodes_at(
              capture,
              "$.mutation.response.data.productUpdateMedia.media",
              product_id,
            )
          seed_media_plan_records(product_id, media, proxy)
        }
        None -> proxy
      }
  }
}

fn seed_product_delete_media_plan_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.productDeleteMedia",
    ])
  {
    False -> proxy
    True -> seed_product_delete_media_plan_preconditions_inner(capture, proxy)
  }
}

fn seed_product_delete_media_plan_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath_string(capture, "$.mutation.variables.productId") {
    Some(product_id) -> {
      let proxy = seed_media_plan_product(product_id, proxy)
      let media_ids =
        jsonpath_string_array(capture, "$.mutation.variables.mediaIds")
      let product_image_ids =
        jsonpath_string_array(
          capture,
          "$.mutation.response.data.productDeleteMedia.deletedProductImageIds",
        )
      let media =
        media_ids
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(id, index) = entry
          ProductMediaRecord(
            key: product_id <> ":media:" <> int.to_string(index) <> ":" <> id,
            product_id: product_id,
            position: index,
            id: Some(id),
            media_content_type: Some("IMAGE"),
            alt: None,
            status: Some("READY"),
            product_image_id: string_at(product_image_ids, index),
            image_url: None,
            image_width: None,
            image_height: None,
            preview_image_url: None,
            source_url: None,
          )
        })
      seed_media_plan_records(product_id, media, proxy)
    }
    None -> proxy
  }
}

fn string_at(items: List(String), index: Int) -> Option(String) {
  case items, index {
    [first, ..], 0 -> Some(first)
    [_, ..rest], index if index > 0 -> string_at(rest, index - 1)
    _, _ -> None
  }
}

fn seed_media_plan_product(
  product_id: String,
  proxy: DraftProxy,
) -> DraftProxy {
  case store_mod.get_effective_product_by_id(proxy.store, product_id) {
    Some(_) -> proxy
    None ->
      seed_product_and_base_variants(
        proxy,
        make_seed_product_relaxed(
          JObject([
            #("id", JString(product_id)),
            #("title", JString("Seed product")),
          ]),
        ),
        [],
      )
  }
}

fn seed_media_plan_records(
  product_id: String,
  media: List(ProductMediaRecord),
  proxy: DraftProxy,
) -> DraftProxy {
  case media {
    [] -> proxy
    _ -> {
      let store =
        store_mod.replace_base_media_for_product(proxy.store, product_id, media)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
  }
}

fn seed_media_nodes_at(
  capture: JsonValue,
  path: String,
  product_id: String,
) -> List(ProductMediaRecord) {
  case jsonpath.lookup(capture, path) {
    Some(JArray(nodes)) ->
      nodes
      |> enumerate_json_values()
      |> list.filter_map(fn(entry) {
        let #(node, index) = entry
        make_seed_product_media_from_node(product_id, node, index)
      })
    _ -> []
  }
}

fn make_seed_product_media(
  source: JsonValue,
  index: Int,
) -> Result(ProductMediaRecord, Nil) {
  use product_id <- result.try(required_string_field(source, "productId"))
  use id <- result.try(required_string_field(source, "id"))
  let position = read_int_field(source, "position") |> option.unwrap(index)
  let key =
    read_string_field(source, "key")
    |> option.unwrap(
      product_id <> ":media:" <> int.to_string(position) <> ":" <> id,
    )
  Ok(ProductMediaRecord(
    key: key,
    product_id: product_id,
    position: position,
    id: Some(id),
    media_content_type: read_string_field(source, "mediaContentType")
      |> option.or(Some("IMAGE")),
    alt: read_string_field(source, "alt"),
    status: read_string_field(source, "status") |> option.or(Some("READY")),
    product_image_id: read_string_field(source, "productImageId"),
    image_url: read_string_field(source, "imageUrl"),
    image_width: read_int_field(source, "imageWidth"),
    image_height: read_int_field(source, "imageHeight"),
    preview_image_url: read_string_field(source, "previewImageUrl"),
    source_url: read_string_field(source, "sourceUrl"),
  ))
}

fn group_product_media_by_product_id(
  media: List(ProductMediaRecord),
) -> List(#(String, List(ProductMediaRecord))) {
  let grouped =
    list.fold(media, dict.new(), fn(groups, record) {
      let existing = case dict.get(groups, record.product_id) {
        Ok(records) -> records
        Error(_) -> []
      }
      dict.insert(groups, record.product_id, [record, ..existing])
    })
  grouped
  |> dict.to_list
  |> list.map(fn(entry) {
    let #(product_id, records) = entry
    #(product_id, list.reverse(records))
  })
}

fn seed_options_for_product(
  store: store_mod.Store,
  product_json: JsonValue,
) -> store_mod.Store {
  case required_string_field(product_json, "id") {
    Ok(product_id) ->
      case read_array_field(product_json, "options") {
        Some(option_nodes) -> {
          let options =
            list.filter_map(option_nodes, fn(option_json) {
              make_seed_product_option(product_id, option_json)
            })
          store_mod.replace_base_options_for_product(store, product_id, options)
        }
        None -> store
      }
    Error(_) -> store
  }
}

fn make_seed_product_option(
  product_id: String,
  source: JsonValue,
) -> Result(ProductOptionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  let position = read_int_field(source, "position") |> option.unwrap(0)
  let option_values = case read_array_field(source, "optionValues") {
    Some(nodes) -> list.filter_map(nodes, make_seed_product_option_value)
    None -> []
  }
  Ok(ProductOptionRecord(
    id: id,
    product_id: product_id,
    name: name,
    position: position,
    option_values: option_values,
  ))
}

fn make_seed_product_option_value(
  source: JsonValue,
) -> Result(ProductOptionValueRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  Ok(ProductOptionValueRecord(
    id: id,
    name: name,
    has_variants: read_bool_field(source, "hasVariants") |> option.unwrap(False),
  ))
}

fn seed_product_publication_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products = case read_object_field(capture, "seedProduct") {
    Some(product_json) ->
      case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
    None -> []
  }
  let store = store_mod.upsert_base_products(proxy.store, products)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_product_feedback_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products = case read_array_field(capture, "seedProducts") {
    Some(product_nodes) ->
      list.filter_map(product_nodes, make_seed_product_relaxed)
    None -> []
  }
  let store = store_mod.upsert_base_products(proxy.store, products)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_product_metafields_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.response.data.product") {
    Some(product_json) ->
      seed_product_metafield_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_product_metafield_product_json(
  product_json: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_id = read_string_field(product_json, "id")
  let store = case make_seed_product_relaxed(product_json) {
    Ok(product) -> store_mod.upsert_base_products(proxy.store, [product])
    Error(_) -> proxy.store
  }
  let store = case product_id {
    Some(owner_id) -> {
      let metafields =
        collect_product_metafield_sources(product_json)
        |> list.filter_map(fn(source) {
          make_seed_product_metafield_for_owner(source, owner_id)
        })
        |> dedupe_product_metafields
      store_mod.replace_base_metafields_for_owner(store, owner_id, metafields)
    }
    None -> store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn collect_product_metafield_sources(
  product_json: JsonValue,
) -> List(JsonValue) {
  let primary = case read_object_field(product_json, "primarySpec") {
    Some(source) -> [source]
    None -> []
  }
  let first_page =
    read_object_field(product_json, "metafields")
    |> option.then(read_array_field(_, "nodes"))
    |> option.unwrap([])
  let next_page =
    read_object_field(product_json, "nextMetafields")
    |> option.then(read_array_field(_, "nodes"))
    |> option.unwrap([])
  list.append(primary, list.append(first_page, next_page))
}

fn seed_metafields_set_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, ["$.mutation.response.data.metafieldsSet"])
  {
    False -> proxy
    True -> seed_metafields_set_preconditions_inner(capture, proxy)
  }
}

fn seed_metafields_set_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let proxy = seed_metafields_set_collection_preconditions(capture, proxy)
  let #(proxy, seeded_from_precondition) = case
    jsonpath.lookup(capture, "$.preconditionRead.data.product")
  {
    Some(product_json) -> #(
      seed_product_metafield_product_json(product_json, proxy),
      True,
    )
    None -> #(proxy, False)
  }
  let inputs = case
    jsonpath.lookup(capture, "$.mutation.variables.metafields")
  {
    Some(JArray(items)) -> items
    _ -> []
  }
  case seeded_from_precondition {
    True -> proxy
    False -> {
      let owner_ids =
        inputs
        |> list.filter_map(fn(input) {
          case read_string_field(input, "ownerId") {
            Some(owner_id) -> Ok(owner_id)
            None -> Error(Nil)
          }
        })
        |> dedupe_strings_preserving_order
      let products =
        owner_ids
        |> list.filter_map(fn(owner_id) {
          make_seed_product_relaxed(JObject([#("id", JString(owner_id))]))
        })
      let metafield_sources = case
        jsonpath.lookup(
          capture,
          "$.mutation.response.data.metafieldsSet.metafields",
        )
      {
        Some(JArray(items)) -> items
        _ -> []
      }
      let metafields =
        metafield_sources
        |> list.filter_map(fn(source) {
          case owner_id_for_metafields_set_source(source, inputs) {
            Some(owner_id) ->
              make_seed_product_metafield_for_owner(source, owner_id)
            None -> Error(Nil)
          }
        })
        |> dedupe_product_metafields
      let store = store_mod.upsert_base_products(proxy.store, products)
      let store =
        list.fold(owner_ids, store, fn(current, owner_id) {
          let owner_metafields =
            metafields
            |> list.filter(fn(metafield) { metafield.owner_id == owner_id })
          store_mod.replace_base_metafields_for_owner(
            current,
            owner_id,
            owner_metafields,
          )
        })
      draft_proxy.DraftProxy(..proxy, store: store)
    }
  }
}

fn seed_metafields_set_collection_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let collection_sources =
    [
      read_object_field(capture, "seedCollection"),
      jsonpath.lookup(capture, "$.downstreamRead.data.collection"),
    ]
    |> list.filter_map(fn(source) {
      case source {
        Some(value) -> Ok(value)
        None -> Error(Nil)
      }
    })
  let collections =
    collection_sources
    |> list.filter_map(make_seed_collection_relaxed)
  let store = store_mod.upsert_base_collections(proxy.store, collections)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_metafields_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.metafieldsDelete",
      "$.mutation.response.data.metafieldDelete",
    ])
  {
    False -> proxy
    True -> seed_metafields_delete_preconditions_inner(capture, proxy)
  }
}

fn seed_metafields_delete_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.product") {
    Some(product_json) -> {
      let proxy = seed_product_metafield_product_json(product_json, proxy)
      case read_string_field(product_json, "id") {
        Some(owner_id) -> {
          let retained =
            collect_product_metafield_sources(product_json)
            |> list.filter_map(fn(source) {
              make_seed_product_metafield_for_owner(source, owner_id)
            })
          let material =
            ProductMetafieldRecord(
              id: "gid://shopify/Metafield/9001",
              owner_id: owner_id,
              namespace: "custom",
              key: "material",
              type_: Some("single_line_text_field"),
              value: Some("Seed material"),
              compare_digest: None,
              json_value: None,
              created_at: None,
              updated_at: None,
              owner_type: Some("PRODUCT"),
            )
          let store =
            store_mod.replace_base_metafields_for_owner(
              proxy.store,
              owner_id,
              dedupe_product_metafields([material, ..retained]),
            )
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        None -> proxy
      }
    }
    None -> proxy
  }
}

fn owner_id_for_metafields_set_source(
  source: JsonValue,
  inputs: List(JsonValue),
) -> Option(String) {
  let namespace = read_string_field(source, "namespace")
  let key = read_string_field(source, "key")
  inputs
  |> list.find_map(fn(input) {
    case
      read_string_field(input, "ownerId"),
      read_string_field(input, "namespace"),
      read_string_field(input, "key")
    {
      Some(owner_id), input_namespace, input_key
        if input_namespace == namespace && input_key == key
      -> Ok(owner_id)
      _, _, _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn seed_inventory_shipment_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_nodes = case read_array_field(capture, "seedProducts") {
    Some(product_nodes) -> product_nodes
    None -> []
  }
  let products = list.filter_map(product_nodes, make_seed_product_relaxed)
  let variants = list.flat_map(product_nodes, seed_variants_for_product)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_inventory_transfer_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_nodes = case read_array_field(capture, "seedProducts") {
    Some(product_nodes) -> product_nodes
    None -> []
  }
  let products = list.filter_map(product_nodes, make_seed_product_relaxed)
  let variants = list.flat_map(product_nodes, seed_variants_for_product)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_product_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data.product") {
    Some(product_json) ->
      case make_seed_product(product_json) {
        Ok(product) -> {
          let store =
            proxy.store
            |> store_mod.upsert_base_products([product])
            |> seed_options_for_product(product_json)
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_collection_detail_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let collection_sources =
    ["$.data.customCollection", "$.data.smartCollection"]
    |> list.filter_map(fn(path) {
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(Nil)
      }
    })
  let has_product_seed_id =
    jsonpath.lookup(
      capture,
      "$.data.customCollection.products.edges[0].node.id",
    )
    |> json_string_option
  let collections = list.filter_map(collection_sources, make_seed_collection)
  let store =
    proxy.store
    |> store_mod.upsert_base_collections(collections)
  let proxy = draft_proxy.DraftProxy(..proxy, store: store)
  list.fold(collection_sources, proxy, fn(acc, collection_json) {
    seed_collection_products(collection_json, has_product_seed_id, acc)
  })
}

fn seed_collection_products(
  collection_json: JsonValue,
  has_product_seed_id: Option(String),
  proxy: DraftProxy,
) -> DraftProxy {
  case make_seed_collection(collection_json) {
    Ok(collection) -> {
      let edges = case jsonpath.lookup(collection_json, "$.products.edges") {
        Some(JArray(edges)) -> edges
        _ -> []
      }
      let products = list.filter_map(edges, make_seed_product_relaxed_from_edge)
      let memberships =
        edges
        |> enumerate_json_values()
        |> list.filter_map(fn(pair) {
          let #(edge, position) = pair
          make_seed_product_collection_from_edge(collection.id, edge, position)
        })
      let memberships = case
        read_bool_field(collection_json, "hasProduct"),
        has_product_seed_id
      {
        Some(True), Some(product_id) ->
          case
            list.any(memberships, fn(record) { record.product_id == product_id })
          {
            True -> memberships
            False ->
              list.append(memberships, [
                ProductCollectionRecord(
                  collection_id: collection.id,
                  product_id: product_id,
                  position: list.length(memberships),
                  cursor: None,
                ),
              ])
          }
        _, _ -> memberships
      }
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.replace_base_products_for_collection(
          collection.id,
          memberships,
        )
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    Error(_) -> proxy
  }
}

fn seed_collections_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let edges = case jsonpath.lookup(capture, "$.data.collections.edges") {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  let collections =
    edges
    |> list.filter_map(make_seed_collection_from_edge)
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.titleWildcard.edges",
      "title",
    )
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.smartCollections.edges",
      "title",
    )
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.updatedNewest.edges",
      "updated_at",
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_collections(collections)
  let proxy = draft_proxy.DraftProxy(..proxy, store: store)
  list.fold(edges, proxy, fn(acc, edge) {
    case read_object_field(edge, "node") {
      Some(collection_json) ->
        seed_collection_products(collection_json, None, acc)
      None -> acc
    }
  })
}

fn seed_collection_add_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.collectionAddProducts.collection",
    )
  {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let products = list.filter_map(product_nodes, make_seed_product_relaxed)
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.second",
            target_collection_id,
          ),
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_collections(existing_memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_remove_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.collectionRemoveProducts",
    ])
  {
    False -> proxy
    True -> seed_collection_remove_products_preconditions_inner(capture, proxy)
  }
}

fn seed_collection_remove_products_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.collection") {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let collection_product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let target_memberships =
        collection_product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position + 1,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let removed_memberships =
        collection_remove_product_ids(capture)
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, position) = entry
          ProductCollectionRecord(
            collection_id: target_collection_id,
            product_id: product_id,
            position: position,
            cursor: None,
          )
        })
      let existing =
        seed_existing_product_collections(
          capture,
          "$.downstreamRead.data.untouched",
          target_collection_id,
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_product_collections(list.append(
          list.append(removed_memberships, target_memberships),
          existing_memberships,
        ))
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn collection_remove_product_ids(capture: JsonValue) -> List(String) {
  case jsonpath.lookup(capture, "$.mutation.variables.productIds") {
    Some(JArray(ids)) ->
      list.filter_map(ids, fn(id) {
        case id {
          JString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn seed_collection_reorder_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.initialCollectionRead.data.collection") {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let target_memberships =
        product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.initialCollectionRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.initialCollectionRead.data.second",
            target_collection_id,
          ),
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_product_collections(list.append(
          target_memberships,
          existing_memberships,
        ))
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.collectionUpdate.collection",
    )
  {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let products = list.filter_map(product_nodes, make_seed_product_relaxed)
      let memberships =
        product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(target_collections)
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_collections(memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.variables.input.id") {
    Some(JString(collection_id)) -> {
      let collection =
        CollectionRecord(
          id: collection_id,
          legacy_resource_id: None,
          title: "Delete parity collection",
          handle: "delete-parity-collection",
          publication_ids: [],
          updated_at: None,
          description: None,
          description_html: None,
          image: None,
          sort_order: Some("MANUAL"),
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          rule_set: None,
          products_count: Some(0),
          is_smart: False,
          cursor: None,
          title_cursor: None,
          updated_at_cursor: None,
        )
      let store = store_mod.upsert_base_collections(proxy.store, [collection])
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _ -> proxy
  }
}

fn seed_collection_create_initial_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamReadVariables.collectionId") {
    Some(JString(target_collection_id)) -> {
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.second",
            target_collection_id,
          ),
        )
      let collections =
        list.map(existing, fn(entry) {
          let #(collection, _) = entry
          collection
        })
      let memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(collections)
        |> store_mod.upsert_base_product_collections(memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _ -> proxy
  }
}

fn seed_locations_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let edges = case jsonpath.lookup(capture, "$.data.locations.edges") {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  let locations = list.filter_map(edges, make_seed_location_from_edge)
  let store = store_mod.upsert_base_locations(proxy.store, locations)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn make_seed_location_from_edge(
  edge: JsonValue,
) -> Result(LocationRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  case read_string_field(node, "id"), read_string_field(node, "name") {
    Some(id), Some(name) ->
      Ok(LocationRecord(
        id: id,
        name: name,
        cursor: read_string_field(edge, "cursor"),
      ))
    _, _ -> Error(Nil)
  }
}

fn seed_publications_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let edges = case
    jsonpath.lookup(capture, "$.payload.data.publications.edges")
  {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  let publications = list.filter_map(edges, make_seed_publication_from_edge)
  let store = store_mod.upsert_base_publications(proxy.store, publications)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn make_seed_publication_from_edge(
  edge: JsonValue,
) -> Result(PublicationRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  case read_string_field(node, "id"), read_string_field(node, "name") {
    Some(id), Some(name) ->
      Ok(PublicationRecord(
        id: id,
        name: Some(name),
        auto_publish: read_bool_field(node, "autoPublish"),
        supports_future_publishing: read_bool_field(
          node,
          "supportsFuturePublishing",
        ),
        catalog_id: read_string_field(node, "catalogId"),
        channel_id: read_string_field(node, "channelId"),
        cursor: read_string_field(edge, "cursor"),
      ))
    _, _ -> Error(Nil)
  }
}

fn seed_publication_roots_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let publications = case read_array_field(capture, "seedPublications") {
    Some(nodes) -> list.filter_map(nodes, make_seed_publication)
    None -> []
  }
  let products = case read_array_field(capture, "seedProducts") {
    Some(nodes) -> list.filter_map(nodes, make_seed_product_relaxed)
    None -> []
  }
  let collections = case read_array_field(capture, "seedCollections") {
    Some(nodes) -> list.filter_map(nodes, make_seed_collection_relaxed)
    None -> []
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_publications(publications)
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_collections(collections)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn make_seed_publication(source: JsonValue) -> Result(PublicationRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(PublicationRecord(
    id: id,
    name: read_string_field(source, "name"),
    auto_publish: read_bool_field(source, "autoPublish"),
    supports_future_publishing: read_bool_field(
      source,
      "supportsFuturePublishing",
    ),
    catalog_id: read_string_field(source, "catalogId"),
    channel_id: read_string_field(source, "channelId"),
    cursor: read_string_field(source, "cursor"),
  ))
}

fn make_seed_collection_relaxed(
  source: JsonValue,
) -> Result(CollectionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let title = read_string_field(source, "title") |> option.unwrap("")
  Ok(CollectionRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: read_string_field(source, "handle") |> option.unwrap(""),
    publication_ids: read_string_array_field(source, "publicationIds"),
    updated_at: read_string_field(source, "updatedAt"),
    description: read_string_field(source, "description"),
    description_html: read_string_field(source, "descriptionHtml"),
    image: make_seed_collection_image(read_object_field(source, "image")),
    sort_order: read_string_field(source, "sortOrder"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: make_seed_product_seo(read_object_field(source, "seo")),
    rule_set: make_seed_collection_rule_set(read_object_field(source, "ruleSet")),
    products_count: read_object_field(source, "productsCount")
      |> option.then(read_int_field(_, "count")),
    is_smart: False,
    cursor: read_string_field(source, "cursor"),
    title_cursor: None,
    updated_at_cursor: None,
  ))
}

fn seed_existing_product_collections(
  capture: JsonValue,
  product_path: String,
  target_collection_id: String,
) -> List(#(CollectionRecord, ProductCollectionRecord)) {
  case jsonpath.lookup(capture, product_path) {
    Some(product_json) -> {
      let product_id = read_string_field(product_json, "id")
      let nodes = case jsonpath.lookup(product_json, "$.collections.nodes") {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      nodes
      |> enumerate_json_values()
      |> list.filter_map(fn(entry) {
        let #(collection_json, position) = entry
        case
          make_seed_collection(collection_json),
          product_id,
          read_string_field(collection_json, "id")
        {
          Ok(collection), Some(product_id), Some(collection_id) ->
            case collection_id == target_collection_id {
              True -> Error(Nil)
              False ->
                Ok(#(
                  collection,
                  ProductCollectionRecord(
                    collection_id: collection.id,
                    product_id: product_id,
                    position: position,
                    cursor: None,
                  ),
                ))
            }
          _, _, _ -> Error(Nil)
        }
      })
    }
    None -> []
  }
}

fn make_seed_collection_from_edge(
  edge: JsonValue,
) -> Result(CollectionRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  use collection <- result.try(make_seed_collection(node))
  Ok(CollectionRecord(..collection, cursor: read_string_field(edge, "cursor")))
}

fn merge_collection_cursors_from_path(
  collections: List(CollectionRecord),
  capture: JsonValue,
  path: String,
  cursor_kind: String,
) -> List(CollectionRecord) {
  let edges = case jsonpath.lookup(capture, path) {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  list.fold(edges, collections, fn(acc, edge) {
    case
      read_object_field(edge, "node")
      |> option.then(read_string_field(_, "id")),
      read_string_field(edge, "cursor")
    {
      Some(collection_id), Some(cursor) ->
        list.map(acc, fn(collection) {
          case collection.id == collection_id, cursor_kind {
            True, "title" ->
              CollectionRecord(..collection, title_cursor: Some(cursor))
            True, "updated_at" ->
              CollectionRecord(..collection, updated_at_cursor: Some(cursor))
            _, _ -> collection
          }
        })
      _, _ -> acc
    }
  })
}

fn make_seed_collection(source: JsonValue) -> Result(CollectionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use handle <- result.try(required_string_field(source, "handle"))
  let rule_set =
    make_seed_collection_rule_set(read_object_field(source, "ruleSet"))
  Ok(CollectionRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: handle,
    publication_ids: read_string_array_field(source, "publicationIds"),
    updated_at: read_string_field(source, "updatedAt"),
    description: read_string_field(source, "description"),
    description_html: read_string_field(source, "descriptionHtml"),
    image: make_seed_collection_image(read_object_field(source, "image")),
    sort_order: read_string_field(source, "sortOrder"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: make_seed_product_seo(read_object_field(source, "seo")),
    rule_set: rule_set,
    products_count: read_object_field(source, "productsCount")
      |> option.then(read_int_field(_, "count")),
    is_smart: case rule_set {
      Some(_) -> True
      None -> False
    },
    cursor: None,
    title_cursor: None,
    updated_at_cursor: None,
  ))
}

fn make_seed_collection_image(
  source: Option(JsonValue),
) -> Option(CollectionImageRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(CollectionImageRecord(
        id: read_string_field(value, "id"),
        alt_text: read_string_field(value, "altText"),
        url: read_string_field(value, "url"),
        width: read_int_field(value, "width"),
        height: read_int_field(value, "height"),
      ))
  }
}

fn make_seed_collection_rule_set(
  source: Option(JsonValue),
) -> Option(CollectionRuleSetRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(
        CollectionRuleSetRecord(
          applied_disjunctively: read_bool_field(value, "appliedDisjunctively")
            |> option.unwrap(False),
          rules: case read_array_field(value, "rules") {
            Some(rules) -> list.filter_map(rules, make_seed_collection_rule)
            None -> []
          },
        ),
      )
  }
}

fn make_seed_collection_rule(
  source: JsonValue,
) -> Result(CollectionRuleRecord, Nil) {
  use column <- result.try(required_string_field(source, "column"))
  use relation <- result.try(required_string_field(source, "relation"))
  use condition <- result.try(required_string_field(source, "condition"))
  Ok(CollectionRuleRecord(
    column: column,
    relation: relation,
    condition: condition,
  ))
}

fn make_seed_product_collection_from_edge(
  collection_id: String,
  edge: JsonValue,
  position: Int,
) -> Result(ProductCollectionRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  use product_id <- result.try(required_string_field(node, "id"))
  Ok(ProductCollectionRecord(
    collection_id: collection_id,
    product_id: product_id,
    position: position,
    cursor: read_string_field(edge, "cursor"),
  ))
}

fn make_seed_product(source: JsonValue) -> Result(ProductRecord, Nil) {
  make_seed_product_with_cursor(source, None)
}

fn seed_products_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products = case jsonpath.lookup(capture, "$.data.products.edges") {
    Some(JArray(edges)) ->
      list.filter_map(edges, fn(edge) { make_seed_product_from_edge(edge) })
    _ -> []
  }
  let store = store_mod.upsert_base_products(proxy.store, products)
  let store = case jsonpath.lookup(capture, "$.data.productsCount.count") {
    Some(JInt(count)) -> store_mod.set_base_product_count(store, count)
    _ -> store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_markets_preconditions(
  scenario_id: String,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case scenario_id {
    "web-presence-delete-local-staging"
    | "web-presence-lifecycle-local-staging" ->
      seed_markets_web_presence_baseline_preconditions(capture, proxy)
    "price-list-fixed-prices-by-product-update"
    | "quantity-pricing-rules-local-staging" ->
      seed_markets_capture_preconditions(capture, proxy)
    "market-localization-metafield-default-validation" ->
      seed_market_localization_metafield_preconditions(capture, proxy)
    "market-catalog-detail-read"
    | "market-catalogs-read"
    | "market-detail-read"
    | "market-web-presences-read"
    | "markets-catalog-read"
    | "markets-resolved-values-read"
    | "price-list-detail-read"
    | "price-list-prices-filtered-read"
    | "price-lists-read" -> seed_markets_capture_preconditions(capture, proxy)
    _ -> proxy
  }
}

fn seed_markets_capture_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let market_records = collect_seed_market_records(capture)
  let catalog_records = collect_seed_catalog_records(capture)
  let price_list_records = collect_seed_price_list_records(capture)
  let web_presence_records = collect_seed_web_presence_records(capture)
  let store =
    proxy.store
    |> store_mod.upsert_base_markets(market_records)
    |> store_mod.upsert_base_catalogs(catalog_records)
    |> store_mod.upsert_base_price_lists(price_list_records)
    |> store_mod.upsert_base_web_presences(web_presence_records)
    |> seed_markets_root_payload(capture, "marketsResolvedValues", [
      "$.data.marketsResolvedValues",
      "$.response.payload.data.marketsResolvedValues",
    ])
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_markets_web_presence_baseline_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = case jsonpath.lookup(capture, "$.data.webPresences.nodes") {
    Some(nodes) -> collect_seed_web_presence_records(nodes)
    None -> []
  }
  let store = store_mod.upsert_base_web_presences(proxy.store, records)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_market_localization_metafield_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.productRead.response.payload.data.product",
    )
  {
    Some(product_json) ->
      seed_product_metafield_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_products_search_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products =
    list.append(
      seed_products_from_connection_path(capture, "$.data.nike.edges"),
      seed_products_from_connection_path(capture, "$.data.lowInventory.edges"),
    )
  let products = append_search_has_next_page_sentinel(capture, products)
  let store = store_mod.upsert_base_products(proxy.store, products)
  let store = case jsonpath.lookup(capture, "$.data.total.count") {
    Some(JInt(count)) -> store_mod.set_base_product_count(store, count)
    _ -> store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_products_sort_keys_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products =
    [
      "$.response.data.titleOrder.edges",
      "$.response.data.vendorOrder.edges",
      "$.response.data.productTypeOrder.edges",
      "$.response.data.publishedAtOrder.edges",
      "$.response.data.idOrder.edges",
    ]
    |> list.flat_map(fn(path) {
      seed_products_from_connection_path(capture, path)
    })
    |> list.map(fn(product) {
      ProductRecord(
        ..product,
        vendor: product.vendor |> option.or(infer_product_vendor(product.title)),
        tags: append_product_tag(product.tags, "egnition-sample-data"),
      )
    })
    |> merge_seed_products
  let store = store_mod.upsert_base_products(proxy.store, products)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_captured_product_connections_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products =
    collect_captured_product_connection_products(capture) |> merge_seed_products
  let store = store_mod.upsert_base_products(proxy.store, products)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_products_search_pagination_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.response.data.firstPage.edges",
      "$.response.data.nextPage.edges",
      "$.response.data.previousPage.edges",
    ])
  {
    False -> proxy
    True -> {
      let products =
        collect_captured_product_connection_products(capture)
        |> append_search_pagination_sentinels
        |> merge_seed_products
      let store = store_mod.upsert_base_products(proxy.store, products)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
  }
}

fn collect_captured_product_connection_products(
  capture: JsonValue,
) -> List(ProductRecord) {
  collect_objects(capture)
  |> list.flat_map(fn(value) {
    case read_array_field(value, "edges") {
      Some(edges) ->
        list.filter_map(edges, make_seed_product_relaxed_from_edge_with_cursor)
      None -> []
    }
  })
  |> list.filter(fn(product) {
    string.starts_with(product.id, "gid://shopify/Product/")
  })
}

fn append_search_pagination_sentinels(
  products: List(ProductRecord),
) -> List(ProductRecord) {
  case
    list.find(products, fn(product) {
      product.id == "gid://shopify/Product/8397257474281"
    })
  {
    Ok(product) ->
      list.append(products, [
        make_search_pagination_sentinel(product, "8397257474280"),
        make_search_pagination_sentinel(product, "8397257474279"),
        make_search_pagination_sentinel(product, "8397257474278"),
        make_search_pagination_sentinel(product, "8397257474277"),
      ])
    Error(_) -> products
  }
}

fn make_search_pagination_sentinel(
  product: ProductRecord,
  legacy_id: String,
) -> ProductRecord {
  ProductRecord(
    ..product,
    id: "gid://shopify/Product/" <> legacy_id,
    legacy_resource_id: Some(legacy_id),
    title: product.title <> " sentinel " <> legacy_id,
    handle: product.handle <> "-sentinel-" <> legacy_id,
    cursor: None,
  )
}

fn append_product_tag(tags: List(String), tag: String) -> List(String) {
  case list.contains(tags, tag) {
    True -> tags
    False -> list.append(tags, [tag])
  }
}

fn infer_product_vendor(title: String) -> Option(String) {
  case string.split(title, "|") {
    [vendor, ..] -> {
      let normalized = string.trim(vendor)
      case normalized {
        "" -> None
        _ -> Some(normalized)
      }
    }
    _ -> None
  }
}

fn merge_seed_products(products: List(ProductRecord)) -> List(ProductRecord) {
  products
  |> list.fold(dict.new(), fn(acc, product) {
    case dict.get(acc, product.id) {
      Ok(existing) ->
        dict.insert(acc, product.id, merge_seed_product(existing, product))
      Error(_) -> dict.insert(acc, product.id, product)
    }
  })
  |> dict.values
}

fn merge_seed_product(
  existing: ProductRecord,
  candidate: ProductRecord,
) -> ProductRecord {
  ProductRecord(
    ..existing,
    legacy_resource_id: candidate.legacy_resource_id
      |> option.or(existing.legacy_resource_id),
    title: non_empty_or(candidate.title, existing.title),
    handle: non_empty_or(candidate.handle, existing.handle),
    status: non_empty_or(candidate.status, existing.status),
    vendor: candidate.vendor |> option.or(existing.vendor),
    product_type: candidate.product_type |> option.or(existing.product_type),
    tags: merge_string_lists(existing.tags, candidate.tags),
    total_inventory: candidate.total_inventory
      |> option.or(existing.total_inventory),
    tracks_inventory: candidate.tracks_inventory
      |> option.or(existing.tracks_inventory),
    created_at: candidate.created_at |> option.or(existing.created_at),
    updated_at: candidate.updated_at |> option.or(existing.updated_at),
    published_at: candidate.published_at |> option.or(existing.published_at),
    description_html: non_empty_or(
      candidate.description_html,
      existing.description_html,
    ),
    online_store_preview_url: candidate.online_store_preview_url
      |> option.or(existing.online_store_preview_url),
    template_suffix: candidate.template_suffix
      |> option.or(existing.template_suffix),
    publication_ids: merge_string_lists(
      existing.publication_ids,
      candidate.publication_ids,
    ),
    contextual_pricing: candidate.contextual_pricing
      |> option.or(existing.contextual_pricing),
    cursor: candidate.cursor |> option.or(existing.cursor),
  )
}

fn non_empty_or(candidate: String, fallback: String) -> String {
  case candidate {
    "" -> fallback
    _ -> candidate
  }
}

fn merge_string_lists(left: List(String), right: List(String)) -> List(String) {
  list.fold(right, left, fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

fn seed_products_from_connection_path(
  capture: JsonValue,
  path: String,
) -> List(ProductRecord) {
  case jsonpath.lookup(capture, path) {
    Some(JArray(edges)) ->
      list.filter_map(edges, fn(edge) {
        make_seed_product_relaxed_from_edge(edge)
      })
    _ -> []
  }
}

fn make_seed_product_relaxed_from_edge(
  edge: JsonValue,
) -> Result(ProductRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) -> make_seed_product_relaxed(node)
    None -> Error(Nil)
  }
}

fn make_seed_product_relaxed_from_edge_with_cursor(
  edge: JsonValue,
) -> Result(ProductRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) -> {
      use product <- result.try(make_seed_product_relaxed(node))
      Ok(ProductRecord(..product, cursor: read_string_field(edge, "cursor")))
    }
    None -> Error(Nil)
  }
}

fn append_search_has_next_page_sentinel(
  capture: JsonValue,
  products: List(ProductRecord),
) -> List(ProductRecord) {
  case jsonpath.lookup(capture, "$.data.nike.pageInfo.hasNextPage") {
    Some(JBool(True)) ->
      case list.find(products, fn(product) { product.vendor == Some("NIKE") }) {
        Ok(product) ->
          list.append(products, [
            ProductRecord(
              ..product,
              id: "gid://shopify/Product/999999999999999",
              legacy_resource_id: Some("999999999999999"),
              title: product.title <> " (pagination sentinel)",
            ),
          ])
        Error(_) -> products
      }
    _ -> products
  }
}

fn seed_product_variants_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data.product") {
    Some(product_json) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let variants = seed_variants_for_product(product_json)
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
        |> seed_options_for_product(product_json)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_pre_mutation_product_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.preMutationRead.data.product") {
    Some(product_json) -> seed_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_product_json(product_json: JsonValue, proxy: DraftProxy) -> DraftProxy {
  let products = case make_seed_product_relaxed(product_json) {
    Ok(product) -> [product]
    Error(_) -> []
  }
  let variants = seed_variants_for_product(product_json)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
    |> seed_options_for_product(product_json)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_product_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.variables.input.id") {
    Some(JString(product_id)) -> {
      let product_json =
        JObject([
          #("id", JString(product_id)),
          #("title", JString("Product delete conformance seed")),
        ])
      seed_product_json(product_json, proxy)
    }
    _ -> proxy
  }
}

fn seed_product_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(capture, "$.mutation.response.data.productUpdate.product")
  {
    Some(product_json) -> seed_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_product_duplicate_async_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productDuplicate.productDuplicateOperation.product",
    )
  {
    Some(product_json) -> seed_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_product_duplicate_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(capture, "$.setup.sourceReadBeforeDuplicate.data.product")
  {
    Some(product_json) -> {
      let proxy = seed_product_json(product_json, proxy)
      let product_id = read_string_field(product_json, "id")
      let collection_nodes = case
        jsonpath.lookup(product_json, "$.collections.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let collections = list.filter_map(collection_nodes, make_seed_collection)
      let memberships = case product_id {
        Some(product_id) ->
          collection_nodes
          |> enumerate_json_values()
          |> list.filter_map(fn(entry) {
            let #(collection_json, position) = entry
            case read_string_field(collection_json, "id") {
              Some(collection_id) ->
                Ok(ProductCollectionRecord(
                  collection_id: collection_id,
                  product_id: product_id,
                  position: position,
                  cursor: None,
                ))
              None -> Error(Nil)
            }
          })
        None -> []
      }
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(collections)
        |> store_mod.upsert_base_product_collections(memberships)
      let store = case product_id {
        Some(owner_id) -> {
          let metafields =
            collect_product_metafield_sources(product_json)
            |> list.filter_map(fn(source) {
              make_seed_product_metafield_for_owner(source, owner_id)
            })
            |> dedupe_product_metafields
          store_mod.replace_base_metafields_for_owner(
            store,
            owner_id,
            metafields,
          )
        }
        None -> store
      }
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_product_set_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let levels =
    list.append(
      product_set_capture_levels(
        capture,
        "$.mutation.response.data.productSet.product.variants.nodes",
      ),
      product_set_capture_levels(
        capture,
        "$.update.mutation.response.data.productSet.product.variants.nodes",
      ),
    )
  let locations =
    levels
    |> list.filter_map(fn(level) {
      case read_object_field(level, "location") {
        Some(location) -> {
          use id <- result.try(required_string_field(location, "id"))
          use name <- result.try(required_string_field(location, "name"))
          Ok(LocationRecord(id: id, name: name, cursor: None))
        }
        None -> Error(Nil)
      }
    })
    |> dedupe_locations
  let store = store_mod.upsert_base_locations(proxy.store, locations)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn product_set_capture_levels(
  capture: JsonValue,
  variants_path: String,
) -> List(JsonValue) {
  case jsonpath.lookup(capture, variants_path) {
    Some(JArray(variants)) ->
      variants
      |> list.flat_map(fn(variant) {
        case jsonpath.lookup(variant, "$.inventoryItem.inventoryLevels.nodes") {
          Some(JArray(levels)) -> levels
          _ -> []
        }
      })
    _ -> []
  }
}

fn dedupe_locations(locations: List(LocationRecord)) -> List(LocationRecord) {
  let #(reversed, _) =
    list.fold(locations, #([], dict.new()), fn(acc, location) {
      let #(items, seen) = acc
      case dict.has_key(seen, location.id) {
        True -> #(items, seen)
        False -> #([location, ..items], dict.insert(seen, location.id, True))
      }
    })
  list.reverse(reversed)
}

fn seed_tags_remove_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.response.data.tagsRemove.node") {
    Some(product_json) -> {
      case make_seed_product_relaxed(product_json) {
        Ok(post_product) -> {
          let removed_tags = case
            jsonpath.lookup(capture, "$.mutation.variables")
          {
            Some(variables) -> read_string_array_field(variables, "tags")
            None -> []
          }
          let search_lagged_tags = read_tags_remove_search_lagged_tags(capture)
          let queried_tags = read_tags_remove_queried_tags(capture)
          let base_tags =
            post_product.tags
            |> list.filter(fn(tag) {
              !list.contains(queried_tags, tag)
              || list.contains(search_lagged_tags, tag)
            })
            |> list.append(
              list.filter(removed_tags, fn(tag) {
                list.contains(search_lagged_tags, tag)
              }),
            )
            |> dedupe_strings_preserving_order
          let pre_mutation_tags =
            list.append(post_product.tags, removed_tags)
            |> dedupe_strings_preserving_order
          let base_product = ProductRecord(..post_product, tags: base_tags)
          let pre_mutation_product =
            ProductRecord(..post_product, tags: pre_mutation_tags)
          let base_store =
            store_mod.upsert_base_products(proxy.store, [base_product])
          let #(_, seeded_store) =
            store_mod.upsert_staged_product(base_store, pre_mutation_product)
          draft_proxy.DraftProxy(..proxy, store: seeded_store)
        }
        Error(_) -> proxy
      }
    }
    None -> proxy
  }
}

fn read_tags_remove_search_lagged_tags(capture: JsonValue) -> List(String) {
  let variables =
    jsonpath.lookup(capture, "$.downstreamReadVariables")
    |> option.unwrap(JObject([]))
  let data =
    jsonpath.lookup(capture, "$.downstreamRead.data")
    |> option.unwrap(JObject([]))
  ["remainingQuery", "removedQuery"]
  |> list.filter_map(fn(key) {
    let tag = read_tag_query_value(read_string_field(variables, key))
    let response_key = case key {
      "remainingQuery" -> "remaining"
      _ -> "removed"
    }
    let has_nodes = case read_object_field(data, response_key) {
      Some(connection) ->
        case read_array_field(connection, "nodes") {
          Some([_, ..]) -> True
          _ -> False
        }
      None -> False
    }
    case tag, has_nodes {
      Some(tag), True -> Ok(tag)
      _, _ -> Error(Nil)
    }
  })
}

fn read_tags_remove_queried_tags(capture: JsonValue) -> List(String) {
  let variables =
    jsonpath.lookup(capture, "$.downstreamReadVariables")
    |> option.unwrap(JObject([]))
  ["remainingQuery", "removedQuery"]
  |> list.filter_map(fn(key) {
    case read_tag_query_value(read_string_field(variables, key)) {
      Some(tag) -> Ok(tag)
      None -> Error(Nil)
    }
  })
}

fn read_tag_query_value(query: Option(String)) -> Option(String) {
  case query {
    Some(raw) ->
      case string.split_once(raw, "tag:") {
        Ok(#(_, tail)) -> {
          let token = case string.split_once(string.trim(tail), " ") {
            Ok(#(head, _)) -> head
            Error(_) -> string.trim(tail)
          }
          Some(strip_query_quotes(token))
        }
        Error(_) -> None
      }
    None -> None
  }
}

fn strip_query_quotes(value: String) -> String {
  let trimmed = string.trim(value)
  let trimmed = case string.ends_with(trimmed, ")") {
    True -> string.drop_end(trimmed, 1)
    False -> trimmed
  }
  case
    string.length(trimmed) >= 2
    && {
      let first = string.slice(trimmed, 0, 1)
      let last = string.slice(trimmed, string.length(trimmed) - 1, 1)
      first == last && { first == "\"" || first == "'" }
    }
  {
    True -> string.slice(trimmed, 1, string.length(trimmed) - 2)
    False -> trimmed
  }
}

fn dedupe_strings_preserving_order(values: List(String)) -> List(String) {
  let #(reversed, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.has_key(seen, value) {
        True -> #(items, seen)
        False -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(reversed)
}

fn seed_product_variant_create_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case capture_has_any_path(capture, ["$.preMutationRead.data.product"]) {
    True -> proxy
    False ->
      case
        jsonpath.lookup(
          capture,
          "$.mutation.response.data.productVariantsBulkCreate.product",
        )
      {
        Some(product_json) -> {
          let product = make_seed_product_relaxed(product_json)
          let variants =
            seed_variants_for_product(product_json) |> take_first(1)
          seed_product_and_base_variants(proxy, product, variants)
        }
        None -> proxy
      }
  }
}

fn seed_product_variant_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.product",
    ),
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.productVariants[0]",
    )
  {
    Some(product_json), Some(variant_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = case
        make_seed_product_variant_from_product_json(product_json, variant_json)
      {
        Ok(variant) -> [
          ProductVariantRecord(..variant, sku: None, selected_options: []),
        ]
        Error(_) -> []
      }
      seed_product_and_base_variants(proxy, product, variants)
    }
    _, _ -> proxy
  }
}

fn seed_product_variants_bulk_create_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.productVariantsBulkCreate",
    ])
    && !capture_has_any_path(capture, ["$.preMutationRead.data.product"])
  {
    False -> proxy
    True ->
      case jsonpath.lookup(capture, "$.downstreamRead.data.product") {
        Some(product_json) -> {
          let product = make_seed_product_relaxed(product_json)
          let variants =
            seed_variants_for_product(product_json) |> take_first(1)
          seed_product_and_base_variants(proxy, product, variants)
        }
        None -> proxy
      }
  }
}

fn seed_product_variants_bulk_validation_atomicity_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.seed.setupOptionsResponse.data.productOptionsCreate.product",
    ),
    jsonpath.lookup(capture, "$.seed.defaultVariantId")
  {
    Some(product_json), Some(JString(default_variant_id)) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [
          ProductRecord(
            ..product,
            total_inventory: Some(0),
            tracks_inventory: Some(False),
          ),
        ]
        Error(_) -> []
      }
      let variants = case required_string_field(product_json, "id") {
        Ok(product_id) -> [
          ProductVariantRecord(
            id: default_variant_id,
            product_id: product_id,
            title: variant_title_with_fallback(
              default_selected_options_from_seed_options(product_json),
              "Default Title",
            ),
            sku: None,
            barcode: None,
            price: None,
            compare_at_price: None,
            taxable: None,
            inventory_policy: None,
            inventory_quantity: Some(0),
            selected_options: default_selected_options_from_seed_options(
              product_json,
            ),
            media_ids: [],
            inventory_item: Some(
              InventoryItemRecord(
                id: "gid://shopify/InventoryItem/0",
                tracked: Some(False),
                requires_shipping: Some(True),
                measurement: None,
                country_code_of_origin: None,
                province_code_of_origin: None,
                harmonized_system_code: None,
                inventory_levels: [],
              ),
            ),
            contextual_pricing: None,
            cursor: None,
          ),
        ]
        Error(_) -> []
      }
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
        |> seed_options_for_product(product_json)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _, _ -> proxy
  }
}

fn default_selected_options_from_seed_options(
  product_json: JsonValue,
) -> List(ProductVariantSelectedOptionRecord) {
  case read_array_field(product_json, "options") {
    Some(options) ->
      list.filter_map(options, fn(option_json) {
        use name <- result.try(required_string_field(option_json, "name"))
        use value <- result.try(default_option_value_name(option_json))
        Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
      })
    None -> []
  }
}

fn default_option_value_name(option_json: JsonValue) -> Result(String, Nil) {
  let values =
    read_array_field(option_json, "optionValues") |> option.unwrap([])
  case
    values
    |> list.find(fn(value) {
      read_bool_field(value, "hasVariants") == Some(True)
    })
    |> option.from_result
  {
    Some(value) -> required_string_field(value, "name")
    None ->
      case values {
        [first, ..] -> required_string_field(first, "name")
        [] -> Error(Nil)
      }
  }
}

fn variant_title_with_fallback(
  selected_options: List(ProductVariantSelectedOptionRecord),
  fallback: String,
) -> String {
  case selected_options {
    [] -> fallback
    _ ->
      selected_options
      |> list.map(fn(option) { option.value })
      |> string.join(" / ")
  }
}

fn seed_product_variants_bulk_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.product",
    ),
    jsonpath.lookup(capture, "$.downstreamRead.data.product.variants.nodes[0]")
  {
    Some(product_json), Some(variant_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = case
        make_seed_product_variant_from_product_json(product_json, variant_json)
      {
        Ok(variant) -> [ProductVariantRecord(..variant, sku: None)]
        Error(_) -> []
      }
      seed_product_and_base_variants(proxy, product, variants)
    }
    _, _ -> proxy
  }
}

fn seed_product_variant_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.mutation.response.data.productVariantsBulkDelete",
      "$.mutation.response.data.productVariantDelete",
    ])
  {
    False -> proxy
    True -> seed_product_variant_delete_preconditions_inner(capture, proxy)
  }
}

fn seed_product_variant_delete_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.product") {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let base_variants = seed_variants_for_product(product_json)
      let delete_variant = case
        jsonpath.lookup(capture, "$.mutation.variables.variantsIds[0]"),
        jsonpath.lookup(capture, "$.mutation.variables.productId")
      {
        Some(JString(variant_id)), Some(JString(product_id)) -> [
          minimal_seed_variant(product_id, variant_id),
        ]
        _, _ -> []
      }
      let proxy = seed_product_and_base_variants(proxy, product, base_variants)
      let staged_variants = list.append(delete_variant, base_variants)
      case product {
        Ok(product) -> {
          let store =
            store_mod.replace_staged_variants_for_product(
              proxy.store,
              product.id,
              staged_variants,
            )
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    }
    None -> proxy
  }
}

fn seed_product_variants_bulk_reorder_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.productVariantsBulkCreate.data.productVariantsBulkCreate.product",
    )
  {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = seed_variants_for_product(product_json)
      seed_product_and_base_variants(proxy, product, variants)
    }
    None -> proxy
  }
}

fn seed_product_reorder_media_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(capture, "$.setup.productCreate.data.productCreate.product"),
    jsonpath.lookup(
      capture,
      "$.setup.productCreateMedia.response.data.productCreateMedia.product",
    )
  {
    Some(product_json), Some(media_product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let proxy = seed_product_and_base_variants(proxy, product, [])
      case required_string_field(media_product_json, "id") {
        Ok(product_id) -> {
          let media = seed_media_for_product(media_product_json, product_id)
          let store =
            store_mod.replace_base_media_for_product(
              proxy.store,
              product_id,
              media,
            )
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    }
    _, _ -> proxy
  }
}

fn seed_media_for_product(
  product_json: JsonValue,
  product_id: String,
) -> List(ProductMediaRecord) {
  case jsonpath.lookup(product_json, "$.media.nodes") {
    Some(JArray(nodes)) ->
      nodes
      |> enumerate_json_values()
      |> list.filter_map(fn(entry) {
        let #(node, index) = entry
        make_seed_product_media_from_node(product_id, node, index)
      })
    _ -> []
  }
}

fn make_seed_product_media_from_node(
  product_id: String,
  source: JsonValue,
  index: Int,
) -> Result(ProductMediaRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(ProductMediaRecord(
    key: product_id <> ":media:" <> int.to_string(index) <> ":" <> id,
    product_id: product_id,
    position: index,
    id: Some(id),
    media_content_type: read_string_field(source, "mediaContentType"),
    alt: read_string_field(source, "alt"),
    status: read_string_field(source, "status"),
    product_image_id: read_string_field(source, "productImageId"),
    image_url: jsonpath_string(source, "$.image.url")
      |> option.or(read_string_field(source, "imageUrl")),
    image_width: None,
    image_height: None,
    preview_image_url: jsonpath_string(source, "$.preview.image.url")
      |> option.or(read_string_field(source, "previewImageUrl")),
    source_url: read_string_field(source, "sourceUrl"),
  ))
}

fn seed_product_relationship_roots_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let collections = case read_array_field(capture, "seedCollections") {
    Some(nodes) -> list.filter_map(nodes, make_seed_collection_relaxed)
    None -> []
  }
  let store = store_mod.upsert_base_collections(proxy.store, collections)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_inventory_quantity_roots_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case capture_has_any_path(capture, ["$.mutationEvidence.setup.productId"]) {
    False -> proxy
    True -> seed_inventory_quantity_roots_preconditions_inner(capture, proxy)
  }
}

fn seed_inventory_quantity_roots_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.productId")
    |> json_string_or("gid://shopify/Product/10171266400562")
  let variant_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.variantId")
    |> json_string_or("gid://shopify/ProductVariant/51101855646002")
  let inventory_item_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53204673823026")
  let location_0_id =
    jsonpath.lookup(
      capture,
      "$.mutationEvidence.inventorySetQuantitiesAvailable.variables.input.quantities[0].locationId",
    )
    |> json_string_or("gid://shopify/Location/106318430514")
  let location_1_id =
    jsonpath.lookup(
      capture,
      "$.mutationEvidence.inventorySetQuantitiesAvailable.variables.input.quantities[1].locationId",
    )
    |> json_string_or("gid://shopify/Location/106318463282")
  let product =
    ProductRecord(
      id: product_id,
      legacy_resource_id: None,
      title: "Inventory quantity parity seed",
      handle: "inventory-quantity-parity-seed",
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      total_inventory: Some(0),
      tracks_inventory: Some(True),
      created_at: None,
      updated_at: None,
      published_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
    )
  let variant =
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: "Default Title",
      sku: None,
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: [],
      media_ids: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(True),
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [
            inventory_quantity_seed_level(
              inventory_item_id,
              location_0_id,
              "Shop location",
            ),
            inventory_quantity_seed_level(
              inventory_item_id,
              location_1_id,
              "My Custom Location",
            ),
          ],
        ),
      ),
      contextual_pricing: None,
      cursor: None,
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_products([product])
    |> store_mod.upsert_base_product_variants([variant])
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_inventory_quantity_contracts_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.setup.product.productId",
      "$.setup.product.variantId",
      "$.setup.product.inventoryItemId",
      "$.inventorySetQuantities.variables.input.quantities[0].locationId",
      "$.downstreamRead.data.inventoryItem.inventoryLevels.nodes[0].location.name",
    ])
  {
    False -> proxy
    True ->
      seed_inventory_quantity_contracts_preconditions_inner(capture, proxy)
  }
}

fn seed_inventory_quantity_contracts_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_id =
    jsonpath.lookup(capture, "$.setup.product.productId")
    |> json_string_or("gid://shopify/Product/10172136718642")
  let variant_id =
    jsonpath.lookup(capture, "$.setup.product.variantId")
    |> json_string_or("gid://shopify/ProductVariant/51105380008242")
  let inventory_item_id =
    jsonpath.lookup(capture, "$.setup.product.inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53208220533042")
  let location_id =
    jsonpath.lookup(
      capture,
      "$.inventorySetQuantities.variables.input.quantities[0].locationId",
    )
    |> json_string_or("gid://shopify/Location/106318430514")
  let location_name =
    jsonpath.lookup(
      capture,
      "$.downstreamRead.data.inventoryItem.inventoryLevels.nodes[0].location.name",
    )
    |> json_string_or("Shop location")
  let product =
    ProductRecord(
      id: product_id,
      legacy_resource_id: None,
      title: "Inventory quantity 2026-04 contract seed",
      handle: "inventory-quantity-2026-04-contract-seed",
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      total_inventory: Some(0),
      tracks_inventory: Some(True),
      created_at: None,
      updated_at: None,
      published_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
    )
  let variant =
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: "Default Title",
      sku: None,
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: [],
      media_ids: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(True),
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [
            inventory_quantity_seed_level(
              inventory_item_id,
              location_id,
              location_name,
            ),
          ],
        ),
      ),
      contextual_pricing: None,
      cursor: None,
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_products([product])
    |> store_mod.upsert_base_product_variants([variant])
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_inventory_adjust_quantities_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.setup.products[0].productId",
      "$.mutation.response.data.inventoryAdjustQuantities.inventoryAdjustmentGroup",
    ])
  {
    False -> proxy
    True -> seed_inventory_adjust_quantities_preconditions_inner(capture, proxy)
  }
}

fn seed_inventory_adjust_quantities_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let location_id =
    jsonpath.lookup(capture, "$.setup.locationId")
    |> json_string_or("gid://shopify/Location/68509171945")
  let location_name =
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[0].location.name",
    )
    |> json_string_or("103 ossington")
  let first_product_id =
    jsonpath.lookup(capture, "$.setup.products[0].productId")
    |> json_string_or("gid://shopify/Product/9257220145385")
  let first_variant_id =
    jsonpath.lookup(capture, "$.setup.products[0].variantId")
    |> json_string_or("gid://shopify/ProductVariant/50897202381033")
  let first_inventory_item_id =
    jsonpath.lookup(capture, "$.setup.products[0].inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53044947747049")
  let second_product_id =
    jsonpath.lookup(capture, "$.setup.products[1].productId")
    |> json_string_or("gid://shopify/Product/9257220178153")
  let second_variant_id =
    jsonpath.lookup(capture, "$.setup.products[1].variantId")
    |> json_string_or("gid://shopify/ProductVariant/50897202413801")
  let second_inventory_item_id =
    jsonpath.lookup(capture, "$.setup.products[1].inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53044947779817")
  let first_available =
    jsonpath.lookup(
      capture,
      "$.setup.seedAdjustment.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[0].delta",
    )
    |> json_int_or(3)
  let second_available =
    jsonpath.lookup(
      capture,
      "$.setup.seedAdjustment.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[1].delta",
    )
    |> json_int_or(7)
  let quantity_updated_at =
    jsonpath.lookup(
      capture,
      "$.nonAvailableMutation.downstreamRead.data.firstInventoryItem.inventoryLevels.nodes[0].quantities[0].updatedAt",
    )
    |> json_string_or("2026-04-18T22:21:57Z")
  let products = [
    inventory_adjust_seed_product(
      first_product_id,
      "inventory-adjust-quantities-first",
    ),
    inventory_adjust_seed_product(
      second_product_id,
      "inventory-adjust-quantities-second",
    ),
  ]
  let variants = [
    inventory_adjust_seed_variant(
      first_product_id,
      first_variant_id,
      first_inventory_item_id,
      location_id,
      location_name,
      first_available,
      quantity_updated_at,
    ),
    inventory_adjust_seed_variant(
      second_product_id,
      second_variant_id,
      second_inventory_item_id,
      location_id,
      location_name,
      second_available,
      quantity_updated_at,
    ),
  ]
  let matching_nodes = case
    jsonpath.lookup(capture, "$.downstreamRead.data.matching.nodes")
  {
    Some(JArray(nodes)) -> nodes
    _ -> []
  }
  let matching_products =
    list.filter_map(matching_nodes, make_seed_product_relaxed)
  let matching_variants =
    list.flat_map(matching_nodes, seed_variants_for_product)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(list.append(products, matching_products))
    |> store_mod.upsert_base_product_variants(list.append(
      variants,
      matching_variants,
    ))
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn inventory_adjust_seed_product(id: String, handle: String) -> ProductRecord {
  ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: handle,
    handle: handle,
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    total_inventory: Some(0),
    tracks_inventory: Some(True),
    created_at: None,
    updated_at: None,
    published_at: None,
    description_html: "",
    online_store_preview_url: None,
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    category: None,
    publication_ids: [],
    contextual_pricing: None,
    cursor: None,
  )
}

fn inventory_adjust_seed_variant(
  product_id: String,
  variant_id: String,
  inventory_item_id: String,
  location_id: String,
  location_name: String,
  available: Int,
  quantity_updated_at: String,
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: variant_id,
    product_id: product_id,
    title: "Default Title",
    sku: None,
    barcode: None,
    price: None,
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(available),
    selected_options: [],
    media_ids: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: inventory_item_id,
        tracked: Some(True),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [
          InventoryLevelRecord(
            id: inventory_quantity_seed_level_id(inventory_item_id, location_id),
            location: InventoryLocationRecord(
              id: location_id,
              name: location_name,
            ),
            quantities: inventory_adjust_seed_quantities(
              available,
              quantity_updated_at,
            ),
            is_active: Some(True),
            cursor: None,
          ),
        ],
      ),
    ),
    contextual_pricing: None,
    cursor: None,
  )
}

fn inventory_adjust_seed_quantities(
  available: Int,
  quantity_updated_at: String,
) -> List(InventoryQuantityRecord) {
  [
    InventoryQuantityRecord(
      name: "available",
      quantity: available,
      updated_at: Some(quantity_updated_at),
    ),
    InventoryQuantityRecord(
      name: "incoming",
      quantity: 0,
      updated_at: Some(quantity_updated_at),
    ),
    InventoryQuantityRecord(name: "reserved", quantity: 0, updated_at: None),
    InventoryQuantityRecord(name: "damaged", quantity: 0, updated_at: None),
    InventoryQuantityRecord(
      name: "quality_control",
      quantity: 0,
      updated_at: None,
    ),
    InventoryQuantityRecord(name: "safety_stock", quantity: 0, updated_at: None),
    InventoryQuantityRecord(name: "committed", quantity: 0, updated_at: None),
    InventoryQuantityRecord(
      name: "on_hand",
      quantity: available,
      updated_at: None,
    ),
  ]
}

fn seed_inventory_activate_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel",
    )
  {
    Some(level_json) -> {
      let product_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.id",
        )
        |> json_string_or("gid://shopify/Product/9257220047081")
      let variant_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.id",
        )
        |> json_string_or("gid://shopify/ProductVariant/50897202282729")
      let inventory_item_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.id",
        )
        |> json_string_or("gid://shopify/InventoryItem/53044947648745")
      let product =
        ProductRecord(
          id: product_id,
          legacy_resource_id: None,
          title: "inventory-activate-parity",
          handle: "inventory-activate-parity",
          status: "ACTIVE",
          vendor: None,
          product_type: None,
          tags: [],
          total_inventory: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.totalInventory",
          )
            |> json_int_or(0)
            |> Some,
          tracks_inventory: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.tracksInventory",
          )
            |> json_bool_or(False)
            |> Some,
          created_at: None,
          updated_at: None,
          published_at: None,
          description_html: "",
          online_store_preview_url: None,
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          category: None,
          publication_ids: [],
          contextual_pricing: None,
          cursor: None,
        )
      let variant =
        ProductVariantRecord(
          id: variant_id,
          product_id: product_id,
          title: "Default Title",
          sku: None,
          barcode: None,
          price: None,
          compare_at_price: None,
          taxable: None,
          inventory_policy: None,
          inventory_quantity: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.inventoryQuantity",
          )
            |> json_int_or(0)
            |> Some,
          selected_options: [],
          media_ids: [],
          inventory_item: Some(
            InventoryItemRecord(
              id: inventory_item_id,
              tracked: jsonpath.lookup(
                capture,
                "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.tracked",
              )
                |> json_bool_or(False)
                |> Some,
              requires_shipping: None,
              measurement: None,
              country_code_of_origin: None,
              province_code_of_origin: None,
              harmonized_system_code: None,
              inventory_levels: [
                make_seed_inventory_level(level_json, None)
                |> result.unwrap(inventory_quantity_seed_level(
                  inventory_item_id,
                  jsonpath.lookup(
                    capture,
                    "$.inventoryActivateNoOp.variables.locationId",
                  )
                    |> json_string_or("gid://shopify/Location/68509171945"),
                  "103 ossington",
                )),
              ],
            ),
          ),
          contextual_pricing: None,
          cursor: None,
        )
      let store =
        proxy.store
        |> store_mod.upsert_base_products([product])
        |> store_mod.upsert_base_product_variants([variant])
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_inventory_inactive_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.setup.seedProductRead.data.product",
      "$.inventoryInactiveLifecycleDeactivate",
    ])
  {
    False -> proxy
    True ->
      seed_inventory_inactive_lifecycle_preconditions_inner(capture, proxy)
  }
}

fn seed_inventory_inactive_lifecycle_preconditions_inner(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_json = case
    jsonpath.lookup(capture, "$.setup.seedProductRead.data.product")
  {
    Some(product) -> Some(product)
    None ->
      case jsonpath.lookup(capture, "$.createdProduct") {
        Some(product) -> Some(product)
        None -> None
      }
  }
  case product_json {
    Some(product_json) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let variants = seed_variants_for_product(product_json)
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_inventory_item_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.create.response.data.productCreate.product",
    )
  {
    Some(product_json) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let variants = seed_variants_for_product(product_json)
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn inventory_quantity_seed_level(
  inventory_item_id: String,
  location_id: String,
  location_name: String,
) -> InventoryLevelRecord {
  InventoryLevelRecord(
    id: inventory_quantity_seed_level_id(inventory_item_id, location_id),
    location: InventoryLocationRecord(id: location_id, name: location_name),
    quantities: [
      InventoryQuantityRecord(name: "available", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "on_hand", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "damaged", quantity: 0, updated_at: None),
    ],
    is_active: Some(True),
    cursor: None,
  )
}

fn inventory_quantity_seed_level_id(
  inventory_item_id: String,
  location_id: String,
) -> String {
  let inventory_item_tail =
    inventory_item_id |> string.split("/") |> list.last |> result.unwrap("0")
  let location_tail =
    location_id |> string.split("/") |> list.last |> result.unwrap("0")
  "gid://shopify/InventoryLevel/" <> inventory_item_tail <> "-" <> location_tail
}

fn seed_product_and_base_variants(
  proxy: DraftProxy,
  product: Result(ProductRecord, Nil),
  variants: List(ProductVariantRecord),
) -> DraftProxy {
  case product {
    Ok(product) -> {
      let store =
        proxy.store
        |> store_mod.upsert_base_products([product])
        |> store_mod.upsert_base_product_variants(variants)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    Error(_) -> proxy
  }
}

fn make_seed_product_variant_from_product_json(
  product_json: JsonValue,
  variant_json: JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  use product_id <- result.try(required_string_field(product_json, "id"))
  make_seed_product_variant(product_id, variant_json, None)
}

fn minimal_seed_variant(
  product_id: String,
  variant_id: String,
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: variant_id,
    product_id: product_id,
    title: "Deleted variant seed",
    sku: None,
    barcode: None,
    price: None,
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: None,
  )
}

fn take_first(items: List(a), count: Int) -> List(a) {
  case items, count <= 0 {
    _, True -> []
    [], False -> []
    [first, ..rest], False -> [first, ..take_first(rest, count - 1)]
  }
}

fn make_seed_product_from_edge(edge: JsonValue) -> Result(ProductRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) -> {
      let cursor = read_string_field(edge, "cursor")
      make_seed_product_with_cursor(node, cursor)
    }
    None -> Error(Nil)
  }
}

fn make_seed_product_relaxed(source: JsonValue) -> Result(ProductRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let title = read_string_field(source, "title") |> option.unwrap("")
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: read_string_field(source, "handle") |> option.unwrap(""),
    status: read_string_field(source, "status") |> option.unwrap("ACTIVE"),
    vendor: read_string_field(source, "vendor"),
    product_type: read_string_field(source, "productType"),
    tags: read_string_array_field(source, "tags"),
    total_inventory: read_int_field(source, "totalInventory"),
    tracks_inventory: read_bool_field(source, "tracksInventory"),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    published_at: read_string_field(source, "publishedAt"),
    description_html: read_string_field(source, "descriptionHtml")
      |> option.unwrap(""),
    online_store_preview_url: read_string_field(source, "onlineStorePreviewUrl"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: make_seed_product_seo(read_object_field(source, "seo")),
    category: read_object_field(source, "category")
      |> option.then(make_seed_product_category),
    publication_ids: read_string_array_field(source, "publicationIds"),
    contextual_pricing: read_captured_json_field(source, "contextualPricing"),
    cursor: None,
  ))
}

fn make_seed_product_with_cursor(
  source: JsonValue,
  cursor: Option(String),
) -> Result(ProductRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use handle <- result.try(required_string_field(source, "handle"))
  use status <- result.try(required_string_field(source, "status"))
  let seo = make_seed_product_seo(read_object_field(source, "seo"))
  let category =
    read_object_field(source, "category")
    |> option.then(make_seed_product_category)
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: handle,
    status: status,
    vendor: read_string_field(source, "vendor"),
    product_type: read_string_field(source, "productType"),
    tags: read_string_array_field(source, "tags"),
    total_inventory: read_int_field(source, "totalInventory"),
    tracks_inventory: read_bool_field(source, "tracksInventory"),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    published_at: read_string_field(source, "publishedAt"),
    description_html: read_string_field(source, "descriptionHtml")
      |> option.unwrap(""),
    online_store_preview_url: read_string_field(source, "onlineStorePreviewUrl"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: seo,
    category: category,
    publication_ids: read_string_array_field(source, "publicationIds"),
    contextual_pricing: read_captured_json_field(source, "contextualPricing"),
    cursor: cursor,
  ))
}

fn seed_variants_for_product(source: JsonValue) -> List(ProductVariantRecord) {
  case required_string_field(source, "id") {
    Ok(product_id) ->
      case read_object_field(source, "variants") {
        Some(connection) ->
          case read_array_field(connection, "edges") {
            Some(edges) ->
              list.filter_map(edges, fn(edge) {
                make_seed_product_variant_from_edge(product_id, edge)
              })
            None ->
              case read_array_field(connection, "nodes") {
                Some(nodes) ->
                  list.filter_map(nodes, fn(node) {
                    make_seed_product_variant(product_id, node, None)
                  })
                None -> []
              }
          }
        None -> []
      }
    Error(_) -> []
  }
}

fn make_seed_product_variant(
  product_id: String,
  source: JsonValue,
  cursor: Option(String),
) -> Result(ProductVariantRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let selected_options = case read_array_field(source, "selectedOptions") {
    Some(nodes) -> list.filter_map(nodes, make_seed_selected_option)
    None -> []
  }
  Ok(ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: read_string_field(source, "title") |> option.unwrap("Default Title"),
    sku: read_string_field(source, "sku"),
    barcode: read_string_field(source, "barcode"),
    price: read_string_field(source, "price"),
    compare_at_price: read_string_field(source, "compareAtPrice"),
    taxable: read_bool_field(source, "taxable"),
    inventory_policy: read_string_field(source, "inventoryPolicy"),
    inventory_quantity: read_int_field(source, "inventoryQuantity"),
    selected_options: selected_options,
    media_ids: read_string_array_field(source, "mediaIds"),
    inventory_item: make_seed_inventory_item(read_object_field(
      source,
      "inventoryItem",
    )),
    contextual_pricing: read_captured_json_field(source, "contextualPricing"),
    cursor: cursor,
  ))
}

fn make_seed_product_variant_from_edge(
  product_id: String,
  edge: JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) ->
      make_seed_product_variant(
        product_id,
        node,
        read_string_field(edge, "cursor"),
      )
    None -> Error(Nil)
  }
}

fn make_seed_inventory_item(
  source: Option(JsonValue),
) -> Option(InventoryItemRecord) {
  case source {
    Some(value) ->
      case required_string_field(value, "id") {
        Ok(id) ->
          Some(InventoryItemRecord(
            id: id,
            tracked: read_bool_field(value, "tracked"),
            requires_shipping: read_bool_field(value, "requiresShipping"),
            measurement: make_seed_inventory_measurement(read_object_field(
              value,
              "measurement",
            )),
            country_code_of_origin: read_string_field(
              value,
              "countryCodeOfOrigin",
            ),
            province_code_of_origin: read_string_field(
              value,
              "provinceCodeOfOrigin",
            ),
            harmonized_system_code: read_string_field(
              value,
              "harmonizedSystemCode",
            ),
            inventory_levels: read_seed_inventory_levels(value),
          ))
        Error(_) -> None
      }
    None -> None
  }
}

fn make_seed_inventory_measurement(
  source: Option(JsonValue),
) -> Option(InventoryMeasurementRecord) {
  case source {
    Some(value) ->
      Some(
        InventoryMeasurementRecord(
          weight: make_seed_inventory_weight(read_object_field(value, "weight")),
        ),
      )
    None -> None
  }
}

fn make_seed_inventory_weight(
  source: Option(JsonValue),
) -> Option(InventoryWeightRecord) {
  case source {
    Some(value) ->
      case
        read_string_field(value, "unit"),
        read_inventory_weight_value(value)
      {
        Some(unit), Some(weight_value) ->
          Some(InventoryWeightRecord(unit: unit, value: weight_value))
        _, _ -> None
      }
    None -> None
  }
}

fn read_inventory_weight_value(value: JsonValue) {
  case json_value.field(value, "value") {
    Some(JInt(i)) -> Some(InventoryWeightInt(i))
    Some(JFloat(f)) -> Some(InventoryWeightFloat(f))
    _ -> None
  }
}

fn read_seed_inventory_levels(source: JsonValue) -> List(InventoryLevelRecord) {
  case read_object_field(source, "inventoryLevels") {
    Some(connection) ->
      case read_array_field(connection, "edges") {
        Some(edges) ->
          list.filter_map(edges, fn(edge) {
            make_seed_inventory_level_from_edge(edge)
          })
        None ->
          case read_array_field(connection, "nodes") {
            Some(nodes) ->
              list.filter_map(nodes, fn(node) {
                make_seed_inventory_level(node, None)
              })
            None -> []
          }
      }
    None ->
      case read_array_field(source, "inventoryLevels") {
        Some(nodes) ->
          list.filter_map(nodes, fn(node) {
            make_seed_inventory_level(node, None)
          })
        None -> []
      }
  }
}

fn make_seed_inventory_level_from_edge(
  edge: JsonValue,
) -> Result(InventoryLevelRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) ->
      make_seed_inventory_level(node, read_string_field(edge, "cursor"))
    None -> Error(Nil)
  }
}

fn make_seed_inventory_level(
  source: JsonValue,
  cursor: Option(String),
) -> Result(InventoryLevelRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use location <- result.try(
    make_seed_inventory_location(read_object_field(source, "location")),
  )
  Ok(InventoryLevelRecord(
    id: id,
    location: location,
    quantities: read_array_field(source, "quantities")
      |> option.unwrap([])
      |> list.filter_map(make_seed_inventory_quantity),
    is_active: read_bool_field(source, "isActive"),
    cursor: cursor,
  ))
}

fn make_seed_inventory_location(
  source: Option(JsonValue),
) -> Result(InventoryLocationRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use name <- result.try(required_string_field(value, "name"))
      Ok(InventoryLocationRecord(id: id, name: name))
    }
    None -> Error(Nil)
  }
}

fn make_seed_inventory_quantity(
  source: JsonValue,
) -> Result(InventoryQuantityRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  use quantity <- result.try(required_int_field(source, "quantity"))
  Ok(InventoryQuantityRecord(
    name: name,
    quantity: quantity,
    updated_at: read_string_field(source, "updatedAt"),
  ))
}

fn make_seed_selected_option(
  source: JsonValue,
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  use value <- result.try(required_string_field(source, "value"))
  Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
}

fn make_seed_product_seo(source: Option(JsonValue)) -> ProductSeoRecord {
  ProductSeoRecord(
    title: read_string_field_from_option(source, "title"),
    description: read_string_field_from_option(source, "description"),
  )
}

fn make_seed_product_category(
  source: JsonValue,
) -> Option(ProductCategoryRecord) {
  case required_string_field(source, "id") {
    Ok(id) ->
      Some(ProductCategoryRecord(
        id: id,
        full_name: read_string_field(source, "fullName") |> option.unwrap(""),
      ))
    Error(_) -> None
  }
}

fn seed_metafield_definition_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let candidates = [
    jsonpath.lookup(capture, "$.response.data.byIdentifier"),
    jsonpath.lookup(capture, "$.response.data.seedCatalog.nodes"),
    jsonpath.lookup(capture, "$.response.data.metafieldDefinitions.nodes"),
  ]
  let definition_sources =
    list.flat_map(candidates, fn(candidate) {
      case candidate {
        Some(JArray(items)) -> items
        Some(JObject(_)) -> [candidate |> option.unwrap(JNull)]
        _ -> []
      }
    })
  let definitions =
    list.filter_map(definition_sources, make_seed_metafield_definition)
    |> dedupe_metafield_definitions
  let metafields =
    list.flat_map(definition_sources, fn(source) {
      case make_seed_metafield_definition(source) {
        Ok(definition) ->
          seed_metafields_for_definition_source(source, definition)
        Error(_) -> []
      }
    })
  let seeded_store =
    proxy.store
    |> store_mod.upsert_base_metafield_definitions(definitions)
  let seeded_store =
    list.fold(metafields, seeded_store, fn(current, metafield) {
      let existing =
        store_mod.get_effective_metafields_by_owner_id(
          current,
          metafield.owner_id,
        )
      store_mod.replace_base_metafields_for_owner(
        current,
        metafield.owner_id,
        list.append(existing, [metafield]),
      )
    })
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn dedupe_metafield_definitions(
  definitions: List(MetafieldDefinitionRecord),
) -> List(MetafieldDefinitionRecord) {
  let #(_, kept) =
    list.fold(definitions, #(dict.new(), []), fn(acc, definition) {
      let #(seen, collected) = acc
      case dict.get(seen, definition.id) {
        Ok(_) -> #(seen, collected)
        Error(_) -> #(dict.insert(seen, definition.id, True), [
          definition,
          ..collected
        ])
      }
    })
  list.reverse(kept)
}

fn make_seed_metafield_definition(
  source: JsonValue,
) -> Result(MetafieldDefinitionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use namespace <- result.try(required_string_field(source, "namespace"))
  use key <- result.try(required_string_field(source, "key"))
  use owner_type <- result.try(required_string_field(source, "ownerType"))
  let type_source = read_object_field(source, "type")
  use type_name <- result.try(required_string_field_from_option(
    type_source,
    "name",
  ))
  Ok(MetafieldDefinitionRecord(
    id: id,
    name: name,
    namespace: namespace,
    key: key,
    owner_type: owner_type,
    type_: MetafieldDefinitionTypeRecord(
      name: type_name,
      category: read_string_field_from_option(type_source, "category"),
    ),
    description: read_string_field(source, "description"),
    validations: read_array_field(source, "validations")
      |> option.unwrap([])
      |> list.filter_map(make_seed_metafield_validation),
    access: read_object_field(source, "access")
      |> json_object_to_runtime_dict,
    capabilities: make_seed_metafield_capabilities(read_object_field(
      source,
      "capabilities",
    )),
    constraints: Some(
      make_seed_metafield_constraints(read_object_field(source, "constraints")),
    ),
    pinned_position: read_int_field(source, "pinnedPosition"),
    validation_status: read_string_field(source, "validationStatus")
      |> option.unwrap("ALL_VALID"),
  ))
}

fn required_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Result(String, Nil) {
  case read_string_field_from_option(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn make_seed_metafield_validation(
  source: JsonValue,
) -> Result(MetafieldDefinitionValidationRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  Ok(MetafieldDefinitionValidationRecord(
    name: name,
    value: read_string_field(source, "value"),
  ))
}

fn make_seed_metafield_capabilities(
  source: Option(JsonValue),
) -> MetafieldDefinitionCapabilitiesRecord {
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: make_seed_metafield_capability(
      source |> option.then(read_object_field(_, "adminFilterable")),
    ),
    smart_collection_condition: make_seed_metafield_capability(
      source
      |> option.then(read_object_field(_, "smartCollectionCondition")),
    ),
    unique_values: make_seed_metafield_capability(
      source |> option.then(read_object_field(_, "uniqueValues")),
    ),
  )
}

fn make_seed_metafield_capability(
  source: Option(JsonValue),
) -> MetafieldDefinitionCapabilityRecord {
  MetafieldDefinitionCapabilityRecord(
    enabled: read_bool_field_from_option(source, "enabled")
      |> option.unwrap(False),
    eligible: read_bool_field_from_option(source, "eligible")
      |> option.unwrap(True),
    status: read_string_field_from_option(source, "status"),
  )
}

fn make_seed_metafield_constraints(
  source: Option(JsonValue),
) -> MetafieldDefinitionConstraintsRecord {
  MetafieldDefinitionConstraintsRecord(
    key: read_string_field_from_option(source, "key"),
    values: source
      |> option.then(read_object_field(_, "values"))
      |> option.then(read_array_field(_, "nodes"))
      |> option.unwrap([])
      |> list.filter_map(fn(value) {
        case read_string_field(value, "value") {
          Some(v) -> Ok(MetafieldDefinitionConstraintValueRecord(value: v))
          None -> Error(Nil)
        }
      }),
  )
}

fn seed_metafields_for_definition_source(
  source: JsonValue,
  definition: MetafieldDefinitionRecord,
) -> List(ProductMetafieldRecord) {
  let nodes =
    read_object_field(source, "metafields")
    |> option.then(read_array_field(_, "nodes"))
    |> option.unwrap([])
  list.filter_map(nodes, fn(node) {
    make_seed_product_metafield(node, definition)
  })
}

fn make_seed_product_metafield(
  source: JsonValue,
  definition: MetafieldDefinitionRecord,
) -> Result(ProductMetafieldRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let owner_id =
    read_object_field(source, "owner")
    |> option.then(read_string_field(_, "id"))
    |> option.unwrap("seed-owner:" <> definition.id)
  Ok(ProductMetafieldRecord(
    id: id,
    owner_id: owner_id,
    namespace: read_string_field(source, "namespace")
      |> option.unwrap(definition.namespace),
    key: read_string_field(source, "key") |> option.unwrap(definition.key),
    type_: read_string_field(source, "type"),
    value: read_string_field(source, "value"),
    compare_digest: read_string_field(source, "compareDigest"),
    json_value: json_value.field(source, "jsonValue")
      |> option.map(runtime_json_from_json_value),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    owner_type: read_string_field(source, "ownerType")
      |> option.or(Some(definition.owner_type)),
  ))
}

fn seed_marketing_baseline_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data") {
    Some(data) -> {
      let SeedMarketingRecords(activities: activities, events: events) =
        collect_seed_marketing_records(data, None, empty_seed_marketing())
      let seeded_store =
        proxy.store
        |> store_mod.upsert_base_marketing_activities(activities)
        |> store_mod.upsert_base_marketing_events(events)
      draft_proxy.DraftProxy(..proxy, store: seeded_store)
    }
    None -> proxy
  }
}

fn empty_seed_marketing() -> SeedMarketingRecords {
  SeedMarketingRecords(activities: [], events: [])
}

fn collect_seed_marketing_records(
  value: JsonValue,
  cursor: Option(String),
  collected: SeedMarketingRecords,
) -> SeedMarketingRecords {
  case value {
    JArray(items) ->
      list.fold(items, collected, fn(acc, item) {
        collect_seed_marketing_records(item, cursor, acc)
      })
    JObject(fields) -> collect_seed_marketing_object(fields, cursor, collected)
    _ -> collected
  }
}

fn collect_seed_marketing_object(
  fields: List(#(String, JsonValue)),
  cursor: Option(String),
  collected: SeedMarketingRecords,
) -> SeedMarketingRecords {
  let edge_cursor = read_string_from_fields(fields, "cursor")
  let collected = case read_value_from_fields(fields, "node"), edge_cursor {
    Some(node), Some(node_cursor) ->
      collect_seed_marketing_records(node, Some(node_cursor), collected)
    _, _ -> collected
  }
  let collected = case read_string_from_fields(fields, "id") {
    Some(id) ->
      case string.starts_with(id, "gid://shopify/MarketingActivity/") {
        True ->
          SeedMarketingRecords(..collected, activities: [
            MarketingRecord(
              id: id,
              cursor: cursor,
              data: seed_marketing_data(fields),
            ),
            ..collected.activities
          ])
        False ->
          case string.starts_with(id, "gid://shopify/MarketingEvent/") {
            True ->
              SeedMarketingRecords(..collected, events: [
                MarketingRecord(
                  id: id,
                  cursor: cursor,
                  data: seed_marketing_data(fields),
                ),
                ..collected.events
              ])
            False -> collected
          }
      }
    None -> collected
  }
  list.fold(fields, collected, fn(acc, pair) {
    let #(name, child) = pair
    case name {
      "node" -> acc
      _ -> collect_seed_marketing_records(child, None, acc)
    }
  })
}

fn seed_marketing_data(
  fields: List(#(String, JsonValue)),
) -> Dict(String, MarketingValue) {
  fields
  |> list.map(fn(pair) {
    let #(key, value) = pair
    #(key, seed_marketing_value(value))
  })
  |> dict.from_list
}

fn seed_marketing_value(value: JsonValue) -> MarketingValue {
  case value {
    JNull -> MarketingNull
    JString(value) -> MarketingString(value)
    JBool(value) -> MarketingBool(value)
    JInt(value) -> MarketingInt(value)
    JFloat(value) -> MarketingFloat(value)
    JArray(items) -> MarketingList(list.map(items, seed_marketing_value))
    JObject(fields) -> MarketingObject(seed_marketing_data(fields))
  }
}

fn read_value_from_fields(
  fields: List(#(String, JsonValue)),
  name: String,
) -> Option(JsonValue) {
  fields
  |> list.find(fn(pair) { pair.0 == name })
  |> result.map(fn(pair) { pair.1 })
  |> option.from_result
}

fn read_string_from_fields(
  fields: List(#(String, JsonValue)),
  name: String,
) -> Option(String) {
  case read_value_from_fields(fields, name) {
    Some(JString(value)) -> Some(value)
    _ -> None
  }
}

fn make_seed_product_metafield_for_owner(
  source: JsonValue,
  owner_id: String,
) -> Result(ProductMetafieldRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use namespace <- result.try(required_string_field(source, "namespace"))
  use key <- result.try(required_string_field(source, "key"))
  Ok(ProductMetafieldRecord(
    id: id,
    owner_id: owner_id,
    namespace: namespace,
    key: key,
    type_: read_string_field(source, "type"),
    value: read_string_field(source, "value"),
    compare_digest: read_string_field(source, "compareDigest"),
    json_value: json_value.field(source, "jsonValue")
      |> option.map(runtime_json_from_json_value),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    owner_type: read_string_field(source, "ownerType")
      |> option.or(Some("PRODUCT")),
  ))
}

fn dedupe_product_metafields(
  metafields: List(ProductMetafieldRecord),
) -> List(ProductMetafieldRecord) {
  let #(_, kept) =
    list.fold(metafields, #(dict.new(), []), fn(acc, metafield) {
      let #(seen, collected) = acc
      case dict.get(seen, metafield.id) {
        Ok(_) -> #(seen, collected)
        Error(_) -> #(dict.insert(seen, metafield.id, True), [
          metafield,
          ..collected
        ])
      }
    })
  list.reverse(kept)
}

fn seed_metaobject_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case capture_has_any_path(capture, ["$.seededReads"]) {
    False -> proxy
    True -> {
      let definitions = collect_metaobject_definitions(capture)
      let metaobjects =
        collect_metaobjects(capture)
        |> filter_seed_metaobjects(matrix_seed_type_prefix(capture))
      case definitions, metaobjects {
        [], [] -> proxy
        _, _ -> {
          let seeded_store =
            proxy.store
            |> store_mod.upsert_base_metaobject_definitions(definitions)
            |> store_mod.upsert_base_metaobjects(metaobjects)
          draft_proxy.DraftProxy(..proxy, store: seeded_store)
        }
      }
    }
  }
}

/// Captures that exercise the metaobject field-type matrix (e.g.
/// `custom-data-metaobject-field-type-matrix`) deliberately seed the
/// matrix metaobject definitions but expect the proxy to create the
/// matrix metaobjects fresh; pre-seeding them would short-circuit the
/// mutation under test. The capture flags this by writing
/// `$.seed.matrixTypePrefix`; metaobjects whose `type` starts with that
/// prefix are filtered out before seeding.
fn matrix_seed_type_prefix(capture: JsonValue) -> Option(String) {
  jsonpath_string(capture, "$.seed.matrixTypePrefix")
}

fn filter_seed_metaobjects(
  metaobjects: List(MetaobjectRecord),
  matrix_prefix: Option(String),
) -> List(MetaobjectRecord) {
  case matrix_prefix {
    Some(prefix) ->
      list.filter(metaobjects, fn(metaobject) {
        !string.starts_with(metaobject.type_, prefix)
      })
    None -> metaobjects
  }
}

fn collect_metaobject_definitions(
  value: JsonValue,
) -> List(MetaobjectDefinitionRecord) {
  let current = case make_seed_metaobject_definition(value) {
    Ok(record) -> [record]
    Error(_) -> []
  }
  list.append(current, collect_metaobject_definitions_nested(value))
}

fn collect_metaobject_definitions_nested(
  value: JsonValue,
) -> List(MetaobjectDefinitionRecord) {
  case value {
    JObject(fields) ->
      list.flat_map(fields, fn(pair) { collect_metaobject_definitions(pair.1) })
    JArray(items) -> list.flat_map(items, collect_metaobject_definitions)
    _ -> []
  }
}

fn collect_metaobjects(value: JsonValue) -> List(MetaobjectRecord) {
  let current = case make_seed_metaobject(value) {
    Ok(record) -> [record]
    Error(_) -> []
  }
  list.append(current, collect_metaobjects_nested(value))
}

fn collect_metaobjects_nested(value: JsonValue) -> List(MetaobjectRecord) {
  case value {
    JObject(fields) ->
      list.flat_map(fields, fn(pair) { collect_metaobjects(pair.1) })
    JArray(items) -> list.flat_map(items, collect_metaobjects)
    _ -> []
  }
}

fn make_seed_metaobject_definition(
  source: JsonValue,
) -> Result(MetaobjectDefinitionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/MetaobjectDefinition/") {
    False -> Error(Nil)
    True -> {
      use type_ <- result.try(required_string_field(source, "type"))
      Ok(MetaobjectDefinitionRecord(
        id: id,
        type_: type_,
        name: read_string_field(source, "name"),
        description: read_string_field(source, "description"),
        display_name_key: read_string_field(source, "displayNameKey"),
        access: read_metaobject_access(read_object_field(source, "access")),
        capabilities: read_metaobject_definition_capabilities(read_object_field(
          source,
          "capabilities",
        )),
        field_definitions: read_metaobject_field_definitions(
          read_array_field(source, "fieldDefinitions") |> option.unwrap([]),
        ),
        has_thumbnail_field: read_bool_field(source, "hasThumbnailField"),
        metaobjects_count: read_int_field(source, "metaobjectsCount"),
        standard_template: read_metaobject_standard_template(read_object_field(
          source,
          "standardTemplate",
        )),
        created_at: read_string_field(source, "createdAt"),
        updated_at: read_string_field(source, "updatedAt"),
      ))
    }
  }
}

fn make_seed_metaobject(source: JsonValue) -> Result(MetaobjectRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/Metaobject/") {
    False -> Error(Nil)
    True -> {
      use handle <- result.try(required_string_field(source, "handle"))
      use type_ <- result.try(required_string_field(source, "type"))
      Ok(MetaobjectRecord(
        id: id,
        handle: handle,
        type_: type_,
        display_name: read_string_field(source, "displayName"),
        fields: read_metaobject_fields(
          read_array_field(source, "fields") |> option.unwrap([]),
        ),
        capabilities: read_metaobject_capabilities(read_object_field(
          source,
          "capabilities",
        )),
        created_at: read_string_field(source, "createdAt"),
        updated_at: read_string_field(source, "updatedAt"),
      ))
    }
  }
}

fn read_metaobject_access(
  source: Option(JsonValue),
) -> dict.Dict(String, Option(String)) {
  let base =
    dict.from_list([
      #("admin", Some("PUBLIC_READ_WRITE")),
      #("storefront", Some("NONE")),
    ])
  case source {
    Some(JObject(fields)) ->
      list.fold(fields, base, fn(acc, pair) {
        case pair.1 {
          JString(value) -> dict.insert(acc, pair.0, Some(value))
          JNull -> dict.insert(acc, pair.0, None)
          _ -> acc
        }
      })
    _ -> base
  }
}

fn read_metaobject_definition_capabilities(
  source: Option(JsonValue),
) -> MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: read_metaobject_definition_capability(source, "publishable"),
    translatable: read_metaobject_definition_capability(source, "translatable"),
    renderable: read_metaobject_definition_capability(source, "renderable"),
    online_store: read_metaobject_definition_capability(source, "onlineStore"),
  )
}

fn read_metaobject_definition_capability(
  source: Option(JsonValue),
  key: String,
) -> Option(MetaobjectDefinitionCapabilityRecord) {
  case source {
    Some(value) ->
      case read_object_field(value, key) {
        Some(capability) ->
          Some(MetaobjectDefinitionCapabilityRecord(
            read_bool_field(capability, "enabled") |> option.unwrap(False),
          ))
        None -> None
      }
    None -> None
  }
}

fn read_metaobject_field_definitions(
  values: List(JsonValue),
) -> List(MetaobjectFieldDefinitionRecord) {
  list.filter_map(values, fn(value) {
    case make_seed_metaobject_field_definition(value) {
      Ok(record) -> Ok(record)
      Error(_) -> Error(Nil)
    }
  })
}

fn make_seed_metaobject_field_definition(
  source: JsonValue,
) -> Result(MetaobjectFieldDefinitionRecord, Nil) {
  use key <- result.try(required_string_field(source, "key"))
  use type_ <- result.try(
    read_metaobject_type(read_object_field(source, "type")),
  )
  Ok(MetaobjectFieldDefinitionRecord(
    key: key,
    name: read_string_field(source, "name"),
    description: read_string_field(source, "description"),
    required: read_bool_field(source, "required"),
    type_: type_,
    validations: read_metaobject_validations(
      read_array_field(source, "validations") |> option.unwrap([]),
    ),
  ))
}

fn read_metaobject_type(
  source: Option(JsonValue),
) -> Result(MetaobjectDefinitionTypeRecord, Nil) {
  case source {
    Some(value) -> {
      use name <- result.try(required_string_field(value, "name"))
      Ok(MetaobjectDefinitionTypeRecord(
        name: name,
        category: read_string_field(value, "category"),
      ))
    }
    None -> Error(Nil)
  }
}

fn read_metaobject_validations(
  values: List(JsonValue),
) -> List(MetaobjectFieldDefinitionValidationRecord) {
  list.filter_map(values, fn(value) {
    case read_string_field(value, "name") {
      Some(name) ->
        Ok(MetaobjectFieldDefinitionValidationRecord(
          name,
          read_string_field(value, "value"),
        ))
      None -> Error(Nil)
    }
  })
}

fn read_metaobject_standard_template(
  source: Option(JsonValue),
) -> Option(MetaobjectStandardTemplateRecord) {
  case source {
    Some(value) ->
      Some(MetaobjectStandardTemplateRecord(
        read_string_field(value, "type"),
        read_string_field(value, "name"),
      ))
    None -> None
  }
}

fn read_metaobject_fields(
  values: List(JsonValue),
) -> List(MetaobjectFieldRecord) {
  list.filter_map(values, fn(value) {
    case make_seed_metaobject_field(value) {
      Ok(record) -> Ok(record)
      Error(_) -> Error(Nil)
    }
  })
}

fn make_seed_metaobject_field(
  source: JsonValue,
) -> Result(MetaobjectFieldRecord, Nil) {
  use key <- result.try(required_string_field(source, "key"))
  Ok(MetaobjectFieldRecord(
    key: key,
    type_: read_string_field(source, "type"),
    value: read_string_field(source, "value"),
    json_value: case json_value.field(source, "jsonValue") {
      Some(value) -> json_to_metaobject_value(value)
      None -> MetaobjectNull
    },
    definition: read_metaobject_field_reference(read_object_field(
      source,
      "definition",
    )),
  ))
}

fn read_metaobject_field_reference(
  source: Option(JsonValue),
) -> Option(MetaobjectFieldDefinitionReferenceRecord) {
  case source {
    Some(value) -> {
      case
        required_string_field(value, "key"),
        read_metaobject_type(read_object_field(value, "type"))
      {
        Ok(key), Ok(type_) ->
          Some(MetaobjectFieldDefinitionReferenceRecord(
            key: key,
            name: read_string_field(value, "name"),
            required: read_bool_field(value, "required"),
            type_: type_,
          ))
        _, _ -> None
      }
    }
    None -> None
  }
}

fn read_metaobject_capabilities(
  source: Option(JsonValue),
) -> MetaobjectCapabilitiesRecord {
  let publishable = case source {
    Some(value) ->
      case read_object_field(value, "publishable") {
        Some(p) ->
          Some(
            MetaobjectPublishableCapabilityRecord(read_string_field(p, "status")),
          )
        None -> None
      }
    None -> None
  }
  let online_store = case source {
    Some(value) ->
      case read_object_field(value, "onlineStore") {
        Some(online) ->
          Some(
            MetaobjectOnlineStoreCapabilityRecord(read_string_field(
              online,
              "templateSuffix",
            )),
          )
        None -> None
      }
    None -> None
  }
  MetaobjectCapabilitiesRecord(publishable, online_store)
}

fn json_to_metaobject_value(value: JsonValue) -> MetaobjectJsonValue {
  case value {
    JNull -> MetaobjectNull
    JBool(value) -> MetaobjectBool(value)
    JInt(value) -> MetaobjectInt(value)
    JFloat(value) -> MetaobjectFloat(value)
    JString(value) -> MetaobjectString(value)
    JArray(items) -> MetaobjectList(list.map(items, json_to_metaobject_value))
    JObject(fields) ->
      MetaobjectObject(
        list.map(fields, fn(pair) {
          #(pair.0, json_to_metaobject_value(pair.1))
        })
        |> dict.from_list,
      )
  }
}

fn seed_shop_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.readOnlyBaselines.shop.data.shop") {
    Some(shop_json) ->
      case make_seed_shop(shop_json) {
        Ok(shop) ->
          draft_proxy.DraftProxy(
            ..proxy,
            store: store_mod.upsert_base_shop(proxy.store, shop),
          )
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_business_entity_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let values = case jsonpath.lookup(capture, "$.data.businessEntities") {
    Some(JArray(items)) -> items
    _ ->
      [
        jsonpath.lookup(capture, "$.data.primary"),
        jsonpath.lookup(capture, "$.data.known"),
      ]
      |> list.filter_map(fn(value) {
        case value {
          Some(JObject(_) as object) -> Ok(object)
          _ -> Error(Nil)
        }
      })
  }
  let store =
    list.fold(values, proxy.store, fn(acc, value) {
      case make_store_property_record(value) {
        Ok(record) -> store_mod.upsert_base_business_entity(acc, record)
        Error(_) -> acc
      }
    })
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_b2b_company_roots_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let companies = case jsonpath.lookup(capture, "$.data.companies.nodes") {
    Some(JArray(nodes)) -> nodes
    _ -> []
  }
  let store =
    list.fold(companies, proxy.store, fn(acc, company) {
      seed_b2b_company_graph(acc, company)
    })
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_b2b_company_graph(
  store: store_mod.Store,
  company_value: JsonValue,
) -> store_mod.Store {
  case make_seed_b2b_company(company_value) {
    Error(_) -> store
    Ok(company) -> {
      let store = store_mod.upsert_base_b2b_company(store, company)
      let store =
        connection_nodes(company_value, "contactRoles")
        |> list.append(
          optional_object_as_list(read_object_field(
            company_value,
            "defaultRole",
          )),
        )
        |> list.fold(store, fn(acc, role_value) {
          case make_seed_b2b_company_contact_role(role_value, company.id) {
            Ok(role) ->
              store_mod.upsert_base_b2b_company_contact_role(acc, role)
            Error(_) -> acc
          }
        })
      let store =
        connection_nodes(company_value, "locations")
        |> list.fold(store, fn(acc, location_value) {
          case make_seed_b2b_company_location(location_value, company.id) {
            Ok(location) ->
              store_mod.upsert_base_b2b_company_location(acc, location)
            Error(_) -> acc
          }
        })
      optional_object_as_list(read_object_field(company_value, "mainContact"))
      |> list.append(connection_nodes(company_value, "contacts"))
      |> list.fold(store, fn(acc, contact_value) {
        case make_seed_b2b_company_contact(contact_value, company.id) {
          Ok(contact) -> store_mod.upsert_base_b2b_company_contact(acc, contact)
          Error(_) -> acc
        }
      })
    }
  }
}

fn make_seed_b2b_company(value: JsonValue) -> Result(B2BCompanyRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  Ok(B2BCompanyRecord(
    id: id,
    cursor: read_string_field(value, "cursor"),
    data: store_property_object_data(value),
    contact_ids: connection_nodes(value, "contacts") |> node_ids,
    location_ids: connection_nodes(value, "locations") |> node_ids,
    contact_role_ids: connection_nodes(value, "contactRoles") |> node_ids,
  ))
}

fn make_seed_b2b_company_contact(
  value: JsonValue,
  company_id: String,
) -> Result(B2BCompanyContactRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  Ok(B2BCompanyContactRecord(
    id: id,
    cursor: read_string_field(value, "cursor"),
    company_id: company_id,
    data: store_property_object_data(value),
  ))
}

fn make_seed_b2b_company_contact_role(
  value: JsonValue,
  company_id: String,
) -> Result(B2BCompanyContactRoleRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  Ok(B2BCompanyContactRoleRecord(
    id: id,
    cursor: read_string_field(value, "cursor"),
    company_id: company_id,
    data: store_property_object_data(value),
  ))
}

fn make_seed_b2b_company_location(
  value: JsonValue,
  company_id: String,
) -> Result(B2BCompanyLocationRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  Ok(B2BCompanyLocationRecord(
    id: id,
    cursor: read_string_field(value, "cursor"),
    company_id: company_id,
    data: store_property_object_data(value),
  ))
}

fn connection_nodes(value: JsonValue, field: String) -> List(JsonValue) {
  case read_object_field(value, field) {
    Some(connection) ->
      read_array_field(connection, "nodes") |> option.unwrap([])
    None -> []
  }
}

fn optional_object_as_list(value: Option(JsonValue)) -> List(JsonValue) {
  case value {
    Some(JObject(_) as object) -> [object]
    _ -> []
  }
}

fn node_ids(nodes: List(JsonValue)) -> List(String) {
  nodes
  |> list.filter_map(fn(node) {
    case read_string_field(node, "id") {
      Some(id) -> Ok(id)
      None -> Error(Nil)
    }
  })
}

fn seed_location_detail_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let values =
    [
      jsonpath.lookup(capture, "$.readOnlyBaselines.location.data.primary"),
      jsonpath.lookup(capture, "$.readOnlyBaselines.location.data.byId"),
      jsonpath.lookup(capture, "$.readOnlyBaselines.location.data.byIdentifier"),
    ]
    |> list.filter_map(fn(value) {
      case value {
        Some(JObject(_) as object) -> Ok(object)
        _ -> Error(Nil)
      }
    })
  let store =
    list.fold(values, proxy.store, fn(acc, value) {
      case make_store_property_record(value) {
        Ok(record) -> store_mod.upsert_base_store_property_location(acc, record)
        Error(_) -> acc
      }
    })
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_location_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.cases[2].variables.locationId") {
    Some(JString(id)) -> {
      let record =
        StorePropertyRecord(
          id: id,
          cursor: None,
          data: dict.from_list([
            #("__typename", StorePropertyString("Location")),
            #("id", StorePropertyString(id)),
            #("name", StorePropertyString("Captured location")),
            #("isActive", StorePropertyBool(True)),
          ]),
        )
      draft_proxy.DraftProxy(
        ..proxy,
        store: store_mod.upsert_base_store_property_location(
          proxy.store,
          record,
        ),
      )
    }
    _ -> proxy
  }
}

/// Publishable mutations (publishablePublish / publishableUnpublish and
/// their *ToCurrentChannel siblings) are not natively executed by the
/// proxy — instead, the captured Shopify response payload is stuffed
/// into the store's mutation-payload table keyed by `<root>:<id>`. The
/// proxy's publishable handler reads that table when it sees the
/// matching mutation, then projects the stored payload back as the
/// response.
///
/// This is a known shortcut documented in the Gleam port log; treat it
/// as cheating that needs replacing with a real publishable engine
/// later. The work here is to keep it data-driven so new publishable
/// scenarios slot in by capture shape alone.
///
/// Markers handled (each detected and seeded independently):
///   - Collection publish lifecycle: `$.publishMutation.response.data.publishablePublish`
///     paired with `$.unpublishMutation.response.data.publishableUnpublish`.
///   - Product aggregate publish: `$.aggregateSelection.response.payload.data.productPublish.product`
///     (or `productUnpublish.product`); seeded under both the generic
///     and `*ToCurrentChannel` root names so either mutation hits the
///     same payload.
///   - Generic publishable shop-count payload: any of
///     `$.mutation.response.data.publishable{Publish,Unpublish,
///     PublishToCurrentChannel,UnpublishToCurrentChannel}` paired with
///     `$.mutation.variables.id`.
fn seed_publishable_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let publishable_root_names = [
    "publishablePublish",
    "publishableUnpublish",
    "publishablePublishToCurrentChannel",
    "publishableUnpublishToCurrentChannel",
  ]

  let store = proxy.store

  // Collection publish/unpublish lifecycle (paired mutations under
  // distinct keys in the capture).
  let store =
    seed_payload_from_capture(
      store,
      capture,
      "$.publishMutation.variables.id",
      "publishablePublish",
      "$.publishMutation.response.data.publishablePublish",
    )
  let store =
    seed_payload_from_capture(
      store,
      capture,
      "$.unpublishMutation.variables.id",
      "publishableUnpublish",
      "$.unpublishMutation.response.data.publishableUnpublish",
    )

  // Generic publishable shop-count payload — capture exposes one of the
  // four root fields under `$.mutation.response.data`; seed whichever
  // exists.
  let store =
    list.fold(publishable_root_names, store, fn(acc, root_name) {
      seed_payload_from_capture(
        acc,
        capture,
        "$.mutation.variables.id",
        root_name,
        "$.mutation.response.data." <> root_name,
      )
    })

  // Product aggregate publish/unpublish — seed under all relevant root
  // names so whichever variant the proxy_request fires, the proxy hits
  // the seeded payload.
  let store =
    seed_product_aggregate_payload(
      store,
      capture,
      ["publishablePublish", "publishablePublishToCurrentChannel"],
      "$.aggregateSelection.response.payload.data.productPublish.product",
      "$.aggregateSelection.response.payload.data.productPublish.userErrors",
    )
  let store =
    seed_product_aggregate_payload(
      store,
      capture,
      ["publishableUnpublish", "publishableUnpublishToCurrentChannel"],
      "$.aggregateSelection.response.payload.data.productUnpublish.product",
      "$.aggregateSelection.response.payload.data.productUnpublish.userErrors",
    )

  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_payload_from_capture(
  store: store_mod.Store,
  capture: JsonValue,
  id_path: String,
  root_name: String,
  payload_path: String,
) -> store_mod.Store {
  case
    jsonpath.lookup(capture, id_path),
    jsonpath.lookup(capture, payload_path)
  {
    Some(JString(id)), Some(JObject(_) as payload) ->
      store_mod.upsert_base_store_property_mutation_payload(
        store,
        StorePropertyMutationPayloadRecord(
          key: root_name <> ":" <> id,
          data: store_property_object_data(payload),
        ),
      )
    _, _ -> store
  }
}

fn seed_product_aggregate_payload(
  store: store_mod.Store,
  capture: JsonValue,
  root_names: List(String),
  product_path: String,
  user_errors_path: String,
) -> store_mod.Store {
  case
    jsonpath.lookup(capture, "$.seedProduct.id"),
    jsonpath.lookup(capture, product_path)
  {
    Some(JString(id)), Some(JObject(_) as product) -> {
      let user_errors =
        jsonpath.lookup(capture, user_errors_path)
        |> option.unwrap(JArray([]))
      let payload =
        JObject([
          #("publishable", product),
          #("userErrors", user_errors),
        ])
      list.fold(root_names, store, fn(acc, root_name) {
        seed_payload_from_value(acc, id, root_name, payload)
      })
    }
    _, _ -> store
  }
}

fn seed_payload_from_value(
  store: store_mod.Store,
  id: String,
  root_name: String,
  payload: JsonValue,
) -> store_mod.Store {
  case payload {
    JObject(_) ->
      store_mod.upsert_base_store_property_mutation_payload(
        store,
        StorePropertyMutationPayloadRecord(
          key: root_name <> ":" <> id,
          data: store_property_object_data(payload),
        ),
      )
    _ -> store
  }
}

fn make_store_property_record(
  value: JsonValue,
) -> Result(StorePropertyRecord, Nil) {
  use id <- result.try(required_string_field(value, "id"))
  Ok(StorePropertyRecord(
    id: id,
    cursor: read_string_field(value, "cursor"),
    data: store_property_object_data(value),
  ))
}

fn store_property_object_data(
  value: JsonValue,
) -> Dict(String, StorePropertyValue) {
  case value {
    JObject(entries) ->
      entries
      |> list.map(fn(pair) { #(pair.0, store_property_value(pair.1)) })
      |> dict.from_list
    _ -> dict.new()
  }
}

fn store_property_value(value: JsonValue) -> StorePropertyValue {
  case value {
    JString(value) -> StorePropertyString(value)
    JBool(value) -> StorePropertyBool(value)
    JInt(value) -> StorePropertyInt(value)
    JFloat(value) -> StorePropertyFloat(value)
    JArray(values) -> StorePropertyList(list.map(values, store_property_value))
    JObject(values) ->
      StorePropertyObject(
        values
        |> list.map(fn(pair) { #(pair.0, store_property_value(pair.1)) })
        |> dict.from_list,
      )
    _ -> StorePropertyNull
  }
}

fn make_seed_shop(source: JsonValue) -> Result(ShopRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use myshopify_domain <- result.try(required_string_field(
    source,
    "myshopifyDomain",
  ))
  use url <- result.try(required_string_field(source, "url"))
  use primary_domain <- result.try(
    make_seed_shop_domain(read_object_field(source, "primaryDomain")),
  )
  use shop_address <- result.try(
    make_seed_shop_address(read_object_field(source, "shopAddress")),
  )
  use plan <- result.try(make_seed_shop_plan(read_object_field(source, "plan")))
  use resource_limits <- result.try(
    make_seed_resource_limits(read_object_field(source, "resourceLimits")),
  )
  use features <- result.try(
    make_seed_shop_features(read_object_field(source, "features")),
  )
  let payment_settings =
    make_seed_payment_settings(read_object_field(source, "paymentSettings"))
  let policies =
    read_array_field(source, "shopPolicies")
    |> option.unwrap([])
    |> list.filter_map(make_seed_shop_policy)
  Ok(ShopRecord(
    id: id,
    name: name,
    myshopify_domain: myshopify_domain,
    url: url,
    primary_domain: primary_domain,
    contact_email: read_string_field(source, "contactEmail")
      |> option.unwrap(""),
    email: read_string_field(source, "email") |> option.unwrap(""),
    currency_code: read_string_field(source, "currencyCode")
      |> option.unwrap(""),
    enabled_presentment_currencies: read_string_array_field(
      source,
      "enabledPresentmentCurrencies",
    ),
    iana_timezone: read_string_field(source, "ianaTimezone")
      |> option.unwrap(""),
    timezone_abbreviation: read_string_field(source, "timezoneAbbreviation")
      |> option.unwrap(""),
    timezone_offset: read_string_field(source, "timezoneOffset")
      |> option.unwrap(""),
    timezone_offset_minutes: read_int_field(source, "timezoneOffsetMinutes")
      |> option.unwrap(0),
    taxes_included: read_bool_field(source, "taxesIncluded")
      |> option.unwrap(False),
    tax_shipping: read_bool_field(source, "taxShipping")
      |> option.unwrap(False),
    unit_system: read_string_field(source, "unitSystem") |> option.unwrap(""),
    weight_unit: read_string_field(source, "weightUnit") |> option.unwrap(""),
    shop_address: shop_address,
    plan: plan,
    resource_limits: resource_limits,
    features: features,
    payment_settings: payment_settings,
    shop_policies: policies,
  ))
}

fn make_seed_shop_domain(
  source: Option(JsonValue),
) -> Result(ShopDomainRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use host <- result.try(required_string_field(value, "host"))
      use url <- result.try(required_string_field(value, "url"))
      Ok(ShopDomainRecord(
        id: id,
        host: host,
        url: url,
        ssl_enabled: read_bool_field(value, "sslEnabled")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_address(
  source: Option(JsonValue),
) -> Result(ShopAddressRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      Ok(ShopAddressRecord(
        id: id,
        address1: read_string_field(value, "address1"),
        address2: read_string_field(value, "address2"),
        city: read_string_field(value, "city"),
        company: read_string_field(value, "company"),
        coordinates_validated: read_bool_field(value, "coordinatesValidated")
          |> option.unwrap(False),
        country: read_string_field(value, "country"),
        country_code_v2: read_string_field(value, "countryCodeV2"),
        formatted: read_string_array_field(value, "formatted"),
        formatted_area: read_string_field(value, "formattedArea"),
        latitude: read_float_field(value, "latitude"),
        longitude: read_float_field(value, "longitude"),
        phone: read_string_field(value, "phone"),
        province: read_string_field(value, "province"),
        province_code: read_string_field(value, "provinceCode"),
        zip: read_string_field(value, "zip"),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_plan(
  source: Option(JsonValue),
) -> Result(ShopPlanRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopPlanRecord(
        partner_development: read_bool_field(value, "partnerDevelopment")
          |> option.unwrap(False),
        public_display_name: read_string_field(value, "publicDisplayName")
          |> option.unwrap(""),
        shopify_plus: read_bool_field(value, "shopifyPlus")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_resource_limits(
  source: Option(JsonValue),
) -> Result(ShopResourceLimitsRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopResourceLimitsRecord(
        location_limit: read_int_field(value, "locationLimit")
          |> option.unwrap(0),
        max_product_options: read_int_field(value, "maxProductOptions")
          |> option.unwrap(0),
        max_product_variants: read_int_field(value, "maxProductVariants")
          |> option.unwrap(0),
        redirect_limit_reached: read_bool_field(value, "redirectLimitReached")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_shop_features(
  source: Option(JsonValue),
) -> Result(ShopFeaturesRecord, Nil) {
  case source {
    Some(value) -> {
      let bundles = case read_object_field(value, "bundles") {
        Some(b) ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: read_bool_field(b, "eligibleForBundles")
              |> option.unwrap(False),
            ineligibility_reason: read_string_field(b, "ineligibilityReason"),
            sells_bundles: read_bool_field(b, "sellsBundles")
              |> option.unwrap(False),
          )
        None ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: False,
            ineligibility_reason: None,
            sells_bundles: False,
          )
      }
      let operations = case
        read_object_field(value, "cartTransform")
        |> option.then(fn(cart) {
          read_object_field(cart, "eligibleOperations")
        })
      {
        Some(op) ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: read_bool_field(op, "expandOperation")
              |> option.unwrap(False),
            merge_operation: read_bool_field(op, "mergeOperation")
              |> option.unwrap(False),
            update_operation: read_bool_field(op, "updateOperation")
              |> option.unwrap(False),
          )
        None ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: False,
            merge_operation: False,
            update_operation: False,
          )
      }
      Ok(ShopFeaturesRecord(
        avalara_avatax: read_bool_field(value, "avalaraAvatax")
          |> option.unwrap(False),
        branding: read_string_field(value, "branding") |> option.unwrap(""),
        bundles: bundles,
        captcha: read_bool_field(value, "captcha") |> option.unwrap(False),
        cart_transform: ShopCartTransformFeatureRecord(
          eligible_operations: operations,
        ),
        dynamic_remarketing: read_bool_field(value, "dynamicRemarketing")
          |> option.unwrap(False),
        eligible_for_subscription_migration: read_bool_field(
          value,
          "eligibleForSubscriptionMigration",
        )
          |> option.unwrap(False),
        eligible_for_subscriptions: read_bool_field(
          value,
          "eligibleForSubscriptions",
        )
          |> option.unwrap(False),
        gift_cards: read_bool_field(value, "giftCards") |> option.unwrap(False),
        harmonized_system_code: read_bool_field(value, "harmonizedSystemCode")
          |> option.unwrap(False),
        legacy_subscription_gateway_enabled: read_bool_field(
          value,
          "legacySubscriptionGatewayEnabled",
        )
          |> option.unwrap(False),
        live_view: read_bool_field(value, "liveView") |> option.unwrap(False),
        paypal_express_subscription_gateway_status: read_string_field(
          value,
          "paypalExpressSubscriptionGatewayStatus",
        )
          |> option.unwrap(""),
        reports: read_bool_field(value, "reports") |> option.unwrap(False),
        sells_subscriptions: read_bool_field(value, "sellsSubscriptions")
          |> option.unwrap(False),
        show_metrics: read_bool_field(value, "showMetrics")
          |> option.unwrap(False),
        storefront: read_bool_field(value, "storefront") |> option.unwrap(False),
        unified_markets: read_bool_field(value, "unifiedMarkets")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_payment_settings(
  source: Option(JsonValue),
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: case source {
    Some(value) -> read_string_array_field(value, "supportedDigitalWallets")
    None -> []
  })
}

fn make_seed_shop_policy(source: JsonValue) -> Result(ShopPolicyRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use body <- result.try(required_string_field(source, "body"))
  use type_ <- result.try(required_string_field(source, "type"))
  use url <- result.try(required_string_field(source, "url"))
  use created_at <- result.try(required_string_field(source, "createdAt"))
  use updated_at <- result.try(required_string_field(source, "updatedAt"))
  Ok(ShopPolicyRecord(
    id: id,
    title: title,
    body: body,
    type_: type_,
    url: url,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn seed_shopify_function_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    capture_has_any_path(capture, [
      "$.seedShopifyFunctions",
      "$.primary.response.data.cartTransformCreate",
      "$.primary.response.data.validationCreate",
      "$.primary.response.data.taxAppConfigure",
    ])
  {
    False -> proxy
    True -> {
      let seeded = seed_shopify_function_records(capture, proxy)
      case capture_has_any_path(capture, ["$.seedDiscounts"]) {
        True -> seeded
        False -> advance_shopify_function_seed_identity(seeded)
      }
    }
  }
}

fn advance_shopify_function_seed_identity(proxy: DraftProxy) -> DraftProxy {
  // The local-runtime fixture was captured after the function metadata
  // seed step had advanced the synthetic counters once.
  let #(_, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      proxy.synthetic_identity,
      "MutationLogEntry",
    )
  let #(_, identity_after_seed) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)

  draft_proxy.DraftProxy(..proxy, synthetic_identity: identity_after_seed)
}

fn seed_shopify_function_records(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = case jsonpath.lookup(capture, "$.seedShopifyFunctions") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_shopify_function)
    _ -> []
  }

  let seeded_store =
    list.fold(records, proxy.store, fn(current_store, record) {
      let #(_, next_store) =
        store_mod.upsert_staged_shopify_function(current_store, record)
      next_store
    })

  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn make_seed_shopify_function(
  source: JsonValue,
) -> Result(ShopifyFunctionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(
    ShopifyFunctionRecord(
      id: id,
      title: read_string_field(source, "title"),
      handle: read_string_field(source, "handle"),
      api_type: read_string_field(source, "apiType"),
      description: read_string_field(source, "description"),
      app_key: read_string_field(source, "appKey"),
      app: case read_object_field(source, "app") {
        Some(app) -> Some(make_seed_shopify_function_app(app))
        None -> None
      },
    ),
  )
}

fn make_seed_shopify_function_app(
  source: JsonValue,
) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: read_string_field(source, "__typename"),
    id: read_string_field(source, "id"),
    title: read_string_field(source, "title"),
    handle: read_string_field(source, "handle"),
    api_key: read_string_field(source, "apiKey"),
  )
}

fn seed_shipping_package_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let packages = case jsonpath.lookup(capture, "$.seed.shippingPackages") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_shipping_package)
    _ -> []
  }
  let store = store_mod.upsert_base_shipping_packages(proxy.store, packages)
  let #(_, identity_after_seed) =
    synthetic_identity.make_synthetic_timestamp(proxy.synthetic_identity)
  draft_proxy.DraftProxy(
    ..proxy,
    store: store,
    synthetic_identity: identity_after_seed,
  )
}

fn seed_shipping_settings_package_pickup_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let carrier_services = case
    jsonpath.lookup(capture, "$.seed.carrierServices")
  {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_carrier_service)
    _ -> []
  }
  let locations = case jsonpath.lookup(capture, "$.seed.locations") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_store_property_record)
    _ -> []
  }
  let store =
    store_mod.upsert_base_carrier_services(proxy.store, carrier_services)
  let store =
    list.fold(locations, store, fn(acc, location) {
      store_mod.upsert_base_store_property_location(acc, location)
    })
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_delivery_profile_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let profiles = case
    jsonpath.lookup(
      capture,
      "$.queries.detail.result.payload.data.deliveryProfile",
    )
  {
    Some(profile_source) ->
      case make_seed_delivery_profile(profile_source, capture) {
        Ok(profile) -> [profile]
        Error(_) -> []
      }
    None -> []
  }
  let store = store_mod.upsert_base_delivery_profiles(proxy.store, profiles)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_delivery_profile_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let profile_items = case
    jsonpath.lookup(
      capture,
      "$.mutations.nestedCreate.result.payload.data.deliveryProfileCreate.profile.profileItems.nodes",
    )
  {
    Some(JArray(nodes)) -> nodes
    _ -> []
  }
  let products =
    profile_items
    |> list.filter_map(fn(item) {
      case read_object_field(item, "product") {
        Some(product) -> make_seed_product_relaxed(product)
        None -> Error(Nil)
      }
    })
  let variants =
    profile_items
    |> list.flat_map(fn(item) {
      let product_id =
        read_object_field(item, "product")
        |> option.then(fn(product) { read_string_field(product, "id") })
      case product_id, read_object_field(item, "variants") {
        Some(id), Some(connection) ->
          case read_array_field(connection, "nodes") {
            Some(nodes) ->
              list.filter_map(nodes, fn(node) {
                make_seed_product_variant(id, node, None)
              })
            None -> []
          }
        _, _ -> []
      }
    })
  let default_profiles = case
    jsonpath.lookup(capture, "$.mutations.defaultRemove.variables.id")
  {
    Some(JString(id)) -> [default_delivery_profile_seed(id)]
    _ -> []
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
    |> store_mod.upsert_base_delivery_profiles(default_profiles)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_fulfillment_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let detail_order =
    jsonpath.lookup(capture, "$.detailRead.response.data.order")
  let detail_order_id =
    detail_order
    |> option.then(fn(order) { read_string_field(order, "id") })
  let shipping_orders =
    detail_order
    |> option.map(fn(order) { [make_seed_shipping_order(order)] })
    |> option.unwrap([])
  let order_records =
    detail_order
    |> option.map(make_seed_order_list)
    |> option.unwrap([])
  let direct_fulfillments = case
    jsonpath.lookup(capture, "$.detailRead.response.data.fulfillment")
  {
    Some(value) -> [make_seed_fulfillment(value, detail_order_id)]
    None -> []
  }
  let order_fulfillments = case
    jsonpath.lookup(capture, "$.detailRead.response.data.order.fulfillments")
  {
    Some(JArray(nodes)) ->
      nodes
      |> list.map(fn(node) { make_seed_fulfillment(node, detail_order_id) })
    _ -> []
  }
  let direct_fulfillment_orders = case
    jsonpath.lookup(capture, "$.detailRead.response.data.fulfillmentOrder")
  {
    Some(value) -> [make_seed_fulfillment_order(value, detail_order_id)]
    None -> []
  }
  let nested_fulfillment_orders = case
    jsonpath.lookup(
      capture,
      "$.detailRead.response.data.order.fulfillmentOrders.nodes",
    )
  {
    Some(JArray(nodes)) ->
      nodes
      |> list.map(fn(node) {
        make_seed_fulfillment_order(node, detail_order_id)
      })
    _ -> []
  }
  let catalog_fulfillment_orders = case
    jsonpath.lookup(
      capture,
      "$.catalogRead.response.data.allFulfillmentOrders.nodes",
    )
  {
    Some(JArray(nodes)) ->
      nodes
      |> list.map(fn(node) {
        make_seed_fulfillment_order(node, detail_order_id)
      })
    _ -> []
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_orders(order_records)
    |> store_mod.upsert_base_shipping_orders(shipping_orders)
    |> store_mod.upsert_base_fulfillments(
      merge_fulfillments(list.append(direct_fulfillments, order_fulfillments)),
    )
    |> store_mod.upsert_base_fulfillment_orders(
      merge_fulfillment_orders(list.append(
        direct_fulfillment_orders,
        list.append(nested_fulfillment_orders, catalog_fulfillment_orders),
      )),
    )
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_assigned_fulfillment_orders_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let order = jsonpath.lookup(capture, "$.seedOrder")
  let order_id =
    order
    |> option.then(fn(value) { read_string_field(value, "id") })
  let shipping_orders =
    order
    |> option.map(fn(value) { [make_seed_shipping_order(value)] })
    |> option.unwrap([])
  let order_records =
    order
    |> option.map(make_seed_order_list)
    |> option.unwrap([])
  let fulfillment_orders = case
    jsonpath.lookup(capture, "$.seedOrder.fulfillmentOrders.nodes")
  {
    Some(JArray(nodes)) ->
      nodes
      |> list.map(fn(node) { make_seed_fulfillment_order(node, order_id) })
    _ -> []
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_orders(order_records)
    |> store_mod.upsert_base_shipping_orders(shipping_orders)
    |> store_mod.upsert_base_fulfillment_orders(fulfillment_orders)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_fulfillment_order_request_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let fulfillment_orders =
    [
      "$.partialSubmit.response.data.fulfillmentOrderSubmitFulfillmentRequest.originalFulfillmentOrder",
      "$.partialSubmit.response.data.fulfillmentOrderSubmitFulfillmentRequest.unsubmittedFulfillmentOrder",
      "$.rejectFulfillmentRequest.response.data.fulfillmentOrderRejectFulfillmentRequest.fulfillmentOrder",
      "$.rejectCancellationRequest.response.data.fulfillmentOrderRejectCancellationRequest.fulfillmentOrder",
    ]
    |> list.filter_map(fn(path) {
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(make_seed_fulfillment_order(value, None))
        None -> Error(Nil)
      }
    })
  let order_records = case fulfillment_orders {
    [] -> []
    _ -> [
      order_record_from_fulfillment_orders(
        "gid://shopify/Order/shipping-fulfillment-order-request-seed",
        fulfillment_orders,
      ),
    ]
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_orders(order_records)
    |> store_mod.upsert_base_fulfillment_orders(fulfillment_orders)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_fulfillment_order_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let create_paths = [
    "$.workflows.holdRelease.create.response.payload.data.orderCreate.order",
    "$.workflows.move.create.response.payload.data.orderCreate.order",
    "$.workflows.scheduleProgressOpenClose.create.response.payload.data.orderCreate.order",
    "$.workflows.reroute.create.response.payload.data.orderCreate.order",
    "$.workflows.residualSplitDeadlineMerge.create.response.payload.data.orderCreate.order",
  ]
  let order_sources =
    create_paths
    |> list.filter_map(fn(path) {
      case jsonpath.lookup(capture, path) {
        Some(order) -> Ok(order)
        None -> Error(Nil)
      }
    })
  let shipping_orders = list.map(order_sources, make_seed_shipping_order)
  let order_records = list.flat_map(order_sources, make_seed_order_list)
  let fulfillment_orders =
    order_sources
    |> list.flat_map(fn(path) {
      let order_id = read_string_field(path, "id")
      case read_object_field(path, "fulfillmentOrders") {
        Some(connection) ->
          case read_array_field(connection, "nodes") {
            Some(nodes) ->
              list.map(nodes, fn(node) {
                make_seed_fulfillment_order(node, order_id)
              })
            None -> []
          }
        None -> []
      }
    })
  let store =
    proxy.store
    |> store_mod.upsert_base_orders(order_records)
    |> store_mod.upsert_base_shipping_orders(shipping_orders)
    |> store_mod.upsert_base_fulfillment_orders(fulfillment_orders)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn make_seed_order_list(source: JsonValue) -> List(OrderRecord) {
  case make_seed_order(source) {
    Ok(record) -> [record]
    Error(_) -> []
  }
}

fn order_record_from_fulfillment_orders(
  id: String,
  fulfillment_orders: List(FulfillmentOrderRecord),
) -> OrderRecord {
  OrderRecord(
    id: id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("fulfillments", CapturedArray([])),
      #(
        "fulfillmentOrders",
        CapturedArray(list.map(fulfillment_orders, fn(record) { record.data })),
      ),
    ]),
  )
}

fn make_seed_shipping_order(source: JsonValue) -> ShippingOrderRecord {
  ShippingOrderRecord(
    id: read_string_field(source, "id") |> option.unwrap(""),
    data: captured_json_from_parity(source),
  )
}

fn make_seed_fulfillment(
  source: JsonValue,
  order_id: Option(String),
) -> FulfillmentRecord {
  FulfillmentRecord(
    id: read_string_field(source, "id") |> option.unwrap(""),
    order_id: order_id,
    data: captured_json_from_parity(source),
  )
}

fn make_seed_fulfillment_order(
  source: JsonValue,
  order_id: Option(String),
) -> FulfillmentOrderRecord {
  let request_status =
    read_string_field(source, "requestStatus") |> option.unwrap("UNSUBMITTED")
  FulfillmentOrderRecord(
    id: read_string_field(source, "id") |> option.unwrap(""),
    order_id: order_id,
    status: read_string_field(source, "status") |> option.unwrap("OPEN"),
    request_status: request_status,
    assigned_location_id: read_assigned_location_id(source),
    assignment_status: infer_assignment_status(source, request_status),
    manually_held: read_bool_field(source, "manuallyHeld")
      |> option.unwrap(False),
    data: captured_json_from_parity(source),
  )
}

fn read_assigned_location_id(source: JsonValue) -> Option(String) {
  read_object_field(source, "assignedLocation")
  |> option.then(fn(assigned_location) {
    read_object_field(assigned_location, "location")
    |> option.then(fn(location) { read_string_field(location, "id") })
  })
}

fn infer_assignment_status(
  source: JsonValue,
  request_status: String,
) -> Option(String) {
  case fulfillment_order_source_has_cancellation_request(source) {
    True -> Some("CANCELLATION_REQUESTED")
    False ->
      case request_status {
        "SUBMITTED" -> Some("FULFILLMENT_REQUESTED")
        "ACCEPTED" -> Some("FULFILLMENT_ACCEPTED")
        _ -> None
      }
  }
}

fn fulfillment_order_source_has_cancellation_request(
  source: JsonValue,
) -> Bool {
  case read_object_field(source, "merchantRequests") {
    Some(requests) ->
      case read_array_field(requests, "nodes") {
        Some(nodes) ->
          list.any(nodes, fn(node) {
            read_string_field(node, "kind") == Some("CANCELLATION_REQUEST")
          })
        None -> False
      }
    None -> False
  }
}

fn merge_fulfillments(
  records: List(FulfillmentRecord),
) -> List(FulfillmentRecord) {
  let initial: List(FulfillmentRecord) = []
  records
  |> list.fold(initial, fn(seen, record) {
    case list.any(seen, fn(existing) { existing.id == record.id }) {
      True -> seen
      False -> list.append(seen, [record])
    }
  })
}

fn merge_fulfillment_orders(
  records: List(FulfillmentOrderRecord),
) -> List(FulfillmentOrderRecord) {
  let initial: List(FulfillmentOrderRecord) = []
  records
  |> list.fold(initial, fn(seen, record) {
    case list.any(seen, fn(existing) { existing.id == record.id }) {
      True -> seen
      False -> list.append(seen, [record])
    }
  })
}

fn make_seed_delivery_profile(
  source: JsonValue,
  capture: JsonValue,
) -> Result(DeliveryProfileRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(DeliveryProfileRecord(
    id: id,
    cursor: find_delivery_profile_cursor(capture, id),
    merchant_owned: read_bool_field(source, "merchantOwned")
      |> option.unwrap(True),
    data: captured_json_from_parity(source),
  ))
}

fn default_delivery_profile_seed(id: String) -> DeliveryProfileRecord {
  DeliveryProfileRecord(
    id: id,
    cursor: None,
    merchant_owned: True,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("name", CapturedString("General profile")),
      #("default", CapturedBool(True)),
      #("merchantOwned", CapturedBool(True)),
      #("version", CapturedInt(1)),
      #("activeMethodDefinitionsCount", CapturedInt(0)),
      #("locationsWithoutRatesCount", CapturedInt(0)),
      #("originLocationCount", CapturedInt(0)),
      #("zoneCountryCount", CapturedInt(0)),
      #(
        "productVariantsCount",
        CapturedObject([
          #("count", CapturedInt(0)),
          #("precision", CapturedString("EXACT")),
        ]),
      ),
      #("profileItems", captured_empty_connection()),
      #("profileLocationGroups", CapturedArray([])),
    ]),
  )
}

fn captured_empty_connection() -> CapturedJsonValue {
  CapturedObject([
    #("nodes", CapturedArray([])),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", CapturedNull),
        #("endCursor", CapturedNull),
      ]),
    ),
  ])
}

fn find_delivery_profile_cursor(
  capture: JsonValue,
  profile_id: String,
) -> Option(String) {
  case
    jsonpath.lookup(
      capture,
      "$.queries.catalogFirst.result.payload.data.deliveryProfiles.edges",
    )
  {
    Some(JArray(edges)) ->
      edges
      |> list.find_map(fn(edge) {
        case read_object_field(edge, "node") {
          Some(node) ->
            case required_string_field(node, "id") {
              Ok(id) if id == profile_id ->
                read_string_field(edge, "cursor") |> option.to_result(Nil)
              _ -> Error(Nil)
            }
          None -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn make_seed_carrier_service(
  source: JsonValue,
) -> Result(CarrierServiceRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(CarrierServiceRecord(
    id: id,
    name: read_string_field(source, "name"),
    formatted_name: read_string_field(source, "formattedName"),
    callback_url: read_string_field(source, "callbackUrl"),
    active: read_bool_field(source, "active") |> option.unwrap(False),
    supports_service_discovery: read_bool_field(
      source,
      "supportsServiceDiscovery",
    )
      |> option.unwrap(False),
    created_at: read_string_field(source, "createdAt")
      |> option.unwrap("2024-01-01T00:00:00.000Z"),
    updated_at: read_string_field(source, "updatedAt")
      |> option.unwrap("2024-01-01T00:00:00.000Z"),
  ))
}

fn make_seed_shipping_package(
  source: JsonValue,
) -> Result(ShippingPackageRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use created_at <- result.try(required_string_field(source, "createdAt"))
  use updated_at <- result.try(required_string_field(source, "updatedAt"))
  Ok(ShippingPackageRecord(
    id: id,
    name: read_string_field(source, "name"),
    type_: read_string_field(source, "type"),
    default: read_bool_field(source, "default") |> option.unwrap(False),
    weight: read_shipping_package_weight(read_object_field(source, "weight")),
    dimensions: read_shipping_package_dimensions(read_object_field(
      source,
      "dimensions",
    )),
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn read_shipping_package_weight(
  source: Option(JsonValue),
) -> Option(ShippingPackageWeightRecord) {
  case source {
    Some(value) ->
      Some(ShippingPackageWeightRecord(
        value: read_float_field(value, "value"),
        unit: read_string_field(value, "unit"),
      ))
    None -> None
  }
}

fn read_shipping_package_dimensions(
  source: Option(JsonValue),
) -> Option(ShippingPackageDimensionsRecord) {
  case source {
    Some(value) ->
      Some(ShippingPackageDimensionsRecord(
        length: read_float_field(value, "length"),
        width: read_float_field(value, "width"),
        height: read_float_field(value, "height"),
        unit: read_string_field(value, "unit"),
      ))
    None -> None
  }
}

fn seed_gift_card_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    [
      jsonpath.lookup(
        capture,
        "$.operations.create.response.payload.data.giftCardCreate.giftCard",
      ),
      jsonpath.lookup(
        capture,
        "$.create.response.payload.data.giftCardCreate.giftCard",
      ),
    ]
    |> list.filter_map(fn(candidate) {
      case candidate {
        Some(value) -> make_seed_gift_card(value, Some("api_client"))
        None -> Error(Nil)
      }
    })

  let empty_read_records = case
    jsonpath.lookup(
      capture,
      "$.operations.emptyRead.response.payload.data.giftCards.nodes",
    )
  {
    Some(JArray(nodes)) ->
      list.filter_map(nodes, fn(node) { make_seed_gift_card(node, None) })
    _ -> []
  }

  let records = list.append(records, empty_read_records)
  let seeded_store = case records {
    [] -> proxy.store
    _ -> store_mod.upsert_base_gift_cards(proxy.store, records)
  }
  let seeded_store = case seed_gift_card_configuration(capture) {
    Some(configuration) ->
      store_mod.upsert_base_gift_card_configuration(seeded_store, configuration)
    None -> seeded_store
  }
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn make_seed_gift_card(
  source: JsonValue,
  source_override: Option(String),
) -> Result(GiftCardRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/GiftCard/") {
    False -> Error(Nil)
    True -> {
      let last_characters =
        read_string_field(source, "lastCharacters")
        |> option.unwrap(gift_card_tail(id))
      let initial_value =
        read_money_record(read_object_field(source, "initialValue"))
      let balance =
        read_money_record(
          read_object_field(source, "balance")
          |> option.or(read_object_field(source, "initialValue")),
        )
      let recipient_attributes_source =
        read_object_field(source, "recipientAttributes")
      let recipient_source =
        recipient_attributes_source
        |> option.then(read_object_field(_, "recipient"))
      let recipient_id =
        read_string_field_from_option(recipient_source, "id")
        |> option.or(read_string_field_from_option(
          read_object_field(source, "recipient"),
          "id",
        ))
      let transactions =
        read_transactions(read_object_field(source, "transactions"))
      Ok(GiftCardRecord(
        id: id,
        legacy_resource_id: read_string_field(source, "legacyResourceId")
          |> option.unwrap(gift_card_tail(id)),
        last_characters: last_characters,
        masked_code: read_string_field(source, "maskedCode")
          |> option.unwrap(masked_code(last_characters)),
        enabled: read_bool_field(source, "enabled") |> option.unwrap(True),
        deactivated_at: read_string_field(source, "deactivatedAt"),
        expires_on: read_string_field(source, "expiresOn"),
        note: read_string_field(source, "note"),
        template_suffix: read_string_field(source, "templateSuffix"),
        created_at: read_string_field(source, "createdAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        updated_at: read_string_field(source, "updatedAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        initial_value: initial_value,
        balance: balance,
        customer_id: read_string_field_from_option(
          read_object_field(source, "customer"),
          "id",
        ),
        recipient_id: recipient_id,
        source: case source_override {
          Some(_) -> source_override
          None -> read_string_field(source, "source")
        },
        recipient_attributes: make_seed_recipient_attributes(
          recipient_attributes_source,
          recipient_id,
        ),
        transactions: transactions,
      ))
    }
  }
}

fn seed_gift_card_configuration(
  capture: JsonValue,
) -> Option(GiftCardConfigurationRecord) {
  let primary =
    jsonpath.lookup(
      capture,
      "$.operations.configurationRead.response.payload.data.giftCardConfiguration",
    )
  let fallback =
    jsonpath.lookup(
      capture,
      "$.configurationRead.response.payload.data.giftCardConfiguration",
    )
  case primary |> option.or(fallback) {
    Some(value) ->
      Some(GiftCardConfigurationRecord(
        issue_limit: read_money_record(read_object_field(value, "issueLimit")),
        purchase_limit: read_money_record(read_object_field(
          value,
          "purchaseLimit",
        )),
      ))
    None -> None
  }
}

fn make_seed_recipient_attributes(
  source: Option(JsonValue),
  recipient_id: Option(String),
) -> Option(GiftCardRecipientAttributesRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(GiftCardRecipientAttributesRecord(
        id: recipient_id,
        message: read_string_field(value, "message"),
        preferred_name: read_string_field(value, "preferredName"),
        send_notification_at: read_string_field(value, "sendNotificationAt"),
      ))
  }
}

fn read_transactions(
  source: Option(JsonValue),
) -> List(GiftCardTransactionRecord) {
  case source |> option.then(read_array_field(_, "nodes")) {
    Some(nodes) ->
      list.filter_map(nodes, fn(node) {
        let amount = read_money_record(read_object_field(node, "amount"))
        Ok(GiftCardTransactionRecord(
          id: read_string_field(node, "id")
            |> option.unwrap("gid://shopify/GiftCardTransaction/0"),
          kind: case string.starts_with(amount.amount, "-") {
            True -> "DEBIT"
            False -> "CREDIT"
          },
          amount: amount,
          processed_at: read_string_field(node, "processedAt")
            |> option.unwrap("2026-01-01T00:00:00Z"),
          note: read_string_field(node, "note"),
        ))
      })
    None -> []
  }
}

fn read_money_record(source: Option(JsonValue)) -> Money {
  case source {
    Some(value) ->
      Money(
        amount: read_string_field(value, "amount") |> option.unwrap("0.0"),
        currency_code: read_string_field(value, "currencyCode")
          |> option.unwrap("CAD"),
      )
    None -> Money(amount: "0.0", currency_code: "CAD")
  }
}

fn required_string_field(
  value: JsonValue,
  name: String,
) -> Result(String, Nil) {
  case read_string_field(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn required_object_field(
  value: JsonValue,
  name: String,
) -> Result(JsonValue, Nil) {
  case read_object_field(value, name) {
    Some(object) -> Ok(object)
    None -> Error(Nil)
  }
}

fn required_int_field(value: JsonValue, name: String) -> Result(Int, Nil) {
  case read_int_field(value, name) {
    Some(i) -> Ok(i)
    None -> Error(Nil)
  }
}

fn jsonpath_string(value: JsonValue, path: String) -> Option(String) {
  case jsonpath.lookup(value, path) {
    Some(JString(value)) -> Some(value)
    _ -> None
  }
}

/// True if any of the given JSONPaths resolves to a non-null value in the
/// capture. Used by `seed_*_preconditions` helpers to self-gate on
/// capture shape rather than scenario id.
fn capture_has_any_path(value: JsonValue, paths: List(String)) -> Bool {
  list.any(paths, fn(path) {
    case jsonpath.lookup(value, path) {
      Some(JNull) -> False
      Some(_) -> True
      None -> False
    }
  })
}

fn jsonpath_string_array(value: JsonValue, path: String) -> List(String) {
  case jsonpath.lookup(value, path) {
    Some(JArray(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          JString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn json_string_option(value: Option(JsonValue)) -> Option(String) {
  case value {
    Some(JString(value)) -> Some(value)
    _ -> None
  }
}

fn read_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    _ -> None
  }
}

fn read_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(String) {
  case value {
    Some(v) -> read_string_field(v, name)
    None -> None
  }
}

fn read_bool_field(value: JsonValue, name: String) -> Option(Bool) {
  case json_value.field(value, name) {
    Some(JBool(b)) -> Some(b)
    _ -> None
  }
}

fn read_bool_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(Bool) {
  case value {
    Some(v) -> read_bool_field(v, name)
    None -> None
  }
}

fn read_int_field(value: JsonValue, name: String) -> Option(Int) {
  case json_value.field(value, name) {
    Some(JInt(i)) -> Some(i)
    _ -> None
  }
}

fn read_float_field(value: JsonValue, name: String) -> Option(Float) {
  case json_value.field(value, name) {
    Some(JFloat(f)) -> Some(f)
    Some(JInt(i)) -> Some(int.to_float(i))
    _ -> None
  }
}

fn read_string_array_field(value: JsonValue, name: String) -> List(String) {
  case read_array_field(value, name) {
    Some(items) ->
      list.filter_map(items, fn(item) {
        case item {
          JString(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    None -> []
  }
}

fn read_object_field(value: JsonValue, name: String) -> Option(JsonValue) {
  case json_value.field(value, name) {
    Some(JObject(_)) as object -> object
    _ -> None
  }
}

fn read_captured_json_field(
  value: JsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case json_value.field(value, name) {
    Some(value) -> Some(captured_json_from_parity(value))
    None -> None
  }
}

fn captured_json_from_parity(value: JsonValue) -> CapturedJsonValue {
  case value {
    JNull -> CapturedNull
    JBool(value) -> CapturedBool(value)
    JInt(value) -> CapturedInt(value)
    JFloat(value) -> CapturedFloat(value)
    JString(value) -> CapturedString(value)
    JArray(items) -> CapturedArray(list.map(items, captured_json_from_parity))
    JObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_from_parity(item))
        }),
      )
  }
}

fn collect_seed_market_records(value: JsonValue) -> List(MarketRecord) {
  collect_captured_resources(value, None)
  |> list.filter_map(fn(entry) {
    let #(node, cursor) = entry
    case read_string_field(node, "id") {
      Some(id) ->
        case string.starts_with(id, "gid://shopify/Market/") {
          True ->
            Ok(MarketRecord(
              id: id,
              cursor: cursor,
              data: captured_json_from_parity(node),
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn collect_seed_catalog_records(value: JsonValue) -> List(CatalogRecord) {
  collect_captured_resources(value, None)
  |> list.filter_map(fn(entry) {
    let #(node, cursor) = entry
    case read_string_field(node, "id") {
      Some(id) ->
        case is_catalog_id(id) {
          True ->
            Ok(CatalogRecord(
              id: id,
              cursor: cursor,
              data: captured_json_from_parity(node),
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn collect_seed_price_list_records(value: JsonValue) -> List(PriceListRecord) {
  collect_captured_resources(value, None)
  |> list.filter_map(fn(entry) {
    let #(node, cursor) = entry
    case read_string_field(node, "id") {
      Some(id) ->
        case string.starts_with(id, "gid://shopify/PriceList/") {
          True ->
            Ok(PriceListRecord(
              id: id,
              cursor: cursor,
              data: captured_json_from_parity(node),
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn collect_seed_web_presence_records(
  value: JsonValue,
) -> List(WebPresenceRecord) {
  collect_captured_resources(value, None)
  |> list.filter_map(fn(entry) {
    let #(node, cursor) = entry
    case read_string_field(node, "id") {
      Some(id) ->
        case string.starts_with(id, "gid://shopify/MarketWebPresence/") {
          True ->
            Ok(WebPresenceRecord(
              id: id,
              cursor: cursor,
              data: captured_json_from_parity(node),
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn collect_captured_resources(
  value: JsonValue,
  edge_cursor: Option(String),
) -> List(#(JsonValue, Option(String))) {
  let self = case value {
    JObject(_) ->
      case read_string_field(value, "id") {
        Some(_) -> [#(value, edge_cursor)]
        None -> []
      }
    _ -> []
  }
  let children = case value {
    JObject(fields) -> {
      let cursor = read_string_field(value, "cursor") |> option.or(edge_cursor)
      let node_entries = case json_value.field(value, "node") {
        Some(node) -> collect_captured_resources(node, cursor)
        None -> []
      }
      let field_entries =
        fields
        |> list.flat_map(fn(pair) {
          let #(key, child) = pair
          case key {
            "node" -> []
            _ -> collect_captured_resources(child, edge_cursor)
          }
        })
      list.append(node_entries, field_entries)
    }
    JArray(items) ->
      list.flat_map(items, fn(item) {
        collect_captured_resources(item, edge_cursor)
      })
    _ -> []
  }
  list.append(self, children)
}

fn is_catalog_id(id: String) -> Bool {
  string.starts_with(id, "gid://shopify/MarketCatalog/")
  || string.starts_with(id, "gid://shopify/CompanyLocationCatalog/")
  || string.starts_with(id, "gid://shopify/AppCatalog/")
  || string.starts_with(id, "gid://shopify/Catalog/")
}

fn seed_markets_root_payload(
  store: store_mod.Store,
  capture: JsonValue,
  key: String,
  paths: List(String),
) -> store_mod.Store {
  case first_jsonpath_match(capture, paths) {
    Some(payload) ->
      store_mod.upsert_base_markets_root_payload(
        store,
        key,
        captured_json_from_parity(payload),
      )
    None -> store
  }
}

fn first_jsonpath_match(
  value: JsonValue,
  paths: List(String),
) -> Option(JsonValue) {
  list.find_map(paths, fn(path) {
    jsonpath.lookup(value, path) |> option.to_result(Nil)
  })
  |> option.from_result
}

fn read_array_field(value: JsonValue, name: String) -> Option(List(JsonValue)) {
  case json_value.field(value, name) {
    Some(JArray(items)) -> Some(items)
    _ -> None
  }
}

fn json_string_or(value: Option(JsonValue), fallback: String) -> String {
  case value {
    Some(JString(value)) -> value
    _ -> fallback
  }
}

fn json_int_or(value: Option(JsonValue), fallback: Int) -> Int {
  case value {
    Some(JInt(value)) -> value
    _ -> fallback
  }
}

fn json_bool_or(value: Option(JsonValue), fallback: Bool) -> Bool {
  case value {
    Some(JBool(value)) -> value
    _ -> fallback
  }
}

fn enumerate_json_values(items: List(JsonValue)) -> List(#(JsonValue, Int)) {
  enumerate_json_values_loop(items, 0, [])
}

fn enumerate_strings(items: List(String)) -> List(#(String, Int)) {
  enumerate_strings_loop(items, 0, [])
}

fn enumerate_strings_loop(
  items: List(String),
  index: Int,
  acc: List(#(String, Int)),
) -> List(#(String, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_strings_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

fn enumerate_json_values_loop(
  items: List(JsonValue),
  index: Int,
  acc: List(#(JsonValue, Int)),
) -> List(#(JsonValue, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_json_values_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

fn json_object_to_runtime_dict(
  value: Option(JsonValue),
) -> dict.Dict(String, json.Json) {
  case value {
    Some(JObject(entries)) ->
      entries
      |> list.map(fn(pair) {
        let #(key, item) = pair
        #(key, runtime_json_from_json_value(item))
      })
      |> dict.from_list
    _ -> dict.new()
  }
}

fn runtime_json_from_json_value(value: JsonValue) -> json.Json {
  case value {
    JNull -> json.null()
    JBool(b) -> json.bool(b)
    JInt(i) -> json.int(i)
    JFloat(f) -> json.float(f)
    JString(s) -> json.string(s)
    JArray(items) -> json.array(items, runtime_json_from_json_value)
    JObject(entries) ->
      json.object(
        list.map(entries, fn(pair) {
          let #(key, item) = pair
          #(key, runtime_json_from_json_value(item))
        }),
      )
  }
}

fn gift_card_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, on: "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

fn generic_gid_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, on: "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

fn masked_code(last_characters: String) -> String {
  "•••• •••• •••• " <> last_characters
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, List(TargetReport)), RunError) {
  list.try_fold(
    parsed.targets,
    #(proxy, [], None, dict.new()),
    fn(state, target) {
      let #(current_proxy, acc_reports, previous_response, named_responses) =
        state
      use #(next_proxy, report) <- result.try(run_target(
        config,
        parsed,
        target,
        capture,
        primary_response,
        previous_response,
        named_responses,
        current_proxy,
      ))
      Ok(#(
        next_proxy,
        [report.0, ..acc_reports],
        Some(report.1),
        dict.insert(named_responses, target.name, report.1),
      ))
    },
  )
  |> result.map(fn(state) {
    let #(final_proxy, reports, _, _) = state
    #(final_proxy, list.reverse(reports))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(#(DraftProxy, #(TargetReport, JsonValue)), RunError) {
  use #(actual_response, next_proxy) <- result.try(actual_response_for(
    config,
    parsed,
    target,
    capture,
    primary_response,
    previous_response,
    named_responses,
    proxy,
  ))
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, None ->
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: [],
          ),
          actual_response,
        ),
      ))
    None, _ ->
      Error(CaptureUnresolved(target: target.name, path: target.capture_path))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = case target.selected_paths {
        [] -> diff.diff_with_expected(expected, actual, rules)
        selected_paths ->
          diff.diff_selected_paths(expected, actual, selected_paths, rules)
      }
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: mismatches,
          ),
          actual_response,
        ),
      ))
    }
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.request {
    ReusePrimary -> proxy_source_value(target, primary_response, proxy)
    OverrideRequest(request: request) -> {
      case
        target.upstream_capture_path,
        override_request_uses_upstream_capture(parsed.scenario_id)
      {
        Some(path), True ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(#(value, proxy))
            None -> Error(CaptureUnresolved(target: target.name, path: path))
          }
        _, _ -> {
          use document <- result.try(
            read_file(resolve(config, request.document_path)),
          )
          use variables <- result.try(resolve_variables(
            config,
            request.variables,
            capture,
            Some(primary_response),
            previous_response,
            named_responses,
            target.name,
          ))
          use #(response, next_proxy) <- result.try(execute(
            proxy,
            document,
            variables,
            target.name,
            request.api_version,
          ))
          use value <- result.try(parse_response_body(response))
          proxy_source_value(target, value, next_proxy)
        }
      }
    }
  }
}

fn proxy_source_value(
  target: Target,
  response_value: JsonValue,
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.proxy_source {
    ProxyResponse -> Ok(#(response_value, proxy))
    ProxyState -> {
      use state_value <- result.try(meta_response_value(proxy, "/__meta/state"))
      Ok(#(state_value, proxy))
    }
    ProxyLog -> {
      use log_value <- result.try(meta_response_value(proxy, "/__meta/log"))
      Ok(#(log_value, proxy))
    }
  }
}

fn meta_response_value(
  proxy: DraftProxy,
  path: String,
) -> Result(JsonValue, RunError) {
  let #(response, _) =
    draft_proxy.process_request(
      proxy,
      Request(method: "GET", path: path, headers: dict.new(), body: ""),
    )
  parse_response_body(response)
}

fn override_request_uses_upstream_capture(scenario_id: String) -> Bool {
  case scenario_id {
    "storefront-access-token-local-staging" -> False
    _ -> True
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn load_capture(
  config: RunnerConfig,
  parsed: Spec,
) -> Result(JsonValue, RunError) {
  let path = resolve(config, parsed.capture_file)
  use source <- result.try(read_file(path))
  parse_json(path, source)
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) ->
          substitute(
            value,
            primary_response,
            previous_response,
            named_responses,
            capture,
          )
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      parse_json(resolved, source)
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(
        template,
        primary_response,
        previous_response,
        named_responses,
        capture,
      )
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_primary_ref(template) {
    Some(path) ->
      case primary {
        None -> Error(PrimaryRefUnresolved(path: path))
        Some(root) ->
          case jsonpath.lookup(root, path) {
            Some(value) -> Ok(value)
            None -> Error(PrimaryRefUnresolved(path: path))
          }
      }
    None ->
      case as_previous_ref(template) {
        Some(path) ->
          case previous {
            None -> Error(PreviousRefUnresolved(path: path))
            Some(root) ->
              case jsonpath.lookup(root, path) {
                Some(value) -> Ok(value)
                None -> Error(PreviousRefUnresolved(path: path))
              }
          }
        None ->
          case as_named_response_ref(template) {
            Some(ref) -> {
              let #(target, path) = ref
              case dict.get(named, target) {
                Ok(root) ->
                  case jsonpath.lookup(root, path) {
                    Some(value) -> Ok(value)
                    None -> Error(ProxyResponseRefUnresolved(target, path))
                  }
                Error(_) -> Error(ProxyResponseRefUnresolved(target, path))
              }
            }
            None ->
              substitute_capture_or_children(
                template,
                primary,
                previous,
                named,
                capture,
              )
          }
      }
  }
}

fn substitute_capture_or_children(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_capture_ref(template) {
    Some(path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(CaptureRefUnresolved(path: path))
      }
    None ->
      case template {
        JObject(entries) ->
          entries
          |> list.try_map(fn(pair) {
            let #(k, v) = pair
            case substitute(v, primary, previous, named, capture) {
              Ok(v2) -> Ok(#(k, v2))
              Error(e) -> Error(e)
            }
          })
          |> result.map(JObject)
        JArray(items) ->
          items
          |> list.try_map(fn(item) {
            substitute(item, primary, previous, named, capture)
          })
          |> result.map(JArray)
        leaf -> Ok(leaf)
      }
  }
}

/// If `value` is exactly `{"fromPreviousProxyPath": "..."}` (one
/// entry with a string value), return the path. Otherwise None.
fn as_previous_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPreviousProxyPath", json_value.JString(path))]) ->
      Some(path)
    _ -> None
  }
}

/// If `value` is exactly an object containing `fromProxyResponse` and
/// `path` string entries, return target/path regardless of field order.
fn as_named_response_ref(value: JsonValue) -> Option(#(String, String)) {
  case value {
    JObject(entries) -> {
      let target = object_string_entry(entries, "fromProxyResponse")
      let path = object_string_entry(entries, "path")
      case target, path {
        Some(target), Some(path) -> Some(#(target, path))
        _, _ -> None
      }
    }
    _ -> None
  }
}

/// If `value` is exactly `{"fromPrimaryProxyPath": "..."}` (one entry
/// with a string value), return the path. Otherwise None.
fn as_primary_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPrimaryProxyPath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn object_string_entry(
  entries: List(#(String, JsonValue)),
  name: String,
) -> Option(String) {
  case entries {
    [] -> None
    [#(key, json_value.JString(value)), ..] if key == name -> Some(value)
    [_, ..rest] -> object_string_entry(rest, name)
  }
}

/// If `value` is exactly `{"fromCapturePath": "..."}` (one entry with
/// a string value), return the path. Otherwise None.
fn as_capture_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromCapturePath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn execute(
  proxy: DraftProxy,
  document: String,
  variables: JsonValue,
  context: String,
  api_version: Option(String),
) -> Result(#(Response, DraftProxy), RunError) {
  let body = build_graphql_body(document, variables)
  let version = option.unwrap(api_version, "2025-01")
  let request =
    Request(
      method: "POST",
      path: "/admin/api/" <> version <> "/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case response.status {
    200 -> Ok(#(response, next_proxy))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn build_graphql_body(document: String, variables: JsonValue) -> String {
  let query = json.to_string(json.string(document))
  let vars = json_value.to_string(variables)
  "{\"query\":" <> query <> ",\"variables\":" <> vars <> "}"
}

fn parse_response_body(response: Response) -> Result(JsonValue, RunError) {
  let serialized = json.to_string(response.body)
  parse_json("<proxy-response>", serialized)
}

fn read_file(path: String) -> Result(String, RunError) {
  case simplifile.read(path) {
    Ok(s) -> Ok(s)
    Error(reason) ->
      Error(FileError(path: path, reason: simplifile.describe_error(reason)))
  }
}

fn parse_json(path: String, source: String) -> Result(JsonValue, RunError) {
  case json_value.parse(source) {
    Ok(v) -> Ok(v)
    Error(e) -> Error(JsonError(path: path, reason: e.message))
  }
}

fn resolve(config: RunnerConfig, path: String) -> String {
  case string.starts_with(path, "/") {
    True -> path
    False -> config.repo_root <> "/" <> path
  }
}

pub fn has_mismatches(report: Report) -> Bool {
  list.any(report.targets, fn(t) { t.mismatches != [] })
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True ->
      report.scenario_id
      <> "\n"
      <> string.join(list.map(report.targets, render_target), "\n")
  }
}

fn render_target(target: TargetReport) -> String {
  case target.mismatches {
    [] -> "  [" <> target.name <> "] OK"
    mismatches ->
      "  ["
      <> target.name
      <> "] "
      <> int.to_string(list.length(mismatches))
      <> " mismatch(es):\n"
      <> diff.render_mismatches(mismatches)
  }
}

pub fn into_assert(report: Report) -> Result(Nil, String) {
  case has_mismatches(report) {
    False -> Ok(Nil)
    True -> Error(render(report))
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    VariablesUnresolved(path) -> "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    PreviousRefUnresolved(path) ->
      "fromPreviousProxyPath did not resolve in previous proxy response: "
      <> path
    ProxyResponseRefUnresolved(target, path) ->
      "fromProxyResponse did not resolve for target '"
      <> target
      <> "' at "
      <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '" <> target <> "': " <> path
    ProxyUnresolved(target, path) ->
      "proxy response jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyStatus(target, status, body) ->
      "proxy returned status "
      <> int.to_string(status)
      <> " for target '"
      <> target
      <> "': "
      <> body
  }
}

import gleam/dict
import gleam/dynamic/decode
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type ShopRecord, PaymentSettingsRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductSeoRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopRecord, ShopResourceLimitsRecord,
}
import simplifile

const admin_platform_fixture_path: String = "fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json"

fn empty_vars() {
  dict.new()
}

/// Apply the dispatcher-level `record_log_drafts` to the outcome.
/// Tests that exercise `admin_platform.process_mutation` directly (no
/// `draft_proxy` round-trip) need this so log-buffer assertions still
/// see the drafts the module emitted; centralized recording is the
/// dispatcher's responsibility post-refactor.
fn record_drafts(
  outcome: mutation_helpers.MutationOutcome,
  request_path: String,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      dict.new(),
      outcome.log_drafts,
    )
  mutation_helpers.MutationOutcome(
    ..outcome,
    store: logged_store,
    identity: logged_identity,
  )
}

fn run_query(source: store.Store, query: String) -> String {
  let assert Ok(body) = admin_platform.process(source, query, empty_vars())
  json.to_string(body)
}

pub fn root_predicates_test() {
  assert admin_platform.is_admin_platform_query_root("publicApiVersions")
  assert admin_platform.is_admin_platform_query_root("node")
  assert admin_platform.is_admin_platform_query_root("nodes")
  assert admin_platform.is_admin_platform_query_root("job")
  assert admin_platform.is_admin_platform_query_root("domain")
  assert admin_platform.is_admin_platform_query_root("backupRegion")
  assert admin_platform.is_admin_platform_query_root("taxonomy")
  assert admin_platform.is_admin_platform_query_root("staffMember")
  assert admin_platform.is_admin_platform_query_root("staffMembers")
  assert admin_platform.is_admin_platform_mutation_root("flowGenerateSignature")
  assert admin_platform.is_admin_platform_mutation_root("flowTriggerReceive")
  assert admin_platform.is_admin_platform_mutation_root("backupRegionUpdate")
  assert !admin_platform.is_admin_platform_query_root("products")
}

pub fn utility_reads_return_local_no_data_shapes_test() {
  let body =
    run_query(
      store.new(),
      "query { publicApiVersions { handle displayName supported } node(id: \"gid://shopify/Product/0\") { id } nodes(ids: [\"gid://shopify/Product/0\", \"gid://shopify/Customer/0\"]) { id } job(id: \"gid://shopify/Job/0\") { __typename id done query { __typename } } domain(id: \"gid://shopify/Domain/0\") { id } backupRegion { __typename id name code } taxonomy { categories(first: 1) { nodes { id } edges { node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }",
    )

  assert string.contains(
    body,
    "\"publicApiVersions\":[{\"handle\":\"2025-07\",\"displayName\":\"2025-07\",\"supported\":true}",
  )
  assert string.contains(body, "\"node\":null")
  assert string.contains(body, "\"nodes\":[null,null]")
  assert string.contains(
    body,
    "\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/0\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}",
  )
  assert string.contains(body, "\"domain\":null")
  assert string.contains(
    body,
    "\"backupRegion\":{\"__typename\":\"MarketRegionCountry\",\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(
    body,
    "\"categories\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}",
  )
}

pub fn product_option_node_reads_resolve_from_product_state_test() {
  let body =
    run_query(
      seeded_product_option_store(),
      "query {
        nodes(ids: [
          \"gid://shopify/ProductOption/color\",
          \"gid://shopify/ProductOptionValue/blue\",
          \"gid://shopify/ProductOption/missing\"
        ]) {
          __typename
          id
          ... on ProductOption {
            name
            position
            values
            optionValues { id name hasVariants }
          }
          ... on ProductOptionValue {
            name
            hasVariants
          }
        }
      }",
    )

  assert body
    == "{\"data\":{\"nodes\":[{\"__typename\":\"ProductOption\",\"id\":\"gid://shopify/ProductOption/color\",\"name\":\"Color\",\"position\":1,\"values\":[\"Red\"],\"optionValues\":[{\"id\":\"gid://shopify/ProductOptionValue/red\",\"name\":\"Red\",\"hasVariants\":true},{\"id\":\"gid://shopify/ProductOptionValue/blue\",\"name\":\"Blue\",\"hasVariants\":false}]},{\"__typename\":\"ProductOptionValue\",\"id\":\"gid://shopify/ProductOptionValue/blue\",\"name\":\"Blue\",\"hasVariants\":false},null]}}"
}

pub fn domain_node_reads_resolve_from_primary_shop_domain_test() {
  let source = store.new() |> store.upsert_base_shop(make_shop())
  let body =
    run_query(
      source,
      "query {
        node(id: \"gid://shopify/Domain/93049946345\") {
          __typename
          id
          ... on Domain {
            host
            url
            sslEnabled
          }
        }
        nodes(ids: [
          \"gid://shopify/Domain/93049946345\",
          \"gid://shopify/Domain/missing\"
        ]) {
          __typename
          id
          ... on Domain {
            host
          }
        }
      }",
    )

  assert body
    == "{\"data\":{\"node\":{\"__typename\":\"Domain\",\"id\":\"gid://shopify/Domain/93049946345\",\"host\":\"very-big-test-store.myshopify.com\",\"url\":\"https://very-big-test-store.myshopify.com\",\"sslEnabled\":true},\"nodes\":[{\"__typename\":\"Domain\",\"id\":\"gid://shopify/Domain/93049946345\",\"host\":\"very-big-test-store.myshopify.com\"},null]}}"
}

pub fn unsupported_node_implementors_match_introspection_snapshot_test() {
  let possible_node_types = read_possible_node_types()
  let supported_node_types =
    admin_platform.list_supported_admin_platform_node_types()
  let unsupported_node_types =
    possible_node_types
    |> list.filter(fn(node_type) {
      !list.contains(supported_node_types, node_type)
    })
  let supported_possible_node_types =
    possible_node_types
    |> list.filter(fn(node_type) {
      list.contains(supported_node_types, node_type)
    })

  assert supported_possible_node_types == supported_node_types
  assert unsupported_node_types
    == [
      "AbandonedCheckout",
      "AbandonedCheckoutLineItem",
      "Abandonment",
      "AddAllProductsOperation",
      "AdditionalFee",
      "AppCatalog",
      "AppCredit",
      "AppRevenueAttributionRecord",
      "Article",
      "BasicEvent",
      "Blog",
      "BulkOperation",
      "BusinessEntity",
      "CalculatedOrder",
      "CartTransform",
      "CashDrawer",
      "CashManagementCustomReasonCode",
      "CashManagementDefaultReasonCode",
      "CashManagementSystemReasonCode",
      "CashTrackingAdjustment",
      "CashTrackingSession",
      "CatalogCsvOperation",
      "Channel",
      "ChannelDefinition",
      "ChannelInformation",
      "CheckoutAndAccountsConfiguration",
      "CheckoutAndAccountsConfigurationOverride",
      "CheckoutProfile",
      "Comment",
      "CommentEvent",
      "Company",
      "CompanyContact",
      "CompanyContactRole",
      "CompanyLocation",
      "CompanyLocationCatalog",
      "CompanyLocationStaffMemberAssignment",
      "ConsentPolicy",
      "CurrencyExchangeAdjustment",
      "CustomerAccountAppExtensionPage",
      "CustomerAccountNativePage",
      "CustomerPaymentMethod",
      "CustomerSegmentMembersQuery",
      "CustomerVisit",
      "DeliveryCarrierService",
      "DeliveryCustomization",
      "DeliveryMethod",
      "DeliveryProfile",
      "DeliveryProfileItem",
      "DeliveryPromiseParticipant",
      "DeliveryPromiseProvider",
      "DiscountAutomaticBxgy",
      "DiscountAutomaticNode",
      "DiscountCodeNode",
      "DiscountNode",
      "DiscountRedeemCodeBulkCreation",
      "DraftOrder",
      "DraftOrderLineItem",
      "DraftOrderTag",
      "Duty",
      "ExchangeLineItem",
      "ExchangeV2",
      "ExternalVideo",
      "Fulfillment",
      "FulfillmentConstraintRule",
      "FulfillmentEvent",
      "FulfillmentHold",
      "FulfillmentLineItem",
      "FulfillmentOrder",
      "FulfillmentOrderDestination",
      "FulfillmentOrderLineItem",
      "FulfillmentOrderMerchantRequest",
      "GenericFile",
      "GiftCard",
      "GiftCardCreditTransaction",
      "GiftCardDebitTransaction",
      "InventoryAdjustmentGroup",
      "InventoryItem",
      "InventoryItemMeasurement",
      "InventoryLevel",
      "InventoryQuantity",
      "InventoryShipment",
      "InventoryShipmentLineItem",
      "InventoryTransfer",
      "InventoryTransferLineItem",
      "LineItem",
      "LineItemGroup",
      "MailingAddress",
      "Market",
      "MarketCatalog",
      "MarketingActivity",
      "MarketingEvent",
      "MediaImage",
      "Menu",
      "MetafieldDefinition",
      "Metaobject",
      "MetaobjectDefinition",
      "Model3d",
      "OnlineStoreTheme",
      "Order",
      "OrderAdjustment",
      "OrderDisputeSummary",
      "OrderEditSession",
      "OrderTransaction",
      "Page",
      "PaymentCustomization",
      "PaymentMandate",
      "PaymentSchedule",
      "PaymentTerms",
      "PaymentTermsTemplate",
      "PointOfSaleDevice",
      "PointOfSaleDevicePaymentSession",
      "PriceList",
      "PriceRule",
      "PriceRuleDiscountCode",
      "ProductBundleOperation",
      "ProductDeleteOperation",
      "ProductDuplicateOperation",
      "ProductFeed",
      "ProductSetOperation",
      "ProductTaxonomyNode",
      "ProductVariant",
      "ProductVariantComponent",
      "Publication",
      "PublicationResourceOperation",
      "QuantityPriceBreak",
      "Refund",
      "RefundShippingLine",
      "Return",
      "ReturnLineItem",
      "ReturnReasonDefinition",
      "ReturnableFulfillment",
      "ReverseDelivery",
      "ReverseDeliveryLineItem",
      "ReverseFulfillmentOrder",
      "ReverseFulfillmentOrderDisposition",
      "ReverseFulfillmentOrderLineItem",
      "SaleAdditionalFee",
      "SavedSearch",
      "ScriptTag",
      "Segment",
      "SellingPlanGroup",
      "ServerPixel",
      "ShopifyPaymentsAccount",
      "ShopifyPaymentsBalanceTransaction",
      "ShopifyPaymentsBankAccount",
      "ShopifyPaymentsDispute",
      "ShopifyPaymentsDisputeEvidence",
      "ShopifyPaymentsDisputeFileUpload",
      "ShopifyPaymentsDisputeFulfillment",
      "ShopifyPaymentsPayout",
      "StaffMember",
      "StandardMetafieldDefinitionTemplate",
      "StoreCreditAccount",
      "StoreCreditAccountCreditTransaction",
      "StoreCreditAccountDebitRevertTransaction",
      "StoreCreditAccountDebitTransaction",
      "StorefrontAccessToken",
      "SubscriptionBillingAttempt",
      "SubscriptionContract",
      "SubscriptionDraft",
      "TaxonomyAttribute",
      "TaxonomyChoiceListAttribute",
      "TaxonomyMeasurementAttribute",
      "TaxonomyValue",
      "TenderTransaction",
      "TransactionFee",
      "UnverifiedReturnLineItem",
      "UrlRedirect",
      "UrlRedirectImport",
      "Validation",
      "Video",
      "WebPixel",
      "WebhookSubscription",
    ]
}

fn read_possible_node_types() -> List(String) {
  let assert Ok(source) = simplifile.read(admin_platform_fixture_path)
  let assert Ok(node_types) = json.parse(source, node_types_decoder())
  node_types |> list.sort(by: string.compare)
}

fn node_types_decoder() -> decode.Decoder(List(String)) {
  use introspection <- decode.field("introspection", introspection_decoder())
  decode.success(introspection)
}

fn introspection_decoder() -> decode.Decoder(List(String)) {
  use node_interface <- decode.field("nodeInterface", node_interface_decoder())
  decode.success(node_interface)
}

fn node_interface_decoder() -> decode.Decoder(List(String)) {
  use possible_types <- decode.field(
    "possibleTypes",
    decode.list(of: node_type_decoder()),
  )
  decode.success(possible_types)
}

fn node_type_decoder() -> decode.Decoder(String) {
  use name <- decode.field("name", decode.string)
  decode.success(name)
}

pub fn staff_roots_return_access_denied_errors_test() {
  let body =
    run_query(
      store.new(),
      "query { staffMember(id: \"gid://shopify/StaffMember/1\") { id } staffMembers(first: 1) { nodes { id } } }",
    )

  assert string.contains(body, "\"staffMember\":null")
  assert string.contains(body, "\"staffMembers\":null")
  assert string.contains(body, "Access denied for staffMember field.")
  assert string.contains(body, "Access denied for staffMembers field.")
  assert string.contains(body, "\"code\":\"ACCESS_DENIED\"")
}

fn seeded_product_option_store() {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      id: "gid://shopify/Product/optioned",
      legacy_resource_id: None,
      title: "Optioned Board",
      handle: "optioned-board",
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: None,
      tracks_inventory: None,
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
    ),
  ])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
        ),
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/blue",
          name: "Blue",
          has_variants: False,
        ),
      ],
    ),
  ])
}

fn make_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/63755419881",
    name: "very-big-test-store",
    myshopify_domain: "very-big-test-store.myshopify.com",
    url: "https://very-big-test-store.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/93049946345",
      host: "very-big-test-store.myshopify.com",
      url: "https://very-big-test-store.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "shopify@gadget.dev",
    email: "shopify@gadget.dev",
    currency_code: "CAD",
    enabled_presentment_currencies: ["CAD"],
    iana_timezone: "America/Toronto",
    timezone_abbreviation: "EDT",
    timezone_offset: "-0400",
    timezone_offset_minutes: -240,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "METRIC_SYSTEM",
    weight_unit: "KILOGRAMS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/63755419881",
      address1: Some("103 ossington"),
      address2: None,
      city: Some("Ottawa"),
      company: None,
      coordinates_validated: False,
      country: Some("Canada"),
      country_code_v2: Some("CA"),
      formatted: ["103 ossington", "Ottawa ON k1s3b7", "Canada"],
      formatted_area: Some("Ottawa ON, Canada"),
      latitude: Some(45.389817),
      longitude: Some(-75.68692920000001),
      phone: Some(""),
      province: Some("Ontario"),
      province_code: Some("ON"),
      zip: Some("k1s3b7"),
    ),
    plan: ShopPlanRecord(
      partner_development: True,
      public_display_name: "Development",
      shopify_plus: False,
    ),
    resource_limits: ShopResourceLimitsRecord(
      location_limit: 1000,
      max_product_options: 3,
      max_product_variants: 2048,
      redirect_limit_reached: False,
    ),
    features: ShopFeaturesRecord(
      avalara_avatax: False,
      branding: "SHOPIFY",
      bundles: ShopBundlesFeatureRecord(
        eligible_for_bundles: True,
        ineligibility_reason: None,
        sells_bundles: False,
      ),
      captcha: True,
      cart_transform: ShopCartTransformFeatureRecord(
        eligible_operations: ShopCartTransformEligibleOperationsRecord(
          expand_operation: True,
          merge_operation: True,
          update_operation: True,
        ),
      ),
      dynamic_remarketing: False,
      eligible_for_subscription_migration: False,
      eligible_for_subscriptions: False,
      gift_cards: True,
      harmonized_system_code: True,
      legacy_subscription_gateway_enabled: False,
      live_view: True,
      paypal_express_subscription_gateway_status: "DISABLED",
      reports: True,
      sells_subscriptions: False,
      show_metrics: True,
      storefront: True,
      unified_markets: True,
    ),
    payment_settings: PaymentSettingsRecord(supported_digital_wallets: []),
    shop_policies: [],
  )
}

pub fn backup_region_update_stages_and_reads_back_test() {
  let source = store.new()
  let identity = synthetic_identity.new()
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id name code } userErrors { field message code } } }"
  let outcome =
    admin_platform.process_mutation(
      source,
      identity,
      request_path,
      document,
      empty_vars(),
    )
  let outcome = record_drafts(outcome, request_path, document)

  let mutation_body = json.to_string(outcome.data)
  assert string.contains(
    mutation_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(mutation_body, "\"userErrors\":[]")

  let read_body = run_query(outcome.store, "{ backupRegion { id name code } }")
  assert string.contains(
    read_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert list.length(store.get_log(outcome.store)) == 1
  let assert [entry] = store.get_log(outcome.store)
  assert entry.staged_resource_ids
    == ["gid://shopify/MarketRegionCountry/4062110417202"]
}

pub fn backup_region_update_omitted_region_returns_current_without_log_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn backup_region_update_null_region_returns_staged_current_test() {
  let source = store.new() |> store.upsert_base_shop(make_shop())
  let identity = synthetic_identity.new()
  let request_path = "/admin/api/2026-04/graphql.json"
  let staged =
    admin_platform.process_mutation(
      source,
      identity,
      request_path,
      "mutation { backupRegionUpdate(region: { countryCode: US }) { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
    )
  let outcome =
    admin_platform.process_mutation(
      staged.store,
      staged.identity,
      request_path,
      "mutation { backupRegionUpdate(region: null) { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454910378217\",\"name\":\"United States\",\"code\":\"US\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn backup_region_update_uses_captured_shop_country_evidence_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let source = store.new() |> store.upsert_base_shop(make_shop())
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      backup_region_update_document("US"),
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454910378217\",\"name\":\"United States\",\"code\":\"US\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")

  let read_body = run_query(outcome.store, "{ backupRegion { id name code } }")
  assert string.contains(
    read_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454910378217\",\"name\":\"United States\",\"code\":\"US\"}",
  )
  assert outcome.staged_resource_ids
    == ["gid://shopify/MarketRegionCountry/454910378217"]

  let harry_store =
    store.new()
    |> store.upsert_base_shop(shop_for_domain("harry-test-heelo.myshopify.com"))
  list.each(harry_test_backed_regions(), fn(region) {
    let #(code, id, name) = region
    let outcome =
      admin_platform.process_mutation(
        harry_store,
        synthetic_identity.new(),
        request_path,
        backup_region_update_document(code),
        empty_vars(),
      )
    let body = json.to_string(outcome.data)
    assert string.contains(
      body,
      "\"backupRegion\":{\"id\":\""
        <> id
        <> "\",\"name\":\""
        <> name
        <> "\",\"code\":\""
        <> code
        <> "\"}",
    )
    assert string.contains(body, "\"userErrors\":[]")
  })
}

pub fn backup_region_update_validation_does_not_log_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: ZZ }) { backupRegion { id } userErrors { field message code } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegion\":null")
  assert string.contains(body, "\"message\":\"Region not found.\"")
  assert string.contains(body, "\"code\":\"REGION_NOT_FOUND\"")
  assert store.get_log(outcome.store) == []
}

pub fn flow_utility_mutations_stage_without_sensitive_state_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { sig: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: \"{\\\"id\\\":1}\") { payload signature userErrors { field message } } receive: flowTriggerReceive(handle: \"local-order-created\", payload: \"{\\\"id\\\":1}\") { userErrors { field message } } }"
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
    )
  let outcome = record_drafts(outcome, request_path, document)

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"sig\":{\"payload\":\"{\\\"id\\\":1}\",\"signature\":\"",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert list.length(outcome.staged_resource_ids) == 2
  assert list.length(store.get_log(outcome.store)) == 1
  let staged = outcome.store.staged_state
  assert list.length(staged.admin_platform_flow_signature_order) == 1
  assert list.length(staged.admin_platform_flow_trigger_order) == 1
  assert !string.contains(
    json.to_string(outcome.data),
    "shopify-draft-proxy-flow-signature-local-secret-v1",
  )
}

pub fn flow_validation_branches_do_not_stage_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { badSig: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/0\", payload: \"{}\") { signature userErrors { field message } } badReceive: flowTriggerReceive(handle: \"har-374-missing\", payload: \"{}\") { userErrors { field message } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"badSig\":null")
  assert string.contains(body, "\"Invalid id: gid://shopify/FlowTrigger/0\"")
  assert string.contains(body, "Invalid handle 'har-374-missing'.")
  assert outcome.staged_resource_ids == []
  assert store.get_log(outcome.store) == []
}

pub fn flow_trigger_receive_validation_matches_shopify_test() {
  let no_args =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive { userErrors { field message } } }",
      empty_vars(),
    )
  let no_args_body = json.to_string(no_args.data)
  assert string.contains(no_args_body, "\"field\":[\"handle\"]")
  assert string.contains(
    no_args_body,
    "`handle` and `payload` arguments are required",
  )
  assert no_args.staged_resource_ids == []

  let payload_only =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(payload: { test: \"value\" }) { userErrors { field message } } }",
      empty_vars(),
    )
  let payload_only_body = json.to_string(payload_only.data)
  assert string.contains(payload_only_body, "\"field\":[\"handle\"]")
  assert string.contains(
    payload_only_body,
    "`handle` and `payload` arguments are required",
  )
  assert payload_only.staged_resource_ids == []

  let empty_handle =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(handle: \"\") { userErrors { field message } } }",
      empty_vars(),
    )
  let empty_handle_body = json.to_string(empty_handle.data)
  assert string.contains(empty_handle_body, "\"field\":[\"handle\"]")
  assert string.contains(
    empty_handle_body,
    "`handle` and `payload` arguments are required",
  )
  assert empty_handle.staged_resource_ids == []

  let null_handle =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(handle: null) { userErrors { field message } } }",
      empty_vars(),
    )
  let null_handle_body = json.to_string(null_handle.data)
  assert string.contains(null_handle_body, "\"field\":[\"handle\"]")
  assert string.contains(
    null_handle_body,
    "`handle` and `payload` arguments are required",
  )
  assert null_handle.staged_resource_ids == []

  let conflict =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"properties\\\":{}}\", handle: \"test\") { userErrors { field message } } }",
      empty_vars(),
    )
  let conflict_body = json.to_string(conflict.data)
  assert string.contains(conflict_body, "\"field\":[\"body\"]")
  assert string.contains(
    conflict_body,
    "Cannot use `handle` and `payload` arguments with `body` argument",
  )
  assert !string.contains(conflict_body, "Invalid handle 'test'.")
  assert conflict.staged_resource_ids == []
}

pub fn flow_trigger_receive_accepts_non_local_handle_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(handle: \"my-real-trigger-handle\", payload: { key: \"v\" }) { userErrors { field message } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"userErrors\":[]")
  assert list.length(outcome.staged_resource_ids) == 1
  let staged = outcome.store.staged_state
  assert list.length(staged.admin_platform_flow_trigger_order) == 1
}

pub fn flow_trigger_receive_payload_size_uses_json_utf8_bytes_test() {
  let document =
    "mutation FlowTriggerReceive($payload: JSON) { flowTriggerReceive(handle: \"my-real-trigger-handle\", payload: $payload) { userErrors { field message } } }"
  let too_large_payload = string.repeat("x", times: 49_995) <> "\u{1F600}"
  let too_large =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      dict.from_list([#("payload", root_field.StringVal(too_large_payload))]),
    )
  let too_large_body = json.to_string(too_large.data)
  assert string.contains(
    too_large_body,
    "Properties size exceeds the limit of 50000 bytes.",
  )
  assert too_large.staged_resource_ids == []

  let allowed_payload = string.repeat("x", times: 49_990)
  let allowed =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      dict.from_list([#("payload", root_field.StringVal(allowed_payload))]),
    )
  let allowed_body = json.to_string(allowed.data)
  assert string.contains(allowed_body, "\"userErrors\":[]")
  assert list.length(allowed.staged_resource_ids) == 1
}

pub fn draft_proxy_routes_admin_platform_reads_and_mutations_test() {
  let proxy = draft_proxy.new()
  let read_request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"{ publicApiVersions { handle supported } backupRegion { code } }\"}",
    )
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    draft_proxy.process_request(proxy, read_request)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"backupRegion\":{\"code\":\"CA\"}",
  )

  let mutation_request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { code } userErrors { message } } }\"}",
    )
  let #(Response(status: mutation_status, body: mutation_body, ..), proxy) =
    draft_proxy.process_request(proxy, mutation_request)
  assert mutation_status == 200
  assert string.contains(
    json.to_string(mutation_body),
    "\"backupRegionUpdate\":{\"backupRegion\":{\"code\":\"CA\"},\"userErrors\":[]}",
  )
  assert list.length(store.get_log(proxy.store)) == 1
}

fn backup_region_update_document(code: String) -> String {
  "mutation { backupRegionUpdate(region: { countryCode: "
  <> code
  <> " }) { backupRegion { id name code } userErrors { field message code } } }"
}

fn harry_test_backed_regions() -> List(#(String, String, String)) {
  [
    #(
      "AE",
      "gid://shopify/MarketRegionCountry/4062110482738",
      "United Arab Emirates",
    ),
    #("AT", "gid://shopify/MarketRegionCountry/4062110515506", "Austria"),
    #("AU", "gid://shopify/MarketRegionCountry/4062110548274", "Australia"),
    #("BE", "gid://shopify/MarketRegionCountry/4062110581042", "Belgium"),
    #("CH", "gid://shopify/MarketRegionCountry/4062110613810", "Switzerland"),
    #("CZ", "gid://shopify/MarketRegionCountry/4062110646578", "Czechia"),
    #("DE", "gid://shopify/MarketRegionCountry/4062110679346", "Germany"),
    #("DK", "gid://shopify/MarketRegionCountry/4062110712114", "Denmark"),
    #("ES", "gid://shopify/MarketRegionCountry/4062110744882", "Spain"),
    #("FI", "gid://shopify/MarketRegionCountry/4062110777650", "Finland"),
    #("MX", "gid://shopify/MarketRegionCountry/4062111334706", "Mexico"),
  ]
}

fn shop_for_domain(domain: String) -> ShopRecord {
  let shop = make_shop()
  ShopRecord(
    ..shop,
    name: "harry-test-heelo",
    myshopify_domain: domain,
    url: "https://" <> domain,
    primary_domain: ShopDomainRecord(
      ..shop.primary_domain,
      host: domain,
      url: "https://" <> domain,
    ),
  )
}

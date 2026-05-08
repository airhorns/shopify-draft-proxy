import gleam/dict
import gleam/dynamic/decode
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/proxy_state.{type Request, Request, Response}
import shopify_draft_proxy/proxy/upstream_query.{
  UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type MarketRecord, type ShopRecord, AccessScopeRecord, AppInstallationRecord,
  AppRecord, BulkOperationRecord, CapturedArray, CapturedObject, CapturedString,
  DelegatedAccessTokenRecord, GiftCardRecord, MarketRecord, Money,
  PaymentSettingsRecord, ProductOptionRecord, ProductOptionValueRecord,
  ProductRecord, ProductSeoRecord, ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopRecord,
  ShopResourceLimitsRecord,
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

pub fn node_reads_resolve_modeled_bulk_operation_and_gift_card_test() {
  let bulk =
    BulkOperationRecord(
      id: "gid://shopify/BulkOperation/1",
      status: "COMPLETED",
      type_: "QUERY",
      error_code: None,
      created_at: "2024-01-01T00:00:00.000Z",
      completed_at: Some("2024-01-01T00:01:00.000Z"),
      object_count: "3",
      root_object_count: "1",
      file_size: Some("120"),
      url: Some("https://example.test/bulk.jsonl"),
      partial_data_url: None,
      query: Some("{ products { edges { node { id } } } }"),
      cursor: None,
      result_jsonl: None,
    )
  let gift_card =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/1",
      legacy_resource_id: "1",
      last_characters: "4242",
      masked_code: "**** **** **** 4242",
      code: None,
      enabled: True,
      notify: True,
      deactivated_at: None,
      expires_on: None,
      note: Some("node coverage"),
      template_suffix: None,
      created_at: "2024-02-01T00:00:00.000Z",
      updated_at: "2024-02-02T00:00:00.000Z",
      initial_value: Money(amount: "50.0", currency_code: "CAD"),
      balance: Money(amount: "25.0", currency_code: "CAD"),
      customer_id: None,
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let source =
    store.new()
    |> store.upsert_base_bulk_operations([bulk])
    |> store.upsert_base_gift_cards([gift_card])
  let body =
    run_query(
      source,
      "query {
        nodes(ids: [
          \"gid://shopify/BulkOperation/1\",
          \"gid://shopify/GiftCard/1\",
          \"gid://shopify/GiftCard/missing\"
        ]) {
          __typename
          id
          ... on BulkOperation {
            status
            type
            objectCount
            url
          }
          ... on GiftCard {
            lastCharacters
            enabled
            balance { amount currencyCode }
          }
        }
      }",
    )

  assert body
    == "{\"data\":{\"nodes\":[{\"__typename\":\"BulkOperation\",\"id\":\"gid://shopify/BulkOperation/1\",\"status\":\"COMPLETED\",\"type\":\"QUERY\",\"objectCount\":\"3\",\"url\":\"https://example.test/bulk.jsonl\"},{\"__typename\":\"GiftCard\",\"id\":\"gid://shopify/GiftCard/1\",\"lastCharacters\":\"4242\",\"enabled\":true,\"balance\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},null]}}"
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
      "BasicEvent",
      "BusinessEntity",
      "CartTransform",
      "CashDrawer",
      "CashManagementCustomReasonCode",
      "CashManagementDefaultReasonCode",
      "CashManagementSystemReasonCode",
      "CashTrackingAdjustment",
      "CashTrackingSession",
      "CatalogCsvOperation",
      "ChannelDefinition",
      "ChannelInformation",
      "CheckoutAndAccountsConfiguration",
      "CheckoutAndAccountsConfigurationOverride",
      "CheckoutProfile",
      "CommentEvent",
      "CompanyLocationCatalog",
      "CompanyLocationStaffMemberAssignment",
      "ConsentPolicy",
      "CurrencyExchangeAdjustment",
      "CustomerAccountAppExtensionPage",
      "CustomerSegmentMembersQuery",
      "CustomerVisit",
      "DeliveryCustomization",
      "DeliveryMethod",
      "DeliveryProfileItem",
      "DeliveryPromiseParticipant",
      "DeliveryPromiseProvider",
      "DiscountAutomaticBxgy",
      "DiscountRedeemCodeBulkCreation",
      "DraftOrderLineItem",
      "DraftOrderTag",
      "Duty",
      "ExchangeLineItem",
      "ExchangeV2",
      "FulfillmentConstraintRule",
      "FulfillmentEvent",
      "FulfillmentHold",
      "FulfillmentLineItem",
      "FulfillmentOrderDestination",
      "FulfillmentOrderLineItem",
      "FulfillmentOrderMerchantRequest",
      "GiftCardCreditTransaction",
      "GiftCardDebitTransaction",
      "InventoryAdjustmentGroup",
      "InventoryItemMeasurement",
      "InventoryQuantity",
      "InventoryShipmentLineItem",
      "InventoryTransferLineItem",
      "LineItem",
      "LineItemGroup",
      "MailingAddress",
      "Menu",
      "OrderAdjustment",
      "OrderDisputeSummary",
      "OrderEditSession",
      "OrderTransaction",
      "PaymentMandate",
      "PaymentTermsTemplate",
      "PointOfSaleDevice",
      "PointOfSaleDevicePaymentSession",
      "PriceRule",
      "PriceRuleDiscountCode",
      "ProductTaxonomyNode",
      "ProductVariantComponent",
      "PublicationResourceOperation",
      "QuantityPriceBreak",
      "Refund",
      "RefundShippingLine",
      "Return",
      "ReturnLineItem",
      "ReturnReasonDefinition",
      "ReturnableFulfillment",
      "ReverseDeliveryLineItem",
      "ReverseFulfillmentOrderDisposition",
      "ReverseFulfillmentOrderLineItem",
      "SaleAdditionalFee",
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
      "StoreCreditAccountCreditTransaction",
      "StoreCreditAccountDebitRevertTransaction",
      "StoreCreditAccountDebitTransaction",
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
      "UrlRedirectImport",
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
      requires_selling_plan: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    ),
  ])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      linked_metafield: None,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
          linked_metafield_value: None,
        ),
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/blue",
          name: "Blue",
          has_variants: False,
          linked_metafield_value: None,
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
      b2b_deposits_enabled: True,
      discounts_by_market_enabled: False,
      markets_granted: 50,
      sells_subscriptions: False,
      show_metrics: True,
      storefront: True,
      unified_markets: True,
    ),
    payment_settings: PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
    shop_policies: [],
  )
}

fn non_markets_home_shop() -> ShopRecord {
  let shop = make_shop()
  ShopRecord(
    ..shop,
    features: ShopFeaturesRecord(..shop.features, unified_markets: False),
  )
}

fn store_with_app_scopes(scope_handles: List(String)) {
  let app =
    AppRecord(
      id: "gid://shopify/App/backup-region-access-test",
      api_key: Some("backup-region-access-test"),
      handle: Some("backup-region-access-test"),
      title: Some("Backup region access test"),
      developer_name: Some("shopify-draft-proxy"),
      embedded: Some(True),
      previously_installed: Some(False),
      requested_access_scopes: access_scopes(scope_handles),
    )
  let installation =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/backup-region-access-test",
      app_id: app.id,
      launch_url: None,
      uninstall_url: None,
      access_scopes: access_scopes(scope_handles),
      active_subscription_ids: [],
      all_subscription_ids: [],
      one_time_purchase_ids: [],
      uninstalled_at: None,
    )
  store.upsert_base_app_installation(store.new(), installation, app)
}

fn access_scopes(handles: List(String)) {
  list.map(handles, fn(handle) {
    AccessScopeRecord(handle: handle, description: None)
  })
}

fn store_with_delegated_token(
  raw_token: String,
  scope_handles: List(String),
) -> store.Store {
  let token =
    DelegatedAccessTokenRecord(
      id: "gid://shopify/DelegateAccessToken/backup-region-access-test",
      api_client_id: "shopify-draft-proxy-local-app",
      parent_access_token_sha256: None,
      access_token_sha256: crypto.sha256_hex(raw_token),
      access_token_preview: "shpat_...proxy_1",
      access_scopes: scope_handles,
      created_at: "2024-01-01T00:00:00.000Z",
      expires_in: Some(3600),
      destroyed_at: None,
    )
  let #(_, staged_store) =
    store.stage_delegated_access_token(store.new(), token)
  staged_store
}

fn upstream_context_with_headers(headers: dict.Dict(String, String)) {
  UpstreamContext(
    transport: None,
    origin: "",
    headers: headers,
    allow_upstream_reads: False,
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
      empty_upstream_context(),
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

pub fn backup_region_update_without_markets_access_returns_access_denied_test() {
  let source = store_with_app_scopes(["read_products", "write_products"])
  let document =
    "mutation BackupRegionUpdateIdempotent { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id name code } userErrors { field message code } } }"
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegionUpdate\":null")
  assert string.contains(
    body,
    "Access denied for backupRegionUpdate field. Required access: `read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.",
  )
  assert string.contains(body, "\"code\":\"ACCESS_DENIED\"")
  assert string.contains(
    body,
    "\"requiredAccess\":\"`read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.\"",
  )
  assert string.contains(body, "\"path\":[\"backupRegionUpdate\"]")
  assert !string.contains(body, "\"userErrors\":[]")
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
  assert store.get_log(outcome.store) == []

  let read_body = run_query(outcome.store, "{ backupRegion { id name code } }")
  assert string.contains(
    read_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
}

pub fn backup_region_update_active_delegate_without_markets_access_returns_access_denied_test() {
  let raw_token = "shpat_delegate_proxy_1"
  let source = store_with_delegated_token(raw_token, ["read_products"])
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id } userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_headers(
        dict.from_list([
          #("X-Shopify-Access-Token", raw_token),
        ]),
      ),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegionUpdate\":null")
  assert string.contains(body, "\"code\":\"ACCESS_DENIED\"")
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
  assert store.get_log(outcome.store) == []
}

pub fn backup_region_update_non_markets_home_shop_returns_access_denied_test() {
  let source = store.new() |> store.upsert_base_shop(non_markets_home_shop())
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegionUpdate\":null")
  assert string.contains(body, "\"code\":\"ACCESS_DENIED\"")
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
  assert store.get_log(outcome.store) == []
}

pub fn backup_region_update_with_markets_access_still_stages_test() {
  let source = store_with_app_scopes(["read_markets", "write_markets"])
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert outcome.staged_resource_ids
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
      empty_upstream_context(),
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
  let source =
    store.new()
    |> store.upsert_base_shop(make_shop())
    |> store.upsert_base_markets([active_region_market("US")])
  let identity = synthetic_identity.new()
  let request_path = "/admin/api/2026-04/graphql.json"
  let staged =
    admin_platform.process_mutation(
      source,
      identity,
      request_path,
      "mutation { backupRegionUpdate(region: { countryCode: US }) { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let outcome =
    admin_platform.process_mutation(
      staged.store,
      staged.identity,
      request_path,
      "mutation { backupRegionUpdate(region: null) { backupRegion { id name code } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
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
      backup_region_update_document("CA"),
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454909493481\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")

  let read_body = run_query(outcome.store, "{ backupRegion { id name code } }")
  assert string.contains(
    read_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454909493481\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert outcome.staged_resource_ids
    == ["gid://shopify/MarketRegionCountry/454909493481"]

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
        empty_upstream_context(),
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

pub fn backup_region_update_country_without_non_legacy_market_returns_region_not_found_test() {
  let source = store.new() |> store.upsert_base_shop(make_shop())
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      backup_region_update_document("US"),
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegion\":null")
  assert string.contains(body, "\"code\":\"REGION_NOT_FOUND\"")
  assert outcome.staged_resource_ids == []
  assert outcome.store.staged_state.backup_region == None
}

pub fn backup_region_update_country_with_local_region_market_still_stages_test() {
  let source =
    store.new()
    |> store.upsert_base_shop(make_shop())
    |> store.upsert_base_markets([active_region_market("US")])
  let outcome =
    admin_platform.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      backup_region_update_document("US"),
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/454910378217\",\"name\":\"United States\",\"code\":\"US\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert outcome.staged_resource_ids
    == ["gid://shopify/MarketRegionCountry/454910378217"]
}

pub fn backup_region_update_validation_does_not_log_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: ZZ }) { backupRegion { id } userErrors { __typename field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegion\":null")
  assert string.contains(body, "\"__typename\":\"MarketUserError\"")
  assert !string.contains(body, "\"__typename\":\"UserError\"")
  assert string.contains(body, "\"message\":\"Region not found.\"")
  assert string.contains(body, "\"code\":\"REGION_NOT_FOUND\"")
  assert store.get_log(outcome.store) == []
}

pub fn backup_region_update_missing_country_code_coercion_error_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_mutation_request(
        "mutation { backupRegionUpdate(region: {}) { backupRegion { id } userErrors { field code } } }",
      ),
    )

  let serialized = json.to_string(body)
  assert status == 200
  assert string.contains(serialized, "\"errors\"")
  assert string.contains(
    serialized,
    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
  )
  assert string.contains(
    serialized,
    "\"code\":\"missingRequiredInputObjectAttribute\"",
  )
  assert string.contains(
    serialized,
    "\"path\":[\"mutation\",\"backupRegionUpdate\",\"region\",\"countryCode\"]",
  )
  assert !string.contains(serialized, "\"data\"")
  assert !string.contains(serialized, "REGION_NOT_FOUND")
  assert store.get_log(proxy.store) == []
}

pub fn backup_region_update_null_country_code_coercion_error_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_mutation_request(
        "mutation { backupRegionUpdate(region: { countryCode: null }) { backupRegion { id } userErrors { field code } } }",
      ),
    )

  let serialized = json.to_string(body)
  assert status == 200
  assert string.contains(serialized, "\"errors\"")
  assert string.contains(
    serialized,
    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (null). Expected type 'CountryCode!'.",
  )
  assert string.contains(
    serialized,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )
  assert string.contains(serialized, "\"typeName\":\"InputObject\"")
  assert !string.contains(serialized, "\"data\"")
  assert !string.contains(serialized, "REGION_NOT_FOUND")
  assert store.get_log(proxy.store) == []
}

pub fn backup_region_update_numeric_country_code_coercion_error_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_mutation_request(
        "mutation { backupRegionUpdate(region: { countryCode: 42 }) { backupRegion { id } userErrors { field code } } }",
      ),
    )

  let serialized = json.to_string(body)
  assert status == 200
  assert string.contains(serialized, "\"errors\"")
  assert string.contains(
    serialized,
    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (42). Expected type 'CountryCode!'.",
  )
  assert string.contains(
    serialized,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )
  assert string.contains(serialized, "\"typeName\":\"InputObject\"")
  assert !string.contains(serialized, "\"data\"")
  assert !string.contains(serialized, "REGION_NOT_FOUND")
  assert store.get_log(proxy.store) == []
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
      empty_upstream_context(),
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

pub fn flow_generate_signature_canonicalizes_payload_before_signing_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let id = "gid://shopify/FlowTrigger/374"
  let compact_payload = "{\"foo\":1,\"bar\":2}"
  let pretty_payload = "{\n  \"foo\": 1,\n  \"bar\": 2\n}"
  let expected_signature =
    crypto.sha256_hex(
      "shopify-draft-proxy-flow-signature-local-secret-v1|"
      <> id
      <> "|"
      <> compact_payload,
    )
  let document =
    "mutation FlowGenerateSignaturePayloadCanonicalization($pretty: String!, $compact: String!) { pretty: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: $pretty) { payload signature userErrors { field message } } compact: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: $compact) { payload signature userErrors { field message } } }"
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      dict.from_list([
        #("pretty", root_field.StringVal(pretty_payload)),
        #("compact", root_field.StringVal(compact_payload)),
      ]),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"pretty\":{\"payload\":\"{\\\"foo\\\":1,\\\"bar\\\":2}\",\"signature\":\""
      <> expected_signature
      <> "\",\"userErrors\":[]}",
  )
  assert string.contains(
    body,
    "\"compact\":{\"payload\":\"{\\\"foo\\\":1,\\\"bar\\\":2}\",\"signature\":\""
      <> expected_signature
      <> "\",\"userErrors\":[]}",
  )
  assert !string.contains(body, "\\n  ")
  assert list.length(outcome.staged_resource_ids) == 2
  let staged = outcome.store.staged_state
  assert list.length(staged.admin_platform_flow_signature_order) == 2
}

pub fn flow_generate_signature_invalid_payload_returns_user_error_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: \"oops\") { payload signature userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"payload\":null")
  assert string.contains(body, "\"signature\":null")
  assert string.contains(body, "\"field\":[\"payload\"]")
  assert string.contains(body, "Errors validating schema:")
  assert !string.contains(body, "\"signature\":\"")
  assert outcome.staged_resource_ids == []
  assert store.get_log(outcome.store) == []
}

pub fn flow_validation_branches_do_not_stage_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { badSig: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/0\", payload: \"{}\") { signature userErrors { field message } } badReceive: flowTriggerReceive(handle: \"har-374-missing\", payload: \"{}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"badSig\":null")
  assert string.contains(body, "\"Invalid id: gid://shopify/FlowTrigger/0\"")
  assert string.contains(body, "Invalid handle 'har-374-missing'.")
  assert outcome.staged_resource_ids == []
  assert store.get_log(outcome.store) == []
}

pub fn flow_generate_signature_required_arguments_do_not_stage_test() {
  let missing_both =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowGenerateSignature { signature } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_both_body = json.to_string(missing_both.data)
  assert string.contains(missing_both_body, "\"flowGenerateSignature\":null")
  assert string.contains(
    missing_both_body,
    "Field 'flowGenerateSignature' is missing required arguments: id, payload",
  )
  assert string.contains(
    missing_both_body,
    "\"code\":\"missingRequiredArguments\"",
  )
  assert string.contains(missing_both_body, "\"arguments\":\"id, payload\"")
  assert !string.contains(missing_both_body, "RESOURCE_NOT_FOUND")
  assert missing_both.staged_resource_ids == []
  assert store.get_log(missing_both.store) == []

  let missing_payload =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\") { signature payload userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_payload_body = json.to_string(missing_payload.data)
  assert string.contains(missing_payload_body, "\"flowGenerateSignature\":null")
  assert string.contains(
    missing_payload_body,
    "Field 'flowGenerateSignature' is missing required arguments: payload",
  )
  assert string.contains(
    missing_payload_body,
    "\"code\":\"missingRequiredArguments\"",
  )
  assert string.contains(missing_payload_body, "\"arguments\":\"payload\"")
  assert !string.contains(missing_payload_body, "\"signature\"")
  assert missing_payload.staged_resource_ids == []
  assert store.get_log(missing_payload.store) == []
}

pub fn flow_generate_signature_null_arguments_do_not_stage_test() {
  let null_id =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowGenerateSignature(id: null, payload: \"{}\") { signature } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let null_id_body = json.to_string(null_id.data)
  assert string.contains(null_id_body, "\"flowGenerateSignature\":null")
  assert string.contains(
    null_id_body,
    "Argument 'id' on Field 'flowGenerateSignature' has an invalid value (null). Expected type 'ID!'.",
  )
  assert string.contains(
    null_id_body,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )
  assert string.contains(null_id_body, "\"argumentName\":\"id\"")
  assert !string.contains(null_id_body, "RESOURCE_NOT_FOUND")
  assert null_id.staged_resource_ids == []
  assert store.get_log(null_id.store) == []

  let null_payload =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: null) { signature } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let null_payload_body = json.to_string(null_payload.data)
  assert string.contains(null_payload_body, "\"flowGenerateSignature\":null")
  assert string.contains(
    null_payload_body,
    "Argument 'payload' on Field 'flowGenerateSignature' has an invalid value (null). Expected type 'String!'.",
  )
  assert string.contains(
    null_payload_body,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )
  assert string.contains(null_payload_body, "\"argumentName\":\"payload\"")
  assert !string.contains(null_payload_body, "\"signature\"")
  assert null_payload.staged_resource_ids == []
  assert store.get_log(null_payload.store) == []
}

pub fn flow_trigger_receive_validation_matches_shopify_test() {
  let no_args =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
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
      empty_upstream_context(),
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
      empty_upstream_context(),
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
      empty_upstream_context(),
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
      empty_upstream_context(),
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

pub fn flow_trigger_receive_body_only_validates_json_and_schema_test() {
  let invalid_json =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"not json\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let invalid_json_body = json.to_string(invalid_json.data)
  assert string.contains(invalid_json_body, "\"field\":[\"body\"]")
  assert string.contains(
    invalid_json_body,
    "Errors validating schema:\\n  unexpected token 'not' at line 1 column 1\\n",
  )
  assert invalid_json.staged_resource_ids == []
  assert invalid_json.store.staged_state.admin_platform_flow_trigger_order == []

  let properties_not_object =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"properties\\\":\\\"oops\\\"}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let properties_not_object_body = json.to_string(properties_not_object.data)
  assert string.contains(properties_not_object_body, "\"field\":[\"body\"]")
  assert string.contains(
    properties_not_object_body,
    "Errors validating schema:\\n  Type error for field 'properties': oops is not an Object.\\n",
  )
  assert properties_not_object.staged_resource_ids == []

  let missing_resource_url =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"resources\\\":[{\\\"name\\\":\\\"x\\\"}],\\\"properties\\\":{}}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_resource_url_body = json.to_string(missing_resource_url.data)
  assert string.contains(missing_resource_url_body, "\"field\":[\"body\"]")
  assert string.contains(
    missing_resource_url_body,
    "Errors validating schema:\\n  Required field missing: 'url'.\\n",
  )
  assert missing_resource_url.staged_resource_ids == []

  let missing_trigger_reference =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"properties\\\":{}}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_trigger_reference_body =
    json.to_string(missing_trigger_reference.data)
  assert string.contains(missing_trigger_reference_body, "\"field\":[\"body\"]")
  assert string.contains(
    missing_trigger_reference_body,
    "Errors validating schema:\\n  Required field missing: 'trigger_id'.\\n",
  )
  assert missing_trigger_reference.staged_resource_ids == []
  assert missing_trigger_reference.store.staged_state.admin_platform_flow_trigger_order
    == []

  let unknown_trigger_id =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"properties\\\":{}}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let unknown_trigger_id_body = json.to_string(unknown_trigger_id.data)
  assert string.contains(
    unknown_trigger_id_body,
    "Errors validating schema:\\n  Invalid trigger_id 'abc'.\\n",
  )
  assert unknown_trigger_id.staged_resource_ids == []

  let unknown_trigger_title =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_title\\\":\\\"foo\\\",\\\"properties\\\":{}}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let unknown_trigger_title_body = json.to_string(unknown_trigger_title.data)
  assert string.contains(
    unknown_trigger_title_body,
    "Errors validating schema:\\n  Invalid trigger_title 'foo'.\\n",
  )
  assert unknown_trigger_title.staged_resource_ids == []

  let non_absolute_url =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"properties\\\":{},\\\"resources\\\":[{\\\"url\\\":\\\"not-a-url\\\",\\\"name\\\":\\\"x\\\"}]}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let non_absolute_url_body = json.to_string(non_absolute_url.data)
  assert string.contains(
    non_absolute_url_body,
    "Errors validating schema:\\n  Type error for field 'url': not-a-url is not an absolute URL.\\n",
  )
  assert non_absolute_url.staged_resource_ids == []

  let unknown_root_key =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"properties\\\":{},\\\"unknown_root\\\":1}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let unknown_root_key_body = json.to_string(unknown_root_key.data)
  assert string.contains(
    unknown_root_key_body,
    "Errors validating schema:\\n  Invalid field: 'unknown_root'.\\n",
  )
  assert !string.contains(unknown_root_key_body, "Invalid trigger_id 'abc'.")
  assert unknown_root_key.staged_resource_ids == []

  let multiple_body_schema_errors =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"{\\\"trigger_id\\\":\\\"abc\\\",\\\"properties\\\":{},\\\"resources\\\":[{\\\"url\\\":\\\"not-a-url\\\"}],\\\"unknown_root\\\":1}\") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let multiple_body_schema_errors_body =
    json.to_string(multiple_body_schema_errors.data)
  assert string.contains(
    multiple_body_schema_errors_body,
    "Errors validating schema:\\n  Invalid field: 'unknown_root'.\\n  Required field missing: 'name'.\\n  Type error for field 'url': not-a-url is not an absolute URL.\\n",
  )
  assert multiple_body_schema_errors.staged_resource_ids == []

  let whitespace_body =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(body: \"   \") { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let whitespace_body_json = json.to_string(whitespace_body.data)
  assert string.contains(whitespace_body_json, "\"field\":[\"handle\"]")
  assert string.contains(
    whitespace_body_json,
    "`handle` and `payload` arguments are required",
  )
  assert whitespace_body.staged_resource_ids == []
}

pub fn flow_trigger_receive_accepts_non_local_handle_test() {
  let outcome =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { flowTriggerReceive(handle: \"my-real-trigger-handle\", payload: { key: \"v\" }) { userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
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
      empty_upstream_context(),
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
      empty_upstream_context(),
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

fn graphql_mutation_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2026-04/graphql.json",
    headers: dict.new(),
    body: "{\"query\":" <> json.to_string(json.string(query)) <> "}",
  )
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

fn active_region_market(country_code: String) -> MarketRecord {
  MarketRecord(
    id: "gid://shopify/Market/test-" <> string.lowercase(country_code),
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString("gid://shopify/Market/test-" <> country_code)),
      #("name", CapturedString("Local " <> country_code)),
      #("status", CapturedString("ACTIVE")),
      #("type", CapturedString("REGION")),
      #(
        "conditions",
        CapturedObject([
          #(
            "regionsCondition",
            CapturedObject([
              #(
                "regions",
                CapturedObject([
                  #(
                    "nodes",
                    CapturedArray([
                      CapturedObject([#("code", CapturedString(country_code))]),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ]),
  )
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

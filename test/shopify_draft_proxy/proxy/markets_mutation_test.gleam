import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, DraftProxy, Request, Response,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type ShopRecord, PaymentSettingsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopRecord, ShopResourceLimitsRecord,
}

fn graphql(query: String) {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  graphql_with_proxy(proxy, query)
}

fn graphql_with_proxy(proxy: DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
}

pub fn price_list_create_accepts_dkk_with_parent_adjustment_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"Denmark\", currency: DKK, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id currency parent { adjustment { type value } } } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/1\",\"currency\":\"DKK\",\"parent\":{\"adjustment\":{\"type\":\"PERCENTAGE_DECREASE\",\"value\":10}}},\"userErrors\":[]}}}"
}

pub fn price_list_create_requires_currency_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"EUR\", parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id currency } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"currency\"],\"message\":\"Currency can't be blank\",\"code\":\"BLANK\"}]}}}"
}

pub fn price_list_create_requires_parent_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"EUR\", currency: EUR }) { priceList { id currency } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"parent\"],\"message\":\"Parent must exist\",\"code\":\"REQUIRED\"}]}}}"
}

pub fn price_list_create_rejects_invalid_parent_adjustment_type_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"EUR\", currency: EUR, parent: { adjustment: { type: FIXED, value: 10 } } }) { priceList { id currency } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"parent\",\"adjustment\",\"type\"],\"message\":\"Type is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn web_presence_create_subfolder_root_urls_include_all_locales_and_shop_domain_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", alternateLocales: [\"fr\", \"de\"], subfolderSuffix: \"intl\" }) { webPresence { subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"domain\":null")
  assert string.contains(
    serialized,
    "\"rootUrls\":[{\"locale\":\"en\",\"url\":\"https://acme.myshopify.com/intl/\"},{\"locale\":\"fr\",\"url\":\"https://acme.myshopify.com/intl/fr/\"},{\"locale\":\"de\",\"url\":\"https://acme.myshopify.com/intl/de/\"}]",
  )
  assert !string.contains(serialized, "harry-test-heelo.myshopify.com")
  assert !string.contains(serialized, "/en-intl/")
}

pub fn web_presence_create_domain_root_urls_resolve_primary_domain_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", alternateLocales: [\"fr\"], domainId: \"gid://shopify/Domain/1000\" }) { webPresence { subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"domain\":{\"id\":\"gid://shopify/Domain/1000\",\"host\":\"acme.myshopify.com\",\"url\":\"https://acme.myshopify.com\",\"sslEnabled\":true}",
  )
  assert string.contains(
    serialized,
    "\"rootUrls\":[{\"locale\":\"en\",\"url\":\"https://acme.myshopify.com/\"},{\"locale\":\"fr\",\"url\":\"https://acme.myshopify.com/fr/\"}]",
  )
  assert string.contains(serialized, "\"subfolderSuffix\":null")
}

fn seeded_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store = store.upsert_base_shop(proxy.store, acme_shop())
  DraftProxy(..proxy, store: seeded_store)
}

fn acme_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/1000",
    name: "acme",
    myshopify_domain: "acme.myshopify.com",
    url: "https://acme.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/1000",
      host: "acme.myshopify.com",
      url: "https://acme.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "shop@example.com",
    email: "shop@example.com",
    currency_code: "USD",
    enabled_presentment_currencies: ["USD"],
    iana_timezone: "America/New_York",
    timezone_abbreviation: "EST",
    timezone_offset: "-0500",
    timezone_offset_minutes: -300,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "IMPERIAL_SYSTEM",
    weight_unit: "POUNDS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/1000",
      address1: Some("1 Main St"),
      address2: None,
      city: Some("New York"),
      company: None,
      coordinates_validated: False,
      country: Some("United States"),
      country_code_v2: Some("US"),
      formatted: ["1 Main St", "New York NY 10001", "United States"],
      formatted_area: Some("New York NY, United States"),
      latitude: None,
      longitude: None,
      phone: None,
      province: Some("New York"),
      province_code: Some("NY"),
      zip: Some("10001"),
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

pub fn market_create_rejects_status_enabled_mismatch_test() {
  let #(Response(status: draft_status, body: draft_body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"Mismatch\", status: DRAFT, enabled: true, regions: [{ countryCode: US }] }) { market { id name status enabled } userErrors { field message code } } }",
    )
  let #(Response(status: active_status, body: active_body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"Mismatch\", status: ACTIVE, enabled: false, regions: [{ countryCode: US }] }) { market { id name status enabled } userErrors { field message code } } }",
    )

  assert draft_status == 200
  assert json.to_string(draft_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Invalid status and enabled combination.\",\"code\":\"INVALID_STATUS_AND_ENABLED_COMBINATION\"}]}}}"
  assert active_status == 200
  assert json.to_string(active_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Invalid status and enabled combination.\",\"code\":\"INVALID_STATUS_AND_ENABLED_COMBINATION\"}]}}}"
}

pub fn market_create_rejects_plan_market_limit_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Market One\", regions: [{ countryCode: BR }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Market Two\", regions: [{ countryCode: CL }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Market Three\", regions: [{ countryCode: PE }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Market Four\", regions: [{ countryCode: CO }] }) { market { id } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/3\"},\"userErrors\":[]}}}"
  assert third_status == 200
  assert json.to_string(third_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/5\"},\"userErrors\":[]}}}"
  assert fourth_status == 200
  assert json.to_string(fourth_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Shop has reached the maximum number of markets for the current plan.\",\"code\":\"SHOP_REACHED_PLAN_MARKETS_LIMIT\"}]}}}"
}

pub fn market_create_rejects_invalid_base_currency_test() {
  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"Currency\", currencySettings: { baseCurrency: XXX } }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: unsupported_status, body: unsupported_body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"Currency\", currencySettings: { baseCurrency: XAF } }) { market { id } userErrors { field message code } } }",
    )

  assert invalid_status == 200
  assert json.to_string(invalid_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"currencySettings\",\"baseCurrency\"],\"message\":\"Base currency is invalid\",\"code\":\"INVALID\"}]}}}"
  assert unsupported_status == 200
  assert json.to_string(unsupported_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"currencySettings\",\"baseCurrency\"],\"message\":\"Base currency is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn market_create_rejects_duplicate_region_country_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Canada Local\", regions: [{ countryCode: CA }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Canada Duplicate\", regions: [{ countryCode: CA }] }) { market { id } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"regions\",\"0\",\"countryCode\"],\"message\":\"Code has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn market_create_dedupes_generated_handles_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe!\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: third_status, body: third_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe?\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe-1\"},\"userErrors\":[]}}}"
  assert third_status == 200
  assert json.to_string(third_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe-2\"},\"userErrors\":[]}}}"
}

pub fn market_create_rejects_duplicate_name_before_handle_dedupe_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn market_create_slugifies_generated_handle_like_shopify_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"  North & South / EU!  \" }) { market { handle } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"north-south-eu\"},\"userErrors\":[]}}}"
}

pub fn market_create_rejects_explicit_duplicate_handle_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Other\", handle: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Generated handle has already been taken\",\"code\":\"GENERATED_DUPLICATED_HANDLE\"}]}}}"
}

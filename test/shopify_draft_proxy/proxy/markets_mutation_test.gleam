import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, DraftProxy, Request, Response,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type MarketRecord, type ProductMetafieldRecord, type ShopRecord,
  CapturedObject, CapturedString, MarketLocalizableContentRecord, MarketRecord,
  PaymentSettingsRecord, ProductMetafieldRecord, ShopAddressRecord,
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

pub fn web_presence_create_accepts_shopify_i18n_locale_codes_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr-CA\", alternateLocales: [\"pt-BR\", \"zh-CN\"], subfolderSuffix: \"fr\" }) { webPresence { subfolderSuffix defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"defaultLocale\":{\"locale\":\"fr-CA\",\"primary\":true}",
  )
  assert string.contains(
    serialized,
    "\"alternateLocales\":[{\"locale\":\"pt-BR\",\"primary\":false},{\"locale\":\"zh-CN\",\"primary\":false}]",
  )
}

pub fn web_presence_create_reports_invalid_alternate_locale_indexes_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr-CA\", alternateLocales: [\"fr\", \"bogus\", \"pt-BR\", \"nope\"], subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"webPresence\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"alternateLocales\",\"1\"],\"message\":\"Invalid locale codes: bogus\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"alternateLocales\",\"3\"],\"message\":\"Invalid locale codes: nope\",\"code\":\"INVALID\"",
  )
  assert !string.contains(serialized, "\"alternateLocales\",\"0\"")
  assert !string.contains(serialized, "\"alternateLocales\",\"2\"")
}

pub fn web_presence_create_validates_routing_and_subfolder_suffix_test() {
  let #(Response(status: mutex_status, body: mutex_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", domainId: \"gid://shopify/Domain/1000\", subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: short_status, body: short_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"x\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: script_status, body: script_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"Latn\" }) { webPresence { id } userErrors { field message code } } }",
    )

  assert mutex_status == 200
  assert missing_status == 200
  assert short_status == 200
  assert script_status == 200
  assert string.contains(
    json.to_string(mutex_body),
    "\"code\":\"CANNOT_HAVE_SUBFOLDER_AND_DOMAIN\"",
  )
  assert string.contains(
    json.to_string(missing_body),
    "\"code\":\"REQUIRES_DOMAIN_OR_SUBFOLDER\"",
  )
  assert string.contains(
    json.to_string(short_body),
    "\"code\":\"SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS\"",
  )
  assert string.contains(
    json.to_string(script_body),
    "\"code\":\"SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE\"",
  )
}

pub fn web_presence_create_reports_unknown_domain_only_when_not_stored_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", domainId: \"gid://shopify/Domain/9999\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"webPresence\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"domainId\"],\"message\":\"Domain does not exist\",\"code\":\"DOMAIN_NOT_FOUND\"",
  )
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
    payment_settings: PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
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

pub fn market_localizations_register_rejects_more_than_100_keys_test() {
  let input = too_many_market_localization_inputs()
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/missing\", marketLocalizations: ["
      <> input
      <> "]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"resourceId\"],\"code\":\"TOO_MANY_KEYS_FOR_RESOURCE\"}]}}}"
}

pub fn market_localizations_register_returns_translation_error_for_missing_resource_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/missing\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"resourceId\"],\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn market_localizations_register_validates_market_key_digest_and_value_test() {
  let proxy = market_localization_proxy()
  let #(Response(status: market_status, body: market_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/missing\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: key_status, body: key_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"value\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: digest_status, body: digest_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"stale\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: value_status, body: value_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"marketId\"],\"code\":\"MARKET_DOES_NOT_EXIST\"}]}}}"
  assert key_status == 200
  assert json.to_string(key_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"key\"],\"code\":\"INVALID_KEY_FOR_MODEL\"}]}}}"
  assert digest_status == 200
  assert json.to_string(digest_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"marketLocalizableContentDigest\"],\"code\":\"INVALID_MARKET_LOCALIZABLE_CONTENT\"}]}}}"
  assert value_status == 200
  assert json.to_string(value_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"value\"],\"code\":\"FAILS_RESOURCE_VALIDATION\"}]}}}"
}

pub fn market_localizations_register_stages_seeded_content_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      market_localization_proxy(),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value market { id name } } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizableContent { key value digest } marketLocalizations { key value market { id name } } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}],\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"marketLocalizableResource\":{\"marketLocalizableContent\":[{\"key\":\"title\",\"value\":\"Title\",\"digest\":\"digest-title\"}],\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}]}}}"
}

pub fn market_localizations_remove_validates_keys_and_markets_test() {
  let proxy = market_localization_proxy()
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key } userErrors { __typename field code } } }",
    )
  let #(Response(status: key_status, body: key_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"value\"], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key } userErrors { __typename field code } } }",
    )
  let #(Response(status: market_status, body: market_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"title\"], marketIds: [\"gid://shopify/Market/missing\"]) { marketLocalizations { key } userErrors { __typename field code } } }",
    )

  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizationKeys\"],\"code\":\"KEY_NOT_FOUND\"}]}}}"
  assert key_status == 200
  assert json.to_string(key_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizationKeys\",\"0\"],\"code\":\"KEY_NOT_FOUND\"}]}}}"
  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketIds\",\"0\"],\"code\":\"MARKET_NOT_FOUND\"}]}}}"
}

pub fn market_localizations_remove_returns_removed_staged_rows_test() {
  let #(Response(status: register_status, body: register_body, ..), proxy) =
    graphql_with_proxy(
      market_localization_proxy(),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value outdated market { id } } userErrors { __typename field code } } }",
    )
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"title\"], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key value outdated market { id } } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizations { key value market { id } } } }",
    )
  let #(Response(status: noop_status, body: noop_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"title\"], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key value outdated market { id } } userErrors { __typename field code } } }",
    )

  assert register_status == 200
  assert json.to_string(register_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"outdated\":false,\"market\":{\"id\":\"gid://shopify/Market/ca\"}}],\"userErrors\":[]}}}"
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"outdated\":false,\"market\":{\"id\":\"gid://shopify/Market/ca\"}}],\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"marketLocalizableResource\":{\"marketLocalizations\":[]}}}"
  assert noop_status == 200
  assert json.to_string(noop_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[]}}}"
}

fn too_many_market_localization_inputs() -> String {
  int.range(from: 1, to: 102, with: [], run: fn(acc, index) {
    [
      "{ marketId: \"gid://shopify/Market/"
        <> int.to_string(index)
        <> "\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }",
      ..acc
    ]
  })
  |> list.reverse
  |> string.join(with: ",")
}

fn market_localization_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_markets([market_localization_market()])
    |> store.replace_base_metafields_for_owner(
      "gid://shopify/Product/localizable",
      [market_localization_metafield()],
    )
  DraftProxy(..proxy, store: seeded_store)
}

fn market_localization_market() -> MarketRecord {
  MarketRecord(
    id: "gid://shopify/Market/ca",
    cursor: Some("gid://shopify/Market/ca"),
    data: CapturedObject([
      #("id", CapturedString("gid://shopify/Market/ca")),
      #("name", CapturedString("Canada")),
    ]),
  )
}

fn market_localization_metafield() -> ProductMetafieldRecord {
  ProductMetafieldRecord(
    id: "gid://shopify/Metafield/localizable",
    owner_id: "gid://shopify/Product/localizable",
    namespace: "custom",
    key: "title",
    type_: Some("single_line_text_field"),
    value: Some("Title"),
    compare_digest: Some("digest-title"),
    json_value: None,
    created_at: None,
    updated_at: None,
    owner_type: Some("PRODUCT"),
    market_localizable_content: [
      MarketLocalizableContentRecord(
        key: "title",
        value: "Title",
        digest: "digest-title",
      ),
    ],
  )
}

pub fn catalog_create_requires_status_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"status\"],\"message\":\"Status is required\",\"code\":\"REQUIRED\"}]}}}"
}

pub fn catalog_create_rejects_invalid_status_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: DISABLED, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"status\"],\"message\":\"Status is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_requires_context_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\"],\"message\":\"Context is required\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_requires_context_driver_type_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: {} }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"driverType\"],\"message\":\"Driver type is required\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_market_context_ids_test() {
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/404\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert missing_status == 200
  assert json.to_string(missing_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"marketIds\",\"0\"],\"message\":\"Market does not exist\",\"code\":\"INVALID\"}]}}}"
  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"marketIds\"],\"message\":\"Market ids can't be blank\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_company_location_context_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" }, companyLocation: { name: \"B2B HQ\" } }) { company { id locations(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [\"gid://shopify/CompanyLocation/404\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: unsupported_status, body: unsupported_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"companyCreate\":{\"company\":{\"id\":\"gid://shopify/Company/1?shopify-draft-proxy=synthetic\",\"locations\":{\"nodes\":[{\"id\":\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"}]}},\"userErrors\":[]}}}"
  assert missing_status == 200
  assert json.to_string(missing_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"companyLocationIds\",\"0\"],\"message\":\"Company location does not exist\",\"code\":\"INVALID\"}]}}}"
  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"companyLocationIds\"],\"message\":\"Company location ids can't be blank\",\"code\":\"INVALID\"}]}}}"
  assert unsupported_status == 200
  assert json.to_string(unsupported_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"driverType\"],\"message\":\"Catalog context driverType COMPANY_LOCATION is not supported by the local MarketCatalog model\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_country_context_test() {
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"Country Catalog\", status: ACTIVE, context: { driverType: COUNTRY, countryCodes: [] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: unsupported_status, body: unsupported_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"Country Catalog\", status: ACTIVE, context: { driverType: COUNTRY, countryCodes: [US] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"countryCodes\"],\"message\":\"Country codes can't be blank\",\"code\":\"INVALID\"}]}}}"
  assert unsupported_status == 200
  assert json.to_string(unsupported_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"driverType\"],\"message\":\"Catalog context driverType COUNTRY is not supported by the local MarketCatalog model\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_stages_market_context_test() {
  let #(Response(status: market_status, body: market_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id title status markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalogs(first: 5, type: MARKET) { nodes { id title status markets(first: 5) { nodes { id } } } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/3\",\"title\":\"EU Catalog\",\"status\":\"ACTIVE\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}},\"userErrors\":[]}}}"
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"title\":\"EU Catalog\",\"status\":\"ACTIVE\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}",
  )
}

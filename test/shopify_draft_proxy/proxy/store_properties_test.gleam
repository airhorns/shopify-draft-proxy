//// Store Properties domain tests for the Gleam port.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type PublicationRecord, type ShopPolicyRecord, type ShopRecord,
  type StorePropertyRecord, type StorePropertyValue, CapturedArray,
  CapturedObject, CapturedString, DeliveryProfileRecord, PaymentSettingsRecord,
  PublicationRecord, ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopPolicyRecord,
  ShopRecord, ShopResourceLimitsRecord, StorePropertyBool, StorePropertyInt,
  StorePropertyList, StorePropertyMutationPayloadRecord, StorePropertyNull,
  StorePropertyObject, StorePropertyRecord, StorePropertyString,
}

fn graphql_request(body: String) -> draft_proxy.Request {
  graphql_request_for_version("2026-04", body)
}

fn graphql_request_for_version(
  api_version: String,
  body: String,
) -> draft_proxy.Request {
  proxy_state.Request(
    method: "POST",
    path: "/admin/api/" <> api_version <> "/graphql.json",
    headers: dict.new(),
    body: body,
  )
}

fn meta_get(path: String) -> draft_proxy.Request {
  proxy_state.Request(method: "GET", path: path, headers: dict.new(), body: "")
}

fn seeded_proxy() -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let seeded_store = store.upsert_base_shop(proxy.store, make_shop([]))
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn make_shop(policies: List(ShopPolicyRecord)) -> ShopRecord {
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
    shop_policies: policies,
  )
}

fn make_policy(body: String) -> ShopPolicyRecord {
  ShopPolicyRecord(
    id: "gid://shopify/ShopPolicy/42438689001",
    title: "Contact",
    body: body,
    type_: "CONTACT_INFORMATION",
    url: "https://very-big-test-store.myshopify.com/63755419881/policies/42438689001.html?locale=en",
    created_at: "2026-04-25T11:52:28Z",
    updated_at: "2026-04-25T11:52:29Z",
    migrated_to_html: True,
  )
}

fn make_legacy_policy(body: String) -> ShopPolicyRecord {
  let policy = make_policy(body)
  ShopPolicyRecord(..policy, migrated_to_html: False)
}

fn make_raw_record(
  id: String,
  typename: String,
  fields: List(#(String, StorePropertyValue)),
) -> StorePropertyRecord {
  StorePropertyRecord(
    id: id,
    cursor: None,
    data: dict.from_list([
      #("__typename", StorePropertyString(typename)),
      #("id", StorePropertyString(id)),
      ..fields
    ]),
  )
}

fn make_publication(id: String) -> PublicationRecord {
  PublicationRecord(
    id: id,
    name: Some("Online Store"),
    auto_publish: Some(False),
    supports_future_publishing: Some(True),
    catalog_id: None,
    channel_id: None,
    cursor: None,
  )
}

fn make_location(
  id: String,
  name: String,
  is_active: Bool,
  activatable: Bool,
  deactivatable: Bool,
  fulfills_online_orders: Bool,
) -> StorePropertyRecord {
  make_raw_record(id, "Location", [
    #("name", StorePropertyString(name)),
    #("isActive", StorePropertyBool(is_active)),
    #("activatable", StorePropertyBool(activatable)),
    #("deactivatable", StorePropertyBool(deactivatable)),
    #("deletable", StorePropertyBool(!is_active)),
    #("fulfillsOnlineOrders", StorePropertyBool(fulfills_online_orders)),
    #("hasActiveInventory", StorePropertyBool(False)),
    #("shipsInventory", StorePropertyBool(is_active)),
  ])
}

fn location_address(
  address1: String,
  city: String,
  country: String,
  country_code: String,
  province_code: String,
  zip: String,
) -> StorePropertyValue {
  StorePropertyObject(
    dict.from_list([
      #("address1", StorePropertyString(address1)),
      #("city", StorePropertyString(city)),
      #("country", StorePropertyString(country)),
      #("countryCode", StorePropertyString(country_code)),
      #("provinceCode", StorePropertyString(province_code)),
      #("zip", StorePropertyString(zip)),
    ]),
  )
}

fn location_inventory_levels(quantity_name: String, quantity: Int) {
  StorePropertyObject(
    dict.from_list([
      #(
        "nodes",
        StorePropertyList([
          StorePropertyObject(
            dict.from_list([
              #(
                "quantities",
                StorePropertyList([
                  StorePropertyObject(
                    dict.from_list([
                      #("name", StorePropertyString(quantity_name)),
                      #("quantity", StorePropertyInt(quantity)),
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

pub fn empty_shop_read_returns_null_test() {
  let body = "{\"query\":\"query { shop { id name } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))

  assert status == 200
  assert json.to_string(response_body) == "{\"data\":{\"shop\":null}}"
}

pub fn shop_read_serializes_seeded_shop_test() {
  let body =
    "{\"query\":\"query { shop { id name myshopifyDomain primaryDomain { id host sslEnabled } shopAddress { id city formatted } features { bundles { eligibleForBundles } cartTransform { eligibleOperations { expandOperation mergeOperation updateOperation } } } shopPolicies { id } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"id\":\"gid://shopify/Shop/63755419881\"",
  )
  assert string.contains(
    serialized,
    "\"myshopifyDomain\":\"very-big-test-store.myshopify.com\"",
  )
  assert string.contains(
    serialized,
    "\"host\":\"very-big-test-store.myshopify.com\"",
  )
  assert string.contains(
    serialized,
    "\"formatted\":[\"103 ossington\",\"Ottawa ON k1s3b7\",\"Canada\"]",
  )
  assert string.contains(serialized, "\"eligibleForBundles\":true")
}

pub fn shop_policy_update_stages_downstream_read_and_log_test() {
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id title body type url createdAt updatedAt translations(locale: \\\"fr\\\") { key } } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"CONTACT_INFORMATION\",\"body\":\"<p>After</p>\"}}}"
  let #(
    proxy_state.Response(status: mutation_status, body: mutation_body_json, ..),
    proxy,
  ) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let mutation_serialized = json.to_string(mutation_body_json)

  assert mutation_status == 200
  assert string.contains(mutation_serialized, "\"userErrors\":[]")
  assert string.contains(
    mutation_serialized,
    "\"title\":\"Contact Information\"",
  )
  assert string.contains(mutation_serialized, "\"body\":\"<p>After</p>\"")
  assert string.contains(
    mutation_serialized,
    "\"url\":\"https://very-big-test-store.myshopify.com/63755419881/policies/1.html?locale=en\"",
  )
  assert string.contains(
    mutation_serialized,
    "\"id\":\"gid://shopify/ShopPolicy/1\"",
  )
  assert string.contains(mutation_serialized, "\"translations\":[]")

  let read_body =
    "{\"query\":\"query { shop { shopPolicies { id title body type url createdAt updatedAt translations(locale: \\\"fr\\\") { key } } } }\"}"
  let #(
    proxy_state.Response(status: read_status, body: read_body_json, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(read_body))
  let read_serialized = json.to_string(read_body_json)

  assert read_status == 200
  assert string.contains(read_serialized, "\"title\":\"Contact Information\"")
  assert string.contains(read_serialized, "\"body\":\"<p>After</p>\"")
  assert string.contains(
    read_serialized,
    "\"url\":\"https://very-big-test-store.myshopify.com/63755419881/policies/1.html?locale=en\"",
  )
  assert string.contains(read_serialized, "\"translations\":[]")

  let log = json.to_string(draft_proxy.get_log_snapshot(proxy))
  assert string.contains(log, "\"domain\":\"store-properties\"")
  assert string.contains(
    log,
    "\"stagedResourceIds\":[\"gid://shopify/ShopPolicy/1\"]",
  )
}

pub fn shop_policy_update_uses_shopify_title_case_for_all_policy_types_test() {
  [
    #("CONTACT_INFORMATION", "Contact Information"),
    #("LEGAL_NOTICE", "Legal Notice"),
    #("PRIVACY_POLICY", "Privacy Policy"),
    #("REFUND_POLICY", "Refund Policy"),
    #("SHIPPING_POLICY", "Shipping Policy"),
    #("SUBSCRIPTION_POLICY", "Subscription Policy"),
    #("TERMS_OF_SALE", "Terms of Sale"),
    #("TERMS_OF_SERVICE", "Terms of Service"),
  ]
  |> list.each(fn(entry) {
    let #(type_, expected_title) = entry
    let mutation_body =
      "{\"query\":\"mutation { shopPolicyUpdate(shopPolicy: { type: "
      <> type_
      <> ", body: \\\"<p>After</p>\\\" }) { shopPolicy { title url } userErrors { field message code } } }\"}"
    let #(proxy_state.Response(status: status, body: response_body, ..), _) =
      draft_proxy.process_request(
        seeded_proxy(),
        graphql_request(mutation_body),
      )
    let serialized = json.to_string(response_body)

    assert status == 200
    assert string.contains(serialized, "\"title\":\"" <> expected_title <> "\"")
    assert string.contains(
      serialized,
      "\"url\":\"https://very-big-test-store.myshopify.com/63755419881/policies/1.html?locale=en\"",
    )
    assert string.contains(serialized, "\"userErrors\":[]")
  })
}

pub fn shop_policy_update_new_plain_text_body_is_migrated_and_verbatim_test() {
  let mutation_body =
    "{\"query\":\"mutation { shopPolicyUpdate(shopPolicy: { type: PRIVACY_POLICY, body: \\\"Line one\\\\nLine two\\\" }) { shopPolicy { body } userErrors { field message code } } }\"}"
  let #(
    proxy_state.Response(status: mutation_status, body: mutation_body_json, ..),
    proxy,
  ) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let mutation_serialized = json.to_string(mutation_body_json)

  assert mutation_status == 200
  assert string.contains(
    mutation_serialized,
    "\"body\":\"Line one\\nLine two\"",
  )
  assert !string.contains(mutation_serialized, "<br />")

  let read_body = "{\"query\":\"query { shop { shopPolicies { body } } }\"}"
  let #(proxy_state.Response(status: read_status, body: read_body_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  let read_serialized = json.to_string(read_body_json)

  assert read_status == 200
  assert string.contains(read_serialized, "\"body\":\"Line one\\nLine two\"")
  assert !string.contains(read_serialized, "<br />")
}

pub fn shop_policy_update_reuses_existing_policy_test() {
  let proxy = draft_proxy.new()
  let seeded_store =
    store.upsert_base_shop(
      proxy.store,
      make_shop([make_policy("<p>Before</p>")]),
    )
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let mutation_body =
    "{\"query\":\"mutation { shopPolicyUpdate(shopPolicy: { type: CONTACT_INFORMATION, body: \\\"<p>After</p>\\\" }) { shopPolicy { id title body type url createdAt updatedAt } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"id\":\"gid://shopify/ShopPolicy/42438689001\"",
  )
  assert string.contains(serialized, "\"createdAt\":\"2026-04-25T11:52:28Z\"")
  assert string.contains(serialized, "\"body\":\"<p>After</p>\"")
  assert !string.contains(serialized, "\"updatedAt\":\"2026-04-25T11:52:29Z\"")
}

pub fn oversized_shop_policy_body_returns_user_error_test() {
  let too_big = string.repeat("x", 524_288)
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"CONTACT_INFORMATION\",\"body\":\""
    <> too_big
    <> "\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"shopPolicy\":null")
  assert string.contains(
    serialized,
    "\"message\":\"Body is too big (maximum is 512 KB)\"",
  )
  assert string.contains(serialized, "\"code\":\"TOO_BIG\"")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn maximum_shop_policy_body_size_succeeds_test() {
  let maximum = string.repeat("x", 524_287)
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"CONTACT_INFORMATION\",\"body\":\""
    <> maximum
    <> "\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"id\":\"gid://shopify/ShopPolicy/1\"")
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "\"stagedResourceIds\":[\"gid://shopify/ShopPolicy/1\"]",
  )
}

pub fn blank_subscription_policy_body_returns_user_error_without_staging_test() {
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id type body } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"SUBSCRIPTION_POLICY\",\"body\":\"\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"shopPolicy\":null")
  assert string.contains(serialized, "\"field\":[\"shopPolicy\",\"body\"]")
  assert string.contains(
    serialized,
    "\"message\":\"Purchase options cancellation policy required\"",
  )
  assert string.contains(serialized, "\"code\":null")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"

  let read_body =
    "{\"query\":\"query { shop { shopPolicies { type body } } }\"}"
  let #(proxy_state.Response(status: read_status, body: read_body_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  let read_serialized = json.to_string(read_body_json)

  assert read_status == 200
  assert string.contains(read_serialized, "\"shopPolicies\":[]")
  assert !string.contains(read_serialized, "SUBSCRIPTION_POLICY")
}

pub fn whitespace_subscription_policy_body_returns_user_error_without_staging_test() {
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id type body } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"SUBSCRIPTION_POLICY\",\"body\":\"   \"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"shopPolicy\":null")
  assert string.contains(serialized, "\"field\":[\"shopPolicy\",\"body\"]")
  assert string.contains(
    serialized,
    "\"message\":\"Purchase options cancellation policy required\"",
  )
  assert string.contains(serialized, "\"code\":null")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"

  let read_body =
    "{\"query\":\"query { shop { shopPolicies { type body } } }\"}"
  let #(proxy_state.Response(status: read_status, body: read_body_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  let read_serialized = json.to_string(read_body_json)

  assert read_status == 200
  assert string.contains(read_serialized, "\"shopPolicies\":[]")
  assert !string.contains(read_serialized, "SUBSCRIPTION_POLICY")
}

pub fn blank_refund_policy_body_still_succeeds_test() {
  let mutation_body =
    "{\"query\":\"mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) { shopPolicyUpdate(shopPolicy: $shopPolicy) { shopPolicy { id type body } userErrors { field message code } } }\",\"variables\":{\"shopPolicy\":{\"type\":\"REFUND_POLICY\",\"body\":\"\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(seeded_proxy(), graphql_request(mutation_body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"type\":\"REFUND_POLICY\"")
  assert string.contains(serialized, "\"body\":\"\"")
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "\"stagedResourceIds\":[\"gid://shopify/ShopPolicy/1\"]",
  )
}

pub fn admin_platform_node_resolves_store_property_records_test() {
  let policy = make_policy("<p>Relay contact policy</p>")
  let proxy = draft_proxy.new()
  let seeded_store = store.upsert_base_shop(proxy.store, make_shop([policy]))
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let body =
    "{\"query\":\"query($ids: [ID!]!) { node(id: \\\"gid://shopify/ShopAddress/63755419881\\\") { __typename ... on Node { nodeId: id } ... on ShopAddress { city countryCodeV2 } } nodes(ids: $ids) { __typename ... on Node { nodeId: id } ... on ShopPolicy { title body type url } } }\",\"variables\":{\"ids\":[\"gid://shopify/ShopPolicy/42438689001\",\"gid://shopify/Unknown/1\"]}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"__typename\":\"ShopAddress\"")
  assert string.contains(serialized, "\"city\":\"Ottawa\"")
  assert string.contains(serialized, "\"__typename\":\"ShopPolicy\"")
  assert string.contains(serialized, "\"body\":\"<p>Relay contact policy</p>\"")
  assert string.contains(serialized, "null]}")
}

pub fn legacy_shop_policy_body_is_simple_formatted_for_shop_and_node_reads_test() {
  let policy = make_legacy_policy("Line one\nLine two")
  let proxy = draft_proxy.new()
  let seeded_store = store.upsert_base_shop(proxy.store, make_shop([policy]))
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let body =
    "{\"query\":\"query($ids: [ID!]!) { shop { shopPolicies { body } } nodes(ids: $ids) { __typename ... on ShopPolicy { body } } }\",\"variables\":{\"ids\":[\"gid://shopify/ShopPolicy/42438689001\"]}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"body\":\"<p>Line one<br />\\nLine two</p>\"",
  )
  assert !string.contains(serialized, "\"body\":\"Line one\\nLine two\"")
}

pub fn location_reads_and_local_mutations_use_store_state_test() {
  let location =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Main")),
      #("isActive", StorePropertyBool(True)),
      #("legacyResourceId", StorePropertyString("1")),
      #(
        "metafields",
        StorePropertyObject(
          dict.from_list([
            #("nodes", StorePropertyList([])),
            #(
              "pageInfo",
              StorePropertyObject(
                dict.from_list([
                  #("hasNextPage", StorePropertyBool(False)),
                  #("hasPreviousPage", StorePropertyBool(False)),
                  #("startCursor", StorePropertyNull),
                  #("endCursor", StorePropertyNull),
                ]),
              ),
            ),
          ]),
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, location),
    )
  let read_body =
    "{\"query\":\"query($id: ID!) { location(id: $id) { id name legacyResourceId metafields(first: 1) { nodes { id } pageInfo { hasNextPage startCursor } } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\"}}"
  let #(proxy_state.Response(status: read_status, body: read_json, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  assert read_status == 200
  assert string.contains(json.to_string(read_json), "\"name\":\"Main\"")

  let edit_body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id name } userErrors { field message } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"name\":\"Annex\"}}}"
  let #(proxy_state.Response(status: edit_status, body: edit_json, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(edit_body))
  assert edit_status == 200
  assert string.contains(json.to_string(edit_json), "\"name\":\"Annex\"")
  assert string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "\"rootFields\":[\"locationEdit\"]",
  )
  let #(proxy_state.Response(status: state_status, body: state_json, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  let serialized_state = json.to_string(state_json)
  assert state_status == 200
  assert string.contains(serialized_state, "\"stagedState\":{")
  assert string.contains(serialized_state, "\"locations\":{")
  assert string.contains(serialized_state, "\"name\":\"Annex\"")
}

pub fn location_add_requires_address_top_level_error_test() {
  let body =
    "{\"query\":\"mutation LocationAddMissingAddress { locationAdd(input: { name: \\\"Warehouse\\\" }) { location { id name } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(
    serialized,
    "\"message\":\"Argument 'address' on InputObject 'LocationAddInput' is required. Expected type LocationAddAddressInput!\"",
  )
  assert string.contains(
    serialized,
    "\"code\":\"missingRequiredInputObjectAttribute\"",
  )
  assert !string.contains(serialized, "\"locationAdd\":{\"location\"")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_variable_missing_address_returns_invalid_variable_test() {
  let body =
    "{\"query\":\"mutation($input: LocationAddInput!) { locationAdd(input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Warehouse\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(
    serialized,
    "Variable $input of type LocationAddInput! was provided invalid value for address (Expected value to not be null)",
  )
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    serialized,
    "\"problems\":[{\"path\":[\"address\"],\"explanation\":\"Expected value to not be null\"}]",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_stages_address_defaults_test() {
  let add_body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Main\\\", address: { address1: \\\"1 Spadina\\\", address2: \\\"Suite 2\\\", city: \\\"Toronto\\\", countryCode: CA, provinceCode: \\\"ON\\\", zip: \\\"M5T 2C2\\\", phone: \\\"+14165550100\\\" } }) { location { id name fulfillsOnlineOrders address { address1 address2 city countryCode provinceCode zip phone } } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: add_status, body: add_json, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(add_body))
  let add_serialized = json.to_string(add_json)

  assert add_status == 200
  assert string.contains(add_serialized, "\"userErrors\":[]")
  assert string.contains(add_serialized, "\"name\":\"Main\"")
  assert string.contains(add_serialized, "\"fulfillsOnlineOrders\":true")
  assert string.contains(add_serialized, "\"address1\":\"1 Spadina\"")
  assert string.contains(add_serialized, "\"address2\":\"Suite 2\"")
  assert string.contains(add_serialized, "\"city\":\"Toronto\"")
  assert string.contains(add_serialized, "\"countryCode\":\"CA\"")
  assert string.contains(add_serialized, "\"provinceCode\":\"ON\"")
  assert string.contains(add_serialized, "\"zip\":\"M5T 2C2\"")
  assert string.contains(add_serialized, "\"phone\":\"+14165550100\"")

  let read_body =
    "{\"query\":\"query { location { name fulfillsOnlineOrders address { countryCode city } } locations(first: 5) { nodes { name fulfillsOnlineOrders address { countryCode city } } } }\"}"
  let #(proxy_state.Response(status: read_status, body: read_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  let read_serialized = json.to_string(read_json)

  assert read_status == 200
  assert string.contains(read_serialized, "\"fulfillsOnlineOrders\":true")
  assert string.contains(
    read_serialized,
    "\"address\":{\"countryCode\":\"CA\",\"city\":\"Toronto\"}",
  )
}

pub fn location_add_honors_explicit_fulfills_online_orders_false_test() {
  let body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Sub\\\", address: { city: \\\"Toronto\\\", countryCode: CA }, fulfillsOnlineOrders: false }) { location { id fulfillsOnlineOrders address { city countryCode } } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"fulfillsOnlineOrders\":false")
  assert string.contains(
    serialized,
    "\"address\":{\"city\":\"Toronto\",\"countryCode\":\"CA\"}",
  )
}

pub fn location_add_accepts_missing_address_components_like_shopify_test() {
  let body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Partial\\\", address: { countryCode: US } }) { location { id name fulfillsOnlineOrders address { address1 city countryCode zip } } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"name\":\"Partial\"")
  assert string.contains(serialized, "\"fulfillsOnlineOrders\":true")
  assert string.contains(serialized, "\"address1\":null")
  assert string.contains(serialized, "\"city\":null")
  assert string.contains(serialized, "\"countryCode\":\"US\"")
  assert string.contains(serialized, "\"zip\":null")
}

pub fn location_add_blank_name_user_error_includes_code_test() {
  let body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"\\\", address: { countryCode: CA } }) { location { id } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"location\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"name\"],\"message\":\"Add a location name\",\"code\":\"BLANK\"",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_rejects_duplicate_and_too_long_fields_without_staging_test() {
  let existing =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Existing")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(False)),
      #(
        "address",
        location_address(
          "1 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, existing),
    )
  let duplicate_body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Existing\\\", fulfillsOnlineOrders: false, address: { address1: \\\"2 Test St\\\", city: \\\"Boston\\\", countryCode: US, provinceCode: \\\"MA\\\", zip: \\\"02111\\\" } }) { location { id name } userErrors { field message code } } }\"}"
  let #(
    proxy_state.Response(status: duplicate_status, body: duplicate_json, ..),
    duplicate_proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(duplicate_body))
  let duplicate = json.to_string(duplicate_json)

  assert duplicate_status == 200
  assert duplicate
    == "{\"data\":{\"locationAdd\":{\"location\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"You already have a location with this name\",\"code\":\"TAKEN\"}]}}}"
  assert json.to_string(draft_proxy.get_log_snapshot(duplicate_proxy))
    == "{\"entries\":[]}"
  assert !string.contains(
    json.to_string(draft_proxy.get_state_snapshot(duplicate_proxy)),
    "\"2 Test St\"",
  )

  let long = string.repeat("A", 256)
  let too_long_body =
    "{\"query\":\"mutation($input: LocationAddInput!) { locationAdd(input: $input) { location { id name } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"New\",\"fulfillsOnlineOrders\":false,\"address\":{\"address1\":\""
    <> long
    <> "\",\"city\":\"Boston\",\"countryCode\":\"US\",\"zip\":\"02112\"}}}}"
  let #(
    proxy_state.Response(status: too_long_status, body: too_long_json, ..),
    too_long_proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(too_long_body))
  let too_long = json.to_string(too_long_json)

  assert too_long_status == 200
  assert too_long
    == "{\"data\":{\"locationAdd\":{\"location\":null,\"userErrors\":[{\"field\":[\"input\",\"address\",\"address1\"],\"message\":\"Use a shorter name for the street (up to 255 characters)\",\"code\":\"TOO_LONG\"}]}}}"
  assert json.to_string(draft_proxy.get_log_snapshot(too_long_proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_rejects_too_long_name_and_zip_test() {
  let long_name = string.repeat("N", 101)
  let long_zip = string.repeat("9", 256)
  let body =
    "{\"query\":\"mutation($name: String!, $zip: String!) { tooLongName: locationAdd(input: { name: $name, fulfillsOnlineOrders: false, address: { address1: \\\"1 Test St\\\", city: \\\"Boston\\\", countryCode: US, zip: \\\"02110\\\" } }) { location { id } userErrors { field message code } } tooLongZip: locationAdd(input: { name: \\\"Zip\\\", fulfillsOnlineOrders: false, address: { address1: \\\"1 Test St\\\", city: \\\"Boston\\\", countryCode: US, zip: $zip } }) { location { id } userErrors { field message code } } }\",\"variables\":{\"name\":\""
    <> long_name
    <> "\",\"zip\":\""
    <> long_zip
    <> "\"}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"name\"],\"message\":\"Use a shorter location name (up to 100 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"address\",\"zip\"],\"message\":\"Use a shorter postal / ZIP code (up to 255 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_variable_missing_country_code_returns_invalid_variable_test() {
  let body =
    "{\"query\":\"mutation($input: LocationAddInput!) { locationAdd(input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Bad\",\"address\":{\"address1\":\"1 Infinite Loop\"}}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(
    serialized,
    "Variable $input of type LocationAddInput! was provided invalid value for address.countryCode (Expected value to not be null)",
  )
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    serialized,
    "\"problems\":[{\"path\":[\"address\",\"countryCode\"],\"explanation\":\"Expected value to not be null\"}]",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_inline_missing_country_code_returns_top_level_error_test() {
  let body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Bad\\\", address: { address1: \\\"1 Infinite Loop\\\" } }) { location { id } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(
    serialized,
    "\"message\":\"Argument 'countryCode' on InputObject 'LocationAddAddressInput' is required. Expected type CountryCode!\"",
  )
  assert string.contains(
    serialized,
    "\"code\":\"missingRequiredInputObjectAttribute\"",
  )
  assert string.contains(
    serialized,
    "\"path\":[\"mutation\",\"locationAdd\",\"input\",\"address\",\"countryCode\"]",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_accepts_shopify_country_code_zz_test() {
  let body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"ZZ\\\", address: { countryCode: ZZ }, fulfillsOnlineOrders: false }) { location { id name fulfillsOnlineOrders address { countryCode } } userErrors { field message code } } }\"}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"name\":\"ZZ\"")
  assert string.contains(serialized, "\"fulfillsOnlineOrders\":false")
  assert string.contains(serialized, "\"countryCode\":\"ZZ\"")
}

pub fn location_add_invalid_country_code_returns_invalid_variable_test() {
  let body =
    "{\"query\":\"mutation($input: LocationAddInput!) { locationAdd(input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Bad\",\"address\":{\"countryCode\":\"QQ\"}}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "Expected \\\"QQ\\\" to be one of:")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_add_rejects_capabilities_inputs_not_in_public_schema_test() {
  let inline_body =
    "{\"query\":\"mutation { locationAdd(input: { name: \\\"Cap\\\", address: { countryCode: CA }, capabilitiesToAdd: [PICKUP] }) { location { id } userErrors { field message code } } }\"}"
  let #(
    proxy_state.Response(status: inline_status, body: inline_json, ..),
    inline_proxy,
  ) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(inline_body))
  let inline_serialized = json.to_string(inline_json)

  assert inline_status == 200
  assert string.contains(inline_serialized, "\"errors\":[")
  assert string.contains(
    inline_serialized,
    "InputObject 'LocationAddInput' doesn't accept argument 'capabilitiesToAdd'",
  )
  assert string.contains(inline_serialized, "\"code\":\"argumentNotAccepted\"")
  assert json.to_string(draft_proxy.get_log_snapshot(inline_proxy))
    == "{\"entries\":[]}"

  let variable_body =
    "{\"query\":\"mutation($input: LocationAddInput!) { locationAdd(input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Cap\",\"address\":{\"countryCode\":\"CA\"},\"capabilities\":{\"pickupEnabled\":true}}}}"
  let #(
    proxy_state.Response(status: variable_status, body: variable_json, ..),
    variable_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(variable_body),
    )
  let variable_serialized = json.to_string(variable_json)

  assert variable_status == 200
  assert string.contains(variable_serialized, "\"errors\":[")
  assert string.contains(
    variable_serialized,
    "Field is not defined on LocationAddInput",
  )
  assert string.contains(variable_serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert json.to_string(draft_proxy.get_log_snapshot(variable_proxy))
    == "{\"entries\":[]}"
}

pub fn location_edit_updates_address_fulfillment_and_metafields_test() {
  let target =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Main")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(True)),
      #(
        "address",
        location_address(
          "1 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let backup =
    make_raw_record("gid://shopify/Location/2", "Location", [
      #("name", StorePropertyString("Backup")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(True)),
      #(
        "address",
        location_address(
          "2 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let store =
    proxy.store
    |> store.upsert_base_store_property_location(target)
    |> store.upsert_base_store_property_location(backup)
  let proxy = proxy_state.DraftProxy(..proxy, store: store)
  let edit_body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id name fulfillsOnlineOrders address { address1 city country countryCode provinceCode zip } metafield(namespace: \\\"custom\\\", key: \\\"x\\\") { id namespace key type value } metafields(first: 5) { nodes { namespace key value type } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"name\":\"Annex\",\"address\":{\"city\":\"Toronto\",\"countryCode\":\"CA\",\"provinceCode\":\"ON\",\"zip\":\"M5T 2C2\"},\"fulfillsOnlineOrders\":false,\"metafields\":[{\"namespace\":\"custom\",\"key\":\"x\",\"value\":\"1\",\"type\":\"single_line_text_field\"}]}}}"
  let #(proxy_state.Response(status: edit_status, body: edit_json, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(edit_body))
  let edited = json.to_string(edit_json)
  assert edit_status == 200
  assert string.contains(edited, "\"name\":\"Annex\"")
  assert string.contains(edited, "\"fulfillsOnlineOrders\":false")
  assert string.contains(edited, "\"city\":\"Toronto\"")
  assert string.contains(edited, "\"country\":\"Canada\"")
  assert string.contains(edited, "\"namespace\":\"custom\"")
  assert string.contains(edited, "\"key\":\"x\"")
  assert string.contains(edited, "\"value\":\"1\"")
  assert string.contains(edited, "\"userErrors\":[]")

  let read_body =
    "{\"query\":\"query($id: ID!) { location(id: $id) { id name fulfillsOnlineOrders address { city countryCode } metafield(namespace: \\\"custom\\\", key: \\\"x\\\") { namespace key value type } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\"}}"
  let #(proxy_state.Response(status: read_status, body: read_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  let read = json.to_string(read_json)
  assert read_status == 200
  assert string.contains(read, "\"name\":\"Annex\"")
  assert string.contains(read, "\"fulfillsOnlineOrders\":false")
  assert string.contains(read, "\"city\":\"Toronto\"")
  assert string.contains(read, "\"value\":\"1\"")
}

pub fn location_edit_rejects_duplicate_name_without_staging_test() {
  let target =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Target")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(False)),
      #(
        "address",
        location_address(
          "1 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let other =
    make_raw_record("gid://shopify/Location/2", "Location", [
      #("name", StorePropertyString("Existing")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(False)),
      #(
        "address",
        location_address(
          "2 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02111",
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let store =
    proxy.store
    |> store.upsert_base_store_property_location(target)
    |> store.upsert_base_store_property_location(other)
  let proxy = proxy_state.DraftProxy(..proxy, store: store)
  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id name } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"name\":\"Existing\"}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert serialized
    == "{\"data\":{\"locationEdit\":{\"location\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"You already have a location with this name\",\"code\":\"TAKEN\"}]}}}"
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
  assert !string.contains(
    json.to_string(draft_proxy.get_state_snapshot(proxy)),
    "\"name\":\"Existing\",\"isActive\":true,\"fulfillsOnlineOrders\":false,\"address\":{\"address1\":\"1 Test St\"",
  )
}

pub fn location_edit_rejects_too_long_name_city_and_zip_test() {
  let target =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Target")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(False)),
      #(
        "address",
        location_address(
          "1 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, target),
    )
  let long_name = string.repeat("N", 101)
  let long_city = string.repeat("C", 256)
  let long_zip = string.repeat("9", 256)
  let body =
    "{\"query\":\"mutation($id: ID!, $name: String!, $city: String!, $zip: String!) { tooLongName: locationEdit(id: $id, input: { name: $name }) { location { id } userErrors { field message code } } tooLongCity: locationEdit(id: $id, input: { address: { city: $city } }) { location { id } userErrors { field message code } } tooLongZip: locationEdit(id: $id, input: { address: { zip: $zip } }) { location { id } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"name\":\""
    <> long_name
    <> "\",\"city\":\""
    <> long_city
    <> "\",\"zip\":\""
    <> long_zip
    <> "\"}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"name\"],\"message\":\"Use a shorter location name (up to 100 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"address\",\"city\"],\"message\":\"Use a shorter city name (up to 255 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"address\",\"zip\"],\"message\":\"Use a shorter postal / ZIP code (up to 255 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn location_edit_invalid_metafield_type_returns_typed_error_test() {
  let location =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Main")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(True)),
      #(
        "address",
        location_address(
          "1 Test St",
          "Boston",
          "United States",
          "US",
          "MA",
          "02110",
        ),
      ),
    ])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, location),
    )
  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"metafields\":[{\"namespace\":\"custom\",\"key\":\"bad\",\"value\":\"1\",\"type\":\"not_a_real_type\"}]}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)
  assert status == 200
  assert string.contains(serialized, "\"location\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"metafields\",\"0\",\"type\"]",
  )
  assert string.contains(serialized, "\"code\":\"INVALID_TYPE\"")
  assert !string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "locationEdit",
  )
}

pub fn location_edit_invalid_country_code_returns_invalid_variable_error_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"address\":{\"countryCode\":\"XX\"}}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)
  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(serialized, "\"path\":[\"address\",\"countryCode\"]")
  assert string.contains(serialized, "Expected \\\"XX\\\" to be one of:")
  assert !string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "locationEdit",
  )
}

pub fn location_edit_rejects_disabling_only_online_location_test() {
  let location =
    make_location("gid://shopify/Location/1", "Only", True, False, True, True)
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, location),
    )
  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id fulfillsOnlineOrders } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"fulfillsOnlineOrders\":false}}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)
  assert status == 200
  assert string.contains(serialized, "\"location\":null")
  assert string.contains(
    serialized,
    "\"code\":\"CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT\"",
  )
  assert !string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "locationEdit",
  )
}

pub fn location_edit_rejects_modeled_pending_and_delivery_profile_blockers_test() {
  let pending =
    make_raw_record("gid://shopify/Location/1", "Location", [
      #("name", StorePropertyString("Pending")),
      #("isActive", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(True)),
      #("hasUnfulfilledOrders", StorePropertyBool(True)),
    ])
  let delivery_profile_location =
    make_location(
      "gid://shopify/Location/2",
      "Profile",
      True,
      False,
      True,
      True,
    )
  let backup =
    make_location("gid://shopify/Location/3", "Backup", True, False, True, True)
  let profile =
    DeliveryProfileRecord(
      id: "gid://shopify/DeliveryProfile/1",
      cursor: None,
      merchant_owned: True,
      data: CapturedObject([
        #(
          "profileLocationGroups",
          CapturedArray([
            CapturedObject([
              #(
                "locationGroup",
                CapturedObject([
                  #(
                    "locations",
                    CapturedArray([
                      CapturedObject([
                        #("id", CapturedString("gid://shopify/Location/2")),
                      ]),
                    ]),
                  ),
                ]),
              ),
            ]),
          ]),
        ),
      ]),
    )
  let proxy = draft_proxy.new()
  let seeded =
    proxy.store
    |> store.upsert_base_store_property_location(pending)
    |> store.upsert_base_store_property_location(delivery_profile_location)
    |> store.upsert_base_store_property_location(backup)
    |> store.upsert_base_delivery_profiles([profile])
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded)
  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id fulfillsOnlineOrders } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/1\",\"input\":{\"fulfillsOnlineOrders\":false}}}"
  let #(
    proxy_state.Response(status: pending_status, body: pending_json, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(body))
  assert pending_status == 200
  assert string.contains(
    json.to_string(pending_json),
    "\"code\":\"CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT\"",
  )

  let body =
    "{\"query\":\"mutation($id: ID!, $input: LocationEditInput!) { locationEdit(id: $id, input: $input) { location { id fulfillsOnlineOrders } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/Location/2\",\"input\":{\"fulfillsOnlineOrders\":false}}}"
  let #(proxy_state.Response(status: profile_status, body: profile_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert profile_status == 200
  assert string.contains(
    json.to_string(profile_json),
    "\"code\":\"CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT\"",
  )
}

pub fn location_activate_deactivate_stage_and_read_back_test() {
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(make_location(
      "gid://shopify/Location/1",
      "Alpha Warehouse",
      True,
      False,
      True,
      True,
    ))
    |> store.upsert_base_store_property_location(make_location(
      "gid://shopify/Location/2",
      "Beta Warehouse",
      True,
      False,
      True,
      False,
    ))
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)

  let deactivate_body =
    "{\"query\":\"mutation { locationDeactivate(locationId: \\\"gid://shopify/Location/2\\\") @idempotent(key: \\\"deactivate-beta\\\") { location { id isActive activatable deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory shipsInventory } locationDeactivateUserErrors { field code message } } }\"}"
  let #(
    proxy_state.Response(status: deactivate_status, body: deactivate_json, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(deactivate_body))
  let deactivate_serialized = json.to_string(deactivate_json)
  assert deactivate_status == 200
  assert string.contains(deactivate_serialized, "\"isActive\":false")
  assert string.contains(deactivate_serialized, "\"activatable\":true")
  assert string.contains(deactivate_serialized, "\"deactivatable\":true")
  assert string.contains(deactivate_serialized, "\"deletable\":true")
  assert string.contains(
    deactivate_serialized,
    "\"fulfillsOnlineOrders\":false",
  )
  assert string.contains(deactivate_serialized, "\"hasActiveInventory\":false")
  assert string.contains(deactivate_serialized, "\"shipsInventory\":false")
  assert string.contains(
    deactivate_serialized,
    "\"locationDeactivateUserErrors\":[]",
  )

  let read_deactivated_body =
    "{\"query\":\"query($id: ID!) { location(id: $id) { id isActive activatable deactivatable } locations(first: 5) { nodes { id isActive activatable deactivatable } } }\",\"variables\":{\"id\":\"gid://shopify/Location/2\"}}"
  let #(proxy_state.Response(status: read_status, body: read_json, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_deactivated_body))
  let read_serialized = json.to_string(read_json)
  assert read_status == 200
  assert string.contains(
    read_serialized,
    "\"location\":{\"id\":\"gid://shopify/Location/2\",\"isActive\":false,\"activatable\":true,\"deactivatable\":true}",
  )
  assert string.contains(read_serialized, "\"locations\":{\"nodes\":[")

  let activate_body =
    "{\"query\":\"mutation { locationActivate(locationId: \\\"gid://shopify/Location/2\\\") @idempotent(key: \\\"activate-beta\\\") { location { id isActive activatable deactivatable deactivatedAt deletable shipsInventory } locationActivateUserErrors { field code message } } }\"}"
  let #(
    proxy_state.Response(status: activate_status, body: activate_json, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(activate_body))
  let activate_serialized = json.to_string(activate_json)
  assert activate_status == 200
  assert string.contains(activate_serialized, "\"isActive\":true")
  assert string.contains(activate_serialized, "\"activatable\":true")
  assert string.contains(activate_serialized, "\"deactivatable\":true")
  assert string.contains(activate_serialized, "\"deactivatedAt\":null")
  assert string.contains(activate_serialized, "\"deletable\":false")
  assert string.contains(activate_serialized, "\"shipsInventory\":false")
  assert string.contains(
    activate_serialized,
    "\"locationActivateUserErrors\":[]",
  )

  let #(proxy_state.Response(status: final_status, body: final_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_deactivated_body))
  assert final_status == 200
  assert string.contains(
    json.to_string(final_json),
    "\"location\":{\"id\":\"gid://shopify/Location/2\",\"isActive\":true,\"activatable\":true,\"deactivatable\":true}",
  )
}

pub fn location_lifecycle_missing_idempotency_is_version_gated_test() {
  let location =
    make_location(
      "gid://shopify/Location/1",
      "Alpha Warehouse",
      False,
      True,
      False,
      False,
    )
  let active_location =
    make_location(
      "gid://shopify/Location/2",
      "Beta Warehouse",
      True,
      True,
      True,
      False,
    )
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(location)
    |> store.upsert_base_store_property_location(active_location)
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let body =
    "{\"query\":\"mutation { locationActivate(locationId: \\\"gid://shopify/Location/1\\\") { location { id isActive } locationActivateUserErrors { field code message } } }\"}"

  let #(proxy_state.Response(body: required_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version("2026-04", body),
    )
  let required_serialized = json.to_string(required_json)
  assert string.contains(required_serialized, "\"code\":\"BAD_REQUEST\"")
  assert string.contains(required_serialized, "\"locationActivate\":null")

  let #(proxy_state.Response(body: optional_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version("2026-01", body),
    )
  let optional_serialized = json.to_string(optional_json)
  assert !string.contains(optional_serialized, "\"code\":\"BAD_REQUEST\"")
  assert string.contains(optional_serialized, "\"isActive\":true")
  assert string.contains(
    optional_serialized,
    "\"locationActivateUserErrors\":[]",
  )

  let deactivate_body =
    "{\"query\":\"mutation { locationDeactivate(locationId: \\\"gid://shopify/Location/2\\\") { location { id isActive } locationDeactivateUserErrors { field code message } } }\"}"

  let #(proxy_state.Response(body: required_deactivate_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version("2026-04", deactivate_body),
    )
  let required_deactivate_serialized = json.to_string(required_deactivate_json)
  assert string.contains(
    required_deactivate_serialized,
    "\"code\":\"BAD_REQUEST\"",
  )
  assert string.contains(
    required_deactivate_serialized,
    "\"locationDeactivate\":null",
  )

  let #(proxy_state.Response(body: optional_deactivate_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version("2026-01", deactivate_body),
    )
  let optional_deactivate_serialized = json.to_string(optional_deactivate_json)
  assert !string.contains(
    optional_deactivate_serialized,
    "\"code\":\"BAD_REQUEST\"",
  )
  assert string.contains(optional_deactivate_serialized, "\"isActive\":false")
  assert string.contains(
    optional_deactivate_serialized,
    "\"locationDeactivateUserErrors\":[]",
  )
}

pub fn location_activate_non_activatable_returns_user_error_test() {
  let location =
    make_location(
      "gid://shopify/Location/1",
      "Alpha Warehouse",
      False,
      False,
      False,
      False,
    )
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_store_property_location(proxy.store, location),
    )
  let body =
    "{\"query\":\"mutation { locationActivate(locationId: \\\"gid://shopify/Location/1\\\") @idempotent(key: \\\"activate-alpha\\\") { location { id isActive } locationActivateUserErrors { field code message } } }\"}"
  let #(proxy_state.Response(status: status, body: response_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_json)

  assert status == 200
  assert string.contains(serialized, "\"isActive\":false")
  assert string.contains(serialized, "\"field\":[\"locationId\"]")
  assert string.contains(serialized, "\"code\":\"GENERIC_ERROR\"")
  assert string.contains(
    serialized,
    "\"message\":\"Location cannot be activated.\"",
  )
}

pub fn location_activate_limit_and_relocation_guards_block_staging_test() {
  let limit_id = "gid://shopify/Location/activate-limit"
  let alternate_limit_id = "gid://shopify/Location/activate-alternate-limit"
  let nested_limit_id = "gid://shopify/Location/activate-nested-limit"
  let relocation_id = "gid://shopify/Location/activate-relocation"
  let duplicate_id = "gid://shopify/Location/activate-duplicate"
  let active_duplicate_id = "gid://shopify/Location/activate-active-duplicate"
  let success_id = "gid://shopify/Location/activate-success"
  let active_duplicate =
    make_location(
      active_duplicate_id,
      "Duplicate Warehouse",
      True,
      True,
      True,
      False,
    )
  let limit_location =
    location_with_extra_bool(
      limit_id,
      "Duplicate Warehouse",
      "reachedLocationLimit",
      True,
    )
  let alternate_limit_location =
    location_with_extra_bool(
      alternate_limit_id,
      "Alternate Limit Warehouse",
      "locationLimitReached",
      True,
    )
  let nested_limit_location =
    StorePropertyRecord(
      ..make_location(
        nested_limit_id,
        "Nested Limit Warehouse",
        False,
        True,
        True,
        False,
      ),
      data: make_location(
          nested_limit_id,
          "Nested Limit Warehouse",
          False,
          True,
          True,
          False,
        ).data
        |> dict.insert(
          "shop",
          StorePropertyObject(
            dict.from_list([
              #(
                "resourceLimits",
                StorePropertyObject(
                  dict.from_list([
                    #("locationLimitReached", StorePropertyBool(True)),
                  ]),
                ),
              ),
            ]),
          ),
        ),
    )
  let relocation_location =
    location_with_extra_bool(
      relocation_id,
      "Duplicate Warehouse",
      "hasIncompleteMassRelocation",
      True,
    )
  let duplicate_location =
    make_location(duplicate_id, "Duplicate Warehouse", False, True, True, False)
  let success_location =
    make_location(success_id, "Success Warehouse", False, True, True, False)
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(active_duplicate)
    |> store.upsert_base_store_property_location(limit_location)
    |> store.upsert_base_store_property_location(alternate_limit_location)
    |> store.upsert_base_store_property_location(nested_limit_location)
    |> store.upsert_base_store_property_location(relocation_location)
    |> store.upsert_base_store_property_location(duplicate_location)
    |> store.upsert_base_store_property_location(success_location)
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let mutation_prefix =
    "{\"query\":\"mutation { locationActivate(locationId: \\\""
  let mutation_suffix =
    "\\\") @idempotent(key: \\\"activate-guard\\\") { location { id isActive } locationActivateUserErrors { field code message } } }\"}"

  let #(proxy_state.Response(body: limit_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> limit_id <> mutation_suffix),
    )
  let limit_serialized = json.to_string(limit_json)
  assert string.contains(limit_serialized, "\"isActive\":false")
  assert string.contains(limit_serialized, "\"field\":[\"locationId\"]")
  assert string.contains(limit_serialized, "\"code\":\"LOCATION_LIMIT\"")
  assert !string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "activate-limit",
  )

  let #(proxy_state.Response(body: alternate_limit_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> alternate_limit_id <> mutation_suffix),
    )
  let alternate_limit_serialized = json.to_string(alternate_limit_json)
  assert string.contains(alternate_limit_serialized, "\"isActive\":false")
  assert string.contains(
    alternate_limit_serialized,
    "\"code\":\"LOCATION_LIMIT\"",
  )

  let #(proxy_state.Response(body: nested_limit_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> nested_limit_id <> mutation_suffix),
    )
  let nested_limit_serialized = json.to_string(nested_limit_json)
  assert string.contains(nested_limit_serialized, "\"isActive\":false")
  assert string.contains(nested_limit_serialized, "\"code\":\"LOCATION_LIMIT\"")

  let #(proxy_state.Response(body: relocation_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> relocation_id <> mutation_suffix),
    )
  let relocation_serialized = json.to_string(relocation_json)
  assert string.contains(relocation_serialized, "\"isActive\":false")
  assert string.contains(
    relocation_serialized,
    "\"code\":\"HAS_ONGOING_RELOCATION\"",
  )

  let #(proxy_state.Response(body: duplicate_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> duplicate_id <> mutation_suffix),
    )
  let duplicate_serialized = json.to_string(duplicate_json)
  assert string.contains(duplicate_serialized, "\"isActive\":false")
  assert string.contains(
    duplicate_serialized,
    "\"code\":\"HAS_NON_UNIQUE_NAME\"",
  )

  let #(proxy_state.Response(body: success_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> success_id <> mutation_suffix),
    )
  let success_serialized = json.to_string(success_json)
  assert string.contains(success_serialized, "\"isActive\":true")
  assert string.contains(
    success_serialized,
    "\"locationActivateUserErrors\":[]",
  )
  assert string.contains(
    json.to_string(draft_proxy.get_log_snapshot(proxy)),
    "activate-success",
  )
}

pub fn location_deactivate_only_online_fulfilling_location_returns_user_error_test() {
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(make_location(
      "gid://shopify/Location/1",
      "Alpha Warehouse",
      True,
      False,
      True,
      True,
    ))
    |> store.upsert_base_store_property_location(make_location(
      "gid://shopify/Location/2",
      "Beta Warehouse",
      True,
      False,
      True,
      False,
    ))
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let body =
    "{\"query\":\"mutation { locationDeactivate(locationId: \\\"gid://shopify/Location/1\\\") @idempotent(key: \\\"deactivate-alpha\\\") { location { id isActive } locationDeactivateUserErrors { field code message } } }\"}"
  let #(proxy_state.Response(status: status, body: response_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_json)

  assert status == 200
  assert string.contains(serialized, "\"isActive\":true")
  assert string.contains(serialized, "\"field\":[\"locationId\"]")
  assert string.contains(
    serialized,
    "\"code\":\"CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT\"",
  )
}

pub fn location_deactivate_validates_destination_location_test() {
  let source_id = "gid://shopify/Location/deactivate-source"
  let inactive_destination_id = "gid://shopify/Location/deactivate-inactive"
  let fulfillment_service_id = "gid://shopify/Location/deactivate-fs"
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(make_location(
      source_id,
      "Deactivate source",
      True,
      False,
      True,
      False,
    ))
    |> store.upsert_base_store_property_location(make_location(
      inactive_destination_id,
      "Inactive destination",
      False,
      True,
      True,
      False,
    ))
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          fulfillment_service_id,
          "Fulfillment-service destination",
          True,
          False,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            fulfillment_service_id,
            "Fulfillment-service destination",
            True,
            False,
            True,
            False,
          ).data,
          "isFulfillmentService",
          StorePropertyBool(True),
        ),
      ),
    )
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let mutation_prefix =
    "{\"query\":\"mutation($destinationLocationId: ID!) { locationDeactivate(locationId: \\\""
    <> source_id
    <> "\\\", destinationLocationId: $destinationLocationId) @idempotent(key: \\\"deactivate-destination\\\") { location { id isActive } locationDeactivateUserErrors { field code message } } }\",\"variables\":{\"destinationLocationId\":\""
  let mutation_suffix = "\"}}"

  let #(proxy_state.Response(body: same_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> source_id <> mutation_suffix),
    )
  let same_serialized = json.to_string(same_json)
  assert string.contains(same_serialized, "\"isActive\":true")
  assert string.contains(
    same_serialized,
    "\"field\":[\"destinationLocationId\"]",
  )
  assert string.contains(
    same_serialized,
    "\"code\":\"DESTINATION_LOCATION_IS_THE_SAME_LOCATION\"",
  )

  let #(proxy_state.Response(body: inactive_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        mutation_prefix <> inactive_destination_id <> mutation_suffix,
      ),
    )
  let inactive_serialized = json.to_string(inactive_json)
  assert string.contains(inactive_serialized, "\"isActive\":true")
  assert string.contains(
    inactive_serialized,
    "\"code\":\"DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE\"",
  )

  let #(proxy_state.Response(body: missing_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        mutation_prefix <> "gid://shopify/Location/missing" <> mutation_suffix,
      ),
    )
  assert string.contains(
    json.to_string(missing_json),
    "\"code\":\"DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE\"",
  )

  let #(proxy_state.Response(body: service_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        mutation_prefix <> fulfillment_service_id <> mutation_suffix,
      ),
    )
  assert string.contains(
    json.to_string(service_json),
    "\"code\":\"DESTINATION_LOCATION_NOT_SHOPIFY_MANAGED\"",
  )
}

pub fn location_deactivate_uses_source_state_machine_guards_test() {
  let inventory_id = "gid://shopify/Location/deactivate-inventory"
  let purchase_order_id = "gid://shopify/Location/deactivate-purchase-order"
  let transfer_id = "gid://shopify/Location/deactivate-transfer"
  let permanent_id = "gid://shopify/Location/deactivate-permanent"
  let temporary_id = "gid://shopify/Location/deactivate-temporary"
  let retail_id = "gid://shopify/Location/deactivate-retail"
  let external_id = "gid://shopify/Location/deactivate-external"
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(location_with_bool(
      inventory_id,
      "hasActiveInventory",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      purchase_order_id,
      "hasOpenPurchaseOrders",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      transfer_id,
      "hasActiveTransfers",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      permanent_id,
      "permanentlyBlockedFromDeactivation",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      temporary_id,
      "temporarilyBlockedFromDeactivation",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      retail_id,
      "hasActiveRetailSubscription",
      True,
    ))
    |> store.upsert_base_store_property_location(location_with_bool(
      external_id,
      "hasIncomingFromExternalDocumentSources",
      True,
    ))
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let mutation_prefix =
    "{\"query\":\"mutation { locationDeactivate(locationId: \\\""
  let mutation_suffix =
    "\\\") @idempotent(key: \\\"deactivate-guard\\\") { location { id isActive hasActiveInventory } locationDeactivateUserErrors { field code message } } }\"}"

  let #(proxy_state.Response(body: inventory_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> inventory_id <> mutation_suffix),
    )
  let inventory_serialized = json.to_string(inventory_json)
  assert string.contains(inventory_serialized, "\"isActive\":true")
  assert string.contains(
    inventory_serialized,
    "\"code\":\"HAS_ACTIVE_INVENTORY_ERROR\"",
  )

  let #(proxy_state.Response(body: purchase_order_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> purchase_order_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(purchase_order_json),
    "\"code\":\"HAS_OPEN_PURCHASE_ORDERS_ERROR\"",
  )

  let #(proxy_state.Response(body: transfer_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> transfer_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(transfer_json),
    "\"code\":\"HAS_ACTIVE_TRANSFERS_ERROR\"",
  )

  let #(proxy_state.Response(body: permanent_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> permanent_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(permanent_json),
    "\"code\":\"PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR\"",
  )

  let #(proxy_state.Response(body: temporary_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> temporary_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(temporary_json),
    "\"code\":\"TEMPORARILY_BLOCKED_FROM_DEACTIVATION_ERROR\"",
  )

  let #(proxy_state.Response(body: retail_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> retail_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(retail_json),
    "\"code\":\"HAS_ACTIVE_RETAIL_SUBSCRIPTIONS\"",
  )

  let #(proxy_state.Response(body: external_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(mutation_prefix <> external_id <> mutation_suffix),
    )
  assert string.contains(
    json.to_string(external_json),
    "\"code\":\"HAS_INCOMING_FROM_EXTERNAL_DOCUMENT_SOURCES\"",
  )
}

fn location_with_bool(
  id: String,
  field: String,
  value: Bool,
) -> StorePropertyRecord {
  let location = make_location(id, id, True, False, True, False)
  StorePropertyRecord(
    ..location,
    data: dict.insert(location.data, field, StorePropertyBool(value)),
  )
}

fn location_with_extra_bool(
  id: String,
  name: String,
  field: String,
  value: Bool,
) -> StorePropertyRecord {
  let location = make_location(id, name, False, True, True, False)
  StorePropertyRecord(
    ..location,
    data: dict.insert(location.data, field, StorePropertyBool(value)),
  )
}

pub fn location_delete_uses_location_state_guards_and_read_back_test() {
  let success_id = "gid://shopify/Location/delete-success"
  let active_id = "gid://shopify/Location/delete-active"
  let inventory_id = "gid://shopify/Location/delete-inventory"
  let active_inventory_id = "gid://shopify/Location/delete-active-inventory"
  let primary_id = "gid://shopify/Location/delete-primary"
  let fulfillment_service_id = "gid://shopify/Location/delete-fs"
  let pending_orders_id = "gid://shopify/Location/delete-pending"
  let retail_subscription_id = "gid://shopify/Location/delete-retail"
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_store_property_location(make_location(
      success_id,
      "Delete success",
      False,
      True,
      True,
      False,
    ))
    |> store.upsert_base_store_property_location(make_location(
      active_id,
      "Delete active",
      True,
      True,
      True,
      False,
    ))
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          inventory_id,
          "Delete inventory",
          False,
          True,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            inventory_id,
            "Delete inventory",
            False,
            True,
            True,
            False,
          ).data,
          "inventoryLevels",
          location_inventory_levels("available", 4),
        ),
      ),
    )
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          active_inventory_id,
          "Delete active inventory",
          True,
          True,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            active_inventory_id,
            "Delete active inventory",
            True,
            True,
            True,
            False,
          ).data,
          "hasActiveInventory",
          StorePropertyBool(True),
        ),
      ),
    )
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(primary_id, "Delete primary", False, True, True, False),
        data: dict.insert(
          make_location(primary_id, "Delete primary", False, True, True, False).data,
          "isPrimary",
          StorePropertyBool(True),
        ),
      ),
    )
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          fulfillment_service_id,
          "Delete fulfillment service",
          False,
          True,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            fulfillment_service_id,
            "Delete fulfillment service",
            False,
            True,
            True,
            False,
          ).data,
          "isFulfillmentService",
          StorePropertyBool(True),
        ),
      ),
    )
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          pending_orders_id,
          "Delete pending orders",
          False,
          True,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            pending_orders_id,
            "Delete pending orders",
            False,
            True,
            True,
            False,
          ).data,
          "hasUnfulfilledOrders",
          StorePropertyBool(True),
        ),
      ),
    )
    |> store.upsert_base_store_property_location(
      StorePropertyRecord(
        ..make_location(
          retail_subscription_id,
          "Delete retail subscription",
          False,
          True,
          True,
          False,
        ),
        data: dict.insert(
          make_location(
            retail_subscription_id,
            "Delete retail subscription",
            False,
            True,
            True,
            False,
          ).data,
          "hasActiveRetailSubscription",
          StorePropertyBool(True),
        ),
      ),
    )
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)

  let delete_body =
    "{\"query\":\"mutation($locationId: ID!) { locationDelete(locationId: $locationId) { deletedLocationId locationDeleteUserErrors { field code message } } }\",\"variables\":{\"locationId\":\""
  let read_body =
    "{\"query\":\"query { locations(first: 20) { nodes { id } } }\"}"

  let #(
    proxy_state.Response(status: success_status, body: success_json, ..),
    proxy,
  ) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> success_id <> "\"}}"),
    )
  assert success_status == 200
  assert string.contains(
    json.to_string(success_json),
    "\"deletedLocationId\":\"" <> success_id <> "\"",
  )
  assert string.contains(
    json.to_string(success_json),
    "\"locationDeleteUserErrors\":[]",
  )

  let #(proxy_state.Response(status: read_status, body: read_json, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  assert read_status == 200
  assert !string.contains(json.to_string(read_json), success_id)

  let #(proxy_state.Response(body: active_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> active_id <> "\"}}"),
    )
  let active_serialized = json.to_string(active_json)
  assert string.contains(active_serialized, "\"code\":\"LOCATION_IS_ACTIVE\"")
  assert !string.contains(
    active_serialized,
    "\"code\":\"LOCATION_HAS_INVENTORY\"",
  )

  let #(proxy_state.Response(body: inventory_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> inventory_id <> "\"}}"),
    )
  let inventory_serialized = json.to_string(inventory_json)
  assert !string.contains(
    inventory_serialized,
    "\"code\":\"LOCATION_IS_ACTIVE\"",
  )
  assert string.contains(
    inventory_serialized,
    "\"code\":\"LOCATION_HAS_INVENTORY\"",
  )

  let #(proxy_state.Response(body: active_inventory_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> active_inventory_id <> "\"}}"),
    )
  let active_inventory_serialized = json.to_string(active_inventory_json)
  assert string.contains(
    active_inventory_serialized,
    "\"code\":\"LOCATION_IS_ACTIVE\"",
  )
  assert string.contains(
    active_inventory_serialized,
    "\"code\":\"LOCATION_HAS_INVENTORY\"",
  )

  let #(proxy_state.Response(body: primary_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> primary_id <> "\"}}"),
    )
  assert string.contains(
    json.to_string(primary_json),
    "\"code\":\"LOCATION_IS_PRIMARY\"",
  )

  let #(proxy_state.Response(body: fulfillment_service_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> fulfillment_service_id <> "\"}}"),
    )
  let fulfillment_service_serialized = json.to_string(fulfillment_service_json)
  assert string.contains(
    fulfillment_service_serialized,
    "\"code\":\"LOCATION_NOT_FOUND\"",
  )
  assert !string.contains(
    fulfillment_service_serialized,
    "\"code\":\"LOCATION_IS_ACTIVE\"",
  )
  assert !string.contains(
    fulfillment_service_serialized,
    "\"code\":\"LOCATION_HAS_INVENTORY\"",
  )

  let #(proxy_state.Response(body: pending_json, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> pending_orders_id <> "\"}}"),
    )
  assert string.contains(
    json.to_string(pending_json),
    "\"code\":\"LOCATION_HAS_PENDING_ORDERS\"",
  )

  let #(proxy_state.Response(body: retail_json, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(delete_body <> retail_subscription_id <> "\"}}"),
    )
  assert string.contains(
    json.to_string(retail_json),
    "\"code\":\"LOCATION_HAS_ACTIVE_RETAIL_SUBSCRIPTION\"",
  )
}

pub fn business_entity_reads_use_primary_and_known_ids_test() {
  let entity =
    make_raw_record("gid://shopify/BusinessEntity/1", "BusinessEntity", [
      #("displayName", StorePropertyString("Primary business")),
      #("companyName", StorePropertyNull),
      #("primary", StorePropertyBool(True)),
      #("archived", StorePropertyBool(False)),
    ])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store.upsert_base_business_entity(proxy.store, entity),
    )
  let body =
    "{\"query\":\"query($id: ID!) { primary: businessEntity { id displayName primary } known: businessEntity(id: $id) { id displayName primary } businessEntities { id displayName } }\",\"variables\":{\"id\":\"gid://shopify/BusinessEntity/1\"}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"primary\":{\"")
  assert string.contains(serialized, "\"known\":{\"")
  assert string.contains(serialized, "\"businessEntities\":[{")
  assert string.contains(serialized, "\"displayName\":\"Primary business\"")
}

pub fn publishable_publish_stages_collection_projection_test() {
  let collection =
    make_raw_record("gid://shopify/Collection/1", "Collection", [
      #("title", StorePropertyString("Draft collection")),
      #("handle", StorePropertyString("draft-collection")),
      #("publishedOnCurrentPublication", StorePropertyBool(False)),
      #(
        "availablePublicationsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(1)),
            #("precision", StorePropertyString("EXACT")),
          ]),
        ),
      ),
      #(
        "resourcePublicationsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(0)),
            #("precision", StorePropertyString("EXACT")),
          ]),
        ),
      ),
    ])
  let published =
    make_raw_record("gid://shopify/Collection/1", "Collection", [
      #("title", StorePropertyString("Draft collection")),
      #("handle", StorePropertyString("draft-collection")),
      #("publishedOnCurrentPublication", StorePropertyBool(True)),
      #(
        "availablePublicationsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(1)),
            #("precision", StorePropertyString("EXACT")),
          ]),
        ),
      ),
      #(
        "resourcePublicationsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(1)),
            #("precision", StorePropertyString("EXACT")),
          ]),
        ),
      ),
    ])
  let payload =
    StorePropertyMutationPayloadRecord(
      key: "publishablePublish:gid://shopify/Collection/1",
      data: dict.from_list([
        #("publishable", StorePropertyObject(published.data)),
        #("userErrors", StorePropertyList([])),
      ]),
    )
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_publishable(collection)
    |> store.upsert_base_store_property_mutation_payload(payload)
  let proxy = proxy_state.DraftProxy(..proxy, store: seeded_store)
  let body =
    "{\"query\":\"mutation($id: ID!, $input: [PublicationInput!]!) { publishablePublish(id: $id, input: $input) { publishable { ... on Collection { id title publishedOnCurrentPublication resourcePublicationsCount { count precision } } } userErrors { field message } } }\",\"variables\":{\"id\":\"gid://shopify/Collection/1\",\"input\":[{\"publicationId\":\"gid://shopify/Publication/1\"}]}}"
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"publishedOnCurrentPublication\":true")
  assert string.contains(serialized, "\"count\":1")

  let read_body =
    "{\"query\":\"query($id: ID!) { collection(id: $id) { id publishedOnCurrentPublication resourcePublicationsCount { count } } }\",\"variables\":{\"id\":\"gid://shopify/Collection/1\"}}"
  let #(proxy_state.Response(status: read_status, body: read_json, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_body))
  assert read_status == 200
  assert string.contains(
    json.to_string(read_json),
    "\"publishedOnCurrentPublication\":true",
  )
}

fn publishable_validation_base_record() -> StorePropertyRecord {
  make_raw_record("gid://shopify/Product/1", "Product", [
    #("publishedOnCurrentPublication", StorePropertyBool(False)),
    #(
      "resourcePublicationsCount",
      StorePropertyObject(
        dict.from_list([
          #("count", StorePropertyInt(0)),
          #("precision", StorePropertyString("EXACT")),
        ]),
      ),
    ),
  ])
}

fn publishable_validation_staged_record() -> StorePropertyRecord {
  make_raw_record("gid://shopify/Product/1", "Product", [
    #("publishedOnCurrentPublication", StorePropertyBool(True)),
    #(
      "resourcePublicationsCount",
      StorePropertyObject(
        dict.from_list([
          #("count", StorePropertyInt(1)),
          #("precision", StorePropertyString("EXACT")),
        ]),
      ),
    ),
  ])
}

fn seeded_publishable_validation_proxy() -> draft_proxy.DraftProxy {
  let base_publishable = publishable_validation_base_record()
  let staged_publishable = publishable_validation_staged_record()
  let payload =
    StorePropertyMutationPayloadRecord(
      key: "publishablePublish:gid://shopify/Product/1",
      data: dict.from_list([
        #("publishable", StorePropertyObject(staged_publishable.data)),
        #("userErrors", StorePropertyList([])),
      ]),
    )
  let unpublish_payload =
    StorePropertyMutationPayloadRecord(
      key: "publishableUnpublish:gid://shopify/Product/1",
      data: dict.from_list([
        #("publishable", StorePropertyObject(base_publishable.data)),
        #("userErrors", StorePropertyList([])),
      ]),
    )
  let proxy = draft_proxy.new()
  let seeded_store =
    proxy.store
    |> store.upsert_base_publishable(base_publishable)
    |> store.upsert_base_publications([
      make_publication("gid://shopify/Publication/1"),
    ])
    |> store.upsert_base_store_property_mutation_payload(payload)
    |> store.upsert_base_store_property_mutation_payload(unpublish_payload)
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn publishable_publish_validation_request(
  input: json.Json,
) -> draft_proxy.Request {
  let query =
    "mutation PublishableInputValidation($id: ID!, $input: [PublicationInput!]!) { publishablePublish(id: $id, input: $input) { publishable { ... on Product { id publishedOnCurrentPublication resourcePublicationsCount { count precision } } } userErrors { field message } } }"
  publishable_validation_request(query, input)
}

fn publishable_unpublish_validation_request(
  input: json.Json,
) -> draft_proxy.Request {
  let query =
    "mutation PublishableInputValidationUnpublish($id: ID!, $input: [PublicationInput!]!) { publishableUnpublish(id: $id, input: $input) { publishable { ... on Product { id publishedOnCurrentPublication resourcePublicationsCount { count precision } } } userErrors { field message } } }"
  publishable_validation_request(query, input)
}

fn publishable_validation_request(
  query: String,
  input: json.Json,
) -> draft_proxy.Request {
  graphql_request(
    json.to_string(
      json.object([
        #("query", json.string(query)),
        #(
          "variables",
          json.object([
            #("id", json.string("gid://shopify/Product/1")),
            #("input", input),
          ]),
        ),
      ]),
    ),
  )
}

fn one_publication_input(publication_id: String) -> json.Json {
  json.object([#("publicationId", json.string(publication_id))])
}

fn publishable_validation_entries(entries: List(json.Json)) -> json.Json {
  json.preprocessed_array(entries)
}

fn assert_publishable_validation_failure(
  response_body: json.Json,
  proxy: draft_proxy.DraftProxy,
  expected_error: String,
) {
  let serialized = json.to_string(response_body)
  assert string.contains(serialized, expected_error)
  assert string.contains(serialized, "\"publishedOnCurrentPublication\":true")
  assert string.contains(serialized, "\"count\":1")
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
  assert dict.size(proxy.store.staged_state.publishables) == 0
  let assert Some(record) =
    store.get_effective_publishable_by_id(
      proxy.store,
      "gid://shopify/Product/1",
    )
  assert dict.get(record.data, "publishedOnCurrentPublication")
    == Ok(StorePropertyBool(False))
  assert dict.get(record.data, "resourcePublicationsCount")
    == Ok(
      StorePropertyObject(
        dict.from_list([
          #("count", StorePropertyInt(0)),
          #("precision", StorePropertyString("EXACT")),
        ]),
      ),
    )
}

pub fn publishable_publish_rejects_duplicate_publication_id_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input =
    publishable_validation_entries([
      one_publication_input("gid://shopify/Publication/1"),
      one_publication_input("gid://shopify/Publication/1"),
    ])
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      publishable_publish_validation_request(input),
    )

  assert status == 200
  assert_publishable_validation_failure(
    response_body,
    proxy,
    "\"field\":[\"input\",\"1\",\"publicationId\"],\"message\":\"The same publication was specified more than once\"",
  )
}

pub fn publishable_publish_rejects_omitted_publication_id_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input = publishable_validation_entries([json.object([])])
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      publishable_publish_validation_request(input),
    )

  assert status == 200
  assert_publishable_validation_failure(
    response_body,
    proxy,
    "\"field\":[\"input\",\"0\",\"publicationId\"],\"message\":\"PublicationId cannot be empty\"",
  )
}

pub fn publishable_publish_rejects_pre_1970_publish_date_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input =
    publishable_validation_entries([
      json.object([
        #("publicationId", json.string("gid://shopify/Publication/1")),
        #("publishDate", json.string("1900-01-01T00:00:00Z")),
      ]),
    ])
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      publishable_publish_validation_request(input),
    )

  assert status == 200
  assert_publishable_validation_failure(
    response_body,
    proxy,
    "\"field\":[\"input\",\"0\",\"publishDate\"],\"message\":\"Publish date must be a date after the year 1969\"",
  )
}

pub fn publishable_publish_rejects_unknown_publication_id_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input =
    publishable_validation_entries([
      one_publication_input("gid://shopify/Publication/999999999"),
    ])
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      publishable_publish_validation_request(input),
    )

  assert status == 200
  assert_publishable_validation_failure(
    response_body,
    proxy,
    "\"field\":[\"input\",\"0\",\"publicationId\"],\"message\":\"Publication does not exist or is not publishable\"",
  )
}

pub fn publishable_unpublish_rejects_input_validation_failures_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input =
    publishable_validation_entries([
      one_publication_input("gid://shopify/Publication/1"),
      one_publication_input("gid://shopify/Publication/1"),
    ])
  let #(proxy_state.Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      publishable_unpublish_validation_request(input),
    )

  assert status == 200
  assert string.contains(
    json.to_string(response_body),
    "\"field\":[\"input\",\"1\",\"publicationId\"],\"message\":\"The same publication was specified more than once\"",
  )
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn publishable_input_empty_string_variable_is_invalid_variable_test() {
  let proxy = seeded_publishable_validation_proxy()
  let input =
    publishable_validation_entries([
      one_publication_input(""),
    ])
  let #(proxy_state.Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      publishable_publish_validation_request(input),
    )
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    serialized,
    "\"problems\":[{\"path\":[0,\"publicationId\"],\"explanation\":\"Invalid global id ''\",\"message\":\"Invalid global id ''\"}]",
  )
}

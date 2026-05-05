//// Store Properties domain tests for the Gleam port.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type ShopPolicyRecord, type ShopRecord, type StorePropertyRecord,
  type StorePropertyValue, CapturedArray, CapturedObject, CapturedString,
  DeliveryProfileRecord, PaymentSettingsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopPolicyRecord, ShopRecord, ShopResourceLimitsRecord,
  StorePropertyBool, StorePropertyInt, StorePropertyList,
  StorePropertyMutationPayloadRecord, StorePropertyNull, StorePropertyObject,
  StorePropertyRecord, StorePropertyString,
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
      sells_subscriptions: False,
      show_metrics: True,
      storefront: True,
      unified_markets: True,
    ),
    payment_settings: PaymentSettingsRecord(supported_digital_wallets: []),
    shop_policies: policies,
  )
}

fn make_policy(body: String) -> ShopPolicyRecord {
  ShopPolicyRecord(
    id: "gid://shopify/ShopPolicy/42438689001",
    title: "Contact",
    body: body,
    type_: "CONTACT_INFORMATION",
    url: "https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en",
    created_at: "2026-04-25T11:52:28Z",
    updated_at: "2026-04-25T11:52:29Z",
  )
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
  assert string.contains(mutation_serialized, "\"body\":\"<p>After</p>\"")
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
  assert string.contains(read_serialized, "\"body\":\"<p>After</p>\"")
  assert string.contains(read_serialized, "\"translations\":[]")

  let log = json.to_string(draft_proxy.get_log_snapshot(proxy))
  assert string.contains(log, "\"domain\":\"store-properties\"")
  assert string.contains(
    log,
    "\"stagedResourceIds\":[\"gid://shopify/ShopPolicy/1\"]",
  )
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
  let too_big = string.repeat("x", 524_289)
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

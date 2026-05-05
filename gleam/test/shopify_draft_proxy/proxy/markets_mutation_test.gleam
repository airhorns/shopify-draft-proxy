import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}

fn graphql(query: String) {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  graphql_with_proxy(proxy, query)
}

fn graphql_with_proxy(proxy, query: String) {
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

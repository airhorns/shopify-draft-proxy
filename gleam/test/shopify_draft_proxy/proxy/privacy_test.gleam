import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
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

fn registry_proxy() {
  draft_proxy.new()
  |> draft_proxy.with_default_registry
}

pub fn data_sale_opt_out_existing_customer_readback_and_log_test() {
  let proxy = registry_proxy()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"privacy@example.com\", firstName: \"Privacy\" }) { customer { id email dataSaleOptOut } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: opt_status, body: opt_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { dataSaleOptOut(email: \"privacy@example.com\") { customerId userErrors { field message code } } }",
    )
  assert opt_status == 200
  let opt_json = json.to_string(opt_body)
  assert string.contains(
    opt_json,
    "\"dataSaleOptOut\":{\"customerId\":\"gid://shopify/Customer/1\",\"userErrors\":[]}",
  )

  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email dataSaleOptOut } customerByIdentifier(identifier: { emailAddress: \"privacy@example.com\" }) { id email dataSaleOptOut } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"privacy@example.com\",\"dataSaleOptOut\":true}",
  )
  assert string.contains(
    read_json,
    "\"customerByIdentifier\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"privacy@example.com\",\"dataSaleOptOut\":true}",
  )

  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  let log_json = json.to_string(log_body)
  assert string.contains(log_json, "\"domain\":\"privacy\"")
  assert string.contains(log_json, "\"primaryRootField\":\"dataSaleOptOut\"")
  assert string.contains(
    log_json,
    "Locally staged privacy-domain data sale opt-out mutation",
  )
}

pub fn data_sale_opt_out_unknown_email_creates_opted_out_customer_test() {
  let proxy = registry_proxy()
  let #(Response(status: opt_status, body: opt_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { dataSaleOptOut(email: \"new-privacy@example.com\") { customerId userErrors { field message code } } }",
    )
  assert opt_status == 200
  assert string.contains(
    json.to_string(opt_body),
    "\"dataSaleOptOut\":{\"customerId\":\"gid://shopify/Customer/1\",\"userErrors\":[]}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email dataSaleOptOut defaultEmailAddress { emailAddress } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"email\":\"new-privacy@example.com\"")
  assert string.contains(read_json, "\"dataSaleOptOut\":true")
  assert string.contains(
    read_json,
    "\"defaultEmailAddress\":{\"emailAddress\":\"new-privacy@example.com\"}",
  )
}

pub fn data_sale_opt_out_invalid_email_matches_shopify_error_test() {
  let proxy = registry_proxy()
  let #(Response(status: opt_status, body: opt_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { dataSaleOptOut(email: \"not-an-email\") { customerId userErrors { field message code } } }",
    )
  assert opt_status == 200
  let opt_json = json.to_string(opt_body)
  assert string.contains(opt_json, "\"customerId\":null")
  assert string.contains(opt_json, "\"field\":null")
  assert string.contains(opt_json, "\"message\":\"Data sale opt out failed.\"")
  assert string.contains(opt_json, "\"code\":\"FAILED\"")

  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  assert json.to_string(log_body) == "{\"entries\":[]}"
}

pub fn unsupported_privacy_roots_stay_without_local_dispatch_test() {
  let proxy = registry_proxy()
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { privacyFeaturesDisable { userErrors { field message code } } }",
    )
  assert status == 400
  assert string.contains(
    json.to_string(body),
    "No mutation dispatcher implemented for root field: privacyFeaturesDisable",
  )
}

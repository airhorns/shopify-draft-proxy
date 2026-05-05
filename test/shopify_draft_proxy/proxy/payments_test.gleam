import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
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

pub fn payment_customization_metafields_and_function_handle_readback_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"baz\" }] }) { paymentCustomization { id title functionId metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(
    create_json,
    "\"namespace\":\"app--347082227713--foo\"",
  )
  assert string.contains(create_json, "\"key\":\"bar\"")
  assert string.contains(create_json, "\"value\":\"baz\"")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionHandle: \"handle-after\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"qux\" }] }) { paymentCustomization { id title functionId functionHandle metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(
    update_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/123\"",
  )
  assert string.contains(update_json, "\"functionHandle\":\"handle-after\"")
  assert string.contains(update_json, "\"value\":\"qux\"")
  assert !string.contains(update_json, "\"value\":\"baz\"")

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId functionHandle metafield(namespace: \"$app:foo\", key: \"bar\") { namespace key type value } metafields(first: 5) { edges { node { namespace key type value } } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"title\":\"After\"")
  assert string.contains(
    read_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/123\"",
  )
  assert string.contains(read_json, "\"functionHandle\":\"handle-after\"")
  assert string.contains(
    read_json,
    "\"metafield\":{\"namespace\":\"app--347082227713--foo\"",
  )
  assert string.contains(read_json, "\"value\":\"qux\"")
  assert !string.contains(read_json, "\"value\":\"baz\"")
}

pub fn payment_customization_invalid_metafield_shape_returns_user_error_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Invalid\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", value: \"baz\" }] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"paymentCustomization\":null")
  assert string.contains(
    create_json,
    "\"field\":[\"paymentCustomization\",\"metafields\",\"0\",\"type\"]",
  )
  assert string.contains(create_json, "\"code\":\"INVALID_METAFIELDS\"")

  let seed_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Seed\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\" }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed_query)
  assert seed_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { metafields: [{ key: \"bar\", type: \"single_line_text_field\", value: \"baz\" }] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"metafields\",\"0\",\"namespace\"]",
  )
  assert string.contains(update_json, "\"code\":\"INVALID_METAFIELDS\"")
}

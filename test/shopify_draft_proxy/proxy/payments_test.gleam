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
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionHandle: \"handle-before\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"baz\" }] }) { paymentCustomization { id title functionId functionHandle metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
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
  assert string.contains(create_json, "\"functionId\":null")
  assert string.contains(create_json, "\"functionHandle\":\"handle-before\"")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionHandle: \"handle-before\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"qux\" }] }) { paymentCustomization { id title functionId functionHandle metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(update_json, "\"functionId\":null")
  assert string.contains(update_json, "\"functionHandle\":\"handle-before\"")
  assert string.contains(update_json, "\"value\":\"qux\"")
  assert !string.contains(update_json, "\"value\":\"baz\"")

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId functionHandle metafield(namespace: \"$app:foo\", key: \"bar\") { namespace key type value } metafields(first: 5) { edges { node { namespace key type value } } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"title\":\"After\"")
  assert string.contains(read_json, "\"functionId\":null")
  assert string.contains(read_json, "\"functionHandle\":\"handle-before\"")
  assert string.contains(
    read_json,
    "\"metafield\":{\"namespace\":\"app--347082227713--foo\"",
  )
  assert string.contains(read_json, "\"value\":\"qux\"")
  assert !string.contains(read_json, "\"value\":\"baz\"")
}

pub fn payment_customization_update_rejects_different_existing_function_test() {
  let seed_a =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_a_status, ..), proxy) =
    graphql(draft_proxy.new(), seed_a)
  assert seed_a_status == 200
  let seed_b =
    "mutation { validationCreate(validation: { title: \"Function B\", functionHandle: \"payment-b\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_b_status, ..), proxy) = graphql(proxy, seed_b)
  assert seed_b_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/5\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/payment-b\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"functionId\"]",
  )
  assert string.contains(
    update_json,
    "\"code\":\"FUNCTION_ID_CANNOT_BE_CHANGED\"",
  )
  assert string.contains(
    update_json,
    "\"message\":\"Function ID cannot be changed.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/5\") { id title functionId functionHandle } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
  assert !string.contains(read_json, "payment-b")
}

pub fn payment_customization_update_allows_equivalent_function_handle_test() {
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { title: \"After\", functionHandle: \"payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(
    update_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
  assert string.contains(update_json, "\"functionHandle\":null")
}

pub fn payment_customization_update_allows_equivalent_function_id_gid_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"raw-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionId: \"gid://shopify/ShopifyFunction/raw-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(update_json, "\"functionId\":\"raw-payment-function\"")
}

pub fn payment_customization_update_unknown_function_id_is_immutable_test() {
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/missing\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"code\":\"FUNCTION_ID_CANNOT_BE_CHANGED\"",
  )
  assert string.contains(
    update_json,
    "\"message\":\"Function ID cannot be changed.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/3\") { id title functionId } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
}

pub fn payment_customization_update_unknown_function_handle_returns_not_found_test() {
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { functionHandle: \"missing-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"functionHandle\"]",
  )
  assert string.contains(update_json, "\"code\":\"FUNCTION_NOT_FOUND\"")
  assert string.contains(
    update_json,
    "\"message\":\"Could not find function with handle: missing-payment-function.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/3\") { id title functionId } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
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

import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy

pub fn product_create_then_product_read_smoke_test() {
  let proxy = draft_proxy.new()
  let create_body =
    "{\"query\":\"mutation { productCreate(product: { title: \\\"Gleam Product Hat\\\" }) { product { id title handle status variants(first: 1) { nodes { id title inventoryItem { id tracked requiresShipping } } } } userErrors { field message } } }\"}"
  let #(create_response, created_proxy) =
    draft_proxy.process_request(
      proxy,
      draft_proxy.Request(
        method: "POST",
        path: draft_proxy.default_graphql_path("2025-01"),
        headers: dict.new(),
        body: create_body,
      ),
    )

  assert create_response.status == 200
  let create_json = json.to_string(create_response.body)
  assert string.contains(create_json, "\"productCreate\"")
  assert string.contains(create_json, "\"title\":\"Gleam Product Hat\"")
  assert string.contains(create_json, "\"handle\":\"gleam-product-hat\"")
  assert string.contains(create_json, "\"userErrors\":[]")

  let product_id = "gid://shopify/Product/1"
  let read_body =
    "{\"query\":\"query { product(id: \\\""
    <> product_id
    <> "\\\") { id title handle status variants(first: 1) { nodes { id title } } } }\"}"
  let #(read_response, _) =
    draft_proxy.process_request(
      created_proxy,
      draft_proxy.Request(
        method: "POST",
        path: draft_proxy.default_graphql_path("2025-01"),
        headers: dict.new(),
        body: read_body,
      ),
    )

  assert read_response.status == 200
  let read_json = json.to_string(read_response.body)
  assert string.contains(read_json, "\"product\":{\"id\":\"" <> product_id)
  assert string.contains(read_json, "\"title\":\"Gleam Product Hat\"")
}

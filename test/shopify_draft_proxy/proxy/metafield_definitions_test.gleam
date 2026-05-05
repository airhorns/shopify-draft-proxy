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

pub fn non_product_owner_definition_lifecycle_test() {
  let proxy = draft_proxy.new()
  let create_customer =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Loyalty Tier\", namespace: \"loyalty\", key: \"tier\", type: \"single_line_text_field\", ownerType: CUSTOMER }) { createdDefinition { id ownerType namespace key name pinnedPosition } userErrors { field code message } } }"
  let #(Response(status: customer_status, body: customer_body, ..), proxy) =
    graphql(proxy, create_customer)
  assert customer_status == 200
  let customer_json = json.to_string(customer_body)
  assert string.contains(
    customer_json,
    "\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(customer_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(customer_json, "\"userErrors\":[]")
  assert !string.contains(customer_json, "UNSUPPORTED_OWNER_TYPE")

  let create_order =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Order Channel\", namespace: \"fulfillment\", key: \"channel\", type: \"single_line_text_field\", ownerType: ORDER }) { createdDefinition { id ownerType namespace key name } userErrors { field code message } } }"
  let #(Response(status: order_status, body: order_body, ..), proxy) =
    graphql(proxy, create_order)
  assert order_status == 200
  let order_json = json.to_string(order_body)
  assert string.contains(order_json, "\"ownerType\":\"ORDER\"")
  assert string.contains(order_json, "\"userErrors\":[]")
  assert !string.contains(order_json, "UNSUPPORTED_OWNER_TYPE")

  let create_company =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Company Segment\", namespace: \"b2b\", key: \"segment\", type: \"single_line_text_field\", ownerType: COMPANY }) { createdDefinition { id ownerType namespace key name } userErrors { field code message } } }"
  let #(Response(status: company_status, body: company_body, ..), proxy) =
    graphql(proxy, create_company)
  assert company_status == 200
  let company_json = json.to_string(company_body)
  assert string.contains(company_json, "\"ownerType\":\"COMPANY\"")
  assert string.contains(company_json, "\"userErrors\":[]")
  assert !string.contains(company_json, "UNSUPPORTED_OWNER_TYPE")

  let update_customer =
    "mutation { metafieldDefinitionUpdate(definition: { name: \"Loyalty Tier Updated\", namespace: \"loyalty\", key: \"tier\", ownerType: CUSTOMER, description: \"Customer loyalty tier\" }) { updatedDefinition { id ownerType namespace key name description } userErrors { field code message } validationJob { id } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_customer)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(update_json, "\"name\":\"Loyalty Tier Updated\"")
  assert string.contains(
    update_json,
    "\"description\":\"Customer loyalty tier\"",
  )
  assert string.contains(update_json, "\"userErrors\":[]")
  assert !string.contains(update_json, "UNSUPPORTED_OWNER_TYPE")

  let read_customer =
    "query { byId: metafieldDefinition(id: \"gid://shopify/MetafieldDefinition/1\") { id ownerType namespace key name description } byIdentifier: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: \"loyalty\", key: \"tier\" }) { id ownerType namespace key name } customer(id: \"gid://shopify/Customer/1\") { id metafieldDefinitions(first: 5, namespace: \"loyalty\") { nodes { id ownerType namespace key name } } } metafieldDefinitions(ownerType: ORDER, namespace: \"fulfillment\", first: 5) { nodes { id ownerType namespace key name } } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read_customer)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"byId\":{\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(
    read_json,
    "\"byIdentifier\":{\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(
    read_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\"",
  )
  assert string.contains(read_json, "\"name\":\"Loyalty Tier Updated\"")
  assert string.contains(read_json, "\"ownerType\":\"ORDER\"")

  let delete_customer =
    "mutation { metafieldDefinitionDelete(id: \"gid://shopify/MetafieldDefinition/1\", deleteAllAssociatedMetafields: true) { deletedDefinitionId deletedDefinition { ownerType namespace key } userErrors { field code message } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(proxy, delete_customer)
  assert delete_status == 200
  let delete_json = json.to_string(delete_body)
  assert string.contains(
    delete_json,
    "\"deletedDefinitionId\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(delete_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(delete_json, "\"userErrors\":[]")

  let read_after_delete =
    "query { deleted: metafieldDefinition(id: \"gid://shopify/MetafieldDefinition/1\") active: metafieldDefinitions(ownerType: CUSTOMER, namespace: \"loyalty\", first: 5) { nodes { id } } order: metafieldDefinitions(ownerType: ORDER, namespace: \"fulfillment\", first: 5) { nodes { ownerType key } } company: metafieldDefinitions(ownerType: COMPANY, namespace: \"b2b\", first: 5) { nodes { ownerType key } } }"
  let #(Response(status: after_status, body: after_body, ..), _) =
    graphql(proxy, read_after_delete)
  assert after_status == 200
  let after_json = json.to_string(after_body)
  assert string.contains(after_json, "\"deleted\":null")
  assert string.contains(after_json, "\"active\":{\"nodes\":[]}")
  assert string.contains(
    after_json,
    "\"order\":{\"nodes\":[{\"ownerType\":\"ORDER\"",
  )
  assert string.contains(
    after_json,
    "\"company\":{\"nodes\":[{\"ownerType\":\"COMPANY\"",
  )
}

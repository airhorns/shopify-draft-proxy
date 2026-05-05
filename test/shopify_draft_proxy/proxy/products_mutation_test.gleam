import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy.{type Request}
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type CollectionRecord, type CollectionRuleSetRecord, type InventoryLevelRecord,
  type InventoryQuantityRecord, type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord, type ProductRecord,
  type ProductVariantRecord, CollectionRecord, CollectionRuleRecord,
  CollectionRuleSetRecord, InventoryItemRecord, InventoryLevelRecord,
  InventoryLocationRecord, InventoryQuantityRecord,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord, ProductCollectionRecord,
  ProductMetafieldRecord, ProductOptionRecord, ProductOptionValueRecord,
  ProductRecord, ProductSeoRecord, ProductVariantRecord,
  ProductVariantSelectedOptionRecord,
}

fn empty_headers() -> dict.Dict(String, String) {
  dict.new()
}

fn graphql_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: "{\"query\":\"" <> query <> "\"}",
  )
}

fn graphql_request_body(body: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: body,
  )
}

fn run_product_mutation(initial_store: store.Store, query: String) {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: initial_store)
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  #(status, json.to_string(body), next_proxy)
}

fn assert_product_option_user_error(
  initial_store: store.Store,
  query: String,
  code: String,
  field_json: String,
) {
  let #(status, body, next_proxy) = run_product_mutation(initial_store, query)

  assert status == 200
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  assert string.contains(body, "\"field\":" <> field_json)
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store.Failed
}

fn graphql_document_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: json.to_string(json.object([#("query", json.string(query))])),
  )
}

pub fn product_options_create_stages_default_product_options_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", position: 1, values: [{ name: \\\"Red\\\" }, { name: \\\"Green\\\" }] }]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsCreate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Color\",\"position\":1,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Red\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Color\",\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Red\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_options_create_rejects_invalid_option_inputs_test() {
  let long_name = repeated_text("N", 256)
  let long_value = repeated_text("V", 256)

  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }] }, { name: \\\"Color\\\", values: [{ name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_NAME",
    "[\"options\",\"1\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }, { name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_VALUE",
    "[\"options\",\"0\",\"values\",\"1\",\"name\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [] }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUES_MISSING",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"\\\", values: [{ name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_NAME_MISSING",
    "[\"options\",\"0\",\"name\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\""
      <> long_name
      <> "\\\", values: [{ name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_NAME_TOO_LONG",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\""
      <> long_value
      <> "\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_NAME_TOO_LONG",
    "[\"options\",\"0\",\"values\",\"0\",\"name\"]",
  )
}

pub fn product_options_create_rejects_product_level_constraints_test() {
  assert_product_option_user_error(
    three_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Finish\\\", values: [{ name: \\\"Matte\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTIONS_OVER_LIMIT",
    "[\"options\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_ALREADY_EXISTS",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Material\\\" }]) { product { id } userErrors { field message code } } }",
    "NEW_OPTION_WITHOUT_VALUE_FOR_EXISTING_VARIANTS",
    "[\"options\",\"0\",\"values\"]",
  )
  assert_product_option_user_error(
    variant_cap_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", variantStrategy: CREATE, options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "TOO_MANY_VARIANTS_CREATED",
    "[\"options\"]",
  )
}

pub fn product_option_update_rejects_invalid_option_inputs_test() {
  let long_value = repeated_text("V", 256)

  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Size\\\" }) { product { id } userErrors { field message code } } }",
    "OPTION_ALREADY_EXISTS",
    "[\"option\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToAdd: [{ name: \\\"Blue\\\" }, { name: \\\"Blue\\\" }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_VALUE",
    "[\"optionValuesToAdd\",\"1\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToAdd: [{ name: \\\"red\\\" }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_ALREADY_EXISTS",
    "[\"optionValuesToAdd\",\"0\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToUpdate: [{ id: \\\"gid://shopify/ProductOptionValue/red\\\", name: \\\""
      <> long_value
      <> "\\\" }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_NAME_TOO_LONG",
    "[\"optionValuesToUpdate\",\"0\",\"name\"]",
  )
}

pub fn product_options_delete_reports_option_codes_for_unknown_ids_test() {
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/optioned\\\", options: [\\\"gid://shopify/ProductOption/missing\\\"]) { deletedOptionsIds product { id } userErrors { field message code } } }",
    "OPTION_DOES_NOT_EXIST",
    "[\"options\",\"0\"]",
  )
}

pub fn product_option_update_repositions_values_and_variants_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Shade\\\", position: 2 }, optionValuesToAdd: [{ name: \\\"Blue\\\" }], optionValuesToUpdate: [{ id: \\\"gid://shopify/ProductOptionValue/red\\\", name: \\\"Crimson\\\" }], optionValuesToDelete: [\\\"gid://shopify/ProductOptionValue/green\\\"]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Shade\",\"position\":2,\"values\":[\"Crimson\"],\"optionValues\":[{\"name\":\"Crimson\",\"hasVariants\":true},{\"name\":\"Blue\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Crimson\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Shade\",\"value\":\"Crimson\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Shade\",\"position\":2,\"values\":[\"Crimson\"],\"optionValues\":[{\"name\":\"Crimson\",\"hasVariants\":true},{\"name\":\"Blue\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Crimson\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Shade\",\"value\":\"Crimson\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_options_delete_restores_default_option_state_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/optioned\\\", options: [\\\"gid://shopify/ProductOption/color\\\", \\\"gid://shopify/ProductOption/size\\\"]) { deletedOptionsIds product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsDelete\":{\"deletedOptionsIds\":[\"gid://shopify/ProductOption/color\",\"gid://shopify/ProductOption/size\"],\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Title\",\"position\":1,\"values\":[\"Default Title\"],\"optionValues\":[{\"name\":\"Default Title\",\"hasVariants\":true}]}],\"variants\":{\"nodes\":[{\"title\":\"Default Title\",\"selectedOptions\":[{\"name\":\"Title\",\"value\":\"Default Title\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Title\",\"position\":1,\"values\":[\"Default Title\"],\"optionValues\":[{\"name\":\"Default Title\",\"hasVariants\":true}]}],\"variants\":{\"nodes\":[{\"title\":\"Default Title\",\"selectedOptions\":[{\"name\":\"Title\",\"value\":\"Default Title\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_user_error_shape_validation_branches_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { productCreate(product: { title: \\\"\\\" }) { product { id } userErrors { field message code } } }",
      ),
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"

  let #(Response(status: options_status, body: options_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/missing\\\", options: [\\\"gid://shopify/ProductOption/missing\\\"]) { deletedOptionsIds product { id } userErrors { field message code } } }",
      ),
    )
  assert options_status == 200
  assert json.to_string(options_body)
    == "{\"data\":{\"productOptionsDelete\":{\"deletedOptionsIds\":[],\"product\":null,\"userErrors\":[{\"field\":[\"productId\"],\"message\":\"Product does not exist\",\"code\":\"PRODUCT_DOES_NOT_EXIST\"}]}}}"

  let #(Response(status: collection_status, body: collection_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { collectionCreate(input: { title: \\\"\\\" }) { collection { id } userErrors { field message code } } }",
      ),
    )
  assert collection_status == 200
  assert json.to_string(collection_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"

  let #(Response(status: activate_status, body: activate_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/missing\\\", locationId: \\\"gid://shopify/Location/missing\\\") { inventoryLevel { id } userErrors { field message code } } }",
      ),
    )
  assert activate_status == 200
  assert json.to_string(activate_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":null,\"userErrors\":[{\"field\":[\"inventoryItemId\"],\"message\":\"Inventory item does not exist\",\"code\":\"INVALID_INVENTORY_ITEM\"}]}}}"
}

pub fn product_options_reorder_reorders_variants_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionsReorder(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ id: \\\"gid://shopify/ProductOption/size\\\", values: [{ id: \\\"gid://shopify/ProductOptionValue/small\\\" }] }, { id: \\\"gid://shopify/ProductOption/color\\\", values: [{ id: \\\"gid://shopify/ProductOptionValue/green\\\" }, { id: \\\"gid://shopify/ProductOptionValue/red\\\" }] }]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsReorder\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Color\",\"position\":2,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Red\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Color\",\"position\":2,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Red\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Color\",\"value\":\"Red\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_change_status_stages_search_lagged_status_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productChangeStatus(productId: \\\"gid://shopify/Product/optioned\\\", status: ARCHIVED) { product { id status updatedAt } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productChangeStatus\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"status\":\"ARCHIVED\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id status updatedAt } products(first: 10, query: \\\"status:archived tag:existing\\\") { nodes { id status } } productsCount(query: \\\"status:archived tag:existing\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"status\":\"ARCHIVED\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"products\":{\"nodes\":[]},\"productsCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_delete_stages_downstream_no_data_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productDelete(input: { id: \\\"gid://shopify/Product/optioned\\\" }) { deletedProductId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productDelete\":{\"deletedProductId\":\"gid://shopify/Product/optioned\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id title } products(first: 10, query: \\\"tag:existing\\\") { nodes { id title } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":null,\"products\":{\"nodes\":[]}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafield_delete_stages_product_owned_deletion_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldDelete(input: { id: \\\"gid://shopify/Metafield/material\\\" }) { deletedId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldDelete\":{\"deletedId\":\"gid://shopify/Metafield/material\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { material: metafield(namespace: \\\"custom\\\", key: \\\"material\\\") { id } origin: metafield(namespace: \\\"details\\\", key: \\\"origin\\\") { id namespace key value } metafields(first: 10) { nodes { id namespace key value } pageInfo { hasNextPage hasPreviousPage } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"material\":null,\"origin\":{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"},\"metafields\":{\"nodes\":[{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafield_delete_unknown_id_keeps_compatibility_payload_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldDelete(input: { id: \\\"gid://shopify/Metafield/missing\\\" }) { deletedId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldDelete\":{\"deletedId\":\"gid://shopify/Metafield/missing\",\"userErrors\":[]}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafields_set_rejects_invalid_input_shape_and_values_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let owner_id = "gid://shopify/Product/optioned"
  let long_namespace = string.repeat("n", times: 256)
  let long_key = string.repeat("k", times: 65)
  let query =
    "mutation { metafieldsSet(metafields: ["
    <> metafields_set_input(owner_id, "ab", "x", "single_line_text_field", "v")
    <> ","
    <> metafields_set_input(
      owner_id,
      long_namespace,
      "long_namespace",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      long_key,
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "bad namespace",
      "good_key",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "bad.key",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "shopify_standard",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "protected",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "shopify-l10n-fields",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "tier",
      "number_integer",
      "not a number",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "flag", "boolean", "yes")
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "color", "color", "blue")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "published",
      "date_time",
      "tomorrow",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "data", "json", "{nope")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "related",
      "product_reference",
      "gid://shopify/Product/missing",
    )
    <> "]) { metafields { id } userErrors { field code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_document_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsSet\":{\"metafields\":[],\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"namespace\"],\"code\":\"TOO_SHORT\"},{\"field\":[\"metafields\",\"0\",\"key\"],\"code\":\"TOO_SHORT\"},{\"field\":[\"metafields\",\"1\",\"namespace\"],\"code\":\"TOO_LONG\"},{\"field\":[\"metafields\",\"2\",\"key\"],\"code\":\"TOO_LONG\"},{\"field\":[\"metafields\",\"3\",\"namespace\"],\"code\":\"INVALID\"},{\"field\":[\"metafields\",\"4\",\"key\"],\"code\":\"INVALID\"},{\"field\":[\"metafields\",\"5\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"6\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"7\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"8\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"9\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"10\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"11\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"12\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"13\",\"value\"],\"code\":\"INVALID_VALUE\"}]}}}"
  assert store.get_effective_metafields_by_owner_id(next_proxy.store, owner_id)
    |> list.length
    == 0
}

pub fn metafields_set_rejects_invalid_list_structured_and_definition_values_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(..proxy, store: definition_validation_store())
  let owner_id = "gid://shopify/Product/optioned"
  let query =
    "mutation { metafieldsSet(metafields: ["
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "scores",
      "list.number_integer",
      "not-json",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "weight", "weight", "heavy")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "min_tier",
      "number_integer",
      "1",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "max_tier",
      "number_integer",
      "10",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "sku_code",
      "single_line_text_field",
      "abc123",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "plan",
      "single_line_text_field",
      "bronze",
    )
    <> "]) { metafields { id } userErrors { field code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_document_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsSet\":{\"metafields\":null,\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"1\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"2\",\"value\"],\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"metafields\",\"3\",\"value\"],\"code\":\"LESS_THAN_OR_EQUAL_TO\"},{\"field\":[\"metafields\",\"4\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"5\",\"value\"],\"code\":\"INCLUSION\"}]}}}"
  assert store.get_effective_metafields_by_owner_id(next_proxy.store, owner_id)
    |> list.length
    == 0
}

pub fn metafields_delete_stages_product_owned_deletions_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldsDelete(metafields: [{ ownerId: \\\"gid://shopify/Product/optioned\\\", namespace: \\\"custom\\\", key: \\\"material\\\" }, { ownerId: \\\"gid://shopify/Product/optioned\\\", namespace: \\\"custom\\\", key: \\\"missing\\\" }]) { deletedMetafields { ownerId namespace key } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsDelete\":{\"deletedMetafields\":[{\"ownerId\":\"gid://shopify/Product/optioned\",\"namespace\":\"custom\",\"key\":\"material\"},null],\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { material: metafield(namespace: \\\"custom\\\", key: \\\"material\\\") { id } origin: metafield(namespace: \\\"details\\\", key: \\\"origin\\\") { id namespace key value } metafields(first: 10) { nodes { id namespace key value } pageInfo { hasNextPage hasPreviousPage } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"material\":null,\"origin\":{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"},\"metafields\":{\"nodes\":[{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_add_stages_tags_and_preserves_base_tag_search_lag_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { tagsAdd(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"winter\\\", \\\"existing\\\", \\\"fall\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"tagsAdd\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"fall\",\"winter\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } products(first: 10, query: \\\"tag:fall\\\") { nodes { id tags } } productsCount(query: \\\"tag:fall\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"fall\",\"winter\"]},\"products\":{\"nodes\":[]},\"productsCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_remove_stages_tags_and_keeps_removed_tag_searchable_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tagged_product_store())
  let query =
    "mutation { tagsRemove(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"sale\\\", \\\"missing\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"tagsRemove\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } remaining: products(first: 10, query: \\\"tag:summer\\\") { nodes { id tags } } removed: products(first: 10, query: \\\"tag:sale\\\") { nodes { id tags } } remainingCount: productsCount(query: \\\"tag:summer\\\") { count precision } removedCount: productsCount(query: \\\"tag:sale\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"remaining\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]}]},\"removed\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]}]},\"remainingCount\":{\"count\":1,\"precision\":\"EXACT\"},\"removedCount\":{\"count\":1,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_add_and_remove_use_shopify_tag_identity_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tagged_product_store())
  let add_query =
    "mutation { tagsAdd(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\" Sale \\\", \\\"SALE\\\", \\\"blue\\\", \\\"Blue\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))

  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"tagsAdd\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"blue\",\"existing\",\"sale\",\"summer\"]},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { tagsRemove(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"SALE\\\", \\\"BLUE\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"
  let #(Response(status: remove_status, body: remove_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))

  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"tagsRemove\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"userErrors\":[]}}}"
}

pub fn product_update_stages_fields_and_downstream_reads_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", title: \\\"Updated Board\\\", vendor: \\\"HERMES\\\", productType: \\\"BOARDS\\\", tags: [\\\"beta\\\", \\\"alpha\\\"], descriptionHtml: \\\"<p>Updated</p>\\\", templateSuffix: \\\"custom\\\", seo: { title: \\\"SEO title\\\", description: \\\"SEO description\\\" } }) { product { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Updated Board\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Updated</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Updated Board\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Updated</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_update_normalizes_tags_like_shopify_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\" Red \\\", \\\"red\\\", \\\"RED\\\", \\\" big   sale \\\"] }) { product { id tags } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"big   sale\",\"Red\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"big   sale\",\"Red\"]}}}"
}

pub fn product_update_rejects_invalid_product_tags_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let long_tag = string.repeat("x", times: 256)
  let long_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\""
    <> long_tag
    <> "\\\"] }) { product { id tags } userErrors { field message } } }"

  let #(Response(status: long_status, body: long_body, ..), long_proxy) =
    draft_proxy.process_request(proxy, graphql_request(long_query))

  assert long_status == 200
  assert json.to_string(long_body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\"]},\"userErrors\":[{\"field\":[\"tags\"],\"message\":\"Product tags is invalid\"}]}}}"
  assert store.get_log(long_proxy.store)
    |> list.length
    == 1

  let too_many_tags = string.repeat("\\\"tag\\\",", times: 251)
  let too_many_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: ["
    <> too_many_tags
    <> "] }) { product { id tags } userErrors { field message } } }"
  let #(
    Response(status: too_many_status, body: too_many_body, ..),
    too_many_proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(too_many_query))
  let too_many_json = json.to_string(too_many_body)
  assert too_many_status == 200
  assert !string.contains(too_many_json, "\"data\"")
  assert string.contains(
    too_many_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    too_many_json,
    "\"path\":[\"productUpdate\",\"product\",\"tags\"]",
  )
  assert string.contains(too_many_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(too_many_proxy.store) == []
}

pub fn product_update_blank_title_returns_existing_product_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", title: \\\"\\\" }) { product { id title handle } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Optioned Board\",\"handle\":\"optioned-board\"},\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\"}]}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_create_and_set_normalize_tags_like_shopify_test() {
  let create_query =
    "mutation { productCreate(product: { title: \\\"Created Board\\\", status: DRAFT, tags: [\\\"Red\\\", \\\"blue\\\", \\\"red\\\"] }) { product { id tags } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"tags\":[\"blue\",\"Red\"]},\"userErrors\":[]}}}"

  let set_query =
    "mutation { productSet(input: { title: \\\"Set Board\\\", status: DRAFT, tags: [\\\"Red\\\", \\\"blue\\\", \\\"red\\\"] }, synchronous: true) { product { id tags } userErrors { field message } } }"
  let #(Response(status: set_status, body: set_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(set_query))
  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"tags\":[\"blue\",\"Red\"]},\"userErrors\":[]}}}"
}

pub fn product_create_stages_product_default_variant_and_inventory_test() {
  let proxy = draft_proxy.new()
  let query =
    "mutation { productCreate(product: { title: \\\"Created Board\\\", status: DRAFT, vendor: \\\"HERMES\\\", productType: \\\"BOARDS\\\", tags: [\\\"beta\\\", \\\"alpha\\\"], descriptionHtml: \\\"<p>Created</p>\\\", templateSuffix: \\\"custom\\\", seo: { title: \\\"SEO title\\\", description: \\\"SEO description\\\" } }) { product { id title handle status vendor productType tags descriptionHtml templateSuffix seo { title description } totalInventory tracksInventory variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Created</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"},\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true}}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\\\") { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } } } variant: productVariant(id: \\\"gid://shopify/ProductVariant/4\\\") { id title inventoryQuantity inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory } } stock: inventoryItem(id: \\\"gid://shopify/InventoryItem/5\\\") { id tracked requiresShipping variant { id title sku inventoryQuantity product { id title handle status totalInventory tracksInventory } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true}}]}},\"variant\":{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true},\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false}},\"stock\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true,\"variant\":{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"sku\":null,\"inventoryQuantity\":0,\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_variant_mutations_recompute_product_derived_fields_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { productCreate(product: { title: \\\"Hat\\\", productOptions: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }] }] }) { product { id priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 10) { nodes { id title price selectedOptions { name value } } } } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"0.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"0.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":1,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":false,\"tracksInventory\":false,\"totalInventory\":0,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Red\",\"price\":\"0.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let product_id = "gid://shopify/Product/1?shopify-draft-proxy=synthetic"
  let price_update_query =
    "mutation { productVariantsBulkUpdate(productId: \\\""
    <> product_id
    <> "\\\", variants: [{ id: \\\"gid://shopify/ProductVariant/4\\\", price: \\\"10.00\\\" }]) { product { priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory } productVariants { id price } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(price_update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantsBulkUpdate\":{\"product\":{\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"10.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"10.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":1,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":false,\"tracksInventory\":false,\"totalInventory\":0},\"productVariants\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"price\":\"10.00\"}],\"userErrors\":[]}}}"

  let bulk_query =
    "mutation { productVariantsBulkCreate(productId: \\\""
    <> product_id
    <> "\\\", variants: [{ optionValues: [{ optionName: \\\"Color\\\", name: \\\"Blue\\\" }], price: \\\"5.00\\\" }, { optionValues: [{ optionName: \\\"Color\\\", name: \\\"Green\\\" }], price: \\\"20.00\\\" }]) { product { id priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } priceRange { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 10) { nodes { title price selectedOptions { name value } } } } productVariants { title price } userErrors { field message code } } }"
  let #(Response(status: bulk_status, body: bulk_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(bulk_query))
  assert bulk_status == 200
  assert json.to_string(bulk_body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"priceRange\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":3,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":0,\"variants\":{\"nodes\":[{\"title\":\"Red\",\"price\":\"10.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]},{\"title\":\"Blue\",\"price\":\"5.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Blue\"}]},{\"title\":\"Green\",\"price\":\"20.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Green\"}]}]}},\"productVariants\":[{\"title\":\"Blue\",\"price\":\"5.00\"},{\"title\":\"Green\",\"price\":\"20.00\"}],\"userErrors\":[]}}}"

  let read_query =
    "query { product(id: \\\""
    <> product_id
    <> "\\\") { priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":3,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":0}}}"
}

pub fn generated_product_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { productCreate(product: { title: \\\"Red shirt\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn generated_product_handles_increment_existing_numeric_suffix_test() {
  let query =
    "mutation { productCreate(product: { title: \\\"Red shirt 2\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn product_set_generated_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Red shirt\\\" }, synchronous: true) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn product_duplicate_generated_handles_increment_numeric_suffixes_test() {
  let source_query =
    "mutation { productCreate(product: { title: \\\"Red shirt\\\" }) { product { id handle } userErrors { field message } } }"
  let copy_query =
    "mutation { productCreate(product: { title: \\\"Red shirt Copy\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: source_status, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(source_query),
    )
  let #(Response(status: first_copy_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(copy_query))
  let #(Response(status: second_copy_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(copy_query))
  let duplicate_query =
    "mutation { productDuplicate(productId: \\\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\\\", newTitle: \\\"Red shirt Copy\\\", synchronous: true) { newProduct { title handle } userErrors { field message } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))

  assert source_status == 200
  assert first_copy_status == 200
  assert second_copy_status == 200
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"productDuplicate\":{\"newProduct\":{\"title\":\"Red shirt Copy\",\"handle\":\"red-shirt-copy-2\"},\"userErrors\":[]}}}"
}

pub fn collection_create_generated_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { collectionCreate(input: { title: \\\"Red shirt\\\" }) { collection { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn collection_create_generated_handles_increment_existing_numeric_suffix_test() {
  let query =
    "mutation { collectionCreate(input: { title: \\\"Red shirt 2\\\" }) { collection { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn explicit_product_handle_collisions_return_user_errors_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let create_query =
    "mutation { productCreate(product: { title: \\\"Explicit Collision\\\", handle: \\\"optioned-board\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(create_query))

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Handle 'optioned-board' already in use. Please provide a new handle.\"}]}}}"

  let set_query =
    "mutation { productSet(input: { title: \\\"Explicit Collision\\\", handle: \\\"optioned-board\\\" }, synchronous: true) { product { handle } userErrors { field message } } }"
  let #(Response(status: set_status, body: set_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(set_query))

  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Handle 'optioned-board' already in use. Please provide a new handle.\"}]}}}"
}

pub fn product_create_validation_branches_return_user_errors_test() {
  let blank_query =
    "mutation { productCreate(product: { title: \\\"\\\" }) { product { id title handle } userErrors { field message } } }"
  let #(Response(status: blank_status, body: blank_body, ..), blank_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(blank_query))
  assert blank_status == 200
  assert json.to_string(blank_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\"}]}}}"
  assert store.get_log(blank_proxy.store)
    |> list.length
    == 1

  let long_handle = string.repeat("a", times: 260)
  let handle_query =
    "mutation { productCreate(product: { title: \\\"Too Long\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { product { id title handle } userErrors { field message } } }"
  let #(Response(status: handle_status, body: handle_body, ..), handle_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(handle_query),
    )
  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\"}]}}}"
  assert store.get_log(handle_proxy.store)
    |> list.length
    == 1

  let variant_query =
    "mutation { productCreate(product: { title: \\\"Invalid Variant Slice\\\", variants: [{ price: \\\"-5\\\" }] }) { product { id } userErrors { field message code } } }"
  let #(Response(status: variant_status, body: variant_body, ..), variant_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(variant_query),
    )
  assert variant_status == 200
  assert json.to_string(variant_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"}]}}}"
  assert store.list_effective_products(variant_proxy.store) == []
}

pub fn collection_create_rejects_long_title_and_handle_test() {
  let long_title = string.repeat("T", times: 256)
  let title_query =
    "mutation { collectionCreate(input: { title: \\\""
    <> long_title
    <> "\\\" }) { collection { id } userErrors { field message code } } }"
  let #(Response(status: title_status, body: title_body, ..), title_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(title_query))

  assert title_status == 200
  assert json.to_string(title_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}]}}}"
  assert store.get_log(title_proxy.store)
    |> list.length
    == 1

  let long_handle = string.repeat("h", times: 256)
  let handle_query =
    "mutation { collectionCreate(input: { title: \\\"Handle Probe\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { collection { id } userErrors { field message code } } }"
  let #(Response(status: handle_status, body: handle_body, ..), handle_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(handle_query),
    )

  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}]}}}"
  assert store.get_log(handle_proxy.store)
    |> list.length
    == 1
}

pub fn collection_create_allows_reserved_like_titles_from_live_probe_test() {
  let create_query =
    "mutation { collectionCreate(input: { title: \\\"Frontpage\\\" }) { collection { id title handle } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/1\",\"title\":\"Frontpage\",\"handle\":\"frontpage\"},\"userErrors\":[]}}}"
  assert store.get_log(proxy.store)
    |> list.length
    == 1

  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let update_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", title: \\\"Vendors\\\" }) { collection { id title } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_query))

  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"title\":\"Vendors\"},\"userErrors\":[]}}}"
}

pub fn collection_create_rejects_invalid_sort_order_as_graphql_error_test() {
  let body =
    "{\"query\":\"mutation($input: CollectionInput!) { collectionCreate(input: $input) { collection { id } userErrors { field message } } }\",\"variables\":{\"input\":{\"title\":\"Sort Probe\",\"sortOrder\":\"INVALID_VALUE\"}}}"

  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request_body(body))

  assert status == 200
  let serialized = json.to_string(response_body)
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(serialized, "\"path\":[\"sortOrder\"]")
  assert string.contains(serialized, "\"title\":\"Sort Probe\"")
  assert string.contains(serialized, "\"sortOrder\":\"INVALID_VALUE\"")
  assert string.contains(
    serialized,
    "Variable $input of type CollectionInput! was provided invalid value for sortOrder",
  )
  assert string.contains(
    serialized,
    "Expected \\\"INVALID_VALUE\\\" to be one of: ALPHA_ASC, ALPHA_DESC, BEST_SELLING, CREATED, CREATED_DESC, MANUAL, PRICE_ASC, PRICE_DESC",
  )
  assert store.get_log(next_proxy.store) == []
}

pub fn collection_create_stages_rule_set_and_unique_handles_test() {
  let first =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle sortOrder } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/1\",\"handle\":\"dedup-probe\",\"sortOrder\":\"BEST_SELLING\"},\"userErrors\":[]}}}"

  let second =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle } userErrors { field message } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/3\",\"handle\":\"dedup-probe-1\"},\"userErrors\":[]}}}"

  let third =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle } userErrors { field message } } }"
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(third))
  assert third_status == 200
  assert json.to_string(third_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/5\",\"handle\":\"dedup-probe-2\"},\"userErrors\":[]}}}"

  let smart =
    "mutation { collectionCreate(input: { title: \\\"Smart Probe\\\", ruleSet: { appliedDisjunctively: false, rules: [{ column: TITLE, relation: CONTAINS, condition: \\\"Probe\\\" }] } }) { collection { id ruleSet { appliedDisjunctively rules { column relation condition } } } userErrors { field message } } }"
  let #(Response(status: smart_status, body: smart_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(smart))
  assert smart_status == 200
  assert json.to_string(smart_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/7\",\"ruleSet\":{\"appliedDisjunctively\":false,\"rules\":[{\"column\":\"TITLE\",\"relation\":\"CONTAINS\",\"condition\":\"Probe\"}]}},\"userErrors\":[]}}}"
}

pub fn collection_add_remove_products_updates_count_and_rejects_smart_collections_test() {
  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let add_query =
    "mutation { collectionAddProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { collection { id products(first: 10) { nodes { id } } } userErrors { field message } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))

  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"collectionAddProducts\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"products\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\"},{\"id\":\"gid://shopify/Product/second\"}]}},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { job { id done } userErrors { field message } } }"
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"collectionRemoveProducts\":{\"job\":{\"id\":\"gid://shopify/Job/2\",\"done\":false},\"userErrors\":[]}}}"

  let read_query =
    "query { collection(id: \\\"gid://shopify/Collection/custom\\\") { id productsCount { count precision } products(first: 10) { nodes { id } } } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"productsCount\":{\"count\":1,\"precision\":\"EXACT\"},\"products\":{\"nodes\":[{\"id\":\"gid://shopify/Product/second\"}]}}}}"

  let smart_add =
    "mutation { collectionAddProducts(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { collection { id } userErrors { field message } } }"
  let #(Response(status: smart_add_status, body: smart_add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(smart_add))
  assert smart_add_status == 200
  assert json.to_string(smart_add_body)
    == "{\"data\":{\"collectionAddProducts\":{\"collection\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually add products to a smart collection\"}]}}}"

  let smart_add_v2 =
    "mutation { collectionAddProductsV2(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { job { id done } userErrors { field message } } }"
  let #(
    Response(status: smart_add_v2_status, body: smart_add_v2_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(smart_add_v2))
  assert smart_add_v2_status == 200
  assert json.to_string(smart_add_v2_body)
    == "{\"data\":{\"collectionAddProductsV2\":{\"job\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually add products to a smart collection\"}]}}}"

  let smart_remove =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { job { id done } userErrors { field message } } }"
  let #(Response(status: smart_remove_status, body: smart_remove_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(smart_remove))
  assert smart_remove_status == 200
  assert json.to_string(smart_remove_body)
    == "{\"data\":{\"collectionRemoveProducts\":{\"job\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually remove products from a smart collection\"}]}}}"
}

pub fn collection_update_rejects_invalid_fields_and_custom_to_smart_switch_test() {
  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let long_title = string.repeat("T", times: 256)
  let title_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", title: \\\""
    <> long_title
    <> "\\\" }) { collection { id } userErrors { field message } } }"
  let #(Response(status: title_status, body: title_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(title_query))
  assert title_status == 200
  assert json.to_string(title_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title is too long (maximum is 255 characters)\"}]}}}"

  let long_handle = string.repeat("h", times: 256)
  let handle_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { collection { id } userErrors { field message } } }"
  let #(Response(status: handle_status, body: handle_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(handle_query))
  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\"}]}}}"

  let switch_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", ruleSet: { appliedDisjunctively: false, rules: [{ column: TITLE, relation: CONTAINS, condition: \\\"Probe\\\" }] } }) { collection { id ruleSet { appliedDisjunctively } } userErrors { field message } } }"
  let #(Response(status: switch_status, body: switch_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(switch_query))
  assert switch_status == 200
  assert json.to_string(switch_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot update rule set of a custom collection\"}]}}}"
}

pub fn product_set_rejects_duplicate_variant_option_tuples_test() {
  // Reproduces the QA-witnessed productSet failure mode where the
  // proxy used to accept input with duplicate variant option-value
  // tuples, only discovering Shopify's userErrors at __meta/commit
  // replay. The local proxy must reject the duplicates immediately
  // and record the entry as Failed so commit replay does not re-send
  // a payload Shopify will also reject. See
  // config/parity-specs/products/productSet-duplicate-variants.json.
  let query =
    "mutation { productSet(input: { title: \\\"Duplicate Variant Probe\\\", status: DRAFT, productOptions: [{ name: \\\"Size\\\", position: 1, values: [{ name: \\\"S\\\" }, { name: \\\"M\\\" }] }, { name: \\\"Color\\\", position: 2, values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }], variants: [{ optionValues: [{ optionName: \\\"Size\\\", name: \\\"S\\\" }, { optionName: \\\"Color\\\", name: \\\"Red\\\" }] }, { optionValues: [{ optionName: \\\"Size\\\", name: \\\"M\\\" }, { optionName: \\\"Color\\\", name: \\\"Blue\\\" }] }, { optionValues: [{ optionName: \\\"Size\\\", name: \\\"S\\\" }, { optionName: \\\"Color\\\", name: \\\"Red\\\" }] }] }, synchronous: true) { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\",\"2\"],\"message\":\"The variant 'S / Red' already exists. Please change at least one option value.\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_set_requires_variants_when_updating_options_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Options Only\\\", status: DRAFT, productOptions: [{ name: \\\"Color\\\", position: 1, values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }, { name: \\\"Size\\\", position: 2, values: [{ name: \\\"Small\\\" }, { name: \\\"Large\\\" }] }] }, synchronous: true) { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\"],\"message\":\"Variants input is required when updating product options\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_set_shape_validator_rejects_collection_limits_test() {
  let too_many_variants =
    string.repeat("{ title: \\\"Overflow\\\" },", times: 2049)
  let variant_query =
    "mutation { productSet(input: { title: \\\"Too Many Variants\\\", vendor: \\\"Hermes\\\", variants: ["
    <> too_many_variants
    <> "] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: variant_status, body: variant_body, ..), variant_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(variant_query),
    )
  let variant_json = json.to_string(variant_body)
  assert variant_status == 200
  assert !string.contains(variant_json, "\"data\"")
  assert string.contains(
    variant_json,
    "\"message\":\"The input array size of 2049 is greater than the maximum allowed of 2048.\"",
  )
  assert string.contains(
    variant_json,
    "\"path\":[\"productSet\",\"input\",\"variants\"]",
  )
  assert string.contains(variant_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(variant_proxy.store) == []

  let too_many_option_values =
    string.repeat("{ name: \\\"Overflow\\\" },", times: 101)
  let too_many_files =
    string.repeat(
      "{ originalSource: \\\"https://example.com/file.jpg\\\" },",
      times: 251,
    )
  let shape_query =
    "mutation { productSet(input: { title: \\\"Shape Limits\\\", vendor: \\\"Hermes\\\", productOptions: [{ name: \\\"Color\\\", position: 1, values: ["
    <> too_many_option_values
    <> "] }, { name: \\\"Size\\\", position: 2, values: [{ name: \\\"Small\\\" }] }, { name: \\\"Material\\\", position: 3, values: [{ name: \\\"Cotton\\\" }] }, { name: \\\"Fit\\\", position: 4, values: [{ name: \\\"Regular\\\" }] }], files: ["
    <> too_many_files
    <> "] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: shape_status, body: shape_body, ..), shape_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(shape_query))
  let shape_json = json.to_string(shape_body)
  assert shape_status == 200
  assert string.contains(shape_json, "\"product\":null")
  assert string.contains(shape_json, "\"field\":[\"input\",\"productOptions\"]")
  assert string.contains(
    shape_json,
    "\"field\":[\"input\",\"productOptions\",\"0\",\"values\"]",
  )
  assert string.contains(shape_json, "\"field\":[\"input\",\"files\"]")
  assert string.contains(shape_json, "\"code\":\"INVALID_INPUT\"")
  let assert [shape_entry] = store.get_log(shape_proxy.store)
  assert shape_entry.operation_name == Some("productSet")
  assert shape_entry.status == store.Failed
  assert shape_entry.staged_resource_ids == []

  let too_many_quantities =
    string.repeat(
      "{ locationId: \\\"gid://shopify/Location/1\\\", name: \\\"available\\\", quantity: 1 },",
      times: 251,
    )
  let inventory_query =
    "mutation { productSet(input: { title: \\\"Too Many Quantities\\\", vendor: \\\"Hermes\\\", variants: [{ inventoryQuantities: ["
    <> too_many_quantities
    <> "] }] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(
    Response(status: inventory_status, body: inventory_body, ..),
    inventory_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(inventory_query),
    )
  let inventory_json = json.to_string(inventory_body)
  assert inventory_status == 200
  assert !string.contains(inventory_json, "\"data\"")
  assert string.contains(
    inventory_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    inventory_json,
    "\"path\":[\"productSet\",\"input\",\"variants\",\"inventoryQuantities\"]",
  )
  assert string.contains(inventory_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(inventory_proxy.store) == []
}

pub fn product_set_rejects_missing_and_suspended_product_references_test() {
  let missing_query =
    "mutation { productSet(input: { id: \\\"gid://shopify/Product/999999999999\\\", title: \\\"Missing\\\", vendor: \\\"Hermes\\\" }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: missing_status, body: missing_body, ..), missing_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_query),
    )
  let missing_json = json.to_string(missing_body)
  assert missing_status == 200
  assert string.contains(missing_json, "\"product\":null")
  assert string.contains(missing_json, "\"field\":[\"input\",\"id\"]")
  assert string.contains(missing_json, "\"code\":\"PRODUCT_DOES_NOT_EXIST\"")
  let assert [missing_entry] = store.get_log(missing_proxy.store)
  assert missing_entry.operation_name == Some("productSet")
  assert missing_entry.status == store.Failed
  assert missing_entry.staged_resource_ids == []

  let suspended_query =
    "mutation { productSet(input: { id: \\\"gid://shopify/Product/suspended\\\", title: \\\"Suspended\\\", vendor: \\\"Hermes\\\" }, synchronous: true) { product { id } userErrors { field message code } } }"
  let suspended_proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: suspended_product_store(),
    )
  let #(
    Response(status: suspended_status, body: suspended_body, ..),
    next_proxy,
  ) =
    draft_proxy.process_request(
      suspended_proxy,
      graphql_request(suspended_query),
    )
  let suspended_json = json.to_string(suspended_body)
  assert suspended_status == 200
  assert string.contains(suspended_json, "\"product\":null")
  assert string.contains(suspended_json, "\"field\":[\"input\"]")
  assert string.contains(suspended_json, "\"code\":\"INVALID_PRODUCT\"")
  let assert [suspended_entry] = store.get_log(next_proxy.store)
  assert suspended_entry.operation_name == Some("productSet")
  assert suspended_entry.status == store.Failed
  assert suspended_entry.staged_resource_ids == []
}

pub fn product_set_async_operation_completes_on_product_operation_read_test() {
  let mutation =
    "mutation { productSet(input: { title: \\\"Async ProductSet\\\", vendor: \\\"Hermes\\\", status: DRAFT }, synchronous: false) { product { id } productSetOperation { id status product { id } userErrors { field message code } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(mutation))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"productSetOperation\":{\"id\":\"gid://shopify/ProductSetOperation/6\",\"status\":\"CREATED\",\"product\":null,\"userErrors\":[]},\"userErrors\":[]}}}"

  let operation_read =
    "query { productOperation(id: \\\"gid://shopify/ProductSetOperation/6\\\") { __typename status product { id title } ... on ProductSetOperation { id userErrors { field message code } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(next_proxy, graphql_request(operation_read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"productOperation\":{\"__typename\":\"ProductSetOperation\",\"status\":\"COMPLETE\",\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Async ProductSet\"},\"id\":\"gid://shopify/ProductSetOperation/6\",\"userErrors\":[]}}}"
}

pub fn product_create_accepts_legacy_input_argument_shape_test() {
  // Real Shopify accepts both `productCreate(product: ProductCreateInput!)`
  // (current) and `productCreate(input: ProductInput!)` (older API versions).
  // The proxy used to read only `product`, fabricating a misleading
  // `["title"], "Title can't be blank"` userError when callers used the
  // legacy `input:` keyword.
  let query =
    "mutation { productCreate(input: { title: \\\"Legacy Shape\\\", status: DRAFT }) { product { id title status } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Legacy Shape\",\"status\":\"DRAFT\"},\"userErrors\":[]}}}"
}

pub fn product_create_missing_input_argument_top_level_error_test() {
  // No `product:` and no `input:` argument → top-level GraphQL error,
  // mirroring real Shopify's `missingRequiredArguments` extension code,
  // rather than fabricating a misleading "Title can't be blank" userError.
  let query =
    "mutation { productCreate { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'productCreate' is missing required arguments: product\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"productCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"productCreate\",\"arguments\":\"product\"}}]}"
}

pub fn product_variant_create_update_delete_stages_lifecycle_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let create_query =
    "mutation { productVariantCreate(input: { productId: \\\"gid://shopify/Product/optioned\\\", title: \\\"Blue\\\", sku: \\\"BLUE-1\\\", barcode: \\\"2222222222222\\\", price: \\\"12.00\\\", inventoryQuantity: 5, selectedOptions: [{ name: \\\"Color\\\", value: \\\"Blue\\\" }], inventoryItem: { tracked: true, requiresShipping: false } }) { product { id totalInventory tracksInventory } productVariant { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } userErrors { field message } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productVariantCreate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":5,\"tracksInventory\":true},\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Blue\",\"sku\":\"BLUE-1\",\"barcode\":\"2222222222222\",\"price\":\"12.00\",\"inventoryQuantity\":5,\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Blue\"}],\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/2\",\"tracked\":true,\"requiresShipping\":false}},\"userErrors\":[]}}}"

  let update_query =
    "mutation { productVariantUpdate(input: { id: \\\"gid://shopify/ProductVariant/1\\\", title: \\\"Blue Deluxe\\\", sku: \\\"BLUE-2\\\", inventoryQuantity: 7, inventoryItem: { tracked: false, requiresShipping: true } }) { product { id totalInventory tracksInventory } productVariant { id title sku inventoryQuantity inventoryItem { id tracked requiresShipping } } userErrors { field message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":0,\"tracksInventory\":false},\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Blue Deluxe\",\"sku\":\"BLUE-2\",\"inventoryQuantity\":7,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/2\",\"tracked\":false,\"requiresShipping\":true}},\"userErrors\":[]}}}"

  let delete_query =
    "mutation { productVariantDelete(id: \\\"gid://shopify/ProductVariant/1\\\") { deletedProductVariantId userErrors { field message } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(delete_query))
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"productVariantDelete\":{\"deletedProductVariantId\":\"gid://shopify/ProductVariant/1\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity } } } productVariant(id: \\\"gid://shopify/ProductVariant/1\\\") { id } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/optioned\",\"title\":\"Red / Small\",\"sku\":null,\"inventoryQuantity\":0}]}},\"productVariant\":null}}"
  assert store.get_log(proxy.store)
    |> list.length
    == 3
}

pub fn product_variants_bulk_create_rejects_invalid_scalar_fields_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let long_text = repeated_text("S", 256)
  let valid_options =
    "selectedOptions: [{ name: \\\"Color\\\", value: \\\"Red\\\" }, { name: \\\"Size\\\", value: \\\"Small\\\" }]"
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ price: null, "
    <> valid_options
    <> " }, { price: \\\"-5\\\", "
    <> valid_options
    <> " }, { price: \\\"1000000000000000000\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", compareAtPrice: \\\"1000000000000000000\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryItem: { measurement: { weight: { value: -1, unit: KILOGRAMS } } }, "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryQuantities: [{ locationId: \\\"gid://shopify/Location/1\\\", availableQuantity: 2000000000 }], "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryItem: { sku: \\\""
    <> long_text
    <> "\\\" }, "
    <> valid_options
    <> " }, { price: \\\"10\\\", barcode: \\\""
    <> long_text
    <> "\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", selectedOptions: [{ name: \\\"Color\\\", value: \\\""
    <> long_text
    <> "\\\" }, { name: \\\"Size\\\", value: \\\"Small\\\" }] }]) { product { id } productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":null,\"productVariants\":[],\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price can't be blank\",\"code\":\"INVALID\"},{\"field\":[\"variants\",\"1\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"2\",\"price\"],\"message\":\"Price must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"3\",\"compareAtPrice\"],\"message\":\"must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"4\"],\"message\":\"Weight must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"5\",\"inventoryQuantities\"],\"message\":\"Inventory quantity must be less than or equal to 1000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"6\"],\"message\":\"SKU is too long (maximum is 255 characters)\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"6\"],\"message\":\"is too long (maximum is 255 characters)\",\"code\":null},{\"field\":[\"variants\",\"7\",\"barcode\"],\"message\":\"Barcode is too long (maximum is 255 characters)\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"8\",\"selectedOptions\",\"0\",\"value\"],\"message\":\"Option value name is too long\",\"code\":\"INVALID_INPUT\"}]}}}"
  assert store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    |> list.length
    == 1
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productVariantsBulkCreate")
  assert entry.status == store.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_variants_bulk_create_rejects_oversized_input_test() {
  let proxy = draft_proxy.new()
  let variants = repeat_csv("{ price: \\\"1\\\" }", 2049)
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: ["
    <> variants
    <> "]) { productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"The input array size of 2049 is greater than the maximum allowed of 2048.\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"productVariantsBulkCreate\",\"variants\"],\"extensions\":{\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"}}]}"
  assert store.get_log(next_proxy.store) == []
}

pub fn product_variants_bulk_create_rejects_cumulative_variant_cap_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: variant_cap_store())
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ price: \\\"1\\\", selectedOptions: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }]) { product { id } productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":null,\"productVariants\":[],\"userErrors\":[{\"field\":null,\"message\":\"You can only have a maximum of 2048 variants per product\",\"code\":\"LIMIT_EXCEEDED\"}]}}}"
  assert store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    |> list.length
    == 2048
}

pub fn product_variants_bulk_update_rejects_invalid_scalar_fields_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productVariantsBulkUpdate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ id: \\\"gid://shopify/ProductVariant/optioned\\\", price: \\\"-5\\\", inventoryQuantity: 1000000001, inventoryItem: { measurement: { weight: { value: 2000000000, unit: STONES } } } }]) { product { id } productVariants { id price inventoryQuantity } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\"},\"productVariants\":null,\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"0\"],\"message\":\"Weight must be less than 2000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"0\"],\"message\":\"Weight unit is not included in the list\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"0\",\"inventoryQuantity\"],\"message\":\"Inventory quantity must be less than or equal to 1000000000\",\"code\":\"INVALID_INPUT\"}]}}}"
  let assert [variant] =
    store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
  assert variant.price == Some("0.00")
  assert variant.inventory_quantity == Some(0)
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store.Failed
}

pub fn product_variant_create_and_update_reject_invalid_scalars_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let create_query =
    "mutation { productVariantCreate(input: { productId: \\\"gid://shopify/Product/optioned\\\", price: \\\"-5\\\", selectedOptions: [{ name: \\\"Color\\\", value: \\\"Blue\\\" }] }) { product { id } productVariant { id } userErrors { field message code } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productVariantCreate\":{\"product\":null,\"productVariant\":null,\"userErrors\":[{\"field\":[\"input\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"}]}}}"

  let update_query =
    "mutation { productVariantUpdate(input: { id: \\\"gid://shopify/ProductVariant/optioned\\\", compareAtPrice: \\\"1000000000000000000\\\" }) { product { id } productVariant { id compareAtPrice } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantUpdate\":{\"product\":null,\"productVariant\":null,\"userErrors\":[{\"field\":[\"input\",\"compareAtPrice\"],\"message\":\"must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"}]}}}"

  let assert [variant] =
    store.get_effective_variants_by_product_id(
      proxy.store,
      "gid://shopify/Product/optioned",
    )
  assert variant.price == Some("0.00")
  assert variant.compare_at_price == None
  let assert [create_entry, update_entry] = store.get_log(proxy.store)
  assert create_entry.status == store.Failed
  assert update_entry.status == store.Failed
}

pub fn product_set_rejects_invalid_variant_scalars_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Scalar Validation Probe\\\", productOptions: [{ name: \\\"Title\\\", position: 1, values: [{ name: \\\"Default Title\\\" }] }], variants: [{ price: \\\"-5\\\", optionValues: [{ optionName: \\\"Title\\\", name: \\\"Default Title\\\" }] }] }, synchronous: true) { product { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"INVALID_VARIANT\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store.Failed
  assert entry.staged_resource_ids == []
}

pub fn inventory_shipment_extended_roots_stage_locally_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let create_query =
    "mutation { inventoryShipmentCreate(input: { movementId: \\\"gid://shopify/InventoryTransfer/7001\\\", trackingInput: { trackingNumber: \\\"1Z999\\\", company: \\\"UPS\\\" }, lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 2 }] }) { inventoryShipment { id status lineItemTotalQuantity tracking { trackingNumber company } lineItems(first: 10) { nodes { id quantity unreceivedQuantity } } } userErrors { field message code } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"inventoryShipmentCreate\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"status\":\"DRAFT\",\"lineItemTotalQuantity\":2,\"tracking\":{\"trackingNumber\":\"1Z999\",\"company\":\"UPS\"},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/2\",\"quantity\":2,\"unreceivedQuantity\":2}]}},\"userErrors\":[]}}}"

  let transit_query =
    "mutation { inventoryShipmentMarkInTransit(id: \\\"gid://shopify/InventoryShipment/1\\\") { inventoryShipment { id status } userErrors { field message code } } }"
  let #(Response(status: transit_status, body: transit_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(transit_query))
  assert transit_status == 200
  assert json.to_string(transit_body)
    == "{\"data\":{\"inventoryShipmentMarkInTransit\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"status\":\"IN_TRANSIT\"},\"userErrors\":[]}}}"

  let add_query =
    "mutation { inventoryShipmentAddItems(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 3 }]) { addedItems { id quantity unreceivedQuantity } inventoryShipment { id lineItemTotalQuantity } userErrors { field message code } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))
  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"inventoryShipmentAddItems\":{\"addedItems\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/5\",\"quantity\":3,\"unreceivedQuantity\":3}],\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"lineItemTotalQuantity\":5},\"userErrors\":[]}}}"

  let tracking_query =
    "mutation { inventoryShipmentSetTracking(id: \\\"gid://shopify/InventoryShipment/1\\\", tracking: { trackingNumber: \\\"TRACK-2\\\", company: \\\"USPS\\\" }) { inventoryShipment { id tracking { trackingNumber company } } userErrors { field message code } } }"
  let #(Response(status: tracking_status, body: tracking_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(tracking_query))
  assert tracking_status == 200
  assert json.to_string(tracking_body)
    == "{\"data\":{\"inventoryShipmentSetTracking\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"tracking\":{\"trackingNumber\":\"TRACK-2\",\"company\":\"USPS\"}},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { inventoryShipmentRemoveItems(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [\\\"gid://shopify/InventoryShipmentLineItem/2\\\"]) { inventoryShipment { id lineItemTotalQuantity lineItems(first: 10) { nodes { id quantity } } } userErrors { field message code } } }"
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"inventoryShipmentRemoveItems\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"lineItemTotalQuantity\":3,\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/5\",\"quantity\":3}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { inventoryItem(id: \\\"gid://shopify/InventoryItem/tracked\\\") { variant { inventoryQuantity } inventoryLevels(first: 1) { nodes { quantities(names: [\\\"available\\\", \\\"on_hand\\\", \\\"incoming\\\"]) { name quantity } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryItem\":{\"variant\":{\"inventoryQuantity\":1},\"inventoryLevels\":{\"nodes\":[{\"quantities\":[{\"name\":\"available\",\"quantity\":1},{\"name\":\"on_hand\",\"quantity\":1},{\"name\":\"incoming\",\"quantity\":3},{\"name\":\"reserved\",\"quantity\":0}]}]}}}}"
  let assert [
    create_entry,
    transit_entry,
    add_entry,
    tracking_entry,
    remove_entry,
  ] = store.get_log(proxy.store)
  assert create_entry.operation_name == Some("inventoryShipmentCreate")
  assert transit_entry.operation_name == Some("inventoryShipmentMarkInTransit")
  assert add_entry.operation_name == Some("inventoryShipmentAddItems")
  assert tracking_entry.operation_name == Some("inventoryShipmentSetTracking")
  assert remove_entry.operation_name == Some("inventoryShipmentRemoveItems")
  assert string.contains(create_entry.query, "inventoryShipmentCreate")
  assert string.contains(remove_entry.query, "inventoryShipmentRemoveItems")
}

pub fn inventory_set_quantities_validates_name_quantity_and_duplicates_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())

  let invalid_name_query =
    "mutation { inventorySetQuantities(input: { name: \\\"damaged\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 5 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: invalid_name_status, body: invalid_name_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(invalid_name_query))
  assert invalid_name_status == 200
  assert json.to_string(invalid_name_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"The quantity name must be either 'available' or 'on_hand'.\",\"code\":\"INVALID_NAME\"}]}}}"

  let too_high_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 1000000001 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: too_high_status, body: too_high_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(too_high_query))
  assert too_high_status == 200
  assert json.to_string(too_high_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"quantity\"],\"message\":\"The quantity can't be higher than 1,000,000,000.\",\"code\":\"INVALID_QUANTITY_TOO_HIGH\"}]}}}"

  let negative_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: -1 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: negative_status, body: negative_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(negative_query))
  assert negative_status == 200
  assert json.to_string(negative_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"quantity\"],\"message\":\"The quantity can't be negative.\",\"code\":\"INVALID_QUANTITY_NEGATIVE\"}]}}}"

  let duplicate_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 2 }, { inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 3 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"locationId\"],\"message\":\"The combination of inventoryItemId and locationId must be unique.\",\"code\":\"NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR\"},{\"field\":[\"input\",\"quantities\",\"1\",\"locationId\"],\"message\":\"The combination of inventoryItemId and locationId must be unique.\",\"code\":\"NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR\"}]}}}"
}

pub fn inventory_set_and_adjust_quantities_accept_on_hand_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())

  let set_query =
    "mutation { inventorySetQuantities(input: { name: \\\"on_hand\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 4 }] }) { inventoryAdjustmentGroup { changes { name delta item { id } location { id name } } } userErrors { field message code } } }"
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(set_query))
  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"available\",\"delta\":3,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}},{\"name\":\"on_hand\",\"delta\":3,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}}]},\"userErrors\":[]}}}"

  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"on_hand\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", delta: 2, ledgerDocumentUri: \\\"ledger://har-568/on-hand\\\" }] }) { inventoryAdjustmentGroup { changes { name delta ledgerDocumentUri item { id } location { id name } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(adjust_query))
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"on_hand\",\"delta\":2,\"ledgerDocumentUri\":\"ledger://har-568/on-hand\",\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}}]},\"userErrors\":[]}}}"
}

pub fn inventory_quantity_mutations_recompute_product_stock_fields_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", delta: -1 }] }) { inventoryAdjustmentGroup { changes { name delta item { id } location { id } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(adjust_query))
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"available\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}},{\"name\":\"on_hand\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}}]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/tracked\\\") { totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 5) { nodes { inventoryQuantity inventoryItem { tracked } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"totalVariants\":1,\"hasOnlyDefaultVariant\":true,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":0,\"variants\":{\"nodes\":[{\"inventoryQuantity\":0,\"inventoryItem\":{\"tracked\":true}}]}}}}"
}

pub fn inventory_deactivate_unknown_level_returns_item_error_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/999999999999?inventory_item_id=999999999998\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked because the product was deleted.\"}]}}}"
}

pub fn inventory_deactivate_known_item_missing_level_returns_location_deleted_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/deleted-location?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked because the location was deleted.\"}]}}}"
}

pub fn inventory_deactivate_allows_non_zero_quantities_test() {
  let target_level =
    inventory_level(
      "gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked",
      "gid://shopify/Location/1",
      "Shop location",
      True,
      [
        inventory_quantity("available", 1),
        inventory_quantity("on_hand", 10),
        inventory_quantity("committed", 2),
        inventory_quantity("incoming", 3),
        inventory_quantity("reserved", 4),
      ],
    )
  let alternate_level =
    inventory_level(
      "gid://shopify/InventoryLevel/alternate?inventory_item_id=tracked",
      "gid://shopify/Location/2",
      "Second location",
      True,
      [
        inventory_quantity("available", 1),
        inventory_quantity("on_hand", 1),
      ],
    )
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_levels([
        target_level,
        alternate_level,
      ]),
    )
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { inventoryLevel(id: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { id isActive } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":false}}}"
}

pub fn inventory_deactivate_only_location_stays_active_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked from Shop location because products need to be stocked at a minimum of 1 location.\"}]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { inventoryLevel(id: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { id isActive } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":true}}}"
}

pub fn inventory_activate_available_conflict_requires_active_level_test() {
  let active_proxy = draft_proxy.new()
  let active_proxy =
    proxy_state.DraftProxy(..active_proxy, store: tracked_inventory_store())
  let active_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", available: 7) { inventoryLevel { id isActive } userErrors { field message } } }"

  let #(Response(status: active_status, body: active_body, ..), _) =
    draft_proxy.process_request(active_proxy, graphql_request(active_query))

  assert active_status == 200
  assert json.to_string(active_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":true},\"userErrors\":[{\"field\":[\"available\"],\"message\":\"Not allowed to set available quantity when the item is already active at the location.\"}]}}}"

  let inactive_level =
    inventory_level(
      "gid://shopify/InventoryLevel/inactive?inventory_item_id=tracked",
      "gid://shopify/Location/2",
      "Second location",
      False,
      [
        inventory_quantity("available", 0),
        inventory_quantity("on_hand", 0),
      ],
    )
  let inactive_proxy = draft_proxy.new()
  let inactive_proxy =
    proxy_state.DraftProxy(
      ..inactive_proxy,
      store: tracked_inventory_store_with_levels([
        tracked_inventory_level(),
        inactive_level,
      ]),
    )
  let inactive_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/2\\\", available: 7) { inventoryLevel { id isActive } userErrors { field message } } }"

  let #(Response(status: inactive_status, body: inactive_body, ..), _) =
    draft_proxy.process_request(inactive_proxy, graphql_request(inactive_query))

  assert inactive_status == 200
  assert json.to_string(inactive_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/inactive?inventory_item_id=tracked\",\"isActive\":true},\"userErrors\":[]}}}"
}

pub fn inventory_transfer_edit_and_duplicate_stage_locally_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let create_query =
    "mutation { inventoryTransferCreate(input: { originLocationId: \\\"gid://shopify/Location/1\\\", destinationLocationId: \\\"gid://shopify/Location/2\\\", referenceName: \\\"HAR-515\\\", note: \\\"local transfer\\\", tags: [\\\"har-515\\\"], lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 2 }] }) { inventoryTransfer { id name referenceName note tags status lineItems(first: 5) { nodes { id totalQuantity } } } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"inventoryTransferCreate\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/1\",\"name\":\"#T0001\",\"referenceName\":\"HAR-515\",\"note\":\"local transfer\",\"tags\":[\"har-515\"],\"status\":\"DRAFT\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryTransferLineItem/2?shopify-draft-proxy=synthetic\",\"totalQuantity\":2}]}},\"userErrors\":[]}}}"

  let edit_query =
    "mutation { inventoryTransferEdit(id: \\\"gid://shopify/InventoryTransfer/1\\\", input: { note: \\\"edited transfer\\\", tags: [\\\"har-515\\\", \\\"edited\\\"] }) { inventoryTransfer { id referenceName note tags } userErrors { field message code } } }"
  let #(Response(status: edit_status, body: edit_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(edit_query))
  assert edit_status == 200
  assert json.to_string(edit_body)
    == "{\"data\":{\"inventoryTransferEdit\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/1\",\"referenceName\":\"HAR-515\",\"note\":\"edited transfer\",\"tags\":[\"har-515\",\"edited\"]},\"userErrors\":[]}}}"

  let duplicate_query =
    "mutation { inventoryTransferDuplicate(id: \\\"gid://shopify/InventoryTransfer/1\\\") { inventoryTransfer { id name status note tags lineItems(first: 5) { nodes { id totalQuantity } } } userErrors { field message code } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"inventoryTransferDuplicate\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/5\",\"name\":\"#T0002\",\"status\":\"DRAFT\",\"note\":\"edited transfer\",\"tags\":[\"har-515\",\"edited\"],\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryTransferLineItem/6?shopify-draft-proxy=synthetic\",\"totalQuantity\":2}]}},\"userErrors\":[]}}}"
  let assert [create_entry, edit_entry, duplicate_entry] =
    store.get_log(proxy.store)
  assert create_entry.operation_name == Some("inventoryTransferCreate")
  assert edit_entry.operation_name == Some("inventoryTransferEdit")
  assert duplicate_entry.operation_name == Some("inventoryTransferDuplicate")
  assert string.contains(create_entry.query, "inventoryTransferCreate")
  assert string.contains(edit_entry.query, "inventoryTransferEdit")
  assert string.contains(duplicate_entry.query, "inventoryTransferDuplicate")
}

fn default_option_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([default_product()])
  |> store.upsert_base_product_variants([default_variant()])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/default",
      product_id: "gid://shopify/Product/optioned",
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/default-title",
          name: "Default Title",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn tagged_product_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_products([
    ProductRecord(..default_product(), tags: ["existing", "sale", "summer"]),
  ])
}

fn metafield_store() -> store.Store {
  default_option_store()
  |> store.replace_base_metafields_for_owner("gid://shopify/Product/optioned", [
    ProductMetafieldRecord(
      id: "gid://shopify/Metafield/material",
      owner_id: "gid://shopify/Product/optioned",
      namespace: "custom",
      key: "material",
      type_: Some("single_line_text_field"),
      value: Some("Canvas"),
      compare_digest: Some("digest-material"),
      json_value: None,
      created_at: None,
      updated_at: None,
      owner_type: Some("PRODUCT"),
      market_localizable_content: [],
    ),
    ProductMetafieldRecord(
      id: "gid://shopify/Metafield/origin",
      owner_id: "gid://shopify/Product/optioned",
      namespace: "details",
      key: "origin",
      type_: Some("single_line_text_field"),
      value: Some("VN"),
      compare_digest: Some("digest-origin"),
      json_value: None,
      created_at: None,
      updated_at: None,
      owner_type: Some("PRODUCT"),
      market_localizable_content: [],
    ),
  ])
}

fn definition_validation_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_metafield_definitions([
    metafield_definition("loyalty", "min_tier", "number_integer", [
      MetafieldDefinitionValidationRecord(name: "min", value: Some("2")),
    ]),
    metafield_definition("loyalty", "max_tier", "number_integer", [
      MetafieldDefinitionValidationRecord(name: "max", value: Some("5")),
    ]),
    metafield_definition("loyalty", "sku_code", "single_line_text_field", [
      MetafieldDefinitionValidationRecord(
        name: "regex",
        value: Some("^[A-Z]+$"),
      ),
    ]),
    metafield_definition("loyalty", "plan", "single_line_text_field", [
      MetafieldDefinitionValidationRecord(
        name: "allowed_list",
        value: Some("[\"gold\",\"silver\"]"),
      ),
    ]),
  ])
}

fn metafield_definition(
  namespace: String,
  key: String,
  type_name: String,
  validations: List(MetafieldDefinitionValidationRecord),
) -> MetafieldDefinitionRecord {
  MetafieldDefinitionRecord(
    id: "gid://shopify/MetafieldDefinition/" <> namespace <> "-" <> key,
    name: key,
    namespace: namespace,
    key: key,
    owner_type: "PRODUCT",
    type_: MetafieldDefinitionTypeRecord(name: type_name, category: None),
    description: None,
    validations: validations,
    access: dict.new(),
    capabilities: default_metafield_definition_capabilities(),
    constraints: None,
    pinned_position: None,
    validation_status: "ALL_VALID",
  )
}

fn default_metafield_definition_capabilities() -> MetafieldDefinitionCapabilitiesRecord {
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: default_metafield_definition_capability(),
    smart_collection_condition: default_metafield_definition_capability(),
    unique_values: default_metafield_definition_capability(),
  )
}

fn default_metafield_definition_capability() -> MetafieldDefinitionCapabilityRecord {
  MetafieldDefinitionCapabilityRecord(
    enabled: False,
    eligible: True,
    status: None,
  )
}

fn metafields_set_input(
  owner_id: String,
  namespace: String,
  key: String,
  type_name: String,
  value: String,
) -> String {
  "{ ownerId: \""
  <> owner_id
  <> "\", namespace: \""
  <> namespace
  <> "\", key: \""
  <> key
  <> "\", type: \""
  <> type_name
  <> "\", value: \""
  <> value
  <> "\" }"
}

fn variant_cap_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_product_variants(variant_cap_records(2047))
}

fn variant_cap_records(count: Int) -> List(ProductVariantRecord) {
  variant_cap_records_loop(count, [])
}

fn variant_cap_records_loop(
  remaining: Int,
  acc: List(ProductVariantRecord),
) -> List(ProductVariantRecord) {
  case remaining <= 0 {
    True -> acc
    False -> {
      let id = int.to_string(remaining)
      variant_cap_records_loop(remaining - 1, [
        ProductVariantRecord(
          ..default_variant(),
          id: "gid://shopify/ProductVariant/cap-" <> id,
          title: "Variant " <> id,
        ),
        ..acc
      ])
    }
  }
}

fn repeated_text(item: String, count: Int) -> String {
  repeated_text_loop(item, count, "")
}

fn repeated_text_loop(item: String, remaining: Int, acc: String) -> String {
  case remaining <= 0 {
    True -> acc
    False -> repeated_text_loop(item, remaining - 1, acc <> item)
  }
}

fn repeat_csv(item: String, count: Int) -> String {
  repeat_csv_loop(item, count, "")
}

fn repeat_csv_loop(item: String, remaining: Int, acc: String) -> String {
  case remaining <= 0 {
    True -> acc
    False -> {
      let next = case acc == "" {
        True -> item
        False -> acc <> ", " <> item
      }
      repeat_csv_loop(item, remaining - 1, next)
    }
  }
}

fn suspended_product_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/suspended",
      handle: "suspended-product",
      status: "SUSPENDED",
    ),
  ])
}

fn collection_membership_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([
    default_product(),
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/second",
      title: "Second Product",
      handle: "second-product",
    ),
  ])
  |> store.upsert_base_collections([
    collection_record(
      "gid://shopify/Collection/custom",
      "Custom",
      "custom",
      None,
    ),
    collection_record(
      "gid://shopify/Collection/smart",
      "Smart",
      "smart",
      Some(
        CollectionRuleSetRecord(applied_disjunctively: False, rules: [
          CollectionRuleRecord(
            column: "TITLE",
            relation: "CONTAINS",
            condition: "Product",
          ),
        ]),
      ),
    ),
  ])
  |> store.upsert_base_product_collections([
    ProductCollectionRecord(
      collection_id: "gid://shopify/Collection/custom",
      product_id: "gid://shopify/Product/optioned",
      position: 0,
      cursor: None,
    ),
    ProductCollectionRecord(
      collection_id: "gid://shopify/Collection/smart",
      product_id: "gid://shopify/Product/optioned",
      position: 0,
      cursor: None,
    ),
  ])
}

fn collection_record(
  id: String,
  title: String,
  handle: String,
  rule_set: Option(CollectionRuleSetRecord),
) -> CollectionRecord {
  CollectionRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: handle,
    publication_ids: [],
    updated_at: None,
    description: None,
    description_html: Some(""),
    image: None,
    sort_order: Some("BEST_SELLING"),
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    rule_set: rule_set,
    products_count: Some(1),
    is_smart: option.is_some(rule_set),
    cursor: None,
    title_cursor: None,
    updated_at_cursor: None,
  )
}

fn inventory_quantity(name: String, quantity: Int) -> InventoryQuantityRecord {
  InventoryQuantityRecord(name: name, quantity: quantity, updated_at: None)
}

fn inventory_level(
  id: String,
  location_id: String,
  location_name: String,
  is_active: Bool,
  quantities: List(InventoryQuantityRecord),
) -> InventoryLevelRecord {
  InventoryLevelRecord(
    id: id,
    cursor: None,
    is_active: Some(is_active),
    location: InventoryLocationRecord(id: location_id, name: location_name),
    quantities: quantities,
  )
}

fn tracked_inventory_level() -> InventoryLevelRecord {
  inventory_level(
    "gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked",
    "gid://shopify/Location/1",
    "Shop location",
    True,
    [
      inventory_quantity("available", 1),
      inventory_quantity("on_hand", 1),
      inventory_quantity("incoming", 0),
      inventory_quantity("reserved", 0),
    ],
  )
}

fn tracked_inventory_store() -> store.Store {
  tracked_inventory_store_with_levels([tracked_inventory_level()])
}

fn tracked_inventory_store_with_levels(
  levels: List(InventoryLevelRecord),
) -> store.Store {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/tracked",
      title: "Tracked Product",
      handle: "tracked-product",
      total_inventory: Some(1),
      tracks_inventory: Some(True),
    ),
  ])
  |> store.upsert_base_product_variants([
    tracked_inventory_variant_with_levels(levels),
  ])
}

fn option_update_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([default_product()])
  |> store.upsert_base_product_variants([option_update_variant()])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
        ),
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/green",
          name: "Green",
          has_variants: False,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/size",
      product_id: "gid://shopify/Product/optioned",
      name: "Size",
      position: 2,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/small",
          name: "Small",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn three_option_store() -> store.Store {
  option_update_store()
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/size",
      product_id: "gid://shopify/Product/optioned",
      name: "Size",
      position: 2,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/small",
          name: "Small",
          has_variants: True,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/material",
      product_id: "gid://shopify/Product/optioned",
      name: "Material",
      position: 3,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/cotton",
          name: "Cotton",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn tracked_inventory_variant_with_levels(
  levels: List(InventoryLevelRecord),
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/tracked",
    product_id: "gid://shopify/Product/tracked",
    title: "Default Title",
    sku: Some("TRACKED-SKU"),
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(1),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Title", value: "Default Title"),
    ],
    media_ids: [],
    inventory_item: Some(InventoryItemRecord(
      id: "gid://shopify/InventoryItem/tracked",
      tracked: Some(True),
      requires_shipping: Some(True),
      measurement: None,
      country_code_of_origin: None,
      province_code_of_origin: None,
      harmonized_system_code: None,
      inventory_levels: levels,
    )),
    contextual_pricing: None,
    cursor: None,
  )
}

fn default_product() -> ProductRecord {
  ProductRecord(
    id: "gid://shopify/Product/optioned",
    legacy_resource_id: None,
    title: "Optioned Board",
    handle: "optioned-board",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: ["existing"],
    price_range_min: None,
    price_range_max: None,
    total_variants: None,
    has_only_default_variant: None,
    has_out_of_stock_variants: None,
    total_inventory: Some(0),
    tracks_inventory: Some(False),
    created_at: None,
    updated_at: None,
    published_at: None,
    description_html: "",
    online_store_preview_url: None,
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    category: None,
    publication_ids: [],
    contextual_pricing: None,
    cursor: None,
  )
}

fn option_update_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/optioned",
    product_id: "gid://shopify/Product/optioned",
    title: "Red / Small",
    sku: None,
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Color", value: "Red"),
      ProductVariantSelectedOptionRecord(name: "Size", value: "Small"),
    ],
    media_ids: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/optioned",
        tracked: Some(False),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
    contextual_pricing: None,
    cursor: None,
  )
}

fn default_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/default",
    product_id: "gid://shopify/Product/optioned",
    title: "Default Title",
    sku: None,
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Title", value: "Default Title"),
    ],
    media_ids: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/default",
        tracked: Some(False),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
    contextual_pricing: None,
    cursor: None,
  )
}

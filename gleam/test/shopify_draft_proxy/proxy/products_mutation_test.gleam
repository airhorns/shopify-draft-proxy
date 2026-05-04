import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy.{type Request}
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type ProductRecord, type ProductVariantRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryQuantityRecord,
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
    == "{\"data\":{\"productVariantUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":7,\"tracksInventory\":false},\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Blue Deluxe\",\"sku\":\"BLUE-2\",\"inventoryQuantity\":7,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/2\",\"tracked\":false,\"requiresShipping\":true}},\"userErrors\":[]}}}"

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
    ),
  ])
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

fn tracked_inventory_store() -> store.Store {
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
  |> store.upsert_base_product_variants([tracked_inventory_variant()])
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

fn tracked_inventory_variant() -> ProductVariantRecord {
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
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/tracked",
        tracked: Some(True),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [
          InventoryLevelRecord(
            id: "gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked",
            cursor: None,
            is_active: Some(True),
            location: InventoryLocationRecord(
              id: "gid://shopify/Location/1",
              name: "Shop location",
            ),
            quantities: [
              InventoryQuantityRecord(
                name: "available",
                quantity: 1,
                updated_at: None,
              ),
              InventoryQuantityRecord(
                name: "on_hand",
                quantity: 1,
                updated_at: None,
              ),
              InventoryQuantityRecord(
                name: "incoming",
                quantity: 0,
                updated_at: None,
              ),
              InventoryQuantityRecord(
                name: "reserved",
                quantity: 0,
                updated_at: None,
              ),
            ],
          ),
        ],
      ),
    ),
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

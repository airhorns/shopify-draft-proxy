import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/draft_proxy.{type Request, Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type ProductRecord, type ProductVariantRecord, InventoryItemRecord,
  ProductOptionRecord, ProductOptionValueRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord,
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: default_option_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: option_update_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: option_update_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: option_update_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: default_option_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: default_option_store())
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

pub fn tags_add_stages_tags_and_preserves_base_tag_search_lag_test() {
  let proxy = draft_proxy.new()
  let proxy = draft_proxy.DraftProxy(..proxy, store: default_option_store())
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
  let proxy = draft_proxy.DraftProxy(..proxy, store: tagged_product_store())
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
    description_html: "",
    online_store_preview_url: None,
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    category: None,
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
    cursor: None,
  )
}

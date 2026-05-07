import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, DraftProxy, Request, Response,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord, type MarketRecord,
  type PriceListRecord, type ProductMetafieldRecord, type ProductRecord,
  type ProductVariantRecord, type PublicationRecord, type ShopRecord,
  type WebPresenceRecord, CapturedArray, CapturedBool, CapturedInt, CapturedNull,
  CapturedObject, CapturedString, CatalogRecord, InventoryItemRecord,
  MarketLocalizableContentRecord, MarketRecord, PaymentSettingsRecord,
  PriceListRecord, ProductMetafieldRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord, PublicationRecord,
  ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopRecord,
  ShopResourceLimitsRecord, WebPresenceRecord,
}

fn graphql(query: String) {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  graphql_with_proxy(proxy, query)
}

fn graphql_with_proxy(proxy: DraftProxy, query: String) {
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

pub fn price_list_create_matches_parent_adjustment_value_bounds_test() {
  let #(Response(status: zero_status, body: zero_body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"Zero\", currency: USD, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 0 } } }) { priceList { id } userErrors { field message code } } }",
    )
  let #(Response(status: negative_status, body: negative_body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"Negative\", currency: USD, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: -10 } } }) { priceList { id } userErrors { field message code } } }",
    )
  let #(Response(status: decrease_status, body: decrease_body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"Too Low\", currency: USD, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 250 } } }) { priceList { id } userErrors { field message code } } }",
    )
  let #(Response(status: increase_status, body: increase_body, ..), _) =
    graphql(
      "mutation { priceListCreate(input: { name: \"Too High\", currency: USD, parent: { adjustment: { type: PERCENTAGE_INCREASE, value: 5000 } } }) { priceList { id } userErrors { field message code } } }",
    )

  assert zero_status == 200
  assert json.to_string(zero_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/1\"},\"userErrors\":[]}}}"
  assert negative_status == 200
  assert json.to_string(negative_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"parent\",\"adjustment\",\"value\"],\"message\":\"The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.\",\"code\":\"INVALID_ADJUSTMENT_VALUE\"}]}}}"
  assert decrease_status == 200
  assert json.to_string(decrease_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"parent\",\"adjustment\",\"value\"],\"message\":\"The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.\",\"code\":\"INVALID_ADJUSTMENT_VALUE\"}]}}}"
  assert increase_status == 200
  assert json.to_string(increase_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":null,\"userErrors\":[{\"field\":[\"input\",\"parent\",\"adjustment\",\"value\"],\"message\":\"The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.\",\"code\":\"INVALID_ADJUSTMENT_VALUE\"}]}}}"
}

pub fn price_list_create_and_update_allow_catalog_market_currency_mismatch_test() {
  let #(Response(status: valid_status, body: valid_body, ..), _) =
    graphql_with_proxy(
      catalog_price_list_proxy(),
      "mutation { priceListCreate(input: { name: \"CAD Catalog\", currency: CAD, catalogId: \"gid://shopify/MarketCatalog/200\", parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id currency catalog { id } } userErrors { field message code } } }",
    )
  let #(Response(status: create_status, body: create_body, ..), _) =
    graphql_with_proxy(
      catalog_price_list_proxy(),
      "mutation { priceListCreate(input: { name: \"USD Catalog\", currency: USD, catalogId: \"gid://shopify/MarketCatalog/200\", parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id currency catalog { id } } userErrors { field message code } } }",
    )
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_proxy(
      catalog_price_list_proxy(),
      "mutation { priceListUpdate(id: \"gid://shopify/PriceList/300\", input: { currency: USD }) { priceList { id currency catalog { id } } userErrors { field message code } } }",
    )

  assert valid_status == 200
  assert json.to_string(valid_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/1\",\"currency\":\"CAD\",\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/200\"}},\"userErrors\":[]}}}"
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/1\",\"currency\":\"USD\",\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/200\"}},\"userErrors\":[]}}}"
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"priceListUpdate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/300\",\"currency\":\"USD\",\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/200\"}},\"userErrors\":[]}}}"
}

pub fn price_list_update_revalidates_existing_parent_adjustment_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      invalid_adjustment_price_list_proxy(),
      "mutation { priceListUpdate(id: \"gid://shopify/PriceList/400\", input: { name: \"Still invalid\" }) { priceList { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"priceListUpdate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/400\"},\"userErrors\":[{\"field\":[\"input\",\"parent\",\"adjustment\",\"value\"],\"message\":\"The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.\",\"code\":\"INVALID_ADJUSTMENT_VALUE\"}]}}}"
}

pub fn quantity_rules_add_validates_numeric_inputs_test() {
  let #(proxy, price_list_id, variant_id) = quantity_rules_subject()
  let #(Response(status: minimum_status, body: minimum_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 0, increment: 1",
      ),
    )
  let #(Response(status: increment_status, body: increment_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 1, increment: 0",
      ),
    )
  let #(Response(status: range_status, body: range_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 10, maximum: 5, increment: 1",
      ),
    )
  let #(
    Response(status: minimum_multiple_status, body: minimum_multiple_body, ..),
    proxy,
  ) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 5, increment: 3",
      ),
    )
  let #(
    Response(status: maximum_multiple_status, body: maximum_multiple_body, ..),
    _,
  ) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 6, maximum: 10, increment: 3",
      ),
    )

  assert minimum_status == 200
  assert increment_status == 200
  assert range_status == 200
  assert minimum_multiple_status == 200
  assert maximum_multiple_status == 200
  assert string.contains(
    json.to_string(minimum_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"minimum\"],\"message\":\"Minimum must be greater than or equal to one.\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"",
  )
  assert string.contains(
    json.to_string(minimum_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"increment\"],\"message\":\"Increment must be lower than or equal to the minimum.\",\"code\":\"INCREMENT_IS_GREATER_THAN_MINIMUM\"",
  )
  assert string.contains(
    json.to_string(increment_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"increment\"],\"message\":\"Increment must be greater than or equal to one.\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"",
  )
  assert string.contains(
    json.to_string(range_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"minimum\"],\"message\":\"Minimum must be lower than or equal to the maximum.\",\"code\":\"MINIMUM_IS_GREATER_THAN_MAXIMUM\"",
  )
  assert string.contains(
    json.to_string(minimum_multiple_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"minimum\"],\"message\":\"Minimum must be a multiple of the increment.\",\"code\":\"MINIMUM_NOT_MULTIPLE_OF_INCREMENT\"",
  )
  assert string.contains(
    json.to_string(maximum_multiple_body),
    "\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"maximum\"],\"message\":\"Maximum must be a multiple of the increment.\",\"code\":\"MAXIMUM_NOT_MULTIPLE_OF_INCREMENT\"",
  )
}

pub fn quantity_rules_add_rejects_duplicate_variant_ids_test() {
  let #(proxy, price_list_id, variant_id) = quantity_rules_subject()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { quantityRulesAdd(priceListId: \""
        <> price_list_id
        <> "\", quantityRules: [{ variantId: \""
        <> variant_id
        <> "\", minimum: 2, increment: 1 }, { variantId: \""
        <> variant_id
        <> "\", minimum: 4, increment: 1 }]) { quantityRules { minimum increment productVariant { id } } userErrors { __typename field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"quantityRulesAdd\":{\"quantityRules\":[],\"userErrors\":[{\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"variantId\"],\"message\":\"Quantity rule inputs must be unique by variant id.\",\"code\":\"DUPLICATE_INPUT_FOR_VARIANT\"},{\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"1\",\"variantId\"],\"message\":\"Quantity rule inputs must be unique by variant id.\",\"code\":\"DUPLICATE_INPUT_FOR_VARIANT\"}]}}}"
}

pub fn quantity_rules_add_rejects_unknown_price_list_test() {
  let #(proxy, _, variant_id) = quantity_rules_subject()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        "gid://shopify/PriceList/999",
        variant_id,
        "minimum: 2, maximum: 10, increment: 2",
      ),
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"quantityRulesAdd\":{\"quantityRules\":[],\"userErrors\":[{\"__typename\":\"QuantityRuleUserError\",\"field\":[\"priceListId\"],\"message\":\"Price list does not exist.\",\"code\":\"PRICE_LIST_DOES_NOT_EXIST\"}]}}}"
}

pub fn quantity_rules_add_still_stages_valid_rules_test() {
  let #(proxy, price_list_id, variant_id) = quantity_rules_subject()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        price_list_id,
        variant_id,
        "minimum: 2, maximum: 10, increment: 2",
      ),
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"quantityRulesAdd\":{\"quantityRules\":[{\"minimum\":2,\"maximum\":10,\"increment\":2,\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/4\"}}],\"userErrors\":[]}}}"
}

pub fn quantity_rules_add_rejects_maximum_below_existing_price_break_test() {
  let proxy = product_bulk_fixed_price_proxy_with_quantity_break(10)
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      quantity_rules_add_mutation(
        "gid://shopify/PriceList/test",
        "gid://shopify/ProductVariant/test",
        "minimum: 1, maximum: 5, increment: 1",
      ),
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/test\") { quantityRules(first: 10) { edges { node { maximum productVariant { id } } } } } }",
    )

  assert status == 200
  assert read_status == 200
  assert json.to_string(body)
    == "{\"data\":{\"quantityRulesAdd\":{\"quantityRules\":[],\"userErrors\":[{\"__typename\":\"QuantityRuleUserError\",\"field\":[\"quantityRules\",\"0\",\"maximum\"],\"message\":\"Maximum must be greater than or equal to all quantity price break minimums associated with this variant in the specified price list.\",\"code\":\"MAXIMUM_IS_LOWER_THAN_QUANTITY_PRICE_BREAK_MINIMUM\"}]}}}"
  assert json.to_string(read_body)
    == "{\"data\":{\"priceList\":{\"quantityRules\":null}}}"
}

pub fn quantity_rules_delete_rejects_variant_without_existing_rule_test() {
  let proxy = product_bulk_fixed_price_proxy()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { quantityRulesDelete(priceListId: \"gid://shopify/PriceList/test\", variantIds: [\"gid://shopify/ProductVariant/test\"]) { deletedQuantityRulesVariantIds userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"quantityRulesDelete\":{\"deletedQuantityRulesVariantIds\":[],\"userErrors\":[{\"field\":[\"variantIds\",\"0\"],\"message\":\"Quantity rule for variant associated with the price list provided does not exist.\",\"code\":\"VARIANT_QUANTITY_RULE_DOES_NOT_EXIST\"}]}}}"
}

fn quantity_rules_subject() -> #(DraftProxy, String, String) {
  let #(Response(status: product_status, body: product_body, ..), proxy) =
    graphql(
      "mutation { productCreate(product: { title: \"Rule Product\" }) { product { id variants(first: 1) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: price_status, body: price_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { priceListCreate(input: { name: \"USD\", currency: USD, parent: { adjustment: { type: PERCENTAGE_DECREASE, value: 10 } } }) { priceList { id } userErrors { field message code } } }",
    )

  assert product_status == 200
  assert price_status == 200
  assert string.contains(
    json.to_string(product_body),
    "\"id\":\"gid://shopify/ProductVariant/4\"",
  )
  assert json.to_string(price_body)
    == "{\"data\":{\"priceListCreate\":{\"priceList\":{\"id\":\"gid://shopify/PriceList/7\"},\"userErrors\":[]}}}"
  #(proxy, "gid://shopify/PriceList/7", "gid://shopify/ProductVariant/4")
}

fn quantity_rules_add_mutation(
  price_list_id: String,
  variant_id: String,
  quantity_rule_fields: String,
) -> String {
  "mutation { quantityRulesAdd(priceListId: \""
  <> price_list_id
  <> "\", quantityRules: [{ variantId: \""
  <> variant_id
  <> "\", "
  <> quantity_rule_fields
  <> " }]) { quantityRules { minimum maximum increment productVariant { id } } userErrors { __typename field message code } } }"
}

pub fn price_list_fixed_prices_by_product_update_rejects_noop_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      product_bulk_fixed_price_proxy(),
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/test\", pricesToAdd: [], pricesToDeleteByProductIds: []) { priceList { id fixedPricesCount } pricesToAddProducts { id } pricesToDeleteProducts { id } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/test\") { fixedPricesCount } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"priceList\":null")
  assert string.contains(
    serialized,
    "\"__typename\":\"PriceListFixedPricesByProductBulkUpdateUserError\"",
  )
  assert string.contains(
    serialized,
    "\"field\":null,\"code\":\"NO_UPDATE_OPERATIONS_SPECIFIED\"",
  )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"priceList\":{\"fixedPricesCount\":0}}}"
}

pub fn price_list_fixed_prices_by_product_update_validates_input_sets_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      product_bulk_fixed_price_proxy(),
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/test\", pricesToAdd: [{ productId: \"gid://shopify/Product/test\", price: { amount: \"12.00\", currencyCode: USD }, compareAtPrice: { amount: \"15.00\", currencyCode: GBP } }, { productId: \"gid://shopify/Product/test\", price: { amount: \"13.00\", currencyCode: EUR } }], pricesToDeleteByProductIds: [\"gid://shopify/Product/test\", \"gid://shopify/Product/test\"]) { priceList { id fixedPricesCount } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/test\") { fixedPricesCount } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"priceList\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"pricesToAdd\",\"0\",\"price\",\"currencyCode\"],\"code\":\"PRICES_TO_ADD_CURRENCY_MISMATCH\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"pricesToAdd\",\"0\",\"compareAtPrice\",\"currencyCode\"],\"code\":\"PRICES_TO_ADD_CURRENCY_MISMATCH\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"pricesToAdd\"],\"code\":\"DUPLICATE_ID_IN_INPUT\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"pricesToDeleteByProductIds\"],\"code\":\"DUPLICATE_ID_IN_INPUT\"",
  )
  assert string.contains(
    serialized,
    "\"field\":null,\"code\":\"ID_MUST_BE_MUTUALLY_EXCLUSIVE\"",
  )
  assert string.contains(
    serialized,
    "\"__typename\":\"PriceListFixedPricesByProductBulkUpdateUserError\"",
  )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"priceList\":{\"fixedPricesCount\":0}}}"
}

pub fn price_list_fixed_prices_by_product_update_types_missing_product_errors_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      product_bulk_fixed_price_proxy(),
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/test\", pricesToAdd: [{ productId: \"gid://shopify/Product/missing\", price: { amount: \"12.00\", currencyCode: EUR } }], pricesToDeleteByProductIds: [\"gid://shopify/Product/missing-delete\"]) { priceList { id } userErrors { __typename field code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"__typename\":\"PriceListFixedPricesByProductBulkUpdateUserError\",\"field\":[\"pricesToAdd\",\"0\",\"productId\"],\"code\":\"PRODUCT_DOES_NOT_EXIST\"",
  )
  assert string.contains(
    serialized,
    "\"__typename\":\"PriceListFixedPricesByProductBulkUpdateUserError\",\"field\":[\"pricesToDeleteByProductIds\",\"0\"],\"code\":\"PRODUCT_DOES_NOT_EXIST\"",
  )
}

pub fn price_list_fixed_prices_by_product_update_enforces_price_limit_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      product_bulk_fixed_price_proxy_with_fixed_edges(9999),
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/test\", pricesToAdd: [{ productId: \"gid://shopify/Product/test\", price: { amount: \"12.00\", currencyCode: EUR } }], pricesToDeleteByProductIds: []) { priceList { fixedPricesCount } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/test\") { fixedPricesCount } }",
    )

  assert status == 200
  assert string.contains(
    json.to_string(body),
    "\"code\":\"PRICE_LIMIT_EXCEEDED\"",
  )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"priceList\":{\"fixedPricesCount\":9999}}}"
}

pub fn price_list_fixed_prices_by_product_update_stages_valid_prices_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      product_bulk_fixed_price_proxy(),
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/test\", pricesToAdd: [{ productId: \"gid://shopify/Product/test\", price: { amount: \"12.00\", currencyCode: EUR }, compareAtPrice: { amount: \"15.00\", currencyCode: EUR } }], pricesToDeleteByProductIds: []) { priceList { fixedPricesCount } pricesToAddProducts { id title } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/test\") { fixedPricesCount prices(first: 10, originType: FIXED) { edges { node { price { amount currencyCode } compareAtPrice { amount currencyCode } variant { id product { id title } } } } } } }",
    )
  let serialized = json.to_string(body)
  let read_serialized = json.to_string(read_body)

  assert status == 200
  assert string.contains(serialized, "\"fixedPricesCount\":1")
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"pricesToAddProducts\":[{\"id\":\"gid://shopify/Product/test\",\"title\":\"Test product\"}]",
  )
  assert read_status == 200
  assert string.contains(read_serialized, "\"fixedPricesCount\":1")
  assert string.contains(
    read_serialized,
    "\"price\":{\"amount\":\"12.0\",\"currencyCode\":\"EUR\"}",
  )
  assert string.contains(
    read_serialized,
    "\"compareAtPrice\":{\"amount\":\"15.0\",\"currencyCode\":\"EUR\"}",
  )
  assert string.contains(
    read_serialized,
    "\"product\":{\"id\":\"gid://shopify/Product/test\",\"title\":\"Test product\"}",
  )
}

pub fn price_list_fixed_prices_add_stages_variant_prices_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      price_list_fixed_price_proxy(),
      "mutation { priceListFixedPricesAdd(priceListId: \"gid://shopify/PriceList/fixed\", prices: [{ variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"12.50\", currencyCode: EUR } }]) { prices { originType price { amount currencyCode } variant { id } } userErrors { __typename field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { priceList(id: \"gid://shopify/PriceList/fixed\") { id fixedPricesCount prices(first: 10, originType: FIXED) { edges { node { originType price { amount currencyCode } variant { id } } } } } }",
    )

  let create_json = json.to_string(create_body)
  let read_json = json.to_string(read_body)
  assert create_status == 200
  assert read_status == 200
  assert string.contains(create_json, "\"prices\":[")
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(
    create_json,
    "\"amount\":\"12.5\",\"currencyCode\":\"EUR\"",
  )
  assert string.contains(
    create_json,
    "\"variant\":{\"id\":\"gid://shopify/ProductVariant/alpha\"}",
  )
  assert string.contains(read_json, "\"fixedPricesCount\":1")
  assert string.contains(
    read_json,
    "\"amount\":\"12.5\",\"currencyCode\":\"EUR\"",
  )
  assert string.contains(
    read_json,
    "\"variant\":{\"id\":\"gid://shopify/ProductVariant/alpha\"}",
  )
}

pub fn price_list_fixed_prices_update_and_delete_share_staged_fixed_rows_test() {
  let #(Response(status: add_status, ..), proxy_after_add) =
    graphql_with_proxy(
      price_list_fixed_price_proxy(),
      "mutation { priceListFixedPricesAdd(priceListId: \"gid://shopify/PriceList/fixed\", prices: [{ variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"12.50\", currencyCode: EUR } }, { variantId: \"gid://shopify/ProductVariant/beta\", price: { amount: \"20.00\", currencyCode: EUR } }]) { priceList { id } userErrors { field message code } } }",
    )
  let #(
    Response(status: update_status, body: update_body, ..),
    proxy_after_update,
  ) =
    graphql_with_proxy(
      proxy_after_add,
      "mutation { priceListFixedPricesUpdate(priceListId: \"gid://shopify/PriceList/fixed\", pricesToAdd: [{ variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"15.00\", currencyCode: EUR } }], variantIdsToDelete: [\"gid://shopify/ProductVariant/beta\"]) { priceList { fixedPricesCount prices(first: 10, originType: FIXED) { edges { node { price { amount currencyCode } variant { id } } } } } pricesAdded { price { amount currencyCode } variant { id } } deletedFixedPriceVariantIds userErrors { field message code } } }",
    )
  let #(
    Response(status: delete_status, body: delete_body, ..),
    proxy_after_delete,
  ) =
    graphql_with_proxy(
      proxy_after_update,
      "mutation { priceListFixedPricesDelete(priceListId: \"gid://shopify/PriceList/fixed\", variantIds: [\"gid://shopify/ProductVariant/beta\", \"gid://shopify/ProductVariant/alpha\"]) { deletedFixedPriceVariantIds userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy_after_delete,
      "query { priceList(id: \"gid://shopify/PriceList/fixed\") { fixedPricesCount prices(first: 10, originType: FIXED) { edges { node { variant { id } } } } } }",
    )

  let update_json = json.to_string(update_body)
  let delete_json = json.to_string(delete_body)
  let read_json = json.to_string(read_body)
  assert add_status == 200
  assert update_status == 200
  assert delete_status == 200
  assert read_status == 200
  assert string.contains(update_json, "\"pricesAdded\":[")
  assert !string.contains(update_json, "\"fixedPriceVariantIds\"")
  assert string.contains(
    update_json,
    "\"deletedFixedPriceVariantIds\":[\"gid://shopify/ProductVariant/beta\"]",
  )
  assert string.contains(update_json, "\"fixedPricesCount\":1")
  assert string.contains(
    update_json,
    "\"amount\":\"15.0\",\"currencyCode\":\"EUR\"",
  )
  assert !string.contains(
    update_json,
    "\"variant\":{\"id\":\"gid://shopify/ProductVariant/beta\"}",
  )
  assert !string.contains(delete_json, "\"fixedPriceVariantIds\"")
  assert string.contains(
    delete_json,
    "\"deletedFixedPriceVariantIds\":[\"gid://shopify/ProductVariant/alpha\"]",
  )
  assert string.contains(read_json, "\"fixedPricesCount\":0")
  assert !string.contains(read_json, "\"variant\":{\"id\"")
}

pub fn price_list_fixed_prices_validates_target_variant_currency_and_duplicates_test() {
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql_with_proxy(
      price_list_fixed_price_proxy(),
      "mutation { priceListFixedPricesAdd(priceListId: \"gid://shopify/PriceList/missing\", prices: [{ variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"12.50\", currencyCode: EUR } }]) { prices { variant { id } } userErrors { __typename field message code } } }",
    )
  let #(Response(status: input_status, body: input_body, ..), _) =
    graphql_with_proxy(
      price_list_fixed_price_proxy(),
      "mutation { priceListFixedPricesAdd(priceListId: \"gid://shopify/PriceList/fixed\", prices: [{ variantId: \"gid://shopify/ProductVariant/missing\", price: { amount: \"12.50\", currencyCode: EUR } }, { variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"10.00\", currencyCode: USD } }, { variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"11.00\", currencyCode: EUR } }]) { prices { variant { id } } userErrors { __typename field message code } } }",
    )

  let missing_json = json.to_string(missing_body)
  let input_json = json.to_string(input_body)
  assert missing_status == 200
  assert input_status == 200
  assert string.contains(
    missing_json,
    "\"__typename\":\"PriceListPriceUserError\"",
  )
  assert string.contains(
    missing_json,
    "\"field\":[\"priceListId\"],\"message\":\"Price list not found.\",\"code\":\"PRICE_LIST_NOT_FOUND\"",
  )
  assert string.contains(
    input_json,
    "\"field\":[\"prices\",\"0\",\"variantId\"],\"message\":\"Variant not found.\",\"code\":\"VARIANT_NOT_FOUND\"",
  )
  assert string.contains(
    input_json,
    "\"field\":[\"prices\",\"1\",\"price\",\"currencyCode\"],\"message\":\"Currency must match price list currency.\",\"code\":\"PRICES_TO_ADD_CURRENCY_MISMATCH\"",
  )
  assert string.contains(
    input_json,
    "\"field\":[\"prices\",\"2\",\"variantId\"],\"message\":\"Duplicate variant ID in input.\",\"code\":\"DUPLICATE_ID_IN_INPUT\"",
  )
  assert string.contains(input_json, "\"prices\":null")
}

pub fn price_list_fixed_prices_update_rejects_missing_fixed_price_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      price_list_fixed_price_proxy(),
      "mutation { priceListFixedPricesUpdate(priceListId: \"gid://shopify/PriceList/fixed\", pricesToAdd: [{ variantId: \"gid://shopify/ProductVariant/alpha\", price: { amount: \"15.00\", currencyCode: EUR } }], variantIdsToDelete: []) { priceList { id } userErrors { __typename field message code } } }",
    )

  assert status == 200
  assert string.contains(
    json.to_string(body),
    "\"field\":[\"pricesToAdd\",\"0\",\"variantId\"],\"message\":\"Price is not fixed.\",\"code\":\"PRICE_NOT_FIXED\"",
  )
}

pub fn web_presence_create_subfolder_root_urls_include_all_locales_and_shop_domain_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", alternateLocales: [\"fr\", \"de\"], subfolderSuffix: \"intl\" }) { webPresence { subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"domain\":null")
  assert string.contains(
    serialized,
    "\"rootUrls\":[{\"locale\":\"en\",\"url\":\"https://acme.myshopify.com/intl/\"},{\"locale\":\"fr\",\"url\":\"https://acme.myshopify.com/intl/fr/\"},{\"locale\":\"de\",\"url\":\"https://acme.myshopify.com/intl/de/\"}]",
  )
  assert !string.contains(serialized, "harry-test-heelo.myshopify.com")
  assert !string.contains(serialized, "/en-intl/")
}

pub fn web_presence_create_domain_root_urls_resolve_primary_domain_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", alternateLocales: [\"fr\"], domainId: \"gid://shopify/Domain/1000\" }) { webPresence { subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"domain\":{\"id\":\"gid://shopify/Domain/1000\",\"host\":\"acme.myshopify.com\",\"url\":\"https://acme.myshopify.com\",\"sslEnabled\":true}",
  )
  assert string.contains(
    serialized,
    "\"rootUrls\":[{\"locale\":\"en\",\"url\":\"https://acme.myshopify.com/\"},{\"locale\":\"fr\",\"url\":\"https://acme.myshopify.com/fr/\"}]",
  )
  assert string.contains(serialized, "\"subfolderSuffix\":null")
}

pub fn web_presence_create_accepts_shopify_i18n_locale_codes_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr-CA\", alternateLocales: [\"pt-BR\", \"es-419\", \"zh-Hant-TW\"], subfolderSuffix: \"fr\" }) { webPresence { subfolderSuffix defaultLocale { locale primary } alternateLocales { locale primary } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"defaultLocale\":{\"locale\":\"fr-CA\",\"primary\":true}",
  )
  assert string.contains(
    serialized,
    "\"alternateLocales\":[{\"locale\":\"pt-BR\",\"primary\":false},{\"locale\":\"es-419\",\"primary\":false},{\"locale\":\"zh-Hant-TW\",\"primary\":false}]",
  )
}

pub fn web_presence_create_normalizes_locale_code_casing_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"EN-us\", alternateLocales: [\"ZH-hant-tw\", \"pt-br\"], subfolderSuffix: \"us\" }) { webPresence { defaultLocale { locale primary } alternateLocales { locale primary } rootUrls { locale url } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(
    serialized,
    "\"defaultLocale\":{\"locale\":\"en-US\",\"primary\":true}",
  )
  assert string.contains(
    serialized,
    "\"alternateLocales\":[{\"locale\":\"zh-Hant-TW\",\"primary\":false},{\"locale\":\"pt-BR\",\"primary\":false}]",
  )
  assert string.contains(
    serialized,
    "\"rootUrls\":[{\"locale\":\"en-US\",\"url\":\"https://acme.myshopify.com/us/\"},{\"locale\":\"zh-Hant-TW\",\"url\":\"https://acme.myshopify.com/us/zh-Hant-TW/\"},{\"locale\":\"pt-BR\",\"url\":\"https://acme.myshopify.com/us/pt-BR/\"}]",
  )
}

pub fn web_presence_create_reports_combined_invalid_alternate_locales_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr-CA\", alternateLocales: [\"fr\", \"zz\", \"pt-BR\", \"yy\"], subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"webPresence\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"alternateLocales\"],\"message\":\"Invalid locale codes: zz, and yy\",\"code\":\"INVALID\"",
  )
  assert !string.contains(serialized, "\"alternateLocales\",\"1\"")
  assert !string.contains(serialized, "\"alternateLocales\",\"3\"")
}

pub fn web_presence_create_validates_routing_and_subfolder_suffix_test() {
  let #(Response(status: mutex_status, body: mutex_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", domainId: \"gid://shopify/Domain/1000\", subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: short_status, body: short_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"x\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: script_status, body: script_body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"Latn\" }) { webPresence { id } userErrors { field message code } } }",
    )

  assert mutex_status == 200
  assert missing_status == 200
  assert short_status == 200
  assert script_status == 200
  assert string.contains(
    json.to_string(mutex_body),
    "\"code\":\"CANNOT_HAVE_SUBFOLDER_AND_DOMAIN\"",
  )
  assert string.contains(
    json.to_string(missing_body),
    "\"code\":\"REQUIRES_DOMAIN_OR_SUBFOLDER\"",
  )
  assert string.contains(
    json.to_string(short_body),
    "\"code\":\"SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS\"",
  )
  assert string.contains(
    json.to_string(script_body),
    "\"code\":\"SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE\"",
  )
}

pub fn web_presence_create_reports_unknown_domain_only_when_not_stored_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", domainId: \"gid://shopify/Domain/9999\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"webPresence\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"domainId\"],\"message\":\"Domain does not exist\",\"code\":\"DOMAIN_NOT_FOUND\"",
  )
}

pub fn web_presence_update_preserves_absent_locale_fields_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr\", alternateLocales: [\"es\"], subfolderSuffix: \"fr\" }) { webPresence { id defaultLocale { locale } alternateLocales { locale } } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { alternateLocales: [\"de\"] }) { webPresence { id defaultLocale { locale } alternateLocales { locale } rootUrls { locale url } } userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "{ webPresences(first: 10) { nodes { id defaultLocale { locale } alternateLocales { locale } rootUrls { locale url } } } }",
    )
  let update_json = json.to_string(update_body)
  let read_json = json.to_string(read_body)

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert update_status == 200
  assert read_status == 200
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"defaultLocale\":{\"locale\":\"fr\"}")
  assert string.contains(
    update_json,
    "\"alternateLocales\":[{\"locale\":\"de\"}]",
  )
  assert string.contains(read_json, "\"defaultLocale\":{\"locale\":\"fr\"}")
  assert string.contains(
    read_json,
    "\"alternateLocales\":[{\"locale\":\"de\"}]",
  )
}

pub fn web_presence_update_preserves_absent_alternate_locales_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", alternateLocales: [\"es\"], subfolderSuffix: \"intl\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { defaultLocale: \"fr\" }) { webPresence { defaultLocale { locale } alternateLocales { locale } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(update_body)

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert update_status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"defaultLocale\":{\"locale\":\"fr\"}")
  assert string.contains(
    serialized,
    "\"alternateLocales\":[{\"locale\":\"es\"}]",
  )
}

pub fn web_presence_update_accepts_empty_input_as_noop_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"fr\", alternateLocales: [\"es\"], subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: {}) { webPresence { defaultLocale { locale } alternateLocales { locale } subfolderSuffix } userErrors { field message code } } }",
    )
  let serialized = json.to_string(update_body)

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert update_status == 200
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"defaultLocale\":{\"locale\":\"fr\"}")
  assert string.contains(
    serialized,
    "\"alternateLocales\":[{\"locale\":\"es\"}]",
  )
  assert string.contains(serialized, "\"subfolderSuffix\":\"fr\"")
}

pub fn web_presence_update_validates_explicit_default_locale_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"en\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"
  let #(Response(status: blank_status, body: blank_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { defaultLocale: \"\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { defaultLocale: \"bogus\" }) { webPresence { id } userErrors { field message code } } }",
    )

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert blank_status == 200
  assert invalid_status == 200
  assert string.contains(json.to_string(blank_body), "\"webPresence\":null")
  assert string.contains(
    json.to_string(blank_body),
    "\"field\":[\"input\",\"defaultLocale\"],\"message\":\"Default locale can't be blank\",\"code\":\"CANNOT_SET_DEFAULT_LOCALE_TO_NULL\"",
  )
  assert string.contains(json.to_string(invalid_body), "\"webPresence\":null")
  assert string.contains(
    json.to_string(invalid_body),
    "\"field\":[\"input\",\"defaultLocale\"],\"message\":\"Invalid locale codes: bogus\",\"code\":\"INVALID\"",
  )
}

pub fn web_presence_update_domain_id_is_not_validated_as_user_error_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"en\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { domainId: \"gid://shopify/Domain/9999\" }) { webPresence { id defaultLocale { locale } } userErrors { field message code } } }",
    )
  let serialized = json.to_string(update_body)

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert update_status == 200
  assert string.contains(
    serialized,
    "\"webPresence\":{\"id\":\"gid://shopify/MarketWebPresence/1\",\"defaultLocale\":{\"locale\":\"en\"}}",
  )
  assert string.contains(serialized, "\"userErrors\":[]")
  assert !string.contains(serialized, "DOMAIN_NOT_FOUND")
}

pub fn web_presence_update_subfolder_domain_mutex_uses_existing_domain_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", domainId: \"gid://shopify/Domain/1000\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let web_presence_id = "gid://shopify/MarketWebPresence/1"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceUpdate(id: \""
        <> web_presence_id
        <> "\", input: { subfolderSuffix: \"fr\" }) { webPresence { id } userErrors { field message code } } }",
    )
  let serialized = json.to_string(update_body)

  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"id\":\"gid://shopify/MarketWebPresence/1\"",
  )
  assert update_status == 200
  assert string.contains(serialized, "\"webPresence\":null")
  assert string.contains(
    serialized,
    "\"code\":\"CANNOT_HAVE_SUBFOLDER_AND_DOMAIN\"",
  )
}

fn price_list_fixed_price_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_products([fixed_price_product()])
    |> store.upsert_base_product_variants([
      fixed_price_variant("alpha", "Alpha"),
      fixed_price_variant("beta", "Beta"),
    ])
    |> store.upsert_base_price_lists([fixed_price_price_list()])
  DraftProxy(..proxy, store: seeded_store)
}

fn fixed_price_price_list() -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/fixed",
    cursor: None,
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/fixed")),
      #("name", CapturedString("EU Fixed")),
      #("currency", CapturedString("EUR")),
      #("fixedPricesCount", CapturedInt(0)),
      #("parent", CapturedNull),
      #("catalog", CapturedNull),
      #("prices", empty_price_connection()),
      #("quantityRules", empty_connection()),
    ]),
  )
}

fn empty_price_connection() {
  CapturedObject([
    #("edges", CapturedArray([])),
    #("nodes", CapturedArray([])),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
      ]),
    ),
  ])
}

fn empty_connection() {
  CapturedObject([
    #("edges", CapturedArray([])),
    #("nodes", CapturedArray([])),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
      ]),
    ),
  ])
}

fn fixed_price_product() -> ProductRecord {
  ProductRecord(
    id: "gid://shopify/Product/fixed",
    legacy_resource_id: None,
    title: "Fixed Price Product",
    handle: "fixed-price-product",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    price_range_min: None,
    price_range_max: None,
    total_variants: Some(2),
    has_only_default_variant: Some(False),
    has_out_of_stock_variants: Some(False),
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
    combined_listing_role: None,
    combined_listing_parent_id: None,
    combined_listing_child_ids: [],
  )
}

fn fixed_price_variant(tail: String, title: String) -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/" <> tail,
    product_id: "gid://shopify/Product/fixed",
    title: title,
    sku: None,
    barcode: None,
    price: Some("10.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Title", value: title),
    ],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: None,
  )
}

fn product_bulk_fixed_price_proxy() -> DraftProxy {
  product_bulk_fixed_price_proxy_with_fixed_edges(0)
}

fn product_bulk_fixed_price_proxy_with_quantity_break(
  minimum_quantity: Int,
) -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_products([product_bulk_fixed_price_product()])
    |> store.upsert_base_product_variants([product_bulk_fixed_price_variant()])
    |> store.upsert_base_price_lists([
      product_bulk_fixed_price_list([
        product_bulk_fixed_price_edge_with_quantity_break(minimum_quantity),
      ]),
    ])
  DraftProxy(..proxy, store: seeded_store)
}

fn product_bulk_fixed_price_proxy_with_fixed_edges(
  edge_count: Int,
) -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_products([product_bulk_fixed_price_product()])
    |> store.upsert_base_product_variants([product_bulk_fixed_price_variant()])
    |> store.upsert_base_price_lists([
      product_bulk_fixed_price_list(product_bulk_fixed_price_edges(edge_count)),
    ])
  DraftProxy(..proxy, store: seeded_store)
}

fn product_bulk_fixed_price_product() -> ProductRecord {
  ProductRecord(
    id: "gid://shopify/Product/test",
    legacy_resource_id: None,
    title: "Test product",
    handle: "test-product",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    price_range_min: None,
    price_range_max: None,
    total_variants: Some(1),
    has_only_default_variant: Some(True),
    has_out_of_stock_variants: None,
    total_inventory: None,
    tracks_inventory: None,
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
    combined_listing_role: None,
    combined_listing_parent_id: None,
    combined_listing_child_ids: [],
  )
}

fn product_bulk_fixed_price_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/test",
    product_id: "gid://shopify/Product/test",
    title: "Default Title",
    sku: Some("sku-test"),
    barcode: None,
    price: Some("9.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: None,
    selected_options: [],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: None,
  )
}

fn product_bulk_fixed_price_list(
  edges: List(CapturedJsonValue),
) -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/test",
    cursor: None,
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/test")),
      #("name", CapturedString("EUR test")),
      #("currency", CapturedString("EUR")),
      #("fixedPricesCount", CapturedInt(list.length(edges))),
      #("prices", product_bulk_fixed_price_connection(edges)),
    ]),
  )
}

fn product_bulk_fixed_price_connection(
  edges: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("edges", CapturedArray(edges)),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", CapturedNull),
        #("endCursor", CapturedNull),
      ]),
    ),
  ])
}

fn product_bulk_fixed_price_edges(count: Int) -> List(CapturedJsonValue) {
  int.range(from: 1, to: count + 1, with: [], run: fn(acc, index) {
    [product_bulk_fixed_price_edge(index), ..acc]
  })
  |> list.reverse
}

fn product_bulk_fixed_price_edge(index: Int) -> CapturedJsonValue {
  let variant_id =
    "gid://shopify/ProductVariant/existing-" <> int.to_string(index)
  CapturedObject([
    #("cursor", CapturedString(variant_id)),
    #(
      "node",
      CapturedObject([
        #("__typename", CapturedString("PriceListPrice")),
        #("originType", CapturedString("FIXED")),
        #("variant", CapturedObject([#("id", CapturedString(variant_id))])),
      ]),
    ),
  ])
}

fn product_bulk_fixed_price_edge_with_quantity_break(
  minimum_quantity: Int,
) -> CapturedJsonValue {
  CapturedObject([
    #("cursor", CapturedString("gid://shopify/ProductVariant/test")),
    #(
      "node",
      CapturedObject([
        #("__typename", CapturedString("PriceListPrice")),
        #("originType", CapturedString("FIXED")),
        #(
          "variant",
          CapturedObject([
            #("id", CapturedString("gid://shopify/ProductVariant/test")),
          ]),
        ),
        #(
          "quantityPriceBreaks",
          CapturedObject([
            #(
              "edges",
              CapturedArray([
                CapturedObject([
                  #("cursor", CapturedString("break")),
                  #(
                    "node",
                    CapturedObject([
                      #("__typename", CapturedString("QuantityPriceBreak")),
                      #("minimumQuantity", CapturedInt(minimum_quantity)),
                    ]),
                  ),
                ]),
              ]),
            ),
          ]),
        ),
      ]),
    ),
  ])
}

fn seeded_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store = store.upsert_base_shop(proxy.store, acme_shop())
  DraftProxy(..proxy, store: seeded_store)
}

fn catalog_price_list_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let market = cad_market_record()
  let catalog = cad_catalog_record(market.data)
  let price_list = cad_price_list_record(catalog.data)
  let seeded_store =
    proxy.store
    |> store.upsert_base_markets([market])
    |> store.upsert_base_catalogs([catalog])
    |> store.upsert_base_price_lists([price_list])
  DraftProxy(..proxy, store: seeded_store)
}

fn invalid_adjustment_price_list_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_price_lists([invalid_adjustment_price_list_record()])
  DraftProxy(..proxy, store: seeded_store)
}

fn cad_market_record() -> MarketRecord {
  MarketRecord(
    id: "gid://shopify/Market/100",
    cursor: Some("gid://shopify/Market/100"),
    data: CapturedObject([
      #("__typename", CapturedString("Market")),
      #("id", CapturedString("gid://shopify/Market/100")),
      #(
        "currencySettings",
        CapturedObject([
          #(
            "baseCurrency",
            CapturedObject([#("currencyCode", CapturedString("CAD"))]),
          ),
        ]),
      ),
    ]),
  )
}

fn cad_catalog_record(market_data: CapturedJsonValue) -> CatalogRecord {
  CatalogRecord(
    id: "gid://shopify/MarketCatalog/200",
    cursor: Some("gid://shopify/MarketCatalog/200"),
    data: CapturedObject([
      #("__typename", CapturedString("MarketCatalog")),
      #("id", CapturedString("gid://shopify/MarketCatalog/200")),
      #("title", CapturedString("Canada Catalog")),
      #("markets", CapturedObject([#("nodes", CapturedArray([market_data]))])),
    ]),
  )
}

fn cad_price_list_record(catalog_data: CapturedJsonValue) -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/300",
    cursor: Some("gid://shopify/PriceList/300"),
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/300")),
      #("name", CapturedString("CAD Price List")),
      #("currency", CapturedString("CAD")),
      #("catalog", catalog_data),
      #(
        "parent",
        CapturedObject([
          #(
            "adjustment",
            CapturedObject([
              #("type", CapturedString("PERCENTAGE_DECREASE")),
              #("value", CapturedInt(10)),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn invalid_adjustment_price_list_record() -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/400",
    cursor: Some("gid://shopify/PriceList/400"),
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/400")),
      #("name", CapturedString("Invalid Existing Price List")),
      #("currency", CapturedString("USD")),
      #(
        "parent",
        CapturedObject([
          #(
            "adjustment",
            CapturedObject([
              #("type", CapturedString("PERCENTAGE_DECREASE")),
              #("value", CapturedInt(250)),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn acme_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/1000",
    name: "acme",
    myshopify_domain: "acme.myshopify.com",
    url: "https://acme.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/1000",
      host: "acme.myshopify.com",
      url: "https://acme.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "shop@example.com",
    email: "shop@example.com",
    currency_code: "USD",
    enabled_presentment_currencies: ["USD"],
    iana_timezone: "America/New_York",
    timezone_abbreviation: "EST",
    timezone_offset: "-0500",
    timezone_offset_minutes: -300,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "IMPERIAL_SYSTEM",
    weight_unit: "POUNDS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/1000",
      address1: Some("1 Main St"),
      address2: None,
      city: Some("New York"),
      company: None,
      coordinates_validated: False,
      country: Some("United States"),
      country_code_v2: Some("US"),
      formatted: ["1 Main St", "New York NY 10001", "United States"],
      formatted_area: Some("New York NY, United States"),
      latitude: None,
      longitude: None,
      phone: None,
      province: Some("New York"),
      province_code: Some("NY"),
      zip: Some("10001"),
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
      discounts_by_market_enabled: False,
      sells_subscriptions: False,
      show_metrics: True,
      storefront: True,
      unified_markets: True,
    ),
    payment_settings: PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
    shop_policies: [],
  )
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

pub fn market_create_dedupes_generated_handles_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe!\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: third_status, body: third_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe?\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe-1\"},\"userErrors\":[]}}}"
  assert third_status == 200
  assert json.to_string(third_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe-2\"},\"userErrors\":[]}}}"
}

pub fn market_create_rejects_duplicate_name_before_handle_dedupe_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn market_create_slugifies_generated_handle_like_shopify_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketCreate(input: { name: \"  North & South / EU!  \" }) { market { handle } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"north-south-eu\"},\"userErrors\":[]}}}"
}

pub fn market_create_rejects_explicit_duplicate_handle_test() {
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )
  let #(Response(status: second_status, body: second_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Other\", handle: \"Europe\" }) { market { handle } userErrors { field message code } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"handle\":\"europe\"},\"userErrors\":[]}}}"
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"marketCreate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Generated handle has already been taken\",\"code\":\"GENERATED_DUPLICATED_HANDLE\"}]}}}"
}

pub fn market_localizations_register_rejects_more_than_100_keys_test() {
  let input = too_many_market_localization_inputs()
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/missing\", marketLocalizations: ["
      <> input
      <> "]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"resourceId\"],\"code\":\"TOO_MANY_KEYS_FOR_RESOURCE\"}]}}}"
}

pub fn market_localizations_register_returns_translation_error_for_missing_resource_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/missing\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"resourceId\"],\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn market_localizations_remove_returns_translation_error_for_missing_resource_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/missing\", marketLocalizationKeys: [], marketIds: []) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"resourceId\"],\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn market_localizations_register_validates_market_key_digest_and_value_test() {
  let proxy = market_localization_proxy()
  let #(Response(status: market_status, body: market_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/missing\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: key_status, body: key_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"value\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: digest_status, body: digest_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"stale\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: value_status, body: value_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"marketId\"],\"code\":\"MARKET_DOES_NOT_EXIST\"}]}}}"
  assert key_status == 200
  assert json.to_string(key_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"key\"],\"code\":\"INVALID_KEY_FOR_MODEL\"}]}}}"
  assert digest_status == 200
  assert json.to_string(digest_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"marketLocalizableContentDigest\"],\"code\":\"INVALID_MARKET_LOCALIZABLE_CONTENT\"}]}}}"
  assert value_status == 200
  assert json.to_string(value_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":null,\"userErrors\":[{\"__typename\":\"TranslationUserError\",\"field\":[\"marketLocalizations\",\"0\",\"value\"],\"code\":\"FAILS_RESOURCE_VALIDATION\"}]}}}"
}

pub fn market_localizations_register_stages_seeded_content_test() {
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      market_localization_proxy(),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value market { id name } } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizableContent { key value digest } marketLocalizations { key value market { id name } } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}],\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"marketLocalizableResource\":{\"marketLocalizableContent\":[{\"key\":\"title\",\"value\":\"Title\",\"digest\":\"digest-title\"},{\"key\":\"subtitle\",\"value\":\"Subtitle\",\"digest\":\"digest-subtitle\"}],\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}]}}}"
}

pub fn market_localizations_remove_deletes_matching_staged_records_test() {
  let #(Response(status: first_status, body: first_body, ..), first_proxy) =
    graphql_with_proxy(
      market_localization_proxy(),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }, { marketId: \"gid://shopify/Market/ca\", key: \"subtitle\", value: \"Sous-titre\", marketLocalizableContentDigest: \"digest-subtitle\" }]) { marketLocalizations { key value market { id name } } userErrors { __typename field code } } }",
    )
  let #(Response(status: remove_status, body: remove_body, ..), removed_proxy) =
    graphql_with_proxy(
      first_proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"title\"], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key value market { id name } } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      removed_proxy,
      "query { marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizations { key value market { id name } } } }",
    )

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}},{\"key\":\"subtitle\",\"value\":\"Sous-titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}],\"userErrors\":[]}}}"
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}],\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"marketLocalizableResource\":{\"marketLocalizations\":[{\"key\":\"subtitle\",\"value\":\"Sous-titre\",\"market\":{\"id\":\"gid://shopify/Market/ca\",\"name\":\"Canada\"}}]}}}"
}

pub fn market_localizations_remove_returns_null_when_no_staged_records_match_test() {
  let #(Response(status: register_status, body: register_body, ..), proxy) =
    graphql_with_proxy(
      market_localization_proxy(),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/ca\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: remove_status, body: remove_body, ..), removed_proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketLocalizationsRemove(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizationKeys: [\"subtitle\"], marketIds: [\"gid://shopify/Market/ca\"]) { marketLocalizations { key value } userErrors { __typename field code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      removed_proxy,
      "query { marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizations { key value } } }",
    )

  assert register_status == 200
  assert json.to_string(register_body)
    == "{\"data\":{\"marketLocalizationsRegister\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\"}],\"userErrors\":[]}}}"
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"marketLocalizationsRemove\":{\"marketLocalizations\":null,\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"marketLocalizableResource\":{\"marketLocalizations\":[{\"key\":\"title\",\"value\":\"Titre\"}]}}}"
}

fn too_many_market_localization_inputs() -> String {
  int.range(from: 1, to: 102, with: [], run: fn(acc, index) {
    [
      "{ marketId: \"gid://shopify/Market/"
        <> int.to_string(index)
        <> "\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }",
      ..acc
    ]
  })
  |> list.reverse
  |> string.join(with: ",")
}

fn market_localization_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_markets([market_localization_market()])
    |> store.replace_base_metafields_for_owner(
      "gid://shopify/Product/localizable",
      [market_localization_metafield()],
    )
  DraftProxy(..proxy, store: seeded_store)
}

fn market_localization_market() -> MarketRecord {
  MarketRecord(
    id: "gid://shopify/Market/ca",
    cursor: Some("gid://shopify/Market/ca"),
    data: CapturedObject([
      #("id", CapturedString("gid://shopify/Market/ca")),
      #("name", CapturedString("Canada")),
    ]),
  )
}

fn market_localization_metafield() -> ProductMetafieldRecord {
  ProductMetafieldRecord(
    id: "gid://shopify/Metafield/localizable",
    owner_id: "gid://shopify/Product/localizable",
    namespace: "custom",
    key: "title",
    type_: Some("single_line_text_field"),
    value: Some("Title"),
    compare_digest: Some("digest-title"),
    json_value: None,
    created_at: None,
    updated_at: None,
    owner_type: Some("PRODUCT"),
    market_localizable_content: [
      MarketLocalizableContentRecord(
        key: "title",
        value: "Title",
        digest: "digest-title",
      ),
      MarketLocalizableContentRecord(
        key: "subtitle",
        value: "Subtitle",
        digest: "digest-subtitle",
      ),
    ],
  )
}

pub fn catalog_create_requires_status_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"status\"],\"message\":\"Status is required\",\"code\":\"REQUIRED\"}]}}}"
}

pub fn catalog_create_rejects_invalid_status_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: DISABLED, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"status\"],\"message\":\"Status is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_requires_context_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\"],\"message\":\"Context is required\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_requires_market_ids_for_empty_context_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: {} }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"marketIds\"],\"message\":\"Market ids can't be blank\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_market_context_ids_test() {
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/404\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert missing_status == 200
  assert json.to_string(missing_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"marketIds\",\"0\"],\"message\":\"Market does not exist\",\"code\":\"INVALID\"}]}}}"
  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"marketIds\"],\"message\":\"Market ids can't be blank\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_company_location_context_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" }, companyLocation: { name: \"B2B HQ\" } }) { company { id locations(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [\"gid://shopify/CompanyLocation/404\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: unsupported_status, body: unsupported_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"B2B Catalog\", status: ACTIVE, context: { driverType: COMPANY_LOCATION, companyLocationIds: [\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"companyCreate\":{\"company\":{\"id\":\"gid://shopify/Company/1?shopify-draft-proxy=synthetic\",\"locations\":{\"nodes\":[{\"id\":\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"}]}},\"userErrors\":[]}}}"
  assert missing_status == 200
  assert json.to_string(missing_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"companyLocationIds\",\"0\"],\"message\":\"Company location does not exist\",\"code\":\"INVALID\"}]}}}"
  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"companyLocationIds\"],\"message\":\"Company location ids can't be blank\",\"code\":\"INVALID\"}]}}}"
  assert unsupported_status == 200
  assert json.to_string(unsupported_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"driverType\"],\"message\":\"Catalog context driverType COMPANY_LOCATION is not supported by the local MarketCatalog model\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_validates_country_context_test() {
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"Country Catalog\", status: ACTIVE, context: { driverType: COUNTRY, countryCodes: [] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: unsupported_status, body: unsupported_body, ..), _) =
    graphql(
      "mutation { catalogCreate(input: { title: \"Country Catalog\", status: ACTIVE, context: { driverType: COUNTRY, countryCodes: [US] } }) { catalog { id } userErrors { field message code } } }",
    )

  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"countryCodes\"],\"message\":\"Country codes can't be blank\",\"code\":\"INVALID\"}]}}}"
  assert unsupported_status == 200
  assert json.to_string(unsupported_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"context\",\"driverType\"],\"message\":\"Catalog context driverType COUNTRY is not supported by the local MarketCatalog model\",\"code\":\"INVALID\"}]}}}"
}

pub fn catalog_create_stages_market_context_test() {
  let #(Response(status: market_status, body: market_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id title status markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalogs(first: 5, type: MARKET) { nodes { id title status markets(first: 5) { nodes { id } } } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/3\",\"title\":\"EU Catalog\",\"status\":\"ACTIVE\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}},\"userErrors\":[]}}}"
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"title\":\"EU Catalog\",\"status\":\"ACTIVE\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}",
  )
}

pub fn market_delete_cascades_dependent_staged_state_test() {
  let #(Response(status: market_status, body: market_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id conditions { regionsCondition { regions { id name countryCode } } } } userErrors { field message code } } }",
    )
  let #(
    Response(status: localization_status, body: localization_body, ..),
    proxy,
  ) =
    graphql_with_proxy(
      market_delete_cascade_proxy(proxy),
      "mutation { marketLocalizationsRegister(resourceId: \"gid://shopify/Metafield/localizable\", marketLocalizations: [{ marketId: \"gid://shopify/Market/1\", key: \"title\", value: \"Titre\", marketLocalizableContentDigest: \"digest-title\" }]) { marketLocalizations { key market { id } } userErrors { __typename field code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketDelete(id: \"gid://shopify/Market/1\") { deletedId userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { market(id: \"gid://shopify/Market/1\") { id } webPresences(first: 10) { nodes { id } } marketLocalizableResource(resourceId: \"gid://shopify/Metafield/localizable\") { marketLocalizations { key market { id } } } catalogs(first: 5, type: MARKET) { nodes { id markets(first: 5) { nodes { id } } } } }",
    )

  assert market_status == 200
  assert localization_status == 200
  assert catalog_status == 200
  assert delete_status == 200
  assert read_status == 200
  assert string.contains(json.to_string(market_body), "\"userErrors\":[]")
  assert string.contains(json.to_string(localization_body), "\"userErrors\":[]")
  assert string.contains(json.to_string(catalog_body), "\"userErrors\":[]")
  assert json.to_string(delete_body)
    == "{\"data\":{\"marketDelete\":{\"deletedId\":\"gid://shopify/Market/1\",\"userErrors\":[]}}}"
  assert json.to_string(read_body)
    == "{\"data\":{\"market\":null,\"webPresences\":{\"nodes\":[]},\"marketLocalizableResource\":{\"marketLocalizations\":[]},\"catalogs\":{\"nodes\":[{\"id\":\"gid://shopify/MarketCatalog/4\",\"markets\":{\"nodes\":[]}}]}}}"
}

pub fn catalog_delete_detaches_surviving_price_list_test() {
  let proxy = attached_catalog_price_list_proxy()
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogDelete(id: \"gid://shopify/MarketCatalog/attached\") { deletedId userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/attached\") { id } priceList(id: \"gid://shopify/PriceList/attached\") { id catalog { id } } }",
    )

  assert delete_status == 200
  assert read_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"catalogDelete\":{\"deletedId\":\"gid://shopify/MarketCatalog/attached\",\"userErrors\":[]}}}"
  assert json.to_string(read_body)
    == "{\"data\":{\"catalog\":null,\"priceList\":{\"id\":\"gid://shopify/PriceList/attached\",\"catalog\":null}}}"
}

pub fn price_list_delete_detaches_catalog_and_clears_fixed_prices_test() {
  let proxy = attached_catalog_price_list_proxy()
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { priceListFixedPricesByProductUpdate(priceListId: \"gid://shopify/PriceList/attached\", pricesToAdd: [{ productId: \"gid://shopify/Product/fixed\", price: { amount: \"12.50\", currencyCode: USD } }], pricesToDeleteByProductIds: []) { priceList { id fixedPricesCount prices(first: 5, originType: FIXED) { nodes { originType variant { id } } } } userErrors { field message code } } }",
    )
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { priceListDelete(id: \"gid://shopify/PriceList/attached\") { deletedId priceList { id fixedPricesCount prices(first: 5, originType: FIXED) { nodes { variant { id } } } } userErrors { field message code } } }",
    )
  let #(Response(status: state_status, body: state_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/attached\") { id priceList { id } } priceList(id: \"gid://shopify/PriceList/attached\") { id fixedPricesCount prices(first: 5, originType: FIXED) { nodes { variant { id } } } } }",
    )

  assert update_status == 200
  assert delete_status == 200
  assert state_status == 200
  assert string.contains(
    json.to_string(update_body),
    "\"fixedPricesCount\":1,\"prices\":{\"nodes\":[{\"originType\":\"FIXED\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/fixed\"}}]}",
  )
  assert json.to_string(delete_body)
    == "{\"data\":{\"priceListDelete\":{\"deletedId\":\"gid://shopify/PriceList/attached\",\"priceList\":{\"id\":\"gid://shopify/PriceList/attached\",\"fixedPricesCount\":0,\"prices\":{\"nodes\":[]}},\"userErrors\":[]}}}"
  assert json.to_string(state_body)
    == "{\"data\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/attached\",\"priceList\":null},\"priceList\":null}}"
}

fn market_delete_cascade_proxy(proxy: DraftProxy) -> DraftProxy {
  let seeded_store =
    proxy.store
    |> store.replace_base_metafields_for_owner(
      "gid://shopify/Product/localizable",
      [market_localization_metafield()],
    )
    |> store.upsert_base_web_presences([market_web_presence()])
  DraftProxy(..proxy, store: seeded_store)
}

fn market_web_presence() -> WebPresenceRecord {
  WebPresenceRecord(
    id: "gid://shopify/MarketWebPresence/market-delete",
    cursor: Some("gid://shopify/MarketWebPresence/market-delete"),
    data: CapturedObject([
      #("__typename", CapturedString("MarketWebPresence")),
      #("id", CapturedString("gid://shopify/MarketWebPresence/market-delete")),
      #(
        "markets",
        CapturedObject([
          #(
            "nodes",
            CapturedArray([
              CapturedObject([
                #("__typename", CapturedString("Market")),
                #("id", CapturedString("gid://shopify/Market/1")),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn attached_catalog_price_list_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_products([attached_fixed_price_product()])
    |> store.upsert_base_product_variants([attached_fixed_price_variant()])
    |> store.upsert_base_catalogs([attached_catalog()])
    |> store.upsert_base_price_lists([attached_price_list()])
  DraftProxy(..proxy, store: seeded_store)
}

fn attached_catalog() -> CatalogRecord {
  CatalogRecord(
    id: "gid://shopify/MarketCatalog/attached",
    cursor: Some("gid://shopify/MarketCatalog/attached"),
    data: CapturedObject([
      #("__typename", CapturedString("MarketCatalog")),
      #("id", CapturedString("gid://shopify/MarketCatalog/attached")),
      #("title", CapturedString("Attached Catalog")),
      #("status", CapturedString("ACTIVE")),
      #("markets", CapturedObject([#("nodes", CapturedArray([]))])),
      #(
        "priceList",
        CapturedObject([
          #("__typename", CapturedString("PriceList")),
          #("id", CapturedString("gid://shopify/PriceList/attached")),
        ]),
      ),
    ]),
  )
}

fn attached_price_list() -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/attached",
    cursor: Some("gid://shopify/PriceList/attached"),
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/attached")),
      #("name", CapturedString("Attached Price List")),
      #("currency", CapturedString("USD")),
      #("fixedPricesCount", CapturedNull),
      #("prices", CapturedObject([#("nodes", CapturedArray([]))])),
      #(
        "catalog",
        CapturedObject([
          #("__typename", CapturedString("MarketCatalog")),
          #("id", CapturedString("gid://shopify/MarketCatalog/attached")),
        ]),
      ),
    ]),
  )
}

fn attached_fixed_price_product() -> ProductRecord {
  ProductRecord(
    id: "gid://shopify/Product/fixed",
    legacy_resource_id: None,
    title: "Fixed Price Product",
    handle: "fixed-price-product",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    price_range_min: None,
    price_range_max: None,
    total_variants: Some(1),
    has_only_default_variant: Some(True),
    has_out_of_stock_variants: Some(False),
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
    combined_listing_role: None,
    combined_listing_parent_id: None,
    combined_listing_child_ids: [],
  )
}

fn attached_fixed_price_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/fixed",
    product_id: "gid://shopify/Product/fixed",
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
        id: "gid://shopify/InventoryItem/fixed",
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

pub fn market_update_adds_and_removes_catalog_links_test() {
  let #(Response(status: first_market_status, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Primary\" }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: second_market_status, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"Secondary\" }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"Linked Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/3\"] } }) { catalog { id markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: noop_status, body: noop_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { catalogsToDelete: [\"gid://shopify/MarketCatalog/5\"] }) { market { id catalogs(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { catalogsToAdd: [\"gid://shopify/MarketCatalog/5\"] }) { market { id catalogs(first: 5) { nodes { id ... on MarketCatalog { markets(first: 5) { nodes { id } } } } } } userErrors { field message code } } }",
    )
  let #(
    Response(status: catalog_read_status, body: catalog_read_body, ..),
    proxy,
  ) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/5\") { id ... on MarketCatalog { markets(first: 5) { nodes { id } } } } }",
    )
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { catalogsToDelete: [\"gid://shopify/MarketCatalog/5\"] }) { market { id catalogs(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(
    Response(status: deleted_catalog_status, body: deleted_catalog_body, ..),
    _,
  ) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/5\") { id ... on MarketCatalog { markets(first: 5) { nodes { id } } } } }",
    )

  assert first_market_status == 200
  assert second_market_status == 200
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/5\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"}]}},\"userErrors\":[]}}}"
  assert noop_status == 200
  assert json.to_string(noop_body)
    == "{\"data\":{\"marketUpdate\":{\"market\":{\"id\":\"gid://shopify/Market/1\",\"catalogs\":{\"nodes\":[]}},\"userErrors\":[]}}}"
  assert add_status == 200
  assert string.contains(
    json.to_string(add_body),
    "\"catalogs\":{\"nodes\":[{\"id\":\"gid://shopify/MarketCatalog/5\"",
  )
  assert string.contains(
    json.to_string(add_body),
    "\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"},{\"id\":\"gid://shopify/Market/1\"}]}",
  )
  assert catalog_read_status == 200
  assert string.contains(
    json.to_string(catalog_read_body),
    "\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"},{\"id\":\"gid://shopify/Market/1\"}]}",
  )
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"marketUpdate\":{\"market\":{\"id\":\"gid://shopify/Market/1\",\"catalogs\":{\"nodes\":[]}},\"userErrors\":[]}}}"
  assert deleted_catalog_status == 200
  assert json.to_string(deleted_catalog_body)
    == "{\"data\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/5\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"}]}}}}"
}

pub fn market_update_adds_and_removes_web_presence_links_test() {
  let #(Response(status: market_status, ..), proxy) =
    graphql_with_proxy(
      seeded_proxy(),
      "mutation { marketCreate(input: { name: \"Primary\" }) { market { id } userErrors { field message code } } }",
    )
  let #(
    Response(status: web_presence_status, body: web_presence_body, ..),
    proxy,
  ) =
    graphql_with_proxy(
      proxy,
      "mutation { webPresenceCreate(input: { defaultLocale: \"en\", subfolderSuffix: \"intl\" }) { webPresence { id markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { webPresencesToAdd: [\"gid://shopify/MarketWebPresence/3\"] }) { market { id webPresences(first: 5) { nodes { id markets(first: 5) { nodes { id } } } } } userErrors { field message code } } }",
    )
  let #(
    Response(status: web_presence_read_status, body: web_presence_read_body, ..),
    proxy,
  ) =
    graphql_with_proxy(
      proxy,
      "query { webPresences(first: 5) { nodes { id markets(first: 5) { nodes { id } } } } }",
    )
  let #(Response(status: delete_status, body: delete_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { webPresencesToDelete: [\"gid://shopify/MarketWebPresence/3\"] }) { market { id webPresences(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )

  assert market_status == 200
  assert web_presence_status == 200
  assert json.to_string(web_presence_body)
    == "{\"data\":{\"webPresenceCreate\":{\"webPresence\":{\"id\":\"gid://shopify/MarketWebPresence/3\",\"markets\":{\"nodes\":[]}},\"userErrors\":[]}}}"
  assert add_status == 200
  assert string.contains(
    json.to_string(add_body),
    "\"webPresences\":{\"nodes\":[{\"id\":\"gid://shopify/MarketWebPresence/3\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}",
  )
  assert web_presence_read_status == 200
  assert string.contains(
    json.to_string(web_presence_read_body),
    "\"id\":\"gid://shopify/MarketWebPresence/3\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"}]}",
  )
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"marketUpdate\":{\"market\":{\"id\":\"gid://shopify/Market/1\",\"webPresences\":{\"nodes\":[]}},\"userErrors\":[]}}}"
}

pub fn market_update_rejects_unknown_link_add_ids_test() {
  let #(Response(status: market_status, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Primary\" }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { catalogsToAdd: [\"gid://shopify/MarketCatalog/9999999999\"] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: web_presence_status, body: web_presence_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { marketUpdate(id: \"gid://shopify/Market/1\", input: { webPresencesToAdd: [\"gid://shopify/MarketWebPresence/9999999999\"] }) { market { id } userErrors { field message code } } }",
    )

  assert market_status == 200
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"marketUpdate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"catalogsToAdd\"],\"message\":\"The following customization IDs were not found: 9999999999\",\"code\":\"CUSTOMIZATIONS_NOT_FOUND\"}]}}}"
  assert web_presence_status == 200
  assert json.to_string(web_presence_body)
    == "{\"data\":{\"marketUpdate\":{\"market\":null,\"userErrors\":[{\"field\":[\"input\",\"webPresencesToAdd\"],\"message\":\"The following customization IDs were not found: 9999999999\",\"code\":\"CUSTOMIZATIONS_NOT_FOUND\"}]}}}"
}

pub fn catalog_create_rejects_unknown_price_list_id_test() {
  let #(Response(status: market_status, body: market_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] }, priceListId: \"gid://shopify/PriceList/9999999999\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"priceListId\"],\"message\":\"Price list not found.\",\"code\":\"PRICE_LIST_NOT_FOUND\"}]}}}"
}

pub fn catalog_create_rejects_taken_price_list_id_test() {
  let proxy = catalog_relation_proxy()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"Second Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] }, priceListId: \"gid://shopify/PriceList/1\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"priceListId\"],\"message\":\"Price list has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn catalog_create_rejects_unknown_publication_id_test() {
  let #(Response(status: market_status, body: market_body, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: catalog_status, body: catalog_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] }, publicationId: \"gid://shopify/Publication/9999999999\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert market_status == 200
  assert json.to_string(market_body)
    == "{\"data\":{\"marketCreate\":{\"market\":{\"id\":\"gid://shopify/Market/1\"},\"userErrors\":[]}}}"
  assert catalog_status == 200
  assert json.to_string(catalog_body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"publicationId\"],\"message\":\"Publication not found.\",\"code\":\"PUBLICATION_NOT_FOUND\"}]}}}"
}

pub fn catalog_create_rejects_taken_publication_id_test() {
  let proxy = catalog_relation_proxy()
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"Second Catalog\", status: ACTIVE, context: { driverType: MARKET, marketIds: [\"gid://shopify/Market/1\"] }, publicationId: \"gid://shopify/Publication/1\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogCreate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"publicationId\"],\"message\":\"Publication is already attached to another catalog\",\"code\":\"PUBLICATION_TAKEN\"}]}}}"
}

pub fn catalog_update_rejects_unknown_attached_ids_test() {
  let proxy = catalog_relation_proxy()
  let #(Response(status: price_status, body: price_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogUpdate(id: \"gid://shopify/MarketCatalog/2\", input: { priceListId: \"gid://shopify/PriceList/9999999999\" }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: publication_status, body: publication_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogUpdate(id: \"gid://shopify/MarketCatalog/2\", input: { publicationId: \"gid://shopify/Publication/9999999999\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert price_status == 200
  assert json.to_string(price_body)
    == "{\"data\":{\"catalogUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"priceListId\"],\"message\":\"Price list not found.\",\"code\":\"PRICE_LIST_NOT_FOUND\"}]}}}"
  assert publication_status == 200
  assert json.to_string(publication_body)
    == "{\"data\":{\"catalogUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"publicationId\"],\"message\":\"Publication not found.\",\"code\":\"PUBLICATION_NOT_FOUND\"}]}}}"
}

pub fn catalog_update_rejects_taken_attached_ids_test() {
  let proxy = catalog_relation_proxy()
  let #(Response(status: price_status, body: price_body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogUpdate(id: \"gid://shopify/MarketCatalog/2\", input: { priceListId: \"gid://shopify/PriceList/1\" }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: publication_status, body: publication_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogUpdate(id: \"gid://shopify/MarketCatalog/2\", input: { publicationId: \"gid://shopify/Publication/1\" }) { catalog { id } userErrors { field message code } } }",
    )

  assert price_status == 200
  assert json.to_string(price_body)
    == "{\"data\":{\"catalogUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"priceListId\"],\"message\":\"Price list has already been taken\",\"code\":\"TAKEN\"}]}}}"
  assert publication_status == 200
  assert json.to_string(publication_body)
    == "{\"data\":{\"catalogUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"input\",\"publicationId\"],\"message\":\"Publication is already attached to another catalog\",\"code\":\"PUBLICATION_TAKEN\"}]}}}"
}

pub fn catalog_context_update_requires_add_or_remove_contexts_test() {
  let #(Response(status: _, body: _, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogContextUpdate(catalogId: \"gid://shopify/MarketCatalog/3\") { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogContextUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"contextsToAdd\"],\"message\":\"Must have `contexts_to_add` or `contexts_to_remove` argument.\",\"code\":\"REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE\"}]}}}"
}

pub fn catalog_context_update_removes_market_contexts_test() {
  let #(Response(status: _, body: _, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"North America\", regions: [{ countryCode: US }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"Global Catalog\", status: ACTIVE, context: { marketIds: [\"gid://shopify/Market/1\", \"gid://shopify/Market/3\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogContextUpdate(catalogId: \"gid://shopify/MarketCatalog/5\", contextsToRemove: { marketIds: [\"gid://shopify/Market/1\"] }) { catalog { id markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/5\") { id markets(first: 5) { nodes { id } } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogContextUpdate\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/5\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"}]}},\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/5\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/3\"}]}}}}"
}

pub fn catalog_context_update_validates_missing_market_context_ids_test() {
  let #(Response(status: _, body: _, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: status, body: body, ..), _) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogContextUpdate(catalogId: \"gid://shopify/MarketCatalog/3\", contextsToAdd: { marketIds: [\"gid://shopify/Market/404\"] }, contextsToRemove: { marketIds: [\"gid://shopify/Market/405\"] }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogContextUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"contextsToAdd\",\"marketIds\",\"0\"],\"message\":\"Market does not exist\",\"code\":\"MARKET_NOT_FOUND\"},{\"field\":[\"contextsToRemove\",\"marketIds\",\"0\"],\"message\":\"Market does not exist\",\"code\":\"MARKET_NOT_FOUND\"}]}}}"
}

pub fn catalog_context_update_allows_market_already_on_another_market_catalog_test() {
  let #(Response(status: _, body: _, ..), proxy) =
    graphql(
      "mutation { marketCreate(input: { name: \"Europe\", regions: [{ countryCode: DK }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { marketCreate(input: { name: \"North America\", regions: [{ countryCode: US }] }) { market { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"EU Catalog\", status: ACTIVE, context: { marketIds: [\"gid://shopify/Market/1\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: _, body: _, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogCreate(input: { title: \"NA Catalog\", status: ACTIVE, context: { marketIds: [\"gid://shopify/Market/3\"] } }) { catalog { id } userErrors { field message code } } }",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(
      proxy,
      "mutation { catalogContextUpdate(catalogId: \"gid://shopify/MarketCatalog/7\", contextsToAdd: { marketIds: [\"gid://shopify/Market/1\"] }) { catalog { id markets(first: 5) { nodes { id } } } userErrors { field message code } } }",
    )
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_with_proxy(
      proxy,
      "query { catalog(id: \"gid://shopify/MarketCatalog/7\") { id markets(first: 5) { nodes { id } } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogContextUpdate\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/7\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"},{\"id\":\"gid://shopify/Market/3\"}]}},\"userErrors\":[]}}}"
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"catalog\":{\"id\":\"gid://shopify/MarketCatalog/7\",\"markets\":{\"nodes\":[{\"id\":\"gid://shopify/Market/1\"},{\"id\":\"gid://shopify/Market/3\"}]}}}}"
}

pub fn catalog_context_update_unknown_catalog_returns_typed_user_error_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      "mutation { catalogContextUpdate(catalogId: \"gid://shopify/MarketCatalog/404\", contextsToAdd: { marketIds: [\"gid://shopify/Market/404\"] }) { catalog { id } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"catalogContextUpdate\":{\"catalog\":null,\"userErrors\":[{\"field\":[\"catalogId\"],\"message\":\"Catalog does not exist\",\"code\":\"CATALOG_NOT_FOUND\"}]}}}"
}

fn catalog_relation_proxy() -> DraftProxy {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry
  let seeded_store =
    proxy.store
    |> store.upsert_base_markets([catalog_relation_market()])
    |> store.upsert_base_price_lists([catalog_relation_price_list()])
    |> store.upsert_base_publications([catalog_relation_publication()])
    |> store.upsert_base_catalogs([
      catalog_with_relations("gid://shopify/MarketCatalog/1"),
      catalog_without_relations("gid://shopify/MarketCatalog/2"),
    ])
  DraftProxy(..proxy, store: seeded_store)
}

fn catalog_relation_market() -> MarketRecord {
  MarketRecord(
    id: "gid://shopify/Market/1",
    cursor: Some("gid://shopify/Market/1"),
    data: CapturedObject([
      #("__typename", CapturedString("Market")),
      #("id", CapturedString("gid://shopify/Market/1")),
      #("name", CapturedString("Europe")),
    ]),
  )
}

fn catalog_relation_price_list() -> PriceListRecord {
  PriceListRecord(
    id: "gid://shopify/PriceList/1",
    cursor: Some("gid://shopify/PriceList/1"),
    data: CapturedObject([
      #("__typename", CapturedString("PriceList")),
      #("id", CapturedString("gid://shopify/PriceList/1")),
      #("name", CapturedString("EU Prices")),
      #("currency", CapturedString("EUR")),
    ]),
  )
}

fn catalog_relation_publication() -> PublicationRecord {
  PublicationRecord(
    id: "gid://shopify/Publication/1",
    name: Some("Online Store"),
    auto_publish: Some(False),
    supports_future_publishing: Some(False),
    catalog_id: None,
    channel_id: None,
    cursor: Some("gid://shopify/Publication/1"),
  )
}

fn catalog_with_relations(id: String) -> CatalogRecord {
  CatalogRecord(
    id: id,
    cursor: Some(id),
    data: CapturedObject([
      #("__typename", CapturedString("MarketCatalog")),
      #("id", CapturedString(id)),
      #("title", CapturedString("First Catalog")),
      #("status", CapturedString("ACTIVE")),
      #("markets", catalog_relation_markets()),
      #("operations", CapturedArray([])),
      #("priceList", catalog_relation_price_list_node()),
      #("publication", catalog_relation_publication_node()),
    ]),
  )
}

fn catalog_without_relations(id: String) -> CatalogRecord {
  CatalogRecord(
    id: id,
    cursor: Some(id),
    data: CapturedObject([
      #("__typename", CapturedString("MarketCatalog")),
      #("id", CapturedString(id)),
      #("title", CapturedString("Second Catalog")),
      #("status", CapturedString("ACTIVE")),
      #("markets", catalog_relation_markets()),
      #("operations", CapturedArray([])),
      #("priceList", CapturedNull),
      #("publication", CapturedNull),
    ]),
  )
}

fn catalog_relation_markets() {
  CapturedObject([
    #(
      "nodes",
      CapturedArray([
        CapturedObject([
          #("__typename", CapturedString("Market")),
          #("id", CapturedString("gid://shopify/Market/1")),
          #("name", CapturedString("Europe")),
        ]),
      ]),
    ),
    #("edges", CapturedArray([])),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", CapturedNull),
        #("endCursor", CapturedNull),
      ]),
    ),
  ])
}

fn catalog_relation_price_list_node() {
  CapturedObject([
    #("__typename", CapturedString("PriceList")),
    #("id", CapturedString("gid://shopify/PriceList/1")),
  ])
}

fn catalog_relation_publication_node() {
  CapturedObject([
    #("__typename", CapturedString("Publication")),
    #("id", CapturedString("gid://shopify/Publication/1")),
  ])
}

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field.{StringVal}
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{type Request, Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  InventoryItemRecord, ProductCategoryRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord,
}

fn graphql_with_variables(query: String, variables: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\""
      <> escape(query)
      <> "\",\"variables\":"
      <> variables
      <> "}",
  )
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
}

const product_feed_create_query: String = "mutation ProductFeedCreate($input: ProductFeedInput) { productFeedCreate(input: $input) { productFeed { id country language status } userErrors { field message code } } }"

pub fn product_empty_state_read_test() {
  let variables =
    dict.from_list([
      #("missingId", StringVal("gid://shopify/Product/999999999999")),
      #("emptyQuery", StringVal("title:'definitely missing'")),
    ])
  let assert Ok(result) =
    products.process(
      store.new(),
      "query ProductEmptyStateConformance($missingId: ID!, $emptyQuery: String!) {
        missingProduct: product(id: $missingId) { id title }
        emptyCount: productsCount(query: $emptyQuery) { count precision }
        emptyProducts: products(first: 3, query: $emptyQuery) {
          edges { cursor node { id title handle } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"missingProduct\":null,\"emptyCount\":{\"count\":0,\"precision\":\"EXACT\"},\"emptyProducts\":{\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}}"
}

pub fn product_related_by_id_missing_read_test() {
  let variables =
    dict.from_list([
      #("missingCollectionId", StringVal("gid://shopify/Collection/1")),
      #("missingProductVariantId", StringVal("gid://shopify/ProductVariant/1")),
      #("missingInventoryItemId", StringVal("gid://shopify/InventoryItem/1")),
      #("missingInventoryLevelId", StringVal("gid://shopify/InventoryLevel/1")),
    ])
  let assert Ok(result) =
    products.process(
      store.new(),
      "query ProductRelatedByIdNotFound(
        $missingCollectionId: ID!
        $missingProductVariantId: ID!
        $missingInventoryItemId: ID!
        $missingInventoryLevelId: ID!
      ) {
        missingCollection: collection(id: $missingCollectionId) { id title handle }
        missingProductVariant: productVariant(id: $missingProductVariantId) { id title sku inventoryItem { id } product { id } }
        missingInventoryItem: inventoryItem(id: $missingInventoryItemId) { id tracked requiresShipping variant { id } }
        missingInventoryLevel: inventoryLevel(id: $missingInventoryLevelId) { id location { id name } quantities(names: [\"available\", \"on_hand\"]) { name quantity updatedAt } }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"missingCollection\":null,\"missingProductVariant\":null,\"missingInventoryItem\":null,\"missingInventoryLevel\":null}}"
}

pub fn product_feeds_empty_read_test() {
  let variables =
    dict.from_list([
      #("missingProductFeedId", StringVal("gid://shopify/ProductFeed/1")),
    ])
  let assert Ok(result) =
    products.process(
      store.new(),
      "query ProductFeedsEmptyRead($missingProductFeedId: ID!) {
        missingProductFeed: productFeed(id: $missingProductFeedId) { id country language status }
        productFeeds(first: 10) {
          nodes { id country language status }
          edges { cursor node { id } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"missingProductFeed\":null,\"productFeeds\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}}"
}

pub fn product_feed_create_rejects_invalid_country_test() {
  let variables = "{\"input\":{\"country\":\"BANANAS\",\"language\":\"EN\"}}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_with_variables(product_feed_create_query, variables),
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productFeedCreate\":{\"productFeed\":null,\"userErrors\":[{\"field\":[\"country\"],\"message\":\"Country is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn product_feed_create_rejects_invalid_language_test() {
  let variables = "{\"input\":{\"country\":\"US\",\"language\":\"BANANAS\"}}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_with_variables(product_feed_create_query, variables),
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productFeedCreate\":{\"productFeed\":null,\"userErrors\":[{\"field\":[\"language\"],\"message\":\"Language is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn product_feed_create_rejects_duplicate_country_language_test() {
  let variables = "{\"input\":{\"country\":\"US\",\"language\":\"EN\"}}"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_with_variables(product_feed_create_query, variables),
    )
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productFeedCreate\":{\"productFeed\":{\"id\":\"gid://shopify/ProductFeed/US-EN\",\"country\":\"US\",\"language\":\"EN\",\"status\":\"ACTIVE\"},\"userErrors\":[]}}}"

  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_with_variables(product_feed_create_query, variables),
    )
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"productFeedCreate\":{\"productFeed\":null,\"userErrors\":[{\"field\":[\"country\"],\"message\":\"Product feed already exists for this country/language pair\",\"code\":\"TAKEN\"}]}}}"

  let read_query =
    "query { productFeed(id: \"gid://shopify/ProductFeed/US-EN\") { id country language status } productFeeds(first: 10) { nodes { id country language status } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_with_variables(read_query, "{}"))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"productFeed\":{\"id\":\"gid://shopify/ProductFeed/US-EN\",\"country\":\"US\",\"language\":\"EN\",\"status\":\"ACTIVE\"},\"productFeeds\":{\"nodes\":[{\"id\":\"gid://shopify/ProductFeed/US-EN\",\"country\":\"US\",\"language\":\"EN\",\"status\":\"ACTIVE\"}]}}}"
}

pub fn product_duplicate_job_unknown_shape_test() {
  let variables =
    dict.from_list([
      #("missingJobId", StringVal("gid://shopify/ProductDuplicateJob/999")),
    ])
  let assert Ok(result) =
    products.process(
      store.new(),
      "query ProductDuplicateJob($missingJobId: ID!) {
        productDuplicateJob(id: $missingJobId) { id done }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"productDuplicateJob\":{\"id\":\"gid://shopify/ProductDuplicateJob/999\",\"done\":true}}}"
}

pub fn seeded_product_detail_read_test() {
  let product =
    ProductRecord(
      id: "gid://shopify/Product/8971842846953",
      legacy_resource_id: None,
      title: "Test Product - 6635",
      handle: "test-product-ge91cbbd6",
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: None,
      tracks_inventory: None,
      created_at: None,
      updated_at: None,
      published_at: None,
      description_html: "",
      online_store_preview_url: Some(
        "https://very-big-test-store.myshopify.com/products/test-product-ge91cbbd6",
      ),
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: Some(ProductCategoryRecord(
        id: "gid://shopify/TaxonomyCategory/na",
        full_name: "Uncategorized",
      )),
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    )
  let seeded_store = store.upsert_base_products(store.new(), [product])
  let variables =
    dict.from_list([
      #("id", StringVal("gid://shopify/Product/8971842846953")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_store,
      "query ProductDetailRead($id: ID!) {
        product(id: $id) {
          id
          title
          handle
          status
          descriptionHtml
          onlineStorePreviewUrl
          templateSuffix
          seo { title description }
          category { id fullName }
          collections(first: 3) { edges { node { id title handle } } }
          media(first: 5) { edges { node { mediaContentType alt preview { image { url } } } } }
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/8971842846953\",\"title\":\"Test Product - 6635\",\"handle\":\"test-product-ge91cbbd6\",\"status\":\"ACTIVE\",\"descriptionHtml\":\"\",\"onlineStorePreviewUrl\":\"https://very-big-test-store.myshopify.com/products/test-product-ge91cbbd6\",\"templateSuffix\":null,\"seo\":{\"title\":null,\"description\":null},\"category\":{\"id\":\"gid://shopify/TaxonomyCategory/na\",\"fullName\":\"Uncategorized\"},\"collections\":{\"edges\":[]},\"media\":{\"edges\":[]}}}}"
}

pub fn seeded_product_by_identifier_id_read_test() {
  let seeded_store = seeded_identifier_store()
  let variables =
    dict.from_list([
      #("id", StringVal("gid://shopify/Product/9801098789170")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_store,
      "query ProductByIdentifierId($id: ID!) {
        productByIdentifier(identifier: { id: $id }) {
          id
          handle
          title
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"productByIdentifier\":{\"id\":\"gid://shopify/Product/9801098789170\",\"handle\":\"the-inventory-not-tracked-snowboard\",\"title\":\"The Inventory Not Tracked Snowboard\"}}}"
}

pub fn seeded_product_by_identifier_handle_read_test() {
  let seeded_store = seeded_identifier_store()
  let variables =
    dict.from_list([
      #("handle", StringVal("the-inventory-not-tracked-snowboard")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_store,
      "query ProductByIdentifierHandle($handle: String!) {
        productByIdentifier(identifier: { handle: $handle }) {
          id
          handle
          title
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"productByIdentifier\":{\"id\":\"gid://shopify/Product/9801098789170\",\"handle\":\"the-inventory-not-tracked-snowboard\",\"title\":\"The Inventory Not Tracked Snowboard\"}}}"
}

pub fn seeded_product_by_identifier_missing_read_test() {
  let seeded_store = seeded_identifier_store()
  let variables =
    dict.from_list([
      #("missingId", StringVal("gid://shopify/Product/999999999999")),
      #("missingHandle", StringVal("missing-product-handle")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_store,
      "query ProductByIdentifierMissing($missingId: ID!, $missingHandle: String!) {
        missingById: productByIdentifier(identifier: { id: $missingId }) {
          id
          handle
          title
        }
        missingByHandle: productByIdentifier(identifier: { handle: $missingHandle }) {
          id
          handle
          title
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"missingById\":null,\"missingByHandle\":null}}"
}

pub fn seeded_product_string_catalogs_read_test() {
  let assert Ok(result) =
    products.process(
      seeded_string_catalog_store(),
      "query ProductStringCatalogs {
        productTags(first: 10) {
          nodes
          edges { cursor node }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
        productTypes(first: 10) {
          nodes
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
        productVendors(first: 10) {
          nodes
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"productTags\":{\"nodes\":[\"Board\",\"Hat\",\"Winter\"],\"edges\":[{\"cursor\":\"cursor:Board\",\"node\":\"Board\"},{\"cursor\":\"cursor:Hat\",\"node\":\"Hat\"},{\"cursor\":\"cursor:Winter\",\"node\":\"Winter\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:Board\",\"endCursor\":\"cursor:Winter\"}},\"productTypes\":{\"nodes\":[\"Accessory\",\"Snowboard\"],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:Accessory\",\"endCursor\":\"cursor:Snowboard\"}},\"productVendors\":{\"nodes\":[\"Acme\",\"Bravo\"],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:Acme\",\"endCursor\":\"cursor:Bravo\"}}}}"
}

pub fn seeded_product_string_catalogs_reverse_page_test() {
  let assert Ok(result) =
    products.process(
      seeded_string_catalog_store(),
      "query ProductStringCatalogsReverse {
        productTags(first: 2, reverse: true) {
          nodes
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"productTags\":{\"nodes\":[\"Winter\",\"Hat\"],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:Winter\",\"endCursor\":\"cursor:Hat\"}}}}"
}

pub fn seeded_product_variant_by_identifier_read_test() {
  let variables =
    dict.from_list([
      #("variantId", StringVal("gid://shopify/ProductVariant/2")),
      #("missingVariantId", StringVal("gid://shopify/ProductVariant/404")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_variant_store(),
      "query ProductVariantHelpers($variantId: ID!, $missingVariantId: ID!) {
        variant: productVariant(id: $variantId) {
          id
          title
          sku
          product { id handle title }
        }
        byId: productVariantByIdentifier(identifier: { id: $variantId }) {
          id
          title
          sku
          product { id }
        }
        missing: productVariantByIdentifier(identifier: { id: $missingVariantId }) {
          id
          title
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"variant\":{\"id\":\"gid://shopify/ProductVariant/2\",\"title\":\"Default Title\",\"sku\":\"sku-untracked-1\",\"product\":{\"id\":\"gid://shopify/Product/1\",\"handle\":\"helper-hat\",\"title\":\"Helper Hat\"}},\"byId\":{\"id\":\"gid://shopify/ProductVariant/2\",\"title\":\"Default Title\",\"sku\":\"sku-untracked-1\",\"product\":{\"id\":\"gid://shopify/Product/1\"}},\"missing\":null}}"
}

pub fn seeded_product_variants_connection_read_test() {
  let assert Ok(result) =
    products.process(
      seeded_variant_store(),
      "query ProductVariantConnection {
        productVariantsCount { count precision }
        productVariants(first: 2, sortKey: ID) {
          nodes { id title sku product { id } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"productVariantsCount\":{\"count\":3,\"precision\":\"EXACT\"},\"productVariants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/2\",\"title\":\"Default Title\",\"sku\":\"sku-untracked-1\",\"product\":{\"id\":\"gid://shopify/Product/1\"}},{\"id\":\"gid://shopify/ProductVariant/10\",\"title\":\"Blue\",\"sku\":null,\"product\":{\"id\":\"gid://shopify/Product/1\"}}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/ProductVariant/2\",\"endCursor\":\"cursor:gid://shopify/ProductVariant/10\"}}}}"
}

pub fn seeded_products_variant_sku_search_read_test() {
  let variables = dict.from_list([#("query", StringVal("sku:sku-untracked-1"))])
  let assert Ok(result) =
    products.process(
      seeded_variant_store(),
      "query ProductsVariantSkuSearch($query: String!) {
        matches: productsCount(query: $query) { count precision }
        products(first: 5, query: $query) {
          edges { node { id title handle } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"matches\":{\"count\":1,\"precision\":\"EXACT\"},\"products\":{\"edges\":[{\"node\":{\"id\":\"gid://shopify/Product/1\",\"title\":\"Helper Hat\",\"handle\":\"helper-hat\"}}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"gid://shopify/Product/1\",\"endCursor\":\"gid://shopify/Product/1\"}}}}"
}

pub fn seeded_products_scalar_search_read_test() {
  let variables =
    dict.from_list([
      #("vendorQuery", StringVal("vendor:NIKE")),
      #("inventoryQuery", StringVal("inventory_total:<=0")),
      #("typeQuery", StringVal("product_type:ACCESSORIES")),
      #("tagQuery", StringVal("tag:Hat")),
      #("idQuery", StringVal("id:101")),
      #("textQuery", StringVal("running")),
    ])
  let assert Ok(result) =
    products.process(
      seeded_product_search_store(),
      "query ProductsScalarSearch(
        $vendorQuery: String!
        $inventoryQuery: String!
        $typeQuery: String!
        $tagQuery: String!
        $idQuery: String!
        $textQuery: String!
      ) {
        vendor: products(first: 5, query: $vendorQuery) {
          nodes { id title vendor }
        }
        lowInventory: productsCount(query: $inventoryQuery) { count precision }
        accessories: productsCount(query: $typeQuery) { count precision }
        hats: productsCount(query: $tagQuery) { count precision }
        byId: products(first: 5, query: $idQuery) { nodes { id title } }
        text: productsCount(query: $textQuery) { count precision }
        active: productsCount(query: \"status:ACTIVE\") { count precision }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"vendor\":{\"nodes\":[{\"id\":\"gid://shopify/Product/100\",\"title\":\"Running Cap\",\"vendor\":\"NIKE\"},{\"id\":\"gid://shopify/Product/101\",\"title\":\"Trail Shoe\",\"vendor\":\"NIKE\"}]},\"lowInventory\":{\"count\":2,\"precision\":\"EXACT\"},\"accessories\":{\"count\":2,\"precision\":\"EXACT\"},\"hats\":{\"count\":1,\"precision\":\"EXACT\"},\"byId\":{\"nodes\":[{\"id\":\"gid://shopify/Product/101\",\"title\":\"Trail Shoe\"}]},\"text\":{\"count\":1,\"precision\":\"EXACT\"},\"active\":{\"count\":2,\"precision\":\"EXACT\"}}}"
}

pub fn seeded_inventory_items_connection_read_test() {
  let assert Ok(result) =
    products.process(
      seeded_inventory_item_store(),
      "query InventoryItemsRead {
        inventoryItems(first: 5) {
          nodes {
            id
            tracked
            requiresShipping
            variant { id sku product { id } }
          }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
        bySku: inventoryItems(first: 5, query: \"sku:sku-low\") {
          nodes { id variant { sku } }
        }
        tracked: inventoryItems(first: 5, query: \"tracked:true\") {
          nodes { id tracked }
        }
        byId: inventoryItems(first: 5, query: \"id:20\") {
          nodes { id variant { id } }
        }
        missing: inventoryItems(first: 1, query: \"id:0\") {
          nodes { id }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"inventoryItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryItem/20\",\"tracked\":false,\"requiresShipping\":true,\"variant\":{\"id\":\"gid://shopify/ProductVariant/20\",\"sku\":\"sku-untracked\",\"product\":{\"id\":\"gid://shopify/Product/1\"}}},{\"id\":\"gid://shopify/InventoryItem/30\",\"tracked\":true,\"requiresShipping\":true,\"variant\":{\"id\":\"gid://shopify/ProductVariant/30\",\"sku\":\"sku-low\",\"product\":{\"id\":\"gid://shopify/Product/2\"}}}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/InventoryItem/20\",\"endCursor\":\"cursor:gid://shopify/InventoryItem/30\"}},\"bySku\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryItem/30\",\"variant\":{\"sku\":\"sku-low\"}}]},\"tracked\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryItem/30\",\"tracked\":true}]},\"byId\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryItem/20\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/20\"}}]},\"missing\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}}"
}

pub fn inventory_properties_quantity_names_read_test() {
  let assert Ok(result) =
    products.process(
      store.new(),
      "query InventoryPropertiesRead {
        inventoryProperties {
          quantityNames {
            name
            displayName
            isInUse
            belongsTo
            comprises
          }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"inventoryProperties\":{\"quantityNames\":[{\"name\":\"available\",\"displayName\":\"Available\",\"isInUse\":true,\"belongsTo\":[\"on_hand\"],\"comprises\":[]},{\"name\":\"committed\",\"displayName\":\"Committed\",\"isInUse\":true,\"belongsTo\":[\"on_hand\"],\"comprises\":[]},{\"name\":\"damaged\",\"displayName\":\"Damaged\",\"isInUse\":false,\"belongsTo\":[\"on_hand\"],\"comprises\":[]},{\"name\":\"incoming\",\"displayName\":\"Incoming\",\"isInUse\":false,\"belongsTo\":[],\"comprises\":[]},{\"name\":\"on_hand\",\"displayName\":\"On hand\",\"isInUse\":true,\"belongsTo\":[],\"comprises\":[\"available\",\"committed\",\"damaged\",\"quality_control\",\"reserved\",\"safety_stock\"]},{\"name\":\"quality_control\",\"displayName\":\"Quality control\",\"isInUse\":false,\"belongsTo\":[\"on_hand\"],\"comprises\":[]},{\"name\":\"reserved\",\"displayName\":\"Reserved\",\"isInUse\":true,\"belongsTo\":[\"on_hand\"],\"comprises\":[]},{\"name\":\"safety_stock\",\"displayName\":\"Safety stock\",\"isInUse\":false,\"belongsTo\":[\"on_hand\"],\"comprises\":[]}]}}}"
}

pub fn seeded_product_options_read_test() {
  let assert Ok(result) =
    products.process(
      seeded_product_option_store(),
      "query ProductOptionsRead($productId: ID!) {
        product(id: $productId) {
          id
          options {
            __typename
            id
            name
            position
            values
            optionValues {
              __typename
              id
              name
              hasVariants
            }
          }
          variants(first: 5) {
            nodes {
              id
              selectedOptions { name value }
            }
          }
        }
      }",
      dict.from_list([
        #("productId", StringVal("gid://shopify/Product/optioned")),
      ]),
    )
  assert json.to_string(result)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"__typename\":\"ProductOption\",\"id\":\"gid://shopify/ProductOption/color\",\"name\":\"Color\",\"position\":1,\"values\":[\"Red\"],\"optionValues\":[{\"__typename\":\"ProductOptionValue\",\"id\":\"gid://shopify/ProductOptionValue/red\",\"name\":\"Red\",\"hasVariants\":true},{\"__typename\":\"ProductOptionValue\",\"id\":\"gid://shopify/ProductOptionValue/blue\",\"name\":\"Blue\",\"hasVariants\":false}]},{\"__typename\":\"ProductOption\",\"id\":\"gid://shopify/ProductOption/size\",\"name\":\"Size\",\"position\":2,\"values\":[\"Small\"],\"optionValues\":[{\"__typename\":\"ProductOptionValue\",\"id\":\"gid://shopify/ProductOptionValue/small\",\"name\":\"Small\",\"hasVariants\":true}]}],\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/optioned\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"},{\"name\":\"Size\",\"value\":\"Small\"}]}]}}}}"
}

pub fn seeded_products_catalog_read_test() {
  let product =
    ProductRecord(
      id: "gid://shopify/Product/8971842846953",
      legacy_resource_id: Some("8971842846953"),
      title: "Test Product - 6635",
      handle: "test-product-ge91cbbd6",
      status: "ACTIVE",
      vendor: Some("very-big-test-store"),
      product_type: Some(""),
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: Some(0),
      tracks_inventory: Some(False),
      created_at: Some("2025-07-01T23:57:25Z"),
      updated_at: Some("2026-04-18T00:58:21Z"),
      published_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: Some(
        "eyJsYXN0X2lkIjo4OTcxODQyODQ2OTUzLCJsYXN0X3ZhbHVlIjoiMjAyNi0wNC0xOCAwMDo1ODoyMS4wMDAwMDAifQ==",
      ),
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    )
  let seeded_store =
    store.new()
    |> store.upsert_base_products([product])
    |> store.set_base_product_count(13_552)
  let assert Ok(result) =
    products.process(
      seeded_store,
      "query ProductsCatalogRead {
        productsCount { count precision }
        products(first: 1, sortKey: UPDATED_AT, reverse: true) {
          edges {
            cursor
            node {
              id
              legacyResourceId
              title
              handle
              status
              vendor
              productType
              tags
              totalInventory
              tracksInventory
              createdAt
              updatedAt
            }
          }
          pageInfo { hasNextPage endCursor }
        }
      }",
      dict.new(),
    )
  assert json.to_string(result)
    == "{\"data\":{\"productsCount\":{\"count\":13552,\"precision\":\"EXACT\"},\"products\":{\"edges\":[{\"cursor\":\"eyJsYXN0X2lkIjo4OTcxODQyODQ2OTUzLCJsYXN0X3ZhbHVlIjoiMjAyNi0wNC0xOCAwMDo1ODoyMS4wMDAwMDAifQ==\",\"node\":{\"id\":\"gid://shopify/Product/8971842846953\",\"legacyResourceId\":\"8971842846953\",\"title\":\"Test Product - 6635\",\"handle\":\"test-product-ge91cbbd6\",\"status\":\"ACTIVE\",\"vendor\":\"very-big-test-store\",\"productType\":\"\",\"tags\":[],\"totalInventory\":0,\"tracksInventory\":false,\"createdAt\":\"2025-07-01T23:57:25Z\",\"updatedAt\":\"2026-04-18T00:58:21Z\"}}],\"pageInfo\":{\"hasNextPage\":true,\"endCursor\":\"eyJsYXN0X2lkIjo4OTcxODQyODQ2OTUzLCJsYXN0X3ZhbHVlIjoiMjAyNi0wNC0xOCAwMDo1ODoyMS4wMDAwMDAifQ==\"}}}}"
}

fn seeded_identifier_store() {
  store.upsert_base_products(store.new(), [identifier_product()])
}

fn identifier_product() {
  ProductRecord(
    id: "gid://shopify/Product/9801098789170",
    legacy_resource_id: Some("9801098789170"),
    title: "The Inventory Not Tracked Snowboard",
    handle: "the-inventory-not-tracked-snowboard",
    status: "ACTIVE",
    vendor: Some("Snowdevil"),
    product_type: Some("snowboard"),
    tags: [],
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
    combined_listing_role: None,
    combined_listing_parent_id: None,
    combined_listing_child_ids: [],
  )
}

fn seeded_string_catalog_store() {
  store.upsert_base_products(store.new(), [
    string_catalog_product(
      "gid://shopify/Product/1",
      "Helper Hat",
      "helper-hat",
      Some("Acme"),
      Some("Accessory"),
      ["Winter", "Hat", ""],
    ),
    string_catalog_product(
      "gid://shopify/Product/2",
      "Helper Board",
      "helper-board",
      Some("Bravo"),
      Some("Snowboard"),
      ["Winter", "Board"],
    ),
    string_catalog_product(
      "gid://shopify/Product/3",
      "Helper Blank",
      "helper-blank",
      None,
      Some(""),
      ["  "],
    ),
  ])
}

fn string_catalog_product(
  id: String,
  title: String,
  handle: String,
  vendor: Option(String),
  product_type: Option(String),
  tags: List(String),
) {
  ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: handle,
    status: "ACTIVE",
    vendor: vendor,
    product_type: product_type,
    tags: tags,
    price_range_min: None,
    price_range_max: None,
    total_variants: None,
    has_only_default_variant: None,
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

fn seeded_variant_store() {
  store.new()
  |> store.upsert_base_products([
    string_catalog_product(
      "gid://shopify/Product/1",
      "Helper Hat",
      "helper-hat",
      Some("Acme"),
      Some("Accessory"),
      ["Winter", "Hat"],
    ),
    string_catalog_product(
      "gid://shopify/Product/2",
      "Helper Board",
      "helper-board",
      Some("Bravo"),
      Some("Snowboard"),
      ["Winter", "Board"],
    ),
  ])
  |> store.upsert_base_product_variants([
    variant_record(
      "gid://shopify/ProductVariant/10",
      "gid://shopify/Product/1",
      "Blue",
      None,
    ),
    variant_record(
      "gid://shopify/ProductVariant/2",
      "gid://shopify/Product/1",
      "Default Title",
      Some("sku-untracked-1"),
    ),
    variant_record(
      "gid://shopify/ProductVariant/11",
      "gid://shopify/Product/2",
      "$10",
      None,
    ),
  ])
}

fn seeded_inventory_item_store() {
  store.new()
  |> store.upsert_base_products([
    string_catalog_product(
      "gid://shopify/Product/1",
      "Helper Hat",
      "helper-hat",
      Some("Acme"),
      Some("Accessory"),
      ["Winter", "Hat"],
    ),
    string_catalog_product(
      "gid://shopify/Product/2",
      "Helper Board",
      "helper-board",
      Some("Bravo"),
      Some("Snowboard"),
      ["Winter", "Board"],
    ),
  ])
  |> store.upsert_base_product_variants([
    inventory_variant_record(
      "gid://shopify/ProductVariant/30",
      "gid://shopify/Product/2",
      "Default Title",
      Some("sku-low"),
      "gid://shopify/InventoryItem/30",
      Some(True),
    ),
    inventory_variant_record(
      "gid://shopify/ProductVariant/20",
      "gid://shopify/Product/1",
      "Default Title",
      Some("sku-untracked"),
      "gid://shopify/InventoryItem/20",
      Some(False),
    ),
  ])
}

fn seeded_product_option_store() {
  store.new()
  |> store.upsert_base_products([
    string_catalog_product(
      "gid://shopify/Product/optioned",
      "Optioned Board",
      "optioned-board",
      Some("Acme"),
      Some("Snowboard"),
      ["Winter"],
    ),
  ])
  |> store.upsert_base_product_variants([
    ProductVariantRecord(
      id: "gid://shopify/ProductVariant/optioned",
      product_id: "gid://shopify/Product/optioned",
      title: "Red / Small",
      sku: Some("optioned-red-small"),
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: None,
      selected_options: [
        ProductVariantSelectedOptionRecord(name: "Color", value: "Red"),
        ProductVariantSelectedOptionRecord(name: "Size", value: "Small"),
      ],
      media_ids: [],
      inventory_item: None,
      contextual_pricing: None,
      cursor: None,
    ),
  ])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/size",
      product_id: "gid://shopify/Product/optioned",
      name: "Size",
      position: 2,
      linked_metafield: None,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/small",
          name: "Small",
          has_variants: True,
          linked_metafield_value: None,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      linked_metafield: None,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
          linked_metafield_value: None,
        ),
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/blue",
          name: "Blue",
          has_variants: False,
          linked_metafield_value: None,
        ),
      ],
    ),
  ])
}

fn variant_record(
  id: String,
  product_id: String,
  title: String,
  sku: Option(String),
) {
  ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: title,
    sku: sku,
    barcode: None,
    price: None,
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

fn inventory_variant_record(
  id: String,
  product_id: String,
  title: String,
  sku: Option(String),
  inventory_item_id: String,
  tracked: Option(Bool),
) {
  ProductVariantRecord(
    ..variant_record(id, product_id, title, sku),
    inventory_item: Some(
      InventoryItemRecord(
        id: inventory_item_id,
        tracked: tracked,
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
  )
}

fn seeded_product_search_store() {
  store.upsert_base_products(store.new(), [
    search_product(
      "gid://shopify/Product/100",
      "Running Cap",
      "running-cap",
      "ACTIVE",
      Some("NIKE"),
      Some("ACCESSORIES"),
      ["egnition-sample-data", "Hat"],
      Some(0),
    ),
    search_product(
      "gid://shopify/Product/101",
      "Trail Shoe",
      "trail-shoe",
      "DRAFT",
      Some("NIKE"),
      Some("FOOTWEAR"),
      ["sample"],
      Some(12),
    ),
    search_product(
      "gid://shopify/Product/102",
      "Classic Slip-On",
      "classic-slip-on",
      "ACTIVE",
      Some("VANS"),
      Some("ACCESSORIES"),
      ["vans"],
      Some(0),
    ),
  ])
}

fn search_product(
  id: String,
  title: String,
  handle: String,
  status: String,
  vendor: Option(String),
  product_type: Option(String),
  tags: List(String),
  total_inventory: Option(Int),
) {
  ProductRecord(
    id: id,
    legacy_resource_id: resource_tail(id),
    title: title,
    handle: handle,
    status: status,
    vendor: vendor,
    product_type: product_type,
    tags: tags,
    price_range_min: None,
    price_range_max: None,
    total_variants: None,
    has_only_default_variant: None,
    has_out_of_stock_variants: None,
    total_inventory: total_inventory,
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

fn resource_tail(id: String) -> Option(String) {
  case string.split(id, "/") |> list.last {
    Ok(tail) -> Some(tail)
    Error(_) -> None
  }
}

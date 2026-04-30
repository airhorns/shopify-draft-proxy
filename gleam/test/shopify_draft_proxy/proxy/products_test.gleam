import gleam/dict
import gleam/json
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/root_field.{StringVal}
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  ProductCategoryRecord, ProductRecord, ProductSeoRecord, ProductVariantRecord,
}

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
      total_inventory: None,
      tracks_inventory: None,
      created_at: None,
      updated_at: None,
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
      cursor: None,
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
      total_inventory: Some(0),
      tracks_inventory: Some(False),
      created_at: Some("2025-07-01T23:57:25Z"),
      updated_at: Some("2026-04-18T00:58:21Z"),
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      cursor: Some(
        "eyJsYXN0X2lkIjo4OTcxODQyODQ2OTUzLCJsYXN0X3ZhbHVlIjoiMjAyNi0wNC0xOCAwMDo1ODoyMS4wMDAwMDAifQ==",
      ),
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
    total_inventory: None,
    tracks_inventory: None,
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
    inventory_item: None,
    cursor: None,
  )
}

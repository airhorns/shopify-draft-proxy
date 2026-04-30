import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/root_field.{StringVal}
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  ProductCategoryRecord, ProductRecord, ProductSeoRecord,
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

import gleam/dict
import gleam/json
import shopify_draft_proxy/graphql/root_field.{StringVal}
import shopify_draft_proxy/proxy/products

pub fn product_empty_state_read_test() {
  let variables =
    dict.from_list([
      #("missingId", StringVal("gid://shopify/Product/999999999999")),
      #("emptyQuery", StringVal("title:'definitely missing'")),
    ])
  let assert Ok(result) =
    products.process(
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
      "query ProductDuplicateJob($missingJobId: ID!) {
        productDuplicateJob(id: $missingJobId) { id done }
      }",
      variables,
    )
  assert json.to_string(result)
    == "{\"data\":{\"productDuplicateJob\":{\"id\":\"gid://shopify/ProductDuplicateJob/999\",\"done\":true}}}"
}

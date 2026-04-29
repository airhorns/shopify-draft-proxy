import gleam/dict
import gleam/json
import shopify_draft_proxy/proxy/saved_searches
import shopify_draft_proxy/state/store

fn handle(query: String) -> String {
  let assert Ok(data) =
    saved_searches.handle_saved_search_query(store.new(), query, dict.new())
  json.to_string(data)
}

pub fn order_saved_searches_first_two_test() {
  let result =
    handle(
      "{ orderSavedSearches(first: 2) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"id\":\"gid://shopify/SavedSearch/3634391515442\",\"name\":\"Unfulfilled\"},{\"id\":\"gid://shopify/SavedSearch/3634391548210\",\"name\":\"Unpaid\"}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/SavedSearch/3634391515442\",\"endCursor\":\"cursor:gid://shopify/SavedSearch/3634391548210\"}}}"
}

pub fn order_saved_searches_returns_all_four_when_unwindowed_test() {
  let result =
    handle("{ orderSavedSearches { nodes { name } } }")
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Unfulfilled\"},{\"name\":\"Unpaid\"},{\"name\":\"Open\"},{\"name\":\"Archived\"}]}}"
}

pub fn draft_order_saved_searches_first_two_test() {
  let result =
    handle(
      "{ draftOrderSavedSearches(first: 2) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"draftOrderSavedSearches\":{\"nodes\":[{\"id\":\"gid://shopify/SavedSearch/3634390597938\",\"name\":\"Open and invoice sent\"},{\"id\":\"gid://shopify/SavedSearch/3634390630706\",\"name\":\"Open\"}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/SavedSearch/3634390597938\",\"endCursor\":\"cursor:gid://shopify/SavedSearch/3634390630706\"}}}"
}

pub fn empty_saved_search_resource_returns_empty_connection_test() {
  // PRODUCT has no defaults yet; expect an empty connection.
  let result =
    handle(
      "{ productSavedSearches(first: 2) { nodes { id } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"productSavedSearches\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn order_saved_searches_query_filters_by_substring_test() {
  let result =
    handle(
      "{ orderSavedSearches(query: \"unfulfilled\") { nodes { name } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Unfulfilled\"}]}}"
}

pub fn order_saved_searches_query_filters_by_query_text_test() {
  // The TS matcher includes `record.query` in the haystack, so
  // searching for `financial_status` returns Unpaid (whose query is
  // `status:open financial_status:unpaid`).
  let result =
    handle(
      "{ orderSavedSearches(query: \"financial_status\") { nodes { name } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Unpaid\"}]}}"
}

pub fn order_saved_searches_query_no_match_returns_empty_test() {
  let result =
    handle(
      "{ orderSavedSearches(query: \"__no_saved_search_match__\") { nodes { name } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[]}}"
}

pub fn order_saved_searches_reverse_test() {
  let result =
    handle(
      "{ orderSavedSearches(reverse: true) { nodes { name } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Archived\"},{\"name\":\"Open\"},{\"name\":\"Unpaid\"},{\"name\":\"Unfulfilled\"}]}}"
}

pub fn order_saved_searches_after_cursor_test() {
  let result =
    handle(
      "{ orderSavedSearches(after: \"cursor:gid://shopify/SavedSearch/3634391548210\") { nodes { name } pageInfo { hasNextPage hasPreviousPage } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Open\"},{\"name\":\"Archived\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":true}}}"
}

pub fn order_saved_searches_edges_cursor_format_test() {
  let result =
    handle(
      "{ orderSavedSearches(first: 1) { edges { cursor node { id } } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"edges\":[{\"cursor\":\"cursor:gid://shopify/SavedSearch/3634391515442\",\"node\":{\"id\":\"gid://shopify/SavedSearch/3634391515442\"}}]}}"
}

pub fn order_saved_searches_full_node_shape_test() {
  let result =
    handle(
      "{ orderSavedSearches(first: 1) { nodes { __typename id legacyResourceId name query resourceType searchTerms filters { key value } } } }",
    )
  assert result
    == "{\"orderSavedSearches\":{\"nodes\":[{\"__typename\":\"SavedSearch\",\"id\":\"gid://shopify/SavedSearch/3634391515442\",\"legacyResourceId\":\"3634391515442\",\"name\":\"Unfulfilled\",\"query\":\"status:open fulfillment_status:unshipped,partial\",\"resourceType\":\"ORDER\",\"searchTerms\":\"\",\"filters\":[{\"key\":\"status\",\"value\":\"open\"},{\"key\":\"fulfillment_status\",\"value\":\"unshipped,partial\"}]}]}}"
}

pub fn aliased_root_test() {
  let result =
    handle(
      "{ ord: orderSavedSearches(first: 1) { nodes { id } } }",
    )
  assert result
    == "{\"ord\":{\"nodes\":[{\"id\":\"gid://shopify/SavedSearch/3634391515442\"}]}}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(envelope) =
    saved_searches.process(
      store.new(),
      "{ orderSavedSearches(first: 1) { nodes { id } } }",
      dict.new(),
    )
  assert json.to_string(envelope)
    == "{\"data\":{\"orderSavedSearches\":{\"nodes\":[{\"id\":\"gid://shopify/SavedSearch/3634391515442\"}]}}}"
}

pub fn parse_failure_propagates_test() {
  let assert Error(saved_searches.ParseFailed(_)) =
    saved_searches.handle_saved_search_query(
      store.new(),
      "{ orderSavedSearches(",
      dict.new(),
    )
}

pub fn is_saved_search_query_root_recognises_each_root_test() {
  let names = [
    "automaticDiscountSavedSearches", "codeDiscountSavedSearches",
    "collectionSavedSearches", "customerSavedSearches",
    "discountRedeemCodeSavedSearches", "draftOrderSavedSearches",
    "fileSavedSearches", "orderSavedSearches", "productSavedSearches",
  ]
  let all_ok = list_all(names, saved_searches.is_saved_search_query_root)
  assert all_ok == True
}

pub fn is_saved_search_query_root_rejects_unknown_test() {
  assert saved_searches.is_saved_search_query_root("events") == False
  assert saved_searches.is_saved_search_query_root("") == False
}

fn list_all(items: List(a), predicate: fn(a) -> Bool) -> Bool {
  case items {
    [] -> True
    [item, ..rest] ->
      case predicate(item) {
        True -> list_all(rest, predicate)
        False -> False
      }
  }
}

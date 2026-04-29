import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy.{
  type Request, Config, LiveHybrid, Request, Response, Snapshot,
}

fn empty_headers() -> dict.Dict(String, String) {
  dict.new()
}

pub fn health_endpoint_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/health",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"ok\":true,\"message\":\"shopify-draft-proxy is running\"}"
}

pub fn health_endpoint_method_not_allowed_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/health",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 405
}

pub fn unknown_path_returns_404_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/totally-unknown",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 404
}

pub fn graphql_events_query_returns_envelope_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ events(first: 5) { nodes { id } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"data\":{\"events\":{\"nodes\":[]}}}"
}

pub fn graphql_event_query_returns_null_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ event(id: \\\"gid://shopify/Event/1\\\") { id } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"data\":{\"event\":null}}"
}

pub fn graphql_with_get_returns_405_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 405
}

pub fn graphql_with_invalid_body_returns_400_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "not-json",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 400
}

pub fn graphql_unimplemented_domain_returns_400_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ products(first: 1) { edges { node { id } } } }\"}",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 400
}

pub fn graphql_mutation_returns_400_for_now_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"mutation { eventDelete(id: \\\"x\\\") { ok } }\"}",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 400
}

pub fn graphql_path_mismatched_version_still_routes_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/unstable/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ events(first: 1) { nodes { id } } }\"}",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
}

pub fn meta_config_default_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/config",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"runtime\":{\"readMode\":\"snapshot\"},\"proxy\":{\"port\":4000,\"shopifyAdminOrigin\":\"https://shopify.com\"},\"snapshot\":{\"enabled\":false,\"path\":null}}"
}

pub fn meta_config_with_snapshot_path_test() {
  let proxy =
    draft_proxy.with_config(Config(
      read_mode: LiveHybrid,
      port: 9000,
      shopify_admin_origin: "https://shop.test",
      snapshot_path: Some("/tmp/snap.json"),
    ))
  let request =
    Request(
      method: "GET",
      path: "/__meta/config",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"runtime\":{\"readMode\":\"live-hybrid\"},\"proxy\":{\"port\":9000,\"shopifyAdminOrigin\":\"https://shop.test\"},\"snapshot\":{\"enabled\":true,\"path\":\"/tmp/snap.json\"}}"
}

pub fn meta_config_post_returns_405_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/config",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 405
}

pub fn meta_log_returns_empty_entries_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/log",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"entries\":[]}"
}

pub fn meta_state_returns_empty_snapshot_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/state",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"baseState\":{},\"stagedState\":{}}"
}

pub fn meta_reset_returns_ok_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/reset",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body) == "{\"ok\":true,\"message\":\"state reset\"}"
}

pub fn meta_reset_get_returns_405_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/reset",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 405
}

pub fn graphql_delivery_settings_routed_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ deliverySettings { legacyModeProfiles } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"deliverySettings\":{\"legacyModeProfiles\":false}}}"
}

pub fn graphql_delivery_promise_settings_routed_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ deliveryPromiseSettings { deliveryDatesEnabled } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"deliveryPromiseSettings\":{\"deliveryDatesEnabled\":false}}}"
}

pub fn graphql_order_saved_searches_routed_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: empty_headers(),
      body: "{\"query\":\"{ orderSavedSearches(first: 1) { nodes { name } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Unfulfilled\"}]}}}"
}

pub fn default_config_round_trip_test() {
  let cfg = draft_proxy.default_config()
  assert cfg.read_mode == Snapshot
  assert cfg.port == 4000
  assert cfg.snapshot_path == None
  assert draft_proxy.config_summary(cfg) == "snapshot@4000"
}

fn graphql_request(body: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: body,
  )
}

fn meta_get(path: String) -> Request {
  Request(method: "GET", path: path, headers: empty_headers(), body: "")
}

const saved_search_create_body: String = "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Promo orders\\\", query: \\\"tag:promo\\\", resourceType: ORDER }) { savedSearch { __typename id legacyResourceId name query resourceType } userErrors { field message } } }\"}"

pub fn graphql_saved_search_create_returns_payload_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Promo orders\\\", query: \\\"tag:promo\\\", resourceType: ORDER }) { savedSearch { __typename id legacyResourceId name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"__typename\":\"SavedSearch\",\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"legacyResourceId\":\"1\",\"name\":\"Promo orders\",\"query\":\"tag:promo\",\"resourceType\":\"ORDER\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_create_missing_input_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Input is required\"}]}}}"
}

pub fn graphql_saved_search_create_blank_name_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"   \\\", query: \\\"tag:promo\\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name can't be blank\"}]}}}"
}

pub fn graphql_saved_search_create_unsupported_resource_type_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"X\\\", query: \\\"foo\\\", resourceType: URL_REDIRECT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"resourceType\"],\"message\":\"URL redirect saved searches require online-store navigation conformance before local support\"}]}}}"
}

pub fn graphql_saved_search_create_customer_deprecated_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"X\\\", query: \\\"foo\\\", resourceType: CUSTOMER }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":null,\"message\":\"Customer saved searches have been deprecated. Use Segmentation API instead.\"}]}}}"
}

pub fn meta_state_reflects_staged_saved_search_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  assert status == 200
  assert json.to_string(body)
    == "{\"baseState\":{},\"stagedState\":{\"savedSearches\":{\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"legacyResourceId\":\"1\",\"name\":\"Promo orders\",\"query\":\"tag:promo\",\"resourceType\":\"ORDER\",\"searchTerms\":\"tag:promo\",\"filters\":[],\"cursor\":null}}}}"
}

pub fn meta_log_reflects_staged_mutation_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  assert status == 200
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"id\":\"gid://shopify/MutationLogEntry/2\"")
  assert string.contains(serialized, "\"receivedAt\":\"2024-01-01T00:00:00.000Z\"")
  assert string.contains(serialized, "\"path\":\"/admin/api/2025-01/graphql.json\"")
  assert string.contains(serialized, "\"status\":\"staged\"")
  assert string.contains(
    serialized,
    "\"stagedResourceIds\":[\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\"]",
  )
  assert string.contains(serialized, "\"primaryRootField\":\"savedSearchCreate\"")
  assert string.contains(serialized, "\"domain\":\"saved-searches\"")
  assert string.contains(serialized, "\"execution\":\"stage-locally\"")
  assert string.contains(
    serialized,
    "\"notes\":\"Locally staged savedSearchCreate in shopify-draft-proxy.\"",
  )
}

pub fn meta_log_reflects_failed_mutation_test() {
  let proxy = draft_proxy.new()
  let create_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"status\":\"failed\"")
  assert string.contains(serialized, "\"stagedResourceIds\":[]")
}

pub fn meta_reset_clears_staged_state_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      Request(
        method: "POST",
        path: "/__meta/reset",
        headers: empty_headers(),
        body: "",
      ),
    )
  let #(Response(body: state_body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  let #(Response(body: log_body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  assert json.to_string(state_body) == "{\"baseState\":{},\"stagedState\":{}}"
  assert json.to_string(log_body) == "{\"entries\":[]}"
}

pub fn graphql_saved_search_create_visible_in_subsequent_query_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ orderSavedSearches(query: \\\"Promo\\\") { nodes { id name } } }\"}",
      ),
    )
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"orderSavedSearches\":{\"nodes\":[{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Promo orders\"}]}}}"
}

pub fn graphql_saved_search_update_renames_record_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(proxy, graphql_request(saved_search_create_body))
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", name: \\\"Renamed promo\\\" }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Renamed promo\",\"query\":\"tag:promo\",\"resourceType\":\"ORDER\"},\"userErrors\":[]}}}"
  // Subsequent state read shows the renamed record.
  let #(Response(body: state_body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  assert string.contains(json.to_string(state_body), "\"name\":\"Renamed promo\"")
}

pub fn graphql_saved_search_update_unknown_id_test() {
  let proxy = draft_proxy.new()
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/999?shopify-draft-proxy=synthetic\\\", name: \\\"X\\\" }) { savedSearch { id } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"id\"],\"message\":\"Saved Search does not exist\"}]}}}"
}

pub fn graphql_saved_search_update_blank_name_returns_existing_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(proxy, graphql_request(saved_search_create_body))
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", name: \\\"\\\" }) { savedSearch { id name query } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  // Validation surfaces the blank-name error; the response echoes the
  // existing record (no record_opt because sanitized input was rejected).
  let serialized = json.to_string(body)
  assert string.contains(
    serialized,
    "\"message\":\"Name can't be blank\"",
  )
  assert string.contains(serialized, "\"name\":\"Promo orders\"")
}

pub fn graphql_saved_search_delete_removes_record_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(proxy, graphql_request(saved_search_create_body))
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\" }) { deletedSavedSearchId userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchDelete\":{\"deletedSavedSearchId\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"userErrors\":[]}}}"
  // After delete, a follow-up query no longer surfaces the record.
  let #(Response(body: list_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ orderSavedSearches(query: \\\"Promo\\\") { nodes { id } } }\"}",
      ),
    )
  assert json.to_string(list_body)
    == "{\"data\":{\"orderSavedSearches\":{\"nodes\":[]}}}"
}

pub fn graphql_saved_search_delete_unknown_id_test() {
  let proxy = draft_proxy.new()
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: { id: \\\"gid://shopify/SavedSearch/777\\\" }) { deletedSavedSearchId userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchDelete\":{\"deletedSavedSearchId\":null,\"userErrors\":[{\"field\":[\"input\",\"id\"],\"message\":\"Saved Search does not exist\"}]}}}"
}

pub fn graphql_saved_search_delete_default_record_test() {
  // Deleting a static default record should fail — the record has no
  // staged or base-state row, so getEffective returns None.
  let proxy = draft_proxy.new()
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: { id: \\\"gid://shopify/SavedSearch/3634391515442\\\" }) { deletedSavedSearchId userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchDelete\":{\"deletedSavedSearchId\":null,\"userErrors\":[{\"field\":[\"input\",\"id\"],\"message\":\"Saved Search does not exist\"}]}}}"
}

pub fn graphql_saved_search_create_with_variables_test() {
  // Variables threaded through dispatcher → arg resolution should
  // substitute into `$input` and produce the same record as the
  // inline-args variant.
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation Create($input: SavedSearchCreateInput!) { savedSearchCreate(input: $input) { savedSearch { id name query resourceType } userErrors { field message } } }\",\"variables\":{\"input\":{\"name\":\"Promo orders\",\"query\":\"tag:promo\",\"resourceType\":\"ORDER\"}}}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Promo orders\",\"query\":\"tag:promo\",\"resourceType\":\"ORDER\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_query_with_variables_test() {
  // Pagination variables ($first, $reverse) threaded through the
  // dispatcher produce the same response as inline arguments would.
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"query Q($first: Int!, $reverse: Boolean) { orderSavedSearches(first: $first, reverse: $reverse) { nodes { id name } } }\",\"variables\":{\"first\":1,\"reverse\":true}}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  // With reverse=true and first=1 we expect the *last* default record
  // (Open) instead of the first (Unfulfilled).
  assert string.contains(json.to_string(response_body), "\"name\":\"Open\"")
}

pub fn graphql_omitted_variables_object_still_parses_test() {
  // Body without a `variables` key should still succeed (defaults to
  // empty dict) — covers the optional_field path.
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"{ orderSavedSearches(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
}


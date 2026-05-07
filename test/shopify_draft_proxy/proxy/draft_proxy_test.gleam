import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy.{type Request}
import shopify_draft_proxy/proxy/operation_registry
import shopify_draft_proxy/proxy/proxy_state.{
  Config, LiveHybrid, PassthroughUnsupportedMutations, Request, Response,
  Snapshot,
}
import shopify_draft_proxy/state/serialization as state_serialization

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
  assert json.to_string(body)
    == "{\"ok\":true,\"message\":\"shopify-draft-proxy is running\"}"
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

pub fn graphql_events_read_count_query_shapes_match_capture_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"query EventEmptyRead($eventId: ID!, $first: Int!, $query: String!) { event(id: $eventId) { id action message } events(first: $first, query: $query, sortKey: ID, reverse: true) { nodes { id action message } edges { cursor node { id action message } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } eventsCount(query: $query) { count precision } }\",\"variables\":{\"eventId\":\"gid://shopify/BasicEvent/999999999999\",\"first\":2,\"query\":\"id:999999999999\"}}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"event\":null,\"events\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"eventsCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"
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

pub fn graphql_products_empty_query_returns_envelope_test() {
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
  assert status == 200
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
    == "{\"runtime\":{\"readMode\":\"snapshot\",\"unsupportedMutationMode\":\"passthrough\"},\"proxy\":{\"port\":4000,\"shopifyAdminOrigin\":\"https://shopify.com\"},\"snapshot\":{\"enabled\":false,\"path\":null}}"
}

pub fn meta_config_with_snapshot_path_test() {
  let proxy =
    draft_proxy.with_config(Config(
      read_mode: LiveHybrid,
      unsupported_mutation_mode: PassthroughUnsupportedMutations,
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
    == "{\"runtime\":{\"readMode\":\"live-hybrid\",\"unsupportedMutationMode\":\"passthrough\"},\"proxy\":{\"port\":9000,\"shopifyAdminOrigin\":\"https://shop.test\"},\"snapshot\":{\"enabled\":true,\"path\":\"/tmp/snap.json\"}}"
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
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"baseState\"")
  assert string.contains(serialized, "\"stagedState\"")
  assert string.contains(serialized, "\"savedSearches\":{}")
  assert string.contains(serialized, "\"webhookSubscriptions\":{}")
  assert string.contains(serialized, "\"marketingActivities\":{}")
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
  assert cfg.unsupported_mutation_mode == PassthroughUnsupportedMutations
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

fn graphql_request_for_version(version: String, body: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/" <> version <> "/graphql.json",
    headers: empty_headers(),
    body: body,
  )
}

/// Synthetic registry that the dispatcher tests use to exercise the
/// capability-driven path without depending on the production registry.
fn capability_test_registry() -> List(operation_registry.RegistryEntry) {
  [
    operation_registry.RegistryEntry(
      name: "events",
      type_: operation_registry.Query,
      domain: operation_registry.Events,
      execution: operation_registry.OverlayRead,
      implemented: True,
      match_names: ["events", "eventsCount"],
      runtime_tests: [],
      support_notes: None,
    ),
    operation_registry.RegistryEntry(
      name: "orderSavedSearches",
      type_: operation_registry.Query,
      domain: operation_registry.SavedSearches,
      execution: operation_registry.OverlayRead,
      implemented: True,
      match_names: ["orderSavedSearches"],
      runtime_tests: [],
      support_notes: None,
    ),
    operation_registry.RegistryEntry(
      name: "savedSearchCreate",
      type_: operation_registry.Mutation,
      domain: operation_registry.SavedSearches,
      execution: operation_registry.StageLocally,
      implemented: True,
      match_names: ["savedSearchCreate"],
      runtime_tests: [],
      support_notes: None,
    ),
  ]
}

pub fn registry_drives_query_dispatch_test() {
  let proxy =
    draft_proxy.new()
    |> draft_proxy.with_registry(capability_test_registry())
  let body = "{\"query\":\"{ events { nodes { id } } }\"}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  // Events is read-only and always empty in the proxy; the dispatcher
  // arrives at events.process via the capability path.
  assert string.contains(json.to_string(response_body), "\"events\":")
}

pub fn registry_drives_mutation_dispatch_test() {
  let proxy =
    draft_proxy.new()
    |> draft_proxy.with_registry(capability_test_registry())
  let body =
    "{\"query\":\"mutation { savedSearchCreate(input: { resourceType: ORDER, name: \\\"X\\\", query: \\\"tag:x\\\" }) { savedSearch { id } userErrors { message } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
}

pub fn registry_unknown_root_falls_back_to_400_test() {
  // A registry without an entry for productSavedSearches still routes
  // because the legacy fallback predicate recognises it. With *only*
  // capability-driven dispatch active, an unknown root would 400.
  let proxy =
    draft_proxy.new()
    |> draft_proxy.with_registry(capability_test_registry())
  let body =
    "{\"query\":\"{ productSavedSearches(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  // productSavedSearches is recognised by the legacy
  // is_saved_search_query_root fallback, so this still succeeds.
  // This test exists to lock in the fallback behavior so a future
  // pass that flips to capability-only dispatch can update it
  // intentionally.
  assert status == 200
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
    == "{\"errors\":[{\"message\":\"Field 'savedSearchCreate' is missing required arguments: input\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"savedSearchCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"savedSearchCreate\",\"arguments\":\"input\"}}]}"
}

pub fn graphql_validation_update_rejects_unknown_function_input_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation ValidationUpdateRebind { validationUpdate(id: \\\"gid://shopify/Validation/1\\\", validation: { functionId: \\\"gid://shopify/ShopifyFunction/other\\\" }) { validation { id functionId } userErrors { field message code } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'functionId' is not defined on ValidationUpdateInput\",\"locations\":[{\"line\":1,\"column\":100}],\"path\":[\"mutation ValidationUpdateRebind\",\"validationUpdate\",\"validation\",\"functionId\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"InputObject\",\"argumentName\":\"functionId\"}}]}"
}

pub fn graphql_validation_update_rejects_unknown_function_handle_input_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation ValidationUpdateRebind { validationUpdate(id: \\\"gid://shopify/Validation/1\\\", validation: { functionHandle: \\\"other\\\" }) { validation { id functionHandle } userErrors { field message code } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'functionHandle' is not defined on ValidationUpdateInput\",\"locations\":[{\"line\":1,\"column\":100}],\"path\":[\"mutation ValidationUpdateRebind\",\"validationUpdate\",\"validation\",\"functionHandle\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"InputObject\",\"argumentName\":\"functionHandle\"}}]}"
}

pub fn graphql_validation_update_rejects_enabled_alias_input_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation ValidationUpdateEnabledAlias { validationUpdate(id: \\\"gid://shopify/Validation/1\\\", validation: { enabled: true }) { validation { id enable } userErrors { field message code } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  let body_json = json.to_string(body)
  assert status == 200
  assert string.contains(
    body_json,
    "\"message\":\"Field 'enabled' is not defined on ValidationUpdateInput\"",
  )
  assert string.contains(
    body_json,
    "\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"InputObject\",\"argumentName\":\"enabled\"}",
  )
}

pub fn graphql_order_create_mandate_payment_missing_mandate_id_errors_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { orderCreateMandatePayment(id: \\\"gid://shopify/Order/1\\\", idempotencyKey: \\\"abc123\\\") { paymentReferenceId userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'orderCreateMandatePayment' is missing required arguments: mandateId\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"orderCreateMandatePayment\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"orderCreateMandatePayment\",\"arguments\":\"mandateId\"}}]}"
}

pub fn graphql_order_edit_add_custom_item_missing_required_title_errors_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { orderEditAddCustomItem(id: \\\"gid://shopify/CalculatedOrder/1\\\", quantity: 1, price: { amount: \\\"1.00\\\", currencyCode: CAD }) { calculatedLineItem { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'orderEditAddCustomItem' is missing required arguments: title\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"orderEditAddCustomItem\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"orderEditAddCustomItem\",\"arguments\":\"title\"}}]}"
}

pub fn graphql_order_edit_add_custom_item_inline_price_requires_currency_code_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { orderEditAddCustomItem(id: \\\"gid://shopify/CalculatedOrder/1\\\", title: \\\"Missing currency\\\", quantity: 1, price: { amount: \\\"1.00\\\" }) { calculatedLineItem { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Argument 'currencyCode' on InputObject 'MoneyInput' is required. Expected type CurrencyCode!\",\"locations\":[{\"line\":1,\"column\":121}],\"path\":[\"mutation\",\"orderEditAddCustomItem\",\"price\",\"currencyCode\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"currencyCode\",\"argumentType\":\"CurrencyCode!\",\"inputObjectType\":\"MoneyInput\"}}]}"
}

pub fn graphql_marketing_engagement_create_inline_input_requires_captured_fields_test() {
  let proxy = draft_proxy.new()
  let sparse_request =
    graphql_request_for_version(
      "2026-04",
      "{\"query\":\"mutation { marketingEngagementCreate(remoteId: \\\"remote-1\\\", marketingEngagement: { occurredOn: \\\"2026-04-02\\\", impressionsCount: 7 }) { marketingEngagement { occurredOn utcOffset isCumulative } userErrors { field message code } } }\"}",
    )
  let #(Response(status: sparse_status, body: sparse_body, ..), _) =
    draft_proxy.process_request(proxy, sparse_request)
  let sparse_json = json.to_string(sparse_body)
  assert sparse_status == 200
  assert string.contains(
    sparse_json,
    "\"code\":\"missingRequiredInputObjectAttribute\"",
  )
  assert string.contains(sparse_json, "\"argumentName\":\"isCumulative\"")
  assert string.contains(sparse_json, "\"argumentType\":\"Boolean!\"")
  assert string.contains(sparse_json, "\"argumentName\":\"utcOffset\"")
  assert string.contains(sparse_json, "\"argumentType\":\"UtcOffset!\"")
  assert !string.contains(sparse_json, "\"data\"")

  let missing_occurred_on_request =
    graphql_request_for_version(
      "2026-04",
      "{\"query\":\"mutation { marketingEngagementCreate(remoteId: \\\"remote-1\\\", marketingEngagement: { utcOffset: \\\"+00:00\\\", isCumulative: false, impressionsCount: 1 }) { marketingEngagement { occurredOn utcOffset isCumulative impressionsCount } userErrors { field message code } } }\"}",
    )
  let #(Response(status: missing_status, body: missing_body, ..), _) =
    draft_proxy.process_request(proxy, missing_occurred_on_request)
  let missing_json = json.to_string(missing_body)
  assert missing_status == 200
  assert string.contains(
    missing_json,
    "\"code\":\"missingRequiredInputObjectAttribute\"",
  )
  assert string.contains(missing_json, "\"argumentName\":\"occurredOn\"")
  assert string.contains(missing_json, "\"argumentType\":\"Date!\"")
  assert !string.contains(missing_json, "\"data\"")
}

pub fn graphql_saved_search_create_missing_required_input_fields_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { resourceType: PRODUCT }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Argument 'name' on InputObject 'SavedSearchCreateInput' is required. Expected type String!\",\"locations\":[{\"line\":1,\"column\":37}],\"path\":[\"mutation\",\"savedSearchCreate\",\"input\",\"name\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"name\",\"argumentType\":\"String!\",\"inputObjectType\":\"SavedSearchCreateInput\"}},{\"message\":\"Argument 'query' on InputObject 'SavedSearchCreateInput' is required. Expected type String!\",\"locations\":[{\"line\":1,\"column\":37}],\"path\":[\"mutation\",\"savedSearchCreate\",\"input\",\"query\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"query\",\"argumentType\":\"String!\",\"inputObjectType\":\"SavedSearchCreateInput\"}}]}"
}

pub fn graphql_saved_search_create_missing_resource_type_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"X\\\", query: \\\"tag:promo\\\" }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Argument 'resourceType' on InputObject 'SavedSearchCreateInput' is required. Expected type SearchResultType!\",\"locations\":[{\"line\":1,\"column\":37}],\"path\":[\"mutation\",\"savedSearchCreate\",\"input\",\"resourceType\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"resourceType\",\"argumentType\":\"SearchResultType!\",\"inputObjectType\":\"SavedSearchCreateInput\"}}]}"
}

pub fn graphql_saved_search_create_empty_query_allowed_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Empty query\\\", query: \\\"\\\", resourceType: PRODUCT }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Empty query\",\"query\":\"\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[]}}}"
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

pub fn graphql_saved_search_create_too_long_name_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"12345678901234567890123456789012345678901\\\", query: \\\"tag:promo\\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name is too long (maximum is 40 characters)\"}]}}}"
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

pub fn graphql_saved_search_create_rejects_order_reserved_filter_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Reserved order\\\", query: \\\"reference_location_id:1\\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Search terms is invalid, 'reference_location_id' is a reserved filter name\"}]}}}"
}

pub fn graphql_saved_search_create_rejects_product_incompatible_filters_test() {
  let proxy = draft_proxy.new()
  let tag_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Collection tag\\\", query: \\\"collection_id:\\\\\\\"123\\\\\\\" tag:\\\\\\\"AAA\\\\\\\"\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: tag_status, body: tag_body, ..), _) =
    draft_proxy.process_request(proxy, tag_request)
  assert tag_status == 200
  assert json.to_string(tag_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query has incompatible filters: collection_id, tag\"}]}}}"

  let published_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Collection published\\\", query: \\\"collection_id:\\\\\\\"123\\\\\\\" published_status:published\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: published_status, body: published_body, ..), _) =
    draft_proxy.process_request(proxy, published_request)
  assert published_status == 200
  assert json.to_string(published_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query has incompatible filters: collection_id, published_status\"}]}}}"

  let error_feedback_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Collection error\\\", query: \\\"collection_id:\\\\\\\"123\\\\\\\" error_feedback:\\\\\\\"x\\\\\\\"\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: error_status, body: error_body, ..), _) =
    draft_proxy.process_request(proxy, error_feedback_request)
  assert error_status == 200
  assert json.to_string(error_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query has incompatible filters: collection_id, error_feedback\"}]}}}"
}

pub fn graphql_saved_search_create_allows_product_collection_id_alone_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Collection only\\\", query: \\\"collection_id:\\\\\\\"12345\\\\\\\"\\\", resourceType: PRODUCT }) { savedSearch { query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"query\":\"collection_id:\\\"12345\\\"\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_create_rejects_unknown_product_filter_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Unknown product filter\\\", query: \\\"made_up_filter:foo\\\", resourceType: PRODUCT }) { savedSearch { id query filters { key value } } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query is invalid, 'made_up_filter' is not a valid filter\"}]}}}"

  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Unknown product filter\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[]}}}"
}

pub fn graphql_saved_search_create_sorts_multiple_unknown_filters_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Unknown product filters\\\", query: \\\"z_filter:1 made_up_filter:2 a_filter:3 made_up_filter:4\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query is invalid, 'a_filter' is not a valid filter\"},{\"field\":[\"input\",\"query\"],\"message\":\"Query is invalid, 'made_up_filter' is not a valid filter\"},{\"field\":[\"input\",\"query\"],\"message\":\"Query is invalid, 'z_filter' is not a valid filter\"}]}}}"
}

pub fn graphql_saved_search_create_allows_resource_without_allowlist_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Price rule unknown\\\", query: \\\"made_up_filter:foo\\\", resourceType: PRICE_RULE }) { savedSearch { query resourceType filters { key value } } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"query\":\"made_up_filter:foo\",\"resourceType\":\"PRICE_RULE\",\"filters\":[{\"key\":\"made_up_filter\",\"value\":\"foo\"}]},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_create_allows_unbalanced_grouping_query_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Grouping literal\\\", query: \\\"(tag:AAA\\\", resourceType: PRODUCT }) { savedSearch { query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"query\":\"(tag:AAA\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_update_rejects_query_grammar_without_staging_test() {
  let proxy = draft_proxy.new()
  let create_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Update grammar\\\", query: \\\"collection_id:\\\\\\\"12345\\\\\\\"\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let update_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", query: \\\"collection_id:\\\\\\\"123\\\\\\\" tag:\\\\\\\"AAA\\\\\\\"\\\" }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, update_request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Update grammar\",\"query\":\"collection_id:\\\"123\\\" tag:\\\"AAA\\\"\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[{\"field\":[\"input\",\"query\"],\"message\":\"Query has incompatible filters: collection_id, tag\"}]}}}"

  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Update grammar\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[{\"name\":\"Update grammar\",\"query\":\"collection_id:12345\"}]}}}"
}

pub fn graphql_saved_search_update_rejects_order_reserved_filter_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let update_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", query: \\\"reference_location_id:1\\\" }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, update_request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Promo orders\",\"query\":\"reference_location_id:1\",\"resourceType\":\"ORDER\"},\"userErrors\":[{\"field\":[\"input\",\"searchTerms\"],\"message\":\"Search terms is invalid, 'reference_location_id' is a reserved filter name\"}]}}}"

  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ orderSavedSearches(query: \\\"Promo\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"orderSavedSearches\":{\"nodes\":[{\"name\":\"Promo orders\",\"query\":\"tag:promo\"}]}}}"
}

pub fn graphql_saved_search_update_rejects_unknown_filter_without_staging_test() {
  let proxy = draft_proxy.new()
  let create_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Update unknown\\\", query: \\\"vendor:Acme\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let update_request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", query: \\\"made_up_filter:foo\\\" }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, update_request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Update unknown\",\"query\":\"made_up_filter:foo\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[{\"field\":[\"input\",\"searchTerms\"],\"message\":\"Query is invalid, 'made_up_filter' is not a valid filter\"}]}}}"

  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Update unknown\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[{\"name\":\"Update unknown\",\"query\":\"vendor:Acme\"}]}}}"
}

pub fn graphql_saved_search_create_duplicate_staged_name_test() {
  let proxy = draft_proxy.new()
  let create_a =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Conflict A\\\", query: \\\"tag:a\\\", resourceType: PRODUCT }) { savedSearch { id name } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_a)
  let duplicate =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Conflict A\\\", query: \\\"tag:b\\\", resourceType: PRODUCT }) { savedSearch { id name } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, duplicate)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Conflict A\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[{\"name\":\"Conflict A\",\"query\":\"tag:a\"}]}}}"
}

pub fn graphql_saved_search_create_duplicate_name_is_case_sensitive_test() {
  let proxy = draft_proxy.new()
  let create_a =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Conflict A\\\", query: \\\"tag:a\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_a)
  let create_lowercase =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"conflict a\\\", query: \\\"tag:b\\\", resourceType: PRODUCT }) { savedSearch { name query resourceType } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, create_lowercase)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"name\":\"conflict a\",\"query\":\"tag:b\",\"resourceType\":\"PRODUCT\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_create_duplicate_static_default_name_test() {
  let proxy = draft_proxy.new()
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Unfulfilled\\\", query: \\\"tag:new\\\", resourceType: ORDER }) { savedSearch { id name } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
}

pub fn graphql_saved_search_create_duplicate_base_state_name_test() {
  let snapshot =
    "{\"kind\":\"normalized-state-snapshot\",\"baseState\":{\"products\":{},\"savedSearches\":{\"gid://shopify/SavedSearch/900\":{\"id\":\"gid://shopify/SavedSearch/900\",\"legacyResourceId\":\"900\",\"name\":\"Base Product Search\",\"query\":\"tag:base\",\"resourceType\":\"PRODUCT\",\"searchTerms\":\"\",\"filters\":[],\"cursor\":null}},\"savedSearchOrder\":[\"gid://shopify/SavedSearch/900\"]}}"
  let assert Ok(proxy) =
    draft_proxy.restore_snapshot(draft_proxy.new(), snapshot)
  let request =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Base Product Search\\\", query: \\\"tag:new\\\", resourceType: PRODUCT }) { savedSearch { id name } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
}

pub fn graphql_saved_search_create_rejects_reserved_resource_names_test() {
  assert_reserved_create_name("PRODUCT", "All products")
  assert_reserved_create_name("PRODUCT", "ALL PRODUCTS")
  assert_reserved_create_name("COLLECTION", "All collections")
  assert_reserved_create_name("ORDER", "All")
  assert_reserved_create_name("DRAFT_ORDER", "All Drafts")
  assert_reserved_create_name("FILE", "All Files")
}

pub fn graphql_saved_search_create_allows_reserved_name_for_unscoped_resource_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"All products\\\", query: \\\"code:SUMMER\\\", resourceType: DISCOUNT_REDEEM_CODE }) { savedSearch { name query resourceType } userErrors { field message } } }\"}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"name\":\"All products\",\"query\":\"code:SUMMER\",\"resourceType\":\"DISCOUNT_REDEEM_CODE\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_update_rejects_reserved_resource_name_test() {
  let proxy = draft_proxy.new()
  let create =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Reserved update source\\\", query: \\\"vendor:Acme\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create)
  let update =
    graphql_request(
      "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", name: \\\"All products\\\", query: \\\"vendor:Changed\\\" }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, update)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Reserved update source\",\"query\":\"vendor:Changed\",\"resourceType\":\"PRODUCT\",\"filters\":[{\"key\":\"vendor\",\"value\":\"Changed\"}]},\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Reserved update source\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[{\"name\":\"Reserved update source\",\"query\":\"vendor:Acme\"}]}}}"
}

fn assert_reserved_create_name(resource_type: String, name: String) {
  let proxy = draft_proxy.new()
  let query = case resource_type {
    "PRODUCT" -> "vendor:Acme"
    "COLLECTION" -> "title:Sale"
    "ORDER" -> "status:open"
    "DRAFT_ORDER" -> "status:open"
    "FILE" -> ""
    _ -> ""
  }
  let body =
    "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\""
    <> name
    <> "\\\", query: \\\""
    <> query
    <> "\\\", resourceType: "
    <> resource_type
    <> " }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}"
  let #(Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  let #(Response(body: state_body, ..), proxy) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  assert string.contains(json.to_string(state_body), "\"savedSearches\":{}")
  let #(Response(body: log_body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  let serialized_log = json.to_string(log_body)
  assert string.contains(
    serialized_log,
    "\"primaryRootField\":\"savedSearchCreate\"",
  )
  assert string.contains(serialized_log, "\"status\":\"failed\"")
  assert string.contains(serialized_log, "\"stagedResourceIds\":[]")
}

pub fn meta_state_reflects_staged_saved_search_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  assert status == 200
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"savedSearches\":{")
  assert string.contains(
    serialized,
    "\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\"",
  )
  assert string.contains(serialized, "\"savedSearchOrder\":[")
  assert string.contains(serialized, "\"name\":\"Promo orders\"")
}

pub fn meta_log_reflects_staged_mutation_test() {
  let proxy = draft_proxy.new()
  let create_request = graphql_request(saved_search_create_body)
  let #(_, proxy) = draft_proxy.process_request(proxy, create_request)
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  assert status == 200
  let serialized = json.to_string(body)
  assert string.contains(
    serialized,
    "\"id\":\"gid://shopify/MutationLogEntry/2\"",
  )
  assert string.contains(
    serialized,
    "\"receivedAt\":\"2024-01-01T00:00:00.000Z\"",
  )
  assert string.contains(
    serialized,
    "\"path\":\"/admin/api/2025-01/graphql.json\"",
  )
  assert string.contains(serialized, "\"status\":\"staged\"")
  assert string.contains(
    serialized,
    "\"stagedResourceIds\":[\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\"]",
  )
  assert string.contains(
    serialized,
    "\"primaryRootField\":\"savedSearchCreate\"",
  )
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
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"   \\\", query: \\\"tag:promo\\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }\"}",
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
  let serialized_state = json.to_string(state_body)
  assert string.contains(serialized_state, "\"savedSearches\":{}")
  assert string.contains(serialized_state, "\"webhookSubscriptions\":{}")
  assert string.contains(serialized_state, "\"marketingActivities\":{}")
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
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
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
  assert string.contains(
    json.to_string(state_body),
    "\"name\":\"Renamed promo\"",
  )
}

pub fn graphql_saved_search_update_duplicate_name_leaves_record_unchanged_test() {
  let proxy = draft_proxy.new()
  let create_a =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Conflict A\\\", query: \\\"tag:a\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_a)
  let create_b =
    graphql_request(
      "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Conflict B\\\", query: \\\"tag:b\\\", resourceType: PRODUCT }) { savedSearch { id } userErrors { field message } } }\"}",
    )
  let #(_, proxy) = draft_proxy.process_request(proxy, create_b)
  let update_b =
    graphql_request(
      "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/3?shopify-draft-proxy=synthetic\\\", name: \\\"Conflict A\\\", query: \\\"tag:changed\\\" }) { savedSearch { id name query } userErrors { field message } } }\"}",
    )
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, update_b)
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/3?shopify-draft-proxy=synthetic\",\"name\":\"Conflict B\",\"query\":\"tag:changed\"},\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  let #(Response(body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "{\"query\":\"{ productSavedSearches(query: \\\"Conflict\\\") { nodes { name query } } }\"}",
      ),
    )
  assert json.to_string(read_body)
    == "{\"data\":{\"productSavedSearches\":{\"nodes\":[{\"name\":\"Conflict A\",\"query\":\"tag:a\"},{\"name\":\"Conflict B\",\"query\":\"tag:b\"}]}}}"
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

pub fn graphql_saved_search_update_missing_id_test() {
  let proxy = draft_proxy.new()
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { name: \\\"X\\\" }) { savedSearch { id } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Argument 'id' on InputObject 'SavedSearchUpdateInput' is required. Expected type ID!\",\"locations\":[{\"line\":1,\"column\":37}],\"path\":[\"mutation\",\"savedSearchUpdate\",\"input\",\"id\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"id\",\"argumentType\":\"ID!\",\"inputObjectType\":\"SavedSearchUpdateInput\"}}]}"
}

pub fn graphql_saved_search_update_empty_query_allowed_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", query: \\\"\\\" }) { savedSearch { id name query resourceType } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchUpdate\":{\"savedSearch\":{\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"name\":\"Promo orders\",\"query\":\"\",\"resourceType\":\"ORDER\"},\"userErrors\":[]}}}"
}

pub fn graphql_saved_search_update_blank_name_returns_existing_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let update_body =
    "{\"query\":\"mutation { savedSearchUpdate(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\", name: \\\"\\\" }) { savedSearch { id name query } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_body))
  assert status == 200
  // Validation surfaces the blank-name error; the response echoes the
  // existing record (no record_opt because sanitized input was rejected).
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"message\":\"Name can't be blank\"")
  assert string.contains(serialized, "\"name\":\"Promo orders\"")
}

pub fn graphql_saved_search_delete_removes_record_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
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

pub fn graphql_saved_search_delete_success_includes_shop_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: { id: \\\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\\\" }) { deletedSavedSearchId shop { id name myshopifyDomain } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchDelete\":{\"deletedSavedSearchId\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\",\"shop\":{\"id\":\"gid://shopify/Shop/1?shopify-draft-proxy=synthetic\",\"name\":\"Shopify Draft Proxy\",\"myshopifyDomain\":\"shopify-draft-proxy.myshopify.com\"},\"userErrors\":[]}}}"
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

pub fn graphql_saved_search_delete_missing_id_test() {
  let proxy = draft_proxy.new()
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: {}) { deletedSavedSearchId userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Argument 'id' on InputObject 'SavedSearchDeleteInput' is required. Expected type ID!\",\"locations\":[{\"line\":1,\"column\":37}],\"path\":[\"mutation\",\"savedSearchDelete\",\"input\",\"id\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"id\",\"argumentType\":\"ID!\",\"inputObjectType\":\"SavedSearchDeleteInput\"}}]}"
}

pub fn graphql_saved_search_delete_unknown_id_includes_shop_test() {
  let proxy = draft_proxy.new()
  let delete_body =
    "{\"query\":\"mutation { savedSearchDelete(input: { id: \\\"gid://shopify/SavedSearch/777\\\" }) { deletedSavedSearchId shop { id name myshopifyDomain } userErrors { field message } } }\"}"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(delete_body))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"savedSearchDelete\":{\"deletedSavedSearchId\":null,\"shop\":{\"id\":\"gid://shopify/Shop/1?shopify-draft-proxy=synthetic\",\"name\":\"Shopify Draft Proxy\",\"myshopifyDomain\":\"shopify-draft-proxy.myshopify.com\"},\"userErrors\":[{\"field\":[\"input\",\"id\"],\"message\":\"Saved Search does not exist\"}]}}}"
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
  let body = "{\"query\":\"{ orderSavedSearches(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
}

// ---------------------------------------------------------------------------
// Webhook mutations end-to-end through the dispatcher
// ---------------------------------------------------------------------------

pub fn graphql_webhook_subscription_create_returns_payload_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \\\"https://hooks.example.com/orders\\\", format: JSON }) { webhookSubscription { id topic uri format } userErrors { field message } } }\"}"
  let #(Response(status: status, body: response_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  // Body confirms the dispatcher routed to webhooks.process_mutation,
  // synthetic identity minted a new gid, and the payload was projected.
  assert json.to_string(response_body)
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"topic\":\"ORDERS_CREATE\",\"uri\":\"https://hooks.example.com/orders\",\"format\":\"JSON\"},\"userErrors\":[]}}}"
  let #(Response(body: state_body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  let serialized_state = json.to_string(state_body)
  assert string.contains(serialized_state, "\"webhookSubscriptions\":{")
  assert string.contains(
    serialized_state,
    "\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\"",
  )
  assert string.contains(serialized_state, "\"topic\":\"ORDERS_CREATE\"")
}

pub fn graphql_webhook_subscription_create_missing_topic_top_level_error_test() {
  // Top-level error envelope: no `data` key, just `errors`.
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation { webhookSubscriptionCreate(webhookSubscription: { uri: \\\"https://hooks.example.com/orders\\\" }) { webhookSubscription { id } userErrors { field message } } }\"}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"errors\":[{\"message\":\"Field 'webhookSubscriptionCreate' is missing required arguments: topic\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"webhookSubscriptionCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"webhookSubscriptionCreate\",\"arguments\":\"topic\"}}]}"
}

pub fn graphql_webhook_subscription_create_blank_uri_user_error_test() {
  // User-error envelope: payload nulls out webhookSubscription and lists
  // a structured user error under the standard `data` envelope.
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \\\"\\\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }\"}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address can't be blank\"}]}}}"
}

pub fn graphql_carrier_service_create_missing_callback_url_invalid_variable_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation CreateCarrier($input: DeliveryCarrierServiceCreateInput!) { carrierServiceCreate(input: $input) { carrierService { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Hermes Missing Callback\",\"supportsServiceDiscovery\":false,\"active\":false}}}"
  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  let response_json = json.to_string(response_body)
  assert string.contains(
    response_json,
    "Variable $input of type DeliveryCarrierServiceCreateInput! was provided invalid value for callbackUrl (Expected value to not be null)",
  )
  assert string.contains(response_json, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    response_json,
    "\"problems\":[{\"path\":[\"callbackUrl\"],\"explanation\":\"Expected value to not be null\"}]",
  )

  let read_body =
    "{\"query\":\"query { carrierServices(first: 5) { nodes { id name callbackUrl } } }\"}"
  let #(Response(status: read_status, body: read_response, ..), _) =
    draft_proxy.process_request(next_proxy, graphql_request(read_body))
  assert read_status == 200
  assert json.to_string(read_response)
    == "{\"data\":{\"carrierServices\":{\"nodes\":[]}}}"
}

pub fn graphql_carrier_service_create_unparseable_callback_url_invalid_variable_test() {
  let proxy = draft_proxy.new()
  let body =
    "{\"query\":\"mutation CreateCarrier($input: DeliveryCarrierServiceCreateInput!) { carrierServiceCreate(input: $input) { carrierService { id } userErrors { field message code } } }\",\"variables\":{\"input\":{\"name\":\"Hermes Bad Callback\",\"callbackUrl\":\"not-a-url\",\"supportsServiceDiscovery\":false,\"active\":false}}}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(body))
  assert status == 200
  let response_json = json.to_string(response_body)
  assert string.contains(
    response_json,
    "Variable $input of type DeliveryCarrierServiceCreateInput! was provided invalid value for callbackUrl (Invalid url 'not-a-url', missing scheme)",
  )
  assert string.contains(response_json, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    response_json,
    "\"problems\":[{\"path\":[\"callbackUrl\"],\"explanation\":\"Invalid url 'not-a-url', missing scheme\",\"message\":\"Invalid url 'not-a-url', missing scheme\"}]",
  )
}

// ---------------------------------------------------------------------------
// Standalone DraftProxy methods
// ---------------------------------------------------------------------------

pub fn get_config_snapshot_default_test() {
  let proxy = draft_proxy.new()
  assert json.to_string(draft_proxy.get_config_snapshot(proxy))
    == "{\"runtime\":{\"readMode\":\"snapshot\",\"unsupportedMutationMode\":\"passthrough\"},\"proxy\":{\"port\":4000,\"shopifyAdminOrigin\":\"https://shopify.com\"},\"snapshot\":{\"enabled\":false,\"path\":null}}"
}

pub fn get_config_snapshot_with_snapshot_path_test() {
  let cfg =
    Config(
      read_mode: LiveHybrid,
      unsupported_mutation_mode: PassthroughUnsupportedMutations,
      port: 4001,
      shopify_admin_origin: "https://example.myshopify.com",
      snapshot_path: Some("/tmp/snap.json"),
    )
  let proxy = draft_proxy.with_config(cfg)
  assert json.to_string(draft_proxy.get_config_snapshot(proxy))
    == "{\"runtime\":{\"readMode\":\"live-hybrid\",\"unsupportedMutationMode\":\"passthrough\"},\"proxy\":{\"port\":4001,\"shopifyAdminOrigin\":\"https://example.myshopify.com\"},\"snapshot\":{\"enabled\":true,\"path\":\"/tmp/snap.json\"}}"
}

pub fn get_config_snapshot_matches_meta_route_body_test() {
  // Drives invariant: the standalone getter and the route handler
  // produce byte-identical bodies.
  let proxy = draft_proxy.new()
  let standalone = json.to_string(draft_proxy.get_config_snapshot(proxy))
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/config"))
  assert standalone == json.to_string(body)
}

pub fn get_log_snapshot_empty_test() {
  let proxy = draft_proxy.new()
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
}

pub fn get_log_snapshot_matches_meta_route_body_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let standalone = json.to_string(draft_proxy.get_log_snapshot(proxy))
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/log"))
  assert standalone == json.to_string(body)
}

pub fn get_state_snapshot_matches_meta_route_body_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let standalone = json.to_string(draft_proxy.get_state_snapshot(proxy))
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/state"))
  assert standalone == json.to_string(body)
}

pub fn reset_method_clears_state_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  // Sanity: the create staged something.
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    != "{\"entries\":[]}"
  let proxy = draft_proxy.reset(proxy)
  assert json.to_string(draft_proxy.get_log_snapshot(proxy))
    == "{\"entries\":[]}"
  let serialized_state = json.to_string(draft_proxy.get_state_snapshot(proxy))
  assert string.contains(serialized_state, "\"savedSearches\":{}")
  assert string.contains(serialized_state, "\"webhookSubscriptions\":{}")
  assert string.contains(serialized_state, "\"marketingActivities\":{}")
}

pub fn reset_method_resets_synthetic_identity_counter_test() {
  // After reset, a fresh saved_searchCreate mints the same gid as it
  // would on a new proxy — confirms the registry was rewound.
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let proxy = draft_proxy.reset(proxy)
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  assert string.contains(
    json.to_string(body),
    "\"id\":\"gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic\"",
  )
}

pub fn process_graphql_request_uses_default_path_test() {
  let proxy = draft_proxy.new()
  let body = "{\"query\":\"{ events(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, body: response_body, ..), _) =
    draft_proxy.process_graphql_request(
      proxy,
      body,
      draft_proxy.default_graphql_request_options(),
    )
  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"events\":{\"nodes\":[]}}}"
}

pub fn process_graphql_request_honors_explicit_api_version_test() {
  let proxy = draft_proxy.new()
  let body = "{\"query\":\"{ events(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_graphql_request(
      proxy,
      body,
      draft_proxy.GraphQLRequestOptions(
        path: None,
        api_version: Some("2024-10"),
        headers: empty_headers(),
      ),
    )
  // Mismatched version still routes since the path matcher is
  // version-agnostic.
  assert status == 200
}

pub fn process_graphql_request_honors_explicit_path_test() {
  let proxy = draft_proxy.new()
  let body = "{\"query\":\"{ events(first: 1) { nodes { id } } }\"}"
  let #(Response(status: status, ..), _) =
    draft_proxy.process_graphql_request(
      proxy,
      body,
      draft_proxy.GraphQLRequestOptions(
        path: Some("/admin/api/2025-04/graphql.json"),
        api_version: None,
        headers: empty_headers(),
      ),
    )
  assert status == 200
}

pub fn default_graphql_path_test() {
  assert draft_proxy.default_graphql_path("2025-01")
    == "/admin/api/2025-01/graphql.json"
  assert draft_proxy.default_graphql_path("unstable")
    == "/admin/api/unstable/graphql.json"
}

// ---------------------------------------------------------------------------
// /__meta/commit
//
// On Erlang, `process_request` drives the upstream replay synchronously
// via gleam_httpc — for an empty log it returns 200 with no attempts.
// On JavaScript, the synchronous route returns 501 pointing callers at
// `process_request_async`.
// ---------------------------------------------------------------------------

@target(erlang)
pub fn meta_commit_empty_log_returns_200_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/commit",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 200
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"ok\":true")
  assert string.contains(serialized, "\"stopIndex\":null")
  assert string.contains(serialized, "\"attempts\":[]")
}

@target(javascript)
pub fn meta_commit_sync_returns_501_on_js_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/commit",
      headers: empty_headers(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 501
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"ok\":false")
  assert string.contains(serialized, "process_request_async")
}

pub fn meta_commit_get_returns_405_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: status, ..), _) =
    draft_proxy.process_request(proxy, meta_get("/__meta/commit"))
  assert status == 405
}

// ---------------------------------------------------------------------------
// dump_state / restore_state
// ---------------------------------------------------------------------------

const fixed_created_at: String = "2026-04-29T12:00:00.000Z"

fn default_dump_string() -> String {
  draft_proxy.new()
  |> draft_proxy.dump_state(fixed_created_at)
  |> json.to_string
}

fn expect_malformed_dump(dump_string: String) {
  let assert Error(err) =
    draft_proxy.restore_state(draft_proxy.new(), dump_string)
  case err {
    draft_proxy.MalformedDumpJson(_) -> Nil
    _ -> panic as "expected MalformedDumpJson error"
  }
}

fn remove_base_state_dump_bucket(dump_string: String, field_name: String) {
  replace_before_marker(
    dump_string,
    "\"stagedState\":{\"kind\":\"plain\",\"value\":{",
    field_name,
  )
}

fn remove_staged_state_dump_bucket(dump_string: String, field_name: String) {
  replace_after_marker(
    dump_string,
    "\"stagedState\":{\"kind\":\"plain\",\"value\":{",
    field_name,
  )
}

fn replace_before_marker(
  dump_string: String,
  marker: String,
  field_name: String,
) -> String {
  let target = "\"" <> field_name <> "\":"
  let replacement = "\"missing" <> field_name <> "\":"
  case string.split_once(dump_string, marker) {
    Ok(#(before, after)) ->
      string.replace(before, target, replacement) <> marker <> after
    Error(_) -> string.replace(dump_string, target, replacement)
  }
}

fn replace_after_marker(
  dump_string: String,
  marker: String,
  field_name: String,
) -> String {
  let target = "\"" <> field_name <> "\":"
  let replacement = "\"missing" <> field_name <> "\":"
  case string.split_once(dump_string, marker) {
    Ok(#(before, after)) ->
      before <> marker <> string.replace(after, target, replacement)
    Error(_) -> string.replace(dump_string, target, replacement)
  }
}

pub fn dump_state_default_proxy_test() {
  let proxy = draft_proxy.new()
  let dumped = json.to_string(draft_proxy.dump_state(proxy, fixed_created_at))
  assert string.contains(
    dumped,
    "\"schema\":\"shopify-draft-proxy/state-dump\"",
  )
  assert string.contains(dumped, "\"createdAt\":\"2026-04-29T12:00:00.000Z\"")
  assert string.contains(dumped, "\"baseState\":{\"kind\":\"plain\"")
  assert string.contains(dumped, "\"stagedState\":{\"kind\":\"plain\"")
  assert string.contains(
    dumped,
    "\"mutationLog\":{\"kind\":\"plain\",\"value\":[]}",
  )
  assert string.contains(dumped, "\"nextSyntheticId\":1")
}

pub fn dump_state_after_mutation_includes_log_and_advances_identity_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let dumped = json.to_string(draft_proxy.dump_state(proxy, fixed_created_at))
  // savedSearchCreate mints SavedSearch/1 + MutationLogEntry/2, advancing
  // the counter to 3.
  assert string.contains(dumped, "\"nextSyntheticId\":3")
  // Synthetic timestamp advances by 1s per mint (mutation log entry).
  assert string.contains(
    dumped,
    "\"nextSyntheticTimestamp\":\"2024-01-01T00:00:01.000Z\"",
  )
  assert string.contains(dumped, "\"id\":\"gid://shopify/MutationLogEntry/2\"")
  assert string.contains(dumped, "\"status\":\"staged\"")
}

pub fn dump_state_now_returns_envelope_with_wallclock_created_at_test() {
  let proxy = draft_proxy.new()
  let dumped = json.to_string(draft_proxy.dump_state_now(proxy))
  // We can't assert the exact timestamp without injecting a clock; just
  // confirm the envelope has the right schema and a non-empty createdAt.
  assert string.contains(
    dumped,
    "\"schema\":\"shopify-draft-proxy/state-dump\"",
  )
  assert string.contains(dumped, "\"createdAt\":\"")
  assert !string.contains(dumped, "\"createdAt\":\"\"")
}

pub fn restore_state_round_trips_synthetic_identity_test() {
  let original = draft_proxy.new()
  let #(_, original) =
    draft_proxy.process_request(
      original,
      graphql_request(saved_search_create_body),
    )
  let dumped =
    json.to_string(draft_proxy.dump_state(original, fixed_created_at))
  let assert Ok(restored) = draft_proxy.restore_state(draft_proxy.new(), dumped)
  // After restore, the next mint reuses the dump's counter, so a new
  // savedSearchCreate gets SavedSearch/3, not SavedSearch/1.
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(
      restored,
      graphql_request(
        "{\"query\":\"mutation { savedSearchCreate(input: { name: \\\"Restored promo\\\", query: \\\"tag:restored\\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }\"}",
      ),
    )
  assert string.contains(
    json.to_string(body),
    "\"id\":\"gid://shopify/SavedSearch/3?shopify-draft-proxy=synthetic\"",
  )
}

pub fn restore_state_round_trips_mutation_log_test() {
  let original = draft_proxy.new()
  let #(_, original) =
    draft_proxy.process_request(
      original,
      graphql_request(saved_search_create_body),
    )
  let original_log = json.to_string(draft_proxy.get_log_snapshot(original))
  let dumped =
    json.to_string(draft_proxy.dump_state(original, fixed_created_at))
  let assert Ok(restored) = draft_proxy.restore_state(draft_proxy.new(), dumped)
  assert json.to_string(draft_proxy.get_log_snapshot(restored)) == original_log
}

pub fn restore_state_round_trips_complete_runtime_dump_test() {
  let original = draft_proxy.new()
  let #(_, original) =
    draft_proxy.process_request(
      original,
      graphql_request(saved_search_create_body),
    )
  let webhook_body =
    "{\"query\":\"mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \\\"https://hooks.example.com/orders\\\", format: JSON }) { webhookSubscription { id topic uri format } userErrors { field message } } }\"}"
  let #(_, original) =
    draft_proxy.process_request(original, graphql_request(webhook_body))
  let original_state = json.to_string(draft_proxy.get_state_snapshot(original))
  let original_log = json.to_string(draft_proxy.get_log_snapshot(original))
  let dumped =
    json.to_string(draft_proxy.dump_state(original, fixed_created_at))
  let assert Ok(restored) = draft_proxy.restore_state(draft_proxy.new(), dumped)
  assert json.to_string(draft_proxy.get_state_snapshot(restored))
    == original_state
  assert json.to_string(draft_proxy.get_log_snapshot(restored)) == original_log
  assert json.to_string(draft_proxy.dump_state(restored, fixed_created_at))
    == dumped
}

pub fn restore_state_round_trips_ported_state_buckets_test() {
  let proxy = draft_proxy.new()
  let #(_, proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(saved_search_create_body),
    )
  let webhook_body =
    "{\"query\":\"mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \\\"https://hooks.example.com/orders\\\", format: JSON }) { webhookSubscription { id topic uri format } userErrors { field message } } }\"}"
  let #(_, proxy) =
    draft_proxy.process_request(proxy, graphql_request(webhook_body))
  let dumped = json.to_string(draft_proxy.dump_state(proxy, fixed_created_at))
  let assert Ok(restored) = draft_proxy.restore_state(draft_proxy.new(), dumped)
  let serialized = json.to_string(draft_proxy.get_state_snapshot(restored))
  assert string.contains(serialized, "\"savedSearches\":{")
  assert string.contains(serialized, "\"webhookSubscriptions\":{")
  assert string.contains(serialized, "\"name\":\"Promo orders\"")
  assert string.contains(serialized, "\"topic\":\"ORDERS_CREATE\"")
}

pub fn restore_snapshot_installs_base_state_and_ignores_unknown_buckets_test() {
  let snapshot =
    "{\"kind\":\"normalized-state-snapshot\",\"baseState\":{\"products\":{},\"savedSearches\":{\"gid://shopify/SavedSearch/900\":{\"id\":\"gid://shopify/SavedSearch/900\",\"legacyResourceId\":\"900\",\"name\":\"Snapshot search\",\"query\":\"tag:snapshot\",\"resourceType\":\"ORDER\",\"searchTerms\":\"\",\"filters\":[],\"cursor\":null}},\"savedSearchOrder\":[\"gid://shopify/SavedSearch/900\"]}}"
  let assert Ok(proxy) =
    draft_proxy.restore_snapshot(draft_proxy.new(), snapshot)
  let serialized = json.to_string(draft_proxy.get_state_snapshot(proxy))
  assert string.contains(serialized, "\"baseState\"")
  assert string.contains(serialized, "\"Snapshot search\"")
  assert string.contains(serialized, "\"stagedState\"")
  assert string.contains(serialized, "\"savedSearches\":{}")
}

pub fn restore_state_rejects_unsupported_schema_test() {
  let proxy = draft_proxy.new()
  let dump_with_bad_schema =
    default_dump_string()
    |> string.replace(
      "\"schema\":\"shopify-draft-proxy/state-dump\"",
      "\"schema\":\"some/other/schema\"",
    )
  let assert Error(err) = draft_proxy.restore_state(proxy, dump_with_bad_schema)
  case err {
    draft_proxy.UnsupportedSchema(found: "some/other/schema") -> Nil
    _ -> panic as "expected UnsupportedSchema error"
  }
}

pub fn restore_state_rejects_unsupported_version_test() {
  let proxy = draft_proxy.new()
  let dump_with_bad_version =
    default_dump_string()
    |> string.replace(
      "\"version\":1,\"createdAt\"",
      "\"version\":99,\"createdAt\"",
    )
  let assert Error(err) =
    draft_proxy.restore_state(proxy, dump_with_bad_version)
  case err {
    draft_proxy.UnsupportedVersion(found: 99) -> Nil
    _ -> panic as "expected UnsupportedVersion error"
  }
}

pub fn restore_state_rejects_unsupported_store_version_test() {
  let proxy = draft_proxy.new()
  let dump_with_bad_store_version =
    default_dump_string()
    |> string.replace("\"store\":{\"version\":1", "\"store\":{\"version\":7")
  let assert Error(err) =
    draft_proxy.restore_state(proxy, dump_with_bad_store_version)
  case err {
    draft_proxy.UnsupportedStoreVersion(found: 7) -> Nil
    _ -> panic as "expected UnsupportedStoreVersion error"
  }
}

pub fn restore_state_rejects_invalid_synthetic_id_test() {
  let proxy = draft_proxy.new()
  let dump_with_zero_id =
    default_dump_string()
    |> string.replace("\"nextSyntheticId\":1", "\"nextSyntheticId\":0")
  let assert Error(err) = draft_proxy.restore_state(proxy, dump_with_zero_id)
  case err {
    draft_proxy.InvalidSyntheticIdentity(_) -> Nil
    _ -> panic as "expected InvalidSyntheticIdentity error"
  }
}

pub fn restore_state_rejects_malformed_json_test() {
  let proxy = draft_proxy.new()
  let assert Error(err) = draft_proxy.restore_state(proxy, "not-json")
  case err {
    draft_proxy.MalformedDumpJson(_) -> Nil
    _ -> panic as "expected MalformedDumpJson error"
  }
}

pub fn restore_state_rejects_missing_fields_test() {
  let proxy = draft_proxy.new()
  // Missing `schema` field.
  let dump_missing_schema =
    "{\"version\":1,\"createdAt\":\"2026-04-29T12:00:00.000Z\",\"store\":{\"version\":1,\"fields\":{\"mutationLog\":[]}},\"syntheticIdentity\":{\"nextSyntheticId\":1,\"nextSyntheticTimestamp\":\"2024-01-01T00:00:00.000Z\"},\"extensions\":{}}"
  let assert Error(err) = draft_proxy.restore_state(proxy, dump_missing_schema)
  case err {
    draft_proxy.MalformedDumpJson(_) -> Nil
    _ -> panic as "expected MalformedDumpJson error"
  }
}

pub fn restore_state_rejects_missing_store_fields_test() {
  let default_dump = default_dump_string()
  expect_malformed_dump(
    default_dump |> string.replace("\"baseState\":", "\"missingBaseState\":"),
  )
  expect_malformed_dump(
    default_dump
    |> string.replace("\"stagedState\":", "\"missingStagedState\":"),
  )
  expect_malformed_dump(
    default_dump
    |> string.replace("\"mutationLog\":", "\"missingMutationLog\":"),
  )
}

pub fn restore_state_rejects_missing_every_serialized_base_state_bucket_test() {
  let default_dump = default_dump_string()
  state_serialization.base_state_dump_field_names()
  |> list.each(fn(field_name) {
    expect_malformed_dump(remove_base_state_dump_bucket(
      default_dump,
      field_name,
    ))
  })
}

pub fn restore_state_rejects_missing_every_serialized_staged_state_bucket_test() {
  let default_dump = default_dump_string()
  state_serialization.staged_state_dump_field_names()
  |> list.each(fn(field_name) {
    expect_malformed_dump(remove_staged_state_dump_bucket(
      default_dump,
      field_name,
    ))
  })
}

pub fn dump_state_constants_are_stable_test() {
  // The schema string and version live in the wire format; assert them
  // explicitly so a refactor can't silently change the on-disk shape.
  assert draft_proxy.state_dump_schema == "shopify-draft-proxy/state-dump"
  assert draft_proxy.state_dump_version == 1
}

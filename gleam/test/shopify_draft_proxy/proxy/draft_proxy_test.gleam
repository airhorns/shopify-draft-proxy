import gleam/dict
import gleam/json
import shopify_draft_proxy/proxy/draft_proxy.{Request, Response}

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

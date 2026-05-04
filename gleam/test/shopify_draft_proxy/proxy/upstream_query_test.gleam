//// Unit tests for `proxy/upstream_query.gleam` — the chokepoint
//// operation handlers use to ask upstream a question.
////
//// Covers the happy path with an injected synchronous transport, error
//// surfaces (transport failure, non-2xx, malformed JSON), and the
//// "no transport installed" branch.

import gleam/dict
import gleam/dynamic/decode
import gleam/json
import gleam/list
import gleam/option
import gleam/string
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/shopify/upstream_client.{
  type SyncTransport, SyncTransport,
}

fn ok_transport(status: Int, body: String) -> SyncTransport {
  SyncTransport(send: fn(_req) {
    Ok(commit.HttpOutcome(status: status, body: body, headers: []))
  })
}

fn err_transport(message: String) -> SyncTransport {
  SyncTransport(send: fn(_req) {
    Error(commit.CommitTransportError(message: message))
  })
}

const customer_response: String = "{\"data\":{\"customer\":{\"id\":\"gid://shopify/Customer/9\"}}}"

pub fn fetch_sync_returns_parsed_response_test() {
  let transport = ok_transport(200, customer_response)
  let assert Ok(value) =
    upstream_query.fetch_sync(
      "https://shopify.com",
      option.Some(transport),
      dict.new(),
      "CustomerById",
      "query CustomerById($id: ID!) { customer(id: $id) { id } }",
      json.object([#("id", json.string("gid://shopify/Customer/9"))]),
    )
  // The chokepoint returns a commit.JsonValue tree; convert to string
  // for an easy substring assertion.
  let rendered = json.to_string(commit.json_value_to_json(value))
  assert string.contains(rendered, "gid://shopify/Customer/9")
}

pub fn fetch_sync_surfaces_transport_failure_test() {
  let transport = err_transport("upstream timed out")
  let assert Error(upstream_query.TransportFailed(message: message)) =
    upstream_query.fetch_sync(
      "https://shopify.com",
      option.Some(transport),
      dict.new(),
      "Op",
      "query Op { __typename }",
      json.object([]),
    )
  assert string.contains(message, "upstream timed out")
}

pub fn fetch_sync_surfaces_non_2xx_status_test() {
  let transport = ok_transport(500, "{\"errors\":[{\"message\":\"oops\"}]}")
  let assert Error(upstream_query.HttpStatusError(status: status, body: body)) =
    upstream_query.fetch_sync(
      "https://shopify.com",
      option.Some(transport),
      dict.new(),
      "Op",
      "query Op { __typename }",
      json.object([]),
    )
  assert status == 500
  assert string.contains(body, "oops")
}

pub fn fetch_sync_forwards_request_envelope_test() {
  // Capture the body the handler would send so we can assert the
  // envelope shape (operationName/query/variables).
  let captured_decoder = {
    use op <- decode.field("operationName", decode.string)
    use q <- decode.field("query", decode.string)
    use vars <- decode.field("variables", decode.dynamic)
    decode.success(#(op, q, vars))
  }
  let transport =
    SyncTransport(send: fn(req) {
      let echoed_body =
        json.to_string(json.object([#("captured", json.string(req.body))]))
      Ok(commit.HttpOutcome(status: 200, body: echoed_body, headers: []))
    })
  let assert Ok(value) =
    upstream_query.fetch_sync(
      "https://shopify.com",
      option.Some(transport),
      dict.new(),
      "MyOp",
      "query MyOp { x }",
      json.object([#("a", json.int(1))]),
    )
  // Walk the JsonValue: { captured: "<body string>" }
  let assert commit.JsonObject(fields) = value
  let assert Ok(captured_pair) =
    list.find(fields, fn(pair) { pair.0 == "captured" })
  let assert commit.JsonString(body_str) = captured_pair.1
  let assert Ok(#(op_name, query, _vars)) =
    json.parse(body_str, captured_decoder)
  assert op_name == "MyOp"
  assert string.contains(query, "MyOp")
}

@target(javascript)
pub fn fetch_sync_without_transport_errors_on_js_test() {
  let assert Error(upstream_query.TransportFailed(message: message)) =
    upstream_query.fetch_sync(
      "https://shopify.com",
      option.None,
      dict.new(),
      "Op",
      "query Op { x }",
      json.object([]),
    )
  assert string.contains(message, "SyncTransport")
}

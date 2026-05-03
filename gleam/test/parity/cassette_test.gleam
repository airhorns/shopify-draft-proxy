//// Unit tests for the parity cassette transport.
////
//// Covers parsing, deep-equal variable matching (including object-key-
//// order insensitivity), repeated-call determinism, and miss messages.

import gleam/http
import gleam/http/request as gleam_http_request
import gleam/string
import parity/cassette
import parity/json_value
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/shopify/upstream_client

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const single_call: String = "{
  \"upstreamCalls\": [
    {
      \"operationName\": \"CustomerById\",
      \"variables\": { \"id\": \"gid://shopify/Customer/123\" },
      \"query\": \"sha:abc\",
      \"response\": {
        \"status\": 200,
        \"body\": { \"data\": { \"customer\": { \"id\": \"gid://shopify/Customer/123\" } } }
      }
    }
  ]
}"

const two_calls: String = "{
  \"upstreamCalls\": [
    {
      \"operationName\": \"CustomerById\",
      \"variables\": { \"id\": \"A\" },
      \"response\": { \"status\": 200, \"body\": { \"data\": { \"tag\": \"first\" } } }
    },
    {
      \"operationName\": \"CustomerById\",
      \"variables\": { \"id\": \"B\" },
      \"response\": { \"status\": 200, \"body\": { \"data\": { \"tag\": \"second\" } } }
    }
  ]
}"

fn build_request(body: String) -> gleam_http_request.Request(String) {
  let assert Ok(req) =
    gleam_http_request.to("https://example.com/admin/api/graphql.json")
  req
  |> gleam_http_request.set_method(http.Post)
  |> gleam_http_request.set_body(body)
}

fn graphql_envelope(operation_name: String, variables: String) -> String {
  "{\"operationName\":\""
  <> operation_name
  <> "\",\"query\":\"q\",\"variables\":"
  <> variables
  <> "}"
}

// ---------------------------------------------------------------------------
// parse_calls_from_capture
// ---------------------------------------------------------------------------

pub fn parse_calls_from_capture_extracts_entries_test() {
  let assert Ok(entries) = cassette.parse_calls_from_capture(single_call)
  assert cassette.entry_count(entries) == 1
}

pub fn parse_calls_from_capture_missing_field_returns_error_test() {
  // No `upstreamCalls` key → MissingUpstreamCalls, so the runner can
  // distinguish unmigrated specs from migrated mutation-only specs.
  let assert Error(cassette.MissingUpstreamCalls) =
    cassette.parse_calls_from_capture("{}")
}

pub fn parse_calls_from_capture_empty_array_is_valid_test() {
  // Empty array is valid — mutation-only scenarios record no upstream
  // traffic but should not be treated as unmigrated.
  let assert Ok(entries) =
    cassette.parse_calls_from_capture("{\"upstreamCalls\":[]}")
  assert cassette.entry_count(entries) == 0
}

pub fn parse_calls_rejects_malformed_test() {
  let result = cassette.parse_calls("not json")
  assert result_is_error(result)
}

// ---------------------------------------------------------------------------
// Transport — happy path
// ---------------------------------------------------------------------------

pub fn transport_matches_recorded_call_test() {
  let assert Ok(transport) = cassette.transport_from_capture(single_call)
  let upstream_client.SyncTransport(send: send) = transport
  let req =
    build_request(graphql_envelope(
      "CustomerById",
      "{\"id\":\"gid://shopify/Customer/123\"}",
    ))
  let assert Ok(commit.HttpOutcome(status: status, body: body)) = send(req)
  assert status == 200
  assert string.contains(body, "gid://shopify/Customer/123")
}

pub fn transport_returns_first_match_for_repeated_call_test() {
  let assert Ok(transport) = cassette.transport_from_capture(two_calls)
  let upstream_client.SyncTransport(send: send) = transport
  let req_a = build_request(graphql_envelope("CustomerById", "{\"id\":\"A\"}"))
  let assert Ok(commit.HttpOutcome(status: _, body: body1)) = send(req_a)
  let assert Ok(commit.HttpOutcome(status: _, body: body2)) = send(req_a)
  // Stateless replay: same input → same output every time.
  assert body1 == body2
  assert string.contains(body1, "first")
}

pub fn transport_distinguishes_variable_values_test() {
  let assert Ok(transport) = cassette.transport_from_capture(two_calls)
  let upstream_client.SyncTransport(send: send) = transport
  let req_a = build_request(graphql_envelope("CustomerById", "{\"id\":\"A\"}"))
  let req_b = build_request(graphql_envelope("CustomerById", "{\"id\":\"B\"}"))
  let assert Ok(commit.HttpOutcome(status: _, body: body_a)) = send(req_a)
  let assert Ok(commit.HttpOutcome(status: _, body: body_b)) = send(req_b)
  assert string.contains(body_a, "first")
  assert string.contains(body_b, "second")
}

pub fn transport_is_object_key_order_insensitive_test() {
  let cassette_json =
    "{
    \"upstreamCalls\": [
      {
        \"operationName\": \"FindThing\",
        \"variables\": { \"a\": 1, \"b\": 2 },
        \"response\": { \"status\": 200, \"body\": { \"hit\": true } }
      }
    ]
  }"
  let assert Ok(transport) = cassette.transport_from_capture(cassette_json)
  let upstream_client.SyncTransport(send: send) = transport
  let req = build_request(graphql_envelope("FindThing", "{\"b\":2,\"a\":1}"))
  let assert Ok(commit.HttpOutcome(status: status, body: body)) = send(req)
  assert status == 200
  assert string.contains(body, "true")
}

// ---------------------------------------------------------------------------
// Transport — miss
// ---------------------------------------------------------------------------

pub fn transport_miss_surfaces_operation_and_variables_test() {
  let assert Ok(transport) = cassette.transport_from_capture(single_call)
  let upstream_client.SyncTransport(send: send) = transport
  let req =
    build_request(graphql_envelope("UnknownOp", "{\"id\":\"gid://X/1\"}"))
  let assert Error(commit.CommitTransportError(message: message)) = send(req)
  assert string.contains(message, "cassette miss")
  assert string.contains(message, "UnknownOp")
}

pub fn transport_miss_includes_variables_in_message_test() {
  let assert Ok(transport) = cassette.transport_from_capture(single_call)
  let upstream_client.SyncTransport(send: send) = transport
  let req =
    build_request(graphql_envelope(
      "CustomerById",
      "{\"id\":\"gid://shopify/Customer/999\"}",
    ))
  let assert Error(commit.CommitTransportError(message: message)) = send(req)
  assert string.contains(message, "gid://shopify/Customer/999")
}

pub fn transport_rejects_malformed_request_body_test() {
  let assert Ok(transport) = cassette.transport_from_capture(single_call)
  let upstream_client.SyncTransport(send: send) = transport
  let req = build_request("not json")
  let assert Error(commit.CommitTransportError(message: message)) = send(req)
  assert string.contains(message, "could not decode request body")
}

// ---------------------------------------------------------------------------
// empty()
// ---------------------------------------------------------------------------

pub fn empty_cassette_misses_every_request_test() {
  let upstream_client.SyncTransport(send: send) = cassette.empty()
  let req = build_request(graphql_envelope("Anything", "{}"))
  let assert Error(commit.CommitTransportError(message: message)) = send(req)
  assert string.contains(message, "cassette miss")
}

// ---------------------------------------------------------------------------
// variables_equal — deep equality semantics
// ---------------------------------------------------------------------------

pub fn variables_equal_handles_int_float_coercion_test() {
  assert cassette.variables_equal(json_value.JInt(1), json_value.JFloat(1.0))
  assert cassette.variables_equal(json_value.JFloat(2.0), json_value.JInt(2))
}

pub fn variables_equal_object_order_insensitive_test() {
  let a =
    json_value.JObject([
      #("a", json_value.JInt(1)),
      #("b", json_value.JInt(2)),
    ])
  let b =
    json_value.JObject([
      #("b", json_value.JInt(2)),
      #("a", json_value.JInt(1)),
    ])
  assert cassette.variables_equal(a, b)
}

pub fn variables_equal_distinguishes_array_order_test() {
  let a = json_value.JArray([json_value.JInt(1), json_value.JInt(2)])
  let b = json_value.JArray([json_value.JInt(2), json_value.JInt(1)])
  assert !cassette.variables_equal(a, b)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn result_is_error(r: Result(_, _)) -> Bool {
  case r {
    Ok(_) -> False
    Error(_) -> True
  }
}

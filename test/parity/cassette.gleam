//// Parity test cassette: recorded `(operationName, variables) → response`
//// entries that the parity runner installs as the proxy's upstream
//// transport. Capture files under `fixtures/conformance/**` carry an
//// `upstreamCalls` array; the runner parses it, builds a `SyncTransport`,
//// and wires it via `draft_proxy.with_upstream_transport`.
////
//// Match key is `(operationName, variables)` with object-key-order-
//// insensitive comparison on variables. The recorded `query` field is
//// debug-only (typically a sha256 of the document).
////
//// Cassette miss is a hard error: tests should fail loudly with the
//// unmatched `(operationName, variables)` so the recorder can be re-run
//// to fill the gap. Order of entries is preserved but not asserted —
//// the same operation issued repeatedly returns the first matching
//// entry every time, which is fine for idempotent reads. Stateful
//// "consume on match" semantics are deliberately not modelled here;
//// when a follow-up scenario needs them, wrap this transport.

import gleam/dict
import gleam/dynamic/decode.{type Decoder}
import gleam/http/request as gleam_http_request
import gleam/int
import gleam/io
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/json_value.{type JsonValue, JArray, JObject}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/proxy/commit.{
  type CommitTransportError, type HttpOutcome, CommitTransportError, HttpOutcome,
}
import shopify_draft_proxy/shopify/upstream_client.{
  type SyncTransport, SyncTransport,
}

/// One recorded upstream interaction.
pub type Entry {
  Entry(
    operation_name: String,
    variables: JsonValue,
    response_status: Int,
    response_body: JsonValue,
  )
}

pub type CassetteError {
  ParseError(message: String)
  MissingUpstreamCalls
}

/// Parse `upstreamCalls` directly from a JSON array string.
pub fn parse_calls(source: String) -> Result(List(Entry), CassetteError) {
  case json.parse(source, decode.list(entry_decoder())) {
    Ok(entries) -> Ok(entries)
    Error(_) -> Error(ParseError(message: "could not decode upstreamCalls"))
  }
}

/// Pull `upstreamCalls` out of a capture file's top-level JSON object.
/// Returns `MissingUpstreamCalls` when the field is absent (the spec
/// hasn't been migrated to cassette playback) or malformed. Returns
/// `Ok([])` when the field is present and empty — that's valid for
/// migrated mutation-only scenarios that don't need any upstream reads.
pub fn parse_calls_from_capture(
  capture_source: String,
) -> Result(List(Entry), CassetteError) {
  let required_decoder = {
    use entries <- decode.field("upstreamCalls", decode.list(entry_decoder()))
    decode.success(entries)
  }
  case json.parse(capture_source, required_decoder) {
    Ok(entries) -> Ok(entries)
    Error(_) -> Error(MissingUpstreamCalls)
  }
}

fn entry_decoder() -> Decoder(Entry) {
  use operation_name <- decode.field("operationName", decode.string)
  use variables <- decode.optional_field(
    "variables",
    json_value.JNull,
    json_value_decoder(),
  )
  use response <- decode.field("response", response_decoder())
  decode.success(Entry(
    operation_name: operation_name,
    variables: variables,
    response_status: response.0,
    response_body: response.1,
  ))
}

fn response_decoder() -> Decoder(#(Int, JsonValue)) {
  use status <- decode.optional_field("status", 200, decode.int)
  use body <- decode.field("body", json_value_decoder())
  decode.success(#(status, body))
}

fn json_value_decoder() -> Decoder(JsonValue) {
  decode.dynamic
  |> decode.then(fn(dyn) {
    case json_value.from_dynamic(dyn) {
      Ok(value) -> decode.success(value)
      Error(message) -> decode.failure(json_value.JNull, message)
    }
  })
}

/// Build a `SyncTransport` from a list of recorded entries.
pub fn make_transport(entries: List(Entry)) -> SyncTransport {
  SyncTransport(send: fn(req) { dispatch(entries, req) })
}

/// Build a `SyncTransport` that logs every match/miss to stderr before
/// returning the same result `make_transport` would. Use during debug
/// runs to see which cassette entries each handler actually consults.
pub fn make_logging_transport(entries: List(Entry)) -> SyncTransport {
  SyncTransport(send: fn(req) {
    let result = dispatch(entries, req)
    log_dispatch(req, result)
    result
  })
}

fn log_dispatch(
  req: gleam_http_request.Request(String),
  result: Result(HttpOutcome, CommitTransportError),
) -> Nil {
  let summary = case parse_request_body(req.body) {
    Ok(#(name, vars)) ->
      "op=" <> name <> " vars=" <> truncate(json_value.to_string(vars), 240)
    Error(_) -> "<unparseable body>"
  }
  case result {
    Ok(HttpOutcome(status: status, body: body, ..)) ->
      io.println_error(
        "[cassette] HIT  "
        <> summary
        <> " -> status="
        <> int.to_string(status)
        <> " body="
        <> truncate(body, 240),
      )
    Error(CommitTransportError(message: message)) ->
      io.println_error("[cassette] MISS " <> summary <> " -> " <> message)
  }
}

fn truncate(value: String, max: Int) -> String {
  case string.length(value) > max {
    True -> string.slice(value, 0, max) <> "…"
    False -> value
  }
}

fn dispatch(
  entries: List(Entry),
  req: gleam_http_request.Request(String),
) -> Result(HttpOutcome, CommitTransportError) {
  case parse_request_body(req.body) {
    Error(message) ->
      Error(CommitTransportError(
        message: "cassette could not decode request body: " <> message,
      ))
    Ok(#(operation_name, variables)) ->
      case find_match(entries, operation_name, variables) {
        Some(entry) ->
          Ok(
            HttpOutcome(
              status: entry.response_status,
              body: encode_response_body(entry.response_body),
              headers: [],
            ),
          )
        None ->
          Error(
            CommitTransportError(message: miss_message(
              operation_name,
              variables,
            )),
          )
      }
  }
}

fn parse_request_body(body: String) -> Result(#(String, JsonValue), String) {
  let envelope_decoder = {
    use operation_name <- decode.optional_field(
      "operationName",
      "",
      decode.string,
    )
    use query <- decode.optional_field("query", "", decode.string)
    use variables <- decode.optional_field(
      "variables",
      json_value.JNull,
      json_value_decoder(),
    )
    decode.success(#(operation_name, query, variables))
  }
  case json.parse(body, envelope_decoder) {
    Ok(#(operation_name, query, variables)) ->
      Ok(#(resolve_operation_name(operation_name, query), variables))
    Error(_) -> Error("request body is not a JSON object")
  }
}

/// Use the explicit `operationName` envelope field when present;
/// otherwise extract it from the `query` document. The runner sends
/// `{query, variables}` without an envelope `operationName`, but the
/// recorded cassette entry was indexed by name (the recorder forwards
/// requests through `runAdminGraphqlRequest`, which derives a name).
fn resolve_operation_name(envelope_name: String, query: String) -> String {
  case envelope_name {
    "" ->
      case parse_operation.parse_operation(query) {
        Ok(parsed) -> option.unwrap(parsed.name, "")
        Error(_) -> ""
      }
    _ -> envelope_name
  }
}

fn find_match(
  entries: List(Entry),
  operation_name: String,
  variables: JsonValue,
) -> Option(Entry) {
  case
    list.find(entries, fn(entry) {
      entry.operation_name == operation_name
      && variables_equal(entry.variables, variables)
    })
  {
    Ok(entry) -> Some(entry)
    Error(_) -> None
  }
}

fn miss_message(operation_name: String, variables: JsonValue) -> String {
  "cassette miss: operation="
  <> operation_name
  <> " variables="
  <> json_value.to_string(variables)
}

fn encode_response_body(value: JsonValue) -> String {
  json_value.to_string(value)
}

/// Deep equality on JSON values where object keys are compared as a
/// set (order-insensitive) and arrays element-wise.
pub fn variables_equal(a: JsonValue, b: JsonValue) -> Bool {
  case a, b {
    json_value.JNull, json_value.JNull -> True
    json_value.JBool(x), json_value.JBool(y) -> x == y
    json_value.JInt(x), json_value.JInt(y) -> x == y
    json_value.JFloat(x), json_value.JFloat(y) -> x == y
    json_value.JString(x), json_value.JString(y) -> x == y
    JArray(xs), JArray(ys) -> arrays_equal(xs, ys)
    JObject(xs), JObject(ys) -> objects_equal(xs, ys)
    // Numbers can be parsed as either int or float depending on
    // serializer choice; treat 1 and 1.0 as equal so cassette matches
    // don't break on round-tripping.
    json_value.JInt(x), json_value.JFloat(y) -> int.to_float(x) == y
    json_value.JFloat(x), json_value.JInt(y) -> x == int.to_float(y)
    _, _ -> False
  }
}

fn arrays_equal(xs: List(JsonValue), ys: List(JsonValue)) -> Bool {
  case xs, ys {
    [], [] -> True
    [x, ..xrest], [y, ..yrest] ->
      case variables_equal(x, y) {
        True -> arrays_equal(xrest, yrest)
        False -> False
      }
    _, _ -> False
  }
}

fn objects_equal(
  xs: List(#(String, JsonValue)),
  ys: List(#(String, JsonValue)),
) -> Bool {
  case list.length(xs) == list.length(ys) {
    False -> False
    True -> {
      let xd = dict.from_list(xs)
      let yd = dict.from_list(ys)
      list.all(dict.to_list(xd), fn(pair) {
        let #(key, x_value) = pair
        case dict.get(yd, key) {
          Ok(y_value) -> variables_equal(x_value, y_value)
          Error(_) -> False
        }
      })
    }
  }
}

/// Convenience: run `parse_calls_from_capture` and immediately wrap the
/// result into a `SyncTransport`. Used by the runner.
pub fn transport_from_capture(
  capture_source: String,
) -> Result(SyncTransport, CassetteError) {
  use entries <- result.try(parse_calls_from_capture(capture_source))
  Ok(make_transport(entries))
}

/// Empty cassette — every request misses. Useful for local runner tests that
/// assert no upstream traffic is permitted.
pub fn empty() -> SyncTransport {
  make_transport([])
}

/// Snapshot of how many entries the cassette holds. Handy for tests
/// that want to assert all entries were consumed (when stateful
/// counting lands later) or just for diagnostics.
pub fn entry_count(entries: List(Entry)) -> Int {
  list.length(entries)
}

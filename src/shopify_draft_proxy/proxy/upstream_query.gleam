//// Single chokepoint that operation handlers use to ask Shopify a
//// question.
////
//// The substrate exists so the parity runner can install a recorded
//// cassette as the proxy's upstream transport (`with_upstream_transport`)
//// and have every per-operation upstream call deterministically replay
//// from that cassette. Production callers leave the transport unset and
//// fall through to the real HTTP shims (`upstream_client.send_sync` on
//// Erlang; the JS-async path is not yet wired here — JS production
//// handlers needing upstream must wait for a Promise-flavoured fetch
//// helper, which lands when the first JS-only domain needs it).
////
//// There is intentionally no domain-wide hydration helper. Each
//// operation calls `fetch_sync` itself and decides what to do with the
//// response: persist into base state, use it once and discard, or
//// transform it into the caller-facing reply. The choice is documented
//// per-handler — see the per-domain migration playbook in
//// `docs/parity-runner.md`.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/proxy/commit.{type JsonValue}
import shopify_draft_proxy/shopify/upstream_client.{type SyncTransport}

/// Shared upstream-call context. Bundles the three pieces every
/// handler needs to issue an upstream GraphQL call: the optional
/// `SyncTransport` (set by parity tests, unset in production), the
/// origin to address, and the inbound request's headers (so the proxy
/// can forward auth tokens etc.). `process_request` builds one of these
/// per inbound request and threads it into any handler that wants to
/// reach upstream.
pub type UpstreamContext {
  UpstreamContext(
    transport: Option(SyncTransport),
    origin: String,
    headers: Dict(String, String),
    allow_upstream_reads: Bool,
  )
}

/// Context whose `fetch_sync` calls fall through to the live HTTP shim
/// on Erlang and fail with `NoTransportInstalled` on JS. Useful for
/// tests and callers that don't have headers or origin in scope.
pub fn empty_upstream_context() -> UpstreamContext {
  UpstreamContext(
    transport: None,
    origin: "",
    headers: dict.new(),
    allow_upstream_reads: False,
  )
}

/// What can go wrong when asking upstream a question. `TransportFailed`
/// reports the underlying network shim's error message; `HttpStatusError`
/// surfaces non-2xx responses (the caller decides whether to swallow or
/// propagate); `MalformedResponse` covers JSON the proxy can't parse;
/// `NoTransportInstalled` is what JS production handlers see today (they
/// can't issue sync upstream calls because production fetch is async,
/// and no async helper exists yet on this seam).
pub type FetchError {
  TransportFailed(message: String)
  HttpStatusError(status: Int, body: String)
  MalformedResponse(message: String)
  NoTransportInstalled
}

/// Path Shopify GraphQL is served on. Mirrors the constant the commit
/// driver uses so cassette entries are keyed by the same URL the
/// production proxy would hit.
const default_graphql_path: String = "/admin/api/graphql.json"

/// Synchronous upstream call. Returns the parsed response body as a
/// `commit.JsonValue` AST so callers can walk it cheaply.
///
/// `variables` is a `Json` tree (the write-only kind produced by
/// `json.object`, `json.string`, etc.). The body sent upstream is the
/// canonical `{"operationName":..,"query":..,"variables":..}` envelope.
///
/// On Erlang the real HTTP shim is used as the fallback when no
/// transport is installed. On JS, no fallback exists yet — call sites
/// that need upstream from JS must install a `SyncTransport` (cassette)
/// or use a future async helper.
pub fn fetch_sync(
  origin: String,
  transport: Option(SyncTransport),
  inbound_headers: Dict(String, String),
  operation_name: String,
  query: String,
  variables: Json,
) -> Result(JsonValue, FetchError) {
  let body = build_request_body(operation_name, query, variables)
  case build_request(origin, body, inbound_headers) {
    Error(message) -> Error(TransportFailed(message: message))
    Ok(req) -> {
      let send = resolve_send(transport)
      case send(req) {
        Ok(commit.HttpOutcome(status: status, body: body_string, ..)) ->
          case status >= 200 && status < 300 {
            True -> Ok(commit.parse_json_value(body_string))
            False -> Error(HttpStatusError(status: status, body: body_string))
          }
        Error(commit.CommitTransportError(message: msg)) ->
          Error(TransportFailed(message: msg))
      }
    }
  }
}

fn build_request_body(
  operation_name: String,
  query: String,
  variables: Json,
) -> String {
  json.to_string(
    json.object([
      #("operationName", json.string(operation_name)),
      #("query", json.string(query)),
      #("variables", variables),
    ]),
  )
}

fn build_request(
  origin: String,
  body: String,
  inbound_headers: Dict(String, String),
) -> Result(_, String) {
  case
    upstream_client.build_graphql_request(
      origin,
      default_graphql_path,
      body,
      inbound_headers,
    )
  {
    Ok(req) -> Ok(req)
    Error(Nil) ->
      Error("invalid upstream url: " <> origin <> default_graphql_path)
  }
}

@target(erlang)
fn resolve_send(
  transport: Option(SyncTransport),
) -> fn(_) -> Result(commit.HttpOutcome, commit.CommitTransportError) {
  case transport {
    Some(t) -> t.send
    None -> upstream_client.send_sync
  }
}

@target(javascript)
fn resolve_send(
  transport: Option(SyncTransport),
) -> fn(_) -> Result(commit.HttpOutcome, commit.CommitTransportError) {
  case transport {
    Some(t) -> t.send
    None -> fn(_) {
      Error(commit.CommitTransportError(
        message: "upstream_query.fetch_sync requires an installed SyncTransport on the JavaScript target (no async fallback yet)",
      ))
    }
  }
}

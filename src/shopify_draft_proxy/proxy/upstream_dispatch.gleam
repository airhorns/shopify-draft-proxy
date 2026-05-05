//// Driver that fetches a GraphQL request upstream when the proxy is in
//// `LiveHybrid` mode. Mirrors the commit-driver split:
////
////   * `fetch_sync/4` — Erlang. Synchronous; returns a `Result` directly.
////   * `fetch_async/4` — JavaScript. Returns a `Promise(Result(...))`.
////
//// Both share the same `send`-shaped seam (built from
//// `upstream_client.send_*`) so tests can inject a fake transport
//// without dragging gleam_httpc/gleam_fetch into the assertion shape.

import gleam/dict.{type Dict}
import gleam/http/request as gleam_http_request
@target(javascript)
import gleam/javascript/promise.{type Promise}
import shopify_draft_proxy/proxy/commit.{
  type CommitTransportError, type HttpOutcome, CommitTransportError,
}
import shopify_draft_proxy/shopify/upstream_client

/// Build the upstream request for a live-hybrid GraphQL passthrough.
/// Mirrors the commit driver's `build_replay_request`, minus the
/// id-map rewrite — live-hybrid bodies are forwarded verbatim.
fn build(
  origin: String,
  path: String,
  body: String,
  inbound_headers: Dict(String, String),
) -> Result(gleam_http_request.Request(String), CommitTransportError) {
  case
    upstream_client.build_graphql_request(origin, path, body, inbound_headers)
  {
    Ok(req) -> Ok(req)
    Error(Nil) ->
      Error(CommitTransportError(
        message: "invalid upstream url: " <> origin <> path,
      ))
  }
}

@target(erlang)
/// Erlang-only synchronous driver. The injected `send` lets tests fake
/// the HTTP transport.
pub fn fetch_sync(
  origin: String,
  path: String,
  body: String,
  inbound_headers: Dict(String, String),
  send: fn(gleam_http_request.Request(String)) ->
    Result(HttpOutcome, CommitTransportError),
) -> Result(HttpOutcome, CommitTransportError) {
  case build(origin, path, body, inbound_headers) {
    Error(e) -> Error(e)
    Ok(req) -> send(req)
  }
}

@target(javascript)
/// JS-only async driver. Promise-based to thread through `fetch`.
pub fn fetch_async(
  origin: String,
  path: String,
  body: String,
  inbound_headers: Dict(String, String),
  send: fn(gleam_http_request.Request(String)) ->
    Promise(Result(HttpOutcome, CommitTransportError)),
) -> Promise(Result(HttpOutcome, CommitTransportError)) {
  case build(origin, path, body, inbound_headers) {
    Error(e) -> promise.resolve(Error(e))
    Ok(req) -> send(req)
  }
}

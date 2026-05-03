//// Shared upstream HTTP client used by both `commit.run_commit_*` and
//// the live-hybrid dispatch path. Mirrors the relevant parts of
//// `src/shopify/upstream-request.ts` plus the small fetch/httpc shims
//// that previously lived inside `commit.gleam` and `draft_proxy.gleam`.
////
//// The transport-level result shapes (`HttpOutcome`, `CommitTransportError`)
//// remain owned by `commit.gleam` so existing tests / consumers don't
//// churn — this module imports and reuses them. The `CommitTransportError`
//// name is a historical accident; treat it as a generic upstream-transport
//// error.

import gleam/dict.{type Dict}
@target(javascript)
import gleam/fetch
import gleam/http
import gleam/http/request as gleam_http_request
@target(erlang)
import gleam/httpc
@target(javascript)
import gleam/javascript/promise.{type Promise}
import gleam/list
import shopify_draft_proxy/proxy/commit.{
  type CommitTransportError, type HttpOutcome,
}

// ---------------------------------------------------------------------------
// Transport seam.
//
// `SyncTransport` is the in-process, synchronous shape that both targets
// can satisfy — it's what parity tests install when they wire a recorded
// cassette into the proxy. Production HTTP on JS is async; that variant
// stays target-specific (`AsyncTransport`) so we don't pretend a Promise
// is a synchronous result.
//
// A handler that needs to ask upstream a question goes through
// `proxy/upstream_query.fetch`; that helper picks `SyncTransport` when the
// proxy has one installed (cassette in tests), otherwise falls back to
// `send_sync` (Erlang) / `send_async` (JS).
// ---------------------------------------------------------------------------

/// Synchronous transport. Cassettes implement this directly; on Erlang
/// production HTTP also fits this shape.
pub type SyncTransport {
  SyncTransport(
    send: fn(gleam_http_request.Request(String)) ->
      Result(HttpOutcome, CommitTransportError),
  )
}

@target(javascript)
/// JS-only async transport. Wraps `send_async` (or any other Promise-
/// returning shim).
pub type AsyncTransport {
  AsyncTransport(
    send: fn(gleam_http_request.Request(String)) ->
      Promise(Result(HttpOutcome, CommitTransportError)),
  )
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

/// Build a POST request to `origin <> path` carrying `body`, with the
/// inbound proxy headers forwarded through `commit.forward_headers/1`.
/// Returns `Error(Nil)` only when the URL is unparseable (config bug).
pub fn build_graphql_request(
  origin: String,
  path: String,
  body: String,
  inbound_headers: Dict(String, String),
) -> Result(gleam_http_request.Request(String), Nil) {
  let url = origin <> path
  use base <- result_try(gleam_http_request.to(url))
  Ok(
    base
    |> gleam_http_request.set_method(http.Post)
    |> gleam_http_request.set_body(body)
    |> apply_headers(commit.forward_headers(inbound_headers)),
  )
}

fn apply_headers(
  req: gleam_http_request.Request(String),
  headers: List(#(String, String)),
) -> gleam_http_request.Request(String) {
  list.fold(headers, req, fn(acc, pair) {
    gleam_http_request.set_header(acc, pair.0, pair.1)
  })
}

fn result_try(r: Result(a, b), fun: fn(a) -> Result(c, b)) -> Result(c, b) {
  case r {
    Ok(value) -> fun(value)
    Error(e) -> Error(e)
  }
}

// ---------------------------------------------------------------------------
// Production HTTP shims. Live here (rather than in `draft_proxy.gleam`)
// so non-commit dispatch paths can reach upstream too. Both adapters
// normalise their library's success / error shapes into
// `commit.HttpOutcome` / `commit.CommitTransportError` so the rest of
// the proxy can be target-agnostic.
// ---------------------------------------------------------------------------

@target(erlang)
pub fn send_sync(
  req: gleam_http_request.Request(String),
) -> Result(commit.HttpOutcome, commit.CommitTransportError) {
  case httpc.send(req) {
    Ok(resp) -> Ok(commit.HttpOutcome(status: resp.status, body: resp.body))
    Error(err) ->
      Error(commit.CommitTransportError(message: httpc_error_message(err)))
  }
}

@target(erlang)
fn httpc_error_message(err: httpc.HttpError) -> String {
  case err {
    httpc.InvalidUtf8Response -> "upstream response body was not valid UTF-8"
    httpc.FailedToConnect(_, _) -> "failed to connect to upstream"
    httpc.ResponseTimeout -> "upstream response timed out"
  }
}

@target(javascript)
pub fn send_async(
  req: gleam_http_request.Request(String),
) -> Promise(Result(commit.HttpOutcome, commit.CommitTransportError)) {
  fetch.send(req)
  |> promise.try_await(fetch.read_text_body)
  |> promise.map(fn(result) {
    case result {
      Ok(resp) -> Ok(commit.HttpOutcome(status: resp.status, body: resp.body))
      Error(err) ->
        Error(commit.CommitTransportError(message: fetch_error_message(err)))
    }
  })
}

@target(javascript)
fn fetch_error_message(err: fetch.FetchError) -> String {
  case err {
    fetch.NetworkError(msg) -> "upstream network error: " <> msg
    fetch.UnableToReadBody -> "unable to read upstream response body"
    fetch.InvalidJsonBody -> "invalid JSON body in upstream response"
  }
}

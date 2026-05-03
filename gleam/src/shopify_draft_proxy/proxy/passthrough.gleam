//// Substrate for forwarding a `DraftProxy` request to upstream Shopify
//// verbatim and returning the upstream response unchanged.
////
//// Lives here (separate from `draft_proxy.gleam`) so domain handlers
//// can call `passthrough_sync` themselves when their per-operation
//// logic decides upstream is the right answer — without importing
//// `draft_proxy` and creating a cycle. The dispatcher in
//// `draft_proxy.gleam` also calls this for its own substrate-level
//// fallbacks (operations marked `Passthrough` in the registry; root
//// fields with no local dispatcher implemented).
////
//// On the Erlang target, `passthrough_sync` always works: it uses the
//// installed `upstream_transport` if any, otherwise the real
//// `upstream_client.send_sync` HTTP shim. On the JavaScript target,
//// the real fetch is async, so `passthrough_sync` only succeeds when a
//// `SyncTransport` (typically a parity cassette) is installed; without
//// one it returns the documented 501 sentinel telling the caller to
//// use `process_request_async`.

import gleam/json
@target(javascript)
import gleam/javascript/promise.{type Promise}
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/proxy_state.{type DraftProxy, Request, Response}
import shopify_draft_proxy/proxy/upstream_dispatch
import shopify_draft_proxy/shopify/upstream_client

@target(erlang)
pub fn passthrough_sync(
  proxy: DraftProxy,
  request: proxy_state.Request,
) -> #(proxy_state.Response, DraftProxy) {
  let send = case proxy.upstream_transport {
    Some(transport) -> transport.send
    None -> upstream_client.send_sync
  }
  let Request(_, path, headers, body) = request
  let outcome =
    upstream_dispatch.fetch_sync(
      proxy.config.shopify_admin_origin,
      path,
      body,
      headers,
      send,
    )
  #(outcome_to_response(outcome), proxy)
}

@target(javascript)
pub fn passthrough_sync(
  proxy: DraftProxy,
  request: proxy_state.Request,
) -> #(proxy_state.Response, DraftProxy) {
  case proxy.upstream_transport {
    Some(transport) -> {
      let Request(_, path, headers, body) = request
      let outcome = case
        upstream_client.build_graphql_request(
          proxy.config.shopify_admin_origin,
          path,
          body,
          headers,
        )
      {
        Ok(req) -> transport.send(req)
        Error(Nil) ->
          Error(commit.CommitTransportError(
            message: "invalid upstream url: "
            <> proxy.config.shopify_admin_origin
            <> path,
          ))
      }
      #(outcome_to_response(outcome), proxy)
    }
    None -> #(async_unsupported_response(), proxy)
  }
}

@target(javascript)
pub fn passthrough_async(
  proxy: DraftProxy,
  request: proxy_state.Request,
) -> Promise(#(proxy_state.Response, DraftProxy)) {
  let send = case proxy.upstream_transport {
    Some(transport) -> sync_transport_to_async(transport.send)
    None -> upstream_client.send_async
  }
  let Request(_, path, headers, body) = request
  upstream_dispatch.fetch_async(
    proxy.config.shopify_admin_origin,
    path,
    body,
    headers,
    send,
  )
  |> promise.map(fn(outcome) { #(outcome_to_response(outcome), proxy) })
}

@target(javascript)
fn sync_transport_to_async(
  send: fn(_) -> Result(commit.HttpOutcome, commit.CommitTransportError),
) -> fn(_) -> Promise(Result(commit.HttpOutcome, commit.CommitTransportError)) {
  fn(req) { promise.resolve(send(req)) }
}

/// Test seam: dispatch a passthrough request with an injected `send`
/// closure so unit tests don't need a real HTTP server. Callers should
/// generally use `passthrough_sync/2` and install a transport via
/// `with_upstream_transport` instead.
@target(erlang)
pub fn passthrough_with_send(
  proxy: DraftProxy,
  request: proxy_state.Request,
  send: fn(_) -> Result(commit.HttpOutcome, commit.CommitTransportError),
) -> #(proxy_state.Response, DraftProxy) {
  let Request(_, path, headers, body) = request
  let outcome =
    upstream_dispatch.fetch_sync(
      proxy.config.shopify_admin_origin,
      path,
      body,
      headers,
      send,
    )
  #(outcome_to_response(outcome), proxy)
}

@target(javascript)
pub fn passthrough_with_send_async(
  proxy: DraftProxy,
  request: proxy_state.Request,
  send: fn(_) -> Promise(Result(commit.HttpOutcome, commit.CommitTransportError)),
) -> Promise(#(proxy_state.Response, DraftProxy)) {
  let Request(_, path, headers, body) = request
  upstream_dispatch.fetch_async(
    proxy.config.shopify_admin_origin,
    path,
    body,
    headers,
    send,
  )
  |> promise.map(fn(outcome) { #(outcome_to_response(outcome), proxy) })
}

fn outcome_to_response(
  outcome: Result(commit.HttpOutcome, commit.CommitTransportError),
) -> proxy_state.Response {
  case outcome {
    Ok(commit.HttpOutcome(status: status, body: body_string)) -> {
      let parsed_body = commit.parse_json_value(body_string)
      Response(
        status: status,
        body: commit.json_value_to_json(parsed_body),
        headers: [],
      )
    }
    Error(commit.CommitTransportError(message: msg)) ->
      Response(
        status: 502,
        body: json.object([
          #(
            "errors",
            json.array([json.object([#("message", json.string(msg))])], fn(x) {
              x
            }),
          ),
        ]),
        headers: [],
      )
  }
}

@target(javascript)
fn async_unsupported_response() -> proxy_state.Response {
  Response(
    status: 501,
    body: json.object([
      #("ok", json.bool(False)),
      #(
        "message",
        json.string(
          "Live-hybrid passthrough requires async dispatch on the JavaScript target. Call process_request_async(proxy, request) and await the returned Promise.",
        ),
      ),
    ]),
    headers: [],
  )
}

//// Substrate-level live-hybrid passthrough tests. The Erlang side
//// uses `process_passthrough_sync` (sync test seam); the JS side uses
//// `process_passthrough_async` and awaits the resulting Promise. Both
//// inject a fake transport so the tests don't need a real upstream.

import gleam/dict
import gleam/http/request as gleam_http_request
@target(javascript)
import gleam/javascript/promise
import gleam/json
import gleam/option.{None}
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/draft_proxy.{type Request}
import shopify_draft_proxy/proxy/proxy_state.{
  Config, LiveHybrid, PassthroughUnsupportedMutations,
  RejectUnsupportedMutations, Request, Response,
}
import shopify_draft_proxy/shopify/upstream_client

fn live_hybrid_proxy() -> draft_proxy.DraftProxy {
  draft_proxy.with_config(Config(
    read_mode: LiveHybrid,
    unsupported_mutation_mode: PassthroughUnsupportedMutations,
    port: 4000,
    shopify_admin_origin: "https://shop.example",
    snapshot_path: None,
  ))
  |> draft_proxy.with_default_registry()
}

fn reject_unsupported_proxy() -> draft_proxy.DraftProxy {
  draft_proxy.with_config(Config(
    read_mode: LiveHybrid,
    unsupported_mutation_mode: RejectUnsupportedMutations,
    port: 4000,
    shopify_admin_origin: "https://shop.example",
    snapshot_path: None,
  ))
  |> draft_proxy.with_default_registry()
}

fn passthrough_request() -> Request {
  // `metaobjectByHandle` is in the registry but lookup misses (it's a
  // Query-only candidate without a matching root field shape that a
  // local handler covers in tests). What matters: the operation's
  // capability resolves to Passthrough/Unknown, exercising the
  // substrate fallback. We use a deliberately unknown root field
  // (`__totallyUnknownRoot`) so the registry's `find_entry` returns
  // None, which the capabilities helper renders as
  // `Unknown / Passthrough`.
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\"{ __totallyUnknownRoot { id } }\"}",
  )
}

fn unsupported_mutation_request() -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\"mutation { definitelyUnsupportedMutation { ok } }\"}",
  )
}

@target(javascript)
fn unported_registry_request() -> Request {
  // `priceListFixedPricesAdd` is an implemented TypeScript registry root,
  // but price-list fixed-price mutation execution is not yet ported to Gleam.
  // Live-hybrid dispatch must therefore use the unsupported passthrough
  // branch instead of claiming a local dispatcher exists.
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\"mutation { priceListFixedPricesAdd(priceListId: \\\"gid://shopify/PriceList/1\\\", prices: []) { userErrors { field message } } }\"}",
  )
}

@target(erlang)
pub fn substrate_passthrough_returns_upstream_body_verbatim_erl_test() {
  let proxy = live_hybrid_proxy()
  let request = passthrough_request()

  let upstream_body = "{\"data\":{\"__totallyUnknownRoot\":{\"id\":\"42\"}}}"
  let fake_send = fn(_req: gleam_http_request.Request(String)) {
    Ok(commit.HttpOutcome(status: 200, body: upstream_body, headers: []))
  }

  let #(Response(status: status, body: body, ..), _next) =
    draft_proxy.process_passthrough_sync(proxy, request, fake_send)

  assert status == 200
  assert json.to_string(body) == upstream_body
}

@target(erlang)
pub fn substrate_passthrough_surfaces_transport_error_as_502_erl_test() {
  let proxy = live_hybrid_proxy()
  let request = passthrough_request()

  let fake_send = fn(_req: gleam_http_request.Request(String)) {
    Error(commit.CommitTransportError(message: "connection refused"))
  }

  let #(Response(status: status, body: body, ..), _next) =
    draft_proxy.process_passthrough_sync(proxy, request, fake_send)

  assert status == 502
  let serialized = json.to_string(body)
  // 502 envelope carries the transport error message under errors[].
  case serialized {
    "{\"errors\":[{\"message\":\"connection refused\"}]}" -> Nil
    _ -> panic as { "unexpected 502 body: " <> serialized }
  }
}

@target(erlang)
pub fn passthrough_only_fires_in_live_hybrid_mode_erl_test() {
  // In Snapshot mode, an unknown operation should NOT trigger
  // passthrough — it should fall through to the existing local error
  // path (bad_request: "No domain dispatcher implemented...").
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry()
  let request = passthrough_request()

  let #(Response(status: status, ..), _next) =
    draft_proxy.process_request(proxy, request)

  // The local fall-through is a 400 bad request, NOT a 502.
  assert status == 400
}

pub fn reject_unsupported_mutations_returns_400_before_upstream_test() {
  let transport =
    upstream_client.SyncTransport(send: fn(_req) {
      panic as "reject mode should not call upstream"
    })
  let proxy =
    reject_unsupported_proxy()
    |> draft_proxy.with_upstream_transport(transport)

  let #(Response(status: status, body: body, ..), _next) =
    draft_proxy.process_request(proxy, unsupported_mutation_request())

  assert status == 400
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Unsupported mutation rejected by configuration: definitelyUnsupportedMutation\"}]}"
}

@target(javascript)
pub fn substrate_passthrough_returns_upstream_body_verbatim_js_test() {
  let proxy = live_hybrid_proxy()
  let request = passthrough_request()

  let upstream_body = "{\"data\":{\"__totallyUnknownRoot\":{\"id\":\"42\"}}}"
  let fake_send = fn(_req: gleam_http_request.Request(String)) {
    promise.resolve(
      Ok(commit.HttpOutcome(status: 200, body: upstream_body, headers: [])),
    )
  }

  use pair <- promise.tap(draft_proxy.process_passthrough_async(
    proxy,
    request,
    fake_send,
  ))
  let #(Response(status: status, body: body, ..), _next) = pair
  assert status == 200
  assert json.to_string(body) == upstream_body
}

@target(javascript)
pub fn substrate_passthrough_surfaces_transport_error_as_502_js_test() {
  let proxy = live_hybrid_proxy()
  let request = passthrough_request()

  let fake_send = fn(_req: gleam_http_request.Request(String)) {
    promise.resolve(Error(commit.CommitTransportError(message: "fetch failed")))
  }

  use pair <- promise.tap(draft_proxy.process_passthrough_async(
    proxy,
    request,
    fake_send,
  ))
  let #(Response(status: status, ..), _next) = pair
  assert status == 502
}

@target(javascript)
pub fn passthrough_sync_dispatch_returns_501_on_js_test() {
  // The sync `process_request` cannot resolve the upstream Promise on
  // JS, so passthrough returns 501 telling the caller to use
  // `process_passthrough_async` / `process_request_async`.
  let proxy = live_hybrid_proxy()
  let request = passthrough_request()

  let #(Response(status: status, body: _body, ..), _next) =
    draft_proxy.process_request(proxy, request)

  assert status == 501
}

@target(javascript)
pub fn unported_implemented_root_uses_passthrough_on_js_test() {
  let proxy = live_hybrid_proxy()
  let request = unported_registry_request()

  let #(Response(status: status, body: body, ..), _next) =
    draft_proxy.process_request(proxy, request)

  assert status == 501
  assert json.to_string(body)
    == "{\"ok\":false,\"message\":\"Live-hybrid passthrough requires async dispatch on the JavaScript target. Call process_request_async(proxy, request) and await the returned Promise.\"}"
}

// ---------------------------------------------------------------------------
// `with_upstream_transport` injection
// ---------------------------------------------------------------------------

pub fn with_upstream_transport_routes_passthrough_via_sync_send_test() {
  let upstream_body = "{\"data\":{\"__totallyUnknownRoot\":{\"id\":\"99\"}}}"
  let transport =
    upstream_client.SyncTransport(send: fn(_req) {
      Ok(commit.HttpOutcome(status: 200, body: upstream_body, headers: []))
    })
  let proxy =
    live_hybrid_proxy()
    |> draft_proxy.with_upstream_transport(transport)

  // `process_request` is the production entry point — when a transport
  // is installed, both Erlang and JS must route passthrough through it
  // synchronously without needing the test seam helpers.
  let #(Response(status: status, body: body, ..), _next) =
    draft_proxy.process_request(proxy, passthrough_request())

  assert status == 200
  assert json.to_string(body) == upstream_body
}

pub fn with_upstream_transport_surfaces_transport_failure_test() {
  let transport =
    upstream_client.SyncTransport(send: fn(_req) {
      Error(commit.CommitTransportError(message: "fake failure"))
    })
  let proxy =
    live_hybrid_proxy()
    |> draft_proxy.with_upstream_transport(transport)

  let #(Response(status: status, ..), _next) =
    draft_proxy.process_request(proxy, passthrough_request())

  assert status == 502
}

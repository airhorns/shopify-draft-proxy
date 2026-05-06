//// JavaScript-only tests for `process_request_async/2` and the
//// Promise-returning `commit/2` driver. These exercise the JS HTTP
//// route end-to-end without hitting a real network — `commit.gleam`
//// exposes `run_commit_async` taking an injected `send` so the tests
//// drive it through a fake fetch shim.
////
//// The point of this file is to prove the JS Promise pipeline works
//// through the *route handler*, not just via a direct `commit/2` call.
//// Cross-target functional coverage (id remap, header forwarding,
//// response interpretation) lives in `commit_test.gleam`. This file
//// stays narrow: confirm `process_request_async` resolves promises
//// for non-commit routes, and that `run_commit_async` produces the
//// same shape as the sync driver.
////
//// Manual smoke (Node REPL):
////
////     gleam build --target javascript
////     node --input-type=module -e '
////       const m = await import("./build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs");
////       const proxy = m.new_();
////       const [resp, _next] = await m.process_request_async(proxy, /* Request */);
////       console.log(resp);
////     '

@target(erlang)
pub fn draft_proxy_async_js_only_placeholder_test() {
  Nil
}

@target(javascript)
import gleam/dict
@target(javascript)
import gleam/javascript/promise.{type Promise}
@target(javascript)
import gleam/json
@target(javascript)
import gleam/list
@target(javascript)
import gleam/option.{None, Some}
@target(javascript)
import gleam/string
@target(javascript)
import shopify_draft_proxy/proxy/commit
@target(javascript)
import shopify_draft_proxy/proxy/draft_proxy
@target(javascript)
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
@target(javascript)
import shopify_draft_proxy/state/store
@target(javascript)
import shopify_draft_proxy/state/store/types as store_types

@target(javascript)
const synthetic_one: String = "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"

@target(javascript)
const authoritative_one: String = "gid://shopify/SavedSearch/12345"

@target(javascript)
fn empty_capability() -> store.Capability {
  store_types.Capability(
    operation_name: Some("savedSearchCreate"),
    domain: "saved-searches",
    execution: "stage-locally",
  )
}

@target(javascript)
fn entry_factory(
  id: String,
  query: String,
  staged: List(String),
) -> store.MutationLogEntry {
  store_types.MutationLogEntry(
    id: id,
    received_at: "2026-04-29T12:00:00.000Z",
    operation_name: Some("savedSearchCreate"),
    path: "/admin/api/2025-01/graphql.json",
    query: query,
    variables: dict.new(),
    staged_resource_ids: staged,
    status: store_types.Staged,
    interpreted: store_types.InterpretedMetadata(
      operation_type: store_types.Mutation,
      operation_name: Some("savedSearchCreate"),
      root_fields: ["savedSearchCreate"],
      primary_root_field: Some("savedSearchCreate"),
      capability: empty_capability(),
    ),
    notes: None,
  )
}

@target(javascript)
fn proxy_with_log(
  entries: List(store.MutationLogEntry),
) -> draft_proxy.DraftProxy {
  let base = draft_proxy.new()
  let s =
    list.fold(entries, base.store, fn(acc, e) {
      store.record_mutation_log_entry(acc, e)
    })
  proxy_state.DraftProxy(..base, store: s)
}

@target(javascript)
fn ok_async_send(
  status: Int,
  body: String,
) -> fn(_) -> Promise(Result(commit.HttpOutcome, commit.CommitTransportError)) {
  fn(_req) {
    promise.resolve(
      Ok(commit.HttpOutcome(status: status, body: body, headers: [])),
    )
  }
}

// ---------------------------------------------------------------------------
// process_request_async — non-commit routes resolve immediately.
// ---------------------------------------------------------------------------

@target(javascript)
pub fn process_request_async_health_resolves_test() -> Promise(Nil) {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/__meta/health",
      headers: dict.new(),
      body: "",
    )
  draft_proxy.process_request_async(proxy, request)
  |> promise.map(fn(pair) {
    let #(Response(status: status, body: body, ..), _) = pair
    assert status == 200
    let serialized = json.to_string(body)
    assert string.contains(serialized, "\"ok\":true")
    Nil
  })
}

@target(javascript)
pub fn process_request_async_unknown_route_returns_404_test() -> Promise(Nil) {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "GET",
      path: "/does-not-exist",
      headers: dict.new(),
      body: "",
    )
  draft_proxy.process_request_async(proxy, request)
  |> promise.map(fn(pair) {
    let #(Response(status: status, ..), _) = pair
    assert status == 404
    Nil
  })
}

// ---------------------------------------------------------------------------
// Sync /__meta/commit on JS still returns 501 with a hint to use async.
// ---------------------------------------------------------------------------

@target(javascript)
pub fn process_request_sync_commit_returns_501_on_js_test() {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/__meta/commit",
      headers: dict.new(),
      body: "",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)
  assert status == 501
  let serialized = json.to_string(body)
  assert string.contains(serialized, "process_request_async")
}

// ---------------------------------------------------------------------------
// run_commit_async — Promise-driver tests with injected fake send.
// ---------------------------------------------------------------------------

@target(javascript)
pub fn run_commit_async_empty_log_resolves_test() -> Promise(Nil) {
  let proxy = draft_proxy.new()
  let send = ok_async_send(200, "{}")
  commit.run_commit_async(proxy.store, "https://shop.example", dict.new(), send)
  |> promise.map(fn(pair) {
    let #(_after, meta) = pair
    assert meta.ok == True
    assert meta.stop_index == None
    assert meta.attempts == []
    Nil
  })
}

@target(javascript)
pub fn run_commit_async_succeeds_for_staged_entry_test() -> Promise(Nil) {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [synthetic_one],
    )
  let proxy = proxy_with_log([entry])
  let response_body =
    "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
    <> authoritative_one
    <> "\"},\"userErrors\":[]}}}"
  let send = ok_async_send(200, response_body)

  commit.run_commit_async(proxy.store, "https://shop.example", dict.new(), send)
  |> promise.map(fn(pair) {
    let #(after, meta) = pair
    assert meta.ok == True
    assert meta.stop_index == None
    let log = store.get_log(after)
    case log {
      [updated] -> {
        assert updated.status == store_types.Committed
        Nil
      }
      _ -> panic as "expected single log entry after commit"
    }
  })
}

@target(javascript)
pub fn run_commit_async_propagates_failure_test() -> Promise(Nil) {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let proxy = proxy_with_log([entry])
  let send = ok_async_send(500, "{\"errors\":[{\"message\":\"boom\"}]}")

  commit.run_commit_async(proxy.store, "https://shop.example", dict.new(), send)
  |> promise.map(fn(pair) {
    let #(after, meta) = pair
    assert meta.ok == False
    assert meta.stop_index == Some(0)
    let log = store.get_log(after)
    case log {
      [updated] -> {
        assert updated.status == store_types.Failed
        Nil
      }
      _ -> panic as "expected single log entry after failure"
    }
  })
}

@target(javascript)
pub fn run_commit_async_transport_error_halts_test() -> Promise(Nil) {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let proxy = proxy_with_log([entry])
  let send = fn(_req) {
    promise.resolve(Error(commit.CommitTransportError(message: "fetch failed")))
  }

  commit.run_commit_async(proxy.store, "https://shop.example", dict.new(), send)
  |> promise.map(fn(pair) {
    let #(_after, meta) = pair
    assert meta.ok == False
    assert meta.stop_index == Some(0)
    case meta.attempts {
      [attempt] -> {
        assert attempt.upstream_error == Some("fetch failed")
        assert attempt.upstream_status == None
        Nil
      }
      _ -> panic as "expected single attempt"
    }
  })
}

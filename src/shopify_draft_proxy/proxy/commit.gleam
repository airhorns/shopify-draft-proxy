//// Mutation-log replay against the upstream Shopify Admin GraphQL endpoint.
//// Mirrors `commitMetaState` in `src/meta/routes.ts:606`.
////
//// All pure logic (id-map building, GID rewriting, response interpretation)
//// lives here and is target-agnostic. The two drivers (`run_commit_sync` for
//// Erlang, `run_commit_async` for JavaScript) are also exposed here, both
//// taking an injected `send` function so tests can drive the engine without
//// real HTTP. `draft_proxy.gleam` wires the production `gleam_httpc` /
//// `gleam_fetch` clients into the corresponding driver via target-specific
//// thin shims.

import gleam/dict.{type Dict}
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/http
import gleam/http/request.{type Request as HttpRequest}
import gleam/http/response.{type Response as HttpResponse}
@target(javascript)
import gleam/javascript/promise.{type Promise}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/state/store.{
  type EntryStatus, type MutationLogEntry, type Store,
}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity

// ---------------------------------------------------------------------------
// Public types — mirror `MetaCommitResponse` and `CommitAttempt` in
// `src/meta/routes.ts:8-24`.
// ---------------------------------------------------------------------------

/// Outcome of a single replay attempt.
pub type CommitAttempt {
  CommitAttempt(
    log_entry_id: String,
    operation_name: Option(String),
    path: String,
    success: Bool,
    status: EntryStatus,
    upstream_status: Option(Int),
    upstream_body: Option(JsonValue),
    upstream_error: Option(String),
    response_body: JsonValue,
  )
}

/// The body shape returned by the `__meta/commit` HTTP route.
pub type MetaCommitResponse {
  MetaCommitResponse(
    ok: Bool,
    stop_index: Option(Int),
    attempts: List(CommitAttempt),
  )
}

/// Normalised HTTP-transport error surfaced by the injected `send`. The
/// Erlang and JS production shims map their respective library errors into
/// this single message-bearing type.
pub type CommitTransportError {
  CommitTransportError(message: String)
}

/// Normalised successful HTTP outcome. Both targets convert their library's
/// response into this so the driver code can remain target-agnostic.
pub type HttpOutcome {
  HttpOutcome(status: Int, body: String, headers: List(#(String, String)))
}

// ---------------------------------------------------------------------------
// Custom JSON-AST. We need a walkable tree to (a) collect authoritative GIDs
// from upstream responses, and (b) round-trip arbitrary JSON unchanged when
// echoing it back through `CommitAttempt.upstream_body` / `response_body`.
// Gleam's `gleam/json.Json` is write-only; this AST is the read-side.
// ---------------------------------------------------------------------------

/// A walkable JSON value parsed from upstream response bodies.
pub type JsonValue {
  JsonNull
  JsonBool(Bool)
  JsonInt(Int)
  JsonFloat(Float)
  JsonString(String)
  JsonArray(List(JsonValue))
  JsonObject(List(#(String, JsonValue)))
}

/// Recursive decoder for arbitrary JSON. Order-preserving for objects so
/// the round-trip matches the input byte ordering when feasible.
pub fn json_value_decoder() -> Decoder(JsonValue) {
  use <- decode.recursive
  decode.one_of(decode.string |> decode.map(JsonString), [
    decode.bool |> decode.map(JsonBool),
    decode.int |> decode.map(JsonInt),
    decode.float |> decode.map(JsonFloat),
    decode.list(json_value_decoder()) |> decode.map(JsonArray),
    decode.dict(decode.string, json_value_decoder())
      |> decode.map(fn(d) { JsonObject(dict.to_list(d)) }),
    decode.success(JsonNull),
  ])
}

/// Convert a parsed `JsonValue` back into a `gleam/json.Json` tree for
/// re-serialisation in the response envelope.
pub fn json_value_to_json(value: JsonValue) -> Json {
  case value {
    JsonNull -> json.null()
    JsonBool(b) -> json.bool(b)
    JsonInt(i) -> json.int(i)
    JsonFloat(f) -> json.float(f)
    JsonString(s) -> json.string(s)
    JsonArray(items) -> json.array(items, json_value_to_json)
    JsonObject(fields) ->
      json.object(
        list.map(fields, fn(pair) {
          let #(k, v) = pair
          #(k, json_value_to_json(v))
        }),
      )
  }
}

/// Parse a JSON string into the AST. Falls back to `JsonString(<raw>)` so
/// upstream responses that aren't JSON don't crash the commit loop —
/// they're surfaced verbatim as a string in `upstream_body`.
pub fn parse_json_value(body: String) -> JsonValue {
  case json.parse(body, json_value_decoder()) {
    Ok(value) -> value
    Error(_) -> JsonString(body)
  }
}

// ---------------------------------------------------------------------------
// GID helpers — mirror `readGidResourceType`, `replaceMappedSyntheticGids`,
// `collectAuthoritativeGidsByType`, `recordCommitIdMappings` in
// `src/meta/routes.ts:48-132`.
// ---------------------------------------------------------------------------

const synthetic_marker_prefix: String = "gid://shopify/"

/// Extract the `Type` segment from `gid://shopify/Type/123(?…)`. Returns
/// `None` if the input isn't a Shopify gid.
pub fn gid_resource_type(value: String) -> Option(String) {
  case string.starts_with(value, synthetic_marker_prefix) {
    False -> None
    True -> {
      let rest =
        string.drop_start(value, string.length(synthetic_marker_prefix))
      case string.split_once(rest, "/") {
        Ok(#(resource_type, _)) ->
          case resource_type {
            "" -> None
            t -> Some(strip_query_suffix(t))
          }
        Error(_) -> None
      }
    }
  }
}

fn strip_query_suffix(value: String) -> String {
  case string.split_once(value, "?") {
    Ok(#(prefix, _)) -> prefix
    Error(_) -> value
  }
}

/// Replace every entry of `id_map` (synthetic gid → authoritative gid) in
/// `value`. Mirrors the chained `replaceAll` walk in the TS helper, but
/// applied to the wire-form request body so we don't have to re-parse and
/// re-serialise every replay. Synthetic GIDs only ever appear in JSON
/// string values (the `gid://…` form is never a JSON key), so substring
/// substitution is equivalent to the AST walk.
pub fn apply_id_map_to_body_string(
  body: String,
  id_map: Dict(String, String),
) -> String {
  dict.fold(id_map, body, fn(acc, synthetic, authoritative) {
    string.replace(acc, synthetic, authoritative)
  })
}

/// Collect every non-synthetic `gid://shopify/Type/…` value found anywhere
/// in `value`, grouped by resource type and de-duplicated in encounter
/// order. Mirrors `collectAuthoritativeGidsByType`.
pub fn collect_authoritative_gids_by_type(
  value: JsonValue,
) -> Dict(String, List(String)) {
  collect_authoritative_walk(value, dict.new())
}

fn collect_authoritative_walk(
  value: JsonValue,
  acc: Dict(String, List(String)),
) -> Dict(String, List(String)) {
  case value {
    JsonString(s) ->
      case
        string.starts_with(s, synthetic_marker_prefix)
        && !synthetic_identity.is_proxy_synthetic_gid(s)
      {
        False -> acc
        True ->
          case gid_resource_type(s) {
            None -> acc
            Some(resource_type) -> {
              let existing = case dict.get(acc, resource_type) {
                Ok(list) -> list
                Error(_) -> []
              }
              case list.contains(existing, s) {
                True -> acc
                False ->
                  dict.insert(acc, resource_type, list.append(existing, [s]))
              }
            }
          }
      }
    JsonArray(items) ->
      list.fold(items, acc, fn(a, item) { collect_authoritative_walk(item, a) })
    JsonObject(fields) ->
      list.fold(fields, acc, fn(a, pair) {
        collect_authoritative_walk(pair.1, a)
      })
    _ -> acc
  }
}

/// Update `id_map` with newly-mintable `synthetic → authoritative` pairs
/// inferred from `entry.staged_resource_ids` (the synthetic GIDs the entry
/// produced when it was staged) and `response_body` (the upstream's actual
/// reply, which carries the real GIDs). Mirrors `recordCommitIdMappings`.
pub fn record_commit_id_mappings(
  entry: MutationLogEntry,
  response_body: JsonValue,
  id_map: Dict(String, String),
) -> Dict(String, String) {
  case entry.staged_resource_ids {
    [] -> id_map
    staged -> {
      let by_type = collect_authoritative_gids_by_type(response_body)
      list.fold(staged, id_map, fn(map, staged_id) {
        case
          synthetic_identity.is_proxy_synthetic_gid(staged_id)
          && !dict_has_key(map, staged_id)
        {
          False -> map
          True ->
            case gid_resource_type(staged_id) {
              None -> map
              Some(resource_type) ->
                case dict.get(by_type, resource_type) {
                  Ok([authoritative, ..]) ->
                    dict.insert(map, staged_id, authoritative)
                  _ -> map
                }
            }
        }
      })
    }
  }
}

fn dict_has_key(d: Dict(String, String), key: String) -> Bool {
  case dict.get(d, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

// ---------------------------------------------------------------------------
// Replay-body construction — mirror `buildCommitReplayBody` in
// `src/meta/routes.ts:39`. The Gleam mutation log doesn't carry a raw
// `requestBody` field yet (deferred — see GLEAM_PORT_LOG.md), so every
// replay is reconstructed from `query` + `variables`. That's the same
// fallback path the TS code takes when `requestBody` is absent.
// ---------------------------------------------------------------------------

pub fn build_replay_body(entry: MutationLogEntry) -> String {
  let body =
    json.object([
      #("query", json.string(entry.query)),
      #(
        "variables",
        json.object(
          list.map(dict.to_list(entry.variables), fn(pair) {
            let #(k, v) = pair
            #(k, root_field.resolved_value_to_json(v))
          }),
        ),
      ),
    ])
  json.to_string(body)
}

// ---------------------------------------------------------------------------
// Header forwarding — mirror `buildForwardedGraphQLHeaders` and
// `buildShopifyDraftProxyUserAgent` in `src/shopify/upstream-request.ts`.
// ---------------------------------------------------------------------------

const proxy_user_agent_marker: String = "shopify-draft-proxy"

const omitted_forward_headers: List(String) = [
  "connection", "content-length", "host", "keep-alive", "proxy-authenticate",
  "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade",
]

/// Lower-cased name + trimmed value, with the omitted set stripped. Forces
/// `content-type: application/json` and stamps `user-agent` with our marker
/// (wrapping the inbound UA when present).
pub fn forward_headers(
  incoming: Dict(String, String),
) -> List(#(String, String)) {
  let normalized =
    dict.fold(incoming, [], fn(acc, name, value) {
      let lower = string.lowercase(name)
      case list.contains(omitted_forward_headers, lower) {
        True -> acc
        False -> [#(lower, value), ..acc]
      }
    })

  let incoming_user_agent =
    list.find_map(normalized, fn(pair) {
      case pair.0 {
        "user-agent" -> Ok(pair.1)
        _ -> Error(Nil)
      }
    })

  let without_overrides =
    list.filter(normalized, fn(pair) {
      pair.0 != "content-type" && pair.0 != "user-agent"
    })

  [
    #("content-type", "application/json"),
    #("user-agent", proxy_user_agent(option.from_result(incoming_user_agent))),
    ..without_overrides
  ]
}

/// Build the User-Agent string the proxy stamps on outbound replays.
/// Mirrors `buildShopifyDraftProxyUserAgent`.
pub fn proxy_user_agent(incoming: Option(String)) -> String {
  case incoming {
    None -> proxy_user_agent_marker
    Some(value) ->
      case string.trim(value) {
        "" -> proxy_user_agent_marker
        trimmed -> proxy_user_agent_marker <> " (wrapping " <> trimmed <> ")"
      }
  }
}

// ---------------------------------------------------------------------------
// Response interpretation — mirror `responseBodyHasGraphQLErrors` and the
// per-entry success/failure branching in `commitMetaState`.
// ---------------------------------------------------------------------------

const commit_succeeded_notes: String = "Committed to upstream Shopify via __meta/commit replay."

const commit_failed_notes: String = "Commit replay failed against upstream Shopify."

const commit_threw_notes_prefix: String = "Commit replay failed before an upstream response was received: "

/// `True` when `body` contains a non-empty top-level `errors` array, the
/// GraphQL convention for a failed operation. Mirrors the TS helper.
pub fn response_body_has_graphql_errors(body: JsonValue) -> Bool {
  case body {
    JsonObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == "errors" }) {
        Ok(#(_, JsonArray([_, ..]))) -> True
        _ -> False
      }
    _ -> False
  }
}

// ---------------------------------------------------------------------------
// Per-entry step. The driver computes the upstream send before calling this;
// `send_outcome` is the normalised result (`Ok(HttpOutcome)` for any HTTP
// reply including 4xx/5xx, `Error(CommitTransportError)` for connection /
// timeout / DNS issues).
// ---------------------------------------------------------------------------

pub fn step(
  proxy_store: Store,
  entry: MutationLogEntry,
  id_map: Dict(String, String),
  send_outcome: Result(HttpOutcome, CommitTransportError),
) -> #(Store, Dict(String, String), CommitAttempt, Bool) {
  case send_outcome {
    Ok(HttpOutcome(status: status, body: body_string, ..)) -> {
      let body = parse_json_value(body_string)
      let failed = status >= 400 || response_body_has_graphql_errors(body)
      case failed {
        True -> {
          let updated_store =
            store.update_log_entry(
              proxy_store,
              entry.id,
              store_types.Failed,
              Some(commit_failed_notes),
            )
          let attempt =
            CommitAttempt(
              log_entry_id: entry.id,
              operation_name: entry.operation_name,
              path: entry.path,
              success: False,
              status: store_types.Failed,
              upstream_status: Some(status),
              upstream_body: Some(body),
              upstream_error: None,
              response_body: body,
            )
          #(updated_store, id_map, attempt, True)
        }
        False -> {
          let new_id_map = record_commit_id_mappings(entry, body, id_map)
          let updated_store =
            store.update_log_entry(
              proxy_store,
              entry.id,
              store_types.Committed,
              Some(commit_succeeded_notes),
            )
          let attempt =
            CommitAttempt(
              log_entry_id: entry.id,
              operation_name: entry.operation_name,
              path: entry.path,
              success: True,
              status: store_types.Committed,
              upstream_status: Some(status),
              upstream_body: Some(body),
              upstream_error: None,
              response_body: body,
            )
          #(updated_store, new_id_map, attempt, False)
        }
      }
    }
    Error(CommitTransportError(message: msg)) -> {
      let updated_store =
        store.update_log_entry(
          proxy_store,
          entry.id,
          store_types.Failed,
          Some(commit_threw_notes_prefix <> msg),
        )
      let attempt =
        CommitAttempt(
          log_entry_id: entry.id,
          operation_name: entry.operation_name,
          path: entry.path,
          success: False,
          status: store_types.Failed,
          upstream_status: None,
          upstream_body: None,
          upstream_error: Some(msg),
          response_body: JsonObject([
            #(
              "errors",
              JsonArray([JsonObject([#("message", JsonString(msg))])]),
            ),
          ]),
        )
      #(updated_store, id_map, attempt, True)
    }
  }
}

// ---------------------------------------------------------------------------
// Replay-request construction — turns an entry + the running id_map into
// the gleam_http `Request(String)` to hand the injected `send`.
// ---------------------------------------------------------------------------

/// Build the `gleam_http` request to send upstream for a single entry.
/// `headers` is the inbound proxy request's headers (forwarded with the
/// usual stripping/overrides applied) and `origin` is the configured
/// `shopifyAdminOrigin`. Returns `Error(Nil)` only when origin+path don't
/// form a parseable URL — which would be a config bug.
pub fn build_replay_request(
  origin: String,
  entry: MutationLogEntry,
  id_map: Dict(String, String),
  inbound_headers: Dict(String, String),
) -> Result(HttpRequest(String), Nil) {
  let body = apply_id_map_to_body_string(build_replay_body(entry), id_map)
  let url = origin <> entry.path
  use base <- result.try(request.to(url))
  Ok(
    base
    |> request.set_method(http.Post)
    |> request.set_body(body)
    |> apply_headers(forward_headers(inbound_headers)),
  )
}

fn apply_headers(
  req: HttpRequest(String),
  headers: List(#(String, String)),
) -> HttpRequest(String) {
  list.fold(headers, req, fn(acc, pair) {
    request.set_header(acc, pair.0, pair.1)
  })
}

// ---------------------------------------------------------------------------
// Response serialisation — mirror the JSON envelope `commitResponse` builds
// at `src/proxy-instance.ts:340` (`{ ok: true, ...result }`).
// ---------------------------------------------------------------------------

pub fn serialize_meta_response(meta: MetaCommitResponse) -> Json {
  json.object([
    #("ok", json.bool(meta.ok)),
    #("stopIndex", case meta.stop_index {
      None -> json.null()
      Some(i) -> json.int(i)
    }),
    #("attempts", json.array(meta.attempts, serialize_attempt)),
  ])
}

fn serialize_attempt(attempt: CommitAttempt) -> Json {
  json.object([
    #("logEntryId", json.string(attempt.log_entry_id)),
    #("operationName", optional_string(attempt.operation_name)),
    #("path", json.string(attempt.path)),
    #("success", json.bool(attempt.success)),
    #("status", json.string(entry_status_to_string(attempt.status))),
    #("upstreamStatus", case attempt.upstream_status {
      None -> json.null()
      Some(i) -> json.int(i)
    }),
    #("upstreamBody", case attempt.upstream_body {
      None -> json.null()
      Some(v) -> json_value_to_json(v)
    }),
    #("upstreamError", case attempt.upstream_error {
      None -> json.null()
      Some(msg) -> json.object([#("message", json.string(msg))])
    }),
    #("responseBody", json_value_to_json(attempt.response_body)),
  ])
}

fn entry_status_to_string(status: EntryStatus) -> String {
  case status {
    store_types.Staged -> "staged"
    store_types.Proxied -> "proxied"
    store_types.Committed -> "committed"
    store_types.Failed -> "failed"
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    None -> json.null()
    Some(s) -> json.string(s)
  }
}

// ---------------------------------------------------------------------------
// Entry filter — mirror `logEntryRequiresCommit`.
// ---------------------------------------------------------------------------

pub fn entry_requires_commit(entry: MutationLogEntry) -> Bool {
  entry.status == store_types.Staged
}

// ---------------------------------------------------------------------------
// Driver: synchronous (Erlang). `send` is invoked once per pending entry;
// the driver halts on the first failure.
//
// Cross-target visible — JS tests can inject a sync fake to exercise the
// pure logic without dragging Promise types into the assertion shape.
// ---------------------------------------------------------------------------

pub fn run_commit_sync(
  proxy_store: Store,
  origin: String,
  inbound_headers: Dict(String, String),
  send: fn(HttpRequest(String)) -> Result(HttpOutcome, CommitTransportError),
) -> #(Store, MetaCommitResponse) {
  let pending = list.filter(store.get_log(proxy_store), entry_requires_commit)
  let initial = #(proxy_store, dict.new(), [], None, 0, False)
  let #(final_store, _id_map, attempts_rev, stop_index, _index, _halted) =
    list.fold(pending, initial, fn(acc, entry) {
      let #(s, id_map, attempts_rev, stop_index, index, halted) = acc
      case halted {
        True -> #(s, id_map, attempts_rev, stop_index, index, halted)
        False -> {
          let send_outcome = case
            build_replay_request(origin, entry, id_map, inbound_headers)
          {
            Error(Nil) ->
              Error(CommitTransportError(
                message: "invalid upstream url: " <> origin <> entry.path,
              ))
            Ok(req) -> send(req)
          }
          let #(s2, id_map2, attempt, halted2) =
            step(s, entry, id_map, send_outcome)
          let stop_index2 = case halted2 {
            True -> Some(index)
            False -> stop_index
          }
          #(
            s2,
            id_map2,
            [attempt, ..attempts_rev],
            stop_index2,
            index + 1,
            halted2,
          )
        }
      }
    })
  let attempts = list.reverse(attempts_rev)
  let meta =
    MetaCommitResponse(
      ok: stop_index == None,
      stop_index: stop_index,
      attempts: attempts,
    )
  #(final_store, meta)
}

// ---------------------------------------------------------------------------
// Driver: asynchronous (JavaScript). Threads the same fold through a
// Promise chain so the Promise leak stays contained to this function and
// `commit/2` in `draft_proxy.gleam`. Exposed only on the JS target.
// ---------------------------------------------------------------------------

@target(javascript)
pub fn run_commit_async(
  proxy_store: Store,
  origin: String,
  inbound_headers: Dict(String, String),
  send: fn(HttpRequest(String)) ->
    Promise(Result(HttpOutcome, CommitTransportError)),
) -> Promise(#(Store, MetaCommitResponse)) {
  let pending = list.filter(store.get_log(proxy_store), entry_requires_commit)
  let initial = promise.resolve(#(proxy_store, dict.new(), [], None, 0, False))
  list.fold(pending, initial, fn(acc, entry) {
    promise.await(acc, fn(state) {
      let #(s, id_map, attempts_rev, stop_index, index, halted) = state
      case halted {
        True ->
          promise.resolve(#(s, id_map, attempts_rev, stop_index, index, halted))
        False -> {
          let send_promise = case
            build_replay_request(origin, entry, id_map, inbound_headers)
          {
            Error(Nil) ->
              promise.resolve(
                Error(CommitTransportError(
                  message: "invalid upstream url: " <> origin <> entry.path,
                )),
              )
            Ok(req) -> send(req)
          }
          promise.await(send_promise, fn(send_outcome) {
            let #(s2, id_map2, attempt, halted2) =
              step(s, entry, id_map, send_outcome)
            let stop_index2 = case halted2 {
              True -> Some(index)
              False -> stop_index
            }
            promise.resolve(#(
              s2,
              id_map2,
              [attempt, ..attempts_rev],
              stop_index2,
              index + 1,
              halted2,
            ))
          })
        }
      }
    })
  })
  |> promise.map(fn(state) {
    let #(final_store, _id_map, attempts_rev, stop_index, _index, _halted) =
      state
    let attempts = list.reverse(attempts_rev)
    let meta =
      MetaCommitResponse(
        ok: stop_index == None,
        stop_index: stop_index,
        attempts: attempts,
      )
    #(final_store, meta)
  })
}

// ---------------------------------------------------------------------------
// Suppress unused-warning for the `Dynamic` import on targets that don't
// reach the JsonValue decoder path. (Kept import to express intent.)
// ---------------------------------------------------------------------------

@internal
pub fn unused_dynamic_keepalive(d: Dynamic) -> Dynamic {
  d
}

// `HttpResponse` import is held for downstream readability (driver returns
// Stores plus `MetaCommitResponse`, not raw `gleam_http` responses, but the
// type appears in helper signatures users may extend). Re-exported via:
@internal
pub fn unused_response_keepalive(
  r: HttpResponse(String),
) -> HttpResponse(String) {
  r
}

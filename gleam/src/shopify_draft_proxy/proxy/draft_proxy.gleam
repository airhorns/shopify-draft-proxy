//// Mirrors the public-API surface of `src/proxy-instance.ts` and the
//// dispatcher spine of `src/proxy/routes.ts`.
////
//// Routes real HTTP-shaped requests through the currently ported
//// GraphQL domains plus the meta API (`/__meta/health`, `/__meta/config`,
//// `/__meta/log`, `/__meta/state`, `/__meta/reset`, `/__meta/commit`).
//// Unsupported paths and unported roots keep returning Shopify-like
//// HTTP/GraphQL error envelopes until their domains land.
////
//// The TS class is mutable; this Gleam port is not. Each dispatch
//// returns a `#(Response, DraftProxy)` pair so the synthetic identity
//// registry (and, eventually, the store) can be threaded forward.

import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/http/request as gleam_http_request
import gleam/int
@target(javascript)
import gleam/javascript/promise.{type Promise}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{Field}
import shopify_draft_proxy/graphql/parse_operation.{
  type GraphQLOperationType, type ParsedOperation, MutationOperation,
  QueryOperation,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/bulk_operations
import shopify_draft_proxy/proxy/capabilities
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers
import shopify_draft_proxy/proxy/delivery_settings
import shopify_draft_proxy/proxy/events
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/proxy/marketing
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/operation_registry.{type RegistryEntry}
import shopify_draft_proxy/proxy/operation_registry_data
import shopify_draft_proxy/proxy/privacy
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/saved_searches
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_dispatch
import shopify_draft_proxy/proxy/webhooks
import shopify_draft_proxy/shopify/upstream_client
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/serialization as state_serialization
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// The `schema` string used in the dump envelope. Mirrors
/// `DRAFT_PROXY_STATE_DUMP_SCHEMA` in the TS proxy so dumps written by
/// either implementation are accepted by both.
pub const state_dump_schema: String = "shopify-draft-proxy/state-dump"

/// The `version` integer used in the dump envelope. Bump only when
/// breaking the on-disk shape.
pub const state_dump_version: Int = 1

/// The `version` integer for the inner `store` slice of the envelope.
/// Mirrors `InMemoryStoreStateDumpV1.version`.
const store_dump_version: Int = 1

/// Default Shopify Admin API version the convenience wrapper uses when
/// the caller doesn't supply one. Mirrors the TS default.
const default_admin_api_version: String = "2025-01"

/// The HTTP-shaped request the proxy accepts. Mirrors
/// `DraftProxyRequest`.
pub type Request {
  Request(
    method: String,
    path: String,
    headers: Dict(String, String),
    body: String,
  )
}

/// HTTP-shaped response. Mirrors `DraftProxyHttpResponse`. The body is
/// pre-serialized as a JSON tree so callers can `json.to_string` it
/// without re-encoding.
pub type Response {
  Response(status: Int, body: Json, headers: List(#(String, String)))
}

/// How the proxy answers reads. Mirrors the TS `AppConfig['readMode']`.
/// Only the variants actually exercised by the spike are modelled; any
/// extension to TS will need a corresponding variant here.
pub type ReadMode {
  Snapshot
  LiveHybrid
  Live
}

/// Sanitised configuration the proxy was constructed with. Mirrors the
/// fields of `AppConfig` that surface through `GET /__meta/config`.
pub type Config {
  Config(
    read_mode: ReadMode,
    port: Int,
    shopify_admin_origin: String,
    snapshot_path: Option(String),
  )
}

/// Long-lived runtime state owned by the proxy. The TS class wraps
/// this in a stateful `DraftProxy`; here it's just a record threaded
/// through each request.
pub type DraftProxy {
  DraftProxy(
    config: Config,
    synthetic_identity: SyntheticIdentityRegistry,
    store: Store,
    /// Registry-driven dispatch table. Empty by default — when empty,
    /// the dispatcher falls back to the hardcoded `domain_for`
    /// predicates (matches Pass 1–7 behavior so existing tests keep
    /// working). Load via `with_registry` once a real config is
    /// available.
    registry: List(RegistryEntry),
  )
}

/// Default config, mirroring the values the TS test suite uses when no
/// explicit config is supplied.
pub fn default_config() -> Config {
  Config(
    read_mode: Snapshot,
    port: 4000,
    shopify_admin_origin: "https://shopify.com",
    snapshot_path: None,
  )
}

/// Fresh proxy with default config. Equivalent to `new DraftProxy(...)`.
pub fn new() -> DraftProxy {
  with_config(default_config())
}

/// Fresh proxy with the supplied config.
pub fn with_config(config: Config) -> DraftProxy {
  DraftProxy(
    config: config,
    synthetic_identity: synthetic_identity.new(),
    store: store.new(),
    registry: [],
  )
}

/// Attach a parsed operation registry to the proxy. Once attached,
/// query/mutation dispatch routes by capability instead of the
/// hardcoded predicates. Mirrors the dispatcher transition the TS
/// proxy made when `operation-registry.json` started driving
/// `routes.ts`.
pub fn with_registry(
  proxy: DraftProxy,
  registry: List(RegistryEntry),
) -> DraftProxy {
  DraftProxy(..proxy, registry: registry)
}

/// Attach the vendored default registry built from
/// `config/operation-registry.json` (mirrored as Gleam source in
/// `operation_registry_data.gleam`). Convenience wrapper around
/// `with_registry/2` for callers that want the registry the TS proxy
/// loads at startup.
pub fn with_default_registry(proxy: DraftProxy) -> DraftProxy {
  with_registry(proxy, operation_registry_data.default_registry())
}

/// Process a request and return the response paired with the updated
/// proxy state. The TS class returns just a response (mutating itself
/// in place); the Gleam port returns both halves so callers can thread
/// the registry forward.
pub fn process_request(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  case route(request) {
    Health -> #(health_response(), proxy)
    MetaConfig -> #(ok_json_response(get_config_snapshot(proxy)), proxy)
    MetaLog -> #(ok_json_response(get_log_snapshot(proxy)), proxy)
    MetaState -> #(ok_json_response(get_state_snapshot(proxy)), proxy)
    MetaReset -> #(reset_response(), reset(proxy))
    MetaCommit -> dispatch_meta_commit_sync(proxy, request)
    GraphQL(version: _) -> dispatch_graphql(proxy, request)
    NotFound -> #(not_found_response(), proxy)
    MethodNotAllowed -> #(method_not_allowed_response(), proxy)
  }
}

type Route {
  Health
  MetaConfig
  MetaLog
  MetaState
  MetaReset
  MetaCommit
  GraphQL(version: String)
  NotFound
  MethodNotAllowed
}

fn route(request: Request) -> Route {
  let method = string.uppercase(request.method)
  case request.path {
    "/__meta/health" -> only_method("GET", method, Health)
    "/__meta/config" -> only_method("GET", method, MetaConfig)
    "/__meta/log" -> only_method("GET", method, MetaLog)
    "/__meta/state" -> only_method("GET", method, MetaState)
    "/__meta/reset" -> only_method("POST", method, MetaReset)
    "/__meta/commit" -> only_method("POST", method, MetaCommit)
    other ->
      case is_admin_graphql_path(other) {
        Ok(version) -> only_method("POST", method, GraphQL(version: version))
        Error(_) -> NotFound
      }
  }
}

fn only_method(expected: String, actual: String, route: Route) -> Route {
  case actual == expected {
    True -> route
    False -> MethodNotAllowed
  }
}

fn is_admin_graphql_path(path: String) -> Result(String, Nil) {
  // Match /admin/api/{version}/graphql.json without pulling in a regex
  // dependency. Splits cheaply into segments and walks the prefix.
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] -> Ok(version)
    _ -> Error(Nil)
  }
}

fn health_response() -> Response {
  Response(
    status: 200,
    body: json.object([
      #("ok", json.bool(True)),
      #("message", json.string("shopify-draft-proxy is running")),
    ]),
    headers: [],
  )
}

fn ok_json_response(body: Json) -> Response {
  Response(status: 200, body: body, headers: [])
}

/// Sanitised runtime configuration, equivalent to the TS class's
/// `getConfig()` and the body of `GET /__meta/config`. Returns the JSON
/// tree directly so callers can `json.to_string` it or thread it into
/// their own envelope.
pub fn get_config_snapshot(proxy: DraftProxy) -> Json {
  let snapshot_enabled = case proxy.config.snapshot_path {
    Some(_) -> True
    None -> False
  }
  let snapshot_path = case proxy.config.snapshot_path {
    Some(p) -> json.string(p)
    None -> json.null()
  }
  json.object([
    #(
      "runtime",
      json.object([
        #("readMode", json.string(read_mode_to_string(proxy.config.read_mode))),
      ]),
    ),
    #(
      "proxy",
      json.object([
        #("port", json.int(proxy.config.port)),
        #("shopifyAdminOrigin", json.string(proxy.config.shopify_admin_origin)),
      ]),
    ),
    #(
      "snapshot",
      json.object([
        #("enabled", json.bool(snapshot_enabled)),
        #("path", snapshot_path),
      ]),
    ),
  ])
}

fn read_mode_to_string(mode: ReadMode) -> String {
  case mode {
    Snapshot -> "snapshot"
    LiveHybrid -> "live-hybrid"
    Live -> "live"
  }
}

/// Mutation log snapshot, equivalent to the TS class's `getLog()` and
/// the body of `GET /__meta/log`. Entries are returned in original
/// replay order.
pub fn get_log_snapshot(proxy: DraftProxy) -> Json {
  json.object([
    #(
      "entries",
      json.array(store.get_log(proxy.store), serialize_mutation_log_entry),
    ),
  ])
}

/// Base + staged in-memory state snapshot, equivalent to the TS class's
/// `getState()` and the body of `GET /__meta/state`.
pub fn get_state_snapshot(proxy: DraftProxy) -> Json {
  json.object([
    #(
      "baseState",
      state_serialization.serialize_base_state(proxy.store.base_state),
    ),
    #(
      "stagedState",
      state_serialization.serialize_staged_state(proxy.store.staged_state),
    ),
  ])
}

fn serialize_mutation_log_entry(entry: store.MutationLogEntry) -> Json {
  json.object([
    #("id", json.string(entry.id)),
    #("receivedAt", json.string(entry.received_at)),
    #("operationName", optional_string(entry.operation_name)),
    #("path", json.string(entry.path)),
    #("query", json.string(entry.query)),
    #(
      "variables",
      json.object(
        dict.to_list(entry.variables)
        |> list.map(fn(pair) {
          let #(k, v) = pair
          #(k, json.string(v))
        }),
      ),
    ),
    #("stagedResourceIds", json.array(entry.staged_resource_ids, json.string)),
    #("status", json.string(entry_status_to_string(entry.status))),
    #("interpreted", serialize_interpreted_metadata(entry.interpreted)),
    #("notes", optional_string(entry.notes)),
  ])
}

fn serialize_interpreted_metadata(meta: store.InterpretedMetadata) -> Json {
  json.object([
    #(
      "operationType",
      json.string(operation_type_to_string(meta.operation_type)),
    ),
    #("operationName", optional_string(meta.operation_name)),
    #("rootFields", json.array(meta.root_fields, json.string)),
    #("primaryRootField", optional_string(meta.primary_root_field)),
    #(
      "capability",
      json.object([
        #("operationName", optional_string(meta.capability.operation_name)),
        #("domain", json.string(meta.capability.domain)),
        #("execution", json.string(meta.capability.execution)),
      ]),
    ),
  ])
}

fn operation_type_to_string(op: store.OperationType) -> String {
  case op {
    store.Query -> "query"
    store.Mutation -> "mutation"
  }
}

fn entry_status_to_string(status: store.EntryStatus) -> String {
  case status {
    store.Staged -> "staged"
    store.Proxied -> "proxied"
    store.Committed -> "committed"
    store.Failed -> "failed"
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

fn reset_response() -> Response {
  Response(
    status: 200,
    body: json.object([
      #("ok", json.bool(True)),
      #("message", json.string("state reset")),
    ]),
    headers: [],
  )
}

fn not_found_response() -> Response {
  Response(
    status: 404,
    body: json.object([
      #(
        "errors",
        json.array(
          [json.object([#("message", json.string("Not found"))])],
          fn(x) { x },
        ),
      ),
    ]),
    headers: [],
  )
}

fn method_not_allowed_response() -> Response {
  Response(
    status: 405,
    body: json.object([
      #(
        "errors",
        json.array(
          [json.object([#("message", json.string("Method not allowed"))])],
          fn(x) { x },
        ),
      ),
    ]),
    headers: [],
  )
}

fn dispatch_graphql(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  case parse_request_body(request.body) {
    Error(message) -> #(bad_request(message), proxy)
    Ok(body) ->
      case parse_operation.parse_operation(body.query) {
        Error(_) -> #(bad_request("Could not parse GraphQL operation"), proxy)
        Ok(parsed) ->
          case live_hybrid_passthrough_target(proxy, parsed) {
            True -> dispatch_passthrough(proxy, request)
            False ->
              case parsed.type_, list.first(parsed.root_fields) {
                QueryOperation, Ok(field) ->
                  route_query(proxy, parsed, body.query, field, body.variables)
                MutationOperation, Ok(field) ->
                  route_mutation(
                    proxy,
                    parsed,
                    request.path,
                    body.query,
                    field,
                    body.variables,
                  )
                _, _ -> #(bad_request("Operation has no root field"), proxy)
              }
          }
      }
  }
}

/// Substrate-level passthrough check. Returns `True` only when the
/// proxy is in `LiveHybrid` mode AND the operation maps to the
/// `Passthrough` execution branch (i.e. unknown / unimplemented). For
/// every other read-mode the proxy answers locally; for every other
/// capability execution the per-domain dispatcher already covers
/// reads. Returning `False` here lets the existing local pipeline
/// handle the request unchanged.
fn live_hybrid_passthrough_target(
  proxy: DraftProxy,
  parsed: ParsedOperation,
) -> Bool {
  case proxy.config.read_mode {
    LiveHybrid -> {
      let cap = capabilities.get_operation_capability(parsed, proxy.registry)
      case cap.execution {
        operation_registry.Passthrough -> True
        _ ->
          case list.first(parsed.root_fields) {
            Ok(primary_root_field) ->
              !local_dispatch_supported(cap.type_, primary_root_field)
            Error(_) -> False
          }
      }
    }
    _ -> False
  }
}

@target(erlang)
fn dispatch_passthrough(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  passthrough_via(proxy, request, upstream_client.send_sync)
}

@target(erlang)
/// Erlang-only test seam: dispatch a passthrough request with an
/// injected `send`. Mirrors `commit.run_commit_sync` accepting a fake
/// transport so tests don't need a real HTTP server. Production
/// callers should use `process_request/2` or `dispatch_graphql`
/// directly.
pub fn process_passthrough_sync(
  proxy: DraftProxy,
  request: Request,
  send: fn(gleam_http_request.Request(String)) ->
    Result(commit.HttpOutcome, commit.CommitTransportError),
) -> #(Response, DraftProxy) {
  passthrough_via(proxy, request, send)
}

@target(javascript)
/// JS-only test seam: same shape as `process_passthrough_sync` but
/// the injected `send` returns a Promise.
pub fn process_passthrough_async(
  proxy: DraftProxy,
  request: Request,
  send: fn(gleam_http_request.Request(String)) ->
    Promise(Result(commit.HttpOutcome, commit.CommitTransportError)),
) -> Promise(#(Response, DraftProxy)) {
  upstream_dispatch.fetch_async(
    proxy.config.shopify_admin_origin,
    request.path,
    request.body,
    request.headers,
    send,
  )
  |> promise.map(fn(outcome) {
    #(passthrough_outcome_to_response(outcome), proxy)
  })
}

@target(javascript)
fn dispatch_passthrough(
  proxy: DraftProxy,
  _request: Request,
) -> #(Response, DraftProxy) {
  // Passthrough requires an async fetch on JS — sync dispatch can't
  // resolve the Promise. Callers that want live-hybrid on JS must use
  // `process_request_async/2`, which awaits the upstream call.
  #(passthrough_async_unsupported_response(), proxy)
}

@target(erlang)
fn passthrough_via(
  proxy: DraftProxy,
  request: Request,
  send: fn(gleam_http_request.Request(String)) ->
    Result(commit.HttpOutcome, commit.CommitTransportError),
) -> #(Response, DraftProxy) {
  let outcome =
    upstream_dispatch.fetch_sync(
      proxy.config.shopify_admin_origin,
      request.path,
      request.body,
      request.headers,
      send,
    )
  #(passthrough_outcome_to_response(outcome), proxy)
}

fn passthrough_outcome_to_response(
  outcome: Result(commit.HttpOutcome, commit.CommitTransportError),
) -> Response {
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
fn passthrough_async_unsupported_response() -> Response {
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

/// Single point of mutation log entry recording. Each domain
/// `process_mutation` returns a `MutationOutcome` carrying
/// `log_drafts: List(LogDraft)`; the dispatcher records them here so
/// individual handlers can never silently skip the buffer (which was
/// the regression in `gift_cards`/`localization`/`metafield_definitions`/
/// `segments` before this refactor centralized recording).
fn finalize_mutation_outcome(
  proxy: DraftProxy,
  request_path: String,
  query: String,
  data: Json,
  next_store: Store,
  next_identity: SyntheticIdentityRegistry,
  log_drafts: List(mutation_helpers.LogDraft),
) -> #(Response, DraftProxy) {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      next_store,
      next_identity,
      request_path,
      query,
      log_drafts,
    )
  #(
    Response(status: 200, body: data, headers: []),
    DraftProxy(
      ..proxy,
      store: logged_store,
      synthetic_identity: logged_identity,
    ),
  )
}

fn route_mutation(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  request_path: String,
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case mutation_domain_for(proxy, parsed, query, primary_root_field) {
    Ok(SavedSearchesDomain) ->
      case
        saved_searches.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle saved searches mutation"),
          proxy,
        )
      }
    Ok(WebhooksDomain) ->
      case
        webhooks.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle webhooks mutation"), proxy)
      }
    Ok(AppsDomain) ->
      case
        apps.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          proxy.config.shopify_admin_origin,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle apps mutation"), proxy)
      }
    Ok(FunctionsDomain) ->
      case
        functions.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle functions mutation"), proxy)
      }
    Ok(GiftCardsDomain) ->
      case
        gift_cards.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle gift cards mutation"),
          proxy,
        )
      }
    Ok(SegmentsDomain) ->
      case
        segments.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle segments mutation"), proxy)
      }
    Ok(MetafieldDefinitionsDomain) ->
      case
        metafield_definitions.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle metafield definitions mutation"),
          proxy,
        )
      }
    Ok(LocalizationDomain) ->
      case
        localization.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle localization mutation"),
          proxy,
        )
      }
    Ok(MetaobjectDefinitionsDomain) ->
      case
        metaobject_definitions.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle metaobject definitions mutation"),
          proxy,
        )
      }
    Ok(MarketingDomain) ->
      case
        marketing.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle marketing mutation"), proxy)
      }
    Ok(BulkOperationsDomain) ->
      case
        bulk_operations.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle bulk operations mutation"),
          proxy,
        )
      }
    Ok(AdminPlatformDomain) ->
      case
        admin_platform.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(
          bad_request("Failed to handle admin platform mutation"),
          proxy,
        )
      }
    Ok(StorePropertiesDomain) ->
      case
        store_properties.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) -> #(
          Response(status: 200, body: outcome.data, headers: []),
          DraftProxy(
            ..proxy,
            store: outcome.store,
            synthetic_identity: outcome.identity,
          ),
        )
        Error(_) -> #(
          bad_request("Failed to handle store properties mutation"),
          proxy,
        )
      }
    Ok(ProductsDomain) ->
      case
        products.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle products mutation"), proxy)
      }
    Ok(PrivacyDomain) ->
      case
        privacy.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) ->
          finalize_mutation_outcome(
            proxy,
            request_path,
            query,
            outcome.data,
            outcome.store,
            outcome.identity,
            outcome.log_drafts,
          )
        Error(_) -> #(bad_request("Failed to handle privacy mutation"), proxy)
      }
    Ok(CustomersDomain) ->
      case
        customers.process_mutation(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
        )
      {
        Ok(outcome) -> #(
          Response(status: 200, body: outcome.data, headers: []),
          DraftProxy(
            ..proxy,
            store: outcome.store,
            synthetic_identity: outcome.identity,
          ),
        )
        Error(_) -> #(bad_request("Failed to handle customers mutation"), proxy)
      }
    Ok(_) | Error(_) -> #(
      bad_request(
        "No mutation dispatcher implemented for root field: "
        <> primary_root_field,
      ),
      proxy,
    )
  }
}

fn route_query(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case query_domain_for(proxy, parsed, query, primary_root_field) {
    Ok(EventsDomain) ->
      respond(proxy, events.process(query), "Failed to handle events query")
    Ok(DeliverySettingsDomain) ->
      respond(
        proxy,
        delivery_settings.process(query),
        "Failed to handle delivery settings query",
      )
    Ok(SavedSearchesDomain) ->
      respond(
        proxy,
        saved_searches.process(proxy.store, query, variables),
        "Failed to handle saved searches query",
      )
    Ok(WebhooksDomain) ->
      respond(
        proxy,
        webhooks.process(proxy.store, query, variables),
        "Failed to handle webhooks query",
      )
    Ok(AppsDomain) ->
      respond(
        proxy,
        apps.process(proxy.store, query, variables),
        "Failed to handle apps query",
      )
    Ok(FunctionsDomain) ->
      respond(
        proxy,
        functions.process(proxy.store, query, variables),
        "Failed to handle functions query",
      )
    Ok(GiftCardsDomain) ->
      respond(
        proxy,
        gift_cards.process(proxy.store, query, variables),
        "Failed to handle gift cards query",
      )
    Ok(SegmentsDomain) ->
      respond(
        proxy,
        segments.process(proxy.store, query, variables),
        "Failed to handle segments query",
      )
    Ok(MetafieldDefinitionsDomain) ->
      respond(
        proxy,
        metafield_definitions.process(proxy.store, query, variables),
        "Failed to handle metafield definitions query",
      )
    Ok(LocalizationDomain) ->
      respond(
        proxy,
        localization.process(proxy.store, query, variables),
        "Failed to handle localization query",
      )
    Ok(MetaobjectDefinitionsDomain) ->
      respond(
        proxy,
        metaobject_definitions.process(proxy.store, query, variables),
        "Failed to handle metaobject definitions query",
      )
    Ok(MarketingDomain) ->
      respond(
        proxy,
        marketing.process(proxy.store, query, variables),
        "Failed to handle marketing query",
      )
    Ok(BulkOperationsDomain) ->
      respond(
        proxy,
        bulk_operations.process(proxy.store, query, variables),
        "Failed to handle bulk operations query",
      )
    Ok(MediaDomain) ->
      respond(proxy, media.process(query), "Failed to handle media query")
    Ok(ProductsDomain) ->
      respond(
        proxy,
        products.process(proxy.store, query, variables),
        "Failed to handle products query",
      )
    Ok(AdminPlatformDomain) ->
      respond(
        proxy,
        admin_platform.process(proxy.store, query, variables),
        "Failed to handle admin platform query",
      )
    Ok(StorePropertiesDomain) ->
      respond(
        proxy,
        store_properties.process(proxy.store, query, variables),
        "Failed to handle store properties query",
      )
    Ok(CustomersDomain) ->
      respond(
        proxy,
        customers.process(proxy.store, query, variables),
        "Failed to handle customers query",
      )
    Ok(PrivacyDomain) -> #(
      bad_request(
        "No domain dispatcher implemented for root field: "
        <> primary_root_field,
      ),
      proxy,
    )
    Error(_) -> #(
      bad_request(
        "No domain dispatcher implemented for root field: "
        <> primary_root_field,
      ),
      proxy,
    )
  }
}

type Domain {
  EventsDomain
  DeliverySettingsDomain
  SavedSearchesDomain
  WebhooksDomain
  AppsDomain
  FunctionsDomain
  GiftCardsDomain
  SegmentsDomain
  MetafieldDefinitionsDomain
  LocalizationDomain
  MetaobjectDefinitionsDomain
  MarketingDomain
  BulkOperationsDomain
  MediaDomain
  ProductsDomain
  AdminPlatformDomain
  StorePropertiesDomain
  PrivacyDomain
  CustomersDomain
}

/// Resolve a query operation's domain. The registry decides whether a
/// known root is implemented at all; the local dispatch table decides
/// whether this Gleam port can actually handle that root today.
fn query_domain_for(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
) -> Result(Domain, Nil) {
  case parsed.type_ {
    QueryOperation -> {
      case
        operation_registry.find_entry(proxy.registry, operation_registry.Query, [
          Some(primary_root_field),
        ])
      {
        Some(entry) ->
          case entry.implemented {
            True -> local_query_dispatch_domain(primary_root_field, query)
            False -> Error(Nil)
          }
        None -> local_query_dispatch_domain(primary_root_field, query)
      }
    }
    _ -> Error(Nil)
  }
}

fn mutation_domain_for(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
) -> Result(Domain, Nil) {
  case parsed.type_ {
    MutationOperation -> {
      case
        operation_registry.find_entry(
          proxy.registry,
          operation_registry.Mutation,
          [
            Some(primary_root_field),
          ],
        )
      {
        Some(entry) ->
          case entry.implemented {
            True -> local_mutation_dispatch_domain(primary_root_field, query)
            False -> Error(Nil)
          }
        None -> local_mutation_dispatch_domain(primary_root_field, query)
      }
    }
    _ -> Error(Nil)
  }
}

/// True when a registry entry names a root that this Gleam port can
/// dispatch locally today. This intentionally gates on the explicit
/// local dispatch table so registry metadata cannot claim unported
/// roots as local support.
pub fn registry_entry_has_local_dispatch(entry: RegistryEntry) -> Bool {
  case entry.implemented {
    False -> False
    True ->
      list.any(entry.match_names, fn(name) {
        local_registry_dispatch_supported(entry.type_, name)
      })
  }
}

fn local_dispatch_supported(type_: GraphQLOperationType, name: String) -> Bool {
  case type_ {
    QueryOperation ->
      case local_query_dispatch_domain(name, "") {
        Ok(_) -> True
        Error(_) -> False
      }
    MutationOperation ->
      case local_mutation_dispatch_domain(name, "") {
        Ok(_) -> True
        Error(_) -> False
      }
  }
}

fn local_registry_dispatch_supported(
  type_: operation_registry.OperationType,
  name: String,
) -> Bool {
  case type_ {
    operation_registry.Query ->
      case local_query_dispatch_domain(name, "") {
        Ok(_) -> True
        Error(_) -> False
      }
    operation_registry.Mutation ->
      case local_mutation_dispatch_domain(name, "") {
        Ok(_) -> True
        Error(_) -> False
      }
  }
}

fn local_query_dispatch_domain(
  name: String,
  query: String,
) -> Result(Domain, Nil) {
  case name {
    "event" | "events" | "eventsCount" -> Ok(EventsDomain)
    "deliverySettings" | "deliveryPromiseSettings" -> Ok(DeliverySettingsDomain)
    "shop" -> Ok(StorePropertiesDomain)
    "product" | "collection" ->
      case store_publishable_owner_query(name, query) {
        True -> Ok(StorePropertiesDomain)
        False -> Ok(ProductsDomain)
      }
    _ ->
      case saved_searches.is_saved_search_query_root(name) {
        True -> Ok(SavedSearchesDomain)
        False ->
          case webhooks.is_webhook_subscription_query_root(name) {
            True -> Ok(WebhooksDomain)
            False ->
              case apps.is_app_query_root(name) {
                True -> Ok(AppsDomain)
                False ->
                  case functions.is_function_query_root(name) {
                    True -> Ok(FunctionsDomain)
                    False ->
                      case gift_cards.is_gift_card_query_root(name) {
                        True -> Ok(GiftCardsDomain)
                        False ->
                          case segments.is_segment_query_root(name) {
                            True -> Ok(SegmentsDomain)
                            False ->
                              case products.is_products_query_root(name) {
                                True -> Ok(ProductsDomain)
                                False ->
                                  case customers.is_customer_query_root(name) {
                                    True -> Ok(CustomersDomain)
                                    False ->
                                      case
                                        metafield_definitions.is_metafield_definitions_query_root(
                                          name,
                                        )
                                      {
                                        True -> Ok(MetafieldDefinitionsDomain)
                                        False ->
                                          case
                                            localization.is_localization_query_root(
                                              name,
                                            )
                                          {
                                            True -> Ok(LocalizationDomain)
                                            False ->
                                              case
                                                metaobject_definitions.is_metaobject_definitions_query_root(
                                                  name,
                                                )
                                              {
                                                True ->
                                                  Ok(
                                                    MetaobjectDefinitionsDomain,
                                                  )
                                                False ->
                                                  case
                                                    marketing.is_marketing_query_root(
                                                      name,
                                                    )
                                                  {
                                                    True -> Ok(MarketingDomain)
                                                    False ->
                                                      case
                                                        bulk_operations.is_bulk_operations_query_root(
                                                          name,
                                                        )
                                                      {
                                                        True ->
                                                          Ok(
                                                            BulkOperationsDomain,
                                                          )
                                                        False ->
                                                          case
                                                            media.is_media_query_root(
                                                              name,
                                                            )
                                                          {
                                                            True ->
                                                              Ok(MediaDomain)
                                                            False ->
                                                              case
                                                                admin_platform.is_admin_platform_query_root(
                                                                  name,
                                                                )
                                                              {
                                                                True ->
                                                                  Ok(
                                                                    AdminPlatformDomain,
                                                                  )
                                                                False ->
                                                                  case
                                                                    store_properties.is_store_properties_query_root(
                                                                      name,
                                                                    )
                                                                  {
                                                                    True ->
                                                                      Ok(
                                                                        StorePropertiesDomain,
                                                                      )
                                                                    False ->
                                                                      Error(Nil)
                                                                  }
                                                              }
                                                          }
                                                      }
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                          }
                      }
                  }
              }
          }
      }
  }
}

fn store_publishable_owner_query(name: String, query: String) -> Bool {
  case root_field.get_root_fields(query) {
    Error(_) -> False
    Ok(fields) ->
      fields
      |> list.any(fn(field) {
        case field {
          Field(name: field_name, ..) if field_name.value == name ->
            selection_names_request_store_publishable_fields(
              name,
              root_field.get_selection_names(field),
            )
          _ -> False
        }
      })
  }
}

fn selection_names_request_store_publishable_fields(
  root_name: String,
  names: List(String),
) -> Bool {
  let has_store_properties_publication_field =
    list.any(names, fn(name) {
      case name {
        "publishedOnCurrentPublication"
        | "publishedOnPublication"
        | "availablePublicationsCount"
        | "resourcePublicationsCount" -> True
        _ -> False
      }
    })
  case root_name {
    "collection" ->
      has_store_properties_publication_field
      && list.any(names, fn(name) {
        name == "availablePublicationsCount"
        || name == "resourcePublicationsCount"
      })
    _ -> False
  }
}

fn local_mutation_dispatch_domain(
  name: String,
  query: String,
) -> Result(Domain, Nil) {
  case publishable_mutation_requests_store_properties(name, query) {
    True -> Ok(StorePropertiesDomain)
    False -> local_non_store_publishable_mutation_dispatch_domain(name)
  }
}

fn local_non_store_publishable_mutation_dispatch_domain(
  name: String,
) -> Result(Domain, Nil) {
  case products.is_products_mutation_root(name) {
    True -> Ok(ProductsDomain)
    False ->
      case store_properties.is_store_properties_mutation_root(name) {
        True -> Ok(StorePropertiesDomain)
        False ->
          case saved_searches.is_saved_search_mutation_root(name) {
            True -> Ok(SavedSearchesDomain)
            False ->
              case webhooks.is_webhook_subscription_mutation_root(name) {
                True -> Ok(WebhooksDomain)
                False ->
                  case apps.is_app_mutation_root(name) {
                    True -> Ok(AppsDomain)
                    False ->
                      case functions.is_function_mutation_root(name) {
                        True -> Ok(FunctionsDomain)
                        False ->
                          case gift_cards.is_gift_card_mutation_root(name) {
                            True -> Ok(GiftCardsDomain)
                            False ->
                              case segments.is_segment_mutation_root(name) {
                                True -> Ok(SegmentsDomain)
                                False ->
                                  case
                                    metafield_definitions.is_metafield_definitions_mutation_root(
                                      name,
                                    )
                                  {
                                    True -> Ok(MetafieldDefinitionsDomain)
                                    False ->
                                      case
                                        localization.is_localization_mutation_root(
                                          name,
                                        )
                                      {
                                        True -> Ok(LocalizationDomain)
                                        False ->
                                          case
                                            metaobject_definitions.is_metaobject_definitions_mutation_root(
                                              name,
                                            )
                                          {
                                            True ->
                                              Ok(MetaobjectDefinitionsDomain)
                                            False ->
                                              case
                                                marketing.is_marketing_mutation_root(
                                                  name,
                                                )
                                              {
                                                True -> Ok(MarketingDomain)
                                                False ->
                                                  case
                                                    bulk_operations.is_bulk_operations_mutation_root(
                                                      name,
                                                    )
                                                  {
                                                    True ->
                                                      Ok(BulkOperationsDomain)
                                                    False ->
                                                      case
                                                        admin_platform.is_admin_platform_mutation_root(
                                                          name,
                                                        )
                                                      {
                                                        True ->
                                                          Ok(
                                                            AdminPlatformDomain,
                                                          )
                                                        False ->
                                                          case
                                                            privacy.is_privacy_mutation_root(
                                                              name,
                                                            )
                                                          {
                                                            True ->
                                                              Ok(PrivacyDomain)
                                                            False ->
                                                              case
                                                                customers.is_customer_mutation_root(
                                                                  name,
                                                                )
                                                              {
                                                                True ->
                                                                  Ok(
                                                                    CustomersDomain,
                                                                  )
                                                                False ->
                                                                  Error(Nil)
                                                              }
                                                          }
                                                      }
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                          }
                      }
                  }
              }
          }
      }
  }
}

fn publishable_mutation_requests_store_properties(
  name: String,
  query: String,
) -> Bool {
  case name {
    "publishablePublish" | "publishableUnpublish" ->
      string.contains(query, "publishedOnCurrentPublication")
      || string.contains(query, "availablePublicationsCount")
      || string.contains(query, " shop ")
      || string.contains(query, "shop {")
    _ -> False
  }
}

fn respond(
  proxy: DraftProxy,
  result: Result(Json, a),
  error_message: String,
) -> #(Response, DraftProxy) {
  case result {
    Ok(envelope) -> #(Response(status: 200, body: envelope, headers: []), proxy)
    Error(_) -> #(bad_request(error_message), proxy)
  }
}

type ParsedBody {
  ParsedBody(query: String, variables: Dict(String, root_field.ResolvedValue))
}

fn parse_request_body(body: String) -> Result(ParsedBody, String) {
  let decoder = {
    use query <- decode.field("query", decode.string)
    use variables <- decode.optional_field(
      "variables",
      dict.new(),
      variables_dict_decoder(),
    )
    decode.success(ParsedBody(query: query, variables: variables))
  }
  json.parse(body, decoder)
  |> result.map_error(fn(_) { "Expected JSON body with a string `query`" })
}

fn variables_dict_decoder() -> decode.Decoder(
  Dict(String, root_field.ResolvedValue),
) {
  decode.dict(decode.string, resolved_value_decoder())
}

/// Recursively decode an arbitrary JSON value into the
/// `root_field.ResolvedValue` shape used by argument resolution.
/// Unknown shapes (including JSON `null`) fall through to `NullVal`.
fn resolved_value_decoder() -> decode.Decoder(root_field.ResolvedValue) {
  decode.recursive(fn() {
    decode.one_of(decode.bool |> decode.map(root_field.BoolVal), or: [
      decode.int |> decode.map(root_field.IntVal),
      decode.float |> decode.map(root_field.FloatVal),
      decode.string |> decode.map(root_field.StringVal),
      decode.list(of: resolved_value_decoder())
        |> decode.map(root_field.ListVal),
      decode.dict(decode.string, resolved_value_decoder())
        |> decode.map(root_field.ObjectVal),
      decode.success(root_field.NullVal),
    ])
  })
}

fn bad_request(message: String) -> Response {
  Response(
    status: 400,
    body: json.object([
      #(
        "errors",
        json.array([json.object([#("message", json.string(message))])], fn(x) {
          x
        }),
      ),
    ]),
    headers: [],
  )
}

/// Render a port number for the cli/server adapter. Currently unused
/// but exposed so callers can confirm the proxy was constructed with
/// the right config.
pub fn config_summary(config: Config) -> String {
  read_mode_to_string(config.read_mode) <> "@" <> int.to_string(config.port)
}

// ---------------------------------------------------------------------------
// Standalone DraftProxy methods
//
// These mirror the TS class's instance methods so callers that don't want
// to thread an HTTP-shaped request through `process_request` can drive
// the proxy directly. Every one of these is also reachable via a `__meta`
// route — the route handlers delegate here.
// ---------------------------------------------------------------------------

/// Clear staged state, mutation log, and synthetic identity counters.
/// Mirrors the TS class's `reset()` method and the body-effect of
/// `POST /__meta/reset`.
pub fn reset(proxy: DraftProxy) -> DraftProxy {
  DraftProxy(
    ..proxy,
    synthetic_identity: synthetic_identity.reset(proxy.synthetic_identity),
    store: store.reset(proxy.store),
  )
}

/// Options accepted by `process_graphql_request`. Mirrors
/// `DraftProxyGraphQLRequestOptions` in TS. Use
/// `default_graphql_request_options()` for the empty value.
pub type GraphQLRequestOptions {
  GraphQLRequestOptions(
    /// Override the request path. Defaults to
    /// `/admin/api/<api_version>/graphql.json`.
    path: Option(String),
    /// Override the API version segment of the default path. Ignored if
    /// `path` is provided.
    api_version: Option(String),
    /// Headers to attach to the synthesized request.
    headers: Dict(String, String),
  )
}

/// Empty options for `process_graphql_request`. Equivalent to passing
/// `{}` to the TS `processGraphQLRequest`.
pub fn default_graphql_request_options() -> GraphQLRequestOptions {
  GraphQLRequestOptions(path: None, api_version: None, headers: dict.new())
}

/// Convenience wrapper that synthesizes a `POST` to the Admin GraphQL
/// path and dispatches it through `process_request`. Mirrors the TS
/// class's `processGraphQLRequest(body, options)`.
pub fn process_graphql_request(
  proxy: DraftProxy,
  body: String,
  options: GraphQLRequestOptions,
) -> #(Response, DraftProxy) {
  let path = case options.path {
    Some(p) -> p
    None ->
      default_graphql_path(option.unwrap(
        options.api_version,
        default_admin_api_version,
      ))
  }
  process_request(
    proxy,
    Request(method: "POST", path: path, headers: options.headers, body: body),
  )
}

@target(javascript)
/// Async JavaScript-target variant of `process_graphql_request`.
/// Keeps the default GraphQL route construction in Gleam while still
/// allowing live-hybrid passthrough requests to await upstream `fetch`.
pub fn process_graphql_request_async(
  proxy: DraftProxy,
  body: String,
  options: GraphQLRequestOptions,
) -> Promise(#(Response, DraftProxy)) {
  let path = case options.path {
    Some(p) -> p
    None ->
      default_graphql_path(option.unwrap(
        options.api_version,
        default_admin_api_version,
      ))
  }
  process_request_async(
    proxy,
    Request(method: "POST", path: path, headers: options.headers, body: body),
  )
}

/// Build the default `/admin/api/<version>/graphql.json` path. Mirrors
/// TS `defaultGraphQLPath`.
pub fn default_graphql_path(api_version: String) -> String {
  "/admin/api/" <> api_version <> "/graphql.json"
}

// ---------------------------------------------------------------------------
// State dump / restore
//
// Envelope shape mirrors the TS `DraftProxyStateDump`:
//   { schema, version, createdAt, store: {version, fields},
//     syntheticIdentity: {nextSyntheticId, nextSyntheticTimestamp},
//     extensions }
//
// The synthetic identity counters, base state, staged state, and mutation
// log round-trip in full for every store bucket currently ported in Gleam.
// ---------------------------------------------------------------------------

/// Reasons `restore_state` can refuse a dump.
pub type StateDumpError {
  /// The dump string failed to parse as JSON, or was missing required
  /// fields with the expected types.
  MalformedDumpJson(message: String)
  /// The `schema` field didn't match `state_dump_schema`.
  UnsupportedSchema(found: String)
  /// The envelope `version` field wasn't `state_dump_version`.
  UnsupportedVersion(found: Int)
  /// The inner `store.version` field wasn't `store_dump_version`.
  UnsupportedStoreVersion(found: Int)
  /// The synthetic identity portion failed validation. See
  /// `synthetic_identity.RestoreError` for details.
  InvalidSyntheticIdentity(synthetic_identity.RestoreError)
}

/// Snapshot all instance-owned runtime state to a JSON-compatible
/// envelope. Mirrors the TS `dumpState()`. `created_at` is taken as a
/// parameter so callers control whether the dump is deterministic;
/// `dump_state_now` is the wall-clock convenience equivalent to TS.
pub fn dump_state(proxy: DraftProxy, created_at: String) -> Json {
  json.object([
    #("schema", json.string(state_dump_schema)),
    #("version", json.int(state_dump_version)),
    #("createdAt", json.string(created_at)),
    #("store", dump_store_slice(proxy.store)),
    #("syntheticIdentity", dump_synthetic_identity(proxy.synthetic_identity)),
    #("extensions", json.object([])),
  ])
}

/// Same as `dump_state` but reads wall-clock time for `createdAt`.
/// Equivalent to TS `dumpState()`.
pub fn dump_state_now(proxy: DraftProxy) -> Json {
  dump_state(proxy, iso_timestamp.now_iso())
}

fn dump_store_slice(store: Store) -> Json {
  json.object([
    #("version", json.int(store_dump_version)),
    #(
      "fields",
      json.object([
        #(
          "baseState",
          dump_plain_field(state_serialization.serialize_base_state(
            store.base_state,
          )),
        ),
        #(
          "stagedState",
          dump_plain_field(state_serialization.serialize_staged_state(
            store.staged_state,
          )),
        ),
        #(
          "mutationLog",
          dump_plain_field(json.array(
            store.mutation_log,
            serialize_mutation_log_entry,
          )),
        ),
      ]),
    ),
  ])
}

fn dump_plain_field(value: Json) -> Json {
  json.object([
    #("kind", json.string("plain")),
    #("value", value),
  ])
}

fn dump_synthetic_identity(registry: SyntheticIdentityRegistry) -> Json {
  let dump = synthetic_identity.dump_state(registry)
  json.object([
    #("nextSyntheticId", json.int(dump.next_synthetic_id)),
    #("nextSyntheticTimestamp", json.string(dump.next_synthetic_timestamp)),
  ])
}

/// Rebuild a proxy from a dump produced by `dump_state`. The supplied
/// proxy provides the substrate (config, registry) the restored state
/// is grafted onto. Mirrors the TS `restoreState(dump)` but returns a
/// `Result` instead of throwing.
///
pub fn restore_state(
  proxy: DraftProxy,
  dump_json: String,
) -> Result(DraftProxy, StateDumpError) {
  let envelope_decoder = {
    use schema <- decode.field("schema", decode.string)
    use version <- decode.field("version", decode.int)
    use store_field <- decode.field("store", store_slice_decoder())
    use identity_field <- decode.field(
      "syntheticIdentity",
      synthetic_identity_dump_decoder(),
    )
    decode.success(#(schema, version, store_field, identity_field))
  }
  use parsed <- result.try(
    json.parse(dump_json, envelope_decoder)
    |> result.map_error(fn(err) {
      MalformedDumpJson(message: string.inspect(err))
    }),
  )
  let #(schema, version, store_dump, identity_dump) = parsed
  use _ <- result.try(case schema == state_dump_schema {
    True -> Ok(Nil)
    False -> Error(UnsupportedSchema(found: schema))
  })
  use _ <- result.try(case version == state_dump_version {
    True -> Ok(Nil)
    False -> Error(UnsupportedVersion(found: version))
  })
  let StoreSliceDump(
    version: store_version,
    base_state: base_state,
    staged_state: staged_state,
    mutation_log: log_entries,
  ) = store_dump
  use _ <- result.try(case store_version == store_dump_version {
    True -> Ok(Nil)
    False -> Error(UnsupportedStoreVersion(found: store_version))
  })
  use restored_identity <- result.try(
    synthetic_identity.restore_state(identity_dump)
    |> result.map_error(InvalidSyntheticIdentity),
  )
  let restored_store =
    restore_store_slice(base_state, staged_state, log_entries)
  Ok(
    DraftProxy(
      ..proxy,
      synthetic_identity: restored_identity,
      store: restored_store,
    ),
  )
}

/// Install a normalized snapshot JSON file into the proxy's base state.
/// Unknown state buckets are ignored so existing TypeScript snapshot files can
/// be consumed incrementally as the Gleam port learns new domains.
pub fn restore_snapshot(
  proxy: DraftProxy,
  snapshot_json: String,
) -> Result(DraftProxy, StateDumpError) {
  let snapshot_decoder = {
    use base_state <- decode.field(
      "baseState",
      state_serialization.base_state_decoder(),
    )
    decode.success(base_state)
  }
  use base_state <- result.try(
    json.parse(snapshot_json, snapshot_decoder)
    |> result.map_error(fn(err) {
      MalformedDumpJson(message: string.inspect(err))
    }),
  )
  Ok(
    DraftProxy(
      ..proxy,
      store: store.Store(
        base_state: base_state,
        staged_state: store.empty_staged_state(),
        mutation_log: [],
      ),
    ),
  )
}

type StoreSliceDump {
  StoreSliceDump(
    version: Int,
    base_state: store.BaseState,
    staged_state: store.StagedState,
    mutation_log: List(store.MutationLogEntry),
  )
}

fn store_slice_decoder() -> decode.Decoder(StoreSliceDump) {
  use version <- decode.field("version", decode.int)
  use fields <- decode.field("fields", store_fields_decoder())
  let StoreFieldsDump(
    base_state: base_state,
    staged_state: staged_state,
    mutation_log: mutation_log,
  ) = fields
  decode.success(StoreSliceDump(
    version: version,
    base_state: base_state,
    staged_state: staged_state,
    mutation_log: mutation_log,
  ))
}

type StoreFieldsDump {
  StoreFieldsDump(
    base_state: store.BaseState,
    staged_state: store.StagedState,
    mutation_log: List(store.MutationLogEntry),
  )
}

fn store_fields_decoder() -> decode.Decoder(StoreFieldsDump) {
  use base_state <- decode.field(
    "baseState",
    store_field_decoder(state_serialization.strict_base_state_decoder()),
  )
  use staged_state <- decode.field(
    "stagedState",
    store_field_decoder(state_serialization.strict_staged_state_decoder()),
  )
  use mutation_log <- decode.field(
    "mutationLog",
    store_field_decoder(decode.list(of: mutation_log_entry_decoder())),
  )
  decode.success(StoreFieldsDump(
    base_state: base_state,
    staged_state: staged_state,
    mutation_log: mutation_log,
  ))
}

fn store_field_decoder(inner: decode.Decoder(a)) -> decode.Decoder(a) {
  decode.one_of(
    {
      use kind <- decode.field("kind", decode.string)
      use value <- decode.field("value", inner)
      case kind {
        "plain" -> decode.success(value)
        _ -> decode.failure(value, "Unsupported store field dump kind")
      }
    },
    or: [inner],
  )
}

fn synthetic_identity_dump_decoder() -> decode.Decoder(
  synthetic_identity.SyntheticIdentityStateDumpV1,
) {
  use next_id <- decode.field("nextSyntheticId", decode.int)
  use next_ts <- decode.field("nextSyntheticTimestamp", decode.string)
  decode.success(synthetic_identity.SyntheticIdentityStateDumpV1(
    next_synthetic_id: next_id,
    next_synthetic_timestamp: next_ts,
  ))
}

fn mutation_log_entry_decoder() -> decode.Decoder(store.MutationLogEntry) {
  use id <- decode.field("id", decode.string)
  use received_at <- decode.field("receivedAt", decode.string)
  use operation_name <- decode.field(
    "operationName",
    decode.optional(decode.string),
  )
  use path <- decode.field("path", decode.string)
  use query <- decode.field("query", decode.string)
  use variables <- decode.optional_field(
    "variables",
    dict.new(),
    decode.dict(decode.string, decode.string),
  )
  use staged_resource_ids <- decode.optional_field(
    "stagedResourceIds",
    [],
    decode.list(of: decode.string),
  )
  use status <- decode.field("status", decode.string)
  use interpreted <- decode.field("interpreted", interpreted_metadata_decoder())
  use notes <- decode.field("notes", decode.optional(decode.string))
  decode.success(store.MutationLogEntry(
    id: id,
    received_at: received_at,
    operation_name: operation_name,
    path: path,
    query: query,
    variables: variables,
    staged_resource_ids: staged_resource_ids,
    status: parse_entry_status(status),
    interpreted: interpreted,
    notes: notes,
  ))
}

fn interpreted_metadata_decoder() -> decode.Decoder(store.InterpretedMetadata) {
  use op_type <- decode.field("operationType", decode.string)
  use op_name <- decode.field("operationName", decode.optional(decode.string))
  use root_fields <- decode.field("rootFields", decode.list(of: decode.string))
  use primary <- decode.field(
    "primaryRootField",
    decode.optional(decode.string),
  )
  use capability <- decode.field("capability", capability_decoder())
  decode.success(store.InterpretedMetadata(
    operation_type: parse_operation_type(op_type),
    operation_name: op_name,
    root_fields: root_fields,
    primary_root_field: primary,
    capability: capability,
  ))
}

fn capability_decoder() -> decode.Decoder(store.Capability) {
  use op_name <- decode.field("operationName", decode.optional(decode.string))
  use domain <- decode.field("domain", decode.string)
  use execution <- decode.field("execution", decode.string)
  decode.success(store.Capability(
    operation_name: op_name,
    domain: domain,
    execution: execution,
  ))
}

fn parse_entry_status(value: String) -> store.EntryStatus {
  case value {
    "staged" -> store.Staged
    "proxied" -> store.Proxied
    "committed" -> store.Committed
    _ -> store.Failed
  }
}

fn parse_operation_type(value: String) -> store.OperationType {
  case value {
    "mutation" -> store.Mutation
    _ -> store.Query
  }
}

fn restore_store_slice(
  base_state: store.BaseState,
  staged_state: store.StagedState,
  mutation_log: List(store.MutationLogEntry),
) -> Store {
  store.Store(
    base_state: base_state,
    staged_state: staged_state,
    mutation_log: mutation_log,
  )
}

// ---------------------------------------------------------------------------
// /__meta/commit dispatch
//
// The route implementation differs by target:
//   * Erlang   — `httpc.send/1` is synchronous, so the route handler can
//                drive `commit.run_commit_sync/4` directly from
//                `process_request/2`.
//   * JavaScript — `fetch` returns a `Promise`, so the synchronous route
//                  cannot resolve the upstream call. `process_request/2`
//                  surfaces a 501 pointing callers at
//                  `process_request_async/2`, which awaits the Promise.
// ---------------------------------------------------------------------------

@target(erlang)
fn dispatch_meta_commit_sync(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  commit_via_route(proxy, request)
}

@target(javascript)
fn dispatch_meta_commit_sync(
  proxy: DraftProxy,
  _request: Request,
) -> #(Response, DraftProxy) {
  #(commit_route_sync_unsupported_response(), proxy)
}

@target(erlang)
fn commit_via_route(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  let #(next_store, meta) =
    commit.run_commit_sync(
      proxy.store,
      proxy.config.shopify_admin_origin,
      request.headers,
      upstream_client.send_sync,
    )
  #(
    Response(
      status: 200,
      body: commit.serialize_meta_response(meta),
      headers: [],
    ),
    DraftProxy(..proxy, store: next_store),
  )
}

@target(erlang)
/// Run the upstream commit replay synchronously. Erlang-only — gleam_httpc
/// blocks until upstream answers, so this returns the response paired with
/// the next proxy state directly.
pub fn commit(
  proxy: DraftProxy,
  inbound_headers: Dict(String, String),
) -> #(Response, DraftProxy) {
  let #(next_store, meta) =
    commit.run_commit_sync(
      proxy.store,
      proxy.config.shopify_admin_origin,
      inbound_headers,
      upstream_client.send_sync,
    )
  #(
    Response(
      status: 200,
      body: commit.serialize_meta_response(meta),
      headers: [],
    ),
    DraftProxy(..proxy, store: next_store),
  )
}

@target(javascript)
/// Run the upstream commit replay asynchronously. JavaScript-only —
/// `fetch` is Promise-based, so callers must `await` the result. Returns
/// the same `#(Response, DraftProxy)` pair as the Erlang version once the
/// Promise resolves.
pub fn commit(
  proxy: DraftProxy,
  inbound_headers: Dict(String, String),
) -> Promise(#(Response, DraftProxy)) {
  commit.run_commit_async(
    proxy.store,
    proxy.config.shopify_admin_origin,
    inbound_headers,
    upstream_client.send_async,
  )
  |> promise.map(fn(pair) {
    let #(next_store, meta) = pair
    #(
      Response(
        status: 200,
        body: commit.serialize_meta_response(meta),
        headers: [],
      ),
      DraftProxy(..proxy, store: next_store),
    )
  })
}

@target(javascript)
/// Async dispatcher exposed only on JavaScript. Routes every request just
/// like `process_request/2`, but the `MetaCommit` arm awaits the upstream
/// fetch instead of returning a 501. Live-hybrid passthrough requests
/// also await an upstream `fetch`. Other routes are wrapped in
/// `promise.resolve` so callers can use a single async entry point.
pub fn process_request_async(
  proxy: DraftProxy,
  request: Request,
) -> Promise(#(Response, DraftProxy)) {
  case route(request) {
    MetaCommit -> commit(proxy, request.headers)
    GraphQL(version: _) ->
      case is_passthrough_request(proxy, request) {
        True -> dispatch_passthrough_async(proxy, request)
        False -> promise.resolve(process_request(proxy, request))
      }
    _ -> promise.resolve(process_request(proxy, request))
  }
}

@target(javascript)
fn is_passthrough_request(proxy: DraftProxy, request: Request) -> Bool {
  case parse_request_body(request.body) {
    Error(_) -> False
    Ok(body) ->
      case parse_operation.parse_operation(body.query) {
        Error(_) -> False
        Ok(parsed) -> live_hybrid_passthrough_target(proxy, parsed)
      }
  }
}

@target(javascript)
fn dispatch_passthrough_async(
  proxy: DraftProxy,
  request: Request,
) -> Promise(#(Response, DraftProxy)) {
  upstream_dispatch.fetch_async(
    proxy.config.shopify_admin_origin,
    request.path,
    request.body,
    request.headers,
    upstream_client.send_async,
  )
  |> promise.map(fn(outcome) {
    #(passthrough_outcome_to_response(outcome), proxy)
  })
}

@target(javascript)
fn commit_route_sync_unsupported_response() -> Response {
  Response(
    status: 501,
    body: json.object([
      #("ok", json.bool(False)),
      #(
        "message",
        json.string(
          "/__meta/commit requires async dispatch on the JavaScript target. Call process_request_async(proxy, request) or commit(proxy, headers) and await the returned Promise.",
        ),
      ),
    ]),
    headers: [],
  )
}

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
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Location, type Selection, Argument, Field,
  FragmentDefinition, FragmentSpread, InlineFragment, SelectionSet,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/parse_operation.{
  type GraphQLOperationType, type ParsedOperation, MutationOperation,
  QueryOperation,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/bulk_operations
import shopify_draft_proxy/proxy/capabilities
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers
import shopify_draft_proxy/proxy/delivery_settings
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/proxy/events
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/proxy/graphql_helpers.{type FragmentMap}
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/proxy/marketing
import shopify_draft_proxy/proxy/markets
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/mutation_schema_lookup
import shopify_draft_proxy/proxy/online_store
import shopify_draft_proxy/proxy/online_store/server_pixel_validation
import shopify_draft_proxy/proxy/operation_registry.{type RegistryEntry}
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/privacy
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{
  DraftProxy, Live, LiveHybrid, PassthroughUnsupportedMutations,
  RejectUnsupportedMutations, Request, Response, Snapshot,
}
import shopify_draft_proxy/proxy/saved_searches
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/proxy/shipping_fulfillments
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/proxy/webhooks
import shopify_draft_proxy/shopify/upstream_client.{type SyncTransport}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/serialization as state_serialization
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
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

/// Re-exports of the runtime types defined in `proxy_state`. The real
/// definitions live there so domain modules (`customers`, `products`,
/// …) can take `DraftProxy` / `Request` / `Response` as parameters
/// without importing `draft_proxy` and creating a cycle. External
/// callers keep using `draft_proxy.DraftProxy`, `draft_proxy.Request`,
/// `draft_proxy.Response`, `draft_proxy.Config`, and
/// `draft_proxy.ReadMode` as type names; for the value-level
/// constructors (`Request(..)`, `Response(..)`, `Config(..)`,
/// `Snapshot`, `LiveHybrid`, `Live`, `DraftProxy(..)`) tests should
/// import from `shopify_draft_proxy/proxy/proxy_state` directly.
pub type DraftProxy =
  proxy_state.DraftProxy

pub type Config =
  proxy_state.Config

pub type ReadMode =
  proxy_state.ReadMode

pub type Request =
  proxy_state.Request

pub type Response =
  proxy_state.Response

/// Default config, mirroring the values the TS test suite uses when no
/// explicit config is supplied.
pub fn default_config() -> Config {
  proxy_state.default_config()
}

/// Fresh proxy with default config. Equivalent to `new DraftProxy(...)`.
pub fn new() -> DraftProxy {
  proxy_state.new()
}

/// Fresh proxy with the supplied config.
pub fn with_config(config: Config) -> DraftProxy {
  proxy_state.with_config(config)
}

/// Install an injected upstream transport. Used by the parity runner
/// to wire a recorded cassette into the proxy; production callers
/// leave this unset.
pub fn with_upstream_transport(
  proxy: DraftProxy,
  transport: SyncTransport,
) -> DraftProxy {
  proxy_state.with_upstream_transport(proxy, transport)
}

/// Attach a parsed operation registry to the proxy. Once attached,
/// query/mutation dispatch routes by capability instead of the
/// hardcoded predicates.
pub fn with_registry(
  proxy: DraftProxy,
  registry: List(RegistryEntry),
) -> DraftProxy {
  proxy_state.with_registry(proxy, registry)
}

/// Attach the vendored default registry.
pub fn with_default_registry(proxy: DraftProxy) -> DraftProxy {
  proxy_state.with_default_registry(proxy)
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
        #(
          "unsupportedMutationMode",
          json.string(unsupported_mutation_mode_to_string(
            proxy.config.unsupported_mutation_mode,
          )),
        ),
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
    Live -> "passthrough"
  }
}

fn unsupported_mutation_mode_to_string(
  mode: proxy_state.UnsupportedMutationMode,
) -> String {
  case mode {
    PassthroughUnsupportedMutations -> "passthrough"
    RejectUnsupportedMutations -> "reject"
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

/// Store staged-upload bytes under a caller-supplied lookup key.
///
/// The JavaScript HTTP adapter owns URL decoding/route matching so this core
/// helper can stay explicit and instance-scoped.
pub fn stage_staged_upload_content(
  proxy: DraftProxy,
  staged_upload_path: String,
  content: String,
) -> DraftProxy {
  DraftProxy(
    ..proxy,
    store: store.stage_staged_upload_content(
      proxy.store,
      staged_upload_path,
      content,
    ),
  )
}

pub fn get_bulk_operation_result_jsonl(proxy: DraftProxy, id: String) -> Json {
  case store.get_effective_bulk_operation_result_jsonl(proxy.store, id) {
    Some(jsonl) -> json.string(jsonl)
    None -> json.null()
  }
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
          #(k, root_field.resolved_value_to_json(v))
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
    store_types.Query -> "query"
    store_types.Mutation -> "mutation"
  }
}

fn entry_status_to_string(status: store.EntryStatus) -> String {
  case status {
    store_types.Staged -> "staged"
    store_types.Proxied -> "proxied"
    store_types.Committed -> "committed"
    store_types.Failed -> "failed"
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
  error_response(404, "Not found")
}

fn method_not_allowed_response() -> Response {
  error_response(405, "Method not allowed")
}

/// Build a Shopify-style `{"errors": [...]}` envelope from a list of
/// already-built error JSON objects.
fn error_envelope(errors: List(Json)) -> Json {
  json.object([#("errors", json.preprocessed_array(errors))])
}

/// Single-message error response with the given HTTP status.
fn error_response(status: Int, message: String) -> Response {
  Response(
    status: status,
    body: error_envelope([json.object([#("message", json.string(message))])]),
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
          case
            reject_unsupported_mutation_target(proxy, parsed, body.variables)
          {
            True -> #(unsupported_mutation_rejected_response(parsed), proxy)
            False ->
              case
                live_hybrid_passthrough_target(proxy, parsed, body.variables)
              {
                True -> dispatch_passthrough_sync(proxy, request)
                False ->
                  case parsed.type_, list.first(parsed.root_fields) {
                    QueryOperation, Ok(field) ->
                      route_query(
                        proxy,
                        request,
                        parsed,
                        body.query,
                        field,
                        body.variables,
                      )
                    MutationOperation, Ok(field) ->
                      route_mutation(
                        proxy,
                        parsed,
                        request.path,
                        request.headers,
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
}

/// Substrate-level passthrough check. Returns `True` only for the
/// dispatcher-irreducible cases: the proxy is in `LiveHybrid` mode AND
/// either the operation maps to the `Passthrough` execution branch
/// (registry says: unimplemented), or no local dispatcher claims the
/// root field at all.
///
/// Per-operation "forward upstream because the local handler doesn't
/// have enough state to answer correctly" decisions live in the domain
/// modules now (`customers.handle_query_request`,
/// `discounts.handle_query_request`, …) — they call
/// `passthrough.passthrough_sync` themselves. The dispatcher only
/// passthroughs when there is no handler to ask.
fn live_hybrid_passthrough_target(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  _variables: Dict(String, root_field.ResolvedValue),
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

fn reject_unsupported_mutation_target(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case proxy.config.unsupported_mutation_mode, parsed.type_ {
    RejectUnsupportedMutations, MutationOperation ->
      live_hybrid_passthrough_target(proxy, parsed, variables)
    _, _ -> False
  }
}

fn unsupported_mutation_rejected_response(parsed: ParsedOperation) -> Response {
  let root = case list.first(parsed.root_fields) {
    Ok(name) -> name
    Error(_) -> "unknown"
  }
  bad_request("Unsupported mutation rejected by configuration: " <> root)
}

fn dispatch_passthrough_sync(
  proxy: DraftProxy,
  request: Request,
) -> #(Response, DraftProxy) {
  let #(response, next_proxy) = passthrough.passthrough_sync(proxy, request)
  case passthrough_reached_upstream(response) {
    True -> #(response, record_proxied_mutation_if_needed(next_proxy, request))
    False -> #(response, next_proxy)
  }
}

@target(erlang)
fn passthrough_reached_upstream(_response: Response) -> Bool {
  True
}

@target(javascript)
fn passthrough_reached_upstream(response: Response) -> Bool {
  !passthrough.response_is_async_unsupported(response)
}

fn record_proxied_mutation_if_needed(
  proxy: DraftProxy,
  request: Request,
) -> DraftProxy {
  case parse_request_body(request.body) {
    Error(_) -> proxy
    Ok(body) ->
      case parse_operation.parse_operation(body.query) {
        Ok(parsed) ->
          case parsed.type_ {
            MutationOperation ->
              record_proxied_mutation(proxy, request.path, parsed, body)
            QueryOperation -> proxy
          }
        Error(_) -> proxy
      }
  }
}

fn record_proxied_mutation(
  proxy: DraftProxy,
  request_path: String,
  parsed: ParsedOperation,
  body: ParsedBody,
) -> DraftProxy {
  let cap = capabilities.get_operation_capability(parsed, proxy.registry)
  let #(domain, execution) = passthrough_capability_strings(proxy, parsed, cap)
  let draft =
    mutation_helpers.LogDraft(
      operation_name: cap.operation_name,
      root_fields: parsed.root_fields,
      primary_root_field: list.first(parsed.root_fields) |> option.from_result,
      domain: domain,
      execution: execution,
      query: Some(body.query),
      variables: Some(body.variables),
      staged_resource_ids: [],
      status: store_types.Proxied,
      notes: Some(
        "Mutation passthrough placeholder until supported local staging is implemented.",
      ),
    )
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      proxy.store,
      proxy.synthetic_identity,
      request_path,
      body.query,
      body.variables,
      [draft],
    )
  DraftProxy(..proxy, store: logged_store, synthetic_identity: logged_identity)
}

fn passthrough_capability_strings(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  cap: capabilities.OperationCapability,
) -> #(String, String) {
  let candidates =
    list.append(list.map(parsed.root_fields, Some), case parsed.name {
      Some(_) -> [parsed.name]
      None -> []
    })
  case
    operation_registry.find_entry(
      proxy.registry,
      operation_registry.Mutation,
      candidates,
    )
  {
    Some(entry) -> #(
      operation_registry.domain_to_string(entry.domain),
      operation_registry.execution_to_string(entry.execution),
    )
    None -> #(
      operation_registry.domain_to_string(cap.domain),
      operation_registry.execution_to_string(cap.execution),
    )
  }
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
  passthrough.passthrough_with_send(proxy, request, send)
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
  passthrough.passthrough_with_send_async(proxy, request, send)
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
  variables: Dict(String, root_field.ResolvedValue),
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
      variables,
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
  request_headers: Dict(String, String),
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  // Schema-driven required-field validation runs once, here, before
  // any domain handler executes. Mirrors how a real GraphQL server
  // validates a request against its schema before invoking
  // resolvers: any missing NON_NULL argument or required input-object
  // attribute is rejected up front with the same `errors` envelope
  // real Shopify produces — no domain handler sees the request.
  case primary_root_field {
    "productFeedCreate" ->
      route_mutation_to_domain(
        proxy,
        parsed,
        request_path,
        request_headers,
        query,
        primary_root_field,
        variables,
      )
    _ ->
      case schema_validation_errors(parsed, query, variables, request_headers) {
        [] ->
          route_mutation_to_domain(
            proxy,
            parsed,
            request_path,
            request_headers,
            query,
            primary_root_field,
            variables,
          )
        errors -> #(schema_validation_error_response(errors), proxy)
      }
  }
}

fn schema_validation_error_response(errors: List(Json)) -> Response {
  Response(status: 200, body: error_envelope(errors), headers: [])
}

fn schema_validation_errors(
  parsed: ParsedOperation,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
  request_headers: Dict(String, String),
) -> List(Json) {
  case root_field.get_root_fields(query) {
    Error(_) -> []
    Ok(fields) -> {
      let schema = mutation_schema_lookup.default_schema()
      let fragments = graphql_helpers.get_document_fragments(query)
      let operation_path = case parsed.name {
        Some(name) -> "mutation " <> name
        None -> "mutation"
      }
      list.flat_map(fields, fn(field) {
        case field {
          Field(name: name, ..) -> {
            let schema_errors =
              mutation_helpers.validate_mutation_field_against_schema(
                field,
                variables,
                name.value,
                operation_path,
                query,
                schema,
              )
            schema_errors
            |> list.append(metaobject_upsert_payload_one_of_errors(
              field,
              variables,
              operation_path,
              query,
            ))
            |> list.append(metaobject_argument_visibility_errors(
              field,
              request_headers,
              operation_path,
              query,
            ))
            |> list.append(server_pixel_endpoint_argument_errors(
              field,
              variables,
              operation_path,
              query,
            ))
            |> list.append(payment_reminder_send_selection_errors(
              field,
              operation_path,
              query,
              fragments,
            ))
            |> list.append(staged_uploads_create_user_error_selection_errors(
              field,
              operation_path,
              query,
              fragments,
            ))
            |> list.append(staged_upload_resource_enum_errors(
              name.value,
              variables,
            ))
          }
          _ -> []
        }
      })
    }
  }
}

fn payment_reminder_send_selection_errors(
  field: Selection,
  operation_path: String,
  source_body: String,
  fragments: FragmentMap,
) -> List(Json) {
  case field {
    Field(name: name, ..) if name.value == "paymentReminderSend" ->
      collect_payload_field_selections(field, fragments)
      |> list.filter_map(fn(selected) {
        let #(field_name, loc) = selected
        case payment_reminder_payload_field_allowed(field_name) {
          True -> Error(Nil)
          False ->
            Ok(build_undefined_payment_reminder_payload_field_error(
              field_name,
              loc,
              operation_path,
              source_body,
            ))
        }
      })
    _ -> []
  }
}

fn server_pixel_endpoint_argument_errors(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  case field {
    Field(name: name, arguments: arguments, loc: field_loc, ..) -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(args) -> args
        Error(_) -> dict.new()
      }
      case name.value {
        "eventBridgeServerPixelUpdate" ->
          case dict.get(args, "arn") {
            Ok(root_field.StringVal(arn)) ->
              case server_pixel_validation.valid_eventbridge_arn(arn) {
                True -> []
                False -> [
                  build_eventbridge_server_pixel_arn_error(
                    arn,
                    operation_path,
                    field_loc,
                    source_body,
                  ),
                ]
              }
            _ -> []
          }
        "pubSubServerPixelUpdate" ->
          list.flat_map(["pubSubProject", "pubSubTopic"], fn(argument_name) {
            case dict.get(args, argument_name) {
              Ok(root_field.StringVal(value)) ->
                case server_pixel_validation.non_blank(value) {
                  True -> []
                  False -> [
                    build_pubsub_server_pixel_blank_error(
                      argument_name,
                      field_loc,
                      argument_location(arguments, argument_name),
                      source_body,
                    ),
                  ]
                }
              _ -> []
            }
          })
        _ -> []
      }
    }
    _ -> []
  }
}

fn metaobject_argument_visibility_errors(
  field: Selection,
  request_headers: Dict(String, String),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  case field {
    Field(name: name, arguments: arguments, loc: field_loc, ..) ->
      case name.value {
        "metaobjectDefinitionUpdate" ->
          case argument_location(arguments, "resetFieldOrder") {
            Some(argument_loc) -> [
              build_top_level_argument_not_accepted_error(
                name.value,
                "resetFieldOrder",
                operation_path,
                field_loc,
                Some(argument_loc),
                source_body,
              ),
            ]
            None -> []
          }
        "standardMetaobjectDefinitionEnable" ->
          case
            argument_location(arguments, "enabledByShopify"),
            app_identity.has_internal_visibility(request_headers)
          {
            Some(argument_loc), False -> [
              build_top_level_argument_not_accepted_error(
                name.value,
                "enabledByShopify",
                operation_path,
                field_loc,
                Some(argument_loc),
                source_body,
              ),
            ]
            _, _ -> []
          }
        _ -> []
      }
    _ -> []
  }
}

fn argument_location(
  arguments: List(Argument),
  argument_name: String,
) -> Option(Location) {
  case mutation_helpers.find_argument(arguments, argument_name) {
    Some(argument) -> {
      let Argument(loc: loc, ..) = argument
      loc
    }
    None -> None
  }
}

fn build_top_level_argument_not_accepted_error(
  field_name: String,
  argument_name: String,
  operation_path: String,
  _field_loc: Option(Location),
  argument_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '"
        <> field_name
        <> "' doesn't accept argument '"
        <> argument_name
        <> "'",
      ),
    ),
  ]
  let with_locations = case location_object(argument_loc, source_body) {
    Some(location) ->
      list.append(base, [
        #("locations", json.preprocessed_array([location])),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array([operation_path, field_name, argument_name], json.string),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentNotAccepted")),
          #("name", json.string(field_name)),
          #("typeName", json.string("Field")),
          #("argumentName", json.string(argument_name)),
        ]),
      ),
    ]),
  )
}

fn build_eventbridge_server_pixel_arn_error(
  arn: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [#("message", json.string("Invalid ARN '" <> arn <> "'"))]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, "eventBridgeServerPixelUpdate", "arn"],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("CoercionError")),
        ]),
      ),
    ]),
  )
}

fn build_pubsub_server_pixel_blank_error(
  argument_name: String,
  field_loc: Option(Location),
  argument_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #("message", json.string(argument_name <> " can't be blank")),
  ]
  let with_locations =
    [field_loc, argument_loc]
    |> list.filter_map(fn(loc) {
      case location_object(loc, source_body) {
        Some(location) -> Ok(location)
        None -> Error(Nil)
      }
    })
  let pairs = case with_locations {
    [] -> base
    _ ->
      list.append(base, [
        #("locations", json.preprocessed_array(with_locations)),
      ])
  }
  json.object(
    list.append(pairs, [
      #(
        "extensions",
        json.object([#("code", json.string("INVALID_FIELD_ARGUMENTS"))]),
      ),
      #("path", json.array(["pubSubServerPixelUpdate"], json.string)),
    ]),
  )
}

fn payment_reminder_payload_field_allowed(field_name: String) -> Bool {
  field_name == "success"
  || field_name == "userErrors"
  || field_name == "__typename"
}

fn staged_uploads_create_user_error_selection_errors(
  field: Selection,
  operation_path: String,
  source_body: String,
  fragments: FragmentMap,
) -> List(Json) {
  case field {
    Field(name: name, ..) if name.value == "stagedUploadsCreate" ->
      collect_named_child_field_selections(field, fragments, "userErrors")
      |> list.filter_map(fn(selected) {
        let #(field_name, loc) = selected
        case field_name {
          "code" ->
            Ok(build_undefined_staged_upload_user_error_field_error(
              field_name,
              loc,
              operation_path,
              source_body,
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn collect_payload_field_selections(
  field: Selection,
  fragments: FragmentMap,
) -> List(#(String, Option(Location))) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      collect_selection_field_names(selections, fragments)
    _ -> []
  }
}

fn collect_named_child_field_selections(
  field: Selection,
  fragments: FragmentMap,
  child_name: String,
) -> List(#(String, Option(Location))) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      collect_named_child_fields_from_selections(
        selections,
        fragments,
        child_name,
      )
    _ -> []
  }
}

fn collect_named_child_fields_from_selections(
  selections: List(Selection),
  fragments: FragmentMap,
  child_name: String,
) -> List(#(String, Option(Location))) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(
        name: name,
        selection_set: Some(SelectionSet(selections: inner, ..)),
        ..,
      )
        if name.value == child_name
      -> collect_selection_field_names(inner, fragments)
      Field(..) -> []
      InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        collect_named_child_fields_from_selections(inner, fragments, child_name)
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            collect_named_child_fields_from_selections(
              inner,
              fragments,
              child_name,
            )
          _ -> []
        }
    }
  })
}

fn collect_selection_field_names(
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(#(String, Option(Location))) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(name: name, loc: loc, ..) -> [#(name.value, loc)]
      InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        collect_selection_field_names(inner, fragments)
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) -> collect_selection_field_names(inner, fragments)
          _ -> []
        }
    }
  })
}

fn build_undefined_payment_reminder_payload_field_error(
  field_name: String,
  field_loc: Option(Location),
  operation_path: String,
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '"
        <> field_name
        <> "' doesn't exist on type 'PaymentReminderSendPayload'",
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, "paymentReminderSend", field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("undefinedField")),
          #("typeName", json.string("PaymentReminderSendPayload")),
          #("fieldName", json.string(field_name)),
        ]),
      ),
    ]),
  )
}

fn build_undefined_staged_upload_user_error_field_error(
  field_name: String,
  field_loc: Option(Location),
  operation_path: String,
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '" <> field_name <> "' doesn't exist on type 'UserError'",
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, "stagedUploadsCreate", "userErrors", field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("undefinedField")),
          #("typeName", json.string("UserError")),
          #("fieldName", json.string(field_name)),
        ]),
      ),
    ]),
  )
}

fn locations_payload(
  field_loc: Option(Location),
  source_body: String,
) -> Option(Json) {
  case location_object(field_loc, source_body) {
    Some(location) -> Some(json.preprocessed_array([location]))
    None -> None
  }
}

fn location_object(
  field_loc: Option(Location),
  source_body: String,
) -> Option(Json) {
  case field_loc {
    None -> None
    Some(loc) -> {
      let source = graphql_source.new(source_body)
      let computed = graphql_location.get_location(source, position: loc.start)
      Some(
        json.object([
          #("line", json.int(computed.line)),
          #("column", json.int(computed.column)),
        ]),
      )
    }
  }
}

fn metaobject_upsert_payload_one_of_errors(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_path: String,
  query: String,
) -> List(Json) {
  case field {
    Field(name: name, ..) if name.value == "metaobjectUpsert" -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(args) -> args
        Error(_) -> dict.new()
      }
      case
        has_present_argument(args, "metaobject")
        || has_present_argument(args, "values")
      {
        True -> []
        False -> [
          mutation_helpers.build_missing_required_argument_error(
            "metaobjectUpsert",
            "metaobject, values",
            operation_path,
            None,
            query,
          ),
        ]
      }
    }
    _ -> []
  }
}

fn has_present_argument(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(args, name) {
    Ok(root_field.NullVal) | Error(_) -> False
    Ok(_) -> True
  }
}

const staged_upload_resource_enum_values: List(String) = [
  "COLLECTION_IMAGE",
  "FILE",
  "IMAGE",
  "MODEL_3D",
  "PRODUCT_IMAGE",
  "SHOP_IMAGE",
  "VIDEO",
  "BULK_MUTATION_VARIABLES",
  "RETURN_LABEL",
  "URL_REDIRECT_IMPORT",
  "DISPUTE_FILE_UPLOAD",
]

fn staged_upload_resource_enum_errors(
  root_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  case root_name, dict.get(variables, "input") {
    "stagedUploadsCreate", Ok(root_field.ListVal(items)) ->
      items
      |> list.index_map(fn(item, index) {
        case item {
          root_field.ObjectVal(fields) ->
            case dict.get(fields, "resource") {
              Ok(root_field.StringVal(resource)) ->
                case
                  list.contains(staged_upload_resource_enum_values, resource)
                {
                  True -> []
                  False -> [
                    staged_upload_invalid_resource_variable_error(
                      variables_value: root_field.ListVal(items),
                      index:,
                      resource:,
                    ),
                  ]
                }
              _ -> []
            }
          _ -> []
        }
      })
      |> list.flatten
    _, _ -> []
  }
}

fn staged_upload_invalid_resource_variable_error(
  variables_value variables_value: root_field.ResolvedValue,
  index index: Int,
  resource resource: String,
) -> Json {
  let explanation =
    "Expected \""
    <> resource
    <> "\" to be one of: "
    <> string.join(staged_upload_resource_enum_values, ", ")
  json.object([
    #(
      "message",
      json.string(
        "Variable $input of type [StagedUploadInput!]! was provided invalid value for "
        <> int.to_string(index)
        <> ".resource ("
        <> explanation
        <> ")",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", root_field.resolved_value_to_json(variables_value)),
        #(
          "problems",
          json.preprocessed_array([
            json.object([
              #(
                "path",
                json.preprocessed_array([
                  json.int(index),
                  json.string("resource"),
                ]),
              ),
              #("explanation", json.string(explanation)),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

fn route_mutation_to_domain(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  request_path: String,
  request_headers: Dict(String, String),
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let upstream =
    upstream_query.UpstreamContext(
      transport: proxy.upstream_transport,
      origin: proxy.config.shopify_admin_origin,
      headers: request_headers,
      allow_upstream_reads: proxy.config.read_mode == LiveHybrid,
    )

  case mutation_handler_for(proxy, parsed, query, primary_root_field) {
    Some(handler) -> {
      let outcome =
        handler(
          proxy.store,
          proxy.synthetic_identity,
          request_path,
          query,
          variables,
          upstream,
        )
      finalize_mutation_outcome(
        proxy,
        request_path,
        query,
        variables,
        outcome.data,
        outcome.store,
        outcome.identity,
        outcome.log_drafts,
      )
    }
    None -> #(
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
  request: Request,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case query_handler_for(proxy, parsed, query, primary_root_field) {
    Some(handler) ->
      handler(proxy, request, parsed, primary_root_field, query, variables)
    None -> #(
      bad_request(
        "No domain dispatcher implemented for root field: "
        <> primary_root_field,
      ),
      proxy,
    )
  }
}

/// Closure invoked for a query whose domain has been resolved. Every
/// domain module's `handle_query_request` already matches this shape,
/// so the dispatch table just hands the function back directly.
type QueryHandler =
  fn(
    DraftProxy,
    Request,
    ParsedOperation,
    String,
    String,
    Dict(String, root_field.ResolvedValue),
  ) -> #(Response, DraftProxy)

/// Closure invoked for a mutation whose domain has been resolved.
/// Every domain module's `process_mutation` already matches this shape,
/// so the dispatch table just hands the function back directly.
type MutationHandler =
  fn(
    Store,
    SyntheticIdentityRegistry,
    String,
    String,
    Dict(String, root_field.ResolvedValue),
    upstream_query.UpstreamContext,
  ) -> mutation_helpers.MutationOutcome

/// Resolve a query operation's handler. The registry decides whether
/// a known root is implemented at all; the local dispatch table
/// decides whether this Gleam port can actually handle that root
/// today.
fn query_handler_for(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
) -> Option(QueryHandler) {
  case parsed.type_ {
    QueryOperation ->
      case
        operation_registry.find_entry(proxy.registry, operation_registry.Query, [
          Some(primary_root_field),
        ])
      {
        Some(entry) ->
          case entry.implemented {
            True -> local_query_handler(primary_root_field, query)
            False -> None
          }
        None -> local_query_handler(primary_root_field, query)
      }
    _ -> None
  }
}

fn mutation_handler_for(
  proxy: DraftProxy,
  parsed: ParsedOperation,
  query: String,
  primary_root_field: String,
) -> Option(MutationHandler) {
  case parsed.type_ {
    MutationOperation ->
      case
        operation_registry.find_entry(
          proxy.registry,
          operation_registry.Mutation,
          [Some(primary_root_field)],
        )
      {
        Some(entry) ->
          case entry.implemented {
            True -> local_mutation_handler(primary_root_field, query)
            False -> None
          }
        None -> local_mutation_handler(primary_root_field, query)
      }
    _ -> None
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
    QueryOperation -> option.is_some(local_query_handler(name, ""))
    MutationOperation -> option.is_some(local_mutation_handler(name, ""))
  }
}

fn local_registry_dispatch_supported(
  type_: operation_registry.OperationType,
  name: String,
) -> Bool {
  case type_ {
    operation_registry.Query -> option.is_some(local_query_handler(name, ""))
    operation_registry.Mutation ->
      option.is_some(local_mutation_handler(name, ""))
  }
}

fn local_query_handler(name: String, query: String) -> Option(QueryHandler) {
  case name {
    "event" | "events" | "eventsCount" -> Some(events.handle_query_request)
    "deliverySettings" | "deliveryPromiseSettings" ->
      Some(delivery_settings.handle_query_request)
    "shop" ->
      case online_store.is_online_store_query_root(name, query) {
        True -> Some(online_store.handle_query_request)
        False -> Some(store_properties.handle_query_request)
      }
    "order" ->
      case shipping_fulfillment_order_lifecycle_query(query) {
        True -> Some(shipping_fulfillments.handle_query_request)
        False -> Some(orders.handle_query_request)
      }
    "draftOrder" ->
      case draft_order_payment_terms_only_query(query) {
        True -> Some(payments.handle_query_request)
        False -> Some(orders.handle_query_request)
      }
    "customer" ->
      case customer_payment_methods_only_query(query) {
        True -> Some(payments.handle_query_request)
        False -> Some(customers.handle_query_request)
      }
    "market"
    | "markets"
    | "catalog"
    | "catalogs"
    | "catalogsCount"
    | "priceList"
    | "priceLists"
    | "webPresences"
    | "marketsResolvedValues"
    | "marketLocalizableResource"
    | "marketLocalizableResources"
    | "marketLocalizableResourcesByIds" -> Some(markets.handle_query_request)
    "product" | "collection" ->
      case store_publishable_owner_query(name, query) {
        True -> Some(store_properties.handle_query_request)
        False -> Some(products.handle_query_request)
      }
    _ ->
      first_matching_handler([
        #(payments.is_payments_query_root(name), payments.handle_query_request),
        #(
          saved_searches.is_saved_search_query_root(name),
          saved_searches.handle_query_request,
        ),
        #(
          webhooks.is_webhook_subscription_query_root(name),
          webhooks.handle_query_request,
        ),
        #(apps.is_app_query_root(name), apps.handle_query_request),
        #(
          functions.is_function_query_root(name),
          functions.handle_query_request,
        ),
        #(
          gift_cards.is_gift_card_query_root(name),
          gift_cards.handle_query_request,
        ),
        #(
          discounts.is_discount_query_root(name),
          discounts.handle_query_request,
        ),
        #(b2b.is_b2b_query_root(name), b2b.handle_query_request),
        #(segments.is_segment_query_root(name), segments.handle_query_request),
        #(products.is_products_query_root(name), products.handle_query_request),
        #(
          customers.is_customer_query_root(name),
          customers.handle_query_request,
        ),
        #(
          shipping_fulfillment_priority_query_root(name),
          shipping_fulfillments.handle_query_request,
        ),
        #(orders.is_orders_query_root(name), orders.handle_query_request),
        #(
          metafield_definitions.is_metafield_definitions_query_root(name),
          metafield_definitions.handle_query_request,
        ),
        #(
          localization.is_localization_query_root(name),
          localization.handle_query_request,
        ),
        #(
          metaobject_definitions.is_metaobject_definitions_query_root(name),
          metaobject_definitions.handle_query_request,
        ),
        #(
          marketing.is_marketing_query_root(name),
          marketing.handle_query_request,
        ),
        #(
          bulk_operations.is_bulk_operations_query_root(name),
          bulk_operations.handle_query_request,
        ),
        #(media.is_media_query_root(name), media.handle_query_request),
        #(
          admin_platform.is_admin_platform_query_root(name),
          admin_platform.handle_query_request,
        ),
        #(
          store_properties.is_store_properties_query_root(name),
          store_properties.handle_query_request,
        ),
        #(
          online_store.is_online_store_query_root(name, query),
          online_store.handle_query_request,
        ),
        #(
          shipping_fulfillments.is_shipping_fulfillment_query_root(name),
          shipping_fulfillments.handle_query_request,
        ),
      ])
  }
}

fn draft_order_payment_terms_only_query(query: String) -> Bool {
  case root_field.get_root_fields(query) {
    Error(_) -> False
    Ok(fields) ->
      fields
      |> list.any(fn(field) {
        case field {
          Field(name: field_name, ..) if field_name.value == "draftOrder" -> {
            let selection_names = root_field.get_selection_names(field)
            !list.is_empty(selection_names)
            && list.all(selection_names, fn(name) {
              name == "id" || name == "paymentTerms" || name == "__typename"
            })
          }
          _ -> False
        }
      })
  }
}

fn customer_payment_methods_only_query(query: String) -> Bool {
  case root_field.get_root_fields(query) {
    Error(_) -> False
    Ok(fields) ->
      fields
      |> list.any(fn(field) {
        case field {
          Field(name: field_name, ..) if field_name.value == "customer" -> {
            let selection_names = root_field.get_selection_names(field)
            !list.is_empty(selection_names)
            && list.all(selection_names, fn(name) {
              name == "id" || name == "paymentMethods" || name == "__typename"
            })
          }
          _ -> False
        }
      })
  }
}

fn first_matching_handler(
  candidates: List(#(Bool, handler)),
) -> Option(handler) {
  case candidates {
    [] -> None
    [#(True, handler), ..] -> Some(handler)
    [_, ..rest] -> first_matching_handler(rest)
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

fn shipping_fulfillment_priority_query_root(name: String) -> Bool {
  case name {
    "deliveryProfile"
    | "deliveryProfiles"
    | "fulfillment"
    | "fulfillmentOrder"
    | "fulfillmentOrders"
    | "assignedFulfillmentOrders"
    | "manualHoldsFulfillmentOrders" -> True
    _ -> False
  }
}

fn shipping_fulfillment_order_lifecycle_query(query: String) -> Bool {
  string.contains(query, "fulfillmentHolds")
  || string.contains(query, "fulfillBy")
  || string.contains(query, "supportedActions")
}

fn local_mutation_handler(
  name: String,
  query: String,
) -> Option(MutationHandler) {
  case publishable_mutation_requests_store_properties(name, query) {
    True -> Some(store_properties.process_mutation)
    False -> local_non_store_publishable_mutation_handler(name)
  }
}

fn local_non_store_publishable_mutation_handler(
  name: String,
) -> Option(MutationHandler) {
  first_matching_handler([
    #(payments.is_payments_mutation_root(name), payments.process_mutation),
    #(products.is_products_mutation_root(name), products.process_mutation),
    #(
      store_properties.is_store_properties_mutation_root(name),
      store_properties.process_mutation,
    ),
    #(
      saved_searches.is_saved_search_mutation_root(name),
      saved_searches.process_mutation,
    ),
    #(
      webhooks.is_webhook_subscription_mutation_root(name),
      webhooks.process_mutation,
    ),
    #(apps.is_app_mutation_root(name), apps.process_mutation),
    #(functions.is_function_mutation_root(name), functions.process_mutation),
    #(gift_cards.is_gift_card_mutation_root(name), gift_cards.process_mutation),
    #(discounts.is_discount_mutation_root(name), discounts.process_mutation),
    #(b2b.is_b2b_mutation_root(name), b2b.process_mutation),
    #(segments.is_segment_mutation_root(name), segments.process_mutation),
    #(
      metafield_definitions.is_metafield_definitions_mutation_root(name),
      metafield_definitions.process_mutation,
    ),
    #(
      localization.is_localization_mutation_root(name),
      localization.process_mutation,
    ),
    #(
      metaobject_definitions.is_metaobject_definitions_mutation_root(name),
      metaobject_definitions.process_mutation,
    ),
    #(marketing.is_marketing_mutation_root(name), marketing.process_mutation),
    #(
      bulk_operations.is_bulk_operations_mutation_root(name),
      bulk_operations.process_mutation,
    ),
    #(media.is_media_mutation_root(name), media.process_mutation),
    #(markets.is_markets_mutation_root(name), markets.process_mutation),
    #(
      admin_platform.is_admin_platform_mutation_root(name),
      admin_platform.process_mutation,
    ),
    #(
      online_store.is_online_store_mutation_root(name),
      online_store.process_mutation,
    ),
    #(privacy.is_privacy_mutation_root(name), privacy.process_mutation),
    #(
      shipping_fulfillment_priority_mutation_root(name),
      shipping_fulfillments.process_mutation,
    ),
    #(orders.is_orders_mutation_root(name), orders.process_mutation),
    #(customers.is_customer_mutation_root(name), customers.process_mutation),
    #(
      shipping_fulfillments.is_shipping_fulfillment_mutation_root(name),
      shipping_fulfillments.process_mutation,
    ),
  ])
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

fn shipping_fulfillment_priority_mutation_root(name: String) -> Bool {
  case name {
    "fulfillmentEventCreate"
    | "fulfillmentOrderSubmitFulfillmentRequest"
    | "fulfillmentOrderAcceptFulfillmentRequest"
    | "fulfillmentOrderRejectFulfillmentRequest"
    | "fulfillmentOrderSubmitCancellationRequest"
    | "fulfillmentOrderAcceptCancellationRequest"
    | "fulfillmentOrderRejectCancellationRequest"
    | "fulfillmentOrderHold"
    | "fulfillmentOrderReleaseHold"
    | "fulfillmentOrderMove"
    | "fulfillmentOrderReschedule"
    | "fulfillmentOrderReportProgress"
    | "fulfillmentOrderOpen"
    | "fulfillmentOrderClose"
    | "fulfillmentOrderCancel"
    | "fulfillmentOrderSplit"
    | "fulfillmentOrdersSetFulfillmentDeadline"
    | "fulfillmentOrderMerge" -> True
    _ -> False
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
  error_response(400, message)
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
      store: store_types.Store(
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
    decode.dict(decode.string, root_field.resolved_value_decoder()),
  )
  use staged_resource_ids <- decode.optional_field(
    "stagedResourceIds",
    [],
    decode.list(of: decode.string),
  )
  use status <- decode.field("status", decode.string)
  use interpreted <- decode.field("interpreted", interpreted_metadata_decoder())
  use notes <- decode.field("notes", decode.optional(decode.string))
  decode.success(store_types.MutationLogEntry(
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
  decode.success(store_types.InterpretedMetadata(
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
  decode.success(store_types.Capability(
    operation_name: op_name,
    domain: domain,
    execution: execution,
  ))
}

fn parse_entry_status(value: String) -> store.EntryStatus {
  case value {
    "staged" -> store_types.Staged
    "proxied" -> store_types.Proxied
    "committed" -> store_types.Committed
    _ -> store_types.Failed
  }
}

fn parse_operation_type(value: String) -> store.OperationType {
  case value {
    "mutation" -> store_types.Mutation
    _ -> store_types.Query
  }
}

fn restore_store_slice(
  base_state: store.BaseState,
  staged_state: store.StagedState,
  mutation_log: List(store.MutationLogEntry),
) -> Store {
  store_types.Store(
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
        False -> dispatch_graphql_async_or_sync(proxy, request)
      }
    _ -> promise.resolve(process_request(proxy, request))
  }
}

@target(javascript)
fn dispatch_graphql_async_or_sync(
  proxy: DraftProxy,
  request: Request,
) -> Promise(#(Response, DraftProxy)) {
  let #(response, next_proxy) = process_request(proxy, request)
  case passthrough.response_is_async_unsupported(response) {
    True -> dispatch_passthrough_async(proxy, request)
    False -> promise.resolve(#(response, next_proxy))
  }
}

@target(javascript)
fn is_passthrough_request(proxy: DraftProxy, request: Request) -> Bool {
  case parse_request_body(request.body) {
    Error(_) -> False
    Ok(body) ->
      case parse_operation.parse_operation(body.query) {
        Error(_) -> False
        Ok(parsed) ->
          live_hybrid_passthrough_target(proxy, parsed, body.variables)
          && !reject_unsupported_mutation_target(proxy, parsed, body.variables)
      }
  }
}

@target(javascript)
fn dispatch_passthrough_async(
  proxy: DraftProxy,
  request: Request,
) -> Promise(#(Response, DraftProxy)) {
  passthrough.passthrough_async(proxy, request)
  |> promise.map(fn(pair) {
    let #(response, next_proxy) = pair
    #(response, record_proxied_mutation_if_needed(next_proxy, request))
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

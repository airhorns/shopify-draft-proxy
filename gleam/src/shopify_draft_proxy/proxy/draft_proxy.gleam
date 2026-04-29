//// Mirrors the public-API surface of `src/proxy-instance.ts` and the
//// dispatcher spine of `src/proxy/routes.ts`.
////
//// This is a deliberate *spike* implementation — it wires the
//// already-ported pieces together (parser → parse_operation → events
//// handler → JSON response) so a real HTTP-shaped request can flow
//// through Gleam end to end. Only the events domain plus the pure-meta
//// routes (`/__meta/health`, `/__meta/config`, `/__meta/log`,
//// `/__meta/state`, `/__meta/reset`) are routed here; every other path
//// returns 404. Adding more domains is a matter of extending
//// `dispatch_graphql` with another branch keyed off `parsed.type` + the
//// first root field name.
////
//// The TS class is mutable; this Gleam port is not. Each dispatch
//// returns a `#(Response, DraftProxy)` pair so the synthetic identity
//// registry (and, eventually, the store) can be threaded forward.

import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/parse_operation.{
  MutationOperation, QueryOperation,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/delivery_settings
import shopify_draft_proxy/proxy/events
import shopify_draft_proxy/proxy/saved_searches
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types

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
  )
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
    MetaConfig -> #(config_response(proxy), proxy)
    MetaLog -> #(log_response(proxy), proxy)
    MetaState -> #(state_response(proxy), proxy)
    MetaReset -> {
      let next =
        DraftProxy(
          ..proxy,
          synthetic_identity: synthetic_identity.reset(
            proxy.synthetic_identity,
          ),
          store: store.reset(proxy.store),
        )
      #(reset_response(), next)
    }
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

fn config_response(proxy: DraftProxy) -> Response {
  let snapshot_enabled = case proxy.config.snapshot_path {
    Some(_) -> True
    None -> False
  }
  let snapshot_path = case proxy.config.snapshot_path {
    Some(p) -> json.string(p)
    None -> json.null()
  }
  Response(
    status: 200,
    body: json.object([
      #("runtime", json.object([
        #("readMode", json.string(read_mode_to_string(proxy.config.read_mode))),
      ])),
      #("proxy", json.object([
        #("port", json.int(proxy.config.port)),
        #("shopifyAdminOrigin", json.string(proxy.config.shopify_admin_origin)),
      ])),
      #("snapshot", json.object([
        #("enabled", json.bool(snapshot_enabled)),
        #("path", snapshot_path),
      ])),
    ]),
    headers: [],
  )
}

fn read_mode_to_string(mode: ReadMode) -> String {
  case mode {
    Snapshot -> "snapshot"
    LiveHybrid -> "live-hybrid"
    Live -> "live"
  }
}

fn log_response(proxy: DraftProxy) -> Response {
  Response(
    status: 200,
    body: json.object([
      #(
        "entries",
        json.array(store.get_log(proxy.store), serialize_mutation_log_entry),
      ),
    ]),
    headers: [],
  )
}

fn state_response(proxy: DraftProxy) -> Response {
  Response(
    status: 200,
    body: json.object([
      #("baseState", serialize_base_state(proxy.store.base_state)),
      #("stagedState", serialize_staged_state(proxy.store.staged_state)),
    ]),
    headers: [],
  )
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
    #(
      "interpreted",
      serialize_interpreted_metadata(entry.interpreted),
    ),
    #("notes", optional_string(entry.notes)),
  ])
}

fn serialize_interpreted_metadata(
  meta: store.InterpretedMetadata,
) -> Json {
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
        #(
          "operationName",
          optional_string(meta.capability.operation_name),
        ),
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

fn serialize_base_state(state: store.BaseState) -> Json {
  case dict.is_empty(state.saved_searches) {
    True -> json.object([])
    False ->
      json.object([
        #("savedSearches", serialize_saved_search_dict(state.saved_searches)),
      ])
  }
}

fn serialize_staged_state(state: store.StagedState) -> Json {
  let entries = case dict.is_empty(state.saved_searches) {
    True -> []
    False -> [
      #("savedSearches", serialize_saved_search_dict(state.saved_searches)),
    ]
  }
  let entries = case dict.is_empty(state.deleted_saved_search_ids) {
    True -> entries
    False ->
      list.append(entries, [
        #(
          "deletedSavedSearchIds",
          json.array(
            dict.keys(state.deleted_saved_search_ids),
            json.string,
          ),
        ),
      ])
  }
  json.object(entries)
}

fn serialize_saved_search_dict(
  records: dict.Dict(String, types.SavedSearchRecord),
) -> Json {
  json.object(
    dict.to_list(records)
    |> list.map(fn(pair) {
      let #(id, record) = pair
      #(id, serialize_saved_search_record(record))
    }),
  )
}

fn serialize_saved_search_record(record: types.SavedSearchRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("legacyResourceId", json.string(record.legacy_resource_id)),
    #("name", json.string(record.name)),
    #("query", json.string(record.query)),
    #("resourceType", json.string(record.resource_type)),
    #("searchTerms", json.string(record.search_terms)),
    #(
      "filters",
      json.array(record.filters, fn(filter) {
        json.object([
          #("key", json.string(filter.key)),
          #("value", json.string(filter.value)),
        ])
      }),
    ),
    #("cursor", optional_string(record.cursor)),
  ])
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
      #("errors", json.array(
        [json.object([#("message", json.string("Not found"))])],
        fn(x) { x },
      )),
    ]),
    headers: [],
  )
}

fn method_not_allowed_response() -> Response {
  Response(
    status: 405,
    body: json.object([
      #("errors", json.array(
        [json.object([#("message", json.string("Method not allowed"))])],
        fn(x) { x },
      )),
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
          case parsed.type_, list.first(parsed.root_fields) {
            QueryOperation, Ok(field) ->
              route_query(proxy, body.query, field, body.variables)
            MutationOperation, Ok(field) ->
              route_mutation(
                proxy,
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

fn route_mutation(
  proxy: DraftProxy,
  request_path: String,
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case saved_searches.is_saved_search_mutation_root(primary_root_field) {
    True ->
      case
        saved_searches.process_mutation(
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
          bad_request("Failed to handle saved searches mutation"),
          proxy,
        )
      }
    False -> #(
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
  query: String,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case domain_for(primary_root_field) {
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
}

fn domain_for(name: String) -> Result(Domain, Nil) {
  case name {
    "event" | "events" | "eventsCount" -> Ok(EventsDomain)
    "deliverySettings" | "deliveryPromiseSettings" ->
      Ok(DeliverySettingsDomain)
    _ ->
      case saved_searches.is_saved_search_query_root(name) {
        True -> Ok(SavedSearchesDomain)
        False -> Error(Nil)
      }
  }
}

fn respond(
  proxy: DraftProxy,
  result: Result(Json, a),
  error_message: String,
) -> #(Response, DraftProxy) {
  case result {
    Ok(envelope) -> #(
      Response(status: 200, body: envelope, headers: []),
      proxy,
    )
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
      #("errors", json.array(
        [json.object([#("message", json.string(message))])],
        fn(x) { x },
      )),
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

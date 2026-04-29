//// Mirrors the public-API surface of `src/proxy-instance.ts` and the
//// dispatcher spine of `src/proxy/routes.ts`.
////
//// This is a deliberate *spike* implementation — it wires the
//// already-ported pieces together (parser → parse_operation → events
//// handler → JSON response) so a real HTTP-shaped request can flow
//// through Gleam end to end. Only the events domain and `__meta/health`
//// are routed here; every other path returns 404. Adding more domains
//// is a matter of extending `dispatch_graphql` with another branch
//// keyed off `parsed.type` + the first root field name.
////
//// The TS class is mutable; this Gleam port is not. Each dispatch
//// returns a `#(Response, DraftProxy)` pair so the synthetic identity
//// registry (and, eventually, the store) can be threaded forward.

import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/json.{type Json}
import gleam/list
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/parse_operation.{
  MutationOperation, QueryOperation,
}
import shopify_draft_proxy/proxy/events
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

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

/// Long-lived runtime state owned by the proxy. The TS class wraps
/// this in a stateful `DraftProxy`; here it's just a record threaded
/// through each request.
pub type DraftProxy {
  DraftProxy(synthetic_identity: SyntheticIdentityRegistry)
}

/// Fresh proxy with default state. Equivalent to `new DraftProxy(...)`.
pub fn new() -> DraftProxy {
  DraftProxy(synthetic_identity: synthetic_identity.new())
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
    GraphQL(version: _) -> dispatch_graphql(proxy, request)
    NotFound -> #(not_found_response(), proxy)
    MethodNotAllowed -> #(method_not_allowed_response(), proxy)
  }
}

type Route {
  Health
  GraphQL(version: String)
  NotFound
  MethodNotAllowed
}

fn route(request: Request) -> Route {
  let method = string.uppercase(request.method)
  case request.path {
    "/__meta/health" ->
      case method {
        "GET" -> Health
        _ -> MethodNotAllowed
      }
    other ->
      case is_admin_graphql_path(other) {
        Ok(version) ->
          case method {
            "POST" -> GraphQL(version: version)
            _ -> MethodNotAllowed
          }
        Error(_) -> NotFound
      }
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
            QueryOperation, Ok(field) -> route_query(proxy, body.query, field)
            MutationOperation, _ -> #(
              bad_request(
                "Mutations are not yet implemented in the Gleam port",
              ),
              proxy,
            )
            _, _ -> #(bad_request("Operation has no root field"), proxy)
          }
      }
  }
}

fn route_query(
  proxy: DraftProxy,
  query: String,
  primary_root_field: String,
) -> #(Response, DraftProxy) {
  case is_events_field(primary_root_field) {
    True ->
      case events.process(query) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(bad_request("Failed to handle events query"), proxy)
      }
    False -> #(
      bad_request(
        "No domain dispatcher implemented for root field: "
        <> primary_root_field,
      ),
      proxy,
    )
  }
}

fn is_events_field(name: String) -> Bool {
  case name {
    "event" | "events" | "eventsCount" -> True
    _ -> False
  }
}

type ParsedBody {
  ParsedBody(query: String)
}

fn parse_request_body(body: String) -> Result(ParsedBody, String) {
  let decoder = {
    use query <- decode.field("query", decode.string)
    decode.success(ParsedBody(query: query))
  }
  json.parse(body, decoder)
  |> result.map_error(fn(_) { "Expected JSON body with a string `query`" })
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

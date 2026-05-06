//// Query routing for the metafield definitions domain.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers
import shopify_draft_proxy/proxy/metafield_definitions/serializers
import shopify_draft_proxy/proxy/metafield_definitions/types as definition_types
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

pub fn is_metafield_definitions_query_root(name: String) -> Bool {
  case name {
    "metafieldDefinition"
    | "metafieldDefinitions"
    | "product"
    | "productVariant"
    | "collection"
    | "customer" -> True
    _ -> False
  }
}

pub fn local_has_metafield_definition_state(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic
  || !list.is_empty(store.list_effective_metafield_definitions(proxy.store))
  || !definition_types.dict_is_empty(
    proxy.store.staged_state.deleted_metafield_definition_ids,
  )
  || !definition_types.dict_is_empty(
    proxy.store.base_state.deleted_metafield_definition_ids,
  )
}

/// Pattern 1: cold LiveHybrid definition catalog/detail reads are just
/// upstream reads. Once a local lifecycle has staged or deleted definitions,
/// keep reads local so read-after-write and read-after-delete behavior does
/// not leak back to Shopify.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case
    proxy.config.read_mode,
    local_has_metafield_definition_state(proxy, variables)
  {
    LiveHybrid, False -> passthrough.passthrough_sync(proxy, request)
    _, _ ->
      respond_local(
        proxy,
        process(proxy.store, query, variables),
        "Failed to handle metafield definitions query",
      )
  }
}

pub fn handle_metafield_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, definition_types.MetafieldDefinitionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(definition_types.ParseFailed(err))
    Ok(fields) ->
      Ok(serializers.serialize_root_fields(store, fields, variables))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, definition_types.MetafieldDefinitionsError) {
  use data <- result.try(handle_metafield_definitions_query(
    store,
    document,
    variables,
  ))
  Ok(graphql_helpers.wrap_data(data))
}

@internal
pub fn respond_local(
  proxy: DraftProxy,
  result: Result(Json, definition_types.MetafieldDefinitionsError),
  error_message: String,
) -> #(Response, DraftProxy) {
  case result {
    Ok(body) -> #(Response(status: 200, body: body, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([#("error", json.string(error_message))]),
        headers: [],
      ),
      proxy,
    )
  }
}

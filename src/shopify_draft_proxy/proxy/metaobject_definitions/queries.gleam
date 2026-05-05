//// Query dispatch for metaobject definitions and metaobjects.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}
import shopify_draft_proxy/proxy/metaobject_definitions/serializers
import shopify_draft_proxy/proxy/metaobject_definitions/types
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/state/store.{
  type Store, get_effective_metaobject_by_id,
  get_effective_metaobject_definition_by_id,
  list_effective_metaobject_definitions, list_effective_metaobjects,
}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

@internal
pub fn is_metaobject_definitions_query_root(name: String) -> Bool {
  case name {
    "metaobject" -> True
    "metaobjectByHandle" -> True
    "metaobjects" -> True
    "metaobjectDefinition" -> True
    "metaobjectDefinitionByType" -> True
    "metaobjectDefinitions" -> True
    _ -> False
  }
}

@internal
pub fn handle_metaobject_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  handle_metaobject_definitions_query_with_app_id(
    store,
    document,
    variables,
    None,
  )
}

@internal
pub fn handle_metaobject_definitions_query_with_app_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serializers.serialize_root_fields(
        store,
        fields,
        fragments,
        variables,
        requesting_api_client_id,
      ))
    }
  }
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  process_with_requesting_api_client_id(store, document, variables, None)
}

@internal
pub fn process_with_requesting_api_client_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, root_field.RootFieldError) {
  case
    handle_metaobject_definitions_query_with_app_id(
      store,
      document,
      variables,
      requesting_api_client_id,
    )
  {
    Ok(data) -> Ok(serializers.wrap_data(data))
    Error(e) -> Error(e)
  }
}

@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    // Pattern 1: cold LiveHybrid metaobject reads are upstream-verbatim.
    // Once local definitions/entries are staged or deleted, reads stay local
    // so supported mutations preserve read-after-write behavior.
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case
        process_with_requesting_api_client_id(
          proxy.store,
          document,
          variables,
          app_identity.read_requesting_api_client_id(request.headers),
        )
      {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string(
                          "Failed to handle metaobject definitions query",
                        ),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "metaobject" ->
      !local_has_metaobject_id(proxy, variables)
    parse_operation.QueryOperation, "metaobjectByHandle" ->
      !local_has_metaobjects(proxy)
    parse_operation.QueryOperation, "metaobjects" ->
      !local_has_metaobjects(proxy)
    parse_operation.QueryOperation, "metaobjectDefinition" ->
      !local_has_metaobject_definition_id(proxy, variables)
    parse_operation.QueryOperation, "metaobjectDefinitionByType" ->
      !local_has_metaobject_definitions(proxy)
    parse_operation.QueryOperation, "metaobjectDefinitions" ->
      !local_has_metaobject_definitions(proxy)
    _, _ -> False
  }
}

fn local_has_metaobject_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.flat_map(types.resolved_value_strings)
  |> list.any(fn(id) {
    is_proxy_synthetic_gid(id) || local_metaobject_id_known(proxy.store, id)
  })
}

fn local_metaobject_id_known(store: Store, id: String) -> Bool {
  case get_effective_metaobject_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_metaobject_ids, id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

fn local_has_metaobject_definition_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.flat_map(types.resolved_value_strings)
  |> list.any(fn(id) {
    is_proxy_synthetic_gid(id)
    || local_metaobject_definition_id_known(proxy.store, id)
  })
}

fn local_metaobject_definition_id_known(store: Store, id: String) -> Bool {
  case get_effective_metaobject_definition_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_metaobject_definition_ids, id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

fn local_has_metaobjects(proxy: DraftProxy) -> Bool {
  !list.is_empty(list_effective_metaobjects(proxy.store))
  || !list.is_empty(dict.keys(proxy.store.staged_state.deleted_metaobject_ids))
}

fn local_has_metaobject_definitions(proxy: DraftProxy) -> Bool {
  !list.is_empty(list_effective_metaobject_definitions(proxy.store))
  || !list.is_empty(dict.keys(
    proxy.store.staged_state.deleted_metaobject_definition_ids,
  ))
}

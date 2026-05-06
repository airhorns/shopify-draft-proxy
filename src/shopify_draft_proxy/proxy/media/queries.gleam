//// Files API query dispatch.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, default_selected_field_options, get_document_fragments,
  get_field_response_key, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/media/serializers
import shopify_draft_proxy/proxy/media/types.{type MediaError, ParseFailed}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/state/store.{type Store}

@internal
pub fn is_media_query_root(name: String) -> Bool {
  case name {
    "files" -> True
    _ -> False
  }
}

@internal
pub fn handle_media_query(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, MediaError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "files" ->
              serializers.serialize_files_connection(
                store,
                field,
                fragments,
                variables,
              )
            "fileSavedSearches" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, MediaError) {
  case handle_media_query(store, document, variables) {
    Ok(data) -> Ok(graphql_helpers.wrap_data(data))
    Error(e) -> Error(e)
  }
}

/// Uniform query entrypoint matching the dispatcher's signature.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process(proxy.store, document, variables),
    "Failed to handle media query",
  )
}

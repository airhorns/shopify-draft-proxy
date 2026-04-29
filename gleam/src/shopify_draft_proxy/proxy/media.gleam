//// Minimal port of `src/proxy/media.ts`.
////
//// The full TS module is ~987 LOC and covers `files` connection
//// pagination over the local file overlay, file create/update/delete
//// mutations, staged-upload creation, the
//// `fileAcknowledgeUpdateFailed` re-up flow, and the cross-cut into
//// product-media records when files are renamed/replaced. This stub
//// only ships the always-on read shape — the single `files` connection
//// returns the empty-connection envelope so the dispatcher can route
//// the "Media" capability without falling back to the upstream proxy.
////
//// Reads (all empty until the store slice ports):
////   - `files(...)` → empty connection (`nodes`/`edges` empty,
////     `pageInfo` all-false-with-null-cursors).
////
//// Mutations are intentionally not handled here.
////
//// Note: `fileSavedSearches` lives in the saved-searches domain and is
//// served by `proxy/saved_searches`, not here — matching the registry.

import gleam/json.{type Json}
import gleam/list
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_field_response_key,
  serialize_empty_connection,
}

pub type MediaError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_media_query_root(name: String) -> Bool {
  case name {
    "files" -> True
    _ -> False
  }
}

pub fn handle_media_query(document: String) -> Result(Json, MediaError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> Ok(serialize_root_fields(fields))
  }
}

fn serialize_root_fields(fields: List(Selection)) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "files" ->
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

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(document: String) -> Result(Json, MediaError) {
  case handle_media_query(document) {
    Ok(data) -> Ok(wrap_data(data))
    Error(e) -> Error(e)
  }
}

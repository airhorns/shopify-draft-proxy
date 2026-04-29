//// Minimal port of `src/proxy/metaobject-definitions.ts`.
////
//// The full TS module is ~2700 LOC and covers metaobject + metaobject-
//// definition lifecycle (create/update/upsert/delete/bulkDelete plus
//// definitionCreate/Update/Delete plus standardMetaobjectDefinitionEnable),
//// field validation, capability inspection, type-scoped enumeration,
//// handle/type lookups, and connection pagination with field-value
//// query filters. This stub only ships the always-on read shape — every
//// query root returns an empty answer so the dispatcher can route the
//// "Metaobjects" capability without falling back to the upstream proxy.
////
//// Reads (all empty/null until the store slice ports):
////   - `metaobject(id:)` / `metaobjectByHandle(handle:, type:)` → null.
////   - `metaobjectDefinition(id:)` /
////     `metaobjectDefinitionByType(type:)` → null.
////   - `metaobjects(...)` / `metaobjectDefinitions(...)` → empty
////     connection (`nodes`/`edges` empty, `pageInfo` all-false-with-
////     null-cursors).
////
//// Mutations are intentionally not handled here. With no store slice
//// the only honest response is a not-implemented error — the existing
//// `Ok(_) | Error(_)` arm in the mutation dispatcher already produces
//// that "No mutation dispatcher implemented" path, so metaobject
//// mutations stay unrouted until a follow-up pass lands the lifecycle.

import gleam/json.{type Json}
import gleam/list
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_field_response_key,
  serialize_empty_connection,
}

/// Errors specific to the metaobject-definitions handler. Currently
/// just propagates upstream parse errors.
pub type MetaobjectDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching every supported metaobject(-definition) query
/// root. The full TS surface is enumerated here even though every
/// branch currently returns null/empty — this lets the dispatcher's
/// legacy fallback recognise the domain by root-field name.
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

/// Handle a `query` operation against the metaobject(-definition)
/// surface. Returns the unwrapped data object — the caller wraps it
/// in `{"data": ...}`.
pub fn handle_metaobject_definitions_query(
  document: String,
) -> Result(Json, MetaobjectDefinitionsError) {
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
            "metaobject" -> json.null()
            "metaobjectByHandle" -> json.null()
            "metaobjects" ->
              serialize_empty_connection(field, default_selected_field_options())
            "metaobjectDefinition" -> json.null()
            "metaobjectDefinitionByType" -> json.null()
            "metaobjectDefinitions" ->
              serialize_empty_connection(field, default_selected_field_options())
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

/// Wrap a successful response in the standard GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  document: String,
) -> Result(Json, MetaobjectDefinitionsError) {
  case handle_metaobject_definitions_query(document) {
    Ok(data) -> Ok(wrap_data(data))
    Error(e) -> Error(e)
  }
}

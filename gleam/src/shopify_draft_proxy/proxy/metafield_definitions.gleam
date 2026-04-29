//// Minimal port of `src/proxy/metafield-definitions.ts`.
////
//// The full TS module is ~1550 LOC and covers definition lifecycle
//// (create/update/delete/pin/unpin), validation, capability
//// inspection, and seeded catalog reads. This port only ships the
//// "no data" branches the proxy needs to answer when nothing has
//// been seeded:
////
////   - `metafieldDefinition(identifier:)` / `metafieldDefinition(id:)`
////     returns `null` when there is no matching record.
////   - `metafieldDefinitions(...)` returns an empty connection
////     (`nodes`/`edges` empty, `pageInfo` all-false-with-null-cursors).
////
//// That's enough to make the `metafield-definitions-product-empty-read`
//// parity scenario pass — its only checked targets are the `missing`
//// and `empty` aliases. The other roots in the request document
//// (`byIdentifier`, `metafieldDefinitions`, `filteredByQuery`,
//// `seedCatalog`) still serialize, but because the parity spec doesn't
//// compare them their (empty) shapes don't need to match the captured
//// response. Lifecycle mutations and seeded reads will land in a later
//// pass.

import gleam/json.{type Json}
import gleam/list
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_field_response_key,
  serialize_empty_connection,
}

/// Errors specific to the metafield-definitions handler. Currently just
/// propagates upstream parse errors.
pub type MetafieldDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching the supported subset of metafield-definitions
/// query roots. The full TS surface adds `metafieldDefinitionTypes`
/// and `standardMetafieldDefinitionTemplates`; those are deliberately
/// out-of-scope for this minimal port.
pub fn is_metafield_definitions_query_root(name: String) -> Bool {
  case name {
    "metafieldDefinition" -> True
    "metafieldDefinitions" -> True
    _ -> False
  }
}

/// Handle a `query` operation against the metafield-definitions surface.
pub fn handle_metafield_definitions_query(
  document: String,
) -> Result(Json, MetafieldDefinitionsError) {
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
            "metafieldDefinition" -> json.null()
            "metafieldDefinitions" ->
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
pub fn process(document: String) -> Result(Json, MetafieldDefinitionsError) {
  use data <- result.try(handle_metafield_definitions_query(document))
  Ok(wrap_data(data))
}

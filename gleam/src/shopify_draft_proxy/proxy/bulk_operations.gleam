//// Minimal port of `src/proxy/bulk-operations.ts`.
////
//// The full TS module is ~1462 LOC and covers bulk-operation lifecycle
//// (run-query / run-mutation / cancel), polling-friendly id-vs-window
//// validation, the import-log JSONL replay, and connection pagination.
//// This stub only ships the always-on read shape — every query root
//// returns an empty answer so the dispatcher can route the
//// "BulkOperations" capability without falling back to the upstream
//// proxy.
////
//// Reads (all empty/null until the store slice ports):
////   - `bulkOperation(id:)` → null.
////   - `currentBulkOperation(...)` → null.
////   - `bulkOperations(...)` → empty connection (`nodes`/`edges` empty,
////     `pageInfo` all-false-with-null-cursors).
////
//// Mutations are intentionally not handled here.

import gleam/json.{type Json}
import gleam/list
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_field_response_key,
  serialize_empty_connection,
}

pub type BulkOperationsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_bulk_operations_query_root(name: String) -> Bool {
  case name {
    "bulkOperation" -> True
    "bulkOperations" -> True
    "currentBulkOperation" -> True
    _ -> False
  }
}

pub fn handle_bulk_operations_query(
  document: String,
) -> Result(Json, BulkOperationsError) {
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
            "bulkOperation" -> json.null()
            "currentBulkOperation" -> json.null()
            "bulkOperations" ->
              serialize_empty_connection(field, default_selected_field_options())
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

pub fn process(document: String) -> Result(Json, BulkOperationsError) {
  case handle_bulk_operations_query(document) {
    Ok(data) -> Ok(wrap_data(data))
    Error(e) -> Error(e)
  }
}

//// Minimal port of `src/proxy/marketing.ts`.
////
//// The full TS module is ~1285 LOC and covers marketing-activity
//// lifecycle (create/update/createExternal/updateExternal/upsertExternal
//// /delete/deleteExternal/deleteAllExternal), marketing engagements,
//// marketing events, channel-handle inspection, query-grammar filters,
//// and connection pagination by tactic/status. This stub only ships
//// the always-on read shape — every query root returns an empty
//// answer so the dispatcher can route the "Marketing" capability
//// without falling back to the upstream proxy.
////
//// Reads (all empty/null until the store slice ports):
////   - `marketingActivity(id:)` / `marketingEvent(id:)` → null.
////   - `marketingActivities(...)` / `marketingEvents(...)` → empty
////     connection (`nodes`/`edges` empty, `pageInfo` all-false-with-
////     null-cursors).
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

pub type MarketingError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_marketing_query_root(name: String) -> Bool {
  case name {
    "marketingActivity" -> True
    "marketingActivities" -> True
    "marketingEvent" -> True
    "marketingEvents" -> True
    _ -> False
  }
}

pub fn handle_marketing_query(
  document: String,
) -> Result(Json, MarketingError) {
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
            "marketingActivity" -> json.null()
            "marketingEvent" -> json.null()
            "marketingActivities" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            "marketingEvents" ->
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

pub fn process(document: String) -> Result(Json, MarketingError) {
  case handle_marketing_query(document) {
    Ok(data) -> Ok(wrap_data(data))
    Error(e) -> Error(e)
  }
}

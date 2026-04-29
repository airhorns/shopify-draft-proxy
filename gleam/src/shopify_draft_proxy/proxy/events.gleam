//// Mirrors `src/proxy/events.ts`.
////
//// The Shopify events API is read-only and the proxy never replays
//// upstream — it just returns empty connections and zero counts. This
//// matches the TS handler exactly: every recognised root field maps to
//// `null`, an empty connection, or a zero-count payload.

import gleam/json.{type Json}
import gleam/list
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, serialize_empty_connection,
}

/// Errors specific to the events handler. Currently just propagates
/// upstream graphql parse errors — every other shape resolves to a JSON
/// payload.
pub type EventsError {
  ParseFailed(root_field.RootFieldError)
}

/// Handle a `query` operation against the events surface. Returns a
/// JSON object suitable for embedding into a `{ data: … }` envelope.
/// Mirrors `handleEventsQuery` in TS.
pub fn handle_events_query(document: String) -> Result(Json, EventsError) {
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
            "event" -> json.null()
            "events" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            "eventsCount" -> serialize_exact_zero_count(field)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

fn serialize_exact_zero_count(field: Selection) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(0))
              "precision" -> #(key, json.string("EXACT"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

/// Wrap a successful events response in the standard GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(document: String) -> Result(Json, EventsError) {
  use data <- result.try(handle_events_query(document))
  Ok(wrap_data(data))
}

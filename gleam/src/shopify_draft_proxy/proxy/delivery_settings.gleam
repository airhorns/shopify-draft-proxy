//// Mirrors `src/proxy/delivery-settings.ts`.
////
//// Delivery settings are fully synthetic — the proxy never replays
//// upstream. Both root fields resolve to a fixed default object that
//// gets projected through the client's selection set. This is the
//// smallest read-only domain handler in the codebase and exercises
//// the new `project_graphql_object` helper end to end.

import gleam/json.{type Json}
import gleam/list
import gleam/option.{Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcBool, SrcNull, SrcString, src_object,
}

pub type DeliverySettingsError {
  ParseFailed(root_field.RootFieldError)
}

/// Mirrors `DEFAULT_DELIVERY_SETTINGS`.
fn default_delivery_settings() -> SourceValue {
  src_object([
    #("__typename", SrcString("DeliverySetting")),
    #("legacyModeProfiles", SrcBool(False)),
    #(
      "legacyModeBlocked",
      src_object([
        #("__typename", SrcString("DeliveryLegacyModeBlocked")),
        #("blocked", SrcBool(False)),
        #("reasons", SrcNull),
      ]),
    ),
  ])
}

/// Mirrors `DEFAULT_DELIVERY_PROMISE_SETTINGS`.
fn default_delivery_promise_settings() -> SourceValue {
  src_object([
    #("__typename", SrcString("DeliveryPromiseSetting")),
    #("deliveryDatesEnabled", SrcBool(False)),
    #("processingTime", SrcNull),
  ])
}

/// Handle a `query` operation against the delivery-settings surface.
/// Returns the projected JSON object; the dispatcher wraps it in a
/// `{ data: … }` envelope. Mirrors `handleDeliverySettingsQuery`.
pub fn handle_delivery_settings_query(
  document: String,
) -> Result(Json, DeliverySettingsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = graphql_helpers.get_document_fragments(document)
      let entries =
        list.map(fields, fn(field) {
          let key = graphql_helpers.get_field_response_key(field)
          let value = serialise_field(field, fragments)
          #(key, value)
        })
      Ok(json.object(entries))
    }
  }
}

fn serialise_field(
  field: Selection,
  fragments: graphql_helpers.FragmentMap,
) -> Json {
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "deliverySettings" ->
          project_settings(default_delivery_settings(), ss, fragments)
        "deliveryPromiseSettings" ->
          project_settings(default_delivery_promise_settings(), ss, fragments)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn project_settings(
  source: SourceValue,
  selection_set: option.Option(ast.SelectionSet),
  fragments: graphql_helpers.FragmentMap,
) -> Json {
  case selection_set {
    Some(SelectionSet(selections: selections, ..)) ->
      graphql_helpers.project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}


/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(document: String) -> Result(Json, DeliverySettingsError) {
  use data <- result.try(handle_delivery_settings_query(document))
  Ok(graphql_helpers.wrap_data(data))
}

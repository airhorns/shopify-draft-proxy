//// Shared internal marketing value readers and serializers.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcFloat, SrcInt, SrcList,
  SrcNull, SrcObject, SrcString, project_graphql_value, source_to_json,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type MarketingRecord, type MarketingValue, MarketingBool, MarketingFloat,
  MarketingInt, MarketingList, MarketingNull, MarketingObject, MarketingString,
}

@internal
pub fn read_arg_string_list(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_value_string(
  source: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(String) {
  case dict.get(source, field) {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.IntVal(value)) -> Some(int.to_string(value))
    Ok(root_field.FloatVal(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

@internal
pub fn read_value_bool(
  source: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(Bool) {
  case dict.get(source, field) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_value_int(
  source: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(Int) {
  case dict.get(source, field) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_money_input(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(Dict(String, MarketingValue)) {
  case dict.get(input, field) {
    Ok(root_field.ObjectVal(money)) -> {
      let amount = read_value_string(money, "amount")
      let currency = read_value_string(money, "currencyCode")
      case amount, currency {
        Some(amount), Some(currency) ->
          Some(
            dict.from_list([
              #("amount", MarketingString(amount)),
              #("currencyCode", MarketingString(currency)),
            ]),
          )
        _, _ -> None
      }
    }
    _ -> None
  }
}

@internal
pub fn read_decimal_input(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(String) {
  read_value_string(input, field)
}

@internal
pub fn read_utm(
  source: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, MarketingValue)) {
  let nested = case dict.get(source, "utm") {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ ->
      case dict.get(source, "utmParameters") {
        Ok(root_field.ObjectVal(value)) -> Some(value)
        _ -> None
      }
  }
  let candidate = option.unwrap(nested, source)
  let campaign = read_value_string(candidate, "campaign")
  let source_value = read_value_string(candidate, "source")
  let medium = read_value_string(candidate, "medium")
  case campaign, source_value, medium {
    Some(campaign), Some(source_value), Some(medium) ->
      Some(
        dict.from_list([
          #("campaign", MarketingString(campaign)),
          #("source", MarketingString(source_value)),
          #("medium", MarketingString(medium)),
        ]),
      )
    _, _, _ -> None
  }
}

@internal
pub fn has_attribution(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_utm(input) {
    Some(_) -> True
    None ->
      case read_value_string(input, "urlParameterValue") {
        Some(_) -> True
        None -> False
      }
  }
}

@internal
pub fn same_utm(
  left: Option(Dict(String, MarketingValue)),
  right: Option(Dict(String, MarketingValue)),
) -> Bool {
  case left, right {
    None, None -> True
    Some(left), Some(right) ->
      read_marketing_object_string(Some(left), "campaign")
      == read_marketing_object_string(Some(right), "campaign")
      && read_marketing_object_string(Some(left), "source")
      == read_marketing_object_string(Some(right), "source")
      && read_marketing_object_string(Some(left), "medium")
      == read_marketing_object_string(Some(right), "medium")
    _, _ -> False
  }
}

@internal
pub fn find_marketing_activity_by_utm(
  store: Store,
  utm: Option(Dict(String, MarketingValue)),
) -> Option(MarketingRecord) {
  find_marketing_activity_by_utm_for_app(store, utm, None)
}

@internal
pub fn find_marketing_activity_by_utm_for_app(
  store: Store,
  utm: Option(Dict(String, MarketingValue)),
  requesting_api_client_id: Option(String),
) -> Option(MarketingRecord) {
  case utm {
    None -> None
    Some(utm) ->
      list.find(
        store.list_effective_marketing_activities_for_app(
          store,
          requesting_api_client_id,
        ),
        fn(activity) {
          same_utm(
            read_marketing_object(activity.data, "utmParameters"),
            Some(utm),
          )
        },
      )
      |> option.from_result
  }
}

@internal
pub fn read_marketing_string(
  source: Dict(String, MarketingValue),
  field: String,
) -> Option(String) {
  case dict.get(source, field) {
    Ok(MarketingString(value)) -> Some(value)
    Ok(MarketingInt(value)) -> Some(int.to_string(value))
    Ok(MarketingFloat(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

@internal
pub fn read_marketing_bool(
  source: Dict(String, MarketingValue),
  field: String,
) -> Bool {
  case dict.get(source, field) {
    Ok(MarketingBool(value)) -> value
    _ -> False
  }
}

@internal
pub fn read_marketing_object(
  source: Dict(String, MarketingValue),
  field: String,
) -> Option(Dict(String, MarketingValue)) {
  case dict.get(source, field) {
    Ok(MarketingObject(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_marketing_object_string(
  source: Option(Dict(String, MarketingValue)),
  field: String,
) -> Option(String) {
  case source {
    Some(source) -> read_marketing_string(source, field)
    None -> None
  }
}

@internal
pub fn find_marketing_event_by_remote_id(
  store: Store,
  remote_id: String,
) -> Option(MarketingRecord) {
  find_marketing_event_by_remote_id_for_app(store, remote_id, None)
}

@internal
pub fn find_marketing_event_by_remote_id_for_app(
  store: Store,
  remote_id: String,
  requesting_api_client_id: Option(String),
) -> Option(MarketingRecord) {
  store.list_effective_marketing_events_for_app(store, requesting_api_client_id)
  |> list.find(fn(event) {
    read_marketing_string(event.data, "remoteId") == Some(remote_id)
  })
  |> option.from_result
}

@internal
pub fn marketing_remote_id(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case read_marketing_string(data, "remoteId") {
    Some(id) -> Some(id)
    None ->
      read_marketing_object(data, "marketingEvent")
      |> read_marketing_object_string("remoteId")
  }
}

@internal
pub fn app_name(data: Dict(String, MarketingValue)) -> Option(String) {
  case read_marketing_object(data, "app") {
    Some(app) ->
      case read_marketing_string(app, "name") {
        Some(name) -> Some(name)
        None -> read_marketing_string(app, "title")
      }
    None -> None
  }
}

@internal
pub fn optional_marketing_string(value: Option(String)) -> MarketingValue {
  case value {
    Some(value) -> MarketingString(value)
    None -> MarketingNull
  }
}

@internal
pub fn optional_marketing_object(
  value: Option(Dict(String, MarketingValue)),
) -> MarketingValue {
  case value {
    Some(value) -> MarketingObject(value)
    None -> MarketingNull
  }
}

@internal
pub fn optional_string_list_source(value: Option(List(String))) -> SourceValue {
  case value {
    Some(values) -> SrcList(list.map(values, SrcString))
    None -> SrcNull
  }
}

@internal
pub fn marketing_value_to_source(value: MarketingValue) -> SourceValue {
  case value {
    MarketingNull -> SrcNull
    MarketingString(value) -> SrcString(value)
    MarketingBool(value) -> SrcBool(value)
    MarketingInt(value) -> SrcInt(value)
    MarketingFloat(value) -> SrcFloat(value)
    MarketingList(values) ->
      SrcList(list.map(values, marketing_value_to_source))
    MarketingObject(values) -> marketing_data_to_source(values)
  }
}

@internal
pub fn marketing_data_to_source(
  data: Dict(String, MarketingValue),
) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, marketing_value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

@internal
pub fn project_payload(
  payload: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> source_to_json(payload)
  }
}

@internal
pub fn overlay_marketing_data(
  base: Dict(String, MarketingValue),
  entries: List(#(String, MarketingValue)),
) -> Dict(String, MarketingValue) {
  list.fold(entries, base, fn(acc, pair) { dict.insert(acc, pair.0, pair.1) })
}

@internal
pub fn is_known_local_marketing_activity_extension(id: Option(String)) -> Bool {
  case id {
    Some(id) ->
      string.starts_with(id, "gid://shopify/MarketingActivityExtension/")
      && !string.ends_with(id, "/00000000-0000-0000-0000-000000000000")
    None -> False
  }
}

@internal
pub fn event_ended_at_for_status(
  status: String,
  timestamp: String,
) -> Option(String) {
  case status {
    "INACTIVE" | "DELETED_EXTERNALLY" -> Some(timestamp)
    _ -> None
  }
}

@internal
pub fn status_label(status: String) -> String {
  case status {
    "ACTIVE" -> "Sending"
    "DELETED" -> "Deleted"
    "INACTIVE" -> "Sent"
    "PAUSED" -> "Paused"
    "PENDING" -> "Pending"
    "SCHEDULED" -> "Scheduled"
    "DRAFT" -> "Draft"
    "FAILED" -> "Failed"
    "DISCONNECTED" -> "Disconnected"
    "DELETED_EXTERNALLY" -> "Deleted externally"
    _ -> "Undefined"
  }
}

@internal
pub fn source_and_medium(
  marketing_channel_type: String,
  tactic: String,
) -> String {
  case marketing_channel_type, tactic {
    "EMAIL", "NEWSLETTER" -> "Email newsletter"
    _, _ ->
      capitalize(string.lowercase(marketing_channel_type))
      <> " "
      <> string.replace(string.lowercase(tactic), "_", " ")
  }
}

@internal
pub fn capitalize(value: String) -> String {
  case string.to_graphemes(value) {
    [] -> "E"
    [first, ..rest] -> string.uppercase(first) <> string.concat(rest)
  }
}

@internal
pub fn id_number(id: String) -> Option(Int) {
  case list.last(string.split(id, "/")) {
    Ok(last) ->
      case int.parse(last) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn option_count(value: Option(a)) -> Int {
  case value {
    Some(_) -> 1
    None -> 0
  }
}

@internal
pub fn dedupe_strings(values: List(String)) -> List(String) {
  let #(deduped, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.get(seen, value) {
        Ok(_) -> #(items, seen)
        Error(_) -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(deduped)
}

@internal
pub fn url_encode(value: String) -> String {
  value
  |> string.replace("%", "%25")
  |> string.replace(":", "%3A")
  |> string.replace("/", "%2F")
  |> string.replace(" ", "%20")
}

@internal
pub fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

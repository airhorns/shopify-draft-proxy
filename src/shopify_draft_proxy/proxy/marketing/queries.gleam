//// Query and hydration handling for marketing roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionPageInfoOptions, type FragmentMap, type SourceValue,
  ConnectionPageInfoOptions, ConnectionWindow, SerializeConnectionConfig,
  SrcBool, SrcFloat, SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, source_to_json,
}
import shopify_draft_proxy/proxy/marketing/serializers
import shopify_draft_proxy/proxy/marketing/types as marketing_types
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type MarketingRecord, type MarketingValue, MarketingBool, MarketingFloat,
  MarketingInt, MarketingList, MarketingNull, MarketingObject, MarketingRecord,
  MarketingString,
}

@internal
pub fn handle_marketing_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  let fragments = get_document_fragments(document)
  Ok(serialize_root_fields(store, fields, fragments, variables))
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_marketing_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process(proxy.store, document, variables),
    "Failed to handle marketing query",
  )
}

@internal
pub fn hydrate_marketing_from_upstream_payload(
  store: Store,
  payload: SourceValue,
) -> Store {
  let marketing_types.CollectedMarketingRecords(
    activities: activities,
    events: events,
  ) = collect_marketing_records(payload, None, empty_collected_records())
  store
  |> store.upsert_base_marketing_activities(activities)
  |> store.upsert_base_marketing_events(events)
}

fn empty_collected_records() -> marketing_types.CollectedMarketingRecords {
  marketing_types.CollectedMarketingRecords(activities: [], events: [])
}

fn collect_marketing_records(
  value: SourceValue,
  cursor: Option(String),
  collected: marketing_types.CollectedMarketingRecords,
) -> marketing_types.CollectedMarketingRecords {
  case value {
    SrcList(items) ->
      list.fold(items, collected, fn(acc, item) {
        collect_marketing_records(item, cursor, acc)
      })
    SrcObject(fields) -> collect_marketing_object(fields, cursor, collected)
    _ -> collected
  }
}

fn collect_marketing_object(
  fields: Dict(String, SourceValue),
  cursor: Option(String),
  collected: marketing_types.CollectedMarketingRecords,
) -> marketing_types.CollectedMarketingRecords {
  let edge_cursor = source_field_string(fields, "cursor")
  let collected = case source_field(fields, "node"), edge_cursor {
    Some(node), Some(node_cursor) ->
      collect_marketing_records(node, Some(node_cursor), collected)
    _, _ -> collected
  }
  let collected = case source_field_string(fields, "id") {
    Some(id) ->
      case string.starts_with(id, marketing_types.activity_id_prefix) {
        True ->
          marketing_types.CollectedMarketingRecords(..collected, activities: [
            MarketingRecord(
              id: id,
              cursor: cursor,
              data: source_object_to_marketing_data(fields),
            ),
            ..collected.activities
          ])
        False ->
          case string.starts_with(id, marketing_types.event_id_prefix) {
            True ->
              marketing_types.CollectedMarketingRecords(..collected, events: [
                MarketingRecord(
                  id: id,
                  cursor: cursor,
                  data: source_object_to_marketing_data(fields),
                ),
                ..collected.events
              ])
            False -> collected
          }
      }
    _ -> collected
  }
  dict.to_list(fields)
  |> list.fold(collected, fn(acc, pair) {
    let #(name, child) = pair
    case name {
      "node" -> acc
      _ -> collect_marketing_records(child, None, acc)
    }
  })
}

fn source_object_to_marketing_data(
  fields: Dict(String, SourceValue),
) -> Dict(String, MarketingValue) {
  dict.to_list(fields)
  |> list.map(fn(pair) {
    let #(key, value) = pair
    #(key, source_to_marketing_value(value))
  })
  |> dict.from_list
}

fn source_to_marketing_value(value: SourceValue) -> MarketingValue {
  case value {
    SrcNull -> MarketingNull
    SrcString(value) -> MarketingString(value)
    SrcBool(value) -> MarketingBool(value)
    SrcInt(value) -> MarketingInt(value)
    SrcFloat(value) -> MarketingFloat(value)
    SrcList(items) -> MarketingList(list.map(items, source_to_marketing_value))
    SrcObject(fields) ->
      MarketingObject(source_object_to_marketing_data(fields))
  }
}

fn source_field(
  fields: Dict(String, SourceValue),
  name: String,
) -> Option(SourceValue) {
  dict.get(fields, name)
  |> option.from_result
}

fn source_field_string(
  fields: Dict(String, SourceValue),
  name: String,
) -> Option(String) {
  case source_field(fields, name) {
    Some(SrcString(value)) -> Some(value)
    _ -> None
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      #(key, root_payload_for_field(store, field, fragments, variables))
    })
  json.object(entries)
}

fn root_payload_for_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "marketingActivity" ->
          serialize_marketing_activity_by_id(store, field, fragments, variables)
        "marketingActivities" ->
          serialize_marketing_connection(
            store.list_effective_marketing_activities(store),
            field,
            fragments,
            variables,
            marketing_types.ActivityKind,
          )
        "marketingEvent" ->
          serialize_marketing_event_by_id(store, field, fragments, variables)
        "marketingEvents" ->
          serialize_marketing_connection(
            store.list_effective_marketing_events(store),
            field,
            fragments,
            variables,
            marketing_types.EventKind,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn serialize_marketing_activity_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_marketing_activity_record_by_id(store, id) {
        Some(record) -> project_marketing_record(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_marketing_event_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_marketing_event_record_by_id(store, id) {
        Some(record) -> project_marketing_record(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_marketing_connection(
  records: List(MarketingRecord),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: marketing_types.MarketingKind,
) -> Json {
  let filtered = filter_records(records, field, variables, kind)
  let items = connection_items(filtered)
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.pagination_cursor },
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: paged,
    has_next_page: has_next,
    has_previous_page: has_previous,
  ) = window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_previous,
      get_cursor_value: fn(item, _index) { item.output_cursor },
      serialize_node: fn(item, node_field, _index) {
        project_marketing_record(item.record, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: marketing_page_info_options(),
    ),
  )
}

fn marketing_page_info_options() -> ConnectionPageInfoOptions {
  ConnectionPageInfoOptions(
    include_inline_fragments: False,
    prefix_cursors: False,
    include_cursors: True,
    fallback_start_cursor: None,
    fallback_end_cursor: None,
  )
}

fn project_marketing_record(
  record: MarketingRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        serializers.marketing_data_to_source(record.data),
        selections,
        fragments,
      )
    _ -> source_to_json(serializers.marketing_data_to_source(record.data))
  }
}

fn filter_records(
  records: List(MarketingRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  kind: marketing_types.MarketingKind,
) -> List(MarketingRecord) {
  let args = graphql_helpers.field_args(field, variables)
  let records = case kind {
    marketing_types.ActivityKind -> filter_activity_id_args(records, args)
    marketing_types.EventKind -> records
  }
  let query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let records =
    search_query_parser.apply_search_query(
      records,
      query,
      search_query_parser.default_parse_options(),
      fn(record, term) { matches_positive_marketing_term(record, term, kind) },
    )
  let sort_key = case
    graphql_helpers.read_arg_string_nonempty(args, "sortKey")
  {
    Some(key) -> key
    None ->
      case kind {
        marketing_types.ActivityKind -> "CREATED_AT"
        marketing_types.EventKind -> "ID"
      }
  }
  let sorted = sort_records(records, sort_key, kind)
  case graphql_helpers.read_arg_bool(args, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

fn filter_activity_id_args(
  records: List(MarketingRecord),
  args: Dict(String, root_field.ResolvedValue),
) -> List(MarketingRecord) {
  let records = case
    serializers.read_arg_string_list(args, "marketingActivityIds")
  {
    [] -> records
    ids -> list.filter(records, fn(record) { list.contains(ids, record.id) })
  }
  case serializers.read_arg_string_list(args, "remoteIds") {
    [] -> records
    ids ->
      list.filter(records, fn(record) {
        case serializers.marketing_remote_id(record.data) {
          Some(remote_id) -> list.contains(ids, remote_id)
          None -> False
        }
      })
  }
}

fn connection_items(
  records: List(MarketingRecord),
) -> List(marketing_types.MarketingConnectionItem) {
  list.map(records, fn(record) {
    let pagination_cursor = option.unwrap(record.cursor, record.id)
    let output_cursor = case record.cursor {
      Some(cursor) -> cursor
      None -> graphql_helpers_build_synthetic_cursor(record.id)
    }
    marketing_types.MarketingConnectionItem(
      record: record,
      pagination_cursor: pagination_cursor,
      output_cursor: output_cursor,
    )
  })
}

fn graphql_helpers_build_synthetic_cursor(id: String) -> String {
  "cursor:" <> id
}

fn sort_records(
  records: List(MarketingRecord),
  sort_key: String,
  kind: marketing_types.MarketingKind,
) -> List(MarketingRecord) {
  let normalized = string.uppercase(sort_key)
  list.sort(records, fn(left, right) {
    case normalized {
      "CREATED_AT" ->
        compare_nullable_string(
          serializers.read_marketing_string(left.data, "createdAt"),
          serializers.read_marketing_string(right.data, "createdAt"),
        )
      "STARTED_AT" ->
        compare_nullable_string(
          serializers.read_marketing_string(left.data, "startedAt"),
          serializers.read_marketing_string(right.data, "startedAt"),
        )
      "TITLE" ->
        compare_nullable_string(
          serializers.read_marketing_string(left.data, "title"),
          serializers.read_marketing_string(right.data, "title"),
        )
      _ -> {
        let _ = kind
        string.compare(left.id, right.id)
      }
    }
  })
}

fn compare_nullable_string(
  left: Option(String),
  right: Option(String),
) -> order.Order {
  string.compare(option.unwrap(left, ""), option.unwrap(right, ""))
}

fn matches_positive_marketing_term(
  record: MarketingRecord,
  term: search_query_parser.SearchQueryTerm,
  kind: marketing_types.MarketingKind,
) -> Bool {
  let data = record.data
  let field = case term.field {
    Some(raw) -> string.lowercase(raw)
    None -> "default"
  }
  case kind {
    marketing_types.ActivityKind ->
      matches_activity_term(data, record.id, field, term)
    marketing_types.EventKind ->
      matches_event_term(data, record.id, field, term)
  }
}

fn matches_activity_term(
  data: Dict(String, MarketingValue),
  id: String,
  field: String,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case field {
    "default" ->
      search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "title"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "sourceAndMedium"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        serializers.app_name(data),
        term,
      )
    "app_name" ->
      search_query_parser.matches_search_query_text(
        serializers.app_name(data),
        term,
      )
    "created_at" ->
      search_query_parser.matches_search_query_date(
        serializers.read_marketing_string(data, "createdAt"),
        term,
        1_704_067_200_000,
      )
    "id" -> matches_id_term(id, term)
    "scheduled_to_end_at" ->
      search_query_parser.matches_search_query_date(
        serializers.read_marketing_string(data, "scheduledToEndAt"),
        term,
        1_704_067_200_000,
      )
    "scheduled_to_start_at" ->
      search_query_parser.matches_search_query_date(
        serializers.read_marketing_string(data, "scheduledToStartAt"),
        term,
        1_704_067_200_000,
      )
    "tactic" ->
      search_query_parser.normalize_search_query_value(option.unwrap(
        serializers.read_marketing_string(data, "tactic"),
        "",
      ))
      == search_query_parser.normalize_search_query_value(term.value)
    "title" ->
      search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "title"),
        term,
      )
    "updated_at" ->
      search_query_parser.matches_search_query_date(
        serializers.read_marketing_string(data, "updatedAt"),
        term,
        1_704_067_200_000,
      )
    _ -> False
  }
}

fn matches_event_term(
  data: Dict(String, MarketingValue),
  id: String,
  field: String,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case field {
    "default" ->
      search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "description"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "sourceAndMedium"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "remoteId"),
        term,
      )
    "description" ->
      search_query_parser.matches_search_query_text(
        serializers.read_marketing_string(data, "description"),
        term,
      )
    "id" -> matches_id_term(id, term)
    "started_at" ->
      search_query_parser.matches_search_query_date(
        serializers.read_marketing_string(data, "startedAt"),
        term,
        1_704_067_200_000,
      )
    "type" ->
      search_query_parser.normalize_search_query_value(option.unwrap(
        serializers.read_marketing_string(data, "type"),
        "",
      ))
      == search_query_parser.normalize_search_query_value(term.value)
    _ -> False
  }
}

fn matches_id_term(
  id: String,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let numeric = serializers.id_number(id)
  case term.comparator, numeric {
    Some(_), Some(value) ->
      search_query_parser.matches_search_query_number(
        Some(int.to_float(value)),
        term,
      )
    _, _ -> {
      let expected =
        search_query_parser.normalize_search_query_value(term.value)
      string.contains(string.lowercase(id), expected)
      || string.contains(int.to_string(option.unwrap(numeric, 0)), expected)
    }
  }
}
// ===========================================================================
// Mutation path

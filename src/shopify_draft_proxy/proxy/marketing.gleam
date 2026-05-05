//// Mirrors the locally staged core of `src/proxy/marketing.ts`.
////
//// This port moves Marketing beyond the empty-read stub: activity/event
//// reads are backed by the normalized store, connection filters/sorting are
//// evaluated locally, and supported Marketing mutations stage activity,
//// event, and engagement records without runtime Shopify writes.

import gleam/dict.{type Dict}
import gleam/float
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
  project_graphql_value, serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, LogDraft, MutationOutcome, respond_to_query,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MarketingEngagementRecord, type MarketingRecord, type MarketingValue,
  MarketingBool, MarketingEngagementRecord, MarketingFloat, MarketingInt,
  MarketingList, MarketingNull, MarketingObject, MarketingRecord,
  MarketingString,
}

pub type MarketingError {
  ParseFailed(root_field.RootFieldError)
}

type MarketingKind {
  ActivityKind
  EventKind
}

const activity_id_prefix: String = "gid://shopify/MarketingActivity/"

const event_id_prefix: String = "gid://shopify/MarketingEvent/"

type CollectedMarketingRecords {
  CollectedMarketingRecords(
    activities: List(MarketingRecord),
    events: List(MarketingRecord),
  )
}

type MarketingConnectionItem {
  MarketingConnectionItem(
    record: MarketingRecord,
    pagination_cursor: String,
    output_cursor: String,
  )
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

pub fn is_marketing_mutation_root(name: String) -> Bool {
  case name {
    "marketingActivityCreate" -> True
    "marketingActivityUpdate" -> True
    "marketingActivityCreateExternal" -> True
    "marketingActivityUpdateExternal" -> True
    "marketingActivityUpsertExternal" -> True
    "marketingActivityDeleteExternal" -> True
    "marketingActivitiesDeleteAllExternal" -> True
    "marketingEngagementCreate" -> True
    "marketingEngagementsDelete" -> True
    _ -> False
  }
}

pub fn handle_marketing_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketingError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketingError) {
  use data <- result.try(handle_marketing_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
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

pub fn hydrate_marketing_from_upstream_payload(
  store: Store,
  payload: SourceValue,
) -> Store {
  let CollectedMarketingRecords(activities: activities, events: events) =
    collect_marketing_records(payload, None, empty_collected_records())
  store
  |> store.upsert_base_marketing_activities(activities)
  |> store.upsert_base_marketing_events(events)
}

fn empty_collected_records() -> CollectedMarketingRecords {
  CollectedMarketingRecords(activities: [], events: [])
}

fn collect_marketing_records(
  value: SourceValue,
  cursor: Option(String),
  collected: CollectedMarketingRecords,
) -> CollectedMarketingRecords {
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
  collected: CollectedMarketingRecords,
) -> CollectedMarketingRecords {
  let edge_cursor = source_field_string(fields, "cursor")
  let collected = case source_field(fields, "node"), edge_cursor {
    Some(node), Some(node_cursor) ->
      collect_marketing_records(node, Some(node_cursor), collected)
    _, _ -> collected
  }
  let collected = case source_field_string(fields, "id") {
    Some(id) ->
      case string.starts_with(id, activity_id_prefix) {
        True ->
          CollectedMarketingRecords(..collected, activities: [
            MarketingRecord(
              id: id,
              cursor: cursor,
              data: source_object_to_marketing_data(fields),
            ),
            ..collected.activities
          ])
        False ->
          case string.starts_with(id, event_id_prefix) {
            True ->
              CollectedMarketingRecords(..collected, events: [
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
            ActivityKind,
          )
        "marketingEvent" ->
          serialize_marketing_event_by_id(store, field, fragments, variables)
        "marketingEvents" ->
          serialize_marketing_connection(
            store.list_effective_marketing_events(store),
            field,
            fragments,
            variables,
            EventKind,
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
  kind: MarketingKind,
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
        marketing_data_to_source(record.data),
        selections,
        fragments,
      )
    _ -> source_to_json(marketing_data_to_source(record.data))
  }
}

fn filter_records(
  records: List(MarketingRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  kind: MarketingKind,
) -> List(MarketingRecord) {
  let args = graphql_helpers.field_args(field, variables)
  let records = case kind {
    ActivityKind -> filter_activity_id_args(records, args)
    EventKind -> records
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
        ActivityKind -> "CREATED_AT"
        EventKind -> "ID"
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
  let records = case read_arg_string_list(args, "marketingActivityIds") {
    [] -> records
    ids -> list.filter(records, fn(record) { list.contains(ids, record.id) })
  }
  case read_arg_string_list(args, "remoteIds") {
    [] -> records
    ids ->
      list.filter(records, fn(record) {
        case marketing_remote_id(record.data) {
          Some(remote_id) -> list.contains(ids, remote_id)
          None -> False
        }
      })
  }
}

fn connection_items(
  records: List(MarketingRecord),
) -> List(MarketingConnectionItem) {
  list.map(records, fn(record) {
    let pagination_cursor = option.unwrap(record.cursor, record.id)
    let output_cursor = case record.cursor {
      Some(cursor) -> cursor
      None -> graphql_helpers_build_synthetic_cursor(record.id)
    }
    MarketingConnectionItem(
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
  kind: MarketingKind,
) -> List(MarketingRecord) {
  let normalized = string.uppercase(sort_key)
  list.sort(records, fn(left, right) {
    case normalized {
      "CREATED_AT" ->
        compare_nullable_string(
          read_marketing_string(left.data, "createdAt"),
          read_marketing_string(right.data, "createdAt"),
        )
      "STARTED_AT" ->
        compare_nullable_string(
          read_marketing_string(left.data, "startedAt"),
          read_marketing_string(right.data, "startedAt"),
        )
      "TITLE" ->
        compare_nullable_string(
          read_marketing_string(left.data, "title"),
          read_marketing_string(right.data, "title"),
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
  kind: MarketingKind,
) -> Bool {
  let data = record.data
  let field = case term.field {
    Some(raw) -> string.lowercase(raw)
    None -> "default"
  }
  case kind {
    ActivityKind -> matches_activity_term(data, record.id, field, term)
    EventKind -> matches_event_term(data, record.id, field, term)
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
        read_marketing_string(data, "title"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        read_marketing_string(data, "sourceAndMedium"),
        term,
      )
      || search_query_parser.matches_search_query_text(app_name(data), term)
    "app_name" ->
      search_query_parser.matches_search_query_text(app_name(data), term)
    "created_at" ->
      search_query_parser.matches_search_query_date(
        read_marketing_string(data, "createdAt"),
        term,
        1_704_067_200_000,
      )
    "id" -> matches_id_term(id, term)
    "scheduled_to_end_at" ->
      search_query_parser.matches_search_query_date(
        read_marketing_string(data, "scheduledToEndAt"),
        term,
        1_704_067_200_000,
      )
    "scheduled_to_start_at" ->
      search_query_parser.matches_search_query_date(
        read_marketing_string(data, "scheduledToStartAt"),
        term,
        1_704_067_200_000,
      )
    "tactic" ->
      search_query_parser.normalize_search_query_value(option.unwrap(
        read_marketing_string(data, "tactic"),
        "",
      ))
      == search_query_parser.normalize_search_query_value(term.value)
    "title" ->
      search_query_parser.matches_search_query_text(
        read_marketing_string(data, "title"),
        term,
      )
    "updated_at" ->
      search_query_parser.matches_search_query_date(
        read_marketing_string(data, "updatedAt"),
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
        read_marketing_string(data, "description"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        read_marketing_string(data, "sourceAndMedium"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        read_marketing_string(data, "remoteId"),
        term,
      )
    "description" ->
      search_query_parser.matches_search_query_text(
        read_marketing_string(data, "description"),
        term,
      )
    "id" -> matches_id_term(id, term)
    "started_at" ->
      search_query_parser.matches_search_query_date(
        read_marketing_string(data, "startedAt"),
        term,
        1_704_067_200_000,
      )
    "type" ->
      search_query_parser.normalize_search_query_value(option.unwrap(
        read_marketing_string(data, "type"),
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
  let numeric = id_number(id)
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
// ===========================================================================

type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

type EngagementIdentifier {
  ActivityIdentifier(value: String, activity: MarketingRecord)
  RemoteIdentifier(value: String, activity: MarketingRecord)
  ChannelIdentifier(value: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: SourceValue,
    staged_resource_ids: List(String),
    should_log: Bool,
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(store, identity, fields, fragments, variables)
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [], False)
  let #(entries, final_store, final_identity, staged_ids, should_log) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, should_log) =
        acc
      case field {
        Field(name: name, ..) ->
          case is_marketing_mutation_root(name.value) {
            False -> acc
            True -> {
              let #(result, next_store, next_identity) =
                handle_marketing_mutation_root(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              #(
                list.append(entries, [
                  #(
                    result.key,
                    project_payload(result.payload, field, fragments),
                  ),
                ]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                should_log || result.should_log,
              )
            }
          }
        _ -> acc
      }
    })

  let root_names = mutation_root_names(fields)
  let final_ids = dedupe_strings(staged_ids)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let log_drafts = case should_log {
    False -> []
    True -> [
      LogDraft(
        operation_name: primary_root,
        root_fields: root_names,
        primary_root_field: primary_root,
        domain: "marketing",
        execution: "stage-locally",
        query: None,
        variables: None,
        staged_resource_ids: final_ids,
        status: store.Staged,
        notes: Some("Staged locally in the in-memory marketing draft store."),
      ),
    ]
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_ids,
    log_drafts: log_drafts,
  )
}

fn handle_marketing_mutation_root(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let _ = fragments
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case field {
    Field(name: name, ..) ->
      case name.value {
        "marketingActivityCreate" ->
          marketing_activity_create(store, identity, key, args)
        "marketingActivityUpdate" ->
          marketing_activity_update(store, identity, key, args)
        "marketingActivityCreateExternal" ->
          marketing_activity_create_external(store, identity, key, args)
        "marketingActivityUpdateExternal" ->
          marketing_activity_update_external(store, identity, key, args)
        "marketingActivityUpsertExternal" ->
          marketing_activity_upsert_external(store, identity, key, args)
        "marketingActivityDeleteExternal" ->
          marketing_activity_delete_external(store, identity, key, args)
        "marketingActivitiesDeleteAllExternal" ->
          marketing_activities_delete_all_external(store, identity, key)
        "marketingEngagementCreate" ->
          marketing_engagement_create(store, identity, key, args)
        "marketingEngagementsDelete" ->
          marketing_engagements_delete(store, identity, key, args)
        _ -> #(
          MutationFieldResult(key, src_object([]), [], False),
          store,
          identity,
        )
      }
    _ -> #(MutationFieldResult(key, src_object([]), [], False), store, identity)
  }
}

fn marketing_activity_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    is_known_local_marketing_activity_extension(read_value_string(
      input,
      "marketingActivityExtensionId",
    ))
  {
    False -> #(
      MutationFieldResult(
        key,
        src_object([
          #(
            "userErrors",
            user_errors_source([missing_marketing_extension_error()]),
          ),
        ]),
        [],
        False,
      ),
      store,
      identity,
    )
    True -> {
      let #(activity, next_identity) =
        build_native_marketing_activity_from_create_input(identity, input)
      let #(staged, next_store) =
        store.stage_marketing_activity(store, activity)
      #(
        MutationFieldResult(
          key,
          src_object([#("userErrors", user_errors_source([]))]),
          [staged.id],
          True,
        ),
        next_store,
        next_identity,
      )
    }
  }
}

fn marketing_activity_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case read_value_string(input, "id") {
    Some(id) ->
      case store.get_effective_marketing_activity_record_by_id(store, id) {
        None -> marketing_missing_activity_result(key, store, identity)
        Some(activity) -> {
          let #(updated, next_identity) =
            apply_native_marketing_activity_update(identity, activity, input)
          let #(staged, next_store) =
            store.stage_marketing_activity(store, updated)
          #(
            MutationFieldResult(
              key,
              src_object([
                #("marketingActivity", marketing_data_to_source(staged.data)),
                #("redirectPath", SrcNull),
                #("userErrors", user_errors_source([])),
              ]),
              [staged.id],
              True,
            ),
            next_store,
            next_identity,
          )
        }
      }
    None -> marketing_missing_activity_result(key, store, identity)
  }
}

fn marketing_activity_create_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case has_attribution(input) {
    False ->
      validation_result(
        key,
        "marketingActivityCreateExternal",
        [non_hierarchical_utm_error()],
        store,
        identity,
      )
    True ->
      case read_value_string(input, "remoteId") {
        Some(remote_id) ->
          case
            store.get_effective_marketing_activity_by_remote_id(
              store,
              remote_id,
            )
          {
            Some(_) ->
              validation_result(
                key,
                "marketingActivityCreateExternal",
                [duplicate_external_activity_error()],
                store,
                identity,
              )
            None ->
              create_external_activity_success(store, identity, key, input)
          }
        None -> create_external_activity_success(store, identity, key, input)
      }
  }
}

fn marketing_activity_update_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let selector_utm =
    read_utm(
      graphql_helpers.read_arg_object(args, "utm") |> option.unwrap(dict.new()),
    )
  let activity = case
    graphql_helpers.read_arg_string_nonempty(args, "remoteId")
  {
    Some(remote_id) ->
      store.get_effective_marketing_activity_by_remote_id(store, remote_id)
    None ->
      case
        graphql_helpers.read_arg_string_nonempty(args, "marketingActivityId")
      {
        Some(id) ->
          store.get_effective_marketing_activity_record_by_id(store, id)
        None -> find_marketing_activity_by_utm(store, selector_utm)
      }
  }
  case activity {
    None ->
      validation_result(
        key,
        "marketingActivityUpdateExternal",
        [marketing_activity_missing_error()],
        store,
        identity,
      )
    Some(activity) -> {
      let requested_utm =
        graphql_helpers.read_arg_object(args, "utm")
        |> option.unwrap(dict.new())
      case
        dict.is_empty(requested_utm)
        || same_utm(
          read_marketing_object(activity.data, "utmParameters"),
          read_utm(requested_utm),
        )
      {
        False ->
          validation_result(
            key,
            "marketingActivityUpdateExternal",
            [immutable_utm_error()],
            store,
            identity,
          )
        True ->
          update_external_activity_success(
            store,
            identity,
            key,
            activity,
            input,
          )
      }
    }
  }
}

fn marketing_activity_upsert_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let existing = case read_value_string(input, "remoteId") {
    Some(remote_id) ->
      store.get_effective_marketing_activity_by_remote_id(store, remote_id)
    None -> None
  }
  case existing {
    None ->
      case has_attribution(input) {
        False ->
          validation_result(
            key,
            "marketingActivityUpsertExternal",
            [non_hierarchical_utm_error()],
            store,
            identity,
          )
        True -> create_external_activity_success(store, identity, key, input)
      }
    Some(activity) ->
      case
        same_utm(
          read_marketing_object(activity.data, "utmParameters"),
          read_utm(input),
        )
      {
        False ->
          validation_result(
            key,
            "marketingActivityUpsertExternal",
            [immutable_utm_error()],
            store,
            identity,
          )
        True ->
          update_external_activity_success(
            store,
            identity,
            key,
            activity,
            input,
          )
      }
  }
}

fn marketing_activity_delete_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let activity = case
    graphql_helpers.read_arg_string_nonempty(args, "remoteId")
  {
    Some(remote_id) ->
      store.get_effective_marketing_activity_by_remote_id(store, remote_id)
    None ->
      case
        graphql_helpers.read_arg_string_nonempty(args, "marketingActivityId")
      {
        Some(id) ->
          store.get_effective_marketing_activity_record_by_id(store, id)
        None -> None
      }
  }
  case activity {
    None ->
      validation_result(
        key,
        "marketingActivityDeleteExternal",
        [marketing_activity_missing_error()],
        store,
        identity,
      )
    Some(activity) -> {
      let next_store = store.stage_delete_marketing_activity(store, activity.id)
      #(
        MutationFieldResult(
          key,
          src_object([
            #("deletedMarketingActivityId", SrcString(activity.id)),
            #("userErrors", user_errors_source([])),
          ]),
          [activity.id],
          True,
        ),
        next_store,
        identity,
      )
    }
  }
}

fn marketing_activities_delete_all_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(deleted_ids, next_store) =
    store.stage_delete_all_external_marketing_activities(store)
  let #(job_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Job")
  #(
    MutationFieldResult(
      key,
      src_object([
        #(
          "job",
          src_object([
            #("__typename", SrcString("Job")),
            #("id", SrcString(job_id)),
            #("done", SrcBool(False)),
          ]),
        ),
        #("userErrors", user_errors_source([])),
      ]),
      [job_id, ..deleted_ids],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn marketing_engagement_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "marketingEngagement")
    |> option.unwrap(dict.new())
  case validate_engagement_input_currency(input) {
    Error(user_error) ->
      validation_result(
        key,
        "marketingEngagementCreate",
        [user_error],
        store,
        identity,
      )
    Ok(engagement_currency_code) ->
      case resolve_marketing_engagement_identifier(store, args) {
        Error(user_error) ->
          validation_result(
            key,
            "marketingEngagementCreate",
            [user_error],
            store,
            identity,
          )
        Ok(identifier) ->
          case
            validate_engagement_activity_currency(
              identifier,
              engagement_currency_code,
            )
          {
            Error(user_error) ->
              validation_result(
                key,
                "marketingEngagementCreate",
                [user_error],
                store,
                identity,
              )
            Ok(Nil) -> {
              let engagement =
                build_marketing_engagement_record(identifier, input)
              let #(staged, next_store) =
                store.stage_marketing_engagement(store, engagement)
              #(
                MutationFieldResult(
                  key,
                  src_object([
                    #(
                      "marketingEngagement",
                      marketing_data_to_source(staged.data),
                    ),
                    #("userErrors", user_errors_source([])),
                  ]),
                  [staged.id],
                  True,
                ),
                next_store,
                identity,
              )
            }
          }
      }
  }
}

fn marketing_engagements_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let channel_handle =
    graphql_helpers.read_arg_string_nonempty(args, "channelHandle")
  let delete_all =
    option.unwrap(
      graphql_helpers.read_arg_bool(args, "deleteEngagementsForAllChannels"),
      False,
    )
  case channel_handle, delete_all {
    Some(_), True ->
      validation_result(
        key,
        "marketingEngagementsDelete",
        [invalid_delete_engagements_arguments_error()],
        store,
        identity,
      )
    None, False ->
      validation_result(
        key,
        "marketingEngagementsDelete",
        [invalid_delete_engagements_arguments_error()],
        store,
        identity,
      )
    Some(handle), False ->
      case store.has_known_marketing_channel_handle(store, handle) {
        False ->
          validation_result(
            key,
            "marketingEngagementsDelete",
            [invalid_channel_handle_error()],
            store,
            identity,
          )
        True -> {
          let #(deleted_ids, next_store) =
            store.stage_delete_marketing_engagements_by_channel_handle(
              store,
              handle,
            )
          #(
            MutationFieldResult(
              key,
              src_object([
                #(
                  "result",
                  SrcString(
                    "Engagement data marked for deletion for 1 channel(s)",
                  ),
                ),
                #("userErrors", user_errors_source([])),
              ]),
              deleted_ids,
              True,
            ),
            next_store,
            identity,
          )
        }
      }
    None, True -> {
      let channel_count =
        store.list_effective_marketing_engagements(store)
        |> list.filter_map(fn(engagement) {
          case engagement.channel_handle {
            Some(handle) -> Ok(handle)
            None -> Error(Nil)
          }
        })
        |> dedupe_strings
        |> list.length
      let #(deleted_ids, next_store) =
        store.stage_delete_all_channel_marketing_engagements(store)
      #(
        MutationFieldResult(
          key,
          src_object([
            #(
              "result",
              SrcString(
                "Engagement data marked for deletion for "
                <> int.to_string(channel_count)
                <> " channel(s)",
              ),
            ),
            #("userErrors", user_errors_source([])),
          ]),
          deleted_ids,
          True,
        ),
        next_store,
        identity,
      )
    }
  }
}

fn create_external_activity_success(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(activity, event, next_identity) =
    build_marketing_records_from_create_input(identity, input)
  let #(_, next_store) = store.stage_marketing_event(store, event)
  let #(staged_activity, next_store) =
    store.stage_marketing_activity(next_store, activity)
  #(
    MutationFieldResult(
      key,
      src_object([
        #("marketingActivity", marketing_data_to_source(staged_activity.data)),
        #("userErrors", user_errors_source([])),
      ]),
      [staged_activity.id, event.id],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn update_external_activity_success(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  activity: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(updated_activity, event, next_identity) =
    apply_external_activity_update(identity, activity, input)
  let #(_, next_store) = store.stage_marketing_event(store, event)
  let #(staged_activity, next_store) =
    store.stage_marketing_activity(next_store, updated_activity)
  #(
    MutationFieldResult(
      key,
      src_object([
        #("marketingActivity", marketing_data_to_source(staged_activity.data)),
        #("userErrors", user_errors_source([])),
      ]),
      [staged_activity.id, event.id],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn marketing_missing_activity_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key,
      src_object([
        #("marketingActivity", SrcNull),
        #("redirectPath", SrcNull),
        #(
          "userErrors",
          user_errors_source([marketing_activity_missing_error()]),
        ),
      ]),
      [],
      False,
    ),
    store,
    identity,
  )
}

fn validation_result(
  key: String,
  root_field: String,
  user_errors: List(UserError),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key,
      marketing_validation_payload(root_field, user_errors),
      [],
      False,
    ),
    store,
    identity,
  )
}

fn marketing_validation_payload(
  root_field: String,
  user_errors: List(UserError),
) -> SourceValue {
  case root_field {
    "marketingEngagementCreate" ->
      src_object([
        #("marketingEngagement", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingEngagementsDelete" ->
      src_object([
        #("result", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingActivityDeleteExternal" ->
      src_object([
        #("deletedMarketingActivityId", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingActivitiesDeleteAllExternal" ->
      src_object([
        #("job", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    _ ->
      src_object([
        #("marketingActivity", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
  }
}

fn user_errors_source(user_errors: List(UserError)) -> SourceValue {
  SrcList(list.map(user_errors, user_error_source))
}

fn user_error_source(user_error: UserError) -> SourceValue {
  src_object([
    #("field", optional_string_list_source(user_error.field)),
    #("message", SrcString(user_error.message)),
    #("code", graphql_helpers.option_string_source(user_error.code)),
  ])
}

fn non_hierarchical_utm_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.",
    code: Some("NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"),
  )
}

fn marketing_activity_missing_error() -> UserError {
  UserError(
    field: None,
    message: "Marketing activity does not exist.",
    code: Some("MARKETING_ACTIVITY_DOES_NOT_EXIST"),
  )
}

fn immutable_utm_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "UTM parameters cannot be modified.",
    code: Some("IMMUTABLE_UTM_PARAMETERS"),
  )
}

fn duplicate_external_activity_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Validation failed: Remote ID has already been taken, Utm campaign has already been taken",
    code: None,
  )
}

fn missing_marketing_extension_error() -> UserError {
  UserError(
    field: Some(["input", "marketingActivityExtensionId"]),
    message: "Could not find the marketing extension",
    code: None,
  )
}

fn engagement_missing_identifier_error() -> UserError {
  UserError(
    field: None,
    message: "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
    code: Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"),
  )
}

fn engagement_invalid_identifier_error() -> UserError {
  UserError(
    field: None,
    message: "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
    code: Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"),
  )
}

fn invalid_channel_handle_error() -> UserError {
  UserError(
    field: Some(["channelHandle"]),
    message: "The channel handle is not recognized. Please contact your partner manager for more information.",
    code: Some("INVALID_CHANNEL_HANDLE"),
  )
}

fn invalid_delete_engagements_arguments_error() -> UserError {
  UserError(
    field: None,
    message: "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
    code: Some("INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"),
  )
}

fn currency_code_mismatch_input_error() -> UserError {
  UserError(
    field: Some(["marketingEngagement"]),
    message: "Currency codes in the marketing engagement input do not match.",
    code: Some("CURRENCY_CODE_MISMATCH_INPUT"),
  )
}

fn marketing_activity_currency_code_mismatch_error() -> UserError {
  UserError(
    field: Some(["marketingEngagement"]),
    message: "Marketing activity currency code does not match the currency code in the marketing engagement input.",
    code: Some("MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"),
  )
}

// ===========================================================================
// Record builders
// ===========================================================================

fn build_marketing_records_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, MarketingRecord, SyntheticIdentityRegistry) {
  let #(activity_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingActivity")
  let #(event_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingEvent")
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title = option.unwrap(read_value_string(input, "title"), "")
  let remote_id = read_value_string(input, "remoteId")
  let status = option.unwrap(read_value_string(input, "status"), "UNDEFINED")
  let tactic = option.unwrap(read_value_string(input, "tactic"), "NEWSLETTER")
  let channel_type =
    option.unwrap(read_value_string(input, "marketingChannelType"), "EMAIL")
  let source_medium = source_and_medium(channel_type, tactic)
  let utm = read_utm(input)
  let started_at =
    option.unwrap(
      read_value_string(input, "start")
        |> option.or(read_value_string(input, "scheduledStart")),
      timestamp,
    )
  let ended_at =
    read_value_string(input, "end")
    |> option.or(event_ended_at_for_status(status, timestamp))
  let event_data =
    dict.from_list([
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #("legacyResourceId", MarketingInt(option.unwrap(id_number(event_id), 0))),
      #("type", MarketingString(tactic)),
      #("remoteId", optional_marketing_string(remote_id)),
      #("startedAt", MarketingString(started_at)),
      #("endedAt", optional_marketing_string(ended_at)),
      #(
        "scheduledToEndAt",
        optional_marketing_string(read_value_string(input, "scheduledEnd")),
      ),
      #(
        "manageUrl",
        optional_marketing_string(read_value_string(input, "remoteUrl")),
      ),
      #(
        "previewUrl",
        optional_marketing_string(read_value_string(
          input,
          "remotePreviewImageUrl",
        )),
      ),
      #(
        "utmCampaign",
        optional_marketing_string(read_marketing_object_string(utm, "campaign")),
      ),
      #(
        "utmMedium",
        optional_marketing_string(read_marketing_object_string(utm, "medium")),
      ),
      #(
        "utmSource",
        optional_marketing_string(read_marketing_object_string(utm, "source")),
      ),
      #("description", MarketingString(title)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "channelHandle",
        optional_marketing_string(read_value_string(input, "channelHandle")),
      ),
    ])
  let activity_data =
    dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(activity_id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString(timestamp)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #("isExternal", MarketingBool(True)),
      #("inMainWorkflowVersion", MarketingBool(False)),
      #(
        "urlParameterValue",
        optional_marketing_string(read_value_string(input, "urlParameterValue")),
      ),
      #(
        "parentActivityId",
        optional_marketing_string(read_value_string(input, "parentActivityId")),
      ),
      #(
        "parentRemoteId",
        optional_marketing_string(read_value_string(input, "parentRemoteId")),
      ),
      #(
        "hierarchyLevel",
        optional_marketing_string(read_value_string(input, "hierarchyLevel")),
      ),
      #("remoteId", optional_marketing_string(remote_id)),
      #(
        "currencyCode",
        optional_marketing_string(activity_input_currency(input)),
      ),
      #("utmParameters", optional_marketing_object(utm)),
      #("marketingEvent", MarketingObject(event_data)),
    ])
  #(
    MarketingRecord(id: activity_id, cursor: None, data: activity_data),
    MarketingRecord(id: event_id, cursor: None, data: event_data),
    identity,
  )
}

fn build_native_marketing_activity_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, SyntheticIdentityRegistry) {
  let #(activity_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingActivity")
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let status = option.unwrap(read_value_string(input, "status"), "UNDEFINED")
  let title =
    option.unwrap(
      read_value_string(input, "marketingActivityTitle")
        |> option.or(read_value_string(input, "title")),
      "Marketing activity",
    )
  let tactic = option.unwrap(read_value_string(input, "tactic"), "NEWSLETTER")
  let channel_type =
    option.unwrap(read_value_string(input, "marketingChannelType"), "EMAIL")
  let source_medium = source_and_medium(channel_type, tactic)
  let data =
    dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(activity_id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString(timestamp)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #("isExternal", MarketingBool(False)),
      #("inMainWorkflowVersion", MarketingBool(True)),
      #(
        "urlParameterValue",
        optional_marketing_string(read_value_string(input, "urlParameterValue")),
      ),
      #(
        "parentActivityId",
        optional_marketing_string(read_value_string(input, "parentActivityId")),
      ),
      #(
        "parentRemoteId",
        optional_marketing_string(read_value_string(input, "parentRemoteId")),
      ),
      #(
        "hierarchyLevel",
        optional_marketing_string(read_value_string(input, "hierarchyLevel")),
      ),
      #(
        "marketingActivityExtensionId",
        optional_marketing_string(read_value_string(
          input,
          "marketingActivityExtensionId",
        )),
      ),
      #(
        "context",
        optional_marketing_string(read_value_string(input, "context")),
      ),
      #(
        "formData",
        optional_marketing_string(read_value_string(input, "formData")),
      ),
      #(
        "currencyCode",
        optional_marketing_string(activity_input_currency(input)),
      ),
      #("utmParameters", optional_marketing_object(read_utm(input))),
      #("marketingEvent", MarketingNull),
    ])
  #(MarketingRecord(id: activity_id, cursor: None, data: data), identity)
}

fn apply_native_marketing_activity_update(
  identity: SyntheticIdentityRegistry,
  record: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let status =
    option.unwrap(
      read_value_string(input, "status")
        |> option.or(read_marketing_string(record.data, "status")),
      "UNDEFINED",
    )
  let tactic =
    option.unwrap(
      read_value_string(input, "tactic")
        |> option.or(read_marketing_string(record.data, "tactic")),
      "NEWSLETTER",
    )
  let channel_type =
    option.unwrap(
      read_value_string(input, "marketingChannelType")
        |> option.or(read_marketing_string(record.data, "marketingChannelType")),
      "EMAIL",
    )
  let title =
    option.unwrap(
      read_value_string(input, "marketingActivityTitle")
        |> option.or(read_value_string(input, "title"))
        |> option.or(read_marketing_string(record.data, "title")),
      "Marketing activity",
    )
  let source_medium = source_and_medium(channel_type, tactic)
  let data =
    overlay_marketing_data(record.data, [
      #("title", MarketingString(title)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "urlParameterValue",
        optional_marketing_string(
          read_value_string(input, "urlParameterValue")
          |> option.or(read_marketing_string(record.data, "urlParameterValue")),
        ),
      ),
      #(
        "context",
        optional_marketing_string(
          read_value_string(input, "context")
          |> option.or(read_marketing_string(record.data, "context")),
        ),
      ),
      #(
        "formData",
        optional_marketing_string(
          read_value_string(input, "formData")
          |> option.or(read_marketing_string(record.data, "formData")),
        ),
      ),
      #(
        "utmParameters",
        optional_marketing_object(
          read_utm(input)
          |> option.or(read_marketing_object(record.data, "utmParameters")),
        ),
      ),
      #(
        "currencyCode",
        optional_marketing_string(
          activity_input_currency(input)
          |> option.or(read_marketing_string(record.data, "currencyCode")),
        ),
      ),
    ])
  #(MarketingRecord(..record, data: data), identity)
}

fn apply_external_activity_update(
  identity: SyntheticIdentityRegistry,
  record: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, MarketingRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let existing_event =
    option.unwrap(
      read_marketing_object(record.data, "marketingEvent"),
      dict.new(),
    )
  let #(event_id, identity) = case
    read_marketing_object_string(Some(existing_event), "id")
  {
    Some(id) -> #(id, identity)
    None -> synthetic_identity.make_synthetic_gid(identity, "MarketingEvent")
  }
  let status =
    option.unwrap(
      read_value_string(input, "status")
        |> option.or(read_marketing_string(record.data, "status")),
      "UNDEFINED",
    )
  let tactic =
    option.unwrap(
      read_value_string(input, "tactic")
        |> option.or(read_marketing_string(record.data, "tactic")),
      "NEWSLETTER",
    )
  let channel_type =
    option.unwrap(
      read_value_string(input, "marketingChannelType")
        |> option.or(read_marketing_string(record.data, "marketingChannelType")),
      "EMAIL",
    )
  let title =
    option.unwrap(
      read_value_string(input, "title")
        |> option.or(read_marketing_string(record.data, "title")),
      "",
    )
  let source_medium = source_and_medium(channel_type, tactic)
  let existing_utm = read_marketing_object(record.data, "utmParameters")
  let ended_at =
    read_value_string(input, "end")
    |> option.or({
      case
        status
        == option.unwrap(read_marketing_string(record.data, "status"), "")
      {
        True -> read_marketing_object_string(Some(existing_event), "endedAt")
        False -> event_ended_at_for_status(status, timestamp)
      }
    })
  let event_data =
    overlay_marketing_data(existing_event, [
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #("legacyResourceId", MarketingInt(option.unwrap(id_number(event_id), 0))),
      #("type", MarketingString(tactic)),
      #("remoteId", optional_marketing_string(marketing_remote_id(record.data))),
      #(
        "startedAt",
        MarketingString(option.unwrap(
          read_value_string(input, "start")
            |> option.or(read_value_string(input, "scheduledStart"))
            |> option.or(read_marketing_object_string(
              Some(existing_event),
              "startedAt",
            )),
          timestamp,
        )),
      ),
      #("endedAt", optional_marketing_string(ended_at)),
      #(
        "scheduledToEndAt",
        optional_marketing_string(
          read_value_string(input, "scheduledEnd")
          |> option.or(read_marketing_object_string(
            Some(existing_event),
            "scheduledToEndAt",
          )),
        ),
      ),
      #(
        "manageUrl",
        optional_marketing_string(
          read_value_string(input, "remoteUrl")
          |> option.or(read_marketing_object_string(
            Some(existing_event),
            "manageUrl",
          )),
        ),
      ),
      #(
        "previewUrl",
        optional_marketing_string(
          read_value_string(input, "remotePreviewImageUrl")
          |> option.or(read_marketing_object_string(
            Some(existing_event),
            "previewUrl",
          )),
        ),
      ),
      #(
        "utmCampaign",
        optional_marketing_string(read_marketing_object_string(
          existing_utm,
          "campaign",
        )),
      ),
      #(
        "utmMedium",
        optional_marketing_string(read_marketing_object_string(
          existing_utm,
          "medium",
        )),
      ),
      #(
        "utmSource",
        optional_marketing_string(read_marketing_object_string(
          existing_utm,
          "source",
        )),
      ),
      #("description", MarketingString(title)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
    ])
  let activity_data =
    overlay_marketing_data(record.data, [
      #("title", MarketingString(title)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "currencyCode",
        optional_marketing_string(
          activity_input_currency(input)
          |> option.or(read_marketing_string(record.data, "currencyCode")),
        ),
      ),
      #("marketingEvent", MarketingObject(event_data)),
    ])
  #(
    MarketingRecord(..record, data: activity_data),
    MarketingRecord(id: event_id, cursor: None, data: event_data),
    identity,
  )
}

fn build_marketing_engagement_record(
  identifier: EngagementIdentifier,
  input: Dict(String, root_field.ResolvedValue),
) -> MarketingEngagementRecord {
  let occurred_on = option.unwrap(read_value_string(input, "occurredOn"), "")
  let activity = engagement_activity(identifier)
  let channel_handle = case identifier {
    ChannelIdentifier(value) -> Some(value)
    _ -> None
  }
  let data =
    dict.from_list([
      #("__typename", MarketingString("MarketingEngagement")),
      #("occurredOn", MarketingString(occurred_on)),
      #(
        "utcOffset",
        MarketingString(option.unwrap(
          read_value_string(input, "utcOffset"),
          "+00:00",
        )),
      ),
      #(
        "isCumulative",
        MarketingBool(option.unwrap(
          read_value_bool(input, "isCumulative"),
          False,
        )),
      ),
      #("channelHandle", optional_marketing_string(channel_handle)),
      #(
        "marketingActivity",
        optional_marketing_object(option.map(activity, fn(a) { a.data })),
      ),
    ])
    |> overlay_marketing_data(integer_engagement_entries(input))
    |> overlay_marketing_data(money_engagement_entries(input))
    |> overlay_marketing_data(decimal_engagement_entries(input))
  MarketingEngagementRecord(
    id: engagement_record_id(identifier, occurred_on),
    marketing_activity_id: option.map(activity, fn(a) { a.id }),
    remote_id: case identifier {
      RemoteIdentifier(value, ..) -> Some(value)
      _ ->
        option.flatten(
          option.map(activity, fn(a) { marketing_remote_id(a.data) }),
        )
    },
    channel_handle: channel_handle,
    occurred_on: occurred_on,
    data: data,
  )
}

fn resolve_marketing_engagement_identifier(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> Result(EngagementIdentifier, UserError) {
  let marketing_activity_id =
    graphql_helpers.read_arg_string_nonempty(args, "marketingActivityId")
  let remote_id = graphql_helpers.read_arg_string_nonempty(args, "remoteId")
  let channel_handle =
    graphql_helpers.read_arg_string_nonempty(args, "channelHandle")
  let count =
    option_count(marketing_activity_id)
    + option_count(remote_id)
    + option_count(channel_handle)
  case count {
    0 -> Error(engagement_missing_identifier_error())
    n if n > 1 -> Error(engagement_invalid_identifier_error())
    _ ->
      case marketing_activity_id, remote_id, channel_handle {
        Some(id), _, _ ->
          case store.get_effective_marketing_activity_record_by_id(store, id) {
            Some(activity) -> Ok(ActivityIdentifier(id, activity))
            None -> Error(marketing_activity_missing_error())
          }
        _, Some(remote_id), _ ->
          case
            store.get_effective_marketing_activity_by_remote_id(
              store,
              remote_id,
            )
          {
            Some(activity) -> Ok(RemoteIdentifier(remote_id, activity))
            None -> Error(marketing_activity_missing_error())
          }
        _, _, Some(handle) ->
          case store.has_known_marketing_channel_handle(store, handle) {
            True -> Ok(ChannelIdentifier(handle))
            False -> Error(invalid_channel_handle_error())
          }
        _, _, _ -> Error(engagement_missing_identifier_error())
      }
  }
}

fn validate_engagement_input_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Result(Option(String), UserError) {
  let ad_spend_currency = money_input_currency(input, "adSpend")
  let sales_currency = money_input_currency(input, "sales")
  case ad_spend_currency, sales_currency {
    Some(ad_spend_currency), Some(sales_currency)
      if ad_spend_currency != sales_currency
    -> Error(currency_code_mismatch_input_error())
    Some(currency), _ -> Ok(Some(currency))
    _, Some(currency) -> Ok(Some(currency))
    _, _ -> Ok(None)
  }
}

fn validate_engagement_activity_currency(
  identifier: EngagementIdentifier,
  engagement_currency_code: Option(String),
) -> Result(Nil, UserError) {
  case engagement_currency_code, engagement_activity(identifier) {
    Some(engagement_currency_code), Some(activity) ->
      case marketing_activity_currency(activity.data) {
        Some(activity_currency_code)
          if activity_currency_code != engagement_currency_code
        -> Error(marketing_activity_currency_code_mismatch_error())
        _ -> Ok(Nil)
      }
    _, _ -> Ok(Nil)
  }
}

fn engagement_activity(
  identifier: EngagementIdentifier,
) -> Option(MarketingRecord) {
  case identifier {
    ActivityIdentifier(activity: activity, ..) -> Some(activity)
    RemoteIdentifier(activity: activity, ..) -> Some(activity)
    ChannelIdentifier(..) -> None
  }
}

fn engagement_record_id(
  identifier: EngagementIdentifier,
  occurred_on: String,
) -> String {
  let target = case identifier {
    ChannelIdentifier(value) -> "channel:" <> value
    ActivityIdentifier(activity: activity, ..) -> "activity:" <> activity.id
    RemoteIdentifier(activity: activity, ..) -> "activity:" <> activity.id
  }
  "gid://shopify/MarketingEngagement/"
  <> url_encode(target <> ":" <> occurred_on)
}

fn integer_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  let fields = [
    "impressionsCount",
    "viewsCount",
    "clicksCount",
    "sharesCount",
    "favoritesCount",
    "commentsCount",
    "unsubscribesCount",
    "complaintsCount",
    "failsCount",
    "sendsCount",
    "uniqueViewsCount",
    "uniqueClicksCount",
    "sessionsCount",
  ]
  list.filter_map(fields, fn(field) {
    case read_value_int(input, field) {
      Some(value) -> Ok(#(field, MarketingInt(value)))
      None -> Error(Nil)
    }
  })
}

fn money_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  ["adSpend", "sales"]
  |> list.filter_map(fn(field) {
    case read_money_input(input, field) {
      Some(value) -> Ok(#(field, MarketingObject(value)))
      None -> Error(Nil)
    }
  })
}

fn activity_input_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  money_input_currency(input, "budget")
  |> option.or(budget_total_currency(input))
  |> option.or(money_input_currency(input, "adSpend"))
}

fn budget_total_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(input, "budget") {
    Ok(root_field.ObjectVal(budget)) -> money_input_currency(budget, "total")
    _ -> None
  }
}

fn money_input_currency(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(String) {
  case dict.get(input, field) {
    Ok(root_field.ObjectVal(money)) -> read_value_string(money, "currencyCode")
    _ -> None
  }
}

fn marketing_activity_currency(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  read_marketing_string(data, "currencyCode")
  |> option.or(
    read_marketing_object(data, "adSpend")
    |> read_marketing_object_string("currencyCode"),
  )
  |> option.or(marketing_budget_currency(read_marketing_object(data, "budget")))
}

fn marketing_budget_currency(
  budget: Option(Dict(String, MarketingValue)),
) -> Option(String) {
  case budget {
    Some(budget) ->
      read_marketing_object_string(Some(budget), "currencyCode")
      |> option.or(
        read_marketing_object(budget, "total")
        |> read_marketing_object_string("currencyCode"),
      )
    None -> None
  }
}

fn decimal_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  [
    "orders",
    "primaryConversions",
    "allConversions",
    "firstTimeCustomers",
    "returningCustomers",
  ]
  |> list.filter_map(fn(field) {
    case read_decimal_input(input, field) {
      Some(value) -> Ok(#(field, MarketingString(value)))
      None -> Error(Nil)
    }
  })
}

// ===========================================================================
// Shared helpers
// ===========================================================================

fn read_arg_string_list(
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

fn read_value_string(
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

fn read_value_bool(
  source: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(Bool) {
  case dict.get(source, field) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_value_int(
  source: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(Int) {
  case dict.get(source, field) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_money_input(
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

fn read_decimal_input(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(String) {
  read_value_string(input, field)
}

fn read_utm(
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

fn has_attribution(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_utm(input) {
    Some(_) -> True
    None ->
      case read_value_string(input, "urlParameterValue") {
        Some(_) -> True
        None -> False
      }
  }
}

fn same_utm(
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

fn find_marketing_activity_by_utm(
  store: Store,
  utm: Option(Dict(String, MarketingValue)),
) -> Option(MarketingRecord) {
  case utm {
    None -> None
    Some(utm) ->
      list.find(store.list_effective_marketing_activities(store), fn(activity) {
        same_utm(
          read_marketing_object(activity.data, "utmParameters"),
          Some(utm),
        )
      })
      |> option.from_result
  }
}

fn read_marketing_string(
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

fn read_marketing_object(
  source: Dict(String, MarketingValue),
  field: String,
) -> Option(Dict(String, MarketingValue)) {
  case dict.get(source, field) {
    Ok(MarketingObject(value)) -> Some(value)
    _ -> None
  }
}

fn read_marketing_object_string(
  source: Option(Dict(String, MarketingValue)),
  field: String,
) -> Option(String) {
  case source {
    Some(source) -> read_marketing_string(source, field)
    None -> None
  }
}

fn marketing_remote_id(data: Dict(String, MarketingValue)) -> Option(String) {
  case read_marketing_string(data, "remoteId") {
    Some(id) -> Some(id)
    None ->
      read_marketing_object(data, "marketingEvent")
      |> read_marketing_object_string("remoteId")
  }
}

fn app_name(data: Dict(String, MarketingValue)) -> Option(String) {
  case read_marketing_object(data, "app") {
    Some(app) ->
      case read_marketing_string(app, "name") {
        Some(name) -> Some(name)
        None -> read_marketing_string(app, "title")
      }
    None -> None
  }
}

fn optional_marketing_string(value: Option(String)) -> MarketingValue {
  case value {
    Some(value) -> MarketingString(value)
    None -> MarketingNull
  }
}

fn optional_marketing_object(
  value: Option(Dict(String, MarketingValue)),
) -> MarketingValue {
  case value {
    Some(value) -> MarketingObject(value)
    None -> MarketingNull
  }
}

fn optional_string_list_source(value: Option(List(String))) -> SourceValue {
  case value {
    Some(values) -> SrcList(list.map(values, SrcString))
    None -> SrcNull
  }
}

fn marketing_value_to_source(value: MarketingValue) -> SourceValue {
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

fn marketing_data_to_source(data: Dict(String, MarketingValue)) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, marketing_value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

fn project_payload(
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

fn overlay_marketing_data(
  base: Dict(String, MarketingValue),
  entries: List(#(String, MarketingValue)),
) -> Dict(String, MarketingValue) {
  list.fold(entries, base, fn(acc, pair) { dict.insert(acc, pair.0, pair.1) })
}

fn is_known_local_marketing_activity_extension(id: Option(String)) -> Bool {
  case id {
    Some(id) ->
      string.starts_with(id, "gid://shopify/MarketingActivityExtension/")
      && !string.ends_with(id, "/00000000-0000-0000-0000-000000000000")
    None -> False
  }
}

fn event_ended_at_for_status(
  status: String,
  timestamp: String,
) -> Option(String) {
  case status {
    "INACTIVE" | "DELETED_EXTERNALLY" -> Some(timestamp)
    _ -> None
  }
}

fn status_label(status: String) -> String {
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

fn source_and_medium(marketing_channel_type: String, tactic: String) -> String {
  case marketing_channel_type, tactic {
    "EMAIL", "NEWSLETTER" -> "Email newsletter"
    _, _ ->
      capitalize(string.lowercase(marketing_channel_type))
      <> " "
      <> string.replace(string.lowercase(tactic), "_", " ")
  }
}

fn capitalize(value: String) -> String {
  case string.to_graphemes(value) {
    [] -> "E"
    [first, ..rest] -> string.uppercase(first) <> string.concat(rest)
  }
}

fn id_number(id: String) -> Option(Int) {
  case list.last(string.split(id, "/")) {
    Ok(last) ->
      case int.parse(last) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
    Error(_) -> None
  }
}

fn option_count(value: Option(a)) -> Int {
  case value {
    Some(_) -> 1
    None -> 0
  }
}

fn dedupe_strings(values: List(String)) -> List(String) {
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

fn url_encode(value: String) -> String {
  value
  |> string.replace("%", "%25")
  |> string.replace(":", "%3A")
  |> string.replace("/", "%2F")
  |> string.replace(" ", "%20")
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

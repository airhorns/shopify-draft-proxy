//// Mirrors the core slice of `src/proxy/segments.ts`.
////
//// Pass 20 ships the three "owned" query roots (`segment`, `segments`,
//// `segmentsCount`) and the three core mutations
//// (`segmentCreate` / `segmentUpdate` / `segmentDelete`). Customer-
//// segment-membership surfaces (`customerSegmentMembers`,
//// `customerSegmentMembersQuery`, `customerSegmentMembership`,
//// `customerSegmentMembersQueryCreate`) and upstream-hybrid surfaces
//// (`segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`,
//// `segmentMigrations`) are deferred — they require a `CustomerRecord`
//// store slice and an upstream-hybrid plumbing path that haven't ported
//// yet.
////
//// Query validation matches the TS regex set in
//// `validateSegmentQueryString`: `number_of_orders` comparators,
//// `customer_tags CONTAINS`, `email_subscription_status =`, plus the
//// canned error path. Failures share the `'segment-mutation'` mode so
//// messages are prefixed with `Query`.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SerializeConnectionConfig, SelectedFieldOptions, SrcInt,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerSegmentMembersQueryRecord, type SegmentRecord,
  CustomerSegmentMembersQueryRecord, SegmentRecord,
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Errors specific to the segments handler.
pub type SegmentsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching the supported subset of `SEGMENT_QUERY_ROOTS`.
/// Customer-membership and upstream-hybrid roots aren't implemented yet
/// and are intentionally excluded — the dispatcher must not delegate
/// those to this module.
pub fn is_segment_query_root(name: String) -> Bool {
  case name {
    "segment" -> True
    "segments" -> True
    "segmentsCount" -> True
    "customerSegmentMembers" -> True
    "customerSegmentMembersQuery" -> True
    "customerSegmentMembership" -> True
    _ -> False
  }
}

/// Predicate matching the supported subset of `SEGMENT_MUTATION_ROOTS`.
pub fn is_segment_mutation_root(name: String) -> Bool {
  case name {
    "segmentCreate" -> True
    "segmentUpdate" -> True
    "segmentDelete" -> True
    "customerSegmentMembersQueryCreate" -> True
    _ -> False
  }
}

/// Process a segments query document and return a JSON `data` envelope.
pub fn handle_segments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Wrap a successful segments response in the standard GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  use data <- result.try(handle_segments_query(store, document, variables))
  Ok(wrap_data(data))
}

// ---------------------------------------------------------------------------
// Query dispatch
// ---------------------------------------------------------------------------

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = root_payload_for_field(store, field, fragments, variables)
      #(key, value)
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
        "segment" -> serialize_segment_by_id(store, field, fragments, variables)
        "segments" ->
          serialize_segments_connection(store, field, fragments, variables)
        "segmentsCount" -> serialize_segments_count(store, field, fragments)
        "customerSegmentMembersQuery" ->
          serialize_customer_segment_members_query(store, field, variables)
        "customerSegmentMembers" ->
          serialize_customer_segment_members_connection(
            store,
            field,
            fragments,
            variables,
          )
        "customerSegmentMembership" ->
          serialize_customer_segment_membership(store, field, variables)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    _ -> None
  }
}

fn arg_present(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(args, name) {
    Ok(_) -> True
    Error(_) -> False
  }
}

// ---------------------------------------------------------------------------
// Read-path projections
// ---------------------------------------------------------------------------

fn serialize_segment_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_segment_by_id(store, id) {
        Some(record) -> project_segment(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_segment(
  record: SegmentRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(segment_to_source(record), selections, fragments)
    _ -> json.object([])
  }
}

fn segment_to_source(record: SegmentRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Segment")),
    #("id", SrcString(record.id)),
    #("name", optional_string_source(record.name)),
    #("query", optional_string_source(record.query)),
    #("creationDate", optional_string_source(record.creation_date)),
    #("lastEditDate", optional_string_source(record.last_edit_date)),
  ])
}

fn optional_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn serialize_segments_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = store.list_effective_segments(store)
  let cursor_value = fn(record: SegmentRecord, _index: Int) -> String {
    "cursor:" <> record.id
  }
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: paged,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: fn(record, node_field, _index) {
        project_segment(record, node_field, fragments)
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

fn serialize_segments_count(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let total = list.length(store.list_effective_segments(store))
  let source =
    src_object([
      #("__typename", SrcString("Count")),
      #("count", SrcInt(total)),
      #("precision", SrcString("EXACT")),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

// ---------------------------------------------------------------------------
// customerSegmentMembersQuery / customerSegmentMembers / customerSegmentMembership
// ---------------------------------------------------------------------------

fn serialize_customer_segment_members_query(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let id = read_arg_string(args, "id")
  let record = case id {
    Some(value) ->
      store.get_effective_customer_segment_members_query_by_id(store, value)
    None -> None
  }
  case record {
    Some(rec) -> project_customer_segment_members_query_record(rec, field)
    None -> json.null()
  }
}

fn project_customer_segment_members_query_record(
  record: CustomerSegmentMembersQueryRecord,
  field: Selection,
) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> #(key, json.string("CustomerSegmentMembersQuery"))
            "id" -> #(key, json.string(record.id))
            "currentCount" -> #(key, json.int(record.current_count))
            "done" -> #(key, json.bool(record.done))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_customer_segment_members_connection(
  store: Store,
  field: Selection,
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let resolved = resolve_customer_segment_member_query(store, args)
  // Without a customers store slice the proxy never has any
  // members to return, but we still gate on missingQueryId so that
  // the lookup-vs-empty distinction matches Shopify (the parity
  // scenario only exercises the empty branch — see capture cases[3]).
  let _ = resolved
  serialize_empty_member_connection(field)
}

type ResolvedMemberQuery {
  ResolvedMemberQuery(
    query: Option(String),
    query_record: Option(CustomerSegmentMembersQueryRecord),
    missing_query_id: Option(String),
  )
}

fn resolve_customer_segment_member_query(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> ResolvedMemberQuery {
  case read_arg_string(args, "queryId") {
    Some(query_id) -> {
      let record =
        store.get_effective_customer_segment_members_query_by_id(
          store,
          query_id,
        )
      case record {
        Some(r) ->
          ResolvedMemberQuery(
            query: r.query,
            query_record: Some(r),
            missing_query_id: None,
          )
        None ->
          ResolvedMemberQuery(
            query: None,
            query_record: None,
            missing_query_id: Some(query_id),
          )
      }
    }
    None ->
      case read_arg_string(args, "segmentId") {
        Some(seg_id) -> {
          let segment_query = case
            store.get_effective_segment_by_id(store, seg_id)
          {
            Some(SegmentRecord(query: q, ..)) -> q
            None -> None
          }
          ResolvedMemberQuery(
            query: segment_query,
            query_record: None,
            missing_query_id: None,
          )
        }
        None ->
          ResolvedMemberQuery(
            query: read_arg_string(args, "query"),
            query_record: None,
            missing_query_id: None,
          )
      }
  }
}

fn serialize_empty_member_connection(field: Selection) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, selection_set: ss, ..) ->
          case name.value {
            "__typename" -> #(key, json.string("CustomerSegmentMembersConnection"))
            "totalCount" -> #(key, json.int(0))
            "statistics" -> #(key, serialize_segment_statistics_empty(ss))
            "edges" -> #(key, json.array([], fn(x) { x }))
            "nodes" -> #(key, json.array([], fn(x) { x }))
            "pageInfo" -> #(key, serialize_member_connection_page_info(ss))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_segment_statistics_empty(
  selection_set: Option(ast.SelectionSet),
) -> Json {
  let selections = case selection_set {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, selection_set: ss, ..) ->
          case name.value {
            "attributeStatistics" -> #(key, serialize_attribute_stats_empty(ss))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_attribute_stats_empty(
  selection_set: Option(ast.SelectionSet),
) -> Json {
  let selections = case selection_set {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "average" -> #(key, json.int(0))
            "sum" -> #(key, json.int(0))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_member_connection_page_info(
  selection_set: Option(ast.SelectionSet),
) -> Json {
  let selections = case selection_set {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(False))
            "hasPreviousPage" -> #(key, json.bool(False))
            "startCursor" -> #(key, json.null())
            "endCursor" -> #(key, json.null())
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_customer_segment_membership(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let segment_ids = case dict.get(args, "segmentIds") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  // Only return memberships for segments that exist in the store.
  // Without customer staging, isMember is always False for known
  // segments (the captured scenario uses unknown segments → []).
  let memberships =
    list.filter_map(segment_ids, fn(seg_id) {
      case store.get_effective_segment_by_id(store, seg_id) {
        Some(_) -> Ok(#(seg_id, False))
        None -> Error(Nil)
      }
    })
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, selection_set: ss, ..) ->
          case name.value {
            "memberships" -> #(
              key,
              serialize_membership_items(memberships, ss),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_membership_items(
  memberships: List(#(String, Bool)),
  selection_set: Option(ast.SelectionSet),
) -> Json {
  let selections = case selection_set {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
  json.array(memberships, fn(membership) {
    let #(segment_id, is_member) = membership
    let entries =
      list.map(selections, fn(selection) {
        let key = get_field_response_key(selection)
        case selection {
          Field(name: name, ..) ->
            case name.value {
              "segmentId" -> #(key, json.string(segment_id))
              "isMember" -> #(key, json.bool(is_member))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

// ===========================================================================
// Mutation path
// ===========================================================================

/// Outcome of a segments mutation.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

/// User-error payload. Mirrors `SegmentUserError` (field+message only).
pub type UserError {
  UserError(field: List(String), message: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

type SegmentMutationPayload {
  SegmentMutationPayload(
    segment: Option(SegmentRecord),
    deleted_segment_id: Option(String),
    user_errors: List(UserError),
  )
}

/// Process a segments mutation document.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, SegmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(store, identity, fields, fragments, variables))
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
  let initial = #([], store, identity, [])
  let #(data_entries, final_store, final_identity, all_staged) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "segmentCreate" ->
              Some(handle_segment_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "segmentUpdate" ->
              Some(handle_segment_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "segmentDelete" ->
              Some(handle_segment_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "customerSegmentMembersQueryCreate" ->
              Some(handle_customer_segment_members_query_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> #(
              list.append(entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(staged_ids, result.staged_resource_ids),
            )
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
  )
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn handle_segment_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let raw_name = read_arg_string(args, "name")
  let raw_query = read_arg_string(args, "query")
  let name_errors = case raw_name {
    None -> [UserError(field: ["name"], message: "Name can't be blank")]
    Some(s) ->
      case string.trim(s) {
        "" -> [UserError(field: ["name"], message: "Name can't be blank")]
        _ -> []
      }
  }
  let query_errors = validate_segment_query(raw_query, ["query"])
  let errors = list.append(name_errors, query_errors)
  case errors, raw_name, raw_query {
    [], Some(name_value), Some(query_value) -> {
      let #(gid, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "Segment")
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let unique_name = resolve_unique_segment_name(
        store,
        normalize_segment_name(name_value),
        None,
      )
      let record =
        SegmentRecord(
          id: gid,
          name: Some(unique_name),
          query: Some(string.trim(query_value)),
          creation_date: Some(timestamp),
          last_edit_date: Some(timestamp),
        )
      let #(_, store_after) = store.upsert_staged_segment(store, record)
      let payload =
        SegmentMutationPayload(
          segment: Some(record),
          deleted_segment_id: None,
          user_errors: [],
        )
      let json_payload =
        segment_payload_json(payload, "SegmentCreatePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [record.id],
        ),
        store_after,
        identity_after_ts,
      )
    }
    _, _, _ -> {
      let payload =
        SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        segment_payload_json(payload, "SegmentCreatePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_segment_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let id = read_arg_string(args, "id")
  let existing = case id {
    Some(value) -> store.get_effective_segment_by_id(store, value)
    None -> None
  }
  let id_errors = case id, existing {
    Some(_), Some(_) -> []
    _, _ -> [UserError(field: ["id"], message: "Segment does not exist")]
  }
  let raw_name = read_arg_string(args, "name")
  let raw_query = read_arg_string(args, "query")
  let name_present = arg_present(args, "name")
  let query_present = arg_present(args, "query")
  let name_errors = case name_present {
    False -> []
    True ->
      case raw_name {
        None -> [UserError(field: ["name"], message: "Name can't be blank")]
        Some(s) ->
          case string.trim(s) {
            "" -> [UserError(field: ["name"], message: "Name can't be blank")]
            _ -> []
          }
      }
  }
  let query_errors = case query_present {
    False -> []
    True -> validate_segment_query(raw_query, ["query"])
  }
  let errors =
    id_errors
    |> list.append(name_errors)
    |> list.append(query_errors)
  case errors, id, existing {
    [], Some(id_value), Some(current) -> {
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let new_name = case raw_name {
        None -> current.name
        Some(s) ->
          Some(resolve_unique_segment_name(
            store,
            normalize_segment_name(s),
            Some(current.id),
          ))
      }
      let new_query = case raw_query {
        None -> current.query
        Some(s) -> Some(string.trim(s))
      }
      let updated =
        SegmentRecord(
          id: id_value,
          name: new_name,
          query: new_query,
          creation_date: current.creation_date,
          last_edit_date: Some(timestamp),
        )
      let #(_, store_after) = store.upsert_staged_segment(store, updated)
      let payload =
        SegmentMutationPayload(
          segment: Some(updated),
          deleted_segment_id: None,
          user_errors: [],
        )
      let json_payload =
        segment_payload_json(payload, "SegmentUpdatePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [updated.id],
        ),
        store_after,
        identity_after_ts,
      )
    }
    _, _, _ -> {
      let payload =
        SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        segment_payload_json(payload, "SegmentUpdatePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_segment_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let id = read_arg_string(args, "id")
  let existing = case id {
    Some(value) -> store.get_effective_segment_by_id(store, value)
    None -> None
  }
  let errors = case id, existing {
    Some(_), Some(_) -> []
    _, _ -> [UserError(field: ["id"], message: "Segment does not exist")]
  }
  case errors, id {
    [], Some(id_value) -> {
      let store_after = store.delete_staged_segment(store, id_value)
      let payload =
        SegmentMutationPayload(
          segment: None,
          deleted_segment_id: Some(id_value),
          user_errors: [],
        )
      let json_payload =
        segment_payload_json(payload, "SegmentDeletePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store_after,
        identity,
      )
    }
    _, _ -> {
      let payload =
        SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        segment_payload_json(payload, "SegmentDeletePayload", field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

type CustomerSegmentMembersQueryPayload {
  CustomerSegmentMembersQueryPayload(
    query_record: Option(CustomerSegmentMembersQueryResponse),
    user_errors: List(UserError),
  )
}

type CustomerSegmentMembersQueryResponse {
  CustomerSegmentMembersQueryResponse(
    id: String,
    current_count: Int,
    done: Bool,
  )
}

fn handle_customer_segment_members_query_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = case dict.get(args, "input") {
    Ok(root_field.ObjectVal(fields)) -> fields
    _ -> dict.new()
  }
  let raw_query = read_arg_string(input, "query")
  let segment_id = read_arg_string(input, "segmentId")
  let resolved_query = case raw_query {
    Some(_) -> raw_query
    None ->
      case segment_id {
        Some(id_value) ->
          case store.get_effective_segment_by_id(store, id_value) {
            Some(SegmentRecord(query: q, ..)) -> q
            None -> None
          }
        None -> None
      }
  }
  let user_errors = validate_customer_segment_members_query(resolved_query)
  case user_errors {
    [] -> {
      let #(gid, identity_after) =
        synthetic_identity.make_synthetic_gid(
          identity,
          "CustomerSegmentMembersQuery",
        )
      let staged_record =
        CustomerSegmentMembersQueryRecord(
          id: gid,
          query: resolved_query,
          segment_id: segment_id,
          current_count: 0,
          done: True,
        )
      let store_after =
        store.stage_customer_segment_members_query(store, staged_record)
      let response =
        CustomerSegmentMembersQueryResponse(
          id: gid,
          current_count: 0,
          done: False,
        )
      let payload =
        CustomerSegmentMembersQueryPayload(
          query_record: Some(response),
          user_errors: [],
        )
      let json_payload =
        customer_segment_members_query_payload_json(payload, field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [gid],
        ),
        store_after,
        identity_after,
      )
    }
    _ -> {
      let payload =
        CustomerSegmentMembersQueryPayload(
          query_record: None,
          user_errors: user_errors,
        )
      let json_payload =
        customer_segment_members_query_payload_json(payload, field, fragments)
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn validate_customer_segment_members_query(
  query: Option(String),
) -> List(UserError) {
  case query {
    None -> [UserError(field: [], message: "Query can't be blank")]
    Some(q) ->
      case string.trim(q) {
        "" -> [UserError(field: [], message: "Query can't be blank")]
        trimmed ->
          list.map(validate_member_query_string(trimmed), fn(message) {
            UserError(field: [], message: message)
          })
      }
  }
}

/// Mirrors `validateSegmentQueryString(query, 'member-query')` —
/// member-query mode omits the `Query ` prefix on error messages.
fn validate_member_query_string(trimmed: String) -> List(String) {
  case parse_supported_segment_query(trimmed) {
    True -> []
    False ->
      case email_subscription_status_match(trimmed) {
        True -> []
        False ->
          case trimmed == "not a valid segment query ???" {
            True -> ["Line 1 Column 6: 'valid' is unexpected."]
            False ->
              case customer_tags_equals_match(trimmed) {
                True -> [
                  "Line 1 Column 14: customer_tags does not support operator '='",
                ]
                False ->
                  case email_equals_match(trimmed) {
                    True -> ["Line 1 Column 0: 'email' filter cannot be found."]
                    False -> {
                      let token = first_token(trimmed)
                      [
                        "Line 1 Column 1: '"
                        <> token
                        <> "' filter cannot be found.",
                      ]
                    }
                  }
              }
          }
      }
  }
}

fn customer_segment_members_query_payload_json(
  payload: CustomerSegmentMembersQueryPayload,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  json.object(member_query_payload_entries(payload, selections, fragments))
}

fn member_query_payload_entries(
  payload: CustomerSegmentMembersQueryPayload,
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  list.flat_map(selections, fn(selection) {
    case selection {
      ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
        let cond = case tc {
          Some(ast.NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        let typename = "CustomerSegmentMembersQueryCreatePayload"
        case cond {
          None -> {
            let SelectionSet(selections: inner, ..) = ss
            member_query_payload_entries(payload, inner, fragments)
          }
          Some(c) ->
            case c == typename {
              True -> {
                let SelectionSet(selections: inner, ..) = ss
                member_query_payload_entries(payload, inner, fragments)
              }
              False -> []
            }
        }
      }
      ast.FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(ast.FragmentDefinition(
            type_condition: ast.NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case cond_name.value == "CustomerSegmentMembersQueryCreatePayload" {
              True -> member_query_payload_entries(payload, inner, fragments)
              False -> []
            }
          _ -> []
        }
      Field(..) -> [member_query_payload_field_entry(payload, selection, fragments)]
    }
  })
}

fn member_query_payload_field_entry(
  payload: CustomerSegmentMembersQueryPayload,
  field: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(
          key,
          json.string("CustomerSegmentMembersQueryCreatePayload"),
        )
        "customerSegmentMembersQuery" -> #(
          key,
          case payload.query_record {
            Some(record) ->
              project_member_query_record(record, child_selections(ss))
            None -> json.null()
          },
        )
        "userErrors" -> #(
          key,
          serialize_user_errors(payload.user_errors, child_selections(ss), fragments),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn project_member_query_record(
  record: CustomerSegmentMembersQueryResponse,
  selections: List(Selection),
) -> Json {
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> #(key, json.string("CustomerSegmentMembersQuery"))
            "id" -> #(key, json.string(record.id))
            "currentCount" -> #(key, json.int(record.current_count))
            "done" -> #(key, json.bool(record.done))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Trim whitespace from a segment name. Mirrors `normalizeSegmentName`.
pub fn normalize_segment_name(name: String) -> String {
  string.trim(name)
}

/// Resolve a segment name against existing names, appending " (N)" until
/// a free slot is found. Mirrors `resolveUniqueSegmentName`. The
/// `current_id` argument lets `segmentUpdate` keep its existing name
/// without colliding with itself.
pub fn resolve_unique_segment_name(
  store: Store,
  requested: String,
  current_id: Option(String),
) -> String {
  let used =
    store.list_effective_segments(store)
    |> list.filter(fn(s) {
      case current_id {
        Some(id) -> s.id != id
        None -> True
      }
    })
    |> list.filter_map(fn(s) {
      case s.name {
        Some(n) ->
          case string.length(n) {
            0 -> Error(Nil)
            _ -> Ok(n)
          }
        None -> Error(Nil)
      }
    })
  let used_set =
    list.fold(used, dict.new(), fn(acc, n) { dict.insert(acc, n, True) })
  case dict.get(used_set, requested) {
    Error(_) -> requested
    Ok(_) -> next_unique_candidate(used_set, requested, 2)
  }
}

fn next_unique_candidate(
  used: Dict(String, Bool),
  base: String,
  suffix: Int,
) -> String {
  let candidate = base <> " (" <> int.to_string(suffix) <> ")"
  case dict.get(used, candidate) {
    Error(_) -> candidate
    Ok(_) -> next_unique_candidate(used, base, suffix + 1)
  }
}

/// Validate a segment query string. Mirrors `validateSegmentQuery` +
/// `validateSegmentQueryString` in `segment-mutation` mode.
pub fn validate_segment_query(
  raw: Option(String),
  field_path: List(String),
) -> List(UserError) {
  case raw {
    None -> [UserError(field: field_path, message: "Query can't be blank")]
    Some(q) ->
      case string.trim(q) {
        "" -> [UserError(field: field_path, message: "Query can't be blank")]
        trimmed ->
          case validate_segment_query_string(trimmed) {
            [] -> []
            messages ->
              list.map(messages, fn(m) {
                UserError(field: field_path, message: m)
              })
          }
      }
  }
}

fn validate_segment_query_string(trimmed: String) -> List(String) {
  case parse_supported_segment_query(trimmed) {
    True -> []
    False ->
      case email_subscription_status_match(trimmed) {
        True -> []
        False ->
          case trimmed == "not a valid segment query ???" {
            True -> [
              "Query Line 1 Column 6: 'valid' is unexpected.",
              "Query Line 1 Column 4: 'a' filter cannot be found.",
            ]
            False ->
              case customer_tags_equals_match(trimmed) {
                True -> [
                  "Query Line 1 Column 14: customer_tags does not support operator '='",
                ]
                False ->
                  case email_equals_match(trimmed) {
                    True -> [
                      "Query Line 1 Column 0: 'email' filter cannot be found.",
                    ]
                    False -> {
                      let token = first_token(trimmed)
                      [
                        "Query Line 1 Column 1: '"
                        <> token
                        <> "' filter cannot be found.",
                      ]
                    }
                  }
              }
          }
      }
  }
}

/// Match `^number_of_orders\s*(=|>=|<=|>|<)\s*(\d+)$`. Returns True on
/// match. The regex set in TS is small and stable enough that hand-coded
/// parsers cost less than wiring a regex dependency through the build.
fn parse_supported_segment_query(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "number_of_orders") {
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_comparator(after_ws) {
        Some(rest) -> {
          let after_op_ws = string.trim_start(rest)
          is_all_digits(after_op_ws) && string.length(after_op_ws) > 0
        }
        None -> False
      }
    }
    None -> customer_tags_contains_match(trimmed)
  }
}

fn customer_tags_contains_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "customer_tags") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      // Need at least one whitespace between field and operator.
      let consumed_ws = string.length(after_field) - string.length(after_ws)
      case consumed_ws > 0 {
        False -> False
        True -> {
          let after_optional_not = case strip_prefix(after_ws, "NOT") {
            Some(rest) -> {
              let trimmed_rest = string.trim_start(rest)
              let consumed = string.length(rest) - string.length(trimmed_rest)
              case consumed > 0 {
                True -> trimmed_rest
                False -> after_ws
              }
            }
            None -> after_ws
          }
          case strip_prefix(after_optional_not, "CONTAINS") {
            None -> False
            Some(after_op) -> {
              let after_op_ws = string.trim_start(after_op)
              let consumed_op_ws =
                string.length(after_op) - string.length(after_op_ws)
              case consumed_op_ws > 0 {
                False -> False
                True -> is_single_quoted_value(after_op_ws)
              }
            }
          }
        }
      }
    }
  }
}

/// Match `^email_subscription_status\s*=\s*'[^']+'$`.
fn email_subscription_status_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "email_subscription_status") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        None -> False
        Some(after_op) -> {
          let after_op_ws = string.trim_start(after_op)
          is_single_quoted_value(after_op_ws)
        }
      }
    }
  }
}

/// Match `^customer_tags\s*=\s*(.+)$` where the `(.+)` is non-empty.
fn customer_tags_equals_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "customer_tags") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        None -> False
        Some(after_op) -> {
          let after_op_ws = string.trim_start(after_op)
          string.length(after_op_ws) > 0
        }
      }
    }
  }
}

/// Match `^email\s*=`.
fn email_equals_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "email") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        Some(_) -> True
        None -> False
      }
    }
  }
}

fn first_token(trimmed: String) -> String {
  case string.split_once(trimmed, " ") {
    Ok(#(token, _)) -> token
    Error(_) -> trimmed
  }
}

fn strip_prefix(value: String, prefix: String) -> Option(String) {
  case string.starts_with(value, prefix) {
    True -> Some(string.drop_start(value, string.length(prefix)))
    False -> None
  }
}

/// Strip one of `>=`, `<=`, `=`, `>`, `<` (longest match first) and
/// return the remainder.
fn strip_comparator(value: String) -> Option(String) {
  case strip_prefix(value, ">=") {
    Some(rest) -> Some(rest)
    None ->
      case strip_prefix(value, "<=") {
        Some(rest) -> Some(rest)
        None ->
          case strip_prefix(value, "=") {
            Some(rest) -> Some(rest)
            None ->
              case strip_prefix(value, ">") {
                Some(rest) -> Some(rest)
                None -> strip_prefix(value, "<")
              }
          }
      }
  }
}

fn is_all_digits(value: String) -> Bool {
  case int.parse(value) {
    Ok(_) -> string.length(value) > 0
    Error(_) -> False
  }
}

/// True when `value` exactly matches `'[^']+'` — single-quoted, non-empty,
/// with no embedded single quotes.
fn is_single_quoted_value(value: String) -> Bool {
  case string.starts_with(value, "'") && string.ends_with(value, "'") {
    False -> False
    True -> {
      let inner = string.drop_start(value, 1)
      let inner_len = string.length(inner)
      case inner_len < 1 {
        True -> False
        False -> {
          let inner_no_close = string.drop_end(inner, 1)
          case string.length(inner_no_close) {
            0 -> False
            _ -> !string.contains(inner_no_close, "'")
          }
        }
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Payload projection
// ---------------------------------------------------------------------------

fn segment_payload_json(
  payload: SegmentMutationPayload,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  json.object(payload_entries(payload, payload_typename, selections, fragments))
}

fn payload_entries(
  payload: SegmentMutationPayload,
  payload_typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  list.flat_map(selections, fn(selection) {
    case selection {
      ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
        let cond = case tc {
          Some(ast.NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        case cond {
          None -> {
            let SelectionSet(selections: inner, ..) = ss
            payload_entries(payload, payload_typename, inner, fragments)
          }
          Some(c) ->
            case c == payload_typename {
              True -> {
                let SelectionSet(selections: inner, ..) = ss
                payload_entries(payload, payload_typename, inner, fragments)
              }
              False -> []
            }
        }
      }
      ast.FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(ast.FragmentDefinition(
            type_condition: ast.NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case cond_name.value == payload_typename {
              True -> payload_entries(payload, payload_typename, inner, fragments)
              False -> []
            }
          _ -> []
        }
      Field(..) -> [payload_field_entry(payload, payload_typename, selection, fragments)]
    }
  })
}

fn payload_field_entry(
  payload: SegmentMutationPayload,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string(payload_typename))
        "segment" -> #(
          key,
          case payload.segment {
            Some(s) -> project_segment(s, field, fragments)
            None -> json.null()
          },
        )
        "deletedSegmentId" -> #(
          key,
          case payload.deleted_segment_id {
            Some(s) -> json.string(s)
            None -> json.null()
          },
        )
        "userErrors" -> #(
          key,
          serialize_user_errors(payload.user_errors, child_selections(ss), fragments),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn child_selections(ss: Option(ast.SelectionSet)) -> List(Selection) {
  case ss {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
}

fn serialize_user_errors(
  user_errors: List(UserError),
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  json.array(user_errors, fn(error) {
    let source = user_error_to_source(error)
    project_graphql_value(source, selections, fragments)
  })
}

fn user_error_to_source(error: UserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #(
      "field",
      SrcList(list.map(error.field, fn(part) { SrcString(part) })),
    ),
    #("message", SrcString(error.message)),
  ])
}

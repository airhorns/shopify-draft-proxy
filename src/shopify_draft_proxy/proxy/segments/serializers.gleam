//// Segments query and mutation payload serializers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcFloat, SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_connection_with_field_serializers, src_object,
}
import shopify_draft_proxy/proxy/segments/types as segment_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CustomerDefaultEmailAddressRecord, type CustomerRecord,
  type CustomerSegmentMembersQueryRecord, type Money, type SegmentRecord,
  type StorePropertyValue, Money, SegmentRecord, StorePropertyBool,
  StorePropertyFloat, StorePropertyInt, StorePropertyList, StorePropertyNull,
  StorePropertyObject, StorePropertyString,
}

@internal
pub fn root_payload_for_field(
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
        "segmentFilters"
        | "segmentFilterSuggestions"
        | "segmentValueSuggestions"
        | "segmentMigrations" ->
          serialize_captured_or_empty_connection(
            store,
            name.value,
            field,
            fragments,
          )
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

@internal
pub fn json_is_null(value: Json) -> Bool {
  json.to_string(value) == "null"
}

@internal
pub fn segment_not_found_error(
  field: Selection,
  document: String,
  key: String,
) -> Json {
  json.object([
    #("message", json.string("Segment does not exist")),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("extensions", json.object([#("code", json.string("NOT_FOUND"))])),
    #("path", json.array([key], json.string)),
  ])
}

@internal
pub fn customer_segment_members_query_not_found_error(
  field: Selection,
  document: String,
  key: String,
) -> Json {
  json.object([
    #("message", json.string("Something went wrong")),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #(
      "extensions",
      json.object([#("code", json.string("INTERNAL_SERVER_ERROR"))]),
    ),
    #("path", json.array([key], json.string)),
  ])
}

fn customer_segment_members_error_json(
  field: Selection,
  document: String,
  key: String,
  message: String,
) -> Json {
  json.object([
    #("message", json.string(message)),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("path", json.array([key], json.string)),
  ])
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
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
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
    #("name", graphql_helpers.option_string_source(record.name)),
    #("query", graphql_helpers.option_string_source(record.query)),
    #(
      "creationDate",
      graphql_helpers.option_string_source(record.creation_date),
    ),
    #(
      "lastEditDate",
      graphql_helpers.option_string_source(record.last_edit_date),
    ),
    #("tagMigrated", SrcBool(False)),
    #("valid", SrcBool(True)),
    #("percentageSnapshot", SrcNull),
    #("percentageSnapshotUpdatedAt", SrcNull),
    #("translation", SrcNull),
    #("author", SrcNull),
  ])
}

fn serialize_segments_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case
    store_has_staged_segments(store),
    store.get_base_segment_root_payload(store, "segments")
  {
    False, Some(payload) ->
      project_store_property_payload(payload, field, fragments)
    _, _ ->
      serialize_effective_segments_connection(
        store,
        field,
        fragments,
        variables,
      )
  }
}

fn serialize_effective_segments_connection(
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
  let source = case
    store_has_staged_segments(store),
    store.get_base_segment_root_payload(store, "segmentsCount")
  {
    False, Some(payload) -> store_property_value_to_source(payload)
    _, _ -> {
      let total = list.length(store.list_effective_segments(store))
      src_object([
        #("__typename", SrcString("Count")),
        #("count", SrcInt(total)),
        #("precision", SrcString("EXACT")),
      ])
    }
  }
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

fn serialize_captured_or_empty_connection(
  store: Store,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case store.get_base_segment_root_payload(store, root_name) {
    Some(payload) -> project_store_property_payload(payload, field, fragments)
    None -> serialize_empty_connection_for_field(field)
  }
}

fn project_store_property_payload(
  payload: StorePropertyValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = store_property_value_to_source(payload)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> source_value_to_json(source)
  }
}

@internal
pub fn store_has_staged_segments(store: Store) -> Bool {
  dict.to_list(store.staged_state.segments) != []
  || store.staged_state.segment_order != []
  || dict.to_list(store.staged_state.deleted_segment_ids) != []
}

fn store_property_value_to_source(value: StorePropertyValue) -> SourceValue {
  case value {
    StorePropertyNull -> SrcNull
    StorePropertyString(value) -> SrcString(value)
    StorePropertyBool(value) -> SrcBool(value)
    StorePropertyInt(value) -> SrcInt(value)
    StorePropertyFloat(value) -> SrcFloat(value)
    StorePropertyList(values) ->
      SrcList(list.map(values, store_property_value_to_source))
    StorePropertyObject(values) ->
      SrcObject(
        dict.to_list(values)
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_to_source(pair.1))
        })
        |> dict.from_list,
      )
  }
}

fn source_value_to_json(value: SourceValue) -> Json {
  case value {
    SrcNull -> json.null()
    SrcString(value) -> json.string(value)
    SrcBool(value) -> json.bool(value)
    SrcInt(value) -> json.int(value)
    SrcFloat(value) -> json.float(value)
    SrcList(values) -> json.array(values, source_value_to_json)
    SrcObject(fields) ->
      json.object(
        dict.to_list(fields)
        |> list.map(fn(pair) { #(pair.0, source_value_to_json(pair.1)) }),
      )
  }
}

fn serialize_empty_connection_for_field(field: Selection) -> Json {
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
            "nodes" -> #(key, json.array([], fn(x) { x }))
            "edges" -> #(key, json.array([], fn(x) { x }))
            "pageInfo" -> #(key, serialize_member_connection_page_info(ss))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// ---------------------------------------------------------------------------
// customerSegmentMembersQuery / customerSegmentMembers / customerSegmentMembership
// ---------------------------------------------------------------------------

fn serialize_customer_segment_members_query(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string_nonempty(args, "id")
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
            "status" -> #(key, json.string(record.status))
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
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let resolved = resolve_customer_segment_member_query(store, args)
  let members = list_customer_segment_members_for_query(store, resolved.query)
  serialize_customer_segment_member_connection(
    members,
    field,
    fragments,
    variables,
  )
}

type ResolvedMemberQuery {
  ResolvedMemberQuery(
    query: Option(String),
    query_record: Option(CustomerSegmentMembersQueryRecord),
    missing_query_id: Option(String),
  )
}

@internal
pub fn customer_segment_members_error(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
  key: String,
) -> Option(Json) {
  let resolved = resolve_customer_segment_member_query(store, args)
  case resolved.missing_query_id {
    Some(_) ->
      Some(customer_segment_members_error_json(
        field,
        document,
        key,
        "this async query cannot be found in segmentMembers",
      ))
    None ->
      case
        segment_types.validate_customer_segment_members_query(resolved.query)
      {
        [] -> None
        [first, ..] ->
          Some(customer_segment_members_error_json(
            field,
            document,
            key,
            first.message,
          ))
      }
  }
}

fn resolve_customer_segment_member_query(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> ResolvedMemberQuery {
  case graphql_helpers.read_arg_string_nonempty(args, "queryId") {
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
      case graphql_helpers.read_arg_string_nonempty(args, "segmentId") {
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
            query: graphql_helpers.read_arg_string_nonempty(args, "query"),
            query_record: None,
            missing_query_id: None,
          )
      }
  }
}

fn serialize_customer_segment_member_connection(
  all_members: List(CustomerRecord),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let cursor_value = fn(customer: CustomerRecord, _index: Int) -> String {
    customer.id
  }
  let window =
    paginate_connection_items(
      all_members,
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
  serialize_connection_with_field_serializers(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: fn(customer, node_field, _index) {
        project_customer_segment_member(customer, node_field, fragments)
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
    fn(_page_info_field) { None },
    fn(selection) {
      case selection {
        Field(name: name, selection_set: ss, ..) ->
          case name.value {
            "totalCount" -> json.int(list.length(all_members))
            "statistics" -> serialize_segment_statistics_empty(ss)
            _ -> json.null()
          }
        _ -> json.null()
      }
    },
  )
}

fn project_customer_segment_member(
  customer: CustomerRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        customer_segment_member_to_source(customer),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn customer_segment_member_to_source(customer: CustomerRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("CustomerSegmentMember")),
    #("id", SrcString(member_id_for_customer(customer))),
    #("displayName", SrcString(option.unwrap(customer.display_name, ""))),
    #("firstName", graphql_helpers.option_string_source(customer.first_name)),
    #("lastName", graphql_helpers.option_string_source(customer.last_name)),
    #(
      "defaultEmailAddress",
      default_email_address_source(customer.default_email_address),
    ),
    #(
      "numberOfOrders",
      SrcString(int.to_string(customer_number_of_orders(customer))),
    ),
    #("amountSpent", money_source(customer.amount_spent)),
  ])
}

fn member_id_for_customer(customer: CustomerRecord) -> String {
  "gid://shopify/CustomerSegmentMember/" <> gid_tail(customer.id)
}

fn gid_tail(id: String) -> String {
  case string.split(id, "/") |> list.reverse() {
    [tail, ..] -> tail
    [] -> id
  }
}

fn default_email_address_source(
  value: Option(CustomerDefaultEmailAddressRecord),
) -> SourceValue {
  case value {
    Some(record) ->
      src_object([
        #(
          "emailAddress",
          graphql_helpers.option_string_source(record.email_address),
        ),
      ])
    None -> SrcNull
  }
}

fn money_source(value: Option(Money)) -> SourceValue {
  let money = option.unwrap(value, Money(amount: "0.0", currency_code: "USD"))
  src_object([
    #("amount", SrcString(money.amount)),
    #("currencyCode", SrcString(money.currency_code)),
  ])
}

fn list_customer_segment_members_for_query(
  store: Store,
  query: Option(String),
) -> List(CustomerRecord) {
  let parsed = case query {
    Some(raw) ->
      segment_types.parse_supported_segment_query_value(string.trim(raw))
    None -> None
  }
  case parsed {
    None -> []
    Some(query) ->
      store.list_effective_customers(store)
      |> list.filter(fn(customer) {
        customer_matches_supported_segment_query(customer, query)
      })
      |> list.sort(fn(left, right) { string.compare(right.id, left.id) })
  }
}

fn customer_matches_supported_segment_query(
  customer: CustomerRecord,
  parsed: segment_types.SupportedSegmentQuery,
) -> Bool {
  case parsed {
    segment_types.CustomerTagsContains(value: value, negated: negated) -> {
      let has_tag = list.contains(customer.tags, value)
      case negated {
        True -> !has_tag
        False -> has_tag
      }
    }
    segment_types.NumberOfOrders(comparator: comparator, value: expected) -> {
      let actual = customer_number_of_orders(customer)
      case comparator {
        "=" -> actual == expected
        ">" -> actual > expected
        ">=" -> actual >= expected
        "<" -> actual < expected
        "<=" -> actual <= expected
        _ -> False
      }
    }
  }
}

fn customer_number_of_orders(customer: CustomerRecord) -> Int {
  case customer.number_of_orders {
    Some(value) ->
      case int.parse(value) {
        Ok(parsed) -> parsed
        Error(_) -> 0
      }
    None -> 0
  }
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
  let args = graphql_helpers.field_args(field, variables)
  let customer = case
    graphql_helpers.read_arg_string_nonempty(args, "customerId")
  {
    Some(id) -> store.get_effective_customer_by_id(store, id)
    None -> None
  }
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
        Some(segment) ->
          Ok(
            #(seg_id, case customer, segment.query {
              Some(customer_record), Some(query) ->
                case
                  segment_types.parse_supported_segment_query_value(string.trim(
                    query,
                  ))
                {
                  Some(parsed) ->
                    customer_matches_supported_segment_query(
                      customer_record,
                      parsed,
                    )
                  None -> False
                }
              _, _ -> False
            }),
          )
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
            "memberships" -> #(key, serialize_membership_items(memberships, ss))
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
@internal
pub fn customer_segment_members_query_payload_json(
  payload: segment_types.CustomerSegmentMembersQueryPayload,
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
  payload: segment_types.CustomerSegmentMembersQueryPayload,
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
      Field(..) -> [
        member_query_payload_field_entry(payload, selection, fragments),
      ]
    }
  })
}

fn member_query_payload_field_entry(
  payload: segment_types.CustomerSegmentMembersQueryPayload,
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
        "customerSegmentMembersQuery" -> #(key, case payload.query_record {
          Some(record) ->
            project_member_query_record(
              record,
              graphql_helpers.selection_set_selections(ss),
            )
          None -> json.null()
        })
        "userErrors" -> #(
          key,
          serialize_user_errors(
            payload.user_errors,
            "CustomerSegmentMembersQueryUserError",
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn project_member_query_record(
  record: segment_types.CustomerSegmentMembersQueryResponse,
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
            "status" -> #(key, json.string(record.status))
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

@internal
pub fn segment_payload_json(
  payload: segment_types.SegmentMutationPayload,
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
  payload: segment_types.SegmentMutationPayload,
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
              True ->
                payload_entries(payload, payload_typename, inner, fragments)
              False -> []
            }
          _ -> []
        }
      Field(..) -> [
        payload_field_entry(payload, payload_typename, selection, fragments),
      ]
    }
  })
}

fn payload_field_entry(
  payload: segment_types.SegmentMutationPayload,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string(payload_typename))
        "segment" -> #(key, case payload.segment {
          Some(s) -> project_segment(s, field, fragments)
          None -> json.null()
        })
        "deletedSegmentId" -> #(key, case payload.deleted_segment_id {
          Some(s) -> json.string(s)
          None -> json.null()
        })
        "userErrors" -> #(
          key,
          serialize_user_errors(
            payload.user_errors,
            "UserError",
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn serialize_user_errors(
  user_errors: List(segment_types.UserError),
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  json.array(user_errors, fn(error) {
    let source = user_error_to_source(error, typename)
    project_graphql_value(source, selections, fragments)
  })
}

fn user_error_to_source(
  error: segment_types.UserError,
  typename: String,
) -> SourceValue {
  src_object([
    #("__typename", SrcString(typename)),
    #("field", case error.field {
      None -> SrcNull
      Some(parts) -> SrcList(list.map(parts, fn(part) { SrcString(part) }))
    }),
    #("message", SrcString(error.message)),
    #("code", case error.code {
      Some(value) -> SrcString(value)
      None -> SrcNull
    }),
  ])
}

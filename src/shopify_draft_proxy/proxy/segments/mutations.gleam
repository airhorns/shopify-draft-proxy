//// Segments mutation handling.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, MutationFieldResult,
  MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/segments/serializers
import shopify_draft_proxy/proxy/segments/types as segment_types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  CustomerSegmentMembersQueryRecord, SegmentRecord,
}

@internal
pub fn is_segment_mutation_root(name: String) -> Bool {
  case name {
    "segmentCreate" -> True
    "segmentUpdate" -> True
    "segmentDelete" -> True
    "customerSegmentMembersQueryCreate" -> True
    _ -> False
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

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  _upstream: UpstreamContext,
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
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
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
            Some(#(result, next_store, next_identity)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  segments_status_for(name.value, result.staged_resource_ids),
                  "segments",
                  "stage-locally",
                  Some(segments_notes_for(name.value)),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
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
    log_drafts: all_drafts,
  )
}

/// Per-root-field log status for segments mutations. Default rule:
/// empty `staged_resource_ids` (validation rejected the request) →
/// `Failed`; otherwise `Staged`.
fn segments_status_for(
  _root_field_name: String,
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store_types.Failed
    [_, ..] -> store_types.Staged
  }
}

/// Notes string mirroring the `segments` dispatcher in `routes.ts`.
fn segments_notes_for(_root_field_name: String) -> String {
  "Staged locally in the in-memory segment draft store."
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
  let args = graphql_helpers.field_args(field, variables)
  let raw_name = graphql_helpers.read_arg_string_nonempty(args, "name")
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let name_errors = validate_segment_name_required(raw_name, ["name"])
  let query_errors = validate_segment_query(raw_query, ["query"])
  let field_errors = list.append(name_errors, query_errors)
  let limit_errors = case field_errors {
    [] -> validate_segment_limit(store)
    _ -> []
  }
  let errors =
    field_errors
    |> list.append(limit_errors)
  case errors, raw_name, raw_query {
    [], Some(name_value), Some(query_value) -> {
      let #(gid, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "Segment")
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let unique_name =
        resolve_unique_segment_name(
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
        segment_types.SegmentMutationPayload(
          segment: Some(record),
          deleted_segment_id: None,
          user_errors: [],
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentCreatePayload",
          field,
          fragments,
        )
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
        segment_types.SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentCreatePayload",
          field,
          fragments,
        )
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
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let existing = case id {
    Some(value) -> store.get_effective_segment_by_id(store, value)
    None -> None
  }
  let id_errors = case id, existing {
    Some(_), Some(_) -> []
    _, _ -> [
      segment_types.user_error(["id"], "Segment does not exist", None),
    ]
  }
  let raw_name = graphql_helpers.read_arg_string_nonempty(args, "name")
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let name_present = arg_present(args, "name")
  let query_present = arg_present(args, "query")
  let name_errors =
    validate_segment_name_optional(raw_name, name_present, [
      "name",
    ])
  let query_errors = case query_present {
    False -> []
    True -> validate_segment_query(raw_query, ["query"])
  }
  let change_errors = case id_errors, name_present, query_present {
    [], False, False -> [
      segment_types.null_field_user_error(
        "At least one attribute to change must be present",
        None,
      ),
    ]
    _, _, _ -> []
  }
  let errors =
    id_errors
    |> list.append(name_errors)
    |> list.append(query_errors)
    |> list.append(change_errors)
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
        segment_types.SegmentMutationPayload(
          segment: Some(updated),
          deleted_segment_id: None,
          user_errors: [],
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentUpdatePayload",
          field,
          fragments,
        )
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
        segment_types.SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentUpdatePayload",
          field,
          fragments,
        )
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
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let existing = case id {
    Some(value) -> store.get_effective_segment_by_id(store, value)
    None -> None
  }
  let errors = case id, existing {
    Some(_), Some(_) -> []
    _, _ -> [
      segment_types.user_error(["id"], "Segment does not exist", None),
    ]
  }
  case errors, id {
    [], Some(id_value) -> {
      let store_after = store.delete_staged_segment(store, id_value)
      let payload =
        segment_types.SegmentMutationPayload(
          segment: None,
          deleted_segment_id: Some(id_value),
          user_errors: [],
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentDeletePayload",
          field,
          fragments,
        )
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
        segment_types.SegmentMutationPayload(
          segment: None,
          deleted_segment_id: None,
          user_errors: errors,
        )
      let json_payload =
        serializers.segment_payload_json(
          payload,
          "SegmentDeletePayload",
          field,
          fragments,
        )
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

fn handle_customer_segment_members_query_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case dict.get(args, "input") {
    Ok(root_field.ObjectVal(fields)) -> fields
    _ -> dict.new()
  }
  let raw_query = graphql_helpers.read_arg_string_nonempty(input, "query")
  let segment_id = graphql_helpers.read_arg_string_nonempty(input, "segmentId")
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
  let user_errors =
    validate_customer_segment_members_query_create(
      raw_query,
      segment_id,
      resolved_query,
    )
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
          status: "INITIALIZED",
          current_count: 0,
          done: False,
        )
      let store_after =
        store.stage_customer_segment_members_query(store, staged_record)
      let response =
        segment_types.CustomerSegmentMembersQueryResponse(
          id: gid,
          status: "INITIALIZED",
          current_count: 0,
          done: False,
        )
      let payload =
        segment_types.CustomerSegmentMembersQueryPayload(
          query_record: Some(response),
          user_errors: [],
        )
      let json_payload =
        serializers.customer_segment_members_query_payload_json(
          payload,
          field,
          fragments,
        )
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
        segment_types.CustomerSegmentMembersQueryPayload(
          query_record: None,
          user_errors: user_errors,
        )
      let json_payload =
        serializers.customer_segment_members_query_payload_json(
          payload,
          field,
          fragments,
        )
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

fn validate_customer_segment_members_query_create(
  raw_query: Option(String),
  segment_id: Option(String),
  resolved_query: Option(String),
) -> List(segment_types.UserError) {
  case raw_query, segment_id {
    Some(_), Some(_) -> [
      invalid_customer_segment_members_query_input_error(
        "Providing both segment_id and query is not supported.",
      ),
    ]
    None, None -> [
      invalid_customer_segment_members_query_input_error(
        "You must provide one of segment_id or query.",
      ),
    ]
    _, _ ->
      segment_types.validate_customer_segment_members_query(resolved_query)
  }
}

fn invalid_customer_segment_members_query_input_error(
  message: String,
) -> segment_types.UserError {
  segment_types.user_error(["input"], message, Some("INVALID"))
}

@internal
pub fn normalize_segment_name(name: String) -> String {
  string.trim(name)
}

fn validate_segment_name_required(
  raw: Option(String),
  field_path: List(String),
) -> List(segment_types.UserError) {
  validate_segment_name(raw, True, field_path)
}

fn validate_segment_name_optional(
  raw: Option(String),
  present: Bool,
  field_path: List(String),
) -> List(segment_types.UserError) {
  validate_segment_name(raw, present, field_path)
}

fn validate_segment_name(
  raw: Option(String),
  present: Bool,
  field_path: List(String),
) -> List(segment_types.UserError) {
  case present, raw {
    False, _ -> []
    True, None -> [
      segment_types.user_error(field_path, "Name can't be blank", None),
    ]
    True, Some(name) ->
      case string.trim(name) {
        "" -> [
          segment_types.user_error(field_path, "Name can't be blank", None),
        ]
        _ ->
          case string.length(name) > segment_types.max_segment_name_length {
            True -> [
              segment_types.user_error(
                field_path,
                "Name is too long (maximum is 255 characters)",
                None,
              ),
            ]
            False -> []
          }
      }
  }
}

fn validate_segment_limit(store: Store) -> List(segment_types.UserError) {
  case
    list.length(store.list_effective_segments(store))
    >= segment_types.max_segments_per_shop
  {
    True -> [
      segment_types.user_error(["base"], "Segment limit reached", None),
    ]
    False -> []
  }
}

/// Resolve a segment name against existing names, appending " (N)" until
/// a free slot is found. Mirrors `resolveUniqueSegmentName`. The
/// `current_id` argument lets `segmentUpdate` keep its existing name
/// without colliding with itself.
@internal
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
@internal
pub fn validate_segment_query(
  raw: Option(String),
  field_path: List(String),
) -> List(segment_types.UserError) {
  case raw {
    None -> [
      segment_types.user_error(field_path, "Query can't be blank", None),
    ]
    Some(q) ->
      case string.trim(q) {
        "" -> [
          segment_types.user_error(field_path, "Query can't be blank", None),
        ]
        trimmed ->
          case string.length(trimmed) > segment_types.max_segment_query_length {
            True -> [
              segment_types.user_error(
                field_path,
                "Query is too long (maximum is 5000 characters)",
                None,
              ),
            ]
            False ->
              case segment_types.validate_segment_query_string(trimmed) {
                [] -> []
                messages ->
                  list.map(messages, fn(m) {
                    segment_types.user_error(field_path, m, None)
                  })
              }
          }
      }
  }
}

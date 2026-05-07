//// Segments mutation handling.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Selection, Argument, Field, StringValue, VariableValue,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
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
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
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
              let next_entries = case result.payload {
                Some(payload) -> list.append(entries, [#(result.key, payload)])
                None -> entries
              }
              let next_errors = list.append(errors, result.top_level_errors)
              let next_staged_ids = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                next_store,
                next_identity,
                next_staged_ids,
                next_drafts,
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = mutation_envelope(data_entries, all_errors)
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: case all_errors {
      [] -> all_staged
      _ -> []
    },
    log_drafts: all_drafts,
  )
}

fn mutation_envelope(
  entries: List(#(String, Json)),
  errors: List(Json),
) -> Json {
  case errors, entries {
    [], _ -> json.object([#("data", json.object(entries))])
    _, [] -> json.object([#("errors", json.preprocessed_array(errors))])
    _, _ ->
      json.object([
        #("errors", json.preprocessed_array(errors)),
        #("data", json.object(entries)),
      ])
  }
}

type SegmentMutationFieldResult {
  SegmentMutationFieldResult(
    key: String,
    payload: Option(Json),
    top_level_errors: List(Json),
    staged_resource_ids: List(String),
  )
}

type SegmentIdRead {
  SegmentIdRead(value: String, source: SegmentIdSource)
  SegmentIdMissing
}

type SegmentIdSource {
  SegmentIdVariable(name: String)
  SegmentIdLiteral
}

type SegmentIdValidation {
  SegmentIdValid(value: String)
  SegmentIdInvalidGlobalId(error: Json)
  SegmentIdWrongResourceType(error: Json)
}

fn validate_segment_id_argument(
  field: Selection,
  args: Dict(String, root_field.ResolvedValue),
  root_name: String,
) -> SegmentIdValidation {
  case read_segment_id(field, args) {
    SegmentIdRead(value, source) -> {
      case valid_segment_gid(value) {
        True -> SegmentIdValid(value)
        False ->
          case string.starts_with(value, "gid://shopify/") {
            True ->
              SegmentIdWrongResourceType(segment_invalid_id_error(root_name))
            False ->
              SegmentIdInvalidGlobalId(invalid_global_id_error(
                source,
                root_name,
                value,
              ))
          }
      }
    }
    SegmentIdMissing ->
      SegmentIdInvalidGlobalId(invalid_global_id_error(
        SegmentIdLiteral,
        root_name,
        "",
      ))
  }
}

fn read_segment_id(
  field: Selection,
  args: Dict(String, root_field.ResolvedValue),
) -> SegmentIdRead {
  case graphql_helpers.read_arg_string(args, "id") {
    Some(value) -> SegmentIdRead(value, segment_id_source(field))
    None -> SegmentIdMissing
  }
}

fn segment_id_source(field: Selection) -> SegmentIdSource {
  case field {
    Field(arguments: arguments, ..) ->
      arguments
      |> list.find_map(fn(argument) {
        case argument {
          Argument(name: name, value: VariableValue(variable), ..)
            if name.value == "id"
          -> {
            Ok(SegmentIdVariable(variable.name.value))
          }
          Argument(name: name, value: StringValue(..), ..)
            if name.value == "id"
          -> Ok(SegmentIdLiteral)
          _ -> Error(Nil)
        }
      })
      |> result.unwrap(SegmentIdLiteral)
    _ -> SegmentIdLiteral
  }
}

fn valid_segment_gid(value: String) -> Bool {
  case string.split(value, "/") {
    ["gid:", "", "shopify", "Segment", tail] -> tail != ""
    _ -> False
  }
}

fn invalid_global_id_error(
  source: SegmentIdSource,
  root_name: String,
  value: String,
) -> Json {
  let message = "Invalid global id '" <> value <> "'"
  case source {
    SegmentIdVariable(variable_name) ->
      json.object([
        #(
          "message",
          json.string(
            "Variable $"
            <> variable_name
            <> " of type ID! was provided invalid value",
          ),
        ),
        #(
          "extensions",
          json.object([
            #("code", json.string("INVALID_VARIABLE")),
            #("value", json.string(value)),
            #(
              "problems",
              json.preprocessed_array([
                json.object([
                  #("path", json.preprocessed_array([])),
                  #("explanation", json.string(message)),
                  #("message", json.string(message)),
                ]),
              ]),
            ),
          ]),
        ),
      ])
    SegmentIdLiteral ->
      json.object([
        #("message", json.string(message)),
        #("path", json.array(["mutation", root_name, "id"], json.string)),
        #(
          "extensions",
          json.object([
            #("code", json.string("argumentLiteralsIncompatible")),
            #("typeName", json.string("CoercionError")),
          ]),
        ),
      ])
  }
}

fn segment_invalid_id_error(root_name: String) -> Json {
  json.object([
    #("message", json.string("invalid id")),
    #("path", json.array([root_name], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
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
) -> #(SegmentMutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let raw_name = graphql_helpers.read_arg_string_nonempty(args, "name")
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let name_errors = validate_segment_name_required(raw_name, ["name"])
  let query_errors = validate_segment_query(raw_query, ["query"])
  let field_errors = list.append(name_errors, query_errors)
  let inventory = case field_errors {
    [] -> Some(segment_name_inventory(store, None))
    _ -> None
  }
  let limit_errors = case inventory {
    Some(#(count, _)) -> segment_limit_errors_for_count(count)
    None -> []
  }
  let used_names = case inventory {
    Some(#(_, names)) -> names
    None -> dict.new()
  }
  let errors =
    field_errors
    |> list.append(limit_errors)
  case errors, raw_name, raw_query {
    [], Some(name_value), Some(query_value) -> {
      case
        resolve_unique_segment_name_from_used_set(
          used_names,
          normalize_segment_name(name_value),
        )
      {
        Ok(unique_name) -> {
          let #(gid, identity_after_id) =
            synthetic_identity.make_synthetic_gid(identity, "Segment")
          let #(timestamp, identity_after_ts) =
            synthetic_identity.make_synthetic_timestamp(identity_after_id)
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
            SegmentMutationFieldResult(
              key: key,
              payload: Some(json_payload),
              top_level_errors: [],
              staged_resource_ids: [record.id],
            ),
            store_after,
            identity_after_ts,
          )
        }
        Error(error) -> {
          let payload =
            segment_types.SegmentMutationPayload(
              segment: None,
              deleted_segment_id: None,
              user_errors: [error],
            )
          let json_payload =
            serializers.segment_payload_json(
              payload,
              "SegmentCreatePayload",
              field,
              fragments,
            )
          #(
            SegmentMutationFieldResult(
              key: key,
              payload: Some(json_payload),
              top_level_errors: [],
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
      }
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
        SegmentMutationFieldResult(
          key: key,
          payload: Some(json_payload),
          top_level_errors: [],
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
) -> #(SegmentMutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case validate_segment_id_argument(field, args, key) {
    SegmentIdInvalidGlobalId(error) -> #(
      SegmentMutationFieldResult(
        key: key,
        payload: None,
        top_level_errors: [error],
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    SegmentIdWrongResourceType(error) -> #(
      SegmentMutationFieldResult(
        key: key,
        payload: Some(json.null()),
        top_level_errors: [error],
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    SegmentIdValid(id_value) -> {
      let existing = store.get_effective_segment_by_id(store, id_value)
      let id_errors = case existing {
        Some(_) -> []
        None -> [
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
      case errors, existing {
        [], Some(current) -> {
          let resolved_name = case raw_name {
            None -> Ok(current.name)
            Some(s) ->
              resolve_unique_segment_name(
                store,
                normalize_segment_name(s),
                Some(current.id),
              )
              |> result.map(Some)
          }
          case resolved_name {
            Ok(new_name) -> {
              let #(timestamp, identity_after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
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
              let #(_, store_after) =
                store.upsert_staged_segment(store, updated)
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
                SegmentMutationFieldResult(
                  key: key,
                  payload: Some(json_payload),
                  top_level_errors: [],
                  staged_resource_ids: [updated.id],
                ),
                store_after,
                identity_after_ts,
              )
            }
            Error(error) -> {
              let payload =
                segment_types.SegmentMutationPayload(
                  segment: None,
                  deleted_segment_id: None,
                  user_errors: [error],
                )
              let json_payload =
                serializers.segment_payload_json(
                  payload,
                  "SegmentUpdatePayload",
                  field,
                  fragments,
                )
              #(
                SegmentMutationFieldResult(
                  key: key,
                  payload: Some(json_payload),
                  top_level_errors: [],
                  staged_resource_ids: [],
                ),
                store,
                identity,
              )
            }
          }
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
              "SegmentUpdatePayload",
              field,
              fragments,
            )
          #(
            SegmentMutationFieldResult(
              key: key,
              payload: Some(json_payload),
              top_level_errors: [],
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
      }
    }
  }
}

fn handle_segment_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(SegmentMutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case validate_segment_id_argument(field, args, key) {
    SegmentIdInvalidGlobalId(error) -> #(
      SegmentMutationFieldResult(
        key: key,
        payload: None,
        top_level_errors: [error],
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    SegmentIdWrongResourceType(error) -> #(
      SegmentMutationFieldResult(
        key: key,
        payload: Some(json.null()),
        top_level_errors: [error],
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    SegmentIdValid(id_value) -> {
      let existing = store.get_effective_segment_by_id(store, id_value)
      let errors = case existing {
        Some(_) -> []
        None -> [
          segment_types.user_error(["id"], "Segment does not exist", None),
        ]
      }
      case errors {
        [] -> {
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
            SegmentMutationFieldResult(
              key: key,
              payload: Some(json_payload),
              top_level_errors: [],
              staged_resource_ids: [],
            ),
            store_after,
            identity,
          )
        }
        _ -> {
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
            SegmentMutationFieldResult(
              key: key,
              payload: Some(json_payload),
              top_level_errors: [],
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
      }
    }
  }
}

fn handle_customer_segment_members_query_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(SegmentMutationFieldResult, Store, SyntheticIdentityRegistry) {
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
        SegmentMutationFieldResult(
          key: key,
          payload: Some(json_payload),
          top_level_errors: [],
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
        SegmentMutationFieldResult(
          key: key,
          payload: Some(json_payload),
          top_level_errors: [],
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
      case normalize_segment_name(name) {
        "" -> [
          segment_types.user_error(field_path, "Name can't be blank", None),
        ]
        normalized ->
          case
            string.length(normalized) > segment_types.max_segment_name_length
          {
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

fn segment_limit_errors_for_count(
  segment_count: Int,
) -> List(segment_types.UserError) {
  case segment_count >= segment_types.max_segments_per_shop {
    True -> [
      segment_types.null_field_user_error(
        "Segment limit reached. Delete an existing segment to create more.",
        None,
      ),
    ]
    False -> []
  }
}

/// Resolve a segment name against existing names, bumping a trailing " (N)"
/// suffix when present. Shopify returns Taken after 10 duplicate retries. The
/// `current_id` argument lets `segmentUpdate` keep its existing name without
/// colliding with itself.
@internal
pub fn resolve_unique_segment_name(
  store: Store,
  requested: String,
  current_id: Option(String),
) -> Result(String, segment_types.UserError) {
  let #(_, used_names) = segment_name_inventory(store, current_id)
  resolve_unique_segment_name_from_used_set(used_names, requested)
}

fn resolve_unique_segment_name_from_used_set(
  used_names: Dict(String, Bool),
  requested: String,
) -> Result(String, segment_types.UserError) {
  case dict.get(used_names, requested) {
    Error(_) -> Ok(requested)
    Ok(_) ->
      next_unique_candidate(
        used_names,
        increment_duplicate_segment_name(requested),
        10,
      )
  }
}

fn segment_name_inventory(
  store: Store,
  current_id: Option(String),
) -> #(Int, Dict(String, Bool)) {
  let deleted =
    dict.merge(
      store.base_state.deleted_segment_ids,
      store.staged_state.deleted_segment_ids,
    )
  dict.merge(store.base_state.segments, store.staged_state.segments)
  |> dict.fold(#(0, dict.new()), fn(acc, id, record) {
    let #(count, names) = acc
    case dict.get(deleted, id) {
      Ok(_) -> acc
      Error(_) -> {
        let names = case current_id {
          Some(current) if current == id -> names
          _ ->
            case record.name {
              Some(name) ->
                case string.length(name) > 0 {
                  True -> dict.insert(names, name, True)
                  False -> names
                }
              _ -> names
            }
        }
        #(count + 1, names)
      }
    }
  })
}

fn next_unique_candidate(
  used: Dict(String, Bool),
  candidate: String,
  remaining_attempts: Int,
) -> Result(String, segment_types.UserError) {
  case dict.get(used, candidate) {
    Error(_) -> Ok(candidate)
    Ok(_) ->
      case remaining_attempts <= 1 {
        True -> Error(segment_name_taken_error())
        False ->
          next_unique_candidate(
            used,
            increment_duplicate_segment_name(candidate),
            remaining_attempts - 1,
          )
      }
  }
}

fn segment_name_taken_error() -> segment_types.UserError {
  segment_types.user_error(["name"], "Name has already been taken", None)
}

fn increment_duplicate_segment_name(name: String) -> String {
  case string.ends_with(name, ")") {
    False -> name <> " (2)"
    True -> {
      let without_close = string.drop_end(name, 1)
      let reversed = string.to_graphemes(without_close) |> list.reverse
      let digits_reversed = list.take_while(reversed, is_decimal_digit)
      let rest = list.drop(reversed, list.length(digits_reversed))
      case digits_reversed, rest {
        [], _ -> name <> " (2)"
        [_, ..], ["(", " ", ..prefix_reversed] -> {
          let raw_number =
            digits_reversed
            |> list.reverse
            |> string.join("")
          case int.parse(raw_number) {
            Ok(number) ->
              prefix_reversed
              |> list.reverse
              |> string.join("")
              |> finish_incremented_duplicate_name(number + 1)
            Error(_) -> name <> " (2)"
          }
        }
        _, _ -> name <> " (2)"
      }
    }
  }
}

fn finish_incremented_duplicate_name(prefix: String, number: Int) -> String {
  prefix <> " (" <> int.to_string(number) <> ")"
}

fn is_decimal_digit(char: String) -> Bool {
  case char {
    "0" -> True
    "1" -> True
    "2" -> True
    "3" -> True
    "4" -> True
    "5" -> True
    "6" -> True
    "7" -> True
    "8" -> True
    "9" -> True
    _ -> False
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
          case string.length(q) > segment_types.max_segment_query_length {
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

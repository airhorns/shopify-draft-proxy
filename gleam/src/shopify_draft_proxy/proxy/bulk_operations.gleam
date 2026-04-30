//// Mirrors the locally staged foundation of `src/proxy/bulk-operations.ts`.
////
//// This pass ports the BulkOperation state/read/cancel/run-query shell:
//// singular reads, catalog reads with cursor windows, current operation
//// derivation, local `bulkOperationCancel`, and local
//// `bulkOperationRunQuery` staging. Product JSONL export contents and
//// `bulkOperationRunMutation` import replay remain deferred until the
//// product/bulk-import substrate lands in Gleam.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionWindow,
  SerializeConnectionConfig, SrcNull, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type LogDraft, LogDraft}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type BulkOperationRecord, BulkOperationRecord,
}

pub type BulkOperationsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_bulk_operations_query_root(name: String) -> Bool {
  case name {
    "bulkOperation" -> True
    "bulkOperations" -> True
    "currentBulkOperation" -> True
    _ -> False
  }
}

pub fn is_bulk_operations_mutation_root(name: String) -> Bool {
  case name {
    "bulkOperationRunQuery" -> True
    "bulkOperationCancel" -> True
    _ -> False
  }
}

pub fn handle_bulk_operations_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, BulkOperationsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, BulkOperationsError) {
  use data <- result.try(handle_bulk_operations_query(
    store,
    document,
    variables,
  ))
  Ok(wrap_data(data))
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
        "bulkOperation" ->
          serialize_bulk_operation_by_id(store, field, fragments, variables)
        "currentBulkOperation" ->
          serialize_current_bulk_operation(store, field, fragments, variables)
        "bulkOperations" ->
          serialize_bulk_operations_connection(
            store,
            field,
            fragments,
            variables,
          )
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

fn read_arg_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(b)) -> Some(b)
    _ -> None
  }
}

fn serialize_bulk_operation_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_bulk_operation_by_id(store, id) {
        Some(operation) -> project_bulk_operation(operation, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_current_bulk_operation(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let requested_type = option.unwrap(read_arg_string(args, "type"), "QUERY")
  let operations =
    store.list_effective_bulk_operations(store)
    |> list.filter(fn(operation) { operation.type_ == requested_type })
    |> sort_bulk_operations("CREATED_AT", False)
  case operations {
    [first, ..] -> project_bulk_operation(first, field, fragments)
    [] -> json.null()
  }
}

fn serialize_bulk_operations_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let raw_query = read_arg_string(args, "query")
  let sort_key = option.unwrap(read_arg_string(args, "sortKey"), "CREATED_AT")
  let reverse = option.unwrap(read_arg_bool(args, "reverse"), False)
  let operations =
    store.list_effective_bulk_operations(store)
    |> search_query_parser.apply_search_query(
      raw_query,
      search_query_parser.default_parse_options(),
      matches_positive_bulk_operation_term,
    )
    |> sort_bulk_operations(sort_key, reverse)
  let window =
    paginate_connection_items(
      operations,
      field,
      variables,
      bulk_operation_cursor,
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
      get_cursor_value: bulk_operation_cursor,
      serialize_node: fn(operation, node_field, _index) {
        project_bulk_operation(operation, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn project_bulk_operation(
  operation: BulkOperationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(ss), ..) -> {
      let SelectionSet(selections: selections, ..) = ss
      project_graphql_value(
        bulk_operation_source(operation),
        selections,
        fragments,
      )
    }
    _ -> json.object([])
  }
}

fn bulk_operation_source(operation: BulkOperationRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("BulkOperation")),
    #("id", SrcString(operation.id)),
    #("status", SrcString(operation.status)),
    #("type", SrcString(operation.type_)),
    #("errorCode", optional_string_to_source(operation.error_code)),
    #("createdAt", SrcString(operation.created_at)),
    #("completedAt", optional_string_to_source(operation.completed_at)),
    #("objectCount", SrcString(operation.object_count)),
    #("rootObjectCount", SrcString(operation.root_object_count)),
    #("fileSize", optional_string_to_source(operation.file_size)),
    #("url", optional_string_to_source(operation.url)),
    #("partialDataUrl", optional_string_to_source(operation.partial_data_url)),
    #("query", optional_string_to_source(operation.query)),
  ])
}

fn optional_string_to_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn bulk_operation_cursor(
  operation: BulkOperationRecord,
  _index: Int,
) -> String {
  option.unwrap(operation.cursor, operation.id)
}

fn sort_bulk_operations(
  operations: List(BulkOperationRecord),
  sort_key: String,
  reverse: Bool,
) -> List(BulkOperationRecord) {
  let sorted =
    list.sort(operations, fn(left, right) {
      case string.uppercase(sort_key) {
        "ID" -> string.compare(left.id, right.id)
        _ -> {
          let date_order = string.compare(right.created_at, left.created_at)
          case date_order {
            order.Eq -> string.compare(right.id, left.id)
            _ -> date_order
          }
        }
      }
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

fn matches_positive_bulk_operation_term(
  operation: BulkOperationRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let field = case term.field {
    Some(raw) -> string.lowercase(raw)
    None -> "default"
  }
  case field {
    "default" | "id" ->
      search_query_parser.matches_search_query_string(
        Some(operation.id),
        term.value,
        search_query_parser.IncludesMatch,
        search_query_parser.default_string_match_options(),
      )
      || search_query_parser.matches_search_query_string(
        Some(last_gid_segment(operation.id)),
        term.value,
        search_query_parser.IncludesMatch,
        search_query_parser.default_string_match_options(),
      )
    "status" ->
      search_query_parser.matches_search_query_string(
        Some(operation.status),
        term.value,
        search_query_parser.IncludesMatch,
        search_query_parser.default_string_match_options(),
      )
    "operation_type" | "type" ->
      search_query_parser.matches_search_query_string(
        Some(operation.type_),
        term.value,
        search_query_parser.IncludesMatch,
        search_query_parser.default_string_match_options(),
      )
    "created_at" ->
      search_query_parser.matches_search_query_date(
        Some(operation.created_at),
        term,
        1_704_067_200_000,
      )
    _ -> False
  }
}

fn last_gid_segment(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(segment) -> segment
    Error(_) -> id
  }
}

// ===========================================================================
// Mutation path
// ===========================================================================

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

pub type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, BulkOperationsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
      ))
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
            "bulkOperationRunQuery" ->
              Some(handle_bulk_operation_run_query(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "bulkOperationCancel" ->
              Some(handle_bulk_operation_cancel(
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
  let root_names = mutation_root_names(fields)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let log_drafts = [
    LogDraft(
      operation_name: primary_root,
      root_fields: root_names,
      primary_root_field: primary_root,
      domain: "bulk-operations",
      execution: "stage-locally",
      staged_resource_ids: all_staged,
      status: store.Staged,
      notes: Some(
        "Handled BulkOperation mutation locally against the in-memory BulkOperation job store.",
      ),
    ),
  ]
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: log_drafts,
  )
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

fn handle_bulk_operation_run_query(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let query = read_arg_string(args, "query")
  let group_objects = option.unwrap(read_arg_bool(args, "groupObjects"), False)
  case query, group_objects {
    None, _ -> #(
      MutationFieldResult(
        key: key,
        payload: serialize_run_query_payload(
          field,
          None,
          [
            UserError(
              field: Some(["query"]),
              message: "Bulk query is required.",
              code: Some("INVALID"),
            ),
          ],
          fragments,
        ),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    _, True -> #(
      MutationFieldResult(
        key: key,
        payload: serialize_run_query_payload(
          field,
          None,
          [
            UserError(
              field: Some(["groupObjects"]),
              message: "groupObjects is not supported by the local bulk query executor.",
              code: Some("INVALID"),
            ),
          ],
          fragments,
        ),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    Some(query_string), False -> {
      let #(operation_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "BulkOperation")
      let #(created_at, identity_after_created) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let #(completed_at, identity_after_completed) =
        synthetic_identity.make_synthetic_timestamp(identity_after_created)
      let result_jsonl = ""
      let operation =
        BulkOperationRecord(
          id: operation_id,
          status: "COMPLETED",
          type_: "QUERY",
          error_code: None,
          created_at: created_at,
          completed_at: Some(completed_at),
          object_count: "0",
          root_object_count: "0",
          file_size: Some(int.to_string(string.length(result_jsonl))),
          url: Some(build_bulk_operation_result_url(operation_id)),
          partial_data_url: None,
          query: Some(query_string),
          cursor: None,
          result_jsonl: Some(result_jsonl),
        )
      let #(staged, next_store) =
        store.stage_bulk_operation_result(store, operation, result_jsonl)
      #(
        MutationFieldResult(
          key: key,
          payload: serialize_run_query_payload(
            field,
            Some(staged),
            [],
            fragments,
          ),
          staged_resource_ids: [staged.id],
        ),
        next_store,
        identity_after_completed,
      )
    }
  }
}

fn handle_bulk_operation_cancel(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None -> #(
      MutationFieldResult(
        key: key,
        payload: serialize_cancel_payload(
          field,
          None,
          [
            missing_bulk_operation_error(),
          ],
          fragments,
        ),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
    Some(id) -> {
      let staged_operation = store.get_staged_bulk_operation_by_id(store, id)
      let effective_operation = case staged_operation {
        Some(op) -> Some(op)
        None -> store.get_effective_bulk_operation_by_id(store, id)
      }
      case effective_operation {
        None -> #(
          MutationFieldResult(
            key: key,
            payload: serialize_cancel_payload(
              field,
              None,
              [
                missing_bulk_operation_error(),
              ],
              fragments,
            ),
            staged_resource_ids: [],
          ),
          store,
          identity,
        )
        Some(operation) ->
          case is_terminal_status(operation.status) {
            True -> #(
              MutationFieldResult(
                key: key,
                payload: serialize_cancel_payload(
                  field,
                  Some(operation),
                  [
                    terminal_cancel_error(operation),
                  ],
                  fragments,
                ),
                staged_resource_ids: [operation.id],
              ),
              store,
              identity,
            )
            False ->
              case staged_operation {
                None -> #(
                  MutationFieldResult(
                    key: key,
                    payload: serialize_cancel_payload(
                      field,
                      None,
                      [
                        missing_bulk_operation_error(),
                      ],
                      fragments,
                    ),
                    staged_resource_ids: [],
                  ),
                  store,
                  identity,
                )
                Some(_) -> {
                  let #(canceled, next_store) =
                    store.cancel_staged_bulk_operation(store, id)
                  let staged_id = case canceled {
                    Some(op) -> [op.id]
                    None -> []
                  }
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: serialize_cancel_payload(
                        field,
                        canceled,
                        [],
                        fragments,
                      ),
                      staged_resource_ids: staged_id,
                    ),
                    next_store,
                    identity,
                  )
                }
              }
          }
      }
    }
  }
}

fn serialize_run_query_payload(
  field: Selection,
  operation: Option(BulkOperationRecord),
  user_errors: List(UserError),
  fragments: FragmentMap,
) -> Json {
  serialize_operation_payload(field, operation, user_errors, fragments)
}

fn serialize_cancel_payload(
  field: Selection,
  operation: Option(BulkOperationRecord),
  user_errors: List(UserError),
  fragments: FragmentMap,
) -> Json {
  serialize_operation_payload(field, operation, user_errors, fragments)
}

fn serialize_operation_payload(
  field: Selection,
  operation: Option(BulkOperationRecord),
  user_errors: List(UserError),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "bulkOperation" ->
                case operation {
                  Some(op) -> #(
                    key,
                    project_bulk_operation(op, child, fragments),
                  )
                  None -> #(key, json.null())
                }
              "userErrors" -> #(key, serialize_user_errors(user_errors, child))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_user_errors(
  user_errors: List(UserError),
  field: Selection,
) -> Json {
  let children =
    get_selected_child_fields(field, default_selected_field_options())
  json.array(user_errors, fn(error) {
    let entries =
      list.map(children, fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "field" ->
                case error.field {
                  Some(parts) -> #(key, json.array(parts, json.string))
                  None -> #(key, json.null())
                }
              "message" -> #(key, json.string(error.message))
              "code" ->
                case error.code {
                  Some(code) -> #(key, json.string(code))
                  None -> #(key, json.null())
                }
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

fn missing_bulk_operation_error() -> UserError {
  UserError(
    field: Some(["id"]),
    message: "Bulk operation does not exist",
    code: None,
  )
}

fn terminal_cancel_error(operation: BulkOperationRecord) -> UserError {
  UserError(
    field: None,
    message: "A bulk operation cannot be canceled when it is "
      <> string.lowercase(operation.status),
    code: None,
  )
}

fn is_terminal_status(status: String) -> Bool {
  case status {
    "CANCELED" | "COMPLETED" | "EXPIRED" | "FAILED" -> True
    _ -> False
  }
}

fn build_bulk_operation_result_url(operation_id: String) -> String {
  "https://shopify-draft-proxy.local/__meta/bulk-operations/"
  <> last_gid_segment(operation_id)
  <> "/result.jsonl"
}

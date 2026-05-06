//// Bulk-operations query handling.
//// Mirrors the locally staged foundation of `src/proxy/bulk-operations.ts`.
////
//// This pass ports the BulkOperation state/read/cancel/run-query/import
//// foundation: singular reads, catalog reads with cursor windows, current
//// operation derivation, local `bulkOperationCancel`, product/productVariant
//// JSONL query exports, and local `bulkOperationRunMutation` replay for
//// product-domain inner mutations.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/bulk_operations/serializers.{
  project_bulk_operation,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionWindow, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, serialize_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{type BulkOperationRecord}

@internal
pub fn is_bulk_operations_query_root(name: String) -> Bool {
  case name {
    "bulkOperation" -> True
    "bulkOperations" -> True
    "currentBulkOperation" -> True
    _ -> False
  }
}

@internal
pub fn handle_bulk_operations_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_bulk_operations_query(
    store,
    document,
    variables,
  ))
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
    "Failed to handle bulk operations query",
  )
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

fn serialize_bulk_operation_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
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
  let args = graphql_helpers.field_args(field, variables)
  let requested_type =
    option.unwrap(
      graphql_helpers.read_arg_string_nonempty(args, "type"),
      "QUERY",
    )
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
  let args = graphql_helpers.field_args(field, variables)
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let sort_key =
    option.unwrap(
      graphql_helpers.read_arg_string_nonempty(args, "sortKey"),
      "CREATED_AT",
    )
  let reverse =
    option.unwrap(graphql_helpers.read_arg_bool(args, "reverse"), False)
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

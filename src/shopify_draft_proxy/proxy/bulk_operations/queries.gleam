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
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, Location}
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
import shopify_draft_proxy/state/iso_timestamp
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
  process_with_operation_name(store, document, variables, None)
}

/// Uniform query entrypoint matching the dispatcher's signature.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process_with_operation_name(proxy.store, document, variables, parsed.name),
    "Failed to handle bulk operations query",
  )
}

fn process_with_operation_name(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: Option(String),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  case
    first_top_level_validation_error(
      fields,
      document,
      variables,
      operation_name,
    )
  {
    Some(envelope) -> Ok(envelope)
    None -> {
      let fragments = get_document_fragments(document)
      let data = serialize_root_fields(store, fields, fragments, variables)
      Ok(envelope_with_search_warnings(data, fields, variables))
    }
  }
}

fn first_top_level_validation_error(
  fields: List(Selection),
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: Option(String),
) -> Option(Json) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case
        top_level_validation_error(field, document, variables, operation_name)
      {
        Some(envelope) -> Some(envelope)
        None ->
          first_top_level_validation_error(
            rest,
            document,
            variables,
            operation_name,
          )
      }
  }
}

fn top_level_validation_error(
  field: Selection,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: Option(String),
) -> Option(Json) {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "bulkOperations" ->
          bulk_operations_connection_error(field, document, variables)
        "bulkOperation" ->
          bulk_operation_id_error(field, document, variables, operation_name)
        _ -> None
      }
    _ -> None
  }
}

fn bulk_operations_connection_error(
  field: Selection,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Json) {
  let args = graphql_helpers.field_args(field, variables)
  let first = graphql_helpers.read_arg_int(args, "first")
  let last = graphql_helpers.read_arg_int(args, "last")
  case first, last {
    None, None ->
      Some(bad_request_error_envelope(
        field,
        document,
        "you must provide one of first or last",
        ["bulkOperations"],
        True,
      ))
    Some(_), Some(_) ->
      Some(bad_request_error_envelope(
        field,
        document,
        "providing both first and last is not supported",
        ["bulkOperations"],
        True,
      ))
    _, _ ->
      case graphql_helpers.read_arg_string_nonempty(args, "query") {
        Some(raw_query) ->
          case created_at_validation_error(raw_query) {
            True ->
              Some(bad_request_error_envelope(
                field,
                document,
                "Invalid timestamp for query filter `created_at`.",
                ["bulkOperations"],
                True,
              ))
            False -> None
          }
        None -> None
      }
  }
}

fn created_at_validation_error(raw_query: String) -> Bool {
  case
    search_query_parser.parse_search_query(
      raw_query,
      search_query_parser.default_parse_options(),
    )
  {
    Some(node) -> search_query_node_has_invalid_created_at(node)
    None -> False
  }
}

fn search_query_node_has_invalid_created_at(
  node: search_query_parser.SearchQueryNode,
) -> Bool {
  case node {
    search_query_parser.TermNode(term: term) ->
      case term.field {
        Some(field) ->
          case string.lowercase(field) {
            "created_at" -> !valid_search_time_value(term.value)
            _ -> False
          }
        _ -> False
      }
    search_query_parser.AndNode(children: children) ->
      list.any(children, search_query_node_has_invalid_created_at)
    search_query_parser.OrNode(children: children) ->
      list.any(children, search_query_node_has_invalid_created_at)
    search_query_parser.NotNode(child: child) ->
      search_query_node_has_invalid_created_at(child)
  }
}

fn valid_search_time_value(value: String) -> Bool {
  let normalized = search_query_parser.normalize_search_query_value(value)
  case normalized {
    "now" -> True
    _ ->
      case iso_timestamp.parse_iso(normalized) {
        Ok(_) -> True
        Error(_) -> False
      }
  }
}

fn bulk_operation_id_error(
  field: Selection,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: Option(String),
) -> Option(Json) {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    Some(id) ->
      case valid_bulk_operation_gid(id) {
        True -> None
        False ->
          case string.starts_with(id, "gid://shopify/") {
            True ->
              Some(
                json.object([
                  #(
                    "errors",
                    json.preprocessed_array([
                      resource_not_found_id_error(field, document, id),
                    ]),
                  ),
                  #(
                    "data",
                    json.object([#(get_field_response_key(field), json.null())]),
                  ),
                ]),
              )
            False ->
              Some(
                json.object([
                  #(
                    "errors",
                    json.preprocessed_array([
                      invalid_global_id_literal_error(
                        field,
                        document,
                        id,
                        operation_name,
                      ),
                    ]),
                  ),
                ]),
              )
          }
      }
    None -> None
  }
}

fn valid_bulk_operation_gid(id: String) -> Bool {
  let prefix = "gid://shopify/BulkOperation/"
  string.starts_with(id, prefix) && string.length(id) > string.length(prefix)
}

fn envelope_with_search_warnings(
  data: Json,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let warnings =
    list.flat_map(fields, fn(field) {
      case field {
        Field(name: name, ..) if name.value == "bulkOperations" ->
          bulk_operations_search_warning_extensions(field, variables)
        _ -> []
      }
    })
  case warnings {
    [] -> graphql_helpers.wrap_data(data)
    _ ->
      json.object([
        #("data", data),
        #(
          "extensions",
          json.object([#("search", json.preprocessed_array(warnings))]),
        ),
      ])
  }
}

fn bulk_operations_search_warning_extensions(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "query") {
    Some(raw_query) ->
      case
        search_query_parser.parse_search_query(
          raw_query,
          search_query_parser.default_parse_options(),
        )
      {
        Some(node) -> {
          let warnings = invalid_filter_value_warnings(node)
          case warnings {
            [] -> []
            _ -> [
              json.object([
                #("path", json.array(["bulkOperations"], json.string)),
                #("query", json.string(raw_query)),
                #("parsed", parsed_search_warning_json(warnings)),
                #(
                  "warnings",
                  json.preprocessed_array(list.map(warnings, warning_json)),
                ),
              ]),
            ]
          }
        }
        None -> []
      }
    None -> []
  }
}

type SearchWarning {
  SearchWarning(field: String, value: String)
}

fn invalid_filter_value_warnings(
  node: search_query_parser.SearchQueryNode,
) -> List(SearchWarning) {
  case node {
    search_query_parser.TermNode(term: term) ->
      invalid_filter_term_warning(term)
    search_query_parser.AndNode(children: children) ->
      list.flat_map(children, invalid_filter_value_warnings)
    search_query_parser.OrNode(children: children) ->
      list.flat_map(children, invalid_filter_value_warnings)
    search_query_parser.NotNode(child: child) ->
      invalid_filter_value_warnings(child)
  }
}

fn invalid_filter_term_warning(
  term: search_query_parser.SearchQueryTerm,
) -> List(SearchWarning) {
  let raw_value =
    term.value
    |> search_query_parser.strip_search_query_value_quotes
    |> string.trim
  case term.field {
    Some(field) -> {
      let normalized_field = string.lowercase(field)
      let normalized_value = string.lowercase(raw_value)
      case normalized_field {
        "status" ->
          case
            list.contains(
              [
                "canceled",
                "canceling",
                "completed",
                "created",
                "failed",
                "running",
              ],
              normalized_value,
            )
          {
            True -> []
            False -> [SearchWarning(field: "status", value: raw_value)]
          }
        "operation_type" ->
          case list.contains(["query", "mutation"], normalized_value) {
            True -> []
            False -> [SearchWarning(field: "operation_type", value: raw_value)]
          }
        _ -> []
      }
    }
    None -> []
  }
}

fn parsed_search_warning_json(warnings: List(SearchWarning)) -> Json {
  case warnings {
    [SearchWarning(field: field, value: value), ..] ->
      json.object([
        #("field", json.string(field)),
        #("match_all", json.string(value)),
      ])
    [] -> json.object([])
  }
}

fn warning_json(warning: SearchWarning) -> Json {
  let SearchWarning(field: field, value: value) = warning
  json.object([
    #("field", json.string(field)),
    #(
      "message",
      json.string("Input `" <> value <> "` is not an accepted value."),
    ),
    #("code", json.string("invalid_value")),
  ])
}

fn bad_request_error_envelope(
  field: Selection,
  document: String,
  message: String,
  path: List(String),
  include_null_data: Bool,
) -> Json {
  let entries = [
    #(
      "errors",
      json.preprocessed_array([
        json.object([
          #("message", json.string(message)),
          #("locations", field_locations_json(field, document)),
          #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
          #("path", json.array(path, json.string)),
        ]),
      ]),
    ),
  ]
  let entries = case include_null_data {
    True -> list.append(entries, [#("data", json.null())])
    False -> entries
  }
  json.object(entries)
}

fn resource_not_found_id_error(
  field: Selection,
  document: String,
  id: String,
) -> Json {
  json.object([
    #("message", json.string("Invalid id: " <> id)),
    #("locations", field_locations_json(field, document)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
    #("path", json.array(["bulkOperation"], json.string)),
  ])
}

fn invalid_global_id_literal_error(
  field: Selection,
  document: String,
  id: String,
  operation_name: Option(String),
) -> Json {
  json.object([
    #("message", json.string("Invalid global id '" <> id <> "'")),
    #("locations", field_locations_json(field, document)),
    #(
      "path",
      json.array(
        [
          operation_path(operation_name),
          "bulkOperation",
          "id",
        ],
        json.string,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("argumentLiteralsIncompatible")),
        #("typeName", json.string("CoercionError")),
      ]),
    ),
  ])
}

fn operation_path(operation_name: Option(String)) -> String {
  case operation_name {
    Some(name) -> "query " <> name
    None -> "query"
  }
}

fn field_locations_json(field: Selection, document: String) -> Json {
  json.array(field_locations(field, document), fn(pair) {
    let #(line, column) = pair
    json.object([#("line", json.int(line)), #("column", json.int(column))])
  })
}

fn field_locations(field: Selection, document: String) -> List(#(Int, Int)) {
  case field {
    Field(loc: Some(Location(start: start, ..)), ..) -> [
      offset_to_line_column(document, start),
    ]
    _ -> []
  }
}

fn offset_to_line_column(document: String, offset: Int) -> #(Int, Int) {
  document
  |> string.to_graphemes()
  |> list.take(offset)
  |> list.fold(#(1, 1), fn(acc, char) {
    let #(line, column) = acc
    case char {
      "\n" -> #(line + 1, 1)
      _ -> #(line, column + 1)
    }
  })
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

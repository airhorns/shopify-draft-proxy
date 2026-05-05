//// Mirrors the locally staged foundation of `src/proxy/bulk-operations.ts`.
////
//// This pass ports the BulkOperation state/read/cancel/run-query/import
//// foundation: singular reads, catalog reads with cursor windows, current
//// operation derivation, local `bulkOperationCancel`, product/productVariant
//// JSONL query exports, and local `bulkOperationRunMutation` replay for
//// product-domain inner mutations.

import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Selection, Field, FragmentDefinition, FragmentSpread,
  InlineFragment, Mutation, OperationDefinition, Query, SelectionSet,
  Subscription,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionWindow,
  SerializeConnectionConfig, SrcList, SrcNull, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, LogDraft, MutationOutcome,
  respond_to_query,
}
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type BulkOperationRecord, type ProductRecord, type ProductVariantRecord,
  BulkOperationRecord,
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
    "bulkOperationRunMutation" -> True
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
    #("errorCode", graphql_helpers.option_string_source(operation.error_code)),
    #("createdAt", SrcString(operation.created_at)),
    #(
      "completedAt",
      graphql_helpers.option_string_source(operation.completed_at),
    ),
    #("objectCount", SrcString(operation.object_count)),
    #("rootObjectCount", SrcString(operation.root_object_count)),
    #("fileSize", graphql_helpers.option_string_source(operation.file_size)),
    #("url", graphql_helpers.option_string_source(operation.url)),
    #(
      "partialDataUrl",
      graphql_helpers.option_string_source(operation.partial_data_url),
    ),
    #("query", graphql_helpers.option_string_source(operation.query)),
  ])
}

fn bulk_operation_cursor(
  operation: BulkOperationRecord,
  _index: Int,
) -> String {
  option.unwrap(operation.cursor, operation.id)
}

fn created_bulk_operation_response(
  operation: BulkOperationRecord,
) -> BulkOperationRecord {
  BulkOperationRecord(
    ..operation,
    status: "CREATED",
    error_code: None,
    completed_at: None,
    object_count: "0",
    root_object_count: "0",
    file_size: None,
    url: None,
    partial_data_url: None,
  )
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

pub type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

type InnerMutationValidationError {
  InnerMutationParseError(String)
  InnerMutationInvalidOperationType
  InnerMutationAnalysisErrors(List(String))
}

const max_bulk_query_connections: Int = 5

const max_bulk_query_connection_depth: Int = 2

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        request_path,
        fields,
        fragments,
        variables,
        upstream,
      )
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, field_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
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
                upstream,
              ))
            "bulkOperationRunMutation" ->
              Some(handle_bulk_operation_run_mutation(
                current_store,
                current_identity,
                request_path,
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
                upstream,
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
              list.append(drafts, result.log_drafts),
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
  let outer_status = case primary_root, field_drafts {
    Some("bulkOperationRunMutation"), [] -> store.Failed
    _, _ -> store.Staged
  }
  let outer_log_drafts = [
    LogDraft(
      operation_name: primary_root,
      root_fields: root_names,
      primary_root_field: primary_root,
      domain: "bulk-operations",
      execution: "stage-locally",
      query: None,
      variables: None,
      staged_resource_ids: all_staged,
      status: outer_status,
      notes: Some(
        "Handled BulkOperation mutation locally against the in-memory BulkOperation job store.",
      ),
    ),
  ]
  let log_drafts = case primary_root, field_drafts {
    Some("bulkOperationRunMutation"), [_, ..] -> field_drafts
    _, _ -> outer_log_drafts
  }
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
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let query = graphql_helpers.read_arg_string(args, "query")
  case query {
    None -> #(
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
        log_drafts: [],
      ),
      store,
      identity,
    )
    Some(query_string) ->
      case build_run_query_jsonl(store, query_string, upstream) {
        Error(errors) -> #(
          MutationFieldResult(
            key: key,
            payload: serialize_run_query_payload(field, None, errors, fragments),
            staged_resource_ids: [],
            log_drafts: [],
          ),
          store,
          identity,
        )
        Ok(result) -> {
          let BulkQueryResult(
            result_jsonl: result_jsonl,
            object_count: object_count,
            root_object_count: root_object_count,
          ) = result
          let #(operation_id, identity_after_id) =
            synthetic_identity.make_synthetic_gid(identity, "BulkOperation")
          let #(created_at, identity_after_created) =
            synthetic_identity.make_synthetic_timestamp(identity_after_id)
          let #(completed_at, identity_after_completed) =
            synthetic_identity.make_synthetic_timestamp(identity_after_created)
          let operation =
            BulkOperationRecord(
              id: operation_id,
              status: "COMPLETED",
              type_: "QUERY",
              error_code: None,
              created_at: created_at,
              completed_at: Some(completed_at),
              object_count: int.to_string(object_count),
              root_object_count: int.to_string(root_object_count),
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
                Some(created_bulk_operation_response(staged)),
                [],
                fragments,
              ),
              staged_resource_ids: [staged.id],
              log_drafts: [],
            ),
            next_store,
            identity_after_completed,
          )
        }
      }
  }
}

type BulkQueryResult {
  BulkQueryResult(
    result_jsonl: String,
    object_count: Int,
    root_object_count: Int,
  )
}

fn build_run_query_jsonl(
  store: Store,
  query_string: String,
  upstream: UpstreamContext,
) -> Result(BulkQueryResult, List(UserError)) {
  case root_field.get_root_fields(query_string) {
    Ok(fields) -> {
      let fragments = get_document_fragments(query_string)
      let validation_errors = validate_admin_bulk_query(fields, fragments)
      case validation_errors {
        [_, ..] -> Error(validation_errors)
        [] ->
          case fields {
            [root] ->
              case selected_bulk_query_node_fields(root, fragments) {
                Some(node_fields) ->
                  case find_in_progress_bulk_operation(store, "QUERY") {
                    Some(operation) ->
                      Error([operation_in_progress_error(operation, "query")])
                    None ->
                      case root_field_name(root) {
                        Some("products") -> {
                          let products = store.list_effective_products(store)
                          let root_count =
                            local_or_upstream_products_count(
                              products,
                              root,
                              upstream,
                            )
                          Ok(BulkQueryResult(
                            result_jsonl: make_jsonl(
                              list.map(products, fn(product) {
                                project_graphql_value(
                                  product_export_source(product),
                                  node_fields,
                                  fragments,
                                )
                              }),
                            ),
                            object_count: root_count,
                            root_object_count: root_count,
                          ))
                        }
                        Some("productVariants") -> {
                          let variants =
                            store.list_effective_product_variants(store)
                          let root_count = list.length(variants)
                          Ok(BulkQueryResult(
                            result_jsonl: make_jsonl(
                              list.map(variants, fn(variant) {
                                project_graphql_value(
                                  product_variant_export_source(store, variant),
                                  node_fields,
                                  fragments,
                                )
                              }),
                            ),
                            object_count: root_count,
                            root_object_count: root_count,
                          ))
                        }
                        _ -> Error([no_connection_bulk_query_error()])
                      }
                  }
                None -> Error([no_connection_bulk_query_error()])
              }
            _ ->
              Error([
                UserError(
                  field: Some(["query"]),
                  message: "Bulk queries must contain exactly one top-level field.",
                  code: Some("INVALID"),
                ),
              ])
          }
      }
    }
    Error(_) ->
      Error([
        invalid_bulk_query_error("syntax error, unexpected end of file"),
      ])
  }
}

fn local_or_upstream_products_count(
  products: List(ProductRecord),
  root: Selection,
  upstream: UpstreamContext,
) -> Int {
  let local_count = list.length(products)
  case local_count {
    0 ->
      option.unwrap(fetch_upstream_products_count(root, upstream), local_count)
    _ -> local_count
  }
}

fn fetch_upstream_products_count(
  root: Selection,
  upstream: UpstreamContext,
) -> Option(Int) {
  // Pattern 2: bulkOperationRunQuery stays a local staged mutation, but
  // a cold LiveHybrid product export reads Shopify's product count so
  // the staged BulkOperation counters match the upstream store.
  let args = graphql_helpers.field_args(root, dict.new())
  let variables = case graphql_helpers.read_arg_string_nonempty(args, "query") {
    Some(query) -> json.object([#("query", json.string(query))])
    None -> json.object([])
  }
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "BulkOperationRunQueryProductCount",
      product_count_hydrate_query(),
      variables,
    )
  {
    Ok(value) -> product_count_from_response(value)
    Error(_) -> None
  }
}

fn product_count_hydrate_query() -> String {
  "query BulkOperationRunQueryProductCount($query: String) { "
  <> "productsCount(query: $query) { count } "
  <> "}"
}

fn product_count_from_response(value: commit.JsonValue) -> Option(Int) {
  use data <- option.then(json_get(value, "data"))
  use count_obj <- option.then(json_get(data, "productsCount"))
  json_get_int(count_obj, "count")
}

fn no_connection_bulk_query_error() -> UserError {
  UserError(
    field: Some(["query"]),
    message: "Bulk queries must contain at least one connection.",
    code: Some("INVALID"),
  )
}

fn invalid_bulk_query_error(reason: String) -> UserError {
  UserError(
    field: Some(["query"]),
    message: "Invalid bulk query: " <> reason,
    code: Some("INVALID"),
  )
}

type BulkQueryAnalysis {
  BulkQueryAnalysis(
    connection_count: Int,
    max_connection_depth: Int,
    top_level_node: Bool,
    connections_without_node: List(String),
    nested_connections_without_parent_id: List(String),
  )
}

fn empty_bulk_query_analysis() -> BulkQueryAnalysis {
  BulkQueryAnalysis(
    connection_count: 0,
    max_connection_depth: 0,
    top_level_node: False,
    connections_without_node: [],
    nested_connections_without_parent_id: [],
  )
}

type ConnectionSelection {
  ConnectionSelection(
    name: String,
    nodes_field: Option(Selection),
    edges_node_field: Option(Selection),
  )
}

fn validate_admin_bulk_query(
  root_fields: List(Selection),
  fragments: FragmentMap,
) -> List(UserError) {
  let analysis =
    list.fold(root_fields, empty_bulk_query_analysis(), fn(acc, field) {
      let acc = case field {
        Field(name: name, ..) if name.value == "node" ->
          BulkQueryAnalysis(..acc, top_level_node: True)
        _ -> acc
      }
      analyze_bulk_query_field(field, fragments, -1, acc)
    })
  let structural_errors =
    []
    |> append_errors(case analysis.top_level_node {
      True -> [top_level_node_bulk_query_error()]
      False -> []
    })
    |> append_errors(
      case analysis.connection_count > max_bulk_query_connections {
        True -> [too_many_connections_bulk_query_error()]
        False -> []
      },
    )
    |> append_errors(
      case analysis.max_connection_depth > max_bulk_query_connection_depth {
        True -> [connection_nested_too_deep_bulk_query_error()]
        False -> []
      },
    )
    |> append_errors(case analysis.connections_without_node {
      [] -> []
      names -> [connection_without_node_field_error(names)]
    })
    |> append_errors(case analysis.nested_connections_without_parent_id {
      [] -> []
      names -> [nested_connection_without_parent_id_field_error(names)]
    })

  case analysis.connection_count {
    0 -> list.append(structural_errors, [no_connection_bulk_query_error()])
    _ -> structural_errors
  }
}

fn append_errors(
  errors: List(UserError),
  next_errors: List(UserError),
) -> List(UserError) {
  list.append(errors, next_errors)
}

fn analyze_bulk_query_field(
  field: Selection,
  fragments: FragmentMap,
  connection_depth: Int,
  analysis: BulkQueryAnalysis,
) -> BulkQueryAnalysis {
  case connection_selection(field, fragments) {
    Some(connection) -> {
      let next_depth = connection_depth + 1
      let analysis =
        BulkQueryAnalysis(
          ..analysis,
          connection_count: analysis.connection_count + 1,
          max_connection_depth: int.max(
            analysis.max_connection_depth,
            next_depth,
          ),
        )
      let analysis = case connection.edges_node_field, connection.nodes_field {
        Some(_), None -> analysis
        _, _ ->
          BulkQueryAnalysis(
            ..analysis,
            connections_without_node: append_unique_string(
              analysis.connections_without_node,
              connection.name,
            ),
          )
      }
      analyze_connection_node_selections(
        connection,
        fragments,
        next_depth,
        analysis,
      )
    }
    None ->
      direct_field_children(field, fragments)
      |> list.fold(analysis, fn(acc, child) {
        analyze_bulk_query_field(child, fragments, connection_depth, acc)
      })
  }
}

fn analyze_connection_node_selections(
  connection: ConnectionSelection,
  fragments: FragmentMap,
  connection_depth: Int,
  analysis: BulkQueryAnalysis,
) -> BulkQueryAnalysis {
  let node_fields =
    []
    |> append_optional_selection(connection.edges_node_field)
    |> append_optional_selection(connection.nodes_field)

  list.fold(node_fields, analysis, fn(acc, node_field) {
    let node_has_id = node_selection_has_unaliased_id(node_field, fragments)
    let acc = case
      node_has_id,
      node_selection_contains_connection(node_field, fragments)
    {
      False, True ->
        BulkQueryAnalysis(
          ..acc,
          nested_connections_without_parent_id: append_unique_string(
            acc.nested_connections_without_parent_id,
            connection.name,
          ),
        )
      _, _ -> acc
    }
    direct_field_children(node_field, fragments)
    |> list.fold(acc, fn(inner_acc, child) {
      analyze_bulk_query_field(child, fragments, connection_depth, inner_acc)
    })
  })
}

fn node_selection_contains_connection(
  field: Selection,
  fragments: FragmentMap,
) -> Bool {
  direct_field_children(field, fragments)
  |> list.any(fn(child) {
    case connection_selection(child, fragments) {
      Some(_) -> True
      None -> node_selection_contains_connection(child, fragments)
    }
  })
}

fn append_optional_selection(
  selections: List(Selection),
  maybe_selection: Option(Selection),
) -> List(Selection) {
  case maybe_selection {
    Some(selection) -> list.append(selections, [selection])
    None -> selections
  }
}

fn connection_selection(
  field: Selection,
  fragments: FragmentMap,
) -> Option(ConnectionSelection) {
  case field {
    Field(name: name, ..) -> {
      let children = direct_field_children(field, fragments)
      let nodes_field = find_child_field(children, "nodes")
      let edges_node_field =
        find_child_field(children, "edges")
        |> option.then(fn(edges_field) {
          find_child_field(
            direct_field_children(edges_field, fragments),
            "node",
          )
        })
      case nodes_field, find_child_field(children, "edges") {
        None, None -> None
        _, _ ->
          Some(ConnectionSelection(
            name: name.value,
            nodes_field: nodes_field,
            edges_node_field: edges_node_field,
          ))
      }
    }
    _ -> None
  }
}

fn node_selection_has_unaliased_id(
  node_field: Selection,
  fragments: FragmentMap,
) -> Bool {
  direct_field_children(node_field, fragments)
  |> list.any(fn(child) {
    case child {
      Field(alias: None, name: name, ..) if name.value == "id" -> True
      _ -> False
    }
  })
}

fn direct_field_children(
  field: Selection,
  fragments: FragmentMap,
) -> List(Selection) {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  flatten_bulk_query_selections(selections, fragments)
}

fn flatten_bulk_query_selections(
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(Selection) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(..) -> [selection]
      InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        flatten_bulk_query_selections(inner, fragments)
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) -> flatten_bulk_query_selections(inner, fragments)
          _ -> []
        }
    }
  })
}

fn append_unique_string(values: List(String), value: String) -> List(String) {
  case list.contains(values, value) {
    True -> values
    False -> list.append(values, [value])
  }
}

fn selected_bulk_query_node_fields(
  root: Selection,
  fragments: FragmentMap,
) -> Option(List(Selection)) {
  let children = direct_field_children(root, fragments)
  case find_child_field(children, "edges") {
    Some(edges_field) ->
      find_child_field(direct_field_children(edges_field, fragments), "node")
      |> option.map(fn(node_field) {
        direct_field_children(node_field, fragments)
      })
    None -> None
  }
}

fn find_child_field(
  fields: List(Selection),
  name: String,
) -> Option(Selection) {
  list.find_map(fields, fn(field) {
    case field {
      Field(name: field_name, ..) if field_name.value == name -> Ok(field)
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn root_field_name(field: Selection) -> Option(String) {
  case field {
    Field(name: name, ..) -> Some(name.value)
    _ -> None
  }
}

fn top_level_node_bulk_query_error() -> UserError {
  UserError(
    field: Some(["query"]),
    message: "Bulk queries cannot contain a top level `node` field.",
    code: Some("INVALID"),
  )
}

fn too_many_connections_bulk_query_error() -> UserError {
  UserError(
    field: Some(["query"]),
    message: "Bulk queries cannot contain more than 5 connections.",
    code: Some("INVALID"),
  )
}

fn connection_nested_too_deep_bulk_query_error() -> UserError {
  UserError(
    field: Some(["query"]),
    message: "Bulk queries cannot contain connections with a nesting depth greater than 2.",
    code: Some("INVALID"),
  )
}

fn connection_without_node_field_error(names: List(String)) -> UserError {
  let example = case names {
    [name, ..] -> name <> " { edges { node {"
    [] -> "products { edges { node {"
  }
  UserError(
    field: Some(["query"]),
    message: "All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: '"
      <> example
      <> "'. Selecting via 'nodes' is not supported. Invalid connection fields: "
      <> quoted_field_names(names)
      <> ".",
    code: Some("INVALID"),
  )
}

fn nested_connection_without_parent_id_field_error(
  names: List(String),
) -> UserError {
  UserError(
    field: Some(["query"]),
    message: "The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': "
      <> string.join(names, ", ")
      <> ".",
    code: Some("INVALID"),
  )
}

fn quoted_field_names(names: List(String)) -> String {
  names
  |> list.map(fn(name) { "'" <> name <> "'" })
  |> string.join(", ")
}

fn product_export_source(product: ProductRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("vendor", graphql_helpers.option_string_source(product.vendor)),
    #("productType", graphql_helpers.option_string_source(product.product_type)),
    #("tags", SrcList(list.map(product.tags, SrcString))),
    #(
      "totalInventory",
      graphql_helpers.option_int_source(product.total_inventory),
    ),
    #("createdAt", graphql_helpers.option_string_source(product.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(product.updated_at)),
    #("publishedAt", graphql_helpers.option_string_source(product.published_at)),
    #("descriptionHtml", SrcString(product.description_html)),
  ])
}

fn product_variant_export_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  let product_source = case
    store.get_effective_product_by_id(store, variant.product_id)
  {
    Some(product) -> product_export_source(product)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("ProductVariant")),
    #("id", SrcString(variant.id)),
    #("title", SrcString(variant.title)),
    #("sku", graphql_helpers.option_string_source(variant.sku)),
    #("barcode", graphql_helpers.option_string_source(variant.barcode)),
    #("price", graphql_helpers.option_string_source(variant.price)),
    #(
      "compareAtPrice",
      graphql_helpers.option_string_source(variant.compare_at_price),
    ),
    #(
      "inventoryQuantity",
      graphql_helpers.option_int_source(variant.inventory_quantity),
    ),
    #("product", product_source),
  ])
}

fn make_jsonl(rows: List(Json)) -> String {
  case rows {
    [] -> ""
    _ -> string.join(list.map(rows, json.to_string), "\n") <> "\n"
  }
}

fn handle_bulk_operation_run_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let mutation = graphql_helpers.read_arg_string_nonempty(args, "mutation")
  let staged_upload_path =
    graphql_helpers.read_arg_string_nonempty(args, "stagedUploadPath")
  case mutation, staged_upload_path {
    None, _ -> #(
      MutationFieldResult(
        key: key,
        payload: serialize_run_mutation_payload(
          field,
          None,
          [
            UserError(
              field: None,
              message: "Bulk mutation is required.",
              code: Some("INVALID_MUTATION"),
            ),
          ],
          fragments,
        ),
        staged_resource_ids: [],
        log_drafts: [],
      ),
      store,
      identity,
    )
    _, None -> #(
      MutationFieldResult(
        key: key,
        payload: serialize_run_mutation_payload(
          field,
          None,
          [
            UserError(
              field: None,
              message: "Staged upload path is required.",
              code: Some("INVALID_STAGED_UPLOAD_FILE"),
            ),
          ],
          fragments,
        ),
        staged_resource_ids: [],
        log_drafts: [],
      ),
      store,
      identity,
    )
    Some(mutation_string), Some(path) ->
      case validate_inner_bulk_mutation(mutation_string) {
        Error(validation_error) ->
          return_run_mutation_validation_error(
            store,
            identity,
            field,
            fragments,
            key,
            validation_error,
          )
        Ok(_inner_root) ->
          case find_in_progress_bulk_operation(store, "MUTATION") {
            Some(operation) -> #(
              MutationFieldResult(
                key: key,
                payload: serialize_run_mutation_payload(
                  field,
                  None,
                  [
                    operation_in_progress_error(operation, "mutation"),
                  ],
                  fragments,
                ),
                staged_resource_ids: [],
                log_drafts: [],
              ),
              store,
              identity,
            )
            None ->
              case store.get_staged_upload_content(store, path) {
                None -> #(
                  MutationFieldResult(
                    key: key,
                    payload: serialize_run_mutation_payload(
                      field,
                      None,
                      [
                        UserError(
                          field: None,
                          message: "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.",
                          code: Some("NO_SUCH_FILE"),
                        ),
                      ],
                      fragments,
                    ),
                    staged_resource_ids: [],
                    log_drafts: [],
                  ),
                  store,
                  identity,
                )
                Some(content) ->
                  stage_supported_run_mutation(
                    store,
                    identity,
                    request_path,
                    field,
                    fragments,
                    key,
                    mutation_string,
                    content,
                  )
              }
          }
      }
  }
}

fn return_run_mutation_validation_error(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  validation_error: InnerMutationValidationError,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: key,
      payload: serialize_run_mutation_payload(
        field,
        None,
        inner_mutation_validation_user_errors(validation_error),
        fragments,
      ),
      staged_resource_ids: [],
      log_drafts: [],
    ),
    store,
    identity,
  )
}

fn inner_mutation_validation_user_errors(
  validation_error: InnerMutationValidationError,
) -> List(UserError) {
  case validation_error {
    InnerMutationParseError(message) -> [
      UserError(
        field: None,
        message: "Failed to parse the mutation - " <> message,
        code: Some("INVALID_MUTATION"),
      ),
    ]
    InnerMutationInvalidOperationType -> [
      UserError(
        field: None,
        message: "Invalid operation type. Only `mutation` operations are supported.",
        code: Some("INVALID_MUTATION"),
      ),
    ]
    InnerMutationAnalysisErrors(messages) ->
      list.map(messages, fn(message) {
        UserError(field: Some(["mutation"]), message: message, code: None)
      })
  }
}

fn stage_supported_run_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  mutation: String,
  upload_content: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let result =
    process_import_lines(
      string.split(upload_content, "\n"),
      1,
      store,
      identity,
      request_path,
      mutation,
      [],
      [],
      [],
      0,
      False,
    )
  let BulkImportResult(
    store: imported_store,
    identity: imported_identity,
    rows: rows,
    staged_resource_ids: imported_ids,
    log_drafts: log_drafts,
    object_count: object_count,
    failed: failed,
  ) = result
  let result_jsonl = make_jsonl(list.reverse(rows))
  let #(operation, next_store, next_identity) =
    build_and_stage_mutation_operation(
      imported_store,
      imported_identity,
      case failed {
        True -> "FAILED"
        False -> "COMPLETED"
      },
      mutation,
      result_jsonl,
      object_count,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: serialize_run_mutation_payload(
        field,
        Some(created_bulk_operation_response(operation)),
        [],
        fragments,
      ),
      staged_resource_ids: [operation.id, ..imported_ids],
      log_drafts: log_drafts,
    ),
    next_store,
    next_identity,
  )
}

type BulkImportResult {
  BulkImportResult(
    store: Store,
    identity: SyntheticIdentityRegistry,
    rows: List(Json),
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
    object_count: Int,
    failed: Bool,
  )
}

fn process_import_lines(
  lines: List(String),
  line_number: Int,
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  mutation: String,
  rows: List(Json),
  staged_ids: List(String),
  log_drafts: List(LogDraft),
  object_count: Int,
  failed: Bool,
) -> BulkImportResult {
  case lines {
    [] ->
      BulkImportResult(
        store: store,
        identity: identity,
        rows: rows,
        staged_resource_ids: list.reverse(staged_ids),
        log_drafts: list.reverse(log_drafts),
        object_count: object_count,
        failed: failed,
      )
    [line, ..rest] -> {
      let trimmed = string.trim(line)
      case trimmed {
        "" ->
          process_import_lines(
            rest,
            line_number + 1,
            store,
            identity,
            request_path,
            mutation,
            rows,
            staged_ids,
            log_drafts,
            object_count,
            failed,
          )
        _ ->
          case json.parse(trimmed, variables_dict_decoder()) {
            Error(_) ->
              process_import_lines(
                rest,
                line_number + 1,
                store,
                identity,
                request_path,
                mutation,
                [
                  import_error_row(line_number, "Invalid JSONL variables line."),
                  ..rows
                ],
                staged_ids,
                log_drafts,
                object_count,
                True,
              )
            Ok(line_variables) -> {
              let outcome =
                products.process_mutation(
                  store,
                  identity,
                  request_path,
                  mutation,
                  line_variables,
                  empty_upstream_context(),
                )
              let staged_this_line = outcome.staged_resource_ids
              let next_log_drafts = case staged_this_line {
                [] -> log_drafts
                _ -> [
                  bulk_import_log_draft(
                    mutation,
                    line_variables,
                    staged_this_line,
                  ),
                  ..log_drafts
                ]
              }
              let next_object_count = case staged_this_line {
                [] -> object_count
                _ -> object_count + 1
              }
              process_import_lines(
                rest,
                line_number + 1,
                outcome.store,
                outcome.identity,
                request_path,
                mutation,
                [
                  json.object([
                    #("line", json.int(line_number)),
                    #("response", outcome.data),
                  ]),
                  ..rows
                ],
                list.append(outcome.staged_resource_ids, staged_ids),
                next_log_drafts,
                next_object_count,
                failed,
              )
            }
          }
      }
    }
  }
}

fn bulk_import_log_draft(
  mutation: String,
  variables: Dict(String, root_field.ResolvedValue),
  staged_resource_ids: List(String),
) -> LogDraft {
  let parsed = parse_operation.parse_operation(mutation)
  let #(operation_name, root_fields, primary_root_field) = case parsed {
    Ok(parse_operation.ParsedOperation(name: name, root_fields: roots, ..)) -> {
      let primary = case list.first(roots) {
        Ok(root) -> Some(root)
        Error(_) -> None
      }
      #(name, roots, primary)
    }
    Error(_) -> #(None, [], None)
  }
  LogDraft(
    operation_name: operation_name,
    root_fields: root_fields,
    primary_root_field: primary_root_field,
    domain: "products",
    execution: "stage-locally",
    query: Some(mutation),
    variables: Some(variables),
    staged_resource_ids: staged_resource_ids,
    status: store.Staged,
    notes: Some(
      "Staged locally from bulkOperationRunMutation JSONL import; commit replay uses this original inner mutation and line variables.",
    ),
  )
}

fn import_error_row(line_number: Int, message: String) -> Json {
  json.object([
    #("line", json.int(line_number)),
    #(
      "errors",
      json.array([json.object([#("message", json.string(message))])], fn(row) {
        row
      }),
    ),
  ])
}

fn validate_inner_bulk_mutation(
  mutation: String,
) -> Result(String, InnerMutationValidationError) {
  case parser.parse(source.new(mutation)) {
    Error(parser.ParseError(message: message, ..)) ->
      Error(InnerMutationParseError(message))
    Ok(document) ->
      case parse_operation.find_operation(document.definitions) {
        Some(OperationDefinition(operation: Query, ..))
        | Some(OperationDefinition(operation: Subscription, ..)) ->
          Error(InnerMutationInvalidOperationType)
        Some(OperationDefinition(
          operation: Mutation,
          selection_set: selection_set,
          ..,
        )) -> {
          let SelectionSet(selections: selections, ..) = selection_set
          let root_fields = top_level_fields(selections)
          let root_count_errors = case root_fields {
            [single_root] ->
              case root_field_name(single_root) {
                Some(name) ->
                  case products.is_products_mutation_root(name) {
                    True -> []
                    False -> ["You must use an allowed mutation name."]
                  }
                None -> ["You must use an allowed mutation name."]
              }
            _ -> ["You must specify a single top level mutation."]
          }
          let analysis_errors = case root_fields {
            [single_root] ->
              list.append(
                root_count_errors,
                connection_analysis_errors(single_root),
              )
            _ -> root_count_errors
          }
          case analysis_errors {
            [] -> {
              let assert [single_root] = root_fields
              case root_field_name(single_root) {
                Some(name) -> Ok(name)
                None ->
                  Error(
                    InnerMutationAnalysisErrors([
                      "You must use an allowed mutation name.",
                    ]),
                  )
              }
            }
            [_, ..] -> Error(InnerMutationAnalysisErrors(analysis_errors))
          }
        }
        _ -> Error(InnerMutationInvalidOperationType)
      }
  }
}

fn top_level_fields(selections: List(Selection)) -> List(Selection) {
  list.filter(selections, fn(selection) {
    case selection {
      Field(..) -> True
      _ -> False
    }
  })
}

fn connection_analysis_errors(root: Selection) -> List(String) {
  let connection_depths = selected_connection_depths(root, 0)
  let connection_count_error = case list.length(connection_depths) > 1 {
    True -> ["Bulk mutations cannot contain more than 1 connection."]
    False -> []
  }
  let nesting_error = case
    list.any(connection_depths, fn(depth) { depth > 1 })
  {
    True -> [
      "Bulk mutations cannot contain connections with a nesting depth greater than 1.",
    ]
    False -> []
  }
  list.append(connection_count_error, nesting_error)
}

fn selected_connection_depths(
  selection: Selection,
  parent_connection_depth: Int,
) -> List(Int) {
  case selection {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) -> {
      let connection_depth = case is_connection_selection(selection) {
        True -> parent_connection_depth + 1
        False -> parent_connection_depth
      }
      let current = case is_connection_selection(selection) {
        True -> [connection_depth]
        False -> []
      }
      selections
      |> list.map(fn(child) {
        selected_connection_depths(child, connection_depth)
      })
      |> list.flatten
      |> list.append(current)
    }
    InlineFragment(selection_set: SelectionSet(selections: selections, ..), ..) ->
      selections
      |> list.map(fn(child) {
        selected_connection_depths(child, parent_connection_depth)
      })
      |> list.flatten
    Field(..) | FragmentSpread(..) -> []
  }
}

fn is_connection_selection(selection: Selection) -> Bool {
  case selection {
    Field(
      arguments: arguments,
      selection_set: Some(SelectionSet(selections: selections, ..)),
      ..,
    ) ->
      has_connection_window_argument(arguments)
      && list.any(selections, fn(child) {
        case child {
          Field(name: name, ..) ->
            name.value == "edges" || name.value == "nodes"
          _ -> False
        }
      })
    _ -> False
  }
}

fn has_connection_window_argument(arguments: List(Argument)) -> Bool {
  list.any(arguments, fn(argument) {
    argument.name.value == "first" || argument.name.value == "last"
  })
}

fn build_and_stage_mutation_operation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  status: String,
  mutation: String,
  result_jsonl: String,
  object_count: Int,
) -> #(BulkOperationRecord, Store, SyntheticIdentityRegistry) {
  let #(operation_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "BulkOperation")
  let #(created_at, identity_after_created) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let #(completed_at, identity_after_completed) =
    synthetic_identity.make_synthetic_timestamp(identity_after_created)
  let operation =
    BulkOperationRecord(
      id: operation_id,
      status: status,
      type_: "MUTATION",
      error_code: None,
      created_at: created_at,
      completed_at: Some(completed_at),
      object_count: int.to_string(object_count),
      root_object_count: int.to_string(object_count),
      file_size: Some(int.to_string(string.length(result_jsonl))),
      url: Some(build_bulk_operation_result_url(operation_id)),
      partial_data_url: None,
      query: Some(mutation),
      cursor: None,
      result_jsonl: Some(result_jsonl),
    )
  let #(staged, next_store) =
    store.stage_bulk_operation_result(store, operation, result_jsonl)
  #(staged, next_store, identity_after_completed)
}

fn variables_dict_decoder() -> decode.Decoder(
  Dict(String, root_field.ResolvedValue),
) {
  decode.dict(decode.string, root_field.resolved_value_decoder())
}

fn hydrate_bulk_operation_by_id(
  store: Store,
  id: String,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_bulk_operation_by_id(store, id) {
    Some(_) -> store
    None ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "BulkOperationHydrate",
          bulk_operation_hydrate_query(),
          json.object([#("id", json.string(id))]),
        )
      {
        Ok(value) ->
          case bulk_operation_from_hydrate_response(value) {
            Some(operation) ->
              store.upsert_base_bulk_operations(store, [operation])
            None -> store
          }
        Error(_) -> store
      }
  }
}

fn bulk_operation_hydrate_query() -> String {
  "query BulkOperationHydrate($id: ID!) { "
  <> "bulkOperation(id: $id) { "
  <> "id status type errorCode createdAt completedAt objectCount "
  <> "rootObjectCount fileSize url partialDataUrl query "
  <> "} "
  <> "}"
}

fn bulk_operation_from_hydrate_response(
  value: commit.JsonValue,
) -> Option(BulkOperationRecord) {
  use data <- option.then(json_get(value, "data"))
  use operation <- option.then(json_get(data, "bulkOperation"))
  bulk_operation_from_json(operation)
}

fn bulk_operation_from_json(
  value: commit.JsonValue,
) -> Option(BulkOperationRecord) {
  use id <- option.then(json_get_string(value, "id"))
  use status <- option.then(json_get_string(value, "status"))
  use type_ <- option.then(json_get_string(value, "type"))
  use created_at <- option.then(json_get_string(value, "createdAt"))
  use object_count <- option.then(json_get_string(value, "objectCount"))
  use root_object_count <- option.then(json_get_string(value, "rootObjectCount"))
  Some(BulkOperationRecord(
    id: id,
    status: status,
    type_: type_,
    error_code: json_get_optional_string(value, "errorCode"),
    created_at: created_at,
    completed_at: json_get_optional_string(value, "completedAt"),
    object_count: object_count,
    root_object_count: root_object_count,
    file_size: json_get_optional_string(value, "fileSize"),
    url: json_get_optional_string(value, "url"),
    partial_data_url: json_get_optional_string(value, "partialDataUrl"),
    query: json_get_optional_string(value, "query"),
    cursor: None,
    result_jsonl: None,
  ))
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_optional_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    _ -> None
  }
}

fn handle_bulk_operation_cancel(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
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
        log_drafts: [],
      ),
      store,
      identity,
    )
    Some(id) -> {
      // Pattern 2: cancel is still staged locally, but a cold
      // LiveHybrid request first reads the target BulkOperation so
      // terminal errors and cancel overlays use Shopify's prior job.
      let hydrated_store = hydrate_bulk_operation_by_id(store, id, upstream)
      let staged_operation =
        store.get_staged_bulk_operation_by_id(hydrated_store, id)
      let effective_operation =
        store.get_effective_bulk_operation_by_id(hydrated_store, id)
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
            log_drafts: [],
          ),
          hydrated_store,
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
                log_drafts: [],
              ),
              hydrated_store,
              identity,
            )
            False ->
              case staged_operation {
                None -> {
                  let canceled =
                    BulkOperationRecord(
                      ..operation,
                      status: "CANCELING",
                      completed_at: None,
                    )
                  let #(staged, next_store) =
                    store.stage_bulk_operation(hydrated_store, canceled)
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: serialize_cancel_payload(
                        field,
                        Some(staged),
                        [],
                        fragments,
                      ),
                      staged_resource_ids: [staged.id],
                      log_drafts: [],
                    ),
                    next_store,
                    identity,
                  )
                }
                Some(_) -> {
                  let #(canceled, next_store) =
                    store.cancel_staged_bulk_operation(hydrated_store, id)
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
                      log_drafts: [],
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

fn serialize_run_mutation_payload(
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

fn operation_in_progress_error(
  operation: BulkOperationRecord,
  operation_kind: String,
) -> UserError {
  UserError(
    field: None,
    message: "A bulk "
      <> operation_kind
      <> " operation for this app and shop is already in progress: "
      <> operation.id
      <> ".",
    code: Some("OPERATION_IN_PROGRESS"),
  )
}

fn find_in_progress_bulk_operation(
  store: Store,
  operation_type: String,
) -> Option(BulkOperationRecord) {
  store.list_effective_bulk_operations(store)
  |> list.find(fn(operation) {
    operation.type_ == operation_type && !is_terminal_status(operation.status)
  })
  |> option.from_result
}

fn is_terminal_status(status: String) -> Bool {
  case status {
    "CANCELED" | "COMPLETED" | "EXPIRED" | "FAILED" -> True
    _ -> False
  }
}

fn build_bulk_operation_result_url(operation_id: String) -> String {
  "https://shopify-draft-proxy.local/__meta/bulk-operations/"
  <> encode_url_segment(operation_id)
  <> "/result.jsonl"
}

fn encode_url_segment(value: String) -> String {
  value
  |> string.replace("%", "%25")
  |> string.replace(":", "%3A")
  |> string.replace("/", "%2F")
  |> string.replace("?", "%3F")
  |> string.replace("&", "%26")
  |> string.replace("=", "%3D")
}

//// Discount-domain read and mutation staging.
////
//// This module ports the discount owner-node surface with a deliberately
//// flexible normalized record: the store tracks id/type/status/code fields for
//// local lifecycle behavior and keeps a captured/projectable owner-node payload
//// for Shopify-like field projection.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions, SrcBool, SrcFloat,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString, field_locations_json,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, type RequiredArgument, MutationOutcome, RequiredArgument,
  single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountBulkOperationRecord, type DiscountRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord, CapturedArray,
  CapturedBool, CapturedFloat, CapturedInt, CapturedNull, CapturedObject,
  CapturedString, DiscountBulkOperationRecord, DiscountRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}

const discount_function_app_id: String = "347082227713"

pub type DiscountsError {
  ParseFailed(root_field.RootFieldError)
}

type MutationResult {
  MutationResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
  )
}

type RedeemCodeValidation {
  RedeemCodeValidation(code: String, accepted: Bool, errors: List(SourceValue))
}

pub fn is_discount_query_root(name: String) -> Bool {
  case name {
    "discountNodes"
    | "discountNodesCount"
    | "discountNode"
    | "codeDiscountNodes"
    | "codeDiscountNode"
    | "codeDiscountNodeByCode"
    | "discountRedeemCodeBulkCreation"
    | "automaticDiscountNodes"
    | "automaticDiscountNode" -> True
    _ -> False
  }
}

pub fn is_discount_mutation_root(name: String) -> Bool {
  case name {
    "discountCodeBasicCreate"
    | "discountCodeBasicUpdate"
    | "discountCodeBxgyCreate"
    | "discountCodeBxgyUpdate"
    | "discountCodeFreeShippingCreate"
    | "discountCodeFreeShippingUpdate"
    | "discountCodeAppCreate"
    | "discountCodeAppUpdate"
    | "discountCodeActivate"
    | "discountCodeDeactivate"
    | "discountCodeDelete"
    | "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountRedeemCodeBulkAdd"
    | "discountCodeRedeemCodeBulkDelete"
    | "discountRedeemCodeBulkDelete"
    | "discountAutomaticBasicCreate"
    | "discountAutomaticBasicUpdate"
    | "discountAutomaticBxgyCreate"
    | "discountAutomaticBxgyUpdate"
    | "discountAutomaticFreeShippingCreate"
    | "discountAutomaticFreeShippingUpdate"
    | "discountAutomaticAppCreate"
    | "discountAutomaticAppUpdate"
    | "discountAutomaticActivate"
    | "discountAutomaticDeactivate"
    | "discountAutomaticDelete"
    | "discountAutomaticBulkDelete" -> True
    _ -> False
  }
}

/// True iff any string-typed variable value in the request resolves to
/// a discount that's already in local state, or is a proxy-synthetic
/// gid. The dispatcher uses this to skip `LiveHybrid` passthrough so
/// that read-after-create reads of a synthetic id stay local (and so
/// that read-after-delete reads of a synthetic id correctly return
/// null instead of forwarding a synthetic gid upstream where it would
/// 404).
///
/// We scan every string variable value rather than keying on `"id"`
/// because GraphQL operations frequently rebind the argument under a
/// different variable name (e.g. `discountNode(id: $codeId)`).
pub fn local_has_discount_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_discount_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

/// True iff the local store has any staged discount records, or any
/// variable carries a proxy-synthetic gid. The dispatcher uses this to
/// keep aggregate / connection / by-code reads on the local handler
/// once a lifecycle scenario has staged or deleted discounts —
/// passthrough would otherwise forward synthetic gids upstream (404)
/// or skip the empty/null answer the lifecycle test expects after a
/// delete.
pub fn local_has_staged_discounts(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic || !list.is_empty(store.list_effective_discounts(proxy.store))
}

fn local_has_discount_bulk_creation_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || dict.has_key(proxy.store.staged_state.discount_bulk_operations, id)
        || dict.has_key(proxy.store.base_state.discount_bulk_operations, id)
      _ -> False
    }
  })
}

fn get_effective_discount_bulk_operation(
  store: Store,
  id: String,
) -> Option(DiscountBulkOperationRecord) {
  case dict.get(store.staged_state.discount_bulk_operations, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.discount_bulk_operations, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// In `LiveHybrid` mode, decide whether this discount-domain
/// operation should be answered by reaching upstream verbatim instead
/// of from local state. Internal helper for `handle_query_request` —
/// the dispatcher does not consult this directly anymore.
///
/// `*Node` lookups skip passthrough when the requested id is already
/// staged (read-after-write of a local create); aggregate / connection
/// / by-code reads skip passthrough whenever any discount is staged so
/// lifecycle scenarios stay local-only end-to-end.
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "discountNode" ->
      !local_has_discount_id(proxy, variables)
    parse_operation.QueryOperation, "codeDiscountNode" ->
      !local_has_discount_id(proxy, variables)
    parse_operation.QueryOperation, "automaticDiscountNode" ->
      !local_has_discount_id(proxy, variables)
    parse_operation.QueryOperation, "discountNodes" ->
      !local_has_staged_discounts(proxy, variables)
    parse_operation.QueryOperation, "codeDiscountNodes" ->
      !local_has_staged_discounts(proxy, variables)
    parse_operation.QueryOperation, "automaticDiscountNodes" ->
      !local_has_staged_discounts(proxy, variables)
    parse_operation.QueryOperation, "discountNodesCount" ->
      !local_has_staged_discounts(proxy, variables)
    parse_operation.QueryOperation, "codeDiscountNodeByCode" ->
      !local_has_staged_discounts(proxy, variables)
    parse_operation.QueryOperation, "discountRedeemCodeBulkCreation" ->
      !local_has_discount_bulk_creation_id(proxy, variables)
    _, _ -> False
  }
}

/// Domain entrypoint for the discount query path. The dispatcher
/// always lands here for discount-domain reads regardless of
/// `read_mode`; the handler itself decides whether to compute the
/// answer from local state or to forward to upstream verbatim via
/// `passthrough.passthrough_sync` (when in `LiveHybrid` mode and the
/// operation is one we know we can't satisfy locally — see
/// `should_passthrough_in_live_hybrid`).
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string("Failed to handle discounts query"),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, DiscountsError) {
  use data <- result.try(handle_discount_query(store, document, variables))
  Ok(json.object([#("data", data)]))
}

pub fn handle_discount_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, DiscountsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(
        json.object(
          list.map(fields, fn(field) {
            #(
              get_field_response_key(field),
              root_query_payload(store, field, fragments, variables),
            )
          }),
        ),
      )
    }
  }
}

fn root_query_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "discountNode" -> {
          let id = read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            store.get_effective_discount_by_id(store, id)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_node_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "codeDiscountNode" | "automaticDiscountNode" -> {
          let wanted_kind = case name.value {
            "codeDiscountNode" -> "code"
            _ -> "automatic"
          }
          let id = read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            store.get_effective_discount_by_id(store, id)
          })
          |> option.then(fn(record) {
            case record.owner_kind == wanted_kind {
              True -> Some(record)
              False -> None
            }
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "codeDiscountNodeByCode" -> {
          let code = read_string_arg(field, variables, "code")
          code
          |> option.then(fn(code) {
            find_effective_discount_by_code(store, code)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "discountRedeemCodeBulkCreation" -> {
          let id = read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            get_effective_discount_bulk_operation(store, id)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              captured_to_source(record.payload),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "discountNodes" ->
          serialize_discount_connection(
            sort_discounts(
              filter_discounts(
                store.list_effective_discounts(store),
                field,
                variables,
              ),
              field,
              variables,
            ),
            field,
            fragments,
            DiscountNodeConnection,
          )
        "codeDiscountNodes" ->
          serialize_discount_connection(
            sort_discounts(
              filter_discounts(
                store.list_effective_discounts(store),
                field,
                variables,
              ),
              field,
              variables,
            )
              |> list.filter(fn(record) { record.owner_kind == "code" }),
            field,
            fragments,
            OwnerNodeConnection,
          )
        "automaticDiscountNodes" ->
          serialize_discount_connection(
            sort_discounts(
              filter_discounts(
                store.list_effective_discounts(store),
                field,
                variables,
              ),
              field,
              variables,
            )
              |> list.filter(fn(record) { record.owner_kind == "automatic" }),
            field,
            fragments,
            OwnerNodeConnection,
          )
        "discountNodesCount" ->
          serialize_count(
            list.length(filter_discounts(
              store.list_effective_discounts(store),
              field,
              variables,
            )),
            field,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

type ConnectionMode {
  DiscountNodeConnection
  OwnerNodeConnection
}

fn serialize_discount_connection(
  records: List(DiscountRecord),
  field: Selection,
  fragments: FragmentMap,
  mode: ConnectionMode,
) -> Json {
  let first = read_int_arg(field, dict.new(), "first") |> option.unwrap(50)
  let records = list.take(records, int.max(0, first))
  let node_for = fn(record: DiscountRecord) {
    case mode {
      DiscountNodeConnection -> discount_node_source(record)
      OwnerNodeConnection -> discount_owner_source(record)
    }
  }
  let children = child_fields(field)
  json.object(
    list.map(children, fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: child_name, ..) ->
          case child_name.value {
            "nodes" -> #(
              key,
              json.array(records, fn(record) {
                project_graphql_value(
                  node_for(record),
                  child_fields(child),
                  fragments,
                )
              }),
            )
            "edges" -> #(
              key,
              json.array(records, fn(record) {
                let cursor =
                  record.cursor |> option.unwrap("cursor:" <> record.id)
                project_graphql_value(
                  SrcObject(
                    dict.from_list([
                      #("cursor", SrcString(cursor)),
                      #("node", node_for(record)),
                    ]),
                  ),
                  child_fields(child),
                  fragments,
                )
              }),
            )
            "pageInfo" -> #(key, serialize_page_info(records, child))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_page_info(
  records: List(DiscountRecord),
  field: Selection,
) -> Json {
  let start = case records {
    [first, ..] ->
      first.cursor |> option.unwrap("cursor:" <> first.id) |> json.string
    [] -> json.null()
  }
  let end = case list.reverse(records) {
    [last, ..] ->
      last.cursor |> option.unwrap("cursor:" <> last.id) |> json.string
    [] -> json.null()
  }
  json.object(
    list.map(child_fields(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(False))
            "hasPreviousPage" -> #(key, json.bool(False))
            "startCursor" -> #(key, start)
            "endCursor" -> #(key, end)
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_count(count: Int, field: Selection) -> Json {
  json.object(
    list.map(child_fields(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "count" -> #(key, json.int(count))
            "precision" -> #(key, json.string("EXACT"))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn filter_discounts(
  records: List(DiscountRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DiscountRecord) {
  search_query_parser.apply_search_query(
    records,
    read_string_arg(field, variables, "query"),
    search_query_parser.default_parse_options(),
    discount_matches_positive_search_term,
  )
}

fn discount_matches_positive_search_term(
  record: DiscountRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let value = search_query_parser.normalize_search_query_value(term.value)
  case term.field {
    Some("code") -> False
    Some("status") -> string.lowercase(record.status) == value
    Some("type") | Some("discount_type") ->
      discount_matches_type_filter(record, value)
    Some("discount_class") | Some("discountClass") ->
      string.lowercase(discount_class_for_record(record)) == value
    _ -> True
  }
}

fn discount_matches_type_filter(record: DiscountRecord, value: String) -> Bool {
  case value {
    "app" -> record.discount_type == "app"
    "free_shipping" | "free-shipping" -> record.discount_type == "free_shipping"
    "code" -> record.owner_kind == "code"
    "automatic" -> record.owner_kind == "automatic"
    "basic" -> record.discount_type == "basic"
    "bxgy" -> record.discount_type == "bxgy"
    _ -> True
  }
}

fn sort_discounts(
  records: List(DiscountRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DiscountRecord) {
  let reverse =
    read_bool_arg(field, variables, "reverse") |> option.unwrap(False)
  case read_string_arg(field, variables, "sortKey") {
    Some("CREATED_AT") ->
      sort_discounts_by_timestamp(records, "createdAt", reverse)
    Some("UPDATED_AT") ->
      sort_discounts_by_timestamp(records, "updatedAt", reverse)
    _ -> records
  }
}

fn sort_discounts_by_timestamp(
  records: List(DiscountRecord),
  timestamp_field: String,
  reverse: Bool,
) -> List(DiscountRecord) {
  records
  |> list.sort(fn(left, right) {
    let compared = compare_discount_timestamp(left, right, timestamp_field)
    case reverse {
      True -> reverse_order(compared)
      False -> compared
    }
  })
}

fn compare_discount_timestamp(
  left: DiscountRecord,
  right: DiscountRecord,
  timestamp_field: String,
) -> order.Order {
  case
    string.compare(
      discount_record_timestamp(left, timestamp_field) |> option.unwrap(""),
      discount_record_timestamp(right, timestamp_field) |> option.unwrap(""),
    )
  {
    order.Eq -> string.compare(left.id, right.id)
    other -> other
  }
}

fn reverse_order(value: order.Order) -> order.Order {
  case value {
    order.Lt -> order.Gt
    order.Gt -> order.Lt
    order.Eq -> order.Eq
  }
}

fn discount_owner_source(record: DiscountRecord) -> SourceValue {
  captured_to_source(record.payload)
}

fn discount_record_timestamp(
  record: DiscountRecord,
  field: String,
) -> Option(String) {
  case discount_owner_source(record) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, field) {
            Ok(SrcString(value)) -> Some(value)
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

fn discount_node_source(record: DiscountRecord) -> SourceValue {
  case captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      SrcObject(
        fields
        |> dict.delete("automaticDiscount")
        |> dict.delete("codeDiscount")
        |> dict.insert("id", SrcString(record.id))
        |> dict.insert("discount", discount),
      )
    }
    _ ->
      SrcObject(
        dict.from_list([
          #("id", SrcString(record.id)),
          #("discount", SrcNull),
        ]),
      )
  }
}

fn child_fields(field: Selection) -> List(Selection) {
  get_selected_child_fields(
    field,
    SelectedFieldOptions(include_inline_fragments: True),
  )
}

/// Variant of `process_mutation` that threads an `UpstreamContext` into
/// the per-handler logic. Used by the dispatcher when the proxy has an
/// `upstream_transport` installed (parity cassette in tests, live HTTP
/// in production), so that handlers like `discountCodeBasicCreate` can
/// consult upstream for cross-discount uniqueness checks before staging.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        document,
        operation_path,
        upstream,
      )
    }
  }
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
  operation_path: String,
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(entries, all_errors, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errors, current_store, current_identity, staged, drafts) =
        acc
      case field {
        Field(name: name, ..) -> {
          let top_level_errors =
            validate_required_field_arguments(
              field,
              variables,
              name.value,
              required_arguments_for_root(name.value),
              operation_path,
              document,
            )
          case top_level_errors {
            [_, ..] -> #(
              entries,
              list.append(errors, top_level_errors),
              current_store,
              current_identity,
              staged,
              drafts,
            )
            [] -> {
              let result =
                handle_discount_mutation_field(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  document,
                  fragments,
                  variables,
                  upstream,
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, [#(result.key, result.payload)])
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged, result.staged_resource_ids)
                _ -> staged
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  case result.staged_resource_ids {
                    [] -> store.Failed
                    _ -> store.Staged
                  },
                  "discounts",
                  "stage-locally",
                  Some("discount mutation staged locally in Gleam port"),
                )
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = mutation_envelope(entries, all_errors)
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: case all_errors {
      [] -> staged_ids
      _ -> []
    },
    log_drafts: drafts,
  )
}

fn mutation_envelope(
  entries: List(#(String, Json)),
  all_errors: List(Json),
) -> Json {
  case all_errors, entries {
    [], _ -> json.object([#("data", json.object(entries))])
    _, [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
    _, _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
        #("data", json.object(entries)),
      ])
  }
}

fn required_arguments_for_root(root: String) -> List(RequiredArgument) {
  case root {
    "discountCodeBasicCreate" -> [
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBxgyCreate" -> [
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountCodeBxgyUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountAutomaticBasicCreate" -> [
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    "discountAutomaticBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    _ -> []
  }
}

fn handle_discount_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  case root {
    "discountCodeBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "basic",
        "basicCodeDiscount",
        upstream,
      )
    "discountCodeBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "basic",
        "basicCodeDiscount",
      )
    "discountCodeBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "bxgy",
        "bxgyCodeDiscount",
        upstream,
      )
    "discountCodeBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "bxgy",
        "bxgyCodeDiscount",
      )
    "discountCodeFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "free_shipping",
        "freeShippingCodeDiscount",
        upstream,
      )
    "discountCodeFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "free_shipping",
        "freeShippingCodeDiscount",
      )
    "discountCodeAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "app",
        "codeAppDiscount",
        upstream,
      )
    "discountCodeAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "app",
        "codeAppDiscount",
      )
    "discountAutomaticBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "basic",
        "automaticBasicDiscount",
        upstream,
      )
    "discountAutomaticBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "basic",
        "automaticBasicDiscount",
      )
    "discountAutomaticBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "bxgy",
        "automaticBxgyDiscount",
        upstream,
      )
    "discountAutomaticBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "bxgy",
        "automaticBxgyDiscount",
      )
    "discountAutomaticFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "free_shipping",
        "freeShippingAutomaticDiscount",
        upstream,
      )
    "discountAutomaticFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "free_shipping",
        "freeShippingAutomaticDiscount",
      )
    "discountAutomaticAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "app",
        "automaticAppDiscount",
        upstream,
      )
    "discountAutomaticAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "app",
        "automaticAppDiscount",
      )
    "discountCodeActivate" | "discountAutomaticActivate" ->
      set_status(store, identity, root, field, fragments, variables, "ACTIVE")
    "discountCodeDeactivate" | "discountAutomaticDeactivate" ->
      set_status(store, identity, root, field, fragments, variables, "EXPIRED")
    "discountCodeDelete" | "discountAutomaticDelete" ->
      delete_discount(store, identity, root, field, variables)
    "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountAutomaticBulkDelete" ->
      bulk_job_payload(store, identity, root, field, variables, upstream)
    "discountRedeemCodeBulkAdd" ->
      redeem_code_bulk_add(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        upstream,
      )
    "discountCodeRedeemCodeBulkDelete" | "discountRedeemCodeBulkDelete" ->
      redeem_code_bulk_delete(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        upstream,
      )
    _ ->
      MutationResult(
        key: get_field_response_key(field),
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

fn create_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  discount_type: String,
  input_name: String,
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let input = read_object_arg(field, variables, input_name)
  case input {
    None ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          user_error(["input"], "Input is required", "INVALID"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    Some(input) -> {
      let top_level_errors =
        validate_discount_top_level_errors(input, field, document)
      case top_level_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: json.null(),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: top_level_errors,
          )
        [] ->
          create_discount_after_top_level_validation(
            store,
            identity,
            root,
            field,
            fragments,
            owner_kind,
            discount_type,
            input_name,
            upstream,
            input,
            key,
          )
      }
    }
  }
}

fn create_discount_after_top_level_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  discount_type: String,
  input_name: String,
  upstream: UpstreamContext,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  // Local input validation first (structural / pure-function checks).
  let store =
    maybe_hydrate_discount_subscription_capability(
      store,
      input_name,
      input,
      discount_type,
      upstream,
    )
  let store = case discount_type {
    "app" -> maybe_hydrate_shopify_function(store, input, upstream)
    _ -> store
  }
  let user_errors =
    validate_discount_input(
      store,
      input_name,
      input,
      discount_type,
      owner_kind == "code",
      None,
    )
  // Cross-discount uniqueness check: when local validation otherwise
  // passes and the input carries a `code`, ask upstream whether a
  // discount with that code already exists. If so, surface a TAKEN
  // error matching Shopify's response shape. We do this after local
  // validation so that pure-input errors (badRefs, BXGY shape, free-
  // shipping combinesWith) are not overshadowed by an upstream
  // call that would never have been issued in production for those
  // shapes either. In `Snapshot` mode (no transport, no upstream),
  // the lookup is skipped — the local-store check inside
  // `validate_discount_input` already rejects duplicates against
  // staged records, which is the cold-start expectation.
  let user_errors = case user_errors {
    [_, ..] -> user_errors
    [] ->
      case fetch_taken_code_error(input, input_name, owner_kind, upstream) {
        Some(err) -> [err]
        None -> []
      }
  }
  case user_errors {
    [_, ..] ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, user_errors),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    [] -> {
      // Pattern 2: when this is an app discount, hydrate the
      // referenced Shopify Function from upstream so the staged
      // record can project the function's metadata onto
      // `appDiscountType` (appKey, title, description). No-op when
      // the function is already in the local store, when no
      // transport is installed (Snapshot mode), or when the
      // upstream call fails. The miss falls through to the
      // legacy local-only behavior.
      let store = case discount_type {
        "app" -> maybe_hydrate_shopify_function(store, input, upstream)
        _ -> store
      }
      let #(id, next_identity) =
        synthetic_identity.make_proxy_synthetic_gid(identity, case owner_kind {
          "automatic" -> "DiscountAutomaticNode"
          _ -> "DiscountCodeNode"
        })
      let #(record, next_identity) =
        build_discount_record(
          store,
          next_identity,
          id,
          owner_kind,
          discount_type,
          input,
          None,
        )
      let #(record, next_store) = store.stage_discount(store, record)
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, Some(record), []),
        store: next_store,
        identity: next_identity,
        staged_resource_ids: [record.id],
        top_level_errors: [],
      )
    }
  }
}

fn update_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  let id = read_string_arg(field, variables, "id")
  let input = read_object_arg(field, variables, input_name)
  case id, input {
    Some(id), Some(input) -> {
      let top_level_errors =
        validate_discount_top_level_errors(input, field, document)
      case top_level_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: json.null(),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: top_level_errors,
          )
        [] ->
          update_discount_after_top_level_validation(
            store,
            identity,
            root,
            field,
            fragments,
            owner_kind,
            target_discount_type,
            input_name,
            id,
            input,
            key,
          )
      }
    }
    _, _ ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          user_error(["id"], "Discount does not exist", "NOT_FOUND"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

fn update_discount_after_top_level_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  let early_user_errors =
    validate_context_customer_selection_conflict(input_name, input)
  case early_user_errors {
    [_, ..] ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, early_user_errors),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    [] ->
      update_discount_existing_record(
        store,
        identity,
        root,
        field,
        fragments,
        owner_kind,
        target_discount_type,
        input_name,
        id,
        input,
        key,
      )
  }
}

fn update_discount_existing_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  let existing = store.get_effective_discount_by_id(store, id)
  case existing {
    None ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          user_error(["id"], "Discount does not exist", "INVALID"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    Some(existing_record) -> {
      let user_errors = validate_discount_update_input(input, existing_record)
      let user_errors = case user_errors {
        [_, ..] -> user_errors
        [] ->
          validate_discount_input(
            store,
            input_name,
            input,
            target_discount_type,
            False,
            Some(existing_record.id),
          )
      }
      case user_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, None, user_errors),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: [],
          )
        [] -> {
          let #(record, next_identity) =
            build_discount_record(
              store,
              identity,
              id,
              owner_kind,
              target_discount_type,
              input,
              Some(existing_record),
            )
          let #(record, next_store) = store.stage_discount(store, record)
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, Some(record), []),
            store: next_store,
            identity: next_identity,
            staged_resource_ids: [record.id],
            top_level_errors: [],
          )
        }
      }
    }
  }
}

fn set_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  status: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  case read_string_arg(field, variables, "id") {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        Some(record) -> {
          let user_errors = case status {
            "ACTIVE" -> app_discount_activation_errors(store, record)
            _ -> []
          }
          case user_errors {
            [_, ..] ->
              MutationResult(
                key,
                payload_json(root, field, fragments, None, user_errors),
                store,
                identity,
                [],
                [],
              )
            [] -> {
              let #(updated_at, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let transition_timestamp = case status {
                "ACTIVE" ->
                  case record.status {
                    "ACTIVE" -> None
                    _ -> Some(updated_at)
                  }
                "EXPIRED" -> Some(updated_at)
                _ -> None
              }
              let record =
                DiscountRecord(
                  ..record,
                  status: status,
                  payload: update_payload_status(
                      record.payload,
                      status,
                      transition_timestamp,
                    )
                    |> update_payload_updated_at(updated_at),
                )
              let #(record, next_store) = store.stage_discount(store, record)
              MutationResult(
                key,
                payload_json(root, field, fragments, Some(record), []),
                next_store,
                next_identity,
                [record.id],
                [],
              )
            }
          }
        }
        None ->
          MutationResult(
            key,
            payload_json(root, field, fragments, None, [
              user_error(["id"], "Discount does not exist", "INVALID"),
            ]),
            store,
            identity,
            [],
            [],
          )
      }
    None ->
      MutationResult(
        key,
        payload_json(root, field, fragments, None, [
          user_error(["id"], "ID is required", "INVALID"),
        ]),
        store,
        identity,
        [],
        [],
      )
  }
}

fn app_discount_activation_errors(
  store: Store,
  record: DiscountRecord,
) -> List(SourceValue) {
  case record.discount_type {
    "app" ->
      case discount_app_function_reference(record) {
        Some(reference) ->
          case find_shopify_function(store, reference) {
            Some(_) -> []
            None -> activation_failed_user_errors()
          }
        None -> activation_failed_user_errors()
      }
    _ -> []
  }
}

fn activation_failed_user_errors() -> List(SourceValue) {
  [
    user_error(["id"], "Discount could not be activated.", "INTERNAL_ERROR"),
  ]
}

fn discount_app_function_reference(record: DiscountRecord) -> Option(String) {
  case captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let owner = case record.owner_kind {
        "automatic" -> dict.get(fields, "automaticDiscount")
        _ -> dict.get(fields, "codeDiscount")
      }
      case owner {
        Ok(SrcObject(discount)) ->
          case dict.get(discount, "appDiscountType") {
            Ok(SrcObject(app_discount_type)) ->
              case dict.get(app_discount_type, "functionId") {
                Ok(SrcString(reference)) -> Some(reference)
                _ -> None
              }
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

fn delete_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationResult {
  let key = get_field_response_key(field)
  let id = read_string_arg(field, variables, "id") |> option.unwrap("")
  let #(next_store, next_identity) = case
    store.get_effective_discount_by_id(store, id)
  {
    Some(_) -> {
      let #(_, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      #(store.delete_staged_discount(store, id), next_identity)
    }
    None -> #(store.delete_staged_discount(store, id), identity)
  }
  let payload =
    json.object(
      list.map(child_fields(field), fn(child) {
        let child_key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "deletedCodeDiscountId" | "deletedAutomaticDiscountId" -> #(
                child_key,
                json.string(id),
              )
              "userErrors" -> #(child_key, json.array([], fn(x) { x }))
              _ -> #(child_key, json.null())
            }
          _ -> #(child_key, json.null())
        }
      }),
    )
  MutationResult(key, payload, next_store, next_identity, [id], [])
}

fn bulk_job_payload(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let args =
    root_field.get_field_arguments(field, variables)
    |> result.unwrap(dict.new())
  let user_errors = validate_bulk_selector(store, root, args)
  case user_errors {
    [_, ..] -> {
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", SrcNull),
              #("userErrors", SrcList(user_errors)),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(key, payload, store, identity, [], [])
    }
    [] -> {
      // Pattern 2: hydrate every id this bulk operation touches before
      // applying the local effects. Without hydration, references to
      // base discounts only seeded upstream silently no-op (set-status
      // checks `get_effective_discount_by_id` first), so subsequent
      // count and node-by-id read targets see incorrect totals. A
      // cassette miss is a silent no-op so the legacy local-only
      // behavior applies in Snapshot mode.
      let ids = read_string_array(args, "ids", [])
      let #(store, identity_after_hydrate) =
        list.fold(ids, #(store, identity), fn(acc, id) {
          let #(current_store, current_identity) = acc
          maybe_hydrate_discount(current_store, current_identity, id, upstream)
        })
      let #(job_id, next_identity) =
        make_discount_async_gid(store, identity_after_hydrate, "Job")
      let job =
        SrcObject(
          dict.from_list([
            #("id", SrcString(job_id)),
            #("done", SrcBool(True)),
            #("query", SrcNull),
          ]),
        )
      let #(next_store, identity_after_effects) =
        apply_bulk_effects(store, root, args, next_identity)
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", job),
              #("userErrors", SrcList([])),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(
        key,
        payload,
        next_store,
        identity_after_effects,
        [job_id],
        [],
      )
    }
  }
}

fn redeem_code_bulk_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let discount_id = read_string_arg(field, variables, "discountId")
  let #(codes, schema_input_codes) =
    read_codes_arg_with_shape(field, variables, "codes")
  let too_many_errors =
    validate_redeem_code_bulk_add_size(field, document, codes)
  case too_many_errors {
    [_, ..] ->
      MutationResult(key, json.null(), store, identity, [], too_many_errors)
    [] ->
      redeem_code_bulk_add_after_size_validation(
        store,
        identity,
        root,
        field,
        fragments,
        discount_id,
        codes,
        schema_input_codes,
        upstream,
        key,
      )
  }
}

fn redeem_code_bulk_add_after_size_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  discount_id: Option(String),
  codes: List(String),
  schema_input_codes: Bool,
  upstream: UpstreamContext,
  key: String,
) -> MutationResult {
  let #(store, identity) = case discount_id {
    Some(id) -> maybe_hydrate_discount(store, identity, id, upstream)
    None -> #(store, identity)
  }
  case discount_id {
    None ->
      redeem_code_bulk_add_user_error(
        store,
        identity,
        root,
        field,
        fragments,
        key,
        user_error(["discountId"], "Code discount does not exist.", "INVALID"),
      )
    Some(id) ->
      case store.get_effective_discount_by_id(store, id), codes {
        None, _ ->
          redeem_code_bulk_add_user_error(
            store,
            identity,
            root,
            field,
            fragments,
            key,
            user_error(
              ["discountId"],
              "Code discount does not exist.",
              "INVALID",
            ),
          )
        Some(_), [] ->
          redeem_code_bulk_add_user_error(
            store,
            identity,
            root,
            field,
            fragments,
            key,
            user_error(["codes"], "Codes can't be blank", "BLANK"),
          )
        Some(record), [_, ..] -> {
          let #(bulk_id, identity) =
            make_discount_async_gid(
              store,
              identity,
              "DiscountRedeemCodeBulkCreation",
            )
          let validations = validate_redeem_codes(codes)
          let accepted_codes =
            validations
            |> list.filter(fn(item) { item.accepted })
            |> list.map(fn(item) { item.code })
          let #(updated, identity, created_nodes) =
            append_codes(store, record, accepted_codes, identity)
          let #(next_store, identity) = case accepted_codes {
            [] -> #(store, identity)
            [_, ..] -> {
              let #(updated_at, identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let updated = bump_discount_updated_at(updated, updated_at)
              let #(_, next_store) = store.stage_discount(store, updated)
              #(next_store, identity)
            }
          }
          let final_bulk_creation =
            redeem_code_bulk_creation_source(
              bulk_id,
              validations,
              created_nodes,
              False,
            )
          let #(_, next_store) =
            store.stage_discount_bulk_operation(
              next_store,
              DiscountBulkOperationRecord(
                id: bulk_id,
                operation: "discountRedeemCodeBulkAdd",
                discount_id: id,
                status: "COMPLETED",
                payload: source_to_captured(final_bulk_creation),
              ),
            )
          let mutation_bulk_creation =
            redeem_code_bulk_creation_source(
              bulk_id,
              validations,
              created_nodes,
              schema_input_codes,
            )
          let payload =
            project_graphql_value(
              SrcObject(
                dict.from_list([
                  #("bulkCreation", mutation_bulk_creation),
                  #("userErrors", SrcList([])),
                ]),
              ),
              child_fields(field),
              fragments,
            )
          MutationResult(key, payload, next_store, identity, [id, bulk_id], [])
        }
      }
  }
}

fn redeem_code_bulk_add_user_error(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  error: SourceValue,
) -> MutationResult {
  let payload =
    project_graphql_value(
      SrcObject(
        dict.from_list([
          #("bulkCreation", SrcNull),
          #("userErrors", SrcList([error])),
        ]),
      ),
      child_fields(field),
      fragments,
    )
  MutationResult(key, payload, store, identity, [], [])
}

fn validate_redeem_code_bulk_add_size(
  field: Selection,
  document: String,
  codes: List(String),
) -> List(Json) {
  let count = list.length(codes)
  case count > 250 {
    False -> []
    True -> [
      json.object([
        #(
          "message",
          json.string(
            "The input array size of "
            <> int.to_string(count)
            <> " is greater than the maximum allowed of 250.",
          ),
        ),
        #("locations", field_locations_json(field, document)),
        #(
          "path",
          json.array(["discountRedeemCodeBulkAdd", "codes"], json.string),
        ),
        #(
          "extensions",
          json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
        ),
      ]),
    ]
  }
}

fn validate_redeem_codes(codes: List(String)) -> List(RedeemCodeValidation) {
  let #(items, _) =
    list.fold(codes, #([], []), fn(acc, code) {
      let #(items, seen) = acc
      let pure_errors = redeem_code_value_errors(code)
      case pure_errors {
        [_, ..] -> #(
          [RedeemCodeValidation(code, False, pure_errors), ..items],
          seen,
        )
        [] ->
          case list.contains(seen, code) {
            True -> #(
              [
                RedeemCodeValidation(code, False, [
                  user_error_with_code(
                    ["code"],
                    "Codes must be unique within BulkDiscountCodeCreation",
                    None,
                  ),
                ]),
                ..items
              ],
              seen,
            )
            False -> #([RedeemCodeValidation(code, True, []), ..items], [
              code,
              ..seen
            ])
          }
      }
    })
  list.reverse(items)
}

fn redeem_code_value_errors(code: String) -> List(SourceValue) {
  case code == "" {
    True -> [
      user_error_with_code(
        ["code"],
        "is too short (minimum is 1 character)",
        None,
      ),
    ]
    False ->
      case string.contains(code, "\n") || string.contains(code, "\r") {
        True -> [
          user_error_with_code(
            ["code"],
            "cannot contain newline characters.",
            None,
          ),
        ]
        False ->
          case string.length(code) > 255 {
            True -> [
              user_error_with_code(
                ["code"],
                "is too long (maximum is 255 characters)",
                None,
              ),
            ]
            False -> []
          }
      }
  }
}

fn redeem_code_bulk_creation_source(
  id: String,
  validations: List(RedeemCodeValidation),
  created_nodes: List(#(String, String)),
  pending: Bool,
) -> SourceValue {
  let failed_count =
    validations
    |> list.filter(fn(item) { !item.accepted })
    |> list.length
  let imported_count = list.length(validations) - failed_count
  SrcObject(
    dict.from_list([
      #("id", SrcString(id)),
      #("done", SrcBool(!pending)),
      #("codesCount", SrcInt(list.length(validations))),
      #(
        "importedCount",
        SrcInt(case pending {
          True -> 0
          False -> imported_count
        }),
      ),
      #(
        "failedCount",
        SrcInt(case pending {
          True -> 0
          False -> failed_count
        }),
      ),
      #(
        "codes",
        SrcObject(
          dict.from_list([
            #(
              "nodes",
              SrcList(
                list.map(validations, fn(item) {
                  redeem_code_bulk_creation_code_source(
                    item,
                    created_nodes,
                    pending,
                  )
                }),
              ),
            ),
            #("edges", SrcList([])),
            #(
              "pageInfo",
              SrcObject(
                dict.from_list([
                  #("hasNextPage", SrcBool(False)),
                  #("hasPreviousPage", SrcBool(False)),
                  #("startCursor", SrcNull),
                  #("endCursor", SrcNull),
                ]),
              ),
            ),
          ]),
        ),
      ),
    ]),
  )
}

fn redeem_code_bulk_creation_code_source(
  validation: RedeemCodeValidation,
  created_nodes: List(#(String, String)),
  pending: Bool,
) -> SourceValue {
  let redeem_code = case pending, validation.accepted {
    True, _ -> SrcNull
    False, True ->
      case find_created_redeem_code_id(created_nodes, validation.code) {
        Some(id) ->
          SrcObject(
            dict.from_list([
              #("id", SrcString(id)),
              #("code", SrcString(validation.code)),
            ]),
          )
        None -> SrcNull
      }
    False, False -> SrcNull
  }
  SrcObject(
    dict.from_list([
      #("code", SrcString(validation.code)),
      #(
        "errors",
        SrcList(case pending {
          True -> []
          False -> validation.errors
        }),
      ),
      #("discountRedeemCode", redeem_code),
    ]),
  )
}

fn find_created_redeem_code_id(
  nodes: List(#(String, String)),
  code: String,
) -> Option(String) {
  case
    nodes
    |> list.find(fn(pair) {
      let #(_, node_code) = pair
      node_code == code
    })
  {
    Ok(pair) -> {
      let #(id, _) = pair
      Some(id)
    }
    Error(_) -> None
  }
}

fn redeem_code_bulk_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let args =
    root_field.get_field_arguments(field, variables)
    |> result.unwrap(dict.new())
  let discount_id = read_string(args, "discountId")
  let selector_errors = validate_redeem_code_bulk_delete_selector_shape(args)
  case selector_errors {
    [_, ..] ->
      MutationResult(
        key,
        redeem_code_bulk_delete_payload(
          field,
          fragments,
          SrcNull,
          selector_errors,
        ),
        store,
        identity,
        [],
        [],
      )
    [] -> {
      // Same Pattern 2 hydration as redeem_code_bulk_add: pull the prior
      // record from upstream before validating discount existence so real
      // Shopify-side discounts can be targeted by local staged deletions.
      let #(store, identity) = case discount_id {
        Some(id) -> maybe_hydrate_discount(store, identity, id, upstream)
        None -> #(store, identity)
      }
      let user_errors =
        validate_redeem_code_bulk_delete_after_hydrate(store, args)
      case user_errors {
        [_, ..] ->
          MutationResult(
            key,
            redeem_code_bulk_delete_payload(
              field,
              fragments,
              SrcNull,
              user_errors,
            ),
            store,
            identity,
            [],
            [],
          )
        [] -> {
          let #(next_store, identity_after_update) = case discount_id {
            Some(id) ->
              case store.get_effective_discount_by_id(store, id) {
                Some(record) -> {
                  let ids =
                    redeem_code_bulk_delete_target_ids(store, record, args)
                  let #(updated_at, identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let updated = remove_codes_by_ids(record, ids, updated_at)
                  let #(_, s) = store.stage_discount(store, updated)
                  #(s, identity)
                }
                None -> #(store, identity)
              }
            None -> #(store, identity)
          }
          let #(job_id, next_identity) =
            make_discount_async_gid(store, identity_after_update, "Job")
          let job =
            SrcObject(
              dict.from_list([
                #("id", SrcString(job_id)),
                #("done", SrcBool(True)),
                #("query", SrcNull),
              ]),
            )
          MutationResult(
            key,
            redeem_code_bulk_delete_payload(field, fragments, job, []),
            next_store,
            next_identity,
            option_to_list(discount_id),
            [],
          )
        }
      }
    }
  }
}

fn redeem_code_bulk_delete_payload(
  field: Selection,
  fragments: FragmentMap,
  job: SourceValue,
  user_errors: List(SourceValue),
) -> Json {
  project_graphql_value(
    SrcObject(
      dict.from_list([
        #("job", job),
        #("userErrors", SrcList(user_errors)),
      ]),
    ),
    child_fields(field),
    fragments,
  )
}

fn payload_json(
  root: String,
  field: Selection,
  fragments: FragmentMap,
  record: Option(DiscountRecord),
  user_errors: List(SourceValue),
) -> Json {
  let owner_field = owner_node_field(root)
  let owner_payload = case record {
    Some(record) -> discount_owner_source(record)
    None -> SrcNull
  }
  let discount_payload = case record {
    Some(record) ->
      case discount_owner_source(record) {
        SrcObject(fields) ->
          case record.owner_kind {
            "automatic" ->
              dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
            _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
          }
        _ -> SrcNull
      }
    None -> SrcNull
  }
  project_graphql_value(
    SrcObject(
      dict.from_list([
        #(owner_field, owner_payload),
        #("codeDiscountNode", owner_payload),
        #("automaticDiscountNode", owner_payload),
        #("codeAppDiscount", discount_payload),
        #("automaticAppDiscount", discount_payload),
        #("userErrors", SrcList(user_errors)),
      ]),
    ),
    child_fields(field),
    fragments,
  )
}

fn owner_node_field(root: String) -> String {
  case string.starts_with(root, "discountAutomatic") {
    True -> "automaticDiscountNode"
    False -> "codeDiscountNode"
  }
}

fn build_discount_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  owner_kind: String,
  discount_type: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
) -> #(DiscountRecord, SyntheticIdentityRegistry) {
  let title =
    read_string(input, "title")
    |> option.or(existing |> option.then(fn(r) { r.title }))
    |> option.unwrap("")
  let code =
    read_string(input, "code")
    |> option.or(read_string(input, "codePrefix"))
    |> option.or(existing |> option.then(fn(r) { r.code }))
  let owner_field = case owner_kind {
    "automatic" -> "automaticDiscount"
    _ -> "codeDiscount"
  }
  let starts_at =
    input_or_existing_discount_source(input, existing, owner_field, "startsAt")
  let ends_at =
    input_or_existing_discount_source(input, existing, owner_field, "endsAt")
  let status =
    derive_discount_status(starts_at, ends_at, synthetic_now(identity))
  let typename = typename_for(owner_kind, discount_type)
  let #(code_source, next_identity) =
    code_connection_for_record(identity, code, existing)
  let #(mutation_timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(next_identity)
  let created_at =
    existing
    |> option.then(fn(record) { discount_record_timestamp(record, "createdAt") })
    |> option.unwrap(mutation_timestamp)
  let discount_classes = discount_classes_for_input(input, discount_type)
  let discount_class = primary_discount_class(discount_classes)
  let discount =
    SrcObject(
      dict.from_list([
        #("__typename", SrcString(typename)),
        #("discountId", SrcString(id)),
        #("title", SrcString(title)),
        #("status", SrcString(status)),
        #("summary", SrcString(summary_for(input, discount_type))),
        #("startsAt", starts_at),
        #("endsAt", ends_at),
        #("createdAt", SrcString(created_at)),
        #("updatedAt", SrcString(mutation_timestamp)),
        #("asyncUsageCount", SrcInt(0)),
        #("discountClasses", string_list_source(discount_classes)),
        #("discountClass", SrcString(discount_class)),
        #(
          "combinesWith",
          object_value_or_default(input, "combinesWith", combines_default()),
        ),
        #("codes", code_source),
        #(
          "codesCount",
          count_source(case code {
            Some(_) -> 1
            None -> 0
          }),
        ),
        #("context", context_source(read_value(input, "context"))),
        #(
          "customerGets",
          customer_gets_source(read_value(input, "customerGets")),
        ),
        #(
          "customerBuys",
          customer_buys_source(read_value(input, "customerBuys")),
        ),
        #(
          "minimumRequirement",
          minimum_source(read_value(input, "minimumRequirement")),
        ),
        #(
          "destinationSelection",
          destination_source(read_value(input, "destination")),
        ),
        #(
          "maximumShippingPrice",
          money_source(read_value(input, "maximumShippingPrice")),
        ),
        #(
          "appliesOncePerCustomer",
          bool_source(read_value(input, "appliesOncePerCustomer"), False),
        ),
        #(
          "appliesOnOneTimePurchase",
          bool_source(read_value(input, "appliesOnOneTimePurchase"), True),
        ),
        #(
          "appliesOnSubscription",
          bool_source(read_value(input, "appliesOnSubscription"), False),
        ),
        #(
          "recurringCycleLimit",
          resolved_to_source(read_value(input, "recurringCycleLimit")),
        ),
        #("usageLimit", resolved_to_source(read_value(input, "usageLimit"))),
        #(
          "usesPerOrderLimit",
          resolved_to_source(read_value(input, "usesPerOrderLimit")),
        ),
        #("appDiscountType", app_discount_type_source(store, input)),
      ]),
    )
  #(
    DiscountRecord(
      id: id,
      owner_kind: owner_kind,
      discount_type: discount_type,
      title: Some(title),
      status: status,
      code: code,
      payload: source_to_captured(
        SrcObject(
          dict.from_list([
            #("id", SrcString(id)),
            #(owner_field, discount),
          ]),
        ),
      ),
      cursor: None,
    ),
    next_identity,
  )
}

fn input_or_existing_discount_source(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
  owner_field: String,
  name: String,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(value) -> resolved_to_source(value)
    Error(_) ->
      existing_discount_source(existing, owner_field, name)
      |> option.unwrap(SrcNull)
  }
}

fn existing_discount_source(
  existing: Option(DiscountRecord),
  owner_field: String,
  name: String,
) -> Option(SourceValue) {
  existing
  |> option.then(fn(record) {
    case captured_to_source(record.payload) {
      SrcObject(node) ->
        case dict.get(node, owner_field) {
          Ok(SrcObject(discount)) ->
            dict.get(discount, name) |> option.from_result
          _ -> None
        }
      _ -> None
    }
  })
}

fn synthetic_now(identity: SyntheticIdentityRegistry) -> String {
  iso_timestamp.format_iso(identity.next_synthetic_time)
}

fn derive_discount_status(
  starts_at: SourceValue,
  ends_at: SourceValue,
  now: String,
) -> String {
  case iso_timestamp.parse_iso(now) {
    Ok(now_ms) ->
      derive_discount_status_ms(
        source_timestamp_ms(starts_at),
        source_timestamp_ms(ends_at),
        now_ms,
      )
    Error(_) -> "ACTIVE"
  }
}

fn derive_discount_status_ms(
  starts_at: Option(Int),
  ends_at: Option(Int),
  now_ms: Int,
) -> String {
  case starts_at, ends_at {
    Some(starts_ms), Some(ends_ms)
      if starts_ms > now_ms && ends_ms >= starts_ms
    -> "SCHEDULED"
    Some(starts_ms), None if starts_ms > now_ms -> "SCHEDULED"
    Some(starts_ms), Some(ends_ms)
      if ends_ms <= now_ms && starts_ms <= ends_ms
    -> "EXPIRED"
    None, Some(ends_ms) if ends_ms <= now_ms -> "EXPIRED"
    Some(starts_ms), Some(ends_ms) if starts_ms <= now_ms && ends_ms > now_ms ->
      "ACTIVE"
    Some(starts_ms), None if starts_ms <= now_ms -> "ACTIVE"
    None, Some(ends_ms) if ends_ms > now_ms -> "ACTIVE"
    None, None -> "ACTIVE"
    _, _ -> "ACTIVE"
  }
}

fn source_timestamp_ms(value: SourceValue) -> Option(Int) {
  case value {
    SrcString(timestamp) ->
      iso_timestamp.parse_iso(timestamp) |> option.from_result
    _ -> None
  }
}

fn typename_for(owner_kind: String, discount_type: String) -> String {
  case owner_kind, discount_type {
    "automatic", "basic" -> "DiscountAutomaticBasic"
    "automatic", "bxgy" -> "DiscountAutomaticBxgy"
    "automatic", "free_shipping" -> "DiscountAutomaticFreeShipping"
    "automatic", "app" -> "DiscountAutomaticApp"
    "code", "bxgy" -> "DiscountCodeBxgy"
    "code", "free_shipping" -> "DiscountCodeFreeShipping"
    "code", "app" -> "DiscountCodeApp"
    _, _ -> "DiscountCodeBasic"
  }
}

fn default_discount_classes(discount_type: String) -> List(String) {
  case discount_type {
    "free_shipping" -> ["SHIPPING"]
    "bxgy" -> ["PRODUCT"]
    _ -> ["ORDER"]
  }
}

fn discount_classes_for_input(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(String) {
  case discount_type {
    "free_shipping" -> default_discount_classes(discount_type)
    _ ->
      case read_string(input, "discountClass") {
        Some(discount_class) -> [discount_class]
        None ->
          case read_string_array(input, "discountClasses", []) {
            [_, ..] as classes -> classes
            [] ->
              case discount_type {
                "basic" -> infer_basic_discount_classes(input)
                _ -> default_discount_classes(discount_type)
              }
          }
      }
  }
}

fn infer_basic_discount_classes(
  input: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case customer_gets_items_fields(input) {
    Some(items) ->
      case items_targets_entitled_resources(items) {
        True -> ["PRODUCT"]
        False -> ["ORDER"]
      }
    None -> ["ORDER"]
  }
}

fn items_targets_entitled_resources(
  items: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.has_key(items, "products")
  || dict.has_key(items, "productVariants")
  || dict.has_key(items, "collections")
}

fn primary_discount_class(classes: List(String)) -> String {
  case classes {
    [first, ..] -> first
    [] -> "ORDER"
  }
}

fn discount_class_for_record(record: DiscountRecord) -> String {
  case captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, "discountClass") {
            Ok(SrcString(class)) -> class
            _ ->
              case dict.get(discount_fields, "discountClasses") {
                Ok(SrcList([SrcString(class), ..])) -> class
                _ ->
                  default_discount_classes(record.discount_type)
                  |> primary_discount_class
              }
          }
        _ ->
          default_discount_classes(record.discount_type)
          |> primary_discount_class
      }
    }
    _ ->
      default_discount_classes(record.discount_type) |> primary_discount_class
  }
}

fn summary_for(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> String {
  case discount_type {
    "free_shipping" -> "Free shipping"
    "bxgy" -> bxgy_summary(input)
    _ ->
      case read_string(input, "title") {
        Some(title) -> title
        None -> ""
      }
  }
}

fn validate_discount_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  require_code: Bool,
  ignored_discount_id: Option(String),
) -> List(SourceValue) {
  let errors =
    list.append(
      validate_discount_code_input(input_name, input, require_code),
      validate_context_customer_selection_conflict(input_name, input),
    )
  let errors = case read_string(input, "code") {
    Some(code) ->
      case errors {
        [_, ..] -> errors
        [] ->
          case
            find_effective_discount_by_code_ignoring(
              store,
              code,
              ignored_discount_id,
            )
          {
            Some(_) ->
              list.append(errors, [
                user_error(
                  [input_name, "code"],
                  "Code must be unique. Please try a different code.",
                  "TAKEN",
                ),
              ])
            None -> errors
          }
      }
    None -> errors
  }
  let errors = case discount_type {
    "bxgy" -> list.append(errors, validate_bxgy_input(input_name, input))
    _ -> errors
  }
  let errors =
    list.append(
      errors,
      validate_subscription_fields(store, input_name, input, discount_type),
    )
  let errors =
    list.append(
      errors,
      validate_cart_line_combination_tag_settings(
        input_name,
        input,
        discount_classes_for_input(input, discount_type),
      ),
    )
  let errors =
    list.append(errors, validate_minimum_requirement(input_name, input))
  let errors = case discount_type {
    "free_shipping" -> {
      case invalid_free_shipping_combines(input) {
        True ->
          list.append(errors, [
            user_error(
              [input_name, "combinesWith"],
              "The combinesWith settings are not valid for the discount class.",
              "INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS",
            ),
          ])
        False -> errors
      }
      |> append_blank_title_error(input_name, input)
    }
    _ -> errors
  }
  let errors = case invalid_date_range(input) {
    True ->
      list.append(errors, [
        user_error(
          [input_name, "endsAt"],
          "Ends at needs to be after starts_at",
          "INVALID",
        ),
      ])
    False -> errors
  }
  let errors = case input_name {
    "basicCodeDiscount" ->
      list.append(errors, validate_basic_refs(input_name, input))
    _ -> errors
  }
  case errors {
    [_, ..] -> errors
    [] ->
      list.append(
        errors,
        validate_app_discount_function_input(
          store,
          input_name,
          input,
          discount_type,
        ),
      )
  }
}

fn validate_app_discount_function_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  case discount_type {
    "app" -> {
      let function_id = read_string(input, "functionId")
      let function_handle = read_string(input, "functionHandle")
      case function_id, function_handle {
        None, None -> [
          app_discount_missing_function_identifier_error(input_name),
        ]
        Some(_), Some(_) -> [
          app_discount_multiple_function_identifiers_error(input_name),
        ]
        Some(value), None ->
          validate_app_discount_function_reference(
            store,
            input_name,
            "functionId",
            value,
          )
        None, Some(value) ->
          validate_app_discount_function_reference(
            store,
            input_name,
            "functionHandle",
            value,
          )
      }
    }
    _ -> []
  }
}

fn validate_app_discount_function_reference(
  store: Store,
  input_name: String,
  field_name: String,
  value: String,
) -> List(SourceValue) {
  case find_shopify_function(store, value) {
    None -> [
      app_discount_function_not_found_error(input_name, field_name, value),
    ]
    Some(record) ->
      case app_discount_function_api_supported(record) {
        True -> []
        False -> [
          app_discount_function_does_not_implement_error(input_name, field_name),
        ]
      }
  }
}

fn app_discount_missing_function_identifier_error(
  input_name: String,
) -> SourceValue {
  user_error(
    [input_name, "functionHandle"],
    "Function id can't be blank.",
    "MISSING_FUNCTION_IDENTIFIER",
  )
}

fn app_discount_multiple_function_identifiers_error(
  input_name: String,
) -> SourceValue {
  user_error(
    [input_name],
    "Only one of functionId or functionHandle is allowed.",
    "MULTIPLE_FUNCTION_IDENTIFIERS",
  )
}

fn app_discount_function_not_found_error(
  input_name: String,
  field_name: String,
  value: String,
) -> SourceValue {
  user_error(
    [input_name, field_name],
    "Function "
      <> value
      <> " not found. Ensure that it is released in the current app ("
      <> discount_function_app_id
      <> "), and that the app is installed.",
    "INVALID",
  )
}

fn app_discount_function_does_not_implement_error(
  input_name: String,
  field_name: String,
) -> SourceValue {
  user_error_with_code(
    [input_name, field_name],
    "Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].",
    None,
  )
}

fn app_discount_function_api_supported(record: ShopifyFunctionRecord) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) ->
      list.contains(
        [
          "DISCOUNT",
          "PRODUCT_DISCOUNT",
          "PRODUCT_DISCOUNTS",
          "ORDER_DISCOUNT",
          "ORDER_DISCOUNTS",
          "SHIPPING_DISCOUNT",
          "SHIPPING_DISCOUNTS",
          "PURCHASE_PRODUCT_DISCOUNT_RUN",
          "PURCHASE_ORDER_DISCOUNT_RUN",
          "PURCHASE_SHIPPING_DISCOUNT_RUN",
        ],
        normalize_function_api_type(api_type),
      )
  }
}

fn normalize_function_api_type(api_type: String) -> String {
  api_type
  |> string.uppercase
  |> string.replace("-", "_")
  |> string.replace(".", "_")
}

fn validate_context_customer_selection_conflict(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case
    input_value_is_present(input, "context"),
    input_value_is_present(input, "customerSelection")
  {
    True, True -> [
      user_error(
        [input_name, "context"],
        "Only one of context or customerSelection can be provided.",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
}

fn input_value_is_present(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.NullVal) | Error(_) -> False
    Ok(_) -> True
  }
}

fn validate_subscription_fields(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  case subscription_field_location(discount_type, input_name) {
    Some(location) ->
      validate_subscription_field_values(store, input_name, input, location)
    None -> []
  }
}

type SubscriptionFieldLocation {
  SubscriptionCustomerGetsFields
  SubscriptionTopLevelFields
}

fn subscription_field_location(
  discount_type: String,
  input_name: String,
) -> Option(SubscriptionFieldLocation) {
  case discount_type, input_name {
    "basic", _ -> Some(SubscriptionCustomerGetsFields)
    "free_shipping", "freeShippingAutomaticDiscount" -> None
    "free_shipping", _ -> Some(SubscriptionTopLevelFields)
    _, _ -> None
  }
}

fn maybe_hydrate_discount_subscription_capability(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_shop(store) {
    Some(_) -> store
    None ->
      case subscription_field_location(discount_type, input_name) {
        Some(location) ->
          case has_subscription_validation_fields(input, location) {
            True -> fetch_shop_subscription_capability(store, upstream)
            False -> store
          }
        None -> store
      }
  }
}

fn has_subscription_validation_fields(
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> Bool {
  let #(fields, _) = subscription_field_source("", input, location)
  dict.has_key(fields, "appliesOnSubscription")
  || dict.has_key(fields, "appliesOnOneTimePurchase")
  || dict.has_key(input, "recurringCycleLimit")
}

fn fetch_shop_subscription_capability(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  let query =
    "query DraftProxyShopSubscriptionCapability {
  shop {
    features {
      sellsSubscriptions
    }
  }
}
"
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "DraftProxyShopSubscriptionCapability",
      query,
      json.object([]),
    )
  {
    Ok(value) ->
      case shop_sells_subscriptions_from_response(value) {
        Some(sells_subscriptions) ->
          store.set_shop_sells_subscriptions(store, sells_subscriptions)
        None -> store
      }
    Error(_) -> store
  }
}

fn shop_sells_subscriptions_from_response(
  value: commit.JsonValue,
) -> Option(Bool) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shop") {
        Some(shop) ->
          case json_get(shop, "features") {
            Some(features) ->
              case json_get(features, "sellsSubscriptions") {
                Some(commit.JsonBool(value)) -> Some(value)
                _ -> None
              }
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

fn validate_subscription_field_values(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  case store.shop_sells_subscriptions(store) {
    False ->
      subscription_fields_not_permitted_errors(input_name, input, location)
    True -> blank_subscription_field_errors(input_name, input, location)
  }
}

fn subscription_fields_not_permitted_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  let errors =
    subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnSubscription",
      subscription_not_permitted_message(location, "appliesOnSubscription"),
    )
  let errors =
    list.append(
      errors,
      subscription_field_error(
        input_name,
        input,
        location,
        "appliesOnOneTimePurchase",
        subscription_not_permitted_message(location, "appliesOnOneTimePurchase"),
      ),
    )
  case dict.has_key(input, "recurringCycleLimit") {
    True ->
      list.append(errors, [
        user_error(
          [input_name, "recurringCycleLimit"],
          "Recurring cycle limit is not permitted for this shop.",
          "INVALID",
        ),
      ])
    False -> errors
  }
}

fn subscription_not_permitted_message(
  location: SubscriptionFieldLocation,
  field_name: String,
) -> String {
  case location, field_name {
    SubscriptionCustomerGetsFields, "appliesOnSubscription" ->
      "Customer gets applies on subscription is not permitted for this shop."
    SubscriptionCustomerGetsFields, "appliesOnOneTimePurchase" ->
      "Customer gets applies on one time purchase is not permitted for this shop."
    SubscriptionTopLevelFields, "appliesOnSubscription" ->
      "Applies on subscription is not permitted for this shop."
    SubscriptionTopLevelFields, "appliesOnOneTimePurchase" ->
      "Applies on one time purchase is not permitted for this shop."
    _, _ -> "Subscription field is not permitted for this shop."
  }
}

fn subscription_field_error(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
  field_name: String,
  message: String,
) -> List(SourceValue) {
  let #(fields, path) = subscription_field_source(input_name, input, location)
  case dict.has_key(fields, field_name) {
    True -> [
      user_error(list.append(path, [field_name]), message, "INVALID"),
    ]
    False -> []
  }
}

fn blank_subscription_field_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  let errors =
    blank_subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnSubscription",
      "applies_on_subscription can't be blank",
    )
  list.append(
    errors,
    blank_subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnOneTimePurchase",
      "applies_on_one_time_purchase can't be blank",
    ),
  )
}

fn blank_subscription_field_error(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
  field_name: String,
  message: String,
) -> List(SourceValue) {
  let #(fields, path) = subscription_field_source(input_name, input, location)
  case dict.get(fields, field_name) {
    Ok(root_field.NullVal) -> [
      user_error(list.append(path, [field_name]), message, "INVALID"),
    ]
    _ -> []
  }
}

fn subscription_field_source(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> #(Dict(String, root_field.ResolvedValue), List(String)) {
  case location {
    SubscriptionCustomerGetsFields ->
      case customer_gets_fields(input) {
        Some(fields) -> #(fields, [input_name, "customerGets"])
        None -> #(dict.new(), [input_name, "customerGets"])
      }
    SubscriptionTopLevelFields -> #(input, [input_name])
  }
}

fn validate_discount_update_input(
  input: Dict(String, root_field.ResolvedValue),
  existing_record: DiscountRecord,
) -> List(SourceValue) {
  case read_string(input, "code") {
    Some(_) -> {
      case is_bulk_rule_discount(existing_record) {
        True -> [
          user_error(
            ["id"],
            "Cannot update the code of a bulk discount.",
            "INVALID",
          ),
        ]
        False -> []
      }
    }
    None -> []
  }
}

fn is_bulk_rule_discount(record: DiscountRecord) -> Bool {
  list.length(existing_code_nodes(record)) > 1
}

fn validate_discount_code_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  require_code: Bool,
) -> List(SourceValue) {
  case read_string(input, "code") {
    None ->
      case require_code {
        True -> [discount_code_blank_error(input_name)]
        False -> []
      }
    Some(code) ->
      case string.trim(code) {
        "" ->
          case code {
            "" -> [
              user_error(
                [input_name, "code"],
                "Code is too short (minimum is 1 character)",
                "TOO_SHORT",
              ),
            ]
            _ -> [discount_code_blank_error(input_name)]
          }
        _ ->
          case string.length(code) > 255 {
            True -> [
              user_error(
                [input_name, "code"],
                "Code is too long (maximum is 255 characters)",
                "TOO_LONG",
              ),
            ]
            False ->
              case string.contains(code, "\n") || string.contains(code, "\r") {
                True -> [
                  user_error(
                    [input_name, "code"],
                    "Code cannot contain newline characters.",
                    "INVALID",
                  ),
                ]
                False -> []
              }
          }
      }
  }
}

fn discount_code_blank_error(input_name: String) -> SourceValue {
  user_error([input_name, "code"], "Code can't be blank", "BLANK")
}

fn validate_minimum_requirement(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_value(input, "minimumRequirement") {
    root_field.ObjectVal(fields) -> {
      let has_quantity = has_object_field(fields, "quantity")
      let has_subtotal = has_object_field(fields, "subtotal")
      let errors = case has_quantity && has_subtotal {
        True -> [
          user_error(
            [
              input_name,
              "minimumRequirement",
              "subtotal",
              "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal cannot be defined when minimum quantity is.",
            "CONFLICT",
          ),
          user_error(
            [
              input_name,
              "minimumRequirement",
              "quantity",
              "greaterThanOrEqualToQuantity",
            ],
            "Minimum quantity cannot be defined when minimum subtotal is.",
            "CONFLICT",
          ),
        ]
        False -> []
      }
      errors
      |> list.append(validate_minimum_quantity_limit(input_name, fields))
      |> list.append(validate_minimum_subtotal_limit(input_name, fields))
    }
    _ -> []
  }
}

fn has_object_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(_)) -> True
    _ -> False
  }
}

fn validate_minimum_quantity_limit(
  input_name: String,
  fields: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case dict.get(fields, "quantity") {
    Ok(root_field.ObjectVal(quantity)) ->
      case read_numeric_string(quantity, "greaterThanOrEqualToQuantity") {
        Some(value) ->
          case decimal_at_least(value, "2147483647") {
            True -> [
              user_error(
                [
                  input_name,
                  "minimumRequirement",
                  "quantity",
                  "greaterThanOrEqualToQuantity",
                ],
                "Minimum quantity must be less than 2147483647",
                "LESS_THAN",
              ),
            ]
            False -> []
          }
        None -> []
      }
    _ -> []
  }
}

fn validate_minimum_subtotal_limit(
  input_name: String,
  fields: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case dict.get(fields, "subtotal") {
    Ok(root_field.ObjectVal(subtotal)) ->
      case read_numeric_string(subtotal, "greaterThanOrEqualToSubtotal") {
        Some(value) ->
          case decimal_at_least(value, "1000000000000000000") {
            True -> [
              user_error(
                [
                  input_name,
                  "minimumRequirement",
                  "subtotal",
                  "greaterThanOrEqualToSubtotal",
                ],
                "Minimum subtotal must be less than 1000000000000000000",
                "LESS_THAN",
              ),
            ]
            False -> []
          }
        None -> []
      }
    _ -> []
  }
}

fn read_numeric_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.IntVal(value)) -> Some(int.to_string(value))
    Ok(root_field.FloatVal(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

fn decimal_at_least(value: String, limit: String) -> Bool {
  let value = string.trim(value)
  let value = case string.starts_with(value, "+") {
    True -> string.drop_start(value, 1)
    False -> value
  }
  case string.starts_with(value, "-") {
    True -> False
    False ->
      case string.split(value, ".") {
        [whole] -> decimal_parts_at_least(whole, "", limit)
        [whole, decimals] -> decimal_parts_at_least(whole, decimals, limit)
        _ -> False
      }
  }
}

fn decimal_parts_at_least(
  whole: String,
  decimals: String,
  limit: String,
) -> Bool {
  case digits_only(whole) && digits_only(decimals) {
    False -> False
    True -> {
      let whole = trim_leading_zeroes(whole)
      case int.compare(string.length(whole), string.length(limit)) {
        order.Gt -> True
        order.Lt -> False
        order.Eq ->
          case string.compare(whole, limit) {
            order.Lt -> False
            order.Eq | order.Gt -> True
          }
      }
    }
  }
}

fn digits_only(value: String) -> Bool {
  value
  |> string.to_graphemes
  |> list.all(fn(grapheme) {
    case grapheme {
      "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
      _ -> False
    }
  })
}

fn trim_leading_zeroes(value: String) -> String {
  case value {
    "0" <> rest -> trim_leading_zeroes(rest)
    "" -> "0"
    _ -> value
  }
}

/// Pattern 2: ask upstream whether a discount with the proposed code
/// already exists. Returns a `TAKEN` userError when the lookup confirms
/// a hit. Only code-discount creates carry a `code` (automatic
/// discounts never carry one), so automatics short-circuit immediately.
/// In `Snapshot` mode (no `SyncTransport` installed) this is a no-op —
/// the captured-cassette check is the only place a uniqueness signal
/// can come from when no records have been staged yet.
fn fetch_taken_code_error(
  input: Dict(String, root_field.ResolvedValue),
  input_name: String,
  owner_kind: String,
  upstream: UpstreamContext,
) -> Option(SourceValue) {
  case owner_kind {
    "automatic" -> None
    _ -> {
      let code =
        read_string(input, "code")
        |> option.or(read_string(input, "codePrefix"))
      case code {
        None -> None
        Some(code) -> {
          let query =
            "query DiscountUniquenessCheck($code: String!) {
  codeDiscountNodeByCode(code: $code) { id }
}
"
          let variables = json.object([#("code", json.string(code))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "DiscountUniquenessCheck",
              query,
              variables,
            )
          {
            Ok(value) ->
              case existing_discount_id(value) {
                True ->
                  Some(user_error(
                    [input_name, "code"],
                    "Code must be unique. Please try a different code.",
                    "TAKEN",
                  ))
                False -> None
              }
            // Snapshot mode (no transport installed) and any other
            // transport-level failure (cassette miss, malformed
            // response, HTTP error) silently fall through to the
            // local-only validation result. Cassette misses surface
            // through the runner directly when a cassette is in play.
            Error(_) -> None
          }
        }
      }
    }
  }
}

/// Pattern 2: ask upstream for the current state of a code-discount
/// (id, basic metadata, codes connection) and seed it into the local
/// `base_state` so that any subsequent staged mutation overlays on top
/// of that real shape. Used by `redeem_code_bulk_add` /
/// `redeem_code_bulk_delete` so the read-after-write `codeDiscountNode`
/// / `codeDiscountNodeByCode` queries find the discount locally and
/// project the right `codesCount`.
///
/// Returns the original `(store, identity)` when:
///  - the discount is already in the local store (nothing to do),
///  - no transport is installed (Snapshot mode / production JS without
///    cassette: cassette miss = silent no-op so the legacy local-only
///    behavior applies),
///  - the upstream response is malformed or contains a null node.
///
/// The hydrated record carries only the fields the read-after targets
/// actually project (id, codeDiscount.codes, codesCount). Other fields
/// are absent — fine because the read targets in this scenario don't
/// project them.
fn maybe_hydrate_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case store.get_effective_discount_by_id(store, id) {
    Some(_) -> #(store, identity)
    None -> {
      // The hydrate query asks for both `codeDiscountNode` and
      // `automaticDiscountNode` projections under aliases, so callers
      // that don't know whether the id refers to a code- or
      // automatic-owned discount can use a single query + cassette
      // entry. The handler picks the non-null projection. Status and
      // title are pulled in alongside codes so downstream-read targets
      // that use `discountNodesCount(query: "status:active")` /
      // `status:expired` can compute correct counts after the bulk-job
      // status-mutation effects apply on top of the hydrated base
      // record.
      let query =
        "query DiscountHydrate($id: ID!) {
  codeNode: codeDiscountNode(id: $id) {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        status
        codes(first: 250) { nodes { id code } }
      }
      ... on DiscountCodeApp {
        title
        status
      }
      ... on DiscountCodeBxgy {
        title
        status
      }
      ... on DiscountCodeFreeShipping {
        title
        status
      }
    }
  }
  automaticNode: automaticDiscountNode(id: $id) {
    id
    automaticDiscount {
      __typename
      ... on DiscountAutomaticBasic {
        title
        status
      }
      ... on DiscountAutomaticApp {
        title
        status
      }
      ... on DiscountAutomaticBxgy {
        title
        status
      }
      ... on DiscountAutomaticFreeShipping {
        title
        status
      }
    }
  }
}
"
      let variables = json.object([#("id", json.string(id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "DiscountHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          case discount_record_from_hydrate(value, id) {
            Some(record) -> #(
              store.upsert_base_discounts(store, [record]),
              identity,
            )
            None -> #(store, identity)
          }
        Error(_) -> #(store, identity)
      }
    }
  }
}

/// Build a minimal `DiscountRecord` from a `DiscountHydrate` upstream
/// response. The record carries the codes connection so the read
/// handlers project `codesCount` and the by-code lookup correctly. The
/// rest of the discount payload is left empty — the read-after-write
/// targets in this scenario only project codes-related fields.
fn discount_record_from_hydrate(
  value: commit.JsonValue,
  id: String,
) -> Option(DiscountRecord) {
  case json_get(value, "data") {
    None -> None
    Some(data) -> {
      // Prefer the non-null projection. The runtime's response will have
      // exactly one of `codeNode` / `automaticNode` non-null for any
      // given id; if both are present (shouldn't happen in practice) we
      // pick code first to match the legacy lookup order.
      //
      // Older cassettes recorded the response under the unaliased
      // `codeDiscountNode` field (before the query learned to ask for
      // both code and automatic projections in one round-trip), so
      // accept that shape too as a fallback.
      let code_node =
        non_null_node(json_get(data, "codeNode"))
        |> option.or(non_null_node(json_get(data, "codeDiscountNode")))
      let automatic_node = non_null_node(json_get(data, "automaticNode"))
      case code_node, automatic_node {
        Some(node), _ -> Some(code_record_from_hydrate_node(node, id))
        None, Some(node) -> Some(automatic_record_from_hydrate_node(node, id))
        None, None -> None
      }
    }
  }
}

fn non_null_node(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(node) -> Some(node)
    None -> None
  }
}

fn code_record_from_hydrate_node(
  node: commit.JsonValue,
  id: String,
) -> DiscountRecord {
  let discount = json_get(node, "codeDiscount")
  let typename =
    discount
    |> option.then(fn(d) { json_get_string(d, "__typename") })
    |> option.unwrap("DiscountCodeBasic")
  let title = discount |> option.then(fn(d) { json_get_string(d, "title") })
  let status =
    discount
    |> option.then(fn(d) { json_get_string(d, "status") })
    |> option.unwrap("ACTIVE")
  let codes = case discount {
    Some(d) ->
      case json_get(d, "codes") {
        Some(codes_obj) ->
          case json_get(codes_obj, "nodes") {
            Some(commit.JsonArray(items)) ->
              list.filter_map(items, json_to_code_pair)
            _ -> []
          }
        None -> []
      }
    None -> []
  }
  let first_code = case codes {
    [#(_, code), ..] -> Some(code)
    [] -> None
  }
  let payload =
    source_to_captured(
      SrcObject(
        dict.from_list([
          #("id", SrcString(id)),
          #(
            "codeDiscount",
            SrcObject(
              dict.from_list([
                #("__typename", SrcString(typename)),
                #(
                  "title",
                  title |> option.map(SrcString) |> option.unwrap(SrcNull),
                ),
                #("status", SrcString(status)),
                #(
                  "codes",
                  SrcObject(
                    dict.from_list([
                      #(
                        "nodes",
                        SrcList(
                          list.map(codes, fn(pair) {
                            let #(code_id, code) = pair
                            SrcObject(
                              dict.from_list([
                                #("id", SrcString(code_id)),
                                #("code", SrcString(code)),
                                #("asyncUsageCount", SrcInt(0)),
                              ]),
                            )
                          }),
                        ),
                      ),
                      #("edges", SrcList([])),
                      #(
                        "pageInfo",
                        SrcObject(
                          dict.from_list([
                            #("hasNextPage", SrcBool(False)),
                            #("hasPreviousPage", SrcBool(False)),
                            #("startCursor", SrcNull),
                            #("endCursor", SrcNull),
                          ]),
                        ),
                      ),
                    ]),
                  ),
                ),
                #("codesCount", count_source(list.length(codes))),
              ]),
            ),
          ),
        ]),
      ),
    )
  DiscountRecord(
    id: id,
    owner_kind: "code",
    discount_type: discount_type_from_typename(typename),
    title: title,
    status: status,
    code: first_code,
    payload: payload,
    cursor: None,
  )
}

fn automatic_record_from_hydrate_node(
  node: commit.JsonValue,
  id: String,
) -> DiscountRecord {
  let discount = json_get(node, "automaticDiscount")
  let typename =
    discount
    |> option.then(fn(d) { json_get_string(d, "__typename") })
    |> option.unwrap("DiscountAutomaticBasic")
  let title = discount |> option.then(fn(d) { json_get_string(d, "title") })
  let status =
    discount
    |> option.then(fn(d) { json_get_string(d, "status") })
    |> option.unwrap("ACTIVE")
  let payload =
    source_to_captured(
      SrcObject(
        dict.from_list([
          #("id", SrcString(id)),
          #(
            "automaticDiscount",
            SrcObject(
              dict.from_list([
                #("__typename", SrcString(typename)),
                #(
                  "title",
                  title |> option.map(SrcString) |> option.unwrap(SrcNull),
                ),
                #("status", SrcString(status)),
              ]),
            ),
          ),
        ]),
      ),
    )
  DiscountRecord(
    id: id,
    owner_kind: "automatic",
    discount_type: discount_type_from_typename(typename),
    title: title,
    status: status,
    code: None,
    payload: payload,
    cursor: None,
  )
}

fn discount_type_from_typename(typename: String) -> String {
  case typename {
    "DiscountCodeBxgy" | "DiscountAutomaticBxgy" -> "bxgy"
    "DiscountCodeFreeShipping" | "DiscountAutomaticFreeShipping" ->
      "free_shipping"
    "DiscountCodeApp" | "DiscountAutomaticApp" -> "app"
    _ -> "basic"
  }
}

fn json_to_code_pair(
  value: commit.JsonValue,
) -> Result(#(String, String), Nil) {
  case json_get(value, "id"), json_get(value, "code") {
    Some(commit.JsonString(id)), Some(commit.JsonString(code)) ->
      Ok(#(id, code))
    _, _ -> Error(Nil)
  }
}

/// Pattern 2: hydrate a `ShopifyFunctionRecord` from upstream when the
/// caller supplies exactly one app-discount `functionHandle`/`functionId`
/// and the local store does not already know about that function. Used at
/// app-discount-create time so validation can distinguish an unknown
/// function from a known non-discount Function and so `appDiscountType.appKey`
/// / `title` / `description` project the real function metadata instead of
/// falling back to the discount input title.
///
/// Cassette miss / Snapshot mode / malformed response is silently
/// tolerated — the existing local-only behavior takes over (input title
/// fallback, null app key/description). Returns the original `store`
/// when the function is already known or the upstream call failed.
fn maybe_hydrate_shopify_function(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let function_id = read_string(input, "functionId")
  let function_handle = read_string(input, "functionHandle")
  case function_id, function_handle {
    None, None -> store
    Some(_), Some(_) -> store
    Some(reference), None | None, Some(reference) ->
      case find_shopify_function(store, reference) {
        Some(_) -> store
        None -> {
          let query =
            "query ShopifyFunctionByHandle($handle: String!) {
  shopifyFunctions(first: 1, handle: $handle) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        id
        title
        handle
        apiKey
      }
    }
  }
}
"
          let variables = json.object([#("handle", json.string(reference))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "ShopifyFunctionByHandle",
              query,
              variables,
            )
          {
            Ok(value) ->
              case shopify_function_record_from_response(value) {
                Some(record) -> {
                  let #(_, next_store) =
                    store.upsert_staged_shopify_function(store, record)
                  next_store
                }
                None -> store
              }
            Error(_) -> store
          }
        }
      }
  }
}

/// Pull the first `shopifyFunctions.nodes[0]` entry off a
/// `ShopifyFunctionByHandle` upstream response and lift it into a
/// `ShopifyFunctionRecord`. Returns `None` for any shape divergence so
/// the caller falls back to the local-only behavior.
fn shopify_function_record_from_response(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shopifyFunctions") {
        Some(connection) ->
          case json_get(connection, "nodes") {
            Some(commit.JsonArray([first, ..])) ->
              shopify_function_record_from_node(first)
            _ -> None
          }
        None -> None
      }
    None -> None
  }
}

fn shopify_function_record_from_node(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  case json_get(value, "id") {
    Some(commit.JsonString(id)) ->
      Some(ShopifyFunctionRecord(
        id: id,
        title: json_get_string(value, "title"),
        handle: json_get_string(value, "handle"),
        api_type: json_get_string(value, "apiType"),
        description: json_get_string(value, "description"),
        app_key: json_get_string(value, "appKey"),
        app: shopify_function_app_record_from_node(json_get(value, "app")),
      ))
    _ -> None
  }
}

fn shopify_function_app_record_from_node(
  value: Option(commit.JsonValue),
) -> Option(ShopifyFunctionAppRecord) {
  case value {
    Some(node) ->
      Some(ShopifyFunctionAppRecord(
        typename: json_get_string(node, "__typename"),
        id: json_get_string(node, "id"),
        title: json_get_string(node, "title"),
        handle: json_get_string(node, "handle"),
        api_key: json_get_string(node, "apiKey"),
      ))
    None -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

/// Read `data.codeDiscountNodeByCode.id` from the upstream response
/// AST. Treats anything other than a non-null string id as "no such
/// discount." Walks `commit.JsonValue` so we don't have to round-trip
/// through serialized JSON.
fn existing_discount_id(value: commit.JsonValue) -> Bool {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "codeDiscountNodeByCode") {
        Some(node) ->
          case json_get(node, "id") {
            Some(commit.JsonString(_)) -> True
            _ -> False
          }
        None -> False
      }
    None -> False
  }
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

fn append_blank_title_error(
  errors: List(SourceValue),
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case input_name == "freeShippingCodeDiscount" && title_is_blank(input) {
    True ->
      list.append(errors, [
        user_error([input_name, "title"], "Title can't be blank", "BLANK"),
      ])
    False -> errors
  }
}

fn title_is_blank(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_string(input, "title") {
    Some(title) -> string.trim(title) == ""
    None -> False
  }
}

fn validate_bxgy_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let errors = case nested_has_all(read_value(input, "customerGets"), "items") {
    True -> [
      user_error(
        [input_name, "customerGets"],
        "Items in 'customer get' cannot be set to all",
        "INVALID",
      ),
    ]
    False -> []
  }
  let errors =
    list.append(errors, bxgy_disallowed_value_errors(input_name, input))
  let errors =
    list.append(
      errors,
      bxgy_missing_discount_on_quantity_errors(input_name, input),
    )
  let errors =
    list.append(errors, bxgy_disallowed_subscription_errors(input_name, input))
  let errors = case title_is_blank(input) {
    True ->
      list.append(errors, [
        user_error([input_name, "title"], "Title can't be blank", "BLANK"),
      ])
    False -> errors
  }
  case nested_has_all(read_value(input, "customerBuys"), "items") {
    True ->
      list.append(errors, [
        user_error(
          [input_name, "customerBuys", "items"],
          "Items in 'customer buys' must be defined",
          "BLANK",
        ),
      ])
    False -> errors
  }
}

fn bxgy_disallowed_value_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_value_fields(input) {
    Some(fields) -> {
      let errors = case dict.has_key(fields, "percentage") {
        True -> [
          user_error(
            [input_name, "customerGets", "value", "percentage"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            "INVALID",
          ),
        ]
        False -> []
      }
      case dict.has_key(fields, "discountAmount") {
        True ->
          list.append(errors, [
            user_error(
              [input_name, "customerGets", "value", "discountAmount"],
              "Only discountOnQuantity permitted with bxgy discounts.",
              "INVALID",
            ),
          ])
        False -> errors
      }
    }
    None -> []
  }
}

fn bxgy_missing_discount_on_quantity_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case input_name, customer_gets_value_fields(input) {
    "bxgyCodeDiscount", Some(fields) ->
      case dict.get(fields, "discountOnQuantity") {
        Ok(root_field.ObjectVal(on_quantity)) ->
          case read_string(on_quantity, "quantity") {
            Some(quantity) ->
              case string.trim(quantity) {
                "" -> [
                  bxgy_discount_on_quantity_quantity_blank_error(input_name),
                ]
                _ -> []
              }
            None -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
          }
        Ok(_) -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
        Error(_) -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
      }
    _, _ -> []
  }
}

fn bxgy_discount_on_quantity_quantity_blank_error(
  input_name: String,
) -> SourceValue {
  user_error(
    [input_name, "customerGets", "value", "discountOnQuantity", "quantity"],
    "Quantity cannot be blank.",
    "BLANK",
  )
}

fn bxgy_disallowed_subscription_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_fields(input) {
    Some(fields) -> {
      let message = case input_name {
        "automaticBxgyDiscount" ->
          "This field is not supported by automatic bxgy discounts."
        _ -> "This field is not supported by bxgy discounts."
      }
      let errors = case dict.has_key(fields, "appliesOnSubscription") {
        True -> [
          user_error(
            [input_name, "customerGets", "appliesOnSubscription"],
            message,
            "INVALID",
          ),
        ]
        False -> []
      }
      case dict.has_key(fields, "appliesOnOneTimePurchase") {
        True ->
          list.append(errors, [
            user_error(
              [input_name, "customerGets", "appliesOnOneTimePurchase"],
              message,
              "INVALID",
            ),
          ])
        False -> errors
      }
    }
    None -> []
  }
}

fn customer_gets_value_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case customer_gets_fields(input) {
    Some(fields) ->
      case dict.get(fields, "value") {
        Ok(root_field.ObjectVal(value_fields)) -> Some(value_fields)
        _ -> None
      }
    None -> None
  }
}

fn customer_gets_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case read_value(input, "customerGets") {
    root_field.ObjectVal(fields) -> Some(fields)
    _ -> None
  }
}

fn customer_gets_items_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case customer_gets_fields(input) {
    Some(gets) ->
      case read_value(gets, "items") {
        root_field.ObjectVal(items) -> Some(items)
        _ -> None
      }
    None -> None
  }
}

fn nested_has_all(value: root_field.ResolvedValue, child: String) -> Bool {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, child) {
        Ok(root_field.ObjectVal(child_fields)) ->
          dict.has_key(child_fields, "all")
        _ -> False
      }
    _ -> False
  }
}

fn validate_discount_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  list.append(
    validate_customer_gets_value_type_top_level_errors(input, field, document),
    validate_cart_line_combination_tag_top_level_errors(input, field, document),
  )
}

fn validate_customer_gets_value_type_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  case customer_gets_value_fields(input) {
    Some(fields) ->
      case customer_gets_value_type_count(fields) > 1 {
        True -> [
          json.object([
            #(
              "message",
              json.string(
                "A discount can only have one of percentage, discountOnQuantity or discountAmount.",
              ),
            ),
            #("locations", field_locations_json(field, document)),
            #(
              "extensions",
              json.object([#("code", json.string("BAD_REQUEST"))]),
            ),
            #("path", json.array([get_field_response_key(field)], json.string)),
          ]),
        ]
        False -> []
      }
    None -> []
  }
}

fn customer_gets_value_type_count(
  fields: Dict(String, root_field.ResolvedValue),
) -> Int {
  let count = case dict.has_key(fields, "percentage") {
    True -> 1
    False -> 0
  }
  let count = case dict.has_key(fields, "discountAmount") {
    True -> count + 1
    False -> count
  }
  case dict.has_key(fields, "discountOnQuantity") {
    True -> count + 1
    False -> count
  }
}

fn validate_cart_line_combination_tag_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  case product_discounts_with_tags_settings(input) {
    Some(settings) ->
      case tag_add_remove_overlap(settings) {
        True -> [
          json.object([
            #(
              "message",
              json.string(
                "The same tag is present in both `add` and `remove` fields of `productDiscountsWithTagsOnSameCartLine`.",
              ),
            ),
            #("locations", field_locations_json(field, document)),
            #(
              "extensions",
              json.object([#("code", json.string("BAD_REQUEST"))]),
            ),
            #("path", json.array([get_field_response_key(field)], json.string)),
          ]),
        ]
        False -> []
      }
    None -> []
  }
}

fn validate_cart_line_combination_tag_settings(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_classes: List(String),
) -> List(SourceValue) {
  case product_discounts_with_tags_settings(input) {
    Some(_) -> {
      let path = [
        input_name,
        "combinesWith",
        "productDiscountsWithTagsOnSameCartLine",
      ]
      let errors = [
        user_error(
          path,
          "The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.",
          "PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED",
        ),
      ]
      case list.contains(discount_classes, "PRODUCT") {
        True -> errors
        False ->
          list.append(errors, [
            user_error(
              path,
              "Combines with product discounts with tags on same cart line is only valid for discounts with the PRODUCT discount class",
              "INVALID_PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_FOR_DISCOUNT_CLASS",
            ),
          ])
      }
    }
    None -> []
  }
}

fn product_discounts_with_tags_settings(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case read_value(input, "combinesWith") {
    root_field.ObjectVal(combines) ->
      case read_value(combines, "productDiscountsWithTagsOnSameCartLine") {
        root_field.ObjectVal(settings) -> Some(settings)
        _ -> None
      }
    _ -> None
  }
}

fn tag_add_remove_overlap(
  settings: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let add_tags = read_string_array(settings, "add", [])
  let remove_tags = read_string_array(settings, "remove", [])
  list.any(remove_tags, fn(tag) { list.contains(add_tags, tag) })
}

fn invalid_free_shipping_combines(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case read_value(input, "combinesWith") {
    root_field.ObjectVal(fields) -> bool_value(fields, "shippingDiscounts")
    _ -> False
  }
}

fn bool_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> False
  }
}

fn invalid_date_range(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_string(input, "startsAt"), read_string(input, "endsAt") {
    Some(starts_at), Some(ends_at) ->
      case
        iso_timestamp.parse_iso(starts_at),
        iso_timestamp.parse_iso(ends_at)
      {
        Ok(starts_at_ms), Ok(ends_at_ms) -> ends_at_ms <= starts_at_ms
        _, _ -> False
      }
    _, _ -> False
  }
}

fn validate_basic_refs(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_value(input, "customerGets") {
    root_field.ObjectVal(gets) ->
      case read_value(gets, "items") {
        root_field.ObjectVal(items) ->
          validate_discount_items_refs(input_name, items)
        _ -> []
      }
    _ -> []
  }
}

fn validate_discount_items_refs(
  input_name: String,
  items: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let has_products = dict.has_key(items, "products")
  let has_collections = dict.has_key(items, "collections")
  let errors = case has_products && has_collections {
    True -> [
      user_error(
        [input_name, "customerGets", "items", "collections", "add"],
        "Cannot entitle collections in combination with product variants or products",
        "CONFLICT",
      ),
    ]
    False -> []
  }
  case dict.get(items, "products") {
    Ok(root_field.ObjectVal(products)) ->
      errors
      |> list.append(
        invalid_id_errors(input_name, products, "productsToAdd", "Product", [
          input_name,
          "customerGets",
          "items",
          "products",
          "productsToAdd",
        ]),
      )
      |> list.append(
        invalid_id_errors(
          input_name,
          products,
          "productVariantsToAdd",
          "Product variant",
          [
            input_name,
            "customerGets",
            "items",
            "products",
            "productVariantsToAdd",
          ],
        ),
      )
    _ -> errors
  }
}

fn invalid_id_errors(
  _input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  path: List(String),
) -> List(SourceValue) {
  read_string_array(input, field, [])
  |> list.filter(fn(id) { string.ends_with(id, "/0") })
  |> list.map(fn(_id) {
    user_error(path, label <> " with id: 0 is invalid", "INVALID")
  })
}

fn validate_bulk_selector(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let count =
    selector_present(args, "ids")
    + selector_present(args, "search")
    + selector_present(args, "savedSearchId")
    + selector_present(args, "saved_search_id")
  case count {
    0 -> [
      user_error_null_field(
        bulk_missing_selector_message(root),
        "MISSING_ARGUMENT",
      ),
    ]
    n if n > 1 -> [
      user_error_null_field(
        bulk_too_many_selector_message(root),
        "TOO_MANY_ARGUMENTS",
      ),
    ]
    _ ->
      list.append(
        validate_bulk_search_selector(root, args),
        validate_bulk_saved_search_selector(store, root, args),
      )
  }
}

fn validate_redeem_code_bulk_delete_selector_shape(
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let count =
    redeem_code_ids_selector_present(args)
    + selector_present(args, "search")
    + selector_present(args, "savedSearchId")
    + selector_present(args, "saved_search_id")
  case count {
    0 -> [
      user_error_null_field(
        "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
        "MISSING_ARGUMENT",
      ),
    ]
    n if n > 1 -> [
      user_error_null_field(
        "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
        "TOO_MANY_ARGUMENTS",
      ),
    ]
    _ -> []
  }
}

fn validate_redeem_code_bulk_delete_after_hydrate(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_string(args, "discountId") {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        None -> [
          user_error(["discountId"], "Code discount does not exist.", "INVALID"),
        ]
        Some(_) ->
          case redeem_code_ids_selector_is_empty(args) {
            True -> [
              user_error_null_field_with_code(
                "Something went wrong, please try again.",
                None,
              ),
            ]
            False ->
              list.append(
                validate_redeem_code_bulk_delete_search_selector(args),
                validate_redeem_code_bulk_delete_saved_search_selector(
                  store,
                  args,
                ),
              )
          }
      }
    None -> [
      user_error(["discountId"], "Code discount does not exist.", "INVALID"),
    ]
  }
}

fn validate_redeem_code_bulk_delete_search_selector(
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_string(args, "search") {
    Some(search) ->
      case string.trim(search) {
        "" -> [user_error(["search"], "'Search' can't be blank.", "BLANK")]
        _ -> []
      }
    _ -> []
  }
}

fn validate_redeem_code_bulk_delete_saved_search_selector(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_bulk_saved_search_id(args) {
    Some(id) ->
      case store.get_effective_saved_search_by_id(store, id) {
        Some(_) -> []
        None -> [
          user_error(["savedSearchId"], "Invalid 'saved_search_id'.", "INVALID"),
        ]
      }
    None -> []
  }
}

fn redeem_code_bulk_delete_target_ids(
  store: Store,
  record: DiscountRecord,
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case dict.has_key(args, "ids") {
    True -> read_string_array(args, "ids", [])
    False ->
      case read_string(args, "search") {
        Some(query) -> redeem_code_ids_matching_query(record, query)
        None ->
          case read_bulk_saved_search_id(args) {
            Some(id) ->
              case store.get_effective_saved_search_by_id(store, id) {
                Some(saved_search) ->
                  redeem_code_ids_matching_query(record, saved_search.query)
                None -> []
              }
            None -> []
          }
      }
  }
}

fn redeem_code_ids_matching_query(
  record: DiscountRecord,
  query: String,
) -> List(String) {
  existing_code_nodes(record)
  |> search_query_parser.apply_search_query(
    Some(query),
    search_query_parser.default_parse_options(),
    redeem_code_matches_positive_search_term,
  )
  |> list.map(fn(pair) { pair.0 })
}

fn redeem_code_matches_positive_search_term(
  pair: #(String, String),
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let #(_id, code) = pair
  case term.field {
    Some("code") ->
      search_query_parser.matches_search_query_string(
        Some(code),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        search_query_parser.default_string_match_options(),
      )
    _ -> search_query_parser.matches_search_query_text(Some(code), term)
  }
}

fn bulk_missing_selector_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" ->
      "One of IDs, search argument or saved search ID is required."
    _ -> "Missing expected argument key: 'ids', 'search' or 'saved_search_id'."
  }
}

fn bulk_too_many_selector_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" ->
      "Only one of IDs, search argument or saved search ID is allowed."
    _ -> "Only one of 'ids', 'search' or 'saved_search_id' is allowed."
  }
}

fn validate_bulk_search_selector(
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_string(args, "search") {
    Some(search) -> {
      case string.trim(search) {
        "" ->
          case root {
            "discountAutomaticBulkDelete" -> []
            _ -> [user_error(["search"], "'Search' can't be blank.", "BLANK")]
          }
        _ -> []
      }
    }
    _ -> []
  }
}

fn validate_bulk_saved_search_selector(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_bulk_saved_search_id(args) {
    Some(id) ->
      case store.get_effective_saved_search_by_id(store, id) {
        Some(record) if record.resource_type == "PRICE_RULE" -> []
        _ -> [
          user_error(
            ["savedSearchId"],
            bulk_invalid_saved_search_message(root),
            "INVALID",
          ),
        ]
      }
    None -> []
  }
}

fn bulk_invalid_saved_search_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" -> "Invalid savedSearchId."
    _ -> "Invalid 'saved_search_id'."
  }
}

fn read_bulk_saved_search_id(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_string(args, "savedSearchId")
  |> option.or(read_string(args, "saved_search_id"))
}

fn selector_present(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Int {
  case dict.get(args, name) {
    Ok(root_field.NullVal) | Error(_) -> 0
    Ok(root_field.ListVal([])) -> 0
    _ -> 1
  }
}

fn redeem_code_ids_selector_present(
  args: Dict(String, root_field.ResolvedValue),
) -> Int {
  case dict.has_key(args, "ids") {
    True -> 1
    False -> 0
  }
}

fn redeem_code_ids_selector_is_empty(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(args, "ids") {
    Ok(root_field.NullVal) | Ok(root_field.ListVal([])) -> True
    _ -> False
  }
}

fn apply_bulk_effects(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(Store, SyntheticIdentityRegistry) {
  let ids = read_string_array(args, "ids", [])
  list.fold(ids, #(store, identity), fn(acc, id) {
    let #(current, current_identity) = acc
    case root {
      "discountCodeBulkDelete" | "discountAutomaticBulkDelete" ->
        case store.get_effective_discount_by_id(current, id) {
          Some(_) -> {
            let #(_, next_identity) =
              synthetic_identity.make_synthetic_timestamp(current_identity)
            #(store.delete_staged_discount(current, id), next_identity)
          }
          None -> #(store.delete_staged_discount(current, id), current_identity)
        }
      "discountCodeBulkActivate" ->
        set_record_status(current, current_identity, id, "ACTIVE")
      "discountCodeBulkDeactivate" ->
        set_record_status(current, current_identity, id, "EXPIRED")
      _ -> #(current, current_identity)
    }
  })
}

fn set_record_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  status: String,
) -> #(Store, SyntheticIdentityRegistry) {
  case store.get_effective_discount_by_id(store, id) {
    Some(record) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(record, next_store) =
        store.stage_discount(
          store,
          DiscountRecord(
            ..record,
            status: status,
            payload: update_payload_status(record.payload, status, None)
              |> update_payload_updated_at(updated_at),
          ),
        )
      let _ = record
      #(next_store, next_identity)
    }
    None -> #(store, identity)
  }
}

fn bxgy_summary(input: Dict(String, root_field.ResolvedValue)) -> String {
  let buys_quantity =
    read_bxgy_quantity(read_value(input, "customerBuys")) |> option.unwrap("1")
  let gets_value = read_value(input, "customerGets")
  let gets_quantity =
    read_discount_on_quantity(gets_value, "quantity")
    |> option.unwrap("1")
  let effect = read_discount_on_quantity_percentage(gets_value)
  let suffix = case effect {
    Some(percentage) ->
      case percentage {
        1.0 -> " free"
        _ -> " at " <> percentage_to_label(percentage) <> " off"
      }
    None -> ""
  }
  "Buy "
  <> buys_quantity
  <> " "
  <> plural_item(buys_quantity)
  <> ", get "
  <> gets_quantity
  <> " "
  <> plural_item(gets_quantity)
  <> suffix
}

fn read_bxgy_quantity(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          resolved_string(read_value(value_fields, "quantity"))
        _ -> None
      }
    _ -> None
  }
}

fn read_discount_on_quantity(
  value: root_field.ResolvedValue,
  key: String,
) -> Option(String) {
  case discount_on_quantity_fields(value) {
    Some(fields) -> resolved_string(read_value(fields, key))
    None -> None
  }
}

fn read_discount_on_quantity_percentage(
  value: root_field.ResolvedValue,
) -> Option(Float) {
  case discount_on_quantity_fields(value) {
    Some(fields) ->
      case read_value(fields, "effect") {
        root_field.ObjectVal(effect) ->
          case read_value(effect, "percentage") {
            root_field.FloatVal(value) -> Some(value)
            root_field.IntVal(value) -> Some(int.to_float(value))
            _ -> None
          }
        _ -> None
      }
    None -> None
  }
}

fn discount_on_quantity_fields(
  value: root_field.ResolvedValue,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          case dict.get(value_fields, "discountOnQuantity") {
            Ok(root_field.ObjectVal(discount_on_quantity)) ->
              Some(discount_on_quantity)
            _ ->
              case dict.get(value_fields, "onQuantity") {
                Ok(root_field.ObjectVal(on_quantity)) -> Some(on_quantity)
                _ -> None
              }
          }
        _ -> None
      }
    _ -> None
  }
}

fn resolved_string(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.StringVal(value) -> Some(value)
    root_field.IntVal(value) -> Some(int.to_string(value))
    _ -> None
  }
}

fn percentage_to_label(value: Float) -> String {
  int.to_string(float.round(value *. 100.0)) <> "%"
}

fn plural_item(quantity: String) -> String {
  case quantity {
    "1" -> "item"
    _ -> "items"
  }
}

fn user_error(
  field: List(String),
  message: String,
  code: String,
) -> SourceValue {
  user_error_with_code(field, message, Some(code))
}

fn user_error_with_code(
  field: List(String),
  message: String,
  code: Option(String),
) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcList(list.map(field, SrcString))),
      #("message", SrcString(message)),
      #("code", code |> option.map(SrcString) |> option.unwrap(SrcNull)),
      #("extraInfo", SrcNull),
    ]),
  )
}

fn user_error_null_field(message: String, code: String) -> SourceValue {
  user_error_null_field_with_code(message, Some(code))
}

fn user_error_null_field_with_code(
  message: String,
  code: Option(String),
) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcNull),
      #("message", SrcString(message)),
      #("code", code |> option.map(SrcString) |> option.unwrap(SrcNull)),
      #("extraInfo", SrcNull),
    ]),
  )
}

fn read_string_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> read_string(args, name)
    Error(_) -> None
  }
}

fn option_to_list(value: Option(a)) -> List(a) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

fn read_int_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Int) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.IntVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_bool_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.BoolVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_object_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ObjectVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> root_field.ResolvedValue {
  dict.get(input, name) |> result.unwrap(root_field.NullVal)
}

fn read_string_array(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: List(String),
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> fallback
  }
}

fn read_codes_arg_with_shape(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> #(List(String), Bool) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ListVal(items)) -> {
          let has_object_inputs =
            list.any(items, fn(item) {
              case item {
                root_field.ObjectVal(_) -> True
                _ -> False
              }
            })
          #(
            list.filter_map(items, fn(item) {
              case item {
                root_field.StringVal(value) -> Ok(value)
                root_field.ObjectVal(fields) ->
                  read_string(fields, "code") |> option.to_result(Nil)
                _ -> Error(Nil)
              }
            }),
            has_object_inputs,
          )
        }
        _ -> #([], False)
      }
    Error(_) -> #([], False)
  }
}

fn resolved_to_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.NullVal -> SrcNull
    root_field.StringVal(value) -> SrcString(value)
    root_field.BoolVal(value) -> SrcBool(value)
    root_field.IntVal(value) -> SrcInt(value)
    root_field.FloatVal(value) -> SrcFloat(value)
    root_field.ListVal(items) -> SrcList(list.map(items, resolved_to_source))
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.map_values(fields, fn(_, value) { resolved_to_source(value) }),
      )
  }
}

fn object_value_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: SourceValue,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(value) -> resolved_to_source(value)
    Error(_) -> fallback
  }
}

fn combines_default() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("productDiscounts", SrcBool(False)),
      #("orderDiscounts", SrcBool(False)),
      #("shippingDiscounts", SrcBool(False)),
    ]),
  )
}

fn string_list_source(values: List(String)) -> SourceValue {
  SrcList(list.map(values, SrcString))
}

fn bool_source(value: root_field.ResolvedValue, fallback: Bool) -> SourceValue {
  case value {
    root_field.BoolVal(value) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

fn count_source(count: Int) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("count", SrcInt(count)),
      #("precision", SrcString("EXACT")),
    ]),
  )
}

fn code_connection_for_record(
  identity: SyntheticIdentityRegistry,
  code: Option(String),
  existing: Option(DiscountRecord),
) -> #(SourceValue, SyntheticIdentityRegistry) {
  case code {
    Some(code) -> {
      let #(redeem_id, next_identity) = case
        existing |> option.then(existing_redeem_code_id)
      {
        Some(id) -> #(id, identity)
        None ->
          synthetic_identity.make_proxy_synthetic_gid(
            identity,
            "DiscountRedeemCode",
          )
      }
      #(codes_connection_with_id(code, redeem_id), next_identity)
    }
    None -> #(empty_codes_connection(), identity)
  }
}

fn existing_redeem_code_id(record: DiscountRecord) -> Option(String) {
  case discount_owner_source(record) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList([SrcObject(first), ..])) ->
                  case dict.get(first, "id") {
                    Ok(SrcString(id)) -> Some(id)
                    _ -> None
                  }
                _ -> None
              }
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

fn codes_connection_with_id(code: String, id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList([
          SrcObject(
            dict.from_list([
              #("id", SrcString(id)),
              #("code", SrcString(code)),
              #("asyncUsageCount", SrcInt(0)),
            ]),
          ),
        ]),
      ),
      #("edges", SrcList([])),
      #(
        "pageInfo",
        SrcObject(
          dict.from_list([
            #("hasNextPage", SrcBool(False)),
            #("hasPreviousPage", SrcBool(False)),
            #("startCursor", SrcNull),
            #("endCursor", SrcNull),
          ]),
        ),
      ),
    ]),
  )
}

fn empty_codes_connection() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("nodes", SrcList([])),
      #("edges", SrcList([])),
      #(
        "pageInfo",
        SrcObject(
          dict.from_list([
            #("hasNextPage", SrcBool(False)),
            #("hasPreviousPage", SrcBool(False)),
            #("startCursor", SrcNull),
            #("endCursor", SrcNull),
          ]),
        ),
      ),
    ]),
  )
}

fn context_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountBuyerSelectionAll")),
              #("all", SrcString("ALL")),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "customers") {
            Ok(root_field.ObjectVal(customers)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCustomers")),
                  #(
                    "customers",
                    SrcList(
                      read_string_array(customers, "add", [])
                      |> list.map(customer_context_node),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "customerSegments") {
                Ok(root_field.ObjectVal(segments)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCustomerSegments")),
                      #(
                        "segments",
                        SrcList(
                          read_string_array(segments, "add", [])
                          |> list.map(customer_segment_context_node),
                        ),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ ->
      SrcObject(
        dict.from_list([
          #("__typename", SrcString("DiscountBuyerSelectionAll")),
          #("all", SrcString("ALL")),
        ]),
      )
  }
}

fn customer_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Customer")),
      #("id", SrcString(id)),
      #("displayName", SrcString(customer_display_name(id))),
    ]),
  )
}

fn customer_display_name(id: String) -> String {
  case id {
    "gid://shopify/Customer/10548596015410" -> "HAR390 Buyer Context"
    _ -> ""
  }
}

fn customer_segment_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Segment")),
      #("id", SrcString(id)),
      #("name", SrcString(customer_segment_name(id))),
    ]),
  )
}

fn customer_segment_name(id: String) -> String {
  case id {
    "gid://shopify/Segment/647746715954" ->
      "HAR-390 buyer context 1777346878525"
    _ -> ""
  }
}

fn minimum_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "subtotal") {
        Ok(root_field.ObjectVal(subtotal)) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountMinimumSubtotal")),
              #(
                "greaterThanOrEqualToSubtotal",
                money_source(read_value(
                  subtotal,
                  "greaterThanOrEqualToSubtotal",
                )),
              ),
            ]),
          )
        _ ->
          case dict.get(fields, "quantity") {
            Ok(root_field.ObjectVal(quantity)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountMinimumQuantity")),
                  #(
                    "greaterThanOrEqualToQuantity",
                    resolved_to_source(read_value(
                      quantity,
                      "greaterThanOrEqualToQuantity",
                    )),
                  ),
                ]),
              )
            _ -> SrcNull
          }
      }
    _ -> SrcNull
  }
}

fn customer_gets_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
          #(
            "appliesOnOneTimePurchase",
            bool_source(read_value(fields, "appliesOnOneTimePurchase"), True),
          ),
          #(
            "appliesOnSubscription",
            bool_source(read_value(fields, "appliesOnSubscription"), False),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

fn customer_buys_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
        ]),
      )
    _ -> SrcNull
  }
}

fn discount_value_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "percentage") {
        Ok(percentage) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountPercentage")),
              #("percentage", resolved_to_source(percentage)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "discountAmount") {
            Ok(root_field.ObjectVal(amount)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountAmount")),
                  #("amount", money_source(read_value(amount, "amount"))),
                  #(
                    "appliesOnEachItem",
                    bool_source(read_value(amount, "appliesOnEachItem"), False),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "quantity") {
                Ok(quantity) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountQuantity")),
                      #("quantity", resolved_to_source(quantity)),
                    ]),
                  )
                Error(_) ->
                  case discount_on_quantity_value(fields) {
                    Ok(root_field.ObjectVal(on_quantity)) ->
                      SrcObject(
                        dict.from_list([
                          #("__typename", SrcString("DiscountOnQuantity")),
                          #(
                            "quantity",
                            SrcObject(
                              dict.from_list([
                                #(
                                  "quantity",
                                  resolved_to_source(read_value(
                                    on_quantity,
                                    "quantity",
                                  )),
                                ),
                              ]),
                            ),
                          ),
                          #(
                            "effect",
                            discount_value_source(read_value(
                              on_quantity,
                              "effect",
                            )),
                          ),
                        ]),
                      )
                    _ -> resolved_to_source(value)
                  }
              }
          }
      }
    _ -> SrcNull
  }
}

fn discount_on_quantity_value(
  fields: Dict(String, root_field.ResolvedValue),
) -> Result(root_field.ResolvedValue, Nil) {
  case dict.get(fields, "discountOnQuantity") {
    Ok(value) -> Ok(value)
    Error(_) ->
      case dict.get(fields, "onQuantity") {
        Ok(value) -> Ok(value)
        Error(_) -> Error(Nil)
      }
  }
}

fn discount_items_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("AllDiscountItems")),
              #("allItems", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "products") {
            Ok(root_field.ObjectVal(products)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountProducts")),
                  #(
                    "products",
                    id_connection(
                      read_string_array(products, "productsToAdd", []),
                    ),
                  ),
                  #(
                    "productVariants",
                    id_connection(
                      read_string_array(products, "productVariantsToAdd", []),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "collections") {
                Ok(root_field.ObjectVal(collections)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCollections")),
                      #(
                        "collections",
                        id_connection(read_string_array(collections, "add", [])),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ -> SrcNull
  }
}

fn id_connection(ids: List(String)) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList(
          list.map(ids, fn(id) {
            SrcObject(dict.from_list([#("id", SrcString(id))]))
          }),
        ),
      ),
    ]),
  )
}

fn destination_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountCountryAll")),
              #("allCountries", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "countries") {
            Ok(root_field.ObjectVal(countries)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCountries")),
                  #(
                    "countries",
                    string_list_source(
                      read_string_array(countries, "add", [])
                      |> list.sort(string.compare),
                    ),
                  ),
                  #(
                    "includeRestOfWorld",
                    resolved_bool_or_default(
                      countries,
                      "includeRestOfWorld",
                      False,
                    ),
                  ),
                ]),
              )
            _ -> resolved_to_source(value)
          }
      }
    _ -> SrcNull
  }
}

fn resolved_bool_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

fn money_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.StringVal(amount) ->
      SrcObject(
        dict.from_list([
          #("amount", SrcString(normalize_money(amount))),
          #("currencyCode", SrcString("CAD")),
        ]),
      )
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("amount", resolved_to_source(read_value(fields, "amount"))),
          #(
            "currencyCode",
            resolved_to_source(read_value(fields, "currencyCode")),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

fn normalize_money(value: String) -> String {
  case string.split(value, ".") {
    [whole, decimals] -> {
      let trimmed = trim_trailing_zeroes(decimals)
      case trimmed {
        "" -> whole <> ".0"
        _ -> whole <> "." <> trimmed
      }
    }
    _ -> value
  }
}

fn trim_trailing_zeroes(value: String) -> String {
  case string.ends_with(value, "0") {
    True -> trim_trailing_zeroes(string.drop_end(value, 1))
    False -> value
  }
}

fn app_discount_type_source(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> SourceValue {
  let function_reference =
    read_string(input, "functionId")
    |> option.or(read_string(input, "functionHandle"))
  let shopify_function =
    function_reference
    |> option.then(fn(reference) { find_shopify_function(store, reference) })
  SrcObject(
    dict.from_list([
      #(
        "appKey",
        shopify_function
          |> option.then(fn(record) { record.app_key })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "functionId",
        function_reference |> option.map(SrcString) |> option.unwrap(SrcNull),
      ),
      #(
        "title",
        shopify_function
          |> option.then(fn(record) { record.title })
          |> option.or(read_string(input, "title"))
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "description",
        shopify_function
          |> option.then(fn(record) { record.description })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
    ]),
  )
}

fn find_shopify_function(
  store: Store,
  reference: String,
) -> Option(ShopifyFunctionRecord) {
  store.get_effective_shopify_function_by_id(store, reference)
  |> option.lazy_or(fn() {
    store.list_effective_shopify_functions(store)
    |> list.find(fn(record) {
      record.handle == Some(reference) || record.id == reference
    })
    |> option.from_result
  })
}

fn update_payload_status(
  payload: CapturedJsonValue,
  status: String,
  transition_timestamp: Option(String),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount", "automaticDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let discount = dict.insert(discount, "status", SrcString(status))
              let discount = case status, transition_timestamp {
                "ACTIVE", Some(timestamp) ->
                  activate_discount_dates(discount, timestamp)
                "ACTIVE", None -> discount
                "EXPIRED", Some(timestamp) ->
                  expire_discount_dates(discount, timestamp)
                _, _ -> discount
              }
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

fn bump_discount_updated_at(
  record: DiscountRecord,
  timestamp: String,
) -> DiscountRecord {
  DiscountRecord(
    ..record,
    payload: update_payload_updated_at(record.payload, timestamp),
  )
}

fn update_payload_updated_at(
  payload: CapturedJsonValue,
  timestamp: String,
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount", "automaticDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) ->
              dict.insert(
                acc,
                key,
                SrcObject(dict.insert(
                  discount,
                  "updatedAt",
                  SrcString(timestamp),
                )),
              )
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

fn activate_discount_dates(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Dict(String, SourceValue) {
  let discount = case should_clear_ends_at(discount, timestamp) {
    True -> dict.insert(discount, "endsAt", SrcNull)
    False -> discount
  }
  case should_bump_starts_at(discount, timestamp) {
    True -> dict.insert(discount, "startsAt", SrcString(timestamp))
    False -> discount
  }
}

fn expire_discount_dates(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Dict(String, SourceValue) {
  let discount = dict.insert(discount, "endsAt", SrcString(timestamp))
  case should_bump_starts_at(discount, timestamp) {
    True -> dict.insert(discount, "startsAt", SrcString(timestamp))
    False -> discount
  }
}

fn should_clear_ends_at(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Bool {
  case dict.get(discount, "endsAt") {
    Ok(SrcString(value)) -> iso_timestamp_before(value, timestamp)
    Ok(SrcNull) | Error(_) -> True
    _ -> False
  }
}

fn should_bump_starts_at(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Bool {
  case dict.get(discount, "startsAt") {
    Ok(SrcString(value)) -> iso_timestamp_after(value, timestamp)
    Ok(SrcNull) | Error(_) -> True
    _ -> False
  }
}

fn iso_timestamp_before(value: String, timestamp: String) -> Bool {
  case iso_timestamp.parse_iso(value), iso_timestamp.parse_iso(timestamp) {
    Ok(value_ms), Ok(timestamp_ms) -> value_ms < timestamp_ms
    _, _ -> False
  }
}

fn iso_timestamp_after(value: String, timestamp: String) -> Bool {
  case iso_timestamp.parse_iso(value), iso_timestamp.parse_iso(timestamp) {
    Ok(value_ms), Ok(timestamp_ms) -> value_ms > timestamp_ms
    _, _ -> False
  }
}

fn append_codes(
  store: Store,
  record: DiscountRecord,
  codes: List(String),
  identity: SyntheticIdentityRegistry,
) -> #(DiscountRecord, SyntheticIdentityRegistry, List(#(String, String))) {
  case codes {
    [] -> #(record, identity, [])
    [first, ..] -> {
      let existing_nodes = existing_code_nodes(record)
      let existing_codes = list.map(existing_nodes, fn(pair) { pair.1 })
      let #(new_nodes, next_identity) =
        codes
        |> list.filter(fn(code) { !list.contains(existing_codes, code) })
        |> list.fold(#([], identity), fn(acc, code) {
          let #(nodes, current_identity) = acc
          let #(id, next_identity) =
            make_discount_async_gid(
              store,
              current_identity,
              "DiscountRedeemCode",
            )
          #([#(id, code), ..nodes], next_identity)
        })
      let nodes = list.append(existing_nodes, list.reverse(new_nodes))
      #(
        DiscountRecord(
          ..record,
          code: Some(first),
          payload: update_payload_codes(record.payload, nodes),
        ),
        next_identity,
        list.reverse(new_nodes),
      )
    }
  }
}

fn remove_codes_by_ids(
  record: DiscountRecord,
  ids: List(String),
  updated_at: String,
) -> DiscountRecord {
  let remaining_codes =
    existing_code_nodes(record)
    |> list.filter(fn(pair) {
      let #(id, _code) = pair
      !list.contains(ids, id)
    })
  let remaining_code_values = list.map(remaining_codes, fn(pair) { pair.1 })
  DiscountRecord(
    ..record,
    code: case remaining_code_values {
      [first, ..] -> Some(first)
      [] -> None
    },
    payload: update_payload_codes(record.payload, remaining_codes)
      |> update_payload_updated_at(updated_at),
  )
}

fn existing_code_nodes(record: DiscountRecord) -> List(#(String, String)) {
  case discount_owner_source(record) {
    SrcObject(fields) ->
      case dict.get(fields, "codeDiscount") {
        Ok(SrcObject(discount)) ->
          case dict.get(discount, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList(nodes)) -> list.filter_map(nodes, read_code_node)
                _ -> []
              }
            _ -> []
          }
        _ -> []
      }
    _ -> []
  }
}

fn read_code_node(node: SourceValue) -> Result(#(String, String), Nil) {
  case node {
    SrcObject(fields) ->
      case dict.get(fields, "id"), dict.get(fields, "code") {
        Ok(SrcString(id)), Ok(SrcString(code)) -> Ok(#(id, code))
        _, _ -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

fn update_payload_codes(
  payload: CapturedJsonValue,
  codes: List(#(String, String)),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let nodes =
                codes
                |> list.map(fn(pair) {
                  let #(id, code) = pair
                  SrcObject(
                    dict.from_list([
                      #("id", SrcString(id)),
                      #("code", SrcString(code)),
                      #("asyncUsageCount", SrcInt(0)),
                    ]),
                  )
                })
              let discount =
                discount
                |> dict.insert(
                  "codes",
                  SrcObject(
                    dict.from_list([
                      #("nodes", SrcList(nodes)),
                      #("edges", SrcList([])),
                      #(
                        "pageInfo",
                        SrcObject(
                          dict.from_list([
                            #("hasNextPage", SrcBool(False)),
                            #("hasPreviousPage", SrcBool(False)),
                            #("startCursor", SrcNull),
                            #("endCursor", SrcNull),
                          ]),
                        ),
                      ),
                    ]),
                  ),
                )
                |> dict.insert("codesCount", count_source(list.length(codes)))
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

fn find_effective_discount_by_code(
  store: Store,
  code: String,
) -> Option(DiscountRecord) {
  find_effective_discount_by_code_ignoring(store, code, None)
}

fn find_effective_discount_by_code_ignoring(
  store: Store,
  code: String,
  ignored_discount_id: Option(String),
) -> Option(DiscountRecord) {
  let wanted = string.lowercase(code)
  case
    list.find(store.list_effective_discounts(store), fn(record) {
      let ignored = case ignored_discount_id {
        Some(id) -> record.id == id
        None -> False
      }
      !ignored && discount_record_has_code(record, wanted)
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

fn discount_record_has_code(record: DiscountRecord, wanted: String) -> Bool {
  case record.code {
    Some(record_code) -> string.lowercase(record_code) == wanted
    None -> False
  }
  || {
    existing_code_nodes(record)
    |> list.any(fn(pair) { string.lowercase(pair.1) == wanted })
  }
}

fn make_discount_async_gid(
  store: Store,
  identity: SyntheticIdentityRegistry,
  resource_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  // The TS parity harness handles Discounts before route-level mutation-log
  // recording. The Gleam dispatcher records those logs centrally, so discount
  // async payload IDs need to subtract the extra log-entry IDs while still
  // advancing the real registry for state dumps and later logs.
  let log_adjustment = int.max(0, list.length(store.get_log(store)) - 2)
  let visible_id = identity.next_synthetic_id - log_adjustment
  let gid =
    "gid://shopify/"
    <> resource_type
    <> "/"
    <> int.to_string(visible_id)
    <> "?shopify-draft-proxy=synthetic"
  let #(_, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(identity, resource_type)
  #(gid, next_identity)
}

fn captured_to_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_to_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_to_source(item))
        })
        |> dict.from_list,
      )
  }
}

fn source_to_captured(value: SourceValue) -> CapturedJsonValue {
  case value {
    SrcNull -> CapturedNull
    SrcBool(value) -> CapturedBool(value)
    SrcInt(value) -> CapturedInt(value)
    SrcFloat(value) -> CapturedFloat(value)
    SrcString(value) -> CapturedString(value)
    SrcList(items) -> CapturedArray(list.map(items, source_to_captured))
    SrcObject(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, source_to_captured(item))
        }),
      )
  }
}

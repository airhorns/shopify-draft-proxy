//// Discounts query handling, live-hybrid passthrough decisions, and local projection.

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
import shopify_draft_proxy/proxy/discounts/types as discount_types
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

@internal
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

@internal
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
@internal
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

@internal
pub fn local_has_discount_bulk_creation_id(
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

@internal
pub fn get_effective_discount_bulk_operation(
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
@internal
pub fn should_passthrough_in_live_hybrid(
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
@internal
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

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_discount_query(store, document, variables))
  Ok(json.object([#("data", data)]))
}

@internal
pub fn handle_discount_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
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

@internal
pub fn root_query_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "discountNode" -> {
          let id = discount_types.read_string_arg(field, variables, "id")
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
          let id = discount_types.read_string_arg(field, variables, "id")
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
              discount_types.discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "codeDiscountNodeByCode" -> {
          let code = discount_types.read_string_arg(field, variables, "code")
          code
          |> option.then(fn(code) {
            discount_types.find_effective_discount_by_code(store, code)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_types.discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "discountRedeemCodeBulkCreation" -> {
          let id = discount_types.read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            get_effective_discount_bulk_operation(store, id)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_types.captured_to_source(record.payload),
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

@internal
pub type ConnectionMode {
  DiscountNodeConnection
  OwnerNodeConnection
}

@internal
pub fn serialize_discount_connection(
  records: List(DiscountRecord),
  field: Selection,
  fragments: FragmentMap,
  mode: ConnectionMode,
) -> Json {
  let first =
    discount_types.read_int_arg(field, dict.new(), "first") |> option.unwrap(50)
  let records = list.take(records, int.max(0, first))
  let node_for = fn(record: DiscountRecord) {
    case mode {
      DiscountNodeConnection -> discount_node_source(record)
      OwnerNodeConnection -> discount_types.discount_owner_source(record)
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

@internal
pub fn serialize_page_info(
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

@internal
pub fn serialize_count(count: Int, field: Selection) -> Json {
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

@internal
pub fn filter_discounts(
  records: List(DiscountRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DiscountRecord) {
  search_query_parser.apply_search_query(
    records,
    discount_types.read_string_arg(field, variables, "query"),
    search_query_parser.default_parse_options(),
    discount_matches_positive_search_term,
  )
}

@internal
pub fn discount_matches_positive_search_term(
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
      string.lowercase(discount_types.discount_class_for_record(record))
      == value
    _ -> True
  }
}

@internal
pub fn discount_matches_type_filter(
  record: DiscountRecord,
  value: String,
) -> Bool {
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

@internal
pub fn sort_discounts(
  records: List(DiscountRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DiscountRecord) {
  let reverse =
    discount_types.read_bool_arg(field, variables, "reverse")
    |> option.unwrap(False)
  case discount_types.read_string_arg(field, variables, "sortKey") {
    Some("CREATED_AT") ->
      sort_discounts_by_timestamp(records, "createdAt", reverse)
    Some("UPDATED_AT") ->
      sort_discounts_by_timestamp(records, "updatedAt", reverse)
    _ -> records
  }
}

@internal
pub fn sort_discounts_by_timestamp(
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

@internal
pub fn compare_discount_timestamp(
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

@internal
pub fn reverse_order(value: order.Order) -> order.Order {
  case value {
    order.Lt -> order.Gt
    order.Gt -> order.Lt
    order.Eq -> order.Eq
  }
}

@internal
pub fn discount_owner_source(record: DiscountRecord) -> SourceValue {
  discount_types.captured_to_source(record.payload)
}

@internal
pub fn discount_record_timestamp(
  record: DiscountRecord,
  field: String,
) -> Option(String) {
  case discount_types.discount_owner_source(record) {
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

@internal
pub fn discount_node_source(record: DiscountRecord) -> SourceValue {
  case discount_types.captured_to_source(record.payload) {
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

@internal
pub fn child_fields(field: Selection) -> List(Selection) {
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

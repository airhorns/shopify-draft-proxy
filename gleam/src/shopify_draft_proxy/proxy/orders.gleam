//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type ObjectField, type Selection, Field, NullValue, ObjectField, ObjectValue,
  SelectionSet, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type DraftOrderRecord, type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductRecord, type ProductVariantRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, DraftOrderRecord,
  OrderRecord,
}

pub type OrdersError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

pub fn is_orders_query_root(name: String) -> Bool {
  list.contains(
    [
      "abandonedCheckouts",
      "abandonedCheckoutsCount",
      "abandonment",
      "abandonmentByAbandonedCheckoutId",
      "draftOrder",
      "draftOrderAvailableDeliveryOptions",
      "draftOrders",
      "draftOrdersCount",
      "order",
      "orders",
      "ordersCount",
      "return",
    ],
    name,
  )
}

pub fn is_orders_mutation_root(name: String) -> Bool {
  list.contains(
    [
      "abandonmentUpdateActivitiesDeliveryStatuses",
      "draftOrderComplete",
      "draftOrderCreate",
      "draftOrderCreateFromOrder",
      "draftOrderDelete",
      "draftOrderDuplicate",
      "draftOrderBulkAddTags",
      "draftOrderBulkDelete",
      "draftOrderBulkRemoveTags",
      "draftOrderCalculate",
      "draftOrderInvoicePreview",
      "draftOrderInvoiceSend",
      "draftOrderUpdate",
      "fulfillmentCancel",
      "fulfillmentCreate",
      "fulfillmentTrackingInfoUpdate",
      "orderCancel",
      "orderCapture",
      "orderClose",
      "orderCreate",
      "orderCreateMandatePayment",
      "orderCreateManualPayment",
      "orderEditAddCustomItem",
      "orderEditAddLineItemDiscount",
      "orderEditAddShippingLine",
      "orderEditAddVariant",
      "orderEditBegin",
      "orderEditCommit",
      "orderEditRemoveDiscount",
      "orderEditRemoveShippingLine",
      "orderEditSetQuantity",
      "orderEditUpdateShippingLine",
      "orderInvoiceSend",
      "orderMarkAsPaid",
      "orderOpen",
      "orderUpdate",
      "refundCreate",
      "removeFromReturn",
      "returnDeclineRequest",
      "returnCancel",
      "returnClose",
      "returnCreate",
      "returnReopen",
      "returnRequest",
      "taxSummaryCreate",
      "transactionVoid",
    ],
    name,
  )
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OrdersError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let entries =
    list.filter_map(fields, fn(field) {
      case field {
        Field(name: name, ..) ->
          Ok(#(
            get_field_response_key(field),
            serialize_query_field(
              store,
              field,
              name.value,
              fragments,
              variables,
            ),
          ))
        _ -> Error(Nil)
      }
    })
  let search_extensions = draft_order_search_extensions(fields, variables)
  Ok(wrap_query_payload(json.object(entries), search_extensions))
}

fn wrap_query_payload(data: Json, search_extensions: List(Json)) -> Json {
  case search_extensions {
    [] -> json.object([#("data", data)])
    [_, ..] ->
      json.object([
        #("data", data),
        #(
          "extensions",
          json.object([
            #(
              "search",
              json.array(search_extensions, fn(extension) { extension }),
            ),
          ]),
        ),
      ])
  }
}

fn draft_order_search_extensions(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  fields
  |> list.filter_map(fn(field) {
    case field {
      Field(name: name, ..) ->
        case name.value {
          "draftOrders" | "draftOrdersCount" ->
            build_draft_order_search_extension(
              read_string_argument(field, "query", variables),
              get_field_response_key(field),
            )
          _ -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn build_draft_order_search_extension(
  query: Option(String),
  response_key: String,
) -> Result(Json, Nil) {
  use raw <- result.try(option_to_result(query))
  let trimmed = string.trim(raw)
  case string.split_once(trimmed, ":") {
    Ok(#(raw_field, raw_value)) -> {
      let field = raw_field |> string.trim |> string.lowercase
      let match_all = string.trim(raw_value)
      case field == "email" && match_all != "" {
        True ->
          Ok(
            json.object([
              #("path", json.array([response_key], json.string)),
              #("query", json.string(trimmed)),
              #(
                "parsed",
                json.object([
                  #("field", json.string(field)),
                  #("match_all", json.string(match_all)),
                ]),
              ),
              #(
                "warnings",
                json.array([field], fn(warning_field) {
                  json.object([
                    #("field", json.string(warning_field)),
                    #(
                      "message",
                      json.string("Invalid search field for this query."),
                    ),
                    #("code", json.string("invalid_field")),
                  ])
                }),
              ),
            ]),
          )
        False -> Error(Nil)
      }
    }
    Error(_) -> Error(Nil)
  }
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

fn serialize_query_field(
  store: Store,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case name {
    "abandonedCheckouts" ->
      serialize_abandoned_checkouts(store, field, fragments, variables)
    "abandonedCheckoutsCount" ->
      serialize_abandoned_checkouts_count(store, field, variables)
    "abandonment" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_abandonment_by_id(store, id) {
            Some(abandonment) ->
              serialize_abandonment_node(store, field, abandonment, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "abandonmentByAbandonedCheckoutId" -> {
      let id = read_string_argument(field, "abandonedCheckoutId", variables)
      case id {
        Some(id) ->
          case store.get_abandonment_by_abandoned_checkout_id(store, id) {
            Some(abandonment) ->
              serialize_abandonment_node(store, field, abandonment, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "draftOrder" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_draft_order_by_id(store, id) {
            Some(draft_order) ->
              serialize_draft_order_node(field, draft_order, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "draftOrderAvailableDeliveryOptions" ->
      serialize_draft_order_available_delivery_options(field, fragments)
    "draftOrders" -> serialize_draft_orders(store, field, fragments, variables)
    "draftOrdersCount" -> serialize_draft_orders_count(store, field, fragments)
    "order" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_order_by_id(store, id) {
            Some(order) -> serialize_order_node(field, order, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "return" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case find_order_return(store, id) {
            Some(match) -> {
              let #(order, order_return) = match
              serialize_order_return(field, order_return, order, fragments)
            }
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "orders" -> serialize_orders(store, field, fragments, variables)
    "ordersCount" -> serialize_orders_count(store, field, fragments, variables)
    _ -> json.null()
  }
}

fn serialize_order_node(
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(order.data)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "returns" -> #(
              key,
              serialize_order_returns_connection(
                child,
                order_returns(order.data),
                order,
                fragments,
              ),
            )
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_orders(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_arguments(field, variables)
  let orders =
    store.list_effective_orders(store)
    |> filter_orders_by_query(read_string(args, "query"))
  let ordered = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(False)) -> list.reverse(orders)
    _ -> orders
  }
  let window =
    paginate_connection_items(
      ordered,
      field,
      variables,
      order_cursor,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      include_inline_fragments: True,
      prefix_cursors: False,
      include_cursors: True,
      fallback_start_cursor: None,
      fallback_end_cursor: None,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: order_cursor,
      serialize_node: fn(order, selection, _index) {
        serialize_order_connection_node(selection, order, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: page_info_options,
    ),
  )
}

fn serialize_order_connection_node(
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(order.data)
  case source {
    SrcObject(fields) ->
      json.object(
        selection_children(field)
        |> list.filter_map(fn(selection) {
          case selection {
            Field(name: name, ..) -> {
              let field_name = name.value
              case
                field_name == "__typename" || dict.has_key(fields, field_name)
              {
                True ->
                  Ok(#(
                    get_field_response_key(selection),
                    project_graphql_field_value(source, selection, fragments),
                  ))
                False -> Error(Nil)
              }
            }
            _ -> Error(Nil)
          }
        }),
      )
    _ -> project_graphql_value(source, selection_children(field), fragments)
  }
}

fn order_cursor(order: OrderRecord, _index: Int) -> String {
  order.cursor |> option.unwrap(order.id)
}

fn filter_orders_by_query(
  orders: List(OrderRecord),
  raw_query: Option(String),
) -> List(OrderRecord) {
  search_query_parser.apply_search_query(
    orders,
    raw_query,
    search_query_parser.default_parse_options(),
    order_matches_search_term,
  )
}

fn order_matches_search_term(
  order: OrderRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case term.field {
    None ->
      search_query_parser.matches_search_query_text(
        captured_string_field(order.data, "id"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        captured_string_field(order.data, "name"),
        term,
      )
      || search_query_parser.matches_search_query_text(
        captured_string_field(order.data, "email"),
        term,
      )
      || list.any(order_tags(order.data), fn(tag) {
        search_query_parser.matches_search_query_text(Some(tag), term)
      })
    Some("tag") ->
      list.any(order_tags(order.data), fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          term.value,
          search_query_parser.ExactMatch,
          search_query_parser.default_string_match_options(),
        )
      })
    Some("name") ->
      search_query_parser.matches_search_query_text(
        captured_string_field(order.data, "name"),
        term,
      )
    Some("financial_status") ->
      search_query_parser.matches_search_query_string(
        captured_string_field(order.data, "displayFinancialStatus"),
        term.value,
        search_query_parser.ExactMatch,
        search_query_parser.default_string_match_options(),
      )
    Some("fulfillment_status") ->
      search_query_parser.matches_search_query_string(
        captured_string_field(order.data, "displayFulfillmentStatus"),
        term.value,
        search_query_parser.ExactMatch,
        search_query_parser.default_string_match_options(),
      )
    _ -> False
  }
}

fn order_tags(data: CapturedJsonValue) -> List(String) {
  case captured_object_field(data, "tags") {
    Some(CapturedArray(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          CapturedString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn serialize_orders_count(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_arguments(field, variables)
  let orders =
    store.list_effective_orders(store)
    |> filter_orders_by_query(read_string(args, "query"))
  let count = list.length(orders)
  let limit = read_int_argument(field, "limit", variables)
  let #(visible_count, precision) = case limit {
    Some(limit) if limit >= 0 && count > limit -> #(limit, "AT_LEAST")
    _ -> #(count, "EXACT")
  }
  let source =
    src_object([
      #("count", SrcInt(visible_count)),
      #("precision", SrcString(precision)),
    ])
  project_graphql_value(source, selection_children(field), fragments)
}

fn serialize_draft_orders(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let draft_orders = store.list_effective_draft_orders(store)
  let args = field_arguments(field, variables)
  let ordered = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(False)) -> list.reverse(draft_orders)
    _ -> draft_orders
  }
  let window =
    paginate_connection_items(
      ordered,
      field,
      variables,
      draft_order_cursor,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      include_inline_fragments: True,
      prefix_cursors: False,
      include_cursors: True,
      fallback_start_cursor: None,
      fallback_end_cursor: None,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: draft_order_cursor,
      serialize_node: fn(draft_order, selection, _index) {
        serialize_draft_order_node(selection, draft_order, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: page_info_options,
    ),
  )
}

fn draft_order_cursor(draft_order: DraftOrderRecord, _index: Int) -> String {
  draft_order.cursor |> option.unwrap(draft_order.id)
}

fn serialize_draft_orders_count(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("count", SrcInt(list.length(store.list_effective_draft_orders(store)))),
      #("precision", SrcString("EXACT")),
    ])
  project_graphql_value(source, selection_children(field), fragments)
}

fn serialize_draft_order_available_delivery_options(
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("availableShippingRates", SrcList([])),
      #("availableLocalDeliveryRates", SrcList([])),
      #("availableLocalPickupOptions", SrcList([])),
      #(
        "pageInfo",
        src_object([
          #("hasNextPage", SrcBool(False)),
          #("hasPreviousPage", SrcBool(False)),
          #("startCursor", SrcNull),
          #("endCursor", SrcNull),
        ]),
      ),
    ])
  project_graphql_value(source, selection_children(field), fragments)
}

fn serialize_abandoned_checkouts(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let checkouts = store.list_effective_abandoned_checkouts(store)
  let args = field_arguments(field, variables)
  let ordered = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(False)) -> list.reverse(checkouts)
    _ -> checkouts
  }
  let window =
    paginate_connection_items(
      ordered,
      field,
      variables,
      abandoned_checkout_cursor,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      include_inline_fragments: True,
      prefix_cursors: False,
      include_cursors: True,
      fallback_start_cursor: None,
      fallback_end_cursor: None,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: abandoned_checkout_cursor,
      serialize_node: fn(checkout, selection, _index) {
        serialize_abandoned_checkout_node(selection, checkout, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: page_info_options,
    ),
  )
}

fn abandoned_checkout_cursor(
  checkout: AbandonedCheckoutRecord,
  _index: Int,
) -> String {
  checkout.cursor |> option.unwrap(checkout.id)
}

fn serialize_abandoned_checkouts_count(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let checkouts = store.list_effective_abandoned_checkouts(store)
  let raw_count = list.length(checkouts)
  let limit = read_int_argument(field, "limit", variables)
  let count = case limit {
    Some(limit) if limit >= 0 -> min_int(raw_count, limit)
    _ -> raw_count
  }
  let precision = case limit {
    Some(limit) if limit >= 0 && raw_count > limit -> "AT_LEAST"
    _ -> "EXACT"
  }
  serialize_count_payload(field, count, precision)
}

fn serialize_count_payload(
  field: Selection,
  count: Int,
  precision: String,
) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(count))
              "precision" -> #(key, json.string(precision))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_abandoned_checkout_node(
  field: Selection,
  checkout: AbandonedCheckoutRecord,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    captured_json_source(checkout.data),
    selection_children(field),
    fragments,
  )
}

fn serialize_abandonment_node(
  store: Store,
  field: Selection,
  abandonment: AbandonmentRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(abandonment.data)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "abandonedCheckoutPayload" -> #(
              key,
              serialize_abandoned_checkout_payload(
                store,
                child,
                abandonment,
                fragments,
              ),
            )
            _ -> #(key, graphql_helpers_project_field(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_abandoned_checkout_payload(
  store: Store,
  field: Selection,
  abandonment: AbandonmentRecord,
  fragments: FragmentMap,
) -> Json {
  case abandonment.abandoned_checkout_id {
    Some(checkout_id) ->
      case store.get_abandoned_checkout_by_id(store, checkout_id) {
        Some(checkout) ->
          serialize_abandoned_checkout_node(field, checkout, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn graphql_helpers_project_field(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_field_value(source, field, fragments)
}

fn serialize_draft_order_node(
  field: Selection,
  draft_order: DraftOrderRecord,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    captured_json_source(draft_order.data),
    selection_children(field),
    fragments,
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, OrdersError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let operation_path = get_operation_path_label(document)
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    staged_ids,
    log_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errors, current_store, current_identity, ids, drafts) = acc
      case field {
        Field(name: name, ..)
          if name.value == "abandonmentUpdateActivitiesDeliveryStatuses"
        -> {
          let result =
            handle_abandonment_delivery_status(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderCreate" -> {
          let result =
            handle_draft_order_create(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderCreateFromOrder" -> {
          let result =
            handle_draft_order_create_from_order(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderComplete" -> {
          let result =
            handle_draft_order_complete(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderDelete" -> {
          let result =
            handle_draft_order_delete(
              current_store,
              document,
              operation_path,
              field,
              variables,
            )
          let #(key, payload, next_store, next_errors, next_drafts) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              current_identity,
              ids,
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderDuplicate" -> {
          let result =
            handle_draft_order_duplicate(
              current_store,
              current_identity,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_store, next_identity, next_ids, next_drafts) =
            result
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..) if name.value == "draftOrderCalculate" -> {
          let result =
            handle_draft_order_calculate(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_errors, next_drafts) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "draftOrderBulkAddTags"
          || name.value == "draftOrderBulkRemoveTags"
          || name.value == "draftOrderBulkDelete"
        -> {
          let result =
            handle_draft_order_bulk_helper(
              current_store,
              current_identity,
              name.value,
              field,
              variables,
            )
          let #(key, payload, next_store, next_identity, next_ids, next_drafts) =
            result
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..) if name.value == "draftOrderInvoicePreview" -> {
          let result =
            handle_draft_order_invoice_preview(
              current_store,
              document,
              operation_path,
              field,
              variables,
            )
          let #(key, payload, next_errors, next_drafts) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderInvoiceSend" -> {
          let result =
            handle_draft_order_invoice_send(
              current_store,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_errors, next_drafts) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "draftOrderUpdate" -> {
          let result =
            handle_draft_order_update(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "fulfillmentCancel"
          || name.value == "fulfillmentTrackingInfoUpdate"
        -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_fulfillment_mutation(
              name.value,
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "fulfillmentCreate" -> {
          let #(key, payload, next_errors) =
            handle_fulfillment_create_invalid_id_guardrail(name.value)
          #(
            list.append(entries, [#(key, payload)]),
            list.append(errors, next_errors),
            current_store,
            current_identity,
            ids,
            drafts,
          )
        }
        Field(name: name, ..) if name.value == "orderCreate" -> {
          let result =
            handle_order_create_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "orderClose" || name.value == "orderOpen"
        -> {
          let result =
            handle_order_lifecycle_mutation(
              current_store,
              current_identity,
              name.value,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderCancel" -> {
          let result =
            handle_order_cancel_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderCapture" -> {
          let result =
            handle_order_capture_mutation(
              current_store,
              current_identity,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_store, next_identity, next_ids, next_drafts) =
            result
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..) if name.value == "transactionVoid" -> {
          let result =
            handle_transaction_void_mutation(
              current_store,
              current_identity,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_store, next_identity, next_ids, next_drafts) =
            result
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..) if name.value == "orderCreateMandatePayment" -> {
          let result =
            handle_order_create_mandate_payment_mutation(
              current_store,
              current_identity,
              field,
              fragments,
              variables,
            )
          let #(key, payload, next_store, next_identity, next_ids, next_drafts) =
            result
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..) if name.value == "orderInvoiceSend" -> {
          let #(key, payload, next_errors) =
            handle_order_invoice_send(
              current_store,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderMarkAsPaid" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_mark_as_paid_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              list.append(drafts, next_drafts),
            )
          }
        }
        Field(name: name, ..) if name.value == "orderUpdate" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_update_mutation(
              current_store,
              current_identity,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "refundCreate" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_refund_create_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderEditBegin" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_edit_begin_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderEditAddVariant" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_edit_add_variant_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderEditSetQuantity" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_edit_set_quantity_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderEditCommit" -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_errors,
            next_drafts,
          ) =
            handle_order_edit_commit_mutation(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, staged_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "orderEditAddCustomItem"
          || name.value == "orderEditAddLineItemDiscount"
          || name.value == "orderEditRemoveDiscount"
          || name.value == "orderEditAddShippingLine"
          || name.value == "orderEditUpdateShippingLine"
          || name.value == "orderEditRemoveShippingLine"
        -> {
          let #(key, payload, next_store, next_identity) =
            handle_order_edit_residual_mutation(
              current_store,
              current_identity,
              name.value,
              field,
              fragments,
              variables,
            )
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            ids,
            drafts,
          )
        }
        Field(name: name, ..)
          if name.value == "returnCreate"
          || name.value == "returnRequest"
          || name.value == "returnCancel"
          || name.value == "returnClose"
          || name.value == "returnReopen"
          || name.value == "removeFromReturn"
          || name.value == "returnDeclineRequest"
        -> {
          let #(
            key,
            payload,
            next_store,
            next_identity,
            staged_ids,
            next_drafts,
          ) =
            handle_return_lifecycle_mutation(
              current_store,
              current_identity,
              name.value,
              field,
              fragments,
              variables,
            )
          #(
            list.append(entries, [#(key, payload)]),
            errors,
            next_store,
            next_identity,
            list.append(ids, staged_ids),
            list.append(drafts, next_drafts),
          )
        }
        Field(name: name, ..)
          if name.value == "orderCreateManualPayment"
          || name.value == "taxSummaryCreate"
        -> {
          let #(key, payload, next_errors, next_drafts) =
            handle_access_denied_guardrail(name.value, field)
          #(
            list.append(entries, [#(key, payload)]),
            list.append(errors, next_errors),
            current_store,
            current_identity,
            ids,
            list.append(drafts, next_drafts),
          )
        }
        _ -> acc
      }
    })
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(data_entries))])
    _ ->
      case data_entries {
        [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
        _ ->
          json.object([
            #("errors", json.preprocessed_array(all_errors)),
            #("data", json.object(data_entries)),
          ])
      }
  }
  Ok(MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: log_drafts,
  ))
}

fn handle_draft_order_complete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderComplete",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      case id {
        Some(id) ->
          case store.get_draft_order_by_id(store, id) {
            Some(draft_order) -> {
              case read_string_arg(args, "paymentGatewayId") {
                Some(_) -> {
                  let payload =
                    serialize_draft_order_mutation_payload(
                      field,
                      None,
                      [#([], "Invalid payment gateway")],
                      fragments,
                    )
                  #(key, payload, store, identity, [], [], [])
                }
                None -> {
                  let #(completed_draft_order, next_identity) =
                    complete_draft_order(
                      store,
                      identity,
                      draft_order,
                      read_string_arg(args, "sourceName"),
                      read_bool(args, "paymentPending", False),
                    )
                  let next_store =
                    store.stage_draft_order(store, completed_draft_order)
                  let payload =
                    serialize_draft_order_mutation_payload(
                      field,
                      Some(completed_draft_order),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "draftOrderComplete",
                      [completed_draft_order.id],
                      store.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged draftOrderComplete in shopify-draft-proxy.",
                      ),
                    )
                  #(
                    key,
                    payload,
                    next_store,
                    next_identity,
                    [completed_draft_order.id],
                    [],
                    [draft],
                  )
                }
              }
            }
            None -> {
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  None,
                  [#(["id"], "Draft order does not exist")],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
          }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn handle_draft_order_delete(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderDelete",
      [RequiredArgument(name: "input", expected_type: "DraftOrderDeleteInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, validation_errors, [])
    [] -> {
      let input = field_arguments(field, variables) |> read_object("input")
      let id = case input {
        Some(input) -> read_string_arg(input, "id")
        None -> None
      }
      case id {
        Some(id) ->
          case store.get_draft_order_by_id(store, id) {
            Some(_) -> {
              let next_store = store.delete_staged_draft_order(store, id)
              let payload =
                serialize_draft_order_delete_payload(field, Some(id), [])
              let draft =
                single_root_log_draft(
                  "draftOrderDelete",
                  [id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderDelete in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, [], [draft])
            }
            None -> #(
              key,
              serialize_draft_order_delete_payload(field, None, [
                #(["id"], "Draft order does not exist"),
              ]),
              store,
              [],
              [],
            )
          }
        None -> #(
          key,
          serialize_draft_order_delete_payload(field, None, [
            #(["id"], "Draft order does not exist"),
          ]),
          store,
          [],
          [],
        )
      }
    }
  }
}

fn serialize_draft_order_delete_payload(
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(#(List(String), String)),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "deletedId" -> #(key, case deleted_id {
              Some(id) -> json.string(id)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn handle_draft_order_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let id =
    read_string_arg(args, "id")
    |> option.or(read_string_arg(args, "draftOrderId"))
  case id {
    Some(id) ->
      case store.get_draft_order_by_id(store, id) {
        Some(draft_order) -> {
          let #(duplicated_draft_order, next_identity) =
            duplicate_draft_order(store, identity, draft_order)
          let next_store =
            store.stage_draft_order(store, duplicated_draft_order)
          let payload =
            serialize_draft_order_mutation_payload(
              field,
              Some(duplicated_draft_order),
              [],
              fragments,
            )
          let draft =
            single_root_log_draft(
              "draftOrderDuplicate",
              [duplicated_draft_order.id],
              store.Staged,
              "orders",
              "stage-locally",
              Some("Locally staged draftOrderDuplicate in shopify-draft-proxy."),
            )
          #(
            key,
            payload,
            next_store,
            next_identity,
            [duplicated_draft_order.id],
            [draft],
          )
        }
        None ->
          unknown_draft_order_duplicate_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      unknown_draft_order_duplicate_result(
        key,
        store,
        identity,
        field,
        fragments,
      )
  }
}

fn unknown_draft_order_duplicate_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let payload =
    serialize_draft_order_mutation_payload(
      field,
      None,
      [
        #(["id"], "Draft order does not exist"),
      ],
      fragments,
    )
  #(key, payload, store, identity, [], [])
}

fn handle_draft_order_invoice_send(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderInvoiceSend",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      let draft_order = case id {
        Some(id) -> store.get_draft_order_by_id(store, id)
        None -> None
      }
      let user_errors = invoice_send_user_errors(args, draft_order)
      let payload =
        serialize_draft_order_invoice_send_payload(
          field,
          draft_order,
          user_errors,
          fragments,
        )
      let draft =
        single_root_log_draft(
          "draftOrderInvoiceSend",
          [],
          case user_errors {
            [] -> store.Staged
            _ -> store.Failed
          },
          "orders",
          "stage-locally",
          Some("Locally handled draftOrderInvoiceSend safety validation."),
        )
      #(key, payload, [], [draft])
    }
  }
}

fn invoice_send_user_errors(
  args: Dict(String, root_field.ResolvedValue),
  draft_order: Option(DraftOrderRecord),
) -> List(#(Option(List(String)), String)) {
  case draft_order {
    None -> [#(None, "Draft order not found")]
    Some(record) -> {
      let recipient_errors = case invoice_send_recipient_present(args, record) {
        True -> []
        False -> [#(None, "To can't be blank")]
      }
      let status_errors = case captured_string_field(record.data, "status") {
        Some("COMPLETED") -> [
          #(
            None,
            "Draft order Invoice can't be sent. This draft order is already paid.",
          ),
        ]
        _ -> []
      }
      list.append(recipient_errors, status_errors)
    }
  }
}

fn invoice_send_recipient_present(
  args: Dict(String, root_field.ResolvedValue),
  draft_order: DraftOrderRecord,
) -> Bool {
  case read_object(args, "email") {
    Some(email) ->
      case read_string(email, "to") {
        Some("") -> False
        Some(_) -> True
        None -> False
      }
    None ->
      case captured_string_field(draft_order.data, "email") {
        Some("") -> False
        Some(_) -> True
        None -> False
      }
  }
}

fn serialize_draft_order_invoice_send_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(Option(List(String)), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_nullable_user_error(
  field: Selection,
  error: #(Option(List(String)), String),
) -> Json {
  let #(field_path, message) = error
  let source =
    src_object([
      #("field", case field_path {
        Some(path) -> SrcList(list.map(path, SrcString))
        None -> SrcNull
      }),
      #("message", SrcString(message)),
    ])
  project_graphql_value(source, selection_children(field), dict.new())
}

fn handle_draft_order_calculate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderCalculate",
      [RequiredArgument(name: "input", expected_type: "DraftOrderInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> {
          let #(draft_order, _) =
            build_draft_order_from_input(store, identity, input)
          let calculated = build_calculated_draft_order_from_draft(draft_order)
          let payload =
            serialize_draft_order_calculate_payload(
              field,
              Some(calculated),
              [],
              fragments,
            )
          let draft =
            single_root_log_draft(
              "draftOrderCalculate",
              [],
              store.Staged,
              "orders",
              "stage-locally",
              Some(
                "Locally calculated draftOrderCalculate in shopify-draft-proxy.",
              ),
            )
          #(key, payload, [], [draft])
        }
        _ -> #(key, json.null(), [], [])
      }
    }
  }
}

fn build_calculated_draft_order_from_draft(
  draft_order: DraftOrderRecord,
) -> CapturedJsonValue {
  let line_items = draft_order_line_items(draft_order.data)
  draft_order.data
  |> replace_captured_object_fields([
    #("currencyCode", CapturedString(captured_order_currency(draft_order.data))),
    #("lineItems", CapturedArray(line_items)),
    #("availableShippingRates", CapturedArray([])),
  ])
}

fn serialize_draft_order_calculate_payload(
  field: Selection,
  calculated: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedDraftOrder" -> #(key, case calculated {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn handle_draft_order_invoice_preview(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderInvoicePreview",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      let draft_order = case id {
        Some(id) -> store.get_draft_order_by_id(store, id)
        None -> None
      }
      let user_errors = case draft_order {
        Some(_) -> []
        None -> [#(["id"], "Draft order does not exist")]
      }
      let payload =
        serialize_draft_order_invoice_preview_payload(field, args, user_errors)
      let draft =
        single_root_log_draft(
          "draftOrderInvoicePreview",
          [],
          case user_errors {
            [] -> store.Staged
            _ -> store.Failed
          },
          "orders",
          "stage-locally",
          Some(
            "Locally handled draftOrderInvoicePreview in shopify-draft-proxy.",
          ),
        )
      #(key, payload, [], [draft])
    }
  }
}

fn serialize_draft_order_invoice_preview_payload(
  field: Selection,
  args: Dict(String, root_field.ResolvedValue),
  user_errors: List(#(List(String), String)),
) -> Json {
  let email = read_object(args, "email") |> option.unwrap(dict.new())
  let subject =
    read_string(email, "subject") |> option.unwrap("Complete your purchase")
  let custom_message = read_string(email, "customMessage") |> option.unwrap("")
  let source =
    src_object([
      #("previewSubject", SrcString(subject)),
      #(
        "previewHtml",
        SrcString(
          "<!DOCTYPE html><html><body><h1>"
          <> subject
          <> "</h1><p>"
          <> custom_message
          <> "</p></body></html>",
        ),
      ),
      #(
        "userErrors",
        SrcList(
          list.map(user_errors, fn(error) {
            let #(field_path, message) = error
            src_object([
              #("field", SrcList(list.map(field_path, SrcString))),
              #("message", SrcString(message)),
            ])
          }),
        ),
      ),
    ])
  project_graphql_value(source, selection_children(field), dict.new())
}

fn handle_draft_order_bulk_helper(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let tags = read_string_list(args, "tags")
  let user_errors = case
    root_name != "draftOrderBulkDelete" && list.is_empty(tags)
  {
    True -> [#(["tags"], "Tags can't be blank")]
    False -> []
  }
  let targets = case user_errors {
    [] -> select_draft_order_bulk_targets(store, args)
    _ -> []
  }
  let #(next_store, changed_ids) = case user_errors {
    [] -> apply_draft_order_bulk_helper(store, root_name, targets, tags)
    _ -> #(store, [])
  }
  let #(job_id, next_identity) = case user_errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "Job")
      #(Some(id), next_identity)
    }
    _ -> #(None, identity)
  }
  let payload = serialize_draft_order_bulk_payload(field, job_id, user_errors)
  let draft =
    single_root_log_draft(
      root_name,
      changed_ids,
      case user_errors {
        [] -> store.Staged
        _ -> store.Failed
      },
      "orders",
      "stage-locally",
      Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
    )
  #(key, payload, next_store, next_identity, changed_ids, [draft])
}

fn apply_draft_order_bulk_helper(
  store: Store,
  root_name: String,
  targets: List(DraftOrderRecord),
  tags: List(String),
) -> #(Store, List(String)) {
  targets
  |> list.fold(#(store, []), fn(acc, draft_order) {
    let #(current_store, ids) = acc
    case root_name {
      "draftOrderBulkDelete" -> #(
        store.delete_staged_draft_order(current_store, draft_order.id),
        [draft_order.id, ..ids],
      )
      "draftOrderBulkAddTags" -> {
        let updated = update_draft_order_tags(draft_order, tags, "add")
        #(store.stage_draft_order(current_store, updated), [
          draft_order.id,
          ..ids
        ])
      }
      "draftOrderBulkRemoveTags" -> {
        let updated = update_draft_order_tags(draft_order, tags, "remove")
        #(store.stage_draft_order(current_store, updated), [
          draft_order.id,
          ..ids
        ])
      }
      _ -> #(current_store, ids)
    }
  })
}

fn select_draft_order_bulk_targets(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(DraftOrderRecord) {
  let ids = read_string_list(args, "ids")
  case ids {
    [_, ..] ->
      ids
      |> list.filter_map(fn(id) {
        store.get_draft_order_by_id(store, id) |> option.to_result(Nil)
      })
    [] ->
      case read_string(args, "search") {
        Some(search) ->
          store.list_effective_draft_orders(store)
          |> list.filter(fn(record) {
            draft_order_matches_bulk_search(record, search)
          })
        None ->
          case read_string(args, "savedSearchId") {
            Some(_) ->
              store.list_effective_draft_orders(store)
              |> list.filter(fn(record) {
                captured_string_field(record.data, "status") == Some("OPEN")
              })
            None -> []
          }
      }
  }
}

fn draft_order_matches_bulk_search(
  draft_order: DraftOrderRecord,
  search: String,
) -> Bool {
  let query = string.trim(search)
  case string.split_once(query, ":") {
    Ok(#("tag", tag)) ->
      list.contains(draft_order_tags(draft_order.data), string.trim(tag))
    Ok(#("id", id)) -> {
      let expected = string.trim(id)
      draft_order.id == expected
      || draft_order_gid_tail(draft_order.id) == expected
    }
    _ -> False
  }
}

fn draft_order_gid_tail(id: String) -> String {
  case string.split(id, "/") |> list.last {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

fn update_draft_order_tags(
  draft_order: DraftOrderRecord,
  tags: List(String),
  mode: String,
) -> DraftOrderRecord {
  let existing = draft_order_tags(draft_order.data)
  let next_tags = case mode {
    "add" ->
      unique_strings(list.append(existing, tags))
      |> list.sort(by: string.compare)
    "remove" ->
      existing
      |> list.filter(fn(tag) { !list.contains(tags, tag) })
      |> list.sort(by: string.compare)
    _ -> existing
  }
  DraftOrderRecord(
    ..draft_order,
    data: replace_captured_object_fields(draft_order.data, [
      #("tags", CapturedArray(list.map(next_tags, CapturedString))),
    ]),
  )
}

fn draft_order_tags(data: CapturedJsonValue) -> List(String) {
  case captured_object_field(data, "tags") {
    Some(CapturedArray(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          CapturedString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn unique_strings(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> [value, ..acc]
    }
  })
}

fn serialize_draft_order_bulk_payload(
  field: Selection,
  job_id: Option(String),
  user_errors: List(#(List(String), String)),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, case job_id {
              Some(id) -> serialize_job(child, id)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_job(field: Selection, id: String) -> Json {
  let source = src_object([#("id", SrcString(id)), #("done", SrcBool(False))])
  project_graphql_value(source, selection_children(field), dict.new())
}

fn handle_draft_order_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderUpdate",
      [
        RequiredArgument(name: "id", expected_type: "ID!"),
        RequiredArgument(name: "input", expected_type: "DraftOrderInput!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      let input = read_object(args, "input")
      case id, input {
        Some(id), Some(input) ->
          case store.get_draft_order_by_id(store, id) {
            Some(draft_order) -> {
              let #(updated_draft_order, next_identity) =
                build_updated_draft_order(store, identity, draft_order, input)
              let next_store =
                store.stage_draft_order(store, updated_draft_order)
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  Some(updated_draft_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderUpdate",
                  [id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderUpdate in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [id], [], [draft])
            }
            None ->
              unknown_draft_order_update_result(
                key,
                store,
                identity,
                field,
                fragments,
              )
          }
        _, _ ->
          unknown_draft_order_update_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    }
  }
}

fn unknown_draft_order_update_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let payload =
    serialize_draft_order_mutation_payload(
      field,
      None,
      [
        #(["id"], "Draft order does not exist"),
      ],
      fragments,
    )
  #(key, payload, store, identity, [], [], [])
}

fn handle_order_lifecycle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let input_type = case root_name {
    "orderClose" -> "OrderCloseInput!"
    "orderOpen" -> "OrderOpenInput!"
    _ -> "OrderInput!"
  }
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      root_name,
      [RequiredArgument(name: "input", expected_type: input_type)],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let order_id = case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> read_string(input, "id")
        _ -> None
      }
      case order_id {
        None -> {
          let payload =
            serialize_order_mutation_payload(
              field,
              None,
              [
                #(["id"], "Order does not exist"),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [], [])
        }
        Some(id) ->
          case store.get_order_by_id(store, id) {
            None -> {
              let payload =
                serialize_order_mutation_payload(
                  field,
                  None,
                  [
                    #(["id"], "Order does not exist"),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
            Some(order) -> {
              let #(updated_order, next_identity) =
                apply_order_lifecycle_update(order, identity, root_name)
              let next_store = store.stage_order(store, updated_order)
              let payload =
                serialize_order_mutation_payload(
                  field,
                  Some(updated_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  root_name,
                  [id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged " <> root_name <> " in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [id], [], [draft])
            }
          }
      }
    }
  }
}

fn apply_order_lifecycle_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
  root_name: String,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let replacements = case root_name {
    "orderClose" -> [
      #("closed", CapturedBool(True)),
      #("closedAt", CapturedString(timestamp)),
      #("updatedAt", CapturedString(timestamp)),
    ]
    "orderOpen" -> [
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("updatedAt", CapturedString(timestamp)),
    ]
    _ -> []
  }
  #(
    OrderRecord(
      ..order,
      data: replace_captured_object_fields(order.data, replacements),
    ),
    next_identity,
  )
}

fn serialize_order_mutation_payload(
  field: Selection,
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, case order {
              Some(record) -> serialize_order_node(child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn handle_order_cancel_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderCancel",
      [
        RequiredArgument(name: "orderId", expected_type: "ID!"),
        RequiredArgument(name: "restock", expected_type: "Boolean!"),
        RequiredArgument(name: "reason", expected_type: "OrderCancelReason!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_string_arg(args, "orderId") {
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            Some(order) -> {
              let reason =
                read_string_arg(args, "reason") |> option.unwrap("OTHER")
              let #(updated_order, next_identity) =
                apply_order_cancel_update(order, identity, reason)
              let next_store = store.stage_order(store, updated_order)
              let #(job_id, identity_after_job) =
                synthetic_identity.make_synthetic_gid(next_identity, "Job")
              let payload =
                serialize_order_cancel_payload(field, Some(job_id), [])
              let draft =
                single_root_log_draft(
                  "orderCancel",
                  [order_id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some("Locally staged orderCancel in shopify-draft-proxy."),
                )
              #(key, payload, next_store, identity_after_job, [order_id], [], [
                draft,
              ])
            }
            None -> {
              let payload =
                serialize_order_cancel_payload(field, None, [
                  #(["orderId"], "Order does not exist"),
                ])
              #(key, payload, store, identity, [], [], [])
            }
          }
        None -> {
          let payload =
            serialize_order_cancel_payload(field, None, [
              #(["orderId"], "Order does not exist"),
            ])
          #(key, payload, store, identity, [], [], [])
        }
      }
    }
  }
}

fn apply_order_cancel_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
  reason: String,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let updated_data =
    order.data
    |> replace_captured_object_fields([
      #("closed", CapturedBool(True)),
      #("closedAt", CapturedString(timestamp)),
      #("cancelledAt", CapturedString(timestamp)),
      #("cancelReason", CapturedString(reason)),
      #("updatedAt", CapturedString(timestamp)),
    ])
  #(OrderRecord(..order, data: updated_data), next_identity)
}

fn serialize_order_cancel_payload(
  field: Selection,
  job_id: Option(String),
  user_errors: List(#(List(String), String)),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, case job_id {
              Some(id) -> serialize_job(child, id)
              None -> json.null()
            })
            "orderCancelUserErrors" | "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn handle_order_invoice_send(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderInvoiceSend",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> {
      let args = field_arguments(field, variables)
      case read_string_arg(args, "id") {
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            Some(order) -> #(
              key,
              serialize_order_mutation_payload(
                field,
                Some(order),
                [],
                fragments,
              ),
              [],
            )
            None -> #(
              key,
              serialize_order_mutation_payload(
                field,
                None,
                [
                  #(["id"], "Order does not exist"),
                ],
                fragments,
              ),
              [],
            )
          }
        None -> #(
          key,
          serialize_order_mutation_payload(
            field,
            None,
            [
              #(["id"], "Order does not exist"),
            ],
            fragments,
          ),
          [],
        )
      }
    }
  }
}

fn handle_order_mark_as_paid_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderMarkAsPaid",
      [RequiredArgument(name: "input", expected_type: "OrderMarkAsPaidInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let order_id = case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> read_string(input, "id")
        _ -> None
      }
      case order_id {
        None -> {
          let payload =
            serialize_order_mutation_payload(
              field,
              None,
              [
                #(["id"], "Order does not exist"),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [], [])
        }
        Some(id) ->
          case store.get_order_by_id(store, id) {
            None -> {
              let payload =
                serialize_order_mutation_payload(
                  field,
                  None,
                  [
                    #(["id"], "Order does not exist"),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
            Some(order) -> {
              let #(updated_order, next_identity) =
                apply_order_mark_as_paid_update(order, identity)
              let next_store = store.stage_order(store, updated_order)
              let payload =
                serialize_order_mutation_payload(
                  field,
                  Some(updated_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "orderMarkAsPaid",
                  [id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some("Locally staged orderMarkAsPaid in shopify-draft-proxy."),
                )
              #(key, payload, next_store, next_identity, [id], [], [draft])
            }
          }
      }
    }
  }
}

fn apply_order_mark_as_paid_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  case captured_string_field(order.data, "displayFinancialStatus") {
    Some("PAID") -> #(order, identity)
    _ -> {
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let amount_set = order_payment_amount_set(order)
      let currency_code = captured_money_set_currency(amount_set)
      let transaction =
        CapturedObject([
          #("kind", CapturedString("SALE")),
          #("status", CapturedString("SUCCESS")),
          #("gateway", CapturedString("manual")),
          #("amountSet", amount_set),
        ])
      let updated_data =
        order.data
        |> replace_captured_object_fields([
          #("updatedAt", CapturedString(timestamp)),
          #("displayFinancialStatus", CapturedString("PAID")),
          #("paymentGatewayNames", CapturedArray([CapturedString("manual")])),
          #("totalOutstandingSet", money_set_string("0.0", currency_code)),
          #(
            "transactions",
            CapturedArray(list.append(order_transactions(order), [transaction])),
          ),
        ])
      #(OrderRecord(..order, data: updated_data), next_identity)
    }
  }
}

fn order_payment_amount_set(order: OrderRecord) -> CapturedJsonValue {
  let outstanding = case
    captured_object_field(order.data, "totalOutstandingSet")
  {
    Some(value) ->
      case captured_money_value(value) >. 0.0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
  outstanding
  |> option.or(captured_object_field(order.data, "currentTotalPriceSet"))
  |> option.or(captured_object_field(order.data, "totalPriceSet"))
  |> option.unwrap(money_set_string("0.0", order_currency_code(order)))
}

fn order_currency_code(order: OrderRecord) -> String {
  captured_object_field(order.data, "currentTotalPriceSet")
  |> option.or(captured_object_field(order.data, "totalOutstandingSet"))
  |> option.or(captured_object_field(order.data, "totalPriceSet"))
  |> option.map(captured_money_set_currency)
  |> option.unwrap("CAD")
}

fn captured_money_set_currency(value: CapturedJsonValue) -> String {
  case captured_object_field(value, "shopMoney") {
    Some(shop_money) ->
      captured_string_field(shop_money, "currencyCode") |> option.unwrap("CAD")
    None -> "CAD"
  }
}

fn money_set_string(
  amount: String,
  currency_code: String,
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "shopMoney",
      CapturedObject([
        #("amount", CapturedString(amount)),
        #("currencyCode", CapturedString(currency_code)),
      ]),
    ),
  ])
}

fn order_transactions(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "transactions") {
    Some(CapturedArray(items)) -> items
    _ -> []
  }
}

fn handle_order_capture_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  case read_object(args, "input") {
    None -> {
      let payload =
        serialize_order_capture_payload(
          field,
          None,
          None,
          [#(["input"], "Input is required.")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCapture", [], store.Failed),
      ])
    }
    Some(input) -> {
      let order_id =
        read_string(input, "id") |> option.or(read_string(input, "orderId"))
      let parent_transaction_id =
        read_string(input, "parentTransactionId")
        |> option.or(read_string(input, "transactionId"))
      case order_id {
        None -> {
          let payload =
            serialize_order_capture_payload(
              field,
              None,
              None,
              [#(["input", "id"], "Order does not exist")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("orderCapture", [], store.Failed),
          ])
        }
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            None -> {
              let payload =
                serialize_order_capture_payload(
                  field,
                  None,
                  None,
                  [#(["input", "id"], "Order does not exist")],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft("orderCapture", [], store.Failed),
              ])
            }
            Some(order) ->
              case parent_transaction_id {
                None -> {
                  let payload =
                    serialize_order_capture_payload(
                      field,
                      None,
                      Some(order),
                      [
                        #(
                          ["input", "parentTransactionId"],
                          "Transaction does not exist",
                        ),
                      ],
                      fragments,
                    )
                  #(key, payload, store, identity, [], [
                    payment_log_draft("orderCapture", [order.id], store.Failed),
                  ])
                }
                Some(transaction_id) ->
                  case find_transaction(order, transaction_id) {
                    None -> {
                      let payload =
                        serialize_order_capture_payload(
                          field,
                          None,
                          Some(order),
                          [
                            #(
                              ["input", "parentTransactionId"],
                              "Transaction does not exist",
                            ),
                          ],
                          fragments,
                        )
                      #(key, payload, store, identity, [], [
                        payment_log_draft(
                          "orderCapture",
                          [order.id],
                          store.Failed,
                        ),
                      ])
                    }
                    Some(authorization) ->
                      capture_order_payment(
                        key,
                        store,
                        identity,
                        field,
                        fragments,
                        order,
                        authorization,
                        input,
                      )
                  }
              }
          }
      }
    }
  }
}

fn capture_order_payment(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  authorization: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let remaining = capturable_amount_for_authorization(order, authorization)
  let amount = payment_input_amount(input, remaining)
  case remaining <=. 0.0 {
    True -> {
      let payload =
        serialize_order_capture_payload(
          field,
          None,
          Some(order),
          [
            #(["input", "parentTransactionId"], "Transaction is not capturable"),
          ],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCapture", [order.id], store.Failed),
      ])
    }
    False ->
      case amount <=. 0.0 {
        True -> {
          let payload =
            serialize_order_capture_payload(
              field,
              None,
              Some(order),
              [#(["input", "amount"], "Amount must be greater than zero")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("orderCapture", [order.id], store.Failed),
          ])
        }
        False ->
          case amount >. remaining {
            True -> {
              let payload =
                serialize_order_capture_payload(
                  field,
                  None,
                  Some(order),
                  [#(["input", "amount"], "Amount exceeds capturable amount")],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft("orderCapture", [order.id], store.Failed),
              ])
            }
            False -> {
              let currency_code =
                payment_input_currency(input, order_currency_code(order))
              let #(payment_reference_id, identity_after_reference) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "PaymentReference",
                )
              let #(transaction, identity_after_capture) =
                build_payment_transaction(
                  identity_after_reference,
                  "CAPTURE",
                  money_set(amount, currency_code),
                  captured_string_field(authorization, "gateway"),
                  captured_string_field(authorization, "id"),
                  Some(payment_reference_id),
                )
              let final_capture = read_bool(input, "finalCapture", False)
              let remaining_after_capture = max_float(0.0, remaining -. amount)
              let #(extra_transactions, next_identity) = case
                final_capture && remaining_after_capture >. 0.0
              {
                True -> {
                  let #(void_transaction, identity_after_void) =
                    build_payment_transaction(
                      identity_after_capture,
                      "VOID",
                      money_set(remaining_after_capture, currency_code),
                      captured_string_field(authorization, "gateway"),
                      captured_string_field(authorization, "id"),
                      None,
                    )
                  #([void_transaction], identity_after_void)
                }
                False -> #([], identity_after_capture)
              }
              let updated_order =
                order
                |> append_order_transactions([transaction, ..extra_transactions])
                |> apply_payment_derived_fields
              let next_store = store.stage_order(store, updated_order)
              let payload =
                serialize_order_capture_payload(
                  field,
                  Some(transaction),
                  Some(updated_order),
                  [],
                  fragments,
                )
              #(key, payload, next_store, next_identity, [order.id], [
                payment_log_draft("orderCapture", [order.id], store.Staged),
              ])
            }
          }
      }
  }
}

fn handle_transaction_void_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let #(transaction_id, field_name) = transaction_void_reference(args)
  case transaction_id {
    None -> {
      let payload =
        serialize_transaction_void_payload(
          field,
          None,
          [#([field_name], "Transaction does not exist")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("transactionVoid", [], store.Failed),
      ])
    }
    Some(transaction_id) ->
      case find_order_with_transaction(store, transaction_id) {
        None -> {
          let payload =
            serialize_transaction_void_payload(
              field,
              None,
              [#([field_name], "Transaction does not exist")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("transactionVoid", [], store.Failed),
          ])
        }
        Some(match) -> {
          let #(order, transaction) = match
          void_order_transaction(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            transaction,
          )
        }
      }
  }
}

fn void_order_transaction(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let user_errors = case is_successful_authorization(authorization) {
    False -> [#(["id"], "Transaction is not voidable")]
    True ->
      case
        transaction_has_voiding_child(
          order,
          captured_string_field(authorization, "id") |> option.unwrap(""),
        )
      {
        True -> [#(["id"], "Transaction has already been voided")]
        False ->
          case
            captured_amount_for_authorization(
              order,
              captured_string_field(authorization, "id") |> option.unwrap(""),
            )
            >. 0.0
          {
            True -> [#(["id"], "Transaction has already been captured")]
            False -> []
          }
      }
  }
  case user_errors {
    [_, ..] -> {
      let payload =
        serialize_transaction_void_payload(field, None, user_errors, fragments)
      #(key, payload, store, identity, [], [
        payment_log_draft("transactionVoid", [order.id], store.Failed),
      ])
    }
    [] -> {
      let #(transaction, next_identity) =
        build_payment_transaction(
          identity,
          "VOID",
          captured_object_field(authorization, "amountSet")
            |> option.unwrap(money_set(0.0, order_currency_code(order))),
          captured_string_field(authorization, "gateway"),
          captured_string_field(authorization, "id"),
          None,
        )
      let updated_order =
        order
        |> append_order_transactions([transaction])
        |> apply_payment_derived_fields
      let next_store = store.stage_order(store, updated_order)
      let payload =
        serialize_transaction_void_payload(
          field,
          Some(transaction),
          [],
          fragments,
        )
      #(key, payload, next_store, next_identity, [order.id], [
        payment_log_draft("transactionVoid", [order.id], store.Staged),
      ])
    }
  }
}

fn handle_order_create_mandate_payment_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let input = read_object(args, "input") |> option.unwrap(args)
  let order_id =
    read_string(input, "id") |> option.or(read_string(input, "orderId"))
  case order_id {
    None -> {
      let payload =
        serialize_mandate_payment_payload(
          field,
          None,
          None,
          None,
          [#(["id"], "Order does not exist")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCreateMandatePayment", [], store.Failed),
      ])
    }
    Some(order_id) ->
      case store.get_order_by_id(store, order_id) {
        None -> {
          let payload =
            serialize_mandate_payment_payload(
              field,
              None,
              None,
              None,
              [#(["id"], "Order does not exist")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("orderCreateMandatePayment", [], store.Failed),
          ])
        }
        Some(order) -> {
          let idempotency_key = read_string(input, "idempotencyKey")
          case idempotency_key {
            None -> {
              let payload =
                serialize_mandate_payment_payload(
                  field,
                  None,
                  None,
                  Some(order),
                  [#(["idempotencyKey"], "Idempotency key is required")],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft(
                  "orderCreateMandatePayment",
                  [order.id],
                  store.Failed,
                ),
              ])
            }
            Some(idempotency_key) ->
              case find_mandate_payment(order, idempotency_key) {
                Some(payment) -> {
                  let payload =
                    serialize_mandate_payment_payload(
                      field,
                      Some(payment),
                      captured_string_field(payment, "paymentReferenceId"),
                      Some(order),
                      [],
                      fragments,
                    )
                  #(key, payload, store, identity, [order.id], [
                    payment_log_draft(
                      "orderCreateMandatePayment",
                      [order.id],
                      store.Staged,
                    ),
                  ])
                }
                None ->
                  create_mandate_payment(
                    key,
                    store,
                    identity,
                    field,
                    fragments,
                    order,
                    input,
                    idempotency_key,
                  )
              }
          }
        }
      }
  }
}

fn create_mandate_payment(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  idempotency_key: String,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let currency_code = payment_input_currency(input, order_currency_code(order))
  let amount =
    payment_input_amount(
      input,
      captured_money_amount(order.data, "totalOutstandingSet")
        |> nonzero_float(captured_money_amount(
          order.data,
          "currentTotalPriceSet",
        )),
    )
  case amount <=. 0.0 {
    True -> {
      let payload =
        serialize_mandate_payment_payload(
          field,
          None,
          None,
          Some(order),
          [#(["amount"], "Amount must be greater than zero")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCreateMandatePayment", [order.id], store.Failed),
      ])
    }
    False -> {
      let #(payment_reference_id, identity_after_reference) =
        synthetic_identity.make_synthetic_gid(identity, "PaymentReference")
      let #(transaction, identity_after_transaction) =
        build_payment_transaction(
          identity_after_reference,
          "MANDATE_PAYMENT",
          money_set(amount, currency_code),
          Some("mandate"),
          None,
          Some(payment_reference_id),
        )
      let #(job_id, identity_after_job) =
        synthetic_identity.make_synthetic_gid(identity_after_transaction, "Job")
      let mandate_payment =
        CapturedObject([
          #("idempotencyKey", CapturedString(idempotency_key)),
          #("jobId", CapturedString(job_id)),
          #("paymentReferenceId", CapturedString(payment_reference_id)),
          #(
            "transactionId",
            optional_captured_string(captured_string_field(transaction, "id")),
          ),
        ])
      let updated_order =
        order
        |> append_order_transactions([transaction])
        |> append_mandate_payment(mandate_payment)
        |> append_payment_gateway("mandate")
        |> apply_payment_derived_fields
      let next_store = store.stage_order(store, updated_order)
      let payload =
        serialize_mandate_payment_payload(
          field,
          Some(mandate_payment),
          Some(payment_reference_id),
          Some(updated_order),
          [],
          fragments,
        )
      #(key, payload, next_store, identity_after_job, [order.id], [
        payment_log_draft("orderCreateMandatePayment", [order.id], store.Staged),
      ])
    }
  }
}

fn build_payment_transaction(
  identity: SyntheticIdentityRegistry,
  kind: String,
  amount_set: CapturedJsonValue,
  gateway: Option(String),
  parent_transaction_id: Option(String),
  payment_reference_id: Option(String),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(transaction_id, identity_after_transaction) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let #(payment_id, identity_after_payment) =
    synthetic_identity.make_synthetic_gid(identity_after_transaction, "Payment")
  let #(processed_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_payment)
  let parent_transaction = case parent_transaction_id {
    Some(id) ->
      CapturedObject([
        #("id", CapturedString(id)),
        #("kind", CapturedString("AUTHORIZATION")),
        #("status", CapturedString("SUCCESS")),
      ])
    None -> CapturedNull
  }
  #(
    CapturedObject([
      #("id", CapturedString(transaction_id)),
      #("kind", CapturedString(kind)),
      #("status", CapturedString("SUCCESS")),
      #("gateway", optional_captured_string(gateway)),
      #("amountSet", amount_set),
      #("parentTransactionId", optional_captured_string(parent_transaction_id)),
      #("parentTransaction", parent_transaction),
      #("paymentId", CapturedString(payment_id)),
      #("paymentReferenceId", optional_captured_string(payment_reference_id)),
      #("processedAt", CapturedString(processed_at)),
    ]),
    next_identity,
  )
}

fn append_order_transactions(
  order: OrderRecord,
  transactions: List(CapturedJsonValue),
) -> OrderRecord {
  let updated =
    order.data
    |> replace_captured_object_fields([
      #(
        "transactions",
        CapturedArray(list.append(order_transactions(order), transactions)),
      ),
    ])
  OrderRecord(..order, data: updated)
}

fn append_mandate_payment(
  order: OrderRecord,
  payment: CapturedJsonValue,
) -> OrderRecord {
  let existing = mandate_payments(order)
  let updated =
    order.data
    |> replace_captured_object_fields([
      #("mandatePayments", CapturedArray(list.append(existing, [payment]))),
    ])
  OrderRecord(..order, data: updated)
}

fn append_payment_gateway(order: OrderRecord, gateway: String) -> OrderRecord {
  let existing = payment_gateway_names(order)
  let gateways = case list.contains(existing, gateway) {
    True -> existing
    False -> list.append(existing, [gateway])
  }
  let updated =
    order.data
    |> replace_captured_object_fields([
      #(
        "paymentGatewayNames",
        CapturedArray(list.map(gateways, CapturedString)),
      ),
    ])
  OrderRecord(..order, data: updated)
}

fn apply_payment_derived_fields(order: OrderRecord) -> OrderRecord {
  let currency_code = order_currency_code(order)
  let received = total_received_amount(order)
  let total =
    captured_money_amount(order.data, "currentTotalPriceSet")
    |> nonzero_float(captured_money_amount(order.data, "totalPriceSet"))
  let outstanding = max_float(0.0, total -. received)
  let capturable = total_capturable_amount(order)
  let has_voided_authorization =
    order_transactions(order)
    |> list.any(fn(transaction) {
      is_successful_authorization(transaction)
      && transaction_has_voiding_child(
        order,
        captured_string_field(transaction, "id") |> option.unwrap(""),
      )
    })
  let display_status = case received >=. total && total >. 0.0 {
    True -> "PAID"
    False ->
      case received >. 0.0 {
        True -> "PARTIALLY_PAID"
        False ->
          case capturable >. 0.0 {
            True -> "AUTHORIZED"
            False ->
              case has_voided_authorization {
                True -> "VOIDED"
                False ->
                  captured_string_field(order.data, "displayFinancialStatus")
                  |> option.unwrap("PENDING")
              }
          }
      }
  }
  let updated =
    order.data
    |> replace_captured_object_fields([
      #("displayFinancialStatus", CapturedString(display_status)),
      #("capturable", CapturedBool(capturable >. 0.0)),
      #("totalCapturable", CapturedString(float.to_string(capturable))),
      #("totalCapturableSet", money_set(capturable, currency_code)),
      #("totalOutstandingSet", money_set(outstanding, currency_code)),
      #("totalReceivedSet", money_set(received, currency_code)),
      #("netPaymentSet", money_set(received, currency_code)),
    ])
  OrderRecord(..order, data: updated)
}

fn find_order_with_transaction(
  store: Store,
  transaction_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case find_transaction(order, transaction_id) {
      Some(transaction) -> Ok(#(order, transaction))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn find_transaction(
  order: OrderRecord,
  transaction_id: String,
) -> Option(CapturedJsonValue) {
  order_transactions(order)
  |> list.find(fn(transaction) {
    captured_string_field(transaction, "id") == Some(transaction_id)
  })
  |> option.from_result
}

fn is_successful_authorization(transaction: CapturedJsonValue) -> Bool {
  captured_string_field(transaction, "kind") == Some("AUTHORIZATION")
  && captured_string_field(transaction, "status") == Some("SUCCESS")
}

fn is_successful_payment_capture(transaction: CapturedJsonValue) -> Bool {
  captured_string_field(transaction, "status") == Some("SUCCESS")
  && case captured_string_field(transaction, "kind") {
    Some("SALE") | Some("CAPTURE") | Some("MANDATE_PAYMENT") -> True
    _ -> False
  }
}

fn transaction_has_voiding_child(
  order: OrderRecord,
  parent_transaction_id: String,
) -> Bool {
  order_transactions(order)
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "kind") == Some("VOID")
    && captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "parentTransactionId")
    == Some(parent_transaction_id)
  })
}

fn captured_amount_for_authorization(
  order: OrderRecord,
  parent_transaction_id: String,
) -> Float {
  order_transactions(order)
  |> list.filter(fn(transaction) {
    captured_string_field(transaction, "kind") == Some("CAPTURE")
    && captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "parentTransactionId")
    == Some(parent_transaction_id)
  })
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_amount(transaction, "amountSet")
  })
}

fn capturable_amount_for_authorization(
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> Float {
  let authorization_id =
    captured_string_field(authorization, "id") |> option.unwrap("")
  case
    !is_successful_authorization(authorization)
    || transaction_has_voiding_child(order, authorization_id)
  {
    True -> 0.0
    False ->
      max_float(
        0.0,
        captured_money_amount(authorization, "amountSet")
          -. captured_amount_for_authorization(order, authorization_id),
      )
  }
}

fn total_capturable_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_authorization)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. capturable_amount_for_authorization(order, transaction)
  })
}

fn total_received_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_payment_capture)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_amount(transaction, "amountSet")
  })
}

fn payment_gateway_names(order: OrderRecord) -> List(String) {
  case captured_object_field(order.data, "paymentGatewayNames") {
    Some(CapturedArray(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          CapturedString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn mandate_payments(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "mandatePayments") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

fn find_mandate_payment(
  order: OrderRecord,
  idempotency_key: String,
) -> Option(CapturedJsonValue) {
  mandate_payments(order)
  |> list.find(fn(payment) {
    captured_string_field(payment, "idempotencyKey") == Some(idempotency_key)
  })
  |> option.from_result
}

fn payment_input_amount(
  input: Dict(String, root_field.ResolvedValue),
  fallback: Float,
) -> Float {
  case dict.get(input, "amount") {
    Ok(root_field.ObjectVal(amount)) ->
      read_number(amount, "amount") |> option.unwrap(fallback)
    _ ->
      read_number(input, "amount")
      |> option.or(case read_object(input, "amountSet") {
        Some(amount_set) ->
          case read_object(amount_set, "shopMoney") {
            Some(shop_money) -> read_number(shop_money, "amount")
            None -> None
          }
        None -> None
      })
      |> option.unwrap(fallback)
  }
}

fn payment_input_currency(
  input: Dict(String, root_field.ResolvedValue),
  fallback: String,
) -> String {
  read_string(input, "currency")
  |> option.or(case read_object(input, "amount") {
    Some(amount) -> read_string(amount, "currencyCode")
    None -> None
  })
  |> option.or(case read_object(input, "amountSet") {
    Some(amount_set) ->
      case read_object(amount_set, "shopMoney") {
        Some(shop_money) -> read_string(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  })
  |> option.unwrap(fallback)
}

fn transaction_void_reference(
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), String) {
  case read_string(args, "parentTransactionId") {
    Some(id) -> #(Some(id), "parentTransactionId")
    None ->
      case read_string(args, "id") {
        Some(id) -> #(Some(id), "id")
        None ->
          case read_object(args, "input") {
            Some(input) ->
              case read_string(input, "parentTransactionId") {
                Some(id) -> #(Some(id), "parentTransactionId")
                None -> #(read_string(input, "id"), "id")
              }
            None -> #(None, "parentTransactionId")
          }
      }
  }
}

fn serialize_order_capture_payload(
  field: Selection,
  transaction: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              serialize_captured_selection(child, transaction, fragments),
            )
            "order" -> #(key, case order {
              Some(order) -> serialize_order_node(child, order, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_transaction_void_payload(
  field: Selection,
  transaction: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              serialize_captured_selection(child, transaction, fragments),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_mandate_payment_payload(
  field: Selection,
  payment: Option(CapturedJsonValue),
  payment_reference_id: Option(String),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, case payment {
              Some(payment) ->
                serialize_job_selection(
                  child,
                  captured_string_field(payment, "jobId"),
                )
              None -> json.null()
            })
            "paymentReferenceId" -> #(
              key,
              option_string_json(payment_reference_id),
            )
            "order" -> #(key, case order {
              Some(order) -> serialize_order_node(child, order, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_captured_selection(
  field: Selection,
  value: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  case value {
    Some(value) ->
      project_graphql_value(
        captured_json_source(value),
        selection_children(field),
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_job_selection(field: Selection, job_id: Option(String)) -> Json {
  case job_id {
    None -> json.null()
    Some(id) ->
      json.object(
        list.map(selection_children(field), fn(child) {
          let key = get_field_response_key(child)
          case child {
            Field(name: name, ..) ->
              case name.value {
                "id" -> #(key, json.string(id))
                "done" -> #(key, json.bool(True))
                _ -> #(key, json.null())
              }
            _ -> #(key, json.null())
          }
        }),
      )
  }
}

fn option_string_json(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

fn payment_log_draft(
  root_name: String,
  staged_ids: List(String),
  status: store.EntryStatus,
) -> LogDraft {
  single_root_log_draft(
    root_name,
    staged_ids,
    status,
    "payments",
    "stage-locally",
    Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
  )
}

fn handle_order_edit_begin_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderEditBegin",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let order =
        read_string(args, "id")
        |> option.then(fn(id) { store.get_order_by_id(store, id) })
      case order {
        Some(order) -> {
          let #(calculated_order, next_identity) =
            build_calculated_order_from_order(order, identity)
          let next_store =
            stage_order_edit_session(store, order, calculated_order)
          let payload =
            serialize_order_edit_begin_payload(
              field,
              calculated_order,
              fragments,
            )
          #(key, payload, next_store, next_identity, [], [], [])
        }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn handle_order_edit_add_variant_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderEditAddVariant",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      let variant_id = read_string(args, "variantId")
      let variant =
        variant_id
        |> option.then(fn(id) { store.get_effective_variant_by_id(store, id) })
      case variant {
        Some(variant) -> {
          let product =
            store.get_effective_product_by_id(store, variant.product_id)
          let quantity = read_int(args, "quantity", 1)
          let session_id =
            calculated_order_id
            |> option.map(order_edit_session_id_from_calculated_id)
            |> option.unwrap("")
          let #(calculated_line_item, next_identity) =
            build_added_calculated_line_item(
              variant,
              product,
              quantity,
              identity,
            )
          let #(next_store, calculated_order) =
            update_order_edit_session_with_line_item(
              store,
              calculated_order_id,
              calculated_line_item,
            )
          let payload =
            serialize_order_edit_add_variant_payload(
              field,
              calculated_line_item,
              calculated_order,
              session_id,
              fragments,
            )
          #(key, payload, next_store, next_identity, [], [], [])
        }
        None -> {
          let payload = case variant_id {
            Some(id) ->
              case draft_order_gid_tail(id) == "0" {
                True ->
                  serialize_order_edit_add_variant_invalid_variant_payload(
                    field,
                  )
                False -> json.null()
              }
            _ -> json.null()
          }
          #(key, payload, store, identity, [], [], [])
        }
      }
    }
  }
}

fn handle_order_edit_set_quantity_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderEditSetQuantity",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      let quantity = read_int(args, "quantity", 0)
      let line_item =
        find_order_edit_session_line_item(
          store,
          calculated_order_id,
          read_string(args, "lineItemId"),
        )
        |> option.or(
          read_string(args, "lineItemId")
          |> option.then(fn(id) {
            find_order_edit_line_item_by_calculated_id(store, id)
          }),
        )
      case line_item {
        Some(line_item) -> {
          let calculated_line_item =
            build_set_quantity_calculated_line_item(line_item, quantity)
          let #(next_store, calculated_order) =
            update_order_edit_session_line_item_quantity(
              store,
              calculated_order_id,
              read_string(args, "lineItemId"),
              quantity,
            )
          let payload =
            serialize_order_edit_set_quantity_payload(
              field,
              calculated_line_item,
              calculated_order,
              calculated_order_id,
              fragments,
            )
          #(key, payload, next_store, identity, [], [], [])
        }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn handle_order_edit_commit_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderEditCommit",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      case find_order_edit_session(store, calculated_order_id) {
        None -> #(key, json.null(), store, identity, [], [], [])
        Some(match) -> {
          let #(order, session) = match
          let #(timestamp, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let committed_order =
            commit_order_edit_session(order, session, timestamp)
          let next_store =
            store.stage_order(
              store,
              remove_order_edit_session(committed_order, calculated_order_id),
            )
          let payload =
            serialize_order_edit_commit_payload(
              field,
              committed_order,
              fragments,
            )
          let draft =
            single_root_log_draft(
              "orderEditCommit",
              [order.id],
              store.Staged,
              "orders",
              "stage-locally",
              Some("Locally staged orderEditCommit in shopify-draft-proxy."),
            )
          #(key, payload, next_store, next_identity, [order.id], [], [draft])
        }
      }
    }
  }
}

fn handle_order_edit_residual_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  case find_order_edit_session(store, read_string(args, "id")) {
    None -> #(key, json.null(), store, identity)
    Some(match) -> {
      let #(order, session) = match
      case root_name {
        "orderEditAddCustomItem" ->
          order_edit_add_custom_item(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditAddLineItemDiscount" ->
          order_edit_add_line_item_discount(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditRemoveDiscount" ->
          order_edit_remove_discount(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditAddShippingLine" ->
          order_edit_add_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditUpdateShippingLine" ->
          order_edit_update_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditRemoveShippingLine" ->
          order_edit_remove_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        _ -> #(key, json.null(), store, identity)
      }
    }
  }
}

fn handle_return_lifecycle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  case root_name {
    "returnCreate" | "returnRequest" -> {
      let args = field_arguments(field, variables)
      let input_key = case root_name {
        "returnCreate" -> "returnInput"
        _ -> "input"
      }
      let status = case root_name {
        "returnCreate" -> "OPEN"
        _ -> "REQUESTED"
      }
      let result =
        apply_return_create(
          store,
          identity,
          read_object(args, input_key),
          status,
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnCancel" | "returnClose" | "returnReopen" -> {
      let status = case root_name {
        "returnCancel" -> "CANCELED"
        "returnClose" -> "CLOSED"
        _ -> "OPEN"
      }
      let result =
        apply_return_status_update(
          store,
          identity,
          read_string_argument(field, "id", variables),
          status,
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "removeFromReturn" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_remove_from_return(
          store,
          identity,
          read_string(args, "returnId"),
          read_object_list(args, "returnLineItems"),
          read_object_list(args, "exchangeLineItems"),
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnDeclineRequest" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_return_decline_request(
          store,
          identity,
          read_object(args, "input"),
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    _ -> #(key, json.null(), store, identity, [], [])
  }
}

type ReturnMutationResult {
  ReturnMutationResult(
    order: Option(OrderRecord),
    order_return: Option(CapturedJsonValue),
    store: Store,
    identity: SyntheticIdentityRegistry,
    user_errors: List(#(List(String), String)),
  )
}

fn apply_return_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
  status: String,
) -> ReturnMutationResult {
  case input {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        #(["input"], "Input is required."),
      ])
    Some(input) -> {
      case read_string(input, "orderId") {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            #(["orderId"], "Order does not exist."),
          ])
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            None ->
              ReturnMutationResult(None, None, store, identity, [
                #(["orderId"], "Order does not exist."),
              ])
            Some(order) -> {
              let line_item_result =
                build_return_line_items(identity, order, input)
              case line_item_result {
                Error(user_errors) ->
                  ReturnMutationResult(
                    Some(order),
                    None,
                    store,
                    identity,
                    user_errors,
                  )
                Ok(line_item_pack) -> {
                  let #(line_items, identity_after_line_items) = line_item_pack
                  let #(order_return, identity_after_return) =
                    build_order_return(
                      identity_after_line_items,
                      order,
                      line_items,
                      input,
                      status,
                    )
                  let #(next_store, next_identity, updated_order) =
                    stage_order_with_returns(
                      store,
                      identity_after_return,
                      order,
                      [order_return, ..order_returns(order.data)],
                    )
                  ReturnMutationResult(
                    Some(updated_order),
                    Some(order_return),
                    next_store,
                    next_identity,
                    [],
                  )
                }
              }
            }
          }
      }
    }
  }
}

fn apply_return_status_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  return_id: Option(String),
  status: String,
) -> ReturnMutationResult {
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        #(["id"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            #(["id"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          let #(closed_at, identity_after_closed) = case status {
            "CLOSED" -> {
              let #(timestamp, after_closed) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(CapturedString(timestamp), after_closed)
            }
            _ -> #(CapturedNull, identity)
          }
          let #(updated_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity_after_closed)
          let updated_return =
            replace_captured_object_fields(order_return, [
              #("status", CapturedString(status)),
              #("closedAt", closed_at),
            ])
          let returns =
            order_returns(order.data)
            |> list.map(fn(candidate) {
              case captured_string_field(candidate, "id") == Some(return_id) {
                True -> updated_return
                False -> candidate
              }
            })
          let updated_order =
            OrderRecord(
              ..order,
              data: replace_captured_object_fields(order.data, [
                #("updatedAt", CapturedString(updated_at)),
                #("returns", CapturedArray(returns)),
              ]),
            )
          ReturnMutationResult(
            Some(updated_order),
            Some(updated_return),
            store.stage_order(store, updated_order),
            next_identity,
            [],
          )
        }
      }
  }
}

fn apply_remove_from_return(
  store: Store,
  identity: SyntheticIdentityRegistry,
  return_id: Option(String),
  raw_return_line_items: List(Dict(String, root_field.ResolvedValue)),
  raw_exchange_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        #(["returnId"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            #(["returnId"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case raw_return_line_items, raw_exchange_line_items {
            [], [] ->
              ReturnMutationResult(Some(order), None, store, identity, [
                #(
                  ["returnLineItems"],
                  "Return line items or exchange line items are required.",
                ),
              ])
            _, [_, ..] ->
              ReturnMutationResult(Some(order), None, store, identity, [
                #(
                  ["exchangeLineItems"],
                  "Exchange line item removal is not supported by the local return model yet.",
                ),
              ])
            _, _ -> {
              let #(next_line_items, user_errors) =
                remove_return_line_items(
                  order_return_line_items(order_return),
                  raw_return_line_items,
                )
              case user_errors {
                [_, ..] ->
                  ReturnMutationResult(
                    Some(order),
                    None,
                    store,
                    identity,
                    user_errors,
                  )
                [] -> {
                  let updated_return =
                    replace_captured_object_fields(order_return, [
                      #(
                        "totalQuantity",
                        CapturedInt(total_return_quantity(next_line_items)),
                      ),
                      #("returnLineItems", CapturedArray(next_line_items)),
                    ])
                    |> sync_reverse_fulfillment_line_items(identity)
                  let #(synced_return, next_identity) = updated_return
                  let returns =
                    order_returns(order.data)
                    |> list.map(fn(candidate) {
                      case
                        captured_string_field(candidate, "id")
                        == Some(return_id)
                      {
                        True -> synced_return
                        False -> candidate
                      }
                    })
                  let #(next_store, staged_identity, updated_order) =
                    stage_order_with_returns(
                      store,
                      next_identity,
                      order,
                      returns,
                    )
                  ReturnMutationResult(
                    Some(updated_order),
                    Some(synced_return),
                    next_store,
                    staged_identity,
                    [],
                  )
                }
              }
            }
          }
        }
      }
  }
}

fn remove_return_line_items(
  existing_line_items: List(CapturedJsonValue),
  raw_return_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(CapturedJsonValue), List(#(List(String), String))) {
  raw_return_line_items
  |> list.index_fold(#(existing_line_items, []), fn(acc, input, index) {
    let #(line_items, user_errors) = acc
    let line_item_id = read_string(input, "returnLineItemId")
    let quantity = read_int(input, "quantity", 0)
    let line_item =
      line_item_id
      |> option.then(fn(id) { find_return_line_item(line_items, id) })
    case line_item_id, line_item {
      None, _ -> #(
        line_items,
        list.append(user_errors, [
          #(
            ["returnLineItems", int.to_string(index), "returnLineItemId"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(_), None -> #(
        line_items,
        list.append(user_errors, [
          #(
            ["returnLineItems", int.to_string(index), "returnLineItemId"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(id), Some(line_item) -> {
        let current_quantity =
          captured_int_field(line_item, "quantity") |> option.unwrap(0)
        let processed_quantity =
          captured_int_field(line_item, "processedQuantity") |> option.unwrap(0)
        let removable_quantity = current_quantity - processed_quantity
        case quantity <= 0 || quantity > removable_quantity {
          True -> #(
            line_items,
            list.append(user_errors, [
              #(
                ["returnLineItems", int.to_string(index), "quantity"],
                "Quantity is not removable from return.",
              ),
            ]),
          )
          False -> #(
            apply_return_line_item_removal(line_items, id, quantity),
            user_errors,
          )
        }
      }
    }
  })
}

fn find_return_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Option(CapturedJsonValue) {
  line_items
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(id)
  })
  |> option.from_result
}

fn apply_return_line_item_removal(
  line_items: List(CapturedJsonValue),
  id: String,
  quantity: Int,
) -> List(CapturedJsonValue) {
  line_items
  |> list.filter_map(fn(line_item) {
    case captured_string_field(line_item, "id") == Some(id) {
      False -> Ok(line_item)
      True -> {
        let current_quantity =
          captured_int_field(line_item, "quantity") |> option.unwrap(0)
        let next_quantity = current_quantity - quantity
        case next_quantity <= 0 {
          True -> Error(Nil)
          False ->
            Ok(
              replace_captured_object_fields(line_item, [
                #("quantity", CapturedInt(next_quantity)),
              ]),
            )
        }
      }
    }
  })
}

fn apply_return_decline_request(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  let return_id = case input {
    Some(input) -> read_string(input, "id")
    None -> None
  }
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        #(["input", "id"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            #(["input", "id"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case captured_string_field(order_return, "status") {
            Some("REQUESTED") -> {
              let input_fields = input |> option.unwrap(dict.new())
              let declined_return =
                replace_captured_object_fields(order_return, [
                  #("status", CapturedString("DECLINED")),
                  #(
                    "decline",
                    CapturedObject([
                      #(
                        "reason",
                        optional_captured_string(read_string(
                          input_fields,
                          "declineReason",
                        )),
                      ),
                      #(
                        "note",
                        optional_captured_string(read_string(
                          input_fields,
                          "declineNote",
                        )),
                      ),
                    ]),
                  ),
                ])
              let returns =
                order_returns(order.data)
                |> list.map(fn(candidate) {
                  case
                    captured_string_field(candidate, "id") == Some(return_id)
                  {
                    True -> declined_return
                    False -> candidate
                  }
                })
              let #(next_store, next_identity, updated_order) =
                stage_order_with_returns(store, identity, order, returns)
              ReturnMutationResult(
                Some(updated_order),
                Some(declined_return),
                next_store,
                next_identity,
                [],
              )
            }
            _ ->
              ReturnMutationResult(Some(order), None, store, identity, [
                #(
                  ["input", "id"],
                  "Return is not declinable. Only non-refunded returns with status REQUESTED can be declined.",
                ),
              ])
          }
        }
      }
  }
}

fn build_return_line_items(
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Result(
  #(List(CapturedJsonValue), SyntheticIdentityRegistry),
  List(#(List(String), String)),
) {
  let raw_line_items = read_object_list(input, "returnLineItems")
  case raw_line_items {
    [] ->
      Error([
        #(["returnLineItems"], "Return must include at least one line item."),
      ])
    _ -> {
      let #(line_items, user_errors, next_identity) =
        list.index_fold(
          raw_line_items,
          #([], [], identity),
          fn(acc, item, index) {
            let #(items, errors, current_identity) = acc
            let fulfillment_line_item_id =
              read_string(item, "fulfillmentLineItemId")
            let quantity = read_int(item, "quantity", 0)
            let fulfillment_line_item =
              fulfillment_line_item_id
              |> option.then(fn(id) { find_fulfillment_line_item(order, id) })
            case fulfillment_line_item {
              None -> #(
                items,
                list.append(errors, [
                  #(
                    [
                      "returnLineItems",
                      int.to_string(index),
                      "fulfillmentLineItemId",
                    ],
                    "Fulfillment line item does not exist.",
                  ),
                ]),
                current_identity,
              )
              Some(fulfillment_line_item) -> {
                let available_quantity =
                  captured_int_field(fulfillment_line_item, "quantity")
                  |> option.unwrap(0)
                case quantity <= 0 || quantity > available_quantity {
                  True -> #(
                    items,
                    list.append(errors, [
                      #(
                        ["returnLineItems", int.to_string(index), "quantity"],
                        "Quantity is not available for return.",
                      ),
                    ]),
                    current_identity,
                  )
                  False -> {
                    let #(id, next_identity) =
                      synthetic_identity.make_synthetic_gid(
                        current_identity,
                        "ReturnLineItem",
                      )
                    #(
                      list.append(items, [
                        build_return_line_item(id, fulfillment_line_item, item),
                      ]),
                      errors,
                      next_identity,
                    )
                  }
                }
              }
            }
          },
        )
      case user_errors {
        [] -> Ok(#(line_items, next_identity))
        _ -> Error(user_errors)
      }
    }
  }
}

fn build_return_line_item(
  id: String,
  fulfillment_line_item: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let line_item =
    captured_object_field(fulfillment_line_item, "lineItem")
    |> option.unwrap(CapturedNull)
  CapturedObject([
    #("id", CapturedString(id)),
    #(
      "fulfillmentLineItemId",
      captured_field_or_null(fulfillment_line_item, "id"),
    ),
    #("lineItemId", captured_field_or_null(line_item, "id")),
    #("title", captured_field_or_null(line_item, "title")),
    #("quantity", CapturedInt(read_int(input, "quantity", 0))),
    #("processedQuantity", CapturedInt(0)),
    #(
      "returnReason",
      CapturedString(
        read_string(input, "returnReason") |> option.unwrap("UNKNOWN"),
      ),
    ),
    #(
      "returnReasonNote",
      CapturedString(
        read_string(input, "returnReasonNote") |> option.unwrap(""),
      ),
    ),
    #("customerNote", CapturedNull),
  ])
}

fn build_order_return(
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  line_items: List(CapturedJsonValue),
  input: Dict(String, root_field.ResolvedValue),
  status: String,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(return_id, identity_after_return) =
    synthetic_identity.make_synthetic_gid(identity_after_time, "Return")
  let base_return =
    CapturedObject([
      #("id", CapturedString(return_id)),
      #("orderId", CapturedString(order.id)),
      #(
        "name",
        CapturedString(
          captured_string_field(order.data, "name")
          |> option.unwrap("#ORDER")
          <> "-R"
          <> int.to_string(list.length(order_returns(order.data)) + 1),
        ),
      ),
      #("status", CapturedString(status)),
      #(
        "createdAt",
        CapturedString(
          read_string(input, "requestedAt") |> option.unwrap(created_at),
        ),
      ),
      #("closedAt", CapturedNull),
      #("decline", CapturedNull),
      #("totalQuantity", CapturedInt(total_return_quantity(line_items))),
      #("returnLineItems", CapturedArray(line_items)),
      #("reverseFulfillmentOrders", CapturedArray([])),
    ])
  case status {
    "OPEN" ->
      ensure_return_reverse_fulfillment_orders(
        identity_after_return,
        order,
        base_return,
      )
    _ -> #(base_return, identity_after_return)
  }
}

fn ensure_return_reverse_fulfillment_orders(
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  order_return: CapturedJsonValue,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  case order_reverse_fulfillment_orders(order_return) {
    [_, ..] -> #(order_return, identity)
    [] -> {
      let #(reverse_order, next_identity) =
        build_reverse_fulfillment_order(identity, order, order_return)
      #(
        replace_captured_object_fields(order_return, [
          #("reverseFulfillmentOrders", CapturedArray([reverse_order])),
        ]),
        next_identity,
      )
    }
  }
}

fn build_reverse_fulfillment_order(
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  order_return: CapturedJsonValue,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(line_items, identity_after_lines) =
    order_return_line_items(order_return)
    |> list.fold(#([], identity), fn(acc, return_line_item) {
      let #(items, current_identity) = acc
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ReverseFulfillmentOrderLineItem",
        )
      let quantity =
        captured_int_field(return_line_item, "quantity") |> option.unwrap(0)
      let processed_quantity =
        captured_int_field(return_line_item, "processedQuantity")
        |> option.unwrap(0)
      #(
        list.append(items, [
          CapturedObject([
            #("id", CapturedString(id)),
            #(
              "returnLineItemId",
              captured_field_or_null(return_line_item, "id"),
            ),
            #(
              "fulfillmentLineItemId",
              captured_field_or_null(return_line_item, "fulfillmentLineItemId"),
            ),
            #(
              "lineItemId",
              captured_field_or_null(return_line_item, "lineItemId"),
            ),
            #("title", captured_field_or_null(return_line_item, "title")),
            #("totalQuantity", CapturedInt(quantity)),
            #(
              "remainingQuantity",
              CapturedInt(int.max(0, quantity - processed_quantity)),
            ),
            #("disposedQuantity", CapturedInt(0)),
            #("dispositionType", CapturedNull),
            #("dispositionLocationId", CapturedNull),
          ]),
        ]),
        next_identity,
      )
    })
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_lines,
      "ReverseFulfillmentOrder",
    )
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("orderId", CapturedString(order.id)),
      #("returnId", captured_field_or_null(order_return, "id")),
      #("status", CapturedString("OPEN")),
      #("lineItems", CapturedArray(line_items)),
      #("reverseDeliveries", CapturedArray([])),
    ]),
    next_identity,
  )
}

fn sync_reverse_fulfillment_line_items(
  order_return: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let reverse_orders = order_reverse_fulfillment_orders(order_return)
  case reverse_orders {
    [] -> #(order_return, identity)
    _ -> {
      let #(synced_reverse_orders, next_identity) =
        reverse_orders
        |> list.fold(#([], identity), fn(acc, reverse_order) {
          let #(orders, current_identity) = acc
          let #(line_items, line_identity) =
            sync_reverse_fulfillment_order_line_items(
              reverse_order,
              order_return_line_items(order_return),
              current_identity,
            )
          #(
            list.append(orders, [
              replace_captured_object_fields(reverse_order, [
                #("lineItems", CapturedArray(line_items)),
              ]),
            ]),
            line_identity,
          )
        })
      #(
        replace_captured_object_fields(order_return, [
          #("reverseFulfillmentOrders", CapturedArray(synced_reverse_orders)),
        ]),
        next_identity,
      )
    }
  }
}

fn sync_reverse_fulfillment_order_line_items(
  reverse_order: CapturedJsonValue,
  return_line_items: List(CapturedJsonValue),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  return_line_items
  |> list.fold(#([], identity), fn(acc, return_line_item) {
    let #(line_items, current_identity) = acc
    let return_line_item_id = captured_string_field(return_line_item, "id")
    let existing =
      return_line_item_id
      |> option.then(fn(id) {
        reverse_fulfillment_order_line_items(reverse_order)
        |> list.find(fn(line_item) {
          captured_string_field(line_item, "returnLineItemId") == Some(id)
        })
        |> option.from_result
      })
    let #(id, next_identity) = case existing {
      Some(line_item) -> #(
        captured_string_field(line_item, "id") |> option.unwrap(""),
        current_identity,
      )
      None ->
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ReverseFulfillmentOrderLineItem",
        )
    }
    let quantity =
      captured_int_field(return_line_item, "quantity") |> option.unwrap(0)
    let processed_quantity =
      captured_int_field(return_line_item, "processedQuantity")
      |> option.unwrap(0)
    #(
      list.append(line_items, [
        CapturedObject([
          #("id", CapturedString(id)),
          #("returnLineItemId", captured_field_or_null(return_line_item, "id")),
          #(
            "fulfillmentLineItemId",
            captured_field_or_null(return_line_item, "fulfillmentLineItemId"),
          ),
          #(
            "lineItemId",
            captured_field_or_null(return_line_item, "lineItemId"),
          ),
          #("title", captured_field_or_null(return_line_item, "title")),
          #("totalQuantity", CapturedInt(quantity)),
          #(
            "remainingQuantity",
            CapturedInt(int.max(0, quantity - processed_quantity)),
          ),
          #(
            "disposedQuantity",
            existing
              |> option.then(fn(line_item) {
                captured_int_field(line_item, "disposedQuantity")
              })
              |> option.unwrap(0)
              |> CapturedInt,
          ),
          #(
            "dispositionType",
            existing
              |> option.then(fn(line_item) {
                captured_object_field(line_item, "dispositionType")
              })
              |> option.unwrap(CapturedNull),
          ),
          #(
            "dispositionLocationId",
            existing
              |> option.then(fn(line_item) {
                captured_object_field(line_item, "dispositionLocationId")
              })
              |> option.unwrap(CapturedNull),
          ),
        ]),
      ]),
      next_identity,
    )
  })
}

fn stage_order_with_returns(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  returns: List(CapturedJsonValue),
) -> #(Store, SyntheticIdentityRegistry, OrderRecord) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let updated_order =
    OrderRecord(
      ..order,
      data: replace_captured_object_fields(order.data, [
        #("updatedAt", CapturedString(updated_at)),
        #("returns", CapturedArray(returns)),
      ]),
    )
  #(store.stage_order(store, updated_order), next_identity, updated_order)
}

fn find_order_return(
  store: Store,
  return_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case
      order_returns(order.data)
      |> list.find(fn(candidate) {
        captured_string_field(candidate, "id") == Some(return_id)
      })
      |> option.from_result
    {
      Some(order_return) -> Ok(#(order, order_return))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn order_returns(order_data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "returns") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

fn order_return_line_items(
  order_return: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_return, "returnLineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

fn order_reverse_fulfillment_orders(
  order_return: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_return, "reverseFulfillmentOrders") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

fn reverse_fulfillment_order_line_items(
  reverse_fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(reverse_fulfillment_order, "lineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

fn connection_nodes(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_array_values
    _ -> []
  }
}

fn total_return_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, line_item) {
    sum + { captured_int_field(line_item, "quantity") |> option.unwrap(0) }
  })
}

fn find_fulfillment_line_item(
  order: OrderRecord,
  fulfillment_line_item_id: String,
) -> Option(CapturedJsonValue) {
  order_fulfillments(order.data)
  |> list.flat_map(fn(fulfillment) {
    case captured_object_field(fulfillment, "fulfillmentLineItems") {
      Some(value) -> connection_nodes(value)
      None -> []
    }
  })
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(fulfillment_line_item_id)
  })
  |> option.from_result
}

fn return_log_draft(
  root_name: String,
  staged_ids: List(String),
  user_errors: List(#(List(String), String)),
) -> LogDraft {
  let status = case user_errors {
    [] -> store.Staged
    _ -> store.Failed
  }
  single_root_log_draft(
    root_name,
    staged_ids,
    status,
    "orders",
    "stage-locally",
    Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
  )
}

fn serialize_return_mutation_payload(
  field: Selection,
  order_return: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "return" -> #(key, case order_return, order {
              Some(order_return), Some(order) ->
                serialize_order_return(child, order_return, order, fragments)
              _, _ -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_return(
  field: Selection,
  order_return: CapturedJsonValue,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(order_return)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, serialize_order_node(child, order, fragments))
            "totalQuantity" -> #(
              key,
              json.int(
                captured_int_field(order_return, "totalQuantity")
                |> option.unwrap(
                  total_return_quantity(order_return_line_items(order_return)),
                ),
              ),
            )
            "returnLineItems" -> #(
              key,
              serialize_return_line_items_connection(
                child,
                order_return_line_items(order_return),
                fragments,
              ),
            )
            "reverseFulfillmentOrders" -> #(
              key,
              serialize_reverse_fulfillment_orders_connection(
                child,
                order_reverse_fulfillment_orders(order_return),
                order_return,
                order,
                fragments,
              ),
            )
            "decline" -> #(
              key,
              project_graphql_field_value(source, child, fragments),
            )
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_returns_connection(
  field: Selection,
  returns: List(CapturedJsonValue),
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: returns,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(order_return, _index) {
        captured_string_field(order_return, "id") |> option.unwrap("")
      },
      serialize_node: fn(order_return, selection, _index) {
        serialize_order_return(selection, order_return, order, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: True,
        prefix_cursors: False,
        include_cursors: False,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn serialize_return_line_items_connection(
  field: Selection,
  line_items: List(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: line_items,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(line_item, _index) {
        captured_string_field(line_item, "id") |> option.unwrap("")
      },
      serialize_node: fn(line_item, selection, _index) {
        serialize_return_line_item(selection, line_item, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: True,
        prefix_cursors: False,
        include_cursors: False,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn serialize_return_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(line_item)
  let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(0)
  let processed =
    captured_int_field(line_item, "processedQuantity") |> option.unwrap(0)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "processedQuantity" | "refundedQuantity" -> #(
              key,
              json.int(processed),
            )
            "unprocessedQuantity" -> #(
              key,
              json.int(int.max(0, quantity - processed)),
            )
            "quantity" | "refundableQuantity" | "processableQuantity" -> #(
              key,
              json.int(quantity),
            )
            "fulfillmentLineItem" -> #(
              key,
              serialize_return_fulfillment_line_item(
                child,
                line_item,
                fragments,
              ),
            )
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_return_fulfillment_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #(
        "id",
        captured_string_field(line_item, "fulfillmentLineItemId")
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "quantity",
        SrcInt(captured_int_field(line_item, "quantity") |> option.unwrap(0)),
      ),
      #(
        "lineItem",
        src_object([
          #(
            "id",
            captured_string_field(line_item, "lineItemId")
              |> option.map(SrcString)
              |> option.unwrap(SrcNull),
          ),
          #(
            "title",
            captured_string_field(line_item, "title")
              |> option.map(SrcString)
              |> option.unwrap(SrcNull),
          ),
        ]),
      ),
    ])
  project_graphql_value(source, selection_children(field), fragments)
}

fn serialize_reverse_fulfillment_orders_connection(
  field: Selection,
  reverse_orders: List(CapturedJsonValue),
  order_return: CapturedJsonValue,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: reverse_orders,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(reverse_order, _index) {
        captured_string_field(reverse_order, "id") |> option.unwrap("")
      },
      serialize_node: fn(reverse_order, selection, _index) {
        serialize_reverse_fulfillment_order(
          selection,
          reverse_order,
          order_return,
          order,
          fragments,
        )
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: True,
        prefix_cursors: False,
        include_cursors: False,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn serialize_reverse_fulfillment_order(
  field: Selection,
  reverse_order: CapturedJsonValue,
  order_return: CapturedJsonValue,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(reverse_order)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "return" -> #(
              key,
              serialize_order_return(child, order_return, order, fragments),
            )
            "order" -> #(key, serialize_order_node(child, order, fragments))
            "lineItems" | "reverseFulfillmentOrderLineItems" -> #(
              key,
              serialize_reverse_fulfillment_order_line_items_connection(
                child,
                reverse_fulfillment_order_line_items(reverse_order),
                order_return,
                fragments,
              ),
            )
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_reverse_fulfillment_order_line_items_connection(
  field: Selection,
  line_items: List(CapturedJsonValue),
  order_return: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: line_items,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(line_item, _index) {
        captured_string_field(line_item, "id") |> option.unwrap("")
      },
      serialize_node: fn(line_item, selection, _index) {
        serialize_reverse_fulfillment_order_line_item(
          selection,
          line_item,
          order_return,
          fragments,
        )
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: True,
        prefix_cursors: False,
        include_cursors: False,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn serialize_reverse_fulfillment_order_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  order_return: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(line_item)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "returnLineItem" -> #(
              key,
              case captured_string_field(line_item, "returnLineItemId") {
                Some(id) ->
                  order_return_line_items(order_return)
                  |> list.find(fn(item) {
                    captured_string_field(item, "id") == Some(id)
                  })
                  |> option.from_result
                  |> option.map(fn(return_line_item) {
                    serialize_return_line_item(
                      child,
                      return_line_item,
                      fragments,
                    )
                  })
                  |> option.unwrap(json.null())
                None -> json.null()
              },
            )
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn order_edit_add_custom_item(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let #(line_item, next_identity) =
    build_order_edit_custom_line_item(identity, args)
  let line_items =
    list.append(order_edit_session_line_items(session), [line_item])
  let added_line_items =
    list.append(order_edit_session_added_line_items(session), [line_item])
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #(
        "addedLineItems",
        CapturedObject([#("nodes", CapturedArray(added_line_items))]),
      ),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      Some(line_item),
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

fn order_edit_add_line_item_discount(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let line_item_id = read_string(args, "lineItemId")
  let discount = read_object(args, "discount") |> option.unwrap(dict.new())
  let description = read_string(discount, "description") |> option.unwrap("")
  let fixed_value =
    read_object(discount, "fixedValue") |> option.unwrap(discount)
  let discount_amount = read_number(fixed_value, "amount") |> option.unwrap(0.0)
  let currency_code =
    read_string(fixed_value, "currencyCode") |> option.unwrap("CAD")
  let #(staged_change_id, identity_after_change) =
    synthetic_identity.make_synthetic_gid(
      identity,
      "OrderStagedChangeAddLineItemDiscount",
    )
  let #(discount_application_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_change,
      "CalculatedManualDiscountApplication",
    )
  let staged_change =
    CapturedObject([
      #("id", CapturedString(staged_change_id)),
      #("description", CapturedString(description)),
    ])
  let line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) {
      case captured_string_field(line_item, "id") == line_item_id {
        True ->
          apply_order_edit_line_discount(
            line_item,
            discount_amount,
            currency_code,
            description,
            discount_application_id,
          )
        False -> line_item
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let calculated_line_item =
    line_item_id
    |> option.then(fn(id) { find_calculated_line_item(line_items, id) })
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      calculated_line_item,
      None,
      Some(staged_change),
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

fn order_edit_remove_discount(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let discount_application_id = read_string(args, "discountApplicationId")
  let line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) {
      case
        order_edit_line_item_has_discount(line_item, discount_application_id)
      {
        True -> remove_order_edit_line_discount(line_item)
        False -> line_item
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

fn order_edit_add_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_input =
    read_object(args, "shippingLine") |> option.unwrap(dict.new())
  let #(shipping_line, next_identity) =
    build_order_edit_shipping_line(identity, shipping_input, "ADDED")
  let shipping_lines =
    list.append(order_edit_session_shipping_lines(session), [shipping_line])
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      Some(shipping_line),
      None,
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

fn order_edit_update_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_line_id = read_string(args, "shippingLineId")
  let shipping_input =
    read_object(args, "shippingLine") |> option.unwrap(dict.new())
  let shipping_lines =
    order_edit_session_shipping_lines(session)
    |> list.map(fn(shipping_line) {
      case captured_string_field(shipping_line, "id") == shipping_line_id {
        True -> update_order_edit_shipping_line(shipping_line, shipping_input)
        False -> shipping_line
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

fn order_edit_remove_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_line_id = read_string(args, "shippingLineId")
  let shipping_lines =
    order_edit_session_shipping_lines(session)
    |> list.filter(fn(shipping_line) {
      captured_string_field(shipping_line, "id") != shipping_line_id
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

fn build_calculated_order_from_order(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedOrder")
  let #(line_items, next_identity) =
    build_calculated_line_items(
      order_line_items(order.data),
      identity_after_order,
    )
  let subtotal = order_edit_line_items_total(line_items)
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #(
        "originalOrder",
        CapturedObject([
          #("id", CapturedString(order.id)),
          #(
            "name",
            optional_captured_string(captured_string_field(order.data, "name")),
          ),
        ]),
      ),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #("addedLineItems", CapturedObject([#("nodes", CapturedArray([]))])),
      #("shippingLines", CapturedArray(order_edit_shipping_lines(order))),
      #(
        "subtotalLineItemsQuantity",
        CapturedInt(order_edit_line_items_quantity(line_items)),
      ),
      #("subtotalPriceSet", money_set(subtotal, "CAD")),
      #("totalPriceSet", money_set(subtotal, "CAD")),
    ]),
    next_identity,
  )
}

fn stage_order_edit_session(
  store: Store,
  order: OrderRecord,
  calculated_order: CapturedJsonValue,
) -> Store {
  let session = order_edit_session_record(order.id, calculated_order)
  store.stage_order(store, upsert_order_edit_session(order, session))
}

fn order_edit_session_record(
  order_id: String,
  calculated_order: CapturedJsonValue,
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "id",
      optional_captured_string(captured_string_field(calculated_order, "id")),
    ),
    #("originalOrderId", CapturedString(order_id)),
    #(
      "lineItems",
      captured_object_field(calculated_order, "lineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "addedLineItems",
      captured_object_field(calculated_order, "addedLineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "shippingLines",
      captured_object_field(calculated_order, "shippingLines")
        |> option.unwrap(CapturedArray([])),
    ),
  ])
}

fn upsert_order_edit_session(
  order: OrderRecord,
  session: CapturedJsonValue,
) -> OrderRecord {
  let session_id = captured_string_field(session, "id") |> option.unwrap("")
  let existing =
    order_edit_sessions(order)
    |> list.filter(fn(existing_session) {
      captured_string_field(existing_session, "id") != Some(session_id)
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("orderEditSessions", CapturedArray(list.append(existing, [session]))),
    ]),
  )
}

fn remove_order_edit_session(
  order: OrderRecord,
  calculated_order_id: Option(String),
) -> OrderRecord {
  let remaining =
    order_edit_sessions(order)
    |> list.filter(fn(session) {
      captured_string_field(session, "id") != calculated_order_id
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("orderEditSessions", CapturedArray(remaining)),
    ]),
  )
}

fn order_edit_sessions(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "orderEditSessions") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

fn find_order_edit_session(
  store: Store,
  calculated_order_id: Option(String),
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  case calculated_order_id {
    None -> None
    Some(id) ->
      store.list_effective_orders(store)
      |> list.find_map(fn(order) {
        case
          order_edit_sessions(order)
          |> list.find(fn(session) {
            captured_string_field(session, "id") == Some(id)
          })
        {
          Ok(session) -> Ok(#(order, session))
          Error(_) -> Error(Nil)
        }
      })
      |> option.from_result
  }
}

fn find_order_edit_session_line_item(
  store: Store,
  calculated_order_id: Option(String),
  line_item_id: Option(String),
) -> Option(CapturedJsonValue) {
  case find_order_edit_session(store, calculated_order_id), line_item_id {
    Some(match), Some(line_item_id) -> {
      let #(_, session) = match
      order_edit_session_line_items(session)
      |> list.find(fn(line_item) {
        captured_string_field(line_item, "id") == Some(line_item_id)
      })
      |> option.from_result
    }
    _, _ -> None
  }
}

fn order_edit_session_line_items(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

fn order_edit_session_added_line_items(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "addedLineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

fn order_edit_session_shipping_lines(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "shippingLines") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

fn stage_updated_order_edit_session(
  store: Store,
  order: OrderRecord,
  session: CapturedJsonValue,
) -> #(Store, CapturedJsonValue) {
  let updated_order = upsert_order_edit_session(order, session)
  #(
    store.stage_order(store, updated_order),
    calculated_order_from_session(session, updated_order),
  )
}

fn build_order_edit_custom_line_item(
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedLineItem")
  let price = read_object(args, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  let quantity = read_int(args, "quantity", 1)
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", optional_captured_string(read_string(args, "title"))),
      #("quantity", CapturedInt(quantity)),
      #("currentQuantity", CapturedInt(quantity)),
      #("sku", CapturedNull),
      #("variant", CapturedNull),
      #("originalUnitPriceSet", money_set(amount, currency_code)),
      #("discountedUnitPriceSet", money_set(amount, currency_code)),
    ]),
    next_identity,
  )
}

fn apply_order_edit_line_discount(
  line_item: CapturedJsonValue,
  discount_amount: Float,
  currency_code: String,
  description: String,
  discount_application_id: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(1)
  let original = captured_money_amount(line_item, "originalUnitPriceSet")
  let discounted = max_float(0.0, original -. discount_amount)
  let allocated = discount_amount *. int.to_float(quantity)
  replace_captured_object_fields(line_item, [
    #("hasStagedLineItemDiscount", CapturedBool(True)),
    #("discountedUnitPriceSet", money_set(discounted, currency_code)),
    #(
      "calculatedDiscountAllocations",
      CapturedArray([
        CapturedObject([
          #("allocatedAmountSet", money_set(allocated, currency_code)),
          #(
            "discountApplication",
            CapturedObject([
              #("id", CapturedString(discount_application_id)),
              #("description", CapturedString(description)),
            ]),
          ),
        ]),
      ]),
    ),
  ])
}

fn remove_order_edit_line_discount(
  line_item: CapturedJsonValue,
) -> CapturedJsonValue {
  let original =
    captured_object_field(line_item, "originalUnitPriceSet")
    |> option.unwrap(money_set(0.0, "CAD"))
  replace_captured_object_fields(line_item, [
    #("hasStagedLineItemDiscount", CapturedBool(False)),
    #("calculatedDiscountAllocations", CapturedArray([])),
    #("discountedUnitPriceSet", original),
  ])
}

fn order_edit_line_item_has_discount(
  line_item: CapturedJsonValue,
  discount_application_id: Option(String),
) -> Bool {
  case discount_application_id {
    None -> False
    Some(id) ->
      case captured_object_field(line_item, "calculatedDiscountAllocations") {
        Some(CapturedArray(allocations)) ->
          allocations
          |> list.any(fn(allocation) {
            captured_object_field(allocation, "discountApplication")
            |> option.then(fn(application) {
              captured_string_field(application, "id")
            })
            == Some(id)
          })
        _ -> False
      }
  }
}

fn find_calculated_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Option(CapturedJsonValue) {
  line_items
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(id)
  })
  |> option.from_result
}

fn build_order_edit_shipping_line(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  staged_status: String,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedShippingLine")
  let price = read_object(input, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", optional_captured_string(read_string(input, "title"))),
      #("stagedStatus", CapturedString(staged_status)),
      #("price", money_set(amount, currency_code)),
    ]),
    next_identity,
  )
}

fn update_order_edit_shipping_line(
  shipping_line: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let price = read_object(input, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  replace_captured_object_fields(shipping_line, [
    #("title", optional_captured_string(read_string(input, "title"))),
    #("price", money_set(amount, currency_code)),
  ])
}

fn order_edit_shipping_lines(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "shippingLines") {
    Some(CapturedObject(fields)) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_array_values
    Some(CapturedArray(items)) -> items
    _ -> []
  }
}

fn captured_array_values(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  case value {
    CapturedArray(values) -> values
    _ -> []
  }
}

fn order_edit_line_items_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, line_item) {
    let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(0)
    sum + quantity
  })
}

fn order_edit_line_items_total(line_items: List(CapturedJsonValue)) -> Float {
  line_items
  |> list.fold(0.0, fn(sum, line_item) {
    let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(0)
    let unit =
      captured_object_field(line_item, "discountedUnitPriceSet")
      |> option.map(captured_money_value)
      |> option.unwrap(captured_money_amount(line_item, "originalUnitPriceSet"))
    sum +. unit *. int.to_float(quantity)
  })
}

fn order_edit_shipping_lines_total(
  shipping_lines: List(CapturedJsonValue),
) -> Float {
  shipping_lines
  |> list.fold(0.0, fn(sum, shipping_line) {
    sum +. captured_money_amount(shipping_line, "price")
  })
}

fn update_order_edit_session_with_line_item(
  store: Store,
  calculated_order_id: Option(String),
  calculated_line_item: CapturedJsonValue,
) -> #(Store, Option(CapturedJsonValue)) {
  case find_order_edit_session(store, calculated_order_id) {
    None -> #(store, None)
    Some(match) -> {
      let #(order, session) = match
      let line_items =
        list.append(order_edit_session_line_items(session), [
          calculated_line_item,
        ])
      let added_line_items =
        list.append(order_edit_session_added_line_items(session), [
          calculated_line_item,
        ])
      let updated_session =
        replace_captured_object_fields(session, [
          #(
            "lineItems",
            CapturedObject([#("nodes", CapturedArray(line_items))]),
          ),
          #(
            "addedLineItems",
            CapturedObject([#("nodes", CapturedArray(added_line_items))]),
          ),
        ])
      let updated_order = upsert_order_edit_session(order, updated_session)
      #(
        store.stage_order(store, updated_order),
        Some(calculated_order_from_session(updated_session, updated_order)),
      )
    }
  }
}

fn update_order_edit_session_line_item_quantity(
  store: Store,
  calculated_order_id: Option(String),
  line_item_id: Option(String),
  quantity: Int,
) -> #(Store, Option(CapturedJsonValue)) {
  case find_order_edit_session(store, calculated_order_id), line_item_id {
    Some(match), Some(line_item_id) -> {
      let #(order, session) = match
      let line_items =
        order_edit_session_line_items(session)
        |> list.map(fn(line_item) {
          case captured_string_field(line_item, "id") == Some(line_item_id) {
            True ->
              replace_captured_object_fields(line_item, [
                #("quantity", CapturedInt(quantity)),
                #("currentQuantity", CapturedInt(quantity)),
              ])
            False -> line_item
          }
        })
      let updated_session =
        replace_captured_object_fields(session, [
          #(
            "lineItems",
            CapturedObject([#("nodes", CapturedArray(line_items))]),
          ),
        ])
      let updated_order = upsert_order_edit_session(order, updated_session)
      #(
        store.stage_order(store, updated_order),
        Some(calculated_order_from_session(updated_session, updated_order)),
      )
    }
    _, _ -> #(store, None)
  }
}

fn calculated_order_from_session(
  session: CapturedJsonValue,
  order: OrderRecord,
) -> CapturedJsonValue {
  let line_items = order_edit_session_line_items(session)
  let shipping_lines = order_edit_session_shipping_lines(session)
  let subtotal = order_edit_line_items_total(line_items)
  let shipping_total = order_edit_shipping_lines_total(shipping_lines)
  CapturedObject([
    #("id", captured_field_or_null(session, "id")),
    #(
      "originalOrder",
      CapturedObject([
        #("id", CapturedString(order.id)),
        #("name", captured_field_or_null(order.data, "name")),
      ]),
    ),
    #(
      "lineItems",
      captured_object_field(session, "lineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "addedLineItems",
      captured_object_field(session, "addedLineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #("shippingLines", CapturedArray(shipping_lines)),
    #(
      "subtotalLineItemsQuantity",
      CapturedInt(order_edit_line_items_quantity(line_items)),
    ),
    #("subtotalPriceSet", money_set(subtotal, "CAD")),
    #("totalPriceSet", money_set(subtotal +. shipping_total, "CAD")),
  ])
}

fn commit_order_edit_session(
  order: OrderRecord,
  session: CapturedJsonValue,
  updated_at: String,
) -> OrderRecord {
  let committed_line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) { commit_order_edit_line_item(order, line_item) })
  let current_quantity =
    committed_line_items
    |> list.fold(0, fn(sum, line_item) {
      let quantity =
        captured_int_field(line_item, "currentQuantity") |> option.unwrap(0)
      sum + quantity
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("updatedAt", CapturedString(updated_at)),
      #("currentSubtotalLineItemsQuantity", CapturedInt(current_quantity)),
      #(
        "lineItems",
        CapturedObject([#("nodes", CapturedArray(committed_line_items))]),
      ),
    ]),
  )
}

fn commit_order_edit_line_item(
  order: OrderRecord,
  calculated_line_item: CapturedJsonValue,
) -> CapturedJsonValue {
  let calculated_id = captured_string_field(calculated_line_item, "id")
  let original_line_item =
    calculated_id
    |> option.then(fn(id) {
      find_order_edit_line_item_by_calculated_id_in_order(order, id)
    })
  case original_line_item {
    Some(original) ->
      replace_captured_object_fields(original, [
        #(
          "currentQuantity",
          captured_field_or_int(calculated_line_item, "currentQuantity", 0),
        ),
      ])
    None ->
      CapturedObject([
        #("id", optional_captured_string(calculated_id)),
        #("title", captured_field_or_null(calculated_line_item, "title")),
        #(
          "quantity",
          captured_field_or_int(calculated_line_item, "quantity", 0),
        ),
        #(
          "currentQuantity",
          captured_field_or_int(calculated_line_item, "currentQuantity", 0),
        ),
        #("sku", captured_field_or_null(calculated_line_item, "sku")),
        #("variant", captured_field_or_null(calculated_line_item, "variant")),
        #(
          "originalUnitPriceSet",
          captured_field_or_money(
            calculated_line_item,
            "originalUnitPriceSet",
            "CAD",
          ),
        ),
      ])
  }
}

fn find_order_edit_line_item_by_calculated_id_in_order(
  order: OrderRecord,
  calculated_line_item_id: String,
) -> Option(CapturedJsonValue) {
  let index = calculated_line_item_index(calculated_line_item_id)
  case index {
    Some(index) -> list_item_at(order_line_items(order.data), index)
    None -> None
  }
}

fn build_calculated_line_items(
  line_items: List(CapturedJsonValue),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  line_items
  |> list.fold(#([], identity), fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "CalculatedLineItem",
      )
    let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
    let current_quantity =
      captured_int_field(item, "currentQuantity") |> option.unwrap(quantity)
    let calculated_item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("title", captured_field_or_null(item, "title")),
        #("quantity", CapturedInt(quantity)),
        #("currentQuantity", CapturedInt(current_quantity)),
        #("sku", captured_field_or_null(item, "sku")),
        #("variant", captured_field_or_null(item, "variant")),
        #(
          "originalUnitPriceSet",
          captured_field_or_money(item, "originalUnitPriceSet", "CAD"),
        ),
      ])
    #(list.append(items, [calculated_item]), next_identity)
  })
}

fn build_added_calculated_line_item(
  variant: ProductVariantRecord,
  product: Option(ProductRecord),
  quantity: Int,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedLineItem")
  let title =
    product
    |> option.map(fn(product) { product.title })
    |> option.unwrap(variant.title)
  let amount =
    variant.price
    |> option.map(parse_amount)
    |> option.map(format_decimal_amount)
    |> option.unwrap("0.0")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", CapturedString(title)),
      #("quantity", CapturedInt(quantity)),
      #("currentQuantity", CapturedInt(quantity)),
      #("sku", optional_captured_string(variant.sku)),
      #(
        "variant",
        CapturedObject([
          #("id", CapturedString(variant.id)),
        ]),
      ),
      #("originalUnitPriceSet", money_set_string(amount, "CAD")),
    ]),
    next_identity,
  )
}

fn build_set_quantity_calculated_line_item(
  line_item: CapturedJsonValue,
  quantity: Int,
) -> CapturedJsonValue {
  CapturedObject([
    #("title", captured_field_or_null(line_item, "title")),
    #("quantity", CapturedInt(quantity)),
    #("currentQuantity", CapturedInt(quantity)),
    #("sku", captured_field_or_null(line_item, "sku")),
    #("variant", captured_field_or_null(line_item, "variant")),
    #(
      "originalUnitPriceSet",
      captured_field_or_money(line_item, "originalUnitPriceSet", "CAD"),
    ),
  ])
}

fn find_order_edit_line_item_by_calculated_id(
  store: Store,
  calculated_line_item_id: String,
) -> Option(CapturedJsonValue) {
  let index = calculated_line_item_index(calculated_line_item_id)
  case index {
    Some(index) ->
      store.list_effective_orders(store)
      |> list.find_map(fn(order) {
        case list_item_at(order_line_items(order.data), index) {
          Some(item) -> Ok(item)
          None -> Error(Nil)
        }
      })
      |> option.from_result
    None -> None
  }
}

fn calculated_line_item_index(calculated_line_item_id: String) -> Option(Int) {
  let tail = draft_order_gid_tail(calculated_line_item_id)
  case int.parse(tail) {
    Ok(value) if value >= 2 -> Some(value - 2)
    _ -> None
  }
}

fn list_item_at(items: List(a), index: Int) -> Option(a) {
  case items, index {
    [], _ -> None
    [item, ..], 0 -> Some(item)
    [_, ..rest], n if n > 0 -> list_item_at(rest, n - 1)
    _, _ -> None
  }
}

fn serialize_order_edit_begin_payload(
  field: Selection,
  calculated_order: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_order),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(
                child,
                captured_string_field(calculated_order, "id")
                  |> option.map(order_edit_session_id_from_calculated_id)
                  |> option.unwrap(""),
              ),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_edit_add_variant_payload(
  field: Selection,
  calculated_line_item: CapturedJsonValue,
  calculated_order: Option(CapturedJsonValue),
  session_id: String,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_line_item),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(child, session_id),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_edit_add_variant_invalid_variant_payload(
  field: Selection,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(key, json.null())
            "calculatedLineItem" -> #(key, json.null())
            "orderEditSession" -> #(key, json.null())
            "userErrors" -> #(
              key,
              json.array([order_edit_invalid_variant_user_error()], fn(error) {
                error
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn order_edit_invalid_variant_user_error() -> Json {
  json.object([
    #("field", json.array(["variantId"], json.string)),
    #(
      "message",
      json.string(
        "can't convert Integer[0] to a positive Integer to use as an untrusted id",
      ),
    ),
  ])
}

fn serialize_order_edit_set_quantity_payload(
  field: Selection,
  calculated_line_item: CapturedJsonValue,
  calculated_order: Option(CapturedJsonValue),
  calculated_order_id: Option(String),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_line_item),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(
                child,
                calculated_order_id
                  |> option.map(order_edit_session_id_from_calculated_id)
                  |> option.unwrap(""),
              ),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_edit_commit_payload(
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, serialize_order_node(child, order, fragments))
            "successMessages" -> #(
              key,
              json.array(["Order updated"], json.string),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_edit_residual_payload(
  field: Selection,
  calculated_order: Option(CapturedJsonValue),
  calculated_line_item: Option(CapturedJsonValue),
  calculated_shipping_line: Option(CapturedJsonValue),
  staged_change: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              serialize_captured_selection(
                child,
                calculated_line_item,
                fragments,
              ),
            )
            "calculatedShippingLine" -> #(
              key,
              serialize_captured_selection(
                child,
                calculated_shipping_line,
                fragments,
              ),
            )
            "addedDiscountStagedChange" -> #(
              key,
              serialize_captured_selection(child, staged_change, fragments),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_order_edit_session(field: Selection, session_id: String) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "id" -> #(key, json.string(session_id))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn order_edit_session_id_from_calculated_id(id: String) -> String {
  string.replace(id, "/CalculatedOrder/", "/OrderEditSession/")
}

fn handle_fulfillment_create_invalid_id_guardrail(
  root_name: String,
) -> #(String, Json, List(Json)) {
  #(root_name, json.null(), [
    json.object([
      #("message", json.string("invalid id")),
      #(
        "extensions",
        json.object([#("code", json.string("RESOURCE_NOT_FOUND"))]),
      ),
      #("path", json.array([root_name], json.string)),
    ]),
  ])
}

fn handle_order_update_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let errors = case field {
    Field(arguments: arguments, ..) ->
      case find_argument(arguments, "input") {
        Some(input_argument) ->
          case input_argument.value {
            ObjectValue(fields: fields, ..) ->
              validate_order_update_inline_input(operation_path, fields)
            VariableValue(variable: variable) ->
              validate_order_update_variable_input(
                variable.name.value,
                variables,
              )
            _ -> []
          }
        None -> []
      }
    _ -> []
  }
  case errors {
    [_, ..] -> #(key, json.null(), store, identity, [], errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) ->
          case read_string(input, "id") {
            Some(id) ->
              case store.get_order_by_id(store, id) {
                None -> #(
                  key,
                  serialize_order_mutation_error_payload(field, [
                    #(["id"], "Order does not exist"),
                  ]),
                  store,
                  identity,
                  [],
                  [],
                  [],
                )
                Some(order) -> {
                  let #(updated_order, next_identity) =
                    build_updated_order(order, input, identity)
                  let next_store = store.stage_order(store, updated_order)
                  let payload =
                    serialize_order_mutation_payload(
                      field,
                      Some(updated_order),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "orderUpdate",
                      [id],
                      store.Staged,
                      "orders",
                      "stage-locally",
                      Some("Locally staged orderUpdate in shopify-draft-proxy."),
                    )
                  #(key, payload, next_store, next_identity, [id], [], [draft])
                }
              }
            None -> #(key, json.null(), store, identity, [], [], [])
          }
        _ -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn handle_refund_create_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "refundCreate",
      [RequiredArgument(name: "input", expected_type: "RefundInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_object(args, "input") {
        None -> #(
          key,
          serialize_refund_create_payload(
            field,
            None,
            None,
            [
              #(Some(["input"]), "Input is required."),
            ],
            fragments,
          ),
          store,
          identity,
          [],
          [],
          [refund_create_log_draft([], store.Failed)],
        )
        Some(input) -> {
          let order_id = read_string(input, "orderId")
          let order = case order_id {
            Some(id) -> store.get_order_by_id(store, id)
            None -> None
          }
          case order {
            None -> #(
              key,
              serialize_refund_create_payload(
                field,
                None,
                None,
                [
                  #(Some(["input", "orderId"]), "Order does not exist"),
                ],
                fragments,
              ),
              store,
              identity,
              [],
              [],
              [refund_create_log_draft([], store.Failed)],
            )
            Some(order) -> {
              let refund_amount = refund_create_requested_amount(input, order)
              let already_refunded = sum_order_refunded_amount(order)
              let refundable_amount =
                order_total_price(order) -. already_refunded
              let allow_over_refunding =
                read_bool(input, "allowOverRefunding", False)
              case !allow_over_refunding && refund_amount >. refundable_amount {
                True -> {
                  let message =
                    "Refund amount $"
                    <> float_to_fixed_2(refund_amount)
                    <> " is greater than net payment received $"
                    <> float_to_fixed_2(refundable_amount)
                  #(
                    key,
                    serialize_refund_create_payload(
                      field,
                      None,
                      Some(order),
                      [
                        #(None, message),
                      ],
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                    [],
                    [refund_create_log_draft([order.id], store.Failed)],
                  )
                }
                False -> {
                  let #(refund, refund_transaction, next_identity) =
                    build_refund_from_input(order, input, identity)
                  let updated_order =
                    apply_refund_to_order(order, refund, refund_transaction)
                  let next_store = store.stage_order(store, updated_order)
                  let payload =
                    serialize_refund_create_payload(
                      field,
                      Some(refund),
                      Some(updated_order),
                      [],
                      fragments,
                    )
                  #(key, payload, next_store, next_identity, [order.id], [], [
                    refund_create_log_draft([order.id], store.Staged),
                  ])
                }
              }
            }
          }
        }
      }
    }
  }
}

fn refund_create_log_draft(
  staged_resource_ids: List(String),
  status: store.EntryStatus,
) -> LogDraft {
  single_root_log_draft(
    "refundCreate",
    staged_resource_ids,
    status,
    "orders",
    "stage-locally",
    Some("Locally handled refundCreate parity slice."),
  )
}

fn serialize_refund_create_payload(
  field: Selection,
  refund: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(Option(List(String)), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "refund" -> #(key, case refund {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
              None -> json.null()
            })
            "order" -> #(key, case order {
              Some(record) -> serialize_order_node(child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn build_refund_from_input(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, CapturedJsonValue, SyntheticIdentityRegistry) {
  let currency_code = order_currency_code(order)
  let #(refund_id, identity_after_refund) =
    synthetic_identity.make_synthetic_gid(identity, "Refund")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_refund)
  let #(refund_line_items, identity_after_lines) =
    build_refund_line_items(order, input, currency_code, identity_after_time)
  let refund_amount = refund_create_requested_amount(input, order)
  let shipping_amount = refund_shipping_amount(input, order)
  let #(transaction, next_identity) =
    build_refund_transaction(
      input,
      refund_amount,
      currency_code,
      identity_after_lines,
    )
  let refund =
    CapturedObject([
      #("id", CapturedString(refund_id)),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #(
        "totalRefundedSet",
        money_set_string(format_decimal_amount(refund_amount), currency_code),
      ),
      #(
        "totalRefundedShippingSet",
        money_set_string(format_decimal_amount(shipping_amount), currency_code),
      ),
      #(
        "refundLineItems",
        CapturedObject([#("nodes", CapturedArray(refund_line_items))]),
      ),
      #(
        "transactions",
        CapturedObject([#("nodes", CapturedArray([transaction]))]),
      ),
    ])
  #(refund, transaction, next_identity)
}

fn build_refund_line_items(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  read_object_list(input, "refundLineItems")
  |> list.fold(#([], identity), fn(acc, refund_line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "RefundLineItem")
    let quantity = read_int(refund_line_item, "quantity", 0)
    let restock_type =
      read_string(refund_line_item, "restockType")
      |> option.unwrap("NO_RESTOCK")
    let line_item =
      read_string(refund_line_item, "lineItemId")
      |> option.then(fn(line_item_id) {
        find_order_line_item(order, line_item_id)
      })
    let subtotal = case restock_type {
      "NO_RESTOCK" -> 0.0
      _ ->
        line_item
        |> option.map(fn(item) {
          captured_money_amount(item, "originalUnitPriceSet")
          *. int.to_float(quantity)
        })
        |> option.unwrap(0.0)
    }
    let restocked = case restock_type {
      "NO_RESTOCK" -> False
      _ -> True
    }
    let item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("quantity", CapturedInt(quantity)),
        #("restockType", CapturedString(restock_type)),
        #("restocked", CapturedBool(restocked)),
        #("lineItem", refund_line_item_reference(line_item)),
        #(
          "subtotalSet",
          money_set_string(format_decimal_amount(subtotal), currency_code),
        ),
      ])
    #(list.append(items, [item]), next_identity)
  })
}

fn refund_line_item_reference(
  line_item: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case line_item {
    Some(item) ->
      CapturedObject([
        #("id", optional_captured_string(captured_string_field(item, "id"))),
        #(
          "title",
          optional_captured_string(captured_string_field(item, "title")),
        ),
      ])
    None -> CapturedNull
  }
}

fn build_refund_transaction(
  input: Dict(String, root_field.ResolvedValue),
  fallback_amount: Float,
  currency_code: String,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let transaction_input = case read_object_list(input, "transactions") {
    [first, ..] -> first
    [] -> dict.new()
  }
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let amount =
    refund_transaction_amount(transaction_input)
    |> nonzero_float(fallback_amount)
  let transaction =
    CapturedObject([
      #("id", CapturedString(id)),
      #(
        "kind",
        CapturedString(
          read_string(transaction_input, "kind") |> option.unwrap("REFUND"),
        ),
      ),
      #(
        "status",
        CapturedString(
          read_string(transaction_input, "status") |> option.unwrap("SUCCESS"),
        ),
      ),
      #(
        "gateway",
        CapturedString(
          read_string(transaction_input, "gateway") |> option.unwrap("manual"),
        ),
      ),
      #(
        "amountSet",
        money_set_string(format_decimal_amount(amount), currency_code),
      ),
    ])
  #(transaction, next_identity)
}

fn nonzero_float(value: Float, fallback: Float) -> Float {
  case value >. 0.0 {
    True -> value
    False -> fallback
  }
}

fn apply_refund_to_order(
  order: OrderRecord,
  refund: CapturedJsonValue,
  refund_transaction: CapturedJsonValue,
) -> OrderRecord {
  let currency_code = order_currency_code(order)
  let total_refunded =
    sum_order_refunded_amount(order)
    +. captured_money_amount(refund, "totalRefundedSet")
  let shipping_refunded =
    sum_order_refunded_shipping_amount(order)
    +. captured_money_amount(refund, "totalRefundedShippingSet")
  let total = order_total_price(order)
  let display_status = case total_refunded >=. total && total >. 0.0 {
    True -> "REFUNDED"
    False -> "PARTIALLY_REFUNDED"
  }
  let updated_data =
    order.data
    |> replace_captured_object_fields([
      #("displayFinancialStatus", CapturedString(display_status)),
      #(
        "totalRefundedSet",
        money_set_string(format_decimal_amount(total_refunded), currency_code),
      ),
      #(
        "totalRefundedShippingSet",
        money_set_string(
          format_decimal_amount(shipping_refunded),
          currency_code,
        ),
      ),
      #(
        "refunds",
        CapturedArray(list.append(order_refunds(order.data), [refund])),
      ),
      #(
        "transactions",
        CapturedArray(
          list.append(order_transactions(order), [refund_transaction]),
        ),
      ),
    ])
  OrderRecord(..order, data: updated_data)
}

fn refund_create_requested_amount(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  let transaction_total =
    read_object_list(input, "transactions")
    |> list.fold(0.0, fn(sum, transaction) {
      sum +. refund_transaction_amount(transaction)
    })
  case transaction_total >. 0.0 {
    True -> transaction_total
    False ->
      refund_line_item_subtotal(input, order)
      +. refund_shipping_amount(input, order)
  }
}

fn refund_transaction_amount(
  transaction: Dict(String, root_field.ResolvedValue),
) -> Float {
  read_number(transaction, "amount")
  |> option.or(
    read_object(transaction, "amountSet")
    |> option.then(fn(amount_set) { read_object(amount_set, "shopMoney") })
    |> option.then(fn(shop_money) { read_number(shop_money, "amount") }),
  )
  |> option.unwrap(0.0)
}

fn refund_line_item_subtotal(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  read_object_list(input, "refundLineItems")
  |> list.fold(0.0, fn(sum, refund_line_item) {
    let quantity = read_int(refund_line_item, "quantity", 0)
    let line_item_id = read_string(refund_line_item, "lineItemId")
    sum
    +. case line_item_id {
      Some(id) ->
        find_order_line_item(order, id)
        |> option.map(fn(line_item) {
          captured_money_amount(line_item, "originalUnitPriceSet")
          *. int.to_float(quantity)
        })
        |> option.unwrap(0.0)
      None -> 0.0
    }
  })
}

fn order_line_items(order_data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

fn find_order_line_item(
  order: OrderRecord,
  line_item_id: String,
) -> Option(CapturedJsonValue) {
  order_line_items(order.data)
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(line_item_id)
  })
  |> option.from_result
}

fn refund_shipping_amount(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  case read_object(input, "shipping") {
    Some(shipping) ->
      case read_bool(shipping, "fullRefund", False) {
        True -> order_shipping_total(order)
        False -> read_number(shipping, "amount") |> option.unwrap(0.0)
      }
    None -> 0.0
  }
}

fn order_shipping_total(order: OrderRecord) -> Float {
  case captured_object_field(order.data, "shippingLines") {
    Some(CapturedObject(fields)) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_shipping_lines_total
    Some(CapturedArray(items)) -> sum_shipping_lines(items)
    _ -> 0.0
  }
}

fn captured_shipping_lines_total(value: CapturedJsonValue) -> Float {
  case value {
    CapturedArray(items) -> sum_shipping_lines(items)
    _ -> 0.0
  }
}

fn sum_shipping_lines(items: List(CapturedJsonValue)) -> Float {
  items
  |> list.fold(0.0, fn(sum, line) {
    sum +. captured_money_amount(line, "originalPriceSet")
  })
}

fn sum_order_refunded_amount(order: OrderRecord) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_amount(refund, "totalRefundedSet")
  })
}

fn sum_order_refunded_shipping_amount(order: OrderRecord) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_amount(refund, "totalRefundedShippingSet")
  })
}

fn order_refunds(order_data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "refunds") {
    Some(CapturedArray(refunds)) -> refunds
    _ -> []
  }
}

fn order_total_price(order: OrderRecord) -> Float {
  captured_object_field(order.data, "totalPriceSet")
  |> option.or(captured_object_field(order.data, "currentTotalPriceSet"))
  |> option.map(captured_money_value)
  |> option.unwrap(0.0)
}

fn float_to_fixed_2(value: Float) -> String {
  let negative = value <. 0.0
  let abs_value = case negative {
    True -> 0.0 -. value
    False -> value
  }
  let cents = float.round(abs_value *. 100.0)
  let dollars = cents / 100
  let remainder = cents - dollars * 100
  let cents_str = case remainder < 10 {
    True -> "0" <> int.to_string(remainder)
    False -> int.to_string(remainder)
  }
  let sign = case negative {
    True -> "-"
    False -> ""
  }
  sign <> int.to_string(dollars) <> "." <> cents_str
}

fn format_decimal_amount(value: Float) -> String {
  let fixed = float_to_fixed_2(value)
  case string.ends_with(fixed, "00") {
    True -> string.drop_end(fixed, 3) <> ".0"
    False ->
      case string.ends_with(fixed, "0") {
        True -> string.drop_end(fixed, 1)
        False -> fixed
      }
  }
}

fn build_updated_order(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(updated_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(metafield_replacements, next_identity) = case
    dict.has_key(input, "metafields")
  {
    True -> {
      let #(metafields, identity_after_metafields) =
        build_order_metafields(
          order,
          read_object_list(input, "metafields"),
          identity_after_time,
        )
      #(
        [
          #("metafield", first_order_metafield(metafields)),
          #("metafields", order_metafields_connection(metafields)),
        ],
        identity_after_metafields,
      )
    }
    False -> #([], identity_after_time)
  }
  let replacements =
    []
    |> prepend_captured_replacement("updatedAt", CapturedString(updated_at))
    |> replace_if_present(
      input,
      "email",
      optional_captured_string(read_string(input, "email")),
    )
    |> replace_if_present(
      input,
      "phone",
      optional_captured_string(read_string(input, "phone")),
    )
    |> replace_if_present(
      input,
      "poNumber",
      optional_captured_string(read_string(input, "poNumber")),
    )
    |> replace_if_present(
      input,
      "note",
      optional_captured_string(read_string(input, "note")),
    )
    |> replace_if_present(
      input,
      "tags",
      CapturedArray(
        read_string_list(input, "tags")
        |> list.sort(by: string.compare)
        |> list.map(CapturedString),
      ),
    )
    |> replace_if_present(
      input,
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    )
    |> replace_if_present(
      input,
      "shippingAddress",
      build_order_update_address(read_object(input, "shippingAddress")),
    )
  let updated_data =
    order.data
    |> replace_captured_object_fields(list.append(
      replacements,
      metafield_replacements,
    ))
  #(OrderRecord(..order, data: updated_data), next_identity)
}

fn build_order_metafields(
  order: OrderRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let existing = order_metafield_nodes(order.data)
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    existing,
    identity,
  )
  inputs
  |> list.fold(initial, fn(acc, input) {
    let #(metafields, current_identity) = acc
    let namespace = read_string(input, "namespace") |> option.unwrap("")
    let key = read_string(input, "key") |> option.unwrap("")
    let existing_metafield =
      find_order_metafield(metafields, namespace, key)
      |> option.or(find_order_metafield(existing, namespace, key))
    let #(id, next_identity) = case
      read_string(input, "id")
      |> option.or(
        option.then(existing_metafield, fn(metafield) {
          captured_string_field(metafield, "id")
        }),
      )
    {
      Some(id) -> #(id, current_identity)
      None ->
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
    }
    let metafield =
      CapturedObject([
        #("id", CapturedString(id)),
        #("namespace", CapturedString(namespace)),
        #("key", CapturedString(key)),
        #(
          "type",
          optional_captured_string(
            read_string(input, "type")
            |> option.or(
              option.then(existing_metafield, fn(metafield) {
                captured_string_field(metafield, "type")
              }),
            ),
          ),
        ),
        #(
          "value",
          optional_captured_string(
            read_string(input, "value")
            |> option.or(
              option.then(existing_metafield, fn(metafield) {
                captured_string_field(metafield, "value")
              }),
            ),
          ),
        ),
      ])
    #(upsert_order_metafield(metafields, metafield), next_identity)
  })
}

fn order_metafield_nodes(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "metafields") {
    Some(CapturedObject(fields)) ->
      case list.find(fields, fn(pair) { pair.0 == "nodes" }) {
        Ok(#(_, CapturedArray(nodes))) -> nodes
        _ -> []
      }
    _ -> []
  }
}

fn find_order_metafield(
  metafields: List(CapturedJsonValue),
  namespace: String,
  key: String,
) -> Option(CapturedJsonValue) {
  metafields
  |> list.find(fn(metafield) {
    captured_string_field(metafield, "namespace") == Some(namespace)
    && captured_string_field(metafield, "key") == Some(key)
  })
  |> option.from_result
}

fn upsert_order_metafield(
  metafields: List(CapturedJsonValue),
  metafield: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  let namespace = captured_string_field(metafield, "namespace")
  let key = captured_string_field(metafield, "key")
  case metafields {
    [] -> [metafield]
    [first, ..rest] ->
      case
        captured_string_field(first, "namespace") == namespace
        && captured_string_field(first, "key") == key
      {
        True -> [metafield, ..rest]
        False -> [first, ..upsert_order_metafield(rest, metafield)]
      }
  }
}

fn first_order_metafield(
  metafields: List(CapturedJsonValue),
) -> CapturedJsonValue {
  case metafields {
    [first, ..] -> first
    [] -> CapturedNull
  }
}

fn order_metafields_connection(
  metafields: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([#("nodes", CapturedArray(metafields))])
}

fn validate_order_update_inline_input(
  operation_path: String,
  fields: List(ObjectField),
) -> List(Json) {
  case find_object_field(fields, "id") {
    None -> [build_order_update_missing_inline_id_error(operation_path)]
    Some(ObjectField(value: NullValue(..), ..)) -> [
      build_order_update_null_inline_id_error(operation_path),
    ]
    _ -> []
  }
}

fn validate_order_update_variable_input(
  variable_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  case dict.get(variables, variable_name) {
    Ok(root_field.ObjectVal(input)) ->
      case dict.get(input, "id") {
        Ok(root_field.NullVal) | Error(_) -> [
          build_order_update_missing_variable_id_error(
            variable_name,
            root_field.ObjectVal(input),
          ),
        ]
        _ -> []
      }
    _ -> []
  }
}

fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

fn build_order_update_missing_inline_id_error(operation_path: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("missingRequiredInputObjectAttribute")),
        #("argumentName", json.string("id")),
        #("argumentType", json.string("ID!")),
        #("inputObjectType", json.string("OrderInput")),
      ]),
    ),
  ])
}

fn build_order_update_null_inline_id_error(operation_path: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("argumentLiteralsIncompatible")),
        #("typeName", json.string("InputObject")),
        #("argumentName", json.string("id")),
      ]),
    ),
  ])
}

fn build_order_update_missing_variable_id_error(
  variable_name: String,
  value: root_field.ResolvedValue,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type OrderInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", source_to_json(resolved_value_to_source(value))),
        #(
          "problems",
          json.array(
            [
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ],
            fn(problem) { problem },
          ),
        ),
      ]),
    ),
  ])
}

fn handle_order_create_validation_guardrail(
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderCreate",
      [
        RequiredArgument(name: "order", expected_type: "OrderCreateOrderInput!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "order") {
        Ok(root_field.ObjectVal(input)) -> {
          let user_errors = validate_order_create_input(input)
          case user_errors {
            [] -> #(key, json.null(), [])
            _ -> #(
              key,
              serialize_order_mutation_error_payload(field, user_errors),
              [],
            )
          }
        }
        _ -> #(key, json.null(), [])
      }
    }
  }
}

fn handle_order_create_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let #(validation_key, validation_payload, validation_errors) =
    handle_order_create_validation_guardrail(
      document,
      operation_path,
      field,
      variables,
    )
  case validation_errors {
    [_, ..] -> #(
      validation_key,
      validation_payload,
      store,
      identity,
      [],
      validation_errors,
      [],
    )
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "order") {
        Ok(root_field.ObjectVal(input)) -> {
          let user_errors = validate_order_create_input(input)
          case user_errors {
            [_, ..] -> #(
              key,
              serialize_order_mutation_error_payload(field, user_errors),
              store,
              identity,
              [],
              [],
              [],
            )
            [] -> {
              let #(order, next_identity) =
                build_order_from_create_input(store, identity, input)
              let next_store = store.stage_order(store, order)
              let payload =
                serialize_order_mutation_payload(
                  field,
                  Some(order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "orderCreate",
                  [order.id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some("Locally staged orderCreate in shopify-draft-proxy."),
                )
              #(key, payload, next_store, next_identity, [order.id], [], [draft])
            }
          }
        }
        _ -> #(key, validation_payload, store, identity, [], [], [])
      }
    }
  }
}

fn validate_order_create_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(List(String), String)) {
  case read_object_list(input, "lineItems") {
    [] -> [
      #(["order", "lineItems"], "Line items must have at least one line item"),
    ]
    _ -> []
  }
}

fn serialize_order_mutation_error_payload(
  field: Selection,
  user_errors: List(#(List(String), String)),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, json.null())
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn build_order_from_create_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let currency_code = order_create_currency(input)
  let #(order_id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "Order")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_order)
  let #(line_items, identity_after_lines) =
    build_order_create_line_items(identity_after_time, input, currency_code)
  let #(transactions, next_identity) =
    build_order_create_transactions(identity_after_lines, input, currency_code)
  let shipping_lines = build_order_create_shipping_lines(input, currency_code)
  let subtotal = order_line_items_subtotal(line_items)
  let shipping_total = order_shipping_lines_total(shipping_lines)
  let tax_total =
    order_create_tax_total(input, line_items, shipping_lines, currency_code)
  let discount =
    order_create_discount(input, currency_code, subtotal, shipping_total)
  let discount_total = captured_money_value(discount.total_discounts_set)
  let total = subtotal +. shipping_total
  let current_total = max_float(0.0, total +. tax_total -. discount_total)
  let has_paid_transaction = order_transactions_include_paid(transactions)
  let has_authorization = order_transactions_include_authorization(transactions)
  let financial_status = case read_string(input, "financialStatus") {
    Some(value) -> string.uppercase(value)
    None ->
      case has_paid_transaction {
        True -> "PAID"
        False ->
          case has_authorization {
            True -> "AUTHORIZED"
            False -> "PENDING"
          }
      }
  }
  let fulfillment_status =
    read_string(input, "fulfillmentStatus")
    |> option.map(string.uppercase)
    |> option.unwrap("UNFULFILLED")
  let payment_gateways = order_transaction_gateways(transactions)
  let payment_gateway_names = case payment_gateways {
    [] ->
      case has_paid_transaction {
        True -> [CapturedString("manual")]
        False -> []
      }
    _ -> list.map(payment_gateways, CapturedString)
  }
  let current_total_set = money_set(current_total, currency_code)
  let zero_money = money_set(0.0, currency_code)
  let total_capturable = case has_authorization {
    True -> current_total
    False -> 0.0
  }
  let total_received = case has_paid_transaction {
    True -> current_total
    False -> 0.0
  }
  let data =
    CapturedObject([
      #("id", CapturedString(order_id)),
      #(
        "name",
        CapturedString(
          "#"
          <> int.to_string(list.length(store.list_effective_orders(store)) + 1),
        ),
      ),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("email", optional_captured_string(read_string(input, "email"))),
      #("phone", optional_captured_string(read_string(input, "phone"))),
      #("poNumber", optional_captured_string(read_string(input, "poNumber"))),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("cancelReason", CapturedNull),
      #(
        "sourceName",
        optional_captured_string(read_string(input, "sourceName")),
      ),
      #("paymentGatewayNames", CapturedArray(payment_gateway_names)),
      #("displayFinancialStatus", CapturedString(financial_status)),
      #("displayFulfillmentStatus", CapturedString(fulfillment_status)),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("tags", CapturedArray(order_create_tags(input))),
      #(
        "customAttributes",
        captured_attributes(read_object_list(input, "customAttributes")),
      ),
      #("metafields", CapturedArray([])),
      #(
        "billingAddress",
        build_draft_order_address(read_object(input, "billingAddress")),
      ),
      #(
        "shippingAddress",
        build_draft_order_address(read_object(input, "shippingAddress")),
      ),
      #("subtotalPriceSet", money_set(subtotal, currency_code)),
      #("currentSubtotalPriceSet", money_set(subtotal, currency_code)),
      #("currentTotalPriceSet", current_total_set),
      #("currentTotalDiscountsSet", discount.total_discounts_set),
      #("currentTotalTaxSet", money_set(tax_total, currency_code)),
      #("totalPriceSet", current_total_set),
      #(
        "totalOutstandingSet",
        money_set(
          case has_paid_transaction {
            True -> 0.0
            False -> current_total
          },
          currency_code,
        ),
      ),
      #("totalCapturable", CapturedString(float.to_string(total_capturable))),
      #("totalCapturableSet", money_set(total_capturable, currency_code)),
      #("capturable", CapturedBool(has_authorization)),
      #("totalRefundedSet", zero_money),
      #("totalRefundedShippingSet", zero_money),
      #("totalReceivedSet", money_set(total_received, currency_code)),
      #("netPaymentSet", money_set(total_received, currency_code)),
      #("totalShippingPriceSet", money_set(shipping_total, currency_code)),
      #("totalTaxSet", money_set(tax_total, currency_code)),
      #("totalDiscountsSet", discount.total_discounts_set),
      #(
        "discountCodes",
        CapturedArray(list.map(discount.codes, CapturedString)),
      ),
      #("discountApplications", CapturedArray(discount.applications)),
      #(
        "taxLines",
        CapturedArray(build_order_create_tax_lines(input, currency_code)),
      ),
      #("taxesIncluded", CapturedBool(read_bool(input, "taxesIncluded", False))),
      #("customer", CapturedNull),
      #(
        "shippingLines",
        CapturedObject([#("nodes", CapturedArray(shipping_lines))]),
      ),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #("paymentTerms", CapturedNull),
      #("fulfillments", CapturedArray([])),
      #("fulfillmentOrders", CapturedArray([])),
      #("transactions", CapturedArray(transactions)),
      #("refunds", CapturedArray([])),
      #("returns", CapturedArray([])),
    ])
  #(OrderRecord(id: order_id, cursor: None, data: data), next_identity)
}

type OrderCreateDiscount {
  OrderCreateDiscount(
    codes: List(String),
    applications: List(CapturedJsonValue),
    total_discounts_set: CapturedJsonValue,
  )
}

fn order_create_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_string(input, "currency") {
    Some(currency) -> currency
    None ->
      read_object_list(input, "lineItems")
      |> list.find_map(fn(line_item) {
        case read_object(line_item, "priceSet") {
          Some(price_set) ->
            case read_object(price_set, "shopMoney") {
              Some(shop_money) ->
                read_string(shop_money, "currencyCode") |> option_to_result
              None -> Error(Nil)
            }
          None -> Error(Nil)
        }
      })
      |> result.unwrap("CAD")
  }
}

fn build_order_create_line_items(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  read_object_list(input, "lineItems")
  |> list.fold(initial, fn(acc, line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "LineItem")
    let price_set =
      read_object(line_item, "originalUnitPriceSet")
      |> option.or(read_object(line_item, "priceSet"))
    let variant_id = read_string(line_item, "variantId")
    let current_quantity = case dict.get(line_item, "currentQuantity") {
      Ok(root_field.IntVal(value)) -> [#("currentQuantity", CapturedInt(value))]
      _ -> []
    }
    let item =
      CapturedObject(list.append(
        [
          #("id", CapturedString(id)),
          #("title", optional_captured_string(read_string(line_item, "title"))),
          #("quantity", CapturedInt(read_int(line_item, "quantity", 0))),
          #("sku", optional_captured_string(read_string(line_item, "sku"))),
          #("variantId", optional_captured_string(variant_id)),
          #("variant", case variant_id {
            Some(id) -> CapturedObject([#("id", CapturedString(id))])
            None -> CapturedNull
          }),
          #(
            "variantTitle",
            optional_captured_string(read_string(line_item, "variantTitle")),
          ),
          #(
            "originalUnitPriceSet",
            order_money_set_from_input(price_set, currency_code, 0.0),
          ),
          #(
            "taxLines",
            CapturedArray(build_order_create_tax_lines(line_item, currency_code)),
          ),
        ],
        current_quantity,
      ))
    #(list.append(items, [item]), next_identity)
  })
}

fn build_order_create_shipping_lines(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> List(CapturedJsonValue) {
  read_object_list(input, "shippingLines")
  |> list.map(fn(shipping_line) {
    CapturedObject([
      #("title", optional_captured_string(read_string(shipping_line, "title"))),
      #("code", optional_captured_string(read_string(shipping_line, "code"))),
      #(
        "source",
        optional_captured_string(read_string(shipping_line, "source")),
      ),
      #(
        "originalPriceSet",
        order_money_set_from_input(
          read_object(shipping_line, "priceSet"),
          currency_code,
          0.0,
        ),
      ),
      #(
        "taxLines",
        CapturedArray(build_order_create_tax_lines(shipping_line, currency_code)),
      ),
    ])
  })
}

fn build_order_create_transactions(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  read_object_list(input, "transactions")
  |> list.fold(initial, fn(acc, transaction) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "OrderTransaction",
      )
    let amount_set = read_object(transaction, "amountSet")
    let direct_amount = read_number(transaction, "amount") |> option.unwrap(0.0)
    let parent_id = read_string(transaction, "parentTransactionId")
    let item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("kind", optional_captured_string(read_string(transaction, "kind"))),
        #(
          "status",
          CapturedString(
            read_string(transaction, "status") |> option.unwrap("SUCCESS"),
          ),
        ),
        #(
          "gateway",
          optional_captured_string(read_string(transaction, "gateway")),
        ),
        #(
          "amountSet",
          order_money_set_from_input(amount_set, currency_code, direct_amount),
        ),
        #("parentTransactionId", optional_captured_string(parent_id)),
        #("parentTransaction", CapturedNull),
        #(
          "paymentId",
          optional_captured_string(read_string(transaction, "paymentId")),
        ),
        #(
          "paymentReferenceId",
          optional_captured_string(read_string(
            transaction,
            "paymentReferenceId",
          )),
        ),
        #(
          "processedAt",
          optional_captured_string(read_string(transaction, "processedAt")),
        ),
      ])
    #(list.append(items, [item]), next_identity)
  })
}

fn build_order_create_tax_lines(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> List(CapturedJsonValue) {
  read_object_list(input, "taxLines")
  |> list.map(fn(tax_line) {
    let channel_liable = case dict.get(tax_line, "channelLiable") {
      Ok(root_field.BoolVal(value)) -> CapturedBool(value)
      _ -> CapturedNull
    }
    CapturedObject([
      #("title", optional_captured_string(read_string(tax_line, "title"))),
      #("rate", captured_number(tax_line, "rate")),
      #("channelLiable", channel_liable),
      #(
        "priceSet",
        order_money_set_from_input(
          read_object(tax_line, "priceSet"),
          currency_code,
          0.0,
        ),
      ),
    ])
  })
}

fn order_money_set_from_input(
  input: Option(Dict(String, root_field.ResolvedValue)),
  currency_code: String,
  fallback_amount: Float,
) -> CapturedJsonValue {
  let fields = input |> option.unwrap(dict.new())
  let shop_money = read_object(fields, "shopMoney")
  let amount =
    case shop_money {
      Some(money) -> read_number(money, "amount")
      None -> read_number(fields, "amount")
    }
    |> option.unwrap(fallback_amount)
  let shop_currency =
    case shop_money {
      Some(money) -> read_string(money, "currencyCode")
      None -> read_string(fields, "currencyCode")
    }
    |> option.unwrap(currency_code)
  let presentment = case read_object(fields, "presentmentMoney") {
    Some(money) -> [
      #(
        "presentmentMoney",
        CapturedObject([
          #(
            "amount",
            CapturedString(float.to_string(
              read_number(money, "amount") |> option.unwrap(0.0),
            )),
          ),
          #(
            "currencyCode",
            CapturedString(
              read_string(money, "currencyCode") |> option.unwrap(shop_currency),
            ),
          ),
        ]),
      ),
    ]
    None -> []
  }
  CapturedObject(list.append(
    [
      #(
        "shopMoney",
        CapturedObject([
          #("amount", CapturedString(float.to_string(amount))),
          #("currencyCode", CapturedString(shop_currency)),
        ]),
      ),
    ],
    presentment,
  ))
}

fn order_create_tags(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  read_string_list(input, "tags")
  |> list.sort(string.compare)
  |> list.map(CapturedString)
}

fn order_line_items_subtotal(line_items: List(CapturedJsonValue)) -> Float {
  line_items
  |> list.fold(0.0, fn(sum, line_item) {
    sum
    +. captured_money_amount(line_item, "originalUnitPriceSet")
    *. int.to_float(
      captured_int_field(line_item, "quantity") |> option.unwrap(0),
    )
  })
}

fn order_shipping_lines_total(
  shipping_lines: List(CapturedJsonValue),
) -> Float {
  shipping_lines
  |> list.fold(0.0, fn(sum, shipping_line) {
    sum +. captured_money_amount(shipping_line, "originalPriceSet")
  })
}

fn order_create_tax_total(
  input: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
  shipping_lines: List(CapturedJsonValue),
  currency_code: String,
) -> Float {
  sum_captured_tax_lines(build_order_create_tax_lines(input, currency_code))
  +. list.fold(line_items, 0.0, fn(sum, line_item) {
    sum +. sum_captured_tax_lines(captured_tax_lines(line_item))
  })
  +. list.fold(shipping_lines, 0.0, fn(sum, shipping_line) {
    sum +. sum_captured_tax_lines(captured_tax_lines(shipping_line))
  })
}

fn captured_tax_lines(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(value, "taxLines") {
    Some(CapturedArray(tax_lines)) -> tax_lines
    _ -> []
  }
}

fn sum_captured_tax_lines(tax_lines: List(CapturedJsonValue)) -> Float {
  tax_lines
  |> list.fold(0.0, fn(sum, tax_line) {
    sum +. captured_money_amount(tax_line, "priceSet")
  })
}

fn order_create_discount(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
  subtotal: Float,
  shipping_total: Float,
) -> OrderCreateDiscount {
  case read_object(input, "discountCode") {
    Some(discount_code) ->
      case read_object(discount_code, "itemFixedDiscountCode") {
        Some(fixed) -> {
          let code = read_string(fixed, "code")
          let amount_set =
            order_money_set_from_input(
              read_object(fixed, "amountSet"),
              currency_code,
              0.0,
            )
          OrderCreateDiscount(
            codes: option_to_list_string(code),
            applications: [
              CapturedObject([
                #("code", optional_captured_string(code)),
                #(
                  "value",
                  CapturedObject([
                    #("type", CapturedString("money")),
                    #(
                      "amount",
                      optional_captured_string(captured_string_field(
                        captured_object_field(amount_set, "shopMoney")
                          |> option.unwrap(CapturedNull),
                        "amount",
                      )),
                    ),
                    #("currencyCode", CapturedString(currency_code)),
                  ]),
                ),
              ]),
            ],
            total_discounts_set: amount_set,
          )
        }
        None ->
          case read_object(discount_code, "itemPercentageDiscountCode") {
            Some(percent_discount) -> {
              let code = read_string(percent_discount, "code")
              let percentage =
                read_number(percent_discount, "percentage")
                |> option.unwrap(0.0)
              let amount = subtotal *. percentage /. 100.0
              OrderCreateDiscount(
                codes: option_to_list_string(code),
                applications: [
                  CapturedObject([
                    #("code", optional_captured_string(code)),
                    #(
                      "value",
                      CapturedObject([
                        #("type", CapturedString("percentage")),
                        #("percentage", CapturedFloat(percentage)),
                      ]),
                    ),
                  ]),
                ],
                total_discounts_set: money_set(amount, currency_code),
              )
            }
            None ->
              case read_object(discount_code, "freeShippingDiscountCode") {
                Some(free_shipping) -> {
                  let code = read_string(free_shipping, "code")
                  OrderCreateDiscount(
                    codes: option_to_list_string(code),
                    applications: [
                      CapturedObject([
                        #("code", optional_captured_string(code)),
                        #(
                          "value",
                          CapturedObject([
                            #("type", CapturedString("money")),
                            #(
                              "amount",
                              CapturedString(float.to_string(shipping_total)),
                            ),
                            #("currencyCode", CapturedString(currency_code)),
                          ]),
                        ),
                      ]),
                    ],
                    total_discounts_set: money_set(
                      shipping_total,
                      currency_code,
                    ),
                  )
                }
                None -> empty_order_create_discount(currency_code)
              }
          }
      }
    None -> empty_order_create_discount(currency_code)
  }
}

fn empty_order_create_discount(currency_code: String) -> OrderCreateDiscount {
  OrderCreateDiscount(
    codes: [],
    applications: [],
    total_discounts_set: money_set(0.0, currency_code),
  )
}

fn option_to_list_string(value: Option(String)) -> List(String) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

fn order_transactions_include_paid(
  transactions: List(CapturedJsonValue),
) -> Bool {
  transactions
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "status") == Some("SUCCESS")
    && case captured_string_field(transaction, "kind") {
      Some("SALE") | Some("CAPTURE") -> True
      _ -> False
    }
  })
}

fn order_transactions_include_authorization(
  transactions: List(CapturedJsonValue),
) -> Bool {
  transactions
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "kind") == Some("AUTHORIZATION")
  })
}

fn order_transaction_gateways(
  transactions: List(CapturedJsonValue),
) -> List(String) {
  transactions
  |> list.filter_map(fn(transaction) {
    captured_string_field(transaction, "gateway") |> option_to_result
  })
}

fn handle_fulfillment_mutation(
  root_name: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let required = case root_name {
    "fulfillmentTrackingInfoUpdate" -> [
      RequiredArgument(name: "fulfillmentId", expected_type: "ID!"),
    ]
    _ -> [RequiredArgument(name: "id", expected_type: "ID!")]
  }
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      root_name,
      required,
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let fulfillment_id = case root_name {
        "fulfillmentTrackingInfoUpdate" ->
          read_string_arg(args, "fulfillmentId")
        _ -> read_string_arg(args, "id")
      }
      case fulfillment_id {
        None -> #(key, json.null(), store, identity, [], [], [])
        Some(id) ->
          case find_order_with_fulfillment(store, id) {
            None -> {
              let payload =
                serialize_fulfillment_mutation_payload(
                  field,
                  None,
                  [
                    #(
                      case root_name {
                        "fulfillmentTrackingInfoUpdate" -> ["fulfillmentId"]
                        _ -> ["id"]
                      },
                      case root_name {
                        "fulfillmentTrackingInfoUpdate" ->
                          "Fulfillment does not exist."
                        _ -> "Fulfillment not found."
                      },
                    ),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
            Some(match) -> {
              let #(order, fulfillment) = match
              let #(updated_fulfillment, next_identity) =
                update_fulfillment_for_root(
                  root_name,
                  fulfillment,
                  args,
                  identity,
                )
              let updated_order =
                update_order_fulfillment(order, id, updated_fulfillment)
              let next_store = store.stage_order(store, updated_order)
              let payload =
                serialize_fulfillment_mutation_payload(
                  field,
                  Some(updated_fulfillment),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  root_name,
                  [id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged " <> root_name <> " in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [order.id], [], [draft])
            }
          }
      }
    }
  }
}

fn find_order_with_fulfillment(
  store: Store,
  fulfillment_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case find_fulfillment(order_fulfillments(order.data), fulfillment_id) {
      Some(fulfillment) -> Ok(#(order, fulfillment))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn find_fulfillment(
  fulfillments: List(CapturedJsonValue),
  fulfillment_id: String,
) -> Option(CapturedJsonValue) {
  fulfillments
  |> list.find(fn(fulfillment) {
    captured_string_field(fulfillment, "id") == Some(fulfillment_id)
  })
  |> option.from_result
}

fn order_fulfillments(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "fulfillments") {
    Some(CapturedArray(fulfillments)) -> fulfillments
    _ -> []
  }
}

fn update_fulfillment_for_root(
  root_name: String,
  fulfillment: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let replacements = case root_name {
    "fulfillmentTrackingInfoUpdate" -> [
      #("updatedAt", CapturedString(updated_at)),
      #("trackingInfo", tracking_info_from_args(args)),
    ]
    _ -> [
      #("updatedAt", CapturedString(updated_at)),
      #("status", CapturedString("CANCELLED")),
      #("displayStatus", CapturedString("CANCELED")),
    ]
  }
  #(replace_captured_object_fields(fulfillment, replacements), next_identity)
}

fn tracking_info_from_args(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case dict.get(args, "trackingInfoInput") {
    Ok(root_field.ObjectVal(input)) ->
      CapturedArray([
        CapturedObject([
          #("number", optional_captured_string(read_string(input, "number"))),
          #("url", optional_captured_string(read_string(input, "url"))),
          #("company", optional_captured_string(read_string(input, "company"))),
        ]),
      ])
    _ -> CapturedArray([])
  }
}

fn update_order_fulfillment(
  order: OrderRecord,
  fulfillment_id: String,
  updated_fulfillment: CapturedJsonValue,
) -> OrderRecord {
  let updated_fulfillments =
    order_fulfillments(order.data)
    |> list.map(fn(fulfillment) {
      case captured_string_field(fulfillment, "id") == Some(fulfillment_id) {
        True -> updated_fulfillment
        False -> fulfillment
      }
    })
  let display_status = case
    captured_string_field(updated_fulfillment, "status")
  {
    Some("CANCELLED") -> [
      #("displayFulfillmentStatus", CapturedString("UNFULFILLED")),
    ]
    _ -> []
  }
  let updated_data =
    order.data
    |> replace_captured_object_fields(list.append(
      [#("fulfillments", CapturedArray(updated_fulfillments))],
      display_status,
    ))
  OrderRecord(..order, data: updated_data)
}

fn serialize_fulfillment_mutation_payload(
  field: Selection,
  fulfillment: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillment" -> #(key, case fulfillment {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn handle_draft_order_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderCreate",
      [RequiredArgument(name: "input", expected_type: "DraftOrderInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> {
          let user_errors = validate_draft_order_create_input(store, input)
          case user_errors {
            [] -> {
              let #(draft_order, next_identity) =
                build_draft_order_from_input(store, identity, input)
              let next_store = store.stage_draft_order(store, draft_order)
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  Some(draft_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreate",
                  [draft_order.id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderCreate in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [draft_order.id], [], [
                draft,
              ])
            }
            _ -> {
              let payload =
                serialize_draft_order_nullable_error_payload(
                  field,
                  None,
                  user_errors,
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreate",
                  [],
                  store.Failed,
                  "orders",
                  "stage-locally",
                  Some("Locally rejected draftOrderCreate validation branch."),
                )
              #(key, payload, store, identity, [], [], [draft])
            }
          }
        }
        _ -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn handle_draft_order_create_from_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderCreateFromOrder",
      [RequiredArgument(name: "orderId", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_string_arg(args, "orderId") {
        Some(order_id) ->
          case find_order_source_by_id(store, order_id) {
            Some(source) -> {
              let #(order, source_draft_order) = source
              let #(draft_order, next_identity) =
                build_draft_order_from_order(
                  store,
                  identity,
                  order,
                  source_draft_order,
                )
              let next_store = store.stage_draft_order(store, draft_order)
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  Some(draft_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreateFromOrder",
                  [draft_order.id],
                  store.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderCreateFromOrder in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [draft_order.id], [], [
                draft,
              ])
            }
            None -> {
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  None,
                  [#(["orderId"], "Order does not exist")],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
          }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn find_order_source_by_id(
  store: Store,
  order_id: String,
) -> Option(#(CapturedJsonValue, DraftOrderRecord)) {
  store.list_effective_draft_orders(store)
  |> list.find_map(fn(draft_order) {
    case captured_object_field(draft_order.data, "order") {
      Some(order) ->
        case captured_string_field(order, "id") {
          Some(id) if id == order_id -> Ok(#(order, draft_order))
          _ -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn build_draft_order_from_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order: CapturedJsonValue,
  source_draft_order: DraftOrderRecord,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let currency_code = captured_source_order_currency(order)
  let #(line_items, next_identity) =
    build_draft_order_line_items_from_order(
      identity_after_time,
      draft_order_line_items(order),
      currency_code,
    )
  let subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum
      +. captured_money_amount(item, "originalUnitPriceSet")
      *. int.to_float(captured_int_field(item, "quantity") |> option.unwrap(0))
    })
  let data =
    CapturedObject([
      #("id", CapturedString(draft_order_id)),
      #(
        "name",
        CapturedString(
          "#D"
          <> int.to_string(
            list.length(store.list_effective_draft_orders(store)) + 1,
          ),
        ),
      ),
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("email", source_order_email(order, source_draft_order.data)),
      #("note", captured_field_or_null(order, "note")),
      #("tags", captured_field_or_empty_array(order, "tags")),
      #("customer", source_order_customer(order, source_draft_order.data)),
      #("taxExempt", CapturedBool(False)),
      #("taxesIncluded", CapturedBool(False)),
      #("reserveInventoryUntil", CapturedNull),
      #("paymentTerms", CapturedNull),
      #("appliedDiscount", CapturedNull),
      #(
        "customAttributes",
        captured_field_or_empty_array(order, "customAttributes"),
      ),
      #("billingAddress", captured_field_or_null(order, "billingAddress")),
      #("shippingAddress", captured_field_or_null(order, "shippingAddress")),
      #("shippingLine", CapturedNull),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("subtotalPriceSet", money_set(subtotal, currency_code)),
      #("totalDiscountsSet", money_set(0.0, currency_code)),
      #("totalShippingPriceSet", money_set(0.0, currency_code)),
      #("totalPriceSet", money_set(subtotal, currency_code)),
      #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    next_identity,
  )
}

fn build_draft_order_line_items_from_order(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    #(
      list.append(items, [
        build_draft_order_line_item_from_order(id, item, currency_code),
      ]),
      next_identity,
    )
  })
}

fn build_draft_order_line_item_from_order(
  id: String,
  item: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
  let original_unit_price =
    captured_field_or_money(item, "originalUnitPriceSet", currency_code)
  let original_total =
    captured_money_value(original_unit_price) *. int.to_float(quantity)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", captured_field_or_null(item, "title")),
    #("name", captured_field_or_null(item, "title")),
    #("quantity", CapturedInt(quantity)),
    #("sku", nullable_empty_captured_string(item, "sku")),
    #("variantTitle", nullable_default_title(item)),
    #(
      "variantId",
      optional_captured_string(source_order_line_item_variant_id(item)),
    ),
    #("productId", CapturedNull),
    #("custom", CapturedBool(source_order_line_item_custom(item))),
    #("requiresShipping", CapturedBool(True)),
    #("taxable", CapturedBool(True)),
    #("customAttributes", CapturedArray([])),
    #("appliedDiscount", CapturedNull),
    #("originalUnitPriceSet", original_unit_price),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(original_total, currency_code)),
    #("totalDiscountSet", money_set(0.0, currency_code)),
    #("variant", source_order_line_item_variant(item)),
  ])
}

fn captured_source_order_currency(order: CapturedJsonValue) -> String {
  captured_money_currency(order, "currentTotalPriceSet")
  |> option.or(captured_money_currency(order, "totalPriceSet"))
  |> option.or(captured_money_currency(order, "subtotalPriceSet"))
  |> option.or(first_order_line_item_currency(order))
  |> option.unwrap("CAD")
}

fn first_order_line_item_currency(order: CapturedJsonValue) -> Option(String) {
  order
  |> draft_order_line_items
  |> list.find_map(fn(item) {
    case captured_money_currency(item, "originalUnitPriceSet") {
      Some(currency) -> Ok(currency)
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn source_order_email(
  order: CapturedJsonValue,
  source_draft_order: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_string_field(order, "email") {
    Some(email) -> CapturedString(email)
    None ->
      case captured_object_field(order, "customer") {
        Some(customer) ->
          case captured_string_field(customer, "email") {
            Some(email) -> CapturedString(email)
            None -> captured_field_or_null(source_draft_order, "email")
          }
        None -> captured_field_or_null(source_draft_order, "email")
      }
  }
}

fn source_order_customer(
  order: CapturedJsonValue,
  source_draft_order: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(order, "customer") {
    Some(customer) -> customer
    None -> captured_field_or_null(source_draft_order, "customer")
  }
}

fn source_order_line_item_variant_id(
  item: CapturedJsonValue,
) -> Option(String) {
  case captured_object_field(item, "variant") {
    Some(variant) -> captured_string_field(variant, "id")
    None -> captured_string_field(item, "variantId")
  }
}

fn source_order_line_item_custom(item: CapturedJsonValue) -> Bool {
  case source_order_line_item_variant_id(item) {
    Some(_) -> False
    None -> True
  }
}

fn source_order_line_item_variant(
  item: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(item, "variant") {
    Some(variant) -> variant
    None ->
      case captured_string_field(item, "variantId") {
        Some(id) ->
          CapturedObject([
            #("id", CapturedString(id)),
            #("title", captured_field_or_null(item, "variantTitle")),
            #("sku", nullable_empty_captured_string(item, "sku")),
          ])
        None -> CapturedNull
      }
  }
}

fn validate_draft_order_create_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String)) {
  let line_items = read_object_list(input, "lineItems")
  case line_items {
    [] -> [#(None, "Add at least 1 product")]
    _ -> {
      let line_item_errors =
        line_items
        |> list.index_map(fn(line_item, index) {
          validate_draft_order_create_line_item(store, line_item, index)
        })
        |> list.flatten
      list.flatten([
        validate_draft_order_create_email(input),
        validate_draft_order_create_reserve(input),
        validate_draft_order_create_payment_terms(input),
        line_item_errors,
      ])
    }
  }
}

fn validate_draft_order_create_email(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String)) {
  case read_string(input, "email") {
    Some(email) ->
      case valid_email_address(email) {
        True -> []
        False -> [#(Some(["email"]), "Email is invalid")]
      }
    _ -> []
  }
}

fn valid_email_address(email: String) -> Bool {
  case string.contains(email, " ") {
    True -> False
    False ->
      case string.split(email, "@") {
        [local, domain] ->
          string.trim(local) != "" && string.contains(domain, ".")
        _ -> False
      }
  }
}

fn validate_draft_order_create_reserve(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String)) {
  case read_string(input, "reserveInventoryUntil") {
    Some(value) ->
      case
        iso_timestamp.parse_iso(value),
        iso_timestamp.parse_iso(iso_timestamp.now_iso())
      {
        Ok(reserve_until), Ok(now) ->
          case reserve_until < now {
            True -> [#(None, "Reserve until can't be in the past")]
            False -> []
          }
        _, _ -> []
      }
    _ -> []
  }
}

fn validate_draft_order_create_payment_terms(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String)) {
  case read_object(input, "paymentTerms") {
    Some(payment_terms) ->
      case read_string(payment_terms, "paymentTermsTemplateId") {
        Some(_) -> [#(None, "The user must have access to set payment terms.")]
        None -> [#(None, "Payment terms template id can not be empty.")]
      }
    None -> []
  }
}

fn validate_draft_order_create_line_item(
  store: Store,
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String)) {
  case read_string(line_item, "variantId") {
    Some(variant_id) ->
      case store.get_draft_order_variant_catalog_by_id(store, variant_id) {
        Some(_) -> []
        None ->
          case store.get_effective_variant_by_id(store, variant_id) {
            Some(_) -> []
            None -> [
              #(
                None,
                "Product with ID "
                  <> draft_order_gid_tail(variant_id)
                  <> " is no longer available.",
              ),
            ]
          }
      }
    None -> validate_custom_draft_order_line_item(line_item, index)
  }
}

fn validate_custom_draft_order_line_item(
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String)) {
  case read_string(line_item, "title") {
    Some(title) ->
      case string.trim(title) != "" {
        True -> validate_custom_draft_order_line_item_values(line_item, index)
        False -> [#(None, "Merchandise title is empty.")]
      }
    _ -> [#(None, "Merchandise title is empty.")]
  }
}

fn validate_custom_draft_order_line_item_values(
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String)) {
  let quantity = read_int(line_item, "quantity", 1)
  case quantity < 1 {
    True -> [
      #(
        Some(["lineItems", int.to_string(index), "quantity"]),
        "Quantity must be greater than or equal to 1",
      ),
    ]
    False -> {
      let amount =
        read_string(line_item, "originalUnitPrice")
        |> option.unwrap("0")
        |> parse_amount
      case amount <. 0.0 {
        True -> [#(None, "Cannot send negative price for line_item")]
        False -> []
      }
    }
  }
}

fn serialize_draft_order_nullable_error_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(Option(List(String)), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_draft_order_mutation_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn build_updated_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(updated_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let currency_code = captured_order_currency(draft_order.data)
  let #(line_items, next_identity) = case dict.has_key(input, "lineItems") {
    True ->
      build_draft_order_line_items(
        store,
        identity_after_time,
        read_object_list(input, "lineItems"),
      )
    False -> #(draft_order_line_items(draft_order.data), identity_after_time)
  }
  let replacements =
    []
    |> replace_if_present(
      input,
      "email",
      optional_captured_string(read_string(input, "email")),
    )
    |> replace_if_present(
      input,
      "note",
      optional_captured_string(read_string(input, "note")),
    )
    |> replace_if_present(
      input,
      "tags",
      CapturedArray(
        read_string_list(input, "tags")
        |> list.sort(by: string.compare)
        |> list.map(CapturedString),
      ),
    )
    |> replace_if_present(
      input,
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    )
    |> replace_if_present(
      input,
      "billingAddress",
      build_draft_order_address(read_object(input, "billingAddress")),
    )
    |> replace_if_present(
      input,
      "shippingAddress",
      build_draft_order_address(read_object(input, "shippingAddress")),
    )
    |> replace_if_present(
      input,
      "shippingLine",
      build_draft_order_shipping_line(read_object(input, "shippingLine")),
    )
    |> prepend_captured_replacement("updatedAt", CapturedString(updated_at))
    |> prepend_captured_replacement(
      "lineItems",
      CapturedObject([#("nodes", CapturedArray(line_items))]),
    )
  let updated_data =
    draft_order.data
    |> replace_captured_object_fields(replacements)
    |> recalculate_draft_order_totals(currency_code)
  #(DraftOrderRecord(..draft_order, data: updated_data), next_identity)
}

fn duplicate_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let currency_code = captured_order_currency(draft_order.data)
  let #(line_items, next_identity) =
    duplicate_draft_order_line_items(
      identity_after_time,
      draft_order_line_items(draft_order.data),
      currency_code,
    )
  let data =
    draft_order.data
    |> replace_captured_object_fields([
      #("id", CapturedString(draft_order_id)),
      #(
        "name",
        CapturedString(
          "#D"
          <> int.to_string(
            list.length(store.list_effective_draft_orders(store)) + 1,
          ),
        ),
      ),
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #("orderId", CapturedNull),
      #("completedAt", CapturedNull),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("taxExempt", CapturedBool(False)),
      #("reserveInventoryUntil", CapturedNull),
      #("paymentTerms", CapturedNull),
      #("appliedDiscount", CapturedNull),
      #("shippingLine", CapturedNull),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
    |> recalculate_draft_order_totals(currency_code)
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    next_identity,
  )
}

fn duplicate_draft_order_line_items(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    #(
      list.append(items, [
        duplicate_draft_order_line_item(id, item, currency_code),
      ]),
      next_identity,
    )
  })
}

fn duplicate_draft_order_line_item(
  id: String,
  item: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
  let original_total = case captured_object_field(item, "originalTotalSet") {
    Some(total) -> total
    None ->
      money_set(
        captured_money_amount(item, "originalUnitPriceSet")
          *. int.to_float(quantity),
        currency_code,
      )
  }
  item
  |> replace_captured_object_fields([
    #("id", CapturedString(id)),
    #("appliedDiscount", CapturedNull),
    #("discountedTotalSet", original_total),
    #("totalDiscountSet", money_set(0.0, currency_code)),
  ])
}

fn complete_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  source_name: Option(String),
  payment_pending: Bool,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(completed_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(order, next_identity) =
    build_order_from_completed_draft_order(
      store,
      identity_after_time,
      draft_order,
      completed_at,
      source_name,
      payment_pending,
    )
  let order_id = captured_string_field(order, "id")
  let data =
    draft_order.data
    |> replace_captured_object_fields([
      #("status", CapturedString("COMPLETED")),
      #("ready", CapturedBool(True)),
      #("completedAt", CapturedString(completed_at)),
      #("updatedAt", CapturedString(completed_at)),
      #("orderId", optional_captured_string(order_id)),
      #("order", order),
    ])
  #(DraftOrderRecord(..draft_order, data: data), next_identity)
}

fn build_order_from_completed_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  completed_at: String,
  source_name: Option(String),
  payment_pending: Bool,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(order_id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "Order")
  let #(line_items, next_identity) =
    build_order_line_items_from_draft_order(
      identity_after_order,
      draft_order_line_items(draft_order.data),
    )
  let currency_code = captured_order_currency(draft_order.data)
  let payment_gateway_names = case payment_pending {
    True -> []
    False -> [CapturedString("manual")]
  }
  let financial_status = case payment_pending {
    True -> "PENDING"
    False -> "PAID"
  }
  #(
    CapturedObject([
      #("id", CapturedString(order_id)),
      #(
        "name",
        CapturedString("#" <> int.to_string(completed_order_count(store) + 1)),
      ),
      #("createdAt", CapturedString(completed_at)),
      #("updatedAt", CapturedString(completed_at)),
      #("email", captured_field_or_null(draft_order.data, "email")),
      #("phone", CapturedNull),
      #("poNumber", CapturedNull),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("cancelReason", CapturedNull),
      #("sourceName", normalized_completed_order_source_name(source_name)),
      #("paymentGatewayNames", CapturedArray(payment_gateway_names)),
      #("displayFinancialStatus", CapturedString(financial_status)),
      #("displayFulfillmentStatus", CapturedString("UNFULFILLED")),
      #("note", captured_field_or_null(draft_order.data, "note")),
      #("tags", captured_field_or_empty_array(draft_order.data, "tags")),
      #(
        "customAttributes",
        captured_field_or_empty_array(draft_order.data, "customAttributes"),
      ),
      #("metafields", CapturedArray([])),
      #(
        "billingAddress",
        captured_field_or_null(draft_order.data, "billingAddress"),
      ),
      #(
        "shippingAddress",
        captured_field_or_null(draft_order.data, "shippingAddress"),
      ),
      #(
        "subtotalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "subtotalPriceSet",
          currency_code,
        ),
      ),
      #(
        "currentTotalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "totalPriceSet",
          currency_code,
        ),
      ),
      #(
        "totalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "totalPriceSet",
          currency_code,
        ),
      ),
      #(
        "totalOutstandingSet",
        money_set(
          case payment_pending {
            True -> captured_money_amount(draft_order.data, "totalPriceSet")
            False -> 0.0
          },
          currency_code,
        ),
      ),
      #("totalRefundedSet", money_set(0.0, currency_code)),
      #("totalTaxSet", money_set(0.0, currency_code)),
      #("totalDiscountsSet", money_set(0.0, currency_code)),
      #("discountCodes", CapturedArray([])),
      #("discountApplications", CapturedArray([])),
      #("taxLines", CapturedArray([])),
      #("taxesIncluded", CapturedBool(False)),
      #("customer", captured_field_or_null(draft_order.data, "customer")),
      #("shippingLines", completed_order_shipping_lines(draft_order.data)),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #(
        "paymentTerms",
        captured_field_or_null(draft_order.data, "paymentTerms"),
      ),
      #("transactions", CapturedArray([])),
      #("refunds", CapturedArray([])),
      #("returns", CapturedArray([])),
    ]),
    next_identity,
  )
}

fn build_order_line_items_from_draft_order(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "LineItem")
    #(
      list.append(items, [build_order_line_item_from_draft_order(id, item)]),
      next_identity,
    )
  })
}

fn build_order_line_item_from_draft_order(
  id: String,
  item: CapturedJsonValue,
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", captured_field_or_null(item, "title")),
    #("quantity", captured_field_or_int(item, "quantity", 0)),
    #("sku", nullable_empty_captured_string(item, "sku")),
    #("variantId", CapturedNull),
    #("variantTitle", nullable_default_title(item)),
    #(
      "originalUnitPriceSet",
      captured_field_or_money(
        item,
        "originalUnitPriceSet",
        captured_order_currency(item),
      ),
    ),
    #("taxLines", CapturedArray([])),
  ])
}

fn completed_order_shipping_lines(
  data: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(data, "shippingLine") {
    Some(CapturedObject(fields)) ->
      CapturedArray([
        CapturedObject(
          upsert_captured_fields(fields, [
            #("source", CapturedNull),
            #("taxLines", CapturedArray([])),
          ]),
        ),
      ])
    _ -> CapturedArray([])
  }
}

fn completed_order_count(store: Store) -> Int {
  store.list_effective_draft_orders(store)
  |> list.fold(0, fn(count, record) {
    case captured_object_field(record.data, "order") {
      Some(CapturedObject(_)) -> count + 1
      _ -> count
    }
  })
}

fn normalized_completed_order_source_name(
  source_name: Option(String),
) -> CapturedJsonValue {
  case source_name {
    Some(_) -> CapturedString("347082227713")
    None -> CapturedNull
  }
}

fn captured_field_or_null(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  captured_object_field(value, name) |> option.unwrap(CapturedNull)
}

fn captured_field_or_empty_array(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  captured_object_field(value, name) |> option.unwrap(CapturedArray([]))
}

fn captured_field_or_money(
  value: CapturedJsonValue,
  name: String,
  currency_code: String,
) -> CapturedJsonValue {
  captured_object_field(value, name)
  |> option.unwrap(money_set(0.0, currency_code))
}

fn captured_field_or_int(
  value: CapturedJsonValue,
  name: String,
  fallback: Int,
) -> CapturedJsonValue {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> CapturedInt(value)
    _ -> CapturedInt(fallback)
  }
}

fn nullable_empty_captured_string(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  case captured_string_field(value, name) {
    Some("") -> CapturedNull
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn nullable_default_title(item: CapturedJsonValue) -> CapturedJsonValue {
  case captured_string_field(item, "variantTitle") {
    Some("Default Title") -> CapturedNull
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn replace_if_present(
  replacements: List(#(String, CapturedJsonValue)),
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  case dict.has_key(input, name) {
    True -> prepend_captured_replacement(replacements, name, value)
    False -> replacements
  }
}

fn prepend_captured_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  name: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  [#(name, value), ..replacements]
}

fn replace_captured_object_fields(
  value: CapturedJsonValue,
  replacements: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  case value {
    CapturedObject(fields) ->
      CapturedObject(upsert_captured_fields(fields, replacements))
    _ -> CapturedObject(replacements)
  }
}

fn upsert_captured_fields(
  fields: List(#(String, CapturedJsonValue)),
  replacements: List(#(String, CapturedJsonValue)),
) -> List(#(String, CapturedJsonValue)) {
  let replaced =
    list.map(fields, fn(pair) {
      let #(key, existing) = pair
      case find_captured_replacement(replacements, key) {
        Some(value) -> #(key, value)
        None -> #(key, existing)
      }
    })
  let appended =
    replacements
    |> list.filter(fn(pair) {
      let #(key, _) = pair
      !list.any(fields, fn(existing_pair) {
        let #(existing_key, _) = existing_pair
        existing_key == key
      })
    })
  list.append(replaced, appended)
}

fn find_captured_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  name: String,
) -> Option(CapturedJsonValue) {
  replacements
  |> list.find_map(fn(pair) {
    let #(key, value) = pair
    case key == name {
      True -> Ok(value)
      False -> Error(Nil)
    }
  })
  |> option.from_result
}

fn recalculate_draft_order_totals(
  data: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let line_items = draft_order_line_items(data)
  let applied_discount =
    captured_object_field(data, "appliedDiscount")
    |> option.unwrap(CapturedNull)
  let shipping_line =
    captured_object_field(data, "shippingLine") |> option.unwrap(CapturedNull)
  let line_discount_total =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "totalDiscountSet")
    })
  let discounted_line_subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. draft_order_line_item_discounted_total(item)
    })
  let order_discount_total =
    discount_amount(applied_discount, discounted_line_subtotal)
  let subtotal =
    max_float(0.0, discounted_line_subtotal -. order_discount_total)
  let shipping_total = captured_money_amount(shipping_line, "originalPriceSet")
  let total_discount = line_discount_total +. order_discount_total
  let total = subtotal +. shipping_total
  data
  |> replace_captured_object_fields([
    #("subtotalPriceSet", money_set(subtotal, currency_code)),
    #("totalDiscountsSet", money_set(total_discount, currency_code)),
    #("totalShippingPriceSet", money_set(shipping_total, currency_code)),
    #("totalPriceSet", money_set(total, currency_code)),
    #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
  ])
}

fn draft_order_line_item_discounted_total(item: CapturedJsonValue) -> Float {
  case captured_object_field(item, "discountedTotalSet") {
    Some(discounted_total) -> captured_money_value(discounted_total)
    None ->
      captured_money_amount(item, "originalUnitPriceSet")
      *. int.to_float(captured_int_field(item, "quantity") |> option.unwrap(0))
  }
}

fn captured_order_currency(data: CapturedJsonValue) -> String {
  captured_money_currency(data, "totalPriceSet")
  |> option.or(captured_money_currency(data, "subtotalPriceSet"))
  |> option.or(captured_money_currency(data, "totalShippingPriceSet"))
  |> option.unwrap("CAD")
}

fn captured_money_currency(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(money_set) ->
      case captured_object_field(money_set, "shopMoney") {
        Some(shop_money) -> captured_string_field(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  }
}

fn draft_order_line_items(data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(data, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

fn build_draft_order_from_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let #(line_items, identity_after_lines) =
    build_draft_order_line_items(
      store,
      identity_after_time,
      read_object_list(input, "lineItems"),
    )
  let currency_code = draft_order_currency(input, line_items)
  let applied_discount =
    build_draft_order_applied_discount(
      read_object(input, "appliedDiscount"),
      currency_code,
    )
  let shipping_line =
    build_draft_order_shipping_line(read_object(input, "shippingLine"))
  let line_discount_total =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "totalDiscountSet")
    })
  let discounted_line_subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "discountedTotalSet")
    })
  let order_discount_total =
    discount_amount(applied_discount, discounted_line_subtotal)
  let subtotal =
    max_float(0.0, discounted_line_subtotal -. order_discount_total)
  let shipping_total = captured_money_amount(shipping_line, "originalPriceSet")
  let total_discount = line_discount_total +. order_discount_total
  let total = subtotal +. shipping_total
  let data =
    CapturedObject([
      #("id", CapturedString(draft_order_id)),
      #(
        "name",
        CapturedString(
          "#D"
          <> int.to_string(
            list.length(store.list_effective_draft_orders(store)) + 1,
          ),
        ),
      ),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("email", optional_captured_string(read_string(input, "email"))),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("customer", build_draft_order_customer(store, input)),
      #("taxExempt", CapturedBool(read_bool(input, "taxExempt", False))),
      #("taxesIncluded", CapturedBool(read_bool(input, "taxesIncluded", False))),
      #(
        "reserveInventoryUntil",
        optional_captured_string(read_string(input, "reserveInventoryUntil")),
      ),
      #("paymentTerms", CapturedNull),
      #(
        "tags",
        CapturedArray(
          read_string_list(input, "tags")
          |> list.sort(by: string.compare)
          |> list.map(CapturedString),
        ),
      ),
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #(
        "customAttributes",
        captured_attributes(read_object_list(input, "customAttributes")),
      ),
      #("appliedDiscount", applied_discount),
      #(
        "billingAddress",
        build_draft_order_address(read_object(input, "billingAddress")),
      ),
      #(
        "shippingAddress",
        build_draft_order_address(read_object(input, "shippingAddress")),
      ),
      #("shippingLine", shipping_line),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("subtotalPriceSet", money_set(subtotal, currency_code)),
      #("totalDiscountsSet", money_set(total_discount, currency_code)),
      #("totalShippingPriceSet", money_set(shipping_total, currency_code)),
      #("totalPriceSet", money_set(total, currency_code)),
      #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    identity_after_lines,
  )
}

fn build_draft_order_line_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  inputs
  |> list.fold(initial, fn(acc, input) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    let item = build_draft_order_line_item(store, id, input)
    #(list.append(items, [item]), next_identity)
  })
}

fn build_draft_order_line_item(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let quantity = read_int(input, "quantity", 1)
  case read_string(input, "variantId") {
    Some(variant_id) -> {
      let catalog =
        store.get_draft_order_variant_catalog_by_id(store, variant_id)
      build_variant_draft_order_line_item(id, variant_id, quantity, catalog)
    }
    None -> build_custom_draft_order_line_item(id, quantity, input)
  }
}

fn build_variant_draft_order_line_item(
  id: String,
  variant_id: String,
  quantity: Int,
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> CapturedJsonValue {
  let title = case catalog {
    Some(record) -> record.title
    None -> "Variant"
  }
  let name = case catalog {
    Some(record) -> record.name
    None -> title
  }
  let variant_title = case catalog {
    Some(record) -> record.variant_title
    None -> None
  }
  let sku = case catalog {
    Some(record) -> record.sku
    None -> None
  }
  let line_variant_title = case variant_title {
    Some("Default Title") -> None
    other -> other
  }
  let nested_variant_sku = case sku {
    Some("") -> None
    other -> other
  }
  let unit_price = case catalog {
    Some(record) -> parse_amount(record.unit_price)
    None -> 0.0
  }
  let currency_code = case catalog {
    Some(record) -> record.currency_code
    None -> "CAD"
  }
  let original_total = unit_price *. int.to_float(quantity)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("name", CapturedString(name)),
    #("quantity", CapturedInt(quantity)),
    #("sku", optional_captured_string(sku)),
    #("variantTitle", optional_captured_string(line_variant_title)),
    #("custom", CapturedBool(False)),
    #("requiresShipping", CapturedBool(catalog_requires_shipping(catalog))),
    #("taxable", CapturedBool(catalog_taxable(catalog))),
    #("customAttributes", CapturedArray([])),
    #("appliedDiscount", CapturedNull),
    #("originalUnitPriceSet", money_set(unit_price, currency_code)),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(original_total, currency_code)),
    #("totalDiscountSet", money_set(0.0, currency_code)),
    #(
      "variant",
      CapturedObject([
        #("id", CapturedString(variant_id)),
        #("title", optional_captured_string(variant_title)),
        #("sku", optional_captured_string(nested_variant_sku)),
      ]),
    ),
  ])
}

fn build_custom_draft_order_line_item(
  id: String,
  quantity: Int,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let currency_code = "CAD"
  let title = read_string(input, "title") |> option.unwrap("Custom item")
  let unit_price = read_string(input, "originalUnitPrice") |> option.unwrap("0")
  let unit_price = parse_amount(unit_price)
  let original_total = unit_price *. int.to_float(quantity)
  let applied_discount =
    build_draft_order_applied_discount(
      read_object(input, "appliedDiscount"),
      currency_code,
    )
  let discount_total = discount_amount(applied_discount, original_total)
  let discounted_total = max_float(0.0, original_total -. discount_total)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("name", CapturedString(title)),
    #("quantity", CapturedInt(quantity)),
    #("sku", optional_captured_string(read_string(input, "sku"))),
    #("variantTitle", CapturedNull),
    #("custom", CapturedBool(True)),
    #(
      "requiresShipping",
      CapturedBool(read_bool(input, "requiresShipping", True)),
    ),
    #("taxable", CapturedBool(read_bool(input, "taxable", True))),
    #(
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    ),
    #("appliedDiscount", applied_discount),
    #("originalUnitPriceSet", money_set(unit_price, currency_code)),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(discounted_total, currency_code)),
    #("totalDiscountSet", money_set(discount_total, currency_code)),
    #("variant", CapturedNull),
  ])
}

fn catalog_requires_shipping(
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> Bool {
  case catalog {
    Some(record) -> record.requires_shipping
    None -> True
  }
}

fn catalog_taxable(catalog: Option(DraftOrderVariantCatalogRecord)) -> Bool {
  case catalog {
    Some(record) -> record.taxable
    None -> True
  }
}

fn build_draft_order_customer(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let customer_id = case read_object(input, "purchasingEntity") {
    Some(entity) -> read_string(entity, "customerId")
    None -> None
  }
  case customer_id {
    None -> CapturedNull
    Some(id) -> {
      let customer = store.get_effective_customer_by_id(store, id)
      CapturedObject([
        #("id", CapturedString(id)),
        #(
          "email",
          optional_captured_string(case customer {
            Some(record) -> record.email
            None -> None
          }),
        ),
        #(
          "displayName",
          optional_captured_string(case customer {
            Some(record) -> record.display_name
            None -> None
          }),
        ),
      ])
    }
  }
}

fn build_draft_order_address(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) ->
      CapturedObject([
        #(
          "firstName",
          optional_captured_string(read_string(input, "firstName")),
        ),
        #("lastName", optional_captured_string(read_string(input, "lastName"))),
        #("address1", optional_captured_string(read_string(input, "address1"))),
        #("city", optional_captured_string(read_string(input, "city"))),
        #(
          "provinceCode",
          optional_captured_string(read_string(input, "provinceCode")),
        ),
        #(
          "countryCodeV2",
          optional_captured_string(
            read_string(input, "countryCodeV2")
            |> option.or(read_string(input, "countryCode")),
          ),
        ),
        #("zip", optional_captured_string(read_string(input, "zip"))),
      ])
  }
}

fn build_order_update_address(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) ->
      CapturedObject([
        #(
          "firstName",
          optional_captured_string(read_string(input, "firstName")),
        ),
        #("lastName", optional_captured_string(read_string(input, "lastName"))),
        #("address1", optional_captured_string(read_string(input, "address1"))),
        #("address2", optional_captured_string(read_string(input, "address2"))),
        #("company", optional_captured_string(read_string(input, "company"))),
        #("city", optional_captured_string(read_string(input, "city"))),
        #("province", optional_captured_string(read_string(input, "province"))),
        #(
          "provinceCode",
          optional_captured_string(read_string(input, "provinceCode")),
        ),
        #("country", optional_captured_string(read_string(input, "country"))),
        #(
          "countryCodeV2",
          optional_captured_string(
            read_string(input, "countryCodeV2")
            |> option.or(read_string(input, "countryCode")),
          ),
        ),
        #("zip", optional_captured_string(read_string(input, "zip"))),
        #("phone", optional_captured_string(read_string(input, "phone"))),
      ])
  }
}

fn build_draft_order_shipping_line(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) -> {
      let money = read_object(input, "priceWithCurrency")
      let amount = case money {
        Some(money) -> read_string(money, "amount") |> option.unwrap("0")
        None -> "0"
      }
      let currency_code = case money {
        Some(money) ->
          read_string(money, "currencyCode") |> option.unwrap("CAD")
        None -> "CAD"
      }
      let amount = parse_amount(amount)
      CapturedObject([
        #("title", optional_captured_string(read_string(input, "title"))),
        #("code", CapturedString("custom")),
        #("custom", CapturedBool(True)),
        #("originalPriceSet", money_set(amount, currency_code)),
        #("discountedPriceSet", money_set(amount, currency_code)),
      ])
    }
  }
}

fn build_draft_order_applied_discount(
  input: Option(Dict(String, root_field.ResolvedValue)),
  currency_code: String,
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) -> {
      let amount =
        read_number(input, "amount")
        |> option.or(read_number(input, "value"))
        |> option.unwrap(0.0)
      CapturedObject([
        #("title", optional_captured_string(read_string(input, "title"))),
        #(
          "description",
          optional_captured_string(read_string(input, "description")),
        ),
        #("value", captured_number(input, "value")),
        #(
          "valueType",
          optional_captured_string(read_string(input, "valueType")),
        ),
        #("amountSet", money_set(amount, currency_code)),
      ])
    }
  }
}

fn captured_attributes(
  attributes: List(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  CapturedArray(
    attributes
    |> list.map(fn(attribute) {
      CapturedObject([
        #("key", optional_captured_string(read_string(attribute, "key"))),
        #("value", optional_captured_string(read_string(attribute, "value"))),
      ])
    }),
  )
}

fn money_set(amount: Float, currency_code: String) -> CapturedJsonValue {
  CapturedObject([
    #(
      "shopMoney",
      CapturedObject([
        #("amount", CapturedString(float.to_string(amount))),
        #("currencyCode", CapturedString(currency_code)),
      ]),
    ),
  ])
}

fn captured_money_amount(value: CapturedJsonValue, name: String) -> Float {
  case captured_object_field(value, name) {
    Some(money) -> captured_money_value(money)
    None -> 0.0
  }
}

fn captured_money_value(value: CapturedJsonValue) -> Float {
  case captured_object_field(value, "shopMoney") {
    Some(shop_money) ->
      case captured_object_field(shop_money, "amount") {
        Some(CapturedString(amount)) -> parse_amount(amount)
        _ -> 0.0
      }
    None -> 0.0
  }
}

fn discount_amount(discount: CapturedJsonValue, base: Float) -> Float {
  case discount {
    CapturedNull -> 0.0
    _ -> {
      let amount = captured_money_amount(discount, "amountSet")
      case captured_string_field(discount, "valueType") {
        Some("PERCENTAGE") ->
          case captured_number_field(discount, "value") {
            Some(percent) -> base *. percent /. 100.0
            None -> amount
          }
        _ -> amount
      }
    }
  }
}

fn draft_order_currency(
  input: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
) -> String {
  case read_object(input, "shippingLine") {
    Some(shipping) ->
      case read_object(shipping, "priceWithCurrency") {
        Some(money) ->
          read_string(money, "currencyCode") |> option.unwrap("CAD")
        None -> line_item_currency(line_items)
      }
    None -> line_item_currency(line_items)
  }
}

fn line_item_currency(line_items: List(CapturedJsonValue)) -> String {
  line_items
  |> list.find_map(fn(item) {
    case captured_object_field(item, "originalUnitPriceSet") {
      Some(money) ->
        case captured_object_field(money, "shopMoney") {
          Some(shop_money) ->
            case captured_object_field(shop_money, "currencyCode") {
              Some(CapturedString(value)) -> Ok(value)
              _ -> Error(Nil)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> result.unwrap("CAD")
}

fn total_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, item) {
    sum
    + case captured_object_field(item, "quantity") {
      Some(CapturedInt(quantity)) -> quantity
      _ -> 0
    }
  })
}

fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
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

fn read_string_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_int(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Int,
) -> Int {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> value
    _ -> fallback
  }
}

fn read_bool(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> fallback
  }
}

fn read_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Float) {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.StringVal(value)) -> Some(parse_amount(value))
    _ -> None
  }
}

fn captured_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> CapturedJsonValue {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> CapturedInt(value)
    Ok(root_field.FloatVal(value)) -> CapturedFloat(value)
    Ok(root_field.StringVal(value)) -> CapturedString(value)
    _ -> CapturedNull
  }
}

fn parse_amount(value: String) -> Float {
  float.parse(value) |> result.unwrap(0.0)
}

fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn max_float(left: Float, right: Float) -> Float {
  case left >. right {
    True -> left
    False -> right
  }
}

fn captured_object_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        let #(key, item) = pair
        case key == name {
          True -> Ok(item)
          False -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

fn captured_number_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Float) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_float(value))
    Some(CapturedFloat(value)) -> Some(value)
    Some(CapturedString(value)) -> Some(parse_amount(value))
    _ -> None
  }
}

fn captured_int_field(value: CapturedJsonValue, name: String) -> Option(Int) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

fn handle_access_denied_guardrail(
  root_name: String,
  field: Selection,
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let required_access = access_denied_required_access(root_name)
  let error = access_denied_error(root_name, required_access)
  let draft =
    single_root_log_draft(
      root_name,
      [],
      store.Failed,
      "orders",
      "stage-locally",
      Some(root_name <> " failed local access-denied guardrail."),
    )
  #(key, json.null(), [error], [draft])
}

fn access_denied_required_access(root_name: String) -> String {
  case root_name {
    "orderCreateManualPayment" ->
      "`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field."
    "taxSummaryCreate" ->
      "`write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on."
    _ -> "`write_orders` access scope."
  }
}

fn access_denied_error(root_name: String, required_access: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Access denied for "
        <> root_name
        <> " field. Required access: "
        <> required_access,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("ACCESS_DENIED")),
        #(
          "documentation",
          json.string("https://shopify.dev/api/usage/access-scopes"),
        ),
        #("requiredAccess", json.string(required_access)),
      ]),
    ),
    #("path", json.array([root_name], json.string)),
  ])
}

fn handle_abandonment_delivery_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "abandonmentUpdateActivitiesDeliveryStatuses",
      [
        RequiredArgument(name: "abandonmentId", expected_type: "ID!"),
        RequiredArgument(name: "marketingActivityId", expected_type: "ID!"),
        RequiredArgument(
          name: "deliveryStatus",
          expected_type: "AbandonmentDeliveryState!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let abandonment_id = read_string_arg(args, "abandonmentId")
      let marketing_activity_id = read_string_arg(args, "marketingActivityId")
      let delivery_status = read_string_arg(args, "deliveryStatus")
      case abandonment_id, marketing_activity_id, delivery_status {
        Some(abandonment_id), Some(marketing_activity_id), Some(delivery_status)
        -> {
          let activity =
            AbandonmentDeliveryActivityRecord(
              marketing_activity_id: marketing_activity_id,
              delivery_status: delivery_status,
              delivered_at: read_string_arg(args, "deliveredAt"),
              delivery_status_change_reason: read_string_arg(
                args,
                "deliveryStatusChangeReason",
              ),
            )
          let #(next_store, updated) =
            store.stage_abandonment_delivery_activity(
              store,
              abandonment_id,
              activity,
            )
          case updated {
            Some(abandonment) -> {
              let payload =
                serialize_abandonment_mutation_payload(
                  next_store,
                  field,
                  Some(abandonment),
                  [],
                  fragments,
                )
              let draft =
                abandonment_log_draft(
                  [abandonment.id],
                  store.Staged,
                  Some(
                    "Locally staged abandonmentUpdateActivitiesDeliveryStatuses in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, identity, [abandonment.id], [], [
                draft,
              ])
            }
            None ->
              unknown_abandonment_result(key, store, identity, field, fragments)
          }
        }
        _, _, _ ->
          unknown_abandonment_result(key, store, identity, field, fragments)
      }
    }
  }
}

fn unknown_abandonment_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let user_errors = [
    #(["abandonmentId"], "abandonment_not_found"),
  ]
  let payload =
    serialize_abandonment_mutation_payload(
      store,
      field,
      None,
      user_errors,
      fragments,
    )
  let draft =
    abandonment_log_draft(
      [],
      store.Failed,
      Some(
        "abandonmentUpdateActivitiesDeliveryStatuses failed local validation.",
      ),
    )
  #(key, payload, store, identity, [], [], [draft])
}

fn abandonment_log_draft(
  staged_resource_ids: List(String),
  status: store.EntryStatus,
  notes: Option(String),
) -> LogDraft {
  single_root_log_draft(
    "abandonmentUpdateActivitiesDeliveryStatuses",
    staged_resource_ids,
    status,
    "orders",
    "stage-locally",
    notes,
  )
}

fn serialize_abandonment_mutation_payload(
  store: Store,
  field: Selection,
  abandonment: Option(AbandonmentRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "abandonment" -> #(key, case abandonment {
              Some(record) ->
                serialize_abandonment_node(store, child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_user_error(
  field: Selection,
  error: #(List(String), String),
) -> Json {
  let #(field_path, message) = error
  let source =
    src_object([
      #("field", SrcList(list.map(field_path, SrcString))),
      #("message", SrcString(message)),
    ])
  project_graphql_value(source, selection_children(field), dict.new())
}

fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.fold(dict.new(), fn(acc, pair) {
          let #(key, item) = pair
          dict.insert(acc, key, captured_json_source(item))
        }),
      )
  }
}

fn selection_children(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn field_arguments(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

fn read_string_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  field_arguments(field, variables) |> read_string_arg(name)
}

fn read_string_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_int_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Int) {
  case dict.get(field_arguments(field, variables), name) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
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

fn min_int(left: Int, right: Int) -> Int {
  case left < right {
    True -> left
    False -> right
  }
}

//// Incremental Orders-domain port.
////
//// This pass intentionally claims only the abandoned-checkout/abandonment
//// roots backed by checked-in executable parity fixtures. The broader order,
//// draft-order, fulfillment, refund, and return roots remain unported.

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
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type DraftOrderRecord, type DraftOrderVariantCatalogRecord, type OrderRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, DraftOrderRecord,
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
      "orderCreate",
      "orderCreateManualPayment",
      "orderEditAddVariant",
      "orderEditBegin",
      "orderEditCommit",
      "orderEditSetQuantity",
      "orderUpdate",
      "taxSummaryCreate",
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
    "orders" -> serialize_orders(store, field, fragments, variables)
    "ordersCount" -> serialize_orders_count(store, field, fragments)
    _ -> json.null()
  }
}

fn serialize_order_node(
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    captured_json_source(order.data),
    selection_children(field),
    fragments,
  )
}

fn serialize_orders(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let orders = store.list_effective_orders(store)
  let args = field_arguments(field, variables)
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

fn serialize_orders_count(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("count", SrcInt(list.length(store.list_effective_orders(store)))),
      #("precision", SrcString("EXACT")),
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
          let #(key, payload, next_errors) =
            handle_fulfillment_validation_guardrail(
              name.value,
              document,
              operation_path,
              field,
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
          let #(key, payload, next_errors) =
            handle_order_create_validation_guardrail(
              document,
              operation_path,
              field,
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
        Field(name: name, ..) if name.value == "orderUpdate" -> {
          let #(key, payload, next_errors) =
            handle_order_update_validation_guardrail(
              operation_path,
              field,
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
        Field(name: name, ..)
          if name.value == "orderEditAddVariant"
          || name.value == "orderEditBegin"
          || name.value == "orderEditCommit"
          || name.value == "orderEditSetQuantity"
        -> {
          let #(key, payload, next_errors) =
            handle_order_edit_validation_guardrail(
              name.value,
              document,
              operation_path,
              field,
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

fn handle_order_edit_validation_guardrail(
  root_name: String,
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
      root_name,
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
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

fn handle_order_update_validation_guardrail(
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
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
    [_, ..] -> #(key, json.null(), errors)
    [] -> #(key, json.null(), [])
  }
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
    [] -> #(key, json.null(), [])
  }
}

fn handle_fulfillment_validation_guardrail(
  root_name: String,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
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
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
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

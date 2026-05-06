//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcInt, SrcList,
  SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_field_or_null, captured_int_field, captured_json_source,
  captured_object_field, captured_string_field, connection_nodes,
  field_arguments, fulfillment_order_line_items,
  fulfillment_order_merchant_requests, fulfillment_order_supported_actions,
  fulfillment_source_line_item_id, fulfillment_source_line_item_title,
  has_pending_cancellation_request, inferred_user_error, min_int, option_is_in,
  option_to_result, optional_captured_string, order_fulfillment_holds,
  order_fulfillment_orders, order_fulfillments, read_bool_argument, read_int,
  read_int_argument, read_object_list, read_string, read_string_list,
  replace_captured_object_fields, selection_children,
  serialize_captured_selection, serialize_order_mutation_user_error,
  serialize_user_error,
}

import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderMutationUserError, type ReverseDeliveryMutationResult,
  ReverseDeliveryMutationResult,
}
import shopify_draft_proxy/proxy/payments/serializers as payment_serializers

import shopify_draft_proxy/search_query_parser

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type DraftOrderRecord, type OrderRecord, type ProductMetafieldRecord,
  CapturedArray, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  OrderRecord,
}

@internal
pub fn serialize_order_node(
  store: Option(Store),
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
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
            "fulfillmentOrders" -> #(
              key,
              serialize_order_fulfillment_orders_connection(
                child,
                order_fulfillment_orders(order.data),
                fragments,
              ),
            )
            "metafield" -> #(key, case store {
              Some(store) -> {
                let value =
                  serialize_order_metafield(store, order.id, child, variables)
                case json.to_string(value) {
                  "null" ->
                    project_graphql_field_value(source, child, fragments)
                  _ -> value
                }
              }
              None -> project_graphql_field_value(source, child, fragments)
            })
            "metafields" -> #(key, case store {
              Some(store) -> {
                case
                  store.get_effective_metafields_by_owner_id(store, order.id)
                {
                  [] -> project_graphql_field_value(source, child, fragments)
                  _ ->
                    serialize_order_metafields_connection(
                      store,
                      order.id,
                      child,
                      variables,
                    )
                }
              }
              None -> project_graphql_field_value(source, child, fragments)
            })
            "paymentTerms" -> #(key, case store {
              Some(store) ->
                serialize_owner_payment_terms(
                  store,
                  order.id,
                  child,
                  fragments,
                  source,
                )
              None -> project_graphql_field_value(source, child, fragments)
            })
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_owner_payment_terms(
  store: Store,
  owner_id: String,
  field: Selection,
  fragments: FragmentMap,
  fallback_source: SourceValue,
) -> Json {
  case store.payment_terms_owner_exists(store, owner_id) {
    True ->
      case store.get_effective_payment_terms_by_owner_id(store, owner_id) {
        Some(terms) ->
          project_graphql_value(
            payment_serializers.payment_terms_source(terms),
            selection_children(field),
            fragments,
          )
        None -> json.null()
      }
    False -> project_graphql_field_value(fallback_source, field, fragments)
  }
}

@internal
pub fn serialize_order_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_arguments(field, variables)
  let namespace = read_string(args, "namespace")
  let key = read_string(args, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        order_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_order_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_arguments(field, variables)
  let namespace = read_string(args, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(order_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

@internal
pub fn order_metafield_to_core(
  record: ProductMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: record.json_value,
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
}

@internal
pub fn serialize_order_fulfillment_orders_connection(
  field: Selection,
  fulfillment_orders: List(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: fulfillment_orders,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(fulfillment_order, _index) {
        captured_string_field(fulfillment_order, "id") |> option.unwrap("")
      },
      serialize_node: fn(fulfillment_order, selection, _index) {
        serialize_order_fulfillment_order(
          selection,
          fulfillment_order,
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

@internal
pub fn serialize_order_fulfillment_order(
  field: Selection,
  fulfillment_order: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(fulfillment_order)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "lineItems" -> #(
              key,
              serialize_order_fulfillment_order_line_items_connection(
                child,
                fulfillment_order_line_items(fulfillment_order),
                fragments,
              ),
            )
            "supportedActions" -> #(
              key,
              json.array(
                fulfillment_order_supported_actions(fulfillment_order),
                fn(action) {
                  project_graphql_value(
                    src_object([#("action", SrcString(action))]),
                    selection_children(child),
                    fragments,
                  )
                },
              ),
            )
            "assignedLocation" -> #(
              key,
              serialize_fulfillment_order_assigned_location(
                child,
                captured_object_field(fulfillment_order, "assignedLocation"),
                fragments,
              ),
            )
            "merchantRequests" -> #(
              key,
              serialize_fulfillment_order_merchant_requests_connection(
                child,
                fulfillment_order,
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

@internal
pub fn serialize_order_fulfillment_order_line_items_connection(
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
        serialize_fulfillment_order_line_item(selection, line_item, fragments)
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

@internal
pub fn serialize_fulfillment_order_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(line_item)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "lineItem" -> #(
              key,
              serialize_fulfillment_source_line_item(
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

@internal
pub fn serialize_fulfillment_source_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  case captured_object_field(line_item, "lineItem") {
    Some(source_line_item) ->
      serialize_captured_selection(field, Some(source_line_item), fragments)
    None -> {
      let source =
        src_object([
          #("id", SrcString(fulfillment_source_line_item_id(line_item))),
          #("title", SrcString(fulfillment_source_line_item_title(line_item))),
          #(
            "quantity",
            SrcInt(
              captured_int_field(line_item, "lineItemQuantity")
              |> option.unwrap(0),
            ),
          ),
          #(
            "fulfillableQuantity",
            SrcInt(
              captured_int_field(line_item, "lineItemFulfillableQuantity")
              |> option.unwrap(
                captured_int_field(line_item, "remainingQuantity")
                |> option.unwrap(0),
              ),
            ),
          ),
        ])
      project_graphql_value(source, selection_children(field), fragments)
    }
  }
}

@internal
pub fn serialize_fulfillment_order_assigned_location(
  field: Selection,
  assigned_location: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  case assigned_location {
    Some(assigned_location) -> {
      let source = captured_json_source(assigned_location)
      let entries =
        list.map(selection_children(field), fn(child) {
          let key = get_field_response_key(child)
          case child {
            Field(name: name, ..) ->
              case name.value {
                "location" -> {
                  let location_id =
                    captured_string_field(assigned_location, "locationId")
                    |> option.unwrap("")
                  let location_name =
                    captured_string_field(assigned_location, "name")
                    |> option.unwrap("")
                  #(
                    key,
                    project_graphql_value(
                      src_object([
                        #("id", SrcString(location_id)),
                        #("name", SrcString(location_name)),
                      ]),
                      selection_children(child),
                      fragments,
                    ),
                  )
                }
                _ -> #(
                  key,
                  project_graphql_field_value(source, child, fragments),
                )
              }
            _ -> #(key, json.null())
          }
        })
      json.object(entries)
    }
    None -> json.null()
  }
}

@internal
pub fn serialize_fulfillment_order_merchant_requests_connection(
  field: Selection,
  fulfillment_order: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let requests = fulfillment_order_merchant_requests(fulfillment_order)
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: requests,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(request, _index) {
        captured_string_field(request, "id") |> option.unwrap("")
      },
      serialize_node: fn(request, selection, _index) {
        serialize_fulfillment_order_merchant_request(
          selection,
          fulfillment_order,
          request,
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

@internal
pub fn serialize_fulfillment_order_merchant_request(
  field: Selection,
  fulfillment_order: CapturedJsonValue,
  request: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(request)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillmentOrder" -> #(
              key,
              serialize_order_fulfillment_order(
                child,
                fulfillment_order,
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

@internal
pub fn serialize_fulfillment_orders_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let include_closed =
    read_bool_argument(field, "includeClosed", variables)
    |> option.unwrap(False)
  let items =
    list_effective_fulfillment_orders(store)
    |> list.filter(fn(pair) {
      let #(_, fulfillment_order) = pair
      include_closed
      || captured_string_field(fulfillment_order, "status") != Some("CLOSED")
    })
    |> list.map(fn(pair) {
      let #(_, fulfillment_order) = pair
      fulfillment_order
    })
  serialize_order_fulfillment_orders_connection(field, items, fragments)
}

@internal
pub fn serialize_manual_holds_fulfillment_orders(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items =
    list_effective_fulfillment_orders(store)
    |> list.filter(fn(pair) {
      let #(_, fulfillment_order) = pair
      captured_string_field(fulfillment_order, "status") == Some("ON_HOLD")
      || !list.is_empty(order_fulfillment_holds(fulfillment_order))
    })
    |> list.map(fn(pair) {
      let #(_, fulfillment_order) = pair
      fulfillment_order
    })
  serialize_order_fulfillment_orders_connection(field, items, fragments)
}

@internal
pub fn serialize_assigned_fulfillment_orders(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_arguments(field, variables)
  let location_ids = read_string_list(args, "locationIds")
  let assignment_status = read_string(args, "assignmentStatus")
  let items =
    list_effective_fulfillment_orders(store)
    |> list.filter(fn(pair) {
      let #(_, fulfillment_order) = pair
      captured_string_field(fulfillment_order, "status") != Some("CLOSED")
      && matches_assigned_fulfillment_order_status(
        fulfillment_order,
        assignment_status,
      )
      && matches_assigned_fulfillment_order_location(
        fulfillment_order,
        location_ids,
      )
    })
    |> list.map(fn(pair) {
      let #(_, fulfillment_order) = pair
      fulfillment_order
    })
  serialize_order_fulfillment_orders_connection(field, items, fragments)
}

@internal
pub fn matches_assigned_fulfillment_order_status(
  fulfillment_order: CapturedJsonValue,
  assignment_status: Option(String),
) -> Bool {
  case assignment_status {
    None -> True
    Some("FULFILLMENT_REQUESTED") ->
      captured_string_field(fulfillment_order, "requestStatus")
      == Some("SUBMITTED")
    Some("FULFILLMENT_ACCEPTED") ->
      captured_string_field(fulfillment_order, "requestStatus")
      == Some("ACCEPTED")
      && !has_pending_cancellation_request(fulfillment_order)
    Some("CANCELLATION_REQUESTED") ->
      has_pending_cancellation_request(fulfillment_order)
    Some(_) -> False
  }
}

@internal
pub fn matches_assigned_fulfillment_order_location(
  fulfillment_order: CapturedJsonValue,
  location_ids: List(String),
) -> Bool {
  case location_ids {
    [] -> True
    [_, ..] ->
      case captured_object_field(fulfillment_order, "assignedLocation") {
        Some(assigned_location) ->
          captured_string_field(assigned_location, "locationId")
          |> option_is_in(location_ids)
        None -> False
      }
  }
}

@internal
pub fn list_effective_fulfillment_orders(
  store: Store,
) -> List(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.flat_map(fn(order) {
    order_fulfillment_orders(order.data)
    |> list.map(fn(fulfillment_order) { #(order, fulfillment_order) })
  })
}

@internal
pub fn serialize_orders(
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

@internal
pub fn serialize_order_connection_node(
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

@internal
pub fn order_cursor(order: OrderRecord, _index: Int) -> String {
  order.cursor |> option.unwrap(order.id)
}

@internal
pub fn filter_orders_by_query(
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

@internal
pub fn order_matches_search_term(
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

@internal
pub fn order_tags(data: CapturedJsonValue) -> List(String) {
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

@internal
pub fn serialize_orders_count(
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

@internal
pub fn serialize_draft_orders(
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
        serialize_draft_order_node(
          Some(store),
          selection,
          draft_order,
          fragments,
        )
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: page_info_options,
    ),
  )
}

@internal
pub fn draft_order_cursor(
  draft_order: DraftOrderRecord,
  _index: Int,
) -> String {
  draft_order.cursor |> option.unwrap(draft_order.id)
}

@internal
pub fn serialize_draft_orders_count(
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

@internal
pub fn serialize_draft_order_available_delivery_options(
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

@internal
pub fn serialize_abandoned_checkouts(
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

@internal
pub fn abandoned_checkout_cursor(
  checkout: AbandonedCheckoutRecord,
  _index: Int,
) -> String {
  checkout.cursor |> option.unwrap(checkout.id)
}

@internal
pub fn serialize_abandoned_checkouts_count(
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

@internal
pub fn serialize_count_payload(
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

@internal
pub fn serialize_abandoned_checkout_node(
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

@internal
pub fn serialize_abandonment_node(
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

@internal
pub fn serialize_abandoned_checkout_payload(
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

@internal
pub fn graphql_helpers_project_field(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_field_value(source, field, fragments)
}

@internal
pub fn serialize_draft_order_node(
  store: Option(Store),
  field: Selection,
  draft_order: DraftOrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(draft_order.data)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "paymentTerms" -> #(key, case store {
              Some(store) ->
                serialize_owner_payment_terms(
                  store,
                  draft_order.id,
                  child,
                  fragments,
                  source,
                )
              None -> project_graphql_field_value(source, child, fragments)
            })
            _ -> #(key, project_graphql_field_value(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_mutation_payload(
  field: Selection,
  order: Option(OrderRecord),
  user_errors: List(OrderMutationUserError),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, case order {
              Some(record) ->
                serialize_order_node(None, child, record, fragments, dict.new())
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_order_mutation_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn build_return_line_items(
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Result(
  #(List(CapturedJsonValue), SyntheticIdentityRegistry),
  List(#(List(String), String, Option(String))),
) {
  let raw_line_items = read_object_list(input, "returnLineItems")
  case raw_line_items {
    [] ->
      Error([
        inferred_user_error(
          ["returnLineItems"],
          "Return must include at least one line item.",
        ),
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
                  inferred_user_error(
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
                let already_returned =
                  fulfillment_line_item_id
                  |> option.map(fn(id) { already_returned_quantity(order, id) })
                  |> option.unwrap(0)
                let remaining_quantity =
                  int.max(0, available_quantity - already_returned)
                case quantity <= 0 || quantity > remaining_quantity {
                  True -> #(
                    items,
                    list.append(errors, [
                      inferred_user_error(
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

@internal
pub fn already_returned_quantity(
  order: OrderRecord,
  fulfillment_line_item_id: String,
) -> Int {
  order_returns(order.data)
  |> list.filter(fn(order_return) {
    captured_string_field(order_return, "status") != Some("CANCELED")
  })
  |> list.flat_map(order_return_line_items)
  |> list.filter(fn(return_line_item) {
    return_line_item_fulfillment_line_item_id(return_line_item)
    == Some(fulfillment_line_item_id)
  })
  |> list.fold(0, fn(total, return_line_item) {
    total
    + { captured_int_field(return_line_item, "quantity") |> option.unwrap(0) }
  })
}

@internal
pub fn return_line_item_fulfillment_line_item_id(
  return_line_item: CapturedJsonValue,
) -> Option(String) {
  case captured_string_field(return_line_item, "fulfillmentLineItemId") {
    Some(id) -> Some(id)
    None ->
      captured_object_field(return_line_item, "fulfillmentLineItem")
      |> option.then(fn(fulfillment_line_item) {
        captured_string_field(fulfillment_line_item, "id")
      })
  }
}

@internal
pub fn build_return_line_item(
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

@internal
pub fn build_order_return(
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

@internal
pub fn ensure_return_reverse_fulfillment_orders(
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

@internal
pub fn build_reverse_fulfillment_order(
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

@internal
pub fn sync_reverse_fulfillment_line_items(
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

@internal
pub fn sync_reverse_fulfillment_order_line_items(
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

@internal
pub fn build_all_reverse_delivery_line_items(
  identity: SyntheticIdentityRegistry,
  reverse_order: CapturedJsonValue,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  reverse_fulfillment_order_line_items(reverse_order)
  |> list.filter(fn(line_item) {
    captured_int_field(line_item, "totalQuantity") |> option.unwrap(0) > 0
  })
  |> list.fold(#([], identity), fn(acc, line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "ReverseDeliveryLineItem",
      )
    #(
      list.append(items, [
        CapturedObject([
          #("id", CapturedString(id)),
          #(
            "reverseFulfillmentOrderLineItemId",
            captured_field_or_null(line_item, "id"),
          ),
          #(
            "quantity",
            CapturedInt(
              captured_int_field(line_item, "totalQuantity")
              |> option.unwrap(0),
            ),
          ),
        ]),
      ]),
      next_identity,
    )
  })
}

@internal
pub fn build_reverse_delivery_line_items(
  identity: SyntheticIdentityRegistry,
  reverse_order: CapturedJsonValue,
  raw_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> Result(
  #(List(CapturedJsonValue), SyntheticIdentityRegistry),
  List(#(List(String), String, Option(String))),
) {
  let #(line_items, user_errors, next_identity) =
    raw_line_items
    |> list.index_fold(#([], [], identity), fn(acc, input, index) {
      let #(items, errors, current_identity) = acc
      let line_item_id = read_string(input, "reverseFulfillmentOrderLineItemId")
      let quantity = read_int(input, "quantity", 0)
      let line_item =
        line_item_id
        |> option.then(fn(id) {
          find_reverse_fulfillment_order_line_item(reverse_order, id)
        })
      case line_item_id, line_item {
        None, _ -> #(
          items,
          list.append(errors, [
            inferred_user_error(
              [
                "reverseDeliveryLineItems",
                int.to_string(index),
                "reverseFulfillmentOrderLineItemId",
              ],
              "Reverse fulfillment order line item does not exist.",
            ),
          ]),
          current_identity,
        )
        Some(_), None -> #(
          items,
          list.append(errors, [
            inferred_user_error(
              [
                "reverseDeliveryLineItems",
                int.to_string(index),
                "reverseFulfillmentOrderLineItemId",
              ],
              "Reverse fulfillment order line item does not exist.",
            ),
          ]),
          current_identity,
        )
        Some(_), Some(line_item) -> {
          let total_quantity =
            captured_int_field(line_item, "totalQuantity") |> option.unwrap(0)
          case quantity <= 0 || quantity > total_quantity {
            True -> #(
              items,
              list.append(errors, [
                inferred_user_error(
                  ["reverseDeliveryLineItems", int.to_string(index), "quantity"],
                  "Quantity is not available for reverse delivery.",
                ),
              ]),
              current_identity,
            )
            False -> {
              let #(id, next_identity) =
                synthetic_identity.make_synthetic_gid(
                  current_identity,
                  "ReverseDeliveryLineItem",
                )
              #(
                list.append(items, [
                  CapturedObject([
                    #("id", CapturedString(id)),
                    #(
                      "reverseFulfillmentOrderLineItemId",
                      captured_field_or_null(line_item, "id"),
                    ),
                    #("quantity", CapturedInt(quantity)),
                  ]),
                ]),
                errors,
                next_identity,
              )
            }
          }
        }
      }
    })
  case user_errors {
    [] -> Ok(#(line_items, next_identity))
    _ -> Error(user_errors)
  }
}

@internal
pub fn normalize_reverse_delivery_tracking(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) ->
      CapturedObject([
        #(
          "number",
          optional_captured_string(
            read_string(input, "number")
            |> option.or(read_string(input, "trackingNumber")),
          ),
        ),
        #(
          "url",
          optional_captured_string(
            read_string(input, "url")
            |> option.or(read_string(input, "trackingUrl")),
          ),
        ),
        #(
          "company",
          optional_captured_string(
            read_string(input, "company")
            |> option.or(read_string(input, "carrierName")),
          ),
        ),
      ])
  }
}

@internal
pub fn normalize_reverse_delivery_label(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) ->
      CapturedObject([
        #(
          "publicFileUrl",
          optional_captured_string(
            read_string(input, "fileUrl")
            |> option.or(read_string(input, "publicFileUrl"))
            |> option.or(read_string(input, "url")),
          ),
        ),
      ])
  }
}

@internal
pub fn stage_reverse_delivery_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  order_return: CapturedJsonValue,
  reverse_order: CapturedJsonValue,
  reverse_delivery: CapturedJsonValue,
) -> ReverseDeliveryMutationResult {
  let updated_return =
    replace_return_reverse_fulfillment_order(order_return, reverse_order)
  let #(next_store, next_identity, updated_order) =
    stage_order_with_return(store, identity, order, updated_return)
  ReverseDeliveryMutationResult(
    Some(updated_order),
    Some(updated_return),
    Some(reverse_order),
    Some(reverse_delivery),
    next_store,
    next_identity,
    [],
  )
}

@internal
pub fn stage_order_with_return(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order: OrderRecord,
  updated_return: CapturedJsonValue,
) -> #(Store, SyntheticIdentityRegistry, OrderRecord) {
  let return_id = captured_string_field(updated_return, "id")
  let returns =
    order_returns(order.data)
    |> list.map(fn(candidate) {
      case captured_string_field(candidate, "id") == return_id {
        True -> updated_return
        False -> candidate
      }
    })
  stage_order_with_returns(store, identity, order, returns)
}

@internal
pub fn replace_return_reverse_fulfillment_order(
  order_return: CapturedJsonValue,
  reverse_order: CapturedJsonValue,
) -> CapturedJsonValue {
  let reverse_order_id = captured_string_field(reverse_order, "id")
  replace_captured_object_fields(order_return, [
    #(
      "reverseFulfillmentOrders",
      CapturedArray(
        order_reverse_fulfillment_orders(order_return)
        |> list.map(fn(candidate) {
          case captured_string_field(candidate, "id") == reverse_order_id {
            True -> reverse_order
            False -> candidate
          }
        }),
      ),
    ),
  ])
}

@internal
pub fn stage_order_with_returns(
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

@internal
pub fn find_order_return(
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

@internal
pub fn find_order_reverse_fulfillment_order(
  store: Store,
  reverse_fulfillment_order_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    order_returns(order.data)
    |> list.find_map(fn(order_return) {
      order_reverse_fulfillment_orders(order_return)
      |> list.find(fn(reverse_order) {
        captured_string_field(reverse_order, "id")
        == Some(reverse_fulfillment_order_id)
      })
      |> option.from_result
      |> option.map(fn(reverse_order) { #(order, order_return, reverse_order) })
      |> option_to_result
    })
  })
  |> option.from_result
}

@internal
pub fn find_order_reverse_delivery(
  store: Store,
  reverse_delivery_id: String,
) -> Option(
  #(OrderRecord, CapturedJsonValue, CapturedJsonValue, CapturedJsonValue),
) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    order_returns(order.data)
    |> list.find_map(fn(order_return) {
      order_reverse_fulfillment_orders(order_return)
      |> list.find_map(fn(reverse_order) {
        reverse_fulfillment_order_reverse_deliveries(reverse_order)
        |> list.find(fn(reverse_delivery) {
          captured_string_field(reverse_delivery, "id")
          == Some(reverse_delivery_id)
        })
        |> option.from_result
        |> option.map(fn(reverse_delivery) {
          #(order, order_return, reverse_order, reverse_delivery)
        })
        |> option_to_result
      })
    })
  })
  |> option.from_result
}

@internal
pub fn find_order_reverse_fulfillment_order_line_item(
  store: Store,
  reverse_line_item_id: String,
) -> Option(
  #(OrderRecord, CapturedJsonValue, CapturedJsonValue, CapturedJsonValue),
) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    order_returns(order.data)
    |> list.find_map(fn(order_return) {
      order_reverse_fulfillment_orders(order_return)
      |> list.find_map(fn(reverse_order) {
        find_reverse_fulfillment_order_line_item(
          reverse_order,
          reverse_line_item_id,
        )
        |> option.map(fn(line_item) {
          #(order, order_return, reverse_order, line_item)
        })
        |> option_to_result
      })
    })
  })
  |> option.from_result
}

@internal
pub fn find_reverse_fulfillment_order_line_item(
  reverse_order: CapturedJsonValue,
  reverse_line_item_id: String,
) -> Option(CapturedJsonValue) {
  reverse_fulfillment_order_line_items(reverse_order)
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(reverse_line_item_id)
  })
  |> option.from_result
}

@internal
pub fn order_returns(order_data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "returns") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn order_return_line_items(
  order_return: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_return, "returnLineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn order_reverse_fulfillment_orders(
  order_return: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_return, "reverseFulfillmentOrders") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn reverse_fulfillment_order_line_items(
  reverse_fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(reverse_fulfillment_order, "lineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn reverse_fulfillment_order_reverse_deliveries(
  reverse_fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(reverse_fulfillment_order, "reverseDeliveries") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn reverse_delivery_line_items(
  reverse_delivery: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(reverse_delivery, "reverseDeliveryLineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn total_return_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, line_item) {
    sum + { captured_int_field(line_item, "quantity") |> option.unwrap(0) }
  })
}

@internal
pub fn find_fulfillment_line_item(
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

@internal
pub fn return_log_draft(
  root_name: String,
  staged_ids: List(String),
  user_errors: List(#(List(String), String, Option(String))),
) -> LogDraft {
  let status = case user_errors {
    [] -> store_types.Staged
    _ -> store_types.Failed
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

@internal
pub fn serialize_return_mutation_payload(
  field: Selection,
  order_return: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String, Option(String))),
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

@internal
pub fn serialize_reverse_delivery_mutation_payload(
  field: Selection,
  reverse_delivery: Option(CapturedJsonValue),
  reverse_order: Option(CapturedJsonValue),
  order_return: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "reverseDelivery" -> #(
              key,
              case reverse_delivery, reverse_order, order_return, order {
                Some(reverse_delivery),
                  Some(reverse_order),
                  Some(order_return),
                  Some(order)
                ->
                  serialize_reverse_delivery(
                    child,
                    reverse_delivery,
                    reverse_order,
                    order_return,
                    order,
                    fragments,
                  )
                _, _, _, _ -> json.null()
              },
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
    })
  json.object(entries)
}

@internal
pub fn serialize_reverse_fulfillment_order_dispose_payload(
  field: Selection,
  line_items: List(CapturedJsonValue),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "reverseFulfillmentOrderLineItems" -> #(
              key,
              json.array(line_items, fn(line_item) {
                serialize_reverse_fulfillment_order_line_item(
                  child,
                  line_item,
                  CapturedObject([]),
                  fragments,
                )
              }),
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
    })
  json.object(entries)
}

@internal
pub fn serialize_order_return(
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
            "order" -> #(
              key,
              serialize_order_node(None, child, order, fragments, dict.new()),
            )
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

@internal
pub fn serialize_order_returns_connection(
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

@internal
pub fn serialize_return_line_items_connection(
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

@internal
pub fn serialize_return_line_item(
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

@internal
pub fn serialize_return_fulfillment_line_item(
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

@internal
pub fn serialize_reverse_fulfillment_orders_connection(
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

@internal
pub fn serialize_reverse_fulfillment_order(
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
            "order" -> #(
              key,
              serialize_order_node(None, child, order, fragments, dict.new()),
            )
            "lineItems" | "reverseFulfillmentOrderLineItems" -> #(
              key,
              serialize_reverse_fulfillment_order_line_items_connection(
                child,
                reverse_fulfillment_order_line_items(reverse_order),
                order_return,
                fragments,
              ),
            )
            "reverseDeliveries" -> #(
              key,
              serialize_reverse_deliveries_connection(
                child,
                reverse_fulfillment_order_reverse_deliveries(reverse_order),
                reverse_order,
                order_return,
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

@internal
pub fn serialize_reverse_delivery(
  field: Selection,
  reverse_delivery: CapturedJsonValue,
  reverse_order: CapturedJsonValue,
  order_return: CapturedJsonValue,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(reverse_delivery)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "reverseFulfillmentOrder" -> #(
              key,
              serialize_reverse_fulfillment_order(
                child,
                reverse_order,
                order_return,
                order,
                fragments,
              ),
            )
            "reverseDeliveryLineItems" -> #(
              key,
              serialize_reverse_delivery_line_items_connection(
                child,
                reverse_delivery_line_items(reverse_delivery),
                reverse_order,
                order_return,
                fragments,
              ),
            )
            "deliverable" -> #(
              key,
              project_graphql_value(
                reverse_delivery_deliverable_source(reverse_delivery),
                selection_children(child),
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

@internal
pub fn reverse_delivery_deliverable_source(
  reverse_delivery: CapturedJsonValue,
) -> SourceValue {
  let tracking = captured_object_field(reverse_delivery, "tracking")
  let label = captured_object_field(reverse_delivery, "label")
  src_object([
    #("__typename", SrcString("ReverseDeliveryShippingDeliverable")),
    #(
      "tracking",
      tracking
        |> option.map(fn(value) {
          src_object([
            #(
              "number",
              captured_string_field(value, "number")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
            #(
              "trackingNumber",
              captured_string_field(value, "number")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
            #(
              "url",
              captured_string_field(value, "url")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
            #(
              "trackingUrl",
              captured_string_field(value, "url")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
            #(
              "company",
              captured_string_field(value, "company")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
            #(
              "carrierName",
              captured_string_field(value, "company")
                |> option.map(SrcString)
                |> option.unwrap(SrcNull),
            ),
          ])
        })
        |> option.unwrap(SrcNull),
    ),
    #(
      "label",
      label
        |> option.map(fn(value) {
          let public_file_url =
            captured_string_field(value, "publicFileUrl")
            |> option.map(SrcString)
            |> option.unwrap(SrcNull)
          src_object([
            #("publicFileUrl", public_file_url),
            #("url", public_file_url),
          ])
        })
        |> option.unwrap(SrcNull),
    ),
  ])
}

@internal
pub fn serialize_reverse_delivery_line_items_connection(
  field: Selection,
  line_items: List(CapturedJsonValue),
  reverse_order: CapturedJsonValue,
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
        serialize_reverse_delivery_line_item(
          selection,
          line_item,
          reverse_order,
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

@internal
pub fn serialize_reverse_delivery_line_item(
  field: Selection,
  line_item: CapturedJsonValue,
  reverse_order: CapturedJsonValue,
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
            "reverseFulfillmentOrderLineItem" -> #(
              key,
              case
                captured_string_field(
                  line_item,
                  "reverseFulfillmentOrderLineItemId",
                )
              {
                Some(id) ->
                  find_reverse_fulfillment_order_line_item(reverse_order, id)
                  |> option.map(fn(reverse_line_item) {
                    serialize_reverse_fulfillment_order_line_item(
                      child,
                      reverse_line_item,
                      order_return,
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

@internal
pub fn serialize_reverse_deliveries_connection(
  field: Selection,
  reverse_deliveries: List(CapturedJsonValue),
  reverse_order: CapturedJsonValue,
  order_return: CapturedJsonValue,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: reverse_deliveries,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(reverse_delivery, _index) {
        captured_string_field(reverse_delivery, "id") |> option.unwrap("")
      },
      serialize_node: fn(reverse_delivery, selection, _index) {
        serialize_reverse_delivery(
          selection,
          reverse_delivery,
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

@internal
pub fn serialize_reverse_fulfillment_order_line_items_connection(
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

@internal
pub fn serialize_reverse_fulfillment_order_line_item(
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
            "quantity" | "totalQuantity" -> #(
              key,
              json.int(
                captured_int_field(line_item, "totalQuantity")
                |> option.unwrap(0),
              ),
            )
            "remainingQuantity" -> #(
              key,
              json.int(
                captured_int_field(line_item, "remainingQuantity")
                |> option.unwrap(0),
              ),
            )
            "dispositionType" -> #(
              key,
              project_graphql_field_value(source, child, fragments),
            )
            "fulfillmentLineItem" -> #(
              key,
              serialize_reverse_fulfillment_line_item(
                child,
                line_item,
                fragments,
              ),
            )
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
            "dispositions" -> #(
              key,
              serialize_reverse_fulfillment_order_line_item_dispositions(
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

@internal
pub fn serialize_reverse_fulfillment_line_item(
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
        SrcInt(
          captured_int_field(line_item, "totalQuantity") |> option.unwrap(0),
        ),
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

@internal
pub fn serialize_reverse_fulfillment_order_line_item_dispositions(
  field: Selection,
  line_item: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let quantity =
    captured_int_field(line_item, "disposedQuantity") |> option.unwrap(0)
  case quantity <= 0, captured_string_field(line_item, "dispositionType") {
    True, _ -> json.array([], fn(value) { value })
    _, None -> json.array([], fn(value) { value })
    False, Some(disposition_type) ->
      json.array(
        [
          src_object([
            #("type", SrcString(disposition_type)),
            #("quantity", SrcInt(quantity)),
            #(
              "location",
              captured_string_field(line_item, "dispositionLocationId")
                |> option.map(fn(id) { src_object([#("id", SrcString(id))]) })
                |> option.unwrap(SrcNull),
            ),
          ]),
        ],
        fn(source) {
          project_graphql_value(source, selection_children(field), fragments)
        },
      )
  }
}

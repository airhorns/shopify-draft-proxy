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
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  find_argument, single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  find_order_with_fulfillment, find_order_with_fulfillment_order,
  option_to_result, read_string_argument, serialize_captured_selection,
}
import shopify_draft_proxy/proxy/orders/order_types.{
  type OrdersError, ParseFailed,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  find_order_return, find_order_reverse_delivery,
  find_order_reverse_fulfillment_order, serialize_abandoned_checkouts,
  serialize_abandoned_checkouts_count, serialize_abandonment_node,
  serialize_assigned_fulfillment_orders,
  serialize_draft_order_available_delivery_options, serialize_draft_order_node,
  serialize_draft_orders, serialize_draft_orders_count,
  serialize_fulfillment_orders_root, serialize_manual_holds_fulfillment_orders,
  serialize_order_fulfillment_order, serialize_order_node,
  serialize_order_return, serialize_orders, serialize_orders_count,
  serialize_reverse_delivery, serialize_reverse_fulfillment_order,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type CustomerRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductMetafieldRecord, type ProductRecord, type ProductVariantRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CustomerOrderSummaryRecord, CustomerRecord, DraftOrderRecord,
  DraftOrderVariantCatalogRecord, OrderRecord, ProductVariantRecord,
}

@internal
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

@internal
pub fn wrap_query_payload(data: Json, search_extensions: List(Json)) -> Json {
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

/// Pattern 1 for cold LiveHybrid orders reads: if no local staged orders state
/// can affect the result, forward the read verbatim to the cassette/upstream.
/// Snapshot mode and locally staged order/draft-order lifecycles stay local.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_upstream = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_order_read(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_upstream {
    True -> passthrough.passthrough_sync(proxy, request)
    False -> respond_query_locally(proxy, document, variables)
  }
}

@internal
pub fn respond_query_locally(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case process(proxy.store, document, variables) {
    Ok(body) -> #(Response(status: 200, body: body, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([
          #(
            "errors",
            json.array(
              [
                json.object([
                  #("message", json.string("Failed to handle orders query")),
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

@internal
pub fn should_passthrough_order_read(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "draftOrder" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "fulfillment" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "fulfillmentOrder" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "order" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "return" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "reverseDelivery" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "reverseFulfillmentOrder" ->
      !local_has_order_domain_id(proxy, variables)
    parse_operation.QueryOperation, "draftOrders" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "draftOrdersCount" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "fulfillmentOrders" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "assignedFulfillmentOrders" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "manualHoldsFulfillmentOrders" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "orders" ->
      !has_local_order_query_state(proxy, variables)
    parse_operation.QueryOperation, "ordersCount" ->
      dict.size(variables) > 0 && !has_local_order_query_state(proxy, variables)
    _, _ -> False
  }
}

@internal
pub fn local_has_order_domain_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id) || local_order_domain_id_exists(proxy, id)
      _ -> False
    }
  })
}

@internal
pub fn local_order_domain_id_exists(proxy: DraftProxy, id: String) -> Bool {
  case store.get_draft_order_by_id(proxy.store, id) {
    Some(_) -> True
    None ->
      case store.get_order_by_id(proxy.store, id) {
        Some(_) -> True
        None ->
          dict.has_key(proxy.store.staged_state.deleted_draft_order_ids, id)
          || dict.has_key(proxy.store.staged_state.deleted_order_ids, id)
          || dict.has_key(proxy.store.staged_state.calculated_orders, id)
          || dict.has_key(proxy.store.base_state.calculated_orders, id)
      }
  }
}

@internal
pub fn has_local_order_query_state(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(value) -> is_proxy_synthetic_gid(value)
        _ -> False
      }
    })
  has_synthetic || has_staged_order_query_state(proxy.store)
}

@internal
pub fn has_staged_order_query_state(store_in: Store) -> Bool {
  dict.size(store_in.staged_state.draft_orders) > 0
  || dict.size(store_in.staged_state.deleted_draft_order_ids) > 0
  || dict.size(store_in.staged_state.orders) > 0
  || dict.size(store_in.staged_state.deleted_order_ids) > 0
  || dict.size(store_in.staged_state.calculated_orders) > 0
}

@internal
pub fn draft_order_search_extensions(
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

@internal
pub fn build_draft_order_search_extension(
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

@internal
pub fn serialize_query_field(
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
    "fulfillment" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case find_order_with_fulfillment(store, id) {
            Some(match) -> {
              let #(_, fulfillment) = match
              serialize_captured_selection(field, Some(fulfillment), fragments)
            }
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "fulfillmentOrder" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case find_order_with_fulfillment_order(store, id) {
            Some(match) -> {
              let #(_, fulfillment_order) = match
              serialize_order_fulfillment_order(
                field,
                fulfillment_order,
                fragments,
              )
            }
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "fulfillmentOrders" ->
      serialize_fulfillment_orders_root(store, field, fragments, variables)
    "assignedFulfillmentOrders" ->
      serialize_assigned_fulfillment_orders(store, field, fragments, variables)
    "manualHoldsFulfillmentOrders" ->
      serialize_manual_holds_fulfillment_orders(
        store,
        field,
        fragments,
        variables,
      )
    "order" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_order_by_id(store, id) {
            Some(order) ->
              serialize_order_node(
                Some(store),
                field,
                order,
                fragments,
                variables,
              )
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
    "reverseDelivery" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case find_order_reverse_delivery(store, id) {
            Some(match) -> {
              let #(order, order_return, reverse_order, reverse_delivery) =
                match
              serialize_reverse_delivery(
                field,
                reverse_delivery,
                reverse_order,
                order_return,
                order,
                fragments,
              )
            }
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "reverseFulfillmentOrder" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case find_order_reverse_fulfillment_order(store, id) {
            Some(match) -> {
              let #(order, order_return, reverse_order) = match
              serialize_reverse_fulfillment_order(
                field,
                reverse_order,
                order_return,
                order,
                fragments,
              )
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

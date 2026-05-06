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
  captured_bool_field, captured_int_field, captured_object_field,
  captured_string_field, captured_supported_actions,
  computed_fulfillment_order_supported_actions, connection_nodes,
  field_arguments, find_captured_replacement, find_order_with_fulfillment,
  find_order_with_fulfillment_order, fulfillment_hold_handle_max_length,
  fulfillment_order_line_items, fulfillment_source_line_item_id,
  fulfillment_source_line_item_title, max_fulfillment_holds_per_api_client,
  nullable_user_error, option_to_result, optional_captured_number,
  optional_captured_string, order_fulfillment_holds, order_fulfillment_orders,
  order_fulfillments, read_object, read_object_list, read_optional_int,
  read_string, replace_captured_object_fields, selection_children,
  serialize_captured_selection, serialize_user_error, upsert_captured_fields,
  user_error,
}
import shopify_draft_proxy/proxy/orders/hydration.{
  maybe_hydrate_order_for_fulfillment_order,
  serialize_fulfillment_mutation_payload, update_order_fulfillment,
}
import shopify_draft_proxy/proxy/orders/order_types.{
  type RequestedFulfillmentLineItem, RequestedFulfillmentLineItem,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
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
pub fn handle_fulfillment_create_invalid_id_guardrail(
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

@internal
pub fn handle_fulfillment_create_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
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
      "fulfillmentCreate",
      [
        RequiredArgument(
          name: "fulfillment",
          expected_type: "FulfillmentInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_object(args, "fulfillment") {
        Some(input) -> {
          let groups =
            fulfillment_create_line_items_by_fulfillment_order_inputs(input)
          let hydrated_store =
            hydrate_store_for_fulfillment_create(store, groups, upstream)
          case first_fulfillment_create_match(hydrated_store, groups) {
            Some(match) -> {
              let #(fulfillment_order_id, order, fulfillment_order) = match
              case
                fulfillment_create_has_missing_fulfillment_order(
                  hydrated_store,
                  groups,
                )
              {
                True ->
                  fulfillment_create_invalid_result(
                    key,
                    hydrated_store,
                    identity,
                  )
                False -> {
                  let precondition_errors =
                    fulfillment_create_precondition_errors(
                      hydrated_store,
                      groups,
                    )
                  case precondition_errors {
                    [_, ..] -> {
                      let payload =
                        serialize_fulfillment_create_payload(
                          field,
                          None,
                          precondition_errors,
                          fragments,
                        )
                      #(key, payload, hydrated_store, identity, [], [], [])
                    }
                    [] -> {
                      let requested_line_items =
                        requested_fulfillment_line_items(input)
                      let #(fulfillment, next_identity) =
                        build_fulfillment_from_order(
                          identity,
                          input,
                          fulfillment_order,
                        )
                      let updated_order =
                        order
                        |> replace_order_fulfillment_order(
                          fulfillment_order_id,
                          close_fulfillment_order(
                            fulfillment_order,
                            requested_line_items,
                          ),
                        )
                        |> append_order_fulfillment(fulfillment)
                      let next_store =
                        store.stage_order(hydrated_store, updated_order)
                      let payload =
                        serialize_fulfillment_create_payload(
                          field,
                          Some(fulfillment),
                          [],
                          fragments,
                        )
                      let draft =
                        single_root_log_draft(
                          "fulfillmentCreate",
                          [
                            captured_string_field(fulfillment, "id")
                            |> option.unwrap(""),
                          ],
                          store_types.Staged,
                          "orders",
                          "stage-locally",
                          Some(
                            "Locally staged fulfillmentCreate in shopify-draft-proxy.",
                          ),
                        )
                      #(
                        key,
                        payload,
                        next_store,
                        next_identity,
                        [updated_order.id],
                        [],
                        [draft],
                      )
                    }
                  }
                }
              }
            }
            None -> fulfillment_create_invalid_result(key, store, identity)
          }
        }
        None -> fulfillment_create_invalid_result(key, store, identity)
      }
    }
  }
}

@internal
pub fn hydrate_store_for_fulfillment_create(
  store: Store,
  groups: List(#(Int, Dict(String, root_field.ResolvedValue))),
  upstream: UpstreamContext,
) -> Store {
  groups
  |> list.fold(store, fn(current_store, group_input) {
    let #(_, group) = group_input
    case read_string(group, "fulfillmentOrderId") {
      Some(fulfillment_order_id) ->
        maybe_hydrate_order_for_fulfillment_order(
          current_store,
          fulfillment_order_id,
          upstream,
        )
      None -> current_store
    }
  })
}

@internal
pub fn fulfillment_create_line_items_by_fulfillment_order_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Int, Dict(String, root_field.ResolvedValue))) {
  read_object_list(input, "lineItemsByFulfillmentOrder")
  |> list.index_map(fn(group, index) { #(index, group) })
}

@internal
pub fn first_fulfillment_create_match(
  store: Store,
  groups: List(#(Int, Dict(String, root_field.ResolvedValue))),
) -> Option(#(String, OrderRecord, CapturedJsonValue)) {
  groups
  |> list.find_map(fn(group_input) {
    let #(_, group) = group_input
    case read_string(group, "fulfillmentOrderId") {
      Some(fulfillment_order_id) ->
        case find_order_with_fulfillment_order(store, fulfillment_order_id) {
          Some(match) -> {
            let #(order, fulfillment_order) = match
            Ok(#(fulfillment_order_id, order, fulfillment_order))
          }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn fulfillment_create_has_missing_fulfillment_order(
  store: Store,
  groups: List(#(Int, Dict(String, root_field.ResolvedValue))),
) -> Bool {
  groups
  |> list.any(fn(group_input) {
    let #(_, group) = group_input
    case read_string(group, "fulfillmentOrderId") {
      Some(fulfillment_order_id) ->
        case find_order_with_fulfillment_order(store, fulfillment_order_id) {
          Some(_) -> False
          None -> True
        }
      None -> True
    }
  })
}

@internal
pub fn fulfillment_create_precondition_errors(
  store: Store,
  groups: List(#(Int, Dict(String, root_field.ResolvedValue))),
) -> List(#(List(String), String, Option(String))) {
  groups
  |> list.flat_map(fn(group_input) {
    let #(group_index, group) = group_input
    case read_string(group, "fulfillmentOrderId") {
      Some(fulfillment_order_id) ->
        case find_order_with_fulfillment_order(store, fulfillment_order_id) {
          Some(match) -> {
            let #(order, fulfillment_order) = match
            fulfillment_create_group_precondition_errors(
              order,
              fulfillment_order,
              group_index,
              group,
            )
          }
          None -> []
        }
      None -> []
    }
  })
}

@internal
pub fn fulfillment_create_group_precondition_errors(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  group_index: Int,
  group: Dict(String, root_field.ResolvedValue),
) -> List(#(List(String), String, Option(String))) {
  case captured_string_field(fulfillment_order, "status") {
    Some("CLOSED") -> [
      user_error(
        ["fulfillment"],
        fulfillment_create_unfulfillable_status_message(
          fulfillment_order,
          "CLOSED",
        ),
        None,
      ),
    ]
    _ ->
      case fulfillment_create_order_is_cancelled(order) {
        True -> [
          user_error(
            ["input", "lineItemsByFulfillmentOrder"],
            "cannot_fulfill_cancelled_order",
            Some(user_error_codes.invalid),
          ),
        ]
        False ->
          fulfillment_create_line_item_quantity_errors(
            fulfillment_order,
            group_index,
            group,
          )
      }
  }
}

fn fulfillment_create_unfulfillable_status_message(
  fulfillment_order: CapturedJsonValue,
  status: String,
) -> String {
  let id =
    captured_string_field(fulfillment_order, "id")
    |> option.then(resource_ids.shopify_gid_tail)
    |> option.unwrap("")
  "Fulfillment order "
  <> id
  <> " has an unfulfillable status= "
  <> string.lowercase(status)
  <> "."
}

@internal
pub fn fulfillment_create_order_is_cancelled(order: OrderRecord) -> Bool {
  case captured_object_field(order.data, "cancelledAt") {
    Some(CapturedString(value)) -> value != ""
    _ -> False
  }
}

@internal
pub fn fulfillment_create_line_item_quantity_errors(
  fulfillment_order: CapturedJsonValue,
  _group_index: Int,
  group: Dict(String, root_field.ResolvedValue),
) -> List(#(List(String), String, Option(String))) {
  read_object_list(group, "fulfillmentOrderLineItems")
  |> list.filter_map(fn(line_item_input) {
    case
      read_string(line_item_input, "id"),
      read_optional_int(line_item_input, "quantity")
    {
      Some(line_item_id), Some(quantity) ->
        case
          fulfillment_order_line_item_by_id(fulfillment_order, line_item_id)
        {
          Some(line_item) -> {
            let remaining_quantity =
              captured_int_field(line_item, "remainingQuantity")
              |> option.or(captured_int_field(line_item, "totalQuantity"))
              |> option.unwrap(0)
            case quantity > remaining_quantity {
              True ->
                Ok(user_error(
                  ["fulfillment"],
                  "Invalid fulfillment order line item quantity requested.",
                  None,
                ))
              False -> Error(Nil)
            }
          }
          None -> Error(Nil)
        }
      _, _ -> Error(Nil)
    }
  })
}

@internal
pub fn fulfillment_order_line_item_by_id(
  fulfillment_order: CapturedJsonValue,
  id: String,
) -> Option(CapturedJsonValue) {
  fulfillment_order_line_items(fulfillment_order)
  |> list.find_map(fn(line_item) {
    case captured_string_field(line_item, "id") == Some(id) {
      True -> Ok(line_item)
      False -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn fulfillment_create_invalid_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let #(_, payload, errors) =
    handle_fulfillment_create_invalid_id_guardrail("fulfillmentCreate")
  #(key, payload, store, identity, [], errors, [])
}

@internal
pub fn handle_fulfillment_event_create_mutation(
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
      "fulfillmentEventCreate",
      [
        RequiredArgument(
          name: "fulfillmentEvent",
          expected_type: "FulfillmentEventInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_object(args, "fulfillmentEvent") {
        Some(input) -> {
          let fulfillment_id = read_string(input, "fulfillmentId")
          case fulfillment_id {
            Some(fulfillment_id) ->
              case find_order_with_fulfillment(store, fulfillment_id) {
                Some(match) -> {
                  let #(order, fulfillment) = match
                  let #(event, next_identity) =
                    build_fulfillment_event(identity, input)
                  let updated_fulfillment =
                    append_fulfillment_event(fulfillment, event)
                  let updated_order =
                    update_order_fulfillment(
                      order,
                      fulfillment_id,
                      updated_fulfillment,
                    )
                  let next_store = store.stage_order(store, updated_order)
                  let payload =
                    serialize_fulfillment_event_create_payload(
                      field,
                      Some(event),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "fulfillmentEventCreate",
                      [captured_string_field(event, "id") |> option.unwrap("")],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged fulfillmentEventCreate in shopify-draft-proxy.",
                      ),
                    )
                  #(
                    key,
                    payload,
                    next_store,
                    next_identity,
                    [updated_order.id],
                    [],
                    [draft],
                  )
                }
                None -> fulfillment_event_invalid_result(key, store, identity)
              }
            None -> fulfillment_event_invalid_result(key, store, identity)
          }
        }
        None -> fulfillment_event_invalid_result(key, store, identity)
      }
    }
  }
}

@internal
pub fn fulfillment_event_invalid_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  #(
    key,
    json.null(),
    store,
    identity,
    [],
    [
      json.object([
        #("message", json.string("invalid id")),
        #(
          "extensions",
          json.object([#("code", json.string("RESOURCE_NOT_FOUND"))]),
        ),
        #("path", json.array(["fulfillmentEventCreate"], json.string)),
      ]),
    ],
    [],
  )
}

@internal
pub fn build_fulfillment_from_order(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  fulfillment_order: CapturedJsonValue,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(fulfillment_id, identity_after_fulfillment) =
    synthetic_identity.make_synthetic_gid(identity, "Fulfillment")
  let #(line_items, identity_after_line_items) =
    build_fulfillment_line_items(
      identity_after_fulfillment,
      fulfillment_order_line_items(fulfillment_order),
      requested_fulfillment_line_items(input),
    )
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_line_items)
  #(
    CapturedObject([
      #("id", CapturedString(fulfillment_id)),
      #("status", CapturedString("SUCCESS")),
      #("displayStatus", CapturedString("FULFILLED")),
      #("createdAt", CapturedString(timestamp)),
      #("updatedAt", CapturedString(timestamp)),
      #("trackingInfo", fulfillment_tracking_info_from_input(input)),
      #(
        "fulfillmentLineItems",
        CapturedObject([#("nodes", CapturedArray(line_items))]),
      ),
      #("events", fulfillment_events_connection([])),
    ]),
    next_identity,
  )
}

@internal
pub fn build_fulfillment_line_items(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  requested: List(RequestedFulfillmentLineItem),
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  line_items
  |> list.filter(fn(line_item) {
    should_fulfill_line_item(line_item, requested)
  })
  |> list.fold(#([], identity), fn(acc, line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "FulfillmentLineItem",
      )
    let fulfillment_order_line_item_id =
      captured_string_field(line_item, "id") |> option.unwrap("")
    let quantity =
      requested_fulfillment_quantity(fulfillment_order_line_item_id, requested)
      |> option.or(captured_int_field(line_item, "remainingQuantity"))
      |> option.or(captured_int_field(line_item, "totalQuantity"))
      |> option.unwrap(0)
    let source_line_item_id = fulfillment_source_line_item_id(line_item)
    let title = fulfillment_source_line_item_title(line_item)
    #(
      list.append(items, [
        CapturedObject([
          #("id", CapturedString(id)),
          #("quantity", CapturedInt(quantity)),
          #(
            "lineItem",
            CapturedObject([
              #("id", CapturedString(source_line_item_id)),
              #("title", CapturedString(title)),
            ]),
          ),
        ]),
      ]),
      next_identity,
    )
  })
}

@internal
pub fn requested_fulfillment_line_items(
  input: Dict(String, root_field.ResolvedValue),
) -> List(RequestedFulfillmentLineItem) {
  read_object_list(input, "lineItemsByFulfillmentOrder")
  |> list.flat_map(fn(group) {
    read_object_list(group, "fulfillmentOrderLineItems")
  })
  |> list.filter_map(fn(item) {
    case read_string(item, "id") {
      Some(id) ->
        Ok(RequestedFulfillmentLineItem(
          id: id,
          quantity: read_optional_int(item, "quantity"),
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn should_fulfill_line_item(
  line_item: CapturedJsonValue,
  requested: List(RequestedFulfillmentLineItem),
) -> Bool {
  case requested {
    [] -> True
    [_, ..] -> {
      let line_item_id = captured_string_field(line_item, "id")
      requested
      |> list.any(fn(request) {
        case request, line_item_id {
          RequestedFulfillmentLineItem(id: requested_id, ..), Some(id) ->
            requested_id == id
          _, _ -> False
        }
      })
    }
  }
}

@internal
pub fn requested_fulfillment_quantity(
  line_item_id: String,
  requested: List(RequestedFulfillmentLineItem),
) -> Option(Int) {
  requested
  |> list.find_map(fn(request) {
    case request {
      RequestedFulfillmentLineItem(id: requested_id, quantity:)
        if requested_id == line_item_id
      ->
        case quantity {
          Some(quantity) -> Ok(quantity)
          None -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn read_fulfillment_order_line_item_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(RequestedFulfillmentLineItem) {
  case dict.get(input, "fulfillmentOrderLineItems") {
    Ok(root_field.ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          root_field.ObjectVal(item) ->
            case read_string(item, "id") {
              Some(id) ->
                case read_optional_int(item, "quantity") {
                  Some(quantity) if quantity > 0 ->
                    Ok(RequestedFulfillmentLineItem(
                      id: id,
                      quantity: Some(quantity),
                    ))
                  _ -> Error(Nil)
                }
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn fulfillment_order_hold_input_from_variables(
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(variables, "fulfillmentHold") {
    Ok(root_field.ObjectVal(input)) -> input
    _ -> dict.new()
  }
}

@internal
pub fn fulfillment_order_hold_handle(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  read_string(input, "handle") |> option.unwrap("")
}

@internal
pub fn fulfillment_order_hold_line_item_input_objects(
  input: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, "fulfillmentOrderLineItems") {
    Ok(root_field.ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          root_field.ObjectVal(item) -> Ok(item)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn fulfillment_order_hold_validation_errors(
  fulfillment_order: CapturedJsonValue,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  let input = fulfillment_order_hold_input_from_variables(variables)
  let line_item_inputs = fulfillment_order_hold_line_item_input_objects(input)
  let input_errors =
    list.append(
      fulfillment_order_hold_line_item_quantity_errors(line_item_inputs),
      fulfillment_order_hold_duplicate_line_item_errors(line_item_inputs),
    )
  case input_errors {
    [_, ..] -> input_errors
    [] -> {
      let handle = fulfillment_order_hold_handle(input)
      case string.length(handle) > fulfillment_hold_handle_max_length {
        True -> [
          nullable_user_error(
            Some(["fulfillmentHold", "handle"]),
            "Handle is too long (maximum is 64 characters)",
            Some(user_error_codes.too_long),
          ),
        ]
        False -> {
          let existing_holds = order_fulfillment_holds(fulfillment_order)
          case
            !list.is_empty(line_item_inputs)
            && captured_string_field(fulfillment_order, "status")
            == Some("ON_HOLD")
          {
            True -> [
              nullable_user_error(
                Some(["fulfillmentHold", "fulfillmentOrderLineItems"]),
                "The fulfillment order is not in a splittable state.",
                Some(user_error_codes.fulfillment_order_not_splittable),
              ),
            ]
            False ->
              case
                fulfillment_order_has_duplicate_hold_handle(
                  existing_holds,
                  handle,
                )
              {
                True -> [
                  nullable_user_error(
                    Some(["fulfillmentHold", "handle"]),
                    "The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order.",
                    Some(user_error_codes.duplicate_fulfillment_hold_handle),
                  ),
                ]
                False ->
                  case
                    fulfillment_order_active_requesting_app_holds(
                      existing_holds,
                    )
                    >= max_fulfillment_holds_per_api_client
                  {
                    True -> [
                      nullable_user_error(
                        Some(["id"]),
                        "The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time.",
                        Some(
                          user_error_codes.fulfillment_order_hold_limit_reached,
                        ),
                      ),
                    ]
                    False -> []
                  }
              }
          }
        }
      }
    }
  }
}

@internal
pub fn fulfillment_order_hold_line_item_quantity_errors(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(Option(List(String)), String, Option(String))) {
  inputs
  |> list.index_fold([], fn(errors, input, index) {
    let invalid_error = case dict.get(input, "quantity") {
      Ok(root_field.IntVal(quantity)) if quantity <= 0 ->
        Some(#(
          "You must select at least one item to place on partial hold.",
          user_error_codes.greater_than_zero,
        ))
      Ok(root_field.IntVal(_)) -> None
      _ ->
        Some(#("The line item quantity is invalid.", user_error_codes.invalid))
    }
    case invalid_error {
      Some(error) ->
        list.append(errors, [
          nullable_user_error(
            Some([
              "fulfillmentHold",
              "fulfillmentOrderLineItems",
              int.to_string(index),
              "quantity",
            ]),
            error.0,
            Some(error.1),
          ),
        ])
      None -> errors
    }
  })
}

@internal
pub fn fulfillment_order_hold_duplicate_line_item_errors(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(Option(List(String)), String, Option(String))) {
  let ids =
    inputs
    |> list.filter_map(fn(input) {
      case read_string(input, "id") {
        Some(id) -> Ok(id)
        None -> Error(Nil)
      }
    })
  case contains_duplicate_string(ids) {
    True -> [
      nullable_user_error(
        Some(["fulfillmentHold", "fulfillmentOrderLineItems"]),
        "must contain unique line item ids",
        Some(user_error_codes.duplicated_fulfillment_order_line_items),
      ),
    ]
    False -> []
  }
}

@internal
pub fn contains_duplicate_string(values: List(String)) -> Bool {
  case values {
    [] -> False
    [first, ..rest] ->
      list.contains(rest, first) || contains_duplicate_string(rest)
  }
}

@internal
pub fn fulfillment_order_has_duplicate_hold_handle(
  holds: List(CapturedJsonValue),
  handle: String,
) -> Bool {
  holds
  |> list.any(fn(hold) {
    fulfillment_hold_held_by_requesting_app(hold)
    && { captured_string_field(hold, "handle") |> option.unwrap("") } == handle
  })
}

@internal
pub fn fulfillment_order_active_requesting_app_holds(
  holds: List(CapturedJsonValue),
) -> Int {
  holds
  |> list.filter(fulfillment_hold_held_by_requesting_app)
  |> list.length
}

@internal
pub fn fulfillment_hold_held_by_requesting_app(
  hold: CapturedJsonValue,
) -> Bool {
  captured_bool_field(hold, "heldByRequestingApp") |> option.unwrap(True)
}

@internal
pub fn fulfillment_tracking_info_from_input(
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_object(input, "trackingInfo") {
    Some(tracking) ->
      CapturedArray([
        CapturedObject([
          #("number", optional_captured_string(read_string(tracking, "number"))),
          #("url", optional_captured_string(read_string(tracking, "url"))),
          #(
            "company",
            optional_captured_string(read_string(tracking, "company")),
          ),
        ]),
      ])
    None -> CapturedArray([])
  }
}

@internal
pub fn split_fulfillment_order_line_items(
  identity: SyntheticIdentityRegistry,
  fulfillment_order: CapturedJsonValue,
  requested: List(RequestedFulfillmentLineItem),
) -> #(
  List(CapturedJsonValue),
  List(CapturedJsonValue),
  SyntheticIdentityRegistry,
) {
  case requested {
    [] -> #(fulfillment_order_line_items(fulfillment_order), [], identity)
    [_, ..] ->
      fulfillment_order_line_items(fulfillment_order)
      |> list.fold(#([], [], identity), fn(acc, line_item) {
        let #(selected, remaining, current_identity) = acc
        let line_item_id =
          captured_string_field(line_item, "id") |> option.unwrap("")
        case requested_fulfillment_quantity(line_item_id, requested) {
          None -> #(
            selected,
            list.append(remaining, [line_item]),
            current_identity,
          )
          Some(quantity) -> {
            let total =
              captured_int_field(line_item, "totalQuantity") |> option.unwrap(0)
            let current_remaining =
              captured_int_field(line_item, "remainingQuantity")
              |> option.unwrap(total)
            let selected_quantity =
              int.min(quantity, int.min(total, current_remaining))
            let remaining_quantity = total - selected_quantity
            let next_selected = case selected_quantity > 0 {
              True ->
                list.append(selected, [
                  replace_captured_object_fields(line_item, [
                    #("totalQuantity", CapturedInt(selected_quantity)),
                    #("remainingQuantity", CapturedInt(selected_quantity)),
                    #(
                      "lineItemFulfillableQuantity",
                      CapturedInt(remaining_quantity),
                    ),
                  ]),
                ])
              False -> selected
            }
            case remaining_quantity > 0 {
              True -> {
                let #(remaining_id, next_identity) =
                  synthetic_identity.make_synthetic_gid(
                    current_identity,
                    "FulfillmentOrderLineItem",
                  )
                let remaining_line_item =
                  replace_captured_object_fields(line_item, [
                    #("id", CapturedString(remaining_id)),
                    #("totalQuantity", CapturedInt(remaining_quantity)),
                    #("remainingQuantity", CapturedInt(remaining_quantity)),
                    #(
                      "lineItemFulfillableQuantity",
                      CapturedInt(remaining_quantity),
                    ),
                  ])
                #(
                  next_selected,
                  list.append(remaining, [remaining_line_item]),
                  next_identity,
                )
              }
              False -> #(next_selected, remaining, current_identity)
            }
          }
        }
      })
  }
}

@internal
pub fn build_replacement_fulfillment_order(
  identity: SyntheticIdentityRegistry,
  fulfillment_order: CapturedJsonValue,
  line_items: List(CapturedJsonValue),
  replacements: List(#(String, CapturedJsonValue)),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "FulfillmentOrder")
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let supported_action_replacements = case
    find_captured_replacement(replacements, "supportedActions")
  {
    Some(_) -> []
    None -> [
      #(
        "supportedActions",
        captured_supported_actions(computed_fulfillment_order_supported_actions(
          Some("OPEN"),
          line_items,
        )),
      ),
    ]
  }
  let replacement =
    fulfillment_order
    |> replace_fulfillment_order_line_items(line_items)
    |> replace_captured_object_fields(list.append(
      [
        #("id", CapturedString(id)),
        #("status", CapturedString("OPEN")),
        #("updatedAt", CapturedString(updated_at)),
        #("requestStatus", CapturedString("UNSUBMITTED")),
        #("fulfillmentHolds", CapturedArray([])),
      ],
      list.append(supported_action_replacements, replacements),
    ))
  #(replacement, next_identity)
}

@internal
pub fn close_fulfillment_order(
  fulfillment_order: CapturedJsonValue,
  requested: List(RequestedFulfillmentLineItem),
) -> CapturedJsonValue {
  let updated_line_items =
    fulfillment_order_line_items(fulfillment_order)
    |> list.map(fn(line_item) {
      case should_fulfill_line_item(line_item, requested) {
        True -> {
          let fulfillment_order_line_item_id =
            captured_string_field(line_item, "id") |> option.unwrap("")
          let current_remaining =
            captured_int_field(line_item, "remainingQuantity")
            |> option.or(captured_int_field(line_item, "totalQuantity"))
            |> option.unwrap(0)
          let fulfilled_quantity =
            requested_fulfillment_quantity(
              fulfillment_order_line_item_id,
              requested,
            )
            |> option.unwrap(current_remaining)
          replace_captured_object_fields(line_item, [
            #(
              "remainingQuantity",
              CapturedInt(int.max(0, current_remaining - fulfilled_quantity)),
            ),
          ])
        }
        False -> line_item
      }
    })
  let updated_line_items_value =
    replace_fulfillment_order_line_items(fulfillment_order, updated_line_items)
  replace_captured_object_fields(updated_line_items_value, [
    #("status", CapturedString("CLOSED")),
  ])
}

@internal
pub fn replace_fulfillment_order_line_items(
  fulfillment_order: CapturedJsonValue,
  line_items: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let replacement = case captured_object_field(fulfillment_order, "lineItems") {
    Some(CapturedObject(fields)) ->
      CapturedObject(
        upsert_captured_fields(fields, [
          #("nodes", CapturedArray(line_items)),
        ]),
      )
    _ -> CapturedArray(line_items)
  }
  replace_captured_object_fields(fulfillment_order, [
    #("lineItems", replacement),
  ])
}

@internal
pub fn replace_order_fulfillment_order(
  order: OrderRecord,
  fulfillment_order_id: String,
  updated_fulfillment_order: CapturedJsonValue,
) -> OrderRecord {
  let updated_fulfillment_orders =
    order_fulfillment_orders(order.data)
    |> list.map(fn(fulfillment_order) {
      case
        captured_string_field(fulfillment_order, "id")
        == Some(fulfillment_order_id)
      {
        True -> updated_fulfillment_order
        False -> fulfillment_order
      }
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("fulfillmentOrders", CapturedArray(updated_fulfillment_orders)),
    ]),
  )
}

@internal
pub fn replace_order_fulfillment_order_with_extras(
  order: OrderRecord,
  fulfillment_order_id: String,
  updated_fulfillment_order: CapturedJsonValue,
  extras: List(CapturedJsonValue),
) -> OrderRecord {
  let updated_fulfillment_orders =
    order_fulfillment_orders(order.data)
    |> list.flat_map(fn(fulfillment_order) {
      case
        captured_string_field(fulfillment_order, "id")
        == Some(fulfillment_order_id)
      {
        True -> [updated_fulfillment_order, ..extras]
        False -> [fulfillment_order]
      }
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("fulfillmentOrders", CapturedArray(updated_fulfillment_orders)),
    ]),
  )
}

@internal
pub fn append_order_fulfillment(
  order: OrderRecord,
  fulfillment: CapturedJsonValue,
) -> OrderRecord {
  let fulfillments = [fulfillment, ..order_fulfillments(order.data)]
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("fulfillments", CapturedArray(fulfillments)),
      #(
        "displayFulfillmentStatus",
        CapturedString(order_display_fulfillment_status_after_create(order)),
      ),
    ]),
  )
}

@internal
pub fn order_display_fulfillment_status_after_create(
  order: OrderRecord,
) -> String {
  let has_open_fulfillment_order =
    order_fulfillment_orders(order.data)
    |> list.any(fn(fulfillment_order) {
      captured_string_field(fulfillment_order, "status") != Some("CLOSED")
      && {
        fulfillment_order_line_items(fulfillment_order)
        |> list.any(fn(line_item) {
          captured_int_field(line_item, "remainingQuantity")
          |> option.unwrap(0)
          > 0
        })
      }
    })
  case has_open_fulfillment_order {
    True -> "PARTIALLY_FULFILLED"
    False -> "FULFILLED"
  }
}

@internal
pub fn build_fulfillment_event(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(event_id, identity_after_event) =
    synthetic_identity.make_synthetic_gid(identity, "FulfillmentEvent")
  let #(created_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_event)
  let happened_at =
    read_string(input, "happenedAt") |> option.unwrap(created_at)
  #(
    CapturedObject([
      #("id", CapturedString(event_id)),
      #("status", optional_captured_string(read_string(input, "status"))),
      #("message", optional_captured_string(read_string(input, "message"))),
      #("happenedAt", CapturedString(happened_at)),
      #("createdAt", CapturedString(created_at)),
      #(
        "estimatedDeliveryAt",
        optional_captured_string(read_string(input, "estimatedDeliveryAt")),
      ),
      #("city", optional_captured_string(read_string(input, "city"))),
      #("province", optional_captured_string(read_string(input, "province"))),
      #("country", optional_captured_string(read_string(input, "country"))),
      #("zip", optional_captured_string(read_string(input, "zip"))),
      #("address1", optional_captured_string(read_string(input, "address1"))),
      #("latitude", optional_captured_number(input, "latitude")),
      #("longitude", optional_captured_number(input, "longitude")),
    ]),
    next_identity,
  )
}

@internal
pub fn append_fulfillment_event(
  fulfillment: CapturedJsonValue,
  event: CapturedJsonValue,
) -> CapturedJsonValue {
  let events =
    case captured_object_field(fulfillment, "events") {
      Some(value) -> connection_nodes(value)
      None -> []
    }
    |> list.append([event])
  let happened_at = captured_string_field(event, "happenedAt")
  let event_status = captured_string_field(event, "status")
  let event_replacements =
    [
      #("updatedAt", event_created_at(event)),
      #(
        "estimatedDeliveryAt",
        captured_object_field(event, "estimatedDeliveryAt")
          |> option.unwrap(CapturedNull),
      ),
    ]
    |> append_status_display_replacement(event_status)
    |> append_event_status_timestamp_replacement(event_status, happened_at)
  replace_captured_object_fields(
    fulfillment,
    list.append(
      [#("events", fulfillment_events_connection(events))],
      event_replacements,
    ),
  )
}

@internal
pub fn append_status_display_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  event_status: Option(String),
) -> List(#(String, CapturedJsonValue)) {
  case event_status {
    Some(status) ->
      list.append(replacements, [
        #("displayStatus", CapturedString(status)),
      ])
    None -> replacements
  }
}

@internal
pub fn append_event_status_timestamp_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  event_status: Option(String),
  happened_at: Option(String),
) -> List(#(String, CapturedJsonValue)) {
  case event_status {
    Some("IN_TRANSIT") ->
      list.append(replacements, [
        #("inTransitAt", optional_captured_string(happened_at)),
      ])
    Some("DELIVERED") ->
      list.append(replacements, [
        #("deliveredAt", optional_captured_string(happened_at)),
      ])
    _ -> replacements
  }
}

@internal
pub fn event_created_at(event: CapturedJsonValue) -> CapturedJsonValue {
  captured_object_field(event, "createdAt") |> option.unwrap(CapturedNull)
}

@internal
pub fn fulfillment_events_connection(
  events: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let cursor = case events {
    [] -> CapturedNull
    [_, ..] -> {
      let last_event =
        events
        |> list.reverse
        |> list.first
        |> result.unwrap(CapturedNull)
      case captured_string_field(last_event, "id") {
        Some(id) -> CapturedString("cursor:" <> id)
        None -> CapturedNull
      }
    }
  }
  CapturedObject([
    #("nodes", CapturedArray(events)),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", cursor),
        #("endCursor", cursor),
      ]),
    ),
  ])
}

@internal
pub fn serialize_fulfillment_create_payload(
  field: Selection,
  fulfillment: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  serialize_fulfillment_mutation_payload(
    field,
    fulfillment,
    user_errors,
    fragments,
  )
}

@internal
pub fn serialize_fulfillment_event_create_payload(
  field: Selection,
  event: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillmentEvent" -> #(
              key,
              serialize_captured_selection(child, event, fragments),
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

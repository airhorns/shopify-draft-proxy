//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcString, get_field_response_key,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_order_helpers.{
  find_unsubmitted_sibling_fulfillment_order, fulfillment_event_missing_result,
  fulfillment_event_payload_json, fulfillment_event_user_error_source,
  fulfillment_event_value, fulfillment_order_merchant_request,
  fulfillment_order_missing_mutation_result,
  fulfillment_order_single_payload_result,
  maybe_append_fulfillment_event_timeline, optional_fulfillment_order_source,
  order_edit_calculated_order_missing_result,
  order_edit_shipping_line_invalid_result, order_edit_shipping_line_payload_json,
  plain_user_error_source, reverse_delivery_missing_delivery_result,
  reverse_delivery_missing_rfo_result, reverse_delivery_payload_json,
  update_fulfillment_for_event,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  append_reverse_delivery, calculated_order_shipping_lines, captured_array_field,
  captured_connection, captured_string_field,
  dispose_reverse_fulfillment_order_line_item,
  find_reverse_fulfillment_order_line_item, make_calculated_shipping_line,
  make_reverse_delivery_line_items, read_bool, read_int, read_object,
  read_object_array, read_string, resolved_args, reverse_delivery_value,
  reverse_line_item_id, update_calculated_order_shipping_lines,
  update_calculated_shipping_line, update_fulfillment_order_fields,
  update_reverse_delivery_shipping, update_reverse_fulfillment_order_line_item,
  zero_fulfillment_order_line_items,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers.{
  fulfillment_order_payload_json,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  captured_json_source, fulfillment_order_source,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type FulfillmentRecord, CapturedBool, CapturedNull, CapturedObject,
  CapturedString, FulfillmentOrderRecord, ReverseDeliveryRecord,
}

@internal
pub fn handle_fulfillment_order_submit_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let message = read_string(args, "message")
          let request_options =
            CapturedObject([
              #("notify_customer", case read_bool(args, "notifyCustomer") {
                Some(value) -> CapturedBool(value)
                None -> CapturedNull
              }),
            ])
          let request =
            fulfillment_order_merchant_request(
              "FULFILLMENT_REQUEST",
              message,
              request_options,
            )
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("OPEN")),
              #("requestStatus", CapturedString("SUBMITTED")),
              #("merchantRequests", captured_connection([request])),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: "OPEN",
              request_status: "SUBMITTED",
              assignment_status: Some("FULFILLMENT_REQUESTED"),
            )
          let #(staged, next_store) =
            store.stage_upsert_fulfillment_order(draft_store, updated)
          let unsubmitted =
            find_unsubmitted_sibling_fulfillment_order(next_store, staged)
          #(
            shipping_types.MutationFieldResult(
              key: key,
              payload: fulfillment_order_payload_json(field, fragments, [
                #(
                  "__typename",
                  SrcString("FulfillmentOrderSubmitFulfillmentRequestPayload"),
                ),
                #("originalFulfillmentOrder", fulfillment_order_source(staged)),
                #("submittedFulfillmentOrder", fulfillment_order_source(staged)),
                #(
                  "unsubmittedFulfillmentOrder",
                  optional_fulfillment_order_source(unsubmitted),
                ),
                #("userErrors", SrcList([])),
              ]),
              errors: [],
              staged_resource_ids: [id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSubmitFulfillmentRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderSubmitFulfillmentRequestPayload",
      )
  }
}

@internal
pub fn handle_fulfillment_order_request_status_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_typename: String,
  request_status: String,
  status: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString(status)),
              #("requestStatus", CapturedString(request_status)),
            ])
          let assignment_status = case request_status {
            "ACCEPTED" -> Some("FULFILLMENT_ACCEPTED")
            "CANCELLATION_REJECTED" -> Some("FULFILLMENT_ACCEPTED")
            _ -> None
          }
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: status,
              request_status: request_status,
              assignment_status: assignment_status,
            )
          fulfillment_order_single_payload_result(
            draft_store,
            identity,
            field,
            fragments,
            payload_typename,
            updated,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            payload_typename,
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        payload_typename,
      )
  }
}

@internal
pub fn handle_fulfillment_order_submit_cancellation_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let existing_requests =
            captured_array_field(order.data, "merchantRequests", "nodes")
          let request =
            fulfillment_order_merchant_request(
              "CANCELLATION_REQUEST",
              read_string(args, "message"),
              CapturedObject([]),
            )
          let updated =
            update_fulfillment_order_fields(order, [
              #(
                "merchantRequests",
                captured_connection(list.append(existing_requests, [request])),
              ),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              assignment_status: Some("CANCELLATION_REQUESTED"),
            )
          fulfillment_order_single_payload_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSubmitCancellationRequestPayload",
            updated,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSubmitCancellationRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderSubmitCancellationRequestPayload",
      )
  }
}

@internal
pub fn handle_fulfillment_order_accept_cancellation_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("CLOSED")),
              #("requestStatus", CapturedString("CANCELLATION_ACCEPTED")),
              #(
                "lineItems",
                zero_fulfillment_order_line_items(order.data, None),
              ),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: "CLOSED",
              request_status: "CANCELLATION_ACCEPTED",
              assignment_status: None,
            )
          fulfillment_order_single_payload_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderAcceptCancellationRequestPayload",
            updated,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderAcceptCancellationRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderAcceptCancellationRequestPayload",
      )
  }
}

@internal
pub fn handle_fulfillment_event_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let input = read_object(args, "fulfillmentEvent") |> option.unwrap(dict.new())
  case read_string(input, "fulfillmentId") {
    Some(fulfillment_id) ->
      case store.get_effective_fulfillment_by_id(draft_store, fulfillment_id) {
        Some(fulfillment) ->
          case validate_fulfillment_event_create_input(fulfillment, input) {
            Some(user_error) ->
              fulfillment_event_user_error_result(
                draft_store,
                identity,
                field,
                fragments,
                user_error,
              )
            None -> {
              let #(event_id, identity) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "FulfillmentEvent",
                )
              let event = fulfillment_event_value(event_id, input)
              let updated = update_fulfillment_for_event(fulfillment, event)
              let #(staged, next_store) =
                store.stage_upsert_fulfillment(draft_store, updated)
              let #(next_store, identity, staged_resource_ids) =
                maybe_append_fulfillment_event_timeline(
                  next_store,
                  identity,
                  staged,
                  event,
                  input,
                  [staged.id, event_id],
                )
              #(
                shipping_types.MutationFieldResult(
                  key: key,
                  payload: fulfillment_event_payload_json(
                    field,
                    fragments,
                    Some(event),
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: staged_resource_ids,
                ),
                next_store,
                identity,
              )
            }
          }
        None ->
          fulfillment_event_missing_result(
            draft_store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      fulfillment_event_missing_result(draft_store, identity, field, fragments)
  }
}

@internal
pub fn validate_fulfillment_event_create_input(
  fulfillment: FulfillmentRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(shipping_types.FulfillmentEventUserError) {
  case fulfillment_event_fulfillment_is_cancelled(fulfillment) {
    True ->
      Some(shipping_types.FulfillmentEventUserError(
        field: ["fulfillmentEvent", "fulfillmentId"],
        message: "Fulfillment is cancelled.",
        code: "fulfillment_cancelled",
      ))
    False ->
      case read_string(input, "status") {
        Some(status) ->
          case valid_fulfillment_event_status(status) {
            True -> None
            False ->
              Some(shipping_types.FulfillmentEventUserError(
                field: ["fulfillmentEvent", "status"],
                message: "Status is invalid.",
                code: "invalid_status",
              ))
          }
        None ->
          Some(shipping_types.FulfillmentEventUserError(
            field: ["fulfillmentEvent", "status"],
            message: "Status is invalid.",
            code: "invalid_status",
          ))
      }
  }
}

@internal
pub fn fulfillment_event_fulfillment_is_cancelled(
  fulfillment: FulfillmentRecord,
) -> Bool {
  captured_string_field(fulfillment.data, "status") == Some("CANCELLED")
  || captured_string_field(fulfillment.data, "displayStatus")
  == Some("CANCELED")
}

@internal
pub fn valid_fulfillment_event_status(status: String) -> Bool {
  case status {
    "LABEL_PRINTED"
    | "LABEL_PURCHASED"
    | "ATTEMPTED_DELIVERY"
    | "READY_FOR_PICKUP"
    | "CONFIRMED"
    | "IN_TRANSIT"
    | "OUT_FOR_DELIVERY"
    | "DELAYED"
    | "DELIVERED"
    | "CARRIER_PICKED_UP"
    | "FAILURE" -> True
    _ -> False
  }
}

@internal
pub fn fulfillment_event_user_error_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_error: shipping_types.FulfillmentEventUserError,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_event_payload_json(field, fragments, None, [
        fulfillment_event_user_error_source(user_error),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn handle_reverse_delivery_create_with_shipping(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "reverseFulfillmentOrderId") {
    Some(reverse_fulfillment_order_id) ->
      case
        store.get_effective_reverse_fulfillment_order_by_id(
          draft_store,
          reverse_fulfillment_order_id,
        )
      {
        Some(reverse_fulfillment_order) -> {
          let #(delivery_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "ReverseDelivery")
          let #(line_items, identity) =
            make_reverse_delivery_line_items(
              reverse_fulfillment_order,
              read_object_array(args, "reverseDeliveryLineItems"),
              identity,
            )
          let reverse_delivery =
            ReverseDeliveryRecord(
              id: delivery_id,
              reverse_fulfillment_order_id: reverse_fulfillment_order.id,
              data: reverse_delivery_value(delivery_id, args, line_items),
            )
          let updated_reverse_fulfillment_order =
            append_reverse_delivery(reverse_fulfillment_order, reverse_delivery)
          let #(_, next_store) =
            store.stage_upsert_reverse_fulfillment_order(
              draft_store,
              updated_reverse_fulfillment_order,
            )
          let #(staged_delivery, next_store) =
            store.stage_upsert_reverse_delivery(next_store, reverse_delivery)
          #(
            shipping_types.MutationFieldResult(
              key: key,
              payload: reverse_delivery_payload_json(
                next_store,
                field,
                fragments,
                "ReverseDeliveryCreateWithShippingPayload",
                Some(staged_delivery),
                [],
              ),
              errors: [],
              staged_resource_ids: [
                updated_reverse_fulfillment_order.id,
                staged_delivery.id,
              ],
            ),
            next_store,
            identity,
          )
        }
        None ->
          reverse_delivery_missing_rfo_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
          )
      }
    None ->
      reverse_delivery_missing_rfo_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
      )
  }
}

@internal
pub fn handle_reverse_delivery_shipping_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "reverseDeliveryId") {
    Some(reverse_delivery_id) ->
      case
        store.get_effective_reverse_delivery_by_id(
          draft_store,
          reverse_delivery_id,
        )
      {
        Some(reverse_delivery) -> {
          let updated =
            ReverseDeliveryRecord(
              ..reverse_delivery,
              data: update_reverse_delivery_shipping(
                reverse_delivery.data,
                args,
              ),
            )
          let #(staged, next_store) =
            store.stage_upsert_reverse_delivery(draft_store, updated)
          #(
            shipping_types.MutationFieldResult(
              key: key,
              payload: reverse_delivery_payload_json(
                next_store,
                field,
                fragments,
                "ReverseDeliveryShippingUpdatePayload",
                Some(staged),
                [],
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          reverse_delivery_missing_delivery_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
          )
      }
    None ->
      reverse_delivery_missing_delivery_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
      )
  }
}

@internal
pub fn handle_reverse_fulfillment_order_dispose(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let inputs = read_object_array(args, "dispositionInputs")
  let #(next_store, updated_line_items, user_errors, _) =
    list.fold(inputs, #(draft_store, [], [], 0), fn(acc, input) {
      let #(current_store, line_items, errors, index) = acc
      case read_string(input, "reverseFulfillmentOrderLineItemId") {
        Some(line_item_id) ->
          case
            find_reverse_fulfillment_order_line_item(
              current_store,
              line_item_id,
            )
          {
            Some(#(reverse_fulfillment_order, line_item)) -> {
              let quantity = read_int(input, "quantity") |> option.unwrap(0)
              let disposition_type =
                read_string(input, "dispositionType")
                |> option.unwrap("UNKNOWN")
              let updated_line_item =
                dispose_reverse_fulfillment_order_line_item(
                  line_item,
                  quantity,
                  disposition_type,
                )
              let updated_order =
                update_reverse_fulfillment_order_line_item(
                  reverse_fulfillment_order,
                  updated_line_item,
                )
              let #(_, updated_store) =
                store.stage_upsert_reverse_fulfillment_order(
                  current_store,
                  updated_order,
                )
              #(
                updated_store,
                list.append(line_items, [updated_line_item]),
                errors,
                index + 1,
              )
            }
            None -> #(
              current_store,
              line_items,
              list.append(errors, [
                plain_user_error_source(
                  [
                    "dispositionInputs",
                    int.to_string(index),
                    "reverseFulfillmentOrderLineItemId",
                  ],
                  "Reverse fulfillment order line item does not exist.",
                ),
              ]),
              index + 1,
            )
          }
        None -> #(
          current_store,
          line_items,
          list.append(errors, [
            plain_user_error_source(
              [
                "dispositionInputs",
                int.to_string(index),
                "reverseFulfillmentOrderLineItemId",
              ],
              "Reverse fulfillment order line item does not exist.",
            ),
          ]),
          index + 1,
        )
      }
    })
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("ReverseFulfillmentOrderDisposePayload")),
        #(
          "reverseFulfillmentOrderLineItems",
          SrcList(list.map(updated_line_items, captured_json_source)),
        ),
        #("userErrors", SrcList(user_errors)),
      ]),
      errors: [],
      staged_resource_ids: list.map(updated_line_items, reverse_line_item_id),
    ),
    next_store,
    identity,
  )
}

@internal
pub fn handle_order_edit_add_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(calculated_order_id) ->
      case
        store.get_effective_calculated_order_by_id(
          draft_store,
          calculated_order_id,
        )
      {
        Some(calculated_order) -> {
          let input =
            read_object(args, "shippingLine") |> option.unwrap(dict.new())
          case make_calculated_shipping_line(input, identity) {
            Some(#(shipping_line, identity)) -> {
              let updated =
                update_calculated_order_shipping_lines(
                  calculated_order,
                  list.append(
                    calculated_order_shipping_lines(calculated_order),
                    [shipping_line],
                  ),
                )
              let #(staged, next_store) =
                store.stage_upsert_calculated_order(draft_store, updated)
              #(
                shipping_types.MutationFieldResult(
                  key: key,
                  payload: order_edit_shipping_line_payload_json(
                    field,
                    fragments,
                    "OrderEditAddShippingLinePayload",
                    Some(staged),
                    Some(shipping_line),
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: [
                    staged.id,
                    reverse_line_item_id(shipping_line),
                  ],
                ),
                next_store,
                identity,
              )
            }
            None ->
              order_edit_shipping_line_invalid_result(
                draft_store,
                identity,
                field,
                fragments,
                key,
                "OrderEditAddShippingLinePayload",
              )
          }
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditAddShippingLinePayload",
          )
      }
    None ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditAddShippingLinePayload",
      )
  }
}

@internal
pub fn handle_order_edit_remove_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let calculated_order_id = read_string(args, "id")
  let shipping_line_id = read_string(args, "shippingLineId")
  case calculated_order_id, shipping_line_id {
    Some(id), Some(line_id) ->
      case store.get_effective_calculated_order_by_id(draft_store, id) {
        Some(calculated_order) -> {
          let existing = calculated_order_shipping_lines(calculated_order)
          let had_line =
            list.any(existing, fn(line) {
              reverse_line_item_id(line) == line_id
            })
          let updated =
            update_calculated_order_shipping_lines(
              calculated_order,
              list.filter(existing, fn(line) {
                reverse_line_item_id(line) != line_id
              }),
            )
          let #(staged, next_store) =
            store.stage_upsert_calculated_order(draft_store, updated)
          let errors = case had_line {
            True -> []
            False -> [
              plain_user_error_source(
                ["shippingLineId"],
                "Shipping line does not exist",
              ),
            ]
          }
          #(
            shipping_types.MutationFieldResult(
              key: key,
              payload: order_edit_shipping_line_payload_json(
                field,
                fragments,
                "OrderEditRemoveShippingLinePayload",
                Some(staged),
                None,
                errors,
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditRemoveShippingLinePayload",
          )
      }
    _, _ ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditRemoveShippingLinePayload",
      )
  }
}

@internal
pub fn handle_order_edit_update_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let calculated_order_id = read_string(args, "id")
  let shipping_line_id = read_string(args, "shippingLineId")
  let input = read_object(args, "shippingLine") |> option.unwrap(dict.new())
  case calculated_order_id, shipping_line_id {
    Some(id), Some(line_id) ->
      case store.get_effective_calculated_order_by_id(draft_store, id) {
        Some(calculated_order) -> {
          let existing = calculated_order_shipping_lines(calculated_order)
          let updated_lines =
            list.map(existing, fn(line) {
              case reverse_line_item_id(line) == line_id {
                True -> update_calculated_shipping_line(line, input)
                False -> line
              }
            })
          let updated_line =
            updated_lines
            |> list.find(fn(line) { reverse_line_item_id(line) == line_id })
            |> option.from_result
          let updated =
            update_calculated_order_shipping_lines(
              calculated_order,
              updated_lines,
            )
          let #(staged, next_store) =
            store.stage_upsert_calculated_order(draft_store, updated)
          let errors = case updated_line {
            Some(_) -> []
            None -> [
              plain_user_error_source(
                ["shippingLineId"],
                "Shipping line does not exist",
              ),
            ]
          }
          #(
            shipping_types.MutationFieldResult(
              key: key,
              payload: order_edit_shipping_line_payload_json(
                field,
                fragments,
                "OrderEditUpdateShippingLinePayload",
                Some(staged),
                updated_line,
                errors,
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditUpdateShippingLinePayload",
          )
      }
    _, _ ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditUpdateShippingLinePayload",
      )
  }
}

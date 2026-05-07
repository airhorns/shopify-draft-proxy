//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{read_optional_string_array}
import shopify_draft_proxy/proxy/shipping_fulfillments/carrier_services.{
  fulfillment_order_assigned_location_value,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_order_helpers.{
  captured_action_list, close_merge_siblings, close_sibling_fulfillment_orders,
  first_fulfillment_order_line_item_quantity,
  first_fulfillment_order_line_item_total, fulfillment_hold_value,
  fulfillment_order_cancel_user_error_payload,
  fulfillment_order_has_manually_reported_progress,
  fulfillment_order_hold_handle,
  fulfillment_order_hold_supported_actions_for_requester,
  fulfillment_order_hold_user_error_source,
  fulfillment_order_hold_validation_errors, fulfillment_order_holds,
  fulfillment_order_line_items_after_split,
  fulfillment_order_line_items_for_split, fulfillment_order_line_items_total,
  fulfillment_order_line_items_with_line_item_fulfillable,
  fulfillment_order_line_items_with_quantity,
  fulfillment_order_line_items_with_quantity_and_fulfillable,
  fulfillment_order_merge_ids, fulfillment_order_missing_mutation_result,
  fulfillment_order_release_hold_validation_errors,
  fulfillment_order_release_remaining_holds,
  fulfillment_order_single_payload_result,
  fulfillment_order_split_supported_actions, max_int,
  normalize_shopify_timestamp_to_seconds, sibling_fulfillment_order_quantity,
  synthetic_timestamp_string, update_shipping_order_display_status,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  captured_array_field, captured_connection, captured_field, captured_int_field,
  captured_string_field, option_to_captured_string, read_bool, read_object,
  read_object_array, read_string, read_string_array, resolved_args,
  update_fulfillment_order_fields,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers.{
  fulfillment_order_payload_json,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  captured_json_source, fulfillment_order_source, is_active_location,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type FulfillmentOrderRecord, CapturedArray,
  CapturedBool, CapturedObject, CapturedString, FulfillmentOrderRecord,
}

@internal
pub fn handle_fulfillment_order_hold(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let hold_input =
    read_object(args, "fulfillmentHold") |> option.unwrap(dict.new())
  let line_item_inputs =
    read_object_array(hold_input, "fulfillmentOrderLineItems")
  let quantity = first_fulfillment_order_line_item_quantity(line_item_inputs)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          case
            fulfillment_order_hold_validation_errors(
              order,
              hold_input,
              requesting_api_client_id,
            )
          {
            [_, ..] as user_errors -> #(
              shipping_types.MutationFieldResult(
                key: key,
                payload: fulfillment_order_payload_json(field, fragments, [
                  #("__typename", SrcString("FulfillmentOrderHoldPayload")),
                  #("fulfillmentHold", SrcNull),
                  #("fulfillmentOrder", SrcNull),
                  #("remainingFulfillmentOrder", SrcNull),
                  #(
                    "userErrors",
                    SrcList(list.map(
                      user_errors,
                      fulfillment_order_hold_user_error_source,
                    )),
                  ),
                ]),
                errors: [],
                staged_resource_ids: [],
              ),
              draft_store,
              identity,
            )
            [] -> {
              let #(hold_id, identity) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "FulfillmentHold",
                )
              let hold =
                fulfillment_hold_value(
                  hold_id,
                  Some(fulfillment_order_hold_handle(hold_input)),
                  read_string(hold_input, "reason"),
                  read_string(hold_input, "reasonNotes"),
                  read_string(hold_input, "externalId"),
                  read_bool(hold_input, "notifyMerchant")
                    |> option.unwrap(False),
                  requesting_api_client_id,
                )
              let remaining_quantity =
                max_int(
                  first_fulfillment_order_line_item_total(order.data) - quantity,
                  0,
                )
              let existing_holds = fulfillment_order_holds(order.data)
              let fulfillment_holds = list.append(existing_holds, [hold])
              let held_line_items = case line_item_inputs {
                [] ->
                  case list.is_empty(existing_holds) {
                    True ->
                      fulfillment_order_line_items_with_line_item_fulfillable(
                        order.data,
                        0,
                      )
                    False ->
                      captured_field(order.data, "lineItems")
                      |> option.unwrap(CapturedArray([]))
                  }
                [_, ..] ->
                  fulfillment_order_line_items_with_quantity_and_fulfillable(
                    order.data,
                    quantity,
                    remaining_quantity,
                  )
              }
              let held =
                update_fulfillment_order_fields(order, [
                  #("status", CapturedString("ON_HOLD")),
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                  #(
                    "supportedActions",
                    captured_action_list(
                      fulfillment_order_hold_supported_actions_for_requester(
                        fulfillment_holds,
                        requesting_api_client_id,
                      ),
                    ),
                  ),
                  #("fulfillmentHolds", CapturedArray(fulfillment_holds)),
                  #("lineItems", held_line_items),
                ])
              let held =
                FulfillmentOrderRecord(
                  ..held,
                  status: "ON_HOLD",
                  manually_held: True,
                )
              let #(held, next_store) =
                store.stage_upsert_fulfillment_order(draft_store, held)
              let #(remaining, next_store, identity) = case line_item_inputs {
                [] -> #(None, next_store, identity)
                [_, ..] -> {
                  case remaining_quantity > 0 {
                    False -> #(None, next_store, identity)
                    True -> {
                      let #(remaining_id, identity) =
                        synthetic_identity.make_synthetic_gid(
                          identity,
                          "FulfillmentOrder",
                        )
                      let remaining =
                        update_fulfillment_order_fields(order, [
                          #("id", CapturedString(remaining_id)),
                          #("status", CapturedString("OPEN")),
                          #(
                            "updatedAt",
                            CapturedString(synthetic_timestamp_string()),
                          ),
                          #(
                            "supportedActions",
                            captured_action_list(case remaining_quantity > 1 {
                              True -> [
                                "CREATE_FULFILLMENT",
                                "REPORT_PROGRESS",
                                "MOVE",
                                "HOLD",
                                "SPLIT",
                              ]
                              False -> [
                                "CREATE_FULFILLMENT",
                                "REPORT_PROGRESS",
                                "MOVE",
                                "HOLD",
                              ]
                            }),
                          ),
                          #("fulfillmentHolds", CapturedArray([])),
                          #(
                            "lineItems",
                            fulfillment_order_line_items_with_quantity(
                              order.data,
                              remaining_quantity,
                              True,
                            ),
                          ),
                        ])
                      let remaining =
                        FulfillmentOrderRecord(
                          ..remaining,
                          id: remaining_id,
                          status: "OPEN",
                          manually_held: False,
                        )
                      let #(remaining, next_store) =
                        store.stage_upsert_fulfillment_order(
                          next_store,
                          remaining,
                        )
                      #(Some(remaining), next_store, identity)
                    }
                  }
                }
              }
              #(
                shipping_types.MutationFieldResult(
                  key: key,
                  payload: fulfillment_order_payload_json(field, fragments, [
                    #("__typename", SrcString("FulfillmentOrderHoldPayload")),
                    #("fulfillmentHold", captured_json_source(hold)),
                    #("fulfillmentOrder", fulfillment_order_source(held)),
                    #("remainingFulfillmentOrder", case remaining {
                      Some(remaining) -> fulfillment_order_source(remaining)
                      None -> SrcNull
                    }),
                    #("userErrors", SrcList([])),
                  ]),
                  errors: [],
                  staged_resource_ids: case remaining {
                    Some(remaining) -> [held.id, remaining.id]
                    None -> [held.id]
                  },
                ),
                next_store,
                identity,
              )
            }
          }
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderHoldPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderHoldPayload",
      )
  }
}

@internal
pub fn handle_fulfillment_order_release_hold(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let hold_ids = read_optional_string_array(args, "holdIds")
          let holds = fulfillment_order_holds(order.data)
          case
            fulfillment_order_release_hold_validation_errors(
              holds,
              hold_ids,
              requesting_api_client_id,
            )
          {
            [_, ..] as user_errors ->
              fulfillment_order_release_hold_user_error_result(
                draft_store,
                identity,
                field,
                fragments,
                user_errors,
              )
            [] -> {
              let remaining_holds =
                fulfillment_order_release_remaining_holds(
                  holds,
                  hold_ids,
                  requesting_api_client_id,
                )
              case remaining_holds {
                [] -> {
                  let restored_quantity =
                    sibling_fulfillment_order_quantity(draft_store, order)
                    + first_fulfillment_order_line_item_total(order.data)
                  let updated =
                    update_fulfillment_order_fields(order, [
                      #("status", CapturedString("OPEN")),
                      #(
                        "updatedAt",
                        CapturedString(synthetic_timestamp_string()),
                      ),
                      #(
                        "supportedActions",
                        captured_action_list([
                          "CREATE_FULFILLMENT",
                          "REPORT_PROGRESS",
                          "MOVE",
                          "HOLD",
                          "SPLIT",
                        ]),
                      ),
                      #("fulfillmentHolds", CapturedArray([])),
                      #(
                        "lineItems",
                        fulfillment_order_line_items_with_quantity(
                          order.data,
                          restored_quantity,
                          True,
                        ),
                      ),
                    ])
                  let updated =
                    FulfillmentOrderRecord(
                      ..updated,
                      status: "OPEN",
                      manually_held: False,
                    )
                  let #(staged, next_store) =
                    store.stage_upsert_fulfillment_order(draft_store, updated)
                  let next_store =
                    close_sibling_fulfillment_orders(next_store, staged)
                  fulfillment_order_single_payload_result(
                    next_store,
                    identity,
                    field,
                    fragments,
                    "FulfillmentOrderReleaseHoldPayload",
                    staged,
                  )
                }
                [_, ..] -> {
                  let updated =
                    update_fulfillment_order_fields(order, [
                      #("status", CapturedString("ON_HOLD")),
                      #(
                        "updatedAt",
                        CapturedString(synthetic_timestamp_string()),
                      ),
                      #(
                        "supportedActions",
                        captured_action_list(
                          fulfillment_order_hold_supported_actions_for_requester(
                            remaining_holds,
                            requesting_api_client_id,
                          ),
                        ),
                      ),
                      #("fulfillmentHolds", CapturedArray(remaining_holds)),
                    ])
                  let updated =
                    FulfillmentOrderRecord(
                      ..updated,
                      status: "ON_HOLD",
                      manually_held: True,
                    )
                  let #(staged, next_store) =
                    store.stage_upsert_fulfillment_order(draft_store, updated)
                  fulfillment_order_single_payload_result(
                    next_store,
                    identity,
                    field,
                    fragments,
                    "FulfillmentOrderReleaseHoldPayload",
                    staged,
                  )
                }
              }
            }
          }
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderReleaseHoldPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderReleaseHoldPayload",
      )
  }
}

@internal
pub fn handle_fulfillment_order_move(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let line_item_inputs = read_object_array(args, "fulfillmentOrderLineItems")
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          case fulfillment_order_move_block_user_error(order) {
            Some(user_error) ->
              fulfillment_order_move_user_error_payload(
                draft_store,
                identity,
                field,
                fragments,
                user_error,
              )
            None ->
              case
                find_fulfillment_order_move_destination(
                  draft_store,
                  read_string(args, "newLocationId"),
                )
              {
                Some(destination) -> {
                  let total_quantity =
                    first_fulfillment_order_line_item_total(order.data)
                  let quantity = case line_item_inputs {
                    [] -> total_quantity
                    _ ->
                      first_fulfillment_order_line_item_quantity(
                        line_item_inputs,
                      )
                  }
                  let remaining_quantity = max_int(total_quantity - quantity, 0)
                  let original_updates = [
                    #("updatedAt", CapturedString(synthetic_timestamp_string())),
                    #(
                      "supportedActions",
                      captured_action_list([
                        "CREATE_FULFILLMENT",
                        "REPORT_PROGRESS",
                        "MOVE",
                        "HOLD",
                      ]),
                    ),
                    #(
                      "lineItems",
                      fulfillment_order_line_items_with_quantity(
                        order.data,
                        remaining_quantity,
                        False,
                      ),
                    ),
                  ]
                  let original_updates = case remaining_quantity > 0 {
                    True -> original_updates
                    False -> [
                      #("assignedLocation", destination.assigned_location),
                      ..original_updates
                    ]
                  }
                  let original =
                    update_fulfillment_order_fields(order, original_updates)
                  let original = case remaining_quantity > 0 {
                    True -> original
                    False ->
                      FulfillmentOrderRecord(
                        ..original,
                        assigned_location_id: Some(destination.id),
                      )
                  }
                  let #(moved_id, identity) =
                    synthetic_identity.make_synthetic_gid(
                      identity,
                      "FulfillmentOrder",
                    )
                  let moved =
                    update_fulfillment_order_fields(order, [
                      #("id", CapturedString(moved_id)),
                      #(
                        "updatedAt",
                        CapturedString(synthetic_timestamp_string()),
                      ),
                      #("assignedLocation", destination.assigned_location),
                      #(
                        "supportedActions",
                        captured_action_list([
                          "CREATE_FULFILLMENT",
                          "REPORT_PROGRESS",
                          "MOVE",
                          "HOLD",
                        ]),
                      ),
                      #(
                        "lineItems",
                        fulfillment_order_line_items_with_quantity(
                          order.data,
                          quantity,
                          False,
                        ),
                      ),
                    ])
                  let moved =
                    FulfillmentOrderRecord(
                      ..moved,
                      id: moved_id,
                      assigned_location_id: Some(destination.id),
                    )
                  let #(original, next_store) =
                    store.stage_upsert_fulfillment_order(draft_store, original)
                  let #(moved, next_store) =
                    store.stage_upsert_fulfillment_order(next_store, moved)
                  let remaining = case remaining_quantity > 0 {
                    True -> fulfillment_order_source(original)
                    False -> SrcNull
                  }
                  #(
                    shipping_types.MutationFieldResult(
                      key: key,
                      payload: fulfillment_order_payload_json(field, fragments, [
                        #(
                          "__typename",
                          SrcString("FulfillmentOrderMovePayload"),
                        ),
                        #(
                          "movedFulfillmentOrder",
                          fulfillment_order_source(moved),
                        ),
                        #(
                          "originalFulfillmentOrder",
                          fulfillment_order_source(original),
                        ),
                        #("remainingFulfillmentOrder", remaining),
                        #("userErrors", SrcList([])),
                      ]),
                      errors: [],
                      staged_resource_ids: [original.id, moved.id],
                    ),
                    next_store,
                    identity,
                  )
                }
                None ->
                  fulfillment_order_move_user_error_payload(
                    draft_store,
                    identity,
                    field,
                    fragments,
                    fulfillment_order_move_location_not_found_user_error(),
                  )
              }
          }
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderMovePayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderMovePayload",
      )
  }
}

@internal
pub fn find_fulfillment_order_move_destination(
  draft_store: Store,
  location_id: Option(String),
) -> Option(shipping_types.FulfillmentOrderMoveDestination) {
  case location_id {
    Some(id) ->
      case store.get_effective_store_property_location_by_id(draft_store, id) {
        Some(location) ->
          case is_active_location(location) {
            True ->
              Some(shipping_types.FulfillmentOrderMoveDestination(
                id: location.id,
                assigned_location: fulfillment_order_assigned_location_value(
                  location,
                ),
              ))
            False -> None
          }
        None ->
          case store.list_effective_store_property_locations(draft_store) {
            [] ->
              Some(shipping_types.FulfillmentOrderMoveDestination(
                id: id,
                assigned_location: fallback_fulfillment_order_assigned_location(
                  id,
                ),
              ))
            _ -> find_fulfillment_order_assigned_location(draft_store, id)
          }
      }
    None -> None
  }
}

@internal
pub fn find_fulfillment_order_assigned_location(
  draft_store: Store,
  location_id: String,
) -> Option(shipping_types.FulfillmentOrderMoveDestination) {
  store.list_effective_fulfillment_orders(draft_store)
  |> list.find(fn(order) { order.assigned_location_id == Some(location_id) })
  |> option.from_result
  |> option.map(fn(order) {
    shipping_types.FulfillmentOrderMoveDestination(
      id: location_id,
      assigned_location: captured_field(order.data, "assignedLocation")
        |> option.unwrap(fulfillment_order_assigned_location_from_id(
          location_id,
        )),
    )
  })
}

@internal
pub fn fulfillment_order_assigned_location_from_id(
  location_id: String,
) -> CapturedJsonValue {
  CapturedObject([
    #("name", CapturedString("")),
    #(
      "location",
      CapturedObject([
        #("id", CapturedString(location_id)),
        #("name", CapturedString("")),
      ]),
    ),
  ])
}

@internal
pub fn fallback_fulfillment_order_assigned_location(
  location_id: String,
) -> CapturedJsonValue {
  let name = case location_id {
    "gid://shopify/Location/106318430514" -> "Shop location"
    "" -> ""
    _ -> "My Custom Location"
  }
  CapturedObject([
    #("name", CapturedString(name)),
    #(
      "location",
      CapturedObject([
        #("id", CapturedString(location_id)),
        #("name", CapturedString(name)),
      ]),
    ),
  ])
}

@internal
pub fn fulfillment_order_move_block_user_error(
  order: FulfillmentOrderRecord,
) -> Option(SourceValue) {
  case fulfillment_order_has_manually_reported_progress(order) {
    True ->
      Some(fulfillment_order_move_user_error(
        SrcList([SrcString("id")]),
        "Cannot move a fulfillment order that has had progress reported. To move a fulfillment order that has had progress reported, the fulfillment order must first be marked as open resolving the ongoing progress state.",
        SrcString("CANNOT_MOVE_FULFILLMENT_ORDER_WITH_REPORTED_PROGRESS"),
      ))
    False ->
      case order.status == "CLOSED" {
        True ->
          Some(fulfillment_order_move_user_error(
            SrcNull,
            "Cannot change location.",
            SrcNull,
          ))
        False ->
          case order.request_status == "SUBMITTED" {
            True ->
              Some(fulfillment_order_move_user_error(
                SrcNull,
                "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                SrcNull,
              ))
            False ->
              case fulfillment_order_move_blocked_by_request_status(order) {
                True ->
                  Some(fulfillment_order_move_user_error(
                    SrcNull,
                    "Fulfillment order is not actionable.",
                    SrcNull,
                  ))
                False -> None
              }
          }
      }
  }
}

@internal
pub fn fulfillment_order_move_blocked_by_request_status(
  order: FulfillmentOrderRecord,
) -> Bool {
  list.contains(
    [
      "SUBMITTED",
      "ACCEPTED",
      "CANCELLATION_REQUESTED",
      "CANCELLATION_REJECTED",
    ],
    order.request_status,
  )
}

@internal
pub fn fulfillment_order_move_location_not_found_user_error() -> SourceValue {
  fulfillment_order_move_user_error(
    SrcList([SrcString("id")]),
    "Location not found.",
    SrcNull,
  )
}

@internal
pub fn fulfillment_order_move_user_error(
  field: SourceValue,
  message: String,
  code: SourceValue,
) -> SourceValue {
  src_object([
    #("field", field),
    #("message", SrcString(message)),
    #("code", code),
  ])
}

@internal
pub fn fulfillment_order_move_user_error_payload(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_error: SourceValue,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("FulfillmentOrderMovePayload")),
        #("movedFulfillmentOrder", SrcNull),
        #("originalFulfillmentOrder", SrcNull),
        #("remainingFulfillmentOrder", SrcNull),
        #("userErrors", SrcList([user_error])),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn handle_fulfillment_order_simple_status(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_typename: String,
  status: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let actions = case status {
            "IN_PROGRESS" -> [
              "CREATE_FULFILLMENT",
              "REPORT_PROGRESS",
              "HOLD",
              "MARK_AS_OPEN",
            ]
            _ -> ["CREATE_FULFILLMENT", "REPORT_PROGRESS", "MOVE", "HOLD"]
          }
          let status_updates = case status {
            "IN_PROGRESS" -> [
              #("status", CapturedString(status)),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #("supportedActions", captured_action_list(actions)),
              #("__draftProxyManuallyReportedProgress", CapturedBool(True)),
            ]
            "OPEN" -> [
              #("status", CapturedString(status)),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #("supportedActions", captured_action_list(actions)),
              #("__draftProxyManuallyReportedProgress", CapturedBool(False)),
            ]
            _ -> [
              #("status", CapturedString(status)),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #("supportedActions", captured_action_list(actions)),
            ]
          }
          let updated = update_fulfillment_order_fields(order, status_updates)
          let updated = FulfillmentOrderRecord(..updated, status: status)
          let draft_store =
            update_shipping_order_display_status(draft_store, updated, status)
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

fn fulfillment_order_release_hold_user_error_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(#(List(String), String, String)),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("FulfillmentOrderReleaseHoldPayload")),
        #("fulfillmentOrder", SrcNull),
        #(
          "userErrors",
          SrcList(list.map(
            user_errors,
            fulfillment_order_hold_user_error_source,
          )),
        ),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn handle_fulfillment_order_cancel(
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
          case fulfillment_order_cancel_block_message(order) {
            Some(message) ->
              fulfillment_order_cancel_user_error_payload(
                draft_store,
                identity,
                field,
                fragments,
                message,
              )
            None -> {
              let canceled =
                update_fulfillment_order_fields(order, [
                  #("status", CapturedString("CLOSED")),
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                  #("supportedActions", CapturedArray([])),
                  #("lineItems", captured_connection([])),
                ])
              let canceled =
                FulfillmentOrderRecord(..canceled, status: "CLOSED")
              let #(replacement_id, identity) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "FulfillmentOrder",
                )
              let replacement =
                update_fulfillment_order_fields(order, [
                  #("id", CapturedString(replacement_id)),
                  #("status", CapturedString("OPEN")),
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                ])
              let replacement =
                FulfillmentOrderRecord(
                  ..replacement,
                  id: replacement_id,
                  status: "OPEN",
                )
              let #(canceled, next_store) =
                store.stage_upsert_fulfillment_order(draft_store, canceled)
              let #(replacement, next_store) =
                store.stage_upsert_fulfillment_order(next_store, replacement)
              #(
                shipping_types.MutationFieldResult(
                  key: key,
                  payload: fulfillment_order_payload_json(field, fragments, [
                    #("__typename", SrcString("FulfillmentOrderCancelPayload")),
                    #("fulfillmentOrder", fulfillment_order_source(canceled)),
                    #(
                      "replacementFulfillmentOrder",
                      fulfillment_order_source(replacement),
                    ),
                    #("userErrors", SrcList([])),
                  ]),
                  errors: [],
                  staged_resource_ids: [canceled.id, replacement.id],
                ),
                next_store,
                identity,
              )
            }
          }
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderCancelPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderCancelPayload",
      )
  }
}

@internal
pub fn fulfillment_order_cancel_block_message(
  order: FulfillmentOrderRecord,
) -> Option(String) {
  case fulfillment_order_has_manually_reported_progress(order) {
    True ->
      Some(
        "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.",
      )
    False ->
      case fulfillment_order_cancel_allowed(order) {
        True -> None
        False ->
          Some(
            "Fulfillment order is not in cancelable request state and can't be canceled.",
          )
      }
  }
}

@internal
pub fn fulfillment_order_cancel_allowed(order: FulfillmentOrderRecord) -> Bool {
  list.contains(["SUBMITTED", "CANCELLATION_REQUESTED"], order.request_status)
  || list.contains(["OPEN", "IN_PROGRESS"], order.status)
}

@internal
pub fn handle_fulfillment_order_split(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let splits = read_fulfillment_order_split_inputs(args)
  let validation_errors =
    validate_fulfillment_order_split_inputs(draft_store, splits)
  case validation_errors {
    [_, ..] ->
      fulfillment_order_split_user_error_result(
        draft_store,
        identity,
        field,
        fragments,
        validation_errors,
      )
    [] -> {
      let #(split_sources, next_store, next_identity, staged_ids) =
        apply_fulfillment_order_splits(draft_store, identity, splits, [], [])
      #(
        shipping_types.MutationFieldResult(
          key: key,
          payload: fulfillment_order_payload_json(field, fragments, [
            #("__typename", SrcString("FulfillmentOrderSplitPayload")),
            #("fulfillmentOrderSplits", SrcList(split_sources)),
            #("userErrors", SrcList([])),
          ]),
          errors: [],
          staged_resource_ids: staged_ids,
        ),
        next_store,
        next_identity,
      )
    }
  }
}

@internal
pub fn read_fulfillment_order_split_inputs(
  args: Dict(String, root_field.ResolvedValue),
) -> List(shipping_types.FulfillmentOrderSplitInput) {
  read_object_array(args, "fulfillmentOrderSplits")
  |> indexed_fulfillment_order_split_inputs(0)
}

@internal
pub fn indexed_fulfillment_order_split_inputs(
  splits: List(Dict(String, root_field.ResolvedValue)),
  index: Int,
) -> List(shipping_types.FulfillmentOrderSplitInput) {
  case splits {
    [] -> []
    [split, ..rest] -> {
      let input =
        shipping_types.FulfillmentOrderSplitInput(
          index: index,
          fulfillment_order_id: read_string(split, "fulfillmentOrderId"),
          line_items: read_object_array(split, "fulfillmentOrderLineItems")
            |> indexed_fulfillment_order_split_line_items(0),
        )
      [input, ..indexed_fulfillment_order_split_inputs(rest, index + 1)]
    }
  }
}

@internal
pub fn indexed_fulfillment_order_split_line_items(
  line_items: List(Dict(String, root_field.ResolvedValue)),
  index: Int,
) -> List(shipping_types.FulfillmentOrderSplitLineItemInput) {
  case line_items {
    [] -> []
    [line_item, ..rest] -> {
      let #(quantity, quantity_is_int) =
        read_fulfillment_order_split_line_item_quantity(line_item)
      let input =
        shipping_types.FulfillmentOrderSplitLineItemInput(
          index: index,
          id: read_string(line_item, "id"),
          quantity: quantity,
          quantity_is_int: quantity_is_int,
        )
      [input, ..indexed_fulfillment_order_split_line_items(rest, index + 1)]
    }
  }
}

@internal
pub fn read_fulfillment_order_split_line_item_quantity(
  line_item: Dict(String, root_field.ResolvedValue),
) -> #(Option(Int), Bool) {
  case dict.get(line_item, "quantity") {
    Ok(root_field.IntVal(quantity)) -> #(Some(quantity), True)
    Ok(_) -> #(None, False)
    Error(_) -> #(None, False)
  }
}

@internal
pub fn validate_fulfillment_order_split_inputs(
  draft_store: Store,
  splits: List(shipping_types.FulfillmentOrderSplitInput),
) -> List(shipping_types.FulfillmentOrderSplitUserError) {
  splits
  |> list.flat_map(fn(split) {
    let shipping_types.FulfillmentOrderSplitInput(
      index:,
      fulfillment_order_id:,
      line_items:,
    ) = split
    let line_item_errors =
      validate_fulfillment_order_split_line_items(index, line_items)
    case line_item_errors {
      [_, ..] -> line_item_errors
      [] ->
        case fulfillment_order_id {
          Some(id) ->
            case store.get_effective_fulfillment_order_by_id(draft_store, id) {
              Some(order) ->
                validate_fulfillment_order_split_line_item_ids(
                  index,
                  line_items,
                  order,
                )
              None -> [fulfillment_order_split_missing_order_error(id)]
            }
          None -> [fulfillment_order_split_missing_order_error("")]
        }
    }
  })
}

@internal
pub fn validate_fulfillment_order_split_line_items(
  split_index: Int,
  line_items: List(shipping_types.FulfillmentOrderSplitLineItemInput),
) -> List(shipping_types.FulfillmentOrderSplitUserError) {
  case line_items {
    [] -> [
      shipping_types.FulfillmentOrderSplitUserError(
        field: SrcList([
          SrcString("fulfillmentOrderSplits"),
          SrcString(int.to_string(split_index)),
          SrcString("fulfillmentOrderLineItems"),
        ]),
        message: "There must be at least one item selected in this fulfillment to split it.",
        code: "NO_LINE_ITEMS_PROVIDED_TO_SPLIT",
      ),
    ]
    [_, ..] ->
      line_items
      |> list.filter_map(fn(line_item) {
        let shipping_types.FulfillmentOrderSplitLineItemInput(
          index:,
          quantity:,
          quantity_is_int:,
          ..,
        ) = line_item
        let field =
          SrcList([
            SrcString("fulfillmentOrderSplits"),
            SrcString(int.to_string(split_index)),
            SrcString("fulfillmentOrderLineItems"),
            SrcString(int.to_string(index)),
            SrcString("quantity"),
          ])
        case quantity_is_int, quantity {
          False, _ ->
            Ok(shipping_types.FulfillmentOrderSplitUserError(
              field: field,
              message: "Line item quantity is invalid.",
              code: "INVALID_LINE_ITEM_QUANTITY",
            ))
          True, Some(quantity) if quantity <= 0 ->
            Ok(shipping_types.FulfillmentOrderSplitUserError(
              field: field,
              message: "You must select at least one item to split into a new fulfillment order.",
              code: "GREATER_THAN",
            ))
          _, _ -> Error(Nil)
        }
      })
  }
}

@internal
pub fn validate_fulfillment_order_split_line_item_ids(
  split_index: Int,
  line_items: List(shipping_types.FulfillmentOrderSplitLineItemInput),
  order: FulfillmentOrderRecord,
) -> List(shipping_types.FulfillmentOrderSplitUserError) {
  line_items
  |> list.filter_map(fn(line_item) {
    let shipping_types.FulfillmentOrderSplitLineItemInput(index:, id:, ..) =
      line_item
    case id {
      Some(id) ->
        case fulfillment_order_has_line_item_id(order.data, id) {
          True -> Error(Nil)
          False ->
            Ok(shipping_types.FulfillmentOrderSplitUserError(
              field: SrcList([
                SrcString("fulfillmentOrderSplits"),
                SrcString(int.to_string(split_index)),
                SrcString("fulfillmentOrderLineItems"),
                SrcString(int.to_string(index)),
                SrcString("id"),
              ]),
              message: "Line item quantity is invalid.",
              code: "INVALID_LINE_ITEM_QUANTITY",
            ))
        }
      None ->
        Ok(shipping_types.FulfillmentOrderSplitUserError(
          field: SrcList([
            SrcString("fulfillmentOrderSplits"),
            SrcString(int.to_string(split_index)),
            SrcString("fulfillmentOrderLineItems"),
            SrcString(int.to_string(index)),
            SrcString("id"),
          ]),
          message: "Line item quantity is invalid.",
          code: "INVALID_LINE_ITEM_QUANTITY",
        ))
    }
  })
}

@internal
pub fn fulfillment_order_has_line_item_id(
  data: CapturedJsonValue,
  id: String,
) -> Bool {
  captured_array_field(data, "lineItems", "nodes")
  |> list.any(fn(node) { captured_string_field(node, "id") == Some(id) })
}

@internal
pub fn fulfillment_order_split_missing_order_error(
  _id: String,
) -> shipping_types.FulfillmentOrderSplitUserError {
  shipping_types.FulfillmentOrderSplitUserError(
    field: SrcNull,
    message: "Fulfillment order does not exist.",
    code: "FULFILLMENT_ORDER_NOT_FOUND",
  )
}

@internal
pub fn fulfillment_order_split_user_error_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(shipping_types.FulfillmentOrderSplitUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("FulfillmentOrderSplitPayload")),
        #("fulfillmentOrderSplits", SrcNull),
        #(
          "userErrors",
          SrcList(list.map(user_errors, fulfillment_order_split_error_source)),
        ),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn fulfillment_order_split_error_source(
  error: shipping_types.FulfillmentOrderSplitUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("shipping_types.FulfillmentOrderSplitUserError")),
    #("field", error.field),
    #("message", SrcString(error.message)),
    #("code", SrcString(error.code)),
  ])
}

@internal
pub fn apply_fulfillment_order_splits(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  splits: List(shipping_types.FulfillmentOrderSplitInput),
  split_sources: List(SourceValue),
  staged_ids: List(String),
) -> #(List(SourceValue), Store, SyntheticIdentityRegistry, List(String)) {
  case splits {
    [] -> #(split_sources, draft_store, identity, staged_ids)
    [split, ..rest] -> {
      let shipping_types.FulfillmentOrderSplitInput(
        fulfillment_order_id:,
        line_items:,
        ..,
      ) = split
      let assert Some(id) = fulfillment_order_id
      let assert Some(order) =
        store.get_effective_fulfillment_order_by_id(draft_store, id)
      let requested_line_items =
        line_items
        |> list.filter_map(fn(item) {
          let shipping_types.FulfillmentOrderSplitLineItemInput(
            id:,
            quantity:,
            ..,
          ) = item
          case id, quantity {
            Some(id), Some(quantity) -> Ok(#(id, quantity))
            _, _ -> Error(Nil)
          }
        })
      let original_line_items =
        fulfillment_order_line_items_after_split(
          order.data,
          requested_line_items,
        )
      let original =
        update_fulfillment_order_fields(order, [
          #("updatedAt", CapturedString(synthetic_timestamp_string())),
          #(
            "supportedActions",
            fulfillment_order_split_supported_actions(
              fulfillment_order_line_items_total(original_line_items),
            ),
          ),
          #("lineItems", original_line_items),
        ])
      let #(remaining_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "FulfillmentOrder")
      let remaining_line_items =
        fulfillment_order_line_items_for_split(order.data, requested_line_items)
      let remaining =
        update_fulfillment_order_fields(order, [
          #("id", CapturedString(remaining_id)),
          #("updatedAt", CapturedString(synthetic_timestamp_string())),
          #(
            "supportedActions",
            fulfillment_order_split_supported_actions(
              fulfillment_order_line_items_total(remaining_line_items),
            ),
          ),
          #("lineItems", remaining_line_items),
        ])
      let remaining = FulfillmentOrderRecord(..remaining, id: remaining_id)
      let #(original, next_store) =
        store.stage_upsert_fulfillment_order(draft_store, original)
      let #(remaining, next_store) =
        store.stage_upsert_fulfillment_order(next_store, remaining)
      let split_source =
        src_object([
          #("fulfillmentOrder", fulfillment_order_source(original)),
          #("remainingFulfillmentOrder", fulfillment_order_source(remaining)),
          #("replacementFulfillmentOrder", SrcNull),
        ])
      apply_fulfillment_order_splits(
        next_store,
        next_identity,
        rest,
        list.append(split_sources, [split_source]),
        list.append(staged_ids, [original.id, remaining.id]),
      )
    }
  }
}

@internal
pub fn handle_fulfillment_orders_set_deadline(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let deadline = read_string(args, "fulfillmentDeadline")
  let ids = read_string_array(args, "fulfillmentOrderIds")
  case fulfillment_deadline_validation_error(draft_store, ids) {
    Some(user_error) -> #(
      shipping_types.MutationFieldResult(
        key: key,
        payload: fulfillment_order_deadline_payload(field, fragments, False, [
          user_error,
        ]),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
    None -> {
      let next_store =
        ids
        |> list.fold(draft_store, fn(current_store, id) {
          case store.get_effective_fulfillment_order_by_id(current_store, id) {
            Some(order) -> {
              let updated =
                update_fulfillment_order_fields(order, [
                  #(
                    "fulfillBy",
                    option_to_captured_string(option.map(
                      deadline,
                      normalize_shopify_timestamp_to_seconds,
                    )),
                  ),
                ])
              let #(_, staged_store) =
                store.stage_upsert_fulfillment_order(current_store, updated)
              staged_store
            }
            None -> current_store
          }
        })
      #(
        shipping_types.MutationFieldResult(
          key: key,
          payload: fulfillment_order_deadline_payload(
            field,
            fragments,
            True,
            [],
          ),
          errors: [],
          staged_resource_ids: ids,
        ),
        next_store,
        identity,
      )
    }
  }
}

fn fulfillment_order_deadline_payload(
  field: Selection,
  fragments: FragmentMap,
  success: Bool,
  user_errors: List(SourceValue),
) -> Json {
  fulfillment_order_payload_json(field, fragments, [
    #("__typename", SrcString("FulfillmentOrdersSetFulfillmentDeadlinePayload")),
    #("success", SrcBool(success)),
    #("userErrors", SrcList(user_errors)),
  ])
}

fn fulfillment_deadline_validation_error(
  draft_store: Store,
  ids: List(String),
) -> Option(SourceValue) {
  case ids {
    [] -> None
    [id, ..rest] ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        None ->
          Some(fulfillment_deadline_user_error(
            "The fulfillment orders could not be found.",
            SrcString("FULFILLMENT_ORDERS_NOT_FOUND"),
          ))
        Some(order) ->
          case order.status {
            "CLOSED" | "CANCELLED" | "CANCELED" ->
              Some(fulfillment_deadline_user_error(
                "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.",
                SrcNull,
              ))
            _ -> fulfillment_deadline_validation_error(draft_store, rest)
          }
      }
  }
}

fn fulfillment_deadline_user_error(
  message: String,
  code: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList([SrcString("base")])),
    #("message", SrcString(message)),
    #("code", code),
  ])
}

@internal
pub fn handle_fulfillment_order_merge(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let ids = fulfillment_order_merge_ids(args)
  case validate_fulfillment_order_merge_inputs(draft_store, args, ids) {
    [_, ..] as user_errors ->
      fulfillment_order_merge_user_error_result(
        draft_store,
        identity,
        field,
        fragments,
        user_errors,
      )
    [] ->
      case ids {
        [primary_id, ..] ->
          case
            store.get_effective_fulfillment_order_by_id(draft_store, primary_id)
          {
            Some(primary) -> {
              let total =
                ids
                |> list.fold(0, fn(sum, id) {
                  sum
                  + case
                    store.get_effective_fulfillment_order_by_id(draft_store, id)
                  {
                    Some(order) ->
                      first_fulfillment_order_line_item_total(order.data)
                    None -> 0
                  }
                })
              let merged =
                update_fulfillment_order_fields(primary, [
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                  #(
                    "supportedActions",
                    captured_action_list([
                      "CREATE_FULFILLMENT",
                      "REPORT_PROGRESS",
                      "MOVE",
                      "HOLD",
                      "SPLIT",
                    ]),
                  ),
                  #(
                    "lineItems",
                    fulfillment_order_line_items_with_quantity(
                      primary.data,
                      total,
                      False,
                    ),
                  ),
                ])
              let #(merged, next_store) =
                store.stage_upsert_fulfillment_order(draft_store, merged)
              let next_store = close_merge_siblings(next_store, ids, primary_id)
              #(
                shipping_types.MutationFieldResult(
                  key: key,
                  payload: fulfillment_order_payload_json(field, fragments, [
                    #("__typename", SrcString("FulfillmentOrderMergePayload")),
                    #(
                      "fulfillmentOrderMerges",
                      SrcList([
                        src_object([
                          #(
                            "fulfillmentOrder",
                            fulfillment_order_source(merged),
                          ),
                        ]),
                      ]),
                    ),
                    #("userErrors", SrcList([])),
                  ]),
                  errors: [],
                  staged_resource_ids: [merged.id],
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
                "FulfillmentOrderMergePayload",
              )
          }
        _ ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderMergePayload",
          )
      }
  }
}

fn validate_fulfillment_order_merge_inputs(
  draft_store: Store,
  args: Dict(String, root_field.ResolvedValue),
  ids: List(String),
) -> List(SourceValue) {
  case fulfillment_order_merge_missing_order_errors(draft_store, ids) {
    [_, ..] as errors -> errors
    [] ->
      case fulfillment_order_merge_line_item_quantity_errors(args) {
        [_, ..] as errors -> errors
        [] ->
          case fulfillment_order_merge_line_item_id_errors(draft_store, args) {
            [_, ..] as errors -> errors
            [] ->
              case
                fulfillment_order_merge_line_item_excess_errors(
                  draft_store,
                  args,
                )
              {
                [_, ..] as errors -> errors
                [] -> fulfillment_order_merge_status_errors(draft_store, ids)
              }
          }
      }
  }
}

fn fulfillment_order_merge_missing_order_errors(
  draft_store: Store,
  ids: List(String),
) -> List(SourceValue) {
  case
    ids
    |> list.any(fn(id) {
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(_) -> False
        None -> True
      }
    })
  {
    True -> [
      fulfillment_order_merge_user_error(
        SrcNull,
        "Fulfillment order does not exist.",
        SrcString("FULFILLMENT_ORDER_NOT_FOUND"),
      ),
    ]
    False -> []
  }
}

fn fulfillment_order_merge_line_item_quantity_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  read_object_array(args, "fulfillmentOrderMergeInputs")
  |> list.index_fold([], fn(errors, input, input_index) {
    let intent_errors =
      read_object_array(input, "mergeIntents")
      |> list.index_fold([], fn(intent_errors, intent, intent_index) {
        let line_item_errors =
          read_object_array(intent, "fulfillmentOrderLineItems")
          |> list.index_fold(
            [],
            fn(line_item_errors, line_item, line_item_index) {
              case
                fulfillment_order_merge_line_item_quantity_error(
                  line_item,
                  input_index,
                  intent_index,
                  line_item_index,
                )
              {
                Some(error) -> list.append(line_item_errors, [error])
                None -> line_item_errors
              }
            },
          )
        list.append(intent_errors, line_item_errors)
      })
    list.append(errors, intent_errors)
  })
}

fn fulfillment_order_merge_line_item_quantity_error(
  line_item: Dict(String, root_field.ResolvedValue),
  input_index: Int,
  intent_index: Int,
  line_item_index: Int,
) -> Option(SourceValue) {
  let field =
    SrcList([
      SrcString("fulfillmentOrderMergeInputs"),
      SrcString(int.to_string(input_index)),
      SrcString("mergeIntents"),
      SrcString(int.to_string(intent_index)),
      SrcString("fulfillmentOrderLineItems"),
      SrcString(int.to_string(line_item_index)),
      SrcString("quantity"),
    ])
  case dict.get(line_item, "quantity") {
    Ok(root_field.IntVal(quantity)) if quantity <= 0 ->
      Some(fulfillment_order_merge_user_error(
        field,
        "You must select at least one item to merge into a new fulfillment order.",
        SrcString("GREATER_THAN"),
      ))
    Ok(root_field.IntVal(_)) -> None
    Ok(_) | Error(_) ->
      Some(fulfillment_order_merge_user_error(
        field,
        "Line item quantity is invalid.",
        SrcString("INVALID_LINE_ITEM_QUANTITY"),
      ))
  }
}

fn fulfillment_order_merge_line_item_id_errors(
  draft_store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  read_object_array(args, "fulfillmentOrderMergeInputs")
  |> list.index_fold([], fn(errors, input, _input_index) {
    let intent_errors =
      read_object_array(input, "mergeIntents")
      |> list.index_fold([], fn(intent_errors, intent, _intent_index) {
        let order_id = case dict.get(intent, "fulfillmentOrderId") {
          Ok(root_field.StringVal(id)) -> Some(id)
          _ -> None
        }
        let line_item_errors =
          read_object_array(intent, "fulfillmentOrderLineItems")
          |> list.filter_map(fn(line_item) {
            case order_id {
              Some(id) ->
                case
                  store.get_effective_fulfillment_order_by_id(draft_store, id)
                {
                  Some(order) ->
                    case dict.get(line_item, "id") {
                      Ok(root_field.StringVal(line_item_id)) ->
                        case
                          fulfillment_order_has_line_item_id(
                            order.data,
                            line_item_id,
                          )
                        {
                          True -> Error(Nil)
                          False ->
                            Ok(fulfillment_order_merge_user_error(
                              SrcNull,
                              "Fulfillment order line item does not exist.",
                              SrcNull,
                            ))
                        }
                      _ ->
                        Ok(fulfillment_order_merge_user_error(
                          SrcNull,
                          "Fulfillment order line item does not exist.",
                          SrcNull,
                        ))
                    }
                  None -> Error(Nil)
                }
              None -> Error(Nil)
            }
          })
        list.append(intent_errors, line_item_errors)
      })
    list.append(errors, intent_errors)
  })
}

fn fulfillment_order_merge_line_item_excess_errors(
  draft_store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  read_object_array(args, "fulfillmentOrderMergeInputs")
  |> list.index_fold([], fn(errors, input, _input_index) {
    let intent_errors =
      read_object_array(input, "mergeIntents")
      |> list.index_fold([], fn(intent_errors, intent, _intent_index) {
        let order_id = case dict.get(intent, "fulfillmentOrderId") {
          Ok(root_field.StringVal(id)) -> Some(id)
          _ -> None
        }
        let line_item_errors =
          read_object_array(intent, "fulfillmentOrderLineItems")
          |> list.filter_map(fn(line_item) {
            case
              order_id,
              dict.get(line_item, "id"),
              dict.get(line_item, "quantity")
            {
              Some(id),
                Ok(root_field.StringVal(line_item_id)),
                Ok(root_field.IntVal(quantity))
              ->
                case
                  store.get_effective_fulfillment_order_by_id(draft_store, id)
                {
                  Some(order) ->
                    case
                      fulfillment_order_line_item_total(
                        order.data,
                        line_item_id,
                      )
                    {
                      Some(total) if quantity > total ->
                        Ok(fulfillment_order_merge_user_error(
                          SrcNull,
                          "Invalid fulfillment order line item quantity requested.",
                          SrcNull,
                        ))
                      _ -> Error(Nil)
                    }
                  None -> Error(Nil)
                }
              _, _, _ -> Error(Nil)
            }
          })
        list.append(intent_errors, line_item_errors)
      })
    list.append(errors, intent_errors)
  })
}

fn fulfillment_order_line_item_total(
  data: CapturedJsonValue,
  id: String,
) -> Option(Int) {
  data
  |> captured_array_field("lineItems", "nodes")
  |> list.find_map(fn(node) {
    case captured_string_field(node, "id") == Some(id) {
      True -> Ok(captured_int_field(node, "totalQuantity", ""))
      False -> Error(Nil)
    }
  })
  |> result.unwrap(None)
}

fn fulfillment_order_merge_status_errors(
  draft_store: Store,
  ids: List(String),
) -> List(SourceValue) {
  let non_open_id =
    ids
    |> list.find(fn(id) {
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> order.status != "OPEN"
        None -> False
      }
    })
  case non_open_id {
    Ok(id) -> [
      fulfillment_order_merge_user_error(
        SrcNull,
        "Fulfillment order: "
          <> fulfillment_order_numeric_id(id)
          <> " is currently not in a mergeable state.",
        SrcNull,
      ),
    ]
    Error(_) -> []
  }
}

fn fulfillment_order_numeric_id(id: String) -> String {
  id
  |> string.split("/")
  |> list.last
  |> result.unwrap(id)
}

fn fulfillment_order_merge_user_error(
  field: SourceValue,
  message: String,
  code: SourceValue,
) -> SourceValue {
  src_object([
    #("field", field),
    #("message", SrcString(message)),
    #("code", code),
  ])
}

fn fulfillment_order_merge_user_error_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(SourceValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("FulfillmentOrderMergePayload")),
        #("fulfillmentOrderMerges", SrcNull),
        #("userErrors", SrcList(user_errors)),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

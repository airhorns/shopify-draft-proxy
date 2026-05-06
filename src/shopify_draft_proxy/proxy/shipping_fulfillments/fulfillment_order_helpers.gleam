//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  get_field_response_key, project_graphql_value, src_object,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  captured_array_field, captured_bool_field, captured_connection, captured_field,
  captured_int_field, captured_string_field, captured_upsert_fields,
  option_to_captured_string, read_bool, read_number, read_object_array,
  read_string, update_fulfillment_order_fields,
  zero_fulfillment_order_line_items,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers.{
  fulfillment_order_payload_json,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  calculated_order_source, captured_json_source, fulfillment_event_source,
  fulfillment_order_source, reverse_delivery_source,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CalculatedOrderRecord, type CapturedJsonValue,
  type FulfillmentOrderRecord, type FulfillmentRecord,
  type ReverseDeliveryRecord, CapturedArray, CapturedBool, CapturedInt,
  CapturedNull, CapturedObject, CapturedString, FulfillmentOrderRecord,
  FulfillmentRecord, ShippingOrderRecord,
}

@internal
pub fn fulfillment_order_has_manually_reported_progress(
  order: FulfillmentOrderRecord,
) -> Bool {
  captured_bool_field(order.data, "__draftProxyManuallyReportedProgress")
  |> option.unwrap(False)
}

@internal
pub fn fulfillment_order_single_payload_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  fulfillment_order: FulfillmentOrderRecord,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let #(staged, next_store) =
    store.stage_upsert_fulfillment_order(draft_store, fulfillment_order)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString(payload_typename)),
        #("fulfillmentOrder", fulfillment_order_source(staged)),
        #("userErrors", SrcList([])),
      ]),
      errors: [],
      staged_resource_ids: [staged.id],
    ),
    next_store,
    identity,
  )
}

@internal
pub fn fulfillment_event_payload_json(
  field: Selection,
  fragments: FragmentMap,
  event: Option(CapturedJsonValue),
  user_errors: List(SourceValue),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("FulfillmentEventCreatePayload")),
      #("fulfillmentEvent", case event {
        Some(event) -> fulfillment_event_source(event)
        None -> SrcNull
      }),
      #("userErrors", SrcList(user_errors)),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

@internal
pub fn fulfillment_event_missing_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_event_payload_json(field, fragments, None, [
        fulfillment_event_user_error_source(
          shipping_types.FulfillmentEventUserError(
            field: ["fulfillmentEvent", "fulfillmentId"],
            message: "Fulfillment does not exist.",
            code: "fulfillment_not_found",
          ),
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
pub fn fulfillment_event_user_error_source(
  user_error: shipping_types.FulfillmentEventUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("FulfillmentEventCreateUserError")),
    #("field", SrcList(list.map(user_error.field, SrcString))),
    #("message", SrcString(user_error.message)),
    #("code", SrcString(user_error.code)),
  ])
}

@internal
pub fn reverse_delivery_payload_json(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  reverse_delivery: Option(ReverseDeliveryRecord),
  user_errors: List(SourceValue),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #("reverseDelivery", case reverse_delivery {
        Some(record) -> reverse_delivery_source(store, record)
        None -> SrcNull
      }),
      #("userErrors", SrcList(user_errors)),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

@internal
pub fn reverse_delivery_missing_rfo_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: reverse_delivery_payload_json(
        draft_store,
        field,
        fragments,
        "ReverseDeliveryCreateWithShippingPayload",
        None,
        [
          plain_user_error_source(
            ["reverseFulfillmentOrderId"],
            "Reverse fulfillment order does not exist.",
          ),
        ],
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn reverse_delivery_missing_delivery_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: reverse_delivery_payload_json(
        draft_store,
        field,
        fragments,
        "ReverseDeliveryShippingUpdatePayload",
        None,
        [
          plain_user_error_source(
            ["reverseDeliveryId"],
            "Reverse delivery does not exist.",
          ),
        ],
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn order_edit_shipping_line_payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  calculated_order: Option(CalculatedOrderRecord),
  calculated_shipping_line: Option(CapturedJsonValue),
  user_errors: List(SourceValue),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #("calculatedOrder", case calculated_order {
        Some(record) -> calculated_order_source(record)
        None -> SrcNull
      }),
      #("calculatedShippingLine", case calculated_shipping_line {
        Some(line) -> captured_json_source(line)
        None -> SrcNull
      }),
      #("userErrors", SrcList(user_errors)),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

@internal
pub fn order_edit_calculated_order_missing_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  payload_typename: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: order_edit_shipping_line_payload_json(
        field,
        fragments,
        payload_typename,
        None,
        None,
        [plain_user_error_source(["id"], "Calculated order does not exist.")],
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn order_edit_shipping_line_invalid_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  payload_typename: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: order_edit_shipping_line_payload_json(
        field,
        fragments,
        payload_typename,
        None,
        None,
        [plain_user_error_source(["shippingLine"], "Shipping line is invalid")],
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn plain_user_error_source(
  field: List(String),
  message: String,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
  ])
}

@internal
pub fn fulfillment_order_missing_mutation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString(payload_typename)),
        #("fulfillmentOrder", SrcNull),
        #("originalFulfillmentOrder", SrcNull),
        #("submittedFulfillmentOrder", SrcNull),
        #("unsubmittedFulfillmentOrder", SrcNull),
        #("userErrors", SrcList([])),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn fulfillment_order_user_error_payload(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  message: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let user_error =
    src_object([
      #("field", SrcNull),
      #("message", SrcString(message)),
    ])
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString(payload_typename)),
        #("fulfillmentOrder", SrcNull),
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
pub fn fulfillment_order_cancel_user_error_payload(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  message: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let field_value = fulfillment_order_cancel_user_error_field(message)
  let user_error =
    src_object([
      #("field", field_value),
      #("message", SrcString(message)),
      #("code", fulfillment_order_cancel_user_error_code(message)),
    ])
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("FulfillmentOrderCancelPayload")),
        #("fulfillmentOrder", SrcNull),
        #("replacementFulfillmentOrder", SrcNull),
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
pub fn fulfillment_order_cancel_user_error_field(
  message: String,
) -> SourceValue {
  case message {
    "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first." ->
      SrcList([SrcString("id")])
    _ -> SrcNull
  }
}

@internal
pub fn fulfillment_order_cancel_user_error_code(
  message: String,
) -> SourceValue {
  case message {
    "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first." ->
      SrcString("fulfillment_order_has_manually_reported_progress")
    "Fulfillment order is not in cancelable request state and can't be canceled." ->
      SrcString("fulfillment_order_cannot_be_cancelled")
    _ -> SrcNull
  }
}

@internal
pub fn optional_fulfillment_order_source(
  fulfillment_order: Option(FulfillmentOrderRecord),
) -> SourceValue {
  case fulfillment_order {
    Some(record) -> fulfillment_order_source(record)
    None -> SrcNull
  }
}

@internal
pub fn fulfillment_event_value(
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let created_at = synthetic_timestamp_string()
  let happened_at =
    read_string(input, "happenedAt") |> option.unwrap(created_at)
  CapturedObject([
    #("id", CapturedString(id)),
    #("status", option_to_captured_string(read_string(input, "status"))),
    #("message", option_to_captured_string(read_string(input, "message"))),
    #("happenedAt", CapturedString(happened_at)),
    #("createdAt", CapturedString(created_at)),
    #(
      "estimatedDeliveryAt",
      option_to_captured_string(read_string(input, "estimatedDeliveryAt")),
    ),
    #("city", option_to_captured_string(read_string(input, "city"))),
    #("province", option_to_captured_string(read_string(input, "province"))),
    #("country", option_to_captured_string(read_string(input, "country"))),
    #("zip", option_to_captured_string(read_string(input, "zip"))),
    #("address1", option_to_captured_string(read_string(input, "address1"))),
    #("latitude", read_number(input, "latitude") |> option.unwrap(CapturedNull)),
    #(
      "longitude",
      read_number(input, "longitude") |> option.unwrap(CapturedNull),
    ),
  ])
}

@internal
pub fn update_fulfillment_for_event(
  fulfillment: FulfillmentRecord,
  event: CapturedJsonValue,
) -> FulfillmentRecord {
  let events =
    list.append(captured_array_field(fulfillment.data, "events", "nodes"), [
      event,
    ])
  let updates = [
    #("events", captured_event_connection(events)),
    #(
      "displayStatus",
      option_to_captured_string(captured_string_field(event, "status")),
    ),
    #(
      "estimatedDeliveryAt",
      captured_field(event, "estimatedDeliveryAt")
        |> option.unwrap(CapturedNull),
    ),
  ]
  let updates = case
    captured_string_field(event, "status"),
    captured_string_field(event, "happenedAt")
  {
    Some("IN_TRANSIT"), Some(happened_at) ->
      list.append(updates, [#("inTransitAt", CapturedString(happened_at))])
    Some("DELIVERED"), Some(happened_at) ->
      list.append(updates, [#("deliveredAt", CapturedString(happened_at))])
    _, _ -> updates
  }
  FulfillmentRecord(
    ..fulfillment,
    data: captured_upsert_fields(fulfillment.data, updates),
  )
}

@internal
pub fn maybe_append_fulfillment_event_timeline(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  fulfillment: FulfillmentRecord,
  event: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
  staged_resource_ids: List(String),
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  case record_timeline_event_requested(input), fulfillment.order_id {
    True, Some(order_id) ->
      case store.get_effective_shipping_order_by_id(draft_store, order_id) {
        Some(order) -> {
          let #(timeline_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "BasicEvent")
          let timeline_event =
            fulfillment_event_timeline_event(timeline_id, fulfillment.id, event)
          let updated_order =
            ShippingOrderRecord(
              ..order,
              data: append_order_timeline_event(order.data, timeline_event),
            )
          let #(_, next_store) =
            store.stage_upsert_shipping_order(draft_store, updated_order)
          #(
            next_store,
            identity,
            list.append(staged_resource_ids, [order.id, timeline_id]),
          )
        }
        None -> #(draft_store, identity, staged_resource_ids)
      }
    _, _ -> #(draft_store, identity, staged_resource_ids)
  }
}

@internal
pub fn record_timeline_event_requested(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  read_bool(input, "record_timeline_event")
  |> option.or(read_bool(input, "recordTimelineEvent"))
  |> option.unwrap(False)
}

@internal
pub fn fulfillment_event_timeline_event(
  id: String,
  fulfillment_id: String,
  event: CapturedJsonValue,
) -> CapturedJsonValue {
  let message =
    captured_string_field(event, "message")
    |> option.unwrap(fulfillment_event_timeline_message(event))
  CapturedObject([
    #("__typename", CapturedString("BasicEvent")),
    #("id", CapturedString(id)),
    #("action", CapturedString("fulfillment_event_create")),
    #("message", CapturedString(message)),
    #(
      "createdAt",
      captured_field(event, "createdAt") |> option.unwrap(CapturedNull),
    ),
    #("subjectId", CapturedString(fulfillment_id)),
    #("subjectType", CapturedString("FULFILLMENT")),
  ])
}

@internal
pub fn fulfillment_event_timeline_message(event: CapturedJsonValue) -> String {
  case captured_string_field(event, "status") {
    Some(status) -> "Fulfillment event " <> status <> " was recorded."
    None -> "Fulfillment event was recorded."
  }
}

@internal
pub fn append_order_timeline_event(
  order: CapturedJsonValue,
  timeline_event: CapturedJsonValue,
) -> CapturedJsonValue {
  let events =
    list.append(captured_array_field(order, "events", "nodes"), [
      timeline_event,
    ])
  captured_upsert_fields(order, [#("events", captured_event_connection(events))])
}

@internal
pub fn captured_event_connection(
  nodes: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let first_cursor = case nodes {
    [first, ..] -> captured_event_cursor(first)
    _ -> CapturedNull
  }
  let last_cursor = case list.last(nodes) {
    Ok(last) -> captured_event_cursor(last)
    Error(_) -> CapturedNull
  }
  CapturedObject([
    #("nodes", CapturedArray(nodes)),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", first_cursor),
        #("endCursor", last_cursor),
      ]),
    ),
  ])
}

@internal
pub fn captured_event_cursor(event: CapturedJsonValue) -> CapturedJsonValue {
  case captured_string_field(event, "id") {
    Some(id) -> CapturedString("cursor:" <> id)
    None -> CapturedNull
  }
}

@internal
pub fn find_unsubmitted_sibling_fulfillment_order(
  draft_store: Store,
  original: FulfillmentOrderRecord,
) -> Option(FulfillmentOrderRecord) {
  case
    store.list_effective_fulfillment_orders(draft_store)
    |> list.find(fn(order) {
      order.id != original.id
      && order.order_id == original.order_id
      && order.request_status == "UNSUBMITTED"
    })
  {
    Ok(order) -> Some(order)
    Error(_) -> None
  }
}

@internal
pub fn fulfillment_order_merchant_request(
  kind: String,
  message: Option(String),
  request_options: CapturedJsonValue,
) -> CapturedJsonValue {
  CapturedObject([
    #("kind", CapturedString(kind)),
    #("message", option_to_captured_string(message)),
    #("requestOptions", request_options),
    #("responseData", CapturedNull),
  ])
}

@internal
pub fn fulfillment_hold_value(
  id: String,
  handle: Option(String),
  reason: Option(String),
  reason_notes: Option(String),
  external_id: Option(String),
  notify_merchant: Bool,
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("handle", option_to_captured_string(handle)),
    #("reason", option_to_captured_string(reason)),
    #("reasonNotes", option_to_captured_string(reason_notes)),
    #("externalId", option_to_captured_string(external_id)),
    #("__draftProxyNotifyMerchant", CapturedBool(notify_merchant)),
    #("displayReason", CapturedString("Other")),
    #("heldByApp", CapturedNull),
    #("heldByRequestingApp", CapturedBool(True)),
  ])
}

const max_fulfillment_holds_per_api_client = 10

const fulfillment_hold_handle_max_length = 64

@internal
pub fn fulfillment_order_hold_handle(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  read_string(input, "handle") |> option.unwrap("")
}

@internal
pub fn fulfillment_order_holds(
  data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_field(data, "fulfillmentHolds") {
    Some(CapturedArray(values)) -> values
    Some(value) -> captured_array_field(value, "nodes", "")
    None -> []
  }
}

@internal
pub fn fulfillment_order_hold_validation_errors(
  order: FulfillmentOrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(List(String), String, String)) {
  let line_item_inputs = read_object_array(input, "fulfillmentOrderLineItems")
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
          #(
            ["fulfillmentHold", "handle"],
            "Handle is too long (maximum is 64 characters)",
            "TOO_LONG",
          ),
        ]
        False -> {
          let existing_holds = fulfillment_order_holds(order.data)
          case !list.is_empty(line_item_inputs) && order.status == "ON_HOLD" {
            True -> [
              #(
                ["fulfillmentHold", "fulfillmentOrderLineItems"],
                "The fulfillment order is not in a splittable state.",
                "FULFILLMENT_ORDER_NOT_SPLITTABLE",
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
                  #(
                    ["fulfillmentHold", "handle"],
                    "The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order.",
                    "DUPLICATE_FULFILLMENT_HOLD_HANDLE",
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
                      #(
                        ["id"],
                        "The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time.",
                        "FULFILLMENT_ORDER_HOLD_LIMIT_REACHED",
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
) -> List(#(List(String), String, String)) {
  inputs
  |> list.index_fold([], fn(errors, input, index) {
    let invalid = case dict.get(input, "quantity") {
      Ok(root_field.IntVal(quantity)) if quantity <= 0 ->
        Some(#(
          "You must select at least one item to place on partial hold.",
          "GREATER_THAN_ZERO",
        ))
      Ok(root_field.IntVal(_)) -> None
      _ ->
        Some(#(
          "The line item quantity is invalid.",
          "INVALID_LINE_ITEM_QUANTITY",
        ))
    }
    case invalid {
      Some(error) -> {
        let #(message, code) = error
        list.append(errors, [
          #(
            [
              "fulfillmentHold",
              "fulfillmentOrderLineItems",
              int.to_string(index),
              "quantity",
            ],
            message,
            code,
          ),
        ])
      }
      None -> errors
    }
  })
}

@internal
pub fn fulfillment_order_hold_duplicate_line_item_errors(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(List(String), String, String)) {
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
      #(
        ["fulfillmentHold", "fulfillmentOrderLineItems"],
        "must contain unique line item ids",
        "DUPLICATED_FULFILLMENT_ORDER_LINE_ITEMS",
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
pub fn fulfillment_order_hold_supported_actions(
  holds: List(CapturedJsonValue),
) -> List(String) {
  case
    fulfillment_order_active_requesting_app_holds(holds)
    >= max_fulfillment_holds_per_api_client
  {
    True -> ["RELEASE_HOLD", "MOVE"]
    False -> ["RELEASE_HOLD", "HOLD", "MOVE"]
  }
}

@internal
pub fn fulfillment_hold_held_by_requesting_app(
  hold: CapturedJsonValue,
) -> Bool {
  captured_bool_field(hold, "heldByRequestingApp") |> option.unwrap(True)
}

@internal
pub fn fulfillment_order_hold_user_error_source(
  error: #(List(String), String, String),
) -> SourceValue {
  let #(field, message, code) = error
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

@internal
pub fn first_fulfillment_order_line_item_quantity(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> Int {
  case inputs {
    [first, ..] ->
      case dict.get(first, "quantity") {
        Ok(root_field.IntVal(value)) -> value
        _ -> 1
      }
    _ -> 1
  }
}

@internal
pub fn first_fulfillment_order_line_item_total(data: CapturedJsonValue) -> Int {
  case captured_array_field(data, "lineItems", "nodes") {
    [first, ..] ->
      captured_int_field(first, "totalQuantity", "") |> option.unwrap(1)
    _ -> 1
  }
}

@internal
pub fn fulfillment_order_line_items_with_quantity(
  data: CapturedJsonValue,
  quantity: Int,
  update_line_item_fulfillable: Bool,
) -> CapturedJsonValue {
  fulfillment_order_line_items_with_quantity_and_optional_fulfillable(
    data,
    quantity,
    case update_line_item_fulfillable {
      True -> Some(quantity)
      False -> None
    },
  )
}

@internal
pub fn fulfillment_order_line_items_with_quantity_and_fulfillable(
  data: CapturedJsonValue,
  quantity: Int,
  line_item_fulfillable_quantity: Int,
) -> CapturedJsonValue {
  fulfillment_order_line_items_with_quantity_and_optional_fulfillable(
    data,
    quantity,
    Some(line_item_fulfillable_quantity),
  )
}

@internal
pub fn fulfillment_order_line_items_with_quantity_and_optional_fulfillable(
  data: CapturedJsonValue,
  quantity: Int,
  line_item_fulfillable_quantity: Option(Int),
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.map(fn(node) {
      let line_item = case captured_field(node, "lineItem") {
        Some(value) -> {
          case line_item_fulfillable_quantity {
            Some(quantity) ->
              captured_upsert_fields(value, [
                #("fulfillableQuantity", CapturedInt(quantity)),
              ])
            None -> value
          }
        }
        None -> CapturedNull
      }
      captured_upsert_fields(node, [
        #("totalQuantity", CapturedInt(quantity)),
        #("remainingQuantity", CapturedInt(quantity)),
        #("lineItem", line_item),
      ])
    })
  captured_connection(nodes)
}

@internal
pub fn fulfillment_order_line_items_after_split(
  data: CapturedJsonValue,
  requested: List(#(String, Int)),
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.map(fn(node) {
      let existing_quantity =
        captured_int_field(node, "totalQuantity", "") |> option.unwrap(0)
      let requested_quantity =
        fulfillment_order_requested_line_item_quantity(
          captured_string_field(node, "id"),
          requested,
        )
      let next_quantity = max_int(existing_quantity - requested_quantity, 0)
      captured_upsert_fields(node, [
        #("totalQuantity", CapturedInt(next_quantity)),
        #("remainingQuantity", CapturedInt(next_quantity)),
      ])
    })
  captured_connection(nodes)
}

@internal
pub fn fulfillment_order_line_items_for_split(
  data: CapturedJsonValue,
  requested: List(#(String, Int)),
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.filter_map(fn(node) {
      let requested_quantity =
        fulfillment_order_requested_line_item_quantity(
          captured_string_field(node, "id"),
          requested,
        )
      case requested_quantity > 0 {
        True ->
          Ok(
            captured_upsert_fields(node, [
              #("totalQuantity", CapturedInt(requested_quantity)),
              #("remainingQuantity", CapturedInt(requested_quantity)),
            ]),
          )
        False -> Error(Nil)
      }
    })
  captured_connection(nodes)
}

@internal
pub fn fulfillment_order_requested_line_item_quantity(
  id: Option(String),
  requested: List(#(String, Int)),
) -> Int {
  case id {
    Some(id) ->
      requested
      |> list.fold(0, fn(total, item) {
        let #(requested_id, quantity) = item
        case requested_id == id {
          True -> total + quantity
          False -> total
        }
      })
    None -> 0
  }
}

@internal
pub fn fulfillment_order_line_items_total(
  line_items: CapturedJsonValue,
) -> Int {
  captured_array_field(line_items, "nodes", "")
  |> list.fold(0, fn(total, line_item) {
    total
    + { captured_int_field(line_item, "totalQuantity", "") |> option.unwrap(0) }
  })
}

@internal
pub fn fulfillment_order_split_supported_actions(
  total_quantity: Int,
) -> CapturedJsonValue {
  let actions = ["CREATE_FULFILLMENT", "REPORT_PROGRESS", "MOVE", "HOLD"]
  case total_quantity > 1 {
    True -> captured_action_list(list.append(actions, ["SPLIT", "MERGE"]))
    False -> captured_action_list(list.append(actions, ["MERGE"]))
  }
}

@internal
pub fn captured_action_list(actions: List(String)) -> CapturedJsonValue {
  actions
  |> list.map(fn(action) {
    CapturedObject([#("action", CapturedString(action))])
  })
  |> CapturedArray
}

@internal
pub fn sibling_fulfillment_order_quantity(
  draft_store: Store,
  order: FulfillmentOrderRecord,
) -> Int {
  store.list_effective_fulfillment_orders(draft_store)
  |> list.filter(fn(candidate) {
    candidate.id != order.id && candidate.order_id == order.order_id
  })
  |> list.fold(0, fn(sum, candidate) {
    sum + first_fulfillment_order_line_item_total(candidate.data)
  })
}

@internal
pub fn close_sibling_fulfillment_orders(
  draft_store: Store,
  order: FulfillmentOrderRecord,
) -> Store {
  store.list_effective_fulfillment_orders(draft_store)
  |> list.filter(fn(candidate) {
    candidate.id != order.id && candidate.order_id == order.order_id
  })
  |> list.fold(draft_store, fn(current_store, candidate) {
    let closed =
      update_fulfillment_order_fields(candidate, [
        #("status", CapturedString("CLOSED")),
        #("updatedAt", CapturedString(synthetic_timestamp_string())),
        #("supportedActions", CapturedArray([])),
        #(
          "lineItems",
          zero_fulfillment_order_line_items(
            candidate.data,
            Some(first_fulfillment_order_line_item_total(order.data)),
          ),
        ),
      ])
    let closed = FulfillmentOrderRecord(..closed, status: "CLOSED")
    let #(_, next_store) =
      store.stage_upsert_fulfillment_order(current_store, closed)
    next_store
  })
}

@internal
pub fn close_merge_siblings(
  draft_store: Store,
  ids: List(String),
  primary_id: String,
) -> Store {
  ids
  |> list.filter(fn(id) { id != primary_id })
  |> list.fold(draft_store, fn(current_store, id) {
    case store.get_effective_fulfillment_order_by_id(current_store, id) {
      Some(order) -> {
        let closed =
          update_fulfillment_order_fields(order, [
            #("status", CapturedString("CLOSED")),
            #("updatedAt", CapturedString(synthetic_timestamp_string())),
            #("supportedActions", CapturedArray([])),
            #("lineItems", zero_fulfillment_order_line_items(order.data, None)),
          ])
        let closed = FulfillmentOrderRecord(..closed, status: "CLOSED")
        let #(_, next_store) =
          store.stage_upsert_fulfillment_order(current_store, closed)
        next_store
      }
      None -> current_store
    }
  })
}

@internal
pub fn fulfillment_order_merge_ids(
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  read_object_array(args, "fulfillmentOrderMergeInputs")
  |> list.flat_map(fn(input) {
    read_object_array(input, "mergeIntents")
    |> list.filter_map(fn(intent) {
      read_string(intent, "fulfillmentOrderId") |> option.to_result(Nil)
    })
  })
}

@internal
pub fn fulfillment_order_split_ids(
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  read_object_array(args, "fulfillmentOrderSplits")
  |> list.filter_map(fn(input) {
    read_string(input, "fulfillmentOrderId") |> option.to_result(Nil)
  })
}

@internal
pub fn synthetic_timestamp_string() -> String {
  "2026-04-28T02:25:00Z"
}

@internal
pub fn normalize_shopify_timestamp_to_seconds(value: String) -> String {
  case string.split_once(value, ".") {
    Ok(#(prefix, suffix)) ->
      case string.ends_with(suffix, "Z") {
        True -> prefix <> "Z"
        False -> value
      }
    Error(_) -> value
  }
}

@internal
pub fn update_shipping_order_display_status(
  draft_store: Store,
  fulfillment_order: FulfillmentOrderRecord,
  fulfillment_status: String,
) -> Store {
  case fulfillment_order.order_id, fulfillment_status {
    Some(order_id), "IN_PROGRESS" ->
      update_shipping_order_display_status_value(
        draft_store,
        order_id,
        "IN_PROGRESS",
      )
    Some(order_id), "OPEN" ->
      update_shipping_order_display_status_value(
        draft_store,
        order_id,
        "UNFULFILLED",
      )
    _, _ -> draft_store
  }
}

@internal
pub fn update_shipping_order_display_status_value(
  draft_store: Store,
  order_id: String,
  display_status: String,
) -> Store {
  case store.get_effective_shipping_order_by_id(draft_store, order_id) {
    Some(order) -> {
      let updated =
        ShippingOrderRecord(
          ..order,
          data: captured_upsert_fields(order.data, [
            #("displayFulfillmentStatus", CapturedString(display_status)),
          ]),
        )
      let #(_, next_store) =
        store.stage_upsert_shipping_order(draft_store, updated)
      next_store
    }
    None -> draft_store
  }
}

@internal
pub fn max_int(left: Int, right: Int) -> Int {
  case left < right {
    True -> right
    False -> left
  }
}

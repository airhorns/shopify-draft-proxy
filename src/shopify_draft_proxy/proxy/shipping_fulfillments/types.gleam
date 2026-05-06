//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/proxy/graphql_helpers.{type SourceValue}
import shopify_draft_proxy/state/types.{type CapturedJsonValue}

@internal
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    errors: List(Json),
    staged_resource_ids: List(String),
  )
}

@internal
pub type QueryFieldResult {
  QueryFieldResult(key: String, value: Json, errors: List(Json))
}

@internal
pub type CarrierServiceUserError {
  CarrierServiceUserError(
    field: Option(List(String)),
    message: String,
    code: String,
  )
}

@internal
pub type LocalPickupUserError {
  LocalPickupUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type FulfillmentServiceUserError {
  FulfillmentServiceUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type DeliveryProfileUserError {
  DeliveryProfileUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type ShippingPackageUpdateUserError {
  ShippingPackageUpdateUserError(
    field: Option(List(String)),
    message: String,
    code: String,
  )
}

@internal
pub type FulfillmentOrderMoveDestination {
  FulfillmentOrderMoveDestination(
    id: String,
    assigned_location: CapturedJsonValue,
  )
}

@internal
pub type FulfillmentEventUserError {
  FulfillmentEventUserError(field: List(String), message: String, code: String)
}

@internal
pub type FulfillmentOrderSplitInput {
  FulfillmentOrderSplitInput(
    index: Int,
    fulfillment_order_id: Option(String),
    line_items: List(FulfillmentOrderSplitLineItemInput),
  )
}

@internal
pub type FulfillmentOrderSplitLineItemInput {
  FulfillmentOrderSplitLineItemInput(
    index: Int,
    id: Option(String),
    quantity: Option(Int),
    quantity_is_int: Bool,
  )
}

@internal
pub type FulfillmentOrderSplitUserError {
  FulfillmentOrderSplitUserError(
    field: SourceValue,
    message: String,
    code: String,
  )
}

//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/option.{type Option}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{type CapturedJsonValue, type OrderRecord}

@internal
pub type OrdersError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type OrderEditUserError {
  OrderEditUserError(
    field_path: List(String),
    message: String,
    code: Option(String),
  )
}

@internal
pub type ReturnMutationResult {
  ReturnMutationResult(
    order: Option(OrderRecord),
    order_return: Option(CapturedJsonValue),
    store: Store,
    identity: SyntheticIdentityRegistry,
    user_errors: List(#(List(String), String, Option(String))),
  )
}

@internal
pub type ReverseDeliveryMutationResult {
  ReverseDeliveryMutationResult(
    order: Option(OrderRecord),
    order_return: Option(CapturedJsonValue),
    reverse_fulfillment_order: Option(CapturedJsonValue),
    reverse_delivery: Option(CapturedJsonValue),
    store: Store,
    identity: SyntheticIdentityRegistry,
    user_errors: List(#(List(String), String, Option(String))),
  )
}

@internal
pub type DisposeMutationResult {
  DisposeMutationResult(
    line_items: List(CapturedJsonValue),
    store: Store,
    identity: SyntheticIdentityRegistry,
    user_errors: List(#(List(String), String, Option(String))),
  )
}

@internal
pub type RequestedFulfillmentLineItem {
  RequestedFulfillmentLineItem(id: String, quantity: Option(Int))
}

@internal
pub type FulfillmentOrderSplitInput {
  FulfillmentOrderSplitInput(
    fulfillment_order_id: String,
    line_items: List(RequestedFulfillmentLineItem),
  )
}

@internal
pub type FulfillmentOrderSplitResult {
  FulfillmentOrderSplitResult(
    fulfillment_order: CapturedJsonValue,
    remaining_fulfillment_order: CapturedJsonValue,
    replacement_fulfillment_order: Option(CapturedJsonValue),
  )
}

@internal
pub type FulfillmentOrderMergeInput {
  FulfillmentOrderMergeInput(ids: List(String))
}

@internal
pub type FulfillmentOrderMergeResult {
  FulfillmentOrderMergeResult(fulfillment_order: CapturedJsonValue)
}

@internal
pub type RefundCreateUserError {
  RefundCreateUserError(
    field_path: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type UserErrorFieldSegment {
  UserErrorField(String)
  UserErrorIndex(Int)
}

@internal
pub type OrderMutationUserError {
  OrderMutationUserError(
    field_path: Option(List(UserErrorFieldSegment)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type OrderCreateDiscount {
  OrderCreateDiscount(
    codes: List(String),
    applications: List(CapturedJsonValue),
    total_discounts_set: CapturedJsonValue,
  )
}

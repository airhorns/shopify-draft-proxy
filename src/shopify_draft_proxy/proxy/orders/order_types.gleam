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

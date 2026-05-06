//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/orders/mutations
import shopify_draft_proxy/proxy/orders/order_types
import shopify_draft_proxy/proxy/orders/queries
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type OrdersError =
  order_types.OrdersError

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
      "fulfillment",
      "fulfillmentOrder",
      "fulfillmentOrders",
      "assignedFulfillmentOrders",
      "manualHoldsFulfillmentOrders",
      "order",
      "orders",
      "ordersCount",
      "reverseDelivery",
      "reverseFulfillmentOrder",
      "return",
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
      "fulfillmentEventCreate",
      "fulfillmentOrderCancel",
      "fulfillmentOrderClose",
      "fulfillmentOrderAcceptCancellationRequest",
      "fulfillmentOrderAcceptFulfillmentRequest",
      "fulfillmentOrderHold",
      "fulfillmentOrderMove",
      "fulfillmentOrderOpen",
      "fulfillmentOrderRejectCancellationRequest",
      "fulfillmentOrderRejectFulfillmentRequest",
      "fulfillmentOrderReleaseHold",
      "fulfillmentOrderReportProgress",
      "fulfillmentOrderReschedule",
      "fulfillmentOrderMerge",
      "fulfillmentOrderSplit",
      "fulfillmentOrderSubmitCancellationRequest",
      "fulfillmentOrderSubmitFulfillmentRequest",
      "fulfillmentOrdersSetFulfillmentDeadline",
      "fulfillmentTrackingInfoUpdate",
      "orderCancel",
      "orderCapture",
      "orderClose",
      "orderCreate",
      "orderCreateMandatePayment",
      "orderCreateManualPayment",
      "orderDelete",
      "orderEditAddCustomItem",
      "orderEditAddLineItemDiscount",
      "orderEditAddShippingLine",
      "orderEditAddVariant",
      "orderEditBegin",
      "orderEditCommit",
      "orderEditRemoveDiscount",
      "orderEditRemoveShippingLine",
      "orderEditSetQuantity",
      "orderEditUpdateShippingLine",
      "orderInvoiceSend",
      "orderMarkAsPaid",
      "orderOpen",
      "orderUpdate",
      "refundCreate",
      "removeFromReturn",
      "returnApproveRequest",
      "returnDeclineRequest",
      "returnCancel",
      "returnClose",
      "returnCreate",
      "returnProcess",
      "returnReopen",
      "returnRequest",
      "reverseDeliveryCreateWithShipping",
      "reverseDeliveryShippingUpdate",
      "reverseFulfillmentOrderDispose",
      "taxSummaryCreate",
      "transactionVoid",
    ],
    name,
  )
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OrdersError) {
  queries.process(store, document, variables)
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

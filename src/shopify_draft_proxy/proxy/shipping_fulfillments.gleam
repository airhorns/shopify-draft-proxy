//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/mutations
import shopify_draft_proxy/proxy/shipping_fulfillments/queries
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type ShippingFulfillmentsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_shipping_fulfillment_query_root(name: String) -> Bool {
  case name {
    "availableCarrierServices"
    | "deliveryProfile"
    | "deliveryProfiles"
    | "locationsAvailableForDeliveryProfilesConnection"
    | "carrierService"
    | "fulfillmentService"
    | "fulfillment"
    | "fulfillmentOrder"
    | "fulfillmentOrders"
    | "reverseDelivery"
    | "reverseFulfillmentOrder"
    | "assignedFulfillmentOrders"
    | "manualHoldsFulfillmentOrders"
    | "carrierServices" -> True
    _ -> False
  }
}

pub fn is_shipping_fulfillment_mutation_root(name: String) -> Bool {
  case name {
    "carrierServiceCreate"
    | "carrierServiceUpdate"
    | "carrierServiceDelete"
    | "deliveryProfileCreate"
    | "deliveryProfileUpdate"
    | "deliveryProfileRemove"
    | "fulfillmentServiceCreate"
    | "fulfillmentServiceUpdate"
    | "fulfillmentServiceDelete"
    | "fulfillmentOrderSubmitFulfillmentRequest"
    | "fulfillmentOrderAcceptFulfillmentRequest"
    | "fulfillmentOrderRejectFulfillmentRequest"
    | "fulfillmentOrderSubmitCancellationRequest"
    | "fulfillmentOrderAcceptCancellationRequest"
    | "fulfillmentOrderRejectCancellationRequest"
    | "fulfillmentEventCreate"
    | "fulfillmentOrderHold"
    | "fulfillmentOrderReleaseHold"
    | "fulfillmentOrderMove"
    | "fulfillmentOrderReschedule"
    | "fulfillmentOrderReportProgress"
    | "fulfillmentOrderOpen"
    | "fulfillmentOrderClose"
    | "fulfillmentOrderCancel"
    | "fulfillmentOrderSplit"
    | "fulfillmentOrdersSetFulfillmentDeadline"
    | "fulfillmentOrderMerge"
    | "reverseDeliveryCreateWithShipping"
    | "reverseDeliveryShippingUpdate"
    | "reverseFulfillmentOrderDispose"
    | "orderEditAddShippingLine"
    | "orderEditRemoveShippingLine"
    | "orderEditUpdateShippingLine"
    | "locationLocalPickupEnable"
    | "locationLocalPickupDisable"
    | "shippingPackageUpdate"
    | "shippingPackageMakeDefault"
    | "shippingPackageDelete" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, ShippingFulfillmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(queries.handle_query_fields(store, fields, fragments, variables))
    }
  }
}

/// Pattern 2 for cold LiveHybrid shipping reads: fetch the captured
/// upstream response, hydrate the shipping/store slices needed by
/// later local lifecycle handlers, and return Shopify's payload
/// verbatim. Once local shipping state or a proxy-synthetic id is
/// involved, stay local so staged read-after-write effects are not
/// bypassed.
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

pub fn local_has_shipping_resource_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_shipping_resource_id(proxy, variables)
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

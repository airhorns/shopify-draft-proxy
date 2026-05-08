//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcObject, SrcString,
  get_document_fragments, project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/mutations
import shopify_draft_proxy/proxy/shipping_fulfillments/queries
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers
import shopify_draft_proxy/proxy/shipping_fulfillments/sources
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

pub fn serialize_delivery_carrier_service_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_carrier_service_by_id(store, id) {
    Some(record) ->
      project_node_source(
        serializers.carrier_service_source(record),
        "DeliveryCarrierService",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_delivery_profile_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_delivery_profile_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.delivery_profile_source(record),
        "DeliveryProfile",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_delivery_profile_nested_node_by_id(
  store: Store,
  id: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
  fallback: fn() -> Json,
) -> Json {
  store
  |> store.list_effective_delivery_profiles
  |> list.find_map(fn(profile) {
    case sources.find_captured_node_source_by_id(profile.data, id) {
      Some(source) -> Ok(source)
      None -> Error(Nil)
    }
  })
  |> option.from_result
  |> option.map(fn(source) {
    project_node_source(source, typename, selections, fragments)
  })
  |> option.unwrap(fallback())
}

pub fn serialize_fulfillment_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_fulfillment_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.fulfillment_source(record),
        "Fulfillment",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_fulfillment_order_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_fulfillment_order_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.fulfillment_order_source(record),
        "FulfillmentOrder",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_reverse_delivery_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_reverse_delivery_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.reverse_delivery_source(store, record),
        "ReverseDelivery",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_reverse_fulfillment_order_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_reverse_fulfillment_order_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.reverse_fulfillment_order_source(store, record),
        "ReverseFulfillmentOrder",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_calculated_order_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_calculated_order_by_id(store, id) {
    Some(record) ->
      project_node_source(
        sources.calculated_order_source(record),
        "CalculatedOrder",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

fn project_node_source(
  source: SourceValue,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    source_with_typename(source, typename),
    selections,
    fragments,
  )
}

fn source_with_typename(source: SourceValue, typename: String) -> SourceValue {
  case source {
    SrcObject(fields) ->
      SrcObject(dict.insert(fields, "__typename", SrcString(typename)))
    _ -> source
  }
}

//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/carrier_services.{
  handle_carrier_service_create, handle_carrier_service_delete,
  handle_carrier_service_update, handle_delivery_profile_create,
  handle_delivery_profile_remove, handle_delivery_profile_update,
  handle_fulfillment_service_create, handle_fulfillment_service_delete,
  handle_fulfillment_service_update, handle_location_local_pickup_disable,
  handle_location_local_pickup_enable, handle_shipping_package_delete,
  handle_shipping_package_make_default, handle_shipping_package_update,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/delivery_profiles.{
  delivery_profile_location_available,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_order_helpers.{
  fulfillment_order_merge_ids, fulfillment_order_split_ids,
  fulfillment_order_user_error_payload,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_orders.{
  handle_fulfillment_order_cancel, handle_fulfillment_order_hold,
  handle_fulfillment_order_merge, handle_fulfillment_order_move,
  handle_fulfillment_order_release_hold, handle_fulfillment_order_simple_status,
  handle_fulfillment_order_split, handle_fulfillment_orders_set_deadline,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_requests.{
  handle_fulfillment_event_create,
  handle_fulfillment_order_accept_cancellation_request,
  handle_fulfillment_order_request_status_update,
  handle_fulfillment_order_submit_cancellation_request,
  handle_fulfillment_order_submit_request, handle_order_edit_add_shipping_line,
  handle_order_edit_remove_shipping_line, handle_order_edit_update_shipping_line,
  handle_reverse_delivery_create_with_shipping,
  handle_reverse_delivery_shipping_update,
  handle_reverse_fulfillment_order_dispose,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  read_object, read_object_array, read_string, read_string_array, resolved_args,
  unique_strings,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/queries.{
  hydrate_from_upstream_response, hydrate_product_variant_nodes,
  hydrate_shipping_package_response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        upstream,
      )
    }
  }
}

@internal
pub fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [], [])
  let #(data_entries, final_store, final_identity, staged_ids, drafts, errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        current_store,
        current_identity,
        all_staged,
        all_drafts,
        all_errors,
      ) = acc
      case field {
        Field(name: name, ..) -> {
          let current_store =
            hydrate_mutation_prerequisites(
              current_store,
              name.value,
              field,
              variables,
              upstream,
            )
          let dispatched = case name.value {
            "carrierServiceCreate" ->
              Some(handle_carrier_service_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "carrierServiceUpdate" ->
              Some(handle_carrier_service_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "carrierServiceDelete" ->
              Some(handle_carrier_service_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "deliveryProfileCreate" ->
              Some(handle_delivery_profile_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "deliveryProfileUpdate" ->
              Some(handle_delivery_profile_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "deliveryProfileRemove" ->
              Some(handle_delivery_profile_remove(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentServiceCreate" ->
              Some(handle_fulfillment_service_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentServiceUpdate" ->
              Some(handle_fulfillment_service_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentServiceDelete" ->
              Some(handle_fulfillment_service_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderSubmitFulfillmentRequest" ->
              Some(handle_fulfillment_order_submit_request(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderAcceptFulfillmentRequest" ->
              Some(handle_fulfillment_order_request_status_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "FulfillmentOrderAcceptFulfillmentRequestPayload",
                "ACCEPTED",
                "IN_PROGRESS",
              ))
            "fulfillmentOrderRejectFulfillmentRequest" ->
              Some(handle_fulfillment_order_request_status_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "FulfillmentOrderRejectFulfillmentRequestPayload",
                "REJECTED",
                "OPEN",
              ))
            "fulfillmentOrderSubmitCancellationRequest" ->
              Some(handle_fulfillment_order_submit_cancellation_request(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderAcceptCancellationRequest" ->
              Some(handle_fulfillment_order_accept_cancellation_request(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderRejectCancellationRequest" ->
              Some(handle_fulfillment_order_request_status_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "FulfillmentOrderRejectCancellationRequestPayload",
                "CANCELLATION_REJECTED",
                "IN_PROGRESS",
              ))
            "fulfillmentEventCreate" ->
              Some(handle_fulfillment_event_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderHold" ->
              Some(handle_fulfillment_order_hold(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderReleaseHold" ->
              Some(handle_fulfillment_order_release_hold(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderMove" ->
              Some(handle_fulfillment_order_move(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderReschedule" ->
              Some(fulfillment_order_user_error_payload(
                current_store,
                current_identity,
                field,
                fragments,
                "FulfillmentOrderReschedulePayload",
                "Fulfillment order must be scheduled.",
              ))
            "fulfillmentOrderReportProgress" ->
              Some(handle_fulfillment_order_simple_status(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "FulfillmentOrderReportProgressPayload",
                "IN_PROGRESS",
              ))
            "fulfillmentOrderOpen" ->
              Some(handle_fulfillment_order_simple_status(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "FulfillmentOrderOpenPayload",
                "OPEN",
              ))
            "fulfillmentOrderClose" ->
              Some(fulfillment_order_user_error_payload(
                current_store,
                current_identity,
                field,
                fragments,
                "FulfillmentOrderClosePayload",
                "The fulfillment order's assigned fulfillment service must be of api type",
              ))
            "fulfillmentOrderCancel" ->
              Some(handle_fulfillment_order_cancel(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderSplit" ->
              Some(handle_fulfillment_order_split(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrdersSetFulfillmentDeadline" ->
              Some(handle_fulfillment_orders_set_deadline(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "fulfillmentOrderMerge" ->
              Some(handle_fulfillment_order_merge(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "reverseDeliveryCreateWithShipping" ->
              Some(handle_reverse_delivery_create_with_shipping(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "reverseDeliveryShippingUpdate" ->
              Some(handle_reverse_delivery_shipping_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "reverseFulfillmentOrderDispose" ->
              Some(handle_reverse_fulfillment_order_dispose(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "orderEditAddShippingLine" ->
              Some(handle_order_edit_add_shipping_line(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "orderEditRemoveShippingLine" ->
              Some(handle_order_edit_remove_shipping_line(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "orderEditUpdateShippingLine" ->
              Some(handle_order_edit_update_shipping_line(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "locationLocalPickupEnable" ->
              Some(handle_location_local_pickup_enable(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "locationLocalPickupDisable" ->
              Some(handle_location_local_pickup_disable(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "shippingPackageUpdate" ->
              Some(handle_shipping_package_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "shippingPackageMakeDefault" ->
              Some(handle_shipping_package_make_default(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "shippingPackageDelete" ->
              Some(handle_shipping_package_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatched {
            None -> acc
            Some(#(result, next_store, next_identity)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "shipping-fulfillments",
                  "stage-locally",
                  Some(
                    "Staged locally in the in-memory shipping/fulfillment draft store; no supported Shopify shipping mutation is sent upstream at runtime.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                next_store,
                next_identity,
                list.append(all_staged, result.staged_resource_ids),
                list.append(all_drafts, [draft]),
                list.append(all_errors, result.errors),
              )
            }
          }
        }
        _ -> acc
      }
    })

  let data = json.object([#("data", json.object(data_entries))])
  let response = case errors {
    [] -> data
    _ ->
      json.object([
        #("errors", json.array(errors, fn(error) { error })),
        #("data", json.object(data_entries)),
      ])
  }

  MutationOutcome(
    data: response,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: drafts,
  )
}

@internal
pub fn hydrate_mutation_prerequisites(
  store_in: Store,
  root_name: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let args = resolved_args(field, variables)
  case root_name {
    "deliveryProfileCreate" -> {
      // Pattern 2: delivery profiles project `profileItems` with
      // product/variant titles, which are upstream product-domain data.
      // Hydrate only the associated variants first; Snapshot mode and
      // missing cassettes fall back to the existing local-only shape.
      case read_object(args, "profile") {
        Some(profile) -> {
          let variant_ids = read_string_array(profile, "variantsToAssociate")
          let location_ids = delivery_profile_create_location_ids(profile)
          store_in
          |> maybe_hydrate_delivery_profile_variants(variant_ids, upstream)
          |> maybe_hydrate_delivery_profile_locations(location_ids, upstream)
        }
        None -> store_in
      }
    }
    "deliveryProfileRemove" ->
      maybe_hydrate_delivery_profile(
        store_in,
        read_string(args, "id"),
        upstream,
      )
    "fulfillmentOrderSubmitFulfillmentRequest"
    | "fulfillmentOrderAcceptFulfillmentRequest"
    | "fulfillmentOrderRejectFulfillmentRequest"
    | "fulfillmentOrderSubmitCancellationRequest"
    | "fulfillmentOrderAcceptCancellationRequest"
    | "fulfillmentOrderRejectCancellationRequest"
    | "fulfillmentOrderHold"
    | "fulfillmentOrderReleaseHold"
    | "fulfillmentOrderMove"
    | "fulfillmentOrderReschedule"
    | "fulfillmentOrderReportProgress"
    | "fulfillmentOrderOpen"
    | "fulfillmentOrderClose"
    | "fulfillmentOrderCancel" ->
      maybe_hydrate_fulfillment_order(
        store_in,
        read_string(args, "id"),
        upstream,
      )
    "fulfillmentOrderSplit" ->
      hydrate_fulfillment_order_ids(
        store_in,
        fulfillment_order_split_ids(args),
        upstream,
      )
    "fulfillmentOrdersSetFulfillmentDeadline" ->
      hydrate_fulfillment_order_ids(
        store_in,
        read_string_array(args, "fulfillmentOrderIds"),
        upstream,
      )
    "fulfillmentOrderMerge" ->
      hydrate_fulfillment_order_ids(
        store_in,
        fulfillment_order_merge_ids(args),
        upstream,
      )
    "fulfillmentEventCreate" ->
      case read_object(args, "fulfillmentEvent") {
        Some(input) ->
          // Pattern 2: events must validate the parent fulfillment before
          // staging. In LiveHybrid, hydrate the existing upstream fulfillment
          // by ID; Snapshot/no-transport mode falls back to local-only lookup.
          maybe_hydrate_fulfillment(
            store_in,
            read_string(input, "fulfillmentId"),
            upstream,
          )
        None -> store_in
      }
    "shippingPackageUpdate"
    | "shippingPackageMakeDefault"
    | "shippingPackageDelete" ->
      maybe_hydrate_shipping_package(
        store_in,
        read_string(args, "id"),
        upstream,
      )
    _ -> store_in
  }
}

@internal
pub fn maybe_hydrate_fulfillment(
  store_in: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    Some(id) -> {
      case is_proxy_synthetic_gid(id) {
        True -> store_in
        False ->
          case store.get_effective_fulfillment_by_id(store_in, id) {
            Some(_) -> store_in
            None -> {
              let query =
                "query ShippingFulfillmentEventCreateFulfillmentHydrate($id: ID!) {
  fulfillment(id: $id) {
    id status displayStatus createdAt updatedAt deliveredAt estimatedDeliveryAt inTransitAt
    trackingInfo(first: 1) { number url company }
    events(first: 5) {
      nodes {
        id status message happenedAt createdAt estimatedDeliveryAt
        city province country zip address1 latitude longitude
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    service {
      id handle serviceName trackingSupport type
      location { id name }
    }
    location { id name }
    originAddress { address1 address2 city countryCode provinceCode zip }
    fulfillmentLineItems(first: 5) {
      nodes { id quantity lineItem { id title } }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    order { id name displayFulfillmentStatus }
  }
}
"
              let variables = json.object([#("id", json.string(id))])
              case
                upstream_query.fetch_sync(
                  upstream.origin,
                  upstream.transport,
                  upstream.headers,
                  "ShippingFulfillmentEventCreateFulfillmentHydrate",
                  query,
                  variables,
                )
              {
                Ok(value) -> hydrate_from_upstream_response(store_in, value)
                Error(_) -> store_in
              }
            }
          }
      }
    }
    None -> store_in
  }
}

@internal
pub fn hydrate_fulfillment_order_ids(
  store_in: Store,
  ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  list.fold(ids, store_in, fn(current, id) {
    maybe_hydrate_fulfillment_order(current, Some(id), upstream)
  })
}

@internal
pub fn maybe_hydrate_fulfillment_order(
  store_in: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    Some(id) -> {
      case is_proxy_synthetic_gid(id) {
        True -> store_in
        False ->
          case store.get_effective_fulfillment_order_by_id(store_in, id) {
            Some(_) -> store_in
            None -> {
              let query =
                "query ShippingFulfillmentOrderHydrate($id: ID!) {
  fulfillmentOrder(id: $id) {
    id status requestStatus assignmentStatus fulfillAt fulfillBy updatedAt
    supportedActions { action }
    assignedLocation { name location { id name } }
    fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
    merchantRequests(first: 10) { nodes { kind message requestOptions } }
    lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
    order { id name displayFulfillmentStatus }
  }
}
"
              let variables = json.object([#("id", json.string(id))])
              case
                upstream_query.fetch_sync(
                  upstream.origin,
                  upstream.transport,
                  upstream.headers,
                  "ShippingFulfillmentOrderHydrate",
                  query,
                  variables,
                )
              {
                Ok(value) -> hydrate_from_upstream_response(store_in, value)
                Error(_) -> store_in
              }
            }
          }
      }
    }
    None -> store_in
  }
}

@internal
pub fn maybe_hydrate_delivery_profile(
  store_in: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    Some(id) ->
      case store.get_effective_delivery_profile_by_id(store_in, id) {
        Some(_) -> store_in
        None -> {
          let query =
            "query ShippingDeliveryProfileHydrate($id: ID!) {
  deliveryProfile(id: $id) { id name default merchantOwned version }
}
"
          let variables = json.object([#("id", json.string(id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "ShippingDeliveryProfileHydrate",
              query,
              variables,
            )
          {
            Ok(value) -> hydrate_from_upstream_response(store_in, value)
            Error(_) -> store_in
          }
        }
      }
    None -> store_in
  }
}

@internal
pub fn maybe_hydrate_delivery_profile_variants(
  store_in: Store,
  ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  let missing =
    ids
    |> list.filter(fn(id) {
      case store.get_effective_variant_by_id(store_in, id) {
        Some(_) -> False
        None -> True
      }
    })
  case missing {
    [] -> store_in
    _ -> {
      let query =
        "query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) {
  nodes(ids: $ids) {
    ... on ProductVariant { id title product { id title handle } }
  }
}
"
      let variables = json.object([#("ids", json.array(missing, json.string))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "ShippingDeliveryProfileVariantsHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_product_variant_nodes(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn maybe_hydrate_delivery_profile_locations(
  store_in: Store,
  ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  let missing =
    ids
    |> list.filter(fn(id) { !delivery_profile_location_available(store_in, id) })
  case missing {
    [] -> store_in
    _ -> {
      let query =
        "query ShippingDeliveryProfileLocationsHydrate {
  locationsAvailableForDeliveryProfilesConnection(first: 250) {
    nodes { id name isActive isFulfillmentService }
  }
}
"
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "ShippingDeliveryProfileLocationsHydrate",
          query,
          json.object([]),
        )
      {
        Ok(value) -> hydrate_from_upstream_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn delivery_profile_create_location_ids(
  input: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  list.append(
    read_object_array(input, "profileLocationGroups"),
    read_object_array(input, "locationGroupsToCreate"),
  )
  |> list.flat_map(fn(group) {
    list.append(
      read_string_array(group, "locations"),
      read_string_array(group, "locationsToAdd"),
    )
  })
  |> unique_strings
}

@internal
pub fn maybe_hydrate_shipping_package(
  store_in: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    Some(id) ->
      case store.get_effective_shipping_package_by_id(store_in, id) {
        Some(_) -> store_in
        None -> {
          // Pattern 2 for local-runtime shipping-package parity: Admin
          // GraphQL has no package read root in the captured API version,
          // so the cassette supplies the recorded local seed package.
          // Without a cassette/Snapshot mode this remains a no-op.
          let query =
            "query ShippingPackageHydrate($id: ID!) {
  shippingPackage(id: $id) { id name type boxType default weight { value unit } dimensions { length width height unit } createdAt updatedAt }
}
"
          let variables = json.object([#("id", json.string(id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "ShippingPackageHydrate",
              query,
              variables,
            )
          {
            Ok(value) -> hydrate_shipping_package_response(store_in, value)
            Error(_) -> store_in
          }
        }
      }
    None -> store_in
  }
}

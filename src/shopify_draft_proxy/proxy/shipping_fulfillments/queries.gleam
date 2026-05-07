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
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, SerializeConnectionConfig, SrcList,
  SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  captured_array_field, read_bool, read_string, resolved_args,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers.{
  carrier_service_source, project_carrier_service, project_delivery_profile,
  project_fulfillment, project_fulfillment_order, project_fulfillment_service,
  project_reverse_delivery, project_reverse_fulfillment_order,
  project_shipping_order, project_store_property_record,
  serialize_assigned_fulfillment_orders_connection,
  serialize_carrier_services_connection, serialize_delivery_profiles_connection,
  serialize_fulfillment_orders_connection,
  serialize_manual_holds_fulfillment_orders_connection,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  filter_active_non_fulfillment_locations, is_active_location,
  sort_store_property_locations_by_id, store_property_record_source,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CarrierServiceRecord, type DeliveryProfileRecord,
  type FulfillmentOrderRecord, type FulfillmentRecord,
  type FulfillmentServiceRecord, type ProductRecord, type ProductVariantRecord,
  type ShippingOrderRecord, type ShippingPackageDimensionsRecord,
  type ShippingPackageRecord, type ShippingPackageWeightRecord,
  type StorePropertyRecord, type StorePropertyValue, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CarrierServiceRecord, DeliveryProfileRecord, FulfillmentOrderRecord,
  FulfillmentRecord, FulfillmentServiceRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ShippingOrderRecord, ShippingPackageDimensionsRecord,
  ShippingPackageRecord, ShippingPackageWeightRecord, StorePropertyBool,
  StorePropertyFloat, StorePropertyInt, StorePropertyList, StorePropertyNull,
  StorePropertyObject, StorePropertyRecord, StorePropertyString,
}

fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, Nil) {
  case root_field.get_root_fields(document) {
    Error(_) -> Error(Nil)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_query_fields(store, fields, fragments, variables))
    }
  }
}

@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_upstream = case proxy.config.read_mode {
    LiveHybrid ->
      should_fetch_upstream_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_upstream {
    True ->
      fetch_and_hydrate_live_hybrid_query(
        proxy,
        request,
        parsed,
        document,
        variables,
      )
    False -> local_query_response(proxy, document, variables)
  }
}

@internal
pub fn should_fetch_upstream_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "deliveryProfile" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "fulfillment" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "fulfillmentOrder" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "carrierService" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "fulfillmentService" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "reverseDelivery" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "reverseFulfillmentOrder" ->
      !local_has_shipping_resource_id(proxy, variables)
    parse_operation.QueryOperation, "deliveryProfiles" ->
      !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation, "fulfillmentOrders" ->
      !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation, "assignedFulfillmentOrders" ->
      !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation, "manualHoldsFulfillmentOrders" ->
      !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation, "order" ->
      !local_has_shipping_order_id(proxy, variables)
    parse_operation.QueryOperation, "availableCarrierServices" ->
      !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation,
      "locationsAvailableForDeliveryProfilesConnection"
    -> !has_local_shipping_query_state(proxy, variables)
    parse_operation.QueryOperation, "carrierServices" ->
      !has_local_shipping_query_state(proxy, variables)
    _, _ -> False
  }
}

@internal
pub fn local_has_shipping_resource_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || local_shipping_resource_staged(proxy.store, id)
      _ -> False
    }
  })
}

@internal
pub fn local_shipping_resource_staged(store_in: Store, id: String) -> Bool {
  dict.has_key(store_in.staged_state.delivery_profiles, id)
  || dict.has_key(store_in.staged_state.deleted_delivery_profile_ids, id)
  || dict.has_key(store_in.staged_state.fulfillments, id)
  || dict.has_key(store_in.staged_state.fulfillment_orders, id)
  || dict.has_key(store_in.staged_state.carrier_services, id)
  || dict.has_key(store_in.staged_state.deleted_carrier_service_ids, id)
  || dict.has_key(store_in.staged_state.fulfillment_services, id)
  || dict.has_key(store_in.staged_state.deleted_fulfillment_service_ids, id)
  || dict.has_key(store_in.staged_state.reverse_deliveries, id)
  || dict.has_key(store_in.staged_state.reverse_fulfillment_orders, id)
  || dict.has_key(store_in.staged_state.shipping_packages, id)
  || dict.has_key(store_in.staged_state.deleted_shipping_package_ids, id)
}

@internal
pub fn local_has_shipping_order_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || dict.has_key(proxy.store.staged_state.shipping_orders, id)
        || dict.has_key(proxy.store.base_state.shipping_orders, id)
      _ -> False
    }
  })
}

@internal
pub fn has_local_shipping_query_state(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic || has_staged_shipping_query_state(proxy.store)
}

@internal
pub fn has_staged_shipping_query_state(store_in: Store) -> Bool {
  dict.size(store_in.staged_state.delivery_profiles) > 0
  || dict.size(store_in.staged_state.deleted_delivery_profile_ids) > 0
  || dict.size(store_in.staged_state.fulfillments) > 0
  || dict.size(store_in.staged_state.fulfillment_orders) > 0
  || dict.size(store_in.staged_state.carrier_services) > 0
  || dict.size(store_in.staged_state.deleted_carrier_service_ids) > 0
  || dict.size(store_in.staged_state.fulfillment_services) > 0
  || dict.size(store_in.staged_state.deleted_fulfillment_service_ids) > 0
  || dict.size(store_in.staged_state.reverse_deliveries) > 0
  || dict.size(store_in.staged_state.reverse_fulfillment_orders) > 0
  || dict.size(store_in.staged_state.shipping_packages) > 0
  || dict.size(store_in.staged_state.deleted_shipping_package_ids) > 0
  || dict.size(store_in.staged_state.store_property_locations) > 0
  || dict.size(store_in.staged_state.deleted_store_property_location_ids) > 0
}

@internal
pub fn fetch_and_hydrate_live_hybrid_query(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let operation_name =
    parsed.name
    |> option.unwrap("ShippingFulfillmentsLiveHybridRead")
  case
    upstream_query.fetch_sync(
      proxy.config.shopify_admin_origin,
      proxy.upstream_transport,
      request.headers,
      operation_name,
      document,
      variables_to_json(variables),
    )
  {
    Ok(value) -> {
      let next_store = hydrate_from_upstream_response(proxy.store, value)
      #(
        Response(
          status: 200,
          body: commit.json_value_to_json(value),
          headers: [],
        ),
        DraftProxy(..proxy, store: next_store),
      )
    }
    Error(err) -> #(
      Response(
        status: 502,
        body: json.object([
          #(
            "errors",
            json.array(
              [
                json.object([
                  #("message", json.string(fetch_error_message(err))),
                ]),
              ],
              fn(x) { x },
            ),
          ),
        ]),
        headers: [],
      ),
      proxy,
    )
  }
}

@internal
pub fn local_query_response(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case process(proxy.store, document, variables) {
    Ok(envelope) -> #(Response(status: 200, body: envelope, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([
          #(
            "errors",
            json.array(
              [
                json.object([
                  #(
                    "message",
                    json.string("Failed to handle shipping fulfillments query"),
                  ),
                ]),
              ],
              fn(x) { x },
            ),
          ),
        ]),
        headers: [],
      ),
      proxy,
    )
  }
}

@internal
pub fn variables_to_json(
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    dict.to_list(variables)
    |> list.map(fn(pair) {
      #(pair.0, root_field.resolved_value_to_json(pair.1))
    }),
  )
}

@internal
pub fn fetch_error_message(error: upstream_query.FetchError) -> String {
  case error {
    upstream_query.TransportFailed(message) -> message
    upstream_query.HttpStatusError(status, body) ->
      "upstream returned HTTP " <> int.to_string(status) <> ": " <> body
    upstream_query.MalformedResponse(message) -> message
    upstream_query.NoTransportInstalled -> "no upstream transport installed"
  }
}

@internal
pub fn hydrate_from_upstream_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      store_in
      |> hydrate_delivery_profile_roots(data)
      |> hydrate_shipping_order_roots(data)
      |> hydrate_fulfillment_roots(data)
      |> hydrate_store_property_locations(data)
      |> hydrate_available_carrier_services(data)
    None -> store_in
  }
}

@internal
pub fn hydrate_shipping_order_roots(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let orders =
    [
      json_get(data, "order")
        |> option.then(non_null_json)
        |> option_to_list,
      nested_order_roots(data),
    ]
    |> list.flatten
    |> list.filter_map(shipping_order_record_from_json)
  case orders {
    [] -> store_in
    _ -> store.upsert_base_shipping_orders(store_in, orders)
  }
}

@internal
pub fn hydrate_delivery_profile_roots(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let profiles =
    list.append(
      json_get(data, "deliveryProfile")
        |> option.then(non_null_json)
        |> option_to_list,
      nodes_from_connection(json_get(data, "deliveryProfiles")),
    )
    |> list.filter_map(delivery_profile_record_from_json)
  case profiles {
    [] -> store_in
    _ -> store.upsert_base_delivery_profiles(store_in, profiles)
  }
}

@internal
pub fn hydrate_fulfillment_roots(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let fulfillment_services =
    [
      json_get(data, "fulfillmentService")
        |> option.then(non_null_json)
        |> option_to_list,
      fulfillment_services_from_shop(data),
    ]
    |> list.flatten
    |> list.filter_map(fulfillment_service_record_from_json)
  let fulfillment_service_locations =
    [
      json_get(data, "fulfillmentService")
        |> option.then(non_null_json)
        |> option_to_list,
      fulfillment_services_from_shop(data),
    ]
    |> list.flatten
    |> list.filter_map(fulfillment_service_location_from_json)
  let fulfillments =
    list.append(
      json_get(data, "fulfillment")
        |> option.then(non_null_json)
        |> option_to_list,
      fulfillments_from_order(data),
    )
    |> list.filter_map(fulfillment_record_from_json)
  let fulfillment_orders =
    [
      json_get(data, "fulfillmentOrder")
        |> option.then(non_null_json)
        |> option_to_list,
      nodes_from_connection(json_get(data, "fulfillmentOrders")),
      nodes_from_connection(json_get(data, "assignedFulfillmentOrders")),
      nodes_from_connection(json_get(data, "manualHoldsFulfillmentOrders")),
      fulfillment_orders_from_order(data),
    ]
    |> list.flatten
    |> list.filter_map(fulfillment_order_record_from_json)
  store_in
  |> store.upsert_base_fulfillment_services(fulfillment_services)
  |> store.upsert_base_fulfillments(fulfillments)
  |> store.upsert_base_fulfillment_orders(fulfillment_orders)
  |> upsert_base_locations(fulfillment_service_locations)
}

fn upsert_base_locations(
  store_in: Store,
  locations: List(StorePropertyRecord),
) -> Store {
  list.fold(locations, store_in, fn(current, location) {
    store.upsert_base_store_property_location(current, location)
  })
}

@internal
pub fn hydrate_product_variant_nodes(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) -> {
      let variants = json_array(json_get(data, "nodes")) |> non_null_json_values
      let products =
        variants
        |> list.filter_map(product_record_from_variant_node)
      let variant_records =
        variants
        |> list.filter_map(product_variant_record_from_json)
      store_in
      |> store.upsert_base_products(products)
      |> store.upsert_base_product_variants(variant_records)
    }
    None -> store_in
  }
}

@internal
pub fn hydrate_shipping_package_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shippingPackage") |> option.then(non_null_json) {
        Some(package) ->
          case shipping_package_record_from_json(package) {
            Ok(record) ->
              store.upsert_base_shipping_packages(store_in, [record])
            Error(_) -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn hydrate_store_property_locations(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let locations =
    nodes_from_connection(json_get(
      data,
      "locationsAvailableForDeliveryProfilesConnection",
    ))
    |> list.append(location_nodes_from_available_carrier_services(data))
    |> list.filter_map(store_property_location_from_json)
  list.fold(locations, store_in, fn(current, location) {
    store.upsert_base_store_property_location(current, location)
  })
}

@internal
pub fn hydrate_available_carrier_services(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let services =
    json_array(json_get(data, "availableCarrierServices"))
    |> list.filter_map(fn(value) {
      case json_get(value, "carrierService") {
        Some(service) -> carrier_service_record_from_json(service)
        None -> Error(Nil)
      }
    })
  case services {
    [] -> store_in
    _ -> store.upsert_base_carrier_services(store_in, services)
  }
}

@internal
pub fn fulfillments_from_order(
  data: commit.JsonValue,
) -> List(commit.JsonValue) {
  case json_get(data, "order") {
    Some(order) -> json_array(json_get(order, "fulfillments"))
    None -> []
  }
}

@internal
pub fn fulfillment_services_from_shop(
  data: commit.JsonValue,
) -> List(commit.JsonValue) {
  case json_get(data, "shop") {
    Some(shop) ->
      fulfillment_service_nodes(json_get(shop, "fulfillmentServices"))
    None -> []
  }
}

fn fulfillment_service_nodes(
  value: Option(commit.JsonValue),
) -> List(commit.JsonValue) {
  case value {
    Some(commit.JsonArray(items)) -> items
    _ -> nodes_from_connection(value)
  }
  |> non_null_json_values
}

@internal
pub fn fulfillment_orders_from_order(
  data: commit.JsonValue,
) -> List(commit.JsonValue) {
  case json_get(data, "order") {
    Some(order) -> {
      let order_id = json_get_string(order, "id")
      nodes_from_connection(json_get(order, "fulfillmentOrders"))
      |> list.map(fn(node) {
        case order_id {
          Some(id) ->
            json_insert_object_field(
              node,
              "order",
              commit.JsonObject([
                #("id", commit.JsonString(id)),
              ]),
            )
          None -> node
        }
      })
    }
    None -> []
  }
}

@internal
pub fn nested_order_roots(data: commit.JsonValue) -> List(commit.JsonValue) {
  [
    json_get(data, "fulfillmentOrder")
      |> option.then(non_null_json)
      |> option_to_list,
    json_get(data, "fulfillment")
      |> option.then(non_null_json)
      |> option_to_list,
  ]
  |> list.flatten
  |> list.filter_map(fn(value) {
    json_get(value, "order")
    |> option.then(non_null_json)
    |> option.to_result(Nil)
  })
}

@internal
pub fn location_nodes_from_available_carrier_services(
  data: commit.JsonValue,
) -> List(commit.JsonValue) {
  json_array(json_get(data, "availableCarrierServices"))
  |> list.flat_map(fn(value) {
    case json_get(value, "locations") {
      Some(commit.JsonArray(items)) -> non_null_json_values(items)
      _ -> []
    }
  })
}

@internal
pub fn delivery_profile_record_from_json(
  value: commit.JsonValue,
) -> Result(DeliveryProfileRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(DeliveryProfileRecord(
    id: id,
    cursor: json_get_string(value, "cursor"),
    merchant_owned: json_get_bool(value, "merchantOwned")
      |> option.unwrap(True),
    data: captured_json_from_commit(value),
  ))
}

@internal
pub fn fulfillment_service_record_from_json(
  value: commit.JsonValue,
) -> Result(FulfillmentServiceRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(FulfillmentServiceRecord(
    id: id,
    handle: json_get_string(value, "handle") |> option.unwrap(""),
    service_name: json_get_string(value, "serviceName") |> option.unwrap(""),
    callback_url: json_get_string(value, "callbackUrl"),
    inventory_management: json_get_bool(value, "inventoryManagement")
      |> option.unwrap(False),
    location_id: case json_get(value, "location") {
      Some(location) -> json_get_string(location, "id")
      None -> None
    },
    requires_shipping_method: json_get_bool(value, "requiresShippingMethod")
      |> option.unwrap(True),
    tracking_support: json_get_bool(value, "trackingSupport")
      |> option.unwrap(False),
    type_: json_get_string(value, "type") |> option.unwrap("THIRD_PARTY"),
  ))
}

@internal
pub fn fulfillment_service_location_from_json(
  value: commit.JsonValue,
) -> Result(StorePropertyRecord, Nil) {
  use location <- result.try(
    json_get(value, "location") |> option.to_result(Nil),
  )
  store_property_location_from_json(location)
}

@internal
pub fn fulfillment_record_from_json(
  value: commit.JsonValue,
) -> Result(FulfillmentRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  let order_id = case json_get(value, "order") {
    Some(order) -> json_get_string(order, "id")
    None -> None
  }
  Ok(FulfillmentRecord(
    id: id,
    order_id: order_id,
    data: captured_json_from_commit(value),
  ))
}

@internal
pub fn shipping_order_record_from_json(
  value: commit.JsonValue,
) -> Result(ShippingOrderRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(ShippingOrderRecord(id: id, data: captured_json_from_commit(value)))
}

@internal
pub fn fulfillment_order_record_from_json(
  value: commit.JsonValue,
) -> Result(FulfillmentOrderRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  let order_id = case json_get(value, "order") {
    Some(order) -> json_get_string(order, "id")
    None -> None
  }
  Ok(FulfillmentOrderRecord(
    id: id,
    order_id: order_id,
    status: json_get_string(value, "status") |> option.unwrap("OPEN"),
    request_status: json_get_string(value, "requestStatus")
      |> option.unwrap("UNSUBMITTED"),
    assigned_location_id: assigned_location_id_from_json(value),
    assignment_status: json_get_string(value, "assignmentStatus"),
    manually_held: !list.is_empty(captured_array_field(
      captured_json_from_commit(value),
      "fulfillmentHolds",
      "nodes",
    )),
    data: captured_json_from_commit(value),
  ))
}

@internal
pub fn product_record_from_variant_node(
  value: commit.JsonValue,
) -> Result(ProductRecord, Nil) {
  use product <- result.try(json_get(value, "product") |> option.to_result(Nil))
  use id <- result.try(json_get_string(product, "id") |> option.to_result(Nil))
  let title = json_get_string(product, "title") |> option.unwrap("")
  Ok(
    ProductRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: json_get_string(product, "handle") |> option.unwrap(""),
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: None,
      tracks_inventory: None,
      created_at: None,
      updated_at: None,
      published_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    ),
  )
}

@internal
pub fn product_variant_record_from_json(
  value: commit.JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  use product <- result.try(json_get(value, "product") |> option.to_result(Nil))
  use product_id <- result.try(
    json_get_string(product, "id") |> option.to_result(Nil),
  )
  Ok(ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: json_get_string(value, "title") |> option.unwrap(""),
    sku: None,
    barcode: None,
    price: None,
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: None,
    selected_options: [],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: json_get_string(value, "cursor"),
  ))
}

@internal
pub fn shipping_package_record_from_json(
  value: commit.JsonValue,
) -> Result(ShippingPackageRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(ShippingPackageRecord(
    id: id,
    name: json_get_string(value, "name"),
    type_: json_get_string(value, "type"),
    box_type: json_get_string(value, "boxType"),
    default: json_get_bool(value, "default") |> option.unwrap(False),
    weight: json_get_weight(value, "weight"),
    dimensions: json_get_dimensions(value, "dimensions"),
    created_at: json_get_string(value, "createdAt") |> option.unwrap(""),
    updated_at: json_get_string(value, "updatedAt") |> option.unwrap(""),
  ))
}

@internal
pub fn assigned_location_id_from_json(
  value: commit.JsonValue,
) -> Option(String) {
  case json_get(value, "assignedLocation") {
    Some(assigned) ->
      case json_get(assigned, "location") {
        Some(location) -> json_get_string(location, "id")
        None -> None
      }
    None -> None
  }
}

@internal
pub fn carrier_service_record_from_json(
  value: commit.JsonValue,
) -> Result(CarrierServiceRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(CarrierServiceRecord(
    id: id,
    name: json_get_string(value, "name"),
    formatted_name: json_get_string(value, "formattedName"),
    callback_url: json_get_string(value, "callbackUrl"),
    active: json_get_bool(value, "active") |> option.unwrap(True),
    supports_service_discovery: json_get_bool(value, "supportsServiceDiscovery")
      |> option.unwrap(False),
    created_at: json_get_string(value, "createdAt") |> option.unwrap(""),
    updated_at: json_get_string(value, "updatedAt") |> option.unwrap(""),
  ))
}

@internal
pub fn store_property_location_from_json(
  value: commit.JsonValue,
) -> Result(StorePropertyRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  let data = case value {
    commit.JsonObject(fields) ->
      fields
      |> list.map(fn(pair) {
        #(pair.0, store_property_value_from_commit(pair.1))
      })
      |> dict.from_list
    _ -> dict.new()
  }
  Ok(StorePropertyRecord(
    id: id,
    cursor: json_get_string(value, "cursor"),
    data: data
      |> dict.insert("id", StorePropertyString(id))
      |> dict.insert(
        "name",
        StorePropertyString(json_get_string(value, "name") |> option.unwrap("")),
      )
      |> dict.insert("isActive", StorePropertyBool(True)),
  ))
}

@internal
pub fn nodes_from_connection(
  value: Option(commit.JsonValue),
) -> List(commit.JsonValue) {
  case value {
    Some(connection) ->
      json_array(json_get(connection, "nodes"))
      |> list.append(edge_nodes_from_connection(connection))
    None -> []
  }
  |> non_null_json_values
}

@internal
pub fn edge_nodes_from_connection(
  connection: commit.JsonValue,
) -> List(commit.JsonValue) {
  json_array(json_get(connection, "edges"))
  |> list.filter_map(fn(edge) {
    case json_get(edge, "node") {
      Some(node) -> Ok(node)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn non_null_json(value: commit.JsonValue) -> Option(commit.JsonValue) {
  case value {
    commit.JsonNull -> None
    _ -> Some(value)
  }
}

@internal
pub fn option_to_list(value: Option(a)) -> List(a) {
  case value {
    Some(item) -> [item]
    None -> []
  }
}

@internal
pub fn non_null_json_values(
  values: List(commit.JsonValue),
) -> List(commit.JsonValue) {
  list.filter(values, fn(value) {
    case value {
      commit.JsonNull -> False
      _ -> True
    }
  })
}

@internal
pub fn json_array(value: Option(commit.JsonValue)) -> List(commit.JsonValue) {
  case value {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

@internal
pub fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

@internal
pub fn json_get_number(value: commit.JsonValue, key: String) -> Option(Float) {
  case json_get(value, key) {
    Some(commit.JsonFloat(n)) -> Some(n)
    Some(commit.JsonInt(n)) -> Some(int.to_float(n))
    _ -> None
  }
}

@internal
pub fn json_get_weight(
  value: commit.JsonValue,
  key: String,
) -> Option(ShippingPackageWeightRecord) {
  case json_get(value, key) {
    Some(weight) ->
      Some(ShippingPackageWeightRecord(
        value: json_get_number(weight, "value"),
        unit: json_get_string(weight, "unit"),
      ))
    None -> None
  }
}

@internal
pub fn json_get_dimensions(
  value: commit.JsonValue,
  key: String,
) -> Option(ShippingPackageDimensionsRecord) {
  case json_get(value, key) {
    Some(dimensions) ->
      Some(ShippingPackageDimensionsRecord(
        length: json_get_number(dimensions, "length"),
        width: json_get_number(dimensions, "width"),
        height: json_get_number(dimensions, "height"),
        unit: json_get_string(dimensions, "unit"),
      ))
    None -> None
  }
}

@internal
pub fn json_get(
  value: commit.JsonValue,
  key: String,
) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn json_insert_object_field(
  value: commit.JsonValue,
  key: String,
  inserted: commit.JsonValue,
) -> commit.JsonValue {
  case value {
    commit.JsonObject(fields) ->
      commit.JsonObject([
        #(key, inserted),
        ..list.filter(fields, fn(pair) { pair.0 != key })
      ])
    _ -> value
  }
}

@internal
pub fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
  case value {
    commit.JsonNull -> CapturedNull
    commit.JsonBool(value) -> CapturedBool(value)
    commit.JsonInt(value) -> CapturedInt(value)
    commit.JsonFloat(value) -> CapturedFloat(value)
    commit.JsonString(value) -> CapturedString(value)
    commit.JsonArray(items) ->
      CapturedArray(list.map(items, captured_json_from_commit))
    commit.JsonObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_json_from_commit(pair.1))
        }),
      )
  }
}

@internal
pub fn store_property_value_from_commit(
  value: commit.JsonValue,
) -> StorePropertyValue {
  case value {
    commit.JsonNull -> StorePropertyNull
    commit.JsonBool(value) -> StorePropertyBool(value)
    commit.JsonInt(value) -> StorePropertyInt(value)
    commit.JsonFloat(value) -> StorePropertyFloat(value)
    commit.JsonString(value) -> StorePropertyString(value)
    commit.JsonArray(items) ->
      StorePropertyList(list.map(items, store_property_value_from_commit))
    commit.JsonObject(fields) ->
      StorePropertyObject(
        fields
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_from_commit(pair.1))
        })
        |> dict.from_list,
      )
  }
}

@internal
pub fn handle_query_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let initial = #([], [])
  let #(data_entries, errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, all_errors) = acc
      case field {
        Field(name: name, ..) -> {
          let result = case name.value {
            "availableCarrierServices" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_available_carrier_services(
                  store,
                  field,
                  fragments,
                ),
                errors: [],
              )
            "locationsAvailableForDeliveryProfilesConnection" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_locations_available_for_delivery_profiles(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            "carrierService" ->
              handle_carrier_service_query(store, field, fragments, variables)
            "deliveryProfile" ->
              handle_delivery_profile_query(store, field, fragments, variables)
            "deliveryProfiles" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_delivery_profiles_connection(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            "fulfillmentService" ->
              handle_fulfillment_service_query(
                store,
                field,
                fragments,
                variables,
              )
            "fulfillment" ->
              handle_fulfillment_query(store, field, fragments, variables)
            "fulfillmentOrder" ->
              handle_fulfillment_order_query(store, field, fragments, variables)
            "fulfillmentOrders" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_fulfillment_orders_connection(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            "reverseDelivery" ->
              handle_reverse_delivery_query(store, field, fragments, variables)
            "reverseFulfillmentOrder" ->
              handle_reverse_fulfillment_order_query(
                store,
                field,
                fragments,
                variables,
              )
            "assignedFulfillmentOrders" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_assigned_fulfillment_orders_connection(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            "manualHoldsFulfillmentOrders" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_manual_holds_fulfillment_orders_connection(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            "location" ->
              handle_location_query(store, field, fragments, variables)
            "order" ->
              handle_shipping_order_query(store, field, fragments, variables)
            "carrierServices" ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_carrier_services_connection(
                  store,
                  field,
                  fragments,
                  variables,
                ),
                errors: [],
              )
            _ ->
              shipping_types.QueryFieldResult(
                key: get_field_response_key(field),
                value: json.null(),
                errors: [],
              )
          }
          #(
            list.append(entries, [#(result.key, result.value)]),
            list.append(all_errors, result.errors),
          )
        }
        _ -> acc
      }
    })
  let data = json.object(data_entries)
  case errors {
    [] -> graphql_helpers.wrap_data(data)
    _ ->
      json.object([
        #("errors", json.array(errors, fn(error) { error })),
        #("data", data),
      ])
  }
}

@internal
pub fn serialize_available_carrier_services(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let locations =
    store.list_effective_store_property_locations(store)
    |> filter_active_non_fulfillment_locations
    |> sort_store_property_locations_by_id
  let services =
    store.list_effective_carrier_services(store)
    |> list.filter(fn(service) { service.active })

  json.array(services, fn(service) {
    project_available_carrier_service_pair(service, locations, field, fragments)
  })
}

@internal
pub fn project_available_carrier_service_pair(
  service: CarrierServiceRecord,
  locations: List(StorePropertyRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("DeliveryCarrierServiceAndLocations")),
      #("carrierService", carrier_service_source(service)),
      #("locations", SrcList(list.map(locations, store_property_record_source))),
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
pub fn serialize_locations_available_for_delivery_profiles(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let locations =
    store.list_effective_store_property_locations(store)
    |> list.filter(is_active_location)
    |> sort_store_property_locations_by_id
  let ordered_locations = case
    read_bool(resolved_args(field, variables), "reverse")
  {
    Some(True) -> list.reverse(locations)
    _ -> locations
  }
  let window =
    paginate_connection_items(
      ordered_locations,
      field,
      variables,
      fn(location, _index) { location.id },
      default_connection_window_options(),
    )
  serialize_store_property_location_connection(field, window, fragments)
}

@internal
pub fn serialize_store_property_location_connection(
  field: Selection,
  window: ConnectionWindow(StorePropertyRecord),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(location, _index) { location.id },
      serialize_node: fn(location, selection, _index) {
        project_store_property_record(location, selection, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn handle_location_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case
          store.get_effective_store_property_location_by_id(store, id)
        {
          Some(location) ->
            project_store_property_record(location, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_carrier_service_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_carrier_service_by_id(store, id) {
          Some(service) -> project_carrier_service(service, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_delivery_profile_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_delivery_profile_by_id(store, id) {
          Some(profile) -> project_delivery_profile(profile, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_fulfillment_service_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_service_by_id(store, id) {
          Some(service) ->
            project_fulfillment_service(store, service, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_fulfillment_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_by_id(store, id) {
          Some(fulfillment) ->
            project_fulfillment(fulfillment, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_fulfillment_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_order_by_id(store, id) {
          Some(fulfillment_order) ->
            project_fulfillment_order(fulfillment_order, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_reverse_delivery_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_reverse_delivery_by_id(store, id) {
          Some(reverse_delivery) ->
            project_reverse_delivery(store, reverse_delivery, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_reverse_fulfillment_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case
          store.get_effective_reverse_fulfillment_order_by_id(store, id)
        {
          Some(reverse_fulfillment_order) ->
            project_reverse_fulfillment_order(
              store,
              reverse_fulfillment_order,
              field,
              fragments,
            )
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

@internal
pub fn handle_shipping_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> shipping_types.QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      shipping_types.QueryFieldResult(
        key: key,
        value: case store.get_effective_shipping_order_by_id(store, id) {
          Some(order) -> project_shipping_order(store, order, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None ->
      shipping_types.QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

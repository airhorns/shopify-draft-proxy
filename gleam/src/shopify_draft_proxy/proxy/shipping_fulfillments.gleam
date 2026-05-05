//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  ConnectionPageInfoOptions, SerializeConnectionConfig, SrcBool, SrcFloat,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store, Staged}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CalculatedOrderRecord, type CapturedJsonValue, type CarrierServiceRecord,
  type DeliveryProfileRecord, type FulfillmentOrderRecord,
  type FulfillmentRecord, type FulfillmentServiceRecord, type ProductRecord,
  type ProductVariantRecord, type ReverseDeliveryRecord,
  type ReverseFulfillmentOrderRecord, type ShippingOrderRecord,
  type ShippingPackageDimensionsRecord, type ShippingPackageRecord,
  type ShippingPackageWeightRecord, type StorePropertyRecord,
  type StorePropertyValue, CalculatedOrderRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CarrierServiceRecord, DeliveryProfileRecord, FulfillmentOrderRecord,
  FulfillmentRecord, FulfillmentServiceRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ReverseDeliveryRecord, ReverseFulfillmentOrderRecord,
  ShippingOrderRecord, ShippingPackageDimensionsRecord, ShippingPackageRecord,
  ShippingPackageWeightRecord, StorePropertyBool, StorePropertyFloat,
  StorePropertyInt, StorePropertyList, StorePropertyNull, StorePropertyObject,
  StorePropertyRecord, StorePropertyString,
}

pub type ShippingFulfillmentsError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    errors: List(Json),
    staged_resource_ids: List(String),
  )
}

type QueryFieldResult {
  QueryFieldResult(key: String, value: Json, errors: List(Json))
}

type CarrierServiceUserError {
  CarrierServiceUserError(field: Option(List(String)), message: String)
}

type LocalPickupUserError {
  LocalPickupUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

type FulfillmentServiceUserError {
  FulfillmentServiceUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

type DeliveryProfileUserError {
  DeliveryProfileUserError(field: Option(List(String)), message: String)
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
      Ok(handle_query_fields(store, fields, fragments, variables))
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

fn should_fetch_upstream_in_live_hybrid(
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

fn local_shipping_resource_staged(store_in: Store, id: String) -> Bool {
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

fn local_has_shipping_order_id(
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

fn has_local_shipping_query_state(
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

fn has_staged_shipping_query_state(store_in: Store) -> Bool {
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

fn fetch_and_hydrate_live_hybrid_query(
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

fn local_query_response(
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

fn variables_to_json(
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    dict.to_list(variables)
    |> list.map(fn(pair) {
      #(pair.0, root_field.resolved_value_to_json(pair.1))
    }),
  )
}

fn fetch_error_message(error: upstream_query.FetchError) -> String {
  case error {
    upstream_query.TransportFailed(message) -> message
    upstream_query.HttpStatusError(status, body) ->
      "upstream returned HTTP " <> int.to_string(status) <> ": " <> body
    upstream_query.MalformedResponse(message) -> message
    upstream_query.NoTransportInstalled -> "no upstream transport installed"
  }
}

fn hydrate_from_upstream_response(
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

fn hydrate_shipping_order_roots(
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

fn hydrate_delivery_profile_roots(
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

fn hydrate_fulfillment_roots(store_in: Store, data: commit.JsonValue) -> Store {
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
  |> store.upsert_base_fulfillments(fulfillments)
  |> store.upsert_base_fulfillment_orders(fulfillment_orders)
}

fn hydrate_product_variant_nodes(
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

fn hydrate_shipping_package_response(
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

fn hydrate_store_property_locations(
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

fn hydrate_available_carrier_services(
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

fn fulfillments_from_order(data: commit.JsonValue) -> List(commit.JsonValue) {
  case json_get(data, "order") {
    Some(order) -> json_array(json_get(order, "fulfillments"))
    None -> []
  }
}

fn fulfillment_orders_from_order(
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

fn nested_order_roots(data: commit.JsonValue) -> List(commit.JsonValue) {
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

fn location_nodes_from_available_carrier_services(
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

fn delivery_profile_record_from_json(
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

fn fulfillment_record_from_json(
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

fn shipping_order_record_from_json(
  value: commit.JsonValue,
) -> Result(ShippingOrderRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(ShippingOrderRecord(id: id, data: captured_json_from_commit(value)))
}

fn fulfillment_order_record_from_json(
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
    assignment_status: None,
    manually_held: !list.is_empty(captured_array_field(
      captured_json_from_commit(value),
      "fulfillmentHolds",
      "nodes",
    )),
    data: captured_json_from_commit(value),
  ))
}

fn product_record_from_variant_node(
  value: commit.JsonValue,
) -> Result(ProductRecord, Nil) {
  use product <- result.try(json_get(value, "product") |> option.to_result(Nil))
  use id <- result.try(json_get_string(product, "id") |> option.to_result(Nil))
  let title = json_get_string(product, "title") |> option.unwrap("")
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: json_get_string(product, "handle") |> option.unwrap(""),
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
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
  ))
}

fn product_variant_record_from_json(
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

fn shipping_package_record_from_json(
  value: commit.JsonValue,
) -> Result(ShippingPackageRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(ShippingPackageRecord(
    id: id,
    name: json_get_string(value, "name"),
    type_: json_get_string(value, "type"),
    default: json_get_bool(value, "default") |> option.unwrap(False),
    weight: json_get_weight(value, "weight"),
    dimensions: json_get_dimensions(value, "dimensions"),
    created_at: json_get_string(value, "createdAt") |> option.unwrap(""),
    updated_at: json_get_string(value, "updatedAt") |> option.unwrap(""),
  ))
}

fn assigned_location_id_from_json(value: commit.JsonValue) -> Option(String) {
  case json_get(value, "assignedLocation") {
    Some(assigned) ->
      case json_get(assigned, "location") {
        Some(location) -> json_get_string(location, "id")
        None -> None
      }
    None -> None
  }
}

fn carrier_service_record_from_json(
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

fn store_property_location_from_json(
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

fn nodes_from_connection(
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

fn edge_nodes_from_connection(
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

fn non_null_json(value: commit.JsonValue) -> Option(commit.JsonValue) {
  case value {
    commit.JsonNull -> None
    _ -> Some(value)
  }
}

fn option_to_list(value: Option(a)) -> List(a) {
  case value {
    Some(item) -> [item]
    None -> []
  }
}

fn non_null_json_values(
  values: List(commit.JsonValue),
) -> List(commit.JsonValue) {
  list.filter(values, fn(value) {
    case value {
      commit.JsonNull -> False
      _ -> True
    }
  })
}

fn json_array(value: Option(commit.JsonValue)) -> List(commit.JsonValue) {
  case value {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

fn json_get_number(value: commit.JsonValue, key: String) -> Option(Float) {
  case json_get(value, key) {
    Some(commit.JsonFloat(n)) -> Some(n)
    Some(commit.JsonInt(n)) -> Some(int.to_float(n))
    _ -> None
  }
}

fn json_get_weight(
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

fn json_get_dimensions(
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

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
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

fn json_insert_object_field(
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

fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
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

fn store_property_value_from_commit(
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

fn handle_query_fields(
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
              QueryFieldResult(
                key: get_field_response_key(field),
                value: serialize_available_carrier_services(
                  store,
                  field,
                  fragments,
                ),
                errors: [],
              )
            "locationsAvailableForDeliveryProfilesConnection" ->
              QueryFieldResult(
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
              QueryFieldResult(
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
              QueryFieldResult(
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
              QueryFieldResult(
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
              QueryFieldResult(
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
              QueryFieldResult(
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
              QueryFieldResult(
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

fn serialize_available_carrier_services(
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

fn project_available_carrier_service_pair(
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

fn serialize_locations_available_for_delivery_profiles(
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

fn serialize_store_property_location_connection(
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

fn handle_location_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
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
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_carrier_service_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_carrier_service_by_id(store, id) {
          Some(service) -> project_carrier_service(service, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_delivery_profile_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_delivery_profile_by_id(store, id) {
          Some(profile) -> project_delivery_profile(profile, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_fulfillment_service_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_service_by_id(store, id) {
          Some(service) ->
            project_fulfillment_service(store, service, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_fulfillment_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_by_id(store, id) {
          Some(fulfillment) ->
            project_fulfillment(fulfillment, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_fulfillment_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_fulfillment_order_by_id(store, id) {
          Some(fulfillment_order) ->
            project_fulfillment_order(fulfillment_order, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_reverse_delivery_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_reverse_delivery_by_id(store, id) {
          Some(reverse_delivery) ->
            project_reverse_delivery(store, reverse_delivery, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_reverse_fulfillment_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
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
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn handle_shipping_order_query(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = resolved_args(field, variables)
  let key = get_field_response_key(field)
  case read_string(args, "id") {
    Some(id) ->
      QueryFieldResult(
        key: key,
        value: case store.get_effective_shipping_order_by_id(store, id) {
          Some(order) -> project_shipping_order(store, order, field, fragments)
          None -> json.null()
        },
        errors: [],
      )
    None -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, ShippingFulfillmentsError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, ShippingFulfillmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        upstream,
      ))
    }
  }
}

fn handle_mutation_fields(
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
                  Staged,
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

fn hydrate_mutation_prerequisites(
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
      let variant_ids = case read_object(args, "profile") {
        Some(profile) -> read_string_array(profile, "variantsToAssociate")
        None -> []
      }
      maybe_hydrate_delivery_profile_variants(store_in, variant_ids, upstream)
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

fn hydrate_fulfillment_order_ids(
  store_in: Store,
  ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  list.fold(ids, store_in, fn(current, id) {
    maybe_hydrate_fulfillment_order(current, Some(id), upstream)
  })
}

fn maybe_hydrate_fulfillment_order(
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
    id status requestStatus fulfillAt fulfillBy updatedAt
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

fn maybe_hydrate_delivery_profile(
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

fn maybe_hydrate_delivery_profile_variants(
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

fn maybe_hydrate_shipping_package(
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
  shippingPackage(id: $id) { id name type default weight { value unit } dimensions { length width height unit } createdAt updatedAt }
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

fn handle_carrier_service_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "input") |> option.unwrap(dict.new())
  let name = read_trimmed_string(input, "name")
  let user_errors = validate_carrier_service_name(name)
  case user_errors, name {
    [], Some(valid_name) -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_proxy_synthetic_gid(
          identity,
          "DeliveryCarrierService",
        )
      let #(now, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let service =
        CarrierServiceRecord(
          id: id,
          name: Some(valid_name),
          formatted_name: carrier_service_formatted_name(Some(valid_name)),
          callback_url: read_carrier_service_callback_url(input),
          active: read_bool(input, "active") |> option.unwrap(False),
          supports_service_discovery: read_bool(
            input,
            "supportsServiceDiscovery",
          )
            |> option.unwrap(False),
          created_at: now,
          updated_at: now,
        )
      let #(staged, next_store) =
        store.stage_create_carrier_service(draft_store, service)
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: carrier_service_payload_json(
            field,
            fragments,
            "CarrierServiceCreatePayload",
            Some(staged),
            [],
          ),
          errors: [],
          staged_resource_ids: [id],
        ),
        next_store,
        next_identity,
      )
    }
    _, _ -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: carrier_service_payload_json(
          field,
          fragments,
          "CarrierServiceCreatePayload",
          None,
          user_errors,
        ),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_carrier_service_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "input") |> option.unwrap(dict.new())
  case read_string(input, "id") {
    Some(id) ->
      case store.get_effective_carrier_service_by_id(draft_store, id) {
        Some(existing) ->
          update_existing_carrier_service(
            draft_store,
            identity,
            field,
            fragments,
            input,
            existing,
          )
        None ->
          carrier_service_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            "CarrierServiceUpdatePayload",
            [carrier_service_not_found_for_update()],
          )
      }
    None ->
      carrier_service_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "CarrierServiceUpdatePayload",
        [carrier_service_not_found_for_update()],
      )
  }
}

fn update_existing_carrier_service(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  input: Dict(String, root_field.ResolvedValue),
  existing: CarrierServiceRecord,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let next_name = case read_trimmed_string(input, "name") {
    Some(value) -> Some(value)
    None -> existing.name
  }
  let user_errors = validate_carrier_service_name(next_name)
  case user_errors, next_name {
    [], Some(valid_name) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let updated =
        CarrierServiceRecord(
          ..existing,
          name: Some(valid_name),
          formatted_name: carrier_service_formatted_name(Some(valid_name)),
          callback_url: case dict.has_key(input, "callbackUrl") {
            True -> read_carrier_service_callback_url(input)
            False -> existing.callback_url
          },
          active: read_bool(input, "active") |> option.unwrap(existing.active),
          supports_service_discovery: read_bool(
              input,
              "supportsServiceDiscovery",
            )
            |> option.unwrap(existing.supports_service_discovery),
          updated_at: updated_at,
        )
      let #(staged, next_store) =
        store.stage_update_carrier_service(draft_store, updated)
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: carrier_service_payload_json(
            field,
            fragments,
            "CarrierServiceUpdatePayload",
            Some(staged),
            [],
          ),
          errors: [],
          staged_resource_ids: [staged.id],
        ),
        next_store,
        next_identity,
      )
    }
    _, _ ->
      carrier_service_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "CarrierServiceUpdatePayload",
        user_errors,
      )
  }
}

fn handle_carrier_service_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_carrier_service_by_id(draft_store, id) {
        Some(_) -> {
          let next_store = store.delete_staged_carrier_service(draft_store, id)
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: carrier_service_delete_payload_json(
                field,
                fragments,
                Some(id),
                [],
              ),
              errors: [],
              staged_resource_ids: [id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          carrier_service_delete_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            [carrier_service_not_found_for_delete()],
          )
      }
    None ->
      carrier_service_delete_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        [carrier_service_not_found_for_delete()],
      )
  }
}

fn carrier_service_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(CarrierServiceUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: carrier_service_payload_json(
        field,
        fragments,
        payload_typename,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn carrier_service_delete_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(CarrierServiceUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: carrier_service_delete_payload_json(
        field,
        fragments,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn handle_delivery_profile_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "profile")
  case input {
    Some(profile_input) -> {
      case read_trimmed_string(profile_input, "name") {
        Some(name) if name != "" -> {
          let #(profile, next_identity) =
            make_delivery_profile(draft_store, identity, profile_input, name)
          let #(staged, next_store) =
            store.stage_create_delivery_profile(draft_store, profile)
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: delivery_profile_payload_json(
                field,
                fragments,
                "DeliveryProfileCreatePayload",
                Some(staged),
                [],
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            next_identity,
          )
        }
        _ ->
          delivery_profile_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            "DeliveryProfileCreatePayload",
            [blank_delivery_profile_name_error()],
          )
      }
    }
    None ->
      delivery_profile_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "DeliveryProfileCreatePayload",
        [blank_delivery_profile_name_error()],
      )
  }
}

fn handle_delivery_profile_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "profile")
  let existing = case read_string(args, "id") {
    Some(id) -> store.get_effective_delivery_profile_by_id(draft_store, id)
    None -> None
  }
  case existing, input {
    Some(profile), Some(profile_input) -> {
      case read_string(profile_input, "name") {
        Some("") ->
          delivery_profile_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            "DeliveryProfileUpdatePayload",
            [blank_delivery_profile_name_error()],
          )
        _ -> {
          let #(updated, next_identity) =
            update_delivery_profile(
              draft_store,
              identity,
              profile,
              profile_input,
            )
          let #(staged, next_store) =
            store.stage_update_delivery_profile(draft_store, updated)
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: delivery_profile_payload_json(
                field,
                fragments,
                "DeliveryProfileUpdatePayload",
                Some(staged),
                [],
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            next_identity,
          )
        }
      }
    }
    _, _ ->
      delivery_profile_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "DeliveryProfileUpdatePayload",
        [delivery_profile_update_not_found()],
      )
  }
}

fn handle_delivery_profile_remove(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) -> {
      case store.get_effective_delivery_profile_by_id(draft_store, id) {
        Some(profile) -> {
          case captured_bool_field(profile.data, "default") {
            Some(True) ->
              delivery_profile_remove_validation_result(
                draft_store,
                identity,
                field,
                fragments,
                [delivery_profile_default_remove_error()],
              )
            _ -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              let next_store =
                store.delete_staged_delivery_profile(draft_store, id)
              #(
                MutationFieldResult(
                  key: get_field_response_key(field),
                  payload: delivery_profile_remove_payload_json(
                    field,
                    fragments,
                    Some(#(job_id, False)),
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: [id, job_id],
                ),
                next_store,
                next_identity,
              )
            }
          }
        }
        None ->
          delivery_profile_remove_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            [delivery_profile_remove_not_found()],
          )
      }
    }
    None ->
      delivery_profile_remove_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        [delivery_profile_remove_not_found()],
      )
  }
}

fn delivery_profile_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(DeliveryProfileUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: delivery_profile_payload_json(
        field,
        fragments,
        payload_typename,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn delivery_profile_remove_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(DeliveryProfileUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: delivery_profile_remove_payload_json(
        field,
        fragments,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn handle_fulfillment_service_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let name = read_trimmed_string(args, "name")
  let callback_url = read_fulfillment_service_callback_url(args)
  let user_errors =
    list.append(
      validate_fulfillment_service_name(name),
      validate_fulfillment_service_callback_url(callback_url),
    )
  case user_errors, name {
    [], Some(valid_name) -> {
      let #(location_id, identity_after_location) =
        synthetic_identity.make_proxy_synthetic_gid(identity, "Location")
      let #(id, identity_after_service) =
        synthetic_identity.make_proxy_synthetic_gid(
          identity_after_location,
          "FulfillmentService",
        )
      let #(now, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_service)
      let service =
        FulfillmentServiceRecord(
          id: id,
          handle: normalize_fulfillment_service_handle(valid_name),
          service_name: valid_name,
          callback_url: callback_url,
          inventory_management: read_bool(args, "inventoryManagement")
            |> option.unwrap(False),
          location_id: Some(location_id),
          requires_shipping_method: read_bool(args, "requiresShippingMethod")
            |> option.unwrap(True),
          tracking_support: read_bool(args, "trackingSupport")
            |> option.unwrap(False),
          type_: "THIRD_PARTY",
        )
      let #(staged_service, service_store) =
        store.stage_create_fulfillment_service(draft_store, service)
      let location = fulfillment_service_location_record(staged_service, now)
      let #(_, next_store) =
        store.upsert_staged_store_property_location(service_store, location)
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: fulfillment_service_payload_json(
            next_store,
            field,
            fragments,
            "FulfillmentServiceCreatePayload",
            Some(staged_service),
            [],
          ),
          errors: [],
          staged_resource_ids: [id, location_id],
        ),
        next_store,
        next_identity,
      )
    }
    _, _ -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: fulfillment_service_payload_json(
          draft_store,
          field,
          fragments,
          "FulfillmentServiceCreatePayload",
          None,
          user_errors,
        ),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_fulfillment_service_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_service_by_id(draft_store, id) {
        Some(existing) ->
          update_existing_fulfillment_service(
            draft_store,
            identity,
            field,
            fragments,
            args,
            existing,
          )
        None ->
          fulfillment_service_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentServiceUpdatePayload",
            [fulfillment_service_not_found()],
          )
      }
    None ->
      fulfillment_service_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentServiceUpdatePayload",
        [fulfillment_service_not_found()],
      )
  }
}

fn update_existing_fulfillment_service(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, root_field.ResolvedValue),
  existing: FulfillmentServiceRecord,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let next_name = case read_trimmed_string(args, "name") {
    Some(value) -> Some(value)
    None -> Some(existing.service_name)
  }
  let callback_url = case dict.has_key(args, "callbackUrl") {
    True -> read_fulfillment_service_callback_url(args)
    False -> existing.callback_url
  }
  let user_errors =
    list.append(
      validate_fulfillment_service_name(next_name),
      validate_fulfillment_service_callback_url(callback_url),
    )
  case user_errors, next_name {
    [], Some(valid_name) -> {
      let updated =
        FulfillmentServiceRecord(
          ..existing,
          service_name: valid_name,
          callback_url: callback_url,
          inventory_management: read_bool(args, "inventoryManagement")
            |> option.unwrap(existing.inventory_management),
          requires_shipping_method: read_bool(args, "requiresShippingMethod")
            |> option.unwrap(existing.requires_shipping_method),
          tracking_support: read_bool(args, "trackingSupport")
            |> option.unwrap(existing.tracking_support),
        )
      let #(now, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(staged_service, service_store) =
        store.stage_update_fulfillment_service(draft_store, updated)
      let next_store = case staged_service.location_id {
        Some(_) -> {
          let location =
            fulfillment_service_location_record(staged_service, now)
          let #(_, staged_store) =
            store.upsert_staged_store_property_location(service_store, location)
          staged_store
        }
        None -> service_store
      }
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: fulfillment_service_payload_json(
            next_store,
            field,
            fragments,
            "FulfillmentServiceUpdatePayload",
            Some(staged_service),
            [],
          ),
          errors: [],
          staged_resource_ids: [staged_service.id],
        ),
        next_store,
        next_identity,
      )
    }
    _, _ ->
      fulfillment_service_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentServiceUpdatePayload",
        user_errors,
      )
  }
}

fn handle_fulfillment_service_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_service_by_id(draft_store, id) {
        Some(existing) -> {
          let inventory_action = read_fulfillment_service_delete_action(args)
          case
            fulfillment_service_delete_destination(
              draft_store,
              inventory_action,
              read_string(args, "destinationLocationId"),
            )
          {
            Ok(destination) -> {
              let service_store =
                store.delete_staged_fulfillment_service(draft_store, id)
              let #(next_store, affected_order_ids) =
                stage_fulfillment_service_delete_effects(
                  service_store,
                  existing,
                  inventory_action,
                  destination,
                )
              #(
                MutationFieldResult(
                  key: get_field_response_key(field),
                  payload: fulfillment_service_delete_payload_json(
                    field,
                    fragments,
                    Some(strip_query_from_gid(id)),
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: list.append([id], affected_order_ids),
                ),
                next_store,
                identity,
              )
            }
            Error(user_errors) ->
              fulfillment_service_delete_validation_result(
                draft_store,
                identity,
                field,
                fragments,
                user_errors,
              )
          }
        }
        None ->
          fulfillment_service_delete_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            [fulfillment_service_not_found()],
          )
      }
    None ->
      fulfillment_service_delete_validation_result(
        draft_store,
        identity,
        field,
        fragments,
        [fulfillment_service_not_found()],
      )
  }
}

fn read_fulfillment_service_delete_action(
  args: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_string(args, "inventoryAction") {
    Some("DELETE") -> "DELETE"
    Some("KEEP") -> "KEEP"
    Some("TRANSFER") -> "TRANSFER"
    _ -> "TRANSFER"
  }
}

fn fulfillment_service_delete_destination(
  draft_store: Store,
  inventory_action: String,
  destination_location_id: Option(String),
) -> Result(Option(StorePropertyRecord), List(FulfillmentServiceUserError)) {
  case inventory_action {
    "TRANSFER" ->
      case
        find_active_merchant_managed_location(
          draft_store,
          destination_location_id,
        )
      {
        Some(location) -> Ok(Some(location))
        None -> Error([invalid_fulfillment_service_destination_location()])
      }
    _ -> Ok(None)
  }
}

fn find_active_merchant_managed_location(
  draft_store: Store,
  location_id: Option(String),
) -> Option(StorePropertyRecord) {
  case location_id {
    Some(id) ->
      case store.get_effective_store_property_location_by_id(draft_store, id) {
        Some(location) ->
          case
            is_active_location(location)
            && !is_fulfillment_service_location(location)
          {
            True -> Some(location)
            False -> None
          }
        None -> None
      }
    None -> None
  }
}

fn stage_fulfillment_service_delete_effects(
  draft_store: Store,
  service: FulfillmentServiceRecord,
  inventory_action: String,
  destination: Option(StorePropertyRecord),
) -> #(Store, List(String)) {
  case service.location_id {
    Some(location_id) -> {
      let location_store = case inventory_action {
        "KEEP" ->
          convert_fulfillment_service_location_to_merchant(
            draft_store,
            location_id,
          )
        _ ->
          store.delete_staged_store_property_location(draft_store, location_id)
      }
      case inventory_action, destination {
        "TRANSFER", Some(destination_location) ->
          reassign_fulfillment_orders_from_service_location(
            location_store,
            location_id,
            destination_location,
          )
        _, _ ->
          close_fulfillment_orders_at_service_location(
            location_store,
            location_id,
          )
      }
    }
    None -> #(draft_store, [])
  }
}

fn convert_fulfillment_service_location_to_merchant(
  draft_store: Store,
  location_id: String,
) -> Store {
  case
    store.get_effective_store_property_location_by_id(draft_store, location_id)
  {
    Some(location) -> {
      let converted =
        StorePropertyRecord(
          ..location,
          data: location.data
            |> dict.insert("isFulfillmentService", StorePropertyBool(False))
            |> dict.insert("fulfillmentService", StorePropertyNull)
            |> dict.insert("shipsInventory", StorePropertyBool(True))
            |> dict.insert(
              "updatedAt",
              StorePropertyString(synthetic_timestamp_string()),
            ),
        )
      let #(_, next_store) =
        store.upsert_staged_store_property_location(draft_store, converted)
      next_store
    }
    None -> draft_store
  }
}

fn reassign_fulfillment_orders_from_service_location(
  draft_store: Store,
  source_location_id: String,
  destination: StorePropertyRecord,
) -> #(Store, List(String)) {
  store.list_effective_fulfillment_orders(draft_store)
  |> list.filter(fn(order) {
    fulfillment_order_is_open(order)
    && fulfillment_order_assigned_to_location(order, source_location_id)
  })
  |> list.fold(#(draft_store, []), fn(acc, order) {
    let #(current_store, staged_ids) = acc
    let reassigned =
      update_fulfillment_order_fields(order, [
        #(
          "assignedLocation",
          fulfillment_order_assigned_location_value(destination),
        ),
        #("updatedAt", CapturedString(synthetic_timestamp_string())),
      ])
    let reassigned =
      FulfillmentOrderRecord(
        ..reassigned,
        assigned_location_id: Some(destination.id),
      )
    let #(_, next_store) =
      store.stage_upsert_fulfillment_order(current_store, reassigned)
    #(next_store, list.append(staged_ids, [order.id]))
  })
}

fn close_fulfillment_orders_at_service_location(
  draft_store: Store,
  source_location_id: String,
) -> #(Store, List(String)) {
  store.list_effective_fulfillment_orders(draft_store)
  |> list.filter(fn(order) {
    fulfillment_order_is_open(order)
    && fulfillment_order_assigned_to_location(order, source_location_id)
  })
  |> list.fold(#(draft_store, []), fn(acc, order) {
    let #(current_store, staged_ids) = acc
    let closed =
      update_fulfillment_order_fields(order, [
        #("status", CapturedString("CLOSED")),
        #("updatedAt", CapturedString(synthetic_timestamp_string())),
        #("supportedActions", CapturedArray([])),
      ])
    let closed = FulfillmentOrderRecord(..closed, status: "CLOSED")
    let #(_, next_store) =
      store.stage_upsert_fulfillment_order(current_store, closed)
    #(next_store, list.append(staged_ids, [order.id]))
  })
}

fn fulfillment_order_is_open(order: FulfillmentOrderRecord) -> Bool {
  order.status != "CLOSED"
}

fn fulfillment_order_assigned_to_location(
  order: FulfillmentOrderRecord,
  location_id: String,
) -> Bool {
  order.assigned_location_id == Some(location_id)
}

fn fulfillment_order_assigned_location_value(
  location: StorePropertyRecord,
) -> CapturedJsonValue {
  let name = store_property_string_field(location, "name") |> option.unwrap("")
  CapturedObject([
    #("name", CapturedString(name)),
    #(
      "location",
      CapturedObject([
        #("id", CapturedString(location.id)),
        #("name", CapturedString(name)),
      ]),
    ),
  ])
}

fn handle_fulfillment_order_submit_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let message = read_string(args, "message")
          let request_options =
            CapturedObject([
              #("notify_customer", case read_bool(args, "notifyCustomer") {
                Some(value) -> CapturedBool(value)
                None -> CapturedNull
              }),
            ])
          let request =
            fulfillment_order_merchant_request(
              "FULFILLMENT_REQUEST",
              message,
              request_options,
            )
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("OPEN")),
              #("requestStatus", CapturedString("SUBMITTED")),
              #("merchantRequests", captured_connection([request])),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: "OPEN",
              request_status: "SUBMITTED",
              assignment_status: Some("FULFILLMENT_REQUESTED"),
            )
          let #(staged, next_store) =
            store.stage_upsert_fulfillment_order(draft_store, updated)
          let unsubmitted =
            find_unsubmitted_sibling_fulfillment_order(next_store, staged)
          #(
            MutationFieldResult(
              key: key,
              payload: fulfillment_order_payload_json(field, fragments, [
                #(
                  "__typename",
                  SrcString("FulfillmentOrderSubmitFulfillmentRequestPayload"),
                ),
                #("originalFulfillmentOrder", fulfillment_order_source(staged)),
                #("submittedFulfillmentOrder", fulfillment_order_source(staged)),
                #(
                  "unsubmittedFulfillmentOrder",
                  optional_fulfillment_order_source(unsubmitted),
                ),
                #("userErrors", SrcList([])),
              ]),
              errors: [],
              staged_resource_ids: [id],
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
            "FulfillmentOrderSubmitFulfillmentRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderSubmitFulfillmentRequestPayload",
      )
  }
}

fn handle_fulfillment_order_request_status_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_typename: String,
  request_status: String,
  status: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString(status)),
              #("requestStatus", CapturedString(request_status)),
            ])
          let assignment_status = case request_status {
            "ACCEPTED" -> Some("FULFILLMENT_ACCEPTED")
            "CANCELLATION_REJECTED" -> Some("FULFILLMENT_ACCEPTED")
            _ -> None
          }
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: status,
              request_status: request_status,
              assignment_status: assignment_status,
            )
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

fn handle_fulfillment_order_submit_cancellation_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let existing_requests =
            captured_array_field(order.data, "merchantRequests", "nodes")
          let request =
            fulfillment_order_merchant_request(
              "CANCELLATION_REQUEST",
              read_string(args, "message"),
              CapturedObject([]),
            )
          let updated =
            update_fulfillment_order_fields(order, [
              #(
                "merchantRequests",
                captured_connection(list.append(existing_requests, [request])),
              ),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              assignment_status: Some("CANCELLATION_REQUESTED"),
            )
          fulfillment_order_single_payload_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSubmitCancellationRequestPayload",
            updated,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSubmitCancellationRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderSubmitCancellationRequestPayload",
      )
  }
}

fn handle_fulfillment_order_accept_cancellation_request(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("CLOSED")),
              #("requestStatus", CapturedString("CANCELLATION_ACCEPTED")),
              #(
                "lineItems",
                zero_fulfillment_order_line_items(order.data, None),
              ),
            ])
          let updated =
            FulfillmentOrderRecord(
              ..updated,
              status: "CLOSED",
              request_status: "CANCELLATION_ACCEPTED",
              assignment_status: None,
            )
          fulfillment_order_single_payload_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderAcceptCancellationRequestPayload",
            updated,
          )
        }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderAcceptCancellationRequestPayload",
          )
      }
    None ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderAcceptCancellationRequestPayload",
      )
  }
}

fn handle_fulfillment_event_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let input = read_object(args, "fulfillmentEvent") |> option.unwrap(dict.new())
  case read_string(input, "fulfillmentId") {
    Some(fulfillment_id) ->
      case store.get_effective_fulfillment_by_id(draft_store, fulfillment_id) {
        Some(fulfillment) -> {
          let #(event_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "FulfillmentEvent")
          let event = fulfillment_event_value(event_id, input)
          let updated = update_fulfillment_for_event(fulfillment, event, input)
          let #(staged, next_store) =
            store.stage_upsert_fulfillment(draft_store, updated)
          #(
            MutationFieldResult(
              key: key,
              payload: fulfillment_event_payload_json(
                field,
                fragments,
                Some(event),
                [],
              ),
              errors: [],
              staged_resource_ids: [staged.id, event_id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          fulfillment_event_missing_result(
            draft_store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      fulfillment_event_missing_result(draft_store, identity, field, fragments)
  }
}

fn handle_reverse_delivery_create_with_shipping(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "reverseFulfillmentOrderId") {
    Some(reverse_fulfillment_order_id) ->
      case
        store.get_effective_reverse_fulfillment_order_by_id(
          draft_store,
          reverse_fulfillment_order_id,
        )
      {
        Some(reverse_fulfillment_order) -> {
          let #(delivery_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "ReverseDelivery")
          let #(line_items, identity) =
            make_reverse_delivery_line_items(
              reverse_fulfillment_order,
              read_object_array(args, "reverseDeliveryLineItems"),
              identity,
            )
          let reverse_delivery =
            ReverseDeliveryRecord(
              id: delivery_id,
              reverse_fulfillment_order_id: reverse_fulfillment_order.id,
              data: reverse_delivery_value(delivery_id, args, line_items),
            )
          let updated_reverse_fulfillment_order =
            append_reverse_delivery(reverse_fulfillment_order, reverse_delivery)
          let #(_, next_store) =
            store.stage_upsert_reverse_fulfillment_order(
              draft_store,
              updated_reverse_fulfillment_order,
            )
          let #(staged_delivery, next_store) =
            store.stage_upsert_reverse_delivery(next_store, reverse_delivery)
          #(
            MutationFieldResult(
              key: key,
              payload: reverse_delivery_payload_json(
                next_store,
                field,
                fragments,
                "ReverseDeliveryCreateWithShippingPayload",
                Some(staged_delivery),
                [],
              ),
              errors: [],
              staged_resource_ids: [
                updated_reverse_fulfillment_order.id,
                staged_delivery.id,
              ],
            ),
            next_store,
            identity,
          )
        }
        None ->
          reverse_delivery_missing_rfo_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
          )
      }
    None ->
      reverse_delivery_missing_rfo_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
      )
  }
}

fn handle_reverse_delivery_shipping_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "reverseDeliveryId") {
    Some(reverse_delivery_id) ->
      case
        store.get_effective_reverse_delivery_by_id(
          draft_store,
          reverse_delivery_id,
        )
      {
        Some(reverse_delivery) -> {
          let updated =
            ReverseDeliveryRecord(
              ..reverse_delivery,
              data: update_reverse_delivery_shipping(
                reverse_delivery.data,
                args,
              ),
            )
          let #(staged, next_store) =
            store.stage_upsert_reverse_delivery(draft_store, updated)
          #(
            MutationFieldResult(
              key: key,
              payload: reverse_delivery_payload_json(
                next_store,
                field,
                fragments,
                "ReverseDeliveryShippingUpdatePayload",
                Some(staged),
                [],
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          reverse_delivery_missing_delivery_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
          )
      }
    None ->
      reverse_delivery_missing_delivery_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
      )
  }
}

fn handle_reverse_fulfillment_order_dispose(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let inputs = read_object_array(args, "dispositionInputs")
  let #(next_store, updated_line_items, user_errors, _) =
    list.fold(inputs, #(draft_store, [], [], 0), fn(acc, input) {
      let #(current_store, line_items, errors, index) = acc
      case read_string(input, "reverseFulfillmentOrderLineItemId") {
        Some(line_item_id) ->
          case
            find_reverse_fulfillment_order_line_item(
              current_store,
              line_item_id,
            )
          {
            Some(#(reverse_fulfillment_order, line_item)) -> {
              let quantity = read_int(input, "quantity") |> option.unwrap(0)
              let disposition_type =
                read_string(input, "dispositionType")
                |> option.unwrap("UNKNOWN")
              let updated_line_item =
                dispose_reverse_fulfillment_order_line_item(
                  line_item,
                  quantity,
                  disposition_type,
                )
              let updated_order =
                update_reverse_fulfillment_order_line_item(
                  reverse_fulfillment_order,
                  updated_line_item,
                )
              let #(_, updated_store) =
                store.stage_upsert_reverse_fulfillment_order(
                  current_store,
                  updated_order,
                )
              #(
                updated_store,
                list.append(line_items, [updated_line_item]),
                errors,
                index + 1,
              )
            }
            None -> #(
              current_store,
              line_items,
              list.append(errors, [
                plain_user_error_source(
                  [
                    "dispositionInputs",
                    int.to_string(index),
                    "reverseFulfillmentOrderLineItemId",
                  ],
                  "Reverse fulfillment order line item does not exist.",
                ),
              ]),
              index + 1,
            )
          }
        None -> #(
          current_store,
          line_items,
          list.append(errors, [
            plain_user_error_source(
              [
                "dispositionInputs",
                int.to_string(index),
                "reverseFulfillmentOrderLineItemId",
              ],
              "Reverse fulfillment order line item does not exist.",
            ),
          ]),
          index + 1,
        )
      }
    })
  #(
    MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #("__typename", SrcString("ReverseFulfillmentOrderDisposePayload")),
        #(
          "reverseFulfillmentOrderLineItems",
          SrcList(list.map(updated_line_items, captured_json_source)),
        ),
        #("userErrors", SrcList(user_errors)),
      ]),
      errors: [],
      staged_resource_ids: list.map(updated_line_items, reverse_line_item_id),
    ),
    next_store,
    identity,
  )
}

fn handle_order_edit_add_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(calculated_order_id) ->
      case
        store.get_effective_calculated_order_by_id(
          draft_store,
          calculated_order_id,
        )
      {
        Some(calculated_order) -> {
          let input =
            read_object(args, "shippingLine") |> option.unwrap(dict.new())
          case make_calculated_shipping_line(input, identity) {
            Some(#(shipping_line, identity)) -> {
              let updated =
                update_calculated_order_shipping_lines(
                  calculated_order,
                  list.append(
                    calculated_order_shipping_lines(calculated_order),
                    [shipping_line],
                  ),
                )
              let #(staged, next_store) =
                store.stage_upsert_calculated_order(draft_store, updated)
              #(
                MutationFieldResult(
                  key: key,
                  payload: order_edit_shipping_line_payload_json(
                    field,
                    fragments,
                    "OrderEditAddShippingLinePayload",
                    Some(staged),
                    Some(shipping_line),
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: [
                    staged.id,
                    reverse_line_item_id(shipping_line),
                  ],
                ),
                next_store,
                identity,
              )
            }
            None ->
              order_edit_shipping_line_invalid_result(
                draft_store,
                identity,
                field,
                fragments,
                key,
                "OrderEditAddShippingLinePayload",
              )
          }
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditAddShippingLinePayload",
          )
      }
    None ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditAddShippingLinePayload",
      )
  }
}

fn handle_order_edit_remove_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let calculated_order_id = read_string(args, "id")
  let shipping_line_id = read_string(args, "shippingLineId")
  case calculated_order_id, shipping_line_id {
    Some(id), Some(line_id) ->
      case store.get_effective_calculated_order_by_id(draft_store, id) {
        Some(calculated_order) -> {
          let existing = calculated_order_shipping_lines(calculated_order)
          let had_line =
            list.any(existing, fn(line) {
              reverse_line_item_id(line) == line_id
            })
          let updated =
            update_calculated_order_shipping_lines(
              calculated_order,
              list.filter(existing, fn(line) {
                reverse_line_item_id(line) != line_id
              }),
            )
          let #(staged, next_store) =
            store.stage_upsert_calculated_order(draft_store, updated)
          let errors = case had_line {
            True -> []
            False -> [
              plain_user_error_source(
                ["shippingLineId"],
                "Shipping line does not exist",
              ),
            ]
          }
          #(
            MutationFieldResult(
              key: key,
              payload: order_edit_shipping_line_payload_json(
                field,
                fragments,
                "OrderEditRemoveShippingLinePayload",
                Some(staged),
                None,
                errors,
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditRemoveShippingLinePayload",
          )
      }
    _, _ ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditRemoveShippingLinePayload",
      )
  }
}

fn handle_order_edit_update_shipping_line(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let calculated_order_id = read_string(args, "id")
  let shipping_line_id = read_string(args, "shippingLineId")
  let input = read_object(args, "shippingLine") |> option.unwrap(dict.new())
  case calculated_order_id, shipping_line_id {
    Some(id), Some(line_id) ->
      case store.get_effective_calculated_order_by_id(draft_store, id) {
        Some(calculated_order) -> {
          let existing = calculated_order_shipping_lines(calculated_order)
          let updated_lines =
            list.map(existing, fn(line) {
              case reverse_line_item_id(line) == line_id {
                True -> update_calculated_shipping_line(line, input)
                False -> line
              }
            })
          let updated_line =
            updated_lines
            |> list.find(fn(line) { reverse_line_item_id(line) == line_id })
            |> option.from_result
          let updated =
            update_calculated_order_shipping_lines(
              calculated_order,
              updated_lines,
            )
          let #(staged, next_store) =
            store.stage_upsert_calculated_order(draft_store, updated)
          let errors = case updated_line {
            Some(_) -> []
            None -> [
              plain_user_error_source(
                ["shippingLineId"],
                "Shipping line does not exist",
              ),
            ]
          }
          #(
            MutationFieldResult(
              key: key,
              payload: order_edit_shipping_line_payload_json(
                field,
                fragments,
                "OrderEditUpdateShippingLinePayload",
                Some(staged),
                updated_line,
                errors,
              ),
              errors: [],
              staged_resource_ids: [staged.id],
            ),
            next_store,
            identity,
          )
        }
        None ->
          order_edit_calculated_order_missing_result(
            draft_store,
            identity,
            field,
            fragments,
            key,
            "OrderEditUpdateShippingLinePayload",
          )
      }
    _, _ ->
      order_edit_calculated_order_missing_result(
        draft_store,
        identity,
        field,
        fragments,
        key,
        "OrderEditUpdateShippingLinePayload",
      )
  }
}

fn handle_fulfillment_order_hold(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let hold_input =
    read_object(args, "fulfillmentHold") |> option.unwrap(dict.new())
  let quantity =
    first_fulfillment_order_line_item_quantity(read_object_array(
      hold_input,
      "fulfillmentOrderLineItems",
    ))
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let #(hold_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "FulfillmentHold")
          let hold =
            fulfillment_hold_value(
              hold_id,
              read_string(hold_input, "handle"),
              read_string(hold_input, "reason"),
              read_string(hold_input, "reasonNotes"),
            )
          let held =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("ON_HOLD")),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #(
                "supportedActions",
                captured_action_list([
                  "RELEASE_HOLD",
                  "HOLD",
                  "MOVE",
                ]),
              ),
              #("fulfillmentHolds", CapturedArray([hold])),
              #(
                "lineItems",
                fulfillment_order_line_items_with_quantity(
                  order.data,
                  quantity,
                  True,
                ),
              ),
            ])
          let held =
            FulfillmentOrderRecord(
              ..held,
              status: "ON_HOLD",
              manually_held: True,
            )
          let remaining_quantity =
            max_int(
              first_fulfillment_order_line_item_total(order.data) - quantity,
              0,
            )
          let #(remaining_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "FulfillmentOrder")
          let remaining =
            update_fulfillment_order_fields(order, [
              #("id", CapturedString(remaining_id)),
              #("status", CapturedString("OPEN")),
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
          let #(held, next_store) =
            store.stage_upsert_fulfillment_order(draft_store, held)
          let #(remaining, next_store) =
            store.stage_upsert_fulfillment_order(next_store, remaining)
          #(
            MutationFieldResult(
              key: key,
              payload: fulfillment_order_payload_json(field, fragments, [
                #("__typename", SrcString("FulfillmentOrderHoldPayload")),
                #("fulfillmentHold", captured_json_source(hold)),
                #("fulfillmentOrder", fulfillment_order_source(held)),
                #(
                  "remainingFulfillmentOrder",
                  fulfillment_order_source(remaining),
                ),
                #("userErrors", SrcList([])),
              ]),
              errors: [],
              staged_resource_ids: [held.id, remaining.id],
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

fn handle_fulfillment_order_release_hold(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let restored_quantity =
            sibling_fulfillment_order_quantity(draft_store, order)
            + first_fulfillment_order_line_item_total(order.data)
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("OPEN")),
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
          let next_store = close_sibling_fulfillment_orders(next_store, staged)
          fulfillment_order_single_payload_result(
            next_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderReleaseHoldPayload",
            staged,
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

fn handle_fulfillment_order_move(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let quantity =
    first_fulfillment_order_line_item_quantity(read_object_array(
      args,
      "fulfillmentOrderLineItems",
    ))
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let remaining_quantity =
            max_int(
              first_fulfillment_order_line_item_total(order.data) - quantity,
              0,
            )
          let original =
            update_fulfillment_order_fields(order, [
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
            ])
          let #(moved_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "FulfillmentOrder")
          let moved =
            update_fulfillment_order_fields(order, [
              #("id", CapturedString(moved_id)),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #(
                "assignedLocation",
                assigned_location_value(read_string(args, "newLocationId")),
              ),
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
              assigned_location_id: read_string(args, "newLocationId"),
            )
          let #(original, next_store) =
            store.stage_upsert_fulfillment_order(draft_store, original)
          let #(moved, next_store) =
            store.stage_upsert_fulfillment_order(next_store, moved)
          #(
            MutationFieldResult(
              key: key,
              payload: fulfillment_order_payload_json(field, fragments, [
                #("__typename", SrcString("FulfillmentOrderMovePayload")),
                #("movedFulfillmentOrder", fulfillment_order_source(moved)),
                #(
                  "originalFulfillmentOrder",
                  fulfillment_order_source(original),
                ),
                #(
                  "remainingFulfillmentOrder",
                  fulfillment_order_source(original),
                ),
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

fn handle_fulfillment_order_simple_status(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_typename: String,
  status: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
          let updated =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString(status)),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #("supportedActions", captured_action_list(actions)),
            ])
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

fn handle_fulfillment_order_cancel(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_fulfillment_order_by_id(draft_store, id) {
        Some(order) -> {
          let canceled =
            update_fulfillment_order_fields(order, [
              #("status", CapturedString("CLOSED")),
              #("updatedAt", CapturedString(synthetic_timestamp_string())),
              #("supportedActions", CapturedArray([])),
              #("lineItems", captured_connection([])),
            ])
          let canceled = FulfillmentOrderRecord(..canceled, status: "CLOSED")
          let #(replacement_id, identity) =
            synthetic_identity.make_synthetic_gid(identity, "FulfillmentOrder")
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
            MutationFieldResult(
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

fn handle_fulfillment_order_split(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let splits = read_object_array(args, "fulfillmentOrderSplits")
  case splits {
    [split, ..] -> {
      let id = read_string(split, "fulfillmentOrderId")
      let quantity =
        first_fulfillment_order_line_item_quantity(read_object_array(
          split,
          "fulfillmentOrderLineItems",
        ))
      case id {
        Some(id) ->
          case store.get_effective_fulfillment_order_by_id(draft_store, id) {
            Some(order) -> {
              let remaining_quantity =
                max_int(
                  first_fulfillment_order_line_item_total(order.data) - quantity,
                  0,
                )
              let original =
                update_fulfillment_order_fields(order, [
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                  #(
                    "supportedActions",
                    captured_action_list([
                      "CREATE_FULFILLMENT",
                      "REPORT_PROGRESS",
                      "MOVE",
                      "HOLD",
                      "SPLIT",
                      "MERGE",
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
                ])
              let #(remaining_id, identity) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "FulfillmentOrder",
                )
              let remaining =
                update_fulfillment_order_fields(order, [
                  #("id", CapturedString(remaining_id)),
                  #("updatedAt", CapturedString(synthetic_timestamp_string())),
                  #(
                    "supportedActions",
                    captured_action_list([
                      "CREATE_FULFILLMENT",
                      "REPORT_PROGRESS",
                      "MOVE",
                      "HOLD",
                      "MERGE",
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
              let remaining =
                FulfillmentOrderRecord(..remaining, id: remaining_id)
              let #(original, next_store) =
                store.stage_upsert_fulfillment_order(draft_store, original)
              let #(remaining, next_store) =
                store.stage_upsert_fulfillment_order(next_store, remaining)
              let split_source =
                src_object([
                  #("fulfillmentOrder", fulfillment_order_source(original)),
                  #(
                    "remainingFulfillmentOrder",
                    fulfillment_order_source(remaining),
                  ),
                  #("replacementFulfillmentOrder", SrcNull),
                ])
              #(
                MutationFieldResult(
                  key: key,
                  payload: fulfillment_order_payload_json(field, fragments, [
                    #("__typename", SrcString("FulfillmentOrderSplitPayload")),
                    #("fulfillmentOrderSplits", SrcList([split_source])),
                    #("userErrors", SrcList([])),
                  ]),
                  errors: [],
                  staged_resource_ids: [original.id, remaining.id],
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
                "FulfillmentOrderSplitPayload",
              )
          }
        None ->
          fulfillment_order_missing_mutation_result(
            draft_store,
            identity,
            field,
            fragments,
            "FulfillmentOrderSplitPayload",
          )
      }
    }
    _ ->
      fulfillment_order_missing_mutation_result(
        draft_store,
        identity,
        field,
        fragments,
        "FulfillmentOrderSplitPayload",
      )
  }
}

fn handle_fulfillment_orders_set_deadline(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let deadline = read_string(args, "fulfillmentDeadline")
  let ids = read_string_array(args, "fulfillmentOrderIds")
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
    MutationFieldResult(
      key: key,
      payload: fulfillment_order_payload_json(field, fragments, [
        #(
          "__typename",
          SrcString("FulfillmentOrdersSetFulfillmentDeadlinePayload"),
        ),
        #("success", SrcBool(True)),
        #("userErrors", SrcList([])),
      ]),
      errors: [],
      staged_resource_ids: ids,
    ),
    next_store,
    identity,
  )
}

fn handle_fulfillment_order_merge(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = resolved_args(field, variables)
  let ids = fulfillment_order_merge_ids(args)
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
            MutationFieldResult(
              key: key,
              payload: fulfillment_order_payload_json(field, fragments, [
                #("__typename", SrcString("FulfillmentOrderMergePayload")),
                #(
                  "fulfillmentOrderMerges",
                  SrcList([
                    src_object([
                      #("fulfillmentOrder", fulfillment_order_source(merged)),
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

fn fulfillment_service_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(FulfillmentServiceUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: fulfillment_service_payload_json(
        draft_store,
        field,
        fragments,
        payload_typename,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn fulfillment_service_delete_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(FulfillmentServiceUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: fulfillment_service_delete_payload_json(
        field,
        fragments,
        None,
        user_errors,
      ),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn handle_location_local_pickup_enable(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input =
    read_object(args, "localPickupSettings") |> option.unwrap(dict.new())
  let location_id = read_string(input, "locationId")
  case find_active_store_property_location(draft_store, location_id) {
    Some(location) -> {
      let settings =
        StorePropertyObject(
          dict.from_list([
            #(
              "pickupTime",
              StorePropertyString(
                read_string(input, "pickupTime") |> option.unwrap("ONE_HOUR"),
              ),
            ),
            #(
              "instructions",
              StorePropertyString(
                read_string(input, "instructions") |> option.unwrap(""),
              ),
            ),
          ]),
        )
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let updated =
        StorePropertyRecord(
          ..location,
          data: location.data
            |> dict.insert("localPickupSettingsV2", settings)
            |> dict.insert("localPickupSettings", settings)
            |> dict.insert("updatedAt", StorePropertyString(timestamp)),
        )
      let #(_, next_store) =
        store.upsert_staged_store_property_location(draft_store, updated)
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: local_pickup_enable_payload_json(
            field,
            fragments,
            Some(settings),
            [],
          ),
          errors: [],
          staged_resource_ids: [location.id],
        ),
        next_store,
        next_identity,
      )
    }
    None -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: local_pickup_enable_payload_json(field, fragments, None, [
          local_pickup_location_not_found("localPickupSettings", location_id),
        ]),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_location_local_pickup_disable(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let location_id = read_string(args, "locationId")
  case find_active_store_property_location(draft_store, location_id) {
    Some(location) -> {
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let updated =
        StorePropertyRecord(
          ..location,
          data: location.data
            |> dict.insert("localPickupSettingsV2", StorePropertyNull)
            |> dict.insert("localPickupSettings", StorePropertyNull)
            |> dict.insert("updatedAt", StorePropertyString(timestamp)),
        )
      let #(_, next_store) =
        store.upsert_staged_store_property_location(draft_store, updated)
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: local_pickup_disable_payload_json(
            field,
            fragments,
            Some(location.id),
            [],
          ),
          errors: [],
          staged_resource_ids: [location.id],
        ),
        next_store,
        next_identity,
      )
    }
    None -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: local_pickup_disable_payload_json(field, fragments, None, [
          local_pickup_location_not_found("locationId", location_id),
        ]),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_shipping_package_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let id = read_string(args, "id")
  let input = read_object(args, "shippingPackage")
  case id, input {
    Some(package_id), Some(package_input) -> {
      case store.get_effective_shipping_package_by_id(draft_store, package_id) {
        Some(base) -> {
          let #(updated_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let #(updated, pre_staged_store) =
            apply_package_input(draft_store, base, package_input, updated_at)
          let #(_, next_store) =
            store.stage_update_shipping_package(pre_staged_store, updated)
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: payload_json(
                field,
                fragments,
                "ShippingPackageUpdatePayload",
                None,
              ),
              errors: [],
              staged_resource_ids: [package_id],
            ),
            next_store,
            next_identity,
          )
        }
        None -> invalid_shipping_package_result(draft_store, identity, field)
      }
    }
    _, _ -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: payload_json(
          field,
          fragments,
          "ShippingPackageUpdatePayload",
          None,
        ),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_shipping_package_make_default(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(package_id) -> {
      case store.get_effective_shipping_package_by_id(draft_store, package_id) {
        Some(_) -> {
          let #(updated_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let packages = store.list_effective_shipping_packages(draft_store)
          let next_store =
            list.fold(
              packages,
              draft_store,
              fn(current_store, shipping_package) {
                let updated =
                  ShippingPackageRecord(
                    ..shipping_package,
                    default: shipping_package.id == package_id,
                    updated_at: updated_at,
                  )
                let #(_, staged_store) =
                  store.stage_update_shipping_package(current_store, updated)
                staged_store
              },
            )
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: payload_json(
                field,
                fragments,
                "ShippingPackageMakeDefaultPayload",
                None,
              ),
              errors: [],
              staged_resource_ids: [package_id],
            ),
            next_store,
            next_identity,
          )
        }
        None -> invalid_shipping_package_result(draft_store, identity, field)
      }
    }
    None -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: payload_json(
          field,
          fragments,
          "ShippingPackageMakeDefaultPayload",
          None,
        ),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn handle_shipping_package_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(package_id) -> {
      case store.get_effective_shipping_package_by_id(draft_store, package_id) {
        Some(_) -> {
          let next_store =
            store.delete_staged_shipping_package(draft_store, package_id)
          #(
            MutationFieldResult(
              key: get_field_response_key(field),
              payload: payload_json(
                field,
                fragments,
                "ShippingPackageDeletePayload",
                Some(package_id),
              ),
              errors: [],
              staged_resource_ids: [package_id],
            ),
            next_store,
            identity,
          )
        }
        None -> invalid_shipping_package_result(draft_store, identity, field)
      }
    }
    None -> #(
      MutationFieldResult(
        key: get_field_response_key(field),
        payload: payload_json(
          field,
          fragments,
          "ShippingPackageDeletePayload",
          None,
        ),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

fn payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  deleted_id: Option(String),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #("deletedId", option_to_source(deleted_id)),
      #("userErrors", SrcList([])),
    ])
  let selections = root_field.get_selection_names(field)
  case field {
    Field(selection_set: Some(selection_set), ..) -> {
      let SelectionSet(selections: child_selections, ..) = selection_set
      project_graphql_value(source, child_selections, fragments)
    }
    _ ->
      case list.contains(selections, "deletedId") {
        True -> json.object([#("deletedId", optional_string_json(deleted_id))])
        False -> json.object([])
      }
  }
}

fn carrier_service_payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  carrier_service: Option(CarrierServiceRecord),
  user_errors: List(CarrierServiceUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #("carrierService", optional_carrier_service_source(carrier_service)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, carrier_service_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn carrier_service_delete_payload_json(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(CarrierServiceUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("CarrierServiceDeletePayload")),
      #("deletedId", option_to_source(deleted_id)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, carrier_service_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn fulfillment_service_payload_json(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  fulfillment_service: Option(FulfillmentServiceRecord),
  user_errors: List(FulfillmentServiceUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #(
        "fulfillmentService",
        optional_fulfillment_service_source(store, fulfillment_service),
      ),
      #(
        "userErrors",
        SrcList(list.map(user_errors, fulfillment_service_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn fulfillment_service_delete_payload_json(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(FulfillmentServiceUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("FulfillmentServiceDeletePayload")),
      #("deletedId", option_to_source(deleted_id)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, fulfillment_service_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn delivery_profile_payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  profile: Option(DeliveryProfileRecord),
  user_errors: List(DeliveryProfileUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString(payload_typename)),
      #("profile", optional_delivery_profile_source(profile)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, delivery_profile_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn delivery_profile_remove_payload_json(
  field: Selection,
  fragments: FragmentMap,
  job: Option(#(String, Bool)),
  user_errors: List(DeliveryProfileUserError),
) -> Json {
  let job_source = case job {
    Some(#(id, done)) ->
      src_object([
        #("__typename", SrcString("Job")),
        #("id", SrcString(id)),
        #("done", SrcBool(done)),
        #("query", SrcNull),
      ])
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("DeliveryProfileRemovePayload")),
      #("job", job_source),
      #(
        "userErrors",
        SrcList(list.map(user_errors, delivery_profile_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn fulfillment_order_payload_json(
  field: Selection,
  fragments: FragmentMap,
  entries: List(#(String, SourceValue)),
) -> Json {
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(src_object(entries), child_selections, fragments)
    _ -> json.object([])
  }
}

fn local_pickup_enable_payload_json(
  field: Selection,
  fragments: FragmentMap,
  settings: Option(StorePropertyValue),
  user_errors: List(LocalPickupUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("LocationLocalPickupEnablePayload")),
      #("localPickupSettings", optional_store_property_source(settings)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, local_pickup_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn local_pickup_disable_payload_json(
  field: Selection,
  fragments: FragmentMap,
  location_id: Option(String),
  user_errors: List(LocalPickupUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("LocationLocalPickupDisablePayload")),
      #("locationId", option_to_source(location_id)),
      #(
        "userErrors",
        SrcList(list.map(user_errors, local_pickup_user_error_source)),
      ),
    ])
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) -> project_graphql_value(source, child_selections, fragments)
    _ -> json.object([])
  }
}

fn project_carrier_service(
  service: CarrierServiceRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(
      selection_set: Some(SelectionSet(selections: child_selections, ..)),
      ..,
    ) ->
      project_graphql_value(
        carrier_service_source(service),
        child_selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn project_fulfillment_service(
  store: Store,
  service: FulfillmentServiceRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    fulfillment_service_source(store, service),
    selected_selections(field),
    fragments,
  )
}

fn project_store_property_record(
  record: StorePropertyRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    store_property_record_source(record),
    selected_selections(field),
    fragments,
  )
}

fn project_delivery_profile(
  profile: DeliveryProfileRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    delivery_profile_source(profile),
    selected_selections(field),
    fragments,
  )
}

fn project_fulfillment(
  fulfillment: FulfillmentRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    fulfillment_source(fulfillment),
    selected_selections(field),
    fragments,
  )
}

fn project_fulfillment_order(
  fulfillment_order: FulfillmentOrderRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    fulfillment_order_source(fulfillment_order),
    selected_selections(field),
    fragments,
  )
}

fn project_shipping_order(
  store: Store,
  order: ShippingOrderRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    shipping_order_source(store, order),
    selected_selections(field),
    fragments,
  )
}

fn project_reverse_delivery(
  store: Store,
  reverse_delivery: ReverseDeliveryRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    reverse_delivery_source(store, reverse_delivery),
    selected_selections(field),
    fragments,
  )
}

fn project_reverse_fulfillment_order(
  store: Store,
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    reverse_fulfillment_order_source(store, reverse_fulfillment_order),
    selected_selections(field),
    fragments,
  )
}

fn serialize_delivery_profiles_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let profiles = list_delivery_profiles_for_connection(store, field, variables)
  let window =
    paginate_connection_items(
      profiles,
      field,
      variables,
      delivery_profile_cursor,
      default_connection_window_options(),
    )
  serialize_delivery_profile_connection(field, window, fragments)
}

fn serialize_delivery_profile_connection(
  field: Selection,
  window: ConnectionWindow(DeliveryProfileRecord),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: delivery_profile_cursor,
      serialize_node: fn(profile, selection, _index) {
        project_delivery_profile(profile, selection, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        ..default_connection_page_info_options(),
        prefix_cursors: False,
      ),
    ),
  )
}

fn list_delivery_profiles_for_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DeliveryProfileRecord) {
  let args = resolved_args(field, variables)
  let profiles =
    store.list_effective_delivery_profiles(store)
    |> list.filter(fn(profile) {
      case read_bool(args, "merchantOwnedOnly") {
        Some(True) -> profile.merchant_owned
        _ -> True
      }
    })
  case read_bool(args, "reverse") {
    Some(True) -> list.reverse(profiles)
    _ -> profiles
  }
}

fn delivery_profile_cursor(
  profile: DeliveryProfileRecord,
  _index: Int,
) -> String {
  profile.cursor |> option.unwrap(profile.id)
}

fn serialize_carrier_services_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let services = list_carrier_services_for_connection(store, field, variables)
  let window =
    paginate_connection_items(
      services,
      field,
      variables,
      fn(service, _index) { service.id },
      default_connection_window_options(),
    )
  serialize_carrier_service_connection(field, window, fragments)
}

fn serialize_carrier_service_connection(
  field: Selection,
  window: ConnectionWindow(CarrierServiceRecord),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(service, _index) { service.id },
      serialize_node: fn(service, selection, _index) {
        project_carrier_service(service, selection, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_fulfillment_orders_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let orders = list_fulfillment_orders_for_connection(store, field, variables)
  let window =
    paginate_connection_items(
      orders,
      field,
      variables,
      fn(order, _index) { order.id },
      default_connection_window_options(),
    )
  serialize_fulfillment_order_connection(field, window, fragments)
}

fn serialize_assigned_fulfillment_orders_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = resolved_args(field, variables)
  let location_ids = read_string_array(args, "locationIds")
  let assignment_status = read_string(args, "assignmentStatus")
  let orders =
    store.list_effective_fulfillment_orders(store)
    |> list.filter(fn(order) {
      order.status != "CLOSED"
      && assigned_fulfillment_order_matches_assignment(order, assignment_status)
      && fulfillment_order_matches_location_ids(order, location_ids)
    })
    |> list.sort(compare_fulfillment_orders_by_id)
  let orders = case read_bool(args, "reverse") {
    Some(True) -> list.reverse(orders)
    _ -> orders
  }
  let window =
    paginate_connection_items(
      orders,
      field,
      variables,
      fn(order, _index) { order.id },
      default_connection_window_options(),
    )
  serialize_fulfillment_order_connection(field, window, fragments)
}

fn serialize_manual_holds_fulfillment_orders_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let orders =
    store.list_effective_fulfillment_orders(store)
    |> list.filter(fn(order) { order.manually_held })
    |> list.sort(compare_fulfillment_orders_by_id)
  let window =
    paginate_connection_items(
      orders,
      field,
      variables,
      fn(order, _index) { order.id },
      default_connection_window_options(),
    )
  serialize_fulfillment_order_connection(field, window, fragments)
}

fn serialize_fulfillment_order_connection(
  field: Selection,
  window: ConnectionWindow(FulfillmentOrderRecord),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(order, _index) { order.id },
      serialize_node: fn(order, selection, _index) {
        project_fulfillment_order(order, selection, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn list_fulfillment_orders_for_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(FulfillmentOrderRecord) {
  let args = resolved_args(field, variables)
  let include_closed = read_bool(args, "includeClosed") |> option.unwrap(False)
  let sorted =
    store.list_effective_fulfillment_orders(store)
    |> list.filter(fn(order) { include_closed || order.status != "CLOSED" })
    |> filter_fulfillment_orders_by_query(dict.get(args, "query"))
    |> list.sort(compare_fulfillment_orders_by_id)
  case read_bool(args, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

fn filter_fulfillment_orders_by_query(
  orders: List(FulfillmentOrderRecord),
  raw_query: Result(root_field.ResolvedValue, Nil),
) -> List(FulfillmentOrderRecord) {
  case raw_query {
    Ok(root_field.StringVal(query)) ->
      filter_fulfillment_orders_by_query_string(orders, query)
    _ -> orders
  }
}

fn filter_fulfillment_orders_by_query_string(
  orders: List(FulfillmentOrderRecord),
  query: String,
) -> List(FulfillmentOrderRecord) {
  let trimmed = string.trim(query)
  case trimmed {
    "" -> orders
    _ -> {
      let terms =
        search_query_parser.parse_search_query_terms(
          trimmed,
          search_query_parser.default_term_list_options(),
        )
        |> list.filter(fn(term) { term.field == Some("status") })
      case terms {
        [] -> orders
        _ ->
          orders
          |> list.filter(fn(order) {
            list.all(terms, fn(term) {
              fulfillment_order_matches_search_term(order, term)
            })
          })
      }
    }
  }
}

fn fulfillment_order_matches_search_term(
  order: FulfillmentOrderRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let normalized = search_query_parser.normalize_search_query_value(term.value)
  let matches = case term.field {
    Some("status") ->
      normalized
      == search_query_parser.normalize_search_query_value(order.status)
    _ -> True
  }
  case term.negated {
    True -> !matches
    False -> matches
  }
}

fn assigned_fulfillment_order_matches_assignment(
  order: FulfillmentOrderRecord,
  assignment_status: Option(String),
) -> Bool {
  case assignment_status {
    Some("FULFILLMENT_REQUESTED") ->
      order.assignment_status == Some("FULFILLMENT_REQUESTED")
      || order.request_status == "SUBMITTED"
    Some("FULFILLMENT_ACCEPTED") ->
      order.assignment_status == Some("FULFILLMENT_ACCEPTED")
      || order.request_status == "ACCEPTED"
      && !fulfillment_order_has_cancellation_request(order)
    Some("CANCELLATION_REQUESTED") ->
      order.assignment_status == Some("CANCELLATION_REQUESTED")
      || fulfillment_order_has_cancellation_request(order)
    _ -> True
  }
}

fn fulfillment_order_matches_location_ids(
  order: FulfillmentOrderRecord,
  location_ids: List(String),
) -> Bool {
  case location_ids {
    [] -> True
    _ ->
      case order.assigned_location_id {
        Some(id) -> list.contains(location_ids, id)
        None -> False
      }
  }
}

fn fulfillment_order_has_cancellation_request(
  order: FulfillmentOrderRecord,
) -> Bool {
  case captured_array_field(order.data, "merchantRequests", "nodes") {
    [] -> False
    requests ->
      list.any(requests, fn(request) {
        captured_string_field(request, "kind") == Some("CANCELLATION_REQUEST")
      })
  }
}

fn compare_fulfillment_orders_by_id(
  left: FulfillmentOrderRecord,
  right: FulfillmentOrderRecord,
) -> Order {
  string.compare(left.id, right.id)
}

fn list_carrier_services_for_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(CarrierServiceRecord) {
  let args = resolved_args(field, variables)
  let sorted =
    store.list_effective_carrier_services(store)
    |> filter_carrier_services_by_query(dict.get(args, "query"))
    |> list.sort(fn(left, right) {
      compare_carrier_services(left, right, dict.get(args, "sortKey"))
    })
  case read_bool(args, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

fn filter_carrier_services_by_query(
  services: List(CarrierServiceRecord),
  raw_query: Result(root_field.ResolvedValue, Nil),
) -> List(CarrierServiceRecord) {
  case raw_query {
    Ok(root_field.StringVal(query)) ->
      filter_carrier_services_by_query_string(services, query)
    _ -> services
  }
}

fn filter_carrier_services_by_query_string(
  services: List(CarrierServiceRecord),
  query: String,
) -> List(CarrierServiceRecord) {
  let trimmed = string.trim(query)
  case trimmed {
    "" -> services
    _ -> {
      let options =
        search_query_parser.SearchQueryTermListOptions(
          ..search_query_parser.default_term_list_options(),
          ignored_keywords: ["AND"],
        )
      let terms =
        search_query_parser.parse_search_query_terms(trimmed, options)
        |> list.filter(fn(term) {
          term.field == Some("active") || term.field == Some("id")
        })
      case terms {
        [] -> services
        _ ->
          services
          |> list.filter(fn(service) {
            list.all(terms, fn(term) {
              carrier_service_matches_term(service, term)
            })
          })
      }
    }
  }
}

fn carrier_service_matches_term(
  service: CarrierServiceRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let normalized = search_query_parser.normalize_search_query_value(term.value)
  let matches = case term.field {
    Some("active") -> normalized == bool_string(service.active)
    Some("id") ->
      normalized == search_query_parser.normalize_search_query_value(service.id)
      || normalized
      == search_query_parser.normalize_search_query_value(
        carrier_service_numeric_id(service.id),
      )
    _ -> True
  }
  case term.negated {
    True -> !matches
    False -> matches
  }
}

fn compare_carrier_services(
  left: CarrierServiceRecord,
  right: CarrierServiceRecord,
  sort_key: Result(root_field.ResolvedValue, Nil),
) -> Order {
  case sort_key {
    Ok(root_field.StringVal("CREATED_AT")) ->
      compare_string_then_id(
        left.created_at,
        right.created_at,
        left.id,
        right.id,
      )
    Ok(root_field.StringVal("UPDATED_AT")) ->
      compare_string_then_id(
        left.updated_at,
        right.updated_at,
        left.id,
        right.id,
      )
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

fn compare_string_then_id(
  left_value: String,
  right_value: String,
  left_id: String,
  right_id: String,
) -> Order {
  case string.compare(left_value, right_value) {
    order.Eq -> resource_ids.compare_shopify_resource_ids(left_id, right_id)
    other -> other
  }
}

fn sort_store_property_locations_by_id(
  locations: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  list.sort(locations, fn(left, right) {
    resource_ids.compare_shopify_resource_ids(left.id, right.id)
  })
}

fn filter_active_non_fulfillment_locations(
  locations: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  locations
  |> list.filter(fn(location) {
    is_active_location(location) && !is_fulfillment_service_location(location)
  })
}

fn find_active_store_property_location(
  draft_store: Store,
  location_id: Option(String),
) -> Option(StorePropertyRecord) {
  case location_id {
    Some(id) ->
      case store.get_effective_store_property_location_by_id(draft_store, id) {
        Some(location) ->
          case is_active_location(location) {
            True -> Some(location)
            False -> None
          }
        None -> None
      }
    None -> None
  }
}

fn is_active_location(location: StorePropertyRecord) -> Bool {
  store_property_bool_field(location, "isActive") |> option.unwrap(True)
}

fn is_fulfillment_service_location(location: StorePropertyRecord) -> Bool {
  store_property_bool_field(location, "isFulfillmentService")
  |> option.unwrap(False)
}

fn optional_delivery_profile_source(
  value: Option(DeliveryProfileRecord),
) -> SourceValue {
  case value {
    Some(profile) -> delivery_profile_source(profile)
    None -> SrcNull
  }
}

fn optional_carrier_service_source(
  value: Option(CarrierServiceRecord),
) -> SourceValue {
  case value {
    Some(service) -> carrier_service_source(service)
    None -> SrcNull
  }
}

fn carrier_service_source(service: CarrierServiceRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("DeliveryCarrierService")),
    #("id", SrcString(service.id)),
    #("name", option_to_source(service.name)),
    #("formattedName", option_to_source(service.formatted_name)),
    #("callbackUrl", option_to_source(service.callback_url)),
    #("active", SrcBool(service.active)),
    #("supportsServiceDiscovery", SrcBool(service.supports_service_discovery)),
  ])
}

fn optional_fulfillment_service_source(
  store: Store,
  value: Option(FulfillmentServiceRecord),
) -> SourceValue {
  case value {
    Some(service) -> fulfillment_service_source(store, service)
    None -> SrcNull
  }
}

fn fulfillment_service_source(
  store: Store,
  service: FulfillmentServiceRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("FulfillmentService")),
    #("id", SrcString(service.id)),
    #("handle", SrcString(service.handle)),
    #("serviceName", SrcString(service.service_name)),
    #("callbackUrl", option_to_source(service.callback_url)),
    #("inventoryManagement", SrcBool(service.inventory_management)),
    #("location", fulfillment_service_location_source(store, service)),
    #("requiresShippingMethod", SrcBool(service.requires_shipping_method)),
    #("trackingSupport", SrcBool(service.tracking_support)),
    #("type", SrcString(service.type_)),
  ])
}

fn fulfillment_service_location_source(
  store: Store,
  service: FulfillmentServiceRecord,
) -> SourceValue {
  case service.location_id {
    Some(location_id) ->
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(location) -> store_property_record_source(location)
        None -> SrcNull
      }
    None -> SrcNull
  }
}

fn make_delivery_profile(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> #(DeliveryProfileRecord, SyntheticIdentityRegistry) {
  let #(profile_id, identity_after_profile) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryProfile")
  let #(groups, origin_count, zone_country_count, active_count, next_identity) =
    make_delivery_profile_location_groups(
      draft_store,
      list.append(
        read_object_array(input, "profileLocationGroups"),
        read_object_array(input, "locationGroupsToCreate"),
      ),
      identity_after_profile,
    )
  let variant_ids = read_string_array(input, "variantsToAssociate")
  let profile_items = profile_item_nodes(draft_store, variant_ids)
  let data =
    CapturedObject([
      #("id", CapturedString(profile_id)),
      #("name", CapturedString(name)),
      #("default", CapturedBool(False)),
      #("merchantOwned", CapturedBool(True)),
      #("version", CapturedInt(1)),
      #("activeMethodDefinitionsCount", CapturedInt(active_count)),
      #("locationsWithoutRatesCount", CapturedInt(0)),
      #("originLocationCount", CapturedInt(origin_count)),
      #("zoneCountryCount", CapturedInt(zone_country_count)),
      #("productVariantsCount", captured_count(list.length(variant_ids))),
      #("profileItems", captured_connection(profile_items)),
      #("profileLocationGroups", CapturedArray(groups)),
      #("sellingPlanGroups", captured_connection([])),
      #("unassignedLocations", CapturedArray([])),
      #("unassignedLocationsPaginated", captured_connection([])),
    ])
  #(
    DeliveryProfileRecord(
      id: profile_id,
      cursor: None,
      merchant_owned: True,
      data: data,
    ),
    next_identity,
  )
}

fn update_delivery_profile(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  profile: DeliveryProfileRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(DeliveryProfileRecord, SyntheticIdentityRegistry) {
  let associated = read_string_array(input, "variantsToAssociate")
  let dissociated = read_string_array(input, "variantsToDissociate")
  let #(profile_items, variant_count) = case associated, dissociated {
    [], [] -> #(
      captured_array_field(profile.data, "profileItems", "nodes"),
      captured_int_field(profile.data, "productVariantsCount", "count")
        |> option.unwrap(0),
    )
    _, [] -> {
      let nodes = profile_item_nodes(draft_store, associated)
      #(nodes, list.length(associated))
    }
    _, _ -> #([], 0)
  }
  let name =
    read_string(input, "name")
    |> option.or(captured_string_field(profile.data, "name"))
    |> option.unwrap("")
  let base_version =
    captured_int_field(profile.data, "version", "") |> option.unwrap(1)
  let version = base_version + 1
  let base_origin_count =
    captured_int_field(profile.data, "originLocationCount", "")
    |> option.unwrap(0)
  let origin_count =
    base_origin_count + count_delivery_profile_locations_to_add(input)
  let base_active_count =
    captured_int_field(profile.data, "activeMethodDefinitionsCount", "")
    |> option.unwrap(0)
  let active_count =
    base_active_count + delivery_profile_active_method_delta(input)
  let active_count = case active_count < 0 {
    True -> 0
    False -> active_count
  }
  let data =
    captured_upsert_fields(profile.data, [
      #("name", CapturedString(name)),
      #("version", CapturedInt(version)),
      #("activeMethodDefinitionsCount", CapturedInt(active_count)),
      #("originLocationCount", CapturedInt(origin_count)),
      #("productVariantsCount", captured_count(variant_count)),
      #("profileItems", captured_connection(profile_items)),
    ])
  #(DeliveryProfileRecord(..profile, data: data), identity)
}

fn make_delivery_profile_location_groups(
  draft_store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), Int, Int, Int, SyntheticIdentityRegistry) {
  let #(groups, location_ids, zone_count, active_count, next_identity) =
    list.fold(inputs, #([], [], 0, 0, identity), fn(acc, input) {
      let #(items, all_locations, all_zones, all_active, current_identity) = acc
      let #(group, locations, zones, active, group_identity) =
        make_delivery_profile_location_group(
          draft_store,
          input,
          current_identity,
        )
      #(
        list.append(items, [group]),
        list.append(all_locations, locations),
        all_zones + zones,
        all_active + active,
        group_identity,
      )
    })
  #(
    groups,
    list.length(unique_strings(location_ids)),
    zone_count,
    active_count,
    next_identity,
  )
}

fn make_delivery_profile_location_group(
  draft_store: Store,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, List(String), Int, Int, SyntheticIdentityRegistry) {
  let #(group_id, identity_after_group) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryLocationGroup")
  let location_ids = read_string_array(input, "locations")
  let #(zone_nodes, zone_country_count, active_count, next_identity) =
    make_delivery_profile_zone_nodes(
      list.append(
        read_object_array(input, "zonesToCreate"),
        read_object_array(input, "zonesToUpdate"),
      ),
      identity_after_group,
    )
  #(
    CapturedObject([
      #(
        "locationGroup",
        CapturedObject([
          #("id", CapturedString(group_id)),
          #(
            "locations",
            captured_connection(location_nodes(draft_store, location_ids)),
          ),
          #("locationsCount", captured_count(list.length(location_ids))),
        ]),
      ),
      #("locationGroupZones", captured_connection(zone_nodes)),
      #("countriesInAnyZone", CapturedArray([])),
    ]),
    location_ids,
    zone_country_count,
    active_count,
    next_identity,
  )
}

fn make_delivery_profile_zone_nodes(
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), Int, Int, SyntheticIdentityRegistry) {
  list.fold(inputs, #([], 0, 0, identity), fn(acc, input) {
    let #(items, all_countries, all_active, current_identity) = acc
    let #(zone, country_count, active_count, next_identity) =
      make_delivery_profile_zone_node(input, current_identity)
    #(
      list.append(items, [zone]),
      all_countries + country_count,
      all_active + active_count,
      next_identity,
    )
  })
}

fn make_delivery_profile_zone_node(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, Int, Int, SyntheticIdentityRegistry) {
  let #(zone_id, identity_after_zone) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryZone")
  let countries = read_object_array(input, "countries")
  let country_nodes = list.map(countries, make_delivery_country)
  let #(method_nodes, active_count, next_identity) =
    make_delivery_profile_method_nodes(
      list.append(
        read_object_array(input, "methodDefinitionsToCreate"),
        read_object_array(input, "methodDefinitionsToUpdate"),
      ),
      identity_after_zone,
    )
  #(
    CapturedObject([
      #(
        "zone",
        CapturedObject([
          #(
            "id",
            CapturedString(read_string(input, "id") |> option.unwrap(zone_id)),
          ),
          #(
            "name",
            CapturedString(
              read_string(input, "name") |> option.unwrap("Shipping zone"),
            ),
          ),
          #("countries", CapturedArray(country_nodes)),
        ]),
      ),
      #("methodDefinitions", captured_connection(method_nodes)),
    ]),
    list.length(countries),
    active_count,
    next_identity,
  )
}

fn make_delivery_profile_method_nodes(
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), Int, SyntheticIdentityRegistry) {
  list.fold(inputs, #([], 0, identity), fn(acc, input) {
    let #(items, all_active, current_identity) = acc
    let #(method, active, next_identity) =
      make_delivery_profile_method(input, current_identity)
    #(
      list.append(items, [method]),
      all_active
        + case active {
        True -> 1
        False -> 0
      },
      next_identity,
    )
  })
}

fn make_delivery_profile_method(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, Bool, SyntheticIdentityRegistry) {
  let #(method_id, identity_after_method) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryMethodDefinition")
  let #(rate_provider, identity_after_rate) =
    make_delivery_rate_provider(input, identity_after_method)
  let #(conditions, next_identity) =
    make_delivery_condition_nodes(
      list.append(
        read_object_array(input, "weightConditionsToCreate"),
        read_object_array(input, "priceConditionsToCreate"),
      ),
      identity_after_rate,
    )
  let active = read_bool(input, "active") |> option.unwrap(True)
  #(
    CapturedObject([
      #(
        "id",
        CapturedString(read_string(input, "id") |> option.unwrap(method_id)),
      ),
      #(
        "name",
        CapturedString(read_string(input, "name") |> option.unwrap("Standard")),
      ),
      #("active", CapturedBool(active)),
      #("description", CapturedNull),
      #("rateProvider", rate_provider),
      #("methodConditions", CapturedArray(conditions)),
    ]),
    active,
    next_identity,
  )
}

fn make_delivery_rate_provider(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let rate_definition =
    read_object(input, "rateDefinition") |> option.unwrap(dict.new())
  let #(rate_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryRateDefinition")
  let price = read_object(rate_definition, "price") |> option.unwrap(dict.new())
  #(
    CapturedObject([
      #("__typename", CapturedString("DeliveryRateDefinition")),
      #(
        "id",
        CapturedString(
          read_string(rate_definition, "id") |> option.unwrap(rate_id),
        ),
      ),
      #(
        "price",
        CapturedObject([
          #(
            "amount",
            CapturedString(read_string(price, "amount") |> option.unwrap("0.0")),
          ),
          #(
            "currencyCode",
            CapturedString(
              read_string(price, "currencyCode") |> option.unwrap("USD"),
            ),
          ),
        ]),
      ),
    ]),
    next_identity,
  )
}

fn make_delivery_condition_nodes(
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  list.fold(inputs, #([], identity), fn(acc, input) {
    let #(items, current_identity) = acc
    let #(condition, next_identity) =
      make_delivery_condition(input, current_identity)
    #(list.append(items, [condition]), next_identity)
  })
}

fn make_delivery_condition(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(condition_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "DeliveryCondition")
  let operator =
    read_string(input, "operator") |> option.unwrap("GREATER_THAN_OR_EQUAL_TO")
  let criteria = read_object(input, "criteria") |> option.unwrap(dict.new())
  let is_price = dict.has_key(criteria, "amount")
  let field = case is_price {
    True -> "TOTAL_PRICE"
    False -> "TOTAL_WEIGHT"
  }
  let condition_criteria = case is_price {
    True ->
      CapturedObject([
        #("__typename", CapturedString("MoneyV2")),
        #(
          "amount",
          CapturedString(normalize_money_amount(
            read_string(criteria, "amount") |> option.unwrap("0.0"),
          )),
        ),
        #(
          "currencyCode",
          CapturedString(
            read_string(criteria, "currencyCode") |> option.unwrap("USD"),
          ),
        ),
      ])
    False ->
      CapturedObject([
        #("__typename", CapturedString("Weight")),
        #(
          "value",
          read_number(criteria, "value") |> option.unwrap(CapturedInt(0)),
        ),
        #(
          "unit",
          CapturedString(
            read_string(criteria, "unit") |> option.unwrap("KILOGRAMS"),
          ),
        ),
      ])
  }
  #(
    CapturedObject([
      #(
        "id",
        CapturedString(
          condition_id <> "?operator=" <> string.lowercase(operator),
        ),
      ),
      #("field", CapturedString(field)),
      #("operator", CapturedString(operator)),
      #("conditionCriteria", condition_criteria),
    ]),
    next_identity,
  )
}

fn make_delivery_country(
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let rest_of_world = read_bool(input, "restOfWorld") |> option.unwrap(False)
  let code = case rest_of_world {
    True -> None
    False -> read_string(input, "code")
  }
  let name = case rest_of_world, code {
    True, _ -> "Rest of world"
    _, Some("CA") -> "Canada"
    _, Some("GB") -> "United Kingdom"
    _, Some("US") -> "United States"
    _, Some(value) -> value
    _, None -> "Unknown"
  }
  CapturedObject([
    #("id", CapturedString("")),
    #("name", CapturedString(name)),
    #("translatedName", CapturedString(name)),
    #(
      "code",
      CapturedObject([
        #("countryCode", option_to_captured_string(code)),
        #("restOfWorld", CapturedBool(rest_of_world)),
      ]),
    ),
    #("provinces", CapturedArray([])),
  ])
}

fn profile_item_nodes(
  draft_store: Store,
  variant_ids: List(String),
) -> List(CapturedJsonValue) {
  variant_ids
  |> list.filter_map(fn(variant_id) {
    case store.get_effective_variant_by_id(draft_store, variant_id) {
      Some(variant) -> {
        let product_id = variant.product_id
        let product_title = case
          store.get_effective_product_by_id(draft_store, product_id)
        {
          Some(product) -> product.title
          None -> ""
        }
        Ok(
          CapturedObject([
            #(
              "product",
              CapturedObject([
                #("id", CapturedString(product_id)),
                #("title", CapturedString(product_title)),
              ]),
            ),
            #(
              "variants",
              captured_connection([
                CapturedObject([
                  #("id", CapturedString(variant.id)),
                  #("title", CapturedString(variant.title)),
                ]),
              ]),
            ),
          ]),
        )
      }
      None -> Error(Nil)
    }
  })
}

fn location_nodes(
  draft_store: Store,
  location_ids: List(String),
) -> List(CapturedJsonValue) {
  list.map(location_ids, fn(location_id) {
    CapturedObject([
      #("id", CapturedString(location_id)),
      #("name", CapturedString(location_name(draft_store, location_id))),
    ])
  })
}

fn location_name(draft_store: Store, location_id: String) -> String {
  case store.get_effective_location_by_id(draft_store, location_id) {
    Some(location) -> location.name
    None ->
      case
        store.get_effective_store_property_location_by_id(
          draft_store,
          location_id,
        )
      {
        Some(location) ->
          store_property_string_field(location, "name") |> option.unwrap("")
        None -> ""
      }
  }
}

fn captured_connection(nodes: List(CapturedJsonValue)) -> CapturedJsonValue {
  CapturedObject([
    #("nodes", CapturedArray(nodes)),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", CapturedNull),
        #("endCursor", CapturedNull),
      ]),
    ),
  ])
}

fn captured_count(count: Int) -> CapturedJsonValue {
  CapturedObject([
    #("count", CapturedInt(count)),
    #("precision", CapturedString("EXACT")),
  ])
}

fn delivery_profile_source(profile: DeliveryProfileRecord) -> SourceValue {
  case captured_json_source(profile.data) |> annotate_delivery_profile_source {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("DeliveryProfile"))
        |> dict.insert("id", SrcString(profile.id)),
      )
    _ ->
      src_object([
        #("__typename", SrcString("DeliveryProfile")),
        #("id", SrcString(profile.id)),
      ])
  }
}

fn annotate_delivery_profile_source(value: SourceValue) -> SourceValue {
  case value {
    SrcList(items) -> SrcList(list.map(items, annotate_delivery_profile_source))
    SrcObject(fields) -> {
      let fields =
        fields
        |> dict.to_list
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, annotate_delivery_profile_source(item))
        })
        |> dict.from_list
      let fields = case dict.has_key(fields, "__typename") {
        True -> fields
        False ->
          case infer_delivery_profile_typename(fields) {
            Some(type_name) ->
              dict.insert(fields, "__typename", SrcString(type_name))
            None -> fields
          }
      }
      SrcObject(fields)
    }
    other -> other
  }
}

fn infer_delivery_profile_typename(
  fields: Dict(String, SourceValue),
) -> Option(String) {
  case dict.has_key(fields, "price") {
    True -> Some("DeliveryRateDefinition")
    False ->
      case
        dict.has_key(fields, "fixedFee")
        || dict.has_key(fields, "percentageOfRateFee")
      {
        True -> Some("DeliveryParticipant")
        False -> None
      }
  }
}

fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_source(item))
        })
        |> dict.from_list,
      )
  }
}

fn fulfillment_source(fulfillment: FulfillmentRecord) -> SourceValue {
  case captured_json_source(fulfillment.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("Fulfillment"))
        |> dict.insert("id", SrcString(fulfillment.id)),
      )
    _ ->
      src_object([
        #("__typename", SrcString("Fulfillment")),
        #("id", SrcString(fulfillment.id)),
      ])
  }
}

fn fulfillment_order_source(order: FulfillmentOrderRecord) -> SourceValue {
  case captured_json_source(order.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("FulfillmentOrder"))
        |> dict.insert("id", SrcString(order.id))
        |> dict.insert("status", SrcString(order.status))
        |> dict.insert("requestStatus", SrcString(order.request_status)),
      )
    _ ->
      src_object([
        #("__typename", SrcString("FulfillmentOrder")),
        #("id", SrcString(order.id)),
        #("status", SrcString(order.status)),
        #("requestStatus", SrcString(order.request_status)),
      ])
  }
}

fn fulfillment_event_source(event: CapturedJsonValue) -> SourceValue {
  captured_json_source(event)
}

fn shipping_order_source(
  store: Store,
  order: ShippingOrderRecord,
) -> SourceValue {
  let fulfillments =
    store.list_effective_fulfillments(store)
    |> list.filter(fn(fulfillment) { fulfillment.order_id == Some(order.id) })
    |> list.map(fulfillment_source)
  let fulfillment_orders =
    store.list_effective_fulfillment_orders(store)
    |> list.filter(fn(fulfillment_order) {
      fulfillment_order.order_id == Some(order.id)
    })
    |> list.map(fulfillment_order_source)
  case captured_json_source(order.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("Order"))
        |> dict.insert("id", SrcString(order.id))
        |> dict.insert("fulfillments", SrcList(fulfillments))
        |> dict.insert(
          "fulfillmentOrders",
          source_connection(fulfillment_orders),
        ),
      )
    _ ->
      src_object([
        #("__typename", SrcString("Order")),
        #("id", SrcString(order.id)),
        #("fulfillments", SrcList(fulfillments)),
        #("fulfillmentOrders", source_connection(fulfillment_orders)),
      ])
  }
}

fn reverse_delivery_source(
  store: Store,
  reverse_delivery: ReverseDeliveryRecord,
) -> SourceValue {
  let reverse_fulfillment_order =
    store.get_effective_reverse_fulfillment_order_by_id(
      store,
      reverse_delivery.reverse_fulfillment_order_id,
    )
  case captured_json_source(reverse_delivery.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("ReverseDelivery"))
        |> dict.insert("id", SrcString(reverse_delivery.id))
        |> dict.insert(
          "reverseFulfillmentOrder",
          optional_reverse_fulfillment_order_source(
            store,
            reverse_fulfillment_order,
          ),
        ),
      )
    _ ->
      src_object([
        #("__typename", SrcString("ReverseDelivery")),
        #("id", SrcString(reverse_delivery.id)),
        #(
          "reverseFulfillmentOrder",
          optional_reverse_fulfillment_order_source(
            store,
            reverse_fulfillment_order,
          ),
        ),
      ])
  }
}

fn optional_reverse_fulfillment_order_source(
  store: Store,
  reverse_fulfillment_order: Option(ReverseFulfillmentOrderRecord),
) -> SourceValue {
  case reverse_fulfillment_order {
    Some(record) -> reverse_fulfillment_order_source(store, record)
    None -> SrcNull
  }
}

fn reverse_fulfillment_order_source(
  store: Store,
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
) -> SourceValue {
  let reverse_deliveries =
    store.list_effective_reverse_deliveries(store)
    |> list.filter(fn(reverse_delivery) {
      reverse_delivery.reverse_fulfillment_order_id
      == reverse_fulfillment_order.id
    })
    |> list.map(fn(reverse_delivery) {
      reverse_delivery_source_without_parent(reverse_delivery)
    })
  case captured_json_source(reverse_fulfillment_order.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("ReverseFulfillmentOrder"))
        |> dict.insert("id", SrcString(reverse_fulfillment_order.id))
        |> dict.insert(
          "reverseDeliveries",
          source_connection(reverse_deliveries),
        ),
      )
    _ ->
      src_object([
        #("__typename", SrcString("ReverseFulfillmentOrder")),
        #("id", SrcString(reverse_fulfillment_order.id)),
        #("reverseDeliveries", source_connection(reverse_deliveries)),
      ])
  }
}

fn reverse_delivery_source_without_parent(
  reverse_delivery: ReverseDeliveryRecord,
) -> SourceValue {
  case captured_json_source(reverse_delivery.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("ReverseDelivery"))
        |> dict.insert("id", SrcString(reverse_delivery.id)),
      )
    _ ->
      src_object([
        #("__typename", SrcString("ReverseDelivery")),
        #("id", SrcString(reverse_delivery.id)),
      ])
  }
}

fn calculated_order_source(
  calculated_order: CalculatedOrderRecord,
) -> SourceValue {
  case captured_json_source(calculated_order.data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString("CalculatedOrder"))
        |> dict.insert("id", SrcString(calculated_order.id)),
      )
    _ ->
      src_object([
        #("__typename", SrcString("CalculatedOrder")),
        #("id", SrcString(calculated_order.id)),
      ])
  }
}

fn source_connection(nodes: List(SourceValue)) -> SourceValue {
  src_object([
    #("nodes", SrcList(nodes)),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

fn optional_store_property_source(
  value: Option(StorePropertyValue),
) -> SourceValue {
  case value {
    Some(value) -> store_property_value_to_source(value)
    None -> SrcNull
  }
}

fn store_property_record_source(record: StorePropertyRecord) -> SourceValue {
  store_property_data_to_source(record.data)
}

fn store_property_value_to_source(value: StorePropertyValue) -> SourceValue {
  case value {
    StorePropertyNull -> SrcNull
    StorePropertyString(value) -> SrcString(value)
    StorePropertyBool(value) -> SrcBool(value)
    StorePropertyInt(value) -> SrcInt(value)
    StorePropertyFloat(value) -> SrcFloat(value)
    StorePropertyList(values) ->
      SrcList(list.map(values, store_property_value_to_source))
    StorePropertyObject(values) -> store_property_data_to_source(values)
  }
}

fn store_property_data_to_source(
  data: Dict(String, StorePropertyValue),
) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, store_property_value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

fn carrier_service_user_error_source(
  error: CarrierServiceUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
  ])
}

fn fulfillment_service_user_error_source(
  error: FulfillmentServiceUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", option_to_source(error.code)),
  ])
}

fn delivery_profile_user_error_source(
  error: DeliveryProfileUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
  ])
}

fn local_pickup_user_error_source(error: LocalPickupUserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("DeliveryLocationLocalPickupSettingsError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", option_to_source(error.code)),
  ])
}

fn optional_string_list_source(value: Option(List(String))) -> SourceValue {
  case value {
    Some(items) -> SrcList(list.map(items, SrcString))
    None -> SrcNull
  }
}

fn carrier_service_formatted_name(name: Option(String)) -> Option(String) {
  case name {
    Some(value) -> Some(value <> " (Rates provided by app)")
    None -> None
  }
}

fn carrier_service_numeric_id(id: String) -> String {
  let last_path_segment = string.split(id, "/") |> list.last
  case last_path_segment {
    Ok(segment) ->
      case string.split(segment, "?") |> list.first {
        Ok(value) -> value
        Error(_) -> segment
      }
    Error(_) -> id
  }
}

fn validate_carrier_service_name(
  name: Option(String),
) -> List(CarrierServiceUserError) {
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> blank_carrier_service_name_errors()
        _ -> []
      }
    None -> blank_carrier_service_name_errors()
  }
}

fn blank_carrier_service_name_errors() -> List(CarrierServiceUserError) {
  [
    CarrierServiceUserError(
      field: None,
      message: "Shipping rate provider name can't be blank",
    ),
  ]
}

fn carrier_service_not_found_for_update() -> CarrierServiceUserError {
  CarrierServiceUserError(
    field: None,
    message: "The carrier or app could not be found.",
  )
}

fn carrier_service_not_found_for_delete() -> CarrierServiceUserError {
  CarrierServiceUserError(
    field: Some(["id"]),
    message: "The carrier or app could not be found.",
  )
}

fn validate_fulfillment_service_name(
  name: Option(String),
) -> List(FulfillmentServiceUserError) {
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> blank_fulfillment_service_name_errors()
        _ -> []
      }
    None -> blank_fulfillment_service_name_errors()
  }
}

fn blank_fulfillment_service_name_errors() -> List(FulfillmentServiceUserError) {
  [
    FulfillmentServiceUserError(
      field: Some(["name"]),
      message: "Name can't be blank",
      code: None,
    ),
  ]
}

fn validate_fulfillment_service_callback_url(
  callback_url: Option(String),
) -> List(FulfillmentServiceUserError) {
  case callback_url {
    Some(value) ->
      case is_allowed_fulfillment_service_callback_url(value) {
        True -> []
        False -> [
          FulfillmentServiceUserError(
            field: Some(["callbackUrl"]),
            message: "Callback url is not allowed",
            code: None,
          ),
        ]
      }
    None -> []
  }
}

fn is_allowed_fulfillment_service_callback_url(callback_url: String) -> Bool {
  string.starts_with(callback_url, "https://mock.shop")
}

fn fulfillment_service_not_found() -> FulfillmentServiceUserError {
  FulfillmentServiceUserError(
    field: Some(["id"]),
    message: "Fulfillment service could not be found.",
    code: None,
  )
}

fn invalid_fulfillment_service_destination_location() -> FulfillmentServiceUserError {
  FulfillmentServiceUserError(
    field: None,
    message: "Invalid destination location.",
    code: None,
  )
}

fn blank_delivery_profile_name_error() -> DeliveryProfileUserError {
  DeliveryProfileUserError(
    field: Some(["profile", "name"]),
    message: "Add a profile name",
  )
}

fn delivery_profile_update_not_found() -> DeliveryProfileUserError {
  DeliveryProfileUserError(
    field: None,
    message: "Profile could not be updated.",
  )
}

fn delivery_profile_remove_not_found() -> DeliveryProfileUserError {
  DeliveryProfileUserError(
    field: None,
    message: "The Delivery Profile cannot be found for the shop.",
  )
}

fn delivery_profile_default_remove_error() -> DeliveryProfileUserError {
  DeliveryProfileUserError(
    field: None,
    message: "Cannot delete the default profile.",
  )
}

fn normalize_fulfillment_service_handle(name: String) -> String {
  name
  |> string.lowercase
  |> string.replace(" ", "-")
}

fn fulfillment_service_location_record(
  service: FulfillmentServiceRecord,
  timestamp: String,
) -> StorePropertyRecord {
  let location_id = service.location_id |> option.unwrap("")
  StorePropertyRecord(
    id: location_id,
    cursor: None,
    data: dict.from_list([
      #("__typename", StorePropertyString("Location")),
      #("id", StorePropertyString(location_id)),
      #("name", StorePropertyString(service.service_name)),
      #("isActive", StorePropertyBool(True)),
      #("isFulfillmentService", StorePropertyBool(True)),
      #("fulfillsOnlineOrders", StorePropertyBool(True)),
      #("shipsInventory", StorePropertyBool(False)),
      #("createdAt", StorePropertyString(timestamp)),
      #("updatedAt", StorePropertyString(timestamp)),
      #(
        "fulfillmentService",
        StorePropertyObject(fulfillment_service_location_reference(service)),
      ),
    ]),
  )
}

fn fulfillment_service_location_reference(
  service: FulfillmentServiceRecord,
) -> Dict(String, StorePropertyValue) {
  dict.from_list([
    #("__typename", StorePropertyString("FulfillmentService")),
    #("id", StorePropertyString(service.id)),
    #("handle", StorePropertyString(service.handle)),
    #("serviceName", StorePropertyString(service.service_name)),
    #("callbackUrl", optional_string_store_property(service.callback_url)),
    #("inventoryManagement", StorePropertyBool(service.inventory_management)),
    #("locationId", optional_string_store_property(service.location_id)),
    #(
      "requiresShippingMethod",
      StorePropertyBool(service.requires_shipping_method),
    ),
    #("trackingSupport", StorePropertyBool(service.tracking_support)),
    #("type", StorePropertyString(service.type_)),
  ])
}

fn optional_string_store_property(value: Option(String)) -> StorePropertyValue {
  case value {
    Some(value) -> StorePropertyString(value)
    None -> StorePropertyNull
  }
}

fn strip_query_from_gid(id: String) -> String {
  case string.split(id, "?") |> list.first {
    Ok(value) -> value
    Error(_) -> id
  }
}

fn local_pickup_location_not_found(
  field: String,
  location_id: Option(String),
) -> LocalPickupUserError {
  let legacy_id = case location_id {
    Some(id) -> carrier_service_numeric_id(id)
    None -> ""
  }
  LocalPickupUserError(
    field: Some([field]),
    message: "Unable to find an active location for location ID " <> legacy_id,
    code: Some("ACTIVE_LOCATION_NOT_FOUND"),
  )
}

fn invalid_shipping_package_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    MutationFieldResult(
      key: key,
      payload: json.null(),
      errors: [shipping_package_invalid_id_error(key)],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn shipping_package_invalid_id_error(key: String) -> Json {
  json.object([
    #("message", json.string("invalid id")),
    #("path", json.array([key], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
}

fn option_to_source(value: Option(String)) -> SourceValue {
  case value {
    Some(string) -> SrcString(string)
    None -> SrcNull
  }
}

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(string) -> json.string(string)
    None -> json.null()
  }
}

fn fulfillment_order_single_payload_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  fulfillment_order: FulfillmentOrderRecord,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let #(staged, next_store) =
    store.stage_upsert_fulfillment_order(draft_store, fulfillment_order)
  #(
    MutationFieldResult(
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

fn fulfillment_event_payload_json(
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

fn fulfillment_event_missing_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    MutationFieldResult(
      key: key,
      payload: fulfillment_event_payload_json(field, fragments, None, [
        src_object([
          #(
            "field",
            SrcList([SrcString("fulfillmentEvent"), SrcString("fulfillmentId")]),
          ),
          #("message", SrcString("Fulfillment does not exist.")),
        ]),
      ]),
      errors: [],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

fn reverse_delivery_payload_json(
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

fn reverse_delivery_missing_rfo_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
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

fn reverse_delivery_missing_delivery_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
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

fn order_edit_shipping_line_payload_json(
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

fn order_edit_calculated_order_missing_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
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

fn order_edit_shipping_line_invalid_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
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

fn plain_user_error_source(
  field: List(String),
  message: String,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
  ])
}

fn fulfillment_order_missing_mutation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    MutationFieldResult(
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

fn fulfillment_order_user_error_payload(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  message: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let user_error =
    src_object([
      #("field", SrcNull),
      #("message", SrcString(message)),
    ])
  #(
    MutationFieldResult(
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

fn optional_fulfillment_order_source(
  fulfillment_order: Option(FulfillmentOrderRecord),
) -> SourceValue {
  case fulfillment_order {
    Some(record) -> fulfillment_order_source(record)
    None -> SrcNull
  }
}

fn fulfillment_event_value(
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("status", option_to_captured_string(read_string(input, "status"))),
    #("message", option_to_captured_string(read_string(input, "message"))),
    #("happenedAt", option_to_captured_string(read_string(input, "happenedAt"))),
    #("createdAt", CapturedString(synthetic_timestamp_string())),
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

fn update_fulfillment_for_event(
  fulfillment: FulfillmentRecord,
  event: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> FulfillmentRecord {
  let events =
    list.append(captured_array_field(fulfillment.data, "events", "nodes"), [
      event,
    ])
  let updates = [
    #("events", captured_event_connection(events)),
    #("displayStatus", option_to_captured_string(read_string(input, "status"))),
    #(
      "estimatedDeliveryAt",
      option_to_captured_string(read_string(input, "estimatedDeliveryAt")),
    ),
  ]
  let updates = case
    read_string(input, "status"),
    read_string(input, "happenedAt")
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

fn captured_event_connection(
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

fn captured_event_cursor(event: CapturedJsonValue) -> CapturedJsonValue {
  case captured_string_field(event, "id") {
    Some(id) -> CapturedString("cursor:" <> id)
    None -> CapturedNull
  }
}

fn find_unsubmitted_sibling_fulfillment_order(
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

fn fulfillment_order_merchant_request(
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

fn fulfillment_hold_value(
  id: String,
  handle: Option(String),
  reason: Option(String),
  reason_notes: Option(String),
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("handle", option_to_captured_string(handle)),
    #("reason", option_to_captured_string(reason)),
    #("reasonNotes", option_to_captured_string(reason_notes)),
    #("displayReason", CapturedString("Other")),
    #("heldByApp", CapturedNull),
    #("heldByRequestingApp", CapturedBool(True)),
  ])
}

fn first_fulfillment_order_line_item_quantity(
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

fn first_fulfillment_order_line_item_total(data: CapturedJsonValue) -> Int {
  case captured_array_field(data, "lineItems", "nodes") {
    [first, ..] ->
      captured_int_field(first, "totalQuantity", "") |> option.unwrap(1)
    _ -> 1
  }
}

fn fulfillment_order_line_items_with_quantity(
  data: CapturedJsonValue,
  quantity: Int,
  update_line_item_fulfillable: Bool,
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.map(fn(node) {
      let line_item = case captured_field(node, "lineItem") {
        Some(value) -> {
          case update_line_item_fulfillable {
            True ->
              captured_upsert_fields(value, [
                #("fulfillableQuantity", CapturedInt(quantity)),
              ])
            False -> value
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

fn captured_action_list(actions: List(String)) -> CapturedJsonValue {
  actions
  |> list.map(fn(action) {
    CapturedObject([#("action", CapturedString(action))])
  })
  |> CapturedArray
}

fn assigned_location_value(location_id: Option(String)) -> CapturedJsonValue {
  let id = location_id |> option.unwrap("")
  let name = case id {
    "gid://shopify/Location/106318430514" -> "Shop location"
    "" -> ""
    _ -> "My Custom Location"
  }
  CapturedObject([
    #("name", CapturedString(name)),
    #(
      "location",
      CapturedObject([
        #("id", CapturedString(id)),
        #("name", CapturedString(name)),
      ]),
    ),
  ])
}

fn sibling_fulfillment_order_quantity(
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

fn close_sibling_fulfillment_orders(
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

fn close_merge_siblings(
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

fn fulfillment_order_merge_ids(
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

fn fulfillment_order_split_ids(
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  read_object_array(args, "fulfillmentOrderSplits")
  |> list.filter_map(fn(input) {
    read_string(input, "fulfillmentOrderId") |> option.to_result(Nil)
  })
}

fn synthetic_timestamp_string() -> String {
  "2026-04-28T02:25:00Z"
}

fn normalize_shopify_timestamp_to_seconds(value: String) -> String {
  case string.split_once(value, ".") {
    Ok(#(prefix, suffix)) ->
      case string.ends_with(suffix, "Z") {
        True -> prefix <> "Z"
        False -> value
      }
    Error(_) -> value
  }
}

fn update_shipping_order_display_status(
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

fn update_shipping_order_display_status_value(
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

fn max_int(left: Int, right: Int) -> Int {
  case left < right {
    True -> right
    False -> left
  }
}

fn update_fulfillment_order_fields(
  order: FulfillmentOrderRecord,
  updates: List(#(String, CapturedJsonValue)),
) -> FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    ..order,
    data: captured_upsert_fields(order.data, updates),
  )
}

fn zero_fulfillment_order_line_items(
  data: CapturedJsonValue,
  line_item_fulfillable: Option(Int),
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.map(fn(node) {
      let line_item = case
        line_item_fulfillable,
        captured_field(node, "lineItem")
      {
        Some(quantity), Some(value) ->
          captured_upsert_fields(value, [
            #("fulfillableQuantity", CapturedInt(quantity)),
          ])
        _, Some(value) -> value
        _, None -> CapturedNull
      }
      captured_upsert_fields(node, [
        #("totalQuantity", CapturedInt(0)),
        #("remainingQuantity", CapturedInt(0)),
        #("lineItem", line_item),
      ])
    })
  captured_connection(nodes)
}

fn apply_package_input(
  draft_store: Store,
  current: ShippingPackageRecord,
  input: Dict(String, root_field.ResolvedValue),
  updated_at: String,
) -> #(ShippingPackageRecord, Store) {
  let requested_default = read_bool(input, "default")
  let updated =
    ShippingPackageRecord(
      ..current,
      name: read_string(input, "name") |> option.or(current.name),
      type_: read_string(input, "type") |> option.or(current.type_),
      default: requested_default |> option.unwrap(current.default),
      weight: read_weight(input, "weight") |> option.or(current.weight),
      dimensions: read_dimensions(input, "dimensions")
        |> option.or(current.dimensions),
      updated_at: updated_at,
    )
  case requested_default {
    Some(True) -> {
      let packages = store.list_effective_shipping_packages(draft_store)
      let cleared_store =
        list.fold(packages, draft_store, fn(current_store, shipping_package) {
          case shipping_package.id == updated.id || !shipping_package.default {
            True -> current_store
            False -> {
              let cleared =
                ShippingPackageRecord(
                  ..shipping_package,
                  default: False,
                  updated_at: updated_at,
                )
              let #(_, next_store) =
                store.stage_update_shipping_package(current_store, cleared)
              next_store
            }
          }
        })
      #(updated, cleared_store)
    }
    _ -> #(updated, draft_store)
  }
}

fn resolved_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> args
    Error(_) -> dict.new()
  }
}

fn read_string(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(fields, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_trimmed_string(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case read_string(fields, key) {
    Some(value) -> Some(string.trim(value))
    None -> None
  }
}

fn read_carrier_service_callback_url(
  fields: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(fields, "callbackUrl") {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

fn read_fulfillment_service_callback_url(
  fields: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(fields, "callbackUrl") {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

fn read_bool(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(fields, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn bool_string(value: Bool) -> String {
  case value {
    True -> "true"
    False -> "false"
  }
}

fn store_property_bool_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(Bool) {
  case dict.get(record.data, key) {
    Ok(StorePropertyBool(value)) -> Some(value)
    _ -> None
  }
}

fn store_property_string_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(String) {
  case dict.get(record.data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

fn read_float(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Float) {
  case dict.get(fields, key) {
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    _ -> None
  }
}

fn read_int(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Int) {
  case dict.get(fields, key) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_number(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(CapturedJsonValue) {
  case dict.get(fields, key) {
    Ok(root_field.IntVal(value)) -> Some(CapturedInt(value))
    Ok(root_field.FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
  }
}

fn read_object(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(fields, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_string_array(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(fields, key) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.StringVal(string) -> Ok(string)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_object_array(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(fields, key) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.ObjectVal(object) -> Ok(object)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn selected_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn unique_strings(values: List(String)) -> List(String) {
  list.fold(values, [], fn(seen, value) {
    case list.contains(seen, value) {
      True -> seen
      False -> list.append(seen, [value])
    }
  })
}

fn count_delivery_profile_locations_to_add(
  input: Dict(String, root_field.ResolvedValue),
) -> Int {
  input
  |> read_object_array("locationGroupsToUpdate")
  |> list.flat_map(fn(group) { read_string_array(group, "locationsToAdd") })
  |> unique_strings
  |> list.length
}

fn delivery_profile_active_method_delta(
  input: Dict(String, root_field.ResolvedValue),
) -> Int {
  let zone_updates =
    input
    |> read_object_array("locationGroupsToUpdate")
    |> list.flat_map(fn(group) { read_object_array(group, "zonesToUpdate") })
  let created_active =
    zone_updates
    |> list.flat_map(fn(zone) {
      read_object_array(zone, "methodDefinitionsToCreate")
    })
    |> list.filter(fn(method) {
      read_bool(method, "active") |> option.unwrap(True)
    })
    |> list.length
  let deactivated =
    zone_updates
    |> list.flat_map(fn(zone) {
      read_object_array(zone, "methodDefinitionsToUpdate")
    })
    |> list.filter(fn(method) { read_bool(method, "active") == Some(False) })
    |> list.length
  created_active - deactivated
}

fn normalize_money_amount(value: String) -> String {
  case string.ends_with(value, ".00") {
    True -> string.drop_end(value, 3)
    False ->
      case string.ends_with(value, "0") && string.contains(value, ".") {
        True -> normalize_money_amount(string.drop_end(value, 1))
        False -> value
      }
  }
}

fn captured_upsert_fields(
  value: CapturedJsonValue,
  updates: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  let update_keys = list.map(updates, fn(pair) { pair.0 })
  let existing = case value {
    CapturedObject(fields) ->
      list.filter(fields, fn(pair) { !list.contains(update_keys, pair.0) })
    _ -> []
  }
  CapturedObject(list.append(existing, updates))
}

fn captured_string_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

fn captured_bool_field(value: CapturedJsonValue, key: String) -> Option(Bool) {
  case captured_field(value, key) {
    Some(CapturedBool(value)) -> Some(value)
    _ -> None
  }
}

fn captured_int_field(
  value: CapturedJsonValue,
  key: String,
  nested_key: String,
) -> Option(Int) {
  let source = case nested_key {
    "" -> captured_field(value, key)
    _ ->
      case captured_field(value, key) {
        Some(nested) -> captured_field(nested, nested_key)
        None -> None
      }
  }
  case source {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

fn captured_array_field(
  value: CapturedJsonValue,
  key: String,
  nested_key: String,
) -> List(CapturedJsonValue) {
  let source = case nested_key {
    "" -> captured_field(value, key)
    _ ->
      case captured_field(value, key) {
        Some(nested) -> captured_field(nested, nested_key)
        None -> None
      }
  }
  case source {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

fn captured_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find(fn(pair) { pair.0 == key })
      |> option.from_result
      |> option.map(fn(pair) { pair.1 })
    _ -> None
  }
}

fn option_to_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn make_reverse_delivery_line_items(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let available =
    captured_array_field(reverse_fulfillment_order.data, "lineItems", "nodes")
  let requested = case inputs {
    [] ->
      list.map(available, fn(line_item) {
        #(
          line_item,
          captured_int_field(line_item, "remainingQuantity", "")
            |> option.unwrap(
              captured_int_field(line_item, "totalQuantity", "")
              |> option.unwrap(1),
            ),
        )
      })
    _ ->
      inputs
      |> list.filter_map(fn(input) {
        use line_item_id <- result.try(
          read_string(input, "reverseFulfillmentOrderLineItemId")
          |> option.to_result(Nil),
        )
        use line_item <- result.try(find_captured_line_item(
          available,
          line_item_id,
        ))
        Ok(#(line_item, read_int(input, "quantity") |> option.unwrap(1)))
      })
  }
  list.fold(requested, #([], identity), fn(acc, pair) {
    let #(items, current_identity) = acc
    let #(line_item, quantity) = pair
    let #(delivery_line_item, next_identity) =
      make_reverse_delivery_line_item(line_item, quantity, current_identity)
    #(list.append(items, [delivery_line_item]), next_identity)
  })
}

fn make_reverse_delivery_line_item(
  reverse_fulfillment_order_line_item: CapturedJsonValue,
  quantity: Int,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "ReverseDeliveryLineItem")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("quantity", CapturedInt(quantity)),
      #("reverseFulfillmentOrderLineItem", reverse_fulfillment_order_line_item),
    ]),
    next_identity,
  )
}

fn reverse_delivery_value(
  id: String,
  args: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("reverseDeliveryLineItems", captured_connection(line_items)),
    #("deliverable", reverse_delivery_deliverable(args)),
  ])
}

fn append_reverse_delivery(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  reverse_delivery: ReverseDeliveryRecord,
) -> ReverseFulfillmentOrderRecord {
  let existing =
    captured_array_field(
      reverse_fulfillment_order.data,
      "reverseDeliveries",
      "nodes",
    )
  ReverseFulfillmentOrderRecord(
    ..reverse_fulfillment_order,
    data: captured_upsert_fields(reverse_fulfillment_order.data, [
      #(
        "reverseDeliveries",
        captured_connection(list.append([reverse_delivery.data], existing)),
      ),
    ]),
  )
}

fn update_reverse_delivery_shipping(
  data: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let existing_deliverable =
    captured_field(data, "deliverable") |> option.unwrap(CapturedObject([]))
  let tracking = case reverse_delivery_tracking(args) {
    CapturedNull -> captured_field(existing_deliverable, "tracking")
    other -> Some(other)
  }
  let label = case reverse_delivery_label(args) {
    CapturedNull -> captured_field(existing_deliverable, "label")
    other -> Some(other)
  }
  captured_upsert_fields(data, [
    #(
      "deliverable",
      CapturedObject([
        #("__typename", CapturedString("ReverseDeliveryShippingDeliverable")),
        #("tracking", tracking |> option.unwrap(CapturedNull)),
        #("label", label |> option.unwrap(CapturedNull)),
      ]),
    ),
  ])
}

fn reverse_delivery_deliverable(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("ReverseDeliveryShippingDeliverable")),
    #("tracking", reverse_delivery_tracking(args)),
    #("label", reverse_delivery_label(args)),
  ])
}

fn reverse_delivery_tracking(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_object(args, "trackingInput") {
    Some(input) ->
      CapturedObject([
        #("number", option_to_captured_string(read_string(input, "number"))),
        #("url", option_to_captured_string(read_string(input, "url"))),
        #("company", option_to_captured_string(read_string(input, "company"))),
      ])
    None -> CapturedNull
  }
}

fn reverse_delivery_label(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_object(args, "labelInput") {
    Some(input) ->
      CapturedObject([
        #(
          "publicFileUrl",
          option_to_captured_string(read_string(input, "publicFileUrl")),
        ),
      ])
    None -> CapturedNull
  }
}

fn find_reverse_fulfillment_order_line_item(
  draft_store: Store,
  line_item_id: String,
) -> Option(#(ReverseFulfillmentOrderRecord, CapturedJsonValue)) {
  store.list_effective_reverse_fulfillment_orders(draft_store)
  |> list.find_map(fn(reverse_fulfillment_order) {
    case
      find_captured_line_item(
        captured_array_field(
          reverse_fulfillment_order.data,
          "lineItems",
          "nodes",
        ),
        line_item_id,
      )
    {
      Ok(line_item) -> Ok(#(reverse_fulfillment_order, line_item))
      Error(_) -> Error(Nil)
    }
  })
  |> option.from_result
}

fn find_captured_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Result(CapturedJsonValue, Nil) {
  list.find(line_items, fn(line_item) { reverse_line_item_id(line_item) == id })
}

fn dispose_reverse_fulfillment_order_line_item(
  line_item: CapturedJsonValue,
  quantity: Int,
  disposition_type: String,
) -> CapturedJsonValue {
  let remaining =
    captured_int_field(line_item, "remainingQuantity", "") |> option.unwrap(0)
  let next_remaining = case remaining - quantity < 0 {
    True -> 0
    False -> remaining - quantity
  }
  captured_upsert_fields(line_item, [
    #("remainingQuantity", CapturedInt(next_remaining)),
    #("dispositionType", CapturedString(disposition_type)),
  ])
}

fn update_reverse_fulfillment_order_line_item(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  updated_line_item: CapturedJsonValue,
) -> ReverseFulfillmentOrderRecord {
  let line_items =
    captured_array_field(reverse_fulfillment_order.data, "lineItems", "nodes")
    |> list.map(fn(line_item) {
      case
        reverse_line_item_id(line_item)
        == reverse_line_item_id(updated_line_item)
      {
        True -> updated_line_item
        False -> line_item
      }
    })
  ReverseFulfillmentOrderRecord(
    ..reverse_fulfillment_order,
    data: captured_upsert_fields(reverse_fulfillment_order.data, [
      #("lineItems", captured_connection(line_items)),
    ]),
  )
}

fn make_calculated_shipping_line(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> Option(#(CapturedJsonValue, SyntheticIdentityRegistry)) {
  case read_string(input, "title"), read_money_input(input, "price") {
    Some(title), Some(#(amount, currency_code)) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          identity,
          "CalculatedShippingLine",
        )
      Some(#(
        CapturedObject([
          #("id", CapturedString(id)),
          #("title", CapturedString(title)),
          #("code", CapturedString(title)),
          #("stagedStatus", CapturedString("ADDED")),
          #("price", money_set(amount, currency_code)),
          #("originalPriceSet", money_set(amount, currency_code)),
        ]),
        next_identity,
      ))
    }
    _, _ -> None
  }
}

fn update_calculated_shipping_line(
  line: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let title = read_string(input, "title")
  let price = read_money_input(input, "price")
  let updates =
    []
    |> append_optional_captured("title", option.map(title, CapturedString))
    |> append_optional_captured("code", option.map(title, CapturedString))
    |> append_optional_captured(
      "price",
      option.map(price, fn(price) { money_set(price.0, price.1) }),
    )
    |> append_optional_captured(
      "originalPriceSet",
      option.map(price, fn(price) { money_set(price.0, price.1) }),
    )
    |> list.append([
      #(
        "stagedStatus",
        CapturedString(case captured_string_field(line, "stagedStatus") {
          Some("ADDED") -> "ADDED"
          _ -> "UPDATED"
        }),
      ),
    ])
  captured_upsert_fields(line, updates)
}

fn update_calculated_order_shipping_lines(
  calculated_order: CalculatedOrderRecord,
  shipping_lines: List(CapturedJsonValue),
) -> CalculatedOrderRecord {
  let data_with_lines =
    captured_upsert_fields(calculated_order.data, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let data_with_total =
    update_calculated_order_total(data_with_lines, shipping_lines)
  CalculatedOrderRecord(..calculated_order, data: data_with_total)
}

fn calculated_order_shipping_lines(
  calculated_order: CalculatedOrderRecord,
) -> List(CapturedJsonValue) {
  captured_array_field(calculated_order.data, "shippingLines", "")
}

fn update_calculated_order_total(
  data: CapturedJsonValue,
  shipping_lines: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let base_amount =
    captured_money_amount(data, "subtotalPriceSet")
    |> option.unwrap(
      captured_money_amount(data, "subtotalLineItemsPriceSet")
      |> option.unwrap(0.0),
    )
  let shipping_amount =
    list.fold(shipping_lines, 0.0, fn(sum, line) {
      sum +. { captured_money_amount(line, "price") |> option.unwrap(0.0) }
    })
  let currency_code =
    captured_money_currency(data, "totalPriceSet")
    |> option.unwrap(
      first_shipping_line_currency(shipping_lines) |> option.unwrap("USD"),
    )
  captured_upsert_fields(data, [
    #(
      "totalPriceSet",
      money_set(
        float.to_string(base_amount +. { shipping_amount }),
        currency_code,
      ),
    ),
  ])
}

fn read_money_input(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(#(String, String)) {
  case read_object(fields, key) {
    Some(input) ->
      case read_string(input, "amount") {
        Some(amount) -> {
          let currency_code =
            read_string(input, "currencyCode") |> option.unwrap("USD")
          Some(#(format_money_amount(amount), currency_code))
        }
        None -> None
      }
    None -> None
  }
}

fn money_set(amount: String, currency_code: String) -> CapturedJsonValue {
  let money =
    CapturedObject([
      #("amount", CapturedString(format_money_amount(amount))),
      #("currencyCode", CapturedString(currency_code)),
    ])
  CapturedObject([
    #("shopMoney", money),
    #("presentmentMoney", money),
  ])
}

fn captured_money_amount(
  value: CapturedJsonValue,
  key: String,
) -> Option(Float) {
  case captured_field(value, key) {
    Some(money_set) ->
      case captured_field(money_set, "shopMoney") {
        Some(shop_money) ->
          case captured_string_field(shop_money, "amount") {
            Some(amount) ->
              case float.parse(amount) {
                Ok(value) -> Some(value)
                Error(_) -> None
              }
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

fn captured_money_currency(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(money_set) ->
      case captured_field(money_set, "shopMoney") {
        Some(shop_money) -> captured_string_field(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  }
}

fn first_shipping_line_currency(
  shipping_lines: List(CapturedJsonValue),
) -> Option(String) {
  shipping_lines
  |> list.find_map(fn(line) {
    captured_money_currency(line, "price") |> option.to_result(Nil)
  })
  |> option.from_result
}

fn format_money_amount(amount: String) -> String {
  case float.parse(amount) {
    Ok(value) -> float.to_string(value)
    Error(_) -> amount
  }
}

fn append_optional_captured(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
  value: Option(CapturedJsonValue),
) -> List(#(String, CapturedJsonValue)) {
  case value {
    Some(value) -> list.append(fields, [#(key, value)])
    None -> fields
  }
}

fn reverse_line_item_id(value: CapturedJsonValue) -> String {
  captured_string_field(value, "id") |> option.unwrap("")
}

fn read_weight(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(ShippingPackageWeightRecord) {
  case read_object(fields, key) {
    Some(weight) ->
      Some(ShippingPackageWeightRecord(
        value: read_float(weight, "value"),
        unit: read_string(weight, "unit"),
      ))
    None -> None
  }
}

fn read_dimensions(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(ShippingPackageDimensionsRecord) {
  case read_object(fields, key) {
    Some(dimensions) ->
      Some(ShippingPackageDimensionsRecord(
        length: read_float(dimensions, "length"),
        width: read_float(dimensions, "width"),
        height: read_float(dimensions, "height"),
        unit: read_string(dimensions, "unit"),
      ))
    None -> None
  }
}

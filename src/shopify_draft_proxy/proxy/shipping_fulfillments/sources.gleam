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
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull, SrcObject,
  SrcString, get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  store_property_bool_field,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CalculatedOrderRecord, type CapturedJsonValue, type DeliveryProfileRecord,
  type FulfillmentOrderRecord, type FulfillmentRecord,
  type FulfillmentServiceRecord, type ReverseDeliveryRecord,
  type ReverseFulfillmentOrderRecord, type ShippingOrderRecord,
  type ShippingPackageRecord, type StorePropertyRecord, type StorePropertyValue,
  CapturedArray, CapturedBool, CapturedFloat, CapturedInt, CapturedNull,
  CapturedObject, CapturedString, StorePropertyBool, StorePropertyFloat,
  StorePropertyInt, StorePropertyList, StorePropertyNull, StorePropertyObject,
  StorePropertyRecord, StorePropertyString,
}

@internal
pub fn sort_store_property_locations_by_id(
  locations: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  list.sort(locations, fn(left, right) {
    resource_ids.compare_shopify_resource_ids(left.id, right.id)
  })
}

@internal
pub fn filter_active_non_fulfillment_locations(
  locations: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  locations
  |> list.filter(fn(location) {
    is_active_location(location) && !is_fulfillment_service_location(location)
  })
}

@internal
pub fn find_active_store_property_location(
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

@internal
pub fn is_active_location(location: StorePropertyRecord) -> Bool {
  store_property_bool_field(location, "isActive") |> option.unwrap(True)
}

@internal
pub fn is_fulfillment_service_location(location: StorePropertyRecord) -> Bool {
  store_property_bool_field(location, "isFulfillmentService")
  |> option.unwrap(False)
}

@internal
pub fn delivery_profile_source(profile: DeliveryProfileRecord) -> SourceValue {
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

@internal
pub fn annotate_delivery_profile_source(value: SourceValue) -> SourceValue {
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

@internal
pub fn infer_delivery_profile_typename(
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

@internal
pub fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
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

@internal
pub fn fulfillment_source(fulfillment: FulfillmentRecord) -> SourceValue {
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

@internal
pub fn fulfillment_order_source(order: FulfillmentOrderRecord) -> SourceValue {
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

@internal
pub fn fulfillment_event_source(event: CapturedJsonValue) -> SourceValue {
  captured_json_source(event)
}

@internal
pub fn shipping_order_source(
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

@internal
pub fn reverse_delivery_source(
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

@internal
pub fn optional_reverse_fulfillment_order_source(
  store: Store,
  reverse_fulfillment_order: Option(ReverseFulfillmentOrderRecord),
) -> SourceValue {
  case reverse_fulfillment_order {
    Some(record) -> reverse_fulfillment_order_source(store, record)
    None -> SrcNull
  }
}

@internal
pub fn reverse_fulfillment_order_source(
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

@internal
pub fn reverse_delivery_source_without_parent(
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

@internal
pub fn calculated_order_source(
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

@internal
pub fn source_connection(nodes: List(SourceValue)) -> SourceValue {
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

@internal
pub fn optional_store_property_source(
  value: Option(StorePropertyValue),
) -> SourceValue {
  case value {
    Some(value) -> store_property_value_to_source(value)
    None -> SrcNull
  }
}

@internal
pub fn store_property_record_source(
  record: StorePropertyRecord,
) -> SourceValue {
  store_property_data_to_source(record.data)
}

@internal
pub fn store_property_value_to_source(
  value: StorePropertyValue,
) -> SourceValue {
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

@internal
pub fn store_property_data_to_source(
  data: Dict(String, StorePropertyValue),
) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, store_property_value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

@internal
pub fn carrier_service_user_error_source(
  error: shipping_types.CarrierServiceUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("shipping_types.CarrierServiceUserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", SrcString(error.code)),
  ])
}

@internal
pub fn fulfillment_service_user_error_source(
  error: shipping_types.FulfillmentServiceUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", option_to_source(error.code)),
  ])
}

@internal
pub fn delivery_profile_user_error_source(
  error: shipping_types.DeliveryProfileUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", option_to_source(error.code)),
  ])
}

@internal
pub fn local_pickup_user_error_source(
  error: shipping_types.LocalPickupUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("DeliveryLocationLocalPickupSettingsError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", option_to_source(error.code)),
  ])
}

@internal
pub fn shipping_package_update_user_error_source(
  error: shipping_types.ShippingPackageUpdateUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("shipping_types.ShippingPackageUpdateUserError")),
    #("field", optional_string_list_source(error.field)),
    #("message", SrcString(error.message)),
    #("code", SrcString(error.code)),
  ])
}

@internal
pub fn optional_string_list_source(value: Option(List(String))) -> SourceValue {
  case value {
    Some(items) -> SrcList(list.map(items, SrcString))
    None -> SrcNull
  }
}

@internal
pub fn carrier_service_formatted_name(name: Option(String)) -> Option(String) {
  case name {
    Some(value) -> Some(value <> " (Rates provided by app)")
    None -> None
  }
}

@internal
pub fn carrier_service_numeric_id(id: String) -> String {
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

@internal
pub fn validate_carrier_service_name(
  name: Option(String),
) -> List(shipping_types.CarrierServiceUserError) {
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> blank_carrier_service_name_errors()
        _ -> []
      }
    None -> blank_carrier_service_name_errors()
  }
}

@internal
pub fn blank_carrier_service_name_errors() -> List(
  shipping_types.CarrierServiceUserError,
) {
  [
    shipping_types.CarrierServiceUserError(
      field: None,
      message: "Shipping rate provider name can't be blank",
      code: "CARRIER_SERVICE_CREATE_FAILED",
    ),
  ]
}

@internal
pub fn validate_carrier_service_create_callback_url(
  callback_url: Option(String),
) -> List(shipping_types.CarrierServiceUserError) {
  validate_carrier_service_callback_url(
    callback_url,
    "CARRIER_SERVICE_CREATE_FAILED",
    enforce_https: True,
  )
}

@internal
pub fn validate_carrier_service_update_callback_url(
  callback_url: Option(String),
  existing_callback_url: Option(String),
) -> List(shipping_types.CarrierServiceUserError) {
  validate_carrier_service_callback_url(
    callback_url,
    "CARRIER_SERVICE_UPDATE_FAILED",
    enforce_https: carrier_service_callback_url_meaningfully_changed(
      callback_url,
      existing_callback_url,
    ),
  )
}

fn validate_carrier_service_callback_url(
  callback_url: Option(String),
  code: String,
  enforce_https enforce_https: Bool,
) -> List(shipping_types.CarrierServiceUserError) {
  case callback_url {
    None -> []
    Some(raw_url) -> {
      let url = string.trim(raw_url)
      let scheme = carrier_service_callback_url_scheme(url)
      let host = carrier_service_callback_url_host(url)
      case host == "" {
        True -> []
        False ->
          case carrier_service_callback_url_host_is_banned(host) {
            True -> [carrier_service_callback_url_invalid_host_error(code)]
            False ->
              case enforce_https && scheme != "https" {
                True -> [carrier_service_callback_url_https_error(code)]
                False -> []
              }
          }
      }
    }
  }
}

fn carrier_service_callback_url_meaningfully_changed(
  callback_url: Option(String),
  existing_callback_url: Option(String),
) -> Bool {
  case callback_url, existing_callback_url {
    Some(next), Some(existing) ->
      normalize_carrier_service_callback_url(next)
      != normalize_carrier_service_callback_url(existing)
    Some(_), None -> True
    None, Some(_) -> True
    None, None -> False
  }
}

fn normalize_carrier_service_callback_url(url: String) -> String {
  let trimmed = string.trim(url)
  let lower = string.lowercase(trimmed)
  let scheme = carrier_service_callback_url_scheme(lower)
  case scheme {
    "http" -> remove_default_port(lower, "http://", ":80")
    "https" -> remove_default_port(lower, "https://", ":443")
    _ -> lower
  }
}

fn remove_default_port(url: String, prefix: String, port: String) -> String {
  let after_scheme = string.drop_start(url, string.length(prefix))
  let authority = carrier_service_callback_url_authority(after_scheme)
  case string.ends_with(authority, port) {
    True ->
      prefix
      <> string.drop_end(authority, string.length(port))
      <> string.drop_start(after_scheme, string.length(authority))
    False -> url
  }
}

fn carrier_service_callback_url_scheme(url: String) -> String {
  case string.split_once(url, "://") {
    Ok(#(scheme, _)) -> string.lowercase(scheme)
    Error(_) -> ""
  }
}

fn carrier_service_callback_url_host(url: String) -> String {
  case string.split_once(url, "://") {
    Ok(#(_, after_scheme)) ->
      after_scheme
      |> carrier_service_callback_url_authority
      |> carrier_service_callback_url_host_without_userinfo
      |> carrier_service_callback_url_host_without_port
      |> carrier_service_callback_url_trim_trailing_dot
      |> string.lowercase
    Error(_) -> ""
  }
}

fn carrier_service_callback_url_authority(after_scheme: String) -> String {
  after_scheme
  |> string_before("/")
  |> string_before("?")
  |> string_before("#")
}

fn string_before(value: String, delimiter: String) -> String {
  case string.split(value, on: delimiter) {
    [head, ..] -> head
    [] -> value
  }
}

fn carrier_service_callback_url_host_without_userinfo(
  authority: String,
) -> String {
  case string.split(authority, on: "@") |> list.last {
    Ok(host) -> host
    Error(_) -> authority
  }
}

fn carrier_service_callback_url_host_without_port(host_port: String) -> String {
  case string.starts_with(host_port, "[") {
    True ->
      case string.drop_start(host_port, 1) |> string.split_once("]") {
        Ok(#(host, _)) -> host
        Error(_) -> host_port
      }
    False -> string_before(host_port, ":")
  }
}

fn carrier_service_callback_url_trim_trailing_dot(host: String) -> String {
  case string.ends_with(host, ".") {
    True ->
      carrier_service_callback_url_trim_trailing_dot(string.drop_end(host, 1))
    False -> host
  }
}

fn carrier_service_callback_url_host_is_banned(host: String) -> Bool {
  host == "localhost"
  || carrier_service_callback_url_domain_matches(host, "shopify.com")
  || carrier_service_callback_url_domain_matches(host, "myshopify.com")
  || carrier_service_callback_url_domain_matches(host, "shopifypreview.com")
  || carrier_service_callback_url_domain_matches(host, "myshopify.dev")
  || carrier_service_callback_url_is_banned_ipv4(host)
}

fn carrier_service_callback_url_domain_matches(
  host: String,
  domain: String,
) -> Bool {
  host == domain || string.ends_with(host, "." <> domain)
}

fn carrier_service_callback_url_is_banned_ipv4(host: String) -> Bool {
  case parse_ipv4(host) {
    Some(#(first, second, _, _)) ->
      first == 0
      || first == 10
      || first == 127
      || { first == 192 && second == 168 }
      || { first == 172 && second >= 16 && second <= 31 }
    None -> False
  }
}

fn parse_ipv4(host: String) -> Option(#(Int, Int, Int, Int)) {
  case string.split(host, on: ".") {
    [a, b, c, d] ->
      case int.parse(a), int.parse(b), int.parse(c), int.parse(d) {
        Ok(first), Ok(second), Ok(third), Ok(fourth) ->
          case
            valid_ipv4_octet(first)
            && valid_ipv4_octet(second)
            && valid_ipv4_octet(third)
            && valid_ipv4_octet(fourth)
          {
            True -> Some(#(first, second, third, fourth))
            False -> None
          }
        _, _, _, _ -> None
      }
    _ -> None
  }
}

fn valid_ipv4_octet(value: Int) -> Bool {
  value >= 0 && value <= 255
}

fn carrier_service_callback_url_https_error(
  code: String,
) -> shipping_types.CarrierServiceUserError {
  shipping_types.CarrierServiceUserError(
    field: None,
    message: "Shipping rate provider callback url must use HTTPS",
    code: code,
  )
}

fn carrier_service_callback_url_invalid_host_error(
  code: String,
) -> shipping_types.CarrierServiceUserError {
  shipping_types.CarrierServiceUserError(
    field: None,
    message: "Shipping rate provider callback url invalid host",
    code: code,
  )
}

@internal
pub fn carrier_service_not_found_for_update() -> shipping_types.CarrierServiceUserError {
  shipping_types.CarrierServiceUserError(
    field: None,
    message: "The carrier or app could not be found.",
    code: "CARRIER_SERVICE_UPDATE_FAILED",
  )
}

@internal
pub fn carrier_service_not_found_for_delete() -> shipping_types.CarrierServiceUserError {
  shipping_types.CarrierServiceUserError(
    field: Some(["id"]),
    message: "The carrier or app could not be found.",
    code: "CARRIER_SERVICE_DELETE_FAILED",
  )
}

@internal
pub fn validate_fulfillment_service_name(
  name: Option(String),
) -> List(shipping_types.FulfillmentServiceUserError) {
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> blank_fulfillment_service_name_errors()
        _ -> []
      }
    None -> blank_fulfillment_service_name_errors()
  }
}

@internal
pub fn blank_fulfillment_service_name_errors() -> List(
  shipping_types.FulfillmentServiceUserError,
) {
  [
    shipping_types.FulfillmentServiceUserError(
      field: Some(["name"]),
      message: "Name can't be blank",
      code: None,
    ),
  ]
}

@internal
pub fn validate_fulfillment_service_callback_url(
  callback_url: Option(String),
  upstream_origin: String,
) -> List(shipping_types.FulfillmentServiceUserError) {
  case callback_url {
    Some(value) ->
      fulfillment_service_callback_url_errors(value, upstream_origin)
    None -> []
  }
}

fn fulfillment_service_callback_url_errors(
  callback_url: String,
  upstream_origin: String,
) -> List(shipping_types.FulfillmentServiceUserError) {
  let url = string.trim(callback_url)
  let scheme = carrier_service_callback_url_scheme(url)
  let host = carrier_service_callback_url_host(url)
  case scheme, host {
    "", _ -> [fulfillment_service_callback_url_not_allowed_error()]
    _, "" -> [fulfillment_service_callback_url_not_allowed_error()]
    "http", _ ->
      fulfillment_service_callback_url_host_errors(host, upstream_origin)
    "https", _ ->
      fulfillment_service_callback_url_host_errors(host, upstream_origin)
    _, _ -> [
      shipping_types.FulfillmentServiceUserError(
        field: Some(["callbackUrl"]),
        message: "Callback url protocol " <> scheme <> ":// is not supported",
        code: None,
      ),
    ]
  }
}

fn fulfillment_service_callback_url_host_errors(
  host: String,
  upstream_origin: String,
) -> List(shipping_types.FulfillmentServiceUserError) {
  case
    fulfillment_service_callback_url_host_is_app_scoped(host, upstream_origin)
  {
    True -> []
    False -> [fulfillment_service_callback_url_not_allowed_error()]
  }
}

fn fulfillment_service_callback_url_not_allowed_error() -> shipping_types.FulfillmentServiceUserError {
  shipping_types.FulfillmentServiceUserError(
    field: Some(["callbackUrl"]),
    message: "Callback url is not allowed",
    code: None,
  )
}

@internal
pub fn is_allowed_fulfillment_service_callback_url(
  callback_url: String,
  upstream_origin: String,
) -> Bool {
  fulfillment_service_callback_url_errors(callback_url, upstream_origin) == []
}

fn fulfillment_service_callback_url_host_is_app_scoped(
  host: String,
  upstream_origin: String,
) -> Bool {
  string.starts_with(host, "mock.shop")
  || {
    let upstream_host = carrier_service_callback_url_host(upstream_origin)
    upstream_host != "" && host == upstream_host
  }
}

@internal
pub fn fulfillment_service_not_found() -> shipping_types.FulfillmentServiceUserError {
  shipping_types.FulfillmentServiceUserError(
    field: Some(["id"]),
    message: "Fulfillment service could not be found.",
    code: None,
  )
}

@internal
pub fn invalid_fulfillment_service_destination_location() -> shipping_types.FulfillmentServiceUserError {
  shipping_types.FulfillmentServiceUserError(
    field: None,
    message: "Invalid destination location.",
    code: None,
  )
}

@internal
pub fn fulfillment_service_destination_location_should_not_be_present() -> shipping_types.FulfillmentServiceUserError {
  shipping_types.FulfillmentServiceUserError(
    field: Some(["inventoryAction"]),
    message: "Inventory action Destination location id should not be present when deleting/keeping the inventory of the fulfillment service.",
    code: Some("DESTINATION_LOCATION_ID_SHOULD_NOT_PRESENT"),
  )
}

@internal
pub fn blank_delivery_profile_name_error() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", "name"]),
    message: "Add a profile name",
    code: None,
  )
}

@internal
pub fn blank_delivery_profile_create_name_error() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", "name"]),
    message: "Add a profile name",
    code: Some("PROFILE_CREATE_REQUIRES_NAME"),
  )
}

@internal
pub fn too_long_delivery_profile_create_name_error() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", "name"]),
    message: "Profile name must be less than 128 characters long",
    code: Some("TOO_LONG"),
  )
}

@internal
pub fn unknown_delivery_profile_location_error(
  field_path: String,
) -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", field_path]),
    message: "The Location could not be found for this shop.",
    code: Some("LOCATION_NOT_FOUND"),
  )
}

@internal
pub fn empty_delivery_profile_zone_countries_error(
  field_path: String,
) -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", field_path]),
    message: "Profile is invalid: cannot create LocationGroupZone without countries.",
    code: Some("CANNOT_UPDATE_ZONES"),
  )
}

@internal
pub fn overlapping_delivery_profile_zone_error(
  field_path: String,
) -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: Some(["profile", field_path]),
    message: "Profile is invalid: zones cannot contain overlapping countries.",
    code: Some("CANNOT_UPDATE_ZONES"),
  )
}

@internal
pub fn delivery_profile_update_not_found() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: None,
    message: "Profile could not be updated.",
    code: None,
  )
}

@internal
pub fn delivery_profile_remove_not_found() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: None,
    message: "The Delivery Profile cannot be found for the shop.",
    code: None,
  )
}

@internal
pub fn delivery_profile_default_remove_error() -> shipping_types.DeliveryProfileUserError {
  shipping_types.DeliveryProfileUserError(
    field: None,
    message: "Cannot delete the default profile.",
    code: None,
  )
}

@internal
pub fn normalize_fulfillment_service_handle(name: String) -> String {
  name
  |> string.lowercase
  |> string.replace(" ", "-")
}

@internal
pub fn fulfillment_service_location_record(
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

@internal
pub fn fulfillment_service_location_reference(
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

@internal
pub fn optional_string_store_property(
  value: Option(String),
) -> StorePropertyValue {
  case value {
    Some(value) -> StorePropertyString(value)
    None -> StorePropertyNull
  }
}

@internal
pub fn strip_query_from_gid(id: String) -> String {
  case string.split(id, "?") |> list.first {
    Ok(value) -> value
    Error(_) -> id
  }
}

@internal
pub fn local_pickup_location_not_found(
  field: String,
  location_id: Option(String),
) -> shipping_types.LocalPickupUserError {
  let legacy_id = case location_id {
    Some(id) -> carrier_service_numeric_id(id)
    None -> ""
  }
  shipping_types.LocalPickupUserError(
    field: Some([field]),
    message: "Unable to find an active location for location ID " <> legacy_id,
    code: Some("ACTIVE_LOCATION_NOT_FOUND"),
  )
}

@internal
pub fn local_pickup_custom_pickup_time_not_allowed() -> shipping_types.LocalPickupUserError {
  shipping_types.LocalPickupUserError(
    field: Some(["localPickupSettings"]),
    message: "Custom pickup time is not allowed for local pickup settings.",
    code: Some("CUSTOM_PICKUP_TIME_NOT_ALLOWED"),
  )
}

@internal
pub fn flat_rate_shipping_package_not_updatable() -> shipping_types.ShippingPackageUpdateUserError {
  shipping_types.ShippingPackageUpdateUserError(
    field: Some(["shippingPackage"]),
    message: "Custom shipping box is not updatable",
    code: "CUSTOM_SHIPPING_BOX_NOT_UPDATABLE",
  )
}

@internal
pub fn is_flat_rate_shipping_package(
  shipping_package: ShippingPackageRecord,
) -> Bool {
  shipping_package.box_type == Some("FLAT_RATE")
}

@internal
pub fn invalid_shipping_package_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  #(
    shipping_types.MutationFieldResult(
      key: key,
      payload: json.null(),
      errors: [shipping_package_invalid_id_error(key)],
      staged_resource_ids: [],
    ),
    draft_store,
    identity,
  )
}

@internal
pub fn shipping_package_invalid_id_error(key: String) -> Json {
  json.object([
    #("message", json.string("invalid id")),
    #("path", json.array([key], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
}

@internal
pub fn option_to_source(value: Option(String)) -> SourceValue {
  case value {
    Some(string) -> SrcString(string)
    None -> SrcNull
  }
}

//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  ConnectionPageInfoOptions, SerializeConnectionConfig, SrcBool, SrcList,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  bool_string, captured_array_field, captured_string_field, optional_string_json,
  read_bool, read_string, read_string_array, resolved_args, selected_selections,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  carrier_service_numeric_id, carrier_service_user_error_source,
  delivery_profile_source, delivery_profile_user_error_source,
  fulfillment_order_source, fulfillment_service_user_error_source,
  fulfillment_source, local_pickup_user_error_source, option_to_source,
  optional_store_property_source, reverse_delivery_source,
  reverse_fulfillment_order_source, shipping_order_source,
  shipping_package_update_user_error_source, store_property_record_source,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CarrierServiceRecord, type DeliveryProfileRecord,
  type FulfillmentOrderRecord, type FulfillmentRecord,
  type FulfillmentServiceRecord, type ReverseDeliveryRecord,
  type ReverseFulfillmentOrderRecord, type ShippingOrderRecord,
  type StorePropertyRecord, type StorePropertyValue,
}

@internal
pub fn payload_json(
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

@internal
pub fn shipping_package_update_payload_json(
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(shipping_types.ShippingPackageUpdateUserError),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("ShippingPackageUpdatePayload")),
      #(
        "userErrors",
        SrcList(list.map(user_errors, shipping_package_update_user_error_source)),
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

@internal
pub fn carrier_service_payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  carrier_service: Option(CarrierServiceRecord),
  user_errors: List(shipping_types.CarrierServiceUserError),
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

@internal
pub fn carrier_service_delete_payload_json(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(shipping_types.CarrierServiceUserError),
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

@internal
pub fn fulfillment_service_payload_json(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  fulfillment_service: Option(FulfillmentServiceRecord),
  user_errors: List(shipping_types.FulfillmentServiceUserError),
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

@internal
pub fn fulfillment_service_delete_payload_json(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(shipping_types.FulfillmentServiceUserError),
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

@internal
pub fn delivery_profile_payload_json(
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  profile: Option(DeliveryProfileRecord),
  user_errors: List(shipping_types.DeliveryProfileUserError),
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

@internal
pub fn delivery_profile_remove_payload_json(
  field: Selection,
  fragments: FragmentMap,
  job: Option(#(String, Bool)),
  user_errors: List(shipping_types.DeliveryProfileUserError),
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

@internal
pub fn fulfillment_order_payload_json(
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

@internal
pub fn local_pickup_enable_payload_json(
  field: Selection,
  fragments: FragmentMap,
  settings: Option(StorePropertyValue),
  user_errors: List(shipping_types.LocalPickupUserError),
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

@internal
pub fn local_pickup_disable_payload_json(
  field: Selection,
  fragments: FragmentMap,
  location_id: Option(String),
  user_errors: List(shipping_types.LocalPickupUserError),
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

@internal
pub fn project_carrier_service(
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

@internal
pub fn project_fulfillment_service(
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

@internal
pub fn project_store_property_record(
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

@internal
pub fn project_delivery_profile(
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

@internal
pub fn project_fulfillment(
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

@internal
pub fn project_fulfillment_order(
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

@internal
pub fn project_shipping_order(
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

@internal
pub fn project_reverse_delivery(
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

@internal
pub fn project_reverse_fulfillment_order(
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

@internal
pub fn serialize_delivery_profiles_connection(
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

@internal
pub fn serialize_delivery_profile_connection(
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

@internal
pub fn list_delivery_profiles_for_connection(
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

@internal
pub fn delivery_profile_cursor(
  profile: DeliveryProfileRecord,
  _index: Int,
) -> String {
  profile.cursor |> option.unwrap(profile.id)
}

@internal
pub fn serialize_carrier_services_connection(
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

@internal
pub fn serialize_carrier_service_connection(
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

@internal
pub fn serialize_fulfillment_orders_connection(
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

@internal
pub fn serialize_assigned_fulfillment_orders_connection(
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

@internal
pub fn serialize_manual_holds_fulfillment_orders_connection(
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

@internal
pub fn serialize_fulfillment_order_connection(
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

@internal
pub fn list_fulfillment_orders_for_connection(
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

@internal
pub fn filter_fulfillment_orders_by_query(
  orders: List(FulfillmentOrderRecord),
  raw_query: Result(root_field.ResolvedValue, Nil),
) -> List(FulfillmentOrderRecord) {
  case raw_query {
    Ok(root_field.StringVal(query)) ->
      filter_fulfillment_orders_by_query_string(orders, query)
    _ -> orders
  }
}

@internal
pub fn filter_fulfillment_orders_by_query_string(
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

@internal
pub fn fulfillment_order_matches_search_term(
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

@internal
pub fn assigned_fulfillment_order_matches_assignment(
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

@internal
pub fn fulfillment_order_matches_location_ids(
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

@internal
pub fn fulfillment_order_has_cancellation_request(
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

@internal
pub fn compare_fulfillment_orders_by_id(
  left: FulfillmentOrderRecord,
  right: FulfillmentOrderRecord,
) -> Order {
  string.compare(left.id, right.id)
}

@internal
pub fn list_carrier_services_for_connection(
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

@internal
pub fn filter_carrier_services_by_query(
  services: List(CarrierServiceRecord),
  raw_query: Result(root_field.ResolvedValue, Nil),
) -> List(CarrierServiceRecord) {
  case raw_query {
    Ok(root_field.StringVal(query)) ->
      filter_carrier_services_by_query_string(services, query)
    _ -> services
  }
}

@internal
pub fn filter_carrier_services_by_query_string(
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

@internal
pub fn carrier_service_matches_term(
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

@internal
pub fn compare_carrier_services(
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

@internal
pub fn compare_string_then_id(
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

@internal
pub fn optional_delivery_profile_source(
  value: Option(DeliveryProfileRecord),
) -> SourceValue {
  case value {
    Some(profile) -> delivery_profile_source(profile)
    None -> SrcNull
  }
}

@internal
pub fn optional_carrier_service_source(
  value: Option(CarrierServiceRecord),
) -> SourceValue {
  case value {
    Some(service) -> carrier_service_source(service)
    None -> SrcNull
  }
}

@internal
pub fn carrier_service_source(service: CarrierServiceRecord) -> SourceValue {
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

@internal
pub fn optional_fulfillment_service_source(
  store: Store,
  value: Option(FulfillmentServiceRecord),
) -> SourceValue {
  case value {
    Some(service) -> fulfillment_service_source(store, service)
    None -> SrcNull
  }
}

@internal
pub fn fulfillment_service_source(
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

@internal
pub fn fulfillment_service_location_source(
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

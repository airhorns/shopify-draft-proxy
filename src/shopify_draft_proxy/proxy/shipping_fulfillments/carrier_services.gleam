//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/delivery_profiles.{
  make_delivery_profile, update_delivery_profile,
  validate_delivery_profile_create_input,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/fulfillment_order_helpers.{
  synthetic_timestamp_string,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  apply_package_input, captured_bool_field, read_bool,
  read_carrier_service_callback_url, read_fulfillment_service_callback_url,
  read_object, read_string, read_trimmed_string, resolved_args,
  store_property_string_field, update_fulfillment_order_fields,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/serializers.{
  carrier_service_delete_payload_json, carrier_service_payload_json,
  delivery_profile_payload_json, delivery_profile_remove_payload_json,
  fulfillment_service_delete_payload_json, fulfillment_service_payload_json,
  local_pickup_disable_payload_json, local_pickup_enable_payload_json,
  payload_json, shipping_package_update_payload_json,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  blank_delivery_profile_name_error, carrier_service_formatted_name,
  carrier_service_not_found_for_delete, carrier_service_not_found_for_update,
  delivery_profile_default_remove_error, delivery_profile_remove_not_found,
  delivery_profile_update_not_found, find_active_store_property_location,
  flat_rate_shipping_package_not_updatable,
  fulfillment_service_destination_location_should_not_be_present,
  fulfillment_service_location_record, fulfillment_service_not_found,
  invalid_fulfillment_service_destination_location,
  invalid_shipping_package_result, is_active_location,
  is_flat_rate_shipping_package, is_fulfillment_service_location,
  local_pickup_custom_pickup_time_not_allowed, local_pickup_location_not_found,
  normalize_fulfillment_service_handle, strip_query_from_gid,
  validate_carrier_service_create_callback_url, validate_carrier_service_name,
  validate_carrier_service_update_callback_url,
  validate_fulfillment_service_callback_url, validate_fulfillment_service_name,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CarrierServiceRecord, type FulfillmentOrderRecord,
  type FulfillmentServiceRecord, type StorePropertyRecord, CapturedArray,
  CapturedObject, CapturedString, CarrierServiceRecord, FulfillmentOrderRecord,
  FulfillmentServiceRecord, ShippingPackageRecord, StorePropertyBool,
  StorePropertyNull, StorePropertyObject, StorePropertyRecord,
  StorePropertyString,
}

@internal
pub fn handle_carrier_service_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "input") |> option.unwrap(dict.new())
  let name = read_trimmed_string(input, "name")
  let callback_url = read_carrier_service_callback_url(input)
  let user_errors =
    validate_carrier_service_name(name)
    |> list.append(validate_carrier_service_create_callback_url(callback_url))
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
          callback_url: callback_url,
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
        shipping_types.MutationFieldResult(
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
      shipping_types.MutationFieldResult(
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

@internal
pub fn handle_carrier_service_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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

@internal
pub fn update_existing_carrier_service(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  input: Dict(String, root_field.ResolvedValue),
  existing: CarrierServiceRecord,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let next_name = case read_trimmed_string(input, "name") {
    Some(value) -> Some(value)
    None -> existing.name
  }
  let next_callback_url = case dict.has_key(input, "callbackUrl") {
    True -> read_carrier_service_callback_url(input)
    False -> existing.callback_url
  }
  let user_errors =
    validate_carrier_service_name(next_name)
    |> list.append(validate_carrier_service_update_callback_url(
      next_callback_url,
      existing.callback_url,
    ))
  case user_errors, next_name {
    [], Some(valid_name) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let updated =
        CarrierServiceRecord(
          ..existing,
          name: Some(valid_name),
          formatted_name: carrier_service_formatted_name(Some(valid_name)),
          callback_url: next_callback_url,
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
        shipping_types.MutationFieldResult(
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

@internal
pub fn handle_carrier_service_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_carrier_service_by_id(draft_store, id) {
        Some(_) -> {
          let next_store = store.delete_staged_carrier_service(draft_store, id)
          #(
            shipping_types.MutationFieldResult(
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

@internal
pub fn carrier_service_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(shipping_types.CarrierServiceUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn carrier_service_delete_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(shipping_types.CarrierServiceUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn handle_delivery_profile_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input = read_object(args, "profile")
  case input {
    Some(profile_input) -> {
      case validate_delivery_profile_create_input(draft_store, profile_input) {
        Ok(name) -> {
          let #(profile, next_identity) =
            make_delivery_profile(draft_store, identity, profile_input, name)
          let #(staged, next_store) =
            store.stage_create_delivery_profile(draft_store, profile)
          #(
            shipping_types.MutationFieldResult(
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
        Error(user_errors) ->
          delivery_profile_validation_result(
            draft_store,
            identity,
            field,
            fragments,
            "DeliveryProfileCreatePayload",
            user_errors,
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

@internal
pub fn handle_delivery_profile_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
            shipping_types.MutationFieldResult(
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

@internal
pub fn handle_delivery_profile_remove(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
                shipping_types.MutationFieldResult(
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

@internal
pub fn delivery_profile_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(shipping_types.DeliveryProfileUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn delivery_profile_remove_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(shipping_types.DeliveryProfileUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn handle_fulfillment_service_create(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream_origin: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let name = read_trimmed_string(args, "name")
  let callback_url = read_fulfillment_service_callback_url(args)
  let user_errors =
    list.append(
      validate_fulfillment_service_name(name),
      validate_fulfillment_service_callback_url(callback_url, upstream_origin),
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
        shipping_types.MutationFieldResult(
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
      shipping_types.MutationFieldResult(
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

@internal
pub fn handle_fulfillment_service_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream_origin: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
            upstream_origin,
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

@internal
pub fn update_existing_fulfillment_service(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, root_field.ResolvedValue),
  existing: FulfillmentServiceRecord,
  upstream_origin: String,
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
      validate_fulfillment_service_callback_url(callback_url, upstream_origin),
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
        shipping_types.MutationFieldResult(
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

@internal
pub fn handle_fulfillment_service_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
                shipping_types.MutationFieldResult(
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

@internal
pub fn read_fulfillment_service_delete_action(
  args: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_string(args, "inventoryAction") {
    Some("DELETE") -> "DELETE"
    Some("KEEP") -> "KEEP"
    Some("TRANSFER") -> "TRANSFER"
    _ -> "TRANSFER"
  }
}

@internal
pub fn fulfillment_service_delete_destination(
  draft_store: Store,
  inventory_action: String,
  destination_location_id: Option(String),
) -> Result(
  Option(StorePropertyRecord),
  List(shipping_types.FulfillmentServiceUserError),
) {
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
    _ ->
      case destination_location_id {
        Some(_) ->
          Error([
            fulfillment_service_destination_location_should_not_be_present(),
          ])
        None -> Ok(None)
      }
  }
}

@internal
pub fn find_active_merchant_managed_location(
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

@internal
pub fn stage_fulfillment_service_delete_effects(
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

@internal
pub fn convert_fulfillment_service_location_to_merchant(
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

@internal
pub fn reassign_fulfillment_orders_from_service_location(
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

@internal
pub fn close_fulfillment_orders_at_service_location(
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

@internal
pub fn fulfillment_order_is_open(order: FulfillmentOrderRecord) -> Bool {
  order.status != "CLOSED"
}

@internal
pub fn fulfillment_order_assigned_to_location(
  order: FulfillmentOrderRecord,
  location_id: String,
) -> Bool {
  order.assigned_location_id == Some(location_id)
}

@internal
pub fn fulfillment_order_assigned_location_value(
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

@internal
pub fn fulfillment_service_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  payload_typename: String,
  user_errors: List(shipping_types.FulfillmentServiceUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn fulfillment_service_delete_validation_result(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(shipping_types.FulfillmentServiceUserError),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    shipping_types.MutationFieldResult(
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

@internal
pub fn handle_location_local_pickup_enable(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let input =
    read_object(args, "localPickupSettings") |> option.unwrap(dict.new())
  let location_id = read_string(input, "locationId")
  case find_active_store_property_location(draft_store, location_id) {
    Some(location) -> {
      let pickup_time =
        read_string(input, "pickupTime") |> option.unwrap("ONE_HOUR")
      case is_standard_local_pickup_time(pickup_time) {
        False -> #(
          shipping_types.MutationFieldResult(
            key: get_field_response_key(field),
            payload: local_pickup_enable_payload_json(field, fragments, None, [
              local_pickup_custom_pickup_time_not_allowed(),
            ]),
            errors: [],
            staged_resource_ids: [],
          ),
          draft_store,
          identity,
        )
        True -> {
          let settings =
            StorePropertyObject(
              dict.from_list([
                #("pickupTime", StorePropertyString(pickup_time)),
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
            shipping_types.MutationFieldResult(
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
      }
    }
    None -> #(
      shipping_types.MutationFieldResult(
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

@internal
pub fn is_standard_local_pickup_time(value: String) -> Bool {
  case value {
    "ONE_HOUR"
    | "TWO_HOURS"
    | "FOUR_HOURS"
    | "TWENTY_FOUR_HOURS"
    | "TWO_TO_FOUR_DAYS"
    | "FIVE_OR_MORE_DAYS" -> True
    _ -> False
  }
}

@internal
pub fn handle_location_local_pickup_disable(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
        shipping_types.MutationFieldResult(
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
      shipping_types.MutationFieldResult(
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

@internal
pub fn handle_shipping_package_update(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  let id = read_string(args, "id")
  let input = read_object(args, "shippingPackage")
  case id, input {
    Some(package_id), Some(package_input) -> {
      case store.get_effective_shipping_package_by_id(draft_store, package_id) {
        Some(base) -> {
          case is_flat_rate_shipping_package(base) {
            True -> #(
              shipping_types.MutationFieldResult(
                key: get_field_response_key(field),
                payload: shipping_package_update_payload_json(field, fragments, [
                  flat_rate_shipping_package_not_updatable(),
                ]),
                errors: [],
                staged_resource_ids: [],
              ),
              draft_store,
              identity,
            )
            False -> {
              let #(updated_at, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let #(updated, pre_staged_store) =
                apply_package_input(
                  draft_store,
                  base,
                  package_input,
                  updated_at,
                )
              let #(_, next_store) =
                store.stage_update_shipping_package(pre_staged_store, updated)
              #(
                shipping_types.MutationFieldResult(
                  key: get_field_response_key(field),
                  payload: shipping_package_update_payload_json(
                    field,
                    fragments,
                    [],
                  ),
                  errors: [],
                  staged_resource_ids: [package_id],
                ),
                next_store,
                next_identity,
              )
            }
          }
        }
        None -> invalid_shipping_package_result(draft_store, identity, field)
      }
    }
    _, _ -> #(
      shipping_types.MutationFieldResult(
        key: get_field_response_key(field),
        payload: shipping_package_update_payload_json(field, fragments, []),
        errors: [],
        staged_resource_ids: [],
      ),
      draft_store,
      identity,
    )
  }
}

@internal
pub fn handle_shipping_package_make_default(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
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
            shipping_types.MutationFieldResult(
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
      shipping_types.MutationFieldResult(
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

@internal
pub fn handle_shipping_package_delete(
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(shipping_types.MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = resolved_args(field, variables)
  case read_string(args, "id") {
    Some(package_id) -> {
      case store.get_effective_shipping_package_by_id(draft_store, package_id) {
        Some(_) -> {
          let next_store =
            store.delete_staged_shipping_package(draft_store, package_id)
          #(
            shipping_types.MutationFieldResult(
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
      shipping_types.MutationFieldResult(
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

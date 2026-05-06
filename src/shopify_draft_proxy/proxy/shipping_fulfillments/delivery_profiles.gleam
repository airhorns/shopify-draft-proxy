//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/shipping_fulfillments/input_helpers.{
  captured_array_field, captured_connection, captured_count, captured_int_field,
  captured_string_field, captured_upsert_fields,
  count_delivery_profile_locations_to_add, delivery_profile_active_method_delta,
  normalize_money_amount, option_to_captured_string, read_bool, read_number,
  read_object, read_object_array, read_string, read_string_array,
  read_trimmed_string, store_property_string_field, unique_strings,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/sources.{
  blank_delivery_profile_create_name_error,
  empty_delivery_profile_zone_countries_error, is_active_location,
  is_fulfillment_service_location, overlapping_delivery_profile_zone_error,
  too_long_delivery_profile_create_name_error,
  unknown_delivery_profile_location_error,
}
import shopify_draft_proxy/proxy/shipping_fulfillments/types as shipping_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DeliveryProfileRecord, CapturedArray,
  CapturedBool, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  DeliveryProfileRecord,
}

@internal
pub fn validate_delivery_profile_create_input(
  draft_store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Result(String, List(shipping_types.DeliveryProfileUserError)) {
  let name = read_trimmed_string(input, "name")
  let name_errors = case name {
    Some(value) if value != "" -> {
      case string.length(value) >= 128 {
        True -> [too_long_delivery_profile_create_name_error()]
        False -> []
      }
    }
    _ -> [blank_delivery_profile_create_name_error()]
  }
  let nested_errors =
    list.append(
      validate_delivery_profile_location_group_inputs(
        draft_store,
        input,
        "profileLocationGroups",
      ),
      validate_delivery_profile_location_group_inputs(
        draft_store,
        input,
        "locationGroupsToCreate",
      ),
    )
  let errors = list.append(name_errors, nested_errors)
  case errors, name {
    [], Some(value) -> Ok(value)
    _, _ -> Error(errors)
  }
}

@internal
pub fn validate_delivery_profile_location_group_inputs(
  draft_store: Store,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(shipping_types.DeliveryProfileUserError) {
  read_indexed_object_array(input, key)
  |> list.flat_map(fn(group) {
    validate_delivery_profile_location_group(draft_store, key, group.0, group.1)
  })
}

@internal
pub fn validate_delivery_profile_location_group(
  draft_store: Store,
  group_key: String,
  group_index: Int,
  input: Dict(String, root_field.ResolvedValue),
) -> List(shipping_types.DeliveryProfileUserError) {
  let location_errors =
    read_indexed_string_array(input, "locations")
    |> list.filter_map(fn(location) {
      case delivery_profile_location_available(draft_store, location.1) {
        True -> Error(Nil)
        False ->
          Ok(unknown_delivery_profile_location_error(
            group_key
            <> "."
            <> int.to_string(group_index)
            <> ".locations."
            <> int.to_string(location.0),
          ))
      }
    })
  let zone_errors =
    list.append(
      validate_delivery_profile_zones(
        read_indexed_object_array(input, "zonesToCreate"),
        group_key,
        group_index,
        "zonesToCreate",
      ),
      validate_delivery_profile_zones(
        read_indexed_object_array(input, "zonesToUpdate"),
        group_key,
        group_index,
        "zonesToUpdate",
      ),
    )
  list.append(location_errors, zone_errors)
}

@internal
pub fn delivery_profile_location_available(
  draft_store: Store,
  location_id: String,
) -> Bool {
  case store.get_effective_location_by_id(draft_store, location_id) {
    Some(_) -> True
    None ->
      case
        store.get_effective_store_property_location_by_id(
          draft_store,
          location_id,
        )
      {
        Some(location) ->
          is_active_location(location)
          && !is_fulfillment_service_location(location)
        None -> False
      }
  }
}

@internal
pub fn validate_delivery_profile_zones(
  zones: List(#(Int, Dict(String, root_field.ResolvedValue))),
  group_key: String,
  group_index: Int,
  zone_key: String,
) -> List(shipping_types.DeliveryProfileUserError) {
  let #(errors, _, _) =
    list.fold(zones, #([], dict.new(), False), fn(acc, zone) {
      let #(current_errors, seen_countries, has_overlap) = acc
      let zone_index = zone.0
      let countries = read_indexed_object_array(zone.1, "countries")
      let empty_errors = case countries {
        [] -> [
          empty_delivery_profile_zone_countries_error(
            group_key
            <> "."
            <> int.to_string(group_index)
            <> "."
            <> zone_key
            <> "."
            <> int.to_string(zone_index)
            <> ".countries",
          ),
        ]
        _ -> []
      }
      let #(next_seen, overlap_errors, next_has_overlap) =
        validate_delivery_profile_zone_countries(
          countries,
          seen_countries,
          has_overlap,
          group_key,
          group_index,
          zone_key,
          zone_index,
        )
      #(
        list.append(current_errors, list.append(empty_errors, overlap_errors)),
        next_seen,
        next_has_overlap,
      )
    })
  errors
}

@internal
pub fn validate_delivery_profile_zone_countries(
  countries: List(#(Int, Dict(String, root_field.ResolvedValue))),
  seen_countries: Dict(String, Bool),
  has_overlap: Bool,
  group_key: String,
  group_index: Int,
  zone_key: String,
  zone_index: Int,
) -> #(Dict(String, Bool), List(shipping_types.DeliveryProfileUserError), Bool) {
  list.fold(countries, #(seen_countries, [], has_overlap), fn(acc, country) {
    let #(current_seen, current_errors, current_has_overlap) = acc
    let keys = delivery_profile_country_overlap_keys(country.1)
    let overlap = list.any(keys, fn(key) { dict.has_key(current_seen, key) })
    let next_seen =
      list.fold(keys, current_seen, fn(seen, key) {
        dict.insert(seen, key, True)
      })
    let next_errors = case overlap, current_has_overlap {
      True, False -> [
        overlapping_delivery_profile_zone_error(
          group_key
          <> "."
          <> int.to_string(group_index)
          <> "."
          <> zone_key
          <> "."
          <> int.to_string(zone_index)
          <> ".countries."
          <> int.to_string(country.0),
        ),
      ]
      _, _ -> []
    }
    #(
      next_seen,
      list.append(current_errors, next_errors),
      current_has_overlap || overlap,
    )
  })
}

@internal
pub fn delivery_profile_country_overlap_keys(
  country: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case read_bool(country, "restOfWorld") {
    Some(True) -> ["REST_OF_WORLD"]
    _ ->
      case read_string(country, "code") {
        Some(code) -> [code]
        None -> []
      }
  }
}

@internal
pub fn read_indexed_object_array(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(#(Int, Dict(String, root_field.ResolvedValue))) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) -> indexed_object_values(items, 0)
    _ -> []
  }
}

@internal
pub fn indexed_object_values(
  values: List(root_field.ResolvedValue),
  index: Int,
) -> List(#(Int, Dict(String, root_field.ResolvedValue))) {
  case values {
    [] -> []
    [first, ..rest] -> {
      let tail = indexed_object_values(rest, index + 1)
      case first {
        root_field.ObjectVal(value) -> [#(index, value), ..tail]
        _ -> tail
      }
    }
  }
}

@internal
pub fn read_indexed_string_array(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(#(Int, String)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) -> indexed_string_values(items, 0)
    _ -> []
  }
}

@internal
pub fn indexed_string_values(
  values: List(root_field.ResolvedValue),
  index: Int,
) -> List(#(Int, String)) {
  case values {
    [] -> []
    [first, ..rest] -> {
      let tail = indexed_string_values(rest, index + 1)
      case first {
        root_field.StringVal(value) -> [#(index, value), ..tail]
        _ -> tail
      }
    }
  }
}

@internal
pub fn make_delivery_profile(
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

@internal
pub fn update_delivery_profile(
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

@internal
pub fn make_delivery_profile_location_groups(
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

@internal
pub fn make_delivery_profile_location_group(
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

@internal
pub fn make_delivery_profile_zone_nodes(
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

@internal
pub fn make_delivery_profile_zone_node(
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

@internal
pub fn make_delivery_profile_method_nodes(
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

@internal
pub fn make_delivery_profile_method(
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

@internal
pub fn make_delivery_rate_provider(
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

@internal
pub fn make_delivery_condition_nodes(
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

@internal
pub fn make_delivery_condition(
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

@internal
pub fn make_delivery_country(
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

@internal
pub fn profile_item_nodes(
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

@internal
pub fn location_nodes(
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

@internal
pub fn location_name(draft_store: Store, location_id: String) -> String {
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

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state
import shopify_draft_proxy/proxy/shipping_fulfillments
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DeliveryProfileRecord,
  type FulfillmentOrderRecord, type ShippingPackageRecord,
  type StorePropertyRecord, CalculatedOrderRecord, CapturedArray, CapturedBool,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CarrierServiceRecord, DeliveryProfileRecord, FulfillmentOrderRecord,
  FulfillmentRecord, LocationRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ReverseFulfillmentOrderRecord,
  ShippingPackageDimensionsRecord, ShippingPackageRecord,
  ShippingPackageWeightRecord, StorePropertyBool, StorePropertyNull,
  StorePropertyRecord, StorePropertyString,
}

fn package(id: String, name: String, default: Bool) -> ShippingPackageRecord {
  ShippingPackageRecord(
    id: id,
    name: Some(name),
    type_: Some("BOX"),
    default: default,
    weight: Some(ShippingPackageWeightRecord(
      value: Some(1.0),
      unit: Some("KILOGRAMS"),
    )),
    dimensions: Some(ShippingPackageDimensionsRecord(
      length: Some(10.0),
      width: Some(8.0),
      height: Some(4.0),
      unit: Some("CENTIMETERS"),
    )),
    created_at: "2026-04-27T00:00:00.000Z",
    updated_at: "2026-04-27T00:00:00.000Z",
  )
}

fn seeded_store() -> store.Store {
  store.upsert_base_shipping_packages(store.new(), [
    package("gid://shopify/ShippingPackage/1", "Starter box", True),
    package("gid://shopify/ShippingPackage/2", "Backup mailer", False),
  ])
}

fn settings_store() -> store.Store {
  let carrier_services = [
    CarrierServiceRecord(
      id: "gid://shopify/DeliveryCarrierService/1",
      name: Some("canada_post"),
      formatted_name: Some("Canada Post"),
      callback_url: None,
      active: True,
      supports_service_discovery: True,
      created_at: "2026-04-27T00:00:00.000Z",
      updated_at: "2026-04-27T00:00:00.000Z",
    ),
    CarrierServiceRecord(
      id: "gid://shopify/DeliveryCarrierService/2",
      name: Some("inactive"),
      formatted_name: Some("Inactive"),
      callback_url: None,
      active: False,
      supports_service_discovery: False,
      created_at: "2026-04-27T00:00:00.000Z",
      updated_at: "2026-04-27T00:00:00.000Z",
    ),
  ]
  let base = store.upsert_base_carrier_services(store.new(), carrier_services)
  base
  |> store.upsert_base_store_property_location(location(
    "gid://shopify/Location/10",
    "Shop location",
    True,
    False,
  ))
  |> store.upsert_base_store_property_location(location(
    "gid://shopify/Location/11",
    "Fulfillment location",
    True,
    True,
  ))
  |> store.upsert_base_store_property_location(location(
    "gid://shopify/Location/12",
    "Inactive location",
    False,
    False,
  ))
}

fn delivery_profile_store() -> store.Store {
  let profile_id = "gid://shopify/DeliveryProfile/125254992178"
  store.upsert_base_delivery_profiles(store.new(), [
    DeliveryProfileRecord(
      id: profile_id,
      cursor: Some("delivery-profile-cursor"),
      merchant_owned: True,
      data: CapturedObject([
        #("id", CapturedString(profile_id)),
        #("name", CapturedString("General profile")),
        #("default", CapturedBool(True)),
        #("version", CapturedInt(1)),
        #("activeMethodDefinitionsCount", CapturedInt(12)),
        #("locationsWithoutRatesCount", CapturedInt(2)),
        #("originLocationCount", CapturedInt(1)),
        #("zoneCountryCount", CapturedInt(28)),
        #(
          "productVariantsCount",
          CapturedObject([
            #("count", CapturedInt(500)),
            #("precision", CapturedString("AT_LEAST")),
          ]),
        ),
        #(
          "sellingPlanGroups",
          CapturedObject([
            #("nodes", CapturedArray([])),
            #(
              "pageInfo",
              CapturedObject([
                #("hasNextPage", CapturedBool(False)),
                #("hasPreviousPage", CapturedBool(False)),
                #("startCursor", CapturedNull),
                #("endCursor", CapturedNull),
              ]),
            ),
          ]),
        ),
      ]),
    ),
  ])
}

fn reverse_logistics_store() -> store.Store {
  store.upsert_base_reverse_fulfillment_orders(store.new(), [
    ReverseFulfillmentOrderRecord(
      id: "gid://shopify/ReverseFulfillmentOrder/1",
      data: CapturedObject([
        #("id", CapturedString("gid://shopify/ReverseFulfillmentOrder/1")),
        #("status", CapturedString("OPEN")),
        #(
          "lineItems",
          CapturedObject([
            #(
              "nodes",
              CapturedArray([
                CapturedObject([
                  #(
                    "id",
                    CapturedString(
                      "gid://shopify/ReverseFulfillmentOrderLineItem/1",
                    ),
                  ),
                  #("totalQuantity", CapturedInt(1)),
                  #("remainingQuantity", CapturedInt(1)),
                ]),
              ]),
            ),
            #(
              "pageInfo",
              CapturedObject([
                #("hasNextPage", CapturedBool(False)),
                #("hasPreviousPage", CapturedBool(False)),
                #("startCursor", CapturedNull),
                #("endCursor", CapturedNull),
              ]),
            ),
          ]),
        ),
        #("reverseDeliveries", empty_captured_connection()),
      ]),
    ),
  ])
}

fn calculated_order_store() -> store.Store {
  store.upsert_base_calculated_orders(store.new(), [
    CalculatedOrderRecord(
      id: "gid://shopify/CalculatedOrder/1",
      data: CapturedObject([
        #("id", CapturedString("gid://shopify/CalculatedOrder/1")),
        #("shippingLines", CapturedArray([])),
        #("subtotalPriceSet", money_bag("10.0", "CAD")),
        #("totalPriceSet", money_bag("10.0", "CAD")),
      ]),
    ),
  ])
}

fn money_bag(amount: String, currency_code: String) -> CapturedJsonValue {
  let money =
    CapturedObject([
      #("amount", CapturedString(amount)),
      #("currencyCode", CapturedString(currency_code)),
    ])
  CapturedObject([
    #("shopMoney", money),
    #("presentmentMoney", money),
  ])
}

fn delivery_profile_lifecycle_store() -> store.Store {
  let product_id = "gid://shopify/Product/1"
  let variant_id = "gid://shopify/ProductVariant/1"
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      id: product_id,
      legacy_resource_id: None,
      title: "Shipping Snowboard",
      handle: "shipping-snowboard",
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
    ),
  ])
  |> store.upsert_base_product_variants([
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: "Default Title",
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
      cursor: None,
    ),
  ])
  |> store.upsert_base_locations([
    LocationRecord(
      id: "gid://shopify/Location/1",
      name: "Shop location",
      cursor: None,
    ),
    LocationRecord(
      id: "gid://shopify/Location/2",
      name: "Warehouse",
      cursor: None,
    ),
  ])
  |> store.upsert_base_delivery_profiles([default_delivery_profile()])
}

fn default_delivery_profile() -> DeliveryProfileRecord {
  DeliveryProfileRecord(
    id: "gid://shopify/DeliveryProfile/125254992178",
    cursor: None,
    merchant_owned: True,
    data: CapturedObject([
      #("id", CapturedString("gid://shopify/DeliveryProfile/125254992178")),
      #("name", CapturedString("General profile")),
      #("default", CapturedBool(True)),
      #("merchantOwned", CapturedBool(True)),
      #("version", CapturedInt(1)),
      #("activeMethodDefinitionsCount", CapturedInt(0)),
      #("locationsWithoutRatesCount", CapturedInt(0)),
      #("originLocationCount", CapturedInt(0)),
      #("zoneCountryCount", CapturedInt(0)),
      #(
        "productVariantsCount",
        CapturedObject([
          #("count", CapturedInt(0)),
          #("precision", CapturedString("EXACT")),
        ]),
      ),
      #("profileItems", empty_captured_connection()),
      #("profileLocationGroups", CapturedArray([])),
    ]),
  )
}

fn empty_captured_connection() -> CapturedJsonValue {
  CapturedObject([
    #("nodes", CapturedArray([])),
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

fn location(
  id: String,
  name: String,
  is_active: Bool,
  is_fulfillment_service: Bool,
) -> StorePropertyRecord {
  StorePropertyRecord(
    id: id,
    cursor: None,
    data: dict.from_list([
      #("__typename", StorePropertyString("Location")),
      #("id", StorePropertyString(id)),
      #("name", StorePropertyString(name)),
      #("isActive", StorePropertyBool(is_active)),
      #("isFulfillmentService", StorePropertyBool(is_fulfillment_service)),
      #("localPickupSettingsV2", StorePropertyNull),
      #("localPickupSettings", StorePropertyNull),
    ]),
  )
}

fn fulfillment_order_at_location(
  id: String,
  location_id: String,
  location_name: String,
) -> FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    id: id,
    order_id: Some("gid://shopify/Order/har-571"),
    status: "OPEN",
    request_status: "UNSUBMITTED",
    assigned_location_id: Some(location_id),
    assignment_status: None,
    manually_held: False,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("status", CapturedString("OPEN")),
      #("requestStatus", CapturedString("UNSUBMITTED")),
      #(
        "assignedLocation",
        CapturedObject([
          #("name", CapturedString(location_name)),
          #(
            "location",
            CapturedObject([
              #("id", CapturedString(location_id)),
              #("name", CapturedString(location_name)),
            ]),
          ),
        ]),
      ),
      #("supportedActions", CapturedArray([])),
      #("lineItems", empty_captured_connection()),
    ]),
  )
}

fn mutation_vars(
  id: String,
  shipping_package: root_field.ResolvedValue,
) -> dict.Dict(String, root_field.ResolvedValue) {
  dict.from_list([
    #("id", root_field.StringVal(id)),
    #("shippingPackage", shipping_package),
  ])
}

pub fn shipping_package_update_stages_local_record_test() {
  let variables =
    mutation_vars(
      "gid://shopify/ShippingPackage/1",
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal("Updated box")),
          #("type", root_field.StringVal("BOX")),
          #("default", root_field.BoolVal(True)),
          #(
            "weight",
            root_field.ObjectVal(
              dict.from_list([
                #("value", root_field.FloatVal(2.5)),
                #("unit", root_field.StringVal("POUNDS")),
              ]),
            ),
          ),
        ]),
      ),
    )
  let outcome =
    shipping_fulfillments.process_mutation(
      seeded_store(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "mutation UpdatePackage($id: ID!, $shippingPackage: CustomShippingPackageInput!) { shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message } } }",
      variables,
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"shippingPackageUpdate\":{\"userErrors\":[]}}}"
  let assert Some(updated) =
    store.get_effective_shipping_package_by_id(
      outcome.store,
      "gid://shopify/ShippingPackage/1",
    )
  assert updated.name == Some("Updated box")
  assert updated.updated_at == "2024-01-01T00:00:00.000Z"
}

pub fn shipping_package_make_default_clears_previous_default_test() {
  let outcome =
    shipping_fulfillments.process_mutation(
      seeded_store(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "mutation MakeDefault($id: ID!) { shippingPackageMakeDefault(id: $id) { userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/ShippingPackage/2")),
      ]),
      empty_upstream_context(),
    )

  let assert Some(first) =
    store.get_effective_shipping_package_by_id(
      outcome.store,
      "gid://shopify/ShippingPackage/1",
    )
  let assert Some(second) =
    store.get_effective_shipping_package_by_id(
      outcome.store,
      "gid://shopify/ShippingPackage/2",
    )
  assert first.default == False
  assert second.default == True
}

pub fn shipping_package_delete_marks_staged_deletion_test() {
  let outcome =
    shipping_fulfillments.process_mutation(
      seeded_store(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "mutation DeletePackage($id: ID!) { shippingPackageDelete(id: $id) { deletedId userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/ShippingPackage/1")),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"shippingPackageDelete\":{\"deletedId\":\"gid://shopify/ShippingPackage/1\",\"userErrors\":[]}}}"
  assert store.get_effective_shipping_package_by_id(
      outcome.store,
      "gid://shopify/ShippingPackage/1",
    )
    == None
}

pub fn shipping_package_update_unknown_id_returns_resource_not_found_test() {
  let variables =
    mutation_vars(
      "gid://shopify/ShippingPackage/999",
      root_field.ObjectVal(
        dict.from_list([#("name", root_field.StringVal("Ghost box"))]),
      ),
    )
  let outcome =
    shipping_fulfillments.process_mutation(
      seeded_store(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "mutation UpdatePackage($id: ID!, $shippingPackage: CustomShippingPackageInput!) { shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message } } }",
      variables,
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"invalid id\",\"path\":[\"shippingPackageUpdate\"],\"extensions\":{\"code\":\"RESOURCE_NOT_FOUND\"}}],\"data\":{\"shippingPackageUpdate\":null}}"
  assert store.get_effective_shipping_package_by_id(
      outcome.store,
      "gid://shopify/ShippingPackage/999",
    )
    == None
}

pub fn carrier_service_create_update_read_and_delete_lifecycle_test() {
  let create_input =
    root_field.ObjectVal(
      dict.from_list([
        #("name", root_field.StringVal("Hermes Carrier")),
        #(
          "callbackUrl",
          root_field.StringVal("https://mock.shop/carrier-service-rates"),
        ),
        #("active", root_field.BoolVal(False)),
        #("supportsServiceDiscovery", root_field.BoolVal(True)),
      ]),
    )
  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateCarrier($input: DeliveryCarrierServiceCreateInput!) { carrierServiceCreate(input: $input) { carrierService { id name formattedName callbackUrl active supportsServiceDiscovery } userErrors { field message } } }",
      dict.from_list([#("input", create_input)]),
      empty_upstream_context(),
    )

  let assert [created_id] = create_outcome.staged_resource_ids
  let assert Some(created) =
    store.get_effective_carrier_service_by_id(create_outcome.store, created_id)
  assert created.name == Some("Hermes Carrier")
  assert created.formatted_name
    == Some("Hermes Carrier (Rates provided by app)")
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"carrierServiceCreate\":{\"carrierService\":{\"id\":\""
    <> created_id
    <> "\",\"name\":\"Hermes Carrier\",\"formattedName\":\"Hermes Carrier (Rates provided by app)\",\"callbackUrl\":\"https://mock.shop/carrier-service-rates\",\"active\":false,\"supportsServiceDiscovery\":true},\"userErrors\":[]}}}"

  let update_input =
    root_field.ObjectVal(
      dict.from_list([
        #("id", root_field.StringVal(created_id)),
        #("name", root_field.StringVal("Hermes Carrier Updated")),
        #(
          "callbackUrl",
          root_field.StringVal(
            "https://mock.shop/carrier-service-rates-updated",
          ),
        ),
        #("active", root_field.BoolVal(True)),
        #("supportsServiceDiscovery", root_field.BoolVal(False)),
      ]),
    )
  let update_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation UpdateCarrier($input: DeliveryCarrierServiceUpdateInput!) { carrierServiceUpdate(input: $input) { carrierService { id name active supportsServiceDiscovery } userErrors { field message } } }",
      dict.from_list([#("input", update_input)]),
      empty_upstream_context(),
    )

  let assert Some(updated) =
    store.get_effective_carrier_service_by_id(update_outcome.store, created_id)
  assert updated.name == Some("Hermes Carrier Updated")
  assert updated.active == True
  assert updated.supports_service_discovery == False

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      update_outcome.store,
      "query ReadCarrier($id: ID!, $query: String) { carrierService(id: $id) { id name active supportsServiceDiscovery } active: carrierServices(first: 5, query: $query, sortKey: ID) { nodes { id name active } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
      dict.from_list([
        #("id", root_field.StringVal(created_id)),
        #("query", root_field.StringVal("active:true")),
      ]),
    )
  assert json.to_string(read_response)
    == "{\"data\":{\"carrierService\":{\"id\":\""
    <> created_id
    <> "\",\"name\":\"Hermes Carrier Updated\",\"active\":true,\"supportsServiceDiscovery\":false},\"active\":{\"nodes\":[{\"id\":\""
    <> created_id
    <> "\",\"name\":\"Hermes Carrier Updated\",\"active\":true}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:"
    <> created_id
    <> "\",\"endCursor\":\"cursor:"
    <> created_id
    <> "\"}}}}"

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      update_outcome.store,
      update_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteCarrier($id: ID!) { carrierServiceDelete(id: $id) { deletedId userErrors { field message } } }",
      dict.from_list([#("id", root_field.StringVal(created_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"carrierServiceDelete\":{\"deletedId\":\""
    <> created_id
    <> "\",\"userErrors\":[]}}}"
  assert store.get_effective_carrier_service_by_id(
      delete_outcome.store,
      created_id,
    )
    == None
}

pub fn carrier_service_validation_branches_return_user_errors_test() {
  let blank_input =
    root_field.ObjectVal(dict.from_list([#("name", root_field.StringVal(""))]))
  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation InvalidCarrier($input: DeliveryCarrierServiceCreateInput!) { carrierServiceCreate(input: $input) { carrierService { id } userErrors { field message } } }",
      dict.from_list([#("input", blank_input)]),
      empty_upstream_context(),
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"carrierServiceCreate\":{\"carrierService\":null,\"userErrors\":[{\"field\":null,\"message\":\"Shipping rate provider name can't be blank\"}]}}}"

  let unknown_update =
    root_field.ObjectVal(
      dict.from_list([
        #(
          "id",
          root_field.StringVal("gid://shopify/DeliveryCarrierService/999"),
        ),
        #("name", root_field.StringVal("Nope")),
      ]),
    )
  let update_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation UnknownCarrier($input: DeliveryCarrierServiceUpdateInput!) { carrierServiceUpdate(input: $input) { carrierService { id } userErrors { field message } } }",
      dict.from_list([#("input", unknown_update)]),
      empty_upstream_context(),
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"carrierServiceUpdate\":{\"carrierService\":null,\"userErrors\":[{\"field\":null,\"message\":\"The carrier or app could not be found.\"}]}}}"

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation UnknownDelete($id: ID!) { carrierServiceDelete(id: $id) { deletedId userErrors { field message } } }",
      dict.from_list([
        #(
          "id",
          root_field.StringVal("gid://shopify/DeliveryCarrierService/999"),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"carrierServiceDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The carrier or app could not be found.\"}]}}}"
}

pub fn shipping_settings_availability_filters_active_services_and_locations_test() {
  let assert Ok(response) =
    shipping_fulfillments.process(
      settings_store(),
      "query Availability { availableCarrierServices { carrierService { id name active supportsServiceDiscovery } locations { id name } } locationsAvailableForDeliveryProfilesConnection(first: 5) { nodes { id name localPickupSettingsV2 { pickupTime instructions } } pageInfo { hasNextPage hasPreviousPage } } }",
      dict.new(),
    )

  assert json.to_string(response)
    == "{\"data\":{\"availableCarrierServices\":[{\"carrierService\":{\"id\":\"gid://shopify/DeliveryCarrierService/1\",\"name\":\"canada_post\",\"active\":true,\"supportsServiceDiscovery\":true},\"locations\":[{\"id\":\"gid://shopify/Location/10\",\"name\":\"Shop location\"}]}],\"locationsAvailableForDeliveryProfilesConnection\":{\"nodes\":[{\"id\":\"gid://shopify/Location/10\",\"name\":\"Shop location\",\"localPickupSettingsV2\":null},{\"id\":\"gid://shopify/Location/11\",\"name\":\"Fulfillment location\",\"localPickupSettingsV2\":null}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false}}}}"
}

pub fn delivery_profile_read_projects_detail_catalog_and_missing_test() {
  let assert Ok(response) =
    shipping_fulfillments.process(
      delivery_profile_store(),
      "query DeliveryProfiles($id: ID!, $missing: ID!) { deliveryProfile(id: $id) { id name default version activeMethodDefinitionsCount locationsWithoutRatesCount originLocationCount zoneCountryCount productVariantsCount { count precision } sellingPlanGroups(first: 2) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } deliveryProfiles(first: 2, reverse: true, merchantOwnedOnly: true) { edges { cursor node { id name default } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } missing: deliveryProfile(id: $missing) { id } }",
      dict.from_list([
        #(
          "id",
          root_field.StringVal("gid://shopify/DeliveryProfile/125254992178"),
        ),
        #("missing", root_field.StringVal("gid://shopify/DeliveryProfile/999")),
      ]),
    )

  assert json.to_string(response)
    == "{\"data\":{\"deliveryProfile\":{\"id\":\"gid://shopify/DeliveryProfile/125254992178\",\"name\":\"General profile\",\"default\":true,\"version\":1,\"activeMethodDefinitionsCount\":12,\"locationsWithoutRatesCount\":2,\"originLocationCount\":1,\"zoneCountryCount\":28,\"productVariantsCount\":{\"count\":500,\"precision\":\"AT_LEAST\"},\"sellingPlanGroups\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}},\"deliveryProfiles\":{\"edges\":[{\"cursor\":\"delivery-profile-cursor\",\"node\":{\"id\":\"gid://shopify/DeliveryProfile/125254992178\",\"name\":\"General profile\",\"default\":true}}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"delivery-profile-cursor\",\"endCursor\":\"delivery-profile-cursor\"}},\"missing\":null}}"
}

pub fn draft_proxy_routes_delivery_profile_to_shipping_domain_test() {
  let proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: delivery_profile_store())
  let body =
    json.object([
      #(
        "query",
        json.string(
          "query DeliveryProfile($id: ID!) { deliveryProfile(id: $id) { id name } }",
        ),
      ),
      #(
        "variables",
        json.object([
          #("id", json.string("gid://shopify/DeliveryProfile/125254992178")),
        ]),
      ),
    ])
    |> json.to_string
  let #(response, _) =
    draft_proxy.process_request(
      proxy,
      proxy_state.Request(
        method: "POST",
        path: "/admin/api/2026-04/graphql.json",
        headers: dict.new(),
        body: body,
      ),
    )

  assert response.status == 200
  assert json.to_string(response.body)
    == "{\"data\":{\"deliveryProfile\":{\"id\":\"gid://shopify/DeliveryProfile/125254992178\",\"name\":\"General profile\"}}}"
}

pub fn delivery_profile_lifecycle_stages_create_update_remove_test() {
  let create_profile =
    root_field.ObjectVal(
      dict.from_list([
        #("name", root_field.StringVal("Local custom shipping")),
        #(
          "variantsToAssociate",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/ProductVariant/1"),
          ]),
        ),
        #(
          "locationGroupsToCreate",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "locations",
                  root_field.ListVal([
                    root_field.StringVal("gid://shopify/Location/1"),
                  ]),
                ),
                #(
                  "zonesToCreate",
                  root_field.ListVal([
                    root_field.ObjectVal(
                      dict.from_list([
                        #("name", root_field.StringVal("Domestic")),
                        #(
                          "countries",
                          root_field.ListVal([
                            root_field.ObjectVal(
                              dict.from_list([
                                #("code", root_field.StringVal("US")),
                              ]),
                            ),
                          ]),
                        ),
                        #(
                          "methodDefinitionsToCreate",
                          root_field.ListVal([
                            root_field.ObjectVal(
                              dict.from_list([
                                #("name", root_field.StringVal("Standard")),
                                #("active", root_field.BoolVal(True)),
                                #(
                                  "rateDefinition",
                                  root_field.ObjectVal(
                                    dict.from_list([
                                      #(
                                        "price",
                                        root_field.ObjectVal(
                                          dict.from_list([
                                            #(
                                              "amount",
                                              root_field.StringVal("7.25"),
                                            ),
                                            #(
                                              "currencyCode",
                                              root_field.StringVal("USD"),
                                            ),
                                          ]),
                                        ),
                                      ),
                                    ]),
                                  ),
                                ),
                                #(
                                  "weightConditionsToCreate",
                                  root_field.ListVal([
                                    root_field.ObjectVal(
                                      dict.from_list([
                                        #(
                                          "operator",
                                          root_field.StringVal(
                                            "GREATER_THAN_OR_EQUAL_TO",
                                          ),
                                        ),
                                        #(
                                          "criteria",
                                          root_field.ObjectVal(
                                            dict.from_list([
                                              #("value", root_field.IntVal(1)),
                                              #(
                                                "unit",
                                                root_field.StringVal(
                                                  "KILOGRAMS",
                                                ),
                                              ),
                                            ]),
                                          ),
                                        ),
                                      ]),
                                    ),
                                  ]),
                                ),
                              ]),
                            ),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ]),
    )
  let create_outcome =
    shipping_fulfillments.process_mutation(
      delivery_profile_lifecycle_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateDeliveryProfile($profile: DeliveryProfileInput!) { deliveryProfileCreate(profile: $profile) { profile { id name version originLocationCount zoneCountryCount activeMethodDefinitionsCount productVariantsCount { count precision } profileItems(first: 5) { nodes { product { id title } variants(first: 5) { nodes { id title } } } } profileLocationGroups { locationGroup { id locations(first: 5) { nodes { id name } } } locationGroupZones(first: 5) { nodes { zone { id name } methodDefinitions(first: 5) { nodes { id name active rateProvider { ... on DeliveryRateDefinition { id price { amount currencyCode } } } methodConditions { id field operator conditionCriteria { __typename ... on Weight { value unit } } } } } } } } } userErrors { field message } } }",
      dict.from_list([#("profile", create_profile)]),
      empty_upstream_context(),
    )
  let create_json = json.to_string(create_outcome.data)
  assert string.contains(
    create_json,
    "\"id\":\"gid://shopify/DeliveryProfile/1\"",
  )
  assert string.contains(create_json, "\"name\":\"Local custom shipping\"")
  assert string.contains(create_json, "\"originLocationCount\":1")
  assert string.contains(create_json, "\"zoneCountryCount\":1")
  assert string.contains(create_json, "\"activeMethodDefinitionsCount\":1")
  assert string.contains(
    create_json,
    "\"product\":{\"id\":\"gid://shopify/Product/1\",\"title\":\"Shipping Snowboard\"}",
  )
  assert string.contains(
    create_json,
    "\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Default Title\"}]}",
  )
  assert string.contains(
    create_json,
    "\"locationGroup\":{\"id\":\"gid://shopify/DeliveryLocationGroup/2\"",
  )
  assert string.contains(
    create_json,
    "\"zone\":{\"id\":\"gid://shopify/DeliveryZone/3\",\"name\":\"Domestic\"}",
  )
  assert string.contains(
    create_json,
    "\"id\":\"gid://shopify/DeliveryCondition/6?operator=greater_than_or_equal_to\"",
  )

  let update_profile =
    root_field.ObjectVal(
      dict.from_list([
        #("name", root_field.StringVal("Local custom shipping updated")),
        #(
          "variantsToDissociate",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/ProductVariant/1"),
          ]),
        ),
        #(
          "conditionsToDelete",
          root_field.ListVal([
            root_field.StringVal(
              "gid://shopify/DeliveryCondition/6?operator=greater_than_or_equal_to",
            ),
          ]),
        ),
        #(
          "locationGroupsToUpdate",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "id",
                  root_field.StringVal("gid://shopify/DeliveryLocationGroup/2"),
                ),
                #(
                  "locationsToAdd",
                  root_field.ListVal([
                    root_field.StringVal("gid://shopify/Location/2"),
                  ]),
                ),
                #(
                  "zonesToUpdate",
                  root_field.ListVal([
                    root_field.ObjectVal(
                      dict.from_list([
                        #(
                          "id",
                          root_field.StringVal("gid://shopify/DeliveryZone/3"),
                        ),
                        #("name", root_field.StringVal("Domestic updated")),
                        #(
                          "methodDefinitionsToUpdate",
                          root_field.ListVal([
                            root_field.ObjectVal(
                              dict.from_list([
                                #(
                                  "id",
                                  root_field.StringVal(
                                    "gid://shopify/DeliveryMethodDefinition/4",
                                  ),
                                ),
                                #("active", root_field.BoolVal(False)),
                              ]),
                            ),
                          ]),
                        ),
                        #(
                          "methodDefinitionsToCreate",
                          root_field.ListVal([
                            root_field.ObjectVal(
                              dict.from_list([
                                #("name", root_field.StringVal("Express")),
                                #("active", root_field.BoolVal(True)),
                              ]),
                            ),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ]),
    )
  let update_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation UpdateDeliveryProfile($id: ID!, $profile: DeliveryProfileInput!) { deliveryProfileUpdate(id: $id, profile: $profile) { profile { id name version originLocationCount zoneCountryCount activeMethodDefinitionsCount productVariantsCount { count precision } profileItems(first: 5) { nodes { product { id } } } } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DeliveryProfile/1")),
        #("profile", update_profile),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"deliveryProfileUpdate\":{\"profile\":{\"id\":\"gid://shopify/DeliveryProfile/1\",\"name\":\"Local custom shipping updated\",\"version\":2,\"originLocationCount\":2,\"zoneCountryCount\":1,\"activeMethodDefinitionsCount\":1,\"productVariantsCount\":{\"count\":0,\"precision\":\"EXACT\"},\"profileItems\":{\"nodes\":[]}},\"userErrors\":[]}}}"

  let remove_outcome =
    shipping_fulfillments.process_mutation(
      update_outcome.store,
      update_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation RemoveDeliveryProfile($id: ID!) { deliveryProfileRemove(id: $id) { job { id done } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DeliveryProfile/1")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(remove_outcome.data)
    == "{\"data\":{\"deliveryProfileRemove\":{\"job\":{\"id\":\"gid://shopify/Job/7\",\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(read_after_remove) =
    shipping_fulfillments.process(
      remove_outcome.store,
      "query ReadAfterRemove($id: ID!) { deliveryProfile(id: $id) { id } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DeliveryProfile/1")),
      ]),
    )
  assert json.to_string(read_after_remove)
    == "{\"data\":{\"deliveryProfile\":null}}"
}

pub fn delivery_profile_lifecycle_validation_branches_test() {
  let create_outcome =
    shipping_fulfillments.process_mutation(
      delivery_profile_lifecycle_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation BlankCreate($profile: DeliveryProfileInput!) { deliveryProfileCreate(profile: $profile) { profile { id } userErrors { field message code } } }",
      dict.from_list([
        #(
          "profile",
          root_field.ObjectVal(
            dict.from_list([
              #("name", root_field.StringVal("")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":null,\"userErrors\":[{\"field\":[\"profile\",\"name\"],\"message\":\"Add a profile name\",\"code\":\"PROFILE_CREATE_REQUIRES_NAME\"}]}}}"

  let update_outcome =
    shipping_fulfillments.process_mutation(
      delivery_profile_lifecycle_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation MissingUpdate($id: ID!, $profile: DeliveryProfileInput!) { deliveryProfileUpdate(id: $id, profile: $profile) { profile { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DeliveryProfile/999")),
        #(
          "profile",
          root_field.ObjectVal(
            dict.from_list([
              #("name", root_field.StringVal("Nope")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"deliveryProfileUpdate\":{\"profile\":null,\"userErrors\":[{\"field\":null,\"message\":\"Profile could not be updated.\"}]}}}"

  let missing_remove_outcome =
    shipping_fulfillments.process_mutation(
      delivery_profile_lifecycle_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation MissingRemove($id: ID!) { deliveryProfileRemove(id: $id) { job { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DeliveryProfile/999")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_remove_outcome.data)
    == "{\"data\":{\"deliveryProfileRemove\":{\"job\":null,\"userErrors\":[{\"field\":null,\"message\":\"The Delivery Profile cannot be found for the shop.\"}]}}}"

  let default_remove_outcome =
    shipping_fulfillments.process_mutation(
      delivery_profile_lifecycle_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation DefaultRemove($id: ID!) { deliveryProfileRemove(id: $id) { job { id } userErrors { field message } } }",
      dict.from_list([
        #(
          "id",
          root_field.StringVal("gid://shopify/DeliveryProfile/125254992178"),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(default_remove_outcome.data)
    == "{\"data\":{\"deliveryProfileRemove\":{\"job\":null,\"userErrors\":[{\"field\":null,\"message\":\"Cannot delete the default profile.\"}]}}}"
}

pub fn delivery_profile_create_validation_returns_coded_errors_test() {
  let long_name =
    delivery_profile_create_validation_json(
      delivery_profile_lifecycle_store(),
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal(string.repeat("x", times: 300))),
        ]),
      ),
    )
  assert long_name
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":null,\"userErrors\":[{\"field\":[\"profile\",\"name\"],\"message\":\"Profile name must be less than 128 characters long\",\"code\":\"TOO_LONG\"}]}}}"

  let unknown_location =
    delivery_profile_create_validation_json(
      delivery_profile_lifecycle_store(),
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal("Bad location")),
          #(
            "locationGroupsToCreate",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "locations",
                    root_field.ListVal([
                      root_field.StringVal("gid://shopify/Location/999999999"),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    )
  assert unknown_location
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":null,\"userErrors\":[{\"field\":[\"profile\",\"locationGroupsToCreate.0.locations.0\"],\"message\":\"The Location could not be found for this shop.\",\"code\":\"LOCATION_NOT_FOUND\"}]}}}"

  let empty_countries =
    delivery_profile_create_validation_json(
      delivery_profile_lifecycle_store(),
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal("Empty countries")),
          #(
            "locationGroupsToCreate",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "locations",
                    root_field.ListVal([
                      root_field.StringVal("gid://shopify/Location/1"),
                    ]),
                  ),
                  #(
                    "zonesToCreate",
                    root_field.ListVal([
                      root_field.ObjectVal(
                        dict.from_list([
                          #("name", root_field.StringVal("Empty")),
                          #("countries", root_field.ListVal([])),
                        ]),
                      ),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    )
  assert empty_countries
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":null,\"userErrors\":[{\"field\":[\"profile\",\"locationGroupsToCreate.0.zonesToCreate.0.countries\"],\"message\":\"Profile is invalid: cannot create LocationGroupZone without countries.\",\"code\":\"CANNOT_UPDATE_ZONES\"}]}}}"

  let overlapping_zones =
    delivery_profile_create_validation_json(
      delivery_profile_lifecycle_store(),
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal("Overlap")),
          #(
            "locationGroupsToCreate",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "locations",
                    root_field.ListVal([
                      root_field.StringVal("gid://shopify/Location/1"),
                    ]),
                  ),
                  #(
                    "zonesToCreate",
                    root_field.ListVal([
                      root_field.ObjectVal(
                        dict.from_list([
                          #("name", root_field.StringVal("One")),
                          #(
                            "countries",
                            root_field.ListVal([
                              root_field.ObjectVal(
                                dict.from_list([
                                  #("code", root_field.StringVal("US")),
                                ]),
                              ),
                            ]),
                          ),
                        ]),
                      ),
                      root_field.ObjectVal(
                        dict.from_list([
                          #("name", root_field.StringVal("Two")),
                          #(
                            "countries",
                            root_field.ListVal([
                              root_field.ObjectVal(
                                dict.from_list([
                                  #("code", root_field.StringVal("US")),
                                ]),
                              ),
                            ]),
                          ),
                        ]),
                      ),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    )
  assert overlapping_zones
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":null,\"userErrors\":[{\"field\":[\"profile\",\"locationGroupsToCreate.0.zonesToCreate.1.countries.0\"],\"message\":\"Profile is invalid: zones cannot contain overlapping countries.\",\"code\":\"CANNOT_UPDATE_ZONES\"}]}}}"
}

pub fn delivery_profile_create_accepts_staged_location_test() {
  let #(_, store_with_staged_location) =
    store.upsert_staged_store_property_location(
      delivery_profile_lifecycle_store(),
      location(
        "gid://shopify/Location/staged",
        "Staged shop location",
        True,
        False,
      ),
    )
  let result =
    delivery_profile_create_validation_json(
      store_with_staged_location,
      root_field.ObjectVal(
        dict.from_list([
          #("name", root_field.StringVal("Staged location profile")),
          #(
            "locationGroupsToCreate",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "locations",
                    root_field.ListVal([
                      root_field.StringVal("gid://shopify/Location/staged"),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    )
  assert result
    == "{\"data\":{\"deliveryProfileCreate\":{\"profile\":{\"id\":\"gid://shopify/DeliveryProfile/1\",\"name\":\"Staged location profile\"},\"userErrors\":[]}}}"
}

fn delivery_profile_create_validation_json(
  draft_store: store.Store,
  profile: root_field.ResolvedValue,
) -> String {
  let outcome =
    shipping_fulfillments.process_mutation(
      draft_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateDeliveryProfile($profile: DeliveryProfileInput!) { deliveryProfileCreate(profile: $profile) { profile { id name } userErrors { field message code } } }",
      dict.from_list([#("profile", profile)]),
      empty_upstream_context(),
    )
  json.to_string(outcome.data)
}

pub fn local_pickup_enable_disable_updates_downstream_location_read_test() {
  let local_pickup_settings =
    root_field.ObjectVal(
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
        #("pickupTime", root_field.StringVal("TWO_HOURS")),
        #("instructions", root_field.StringVal("HAR-493 pickup instructions")),
      ]),
    )
  let enable_outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([#("localPickupSettings", local_pickup_settings)]),
      empty_upstream_context(),
    )

  assert json.to_string(enable_outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":{\"pickupTime\":\"TWO_HOURS\",\"instructions\":\"HAR-493 pickup instructions\"},\"userErrors\":[]}}}"

  let assert Ok(after_enable_read) =
    store_properties.process(
      enable_outcome.store,
      "query ReadPickup($locationId: ID!) { location(id: $locationId) { id name localPickupSettingsV2 { pickupTime instructions } } }",
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
      ]),
    )
  assert json.to_string(after_enable_read)
    == "{\"data\":{\"location\":{\"id\":\"gid://shopify/Location/10\",\"name\":\"Shop location\",\"localPickupSettingsV2\":{\"pickupTime\":\"TWO_HOURS\",\"instructions\":\"HAR-493 pickup instructions\"}}}}"

  let disable_outcome =
    shipping_fulfillments.process_mutation(
      enable_outcome.store,
      enable_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DisablePickup($locationId: ID!) { locationLocalPickupDisable(locationId: $locationId) { locationId userErrors { field message code } } }",
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(disable_outcome.data)
    == "{\"data\":{\"locationLocalPickupDisable\":{\"locationId\":\"gid://shopify/Location/10\",\"userErrors\":[]}}}"

  let assert Ok(after_disable_read) =
    store_properties.process(
      disable_outcome.store,
      "query ReadPickup($locationId: ID!) { location(id: $locationId) { id name localPickupSettingsV2 { pickupTime instructions } } }",
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
      ]),
    )
  assert json.to_string(after_disable_read)
    == "{\"data\":{\"location\":{\"id\":\"gid://shopify/Location/10\",\"name\":\"Shop location\",\"localPickupSettingsV2\":null}}}"
}

pub fn local_pickup_unknown_location_returns_active_location_error_test() {
  let local_pickup_settings =
    root_field.ObjectVal(
      dict.from_list([
        #(
          "locationId",
          root_field.StringVal("gid://shopify/Location/999999999999"),
        ),
        #("pickupTime", root_field.StringVal("ONE_HOUR")),
      ]),
    )
  let outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([#("localPickupSettings", local_pickup_settings)]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":null,\"userErrors\":[{\"field\":[\"localPickupSettings\"],\"message\":\"Unable to find an active location for location ID 999999999999\",\"code\":\"ACTIVE_LOCATION_NOT_FOUND\"}]}}}"
}

pub fn local_pickup_inactive_location_returns_active_location_error_test() {
  let local_pickup_settings =
    root_field.ObjectVal(
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/12")),
        #("pickupTime", root_field.StringVal("ONE_HOUR")),
      ]),
    )
  let outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([#("localPickupSettings", local_pickup_settings)]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":null,\"userErrors\":[{\"field\":[\"localPickupSettings\"],\"message\":\"Unable to find an active location for location ID 12\",\"code\":\"ACTIVE_LOCATION_NOT_FOUND\"}]}}}"
}

pub fn local_pickup_custom_pickup_time_returns_code_error_test() {
  let local_pickup_settings =
    root_field.ObjectVal(
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
        #("pickupTime", root_field.StringVal("CUSTOM")),
        #("instructions", root_field.StringVal("HAR-567 custom pickup time")),
      ]),
    )
  let outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([#("localPickupSettings", local_pickup_settings)]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":null,\"userErrors\":[{\"field\":[\"localPickupSettings\"],\"message\":\"Custom pickup time is not allowed for local pickup settings.\",\"code\":\"CUSTOM_PICKUP_TIME_NOT_ALLOWED\"}]}}}"

  let assert Ok(read_after_invalid) =
    store_properties.process(
      outcome.store,
      "query ReadPickup($locationId: ID!) { location(id: $locationId) { id name localPickupSettingsV2 { pickupTime instructions } } }",
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
      ]),
    )
  assert json.to_string(read_after_invalid)
    == "{\"data\":{\"location\":{\"id\":\"gid://shopify/Location/10\",\"name\":\"Shop location\",\"localPickupSettingsV2\":null}}}"
}

pub fn local_pickup_accepts_captured_multi_day_standard_values_test() {
  let two_to_four_days_settings =
    root_field.ObjectVal(
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
        #("pickupTime", root_field.StringVal("TWO_TO_FOUR_DAYS")),
        #("instructions", root_field.StringVal("HAR-567 two to four days")),
      ]),
    )
  let two_to_four_days_outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([
        #("localPickupSettings", two_to_four_days_settings),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(two_to_four_days_outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":{\"pickupTime\":\"TWO_TO_FOUR_DAYS\",\"instructions\":\"HAR-567 two to four days\"},\"userErrors\":[]}}}"

  let five_or_more_days_settings =
    root_field.ObjectVal(
      dict.from_list([
        #("locationId", root_field.StringVal("gid://shopify/Location/10")),
        #("pickupTime", root_field.StringVal("FIVE_OR_MORE_DAYS")),
        #("instructions", root_field.StringVal("HAR-567 five or more days")),
      ]),
    )
  let five_or_more_days_outcome =
    shipping_fulfillments.process_mutation(
      settings_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation EnablePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) { locationLocalPickupEnable(localPickupSettings: $localPickupSettings) { localPickupSettings { pickupTime instructions } userErrors { field message code } } }",
      dict.from_list([
        #("localPickupSettings", five_or_more_days_settings),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(five_or_more_days_outcome.data)
    == "{\"data\":{\"locationLocalPickupEnable\":{\"localPickupSettings\":{\"pickupTime\":\"FIVE_OR_MORE_DAYS\",\"instructions\":\"HAR-567 five or more days\"},\"userErrors\":[]}}}"
}

pub fn fulfillment_service_create_update_read_and_delete_lifecycle_test() {
  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateFs($name: String!) { fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) { fulfillmentService { id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } } userErrors { field message } } }",
      dict.from_list([#("name", root_field.StringVal("Hermes FS"))]),
      empty_upstream_context(),
    )
  let assert [created, ..] =
    store.list_effective_fulfillment_services(create_outcome.store)
  let location_id = created.location_id |> option.unwrap("")
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"fulfillmentServiceCreate\":{\"fulfillmentService\":{\"id\":\""
    <> created.id
    <> "\",\"handle\":\"hermes-fs\",\"serviceName\":\"Hermes FS\",\"callbackUrl\":null,\"trackingSupport\":true,\"inventoryManagement\":true,\"requiresShippingMethod\":true,\"type\":\"THIRD_PARTY\",\"location\":{\"id\":\""
    <> location_id
    <> "\",\"name\":\"Hermes FS\",\"isFulfillmentService\":true,\"fulfillsOnlineOrders\":true,\"shipsInventory\":false}},\"userErrors\":[]}}}"

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      create_outcome.store,
      "query ReadFs($id: ID!, $locationId: ID!) { fulfillmentService(id: $id) { id handle serviceName location { id name isFulfillmentService } } location(id: $locationId) { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } }",
      dict.from_list([
        #("id", root_field.StringVal(created.id)),
        #("locationId", root_field.StringVal(location_id)),
      ]),
    )
  assert json.to_string(read_response)
    == "{\"data\":{\"fulfillmentService\":{\"id\":\""
    <> created.id
    <> "\",\"handle\":\"hermes-fs\",\"serviceName\":\"Hermes FS\",\"location\":{\"id\":\""
    <> location_id
    <> "\",\"name\":\"Hermes FS\",\"isFulfillmentService\":true}},\"location\":{\"id\":\""
    <> location_id
    <> "\",\"name\":\"Hermes FS\",\"isFulfillmentService\":true,\"fulfillsOnlineOrders\":true,\"shipsInventory\":false}}}"

  let update_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation UpdateFs($id: ID!, $name: String!) { fulfillmentServiceUpdate(id: $id, name: $name, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) { fulfillmentService { id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal(created.id)),
        #("name", root_field.StringVal("Hermes FS Updated")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"fulfillmentServiceUpdate\":{\"fulfillmentService\":{\"id\":\""
    <> created.id
    <> "\",\"handle\":\"hermes-fs\",\"serviceName\":\"Hermes FS Updated\",\"callbackUrl\":null,\"trackingSupport\":false,\"inventoryManagement\":false,\"requiresShippingMethod\":false,\"type\":\"THIRD_PARTY\",\"location\":{\"id\":\""
    <> location_id
    <> "\",\"name\":\"Hermes FS Updated\",\"isFulfillmentService\":true,\"fulfillsOnlineOrders\":true,\"shipsInventory\":false}},\"userErrors\":[]}}}"

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      update_outcome.store,
      update_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteFs($id: ID!) { fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) { deletedId userErrors { field message } } }",
      dict.from_list([#("id", root_field.StringVal(created.id))]),
      empty_upstream_context(),
    )
  let deleted_id =
    string.split(created.id, "?") |> list.first |> result.unwrap(created.id)
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":\""
    <> deleted_id
    <> "\",\"userErrors\":[]}}}"
  assert store.get_effective_fulfillment_service_by_id(
      delete_outcome.store,
      created.id,
    )
    == None
  assert store.get_effective_store_property_location_by_id(
      delete_outcome.store,
      location_id,
    )
    == None
}

pub fn fulfillment_service_validation_branches_return_user_errors_test() {
  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation InvalidFs($name: String!, $callbackUrl: URL) { fulfillmentServiceCreate(name: $name, callbackUrl: $callbackUrl) { fulfillmentService { id } userErrors { field message } } }",
      dict.from_list([
        #("name", root_field.StringVal("")),
        #(
          "callbackUrl",
          root_field.StringVal("https://example.com/fulfillment-service"),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"fulfillmentServiceCreate\":{\"fulfillmentService\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name can't be blank\"},{\"field\":[\"callbackUrl\"],\"message\":\"Callback url is not allowed\"}]}}}"

  let update_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation UnknownUpdate($id: ID!) { fulfillmentServiceUpdate(id: $id, name: \"Nope\") { fulfillmentService { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/FulfillmentService/999")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"fulfillmentServiceUpdate\":{\"fulfillmentService\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Fulfillment service could not be found.\"}]}}}"

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation UnknownDelete($id: ID!) { fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) { deletedId userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/FulfillmentService/999")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Fulfillment service could not be found.\"}]}}}"
}

pub fn fulfillment_service_delete_transfer_validates_destination_test() {
  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateFs($name: String!) { fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) { fulfillmentService { id } userErrors { field message } } }",
      dict.from_list([#("name", root_field.StringVal("Hermes FS"))]),
      empty_upstream_context(),
    )
  let assert [created, ..] =
    store.list_effective_fulfillment_services(create_outcome.store)

  let missing_destination_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteFs($id: ID!) { fulfillmentServiceDelete(id: $id, inventoryAction: TRANSFER) { deletedId userErrors { field message code } } }",
      dict.from_list([#("id", root_field.StringVal(created.id))]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_destination_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":null,\"message\":\"Invalid destination location.\",\"code\":null}]}}}"
  assert store.get_effective_fulfillment_service_by_id(
      missing_destination_outcome.store,
      created.id,
    )
    == Some(created)

  let invalid_destination_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteFs($id: ID!, $destinationLocationId: ID!) { fulfillmentServiceDelete(id: $id, destinationLocationId: $destinationLocationId, inventoryAction: TRANSFER) { deletedId userErrors { field message code } } }",
      dict.from_list([
        #("id", root_field.StringVal(created.id)),
        #(
          "destinationLocationId",
          root_field.StringVal("gid://shopify/Location/999999999"),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(invalid_destination_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":null,\"message\":\"Invalid destination location.\",\"code\":null}]}}}"
  assert store.get_effective_fulfillment_service_by_id(
      invalid_destination_outcome.store,
      created.id,
    )
    == Some(created)
}

pub fn fulfillment_service_delete_transfer_reassigns_fulfillment_orders_test() {
  let destination_id = "gid://shopify/Location/destination"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/har-571-transfer"

  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateFs($name: String!) { fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) { fulfillmentService { id } userErrors { field message } } }",
      dict.from_list([#("name", root_field.StringVal("Hermes FS"))]),
      empty_upstream_context(),
    )
  let assert [created, ..] =
    store.list_effective_fulfillment_services(create_outcome.store)
  let service_location_id = created.location_id |> option.unwrap("")
  let #(_, store_with_order) =
    create_outcome.store
    |> store.upsert_base_store_property_location(location(
      destination_id,
      "Destination location",
      True,
      False,
    ))
    |> store.stage_upsert_fulfillment_order(fulfillment_order_at_location(
      fulfillment_order_id,
      service_location_id,
      "Hermes FS",
    ))

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      store_with_order,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteFs($id: ID!, $destinationLocationId: ID!) { fulfillmentServiceDelete(id: $id, destinationLocationId: $destinationLocationId, inventoryAction: TRANSFER) { deletedId userErrors { field message code } } }",
      dict.from_list([
        #("id", root_field.StringVal(created.id)),
        #("destinationLocationId", root_field.StringVal(destination_id)),
      ]),
      empty_upstream_context(),
    )
  let deleted_id =
    string.split(created.id, "?") |> list.first |> result.unwrap(created.id)
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":\""
    <> deleted_id
    <> "\",\"userErrors\":[]}}}"

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      delete_outcome.store,
      "query ReadFo($id: ID!) { fulfillmentOrder(id: $id) { id status assignedLocation { name location { id name } } } }",
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
    )
  assert json.to_string(read_response)
    == "{\"data\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/har-571-transfer\",\"status\":\"OPEN\",\"assignedLocation\":{\"name\":\"Destination location\",\"location\":{\"id\":\"gid://shopify/Location/destination\",\"name\":\"Destination location\"}}}}}"
}

pub fn fulfillment_service_delete_keep_closes_fulfillment_orders_test() {
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/har-571-keep"

  let create_outcome =
    shipping_fulfillments.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateFs($name: String!) { fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) { fulfillmentService { id } userErrors { field message } } }",
      dict.from_list([#("name", root_field.StringVal("Hermes FS"))]),
      empty_upstream_context(),
    )
  let assert [created, ..] =
    store.list_effective_fulfillment_services(create_outcome.store)
  let service_location_id = created.location_id |> option.unwrap("")
  let #(_, store_with_order) =
    create_outcome.store
    |> store.stage_upsert_fulfillment_order(fulfillment_order_at_location(
      fulfillment_order_id,
      service_location_id,
      "Hermes FS",
    ))

  let delete_outcome =
    shipping_fulfillments.process_mutation(
      store_with_order,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation DeleteFs($id: ID!) { fulfillmentServiceDelete(id: $id, inventoryAction: KEEP) { deletedId userErrors { field message code } } }",
      dict.from_list([#("id", root_field.StringVal(created.id))]),
      empty_upstream_context(),
    )
  let deleted_id =
    string.split(created.id, "?") |> list.first |> result.unwrap(created.id)
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"fulfillmentServiceDelete\":{\"deletedId\":\""
    <> deleted_id
    <> "\",\"userErrors\":[]}}}"

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      delete_outcome.store,
      "query ReadAfterKeep($fulfillmentOrderId: ID!, $locationId: ID!) { fulfillmentOrder(id: $fulfillmentOrderId) { id status assignedLocation { location { id } } } location(id: $locationId) { id isFulfillmentService fulfillmentService { id } } }",
      dict.from_list([
        #("fulfillmentOrderId", root_field.StringVal(fulfillment_order_id)),
        #("locationId", root_field.StringVal(service_location_id)),
      ]),
    )
  assert json.to_string(read_response)
    == "{\"data\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/har-571-keep\",\"status\":\"CLOSED\",\"assignedLocation\":{\"location\":{\"id\":\""
    <> service_location_id
    <> "\"}}},\"location\":{\"id\":\""
    <> service_location_id
    <> "\",\"isFulfillmentService\":false,\"fulfillmentService\":null}}}"
}

pub fn fulfillment_event_create_stages_event_and_downstream_read_test() {
  let fulfillment_id = "gid://shopify/Fulfillment/event-flow"
  let base_store =
    store.new()
    |> store.upsert_base_fulfillments([
      FulfillmentRecord(
        id: fulfillment_id,
        order_id: Some("gid://shopify/Order/event-flow"),
        data: CapturedObject([
          #("id", CapturedString(fulfillment_id)),
          #("status", CapturedString("SUCCESS")),
          #("displayStatus", CapturedString("FULFILLED")),
          #("deliveredAt", CapturedNull),
          #("estimatedDeliveryAt", CapturedNull),
          #("inTransitAt", CapturedNull),
          #("trackingInfo", CapturedArray([])),
          #(
            "events",
            CapturedObject([
              #("nodes", CapturedArray([])),
              #(
                "pageInfo",
                CapturedObject([
                  #("hasNextPage", CapturedBool(False)),
                  #("hasPreviousPage", CapturedBool(False)),
                  #("startCursor", CapturedNull),
                  #("endCursor", CapturedNull),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])

  let outcome =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation Event($fulfillmentEvent: FulfillmentEventInput!) { fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) { fulfillmentEvent { id status message happenedAt createdAt estimatedDeliveryAt city province country zip address1 latitude longitude } userErrors { field message } } }",
      dict.from_list([
        #(
          "fulfillmentEvent",
          root_field.ObjectVal(
            dict.from_list([
              #("fulfillmentId", root_field.StringVal(fulfillment_id)),
              #("status", root_field.StringVal("IN_TRANSIT")),
              #("message", root_field.StringVal("Package scanned in transit")),
              #("happenedAt", root_field.StringVal("2026-04-25T22:25:00Z")),
              #(
                "estimatedDeliveryAt",
                root_field.StringVal("2026-04-27T18:00:00Z"),
              ),
              #("city", root_field.StringVal("Toronto")),
              #("province", root_field.StringVal("Ontario")),
              #("country", root_field.StringVal("Canada")),
              #("zip", root_field.StringVal("M5H 2M9")),
              #("address1", root_field.StringVal("123 Queen St W")),
              #("latitude", root_field.FloatVal(43.6532)),
              #("longitude", root_field.FloatVal(-79.3832)),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )

  let assert [updated_fulfillment, ..] =
    store.list_effective_fulfillments(outcome.store)
  let event_id =
    captured_event_id(updated_fulfillment.data) |> result.unwrap("")
  assert json.to_string(outcome.data)
    == "{\"data\":{\"fulfillmentEventCreate\":{\"fulfillmentEvent\":{\"id\":\""
    <> event_id
    <> "\",\"status\":\"IN_TRANSIT\",\"message\":\"Package scanned in transit\",\"happenedAt\":\"2026-04-25T22:25:00Z\",\"createdAt\":\"2026-04-28T02:25:00Z\",\"estimatedDeliveryAt\":\"2026-04-27T18:00:00Z\",\"city\":\"Toronto\",\"province\":\"Ontario\",\"country\":\"Canada\",\"zip\":\"M5H 2M9\",\"address1\":\"123 Queen St W\",\"latitude\":43.6532,\"longitude\":-79.3832},\"userErrors\":[]}}}"

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      outcome.store,
      "query Read($id: ID!) { fulfillment(id: $id) { id displayStatus estimatedDeliveryAt inTransitAt events(first: 5) { nodes { id status message happenedAt } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }",
      dict.from_list([#("id", root_field.StringVal(fulfillment_id))]),
    )
  assert json.to_string(read_response)
    == "{\"data\":{\"fulfillment\":{\"id\":\""
    <> fulfillment_id
    <> "\",\"displayStatus\":\"IN_TRANSIT\",\"estimatedDeliveryAt\":\"2026-04-27T18:00:00Z\",\"inTransitAt\":\"2026-04-25T22:25:00Z\",\"events\":{\"nodes\":[{\"id\":\""
    <> event_id
    <> "\",\"status\":\"IN_TRANSIT\",\"message\":\"Package scanned in transit\",\"happenedAt\":\"2026-04-25T22:25:00Z\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"cursor:"
    <> event_id
    <> "\",\"endCursor\":\"cursor:"
    <> event_id
    <> "\"}}}}}"
}

pub fn fulfillment_order_cancel_preconditions_direct_handler_test() {
  let closed_order_id = "gid://shopify/FulfillmentOrder/direct-closed"
  let progress_order_id = "gid://shopify/FulfillmentOrder/direct-progress"
  let base_store =
    store.new()
    |> store.upsert_base_fulfillment_orders([
      fulfillment_order_record(closed_order_id, "CLOSED", "UNSUBMITTED"),
      fulfillment_order_record(progress_order_id, "OPEN", "UNSUBMITTED"),
    ])

  let cancel_mutation =
    "
    mutation Cancel($id: ID!) {
      fulfillmentOrderCancel(id: $id) {
        fulfillmentOrder {
          id
        }
        replacementFulfillmentOrder {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let closed_cancel =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(closed_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(closed_cancel.data)
    == "{\"data\":{\"fulfillmentOrderCancel\":{\"fulfillmentOrder\":null,\"replacementFulfillmentOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Fulfillment order is not in cancelable request state and can't be canceled.\",\"code\":\"fulfillment_order_cannot_be_cancelled\"}]}}}"

  let report_mutation =
    "
    mutation Progress($id: ID!) {
      fulfillmentOrderReportProgress(id: $id, progressReport: { reasonNotes: \"manual progress\" }) {
        fulfillmentOrder {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let progress_report =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      report_mutation,
      dict.from_list([#("id", root_field.StringVal(progress_order_id))]),
      empty_upstream_context(),
    )
  let progress_cancel =
    shipping_fulfillments.process_mutation(
      progress_report.store,
      progress_report.identity,
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(progress_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(progress_cancel.data)
    == "{\"data\":{\"fulfillmentOrderCancel\":{\"fulfillmentOrder\":null,\"replacementFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.\",\"code\":\"fulfillment_order_has_manually_reported_progress\"}]}}}"
}

fn fulfillment_order_record(
  id: String,
  status: String,
  request_status: String,
) -> FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    id: id,
    order_id: None,
    status: status,
    request_status: request_status,
    assigned_location_id: None,
    assignment_status: None,
    manually_held: False,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("status", CapturedString(status)),
      #("requestStatus", CapturedString(request_status)),
      #("lineItems", CapturedObject([#("nodes", CapturedArray([]))])),
      #("fulfillmentHolds", CapturedArray([])),
    ]),
  )
}

fn split_fulfillment_order_record(
  id: String,
  line_items: List(#(String, String, Int)),
) -> FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    id: id,
    order_id: Some("gid://shopify/Order/har-559"),
    status: "OPEN",
    request_status: "UNSUBMITTED",
    assigned_location_id: None,
    assignment_status: None,
    manually_held: False,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("status", CapturedString("OPEN")),
      #("requestStatus", CapturedString("UNSUBMITTED")),
      #(
        "supportedActions",
        CapturedArray([
          CapturedObject([#("action", CapturedString("SPLIT"))]),
        ]),
      ),
      #(
        "lineItems",
        CapturedObject([
          #(
            "nodes",
            CapturedArray(
              list.map(line_items, fn(item) {
                let #(fulfillment_order_line_item_id, line_item_id, quantity) =
                  item
                CapturedObject([
                  #("id", CapturedString(fulfillment_order_line_item_id)),
                  #("totalQuantity", CapturedInt(quantity)),
                  #("remainingQuantity", CapturedInt(quantity)),
                  #(
                    "lineItem",
                    CapturedObject([
                      #("id", CapturedString(line_item_id)),
                      #("quantity", CapturedInt(quantity)),
                      #("fulfillableQuantity", CapturedInt(quantity)),
                    ]),
                  ),
                ])
              }),
            ),
          ),
        ]),
      ),
      #("fulfillmentHolds", CapturedArray([])),
    ]),
  )
}

fn split_input(
  fulfillment_order_id: String,
  line_items: List(#(String, Int)),
) -> root_field.ResolvedValue {
  root_field.ObjectVal(
    dict.from_list([
      #("fulfillmentOrderId", root_field.StringVal(fulfillment_order_id)),
      #(
        "fulfillmentOrderLineItems",
        root_field.ListVal(
          list.map(line_items, fn(item) {
            let #(line_item_id, quantity) = item
            root_field.ObjectVal(
              dict.from_list([
                #("id", root_field.StringVal(line_item_id)),
                #("quantity", root_field.IntVal(quantity)),
              ]),
            )
          }),
        ),
      ),
    ]),
  )
}

fn split_mutation() -> String {
  "
    mutation Split($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
      fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
        fulfillmentOrderSplits {
          fulfillmentOrder {
            id
            lineItems(first: 10) {
              nodes {
                id
                totalQuantity
                remainingQuantity
              }
            }
          }
          remainingFulfillmentOrder {
            id
            lineItems(first: 10) {
              nodes {
                id
                totalQuantity
                remainingQuantity
              }
            }
          }
          replacementFulfillmentOrder {
            id
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

pub fn fulfillment_order_split_processes_all_inputs_and_line_items_test() {
  let order_a_id = "gid://shopify/FulfillmentOrder/har-559-a"
  let order_b_id = "gid://shopify/FulfillmentOrder/har-559-b"
  let order_a_line_1 = "gid://shopify/FulfillmentOrderLineItem/har-559-a-1"
  let order_a_line_2 = "gid://shopify/FulfillmentOrderLineItem/har-559-a-2"
  let order_b_line = "gid://shopify/FulfillmentOrderLineItem/har-559-b-1"
  let base_store =
    store.new()
    |> store.upsert_base_fulfillment_orders([
      split_fulfillment_order_record(order_a_id, [
        #(order_a_line_1, "gid://shopify/LineItem/har-559-a-1", 2),
        #(order_a_line_2, "gid://shopify/LineItem/har-559-a-2", 3),
      ]),
      split_fulfillment_order_record(order_b_id, [
        #(order_b_line, "gid://shopify/LineItem/har-559-b-1", 3),
      ]),
    ])

  let outcome =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation(),
      dict.from_list([
        #(
          "fulfillmentOrderSplits",
          root_field.ListVal([
            split_input(order_a_id, [#(order_a_line_1, 1), #(order_a_line_2, 1)]),
            split_input(order_b_id, [#(order_b_line, 2)]),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":[{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/har-559-a\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-a-1\",\"totalQuantity\":1,\"remainingQuantity\":1},{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-a-2\",\"totalQuantity\":2,\"remainingQuantity\":2}]}},\"remainingFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/1\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-a-1\",\"totalQuantity\":1,\"remainingQuantity\":1},{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-a-2\",\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"replacementFulfillmentOrder\":null},{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/har-559-b\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-b-1\",\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"remainingFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/har-559-b-1\",\"totalQuantity\":2,\"remainingQuantity\":2}]}},\"replacementFulfillmentOrder\":null}],\"userErrors\":[]}}}"
}

pub fn fulfillment_order_split_validates_indexed_inputs_test() {
  let order_id = "gid://shopify/FulfillmentOrder/har-559-validation"
  let line_item_id = "gid://shopify/FulfillmentOrderLineItem/har-559-validation"
  let base_store =
    store.new()
    |> store.upsert_base_fulfillment_orders([
      split_fulfillment_order_record(order_id, [
        #(line_item_id, "gid://shopify/LineItem/har-559-validation", 2),
      ]),
    ])

  let empty_line_items =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation(),
      dict.from_list([
        #(
          "fulfillmentOrderSplits",
          root_field.ListVal([split_input(order_id, [])]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(empty_line_items.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":null,\"userErrors\":[{\"field\":[\"fulfillmentOrderSplits\",\"0\",\"fulfillmentOrderLineItems\"],\"message\":\"There must be at least one item selected in this fulfillment to split it.\",\"code\":\"NO_LINE_ITEMS_PROVIDED_TO_SPLIT\"}]}}}"

  let zero_quantity =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation(),
      dict.from_list([
        #(
          "fulfillmentOrderSplits",
          root_field.ListVal([split_input(order_id, [#(line_item_id, 0)])]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(zero_quantity.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":null,\"userErrors\":[{\"field\":[\"fulfillmentOrderSplits\",\"0\",\"fulfillmentOrderLineItems\",\"0\",\"quantity\"],\"message\":\"You must select at least one item to split into a new fulfillment order.\",\"code\":\"GREATER_THAN\"}]}}}"

  let invalid_line_item =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation(),
      dict.from_list([
        #(
          "fulfillmentOrderSplits",
          root_field.ListVal([
            split_input(order_id, [
              #("gid://shopify/FulfillmentOrderLineItem/har-559-missing", 1),
            ]),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(invalid_line_item.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":null,\"userErrors\":[{\"field\":[\"fulfillmentOrderSplits\",\"0\",\"fulfillmentOrderLineItems\",\"0\",\"id\"],\"message\":\"Line item quantity is invalid.\",\"code\":\"INVALID_LINE_ITEM_QUANTITY\"}]}}}"

  let unknown_id = "gid://shopify/FulfillmentOrder/har-559-missing"
  let unknown_order =
    shipping_fulfillments.process_mutation(
      base_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation(),
      dict.from_list([
        #(
          "fulfillmentOrderSplits",
          root_field.ListVal([split_input(unknown_id, [#(line_item_id, 1)])]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(unknown_order.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":null,\"userErrors\":[{\"field\":null,\"message\":\"Fulfillment order does not exist.\",\"code\":\"FULFILLMENT_ORDER_NOT_FOUND\"}]}}}"
}

pub fn reverse_delivery_lifecycle_and_detail_reads_stage_locally_test() {
  let reverse_fulfillment_order_id = "gid://shopify/ReverseFulfillmentOrder/1"
  let reverse_fulfillment_order_line_item_id =
    "gid://shopify/ReverseFulfillmentOrderLineItem/1"
  let create_outcome =
    shipping_fulfillments.process_mutation(
      reverse_logistics_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation CreateReverseDelivery($reverseFulfillmentOrderId: ID!, $reverseDeliveryLineItems: [ReverseDeliveryLineItemInput!]!) { reverseDeliveryCreateWithShipping(reverseFulfillmentOrderId: $reverseFulfillmentOrderId, reverseDeliveryLineItems: $reverseDeliveryLineItems, trackingInput: { number: \"TRACK-1\", url: \"https://tracking.example/1\", company: \"Example Carrier\" }, labelInput: { publicFileUrl: \"https://labels.example/return.pdf\" }) { reverseDelivery { id reverseFulfillmentOrder { id } reverseDeliveryLineItems(first: 5) { nodes { quantity reverseFulfillmentOrderLineItem { id remainingQuantity } } } deliverable { __typename ... on ReverseDeliveryShippingDeliverable { tracking { number url company } label { publicFileUrl } } } } userErrors { field message } } }",
      dict.from_list([
        #(
          "reverseFulfillmentOrderId",
          root_field.StringVal(reverse_fulfillment_order_id),
        ),
        #(
          "reverseDeliveryLineItems",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "reverseFulfillmentOrderLineItemId",
                  root_field.StringVal(reverse_fulfillment_order_line_item_id),
                ),
                #("quantity", root_field.IntVal(1)),
              ]),
            ),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  let create_json = json.to_string(create_outcome.data)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(
    create_json,
    "\"reverseFulfillmentOrder\":{\"id\":\""
      <> reverse_fulfillment_order_id
      <> "\"}",
  )
  assert string.contains(create_json, "\"number\":\"TRACK-1\"")
  let assert [reverse_delivery_id, ..] =
    create_outcome.staged_resource_ids |> list.reverse

  let update_outcome =
    shipping_fulfillments.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation UpdateReverseDelivery($reverseDeliveryId: ID!) { reverseDeliveryShippingUpdate(reverseDeliveryId: $reverseDeliveryId, trackingInput: { number: \"TRACK-2\", url: \"https://tracking.example/2\", company: \"Updated Carrier\" }) { reverseDelivery { id deliverable { __typename ... on ReverseDeliveryShippingDeliverable { tracking { number url company } } } } userErrors { field message } } }",
      dict.from_list([
        #("reverseDeliveryId", root_field.StringVal(reverse_delivery_id)),
      ]),
      empty_upstream_context(),
    )
  assert string.contains(json.to_string(update_outcome.data), "\"TRACK-2\"")

  let dispose_outcome =
    shipping_fulfillments.process_mutation(
      update_outcome.store,
      update_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation Dispose($dispositionInputs: [ReverseFulfillmentOrderDisposeInput!]!) { reverseFulfillmentOrderDispose(dispositionInputs: $dispositionInputs) { reverseFulfillmentOrderLineItems { id remainingQuantity dispositionType } userErrors { field message } } }",
      dict.from_list([
        #(
          "dispositionInputs",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "reverseFulfillmentOrderLineItemId",
                  root_field.StringVal(reverse_fulfillment_order_line_item_id),
                ),
                #("quantity", root_field.IntVal(1)),
                #("dispositionType", root_field.StringVal("RESTOCKED")),
              ]),
            ),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  let dispose_json = json.to_string(dispose_outcome.data)
  assert string.contains(dispose_json, "\"remainingQuantity\":0")
  assert string.contains(dispose_json, "\"dispositionType\":\"RESTOCKED\"")

  let assert Ok(read_response) =
    shipping_fulfillments.process(
      dispose_outcome.store,
      "query ReverseRead($reverseDeliveryId: ID!, $reverseFulfillmentOrderId: ID!) { reverseDelivery(id: $reverseDeliveryId) { id deliverable { __typename ... on ReverseDeliveryShippingDeliverable { tracking { number } } } } reverseFulfillmentOrder(id: $reverseFulfillmentOrderId) { id lineItems(first: 5) { nodes { id remainingQuantity dispositionType } } reverseDeliveries(first: 5) { nodes { id } } } }",
      dict.from_list([
        #("reverseDeliveryId", root_field.StringVal(reverse_delivery_id)),
        #(
          "reverseFulfillmentOrderId",
          root_field.StringVal(reverse_fulfillment_order_id),
        ),
      ]),
    )
  let read_json = json.to_string(read_response)
  assert string.contains(read_json, "\"reverseDelivery\":{\"id\":\"")
  assert string.contains(read_json, "\"number\":\"TRACK-2\"")
  assert string.contains(read_json, "\"remainingQuantity\":0")
  assert string.contains(
    read_json,
    "\"reverseDeliveries\":{\"nodes\":[{\"id\":\"",
  )
}

pub fn order_edit_shipping_line_roots_stage_calculated_order_test() {
  let calculated_order_id = "gid://shopify/CalculatedOrder/1"
  let add_outcome =
    shipping_fulfillments.process_mutation(
      calculated_order_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation AddShipping($id: ID!, $price: MoneyInput!) { orderEditAddShippingLine(id: $id, shippingLine: { title: \"Ground\", price: $price }) { calculatedOrder { id totalPriceSet { shopMoney { amount currencyCode } } shippingLines { id title stagedStatus price { shopMoney { amount currencyCode } } } } calculatedShippingLine { id title stagedStatus } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #(
          "price",
          root_field.ObjectVal(
            dict.from_list([
              #("amount", root_field.StringVal("4.00")),
              #("currencyCode", root_field.StringVal("CAD")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  let add_json = json.to_string(add_outcome.data)
  assert string.contains(add_json, "\"amount\":\"14.0\"")
  assert string.contains(add_json, "\"title\":\"Ground\"")
  assert string.contains(add_json, "\"stagedStatus\":\"ADDED\"")
  let assert [_, shipping_line_id] = add_outcome.staged_resource_ids

  let update_outcome =
    shipping_fulfillments.process_mutation(
      add_outcome.store,
      add_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation UpdateShipping($id: ID!, $shippingLineId: ID!, $price: MoneyInput!) { orderEditUpdateShippingLine(id: $id, shippingLineId: $shippingLineId, shippingLine: { title: \"Express\", price: $price }) { calculatedOrder { id totalPriceSet { shopMoney { amount currencyCode } } shippingLines { id title stagedStatus } } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("shippingLineId", root_field.StringVal(shipping_line_id)),
        #(
          "price",
          root_field.ObjectVal(
            dict.from_list([
              #("amount", root_field.StringVal("6.00")),
              #("currencyCode", root_field.StringVal("CAD")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  let update_json = json.to_string(update_outcome.data)
  assert string.contains(update_json, "\"amount\":\"16.0\"")
  assert string.contains(update_json, "\"title\":\"Express\"")
  assert string.contains(update_json, "\"userErrors\":[]")

  let remove_outcome =
    shipping_fulfillments.process_mutation(
      update_outcome.store,
      update_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation RemoveShipping($id: ID!, $shippingLineId: ID!) { orderEditRemoveShippingLine(id: $id, shippingLineId: $shippingLineId) { calculatedOrder { id totalPriceSet { shopMoney { amount currencyCode } } shippingLines { id } } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("shippingLineId", root_field.StringVal(shipping_line_id)),
      ]),
      empty_upstream_context(),
    )
  let remove_json = json.to_string(remove_outcome.data)
  assert string.contains(remove_json, "\"amount\":\"10.0\"")
  assert string.contains(remove_json, "\"shippingLines\":[]")
}

fn captured_event_id(data: CapturedJsonValue) -> Result(String, Nil) {
  case data {
    CapturedObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == "events" }) {
        Ok(#(_, CapturedObject(event_fields))) ->
          case list.find(event_fields, fn(pair) { pair.0 == "nodes" }) {
            Ok(#(_, CapturedArray([CapturedObject(event_node), ..]))) ->
              case list.find(event_node, fn(pair) { pair.0 == "id" }) {
                Ok(#(_, CapturedString(id))) -> Ok(id)
                _ -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        _ -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

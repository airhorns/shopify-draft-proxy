//// Serialization helpers for Store Properties roots and node resolution.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Selection, Field, FragmentDefinition, FragmentSpread, InlineFragment,
  SelectionSet,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, read_arg_string,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/store_properties/types as store_properties_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type PaymentSettingsRecord, type ProductMetafieldRecord,
  type ShopAddressRecord, type ShopBundlesFeatureRecord,
  type ShopCartTransformEligibleOperationsRecord,
  type ShopCartTransformFeatureRecord, type ShopDomainRecord,
  type ShopFeaturesRecord, type ShopPlanRecord, type ShopPolicyRecord,
  type ShopRecord, type ShopResourceLimitsRecord, type StorePropertyRecord,
  type StorePropertyValue, PaymentSettingsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopPolicyRecord, ShopRecord, ShopResourceLimitsRecord,
  StorePropertyBool, StorePropertyFloat, StorePropertyInt, StorePropertyList,
  StorePropertyNull, StorePropertyObject, StorePropertyRecord,
  StorePropertyString,
}

@internal
pub fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
}

fn selected_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

@internal
pub fn serialize_shop_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      project_graphql_value(
        shop_source(shop),
        selected_children(field),
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_location_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let location = case read_arg_string(args, "id") {
    Some(id) -> store.get_effective_store_property_location_by_id(store, id)
    None ->
      case store.list_effective_store_property_locations(store) {
        [first, ..] -> Some(first)
        [] -> None
      }
  }
  case location {
    Some(record) ->
      serialize_location_record(store, record, field, fragments, variables)
    None -> json.null()
  }
}

@internal
pub fn serialize_location_by_identifier_result(
  store: Store,
  field: Selection,
  key: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> store_properties_types.QueryFieldResult {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "identifier") {
    Ok(root_field.ObjectVal(identifier)) ->
      case read_arg_string(identifier, "id") {
        Some(id) ->
          store_properties_types.QueryFieldResult(
            key: key,
            value: case
              store.get_effective_store_property_location_by_id(store, id)
            {
              Some(record) ->
                serialize_location_record(
                  store,
                  record,
                  field,
                  fragments,
                  variables,
                )
              None -> json.null()
            },
            errors: [],
          )
        None ->
          store_properties_types.QueryFieldResult(
            key: key,
            value: json.null(),
            errors: [
              custom_location_identifier_error(key),
            ],
          )
      }
    _ ->
      store_properties_types.QueryFieldResult(
        key: key,
        value: json.null(),
        errors: [],
      )
  }
}

@internal
pub fn serialize_locations_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let locations = store.list_effective_store_property_locations(store)
  let window =
    paginate_store_records(locations, field, variables, fn(record, _index) {
      record.cursor |> option.unwrap(record.id)
    })
  serialize_location_connection(field, window, fragments, variables, store)
}

fn paginate_store_records(
  records: List(StorePropertyRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  get_cursor: fn(StorePropertyRecord, Int) -> String,
) -> ConnectionWindow(StorePropertyRecord) {
  paginate_connection_items(
    records,
    field,
    variables,
    get_cursor,
    default_connection_window_options(),
  )
}

fn serialize_location_connection(
  field: Selection,
  window: ConnectionWindow(StorePropertyRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  store: Store,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(record, _index) {
        record.cursor |> option.unwrap(record.id)
      },
      serialize_node: fn(record, selection, _index) {
        serialize_location_record(
          store,
          record,
          selection,
          fragments,
          variables,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn serialize_business_entities_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  json.array(store.list_effective_business_entities(store), fn(record) {
    project_store_property_record(record, field, fragments)
  })
}

@internal
pub fn serialize_business_entity_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let entity = case read_arg_string(args, "id") {
    Some(id) -> store.get_business_entity_by_id(store, id)
    None ->
      find_primary_business_entity(store.list_effective_business_entities(store))
  }
  case entity {
    Some(record) -> project_store_property_record(record, field, fragments)
    None -> json.null()
  }
}

@internal
pub fn serialize_publishable_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_publishable_by_id(store, id) {
        Some(record) -> project_store_property_record(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_store_property_record(
  record: StorePropertyRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    store_property_data_to_source(record.data),
    selected_selections(field),
    fragments,
  )
}

@internal
pub fn serialize_location_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_store_property_location_by_id(store, id) {
    Some(record) ->
      serialize_location_selections(
        store,
        record,
        selections,
        fragments,
        dict.new(),
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_location_record(
  store: Store,
  record: StorePropertyRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let selections = selected_location_child_fields(field, fragments)
  case location_has_direct_metafield_selection(selections) {
    True ->
      serialize_location_selections(
        store,
        record,
        selections,
        fragments,
        variables,
      )
    False ->
      project_graphql_value(
        store_property_data_to_source(record.data),
        selections,
        fragments,
      )
  }
}

fn selected_location_child_fields(
  field: Selection,
  fragments: FragmentMap,
) -> List(Selection) {
  selected_selections(field)
  |> expand_location_fragment_selections(fragments)
}

fn expand_location_fragment_selections(
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(Selection) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(..) -> [selection]
      InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        expand_location_fragment_selections(inner, fragments)
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) -> expand_location_fragment_selections(inner, fragments)
          _ -> []
        }
    }
  })
}

fn location_has_direct_metafield_selection(
  selections: List(Selection),
) -> Bool {
  list.any(selections, fn(selection) {
    case selection {
      Field(name: name, ..) ->
        name.value == "metafield" || name.value == "metafields"
      _ -> False
    }
  })
}

fn serialize_location_selections(
  store: Store,
  record: StorePropertyRecord,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source = store_property_data_to_source(record.data)
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" -> #(
              key,
              serialize_location_metafield(
                store,
                record.id,
                selection,
                variables,
              ),
            )
            "metafields" -> #(
              key,
              serialize_location_metafields_connection(
                store,
                record.id,
                selection,
                variables,
              ),
            )
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_location_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let key = read_string_argument(field, variables, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        product_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

fn serialize_location_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(product_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

@internal
pub fn product_metafield_to_core(
  record: ProductMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: record.json_value,
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
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

fn find_primary_business_entity(
  entities: List(StorePropertyRecord),
) -> Option(StorePropertyRecord) {
  case entities {
    [] -> None
    [first, ..rest] ->
      case dict.get(first.data, "primary") {
        Ok(StorePropertyBool(True)) -> Some(first)
        _ -> find_primary_business_entity(rest)
      }
  }
}

fn custom_location_identifier_error(key: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Metafield definition of type 'id' is required when using custom ids.",
      ),
    ),
    #(
      "locations",
      json.array([#(3, 5)], fn(location) {
        json.object([
          #("line", json.int(location.0)),
          #("column", json.int(location.1)),
        ])
      }),
    ),
    #("extensions", json.object([#("code", json.string("NOT_FOUND"))])),
    #("path", json.array([key], json.string)),
  ])
}

@internal
pub fn serialize_shop_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.id == id {
        True -> project_graphql_value(shop_source(shop), selections, fragments)
        False -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_shop_address_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.shop_address.id == id {
        True ->
          project_graphql_value(
            shop_address_source(shop.shop_address),
            selections,
            fragments,
          )
        False -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_shop_policy_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case find_policy_by_id(shop.shop_policies, id) {
        Some(policy) ->
          project_graphql_value(
            shop_policy_source(policy),
            selections,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn primary_domain_for_id(
  store: Store,
  id: String,
) -> Option(ShopDomainRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.primary_domain.id == id {
        True -> Some(shop.primary_domain)
        False -> None
      }
    None -> None
  }
}

@internal
pub fn shop_source(shop: ShopRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Shop")),
    #("id", SrcString(shop.id)),
    #("name", SrcString(shop.name)),
    #("myshopifyDomain", SrcString(shop.myshopify_domain)),
    #("url", SrcString(shop.url)),
    #("primaryDomain", shop_domain_source(shop.primary_domain)),
    #("contactEmail", SrcString(shop.contact_email)),
    #("email", SrcString(shop.email)),
    #("currencyCode", SrcString(shop.currency_code)),
    #(
      "enabledPresentmentCurrencies",
      SrcList(list.map(shop.enabled_presentment_currencies, SrcString)),
    ),
    #("ianaTimezone", SrcString(shop.iana_timezone)),
    #("timezoneAbbreviation", SrcString(shop.timezone_abbreviation)),
    #("timezoneOffset", SrcString(shop.timezone_offset)),
    #("timezoneOffsetMinutes", SrcInt(shop.timezone_offset_minutes)),
    #("taxesIncluded", SrcBool(shop.taxes_included)),
    #("taxShipping", SrcBool(shop.tax_shipping)),
    #("unitSystem", SrcString(shop.unit_system)),
    #("weightUnit", SrcString(shop.weight_unit)),
    #("shopAddress", shop_address_source(shop.shop_address)),
    #("plan", shop_plan_source(shop.plan)),
    #("resourceLimits", shop_resource_limits_source(shop.resource_limits)),
    #("features", shop_features_source(shop.features)),
    #("paymentSettings", payment_settings_source(shop.payment_settings)),
    #("shopPolicies", SrcList(list.map(shop.shop_policies, shop_policy_source))),
  ])
}

@internal
pub fn shop_domain_source(domain: ShopDomainRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Domain")),
    #("id", SrcString(domain.id)),
    #("host", SrcString(domain.host)),
    #("url", SrcString(domain.url)),
    #("sslEnabled", SrcBool(domain.ssl_enabled)),
  ])
}

fn shop_address_source(address: ShopAddressRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopAddress")),
    #("id", SrcString(address.id)),
    #("address1", graphql_helpers.option_string_source(address.address1)),
    #("address2", graphql_helpers.option_string_source(address.address2)),
    #("city", graphql_helpers.option_string_source(address.city)),
    #("company", graphql_helpers.option_string_source(address.company)),
    #("coordinatesValidated", SrcBool(address.coordinates_validated)),
    #("country", graphql_helpers.option_string_source(address.country)),
    #(
      "countryCodeV2",
      graphql_helpers.option_string_source(address.country_code_v2),
    ),
    #("formatted", SrcList(list.map(address.formatted, SrcString))),
    #(
      "formattedArea",
      graphql_helpers.option_string_source(address.formatted_area),
    ),
    #("latitude", optional_float_source(address.latitude)),
    #("longitude", optional_float_source(address.longitude)),
    #("phone", graphql_helpers.option_string_source(address.phone)),
    #("province", graphql_helpers.option_string_source(address.province)),
    #(
      "provinceCode",
      graphql_helpers.option_string_source(address.province_code),
    ),
    #("zip", graphql_helpers.option_string_source(address.zip)),
  ])
}

fn shop_plan_source(plan: ShopPlanRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPlan")),
    #("partnerDevelopment", SrcBool(plan.partner_development)),
    #("publicDisplayName", SrcString(plan.public_display_name)),
    #("shopifyPlus", SrcBool(plan.shopify_plus)),
  ])
}

fn shop_resource_limits_source(
  limits: ShopResourceLimitsRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopResourceLimits")),
    #("locationLimit", SrcInt(limits.location_limit)),
    #("maxProductOptions", SrcInt(limits.max_product_options)),
    #("maxProductVariants", SrcInt(limits.max_product_variants)),
    #("redirectLimitReached", SrcBool(limits.redirect_limit_reached)),
  ])
}

fn shop_features_source(features: ShopFeaturesRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopFeatures")),
    #("avalaraAvatax", SrcBool(features.avalara_avatax)),
    #("branding", SrcString(features.branding)),
    #("bundles", shop_bundles_feature_source(features.bundles)),
    #("captcha", SrcBool(features.captcha)),
    #(
      "cartTransform",
      shop_cart_transform_feature_source(features.cart_transform),
    ),
    #("dynamicRemarketing", SrcBool(features.dynamic_remarketing)),
    #(
      "eligibleForSubscriptionMigration",
      SrcBool(features.eligible_for_subscription_migration),
    ),
    #("eligibleForSubscriptions", SrcBool(features.eligible_for_subscriptions)),
    #("giftCards", SrcBool(features.gift_cards)),
    #("harmonizedSystemCode", SrcBool(features.harmonized_system_code)),
    #(
      "legacySubscriptionGatewayEnabled",
      SrcBool(features.legacy_subscription_gateway_enabled),
    ),
    #("liveView", SrcBool(features.live_view)),
    #(
      "paypalExpressSubscriptionGatewayStatus",
      SrcString(features.paypal_express_subscription_gateway_status),
    ),
    #("reports", SrcBool(features.reports)),
    #("sellsSubscriptions", SrcBool(features.sells_subscriptions)),
    #("showMetrics", SrcBool(features.show_metrics)),
    #("storefront", SrcBool(features.storefront)),
    #("unifiedMarkets", SrcBool(features.unified_markets)),
  ])
}

fn shop_bundles_feature_source(
  feature: ShopBundlesFeatureRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopBundlesFeature")),
    #("eligibleForBundles", SrcBool(feature.eligible_for_bundles)),
    #(
      "ineligibilityReason",
      graphql_helpers.option_string_source(feature.ineligibility_reason),
    ),
    #("sellsBundles", SrcBool(feature.sells_bundles)),
  ])
}

fn shop_cart_transform_feature_source(
  feature: ShopCartTransformFeatureRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopCartTransformFeature")),
    #(
      "eligibleOperations",
      shop_cart_transform_eligible_operations_source(
        feature.eligible_operations,
      ),
    ),
  ])
}

fn shop_cart_transform_eligible_operations_source(
  operations: ShopCartTransformEligibleOperationsRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopCartTransformEligibleOperations")),
    #("expandOperation", SrcBool(operations.expand_operation)),
    #("mergeOperation", SrcBool(operations.merge_operation)),
    #("updateOperation", SrcBool(operations.update_operation)),
  ])
}

fn payment_settings_source(settings: PaymentSettingsRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentSettings")),
    #(
      "supportedDigitalWallets",
      SrcList(list.map(settings.supported_digital_wallets, SrcString)),
    ),
  ])
}

fn shop_policy_source(policy: ShopPolicyRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicy")),
    #("id", SrcString(policy.id)),
    #("title", SrcString(policy.title)),
    #("body", SrcString(render_shop_policy_body(policy))),
    #("type", SrcString(policy.type_)),
    #("url", SrcString(policy.url)),
    #("createdAt", SrcString(policy.created_at)),
    #("updatedAt", SrcString(policy.updated_at)),
    #("translations", SrcList([])),
  ])
}

@internal
pub fn shop_policy_update_payload_source(
  policy: Option(ShopPolicyRecord),
  errors: List(store_properties_types.ShopPolicyUserError),
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicyUpdatePayload")),
    #("shopPolicy", case policy {
      Some(p) -> shop_policy_source(p)
      None -> SrcNull
    }),
    #("userErrors", SrcList(list.map(errors, shop_policy_user_error_source))),
  ])
}

fn shop_policy_user_error_source(
  error: store_properties_types.ShopPolicyUserError,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("store_properties_types.ShopPolicyUserError")),
    #("field", case error.field {
      Some(parts) -> SrcList(list.map(parts, SrcString))
      None -> SrcNull
    }),
    #("message", SrcString(error.message)),
    #("code", graphql_helpers.option_string_source(error.code)),
  ])
}

@internal
pub fn json_path(
  value: commit.JsonValue,
  path: List(String),
) -> Option(commit.JsonValue) {
  case path {
    [] -> Some(value)
    [key, ..rest] ->
      case json_get(value, key) {
        Some(child) -> json_path(child, rest)
        None -> None
      }
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == key }) {
        Ok(pair) -> Some(pair.1)
        Error(_) -> None
      }
    _ -> None
  }
}

@internal
pub fn store_property_record_from_json(
  value: commit.JsonValue,
  fallback_id: String,
  fallback_typename: String,
) -> Option(StorePropertyRecord) {
  use data <- option.then(store_property_object_from_json(value))
  let id = store_property_string(data, "id") |> option.unwrap(fallback_id)
  let data = case dict.has_key(data, "__typename") {
    True -> data
    False ->
      dict.insert(
        data,
        "__typename",
        StorePropertyString(infer_typename(id, fallback_typename)),
      )
  }
  Some(StorePropertyRecord(id: id, cursor: None, data: data))
}

fn store_property_object_from_json(
  value: commit.JsonValue,
) -> Option(Dict(String, StorePropertyValue)) {
  case value {
    commit.JsonObject(fields) ->
      Some(dict.from_list(
        fields
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_from_json(pair.1))
        }),
      ))
    _ -> None
  }
}

@internal
pub fn store_property_value_from_json(
  value: commit.JsonValue,
) -> StorePropertyValue {
  case value {
    commit.JsonNull -> StorePropertyNull
    commit.JsonBool(value) -> StorePropertyBool(value)
    commit.JsonInt(value) -> StorePropertyInt(value)
    commit.JsonFloat(value) -> StorePropertyFloat(value)
    commit.JsonString(value) -> StorePropertyString(value)
    commit.JsonArray(values) ->
      StorePropertyList(list.map(values, store_property_value_from_json))
    commit.JsonObject(fields) ->
      StorePropertyObject(dict.from_list(
        fields
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_from_json(pair.1))
        }),
      ))
  }
}

fn store_property_string(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> Option(String) {
  case dict.get(data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

fn infer_typename(id: String, fallback: String) -> String {
  case string.split(id, on: "/") |> list.reverse {
    [tail, resource, ..] ->
      case string.contains(tail, "?") {
        True -> resource
        False -> resource
      }
    _ -> fallback
  }
}

@internal
pub fn shop_from_json(value: commit.JsonValue) -> Option(ShopRecord) {
  case value {
    commit.JsonObject(_) ->
      Some(ShopRecord(
        id: json_string(value, "id", ""),
        name: json_string(value, "name", ""),
        myshopify_domain: json_string(value, "myshopifyDomain", ""),
        url: json_string(value, "url", ""),
        primary_domain: shop_domain_from_json(json_object(
          value,
          "primaryDomain",
        )),
        contact_email: json_string(value, "contactEmail", ""),
        email: json_string(value, "email", ""),
        currency_code: json_string(value, "currencyCode", ""),
        enabled_presentment_currencies: json_string_list(
          value,
          "enabledPresentmentCurrencies",
        ),
        iana_timezone: json_string(value, "ianaTimezone", ""),
        timezone_abbreviation: json_string(value, "timezoneAbbreviation", ""),
        timezone_offset: json_string(value, "timezoneOffset", ""),
        timezone_offset_minutes: json_int(value, "timezoneOffsetMinutes", 0),
        taxes_included: json_bool(value, "taxesIncluded", False),
        tax_shipping: json_bool(value, "taxShipping", False),
        unit_system: json_string(value, "unitSystem", ""),
        weight_unit: json_string(value, "weightUnit", ""),
        shop_address: shop_address_from_json(json_object(value, "shopAddress")),
        plan: shop_plan_from_json(json_object(value, "plan")),
        resource_limits: shop_resource_limits_from_json(json_object(
          value,
          "resourceLimits",
        )),
        features: shop_features_from_json(json_object(value, "features")),
        payment_settings: payment_settings_from_json(json_object(
          value,
          "paymentSettings",
        )),
        shop_policies: json_array(value, "shopPolicies")
          |> list.filter_map(shop_policy_from_json),
      ))
    _ -> None
  }
}

fn shop_domain_from_json(value: commit.JsonValue) -> ShopDomainRecord {
  ShopDomainRecord(
    id: json_string(value, "id", ""),
    host: json_string(value, "host", ""),
    url: json_string(value, "url", ""),
    ssl_enabled: json_bool(value, "sslEnabled", False),
  )
}

fn shop_address_from_json(value: commit.JsonValue) -> ShopAddressRecord {
  ShopAddressRecord(
    id: json_string(value, "id", ""),
    address1: json_string_option(value, "address1"),
    address2: json_string_option(value, "address2"),
    city: json_string_option(value, "city"),
    company: json_string_option(value, "company"),
    coordinates_validated: json_bool(value, "coordinatesValidated", False),
    country: json_string_option(value, "country"),
    country_code_v2: json_string_option(value, "countryCodeV2"),
    formatted: json_string_list(value, "formatted"),
    formatted_area: json_string_option(value, "formattedArea"),
    latitude: json_float_option(value, "latitude"),
    longitude: json_float_option(value, "longitude"),
    phone: json_string_option(value, "phone"),
    province: json_string_option(value, "province"),
    province_code: json_string_option(value, "provinceCode"),
    zip: json_string_option(value, "zip"),
  )
}

fn shop_plan_from_json(value: commit.JsonValue) -> ShopPlanRecord {
  ShopPlanRecord(
    partner_development: json_bool(value, "partnerDevelopment", False),
    public_display_name: json_string(value, "publicDisplayName", ""),
    shopify_plus: json_bool(value, "shopifyPlus", False),
  )
}

fn shop_resource_limits_from_json(
  value: commit.JsonValue,
) -> ShopResourceLimitsRecord {
  ShopResourceLimitsRecord(
    location_limit: json_int(value, "locationLimit", 0),
    max_product_options: json_int(value, "maxProductOptions", 0),
    max_product_variants: json_int(value, "maxProductVariants", 0),
    redirect_limit_reached: json_bool(value, "redirectLimitReached", False),
  )
}

fn shop_features_from_json(value: commit.JsonValue) -> ShopFeaturesRecord {
  ShopFeaturesRecord(
    avalara_avatax: json_bool(value, "avalaraAvatax", False),
    branding: json_string(value, "branding", ""),
    bundles: shop_bundles_feature_from_json(json_object(value, "bundles")),
    captcha: json_bool(value, "captcha", False),
    cart_transform: shop_cart_transform_feature_from_json(json_object(
      value,
      "cartTransform",
    )),
    dynamic_remarketing: json_bool(value, "dynamicRemarketing", False),
    eligible_for_subscription_migration: json_bool(
      value,
      "eligibleForSubscriptionMigration",
      False,
    ),
    eligible_for_subscriptions: json_bool(
      value,
      "eligibleForSubscriptions",
      False,
    ),
    gift_cards: json_bool(value, "giftCards", False),
    harmonized_system_code: json_bool(value, "harmonizedSystemCode", False),
    legacy_subscription_gateway_enabled: json_bool(
      value,
      "legacySubscriptionGatewayEnabled",
      False,
    ),
    live_view: json_bool(value, "liveView", False),
    paypal_express_subscription_gateway_status: json_string(
      value,
      "paypalExpressSubscriptionGatewayStatus",
      "",
    ),
    reports: json_bool(value, "reports", False),
    sells_subscriptions: json_bool(value, "sellsSubscriptions", False),
    show_metrics: json_bool(value, "showMetrics", False),
    storefront: json_bool(value, "storefront", False),
    unified_markets: json_bool(value, "unifiedMarkets", False),
  )
}

fn shop_bundles_feature_from_json(
  value: commit.JsonValue,
) -> ShopBundlesFeatureRecord {
  ShopBundlesFeatureRecord(
    eligible_for_bundles: json_bool(value, "eligibleForBundles", False),
    ineligibility_reason: json_string_option(value, "ineligibilityReason"),
    sells_bundles: json_bool(value, "sellsBundles", False),
  )
}

fn shop_cart_transform_feature_from_json(
  value: commit.JsonValue,
) -> ShopCartTransformFeatureRecord {
  ShopCartTransformFeatureRecord(
    eligible_operations: shop_cart_transform_eligible_operations_from_json(
      json_object(value, "eligibleOperations"),
    ),
  )
}

fn shop_cart_transform_eligible_operations_from_json(
  value: commit.JsonValue,
) -> ShopCartTransformEligibleOperationsRecord {
  ShopCartTransformEligibleOperationsRecord(
    expand_operation: json_bool(value, "expandOperation", False),
    merge_operation: json_bool(value, "mergeOperation", False),
    update_operation: json_bool(value, "updateOperation", False),
  )
}

fn payment_settings_from_json(
  value: commit.JsonValue,
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(
    supported_digital_wallets: json_string_list(
      value,
      "supportedDigitalWallets",
    ),
    payment_gateways: [],
  )
}

fn shop_policy_from_json(
  value: commit.JsonValue,
) -> Result(ShopPolicyRecord, Nil) {
  case value {
    commit.JsonObject(_) ->
      Ok(ShopPolicyRecord(
        id: json_string(value, "id", ""),
        title: json_string(value, "title", ""),
        body: json_string(value, "body", ""),
        type_: json_string(value, "type", ""),
        url: json_string(value, "url", ""),
        created_at: json_string(value, "createdAt", ""),
        updated_at: json_string(value, "updatedAt", ""),
        migrated_to_html: json_bool(value, "migratedToHtml", True),
      ))
    _ -> Error(Nil)
  }
}

fn json_object(value: commit.JsonValue, key: String) -> commit.JsonValue {
  case json_get(value, key) {
    Some(child) -> child
    None -> commit.JsonObject([])
  }
}

fn json_array(value: commit.JsonValue, key: String) -> List(commit.JsonValue) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_string(
  value: commit.JsonValue,
  key: String,
  default: String,
) -> String {
  json_string_option(value, key) |> option.unwrap(default)
}

fn json_string_option(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

fn json_string_list(value: commit.JsonValue, key: String) -> List(String) {
  json_array(value, key)
  |> list.filter_map(fn(item) {
    case item {
      commit.JsonString(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

fn json_bool(value: commit.JsonValue, key: String, default: Bool) -> Bool {
  case json_get(value, key) {
    Some(commit.JsonBool(value)) -> value
    _ -> default
  }
}

fn json_int(value: commit.JsonValue, key: String, default: Int) -> Int {
  case json_get(value, key) {
    Some(commit.JsonInt(value)) -> value
    _ -> default
  }
}

fn json_float_option(value: commit.JsonValue, key: String) -> Option(Float) {
  case json_get(value, key) {
    Some(commit.JsonFloat(value)) -> Some(value)
    Some(commit.JsonInt(value)) -> Some(int.to_float(value))
    _ -> None
  }
}

fn read_string_argument(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> read_arg_string(args, name)
    Error(_) -> None
  }
}

fn find_policy_by_id(
  policies: List(ShopPolicyRecord),
  id: String,
) -> Option(ShopPolicyRecord) {
  case policies {
    [] -> None
    [policy, ..rest] ->
      case policy.id == id {
        True -> Some(policy)
        False -> find_policy_by_id(rest, id)
      }
  }
}

fn render_shop_policy_body(policy: ShopPolicyRecord) -> String {
  case policy.migrated_to_html {
    True -> policy.body
    False -> simple_format(policy.body)
  }
}

fn simple_format(body: String) -> String {
  let normalized =
    body
    |> string.replace("\r\n", "\n")
    |> string.replace("\r", "\n")
  let paragraphs =
    normalized
    |> string.split(on: "\n\n")
    |> list.map(simple_format_paragraph)
  string.join(paragraphs, "\n\n")
}

fn simple_format_paragraph(paragraph: String) -> String {
  "<p>" <> string.replace(paragraph, "\n", "<br />\n") <> "</p>"
}

fn optional_float_source(value: Option(Float)) -> SourceValue {
  case value {
    Some(f) -> SrcFloat(f)
    None -> SrcNull
  }
}

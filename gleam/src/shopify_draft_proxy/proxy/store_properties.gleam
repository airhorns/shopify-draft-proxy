//// Store Properties domain port.
////
//// Mirrors the shop/read and shopPolicyUpdate local-staging slice from
//// `src/proxy/store-properties.ts`. Other Store Properties roots remain
//// on the TypeScript side until their state slices are ported.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type PaymentSettingsRecord, type ShopAddressRecord,
  type ShopBundlesFeatureRecord, type ShopCartTransformEligibleOperationsRecord,
  type ShopCartTransformFeatureRecord, type ShopDomainRecord,
  type ShopFeaturesRecord, type ShopPlanRecord, type ShopPolicyRecord,
  type ShopRecord, type ShopResourceLimitsRecord, type StorePropertyRecord,
  type StorePropertyValue, ShopPolicyRecord, ShopRecord, StorePropertyBool,
  StorePropertyFloat, StorePropertyInt, StorePropertyList, StorePropertyNull,
  StorePropertyObject, StorePropertyRecord, StorePropertyString,
}

const shop_policy_body_limit_chars = 524_288

const shop_policy_type_order = [
  "CONTACT_INFORMATION",
  "LEGAL_NOTICE",
  "PRIVACY_POLICY",
  "REFUND_POLICY",
  "SHIPPING_POLICY",
  "SUBSCRIPTION_POLICY",
  "TERMS_OF_SALE",
  "TERMS_OF_SERVICE",
]

pub type StorePropertiesError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type ShopPolicyUserError {
  ShopPolicyUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

type PolicyValidation {
  PolicyValidation(
    type_: Option(String),
    body: Option(String),
    user_errors: List(ShopPolicyUserError),
  )
}

type StagePolicyResult {
  StagePolicyResult(
    shop_policy: Option(ShopPolicyRecord),
    user_errors: List(ShopPolicyUserError),
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type GenericMutationResult {
  GenericMutationResult(
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    should_log: Bool,
  )
}

type QueryFieldResult {
  QueryFieldResult(key: String, value: Json, errors: List(Json))
}

pub fn is_store_properties_query_root(name: String) -> Bool {
  case name {
    "shop"
    | "location"
    | "locations"
    | "locationByIdentifier"
    | "businessEntities"
    | "businessEntity"
    | "collection" -> True
    _ -> False
  }
}

pub fn is_store_properties_mutation_root(name: String) -> Bool {
  case name {
    "shopPolicyUpdate"
    | "locationAdd"
    | "locationEdit"
    | "locationActivate"
    | "locationDeactivate"
    | "locationDelete"
    | "publishablePublish"
    | "publishablePublishToCurrentChannel"
    | "publishableUnpublish"
    | "publishableUnpublishToCurrentChannel" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, StorePropertiesError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let results =
    list.map(fields, fn(field) {
      root_query_result(store, field, fragments, variables)
    })
  let data_entries =
    list.map(results, fn(result) { #(result.key, result.value) })
  let errors = list.flat_map(results, fn(result) { result.errors })
  let entries = case errors {
    [] -> [#("data", json.object(data_entries))]
    _ -> [
      #("errors", json.array(errors, fn(error) { error })),
      #("data", json.object(data_entries)),
    ]
  }
  Ok(json.object(entries))
}

fn root_query_result(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, ..) ->
      case name.value {
        "shop" ->
          QueryFieldResult(
            key: key,
            value: serialize_shop_root(store, field, fragments),
            errors: [],
          )
        "location" ->
          QueryFieldResult(
            key: key,
            value: serialize_location_root(store, field, fragments, variables),
            errors: [],
          )
        "locations" ->
          QueryFieldResult(
            key: key,
            value: serialize_locations_root(store, field, fragments, variables),
            errors: [],
          )
        "locationByIdentifier" ->
          serialize_location_by_identifier_result(
            store,
            field,
            key,
            fragments,
            variables,
          )
        "businessEntities" ->
          QueryFieldResult(
            key: key,
            value: serialize_business_entities_root(store, field, fragments),
            errors: [],
          )
        "businessEntity" ->
          QueryFieldResult(
            key: key,
            value: serialize_business_entity_root(
              store,
              field,
              fragments,
              variables,
            ),
            errors: [],
          )
        "collection" ->
          QueryFieldResult(
            key: key,
            value: serialize_publishable_root(
              store,
              field,
              fragments,
              variables,
            ),
            errors: [],
          )
        _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
      }
    _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, StorePropertiesError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, top_errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        data_entries,
        current_store,
        current_identity,
        current_ids,
        current_errors,
      ) = acc
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) ->
          case name.value {
            "shopPolicyUpdate" -> {
              let result =
                stage_shop_policy_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let payload =
                shop_policy_update_payload_source(
                  result.shop_policy,
                  result.user_errors,
                )
              let projected =
                project_graphql_value(
                  payload,
                  selected_children(field),
                  fragments,
                )
              let #(logged_store, logged_identity) = case result.user_errors {
                [] ->
                  record_mutation_log(
                    result.store,
                    result.identity,
                    request_path,
                    document,
                    "shopPolicyUpdate",
                    result.staged_resource_ids,
                  )
                _ -> #(result.store, result.identity)
              }
              #(
                list.append(data_entries, [#(key, projected)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                current_errors,
              )
            }
            "locationAdd"
            | "locationEdit"
            | "locationActivate"
            | "locationDeactivate"
            | "locationDelete" -> {
              let result =
                stage_location_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let #(logged_store, logged_identity) = case result.should_log {
                True ->
                  record_mutation_log(
                    result.store,
                    result.identity,
                    request_path,
                    document,
                    name.value,
                    result.staged_resource_ids,
                  )
                False -> #(result.store, result.identity)
              }
              #(
                list.append(data_entries, [#(key, result.payload)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                list.append(current_errors, result.top_level_errors),
              )
            }
            "publishablePublish"
            | "publishablePublishToCurrentChannel"
            | "publishableUnpublish"
            | "publishableUnpublishToCurrentChannel" -> {
              let result =
                stage_publishable_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let #(logged_store, logged_identity) =
                record_mutation_log(
                  result.store,
                  result.identity,
                  request_path,
                  document,
                  name.value,
                  result.staged_resource_ids,
                )
              #(
                list.append(data_entries, [#(key, result.payload)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                current_errors,
              )
            }
            _ -> #(
              list.append(data_entries, [#(key, json.null())]),
              current_store,
              current_identity,
              current_ids,
              current_errors,
            )
          }
        _ -> #(
          list.append(data_entries, [#(key, json.null())]),
          current_store,
          current_identity,
          current_ids,
          current_errors,
        )
      }
    })
  let data = json.object(entries)
  let envelope = case top_errors {
    [] -> json.object([#("data", data)])
    _ ->
      json.object([
        #("errors", json.array(top_errors, fn(error) { error })),
        #("data", data),
      ])
  }
  Ok(MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
  ))
}

fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
}

fn selected_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn serialize_shop_root(
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

fn serialize_location_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let location = case read_string(args, "id") {
    Some(id) -> store.get_effective_store_property_location_by_id(store, id)
    None ->
      case store.list_effective_store_property_locations(store) {
        [first, ..] -> Some(first)
        [] -> None
      }
  }
  case location {
    Some(record) -> project_store_property_record(record, field, fragments)
    None -> json.null()
  }
}

fn serialize_location_by_identifier_result(
  store: Store,
  field: Selection,
  key: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = field_args(field, variables)
  case dict.get(args, "identifier") {
    Ok(root_field.ObjectVal(identifier)) ->
      case read_string(identifier, "id") {
        Some(id) ->
          QueryFieldResult(
            key: key,
            value: case
              store.get_effective_store_property_location_by_id(store, id)
            {
              Some(record) ->
                project_store_property_record(record, field, fragments)
              None -> json.null()
            },
            errors: [],
          )
        None ->
          QueryFieldResult(key: key, value: json.null(), errors: [
            custom_location_identifier_error(key),
          ])
      }
    _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn serialize_locations_root(
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
  serialize_store_record_connection(field, window, fragments)
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

fn serialize_store_record_connection(
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
      get_cursor_value: fn(record, _index) {
        record.cursor |> option.unwrap(record.id)
      },
      serialize_node: fn(record, selection, _index) {
        project_graphql_value(
          store_property_data_to_source(record.data),
          selected_selections(selection),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_business_entities_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  json.array(store.list_effective_business_entities(store), fn(record) {
    project_store_property_record(record, field, fragments)
  })
}

fn serialize_business_entity_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let entity = case read_string(args, "id") {
    Some(id) -> store.get_business_entity_by_id(store, id)
    None ->
      find_primary_business_entity(store.list_effective_business_entities(store))
  }
  case entity {
    Some(record) -> project_store_property_record(record, field, fragments)
    None -> json.null()
  }
}

fn serialize_publishable_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_string(args, "id") {
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

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
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
    #("address1", optional_string_source(address.address1)),
    #("address2", optional_string_source(address.address2)),
    #("city", optional_string_source(address.city)),
    #("company", optional_string_source(address.company)),
    #("coordinatesValidated", SrcBool(address.coordinates_validated)),
    #("country", optional_string_source(address.country)),
    #("countryCodeV2", optional_string_source(address.country_code_v2)),
    #("formatted", SrcList(list.map(address.formatted, SrcString))),
    #("formattedArea", optional_string_source(address.formatted_area)),
    #("latitude", optional_float_source(address.latitude)),
    #("longitude", optional_float_source(address.longitude)),
    #("phone", optional_string_source(address.phone)),
    #("province", optional_string_source(address.province)),
    #("provinceCode", optional_string_source(address.province_code)),
    #("zip", optional_string_source(address.zip)),
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
      optional_string_source(feature.ineligibility_reason),
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
    #("body", SrcString(policy.body)),
    #("type", SrcString(policy.type_)),
    #("url", SrcString(policy.url)),
    #("createdAt", SrcString(policy.created_at)),
    #("updatedAt", SrcString(policy.updated_at)),
    #("translations", SrcList([])),
  ])
}

fn shop_policy_update_payload_source(
  policy: Option(ShopPolicyRecord),
  errors: List(ShopPolicyUserError),
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

fn shop_policy_user_error_source(error: ShopPolicyUserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicyUserError")),
    #("field", case error.field {
      Some(parts) -> SrcList(list.map(parts, SrcString))
      None -> SrcNull
    }),
    #("message", SrcString(error.message)),
    #("code", optional_string_source(error.code)),
  ])
}

fn stage_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> StagePolicyResult {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_shop_policy_input(args)
  let validation = validate_shop_policy_input(input)
  case validation.user_errors, validation.type_, validation.body {
    [], Some(type_), Some(body) ->
      stage_valid_shop_policy_update(store, identity, type_, body)
    _, _, _ ->
      StagePolicyResult(
        shop_policy: None,
        user_errors: validation.user_errors,
        store: store,
        identity: identity,
        staged_resource_ids: [],
      )
  }
}

fn stage_location_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  case root_name {
    "locationAdd" ->
      stage_location_add(store, identity, field, fragments, variables)
    "locationEdit" ->
      stage_location_edit(store, identity, field, fragments, variables)
    "locationActivate" | "locationDeactivate" ->
      missing_idempotency_location_result(store, identity, root_name)
    "locationDelete" ->
      stage_location_delete(store, identity, field, fragments, variables)
    _ ->
      GenericMutationResult(
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
        should_log: False,
      )
  }
}

fn stage_location_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = field_args(field, variables)
  let input = read_object(args, "input")
  let name = input |> option.then(fn(values) { read_string(values, "name") })
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> location_add_blank_name_result(store, identity, field, fragments)
        _ -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(identity, "Location")
          let record =
            StorePropertyRecord(
              id: id,
              cursor: None,
              data: dict.from_list([
                #("__typename", StorePropertyString("Location")),
                #("id", StorePropertyString(id)),
                #("name", StorePropertyString(value)),
                #("isActive", StorePropertyBool(True)),
                #("activatable", StorePropertyBool(False)),
                #("deactivatable", StorePropertyBool(True)),
                #("deletable", StorePropertyBool(False)),
              ]),
            )
          let #(_, next_store) =
            store.upsert_staged_store_property_location(store, record)
          let payload_source =
            src_object([
              #("location", store_property_data_to_source(record.data)),
              #("userErrors", SrcList([])),
            ])
          GenericMutationResult(
            payload: project_graphql_value(
              payload_source,
              selected_children(field),
              fragments,
            ),
            store: next_store,
            identity: next_identity,
            staged_resource_ids: [id],
            top_level_errors: [],
            should_log: True,
          )
        }
      }
    _ -> {
      location_add_blank_name_result(store, identity, field, fragments)
    }
  }
}

fn location_add_blank_name_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> GenericMutationResult {
  let payload_source =
    src_object([
      #("location", SrcNull),
      #(
        "userErrors",
        SrcList([
          src_object([
            #("field", SrcList([SrcString("input"), SrcString("name")])),
            #("message", SrcString("Add a location name")),
          ]),
        ]),
      ),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn stage_location_edit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = field_args(field, variables)
  let id = read_string(args, "id")
  case id {
    Some(location_id) ->
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(record) -> {
          let input = read_object(args, "input")
          let next_data = case
            input |> option.then(fn(values) { read_string(values, "name") })
          {
            Some(name) ->
              dict.insert(record.data, "name", StorePropertyString(name))
            None -> record.data
          }
          let next_record = StorePropertyRecord(..record, data: next_data)
          let #(_, next_store) =
            store.upsert_staged_store_property_location(store, next_record)
          let payload_source =
            src_object([
              #("location", store_property_data_to_source(next_record.data)),
              #("userErrors", SrcList([])),
            ])
          GenericMutationResult(
            payload: project_graphql_value(
              payload_source,
              selected_children(field),
              fragments,
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: [location_id],
            top_level_errors: [],
            should_log: True,
          )
        }
        None ->
          location_user_error_result(
            store,
            identity,
            field,
            fragments,
            "location",
            "userErrors",
            "id",
            "Location not found.",
            None,
          )
      }
    None ->
      location_user_error_result(
        store,
        identity,
        field,
        fragments,
        "location",
        "userErrors",
        "id",
        "Location not found.",
        None,
      )
  }
}

fn stage_location_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = field_args(field, variables)
  let location_id = read_string(args, "locationId")
  case location_id {
    Some(id) ->
      case store.get_effective_store_property_location_by_id(store, id) {
        Some(record) ->
          case
            store_property_bool_field(record, "isActive") |> option.unwrap(True)
          {
            True ->
              active_location_delete_result(store, identity, field, fragments)
            False -> {
              let next_store =
                store.delete_staged_store_property_location(store, id)
              let payload_source =
                src_object([
                  #("deletedLocationId", SrcString(id)),
                  #("locationDeleteUserErrors", SrcList([])),
                ])
              GenericMutationResult(
                payload: project_graphql_value(
                  payload_source,
                  selected_children(field),
                  fragments,
                ),
                store: next_store,
                identity: identity,
                staged_resource_ids: [id],
                top_level_errors: [],
                should_log: True,
              )
            }
          }
        None ->
          location_user_error_result(
            store,
            identity,
            field,
            fragments,
            "deletedLocationId",
            "locationDeleteUserErrors",
            "locationId",
            "Location not found.",
            Some("LOCATION_NOT_FOUND"),
          )
      }
    None ->
      location_user_error_result(
        store,
        identity,
        field,
        fragments,
        "deletedLocationId",
        "locationDeleteUserErrors",
        "locationId",
        "Location not found.",
        Some("LOCATION_NOT_FOUND"),
      )
  }
}

fn missing_idempotency_location_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
) -> GenericMutationResult {
  GenericMutationResult(
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [missing_idempotency_error(root_name)],
    should_log: False,
  )
}

fn missing_idempotency_error(root_name: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The @idempotent directive is required for this mutation but was not provided.",
      ),
    ),
    #(
      "locations",
      json.array([#(3, 3)], fn(location) {
        json.object([
          #("line", json.int(location.0)),
          #("column", json.int(location.1)),
        ])
      }),
    ),
    #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
    #("path", json.array([root_name], json.string)),
  ])
}

fn active_location_delete_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> GenericMutationResult {
  let payload_source =
    src_object([
      #("deletedLocationId", SrcNull),
      #(
        "locationDeleteUserErrors",
        SrcList([
          src_object([
            #("field", SrcList([SrcString("locationId")])),
            #(
              "message",
              SrcString("The location cannot be deleted while it is active."),
            ),
            #("code", SrcString("LOCATION_IS_ACTIVE")),
          ]),
          src_object([
            #("field", SrcList([SrcString("locationId")])),
            #(
              "message",
              SrcString(
                "The location cannot be deleted while it has inventory.",
              ),
            ),
            #("code", SrcString("LOCATION_HAS_INVENTORY")),
          ]),
        ]),
      ),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn location_user_error_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  nullable_field: String,
  errors_field: String,
  error_field: String,
  message: String,
  code: Option(String),
) -> GenericMutationResult {
  let error_entries = case code {
    Some(value) -> [
      #("field", SrcList([SrcString(error_field)])),
      #("message", SrcString(message)),
      #("code", SrcString(value)),
    ]
    None -> [
      #("field", SrcList([SrcString(error_field)])),
      #("message", SrcString(message)),
    ]
  }
  let payload_source =
    src_object([
      #(nullable_field, SrcNull),
      #(errors_field, SrcList([src_object(error_entries)])),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn stage_publishable_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = field_args(field, variables)
  let id = read_string(args, "id") |> option.unwrap("")
  let key = root_name <> ":" <> id
  let payload_record = store.get_store_property_mutation_payload(store, key)
  let payload_data = case payload_record {
    Some(record) -> record.data
    None ->
      dict.from_list([
        #("publishable", StorePropertyNull),
        #(
          "userErrors",
          StorePropertyList([
            StorePropertyObject(
              dict.from_list([
                #("field", StorePropertyList([StorePropertyString("id")])),
                #("message", StorePropertyString("Publishable not found.")),
              ]),
            ),
          ]),
        ),
      ])
  }
  let next_store = case dict.get(payload_data, "publishable") {
    Ok(StorePropertyObject(data)) ->
      case dict.get(data, "id") {
        Ok(StorePropertyString(publishable_id)) -> {
          let #(_, staged) =
            store.upsert_staged_publishable(
              store,
              StorePropertyRecord(id: publishable_id, cursor: None, data: data),
            )
          staged
        }
        _ -> store
      }
    _ -> store
  }
  GenericMutationResult(
    payload: project_graphql_value(
      store_property_data_to_source(payload_data),
      selected_children(field),
      fragments,
    ),
    store: next_store,
    identity: identity,
    staged_resource_ids: case id {
      "" -> []
      _ -> [id]
    },
    top_level_errors: [],
    should_log: True,
  )
}

fn stage_valid_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  type_: String,
  body: String,
) -> StagePolicyResult {
  case store.get_effective_shop(store) {
    None ->
      StagePolicyResult(
        shop_policy: None,
        user_errors: [
          ShopPolicyUserError(
            field: Some(["shopPolicy"]),
            message: "Shop baseline is required to stage a shop policy update",
            code: None,
          ),
        ],
        store: store,
        identity: identity,
        staged_resource_ids: [],
      )
    Some(shop) -> {
      let existing = find_policy_by_type(shop.shop_policies, type_)
      let #(now, identity_after_time) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(id, identity_after_id) = case existing {
        Some(policy) -> #(policy.id, identity_after_time)
        None ->
          synthetic_identity.make_synthetic_gid(
            identity_after_time,
            "ShopPolicy",
          )
      }
      let policy =
        ShopPolicyRecord(
          id: id,
          title: case existing {
            Some(policy) -> policy.title
            None -> shop_policy_title(type_)
          },
          body: body,
          type_: type_,
          url: case existing {
            Some(policy) -> policy.url
            None -> build_shop_policy_url(shop, id, type_)
          },
          created_at: case existing {
            Some(policy) -> policy.created_at
            None -> now
          },
          updated_at: now,
        )
      let other_policies =
        list.filter(shop.shop_policies, fn(candidate) {
          candidate.type_ != policy.type_
        })
      let updated_shop =
        ShopRecord(
          ..shop,
          shop_policies: sort_shop_policies([policy, ..other_policies]),
        )
      let #(_, next_store) = store.stage_shop(store, updated_shop)
      StagePolicyResult(
        shop_policy: Some(policy),
        user_errors: [],
        store: next_store,
        identity: identity_after_id,
        staged_resource_ids: [policy.id],
      )
    }
  }
}

fn validate_shop_policy_input(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> PolicyValidation {
  let type_ = case input {
    Some(values) -> read_string(values, "type")
    None -> None
  }
  let body = case input {
    Some(values) -> read_string(values, "body")
    None -> None
  }
  let type_errors = case type_ {
    Some(value) ->
      case list.contains(shop_policy_type_order, value) {
        True -> []
        False -> [invalid_type_error()]
      }
    None -> [invalid_type_error()]
  }
  let body_errors = case body {
    Some(value) ->
      case string.byte_size(value) > shop_policy_body_limit_chars {
        True -> [
          ShopPolicyUserError(
            field: Some(["shopPolicy", "body"]),
            message: "Body is too big (maximum is 512 KB)",
            code: Some("TOO_BIG"),
          ),
        ]
        False -> []
      }
    None -> [
      ShopPolicyUserError(
        field: Some(["shopPolicy", "body"]),
        message: "Body is required",
        code: None,
      ),
    ]
  }
  PolicyValidation(
    type_: type_,
    body: body,
    user_errors: list.append(type_errors, body_errors),
  )
}

fn invalid_type_error() -> ShopPolicyUserError {
  ShopPolicyUserError(
    field: Some(["shopPolicy", "type"]),
    message: "Type is invalid",
    code: None,
  )
}

fn read_string(
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(values, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object(
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(values, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
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

fn read_shop_policy_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "shopPolicy") {
    Ok(root_field.ObjectVal(values)) -> Some(values)
    _ ->
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(values)) -> Some(values)
        _ -> None
      }
  }
}

fn record_mutation_log(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  root_name: String,
  staged_ids: List(String),
) -> #(Store, SyntheticIdentityRegistry) {
  let #(log_id, identity_after_log_id) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(received_at, identity_final) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log_id)
  let entry =
    store.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: None,
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store.Staged,
      interpreted: store.InterpretedMetadata(
        operation_type: store.Mutation,
        operation_name: None,
        root_fields: [root_name],
        primary_root_field: Some(root_name),
        capability: store.Capability(
          operation_name: Some(root_name),
          domain: "store-properties",
          execution: "stage-locally",
        ),
      ),
      notes: Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
    )
  #(store.record_mutation_log_entry(store, entry), identity_final)
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

fn find_policy_by_type(
  policies: List(ShopPolicyRecord),
  type_: String,
) -> Option(ShopPolicyRecord) {
  case policies {
    [] -> None
    [policy, ..rest] ->
      case policy.type_ == type_ {
        True -> Some(policy)
        False -> find_policy_by_type(rest, type_)
      }
  }
}

fn sort_shop_policies(
  policies: List(ShopPolicyRecord),
) -> List(ShopPolicyRecord) {
  list.sort(policies, fn(left, right) {
    let by_index =
      int.compare(policy_type_index(left.type_), policy_type_index(right.type_))
    case by_index {
      order.Eq -> string.compare(left.type_, right.type_)
      _ -> by_index
    }
  })
}

fn policy_type_index(type_: String) -> Int {
  policy_type_index_loop(shop_policy_type_order, type_, 0)
}

fn policy_type_index_loop(
  types: List(String),
  type_: String,
  index: Int,
) -> Int {
  case types {
    [] -> 999
    [first, ..rest] ->
      case first == type_ {
        True -> index
        False -> policy_type_index_loop(rest, type_, index + 1)
      }
  }
}

fn shop_policy_title(type_: String) -> String {
  case type_ {
    "CONTACT_INFORMATION" -> "Contact"
    "LEGAL_NOTICE" -> "Legal notice"
    "PRIVACY_POLICY" -> "Privacy policy"
    "REFUND_POLICY" -> "Refund policy"
    "SHIPPING_POLICY" -> "Shipping policy"
    "SUBSCRIPTION_POLICY" -> "Cancellation policy"
    "TERMS_OF_SALE" -> "Terms of sale"
    "TERMS_OF_SERVICE" -> "Terms of service"
    _ -> type_
  }
}

fn build_shop_policy_url(
  shop: ShopRecord,
  policy_id: String,
  type_: String,
) -> String {
  case read_numeric_gid_tail(shop.id), read_numeric_gid_tail(policy_id) {
    Some(shop_tail), Some(policy_tail) ->
      "https://checkout.shopify.com/"
      <> shop_tail
      <> "/policies/"
      <> policy_tail
      <> ".html?locale=en"
    _, _ ->
      trim_trailing_slash(shop.url)
      <> "/policies/"
      <> string.replace(string.lowercase(type_), "_", "-")
  }
}

fn trim_trailing_slash(value: String) -> String {
  case string.ends_with(value, "/") {
    True -> string.drop_end(value, 1)
    False -> value
  }
}

fn read_numeric_gid_tail(id: String) -> Option(String) {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) -> {
      let tail = case string.split(tail_with_query, on: "?") {
        [head, ..] -> head
        [] -> tail_with_query
      }
      case int.parse(tail) {
        Ok(_) -> Some(tail)
        Error(_) -> None
      }
    }
    Error(_) -> None
  }
}

fn optional_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn optional_float_source(value: Option(Float)) -> SourceValue {
  case value {
    Some(f) -> SrcFloat(f)
    None -> SrcNull
  }
}

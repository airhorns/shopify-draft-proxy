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
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcFloat, SrcInt, SrcList,
  SrcNull, SrcString, default_selected_field_options, get_document_fragments,
  get_field_response_key, get_selected_child_fields, project_graphql_value,
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
  type ShopRecord, type ShopResourceLimitsRecord, ShopPolicyRecord, ShopRecord,
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

pub fn is_store_properties_query_root(name: String) -> Bool {
  name == "shop"
}

pub fn is_store_properties_mutation_root(name: String) -> Bool {
  name == "shopPolicyUpdate"
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
  let data_entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) ->
          case name.value {
            "shop" -> #(key, serialize_shop_root(store, field, fragments))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  let _ = variables
  Ok(json.object([#("data", json.object(data_entries))]))
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
  let initial = #([], store, identity, [])
  let #(entries, final_store, final_identity, staged_ids) =
    list.fold(fields, initial, fn(acc, field) {
      let #(data_entries, current_store, current_identity, current_ids) = acc
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
                    result.staged_resource_ids,
                  )
                _ -> #(result.store, result.identity)
              }
              #(
                list.append(data_entries, [#(key, projected)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
              )
            }
            _ -> #(
              list.append(data_entries, [#(key, json.null())]),
              current_store,
              current_identity,
              current_ids,
            )
          }
        _ -> #(
          list.append(data_entries, [#(key, json.null())]),
          current_store,
          current_identity,
          current_ids,
        )
      }
    })
  Ok(MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
  ))
}

fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
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
        root_fields: ["shopPolicyUpdate"],
        primary_root_field: Some("shopPolicyUpdate"),
        capability: store.Capability(
          operation_name: Some("shopPolicyUpdate"),
          domain: "store-properties",
          execution: "stage-locally",
        ),
      ),
      notes: Some("Locally staged shopPolicyUpdate in shopify-draft-proxy."),
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

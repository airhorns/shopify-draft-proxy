import gleam/dict
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/state/serialization/shared.{
  dict_field, dict_to_json, optional_field, optional_string_field,
  string_list_field,
}
import shopify_draft_proxy/state/types

@internal
pub fn runtime_json_decoder() -> Decoder(Json) {
  decode.dynamic |> decode.map(runtime_json_from_dynamic)
}

@internal
pub fn captured_json_value_decoder() -> Decoder(types.CapturedJsonValue) {
  decode.dynamic |> decode.map(captured_json_value_from_dynamic)
}

@internal
pub fn captured_json_value_from_dynamic(
  value: Dynamic,
) -> types.CapturedJsonValue {
  case decode.run(value, decode.bool) {
    Ok(b) -> types.CapturedBool(b)
    Error(_) -> captured_json_value_from_non_bool_dynamic(value)
  }
}

@internal
pub fn captured_json_value_from_non_bool_dynamic(
  value: Dynamic,
) -> types.CapturedJsonValue {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> types.CapturedNull
    _ -> captured_json_value_from_present_dynamic(value)
  }
}

@internal
pub fn captured_json_value_from_present_dynamic(
  value: Dynamic,
) -> types.CapturedJsonValue {
  case decode.run(value, decode.int) {
    Ok(i) -> types.CapturedInt(i)
    Error(_) ->
      case decode.run(value, decode.float) {
        Ok(f) -> types.CapturedFloat(f)
        Error(_) ->
          case decode.run(value, decode.string) {
            Ok(s) -> types.CapturedString(s)
            Error(_) ->
              case decode.run(value, decode.list(decode.dynamic)) {
                Ok(items) ->
                  types.CapturedArray(list.map(
                    items,
                    captured_json_value_from_dynamic,
                  ))
                Error(_) ->
                  case
                    decode.run(
                      value,
                      decode.dict(decode.string, decode.dynamic),
                    )
                  {
                    Ok(fields) ->
                      types.CapturedObject(
                        fields
                        |> dict.to_list()
                        |> list.map(fn(pair) {
                          #(pair.0, captured_json_value_from_dynamic(pair.1))
                        }),
                      )
                    Error(_) -> types.CapturedNull
                  }
              }
          }
      }
  }
}

@internal
pub fn runtime_json_from_dynamic(value: Dynamic) -> Json {
  case decode.run(value, decode.bool) {
    Ok(b) -> json.bool(b)
    Error(_) -> runtime_json_from_non_bool_dynamic(value)
  }
}

@internal
pub fn runtime_json_from_non_bool_dynamic(value: Dynamic) -> Json {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> json.null()
    _ -> runtime_json_from_present_dynamic(value)
  }
}

@internal
pub fn runtime_json_from_present_dynamic(value: Dynamic) -> Json {
  case decode.run(value, decode.int) {
    Ok(i) -> json.int(i)
    Error(_) ->
      case decode.run(value, decode.float) {
        Ok(f) -> json.float(f)
        Error(_) ->
          case decode.run(value, decode.string) {
            Ok(s) -> json.string(s)
            Error(_) ->
              case decode.run(value, decode.list(decode.dynamic)) {
                Ok(items) -> json.array(items, runtime_json_from_dynamic)
                Error(_) ->
                  case
                    decode.run(
                      value,
                      decode.dict(decode.string, decode.dynamic),
                    )
                  {
                    Ok(fields) ->
                      dict_to_json(fields, runtime_json_from_dynamic)
                    Error(_) -> json.null()
                  }
              }
          }
      }
  }
}

@internal
pub fn float_decoder() -> Decoder(Float) {
  decode.one_of(decode.float, or: [decode.int |> decode.map(int.to_float)])
}

@internal
pub fn abandoned_checkout_decoder() -> Decoder(types.AbandonedCheckoutRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.AbandonedCheckoutRecord(
    id: id,
    cursor: cursor,
    data: data,
  ))
}

@internal
pub fn abandonment_delivery_activity_decoder() -> Decoder(
  types.AbandonmentDeliveryActivityRecord,
) {
  use marketing_activity_id <- decode.field(
    "marketingActivityId",
    decode.string,
  )
  use delivery_status <- decode.field("deliveryStatus", decode.string)
  use delivered_at <- optional_string_field("deliveredAt")
  use delivery_status_change_reason <- optional_string_field(
    "deliveryStatusChangeReason",
  )
  decode.success(types.AbandonmentDeliveryActivityRecord(
    marketing_activity_id: marketing_activity_id,
    delivery_status: delivery_status,
    delivered_at: delivered_at,
    delivery_status_change_reason: delivery_status_change_reason,
  ))
}

@internal
pub fn abandonment_decoder() -> Decoder(types.AbandonmentRecord) {
  use id <- decode.field("id", decode.string)
  use abandoned_checkout_id <- optional_string_field("abandonedCheckoutId")
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  use delivery_activities <- optional_field(
    "deliveryActivities",
    dict.new(),
    decode.dict(decode.string, abandonment_delivery_activity_decoder()),
  )
  decode.success(types.AbandonmentRecord(
    id: id,
    abandoned_checkout_id: abandoned_checkout_id,
    cursor: cursor,
    data: data,
    delivery_activities: delivery_activities,
  ))
}

@internal
pub fn draft_order_decoder() -> Decoder(types.DraftOrderRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.DraftOrderRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn order_decoder() -> Decoder(types.OrderRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.OrderRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn shipping_package_decoder() -> Decoder(types.ShippingPackageRecord) {
  use id <- decode.field("id", decode.string)
  use name <- optional_string_field("name")
  use type_ <- optional_string_field("type")
  use box_type <- optional_string_field("boxType")
  use default <- optional_field("default", False, decode.bool)
  use weight <- optional_field(
    "weight",
    None,
    decode.optional(shipping_package_weight_decoder()),
  )
  use dimensions <- optional_field(
    "dimensions",
    None,
    decode.optional(shipping_package_dimensions_decoder()),
  )
  use created_at <- optional_field("createdAt", "", decode.string)
  use updated_at <- optional_field("updatedAt", "", decode.string)
  decode.success(types.ShippingPackageRecord(
    id: id,
    name: name,
    type_: type_,
    box_type: box_type,
    default: default,
    weight: weight,
    dimensions: dimensions,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

@internal
pub fn shipping_package_weight_decoder() -> Decoder(
  types.ShippingPackageWeightRecord,
) {
  use value <- optional_field("value", None, decode.optional(float_decoder()))
  use unit <- optional_string_field("unit")
  decode.success(types.ShippingPackageWeightRecord(value: value, unit: unit))
}

@internal
pub fn shipping_package_dimensions_decoder() -> Decoder(
  types.ShippingPackageDimensionsRecord,
) {
  use length <- optional_field("length", None, decode.optional(float_decoder()))
  use width <- optional_field("width", None, decode.optional(float_decoder()))
  use height <- optional_field("height", None, decode.optional(float_decoder()))
  use unit <- optional_string_field("unit")
  decode.success(types.ShippingPackageDimensionsRecord(
    length: length,
    width: width,
    height: height,
    unit: unit,
  ))
}

@internal
pub fn draft_order_variant_catalog_decoder() -> Decoder(
  types.DraftOrderVariantCatalogRecord,
) {
  use variant_id <- decode.field("variantId", decode.string)
  use title <- decode.field("title", decode.string)
  use name <- decode.field("name", decode.string)
  use variant_title <- optional_string_field("variantTitle")
  use sku <- optional_string_field("sku")
  use requires_shipping <- decode.field("requiresShipping", decode.bool)
  use taxable <- decode.field("taxable", decode.bool)
  use unit_price <- decode.field("unitPrice", decode.string)
  use currency_code <- decode.field("currencyCode", decode.string)
  decode.success(types.DraftOrderVariantCatalogRecord(
    variant_id: variant_id,
    title: title,
    name: name,
    variant_title: variant_title,
    sku: sku,
    requires_shipping: requires_shipping,
    taxable: taxable,
    unit_price: unit_price,
    currency_code: currency_code,
  ))
}

@internal
pub fn backup_region_decoder() -> Decoder(types.BackupRegionRecord) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use code <- decode.field("code", decode.string)
  decode.success(types.BackupRegionRecord(id: id, name: name, code: code))
}

@internal
pub fn flow_signature_decoder() -> Decoder(
  types.AdminPlatformFlowSignatureRecord,
) {
  use id <- decode.field("id", decode.string)
  use flow_trigger_id <- decode.field("flowTriggerId", decode.string)
  use payload_sha256 <- decode.field("payloadSha256", decode.string)
  use signature_sha256 <- decode.field("signatureSha256", decode.string)
  use created_at <- decode.field("createdAt", decode.string)
  decode.success(types.AdminPlatformFlowSignatureRecord(
    id: id,
    flow_trigger_id: flow_trigger_id,
    payload_sha256: payload_sha256,
    signature_sha256: signature_sha256,
    created_at: created_at,
  ))
}

@internal
pub fn flow_trigger_decoder() -> Decoder(types.AdminPlatformFlowTriggerRecord) {
  use id <- decode.field("id", decode.string)
  use handle <- decode.field("handle", decode.string)
  use payload_bytes <- decode.field("payloadBytes", decode.int)
  use payload_sha256 <- decode.field("payloadSha256", decode.string)
  use received_at <- decode.field("receivedAt", decode.string)
  decode.success(types.AdminPlatformFlowTriggerRecord(
    id: id,
    handle: handle,
    payload_bytes: payload_bytes,
    payload_sha256: payload_sha256,
    received_at: received_at,
  ))
}

@internal
pub fn shop_decoder() -> Decoder(types.ShopRecord) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use myshopify_domain <- decode.field("myshopifyDomain", decode.string)
  use url <- decode.field("url", decode.string)
  use primary_domain <- decode.field("primaryDomain", shop_domain_decoder())
  use contact_email <- decode.field("contactEmail", decode.string)
  use email <- decode.field("email", decode.string)
  use currency_code <- decode.field("currencyCode", decode.string)
  use enabled_presentment_currencies <- decode.field(
    "enabledPresentmentCurrencies",
    decode.list(of: decode.string),
  )
  use iana_timezone <- decode.field("ianaTimezone", decode.string)
  use timezone_abbreviation <- decode.field(
    "timezoneAbbreviation",
    decode.string,
  )
  use timezone_offset <- decode.field("timezoneOffset", decode.string)
  use timezone_offset_minutes <- decode.field(
    "timezoneOffsetMinutes",
    decode.int,
  )
  use taxes_included <- decode.field("taxesIncluded", decode.bool)
  use tax_shipping <- decode.field("taxShipping", decode.bool)
  use unit_system <- decode.field("unitSystem", decode.string)
  use weight_unit <- decode.field("weightUnit", decode.string)
  use shop_address <- decode.field("shopAddress", shop_address_decoder())
  use plan <- decode.field("plan", shop_plan_decoder())
  use resource_limits <- decode.field(
    "resourceLimits",
    shop_resource_limits_decoder(),
  )
  use features <- decode.field("features", shop_features_decoder())
  use payment_settings <- decode.field(
    "paymentSettings",
    payment_settings_decoder(),
  )
  use shop_policies <- decode.field(
    "shopPolicies",
    decode.list(of: shop_policy_decoder()),
  )
  decode.success(types.ShopRecord(
    id: id,
    name: name,
    myshopify_domain: myshopify_domain,
    url: url,
    primary_domain: primary_domain,
    contact_email: contact_email,
    email: email,
    currency_code: currency_code,
    enabled_presentment_currencies: enabled_presentment_currencies,
    iana_timezone: iana_timezone,
    timezone_abbreviation: timezone_abbreviation,
    timezone_offset: timezone_offset,
    timezone_offset_minutes: timezone_offset_minutes,
    taxes_included: taxes_included,
    tax_shipping: tax_shipping,
    unit_system: unit_system,
    weight_unit: weight_unit,
    shop_address: shop_address,
    plan: plan,
    resource_limits: resource_limits,
    features: features,
    payment_settings: payment_settings,
    shop_policies: shop_policies,
  ))
}

@internal
pub fn shop_domain_decoder() -> Decoder(types.ShopDomainRecord) {
  use id <- decode.field("id", decode.string)
  use host <- decode.field("host", decode.string)
  use url <- decode.field("url", decode.string)
  use ssl_enabled <- decode.field("sslEnabled", decode.bool)
  decode.success(types.ShopDomainRecord(
    id: id,
    host: host,
    url: url,
    ssl_enabled: ssl_enabled,
  ))
}

@internal
pub fn shop_address_decoder() -> Decoder(types.ShopAddressRecord) {
  use id <- decode.field("id", decode.string)
  use address1 <- optional_string_field("address1")
  use address2 <- optional_string_field("address2")
  use city <- optional_string_field("city")
  use company <- optional_string_field("company")
  use coordinates_validated <- decode.field("coordinatesValidated", decode.bool)
  use country <- optional_string_field("country")
  use country_code_v2 <- optional_string_field("countryCodeV2")
  use formatted <- string_list_field("formatted")
  use formatted_area <- optional_string_field("formattedArea")
  use latitude <- optional_field(
    "latitude",
    None,
    decode.optional(float_decoder()),
  )
  use longitude <- optional_field(
    "longitude",
    None,
    decode.optional(float_decoder()),
  )
  use phone <- optional_string_field("phone")
  use province <- optional_string_field("province")
  use province_code <- optional_string_field("provinceCode")
  use zip <- optional_string_field("zip")
  decode.success(types.ShopAddressRecord(
    id: id,
    address1: address1,
    address2: address2,
    city: city,
    company: company,
    coordinates_validated: coordinates_validated,
    country: country,
    country_code_v2: country_code_v2,
    formatted: formatted,
    formatted_area: formatted_area,
    latitude: latitude,
    longitude: longitude,
    phone: phone,
    province: province,
    province_code: province_code,
    zip: zip,
  ))
}

@internal
pub fn shop_plan_decoder() -> Decoder(types.ShopPlanRecord) {
  use partner_development <- decode.field("partnerDevelopment", decode.bool)
  use public_display_name <- decode.field("publicDisplayName", decode.string)
  use shopify_plus <- decode.field("shopifyPlus", decode.bool)
  decode.success(types.ShopPlanRecord(
    partner_development: partner_development,
    public_display_name: public_display_name,
    shopify_plus: shopify_plus,
  ))
}

@internal
pub fn shop_resource_limits_decoder() -> Decoder(types.ShopResourceLimitsRecord) {
  use location_limit <- decode.field("locationLimit", decode.int)
  use max_product_options <- decode.field("maxProductOptions", decode.int)
  use max_product_variants <- decode.field("maxProductVariants", decode.int)
  use redirect_limit_reached <- decode.field(
    "redirectLimitReached",
    decode.bool,
  )
  decode.success(types.ShopResourceLimitsRecord(
    location_limit: location_limit,
    max_product_options: max_product_options,
    max_product_variants: max_product_variants,
    redirect_limit_reached: redirect_limit_reached,
  ))
}

@internal
pub fn shop_features_decoder() -> Decoder(types.ShopFeaturesRecord) {
  use avalara_avatax <- decode.field("avalaraAvatax", decode.bool)
  use branding <- decode.field("branding", decode.string)
  use bundles <- decode.field("bundles", shop_bundles_feature_decoder())
  use captcha <- decode.field("captcha", decode.bool)
  use cart_transform <- decode.field(
    "cartTransform",
    shop_cart_transform_feature_decoder(),
  )
  use dynamic_remarketing <- decode.field("dynamicRemarketing", decode.bool)
  use eligible_for_subscription_migration <- decode.field(
    "eligibleForSubscriptionMigration",
    decode.bool,
  )
  use eligible_for_subscriptions <- decode.field(
    "eligibleForSubscriptions",
    decode.bool,
  )
  use gift_cards <- decode.field("giftCards", decode.bool)
  use harmonized_system_code <- decode.field(
    "harmonizedSystemCode",
    decode.bool,
  )
  use legacy_subscription_gateway_enabled <- decode.field(
    "legacySubscriptionGatewayEnabled",
    decode.bool,
  )
  use live_view <- decode.field("liveView", decode.bool)
  use paypal_express_subscription_gateway_status <- decode.field(
    "paypalExpressSubscriptionGatewayStatus",
    decode.string,
  )
  use reports <- decode.field("reports", decode.bool)
  use discounts_by_market_enabled <- optional_field(
    "discountsByMarketEnabled",
    False,
    decode.bool,
  )
  use markets_granted <- optional_field("marketsGranted", 50, decode.int)
  use sells_subscriptions <- decode.field("sellsSubscriptions", decode.bool)
  use show_metrics <- decode.field("showMetrics", decode.bool)
  use storefront <- decode.field("storefront", decode.bool)
  use unified_markets <- decode.field("unifiedMarkets", decode.bool)
  decode.success(types.ShopFeaturesRecord(
    avalara_avatax: avalara_avatax,
    branding: branding,
    bundles: bundles,
    captcha: captcha,
    cart_transform: cart_transform,
    dynamic_remarketing: dynamic_remarketing,
    eligible_for_subscription_migration: eligible_for_subscription_migration,
    eligible_for_subscriptions: eligible_for_subscriptions,
    gift_cards: gift_cards,
    harmonized_system_code: harmonized_system_code,
    legacy_subscription_gateway_enabled: legacy_subscription_gateway_enabled,
    live_view: live_view,
    paypal_express_subscription_gateway_status: paypal_express_subscription_gateway_status,
    reports: reports,
    discounts_by_market_enabled: discounts_by_market_enabled,
    markets_granted: markets_granted,
    sells_subscriptions: sells_subscriptions,
    show_metrics: show_metrics,
    storefront: storefront,
    unified_markets: unified_markets,
  ))
}

@internal
pub fn shop_bundles_feature_decoder() -> Decoder(types.ShopBundlesFeatureRecord) {
  use eligible_for_bundles <- decode.field("eligibleForBundles", decode.bool)
  use ineligibility_reason <- optional_string_field("ineligibilityReason")
  use sells_bundles <- decode.field("sellsBundles", decode.bool)
  decode.success(types.ShopBundlesFeatureRecord(
    eligible_for_bundles: eligible_for_bundles,
    ineligibility_reason: ineligibility_reason,
    sells_bundles: sells_bundles,
  ))
}

@internal
pub fn shop_cart_transform_feature_decoder() -> Decoder(
  types.ShopCartTransformFeatureRecord,
) {
  use eligible_operations <- decode.field(
    "eligibleOperations",
    shop_cart_transform_eligible_operations_decoder(),
  )
  decode.success(types.ShopCartTransformFeatureRecord(
    eligible_operations: eligible_operations,
  ))
}

@internal
pub fn shop_cart_transform_eligible_operations_decoder() -> Decoder(
  types.ShopCartTransformEligibleOperationsRecord,
) {
  use expand_operation <- decode.field("expandOperation", decode.bool)
  use merge_operation <- decode.field("mergeOperation", decode.bool)
  use update_operation <- decode.field("updateOperation", decode.bool)
  decode.success(types.ShopCartTransformEligibleOperationsRecord(
    expand_operation: expand_operation,
    merge_operation: merge_operation,
    update_operation: update_operation,
  ))
}

@internal
pub fn payment_settings_decoder() -> Decoder(types.PaymentSettingsRecord) {
  use supported_digital_wallets <- decode.field(
    "supportedDigitalWallets",
    decode.list(of: decode.string),
  )
  use payment_gateways <- optional_field(
    "paymentGateways",
    [],
    decode.list(of: payment_gateway_decoder()),
  )
  decode.success(types.PaymentSettingsRecord(
    supported_digital_wallets: supported_digital_wallets,
    payment_gateways: payment_gateways,
  ))
}

@internal
pub fn payment_gateway_decoder() -> Decoder(types.PaymentGatewayRecord) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use active <- optional_field("active", True, decode.bool)
  decode.success(types.PaymentGatewayRecord(id: id, name: name, active: active))
}

@internal
pub fn shop_policy_decoder() -> Decoder(types.ShopPolicyRecord) {
  use id <- decode.field("id", decode.string)
  use title <- decode.field("title", decode.string)
  use body <- decode.field("body", decode.string)
  use type_ <- decode.field("type", decode.string)
  use url <- decode.field("url", decode.string)
  use created_at <- decode.field("createdAt", decode.string)
  use updated_at <- decode.field("updatedAt", decode.string)
  use migrated_to_html <- optional_field("migratedToHtml", True, decode.bool)
  decode.success(types.ShopPolicyRecord(
    id: id,
    title: title,
    body: body,
    type_: type_,
    url: url,
    created_at: created_at,
    updated_at: updated_at,
    migrated_to_html: migrated_to_html,
  ))
}

@internal
pub fn market_decoder() -> Decoder(types.MarketRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.MarketRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn catalog_decoder() -> Decoder(types.CatalogRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.CatalogRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn price_list_decoder() -> Decoder(types.PriceListRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.PriceListRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn web_presence_decoder() -> Decoder(types.WebPresenceRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.WebPresenceRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn market_localization_decoder() -> Decoder(types.MarketLocalizationRecord) {
  use resource_id <- decode.field("resourceId", decode.string)
  use market_id <- decode.field("marketId", decode.string)
  use key <- decode.field("key", decode.string)
  use value <- decode.field("value", decode.string)
  use updated_at <- decode.field("updatedAt", decode.string)
  use outdated <- decode.field("outdated", decode.bool)
  decode.success(types.MarketLocalizationRecord(
    resource_id: resource_id,
    market_id: market_id,
    key: key,
    value: value,
    updated_at: updated_at,
    outdated: outdated,
  ))
}

@internal
pub fn store_property_record_decoder() -> Decoder(types.StorePropertyRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- dict_field("data", store_property_value_decoder())
  decode.success(types.StorePropertyRecord(id: id, cursor: cursor, data: data))
}

@internal
pub fn store_property_mutation_payload_decoder() -> Decoder(
  types.StorePropertyMutationPayloadRecord,
) {
  use key <- decode.field("key", decode.string)
  use data <- dict_field("data", store_property_value_decoder())
  decode.success(types.StorePropertyMutationPayloadRecord(key: key, data: data))
}

@internal
pub fn store_property_value_decoder() -> Decoder(types.StorePropertyValue) {
  decode.recursive(fn() {
    decode.one_of(decode.bool |> decode.map(types.StorePropertyBool), or: [
      decode.int |> decode.map(types.StorePropertyInt),
      decode.float |> decode.map(types.StorePropertyFloat),
      decode.string |> decode.map(types.StorePropertyString),
      decode.list(of: store_property_value_decoder())
        |> decode.map(types.StorePropertyList),
      decode.dict(decode.string, store_property_value_decoder())
        |> decode.map(types.StorePropertyObject),
      decode.success(types.StorePropertyNull),
    ])
  })
}

@internal
pub fn saved_search_decoder() -> Decoder(types.SavedSearchRecord) {
  use id <- decode.field("id", decode.string)
  use legacy_resource_id <- decode.field("legacyResourceId", decode.string)
  use name <- decode.field("name", decode.string)
  use query <- decode.field("query", decode.string)
  use resource_type <- decode.field("resourceType", decode.string)
  use search_terms <- decode.field("searchTerms", decode.string)
  use filters <- decode.field(
    "filters",
    decode.list(of: saved_search_filter_decoder()),
  )
  use cursor <- optional_string_field("cursor")
  decode.success(types.SavedSearchRecord(
    id: id,
    legacy_resource_id: legacy_resource_id,
    name: name,
    query: query,
    resource_type: resource_type,
    search_terms: search_terms,
    filters: filters,
    cursor: cursor,
  ))
}

@internal
pub fn saved_search_filter_decoder() -> Decoder(types.SavedSearchFilter) {
  use key <- decode.field("key", decode.string)
  use value <- decode.field("value", decode.string)
  decode.success(types.SavedSearchFilter(key: key, value: value))
}

@internal
pub fn webhook_subscription_decoder() -> Decoder(
  types.WebhookSubscriptionRecord,
) {
  use id <- decode.field("id", decode.string)
  use topic <- optional_string_field("topic")
  use uri <- optional_string_field("uri")
  use name <- optional_string_field("name")
  use format <- optional_string_field("format")
  use include_fields <- string_list_field("includeFields")
  use metafield_namespaces <- string_list_field("metafieldNamespaces")
  use filter <- optional_string_field("filter")
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  use endpoint <- optional_field(
    "endpoint",
    None,
    decode.optional(webhook_endpoint_decoder()),
  )
  decode.success(types.WebhookSubscriptionRecord(
    id: id,
    topic: topic,
    uri: uri,
    name: name,
    format: format,
    include_fields: include_fields,
    metafield_namespaces: metafield_namespaces,
    filter: filter,
    created_at: created_at,
    updated_at: updated_at,
    endpoint: endpoint,
  ))
}

@internal
pub fn webhook_endpoint_decoder() -> Decoder(types.WebhookSubscriptionEndpoint) {
  use typename <- decode.field("__typename", decode.string)
  case typename {
    "WebhookEventBridgeEndpoint" -> {
      use arn <- optional_string_field("arn")
      decode.success(types.WebhookEventBridgeEndpoint(arn: arn))
    }
    "WebhookPubSubEndpoint" -> {
      use pub_sub_project <- optional_string_field("pubSubProject")
      use pub_sub_topic <- optional_string_field("pubSubTopic")
      decode.success(types.WebhookPubSubEndpoint(
        pub_sub_project: pub_sub_project,
        pub_sub_topic: pub_sub_topic,
      ))
    }
    _ -> {
      use callback_url <- optional_string_field("callbackUrl")
      decode.success(types.WebhookHttpEndpoint(callback_url: callback_url))
    }
  }
}

@internal
pub fn money_decoder() -> Decoder(types.Money) {
  use amount <- decode.field("amount", decode.string)
  use currency_code <- decode.field("currencyCode", decode.string)
  decode.success(types.Money(amount: amount, currency_code: currency_code))
}

@internal
pub fn access_scope_decoder() -> Decoder(types.AccessScopeRecord) {
  use handle <- decode.field("handle", decode.string)
  use description <- optional_string_field("description")
  decode.success(types.AccessScopeRecord(
    handle: handle,
    description: description,
  ))
}

@internal
pub fn app_decoder() -> Decoder(types.AppRecord) {
  use id <- decode.field("id", decode.string)
  use api_key <- optional_string_field("apiKey")
  use handle <- optional_string_field("handle")
  use title <- optional_string_field("title")
  use developer_name <- optional_string_field("developerName")
  use embedded <- optional_field("embedded", None, decode.optional(decode.bool))
  use previously_installed <- optional_field(
    "previouslyInstalled",
    None,
    decode.optional(decode.bool),
  )
  use requested_access_scopes <- optional_field(
    "requestedAccessScopes",
    [],
    decode.list(of: access_scope_decoder()),
  )
  decode.success(types.AppRecord(
    id: id,
    api_key: api_key,
    handle: handle,
    title: title,
    developer_name: developer_name,
    embedded: embedded,
    previously_installed: previously_installed,
    requested_access_scopes: requested_access_scopes,
  ))
}

@internal
pub fn app_subscription_pricing_decoder() -> Decoder(
  types.AppSubscriptionPricing,
) {
  use typename <- decode.field("__typename", decode.string)
  case typename {
    "AppUsagePricing" -> {
      use capped_amount <- decode.field("cappedAmount", money_decoder())
      use balance_used <- decode.field("balanceUsed", money_decoder())
      use interval <- decode.field("interval", decode.string)
      use terms <- optional_string_field("terms")
      decode.success(types.AppUsagePricing(
        capped_amount: capped_amount,
        balance_used: balance_used,
        interval: interval,
        terms: terms,
      ))
    }
    _ -> {
      use price <- decode.field("price", money_decoder())
      use interval <- decode.field("interval", decode.string)
      use plan_handle <- optional_string_field("planHandle")
      decode.success(types.AppRecurringPricing(
        price: price,
        interval: interval,
        plan_handle: plan_handle,
      ))
    }
  }
}

@internal
pub fn app_subscription_line_item_plan_decoder() -> Decoder(
  types.AppSubscriptionLineItemPlan,
) {
  use pricing_details <- decode.field(
    "pricingDetails",
    app_subscription_pricing_decoder(),
  )
  decode.success(types.AppSubscriptionLineItemPlan(
    pricing_details: pricing_details,
  ))
}

@internal
pub fn app_subscription_line_item_decoder() -> Decoder(
  types.AppSubscriptionLineItemRecord,
) {
  use id <- decode.field("id", decode.string)
  use subscription_id <- decode.field("subscriptionId", decode.string)
  use plan <- decode.field("plan", app_subscription_line_item_plan_decoder())
  decode.success(types.AppSubscriptionLineItemRecord(
    id: id,
    subscription_id: subscription_id,
    plan: plan,
  ))
}

@internal
pub fn app_subscription_decoder() -> Decoder(types.AppSubscriptionRecord) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use status <- decode.field("status", decode.string)
  use is_test <- decode.field("isTest", decode.bool)
  use trial_days <- optional_field(
    "trialDays",
    None,
    decode.optional(decode.int),
  )
  use current_period_end <- optional_string_field("currentPeriodEnd")
  use created_at <- decode.field("createdAt", decode.string)
  use line_item_ids <- string_list_field("lineItemIds")
  decode.success(types.AppSubscriptionRecord(
    id: id,
    name: name,
    status: status,
    is_test: is_test,
    trial_days: trial_days,
    current_period_end: current_period_end,
    created_at: created_at,
    line_item_ids: line_item_ids,
  ))
}

@internal
pub fn app_one_time_purchase_decoder() -> Decoder(
  types.AppOneTimePurchaseRecord,
) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use status <- decode.field("status", decode.string)
  use is_test <- decode.field("isTest", decode.bool)
  use created_at <- decode.field("createdAt", decode.string)
  use price <- decode.field("price", money_decoder())
  decode.success(types.AppOneTimePurchaseRecord(
    id: id,
    name: name,
    status: status,
    is_test: is_test,
    created_at: created_at,
    price: price,
  ))
}

@internal
pub fn app_usage_decoder() -> Decoder(types.AppUsageRecord) {
  use id <- decode.field("id", decode.string)
  use subscription_line_item_id <- decode.field(
    "subscriptionLineItemId",
    decode.string,
  )
  use description <- decode.field("description", decode.string)
  use price <- decode.field("price", money_decoder())
  use created_at <- decode.field("createdAt", decode.string)
  use idempotency_key <- optional_string_field("idempotencyKey")
  decode.success(types.AppUsageRecord(
    id: id,
    subscription_line_item_id: subscription_line_item_id,
    description: description,
    price: price,
    created_at: created_at,
    idempotency_key: idempotency_key,
  ))
}

@internal
pub fn delegated_access_token_decoder() -> Decoder(
  types.DelegatedAccessTokenRecord,
) {
  use id <- decode.field("id", decode.string)
  use api_client_id <- optional_field(
    "apiClientId",
    "shopify-draft-proxy-local-app",
    decode.string,
  )
  use parent_access_token_sha256 <- optional_string_field(
    "parentAccessTokenSha256",
  )
  use access_token_sha256 <- decode.field("accessTokenSha256", decode.string)
  use access_token_preview <- decode.field("accessTokenPreview", decode.string)
  use access_scopes <- string_list_field("accessScopes")
  use created_at <- decode.field("createdAt", decode.string)
  use expires_in <- optional_field(
    "expiresIn",
    None,
    decode.optional(decode.int),
  )
  use destroyed_at <- optional_string_field("destroyedAt")
  decode.success(types.DelegatedAccessTokenRecord(
    id: id,
    api_client_id: api_client_id,
    parent_access_token_sha256: parent_access_token_sha256,
    access_token_sha256: access_token_sha256,
    access_token_preview: access_token_preview,
    access_scopes: access_scopes,
    created_at: created_at,
    expires_in: expires_in,
    destroyed_at: destroyed_at,
  ))
}

@internal
pub fn app_installation_decoder() -> Decoder(types.AppInstallationRecord) {
  use id <- decode.field("id", decode.string)
  use app_id <- decode.field("appId", decode.string)
  use launch_url <- optional_string_field("launchUrl")
  use uninstall_url <- optional_string_field("uninstallUrl")
  use access_scopes <- optional_field(
    "accessScopes",
    [],
    decode.list(of: access_scope_decoder()),
  )
  use active_subscription_ids <- string_list_field("activeSubscriptionIds")
  use all_subscription_ids <- string_list_field("allSubscriptionIds")
  use one_time_purchase_ids <- string_list_field("oneTimePurchaseIds")
  use uninstalled_at <- optional_string_field("uninstalledAt")
  decode.success(types.AppInstallationRecord(
    id: id,
    app_id: app_id,
    launch_url: launch_url,
    uninstall_url: uninstall_url,
    access_scopes: access_scopes,
    active_subscription_ids: active_subscription_ids,
    all_subscription_ids: all_subscription_ids,
    one_time_purchase_ids: one_time_purchase_ids,
    uninstalled_at: uninstalled_at,
  ))
}

@internal
pub fn shopify_function_app_decoder() -> Decoder(types.ShopifyFunctionAppRecord) {
  use typename <- optional_string_field("__typename")
  use id <- optional_string_field("id")
  use title <- optional_string_field("title")
  use handle <- optional_string_field("handle")
  use api_key <- optional_string_field("apiKey")
  decode.success(types.ShopifyFunctionAppRecord(
    typename: typename,
    id: id,
    title: title,
    handle: handle,
    api_key: api_key,
  ))
}

@internal
pub fn shopify_function_decoder() -> Decoder(types.ShopifyFunctionRecord) {
  use id <- decode.field("id", decode.string)
  use title <- optional_string_field("title")
  use handle <- optional_string_field("handle")
  use api_type <- optional_string_field("apiType")
  use description <- optional_string_field("description")
  use app_key <- optional_string_field("appKey")
  use app <- optional_field(
    "app",
    None,
    decode.optional(shopify_function_app_decoder()),
  )
  decode.success(types.ShopifyFunctionRecord(
    id: id,
    title: title,
    handle: handle,
    api_type: api_type,
    description: description,
    app_key: app_key,
    app: app,
  ))
}

@internal
pub fn bulk_operation_decoder() -> Decoder(types.BulkOperationRecord) {
  use id <- decode.field("id", decode.string)
  use status <- decode.field("status", decode.string)
  use type_ <- decode.field("type", decode.string)
  use error_code <- optional_string_field("errorCode")
  use created_at <- decode.field("createdAt", decode.string)
  use completed_at <- optional_string_field("completedAt")
  use object_count <- decode.field("objectCount", decode.string)
  use root_object_count <- decode.field("rootObjectCount", decode.string)
  use file_size <- optional_string_field("fileSize")
  use url <- optional_string_field("url")
  use partial_data_url <- optional_string_field("partialDataUrl")
  use query <- optional_string_field("query")
  use cursor <- optional_string_field("cursor")
  use result_jsonl <- optional_string_field("resultJsonl")
  decode.success(types.BulkOperationRecord(
    id: id,
    status: status,
    type_: type_,
    error_code: error_code,
    created_at: created_at,
    completed_at: completed_at,
    object_count: object_count,
    root_object_count: root_object_count,
    file_size: file_size,
    url: url,
    partial_data_url: partial_data_url,
    query: query,
    cursor: cursor,
    result_jsonl: result_jsonl,
  ))
}

@internal
pub fn market_localizable_content_decoder() -> Decoder(
  types.MarketLocalizableContentRecord,
) {
  use key <- decode.field("key", decode.string)
  use value <- decode.field("value", decode.string)
  use digest <- decode.field("digest", decode.string)
  decode.success(types.MarketLocalizableContentRecord(
    key: key,
    value: value,
    digest: digest,
  ))
}

@internal
pub fn product_metafield_decoder() -> Decoder(types.ProductMetafieldRecord) {
  use id <- decode.field("id", decode.string)
  use owner_id <- decode.field("ownerId", decode.string)
  use namespace <- decode.field("namespace", decode.string)
  use key <- decode.field("key", decode.string)
  use type_ <- optional_string_field("type")
  use value <- optional_string_field("value")
  use compare_digest <- optional_string_field("compareDigest")
  use json_value <- optional_field(
    "jsonValue",
    None,
    decode.optional(runtime_json_decoder()),
  )
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  use owner_type <- optional_string_field("ownerType")
  use market_localizable_content <- optional_field(
    "marketLocalizableContent",
    [],
    decode.list(of: market_localizable_content_decoder()),
  )
  decode.success(types.ProductMetafieldRecord(
    id: id,
    owner_id: owner_id,
    namespace: namespace,
    key: key,
    type_: type_,
    value: value,
    compare_digest: compare_digest,
    json_value: json_value,
    created_at: created_at,
    updated_at: updated_at,
    owner_type: owner_type,
    market_localizable_content: market_localizable_content,
  ))
}

@internal
pub fn metafield_definition_decoder() -> Decoder(
  types.MetafieldDefinitionRecord,
) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use namespace <- decode.field("namespace", decode.string)
  use key <- decode.field("key", decode.string)
  use owner_type <- decode.field("ownerType", decode.string)
  use type_ <- decode.field("type", metafield_definition_type_decoder())
  use description <- optional_string_field("description")
  use validations <- decode.field(
    "validations",
    decode.list(of: metafield_definition_validation_decoder()),
  )
  use access <- optional_field(
    "access",
    dict.new(),
    decode.dict(decode.string, runtime_json_decoder()),
  )
  use capabilities <- decode.field(
    "capabilities",
    metafield_definition_capabilities_decoder(),
  )
  use constraints <- optional_field(
    "constraints",
    None,
    decode.optional(metafield_definition_constraints_decoder()),
  )
  use pinned_position <- optional_field(
    "pinnedPosition",
    None,
    decode.optional(decode.int),
  )
  use validation_status <- decode.field("validationStatus", decode.string)
  decode.success(types.MetafieldDefinitionRecord(
    id: id,
    name: name,
    namespace: namespace,
    key: key,
    owner_type: owner_type,
    type_: type_,
    description: description,
    validations: validations,
    access: access,
    capabilities: capabilities,
    constraints: constraints,
    pinned_position: pinned_position,
    validation_status: validation_status,
  ))
}

@internal
pub fn metafield_definition_type_decoder() -> Decoder(
  types.MetafieldDefinitionTypeRecord,
) {
  use name <- decode.field("name", decode.string)
  use category <- optional_string_field("category")
  decode.success(types.MetafieldDefinitionTypeRecord(
    name: name,
    category: category,
  ))
}

@internal
pub fn metafield_definition_validation_decoder() -> Decoder(
  types.MetafieldDefinitionValidationRecord,
) {
  use name <- decode.field("name", decode.string)
  use value <- optional_string_field("value")
  decode.success(types.MetafieldDefinitionValidationRecord(
    name: name,
    value: value,
  ))
}

@internal
pub fn metafield_definition_capabilities_decoder() -> Decoder(
  types.MetafieldDefinitionCapabilitiesRecord,
) {
  use admin_filterable <- decode.field(
    "adminFilterable",
    metafield_definition_capability_decoder(),
  )
  use smart_collection_condition <- decode.field(
    "smartCollectionCondition",
    metafield_definition_capability_decoder(),
  )
  use unique_values <- decode.field(
    "uniqueValues",
    metafield_definition_capability_decoder(),
  )
  decode.success(types.MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: admin_filterable,
    smart_collection_condition: smart_collection_condition,
    unique_values: unique_values,
  ))
}

@internal
pub fn metafield_definition_capability_decoder() -> Decoder(
  types.MetafieldDefinitionCapabilityRecord,
) {
  use enabled <- decode.field("enabled", decode.bool)
  use eligible <- decode.field("eligible", decode.bool)
  use status <- optional_string_field("status")
  decode.success(types.MetafieldDefinitionCapabilityRecord(
    enabled: enabled,
    eligible: eligible,
    status: status,
  ))
}

@internal
pub fn metafield_definition_constraints_decoder() -> Decoder(
  types.MetafieldDefinitionConstraintsRecord,
) {
  use key <- optional_string_field("key")
  use values <- decode.field(
    "values",
    decode.list(of: metafield_definition_constraint_value_decoder()),
  )
  decode.success(types.MetafieldDefinitionConstraintsRecord(
    key: key,
    values: values,
  ))
}

@internal
pub fn metafield_definition_constraint_value_decoder() -> Decoder(
  types.MetafieldDefinitionConstraintValueRecord,
) {
  use value <- decode.field("value", decode.string)
  decode.success(types.MetafieldDefinitionConstraintValueRecord(value: value))
}

@internal
pub fn metaobject_definition_decoder() -> Decoder(
  types.MetaobjectDefinitionRecord,
) {
  use id <- decode.field("id", decode.string)
  use type_ <- decode.field("type", decode.string)
  use name <- optional_string_field("name")
  use description <- optional_string_field("description")
  use display_name_key <- optional_string_field("displayNameKey")
  use access <- optional_field(
    "access",
    dict.new(),
    decode.dict(decode.string, decode.optional(decode.string)),
  )
  use capabilities <- decode.field(
    "capabilities",
    metaobject_definition_capabilities_decoder(),
  )
  use field_definitions <- decode.field(
    "fieldDefinitions",
    decode.list(of: metaobject_field_definition_decoder()),
  )
  use has_thumbnail_field <- optional_field(
    "hasThumbnailField",
    None,
    decode.optional(decode.bool),
  )
  use metaobjects_count <- optional_field(
    "metaobjectsCount",
    None,
    decode.optional(decode.int),
  )
  use standard_template <- optional_field(
    "standardTemplate",
    None,
    decode.optional(metaobject_standard_template_decoder()),
  )
  use standard_template_id <- optional_string_field("standardTemplateId")
  use standard_template_dependent_on_app <- optional_field(
    "standardTemplateDependentOnApp",
    False,
    decode.bool,
  )
  use app_config_managed <- optional_field(
    "appConfigManaged",
    False,
    decode.bool,
  )
  use enabled_by_shopify <- optional_field(
    "enabledByShopify",
    False,
    decode.bool,
  )
  use enabled_by_shopify_at <- optional_string_field("enabledByShopifyAt")
  use linked_metafields <- optional_field(
    "linkedMetafields",
    [],
    decode.list(of: metaobject_definition_linked_metafield_decoder()),
  )
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  decode.success(types.MetaobjectDefinitionRecord(
    id: id,
    type_: type_,
    name: name,
    description: description,
    display_name_key: display_name_key,
    access: access,
    capabilities: capabilities,
    field_definitions: field_definitions,
    has_thumbnail_field: has_thumbnail_field,
    metaobjects_count: metaobjects_count,
    standard_template: standard_template,
    standard_template_id: standard_template_id,
    standard_template_dependent_on_app: standard_template_dependent_on_app,
    app_config_managed: app_config_managed,
    enabled_by_shopify: enabled_by_shopify,
    enabled_by_shopify_at: enabled_by_shopify_at,
    linked_metafields: linked_metafields,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

@internal
pub fn metaobject_definition_linked_metafield_decoder() -> Decoder(
  types.MetaobjectDefinitionLinkedMetafieldRecord,
) {
  use owner_type <- decode.field("ownerType", decode.string)
  use namespace <- decode.field("namespace", decode.string)
  use key <- decode.field("key", decode.string)
  use metafield_definition_id <- optional_string_field("metafieldDefinitionId")
  use product_id <- decode.field("productId", decode.string)
  use product_option_id <- decode.field("productOptionId", decode.string)
  decode.success(types.MetaobjectDefinitionLinkedMetafieldRecord(
    owner_type: owner_type,
    namespace: namespace,
    key: key,
    metafield_definition_id: metafield_definition_id,
    product_id: product_id,
    product_option_id: product_option_id,
  ))
}

@internal
pub fn metaobject_definition_capabilities_decoder() -> Decoder(
  types.MetaobjectDefinitionCapabilitiesRecord,
) {
  use publishable <- optional_field(
    "publishable",
    None,
    decode.optional(metaobject_definition_capability_decoder()),
  )
  use translatable <- optional_field(
    "translatable",
    None,
    decode.optional(metaobject_definition_capability_decoder()),
  )
  use renderable <- optional_field(
    "renderable",
    None,
    decode.optional(metaobject_definition_capability_decoder()),
  )
  use online_store <- optional_field(
    "onlineStore",
    None,
    decode.optional(metaobject_definition_capability_decoder()),
  )
  decode.success(types.MetaobjectDefinitionCapabilitiesRecord(
    publishable: publishable,
    translatable: translatable,
    renderable: renderable,
    online_store: online_store,
  ))
}

@internal
pub fn metaobject_definition_capability_decoder() -> Decoder(
  types.MetaobjectDefinitionCapabilityRecord,
) {
  use enabled <- decode.field("enabled", decode.bool)
  decode.success(types.MetaobjectDefinitionCapabilityRecord(enabled: enabled))
}

@internal
pub fn metaobject_definition_type_decoder() -> Decoder(
  types.MetaobjectDefinitionTypeRecord,
) {
  use name <- decode.field("name", decode.string)
  use category <- optional_string_field("category")
  decode.success(types.MetaobjectDefinitionTypeRecord(
    name: name,
    category: category,
  ))
}

@internal
pub fn metaobject_field_definition_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionRecord,
) {
  use key <- decode.field("key", decode.string)
  use name <- optional_string_field("name")
  use description <- optional_string_field("description")
  use required <- optional_field("required", None, decode.optional(decode.bool))
  use type_ <- decode.field("type", metaobject_definition_type_decoder())
  use capabilities <- optional_field(
    "capabilities",
    default_metaobject_field_definition_capabilities(),
    metaobject_field_definition_capabilities_decoder(),
  )
  use validations <- decode.field(
    "validations",
    decode.list(of: metaobject_field_definition_validation_decoder()),
  )
  decode.success(types.MetaobjectFieldDefinitionRecord(
    key: key,
    name: name,
    description: description,
    required: required,
    type_: type_,
    capabilities: capabilities,
    validations: validations,
  ))
}

@internal
pub fn default_metaobject_field_definition_capabilities() -> types.MetaobjectFieldDefinitionCapabilitiesRecord {
  types.MetaobjectFieldDefinitionCapabilitiesRecord(
    admin_filterable: Some(types.MetaobjectDefinitionCapabilityRecord(False)),
  )
}

@internal
pub fn metaobject_field_definition_capabilities_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionCapabilitiesRecord,
) {
  use admin_filterable <- optional_field(
    "adminFilterable",
    None,
    decode.optional(metaobject_definition_capability_decoder()),
  )
  decode.success(types.MetaobjectFieldDefinitionCapabilitiesRecord(
    admin_filterable: admin_filterable,
  ))
}

@internal
pub fn metaobject_field_definition_validation_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionValidationRecord,
) {
  use name <- decode.field("name", decode.string)
  use value <- optional_string_field("value")
  decode.success(types.MetaobjectFieldDefinitionValidationRecord(
    name: name,
    value: value,
  ))
}

@internal
pub fn metaobject_standard_template_decoder() -> Decoder(
  types.MetaobjectStandardTemplateRecord,
) {
  use type_ <- optional_string_field("type")
  use name <- optional_string_field("name")
  use enabled_by_shopify <- optional_field(
    "enabledByShopify",
    False,
    decode.bool,
  )
  use enabled_by_shopify_at <- optional_string_field("enabledByShopifyAt")
  decode.success(types.MetaobjectStandardTemplateRecord(
    type_: type_,
    name: name,
    enabled_by_shopify: enabled_by_shopify,
    enabled_by_shopify_at: enabled_by_shopify_at,
  ))
}

@internal
pub fn metaobject_decoder() -> Decoder(types.MetaobjectRecord) {
  use id <- decode.field("id", decode.string)
  use handle <- decode.field("handle", decode.string)
  use type_ <- decode.field("type", decode.string)
  use display_name <- optional_string_field("displayName")
  use fields <- decode.field(
    "fields",
    decode.list(of: metaobject_field_decoder()),
  )
  use capabilities <- decode.field(
    "capabilities",
    metaobject_capabilities_decoder(),
  )
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  decode.success(types.MetaobjectRecord(
    id: id,
    handle: handle,
    type_: type_,
    display_name: display_name,
    fields: fields,
    capabilities: capabilities,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

@internal
pub fn metaobject_field_decoder() -> Decoder(types.MetaobjectFieldRecord) {
  use key <- decode.field("key", decode.string)
  use type_ <- optional_string_field("type")
  use value <- optional_string_field("value")
  use json_value <- decode.field("jsonValue", metaobject_json_value_decoder())
  use definition <- optional_field(
    "definition",
    None,
    decode.optional(metaobject_field_definition_ref_decoder()),
  )
  decode.success(types.MetaobjectFieldRecord(
    key: key,
    type_: type_,
    value: value,
    json_value: json_value,
    definition: definition,
  ))
}

@internal
pub fn metaobject_json_value_decoder() -> Decoder(types.MetaobjectJsonValue) {
  decode.recursive(fn() {
    decode.one_of(decode.bool |> decode.map(types.MetaobjectBool), or: [
      decode.int |> decode.map(types.MetaobjectInt),
      decode.float |> decode.map(types.MetaobjectFloat),
      decode.string |> decode.map(types.MetaobjectString),
      decode.list(of: metaobject_json_value_decoder())
        |> decode.map(types.MetaobjectList),
      decode.dict(decode.string, metaobject_json_value_decoder())
        |> decode.map(types.MetaobjectObject),
      decode.success(types.MetaobjectNull),
    ])
  })
}

@internal
pub fn metaobject_field_definition_ref_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionReferenceRecord,
) {
  use key <- decode.field("key", decode.string)
  use name <- optional_string_field("name")
  use required <- optional_field("required", None, decode.optional(decode.bool))
  use type_ <- decode.field("type", metaobject_definition_type_decoder())
  decode.success(types.MetaobjectFieldDefinitionReferenceRecord(
    key: key,
    name: name,
    required: required,
    type_: type_,
  ))
}

@internal
pub fn metaobject_capabilities_decoder() -> Decoder(
  types.MetaobjectCapabilitiesRecord,
) {
  use publishable <- optional_field(
    "publishable",
    None,
    decode.optional(metaobject_publishable_decoder()),
  )
  use online_store <- optional_field(
    "onlineStore",
    None,
    decode.optional(metaobject_online_store_decoder()),
  )
  decode.success(types.MetaobjectCapabilitiesRecord(
    publishable: publishable,
    online_store: online_store,
  ))
}

@internal
pub fn metaobject_publishable_decoder() -> Decoder(
  types.MetaobjectPublishableCapabilityRecord,
) {
  use status <- optional_string_field("status")
  decode.success(types.MetaobjectPublishableCapabilityRecord(status: status))
}

@internal
pub fn metaobject_online_store_decoder() -> Decoder(
  types.MetaobjectOnlineStoreCapabilityRecord,
) {
  use template_suffix <- optional_string_field("templateSuffix")
  decode.success(types.MetaobjectOnlineStoreCapabilityRecord(
    template_suffix: template_suffix,
  ))
}

@internal
pub fn marketing_record_decoder() -> Decoder(types.MarketingRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use api_client_id <- optional_string_field("apiClientId")
  use data <- optional_field(
    "data",
    dict.new(),
    decode.dict(decode.string, marketing_value_decoder()),
  )
  decode.success(types.MarketingRecord(
    id: id,
    cursor: cursor,
    api_client_id: api_client_id,
    data: data,
  ))
}

@internal
pub fn marketing_channel_definition_decoder() -> Decoder(
  types.MarketingChannelDefinitionRecord,
) {
  use handle <- decode.field("handle", decode.string)
  use api_client_ids <- optional_field(
    "apiClientIds",
    [],
    decode.list(of: decode.string),
  )
  decode.success(types.MarketingChannelDefinitionRecord(
    handle: handle,
    api_client_ids: api_client_ids,
  ))
}

@internal
pub fn marketing_engagement_decoder() -> Decoder(
  types.MarketingEngagementRecord,
) {
  use id <- decode.field("id", decode.string)
  use api_client_id <- optional_string_field("apiClientId")
  use marketing_activity_id <- optional_string_field("marketingActivityId")
  use remote_id <- optional_string_field("remoteId")
  use channel_handle <- optional_string_field("channelHandle")
  use occurred_on <- decode.field("occurredOn", decode.string)
  use data <- optional_field(
    "data",
    dict.new(),
    decode.dict(decode.string, marketing_value_decoder()),
  )
  decode.success(types.MarketingEngagementRecord(
    id: id,
    api_client_id: api_client_id,
    marketing_activity_id: marketing_activity_id,
    remote_id: remote_id,
    channel_handle: channel_handle,
    occurred_on: occurred_on,
    data: data,
  ))
}

@internal
pub fn marketing_value_decoder() -> Decoder(types.MarketingValue) {
  decode.recursive(fn() {
    decode.one_of(decode.bool |> decode.map(types.MarketingBool), or: [
      decode.int |> decode.map(types.MarketingInt),
      decode.float |> decode.map(types.MarketingFloat),
      decode.string |> decode.map(types.MarketingString),
      decode.list(of: marketing_value_decoder())
        |> decode.map(types.MarketingList),
      decode.dict(decode.string, marketing_value_decoder())
        |> decode.map(types.MarketingObject),
      decode.success(types.MarketingNull),
    ])
  })
}

@internal
pub fn validation_decoder() -> Decoder(types.ValidationRecord) {
  use id <- decode.field("id", decode.string)
  use title <- optional_string_field("title")
  use enable <- optional_field("enable", None, decode.optional(decode.bool))
  use block_on_failure <- optional_field(
    "blockOnFailure",
    None,
    decode.optional(decode.bool),
  )
  use function_id <- optional_string_field("functionId")
  use function_handle <- optional_string_field("functionHandle")
  use shopify_function_id <- optional_string_field("shopifyFunctionId")
  use metafields <- optional_field(
    "metafields",
    [],
    decode.list(of: validation_metafield_decoder()),
  )
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  decode.success(types.ValidationRecord(
    id: id,
    title: title,
    enable: enable,
    block_on_failure: block_on_failure,
    function_id: function_id,
    function_handle: function_handle,
    shopify_function_id: shopify_function_id,
    metafields: metafields,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

@internal
pub fn validation_metafield_decoder() -> Decoder(
  types.ValidationMetafieldRecord,
) {
  use id <- decode.field("id", decode.string)
  use validation_id <- decode.field("validationId", decode.string)
  use namespace <- decode.field("namespace", decode.string)
  use key <- decode.field("key", decode.string)
  use type_ <- optional_string_field("type")
  use value <- optional_string_field("value")
  use compare_digest <- optional_string_field("compareDigest")
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  use owner_type <- optional_string_field("ownerType")
  decode.success(types.ValidationMetafieldRecord(
    id: id,
    validation_id: validation_id,
    namespace: namespace,
    key: key,
    type_: type_,
    value: value,
    compare_digest: compare_digest,
    created_at: created_at,
    updated_at: updated_at,
    owner_type: owner_type,
  ))
}

@internal
pub fn cart_transform_decoder() -> Decoder(types.CartTransformRecord) {
  use id <- decode.field("id", decode.string)
  use title <- optional_string_field("title")
  use block_on_failure <- optional_field(
    "blockOnFailure",
    None,
    decode.optional(decode.bool),
  )
  use function_id <- optional_string_field("functionId")
  use function_handle <- optional_string_field("functionHandle")
  use shopify_function_id <- optional_string_field("shopifyFunctionId")
  use created_at <- optional_string_field("createdAt")
  use updated_at <- optional_string_field("updatedAt")
  decode.success(types.CartTransformRecord(
    id: id,
    title: title,
    block_on_failure: block_on_failure,
    function_id: function_id,
    function_handle: function_handle,
    shopify_function_id: shopify_function_id,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

@internal
pub fn tax_app_configuration_decoder() -> Decoder(
  types.TaxAppConfigurationRecord,
) {
  use id <- decode.field("id", decode.string)
  use ready <- decode.field("ready", decode.bool)
  use state <- decode.field("state", decode.string)
  use updated_at <- optional_string_field("updatedAt")
  decode.success(types.TaxAppConfigurationRecord(
    id: id,
    ready: ready,
    state: state,
    updated_at: updated_at,
  ))
}

@internal
pub fn gift_card_transaction_decoder() -> Decoder(
  types.GiftCardTransactionRecord,
) {
  use id <- decode.field("id", decode.string)
  use kind <- decode.field("kind", decode.string)
  use amount <- decode.field("amount", money_decoder())
  use processed_at <- decode.field("processedAt", decode.string)
  use note <- optional_string_field("note")
  decode.success(types.GiftCardTransactionRecord(
    id: id,
    kind: kind,
    amount: amount,
    processed_at: processed_at,
    note: note,
  ))
}

@internal
pub fn gift_card_recipient_attributes_decoder() -> Decoder(
  types.GiftCardRecipientAttributesRecord,
) {
  use id <- optional_string_field("id")
  use message <- optional_string_field("message")
  use preferred_name <- optional_string_field("preferredName")
  use send_notification_at <- optional_string_field("sendNotificationAt")
  decode.success(types.GiftCardRecipientAttributesRecord(
    id: id,
    message: message,
    preferred_name: preferred_name,
    send_notification_at: send_notification_at,
  ))
}

@internal
pub fn gift_card_decoder() -> Decoder(types.GiftCardRecord) {
  use id <- decode.field("id", decode.string)
  use legacy_resource_id <- decode.field("legacyResourceId", decode.string)
  use last_characters <- decode.field("lastCharacters", decode.string)
  use masked_code <- decode.field("maskedCode", decode.string)
  use code <- optional_string_field("code")
  use enabled <- decode.field("enabled", decode.bool)
  use notify <- optional_field("notify", True, decode.bool)
  use deactivated_at <- optional_string_field("deactivatedAt")
  use expires_on <- optional_string_field("expiresOn")
  use note <- optional_string_field("note")
  use template_suffix <- optional_string_field("templateSuffix")
  use created_at <- decode.field("createdAt", decode.string)
  use updated_at <- decode.field("updatedAt", decode.string)
  use initial_value <- decode.field("initialValue", money_decoder())
  use balance <- decode.field("balance", money_decoder())
  use customer_id <- optional_string_field("customerId")
  use recipient_id <- optional_string_field("recipientId")
  use source <- optional_string_field("source")
  use recipient_attributes <- optional_field(
    "recipientAttributes",
    None,
    decode.optional(gift_card_recipient_attributes_decoder()),
  )
  use transactions <- optional_field(
    "transactions",
    [],
    decode.list(of: gift_card_transaction_decoder()),
  )
  decode.success(types.GiftCardRecord(
    id: id,
    legacy_resource_id: legacy_resource_id,
    last_characters: last_characters,
    masked_code: masked_code,
    code: code,
    enabled: enabled,
    notify: notify,
    deactivated_at: deactivated_at,
    expires_on: expires_on,
    note: note,
    template_suffix: template_suffix,
    created_at: created_at,
    updated_at: updated_at,
    initial_value: initial_value,
    balance: balance,
    customer_id: customer_id,
    recipient_id: recipient_id,
    source: source,
    recipient_attributes: recipient_attributes,
    transactions: transactions,
  ))
}

@internal
pub fn gift_card_configuration_decoder() -> Decoder(
  types.GiftCardConfigurationRecord,
) {
  use issue_limit <- decode.field("issueLimit", money_decoder())
  use purchase_limit <- decode.field("purchaseLimit", money_decoder())
  decode.success(types.GiftCardConfigurationRecord(
    issue_limit: issue_limit,
    purchase_limit: purchase_limit,
  ))
}

@internal
pub fn segment_decoder() -> Decoder(types.SegmentRecord) {
  use id <- decode.field("id", decode.string)
  use name <- optional_string_field("name")
  use query <- optional_string_field("query")
  use creation_date <- optional_string_field("creationDate")
  use last_edit_date <- optional_string_field("lastEditDate")
  decode.success(types.SegmentRecord(
    id: id,
    name: name,
    query: query,
    creation_date: creation_date,
    last_edit_date: last_edit_date,
  ))
}

@internal
pub fn customer_segment_members_query_decoder() -> Decoder(
  types.CustomerSegmentMembersQueryRecord,
) {
  use id <- decode.field("id", decode.string)
  use query <- optional_string_field("query")
  use segment_id <- optional_string_field("segmentId")
  use status <- optional_field("status", "INITIALIZED", decode.string)
  use current_count <- decode.field("currentCount", decode.int)
  use done <- decode.field("done", decode.bool)
  decode.success(types.CustomerSegmentMembersQueryRecord(
    id: id,
    query: query,
    segment_id: segment_id,
    status: status,
    current_count: current_count,
    done: done,
  ))
}

@internal
pub fn locale_decoder() -> Decoder(types.LocaleRecord) {
  use iso_code <- decode.field("isoCode", decode.string)
  use name <- decode.field("name", decode.string)
  decode.success(types.LocaleRecord(iso_code: iso_code, name: name))
}

@internal
pub fn shop_locale_decoder() -> Decoder(types.ShopLocaleRecord) {
  use locale <- decode.field("locale", decode.string)
  use name <- decode.field("name", decode.string)
  use primary <- decode.field("primary", decode.bool)
  use published <- decode.field("published", decode.bool)
  use market_web_presence_ids <- string_list_field("marketWebPresenceIds")
  decode.success(types.ShopLocaleRecord(
    locale: locale,
    name: name,
    primary: primary,
    published: published,
    market_web_presence_ids: market_web_presence_ids,
  ))
}

@internal
pub fn translation_decoder() -> Decoder(types.TranslationRecord) {
  use resource_id <- decode.field("resourceId", decode.string)
  use key <- decode.field("key", decode.string)
  use locale <- decode.field("locale", decode.string)
  use value <- decode.field("value", decode.string)
  use translatable_content_digest <- decode.field(
    "translatableContentDigest",
    decode.string,
  )
  use market_id <- optional_string_field("marketId")
  use updated_at <- decode.field("updatedAt", decode.string)
  use outdated <- decode.field("outdated", decode.bool)
  decode.success(types.TranslationRecord(
    resource_id: resource_id,
    key: key,
    locale: locale,
    value: value,
    translatable_content_digest: translatable_content_digest,
    market_id: market_id,
    updated_at: updated_at,
    outdated: outdated,
  ))
}

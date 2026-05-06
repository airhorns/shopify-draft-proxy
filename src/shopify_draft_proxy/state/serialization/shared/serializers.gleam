import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/serialization/shared.{
  dict_to_json, optional_bool, optional_float, optional_int, optional_string,
  optional_to_json,
}
import shopify_draft_proxy/state/types

@internal
pub fn file_json(record: types.FileRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("alt", optional_string(record.alt)),
    #("contentType", optional_string(record.content_type)),
    #("createdAt", json.string(record.created_at)),
    #("fileStatus", json.string(record.file_status)),
    #("filename", optional_string(record.filename)),
    #("originalSource", json.string(record.original_source)),
    #("imageUrl", optional_string(record.image_url)),
    #("imageWidth", optional_int(record.image_width)),
    #("imageHeight", optional_int(record.image_height)),
    #(
      "updateFailureAcknowledgedAt",
      optional_string(record.update_failure_acknowledged_at),
    ),
  ])
}

@internal
pub fn backup_region_json(record: types.BackupRegionRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("code", json.string(record.code)),
  ])
}

@internal
pub fn admin_platform_generic_node_json(
  record: types.AdminPlatformGenericNodeRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("typename", json.string(record.typename)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn admin_platform_taxonomy_category_json(
  record: types.AdminPlatformTaxonomyCategoryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn flow_signature_json(
  record: types.AdminPlatformFlowSignatureRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("flowTriggerId", json.string(record.flow_trigger_id)),
    #("payloadSha256", json.string(record.payload_sha256)),
    #("signatureSha256", json.string(record.signature_sha256)),
    #("createdAt", json.string(record.created_at)),
  ])
}

@internal
pub fn flow_trigger_json(record: types.AdminPlatformFlowTriggerRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("handle", json.string(record.handle)),
    #("payloadBytes", json.int(record.payload_bytes)),
    #("payloadSha256", json.string(record.payload_sha256)),
    #("receivedAt", json.string(record.received_at)),
  ])
}

@internal
pub fn shop_json(record: types.ShopRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("myshopifyDomain", json.string(record.myshopify_domain)),
    #("url", json.string(record.url)),
    #("primaryDomain", shop_domain_json(record.primary_domain)),
    #("contactEmail", json.string(record.contact_email)),
    #("email", json.string(record.email)),
    #("currencyCode", json.string(record.currency_code)),
    #(
      "enabledPresentmentCurrencies",
      json.array(record.enabled_presentment_currencies, json.string),
    ),
    #("ianaTimezone", json.string(record.iana_timezone)),
    #("timezoneAbbreviation", json.string(record.timezone_abbreviation)),
    #("timezoneOffset", json.string(record.timezone_offset)),
    #("timezoneOffsetMinutes", json.int(record.timezone_offset_minutes)),
    #("taxesIncluded", json.bool(record.taxes_included)),
    #("taxShipping", json.bool(record.tax_shipping)),
    #("unitSystem", json.string(record.unit_system)),
    #("weightUnit", json.string(record.weight_unit)),
    #("shopAddress", shop_address_json(record.shop_address)),
    #("plan", shop_plan_json(record.plan)),
    #("resourceLimits", shop_resource_limits_json(record.resource_limits)),
    #("features", shop_features_json(record.features)),
    #("paymentSettings", payment_settings_json(record.payment_settings)),
    #("shopPolicies", json.array(record.shop_policies, shop_policy_json)),
  ])
}

@internal
pub fn shop_domain_json(record: types.ShopDomainRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("host", json.string(record.host)),
    #("url", json.string(record.url)),
    #("sslEnabled", json.bool(record.ssl_enabled)),
  ])
}

@internal
pub fn shop_address_json(record: types.ShopAddressRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("address1", optional_string(record.address1)),
    #("address2", optional_string(record.address2)),
    #("city", optional_string(record.city)),
    #("company", optional_string(record.company)),
    #("coordinatesValidated", json.bool(record.coordinates_validated)),
    #("country", optional_string(record.country)),
    #("countryCodeV2", optional_string(record.country_code_v2)),
    #("formatted", json.array(record.formatted, json.string)),
    #("formattedArea", optional_string(record.formatted_area)),
    #("latitude", optional_float(record.latitude)),
    #("longitude", optional_float(record.longitude)),
    #("phone", optional_string(record.phone)),
    #("province", optional_string(record.province)),
    #("provinceCode", optional_string(record.province_code)),
    #("zip", optional_string(record.zip)),
  ])
}

@internal
pub fn shop_plan_json(record: types.ShopPlanRecord) -> Json {
  json.object([
    #("partnerDevelopment", json.bool(record.partner_development)),
    #("publicDisplayName", json.string(record.public_display_name)),
    #("shopifyPlus", json.bool(record.shopify_plus)),
  ])
}

@internal
pub fn shop_resource_limits_json(
  record: types.ShopResourceLimitsRecord,
) -> Json {
  json.object([
    #("locationLimit", json.int(record.location_limit)),
    #("maxProductOptions", json.int(record.max_product_options)),
    #("maxProductVariants", json.int(record.max_product_variants)),
    #("redirectLimitReached", json.bool(record.redirect_limit_reached)),
  ])
}

@internal
pub fn shop_features_json(record: types.ShopFeaturesRecord) -> Json {
  json.object([
    #("avalaraAvatax", json.bool(record.avalara_avatax)),
    #("branding", json.string(record.branding)),
    #("bundles", shop_bundles_feature_json(record.bundles)),
    #("captcha", json.bool(record.captcha)),
    #("cartTransform", shop_cart_transform_feature_json(record.cart_transform)),
    #("dynamicRemarketing", json.bool(record.dynamic_remarketing)),
    #(
      "eligibleForSubscriptionMigration",
      json.bool(record.eligible_for_subscription_migration),
    ),
    #("eligibleForSubscriptions", json.bool(record.eligible_for_subscriptions)),
    #("giftCards", json.bool(record.gift_cards)),
    #("harmonizedSystemCode", json.bool(record.harmonized_system_code)),
    #(
      "legacySubscriptionGatewayEnabled",
      json.bool(record.legacy_subscription_gateway_enabled),
    ),
    #("liveView", json.bool(record.live_view)),
    #(
      "paypalExpressSubscriptionGatewayStatus",
      json.string(record.paypal_express_subscription_gateway_status),
    ),
    #("reports", json.bool(record.reports)),
    #("discountsByMarketEnabled", json.bool(record.discounts_by_market_enabled)),
    #("sellsSubscriptions", json.bool(record.sells_subscriptions)),
    #("showMetrics", json.bool(record.show_metrics)),
    #("storefront", json.bool(record.storefront)),
    #("unifiedMarkets", json.bool(record.unified_markets)),
  ])
}

@internal
pub fn shop_bundles_feature_json(
  record: types.ShopBundlesFeatureRecord,
) -> Json {
  json.object([
    #("eligibleForBundles", json.bool(record.eligible_for_bundles)),
    #("ineligibilityReason", optional_string(record.ineligibility_reason)),
    #("sellsBundles", json.bool(record.sells_bundles)),
  ])
}

@internal
pub fn shop_cart_transform_feature_json(
  record: types.ShopCartTransformFeatureRecord,
) -> Json {
  json.object([
    #(
      "eligibleOperations",
      shop_cart_transform_eligible_operations_json(record.eligible_operations),
    ),
  ])
}

@internal
pub fn shop_cart_transform_eligible_operations_json(
  record: types.ShopCartTransformEligibleOperationsRecord,
) -> Json {
  json.object([
    #("expandOperation", json.bool(record.expand_operation)),
    #("mergeOperation", json.bool(record.merge_operation)),
    #("updateOperation", json.bool(record.update_operation)),
  ])
}

@internal
pub fn payment_settings_json(record: types.PaymentSettingsRecord) -> Json {
  json.object([
    #(
      "supportedDigitalWallets",
      json.array(record.supported_digital_wallets, json.string),
    ),
    #(
      "paymentGateways",
      json.array(record.payment_gateways, payment_gateway_json),
    ),
  ])
}

@internal
pub fn payment_gateway_json(record: types.PaymentGatewayRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("active", json.bool(record.active)),
  ])
}

@internal
pub fn shop_policy_json(record: types.ShopPolicyRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", json.string(record.title)),
    #("body", json.string(record.body)),
    #("type", json.string(record.type_)),
    #("url", json.string(record.url)),
    #("createdAt", json.string(record.created_at)),
    #("updatedAt", json.string(record.updated_at)),
    #("migratedToHtml", json.bool(record.migrated_to_html)),
  ])
}

@internal
pub fn b2b_company_json(record: types.B2BCompanyRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", store_property_data_json(record.data)),
    #("mainContactId", optional_string(record.main_contact_id)),
    #("contactIds", json.array(record.contact_ids, json.string)),
    #("locationIds", json.array(record.location_ids, json.string)),
    #("contactRoleIds", json.array(record.contact_role_ids, json.string)),
  ])
}

@internal
pub fn b2b_company_contact_json(record: types.B2BCompanyContactRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

@internal
pub fn b2b_company_contact_role_json(
  record: types.B2BCompanyContactRoleRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

@internal
pub fn b2b_company_location_json(
  record: types.B2BCompanyLocationRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

@internal
pub fn store_property_record_json(record: types.StorePropertyRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", store_property_data_json(record.data)),
  ])
}

@internal
pub fn store_property_mutation_payload_json(
  record: types.StorePropertyMutationPayloadRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("data", store_property_data_json(record.data)),
  ])
}

@internal
pub fn store_property_data_json(
  data: Dict(String, types.StorePropertyValue),
) -> Json {
  dict_to_json(data, store_property_value_json)
}

@internal
pub fn store_property_value_json(value: types.StorePropertyValue) -> Json {
  case value {
    types.StorePropertyNull -> json.null()
    types.StorePropertyString(value) -> json.string(value)
    types.StorePropertyBool(value) -> json.bool(value)
    types.StorePropertyInt(value) -> json.int(value)
    types.StorePropertyFloat(value) -> json.float(value)
    types.StorePropertyList(items) ->
      json.array(items, store_property_value_json)
    types.StorePropertyObject(fields) -> store_property_data_json(fields)
  }
}

@internal
pub fn product_json(record: types.ProductRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("legacyResourceId", optional_string(record.legacy_resource_id)),
    #("title", json.string(record.title)),
    #("handle", json.string(record.handle)),
    #("status", json.string(record.status)),
    #("vendor", optional_string(record.vendor)),
    #("productType", optional_string(record.product_type)),
    #("tags", json.array(record.tags, json.string)),
    #("priceRangeMin", optional_string(record.price_range_min)),
    #("priceRangeMax", optional_string(record.price_range_max)),
    #("totalVariants", optional_int(record.total_variants)),
    #("hasOnlyDefaultVariant", optional_bool(record.has_only_default_variant)),
    #("hasOutOfStockVariants", optional_bool(record.has_out_of_stock_variants)),
    #("totalInventory", optional_int(record.total_inventory)),
    #("tracksInventory", optional_bool(record.tracks_inventory)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("publishedAt", optional_string(record.published_at)),
    #("descriptionHtml", json.string(record.description_html)),
    #("onlineStorePreviewUrl", optional_string(record.online_store_preview_url)),
    #("templateSuffix", optional_string(record.template_suffix)),
    #("seo", product_seo_json(record.seo)),
    #("category", optional_to_json(record.category, product_category_json)),
    #("publicationIds", json.array(record.publication_ids, json.string)),
    #(
      "contextualPricing",
      optional_to_json(record.contextual_pricing, captured_json_value_json),
    ),
    #("cursor", optional_string(record.cursor)),
    #("combinedListingRole", optional_string(record.combined_listing_role)),
    #(
      "combinedListingParentId",
      optional_string(record.combined_listing_parent_id),
    ),
    #(
      "combinedListingChildIds",
      json.array(record.combined_listing_child_ids, json.string),
    ),
  ])
}

@internal
pub fn product_variant_json(record: types.ProductVariantRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("productId", json.string(record.product_id)),
    #("title", json.string(record.title)),
    #("sku", optional_string(record.sku)),
    #("barcode", optional_string(record.barcode)),
    #("price", optional_string(record.price)),
    #("compareAtPrice", optional_string(record.compare_at_price)),
    #("taxable", optional_bool(record.taxable)),
    #("inventoryPolicy", optional_string(record.inventory_policy)),
    #("inventoryQuantity", optional_int(record.inventory_quantity)),
    #(
      "selectedOptions",
      json.array(record.selected_options, selected_option_json),
    ),
    #("mediaIds", json.array(record.media_ids, json.string)),
    #(
      "inventoryItemId",
      optional_to_json(record.inventory_item, fn(item) { json.string(item.id) }),
    ),
    #(
      "contextualPricing",
      optional_to_json(record.contextual_pricing, captured_json_value_json),
    ),
    #("cursor", optional_string(record.cursor)),
  ])
}

@internal
pub fn selling_plan_group_json(record: types.SellingPlanGroupRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("appId", optional_string(record.app_id)),
    #("name", json.string(record.name)),
    #("merchantCode", json.string(record.merchant_code)),
    #("description", optional_string(record.description)),
    #("options", json.array(record.options, json.string)),
    #("position", optional_int(record.position)),
    #("summary", optional_string(record.summary)),
    #("createdAt", optional_string(record.created_at)),
    #("productIds", json.array(record.product_ids, json.string)),
    #("productVariantIds", json.array(record.product_variant_ids, json.string)),
    #("sellingPlans", json.array(record.selling_plans, selling_plan_json)),
    #("cursor", optional_string(record.cursor)),
  ])
}

@internal
pub fn selling_plan_json(record: types.SellingPlanRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn delivery_profile_json(record: types.DeliveryProfileRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("merchantOwned", json.bool(record.merchant_owned)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn market_json(record: types.MarketRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn catalog_json(record: types.CatalogRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn price_list_json(record: types.PriceListRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn web_presence_json(record: types.WebPresenceRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn market_localization_json(
  record: types.MarketLocalizationRecord,
) -> Json {
  json.object([
    #("resourceId", json.string(record.resource_id)),
    #("marketId", json.string(record.market_id)),
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
    #("updatedAt", json.string(record.updated_at)),
    #("outdated", json.bool(record.outdated)),
  ])
}

@internal
pub fn market_localizable_content_json(
  record: types.MarketLocalizableContentRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
    #("digest", json.string(record.digest)),
  ])
}

@internal
pub fn selected_option_json(
  record: types.ProductVariantSelectedOptionRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", json.string(record.value)),
  ])
}

@internal
pub fn product_seo_json(record: types.ProductSeoRecord) -> Json {
  json.object([
    #("title", optional_string(record.title)),
    #("description", optional_string(record.description)),
  ])
}

@internal
pub fn product_category_json(record: types.ProductCategoryRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("fullName", json.string(record.full_name)),
  ])
}

@internal
pub fn abandoned_checkout_json(record: types.AbandonedCheckoutRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn abandonment_delivery_activity_json(
  record: types.AbandonmentDeliveryActivityRecord,
) -> Json {
  json.object([
    #("marketingActivityId", json.string(record.marketing_activity_id)),
    #("deliveryStatus", json.string(record.delivery_status)),
    #("deliveredAt", optional_string(record.delivered_at)),
    #(
      "deliveryStatusChangeReason",
      optional_string(record.delivery_status_change_reason),
    ),
  ])
}

@internal
pub fn abandonment_json(record: types.AbandonmentRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("abandonedCheckoutId", optional_string(record.abandoned_checkout_id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
    #(
      "deliveryActivities",
      dict_to_json(
        record.delivery_activities,
        abandonment_delivery_activity_json,
      ),
    ),
  ])
}

@internal
pub fn draft_order_json(record: types.DraftOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn order_json(record: types.OrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn draft_order_variant_catalog_json(
  record: types.DraftOrderVariantCatalogRecord,
) -> Json {
  json.object([
    #("variantId", json.string(record.variant_id)),
    #("title", json.string(record.title)),
    #("name", json.string(record.name)),
    #("variantTitle", optional_string(record.variant_title)),
    #("sku", optional_string(record.sku)),
    #("requiresShipping", json.bool(record.requires_shipping)),
    #("taxable", json.bool(record.taxable)),
    #("unitPrice", json.string(record.unit_price)),
    #("currencyCode", json.string(record.currency_code)),
  ])
}

@internal
pub fn captured_json_value_json(value: types.CapturedJsonValue) -> Json {
  case value {
    types.CapturedNull -> json.null()
    types.CapturedBool(value) -> json.bool(value)
    types.CapturedInt(value) -> json.int(value)
    types.CapturedFloat(value) -> json.float(value)
    types.CapturedString(value) -> json.string(value)
    types.CapturedArray(items) -> json.array(items, captured_json_value_json)
    types.CapturedObject(fields) ->
      json.object(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_value_json(item))
        }),
      )
  }
}

@internal
pub fn optional_string_value(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

@internal
pub fn discount_json(record: types.DiscountRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("ownerKind", json.string(record.owner_kind)),
    #("discountType", json.string(record.discount_type)),
    #("title", optional_string_value(record.title)),
    #("status", json.string(record.status)),
    #("code", optional_string_value(record.code)),
    #("payload", captured_json_value_json(record.payload)),
    #("cursor", optional_string_value(record.cursor)),
  ])
}

@internal
pub fn discount_bulk_operation_json(
  record: types.DiscountBulkOperationRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("operation", json.string(record.operation)),
    #("discountId", json.string(record.discount_id)),
    #("status", json.string(record.status)),
    #("payload", captured_json_value_json(record.payload)),
  ])
}

@internal
pub fn saved_search_json(record: types.SavedSearchRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("legacyResourceId", json.string(record.legacy_resource_id)),
    #("name", json.string(record.name)),
    #("query", json.string(record.query)),
    #("resourceType", json.string(record.resource_type)),
    #("searchTerms", json.string(record.search_terms)),
    #("filters", json.array(record.filters, saved_search_filter_json)),
    #("cursor", optional_string(record.cursor)),
  ])
}

@internal
pub fn saved_search_filter_json(record: types.SavedSearchFilter) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
  ])
}

@internal
pub fn webhook_subscription_json(
  record: types.WebhookSubscriptionRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("topic", optional_string(record.topic)),
    #("uri", optional_string(record.uri)),
    #("name", optional_string(record.name)),
    #("format", optional_string(record.format)),
    #("includeFields", json.array(record.include_fields, json.string)),
    #(
      "metafieldNamespaces",
      json.array(record.metafield_namespaces, json.string),
    ),
    #("filter", optional_string(record.filter)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("endpoint", optional_to_json(record.endpoint, webhook_endpoint_json)),
  ])
}

@internal
pub fn webhook_endpoint_json(
  record: types.WebhookSubscriptionEndpoint,
) -> Json {
  case record {
    types.WebhookHttpEndpoint(callback_url) ->
      json.object([
        #("__typename", json.string("WebhookHttpEndpoint")),
        #("callbackUrl", optional_string(callback_url)),
      ])
    types.WebhookEventBridgeEndpoint(arn) ->
      json.object([
        #("__typename", json.string("WebhookEventBridgeEndpoint")),
        #("arn", optional_string(arn)),
      ])
    types.WebhookPubSubEndpoint(pub_sub_project, pub_sub_topic) ->
      json.object([
        #("__typename", json.string("WebhookPubSubEndpoint")),
        #("pubSubProject", optional_string(pub_sub_project)),
        #("pubSubTopic", optional_string(pub_sub_topic)),
      ])
  }
}

@internal
pub fn online_store_content_kind_json(
  records: Dict(String, types.OnlineStoreContentRecord),
  kind: String,
) -> Json {
  json.object(
    records
    |> dict.to_list()
    |> list.filter_map(fn(pair) {
      let #(id, record) = pair
      case record.kind == kind {
        True -> Ok(#(id, online_store_content_json(record)))
        False -> Error(Nil)
      }
    }),
  )
}

@internal
pub fn online_store_content_json(
  record: types.OnlineStoreContentRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("kind", json.string(record.kind)),
    #("cursor", optional_string(record.cursor)),
    #("parentId", optional_string(record.parent_id)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn online_store_integration_kind_json(
  records: Dict(String, types.OnlineStoreIntegrationRecord),
  kind: String,
) -> Json {
  json.object(
    records
    |> dict.to_list()
    |> list.filter_map(fn(pair) {
      let #(id, record) = pair
      case record.kind == kind {
        True -> Ok(#(id, online_store_integration_json(record)))
        False -> Error(Nil)
      }
    }),
  )
}

@internal
pub fn online_store_integration_json(
  record: types.OnlineStoreIntegrationRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("kind", json.string(record.kind)),
    #("cursor", optional_string(record.cursor)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("data", captured_json_value_json(online_store_integration_data(record))),
  ])
}

@internal
pub fn online_store_integration_data(
  record: types.OnlineStoreIntegrationRecord,
) -> types.CapturedJsonValue {
  case record.kind, record.data {
    "webPixel", types.CapturedObject(fields) ->
      types.CapturedObject(
        fields
        |> list.filter(fn(pair) { pair.0 != "webhookEndpointAddress" }),
      )
    _, data -> data
  }
}

@internal
pub fn deleted_online_store_ids_json(
  records: Dict(String, Bool),
  gid_type: String,
) -> Json {
  json.object(
    records
    |> dict.to_list()
    |> list.filter_map(fn(pair) {
      let #(id, deleted) = pair
      case deleted && string_contains_gid_type(id, gid_type) {
        True -> Ok(#(id, json.bool(True)))
        False -> Error(Nil)
      }
    }),
  )
}

@internal
pub fn string_contains_gid_type(id: String, gid_type: String) -> Bool {
  string.contains(id, "gid://shopify/" <> gid_type <> "/")
}

@internal
pub fn money_json(record: types.Money) -> Json {
  json.object([
    #("amount", json.string(record.amount)),
    #("currencyCode", json.string(record.currency_code)),
  ])
}

@internal
pub fn access_scope_json(record: types.AccessScopeRecord) -> Json {
  json.object([
    #("handle", json.string(record.handle)),
    #("description", optional_string(record.description)),
  ])
}

@internal
pub fn app_json(record: types.AppRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("apiKey", optional_string(record.api_key)),
    #("handle", optional_string(record.handle)),
    #("title", optional_string(record.title)),
    #("developerName", optional_string(record.developer_name)),
    #("embedded", optional_bool(record.embedded)),
    #("previouslyInstalled", optional_bool(record.previously_installed)),
    #(
      "requestedAccessScopes",
      json.array(record.requested_access_scopes, access_scope_json),
    ),
  ])
}

@internal
pub fn app_subscription_pricing_json(
  record: types.AppSubscriptionPricing,
) -> Json {
  case record {
    types.AppRecurringPricing(price, interval, plan_handle) ->
      json.object([
        #("__typename", json.string("AppRecurringPricing")),
        #("price", money_json(price)),
        #("interval", json.string(interval)),
        #("planHandle", optional_string(plan_handle)),
      ])
    types.AppUsagePricing(capped_amount, balance_used, interval, terms) ->
      json.object([
        #("__typename", json.string("AppUsagePricing")),
        #("cappedAmount", money_json(capped_amount)),
        #("balanceUsed", money_json(balance_used)),
        #("interval", json.string(interval)),
        #("terms", optional_string(terms)),
      ])
  }
}

@internal
pub fn app_subscription_line_item_plan_json(
  record: types.AppSubscriptionLineItemPlan,
) -> Json {
  json.object([
    #("pricingDetails", app_subscription_pricing_json(record.pricing_details)),
  ])
}

@internal
pub fn app_subscription_line_item_json(
  record: types.AppSubscriptionLineItemRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("subscriptionId", json.string(record.subscription_id)),
    #("plan", app_subscription_line_item_plan_json(record.plan)),
  ])
}

@internal
pub fn app_subscription_json(record: types.AppSubscriptionRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("status", json.string(record.status)),
    #("isTest", json.bool(record.is_test)),
    #("trialDays", optional_int(record.trial_days)),
    #("currentPeriodEnd", optional_string(record.current_period_end)),
    #("createdAt", json.string(record.created_at)),
    #("lineItemIds", json.array(record.line_item_ids, json.string)),
  ])
}

@internal
pub fn app_one_time_purchase_json(
  record: types.AppOneTimePurchaseRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("status", json.string(record.status)),
    #("isTest", json.bool(record.is_test)),
    #("createdAt", json.string(record.created_at)),
    #("price", money_json(record.price)),
  ])
}

@internal
pub fn app_usage_json(record: types.AppUsageRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("subscriptionLineItemId", json.string(record.subscription_line_item_id)),
    #("description", json.string(record.description)),
    #("price", money_json(record.price)),
    #("createdAt", json.string(record.created_at)),
    #("idempotencyKey", optional_string(record.idempotency_key)),
  ])
}

@internal
pub fn delegated_access_token_json(
  record: types.DelegatedAccessTokenRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("apiClientId", json.string(record.api_client_id)),
    #(
      "parentAccessTokenSha256",
      optional_string(record.parent_access_token_sha256),
    ),
    #("accessTokenSha256", json.string(record.access_token_sha256)),
    #("accessTokenPreview", json.string(record.access_token_preview)),
    #("accessScopes", json.array(record.access_scopes, json.string)),
    #("createdAt", json.string(record.created_at)),
    #("expiresIn", optional_int(record.expires_in)),
    #("destroyedAt", optional_string(record.destroyed_at)),
  ])
}

@internal
pub fn app_installation_json(record: types.AppInstallationRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("appId", json.string(record.app_id)),
    #("launchUrl", optional_string(record.launch_url)),
    #("uninstallUrl", optional_string(record.uninstall_url)),
    #("accessScopes", json.array(record.access_scopes, access_scope_json)),
    #(
      "activeSubscriptionIds",
      json.array(record.active_subscription_ids, json.string),
    ),
    #(
      "allSubscriptionIds",
      json.array(record.all_subscription_ids, json.string),
    ),
    #(
      "oneTimePurchaseIds",
      json.array(record.one_time_purchase_ids, json.string),
    ),
    #("uninstalledAt", optional_string(record.uninstalled_at)),
  ])
}

@internal
pub fn shopify_function_app_json(
  record: types.ShopifyFunctionAppRecord,
) -> Json {
  json.object([
    #("__typename", optional_string(record.typename)),
    #("id", optional_string(record.id)),
    #("title", optional_string(record.title)),
    #("handle", optional_string(record.handle)),
    #("apiKey", optional_string(record.api_key)),
  ])
}

@internal
pub fn shopify_function_json(record: types.ShopifyFunctionRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", optional_string(record.title)),
    #("handle", optional_string(record.handle)),
    #("apiType", optional_string(record.api_type)),
    #("description", optional_string(record.description)),
    #("appKey", optional_string(record.app_key)),
    #("app", optional_to_json(record.app, shopify_function_app_json)),
  ])
}

@internal
pub fn bulk_operation_json(record: types.BulkOperationRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("status", json.string(record.status)),
    #("type", json.string(record.type_)),
    #("errorCode", optional_string(record.error_code)),
    #("createdAt", json.string(record.created_at)),
    #("completedAt", optional_string(record.completed_at)),
    #("objectCount", json.string(record.object_count)),
    #("rootObjectCount", json.string(record.root_object_count)),
    #("fileSize", optional_string(record.file_size)),
    #("url", optional_string(record.url)),
    #("partialDataUrl", optional_string(record.partial_data_url)),
    #("query", optional_string(record.query)),
    #("cursor", optional_string(record.cursor)),
    #("resultJsonl", optional_string(record.result_jsonl)),
  ])
}

@internal
pub fn product_metafield_json(record: types.ProductMetafieldRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("ownerId", json.string(record.owner_id)),
    #("namespace", json.string(record.namespace)),
    #("key", json.string(record.key)),
    #("type", optional_string(record.type_)),
    #("value", optional_string(record.value)),
    #("compareDigest", optional_string(record.compare_digest)),
    #("jsonValue", optional_to_json(record.json_value, fn(value) { value })),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("ownerType", optional_string(record.owner_type)),
    #(
      "marketLocalizableContent",
      json.array(
        record.market_localizable_content,
        market_localizable_content_json,
      ),
    ),
  ])
}

@internal
pub fn metafield_definition_json(
  record: types.MetafieldDefinitionRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("namespace", json.string(record.namespace)),
    #("key", json.string(record.key)),
    #("ownerType", json.string(record.owner_type)),
    #("type", metafield_definition_type_json(record.type_)),
    #("description", optional_string(record.description)),
    #(
      "validations",
      json.array(record.validations, metafield_definition_validation_json),
    ),
    #("access", dict_to_json(record.access, fn(value) { value })),
    #(
      "capabilities",
      metafield_definition_capabilities_json(record.capabilities),
    ),
    #(
      "constraints",
      optional_to_json(
        record.constraints,
        metafield_definition_constraints_json,
      ),
    ),
    #("pinnedPosition", optional_int(record.pinned_position)),
    #("validationStatus", json.string(record.validation_status)),
  ])
}

@internal
pub fn metafield_definition_type_json(
  record: types.MetafieldDefinitionTypeRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("category", optional_string(record.category)),
  ])
}

@internal
pub fn metafield_definition_validation_json(
  record: types.MetafieldDefinitionValidationRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", optional_string(record.value)),
  ])
}

@internal
pub fn metafield_definition_capabilities_json(
  record: types.MetafieldDefinitionCapabilitiesRecord,
) -> Json {
  json.object([
    #(
      "adminFilterable",
      metafield_definition_capability_json(record.admin_filterable),
    ),
    #(
      "smartCollectionCondition",
      metafield_definition_capability_json(record.smart_collection_condition),
    ),
    #(
      "uniqueValues",
      metafield_definition_capability_json(record.unique_values),
    ),
  ])
}

@internal
pub fn metafield_definition_capability_json(
  record: types.MetafieldDefinitionCapabilityRecord,
) -> Json {
  json.object([
    #("enabled", json.bool(record.enabled)),
    #("eligible", json.bool(record.eligible)),
    #("status", optional_string(record.status)),
  ])
}

@internal
pub fn metafield_definition_constraints_json(
  record: types.MetafieldDefinitionConstraintsRecord,
) -> Json {
  json.object([
    #("key", optional_string(record.key)),
    #(
      "values",
      json.array(record.values, metafield_definition_constraint_value_json),
    ),
  ])
}

@internal
pub fn metafield_definition_constraint_value_json(
  record: types.MetafieldDefinitionConstraintValueRecord,
) -> Json {
  json.object([#("value", json.string(record.value))])
}

@internal
pub fn metaobject_definition_json(
  record: types.MetaobjectDefinitionRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("type", json.string(record.type_)),
    #("name", optional_string(record.name)),
    #("description", optional_string(record.description)),
    #("displayNameKey", optional_string(record.display_name_key)),
    #("access", dict_to_json(record.access, optional_string)),
    #(
      "capabilities",
      metaobject_definition_capabilities_json(record.capabilities),
    ),
    #(
      "fieldDefinitions",
      json.array(record.field_definitions, metaobject_field_definition_json),
    ),
    #("hasThumbnailField", optional_bool(record.has_thumbnail_field)),
    #("metaobjectsCount", optional_int(record.metaobjects_count)),
    #(
      "standardTemplate",
      optional_to_json(
        record.standard_template,
        metaobject_standard_template_json,
      ),
    ),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn metaobject_definition_capabilities_json(
  record: types.MetaobjectDefinitionCapabilitiesRecord,
) -> Json {
  json.object([
    #(
      "publishable",
      optional_to_json(
        record.publishable,
        metaobject_definition_capability_json,
      ),
    ),
    #(
      "translatable",
      optional_to_json(
        record.translatable,
        metaobject_definition_capability_json,
      ),
    ),
    #(
      "renderable",
      optional_to_json(record.renderable, metaobject_definition_capability_json),
    ),
    #(
      "onlineStore",
      optional_to_json(
        record.online_store,
        metaobject_definition_capability_json,
      ),
    ),
  ])
}

@internal
pub fn metaobject_definition_capability_json(
  record: types.MetaobjectDefinitionCapabilityRecord,
) -> Json {
  json.object([#("enabled", json.bool(record.enabled))])
}

@internal
pub fn metaobject_definition_type_json(
  record: types.MetaobjectDefinitionTypeRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("category", optional_string(record.category)),
  ])
}

@internal
pub fn metaobject_field_definition_json(
  record: types.MetaobjectFieldDefinitionRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("name", optional_string(record.name)),
    #("description", optional_string(record.description)),
    #("required", optional_bool(record.required)),
    #("type", metaobject_definition_type_json(record.type_)),
    #(
      "capabilities",
      metaobject_field_definition_capabilities_json(record.capabilities),
    ),
    #(
      "validations",
      json.array(
        record.validations,
        metaobject_field_definition_validation_json,
      ),
    ),
  ])
}

@internal
pub fn metaobject_field_definition_capabilities_json(
  record: types.MetaobjectFieldDefinitionCapabilitiesRecord,
) -> Json {
  json.object([
    #(
      "adminFilterable",
      optional_to_json(
        record.admin_filterable,
        metaobject_definition_capability_json,
      ),
    ),
  ])
}

@internal
pub fn metaobject_field_definition_validation_json(
  record: types.MetaobjectFieldDefinitionValidationRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", optional_string(record.value)),
  ])
}

@internal
pub fn metaobject_standard_template_json(
  record: types.MetaobjectStandardTemplateRecord,
) -> Json {
  json.object([
    #("type", optional_string(record.type_)),
    #("name", optional_string(record.name)),
  ])
}

@internal
pub fn metaobject_json(record: types.MetaobjectRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("handle", json.string(record.handle)),
    #("type", json.string(record.type_)),
    #("displayName", optional_string(record.display_name)),
    #("fields", json.array(record.fields, metaobject_field_json)),
    #("capabilities", metaobject_capabilities_json(record.capabilities)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn metaobject_field_json(record: types.MetaobjectFieldRecord) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("type", optional_string(record.type_)),
    #("value", optional_string(record.value)),
    #("jsonValue", metaobject_json_value_json(record.json_value)),
    #(
      "definition",
      optional_to_json(record.definition, metaobject_field_definition_ref_json),
    ),
  ])
}

@internal
pub fn metaobject_json_value_json(value: types.MetaobjectJsonValue) -> Json {
  case value {
    types.MetaobjectNull -> json.null()
    types.MetaobjectString(value) -> json.string(value)
    types.MetaobjectBool(value) -> json.bool(value)
    types.MetaobjectInt(value) -> json.int(value)
    types.MetaobjectFloat(value) -> json.float(value)
    types.MetaobjectList(items) -> json.array(items, metaobject_json_value_json)
    types.MetaobjectObject(fields) ->
      dict_to_json(fields, metaobject_json_value_json)
  }
}

@internal
pub fn metaobject_field_definition_ref_json(
  record: types.MetaobjectFieldDefinitionReferenceRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("name", optional_string(record.name)),
    #("required", optional_bool(record.required)),
    #("type", metaobject_definition_type_json(record.type_)),
  ])
}

@internal
pub fn metaobject_capabilities_json(
  record: types.MetaobjectCapabilitiesRecord,
) -> Json {
  json.object([
    #(
      "publishable",
      optional_to_json(record.publishable, metaobject_publishable_json),
    ),
    #(
      "onlineStore",
      optional_to_json(record.online_store, metaobject_online_store_json),
    ),
  ])
}

@internal
pub fn metaobject_publishable_json(
  record: types.MetaobjectPublishableCapabilityRecord,
) -> Json {
  json.object([#("status", optional_string(record.status))])
}

@internal
pub fn metaobject_online_store_json(
  record: types.MetaobjectOnlineStoreCapabilityRecord,
) -> Json {
  json.object([#("templateSuffix", optional_string(record.template_suffix))])
}

@internal
pub fn marketing_record_json(record: types.MarketingRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("apiClientId", optional_string(record.api_client_id)),
    #("data", marketing_object_json(record.data)),
  ])
}

@internal
pub fn marketing_channel_definition_json(
  record: types.MarketingChannelDefinitionRecord,
) -> Json {
  json.object([
    #("handle", json.string(record.handle)),
    #("apiClientIds", json.array(record.api_client_ids, json.string)),
  ])
}

@internal
pub fn marketing_engagement_json(
  record: types.MarketingEngagementRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("apiClientId", optional_string(record.api_client_id)),
    #("marketingActivityId", optional_string(record.marketing_activity_id)),
    #("remoteId", optional_string(record.remote_id)),
    #("channelHandle", optional_string(record.channel_handle)),
    #("occurredOn", json.string(record.occurred_on)),
    #("data", marketing_object_json(record.data)),
  ])
}

@internal
pub fn marketing_object_json(data: Dict(String, types.MarketingValue)) -> Json {
  dict_to_json(data, marketing_value_json)
}

@internal
pub fn marketing_value_json(value: types.MarketingValue) -> Json {
  case value {
    types.MarketingNull -> json.null()
    types.MarketingString(value) -> json.string(value)
    types.MarketingBool(value) -> json.bool(value)
    types.MarketingInt(value) -> json.int(value)
    types.MarketingFloat(value) -> json.float(value)
    types.MarketingList(items) -> json.array(items, marketing_value_json)
    types.MarketingObject(fields) -> marketing_object_json(fields)
  }
}

@internal
pub fn validation_json(record: types.ValidationRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", optional_string(record.title)),
    #("enable", optional_bool(record.enable)),
    #("blockOnFailure", optional_bool(record.block_on_failure)),
    #("functionId", optional_string(record.function_id)),
    #("functionHandle", optional_string(record.function_handle)),
    #("shopifyFunctionId", optional_string(record.shopify_function_id)),
    #("metafields", json.array(record.metafields, validation_metafield_json)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn validation_metafield_json(
  record: types.ValidationMetafieldRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("validationId", json.string(record.validation_id)),
    #("namespace", json.string(record.namespace)),
    #("key", json.string(record.key)),
    #("type", optional_string(record.type_)),
    #("value", optional_string(record.value)),
    #("compareDigest", optional_string(record.compare_digest)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("ownerType", optional_string(record.owner_type)),
  ])
}

@internal
pub fn cart_transform_json(record: types.CartTransformRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", optional_string(record.title)),
    #("blockOnFailure", optional_bool(record.block_on_failure)),
    #("functionId", optional_string(record.function_id)),
    #("functionHandle", optional_string(record.function_handle)),
    #("shopifyFunctionId", optional_string(record.shopify_function_id)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn tax_app_configuration_json(
  record: types.TaxAppConfigurationRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("ready", json.bool(record.ready)),
    #("state", json.string(record.state)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn shipping_package_weight_json(
  record: types.ShippingPackageWeightRecord,
) -> Json {
  json.object([
    #("value", optional_float(record.value)),
    #("unit", optional_string(record.unit)),
  ])
}

@internal
pub fn shipping_package_dimensions_json(
  record: types.ShippingPackageDimensionsRecord,
) -> Json {
  json.object([
    #("length", optional_float(record.length)),
    #("width", optional_float(record.width)),
    #("height", optional_float(record.height)),
    #("unit", optional_string(record.unit)),
  ])
}

@internal
pub fn shipping_package_json(record: types.ShippingPackageRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", optional_string(record.name)),
    #("type", optional_string(record.type_)),
    #("boxType", optional_string(record.box_type)),
    #("default", json.bool(record.default)),
    #("weight", optional_to_json(record.weight, shipping_package_weight_json)),
    #(
      "dimensions",
      optional_to_json(record.dimensions, shipping_package_dimensions_json),
    ),
    #("createdAt", json.string(record.created_at)),
    #("updatedAt", json.string(record.updated_at)),
  ])
}

@internal
pub fn carrier_service_json(record: types.CarrierServiceRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", optional_string(record.name)),
    #("formattedName", optional_string(record.formatted_name)),
    #("callbackUrl", optional_string(record.callback_url)),
    #("active", json.bool(record.active)),
    #("supportsServiceDiscovery", json.bool(record.supports_service_discovery)),
    #("createdAt", json.string(record.created_at)),
    #("updatedAt", json.string(record.updated_at)),
  ])
}

@internal
pub fn fulfillment_service_json(
  record: types.FulfillmentServiceRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("handle", json.string(record.handle)),
    #("serviceName", json.string(record.service_name)),
    #("callbackUrl", optional_string(record.callback_url)),
    #("inventoryManagement", json.bool(record.inventory_management)),
    #("locationId", optional_string(record.location_id)),
    #("requiresShippingMethod", json.bool(record.requires_shipping_method)),
    #("trackingSupport", json.bool(record.tracking_support)),
    #("type", json.string(record.type_)),
  ])
}

@internal
pub fn fulfillment_json(record: types.FulfillmentRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("orderId", optional_string(record.order_id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn fulfillment_order_json(record: types.FulfillmentOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("orderId", optional_string(record.order_id)),
    #("status", json.string(record.status)),
    #("requestStatus", json.string(record.request_status)),
    #("assignedLocationId", optional_string(record.assigned_location_id)),
    #("assignmentStatus", optional_string(record.assignment_status)),
    #("manuallyHeld", json.bool(record.manually_held)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn shipping_order_json(record: types.ShippingOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn reverse_fulfillment_order_json(
  record: types.ReverseFulfillmentOrderRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn reverse_delivery_json(record: types.ReverseDeliveryRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #(
      "reverseFulfillmentOrderId",
      json.string(record.reverse_fulfillment_order_id),
    ),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn calculated_order_json(record: types.CalculatedOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

@internal
pub fn gift_card_transaction_json(
  record: types.GiftCardTransactionRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("kind", json.string(record.kind)),
    #("amount", money_json(record.amount)),
    #("processedAt", json.string(record.processed_at)),
    #("note", optional_string(record.note)),
  ])
}

@internal
pub fn gift_card_recipient_attributes_json(
  record: types.GiftCardRecipientAttributesRecord,
) -> Json {
  json.object([
    #("id", optional_string(record.id)),
    #("message", optional_string(record.message)),
    #("preferredName", optional_string(record.preferred_name)),
    #("sendNotificationAt", optional_string(record.send_notification_at)),
  ])
}

@internal
pub fn gift_card_json(record: types.GiftCardRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("legacyResourceId", json.string(record.legacy_resource_id)),
    #("lastCharacters", json.string(record.last_characters)),
    #("maskedCode", json.string(record.masked_code)),
    #("code", optional_string(record.code)),
    #("enabled", json.bool(record.enabled)),
    #("notify", json.bool(record.notify)),
    #("deactivatedAt", optional_string(record.deactivated_at)),
    #("expiresOn", optional_string(record.expires_on)),
    #("note", optional_string(record.note)),
    #("templateSuffix", optional_string(record.template_suffix)),
    #("createdAt", json.string(record.created_at)),
    #("updatedAt", json.string(record.updated_at)),
    #("initialValue", money_json(record.initial_value)),
    #("balance", money_json(record.balance)),
    #("customerId", optional_string(record.customer_id)),
    #("recipientId", optional_string(record.recipient_id)),
    #("source", optional_string(record.source)),
    #(
      "recipientAttributes",
      optional_to_json(
        record.recipient_attributes,
        gift_card_recipient_attributes_json,
      ),
    ),
    #(
      "transactions",
      json.array(record.transactions, gift_card_transaction_json),
    ),
  ])
}

@internal
pub fn gift_card_configuration_json(
  record: types.GiftCardConfigurationRecord,
) -> Json {
  json.object([
    #("issueLimit", money_json(record.issue_limit)),
    #("purchaseLimit", money_json(record.purchase_limit)),
  ])
}

@internal
pub fn segment_json(record: types.SegmentRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", optional_string(record.name)),
    #("query", optional_string(record.query)),
    #("creationDate", optional_string(record.creation_date)),
    #("lastEditDate", optional_string(record.last_edit_date)),
  ])
}

@internal
pub fn customer_segment_members_query_json(
  record: types.CustomerSegmentMembersQueryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("query", optional_string(record.query)),
    #("segmentId", optional_string(record.segment_id)),
    #("status", json.string(record.status)),
    #("currentCount", json.int(record.current_count)),
    #("done", json.bool(record.done)),
  ])
}

@internal
pub fn customer_json(record: types.CustomerRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("firstName", optional_string(record.first_name)),
    #("lastName", optional_string(record.last_name)),
    #("displayName", optional_string(record.display_name)),
    #("email", optional_string(record.email)),
    #("legacyResourceId", optional_string(record.legacy_resource_id)),
    #("locale", optional_string(record.locale)),
    #("note", optional_string(record.note)),
    #("canDelete", optional_bool(record.can_delete)),
    #("verifiedEmail", optional_bool(record.verified_email)),
    #("dataSaleOptOut", json.bool(record.data_sale_opt_out)),
    #("taxExempt", optional_bool(record.tax_exempt)),
    #("taxExemptions", json.array(record.tax_exemptions, json.string)),
    #("state", optional_string(record.state)),
    #("tags", json.array(record.tags, json.string)),
    #("numberOfOrders", optional_string(record.number_of_orders)),
    #("amountSpent", optional_to_json(record.amount_spent, money_json)),
    #(
      "defaultEmailAddress",
      optional_to_json(
        record.default_email_address,
        customer_default_email_address_json,
      ),
    ),
    #(
      "defaultPhoneNumber",
      optional_to_json(
        record.default_phone_number,
        customer_default_phone_number_json,
      ),
    ),
    #(
      "emailMarketingConsent",
      optional_to_json(
        record.email_marketing_consent,
        customer_email_marketing_consent_json,
      ),
    ),
    #(
      "smsMarketingConsent",
      optional_to_json(
        record.sms_marketing_consent,
        customer_sms_marketing_consent_json,
      ),
    ),
    #(
      "defaultAddress",
      optional_to_json(record.default_address, customer_default_address_json),
    ),
    #(
      "accountActivationToken",
      optional_string(record.account_activation_token),
    ),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn customer_default_email_address_json(
  record: types.CustomerDefaultEmailAddressRecord,
) -> Json {
  json.object([
    #("emailAddress", optional_string(record.email_address)),
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("marketingUpdatedAt", optional_string(record.marketing_updated_at)),
  ])
}

@internal
pub fn customer_default_phone_number_json(
  record: types.CustomerDefaultPhoneNumberRecord,
) -> Json {
  json.object([
    #("phoneNumber", optional_string(record.phone_number)),
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("marketingUpdatedAt", optional_string(record.marketing_updated_at)),
  ])
}

@internal
pub fn customer_email_marketing_consent_json(
  record: types.CustomerEmailMarketingConsentRecord,
) -> Json {
  json.object([
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("consentUpdatedAt", optional_string(record.consent_updated_at)),
  ])
}

@internal
pub fn customer_sms_marketing_consent_json(
  record: types.CustomerSmsMarketingConsentRecord,
) -> Json {
  json.object([
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("consentUpdatedAt", optional_string(record.consent_updated_at)),
    #("consentCollectedFrom", optional_string(record.consent_collected_from)),
  ])
}

@internal
pub fn customer_default_address_json(
  record: types.CustomerDefaultAddressRecord,
) -> Json {
  json.object([
    #("id", optional_string(record.id)),
    #("address1", optional_string(record.address1)),
    #("city", optional_string(record.city)),
    #("country", optional_string(record.country)),
    #("zip", optional_string(record.zip)),
  ])
}

@internal
pub fn customer_address_json(record: types.CustomerAddressRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
    #("firstName", optional_string(record.first_name)),
    #("lastName", optional_string(record.last_name)),
    #("address1", optional_string(record.address1)),
    #("address2", optional_string(record.address2)),
    #("city", optional_string(record.city)),
    #("company", optional_string(record.company)),
    #("province", optional_string(record.province)),
    #("provinceCode", optional_string(record.province_code)),
    #("country", optional_string(record.country)),
    #("countryCodeV2", optional_string(record.country_code_v2)),
    #("zip", optional_string(record.zip)),
    #("phone", optional_string(record.phone)),
    #("name", optional_string(record.name)),
    #("formattedArea", optional_string(record.formatted_area)),
  ])
}

@internal
pub fn customer_catalog_connection_json(
  record: types.CustomerCatalogConnectionRecord,
) -> Json {
  json.object([
    #(
      "orderedCustomerIds",
      json.array(record.ordered_customer_ids, json.string),
    ),
    #(
      "cursorByCustomerId",
      dict_to_json(record.cursor_by_customer_id, json.string),
    ),
    #("pageInfo", customer_catalog_page_info_json(record.page_info)),
  ])
}

@internal
pub fn customer_catalog_page_info_json(
  record: types.CustomerCatalogPageInfoRecord,
) -> Json {
  json.object([
    #("hasNextPage", json.bool(record.has_next_page)),
    #("hasPreviousPage", json.bool(record.has_previous_page)),
    #("startCursor", optional_string(record.start_cursor)),
    #("endCursor", optional_string(record.end_cursor)),
  ])
}

@internal
pub fn customer_order_summary_json(
  record: types.CustomerOrderSummaryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", optional_string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
    #("name", optional_string(record.name)),
    #("email", optional_string(record.email)),
    #("createdAt", optional_string(record.created_at)),
    #(
      "currentTotalPrice",
      optional_to_json(record.current_total_price, money_json),
    ),
  ])
}

@internal
pub fn customer_event_summary_json(
  record: types.CustomerEventSummaryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
  ])
}

@internal
pub fn customer_metafield_json(record: types.CustomerMetafieldRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("namespace", json.string(record.namespace)),
    #("key", json.string(record.key)),
    #("type", json.string(record.type_)),
    #("value", json.string(record.value)),
    #("compareDigest", optional_string(record.compare_digest)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

@internal
pub fn customer_payment_method_json(
  record: types.CustomerPaymentMethodRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
    #(
      "instrument",
      optional_to_json(
        record.instrument,
        customer_payment_method_instrument_json,
      ),
    ),
    #("revokedAt", optional_string(record.revoked_at)),
    #("revokedReason", optional_string(record.revoked_reason)),
    #(
      "subscriptionContracts",
      json.array(
        record.subscription_contracts,
        customer_payment_method_subscription_contract_json,
      ),
    ),
  ])
}

@internal
pub fn customer_payment_method_instrument_json(
  record: types.CustomerPaymentMethodInstrumentRecord,
) -> Json {
  json.object([
    #("__typename", json.string(record.type_name)),
    #("data", dict_to_json(record.data, json.string)),
  ])
}

@internal
pub fn customer_payment_method_subscription_contract_json(
  record: types.CustomerPaymentMethodSubscriptionContractRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", dict_to_json(record.data, json.string)),
  ])
}

@internal
pub fn customer_payment_method_update_url_json(
  record: types.CustomerPaymentMethodUpdateUrlRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerPaymentMethodId", json.string(record.customer_payment_method_id)),
    #("updatePaymentMethodUrl", json.string(record.update_payment_method_url)),
    #("createdAt", json.string(record.created_at)),
  ])
}

@internal
pub fn payment_reminder_send_json(
  record: types.PaymentReminderSendRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("paymentScheduleId", json.string(record.payment_schedule_id)),
    #("sentAt", json.string(record.sent_at)),
  ])
}

@internal
pub fn payment_customization_json(
  record: types.PaymentCustomizationRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", optional_string(record.title)),
    #("enabled", optional_bool(record.enabled)),
    #("functionId", optional_string(record.function_id)),
    #("functionHandle", optional_string(record.function_handle)),
    #(
      "metafields",
      json.array(record.metafields, payment_customization_metafield_json),
    ),
  ])
}

@internal
pub fn payment_customization_metafield_json(
  record: types.PaymentCustomizationMetafieldRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("paymentCustomizationId", json.string(record.payment_customization_id)),
    #("namespace", json.string(record.namespace)),
    #("key", json.string(record.key)),
    #("type", optional_string(record.type_)),
    #("value", optional_string(record.value)),
    #("compareDigest", optional_string(record.compare_digest)),
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
    #("ownerType", optional_string(record.owner_type)),
  ])
}

@internal
pub fn payment_schedule_json(record: types.PaymentScheduleRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("dueAt", optional_string(record.due_at)),
    #("issuedAt", optional_string(record.issued_at)),
    #("completedAt", optional_string(record.completed_at)),
    #("due", optional_bool(record.due)),
    #("amount", optional_to_json(record.amount, money_json)),
    #("balanceDue", optional_to_json(record.balance_due, money_json)),
    #("totalBalance", optional_to_json(record.total_balance, money_json)),
  ])
}

@internal
pub fn payment_terms_json(record: types.PaymentTermsRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("ownerId", json.string(record.owner_id)),
    #("due", json.bool(record.due)),
    #("overdue", json.bool(record.overdue)),
    #("dueInDays", optional_int(record.due_in_days)),
    #("paymentTermsName", json.string(record.payment_terms_name)),
    #("paymentTermsType", json.string(record.payment_terms_type)),
    #("translatedName", json.string(record.translated_name)),
    #(
      "paymentSchedules",
      json.array(record.payment_schedules, payment_schedule_json),
    ),
  ])
}

@internal
pub fn order_mandate_payment_json(
  record: types.OrderMandatePaymentRecord,
) -> Json {
  json.object([
    #("orderId", json.string(record.order_id)),
    #("idempotencyKey", json.string(record.idempotency_key)),
    #("jobId", json.string(record.job_id)),
    #("paymentReferenceId", json.string(record.payment_reference_id)),
    #("transactionId", json.string(record.transaction_id)),
  ])
}

@internal
pub fn store_credit_account_json(
  record: types.StoreCreditAccountRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
    #("balance", money_json(record.balance)),
  ])
}

@internal
pub fn store_credit_account_transaction_json(
  record: types.StoreCreditAccountTransactionRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("accountId", json.string(record.account_id)),
    #("amount", money_json(record.amount)),
    #("balanceAfterTransaction", money_json(record.balance_after_transaction)),
    #("createdAt", json.string(record.created_at)),
    #("event", json.string(record.event)),
  ])
}

@internal
pub fn customer_account_page_json(
  record: types.CustomerAccountPageRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", json.string(record.title)),
    #("handle", json.string(record.handle)),
    #("defaultCursor", json.string(record.default_cursor)),
    #("cursor", optional_string(record.cursor)),
  ])
}

@internal
pub fn customer_data_erasure_request_json(
  record: types.CustomerDataErasureRequestRecord,
) -> Json {
  json.object([
    #("customerId", json.string(record.customer_id)),
    #("requestedAt", json.string(record.requested_at)),
    #("canceledAt", optional_string(record.canceled_at)),
  ])
}

@internal
pub fn customer_merge_request_json(
  record: types.CustomerMergeRequestRecord,
) -> Json {
  json.object([
    #("jobId", json.string(record.job_id)),
    #("resultingCustomerId", json.string(record.resulting_customer_id)),
    #("status", json.string(record.status)),
    #(
      "customerMergeErrors",
      json.array(record.customer_merge_errors, customer_merge_error_json),
    ),
  ])
}

@internal
pub fn customer_merge_error_json(
  record: types.CustomerMergeErrorRecord,
) -> Json {
  json.object([
    #("errorFields", json.array(record.error_fields, json.string)),
    #("message", json.string(record.message)),
    #("code", optional_string(record.code)),
    #("blockType", optional_string(record.block_type)),
  ])
}

@internal
pub fn locale_json(record: types.LocaleRecord) -> Json {
  json.object([
    #("isoCode", json.string(record.iso_code)),
    #("name", json.string(record.name)),
  ])
}

@internal
pub fn shop_locale_json(record: types.ShopLocaleRecord) -> Json {
  json.object([
    #("locale", json.string(record.locale)),
    #("name", json.string(record.name)),
    #("primary", json.bool(record.primary)),
    #("published", json.bool(record.published)),
    #(
      "marketWebPresenceIds",
      json.array(record.market_web_presence_ids, json.string),
    ),
  ])
}

@internal
pub fn translation_json(record: types.TranslationRecord) -> Json {
  json.object([
    #("resourceId", json.string(record.resource_id)),
    #("key", json.string(record.key)),
    #("locale", json.string(record.locale)),
    #("value", json.string(record.value)),
    #(
      "translatableContentDigest",
      json.string(record.translatable_content_digest),
    ),
    #("marketId", optional_string(record.market_id)),
    #("updatedAt", json.string(record.updated_at)),
    #("outdated", json.bool(record.outdated)),
  ])
}

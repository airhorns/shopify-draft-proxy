import gleam/dynamic/decode.{type Decoder}
import gleam/json.{type Json}
import shopify_draft_proxy/state/serialization/shared.{
  bool_dict_field, bool_dict_to_json, dict_field, dict_to_json, dump_field_names,
  optional_field, optional_string, optional_string_field, optional_to_json,
  require_object_fields, string_list_field,
}
import shopify_draft_proxy/state/serialization/shared/decoders.{
  abandoned_checkout_decoder, abandonment_decoder, app_decoder,
  app_installation_decoder, app_one_time_purchase_decoder,
  app_subscription_decoder, app_subscription_line_item_decoder,
  app_usage_decoder, backup_region_decoder, bulk_operation_decoder,
  captured_json_value_decoder, cart_transform_decoder, catalog_decoder,
  customer_segment_members_query_decoder, delegated_access_token_decoder,
  draft_order_decoder, draft_order_variant_catalog_decoder,
  flow_signature_decoder, flow_trigger_decoder, gift_card_configuration_decoder,
  gift_card_decoder, locale_decoder, market_decoder, market_localization_decoder,
  marketing_channel_definition_decoder, marketing_engagement_decoder,
  marketing_record_decoder, metafield_definition_decoder, metaobject_decoder,
  metaobject_definition_decoder, order_decoder, price_list_decoder,
  product_metafield_decoder, saved_search_decoder, segment_decoder,
  shipping_package_decoder, shop_decoder, shop_locale_decoder,
  shopify_function_decoder, store_property_mutation_payload_decoder,
  store_property_record_decoder, store_property_value_decoder,
  tax_app_configuration_decoder, translation_decoder, url_redirect_decoder,
  validation_decoder, web_presence_decoder, webhook_subscription_decoder,
}
import shopify_draft_proxy/state/serialization/shared/serializers.{
  abandoned_checkout_json, abandonment_json, admin_platform_generic_node_json,
  admin_platform_taxonomy_category_json, app_installation_json, app_json,
  app_one_time_purchase_json, app_subscription_json,
  app_subscription_line_item_json, app_usage_json, b2b_company_contact_json,
  b2b_company_contact_role_json, b2b_company_json, b2b_company_location_json,
  backup_region_json, bulk_operation_json, calculated_order_json,
  captured_json_value_json, carrier_service_json, cart_transform_json,
  catalog_json, customer_account_page_json, customer_address_json,
  customer_catalog_connection_json, customer_catalog_page_info_json,
  customer_data_erasure_request_json, customer_event_summary_json, customer_json,
  customer_merge_request_json, customer_metafield_json,
  customer_order_summary_json, customer_payment_method_json,
  customer_payment_method_update_url_json, customer_segment_members_query_json,
  delegated_access_token_json, delivery_profile_json,
  discount_bulk_operation_json, discount_json, draft_order_json,
  draft_order_variant_catalog_json, file_json, flow_signature_json,
  flow_trigger_json, fulfillment_json, fulfillment_order_json,
  fulfillment_service_json, gift_card_configuration_json, gift_card_json,
  locale_json, market_json, market_localization_json,
  marketing_channel_definition_json, marketing_engagement_json,
  marketing_record_json, metafield_definition_json, metaobject_definition_json,
  metaobject_json, online_store_content_kind_json,
  online_store_integration_kind_json, order_json, order_mandate_payment_json,
  payment_customization_json, payment_reminder_send_json, payment_terms_json,
  price_list_json, product_json, product_metafield_json, product_variant_json,
  reverse_delivery_json, reverse_fulfillment_order_json, saved_search_json,
  segment_json, selling_plan_group_json, shipping_order_json,
  shipping_package_json, shop_json, shop_locale_json, shopify_function_json,
  store_credit_account_json, store_credit_account_transaction_json,
  store_property_mutation_payload_json, store_property_record_json,
  store_property_value_json, tax_app_configuration_json, translation_json,
  url_redirect_json, validation_json, web_presence_json,
  webhook_subscription_json,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types

pub fn serialize_base_state(state: store.BaseState) -> Json {
  json.object(base_state_dump_fields(state))
}

pub fn base_state_dump_field_names() -> List(String) {
  base_state_dump_fields(store.empty_base_state())
  |> dump_field_names
}

fn base_state_dump_fields(state: store.BaseState) -> List(#(String, Json)) {
  [
    #("backupRegion", optional_to_json(state.backup_region, backup_region_json)),
    #(
      "adminPlatformGenericNodes",
      dict_to_json(
        state.admin_platform_generic_nodes,
        admin_platform_generic_node_json,
      ),
    ),
    #(
      "adminPlatformTaxonomyCategories",
      dict_to_json(
        state.admin_platform_taxonomy_categories,
        admin_platform_taxonomy_category_json,
      ),
    ),
    #(
      "adminPlatformTaxonomyCategoryOrder",
      json.array(state.admin_platform_taxonomy_category_order, json.string),
    ),
    #(
      "adminPlatformFlowSignatures",
      dict_to_json(state.admin_platform_flow_signatures, flow_signature_json),
    ),
    #(
      "adminPlatformFlowSignatureOrder",
      json.array(state.admin_platform_flow_signature_order, json.string),
    ),
    #(
      "adminPlatformFlowTriggers",
      dict_to_json(state.admin_platform_flow_triggers, flow_trigger_json),
    ),
    #(
      "adminPlatformFlowTriggerOrder",
      json.array(state.admin_platform_flow_trigger_order, json.string),
    ),
    #("shop", optional_to_json(state.shop, shop_json)),
    #(
      "abandonedCheckouts",
      dict_to_json(state.abandoned_checkouts, abandoned_checkout_json),
    ),
    #(
      "abandonedCheckoutOrder",
      json.array(state.abandoned_checkout_order, json.string),
    ),
    #("abandonments", dict_to_json(state.abandonments, abandonment_json)),
    #("abandonmentOrder", json.array(state.abandonment_order, json.string)),
    #("draftOrders", dict_to_json(state.draft_orders, draft_order_json)),
    #("draftOrderOrder", json.array(state.draft_order_order, json.string)),
    #("deletedDraftOrderIds", bool_dict_to_json(state.deleted_draft_order_ids)),
    #(
      "draftOrderVariantCatalog",
      dict_to_json(
        state.draft_order_variant_catalog,
        draft_order_variant_catalog_json,
      ),
    ),
    #("orders", dict_to_json(state.orders, order_json)),
    #("orderOrder", json.array(state.order_order, json.string)),
    #("deletedOrderIds", bool_dict_to_json(state.deleted_order_ids)),
    #("b2bCompanies", dict_to_json(state.b2b_companies, b2b_company_json)),
    #("b2bCompanyOrder", json.array(state.b2b_company_order, json.string)),
    #("deletedB2BCompanyIds", bool_dict_to_json(state.deleted_b2b_company_ids)),
    #(
      "b2bCompanyContacts",
      dict_to_json(state.b2b_company_contacts, b2b_company_contact_json),
    ),
    #(
      "b2bCompanyContactOrder",
      json.array(state.b2b_company_contact_order, json.string),
    ),
    #(
      "deletedB2BCompanyContactIds",
      bool_dict_to_json(state.deleted_b2b_company_contact_ids),
    ),
    #(
      "b2bCompanyContactRoles",
      dict_to_json(
        state.b2b_company_contact_roles,
        b2b_company_contact_role_json,
      ),
    ),
    #(
      "b2bCompanyContactRoleOrder",
      json.array(state.b2b_company_contact_role_order, json.string),
    ),
    #(
      "deletedB2BCompanyContactRoleIds",
      bool_dict_to_json(state.deleted_b2b_company_contact_role_ids),
    ),
    #(
      "b2bCompanyLocations",
      dict_to_json(state.b2b_company_locations, b2b_company_location_json),
    ),
    #(
      "b2bCompanyLocationOrder",
      json.array(state.b2b_company_location_order, json.string),
    ),
    #(
      "deletedB2BCompanyLocationIds",
      bool_dict_to_json(state.deleted_b2b_company_location_ids),
    ),
    #("products", dict_to_json(state.products, product_json)),
    #("productOrder", json.array(state.product_order, json.string)),
    #(
      "productVariants",
      dict_to_json(state.product_variants, product_variant_json),
    ),
    #(
      "productVariantOrder",
      json.array(state.product_variant_order, json.string),
    ),
    #(
      "sellingPlanGroups",
      dict_to_json(state.selling_plan_groups, selling_plan_group_json),
    ),
    #(
      "sellingPlanGroupOrder",
      json.array(state.selling_plan_group_order, json.string),
    ),
    #(
      "deletedSellingPlanGroupIds",
      bool_dict_to_json(state.deleted_selling_plan_group_ids),
    ),
    #("markets", dict_to_json(state.markets, market_json)),
    #("marketOrder", json.array(state.market_order, json.string)),
    #("deletedMarketIds", bool_dict_to_json(state.deleted_market_ids)),
    #("catalogs", dict_to_json(state.catalogs, catalog_json)),
    #("catalogOrder", json.array(state.catalog_order, json.string)),
    #("deletedCatalogIds", bool_dict_to_json(state.deleted_catalog_ids)),
    #("priceLists", dict_to_json(state.price_lists, price_list_json)),
    #("priceListOrder", json.array(state.price_list_order, json.string)),
    #("deletedPriceListIds", bool_dict_to_json(state.deleted_price_list_ids)),
    #("webPresences", dict_to_json(state.web_presences, web_presence_json)),
    #("webPresenceOrder", json.array(state.web_presence_order, json.string)),
    #(
      "deletedWebPresenceIds",
      bool_dict_to_json(state.deleted_web_presence_ids),
    ),
    #(
      "marketLocalizations",
      dict_to_json(state.market_localizations, market_localization_json),
    ),
    #(
      "marketsRootPayloads",
      dict_to_json(state.markets_root_payloads, captured_json_value_json),
    ),
    #("files", dict_to_json(state.files, file_json)),
    #("fileOrder", json.array(state.file_order, json.string)),
    #("deletedFileIds", bool_dict_to_json(state.deleted_file_ids)),
    #(
      "locations",
      dict_to_json(state.store_property_locations, store_property_record_json),
    ),
    #(
      "locationOrder",
      json.array(state.store_property_location_order, json.string),
    ),
    #(
      "deletedLocationIds",
      bool_dict_to_json(state.deleted_store_property_location_ids),
    ),
    #(
      "businessEntities",
      dict_to_json(state.business_entities, store_property_record_json),
    ),
    #(
      "businessEntityOrder",
      json.array(state.business_entity_order, json.string),
    ),
    #(
      "publishables",
      dict_to_json(state.publishables, store_property_record_json),
    ),
    #("publishableOrder", json.array(state.publishable_order, json.string)),
    #(
      "storePropertyMutationPayloads",
      dict_to_json(
        state.store_property_mutation_payloads,
        store_property_mutation_payload_json,
      ),
    ),
    #(
      "productMetafields",
      dict_to_json(state.product_metafields, product_metafield_json),
    ),
    #(
      "metafieldDefinitions",
      dict_to_json(state.metafield_definitions, metafield_definition_json),
    ),
    #(
      "deletedMetafieldDefinitionIds",
      bool_dict_to_json(state.deleted_metafield_definition_ids),
    ),
    #("savedSearches", dict_to_json(state.saved_searches, saved_search_json)),
    #("savedSearchOrder", json.array(state.saved_search_order, json.string)),
    #(
      "deletedSavedSearchIds",
      bool_dict_to_json(state.deleted_saved_search_ids),
    ),
    #(
      "webhookSubscriptions",
      dict_to_json(state.webhook_subscriptions, webhook_subscription_json),
    ),
    #(
      "webhookSubscriptionOrder",
      json.array(state.webhook_subscription_order, json.string),
    ),
    #(
      "deletedWebhookSubscriptionIds",
      bool_dict_to_json(state.deleted_webhook_subscription_ids),
    ),
    #(
      "onlineStoreArticles",
      online_store_content_kind_json(state.online_store_content, "article"),
    ),
    #(
      "onlineStoreBlogs",
      online_store_content_kind_json(state.online_store_content, "blog"),
    ),
    #(
      "onlineStorePages",
      online_store_content_kind_json(state.online_store_content, "page"),
    ),
    #(
      "onlineStoreComments",
      online_store_content_kind_json(state.online_store_content, "comment"),
    ),
    #(
      "onlineStoreThemes",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "theme",
      ),
    ),
    #(
      "onlineStoreScriptTags",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "scriptTag",
      ),
    ),
    #(
      "onlineStoreWebPixels",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "webPixel",
      ),
    ),
    #(
      "onlineStoreServerPixels",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "serverPixel",
      ),
    ),
    #(
      "onlineStoreStorefrontAccessTokens",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "storefrontAccessToken",
      ),
    ),
    #(
      "onlineStoreMobilePlatformApplications",
      online_store_integration_kind_json(
        state.online_store_integrations,
        "mobilePlatformApplication",
      ),
    ),
    #("apps", dict_to_json(state.apps, app_json)),
    #("appOrder", json.array(state.app_order, json.string)),
    #(
      "appInstallations",
      dict_to_json(state.app_installations, app_installation_json),
    ),
    #(
      "appInstallationOrder",
      json.array(state.app_installation_order, json.string),
    ),
    #(
      "currentAppInstallationId",
      optional_string(state.current_installation_id),
    ),
    #(
      "appSubscriptions",
      dict_to_json(state.app_subscriptions, app_subscription_json),
    ),
    #(
      "appSubscriptionOrder",
      json.array(state.app_subscription_order, json.string),
    ),
    #(
      "appSubscriptionLineItems",
      dict_to_json(
        state.app_subscription_line_items,
        app_subscription_line_item_json,
      ),
    ),
    #(
      "appSubscriptionLineItemOrder",
      json.array(state.app_subscription_line_item_order, json.string),
    ),
    #(
      "appOneTimePurchases",
      dict_to_json(state.app_one_time_purchases, app_one_time_purchase_json),
    ),
    #(
      "appOneTimePurchaseOrder",
      json.array(state.app_one_time_purchase_order, json.string),
    ),
    #("appUsageRecords", dict_to_json(state.app_usage_records, app_usage_json)),
    #(
      "appUsageRecordOrder",
      json.array(state.app_usage_record_order, json.string),
    ),
    #(
      "delegatedAccessTokens",
      dict_to_json(state.delegated_access_tokens, delegated_access_token_json),
    ),
    #(
      "delegatedAccessTokenOrder",
      json.array(state.delegated_access_token_order, json.string),
    ),
    #(
      "shopifyFunctions",
      dict_to_json(state.shopify_functions, shopify_function_json),
    ),
    #(
      "shopifyFunctionOrder",
      json.array(state.shopify_function_order, json.string),
    ),
    #(
      "bulkOperations",
      dict_to_json(state.bulk_operations, bulk_operation_json),
    ),
    #("bulkOperationOrder", json.array(state.bulk_operation_order, json.string)),
    #(
      "metaobjectDefinitions",
      dict_to_json(state.metaobject_definitions, metaobject_definition_json),
    ),
    #(
      "metaobjectDefinitionOrder",
      json.array(state.metaobject_definition_order, json.string),
    ),
    #(
      "deletedMetaobjectDefinitionIds",
      bool_dict_to_json(state.deleted_metaobject_definition_ids),
    ),
    #("metaobjects", dict_to_json(state.metaobjects, metaobject_json)),
    #("metaobjectOrder", json.array(state.metaobject_order, json.string)),
    #("deletedMetaobjectIds", bool_dict_to_json(state.deleted_metaobject_ids)),
    #("urlRedirects", dict_to_json(state.url_redirects, url_redirect_json)),
    #("urlRedirectOrder", json.array(state.url_redirect_order, json.string)),
    #(
      "deletedUrlRedirectIds",
      bool_dict_to_json(state.deleted_url_redirect_ids),
    ),
    #(
      "marketingActivities",
      dict_to_json(state.marketing_activities, marketing_record_json),
    ),
    #(
      "marketingActivityOrder",
      json.array(state.marketing_activity_order, json.string),
    ),
    #(
      "marketingEvents",
      dict_to_json(state.marketing_events, marketing_record_json),
    ),
    #(
      "marketingEventOrder",
      json.array(state.marketing_event_order, json.string),
    ),
    #(
      "marketingChannelDefinitions",
      dict_to_json(
        state.marketing_channel_definitions,
        marketing_channel_definition_json,
      ),
    ),
    #(
      "marketingEngagements",
      dict_to_json(state.marketing_engagements, marketing_engagement_json),
    ),
    #(
      "marketingEngagementOrder",
      json.array(state.marketing_engagement_order, json.string),
    ),
    #(
      "deletedMarketingActivityIds",
      bool_dict_to_json(state.deleted_marketing_activity_ids),
    ),
    #(
      "deletedMarketingEventIds",
      bool_dict_to_json(state.deleted_marketing_event_ids),
    ),
    #(
      "deletedMarketingEngagementIds",
      bool_dict_to_json(state.deleted_marketing_engagement_ids),
    ),
    #("validations", dict_to_json(state.validations, validation_json)),
    #("validationOrder", json.array(state.validation_order, json.string)),
    #("deletedValidationIds", bool_dict_to_json(state.deleted_validation_ids)),
    #(
      "cartTransforms",
      dict_to_json(state.cart_transforms, cart_transform_json),
    ),
    #("cartTransformOrder", json.array(state.cart_transform_order, json.string)),
    #(
      "deletedCartTransformIds",
      bool_dict_to_json(state.deleted_cart_transform_ids),
    ),
    #(
      "taxAppConfiguration",
      optional_to_json(state.tax_app_configuration, tax_app_configuration_json),
    ),
    #(
      "carrierServices",
      dict_to_json(state.carrier_services, carrier_service_json),
    ),
    #(
      "carrierServiceOrder",
      json.array(state.carrier_service_order, json.string),
    ),
    #(
      "deletedCarrierServiceIds",
      bool_dict_to_json(state.deleted_carrier_service_ids),
    ),
    #(
      "fulfillmentServices",
      dict_to_json(state.fulfillment_services, fulfillment_service_json),
    ),
    #(
      "fulfillmentServiceOrder",
      json.array(state.fulfillment_service_order, json.string),
    ),
    #(
      "deletedFulfillmentServiceIds",
      bool_dict_to_json(state.deleted_fulfillment_service_ids),
    ),
    #("fulfillments", dict_to_json(state.fulfillments, fulfillment_json)),
    #("fulfillmentOrder", json.array(state.fulfillment_order, json.string)),
    #(
      "fulfillmentOrders",
      dict_to_json(state.fulfillment_orders, fulfillment_order_json),
    ),
    #(
      "fulfillmentOrderOrder",
      json.array(state.fulfillment_order_order, json.string),
    ),
    #(
      "shippingOrders",
      dict_to_json(state.shipping_orders, shipping_order_json),
    ),
    #(
      "reverseFulfillmentOrders",
      dict_to_json(
        state.reverse_fulfillment_orders,
        reverse_fulfillment_order_json,
      ),
    ),
    #(
      "reverseFulfillmentOrderOrder",
      json.array(state.reverse_fulfillment_order_order, json.string),
    ),
    #(
      "reverseDeliveries",
      dict_to_json(state.reverse_deliveries, reverse_delivery_json),
    ),
    #(
      "reverseDeliveryOrder",
      json.array(state.reverse_delivery_order, json.string),
    ),
    #(
      "calculatedOrders",
      dict_to_json(state.calculated_orders, calculated_order_json),
    ),
    #(
      "deliveryProfiles",
      dict_to_json(state.delivery_profiles, delivery_profile_json),
    ),
    #(
      "deliveryProfileOrder",
      json.array(state.delivery_profile_order, json.string),
    ),
    #(
      "deletedDeliveryProfileIds",
      bool_dict_to_json(state.deleted_delivery_profile_ids),
    ),
    #(
      "shippingPackages",
      dict_to_json(state.shipping_packages, shipping_package_json),
    ),
    #(
      "shippingPackageOrder",
      json.array(state.shipping_package_order, json.string),
    ),
    #(
      "deletedShippingPackageIds",
      bool_dict_to_json(state.deleted_shipping_package_ids),
    ),
    #("discounts", dict_to_json(state.discounts, discount_json)),
    #("discountOrder", json.array(state.discount_order, json.string)),
    #("deletedDiscountIds", bool_dict_to_json(state.deleted_discount_ids)),
    #(
      "discountBulkOperations",
      dict_to_json(state.discount_bulk_operations, discount_bulk_operation_json),
    ),
    #("giftCards", dict_to_json(state.gift_cards, gift_card_json)),
    #("giftCardOrder", json.array(state.gift_card_order, json.string)),
    #(
      "giftCardConfiguration",
      optional_to_json(
        state.gift_card_configuration,
        gift_card_configuration_json,
      ),
    ),
    #("customers", dict_to_json(state.customers, customer_json)),
    #("customerOrder", json.array(state.customer_order, json.string)),
    #(
      "customerCatalogConnections",
      dict_to_json(
        state.customer_catalog_connections,
        customer_catalog_connection_json,
      ),
    ),
    #("deletedCustomerIds", bool_dict_to_json(state.deleted_customer_ids)),
    #(
      "customerAddresses",
      dict_to_json(state.customer_addresses, customer_address_json),
    ),
    #(
      "customerAddressOrder",
      json.array(state.customer_address_order, json.string),
    ),
    #(
      "deletedCustomerAddressIds",
      bool_dict_to_json(state.deleted_customer_address_ids),
    ),
    #(
      "customerOrderSummaries",
      dict_to_json(state.customer_order_summaries, customer_order_summary_json),
    ),
    #(
      "customerOrderConnectionPageInfos",
      dict_to_json(
        state.customer_order_connection_page_infos,
        customer_catalog_page_info_json,
      ),
    ),
    #(
      "customerEventSummaries",
      dict_to_json(state.customer_event_summaries, customer_event_summary_json),
    ),
    #(
      "customerEventConnectionPageInfos",
      dict_to_json(
        state.customer_event_connection_page_infos,
        customer_catalog_page_info_json,
      ),
    ),
    #(
      "customerLastOrders",
      dict_to_json(state.customer_last_orders, customer_order_summary_json),
    ),
    #(
      "customerMetafields",
      dict_to_json(state.customer_metafields, customer_metafield_json),
    ),
    #(
      "customerPaymentMethods",
      dict_to_json(state.customer_payment_methods, customer_payment_method_json),
    ),
    #(
      "customerPaymentMethodUpdateUrls",
      dict_to_json(
        state.customer_payment_method_update_urls,
        customer_payment_method_update_url_json,
      ),
    ),
    #(
      "deletedCustomerPaymentMethodIds",
      bool_dict_to_json(state.deleted_customer_payment_method_ids),
    ),
    #(
      "paymentReminderSends",
      dict_to_json(state.payment_reminder_sends, payment_reminder_send_json),
    ),
    #(
      "paymentCustomizations",
      dict_to_json(state.payment_customizations, payment_customization_json),
    ),
    #(
      "paymentCustomizationOrder",
      json.array(state.payment_customization_order, json.string),
    ),
    #(
      "deletedPaymentCustomizationIds",
      bool_dict_to_json(state.deleted_payment_customization_ids),
    ),
    #("paymentTerms", dict_to_json(state.payment_terms, payment_terms_json)),
    #("paymentTermsOwnerIds", bool_dict_to_json(state.payment_terms_owner_ids)),
    #(
      "paymentTermsByOwnerId",
      dict_to_json(state.payment_terms_by_owner_id, json.string),
    ),
    #(
      "deletedPaymentTermsIds",
      bool_dict_to_json(state.deleted_payment_terms_ids),
    ),
    #(
      "orderMandatePayments",
      dict_to_json(state.order_mandate_payments, order_mandate_payment_json),
    ),
    #(
      "storeCreditAccounts",
      dict_to_json(state.store_credit_accounts, store_credit_account_json),
    ),
    #(
      "storeCreditAccountTransactions",
      dict_to_json(
        state.store_credit_account_transactions,
        store_credit_account_transaction_json,
      ),
    ),
    #(
      "customerAccountPages",
      dict_to_json(state.customer_account_pages, customer_account_page_json),
    ),
    #(
      "customerAccountPageOrder",
      json.array(state.customer_account_page_order, json.string),
    ),
    #(
      "customerDataErasureRequests",
      dict_to_json(
        state.customer_data_erasure_requests,
        customer_data_erasure_request_json,
      ),
    ),
    #("mergedCustomerIds", dict_to_json(state.merged_customer_ids, json.string)),
    #(
      "customerMergeRequests",
      dict_to_json(state.customer_merge_requests, customer_merge_request_json),
    ),
    #("segments", dict_to_json(state.segments, segment_json)),
    #("segmentOrder", json.array(state.segment_order, json.string)),
    #("deletedSegmentIds", bool_dict_to_json(state.deleted_segment_ids)),
    #(
      "segmentRootPayloads",
      dict_to_json(state.segment_root_payloads, store_property_value_json),
    ),
    #(
      "customerSegmentMembersQueries",
      dict_to_json(
        state.customer_segment_members_queries,
        customer_segment_members_query_json,
      ),
    ),
    #(
      "customerSegmentMembersQueryOrder",
      json.array(state.customer_segment_members_query_order, json.string),
    ),
    #("availableLocales", json.array(state.available_locales, locale_json)),
    #("shopLocales", dict_to_json(state.shop_locales, shop_locale_json)),
    #("deletedShopLocales", json.object([])),
    #("translations", dict_to_json(state.translations, translation_json)),
    #("deletedTranslations", json.object([])),
  ]
}

pub fn strict_base_state_decoder() -> Decoder(store.BaseState) {
  use _ <- decode.then(require_object_fields(base_state_dump_field_names()))
  base_state_decoder()
}

pub fn base_state_decoder() -> Decoder(store.BaseState) {
  let empty = store.empty_base_state()
  use backup_region <- optional_field(
    "backupRegion",
    empty.backup_region,
    decode.optional(backup_region_decoder()),
  )
  use flow_signatures <- dict_field(
    "adminPlatformFlowSignatures",
    flow_signature_decoder(),
  )
  use flow_signature_order <- string_list_field(
    "adminPlatformFlowSignatureOrder",
  )
  use flow_triggers <- dict_field(
    "adminPlatformFlowTriggers",
    flow_trigger_decoder(),
  )
  use flow_trigger_order <- string_list_field("adminPlatformFlowTriggerOrder")
  use shop <- optional_field(
    "shop",
    empty.shop,
    decode.optional(shop_decoder()),
  )
  use abandoned_checkouts <- dict_field(
    "abandonedCheckouts",
    abandoned_checkout_decoder(),
  )
  use abandoned_checkout_order <- string_list_field("abandonedCheckoutOrder")
  use abandonments <- dict_field("abandonments", abandonment_decoder())
  use abandonment_order <- string_list_field("abandonmentOrder")
  use draft_orders <- dict_field("draftOrders", draft_order_decoder())
  use draft_order_order <- string_list_field("draftOrderOrder")
  use deleted_draft_order_ids <- bool_dict_field("deletedDraftOrderIds")
  use draft_order_variant_catalog <- dict_field(
    "draftOrderVariantCatalog",
    draft_order_variant_catalog_decoder(),
  )
  use orders <- dict_field("orders", order_decoder())
  use order_order <- string_list_field("orderOrder")
  use deleted_order_ids <- bool_dict_field("deletedOrderIds")
  use markets <- dict_field("markets", market_decoder())
  use market_order <- string_list_field("marketOrder")
  use deleted_market_ids <- bool_dict_field("deletedMarketIds")
  use catalogs <- dict_field("catalogs", catalog_decoder())
  use catalog_order <- string_list_field("catalogOrder")
  use deleted_catalog_ids <- bool_dict_field("deletedCatalogIds")
  use price_lists <- dict_field("priceLists", price_list_decoder())
  use price_list_order <- string_list_field("priceListOrder")
  use deleted_price_list_ids <- bool_dict_field("deletedPriceListIds")
  use web_presences <- dict_field("webPresences", web_presence_decoder())
  use web_presence_order <- string_list_field("webPresenceOrder")
  use deleted_web_presence_ids <- bool_dict_field("deletedWebPresenceIds")
  use market_localizations <- dict_field(
    "marketLocalizations",
    market_localization_decoder(),
  )
  use markets_root_payloads <- dict_field(
    "marketsRootPayloads",
    captured_json_value_decoder(),
  )
  use store_property_locations <- dict_field(
    "locations",
    store_property_record_decoder(),
  )
  use store_property_location_order <- string_list_field("locationOrder")
  use deleted_store_property_location_ids <- bool_dict_field(
    "deletedLocationIds",
  )
  use business_entities <- dict_field(
    "businessEntities",
    store_property_record_decoder(),
  )
  use business_entity_order <- string_list_field("businessEntityOrder")
  use publishables <- dict_field(
    "publishables",
    store_property_record_decoder(),
  )
  use publishable_order <- string_list_field("publishableOrder")
  use store_property_mutation_payloads <- dict_field(
    "storePropertyMutationPayloads",
    store_property_mutation_payload_decoder(),
  )
  use product_metafields <- dict_field(
    "productMetafields",
    product_metafield_decoder(),
  )
  use metafield_definitions <- dict_field(
    "metafieldDefinitions",
    metafield_definition_decoder(),
  )
  use deleted_metafield_definition_ids <- bool_dict_field(
    "deletedMetafieldDefinitionIds",
  )
  use saved_searches <- dict_field("savedSearches", saved_search_decoder())
  use saved_search_order <- string_list_field("savedSearchOrder")
  use deleted_saved_search_ids <- bool_dict_field("deletedSavedSearchIds")
  use webhook_subscriptions <- dict_field(
    "webhookSubscriptions",
    webhook_subscription_decoder(),
  )
  use webhook_subscription_order <- string_list_field(
    "webhookSubscriptionOrder",
  )
  use deleted_webhook_subscription_ids <- bool_dict_field(
    "deletedWebhookSubscriptionIds",
  )
  use apps <- dict_field("apps", app_decoder())
  use app_order <- string_list_field("appOrder")
  use app_installations <- dict_field(
    "appInstallations",
    app_installation_decoder(),
  )
  use app_installation_order <- string_list_field("appInstallationOrder")
  use current_installation_id <- optional_string_field(
    "currentAppInstallationId",
  )
  use app_subscriptions <- dict_field(
    "appSubscriptions",
    app_subscription_decoder(),
  )
  use app_subscription_order <- string_list_field("appSubscriptionOrder")
  use app_subscription_line_items <- dict_field(
    "appSubscriptionLineItems",
    app_subscription_line_item_decoder(),
  )
  use app_subscription_line_item_order <- string_list_field(
    "appSubscriptionLineItemOrder",
  )
  use app_one_time_purchases <- dict_field(
    "appOneTimePurchases",
    app_one_time_purchase_decoder(),
  )
  use app_one_time_purchase_order <- string_list_field(
    "appOneTimePurchaseOrder",
  )
  use app_usage_records <- dict_field("appUsageRecords", app_usage_decoder())
  use app_usage_record_order <- string_list_field("appUsageRecordOrder")
  use delegated_access_tokens <- dict_field(
    "delegatedAccessTokens",
    delegated_access_token_decoder(),
  )
  use delegated_access_token_order <- string_list_field(
    "delegatedAccessTokenOrder",
  )
  use shopify_functions <- dict_field(
    "shopifyFunctions",
    shopify_function_decoder(),
  )
  use shopify_function_order <- string_list_field("shopifyFunctionOrder")
  use bulk_operations <- dict_field("bulkOperations", bulk_operation_decoder())
  use bulk_operation_order <- string_list_field("bulkOperationOrder")
  use metaobject_definitions <- dict_field(
    "metaobjectDefinitions",
    metaobject_definition_decoder(),
  )
  use metaobject_definition_order <- string_list_field(
    "metaobjectDefinitionOrder",
  )
  use deleted_metaobject_definition_ids <- bool_dict_field(
    "deletedMetaobjectDefinitionIds",
  )
  use metaobjects <- dict_field("metaobjects", metaobject_decoder())
  use metaobject_order <- string_list_field("metaobjectOrder")
  use deleted_metaobject_ids <- bool_dict_field("deletedMetaobjectIds")
  use url_redirects <- dict_field("urlRedirects", url_redirect_decoder())
  use url_redirect_order <- string_list_field("urlRedirectOrder")
  use deleted_url_redirect_ids <- bool_dict_field("deletedUrlRedirectIds")
  use marketing_activities <- dict_field(
    "marketingActivities",
    marketing_record_decoder(),
  )
  use marketing_activity_order <- string_list_field("marketingActivityOrder")
  use marketing_events <- dict_field(
    "marketingEvents",
    marketing_record_decoder(),
  )
  use marketing_event_order <- string_list_field("marketingEventOrder")
  use marketing_channel_definitions <- dict_field(
    "marketingChannelDefinitions",
    marketing_channel_definition_decoder(),
  )
  use marketing_engagements <- dict_field(
    "marketingEngagements",
    marketing_engagement_decoder(),
  )
  use marketing_engagement_order <- string_list_field(
    "marketingEngagementOrder",
  )
  use deleted_marketing_activity_ids <- bool_dict_field(
    "deletedMarketingActivityIds",
  )
  use deleted_marketing_event_ids <- bool_dict_field("deletedMarketingEventIds")
  use deleted_marketing_engagement_ids <- bool_dict_field(
    "deletedMarketingEngagementIds",
  )
  use validations <- dict_field("validations", validation_decoder())
  use validation_order <- string_list_field("validationOrder")
  use deleted_validation_ids <- bool_dict_field("deletedValidationIds")
  use cart_transforms <- dict_field("cartTransforms", cart_transform_decoder())
  use cart_transform_order <- string_list_field("cartTransformOrder")
  use deleted_cart_transform_ids <- bool_dict_field("deletedCartTransformIds")
  use tax_app_configuration <- optional_field(
    "taxAppConfiguration",
    empty.tax_app_configuration,
    decode.optional(tax_app_configuration_decoder()),
  )
  use gift_cards <- dict_field("giftCards", gift_card_decoder())
  use gift_card_order <- string_list_field("giftCardOrder")
  use gift_card_configuration <- optional_field(
    "giftCardConfiguration",
    empty.gift_card_configuration,
    decode.optional(gift_card_configuration_decoder()),
  )
  use segments <- dict_field("segments", segment_decoder())
  use segment_order <- string_list_field("segmentOrder")
  use deleted_segment_ids <- bool_dict_field("deletedSegmentIds")
  use segment_root_payloads <- optional_field(
    "segmentRootPayloads",
    empty.segment_root_payloads,
    decode.dict(decode.string, store_property_value_decoder()),
  )
  use customer_segment_members_queries <- dict_field(
    "customerSegmentMembersQueries",
    customer_segment_members_query_decoder(),
  )
  use customer_segment_members_query_order <- string_list_field(
    "customerSegmentMembersQueryOrder",
  )
  use available_locales <- optional_field(
    "availableLocales",
    [],
    decode.list(of: locale_decoder()),
  )
  use shop_locales <- dict_field("shopLocales", shop_locale_decoder())
  use translations <- dict_field("translations", translation_decoder())
  use shipping_packages <- dict_field(
    "shippingPackages",
    shipping_package_decoder(),
  )
  use shipping_package_order <- string_list_field("shippingPackageOrder")
  use deleted_shipping_package_ids <- bool_dict_field(
    "deletedShippingPackageIds",
  )
  decode.success(store_types.BaseState(
    products: empty.products,
    product_order: empty.product_order,
    deleted_product_ids: empty.deleted_product_ids,
    product_count: empty.product_count,
    product_variants: empty.product_variants,
    product_variant_order: empty.product_variant_order,
    product_variant_count: empty.product_variant_count,
    product_options: empty.product_options,
    product_operations: empty.product_operations,
    selling_plan_groups: empty.selling_plan_groups,
    selling_plan_group_order: empty.selling_plan_group_order,
    deleted_selling_plan_group_ids: empty.deleted_selling_plan_group_ids,
    markets: markets,
    market_order: market_order,
    deleted_market_ids: deleted_market_ids,
    catalogs: catalogs,
    catalog_order: catalog_order,
    deleted_catalog_ids: deleted_catalog_ids,
    price_lists: price_lists,
    price_list_order: price_list_order,
    deleted_price_list_ids: deleted_price_list_ids,
    web_presences: web_presences,
    web_presence_order: web_presence_order,
    deleted_web_presence_ids: deleted_web_presence_ids,
    market_localizations: market_localizations,
    markets_root_payloads: markets_root_payloads,
    product_media: empty.product_media,
    files: empty.files,
    file_order: empty.file_order,
    deleted_file_ids: empty.deleted_file_ids,
    collections: empty.collections,
    collection_order: empty.collection_order,
    product_collections: empty.product_collections,
    deleted_collection_ids: empty.deleted_collection_ids,
    locations: empty.locations,
    location_order: empty.location_order,
    publications: empty.publications,
    publication_order: empty.publication_order,
    deleted_publication_ids: empty.deleted_publication_ids,
    channels: empty.channels,
    channel_order: empty.channel_order,
    product_feeds: empty.product_feeds,
    product_feed_order: empty.product_feed_order,
    deleted_product_feed_ids: empty.deleted_product_feed_ids,
    product_resource_feedback: empty.product_resource_feedback,
    shop_resource_feedback: empty.shop_resource_feedback,
    abandoned_checkouts: abandoned_checkouts,
    abandoned_checkout_order: abandoned_checkout_order,
    abandonments: abandonments,
    abandonment_order: abandonment_order,
    draft_orders: draft_orders,
    draft_order_order: draft_order_order,
    deleted_draft_order_ids: deleted_draft_order_ids,
    draft_order_variant_catalog: draft_order_variant_catalog,
    orders: orders,
    order_order: order_order,
    deleted_order_ids: deleted_order_ids,
    inventory_transfers: empty.inventory_transfers,
    inventory_transfer_order: empty.inventory_transfer_order,
    deleted_inventory_transfer_ids: empty.deleted_inventory_transfer_ids,
    inventory_shipments: empty.inventory_shipments,
    inventory_shipment_order: empty.inventory_shipment_order,
    deleted_inventory_shipment_ids: empty.deleted_inventory_shipment_ids,
    carrier_services: empty.carrier_services,
    carrier_service_order: empty.carrier_service_order,
    deleted_carrier_service_ids: empty.deleted_carrier_service_ids,
    fulfillment_services: empty.fulfillment_services,
    fulfillment_service_order: empty.fulfillment_service_order,
    deleted_fulfillment_service_ids: empty.deleted_fulfillment_service_ids,
    fulfillments: empty.fulfillments,
    fulfillment_order: empty.fulfillment_order,
    fulfillment_orders: empty.fulfillment_orders,
    fulfillment_order_order: empty.fulfillment_order_order,
    shipping_orders: empty.shipping_orders,
    reverse_fulfillment_orders: empty.reverse_fulfillment_orders,
    reverse_fulfillment_order_order: empty.reverse_fulfillment_order_order,
    reverse_deliveries: empty.reverse_deliveries,
    reverse_delivery_order: empty.reverse_delivery_order,
    calculated_orders: empty.calculated_orders,
    delivery_profiles: empty.delivery_profiles,
    delivery_profile_order: empty.delivery_profile_order,
    deleted_delivery_profile_ids: empty.deleted_delivery_profile_ids,
    shipping_packages: shipping_packages,
    shipping_package_order: shipping_package_order,
    deleted_shipping_package_ids: deleted_shipping_package_ids,
    backup_region: backup_region,
    admin_platform_generic_nodes: empty.admin_platform_generic_nodes,
    admin_platform_taxonomy_categories: empty.admin_platform_taxonomy_categories,
    admin_platform_taxonomy_category_order: empty.admin_platform_taxonomy_category_order,
    admin_platform_flow_signatures: flow_signatures,
    admin_platform_flow_signature_order: flow_signature_order,
    admin_platform_flow_triggers: flow_triggers,
    admin_platform_flow_trigger_order: flow_trigger_order,
    shop: shop,
    b2b_companies: empty.b2b_companies,
    b2b_company_order: empty.b2b_company_order,
    deleted_b2b_company_ids: empty.deleted_b2b_company_ids,
    b2b_company_contacts: empty.b2b_company_contacts,
    b2b_company_contact_order: empty.b2b_company_contact_order,
    deleted_b2b_company_contact_ids: empty.deleted_b2b_company_contact_ids,
    b2b_company_contact_roles: empty.b2b_company_contact_roles,
    b2b_company_contact_role_order: empty.b2b_company_contact_role_order,
    deleted_b2b_company_contact_role_ids: empty.deleted_b2b_company_contact_role_ids,
    b2b_company_locations: empty.b2b_company_locations,
    b2b_company_location_order: empty.b2b_company_location_order,
    deleted_b2b_company_location_ids: empty.deleted_b2b_company_location_ids,
    store_property_locations: store_property_locations,
    store_property_location_order: store_property_location_order,
    deleted_store_property_location_ids: deleted_store_property_location_ids,
    business_entities: business_entities,
    business_entity_order: business_entity_order,
    publishables: publishables,
    publishable_order: publishable_order,
    store_property_mutation_payloads: store_property_mutation_payloads,
    product_metafields: product_metafields,
    metafield_definitions: metafield_definitions,
    deleted_metafield_definition_ids: deleted_metafield_definition_ids,
    saved_searches: saved_searches,
    saved_search_order: saved_search_order,
    deleted_saved_search_ids: deleted_saved_search_ids,
    webhook_subscriptions: webhook_subscriptions,
    webhook_subscription_order: webhook_subscription_order,
    deleted_webhook_subscription_ids: deleted_webhook_subscription_ids,
    online_store_content: empty.online_store_content,
    online_store_content_order: empty.online_store_content_order,
    deleted_online_store_content_ids: empty.deleted_online_store_content_ids,
    online_store_integrations: empty.online_store_integrations,
    online_store_integration_order: empty.online_store_integration_order,
    deleted_online_store_integration_ids: empty.deleted_online_store_integration_ids,
    apps: apps,
    app_order: app_order,
    app_installations: app_installations,
    app_installation_order: app_installation_order,
    current_installation_id: current_installation_id,
    app_subscriptions: app_subscriptions,
    app_subscription_order: app_subscription_order,
    app_subscription_line_items: app_subscription_line_items,
    app_subscription_line_item_order: app_subscription_line_item_order,
    app_one_time_purchases: app_one_time_purchases,
    app_one_time_purchase_order: app_one_time_purchase_order,
    app_usage_records: app_usage_records,
    app_usage_record_order: app_usage_record_order,
    delegated_access_tokens: delegated_access_tokens,
    delegated_access_token_order: delegated_access_token_order,
    shopify_functions: shopify_functions,
    shopify_function_order: shopify_function_order,
    bulk_operations: bulk_operations,
    bulk_operation_order: bulk_operation_order,
    metaobject_definitions: metaobject_definitions,
    metaobject_definition_order: metaobject_definition_order,
    deleted_metaobject_definition_ids: deleted_metaobject_definition_ids,
    metaobjects: metaobjects,
    metaobject_order: metaobject_order,
    deleted_metaobject_ids: deleted_metaobject_ids,
    url_redirects: url_redirects,
    url_redirect_order: url_redirect_order,
    deleted_url_redirect_ids: deleted_url_redirect_ids,
    marketing_activities: marketing_activities,
    marketing_activity_order: marketing_activity_order,
    marketing_events: marketing_events,
    marketing_event_order: marketing_event_order,
    marketing_channel_definitions: marketing_channel_definitions,
    marketing_engagements: marketing_engagements,
    marketing_engagement_order: marketing_engagement_order,
    deleted_marketing_activity_ids: deleted_marketing_activity_ids,
    deleted_marketing_event_ids: deleted_marketing_event_ids,
    deleted_marketing_engagement_ids: deleted_marketing_engagement_ids,
    validations: validations,
    validation_order: validation_order,
    deleted_validation_ids: deleted_validation_ids,
    cart_transforms: cart_transforms,
    cart_transform_order: cart_transform_order,
    deleted_cart_transform_ids: deleted_cart_transform_ids,
    tax_app_configuration: tax_app_configuration,
    discounts: empty.discounts,
    discount_order: empty.discount_order,
    deleted_discount_ids: empty.deleted_discount_ids,
    discount_bulk_operations: empty.discount_bulk_operations,
    gift_cards: gift_cards,
    gift_card_order: gift_card_order,
    gift_card_configuration: gift_card_configuration,
    customers: empty.customers,
    customer_order: empty.customer_order,
    customer_catalog_connections: empty.customer_catalog_connections,
    deleted_customer_ids: empty.deleted_customer_ids,
    customer_addresses: empty.customer_addresses,
    customer_address_order: empty.customer_address_order,
    deleted_customer_address_ids: empty.deleted_customer_address_ids,
    customer_order_summaries: empty.customer_order_summaries,
    customer_order_connection_page_infos: empty.customer_order_connection_page_infos,
    customer_event_summaries: empty.customer_event_summaries,
    customer_event_connection_page_infos: empty.customer_event_connection_page_infos,
    customer_last_orders: empty.customer_last_orders,
    customer_metafields: empty.customer_metafields,
    customer_payment_methods: empty.customer_payment_methods,
    customer_payment_method_update_urls: empty.customer_payment_method_update_urls,
    deleted_customer_payment_method_ids: empty.deleted_customer_payment_method_ids,
    payment_reminder_sends: empty.payment_reminder_sends,
    payment_customizations: empty.payment_customizations,
    payment_customization_order: empty.payment_customization_order,
    deleted_payment_customization_ids: empty.deleted_payment_customization_ids,
    payment_terms: empty.payment_terms,
    payment_terms_owner_ids: empty.payment_terms_owner_ids,
    payment_terms_by_owner_id: empty.payment_terms_by_owner_id,
    deleted_payment_terms_ids: empty.deleted_payment_terms_ids,
    order_mandate_payments: empty.order_mandate_payments,
    store_credit_accounts: empty.store_credit_accounts,
    store_credit_account_transactions: empty.store_credit_account_transactions,
    customer_account_pages: empty.customer_account_pages,
    customer_account_page_order: empty.customer_account_page_order,
    customer_data_erasure_requests: empty.customer_data_erasure_requests,
    merged_customer_ids: empty.merged_customer_ids,
    customer_merge_requests: empty.customer_merge_requests,
    segments: segments,
    segment_order: segment_order,
    deleted_segment_ids: deleted_segment_ids,
    segment_root_payloads: segment_root_payloads,
    customer_segment_members_queries: customer_segment_members_queries,
    customer_segment_members_query_order: customer_segment_members_query_order,
    available_locales: available_locales,
    shop_locales: shop_locales,
    translations: translations,
  ))
}

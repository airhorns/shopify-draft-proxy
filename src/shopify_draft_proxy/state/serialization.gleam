import gleam/dict.{type Dict}
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types

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

pub fn serialize_staged_state(state: store.StagedState) -> Json {
  json.object(staged_state_dump_fields(state))
}

pub fn staged_state_dump_field_names() -> List(String) {
  staged_state_dump_fields(store.empty_staged_state())
  |> dump_field_names
}

fn staged_state_dump_fields(state: store.StagedState) -> List(#(String, Json)) {
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
      "deletedOnlineStoreArticleIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_content_ids,
        "Article",
      ),
    ),
    #(
      "deletedOnlineStoreBlogIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_content_ids,
        "Blog",
      ),
    ),
    #(
      "deletedOnlineStorePageIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_content_ids,
        "Page",
      ),
    ),
    #(
      "deletedOnlineStoreCommentIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_content_ids,
        "Comment",
      ),
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
    #(
      "deletedOnlineStoreThemeIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "OnlineStoreTheme",
      ),
    ),
    #(
      "deletedOnlineStoreScriptTagIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "ScriptTag",
      ),
    ),
    #(
      "deletedOnlineStoreWebPixelIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "WebPixel",
      ),
    ),
    #(
      "deletedOnlineStoreServerPixelIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "ServerPixel",
      ),
    ),
    #(
      "deletedOnlineStoreStorefrontAccessTokenIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "StorefrontAccessToken",
      ),
    ),
    #(
      "deletedOnlineStoreMobilePlatformApplicationIds",
      deleted_online_store_ids_json(
        state.deleted_online_store_integration_ids,
        "MobilePlatformApplication",
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
    #("availableLocales", json.array([], json.string)),
    #("shopLocales", dict_to_json(state.shop_locales, shop_locale_json)),
    #("deletedShopLocales", bool_dict_to_json(state.deleted_shop_locales)),
    #("translations", dict_to_json(state.translations, translation_json)),
    #("deletedTranslations", bool_dict_to_json(state.deleted_translations)),
  ]
}

fn dump_field_names(fields: List(#(String, Json))) -> List(String) {
  list.map(fields, fn(field) {
    let #(name, _) = field
    name
  })
}

fn optional_to_json(value: Option(a), encode: fn(a) -> Json) -> Json {
  case value {
    Some(inner) -> encode(inner)
    None -> json.null()
  }
}

fn optional_string(value: Option(String)) -> Json {
  optional_to_json(value, json.string)
}

fn optional_int(value: Option(Int)) -> Json {
  optional_to_json(value, json.int)
}

fn optional_float(value: Option(Float)) -> Json {
  optional_to_json(value, json.float)
}

fn optional_bool(value: Option(Bool)) -> Json {
  optional_to_json(value, json.bool)
}

fn file_json(record: types.FileRecord) -> Json {
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

fn dict_to_json(records: Dict(String, a), encode: fn(a) -> Json) -> Json {
  json.object(
    dict.to_list(records)
    |> list.map(fn(pair) {
      let #(key, value) = pair
      #(key, encode(value))
    }),
  )
}

fn bool_dict_to_json(records: Dict(String, Bool)) -> Json {
  dict_to_json(records, json.bool)
}

fn backup_region_json(record: types.BackupRegionRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("code", json.string(record.code)),
  ])
}

fn admin_platform_generic_node_json(
  record: types.AdminPlatformGenericNodeRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("typename", json.string(record.typename)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn admin_platform_taxonomy_category_json(
  record: types.AdminPlatformTaxonomyCategoryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn flow_signature_json(record: types.AdminPlatformFlowSignatureRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("flowTriggerId", json.string(record.flow_trigger_id)),
    #("payloadSha256", json.string(record.payload_sha256)),
    #("signatureSha256", json.string(record.signature_sha256)),
    #("createdAt", json.string(record.created_at)),
  ])
}

fn flow_trigger_json(record: types.AdminPlatformFlowTriggerRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("handle", json.string(record.handle)),
    #("payloadBytes", json.int(record.payload_bytes)),
    #("payloadSha256", json.string(record.payload_sha256)),
    #("receivedAt", json.string(record.received_at)),
  ])
}

fn shop_json(record: types.ShopRecord) -> Json {
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

fn shop_domain_json(record: types.ShopDomainRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("host", json.string(record.host)),
    #("url", json.string(record.url)),
    #("sslEnabled", json.bool(record.ssl_enabled)),
  ])
}

fn shop_address_json(record: types.ShopAddressRecord) -> Json {
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

fn shop_plan_json(record: types.ShopPlanRecord) -> Json {
  json.object([
    #("partnerDevelopment", json.bool(record.partner_development)),
    #("publicDisplayName", json.string(record.public_display_name)),
    #("shopifyPlus", json.bool(record.shopify_plus)),
  ])
}

fn shop_resource_limits_json(record: types.ShopResourceLimitsRecord) -> Json {
  json.object([
    #("locationLimit", json.int(record.location_limit)),
    #("maxProductOptions", json.int(record.max_product_options)),
    #("maxProductVariants", json.int(record.max_product_variants)),
    #("redirectLimitReached", json.bool(record.redirect_limit_reached)),
  ])
}

fn shop_features_json(record: types.ShopFeaturesRecord) -> Json {
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
    #("sellsSubscriptions", json.bool(record.sells_subscriptions)),
    #("showMetrics", json.bool(record.show_metrics)),
    #("storefront", json.bool(record.storefront)),
    #("unifiedMarkets", json.bool(record.unified_markets)),
  ])
}

fn shop_bundles_feature_json(record: types.ShopBundlesFeatureRecord) -> Json {
  json.object([
    #("eligibleForBundles", json.bool(record.eligible_for_bundles)),
    #("ineligibilityReason", optional_string(record.ineligibility_reason)),
    #("sellsBundles", json.bool(record.sells_bundles)),
  ])
}

fn shop_cart_transform_feature_json(
  record: types.ShopCartTransformFeatureRecord,
) -> Json {
  json.object([
    #(
      "eligibleOperations",
      shop_cart_transform_eligible_operations_json(record.eligible_operations),
    ),
  ])
}

fn shop_cart_transform_eligible_operations_json(
  record: types.ShopCartTransformEligibleOperationsRecord,
) -> Json {
  json.object([
    #("expandOperation", json.bool(record.expand_operation)),
    #("mergeOperation", json.bool(record.merge_operation)),
    #("updateOperation", json.bool(record.update_operation)),
  ])
}

fn payment_settings_json(record: types.PaymentSettingsRecord) -> Json {
  json.object([
    #(
      "supportedDigitalWallets",
      json.array(record.supported_digital_wallets, json.string),
    ),
  ])
}

fn shop_policy_json(record: types.ShopPolicyRecord) -> Json {
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

fn b2b_company_json(record: types.B2BCompanyRecord) -> Json {
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

fn b2b_company_contact_json(record: types.B2BCompanyContactRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

fn b2b_company_contact_role_json(
  record: types.B2BCompanyContactRoleRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

fn b2b_company_location_json(record: types.B2BCompanyLocationRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("companyId", json.string(record.company_id)),
    #("data", store_property_data_json(record.data)),
  ])
}

fn store_property_record_json(record: types.StorePropertyRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", store_property_data_json(record.data)),
  ])
}

fn store_property_mutation_payload_json(
  record: types.StorePropertyMutationPayloadRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("data", store_property_data_json(record.data)),
  ])
}

fn store_property_data_json(
  data: Dict(String, types.StorePropertyValue),
) -> Json {
  dict_to_json(data, store_property_value_json)
}

fn store_property_value_json(value: types.StorePropertyValue) -> Json {
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

fn product_json(record: types.ProductRecord) -> Json {
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
  ])
}

fn product_variant_json(record: types.ProductVariantRecord) -> Json {
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

fn selling_plan_group_json(record: types.SellingPlanGroupRecord) -> Json {
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

fn selling_plan_json(record: types.SellingPlanRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn delivery_profile_json(record: types.DeliveryProfileRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("merchantOwned", json.bool(record.merchant_owned)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn market_json(record: types.MarketRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn catalog_json(record: types.CatalogRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn price_list_json(record: types.PriceListRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn web_presence_json(record: types.WebPresenceRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn market_localization_json(record: types.MarketLocalizationRecord) -> Json {
  json.object([
    #("resourceId", json.string(record.resource_id)),
    #("marketId", json.string(record.market_id)),
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
    #("updatedAt", json.string(record.updated_at)),
    #("outdated", json.bool(record.outdated)),
  ])
}

fn market_localizable_content_json(
  record: types.MarketLocalizableContentRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
    #("digest", json.string(record.digest)),
  ])
}

fn selected_option_json(
  record: types.ProductVariantSelectedOptionRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", json.string(record.value)),
  ])
}

fn product_seo_json(record: types.ProductSeoRecord) -> Json {
  json.object([
    #("title", optional_string(record.title)),
    #("description", optional_string(record.description)),
  ])
}

fn product_category_json(record: types.ProductCategoryRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("fullName", json.string(record.full_name)),
  ])
}

fn abandoned_checkout_json(record: types.AbandonedCheckoutRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn abandonment_delivery_activity_json(
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

fn abandonment_json(record: types.AbandonmentRecord) -> Json {
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

fn draft_order_json(record: types.DraftOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn order_json(record: types.OrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn draft_order_variant_catalog_json(
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

fn captured_json_value_json(value: types.CapturedJsonValue) -> Json {
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

fn optional_string_value(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

fn discount_json(record: types.DiscountRecord) -> Json {
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

fn discount_bulk_operation_json(
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

fn saved_search_json(record: types.SavedSearchRecord) -> Json {
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

fn saved_search_filter_json(record: types.SavedSearchFilter) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("value", json.string(record.value)),
  ])
}

fn webhook_subscription_json(record: types.WebhookSubscriptionRecord) -> Json {
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

fn webhook_endpoint_json(record: types.WebhookSubscriptionEndpoint) -> Json {
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

fn online_store_content_kind_json(
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

fn online_store_content_json(record: types.OnlineStoreContentRecord) -> Json {
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

fn online_store_integration_kind_json(
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

fn online_store_integration_json(
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

fn online_store_integration_data(
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

fn deleted_online_store_ids_json(
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

fn string_contains_gid_type(id: String, gid_type: String) -> Bool {
  string.contains(id, "gid://shopify/" <> gid_type <> "/")
}

fn money_json(record: types.Money) -> Json {
  json.object([
    #("amount", json.string(record.amount)),
    #("currencyCode", json.string(record.currency_code)),
  ])
}

fn access_scope_json(record: types.AccessScopeRecord) -> Json {
  json.object([
    #("handle", json.string(record.handle)),
    #("description", optional_string(record.description)),
  ])
}

fn app_json(record: types.AppRecord) -> Json {
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

fn app_subscription_pricing_json(record: types.AppSubscriptionPricing) -> Json {
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

fn app_subscription_line_item_plan_json(
  record: types.AppSubscriptionLineItemPlan,
) -> Json {
  json.object([
    #("pricingDetails", app_subscription_pricing_json(record.pricing_details)),
  ])
}

fn app_subscription_line_item_json(
  record: types.AppSubscriptionLineItemRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("subscriptionId", json.string(record.subscription_id)),
    #("plan", app_subscription_line_item_plan_json(record.plan)),
  ])
}

fn app_subscription_json(record: types.AppSubscriptionRecord) -> Json {
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

fn app_one_time_purchase_json(record: types.AppOneTimePurchaseRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", json.string(record.name)),
    #("status", json.string(record.status)),
    #("isTest", json.bool(record.is_test)),
    #("createdAt", json.string(record.created_at)),
    #("price", money_json(record.price)),
  ])
}

fn app_usage_json(record: types.AppUsageRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("subscriptionLineItemId", json.string(record.subscription_line_item_id)),
    #("description", json.string(record.description)),
    #("price", money_json(record.price)),
    #("createdAt", json.string(record.created_at)),
    #("idempotencyKey", optional_string(record.idempotency_key)),
  ])
}

fn delegated_access_token_json(
  record: types.DelegatedAccessTokenRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("accessTokenSha256", json.string(record.access_token_sha256)),
    #("accessTokenPreview", json.string(record.access_token_preview)),
    #("accessScopes", json.array(record.access_scopes, json.string)),
    #("createdAt", json.string(record.created_at)),
    #("expiresIn", optional_int(record.expires_in)),
    #("destroyedAt", optional_string(record.destroyed_at)),
  ])
}

fn app_installation_json(record: types.AppInstallationRecord) -> Json {
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

fn shopify_function_app_json(record: types.ShopifyFunctionAppRecord) -> Json {
  json.object([
    #("__typename", optional_string(record.typename)),
    #("id", optional_string(record.id)),
    #("title", optional_string(record.title)),
    #("handle", optional_string(record.handle)),
    #("apiKey", optional_string(record.api_key)),
  ])
}

fn shopify_function_json(record: types.ShopifyFunctionRecord) -> Json {
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

fn bulk_operation_json(record: types.BulkOperationRecord) -> Json {
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

fn product_metafield_json(record: types.ProductMetafieldRecord) -> Json {
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

fn metafield_definition_json(record: types.MetafieldDefinitionRecord) -> Json {
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

fn metafield_definition_type_json(
  record: types.MetafieldDefinitionTypeRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("category", optional_string(record.category)),
  ])
}

fn metafield_definition_validation_json(
  record: types.MetafieldDefinitionValidationRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", optional_string(record.value)),
  ])
}

fn metafield_definition_capabilities_json(
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

fn metafield_definition_capability_json(
  record: types.MetafieldDefinitionCapabilityRecord,
) -> Json {
  json.object([
    #("enabled", json.bool(record.enabled)),
    #("eligible", json.bool(record.eligible)),
    #("status", optional_string(record.status)),
  ])
}

fn metafield_definition_constraints_json(
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

fn metafield_definition_constraint_value_json(
  record: types.MetafieldDefinitionConstraintValueRecord,
) -> Json {
  json.object([#("value", json.string(record.value))])
}

fn metaobject_definition_json(
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

fn metaobject_definition_capabilities_json(
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

fn metaobject_definition_capability_json(
  record: types.MetaobjectDefinitionCapabilityRecord,
) -> Json {
  json.object([#("enabled", json.bool(record.enabled))])
}

fn metaobject_definition_type_json(
  record: types.MetaobjectDefinitionTypeRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("category", optional_string(record.category)),
  ])
}

fn metaobject_field_definition_json(
  record: types.MetaobjectFieldDefinitionRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("name", optional_string(record.name)),
    #("description", optional_string(record.description)),
    #("required", optional_bool(record.required)),
    #("type", metaobject_definition_type_json(record.type_)),
    #(
      "validations",
      json.array(
        record.validations,
        metaobject_field_definition_validation_json,
      ),
    ),
  ])
}

fn metaobject_field_definition_validation_json(
  record: types.MetaobjectFieldDefinitionValidationRecord,
) -> Json {
  json.object([
    #("name", json.string(record.name)),
    #("value", optional_string(record.value)),
  ])
}

fn metaobject_standard_template_json(
  record: types.MetaobjectStandardTemplateRecord,
) -> Json {
  json.object([
    #("type", optional_string(record.type_)),
    #("name", optional_string(record.name)),
  ])
}

fn metaobject_json(record: types.MetaobjectRecord) -> Json {
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

fn metaobject_field_json(record: types.MetaobjectFieldRecord) -> Json {
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

fn metaobject_json_value_json(value: types.MetaobjectJsonValue) -> Json {
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

fn metaobject_field_definition_ref_json(
  record: types.MetaobjectFieldDefinitionReferenceRecord,
) -> Json {
  json.object([
    #("key", json.string(record.key)),
    #("name", optional_string(record.name)),
    #("required", optional_bool(record.required)),
    #("type", metaobject_definition_type_json(record.type_)),
  ])
}

fn metaobject_capabilities_json(
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

fn metaobject_publishable_json(
  record: types.MetaobjectPublishableCapabilityRecord,
) -> Json {
  json.object([#("status", optional_string(record.status))])
}

fn metaobject_online_store_json(
  record: types.MetaobjectOnlineStoreCapabilityRecord,
) -> Json {
  json.object([#("templateSuffix", optional_string(record.template_suffix))])
}

fn marketing_record_json(record: types.MarketingRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", marketing_object_json(record.data)),
  ])
}

fn marketing_engagement_json(record: types.MarketingEngagementRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("marketingActivityId", optional_string(record.marketing_activity_id)),
    #("remoteId", optional_string(record.remote_id)),
    #("channelHandle", optional_string(record.channel_handle)),
    #("occurredOn", json.string(record.occurred_on)),
    #("data", marketing_object_json(record.data)),
  ])
}

fn marketing_object_json(data: Dict(String, types.MarketingValue)) -> Json {
  dict_to_json(data, marketing_value_json)
}

fn marketing_value_json(value: types.MarketingValue) -> Json {
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

fn validation_json(record: types.ValidationRecord) -> Json {
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

fn validation_metafield_json(record: types.ValidationMetafieldRecord) -> Json {
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

fn cart_transform_json(record: types.CartTransformRecord) -> Json {
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

fn tax_app_configuration_json(record: types.TaxAppConfigurationRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("ready", json.bool(record.ready)),
    #("state", json.string(record.state)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

fn shipping_package_weight_json(
  record: types.ShippingPackageWeightRecord,
) -> Json {
  json.object([
    #("value", optional_float(record.value)),
    #("unit", optional_string(record.unit)),
  ])
}

fn shipping_package_dimensions_json(
  record: types.ShippingPackageDimensionsRecord,
) -> Json {
  json.object([
    #("length", optional_float(record.length)),
    #("width", optional_float(record.width)),
    #("height", optional_float(record.height)),
    #("unit", optional_string(record.unit)),
  ])
}

fn shipping_package_json(record: types.ShippingPackageRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", optional_string(record.name)),
    #("type", optional_string(record.type_)),
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

fn carrier_service_json(record: types.CarrierServiceRecord) -> Json {
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

fn fulfillment_service_json(record: types.FulfillmentServiceRecord) -> Json {
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

fn fulfillment_json(record: types.FulfillmentRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("orderId", optional_string(record.order_id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn fulfillment_order_json(record: types.FulfillmentOrderRecord) -> Json {
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

fn shipping_order_json(record: types.ShippingOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn reverse_fulfillment_order_json(
  record: types.ReverseFulfillmentOrderRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn reverse_delivery_json(record: types.ReverseDeliveryRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #(
      "reverseFulfillmentOrderId",
      json.string(record.reverse_fulfillment_order_id),
    ),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn calculated_order_json(record: types.CalculatedOrderRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("data", captured_json_value_json(record.data)),
  ])
}

fn gift_card_transaction_json(record: types.GiftCardTransactionRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("kind", json.string(record.kind)),
    #("amount", money_json(record.amount)),
    #("processedAt", json.string(record.processed_at)),
    #("note", optional_string(record.note)),
  ])
}

fn gift_card_recipient_attributes_json(
  record: types.GiftCardRecipientAttributesRecord,
) -> Json {
  json.object([
    #("id", optional_string(record.id)),
    #("message", optional_string(record.message)),
    #("preferredName", optional_string(record.preferred_name)),
    #("sendNotificationAt", optional_string(record.send_notification_at)),
  ])
}

fn gift_card_json(record: types.GiftCardRecord) -> Json {
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

fn gift_card_configuration_json(
  record: types.GiftCardConfigurationRecord,
) -> Json {
  json.object([
    #("issueLimit", money_json(record.issue_limit)),
    #("purchaseLimit", money_json(record.purchase_limit)),
  ])
}

fn segment_json(record: types.SegmentRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("name", optional_string(record.name)),
    #("query", optional_string(record.query)),
    #("creationDate", optional_string(record.creation_date)),
    #("lastEditDate", optional_string(record.last_edit_date)),
  ])
}

fn customer_segment_members_query_json(
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

fn customer_json(record: types.CustomerRecord) -> Json {
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
    #("createdAt", optional_string(record.created_at)),
    #("updatedAt", optional_string(record.updated_at)),
  ])
}

fn customer_default_email_address_json(
  record: types.CustomerDefaultEmailAddressRecord,
) -> Json {
  json.object([
    #("emailAddress", optional_string(record.email_address)),
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("marketingUpdatedAt", optional_string(record.marketing_updated_at)),
  ])
}

fn customer_default_phone_number_json(
  record: types.CustomerDefaultPhoneNumberRecord,
) -> Json {
  json.object([
    #("phoneNumber", optional_string(record.phone_number)),
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("marketingUpdatedAt", optional_string(record.marketing_updated_at)),
  ])
}

fn customer_email_marketing_consent_json(
  record: types.CustomerEmailMarketingConsentRecord,
) -> Json {
  json.object([
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("consentUpdatedAt", optional_string(record.consent_updated_at)),
  ])
}

fn customer_sms_marketing_consent_json(
  record: types.CustomerSmsMarketingConsentRecord,
) -> Json {
  json.object([
    #("marketingState", optional_string(record.marketing_state)),
    #("marketingOptInLevel", optional_string(record.marketing_opt_in_level)),
    #("consentUpdatedAt", optional_string(record.consent_updated_at)),
    #("consentCollectedFrom", optional_string(record.consent_collected_from)),
  ])
}

fn customer_default_address_json(
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

fn customer_address_json(record: types.CustomerAddressRecord) -> Json {
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

fn customer_catalog_connection_json(
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

fn customer_catalog_page_info_json(
  record: types.CustomerCatalogPageInfoRecord,
) -> Json {
  json.object([
    #("hasNextPage", json.bool(record.has_next_page)),
    #("hasPreviousPage", json.bool(record.has_previous_page)),
    #("startCursor", optional_string(record.start_cursor)),
    #("endCursor", optional_string(record.end_cursor)),
  ])
}

fn customer_order_summary_json(
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

fn customer_event_summary_json(
  record: types.CustomerEventSummaryRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
  ])
}

fn customer_metafield_json(record: types.CustomerMetafieldRecord) -> Json {
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

fn customer_payment_method_json(
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

fn customer_payment_method_instrument_json(
  record: types.CustomerPaymentMethodInstrumentRecord,
) -> Json {
  json.object([
    #("__typename", json.string(record.type_name)),
    #("data", dict_to_json(record.data, json.string)),
  ])
}

fn customer_payment_method_subscription_contract_json(
  record: types.CustomerPaymentMethodSubscriptionContractRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("cursor", optional_string(record.cursor)),
    #("data", dict_to_json(record.data, json.string)),
  ])
}

fn customer_payment_method_update_url_json(
  record: types.CustomerPaymentMethodUpdateUrlRecord,
) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerPaymentMethodId", json.string(record.customer_payment_method_id)),
    #("updatePaymentMethodUrl", json.string(record.update_payment_method_url)),
    #("createdAt", json.string(record.created_at)),
  ])
}

fn payment_reminder_send_json(record: types.PaymentReminderSendRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("paymentScheduleId", json.string(record.payment_schedule_id)),
    #("sentAt", json.string(record.sent_at)),
  ])
}

fn payment_customization_json(
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

fn payment_customization_metafield_json(
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

fn payment_schedule_json(record: types.PaymentScheduleRecord) -> Json {
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

fn payment_terms_json(record: types.PaymentTermsRecord) -> Json {
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

fn order_mandate_payment_json(record: types.OrderMandatePaymentRecord) -> Json {
  json.object([
    #("orderId", json.string(record.order_id)),
    #("idempotencyKey", json.string(record.idempotency_key)),
    #("jobId", json.string(record.job_id)),
    #("paymentReferenceId", json.string(record.payment_reference_id)),
    #("transactionId", json.string(record.transaction_id)),
  ])
}

fn store_credit_account_json(record: types.StoreCreditAccountRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("customerId", json.string(record.customer_id)),
    #("cursor", optional_string(record.cursor)),
    #("balance", money_json(record.balance)),
  ])
}

fn store_credit_account_transaction_json(
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

fn customer_account_page_json(record: types.CustomerAccountPageRecord) -> Json {
  json.object([
    #("id", json.string(record.id)),
    #("title", json.string(record.title)),
    #("handle", json.string(record.handle)),
    #("defaultCursor", json.string(record.default_cursor)),
    #("cursor", optional_string(record.cursor)),
  ])
}

fn customer_data_erasure_request_json(
  record: types.CustomerDataErasureRequestRecord,
) -> Json {
  json.object([
    #("customerId", json.string(record.customer_id)),
    #("requestedAt", json.string(record.requested_at)),
    #("canceledAt", optional_string(record.canceled_at)),
  ])
}

fn customer_merge_request_json(
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

fn customer_merge_error_json(record: types.CustomerMergeErrorRecord) -> Json {
  json.object([
    #("errorFields", json.array(record.error_fields, json.string)),
    #("message", json.string(record.message)),
  ])
}

fn locale_json(record: types.LocaleRecord) -> Json {
  json.object([
    #("isoCode", json.string(record.iso_code)),
    #("name", json.string(record.name)),
  ])
}

fn shop_locale_json(record: types.ShopLocaleRecord) -> Json {
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

fn translation_json(record: types.TranslationRecord) -> Json {
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
  decode.success(store.BaseState(
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
    shipping_packages: empty.shipping_packages,
    shipping_package_order: empty.shipping_package_order,
    deleted_shipping_package_ids: empty.deleted_shipping_package_ids,
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
    marketing_activities: marketing_activities,
    marketing_activity_order: marketing_activity_order,
    marketing_events: marketing_events,
    marketing_event_order: marketing_event_order,
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

pub fn strict_staged_state_decoder() -> Decoder(store.StagedState) {
  use _ <- decode.then(require_object_fields(staged_state_dump_field_names()))
  staged_state_decoder()
}

pub fn staged_state_decoder() -> Decoder(store.StagedState) {
  let empty = store.empty_staged_state()
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
  use customer_segment_members_queries <- dict_field(
    "customerSegmentMembersQueries",
    customer_segment_members_query_decoder(),
  )
  use customer_segment_members_query_order <- string_list_field(
    "customerSegmentMembersQueryOrder",
  )
  use shop_locales <- dict_field("shopLocales", shop_locale_decoder())
  use deleted_shop_locales <- bool_dict_field("deletedShopLocales")
  use translations <- dict_field("translations", translation_decoder())
  use deleted_translations <- bool_dict_field("deletedTranslations")
  decode.success(store.StagedState(
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
    staged_product_collection_families: empty.staged_product_collection_families,
    deleted_collection_ids: empty.deleted_collection_ids,
    publications: empty.publications,
    publication_order: empty.publication_order,
    deleted_publication_ids: empty.deleted_publication_ids,
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
    shipping_packages: empty.shipping_packages,
    shipping_package_order: empty.shipping_package_order,
    deleted_shipping_package_ids: empty.deleted_shipping_package_ids,
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
    staged_upload_contents: empty.staged_upload_contents,
    metaobject_definitions: metaobject_definitions,
    metaobject_definition_order: metaobject_definition_order,
    deleted_metaobject_definition_ids: deleted_metaobject_definition_ids,
    metaobjects: metaobjects,
    metaobject_order: metaobject_order,
    deleted_metaobject_ids: deleted_metaobject_ids,
    marketing_activities: marketing_activities,
    marketing_activity_order: marketing_activity_order,
    marketing_events: marketing_events,
    marketing_event_order: marketing_event_order,
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
    customer_segment_members_queries: customer_segment_members_queries,
    customer_segment_members_query_order: customer_segment_members_query_order,
    shop_locales: shop_locales,
    deleted_shop_locales: deleted_shop_locales,
    translations: translations,
    deleted_translations: deleted_translations,
  ))
}

fn optional_field(
  name: String,
  default: a,
  decoder: Decoder(a),
  next: fn(a) -> Decoder(b),
) -> Decoder(b) {
  decode.optional_field(name, default, decoder, next)
}

fn optional_string_field(
  name: String,
  next: fn(Option(String)) -> Decoder(a),
) -> Decoder(a) {
  optional_field(name, None, decode.optional(decode.string), next)
}

fn string_list_field(
  name: String,
  next: fn(List(String)) -> Decoder(a),
) -> Decoder(a) {
  optional_field(name, [], decode.list(of: decode.string), next)
}

fn dict_field(
  name: String,
  item_decoder: Decoder(a),
  next: fn(Dict(String, a)) -> Decoder(b),
) -> Decoder(b) {
  optional_field(
    name,
    dict.new(),
    decode.dict(decode.string, item_decoder),
    next,
  )
}

fn bool_dict_field(
  name: String,
  next: fn(Dict(String, Bool)) -> Decoder(a),
) -> Decoder(a) {
  dict_field(name, decode.bool, next)
}

fn require_object_fields(names: List(String)) -> Decoder(Nil) {
  list.fold(names, decode.success(Nil), fn(decoder, name) {
    use _ <- decode.then(decoder)
    use _ <- decode.field(name, decode.dynamic)
    decode.success(Nil)
  })
}

fn runtime_json_decoder() -> Decoder(Json) {
  decode.dynamic |> decode.map(runtime_json_from_dynamic)
}

fn captured_json_value_decoder() -> Decoder(types.CapturedJsonValue) {
  decode.dynamic |> decode.map(captured_json_value_from_dynamic)
}

fn captured_json_value_from_dynamic(value: Dynamic) -> types.CapturedJsonValue {
  case decode.run(value, decode.bool) {
    Ok(b) -> types.CapturedBool(b)
    Error(_) -> captured_json_value_from_non_bool_dynamic(value)
  }
}

fn captured_json_value_from_non_bool_dynamic(
  value: Dynamic,
) -> types.CapturedJsonValue {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> types.CapturedNull
    _ -> captured_json_value_from_present_dynamic(value)
  }
}

fn captured_json_value_from_present_dynamic(
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

fn runtime_json_from_dynamic(value: Dynamic) -> Json {
  case decode.run(value, decode.bool) {
    Ok(b) -> json.bool(b)
    Error(_) -> runtime_json_from_non_bool_dynamic(value)
  }
}

fn runtime_json_from_non_bool_dynamic(value: Dynamic) -> Json {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> json.null()
    _ -> runtime_json_from_present_dynamic(value)
  }
}

fn runtime_json_from_present_dynamic(value: Dynamic) -> Json {
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

fn float_decoder() -> Decoder(Float) {
  decode.one_of(decode.float, or: [decode.int |> decode.map(int.to_float)])
}

fn abandoned_checkout_decoder() -> Decoder(types.AbandonedCheckoutRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.AbandonedCheckoutRecord(
    id: id,
    cursor: cursor,
    data: data,
  ))
}

fn abandonment_delivery_activity_decoder() -> Decoder(
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

fn abandonment_decoder() -> Decoder(types.AbandonmentRecord) {
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

fn draft_order_decoder() -> Decoder(types.DraftOrderRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.DraftOrderRecord(id: id, cursor: cursor, data: data))
}

fn order_decoder() -> Decoder(types.OrderRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.OrderRecord(id: id, cursor: cursor, data: data))
}

fn draft_order_variant_catalog_decoder() -> Decoder(
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

fn backup_region_decoder() -> Decoder(types.BackupRegionRecord) {
  use id <- decode.field("id", decode.string)
  use name <- decode.field("name", decode.string)
  use code <- decode.field("code", decode.string)
  decode.success(types.BackupRegionRecord(id: id, name: name, code: code))
}

fn flow_signature_decoder() -> Decoder(types.AdminPlatformFlowSignatureRecord) {
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

fn flow_trigger_decoder() -> Decoder(types.AdminPlatformFlowTriggerRecord) {
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

fn shop_decoder() -> Decoder(types.ShopRecord) {
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

fn shop_domain_decoder() -> Decoder(types.ShopDomainRecord) {
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

fn shop_address_decoder() -> Decoder(types.ShopAddressRecord) {
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

fn shop_plan_decoder() -> Decoder(types.ShopPlanRecord) {
  use partner_development <- decode.field("partnerDevelopment", decode.bool)
  use public_display_name <- decode.field("publicDisplayName", decode.string)
  use shopify_plus <- decode.field("shopifyPlus", decode.bool)
  decode.success(types.ShopPlanRecord(
    partner_development: partner_development,
    public_display_name: public_display_name,
    shopify_plus: shopify_plus,
  ))
}

fn shop_resource_limits_decoder() -> Decoder(types.ShopResourceLimitsRecord) {
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

fn shop_features_decoder() -> Decoder(types.ShopFeaturesRecord) {
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
    sells_subscriptions: sells_subscriptions,
    show_metrics: show_metrics,
    storefront: storefront,
    unified_markets: unified_markets,
  ))
}

fn shop_bundles_feature_decoder() -> Decoder(types.ShopBundlesFeatureRecord) {
  use eligible_for_bundles <- decode.field("eligibleForBundles", decode.bool)
  use ineligibility_reason <- optional_string_field("ineligibilityReason")
  use sells_bundles <- decode.field("sellsBundles", decode.bool)
  decode.success(types.ShopBundlesFeatureRecord(
    eligible_for_bundles: eligible_for_bundles,
    ineligibility_reason: ineligibility_reason,
    sells_bundles: sells_bundles,
  ))
}

fn shop_cart_transform_feature_decoder() -> Decoder(
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

fn shop_cart_transform_eligible_operations_decoder() -> Decoder(
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

fn payment_settings_decoder() -> Decoder(types.PaymentSettingsRecord) {
  use supported_digital_wallets <- decode.field(
    "supportedDigitalWallets",
    decode.list(of: decode.string),
  )
  decode.success(types.PaymentSettingsRecord(
    supported_digital_wallets: supported_digital_wallets,
  ))
}

fn shop_policy_decoder() -> Decoder(types.ShopPolicyRecord) {
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

fn market_decoder() -> Decoder(types.MarketRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.MarketRecord(id: id, cursor: cursor, data: data))
}

fn catalog_decoder() -> Decoder(types.CatalogRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.CatalogRecord(id: id, cursor: cursor, data: data))
}

fn price_list_decoder() -> Decoder(types.PriceListRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.PriceListRecord(id: id, cursor: cursor, data: data))
}

fn web_presence_decoder() -> Decoder(types.WebPresenceRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- decode.field("data", captured_json_value_decoder())
  decode.success(types.WebPresenceRecord(id: id, cursor: cursor, data: data))
}

fn market_localization_decoder() -> Decoder(types.MarketLocalizationRecord) {
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

fn store_property_record_decoder() -> Decoder(types.StorePropertyRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- dict_field("data", store_property_value_decoder())
  decode.success(types.StorePropertyRecord(id: id, cursor: cursor, data: data))
}

fn store_property_mutation_payload_decoder() -> Decoder(
  types.StorePropertyMutationPayloadRecord,
) {
  use key <- decode.field("key", decode.string)
  use data <- dict_field("data", store_property_value_decoder())
  decode.success(types.StorePropertyMutationPayloadRecord(key: key, data: data))
}

fn store_property_value_decoder() -> Decoder(types.StorePropertyValue) {
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

fn saved_search_decoder() -> Decoder(types.SavedSearchRecord) {
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

fn saved_search_filter_decoder() -> Decoder(types.SavedSearchFilter) {
  use key <- decode.field("key", decode.string)
  use value <- decode.field("value", decode.string)
  decode.success(types.SavedSearchFilter(key: key, value: value))
}

fn webhook_subscription_decoder() -> Decoder(types.WebhookSubscriptionRecord) {
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

fn webhook_endpoint_decoder() -> Decoder(types.WebhookSubscriptionEndpoint) {
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

fn money_decoder() -> Decoder(types.Money) {
  use amount <- decode.field("amount", decode.string)
  use currency_code <- decode.field("currencyCode", decode.string)
  decode.success(types.Money(amount: amount, currency_code: currency_code))
}

fn access_scope_decoder() -> Decoder(types.AccessScopeRecord) {
  use handle <- decode.field("handle", decode.string)
  use description <- optional_string_field("description")
  decode.success(types.AccessScopeRecord(
    handle: handle,
    description: description,
  ))
}

fn app_decoder() -> Decoder(types.AppRecord) {
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

fn app_subscription_pricing_decoder() -> Decoder(types.AppSubscriptionPricing) {
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

fn app_subscription_line_item_plan_decoder() -> Decoder(
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

fn app_subscription_line_item_decoder() -> Decoder(
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

fn app_subscription_decoder() -> Decoder(types.AppSubscriptionRecord) {
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

fn app_one_time_purchase_decoder() -> Decoder(types.AppOneTimePurchaseRecord) {
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

fn app_usage_decoder() -> Decoder(types.AppUsageRecord) {
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

fn delegated_access_token_decoder() -> Decoder(types.DelegatedAccessTokenRecord) {
  use id <- decode.field("id", decode.string)
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
    access_token_sha256: access_token_sha256,
    access_token_preview: access_token_preview,
    access_scopes: access_scopes,
    created_at: created_at,
    expires_in: expires_in,
    destroyed_at: destroyed_at,
  ))
}

fn app_installation_decoder() -> Decoder(types.AppInstallationRecord) {
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

fn shopify_function_app_decoder() -> Decoder(types.ShopifyFunctionAppRecord) {
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

fn shopify_function_decoder() -> Decoder(types.ShopifyFunctionRecord) {
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

fn bulk_operation_decoder() -> Decoder(types.BulkOperationRecord) {
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

fn market_localizable_content_decoder() -> Decoder(
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

fn product_metafield_decoder() -> Decoder(types.ProductMetafieldRecord) {
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

fn metafield_definition_decoder() -> Decoder(types.MetafieldDefinitionRecord) {
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

fn metafield_definition_type_decoder() -> Decoder(
  types.MetafieldDefinitionTypeRecord,
) {
  use name <- decode.field("name", decode.string)
  use category <- optional_string_field("category")
  decode.success(types.MetafieldDefinitionTypeRecord(
    name: name,
    category: category,
  ))
}

fn metafield_definition_validation_decoder() -> Decoder(
  types.MetafieldDefinitionValidationRecord,
) {
  use name <- decode.field("name", decode.string)
  use value <- optional_string_field("value")
  decode.success(types.MetafieldDefinitionValidationRecord(
    name: name,
    value: value,
  ))
}

fn metafield_definition_capabilities_decoder() -> Decoder(
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

fn metafield_definition_capability_decoder() -> Decoder(
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

fn metafield_definition_constraints_decoder() -> Decoder(
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

fn metafield_definition_constraint_value_decoder() -> Decoder(
  types.MetafieldDefinitionConstraintValueRecord,
) {
  use value <- decode.field("value", decode.string)
  decode.success(types.MetafieldDefinitionConstraintValueRecord(value: value))
}

fn metaobject_definition_decoder() -> Decoder(types.MetaobjectDefinitionRecord) {
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
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn metaobject_definition_capabilities_decoder() -> Decoder(
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

fn metaobject_definition_capability_decoder() -> Decoder(
  types.MetaobjectDefinitionCapabilityRecord,
) {
  use enabled <- decode.field("enabled", decode.bool)
  decode.success(types.MetaobjectDefinitionCapabilityRecord(enabled: enabled))
}

fn metaobject_definition_type_decoder() -> Decoder(
  types.MetaobjectDefinitionTypeRecord,
) {
  use name <- decode.field("name", decode.string)
  use category <- optional_string_field("category")
  decode.success(types.MetaobjectDefinitionTypeRecord(
    name: name,
    category: category,
  ))
}

fn metaobject_field_definition_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionRecord,
) {
  use key <- decode.field("key", decode.string)
  use name <- optional_string_field("name")
  use description <- optional_string_field("description")
  use required <- optional_field("required", None, decode.optional(decode.bool))
  use type_ <- decode.field("type", metaobject_definition_type_decoder())
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
    validations: validations,
  ))
}

fn metaobject_field_definition_validation_decoder() -> Decoder(
  types.MetaobjectFieldDefinitionValidationRecord,
) {
  use name <- decode.field("name", decode.string)
  use value <- optional_string_field("value")
  decode.success(types.MetaobjectFieldDefinitionValidationRecord(
    name: name,
    value: value,
  ))
}

fn metaobject_standard_template_decoder() -> Decoder(
  types.MetaobjectStandardTemplateRecord,
) {
  use type_ <- optional_string_field("type")
  use name <- optional_string_field("name")
  decode.success(types.MetaobjectStandardTemplateRecord(
    type_: type_,
    name: name,
  ))
}

fn metaobject_decoder() -> Decoder(types.MetaobjectRecord) {
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

fn metaobject_field_decoder() -> Decoder(types.MetaobjectFieldRecord) {
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

fn metaobject_json_value_decoder() -> Decoder(types.MetaobjectJsonValue) {
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

fn metaobject_field_definition_ref_decoder() -> Decoder(
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

fn metaobject_capabilities_decoder() -> Decoder(
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

fn metaobject_publishable_decoder() -> Decoder(
  types.MetaobjectPublishableCapabilityRecord,
) {
  use status <- optional_string_field("status")
  decode.success(types.MetaobjectPublishableCapabilityRecord(status: status))
}

fn metaobject_online_store_decoder() -> Decoder(
  types.MetaobjectOnlineStoreCapabilityRecord,
) {
  use template_suffix <- optional_string_field("templateSuffix")
  decode.success(types.MetaobjectOnlineStoreCapabilityRecord(
    template_suffix: template_suffix,
  ))
}

fn marketing_record_decoder() -> Decoder(types.MarketingRecord) {
  use id <- decode.field("id", decode.string)
  use cursor <- optional_string_field("cursor")
  use data <- optional_field(
    "data",
    dict.new(),
    decode.dict(decode.string, marketing_value_decoder()),
  )
  decode.success(types.MarketingRecord(id: id, cursor: cursor, data: data))
}

fn marketing_engagement_decoder() -> Decoder(types.MarketingEngagementRecord) {
  use id <- decode.field("id", decode.string)
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
    marketing_activity_id: marketing_activity_id,
    remote_id: remote_id,
    channel_handle: channel_handle,
    occurred_on: occurred_on,
    data: data,
  ))
}

fn marketing_value_decoder() -> Decoder(types.MarketingValue) {
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

fn validation_decoder() -> Decoder(types.ValidationRecord) {
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

fn validation_metafield_decoder() -> Decoder(types.ValidationMetafieldRecord) {
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

fn cart_transform_decoder() -> Decoder(types.CartTransformRecord) {
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

fn tax_app_configuration_decoder() -> Decoder(types.TaxAppConfigurationRecord) {
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

fn gift_card_transaction_decoder() -> Decoder(types.GiftCardTransactionRecord) {
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

fn gift_card_recipient_attributes_decoder() -> Decoder(
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

fn gift_card_decoder() -> Decoder(types.GiftCardRecord) {
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

fn gift_card_configuration_decoder() -> Decoder(
  types.GiftCardConfigurationRecord,
) {
  use issue_limit <- decode.field("issueLimit", money_decoder())
  use purchase_limit <- decode.field("purchaseLimit", money_decoder())
  decode.success(types.GiftCardConfigurationRecord(
    issue_limit: issue_limit,
    purchase_limit: purchase_limit,
  ))
}

fn segment_decoder() -> Decoder(types.SegmentRecord) {
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

fn customer_segment_members_query_decoder() -> Decoder(
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

fn locale_decoder() -> Decoder(types.LocaleRecord) {
  use iso_code <- decode.field("isoCode", decode.string)
  use name <- decode.field("name", decode.string)
  decode.success(types.LocaleRecord(iso_code: iso_code, name: name))
}

fn shop_locale_decoder() -> Decoder(types.ShopLocaleRecord) {
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

fn translation_decoder() -> Decoder(types.TranslationRecord) {
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

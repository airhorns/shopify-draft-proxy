//// Mirrors the slices of `src/state/store.ts` that have been ported to
//// Gleam plus the mutation log. Additional resources still land
//// slice-by-slice with their domain handlers.
////
//// The TS class mutates state in place. This Gleam port returns updated
//// `Store` records from every mutator so callers thread state through
//// their own pipeline (matching the pattern already established for
//// `SyntheticIdentityRegistry`).

import gleam/dict.{type Dict}
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentDeliveryActivityRecord,
  type AbandonmentRecord, type AdminPlatformFlowSignatureRecord,
  type AdminPlatformFlowTriggerRecord, type AdminPlatformGenericNodeRecord,
  type AdminPlatformTaxonomyCategoryRecord, type AppInstallationRecord,
  type AppOneTimePurchaseRecord, type AppRecord,
  type AppSubscriptionLineItemRecord, type AppSubscriptionRecord,
  type AppUsageRecord, type B2BCompanyContactRecord,
  type B2BCompanyContactRoleRecord, type B2BCompanyLocationRecord,
  type B2BCompanyRecord, type BackupRegionRecord, type BulkOperationRecord,
  type CapturedJsonValue, type CartTransformRecord, type CatalogRecord,
  type ChannelRecord, type CollectionRecord, type CustomerAccountPageRecord,
  type CustomerAddressRecord, type CustomerCatalogConnectionRecord,
  type CustomerCatalogPageInfoRecord, type CustomerDataErasureRequestRecord,
  type CustomerEventSummaryRecord, type CustomerMergeRequestRecord,
  type CustomerMetafieldRecord, type CustomerOrderSummaryRecord,
  type CustomerPaymentMethodRecord, type CustomerPaymentMethodUpdateUrlRecord,
  type CustomerRecord, type CustomerSegmentMembersQueryRecord,
  type DelegatedAccessTokenRecord, type DiscountBulkOperationRecord,
  type DiscountRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type FileRecord,
  type GiftCardConfigurationRecord, type GiftCardRecord,
  type InventoryLevelRecord, type InventoryShipmentRecord,
  type InventoryTransferRecord, type LocaleRecord, type LocationRecord,
  type MarketLocalizationRecord, type MarketRecord,
  type MarketingEngagementRecord, type MarketingRecord, type MarketingValue,
  type MetafieldDefinitionRecord, type MetaobjectDefinitionRecord,
  type MetaobjectRecord, type OnlineStoreContentRecord,
  type OnlineStoreIntegrationRecord, type OrderMandatePaymentRecord,
  type OrderRecord, type PaymentCustomizationRecord,
  type PaymentReminderSendRecord, type PaymentTermsRecord, type PriceListRecord,
  type ProductCollectionRecord, type ProductFeedRecord, type ProductMediaRecord,
  type ProductMetafieldRecord, type ProductOperationRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductVariantRecord,
  type PublicationRecord, type SavedSearchRecord, type SegmentRecord,
  type SellingPlanGroupRecord, type ShopLocaleRecord, type ShopRecord,
  type ShopResourceFeedbackRecord, type ShopifyFunctionRecord,
  type StoreCreditAccountRecord, type StoreCreditAccountTransactionRecord,
  type StorePropertyMutationPayloadRecord, type StorePropertyRecord,
  type StorePropertyValue, type TaxAppConfigurationRecord,
  type TranslationRecord, type ValidationRecord, type WebPresenceRecord,
  type WebhookSubscriptionRecord, AbandonmentRecord,
  AdminPlatformTaxonomyCategoryRecord, BulkOperationRecord, ChannelRecord,
  MarketingObject, MarketingString, PublicationRecord,
} as types_mod

/// Server-authoritative state. Mirrors the ported slices of `StateSnapshot`
/// for `baseState`. Other resources land slice-by-slice as their domain
/// handlers port.
pub type BaseState {
  BaseState(
    products: Dict(String, ProductRecord),
    product_order: List(String),
    deleted_product_ids: Dict(String, Bool),
    product_count: Option(Int),
    product_variants: Dict(String, ProductVariantRecord),
    product_variant_order: List(String),
    product_variant_count: Option(Int),
    product_options: Dict(String, ProductOptionRecord),
    product_operations: Dict(String, ProductOperationRecord),
    selling_plan_groups: Dict(String, SellingPlanGroupRecord),
    selling_plan_group_order: List(String),
    deleted_selling_plan_group_ids: Dict(String, Bool),
    markets: Dict(String, MarketRecord),
    market_order: List(String),
    deleted_market_ids: Dict(String, Bool),
    catalogs: Dict(String, CatalogRecord),
    catalog_order: List(String),
    deleted_catalog_ids: Dict(String, Bool),
    price_lists: Dict(String, PriceListRecord),
    price_list_order: List(String),
    deleted_price_list_ids: Dict(String, Bool),
    web_presences: Dict(String, WebPresenceRecord),
    web_presence_order: List(String),
    deleted_web_presence_ids: Dict(String, Bool),
    market_localizations: Dict(String, MarketLocalizationRecord),
    markets_root_payloads: Dict(String, CapturedJsonValue),
    product_media: Dict(String, List(ProductMediaRecord)),
    files: Dict(String, FileRecord),
    file_order: List(String),
    deleted_file_ids: Dict(String, Bool),
    collections: Dict(String, CollectionRecord),
    collection_order: List(String),
    product_collections: Dict(String, ProductCollectionRecord),
    deleted_collection_ids: Dict(String, Bool),
    locations: Dict(String, LocationRecord),
    location_order: List(String),
    publications: Dict(String, PublicationRecord),
    publication_order: List(String),
    deleted_publication_ids: Dict(String, Bool),
    channels: Dict(String, ChannelRecord),
    channel_order: List(String),
    product_feeds: Dict(String, ProductFeedRecord),
    product_feed_order: List(String),
    deleted_product_feed_ids: Dict(String, Bool),
    product_resource_feedback: Dict(String, ProductResourceFeedbackRecord),
    shop_resource_feedback: Dict(String, ShopResourceFeedbackRecord),
    abandoned_checkouts: Dict(String, AbandonedCheckoutRecord),
    abandoned_checkout_order: List(String),
    abandonments: Dict(String, AbandonmentRecord),
    abandonment_order: List(String),
    draft_orders: Dict(String, DraftOrderRecord),
    draft_order_order: List(String),
    deleted_draft_order_ids: Dict(String, Bool),
    draft_order_variant_catalog: Dict(String, DraftOrderVariantCatalogRecord),
    orders: Dict(String, OrderRecord),
    order_order: List(String),
    deleted_order_ids: Dict(String, Bool),
    order_mandate_payments: Dict(String, OrderMandatePaymentRecord),
    inventory_transfers: Dict(String, InventoryTransferRecord),
    inventory_transfer_order: List(String),
    deleted_inventory_transfer_ids: Dict(String, Bool),
    inventory_shipments: Dict(String, InventoryShipmentRecord),
    inventory_shipment_order: List(String),
    deleted_inventory_shipment_ids: Dict(String, Bool),
    backup_region: Option(BackupRegionRecord),
    admin_platform_generic_nodes: Dict(String, AdminPlatformGenericNodeRecord),
    admin_platform_taxonomy_categories: Dict(
      String,
      AdminPlatformTaxonomyCategoryRecord,
    ),
    admin_platform_taxonomy_category_order: List(String),
    admin_platform_flow_signatures: Dict(
      String,
      AdminPlatformFlowSignatureRecord,
    ),
    admin_platform_flow_signature_order: List(String),
    admin_platform_flow_triggers: Dict(String, AdminPlatformFlowTriggerRecord),
    admin_platform_flow_trigger_order: List(String),
    shop: Option(ShopRecord),
    b2b_companies: Dict(String, B2BCompanyRecord),
    b2b_company_order: List(String),
    deleted_b2b_company_ids: Dict(String, Bool),
    b2b_company_contacts: Dict(String, B2BCompanyContactRecord),
    b2b_company_contact_order: List(String),
    deleted_b2b_company_contact_ids: Dict(String, Bool),
    b2b_company_contact_roles: Dict(String, B2BCompanyContactRoleRecord),
    b2b_company_contact_role_order: List(String),
    deleted_b2b_company_contact_role_ids: Dict(String, Bool),
    b2b_company_locations: Dict(String, B2BCompanyLocationRecord),
    b2b_company_location_order: List(String),
    deleted_b2b_company_location_ids: Dict(String, Bool),
    store_property_locations: Dict(String, StorePropertyRecord),
    store_property_location_order: List(String),
    deleted_store_property_location_ids: Dict(String, Bool),
    business_entities: Dict(String, StorePropertyRecord),
    business_entity_order: List(String),
    publishables: Dict(String, StorePropertyRecord),
    publishable_order: List(String),
    store_property_mutation_payloads: Dict(
      String,
      StorePropertyMutationPayloadRecord,
    ),
    product_metafields: Dict(String, ProductMetafieldRecord),
    metafield_definitions: Dict(String, MetafieldDefinitionRecord),
    deleted_metafield_definition_ids: Dict(String, Bool),
    saved_searches: Dict(String, SavedSearchRecord),
    saved_search_order: List(String),
    deleted_saved_search_ids: Dict(String, Bool),
    webhook_subscriptions: Dict(String, WebhookSubscriptionRecord),
    webhook_subscription_order: List(String),
    deleted_webhook_subscription_ids: Dict(String, Bool),
    online_store_content: Dict(String, OnlineStoreContentRecord),
    online_store_content_order: List(String),
    deleted_online_store_content_ids: Dict(String, Bool),
    online_store_integrations: Dict(String, OnlineStoreIntegrationRecord),
    online_store_integration_order: List(String),
    deleted_online_store_integration_ids: Dict(String, Bool),
    apps: Dict(String, AppRecord),
    app_order: List(String),
    app_installations: Dict(String, AppInstallationRecord),
    app_installation_order: List(String),
    current_installation_id: Option(String),
    app_subscriptions: Dict(String, AppSubscriptionRecord),
    app_subscription_order: List(String),
    app_subscription_line_items: Dict(String, AppSubscriptionLineItemRecord),
    app_subscription_line_item_order: List(String),
    app_one_time_purchases: Dict(String, AppOneTimePurchaseRecord),
    app_one_time_purchase_order: List(String),
    app_usage_records: Dict(String, AppUsageRecord),
    app_usage_record_order: List(String),
    delegated_access_tokens: Dict(String, DelegatedAccessTokenRecord),
    delegated_access_token_order: List(String),
    shopify_functions: Dict(String, ShopifyFunctionRecord),
    shopify_function_order: List(String),
    bulk_operations: Dict(String, BulkOperationRecord),
    bulk_operation_order: List(String),
    metaobject_definitions: Dict(String, MetaobjectDefinitionRecord),
    metaobject_definition_order: List(String),
    deleted_metaobject_definition_ids: Dict(String, Bool),
    metaobjects: Dict(String, MetaobjectRecord),
    metaobject_order: List(String),
    deleted_metaobject_ids: Dict(String, Bool),
    marketing_activities: Dict(String, MarketingRecord),
    marketing_activity_order: List(String),
    marketing_events: Dict(String, MarketingRecord),
    marketing_event_order: List(String),
    marketing_engagements: Dict(String, MarketingEngagementRecord),
    marketing_engagement_order: List(String),
    deleted_marketing_activity_ids: Dict(String, Bool),
    deleted_marketing_event_ids: Dict(String, Bool),
    deleted_marketing_engagement_ids: Dict(String, Bool),
    validations: Dict(String, ValidationRecord),
    validation_order: List(String),
    deleted_validation_ids: Dict(String, Bool),
    cart_transforms: Dict(String, CartTransformRecord),
    cart_transform_order: List(String),
    deleted_cart_transform_ids: Dict(String, Bool),
    tax_app_configuration: Option(TaxAppConfigurationRecord),
    discounts: Dict(String, DiscountRecord),
    discount_order: List(String),
    deleted_discount_ids: Dict(String, Bool),
    discount_bulk_operations: Dict(String, DiscountBulkOperationRecord),
    gift_cards: Dict(String, GiftCardRecord),
    gift_card_order: List(String),
    gift_card_configuration: Option(GiftCardConfigurationRecord),
    customers: Dict(String, CustomerRecord),
    customer_order: List(String),
    customer_catalog_connections: Dict(String, CustomerCatalogConnectionRecord),
    deleted_customer_ids: Dict(String, Bool),
    customer_addresses: Dict(String, CustomerAddressRecord),
    customer_address_order: List(String),
    deleted_customer_address_ids: Dict(String, Bool),
    customer_order_summaries: Dict(String, CustomerOrderSummaryRecord),
    customer_order_connection_page_infos: Dict(
      String,
      CustomerCatalogPageInfoRecord,
    ),
    customer_event_summaries: Dict(String, CustomerEventSummaryRecord),
    customer_event_connection_page_infos: Dict(
      String,
      CustomerCatalogPageInfoRecord,
    ),
    customer_last_orders: Dict(String, CustomerOrderSummaryRecord),
    customer_metafields: Dict(String, CustomerMetafieldRecord),
    customer_payment_methods: Dict(String, CustomerPaymentMethodRecord),
    customer_payment_method_update_urls: Dict(
      String,
      CustomerPaymentMethodUpdateUrlRecord,
    ),
    deleted_customer_payment_method_ids: Dict(String, Bool),
    payment_reminder_sends: Dict(String, PaymentReminderSendRecord),
    payment_customizations: Dict(String, PaymentCustomizationRecord),
    payment_customization_order: List(String),
    deleted_payment_customization_ids: Dict(String, Bool),
    payment_terms: Dict(String, PaymentTermsRecord),
    payment_terms_owner_ids: Dict(String, Bool),
    payment_terms_by_owner_id: Dict(String, String),
    deleted_payment_terms_ids: Dict(String, Bool),
    store_credit_accounts: Dict(String, StoreCreditAccountRecord),
    store_credit_account_transactions: Dict(
      String,
      StoreCreditAccountTransactionRecord,
    ),
    customer_account_pages: Dict(String, CustomerAccountPageRecord),
    customer_account_page_order: List(String),
    customer_data_erasure_requests: Dict(
      String,
      CustomerDataErasureRequestRecord,
    ),
    merged_customer_ids: Dict(String, String),
    customer_merge_requests: Dict(String, CustomerMergeRequestRecord),
    segments: Dict(String, SegmentRecord),
    segment_order: List(String),
    deleted_segment_ids: Dict(String, Bool),
    segment_root_payloads: Dict(String, StorePropertyValue),
    customer_segment_members_queries: Dict(
      String,
      CustomerSegmentMembersQueryRecord,
    ),
    customer_segment_members_query_order: List(String),
    available_locales: List(LocaleRecord),
    shop_locales: Dict(String, ShopLocaleRecord),
    translations: Dict(String, TranslationRecord),
  )
}

/// Mutations the proxy has staged but not yet committed upstream.
/// Mirrors the staged slices of `StateSnapshot`.
pub type StagedState {
  StagedState(
    products: Dict(String, ProductRecord),
    product_order: List(String),
    deleted_product_ids: Dict(String, Bool),
    product_count: Option(Int),
    product_variants: Dict(String, ProductVariantRecord),
    product_variant_order: List(String),
    product_variant_count: Option(Int),
    product_options: Dict(String, ProductOptionRecord),
    product_operations: Dict(String, ProductOperationRecord),
    selling_plan_groups: Dict(String, SellingPlanGroupRecord),
    selling_plan_group_order: List(String),
    deleted_selling_plan_group_ids: Dict(String, Bool),
    markets: Dict(String, MarketRecord),
    market_order: List(String),
    deleted_market_ids: Dict(String, Bool),
    catalogs: Dict(String, CatalogRecord),
    catalog_order: List(String),
    deleted_catalog_ids: Dict(String, Bool),
    price_lists: Dict(String, PriceListRecord),
    price_list_order: List(String),
    deleted_price_list_ids: Dict(String, Bool),
    web_presences: Dict(String, WebPresenceRecord),
    web_presence_order: List(String),
    deleted_web_presence_ids: Dict(String, Bool),
    market_localizations: Dict(String, MarketLocalizationRecord),
    markets_root_payloads: Dict(String, CapturedJsonValue),
    product_media: Dict(String, List(ProductMediaRecord)),
    files: Dict(String, FileRecord),
    file_order: List(String),
    deleted_file_ids: Dict(String, Bool),
    collections: Dict(String, CollectionRecord),
    collection_order: List(String),
    product_collections: Dict(String, ProductCollectionRecord),
    staged_product_collection_families: Dict(String, Bool),
    deleted_collection_ids: Dict(String, Bool),
    publications: Dict(String, PublicationRecord),
    publication_order: List(String),
    deleted_publication_ids: Dict(String, Bool),
    product_feeds: Dict(String, ProductFeedRecord),
    product_feed_order: List(String),
    deleted_product_feed_ids: Dict(String, Bool),
    product_resource_feedback: Dict(String, ProductResourceFeedbackRecord),
    shop_resource_feedback: Dict(String, ShopResourceFeedbackRecord),
    abandoned_checkouts: Dict(String, AbandonedCheckoutRecord),
    abandoned_checkout_order: List(String),
    abandonments: Dict(String, AbandonmentRecord),
    abandonment_order: List(String),
    draft_orders: Dict(String, DraftOrderRecord),
    draft_order_order: List(String),
    deleted_draft_order_ids: Dict(String, Bool),
    draft_order_variant_catalog: Dict(String, DraftOrderVariantCatalogRecord),
    orders: Dict(String, OrderRecord),
    order_order: List(String),
    deleted_order_ids: Dict(String, Bool),
    order_mandate_payments: Dict(String, OrderMandatePaymentRecord),
    inventory_transfers: Dict(String, InventoryTransferRecord),
    inventory_transfer_order: List(String),
    deleted_inventory_transfer_ids: Dict(String, Bool),
    inventory_shipments: Dict(String, InventoryShipmentRecord),
    inventory_shipment_order: List(String),
    deleted_inventory_shipment_ids: Dict(String, Bool),
    backup_region: Option(BackupRegionRecord),
    admin_platform_generic_nodes: Dict(String, AdminPlatformGenericNodeRecord),
    admin_platform_taxonomy_categories: Dict(
      String,
      AdminPlatformTaxonomyCategoryRecord,
    ),
    admin_platform_taxonomy_category_order: List(String),
    admin_platform_flow_signatures: Dict(
      String,
      AdminPlatformFlowSignatureRecord,
    ),
    admin_platform_flow_signature_order: List(String),
    admin_platform_flow_triggers: Dict(String, AdminPlatformFlowTriggerRecord),
    admin_platform_flow_trigger_order: List(String),
    shop: Option(ShopRecord),
    b2b_companies: Dict(String, B2BCompanyRecord),
    b2b_company_order: List(String),
    deleted_b2b_company_ids: Dict(String, Bool),
    b2b_company_contacts: Dict(String, B2BCompanyContactRecord),
    b2b_company_contact_order: List(String),
    deleted_b2b_company_contact_ids: Dict(String, Bool),
    b2b_company_contact_roles: Dict(String, B2BCompanyContactRoleRecord),
    b2b_company_contact_role_order: List(String),
    deleted_b2b_company_contact_role_ids: Dict(String, Bool),
    b2b_company_locations: Dict(String, B2BCompanyLocationRecord),
    b2b_company_location_order: List(String),
    deleted_b2b_company_location_ids: Dict(String, Bool),
    store_property_locations: Dict(String, StorePropertyRecord),
    store_property_location_order: List(String),
    deleted_store_property_location_ids: Dict(String, Bool),
    business_entities: Dict(String, StorePropertyRecord),
    business_entity_order: List(String),
    publishables: Dict(String, StorePropertyRecord),
    publishable_order: List(String),
    store_property_mutation_payloads: Dict(
      String,
      StorePropertyMutationPayloadRecord,
    ),
    product_metafields: Dict(String, ProductMetafieldRecord),
    metafield_definitions: Dict(String, MetafieldDefinitionRecord),
    deleted_metafield_definition_ids: Dict(String, Bool),
    saved_searches: Dict(String, SavedSearchRecord),
    saved_search_order: List(String),
    deleted_saved_search_ids: Dict(String, Bool),
    webhook_subscriptions: Dict(String, WebhookSubscriptionRecord),
    webhook_subscription_order: List(String),
    deleted_webhook_subscription_ids: Dict(String, Bool),
    online_store_content: Dict(String, OnlineStoreContentRecord),
    online_store_content_order: List(String),
    deleted_online_store_content_ids: Dict(String, Bool),
    online_store_integrations: Dict(String, OnlineStoreIntegrationRecord),
    online_store_integration_order: List(String),
    deleted_online_store_integration_ids: Dict(String, Bool),
    apps: Dict(String, AppRecord),
    app_order: List(String),
    app_installations: Dict(String, AppInstallationRecord),
    app_installation_order: List(String),
    current_installation_id: Option(String),
    app_subscriptions: Dict(String, AppSubscriptionRecord),
    app_subscription_order: List(String),
    app_subscription_line_items: Dict(String, AppSubscriptionLineItemRecord),
    app_subscription_line_item_order: List(String),
    app_one_time_purchases: Dict(String, AppOneTimePurchaseRecord),
    app_one_time_purchase_order: List(String),
    app_usage_records: Dict(String, AppUsageRecord),
    app_usage_record_order: List(String),
    delegated_access_tokens: Dict(String, DelegatedAccessTokenRecord),
    delegated_access_token_order: List(String),
    shopify_functions: Dict(String, ShopifyFunctionRecord),
    shopify_function_order: List(String),
    bulk_operations: Dict(String, BulkOperationRecord),
    bulk_operation_order: List(String),
    metaobject_definitions: Dict(String, MetaobjectDefinitionRecord),
    metaobject_definition_order: List(String),
    deleted_metaobject_definition_ids: Dict(String, Bool),
    metaobjects: Dict(String, MetaobjectRecord),
    metaobject_order: List(String),
    deleted_metaobject_ids: Dict(String, Bool),
    marketing_activities: Dict(String, MarketingRecord),
    marketing_activity_order: List(String),
    marketing_events: Dict(String, MarketingRecord),
    marketing_event_order: List(String),
    marketing_engagements: Dict(String, MarketingEngagementRecord),
    marketing_engagement_order: List(String),
    deleted_marketing_activity_ids: Dict(String, Bool),
    deleted_marketing_event_ids: Dict(String, Bool),
    deleted_marketing_engagement_ids: Dict(String, Bool),
    validations: Dict(String, ValidationRecord),
    validation_order: List(String),
    deleted_validation_ids: Dict(String, Bool),
    cart_transforms: Dict(String, CartTransformRecord),
    cart_transform_order: List(String),
    deleted_cart_transform_ids: Dict(String, Bool),
    tax_app_configuration: Option(TaxAppConfigurationRecord),
    discounts: Dict(String, DiscountRecord),
    discount_order: List(String),
    deleted_discount_ids: Dict(String, Bool),
    discount_bulk_operations: Dict(String, DiscountBulkOperationRecord),
    gift_cards: Dict(String, GiftCardRecord),
    gift_card_order: List(String),
    gift_card_configuration: Option(GiftCardConfigurationRecord),
    customers: Dict(String, CustomerRecord),
    customer_order: List(String),
    deleted_customer_ids: Dict(String, Bool),
    customer_addresses: Dict(String, CustomerAddressRecord),
    customer_address_order: List(String),
    deleted_customer_address_ids: Dict(String, Bool),
    customer_order_summaries: Dict(String, CustomerOrderSummaryRecord),
    customer_order_connection_page_infos: Dict(
      String,
      CustomerCatalogPageInfoRecord,
    ),
    customer_event_summaries: Dict(String, CustomerEventSummaryRecord),
    customer_event_connection_page_infos: Dict(
      String,
      CustomerCatalogPageInfoRecord,
    ),
    customer_last_orders: Dict(String, CustomerOrderSummaryRecord),
    customer_metafields: Dict(String, CustomerMetafieldRecord),
    customer_payment_methods: Dict(String, CustomerPaymentMethodRecord),
    customer_payment_method_update_urls: Dict(
      String,
      CustomerPaymentMethodUpdateUrlRecord,
    ),
    deleted_customer_payment_method_ids: Dict(String, Bool),
    payment_reminder_sends: Dict(String, PaymentReminderSendRecord),
    payment_customizations: Dict(String, PaymentCustomizationRecord),
    payment_customization_order: List(String),
    deleted_payment_customization_ids: Dict(String, Bool),
    payment_terms: Dict(String, PaymentTermsRecord),
    payment_terms_owner_ids: Dict(String, Bool),
    payment_terms_by_owner_id: Dict(String, String),
    deleted_payment_terms_ids: Dict(String, Bool),
    store_credit_accounts: Dict(String, StoreCreditAccountRecord),
    store_credit_account_transactions: Dict(
      String,
      StoreCreditAccountTransactionRecord,
    ),
    customer_account_pages: Dict(String, CustomerAccountPageRecord),
    customer_account_page_order: List(String),
    customer_data_erasure_requests: Dict(
      String,
      CustomerDataErasureRequestRecord,
    ),
    merged_customer_ids: Dict(String, String),
    customer_merge_requests: Dict(String, CustomerMergeRequestRecord),
    segments: Dict(String, SegmentRecord),
    segment_order: List(String),
    deleted_segment_ids: Dict(String, Bool),
    customer_segment_members_queries: Dict(
      String,
      CustomerSegmentMembersQueryRecord,
    ),
    customer_segment_members_query_order: List(String),
    shop_locales: Dict(String, ShopLocaleRecord),
    deleted_shop_locales: Dict(String, Bool),
    translations: Dict(String, TranslationRecord),
    deleted_translations: Dict(String, Bool),
  )
}

/// Operation type a mutation log entry was recorded for. Mirrors the
/// `'query' | 'mutation'` union in TS.
pub type OperationType {
  Query
  Mutation
}

/// Status the mutation log records each entry under. Mirrors
/// `'staged' | 'proxied' | 'committed' | 'failed'`.
pub type EntryStatus {
  Staged
  Proxied
  Committed
  Failed
}

/// Capability metadata recorded alongside each mutation log entry.
/// Mirrors `MutationLogInterpretedMetadata['capability']`.
pub type Capability {
  Capability(operation_name: Option(String), domain: String, execution: String)
}

/// Slim port of `MutationLogInterpretedMetadata`. Only the fields the
/// Gleam port currently writes are modelled. The optional pieces
/// (`registeredOperation`, `safety`, `bulkOperationImport`) are deferred
/// until their producers port.
pub type InterpretedMetadata {
  InterpretedMetadata(
    operation_type: OperationType,
    operation_name: Option(String),
    root_fields: List(String),
    primary_root_field: Option(String),
    capability: Capability,
  )
}

/// Slim port of `MutationLogEntry`. `requestBody` and the optional
/// fields are deferred to the next pass that produces them.
pub type MutationLogEntry {
  MutationLogEntry(
    id: String,
    received_at: String,
    operation_name: Option(String),
    path: String,
    query: String,
    variables: Dict(String, String),
    staged_resource_ids: List(String),
    status: EntryStatus,
    interpreted: InterpretedMetadata,
    notes: Option(String),
  )
}

/// Long-lived runtime store. The TS class also tracks lagged search
/// caches and a handful of cross-domain side tables; those will land
/// when their domains do.
pub type Store {
  Store(
    base_state: BaseState,
    staged_state: StagedState,
    mutation_log: List(MutationLogEntry),
  )
}

/// An empty `BaseState`. Equivalent to `cloneSnapshot(EMPTY_SNAPSHOT)`
/// projected onto the slices we ship.
pub fn empty_base_state() -> BaseState {
  BaseState(
    products: dict.new(),
    product_order: [],
    deleted_product_ids: dict.new(),
    product_count: None,
    product_variants: dict.new(),
    product_variant_order: [],
    product_variant_count: None,
    product_options: dict.new(),
    product_operations: dict.new(),
    selling_plan_groups: dict.new(),
    selling_plan_group_order: [],
    deleted_selling_plan_group_ids: dict.new(),
    markets: dict.new(),
    market_order: [],
    deleted_market_ids: dict.new(),
    catalogs: dict.new(),
    catalog_order: [],
    deleted_catalog_ids: dict.new(),
    price_lists: dict.new(),
    price_list_order: [],
    deleted_price_list_ids: dict.new(),
    web_presences: dict.new(),
    web_presence_order: [],
    deleted_web_presence_ids: dict.new(),
    market_localizations: dict.new(),
    markets_root_payloads: dict.new(),
    product_media: dict.new(),
    files: dict.new(),
    file_order: [],
    deleted_file_ids: dict.new(),
    collections: dict.new(),
    collection_order: [],
    product_collections: dict.new(),
    deleted_collection_ids: dict.new(),
    locations: dict.new(),
    location_order: [],
    publications: dict.new(),
    publication_order: [],
    deleted_publication_ids: dict.new(),
    channels: dict.new(),
    channel_order: [],
    product_feeds: dict.new(),
    product_feed_order: [],
    deleted_product_feed_ids: dict.new(),
    product_resource_feedback: dict.new(),
    shop_resource_feedback: dict.new(),
    abandoned_checkouts: dict.new(),
    abandoned_checkout_order: [],
    abandonments: dict.new(),
    abandonment_order: [],
    draft_orders: dict.new(),
    draft_order_order: [],
    deleted_draft_order_ids: dict.new(),
    draft_order_variant_catalog: dict.new(),
    orders: dict.new(),
    order_order: [],
    deleted_order_ids: dict.new(),
    order_mandate_payments: dict.new(),
    inventory_transfers: dict.new(),
    inventory_transfer_order: [],
    deleted_inventory_transfer_ids: dict.new(),
    inventory_shipments: dict.new(),
    inventory_shipment_order: [],
    deleted_inventory_shipment_ids: dict.new(),
    backup_region: None,
    admin_platform_generic_nodes: dict.new(),
    admin_platform_taxonomy_categories: dict.new(),
    admin_platform_taxonomy_category_order: [],
    admin_platform_flow_signatures: dict.new(),
    admin_platform_flow_signature_order: [],
    admin_platform_flow_triggers: dict.new(),
    admin_platform_flow_trigger_order: [],
    shop: None,
    b2b_companies: dict.new(),
    b2b_company_order: [],
    deleted_b2b_company_ids: dict.new(),
    b2b_company_contacts: dict.new(),
    b2b_company_contact_order: [],
    deleted_b2b_company_contact_ids: dict.new(),
    b2b_company_contact_roles: dict.new(),
    b2b_company_contact_role_order: [],
    deleted_b2b_company_contact_role_ids: dict.new(),
    b2b_company_locations: dict.new(),
    b2b_company_location_order: [],
    deleted_b2b_company_location_ids: dict.new(),
    store_property_locations: dict.new(),
    store_property_location_order: [],
    deleted_store_property_location_ids: dict.new(),
    business_entities: dict.new(),
    business_entity_order: [],
    publishables: dict.new(),
    publishable_order: [],
    store_property_mutation_payloads: dict.new(),
    product_metafields: dict.new(),
    metafield_definitions: dict.new(),
    deleted_metafield_definition_ids: dict.new(),
    saved_searches: dict.new(),
    saved_search_order: [],
    deleted_saved_search_ids: dict.new(),
    webhook_subscriptions: dict.new(),
    webhook_subscription_order: [],
    deleted_webhook_subscription_ids: dict.new(),
    online_store_content: dict.new(),
    online_store_content_order: [],
    deleted_online_store_content_ids: dict.new(),
    online_store_integrations: dict.new(),
    online_store_integration_order: [],
    deleted_online_store_integration_ids: dict.new(),
    apps: dict.new(),
    app_order: [],
    app_installations: dict.new(),
    app_installation_order: [],
    current_installation_id: None,
    app_subscriptions: dict.new(),
    app_subscription_order: [],
    app_subscription_line_items: dict.new(),
    app_subscription_line_item_order: [],
    app_one_time_purchases: dict.new(),
    app_one_time_purchase_order: [],
    app_usage_records: dict.new(),
    app_usage_record_order: [],
    delegated_access_tokens: dict.new(),
    delegated_access_token_order: [],
    shopify_functions: dict.new(),
    shopify_function_order: [],
    bulk_operations: dict.new(),
    bulk_operation_order: [],
    metaobject_definitions: dict.new(),
    metaobject_definition_order: [],
    deleted_metaobject_definition_ids: dict.new(),
    metaobjects: dict.new(),
    metaobject_order: [],
    deleted_metaobject_ids: dict.new(),
    marketing_activities: dict.new(),
    marketing_activity_order: [],
    marketing_events: dict.new(),
    marketing_event_order: [],
    marketing_engagements: dict.new(),
    marketing_engagement_order: [],
    deleted_marketing_activity_ids: dict.new(),
    deleted_marketing_event_ids: dict.new(),
    deleted_marketing_engagement_ids: dict.new(),
    validations: dict.new(),
    validation_order: [],
    deleted_validation_ids: dict.new(),
    cart_transforms: dict.new(),
    cart_transform_order: [],
    deleted_cart_transform_ids: dict.new(),
    tax_app_configuration: None,
    discounts: dict.new(),
    discount_order: [],
    deleted_discount_ids: dict.new(),
    discount_bulk_operations: dict.new(),
    gift_cards: dict.new(),
    gift_card_order: [],
    gift_card_configuration: None,
    customers: dict.new(),
    customer_order: [],
    customer_catalog_connections: dict.new(),
    deleted_customer_ids: dict.new(),
    customer_addresses: dict.new(),
    customer_address_order: [],
    deleted_customer_address_ids: dict.new(),
    customer_order_summaries: dict.new(),
    customer_order_connection_page_infos: dict.new(),
    customer_event_summaries: dict.new(),
    customer_event_connection_page_infos: dict.new(),
    customer_last_orders: dict.new(),
    customer_metafields: dict.new(),
    customer_payment_methods: dict.new(),
    customer_payment_method_update_urls: dict.new(),
    deleted_customer_payment_method_ids: dict.new(),
    payment_reminder_sends: dict.new(),
    payment_customizations: dict.new(),
    payment_customization_order: [],
    deleted_payment_customization_ids: dict.new(),
    payment_terms: dict.new(),
    payment_terms_owner_ids: dict.new(),
    payment_terms_by_owner_id: dict.new(),
    deleted_payment_terms_ids: dict.new(),
    store_credit_accounts: dict.new(),
    store_credit_account_transactions: dict.new(),
    customer_account_pages: dict.new(),
    customer_account_page_order: [],
    customer_data_erasure_requests: dict.new(),
    merged_customer_ids: dict.new(),
    customer_merge_requests: dict.new(),
    segments: dict.new(),
    segment_order: [],
    deleted_segment_ids: dict.new(),
    segment_root_payloads: dict.new(),
    customer_segment_members_queries: dict.new(),
    customer_segment_members_query_order: [],
    available_locales: [],
    shop_locales: dict.new(),
    translations: dict.new(),
  )
}

/// An empty `StagedState`.
pub fn empty_staged_state() -> StagedState {
  StagedState(
    products: dict.new(),
    product_order: [],
    deleted_product_ids: dict.new(),
    product_count: None,
    product_variants: dict.new(),
    product_variant_order: [],
    product_variant_count: None,
    product_options: dict.new(),
    product_operations: dict.new(),
    selling_plan_groups: dict.new(),
    selling_plan_group_order: [],
    deleted_selling_plan_group_ids: dict.new(),
    markets: dict.new(),
    market_order: [],
    deleted_market_ids: dict.new(),
    catalogs: dict.new(),
    catalog_order: [],
    deleted_catalog_ids: dict.new(),
    price_lists: dict.new(),
    price_list_order: [],
    deleted_price_list_ids: dict.new(),
    web_presences: dict.new(),
    web_presence_order: [],
    deleted_web_presence_ids: dict.new(),
    market_localizations: dict.new(),
    markets_root_payloads: dict.new(),
    product_media: dict.new(),
    files: dict.new(),
    file_order: [],
    deleted_file_ids: dict.new(),
    collections: dict.new(),
    collection_order: [],
    product_collections: dict.new(),
    staged_product_collection_families: dict.new(),
    deleted_collection_ids: dict.new(),
    publications: dict.new(),
    publication_order: [],
    deleted_publication_ids: dict.new(),
    product_feeds: dict.new(),
    product_feed_order: [],
    deleted_product_feed_ids: dict.new(),
    product_resource_feedback: dict.new(),
    shop_resource_feedback: dict.new(),
    abandoned_checkouts: dict.new(),
    abandoned_checkout_order: [],
    abandonments: dict.new(),
    abandonment_order: [],
    draft_orders: dict.new(),
    draft_order_order: [],
    deleted_draft_order_ids: dict.new(),
    draft_order_variant_catalog: dict.new(),
    orders: dict.new(),
    order_order: [],
    deleted_order_ids: dict.new(),
    order_mandate_payments: dict.new(),
    inventory_transfers: dict.new(),
    inventory_transfer_order: [],
    deleted_inventory_transfer_ids: dict.new(),
    inventory_shipments: dict.new(),
    inventory_shipment_order: [],
    deleted_inventory_shipment_ids: dict.new(),
    backup_region: None,
    admin_platform_generic_nodes: dict.new(),
    admin_platform_taxonomy_categories: dict.new(),
    admin_platform_taxonomy_category_order: [],
    admin_platform_flow_signatures: dict.new(),
    admin_platform_flow_signature_order: [],
    admin_platform_flow_triggers: dict.new(),
    admin_platform_flow_trigger_order: [],
    shop: None,
    b2b_companies: dict.new(),
    b2b_company_order: [],
    deleted_b2b_company_ids: dict.new(),
    b2b_company_contacts: dict.new(),
    b2b_company_contact_order: [],
    deleted_b2b_company_contact_ids: dict.new(),
    b2b_company_contact_roles: dict.new(),
    b2b_company_contact_role_order: [],
    deleted_b2b_company_contact_role_ids: dict.new(),
    b2b_company_locations: dict.new(),
    b2b_company_location_order: [],
    deleted_b2b_company_location_ids: dict.new(),
    store_property_locations: dict.new(),
    store_property_location_order: [],
    deleted_store_property_location_ids: dict.new(),
    business_entities: dict.new(),
    business_entity_order: [],
    publishables: dict.new(),
    publishable_order: [],
    store_property_mutation_payloads: dict.new(),
    product_metafields: dict.new(),
    metafield_definitions: dict.new(),
    deleted_metafield_definition_ids: dict.new(),
    saved_searches: dict.new(),
    saved_search_order: [],
    deleted_saved_search_ids: dict.new(),
    webhook_subscriptions: dict.new(),
    webhook_subscription_order: [],
    deleted_webhook_subscription_ids: dict.new(),
    online_store_content: dict.new(),
    online_store_content_order: [],
    deleted_online_store_content_ids: dict.new(),
    online_store_integrations: dict.new(),
    online_store_integration_order: [],
    deleted_online_store_integration_ids: dict.new(),
    apps: dict.new(),
    app_order: [],
    app_installations: dict.new(),
    app_installation_order: [],
    current_installation_id: None,
    app_subscriptions: dict.new(),
    app_subscription_order: [],
    app_subscription_line_items: dict.new(),
    app_subscription_line_item_order: [],
    app_one_time_purchases: dict.new(),
    app_one_time_purchase_order: [],
    app_usage_records: dict.new(),
    app_usage_record_order: [],
    delegated_access_tokens: dict.new(),
    delegated_access_token_order: [],
    shopify_functions: dict.new(),
    shopify_function_order: [],
    bulk_operations: dict.new(),
    bulk_operation_order: [],
    metaobject_definitions: dict.new(),
    metaobject_definition_order: [],
    deleted_metaobject_definition_ids: dict.new(),
    metaobjects: dict.new(),
    metaobject_order: [],
    deleted_metaobject_ids: dict.new(),
    marketing_activities: dict.new(),
    marketing_activity_order: [],
    marketing_events: dict.new(),
    marketing_event_order: [],
    marketing_engagements: dict.new(),
    marketing_engagement_order: [],
    deleted_marketing_activity_ids: dict.new(),
    deleted_marketing_event_ids: dict.new(),
    deleted_marketing_engagement_ids: dict.new(),
    validations: dict.new(),
    validation_order: [],
    deleted_validation_ids: dict.new(),
    cart_transforms: dict.new(),
    cart_transform_order: [],
    deleted_cart_transform_ids: dict.new(),
    tax_app_configuration: None,
    discounts: dict.new(),
    discount_order: [],
    deleted_discount_ids: dict.new(),
    discount_bulk_operations: dict.new(),
    gift_cards: dict.new(),
    gift_card_order: [],
    gift_card_configuration: None,
    customers: dict.new(),
    customer_order: [],
    deleted_customer_ids: dict.new(),
    customer_addresses: dict.new(),
    customer_address_order: [],
    deleted_customer_address_ids: dict.new(),
    customer_order_summaries: dict.new(),
    customer_order_connection_page_infos: dict.new(),
    customer_event_summaries: dict.new(),
    customer_event_connection_page_infos: dict.new(),
    customer_last_orders: dict.new(),
    customer_metafields: dict.new(),
    customer_payment_methods: dict.new(),
    customer_payment_method_update_urls: dict.new(),
    deleted_customer_payment_method_ids: dict.new(),
    payment_reminder_sends: dict.new(),
    payment_customizations: dict.new(),
    payment_customization_order: [],
    deleted_payment_customization_ids: dict.new(),
    payment_terms: dict.new(),
    payment_terms_owner_ids: dict.new(),
    payment_terms_by_owner_id: dict.new(),
    deleted_payment_terms_ids: dict.new(),
    store_credit_accounts: dict.new(),
    store_credit_account_transactions: dict.new(),
    customer_account_pages: dict.new(),
    customer_account_page_order: [],
    customer_data_erasure_requests: dict.new(),
    merged_customer_ids: dict.new(),
    customer_merge_requests: dict.new(),
    segments: dict.new(),
    segment_order: [],
    deleted_segment_ids: dict.new(),
    customer_segment_members_queries: dict.new(),
    customer_segment_members_query_order: [],
    shop_locales: dict.new(),
    deleted_shop_locales: dict.new(),
    translations: dict.new(),
    deleted_translations: dict.new(),
  )
}

/// Fresh store, equivalent to `new InMemoryStore()`.
pub fn new() -> Store {
  Store(
    base_state: empty_base_state(),
    staged_state: empty_staged_state(),
    mutation_log: [],
  )
}

/// Reset both base and staged state plus the mutation log. Mirrors
/// `reset()` (which calls `restoreInitialState()` against an empty
/// snapshot — equivalent to a fresh store for the slices we ship).
pub fn reset(_store: Store) -> Store {
  new()
}

// ---------------------------------------------------------------------------
// Orders / abandonments slice
// ---------------------------------------------------------------------------

pub fn upsert_base_abandoned_checkouts(
  store: Store,
  records: List(AbandonedCheckoutRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        abandoned_checkouts: dict.insert(
          base.abandoned_checkouts,
          record.id,
          record,
        ),
        abandoned_checkout_order: append_unique_id(
          base.abandoned_checkout_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_abandonments(
  store: Store,
  records: List(AbandonmentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        abandonments: dict.insert(base.abandonments, record.id, record),
        abandonment_order: append_unique_id(base.abandonment_order, record.id),
      ),
    )
  })
}

pub fn upsert_base_draft_orders(
  store: Store,
  records: List(DraftOrderRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        draft_orders: dict.insert(base.draft_orders, record.id, record),
        draft_order_order: append_unique_id(base.draft_order_order, record.id),
        deleted_draft_order_ids: dict.delete(
          base.deleted_draft_order_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_draft_order_ids: dict.delete(
          staged.deleted_draft_order_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_draft_order_variant_catalog(
  store: Store,
  records: List(DraftOrderVariantCatalogRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        draft_order_variant_catalog: dict.insert(
          base.draft_order_variant_catalog,
          record.variant_id,
          record,
        ),
      ),
    )
  })
}

pub fn upsert_base_orders(store: Store, records: List(OrderRecord)) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        orders: dict.insert(base.orders, record.id, record),
        order_order: append_unique_id(base.order_order, record.id),
        deleted_order_ids: dict.delete(base.deleted_order_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_order_ids: dict.delete(staged.deleted_order_ids, record.id),
      ),
    )
  })
}

pub fn stage_order(store: Store, record: OrderRecord) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      orders: dict.insert(staged.orders, record.id, record),
      order_order: append_unique_id(staged.order_order, record.id),
      deleted_order_ids: dict.delete(staged.deleted_order_ids, record.id),
    ),
  )
}

pub fn stage_draft_order(store: Store, record: DraftOrderRecord) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      draft_orders: dict.insert(staged.draft_orders, record.id, record),
      draft_order_order: append_unique_id(staged.draft_order_order, record.id),
      deleted_draft_order_ids: dict.delete(
        staged.deleted_draft_order_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_draft_order(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      draft_orders: dict.delete(staged.draft_orders, id),
      deleted_draft_order_ids: dict.insert(
        staged.deleted_draft_order_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn delete_staged_order(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      orders: dict.delete(staged.orders, id),
      deleted_order_ids: dict.insert(staged.deleted_order_ids, id, True),
    ),
  )
}

pub fn get_abandoned_checkout_by_id(
  store: Store,
  id: String,
) -> Option(AbandonedCheckoutRecord) {
  case dict.get(store.staged_state.abandoned_checkouts, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.abandoned_checkouts, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_draft_order_by_id(
  store: Store,
  id: String,
) -> Option(DraftOrderRecord) {
  case dict.get(store.staged_state.deleted_draft_order_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.draft_orders, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.deleted_draft_order_ids, id) {
            Ok(True) -> None
            _ ->
              case dict.get(store.base_state.draft_orders, id) {
                Ok(record) -> Some(record)
                Error(_) -> None
              }
          }
      }
  }
}

pub fn get_order_by_id(store: Store, id: String) -> Option(OrderRecord) {
  case dict.get(store.staged_state.deleted_order_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.orders, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.deleted_order_ids, id) {
            Ok(True) -> None
            _ ->
              case dict.get(store.base_state.orders, id) {
                Ok(record) -> Some(record)
                Error(_) -> None
              }
          }
      }
  }
}

pub fn list_effective_draft_orders(store: Store) -> List(DraftOrderRecord) {
  let ordered_ids =
    list.append(
      store.base_state.draft_order_order,
      store.staged_state.draft_order_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_draft_order_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.draft_orders)
    |> list.append(dict.values(store.staged_state.draft_orders))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered) |> dedupe_draft_orders()
}

pub fn list_effective_orders(store: Store) -> List(OrderRecord) {
  let ordered_ids =
    list.append(store.base_state.order_order, store.staged_state.order_order)
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_order_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.orders)
    |> list.append(dict.values(store.staged_state.orders))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered) |> dedupe_orders()
}

pub fn get_order_mandate_payment(
  store: Store,
  order_id: String,
  idempotency_key: String,
) -> Option(OrderMandatePaymentRecord) {
  let key = order_mandate_payment_key(order_id, idempotency_key)
  case dict.get(store.staged_state.order_mandate_payments, key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.order_mandate_payments, key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_staged_order_mandate_payment(
  store: Store,
  record: OrderMandatePaymentRecord,
) -> Store {
  let key = order_mandate_payment_key(record.order_id, record.idempotency_key)
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      order_mandate_payments: dict.insert(
        store.staged_state.order_mandate_payments,
        key,
        record,
      ),
    ),
  )
}

fn order_mandate_payment_key(
  order_id: String,
  idempotency_key: String,
) -> String {
  order_id <> "::" <> idempotency_key
}

pub fn get_draft_order_variant_catalog_by_id(
  store: Store,
  variant_id: String,
) -> Option(DraftOrderVariantCatalogRecord) {
  case dict.get(store.staged_state.draft_order_variant_catalog, variant_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.draft_order_variant_catalog, variant_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_abandonment_by_id(
  store: Store,
  id: String,
) -> Option(AbandonmentRecord) {
  case dict.get(store.staged_state.abandonments, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.abandonments, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_abandonment_by_abandoned_checkout_id(
  store: Store,
  checkout_id: String,
) -> Option(AbandonmentRecord) {
  case
    list_effective_abandonments(store)
    |> list.find(fn(record) {
      record.abandoned_checkout_id == Some(checkout_id)
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_abandoned_checkouts(
  store: Store,
) -> List(AbandonedCheckoutRecord) {
  let ordered_ids =
    list.append(
      store.base_state.abandoned_checkout_order,
      store.staged_state.abandoned_checkout_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_abandoned_checkout_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.abandoned_checkouts)
    |> list.append(dict.values(store.staged_state.abandoned_checkouts))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered)
  |> dedupe_abandoned_checkouts()
  |> list.sort(by: compare_abandoned_checkouts)
}

pub fn list_effective_abandonments(store: Store) -> List(AbandonmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.abandonment_order,
      store.staged_state.abandonment_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_abandonment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.abandonments)
    |> list.append(dict.values(store.staged_state.abandonments))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered)
  |> dedupe_abandonments()
  |> list.sort(by: compare_abandonments)
}

pub fn stage_abandonment_delivery_activity(
  store: Store,
  abandonment_id: String,
  activity: AbandonmentDeliveryActivityRecord,
) -> #(Store, Option(AbandonmentRecord)) {
  case get_abandonment_by_id(store, abandonment_id) {
    None -> #(store, None)
    Some(record) -> {
      let updated_data =
        captured_object_upsert(
          captured_object_upsert(
            record.data,
            "emailState",
            types_mod.CapturedString(activity.delivery_status),
          ),
          "emailSentAt",
          optional_captured_string(activity.delivered_at),
        )
      let updated =
        AbandonmentRecord(
          ..record,
          data: updated_data,
          delivery_activities: dict.insert(
            record.delivery_activities,
            activity.marketing_activity_id,
            activity,
          ),
        )
      let staged = store.staged_state
      #(
        Store(
          ..store,
          staged_state: StagedState(
            ..staged,
            abandonments: dict.insert(staged.abandonments, updated.id, updated),
            abandonment_order: append_unique_id(
              staged.abandonment_order,
              updated.id,
            ),
          ),
        ),
        Some(updated),
      )
    }
  }
}

fn optional_captured_string(
  value: Option(String),
) -> types_mod.CapturedJsonValue {
  case value {
    Some(value) -> types_mod.CapturedString(value)
    None -> types_mod.CapturedNull
  }
}

fn captured_object_upsert(
  value: types_mod.CapturedJsonValue,
  key: String,
  field_value: types_mod.CapturedJsonValue,
) -> types_mod.CapturedJsonValue {
  case value {
    types_mod.CapturedObject(fields) ->
      types_mod.CapturedObject(upsert_captured_field(fields, key, field_value))
    _ -> value
  }
}

fn upsert_captured_field(
  fields: List(#(String, types_mod.CapturedJsonValue)),
  key: String,
  value: types_mod.CapturedJsonValue,
) -> List(#(String, types_mod.CapturedJsonValue)) {
  case fields {
    [] -> [#(key, value)]
    [first, ..rest] -> {
      let #(field_key, _) = first
      case field_key == key {
        True -> [#(key, value), ..rest]
        False -> [first, ..upsert_captured_field(rest, key, value)]
      }
    }
  }
}

fn captured_string_field(
  value: types_mod.CapturedJsonValue,
  key: String,
) -> String {
  case value {
    types_mod.CapturedObject(fields) -> {
      case list.find(fields, fn(pair) { pair.0 == key }) {
        Ok(#(_, types_mod.CapturedString(value))) -> value
        _ -> ""
      }
    }
    _ -> ""
  }
}

fn compare_abandoned_checkouts(
  left: AbandonedCheckoutRecord,
  right: AbandonedCheckoutRecord,
) -> order.Order {
  case
    string.compare(
      captured_string_field(right.data, "createdAt"),
      captured_string_field(left.data, "createdAt"),
    )
  {
    order.Eq -> resource_ids.compare_shopify_resource_ids(right.id, left.id)
    other -> other
  }
}

fn compare_abandonments(
  left: AbandonmentRecord,
  right: AbandonmentRecord,
) -> order.Order {
  case
    string.compare(
      captured_string_field(right.data, "createdAt"),
      captured_string_field(left.data, "createdAt"),
    )
  {
    order.Eq -> resource_ids.compare_shopify_resource_ids(right.id, left.id)
    other -> other
  }
}

fn dedupe_abandoned_checkouts(
  records: List(AbandonedCheckoutRecord),
) -> List(AbandonedCheckoutRecord) {
  let initial: List(AbandonedCheckoutRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_abandonments(
  records: List(AbandonmentRecord),
) -> List(AbandonmentRecord) {
  let initial: List(AbandonmentRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_draft_orders(
  records: List(DraftOrderRecord),
) -> List(DraftOrderRecord) {
  let initial: List(DraftOrderRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_orders(records: List(OrderRecord)) -> List(OrderRecord) {
  let initial: List(OrderRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

// ---------------------------------------------------------------------------
// Products slice
// ---------------------------------------------------------------------------

/// Upsert product records into the base state. Capture/upstream state wins
/// over stale deleted markers, matching the existing collection slice pattern.
pub fn upsert_base_products(
  store: Store,
  records: List(ProductRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        products: dict.insert(base.products, record.id, record),
        product_order: append_unique_id(base.product_order, record.id),
        deleted_product_ids: dict.delete(base.deleted_product_ids, record.id),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_product_ids: dict.delete(staged.deleted_product_ids, record.id),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn set_base_product_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    base_state: BaseState(..store.base_state, product_count: Some(count)),
  )
}

pub fn set_staged_product_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    staged_state: StagedState(..store.staged_state, product_count: Some(count)),
  )
}

pub fn upsert_staged_product(
  store: Store,
  record: ProductRecord,
) -> #(ProductRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.product_order, record.id)
    || list.contains(staged.product_order, record.id)
  let new_order = case already_known {
    True -> staged.product_order
    False -> list.append(staged.product_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      products: dict.insert(staged.products, record.id, record),
      product_order: new_order,
      deleted_product_ids: dict.delete(staged.deleted_product_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_product(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      products: dict.delete(staged.products, id),
      deleted_product_ids: dict.insert(staged.deleted_product_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

pub fn replace_base_media_for_product(
  store: Store,
  product_id: String,
  media: List(ProductMediaRecord),
) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      product_media: dict.insert(
        store.base_state.product_media,
        product_id,
        media,
      ),
    ),
  )
}

pub fn replace_staged_media_for_product(
  store: Store,
  product_id: String,
  media: List(ProductMediaRecord),
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      product_media: dict.insert(
        store.staged_state.product_media,
        product_id,
        media,
      ),
    ),
  )
}

pub fn get_effective_media_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductMediaRecord) {
  case dict.get(store.staged_state.product_media, product_id) {
    Ok(media) -> sort_product_media(media)
    Error(_) ->
      case dict.get(store.base_state.product_media, product_id) {
        Ok(media) -> sort_product_media(media)
        Error(_) -> []
      }
  }
}

fn sort_product_media(
  media: List(ProductMediaRecord),
) -> List(ProductMediaRecord) {
  list.sort(media, fn(left, right) {
    case int.compare(left.position, right.position) {
      order.Eq -> string_compare(left.key, right.key)
      other -> other
    }
  })
}

// ---------------------------------------------------------------------------
// Files slice
// ---------------------------------------------------------------------------

pub fn upsert_base_files(store: Store, files: List(FileRecord)) -> Store {
  list.fold(files, store, fn(current, file) {
    let base = current.base_state
    Store(
      ..current,
      base_state: BaseState(
        ..base,
        files: dict.insert(base.files, file.id, file),
        file_order: append_unique_id(base.file_order, file.id),
        deleted_file_ids: dict.delete(base.deleted_file_ids, file.id),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_file_ids: dict.delete(
          current.staged_state.deleted_file_ids,
          file.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_files(store: Store, files: List(FileRecord)) -> Store {
  list.fold(files, store, fn(current, file) {
    let staged = current.staged_state
    let already_known =
      dict.has_key(current.base_state.files, file.id)
      || dict.has_key(staged.files, file.id)
      || list.contains(current.base_state.file_order, file.id)
      || list.contains(staged.file_order, file.id)
    let file_order = case already_known {
      True -> staged.file_order
      False -> list.append(staged.file_order, [file.id])
    }
    Store(
      ..current,
      staged_state: StagedState(
        ..staged,
        files: dict.insert(staged.files, file.id, file),
        file_order: file_order,
        deleted_file_ids: dict.delete(staged.deleted_file_ids, file.id),
      ),
    )
  })
}

pub fn delete_staged_files(store: Store, file_ids: List(String)) -> Store {
  let store =
    list.fold(file_ids, store, fn(current, file_id) {
      let staged = current.staged_state
      Store(
        ..current,
        staged_state: StagedState(
          ..staged,
          files: dict.delete(staged.files, file_id),
          deleted_file_ids: dict.insert(staged.deleted_file_ids, file_id, True),
        ),
      )
    })

  product_ids_with_media_ids(store, file_ids)
  |> list.fold(store, fn(current, product_id) {
    let next_media =
      get_effective_media_by_product_id(current, product_id)
      |> list.filter(fn(media) {
        case media.id {
          Some(id) -> !list.contains(file_ids, id)
          None -> True
        }
      })
    replace_staged_media_for_product(current, product_id, next_media)
  })
}

pub fn has_effective_file_by_id(store: Store, file_id: String) -> Bool {
  case dict_has(store.staged_state.deleted_file_ids, file_id) {
    True -> False
    False ->
      dict.has_key(store.staged_state.files, file_id)
      || dict.has_key(store.base_state.files, file_id)
      || product_media_file_exists(store, file_id)
  }
}

pub fn get_effective_file_by_id(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  case dict_has(store.staged_state.deleted_file_ids, file_id) {
    True -> None
    False ->
      case dict.get(store.staged_state.files, file_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.files, file_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_files(store: Store) -> List(FileRecord) {
  let ids =
    list.append(store.base_state.file_order, store.staged_state.file_order)
    |> dedupe_strings
  let ordered =
    ids
    |> list.filter_map(fn(id) {
      case get_effective_file_by_id(store, id) {
        Some(file) -> Ok(file)
        None -> Error(Nil)
      }
    })
  let ordered_ids = list.map(ordered, fn(file) { file.id })
  let unordered =
    list.append(
      dict.values(store.base_state.files),
      dict.values(store.staged_state.files),
    )
    |> list.filter(fn(file) { !list.contains(ordered_ids, file.id) })
    |> list.filter(fn(file) {
      !dict_has(store.staged_state.deleted_file_ids, file.id)
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

pub fn list_effective_product_media(store: Store) -> List(ProductMediaRecord) {
  let product_ids =
    list.append(
      dict.keys(store.base_state.product_media),
      dict.keys(store.staged_state.product_media),
    )
    |> dedupe_strings
  product_ids
  |> list.flat_map(fn(product_id) {
    get_effective_media_by_product_id(store, product_id)
  })
}

fn product_media_file_exists(store: Store, file_id: String) -> Bool {
  list.any(list_effective_product_media(store), fn(media) {
    media.id == Some(file_id)
  })
}

fn product_ids_with_media_ids(
  store: Store,
  file_ids: List(String),
) -> List(String) {
  list_effective_product_media(store)
  |> list.filter_map(fn(media) {
    case media.id {
      Some(id) ->
        case list.contains(file_ids, id) {
          True -> Ok(media.product_id)
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
  |> dedupe_strings
}

pub fn get_effective_product_by_id(
  store: Store,
  id: String,
) -> Option(ProductRecord) {
  let deleted =
    dict_has(store.base_state.deleted_product_ids, id)
    || dict_has(store.staged_state.deleted_product_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.products, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.products, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_product_by_handle(
  store: Store,
  handle: String,
) -> Option(ProductRecord) {
  case
    list.find(list_effective_products(store), fn(product) {
      product.handle == handle
    })
  {
    Ok(product) -> Some(product)
    Error(_) -> None
  }
}

pub fn list_effective_products(store: Store) -> List(ProductRecord) {
  let ordered_ids =
    list.append(
      store.base_state.product_order,
      store.staged_state.product_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_product_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.products, store.staged_state.products)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_product_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn get_effective_product_count(store: Store) -> Int {
  case store.staged_state.product_count {
    Some(count) -> count
    None ->
      case store.base_state.product_count {
        Some(count) -> count
        None -> list.length(list_effective_products(store))
      }
  }
}

pub fn upsert_base_collections(
  store: Store,
  records: List(CollectionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        collections: dict.insert(base.collections, record.id, record),
        collection_order: append_unique_id(base.collection_order, record.id),
        deleted_collection_ids: dict.delete(
          base.deleted_collection_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_collection_ids: dict.delete(
          staged.deleted_collection_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_collections(
  store: Store,
  records: List(CollectionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let staged = acc.staged_state
    let new_staged =
      StagedState(
        ..staged,
        collections: dict.insert(staged.collections, record.id, record),
        collection_order: append_unique_id(staged.collection_order, record.id),
        deleted_collection_ids: dict.delete(
          staged.deleted_collection_ids,
          record.id,
        ),
      )
    Store(..acc, staged_state: new_staged)
  })
}

pub fn delete_staged_collection(store: Store, id: String) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  let base_product_collections =
    base.product_collections
    |> dict.keys()
    |> list.fold(base.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let staged_product_collections =
    staged.product_collections
    |> dict.keys()
    |> list.fold(staged.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let new_base =
    BaseState(..base, product_collections: base_product_collections)
  let new_staged =
    StagedState(
      ..staged,
      collections: dict.delete(staged.collections, id),
      product_collections: staged_product_collections,
      deleted_collection_ids: dict.insert(
        staged.deleted_collection_ids,
        id,
        True,
      ),
    )
  Store(..store, base_state: new_base, staged_state: new_staged)
}

pub fn upsert_base_locations(
  store: Store,
  records: List(LocationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        locations: dict.insert(base.locations, record.id, record),
        location_order: append_unique_id(base.location_order, record.id),
      ),
    )
  })
}

pub fn get_effective_location_by_id(
  store: Store,
  id: String,
) -> Option(LocationRecord) {
  case dict.get(store.base_state.locations, id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_locations(store: Store) -> List(LocationRecord) {
  let ordered_records =
    list.filter_map(store.base_state.location_order, fn(id) {
      case get_effective_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(store.base_state.location_order)
  let unordered =
    dict.keys(store.base_state.locations)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      case get_effective_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered)
}

pub fn upsert_base_publications(
  store: Store,
  records: List(PublicationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        publications: dict.insert(base.publications, record.id, record),
        publication_order: append_unique_id(base.publication_order, record.id),
        deleted_publication_ids: dict.delete(
          base.deleted_publication_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_publication_ids: dict.delete(
          staged.deleted_publication_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_base_channels(
  store: Store,
  records: List(ChannelRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        channels: dict.insert(base.channels, record.id, record),
        channel_order: append_unique_id(base.channel_order, record.id),
      ),
    )
  })
}

pub fn upsert_staged_publication(
  store: Store,
  record: PublicationRecord,
) -> #(PublicationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.publication_order, record.id)
    || list.contains(staged.publication_order, record.id)
  let new_order = case already_known {
    True -> staged.publication_order
    False -> list.append(staged.publication_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      publications: dict.insert(staged.publications, record.id, record),
      publication_order: new_order,
      deleted_publication_ids: dict.delete(
        staged.deleted_publication_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_publication(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      publications: dict.delete(staged.publications, id),
      deleted_publication_ids: dict.insert(
        staged.deleted_publication_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_publication_by_id(
  store: Store,
  id: String,
) -> Option(PublicationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_publication_ids, id)
    || dict_has(store.staged_state.deleted_publication_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.publications, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.publications, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_publications(store: Store) -> List(PublicationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.publication_order,
      store.staged_state.publication_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_publication_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.publications, store.staged_state.publications)
  let relation_ids =
    list.append(
      list.flat_map(list_effective_products(store), fn(product) {
        product.publication_ids
      }),
      list.flat_map(list_effective_collections(store), fn(collection) {
        collection.publication_ids
      }),
    )
  let relation_publications =
    relation_ids
    |> dedupe_strings()
    |> list.filter(fn(id) {
      !dict_has(ordered_set, id)
      && !dict_has(merged, id)
      && !dict_has(store.base_state.deleted_publication_ids, id)
      && !dict_has(store.staged_state.deleted_publication_ids, id)
    })
    |> list.map(fn(id) {
      PublicationRecord(
        id: id,
        name: None,
        auto_publish: None,
        supports_future_publishing: None,
        catalog_id: None,
        channel_id: None,
        cursor: None,
      )
    })
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) {
      case get_effective_publication_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(list.append(ordered_records, unordered), relation_publications)
}

pub fn get_effective_channel_by_id(
  store: Store,
  id: String,
) -> Option(ChannelRecord) {
  case dict.get(store.base_state.channels, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case
        list.find(list_effective_channels(store), fn(channel) {
          channel.id == id
        })
      {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_channels(store: Store) -> List(ChannelRecord) {
  let direct_channels =
    list.filter_map(store.base_state.channel_order, fn(id) {
      case dict.get(store.base_state.channels, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  let direct_set =
    list_to_set(list.map(direct_channels, fn(channel) { channel.id }))
  let derived =
    list_effective_publications(store)
    |> list.filter_map(fn(publication) {
      case channel_from_publication(publication) {
        Some(channel) ->
          case dict_has(direct_set, channel.id) {
            True -> Error(Nil)
            False -> Ok(channel)
          }
        None -> Error(Nil)
      }
    })
  list.append(direct_channels, derived)
  |> list.sort(fn(left, right) {
    resource_ids.compare_shopify_resource_ids(left.id, right.id)
  })
}

pub fn upsert_base_product_feeds(
  store: Store,
  records: List(ProductFeedRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        product_feeds: dict.insert(base.product_feeds, record.id, record),
        product_feed_order: append_unique_id(base.product_feed_order, record.id),
        deleted_product_feed_ids: dict.delete(
          base.deleted_product_feed_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_product_feed_ids: dict.delete(
          staged.deleted_product_feed_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_product_feed(
  store: Store,
  record: ProductFeedRecord,
) -> #(ProductFeedRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.product_feed_order, record.id)
    || list.contains(staged.product_feed_order, record.id)
  let new_order = case already_known {
    True -> staged.product_feed_order
    False -> list.append(staged.product_feed_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      product_feeds: dict.insert(staged.product_feeds, record.id, record),
      product_feed_order: new_order,
      deleted_product_feed_ids: dict.delete(
        staged.deleted_product_feed_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_product_feed(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_feeds: dict.delete(staged.product_feeds, id),
      deleted_product_feed_ids: dict.insert(
        staged.deleted_product_feed_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_product_feed_by_id(
  store: Store,
  id: String,
) -> Option(ProductFeedRecord) {
  let deleted =
    dict_has(store.base_state.deleted_product_feed_ids, id)
    || dict_has(store.staged_state.deleted_product_feed_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.product_feeds, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.product_feeds, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_product_feeds(store: Store) -> List(ProductFeedRecord) {
  let ordered_ids =
    list.append(
      store.base_state.product_feed_order,
      store.staged_state.product_feed_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_product_feed_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.product_feeds, store.staged_state.product_feeds)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_product_feed_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_product_resource_feedback(
  store: Store,
  records: List(ProductResourceFeedbackRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        product_resource_feedback: dict.insert(
          base.product_resource_feedback,
          record.product_id,
          record,
        ),
      ),
    )
  })
}

pub fn upsert_staged_product_resource_feedback(
  store: Store,
  record: ProductResourceFeedbackRecord,
) -> #(ProductResourceFeedbackRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      product_resource_feedback: dict.insert(
        staged.product_resource_feedback,
        record.product_id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn get_effective_product_resource_feedback(
  store: Store,
  product_id: String,
) -> Option(ProductResourceFeedbackRecord) {
  case dict.get(store.staged_state.product_resource_feedback, product_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.product_resource_feedback, product_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_staged_shop_resource_feedback(
  store: Store,
  record: ShopResourceFeedbackRecord,
) -> #(ShopResourceFeedbackRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      shop_resource_feedback: dict.insert(
        staged.shop_resource_feedback,
        record.id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_inventory_transfers(
  store: Store,
  transfers: List(InventoryTransferRecord),
) -> Store {
  list.fold(transfers, store, fn(acc, transfer) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_order = case
      list.contains(base.inventory_transfer_order, transfer.id)
    {
      True -> base.inventory_transfer_order
      False -> list.append(base.inventory_transfer_order, [transfer.id])
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        inventory_transfers: dict.insert(
          base.inventory_transfers,
          transfer.id,
          transfer,
        ),
        inventory_transfer_order: next_order,
        deleted_inventory_transfer_ids: dict.delete(
          base.deleted_inventory_transfer_ids,
          transfer.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_inventory_transfer_ids: dict.delete(
          staged.deleted_inventory_transfer_ids,
          transfer.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_inventory_transfer(
  store: Store,
  transfer: InventoryTransferRecord,
) -> #(InventoryTransferRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.inventory_transfer_order, transfer.id)
    || list.contains(staged.inventory_transfer_order, transfer.id)
  {
    True -> staged.inventory_transfer_order
    False -> list.append(staged.inventory_transfer_order, [transfer.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      inventory_transfers: dict.insert(
        staged.inventory_transfers,
        transfer.id,
        transfer,
      ),
      inventory_transfer_order: next_order,
      deleted_inventory_transfer_ids: dict.delete(
        staged.deleted_inventory_transfer_ids,
        transfer.id,
      ),
    )
  #(transfer, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_inventory_transfer(
  store: Store,
  transfer_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      inventory_transfers: dict.delete(staged.inventory_transfers, transfer_id),
      inventory_transfer_order: list.filter(
        staged.inventory_transfer_order,
        fn(id) { id != transfer_id },
      ),
      deleted_inventory_transfer_ids: dict.insert(
        staged.deleted_inventory_transfer_ids,
        transfer_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_inventory_transfer_by_id(
  store: Store,
  transfer_id: String,
) -> Option(InventoryTransferRecord) {
  case
    dict.has_key(store.staged_state.deleted_inventory_transfer_ids, transfer_id)
    || dict.has_key(
      store.base_state.deleted_inventory_transfer_ids,
      transfer_id,
    )
  {
    True -> None
    False ->
      case dict.get(store.staged_state.inventory_transfers, transfer_id) {
        Ok(transfer) -> Some(transfer)
        Error(_) ->
          case dict.get(store.base_state.inventory_transfers, transfer_id) {
            Ok(transfer) -> Some(transfer)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_inventory_transfers(
  store: Store,
) -> List(InventoryTransferRecord) {
  let ordered_ids =
    list.append(
      store.base_state.inventory_transfer_order,
      store.staged_state.inventory_transfer_order,
    )
    |> dedupe_strings
  let ordered_transfers =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_transfer_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.inventory_transfers)
    |> list.append(dict.keys(store.staged_state.inventory_transfers))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_transfers =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_transfer_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_transfers, unordered_transfers)
}

pub fn upsert_base_inventory_shipments(
  store: Store,
  shipments: List(InventoryShipmentRecord),
) -> Store {
  list.fold(shipments, store, fn(acc, shipment) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_order = case
      list.contains(base.inventory_shipment_order, shipment.id)
    {
      True -> base.inventory_shipment_order
      False -> list.append(base.inventory_shipment_order, [shipment.id])
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        inventory_shipments: dict.insert(
          base.inventory_shipments,
          shipment.id,
          shipment,
        ),
        inventory_shipment_order: next_order,
        deleted_inventory_shipment_ids: dict.delete(
          base.deleted_inventory_shipment_ids,
          shipment.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_inventory_shipment_ids: dict.delete(
          staged.deleted_inventory_shipment_ids,
          shipment.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_inventory_shipment(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> #(InventoryShipmentRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.inventory_shipment_order, shipment.id)
    || list.contains(staged.inventory_shipment_order, shipment.id)
  {
    True -> staged.inventory_shipment_order
    False -> list.append(staged.inventory_shipment_order, [shipment.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      inventory_shipments: dict.insert(
        staged.inventory_shipments,
        shipment.id,
        shipment,
      ),
      inventory_shipment_order: next_order,
      deleted_inventory_shipment_ids: dict.delete(
        staged.deleted_inventory_shipment_ids,
        shipment.id,
      ),
    )
  #(shipment, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_inventory_shipment(
  store: Store,
  shipment_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      inventory_shipments: dict.delete(staged.inventory_shipments, shipment_id),
      deleted_inventory_shipment_ids: dict.insert(
        staged.deleted_inventory_shipment_ids,
        shipment_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_inventory_shipment_by_id(
  store: Store,
  shipment_id: String,
) -> Option(InventoryShipmentRecord) {
  case
    dict.has_key(store.staged_state.deleted_inventory_shipment_ids, shipment_id)
    || dict.has_key(
      store.base_state.deleted_inventory_shipment_ids,
      shipment_id,
    )
  {
    True -> None
    False ->
      case dict.get(store.staged_state.inventory_shipments, shipment_id) {
        Ok(shipment) -> Some(shipment)
        Error(_) ->
          case dict.get(store.base_state.inventory_shipments, shipment_id) {
            Ok(shipment) -> Some(shipment)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_inventory_shipments(
  store: Store,
) -> List(InventoryShipmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.inventory_shipment_order,
      store.staged_state.inventory_shipment_order,
    )
    |> dedupe_strings
  let ordered_shipments =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_shipment_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.inventory_shipments)
    |> list.append(dict.keys(store.staged_state.inventory_shipments))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_shipments =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_shipment_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_shipments, unordered_shipments)
}

pub fn replace_base_products_for_collection(
  store: Store,
  collection_id: String,
  records: List(ProductCollectionRecord),
) -> Store {
  let base = store.base_state
  let retained =
    base.product_collections
    |> dict.keys()
    |> list.fold(base.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == collection_id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let next =
    list.fold(records, retained, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(..store, base_state: BaseState(..base, product_collections: next))
}

pub fn upsert_base_product_collections(
  store: Store,
  records: List(ProductCollectionRecord),
) -> Store {
  let base = store.base_state
  let product_collections =
    list.fold(records, base.product_collections, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    base_state: BaseState(..base, product_collections: product_collections),
  )
}

pub fn upsert_staged_product_collections(
  store: Store,
  records: List(ProductCollectionRecord),
) -> Store {
  let staged = store.staged_state
  let product_collections =
    list.fold(records, staged.product_collections, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_collections: product_collections,
    ),
  )
}

pub fn replace_staged_collections_for_product(
  store: Store,
  product_id: String,
  records: List(ProductCollectionRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    staged.product_collections
    |> dict.keys()
    |> list.fold(staged.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.product_id == product_id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let product_collections =
    list.fold(records, retained, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_collections: product_collections,
      staged_product_collection_families: dict.insert(
        staged.staged_product_collection_families,
        product_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_collection_by_id(
  store: Store,
  id: String,
) -> Option(CollectionRecord) {
  let deleted =
    dict_has(store.base_state.deleted_collection_ids, id)
    || dict_has(store.staged_state.deleted_collection_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.collections, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.collections, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_collection_by_handle(
  store: Store,
  handle: String,
) -> Option(CollectionRecord) {
  list.find(list_effective_collections(store), fn(collection) {
    collection.handle == handle
  })
  |> option.from_result
}

pub fn list_effective_collections(store: Store) -> List(CollectionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.collection_order,
      store.staged_state.collection_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_collection_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.collections, store.staged_state.collections)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_collection_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn list_effective_products_for_collection(
  store: Store,
  collection_id: String,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let memberships =
    list_effective_products(store)
    |> list.flat_map(fn(product) {
      list_effective_collections_for_product(store, product.id)
    })
    |> list.map(fn(entry) {
      let #(_, membership) = entry
      membership
    })
    |> list.filter(fn(record) { record.collection_id == collection_id })
    |> list.sort(compare_product_collection_records)
  list.filter_map(memberships, fn(membership) {
    case get_effective_product_by_id(store, membership.product_id) {
      Some(product) -> Ok(#(product, membership))
      None -> Error(Nil)
    }
  })
}

pub fn list_effective_collections_for_product(
  store: Store,
  product_id: String,
) -> List(#(CollectionRecord, ProductCollectionRecord)) {
  let staged_memberships =
    dict.values(store.staged_state.product_collections)
    |> list.filter(fn(record) { record.product_id == product_id })
  let source_memberships = case
    dict_has(store.staged_state.staged_product_collection_families, product_id)
  {
    True -> staged_memberships
    False ->
      dict.values(store.base_state.product_collections)
      |> list.append(staged_memberships)
      |> list.filter(fn(record) { record.product_id == product_id })
  }
  let memberships =
    source_memberships
    |> list.sort(compare_product_collection_records)
  list.filter_map(memberships, fn(membership) {
    case get_effective_collection_by_id(store, membership.collection_id) {
      Some(collection) -> Ok(#(collection, membership))
      None -> Error(Nil)
    }
  })
}

pub fn upsert_base_product_variants(
  store: Store,
  records: List(ProductVariantRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        product_variants: dict.insert(base.product_variants, record.id, record),
        product_variant_order: append_unique_id(
          base.product_variant_order,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base)
  })
}

pub fn set_base_product_variant_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      product_variant_count: Some(count),
    ),
  )
}

pub fn set_staged_product_variant_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      product_variant_count: Some(count),
    ),
  )
}

pub fn upsert_staged_product_variant(
  store: Store,
  record: ProductVariantRecord,
) -> #(ProductVariantRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.product_variant_order, record.id)
    || list.contains(staged.product_variant_order, record.id)
  let new_order = case already_known {
    True -> staged.product_variant_order
    False -> list.append(staged.product_variant_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      product_variants: dict.insert(staged.product_variants, record.id, record),
      product_variant_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn replace_staged_variants_for_product(
  store: Store,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    remove_variants_for_product(staged.product_variants, product_id)
  let retained_order =
    staged.product_variant_order
    |> list.filter(fn(id) { dict.has_key(retained, id) })
  let next_variants =
    list.fold(variants, retained, fn(acc, variant) {
      dict.insert(acc, variant.id, variant)
    })
  let next_order =
    list.fold(variants, retained_order, fn(acc, variant) {
      append_unique_id(acc, variant.id)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_variants: next_variants,
      product_variant_order: next_order,
    ),
  )
}

pub fn get_base_variants_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False ->
      list_variant_records(
        store.base_state.product_variants,
        store.base_state.product_variant_order,
      )
      |> list.filter(fn(variant) { variant.product_id == product_id })
  }
}

pub fn get_effective_variants_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False -> {
      let staged_variants =
        list_variant_records(
          store.staged_state.product_variants,
          store.staged_state.product_variant_order,
        )
        |> list.filter(fn(variant) { variant.product_id == product_id })
      case staged_variants {
        [] -> get_base_variants_by_product_id(store, product_id)
        _ -> staged_variants
      }
    }
  }
}

pub fn get_effective_variant_by_id(
  store: Store,
  variant_id: String,
) -> Option(ProductVariantRecord) {
  case dict.get(store.staged_state.product_variants, variant_id) {
    Ok(variant) ->
      case product_is_deleted(store, variant.product_id) {
        True -> None
        False -> Some(variant)
      }
    Error(_) ->
      case dict.get(store.base_state.product_variants, variant_id) {
        Ok(variant) ->
          case
            product_is_deleted(store, variant.product_id)
            || has_staged_variant_family(store, variant.product_id)
          {
            True -> None
            False -> Some(variant)
          }
        Error(_) -> None
      }
  }
}

pub fn find_effective_variant_by_inventory_item_id(
  store: Store,
  inventory_item_id: String,
) -> Option(ProductVariantRecord) {
  case
    list.find(list_effective_product_variants(store), fn(variant) {
      case variant.inventory_item {
        Some(item) -> item.id == inventory_item_id
        None -> False
      }
    })
  {
    Ok(variant) -> Some(variant)
    Error(_) -> None
  }
}

pub fn find_effective_inventory_level_by_id(
  store: Store,
  inventory_level_id: String,
) -> Option(InventoryLevelRecord) {
  store
  |> list_effective_product_variants
  |> list.filter_map(fn(variant) {
    case variant.inventory_item {
      Some(item) ->
        case
          list.find(item.inventory_levels, fn(level) {
            level.id == inventory_level_id
          })
        {
          Ok(level) -> Ok(level)
          Error(_) -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> list.first
  |> option.from_result
}

pub fn list_effective_product_variants(
  store: Store,
) -> List(ProductVariantRecord) {
  list_effective_products(store)
  |> list.flat_map(fn(product) {
    get_effective_variants_by_product_id(store, product.id)
  })
}

pub fn get_effective_product_variant_count(store: Store) -> Int {
  case store.staged_state.product_variant_count {
    Some(count) -> count
    None ->
      case store.base_state.product_variant_count {
        Some(count) -> count
        None -> list.length(list_effective_product_variants(store))
      }
  }
}

pub fn replace_base_options_for_product(
  store: Store,
  product_id: String,
  options: List(ProductOptionRecord),
) -> Store {
  let base = store.base_state
  let retained = remove_options_for_product(base.product_options, product_id)
  let next_options =
    list.fold(options, retained, fn(acc, option) {
      dict.insert(acc, option.id, option)
    })
  Store(..store, base_state: BaseState(..base, product_options: next_options))
}

pub fn replace_staged_options_for_product(
  store: Store,
  product_id: String,
  options: List(ProductOptionRecord),
) -> Store {
  let staged = store.staged_state
  let retained = remove_options_for_product(staged.product_options, product_id)
  let next_options =
    list.fold(options, retained, fn(acc, option) {
      dict.insert(acc, option.id, option)
    })
  Store(
    ..store,
    staged_state: StagedState(..staged, product_options: next_options),
  )
}

pub fn get_effective_options_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductOptionRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False -> {
      let staged_options =
        store.staged_state.product_options
        |> dict.values()
        |> list.filter(fn(option) { option.product_id == product_id })
      let source_options = case staged_options {
        [] ->
          store.base_state.product_options
          |> dict.values()
          |> list.filter(fn(option) { option.product_id == product_id })
        _ -> staged_options
      }
      list.sort(source_options, compare_product_options)
    }
  }
}

pub fn get_effective_product_option_by_id(
  store: Store,
  option_id: String,
) -> Option(ProductOptionRecord) {
  store
  |> list_effective_products
  |> list.find_map(fn(product) {
    get_effective_options_by_product_id(store, product.id)
    |> list.find(fn(option) { option.id == option_id })
  })
  |> option.from_result
}

pub fn get_effective_product_option_value_by_id(
  store: Store,
  option_value_id: String,
) -> Option(ProductOptionValueRecord) {
  store
  |> list_effective_products
  |> list.find_map(fn(product) {
    get_effective_options_by_product_id(store, product.id)
    |> list.find_map(fn(option) {
      option.option_values
      |> list.find(fn(value) { value.id == option_value_id })
    })
  })
  |> option.from_result
}

pub fn stage_product_operation(
  store: Store,
  operation: ProductOperationRecord,
) -> #(ProductOperationRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      product_operations: dict.insert(
        staged.product_operations,
        operation.id,
        operation,
      ),
    )
  #(operation, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_product_operations(
  store: Store,
  operations: List(ProductOperationRecord),
) -> Store {
  list.fold(operations, store, fn(acc, operation) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        product_operations: dict.insert(
          base.product_operations,
          operation.id,
          operation,
        ),
      ),
    )
  })
}

pub fn get_effective_product_operation_by_id(
  store: Store,
  id: String,
) -> Option(ProductOperationRecord) {
  case dict.get(store.staged_state.product_operations, id) {
    Ok(operation) -> Some(operation)
    Error(_) ->
      case dict.get(store.base_state.product_operations, id) {
        Ok(operation) -> Some(operation)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_selling_plan_groups(
  store: Store,
  groups: List(SellingPlanGroupRecord),
) -> Store {
  list.fold(groups, store, fn(acc, group) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_base =
      BaseState(
        ..base,
        selling_plan_groups: dict.insert(
          base.selling_plan_groups,
          group.id,
          group,
        ),
        selling_plan_group_order: append_unique_id(
          base.selling_plan_group_order,
          group.id,
        ),
        deleted_selling_plan_group_ids: dict.delete(
          base.deleted_selling_plan_group_ids,
          group.id,
        ),
      )
    let next_staged =
      StagedState(
        ..staged,
        deleted_selling_plan_group_ids: dict.delete(
          staged.deleted_selling_plan_group_ids,
          group.id,
        ),
      )
    Store(..acc, base_state: next_base, staged_state: next_staged)
  })
}

pub fn upsert_staged_selling_plan_group(
  store: Store,
  group: SellingPlanGroupRecord,
) -> #(SellingPlanGroupRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.selling_plan_group_order, group.id)
    || list.contains(staged.selling_plan_group_order, group.id)
  let next_order = case already_known {
    True -> staged.selling_plan_group_order
    False -> list.append(staged.selling_plan_group_order, [group.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      selling_plan_groups: dict.insert(
        staged.selling_plan_groups,
        group.id,
        group,
      ),
      selling_plan_group_order: next_order,
      deleted_selling_plan_group_ids: dict.delete(
        staged.deleted_selling_plan_group_ids,
        group.id,
      ),
    )
  #(group, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_selling_plan_group(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      selling_plan_groups: dict.delete(staged.selling_plan_groups, id),
      deleted_selling_plan_group_ids: dict.insert(
        staged.deleted_selling_plan_group_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_selling_plan_group_by_id(
  store: Store,
  id: String,
) -> Option(SellingPlanGroupRecord) {
  let deleted =
    dict_has(store.base_state.deleted_selling_plan_group_ids, id)
    || dict_has(store.staged_state.deleted_selling_plan_group_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.selling_plan_groups, id) {
        Ok(group) -> Some(group)
        Error(_) ->
          case dict.get(store.base_state.selling_plan_groups, id) {
            Ok(group) -> Some(group)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_selling_plan_groups(
  store: Store,
) -> List(SellingPlanGroupRecord) {
  let ordered_ids =
    list.append(
      store.base_state.selling_plan_group_order,
      store.staged_state.selling_plan_group_order,
    )
    |> dedupe_strings()
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      get_effective_selling_plan_group_by_id(store, id) |> option_to_result
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.selling_plan_groups,
      store.staged_state.selling_plan_groups,
    )
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) {
      get_effective_selling_plan_group_by_id(store, id) |> option_to_result
    })
  list.append(ordered, unordered)
}

// ---------------------------------------------------------------------------
// Markets slice
// ---------------------------------------------------------------------------

fn upsert_base_ordered_record(ids: List(String), id: String) -> List(String) {
  append_unique_id(ids, id)
}

pub fn upsert_base_markets(store: Store, records: List(MarketRecord)) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        markets: dict.insert(base.markets, record.id, record),
        market_order: upsert_base_ordered_record(base.market_order, record.id),
        deleted_market_ids: dict.delete(base.deleted_market_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_market_ids: dict.delete(staged.deleted_market_ids, record.id),
      ),
    )
  })
}

pub fn get_effective_market_by_id(
  store: Store,
  id: String,
) -> Option(MarketRecord) {
  case dict_has(store.staged_state.deleted_market_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.markets, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.markets, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_markets(store: Store) -> List(MarketRecord) {
  list_effective_ordered_records(
    store.base_state.market_order,
    store.staged_state.market_order,
    dict.merge(store.base_state.markets, store.staged_state.markets),
    fn(id) { get_effective_market_by_id(store, id) },
  )
}

pub fn upsert_staged_market(
  store: Store,
  record: MarketRecord,
) -> #(MarketRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.market_order, record.id)
    || list.contains(staged.market_order, record.id)
  let new_order = case already_known {
    True -> staged.market_order
    False -> list.append(staged.market_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      markets: dict.insert(staged.markets, record.id, record),
      market_order: new_order,
      deleted_market_ids: dict.delete(staged.deleted_market_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_market(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      markets: dict.delete(staged.markets, id),
      deleted_market_ids: dict.insert(staged.deleted_market_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

pub fn upsert_base_catalogs(
  store: Store,
  records: List(CatalogRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        catalogs: dict.insert(base.catalogs, record.id, record),
        catalog_order: upsert_base_ordered_record(base.catalog_order, record.id),
        deleted_catalog_ids: dict.delete(base.deleted_catalog_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_catalog_ids: dict.delete(staged.deleted_catalog_ids, record.id),
      ),
    )
  })
}

pub fn get_effective_catalog_by_id(
  store: Store,
  id: String,
) -> Option(CatalogRecord) {
  case dict_has(store.staged_state.deleted_catalog_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.catalogs, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.catalogs, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_catalogs(store: Store) -> List(CatalogRecord) {
  list_effective_ordered_records(
    store.base_state.catalog_order,
    store.staged_state.catalog_order,
    dict.merge(store.base_state.catalogs, store.staged_state.catalogs),
    fn(id) { get_effective_catalog_by_id(store, id) },
  )
}

pub fn upsert_staged_catalog(
  store: Store,
  record: CatalogRecord,
) -> #(CatalogRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.catalog_order, record.id)
    || list.contains(staged.catalog_order, record.id)
  let new_order = case already_known {
    True -> staged.catalog_order
    False -> list.append(staged.catalog_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      catalogs: dict.insert(staged.catalogs, record.id, record),
      catalog_order: new_order,
      deleted_catalog_ids: dict.delete(staged.deleted_catalog_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_catalog(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      catalogs: dict.delete(staged.catalogs, id),
      deleted_catalog_ids: dict.insert(staged.deleted_catalog_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

pub fn upsert_base_price_lists(
  store: Store,
  records: List(PriceListRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        price_lists: dict.insert(base.price_lists, record.id, record),
        price_list_order: upsert_base_ordered_record(
          base.price_list_order,
          record.id,
        ),
        deleted_price_list_ids: dict.delete(
          base.deleted_price_list_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_price_list_ids: dict.delete(
          staged.deleted_price_list_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_price_list_by_id(
  store: Store,
  id: String,
) -> Option(PriceListRecord) {
  case dict_has(store.staged_state.deleted_price_list_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.price_lists, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.price_lists, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_price_lists(store: Store) -> List(PriceListRecord) {
  list_effective_ordered_records(
    store.base_state.price_list_order,
    store.staged_state.price_list_order,
    dict.merge(store.base_state.price_lists, store.staged_state.price_lists),
    fn(id) { get_effective_price_list_by_id(store, id) },
  )
}

pub fn upsert_staged_price_list(
  store: Store,
  record: PriceListRecord,
) -> #(PriceListRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.price_list_order, record.id)
    || list.contains(staged.price_list_order, record.id)
  let new_order = case already_known {
    True -> staged.price_list_order
    False -> list.append(staged.price_list_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      price_lists: dict.insert(staged.price_lists, record.id, record),
      price_list_order: new_order,
      deleted_price_list_ids: dict.delete(
        staged.deleted_price_list_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_price_list(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      price_lists: dict.delete(staged.price_lists, id),
      deleted_price_list_ids: dict.insert(
        staged.deleted_price_list_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

pub fn upsert_base_web_presences(
  store: Store,
  records: List(WebPresenceRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        web_presences: dict.insert(base.web_presences, record.id, record),
        web_presence_order: upsert_base_ordered_record(
          base.web_presence_order,
          record.id,
        ),
        deleted_web_presence_ids: dict.delete(
          base.deleted_web_presence_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_web_presence_ids: dict.delete(
          staged.deleted_web_presence_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_web_presence_by_id(
  store: Store,
  id: String,
) -> Option(WebPresenceRecord) {
  case dict_has(store.staged_state.deleted_web_presence_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.web_presences, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.web_presences, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_web_presences(store: Store) -> List(WebPresenceRecord) {
  list_effective_ordered_records(
    store.base_state.web_presence_order,
    store.staged_state.web_presence_order,
    dict.merge(store.base_state.web_presences, store.staged_state.web_presences),
    fn(id) { get_effective_web_presence_by_id(store, id) },
  )
}

pub fn upsert_staged_web_presence(
  store: Store,
  record: WebPresenceRecord,
) -> #(WebPresenceRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.web_presence_order, record.id)
    || list.contains(staged.web_presence_order, record.id)
  let new_order = case already_known {
    True -> staged.web_presence_order
    False -> list.append(staged.web_presence_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      web_presences: dict.insert(staged.web_presences, record.id, record),
      web_presence_order: new_order,
      deleted_web_presence_ids: dict.delete(
        staged.deleted_web_presence_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_web_presence(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      web_presences: dict.delete(staged.web_presences, id),
      deleted_web_presence_ids: dict.insert(
        staged.deleted_web_presence_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

pub fn upsert_base_markets_root_payload(
  store: Store,
  key: String,
  payload: CapturedJsonValue,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      markets_root_payloads: dict.insert(
        base.markets_root_payloads,
        key,
        payload,
      ),
    ),
  )
}

pub fn get_effective_markets_root_payload(
  store: Store,
  key: String,
) -> Option(CapturedJsonValue) {
  case dict.get(store.staged_state.markets_root_payloads, key) {
    Ok(payload) -> Some(payload)
    Error(_) ->
      case dict.get(store.base_state.markets_root_payloads, key) {
        Ok(payload) -> Some(payload)
        Error(_) -> None
      }
  }
}

fn list_effective_ordered_records(
  base_order: List(String),
  staged_order: List(String),
  merged: Dict(String, a),
  by_id: fn(String) -> Option(a),
) -> List(a) {
  let ordered_ids = list.append(base_order, staged_order) |> dedupe_strings()
  let ordered =
    list.filter_map(ordered_ids, fn(id) { by_id(id) |> option_to_result })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) { by_id(id) |> option_to_result })
  list.append(ordered, unordered)
}

pub fn list_effective_selling_plan_groups_for_product(
  store: Store,
  product_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) { list.contains(group.product_ids, product_id) })
}

pub fn list_effective_selling_plan_groups_visible_for_product(
  store: Store,
  product_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_ids, product_id)
    || list.any(group.product_variant_ids, fn(variant_id) {
      case get_effective_variant_by_id(store, variant_id) {
        Some(variant) -> variant.product_id == product_id
        None -> False
      }
    })
  })
}

pub fn list_effective_selling_plan_groups_for_product_variant(
  store: Store,
  variant_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_variant_ids, variant_id)
  })
}

pub fn list_effective_selling_plan_groups_visible_for_product_variant(
  store: Store,
  variant_id: String,
) -> List(SellingPlanGroupRecord) {
  let product_id = case get_effective_variant_by_id(store, variant_id) {
    Some(variant) -> Some(variant.product_id)
    None -> None
  }
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_variant_ids, variant_id)
    || case product_id {
      Some(id) -> list.contains(group.product_ids, id)
      None -> False
    }
  })
}

fn remove_options_for_product(
  options: Dict(String, ProductOptionRecord),
  product_id: String,
) -> Dict(String, ProductOptionRecord) {
  options
  |> dict.keys()
  |> list.fold(options, fn(acc, id) {
    case dict.get(options, id) {
      Ok(option) ->
        case option.product_id == product_id {
          True -> dict.delete(acc, id)
          False -> acc
        }
      Error(_) -> acc
    }
  })
}

fn remove_variants_for_product(
  variants: Dict(String, ProductVariantRecord),
  product_id: String,
) -> Dict(String, ProductVariantRecord) {
  variants
  |> dict.keys()
  |> list.fold(variants, fn(acc, id) {
    case dict.get(variants, id) {
      Ok(variant) ->
        case variant.product_id == product_id {
          True -> dict.delete(acc, id)
          False -> acc
        }
      Error(_) -> acc
    }
  })
}

fn compare_product_options(
  left: ProductOptionRecord,
  right: ProductOptionRecord,
) -> order.Order {
  case int.compare(left.position, right.position) {
    order.Eq -> string.compare(left.id, right.id)
    other -> other
  }
}

fn product_is_deleted(store: Store, product_id: String) -> Bool {
  dict_has(store.base_state.deleted_product_ids, product_id)
  || dict_has(store.staged_state.deleted_product_ids, product_id)
}

fn has_staged_variant_family(store: Store, product_id: String) -> Bool {
  store.staged_state.product_variants
  |> dict.values()
  |> list.any(fn(variant) { variant.product_id == product_id })
}

fn list_variant_records(
  records: Dict(String, ProductVariantRecord),
  order: List(String),
) -> List(ProductVariantRecord) {
  let ordered_records =
    list.filter_map(order, fn(id) {
      case dict.get(records, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(order)
  let unordered_records =
    dict.keys(records)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      case dict.get(records, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Admin Platform utility slice
// ---------------------------------------------------------------------------

/// Seed or update the captured/effective backup region in base state.
pub fn upsert_base_backup_region(
  store: Store,
  record: BackupRegionRecord,
) -> Store {
  Store(
    ..store,
    base_state: BaseState(..store.base_state, backup_region: Some(record)),
  )
}

/// Stage the shop backup region. Mirrors `stageBackupRegion`.
pub fn stage_backup_region(
  store: Store,
  record: BackupRegionRecord,
) -> #(BackupRegionRecord, Store) {
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..store.staged_state,
        backup_region: Some(record),
      ),
    ),
  )
}

/// Return the staged backup region when present, otherwise the seeded base
/// region. The domain handler applies the no-shop captured fallback.
pub fn get_effective_backup_region(store: Store) -> Option(BackupRegionRecord) {
  case store.staged_state.backup_region {
    Some(region) -> Some(region)
    None -> store.base_state.backup_region
  }
}

pub fn upsert_base_admin_platform_generic_nodes(
  store: Store,
  records: List(AdminPlatformGenericNodeRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        admin_platform_generic_nodes: dict.insert(
          base.admin_platform_generic_nodes,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn get_effective_admin_platform_generic_node_by_id(
  store: Store,
  id: String,
) -> Option(AdminPlatformGenericNodeRecord) {
  case dict.get(store.staged_state.admin_platform_generic_nodes, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.admin_platform_generic_nodes, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_admin_platform_taxonomy_categories(
  store: Store,
  records: List(AdminPlatformTaxonomyCategoryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let existing = dict.get(base.admin_platform_taxonomy_categories, record.id)
    let merged = case existing {
      Ok(current) ->
        AdminPlatformTaxonomyCategoryRecord(
          ..record,
          cursor: record.cursor |> option.or(current.cursor),
        )
      Error(_) -> record
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        admin_platform_taxonomy_categories: dict.insert(
          base.admin_platform_taxonomy_categories,
          record.id,
          merged,
        ),
        admin_platform_taxonomy_category_order: append_unique_id(
          base.admin_platform_taxonomy_category_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_admin_platform_taxonomy_category_by_id(
  store: Store,
  id: String,
) -> Option(AdminPlatformTaxonomyCategoryRecord) {
  case dict.get(store.staged_state.admin_platform_taxonomy_categories, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.admin_platform_taxonomy_categories, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_admin_platform_taxonomy_categories(
  store: Store,
) -> List(AdminPlatformTaxonomyCategoryRecord) {
  let ordered_ids =
    list.append(
      store.base_state.admin_platform_taxonomy_category_order,
      store.staged_state.admin_platform_taxonomy_category_order,
    )
    |> dedupe_strings()
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_admin_platform_taxonomy_category_by_id(store, id)
      |> option_to_result
    })
  let ordered_lookup = list_to_set(ordered_ids)
  let unordered =
    dict.merge(
      store.base_state.admin_platform_taxonomy_categories,
      store.staged_state.admin_platform_taxonomy_categories,
    )
    |> dict.keys()
    |> list.filter(fn(id) { !dict_has(ordered_lookup, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      get_effective_admin_platform_taxonomy_category_by_id(store, id)
      |> option_to_result
    })
  list.append(ordered, unordered)
}

// ---------------------------------------------------------------------------
// Store properties slice
// ---------------------------------------------------------------------------

pub fn upsert_base_shop(store: Store, record: ShopRecord) -> Store {
  Store(..store, base_state: BaseState(..store.base_state, shop: Some(record)))
}

pub fn stage_shop(store: Store, record: ShopRecord) -> #(ShopRecord, Store) {
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(..store.staged_state, shop: Some(record)),
    ),
  )
}

pub fn get_effective_shop(store: Store) -> Option(ShopRecord) {
  case store.staged_state.shop {
    Some(shop) -> Some(shop)
    None -> store.base_state.shop
  }
}

// ---------------------------------------------------------------------------
// B2B company slice
// ---------------------------------------------------------------------------

pub fn upsert_base_b2b_company(
  store: Store,
  record: B2BCompanyRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_companies: dict.insert(base.b2b_companies, record.id, record),
      b2b_company_order: append_unique_id(base.b2b_company_order, record.id),
      deleted_b2b_company_ids: dict.delete(
        base.deleted_b2b_company_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_ids: dict.delete(
        staged.deleted_b2b_company_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company(
  store: Store,
  record: B2BCompanyRecord,
) -> #(B2BCompanyRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_order, record.id)
    || list.contains(staged.b2b_company_order, record.id)
    || dict_has(store.base_state.b2b_companies, record.id)
    || dict_has(staged.b2b_companies, record.id)
  let order = case known {
    True -> staged.b2b_company_order
    False -> list.append(staged.b2b_company_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_companies: dict.insert(staged.b2b_companies, record.id, record),
        b2b_company_order: order,
        deleted_b2b_company_ids: dict.delete(
          staged.deleted_b2b_company_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_companies: dict.delete(staged.b2b_companies, id),
      deleted_b2b_company_ids: dict.insert(
        staged.deleted_b2b_company_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_b2b_company_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyRecord) {
  case
    dict_has(store.staged_state.deleted_b2b_company_ids, id)
    || dict_has(store.base_state.deleted_b2b_company_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.b2b_companies, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.b2b_companies, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_b2b_companies(store: Store) -> List(B2BCompanyRecord) {
  let ids =
    append_unique_ids(
      store.base_state.b2b_company_order,
      store.staged_state.b2b_company_order,
    )
  let ordered =
    ids
    |> list.filter_map(fn(id) {
      case get_effective_b2b_company_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let seen =
    list.fold(ids, dict.new(), fn(acc, id) { dict.insert(acc, id, True) })
  let extras =
    dict.to_list(store.base_state.b2b_companies)
    |> list.append(dict.to_list(store.staged_state.b2b_companies))
    |> list.filter_map(fn(pair) {
      let #(id, _) = pair
      case dict_has(seen, id) {
        True -> Error(Nil)
        False ->
          case get_effective_b2b_company_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
      }
    })
    |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
  list.append(ordered, extras)
}

pub fn upsert_base_b2b_company_contact(
  store: Store,
  record: B2BCompanyContactRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_contacts: dict.insert(
        base.b2b_company_contacts,
        record.id,
        record,
      ),
      b2b_company_contact_order: append_unique_id(
        base.b2b_company_contact_order,
        record.id,
      ),
      deleted_b2b_company_contact_ids: dict.delete(
        base.deleted_b2b_company_contact_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_contact_ids: dict.delete(
        staged.deleted_b2b_company_contact_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_contact(
  store: Store,
  record: B2BCompanyContactRecord,
) -> #(B2BCompanyContactRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_contact_order, record.id)
    || list.contains(staged.b2b_company_contact_order, record.id)
    || dict_has(store.base_state.b2b_company_contacts, record.id)
    || dict_has(staged.b2b_company_contacts, record.id)
  let order = case known {
    True -> staged.b2b_company_contact_order
    False -> list.append(staged.b2b_company_contact_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_contacts: dict.insert(
          staged.b2b_company_contacts,
          record.id,
          record,
        ),
        b2b_company_contact_order: order,
        deleted_b2b_company_contact_ids: dict.delete(
          staged.deleted_b2b_company_contact_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_contact(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_contacts: dict.delete(staged.b2b_company_contacts, id),
      deleted_b2b_company_contact_ids: dict.insert(
        staged.deleted_b2b_company_contact_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_b2b_company_contact_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyContactRecord) {
  case
    dict_has(store.staged_state.deleted_b2b_company_contact_ids, id)
    || dict_has(store.base_state.deleted_b2b_company_contact_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.b2b_company_contacts, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.b2b_company_contacts, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_b2b_company_contacts(
  store: Store,
) -> List(B2BCompanyContactRecord) {
  append_unique_ids(
    store.base_state.b2b_company_contact_order,
    store.staged_state.b2b_company_contact_order,
  )
  |> list.filter_map(fn(id) {
    case get_effective_b2b_company_contact_by_id(store, id) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
}

pub fn upsert_base_b2b_company_contact_role(
  store: Store,
  record: B2BCompanyContactRoleRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_contact_roles: dict.insert(
        base.b2b_company_contact_roles,
        record.id,
        record,
      ),
      b2b_company_contact_role_order: append_unique_id(
        base.b2b_company_contact_role_order,
        record.id,
      ),
      deleted_b2b_company_contact_role_ids: dict.delete(
        base.deleted_b2b_company_contact_role_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_contact_role_ids: dict.delete(
        staged.deleted_b2b_company_contact_role_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_contact_role(
  store: Store,
  record: B2BCompanyContactRoleRecord,
) -> #(B2BCompanyContactRoleRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_contact_role_order, record.id)
    || list.contains(staged.b2b_company_contact_role_order, record.id)
    || dict_has(store.base_state.b2b_company_contact_roles, record.id)
    || dict_has(staged.b2b_company_contact_roles, record.id)
  let order = case known {
    True -> staged.b2b_company_contact_role_order
    False -> list.append(staged.b2b_company_contact_role_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_contact_roles: dict.insert(
          staged.b2b_company_contact_roles,
          record.id,
          record,
        ),
        b2b_company_contact_role_order: order,
        deleted_b2b_company_contact_role_ids: dict.delete(
          staged.deleted_b2b_company_contact_role_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_contact_role(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_contact_roles: dict.delete(
        staged.b2b_company_contact_roles,
        id,
      ),
      deleted_b2b_company_contact_role_ids: dict.insert(
        staged.deleted_b2b_company_contact_role_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_b2b_company_contact_role_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyContactRoleRecord) {
  case
    dict_has(store.staged_state.deleted_b2b_company_contact_role_ids, id)
    || dict_has(store.base_state.deleted_b2b_company_contact_role_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.b2b_company_contact_roles, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.b2b_company_contact_roles, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn upsert_base_b2b_company_location(
  store: Store,
  record: B2BCompanyLocationRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_locations: dict.insert(
        base.b2b_company_locations,
        record.id,
        record,
      ),
      b2b_company_location_order: append_unique_id(
        base.b2b_company_location_order,
        record.id,
      ),
      deleted_b2b_company_location_ids: dict.delete(
        base.deleted_b2b_company_location_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_location_ids: dict.delete(
        staged.deleted_b2b_company_location_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_location(
  store: Store,
  record: B2BCompanyLocationRecord,
) -> #(B2BCompanyLocationRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_location_order, record.id)
    || list.contains(staged.b2b_company_location_order, record.id)
    || dict_has(store.base_state.b2b_company_locations, record.id)
    || dict_has(staged.b2b_company_locations, record.id)
  let order = case known {
    True -> staged.b2b_company_location_order
    False -> list.append(staged.b2b_company_location_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_locations: dict.insert(
          staged.b2b_company_locations,
          record.id,
          record,
        ),
        b2b_company_location_order: order,
        deleted_b2b_company_location_ids: dict.delete(
          staged.deleted_b2b_company_location_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_location(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_locations: dict.delete(staged.b2b_company_locations, id),
      deleted_b2b_company_location_ids: dict.insert(
        staged.deleted_b2b_company_location_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_b2b_company_location_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyLocationRecord) {
  case
    dict_has(store.staged_state.deleted_b2b_company_location_ids, id)
    || dict_has(store.base_state.deleted_b2b_company_location_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.b2b_company_locations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.b2b_company_locations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_b2b_company_locations(
  store: Store,
) -> List(B2BCompanyLocationRecord) {
  let ids =
    append_unique_ids(
      store.base_state.b2b_company_location_order,
      store.staged_state.b2b_company_location_order,
    )
  let ordered =
    ids
    |> list.filter_map(fn(id) {
      case get_effective_b2b_company_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let seen =
    list.fold(ids, dict.new(), fn(acc, id) { dict.insert(acc, id, True) })
  let extras =
    dict.to_list(store.base_state.b2b_company_locations)
    |> list.append(dict.to_list(store.staged_state.b2b_company_locations))
    |> list.filter_map(fn(pair) {
      let #(id, _) = pair
      case dict_has(seen, id) {
        True -> Error(Nil)
        False ->
          case get_effective_b2b_company_location_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
      }
    })
    |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
  list.append(ordered, extras)
}

pub fn upsert_base_store_property_location(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      store_property_locations: dict.insert(
        base.store_property_locations,
        record.id,
        record,
      ),
      store_property_location_order: append_unique_id(
        base.store_property_location_order,
        record.id,
      ),
      deleted_store_property_location_ids: dict.delete(
        base.deleted_store_property_location_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_store_property_location_ids: dict.delete(
        staged.deleted_store_property_location_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_store_property_location(
  store: Store,
  record: StorePropertyRecord,
) -> #(StorePropertyRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let known =
    list.contains(base.store_property_location_order, record.id)
    || list.contains(staged.store_property_location_order, record.id)
    || dict_has(base.store_property_locations, record.id)
    || dict_has(staged.store_property_locations, record.id)
  let order = case known {
    True -> staged.store_property_location_order
    False -> list.append(staged.store_property_location_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        store_property_locations: dict.insert(
          staged.store_property_locations,
          record.id,
          record,
        ),
        store_property_location_order: order,
        deleted_store_property_location_ids: dict.delete(
          staged.deleted_store_property_location_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_store_property_location(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      store_property_locations: dict.delete(staged.store_property_locations, id),
      deleted_store_property_location_ids: dict.insert(
        staged.deleted_store_property_location_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_store_property_location_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  case
    dict_has(store.staged_state.deleted_store_property_location_ids, id)
    || dict_has(store.base_state.deleted_store_property_location_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.store_property_locations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.store_property_locations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_store_property_locations(
  store: Store,
) -> List(StorePropertyRecord) {
  let ordered_ids =
    append_unique_ids(
      store.base_state.store_property_location_order,
      store.staged_state.store_property_location_order,
    )
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      case get_effective_store_property_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_lookup =
    list.fold(ordered_ids, dict.new(), fn(acc, id) {
      dict.insert(acc, id, True)
    })
  let extras =
    dict.to_list(store.base_state.store_property_locations)
    |> list.append(dict.to_list(store.staged_state.store_property_locations))
    |> list.filter_map(fn(pair) {
      let #(id, _) = pair
      case dict_has(ordered_lookup, id) {
        True -> Error(Nil)
        False ->
          case get_effective_store_property_location_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
      }
    })
    |> sort_store_property_records
  list.append(ordered, extras)
}

pub fn upsert_base_business_entity(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      business_entities: dict.insert(base.business_entities, record.id, record),
      business_entity_order: append_unique_id(
        base.business_entity_order,
        record.id,
      ),
    ),
  )
}

pub fn get_business_entity_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  case dict.get(store.base_state.business_entities, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.staged_state.business_entities, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_business_entities(
  store: Store,
) -> List(StorePropertyRecord) {
  let ordered_ids =
    append_unique_ids(
      store.base_state.business_entity_order,
      store.staged_state.business_entity_order,
    )
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      case get_business_entity_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_lookup =
    list.fold(ordered_ids, dict.new(), fn(acc, id) {
      dict.insert(acc, id, True)
    })
  let extras =
    dict.to_list(store.base_state.business_entities)
    |> list.append(dict.to_list(store.staged_state.business_entities))
    |> list.filter_map(fn(pair) {
      let #(id, _) = pair
      case dict_has(ordered_lookup, id) {
        True -> Error(Nil)
        False ->
          case get_business_entity_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
      }
    })
    |> sort_store_property_records
  list.append(ordered, extras)
}

pub fn upsert_base_publishable(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      publishables: dict.insert(base.publishables, record.id, record),
      publishable_order: append_unique_id(base.publishable_order, record.id),
    ),
  )
}

pub fn upsert_staged_publishable(
  store: Store,
  record: StorePropertyRecord,
) -> #(StorePropertyRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let known =
    list.contains(base.publishable_order, record.id)
    || list.contains(staged.publishable_order, record.id)
    || dict_has(base.publishables, record.id)
    || dict_has(staged.publishables, record.id)
  let order = case known {
    True -> staged.publishable_order
    False -> list.append(staged.publishable_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        publishables: dict.insert(staged.publishables, record.id, record),
        publishable_order: order,
      ),
    ),
  )
}

pub fn get_effective_publishable_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  case dict.get(store.staged_state.publishables, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.publishables, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_store_property_mutation_payload(
  store: Store,
  record: StorePropertyMutationPayloadRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      store_property_mutation_payloads: dict.insert(
        base.store_property_mutation_payloads,
        record.key,
        record,
      ),
    ),
  )
}

pub fn get_store_property_mutation_payload(
  store: Store,
  key: String,
) -> Option(StorePropertyMutationPayloadRecord) {
  case dict.get(store.staged_state.store_property_mutation_payloads, key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.store_property_mutation_payloads, key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

fn append_unique_ids(left: List(String), right: List(String)) -> List(String) {
  list.fold(right, left, append_unique_id)
}

fn sort_store_property_records(
  records: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  list.sort(records, fn(a, b) { string.compare(a.id, b.id) })
}

// ---------------------------------------------------------------------------
// Metafields slice
// ---------------------------------------------------------------------------

pub fn replace_base_metafields_for_owner(
  store: Store,
  owner_id: String,
  metafields: List(ProductMetafieldRecord),
) -> Store {
  let base = store.base_state
  let retained =
    base.product_metafields
    |> dict.to_list
    |> list.filter(fn(pair) {
      let #(_, metafield) = pair
      metafield.owner_id != owner_id
    })
    |> dict.from_list
  let next_bucket =
    list.fold(metafields, retained, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(..store, base_state: BaseState(..base, product_metafields: next_bucket))
}

pub fn replace_staged_metafields_for_owner(
  store: Store,
  owner_id: String,
  metafields: List(ProductMetafieldRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    staged.product_metafields
    |> dict.to_list
    |> list.filter(fn(pair) {
      let #(_, metafield) = pair
      metafield.owner_id != owner_id
    })
    |> dict.from_list
  let next_bucket =
    list.fold(metafields, retained, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(
    ..store,
    staged_state: StagedState(..staged, product_metafields: next_bucket),
  )
}

pub fn upsert_base_metafield_definitions(
  store: Store,
  definitions: List(MetafieldDefinitionRecord),
) -> Store {
  list.fold(definitions, store, fn(acc, definition) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metafield_definitions: dict.insert(
          base.metafield_definitions,
          definition.id,
          definition,
        ),
        deleted_metafield_definition_ids: dict.delete(
          base.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metafield_definition_ids: dict.delete(
          staged.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metafield_definitions(
  store: Store,
  definitions: List(MetafieldDefinitionRecord),
) -> Store {
  list.fold(definitions, store, fn(acc, definition) {
    let staged = acc.staged_state
    Store(
      ..acc,
      staged_state: StagedState(
        ..staged,
        metafield_definitions: dict.insert(
          staged.metafield_definitions,
          definition.id,
          definition,
        ),
        deleted_metafield_definition_ids: dict.delete(
          staged.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
    )
  })
}

pub fn stage_delete_metafield_definition(
  store: Store,
  definition_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metafield_definitions: dict.delete(
        staged.metafield_definitions,
        definition_id,
      ),
      deleted_metafield_definition_ids: dict.insert(
        staged.deleted_metafield_definition_ids,
        definition_id,
        True,
      ),
    ),
  )
}

pub fn delete_product_metafields_for_definition(
  store: Store,
  definition: MetafieldDefinitionRecord,
) -> Store {
  case definition.owner_type {
    "PRODUCT" -> {
      let keep = fn(metafield: ProductMetafieldRecord) {
        !{
          metafield.owner_type == Some("PRODUCT")
          && metafield.namespace == definition.namespace
          && metafield.key == definition.key
        }
      }
      let base = store.base_state
      let staged = store.staged_state
      let base_bucket =
        base.product_metafields
        |> dict.to_list
        |> list.filter(fn(pair) {
          let #(_, metafield) = pair
          keep(metafield)
        })
        |> dict.from_list
      let staged_bucket =
        staged.product_metafields
        |> dict.to_list
        |> list.filter(fn(pair) {
          let #(_, metafield) = pair
          keep(metafield)
        })
        |> dict.from_list
      Store(
        ..store,
        base_state: BaseState(..base, product_metafields: base_bucket),
        staged_state: StagedState(..staged, product_metafields: staged_bucket),
      )
    }
    _ -> store
  }
}

pub fn get_effective_metafields_by_owner_id(
  store: Store,
  owner_id: String,
) -> List(ProductMetafieldRecord) {
  let staged =
    dict.values(store.staged_state.product_metafields)
    |> list.filter(fn(metafield) { metafield.owner_id == owner_id })
  let source = case staged {
    [] ->
      dict.values(store.base_state.product_metafields)
      |> list.filter(fn(metafield) { metafield.owner_id == owner_id })
    _ -> staged
  }
  source
  |> list.sort(fn(left, right) {
    case
      bool_compare(
        string.starts_with(left.namespace, "app--"),
        string.starts_with(right.namespace, "app--"),
      )
    {
      order.Eq -> compare_product_metafield_ids(left, right)
      other -> other
    }
  })
}

fn compare_product_metafield_ids(
  left: ProductMetafieldRecord,
  right: ProductMetafieldRecord,
) -> order.Order {
  case is_low_local_metafield_id(left), is_low_local_metafield_id(right) {
    True, False -> order.Gt
    False, True -> order.Lt
    _, _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

fn is_low_local_metafield_id(record: ProductMetafieldRecord) -> Bool {
  let has_draft_digest = case record.compare_digest {
    Some(digest) -> string.starts_with(digest, "draft:")
    None -> False
  }
  case has_draft_digest, metafield_id_tail(record.id) {
    True, Some(id) -> id < 1_000_000
    _, _ -> False
  }
}

fn metafield_id_tail(id: String) -> Option(Int) {
  case list.last(string.split(id, "/")) {
    Ok(tail) ->
      case int.parse(tail) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
    Error(_) -> None
  }
}

pub fn find_effective_metafield_by_id(
  store: Store,
  metafield_id: String,
) -> Option(ProductMetafieldRecord) {
  case dict.get(store.staged_state.product_metafields, metafield_id) {
    Ok(metafield) -> Some(metafield)
    Error(_) ->
      case dict.get(store.base_state.product_metafields, metafield_id) {
        Ok(metafield) -> Some(metafield)
        Error(_) -> None
      }
  }
}

pub fn list_effective_metafield_definitions(
  store: Store,
) -> List(MetafieldDefinitionRecord) {
  let merged =
    dict.merge(
      store.base_state.metafield_definitions,
      store.staged_state.metafield_definitions,
    )
  dict.values(merged)
  |> list.filter(fn(definition) {
    !dict_has(
      store.staged_state.deleted_metafield_definition_ids,
      definition.id,
    )
  })
  |> list.sort(fn(left, right) {
    case string_compare(left.owner_type, right.owner_type) {
      order.Eq ->
        case string_compare(left.namespace, right.namespace) {
          order.Eq ->
            case string_compare(left.key, right.key) {
              order.Eq -> string_compare(left.id, right.id)
              other -> other
            }
          other -> other
        }
      other -> other
    }
  })
}

pub fn get_effective_metafield_definition_by_id(
  store: Store,
  definition_id: String,
) -> Option(MetafieldDefinitionRecord) {
  case
    dict_has(store.staged_state.deleted_metafield_definition_ids, definition_id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.metafield_definitions, definition_id) {
        Ok(definition) -> Some(definition)
        Error(_) ->
          case dict.get(store.base_state.metafield_definitions, definition_id) {
            Ok(definition) -> Some(definition)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metafield_definition(
  store: Store,
  owner_type: String,
  namespace: String,
  key: String,
) -> Option(MetafieldDefinitionRecord) {
  list.find(list_effective_metafield_definitions(store), fn(definition) {
    definition.owner_type == owner_type
    && definition.namespace == namespace
    && definition.key == key
  })
  |> option.from_result
}

/// Stage a local Flow signature audit record.
pub fn stage_admin_platform_flow_signature(
  store: Store,
  record: AdminPlatformFlowSignatureRecord,
) -> #(AdminPlatformFlowSignatureRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(staged.admin_platform_flow_signature_order, record.id)
    || dict_has(staged.admin_platform_flow_signatures, record.id)
  let order = case known {
    True -> staged.admin_platform_flow_signature_order
    False ->
      list.append(staged.admin_platform_flow_signature_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        admin_platform_flow_signatures: dict.insert(
          staged.admin_platform_flow_signatures,
          record.id,
          record,
        ),
        admin_platform_flow_signature_order: order,
      ),
    ),
  )
}

/// Stage a local Flow trigger receipt audit record.
pub fn stage_admin_platform_flow_trigger(
  store: Store,
  record: AdminPlatformFlowTriggerRecord,
) -> #(AdminPlatformFlowTriggerRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(staged.admin_platform_flow_trigger_order, record.id)
    || dict_has(staged.admin_platform_flow_triggers, record.id)
  let order = case known {
    True -> staged.admin_platform_flow_trigger_order
    False -> list.append(staged.admin_platform_flow_trigger_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        admin_platform_flow_triggers: dict.insert(
          staged.admin_platform_flow_triggers,
          record.id,
          record,
        ),
        admin_platform_flow_trigger_order: order,
      ),
    ),
  )
}

// ---------------------------------------------------------------------------
// Saved-search slice
// ---------------------------------------------------------------------------

/// Upsert one or more saved-search records into the base state.
/// Mirrors `upsertBaseSavedSearches`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
pub fn upsert_base_saved_searches(
  store: Store,
  records: List(SavedSearchRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        saved_searches: dict.insert(base.saved_searches, record.id, record),
        saved_search_order: append_unique_id(base.saved_search_order, record.id),
        deleted_saved_search_ids: dict.delete(
          base.deleted_saved_search_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_saved_search_ids: dict.delete(
          staged.deleted_saved_search_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a saved-search record. Mirrors `upsertStagedSavedSearch`. The
/// TS version returns a fresh clone — Gleam values are already
/// immutable, so we return the record unchanged.
pub fn upsert_staged_saved_search(
  store: Store,
  record: SavedSearchRecord,
) -> #(SavedSearchRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.saved_search_order, record.id)
    || list.contains(staged.saved_search_order, record.id)
  let new_order = case already_known {
    True -> staged.saved_search_order
    False -> list.append(staged.saved_search_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      saved_searches: dict.insert(staged.saved_searches, record.id, record),
      saved_search_order: new_order,
      deleted_saved_search_ids: dict.delete(
        staged.deleted_saved_search_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a saved-search id as deleted. Mirrors
/// `deleteStagedSavedSearch`.
pub fn delete_staged_saved_search(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      saved_searches: dict.delete(staged.saved_searches, id),
      deleted_saved_search_ids: dict.insert(
        staged.deleted_saved_search_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective saved search for an id. Staged wins over base;
/// any "deleted" marker on either side suppresses the record.
/// Mirrors `getEffectiveSavedSearchById`.
pub fn get_effective_saved_search_by_id(
  store: Store,
  id: String,
) -> Option(SavedSearchRecord) {
  let deleted =
    dict_has(store.base_state.deleted_saved_search_ids, id)
    || dict_has(store.staged_state.deleted_saved_search_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.saved_searches, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.saved_searches, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective saved search the store knows about. Mirrors
/// `listEffectiveSavedSearches`. Ordered records (those tracked by the
/// `savedSearchOrder` arrays) come first, followed by any unordered
/// staged/base records sorted by id.
pub fn list_effective_saved_searches(store: Store) -> List(SavedSearchRecord) {
  let ordered_ids =
    list.append(
      store.base_state.saved_search_order,
      store.staged_state.saved_search_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.saved_searches,
      store.staged_state.saved_searches,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Webhook-subscription slice
// ---------------------------------------------------------------------------

/// Upsert one or more webhook-subscription records into the base state.
/// Mirrors `upsertBaseWebhookSubscriptions`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
pub fn upsert_base_webhook_subscriptions(
  store: Store,
  records: List(WebhookSubscriptionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        webhook_subscriptions: dict.insert(
          base.webhook_subscriptions,
          record.id,
          record,
        ),
        webhook_subscription_order: append_unique_id(
          base.webhook_subscription_order,
          record.id,
        ),
        deleted_webhook_subscription_ids: dict.delete(
          base.deleted_webhook_subscription_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_webhook_subscription_ids: dict.delete(
          staged.deleted_webhook_subscription_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a webhook-subscription record. Mirrors
/// `upsertStagedWebhookSubscription`. The TS version returns a fresh
/// clone — Gleam values are already immutable, so we return the record
/// unchanged.
pub fn upsert_staged_webhook_subscription(
  store: Store,
  record: WebhookSubscriptionRecord,
) -> #(WebhookSubscriptionRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.webhook_subscription_order, record.id)
    || list.contains(staged.webhook_subscription_order, record.id)
  let new_order = case already_known {
    True -> staged.webhook_subscription_order
    False -> list.append(staged.webhook_subscription_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      webhook_subscriptions: dict.insert(
        staged.webhook_subscriptions,
        record.id,
        record,
      ),
      webhook_subscription_order: new_order,
      deleted_webhook_subscription_ids: dict.delete(
        staged.deleted_webhook_subscription_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a webhook-subscription id as deleted. Mirrors
/// `deleteStagedWebhookSubscription`.
pub fn delete_staged_webhook_subscription(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      webhook_subscriptions: dict.delete(staged.webhook_subscriptions, id),
      deleted_webhook_subscription_ids: dict.insert(
        staged.deleted_webhook_subscription_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective webhook subscription for an id. Staged wins
/// over base; any "deleted" marker on either side suppresses the record.
/// Mirrors `getEffectiveWebhookSubscriptionById`.
pub fn get_effective_webhook_subscription_by_id(
  store: Store,
  id: String,
) -> Option(WebhookSubscriptionRecord) {
  let deleted =
    dict_has(store.base_state.deleted_webhook_subscription_ids, id)
    || dict_has(store.staged_state.deleted_webhook_subscription_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.webhook_subscriptions, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.webhook_subscriptions, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective webhook subscription the store knows about.
/// Mirrors `listEffectiveWebhookSubscriptions`. Ordered records (those
/// tracked by the `webhookSubscriptionOrder` arrays) come first,
/// followed by any unordered staged/base records sorted by id.
pub fn list_effective_webhook_subscriptions(
  store: Store,
) -> List(WebhookSubscriptionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.webhook_subscription_order,
      store.staged_state.webhook_subscription_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_webhook_subscription_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.webhook_subscriptions,
      store.staged_state.webhook_subscriptions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_webhook_subscription_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Online-store slices
// ---------------------------------------------------------------------------

pub fn upsert_base_online_store_content(
  store: Store,
  records: List(OnlineStoreContentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        online_store_content: dict.insert(
          base.online_store_content,
          record.id,
          record,
        ),
        online_store_content_order: append_unique_id(
          base.online_store_content_order,
          record.id,
        ),
        deleted_online_store_content_ids: dict.delete(
          base.deleted_online_store_content_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_online_store_content_ids: dict.delete(
          staged.deleted_online_store_content_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_online_store_content(
  store: Store,
  record: OnlineStoreContentRecord,
) -> #(OnlineStoreContentRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.online_store_content_order, record.id)
    || list.contains(staged.online_store_content_order, record.id)
  let new_order = case already_known {
    True -> staged.online_store_content_order
    False -> list.append(staged.online_store_content_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      online_store_content: dict.insert(
        staged.online_store_content,
        record.id,
        record,
      ),
      online_store_content_order: new_order,
      deleted_online_store_content_ids: dict.delete(
        staged.deleted_online_store_content_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_online_store_content(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      online_store_content: dict.delete(staged.online_store_content, id),
      deleted_online_store_content_ids: dict.insert(
        staged.deleted_online_store_content_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_online_store_content_by_id(
  store: Store,
  id: String,
) -> Option(OnlineStoreContentRecord) {
  let deleted =
    dict_has(store.base_state.deleted_online_store_content_ids, id)
    || dict_has(store.staged_state.deleted_online_store_content_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.online_store_content, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.online_store_content, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_online_store_content(
  store: Store,
  kind: String,
) -> List(OnlineStoreContentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.online_store_content_order,
      store.staged_state.online_store_content_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_online_store_content_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.online_store_content,
      store.staged_state.online_store_content,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_online_store_content_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_online_store_integrations(
  store: Store,
  records: List(OnlineStoreIntegrationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        online_store_integrations: dict.insert(
          base.online_store_integrations,
          record.id,
          record,
        ),
        online_store_integration_order: append_unique_id(
          base.online_store_integration_order,
          record.id,
        ),
        deleted_online_store_integration_ids: dict.delete(
          base.deleted_online_store_integration_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_online_store_integration_ids: dict.delete(
          staged.deleted_online_store_integration_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_online_store_integration(
  store: Store,
  record: OnlineStoreIntegrationRecord,
) -> #(OnlineStoreIntegrationRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.online_store_integration_order, record.id)
    || list.contains(staged.online_store_integration_order, record.id)
  let new_order = case already_known {
    True -> staged.online_store_integration_order
    False -> list.append(staged.online_store_integration_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      online_store_integrations: dict.insert(
        staged.online_store_integrations,
        record.id,
        record,
      ),
      online_store_integration_order: new_order,
      deleted_online_store_integration_ids: dict.delete(
        staged.deleted_online_store_integration_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_online_store_integration(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      online_store_integrations: dict.delete(
        staged.online_store_integrations,
        id,
      ),
      deleted_online_store_integration_ids: dict.insert(
        staged.deleted_online_store_integration_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_online_store_integration_by_id(
  store: Store,
  id: String,
) -> Option(OnlineStoreIntegrationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_online_store_integration_ids, id)
    || dict_has(store.staged_state.deleted_online_store_integration_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.online_store_integrations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.online_store_integrations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_online_store_integrations(
  store: Store,
  kind: String,
) -> List(OnlineStoreIntegrationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.online_store_integration_order,
      store.staged_state.online_store_integration_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_online_store_integration_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.online_store_integrations,
      store.staged_state.online_store_integrations,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_online_store_integration_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Apps slice (Pass 15)
// ---------------------------------------------------------------------------

/// Upsert an `AppRecord` into the base state. Used by hydration to seed
/// upstream-known apps. Mirrors `upsertBaseAppInstallation` (the app
/// half) and the implicit "stage app" the TS uses when the proxy mints
/// its own.
pub fn upsert_base_app(store: Store, record: AppRecord) -> Store {
  let base = store.base_state
  let new_base =
    BaseState(
      ..base,
      apps: dict.insert(base.apps, record.id, record),
      app_order: append_unique_id(base.app_order, record.id),
    )
  Store(..store, base_state: new_base)
}

/// Stage an `AppRecord`. The TS handler calls `stageApp` when it mints
/// a default app for a fresh proxy. Returns the record (unchanged in
/// Gleam since values are already immutable) alongside the new store.
pub fn stage_app(store: Store, record: AppRecord) -> #(AppRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.apps, record.id)
    || dict_has(staged.apps, record.id)
  let new_order = case already {
    True -> staged.app_order
    False -> list.append(staged.app_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      apps: dict.insert(staged.apps, record.id, record),
      app_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective app (staged-over-base). Mirrors
/// `getEffectiveAppById`.
pub fn get_effective_app_by_id(store: Store, id: String) -> Option(AppRecord) {
  case dict.get(store.staged_state.apps, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.apps, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Find an effective app whose `handle` matches the given value.
/// Mirrors `findEffectiveAppByHandle`. Staged wins on a tie.
pub fn find_effective_app_by_handle(
  store: Store,
  handle: String,
) -> Option(AppRecord) {
  case
    find_app_in_dict(store.staged_state.apps, fn(a) { a.handle == Some(handle) })
  {
    Some(record) -> Some(record)
    None ->
      find_app_in_dict(store.base_state.apps, fn(a) { a.handle == Some(handle) })
  }
}

/// Find an effective app whose `api_key` matches the given value.
/// Mirrors `findEffectiveAppByApiKey`.
pub fn find_effective_app_by_api_key(
  store: Store,
  api_key: String,
) -> Option(AppRecord) {
  case
    find_app_in_dict(store.staged_state.apps, fn(a) {
      a.api_key == Some(api_key)
    })
  {
    Some(record) -> Some(record)
    None ->
      find_app_in_dict(store.base_state.apps, fn(a) {
        a.api_key == Some(api_key)
      })
  }
}

/// List every effective app. Mirrors the implicit pattern of
/// `listEffectiveApps` (TS doesn't expose one but the same merge rules
/// apply).
pub fn list_effective_apps(store: Store) -> List(AppRecord) {
  let ordered_ids =
    list.append(store.base_state.app_order, store.staged_state.app_order)
    |> dedupe_strings()
  list.filter_map(ordered_ids, fn(id) {
    case get_effective_app_by_id(store, id) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
}

/// Upsert an installation + its app together. Mirrors
/// `upsertBaseAppInstallation`, which atomically writes both to base.
pub fn upsert_base_app_installation(
  store: Store,
  installation: AppInstallationRecord,
  app: AppRecord,
) -> Store {
  let store = upsert_base_app(store, app)
  let base = store.base_state
  let new_base =
    BaseState(
      ..base,
      app_installations: dict.insert(
        base.app_installations,
        installation.id,
        installation,
      ),
      app_installation_order: append_unique_id(
        base.app_installation_order,
        installation.id,
      ),
      current_installation_id: case base.current_installation_id {
        None -> Some(installation.id)
        existing -> existing
      },
    )
  Store(..store, base_state: new_base)
}

/// Stage an installation. Mirrors `stageAppInstallation`. If no
/// installation is registered as current, the new one becomes current.
pub fn stage_app_installation(
  store: Store,
  record: AppInstallationRecord,
) -> #(AppInstallationRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_installations, record.id)
    || dict_has(staged.app_installations, record.id)
  let new_order = case already {
    True -> staged.app_installation_order
    False -> list.append(staged.app_installation_order, [record.id])
  }
  let new_current = case
    staged.current_installation_id,
    store.base_state.current_installation_id
  {
    None, None -> Some(record.id)
    Some(_), _ -> staged.current_installation_id
    None, Some(_) -> staged.current_installation_id
  }
  let new_staged =
    StagedState(
      ..staged,
      app_installations: dict.insert(
        staged.app_installations,
        record.id,
        record,
      ),
      app_installation_order: new_order,
      current_installation_id: new_current,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective installation by id.
pub fn get_effective_app_installation_by_id(
  store: Store,
  id: String,
) -> Option(AppInstallationRecord) {
  case dict.get(store.staged_state.app_installations, id) {
    Ok(record) -> visible_app_installation(record)
    Error(_) ->
      case dict.get(store.base_state.app_installations, id) {
        Ok(record) -> visible_app_installation(record)
        Error(_) -> None
      }
  }
}

fn visible_app_installation(
  record: AppInstallationRecord,
) -> Option(AppInstallationRecord) {
  case record.uninstalled_at {
    Some(_) -> None
    None -> Some(record)
  }
}

/// Return the effective current installation, if one is registered.
/// Staged wins; falls back to base. Mirrors `getCurrentAppInstallation`.
pub fn get_current_app_installation(
  store: Store,
) -> Option(AppInstallationRecord) {
  case store.staged_state.current_installation_id {
    Some(id) -> get_effective_app_installation_by_id(store, id)
    None ->
      case store.base_state.current_installation_id {
        Some(id) -> get_effective_app_installation_by_id(store, id)
        None -> None
      }
  }
}

/// Stage an `AppSubscriptionRecord`. Mirrors `stageAppSubscription`.
pub fn stage_app_subscription(
  store: Store,
  record: AppSubscriptionRecord,
) -> #(AppSubscriptionRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_subscriptions, record.id)
    || dict_has(staged.app_subscriptions, record.id)
  let new_order = case already {
    True -> staged.app_subscription_order
    False -> list.append(staged.app_subscription_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_subscriptions: dict.insert(
        staged.app_subscriptions,
        record.id,
        record,
      ),
      app_subscription_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective subscription by id.
pub fn get_effective_app_subscription_by_id(
  store: Store,
  id: String,
) -> Option(AppSubscriptionRecord) {
  case dict.get(store.staged_state.app_subscriptions, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_subscriptions, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppSubscriptionLineItemRecord`. Mirrors
/// `stageAppSubscriptionLineItem`.
pub fn stage_app_subscription_line_item(
  store: Store,
  record: AppSubscriptionLineItemRecord,
) -> #(AppSubscriptionLineItemRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_subscription_line_items, record.id)
    || dict_has(staged.app_subscription_line_items, record.id)
  let new_order = case already {
    True -> staged.app_subscription_line_item_order
    False -> list.append(staged.app_subscription_line_item_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_subscription_line_items: dict.insert(
        staged.app_subscription_line_items,
        record.id,
        record,
      ),
      app_subscription_line_item_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a line item by id.
pub fn get_effective_app_subscription_line_item_by_id(
  store: Store,
  id: String,
) -> Option(AppSubscriptionLineItemRecord) {
  case dict.get(store.staged_state.app_subscription_line_items, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_subscription_line_items, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppOneTimePurchaseRecord`. Mirrors
/// `stageAppOneTimePurchase`.
pub fn stage_app_one_time_purchase(
  store: Store,
  record: AppOneTimePurchaseRecord,
) -> #(AppOneTimePurchaseRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_one_time_purchases, record.id)
    || dict_has(staged.app_one_time_purchases, record.id)
  let new_order = case already {
    True -> staged.app_one_time_purchase_order
    False -> list.append(staged.app_one_time_purchase_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_one_time_purchases: dict.insert(
        staged.app_one_time_purchases,
        record.id,
        record,
      ),
      app_one_time_purchase_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a one-time purchase by id.
pub fn get_effective_app_one_time_purchase_by_id(
  store: Store,
  id: String,
) -> Option(AppOneTimePurchaseRecord) {
  case dict.get(store.staged_state.app_one_time_purchases, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_one_time_purchases, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppUsageRecord`. Mirrors `stageAppUsageRecord`.
pub fn stage_app_usage_record(
  store: Store,
  record: AppUsageRecord,
) -> #(AppUsageRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_usage_records, record.id)
    || dict_has(staged.app_usage_records, record.id)
  let new_order = case already {
    True -> staged.app_usage_record_order
    False -> list.append(staged.app_usage_record_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_usage_records: dict.insert(
        staged.app_usage_records,
        record.id,
        record,
      ),
      app_usage_record_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a usage record by id.
pub fn get_effective_app_usage_record_by_id(
  store: Store,
  id: String,
) -> Option(AppUsageRecord) {
  case dict.get(store.staged_state.app_usage_records, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_usage_records, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective usage record attached to a given line item.
/// Mirrors `listEffectiveAppUsageRecordsForLineItem`. Staged-over-base.
pub fn list_effective_app_usage_records_for_line_item(
  store: Store,
  line_item_id: String,
) -> List(AppUsageRecord) {
  let ordered_ids =
    list.append(
      store.base_state.app_usage_record_order,
      store.staged_state.app_usage_record_order,
    )
    |> dedupe_strings()
  list.filter_map(ordered_ids, fn(id) {
    case get_effective_app_usage_record_by_id(store, id) {
      Some(record) ->
        case record.subscription_line_item_id == line_item_id {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

/// Stage a delegated access token. Mirrors `stageDelegatedAccessToken`.
pub fn stage_delegated_access_token(
  store: Store,
  record: DelegatedAccessTokenRecord,
) -> #(DelegatedAccessTokenRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.delegated_access_tokens, record.id)
    || dict_has(staged.delegated_access_tokens, record.id)
  let new_order = case already {
    True -> staged.delegated_access_token_order
    False -> list.append(staged.delegated_access_token_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      delegated_access_tokens: dict.insert(
        staged.delegated_access_tokens,
        record.id,
        record,
      ),
      delegated_access_token_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Find a delegated access token by sha256 hash. Mirrors
/// `findDelegatedAccessTokenByHash`. Searches staged before base.
pub fn find_delegated_access_token_by_hash(
  store: Store,
  hash: String,
) -> Option(DelegatedAccessTokenRecord) {
  case
    find_token_in_dict(store.staged_state.delegated_access_tokens, fn(t) {
      t.access_token_sha256 == hash && t.destroyed_at == None
    })
  {
    Some(record) -> Some(record)
    None ->
      find_token_in_dict(store.base_state.delegated_access_tokens, fn(t) {
        t.access_token_sha256 == hash && t.destroyed_at == None
      })
  }
}

/// Mark a delegated access token destroyed. Mirrors
/// `destroyDelegatedAccessToken`.
pub fn destroy_delegated_access_token(
  store: Store,
  id: String,
  destroyed_at: String,
) -> Store {
  case
    case dict.get(store.staged_state.delegated_access_tokens, id) {
      Ok(record) -> Some(record)
      Error(_) ->
        case dict.get(store.base_state.delegated_access_tokens, id) {
          Ok(record) -> Some(record)
          Error(_) -> None
        }
    }
  {
    None -> store
    Some(record) -> {
      let updated =
        types_mod.DelegatedAccessTokenRecord(
          ..record,
          destroyed_at: Some(destroyed_at),
        )
      let #(_, new_store) = stage_delegated_access_token(store, updated)
      new_store
    }
  }
}

// ---------------------------------------------------------------------------
// Functions domain (Pass 18)
// ---------------------------------------------------------------------------

/// Stage a `ShopifyFunctionRecord`. Mirrors `upsertStagedShopifyFunction`.
/// Functions cannot be deleted in the proxy — once a record is staged or
/// hydrated upstream, it stays.
pub fn upsert_staged_shopify_function(
  store: Store,
  record: ShopifyFunctionRecord,
) -> #(ShopifyFunctionRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.shopify_functions, record.id)
    || dict_has(staged.shopify_functions, record.id)
  let new_order = case already {
    True -> staged.shopify_function_order
    False -> list.append(staged.shopify_function_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      shopify_functions: dict.insert(
        staged.shopify_functions,
        record.id,
        record,
      ),
      shopify_function_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective `ShopifyFunctionRecord` (staged-over-base).
/// Mirrors `getEffectiveShopifyFunctionById`.
pub fn get_effective_shopify_function_by_id(
  store: Store,
  id: String,
) -> Option(ShopifyFunctionRecord) {
  case dict.get(store.staged_state.shopify_functions, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.shopify_functions, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective `ShopifyFunctionRecord`. Mirrors
/// `listEffectiveShopifyFunctions`. Ordered records first, then any
/// unordered ones sorted by id.
pub fn list_effective_shopify_functions(
  store: Store,
) -> List(ShopifyFunctionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.shopify_function_order,
      store.staged_state.shopify_function_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_shopify_function_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.shopify_functions,
      store.staged_state.shopify_functions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_shopify_function_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Marketing slice
// ---------------------------------------------------------------------------

pub fn upsert_base_marketing_activities(
  store: Store,
  records: List(MarketingRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        marketing_activities: dict.insert(
          base.marketing_activities,
          record.id,
          record,
        ),
        marketing_activity_order: append_unique_id(
          base.marketing_activity_order,
          record.id,
        ),
        deleted_marketing_activity_ids: dict.delete(
          base.deleted_marketing_activity_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_marketing_events(
  store: Store,
  records: List(MarketingRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        marketing_events: dict.insert(base.marketing_events, record.id, record),
        marketing_event_order: append_unique_id(
          base.marketing_event_order,
          record.id,
        ),
        deleted_marketing_event_ids: dict.delete(
          base.deleted_marketing_event_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_marketing_activity(
  store: Store,
  record: MarketingRecord,
) -> #(MarketingRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_activities: dict.insert(
        staged.marketing_activities,
        record.id,
        record,
      ),
      marketing_activity_order: append_unique_id(
        staged.marketing_activity_order,
        record.id,
      ),
      deleted_marketing_activity_ids: dict.delete(
        staged.deleted_marketing_activity_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_marketing_event(
  store: Store,
  record: MarketingRecord,
) -> #(MarketingRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_events: dict.insert(staged.marketing_events, record.id, record),
      marketing_event_order: append_unique_id(
        staged.marketing_event_order,
        record.id,
      ),
      deleted_marketing_event_ids: dict.delete(
        staged.deleted_marketing_event_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_delete_marketing_activity(store: Store, id: String) -> Store {
  let event_id = case get_effective_marketing_activity_record_by_id(store, id) {
    Some(record) -> read_marketing_event_id(record.data)
    None -> None
  }
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_activities: dict.delete(staged.marketing_activities, id),
      deleted_marketing_activity_ids: dict.insert(
        staged.deleted_marketing_activity_ids,
        id,
        True,
      ),
    )
  let next = case event_id {
    None -> next
    Some(event_id) ->
      StagedState(
        ..next,
        marketing_events: dict.delete(next.marketing_events, event_id),
        deleted_marketing_event_ids: dict.insert(
          next.deleted_marketing_event_ids,
          event_id,
          True,
        ),
      )
  }
  Store(..store, staged_state: next)
}

pub fn stage_delete_all_external_marketing_activities(
  store: Store,
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_activities(store),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case marketing_bool_field(record.data, "isExternal") {
          True -> #(
            [record.id, ..deleted],
            stage_delete_marketing_activity(current, record.id),
          )
          False -> acc
        }
      },
    )
  #(list.reverse(ids), next_store)
}

pub fn get_effective_marketing_activity_record_by_id(
  store: Store,
  id: String,
) -> Option(MarketingRecord) {
  case dict.get(store.staged_state.deleted_marketing_activity_ids, id) {
    Ok(_) -> None
    Error(_) ->
      case dict.get(store.staged_state.marketing_activities, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.marketing_activities, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_marketing_event_record_by_id(
  store: Store,
  id: String,
) -> Option(MarketingRecord) {
  case dict.get(store.staged_state.deleted_marketing_event_ids, id) {
    Ok(_) -> None
    Error(_) ->
      case dict.get(store.staged_state.marketing_events, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.marketing_events, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_marketing_activity_by_remote_id(
  store: Store,
  remote_id: String,
) -> Option(MarketingRecord) {
  list.find(list_effective_marketing_activities(store), fn(record) {
    read_marketing_remote_id(record.data) == Some(remote_id)
  })
  |> option.from_result
}

pub fn list_effective_marketing_activities(
  store: Store,
) -> List(MarketingRecord) {
  list_effective_marketing_records(
    store.base_state.marketing_activities,
    store.base_state.marketing_activity_order,
    store.staged_state.marketing_activities,
    store.staged_state.marketing_activity_order,
    store.staged_state.deleted_marketing_activity_ids,
  )
}

pub fn list_effective_marketing_events(store: Store) -> List(MarketingRecord) {
  list_effective_marketing_records(
    store.base_state.marketing_events,
    store.base_state.marketing_event_order,
    store.staged_state.marketing_events,
    store.staged_state.marketing_event_order,
    store.staged_state.deleted_marketing_event_ids,
  )
}

pub fn has_staged_marketing_records(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.staged_state.marketing_activities))
  || !list.is_empty(dict.keys(store.staged_state.marketing_events))
  || !list.is_empty(dict.keys(store.staged_state.marketing_engagements))
  || !list.is_empty(dict.keys(store.staged_state.deleted_marketing_activity_ids))
  || !list.is_empty(dict.keys(store.staged_state.deleted_marketing_event_ids))
  || !list.is_empty(dict.keys(
    store.staged_state.deleted_marketing_engagement_ids,
  ))
}

pub fn stage_marketing_engagement(
  store: Store,
  record: MarketingEngagementRecord,
) -> #(MarketingEngagementRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_engagements: dict.insert(
        staged.marketing_engagements,
        record.id,
        record,
      ),
      marketing_engagement_order: append_unique_id(
        staged.marketing_engagement_order,
        record.id,
      ),
      deleted_marketing_engagement_ids: dict.delete(
        staged.deleted_marketing_engagement_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_delete_marketing_engagement(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_engagements: dict.delete(staged.marketing_engagements, id),
      deleted_marketing_engagement_ids: dict.insert(
        staged.deleted_marketing_engagement_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: next)
}

pub fn stage_delete_marketing_engagements_by_channel_handle(
  store: Store,
  channel_handle: String,
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_engagements(store),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case record.channel_handle == Some(channel_handle) {
          True -> #(
            [record.id, ..deleted],
            stage_delete_marketing_engagement(current, record.id),
          )
          False -> acc
        }
      },
    )
  #(list.reverse(ids), next_store)
}

pub fn stage_delete_all_channel_marketing_engagements(
  store: Store,
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_engagements(store),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case record.channel_handle {
          Some(_) -> #(
            [record.id, ..deleted],
            stage_delete_marketing_engagement(current, record.id),
          )
          None -> acc
        }
      },
    )
  #(list.reverse(ids), next_store)
}

pub fn list_effective_marketing_engagements(
  store: Store,
) -> List(MarketingEngagementRecord) {
  let ordered_ids =
    list.append(
      store.base_state.marketing_engagement_order,
      store.staged_state.marketing_engagement_order,
    )
    |> dedupe_strings()
  let merged =
    dict.merge(
      store.base_state.marketing_engagements,
      store.staged_state.marketing_engagements,
    )
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      case dict.get(store.staged_state.deleted_marketing_engagement_ids, id) {
        Ok(_) -> Error(Nil)
        Error(_) ->
          case dict.get(merged, id) {
            Ok(record) -> Ok(record)
            Error(_) -> Error(Nil)
          }
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.values(merged)
    |> list.filter(fn(record) {
      !dict_has(ordered_set, record.id)
      && !dict_has(
        store.staged_state.deleted_marketing_engagement_ids,
        record.id,
      )
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

pub fn has_known_marketing_channel_handle(
  store: Store,
  handle: String,
) -> Bool {
  list.any(list_effective_marketing_events(store), fn(event) {
    read_marketing_channel_handle(event.data) == Some(handle)
  })
}

fn list_effective_marketing_records(
  base_bucket: Dict(String, MarketingRecord),
  base_order: List(String),
  staged_bucket: Dict(String, MarketingRecord),
  staged_order: List(String),
  deleted_bucket: Dict(String, Bool),
) -> List(MarketingRecord) {
  let ordered_ids = list.append(base_order, staged_order) |> dedupe_strings()
  let merged = dict.merge(base_bucket, staged_bucket)
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      case dict.get(deleted_bucket, id) {
        Ok(_) -> Error(Nil)
        Error(_) ->
          case dict.get(merged, id) {
            Ok(record) -> Ok(record)
            Error(_) -> Error(Nil)
          }
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.values(merged)
    |> list.filter(fn(record) {
      !dict_has(ordered_set, record.id) && !dict_has(deleted_bucket, record.id)
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

fn read_marketing_event_id(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case dict.get(data, "marketingEvent") {
    Ok(MarketingObject(event)) -> marketing_string_field(event, "id")
    _ -> None
  }
}

fn read_marketing_remote_id(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case marketing_string_field(data, "remoteId") {
    Some(id) -> Some(id)
    None ->
      case dict.get(data, "marketingEvent") {
        Ok(MarketingObject(event)) -> marketing_string_field(event, "remoteId")
        _ -> None
      }
  }
}

fn read_marketing_channel_handle(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case marketing_string_field(data, "channelHandle") {
    Some(handle) -> Some(handle)
    None ->
      case dict.get(data, "marketingEvent") {
        Ok(MarketingObject(event)) ->
          marketing_string_field(event, "channelHandle")
        _ -> None
      }
  }
}

fn marketing_string_field(
  data: Dict(String, MarketingValue),
  field: String,
) -> Option(String) {
  case dict.get(data, field) {
    Ok(MarketingString(value)) -> Some(value)
    _ -> None
  }
}

fn marketing_bool_field(
  data: Dict(String, MarketingValue),
  field: String,
) -> Bool {
  case dict.get(data, field) {
    Ok(types_mod.MarketingBool(value)) -> value
    _ -> False
  }
}

// ---------------------------------------------------------------------------
// Bulk-operations slice
// ---------------------------------------------------------------------------

/// Upsert BulkOperation records into base state. Mirrors
/// `upsertBaseBulkOperations`.
pub fn upsert_base_bulk_operations(
  store: Store,
  records: List(BulkOperationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        bulk_operations: dict.insert(base.bulk_operations, record.id, record),
        bulk_operation_order: append_unique_id(
          base.bulk_operation_order,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base)
  })
}

/// Stage a BulkOperation record. Mirrors `stageBulkOperation`.
pub fn stage_bulk_operation(
  store: Store,
  record: BulkOperationRecord,
) -> #(BulkOperationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.bulk_operation_order, record.id)
    || list.contains(staged.bulk_operation_order, record.id)
  let new_order = case already_known {
    True -> staged.bulk_operation_order
    False -> list.append(staged.bulk_operation_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      bulk_operations: dict.insert(staged.bulk_operations, record.id, record),
      bulk_operation_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Stage a BulkOperation and its generated result JSONL. The TS store
/// keeps result payloads in a sibling `bulkOperationResults` map; in
/// Gleam the not-yet-exposed result payload lives on the record.
pub fn stage_bulk_operation_result(
  store: Store,
  record: BulkOperationRecord,
  jsonl: String,
) -> #(BulkOperationRecord, Store) {
  stage_bulk_operation(
    store,
    BulkOperationRecord(..record, result_jsonl: Some(jsonl)),
  )
}

pub fn get_effective_bulk_operation_by_id(
  store: Store,
  id: String,
) -> Option(BulkOperationRecord) {
  case dict.get(store.staged_state.bulk_operations, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.bulk_operations, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_staged_bulk_operation_by_id(
  store: Store,
  id: String,
) -> Option(BulkOperationRecord) {
  case dict.get(store.staged_state.bulk_operations, id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

/// List effective BulkOperations. Ordered ids from base+staged come
/// first, then unordered ids sorted by createdAt descending / id
/// ascending, matching the TS store helper.
pub fn list_effective_bulk_operations(
  store: Store,
) -> List(BulkOperationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.bulk_operation_order,
      store.staged_state.bulk_operation_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_bulk_operation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.bulk_operations,
      store.staged_state.bulk_operations,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> {
          let date_order = string.compare(r.created_at, l.created_at)
          case date_order {
            order.Eq -> string_compare(l.id, r.id)
            _ -> date_order
          }
        }
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_bulk_operation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn get_effective_bulk_operation_result_jsonl(
  store: Store,
  id: String,
) -> Option(String) {
  case get_effective_bulk_operation_by_id(store, id) {
    Some(BulkOperationRecord(result_jsonl: Some(jsonl), ..)) -> Some(jsonl)
    _ -> None
  }
}

/// Cancel only a staged operation, matching TS
/// `cancelStagedBulkOperation`.
pub fn cancel_staged_bulk_operation(
  store: Store,
  id: String,
) -> #(Option(BulkOperationRecord), Store) {
  case get_staged_bulk_operation_by_id(store, id) {
    None -> #(None, store)
    Some(record) -> {
      let canceled =
        BulkOperationRecord(..record, status: "CANCELING", completed_at: None)
      let staged = store.staged_state
      let new_staged =
        StagedState(
          ..staged,
          bulk_operations: dict.insert(staged.bulk_operations, id, canceled),
        )
      #(Some(canceled), Store(..store, staged_state: new_staged))
    }
  }
}

pub fn has_bulk_operations(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.base_state.bulk_operations))
  || !list.is_empty(dict.keys(store.staged_state.bulk_operations))
}

pub fn has_staged_bulk_operations(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.staged_state.bulk_operations))
}

// ---------------------------------------------------------------------------
// Metaobjects slice
// ---------------------------------------------------------------------------

pub fn upsert_base_metaobject_definitions(
  store: Store,
  records: List(MetaobjectDefinitionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metaobject_definitions: dict.insert(
          base.metaobject_definitions,
          record.id,
          record,
        ),
        metaobject_definition_order: append_unique_id(
          base.metaobject_definition_order,
          record.id,
        ),
        deleted_metaobject_definition_ids: dict.delete(
          base.deleted_metaobject_definition_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metaobject_definition_ids: dict.delete(
          staged.deleted_metaobject_definition_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metaobject_definition(
  store: Store,
  record: MetaobjectDefinitionRecord,
) -> #(MetaobjectDefinitionRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.metaobject_definition_order, record.id)
    || list.contains(staged.metaobject_definition_order, record.id)
  let new_order = case already_known {
    True -> staged.metaobject_definition_order
    False -> list.append(staged.metaobject_definition_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      metaobject_definitions: dict.insert(
        staged.metaobject_definitions,
        record.id,
        record,
      ),
      metaobject_definition_order: new_order,
      deleted_metaobject_definition_ids: dict.delete(
        staged.deleted_metaobject_definition_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_metaobject_definition(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metaobject_definitions: dict.delete(staged.metaobject_definitions, id),
      deleted_metaobject_definition_ids: dict.insert(
        staged.deleted_metaobject_definition_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_metaobject_definition_by_id(
  store: Store,
  id: String,
) -> Option(MetaobjectDefinitionRecord) {
  case dict_has(store.staged_state.deleted_metaobject_definition_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.metaobject_definitions, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.metaobject_definitions, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metaobject_definition_by_type(
  store: Store,
  type_: String,
) -> Option(MetaobjectDefinitionRecord) {
  list.find(list_effective_metaobject_definitions(store), fn(record) {
    record.type_ == type_
  })
  |> option.from_result
}

pub fn list_effective_metaobject_definitions(
  store: Store,
) -> List(MetaobjectDefinitionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.metaobject_definition_order,
      store.staged_state.metaobject_definition_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_metaobject_definition_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.metaobject_definitions,
      store.staged_state.metaobject_definitions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> compare_metaobject_definitions(l, r)
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_metaobject_definition_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_metaobjects(
  store: Store,
  records: List(MetaobjectRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metaobjects: dict.insert(base.metaobjects, record.id, record),
        metaobject_order: append_unique_id(base.metaobject_order, record.id),
        deleted_metaobject_ids: dict.delete(
          base.deleted_metaobject_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metaobject_ids: dict.delete(
          staged.deleted_metaobject_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metaobject(
  store: Store,
  record: MetaobjectRecord,
) -> #(MetaobjectRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.metaobject_order, record.id)
    || list.contains(staged.metaobject_order, record.id)
  let new_order = case already_known {
    True -> staged.metaobject_order
    False -> list.append(staged.metaobject_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      metaobjects: dict.insert(staged.metaobjects, record.id, record),
      metaobject_order: new_order,
      deleted_metaobject_ids: dict.delete(
        staged.deleted_metaobject_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_metaobject(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metaobjects: dict.delete(staged.metaobjects, id),
      deleted_metaobject_ids: dict.insert(
        staged.deleted_metaobject_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_metaobject_by_id(
  store: Store,
  id: String,
) -> Option(MetaobjectRecord) {
  case dict_has(store.staged_state.deleted_metaobject_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.metaobjects, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.metaobjects, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metaobject_by_handle(
  store: Store,
  type_: String,
  handle: String,
) -> Option(MetaobjectRecord) {
  list.find(list_effective_metaobjects(store), fn(record) {
    record.type_ == type_ && record.handle == handle
  })
  |> option.from_result
}

pub fn list_effective_metaobjects(store: Store) -> List(MetaobjectRecord) {
  let ordered_ids =
    list.append(
      store.base_state.metaobject_order,
      store.staged_state.metaobject_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_metaobject_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.metaobjects, store.staged_state.metaobjects)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> compare_metaobjects(l, r)
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_metaobject_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn list_effective_metaobjects_by_type(
  store: Store,
  type_: String,
) -> List(MetaobjectRecord) {
  list.filter(list_effective_metaobjects(store), fn(record) {
    record.type_ == type_
  })
}

pub fn has_effective_metaobjects(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.base_state.metaobjects))
  || !list.is_empty(dict.keys(store.staged_state.metaobjects))
  || !list.is_empty(dict.keys(store.staged_state.deleted_metaobject_ids))
}

fn compare_metaobject_definitions(
  left: MetaobjectDefinitionRecord,
  right: MetaobjectDefinitionRecord,
) -> order.Order {
  case string.compare(left.type_, right.type_) {
    order.Eq -> string_compare(left.id, right.id)
    other -> other
  }
}

fn compare_metaobjects(
  left: MetaobjectRecord,
  right: MetaobjectRecord,
) -> order.Order {
  case string.compare(left.type_, right.type_) {
    order.Eq ->
      case string.compare(left.handle, right.handle) {
        order.Eq -> string_compare(left.id, right.id)
        other -> other
      }
    other -> other
  }
}

/// Stage a `ValidationRecord`. Mirrors `upsertStagedValidation`. Clears
/// any deletion marker the staged side may carry for the same id.
pub fn upsert_staged_validation(
  store: Store,
  record: ValidationRecord,
) -> #(ValidationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.validation_order, record.id)
    || list.contains(staged.validation_order, record.id)
  let new_order = case already_known {
    True -> staged.validation_order
    False -> list.append(staged.validation_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      validations: dict.insert(staged.validations, record.id, record),
      validation_order: new_order,
      deleted_validation_ids: dict.delete(
        staged.deleted_validation_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a validation id as deleted. Mirrors `deleteStagedValidation`.
pub fn delete_staged_validation(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      validations: dict.delete(staged.validations, id),
      deleted_validation_ids: dict.insert(
        staged.deleted_validation_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up an effective validation. Mirrors
/// `getEffectiveValidationById`.
pub fn get_effective_validation_by_id(
  store: Store,
  id: String,
) -> Option(ValidationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_validation_ids, id)
    || dict_has(store.staged_state.deleted_validation_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.validations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.validations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective validation. Mirrors `listEffectiveValidations`.
pub fn list_effective_validations(store: Store) -> List(ValidationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.validation_order,
      store.staged_state.validation_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_validation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.validations, store.staged_state.validations)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_validation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage a `CartTransformRecord`. Mirrors `upsertStagedCartTransform`.
pub fn upsert_staged_cart_transform(
  store: Store,
  record: CartTransformRecord,
) -> #(CartTransformRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.cart_transform_order, record.id)
    || list.contains(staged.cart_transform_order, record.id)
  let new_order = case already_known {
    True -> staged.cart_transform_order
    False -> list.append(staged.cart_transform_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      cart_transforms: dict.insert(staged.cart_transforms, record.id, record),
      cart_transform_order: new_order,
      deleted_cart_transform_ids: dict.delete(
        staged.deleted_cart_transform_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a cart-transform id as deleted. Mirrors
/// `deleteStagedCartTransform`.
pub fn delete_staged_cart_transform(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      cart_transforms: dict.delete(staged.cart_transforms, id),
      deleted_cart_transform_ids: dict.insert(
        staged.deleted_cart_transform_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up an effective cart-transform. Mirrors
/// `getEffectiveCartTransformById`.
pub fn get_effective_cart_transform_by_id(
  store: Store,
  id: String,
) -> Option(CartTransformRecord) {
  let deleted =
    dict_has(store.base_state.deleted_cart_transform_ids, id)
    || dict_has(store.staged_state.deleted_cart_transform_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.cart_transforms, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.cart_transforms, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective cart-transform. Mirrors
/// `listEffectiveCartTransforms`.
pub fn list_effective_cart_transforms(
  store: Store,
) -> List(CartTransformRecord) {
  let ordered_ids =
    list.append(
      store.base_state.cart_transform_order,
      store.staged_state.cart_transform_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_cart_transform_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.cart_transforms,
      store.staged_state.cart_transforms,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_cart_transform_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage the singleton tax-app configuration. Mirrors
/// `setStagedTaxAppConfiguration`. The TS proxy permits one
/// configuration per shop; here it lives as `Option` on staged state.
pub fn set_staged_tax_app_configuration(
  store: Store,
  record: TaxAppConfigurationRecord,
) -> Store {
  let staged = store.staged_state
  let new_staged = StagedState(..staged, tax_app_configuration: Some(record))
  Store(..store, staged_state: new_staged)
}

/// Read the effective tax-app configuration (staged-over-base).
/// Mirrors `getEffectiveTaxAppConfiguration`.
pub fn get_effective_tax_app_configuration(
  store: Store,
) -> Option(TaxAppConfigurationRecord) {
  case store.staged_state.tax_app_configuration {
    Some(record) -> Some(record)
    None -> store.base_state.tax_app_configuration
  }
}

// ---------------------------------------------------------------------------
// Discounts slice
// ---------------------------------------------------------------------------

pub fn upsert_base_discounts(
  store: Store,
  records: List(DiscountRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        discounts: dict.insert(base.discounts, record.id, record),
        discount_order: append_unique_id(base.discount_order, record.id),
        deleted_discount_ids: dict.delete(base.deleted_discount_ids, record.id),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_discount_ids: dict.delete(
          staged.deleted_discount_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn stage_discount(
  store: Store,
  record: DiscountRecord,
) -> #(DiscountRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.discount_order, record.id)
    || list.contains(staged.discount_order, record.id)
  let new_order = case already_known {
    True -> staged.discount_order
    False -> list.append(staged.discount_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      discounts: dict.insert(staged.discounts, record.id, record),
      discount_order: new_order,
      deleted_discount_ids: dict.delete(staged.deleted_discount_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_discount(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      discounts: dict.delete(staged.discounts, id),
      deleted_discount_ids: dict.insert(staged.deleted_discount_ids, id, True),
    ),
  )
}

pub fn stage_discount_bulk_operation(
  store: Store,
  record: DiscountBulkOperationRecord,
) -> #(DiscountBulkOperationRecord, Store) {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      discount_bulk_operations: dict.insert(
        staged.discount_bulk_operations,
        record.id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn get_effective_discount_by_id(
  store: Store,
  id: String,
) -> Option(DiscountRecord) {
  let deleted =
    dict_has(store.base_state.deleted_discount_ids, id)
    || dict_has(store.staged_state.deleted_discount_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.discounts, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.discounts, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_discounts(store: Store) -> List(DiscountRecord) {
  let ordered_ids =
    list.append(
      store.base_state.discount_order,
      store.staged_state.discount_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_discount_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.discounts, store.staged_state.discounts)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_discount_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn find_effective_discount_by_code(
  store: Store,
  code: String,
) -> Option(DiscountRecord) {
  let wanted = string.lowercase(code)
  case
    list.find(list_effective_discounts(store), fn(record) {
      case record.code {
        Some(record_code) -> string.lowercase(record_code) == wanted
        None -> False
      }
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

// ---------------------------------------------------------------------------
// Gift card slice (Pass 19)
// ---------------------------------------------------------------------------

/// Upsert one or more gift-card records into the base state.
/// Mirrors `upsertBaseGiftCards`.
pub fn upsert_base_gift_cards(
  store: Store,
  records: List(GiftCardRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        gift_cards: dict.insert(base.gift_cards, record.id, record),
        gift_card_order: append_unique_id(base.gift_card_order, record.id),
      )
    Store(..acc, base_state: new_base)
  })
}

/// Upsert the singleton base gift-card configuration.
/// Mirrors `upsertBaseGiftCardConfiguration`.
pub fn upsert_base_gift_card_configuration(
  store: Store,
  record: GiftCardConfigurationRecord,
) -> Store {
  let base = store.base_state
  let new_base = BaseState(..base, gift_card_configuration: Some(record))
  Store(..store, base_state: new_base)
}

/// Stage a freshly minted `GiftCardRecord`. Mirrors
/// `stageCreateGiftCard` — appends the id to staged order on first
/// sight, otherwise leaves the order alone (idempotent re-stage).
pub fn stage_create_gift_card(
  store: Store,
  record: GiftCardRecord,
) -> #(GiftCardRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.gift_card_order, record.id)
    || list.contains(staged.gift_card_order, record.id)
  let new_order = case already_known {
    True -> staged.gift_card_order
    False -> list.append(staged.gift_card_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      gift_cards: dict.insert(staged.gift_cards, record.id, record),
      gift_card_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Stage an updated `GiftCardRecord`. Mirrors `stageUpdateGiftCard`.
/// Same semantics as `stage_create_gift_card` since gift cards are
/// never deleted (deactivation flips a flag instead).
pub fn stage_update_gift_card(
  store: Store,
  record: GiftCardRecord,
) -> #(GiftCardRecord, Store) {
  stage_create_gift_card(store, record)
}

/// Look up the effective gift card for an id (staged-over-base).
/// Mirrors `getEffectiveGiftCardById`.
pub fn get_effective_gift_card_by_id(
  store: Store,
  id: String,
) -> Option(GiftCardRecord) {
  case dict.get(store.staged_state.gift_cards, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.gift_cards, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective gift card. Mirrors `listEffectiveGiftCards`.
/// Ordered records first (`giftCardOrder`), then any unordered records
/// sorted by id.
pub fn list_effective_gift_cards(store: Store) -> List(GiftCardRecord) {
  let ordered_ids =
    list.append(
      store.base_state.gift_card_order,
      store.staged_state.gift_card_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_gift_card_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.gift_cards, store.staged_state.gift_cards)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_gift_card_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage the singleton gift-card configuration. Mirrors
/// `setStagedGiftCardConfiguration`.
pub fn set_staged_gift_card_configuration(
  store: Store,
  record: GiftCardConfigurationRecord,
) -> Store {
  let staged = store.staged_state
  let new_staged = StagedState(..staged, gift_card_configuration: Some(record))
  Store(..store, staged_state: new_staged)
}

/// Read the effective gift-card configuration (staged-over-base).
/// Mirrors `getEffectiveGiftCardConfiguration`. Returns the proxy's
/// default (CAD 0.0 limits) when neither side has staged a
/// configuration — matches the TS fallback.
pub fn get_effective_gift_card_configuration(
  store: Store,
) -> GiftCardConfigurationRecord {
  case store.staged_state.gift_card_configuration {
    Some(record) -> record
    None ->
      case store.base_state.gift_card_configuration {
        Some(record) -> record
        None -> default_gift_card_configuration()
      }
  }
}

fn default_gift_card_configuration() -> GiftCardConfigurationRecord {
  types_mod.GiftCardConfigurationRecord(
    issue_limit: types_mod.Money(amount: "0.0", currency_code: "CAD"),
    purchase_limit: types_mod.Money(amount: "0.0", currency_code: "CAD"),
  )
}

// ---------------------------------------------------------------------------
// Customers slice
// ---------------------------------------------------------------------------

pub fn upsert_base_customers(
  store: Store,
  records: List(CustomerRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customers: dict.insert(base.customers, record.id, record),
        customer_order: append_unique_id(base.customer_order, record.id),
        deleted_customer_ids: dict.delete(base.deleted_customer_ids, record.id),
        merged_customer_ids: dict.delete(base.merged_customer_ids, record.id),
      ),
    )
  })
}

pub fn set_base_customer_catalog_connection(
  store: Store,
  key: String,
  connection: CustomerCatalogConnectionRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_catalog_connections: dict.insert(
        base.customer_catalog_connections,
        key,
        connection,
      ),
    ),
  )
}

pub fn get_base_customer_catalog_connection(
  store: Store,
  key: String,
) -> Option(CustomerCatalogConnectionRecord) {
  case dict.get(store.base_state.customer_catalog_connections, key) {
    Ok(connection) -> Some(connection)
    Error(_) -> None
  }
}

pub fn stage_create_customer(
  store: Store,
  record: CustomerRecord,
) -> #(CustomerRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.customer_order, record.id)
    || list.contains(staged.customer_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_order
    False -> list.append(staged.customer_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customers: dict.insert(staged.customers, record.id, record),
      customer_order: new_order,
      deleted_customer_ids: dict.delete(staged.deleted_customer_ids, record.id),
      merged_customer_ids: dict.delete(staged.merged_customer_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn stage_update_customer(
  store: Store,
  record: CustomerRecord,
) -> #(CustomerRecord, Store) {
  stage_create_customer(store, record)
}

pub fn stage_delete_customer(store: Store, customer_id: String) -> Store {
  let staged = store.staged_state
  let staged_addresses =
    dict.filter(staged.customer_addresses, fn(_id, address) {
      address.customer_id != customer_id
    })
  let deleted_address_ids =
    dict.to_list(store.base_state.customer_addresses)
    |> list.fold(staged.deleted_customer_address_ids, fn(acc, pair) {
      let #(id, address) = pair
      case address.customer_id == customer_id {
        True -> dict.insert(acc, id, True)
        False -> acc
      }
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customers: dict.delete(staged.customers, customer_id),
      customer_addresses: staged_addresses,
      deleted_customer_address_ids: deleted_address_ids,
      deleted_customer_ids: dict.insert(
        staged.deleted_customer_ids,
        customer_id,
        True,
      ),
      merged_customer_ids: dict.delete(staged.merged_customer_ids, customer_id),
    ),
  )
}

pub fn get_effective_customer_by_id(
  store: Store,
  customer_id: String,
) -> Option(CustomerRecord) {
  case dict.get(store.staged_state.deleted_customer_ids, customer_id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.customers, customer_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.customers, customer_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_customers(store: Store) -> List(CustomerRecord) {
  let ordered_ids =
    list.append(
      store.base_state.customer_order,
      store.staged_state.customer_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_customer_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.customers, store.staged_state.customers)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_customer_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_customer_addresses(
  store: Store,
  records: List(CustomerAddressRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_addresses: dict.insert(
          base.customer_addresses,
          record.id,
          record,
        ),
        customer_address_order: append_unique_id(
          base.customer_address_order,
          record.id,
        ),
        deleted_customer_address_ids: dict.delete(
          base.deleted_customer_address_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_customer_address(
  store: Store,
  record: CustomerAddressRecord,
) -> #(CustomerAddressRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.customer_address_order, record.id)
    || list.contains(staged.customer_address_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_address_order
    False -> list.append(staged.customer_address_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customer_addresses: dict.insert(
        staged.customer_addresses,
        record.id,
        record,
      ),
      customer_address_order: new_order,
      deleted_customer_address_ids: dict.delete(
        staged.deleted_customer_address_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn stage_delete_customer_address(
  store: Store,
  address_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customer_addresses: dict.delete(staged.customer_addresses, address_id),
      deleted_customer_address_ids: dict.insert(
        staged.deleted_customer_address_ids,
        address_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_customer_address_by_id(
  store: Store,
  address_id: String,
) -> Option(CustomerAddressRecord) {
  case dict.get(store.staged_state.deleted_customer_address_ids, address_id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.customer_addresses, address_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.customer_addresses, address_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_customer_addresses(
  store: Store,
  customer_id: String,
) -> List(CustomerAddressRecord) {
  case dict.get(store.staged_state.deleted_customer_ids, customer_id) {
    Ok(True) -> []
    _ -> {
      let ids =
        list.append(
          store.base_state.customer_address_order,
          store.staged_state.customer_address_order,
        )
        |> dedupe_strings()
      let from_order =
        list.filter_map(ids, fn(id) {
          case get_effective_customer_address_by_id(store, id) {
            Some(address) ->
              case address.customer_id == customer_id {
                True -> Ok(address)
                False -> Error(Nil)
              }
            None -> Error(Nil)
          }
        })
      let ordered_set = list_to_set(ids)
      let merged =
        dict.merge(
          store.base_state.customer_addresses,
          store.staged_state.customer_addresses,
        )
      let unordered =
        dict.keys(merged)
        |> list.filter(fn(id) { !dict_has(ordered_set, id) })
        |> list.sort(string_compare)
        |> list.filter_map(fn(id) {
          case get_effective_customer_address_by_id(store, id) {
            Some(address) ->
              case address.customer_id == customer_id {
                True -> Ok(address)
                False -> Error(Nil)
              }
            None -> Error(Nil)
          }
        })
      let effective = list.append(from_order, unordered)
      case list.any(effective, fn(address) { address.position < 0 }) {
        True ->
          list.sort(effective, fn(a, b) {
            case a.position < 0, b.position < 0 {
              True, True -> int.compare(a.position, b.position)
              True, False -> order.Lt
              False, True -> order.Gt
              False, False -> order.Eq
            }
          })
        False -> effective
      }
    }
  }
}

pub fn upsert_base_customer_order_summaries(
  store: Store,
  records: List(CustomerOrderSummaryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_order_summaries: dict.insert(
          base.customer_order_summaries,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn stage_customer_order_summary(
  store: Store,
  record: CustomerOrderSummaryRecord,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customer_order_summaries: dict.insert(
        staged.customer_order_summaries,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_customer_order_summary_by_id(
  store: Store,
  order_id: String,
) -> Option(CustomerOrderSummaryRecord) {
  case dict.get(store.staged_state.customer_order_summaries, order_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_order_summaries, order_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_customer_order_summaries(
  store: Store,
  customer_id: String,
) -> List(CustomerOrderSummaryRecord) {
  dict.keys(dict.merge(
    store.base_state.customer_order_summaries,
    store.staged_state.customer_order_summaries,
  ))
  |> list.sort(string_compare)
  |> list.filter_map(fn(id) {
    case get_effective_customer_order_summary_by_id(store, id) {
      Some(record) ->
        case record.customer_id == Some(customer_id) {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn set_base_customer_order_connection_page_info(
  store: Store,
  customer_id: String,
  page_info: CustomerCatalogPageInfoRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_order_connection_page_infos: dict.insert(
        base.customer_order_connection_page_infos,
        customer_id,
        page_info,
      ),
    ),
  )
}

pub fn get_effective_customer_order_connection_page_info(
  store: Store,
  customer_id: String,
) -> Option(CustomerCatalogPageInfoRecord) {
  case
    dict.get(
      store.staged_state.customer_order_connection_page_infos,
      customer_id,
    )
  {
    Ok(info) -> Some(info)
    Error(_) ->
      case
        dict.get(
          store.base_state.customer_order_connection_page_infos,
          customer_id,
        )
      {
        Ok(info) -> Some(info)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_customer_event_summaries(
  store: Store,
  records: List(CustomerEventSummaryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_event_summaries: dict.insert(
          base.customer_event_summaries,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn list_effective_customer_event_summaries(
  store: Store,
  customer_id: String,
) -> List(CustomerEventSummaryRecord) {
  dict.values(dict.merge(
    store.base_state.customer_event_summaries,
    store.staged_state.customer_event_summaries,
  ))
  |> list.filter(fn(event) { event.customer_id == customer_id })
  |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
}

pub fn set_base_customer_event_connection_page_info(
  store: Store,
  customer_id: String,
  page_info: CustomerCatalogPageInfoRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_event_connection_page_infos: dict.insert(
        base.customer_event_connection_page_infos,
        customer_id,
        page_info,
      ),
    ),
  )
}

pub fn get_effective_customer_event_connection_page_info(
  store: Store,
  customer_id: String,
) -> Option(CustomerCatalogPageInfoRecord) {
  case
    dict.get(
      store.staged_state.customer_event_connection_page_infos,
      customer_id,
    )
  {
    Ok(info) -> Some(info)
    Error(_) ->
      case
        dict.get(
          store.base_state.customer_event_connection_page_infos,
          customer_id,
        )
      {
        Ok(info) -> Some(info)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_customer_last_orders(
  store: Store,
  records: List(#(String, CustomerOrderSummaryRecord)),
) -> Store {
  list.fold(records, store, fn(acc, pair) {
    let #(customer_id, record) = pair
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_last_orders: dict.insert(
          base.customer_last_orders,
          customer_id,
          record,
        ),
      ),
    )
  })
}

pub fn get_effective_customer_last_order(
  store: Store,
  customer_id: String,
) -> Option(CustomerOrderSummaryRecord) {
  case dict.get(store.staged_state.customer_last_orders, customer_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_last_orders, customer_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn stage_customer_metafields(
  store: Store,
  customer_id: String,
  records: List(CustomerMetafieldRecord),
) -> Store {
  let staged_without_customer =
    dict.filter(store.staged_state.customer_metafields, fn(_id, metafield) {
      metafield.customer_id != customer_id
    })
  let new_metafields =
    list.fold(records, staged_without_customer, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_metafields: new_metafields,
    ),
  )
}

pub fn get_effective_metafields_by_customer_id(
  store: Store,
  customer_id: String,
) -> List(CustomerMetafieldRecord) {
  let staged =
    dict.values(store.staged_state.customer_metafields)
    |> list.filter(fn(m) { m.customer_id == customer_id })
    |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
  case staged {
    [] ->
      dict.values(store.base_state.customer_metafields)
      |> list.filter(fn(m) { m.customer_id == customer_id })
      |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
    _ -> staged
  }
}

pub fn stage_customer_payment_method(
  store: Store,
  record: CustomerPaymentMethodRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_payment_methods: dict.insert(
        store.staged_state.customer_payment_methods,
        record.id,
        record,
      ),
      deleted_customer_payment_method_ids: dict.delete(
        store.staged_state.deleted_customer_payment_method_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_base_customer_payment_methods(
  store: Store,
  records: List(CustomerPaymentMethodRecord),
) -> Store {
  list.fold(records, store, fn(current, record) {
    Store(
      ..current,
      base_state: BaseState(
        ..current.base_state,
        customer_payment_methods: dict.insert(
          current.base_state.customer_payment_methods,
          record.id,
          record,
        ),
        deleted_customer_payment_method_ids: dict.delete(
          current.base_state.deleted_customer_payment_method_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_customer_payment_method_ids: dict.delete(
          current.staged_state.deleted_customer_payment_method_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_customer_payment_method_update_url(
  store: Store,
  record: CustomerPaymentMethodUpdateUrlRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_payment_method_update_urls: dict.insert(
        store.staged_state.customer_payment_method_update_urls,
        record.id,
        record,
      ),
    ),
  )
}

pub fn stage_payment_reminder_send(
  store: Store,
  record: PaymentReminderSendRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_reminder_sends: dict.insert(
        store.staged_state.payment_reminder_sends,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_customer_payment_method_by_id(
  store: Store,
  payment_method_id: String,
  show_revoked: Bool,
) -> Option(CustomerPaymentMethodRecord) {
  case
    dict.get(
      store.staged_state.deleted_customer_payment_method_ids,
      payment_method_id,
    )
  {
    Ok(True) -> None
    _ -> {
      let found = case
        dict.get(store.staged_state.customer_payment_methods, payment_method_id)
      {
        Ok(record) -> Some(record)
        Error(_) ->
          case
            dict.get(
              store.base_state.customer_payment_methods,
              payment_method_id,
            )
          {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
      case found {
        Some(record) ->
          case
            get_effective_customer_by_id(store, record.customer_id),
            record.revoked_at
          {
            None, _ -> None
            _, Some(_) if !show_revoked -> None
            _, _ -> Some(record)
          }
        None -> None
      }
    }
  }
}

pub fn list_effective_customer_payment_methods(
  store: Store,
  customer_id: String,
  show_revoked: Bool,
) -> List(CustomerPaymentMethodRecord) {
  let ids =
    dict.keys(dict.merge(
      store.base_state.customer_payment_methods,
      store.staged_state.customer_payment_methods,
    ))
    |> list.sort(string_compare)
  list.filter_map(ids, fn(id) {
    case get_effective_customer_payment_method_by_id(store, id, show_revoked) {
      Some(record) ->
        case record.customer_id == customer_id {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn upsert_base_payment_customizations(
  store: Store,
  records: List(PaymentCustomizationRecord),
) -> Store {
  list.fold(records, store, fn(current, record) {
    Store(
      ..current,
      base_state: BaseState(
        ..current.base_state,
        payment_customizations: dict.insert(
          current.base_state.payment_customizations,
          record.id,
          record,
        ),
        payment_customization_order: append_unique_id(
          current.base_state.payment_customization_order,
          record.id,
        ),
        deleted_payment_customization_ids: dict.delete(
          current.base_state.deleted_payment_customization_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_payment_customization_ids: dict.delete(
          current.staged_state.deleted_payment_customization_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_payment_customization(
  store: Store,
  record: PaymentCustomizationRecord,
) -> Store {
  let staged_order = case
    list.contains(store.base_state.payment_customization_order, record.id)
    || list.contains(store.staged_state.payment_customization_order, record.id)
  {
    True -> store.staged_state.payment_customization_order
    False ->
      list.append(store.staged_state.payment_customization_order, [record.id])
  }
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_customizations: dict.insert(
        store.staged_state.payment_customizations,
        record.id,
        record,
      ),
      payment_customization_order: staged_order,
      deleted_payment_customization_ids: dict.delete(
        store.staged_state.deleted_payment_customization_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_payment_customization(store: Store, id: String) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_customizations: dict.delete(
        store.staged_state.payment_customizations,
        id,
      ),
      deleted_payment_customization_ids: dict.insert(
        store.staged_state.deleted_payment_customization_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_payment_customization_by_id(
  store: Store,
  id: String,
) -> Option(PaymentCustomizationRecord) {
  case dict.get(store.staged_state.deleted_payment_customization_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.payment_customizations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.payment_customizations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_payment_customizations(
  store: Store,
) -> List(PaymentCustomizationRecord) {
  let ordered_ids =
    append_unique_ids(
      store.base_state.payment_customization_order,
      store.staged_state.payment_customization_order,
    )
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      case get_effective_payment_customization_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(dict.merge(
      store.base_state.payment_customizations,
      store.staged_state.payment_customizations,
    ))
    |> list.filter(fn(record) {
      !list.contains(ordered_ids, record.id)
      && case
        dict.get(
          store.staged_state.deleted_payment_customization_ids,
          record.id,
        )
      {
        Ok(True) -> False
        _ -> True
      }
    })
    |> list.sort(fn(a, b) {
      resource_ids.compare_shopify_resource_ids(a.id, b.id)
    })
  list.append(ordered, unordered)
}

pub fn has_payment_customizations(store: Store) -> Bool {
  dict.size(store.base_state.payment_customizations) > 0
  || dict.size(store.staged_state.payment_customizations) > 0
  || dict.size(store.staged_state.deleted_payment_customization_ids) > 0
}

pub fn register_payment_terms_owner(store: Store, owner_id: String) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      payment_terms_owner_ids: dict.insert(
        store.base_state.payment_terms_owner_ids,
        owner_id,
        True,
      ),
    ),
  )
}

pub fn upsert_staged_payment_terms(
  store: Store,
  record: PaymentTermsRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_terms: dict.insert(
        store.staged_state.payment_terms,
        record.id,
        record,
      ),
      payment_terms_owner_ids: dict.insert(
        store.staged_state.payment_terms_owner_ids,
        record.owner_id,
        True,
      ),
      payment_terms_by_owner_id: dict.insert(
        store.staged_state.payment_terms_by_owner_id,
        record.owner_id,
        record.id,
      ),
      deleted_payment_terms_ids: dict.delete(
        store.staged_state.deleted_payment_terms_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_payment_terms(store: Store, id: String) -> Store {
  let owner_id = case get_effective_payment_terms_by_id(store, id) {
    Some(record) -> Some(record.owner_id)
    None -> None
  }
  let by_owner = case owner_id {
    Some(owner) ->
      dict.delete(store.staged_state.payment_terms_by_owner_id, owner)
    None -> store.staged_state.payment_terms_by_owner_id
  }
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_terms: dict.delete(store.staged_state.payment_terms, id),
      payment_terms_by_owner_id: by_owner,
      deleted_payment_terms_ids: dict.insert(
        store.staged_state.deleted_payment_terms_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn payment_terms_owner_exists(store: Store, owner_id: String) -> Bool {
  case dict.get(store.staged_state.payment_terms_owner_ids, owner_id) {
    Ok(True) -> True
    _ ->
      case dict.get(store.base_state.payment_terms_owner_ids, owner_id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

pub fn get_effective_payment_terms_by_id(
  store: Store,
  id: String,
) -> Option(PaymentTermsRecord) {
  case dict.get(store.staged_state.deleted_payment_terms_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.payment_terms, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.payment_terms, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_payment_terms_by_owner_id(
  store: Store,
  owner_id: String,
) -> Option(PaymentTermsRecord) {
  let id = case
    dict.get(store.staged_state.payment_terms_by_owner_id, owner_id)
  {
    Ok(value) -> Some(value)
    Error(_) ->
      case dict.get(store.base_state.payment_terms_by_owner_id, owner_id) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
  }
  case id {
    Some(payment_terms_id) ->
      get_effective_payment_terms_by_id(store, payment_terms_id)
    None -> None
  }
}

pub fn stage_store_credit_account(
  store: Store,
  record: StoreCreditAccountRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      store_credit_accounts: dict.insert(
        store.staged_state.store_credit_accounts,
        record.id,
        record,
      ),
    ),
  )
}

pub fn stage_store_credit_account_transaction(
  store: Store,
  record: StoreCreditAccountTransactionRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      store_credit_account_transactions: dict.insert(
        store.staged_state.store_credit_account_transactions,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_store_credit_account_by_id(
  store: Store,
  account_id: String,
) -> Option(StoreCreditAccountRecord) {
  let found = case
    dict.get(store.staged_state.store_credit_accounts, account_id)
  {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.store_credit_accounts, account_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  case found {
    Some(account) ->
      case get_effective_customer_by_id(store, account.customer_id) {
        Some(_) -> Some(account)
        None -> None
      }
    None -> None
  }
}

pub fn list_effective_store_credit_accounts_for_customer(
  store: Store,
  customer_id: String,
) -> List(StoreCreditAccountRecord) {
  dict.keys(dict.merge(
    store.base_state.store_credit_accounts,
    store.staged_state.store_credit_accounts,
  ))
  |> list.sort(string_compare)
  |> list.filter_map(fn(id) {
    case get_effective_store_credit_account_by_id(store, id) {
      Some(account) ->
        case account.customer_id == customer_id {
          True -> Ok(account)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn list_effective_store_credit_account_transactions(
  store: Store,
  account_id: String,
) -> List(StoreCreditAccountTransactionRecord) {
  dict.values(dict.merge(
    store.base_state.store_credit_account_transactions,
    store.staged_state.store_credit_account_transactions,
  ))
  |> list.filter(fn(txn) { txn.account_id == account_id })
  |> list.sort(fn(a, b) {
    case string.compare(b.created_at, a.created_at) {
      order.Eq -> string.compare(b.id, a.id)
      other -> other
    }
  })
}

pub fn upsert_base_customer_account_pages(
  store: Store,
  records: List(CustomerAccountPageRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_account_pages: dict.insert(
          base.customer_account_pages,
          record.id,
          record,
        ),
        customer_account_page_order: append_unique_id(
          base.customer_account_page_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_customer_account_page_by_id(
  store: Store,
  page_id: String,
) -> Option(CustomerAccountPageRecord) {
  case dict.get(store.base_state.customer_account_pages, page_id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_customer_account_pages(
  store: Store,
) -> List(CustomerAccountPageRecord) {
  let ids =
    list.append(
      store.base_state.customer_account_page_order,
      store.staged_state.customer_account_page_order,
    )
    |> dedupe_strings()
  list.filter_map(ids, fn(id) {
    case get_effective_customer_account_page_by_id(store, id) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
}

pub fn stage_customer_data_erasure_request(
  store: Store,
  request: CustomerDataErasureRequestRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_data_erasure_requests: dict.insert(
        store.staged_state.customer_data_erasure_requests,
        request.customer_id,
        request,
      ),
    ),
  )
}

pub fn get_customer_data_erasure_request(
  store: Store,
  customer_id: String,
) -> Option(CustomerDataErasureRequestRecord) {
  case
    dict.get(store.staged_state.customer_data_erasure_requests, customer_id)
  {
    Ok(request) -> Some(request)
    Error(_) ->
      case
        dict.get(store.base_state.customer_data_erasure_requests, customer_id)
      {
        Ok(request) -> Some(request)
        Error(_) -> None
      }
  }
}

pub fn stage_merge_customers(
  store: Store,
  source_customer_id: String,
  resulting_customer: CustomerRecord,
  merge_request: CustomerMergeRequestRecord,
) -> Store {
  let after_delete = stage_delete_customer(store, source_customer_id)
  let #(stored, after_update) =
    stage_update_customer(after_delete, resulting_customer)
  let _ = stored
  Store(
    ..after_update,
    staged_state: StagedState(
      ..after_update.staged_state,
      merged_customer_ids: dict.insert(
        after_update.staged_state.merged_customer_ids,
        source_customer_id,
        resulting_customer.id,
      ),
      customer_merge_requests: dict.insert(
        after_update.staged_state.customer_merge_requests,
        merge_request.job_id,
        merge_request,
      ),
    ),
  )
}

pub fn get_customer_merge_request(
  store: Store,
  job_id: String,
) -> Option(CustomerMergeRequestRecord) {
  case dict.get(store.staged_state.customer_merge_requests, job_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_merge_requests, job_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

// ---------------------------------------------------------------------------
// Segment slice (Pass 20)
// ---------------------------------------------------------------------------

pub fn upsert_base_segments(
  store: Store,
  records: List(SegmentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        segments: dict.insert(base.segments, record.id, record),
        segment_order: append_unique_id(base.segment_order, record.id),
        deleted_segment_ids: dict.delete(base.deleted_segment_ids, record.id),
      ),
    )
  })
}

/// Stage a segment record. Mirrors `upsertStagedSegment`. Returns the
/// stored record alongside the new store so the caller can build a
/// mutation payload.
pub fn upsert_staged_segment(
  store: Store,
  record: SegmentRecord,
) -> #(SegmentRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.segment_order, record.id)
    || list.contains(staged.segment_order, record.id)
  let new_order = case already_known {
    True -> staged.segment_order
    False -> list.append(staged.segment_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      segments: dict.insert(staged.segments, record.id, record),
      segment_order: new_order,
      deleted_segment_ids: dict.delete(staged.deleted_segment_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a segment id as deleted. Mirrors `deleteStagedSegment`.
pub fn delete_staged_segment(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      segments: dict.delete(staged.segments, id),
      deleted_segment_ids: dict.insert(staged.deleted_segment_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective segment for an id. Staged wins over base; any
/// "deleted" marker on either side suppresses the record. Mirrors
/// `getEffectiveSegmentById`.
pub fn get_effective_segment_by_id(
  store: Store,
  id: String,
) -> Option(SegmentRecord) {
  let deleted =
    dict_has(store.base_state.deleted_segment_ids, id)
    || dict_has(store.staged_state.deleted_segment_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.segments, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.segments, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective segment the store knows about. Ordered records
/// (those tracked by `segmentOrder`) come first, followed by any
/// unordered staged/base records sorted by id. Mirrors
/// `listEffectiveSegments`.
pub fn list_effective_segments(store: Store) -> List(SegmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.segment_order,
      store.staged_state.segment_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_segment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.segments, store.staged_state.segments)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_segment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn set_base_segment_root_payload(
  store: Store,
  root_name: String,
  payload: StorePropertyValue,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      segment_root_payloads: dict.insert(
        base.segment_root_payloads,
        root_name,
        payload,
      ),
    ),
  )
}

pub fn get_base_segment_root_payload(
  store: Store,
  root_name: String,
) -> Option(StorePropertyValue) {
  case dict.get(store.base_state.segment_root_payloads, root_name) {
    Ok(payload) -> Some(payload)
    Error(_) -> None
  }
}

// ---------------------------------------------------------------------------
// Customer-segment-members-query slice (Pass 22j)
// ---------------------------------------------------------------------------

/// Stage a customer-segment-members-query record. Mirrors
/// `stageCustomerSegmentMembersQuery`.
pub fn stage_customer_segment_members_query(
  store: Store,
  record: CustomerSegmentMembersQueryRecord,
) -> Store {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.customer_segment_members_query_order, record.id)
    || list.contains(staged.customer_segment_members_query_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_segment_members_query_order
    False ->
      list.append(staged.customer_segment_members_query_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customer_segment_members_queries: dict.insert(
        staged.customer_segment_members_queries,
        record.id,
        record,
      ),
      customer_segment_members_query_order: new_order,
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective customer-segment-members-query for an id.
/// Staged wins over base. Mirrors
/// `getEffectiveCustomerSegmentMembersQueryById`.
pub fn get_effective_customer_segment_members_query_by_id(
  store: Store,
  id: String,
) -> Option(CustomerSegmentMembersQueryRecord) {
  case dict.get(store.staged_state.customer_segment_members_queries, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_segment_members_queries, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

// ---------------------------------------------------------------------------
// Localization slice (Pass 23)
// ---------------------------------------------------------------------------

/// Replace the entire `availableLocales` catalog. Mirrors
/// `replaceBaseAvailableLocales`. The TS handler hydrates this from
/// upstream responses; the Gleam port only ever sees it via tests
/// today, but keeping the helper surface intact unblocks future
/// hydration work.
pub fn replace_base_available_locales(
  store: Store,
  locales: List(LocaleRecord),
) -> Store {
  let new_base = BaseState(..store.base_state, available_locales: locales)
  Store(..store, base_state: new_base)
}

/// Read the catalog of every locale Shopify recognises. Mirrors
/// `listEffectiveAvailableLocales`. Empty when no upstream response
/// has hydrated it; the localization handler falls back to its own
/// hardcoded default catalog in that case.
pub fn list_effective_available_locales(store: Store) -> List(LocaleRecord) {
  store.base_state.available_locales
}

/// Upsert one or more shop-locale records into the base state. Mirrors
/// `upsertBaseShopLocales`. Removes any existing "deleted" markers
/// (in either base or staged) for the same locale, since the upstream
/// answer wins.
pub fn upsert_base_shop_locales(
  store: Store,
  records: List(ShopLocaleRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        shop_locales: dict.insert(base.shop_locales, record.locale, record),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_shop_locales: dict.delete(
          staged.deleted_shop_locales,
          record.locale,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a shop-locale record. Mirrors `stageShopLocale`.
pub fn stage_shop_locale(
  store: Store,
  record: ShopLocaleRecord,
) -> #(ShopLocaleRecord, Store) {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      shop_locales: dict.insert(staged.shop_locales, record.locale, record),
      deleted_shop_locales: dict.delete(
        staged.deleted_shop_locales,
        record.locale,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a shop-locale as disabled. Mirrors `disableShopLocale`. Returns
/// the record that was previously effective (if any) so the caller can
/// build the mutation response payload.
pub fn disable_shop_locale(
  store: Store,
  locale: String,
) -> #(Option(ShopLocaleRecord), Store) {
  let staged = store.staged_state
  let base = store.base_state
  let existing = case dict.get(staged.shop_locales, locale) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(base.shop_locales, locale) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  let new_staged = case existing {
    Some(_) ->
      StagedState(
        ..staged,
        shop_locales: dict.delete(staged.shop_locales, locale),
        deleted_shop_locales: dict.insert(
          staged.deleted_shop_locales,
          locale,
          True,
        ),
      )
    None ->
      StagedState(
        ..staged,
        shop_locales: dict.delete(staged.shop_locales, locale),
      )
  }
  #(existing, Store(..store, staged_state: new_staged))
}

/// Look up the effective shop-locale for a locale code. Staged wins
/// over base; any "deleted" marker on the staged side suppresses the
/// record. Mirrors `getEffectiveShopLocale`.
pub fn get_effective_shop_locale(
  store: Store,
  locale: String,
) -> Option(ShopLocaleRecord) {
  case dict_has(store.staged_state.deleted_shop_locales, locale) {
    True -> None
    False ->
      case dict.get(store.staged_state.shop_locales, locale) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.shop_locales, locale) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective shop locale. Optionally filter by `published`.
/// Sort: primary locale first, then by locale code. Mirrors
/// `listEffectiveShopLocales`.
pub fn list_effective_shop_locales(
  store: Store,
  published: Option(Bool),
) -> List(ShopLocaleRecord) {
  let base_records =
    dict.values(store.base_state.shop_locales)
    |> list.filter(fn(record) {
      !dict_has(store.staged_state.deleted_shop_locales, record.locale)
    })
  let staged_records =
    dict.values(store.staged_state.shop_locales)
    |> list.filter(fn(record) {
      !dict_has(store.staged_state.deleted_shop_locales, record.locale)
    })
  let merged_dict =
    list.fold(base_records, dict.new(), fn(acc, record) {
      dict.insert(acc, record.locale, record)
    })
  let merged_dict =
    list.fold(staged_records, merged_dict, fn(acc, record) {
      dict.insert(acc, record.locale, record)
    })
  let merged = dict.values(merged_dict)
  let filtered = case published {
    Some(target) -> list.filter(merged, fn(r) { r.published == target })
    None -> merged
  }
  list.sort(filtered, fn(left, right) {
    case left.primary, right.primary {
      True, False -> order.Lt
      False, True -> order.Gt
      _, _ -> string.compare(left.locale, right.locale)
    }
  })
}

/// Build the storage key used to address a translation:
/// `<resource_id>::<locale>::<market_id?>::<key>`. Mirrors
/// `translationStorageKey`.
pub fn translation_storage_key(
  resource_id: String,
  locale: String,
  key: String,
  market_id: Option(String),
) -> String {
  let market_part = option.unwrap(market_id, "")
  resource_id <> "::" <> locale <> "::" <> market_part <> "::" <> key
}

/// Stage a translation record. Mirrors `stageTranslation`.
pub fn stage_translation(
  store: Store,
  record: TranslationRecord,
) -> #(TranslationRecord, Store) {
  let storage_key =
    translation_storage_key(
      record.resource_id,
      record.locale,
      record.key,
      record.market_id,
    )
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      translations: dict.insert(staged.translations, storage_key, record),
      deleted_translations: dict.delete(
        staged.deleted_translations,
        storage_key,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Remove a translation. Returns the record that was effective before
/// removal (if any). Mirrors `removeTranslation`.
pub fn remove_translation(
  store: Store,
  resource_id: String,
  locale: String,
  key: String,
  market_id: Option(String),
) -> #(Option(TranslationRecord), Store) {
  let storage_key = translation_storage_key(resource_id, locale, key, market_id)
  let staged = store.staged_state
  let base = store.base_state
  let existing = case dict.get(staged.translations, storage_key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(base.translations, storage_key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  let new_staged = case existing {
    Some(_) ->
      StagedState(
        ..staged,
        translations: dict.delete(staged.translations, storage_key),
        deleted_translations: dict.insert(
          staged.deleted_translations,
          storage_key,
          True,
        ),
      )
    None ->
      StagedState(
        ..staged,
        translations: dict.delete(staged.translations, storage_key),
      )
  }
  #(existing, Store(..store, staged_state: new_staged))
}

/// Remove every translation registered against a given locale. Returns
/// the records that were effective before removal, sorted by
/// (resource_id, key, updated_at). Mirrors `removeTranslationsForLocale`.
pub fn remove_translations_for_locale(
  store: Store,
  locale: String,
) -> #(List(TranslationRecord), Store) {
  let base_matching =
    dict.values(store.base_state.translations)
    |> list.filter(fn(t) { t.locale == locale })
  let staged_matching =
    dict.values(store.staged_state.translations)
    |> list.filter(fn(t) { t.locale == locale })
  let merged_dict =
    list.fold(base_matching, dict.new(), fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let merged_dict =
    list.fold(staged_matching, merged_dict, fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let staged = store.staged_state
  let staged_after_removal =
    list.fold(dict.keys(merged_dict), staged, fn(acc, storage_key) {
      StagedState(
        ..acc,
        translations: dict.delete(acc.translations, storage_key),
        deleted_translations: dict.insert(
          acc.deleted_translations,
          storage_key,
          True,
        ),
      )
    })
  let removed =
    dict.values(merged_dict)
    |> list.sort(fn(left, right) {
      case string.compare(left.resource_id, right.resource_id) {
        order.Eq ->
          case string.compare(left.key, right.key) {
            order.Eq -> string.compare(left.updated_at, right.updated_at)
            other -> other
          }
        other -> other
      }
    })
  #(removed, Store(..store, staged_state: staged_after_removal))
}

/// List the effective translations for a `(resource_id, locale, market_id)`
/// triple. Mirrors `listEffectiveTranslations`. Sort: by `key`, then
/// `updated_at`.
pub fn list_effective_translations(
  store: Store,
  resource_id: String,
  locale: String,
  market_id: Option(String),
) -> List(TranslationRecord) {
  let base_matching =
    dict.values(store.base_state.translations)
    |> list.filter(fn(t) {
      t.resource_id == resource_id
      && t.locale == locale
      && t.market_id == market_id
      && {
        let storage_key =
          translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
        !dict_has(store.staged_state.deleted_translations, storage_key)
      }
    })
  let staged_matching =
    dict.values(store.staged_state.translations)
    |> list.filter(fn(t) {
      t.resource_id == resource_id
      && t.locale == locale
      && t.market_id == market_id
    })
  let merged_dict =
    list.fold(base_matching, dict.new(), fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let merged_dict =
    list.fold(staged_matching, merged_dict, fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  dict.values(merged_dict)
  |> list.sort(fn(left, right) {
    case string.compare(left.key, right.key) {
      order.Eq -> string.compare(left.updated_at, right.updated_at)
      other -> other
    }
  })
}

/// True if the store contains any localization state. Mirrors
/// `hasLocalizationState`. Used by the meta-state serializer (not yet
/// ported on the Gleam side); kept here for parity.
pub fn has_localization_state(store: Store) -> Bool {
  let base = store.base_state
  let staged = store.staged_state
  !list.is_empty(base.available_locales)
  || !list.is_empty(dict.keys(base.shop_locales))
  || !list.is_empty(dict.keys(staged.shop_locales))
  || !list.is_empty(dict.keys(staged.deleted_shop_locales))
  || !list.is_empty(dict.keys(base.translations))
  || !list.is_empty(dict.keys(staged.translations))
  || !list.is_empty(dict.keys(staged.deleted_translations))
}

// ---------------------------------------------------------------------------
// Mutation log
// ---------------------------------------------------------------------------

/// Append a mutation log entry. Mirrors `recordMutationLogEntry`.
pub fn record_mutation_log_entry(
  store: Store,
  entry: MutationLogEntry,
) -> Store {
  Store(..store, mutation_log: list.append(store.mutation_log, [entry]))
}

/// Read the mutation log in insertion order. Mirrors `getLog`.
pub fn get_log(store: Store) -> List(MutationLogEntry) {
  store.mutation_log
}

/// Update the status and notes of a single log entry, looked up by id.
/// Mirrors `InMemoryStore.updateLogEntry` — used by the commit path to
/// flip entries from `Staged` to `Committed` or `Failed` and stamp the
/// reason. A no-op when no entry matches the id.
pub fn update_log_entry(
  store: Store,
  id: String,
  status: EntryStatus,
  notes: Option(String),
) -> Store {
  let updated =
    list.map(store.mutation_log, fn(entry) {
      case entry.id == id {
        True -> MutationLogEntry(..entry, status: status, notes: notes)
        False -> entry
      }
    })
  Store(..store, mutation_log: updated)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn append_unique_id(order: List(String), id: String) -> List(String) {
  case list.contains(order, id) {
    True -> order
    False -> list.append(order, [id])
  }
}

fn product_collection_storage_key(record: ProductCollectionRecord) -> String {
  record.product_id <> "::" <> record.collection_id
}

fn compare_product_collection_records(
  left: ProductCollectionRecord,
  right: ProductCollectionRecord,
) -> order.Order {
  case int.compare(left.position, right.position) {
    order.Eq -> string.compare(left.product_id, right.product_id)
    other -> other
  }
}

fn channel_from_publication(
  publication: PublicationRecord,
) -> Option(ChannelRecord) {
  case publication.channel_id {
    Some(id) ->
      Some(ChannelRecord(
        id: id,
        name: publication.name,
        handle: None,
        publication_id: Some(publication.id),
        cursor: None,
      ))
    None -> {
      let tail = resource_tail(publication.id)
      case tail == "" {
        True -> None
        False ->
          Some(ChannelRecord(
            id: "gid://shopify/Channel/" <> tail,
            name: publication.name,
            handle: None,
            publication_id: Some(publication.id),
            cursor: None,
          ))
      }
    }
  }
}

fn resource_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, "?") {
        [tail, ..] -> tail
        [] -> tail_with_query
      }
    Error(_) -> ""
  }
}

fn dict_has(d: Dict(String, a), key: String) -> Bool {
  case dict.get(d, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

fn dedupe_strings(items: List(String)) -> List(String) {
  do_dedupe(items, dict.new(), [])
}

fn do_dedupe(
  remaining: List(String),
  seen: Dict(String, Bool),
  acc: List(String),
) -> List(String) {
  case remaining {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      case dict.get(seen, first) {
        Ok(_) -> do_dedupe(rest, seen, acc)
        Error(_) ->
          do_dedupe(rest, dict.insert(seen, first, True), [first, ..acc])
      }
  }
}

fn list_to_set(items: List(String)) -> Dict(String, Bool) {
  list.fold(items, dict.new(), fn(acc, item) { dict.insert(acc, item, True) })
}

fn string_compare(a: String, b: String) -> order.Order {
  string.compare(a, b)
}

fn bool_compare(a: Bool, b: Bool) -> order.Order {
  case a, b {
    True, False -> order.Gt
    False, True -> order.Lt
    _, _ -> order.Eq
  }
}

fn find_app_in_dict(
  d: Dict(String, AppRecord),
  predicate: fn(AppRecord) -> Bool,
) -> Option(AppRecord) {
  list.find(dict.values(d), predicate)
  |> option.from_result
}

fn find_token_in_dict(
  d: Dict(String, DelegatedAccessTokenRecord),
  predicate: fn(DelegatedAccessTokenRecord) -> Bool,
) -> Option(DelegatedAccessTokenRecord) {
  list.find(dict.values(d), predicate)
  |> option.from_result
}

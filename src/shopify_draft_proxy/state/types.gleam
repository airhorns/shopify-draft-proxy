//// Mirrors the slices of `src/state/types.ts` that the Gleam port
//// currently exercises. Only resource types this port knows about are
//// included; everything else is deliberately deferred until the
//// corresponding domain handler lands.
////
//// Putting the resource records here (rather than in either the
//// `state/store` or `proxy/saved_searches` module) avoids a circular
//// import: the store needs to know the shapes of the records it stores,
//// and the domain handler needs to read them back; both depend on this
//// module.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}

// Products domain
// ---------------------------------------------------------------------------

pub type ProductSeoRecord {
  ProductSeoRecord(title: Option(String), description: Option(String))
}

pub type ProductCategoryRecord {
  ProductCategoryRecord(id: String, full_name: String)
}

pub type CollectionImageRecord {
  CollectionImageRecord(
    id: Option(String),
    alt_text: Option(String),
    url: Option(String),
    width: Option(Int),
    height: Option(Int),
  )
}

pub type CollectionRuleRecord {
  CollectionRuleRecord(column: String, relation: String, condition: String)
}

pub type CollectionRuleSetRecord {
  CollectionRuleSetRecord(
    applied_disjunctively: Bool,
    rules: List(CollectionRuleRecord),
  )
}

pub type CollectionRecord {
  CollectionRecord(
    id: String,
    legacy_resource_id: Option(String),
    title: String,
    handle: String,
    publication_ids: List(String),
    updated_at: Option(String),
    description: Option(String),
    description_html: Option(String),
    image: Option(CollectionImageRecord),
    sort_order: Option(String),
    template_suffix: Option(String),
    seo: ProductSeoRecord,
    rule_set: Option(CollectionRuleSetRecord),
    products_count: Option(Int),
    is_smart: Bool,
    cursor: Option(String),
    title_cursor: Option(String),
    updated_at_cursor: Option(String),
  )
}

pub type ProductCollectionRecord {
  ProductCollectionRecord(
    collection_id: String,
    product_id: String,
    position: Int,
    cursor: Option(String),
  )
}

pub type ProductMediaRecord {
  ProductMediaRecord(
    key: String,
    product_id: String,
    position: Int,
    id: Option(String),
    media_content_type: Option(String),
    alt: Option(String),
    status: Option(String),
    product_image_id: Option(String),
    image_url: Option(String),
    image_width: Option(Int),
    image_height: Option(Int),
    preview_image_url: Option(String),
    source_url: Option(String),
  )
}

pub type FileRecord {
  FileRecord(
    id: String,
    alt: Option(String),
    content_type: Option(String),
    created_at: String,
    file_status: String,
    filename: Option(String),
    original_source: String,
    image_url: Option(String),
    image_width: Option(Int),
    image_height: Option(Int),
    update_failure_acknowledged_at: Option(String),
  )
}

pub type ProductVariantSelectedOptionRecord {
  ProductVariantSelectedOptionRecord(name: String, value: String)
}

pub type CapturedJsonValue {
  CapturedNull
  CapturedBool(Bool)
  CapturedInt(Int)
  CapturedFloat(Float)
  CapturedString(String)
  CapturedArray(List(CapturedJsonValue))
  CapturedObject(List(#(String, CapturedJsonValue)))
}

pub type OnlineStoreContentRecord {
  OnlineStoreContentRecord(
    id: String,
    kind: String,
    cursor: Option(String),
    parent_id: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    data: CapturedJsonValue,
  )
}

pub type OnlineStoreIntegrationRecord {
  OnlineStoreIntegrationRecord(
    id: String,
    kind: String,
    cursor: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    data: CapturedJsonValue,
  )
}

// Discounts domain
// ---------------------------------------------------------------------------

pub type DiscountRecord {
  DiscountRecord(
    id: String,
    owner_kind: String,
    discount_type: String,
    title: Option(String),
    status: String,
    code: Option(String),
    payload: CapturedJsonValue,
    cursor: Option(String),
  )
}

pub type DiscountBulkOperationRecord {
  DiscountBulkOperationRecord(
    id: String,
    operation: String,
    discount_id: String,
    status: String,
    payload: CapturedJsonValue,
  )
}

// Orders domain
// ---------------------------------------------------------------------------

pub type AbandonedCheckoutRecord {
  AbandonedCheckoutRecord(
    id: String,
    cursor: Option(String),
    data: CapturedJsonValue,
  )
}

pub type AbandonmentDeliveryActivityRecord {
  AbandonmentDeliveryActivityRecord(
    marketing_activity_id: String,
    delivery_status: String,
    delivered_at: Option(String),
    delivery_status_change_reason: Option(String),
  )
}

pub type AbandonmentRecord {
  AbandonmentRecord(
    id: String,
    abandoned_checkout_id: Option(String),
    cursor: Option(String),
    data: CapturedJsonValue,
    delivery_activities: Dict(String, AbandonmentDeliveryActivityRecord),
  )
}

pub type DraftOrderRecord {
  DraftOrderRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

pub type OrderRecord {
  OrderRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

pub type DraftOrderVariantCatalogRecord {
  DraftOrderVariantCatalogRecord(
    variant_id: String,
    title: String,
    name: String,
    variant_title: Option(String),
    sku: Option(String),
    requires_shipping: Bool,
    taxable: Bool,
    unit_price: String,
    currency_code: String,
  )
}

pub type InventoryWeightValue {
  InventoryWeightInt(Int)
  InventoryWeightFloat(Float)
}

pub type InventoryWeightRecord {
  InventoryWeightRecord(unit: String, value: InventoryWeightValue)
}

pub type InventoryMeasurementRecord {
  InventoryMeasurementRecord(weight: Option(InventoryWeightRecord))
}

pub type InventoryLocationRecord {
  InventoryLocationRecord(id: String, name: String)
}

pub type LocationRecord {
  LocationRecord(
    id: String,
    name: String,
    is_active: Option(Bool),
    cursor: Option(String),
  )
}

pub type PublicationRecord {
  PublicationRecord(
    id: String,
    name: Option(String),
    auto_publish: Option(Bool),
    supports_future_publishing: Option(Bool),
    catalog_id: Option(String),
    channel_id: Option(String),
    cursor: Option(String),
  )
}

pub type ChannelRecord {
  ChannelRecord(
    id: String,
    name: Option(String),
    handle: Option(String),
    publication_id: Option(String),
    cursor: Option(String),
  )
}

pub type ProductFeedRecord {
  ProductFeedRecord(
    id: String,
    country: Option(String),
    language: Option(String),
    status: String,
  )
}

pub type ProductResourceFeedbackRecord {
  ProductResourceFeedbackRecord(
    product_id: String,
    state: String,
    feedback_generated_at: String,
    product_updated_at: String,
    messages: List(String),
  )
}

pub type ShopResourceFeedbackRecord {
  ShopResourceFeedbackRecord(
    id: String,
    state: String,
    feedback_generated_at: String,
    messages: List(String),
  )
}

pub type InventoryQuantityRecord {
  InventoryQuantityRecord(
    name: String,
    quantity: Int,
    updated_at: Option(String),
  )
}

pub type InventoryLevelRecord {
  InventoryLevelRecord(
    id: String,
    location: InventoryLocationRecord,
    quantities: List(InventoryQuantityRecord),
    is_active: Option(Bool),
    cursor: Option(String),
  )
}

pub type InventoryItemRecord {
  InventoryItemRecord(
    id: String,
    tracked: Option(Bool),
    requires_shipping: Option(Bool),
    measurement: Option(InventoryMeasurementRecord),
    country_code_of_origin: Option(String),
    province_code_of_origin: Option(String),
    harmonized_system_code: Option(String),
    inventory_levels: List(InventoryLevelRecord),
  )
}

pub type InventoryTransferLocationSnapshotRecord {
  InventoryTransferLocationSnapshotRecord(
    id: Option(String),
    name: String,
    snapshotted_at: String,
  )
}

pub type InventoryTransferLineItemRecord {
  InventoryTransferLineItemRecord(
    id: String,
    inventory_item_id: String,
    title: Option(String),
    total_quantity: Int,
    shipped_quantity: Int,
    picked_for_shipment_quantity: Int,
  )
}

pub type InventoryTransferRecord {
  InventoryTransferRecord(
    id: String,
    name: String,
    reference_name: Option(String),
    status: String,
    note: Option(String),
    tags: List(String),
    date_created: String,
    origin: Option(InventoryTransferLocationSnapshotRecord),
    destination: Option(InventoryTransferLocationSnapshotRecord),
    line_items: List(InventoryTransferLineItemRecord),
  )
}

pub type InventoryShipmentTrackingRecord {
  InventoryShipmentTrackingRecord(
    tracking_number: Option(String),
    company: Option(String),
    tracking_url: Option(String),
    arrives_at: Option(String),
  )
}

pub type InventoryShipmentLineItemRecord {
  InventoryShipmentLineItemRecord(
    id: String,
    inventory_item_id: String,
    quantity: Int,
    accepted_quantity: Int,
    rejected_quantity: Int,
  )
}

pub type InventoryShipmentRecord {
  InventoryShipmentRecord(
    id: String,
    movement_id: String,
    name: String,
    status: String,
    created_at: String,
    updated_at: String,
    tracking: Option(InventoryShipmentTrackingRecord),
    line_items: List(InventoryShipmentLineItemRecord),
  )
}

pub type ShippingPackageWeightRecord {
  ShippingPackageWeightRecord(value: Option(Float), unit: Option(String))
}

pub type ShippingPackageDimensionsRecord {
  ShippingPackageDimensionsRecord(
    length: Option(Float),
    width: Option(Float),
    height: Option(Float),
    unit: Option(String),
  )
}

pub type ShippingPackageRecord {
  ShippingPackageRecord(
    id: String,
    name: Option(String),
    type_: Option(String),
    box_type: Option(String),
    default: Bool,
    weight: Option(ShippingPackageWeightRecord),
    dimensions: Option(ShippingPackageDimensionsRecord),
    created_at: String,
    updated_at: String,
  )
}

pub type CarrierServiceRecord {
  CarrierServiceRecord(
    id: String,
    name: Option(String),
    formatted_name: Option(String),
    callback_url: Option(String),
    active: Bool,
    supports_service_discovery: Bool,
    created_at: String,
    updated_at: String,
  )
}

pub type FulfillmentServiceRecord {
  FulfillmentServiceRecord(
    id: String,
    handle: String,
    service_name: String,
    callback_url: Option(String),
    inventory_management: Bool,
    location_id: Option(String),
    requires_shipping_method: Bool,
    tracking_support: Bool,
    type_: String,
  )
}

pub type FulfillmentRecord {
  FulfillmentRecord(
    id: String,
    order_id: Option(String),
    data: CapturedJsonValue,
  )
}

pub type FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    id: String,
    order_id: Option(String),
    status: String,
    request_status: String,
    assigned_location_id: Option(String),
    assignment_status: Option(String),
    manually_held: Bool,
    data: CapturedJsonValue,
  )
}

pub type ShippingOrderRecord {
  ShippingOrderRecord(id: String, data: CapturedJsonValue)
}

pub type ReverseFulfillmentOrderRecord {
  ReverseFulfillmentOrderRecord(id: String, data: CapturedJsonValue)
}

pub type ReverseDeliveryRecord {
  ReverseDeliveryRecord(
    id: String,
    reverse_fulfillment_order_id: String,
    data: CapturedJsonValue,
  )
}

pub type CalculatedOrderRecord {
  CalculatedOrderRecord(id: String, data: CapturedJsonValue)
}

pub type DeliveryProfileRecord {
  DeliveryProfileRecord(
    id: String,
    cursor: Option(String),
    merchant_owned: Bool,
    data: CapturedJsonValue,
  )
}

pub type ProductVariantRecord {
  ProductVariantRecord(
    id: String,
    product_id: String,
    title: String,
    sku: Option(String),
    barcode: Option(String),
    price: Option(String),
    compare_at_price: Option(String),
    taxable: Option(Bool),
    inventory_policy: Option(String),
    inventory_quantity: Option(Int),
    selected_options: List(ProductVariantSelectedOptionRecord),
    media_ids: List(String),
    inventory_item: Option(InventoryItemRecord),
    contextual_pricing: Option(CapturedJsonValue),
    cursor: Option(String),
  )
}

pub type ProductOptionValueRecord {
  ProductOptionValueRecord(
    id: String,
    name: String,
    has_variants: Bool,
    linked_metafield_value: Option(String),
  )
}

pub type ProductOptionLinkedMetafieldRecord {
  ProductOptionLinkedMetafieldRecord(
    namespace: String,
    key: String,
    metafield_definition_id: Option(String),
  )
}

pub type ProductOptionRecord {
  ProductOptionRecord(
    id: String,
    product_id: String,
    name: String,
    position: Int,
    linked_metafield: Option(ProductOptionLinkedMetafieldRecord),
    option_values: List(ProductOptionValueRecord),
  )
}

pub type ProductOperationUserErrorRecord {
  ProductOperationUserErrorRecord(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

pub type ProductOperationRecord {
  ProductOperationRecord(
    id: String,
    type_name: String,
    product_id: Option(String),
    new_product_id: Option(String),
    status: String,
    user_errors: List(ProductOperationUserErrorRecord),
  )
}

pub type ProductRecord {
  ProductRecord(
    id: String,
    legacy_resource_id: Option(String),
    title: String,
    handle: String,
    status: String,
    vendor: Option(String),
    product_type: Option(String),
    tags: List(String),
    price_range_min: Option(String),
    price_range_max: Option(String),
    total_variants: Option(Int),
    has_only_default_variant: Option(Bool),
    has_out_of_stock_variants: Option(Bool),
    total_inventory: Option(Int),
    tracks_inventory: Option(Bool),
    created_at: Option(String),
    updated_at: Option(String),
    published_at: Option(String),
    description_html: String,
    online_store_preview_url: Option(String),
    template_suffix: Option(String),
    seo: ProductSeoRecord,
    category: Option(ProductCategoryRecord),
    publication_ids: List(String),
    contextual_pricing: Option(CapturedJsonValue),
    cursor: Option(String),
    combined_listing_role: Option(String),
    combined_listing_parent_id: Option(String),
    combined_listing_child_ids: List(String),
  )
}

pub type SellingPlanRecord {
  SellingPlanRecord(id: String, data: CapturedJsonValue)
}

pub type SellingPlanGroupRecord {
  SellingPlanGroupRecord(
    id: String,
    app_id: Option(String),
    name: String,
    merchant_code: String,
    description: Option(String),
    options: List(String),
    position: Option(Int),
    summary: Option(String),
    created_at: Option(String),
    product_ids: List(String),
    product_variant_ids: List(String),
    selling_plans: List(SellingPlanRecord),
    cursor: Option(String),
  )
}

// ---------------------------------------------------------------------------
// Markets domain
// ---------------------------------------------------------------------------

/// Mirrors `MarketRecord` in `src/state/types.ts`. The captured Shopify
/// payload remains JSON-shaped because Markets expose several nested union and
/// connection subtrees that are projected by the domain serializer.
pub type MarketRecord {
  MarketRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

/// Mirrors `CatalogRecord` in `src/state/types.ts`.
pub type CatalogRecord {
  CatalogRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

/// Mirrors `PriceListRecord` in `src/state/types.ts`.
pub type PriceListRecord {
  PriceListRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

/// Mirrors `WebPresenceRecord` in `src/state/types.ts`.
pub type WebPresenceRecord {
  WebPresenceRecord(id: String, cursor: Option(String), data: CapturedJsonValue)
}

/// Mirrors `MarketLocalizationRecord` in `src/state/types.ts`.
pub type MarketLocalizationRecord {
  MarketLocalizationRecord(
    resource_id: String,
    market_id: String,
    key: String,
    value: String,
    updated_at: String,
    outdated: Bool,
  )
}

pub type MarketLocalizableContentRecord {
  MarketLocalizableContentRecord(key: String, value: String, digest: String)
}

// ---------------------------------------------------------------------------
// Metafields domain
// ---------------------------------------------------------------------------

pub type ProductMetafieldRecord {
  ProductMetafieldRecord(
    id: String,
    owner_id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    json_value: Option(Json),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
    market_localizable_content: List(MarketLocalizableContentRecord),
  )
}

pub type MetafieldDefinitionCapabilityRecord {
  MetafieldDefinitionCapabilityRecord(
    enabled: Bool,
    eligible: Bool,
    status: Option(String),
  )
}

pub type MetafieldDefinitionCapabilitiesRecord {
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: MetafieldDefinitionCapabilityRecord,
    smart_collection_condition: MetafieldDefinitionCapabilityRecord,
    unique_values: MetafieldDefinitionCapabilityRecord,
  )
}

pub type MetafieldDefinitionConstraintValueRecord {
  MetafieldDefinitionConstraintValueRecord(value: String)
}

pub type MetafieldDefinitionConstraintsRecord {
  MetafieldDefinitionConstraintsRecord(
    key: Option(String),
    values: List(MetafieldDefinitionConstraintValueRecord),
  )
}

pub type MetafieldDefinitionTypeRecord {
  MetafieldDefinitionTypeRecord(name: String, category: Option(String))
}

pub type MetafieldDefinitionValidationRecord {
  MetafieldDefinitionValidationRecord(name: String, value: Option(String))
}

pub type MetafieldDefinitionRecord {
  MetafieldDefinitionRecord(
    id: String,
    name: String,
    namespace: String,
    key: String,
    owner_type: String,
    type_: MetafieldDefinitionTypeRecord,
    description: Option(String),
    validations: List(MetafieldDefinitionValidationRecord),
    access: Dict(String, Json),
    capabilities: MetafieldDefinitionCapabilitiesRecord,
    constraints: Option(MetafieldDefinitionConstraintsRecord),
    pinned_position: Option(Int),
    validation_status: String,
  )
}

// ---------------------------------------------------------------------------
// Admin Platform utility domain
// ---------------------------------------------------------------------------

/// Mirrors the captured `MarketRegionCountry` shape used by
/// `backupRegion` and `backupRegionUpdate`.
pub type BackupRegionRecord {
  BackupRegionRecord(id: String, name: String, code: String)
}

pub type AdminPlatformGenericNodeRecord {
  AdminPlatformGenericNodeRecord(
    id: String,
    typename: String,
    data: CapturedJsonValue,
  )
}

pub type AdminPlatformTaxonomyCategoryRecord {
  AdminPlatformTaxonomyCategoryRecord(
    id: String,
    cursor: Option(String),
    data: CapturedJsonValue,
  )
}

/// Audit record for locally handled `flowGenerateSignature`.
/// The proxy stores hashes rather than the raw signature or payload.
pub type AdminPlatformFlowSignatureRecord {
  AdminPlatformFlowSignatureRecord(
    id: String,
    flow_trigger_id: String,
    payload_sha256: String,
    signature_sha256: String,
    created_at: String,
  )
}

/// Audit record for locally handled `flowTriggerReceive`.
/// The external Flow trigger delivery side effect is intentionally not
/// attempted by the draft proxy.
pub type AdminPlatformFlowTriggerRecord {
  AdminPlatformFlowTriggerRecord(
    id: String,
    handle: String,
    payload_bytes: Int,
    payload_sha256: String,
    received_at: String,
  )
}

// ---------------------------------------------------------------------------
// Store properties domain
// ---------------------------------------------------------------------------

pub type ShopDomainRecord {
  ShopDomainRecord(id: String, host: String, url: String, ssl_enabled: Bool)
}

pub type ShopAddressRecord {
  ShopAddressRecord(
    id: String,
    address1: Option(String),
    address2: Option(String),
    city: Option(String),
    company: Option(String),
    coordinates_validated: Bool,
    country: Option(String),
    country_code_v2: Option(String),
    formatted: List(String),
    formatted_area: Option(String),
    latitude: Option(Float),
    longitude: Option(Float),
    phone: Option(String),
    province: Option(String),
    province_code: Option(String),
    zip: Option(String),
  )
}

pub type ShopPlanRecord {
  ShopPlanRecord(
    partner_development: Bool,
    public_display_name: String,
    shopify_plus: Bool,
  )
}

pub type ShopResourceLimitsRecord {
  ShopResourceLimitsRecord(
    location_limit: Int,
    max_product_options: Int,
    max_product_variants: Int,
    redirect_limit_reached: Bool,
  )
}

pub type ShopBundlesFeatureRecord {
  ShopBundlesFeatureRecord(
    eligible_for_bundles: Bool,
    ineligibility_reason: Option(String),
    sells_bundles: Bool,
  )
}

pub type ShopCartTransformEligibleOperationsRecord {
  ShopCartTransformEligibleOperationsRecord(
    expand_operation: Bool,
    merge_operation: Bool,
    update_operation: Bool,
  )
}

pub type ShopCartTransformFeatureRecord {
  ShopCartTransformFeatureRecord(
    eligible_operations: ShopCartTransformEligibleOperationsRecord,
  )
}

pub type ShopFeaturesRecord {
  ShopFeaturesRecord(
    avalara_avatax: Bool,
    branding: String,
    bundles: ShopBundlesFeatureRecord,
    captcha: Bool,
    cart_transform: ShopCartTransformFeatureRecord,
    dynamic_remarketing: Bool,
    eligible_for_subscription_migration: Bool,
    eligible_for_subscriptions: Bool,
    gift_cards: Bool,
    harmonized_system_code: Bool,
    legacy_subscription_gateway_enabled: Bool,
    live_view: Bool,
    paypal_express_subscription_gateway_status: String,
    reports: Bool,
    discounts_by_market_enabled: Bool,
    markets_granted: Int,
    sells_subscriptions: Bool,
    show_metrics: Bool,
    storefront: Bool,
    unified_markets: Bool,
  )
}

pub type PaymentGatewayRecord {
  PaymentGatewayRecord(id: String, name: String, active: Bool)
}

pub type PaymentSettingsRecord {
  PaymentSettingsRecord(
    supported_digital_wallets: List(String),
    payment_gateways: List(PaymentGatewayRecord),
  )
}

pub type ShopPolicyRecord {
  ShopPolicyRecord(
    id: String,
    title: String,
    body: String,
    type_: String,
    url: String,
    created_at: String,
    updated_at: String,
    migrated_to_html: Bool,
  )
}

pub type ShopRecord {
  ShopRecord(
    id: String,
    name: String,
    myshopify_domain: String,
    url: String,
    primary_domain: ShopDomainRecord,
    contact_email: String,
    email: String,
    currency_code: String,
    enabled_presentment_currencies: List(String),
    iana_timezone: String,
    timezone_abbreviation: String,
    timezone_offset: String,
    timezone_offset_minutes: Int,
    taxes_included: Bool,
    tax_shipping: Bool,
    unit_system: String,
    weight_unit: String,
    shop_address: ShopAddressRecord,
    plan: ShopPlanRecord,
    resource_limits: ShopResourceLimitsRecord,
    features: ShopFeaturesRecord,
    payment_settings: PaymentSettingsRecord,
    shop_policies: List(ShopPolicyRecord),
  )
}

/// JSON-shaped Store Properties resource value. This is used for captured
/// Location, BusinessEntity, Product, and Collection projection slices whose
/// full owning domains have not all been ported yet.
pub type StorePropertyValue {
  StorePropertyNull
  StorePropertyString(String)
  StorePropertyBool(Bool)
  StorePropertyInt(Int)
  StorePropertyFloat(Float)
  StorePropertyList(List(StorePropertyValue))
  StorePropertyObject(Dict(String, StorePropertyValue))
}

/// Captured Store Properties resource row, keyed by Shopify GID where the
/// captured payload has one. `cursor` preserves connection order evidence.
pub type StorePropertyRecord {
  StorePropertyRecord(
    id: String,
    cursor: Option(String),
    data: Dict(String, StorePropertyValue),
  )
}

// ---------------------------------------------------------------------------
// B2B company domain
// ---------------------------------------------------------------------------

/// JSON-shaped B2B company row. Relationships are normalized as Shopify GIDs
/// so contacts, locations, and roles can be updated independently.
pub type B2BCompanyRecord {
  B2BCompanyRecord(
    id: String,
    cursor: Option(String),
    data: Dict(String, StorePropertyValue),
    main_contact_id: Option(String),
    contact_ids: List(String),
    location_ids: List(String),
    contact_role_ids: List(String),
  )
}

pub type B2BCompanyContactRecord {
  B2BCompanyContactRecord(
    id: String,
    cursor: Option(String),
    company_id: String,
    data: Dict(String, StorePropertyValue),
  )
}

pub type B2BCompanyContactRoleRecord {
  B2BCompanyContactRoleRecord(
    id: String,
    cursor: Option(String),
    company_id: String,
    data: Dict(String, StorePropertyValue),
  )
}

pub type B2BCompanyLocationRecord {
  B2BCompanyLocationRecord(
    id: String,
    cursor: Option(String),
    company_id: String,
    data: Dict(String, StorePropertyValue),
  )
}

/// Locally staged publishable mutation payload for the minimal Product /
/// Collection publication projection used by Store Properties parity.
pub type StorePropertyMutationPayloadRecord {
  StorePropertyMutationPayloadRecord(
    key: String,
    data: Dict(String, StorePropertyValue),
  )
}

/// A single saved-search record. Mirrors `SavedSearchRecord` in
/// `src/state/types.ts`. `cursor` is set on records the proxy stages
/// from upstream-hybrid responses; static defaults and freshly-created
/// records carry `None`.
pub type SavedSearchRecord {
  SavedSearchRecord(
    id: String,
    legacy_resource_id: String,
    name: String,
    query: String,
    resource_type: String,
    search_terms: String,
    filters: List(SavedSearchFilter),
    cursor: Option(String),
  )
}

/// One key/value filter on a saved search. Mirrors
/// `SavedSearchRecord['filters'][number]`.
pub type SavedSearchFilter {
  SavedSearchFilter(key: String, value: String)
}

/// Mirrors `WebhookSubscriptionEndpointRecord` in
/// `src/state/types.ts`. The TS schema is one record with all three
/// endpoint variants and a `__typename` discriminator; in Gleam it's
/// a sum type with one variant per endpoint kind. Each variant only
/// carries the fields its `__typename` uses, so impossible
/// combinations (e.g. an HTTP endpoint with an `arn`) are
/// unrepresentable.
pub type WebhookSubscriptionEndpoint {
  WebhookHttpEndpoint(callback_url: Option(String))
  WebhookEventBridgeEndpoint(arn: Option(String))
  WebhookPubSubEndpoint(
    pub_sub_project: Option(String),
    pub_sub_topic: Option(String),
  )
}

/// Mirrors `WebhookSubscriptionRecord`. `endpoint` is `None` to model
/// the TS `endpoint: ... | null`.
pub type WebhookSubscriptionRecord {
  WebhookSubscriptionRecord(
    id: String,
    topic: Option(String),
    uri: Option(String),
    name: Option(String),
    format: Option(String),
    include_fields: List(String),
    metafield_namespaces: List(String),
    filter: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    endpoint: Option(WebhookSubscriptionEndpoint),
  )
}

// ---------------------------------------------------------------------------
// Apps domain (Pass 15)
// ---------------------------------------------------------------------------

/// Shopify `MoneyV2` shape (`{ amount: String, currencyCode: String }`).
/// Many domains use this — defined here so the apps pass doesn't have to
/// invent its own. Future domain ports should reuse this rather than
/// rolling a private one.
pub type Money {
  Money(amount: String, currency_code: String)
}

/// Mirrors `AccessScopeRecord`. `description` is `None` for scopes the
/// proxy invents locally; upstream-hydrated scopes may carry one.
pub type AccessScopeRecord {
  AccessScopeRecord(handle: String, description: Option(String))
}

/// Mirrors `AppRecord`. Most fields are nullable in TS to model partially
/// populated upstream responses; the proxy's locally-minted default app
/// fills them all in.
pub type AppRecord {
  AppRecord(
    id: String,
    api_key: Option(String),
    handle: Option(String),
    title: Option(String),
    developer_name: Option(String),
    embedded: Option(Bool),
    previously_installed: Option(Bool),
    requested_access_scopes: List(AccessScopeRecord),
  )
}

/// Pricing shape attached to a subscription line item. Mirrors the
/// `AppRecurringPricing` / `AppUsagePricing` `__typename` discriminated
/// union — typed here as a sum so the variants can't get mixed.
pub type AppSubscriptionPricing {
  AppRecurringPricing(
    price: Money,
    interval: String,
    plan_handle: Option(String),
  )
  AppUsagePricing(
    capped_amount: Money,
    balance_used: Money,
    interval: String,
    terms: Option(String),
  )
}

/// Mirrors `AppSubscriptionLineItemRecord['plan']`. The TS schema is
/// `Record<string, jsonValue>`; we model the only shape the handler
/// actually produces — `{ pricingDetails: ... }` — so consumers get
/// type-checked access.
pub type AppSubscriptionLineItemPlan {
  AppSubscriptionLineItemPlan(pricing_details: AppSubscriptionPricing)
}

/// Mirrors `AppSubscriptionLineItemRecord`.
pub type AppSubscriptionLineItemRecord {
  AppSubscriptionLineItemRecord(
    id: String,
    subscription_id: String,
    plan: AppSubscriptionLineItemPlan,
  )
}

/// Mirrors `AppSubscriptionRecord`.
pub type AppSubscriptionRecord {
  AppSubscriptionRecord(
    id: String,
    name: String,
    status: String,
    is_test: Bool,
    trial_days: Option(Int),
    current_period_end: Option(String),
    created_at: String,
    line_item_ids: List(String),
  )
}

/// Mirrors `AppOneTimePurchaseRecord`.
pub type AppOneTimePurchaseRecord {
  AppOneTimePurchaseRecord(
    id: String,
    name: String,
    status: String,
    is_test: Bool,
    created_at: String,
    price: Money,
  )
}

/// Mirrors `AppUsageRecord`.
pub type AppUsageRecord {
  AppUsageRecord(
    id: String,
    subscription_line_item_id: String,
    description: String,
    price: Money,
    created_at: String,
    idempotency_key: Option(String),
  )
}

/// Mirrors `DelegatedAccessTokenRecord`. The proxy stores a sha256 of
/// the access token plus a redacted preview rather than the raw token —
/// the raw token is only returned in the create mutation response.
pub type DelegatedAccessTokenRecord {
  DelegatedAccessTokenRecord(
    id: String,
    api_client_id: String,
    parent_access_token_sha256: Option(String),
    access_token_sha256: String,
    access_token_preview: String,
    access_scopes: List(String),
    created_at: String,
    expires_in: Option(Int),
    destroyed_at: Option(String),
  )
}

/// Mirrors `AppInstallationRecord`. The proxy treats the
/// "current installation" as a singleton in the store; this record
/// captures everything else.
pub type AppInstallationRecord {
  AppInstallationRecord(
    id: String,
    app_id: String,
    launch_url: Option(String),
    uninstall_url: Option(String),
    access_scopes: List(AccessScopeRecord),
    active_subscription_ids: List(String),
    all_subscription_ids: List(String),
    one_time_purchase_ids: List(String),
    uninstalled_at: Option(String),
  )
}

// ---------------------------------------------------------------------------
// Functions domain (Pass 18)
// ---------------------------------------------------------------------------

/// Mirrors the selected `App` object carried by upstream-hydrated
/// `ShopifyFunctionRecord.app` metadata.
pub type ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: Option(String),
    id: Option(String),
    title: Option(String),
    handle: Option(String),
    api_key: Option(String),
  )
}

/// Mirrors `ShopifyFunctionRecord`. Locally minted records do not
/// invent app metadata, but captured/upstream-seeded records preserve
/// their owner app shape for downstream reads.
pub type ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    id: String,
    title: Option(String),
    handle: Option(String),
    api_type: Option(String),
    description: Option(String),
    app_key: Option(String),
    app: Option(ShopifyFunctionAppRecord),
  )
}

// ---------------------------------------------------------------------------
// Bulk operations domain
// ---------------------------------------------------------------------------

/// Mirrors `BulkOperationRecord` in `src/state/types.ts`. `result_jsonl`
/// is intentionally stored on the record in Gleam rather than a second
/// side-map until the HTTP result-file route ports.
pub type BulkOperationRecord {
  BulkOperationRecord(
    id: String,
    status: String,
    type_: String,
    error_code: Option(String),
    created_at: String,
    completed_at: Option(String),
    object_count: String,
    root_object_count: String,
    file_size: Option(String),
    url: Option(String),
    partial_data_url: Option(String),
    query: Option(String),
    cursor: Option(String),
    result_jsonl: Option(String),
  )
}

// ---------------------------------------------------------------------------
// Metaobjects domain
// ---------------------------------------------------------------------------

pub type MetaobjectJsonValue {
  MetaobjectNull
  MetaobjectString(String)
  MetaobjectBool(Bool)
  MetaobjectInt(Int)
  MetaobjectFloat(Float)
  MetaobjectList(List(MetaobjectJsonValue))
  MetaobjectObject(Dict(String, MetaobjectJsonValue))
}

pub type MetaobjectDefinitionCapabilityRecord {
  MetaobjectDefinitionCapabilityRecord(enabled: Bool)
}

pub type MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: Option(MetaobjectDefinitionCapabilityRecord),
    translatable: Option(MetaobjectDefinitionCapabilityRecord),
    renderable: Option(MetaobjectDefinitionCapabilityRecord),
    online_store: Option(MetaobjectDefinitionCapabilityRecord),
  )
}

pub type MetaobjectDefinitionTypeRecord {
  MetaobjectDefinitionTypeRecord(name: String, category: Option(String))
}

pub type MetaobjectFieldDefinitionValidationRecord {
  MetaobjectFieldDefinitionValidationRecord(name: String, value: Option(String))
}

pub type MetaobjectFieldDefinitionCapabilitiesRecord {
  MetaobjectFieldDefinitionCapabilitiesRecord(
    admin_filterable: Option(MetaobjectDefinitionCapabilityRecord),
  )
}

pub type MetaobjectFieldDefinitionRecord {
  MetaobjectFieldDefinitionRecord(
    key: String,
    name: Option(String),
    description: Option(String),
    required: Option(Bool),
    type_: MetaobjectDefinitionTypeRecord,
    capabilities: MetaobjectFieldDefinitionCapabilitiesRecord,
    validations: List(MetaobjectFieldDefinitionValidationRecord),
  )
}

pub type MetaobjectStandardTemplateRecord {
  MetaobjectStandardTemplateRecord(type_: Option(String), name: Option(String))
}

pub type MetaobjectDefinitionLinkedMetafieldRecord {
  MetaobjectDefinitionLinkedMetafieldRecord(
    owner_type: String,
    namespace: String,
    key: String,
    metafield_definition_id: Option(String),
    product_id: String,
    product_option_id: String,
  )
}

pub type MetaobjectDefinitionRecord {
  MetaobjectDefinitionRecord(
    id: String,
    type_: String,
    name: Option(String),
    description: Option(String),
    display_name_key: Option(String),
    access: Dict(String, Option(String)),
    capabilities: MetaobjectDefinitionCapabilitiesRecord,
    field_definitions: List(MetaobjectFieldDefinitionRecord),
    has_thumbnail_field: Option(Bool),
    metaobjects_count: Option(Int),
    standard_template: Option(MetaobjectStandardTemplateRecord),
    linked_metafields: List(MetaobjectDefinitionLinkedMetafieldRecord),
    created_at: Option(String),
    updated_at: Option(String),
  )
}

pub type MetaobjectFieldDefinitionReferenceRecord {
  MetaobjectFieldDefinitionReferenceRecord(
    key: String,
    name: Option(String),
    required: Option(Bool),
    type_: MetaobjectDefinitionTypeRecord,
  )
}

pub type MetaobjectFieldRecord {
  MetaobjectFieldRecord(
    key: String,
    type_: Option(String),
    value: Option(String),
    json_value: MetaobjectJsonValue,
    definition: Option(MetaobjectFieldDefinitionReferenceRecord),
  )
}

pub type MetaobjectPublishableCapabilityRecord {
  MetaobjectPublishableCapabilityRecord(status: Option(String))
}

pub type MetaobjectOnlineStoreCapabilityRecord {
  MetaobjectOnlineStoreCapabilityRecord(template_suffix: Option(String))
}

pub type MetaobjectCapabilitiesRecord {
  MetaobjectCapabilitiesRecord(
    publishable: Option(MetaobjectPublishableCapabilityRecord),
    online_store: Option(MetaobjectOnlineStoreCapabilityRecord),
  )
}

pub type MetaobjectRecord {
  MetaobjectRecord(
    id: String,
    handle: String,
    type_: String,
    display_name: Option(String),
    fields: List(MetaobjectFieldRecord),
    capabilities: MetaobjectCapabilitiesRecord,
    created_at: Option(String),
    updated_at: Option(String),
  )
}

// ---------------------------------------------------------------------------
// Marketing domain
// ---------------------------------------------------------------------------

/// JSON-shaped value carried by upstream-hydrated or locally staged marketing
/// activity/event/engagement records. The TypeScript store keeps these as
/// `Record<string, jsonValue>`; this ADT gives the Gleam port the same shape
/// without coupling state types back to the GraphQL projector module.
pub type MarketingValue {
  MarketingNull
  MarketingString(String)
  MarketingBool(Bool)
  MarketingInt(Int)
  MarketingFloat(Float)
  MarketingList(List(MarketingValue))
  MarketingObject(Dict(String, MarketingValue))
}

/// Mirrors `MarketingRecord`.
pub type MarketingRecord {
  MarketingRecord(
    id: String,
    cursor: Option(String),
    api_client_id: Option(String),
    data: Dict(String, MarketingValue),
  )
}

/// Local registry entry for Shopify MarketingChannelDefinition handles.
/// An empty `api_client_ids` list means the handle is recognized without
/// app scoping in local/snapshot fixtures that do not model a caller app.
pub type MarketingChannelDefinitionRecord {
  MarketingChannelDefinitionRecord(handle: String, api_client_ids: List(String))
}

/// Mirrors `MarketingEngagementRecord`.
pub type MarketingEngagementRecord {
  MarketingEngagementRecord(
    id: String,
    api_client_id: Option(String),
    marketing_activity_id: Option(String),
    remote_id: Option(String),
    channel_handle: Option(String),
    occurred_on: String,
    data: Dict(String, MarketingValue),
  )
}

/// Mirrors `ValidationRecord`. `enable`/`blockOnFailure` are nullable
/// in TS so the same handler can model partial upstream payloads;
/// here they're `Option(Bool)`.
pub type ValidationMetafieldRecord {
  ValidationMetafieldRecord(
    id: String,
    validation_id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
  )
}

pub type CartTransformMetafieldRecord {
  CartTransformMetafieldRecord(
    id: String,
    cart_transform_id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
  )
}

pub type ValidationRecord {
  ValidationRecord(
    id: String,
    title: Option(String),
    enable: Option(Bool),
    block_on_failure: Option(Bool),
    function_id: Option(String),
    function_handle: Option(String),
    shopify_function_id: Option(String),
    metafields: List(ValidationMetafieldRecord),
    created_at: Option(String),
    updated_at: Option(String),
  )
}

/// Mirrors `CartTransformRecord`. Same shape as `ValidationRecord`
/// minus the `enable` flag.
pub type CartTransformRecord {
  CartTransformRecord(
    id: String,
    title: Option(String),
    block_on_failure: Option(Bool),
    function_id: Option(String),
    function_handle: Option(String),
    shopify_function_id: Option(String),
    metafields: List(CartTransformMetafieldRecord),
    created_at: Option(String),
    updated_at: Option(String),
  )
}

/// Mirrors `TaxAppConfigurationRecord`. The proxy stores this as a
/// singleton (one configuration per shop), unlike the validation /
/// cart-transform records which live in keyed dictionaries.
pub type TaxAppConfigurationRecord {
  TaxAppConfigurationRecord(
    id: String,
    ready: Bool,
    state: String,
    updated_at: Option(String),
  )
}

// ---------------------------------------------------------------------------
// Gift cards domain (Pass 19)
// ---------------------------------------------------------------------------

/// Mirrors `GiftCardTransactionRecord`. `kind` is `"CREDIT"` or
/// `"DEBIT"` — kept as a `String` to match the TS literal-union shape;
/// the gift-card handler never inspects it as a sum.
pub type GiftCardTransactionRecord {
  GiftCardTransactionRecord(
    id: String,
    kind: String,
    amount: Money,
    processed_at: String,
    note: Option(String),
  )
}

/// Mirrors `GiftCardRecipientAttributesRecord`. Every field is nullable
/// in TS to match the Admin GraphQL schema; the proxy's create/update
/// helpers preserve null-vs-omit semantics by reading/writing
/// `Option(String)` here directly.
pub type GiftCardRecipientAttributesRecord {
  GiftCardRecipientAttributesRecord(
    id: Option(String),
    message: Option(String),
    preferred_name: Option(String),
    send_notification_at: Option(String),
  )
}

/// Mirrors `GiftCardRecord`. `recipient_attributes` is `None` for cards
/// minted without recipient input; the serializer falls back to a
/// constructed attributes record built from `recipient_id` if present.
pub type GiftCardRecord {
  GiftCardRecord(
    id: String,
    legacy_resource_id: String,
    last_characters: String,
    masked_code: String,
    code: Option(String),
    enabled: Bool,
    notify: Bool,
    deactivated_at: Option(String),
    expires_on: Option(String),
    note: Option(String),
    template_suffix: Option(String),
    created_at: String,
    updated_at: String,
    initial_value: Money,
    balance: Money,
    customer_id: Option(String),
    recipient_id: Option(String),
    source: Option(String),
    recipient_attributes: Option(GiftCardRecipientAttributesRecord),
    transactions: List(GiftCardTransactionRecord),
  )
}

/// Mirrors `GiftCardConfigurationRecord`. Stored as a singleton on the
/// store like `TaxAppConfigurationRecord` — one configuration per shop.
pub type GiftCardConfigurationRecord {
  GiftCardConfigurationRecord(issue_limit: Money, purchase_limit: Money)
}

// ---------------------------------------------------------------------------
// Customers domain
// ---------------------------------------------------------------------------

/// Mirrors `CustomerDefaultEmailAddressRecord`.
pub type CustomerDefaultEmailAddressRecord {
  CustomerDefaultEmailAddressRecord(
    email_address: Option(String),
    marketing_state: Option(String),
    marketing_opt_in_level: Option(String),
    marketing_updated_at: Option(String),
  )
}

/// Mirrors `CustomerDefaultPhoneNumberRecord`.
pub type CustomerDefaultPhoneNumberRecord {
  CustomerDefaultPhoneNumberRecord(
    phone_number: Option(String),
    marketing_state: Option(String),
    marketing_opt_in_level: Option(String),
    marketing_updated_at: Option(String),
    marketing_collected_from: Option(String),
  )
}

/// Mirrors `CustomerEmailMarketingConsentRecord`.
pub type CustomerEmailMarketingConsentRecord {
  CustomerEmailMarketingConsentRecord(
    marketing_state: Option(String),
    marketing_opt_in_level: Option(String),
    consent_updated_at: Option(String),
  )
}

/// Mirrors `CustomerSmsMarketingConsentRecord`.
pub type CustomerSmsMarketingConsentRecord {
  CustomerSmsMarketingConsentRecord(
    marketing_state: Option(String),
    marketing_opt_in_level: Option(String),
    consent_updated_at: Option(String),
    consent_collected_from: Option(String),
  )
}

/// Mirrors `CustomerDefaultAddressRecord`.
pub type CustomerDefaultAddressRecord {
  CustomerDefaultAddressRecord(
    id: Option(String),
    first_name: Option(String),
    last_name: Option(String),
    address1: Option(String),
    address2: Option(String),
    city: Option(String),
    company: Option(String),
    province: Option(String),
    province_code: Option(String),
    country: Option(String),
    country_code_v2: Option(String),
    zip: Option(String),
    phone: Option(String),
    name: Option(String),
    formatted_area: Option(String),
  )
}

/// Mirrors `CustomerAddressRecord`.
pub type CustomerAddressRecord {
  CustomerAddressRecord(
    id: String,
    customer_id: String,
    cursor: Option(String),
    position: Int,
    first_name: Option(String),
    last_name: Option(String),
    address1: Option(String),
    address2: Option(String),
    city: Option(String),
    company: Option(String),
    province: Option(String),
    province_code: Option(String),
    country: Option(String),
    country_code_v2: Option(String),
    zip: Option(String),
    phone: Option(String),
    name: Option(String),
    formatted_area: Option(String),
  )
}

/// Mirrors `CustomerRecord`.
pub type CustomerRecord {
  CustomerRecord(
    id: String,
    first_name: Option(String),
    last_name: Option(String),
    display_name: Option(String),
    email: Option(String),
    legacy_resource_id: Option(String),
    locale: Option(String),
    note: Option(String),
    can_delete: Option(Bool),
    verified_email: Option(Bool),
    data_sale_opt_out: Bool,
    tax_exempt: Option(Bool),
    tax_exemptions: List(String),
    state: Option(String),
    tags: List(String),
    number_of_orders: Option(String),
    amount_spent: Option(Money),
    default_email_address: Option(CustomerDefaultEmailAddressRecord),
    default_phone_number: Option(CustomerDefaultPhoneNumberRecord),
    email_marketing_consent: Option(CustomerEmailMarketingConsentRecord),
    sms_marketing_consent: Option(CustomerSmsMarketingConsentRecord),
    default_address: Option(CustomerDefaultAddressRecord),
    account_activation_token: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
  )
}

pub type CustomerCatalogPageInfoRecord {
  CustomerCatalogPageInfoRecord(
    has_next_page: Bool,
    has_previous_page: Bool,
    start_cursor: Option(String),
    end_cursor: Option(String),
  )
}

pub type CustomerCatalogConnectionRecord {
  CustomerCatalogConnectionRecord(
    ordered_customer_ids: List(String),
    cursor_by_customer_id: Dict(String, String),
    page_info: CustomerCatalogPageInfoRecord,
  )
}

/// Minimal customer-owned order summary used by Customer.orders/lastOrder.
pub type CustomerOrderSummaryRecord {
  CustomerOrderSummaryRecord(
    id: String,
    customer_id: Option(String),
    cursor: Option(String),
    name: Option(String),
    email: Option(String),
    created_at: Option(String),
    current_total_price: Option(Money),
  )
}

/// Minimal customer-owned event summary used by Customer.events.
pub type CustomerEventSummaryRecord {
  CustomerEventSummaryRecord(
    id: String,
    customer_id: String,
    cursor: Option(String),
  )
}

/// Customer-owned metafield record.
pub type CustomerMetafieldRecord {
  CustomerMetafieldRecord(
    id: String,
    customer_id: String,
    namespace: String,
    key: String,
    type_: String,
    value: String,
    compare_digest: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
  )
}

/// Mirrors `CustomerPaymentMethodInstrumentRecord` as a JSON-ish string map.
pub type CustomerPaymentMethodInstrumentRecord {
  CustomerPaymentMethodInstrumentRecord(
    type_name: String,
    data: Dict(String, String),
  )
}

/// Mirrors `CustomerPaymentMethodSubscriptionContractRecord`.
pub type CustomerPaymentMethodSubscriptionContractRecord {
  CustomerPaymentMethodSubscriptionContractRecord(
    id: String,
    cursor: Option(String),
    data: Dict(String, String),
  )
}

/// Mirrors `CustomerPaymentMethodRecord`.
pub type CustomerPaymentMethodRecord {
  CustomerPaymentMethodRecord(
    id: String,
    customer_id: String,
    cursor: Option(String),
    instrument: Option(CustomerPaymentMethodInstrumentRecord),
    revoked_at: Option(String),
    revoked_reason: Option(String),
    subscription_contracts: List(
      CustomerPaymentMethodSubscriptionContractRecord,
    ),
  )
}

/// Mirrors `CustomerPaymentMethodUpdateUrlRecord`.
pub type CustomerPaymentMethodUpdateUrlRecord {
  CustomerPaymentMethodUpdateUrlRecord(
    id: String,
    customer_payment_method_id: String,
    update_payment_method_url: String,
    created_at: String,
  )
}

/// Mirrors `PaymentReminderSendRecord`.
pub type PaymentReminderSendRecord {
  PaymentReminderSendRecord(
    id: String,
    payment_schedule_id: String,
    sent_at: String,
  )
}

/// Payment-customization owned metafield row.
pub type PaymentCustomizationMetafieldRecord {
  PaymentCustomizationMetafieldRecord(
    id: String,
    payment_customization_id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
  )
}

/// Mirrors `PaymentCustomizationRecord`.
pub type PaymentCustomizationRecord {
  PaymentCustomizationRecord(
    id: String,
    title: Option(String),
    enabled: Option(Bool),
    function_id: Option(String),
    function_handle: Option(String),
    metafields: List(PaymentCustomizationMetafieldRecord),
  )
}

/// Mirrors `PaymentTermsTemplateRecord`.
pub type PaymentTermsTemplateRecord {
  PaymentTermsTemplateRecord(
    id: String,
    name: String,
    description: String,
    due_in_days: Option(Int),
    payment_terms_type: String,
    translated_name: String,
  )
}

/// Mirrors the payment-schedule projection used by payment terms.
pub type PaymentScheduleRecord {
  PaymentScheduleRecord(
    id: String,
    due_at: Option(String),
    issued_at: Option(String),
    completed_at: Option(String),
    due: Option(Bool),
    amount: Option(Money),
    balance_due: Option(Money),
    total_balance: Option(Money),
  )
}

/// Normalized payment terms staged against an order or draft order owner.
pub type PaymentTermsRecord {
  PaymentTermsRecord(
    id: String,
    owner_id: String,
    due: Bool,
    overdue: Bool,
    due_in_days: Option(Int),
    payment_terms_name: String,
    payment_terms_type: String,
    translated_name: String,
    payment_schedules: List(PaymentScheduleRecord),
  )
}

/// Idempotency record for `orderCreateMandatePayment`.
pub type OrderMandatePaymentRecord {
  OrderMandatePaymentRecord(
    order_id: String,
    idempotency_key: String,
    job_id: String,
    payment_reference_id: String,
    transaction_id: String,
  )
}

/// Mirrors `StoreCreditAccountTransactionRecord`.
pub type StoreCreditAccountTransactionRecord {
  StoreCreditAccountTransactionRecord(
    id: String,
    account_id: String,
    amount: Money,
    balance_after_transaction: Money,
    created_at: String,
    event: String,
  )
}

/// Mirrors `StoreCreditAccountRecord`.
pub type StoreCreditAccountRecord {
  StoreCreditAccountRecord(
    id: String,
    customer_id: String,
    cursor: Option(String),
    balance: Money,
  )
}

/// Mirrors `CustomerAccountPageRecord`.
pub type CustomerAccountPageRecord {
  CustomerAccountPageRecord(
    id: String,
    title: String,
    handle: String,
    default_cursor: String,
    cursor: Option(String),
  )
}

/// Mirrors `CustomerDataErasureRequestRecord`.
pub type CustomerDataErasureRequestRecord {
  CustomerDataErasureRequestRecord(
    customer_id: String,
    requested_at: String,
    canceled_at: Option(String),
  )
}

/// Mirrors `CustomerMergeRequestRecord`.
pub type CustomerMergeErrorRecord {
  CustomerMergeErrorRecord(
    error_fields: List(String),
    message: String,
    code: Option(String),
    block_type: Option(String),
  )
}

pub type CustomerMergeRequestRecord {
  CustomerMergeRequestRecord(
    job_id: String,
    resulting_customer_id: String,
    status: String,
    customer_merge_errors: List(CustomerMergeErrorRecord),
  )
}

/// Mirrors `SegmentRecord`. Customer segments are upstream resources the
/// proxy mirrors locally so create/update/delete mutations can be staged
/// without contacting Admin. Every field except `id` is nullable to match
/// the Admin GraphQL schema.
pub type SegmentRecord {
  SegmentRecord(
    id: String,
    name: Option(String),
    query: Option(String),
    creation_date: Option(String),
    last_edit_date: Option(String),
  )
}

/// Mirrors `CustomerSegmentMembersQueryRecord`. A staged record captures
/// the resolved query string + originating segmentId, plus Shopify's
/// asynchronous query-job status fields.
pub type CustomerSegmentMembersQueryRecord {
  CustomerSegmentMembersQueryRecord(
    id: String,
    query: Option(String),
    segment_id: Option(String),
    status: String,
    current_count: Int,
    done: Bool,
  )
}

// ---------------------------------------------------------------------------
// Localization domain (Pass 23)
// ---------------------------------------------------------------------------

/// Mirrors `LocaleRecord`. The catalog of every locale Shopify recognises
/// (independent of which ones the shop has enabled).
pub type LocaleRecord {
  LocaleRecord(iso_code: String, name: String)
}

/// Mirrors `ShopLocaleRecord`. The set of locales this shop has enabled,
/// each with its primary/published flags and any market web-presence
/// pinning. `market_web_presence_ids` defaults to `[]` for shops without
/// markets configured.
pub type ShopLocaleRecord {
  ShopLocaleRecord(
    locale: String,
    name: String,
    primary: Bool,
    published: Bool,
    market_web_presence_ids: List(String),
  )
}

/// Mirrors `TranslationRecord`. One translation entry keyed by
/// `(resource_id, locale, market_id, key)`. `translatable_content_digest`
/// is the upstream digest the client supplied at register time;
/// `outdated` flips to `True` when the underlying source content
/// changes (this port treats every staged translation as fresh —
/// `outdated: False` — until source-content tracking ports).
pub type TranslationRecord {
  TranslationRecord(
    resource_id: String,
    key: String,
    locale: String,
    value: String,
    translatable_content_digest: String,
    market_id: Option(String),
    updated_at: String,
    outdated: Bool,
  )
}

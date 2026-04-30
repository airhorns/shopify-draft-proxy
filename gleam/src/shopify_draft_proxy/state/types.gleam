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
import gleam/option.{type Option}

// ---------------------------------------------------------------------------
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

pub type ProductVariantSelectedOptionRecord {
  ProductVariantSelectedOptionRecord(name: String, value: String)
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
  LocationRecord(id: String, name: String, cursor: Option(String))
}

pub type PublicationRecord {
  PublicationRecord(id: String, name: String, cursor: Option(String))
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
    inventory_item: Option(InventoryItemRecord),
    cursor: Option(String),
  )
}

pub type ProductOptionValueRecord {
  ProductOptionValueRecord(id: String, name: String, has_variants: Bool)
}

pub type ProductOptionRecord {
  ProductOptionRecord(
    id: String,
    product_id: String,
    name: String,
    position: Int,
    option_values: List(ProductOptionValueRecord),
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
    total_inventory: Option(Int),
    tracks_inventory: Option(Bool),
    created_at: Option(String),
    updated_at: Option(String),
    description_html: String,
    online_store_preview_url: Option(String),
    template_suffix: Option(String),
    seo: ProductSeoRecord,
    category: Option(ProductCategoryRecord),
    cursor: Option(String),
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
    sells_subscriptions: Bool,
    show_metrics: Bool,
    storefront: Bool,
    unified_markets: Bool,
  )
}

pub type PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: List(String))
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
    data: Dict(String, MarketingValue),
  )
}

/// Mirrors `MarketingEngagementRecord`.
pub type MarketingEngagementRecord {
  MarketingEngagementRecord(
    id: String,
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
pub type ValidationRecord {
  ValidationRecord(
    id: String,
    title: Option(String),
    enable: Option(Bool),
    block_on_failure: Option(Bool),
    function_id: Option(String),
    function_handle: Option(String),
    shopify_function_id: Option(String),
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
    enabled: Bool,
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
/// the resolved query string + originating segmentId, plus the
/// realized member count and `done` flag. The proxy stages these in
/// finished form (done=true) at create time; the create-mutation
/// response shape returns currentCount=0/done=false to match Shopify's
/// asynchronous job semantics.
pub type CustomerSegmentMembersQueryRecord {
  CustomerSegmentMembersQueryRecord(
    id: String,
    query: Option(String),
    segment_id: Option(String),
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

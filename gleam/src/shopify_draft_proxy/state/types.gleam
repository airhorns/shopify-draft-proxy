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

import gleam/option.{type Option}

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

/// Mirrors `ShopifyFunctionRecord`. The TS schema also carries an
/// `app: jsonObjectSchema.optional()` field for upstream-hydrated
/// functions; the Gleam port omits it because the proxy never mints
/// app metadata locally — `app` always projects to `null` until
/// upstream hydration lands.
pub type ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    id: String,
    title: Option(String),
    handle: Option(String),
    api_type: Option(String),
    description: Option(String),
    app_key: Option(String),
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

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

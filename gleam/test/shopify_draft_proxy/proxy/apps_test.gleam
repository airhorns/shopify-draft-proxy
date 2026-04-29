//// Read-path smoke tests for `proxy/apps`.
////
//// Covers the six query roots (`app`, `appByHandle`, `appByKey`,
//// `appInstallation`, `appInstallations`, `currentAppInstallation`)
//// plus a handful of projection edge cases (the `AppSubscriptionPricing`
//// __typename split, child connections off `AppInstallation`).

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppRecord, type Money, AccessScopeRecord,
  AppInstallationRecord, AppOneTimePurchaseRecord, AppRecord, AppRecurringPricing,
  AppSubscriptionLineItemPlan, AppSubscriptionLineItemRecord,
  AppSubscriptionRecord, AppUsagePricing, AppUsageRecord, Money,
}

// ----------- Helpers -----------

fn money(amount: String, code: String) -> Money {
  Money(amount: amount, currency_code: code)
}

fn app(id: String, handle: String, api_key: String) -> AppRecord {
  AppRecord(
    id: id,
    api_key: Some(api_key),
    handle: Some(handle),
    title: Some(handle),
    developer_name: Some("test-dev"),
    embedded: Some(True),
    previously_installed: Some(False),
    requested_access_scopes: [
      AccessScopeRecord(handle: "read_products", description: None),
    ],
  )
}

fn installation(id: String, app_id: String) -> AppInstallationRecord {
  AppInstallationRecord(
    id: id,
    app_id: app_id,
    launch_url: Some("https://launch"),
    uninstall_url: None,
    access_scopes: [AccessScopeRecord(handle: "read_products", description: None)],
    active_subscription_ids: [],
    all_subscription_ids: [],
    one_time_purchase_ids: [],
    uninstalled_at: None,
  )
}

fn run(store: store.Store, query: String) -> String {
  let assert Ok(data) = apps.handle_app_query(store, query, dict.new())
  json.to_string(data)
}

// ----------- is_app_query_root -----------

pub fn is_app_query_root_test() {
  assert apps.is_app_query_root("app")
  assert apps.is_app_query_root("appByHandle")
  assert apps.is_app_query_root("appByKey")
  assert apps.is_app_query_root("appInstallation")
  assert apps.is_app_query_root("appInstallations")
  assert apps.is_app_query_root("currentAppInstallation")
  assert !apps.is_app_query_root("event")
  assert !apps.is_app_query_root("webhookSubscription")
}

// ----------- currentAppInstallation -----------

pub fn current_app_installation_returns_null_when_none_test() {
  let result = run(store.new(), "{ currentAppInstallation { id } }")
  assert result == "{\"currentAppInstallation\":null}"
}

pub fn current_app_installation_returns_installation_test() {
  let a = app("gid://shopify/App/1", "my-app", "key-1")
  let i = installation("gid://shopify/AppInstallation/1", a.id)
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { __typename id launchUrl app { id handle } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"__typename\":\"AppInstallation\",\"id\":\"gid://shopify/AppInstallation/1\",\"launchUrl\":\"https://launch\",\"app\":{\"id\":\"gid://shopify/App/1\",\"handle\":\"my-app\"}}}"
}

// ----------- app(id:) -----------

pub fn app_by_id_returns_record_test() {
  let a = app("gid://shopify/App/2", "second", "key-2")
  let s = store.upsert_base_app(store.new(), a)
  let result =
    run(
      s,
      "{ app(id: \"gid://shopify/App/2\") { __typename id handle apiKey title } }",
    )
  assert result
    == "{\"app\":{\"__typename\":\"App\",\"id\":\"gid://shopify/App/2\",\"handle\":\"second\",\"apiKey\":\"key-2\",\"title\":\"second\"}}"
}

pub fn app_by_id_missing_returns_null_test() {
  let result =
    run(
      store.new(),
      "{ app(id: \"gid://shopify/App/missing\") { id handle } }",
    )
  assert result == "{\"app\":null}"
}

pub fn app_by_id_missing_argument_returns_null_test() {
  let result = run(store.new(), "{ app { id handle } }")
  assert result == "{\"app\":null}"
}

// ----------- appByHandle / appByKey -----------

pub fn app_by_handle_returns_match_test() {
  let a = app("gid://shopify/App/3", "third", "key-3")
  let s = store.upsert_base_app(store.new(), a)
  let result =
    run(s, "{ appByHandle(handle: \"third\") { id handle } }")
  assert result
    == "{\"appByHandle\":{\"id\":\"gid://shopify/App/3\",\"handle\":\"third\"}}"
}

pub fn app_by_handle_no_match_test() {
  let result =
    run(
      store.new(),
      "{ appByHandle(handle: \"missing\") { id } }",
    )
  assert result == "{\"appByHandle\":null}"
}

pub fn app_by_key_returns_match_test() {
  let a = app("gid://shopify/App/4", "fourth", "key-4")
  let s = store.upsert_base_app(store.new(), a)
  let result =
    run(s, "{ appByKey(apiKey: \"key-4\") { id apiKey } }")
  assert result
    == "{\"appByKey\":{\"id\":\"gid://shopify/App/4\",\"apiKey\":\"key-4\"}}"
}

// ----------- appInstallation(id:) -----------

pub fn app_installation_by_id_returns_record_test() {
  let a = app("gid://shopify/App/5", "fifth", "key-5")
  let i = installation("gid://shopify/AppInstallation/5", a.id)
  let s =
    store.upsert_base_app_installation(store.new(), i, a)
  let result =
    run(
      s,
      "{ appInstallation(id: \"gid://shopify/AppInstallation/5\") { __typename id app { id } } }",
    )
  assert result
    == "{\"appInstallation\":{\"__typename\":\"AppInstallation\",\"id\":\"gid://shopify/AppInstallation/5\",\"app\":{\"id\":\"gid://shopify/App/5\"}}}"
}

pub fn app_installation_by_id_missing_test() {
  let result =
    run(
      store.new(),
      "{ appInstallation(id: \"missing\") { id } }",
    )
  assert result == "{\"appInstallation\":null}"
}

// ----------- appInstallations connection -----------

pub fn app_installations_connection_empty_test() {
  let result =
    run(
      store.new(),
      "{ appInstallations(first: 5) { nodes { id } } }",
    )
  assert result == "{\"appInstallations\":{\"nodes\":[]}}"
}

pub fn app_installations_connection_returns_current_test() {
  let a = app("gid://shopify/App/6", "sixth", "key-6")
  let i = installation("gid://shopify/AppInstallation/6", a.id)
  let s = store.upsert_base_app_installation(store.new(), i, a)
  let result =
    run(s, "{ appInstallations(first: 5) { nodes { id } } }")
  assert result
    == "{\"appInstallations\":{\"nodes\":[{\"id\":\"gid://shopify/AppInstallation/6\"}]}}"
}

// ----------- access scopes projection -----------

pub fn current_installation_access_scopes_projection_test() {
  let a = app("gid://shopify/App/7", "seventh", "key-7")
  let i = installation("gid://shopify/AppInstallation/7", a.id)
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { accessScopes { handle description } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"accessScopes\":[{\"handle\":\"read_products\",\"description\":null}]}}"
}

// ----------- subscription pricing __typename split -----------

pub fn subscription_recurring_pricing_typename_test() {
  let a = app("gid://shopify/App/8", "eighth", "key-8")
  let li_id = "gid://shopify/AppSubscriptionLineItem/8"
  let sub_id = "gid://shopify/AppSubscription/8"
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(
        pricing_details: AppRecurringPricing(
          price: money("9.99", "USD"),
          interval: "EVERY_30_DAYS",
          plan_handle: Some("standard"),
        ),
      ),
    )
  let sub =
    AppSubscriptionRecord(
      id: sub_id,
      name: "Standard plan",
      status: "ACTIVE",
      is_test: True,
      trial_days: Some(7),
      current_period_end: Some("2025-01-01T00:00:00Z"),
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [li_id],
    )
  let i =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/8",
      app_id: a.id,
      launch_url: Some("https://launch"),
      uninstall_url: None,
      access_scopes: [],
      active_subscription_ids: [sub_id],
      all_subscription_ids: [sub_id],
      one_time_purchase_ids: [],
      uninstalled_at: None,
    )
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription_line_item(s, li)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription(s, sub)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { activeSubscriptions { name lineItems { plan { pricingDetails { __typename ... on AppRecurringPricing { interval price { amount currencyCode } } } } } } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"activeSubscriptions\":[{\"name\":\"Standard plan\",\"lineItems\":[{\"plan\":{\"pricingDetails\":{\"__typename\":\"AppRecurringPricing\",\"interval\":\"EVERY_30_DAYS\",\"price\":{\"amount\":\"9.99\",\"currencyCode\":\"USD\"}}}}]}]}}"
}

pub fn subscription_usage_pricing_typename_test() {
  let a = app("gid://shopify/App/9", "ninth", "key-9")
  let li_id = "gid://shopify/AppSubscriptionLineItem/9"
  let sub_id = "gid://shopify/AppSubscription/9"
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(
        pricing_details: AppUsagePricing(
          capped_amount: money("100.00", "USD"),
          balance_used: money("0.00", "USD"),
          interval: "ANNUAL",
          terms: Some("per ticket"),
        ),
      ),
    )
  let sub =
    AppSubscriptionRecord(
      id: sub_id,
      name: "Usage plan",
      status: "ACTIVE",
      is_test: False,
      trial_days: None,
      current_period_end: None,
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [li_id],
    )
  let i =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/9",
      app_id: a.id,
      launch_url: None,
      uninstall_url: None,
      access_scopes: [],
      active_subscription_ids: [sub_id],
      all_subscription_ids: [sub_id],
      one_time_purchase_ids: [],
      uninstalled_at: None,
    )
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription_line_item(s, li)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription(s, sub)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { activeSubscriptions { lineItems { plan { pricingDetails { __typename ... on AppUsagePricing { interval terms cappedAmount { amount } } } } } } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"activeSubscriptions\":[{\"lineItems\":[{\"plan\":{\"pricingDetails\":{\"__typename\":\"AppUsagePricing\",\"interval\":\"ANNUAL\",\"terms\":\"per ticket\",\"cappedAmount\":{\"amount\":\"100.00\"}}}}]}]}}"
}

// ----------- one-time purchases connection -----------

pub fn one_time_purchases_connection_test() {
  let a = app("gid://shopify/App/10", "tenth", "key-10")
  let p_id = "gid://shopify/AppPurchaseOneTime/10"
  let purchase =
    AppOneTimePurchaseRecord(
      id: p_id,
      name: "One-time",
      status: "ACTIVE",
      is_test: True,
      created_at: "2024-12-01T00:00:00Z",
      price: money("19.00", "USD"),
    )
  let i =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/10",
      app_id: a.id,
      launch_url: None,
      uninstall_url: None,
      access_scopes: [],
      active_subscription_ids: [],
      all_subscription_ids: [],
      one_time_purchase_ids: [p_id],
      uninstalled_at: None,
    )
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_one_time_purchase(s, purchase)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { oneTimePurchases { nodes { __typename id name price { amount } } } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"oneTimePurchases\":{\"nodes\":[{\"__typename\":\"AppPurchaseOneTime\",\"id\":\"gid://shopify/AppPurchaseOneTime/10\",\"name\":\"One-time\",\"price\":{\"amount\":\"19.00\"}}]}}}"
}

// ----------- usage records connection on a line item -----------

pub fn line_item_usage_records_connection_test() {
  let a = app("gid://shopify/App/11", "eleventh", "key-11")
  let li_id = "gid://shopify/AppSubscriptionLineItem/11"
  let sub_id = "gid://shopify/AppSubscription/11"
  let usage_id = "gid://shopify/AppUsageRecord/11"
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(
        pricing_details: AppUsagePricing(
          capped_amount: money("50.00", "USD"),
          balance_used: money("3.00", "USD"),
          interval: "ANNUAL",
          terms: None,
        ),
      ),
    )
  let usage =
    AppUsageRecord(
      id: usage_id,
      subscription_line_item_id: li_id,
      description: "API call",
      price: money("3.00", "USD"),
      created_at: "2024-12-15T00:00:00Z",
      idempotency_key: Some("idem-1"),
    )
  let sub =
    AppSubscriptionRecord(
      id: sub_id,
      name: "Usage plan",
      status: "ACTIVE",
      is_test: False,
      trial_days: None,
      current_period_end: None,
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [li_id],
    )
  let i =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/11",
      app_id: a.id,
      launch_url: None,
      uninstall_url: None,
      access_scopes: [],
      active_subscription_ids: [sub_id],
      all_subscription_ids: [sub_id],
      one_time_purchase_ids: [],
      uninstalled_at: None,
    )
  let s =
    store.new()
    |> fn(s) { store.upsert_base_app(s, a) }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription_line_item(s, li)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_subscription(s, sub)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_usage_record(s, usage)
      s
    }
    |> fn(s) {
      let #(_, s) = store.stage_app_installation(s, i)
      s
    }
  let result =
    run(
      s,
      "{ currentAppInstallation { activeSubscriptions { lineItems { usageRecords { nodes { id description price { amount } } } } } } }",
    )
  assert result
    == "{\"currentAppInstallation\":{\"activeSubscriptions\":[{\"lineItems\":[{\"usageRecords\":{\"nodes\":[{\"id\":\"gid://shopify/AppUsageRecord/11\",\"description\":\"API call\",\"price\":{\"amount\":\"3.00\"}}]}}]}]}}"
}

// ----------- process wraps in `data` envelope -----------

pub fn process_wraps_data_envelope_test() {
  let a = app("gid://shopify/App/12", "twelfth", "key-12")
  let s = store.upsert_base_app(store.new(), a)
  let assert Ok(envelope) =
    apps.process(
      s,
      "{ app(id: \"gid://shopify/App/12\") { id } }",
      dict.new(),
    )
  assert json.to_string(envelope)
    == "{\"data\":{\"app\":{\"id\":\"gid://shopify/App/12\"}}}"
}

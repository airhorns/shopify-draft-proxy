import gleam/dict
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppRecord, type Money,
  type WebhookSubscriptionRecord, AccessScopeRecord, AppInstallationRecord,
  AppOneTimePurchaseRecord, AppRecord, AppRecurringPricing,
  AppSubscriptionLineItemPlan, AppSubscriptionLineItemRecord,
  AppSubscriptionRecord, AppUsageRecord, DelegatedAccessTokenRecord, Money,
  SavedSearchRecord, WebhookHttpEndpoint, WebhookSubscriptionRecord,
}

fn record(id: String, name: String) -> types.SavedSearchRecord {
  SavedSearchRecord(
    id: id,
    legacy_resource_id: id,
    name: name,
    query: "status:open",
    resource_type: "ORDER",
    search_terms: "",
    filters: [],
    cursor: None,
  )
}

pub fn new_store_is_empty_test() {
  let s = store.new()
  assert store.list_effective_saved_searches(s) == []
  assert store.get_log(s) == []
}

pub fn upsert_base_saved_searches_orders_inserts_test() {
  let s =
    store.upsert_base_saved_searches(store.new(), [
      record("a", "A"),
      record("b", "B"),
    ])
  let names =
    store.list_effective_saved_searches(s)
    |> list.map(fn(r) { r.name })
  assert names == ["A", "B"]
}

pub fn staged_overrides_base_test() {
  let base = record("a", "Base A")
  let staged = record("a", "Staged A")
  let s =
    store.new()
    |> store.upsert_base_saved_searches([base])
  let #(_, s) = store.upsert_staged_saved_search(s, staged)
  let assert Some(found) = store.get_effective_saved_search_by_id(s, "a")
  assert found.name == "Staged A"
}

pub fn delete_staged_hides_record_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
  let s = store.delete_staged_saved_search(s, "a")
  assert store.get_effective_saved_search_by_id(s, "a") == None
  assert store.list_effective_saved_searches(s) == []
}

pub fn upsert_base_clears_deleted_marker_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
    |> store.delete_staged_saved_search("a")
  let s = store.upsert_base_saved_searches(s, [record("a", "A again")])
  let assert Some(found) = store.get_effective_saved_search_by_id(s, "a")
  assert found.name == "A again"
}

pub fn list_returns_ordered_then_unordered_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("z", "Z"), record("a", "A")])
  let names =
    store.list_effective_saved_searches(s)
    |> list.map(fn(r) { r.name })
  assert names == ["Z", "A"]
}

pub fn get_log_preserves_insertion_order_test() {
  let entry1 =
    store_types.MutationLogEntry(
      id: "log-1",
      received_at: "2024-01-01T00:00:00.000Z",
      operation_name: None,
      path: "/admin/api/2025-01/graphql.json",
      query: "mutation { ... }",
      variables: dict.new(),
      staged_resource_ids: [],
      status: store_types.Staged,
      interpreted: store_types.InterpretedMetadata(
        operation_type: store_types.Mutation,
        operation_name: None,
        root_fields: ["x"],
        primary_root_field: Some("x"),
        capability: store_types.Capability(
          operation_name: Some("x"),
          domain: "saved-searches",
          execution: "stage-locally",
        ),
      ),
      notes: None,
    )
  let entry2 = store_types.MutationLogEntry(..entry1, id: "log-2")
  let s =
    store.new()
    |> store.record_mutation_log_entry(entry1)
    |> store.record_mutation_log_entry(entry2)
  let ids = list.map(store.get_log(s), fn(e) { e.id })
  assert ids == ["log-1", "log-2"]
}

pub fn reset_clears_state_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
  assert list.length(store.list_effective_saved_searches(s)) == 1
  let s = store.reset(s)
  assert store.list_effective_saved_searches(s) == []
}

// ---------------------------------------------------------------------------
// Webhook subscription slice
// ---------------------------------------------------------------------------

fn webhook(
  id: String,
  topic: String,
  url: String,
) -> WebhookSubscriptionRecord {
  WebhookSubscriptionRecord(
    id: id,
    topic: Some(topic),
    uri: Some(url),
    name: None,
    format: Some("JSON"),
    include_fields: [],
    metafield_namespaces: [],
    filter: None,
    created_at: Some("2024-01-01T00:00:00Z"),
    updated_at: Some("2024-01-02T00:00:00Z"),
    endpoint: Some(WebhookHttpEndpoint(callback_url: Some(url))),
  )
}

pub fn upsert_base_webhooks_orders_inserts_test() {
  let s =
    store.upsert_base_webhook_subscriptions(store.new(), [
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_CREATE",
        "https://a",
      ),
      webhook(
        "gid://shopify/WebhookSubscription/2",
        "ORDERS_UPDATE",
        "https://b",
      ),
    ])
  let topics =
    store.list_effective_webhook_subscriptions(s)
    |> list.map(fn(r) { r.topic })
  assert topics == [Some("ORDERS_CREATE"), Some("ORDERS_UPDATE")]
}

pub fn webhook_staged_overrides_base_test() {
  let base =
    webhook(
      "gid://shopify/WebhookSubscription/1",
      "ORDERS_CREATE",
      "https://base",
    )
  let staged =
    webhook(
      "gid://shopify/WebhookSubscription/1",
      "ORDERS_UPDATE",
      "https://staged",
    )
  let s = store.upsert_base_webhook_subscriptions(store.new(), [base])
  let #(_, s) = store.upsert_staged_webhook_subscription(s, staged)
  let assert Some(found) =
    store.get_effective_webhook_subscription_by_id(
      s,
      "gid://shopify/WebhookSubscription/1",
    )
  assert found.topic == Some("ORDERS_UPDATE")
}

pub fn delete_staged_webhook_hides_record_test() {
  let s =
    store.new()
    |> store.upsert_base_webhook_subscriptions([
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_CREATE",
        "https://a",
      ),
    ])
  let s =
    store.delete_staged_webhook_subscription(
      s,
      "gid://shopify/WebhookSubscription/1",
    )
  assert store.get_effective_webhook_subscription_by_id(
      s,
      "gid://shopify/WebhookSubscription/1",
    )
    == None
  assert store.list_effective_webhook_subscriptions(s) == []
}

pub fn upsert_base_webhook_clears_deleted_marker_test() {
  let s =
    store.new()
    |> store.upsert_base_webhook_subscriptions([
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_CREATE",
        "https://a",
      ),
    ])
    |> store.delete_staged_webhook_subscription(
      "gid://shopify/WebhookSubscription/1",
    )
  let s =
    store.upsert_base_webhook_subscriptions(s, [
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_CREATE",
        "https://a2",
      ),
    ])
  let assert Some(found) =
    store.get_effective_webhook_subscription_by_id(
      s,
      "gid://shopify/WebhookSubscription/1",
    )
  assert found.uri == Some("https://a2")
}

pub fn webhook_list_orders_then_unordered_test() {
  let s =
    store.new()
    |> store.upsert_base_webhook_subscriptions([
      webhook(
        "gid://shopify/WebhookSubscription/9",
        "ORDERS_CREATE",
        "https://9",
      ),
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_UPDATE",
        "https://1",
      ),
    ])
  let topics =
    store.list_effective_webhook_subscriptions(s)
    |> list.map(fn(r) { r.topic })
  assert topics == [Some("ORDERS_CREATE"), Some("ORDERS_UPDATE")]
}

pub fn reset_clears_webhook_state_test() {
  let s =
    store.new()
    |> store.upsert_base_webhook_subscriptions([
      webhook(
        "gid://shopify/WebhookSubscription/1",
        "ORDERS_CREATE",
        "https://a",
      ),
    ])
  assert list.length(store.list_effective_webhook_subscriptions(s)) == 1
  let s = store.reset(s)
  assert store.list_effective_webhook_subscriptions(s) == []
}

// ----------- Apps slice (Pass 15) -----------

fn money(amount: String, currency_code: String) -> Money {
  Money(amount: amount, currency_code: currency_code)
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
    launch_url: Some("https://example.com/admin/apps/test"),
    uninstall_url: None,
    access_scopes: [],
    active_subscription_ids: [],
    all_subscription_ids: [],
    one_time_purchase_ids: [],
    uninstalled_at: None,
  )
}

pub fn upsert_base_app_records_test() {
  let s =
    store.upsert_base_app(store.new(), app("gid://shopify/App/1", "h1", "k1"))
  assert store.get_effective_app_by_id(s, "gid://shopify/App/1")
    == Some(app("gid://shopify/App/1", "h1", "k1"))
  assert list.length(store.list_effective_apps(s)) == 1
}

pub fn stage_app_overrides_base_test() {
  let s =
    store.upsert_base_app(store.new(), app("gid://shopify/App/1", "h1", "k1"))
  let updated =
    AppRecord(..app("gid://shopify/App/1", "h1", "k1"), title: Some("renamed"))
  let #(_, s) = store.stage_app(s, updated)
  let assert Some(record) =
    store.get_effective_app_by_id(s, "gid://shopify/App/1")
  assert record.title == Some("renamed")
}

pub fn find_app_by_handle_test() {
  let s =
    store.upsert_base_app(store.new(), app("gid://shopify/App/1", "h1", "k1"))
  let s = store.upsert_base_app(s, app("gid://shopify/App/2", "h2", "k2"))
  let assert Some(record) = store.find_effective_app_by_handle(s, "h2")
  assert record.id == "gid://shopify/App/2"
  assert store.find_effective_app_by_handle(s, "missing") == None
}

pub fn find_app_by_api_key_test() {
  let s =
    store.upsert_base_app(store.new(), app("gid://shopify/App/1", "h1", "k1"))
  let assert Some(record) = store.find_effective_app_by_api_key(s, "k1")
  assert record.id == "gid://shopify/App/1"
  assert store.find_effective_app_by_api_key(s, "kx") == None
}

pub fn upsert_base_installation_seeds_app_and_current_test() {
  let s =
    store.upsert_base_app_installation(
      store.new(),
      installation("gid://shopify/AppInstallation/1", "gid://shopify/App/1"),
      app("gid://shopify/App/1", "h1", "k1"),
    )
  // Both records present
  assert store.get_effective_app_by_id(s, "gid://shopify/App/1") != None
  let assert Some(install) = store.get_current_app_installation(s)
  assert install.id == "gid://shopify/AppInstallation/1"
}

pub fn stage_installation_sets_current_when_unset_test() {
  let #(_, s) =
    store.stage_app_installation(
      store.new(),
      installation("gid://shopify/AppInstallation/1", "gid://shopify/App/1"),
    )
  let assert Some(install) = store.get_current_app_installation(s)
  assert install.id == "gid://shopify/AppInstallation/1"
}

pub fn subscription_lifecycle_test() {
  let sub =
    AppSubscriptionRecord(
      id: "gid://shopify/AppSubscription/1",
      name: "Pro",
      status: "PENDING",
      is_test: True,
      trial_days: Some(14),
      current_period_end: None,
      created_at: "2026-04-29T00:00:00.000Z",
      line_item_ids: [],
    )
  let #(_, s) = store.stage_app_subscription(store.new(), sub)
  assert store.get_effective_app_subscription_by_id(s, sub.id) == Some(sub)
}

pub fn line_item_lifecycle_test() {
  let plan =
    AppSubscriptionLineItemPlan(pricing_details: AppRecurringPricing(
      price: money("10.00", "USD"),
      interval: "EVERY_30_DAYS",
      plan_handle: None,
    ))
  let li =
    AppSubscriptionLineItemRecord(
      id: "gid://shopify/AppSubscriptionLineItem/1",
      subscription_id: "gid://shopify/AppSubscription/1",
      plan: plan,
    )
  let #(_, s) = store.stage_app_subscription_line_item(store.new(), li)
  assert store.get_effective_app_subscription_line_item_by_id(s, li.id)
    == Some(li)
}

pub fn one_time_purchase_lifecycle_test() {
  let p =
    AppOneTimePurchaseRecord(
      id: "gid://shopify/AppPurchaseOneTime/1",
      name: "Boost",
      status: "PENDING",
      is_test: True,
      created_at: "2026-04-29T00:00:00.000Z",
      price: money("5.00", "USD"),
    )
  let #(_, s) = store.stage_app_one_time_purchase(store.new(), p)
  assert store.get_effective_app_one_time_purchase_by_id(s, p.id) == Some(p)
}

pub fn usage_records_filter_by_line_item_test() {
  let r1 =
    AppUsageRecord(
      id: "gid://shopify/AppUsageRecord/1",
      subscription_line_item_id: "gid://shopify/AppSubscriptionLineItem/A",
      description: "u1",
      price: money("1.00", "USD"),
      created_at: "2026-04-29T00:00:00.000Z",
      idempotency_key: None,
    )
  let r2 =
    AppUsageRecord(
      id: "gid://shopify/AppUsageRecord/2",
      subscription_line_item_id: "gid://shopify/AppSubscriptionLineItem/B",
      description: "u2",
      price: money("2.00", "USD"),
      created_at: "2026-04-29T00:00:00.000Z",
      idempotency_key: None,
    )
  let #(_, s) = store.stage_app_usage_record(store.new(), r1)
  let #(_, s) = store.stage_app_usage_record(s, r2)
  let only_a =
    store.list_effective_app_usage_records_for_line_item(
      s,
      "gid://shopify/AppSubscriptionLineItem/A",
    )
  assert list.length(only_a) == 1
  let assert [first] = only_a
  assert first.id == r1.id
}

pub fn delegated_token_lifecycle_test() {
  let token =
    DelegatedAccessTokenRecord(
      id: "gid://shopify/DelegateAccessToken/1",
      api_client_id: "gid://shopify/App/1",
      parent_access_token_sha256: Some("parent"),
      access_token_sha256: "abcd",
      access_token_preview: "[redacted]xxxx",
      access_scopes: ["read_products"],
      created_at: "2026-04-29T00:00:00.000Z",
      expires_in: None,
      destroyed_at: None,
    )
  let #(_, s) = store.stage_delegated_access_token(store.new(), token)
  let assert Some(found) = store.find_delegated_access_token_by_hash(s, "abcd")
  assert found.id == token.id
  // Destroying removes the token from hash lookup so repeat destroy
  // attempts match Shopify's ACCESS_TOKEN_NOT_FOUND branch.
  let s =
    store.destroy_delegated_access_token(
      s,
      token.id,
      "2026-04-30T00:00:00.000Z",
    )
  assert store.find_delegated_access_token_by_hash(s, "abcd") == None
}

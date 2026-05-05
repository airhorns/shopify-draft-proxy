//// Mutation-path tests for `proxy/apps`.
////
//// Covers all 10 apps mutation roots (`appUninstall`,
//// `appRevokeAccessScopes`, `delegateAccessTokenCreate`/`Destroy`,
//// `appPurchaseOneTimeCreate`, `appSubscriptionCreate`/`Cancel`,
//// `appSubscriptionLineItemUpdate`, `appSubscriptionTrialExtend`,
//// `appUsageRecordCreate`) plus the `is_app_mutation_root` predicate
//// and the `process_mutation` `{"data": …}` envelope.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppRecord, type Money, AccessScopeRecord,
  AppInstallationRecord, AppRecord, AppSubscriptionLineItemPlan,
  AppSubscriptionLineItemRecord, AppSubscriptionRecord, AppUsagePricing,
  DelegatedAccessTokenRecord, Money,
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
      AccessScopeRecord(handle: "write_products", description: None),
    ],
  )
}

fn installation(id: String, app_id: String) -> AppInstallationRecord {
  AppInstallationRecord(
    id: id,
    app_id: app_id,
    launch_url: Some("https://launch"),
    uninstall_url: None,
    access_scopes: [
      AccessScopeRecord(handle: "read_products", description: None),
      AccessScopeRecord(handle: "write_products", description: None),
    ],
    active_subscription_ids: [],
    all_subscription_ids: [],
    one_time_purchase_ids: [],
    uninstalled_at: None,
  )
}

fn run_mutation_outcome(
  store_in: store.Store,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let assert Ok(outcome) =
    apps.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      "https://shopify.example",
      document,
      dict.new(),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn seeded_with_installation() -> store.Store {
  let a = app("gid://shopify/App/100", "shopify-draft-proxy", "key-100")
  let i = installation("gid://shopify/AppInstallation/100", a.id)
  store.upsert_base_app_installation(store.new(), i, a)
}

// ----------- is_app_mutation_root -----------

pub fn is_app_mutation_root_test() {
  assert apps.is_app_mutation_root("appUninstall")
  assert apps.is_app_mutation_root("appRevokeAccessScopes")
  assert apps.is_app_mutation_root("delegateAccessTokenCreate")
  assert apps.is_app_mutation_root("delegateAccessTokenDestroy")
  assert apps.is_app_mutation_root("appPurchaseOneTimeCreate")
  assert apps.is_app_mutation_root("appSubscriptionCreate")
  assert apps.is_app_mutation_root("appSubscriptionCancel")
  assert apps.is_app_mutation_root("appSubscriptionLineItemUpdate")
  assert apps.is_app_mutation_root("appSubscriptionTrialExtend")
  assert apps.is_app_mutation_root("appUsageRecordCreate")
  assert !apps.is_app_mutation_root("webhookSubscriptionCreate")
  assert !apps.is_app_mutation_root("app")
}

// ----------- envelope -----------

pub fn process_mutation_returns_data_envelope_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appUninstall { app { id } userErrors { field } } }",
    )
  // Always wraps in `{"data": {...}}` — apps mutations have no top-level errors.
  assert body
    == "{\"data\":{\"appUninstall\":{\"app\":{\"id\":\"gid://shopify/App/100\"},\"userErrors\":[]}}}"
}

// ----------- appUninstall -----------

pub fn app_uninstall_creates_default_when_no_installation_test() {
  // No installation seeded — handler must mint a default app + installation.
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { appUninstall { app { id handle } userErrors { field } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"appUninstall\":{\"app\":{\"id\":\"gid://shopify/App/1\",\"handle\":\"shopify-draft-proxy\"},\"userErrors\":[]}}}"
  // Staged uninstall suppresses the current installation from downstream reads.
  assert store.get_current_app_installation(outcome.store) == None
  let assert Some(_) =
    store.get_effective_app_by_id(outcome.store, "gid://shopify/App/1")
}

pub fn app_uninstall_marks_installation_uninstalled_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appUninstall { app { id } userErrors { field } } }",
    )
  assert store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
    == None
  assert outcome.staged_resource_ids == ["gid://shopify/AppInstallation/100"]
}

// ----------- appRevokeAccessScopes -----------

pub fn revoke_access_scopes_removes_granted_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appRevokeAccessScopes(scopes: [\"read_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[{\"handle\":\"read_products\"}],\"userErrors\":[]}}}"
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert list.map(install.access_scopes, fn(s) { s.handle })
    == ["write_products"]
}

pub fn revoke_access_scopes_unknown_emits_user_error_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appRevokeAccessScopes(scopes: [\"admin_secrets\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"scopes\"],\"message\":\"Access scope 'admin_secrets' is not granted.\",\"code\":\"UNKNOWN_SCOPES\"}]}}}"
}

// ----------- delegateAccessTokenCreate / Destroy -----------

pub fn delegate_token_create_round_trip_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { accessScopes: [\"read_products\"], expiresIn: 3600 }) { delegateAccessToken { accessToken accessScopes createdAt expiresIn } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // Token id is the first synthetic gid (#1). Raw token = "shpat_delegate_proxy_1".
  assert body
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":{\"accessToken\":\"shpat_delegate_proxy_1\",\"accessScopes\":[\"read_products\"],\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"expiresIn\":3600},\"userErrors\":[]}}}"
  // The store must hold a record whose sha256 matches the raw token's hash.
  let assert Some(record) =
    store.find_delegated_access_token_by_hash(
      outcome.store,
      shopify_sha256_of("shpat_delegate_proxy_1"),
    )
  assert record.access_scopes == ["read_products"]
  assert record.destroyed_at == None
}

pub fn delegate_token_destroy_marks_destroyed_test() {
  // Pre-stage a token whose hash we know.
  let raw = "shpat_delegate_proxy_seeded"
  let record =
    DelegatedAccessTokenRecord(
      id: "gid://shopify/DelegateAccessToken/77",
      access_token_sha256: shopify_sha256_of(raw),
      access_token_preview: "[redacted]eded",
      access_scopes: ["read_products"],
      created_at: "2023-12-01T00:00:00Z",
      expires_in: None,
      destroyed_at: None,
    )
  let s = store.new()
  let #(_, s) = store.stage_delegated_access_token(s, record)
  let document =
    "mutation { delegateAccessTokenDestroy(accessToken: \""
    <> raw
    <> "\") { status userErrors { field message code } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenDestroy\":{\"status\":true,\"userErrors\":[]}}}"
  // Destroyed delegated tokens are no longer discoverable by raw token hash.
  assert store.find_delegated_access_token_by_hash(
      outcome.store,
      shopify_sha256_of(raw),
    )
    == None
}

pub fn delegate_token_destroy_unknown_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { delegateAccessTokenDestroy(accessToken: \"shpat_does_not_exist\") { status userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"delegateAccessTokenDestroy\":{\"status\":false,\"userErrors\":[{\"field\":[\"accessToken\"],\"message\":\"Access token not found.\",\"code\":\"ACCESS_TOKEN_NOT_FOUND\"}]}}}"
}

// ----------- appPurchaseOneTimeCreate -----------

pub fn purchase_create_returns_confirmation_url_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appPurchaseOneTimeCreate(name: \"Pro\", price: { amount: \"19.00\", currencyCode: USD }, test: true) { appPurchaseOneTime { id name status price { amount currencyCode } test } confirmationUrl userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // The synthetic gid for the purchase is #1. Confirmation url uses the
  // trailing segment + "ApplicationCharge" + signature.
  assert body
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":{\"id\":\"gid://shopify/AppPurchaseOneTime/1\",\"name\":\"Pro\",\"status\":\"PENDING\",\"price\":{\"amount\":\"19.00\",\"currencyCode\":\"USD\"},\"test\":true},\"confirmationUrl\":\"https://shopify.example/admin/charges/shopify-draft-proxy/1/ApplicationCharge/confirm?signature=shopify-draft-proxy-local-redacted\",\"userErrors\":[]}}}"
  // The installation tracks the new purchase id.
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert install.one_time_purchase_ids == ["gid://shopify/AppPurchaseOneTime/1"]
}

// ----------- appSubscriptionCreate -----------

pub fn subscription_create_with_recurring_line_item_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appSubscriptionCreate(name: \"Pro\", lineItems: [{ plan: { appRecurringPricingDetails: { price: { amount: \"10.00\", currencyCode: USD }, interval: EVERY_30_DAYS } } }]) { appSubscription { id name status lineItems { id plan { pricingDetails { __typename ... on AppRecurringPricing { interval price { amount } } } } } } confirmationUrl userErrors { field message } } }",
    )
  // First synthetic gid → AppSubscription/1; line item base id #2 with
  // ?v=1&index=1 query suffix; trailing_segment strips that for the URL.
  assert body
    == "{\"data\":{\"appSubscriptionCreate\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/1\",\"name\":\"Pro\",\"status\":\"PENDING\",\"lineItems\":[{\"id\":\"gid://shopify/AppSubscriptionLineItem/2?v=1&index=1\",\"plan\":{\"pricingDetails\":{\"__typename\":\"AppRecurringPricing\",\"interval\":\"EVERY_30_DAYS\",\"price\":{\"amount\":\"10.00\"}}}}]},\"confirmationUrl\":\"https://shopify.example/admin/charges/shopify-draft-proxy/1/RecurringApplicationCharge/confirm?signature=shopify-draft-proxy-local-redacted\",\"userErrors\":[]}}}"
}

pub fn subscription_create_with_usage_line_item_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appSubscriptionCreate(name: \"Usage\", lineItems: [{ plan: { appUsagePricingDetails: { cappedAmount: { amount: \"100.00\", currencyCode: USD }, interval: ANNUAL, terms: \"per ticket\" } } }]) { appSubscription { id status } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"appSubscriptionCreate\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/1\",\"status\":\"PENDING\"},\"userErrors\":[]}}}"
  // Line item is staged with usage pricing.
  let assert Some(li) =
    store.get_effective_app_subscription_line_item_by_id(
      outcome.store,
      "gid://shopify/AppSubscriptionLineItem/2?v=1&index=1",
    )
  case li.plan.pricing_details {
    AppUsagePricing(capped_amount: capped, interval: interval, ..) -> {
      assert capped.amount == "100.00"
      assert interval == "ANNUAL"
    }
    _ -> panic as "expected usage pricing"
  }
}

// ----------- appSubscriptionCancel -----------

pub fn subscription_cancel_flips_status_test() {
  let s = seeded_with_installation()
  let sub =
    AppSubscriptionRecord(
      id: "gid://shopify/AppSubscription/9",
      name: "Pro",
      status: "ACTIVE",
      is_test: False,
      trial_days: None,
      current_period_end: None,
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [],
    )
  let #(_, s) = store.stage_app_subscription(s, sub)
  let body =
    run_mutation(
      s,
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/9\",\"status\":\"CANCELLED\"},\"userErrors\":[]}}}"
}

pub fn subscription_cancel_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/missing\") { appSubscription { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Subscription not found\",\"code\":null}]}}}"
}

// ----------- appSubscriptionLineItemUpdate -----------

pub fn line_item_update_caps_usage_amount_test() {
  let s = seeded_with_installation()
  let sub_id = "gid://shopify/AppSubscription/30"
  let li_id = "gid://shopify/AppSubscriptionLineItem/30?v=1&index=1"
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(pricing_details: AppUsagePricing(
        capped_amount: money("50.00", "USD"),
        balance_used: money("0.00", "USD"),
        interval: "ANNUAL",
        terms: Some("per row"),
      )),
    )
  let sub =
    AppSubscriptionRecord(
      id: sub_id,
      name: "Usage",
      status: "ACTIVE",
      is_test: False,
      trial_days: None,
      current_period_end: None,
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [li_id],
    )
  let #(_, s) = store.stage_app_subscription_line_item(s, li)
  let #(_, s) = store.stage_app_subscription(s, sub)
  let document =
    "mutation { appSubscriptionLineItemUpdate(id: \""
    <> li_id
    <> "\", cappedAmount: { amount: \"200.00\", currencyCode: USD }) { appSubscription { id } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  let assert Some(updated) =
    store.get_effective_app_subscription_line_item_by_id(outcome.store, li_id)
  case updated.plan.pricing_details {
    AppUsagePricing(capped_amount: c, ..) -> {
      assert c.amount == "200.00"
    }
    _ -> panic as "expected usage pricing"
  }
}

pub fn line_item_update_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appSubscriptionLineItemUpdate(id: \"gid://shopify/AppSubscriptionLineItem/missing\", cappedAmount: { amount: \"5.00\", currencyCode: USD }) { appSubscription { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Subscription line item not found\",\"code\":null}]}}}"
}

// ----------- appSubscriptionTrialExtend -----------

pub fn trial_extend_adds_days_test() {
  let s = seeded_with_installation()
  let sub =
    AppSubscriptionRecord(
      id: "gid://shopify/AppSubscription/40",
      name: "Pro",
      status: "ACTIVE",
      is_test: False,
      trial_days: Some(7),
      current_period_end: None,
      created_at: "2024-12-01T00:00:00Z",
      line_item_ids: [],
    )
  let #(_, s) = store.stage_app_subscription(s, sub)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/40\", days: 14) { appSubscription { id trialDays } userErrors { field message } } }",
    )
  let assert Some(updated) =
    store.get_effective_app_subscription_by_id(
      outcome.store,
      "gid://shopify/AppSubscription/40",
    )
  assert updated.trial_days == Some(21)
}

// ----------- appUsageRecordCreate -----------

pub fn usage_record_create_attaches_to_line_item_test() {
  let s = seeded_with_installation()
  let sub_id = "gid://shopify/AppSubscription/50"
  let li_id = "gid://shopify/AppSubscriptionLineItem/50?v=1&index=1"
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(pricing_details: AppUsagePricing(
        capped_amount: money("100.00", "USD"),
        balance_used: money("0.00", "USD"),
        interval: "ANNUAL",
        terms: None,
      )),
    )
  let #(_, s) = store.stage_app_subscription_line_item(s, li)
  let document =
    "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
    <> li_id
    <> "\", description: \"API call\", price: { amount: \"3.00\", currencyCode: USD }) { appUsageRecord { id description price { amount } } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  let body = json.to_string(outcome.data)
  // The synthetic gid for the usage record is #1.
  assert body
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":{\"id\":\"gid://shopify/AppUsageRecord/1\",\"description\":\"API call\",\"price\":{\"amount\":\"3.00\"}},\"userErrors\":[]}}}"
  let assert Some(_) =
    store.get_effective_app_usage_record_by_id(
      outcome.store,
      "gid://shopify/AppUsageRecord/1",
    )
}

pub fn usage_record_create_unknown_line_item_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appUsageRecordCreate(subscriptionLineItemId: \"gid://shopify/AppSubscriptionLineItem/missing\", description: \"x\", price: { amount: \"1.00\", currencyCode: USD }) { appUsageRecord { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":null,\"userErrors\":[{\"field\":[\"subscriptionLineItemId\"],\"message\":\"Subscription line item not found\",\"code\":null}]}}}"
}

// Bridge into the package's sha256 helper so tests can construct the same
// hash the handler stores.
@external(erlang, "crypto_ffi", "sha256_hex")
@external(javascript, "../../shopify_draft_proxy/crypto_ffi.js", "sha256_hex")
fn shopify_sha256_of(input: String) -> String

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
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type AccessScopeRecord, type AppInstallationRecord, type AppRecord,
  type AppSubscriptionLineItemPlan, type AppSubscriptionPricing,
  type AppSubscriptionRecord, type Money, AccessScopeRecord,
  AppInstallationRecord, AppRecord, AppRecurringPricing,
  AppSubscriptionLineItemPlan, AppSubscriptionLineItemRecord,
  AppSubscriptionRecord, AppUsagePricing, DelegatedAccessTokenRecord, Money,
}

// ----------- Helpers -----------

fn money(amount: String, code: String) -> Money {
  Money(amount: amount, currency_code: code)
}

fn scope(handle: String) -> AccessScopeRecord {
  AccessScopeRecord(handle: handle, description: None)
}

fn app(id: String, handle: String, api_key: String) -> AppRecord {
  app_with_scopes(id, handle, api_key, [scope("read_products")])
}

fn app_with_scopes(
  id: String,
  handle: String,
  api_key: String,
  requested_access_scopes: List(AccessScopeRecord),
) -> AppRecord {
  AppRecord(
    id: id,
    api_key: Some(api_key),
    handle: Some(handle),
    title: Some(handle),
    developer_name: Some("test-dev"),
    embedded: Some(True),
    previously_installed: Some(False),
    requested_access_scopes: requested_access_scopes,
  )
}

fn installation(id: String, app_id: String) -> AppInstallationRecord {
  installation_with_scopes(id, app_id, [
    scope("read_products"),
    scope("write_products"),
  ])
}

fn installation_with_scopes(
  id: String,
  app_id: String,
  access_scopes: List(AccessScopeRecord),
) -> AppInstallationRecord {
  AppInstallationRecord(
    id: id,
    app_id: app_id,
    launch_url: Some("https://launch"),
    uninstall_url: None,
    access_scopes: access_scopes,
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
  run_mutation_outcome_with_headers(store_in, document, dict.new())
}

fn run_mutation_outcome_with_headers(
  store_in: store.Store,
  document: String,
  headers: dict.Dict(String, String),
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let outcome =
    apps.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
      upstream_query.UpstreamContext(
        transport: None,
        origin: "https://shopify.example",
        headers: headers,
      ),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn run_query(store_in: store.Store, query: String) -> String {
  let assert Ok(data) = apps.handle_app_query(store_in, query, dict.new())
  json.to_string(data)
}

fn seeded_with_installation() -> store.Store {
  let a = app("gid://shopify/App/100", "shopify-draft-proxy", "key-100")
  let i = installation("gid://shopify/AppInstallation/100", a.id)
  store.upsert_base_app_installation(store.new(), i, a)
}

fn subscription(id: String, status: String) -> AppSubscriptionRecord {
  AppSubscriptionRecord(
    id: id,
    name: "Pro",
    status: status,
    is_test: False,
    trial_days: None,
    current_period_end: None,
    created_at: "2024-12-01T00:00:00Z",
    line_item_ids: [],
  )
}

fn subscription_with_trial(
  id: String,
  status: String,
  trial_days: Option(Int),
  current_period_end: Option(String),
) -> AppSubscriptionRecord {
  AppSubscriptionRecord(
    id: id,
    name: "Pro",
    status: status,
    is_test: False,
    trial_days: trial_days,
    current_period_end: current_period_end,
    created_at: "2024-12-01T00:00:00Z",
    line_item_ids: [],
  )
}

fn seeded_with_subscription(status: String) -> store.Store {
  let s = seeded_with_installation()
  let sub = subscription("gid://shopify/AppSubscription/9", status)
  let #(_, s) = store.stage_app_subscription(s, sub)
  s
}

fn seeded_with_line_item(
  sub_id: String,
  li_id: String,
  pricing: AppSubscriptionPricing,
) -> store.Store {
  let s = seeded_with_installation()
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: AppSubscriptionLineItemPlan(pricing_details: pricing),
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
  s
}

fn seeded_billing_line_item(
  pricing: AppSubscriptionLineItemPlan,
) -> #(store.Store, String) {
  let app_record =
    app("gid://shopify/App/200", "shopify-draft-proxy", "key-200")
  let sub_id = "gid://shopify/AppSubscription/200"
  let li_id = "gid://shopify/AppSubscriptionLineItem/200?v=1&index=1"
  let installation_record =
    AppInstallationRecord(
      ..installation("gid://shopify/AppInstallation/200", app_record.id),
      active_subscription_ids: [sub_id],
      all_subscription_ids: [sub_id],
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      installation_record,
      app_record,
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
  let li =
    AppSubscriptionLineItemRecord(
      id: li_id,
      subscription_id: sub_id,
      plan: pricing,
    )
  let #(_, s) = store.stage_app_subscription_line_item(s, li)
  let #(_, s) = store.stage_app_subscription(s, sub)
  #(s, li_id)
}

fn usage_line_item_plan(
  capped_amount: String,
  balance_used: String,
  currency_code: String,
) -> AppSubscriptionLineItemPlan {
  AppSubscriptionLineItemPlan(pricing_details: AppUsagePricing(
    capped_amount: money(capped_amount, currency_code),
    balance_used: money(balance_used, currency_code),
    interval: "ANNUAL",
    terms: None,
  ))
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
      "mutation { appRevokeAccessScopes(scopes: [\"write_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[{\"handle\":\"write_products\"}],\"userErrors\":[]}}}"
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert list.map(install.access_scopes, fn(s) { s.handle })
    == ["read_products"]
}

pub fn revoke_access_scopes_unknown_app_id_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appRevokeAccessScopes(scopes: [\"unicorn_dust\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"scopes\"],\"message\":\"The requested list of scopes to revoke includes invalid handles.\",\"code\":\"UNKNOWN_SCOPES\"}]}}}"
}

pub fn revoke_access_scopes_not_granted_known_scope_is_undeclared_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appRevokeAccessScopes(scopes: [\"read_orders\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"scopes\"],\"message\":\"Scopes that are not declared cannot be revoked.\",\"code\":\"CANNOT_REVOKE_UNDECLARED_SCOPES\"}]}}}"
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert list.map(install.access_scopes, fn(s) { s.handle })
    == ["read_products", "write_products"]
  let assert [
    mutation_helpers.LogDraft(
      status: store.Failed,
      staged_resource_ids: ["gid://shopify/AppInstallation/100"],
      ..,
    ),
  ] = outcome.log_drafts
}

pub fn revoke_access_scopes_required_scope_is_not_revoked_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appRevokeAccessScopes(scopes: [\"read_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"scopes\"],\"message\":\"Scopes that are declared as required cannot be revoked.\",\"code\":\"CANNOT_REVOKE_REQUIRED_SCOPES\"}]}}}"
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert list.map(install.access_scopes, fn(s) { s.handle })
    == ["read_products", "write_products"]
}

pub fn revoke_access_scopes_implied_scope_is_not_revoked_test() {
  let a =
    app_with_scopes("gid://shopify/App/101", "shopify-draft-proxy", "key-101", [
      scope("write_products"),
    ])
  let i =
    installation_with_scopes("gid://shopify/AppInstallation/101", a.id, [
      scope("write_products"),
      scope("read_products"),
    ])
  let outcome =
    run_mutation_outcome(
      store.upsert_base_app_installation(store.new(), i, a),
      "mutation { appRevokeAccessScopes(scopes: [\"read_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"scopes\"],\"message\":\"Scopes that are implied by other granted scopes cannot be revoked.\",\"code\":\"CANNOT_REVOKE_IMPLIED_SCOPES\"}]}}}"
}

pub fn revoke_access_scopes_missing_source_app_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appRevokeAccessScopes(scopes: [\"write_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"base\"],\"message\":\"Source app is missing.\",\"code\":\"MISSING_SOURCE_APP\"}]}}}"
}

pub fn revoke_access_scopes_application_cannot_be_found_test() {
  let i =
    installation(
      "gid://shopify/AppInstallation/102",
      "gid://shopify/App/missing",
    )
  let #(_, s) = store.stage_app_installation(store.new(), i)
  let body =
    run_mutation(
      s,
      "mutation { appRevokeAccessScopes(scopes: [\"write_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"base\"],\"message\":\"Application cannot be found.\",\"code\":\"APPLICATION_CANNOT_BE_FOUND\"}]}}}"
}

pub fn revoke_access_scopes_app_not_installed_test() {
  let a = app("gid://shopify/App/103", "shopify-draft-proxy", "key-103")
  let #(_, s) = store.stage_app(store.new(), a)
  let body =
    run_mutation(
      s,
      "mutation { appRevokeAccessScopes(scopes: [\"write_products\"]) { revoked { handle } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appRevokeAccessScopes\":{\"revoked\":[],\"userErrors\":[{\"field\":[\"base\"],\"message\":\"App is not installed on this shop.\",\"code\":\"APP_NOT_INSTALLED\"}]}}}"
}

// ----------- delegateAccessTokenCreate / Destroy -----------

pub fn delegate_token_create_round_trip_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [\"read_products\"], expiresIn: 3600 }) { delegateAccessToken { accessToken accessScopes createdAt expiresIn } shop { id name } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  // Token id is the first synthetic gid (#1). Raw token = "shpat_delegate_proxy_1".
  assert body
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":{\"accessToken\":\"shpat_delegate_proxy_1\",\"accessScopes\":[\"read_products\"],\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"expiresIn\":3600},\"shop\":{\"id\":\"gid://shopify/Shop/1?shopify-draft-proxy=synthetic\",\"name\":\"Shopify Draft Proxy\"},\"userErrors\":[]}}}"
  // The store must hold a record whose sha256 matches the raw token's hash.
  let assert Some(record) =
    store.find_delegated_access_token_by_hash(
      outcome.store,
      shopify_sha256_of("shpat_delegate_proxy_1"),
    )
  assert record.access_scopes == ["read_products"]
  assert record.destroyed_at == None
}

pub fn delegate_token_create_legacy_access_scopes_fallback_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { accessScopes: [\"read_products\"], expiresIn: 3600 }) { delegateAccessToken { accessToken accessScopes expiresIn } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":{\"accessToken\":\"shpat_delegate_proxy_1\",\"accessScopes\":[\"read_products\"],\"expiresIn\":3600},\"userErrors\":[]}}}"
}

pub fn delegate_token_create_delegate_scope_presence_blocks_legacy_fallback_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [], accessScopes: [\"read_products\"] }) { delegateAccessToken { accessScopes } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":null,\"userErrors\":[{\"field\":null,\"message\":\"The access scope can't be empty.\",\"code\":\"EMPTY_ACCESS_SCOPE\"}]}}}"
  assert dict.size(outcome.store.staged_state.delegated_access_tokens) == 0
}

pub fn delegate_token_create_rejects_empty_scope_list_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [] }) { delegateAccessToken { accessToken accessScopes } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":null,\"userErrors\":[{\"field\":null,\"message\":\"The access scope can't be empty.\",\"code\":\"EMPTY_ACCESS_SCOPE\"}]}}}"
  assert dict.size(outcome.store.staged_state.delegated_access_tokens) == 0
  let assert [
    mutation_helpers.LogDraft(status: store.Failed, staged_resource_ids: [], ..),
  ] = outcome.log_drafts
}

pub fn delegate_token_create_rejects_non_positive_expires_in_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [\"read_products\"], expiresIn: -1 }) { delegateAccessToken { accessToken accessScopes expiresIn } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":null,\"userErrors\":[{\"field\":null,\"message\":\"The expires_in value must be greater than 0.\",\"code\":\"NEGATIVE_EXPIRES_IN\"}]}}}"
  assert dict.size(outcome.store.staged_state.delegated_access_tokens) == 0
}

pub fn delegate_token_create_rejects_unknown_scope_handles_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [\"fake_scope\", \"another_fake_scope\"] }) { delegateAccessToken { accessScopes } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":null,\"userErrors\":[{\"field\":null,\"message\":\"The access scope is invalid: fake_scope, another_fake_scope\",\"code\":\"UNKNOWN_SCOPES\"}]}}}"
  assert dict.size(outcome.store.staged_state.delegated_access_tokens) == 0
}

pub fn delegate_token_create_rejects_active_delegate_parent_test() {
  let raw = "shpat_delegate_parent"
  let record =
    DelegatedAccessTokenRecord(
      id: "gid://shopify/DelegateAccessToken/parent",
      access_token_sha256: shopify_sha256_of(raw),
      access_token_preview: "[redacted]rent",
      access_scopes: ["read_products"],
      created_at: "2024-01-01T00:00:00.000Z",
      expires_in: Some(3600),
      destroyed_at: None,
    )
  let #(_, s) = store.stage_delegated_access_token(store.new(), record)
  let outcome =
    run_mutation_outcome_with_headers(
      s,
      "mutation { delegateAccessTokenCreate(input: { delegateAccessScope: [\"read_products\"] }) { delegateAccessToken { accessScopes } userErrors { field message code } } }",
      dict.from_list([#("X-Shopify-Access-Token", raw)]),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"delegateAccessTokenCreate\":{\"delegateAccessToken\":null,\"userErrors\":[{\"field\":null,\"message\":\"The parent access token can't be a delegate token.\",\"code\":\"DELEGATE_ACCESS_TOKEN\"}]}}}"
  assert dict.size(outcome.store.staged_state.delegated_access_tokens) == 1
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
      "mutation { appPurchaseOneTimeCreate(name: \"Pro\", returnUrl: \"https://app.example.test/return\", price: { amount: \"19.00\", currencyCode: USD }, test: true) { appPurchaseOneTime { id name status createdAt price { amount currencyCode } test } confirmationUrl userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // The synthetic gid for the purchase is #1. Confirmation url uses the
  // trailing segment + "ApplicationCharge" + signature.
  assert body
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":{\"id\":\"gid://shopify/AppPurchaseOneTime/1\",\"name\":\"Pro\",\"status\":\"ACTIVE\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"price\":{\"amount\":\"19.00\",\"currencyCode\":\"USD\"},\"test\":true},\"confirmationUrl\":\"https://shopify.example/admin/charges/shopify-draft-proxy/1/ApplicationCharge/confirm?signature=shopify-draft-proxy-local-redacted\",\"userErrors\":[]}}}"
  let assert Some(purchase) =
    store.get_effective_app_one_time_purchase_by_id(
      outcome.store,
      "gid://shopify/AppPurchaseOneTime/1",
    )
  assert purchase.status == "ACTIVE"
  // The installation tracks the new purchase id.
  let assert Some(install) =
    store.get_effective_app_installation_by_id(
      outcome.store,
      "gid://shopify/AppInstallation/100",
    )
  assert install.one_time_purchase_ids == ["gid://shopify/AppPurchaseOneTime/1"]
}

pub fn purchase_create_requires_return_url_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appPurchaseOneTimeCreate(name: \"Pro\", price: { amount: \"19.00\", currencyCode: USD }, test: true) { appPurchaseOneTime { id } confirmationUrl userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":null,\"confirmationUrl\":null,\"userErrors\":[{\"field\":[\"returnUrl\"],\"message\":\"Return URL is required.\",\"code\":null}]}}}"
  assert dict.size(outcome.store.staged_state.app_one_time_purchases) == 0
}

pub fn purchase_create_rejects_blank_return_url_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appPurchaseOneTimeCreate(name: \"Pro\", returnUrl: \"   \", price: { amount: \"19.00\", currencyCode: USD }, test: true) { appPurchaseOneTime { id } confirmationUrl userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":null,\"confirmationUrl\":null,\"userErrors\":[{\"field\":[\"returnUrl\"],\"message\":\"Return URL must be a valid URL.\",\"code\":null}]}}}"
}

pub fn purchase_create_rejects_blank_name_and_low_price_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appPurchaseOneTimeCreate(name: \"  \", returnUrl: \"https://app.example.test/return\", price: { amount: \"0.49\", currencyCode: USD }, test: true) { appPurchaseOneTime { id } confirmationUrl userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":null,\"confirmationUrl\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name can't be blank\",\"code\":null},{\"field\":[\"price\"],\"message\":\"Price must be at least 0.50 USD.\",\"code\":\"PRICE_TOO_LOW\"}]}}}"
  assert dict.size(outcome.store.staged_state.app_one_time_purchases) == 0
}

pub fn purchase_create_rejects_currency_mismatch_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appPurchaseOneTimeCreate(name: \"Pro\", returnUrl: \"https://app.example.test/return\", price: { amount: \"5.00\", currencyCode: EUR }, test: true) { appPurchaseOneTime { id } confirmationUrl userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appPurchaseOneTimeCreate\":{\"appPurchaseOneTime\":null,\"confirmationUrl\":null,\"userErrors\":[{\"field\":[\"price\"],\"message\":\"Price currency must match shop billing currency USD.\",\"code\":null}]}}}"
}

// ----------- appSubscriptionCreate -----------

pub fn subscription_create_with_recurring_line_item_test() {
  let outcome =
    run_mutation_outcome(
      seeded_with_installation(),
      "mutation { appSubscriptionCreate(name: \"Pro\", lineItems: [{ plan: { appRecurringPricingDetails: { price: { amount: \"10.00\", currencyCode: USD }, interval: EVERY_30_DAYS } } }], test: true) { appSubscription { id name status currentPeriodEnd lineItems { id plan { pricingDetails { __typename ... on AppRecurringPricing { interval price { amount } } } } } } confirmationUrl userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // First synthetic gid → AppSubscription/1; line item base id #2 with
  // ?v=1&index=1 query suffix; trailing_segment strips that for the URL.
  assert body
    == "{\"data\":{\"appSubscriptionCreate\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/1\",\"name\":\"Pro\",\"status\":\"ACTIVE\",\"currentPeriodEnd\":\"2024-01-31T00:00:00.000Z\",\"lineItems\":[{\"id\":\"gid://shopify/AppSubscriptionLineItem/2?v=1&index=1\",\"plan\":{\"pricingDetails\":{\"__typename\":\"AppRecurringPricing\",\"interval\":\"EVERY_30_DAYS\",\"price\":{\"amount\":\"10.00\"}}}}]},\"confirmationUrl\":\"https://shopify.example/admin/charges/shopify-draft-proxy/1/RecurringApplicationCharge/confirm?signature=shopify-draft-proxy-local-redacted\",\"userErrors\":[]}}}"
  let assert Some(subscription) =
    store.get_effective_app_subscription_by_id(
      outcome.store,
      "gid://shopify/AppSubscription/1",
    )
  assert subscription.status == "ACTIVE"
  assert subscription.current_period_end == Some("2024-01-31T00:00:00.000Z")
  let assert Some(install) = store.get_current_app_installation(outcome.store)
  assert install.active_subscription_ids == ["gid://shopify/AppSubscription/1"]
  let assert Ok(readback) =
    apps.process(
      outcome.store,
      "{ currentAppInstallation { activeSubscriptions { id status currentPeriodEnd } } }",
      dict.new(),
    )
  assert json.to_string(readback)
    == "{\"data\":{\"currentAppInstallation\":{\"activeSubscriptions\":[{\"id\":\"gid://shopify/AppSubscription/1\",\"status\":\"ACTIVE\",\"currentPeriodEnd\":\"2024-01-31T00:00:00.000Z\"}]}}}"
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
  let sub = subscription("gid://shopify/AppSubscription/9", "ACTIVE")
  let #(_, s) = store.stage_app_subscription(s, sub)
  let assert Some(install) = store.get_current_app_installation(s)
  let #(_, s) =
    store.stage_app_installation(
      s,
      AppInstallationRecord(..install, active_subscription_ids: [sub.id]),
    )
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/9\",\"status\":\"CANCELLED\"},\"userErrors\":[]}}}"
  let assert Some(updated_install) =
    store.get_current_app_installation(outcome.store)
  assert updated_install.active_subscription_ids == []
}

pub fn subscription_cancel_accepts_pending_and_accepted_test() {
  let pending_body =
    run_mutation(
      seeded_with_subscription("PENDING"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert pending_body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/9\",\"status\":\"CANCELLED\"},\"userErrors\":[]}}}"

  let accepted_body =
    run_mutation(
      seeded_with_subscription("ACCEPTED"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert accepted_body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/9\",\"status\":\"CANCELLED\"},\"userErrors\":[]}}}"
}

pub fn subscription_cancel_rejects_repeat_cancel_test() {
  let first =
    run_mutation_outcome(
      seeded_with_subscription("ACTIVE"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  let second =
    run_mutation(
      first.store,
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert second
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot transition status via :cancel from :cancelled\"}]}}}"
}

pub fn subscription_cancel_rejects_non_cancellable_statuses_test() {
  let expired_body =
    run_mutation(
      seeded_with_subscription("EXPIRED"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert expired_body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot transition status via :cancel from :expired\"}]}}}"

  let declined_body =
    run_mutation(
      seeded_with_subscription("DECLINED"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert declined_body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot transition status via :cancel from :declined\"}]}}}"

  let frozen_body =
    run_mutation(
      seeded_with_subscription("FROZEN"),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/9\") { appSubscription { id status } userErrors { field message } } }",
    )
  assert frozen_body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot transition status via :cancel from :frozen\"}]}}}"
}

pub fn subscription_cancel_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appSubscriptionCancel(id: \"gid://shopify/AppSubscription/missing\") { appSubscription { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionCancel\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Couldn't find RecurringApplicationCharge\",\"code\":null}]}}}"
}

// ----------- appSubscriptionLineItemUpdate -----------

pub fn line_item_update_caps_usage_amount_test() {
  let sub_id = "gid://shopify/AppSubscription/30"
  let li_id = "gid://shopify/AppSubscriptionLineItem/30?v=1&index=1"
  let s =
    seeded_with_line_item(
      sub_id,
      li_id,
      AppUsagePricing(
        capped_amount: money("50.00", "USD"),
        balance_used: money("0.00", "USD"),
        interval: "ANNUAL",
        terms: Some("per row"),
      ),
    )
  let document =
    "mutation { appSubscriptionLineItemUpdate(id: \""
    <> li_id
    <> "\", cappedAmount: { amount: \"200.00\", currencyCode: USD }) { confirmationUrl appSubscription { id lineItems { id plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } } } } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"confirmationUrl\":\"https://shopify.example/admin/charges/shopify-draft-proxy/30/RecurringApplicationCharge/confirm?signature=shopify-draft-proxy-local-redacted\",\"appSubscription\":{\"id\":\"gid://shopify/AppSubscription/30\",\"lineItems\":[{\"id\":\"gid://shopify/AppSubscriptionLineItem/30?v=1&index=1\",\"plan\":{\"pricingDetails\":{\"__typename\":\"AppUsagePricing\",\"cappedAmount\":{\"amount\":\"200.00\",\"currencyCode\":\"USD\"}}}}]},\"userErrors\":[]}}}"
  let assert Some(updated) =
    store.get_effective_app_subscription_line_item_by_id(outcome.store, li_id)
  case updated.plan.pricing_details {
    AppUsagePricing(capped_amount: c, ..) -> {
      assert c.amount == "200.00"
    }
    _ -> panic as "expected usage pricing"
  }
}

pub fn line_item_update_malformed_gid_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { appSubscriptionLineItemUpdate(id: \"not-a-gid\", cappedAmount: { amount: \"5.00\", currencyCode: USD }) { appSubscription { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Invalid app subscription line item id\",\"code\":null}]}}}"
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

pub fn line_item_update_recurring_line_item_emits_user_error_test() {
  let sub_id = "gid://shopify/AppSubscription/31"
  let li_id = "gid://shopify/AppSubscriptionLineItem/31?v=1&index=1"
  let s =
    seeded_with_line_item(
      sub_id,
      li_id,
      AppRecurringPricing(
        price: money("10.00", "USD"),
        interval: "EVERY_30_DAYS",
        plan_handle: None,
      ),
    )
  let document =
    "mutation { appSubscriptionLineItemUpdate(id: \""
    <> li_id
    <> "\", cappedAmount: { amount: \"20.00\", currencyCode: USD }) { appSubscription { id } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"cappedAmount\"],\"message\":\"Only usage-pricing line items support cappedAmount updates\"}]}}}"
  let assert Some(unchanged) =
    store.get_effective_app_subscription_line_item_by_id(outcome.store, li_id)
  case unchanged.plan.pricing_details {
    AppRecurringPricing(..) -> Nil
    _ -> panic as "expected recurring pricing to remain unchanged"
  }
}

pub fn line_item_update_currency_mismatch_emits_user_error_test() {
  let sub_id = "gid://shopify/AppSubscription/32"
  let li_id = "gid://shopify/AppSubscriptionLineItem/32?v=1&index=1"
  let s =
    seeded_with_line_item(
      sub_id,
      li_id,
      AppUsagePricing(
        capped_amount: money("50.00", "USD"),
        balance_used: money("0.00", "USD"),
        interval: "ANNUAL",
        terms: Some("per row"),
      ),
    )
  let document =
    "mutation { appSubscriptionLineItemUpdate(id: \""
    <> li_id
    <> "\", cappedAmount: { amount: \"100.00\", currencyCode: EUR }) { appSubscription { id } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"cappedAmount\"],\"message\":\"Capped amount currency mismatch. Expected USD\"}]}}}"
  let assert Some(unchanged) =
    store.get_effective_app_subscription_line_item_by_id(outcome.store, li_id)
  case unchanged.plan.pricing_details {
    AppUsagePricing(capped_amount: capped, ..) -> {
      assert capped.amount == "50.00"
      assert capped.currency_code == "USD"
    }
    _ -> panic as "expected usage pricing"
  }
}

pub fn line_item_update_non_increasing_amount_emits_user_error_test() {
  let sub_id = "gid://shopify/AppSubscription/33"
  let li_id = "gid://shopify/AppSubscriptionLineItem/33?v=1&index=1"
  let s =
    seeded_with_line_item(
      sub_id,
      li_id,
      AppUsagePricing(
        capped_amount: money("50.00", "USD"),
        balance_used: money("0.00", "USD"),
        interval: "ANNUAL",
        terms: Some("per row"),
      ),
    )
  let document =
    "mutation { appSubscriptionLineItemUpdate(id: \""
    <> li_id
    <> "\", cappedAmount: { amount: \"3.00\", currencyCode: USD }) { appSubscription { id } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionLineItemUpdate\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"cappedAmount\"],\"message\":\"The capped amount must be greater than the existing capped amount\"}]}}}"
  let assert Some(unchanged) =
    store.get_effective_app_subscription_line_item_by_id(outcome.store, li_id)
  case unchanged.plan.pricing_details {
    AppUsagePricing(capped_amount: capped, ..) -> {
      assert capped.amount == "50.00"
      assert capped.currency_code == "USD"
    }
    _ -> panic as "expected usage pricing"
  }
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
      current_period_end: Some("2099-01-01T00:00:00Z"),
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

pub fn trial_extend_rejects_days_outside_shopify_range_test() {
  let sub_id = "gid://shopify/AppSubscription/range"
  let sub =
    subscription_with_trial(
      sub_id,
      "ACTIVE",
      Some(7),
      Some("2099-01-01T00:00:00Z"),
    )
  let #(_, s) = store.stage_app_subscription(seeded_with_installation(), sub)

  let zero_outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/range\", days: 0) { appSubscription { id trialDays } userErrors { field message code } } }",
    )
  assert json.to_string(zero_outcome.data)
    == "{\"data\":{\"appSubscriptionTrialExtend\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"days\"],\"message\":\"Days must be greater than 0\",\"code\":null}]}}}"
  let assert Some(after_zero) =
    store.get_effective_app_subscription_by_id(zero_outcome.store, sub_id)
  assert after_zero.trial_days == Some(7)

  let too_large_outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/range\", days: 1001) { appSubscription { id trialDays } userErrors { field message code } } }",
    )
  assert json.to_string(too_large_outcome.data)
    == "{\"data\":{\"appSubscriptionTrialExtend\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"days\"],\"message\":\"Days must be less than or equal to 1000\",\"code\":null}]}}}"
  let assert Some(after_too_large) =
    store.get_effective_app_subscription_by_id(too_large_outcome.store, sub_id)
  assert after_too_large.trial_days == Some(7)
}

pub fn trial_extend_unknown_id_sets_subscription_not_found_code_test() {
  let body =
    run_mutation(
      seeded_with_installation(),
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/missing\", days: 5) { appSubscription { id trialDays } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appSubscriptionTrialExtend\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The app subscription wasn't found.\",\"code\":\"SUBSCRIPTION_NOT_FOUND\"}]}}}"
}

pub fn trial_extend_rejects_inactive_subscription_without_mutating_test() {
  let sub_id = "gid://shopify/AppSubscription/pending"
  let sub =
    subscription_with_trial(
      sub_id,
      "PENDING",
      Some(7),
      Some("2099-01-01T00:00:00Z"),
    )
  let #(_, s) = store.stage_app_subscription(seeded_with_installation(), sub)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/pending\", days: 5) { appSubscription { id trialDays } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionTrialExtend\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The trial can't be extended on inactive app subscriptions.\",\"code\":\"SUBSCRIPTION_NOT_ACTIVE\"}]}}}"
  let assert Some(updated) =
    store.get_effective_app_subscription_by_id(outcome.store, sub_id)
  assert updated.trial_days == Some(7)
}

pub fn trial_extend_rejects_expired_active_trial_without_mutating_test() {
  let sub_id = "gid://shopify/AppSubscription/expired"
  let sub =
    subscription_with_trial(
      sub_id,
      "ACTIVE",
      Some(7),
      Some("2024-01-01T00:00:00Z"),
    )
  let #(_, s) = store.stage_app_subscription(seeded_with_installation(), sub)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { appSubscriptionTrialExtend(id: \"gid://shopify/AppSubscription/expired\", days: 5) { appSubscription { id trialDays } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"appSubscriptionTrialExtend\":{\"appSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The trial can't be extended after expiration.\",\"code\":\"TRIAL_NOT_ACTIVE\"}]}}}"
  let assert Some(updated) =
    store.get_effective_app_subscription_by_id(outcome.store, sub_id)
  assert updated.trial_days == Some(7)
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

pub fn usage_record_create_caps_balance_and_reuses_idempotency_key_test() {
  let #(s, li_id) =
    seeded_billing_line_item(usage_line_item_plan("5.00", "0.00", "USD"))
  let first_document =
    "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
    <> li_id
    <> "\", description: \"first\", price: { amount: \"3.00\", currencyCode: USD }, idempotencyKey: \"usage-key-1\") { appUsageRecord { id description price { amount currencyCode } subscriptionLineItem { id plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } } } userErrors { field message } } }"
  let first = run_mutation_outcome(s, first_document)
  assert json.to_string(first.data)
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":{\"id\":\"gid://shopify/AppUsageRecord/1\",\"description\":\"first\",\"price\":{\"amount\":\"3.00\",\"currencyCode\":\"USD\"},\"subscriptionLineItem\":{\"id\":\"gid://shopify/AppSubscriptionLineItem/200?v=1&index=1\",\"plan\":{\"pricingDetails\":{\"__typename\":\"AppUsagePricing\",\"balanceUsed\":{\"amount\":\"3.00\",\"currencyCode\":\"USD\"}}}}},\"userErrors\":[]}}}"
  let assert Some(after_first) =
    store.get_effective_app_subscription_line_item_by_id(first.store, li_id)
  case after_first.plan.pricing_details {
    AppUsagePricing(balance_used: balance, ..) -> {
      assert balance.amount == "3.00"
      assert balance.currency_code == "USD"
    }
    _ -> panic as "expected usage pricing"
  }

  let over_cap_document =
    "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
    <> li_id
    <> "\", description: \"second\", price: { amount: \"3.00\", currencyCode: USD }, idempotencyKey: \"usage-key-2\") { appUsageRecord { id } userErrors { field message } } }"
  let over_cap =
    apps.process_mutation(
      first.store,
      first.identity,
      "/admin/api/2025-01/graphql.json",
      over_cap_document,
      dict.new(),
      upstream_query.UpstreamContext(
        transport: None,
        origin: "https://shopify.example",
        headers: dict.new(),
      ),
    )
  assert json.to_string(over_cap.data)
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":null,\"userErrors\":[{\"field\":[],\"message\":\"Total price exceeds balance remaining\"}]}}}"

  let duplicate_document =
    "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
    <> li_id
    <> "\", description: \"first again\", price: { amount: \"3.00\", currencyCode: USD }, idempotencyKey: \"usage-key-1\") { appUsageRecord { id description price { amount currencyCode } } userErrors { field message } } }"
  let duplicate =
    apps.process_mutation(
      over_cap.store,
      over_cap.identity,
      "/admin/api/2025-01/graphql.json",
      duplicate_document,
      dict.new(),
      upstream_query.UpstreamContext(
        transport: None,
        origin: "https://shopify.example",
        headers: dict.new(),
      ),
    )
  assert json.to_string(duplicate.data)
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":{\"id\":\"gid://shopify/AppUsageRecord/1\",\"description\":\"first\",\"price\":{\"amount\":\"3.00\",\"currencyCode\":\"USD\"}},\"userErrors\":[]}}}"

  let readback =
    run_query(
      duplicate.store,
      "{ currentAppInstallation { activeSubscriptions { lineItems { plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } usageRecords { nodes { id description price { amount currencyCode } } } } } } }",
    )
  assert readback
    == "{\"currentAppInstallation\":{\"activeSubscriptions\":[{\"lineItems\":[{\"plan\":{\"pricingDetails\":{\"__typename\":\"AppUsagePricing\",\"balanceUsed\":{\"amount\":\"3.00\",\"currencyCode\":\"USD\"}}},\"usageRecords\":{\"nodes\":[{\"id\":\"gid://shopify/AppUsageRecord/1\",\"description\":\"first\",\"price\":{\"amount\":\"3.00\",\"currencyCode\":\"USD\"}}]}}]}]}}"
}

pub fn usage_record_create_rejects_non_usage_line_item_test() {
  let #(s, li_id) =
    seeded_billing_line_item(
      AppSubscriptionLineItemPlan(pricing_details: AppRecurringPricing(
        price: money("9.99", "USD"),
        interval: "EVERY_30_DAYS",
        plan_handle: None,
      )),
    )
  let body =
    run_mutation(
      s,
      "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
        <> li_id
        <> "\", description: \"recurring\", price: { amount: \"1.00\", currencyCode: USD }) { appUsageRecord { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":null,\"userErrors\":[{\"field\":[\"subscriptionLineItemId\"],\"message\":\"Subscription line item must use usage pricing\"}]}}}"
}

pub fn usage_record_create_rejects_long_idempotency_key_test() {
  let #(s, li_id) =
    seeded_billing_line_item(usage_line_item_plan("5.00", "0.00", "USD"))
  let long_key = string.repeat("x", times: 256)
  let body =
    run_mutation(
      s,
      "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
        <> li_id
        <> "\", description: \"too long\", price: { amount: \"1.00\", currencyCode: USD }, idempotencyKey: \""
        <> long_key
        <> "\") { appUsageRecord { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":null,\"userErrors\":[{\"field\":[\"idempotencyKey\"],\"message\":\"Idempotency key must be at most 255 characters\",\"code\":null}]}}}"
}

pub fn usage_record_create_rejects_currency_mismatch_test() {
  let #(s, li_id) =
    seeded_billing_line_item(usage_line_item_plan("5.00", "0.00", "USD"))
  let body =
    run_mutation(
      s,
      "mutation { appUsageRecordCreate(subscriptionLineItemId: \""
        <> li_id
        <> "\", description: \"wrong currency\", price: { amount: \"1.00\", currencyCode: CAD }) { appUsageRecord { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"appUsageRecordCreate\":{\"appUsageRecord\":null,\"userErrors\":[{\"field\":[\"price\",\"currencyCode\"],\"message\":\"Currency code must match capped amount currency\"}]}}}"
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

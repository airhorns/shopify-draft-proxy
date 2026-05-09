//// Mutation-path tests for `proxy/functions`.
////
//// Covers all 6 mutation roots (`validationCreate`/`Update`/`Delete`,
//// `cartTransformCreate`/`Delete`, `taxAppConfigure`) plus the
//// `is_function_mutation_root` predicate, the `process_mutation`
//// `{"data": …}` envelope, and Function reference resolution behavior.

import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{UpstreamContext}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type AccessScopeRecord, type AppInstallationRecord, type AppRecord,
  type CartTransformRecord, type ShopRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, type ValidationMetafieldRecord,
  type ValidationRecord, AccessScopeRecord, AppInstallationRecord, AppRecord,
  CartTransformRecord, PaymentSettingsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopEntitlementsRecord,
  ShopFeaturesRecord, ShopGiftCardsEntitlementRecord, ShopPlanRecord, ShopRecord,
  ShopResourceLimitsRecord, ShopifyFunctionAppRecord, ShopifyFunctionRecord,
  ValidationMetafieldRecord, ValidationRecord,
}

// ----------- Helpers -----------

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
  let request_path = "/admin/api/2025-01/graphql.json"
  let outcome =
    functions.process_mutation(
      store_in,
      identity,
      request_path,
      document,
      dict.new(),
      UpstreamContext(
        transport: None,
        origin: "",
        headers: headers,
        allow_upstream_reads: False,
      ),
    )
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      dict.new(),
      outcome.log_drafts,
    )
  mutation_helpers.MutationOutcome(
    ..outcome,
    store: logged_store,
    identity: logged_identity,
  )
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn shopify_fn(
  id: String,
  handle: String,
  api_type: String,
) -> ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    id: id,
    title: Some("Function " <> handle),
    handle: Some(handle),
    api_type: Some(api_type),
    description: None,
    app_key: None,
    app: None,
    create_guardrail_code: None,
    create_guardrail_message: None,
  )
}

fn shopify_fn_with_app_key(
  id: String,
  handle: String,
  api_type: String,
  app_key: String,
) -> ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    ..shopify_fn(id, handle, api_type),
    app_key: Some(app_key),
  )
}

fn shopify_fn_with_app(
  id: String,
  handle: String,
  api_type: String,
  app: ShopifyFunctionAppRecord,
) -> ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    ..shopify_fn(id, handle, api_type),
    app_key: app.api_key,
    app: Some(app),
  )
}

fn function_app(id: String, api_key: String) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: Some("App"),
    id: Some(id),
    title: Some("Function owner"),
    handle: Some("function-owner"),
    api_key: Some(api_key),
  )
}

fn function_app_without_id(api_key: String) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: Some("App"),
    id: None,
    title: Some("Function owner"),
    handle: Some("function-owner"),
    api_key: Some(api_key),
  )
}

fn access_scope(handle: String) -> AccessScopeRecord {
  AccessScopeRecord(handle: handle, description: None)
}

fn shopify_fn_with_guardrail(
  id: String,
  handle: String,
  api_type: String,
  code: String,
  message: String,
) -> ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    ..shopify_fn(id, handle, api_type),
    create_guardrail_code: Some(code),
    create_guardrail_message: Some(message),
  )
}

fn app(id: String, api_key: String) -> AppRecord {
  AppRecord(
    id: id,
    api_key: Some(api_key),
    handle: Some("test-app"),
    title: Some("Test app"),
    developer_name: Some("test-dev"),
    embedded: Some(True),
    previously_installed: Some(False),
    requested_access_scopes: [access_scope("read_products")],
  )
}

fn installation(id: String, app_id: String) -> AppInstallationRecord {
  installation_with_scopes(id, app_id, [])
}

fn installation_with_scopes(
  id: String,
  app_id: String,
  access_scopes: List(AccessScopeRecord),
) -> AppInstallationRecord {
  AppInstallationRecord(
    id: id,
    app_id: app_id,
    launch_url: Some("https://example.com/admin/apps/test"),
    uninstall_url: None,
    access_scopes: access_scopes,
    active_subscription_ids: [],
    all_subscription_ids: [],
    one_time_purchase_ids: [],
    uninstalled_at: None,
  )
}

fn validation_metafield(
  id: String,
  validation_id: String,
  namespace: String,
  key: String,
  value: String,
) -> ValidationMetafieldRecord {
  ValidationMetafieldRecord(
    id: id,
    validation_id: validation_id,
    namespace: namespace,
    key: key,
    type_: Some("single_line_text_field"),
    value: Some(value),
    compare_digest: None,
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-01T00:00:00.000Z"),
    owner_type: Some("VALIDATION"),
  )
}

fn seed_function(
  store_in: store.Store,
  record: ShopifyFunctionRecord,
) -> store.Store {
  let #(_, s) = store.upsert_staged_shopify_function(store_in, record)
  s
}

fn seed_base_function(
  store_in: store.Store,
  record: ShopifyFunctionRecord,
) -> store.Store {
  store.upsert_base_shopify_functions(store_in, [record])
}

fn seed_validation(
  store_in: store.Store,
  record: ValidationRecord,
) -> store.Store {
  let #(_, s) = store.upsert_staged_validation(store_in, record)
  s
}

fn seed_active_validations(
  store_in: store.Store,
  fn_record: ShopifyFunctionRecord,
  index: Int,
  count: Int,
) -> store.Store {
  case index > count {
    True -> store_in
    False ->
      seed_active_validations(
        seed_validation(
          store_in,
          ValidationRecord(
            id: "gid://shopify/Validation/active-" <> int.to_string(index),
            title: Some("Active " <> int.to_string(index)),
            enable: Some(True),
            block_on_failure: Some(False),
            function_id: Some(fn_record.id),
            function_handle: Some("cap"),
            shopify_function_id: Some(fn_record.id),
            metafields: [],
            created_at: None,
            updated_at: None,
          ),
        ),
        fn_record,
        index + 1,
        count,
      )
  }
}

fn seed_cart_transform(
  store_in: store.Store,
  record: CartTransformRecord,
) -> store.Store {
  let #(_, s) = store.upsert_staged_cart_transform(store_in, record)
  s
}

fn non_plus_custom_app_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/guardrail",
    name: "Guardrail shop",
    myshopify_domain: "guardrail.myshopify.com",
    url: "https://guardrail.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/guardrail",
      host: "guardrail.myshopify.com",
      url: "https://guardrail.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "owner@example.com",
    email: "owner@example.com",
    currency_code: "USD",
    enabled_presentment_currencies: ["USD"],
    iana_timezone: "UTC",
    timezone_abbreviation: "UTC",
    timezone_offset: "+0000",
    timezone_offset_minutes: 0,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "IMPERIAL_SYSTEM",
    weight_unit: "POUNDS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/guardrail",
      address1: None,
      address2: None,
      city: None,
      company: None,
      coordinates_validated: False,
      country: None,
      country_code_v2: None,
      formatted: [],
      formatted_area: None,
      latitude: None,
      longitude: None,
      phone: None,
      province: None,
      province_code: None,
      zip: None,
    ),
    plan: ShopPlanRecord(
      partner_development: False,
      public_display_name: "Basic",
      shopify_plus: False,
    ),
    resource_limits: ShopResourceLimitsRecord(
      location_limit: 0,
      max_product_options: 0,
      max_product_variants: 0,
      redirect_limit_reached: False,
    ),
    features: ShopFeaturesRecord(
      avalara_avatax: False,
      branding: "SHOPIFY",
      bundles: ShopBundlesFeatureRecord(
        eligible_for_bundles: False,
        ineligibility_reason: None,
        sells_bundles: False,
      ),
      captcha: False,
      cart_transform: ShopCartTransformFeatureRecord(
        eligible_operations: ShopCartTransformEligibleOperationsRecord(
          expand_operation: False,
          merge_operation: False,
          update_operation: False,
        ),
      ),
      dynamic_remarketing: False,
      eligible_for_subscription_migration: False,
      eligible_for_subscriptions: False,
      gift_cards: False,
      harmonized_system_code: False,
      legacy_subscription_gateway_enabled: False,
      live_view: False,
      paypal_express_subscription_gateway_status: "DISABLED",
      reports: False,
      b2b_deposits_enabled: True,
      discounts_by_market_enabled: False,
      markets_granted: 50,
      sells_subscriptions: False,
      show_metrics: False,
      storefront: False,
      unified_markets: True,
    ),
    entitlements: ShopEntitlementsRecord(
      gift_cards: ShopGiftCardsEntitlementRecord(enabled: True),
    ),
    payment_settings: PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
    shop_policies: [],
  )
}

// ----------- is_function_mutation_root -----------

pub fn is_function_mutation_root_test() {
  assert functions.is_function_mutation_root("validationCreate")
  assert functions.is_function_mutation_root("validationUpdate")
  assert functions.is_function_mutation_root("validationDelete")
  assert functions.is_function_mutation_root("cartTransformCreate")
  assert functions.is_function_mutation_root("cartTransformDelete")
  assert functions.is_function_mutation_root("taxAppConfigure")
  assert !functions.is_function_mutation_root("validation")
  assert !functions.is_function_mutation_root("appUninstall")
}

// ----------- envelope -----------

pub fn process_mutation_returns_data_envelope_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let body =
    run_mutation(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"checkout-validator\", title: \"My validator\" }) { validation { id title } userErrors { field } } }",
    )
  // Always wraps in `{"data": {...}}`.
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"title\":\"My validator\"},\"userErrors\":[]}}}"
}

pub fn process_mutation_records_staged_log_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { taxAppConfigure(ready: true) { taxAppConfiguration { id } userErrors { field message code } } }",
    )

  let assert [entry] = store.get_log(outcome.store)
  assert entry.operation_name == Some("taxAppConfigure")
  assert entry.path == "/admin/api/2025-01/graphql.json"
  assert entry.status == store_types.Staged
  assert entry.staged_resource_ids
    == [
      "gid://shopify/TaxAppConfiguration/local",
    ]
  assert entry.interpreted.capability.domain == "functions"
  assert entry.interpreted.capability.execution == "stage-locally"
}

// ----------- validationCreate -----------

pub fn validation_create_with_handle_stages_validation_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"checkout-validator\", title: \"My validator\" }) { validation { id title enable enabled blockOnFailure functionId functionHandle shopifyFunction { id handle apiType } createdAt updatedAt } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  // Validation gid: synthetic #1. ShopifyFunction metadata is reused from state.
  // Timestamp: 2024-01-01T00:00:00.000Z (first synthetic timestamp).
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"title\":\"My validator\",\"enable\":false,\"enabled\":false,\"blockOnFailure\":false,\"functionId\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"functionHandle\":\"checkout-validator\",\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"handle\":\"checkout-validator\",\"apiType\":\"VALIDATION\"},\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Validation/1"]
  // The staged validation is visible and the referenced Function metadata is preserved.
  let assert Some(created) =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/1",
    )
  assert created.function_id == Some(fn_record.id)
  assert created.function_handle == Some("checkout-validator")
  assert created.shopify_function_id == Some(fn_record.id)
  let assert Some(_) =
    store.get_effective_shopify_function_by_id(
      outcome.store,
      "gid://shopify/ShopifyFunction/checkout-validator",
    )
}

pub fn validation_create_missing_function_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { validationCreate(validation: { title: \"No function\" }) { validation { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionHandle\"],\"message\":\"Either function_id or function_handle must be provided.\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\"}]}}}"
}

pub fn validation_create_multiple_function_identifiers_emits_user_error_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { validationCreate(validation: { functionId: \"gid://shopify/ShopifyFunction/one\", functionHandle: \"two\" }) { validation { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\"],\"message\":\"Only one of function_id or function_handle can be provided, not both.\",\"code\":\"MULTIPLE_FUNCTION_IDENTIFIERS\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
  assert store.list_effective_shopify_functions(outcome.store) == []
}

pub fn validation_create_unknown_function_emits_function_not_found_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { validationCreate(validation: { functionId: \"gid://shopify/ShopifyFunction/missing\" }) { validation { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionId\"],\"message\":\"Extension not found.\",\"code\":\"NOT_FOUND\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
  assert store.list_effective_shopify_functions(outcome.store) == []
}

pub fn validation_create_rejects_non_validation_function_test() {
  let cart_fn =
    shopify_fn("gid://shopify/ShopifyFunction/cart", "cart", "CART_TRANSFORM")
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), cart_fn),
      "mutation { validationCreate(validation: { functionId: \"gid://shopify/ShopifyFunction/cart\" }) { validation { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionId\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].\",\"code\":\"FUNCTION_DOES_NOT_IMPLEMENT\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
}

pub fn validation_create_rejects_custom_app_function_on_known_non_plus_shop_test() {
  let fn_record =
    shopify_fn_with_app_key(
      "gid://shopify/ShopifyFunction/non-plus-validation",
      "non-plus-validation",
      "VALIDATION",
      "custom-app-key",
    )
  let s =
    store.new()
    |> store.upsert_base_shop(non_plus_custom_app_shop())
    |> seed_function(fn_record)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { validationCreate(validation: { functionId: \"gid://shopify/ShopifyFunction/non-plus-validation\", enable: true }) { validation { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionId\"],\"message\":\"Shop must be on a Shopify Plus plan to activate functions from a custom app.\",\"code\":\"CUSTOM_APP_FUNCTION_NOT_ELIGIBLE\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
}

pub fn validation_create_rejects_required_input_guardrail_test() {
  let fn_record =
    shopify_fn_with_guardrail(
      "gid://shopify/ShopifyFunction/input-validation",
      "input-validation",
      "VALIDATION",
      "REQUIRED_INPUT_FIELD",
      "Required input field must be present.",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"input-validation\" }) { validation { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionHandle\"],\"message\":\"Required input field must be present.\",\"code\":\"REQUIRED_INPUT_FIELD\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
}

pub fn validation_create_reuses_existing_function_test() {
  // Pre-seed a ShopifyFunction; create a validation that references it by
  // handle. The handler must reuse the seeded function rather than mint a
  // new one.
  let seeded =
    shopify_fn(
      "gid://shopify/ShopifyFunction/seeded",
      "checkout-validator",
      "VALIDATION",
    )
  let s = seed_function(store.new(), seeded)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { validationCreate(validation: { functionHandle: \"checkout-validator\" }) { validation { shopifyFunction { id title handle } } userErrors { field } } }",
    )
  // Reused id "seeded" — not the handle-derived id.
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/seeded\",\"title\":\"Function checkout-validator\",\"handle\":\"checkout-validator\"}},\"userErrors\":[]}}}"
}

pub fn validation_create_defaults_enable_and_block_test() {
  let fn_record =
    shopify_fn("gid://shopify/ShopifyFunction/v", "v", "VALIDATION")
  let body =
    run_mutation(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"v\" }) { validation { enable blockOnFailure } } }",
    )
  // Shopify defaults omitted enable/enabled to false, blockOnFailure to false.
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"enable\":false,\"blockOnFailure\":false}}}}"
}

pub fn validation_create_title_falls_back_to_function_title_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/conformance-validation",
      "conformance-validation",
      "VALIDATION",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { omitted: validationCreate(validation: { functionHandle: \"conformance-validation\" }) { validation { id title functionId functionHandle } userErrors { field } } explicitNull: validationCreate(validation: { functionHandle: \"conformance-validation\", title: null }) { validation { id title functionId functionHandle } userErrors { field } } emptyString: validationCreate(validation: { functionHandle: \"conformance-validation\", title: \"\" }) { validation { id title functionId functionHandle } userErrors { field } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"omitted\":{\"validation\":{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},\"userErrors\":[]},\"explicitNull\":{\"validation\":{\"id\":\"gid://shopify/Validation/3\",\"title\":\"Function conformance-validation\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},\"userErrors\":[]},\"emptyString\":{\"validation\":{\"id\":\"gid://shopify/Validation/4\",\"title\":\"\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      outcome.store,
      "{ validation(id: \"gid://shopify/Validation/2\") { id title functionId functionHandle } validations(first: 5) { nodes { id title functionId functionHandle } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validation\":{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},\"validations\":{\"nodes\":[{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},{\"id\":\"gid://shopify/Validation/3\",\"title\":\"Function conformance-validation\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"},{\"id\":\"gid://shopify/Validation/4\",\"title\":\"\",\"functionId\":\"gid://shopify/ShopifyFunction/conformance-validation\",\"functionHandle\":\"conformance-validation\"}]}}"
}

pub fn validation_create_does_not_accept_enabled_alias_test() {
  let fn_record =
    shopify_fn("gid://shopify/ShopifyFunction/v", "v", "VALIDATION")
  let body =
    run_mutation(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"v\", enabled: true }) { validation { enable blockOnFailure } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"enable\":false,\"blockOnFailure\":false},\"userErrors\":[]}}}"
}

pub fn validation_create_active_cap_returns_user_error_and_stages_nothing_test() {
  let fn_record =
    shopify_fn("gid://shopify/ShopifyFunction/cap", "cap", "VALIDATION")
  let seeded =
    seed_active_validations(
      seed_function(store.new(), fn_record),
      fn_record,
      1,
      25,
    )
  let outcome =
    run_mutation_outcome(
      seeded,
      "mutation { validationCreate(validation: { functionHandle: \"cap\", enable: true }) { validation { id enable } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[],\"message\":\"Cannot have more than 25 active validation functions.\",\"code\":\"MAX_VALIDATIONS_ACTIVATED\"}]}}}"
  assert list.length(store.list_effective_validations(outcome.store)) == 25
}

pub fn validation_create_persists_metafields_for_downstream_reads_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/metafield-validation",
      "metafield-validation",
      "VALIDATION",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"metafield-validation\", title: \"Metafield validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"strict\" }] }) { validation { id metafields(first: 5) { nodes { namespace key value } } } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\"}]}},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      outcome.store,
      "{ validation(id: \"gid://shopify/Validation/1\") { id metafields(first: 5) { nodes { namespace key value } } } validations(first: 5) { nodes { id metafields(first: 5) { nodes { namespace key value } } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\"}]}},\"validations\":{\"nodes\":[{\"id\":\"gid://shopify/Validation/1\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\"}]}}]}}"
}

pub fn validation_create_rejects_invalid_metafields_atomically_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/metafield-validation",
      "metafield-validation",
      "VALIDATION",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { mixedMissingKey: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"strict\" }, { namespace: \"custom\", type: \"single_line_text_field\", value: \"v\" }] }) { validation { id } userErrors { field message code } } missingType: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", value: \"v\" }] }) { validation { id } userErrors { field message code } } blankType: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"\", value: \"v\" }] }) { validation { id } userErrors { field message code } } missingValue: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\" }] }) { validation { id } userErrors { field message code } } blankValue: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"\" }] }) { validation { id } userErrors { field message code } } invalidType: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"bogus_type\", value: \"v\" }] }) { validation { id } userErrors { field message code } } reservedShopify: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"shopify\", key: \"mode\", type: \"single_line_text_field\", value: \"v\" }] }) { validation { id } userErrors { field message code } } invalidValue: validationCreate(validation: { functionHandle: \"metafield-validation\", metafields: [{ namespace: \"custom\", key: \"count\", type: \"number_integer\", value: \"not a number\" }] }) { validation { id } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"mixedMissingKey\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"1\"],\"message\":\"presence\",\"code\":null}]},\"missingType\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"One or more required inputs are blank.\",\"code\":\"BLANK\"}]},\"blankType\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"One or more required inputs are blank.\",\"code\":\"BLANK\"}]},\"missingValue\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"presence\",\"code\":null}]},\"blankValue\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"The value is invalid.\",\"code\":\"INVALID_VALUE\"}]},\"invalidType\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"The type is invalid.\",\"code\":\"INVALID_TYPE\"}]},\"reservedShopify\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"ApiPermission metafields can only be created or updated by the app owner.\",\"code\":\"APP_NOT_AUTHORIZED\"}]},\"invalidValue\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"The value is invalid.\",\"code\":\"INVALID_VALUE\"}]}}}"
  assert store.list_effective_validations(outcome.store) == []
}

// ----------- validationUpdate -----------

pub fn validation_update_changes_title_and_enable_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v =
    ValidationRecord(
      id: "gid://shopify/Validation/77",
      title: Some("Original"),
      enable: Some(True),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let body =
    run_mutation(
      s,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/77\", validation: { title: \"Renamed\", enable: false }) { validation { id title enable } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/77\",\"title\":\"Renamed\",\"enable\":false},\"userErrors\":[]}}}"
}

pub fn validation_update_omitted_enable_and_block_reset_defaults_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v =
    ValidationRecord(
      id: "gid://shopify/Validation/defaults",
      title: Some("Original"),
      enable: Some(True),
      block_on_failure: Some(True),
      function_id: None,
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let document =
    "mutation { validationUpdate(id: \"gid://shopify/Validation/defaults\", validation: { title: \"Renamed\" }) { validation { id title enable blockOnFailure } userErrors { field message code } } }"
  let outcome =
    run_mutation_outcome(
      store.new()
        |> seed_function(fn_record)
        |> seed_validation(v),
      document,
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/defaults\",\"title\":\"Renamed\",\"enable\":false,\"blockOnFailure\":false},\"userErrors\":[]}}}"
  let assert Some(updated) =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/defaults",
    )
  assert updated.enable == Some(False)
  assert updated.block_on_failure == Some(False)

  let assert [entry] = store.get_log(outcome.store)
  assert entry.status == store_types.Staged
  assert entry.operation_name == Some("validationUpdate")
  assert entry.query == document
  assert entry.staged_resource_ids == ["gid://shopify/Validation/defaults"]
}

pub fn validation_update_function_inputs_do_not_rebind_test() {
  let fn_a =
    shopify_fn(
      "gid://shopify/ShopifyFunction/function-a",
      "function-a",
      "VALIDATION",
    )
  let fn_b =
    shopify_fn(
      "gid://shopify/ShopifyFunction/function-b",
      "function-b",
      "VALIDATION",
    )
  let v =
    ValidationRecord(
      id: "gid://shopify/Validation/rebind",
      title: Some("Original"),
      enable: Some(True),
      block_on_failure: Some(False),
      function_id: Some(fn_a.id),
      function_handle: Some("function-a"),
      shopify_function_id: Some(fn_a.id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let outcome =
    run_mutation_outcome(
      store.new()
        |> seed_function(fn_a)
        |> seed_function(fn_b)
        |> seed_validation(v),
      "mutation { validationUpdate(id: \"gid://shopify/Validation/rebind\", validation: { functionId: \"gid://shopify/ShopifyFunction/function-b\", title: \"Still A\" }) { validation { id title functionId functionHandle shopifyFunction { id handle } } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/rebind\",\"title\":\"Still A\",\"functionId\":\"gid://shopify/ShopifyFunction/function-a\",\"functionHandle\":\"function-a\",\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/function-a\",\"handle\":\"function-a\"}},\"userErrors\":[]}}}"
  let assert Some(updated) =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/rebind",
    )
  assert updated.function_id == Some(fn_a.id)
  assert updated.function_handle == Some("function-a")
  assert updated.shopify_function_id == Some(fn_a.id)
}

pub fn validation_update_reenable_enforces_active_cap_test() {
  let fn_record =
    shopify_fn("gid://shopify/ShopifyFunction/cap", "cap", "VALIDATION")
  let seeded =
    seed_active_validations(
      seed_function(store.new(), fn_record),
      fn_record,
      1,
      25,
    )
  let inactive =
    ValidationRecord(
      id: "gid://shopify/Validation/inactive-26",
      title: Some("Inactive"),
      enable: Some(False),
      block_on_failure: Some(False),
      function_id: Some(fn_record.id),
      function_handle: Some("cap"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let outcome =
    run_mutation_outcome(
      seed_validation(seeded, inactive),
      "mutation { validationUpdate(id: \"gid://shopify/Validation/inactive-26\", validation: { enable: true }) { validation { id enable } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":null,\"userErrors\":[{\"field\":[],\"message\":\"Cannot have more than 25 active validation functions.\",\"code\":\"MAX_VALIDATIONS_ACTIVATED\"}]}}}"
  let assert Some(unchanged) =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/inactive-26",
    )
  assert unchanged.enable == Some(False)
}

pub fn validation_update_persists_metafields_for_downstream_reads_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/metafield-validation",
      "metafield-validation",
      "VALIDATION",
    )
  let create_outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"metafield-validation\", title: \"Metafield validation\" }) { validation { id } userErrors { field } } }",
    )
  let update_outcome =
    run_mutation_outcome(
      create_outcome.store,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/1\", validation: { metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"strict\" }] }) { validation { id metafields(first: 5) { nodes { namespace key value } } } userErrors { field message code } } }",
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\"}]}},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      update_outcome.store,
      "{ validations(first: 5) { nodes { id metafields(first: 5) { nodes { namespace key value } } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validations\":{\"nodes\":[{\"id\":\"gid://shopify/Validation/1\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\"}]}}]}}"
}

pub fn validation_update_rejects_invalid_metafields_without_mutation_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/metafield-validation",
      "metafield-validation",
      "VALIDATION",
    )
  let create_outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { validationCreate(validation: { functionHandle: \"metafield-validation\", title: \"Metafield validation\", metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"strict\" }] }) { validation { id } userErrors { field } } }",
    )
  let update_outcome =
    run_mutation_outcome(
      create_outcome.store,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/1\", validation: { metafields: [{ namespace: \"custom\", type: \"single_line_text_field\", value: \"loose\" }] }) { validation { id metafields(first: 5) { nodes { namespace key value } } } userErrors { field message code } } }",
    )

  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"metafields\",\"0\"],\"message\":\"presence\",\"code\":null}]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      update_outcome.store,
      "{ validation(id: \"gid://shopify/Validation/1\") { title metafields(first: 5) { nodes { namespace key type value } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validation\":{\"title\":\"Metafield validation\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"type\":\"single_line_text_field\",\"value\":\"strict\"}]}}}"
}

pub fn validation_update_upserts_metafields_without_wiping_existing_rows_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/metafield-validation",
      "metafield-validation",
      "VALIDATION",
    )
  let validation_id = "gid://shopify/Validation/metafields-upsert"
  let base_metafields = [
    validation_metafield(
      "gid://shopify/Metafield/existing-mode",
      validation_id,
      "custom",
      "mode",
      "strict",
    ),
    validation_metafield(
      "gid://shopify/Metafield/existing-color",
      validation_id,
      "custom",
      "color",
      "blue",
    ),
  ]
  let v =
    ValidationRecord(
      id: validation_id,
      title: Some("Original"),
      enable: Some(True),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("metafield-validation"),
      shopify_function_id: Some(fn_record.id),
      metafields: base_metafields,
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let seeded =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)

  let title_only =
    run_mutation_outcome(
      seeded,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/metafields-upsert\", validation: { title: \"Renamed\" }) { validation { id metafields(first: 5) { nodes { namespace key value updatedAt } } } userErrors { field message code } } }",
    )
  assert json.to_string(title_only.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/metafields-upsert\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"color\",\"value\":\"blue\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}]}},\"userErrors\":[]}}}"

  let empty_update =
    run_mutation_outcome(
      title_only.store,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/metafields-upsert\", validation: { metafields: [] }) { validation { id metafields(first: 5) { nodes { namespace key value updatedAt } } } userErrors { field message code } } }",
    )
  assert json.to_string(empty_update.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/metafields-upsert\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"strict\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"color\",\"value\":\"blue\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}]}},\"userErrors\":[]}}}"

  let partial_upsert =
    run_mutation_outcome(
      empty_update.store,
      "mutation { validationUpdate(id: \"gid://shopify/Validation/metafields-upsert\", validation: { metafields: [{ namespace: \"custom\", key: \"mode\", type: \"single_line_text_field\", value: \"relaxed\" }, { namespace: \"custom\", key: \"size\", type: \"single_line_text_field\", value: \"large\" }] }) { validation { id metafields(first: 5) { nodes { namespace key value updatedAt } } } userErrors { field message code } } }",
    )
  assert json.to_string(partial_upsert.data)
    == "{\"data\":{\"validationUpdate\":{\"validation\":{\"id\":\"gid://shopify/Validation/metafields-upsert\",\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"relaxed\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"color\",\"value\":\"blue\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"size\",\"value\":\"large\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}]}},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      partial_upsert.store,
      "{ validation(id: \"gid://shopify/Validation/metafields-upsert\") { metafields(first: 5) { nodes { namespace key value updatedAt } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validation\":{\"metafields\":{\"nodes\":[{\"namespace\":\"custom\",\"key\":\"mode\",\"value\":\"relaxed\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"color\",\"value\":\"blue\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"custom\",\"key\":\"size\",\"value\":\"large\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}]}}}"
}

pub fn validation_update_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { validationUpdate(id: \"gid://shopify/Validation/missing\", validation: { title: \"x\" }) { validation { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"validationUpdate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Extension not found.\",\"code\":\"NOT_FOUND\"}]}}}"
}

// ----------- validationDelete -----------

pub fn validation_delete_removes_record_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v =
    ValidationRecord(
      id: "gid://shopify/Validation/88",
      title: Some("Doomed"),
      enable: Some(True),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { validationDelete(id: \"gid://shopify/Validation/88\") { deletedId userErrors { field message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationDelete\":{\"deletedId\":\"gid://shopify/Validation/88\",\"userErrors\":[]}}}"
  // Now invisible to effective lookup.
  let assert None =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/88",
    )
  assert outcome.staged_resource_ids == ["gid://shopify/Validation/88"]
}

pub fn validation_delete_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { validationDelete(id: \"gid://shopify/Validation/missing\") { deletedId userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"validationDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Extension not found.\",\"code\":\"NOT_FOUND\"}]}}}"
}

pub fn validation_delete_bare_id_returns_canonical_deleted_id_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v =
    ValidationRecord(
      id: "gid://shopify/Validation/88",
      title: Some("Doomed"),
      enable: Some(True),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { validationDelete(id: \"88\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"validationDelete\":{\"deletedId\":\"gid://shopify/Validation/88\",\"userErrors\":[]}}}"
  let assert None =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/88",
    )
  assert outcome.staged_resource_ids == ["gid://shopify/Validation/88"]
}

// ----------- cartTransformCreate -----------

pub fn cart_transform_create_with_handle_mints_records_test() {
  let s =
    seed_function(
      store.new(),
      shopify_fn(
        "gid://shopify/ShopifyFunction/cart-transformer",
        "cart-transformer",
        "CART_TRANSFORM",
      ),
    )
  let body =
    run_mutation(
      s,
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\", blockOnFailure: false) { cartTransform { id functionId blockOnFailure } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\",\"functionId\":\"gid://shopify/ShopifyFunction/cart-transformer\",\"blockOnFailure\":false},\"userErrors\":[]}}}"
}

pub fn cart_transform_create_reads_top_level_args_test() {
  let s =
    seed_function(
      store.new(),
      shopify_fn(
        "gid://shopify/ShopifyFunction/cart-transformer",
        "cart-transformer",
        "CART_TRANSFORM",
      ),
    )
  let body =
    run_mutation(
      s,
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\") { cartTransform { id functionId } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\",\"functionId\":\"gid://shopify/ShopifyFunction/cart-transformer\"},\"userErrors\":[]}}}"
}

pub fn cart_transform_create_missing_function_emits_user_error_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { cartTransformCreate { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Either function_id or function_handle must be provided.\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  assert list.is_empty(store.list_effective_shopify_functions(outcome.store))
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_with_both_function_identifiers_errors_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/cart-transformer\", functionHandle: \"cart-transformer\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Only one of function_id or function_handle can be provided, not both.\",\"code\":\"MULTIPLE_FUNCTION_IDENTIFIERS\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  assert list.is_empty(store.list_effective_shopify_functions(outcome.store))
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_unknown_function_id_errors_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/missing\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Function gid://shopify/ShopifyFunction/missing not found. Ensure that it is released in the current app (347082227713), and that the app is installed.\",\"code\":\"FUNCTION_NOT_FOUND\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  assert list.is_empty(store.list_effective_shopify_functions(outcome.store))
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_rejects_non_cart_transform_function_id_test() {
  let s =
    seed_base_function(
      store.new(),
      shopify_fn(
        "gid://shopify/ShopifyFunction/checkout-validator",
        "checkout-validator",
        "VALIDATION",
      ),
    )
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].\",\"code\":\"FUNCTION_NOT_FOUND\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert outcome.staged_resource_ids == []
  assert dict.size(outcome.store.staged_state.cart_transforms) == 0
  assert dict.size(outcome.store.staged_state.shopify_functions) == 0
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_rejects_non_cart_transform_function_handle_test() {
  let s =
    seed_base_function(
      store.new(),
      shopify_fn(
        "gid://shopify/ShopifyFunction/checkout-validator",
        "checkout-validator",
        "VALIDATION",
      ),
    )
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionHandle: \"checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].\",\"code\":\"FUNCTION_DOES_NOT_IMPLEMENT\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert outcome.staged_resource_ids == []
  assert dict.size(outcome.store.staged_state.cart_transforms) == 0
  assert dict.size(outcome.store.staged_state.shopify_functions) == 0
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_rejects_custom_app_function_on_known_non_plus_shop_test() {
  let fn_record =
    shopify_fn_with_app_key(
      "gid://shopify/ShopifyFunction/non-plus-cart-transform",
      "non-plus-cart-transform",
      "CART_TRANSFORM",
      "custom-app-key",
    )
  let s =
    store.new()
    |> store.upsert_base_shop(non_plus_custom_app_shop())
    |> seed_function(fn_record)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionHandle: \"non-plus-cart-transform\") { cartTransform { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Shop must be on a Shopify Plus plan to activate functions from a custom app.\",\"code\":\"CUSTOM_APP_FUNCTION_NOT_ELIGIBLE\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
}

pub fn cart_transform_create_rejects_pending_deletion_guardrail_test() {
  let fn_record =
    shopify_fn_with_guardrail(
      "gid://shopify/ShopifyFunction/pending-cart-transform",
      "pending-cart-transform",
      "CART_TRANSFORM",
      "FUNCTION_PENDING_DELETION",
      "Function is pending deletion.",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { cartTransformCreate(functionHandle: \"pending-cart-transform\") { cartTransform { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Function is pending deletion.\",\"code\":\"FUNCTION_PENDING_DELETION\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
}

pub fn cart_transform_create_rejects_plus_only_guardrail_test() {
  let fn_record =
    shopify_fn_with_guardrail(
      "gid://shopify/ShopifyFunction/plus-cart-transform",
      "plus-cart-transform",
      "CART_TRANSFORM",
      "FUNCTION_IS_PLUS_ONLY",
      "Shop must be on a Shopify Plus plan to activate this function.",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/plus-cart-transform\") { cartTransform { id } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Shop must be on a Shopify Plus plan to activate this function.\",\"code\":\"FUNCTION_IS_PLUS_ONLY\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
}

pub fn cart_transform_create_rejects_duplicate_function_id_test() {
  let function_id = "gid://shopify/ShopifyFunction/cart-transformer"
  let fn_record = shopify_fn(function_id, "cart-transformer", "CART_TRANSFORM")
  let existing =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/existing",
      title: Some("Existing"),
      block_on_failure: Some(False),
      function_id: Some(function_id),
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(function_id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_cart_transform(existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/cart-transformer\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Could not enable cart transform because it is already registered\",\"code\":\"FUNCTION_ALREADY_REGISTERED\"}]}}}"
  let assert [_] = store.list_effective_cart_transforms(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_duplicate_wrong_api_function_id_precedes_api_mismatch_test() {
  let function_id = "gid://shopify/ShopifyFunction/checkout-validator"
  let fn_record = shopify_fn(function_id, "checkout-validator", "VALIDATION")
  let existing =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/existing",
      title: Some("Existing"),
      block_on_failure: Some(False),
      function_id: Some(function_id),
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(function_id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_cart_transform(existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Could not enable cart transform because it is already registered\",\"code\":\"FUNCTION_ALREADY_REGISTERED\"}]}}}"
  let assert [_] = store.list_effective_cart_transforms(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_duplicate_wrong_api_function_handle_keeps_api_mismatch_first_test() {
  let function_id = "gid://shopify/ShopifyFunction/checkout-validator"
  let fn_record = shopify_fn(function_id, "checkout-validator", "VALIDATION")
  let existing =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/existing",
      title: Some("Existing"),
      block_on_failure: Some(False),
      function_id: Some(function_id),
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(function_id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_cart_transform(existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionHandle: \"checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].\",\"code\":\"FUNCTION_DOES_NOT_IMPLEMENT\"}]}}}"
  let assert [_] = store.list_effective_cart_transforms(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_validation_registered_wrong_api_function_id_precedes_api_mismatch_test() {
  let function_id = "gid://shopify/ShopifyFunction/checkout-validator"
  let fn_record = shopify_fn(function_id, "checkout-validator", "VALIDATION")
  let validation =
    ValidationRecord(
      id: "gid://shopify/Validation/existing",
      title: Some("Existing"),
      enable: Some(False),
      block_on_failure: Some(False),
      function_id: Some(function_id),
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(function_id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(validation)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionId: \"gid://shopify/ShopifyFunction/checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Could not enable cart transform because it is already registered\",\"code\":\"FUNCTION_ALREADY_REGISTERED\"}]}}}"
  let assert [_] = store.list_effective_validations(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_validation_registered_wrong_api_function_handle_keeps_api_mismatch_first_test() {
  let function_id = "gid://shopify/ShopifyFunction/checkout-validator"
  let fn_record = shopify_fn(function_id, "checkout-validator", "VALIDATION")
  let validation =
    ValidationRecord(
      id: "gid://shopify/Validation/existing",
      title: Some("Existing"),
      enable: Some(False),
      block_on_failure: Some(False),
      function_id: Some(function_id),
      function_handle: Some("checkout-validator"),
      shopify_function_id: Some(function_id),
      metafields: [],
      created_at: Some("2024-01-01T00:00:00.000Z"),
      updated_at: Some("2024-01-01T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(validation)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionHandle: \"checkout-validator\") { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionHandle\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].\",\"code\":\"FUNCTION_DOES_NOT_IMPLEMENT\"}]}}}"
  let assert [_] = store.list_effective_validations(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  assert list.is_empty(store.get_log(outcome.store))
}

pub fn cart_transform_create_persists_metafields_for_downstream_reads_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\", metafields: [{ namespace: \"bundles\", key: \"config\", type: \"json\", value: \"{\\\"enabled\\\":true}\" }, { namespace: \"bundles\", key: \"mode\", type: \"single_line_text_field\", value: \"strict\" }]) { cartTransform { id metafield(namespace: \"bundles\", key: \"config\") { namespace key type value ownerType compareDigest createdAt updatedAt } metafields(first: 5) { nodes { namespace key type value ownerType compareDigest createdAt updatedAt } } } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\",\"metafield\":{\"namespace\":\"bundles\",\"key\":\"config\",\"type\":\"json\",\"value\":\"{\\\"enabled\\\":true}\",\"ownerType\":\"CARTTRANSFORM\",\"compareDigest\":\"draft:WyJidW5kbGVzIiwiY29uZmlnIiwianNvbiIsIntcImVuYWJsZWRcIjp0cnVlfSIsbnVsbCwiMjAyNC0wMS0wMVQwMDowMDowMC4wMDBaIl0\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"metafields\":{\"nodes\":[{\"namespace\":\"bundles\",\"key\":\"config\",\"type\":\"json\",\"value\":\"{\\\"enabled\\\":true}\",\"ownerType\":\"CARTTRANSFORM\",\"compareDigest\":\"draft:WyJidW5kbGVzIiwiY29uZmlnIiwianNvbiIsIntcImVuYWJsZWRcIjp0cnVlfSIsbnVsbCwiMjAyNC0wMS0wMVQwMDowMDowMC4wMDBaIl0\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},{\"namespace\":\"bundles\",\"key\":\"mode\",\"type\":\"single_line_text_field\",\"value\":\"strict\",\"ownerType\":\"CARTTRANSFORM\",\"compareDigest\":\"draft:WyJidW5kbGVzIiwibW9kZSIsInNpbmdsZV9saW5lX3RleHRfZmllbGQiLCJzdHJpY3QiLG51bGwsIjIwMjQtMDEtMDFUMDA6MDA6MDAuMDAwWiJd\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}]}},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      outcome.store,
      "{ cartTransforms(first: 5) { nodes { id metafield(namespace: \"bundles\", key: \"mode\") { namespace key value } metafields(first: 5) { nodes { namespace key value } } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"cartTransforms\":{\"nodes\":[{\"id\":\"gid://shopify/CartTransform/1\",\"metafield\":{\"namespace\":\"bundles\",\"key\":\"mode\",\"value\":\"strict\"},\"metafields\":{\"nodes\":[{\"namespace\":\"bundles\",\"key\":\"config\",\"value\":\"{\\\"enabled\\\":true}\"},{\"namespace\":\"bundles\",\"key\":\"mode\",\"value\":\"strict\"}]}}]}}"
}

pub fn cart_transform_create_rejects_invalid_metafields_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\", metafields: [{ namespace: \"bundles\", key: \"missing_value\", type: \"single_line_text_field\" }, { namespace: \"bundles\", key: \"bad_json\", type: \"json\", value: \"not-json\" }]) { cartTransform { id } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"value\"],\"message\":\"may not be empty\",\"code\":\"INVALID_METAFIELDS\"},{\"field\":[\"metafields\",\"1\",\"value\"],\"message\":\"is invalid JSON: unexpected token 'not-json' at line 1 column 1.\",\"code\":\"INVALID_METAFIELDS\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
}

// ----------- cartTransformDelete -----------

pub fn cart_transform_delete_removes_record_test() {
  // Pre-stage by minting via create.
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/current", "current-app-key"),
    ))
  let create_outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\") { cartTransform { id } } }",
    )
  let body =
    run_mutation(
      create_outcome.store,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/1\") { deletedId userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/1\",\"userErrors\":[]}}}"
}

pub fn cart_transform_delete_after_ownerless_create_uses_modeled_current_app_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let create_outcome =
    run_mutation_outcome(
      seed_function(store.new(), fn_record),
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\") { cartTransform { id } userErrors { field message code } } }",
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\"},\"userErrors\":[]}}}"
  let assert Some(installation) =
    store.get_current_app_installation(create_outcome.store)
  let assert Some(owner_function) =
    store.get_effective_shopify_function_by_id(
      create_outcome.store,
      "gid://shopify/ShopifyFunction/cart-transformer",
    )
  let assert Some(owner_app) = owner_function.app
  assert owner_app.id == Some(installation.app_id)

  let delete_outcome =
    run_mutation_outcome(
      create_outcome.store,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/1\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/1\",\"userErrors\":[]}}}"
}

pub fn cart_transform_delete_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/missing\") { deletedId userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Could not find cart transform with id: gid://shopify/CartTransform/missing\",\"code\":\"NOT_FOUND\"}]}}}"
}

pub fn cart_transform_delete_bare_id_returns_canonical_deleted_id_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/current", "current-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/01ABC",
      title: Some("Doomed cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"01ABC\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/01ABC\",\"userErrors\":[]}}}"
  let assert None =
    store.get_effective_cart_transform_by_id(
      outcome.store,
      "gid://shopify/CartTransform/01ABC",
    )
  assert outcome.staged_resource_ids == ["gid://shopify/CartTransform/01ABC"]
}

pub fn cart_transform_delete_cross_app_function_emits_unauthorized_scope_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn_with_app_key(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      "other-app-key",
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/77",
      title: Some("Other app cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
  let s =
    s
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/77\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"The app is not authorized to access this Function resource.\",\"code\":\"UNAUTHORIZED_APP_SCOPE\"}]}}}"
  let assert Some(_) =
    store.get_effective_cart_transform_by_id(
      outcome.store,
      "gid://shopify/CartTransform/77",
    )
  assert outcome.staged_resource_ids == []
}

pub fn cart_transform_delete_same_app_function_owner_succeeds_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/current", "current-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/same-app",
      title: Some("Same app cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/same-app\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/same-app\",\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/CartTransform/same-app"]
}

pub fn cart_transform_delete_missing_function_owner_emits_unauthorized_scope_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/missing-owner",
      title: Some("Missing owner cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/missing-owner\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"The app is not authorized to access this Function resource.\",\"code\":\"UNAUTHORIZED_APP_SCOPE\"}]}}}"
  let assert Some(_) =
    store.get_effective_cart_transform_by_id(
      outcome.store,
      "gid://shopify/CartTransform/missing-owner",
    )
  assert outcome.staged_resource_ids == []
}

pub fn cart_transform_delete_missing_current_installation_emits_unauthorized_scope_test() {
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/current", "current-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/missing-installation",
      title: Some("Missing installation cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/missing-installation\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"The app is not authorized to access this Function resource.\",\"code\":\"UNAUTHORIZED_APP_SCOPE\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn cart_transform_delete_owner_app_key_fallback_succeeds_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app_without_id("current-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/app-key-owner",
      title: Some("App key owner cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/app-key-owner\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/app-key-owner\",\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == ["gid://shopify/CartTransform/app-key-owner"]
}

pub fn cart_transform_delete_all_cart_transforms_scope_allows_cross_app_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation_with_scopes(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
      [access_scope("all_cart_transforms")],
    )
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/other", "other-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/all-scope",
      title: Some("Cross app cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/all-scope\") { deletedId userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/all-scope\",\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == ["gid://shopify/CartTransform/all-scope"]
}

pub fn cart_transform_delete_internal_visibility_allows_cross_app_test() {
  let current_app = app("gid://shopify/App/current", "current-app-key")
  let current_installation =
    installation(
      "gid://shopify/AppInstallation/current",
      "gid://shopify/App/current",
    )
  let fn_record =
    shopify_fn_with_app(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
      function_app("gid://shopify/App/other", "other-app-key"),
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/internal-visibility",
      title: Some("Cross app cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
      metafields: [],
      created_at: None,
      updated_at: None,
    )
  let s =
    store.upsert_base_app_installation(
      store.new(),
      current_installation,
      current_app,
    )
    |> seed_function(fn_record)
    |> seed_cart_transform(cart_transform)
  let outcome =
    run_mutation_outcome_with_headers(
      s,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/internal-visibility\") { deletedId userErrors { field message code } } }",
      dict.from_list([#(app_identity.internal_visibility_header, "true")]),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":\"gid://shopify/CartTransform/internal-visibility\",\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == [
      "gid://shopify/CartTransform/internal-visibility",
    ]
}

// ----------- taxAppConfigure -----------

pub fn tax_app_configure_ready_true_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { taxAppConfigure(ready: true) { taxAppConfiguration { id ready state updatedAt } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"taxAppConfigure\":{\"taxAppConfiguration\":{\"id\":\"gid://shopify/TaxAppConfiguration/local\",\"ready\":true,\"state\":\"READY\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  // Singleton stored.
  let assert Some(record) =
    store.get_effective_tax_app_configuration(outcome.store)
  assert record.ready == True
  assert record.state == "READY"
}

pub fn tax_app_configure_ready_false_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { taxAppConfigure(ready: false) { taxAppConfiguration { ready state } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"taxAppConfigure\":{\"taxAppConfiguration\":{\"ready\":false,\"state\":\"NOT_READY\"},\"userErrors\":[]}}}"
}

pub fn tax_app_configure_missing_ready_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { taxAppConfigure { taxAppConfiguration { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"taxAppConfigure\":{\"taxAppConfiguration\":null,\"userErrors\":[{\"field\":[\"ready\"],\"message\":\"Ready must be true or false\",\"code\":\"INVALID\"}]}}}"
}

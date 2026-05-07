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
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppRecord, type CartTransformRecord,
  type ShopifyFunctionRecord, type ValidationRecord, AccessScopeRecord,
  AppInstallationRecord, AppRecord, CartTransformRecord, ShopifyFunctionRecord,
  ValidationRecord,
}

// ----------- Helpers -----------

fn run_mutation_outcome(
  store_in: store.Store,
  document: String,
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
      empty_upstream_context(),
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

fn app(id: String, api_key: String) -> AppRecord {
  AppRecord(
    id: id,
    api_key: Some(api_key),
    handle: Some("test-app"),
    title: Some("Test app"),
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
      "mutation { validationCreate(validation: { functionHandle: \"checkout-validator\", title: \"My validator\" }) { validation { id title enable enabled blockOnFailure functionHandle shopifyFunction { id handle apiType } createdAt updatedAt } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  // Validation gid: synthetic #1. ShopifyFunction metadata is reused from state.
  // Timestamp: 2024-01-01T00:00:00.000Z (first synthetic timestamp).
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"title\":\"My validator\",\"enable\":false,\"enabled\":false,\"blockOnFailure\":false,\"functionHandle\":\"checkout-validator\",\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"handle\":\"checkout-validator\",\"apiType\":\"VALIDATION\"},\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Validation/1"]
  // The staged validation is visible and the referenced Function metadata is preserved.
  let assert Some(_) =
    store.get_effective_validation_by_id(
      outcome.store,
      "gid://shopify/Validation/1",
    )
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
      "mutation { omitted: validationCreate(validation: { functionHandle: \"conformance-validation\" }) { validation { id title } userErrors { field } } explicitNull: validationCreate(validation: { functionHandle: \"conformance-validation\", title: null }) { validation { id title } userErrors { field } } emptyString: validationCreate(validation: { functionHandle: \"conformance-validation\", title: \"\" }) { validation { id title } userErrors { field } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"omitted\":{\"validation\":{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\"},\"userErrors\":[]},\"explicitNull\":{\"validation\":{\"id\":\"gid://shopify/Validation/3\",\"title\":\"Function conformance-validation\"},\"userErrors\":[]},\"emptyString\":{\"validation\":{\"id\":\"gid://shopify/Validation/4\",\"title\":\"\"},\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    functions.handle_function_query(
      outcome.store,
      "{ validation(id: \"gid://shopify/Validation/2\") { id title } validations(first: 5) { nodes { id title } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"validation\":{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\"},\"validations\":{\"nodes\":[{\"id\":\"gid://shopify/Validation/2\",\"title\":\"Function conformance-validation\"},{\"id\":\"gid://shopify/Validation/3\",\"title\":\"Function conformance-validation\"},{\"id\":\"gid://shopify/Validation/4\",\"title\":\"\"}]}}"
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
      "mutation { cartTransformCreate(cartTransform: { functionHandle: \"cart-transformer\", title: \"My transformer\" }) { cartTransform { id title functionHandle blockOnFailure } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\",\"title\":\"My transformer\",\"functionHandle\":\"cart-transformer\",\"blockOnFailure\":false},\"userErrors\":[]}}}"
}

pub fn cart_transform_create_falls_back_to_top_level_args_test() {
  // TS quirk: cartTransformCreate accepts either nested input (cartTransform: {...})
  // or top-level args (functionHandle directly).
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
      "mutation { cartTransformCreate(functionHandle: \"cart-transformer\") { cartTransform { id functionHandle } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":{\"id\":\"gid://shopify/CartTransform/1\",\"functionHandle\":\"cart-transformer\"},\"userErrors\":[]}}}"
}

pub fn cart_transform_create_missing_function_emits_user_error_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { cartTransformCreate(cartTransform: { title: \"No function\" }) { cartTransform { id } userErrors { field message code } } }",
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
      "mutation { cartTransformCreate(cartTransform: { functionId: \"gid://shopify/ShopifyFunction/cart-transformer\", functionHandle: \"cart-transformer\" }) { cartTransform { id } userErrors { field message code } } }",
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
      "mutation { cartTransformCreate(cartTransform: { functionId: \"gid://shopify/ShopifyFunction/missing\" }) { cartTransform { id } userErrors { field message code } } }",
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
      "mutation { cartTransformCreate(cartTransform: { functionId: \"gid://shopify/ShopifyFunction/checkout-validator\" }) { cartTransform { id } userErrors { field message code } } }",
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
      "mutation { cartTransformCreate(cartTransform: { functionHandle: \"checkout-validator\" }) { cartTransform { id } userErrors { field message code } } }",
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
      "mutation { cartTransformCreate(cartTransform: { functionId: \"gid://shopify/ShopifyFunction/cart-transformer\" }) { cartTransform { id } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Could not enable cart transform because it is already registered\",\"code\":\"FUNCTION_ALREADY_REGISTERED\"}]}}}"
  let assert [_] = store.list_effective_cart_transforms(outcome.store)
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
  assert list.is_empty(store.get_log(outcome.store))
}

// ----------- cartTransformDelete -----------

pub fn cart_transform_delete_removes_record_test() {
  // Pre-stage by minting via create.
  let s =
    seed_function(
      store.new(),
      shopify_fn(
        "gid://shopify/ShopifyFunction/cart-transformer",
        "cart-transformer",
        "CART_TRANSFORM",
      ),
    )
  let create_outcome =
    run_mutation_outcome(
      s,
      "mutation { cartTransformCreate(cartTransform: { functionHandle: \"cart-transformer\" }) { cartTransform { id } } }",
    )
  let body =
    run_mutation(
      create_outcome.store,
      "mutation { cartTransformDelete(id: \"gid://shopify/CartTransform/1\") { deletedId userErrors { field } } }",
    )
  assert body
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
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let cart_transform =
    CartTransformRecord(
      id: "gid://shopify/CartTransform/01ABC",
      title: Some("Doomed cart transform"),
      block_on_failure: Some(False),
      function_id: None,
      function_handle: Some("cart-transformer"),
      shopify_function_id: Some(fn_record.id),
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

//// Mutation-path tests for `proxy/functions`.
////
//// Covers all 6 mutation roots (`validationCreate`/`Update`/`Delete`,
//// `cartTransformCreate`/`Delete`, `taxAppConfigure`) plus the
//// `is_function_mutation_root` predicate, the `process_mutation`
//// `{"data": …}` envelope, and the `ensure_shopify_function`
//// reuse-vs-mint behavior.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionRecord, type ValidationRecord,
  CartTransformRecord, ShopifyFunctionRecord, ValidationRecord,
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

fn seed_function(
  store_in: store.Store,
  record: ShopifyFunctionRecord,
) -> store.Store {
  let #(_, s) = store.upsert_staged_shopify_function(store_in, record)
  s
}

fn seed_validation(
  store_in: store.Store,
  record: ValidationRecord,
) -> store.Store {
  let #(_, s) = store.upsert_staged_validation(store_in, record)
  s
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
  let body =
    run_mutation(
      store.new(),
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
  assert entry.status == store.Staged
  assert entry.staged_resource_ids
    == [
      "gid://shopify/TaxAppConfiguration/local",
    ]
  assert entry.interpreted.capability.domain == "functions"
  assert entry.interpreted.capability.execution == "stage-locally"
}

// ----------- validationCreate -----------

pub fn validation_create_with_handle_mints_records_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { validationCreate(validation: { functionHandle: \"checkout-validator\", title: \"My validator\" }) { validation { id title enable enabled blockOnFailure functionHandle shopifyFunction { id handle apiType } createdAt updatedAt } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  // Validation gid: synthetic #1. ShopifyFunction id derived from handle.
  // Timestamp: 2024-01-01T00:00:00.000Z (first synthetic timestamp).
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"id\":\"gid://shopify/Validation/1\",\"title\":\"My validator\",\"enable\":true,\"enabled\":true,\"blockOnFailure\":false,\"functionHandle\":\"checkout-validator\",\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"handle\":\"checkout-validator\",\"apiType\":\"VALIDATION\"},\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Validation/1"]
  // Both records ended up in the store.
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
    == "{\"data\":{\"validationCreate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"validation\",\"functionHandle\"],\"message\":\"Function handle or function ID must be provided\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\"}]}}}"
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
  let body =
    run_mutation(
      store.new(),
      "mutation { validationCreate(validation: { functionHandle: \"v\" }) { validation { enable blockOnFailure } } }",
    )
  // enable defaults to true, blockOnFailure defaults to false.
  assert body
    == "{\"data\":{\"validationCreate\":{\"validation\":{\"enable\":true,\"blockOnFailure\":false}}}}"
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

pub fn validation_update_unknown_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { validationUpdate(id: \"gid://shopify/Validation/missing\", validation: { title: \"x\" }) { validation { id } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"validationUpdate\":{\"validation\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"No function-backed resource exists with id gid://shopify/Validation/missing\",\"code\":\"NOT_FOUND\"}]}}}"
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
    == "{\"data\":{\"validationDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"No function-backed resource exists with id gid://shopify/Validation/missing\",\"code\":\"NOT_FOUND\"}]}}}"
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

pub fn cart_transform_create_rejects_non_cart_transform_function_test() {
  let s =
    seed_function(
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
    == "{\"data\":{\"cartTransformCreate\":{\"cartTransform\":null,\"userErrors\":[{\"field\":[\"functionId\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].\",\"code\":\"FUNCTION_DOES_NOT_IMPLEMENT\"}]}}}"
  assert list.is_empty(store.list_effective_cart_transforms(outcome.store))
  let assert [_] = store.list_effective_shopify_functions(outcome.store)
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
    == "{\"data\":{\"cartTransformDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"No function-backed resource exists with id gid://shopify/CartTransform/missing\",\"code\":\"NOT_FOUND\"}]}}}"
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

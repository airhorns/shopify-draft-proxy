//// Read-path tests for `proxy/functions`.
////
//// Covers the five query roots (`validation`, `validations`,
//// `cartTransforms`, `shopifyFunction`, `shopifyFunctions`) plus
//// the `is_function_query_root` predicate, the per-record
//// projections, the `enable`/`enabled` aliasing on Validation, the
//// `apiType` filter on `shopifyFunctions`, and the embedded
//// `shopifyFunction` rehydration on Validation.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionRecord, type ValidationRecord,
  CartTransformRecord, ShopifyFunctionRecord, ValidationRecord,
}

// ----------- Helpers -----------

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

fn validation(id: String, fn_id: String) -> ValidationRecord {
  ValidationRecord(
    id: id,
    title: Some("Some validation"),
    enable: Some(True),
    block_on_failure: Some(False),
    function_id: None,
    function_handle: Some("checkout-validator"),
    shopify_function_id: Some(fn_id),
    metafields: [],
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-02T00:00:00.000Z"),
  )
}

fn cart_transform(id: String, fn_id: String) -> CartTransformRecord {
  CartTransformRecord(
    id: id,
    title: Some("Cart transformer"),
    block_on_failure: Some(False),
    function_id: None,
    function_handle: Some("cart-transformer"),
    shopify_function_id: Some(fn_id),
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-02T00:00:00.000Z"),
  )
}

fn run(store_in: store.Store, query: String) -> String {
  let assert Ok(data) =
    functions.handle_function_query(store_in, query, dict.new())
  json.to_string(data)
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

// ----------- is_function_query_root -----------

pub fn is_function_query_root_test() {
  assert functions.is_function_query_root("validation")
  assert functions.is_function_query_root("validations")
  assert functions.is_function_query_root("cartTransforms")
  assert functions.is_function_query_root("shopifyFunction")
  assert functions.is_function_query_root("shopifyFunctions")
  assert !functions.is_function_query_root("app")
  assert !functions.is_function_query_root("validationCreate")
}

// ----------- validation(id:) -----------

pub fn validation_by_id_returns_record_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v = validation("gid://shopify/Validation/1", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let result =
    run(
      s,
      "{ validation(id: \"gid://shopify/Validation/1\") { __typename id title blockOnFailure } }",
    )
  assert result
    == "{\"validation\":{\"__typename\":\"Validation\",\"id\":\"gid://shopify/Validation/1\",\"title\":\"Some validation\",\"blockOnFailure\":false}}"
}

pub fn validation_by_id_missing_returns_null_test() {
  let result =
    run(
      store.new(),
      "{ validation(id: \"gid://shopify/Validation/missing\") { id } }",
    )
  assert result == "{\"validation\":null}"
}

pub fn validation_by_id_missing_argument_returns_null_test() {
  let result = run(store.new(), "{ validation { id } }")
  assert result == "{\"validation\":null}"
}

pub fn validation_enable_and_enabled_alias_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v = validation("gid://shopify/Validation/2", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let result =
    run(
      s,
      "{ validation(id: \"gid://shopify/Validation/2\") { enable enabled } }",
    )
  // Both fields project the same boolean — the proxy aliases `enabled`.
  assert result == "{\"validation\":{\"enable\":true,\"enabled\":true}}"
}

pub fn validation_embedded_shopify_function_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v = validation("gid://shopify/Validation/3", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let result =
    run(
      s,
      "{ validation(id: \"gid://shopify/Validation/3\") { shopifyFunction { id handle apiType } } }",
    )
  assert result
    == "{\"validation\":{\"shopifyFunction\":{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"handle\":\"checkout-validator\",\"apiType\":\"VALIDATION\"}}}"
}

pub fn validation_function_id_falls_back_to_shopify_function_id_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  // function_id is None in the helper — functionId should fall back to
  // shopify_function_id.
  let v = validation("gid://shopify/Validation/4", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v)
  let result =
    run(
      s,
      "{ validation(id: \"gid://shopify/Validation/4\") { functionId functionHandle } }",
    )
  assert result
    == "{\"validation\":{\"functionId\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"functionHandle\":\"checkout-validator\"}}"
}

// ----------- validations connection -----------

pub fn validations_connection_empty_test() {
  let result = run(store.new(), "{ validations(first: 5) { nodes { id } } }")
  assert result == "{\"validations\":{\"nodes\":[]}}"
}

pub fn validations_connection_returns_seeded_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let v1 = validation("gid://shopify/Validation/10", fn_record.id)
  let v2 = validation("gid://shopify/Validation/11", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_validation(v1)
    |> seed_validation(v2)
  let result = run(s, "{ validations(first: 5) { nodes { id title } } }")
  assert result
    == "{\"validations\":{\"nodes\":[{\"id\":\"gid://shopify/Validation/10\",\"title\":\"Some validation\"},{\"id\":\"gid://shopify/Validation/11\",\"title\":\"Some validation\"}]}}"
}

// ----------- cartTransforms connection -----------

pub fn cart_transforms_connection_empty_test() {
  let result = run(store.new(), "{ cartTransforms(first: 5) { nodes { id } } }")
  assert result == "{\"cartTransforms\":{\"nodes\":[]}}"
}

pub fn cart_transforms_connection_returns_seeded_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let ct = cart_transform("gid://shopify/CartTransform/20", fn_record.id)
  let s =
    store.new()
    |> seed_function(fn_record)
    |> seed_cart_transform(ct)
  let result =
    run(
      s,
      "{ cartTransforms(first: 5) { nodes { __typename id title functionHandle } } }",
    )
  assert result
    == "{\"cartTransforms\":{\"nodes\":[{\"__typename\":\"CartTransform\",\"id\":\"gid://shopify/CartTransform/20\",\"title\":\"Cart transformer\",\"functionHandle\":\"cart-transformer\"}]}}"
}

// ----------- shopifyFunction(id:) -----------

pub fn shopify_function_by_id_returns_record_test() {
  let fn_record =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let s = seed_function(store.new(), fn_record)
  let result =
    run(
      s,
      "{ shopifyFunction(id: \"gid://shopify/ShopifyFunction/checkout-validator\") { __typename id title handle apiType app appKey } }",
    )
  // `app` projects to null because the proxy never mints app metadata
  // locally — see the comment on `ShopifyFunctionRecord` in state/types.
  assert result
    == "{\"shopifyFunction\":{\"__typename\":\"ShopifyFunction\",\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"title\":\"Function checkout-validator\",\"handle\":\"checkout-validator\",\"apiType\":\"VALIDATION\",\"app\":null,\"appKey\":null}}"
}

pub fn shopify_function_by_id_missing_returns_null_test() {
  let result =
    run(
      store.new(),
      "{ shopifyFunction(id: \"gid://shopify/ShopifyFunction/missing\") { id } }",
    )
  assert result == "{\"shopifyFunction\":null}"
}

// ----------- shopifyFunctions connection + apiType filter -----------

pub fn shopify_functions_connection_empty_test() {
  let result =
    run(store.new(), "{ shopifyFunctions(first: 5) { nodes { id } } }")
  assert result == "{\"shopifyFunctions\":{\"nodes\":[]}}"
}

pub fn shopify_functions_connection_returns_all_test() {
  let v_fn =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let ct_fn =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let s =
    store.new()
    |> seed_function(v_fn)
    |> seed_function(ct_fn)
  let result = run(s, "{ shopifyFunctions(first: 5) { nodes { id apiType } } }")
  assert result
    == "{\"shopifyFunctions\":{\"nodes\":[{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"apiType\":\"VALIDATION\"},{\"id\":\"gid://shopify/ShopifyFunction/cart-transformer\",\"apiType\":\"CART_TRANSFORM\"}]}}"
}

pub fn shopify_functions_connection_filters_by_api_type_test() {
  let v_fn =
    shopify_fn(
      "gid://shopify/ShopifyFunction/checkout-validator",
      "checkout-validator",
      "VALIDATION",
    )
  let ct_fn =
    shopify_fn(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      "CART_TRANSFORM",
    )
  let s =
    store.new()
    |> seed_function(v_fn)
    |> seed_function(ct_fn)
  let result =
    run(
      s,
      "{ shopifyFunctions(first: 5, apiType: \"VALIDATION\") { nodes { id apiType } } }",
    )
  assert result
    == "{\"shopifyFunctions\":{\"nodes\":[{\"id\":\"gid://shopify/ShopifyFunction/checkout-validator\",\"apiType\":\"VALIDATION\"}]}}"
}

// ----------- helpers -----------

pub fn normalize_function_handle_basic_test() {
  assert functions.normalize_function_handle("Checkout Validator")
    == "checkout-validator"
  assert functions.normalize_function_handle("  Trim Me  ") == "trim-me"
  assert functions.normalize_function_handle("foo!@#bar") == "foo-bar"
  assert functions.normalize_function_handle("") == "local-function"
  assert functions.normalize_function_handle("---") == "local-function"
}

pub fn shopify_function_id_from_handle_test() {
  assert functions.shopify_function_id_from_handle("Checkout Validator")
    == "gid://shopify/ShopifyFunction/checkout-validator"
}

pub fn title_from_handle_test() {
  assert functions.title_from_handle("checkout-validator")
    == "Checkout Validator"
  assert functions.title_from_handle("snake_case_handle") == "Snake Case Handle"
  assert functions.title_from_handle("Spaces  In   Between")
    == "Spaces In Between"
}

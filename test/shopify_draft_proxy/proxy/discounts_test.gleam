//// Focused mutation/read tests for `proxy/discounts`.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/proxy_state.{type Request, Request, Response}
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountRecord, type SavedSearchRecord,
  type ShopifyFunctionRecord, CapturedNull, CapturedObject, CapturedString,
  DiscountRecord, SavedSearchRecord, ShopifyFunctionRecord,
}

fn run_mutation(document: String) -> mutation_helpers.MutationOutcome {
  run_mutation_from(store.new(), synthetic_identity.new(), document)
}

fn subscription_store() -> store.Store {
  store.set_shop_sells_subscriptions(store.new(), True)
}

fn discount_function_store() -> store.Store {
  store.upsert_base_shopify_functions(store.new(), [
    shopify_function(
      "gid://shopify/ShopifyFunction/discount-local",
      "discount-local",
      Some("DISCOUNT"),
    ),
  ])
}

fn mismatched_function_store() -> store.Store {
  store.upsert_base_shopify_functions(store.new(), [
    shopify_function(
      "gid://shopify/ShopifyFunction/cart-transformer",
      "cart-transformer",
      Some("CART_TRANSFORM"),
    ),
  ])
}

fn shopify_function(
  id: String,
  handle: String,
  api_type: Option(String),
) -> ShopifyFunctionRecord {
  ShopifyFunctionRecord(
    id: id,
    title: Some(handle <> " title"),
    handle: Some(handle),
    api_type: api_type,
    description: Some(handle <> " description"),
    app_key: Some("app-key"),
    app: None,
  )
}

fn run_mutation_from(
  store: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  document: String,
) -> mutation_helpers.MutationOutcome {
  discounts.process_mutation(
    store,
    identity,
    "/admin/api/2026-04/graphql.json",
    document,
    dict.new(),
    empty_upstream_context(),
  )
}

fn graphql_request_body(body: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2026-04/graphql.json",
    headers: dict.new(),
    body: body,
  )
}

fn price_rule_saved_search(id: String) -> SavedSearchRecord {
  SavedSearchRecord(
    id: id,
    legacy_resource_id: "98765",
    name: "Price rule search",
    query: "status:active",
    resource_type: "PRICE_RULE",
    search_terms: "",
    filters: [],
    cursor: None,
  )
}

fn redeem_code_saved_search(id: String, query: String) -> SavedSearchRecord {
  SavedSearchRecord(
    id: id,
    legacy_resource_id: "67890",
    name: "Redeem code search",
    query: query,
    resource_type: "DISCOUNT_REDEEM_CODE",
    search_terms: "",
    filters: [],
    cursor: None,
  )
}

pub fn bulk_selector_validation_matches_captured_code_roots_test() {
  let outcome =
    run_mutation(
      "mutation { activateMissing: discountCodeBulkActivate { userErrors { field message code extraInfo } } activateBlank: discountCodeBulkActivate(search: \"\") { userErrors { field message code extraInfo } } activateSaved: discountCodeBulkActivate(savedSearchId: \"gid://shopify/SavedSearch/0\") { userErrors { field message code extraInfo } } deactivateTooMany: discountCodeBulkDeactivate(ids: [\"gid://shopify/DiscountCodeNode/0\"], search: \"status:active\") { userErrors { field message code extraInfo } } deleteMissing: discountCodeBulkDelete { userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"activateMissing\":{\"userErrors\":[{\"field\":null,\"message\":\"Missing expected argument key: 'ids', 'search' or 'saved_search_id'.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]},\"activateBlank\":{\"userErrors\":[{\"field\":[\"search\"],\"message\":\"'Search' can't be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]},\"activateSaved\":{\"userErrors\":[{\"field\":[\"savedSearchId\"],\"message\":\"Invalid 'saved_search_id'.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"deactivateTooMany\":{\"userErrors\":[{\"field\":null,\"message\":\"Only one of 'ids', 'search' or 'saved_search_id' is allowed.\",\"code\":\"TOO_MANY_ARGUMENTS\",\"extraInfo\":null}]},\"deleteMissing\":{\"userErrors\":[{\"field\":null,\"message\":\"Missing expected argument key: 'ids', 'search' or 'saved_search_id'.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]}}}"
}

pub fn customer_gets_value_bounds_match_captured_basic_create_test() {
  let outcome =
    run_mutation(
      "mutation { percentageHigh: discountCodeBasicCreate(basicCodeDiscount: { title: \"Too high\", code: \"TOOHIGH\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 1.5 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } percentageNegative: discountCodeBasicCreate(basicCodeDiscount: { title: \"Negative percentage\", code: \"NEGATIVEPCT\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: -0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } amountNegative: discountCodeBasicCreate(basicCodeDiscount: { title: \"Negative amount\", code: \"NEGAMT\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountAmount: { amount: \"-5\", appliesOnEachItem: false } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } percentageZero: discountCodeBasicCreate(basicCodeDiscount: { title: \"Zero percentage\", code: \"ZEROPCT\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } amountZero: discountCodeBasicCreate(basicCodeDiscount: { title: \"Zero amount\", code: \"ZEROAMT\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountAmount: { amount: \"0\", appliesOnEachItem: false } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let rendered = json.to_string(outcome.data)

  assert string.contains(
    rendered,
    "\"percentageHigh\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Value must be between 0.0 and 1.0\",\"code\":\"VALUE_OUTSIDE_RANGE\",\"extraInfo\":null}]}",
  )
  assert string.contains(
    rendered,
    "\"percentageNegative\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Value must be between 0.0 and 1.0\",\"code\":\"VALUE_OUTSIDE_RANGE\",\"extraInfo\":null}]}",
  )
  assert string.contains(
    rendered,
    "\"amountNegative\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"discountAmount\",\"amount\"],\"message\":\"Value must be less than or equal to 0\",\"code\":\"LESS_THAN_OR_EQUAL_TO\",\"extraInfo\":null}]}",
  )
  assert string.contains(
    rendered,
    "\"percentageZero\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/",
  )
  assert string.contains(rendered, "\"percentageZero\":")
  assert string.contains(rendered, "\"amountZero\":")
  assert list.contains(
    outcome.staged_resource_ids,
    "gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic",
  )
  assert list.contains(
    outcome.staged_resource_ids,
    "gid://shopify/DiscountCodeNode/3?shopify-draft-proxy=synthetic",
  )
}

pub fn customer_gets_value_bounds_apply_to_update_and_automatic_basic_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Valid\", code: \"VALIDBOUNDS\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let updated =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Invalid update\", code: \"VALIDBOUNDS\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 1.5 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Invalid automatic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountAmount: { amount: \"-5\", appliesOnEachItem: false } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_created =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Valid automatic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountAmount: { amount: \"5\", appliesOnEachItem: false } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_updated =
    run_mutation_from(
      automatic_created.store,
      automatic_created.identity,
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Invalid automatic update\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: -0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(updated.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Value must be between 0.0 and 1.0\",\"code\":\"VALUE_OUTSIDE_RANGE\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic.data)
    == "{\"data\":{\"discountAutomaticBasicCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"customerGets\",\"value\",\"discountAmount\",\"amount\"],\"message\":\"Value must be less than or equal to 0\",\"code\":\"LESS_THAN_OR_EQUAL_TO\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic_updated.data)
    == "{\"data\":{\"discountAutomaticBasicUpdate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Value must be between 0.0 and 1.0\",\"code\":\"VALUE_OUTSIDE_RANGE\",\"extraInfo\":null}]}}}"
}

pub fn discount_amount_non_numeric_variable_is_graphql_coercion_error_test() {
  let body =
    "{\"query\":\"mutation DiscountValueBoundsNonNumeric($input: DiscountCodeBasicInput!) { discountCodeBasicCreate(basicCodeDiscount: $input) { codeDiscountNode { id } userErrors { field message code extraInfo } } }\",\"variables\":{\"input\":{\"title\":\"Non numeric\",\"code\":\"NONNUMERIC\",\"startsAt\":\"2026-04-25T00:00:00Z\",\"customerGets\":{\"value\":{\"discountAmount\":{\"amount\":\"abc\",\"appliesOnEachItem\":false}},\"items\":{\"all\":true}}}}}"
  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request_body(body))
  let rendered = json.to_string(response_body)

  assert status == 200
  assert string.contains(rendered, "\"errors\":[")
  assert string.contains(rendered, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    rendered,
    "Variable $input of type DiscountCodeBasicInput! was provided invalid value for customerGets.value.discountAmount.amount (invalid decimal 'abc')",
  )
  assert string.contains(
    rendered,
    "\"path\":[\"customerGets\",\"value\",\"discountAmount\",\"amount\"]",
  )
  assert string.contains(rendered, "\"message\":\"invalid decimal 'abc'\"")
  assert store.get_log(next_proxy.store) == []
}

pub fn discount_allocation_method_reflects_value_and_class_test() {
  let order_percentage =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Order percentage\", code: \"ORDERPCTALLOC\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { allocationMethod } } } userErrors { message } } }",
    )
  let product_percentage =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Product percentage\", code: \"PRODUCTPCTALLOC\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { allocationMethod } } } userErrors { message } } }",
    )
  let product_amount =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Product amount\", code: \"PRODUCTAMTALLOC\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountAmount: { amount: \"5\", appliesOnEachItem: false } }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { allocationMethod } } } userErrors { message } } }",
    )

  assert json.to_string(order_percentage.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"allocationMethod\":\"ACROSS\"}},\"userErrors\":[]}}}"
  assert json.to_string(product_percentage.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"allocationMethod\":\"EACH\"}},\"userErrors\":[]}}}"
  assert json.to_string(product_amount.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"allocationMethod\":\"ACROSS\"}},\"userErrors\":[]}}}"
}

pub fn bulk_selector_validation_matches_captured_automatic_delete_test() {
  let outcome =
    run_mutation(
      "mutation { missing: discountAutomaticBulkDelete { userErrors { field message code extraInfo } } blank: discountAutomaticBulkDelete(search: \"\") { userErrors { field message code extraInfo } } tooMany: discountAutomaticBulkDelete(ids: [\"gid://shopify/DiscountAutomaticNode/0\"], search: \"status:active\") { userErrors { field message code extraInfo } } saved: discountAutomaticBulkDelete(savedSearchId: \"gid://shopify/SavedSearch/0\") { userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"missing\":{\"userErrors\":[{\"field\":null,\"message\":\"One of IDs, search argument or saved search ID is required.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]},\"blank\":{\"userErrors\":[]},\"tooMany\":{\"userErrors\":[{\"field\":null,\"message\":\"Only one of IDs, search argument or saved search ID is allowed.\",\"code\":\"TOO_MANY_ARGUMENTS\",\"extraInfo\":null}]},\"saved\":{\"userErrors\":[{\"field\":[\"savedSearchId\"],\"message\":\"Invalid savedSearchId.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn bulk_selector_validation_accepts_known_price_rule_saved_search_test() {
  let base_store =
    store.upsert_base_saved_searches(store.new(), [
      price_rule_saved_search("gid://shopify/SavedSearch/98765"),
    ])
  let outcome =
    run_mutation_from(
      base_store,
      synthetic_identity.new(),
      "mutation { discountCodeBulkActivate(savedSearchId: \"gid://shopify/SavedSearch/98765\") { userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBulkActivate\":{\"userErrors\":[]}}}"
}

pub fn redeem_code_bulk_delete_validation_matches_captured_shopify_order_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk\", code: \"BULK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let outcome =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { missing: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { job { done } userErrors { field message code extraInfo } } tooMany: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", ids: [\"gid://shopify/DiscountRedeemCode/2?shopify-draft-proxy=synthetic\"], search: \"code:BULK\") { job { done } userErrors { field message code extraInfo } } unknownDiscount: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/0\", ids: [\"gid://shopify/DiscountRedeemCode/2?shopify-draft-proxy=synthetic\"]) { job { done } userErrors { field message code extraInfo } } emptyIds: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", ids: []) { job { done } userErrors { field message code extraInfo } } blankSearch: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", search: \"   \") { job { done } userErrors { field message code extraInfo } } invalidSavedSearch: discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", savedSearchId: \"gid://shopify/SavedSearch/0\") { job { done } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"missing\":{\"job\":null,\"userErrors\":[{\"field\":null,\"message\":\"Missing expected argument key: 'ids', 'search' or 'saved_search_id'.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]},\"tooMany\":{\"job\":null,\"userErrors\":[{\"field\":null,\"message\":\"Only one of 'ids', 'search' or 'saved_search_id' is allowed.\",\"code\":\"TOO_MANY_ARGUMENTS\",\"extraInfo\":null}]},\"unknownDiscount\":{\"job\":null,\"userErrors\":[{\"field\":[\"discountId\"],\"message\":\"Code discount does not exist.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"emptyIds\":{\"job\":null,\"userErrors\":[{\"field\":null,\"message\":\"Something went wrong, please try again.\",\"code\":null,\"extraInfo\":null}]},\"blankSearch\":{\"job\":null,\"userErrors\":[{\"field\":[\"search\"],\"message\":\"'Search' can't be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]},\"invalidSavedSearch\":{\"job\":null,\"userErrors\":[{\"field\":[\"savedSearchId\"],\"message\":\"Invalid 'saved_search_id'.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn redeem_code_bulk_delete_saved_search_selector_removes_matching_codes_test() {
  let base_store =
    store.upsert_base_saved_searches(store.new(), [
      redeem_code_saved_search("gid://shopify/SavedSearch/67890", "code:BULK"),
    ])
  let created =
    run_mutation_from(
      base_store,
      synthetic_identity.new(),
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk\", code: \"BULK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let added =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: [\"EXTRA\"]) { bulkCreation { codesCount } userErrors { message } } }",
    )
  let deleted =
    run_mutation_from(
      added.store,
      added.identity,
      "mutation { discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", savedSearchId: \"gid://shopify/SavedSearch/67890\") { job { done } userErrors { field message code extraInfo } } }",
    )

  let assert Ok(after_delete) =
    discounts.handle_discount_query(
      deleted.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { codes(first: 5) { nodes { code } } } } }",
      dict.new(),
    )

  assert json.to_string(deleted.data)
    == "{\"data\":{\"discountCodeRedeemCodeBulkDelete\":{\"job\":{\"done\":true},\"userErrors\":[]}}}"
  assert json.to_string(after_delete)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"codes\":{\"nodes\":[{\"code\":\"EXTRA\"}]}}}}"
}

pub fn bulk_selector_validation_keeps_unknown_ids_success_noop_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBulkActivate(ids: [\"gid://shopify/DiscountCodeNode/0\"]) { userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBulkActivate\":{\"userErrors\":[]}}}"
}

pub fn code_basic_create_is_readable_by_code_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Launch\", code: \"LAUNCH10\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { title codes(first: 1) { nodes { code } } } } userErrors { message } } }",
    )

  let assert Ok(data) =
    discounts.handle_discount_query(
      outcome.store,
      "query { codeDiscountNodeByCode(code: \"LAUNCH10\") { codeDiscount { title codes(first: 1) { nodes { code } } } } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"codeDiscountNodeByCode\":{\"codeDiscount\":{\"title\":\"Launch\",\"codes\":{\"nodes\":[{\"code\":\"LAUNCH10\"}]}}}}"
}

pub fn code_basic_status_uses_starts_and_ends_for_create_read_and_filters_test() {
  let scheduled =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Future\", code: \"FUTURE2099\", startsAt: \"2099-01-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { status } } } userErrors { message } } }",
    )
  let expired =
    run_mutation_from(
      scheduled.store,
      scheduled.identity,
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Past\", code: \"PAST2020\", startsAt: \"2019-01-01T00:00:00Z\", endsAt: \"2020-01-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { status } } } userErrors { message } } }",
    )
  let active =
    run_mutation_from(
      expired.store,
      expired.identity,
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Now\", code: \"NOW2024\", startsAt: \"2020-01-01T00:00:00Z\", endsAt: \"2099-01-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { status } } } userErrors { message } } }",
    )

  assert json.to_string(scheduled.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"status\":\"SCHEDULED\"}},\"userErrors\":[]}}}"
  assert json.to_string(expired.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"status\":\"EXPIRED\"}},\"userErrors\":[]}}}"
  assert json.to_string(active.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"status\":\"ACTIVE\"}},\"userErrors\":[]}}}"

  let assert Ok(data) =
    discounts.handle_discount_query(
      active.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { ... on DiscountCodeBasic { status } } } scheduled: discountNodes(first: 5, query: \"status:scheduled\") { nodes { id } } expiredCount: discountNodesCount(query: \"status:expired\") { count precision } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"status\":\"SCHEDULED\"}},\"scheduled\":{\"nodes\":[{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"}]},\"expiredCount\":{\"count\":1,\"precision\":\"EXACT\"}}"
}

pub fn automatic_basic_status_uses_starts_and_ends_for_create_and_read_test() {
  let outcome =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Future automatic\", startsAt: \"2099-01-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { automaticDiscount { ... on DiscountAutomaticBasic { status } } } userErrors { message } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountAutomaticBasicCreate\":{\"automaticDiscountNode\":{\"automaticDiscount\":{\"status\":\"SCHEDULED\"}},\"userErrors\":[]}}}"

  let assert Ok(data) =
    discounts.handle_discount_query(
      outcome.store,
      "query { automaticDiscountNode(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\") { automaticDiscount { ... on DiscountAutomaticBasic { status } } } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"automaticDiscountNode\":{\"automaticDiscount\":{\"status\":\"SCHEDULED\"}}}"
}

pub fn code_bulk_deactivate_preserves_status_override_on_reads_test() {
  let create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Active\", code: \"ACTIVE2024\", startsAt: \"2020-01-01T00:00:00Z\", endsAt: \"2099-01-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { ... on DiscountCodeBasic { status startsAt endsAt } } } userErrors { message } } }",
    )
  let bulk =
    run_mutation_from(
      create.store,
      create.identity,
      "mutation { discountCodeBulkDeactivate(ids: [\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"]) { job { done } userErrors { message } } }",
    )

  let assert Ok(data) =
    discounts.handle_discount_query(
      bulk.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { ... on DiscountCodeBasic { status startsAt endsAt } } } discountNodesCount(query: \"status:expired\") { count precision } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"status\":\"EXPIRED\",\"startsAt\":\"2020-01-01T00:00:00Z\",\"endsAt\":\"2099-01-01T00:00:00Z\"}},\"discountNodesCount\":{\"count\":1,\"precision\":\"EXACT\"}}"
}

pub fn code_basic_timestamps_use_synthetic_clock_and_sort_by_recency_test() {
  let first =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"First\", code: \"FIRST\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { title createdAt updatedAt } } userErrors { message } } }",
    )
  let second =
    run_mutation_from(
      first.store,
      first.identity,
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Second\", code: \"SECOND\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { title createdAt updatedAt } } userErrors { message } } }",
    )
  let updated =
    run_mutation_from(
      second.store,
      second.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"First Updated\", code: \"FIRST\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { title createdAt updatedAt } } userErrors { message } } }",
    )

  assert json.to_string(first.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\",\"codeDiscount\":{\"title\":\"First\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
  assert json.to_string(second.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/3?shopify-draft-proxy=synthetic\",\"codeDiscount\":{\"title\":\"Second\",\"createdAt\":\"2024-01-01T00:00:01.000Z\",\"updatedAt\":\"2024-01-01T00:00:01.000Z\"}},\"userErrors\":[]}}}"
  assert json.to_string(updated.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\",\"codeDiscount\":{\"title\":\"First Updated\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:02.000Z\"}},\"userErrors\":[]}}}"

  let assert Ok(data) =
    discounts.handle_discount_query(
      updated.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { title createdAt updatedAt } } discountNodes(first: 2, sortKey: UPDATED_AT, reverse: true) { nodes { id discount { title updatedAt } } } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"title\":\"First Updated\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:02.000Z\"}},\"discountNodes\":{\"nodes\":[{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\",\"discount\":{\"title\":\"First Updated\",\"updatedAt\":\"2024-01-01T00:00:02.000Z\"}},{\"id\":\"gid://shopify/DiscountCodeNode/3?shopify-draft-proxy=synthetic\",\"discount\":{\"title\":\"Second\",\"updatedAt\":\"2024-01-01T00:00:01.000Z\"}}]}}"
}

pub fn redeem_code_bulk_mutations_bump_discount_updated_at_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk\", code: \"BULK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id codeDiscount { updatedAt codes(first: 5) { nodes { id code } } } } userErrors { message } } }",
    )
  let added =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: [\"EXTRA\"]) { bulkCreation { codesCount } userErrors { message } } }",
    )

  let assert Ok(after_add) =
    discounts.handle_discount_query(
      added.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { updatedAt codes(first: 5) { nodes { code } } } } }",
      dict.new(),
    )

  let deleted =
    run_mutation_from(
      added.store,
      added.identity,
      "mutation { discountCodeRedeemCodeBulkDelete(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", ids: [\"gid://shopify/DiscountRedeemCode/2?shopify-draft-proxy=synthetic\"]) { job { done } userErrors { message } } }",
    )

  let assert Ok(after_delete) =
    discounts.handle_discount_query(
      deleted.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { updatedAt codes(first: 5) { nodes { code } } } } }",
      dict.new(),
    )

  assert string.contains(
    json.to_string(created.data),
    "\"updatedAt\":\"2024-01-01T00:00:00.000Z\"",
  )
  assert json.to_string(added.data)
    == "{\"data\":{\"discountRedeemCodeBulkAdd\":{\"bulkCreation\":{\"codesCount\":1},\"userErrors\":[]}}}"
  assert json.to_string(after_add)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"updatedAt\":\"2024-01-01T00:00:01.000Z\",\"codes\":{\"nodes\":[{\"code\":\"BULK\"},{\"code\":\"EXTRA\"}]}}}}"
  assert json.to_string(deleted.data)
    == "{\"data\":{\"discountCodeRedeemCodeBulkDelete\":{\"job\":{\"done\":true},\"userErrors\":[]}}}"
  assert json.to_string(after_delete)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"updatedAt\":\"2024-01-01T00:00:02.000Z\",\"codes\":{\"nodes\":[{\"code\":\"EXTRA\"}]}}}}"
}

pub fn redeem_code_bulk_add_rejects_unknown_empty_and_oversized_inputs_test() {
  let unknown =
    run_mutation(
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/0\", codes: [{ code: \"ABC\" }]) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }",
    )
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk\", code: \"BULK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let empty =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: []) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }",
    )
  let too_many_codes =
    string.repeat("{ code: \"MAX\" },", 250) <> "{ code: \"MAX\" }"
  let too_many =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: ["
        <> too_many_codes
        <> "]) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(unknown.data)
    == "{\"data\":{\"discountRedeemCodeBulkAdd\":{\"bulkCreation\":null,\"userErrors\":[{\"field\":[\"discountId\"],\"message\":\"Code discount does not exist.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(empty.data)
    == "{\"data\":{\"discountRedeemCodeBulkAdd\":{\"bulkCreation\":null,\"userErrors\":[{\"field\":[\"codes\"],\"message\":\"Codes can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
  assert string.contains(
    json.to_string(too_many.data),
    "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"",
  )
  assert string.contains(
    json.to_string(too_many.data),
    "The input array size of 251 is greater than the maximum allowed of 250.",
  )
}

pub fn redeem_code_bulk_add_records_per_code_failures_on_bulk_creation_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk\", code: \"BULK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let long_code = string.repeat("X", 256)
  let added =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: [{ code: \"\" }, { code: \"LINE\\nBAD\" }, { code: \"CR\\rBAD\" }, { code: \""
        <> long_code
        <> "\" }, { code: \"DUP\" }, { code: \"DUP\" }, { code: \"OK\" }]) { bulkCreation { id done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { id code } } } } userErrors { field message code extraInfo } } }",
    )
  let assert Ok(creation) =
    discounts.handle_discount_query(
      added.store,
      "query { discountRedeemCodeBulkCreation(id: \"gid://shopify/DiscountRedeemCodeBulkCreation/3?shopify-draft-proxy=synthetic\") { done codesCount importedCount failedCount codes(first: 10) { nodes { code errors { field message code extraInfo } discountRedeemCode { code } } } } }",
      dict.new(),
    )
  let assert Ok(read) =
    discounts.handle_discount_query(
      added.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { codes(first: 10) { nodes { code } } codesCount { count precision } } } byDup: codeDiscountNodeByCode(code: \"DUP\") { id } byOk: codeDiscountNodeByCode(code: \"OK\") { id } }",
      dict.new(),
    )

  assert string.contains(
    json.to_string(added.data),
    "\"bulkCreation\":{\"id\":\"gid://shopify/DiscountRedeemCodeBulkCreation/3?shopify-draft-proxy=synthetic\",\"done\":false,\"codesCount\":7,\"importedCount\":0,\"failedCount\":0",
  )
  assert json.to_string(creation)
    == "{\"discountRedeemCodeBulkCreation\":{\"done\":true,\"codesCount\":7,\"importedCount\":2,\"failedCount\":5,\"codes\":{\"nodes\":[{\"code\":\"\",\"errors\":[{\"field\":[\"code\"],\"message\":\"is too short (minimum is 1 character)\",\"code\":null,\"extraInfo\":null}],\"discountRedeemCode\":null},{\"code\":\"LINE\\nBAD\",\"errors\":[{\"field\":[\"code\"],\"message\":\"cannot contain newline characters.\",\"code\":null,\"extraInfo\":null}],\"discountRedeemCode\":null},{\"code\":\"CR\\rBAD\",\"errors\":[{\"field\":[\"code\"],\"message\":\"cannot contain newline characters.\",\"code\":null,\"extraInfo\":null}],\"discountRedeemCode\":null},{\"code\":\""
    <> long_code
    <> "\",\"errors\":[{\"field\":[\"code\"],\"message\":\"is too long (maximum is 255 characters)\",\"code\":null,\"extraInfo\":null}],\"discountRedeemCode\":null},{\"code\":\"DUP\",\"errors\":[],\"discountRedeemCode\":{\"code\":\"DUP\"}},{\"code\":\"DUP\",\"errors\":[{\"field\":[\"code\"],\"message\":\"Codes must be unique within BulkDiscountCodeCreation\",\"code\":null,\"extraInfo\":null}],\"discountRedeemCode\":null},{\"code\":\"OK\",\"errors\":[],\"discountRedeemCode\":{\"code\":\"OK\"}}]}}}"
  assert json.to_string(read)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"codes\":{\"nodes\":[{\"code\":\"BULK\"},{\"code\":\"DUP\"},{\"code\":\"OK\"}]},\"codesCount\":{\"count\":3,\"precision\":\"EXACT\"}}},\"byDup\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"byOk\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"}}"
}

pub fn code_discount_creates_reject_missing_and_blank_codes_test() {
  let missing_basic =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let blank_bxgy =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let whitespace_free_shipping =
    run_mutation(
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Free ship\", code: \"   \", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let missing_app =
    run_mutation(
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", startsAt: \"2026-04-25T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(missing_basic.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"code\"],\"message\":\"Code can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
  assert json.to_string(blank_bxgy.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"code\"],\"message\":\"Code is too short (minimum is 1 character)\",\"code\":\"TOO_SHORT\",\"extraInfo\":null}]}}}"
  assert json.to_string(whitespace_free_shipping.data)
    == "{\"data\":{\"discountCodeFreeShippingCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"freeShippingCodeDiscount\",\"code\"],\"message\":\"Code can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
  assert json.to_string(missing_app.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\",\"code\"],\"message\":\"Code can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
}

pub fn app_discount_creates_validate_function_identifiers_test() {
  let missing_code =
    run_mutation(
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let multiple_code =
    run_mutation(
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\", functionId: \"gid://shopify/ShopifyFunction/discount-local\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let unknown_code =
    run_mutation(
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\", functionId: \"gid://shopify/ShopifyFunction/missing\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let wrong_api_code =
    run_mutation_from(
      mismatched_function_store(),
      synthetic_identity.new(),
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\", functionId: \"gid://shopify/ShopifyFunction/cart-transformer\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let missing_automatic =
    run_mutation(
      "mutation { discountAutomaticAppCreate(automaticAppDiscount: { title: \"Auto\", startsAt: \"2026-04-25T00:00:00Z\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let unknown_automatic =
    run_mutation(
      "mutation { discountAutomaticAppCreate(automaticAppDiscount: { title: \"Auto\", startsAt: \"2026-04-25T00:00:00Z\", functionHandle: \"missing-discount\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(missing_code.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\",\"functionHandle\"],\"message\":\"Function id can't be blank.\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\",\"extraInfo\":null}]}}}"
  assert json.to_string(multiple_code.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\"],\"message\":\"Only one of functionId or functionHandle is allowed.\",\"code\":\"MULTIPLE_FUNCTION_IDENTIFIERS\",\"extraInfo\":null}]}}}"
  assert json.to_string(unknown_code.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\",\"functionId\"],\"message\":\"Function gid://shopify/ShopifyFunction/missing not found. Ensure that it is released in the current app (347082227713), and that the app is installed.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(wrong_api_code.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\",\"functionId\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].\",\"code\":null,\"extraInfo\":null}]}}}"
  assert json.to_string(missing_automatic.data)
    == "{\"data\":{\"discountAutomaticAppCreate\":{\"automaticAppDiscount\":null,\"userErrors\":[{\"field\":[\"automaticAppDiscount\",\"functionHandle\"],\"message\":\"Function id can't be blank.\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\",\"extraInfo\":null}]}}}"
  assert json.to_string(unknown_automatic.data)
    == "{\"data\":{\"discountAutomaticAppCreate\":{\"automaticAppDiscount\":null,\"userErrors\":[{\"field\":[\"automaticAppDiscount\",\"functionHandle\"],\"message\":\"Function missing-discount not found. Ensure that it is released in the current app (347082227713), and that the app is installed.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn app_discount_updates_validate_function_identifiers_test() {
  let code_create =
    run_mutation_from(
      discount_function_store(),
      synthetic_identity.new(),
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation_from(
      code_create.store,
      code_create.identity,
      "mutation { discountAutomaticAppCreate(automaticAppDiscount: { title: \"Auto\", startsAt: \"2026-04-25T00:00:00Z\", functionHandle: \"discount-local\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )

  let missing_code_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountCodeAppUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codeAppDiscount: { title: \"App update\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let multiple_code_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountCodeAppUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codeAppDiscount: { title: \"App update\", functionId: \"gid://shopify/ShopifyFunction/discount-local\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let unknown_automatic_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountAutomaticAppUpdate(id: \"gid://shopify/DiscountAutomaticNode/3?shopify-draft-proxy=synthetic\", automaticAppDiscount: { title: \"Auto update\", functionHandle: \"missing-discount\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )
  let wrong_api_store =
    store.upsert_base_shopify_functions(automatic_create.store, [
      shopify_function(
        "gid://shopify/ShopifyFunction/cart-transformer",
        "cart-transformer",
        Some("CART_TRANSFORM"),
      ),
    ])
  let wrong_api_automatic_update =
    run_mutation_from(
      wrong_api_store,
      automatic_create.identity,
      "mutation { discountAutomaticAppUpdate(id: \"gid://shopify/DiscountAutomaticNode/3?shopify-draft-proxy=synthetic\", automaticAppDiscount: { title: \"Auto update\", functionHandle: \"cart-transformer\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(missing_code_update.data)
    == "{\"data\":{\"discountCodeAppUpdate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\",\"functionHandle\"],\"message\":\"Function id can't be blank.\",\"code\":\"MISSING_FUNCTION_IDENTIFIER\",\"extraInfo\":null}]}}}"
  assert json.to_string(multiple_code_update.data)
    == "{\"data\":{\"discountCodeAppUpdate\":{\"codeAppDiscount\":null,\"userErrors\":[{\"field\":[\"codeAppDiscount\"],\"message\":\"Only one of functionId or functionHandle is allowed.\",\"code\":\"MULTIPLE_FUNCTION_IDENTIFIERS\",\"extraInfo\":null}]}}}"
  assert json.to_string(unknown_automatic_update.data)
    == "{\"data\":{\"discountAutomaticAppUpdate\":{\"automaticAppDiscount\":null,\"userErrors\":[{\"field\":[\"automaticAppDiscount\",\"functionHandle\"],\"message\":\"Function missing-discount not found. Ensure that it is released in the current app (347082227713), and that the app is installed.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(wrong_api_automatic_update.data)
    == "{\"data\":{\"discountAutomaticAppUpdate\":{\"automaticAppDiscount\":null,\"userErrors\":[{\"field\":[\"automaticAppDiscount\",\"functionHandle\"],\"message\":\"Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].\",\"code\":null,\"extraInfo\":null}]}}}"
}

pub fn app_discount_create_with_valid_function_still_stages_test() {
  let outcome =
    run_mutation_from(
      discount_function_store(),
      synthetic_identity.new(),
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App\", code: \"APP\", startsAt: \"2026-04-25T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId appDiscountType { functionId title } } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeAppCreate\":{\"codeAppDiscount\":{\"discountId\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\",\"appDiscountType\":{\"functionId\":\"discount-local\",\"title\":\"discount-local title\"}},\"userErrors\":[]}}}"
}

pub fn code_discount_creates_reject_code_format_constraints_test() {
  let long_code = string.repeat("x", times: 256)
  let long_basic =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Long\", code: \""
      <> long_code
      <> "\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let newline_basic =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Newline\", code: \"abc\\ndef\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(long_basic.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"code\"],\"message\":\"Code is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"extraInfo\":null}]}}}"
  assert json.to_string(newline_basic.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"code\"],\"message\":\"Code cannot contain newline characters.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn automatic_discount_creates_do_not_require_codes_test() {
  let outcome =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"automaticDiscountNode\":{\"id\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\"}",
  )
  assert string.contains(body, "\"userErrors\":[]")
}

pub fn delete_unknown_discounts_returns_invalid_user_error_test() {
  let code =
    run_mutation(
      "mutation { discountCodeDelete(id: \"gid://shopify/DiscountCodeNode/0\") { deletedCodeDiscountId userErrors { field message code } } }",
    )
  let automatic =
    run_mutation(
      "mutation { discountAutomaticDelete(id: \"gid://shopify/DiscountAutomaticNode/0\") { deletedAutomaticDiscountId userErrors { field message code } } }",
    )

  assert json.to_string(code.data)
    == "{\"data\":{\"discountCodeDelete\":{\"deletedCodeDiscountId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Code discount does not exist.\",\"code\":\"INVALID\"}]}}}"
  assert json.to_string(automatic.data)
    == "{\"data\":{\"discountAutomaticDelete\":{\"deletedAutomaticDiscountId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Automatic discount does not exist.\",\"code\":\"INVALID\"}]}}}"
}

pub fn delete_existing_discounts_still_returns_deleted_id_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Delete me\", code: \"DELETE-ME\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    )
  let code_delete =
    run_mutation_from(
      code_create.store,
      code_create.identity,
      "mutation { discountCodeDelete(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { deletedCodeDiscountId userErrors { field message code } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Delete automatic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code } } }",
    )
  let automatic_delete =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountAutomaticDelete(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\") { deletedAutomaticDiscountId userErrors { field message code } } }",
    )

  assert json.to_string(code_delete.data)
    == "{\"data\":{\"discountCodeDelete\":{\"deletedCodeDiscountId\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\",\"userErrors\":[]}}}"
  assert json.to_string(automatic_delete.data)
    == "{\"data\":{\"discountAutomaticDelete\":{\"deletedAutomaticDiscountId\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\",\"userErrors\":[]}}}"
}

pub fn create_discount_inputs_reject_context_customer_selection_conflicts_test() {
  let cases = [
    #(
      "discountCodeBasicCreate",
      "codeDiscountNode",
      "basicCodeDiscount",
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic conflict\", code: \"CONFLICT-BASIC\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeBxgyCreate",
      "codeDiscountNode",
      "bxgyCodeDiscount",
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY conflict\", code: \"CONFLICT-BXGY\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeFreeShippingCreate",
      "codeDiscountNode",
      "freeShippingCodeDiscount",
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping conflict\", code: \"CONFLICT-SHIP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeAppCreate",
      "codeAppDiscount",
      "codeAppDiscount",
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App conflict\", code: \"CONFLICT-APP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code } } }",
    ),
  ]

  list.each(cases, fn(test_case) {
    let #(root, node_field, input_name, document) = test_case
    let outcome = run_mutation(document)

    assert json.to_string(outcome.data)
      == context_customer_selection_conflict_payload(
        root,
        node_field,
        input_name,
      )
  })
}

pub fn update_discount_inputs_reject_context_customer_selection_conflicts_test() {
  let cases = [
    #(
      "discountCodeBasicUpdate",
      "codeDiscountNode",
      "basicCodeDiscount",
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic valid\", code: \"CONFLICT-BASIC-UP\", startsAt: \"2026-04-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Basic conflict update\", code: \"CONFLICT-BASIC-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeBxgyUpdate",
      "codeDiscountNode",
      "bxgyCodeDiscount",
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY valid\", code: \"CONFLICT-BXGY-UP\", startsAt: \"2026-04-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code } } }",
      "mutation { discountCodeBxgyUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", bxgyCodeDiscount: { title: \"BXGY conflict update\", code: \"CONFLICT-BXGY-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeFreeShippingUpdate",
      "codeDiscountNode",
      "freeShippingCodeDiscount",
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping valid\", code: \"CONFLICT-SHIP-UP\", startsAt: \"2026-04-01T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code } } }",
      "mutation { discountCodeFreeShippingUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", freeShippingCodeDiscount: { title: \"Shipping conflict update\", code: \"CONFLICT-SHIP-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code } } }",
    ),
    #(
      "discountCodeAppUpdate",
      "codeAppDiscount",
      "codeAppDiscount",
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App valid\", code: \"CONFLICT-APP-UP\", startsAt: \"2026-04-01T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code } } }",
      "mutation { discountCodeAppUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codeAppDiscount: { title: \"App conflict update\", code: \"CONFLICT-APP-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerSelection: { customers: { add: [\"gid://shopify/Customer/2\"] } }, functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code } } }",
    ),
  ]

  list.each(cases, fn(test_case) {
    let #(root, node_field, input_name, create_document, update_document) =
      test_case
    let create_outcome = run_mutation(create_document)
    let update_outcome =
      run_mutation_from(
        create_outcome.store,
        create_outcome.identity,
        update_document,
      )

    assert json.to_string(update_outcome.data)
      == context_customer_selection_conflict_payload(
        root,
        node_field,
        input_name,
      )
  })
}

pub fn code_basic_create_keeps_single_buyer_selection_inputs_valid_test() {
  let context_only =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Context only\", code: \"CONTEXT-ONLY\", startsAt: \"2026-04-25T00:00:00Z\", context: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    )
  let customer_selection_only =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Selection only\", code: \"SELECTION-ONLY\", startsAt: \"2026-04-25T00:00:00Z\", customerSelection: { customers: { add: [\"gid://shopify/Customer/1\"] } }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code } } }",
    )

  assert json.to_string(context_only.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(customer_selection_only.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

pub fn minimum_requirement_quantity_and_subtotal_are_mutually_exclusive_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Minimum both\", code: \"MIN-BOTH\", startsAt: \"2026-04-25T00:00:00Z\", context: { all: ALL }, customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: \"2\" }, subtotal: { greaterThanOrEqualToSubtotal: \"10.00\" } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic minimum both\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: \"2\" }, subtotal: { greaterThanOrEqualToSubtotal: \"10.00\" } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(code_create.data)
    == minimum_requirement_conflict_payload(
      "discountCodeBasicCreate",
      "codeDiscountNode",
      "basicCodeDiscount",
    )
  assert json.to_string(automatic_create.data)
    == minimum_requirement_conflict_payload(
      "discountAutomaticBasicCreate",
      "automaticDiscountNode",
      "automaticBasicDiscount",
    )
}

pub fn minimum_requirement_updates_reject_quantity_and_subtotal_test() {
  let code_created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Minimum update\", code: \"MIN-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { all: ALL }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let code_update =
    run_mutation_from(
      code_created.store,
      code_created.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Minimum update\", code: \"MIN-UP\", startsAt: \"2026-04-25T00:00:00Z\", context: { all: ALL }, customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: \"2\" }, subtotal: { greaterThanOrEqualToSubtotal: \"10.00\" } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_created =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic minimum update\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_update =
    run_mutation_from(
      automatic_created.store,
      automatic_created.identity,
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Automatic minimum update\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: \"2\" }, subtotal: { greaterThanOrEqualToSubtotal: \"10.00\" } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(code_update.data)
    == minimum_requirement_conflict_payload(
      "discountCodeBasicUpdate",
      "codeDiscountNode",
      "basicCodeDiscount",
    )
  assert json.to_string(automatic_update.data)
    == minimum_requirement_conflict_payload(
      "discountAutomaticBasicUpdate",
      "automaticDiscountNode",
      "automaticBasicDiscount",
    )
}

pub fn minimum_requirement_limits_reject_captured_upper_bounds_test() {
  let quantity =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Minimum quantity limit\", code: \"MIN-Q-LIMIT\", startsAt: \"2026-04-25T00:00:00Z\", context: { all: ALL }, customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: \"9999999999\" } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let subtotal =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Minimum subtotal limit\", code: \"MIN-S-LIMIT\", startsAt: \"2026-04-25T00:00:00Z\", context: { all: ALL }, customerGets: { value: { percentage: 0.1 }, items: { all: true } }, minimumRequirement: { subtotal: { greaterThanOrEqualToSubtotal: \"1000000000000000001.00\" } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(quantity.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"minimumRequirement\",\"quantity\",\"greaterThanOrEqualToQuantity\"],\"message\":\"Minimum quantity must be less than 2147483647\",\"code\":\"LESS_THAN\",\"extraInfo\":null}]}}}"
  assert json.to_string(subtotal.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"minimumRequirement\",\"subtotal\",\"greaterThanOrEqualToSubtotal\"],\"message\":\"Minimum subtotal must be less than 1000000000000000000\",\"code\":\"LESS_THAN\",\"extraInfo\":null}]}}}"
}

fn minimum_requirement_conflict_payload(
  root: String,
  node_field: String,
  input_name: String,
) -> String {
  "{\"data\":{\""
  <> root
  <> "\":{\""
  <> node_field
  <> "\":null,\"userErrors\":[{\"field\":[\""
  <> input_name
  <> "\",\"minimumRequirement\",\"subtotal\",\"greaterThanOrEqualToSubtotal\"],\"message\":\"Minimum subtotal cannot be defined when minimum quantity is.\",\"code\":\"CONFLICT\",\"extraInfo\":null},{\"field\":[\""
  <> input_name
  <> "\",\"minimumRequirement\",\"quantity\",\"greaterThanOrEqualToQuantity\"],\"message\":\"Minimum quantity cannot be defined when minimum subtotal is.\",\"code\":\"CONFLICT\",\"extraInfo\":null}]}}}"
}

pub fn create_discount_inputs_reject_inverted_date_ranges_test() {
  let cases = [
    #(
      "discountCodeBasicCreate",
      "codeDiscountNode",
      "basicCodeDiscount",
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic invalid dates\", code: \"DATE-BASIC\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeBxgyCreate",
      "codeDiscountNode",
      "bxgyCodeDiscount",
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY invalid dates\", code: \"DATE-BXGY\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeFreeShippingCreate",
      "codeDiscountNode",
      "freeShippingCodeDiscount",
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping invalid dates\", code: \"DATE-SHIP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeAppCreate",
      "codeAppDiscount",
      "codeAppDiscount",
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App invalid dates\", code: \"DATE-APP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticBasicCreate",
      "automaticDiscountNode",
      "automaticBasicDiscount",
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic basic invalid dates\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticBxgyCreate",
      "automaticDiscountNode",
      "automaticBxgyDiscount",
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"Automatic BXGY invalid dates\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticFreeShippingCreate",
      "automaticDiscountNode",
      "freeShippingAutomaticDiscount",
      "mutation { discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: \"Automatic shipping invalid dates\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", destination: { all: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticAppCreate",
      "automaticAppDiscount",
      "automaticAppDiscount",
      "mutation { discountAutomaticAppCreate(automaticAppDiscount: { title: \"Automatic app invalid dates\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", functionHandle: \"discount-local\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    ),
  ]

  list.each(cases, fn(test_case) {
    let #(root, node_field, input_name, document) = test_case
    let outcome = run_mutation(document)

    assert json.to_string(outcome.data)
      == invalid_date_range_payload(root, node_field, input_name)
  })
}

pub fn update_discount_inputs_reject_inverted_date_ranges_test() {
  let cases = [
    #(
      "discountCodeBasicUpdate",
      "codeDiscountNode",
      "basicCodeDiscount",
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic valid\", code: \"DATE-BASIC-UP\", startsAt: \"2026-04-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Basic invalid update\", code: \"DATE-BASIC-UP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeBxgyUpdate",
      "codeDiscountNode",
      "bxgyCodeDiscount",
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY valid\", code: \"DATE-BXGY-UP\", startsAt: \"2026-04-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountCodeBxgyUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", bxgyCodeDiscount: { title: \"BXGY invalid update\", code: \"DATE-BXGY-UP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeFreeShippingUpdate",
      "codeDiscountNode",
      "freeShippingCodeDiscount",
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping valid\", code: \"DATE-SHIP-UP\", startsAt: \"2026-04-01T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountCodeFreeShippingUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", freeShippingCodeDiscount: { title: \"Shipping invalid update\", code: \"DATE-SHIP-UP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountCodeAppUpdate",
      "codeAppDiscount",
      "codeAppDiscount",
      "mutation { discountCodeAppCreate(codeAppDiscount: { title: \"App valid\", code: \"DATE-APP-UP\", startsAt: \"2026-04-01T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
      "mutation { discountCodeAppUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codeAppDiscount: { title: \"App invalid update\", code: \"DATE-APP-UP\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", functionHandle: \"discount-local\" }) { codeAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticBasicUpdate",
      "automaticDiscountNode",
      "automaticBasicDiscount",
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic basic valid\", startsAt: \"2026-04-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Automatic basic invalid update\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticBxgyUpdate",
      "automaticDiscountNode",
      "automaticBxgyDiscount",
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"Automatic BXGY valid\", startsAt: \"2026-04-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountAutomaticBxgyUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBxgyDiscount: { title: \"Automatic BXGY invalid update\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticFreeShippingUpdate",
      "automaticDiscountNode",
      "freeShippingAutomaticDiscount",
      "mutation { discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: \"Automatic shipping valid\", startsAt: \"2026-04-01T00:00:00Z\", destination: { all: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
      "mutation { discountAutomaticFreeShippingUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", freeShippingAutomaticDiscount: { title: \"Automatic shipping invalid update\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", destination: { all: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    ),
    #(
      "discountAutomaticAppUpdate",
      "automaticAppDiscount",
      "automaticAppDiscount",
      "mutation { discountAutomaticAppCreate(automaticAppDiscount: { title: \"Automatic app valid\", startsAt: \"2026-04-01T00:00:00Z\", functionHandle: \"discount-local\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
      "mutation { discountAutomaticAppUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticAppDiscount: { title: \"Automatic app invalid update\", startsAt: \"2026-06-01T00:00:00Z\", endsAt: \"2026-05-01T00:00:00Z\", functionHandle: \"discount-local\" }) { automaticAppDiscount { discountId } userErrors { field message code extraInfo } } }",
    ),
  ]

  list.each(cases, fn(test_case) {
    let #(root, node_field, input_name, create_document, update_document) =
      test_case
    let initial_store = case input_name {
      "codeAppDiscount" | "automaticAppDiscount" -> discount_function_store()
      _ -> store.new()
    }
    let create_outcome =
      run_mutation_from(
        initial_store,
        synthetic_identity.new(),
        create_document,
      )
    let update_outcome =
      run_mutation_from(
        create_outcome.store,
        create_outcome.identity,
        update_document,
      )

    assert json.to_string(update_outcome.data)
      == invalid_date_range_payload(root, node_field, input_name)
  })
}

pub fn discount_date_range_comparison_normalizes_offsets_test() {
  let invalid_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Offset invalid\", code: \"DATE-OFFSET-BAD\", startsAt: \"2026-06-01T00:00:00-05:00\", endsAt: \"2026-06-01T01:00:00+00:00\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let valid_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Offset valid\", code: \"DATE-OFFSET-OK\", startsAt: \"2026-06-01T00:00:00+00:00\", endsAt: \"2026-05-31T20:00:00-05:00\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(invalid_outcome.data)
    == invalid_date_range_payload(
      "discountCodeBasicCreate",
      "codeDiscountNode",
      "basicCodeDiscount",
    )
  assert json.to_string(valid_outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

fn invalid_date_range_payload(
  root: String,
  node_field: String,
  input_name: String,
) -> String {
  "{\"data\":{\""
  <> root
  <> "\":{\""
  <> node_field
  <> "\":null,\"userErrors\":[{\"field\":[\""
  <> input_name
  <> "\",\"endsAt\"],\"message\":\"Ends at needs to be after starts_at\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

fn context_customer_selection_conflict_payload(
  root: String,
  node_field: String,
  input_name: String,
) -> String {
  "{\"data\":{\""
  <> root
  <> "\":{\""
  <> node_field
  <> "\":null,\"userErrors\":[{\"field\":[\""
  <> input_name
  <> "\",\"context\"],\"message\":\"Only one of context or customerSelection can be provided.\",\"code\":\"INVALID\"}]}}}"
}

pub fn code_basic_update_rejects_code_change_after_redeem_code_bulk_add_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk rule\", code: \"BULK-RULE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let bulk_added =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: [\"BULK-ONE\", \"BULK-TWO\", \"BULK-THREE\", \"BULK-FOUR\", \"BULK-FIVE\"]) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }",
    )
  let update =
    run_mutation_from(
      bulk_added.store,
      bulk_added.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Bulk rule renamed\", code: \"BULK-RULE-NEW\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot update the code of a bulk discount.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"

  let assert Ok(read) =
    discounts.handle_discount_query(
      update.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { ... on DiscountCodeBasic { title codes(first: 10) { nodes { code } } } } } byNewCode: codeDiscountNodeByCode(code: \"BULK-RULE-NEW\") { id } }",
      dict.new(),
    )

  assert json.to_string(read)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"title\":\"Bulk rule\",\"codes\":{\"nodes\":[{\"code\":\"BULK-RULE\"},{\"code\":\"BULK-ONE\"},{\"code\":\"BULK-TWO\"},{\"code\":\"BULK-THREE\"},{\"code\":\"BULK-FOUR\"},{\"code\":\"BULK-FIVE\"}]}}},\"byNewCode\":null}"
}

pub fn code_basic_update_rejects_same_code_after_redeem_code_bulk_add_test() {
  let created =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Bulk rule\", code: \"BULK-RULE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let bulk_added =
    run_mutation_from(
      created.store,
      created.identity,
      "mutation { discountRedeemCodeBulkAdd(discountId: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", codes: [\"BULK-ONE\"]) { bulkCreation { codesCount } userErrors { field message code extraInfo } } }",
    )
  let update =
    run_mutation_from(
      bulk_added.store,
      bulk_added.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Bulk rule renamed\", code: \"BULK-RULE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot update the code of a bulk discount.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_update_rejects_code_taken_by_another_local_discount_test() {
  let first =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"First\", code: \"TAKEN-ONE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let second =
    run_mutation_from(
      first.store,
      first.identity,
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Second\", code: \"TAKEN-TWO\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let update =
    run_mutation_from(
      second.store,
      second.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/3?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Second\", code: \"TAKEN-ONE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"code\"],\"message\":\"Code must be unique. Please try a different code.\",\"code\":\"TAKEN\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_update_on_bxgy_discount_transitions_to_basic_test() {
  let bxgy =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGY-TO-BASIC\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id codeDiscount { __typename } } userErrors { field message code extraInfo } } }",
    )
  let update =
    run_mutation_from(
      bxgy.store,
      bxgy.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Now basic\", code: \"BXGY-TO-BASIC\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { __typename ... on DiscountCodeBasic { title discountClasses customerGets { value { __typename ... on DiscountPercentage { percentage } } } } ... on DiscountCodeBxgy { title customerBuys { value { quantity } } } } } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"__typename\":\"DiscountCodeBasic\",\"title\":\"Now basic\",\"discountClasses\":[\"ORDER\"],\"customerGets\":{\"value\":{\"__typename\":\"DiscountPercentage\",\"percentage\":0.2}}}},\"userErrors\":[]}}}"

  let assert Ok(read) =
    discounts.handle_discount_query(
      update.store,
      "query { codeDiscountNode(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\") { codeDiscount { __typename ... on DiscountCodeBasic { title } ... on DiscountCodeBxgy { title } } } }",
      dict.new(),
    )

  assert json.to_string(read)
    == "{\"codeDiscountNode\":{\"codeDiscount\":{\"__typename\":\"DiscountCodeBasic\",\"title\":\"Now basic\"}}}"
}

pub fn code_basic_update_unknown_id_uses_invalid_error_code_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/0\", basicCodeDiscount: { title: \"Missing\", code: \"MISSING\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_rejects_cart_line_tag_settings_for_order_class_test() {
  let outcome =
    run_mutation(
      "mutation { orderTagStacking: discountCodeBasicCreate(basicCodeDiscount: { title: \"Order tags invalid\", code: \"ORDER-TAGS\", startsAt: \"2026-05-05T00:00:00Z\", combinesWith: { productDiscounts: true, productDiscountsWithTagsOnSameCartLine: { add: [\"vip\"] } }, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderTagStacking\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"combinesWith\",\"productDiscountsWithTagsOnSameCartLine\"],\"message\":\"The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.\",\"code\":\"PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED\",\"extraInfo\":null},{\"field\":[\"basicCodeDiscount\",\"combinesWith\",\"productDiscountsWithTagsOnSameCartLine\"],\"message\":\"Combines with product discounts with tags on same cart line is only valid for discounts with the PRODUCT discount class\",\"code\":\"INVALID_PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_FOR_DISCOUNT_CLASS\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_product_class_tag_settings_skip_class_error_test() {
  let outcome =
    run_mutation(
      "mutation { productTagStacking: discountCodeBasicCreate(basicCodeDiscount: { title: \"Product tags invalid\", code: \"PRODUCT-TAGS\", startsAt: \"2026-05-05T00:00:00Z\", combinesWith: { productDiscountsWithTagsOnSameCartLine: { add: [\"vip\"] } }, customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"productTagStacking\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"combinesWith\",\"productDiscountsWithTagsOnSameCartLine\"],\"message\":\"The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.\",\"code\":\"PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_discount_class_follows_customer_gets_items_test() {
  let order_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Order class\", code: \"ORDER-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { discountClasses discountClass } } } userErrors { message } } }",
    )
  let product_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Product class\", code: \"PRODUCT-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { discountClasses discountClass } } } userErrors { message } } }",
    )
  let collection_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Collection class\", code: \"COLLECTION-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { collections: { add: [\"gid://shopify/Collection/1\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { discountClasses discountClass } } } userErrors { message } } }",
    )

  assert json.to_string(order_outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"ORDER\"],\"discountClass\":\"ORDER\"}},\"userErrors\":[]}}}"
  assert json.to_string(product_outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"PRODUCT\"],\"discountClass\":\"PRODUCT\"}},\"userErrors\":[]}}}"
  assert json.to_string(collection_outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"PRODUCT\"],\"discountClass\":\"PRODUCT\"}},\"userErrors\":[]}}}"
}

pub fn explicit_singular_discount_class_overrides_basic_inference_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Explicit class\", code: \"EXPLICIT-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", discountClass: ORDER, customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { discountClasses discountClass } } } userErrors { message } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"ORDER\"],\"discountClass\":\"ORDER\"}},\"userErrors\":[]}}}"
}

pub fn bxgy_and_free_shipping_default_discount_classes_test() {
  let bxgy_outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGY-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeBxgy { discountClasses discountClass } } } userErrors { message } } }",
    )
  let free_shipping_outcome =
    run_mutation(
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Free shipping\", code: \"SHIP-CLASS\", startsAt: \"2026-05-05T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { codeDiscount { ... on DiscountCodeFreeShipping { discountClasses discountClass } } } userErrors { message } } }",
    )

  assert json.to_string(bxgy_outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"PRODUCT\"],\"discountClass\":\"PRODUCT\"}},\"userErrors\":[]}}}"
  assert json.to_string(free_shipping_outcome.data)
    == "{\"data\":{\"discountCodeFreeShippingCreate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"discountClasses\":[\"SHIPPING\"],\"discountClass\":\"SHIPPING\"}},\"userErrors\":[]}}}"
}

pub fn discount_nodes_filter_by_discount_class_test() {
  let order_outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Order class\", code: \"ORDER-FILTER\", startsAt: \"2026-05-05T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let product_outcome =
    run_mutation_from(
      order_outcome.store,
      order_outcome.identity,
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Product class\", code: \"PRODUCT-FILTER\", startsAt: \"2026-05-05T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let bxgy_outcome =
    run_mutation_from(
      product_outcome.store,
      product_outcome.identity,
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY class\", code: \"BXGY-FILTER\", startsAt: \"2026-05-05T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { message } } }",
    )

  let assert Ok(data) =
    discounts.handle_discount_query(
      bxgy_outcome.store,
      "query { discountNodes(first: 10, query: \"discount_class:product\") { nodes { discount { ... on DiscountCodeBasic { title discountClass } ... on DiscountCodeBxgy { title discountClass } } } } discountNodesCount(query: \"discount_class:product\") { count precision } }",
      dict.new(),
    )

  assert json.to_string(data)
    == "{\"discountNodes\":{\"nodes\":[{\"discount\":{\"title\":\"Product class\",\"discountClass\":\"PRODUCT\"}},{\"discount\":{\"title\":\"BXGY class\",\"discountClass\":\"PRODUCT\"}}]},\"discountNodesCount\":{\"count\":2,\"precision\":\"EXACT\"}}"
}

pub fn code_basic_rejects_cart_line_tag_overlap_as_bad_request_test() {
  let outcome =
    run_mutation(
      "mutation { tagOverlap: discountCodeBasicCreate(basicCodeDiscount: { title: \"Overlap invalid\", code: \"TAG-OVERLAP\", startsAt: \"2026-05-05T00:00:00Z\", combinesWith: { productDiscountsWithTagsOnSameCartLine: { add: [\"same\"], remove: [\"same\"] } }, customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"The same tag is present in both `add` and `remove` fields of `productDiscountsWithTagsOnSameCartLine`.\",\"locations\":[{\"line\":1,\"column\":12}],\"extensions\":{\"code\":\"BAD_REQUEST\"},\"path\":[\"tagOverlap\"]}],\"data\":{\"tagOverlap\":null}}"
}

fn assert_customer_gets_value_bad_request(data: json.Json, root: String) {
  let body = json.to_string(data)
  assert string.contains(
    body,
    "\"message\":\"A discount can only have one of percentage, discountOnQuantity or discountAmount.\"",
  )
  assert string.contains(body, "\"extensions\":{\"code\":\"BAD_REQUEST\"}")
  assert string.contains(body, "\"path\":[\"" <> root <> "\"]")
  assert string.contains(body, "\"data\":{\"" <> root <> "\":null}")
  assert !string.contains(body, "userErrors")
}

pub fn customer_gets_multiple_value_types_bad_request_on_basic_creates_test() {
  let code =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Multi\", code: \"MULTI-CODE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1, discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Multi\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1, discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert_customer_gets_value_bad_request(code.data, "discountCodeBasicCreate")
  assert_customer_gets_value_bad_request(
    automatic.data,
    "discountAutomaticBasicCreate",
  )
}

pub fn customer_gets_multiple_value_types_bad_request_on_basic_updates_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Valid\", code: \"VALID-CODE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let code_update =
    run_mutation_from(
      code_create.store,
      code_create.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Multi\", code: \"MULTI-CODE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2, discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Valid\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { message } } }",
    )
  let automatic_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Multi\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.2, discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert_customer_gets_value_bad_request(
    code_update.data,
    "discountCodeBasicUpdate",
  )
  assert_customer_gets_value_bad_request(
    automatic_update.data,
    "discountAutomaticBasicUpdate",
  )
}

pub fn customer_gets_multiple_value_types_bad_request_on_non_basic_updates_test() {
  let code_bxgy_create =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYUP\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let code_bxgy_update =
    run_mutation_from(
      code_bxgy_create.store,
      code_bxgy_create.identity,
      "mutation { discountCodeBxgyUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYUP2\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { percentage: 0.5, discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_bxgy_create =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { message } } }",
    )
  let automatic_bxgy_update =
    run_mutation_from(
      automatic_bxgy_create.store,
      automatic_bxgy_create.identity,
      "mutation { discountAutomaticBxgyUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { percentage: 0.5, discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let code_shipping_create =
    run_mutation(
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Ship\", code: \"SHIPUP\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let code_shipping_update =
    run_mutation_from(
      code_shipping_create.store,
      code_shipping_create.identity,
      "mutation { discountCodeFreeShippingUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", freeShippingCodeDiscount: { title: \"Ship\", code: \"SHIPUP2\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, customerGets: { value: { percentage: 0.5, discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_shipping_create =
    run_mutation(
      "mutation { discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: \"Ship\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true } }) { automaticDiscountNode { id } userErrors { message } } }",
    )
  let automatic_shipping_update =
    run_mutation_from(
      automatic_shipping_create.store,
      automatic_shipping_create.identity,
      "mutation { discountAutomaticFreeShippingUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", freeShippingAutomaticDiscount: { title: \"Ship\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, customerGets: { value: { percentage: 0.5, discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert_customer_gets_value_bad_request(
    code_bxgy_update.data,
    "discountCodeBxgyUpdate",
  )
  assert_customer_gets_value_bad_request(
    automatic_bxgy_update.data,
    "discountAutomaticBxgyUpdate",
  )
  assert_customer_gets_value_bad_request(
    code_shipping_update.data,
    "discountCodeFreeShippingUpdate",
  )
  assert_customer_gets_value_bad_request(
    automatic_shipping_update.data,
    "discountAutomaticFreeShippingUpdate",
  )
}

pub fn basic_discount_inputs_reject_discount_on_quantity_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic quantity\", code: \"BASIC-QTY\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountOnQuantity: { quantity: \"2\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let valid_code =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Valid basic\", code: \"VALID-QTY-UP\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { message } } }",
    )
  let code_update =
    run_mutation_from(
      valid_code.store,
      valid_code.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Basic quantity update\", code: \"VALID-QTY-UP\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountOnQuantity: { quantity: \"2\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic quantity\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountOnQuantity: { quantity: \"2\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let valid_automatic =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Valid automatic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { message } } }",
    )
  let automatic_update =
    run_mutation_from(
      valid_automatic.store,
      valid_automatic.identity,
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Automatic quantity update\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { discountOnQuantity: { quantity: \"2\", effect: { percentage: 0.5 } } }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(code_create.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\"],\"message\":\"discountOnQuantity field is only permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(code_update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\"],\"message\":\"discountOnQuantity field is only permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic_create.data)
    == "{\"data\":{\"discountAutomaticBasicCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\"],\"message\":\"discountOnQuantity field is only permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic_update.data)
    == "{\"data\":{\"discountAutomaticBasicUpdate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\"],\"message\":\"discountOnQuantity field is only permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn bxgy_discount_on_quantity_remains_valid_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGY-OK-QTY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let code_update =
    run_mutation_from(
      code_create.store,
      code_create.identity,
      "mutation { discountCodeBxgyUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", bxgyCodeDiscount: { title: \"BXGY update\", code: \"BXGY-OK-QTY-UP\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountAutomaticBxgyUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBxgyDiscount: { title: \"BXGY update\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(code_create.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(code_update.data)
    == "{\"data\":{\"discountCodeBxgyUpdate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(automatic_create.data)
    == "{\"data\":{\"discountAutomaticBxgyCreate\":{\"automaticDiscountNode\":{\"id\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(automatic_update.data)
    == "{\"data\":{\"discountAutomaticBxgyUpdate\":{\"automaticDiscountNode\":{\"id\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

pub fn blank_bxgy_returns_captured_user_errors_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"\", code: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { all: true } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\"],\"message\":\"Items in 'customer get' cannot be set to all\",\"code\":\"INVALID\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"customerBuys\",\"items\"],\"message\":\"Items in 'customer buys' must be defined\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
}

pub fn code_bxgy_rejects_customer_gets_percentage_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYPCT\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { percentage: 0.5 }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Only discountOnQuantity permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\",\"quantity\"],\"message\":\"Quantity cannot be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
}

pub fn code_bxgy_rejects_customer_gets_discount_amount_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYAMT\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"discountAmount\"],\"message\":\"Only discountOnQuantity permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\",\"quantity\"],\"message\":\"Quantity cannot be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
}

pub fn code_bxgy_rejects_customer_gets_subscription_fields_test() {
  let subscription_outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYSUB\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let one_time_outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYOTP\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } }, appliesOnOneTimePurchase: false } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(subscription_outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"This field is not supported by bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(one_time_outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"appliesOnOneTimePurchase\"],\"message\":\"This field is not supported by bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn code_basic_rejects_subscription_fields_for_default_non_subscription_shop_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Sub gated\", code: \"SUBGATE\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"Customer gets applies on subscription is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn default_non_subscription_shop_rejects_basic_subscription_inputs_test() {
  let outcome =
    run_mutation(
      "mutation { oneTime: discountCodeBasicCreate(basicCodeDiscount: { title: \"One time gated\", code: \"SUBGATE-OTP\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnOneTimePurchase: false } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } nullSub: discountCodeBasicCreate(basicCodeDiscount: { title: \"Null gated\", code: \"SUBGATE-NULL\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: null } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } recurring: discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Recurring gated\", startsAt: \"2026-04-25T00:00:00Z\", recurringCycleLimit: 2, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"oneTime\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"appliesOnOneTimePurchase\"],\"message\":\"Customer gets applies on one time purchase is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"nullSub\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"Customer gets applies on subscription is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"recurring\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"recurringCycleLimit\"],\"message\":\"Recurring cycle limit is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn default_non_subscription_shop_rejects_free_shipping_subscription_inputs_test() {
  let outcome =
    run_mutation(
      "mutation { sub: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping sub gated\", code: \"SHIP-SUB\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, appliesOnSubscription: true }) { codeDiscountNode { id } userErrors { field message code extraInfo } } recurring: discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping recurring gated\", code: \"SHIP-REC\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, recurringCycleLimit: 2 }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"sub\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"freeShippingCodeDiscount\",\"appliesOnSubscription\"],\"message\":\"Applies on subscription is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"recurring\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"freeShippingCodeDiscount\",\"recurringCycleLimit\"],\"message\":\"Recurring cycle limit is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn subscription_shop_allows_subscription_flags_but_rejects_explicit_null_test() {
  let allowed =
    run_mutation_from(
      subscription_store(),
      synthetic_identity.new(),
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Subscription enabled\", code: \"SUB-ENABLED\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let blank =
    run_mutation_from(
      subscription_store(),
      synthetic_identity.new(),
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Subscription blank\", code: \"SUB-BLANK\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: null } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(allowed.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":{\"id\":\"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(blank.data)
    == "{\"data\":{\"discountCodeBasicCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"applies_on_subscription can't be blank\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn subscription_field_gating_applies_to_update_roots_test() {
  let basic_code_create =
    run_mutation(
      "mutation { discountCodeBasicCreate(basicCodeDiscount: { title: \"Basic\", code: \"BASIC-UP-SUB\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let basic_code_update =
    run_mutation_from(
      basic_code_create.store,
      basic_code_create.identity,
      "mutation { discountCodeBasicUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", basicCodeDiscount: { title: \"Basic\", code: \"BASIC-UP-SUB\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true }, appliesOnSubscription: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let shipping_code_create =
    run_mutation(
      "mutation { discountCodeFreeShippingCreate(freeShippingCodeDiscount: { title: \"Shipping\", code: \"SHIP-UP-SUB\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let shipping_code_update =
    run_mutation_from(
      shipping_code_create.store,
      shipping_code_create.identity,
      "mutation { discountCodeFreeShippingUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", freeShippingCodeDiscount: { title: \"Shipping\", code: \"SHIP-UP-SUB\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, appliesOnOneTimePurchase: false }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_basic_create =
    run_mutation(
      "mutation { discountAutomaticBasicCreate(automaticBasicDiscount: { title: \"Automatic basic\", startsAt: \"2026-04-25T00:00:00Z\", customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_basic_update =
    run_mutation_from(
      automatic_basic_create.store,
      automatic_basic_create.identity,
      "mutation { discountAutomaticBasicUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBasicDiscount: { title: \"Automatic basic\", startsAt: \"2026-04-25T00:00:00Z\", recurringCycleLimit: 2, customerGets: { value: { percentage: 0.1 }, items: { all: true } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(basic_code_update.data)
    == "{\"data\":{\"discountCodeBasicUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"basicCodeDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"Customer gets applies on subscription is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(shipping_code_update.data)
    == "{\"data\":{\"discountCodeFreeShippingUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"freeShippingCodeDiscount\",\"appliesOnOneTimePurchase\"],\"message\":\"Applies on one time purchase is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic_basic_update.data)
    == "{\"data\":{\"discountAutomaticBasicUpdate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBasicDiscount\",\"recurringCycleLimit\"],\"message\":\"Recurring cycle limit is not permitted for this shop.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn automatic_free_shipping_skips_subscription_field_validation_test() {
  let create =
    run_mutation(
      "mutation { discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: { title: \"Automatic shipping\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 2 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let update =
    run_mutation_from(
      create.store,
      create.identity,
      "mutation { discountAutomaticFreeShippingUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", freeShippingAutomaticDiscount: { title: \"Automatic shipping\", startsAt: \"2026-04-25T00:00:00Z\", destination: { all: true }, appliesOnSubscription: true, appliesOnOneTimePurchase: false, recurringCycleLimit: 3 }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(create.data)
    == "{\"data\":{\"discountAutomaticFreeShippingCreate\":{\"automaticDiscountNode\":{\"id\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
  assert json.to_string(update.data)
    == "{\"data\":{\"discountAutomaticFreeShippingUpdate\":{\"automaticDiscountNode\":{\"id\":\"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

pub fn automatic_bxgy_rejects_disallowed_customer_gets_fields_test() {
  let percentage_outcome =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { percentage: 0.5 }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let amount_outcome =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountAmount: { amount: \"5.00\", appliesOnEachItem: false } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let subscription_outcome =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } }, appliesOnSubscription: true } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let one_time_outcome =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } }, appliesOnOneTimePurchase: false } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(percentage_outcome.data)
    == "{\"data\":{\"discountAutomaticBxgyCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBxgyDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Only discountOnQuantity permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(amount_outcome.data)
    == "{\"data\":{\"discountAutomaticBxgyCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBxgyDiscount\",\"customerGets\",\"value\",\"discountAmount\"],\"message\":\"Only discountOnQuantity permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(subscription_outcome.data)
    == "{\"data\":{\"discountAutomaticBxgyCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBxgyDiscount\",\"customerGets\",\"appliesOnSubscription\"],\"message\":\"This field is not supported by automatic bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
  assert json.to_string(one_time_outcome.data)
    == "{\"data\":{\"discountAutomaticBxgyCreate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBxgyDiscount\",\"customerGets\",\"appliesOnOneTimePurchase\"],\"message\":\"This field is not supported by automatic bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn bxgy_updates_reuse_create_validation_rules_test() {
  let code_create =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYUP\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let code_update =
    run_mutation_from(
      code_create.store,
      code_create.identity,
      "mutation { discountCodeBxgyUpdate(id: \"gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic\", bxgyCodeDiscount: { title: \"BXGY\", code: \"BXGYUP2\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { percentage: 0.5 }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_create =
    run_mutation(
      "mutation { discountAutomaticBxgyCreate(automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } } } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )
  let automatic_update =
    run_mutation_from(
      automatic_create.store,
      automatic_create.identity,
      "mutation { discountAutomaticBxgyUpdate(id: \"gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic\", automaticBxgyDiscount: { title: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 0.5 } } }, items: { products: { productsToAdd: [\"gid://shopify/Product/2\"] } }, appliesOnOneTimePurchase: false } }) { automaticDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(code_update.data)
    == "{\"data\":{\"discountCodeBxgyUpdate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"percentage\"],\"message\":\"Only discountOnQuantity permitted with bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"customerGets\",\"value\",\"discountOnQuantity\",\"quantity\"],\"message\":\"Quantity cannot be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
  assert json.to_string(automatic_update.data)
    == "{\"data\":{\"discountAutomaticBxgyUpdate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"automaticBxgyDiscount\",\"customerGets\",\"appliesOnOneTimePurchase\"],\"message\":\"This field is not supported by automatic bxgy discounts.\",\"code\":\"INVALID\",\"extraInfo\":null}]}}}"
}

pub fn inline_null_input_returns_top_level_error_test() {
  let outcome =
    run_mutation(
      "mutation DiscountCodeBasicCreateInlineNullInput { discountCodeBasicCreate(basicCodeDiscount: null) { codeDiscountNode { id } userErrors { message } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'basicCodeDiscount' on Field 'discountCodeBasicCreate' has an invalid value (null). Expected type 'DiscountCodeBasicInput!'.\",\"locations\":[{\"line\":1,\"column\":51}],\"path\":[\"mutation DiscountCodeBasicCreateInlineNullInput\",\"discountCodeBasicCreate\",\"basicCodeDiscount\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"basicCodeDiscount\"}}]}"
}

pub fn activate_expired_code_discount_rewrites_stale_dates_test() {
  let id = "gid://shopify/DiscountCodeNode/expired"
  let record =
    code_discount_record(
      id,
      "DiscountCodeBasic",
      "SCHEDULED",
      "2030-01-01T00:00:00Z",
      Some("2023-01-01T00:00:00Z"),
      None,
    )
  let #(_, seeded_store) = store.stage_discount(store.new(), record)
  let outcome =
    run_mutation_from(
      seeded_store,
      synthetic_identity.new(),
      "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/expired\") { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"startsAt\":\"2024-01-01T00:00:00.000Z\",\"endsAt\":null,\"status\":\"ACTIVE\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
}

pub fn activate_already_active_code_discount_preserves_dates_test() {
  let id = "gid://shopify/DiscountCodeNode/active"
  let record =
    code_discount_record(
      id,
      "DiscountCodeBasic",
      "ACTIVE",
      "2024-02-01T00:00:00Z",
      Some("2030-01-01T00:00:00Z"),
      None,
    )
  let #(_, seeded_store) = store.stage_discount(store.new(), record)
  let outcome =
    run_mutation_from(
      seeded_store,
      synthetic_identity.new(),
      "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/active\") { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"startsAt\":\"2024-02-01T00:00:00Z\",\"endsAt\":\"2030-01-01T00:00:00Z\",\"status\":\"ACTIVE\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
}

pub fn deactivate_code_discount_rewrites_future_start_and_end_test() {
  let id = "gid://shopify/DiscountCodeNode/deactivate"
  let record =
    code_discount_record(
      id,
      "DiscountCodeBasic",
      "SCHEDULED",
      "2030-01-01T00:00:00Z",
      None,
      None,
    )
  let #(_, seeded_store) = store.stage_discount(store.new(), record)
  let outcome =
    run_mutation_from(
      seeded_store,
      synthetic_identity.new(),
      "mutation { discountCodeDeactivate(id: \"gid://shopify/DiscountCodeNode/deactivate\") { codeDiscountNode { codeDiscount { ... on DiscountCodeBasic { startsAt endsAt status updatedAt } } } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeDeactivate\":{\"codeDiscountNode\":{\"codeDiscount\":{\"startsAt\":\"2024-01-01T00:00:00.000Z\",\"endsAt\":\"2024-01-01T00:00:00.000Z\",\"status\":\"EXPIRED\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
}

pub fn activate_missing_app_function_returns_base_internal_error_test() {
  let id = "gid://shopify/DiscountCodeNode/app-missing"
  let function_id = "gid://shopify/ShopifyFunction/missing"
  let record =
    code_discount_record(
      id,
      "DiscountCodeApp",
      "EXPIRED",
      "2023-01-01T00:00:00Z",
      Some("2023-02-01T00:00:00Z"),
      Some(function_id),
    )
  let #(_, seeded_store) = store.stage_discount(store.new(), record)
  let outcome =
    run_mutation_from(
      seeded_store,
      synthetic_identity.new(),
      "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/app-missing\") { codeDiscountNode { id } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"Discount could not be activated.\",\"code\":\"INTERNAL_ERROR\"}]}}}"
}

pub fn automatic_activate_missing_app_function_returns_base_internal_error_test() {
  let id = "gid://shopify/DiscountAutomaticNode/app-missing"
  let function_id = "gid://shopify/ShopifyFunction/missing"
  let record =
    automatic_app_discount_record(
      id,
      "EXPIRED",
      "2023-01-01T00:00:00Z",
      Some("2023-02-01T00:00:00Z"),
      Some(function_id),
    )
  let #(_, seeded_store) = store.stage_discount(store.new(), record)
  let outcome =
    run_mutation_from(
      seeded_store,
      synthetic_identity.new(),
      "mutation { discountAutomaticActivate(id: \"gid://shopify/DiscountAutomaticNode/app-missing\") { automaticDiscountNode { id } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountAutomaticActivate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"Discount could not be activated.\",\"code\":\"INTERNAL_ERROR\"}]}}}"
}

pub fn activate_deactivate_unknown_discounts_keep_id_invalid_error_test() {
  let code_activate =
    run_mutation(
      "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/0\") { codeDiscountNode { id } userErrors { field message code } } }",
    )
  let code_deactivate =
    run_mutation(
      "mutation { discountCodeDeactivate(id: \"gid://shopify/DiscountCodeNode/0\") { codeDiscountNode { id } userErrors { field message code } } }",
    )
  let automatic_activate =
    run_mutation(
      "mutation { discountAutomaticActivate(id: \"gid://shopify/DiscountAutomaticNode/0\") { automaticDiscountNode { id } userErrors { field message code } } }",
    )
  let automatic_deactivate =
    run_mutation(
      "mutation { discountAutomaticDeactivate(id: \"gid://shopify/DiscountAutomaticNode/0\") { automaticDiscountNode { id } userErrors { field message code } } }",
    )

  assert json.to_string(code_activate.data)
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\"}]}}}"
  assert json.to_string(code_deactivate.data)
    == "{\"data\":{\"discountCodeDeactivate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\"}]}}}"
  assert json.to_string(automatic_activate.data)
    == "{\"data\":{\"discountAutomaticActivate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\"}]}}}"
  assert json.to_string(automatic_deactivate.data)
    == "{\"data\":{\"discountAutomaticDeactivate\":{\"automaticDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\"}]}}}"
}

fn code_discount_record(
  id: String,
  typename: String,
  status: String,
  starts_at: String,
  ends_at: Option(String),
  function_id: Option(String),
) -> DiscountRecord {
  DiscountRecord(
    id: id,
    owner_kind: "code",
    discount_type: case typename {
      "DiscountCodeApp" -> "app"
      _ -> "basic"
    },
    title: Some("Test discount"),
    status: status,
    code: Some("TEST"),
    payload: CapturedObject([
      #("id", CapturedString(id)),
      #(
        "codeDiscount",
        CapturedObject(
          [
            #("__typename", CapturedString(typename)),
            #("title", CapturedString("Test discount")),
            #("status", CapturedString(status)),
            #("startsAt", CapturedString(starts_at)),
            #("endsAt", case ends_at {
              Some(value) -> CapturedString(value)
              None -> CapturedNull
            }),
          ]
          |> with_app_discount_type(function_id),
        ),
      ),
    ]),
    cursor: None,
  )
}

fn automatic_app_discount_record(
  id: String,
  status: String,
  starts_at: String,
  ends_at: Option(String),
  function_id: Option(String),
) -> DiscountRecord {
  DiscountRecord(
    id: id,
    owner_kind: "automatic",
    discount_type: "app",
    title: Some("Test automatic discount"),
    status: status,
    code: None,
    payload: CapturedObject([
      #("id", CapturedString(id)),
      #(
        "automaticDiscount",
        CapturedObject(
          [
            #("__typename", CapturedString("DiscountAutomaticApp")),
            #("title", CapturedString("Test automatic discount")),
            #("status", CapturedString(status)),
            #("startsAt", CapturedString(starts_at)),
            #("endsAt", case ends_at {
              Some(value) -> CapturedString(value)
              None -> CapturedNull
            }),
          ]
          |> with_app_discount_type(function_id),
        ),
      ),
    ]),
    cursor: None,
  )
}

fn with_app_discount_type(
  fields: List(#(String, CapturedJsonValue)),
  function_id: Option(String),
) -> List(#(String, CapturedJsonValue)) {
  case function_id {
    Some(id) ->
      list.append(fields, [
        #(
          "appDiscountType",
          CapturedObject([
            #("functionId", CapturedString(id)),
            #("title", CapturedString("Test app discount")),
          ]),
        ),
      ])
    None -> fields
  }
}

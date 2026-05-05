//// Focused mutation/read tests for `proxy/discounts`.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountRecord, type SavedSearchRecord,
  CapturedNull, CapturedObject, CapturedString, DiscountRecord,
  SavedSearchRecord,
}

fn run_mutation(document: String) -> mutation_helpers.MutationOutcome {
  run_mutation_from(store.new(), synthetic_identity.new(), document)
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

pub fn bulk_selector_validation_matches_captured_code_roots_test() {
  let outcome =
    run_mutation(
      "mutation { activateMissing: discountCodeBulkActivate { userErrors { field message code extraInfo } } activateBlank: discountCodeBulkActivate(search: \"\") { userErrors { field message code extraInfo } } activateSaved: discountCodeBulkActivate(savedSearchId: \"gid://shopify/SavedSearch/0\") { userErrors { field message code extraInfo } } deactivateTooMany: discountCodeBulkDeactivate(ids: [\"gid://shopify/DiscountCodeNode/0\"], search: \"status:active\") { userErrors { field message code extraInfo } } deleteMissing: discountCodeBulkDelete { userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"activateMissing\":{\"userErrors\":[{\"field\":null,\"message\":\"Missing expected argument key: 'ids', 'search' or 'saved_search_id'.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]},\"activateBlank\":{\"userErrors\":[{\"field\":[\"search\"],\"message\":\"'Search' can't be blank.\",\"code\":\"BLANK\",\"extraInfo\":null}]},\"activateSaved\":{\"userErrors\":[{\"field\":[\"savedSearchId\"],\"message\":\"Invalid 'saved_search_id'.\",\"code\":\"INVALID\",\"extraInfo\":null}]},\"deactivateTooMany\":{\"userErrors\":[{\"field\":null,\"message\":\"Only one of 'ids', 'search' or 'saved_search_id' is allowed.\",\"code\":\"TOO_MANY_ARGUMENTS\",\"extraInfo\":null}]},\"deleteMissing\":{\"userErrors\":[{\"field\":null,\"message\":\"Missing expected argument key: 'ids', 'search' or 'saved_search_id'.\",\"code\":\"MISSING_ARGUMENT\",\"extraInfo\":null}]}}}"
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

pub fn code_basic_rejects_cart_line_tag_overlap_as_bad_request_test() {
  let outcome =
    run_mutation(
      "mutation { tagOverlap: discountCodeBasicCreate(basicCodeDiscount: { title: \"Overlap invalid\", code: \"TAG-OVERLAP\", startsAt: \"2026-05-05T00:00:00Z\", combinesWith: { productDiscountsWithTagsOnSameCartLine: { add: [\"same\"], remove: [\"same\"] } }, customerGets: { value: { percentage: 0.1 }, items: { products: { productsToAdd: [\"gid://shopify/Product/1\"] } } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"The same tag is present in both `add` and `remove` fields of `productDiscountsWithTagsOnSameCartLine`.\",\"locations\":[{\"line\":1,\"column\":12}],\"extensions\":{\"code\":\"BAD_REQUEST\"},\"path\":[\"tagOverlap\"]}],\"data\":{\"tagOverlap\":null}}"
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

pub fn activate_missing_app_function_returns_internal_error_test() {
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
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount could not be activated.\",\"code\":\"INTERNAL_ERROR\"}]}}}"
}

pub fn activate_unknown_discount_uses_invalid_error_code_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/0\") { codeDiscountNode { id } userErrors { field message code } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeActivate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Discount does not exist\",\"code\":\"INVALID\"}]}}}"
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

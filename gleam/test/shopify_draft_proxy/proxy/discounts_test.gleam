//// Focused mutation/read tests for `proxy/discounts`.

import gleam/dict
import gleam/json
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

fn run_mutation(document: String) -> discounts.MutationOutcome {
  let assert Ok(outcome) =
    discounts.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      dict.new(),
    )
  outcome
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

pub fn blank_bxgy_returns_captured_user_errors_test() {
  let outcome =
    run_mutation(
      "mutation { discountCodeBxgyCreate(bxgyCodeDiscount: { title: \"\", code: \"BXGY\", startsAt: \"2026-04-25T00:00:00Z\", customerBuys: { value: { quantity: \"1\" }, items: { all: true } }, customerGets: { value: { discountOnQuantity: { quantity: \"1\", effect: { percentage: 1 } } }, items: { all: true } } }) { codeDiscountNode { id } userErrors { field message code extraInfo } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"discountCodeBxgyCreate\":{\"codeDiscountNode\":null,\"userErrors\":[{\"field\":[\"bxgyCodeDiscount\",\"customerGets\"],\"message\":\"Items in 'customer get' cannot be set to all\",\"code\":\"INVALID\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\",\"extraInfo\":null},{\"field\":[\"bxgyCodeDiscount\",\"customerBuys\",\"items\"],\"message\":\"Items in 'customer buys' must be defined\",\"code\":\"BLANK\",\"extraInfo\":null}]}}}"
}

pub fn inline_null_input_returns_top_level_error_test() {
  let outcome =
    run_mutation(
      "mutation DiscountCodeBasicCreateInlineNullInput { discountCodeBasicCreate(basicCodeDiscount: null) { codeDiscountNode { id } userErrors { message } } }",
    )

  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'basicCodeDiscount' on Field 'discountCodeBasicCreate' has an invalid value (null). Expected type 'DiscountCodeBasicInput!'.\",\"locations\":[{\"line\":1,\"column\":51}],\"path\":[\"mutation DiscountCodeBasicCreateInlineNullInput\",\"discountCodeBasicCreate\",\"basicCodeDiscount\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"basicCodeDiscount\"}}]}"
}

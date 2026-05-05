import gleam/list
import parity/diff
import parity/json_value.{JString}

pub fn shop_policy_url_base_matcher_accepts_primary_domain_fallback_test() {
  let mismatches =
    diff.compare_payloads(
      JString(
        "https://checkout.shopify.com/92891250994/policies/44470108466.html?locale=en",
      ),
      JString(
        "https://harry-test-heelo.myshopify.com/92891250994/policies/1.html?locale=en",
      ),
      [
        diff.expected_match(
          "$",
          "shop-policy-url-base:https://harry-test-heelo.myshopify.com",
        ),
      ],
    )

  assert mismatches == []
}

pub fn shop_policy_url_base_matcher_rejects_non_policy_paths_test() {
  let mismatches =
    diff.compare_payloads(
      JString(
        "https://checkout.shopify.com/92891250994/policies/44470108466.html?locale=en",
      ),
      JString("https://harry-test-heelo.myshopify.com/policies/privacy-policy"),
      [
        diff.expected_match(
          "$",
          "shop-policy-url-base:https://harry-test-heelo.myshopify.com",
        ),
      ],
    )

  assert list.length(mismatches) == 1
}

pub fn exact_string_matcher_asserts_proxy_value_test() {
  let mismatches =
    diff.compare_payloads(JString("Privacy policy"), JString("Privacy Policy"), [
      diff.expected_match("$", "exact-string:Privacy Policy"),
    ])

  assert mismatches == []
}

import gleam/list
import gleam/option.{None, Some}
import gleam/order.{Eq, Gt, Lt}
import shopify_draft_proxy/shopify/resource_ids

pub fn compare_resource_ids_numeric_when_both_parse_test() {
  assert resource_ids.compare_shopify_resource_ids(
      "gid://shopify/Product/2",
      "gid://shopify/Product/10",
    )
    == Lt
  assert resource_ids.compare_shopify_resource_ids(
      "gid://shopify/Product/10",
      "gid://shopify/Product/2",
    )
    == Gt
  assert resource_ids.compare_shopify_resource_ids(
      "gid://shopify/Product/5",
      "gid://shopify/Product/5",
    )
    == Eq
}

pub fn compare_resource_ids_numeric_across_types_test() {
  // Numeric compare wins even across different resource types — the
  // TS implementation only inspects the trailing integer.
  assert resource_ids.compare_shopify_resource_ids(
      "gid://shopify/Product/2",
      "gid://shopify/Order/10",
    )
    == Lt
}

pub fn compare_resource_ids_lexicographic_fallback_test() {
  // Tail isn't an int → falls back to lexicographic compare.
  assert resource_ids.compare_shopify_resource_ids("foo", "bar") == Gt
  assert resource_ids.compare_shopify_resource_ids("apple", "banana") == Lt
  assert resource_ids.compare_shopify_resource_ids("same", "same") == Eq
}

pub fn compare_resource_ids_one_side_parses_falls_back_test() {
  // Only one side has an int tail → still lexicographic compare.
  assert resource_ids.compare_shopify_resource_ids(
      "gid://shopify/Product/1",
      "gid://shopify/Product/abc",
    )
    == Lt
}

pub fn compare_nullable_strings_some_some_test() {
  assert resource_ids.compare_nullable_strings(Some("a"), Some("b")) == Lt
  assert resource_ids.compare_nullable_strings(Some("b"), Some("a")) == Gt
  assert resource_ids.compare_nullable_strings(Some("a"), Some("a")) == Eq
}

pub fn compare_nullable_strings_some_sorts_before_none_test() {
  assert resource_ids.compare_nullable_strings(Some("a"), None) == Lt
  assert resource_ids.compare_nullable_strings(None, Some("a")) == Gt
  assert resource_ids.compare_nullable_strings(None, None) == Eq
}

pub fn compare_resource_ids_usable_with_list_sort_test() {
  let sorted =
    list.sort(
      [
        "gid://shopify/Product/10",
        "gid://shopify/Product/2",
        "gid://shopify/Product/100",
      ],
      resource_ids.compare_shopify_resource_ids,
    )
  assert sorted
    == [
      "gid://shopify/Product/2",
      "gid://shopify/Product/10",
      "gid://shopify/Product/100",
    ]
}

pub fn compare_nullable_strings_usable_with_list_sort_test() {
  let sorted =
    list.sort(
      [None, Some("c"), Some("a"), None, Some("b")],
      resource_ids.compare_nullable_strings,
    )
  assert sorted == [Some("a"), Some("b"), Some("c"), None, None]
}

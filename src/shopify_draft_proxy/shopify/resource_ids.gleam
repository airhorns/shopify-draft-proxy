//// Mirrors `src/shopify/resource-ids.ts`.
////
//// Two ordering helpers that several domain serializers reach for:
//// - `compare_shopify_resource_ids` — sort GIDs (`gid://shopify/X/123`)
//// numerically by the trailing integer when both ends parse, falling
//// back to lexicographic compare.
//// - `compare_nullable_strings` — sort with explicit `Some`/`None`
//// nullable handling: `None` sorts last, otherwise lexicographic.
////
//// Both return the conventional `Lt`/`Eq`/`Gt` instead of TS's signed
//// integer; callers pair with `list.sort` directly.

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order, Eq, Gt, Lt}
import gleam/string

const shopify_gid_prefix: String = "gid://shopify/"

/// Build the canonical Shopify GID for a resource type from either a
/// fully-qualified GID, a numeric tail, or a non-numeric opaque tail.
pub fn canonical_shopify_resource_gid(
  resource_type: String,
  id: String,
) -> String {
  let tail = option.unwrap(shopify_gid_tail(id), id)
  shopify_gid_prefix <> resource_type <> "/" <> tail
}

/// Return the final path segment of a Shopify GID or bare ID. Query-string
/// suffixes are ignored because Shopify GIDs sometimes carry opaque metadata.
pub fn shopify_gid_tail(id: String) -> Option(String) {
  let without_query = case string.split_once(id, "?") {
    Ok(#(prefix, _)) -> prefix
    Error(_) -> id
  }
  case list.last(string.split(without_query, "/")) {
    Ok(tail) if tail != "" -> Some(tail)
    _ -> None
  }
}

/// Compare two Shopify GID strings. Tries to extract the trailing
/// integer (`gid://shopify/Foo/123` → `123`); when both sides parse,
/// compares numerically. Falls back to lexicographic compare.
pub fn compare_shopify_resource_ids(left: String, right: String) -> Order {
  case extract_tail_int(left), extract_tail_int(right) {
    Some(a), Some(b) ->
      case int.compare(a, b) {
        Eq -> string.compare(left, right)
        order -> order
      }
    _, _ -> string.compare(left, right)
  }
}

fn extract_tail_int(id: String) -> Option(Int) {
  use tail <- option.then(shopify_gid_tail(id))
  case int.parse(tail) {
    Ok(n) -> Some(n)
    Error(_) -> None
  }
}

/// Compare two `Option(String)` values: `None` sorts after `Some(_)`;
/// two `Some(_)` values compare lexicographically.
pub fn compare_nullable_strings(
  left: Option(String),
  right: Option(String),
) -> Order {
  case left, right {
    Some(a), Some(b) -> string.compare(a, b)
    Some(_), None -> Lt
    None, Some(_) -> Gt
    None, None -> Eq
  }
}

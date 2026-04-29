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

/// Compare two Shopify GID strings. Tries to extract the trailing
/// integer (`gid://shopify/Foo/123` → `123`); when both sides parse,
/// compares numerically. Falls back to lexicographic compare.
pub fn compare_shopify_resource_ids(left: String, right: String) -> Order {
  case extract_tail_int(left), extract_tail_int(right) {
    Some(a), Some(b) -> int.compare(a, b)
    _, _ -> string.compare(left, right)
  }
}

fn extract_tail_int(id: String) -> Option(Int) {
  let segments = string.split(id, "/")
  case list.last(segments) {
    Ok(tail) ->
      case int.parse(tail) {
        Ok(n) -> Some(n)
        Error(_) -> None
      }
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

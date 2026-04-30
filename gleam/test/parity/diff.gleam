//// Structural JSON diff with `expectedDifferences` matcher support.
////
//// The parity engine compares two JsonValue trees and reports each
//// mismatch with a JSONPath-style location. Mismatches that match a
//// pre-declared `expectedDifferences` rule are filtered out before the
//// diff is reported.
////
//// Supported matchers (mirrors the TS engine's `diff-matchers.ts`):
////   * "shopify-gid:<Type>"  – any string that looks like a Shopify gid
////                             of the given type, optionally with the
////                             `?shopify-draft-proxy=synthetic` marker.
////   * "non-empty-string"    – any non-empty string.
////   * "any-string"          – any string.
////   * "any-number"          – any int or float.
////
//// Anything else is treated as an exact-string match against the actual
//// value (when the actual is itself a string).

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import parity/json_value.{
  type JsonValue, JArray, JBool, JFloat, JInt, JNull, JObject, JString,
}

pub type Mismatch {
  Mismatch(path: String, expected: String, actual: String)
}

pub type ExpectedDifference {
  /// Skip diff at this path entirely.
  IgnoreDifference(path: String)
  /// Diff at this path is allowed iff actual value satisfies matcher.
  MatcherDifference(path: String, matcher: String)
}

/// Backwards-compatible factory used by the old single-shape API.
pub fn expected_match(path: String, matcher: String) -> ExpectedDifference {
  MatcherDifference(path: path, matcher: matcher)
}

pub fn expected_ignore(path: String) -> ExpectedDifference {
  IgnoreDifference(path: path)
}

/// Compare two JsonValue trees. Returns a list of mismatches; empty
/// means structurally equal.
pub fn diff(expected: JsonValue, actual: JsonValue) -> List(Mismatch) {
  diff_at(expected, actual, "$", [])
  |> list.reverse
}

/// Compare two JsonValue trees, filtering out any mismatch whose path
/// matches one of the supplied `expectedDifferences` and whose actual
/// value satisfies the rule's matcher.
pub fn diff_with_expected(
  expected: JsonValue,
  actual: JsonValue,
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  diff(expected, actual)
  |> list.filter(fn(m) { !is_expected(m, rules, actual) })
}

fn is_expected(
  m: Mismatch,
  rules: List(ExpectedDifference),
  actual_root: JsonValue,
) -> Bool {
  list.any(rules, fn(rule) {
    case rule {
      IgnoreDifference(path: path) -> path_matches(path, m.path)
      MatcherDifference(path: path, matcher: matcher) ->
        path_matches(path, m.path)
        && satisfies_matcher(actual_root, m.path, matcher)
    }
  })
}

fn path_matches(pattern: String, path: String) -> Bool {
  case pattern == path {
    True -> True
    False ->
      string.starts_with(path, pattern <> ".")
      || string.starts_with(path, pattern <> "[")
      || wildcard_path_matches(pattern, path)
  }
}

fn wildcard_path_matches(pattern: String, path: String) -> Bool {
  case string.split(pattern, on: "[*]") {
    [prefix, suffix] ->
      string.starts_with(path, prefix)
      && string.ends_with(path, suffix)
      && wildcard_index_segment(path, prefix, suffix)
    _ -> False
  }
}

fn wildcard_index_segment(
  path: String,
  prefix: String,
  suffix: String,
) -> Bool {
  let middle_start = string.length(prefix)
  let middle_end = string.length(path) - string.length(suffix)
  case middle_end > middle_start {
    True -> {
      let middle = string.slice(path, middle_start, middle_end - middle_start)
      string.starts_with(middle, "[") && string.ends_with(middle, "]")
    }
    False -> False
  }
}

fn satisfies_matcher(
  actual_root: JsonValue,
  path: String,
  matcher: String,
) -> Bool {
  case parity_lookup(actual_root, path) {
    None -> False
    Some(value) -> value_matches(value, matcher)
  }
}

fn parity_lookup(value: JsonValue, path: String) -> Option(JsonValue) {
  // Lazy import-by-rebuild to avoid a cyclic dep on the runner.
  case path {
    "$" -> Some(value)
    "$." <> _ | "$[" <> _ -> walk(value, path)
    _ -> None
  }
}

fn walk(value: JsonValue, path: String) -> Option(JsonValue) {
  // We re-implement the JSONPath walk locally so this module stays
  // self-contained. The grammar is the same as parity/jsonpath but
  // restricted to evaluation only.
  case path {
    "$" -> Some(value)
    "$" <> rest -> walk_steps(value, rest)
    _ -> None
  }
}

fn walk_steps(value: JsonValue, rest: String) -> Option(JsonValue) {
  case rest {
    "" -> Some(value)
    "." <> tail -> walk_field(value, tail)
    "[" <> tail -> walk_index(value, tail)
    _ -> None
  }
}

fn walk_field(value: JsonValue, rest: String) -> Option(JsonValue) {
  let #(name, tail) = take_until_delim(rest, "")
  case name, json_value.field(value, name) {
    "", _ -> None
    _, None -> None
    _, Some(next) -> walk_steps(next, tail)
  }
}

fn walk_index(value: JsonValue, rest: String) -> Option(JsonValue) {
  let #(digits, after) = take_digits(rest, "")
  case digits, after {
    "", _ -> None
    _, "]" <> tail ->
      case parse_int(digits) {
        Ok(n) ->
          case json_value.index(value, n) {
            Some(next) -> walk_steps(next, tail)
            None -> None
          }
        Error(_) -> None
      }
    _, _ -> None
  }
}

fn take_until_delim(input: String, acc: String) -> #(String, String) {
  case string.pop_grapheme(input) {
    Error(_) -> #(acc, "")
    Ok(#(g, rest)) ->
      case g {
        "." | "[" -> #(acc, g <> rest)
        _ -> take_until_delim(rest, acc <> g)
      }
  }
}

fn take_digits(input: String, acc: String) -> #(String, String) {
  case string.pop_grapheme(input) {
    Error(_) -> #(acc, "")
    Ok(#(g, rest)) ->
      case is_digit(g) {
        True -> take_digits(rest, acc <> g)
        False -> #(acc, g <> rest)
      }
  }
}

fn is_digit(g: String) -> Bool {
  case g {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn parse_int(s: String) -> Result(Int, Nil) {
  int.parse(s)
}

fn value_matches(value: JsonValue, matcher: String) -> Bool {
  case matcher {
    "non-empty-string" ->
      case value {
        JString(s) -> string.length(s) > 0
        _ -> False
      }
    "any-string" ->
      case value {
        JString(_) -> True
        _ -> False
      }
    "any-number" ->
      case value {
        JInt(_) | JFloat(_) -> True
        _ -> False
      }
    "iso-timestamp" ->
      case value {
        JString(s) -> looks_like_iso_timestamp(s)
        _ -> False
      }
    "shopify-gid:" <> ty ->
      case value {
        JString(s) -> is_shopify_gid(s, ty)
        _ -> False
      }
    _ ->
      case value {
        JString(s) -> s == matcher
        _ -> False
      }
  }
}

/// Permissive ISO-8601 check: matches `YYYY-MM-DDTHH:MM:SS` with an
/// optional fractional second and a `Z` or `±HH:MM` offset. Good enough
/// for the parity surface — Shopify always emits a `Z`.
fn looks_like_iso_timestamp(s: String) -> Bool {
  case string.length(s) >= 20 {
    False -> False
    True ->
      string.contains(s, "T")
      && { string.ends_with(s, "Z") || has_offset(s) }
      && all_chars_iso(s)
  }
}

fn has_offset(s: String) -> Bool {
  string.contains(s, "+") || iso_minus_offset(s)
}

fn iso_minus_offset(s: String) -> Bool {
  // The leading date already contains '-' chars, so a trailing '-HH:MM'
  // means there's a '-' after position 10. Cheap heuristic.
  case string.length(s) >= 16 {
    False -> False
    True -> {
      let suffix = string.slice(s, 10, string.length(s) - 10)
      string.contains(suffix, "-")
    }
  }
}

fn all_chars_iso(s: String) -> Bool {
  s
  |> string.to_graphemes
  |> list.all(is_iso_char)
}

fn is_iso_char(g: String) -> Bool {
  case g {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "-" | ":" | "T" | "Z" | "." | "+" -> True
    _ -> False
  }
}

fn is_shopify_gid(s: String, ty: String) -> Bool {
  let prefix = "gid://shopify/" <> ty <> "/"
  case string.starts_with(s, prefix) {
    False -> False
    True -> {
      let rest = string.drop_start(s, string.length(prefix))
      let id_part = case string.split_once(rest, "?") {
        Ok(#(id, _query)) -> id
        Error(_) -> rest
      }
      string.length(id_part) > 0
    }
  }
}

fn diff_at(
  expected: JsonValue,
  actual: JsonValue,
  path: String,
  acc: List(Mismatch),
) -> List(Mismatch) {
  case expected, actual {
    JNull, JNull -> acc
    JBool(a), JBool(b) if a == b -> acc
    JInt(a), JInt(b) if a == b -> acc
    JFloat(a), JFloat(b) if a == b -> acc
    JString(a), JString(b) if a == b -> acc
    JArray(a), JArray(b) -> diff_arrays(a, b, path, 0, acc)
    JObject(a), JObject(b) -> diff_objects(a, b, path, acc)
    _, _ -> [
      Mismatch(
        path: path,
        expected: json_value.to_string(expected),
        actual: json_value.to_string(actual),
      ),
      ..acc
    ]
  }
}

fn diff_arrays(
  expected: List(JsonValue),
  actual: List(JsonValue),
  path: String,
  idx: Int,
  acc: List(Mismatch),
) -> List(Mismatch) {
  case expected, actual {
    [], [] -> acc
    [], _ -> [
      Mismatch(
        path: path <> "[" <> int.to_string(idx) <> "]",
        expected: "<missing>",
        actual: json_value.to_string(JArray(actual)),
      ),
      ..acc
    ]
    _, [] -> [
      Mismatch(
        path: path <> "[" <> int.to_string(idx) <> "]",
        expected: json_value.to_string(JArray(expected)),
        actual: "<missing>",
      ),
      ..acc
    ]
    [eh, ..et], [ah, ..at] -> {
      let acc = diff_at(eh, ah, path <> "[" <> int.to_string(idx) <> "]", acc)
      diff_arrays(et, at, path, idx + 1, acc)
    }
  }
}

fn diff_objects(
  expected: List(#(String, JsonValue)),
  actual: List(#(String, JsonValue)),
  path: String,
  acc: List(Mismatch),
) -> List(Mismatch) {
  let expected_keys = list.map(expected, fn(p) { p.0 })
  let actual_keys = list.map(actual, fn(p) { p.0 })
  let acc =
    list.fold(expected, acc, fn(a, pair) {
      let #(key, eval) = pair
      case list.find(actual, fn(p) { p.0 == key }) {
        Ok(found) -> diff_at(eval, found.1, path <> "." <> key, a)
        Error(_) -> [
          Mismatch(
            path: path <> "." <> key,
            expected: json_value.to_string(eval),
            actual: "<missing>",
          ),
          ..a
        ]
      }
    })
  list.fold(actual_keys, acc, fn(a, key) {
    case list.contains(expected_keys, key) {
      True -> a
      False -> {
        let assert Ok(found) = list.find(actual, fn(p) { p.0 == key })
        [
          Mismatch(
            path: path <> "." <> key,
            expected: "<missing>",
            actual: json_value.to_string(found.1),
          ),
          ..a
        ]
      }
    }
  })
}

/// Render a list of mismatches into a multi-line string suitable for a
/// failed-test message.
pub fn render_mismatches(mismatches: List(Mismatch)) -> String {
  list.map(mismatches, fn(m) {
    "  at "
    <> m.path
    <> "\n    expected: "
    <> m.expected
    <> "\n    actual:   "
    <> m.actual
  })
  |> string.join("\n")
}

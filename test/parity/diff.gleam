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

/// Compare two JsonValue trees AND verify that every applicable
/// `expectedDifferences` rule actually corresponded to a real diff.
/// Mirrors `compareJsonPayloads` in the TS runner (`scripts/conformance-parity-lib.ts`).
///
/// Filters matchable mismatches the same way `diff_with_expected` does,
/// then appends a synthetic "expected difference was not observed"
/// mismatch for every rule whose path resolves in either tree but
/// whose path does not appear in the raw (pre-filter) mismatch list.
pub fn compare_payloads(
  expected: JsonValue,
  actual: JsonValue,
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  let raw = diff(expected, actual)
  let filtered = list.filter(raw, fn(m) { !is_expected(m, rules, actual) })
  let unobserved = unobserved_rule_mismatches(expected, actual, raw, rules)
  list.append(filtered, unobserved)
}

/// Compare only the selected JSONPath slices. Paths are relative to
/// the target's capture/proxy slice, matching the `selectedPaths`
/// contract in checked-in parity specs.
pub fn diff_selected_paths(
  expected: JsonValue,
  actual: JsonValue,
  selected_paths: List(String),
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  selected_paths
  |> list.fold([], fn(acc, path) {
    let path_mismatches = diff_selected_path(expected, actual, path, rules)
    list.append(path_mismatches, acc)
  })
  |> list.reverse
}

/// Like `diff_selected_paths`, but also synthesises "expected difference
/// was not observed" entries for rules that resolve in any slice's
/// expected/actual subtree but never produced a raw mismatch.
/// Aggregates across slices so a single unobserved entry is emitted
/// per rule, never duplicated per slice.
pub fn compare_selected_paths(
  expected: JsonValue,
  actual: JsonValue,
  selected_paths: List(String),
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  let slice_results =
    list.map(selected_paths, fn(path) {
      slice_diff(expected, actual, path, rules)
    })
  let filtered =
    slice_results
    |> list.flat_map(fn(r) { r.filtered })
  let raw_aggregate =
    slice_results
    |> list.flat_map(fn(r) { r.raw })
  let expected_subtrees =
    slice_results
    |> list.filter_map(fn(r) {
      case r.expected_subtree {
        Some(v) -> Ok(v)
        None -> Error(Nil)
      }
    })
  let actual_subtrees =
    slice_results
    |> list.filter_map(fn(r) {
      case r.actual_subtree {
        Some(v) -> Ok(v)
        None -> Error(Nil)
      }
    })
  let unobserved =
    unobserved_rule_mismatches_multi(
      expected_subtrees,
      actual_subtrees,
      raw_aggregate,
      rules,
    )
  list.append(filtered, unobserved)
}

type SliceResult {
  SliceResult(
    raw: List(Mismatch),
    filtered: List(Mismatch),
    expected_subtree: Option(JsonValue),
    actual_subtree: Option(JsonValue),
  )
}

fn slice_diff(
  expected: JsonValue,
  actual: JsonValue,
  path: String,
  rules: List(ExpectedDifference),
) -> SliceResult {
  case parity_lookup(expected, path), parity_lookup(actual, path) {
    None, None ->
      SliceResult(
        raw: [],
        filtered: [],
        expected_subtree: None,
        actual_subtree: None,
      )
    None, Some(actual_value) -> {
      let m = [
        Mismatch(
          path: path,
          expected: "<missing>",
          actual: json_value.to_string(actual_value),
        ),
      ]
      SliceResult(
        raw: m,
        filtered: m,
        expected_subtree: None,
        actual_subtree: Some(actual_value),
      )
    }
    Some(expected_value), None -> {
      let m = [
        Mismatch(
          path: path,
          expected: json_value.to_string(expected_value),
          actual: "<missing>",
        ),
      ]
      SliceResult(
        raw: m,
        filtered: m,
        expected_subtree: Some(expected_value),
        actual_subtree: None,
      )
    }
    Some(expected_value), Some(actual_value) -> {
      let raw = diff(expected_value, actual_value)
      let filtered =
        raw
        |> list.filter(fn(m) { !is_expected(m, rules, actual_value) })
      let rebase = fn(m: Mismatch) {
        Mismatch(
          path: path <> selected_suffix(m.path),
          expected: m.expected,
          actual: m.actual,
        )
      }
      // Keep raw RELATIVE to the slice — rules are interpreted relative
      // to the slice subtree (consistent with `is_expected` above), so
      // unobserved checks must compare against relative paths.
      SliceResult(
        raw: raw,
        filtered: list.map(filtered, rebase),
        expected_subtree: Some(expected_value),
        actual_subtree: Some(actual_value),
      )
    }
  }
}

fn diff_selected_path(
  expected: JsonValue,
  actual: JsonValue,
  path: String,
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  case parity_lookup(expected, path), parity_lookup(actual, path) {
    None, None -> []
    None, Some(actual_value) -> [
      Mismatch(
        path: path,
        expected: "<missing>",
        actual: json_value.to_string(actual_value),
      ),
    ]
    Some(expected_value), None -> [
      Mismatch(
        path: path,
        expected: json_value.to_string(expected_value),
        actual: "<missing>",
      ),
    ]
    Some(expected_value), Some(actual_value) ->
      diff_with_expected(expected_value, actual_value, rules)
      |> list.map(fn(m) {
        Mismatch(
          path: path <> selected_suffix(m.path),
          expected: m.expected,
          actual: m.actual,
        )
      })
  }
}

fn selected_suffix(path: String) -> String {
  case path {
    "$" -> ""
    "$" <> rest -> rest
    _ -> path
  }
}

/// Synthesise "expected difference was not observed" mismatches for
/// every rule whose path resolves in `expected` or `actual` but never
/// matched any of the supplied raw mismatches. Mirrors the
/// `applicableRuleIndexes` / `observedRuleIndexes` logic in TS
/// `compareJsonPayloads` (`scripts/conformance-parity-lib.ts:797`).
fn unobserved_rule_mismatches(
  expected: JsonValue,
  actual: JsonValue,
  raw_mismatches: List(Mismatch),
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  unobserved_rule_mismatches_multi([expected], [actual], raw_mismatches, rules)
}

fn unobserved_rule_mismatches_multi(
  expected_trees: List(JsonValue),
  actual_trees: List(JsonValue),
  raw_mismatches: List(Mismatch),
  rules: List(ExpectedDifference),
) -> List(Mismatch) {
  let all_paths =
    list.append(
      list.flat_map(expected_trees, fn(v) { enumerate_paths(v, "$") }),
      list.flat_map(actual_trees, fn(v) { enumerate_paths(v, "$") }),
    )
  list.filter_map(rules, fn(rule) {
    let rule_path = expected_difference_path(rule)
    case
      rule_path_applicable(rule_path, all_paths),
      rule_path_observed(rule_path, raw_mismatches)
    {
      True, False ->
        Ok(Mismatch(
          path: rule_path,
          expected: "<expectedDifference rule was not satisfied>",
          actual: "(no diff at this path)",
        ))
      _, _ -> Error(Nil)
    }
  })
}

fn expected_difference_path(rule: ExpectedDifference) -> String {
  case rule {
    IgnoreDifference(path: path) -> path
    MatcherDifference(path: path, matcher: _) -> path
  }
}

fn rule_path_applicable(rule_path: String, all_paths: List(String)) -> Bool {
  let normalized_rule = normalize_path(rule_path)
  list.any(all_paths, fn(p) {
    let np = normalize_path(p)
    case normalized_rule == np {
      True -> True
      False ->
        case string.contains(normalized_rule, "[*]") {
          True -> wildcard_path_matches(normalized_rule, np)
          False -> False
        }
    }
  })
}

fn rule_path_observed(
  rule_path: String,
  raw_mismatches: List(Mismatch),
) -> Bool {
  list.any(raw_mismatches, fn(m) { path_matches(rule_path, m.path) })
}

fn enumerate_paths(value: JsonValue, prefix: String) -> List(String) {
  case value {
    JObject(entries) -> {
      let child_paths =
        list.flat_map(entries, fn(pair) {
          let #(key, child) = pair
          enumerate_paths(child, prefix <> "." <> key)
        })
      [prefix, ..child_paths]
    }
    JArray(items) -> {
      let child_paths =
        items
        |> list.index_map(fn(child, idx) {
          enumerate_paths(child, prefix <> "[" <> int.to_string(idx) <> "]")
        })
        |> list.flatten
      [prefix, ..child_paths]
    }
    _ -> [prefix]
  }
}

fn is_expected(
  m: Mismatch,
  rules: List(ExpectedDifference),
  actual_root: JsonValue,
) -> Bool {
  list.any(rules, fn(rule) {
    case rule {
      IgnoreDifference(path: path) ->
        path_matches(path, m.path) || path_is_child_of(path, m.path)
      MatcherDifference(path: path, matcher: matcher) ->
        path_matches(path, m.path)
        && satisfies_matcher(actual_root, m.path, matcher)
    }
  })
}

fn path_is_child_of(parent: String, child: String) -> Bool {
  let parent = normalize_path(parent)
  let child = normalize_path(child)
  string.starts_with(child, parent <> ".")
  || string.starts_with(child, parent <> "[")
}

fn path_matches(pattern: String, path: String) -> Bool {
  let pattern = normalize_path(pattern)
  let path = normalize_path(path)
  case pattern == path {
    True -> True
    False ->
      string.starts_with(path, pattern <> ".")
      || string.starts_with(path, pattern <> "[")
      || wildcard_path_matches(pattern, path)
  }
}

fn normalize_path(path: String) -> String {
  path
  |> string.replace("[\"nodes\"]", ".nodes")
  |> string.replace("[\"edges\"]", ".edges")
}

fn wildcard_path_matches(pattern: String, path: String) -> Bool {
  case string.split_once(pattern, "[*]") {
    Ok(#(prefix, suffix_pattern)) ->
      string.starts_with(path, prefix)
      && {
        let rest = string.drop_start(path, string.length(prefix))
        case consume_wildcard_index(rest) {
          Some(suffix_path) ->
            wildcard_path_matches(suffix_pattern, suffix_path)
          None -> False
        }
      }
    Error(_) -> pattern == path
  }
}

fn consume_wildcard_index(path: String) -> Option(String) {
  case path {
    "[" <> tail -> {
      let #(digits, after_digits) = take_digits(tail, "")
      case digits, after_digits {
        "", _ -> None
        _, "]" <> suffix -> Some(suffix)
        _, _ -> None
      }
    }
    _ -> None
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
    "shop-policy-url-base:" <> base ->
      case value {
        JString(s) -> is_shop_policy_url(s, base)
        _ -> False
      }
    "exact-string:" <> expected ->
      case value {
        JString(s) -> s == expected
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

fn is_shop_policy_url(s: String, base: String) -> Bool {
  let normalized_base = case string.ends_with(base, "/") {
    True -> string.drop_end(base, 1)
    False -> base
  }
  let prefix = normalized_base <> "/"
  case string.starts_with(s, prefix) {
    False -> False
    True -> {
      let rest = string.drop_start(s, string.length(prefix))
      let #(shop_tail, after_shop_tail) = take_digits(rest, "")
      case shop_tail, string.starts_with(after_shop_tail, "/policies/") {
        "", _ -> False
        _, False -> False
        _, True -> {
          let policy_part = string.drop_start(after_shop_tail, 10)
          let #(policy_tail, suffix) = take_digits(policy_part, "")
          policy_tail != "" && suffix == ".html?locale=en"
        }
      }
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

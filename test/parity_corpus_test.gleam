//// Mirrors `tests/unit/conformance-parity-scenarios.test.ts`'s
//// scaffolding pass — the bit that *discovers* every spec under
//// `config/parity-specs/**` and asserts the classification partition
//// is what we expect. Per-scenario execution is handled by
//// `parity_test.gleam`, which discovers and runs the full corpus.

import gleam/dict
import gleam/list
import gleam/string
import parity/classify
import parity/discover
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import simplifile

const parity_root: String = "config/parity-specs"

const parity_request_root: String = "config/parity-requests"

pub fn corpus_discovers_all_specs_test() {
  let assert Ok(files) = discover.discover(parity_root)
  // The TS side ships 379 specs today; assert a generous lower bound
  // so adding more specs doesn't tip the test red. Drop the guard
  // entirely if specs ever go below 200.
  assert list.length(files) >= 379
}

pub fn corpus_partition_is_all_ready_for_comparison_test() {
  let assert Ok(files) = discover.discover(parity_root)
  let states = classify_all(files)
  // Today every checked-in spec is `captured` + `captured-vs-proxy-request`,
  // so the entire corpus must classify as `ReadyForComparison`. If a
  // spec ever flips to `captured-fixture` or `planned`, this test
  // fails loudly so the corpus doesn't silently shrink.
  let invalid =
    list.filter(states, fn(state) {
      state == classify.InvalidMissingComparisonContract
    })
  let not_implemented =
    list.filter(states, fn(state) { state == classify.NotYetImplemented })
  assert invalid == []
  assert not_implemented == []
}

pub fn corpus_every_spec_decodes_test() {
  let assert Ok(files) = discover.discover(parity_root)
  let failures =
    list.filter_map(files, fn(path) {
      case simplifile.read(path) {
        Error(_) -> Ok(path)
        Ok(source) ->
          case classify.parse(path, source) {
            Ok(_) -> Error(Nil)
            Error(_) -> Ok(path)
          }
      }
    })
  case failures {
    [] -> Nil
    _ -> {
      let count = list.length(failures)
      panic as { "specs failed to decode: " <> int_to_string(count) }
    }
  }
}

pub fn parity_requests_parse_and_resolve_root_arguments_test() {
  let assert Ok(all_files) = simplifile.get_files(parity_request_root)
  let files =
    list.filter(all_files, fn(path) { string.ends_with(path, ".graphql") })
  assert list.length(files) >= 750
  let failures =
    list.filter_map(files, fn(path) {
      case simplifile.read(path) {
        Error(_) -> Ok(path <> ": read failed")
        Ok(source) ->
          case parse_operation.parse_operation(source) {
            Error(_) -> Ok(path <> ": parse/classify failed")
            Ok(_) ->
              case root_field.get_root_fields(source) {
                Error(_) -> Ok(path <> ": root fields failed")
                Ok(fields) ->
                  case root_arguments_ok(fields) {
                    True -> Error(Nil)
                    False -> Ok(path <> ": root arguments failed")
                  }
              }
          }
      }
    })
  case failures {
    [] -> Nil
    [first, ..] -> {
      let count = list.length(failures)
      panic as {
        "parity requests failed GraphQL substrate check: "
        <> int_to_string(count)
        <> "; first: "
        <> first
      }
    }
  }
}

fn root_arguments_ok(fields: List(Selection)) -> Bool {
  list.all(fields, fn(field) {
    case root_field.get_field_arguments(field, dict.new()) {
      Ok(_) -> True
      Error(_) -> False
    }
  })
}

fn classify_all(files: List(String)) -> List(classify.ScenarioState) {
  list.filter_map(files, fn(path) {
    case simplifile.read(path) {
      Error(_) -> Error(Nil)
      Ok(source) ->
        case classify.parse(path, source) {
          Ok(spec) -> Ok(classify.classify(spec))
          Error(_) -> Error(Nil)
        }
    }
  })
}

fn int_to_string(n: Int) -> String {
  case n {
    0 -> "0"
    _ -> int_to_string_loop(n, "")
  }
}

fn int_to_string_loop(n: Int, acc: String) -> String {
  case n {
    0 -> acc
    _ -> {
      let digit = n % 10
      let rest = n / 10
      int_to_string_loop(rest, digit_char(digit) <> acc)
    }
  }
}

fn digit_char(d: Int) -> String {
  case d {
    0 -> "0"
    1 -> "1"
    2 -> "2"
    3 -> "3"
    4 -> "4"
    5 -> "5"
    6 -> "6"
    7 -> "7"
    8 -> "8"
    9 -> "9"
    _ -> "?"
  }
}

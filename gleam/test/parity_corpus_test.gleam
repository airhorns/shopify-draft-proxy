//// Mirrors `tests/unit/conformance-parity-scenarios.test.ts`'s
//// scaffolding pass — the bit that *discovers* every spec under
//// `config/parity-specs/**` and asserts the classification partition
//// is what we expect. Per-scenario execution is still
//// `parity_test.gleam`'s job (hand-listed for now).
////
//// Erlang-only: simplifile's `get_files` walks the FS, which the JS
//// test harness can't do.

@target(erlang)
import gleam/list
@target(erlang)
import parity/classify
@target(erlang)
import parity/discover
@target(erlang)
import simplifile

@target(erlang)
const parity_root: String = "../config/parity-specs"

@target(erlang)
pub fn corpus_discovers_all_specs_test() {
  let assert Ok(files) = discover.discover(parity_root)
  // The TS side ships 379 specs today; assert a generous lower bound
  // so adding more specs doesn't tip the test red. Drop the guard
  // entirely if specs ever go below 200.
  assert list.length(files) >= 379
}

@target(erlang)
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

@target(erlang)
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

@target(erlang)
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

@target(erlang)
fn int_to_string(n: Int) -> String {
  case n {
    0 -> "0"
    _ -> int_to_string_loop(n, "")
  }
}

@target(erlang)
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

@target(erlang)
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

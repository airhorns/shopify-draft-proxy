//// Pure-Gleam parity scenario suite.
////
//// This is the Gleam port's replacement for
//// `tests/unit/conformance-parity-scenarios.test.ts`. The suite discovers every
//// parity spec under `config/parity-specs/**`, drives each GraphQL document
//// through `draft_proxy.process_request`, and compares proxy output to the
//// corresponding capture slice using the spec's `expectedDifferences` matchers.
////
//// Specs that are not ported yet are tracked in
//// `config/gleam-port-ci-gates.json` as expected failures. Porting work removes
//// entries from that list; this suite still attempts every discovered spec.

import gleam/dynamic/decode
import gleam/int
import gleam/json
import gleam/list
import gleam/result
import gleam/string
import parity/diff
import parity/discover
import parity/runner
import simplifile

const parity_root: String = "../config/parity-specs"

const gate_config_path: String = "../config/gleam-port-ci-gates.json"

@target(erlang)
const current_target: String = "erlang"

@target(javascript)
const current_target: String = "javascript"

pub type ExpectedFailure {
  ExpectedFailure(spec_path: String, reason: String, targets: List(String))
}

pub type Outcome {
  Passed(spec_path: String)
  Failed(spec_path: String, message: String)
  /// Spec hasn't been migrated to cassette playback yet. Counted in
  /// the gate summary but does not flag as an unexpected failure or
  /// pass — these specs are awaiting `pnpm parity:record`.
  Skipped(spec_path: String, reason: String)
}

pub fn all_discovered_parity_specs_follow_expected_failures_test() {
  let assert Ok(discovered_paths) = discover.discover(parity_root)
  let spec_paths =
    discovered_paths
    |> list.map(repo_relative_path)
    |> list.sort(by: string.compare)
  let assert Ok(expected_failures) = load_expected_failures()
  let applicable_expected_failures =
    list.filter(expected_failures, expected_failure_applies)
  let outcomes = list.map(spec_paths, run_one)

  let unexpected_failures =
    outcomes
    |> list.filter_map(fn(outcome) {
      case outcome {
        Failed(spec_path, message) ->
          case is_expected_failure(spec_path, applicable_expected_failures) {
            True -> Error(Nil)
            False -> Ok(spec_path <> ": " <> first_line(message))
          }
        Passed(_) -> Error(Nil)
        Skipped(_, _) -> Error(Nil)
      }
    })

  let unexpected_passes =
    applicable_expected_failures
    |> list.filter_map(fn(failure) {
      let spec_path = expected_failure_path(failure)
      case outcome_passed(spec_path, outcomes) {
        True -> Ok(spec_path)
        False -> Error(Nil)
      }
    })

  let missing_expected_specs =
    expected_failures
    |> list.filter_map(fn(failure) {
      let spec_path = expected_failure_path(failure)
      case list.contains(spec_paths, spec_path) {
        True -> Error(Nil)
        False -> Ok(spec_path)
      }
    })

  case unexpected_failures, unexpected_passes, missing_expected_specs {
    [], [], [] -> Nil
    _, _, _ ->
      panic as render_summary(
          unexpected_failures,
          unexpected_passes,
          missing_expected_specs,
          count_skipped(outcomes),
        )
  }
}

fn count_skipped(outcomes: List(Outcome)) -> Int {
  list.fold(outcomes, 0, fn(acc, outcome) {
    case outcome {
      Skipped(_, _) -> acc + 1
      _ -> acc
    }
  })
}

fn run_one(spec_path: String) -> Outcome {
  case runner.run(spec_path) {
    Ok(report) -> {
      case report.targets {
        [] -> Failed(spec_path, "spec defines no comparison targets")
        _ ->
          case runner.into_assert(report) {
            Ok(Nil) -> Passed(spec_path)
            Error(message) -> Failed(spec_path, message)
          }
      }
    }
    Error(err) ->
      case runner.is_spec_not_migrated(err) {
        True -> Skipped(spec_path, runner.render_error(err))
        False -> Failed(spec_path, runner.render_error(err))
      }
  }
}

fn load_expected_failures() -> Result(List(ExpectedFailure), String) {
  use source <- result.try(read_text(gate_config_path))
  json.parse(source, expected_failures_decoder())
  |> result.map_error(fn(_) {
    "could not decode expectedGleamParityFailures from " <> gate_config_path
  })
}

fn read_text(path: String) -> Result(String, String) {
  case simplifile.read(path) {
    Ok(source) -> Ok(source)
    Error(err) ->
      Error("could not read " <> path <> ": " <> simplifile.describe_error(err))
  }
}

fn expected_failures_decoder() -> decode.Decoder(List(ExpectedFailure)) {
  use failures <- decode.field(
    "expectedGleamParityFailures",
    decode.list(expected_failure_decoder()),
  )
  decode.success(failures)
}

fn expected_failure_decoder() -> decode.Decoder(ExpectedFailure) {
  use spec_path <- decode.field("specPath", decode.string)
  use reason <- decode.field("reason", decode.string)
  use targets <- decode.optional_field(
    "targets",
    [],
    decode.list(decode.string),
  )
  decode.success(ExpectedFailure(
    spec_path: spec_path,
    reason: reason,
    targets: targets,
  ))
}

fn is_expected_failure(
  spec_path: String,
  expected_failures: List(ExpectedFailure),
) -> Bool {
  list.any(expected_failures, fn(failure) {
    expected_failure_path(failure) == spec_path
  })
}

fn expected_failure_path(failure: ExpectedFailure) -> String {
  case failure {
    ExpectedFailure(spec_path: spec_path, reason: _, targets: _) -> spec_path
  }
}

fn expected_failure_applies(failure: ExpectedFailure) -> Bool {
  case failure {
    ExpectedFailure(spec_path: _, reason: _, targets: []) -> True
    ExpectedFailure(spec_path: _, reason: _, targets: targets) ->
      list.contains(targets, current_target)
  }
}

fn outcome_passed(spec_path: String, outcomes: List(Outcome)) -> Bool {
  list.any(outcomes, fn(outcome) {
    case outcome {
      Passed(path) -> path == spec_path
      Failed(_, _) -> False
      Skipped(_, _) -> False
    }
  })
}

fn repo_relative_path(path: String) -> String {
  case string.starts_with(path, "../") {
    True -> string.drop_start(from: path, up_to: 3)
    False ->
      case string.starts_with(path, "./") {
        True -> string.drop_start(from: path, up_to: 2)
        False -> path
      }
  }
}

fn first_line(message: String) -> String {
  case string.split(message, on: "\n") |> list.first {
    Ok(line) -> line
    Error(_) -> message
  }
}

fn render_summary(
  unexpected_failures: List(String),
  unexpected_passes: List(String),
  missing_expected_specs: List(String),
  skipped_count: Int,
) -> String {
  string.join(
    [
      "Gleam parity expected-failure gate failed.",
      "skipped (awaiting cassette migration): " <> int.to_string(skipped_count),
      render_section("unexpected failures", unexpected_failures),
      render_section("expected failures that now pass", unexpected_passes),
      render_section(
        "expected failure specs not discovered",
        missing_expected_specs,
      ),
    ],
    "\n",
  )
}

fn render_section(label: String, values: List(String)) -> String {
  case values {
    [] -> label <> ": 0"
    _ ->
      label
      <> ": "
      <> int.to_string(list.length(values))
      <> "\n"
      <> string.join(list.take(values, 20), "\n")
  }
}

/// Confirms `into_assert` actually surfaces non-empty mismatches as a
/// failure, so the parity test above is not trivially passing on empty reports.
pub fn runner_into_assert_flags_mismatches_test() {
  let report =
    runner.Report(
      scenario_id: "synthetic",
      targets: [
        runner.TargetReport(
          name: "always-fails",
          capture_path: "$",
          proxy_path: "$",
          mismatches: [
            diff.Mismatch(path: "$.x", expected: "1", actual: "2"),
          ],
        ),
      ],
      operation_name_errors: [],
    )
  assert runner.has_mismatches(report)
  let assert Error(_) = runner.into_assert(report)
}

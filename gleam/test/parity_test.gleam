//// Pure-Gleam parity scenario suite.
////
//// This is the Gleam port's replacement for
//// `tests/unit/conformance-parity-scenarios.test.ts`. The suite discovers every
//// parity spec under `config/parity-specs/**`, drives each GraphQL document
//// through `draft_proxy.process_request`, and compares proxy output to the
//// corresponding capture slice using the spec's `expectedDifferences` matchers.

import gleam/int
import gleam/list
import gleam/string
import parity/diff
import parity/discover
import parity/runner

const parity_root: String = "../config/parity-specs"

pub type Outcome {
  Passed(spec_path: String)
  Failed(spec_path: String, message: String)
}

pub fn all_discovered_parity_specs_pass_test() {
  let assert Ok(discovered_paths) = discover.discover(parity_root)
  let spec_paths =
    discovered_paths
    |> list.map(repo_relative_path)
    |> list.sort(by: string.compare)
  let outcomes = list.map(spec_paths, run_one)

  let failures =
    outcomes
    |> list.filter_map(fn(outcome) {
      case outcome {
        Failed(spec_path, message) ->
          Ok(spec_path <> ": " <> first_line(message))
        Passed(_) -> Error(Nil)
      }
    })

  case failures {
    [] -> Nil
    _ -> panic as render_summary(failures)
  }
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
    Error(err) -> Failed(spec_path, runner.render_error(err))
  }
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

fn render_summary(failures: List(String)) -> String {
  string.join(
    [
      "Gleam parity corpus failed.",
      render_section("failures", failures),
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

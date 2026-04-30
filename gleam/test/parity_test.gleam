//// Pure-Gleam parity scenario suite.
////
//// This is the Gleam port's replacement for
//// `tests/unit/conformance-parity-scenarios.test.ts`. The suite now
//// discovers every checked-in TS-ready parity spec and executes it
//// through the Gleam runner. Specs that are blocked by unported Gleam
//// domains are still executed; a blocker is considered stale if the
//// spec starts passing.

import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import parity/blockers
import parity/diff
import parity/discover
import parity/runner

const parity_root: String = "../config/parity-specs"

pub type CorpusOutcome {
  Passed(path: String)
  Skipped(path: String, reason: String)
  Failed(path: String, message: String)
  StaleBlocker(path: String, reason: String)
}

pub fn all_ts_ready_specs_execute_with_reviewed_gleam_partition_test() {
  let assert Ok(paths) = discover.discover(parity_root)
  let outcomes =
    paths
    |> list.map(repo_relative)
    |> list.sort(by: string.compare)
    |> list.map(run_spec)

  let failed = filter_failed(outcomes)
  let stale = filter_stale(outcomes)
  case failed, stale {
    [], [] -> Nil
    _, _ -> panic as render_partition_failures(failed, stale)
  }

  assert count_passed(outcomes) == list.length(blockers.known_passing_paths())
  assert count_skipped(outcomes) == 354
  assert list.length(outcomes) == 379
}

fn repo_relative(path: String) -> String {
  case string.starts_with(path, "../") {
    True -> string.drop_start(path, 3)
    False -> path
  }
}

fn run_spec(path: String) -> CorpusOutcome {
  let blocker = blockers.blocker_for(path)
  let result = runner.run(path)
  case result {
    Ok(report) ->
      case runner.into_assert(report) {
        Ok(Nil) ->
          case blocker {
            Some(reason) -> StaleBlocker(path: path, reason: reason)
            None -> Passed(path: path)
          }
        Error(message) ->
          case blocker {
            Some(reason) -> Skipped(path: path, reason: reason)
            None -> Failed(path: path, message: message)
          }
      }
    Error(error) ->
      case blocker {
        Some(reason) -> Skipped(path: path, reason: reason)
        None -> Failed(path: path, message: runner.render_error(error))
      }
  }
}

fn filter_failed(outcomes: List(CorpusOutcome)) -> List(CorpusOutcome) {
  list.filter(outcomes, fn(outcome) {
    case outcome {
      Failed(_, _) -> True
      _ -> False
    }
  })
}

fn filter_stale(outcomes: List(CorpusOutcome)) -> List(CorpusOutcome) {
  list.filter(outcomes, fn(outcome) {
    case outcome {
      StaleBlocker(_, _) -> True
      _ -> False
    }
  })
}

fn count_passed(outcomes: List(CorpusOutcome)) -> Int {
  outcomes
  |> list.filter(fn(outcome) {
    case outcome {
      Passed(_) -> True
      _ -> False
    }
  })
  |> list.length
}

fn count_skipped(outcomes: List(CorpusOutcome)) -> Int {
  outcomes
  |> list.filter(fn(outcome) {
    case outcome {
      Skipped(_, _) -> True
      _ -> False
    }
  })
  |> list.length
}

fn render_partition_failures(
  failed: List(CorpusOutcome),
  stale: List(CorpusOutcome),
) -> String {
  "unreviewed Gleam parity partition changes\n"
  <> "failed without blocker: "
  <> int.to_string(list.length(failed))
  <> "\n"
  <> string.join(list.map(failed, render_outcome), "\n")
  <> "\n"
  <> "stale blockers: "
  <> int.to_string(list.length(stale))
  <> "\n"
  <> string.join(list.map(stale, render_outcome), "\n")
}

fn render_outcome(outcome: CorpusOutcome) -> String {
  case outcome {
    Passed(path) -> path <> " passed"
    Skipped(path, reason) -> path <> " skipped: " <> reason
    Failed(path, message) -> path <> " failed: " <> message
    StaleBlocker(path, reason) ->
      path <> " passed with stale blocker: " <> reason
  }
}

/// Confirms `into_assert` actually surfaces non-empty mismatches as a
/// failure, so the corpus test above cannot trivially pass on empty
/// reports.
pub fn runner_into_assert_flags_mismatches_test() {
  let report =
    runner.Report(scenario_id: "synthetic", targets: [
      runner.TargetReport(
        name: "always-fails",
        capture_path: "$",
        proxy_path: "$",
        mismatches: [
          diff.Mismatch(path: "$.x", expected: "1", actual: "2"),
        ],
      ),
    ])
  assert runner.has_mismatches(report)
  let assert Error(_) = runner.into_assert(report)
}

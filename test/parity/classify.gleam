//// Mirrors `classifyParityScenarioState` in
//// `scripts/conformance-parity-lib.ts`. Captures only the fields the
//// classifier reads — full-spec decoding lives in `parity/spec.gleam`.

import gleam/dynamic/decode
import gleam/json
import gleam/list

pub type ScenarioState {
  ReadyForComparison
  EnforcedByFixture
  InvalidMissingComparisonContract
  NotYetImplemented
}

pub type ClassifySpec {
  ClassifySpec(
    scenario_id: String,
    scenario_status: String,
    comparison_mode: String,
    capture_files: List(String),
    has_proxy_request: Bool,
    has_comparison_targets: Bool,
  )
}

pub type ClassifyError {
  ClassifyError(path: String, message: String)
}

pub fn parse(
  path: String,
  source: String,
) -> Result(ClassifySpec, ClassifyError) {
  case json.parse(source, decoder()) {
    Ok(spec) -> Ok(spec)
    Error(_) ->
      Error(ClassifyError(path: path, message: "could not decode spec"))
  }
}

fn decoder() -> decode.Decoder(ClassifySpec) {
  use scenario_id <- decode.field("scenarioId", decode.string)
  use scenario_status <- decode.optional_field(
    "scenarioStatus",
    "",
    decode.string,
  )
  use comparison_mode <- decode.optional_field(
    "comparisonMode",
    "",
    decode.string,
  )
  use capture_files <- decode.optional_field(
    "liveCaptureFiles",
    [],
    decode.list(decode.string),
  )
  use has_proxy_request <- decode.optional_field(
    "proxyRequest",
    False,
    decode.success(True),
  )
  use has_comparison_targets <- decode.optional_field(
    "comparison",
    False,
    comparison_has_targets_decoder(),
  )
  decode.success(ClassifySpec(
    scenario_id: scenario_id,
    scenario_status: scenario_status,
    comparison_mode: comparison_mode,
    capture_files: capture_files,
    has_proxy_request: has_proxy_request,
    has_comparison_targets: has_comparison_targets,
  ))
}

fn comparison_has_targets_decoder() -> decode.Decoder(Bool) {
  use targets <- decode.optional_field(
    "targets",
    [],
    decode.list(decode.dynamic),
  )
  decode.success(!list.is_empty(targets))
}

pub fn classify(spec: ClassifySpec) -> ScenarioState {
  case spec.scenario_status {
    "captured" ->
      case
        spec.comparison_mode == "captured-fixture"
        && !list.is_empty(spec.capture_files)
      {
        True -> EnforcedByFixture
        False ->
          case spec.has_proxy_request && spec.has_comparison_targets {
            True -> ReadyForComparison
            False -> InvalidMissingComparisonContract
          }
      }
    _ -> NotYetImplemented
  }
}

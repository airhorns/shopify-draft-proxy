//// Parity-spec record + decoder.
////
//// The on-disk JSON shape we care about (extra fields are
//// scaffolding for the TS engine and are ignored):
////
////   {
////     "scenarioId": "...",
////     "liveCaptureFiles": ["fixtures/.../capture.json"],
////     "setup": [{ ... optional setup proxy requests ... }],
////     "proxyRequest": {                                <-- primary
////       "documentPath": "config/parity-requests/.../op.graphql",
////       "apiVersion": "2026-04",
////       "variablesCapturePath": "$.cases[1].variables"  // OR
////       "variablesPath": "config/.../variables.json"    // OR
////       "variables": { … inline, may contain
////                       {"fromPrimaryProxyPath": "$..."} markers }
////     },
////     "comparison": {
////       "expectedDifferences": [...]
////       "targets": [
////         {
////           "name": "...",
////           "capturePath": "$....",
////           "proxyPath": "$....",
////           "selectedPaths": ["$.field"],
////           "upstreamCapturePath": "$.response.payload",
////           "expectedDifferences": [...],
////           "proxyRequest": { … per-target override, same shape as
////                             primary, may use fromPrimaryProxyPath
////                             markers in inline `variables` }
////         }
////       ]
////     }
////   }

import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import parity/diff.{type ExpectedDifference}
import parity/json_value.{type JsonValue}

pub type ParityVariables {
  /// Resolve variables by following a JSONPath into the primary capture.
  VariablesFromCapture(path: String)
  /// Resolve variables by reading a sibling JSON file.
  VariablesFromFile(path: String)
  /// Inline literal/templated variables. May contain nested
  /// `{"fromPrimaryProxyPath": "$..."}` markers that the runner
  /// substitutes against the primary proxy response.
  VariablesInline(template: JsonValue)
  /// No variables.
  NoVariables
}

pub type ProxyRequest {
  ProxyRequest(
    document_path: String,
    variables: ParityVariables,
    api_version: Option(String),
  )
}

pub type TargetRequest {
  /// Target reuses the primary `proxyRequest` and its response. The
  /// runner does NOT re-execute the primary; targets that share the
  /// primary diff against the same response.
  ReusePrimary
  /// Target executes its own request after the primary. State (store,
  /// synthetic identity) is threaded forward from the primary.
  OverrideRequest(request: ProxyRequest)
}

pub type ProxySource {
  ProxyResponse
  ProxyState
  ProxyLog
}

pub type Target {
  Target(
    name: String,
    capture_path: String,
    proxy_path: String,
    proxy_source: ProxySource,
    upstream_capture_path: Option(String),
    selected_paths: List(String),
    expected_differences: List(ExpectedDifference),
    excluded_paths: List(String),
    request: TargetRequest,
  )
}

pub type Spec {
  Spec(
    scenario_id: String,
    capture_file: String,
    setup_requests: List(ProxyRequest),
    proxy_request: ProxyRequest,
    targets: List(Target),
    expected_differences: List(ExpectedDifference),
    operation_names: List(String),
  )
}

pub type DecodeError {
  DecodeError(message: String)
}

pub fn decode(source: String) -> Result(Spec, DecodeError) {
  json.parse(source, spec_decoder())
  |> result.map_error(fn(_) {
    DecodeError(message: "could not decode parity spec JSON")
  })
}

fn spec_decoder() -> Decoder(Spec) {
  use scenario_id <- decode.field("scenarioId", decode.string)
  use captures <- decode.field("liveCaptureFiles", decode.list(decode.string))
  use setup_requests <- decode.optional_field(
    "setup",
    [],
    decode.list(proxy_request_decoder()),
  )
  use proxy_request <- decode.field("proxyRequest", proxy_request_decoder())
  use comparison <- decode.field("comparison", comparison_decoder())
  use operation_names <- decode.optional_field(
    "operationNames",
    [],
    decode.list(decode.string),
  )
  case captures {
    [first, ..] ->
      decode.success(Spec(
        scenario_id: scenario_id,
        capture_file: first,
        setup_requests: setup_requests,
        proxy_request: proxy_request,
        targets: comparison.0,
        expected_differences: comparison.1,
        operation_names: operation_names,
      ))
    [] -> decode.failure(empty_spec(), "liveCaptureFiles cannot be empty")
  }
}

fn empty_spec() -> Spec {
  Spec(
    scenario_id: "",
    capture_file: "",
    setup_requests: [],
    proxy_request: ProxyRequest(
      document_path: "",
      variables: NoVariables,
      api_version: None,
    ),
    targets: [],
    expected_differences: [],
    operation_names: [],
  )
}

fn proxy_request_decoder() -> Decoder(ProxyRequest) {
  use document_path <- decode.field("documentPath", decode.string)
  use api_version <- decode.optional_field(
    "apiVersion",
    None,
    decode.optional(decode.string),
  )
  use variables_capture_path <- decode.optional_field(
    "variablesCapturePath",
    None,
    decode.optional(decode.string),
  )
  use variables_path <- decode.optional_field(
    "variablesPath",
    None,
    decode.optional(decode.string),
  )
  use variables_inline <- decode.optional_field(
    "variables",
    None,
    decode.optional(decode.dynamic),
  )
  let variables =
    variables_from_fields(
      variables_capture_path,
      variables_path,
      variables_inline,
    )
  decode.success(ProxyRequest(
    document_path: document_path,
    variables: variables,
    api_version: api_version,
  ))
}

fn variables_from_fields(
  capture_path: Option(String),
  file_path: Option(String),
  inline_dyn: Option(Dynamic),
) -> ParityVariables {
  case capture_path, file_path, inline_dyn {
    Some(path), _, _ -> VariablesFromCapture(path: path)
    _, Some(path), _ -> VariablesFromFile(path: path)
    _, _, Some(dyn) ->
      case json_value.from_dynamic(dyn) {
        Ok(value) -> VariablesInline(template: value)
        Error(_) -> NoVariables
      }
    None, None, None -> NoVariables
  }
}

fn comparison_decoder() -> Decoder(#(List(Target), List(ExpectedDifference))) {
  use targets <- decode.field("targets", decode.list(target_decoder()))
  use expected_differences <- decode.optional_field(
    "expectedDifferences",
    [],
    decode.list(expected_difference_decoder()),
  )
  decode.success(#(targets, expected_differences))
}

fn target_decoder() -> Decoder(Target) {
  use name <- decode.field("name", decode.string)
  use capture_path <- decode.field("capturePath", decode.string)
  use proxy_path <- decode.optional_field(
    "proxyPath",
    None,
    decode.optional(decode.string),
  )
  use proxy_state_path <- decode.optional_field(
    "proxyStatePath",
    None,
    decode.optional(decode.string),
  )
  use proxy_log_path <- decode.optional_field(
    "proxyLogPath",
    None,
    decode.optional(decode.string),
  )
  use upstream_capture_path <- decode.optional_field(
    "upstreamCapturePath",
    None,
    decode.optional(decode.string),
  )
  use expected_differences <- decode.optional_field(
    "expectedDifferences",
    [],
    decode.list(expected_difference_decoder()),
  )
  use selected_paths <- decode.optional_field(
    "selectedPaths",
    [],
    decode.list(decode.string),
  )
  use excluded_paths <- decode.optional_field(
    "excludedPaths",
    [],
    decode.list(decode.string),
  )
  use request <- decode.optional_field(
    "proxyRequest",
    ReusePrimary,
    decode.map(proxy_request_decoder(), OverrideRequest),
  )
  let expected_differences =
    list.append(
      expected_differences,
      list.map(excluded_paths, diff.expected_ignore),
    )
  case proxy_path, proxy_state_path, proxy_log_path {
    Some(path), _, _ ->
      decode.success(Target(
        name: name,
        capture_path: capture_path,
        proxy_path: path,
        proxy_source: ProxyResponse,
        upstream_capture_path: upstream_capture_path,
        selected_paths: selected_paths,
        expected_differences: expected_differences,
        excluded_paths: excluded_paths,
        request: request,
      ))
    None, Some(path), _ ->
      decode.success(Target(
        name: name,
        capture_path: capture_path,
        proxy_path: path,
        proxy_source: ProxyState,
        upstream_capture_path: upstream_capture_path,
        selected_paths: selected_paths,
        expected_differences: expected_differences,
        excluded_paths: excluded_paths,
        request: request,
      ))
    None, None, Some(path) ->
      decode.success(Target(
        name: name,
        capture_path: capture_path,
        proxy_path: path,
        proxy_source: ProxyLog,
        upstream_capture_path: upstream_capture_path,
        selected_paths: selected_paths,
        expected_differences: expected_differences,
        excluded_paths: excluded_paths,
        request: request,
      ))
    None, None, None ->
      decode.failure(
        Target(
          name: name,
          capture_path: capture_path,
          proxy_path: "$",
          proxy_source: ProxyResponse,
          upstream_capture_path: upstream_capture_path,
          selected_paths: selected_paths,
          expected_differences: expected_differences,
          excluded_paths: excluded_paths,
          request: request,
        ),
        "target must define proxyPath, proxyStatePath, or proxyLogPath",
      )
  }
}

fn expected_difference_decoder() -> Decoder(ExpectedDifference) {
  use path <- decode.field("path", decode.string)
  use ignore <- decode.optional_field("ignore", False, decode.bool)
  use matcher <- decode.optional_field("matcher", "", decode.string)
  case ignore {
    True -> decode.success(diff.expected_ignore(path))
    False -> decode.success(diff.expected_match(path, matcher))
  }
}

/// Combine spec-level and target-level expected differences.
pub fn rules_for(spec: Spec, target: Target) -> List(ExpectedDifference) {
  let excluded = list.map(target.excluded_paths, diff.expected_ignore)
  list.append(spec.expected_differences, target.expected_differences)
  |> list.append(excluded)
}

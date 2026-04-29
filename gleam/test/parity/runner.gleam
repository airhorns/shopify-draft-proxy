//// Pure-Gleam parity runner.
////
//// Replaces the legacy vitest harness in
//// `tests/unit/conformance-parity-scenarios.test.ts`. Reads a parity
//// spec, loads the capture and GraphQL document referenced by the
//// spec, drives them through `draft_proxy.process_request`, and
//// compares each target's `capturePath` slice of the capture against
//// the same `proxyPath` slice of the proxy response — applying the
//// spec's `expectedDifferences` matchers.
////
//// Per-target `proxyRequest` overrides are supported. State (store,
//// synthetic identity) is threaded forward across requests, so a
//// target can read back records the primary mutation created.
////
//// File-system paths in the spec are repo-root relative. Tests run
//// from the `gleam/` subdirectory; the runner resolves paths via `..`
//// (configurable via `RunnerConfig.repo_root`).

import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/diff.{type Mismatch}
import parity/json_value.{type JsonValue, JArray, JObject}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, NoVariables, OverrideRequest, ReusePrimary,
  VariablesFromCapture, VariablesFromFile, VariablesInline,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromCapturePath` substitution path didn't resolve.
  CaptureRefUnresolved(path: String)
  /// Capture JSONPath did not resolve for a target.
  CaptureUnresolved(target: String, path: String)
  /// Proxy response JSONPath did not resolve for a target.
  ProxyUnresolved(target: String, path: String)
  /// Proxy returned a non-200 status.
  ProxyStatus(target: String, status: Int, body: String)
}

pub type TargetReport {
  TargetReport(
    name: String,
    capture_path: String,
    proxy_path: String,
    mismatches: List(Mismatch),
  )
}

pub type Report {
  Report(scenario_id: String, targets: List(TargetReport))
}

pub type RunnerConfig {
  RunnerConfig(repo_root: String)
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..")
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  use capture <- result.try(load_capture(config, parsed))
  use primary_doc <- result.try(read_file(
    resolve(config, parsed.proxy_request.document_path),
  ))
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    "<primary>",
  ))
  let proxy = draft_proxy.new()
  use #(primary_response, proxy) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  Ok(Report(scenario_id: parsed.scenario_id, targets: target_reports))
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, List(TargetReport)), RunError) {
  list.try_fold(parsed.targets, #(proxy, []), fn(state, target) {
    let #(current_proxy, acc_reports) = state
    use #(next_proxy, report) <- result.try(run_target(
      config,
      parsed,
      target,
      capture,
      primary_response,
      current_proxy,
    ))
    Ok(#(next_proxy, [report, ..acc_reports]))
  })
  |> result.map(fn(state) {
    let #(final_proxy, reports) = state
    #(final_proxy, list.reverse(reports))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, TargetReport), RunError) {
  use #(actual_response, next_proxy) <- result.try(actual_response_for(
    config,
    target,
    capture,
    primary_response,
    proxy,
  ))
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, _ ->
      Error(CaptureUnresolved(
        target: target.name,
        path: target.capture_path,
      ))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = diff.diff_with_expected(expected, actual, rules)
      Ok(#(
        next_proxy,
        TargetReport(
          name: target.name,
          capture_path: target.capture_path,
          proxy_path: target.proxy_path,
          mismatches: mismatches,
        ),
      ))
    }
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.request {
    ReusePrimary -> Ok(#(primary_response, proxy))
    OverrideRequest(request: request) -> {
      use document <- result.try(read_file(
        resolve(config, request.document_path),
      ))
      use variables <- result.try(resolve_variables(
        config,
        request.variables,
        capture,
        Some(primary_response),
        target.name,
      ))
      use #(response, next_proxy) <- result.try(execute(
        proxy,
        document,
        variables,
        target.name,
      ))
      use value <- result.try(parse_response_body(response))
      Ok(#(value, next_proxy))
    }
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn load_capture(
  config: RunnerConfig,
  parsed: Spec,
) -> Result(JsonValue, RunError) {
  let path = resolve(config, parsed.capture_file)
  use source <- result.try(read_file(path))
  parse_json(path, source)
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      parse_json(resolved, source)
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(template, primary_response, capture)
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_primary_ref(template) {
    Some(path) ->
      case primary {
        None -> Error(PrimaryRefUnresolved(path: path))
        Some(root) ->
          case jsonpath.lookup(root, path) {
            Some(value) -> Ok(value)
            None -> Error(PrimaryRefUnresolved(path: path))
          }
      }
    None ->
      case as_capture_ref(template) {
        Some(path) ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(value)
            None -> Error(CaptureRefUnresolved(path: path))
          }
        None ->
          case template {
            JObject(entries) ->
              entries
              |> list.try_map(fn(pair) {
                let #(k, v) = pair
                case substitute(v, primary, capture) {
                  Ok(v2) -> Ok(#(k, v2))
                  Error(e) -> Error(e)
                }
              })
              |> result.map(JObject)
            JArray(items) ->
              items
              |> list.try_map(fn(item) { substitute(item, primary, capture) })
              |> result.map(JArray)
            leaf -> Ok(leaf)
          }
      }
  }
}

/// If `value` is exactly `{"fromPrimaryProxyPath": "..."}` (one entry
/// with a string value), return the path. Otherwise None.
fn as_primary_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPrimaryProxyPath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

/// If `value` is exactly `{"fromCapturePath": "..."}` (one entry with
/// a string value), return the path. Otherwise None.
fn as_capture_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromCapturePath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn execute(
  proxy: DraftProxy,
  document: String,
  variables: JsonValue,
  context: String,
) -> Result(#(Response, DraftProxy), RunError) {
  let body = build_graphql_body(document, variables)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case response.status {
    200 -> Ok(#(response, next_proxy))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn build_graphql_body(document: String, variables: JsonValue) -> String {
  let query = json.to_string(json.string(document))
  let vars = json_value.to_string(variables)
  "{\"query\":" <> query <> ",\"variables\":" <> vars <> "}"
}

fn parse_response_body(response: Response) -> Result(JsonValue, RunError) {
  let serialized = json.to_string(response.body)
  parse_json("<proxy-response>", serialized)
}

fn read_file(path: String) -> Result(String, RunError) {
  case simplifile.read(path) {
    Ok(s) -> Ok(s)
    Error(reason) ->
      Error(FileError(path: path, reason: simplifile.describe_error(reason)))
  }
}

fn parse_json(path: String, source: String) -> Result(JsonValue, RunError) {
  case json_value.parse(source) {
    Ok(v) -> Ok(v)
    Error(e) -> Error(JsonError(path: path, reason: e.message))
  }
}

fn resolve(config: RunnerConfig, path: String) -> String {
  case string.starts_with(path, "/") {
    True -> path
    False -> config.repo_root <> "/" <> path
  }
}

pub fn has_mismatches(report: Report) -> Bool {
  list.any(report.targets, fn(t) { t.mismatches != [] })
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True ->
      report.scenario_id
      <> "\n"
      <> string.join(list.map(report.targets, render_target), "\n")
  }
}

fn render_target(target: TargetReport) -> String {
  case target.mismatches {
    [] -> "  [" <> target.name <> "] OK"
    mismatches ->
      "  ["
      <> target.name
      <> "] "
      <> int.to_string(list.length(mismatches))
      <> " mismatch(es):\n"
      <> diff.render_mismatches(mismatches)
  }
}

pub fn into_assert(report: Report) -> Result(Nil, String) {
  case has_mismatches(report) {
    False -> Ok(Nil)
    True -> Error(render(report))
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    VariablesUnresolved(path) ->
      "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyUnresolved(target, path) ->
      "proxy response jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyStatus(target, status, body) ->
      "proxy returned status "
      <> int.to_string(status)
      <> " for target '"
      <> target
      <> "': "
      <> body
  }
}

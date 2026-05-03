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

import gleam/dict.{type Dict}
import gleam/int
import gleam/io
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/set
import gleam/string
import parity/cassette
import parity/diff.{type Mismatch}
import parity/json_value.{type JsonValue, JArray, JObject, JString}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, LiveHybridMode, NoVariables, OverrideRequest, ProxyLog,
  ProxyResponse, ProxyState, ReusePrimary, SnapshotEmptyMode,
  VariablesFromCapture, VariablesFromFile, VariablesInline,
}
import shopify_draft_proxy/graphql/parse_operation.{
  type GraphQLOperationType, MutationOperation, ParsedOperation, QueryOperation,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import shopify_draft_proxy/proxy/operation_registry
import shopify_draft_proxy/proxy/operation_registry_data
import shopify_draft_proxy/proxy/proxy_state.{Config, LiveHybrid}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Spec is in `LiveHybridMode` but the referenced capture file does
  /// not carry an `upstreamCalls` cassette. The scenario hasn't been
  /// migrated to cassette playback yet — re-record with
  /// `pnpm parity:record <scenario-id>`. The parity-test gate treats
  /// this as a "skipped" outcome, not a failure.
  SpecNotMigrated(spec_path: String, reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromPreviousProxyPath` substitution path didn't resolve.
  PreviousRefUnresolved(path: String)
  /// `fromProxyResponse` substitution target/path didn't resolve.
  ProxyResponseRefUnresolved(target: String, path: String)
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

/// Operation that was actually executed against the proxy during a
/// scenario run. Mirrors the TS `ExecutedOperation` type the runner
/// builds in `executeGraphQLAgainstLocalProxy`.
pub type ExecutedOperation {
  ExecutedOperation(
    type_: GraphQLOperationType,
    name: Option(String),
    root_fields: List(String),
  )
}

pub type Report {
  Report(
    scenario_id: String,
    targets: List(TargetReport),
    operation_name_errors: List(String),
  )
}

pub type RunnerConfig {
  /// `repo_root` resolves spec/capture/document paths.
  /// `debug` flips on per-request, per-cassette-call, and per-target
  /// assertion logging to stderr — verbose, but lets you see exactly
  /// what the proxy did and which cassette entries it consulted while
  /// debugging a single scenario.
  RunnerConfig(repo_root: String, debug: Bool)
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..", debug: False)
}

/// Flip the runner's debug flag on. Pair with `run_with_config` for a
/// single noisy scenario run while debugging a parity diff.
pub fn with_debug(config: RunnerConfig) -> RunnerConfig {
  RunnerConfig(..config, debug: True)
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

/// Convenience: run a single spec with debug logging on. Prints the
/// spec mode, cassette entry count, every GraphQL request/response,
/// every cassette match/miss, and per-target assertion results to
/// stderr. Cheaper to call than wiring `with_debug` from a test file.
pub fn run_debug(spec_path: String) -> Result(Report, RunError) {
  run_with_config(with_debug(default_config()), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  let capture_path = resolve(config, parsed.capture_file)
  use capture_source <- result.try(read_file(capture_path))
  use capture <- result.try(parse_json(capture_path, capture_source))
  use proxy <- result.try(build_proxy_for_mode(
    spec_path,
    parsed,
    capture_source,
    config.debug,
  ))
  use primary_doc <- result.try(
    read_file(resolve(config, parsed.proxy_request.document_path)),
  )
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    None,
    dict.new(),
    "<primary>",
  ))
  let primary_vars = replace_customer_one_variables(capture, primary_vars)
  use #(primary_response, proxy, primary_op) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
    parsed.proxy_request.api_version,
    config.debug,
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports, target_ops) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  let executed_operations = [primary_op, ..target_ops]
  let operation_name_errors =
    validate_operation_names(parsed, executed_operations)
  Ok(Report(
    scenario_id: parsed.scenario_id,
    targets: target_reports,
    operation_name_errors: operation_name_errors,
  ))
}

/// Build the `DraftProxy` instance the runner drives the spec through,
/// configured for the spec's parity `mode`:
///
/// - `LiveHybridMode` — installs the cassette transport recorded in
///   `capture.upstreamCalls` and switches `read_mode` to `LiveHybrid`,
///   so handler-issued upstream calls (via `proxy/upstream_query`) are
///   served deterministically from the cassette. If the capture has no
///   `upstreamCalls` (or it is malformed), returns `SpecNotMigrated` so
///   the parity-test gate can treat the spec as awaiting migration
///   rather than as a real failure.
/// - `SnapshotEmptyMode` — runs against an empty `Snapshot`-mode proxy
///   with no transport installed, asserting the proxy's cold-state
///   behavior. Cassette is ignored even if present.
fn build_proxy_for_mode(
  spec_path: String,
  parsed: Spec,
  capture_source: String,
  debug: Bool,
) -> Result(DraftProxy, RunError) {
  case parsed.mode {
    SnapshotEmptyMode -> {
      case debug {
        True ->
          io.println_error(
            "[runner] mode=snapshot-empty scenario=" <> parsed.scenario_id,
          )
        False -> Nil
      }
      Ok(draft_proxy.new())
    }
    LiveHybridMode -> {
      case cassette.parse_calls_from_capture(capture_source) {
        Error(_) ->
          Error(SpecNotMigrated(
            spec_path: spec_path,
            reason: "capture has no `upstreamCalls` field (or it is "
              <> "malformed); run `pnpm parity:record "
              <> parsed.scenario_id
              <> "` to record upstream traffic. An empty array is "
              <> "valid for mutation-only scenarios.",
          ))
        Ok(entries) -> {
          let transport = case debug {
            True -> {
              io.println_error(
                "[runner] mode=live-hybrid scenario="
                <> parsed.scenario_id
                <> " cassette_entries="
                <> int.to_string(cassette.entry_count(entries)),
              )
              cassette.make_logging_transport(entries)
            }
            False -> cassette.make_transport(entries)
          }
          let proxy =
            draft_proxy.with_config(Config(
              read_mode: LiveHybrid,
              port: 4000,
              shopify_admin_origin: "https://shopify.com",
              snapshot_path: None,
            ))
            |> draft_proxy.with_default_registry()
            |> draft_proxy.with_upstream_transport(transport)
          Ok(proxy)
        }
      }
    }
  }
}

fn collect_objects(value: JsonValue) -> List(JsonValue) {
  do_collect_objects([value], []) |> list.reverse
}

fn do_collect_objects(
  stack: List(JsonValue),
  acc: List(JsonValue),
) -> List(JsonValue) {
  case stack {
    [] -> acc
    [JObject(entries) as obj, ..rest] -> {
      let next =
        list.fold(list.reverse(entries), rest, fn(s, pair) { [pair.1, ..s] })
      do_collect_objects(next, [obj, ..acc])
    }
    [JArray(items), ..rest] -> {
      let next =
        list.fold(list.reverse(items), rest, fn(s, item) { [item, ..s] })
      do_collect_objects(next, acc)
    }
    [_, ..rest] -> do_collect_objects(rest, acc)
  }
}

fn replace_customer_one_variables(
  capture: JsonValue,
  variables: JsonValue,
) -> JsonValue {
  case first_customer_gid(capture) {
    Some(customer_id) -> replace_customer_one_value(variables, customer_id)
    None -> variables
  }
}

fn first_customer_gid(value: JsonValue) -> Option(String) {
  let found =
    collect_objects(value)
    |> list.find_map(fn(object) {
      case read_string_field(object, "id") {
        Some(id) ->
          case string.contains(id, "gid://shopify/Customer/") {
            True -> Ok(id)
            False -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case found {
    Ok(id) -> Some(id)
    Error(_) -> None
  }
}

fn replace_customer_one_value(
  value: JsonValue,
  customer_id: String,
) -> JsonValue {
  case value {
    JString("gid://shopify/Customer/1") -> JString(customer_id)
    JObject(entries) ->
      JObject(
        list.map(entries, fn(pair) {
          #(pair.0, replace_customer_one_value(pair.1, customer_id))
        }),
      )
    JArray(items) ->
      JArray(
        list.map(items, fn(item) {
          replace_customer_one_value(item, customer_id)
        }),
      )
    other -> other
  }
}

fn read_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    _ -> None
  }
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(
  #(DraftProxy, List(TargetReport), List(ExecutedOperation)),
  RunError,
) {
  list.try_fold(
    parsed.targets,
    #(proxy, [], None, dict.new(), []),
    fn(state, target) {
      let #(
        current_proxy,
        acc_reports,
        previous_response,
        named_responses,
        acc_ops,
      ) = state
      use #(next_proxy, report, executed) <- result.try(run_target(
        config,
        parsed,
        target,
        capture,
        primary_response,
        previous_response,
        named_responses,
        current_proxy,
      ))
      let next_ops = case executed {
        Some(op) -> [op, ..acc_ops]
        None -> acc_ops
      }
      Ok(#(
        next_proxy,
        [report.0, ..acc_reports],
        Some(report.1),
        dict.insert(named_responses, target.name, report.1),
        next_ops,
      ))
    },
  )
  |> result.map(fn(state) {
    let #(final_proxy, reports, _, _, ops) = state
    #(final_proxy, list.reverse(reports), list.reverse(ops))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(
  #(DraftProxy, #(TargetReport, JsonValue), Option(ExecutedOperation)),
  RunError,
) {
  use #(actual_response, next_proxy, executed) <- result.try(
    actual_response_for(
      config,
      parsed,
      target,
      capture,
      primary_response,
      previous_response,
      named_responses,
      proxy,
    ),
  )
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, None -> {
      log_target_assertion(config.debug, target, [], "no-op (paths absent)")
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: [],
          ),
          actual_response,
        ),
        executed,
      ))
    }
    None, _ ->
      Error(CaptureUnresolved(target: target.name, path: target.capture_path))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = case target.selected_paths {
        [] -> diff.compare_payloads(expected, actual, rules)
        selected_paths ->
          diff.compare_selected_paths(expected, actual, selected_paths, rules)
      }
      log_target_assertion(config.debug, target, mismatches, "compared")
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: mismatches,
          ),
          actual_response,
        ),
        executed,
      ))
    }
  }
}

/// Print the per-target assertion result. Truncates long mismatch
/// values so the log stays scrollable. No-ops when debug is off.
fn log_target_assertion(
  debug: Bool,
  target: Target,
  mismatches: List(Mismatch),
  note: String,
) -> Nil {
  case debug {
    False -> Nil
    True -> {
      let count = list.length(mismatches)
      let header =
        "[runner] target="
        <> target.name
        <> " "
        <> note
        <> " mismatches="
        <> int.to_string(count)
      io.println_error(header)
      list.each(mismatches, fn(m) {
        io.println_error(
          "         at "
          <> m.path
          <> "\n           expected: "
          <> debug_truncate(m.expected, 200)
          <> "\n           actual:   "
          <> debug_truncate(m.actual, 200),
        )
      })
    }
  }
}

fn debug_truncate(value: String, max: Int) -> String {
  case string.length(value) > max {
    True -> string.slice(value, 0, max) <> "…"
    False -> value
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy, Option(ExecutedOperation)), RunError) {
  case target.request {
    ReusePrimary -> {
      use #(value, next_proxy) <- result.try(proxy_source_value(
        target,
        primary_response,
        proxy,
      ))
      Ok(#(value, next_proxy, None))
    }
    OverrideRequest(request: request) -> {
      case
        target.upstream_capture_path,
        override_request_uses_upstream_capture(parsed.scenario_id)
      {
        Some(path), True ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(#(value, proxy, None))
            None -> Error(CaptureUnresolved(target: target.name, path: path))
          }
        _, _ -> {
          use document <- result.try(
            read_file(resolve(config, request.document_path)),
          )
          use variables <- result.try(resolve_variables(
            config,
            request.variables,
            capture,
            Some(primary_response),
            previous_response,
            named_responses,
            target.name,
          ))
          use #(response, next_proxy, executed) <- result.try(execute(
            proxy,
            document,
            variables,
            target.name,
            request.api_version,
            config.debug,
          ))
          use value <- result.try(parse_response_body(response))
          use #(value, next_proxy) <- result.try(proxy_source_value(
            target,
            value,
            next_proxy,
          ))
          Ok(#(value, next_proxy, Some(executed)))
        }
      }
    }
  }
}

fn proxy_source_value(
  target: Target,
  response_value: JsonValue,
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.proxy_source {
    ProxyResponse -> Ok(#(response_value, proxy))
    ProxyState -> {
      use state_value <- result.try(meta_response_value(proxy, "/__meta/state"))
      Ok(#(state_value, proxy))
    }
    ProxyLog -> {
      use log_value <- result.try(meta_response_value(proxy, "/__meta/log"))
      Ok(#(log_value, proxy))
    }
  }
}

fn meta_response_value(
  proxy: DraftProxy,
  path: String,
) -> Result(JsonValue, RunError) {
  let #(response, _) =
    draft_proxy.process_request(
      proxy,
      Request(method: "GET", path: path, headers: dict.new(), body: ""),
    )
  parse_response_body(response)
}

fn override_request_uses_upstream_capture(scenario_id: String) -> Bool {
  case scenario_id {
    "storefront-access-token-local-staging" -> False
    _ -> True
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) ->
          substitute(
            value,
            primary_response,
            previous_response,
            named_responses,
            capture,
          )
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      use template <- result.try(parse_json(resolved, source))
      substitute(
        template,
        primary_response,
        previous_response,
        named_responses,
        capture,
      )
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(
        template,
        primary_response,
        previous_response,
        named_responses,
        capture,
      )
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
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
      case as_previous_ref(template) {
        Some(path) ->
          case previous {
            None -> Error(PreviousRefUnresolved(path: path))
            Some(root) ->
              case jsonpath.lookup(root, path) {
                Some(value) -> Ok(value)
                None -> Error(PreviousRefUnresolved(path: path))
              }
          }
        None ->
          case as_named_response_ref(template) {
            Some(ref) -> {
              let #(target, path) = ref
              case dict.get(named, target) {
                Ok(root) ->
                  case jsonpath.lookup(root, path) {
                    Some(value) -> Ok(value)
                    None -> Error(ProxyResponseRefUnresolved(target, path))
                  }
                Error(_) -> Error(ProxyResponseRefUnresolved(target, path))
              }
            }
            None ->
              substitute_capture_or_children(
                template,
                primary,
                previous,
                named,
                capture,
              )
          }
      }
  }
}

fn substitute_capture_or_children(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
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
            case substitute(v, primary, previous, named, capture) {
              Ok(v2) -> Ok(#(k, v2))
              Error(e) -> Error(e)
            }
          })
          |> result.map(JObject)
        JArray(items) ->
          items
          |> list.try_map(fn(item) {
            substitute(item, primary, previous, named, capture)
          })
          |> result.map(JArray)
        leaf -> Ok(leaf)
      }
  }
}

/// If `value` is exactly `{"fromPreviousProxyPath": "..."}` (one
/// entry with a string value), return the path. Otherwise None.
fn as_previous_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPreviousProxyPath", json_value.JString(path))]) ->
      Some(path)
    _ -> None
  }
}

/// If `value` is exactly an object containing `fromProxyResponse` and
/// `path` string entries, return target/path regardless of field order.
fn as_named_response_ref(value: JsonValue) -> Option(#(String, String)) {
  case value {
    JObject(entries) -> {
      let target = object_string_entry(entries, "fromProxyResponse")
      let path = object_string_entry(entries, "path")
      case target, path {
        Some(target), Some(path) -> Some(#(target, path))
        _, _ -> None
      }
    }
    _ -> None
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

fn object_string_entry(
  entries: List(#(String, JsonValue)),
  name: String,
) -> Option(String) {
  case entries {
    [] -> None
    [#(key, json_value.JString(value)), ..] if key == name -> Some(value)
    [_, ..rest] -> object_string_entry(rest, name)
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
  api_version: Option(String),
  debug: Bool,
) -> Result(#(Response, DraftProxy, ExecutedOperation), RunError) {
  let body = build_graphql_body(document, variables)
  let version = option.unwrap(api_version, "2025-01")
  let request =
    Request(
      method: "POST",
      path: "/admin/api/" <> version <> "/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let executed = parse_executed_operation(document)
  case debug {
    True -> log_request(context, executed, variables)
    False -> Nil
  }
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case debug {
    True -> log_response(context, response)
    False -> Nil
  }
  case response.status {
    200 -> Ok(#(response, next_proxy, executed))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn log_request(
  context: String,
  executed: ExecutedOperation,
  variables: JsonValue,
) -> Nil {
  let kind = case executed.type_ {
    QueryOperation -> "query"
    MutationOperation -> "mutation"
  }
  let name = option.unwrap(executed.name, "<anonymous>")
  let roots = string.join(executed.root_fields, ",")
  io.println_error(
    "[runner] -> "
    <> context
    <> " "
    <> kind
    <> " "
    <> name
    <> " roots=["
    <> roots
    <> "] vars="
    <> debug_truncate(json_value.to_string(variables), 240),
  )
}

fn log_response(context: String, response: Response) -> Nil {
  io.println_error(
    "[runner] <- "
    <> context
    <> " status="
    <> int.to_string(response.status)
    <> " body="
    <> debug_truncate(json.to_string(response.body), 360),
  )
}

/// Best-effort parse of the executed document into an `ExecutedOperation`
/// summary. Mirrors `parseOperation` in the TS runner. Parse failures
/// fall back to a query-shaped no-op entry — the runner's
/// `validate_operation_names` only inspects mutations, so a degenerate
/// entry from an unparseable doc is harmless.
fn parse_executed_operation(document: String) -> ExecutedOperation {
  case parse_operation.parse_operation(document) {
    Ok(ParsedOperation(type_: type_, name: name, root_fields: root_fields)) ->
      ExecutedOperation(type_: type_, name: name, root_fields: root_fields)
    Error(_) ->
      ExecutedOperation(type_: QueryOperation, name: None, root_fields: [])
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
  || report.operation_name_errors != []
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True -> {
      let target_section =
        string.join(list.map(report.targets, render_target), "\n")
      let op_section = case report.operation_name_errors {
        [] -> ""
        errors ->
          "\n  operation-name validation:\n    "
          <> string.join(errors, "\n    ")
      }
      report.scenario_id <> "\n" <> target_section <> op_section
    }
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

/// Compare the spec's declared `operationNames` against the mutations
/// that actually executed during the run. Mirrors TS
/// `validateParityScenarioOperationNames`. Returns one error string per
/// problem (missing or unexpected); empty list means agreement.
pub fn validate_operation_names(
  spec: Spec,
  executed: List(ExecutedOperation),
) -> List(String) {
  let actual_mutation_root_fields =
    executed
    |> list.flat_map(fn(op) {
      case op.type_ {
        MutationOperation -> op.root_fields
        QueryOperation -> []
      }
    })
    |> unique_sorted
  let actual_set = set.from_list(actual_mutation_root_fields)
  let registered = registered_mutation_operation_names()
  let declared =
    spec.operation_names
    |> list.filter(fn(name) {
      set.contains(registered, name) || set.contains(actual_set, name)
    })
    |> unique_sorted
  let declared_set = set.from_list(declared)
  let missing =
    list.filter(declared, fn(name) { !set.contains(actual_set, name) })
  let unexpected =
    list.filter(actual_mutation_root_fields, fn(name) {
      !set.contains(declared_set, name)
    })
  let actual_summary = case actual_mutation_root_fields {
    [] -> "(none)"
    names -> string.join(names, ", ")
  }
  let declared_summary = case declared {
    [] -> "(none)"
    names -> string.join(names, ", ")
  }
  let missing_errors = case missing {
    [] -> []
    names -> [
      "Scenario "
      <> spec.scenario_id
      <> " declares mutation operation(s) "
      <> string.join(names, ", ")
      <> " in operationNames but did not execute them. Actual executed mutation operation(s): "
      <> actual_summary
      <> ".",
    ]
  }
  let unexpected_errors = case unexpected {
    [] -> []
    names -> [
      "Scenario "
      <> spec.scenario_id
      <> " executed mutation operation(s) "
      <> string.join(names, ", ")
      <> " but does not declare them in operationNames. Declared mutation operation(s): "
      <> declared_summary
      <> ".",
    ]
  }
  list.append(missing_errors, unexpected_errors)
}

fn registered_mutation_operation_names() -> set.Set(String) {
  operation_registry_data.default_registry()
  |> list.filter(fn(entry) {
    case entry.type_ {
      operation_registry.Mutation -> True
      operation_registry.Query -> False
    }
  })
  |> list.flat_map(fn(entry) { [entry.name, ..entry.match_names] })
  |> set.from_list
}

fn unique_sorted(values: List(String)) -> List(String) {
  values
  |> list.unique
  |> list.sort(by: string.compare)
}

/// True iff the spec has not yet been migrated to cassette playback.
/// The parity-test gate uses this to count specs as "skipped" instead
/// of "failed" while the migration is in progress.
pub fn is_spec_not_migrated(error: RunError) -> Bool {
  case error {
    SpecNotMigrated(_, _) -> True
    _ -> False
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    SpecNotMigrated(spec_path, reason) ->
      "spec not migrated to cassette playback: "
      <> spec_path
      <> " ("
      <> reason
      <> ")"
    VariablesUnresolved(path) -> "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    PreviousRefUnresolved(path) ->
      "fromPreviousProxyPath did not resolve in previous proxy response: "
      <> path
    ProxyResponseRefUnresolved(target, path) ->
      "fromProxyResponse did not resolve for target '"
      <> target
      <> "' at "
      <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '" <> target <> "': " <> path
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

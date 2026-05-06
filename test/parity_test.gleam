//// Pure-Gleam parity scenario suite.
////
//// This is the Gleam port's replacement for
//// `tests/unit/conformance-parity-scenarios.test.ts`. The suite discovers every
//// parity spec under `config/parity-specs/**`, drives each GraphQL document
//// through `draft_proxy.process_request`, and compares proxy output to the
//// corresponding capture slice using the spec's `expectedDifferences` matchers.

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import parity/diff
import parity/discover
import parity/runner

const parity_root: String = "config/parity-specs"

pub type Outcome {
  Passed(spec_path: String)
  Failed(spec_path: String, message: String)
}

pub fn parity_specs_admin_to_discounts_pass_test() {
  run_shard("admin-discounts")
}

pub fn parity_specs_events_to_metaobjects_pass_test() {
  run_shard("events-metaobjects")
}

pub fn parity_specs_online_store_to_products_pass_test() {
  run_shard("online-products")
}

pub fn parity_specs_saved_searches_to_webhooks_pass_test() {
  run_shard("saved-webhooks")
}

pub fn parity_shards_cover_every_discovered_spec_test() {
  let assert Ok(discovered_paths) = discover.discover(parity_root)
  let spec_paths =
    discovered_paths
    |> list.map(repo_relative_path)
    |> list.sort(by: string.compare)
  let unassigned =
    list.filter(spec_paths, fn(path) {
      case shard_for_domain(spec_domain(path)) {
        Some(_) -> False
        None -> True
      }
    })
  case unassigned {
    [] -> Nil
    _ -> panic as render_section("unassigned parity specs", unassigned)
  }
}

fn run_shard(label: String) {
  let assert Ok(discovered_paths) = discover.discover(parity_root)
  let spec_paths =
    discovered_paths
    |> list.map(repo_relative_path)
    |> list.sort(by: string.compare)
    |> list.filter(fn(path) {
      case shard_for_domain(spec_domain(path)) {
        Some(shard) -> shard == label
        None -> False
      }
    })
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
    [] ->
      case spec_paths {
        [] -> {
          let message = "parity shard '" <> label <> "' matched no specs"
          panic as message
        }
        _ -> Nil
      }
    _ -> panic as render_summary(label, failures)
  }
}

fn spec_domain(path: String) -> String {
  case string.split(path, on: "/") {
    ["config", "parity-specs", domain, ..] -> domain
    [domain, ..] -> domain
    _ -> ""
  }
}

fn shard_for_domain(domain: String) -> Option(String) {
  case domain {
    "admin-platform" -> Some("admin-discounts")
    "apps" -> Some("admin-discounts")
    "b2b" -> Some("admin-discounts")
    "bulk-operations" -> Some("admin-discounts")
    "customers" -> Some("admin-discounts")
    "discounts" -> Some("admin-discounts")
    "events" -> Some("events-metaobjects")
    "functions" -> Some("events-metaobjects")
    "gift-cards" -> Some("events-metaobjects")
    "localization" -> Some("events-metaobjects")
    "marketing" -> Some("events-metaobjects")
    "markets" -> Some("events-metaobjects")
    "media" -> Some("events-metaobjects")
    "metafields" -> Some("events-metaobjects")
    "metaobjects" -> Some("events-metaobjects")
    "online-store-article-media-navigation-follow-through.json" ->
      Some("online-products")
    "online-store" -> Some("online-products")
    "orders" -> Some("online-products")
    "payments" -> Some("online-products")
    "privacy" -> Some("online-products")
    "products" -> Some("online-products")
    "saved-searches" -> Some("saved-webhooks")
    "segments" -> Some("saved-webhooks")
    "shipping-fulfillments" -> Some("saved-webhooks")
    "store-properties" -> Some("saved-webhooks")
    "webhooks" -> Some("saved-webhooks")
    _ -> None
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

fn render_summary(shard: String, failures: List(String)) -> String {
  string.join(
    [
      "Gleam parity corpus failed for shard '" <> shard <> "'.",
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

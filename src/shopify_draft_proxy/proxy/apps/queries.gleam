//// Apps query handling.

import gleam/dict.{type Dict}
import gleam/json
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps/serializers
import shopify_draft_proxy/proxy/graphql_helpers
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/state/store.{type Store}

@internal
pub fn is_app_query_root(name: String) -> Bool {
  case name {
    "app" -> True
    "appByHandle" -> True
    "appByKey" -> True
    "appInstallation" -> True
    "appInstallations" -> True
    "currentAppInstallation" -> True
    _ -> False
  }
}

/// Process an apps query document and return a JSON `data` envelope.
/// Mirrors `handleAppQuery`. The store argument supplies effective
/// (base + staged) records.
@internal
pub fn handle_app_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(json.Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
    Ok(fields) -> {
      let fragments = graphql_helpers.get_document_fragments(document)
      Ok(serializers.serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(json.Json, root_field.RootFieldError) {
  use data <- result.try(handle_app_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// True iff the app-domain store has any local app/installation/billing/access
/// records. LiveHybrid app reads pass through while cold, but once mutations
/// stage app state, downstream reads must stay local instead of forwarding
/// synthetic billing/install IDs upstream.
@internal
pub fn local_has_app_state(proxy: DraftProxy) -> Bool {
  let base = proxy.store.base_state
  let staged = proxy.store.staged_state
  dict.size(base.apps) > 0
  || dict.size(staged.apps) > 0
  || dict.size(base.app_installations) > 0
  || dict.size(staged.app_installations) > 0
  || has_option(base.current_installation_id)
  || has_option(staged.current_installation_id)
  || dict.size(base.app_subscriptions) > 0
  || dict.size(staged.app_subscriptions) > 0
  || dict.size(base.app_subscription_line_items) > 0
  || dict.size(staged.app_subscription_line_items) > 0
  || dict.size(base.app_one_time_purchases) > 0
  || dict.size(staged.app_one_time_purchases) > 0
  || dict.size(base.app_usage_records) > 0
  || dict.size(staged.app_usage_records) > 0
  || dict.size(base.delegated_access_tokens) > 0
  || dict.size(staged.delegated_access_tokens) > 0
}

fn has_option(value: Option(a)) -> Bool {
  case value {
    Some(_) -> True
    None -> False
  }
}

/// Pattern 1: app reads are transparent LiveHybrid passthroughs until
/// local app-domain state exists. After app billing/access mutations stage
/// state, the same roots must resolve locally so read-after-write and
/// read-after-uninstall behavior never consult upstream.
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "currentAppInstallation" ->
      !local_has_app_state(proxy)
    _, _ -> False
  }
}

/// Domain entrypoint for app queries. The registry now lets implemented app
/// reads reach this handler; LiveHybrid passthrough remains a domain decision
/// so staged billing/access scenarios stay local-only after their first write.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(proxy, parsed.type_, primary_root_field)
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #("message", json.string("Failed to handle apps query")),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

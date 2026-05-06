//// Apps billing/access draft runtime.
////
//// Public entrypoint for app query, serializer, and mutation handling. The
//// implementation is split under `proxy/apps/` by concern.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps/mutations
import shopify_draft_proxy/proxy/apps/queries
import shopify_draft_proxy/proxy/apps/serializers
import shopify_draft_proxy/proxy/apps/types
import shopify_draft_proxy/proxy/graphql_helpers.{type FragmentMap}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// Errors specific to the apps handler. Mirrors `WebhooksError`.
pub type AppsError {
  ParseFailed(root_field.RootFieldError)
}

pub type UserError =
  types.UserError

/// Predicate matching the TS `APP_QUERY_ROOTS` set.
pub fn is_app_query_root(name: String) -> Bool {
  queries.is_app_query_root(name)
}

/// Predicate matching the TS `APP_MUTATION_ROOTS` set.
pub fn is_app_mutation_root(name: String) -> Bool {
  mutations.is_app_mutation_root(name)
}

/// Process an apps query document and return a JSON `data` envelope.
/// Mirrors `handleAppQuery`. The store argument supplies effective
/// (base + staged) records.
pub fn handle_app_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AppsError) {
  case queries.handle_app_query(store, document, variables) {
    Error(err) -> Error(ParseFailed(err))
    Ok(data) -> Ok(data)
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AppsError) {
  case queries.process(store, document, variables) {
    Error(err) -> Error(ParseFailed(err))
    Ok(data) -> Ok(data)
  }
}

/// True iff the app-domain store has any local app/installation/billing/access
/// records. LiveHybrid app reads pass through while cold, but once mutations
/// stage app state, downstream reads must stay local instead of forwarding
/// synthetic billing/install IDs upstream.
pub fn local_has_app_state(proxy: DraftProxy) -> Bool {
  queries.local_has_app_state(proxy)
}

/// Domain entrypoint for app queries. The registry now lets implemented app
/// reads reach this handler; LiveHybrid passthrough remains a domain decision
/// so staged billing/access scenarios stay local-only after their first write.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn serialize_app_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_app_node_by_id(store, id, selections, fragments)
}

pub fn serialize_app_installation_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_app_installation_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

pub fn serialize_app_subscription_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_app_subscription_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

pub fn serialize_app_one_time_purchase_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_app_one_time_purchase_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

pub fn serialize_app_usage_record_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_app_usage_record_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

/// Process an apps mutation document. Mirrors `handleAppMutation`. Each
/// mutation handler stages its records and returns a payload; the
/// outcomes are combined into a single `{"data": {...}}` envelope. Apps
/// mutations don't currently produce top-level error envelopes — every
/// failure mode is surfaced through `userErrors` instead.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

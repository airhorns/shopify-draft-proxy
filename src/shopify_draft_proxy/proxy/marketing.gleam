//// Public entrypoint for marketing handling.
////
//// Implementation is split across marketing/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option, None}
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/graphql_helpers.{type SourceValue}
import shopify_draft_proxy/proxy/marketing/mutations
import shopify_draft_proxy/proxy/marketing/queries
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, respond_to_query,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type MarketingError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_marketing_query_root(name: String) -> Bool {
  case name {
    "marketingActivity" -> True
    "marketingActivities" -> True
    "marketingEvent" -> True
    "marketingEvents" -> True
    _ -> False
  }
}

pub fn is_marketing_mutation_root(name: String) -> Bool {
  mutations.is_marketing_mutation_root(name)
}

pub fn handle_marketing_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketingError) {
  handle_marketing_query_for_app(store, document, variables, None)
}

pub fn handle_marketing_query_for_app(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, MarketingError) {
  case
    queries.handle_marketing_query_for_app(
      store,
      document,
      variables,
      requesting_api_client_id,
    )
  {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketingError) {
  use data <- result.try(handle_marketing_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    queries.handle_marketing_query_for_app(
      proxy.store,
      document,
      variables,
      app_identity.read_requesting_api_client_id(request.headers),
    )
      |> result.map(graphql_helpers.wrap_data)
      |> result.map_error(ParseFailed),
    "Failed to handle marketing query",
  )
}

pub fn hydrate_marketing_from_upstream_payload(
  store: Store,
  payload: SourceValue,
) -> Store {
  queries.hydrate_marketing_from_upstream_payload(store, payload)
}

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

//// Public entrypoint for metaobject definition and metaobject handling.
////
//// Implementation is split across the metaobject_definitions/* submodules;
//// this file keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option, None}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/metaobject_definitions/mutations
import shopify_draft_proxy/proxy/metaobject_definitions/queries
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type MetaobjectDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_metaobject_definitions_query_root(name: String) -> Bool {
  queries.is_metaobject_definitions_query_root(name)
}

pub fn is_metaobject_definitions_mutation_root(name: String) -> Bool {
  mutations.is_metaobject_definitions_mutation_root(name)
}

pub fn handle_metaobject_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetaobjectDefinitionsError) {
  handle_metaobject_definitions_query_with_app_id(
    store,
    document,
    variables,
    None,
  )
}

pub fn handle_metaobject_definitions_query_with_app_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, MetaobjectDefinitionsError) {
  case
    queries.handle_metaobject_definitions_query_with_app_id(
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
) -> Result(Json, MetaobjectDefinitionsError) {
  process_with_requesting_api_client_id(store, document, variables, None)
}

pub fn process_with_requesting_api_client_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, MetaobjectDefinitionsError) {
  case
    queries.process_with_requesting_api_client_id(
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

pub fn process_mutation_with_headers(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  request_headers: Dict(String, String),
) -> MutationOutcome {
  mutations.process_mutation_with_headers(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
    request_headers,
  )
}

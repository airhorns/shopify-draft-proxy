//// Public entrypoint for admin-platform handling.
////
//// Implementation is split across admin_platform/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform/mutations
import shopify_draft_proxy/proxy/admin_platform/queries
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type AdminPlatformError {
  ParseFailed(root_field.RootFieldError)
}

pub fn list_supported_admin_platform_node_types() -> List(String) {
  queries.list_supported_admin_platform_node_types()
}

pub fn is_admin_platform_query_root(name: String) -> Bool {
  queries.is_admin_platform_query_root(name)
}

pub fn is_admin_platform_mutation_root(name: String) -> Bool {
  mutations.is_admin_platform_mutation_root(name)
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

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AdminPlatformError) {
  process_with_shop_origin(store, "", document, variables)
}

pub fn process_with_shop_origin(
  store: Store,
  shop_origin: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AdminPlatformError) {
  case
    queries.process_with_shop_origin(store, shop_origin, document, variables)
  {
    Ok(body) -> Ok(body)
    Error(err) -> Error(ParseFailed(err))
  }
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

pub fn process_mutation_with_shop_origin(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  mutations.process_mutation_with_shop_origin(
    store,
    identity,
    shop_origin,
    document,
    variables,
  )
}

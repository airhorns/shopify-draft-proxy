//// B2B company domain public entrypoint.
////
//// Implementation is split across the b2b/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b/mutations
import shopify_draft_proxy/proxy/b2b/queries
import shopify_draft_proxy/proxy/b2b/serializers
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

pub type B2BError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_b2b_query_root(name: String) -> Bool {
  queries.is_b2b_query_root(name)
}

pub fn is_b2b_mutation_root(name: String) -> Bool {
  mutations.is_b2b_mutation_root(name)
}

pub fn local_has_b2b_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_b2b_id(proxy, variables)
}

pub fn local_has_staged_b2b(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_staged_b2b(proxy, variables)
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
) -> Result(Json, B2BError) {
  case queries.process(store, document, variables) {
    Ok(envelope) -> Ok(envelope)
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

pub fn serialize_company_address_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_company_address_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

pub fn serialize_company_contact_role_assignment_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_company_contact_role_assignment_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

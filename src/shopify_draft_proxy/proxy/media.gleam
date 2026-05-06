//// Public entrypoint for Files API and staged-upload handling.
////
//// Implementation lives under proxy/media/* so this file can preserve the
//// original public API surface without keeping the whole domain in one module.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/media/mutations
import shopify_draft_proxy/proxy/media/queries
import shopify_draft_proxy/proxy/media/types
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type MediaError =
  types.MediaError

pub fn is_media_query_root(name: String) -> Bool {
  queries.is_media_query_root(name)
}

pub fn is_media_mutation_root(name: String) -> Bool {
  mutations.is_media_mutation_root(name)
}

pub fn handle_media_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MediaError) {
  queries.handle_media_query(store, document, variables)
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MediaError) {
  queries.process(store, document, variables)
}

/// Uniform query entrypoint matching the dispatcher's signature.
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

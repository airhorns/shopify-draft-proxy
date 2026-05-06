//// Public entrypoint for bulk-operations domain handling.
////
//// Implementation is split across the bulk_operations/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/bulk_operations/mutations
import shopify_draft_proxy/proxy/bulk_operations/queries
import shopify_draft_proxy/proxy/bulk_operations/types
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type BulkOperationsError =
  types.BulkOperationsError

pub type UserError =
  mutations.UserError

pub fn is_bulk_operations_query_root(name: String) -> Bool {
  queries.is_bulk_operations_query_root(name)
}

pub fn is_bulk_operations_mutation_root(name: String) -> Bool {
  mutations.is_bulk_operations_mutation_root(name)
}

pub fn handle_bulk_operations_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, BulkOperationsError) {
  case queries.handle_bulk_operations_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(types.ParseFailed(err))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, BulkOperationsError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(types.ParseFailed(err))
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

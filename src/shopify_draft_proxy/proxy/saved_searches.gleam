//// Public entrypoint for saved-search domain handling.
////
//// Implementation is split across saved_searches/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/saved_searches/mutations
import shopify_draft_proxy/proxy/saved_searches/queries
import shopify_draft_proxy/proxy/saved_searches/types as saved_search_types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type SavedSearchFilter, type SavedSearchRecord,
}

pub type SavedSearchesError {
  ParseFailed(root_field.RootFieldError)
}

pub type UserError {
  UserError(field: Option(List(String)), message: String)
}

pub type ParsedSavedSearchQuery {
  ParsedSavedSearchQuery(
    filters: List(SavedSearchFilter),
    search_terms: String,
    canonical_query: String,
  )
}

pub fn is_saved_search_query_root(name: String) -> Bool {
  queries.is_saved_search_query_root(name)
}

pub fn is_saved_search_mutation_root(name: String) -> Bool {
  mutations.is_saved_search_mutation_root(name)
}

pub fn handle_saved_search_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SavedSearchesError) {
  case queries.handle_saved_search_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(queries.ParseFailed(err)) -> Error(ParseFailed(err))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SavedSearchesError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(queries.ParseFailed(err)) -> Error(ParseFailed(err))
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

pub fn root_field_resource_type(name: String) -> Result(String, Nil) {
  queries.root_field_resource_type(name)
}

pub fn order_saved_searches() -> List(SavedSearchRecord) {
  queries.order_saved_searches()
}

pub fn draft_order_saved_searches() -> List(SavedSearchRecord) {
  queries.draft_order_saved_searches()
}

pub fn saved_search_cursor(record: SavedSearchRecord) -> String {
  queries.saved_search_cursor(record)
}

pub fn parse_saved_search_query(raw_query: String) -> ParsedSavedSearchQuery {
  let parsed = saved_search_types.parse_saved_search_query(raw_query)
  ParsedSavedSearchQuery(
    filters: parsed.filters,
    search_terms: parsed.search_terms,
    canonical_query: parsed.canonical_query,
  )
}

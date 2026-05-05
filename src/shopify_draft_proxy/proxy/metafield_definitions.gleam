//// Public entrypoint for the metafield definitions domain.
////
//// Implementation is split across metafield_definitions/* submodules; this
//// file keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/metafield_definitions/mutations
import shopify_draft_proxy/proxy/metafield_definitions/queries
import shopify_draft_proxy/proxy/metafield_definitions/types as definition_types
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type MetafieldDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

pub type UserError {
  UserError(field: Option(List(String)), message: String, code: String)
}

pub type MetafieldsSetUserError {
  MetafieldsSetUserError(
    field: List(String),
    message: String,
    code: Option(String),
    element_index: Option(Int),
  )
}

pub fn is_metafield_definitions_query_root(name: String) -> Bool {
  queries.is_metafield_definitions_query_root(name)
}

pub fn is_metafield_definitions_mutation_root(name: String) -> Bool {
  mutations.is_metafield_definitions_mutation_root(name)
}

pub fn local_has_metafield_definition_state(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_metafield_definition_state(proxy, variables)
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    query,
    variables,
  )
}

pub fn handle_metafield_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetafieldDefinitionsError) {
  queries.handle_metafield_definitions_query(store, document, variables)
  |> map_query_error
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetafieldDefinitionsError) {
  queries.process(store, document, variables)
  |> map_query_error
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

fn map_query_error(
  result: Result(Json, definition_types.MetafieldDefinitionsError),
) -> Result(Json, MetafieldDefinitionsError) {
  case result {
    Ok(data) -> Ok(data)
    Error(definition_types.ParseFailed(err)) -> Error(ParseFailed(err))
  }
}

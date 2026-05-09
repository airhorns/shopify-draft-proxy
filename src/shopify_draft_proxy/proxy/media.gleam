//// Public entrypoint for Files API and staged-upload handling.
////
//// Implementation lives under proxy/media/* so this file can preserve the
//// original public API surface without keeping the whole domain in one module.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcObject, SrcString,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/media/mutations
import shopify_draft_proxy/proxy/media/queries
import shopify_draft_proxy/proxy/media/serializers
import shopify_draft_proxy/proxy/media/types
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type Config, type DraftProxy, type Request, type Response,
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

pub fn process_mutation_with_config(
  config: Config,
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation_with_config(
    config,
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

pub fn serialize_file_node_by_id(
  store: Store,
  id: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Option(Json) {
  case store.get_effective_file_by_id(store, id) {
    Some(record) ->
      Some(project_node_source(
        serializers.file_source(record),
        typename,
        selections,
        fragments,
      ))
    None -> None
  }
}

fn project_node_source(
  source: SourceValue,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    source_with_typename(source, typename),
    selections,
    fragments,
  )
}

fn source_with_typename(source: SourceValue, typename: String) -> SourceValue {
  case source {
    SrcObject(fields) ->
      SrcObject(dict.insert(fields, "__typename", SrcString(typename)))
    _ -> source
  }
}

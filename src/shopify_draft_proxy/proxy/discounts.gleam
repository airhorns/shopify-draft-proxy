//// Public entrypoint for Discounts domain handling.
////
//// Implementation is split across the discounts/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/discounts/mutations
import shopify_draft_proxy/proxy/discounts/queries
import shopify_draft_proxy/proxy/discounts/types as discount_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcObject, SrcString,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type DiscountsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_discount_query_root(name: String) -> Bool {
  queries.is_discount_query_root(name)
}

pub fn is_discount_mutation_root(name: String) -> Bool {
  mutations.is_discount_mutation_root(name)
}

pub fn local_has_discount_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_discount_id(proxy, variables)
}

pub fn local_has_staged_discounts(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_staged_discounts(proxy, variables)
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
) -> Result(Json, DiscountsError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn handle_discount_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, DiscountsError) {
  case queries.handle_discount_query(store, document, variables) {
    Ok(data) -> Ok(data)
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

pub fn serialize_discount_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_discount_by_id(store, id) {
    Some(record) ->
      project_node_source(
        queries.discount_node_source(record),
        "DiscountNode",
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_discount_owner_node_by_id(
  store: Store,
  id: String,
  owner_kind: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_discount_by_id(store, id) {
    Some(record) if record.owner_kind == owner_kind ->
      project_node_source(
        discount_types.discount_owner_source(record),
        typename,
        selections,
        fragments,
      )
    _ -> json.null()
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

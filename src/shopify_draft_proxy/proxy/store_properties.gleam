//// Public entrypoint for Store Properties handling.
////
//// Implementation is split across store_properties/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/store_properties/mutations
import shopify_draft_proxy/proxy/store_properties/queries
import shopify_draft_proxy/proxy/store_properties/serializers
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{type ShopDomainRecord, type ShopRecord}

pub type StorePropertiesError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_store_properties_query_root(name: String) -> Bool {
  queries.is_store_properties_query_root(name)
}

pub fn is_store_properties_mutation_root(name: String) -> Bool {
  mutations.is_store_properties_mutation_root(name)
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
) -> Result(Json, StorePropertiesError) {
  queries.process(store, document, variables)
  |> result.map_error(ParseFailed)
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

pub fn serialize_location_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_location_node_by_id(store, id, selections, fragments)
}

pub fn serialize_shop_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_shop_node_by_id(store, id, selections, fragments)
}

pub fn serialize_shop_address_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_shop_address_node_by_id(
    store,
    id,
    selections,
    fragments,
  )
}

pub fn serialize_shop_policy_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_shop_policy_node_by_id(store, id, selections, fragments)
}

pub fn primary_domain_for_id(
  store: Store,
  id: String,
) -> Option(ShopDomainRecord) {
  serializers.primary_domain_for_id(store, id)
}

pub fn shop_source(shop: ShopRecord) -> SourceValue {
  serializers.shop_source(shop)
}

pub fn shop_domain_source(domain: ShopDomainRecord) -> SourceValue {
  serializers.shop_domain_source(domain)
}

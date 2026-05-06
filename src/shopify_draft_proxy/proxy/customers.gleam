//// Customer domain public entrypoint.
////
//// The implementation is split by concern under `proxy/customers/`; this file
//// preserves the public API consumed by draft proxy dispatch, cross-domain node
//// resolution, and customer tests.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/customers/customer_types
import shopify_draft_proxy/proxy/customers/mutations
import shopify_draft_proxy/proxy/customers/queries
import shopify_draft_proxy/proxy/customers/serializers
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

pub type CustomersError =
  customer_types.CustomersError

pub fn is_customer_query_root(name: String) -> Bool {
  queries.is_customer_query_root(name)
}

pub fn is_customer_mutation_root(name: String) -> Bool {
  mutations.is_customer_mutation_root(name)
}

pub fn local_has_customer_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_customer_id(proxy, variables)
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

pub fn handle_customer_query(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  queries.handle_customer_query(proxy, document, variables)
}

pub fn process(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  queries.process(proxy, document, variables)
}

pub fn serialize_customer_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  serializers.serialize_customer_node_by_id(store, id, selections, fragments)
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

//// Mirrors `src/proxy/functions.ts`.
////
//// Pass 18 ships the five query roots (`validation`, `validations`,
//// `cartTransforms`, `shopifyFunction`, `shopifyFunctions`) plus the six
//// mutation roots (`validationCreate`/`Update`/`Delete`,
//// `cartTransformCreate`/`Delete`, `taxAppConfigure`).
////
//// Validation mutations still preserve the legacy local helper that mints
//// synthetic function metadata for local fixtures. Cart-transform creates
//// follow Shopify's Function-resolution guardrails: ambiguous, missing,
//// unknown, duplicate, or wrong-API Function references return userErrors
//// before staging any local CartTransform. The mutation pipeline returns a
//// `MutationOutcome` carrying the updated store + identity registry +
//// staged GIDs, matching the apps/webhooks/saved-search shape.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/functions/helpers
import shopify_draft_proxy/proxy/functions/mutations
import shopify_draft_proxy/proxy/functions/queries
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// Errors specific to the functions handler. Mirrors `AppsError`.
pub type FunctionsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching the TS `FUNCTION_QUERY_ROOTS` set.
pub fn is_function_query_root(name: String) -> Bool {
  queries.is_function_query_root(name)
}

/// Predicate matching the TS `FUNCTION_MUTATION_ROOTS` set.
pub fn is_function_mutation_root(name: String) -> Bool {
  mutations.is_function_mutation_root(name)
}

/// Process a functions query document and return a JSON `data`
/// envelope. Mirrors `handleFunctionQuery`.
pub fn handle_function_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, FunctionsError) {
  case queries.handle_function_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, FunctionsError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

/// True when functions-domain reads need local handling because the
/// proxy already knows about function metadata or staged lifecycle
/// effects. In LiveHybrid, cold reads can be forwarded upstream
/// verbatim; once any local function metadata exists, reads must stay
/// local so staged Validation / CartTransform state remains visible.
pub fn local_has_function_metadata(proxy: DraftProxy) -> Bool {
  queries.local_has_function_metadata(proxy)
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

/// Process a functions mutation document. Mirrors
/// `handleFunctionMutation`.
/// Pattern 2: dispatched LiveHybrid function metadata mutations first
/// try to hydrate referenced ShopifyFunction owner/app metadata from
/// upstream, then stage the mutation locally. Cart-transform creation
/// requires the referenced Function to resolve locally or from that
/// upstream lookup before it stages any local write.
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

/// Mirror `normalizeFunctionHandle`. Lowercases, trims, replaces runs of
/// disallowed characters with `-`, strips leading/trailing `-`, and
/// returns `local-function` if the result is empty.
pub fn normalize_function_handle(handle: String) -> String {
  helpers.normalize_function_handle(handle)
}

/// Build a deterministic ShopifyFunction gid from a handle. Mirrors
/// `shopifyFunctionIdFromHandle`.
pub fn shopify_function_id_from_handle(handle: String) -> String {
  helpers.shopify_function_id_from_handle(handle)
}

/// Convert a handle to a human-readable title. Mirrors `titleFromHandle`
/// — splits on `-`, `_`, and whitespace; drops empty segments;
/// title-cases each segment; joins with a single space.
pub fn title_from_handle(handle: String) -> String {
  helpers.title_from_handle(handle)
}

//// Segments draft runtime.
////
//// Public entrypoint for segment query, mutation, and serializer handling. The
//// implementation is split under `proxy/segments/` by concern.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/segments/mutations
import shopify_draft_proxy/proxy/segments/queries
import shopify_draft_proxy/proxy/segments/types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// Errors specific to the segments handler.
pub type SegmentsError =
  types.SegmentsError

/// User-error payload. Mirrors selected Shopify user-error fields.
pub type UserError =
  types.UserError

/// Predicate matching the supported subset of `SEGMENT_QUERY_ROOTS`.
pub fn is_segment_query_root(name: String) -> Bool {
  queries.is_segment_query_root(name)
}

/// Predicate matching the supported subset of `SEGMENT_MUTATION_ROOTS`.
pub fn is_segment_mutation_root(name: String) -> Bool {
  mutations.is_segment_mutation_root(name)
}

/// True iff any string variable names a segment that local state must answer.
pub fn local_has_segment_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_segment_id(proxy, variables)
}

/// True iff segment lifecycle state has been staged locally.
pub fn local_has_staged_segments(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_staged_segments(proxy, variables)
}

/// Process a segments query document and return a JSON `data` envelope.
pub fn handle_segments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  queries.handle_segments_query(store, document, variables)
}

/// Domain entrypoint for segment queries.
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

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  queries.process(store, document, variables)
}

/// Process a segments mutation document.
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

/// Trim whitespace from a segment name. Mirrors `normalizeSegmentName`.
pub fn normalize_segment_name(name: String) -> String {
  mutations.normalize_segment_name(name)
}

/// Resolve a segment name against existing names, appending " (N)" until free.
pub fn resolve_unique_segment_name(
  store: Store,
  requested: String,
  current_id: Option(String),
) -> Result(String, UserError) {
  mutations.resolve_unique_segment_name(store, requested, current_id)
}

/// Validate a segment query string.
pub fn validate_segment_query(
  raw: Option(String),
  field_path: List(String),
) -> List(UserError) {
  mutations.validate_segment_query(raw, field_path)
}

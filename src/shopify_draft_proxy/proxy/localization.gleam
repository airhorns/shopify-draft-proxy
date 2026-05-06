//// Public entrypoint for localization handling.
////
//// Implementation is split across localization/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/localization/mutations
import shopify_draft_proxy/proxy/localization/queries
import shopify_draft_proxy/proxy/localization/types as localization_types
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type LocalizationError {
  ParseFailed(root_field.RootFieldError)
}

pub type TranslationErrorCode =
  localization_types.TranslationErrorCode

pub type TranslatableContent =
  localization_types.TranslatableContent

pub type TranslatableResource =
  localization_types.TranslatableResource

pub const translation_error_code_allow_list = localization_types.translation_error_code_allow_list

pub const emitted_translation_mutation_error_codes = localization_types.emitted_translation_mutation_error_codes

pub fn is_localization_query_root(name: String) -> Bool {
  queries.is_localization_query_root(name)
}

pub fn is_localization_mutation_root(name: String) -> Bool {
  mutations.is_localization_mutation_root(name)
}

pub fn handle_localization_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
  case queries.handle_localization_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
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

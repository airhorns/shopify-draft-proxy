//// Public entrypoint for online-store handling.
////
//// Implementation is split across the online_store/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/string
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/online_store/mutations
import shopify_draft_proxy/proxy/online_store/queries
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type OnlineStoreError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_online_store_query_root(name: String, query: String) -> Bool {
  case name {
    "article"
    | "articleAuthors"
    | "articles"
    | "articleTags"
    | "blog"
    | "blogs"
    | "blogsCount"
    | "page"
    | "pages"
    | "pagesCount"
    | "comment"
    | "comments"
    | "theme"
    | "themes"
    | "scriptTag"
    | "scriptTags"
    | "webPixel"
    | "serverPixel"
    | "mobilePlatformApplication"
    | "mobilePlatformApplications" -> True
    "shop" -> string.contains(query, "storefrontAccessTokens")
    _ -> False
  }
}

pub fn is_online_store_mutation_root(name: String) -> Bool {
  case name {
    "articleCreate"
    | "articleUpdate"
    | "articleDelete"
    | "blogCreate"
    | "blogUpdate"
    | "blogDelete"
    | "pageCreate"
    | "pageUpdate"
    | "pageDelete"
    | "commentApprove"
    | "commentSpam"
    | "commentNotSpam"
    | "commentDelete"
    | "themeCreate"
    | "themeUpdate"
    | "themeDelete"
    | "themePublish"
    | "themeFilesCopy"
    | "themeFilesUpsert"
    | "themeFilesDelete"
    | "scriptTagCreate"
    | "scriptTagUpdate"
    | "scriptTagDelete"
    | "webPixelCreate"
    | "webPixelUpdate"
    | "webPixelDelete"
    | "serverPixelCreate"
    | "serverPixelDelete"
    | "eventBridgeServerPixelUpdate"
    | "pubSubServerPixelUpdate"
    | "storefrontAccessTokenCreate"
    | "storefrontAccessTokenDelete"
    | "mobilePlatformApplicationCreate"
    | "mobilePlatformApplicationUpdate"
    | "mobilePlatformApplicationDelete" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OnlineStoreError) {
  case queries.process_with_upstream(store, document, variables) {
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

pub fn local_has_online_store_content_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_online_store_content_id(proxy, variables)
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

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation_with_upstream(
    store,
    identity,
    document,
    variables,
    upstream,
  )
}

//// Public entrypoint for online-store handling.
////
//// Implementation is split across the online_store/* submodules; this file
//// keeps the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Selection, Field, Name, SelectionSet,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcObject, SrcString,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/online_store/mutations
import shopify_draft_proxy/proxy/online_store/queries
import shopify_draft_proxy/proxy/online_store/serializers
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
    | "mobilePlatformApplications"
    | "urlRedirect"
    | "urlRedirects" -> True
    "shop" -> string.contains(query, "storefrontAccessTokens")
    _ -> False
  }
}

pub fn is_online_store_mutation_root(name: String) -> Bool {
  case name {
    "articleCreate"
    | "articleUpdate"
    | "articleDelete"
    | "onlineStoreArticleBulkAddTags"
    | "onlineStoreArticleBulkDelete"
    | "onlineStoreArticleBulkPublish"
    | "onlineStoreArticleBulkRemoveTags"
    | "onlineStoreArticleBulkUnpublish"
    | "blogCreate"
    | "blogUpdate"
    | "blogDelete"
    | "onlineStoreBlogBulkDelete"
    | "pageCreate"
    | "pageUpdate"
    | "pageDelete"
    | "onlineStorePageBulkDelete"
    | "onlineStorePageBulkPublish"
    | "onlineStorePageBulkUnpublish"
    | "commentApprove"
    | "commentSpam"
    | "commentNotSpam"
    | "commentDelete"
    | "onlineStoreCommentBulkApprove"
    | "onlineStoreCommentBulkDelete"
    | "onlineStoreCommentBulkMarkNotSpam"
    | "onlineStoreCommentBulkMarkSpam"
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

pub fn is_online_store_bulk_mutation_fallback_root(name: String) -> Bool {
  case name {
    "onlineStorePageBulkDelete"
    | "onlineStorePageBulkPublish"
    | "onlineStorePageBulkUnpublish"
    | "onlineStoreArticleBulkDelete"
    | "onlineStoreArticleBulkPublish"
    | "onlineStoreArticleBulkUnpublish"
    | "onlineStoreArticleBulkAddTags"
    | "onlineStoreArticleBulkRemoveTags"
    | "onlineStoreBlogBulkDelete"
    | "onlineStoreCommentBulkApprove"
    | "onlineStoreCommentBulkDelete"
    | "onlineStoreCommentBulkMarkSpam"
    | "onlineStoreCommentBulkMarkNotSpam" -> True
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

pub fn serialize_content_node_by_id(
  store: Store,
  id: String,
  kind: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case store.get_effective_online_store_content_by_id(store, id) {
    Some(record) if record.kind == kind ->
      serializers.project_content_record(
        store,
        record,
        synthetic_node_field(typename, selections),
        fragments,
        variables,
      )
    _ -> json.null()
  }
}

pub fn serialize_integration_node_by_id(
  store: Store,
  id: String,
  kind: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_online_store_integration_by_id(store, id) {
    Some(record) if record.kind == kind ->
      project_node_source(
        serializers.integration_projection_source(record),
        typename,
        selections,
        fragments,
      )
    _ -> json.null()
  }
}

pub fn serialize_url_redirect_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_url_redirect_by_id(store, id) {
    Some(record) ->
      serializers.project_url_redirect_record(
        record,
        synthetic_node_field("UrlRedirect", selections),
        fragments,
      )
    None -> json.null()
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

fn synthetic_node_field(
  name: String,
  selections: List(Selection),
) -> Selection {
  Field(
    alias: None,
    name: Name(value: name, loc: None),
    arguments: [],
    directives: [],
    selection_set: Some(SelectionSet(selections: selections, loc: None)),
    loc: None,
  )
}

//// Query dispatch for online-store roots.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/online_store/serializers
import shopify_draft_proxy/proxy/online_store/types as online_store_types
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

@internal
pub fn process_with_upstream(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  process_with_upstream_context(
    store,
    document,
    variables,
    empty_upstream_context(),
  )
}

fn process_with_upstream_context(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  let fragments = get_document_fragments(document)
  let entries =
    list.map(fields, fn(field) {
      #(
        get_field_response_key(field),
        handle_query_field(store, field, fragments, variables, upstream),
      )
    })
  Ok(graphql_helpers.wrap_data(json.object(entries)))
}

/// Online-store cold catalog/search reads use Pattern 1 in LiveHybrid: when
/// no local content state is staged or hydrated, forward the captured read
/// verbatim; once content exists locally, keep the read local so staged
/// synthetic IDs and read-after-write overlays remain visible. Counts are the
/// exception below: local lifecycle reads add a narrow upstream baseline count.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case
        process_with_upstream_context(
          proxy.store,
          document,
          variables,
          upstream_query.UpstreamContext(
            transport: proxy.upstream_transport,
            origin: proxy.config.shopify_admin_origin,
            headers: request.headers,
          ),
        )
      {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string("Failed to handle online-store query"),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "article" ->
      !local_has_online_store_content_id(proxy, variables)
    parse_operation.QueryOperation, "blog" ->
      !local_has_online_store_content_id(proxy, variables)
    parse_operation.QueryOperation, "page" ->
      !local_has_online_store_content_id(proxy, variables)
    parse_operation.QueryOperation, "articles" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "blogs" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "pages" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "articleAuthors" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "articleTags" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "blogsCount" ->
      !has_local_online_store_content_query_state(proxy, variables)
    parse_operation.QueryOperation, "pagesCount" ->
      !has_local_online_store_content_query_state(proxy, variables)
    _, _ -> False
  }
}

@internal
pub fn local_has_online_store_content_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || local_online_store_content_id_known(proxy.store, id)
      _ -> False
    }
  })
}

fn local_online_store_content_id_known(store_in: Store, id: String) -> Bool {
  case store.get_effective_online_store_content_by_id(store_in, id) {
    Some(_) -> True
    None ->
      dict.has_key(store_in.staged_state.deleted_online_store_content_ids, id)
      || dict.has_key(store_in.base_state.deleted_online_store_content_ids, id)
  }
}

fn has_local_online_store_content_query_state(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic || has_any_online_store_content_state(proxy.store)
}

fn has_any_online_store_content_state(store_in: Store) -> Bool {
  dict.size(store_in.base_state.online_store_content) > 0
  || dict.size(store_in.base_state.deleted_online_store_content_ids) > 0
  || dict.size(store_in.staged_state.online_store_content) > 0
  || dict.size(store_in.staged_state.deleted_online_store_content_ids) > 0
}

fn handle_query_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Json {
  case field {
    Field(name: name, ..) -> {
      case name.value {
        "article" ->
          serializers.singular_content(
            store,
            field,
            fragments,
            variables,
            "article",
          )
        "blog" ->
          serializers.singular_content(
            store,
            field,
            fragments,
            variables,
            "blog",
          )
        "page" ->
          serializers.singular_content(
            store,
            field,
            fragments,
            variables,
            "page",
          )
        "comment" ->
          serializers.singular_content(
            store,
            field,
            fragments,
            variables,
            "comment",
          )
        "articles" ->
          serializers.content_connection(
            store,
            field,
            fragments,
            variables,
            "article",
          )
        "blogs" ->
          serializers.content_connection(
            store,
            field,
            fragments,
            variables,
            "blog",
          )
        "pages" ->
          serializers.content_connection(
            store,
            field,
            fragments,
            variables,
            "page",
          )
        "comments" ->
          serializers.content_connection(
            store,
            field,
            fragments,
            variables,
            "comment",
          )
        "articleAuthors" ->
          serializers.article_authors_connection(
            store,
            field,
            fragments,
            variables,
          )
        "articleTags" ->
          json.array(serializers.article_tags(store), json.string)
        "blogsCount" ->
          serializers.content_count_json(
            store,
            "blog",
            upstream,
            "OnlineStoreBlogsCountHydrate",
            online_store_types.online_store_blogs_count_query,
            "blogsCount",
          )
        "pagesCount" ->
          serializers.content_count_json(
            store,
            "page",
            upstream,
            "OnlineStorePagesCountHydrate",
            online_store_types.online_store_pages_count_query,
            "pagesCount",
          )
        "theme" ->
          serializers.singular_integration(
            store,
            field,
            fragments,
            variables,
            "theme",
          )
        "themes" ->
          serializers.integration_connection(
            store,
            field,
            fragments,
            variables,
            "theme",
          )
        "scriptTag" ->
          serializers.singular_integration(
            store,
            field,
            fragments,
            variables,
            "scriptTag",
          )
        "scriptTags" ->
          serializers.integration_connection(
            store,
            field,
            fragments,
            variables,
            "scriptTag",
          )
        "webPixel" ->
          serializers.first_integration(store, field, fragments, "webPixel")
        "serverPixel" ->
          serializers.first_integration(store, field, fragments, "serverPixel")
        "mobilePlatformApplication" ->
          serializers.singular_integration(
            store,
            field,
            fragments,
            variables,
            "mobilePlatformApplication",
          )
        "mobilePlatformApplications" ->
          serializers.integration_connection(
            store,
            field,
            fragments,
            variables,
            "mobilePlatformApplication",
          )
        "shop" -> serializers.project_shop(store, field, fragments, variables)
        _ -> json.null()
      }
    }
    _ -> json.null()
  }
}

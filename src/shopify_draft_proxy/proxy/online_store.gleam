import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, SerializeConnectionConfig,
  SrcBool, SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type OnlineStoreContentRecord,
  type OnlineStoreIntegrationRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  OnlineStoreContentRecord, OnlineStoreIntegrationRecord,
}

const synthetic_shop_id: String = "gid://shopify/Shop/92891250994"

const online_store_blogs_count_query: String = "query OnlineStoreBlogsCountHydrate { blogsCount { count precision } }"

const online_store_pages_count_query: String = "query OnlineStorePagesCountHydrate { pagesCount { count precision } }"

const online_store_comment_hydrate_query: String = "query OnlineStoreCommentHydrate($id: ID!) { comment(id: $id) { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } }"

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
  process_with_upstream(store, document, variables, empty_upstream_context())
}

fn process_with_upstream(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(Json, OnlineStoreError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
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
        process_with_upstream(
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

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  process_mutation_with_upstream(store, identity, document, variables, upstream)
}

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let initial =
        MutationOutcome(
          data: json.object([]),
          store: store,
          identity: identity,
          staged_resource_ids: [],
          log_drafts: [],
        )
      let #(entries, outcome) =
        list.fold(fields, #([], initial), fn(acc, field) {
          let #(pairs, current) = acc
          let #(key, payload, next) =
            handle_mutation_field(
              current,
              field,
              fragments,
              variables,
              upstream,
            )
          let merged =
            MutationOutcome(
              ..next,
              staged_resource_ids: list.append(
                current.staged_resource_ids,
                next.staged_resource_ids,
              ),
              log_drafts: list.append(current.log_drafts, next.log_drafts),
            )
          #(list.append(pairs, [#(key, payload)]), merged)
        })
      MutationOutcome(
        ..outcome,
        data: graphql_helpers.wrap_data(json.object(entries)),
      )
    }
  }
}

fn handle_mutation_field(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, ..) -> {
      let root = name.value
      case root {
        "blogCreate" ->
          create_content(
            outcome,
            field,
            fragments,
            variables,
            root,
            "blog",
            "blog",
          )
        "pageCreate" ->
          create_content(
            outcome,
            field,
            fragments,
            variables,
            root,
            "page",
            "page",
          )
        "articleCreate" -> create_article(outcome, field, fragments, variables)
        "blogUpdate" ->
          update_content(
            outcome,
            field,
            fragments,
            variables,
            root,
            "blog",
            "blog",
          )
        "pageUpdate" ->
          update_content(
            outcome,
            field,
            fragments,
            variables,
            root,
            "page",
            "page",
          )
        "articleUpdate" ->
          update_content(
            outcome,
            field,
            fragments,
            variables,
            root,
            "article",
            "article",
          )
        "blogDelete" ->
          delete_content(
            outcome,
            field,
            variables,
            root,
            "blog",
            "deletedBlogId",
          )
        "pageDelete" ->
          delete_content(
            outcome,
            field,
            variables,
            root,
            "page",
            "deletedPageId",
          )
        "articleDelete" ->
          delete_content(
            outcome,
            field,
            variables,
            root,
            "article",
            "deletedArticleId",
          )
        "commentApprove" | "commentSpam" | "commentNotSpam" ->
          moderate_comment(outcome, field, variables, root, upstream)
        "commentDelete" -> delete_comment(outcome, field, variables, upstream)
        "themeCreate" -> create_theme(outcome, field, fragments, variables)
        "themeUpdate" ->
          update_theme(outcome, field, fragments, variables, "themeUpdate")
        "themePublish" ->
          update_theme(outcome, field, fragments, variables, "themePublish")
        "themeDelete" ->
          delete_integration(
            outcome,
            field,
            variables,
            root,
            "theme",
            "deletedThemeId",
          )
        "themeFilesUpsert" -> theme_files_upsert(outcome, field, variables)
        "themeFilesCopy" -> theme_files_copy(outcome, field, variables)
        "themeFilesDelete" -> theme_files_delete(outcome, field, variables)
        "scriptTagCreate" ->
          create_script_tag(outcome, field, fragments, variables)
        "scriptTagUpdate" ->
          update_script_tag(outcome, field, fragments, variables)
        "scriptTagDelete" ->
          delete_integration(
            outcome,
            field,
            variables,
            root,
            "scriptTag",
            "deletedScriptTagId",
          )
        "webPixelCreate" ->
          create_pixel(
            outcome,
            field,
            fragments,
            variables,
            "webPixelCreate",
            "webPixel",
          )
        "webPixelUpdate" ->
          update_pixel(
            outcome,
            field,
            fragments,
            variables,
            "webPixelUpdate",
            "webPixel",
          )
        "webPixelDelete" ->
          delete_integration(
            outcome,
            field,
            variables,
            root,
            "webPixel",
            "deletedWebPixelId",
          )
        "serverPixelCreate" ->
          create_pixel(
            outcome,
            field,
            fragments,
            variables,
            "serverPixelCreate",
            "serverPixel",
          )
        "serverPixelDelete" ->
          delete_integration(
            outcome,
            field,
            variables,
            root,
            "serverPixel",
            "deletedServerPixelId",
          )
        "eventBridgeServerPixelUpdate" ->
          update_server_pixel_endpoint(
            outcome,
            field,
            fragments,
            variables,
            root,
            "arn",
          )
        "pubSubServerPixelUpdate" ->
          update_server_pixel_endpoint(
            outcome,
            field,
            fragments,
            variables,
            root,
            "pubsub",
          )
        "storefrontAccessTokenCreate" ->
          create_storefront_token(outcome, field, fragments, variables)
        "storefrontAccessTokenDelete" ->
          delete_storefront_token(outcome, field, variables)
        "mobilePlatformApplicationCreate" ->
          create_mobile_app(outcome, field, fragments, variables)
        "mobilePlatformApplicationUpdate" ->
          update_mobile_app(outcome, field, fragments, variables)
        "mobilePlatformApplicationDelete" ->
          delete_integration(
            outcome,
            field,
            variables,
            root,
            "mobilePlatformApplication",
            "deletedMobilePlatformApplicationId",
          )
        _ -> #(key, json.null(), outcome)
      }
    }
    _ -> #(key, json.null(), outcome)
  }
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
          singular_content(store, field, fragments, variables, "article")
        "blog" -> singular_content(store, field, fragments, variables, "blog")
        "page" -> singular_content(store, field, fragments, variables, "page")
        "comment" ->
          singular_content(store, field, fragments, variables, "comment")
        "articles" ->
          content_connection(store, field, fragments, variables, "article")
        "blogs" ->
          content_connection(store, field, fragments, variables, "blog")
        "pages" ->
          content_connection(store, field, fragments, variables, "page")
        "comments" ->
          content_connection(store, field, fragments, variables, "comment")
        "articleAuthors" ->
          article_authors_connection(store, field, fragments, variables)
        "articleTags" -> json.array(article_tags(store), json.string)
        "blogsCount" ->
          content_count_json(
            store,
            "blog",
            upstream,
            "OnlineStoreBlogsCountHydrate",
            online_store_blogs_count_query,
            "blogsCount",
          )
        "pagesCount" ->
          content_count_json(
            store,
            "page",
            upstream,
            "OnlineStorePagesCountHydrate",
            online_store_pages_count_query,
            "pagesCount",
          )
        "theme" ->
          singular_integration(store, field, fragments, variables, "theme")
        "themes" ->
          integration_connection(store, field, fragments, variables, "theme")
        "scriptTag" ->
          singular_integration(store, field, fragments, variables, "scriptTag")
        "scriptTags" ->
          integration_connection(
            store,
            field,
            fragments,
            variables,
            "scriptTag",
          )
        "webPixel" -> first_integration(store, field, fragments, "webPixel")
        "serverPixel" ->
          first_integration(store, field, fragments, "serverPixel")
        "mobilePlatformApplication" ->
          singular_integration(
            store,
            field,
            fragments,
            variables,
            "mobilePlatformApplication",
          )
        "mobilePlatformApplications" ->
          integration_connection(
            store,
            field,
            fragments,
            variables,
            "mobilePlatformApplication",
          )
        "shop" -> project_shop(store, field, fragments, variables)
        _ -> json.null()
      }
    }
    _ -> json.null()
  }
}

fn create_content(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
  payload_key: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      payload_key,
    )
    |> option.unwrap(dict.new())
  case required_title_error(payload_key, input) {
    Some(error) ->
      content_validation_error_payload(
        outcome,
        field,
        fragments,
        root,
        payload_key,
        error,
      )
    None ->
      case resolve_content_handle(outcome.store, kind, input, None, None) {
        Error(error) ->
          content_validation_error_payload(
            outcome,
            field,
            fragments,
            root,
            payload_key,
            error,
          )
        Ok(handle) -> {
          let #(record, identity) =
            make_content(outcome.identity, kind, input, None, None, handle)
          let #(_, store) =
            store.upsert_staged_online_store_content(outcome.store, record)
          let payload =
            mutation_payload(
              field,
              fragments,
              payload_key,
              project_content_payload(
                store,
                record,
                field,
                fragments,
                variables,
                payload_key,
              ),
              [],
            )
          #(
            key,
            payload,
            mutation_outcome(outcome, store, identity, root, [record.id]),
          )
        }
      }
  }
}

fn create_article(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let article_input =
    graphql_helpers.read_arg_object(args, "article")
    |> option.unwrap(dict.new())
  case article_create_validation_error(args, article_input) {
    Some(error) ->
      content_validation_error_payload(
        outcome,
        field,
        fragments,
        "articleCreate",
        "article",
        error,
      )
    None -> {
      let blog_from_arg =
        graphql_helpers.read_arg_object(args, "blog")
        |> option.unwrap(dict.new())
      case prepare_article_parent_blog(outcome, blog_from_arg, article_input) {
        Error(error) ->
          content_validation_error_payload(
            outcome,
            field,
            fragments,
            "articleCreate",
            "article",
            error,
          )
        Ok(prepared) -> {
          let ArticleParent(
            blog_id: blog_id,
            blog_record: blog_record,
            identity: identity,
            staged_blog_ids: staged_blog_ids,
          ) = prepared
          case
            resolve_content_handle(
              outcome.store,
              "article",
              article_input,
              Some(blog_id),
              None,
            )
          {
            Error(error) ->
              content_validation_error_payload(
                outcome,
                field,
                fragments,
                "articleCreate",
                "article",
                error,
              )
            Ok(handle) -> {
              let store = case blog_record {
                Some(blog) -> {
                  let #(_, next_store) =
                    store.upsert_staged_online_store_content(
                      outcome.store,
                      blog,
                    )
                  next_store
                }
                None -> outcome.store
              }
              let #(record, identity) =
                make_content(
                  identity,
                  "article",
                  article_input,
                  Some(blog_id),
                  None,
                  handle,
                )
              let #(_, store) =
                store.upsert_staged_online_store_content(store, record)
              let payload =
                mutation_payload(
                  field,
                  fragments,
                  "article",
                  project_content_payload(
                    store,
                    record,
                    field,
                    fragments,
                    variables,
                    "article",
                  ),
                  [],
                )
              #(
                key,
                payload,
                mutation_outcome(
                  outcome,
                  store,
                  identity,
                  "articleCreate",
                  list.append(staged_blog_ids, [record.id]),
                ),
              )
            }
          }
        }
      }
    }
  }
}

type ArticleParent {
  ArticleParent(
    blog_id: String,
    blog_record: Option(OnlineStoreContentRecord),
    identity: SyntheticIdentityRegistry,
    staged_blog_ids: List(String),
  )
}

fn prepare_article_parent_blog(
  outcome: MutationOutcome,
  blog_input: Dict(String, root_field.ResolvedValue),
  article_input: Dict(String, root_field.ResolvedValue),
) -> Result(ArticleParent, graphql_helpers.SourceValue) {
  case input_string(article_input, "blogId") {
    Some(id) ->
      Ok(
        ArticleParent(
          blog_id: id,
          blog_record: None,
          identity: outcome.identity,
          staged_blog_ids: [],
        ),
      )
    None ->
      case
        resolve_content_handle(outcome.store, "blog", blog_input, None, None)
      {
        Error(error) -> Error(error)
        Ok(handle) -> {
          let #(blog, identity) =
            make_content(
              outcome.identity,
              "blog",
              blog_input,
              None,
              None,
              handle,
            )
          Ok(
            ArticleParent(
              blog_id: blog.id,
              blog_record: Some(blog),
              identity: identity,
              staged_blog_ids: [blog.id],
            ),
          )
        }
      }
  }
}

fn article_create_validation_error(
  args: Dict(String, root_field.ResolvedValue),
  article_input: Dict(String, root_field.ResolvedValue),
) -> Option(graphql_helpers.SourceValue) {
  let has_blog_id = option.is_some(input_string(article_input, "blogId"))
  let has_inline_blog = case graphql_helpers.read_arg_object(args, "blog") {
    Some(_) -> True
    None -> False
  }
  case required_title_error("article", article_input) {
    Some(error) -> Some(error)
    None ->
      case has_blog_id, has_inline_blog {
        True, True ->
          Some(article_user_error(
            "Can't create a blog from input if a blog ID is supplied.",
            "AMBIGUOUS_BLOG",
          ))
        False, False ->
          Some(article_user_error(
            "Must reference or create a blog when creating an article.",
            "BLOG_REFERENCE_REQUIRED",
          ))
        _, _ -> article_author_validation_error(article_input)
      }
  }
}

fn article_author_validation_error(
  article_input: Dict(String, root_field.ResolvedValue),
) -> Option(graphql_helpers.SourceValue) {
  case dict.get(article_input, "author") {
    Ok(root_field.ObjectVal(author)) -> {
      let has_name = option.is_some(input_non_blank_string(author, "name"))
      let has_user_id = option.is_some(input_non_blank_string(author, "userId"))
      case has_name, has_user_id {
        True, True ->
          Some(article_user_error(
            "Can't create an article author if both author name and user ID are supplied.",
            "AMBIGUOUS_AUTHOR",
          ))
        False, False ->
          Some(article_user_error(
            "Can't create an article if both author name and user ID are blank.",
            "AUTHOR_FIELD_REQUIRED",
          ))
        _, _ -> None
      }
    }
    _ ->
      Some(article_user_error(
        "Can't create an article if both author name and user ID are blank.",
        "AUTHOR_FIELD_REQUIRED",
      ))
  }
}

fn update_content(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
  payload_key: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = input_string(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, payload_key)
    |> option.unwrap(dict.new())
  case id {
    Some(id) ->
      case store.get_effective_online_store_content_by_id(outcome.store, id) {
        Some(existing) -> {
          case
            resolve_content_handle(
              outcome.store,
              kind,
              input,
              existing.parent_id,
              Some(existing),
            )
          {
            Error(error) ->
              content_validation_error_payload(
                outcome,
                field,
                fragments,
                root,
                payload_key,
                error,
              )
            Ok(handle) -> {
              let #(record, identity) =
                make_content(
                  outcome.identity,
                  kind,
                  input,
                  existing.parent_id,
                  Some(existing),
                  handle,
                )
              let #(_, store) =
                store.upsert_staged_online_store_content(outcome.store, record)
              let payload =
                mutation_payload(
                  field,
                  fragments,
                  payload_key,
                  project_content_payload(
                    store,
                    record,
                    field,
                    fragments,
                    variables,
                    payload_key,
                  ),
                  [],
                )
              #(
                key,
                payload,
                mutation_outcome(outcome, store, identity, root, [id]),
              )
            }
          }
        }
        None ->
          not_found_payload(
            outcome,
            field,
            root,
            payload_key,
            "Content does not exist",
          )
      }
    None ->
      not_found_payload(
        outcome,
        field,
        root,
        payload_key,
        "Content does not exist",
      )
  }
}

fn delete_content(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  _kind: String,
  deleted_key: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  let #(deleted, errors, store) = case id {
    Some(id) ->
      case store.get_effective_online_store_content_by_id(outcome.store, id) {
        Some(_) -> #(
          SrcString(id),
          [],
          store.delete_staged_online_store_content(outcome.store, id),
        )
        None -> #(
          SrcNull,
          [user_error(["id"], "Content does not exist")],
          outcome.store,
        )
      }
    None -> #(
      SrcNull,
      [user_error(["id"], "Content does not exist")],
      outcome.store,
    )
  }
  let payload =
    project_payload_source(
      field,
      src_object([
        #(deleted_key, deleted),
        #("userErrors", user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    mutation_outcome(outcome, store, outcome.identity, root, case errors {
      [] -> option_list(id)
      _ -> []
    }),
  )
}

fn moderate_comment(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  upstream: UpstreamContext,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  let target_status = comment_target_status(root)
  let #(comment, errors, store, identity) = case id {
    Some(id) ->
      case get_effective_or_hydrated_comment(outcome.store, upstream, id) {
        #(Some(existing), hydrated_store) -> {
          case comment_status(existing) == "REMOVED" {
            True -> #(
              SrcNull,
              [comment_removed_user_error()],
              hydrated_store,
              outcome.identity,
            )
            False -> {
              let #(record, identity) =
                comment_record_with_status(
                  existing,
                  target_status,
                  outcome.identity,
                )
              let #(_, next_store) =
                store.upsert_staged_online_store_content(hydrated_store, record)
              #(
                content_payload_source(next_store, record),
                [],
                next_store,
                identity,
              )
            }
          }
        }
        #(None, hydrated_store) -> #(
          SrcNull,
          [comment_not_found_user_error()],
          hydrated_store,
          outcome.identity,
        )
      }
    None -> #(
      SrcNull,
      [comment_not_found_user_error()],
      outcome.store,
      outcome.identity,
    )
  }
  let payload =
    project_payload_source(
      field,
      src_object([
        #("comment", comment),
        #("userErrors", user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(key, payload, mutation_outcome(outcome, store, identity, root, []))
}

fn delete_comment(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  let #(deleted, errors, store) = case id {
    Some(id) ->
      case get_effective_or_hydrated_comment(outcome.store, upstream, id) {
        #(Some(existing), hydrated_store) -> {
          case comment_status(existing) == "REMOVED" {
            True -> #(SrcString(id), [], hydrated_store)
            False -> {
              let #(record, _) =
                comment_record_with_status(
                  existing,
                  "REMOVED",
                  outcome.identity,
                )
              let #(_, next_store) =
                store.upsert_staged_online_store_content(hydrated_store, record)
              #(SrcString(id), [], next_store)
            }
          }
        }
        #(None, hydrated_store) -> #(
          SrcNull,
          [comment_not_found_user_error()],
          hydrated_store,
        )
      }
    None -> #(SrcNull, [comment_not_found_user_error()], outcome.store)
  }
  let payload =
    project_payload_source(
      field,
      src_object([
        #("deletedCommentId", deleted),
        #("userErrors", user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    mutation_outcome(
      outcome,
      store,
      outcome.identity,
      "commentDelete",
      case errors {
        [] -> option_list(id)
        _ -> []
      },
    ),
  )
}

fn get_effective_or_hydrated_comment(
  store_in: Store,
  upstream: UpstreamContext,
  id: String,
) -> #(Option(OnlineStoreContentRecord), Store) {
  case store.get_effective_online_store_content_by_id(store_in, id) {
    Some(existing) if existing.kind == "comment" -> #(Some(existing), store_in)
    _ ->
      case hydrate_comment(store_in, upstream, id) {
        Some(next_store) -> #(
          store.get_effective_online_store_content_by_id(next_store, id),
          next_store,
        )
        None -> #(None, store_in)
      }
  }
}

fn hydrate_comment(
  store_in: Store,
  upstream: UpstreamContext,
  id: String,
) -> Option(Store) {
  let variables = json.object([#("id", json.string(id))])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "OnlineStoreCommentHydrate",
      online_store_comment_hydrate_query,
      variables,
    )
  {
    Ok(value) ->
      json_get(value, "data")
      |> option.then(json_get(_, "comment"))
      |> option.then(comment_record_from_commit)
      |> option.map(fn(record) {
        store.upsert_base_online_store_content(store_in, [record])
      })
    Error(_) -> None
  }
}

fn comment_record_from_commit(
  value: commit.JsonValue,
) -> Option(OnlineStoreContentRecord) {
  case json_get_string(value, "id") {
    Some(id) ->
      Some(OnlineStoreContentRecord(
        id: id,
        kind: "comment",
        cursor: None,
        parent_id: comment_parent_article_id(value),
        created_at: json_get_string(value, "createdAt"),
        updated_at: json_get_string(value, "updatedAt"),
        data: captured_json_from_commit(value),
      ))
    None -> None
  }
}

fn comment_parent_article_id(value: commit.JsonValue) -> Option(String) {
  json_get(value, "article")
  |> option.then(json_get_string(_, "id"))
}

fn comment_target_status(root: String) -> String {
  case root {
    "commentApprove" -> "PUBLISHED"
    "commentSpam" -> "SPAM"
    "commentNotSpam" -> "UNAPPROVED"
    _ -> "UNAPPROVED"
  }
}

fn comment_status(record: OnlineStoreContentRecord) -> String {
  source_string_field(captured_to_source(record.data), "status", "")
}

fn comment_record_with_status(
  existing: OnlineStoreContentRecord,
  status: String,
  identity: SyntheticIdentityRegistry,
) -> #(OnlineStoreContentRecord, SyntheticIdentityRegistry) {
  let #(data, identity) =
    comment_data_with_status(existing.data, status, identity)
  #(OnlineStoreContentRecord(..existing, data: data), identity)
}

fn comment_data_with_status(
  data: CapturedJsonValue,
  status: String,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let source = captured_to_source(data)
  let data = captured_object_insert(data, "status", CapturedString(status))
  case status {
    "PUBLISHED" -> {
      let data = captured_object_insert(data, "isPublished", CapturedBool(True))
      case source_optional_string_field(source, "publishedAt") {
        Some(_) -> #(data, identity)
        None -> {
          let #(timestamp, identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          #(
            captured_object_insert(
              data,
              "publishedAt",
              CapturedString(timestamp),
            ),
            identity,
          )
        }
      }
    }
    "REMOVED" | "SPAM" | "UNAPPROVED" -> #(
      captured_object_insert(data, "isPublished", CapturedBool(False)),
      identity,
    )
    _ -> #(data, identity)
  }
}

fn comment_not_found_user_error() -> graphql_helpers.SourceValue {
  user_error(["id"], "Comment does not exist")
}

fn comment_removed_user_error() -> graphql_helpers.SourceValue {
  user_error_with_code(["id"], "Comment has been removed", "INVALID")
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
  case value {
    commit.JsonNull -> CapturedNull
    commit.JsonBool(value) -> CapturedBool(value)
    commit.JsonInt(value) -> CapturedInt(value)
    commit.JsonFloat(value) -> CapturedFloat(value)
    commit.JsonString(value) -> CapturedString(value)
    commit.JsonArray(items) ->
      CapturedArray(list.map(items, captured_json_from_commit))
    commit.JsonObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_json_from_commit(pair.1))
        }),
      )
  }
}

fn create_theme(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let source = input_string(args, "source")
  let errors = case source {
    Some(_) -> []
    None -> [user_error(["source"], "Source can't be blank")]
  }
  let #(record, identity, store, staged_ids) = case errors {
    [] -> {
      let #(record, identity) =
        make_integration(outcome.identity, "theme", [
          #("__typename", SrcString("OnlineStoreTheme")),
          #(
            "name",
            option_source(input_string(args, "name"), "Draft proxy theme"),
          ),
          #("role", option_source(input_string(args, "role"), "UNPUBLISHED")),
          #("processing", SrcBool(False)),
          #("processingFailed", SrcBool(False)),
          #("files", SrcList([])),
        ])
      let #(_, store) =
        store.upsert_staged_online_store_integration(outcome.store, record)
      #(Some(record), identity, store, [record.id])
    }
    _ -> #(None, outcome.identity, outcome.store, [])
  }
  integration_payload_result(
    outcome,
    field,
    fragments,
    variables,
    "themeCreate",
    "theme",
    record,
    errors,
    store,
    identity,
    staged_ids,
  )
}

fn update_theme(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = input_string(args, "id")
  case id {
    Some(id) ->
      case
        store.get_effective_online_store_integration_by_id(outcome.store, id)
      {
        Some(existing) -> {
          let current_role =
            source_string_field(captured_to_source(existing.data), "role", "")
          let publish_blocked =
            root == "themePublish"
            && is_publish_blocked_theme_role(current_role)
          let input =
            graphql_helpers.read_arg_object(args, "input")
            |> option.unwrap(dict.new())
          let role = case root {
            "themePublish" -> Some("MAIN")
            _ -> input_string(input, "role")
          }
          let name = input_string(input, "name")
          case publish_blocked {
            True ->
              integration_payload_result(
                outcome,
                field,
                fragments,
                variables,
                root,
                "theme",
                None,
                [
                  user_error(
                    ["id"],
                    "Theme cannot be published from role " <> current_role,
                  ),
                ],
                outcome.store,
                outcome.identity,
                [],
              )
            False -> {
              let data =
                existing.data
                |> maybe_insert_string("name", name)
                |> maybe_insert_string("role", role)
              let record = OnlineStoreIntegrationRecord(..existing, data: data)
              let target_store = case root {
                "themePublish" -> demote_previous_main_themes(outcome.store, id)
                _ -> outcome.store
              }
              let #(_, store) =
                store.upsert_staged_online_store_integration(
                  target_store,
                  record,
                )
              integration_payload_result(
                outcome,
                field,
                fragments,
                variables,
                root,
                "theme",
                Some(record),
                [],
                store,
                outcome.identity,
                [id],
              )
            }
          }
        }
        None ->
          integration_payload_result(
            outcome,
            field,
            fragments,
            variables,
            root,
            "theme",
            None,
            [user_error(["id"], "Theme does not exist")],
            outcome.store,
            outcome.identity,
            [],
          )
      }
    None ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        "theme",
        None,
        [user_error(["id"], "Theme does not exist")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn is_publish_blocked_theme_role(role: String) -> Bool {
  case role {
    "DEMO" | "LOCKED" | "ARCHIVED" -> True
    _ -> False
  }
}

fn demote_previous_main_themes(store_in: Store, published_id: String) -> Store {
  store.list_effective_online_store_integrations(store_in, "theme")
  |> list.fold(store_in, fn(acc, record) {
    let role = source_string_field(captured_to_source(record.data), "role", "")
    case record.id != published_id && role == "MAIN" {
      True -> {
        let demoted =
          OnlineStoreIntegrationRecord(
            ..record,
            data: maybe_insert_string(record.data, "role", Some("UNPUBLISHED")),
          )
        let #(_, next) =
          store.upsert_staged_online_store_integration(acc, demoted)
        next
      }
      False -> acc
    }
  })
}

fn theme_files_upsert(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  theme_files_change(outcome, field, variables, "themeFilesUpsert")
}

fn theme_files_copy(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  theme_files_change(outcome, field, variables, "themeFilesCopy")
}

fn theme_files_delete(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  theme_files_change(outcome, field, variables, "themeFilesDelete")
}

fn theme_files_change(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let theme_id = case input_string(args, "themeId") {
    Some(id) -> Some(id)
    None -> input_string(args, "id")
  }
  let existing =
    option_then(theme_id, fn(id) {
      store.get_effective_online_store_integration_by_id(outcome.store, id)
    })
  let errors = case existing {
    Some(_) -> []
    None -> [user_error(["themeId"], "Theme does not exist")]
  }
  let result = case existing, errors {
    Some(theme), [] ->
      theme_files_change_result(outcome.store, theme, args, root)
    _, _ ->
      ThemeFilesChangeResult(files: [], errors: errors, store: outcome.store)
  }
  let payload = case root {
    "themeFilesUpsert" ->
      src_object([
        #(
          "job",
          src_object([
            #("id", SrcString("gid://shopify/Job/online-store-theme-files")),
            #("done", SrcBool(True)),
          ]),
        ),
        #("upsertedThemeFiles", SrcList(result.files)),
        #("userErrors", user_errors_source(result.errors)),
      ])
    "themeFilesCopy" ->
      src_object([
        #(
          "job",
          src_object([
            #("id", SrcString("gid://shopify/Job/online-store-theme-files")),
            #("done", SrcBool(True)),
          ]),
        ),
        #("copiedThemeFiles", SrcList(result.files)),
        #("userErrors", user_errors_source(result.errors)),
      ])
    _ ->
      src_object([
        #(
          "job",
          src_object([
            #("id", SrcString("gid://shopify/Job/online-store-theme-files")),
            #("done", SrcBool(True)),
          ]),
        ),
        #("deletedThemeFiles", SrcList(result.files)),
        #("userErrors", user_errors_source(result.errors)),
      ])
  }
  #(
    key,
    project_payload_source(field, payload, dict.new()),
    mutation_outcome(outcome, result.store, outcome.identity, root, []),
  )
}

type ThemeFilesChangeResult {
  ThemeFilesChangeResult(
    files: List(graphql_helpers.SourceValue),
    errors: List(graphql_helpers.SourceValue),
    store: Store,
  )
}

fn theme_files_change_result(
  store_in: Store,
  theme: OnlineStoreIntegrationRecord,
  args: Dict(String, root_field.ResolvedValue),
  root: String,
) -> ThemeFilesChangeResult {
  case root {
    "themeFilesUpsert" -> theme_files_upsert_result(store_in, theme, args)
    "themeFilesCopy" -> theme_files_copy_result(store_in, theme, args)
    _ -> theme_files_delete_result(store_in, theme, args)
  }
}

fn theme_files_upsert_result(
  store_in: Store,
  theme: OnlineStoreIntegrationRecord,
  args: Dict(String, root_field.ResolvedValue),
) -> ThemeFilesChangeResult {
  let inputs = input_list(args, "files")
  let errors = theme_file_input_filename_errors(inputs, "filename")
  case errors {
    [] -> {
      let files = make_theme_files(inputs)
      let updated_files =
        list.fold(files, theme_record_files(theme), replace_theme_file)
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          theme_with_files(theme, updated_files),
        )
      ThemeFilesChangeResult(files: files, errors: [], store: store)
    }
    _ -> ThemeFilesChangeResult(files: [], errors: errors, store: store_in)
  }
}

fn theme_files_copy_result(
  store_in: Store,
  theme: OnlineStoreIntegrationRecord,
  args: Dict(String, root_field.ResolvedValue),
) -> ThemeFilesChangeResult {
  let current_files = theme_record_files(theme)
  let inputs = input_list(args, "files")
  let errors =
    theme_file_input_filename_errors(inputs, "dstFilename")
    |> list.append(theme_file_copy_source_errors(inputs, current_files))
  case errors {
    [] -> {
      let files = make_copied_theme_files(inputs, current_files)
      let updated_files = list.fold(files, current_files, replace_theme_file)
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          theme_with_files(theme, updated_files),
        )
      ThemeFilesChangeResult(files: files, errors: [], store: store)
    }
    _ -> ThemeFilesChangeResult(files: [], errors: errors, store: store_in)
  }
}

fn theme_files_delete_result(
  store_in: Store,
  theme: OnlineStoreIntegrationRecord,
  args: Dict(String, root_field.ResolvedValue),
) -> ThemeFilesChangeResult {
  let filenames = input_string_values(input_list(args, "files"))
  let errors = required_theme_file_delete_errors(filenames)
  case errors {
    [] -> {
      let current_files = theme_record_files(theme)
      let deleted_files =
        list.filter(current_files, fn(file) {
          list.contains(filenames, theme_file_filename(file))
        })
      let updated_files =
        list.filter(current_files, fn(file) {
          !list.contains(filenames, theme_file_filename(file))
        })
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          theme_with_files(theme, updated_files),
        )
      ThemeFilesChangeResult(files: deleted_files, errors: [], store: store)
    }
    _ -> ThemeFilesChangeResult(files: [], errors: errors, store: store_in)
  }
}

fn create_script_tag(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let #(record, identity) =
    make_integration(outcome.identity, "scriptTag", [
      #("__typename", SrcString("ScriptTag")),
      #("src", option_source(input_string(input, "src"), "")),
      #(
        "displayScope",
        option_source(input_string(input, "displayScope"), "ONLINE_STORE"),
      ),
      #("cache", bool_source(input_bool(input, "cache"), False)),
    ])
  let #(_, store) =
    store.upsert_staged_online_store_integration(outcome.store, record)
  integration_payload_result(
    outcome,
    field,
    fragments,
    variables,
    "scriptTagCreate",
    "scriptTag",
    Some(record),
    [],
    store,
    identity,
    [record.id],
  )
}

fn update_script_tag(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = input_string(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case id {
    Some(id) ->
      case
        store.get_effective_online_store_integration_by_id(outcome.store, id)
      {
        Some(existing) -> {
          let data =
            existing.data
            |> maybe_insert_string("src", input_string(input, "src"))
            |> maybe_insert_string(
              "displayScope",
              input_string(input, "displayScope"),
            )
            |> maybe_insert_bool("cache", input_bool(input, "cache"))
          let record = OnlineStoreIntegrationRecord(..existing, data: data)
          let #(_, store) =
            store.upsert_staged_online_store_integration(outcome.store, record)
          integration_payload_result(
            outcome,
            field,
            fragments,
            variables,
            "scriptTagUpdate",
            "scriptTag",
            Some(record),
            [],
            store,
            outcome.identity,
            [record.id],
          )
        }
        None ->
          integration_payload_result(
            outcome,
            field,
            fragments,
            variables,
            "scriptTagUpdate",
            "scriptTag",
            None,
            [user_error(["id"], "Script tag does not exist")],
            outcome.store,
            outcome.identity,
            [],
          )
      }
    None ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "scriptTagUpdate",
        "scriptTag",
        None,
        [user_error(["id"], "Script tag does not exist")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn create_pixel(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let settings = case kind {
    "webPixel" ->
      value_source_from_dict(
        graphql_helpers.read_arg_object(args, "webPixel")
          |> option.unwrap(dict.new()),
        "settings",
      )
    _ -> SrcNull
  }
  let duplicate_web_pixel =
    kind == "webPixel"
    && list.any(
      store.list_effective_online_store_integrations(outcome.store, "webPixel"),
      same_current_app_web_pixel,
    )
  case duplicate_web_pixel {
    True ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        None,
        [web_pixel_taken_error()],
        outcome.store,
        outcome.identity,
        [],
      )
    False ->
      create_pixel_record(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        settings,
      )
  }
}

fn create_pixel_record(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
  settings: graphql_helpers.SourceValue,
) -> #(String, Json, MutationOutcome) {
  let type_name = case kind {
    "webPixel" -> "WebPixel"
    _ -> "ServerPixel"
  }
  let entries = case kind {
    "webPixel" -> [
      #("__typename", SrcString(type_name)),
      #("settings", settings),
      #("status", web_pixel_status_source(settings)),
    ]
    _ -> [
      #("__typename", SrcString(type_name)),
      #("settings", settings),
      #("status", SrcString("CONNECTED")),
      #("webhookEndpointAddress", SrcNull),
    ]
  }
  let #(record, identity) = make_integration(outcome.identity, kind, entries)
  let #(_, store) =
    store.upsert_staged_online_store_integration(outcome.store, record)
  integration_payload_result(
    outcome,
    field,
    fragments,
    variables,
    root,
    kind,
    Some(record),
    [],
    store,
    identity,
    [record.id],
  )
}

fn update_pixel(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = input_string(args, "id")
  let existing = case id {
    Some(id) ->
      store.get_effective_online_store_integration_by_id(outcome.store, id)
    None ->
      first_option(store.list_effective_online_store_integrations(
        outcome.store,
        kind,
      ))
  }
  case existing {
    Some(record) -> {
      let input =
        graphql_helpers.read_arg_object(args, kind)
        |> option.unwrap(dict.new())
      let prior = captured_to_source(record.data)
      let settings =
        value_or_default(
          input,
          "settings",
          source_field(prior, "settings", SrcNull),
        )
      let record = case kind {
        "webPixel" ->
          OnlineStoreIntegrationRecord(
            ..record,
            data: base_source(prior, [
                #("settings", settings),
                #("status", web_pixel_status_source(settings)),
              ])
              |> source_to_captured,
          )
        _ -> record
      }
      let #(_, store) =
        store.upsert_staged_online_store_integration(outcome.store, record)
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        Some(record),
        [],
        store,
        outcome.identity,
        [record.id],
      )
    }
    None ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        None,
        [web_pixel_user_error(["id"], "Pixel does not exist", None)],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn update_server_pixel_endpoint(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  mode: String,
) -> #(String, Json, MutationOutcome) {
  let existing =
    first_option(store.list_effective_online_store_integrations(
      outcome.store,
      "serverPixel",
    ))
  let args = graphql_helpers.field_args(field, variables)
  let address = case mode {
    "arn" -> input_string(args, "arn")
    _ ->
      case
        input_string(args, "pubSubProject"),
        input_string(args, "pubSubTopic")
      {
        Some(p), Some(t) -> Some(p <> "/" <> t)
        _, _ -> None
      }
  }
  case existing {
    Some(existing) -> {
      let record =
        OnlineStoreIntegrationRecord(
          ..existing,
          data: maybe_insert_string(
            existing.data,
            "webhookEndpointAddress",
            address,
          ),
        )
      let #(_, store) =
        store.upsert_staged_online_store_integration(outcome.store, record)
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        "serverPixel",
        Some(record),
        [],
        store,
        outcome.identity,
        [record.id],
      )
    }
    None ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        "serverPixel",
        None,
        [user_error([], "Server pixel does not exist")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn create_storefront_token(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let #(record, identity) =
    make_integration(outcome.identity, "storefrontAccessToken", [
      #("__typename", SrcString("StorefrontAccessToken")),
      #(
        "title",
        option_source(input_string(input, "title"), "Headless preview"),
      ),
      #("accessToken", SrcString("shpat_redacted")),
      #("accessScopes", SrcList([])),
    ])
  let #(_, store) =
    store.upsert_staged_online_store_integration(outcome.store, record)
  integration_payload_result(
    outcome,
    field,
    fragments,
    variables,
    "storefrontAccessTokenCreate",
    "storefrontAccessToken",
    Some(record),
    [],
    store,
    identity,
    [record.id],
  )
}

fn delete_storefront_token(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let id = input_string(input, "id")
  let key = get_field_response_key(field)
  let #(deleted, errors, store) = case id {
    Some(id) ->
      case
        store.get_effective_online_store_integration_by_id(outcome.store, id)
      {
        Some(_) -> #(
          SrcString(id),
          [],
          store.delete_staged_online_store_integration(outcome.store, id),
        )
        None -> #(
          SrcNull,
          [user_error(["id"], "Storefront access token does not exist")],
          outcome.store,
        )
      }
    None -> #(
      SrcNull,
      [user_error(["id"], "Storefront access token does not exist")],
      outcome.store,
    )
  }
  let payload =
    project_payload_source(
      field,
      src_object([
        #("deletedStorefrontAccessTokenId", deleted),
        #("userErrors", user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    mutation_outcome(
      outcome,
      store,
      outcome.identity,
      "storefrontAccessTokenDelete",
      case errors {
        [] -> option_list(id)
        _ -> []
      },
    ),
  )
}

fn create_mobile_app(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let app_type =
    option_string(input_string(input, "applicationType"), "ANDROID")
  let typename = case app_type {
    "APPLE" -> "AppleApplication"
    _ -> "AndroidApplication"
  }
  let app_input = mobile_platform_payload(input)
  let #(record, identity) =
    make_integration(outcome.identity, "mobilePlatformApplication", [
      #("__typename", SrcString(typename)),
      #(
        "applicationId",
        option_source(
          input_string(app_input, "applicationId"),
          "com.example.local",
        ),
      ),
      #(
        "appId",
        option_source(input_string(app_input, "appId"), "com.example.local"),
      ),
      #(
        "appLinksEnabled",
        bool_source(input_bool(app_input, "appLinksEnabled"), True),
      ),
      #(
        "sha256CertFingerprints",
        value_source_from_dict(app_input, "sha256CertFingerprints"),
      ),
    ])
  let #(_, store) =
    store.upsert_staged_online_store_integration(outcome.store, record)
  integration_payload_result(
    outcome,
    field,
    fragments,
    variables,
    "mobilePlatformApplicationCreate",
    "mobilePlatformApplication",
    Some(record),
    [],
    store,
    identity,
    [record.id],
  )
}

fn update_mobile_app(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = input_string(args, "id")
  case id {
    Some(id) ->
      case
        store.get_effective_online_store_integration_by_id(outcome.store, id)
      {
        Some(record) ->
          integration_payload_result(
            outcome,
            field,
            fragments,
            variables,
            "mobilePlatformApplicationUpdate",
            "mobilePlatformApplication",
            Some(record),
            [],
            outcome.store,
            outcome.identity,
            [record.id],
          )
        None ->
          integration_payload_result(
            outcome,
            field,
            fragments,
            variables,
            "mobilePlatformApplicationUpdate",
            "mobilePlatformApplication",
            None,
            [user_error(["id"], "Mobile platform application does not exist")],
            outcome.store,
            outcome.identity,
            [],
          )
      }
    None ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "mobilePlatformApplicationUpdate",
        "mobilePlatformApplication",
        None,
        [user_error(["id"], "Mobile platform application does not exist")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn delete_integration(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
  deleted_key: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  let #(deleted, errors, store) = case id {
    Some(id) ->
      case
        store.get_effective_online_store_integration_by_id(outcome.store, id)
      {
        Some(_) -> #(
          SrcString(id),
          [],
          store.delete_staged_online_store_integration(outcome.store, id),
        )
        None -> #(SrcNull, [integration_not_found_error(kind)], outcome.store)
      }
    None -> #(SrcNull, [integration_not_found_error(kind)], outcome.store)
  }
  let payload =
    project_payload_source(
      field,
      src_object([
        #(deleted_key, deleted),
        #("userErrors", user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    mutation_outcome(outcome, store, outcome.identity, root, case errors {
      [] -> option_list(id)
      _ -> []
    }),
  )
}

fn integration_payload_result(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  payload_key: String,
  record: Option(OnlineStoreIntegrationRecord),
  errors: List(graphql_helpers.SourceValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_ids: List(String),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let value = case record {
    Some(record) ->
      project_integration_payload(
        record,
        field,
        fragments,
        variables,
        payload_key,
      )
    None -> json.null()
  }
  let payload = mutation_payload(field, fragments, payload_key, value, errors)
  #(key, payload, mutation_outcome(outcome, store, identity, root, staged_ids))
}

fn mutation_outcome(
  outcome: MutationOutcome,
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  staged_ids: List(String),
) -> MutationOutcome {
  mutation_outcome_with_status(
    outcome,
    store,
    identity,
    root,
    staged_ids,
    store.Staged,
    Some("Locally staged " <> root <> " in shopify-draft-proxy."),
  )
}

fn mutation_outcome_with_status(
  _outcome: MutationOutcome,
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  staged_ids: List(String),
  status: store.EntryStatus,
  notes: Option(String),
) -> MutationOutcome {
  MutationOutcome(
    data: json.object([]),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [
      single_root_log_draft(
        root,
        staged_ids,
        status,
        "online-store",
        "stage-locally",
        notes,
      ),
    ],
  )
}

fn not_found_payload(
  outcome: MutationOutcome,
  field: Selection,
  root: String,
  payload_key: String,
  message: String,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let errors = [user_error(["id"], message)]
  let payload =
    mutation_payload(field, dict.new(), payload_key, json.null(), errors)
  #(
    key,
    payload,
    mutation_outcome(outcome, outcome.store, outcome.identity, root, []),
  )
}

fn content_validation_error_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  root: String,
  payload_key: String,
  error: graphql_helpers.SourceValue,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let payload =
    mutation_payload(field, fragments, payload_key, json.null(), [error])
  #(
    key,
    payload,
    mutation_outcome_with_status(
      outcome,
      outcome.store,
      outcome.identity,
      root,
      [],
      store.Failed,
      Some("Rejected " <> root <> " validation in shopify-draft-proxy."),
    ),
  )
}

fn make_content(
  identity: SyntheticIdentityRegistry,
  kind: String,
  input: Dict(String, root_field.ResolvedValue),
  parent_id: Option(String),
  existing: Option(OnlineStoreContentRecord),
  handle: String,
) -> #(OnlineStoreContentRecord, SyntheticIdentityRegistry) {
  let gid_type = content_gid_type(kind)
  let #(id, identity) = case existing {
    Some(record) -> #(record.id, identity)
    None -> synthetic_identity.make_proxy_synthetic_gid(identity, gid_type)
  }
  let #(timestamp, identity) = case existing {
    Some(record) -> #(
      option_string(record.updated_at, "2024-01-01T00:00:00.000Z"),
      identity,
    )
    None -> synthetic_identity.make_synthetic_timestamp(identity)
  }
  let prior = case existing {
    Some(record) -> captured_to_source(record.data)
    None -> src_object([])
  }
  let title =
    option_string(
      input_string(input, "title"),
      source_string_field(prior, "title", ""),
    )
  let body = case input_string(input, "body") {
    Some(value) -> scrub_body_html(value)
    None -> source_string_field(prior, "body", "")
  }
  let is_published =
    option_bool(
      input_bool(input, "isPublished"),
      source_bool_field(prior, "isPublished", True),
    )
  let published_at = case is_published {
    True ->
      option_string(
        source_optional_string_field(prior, "publishedAt"),
        timestamp,
      )
    False -> ""
  }
  let source =
    base_source(prior, [
      #("__typename", SrcString(content_typename(kind))),
      #("id", SrcString(id)),
      #("title", SrcString(title)),
      #("handle", SrcString(handle)),
      #("body", SrcString(body)),
      #("bodySummary", SrcString(strip_html(body))),
      #(
        "summary",
        option_source(
          input_string(input, "summary"),
          source_string_field(prior, "summary", ""),
        ),
      ),
      #(
        "tags",
        value_or_default(
          input,
          "tags",
          source_field(prior, "tags", SrcList([])),
        ),
      ),
      #(
        "author",
        value_or_default(
          input,
          "author",
          source_field(prior, "author", src_object([#("name", SrcString(""))])),
        ),
      ),
      #(
        "commentPolicy",
        option_source(
          input_string(input, "commentPolicy"),
          source_string_field(prior, "commentPolicy", "MODERATED"),
        ),
      ),
      #("isPublished", SrcBool(is_published)),
      #("publishedAt", case is_published {
        True -> SrcString(published_at)
        False -> SrcNull
      }),
      #("templateSuffix", source_field(prior, "templateSuffix", SrcNull)),
      #("createdAt", source_field(prior, "createdAt", SrcString(timestamp))),
      #("updatedAt", SrcString(timestamp)),
      #("blogId", case parent_id {
        Some(id) -> SrcString(id)
        None -> source_field(prior, "blogId", SrcNull)
      }),
      #(
        "image",
        value_or_default(input, "image", source_field(prior, "image", SrcNull)),
      ),
      #("metafields", content_metafields_source(kind, input, prior)),
    ])
  #(
    OnlineStoreContentRecord(
      id: id,
      kind: kind,
      cursor: None,
      parent_id: parent_id,
      created_at: source_optional_string_field(source, "createdAt"),
      updated_at: Some(timestamp),
      data: source_to_captured(source),
    ),
    identity,
  )
}

fn make_integration(
  identity: SyntheticIdentityRegistry,
  kind: String,
  entries: List(#(String, graphql_helpers.SourceValue)),
) -> #(OnlineStoreIntegrationRecord, SyntheticIdentityRegistry) {
  let #(id, identity) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity,
      integration_gid_type(kind),
    )
  let source = src_object([#("id", SrcString(id)), ..entries])
  #(
    OnlineStoreIntegrationRecord(
      id: id,
      kind: kind,
      cursor: None,
      created_at: None,
      updated_at: None,
      data: source_to_captured(source),
    ),
    identity,
  )
}

fn mobile_platform_payload(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, "android") {
    Ok(root_field.ObjectVal(fields)) -> fields
    _ ->
      case dict.get(input, "apple") {
        Ok(root_field.ObjectVal(fields)) -> fields
        _ -> input
      }
  }
}

fn content_metafields_source(
  kind: String,
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  let raw =
    value_or_default(
      input,
      "metafields",
      source_field(prior, "metafields", SrcList([])),
    )
  case owner_type_for_content(kind) {
    Some(owner_type) -> enrich_metafields(raw, owner_type)
    None -> raw
  }
}

fn owner_type_for_content(kind: String) -> Option(String) {
  case kind {
    "article" -> Some("ARTICLE")
    "blog" -> Some("BLOG")
    "page" -> Some("PAGE")
    "comment" -> Some("COMMENT")
    _ -> None
  }
}

fn enrich_metafields(
  value: graphql_helpers.SourceValue,
  owner_type: String,
) -> graphql_helpers.SourceValue {
  case value {
    SrcList(items) -> SrcList(list.map(items, enrich_metafield(_, owner_type)))
    _ -> value
  }
}

fn enrich_metafield(
  value: graphql_helpers.SourceValue,
  owner_type: String,
) -> graphql_helpers.SourceValue {
  case value {
    SrcObject(fields) -> {
      let json_value = case dict.get(fields, "jsonValue") {
        Ok(existing) -> existing
        Error(_) ->
          case dict.get(fields, "value") {
            Ok(raw_value) -> raw_value
            Error(_) -> SrcNull
          }
      }
      SrcObject(
        fields
        |> dict.insert("ownerType", SrcString(owner_type))
        |> dict.insert("jsonValue", json_value),
      )
    }
    _ -> value
  }
}

fn singular_content(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
) -> Json {
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  case id {
    Some(id) ->
      case store.get_effective_online_store_content_by_id(store, id) {
        Some(record) if record.kind == kind ->
          project_content_record(store, record, field, fragments, variables)
        _ -> json.null()
      }
    None -> json.null()
  }
}

fn content_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
) -> Json {
  let records =
    store.list_effective_online_store_content(store, kind)
    |> list.filter(root_connection_visible(kind, _))
    |> filter_content_by_query(field, variables)
  let window =
    paginate_connection_items(
      records,
      field,
      variables,
      fn(record, _index) { option_string(record.cursor, record.id) },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(record, _index) {
        option_string(record.cursor, record.id)
      },
      serialize_node: fn(record, node_field, _index) {
        project_content_record(store, record, node_field, fragments, variables)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn singular_integration(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
) -> Json {
  let id = input_string(graphql_helpers.field_args(field, variables), "id")
  case id {
    Some(id) ->
      case store.get_effective_online_store_integration_by_id(store, id) {
        Some(record) if record.kind == kind ->
          project_integration_record(record, field, fragments, variables)
        _ -> json.null()
      }
    None -> json.null()
  }
}

fn first_integration(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  kind: String,
) -> Json {
  case list.first(store.list_effective_online_store_integrations(store, kind)) {
    Ok(record) ->
      project_integration_record(record, field, fragments, dict.new())
    Error(_) -> json.null()
  }
}

fn integration_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
) -> Json {
  let records =
    store.list_effective_online_store_integrations(store, kind)
    |> filter_integration_connection_records(field, variables, kind)
  let window =
    paginate_connection_items(
      records,
      field,
      variables,
      fn(record, _index) { option_string(record.cursor, record.id) },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(record, _index) {
        option_string(record.cursor, record.id)
      },
      serialize_node: fn(record, node_field, _index) {
        project_integration_record(record, node_field, fragments, variables)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn filter_integration_connection_records(
  records: List(OnlineStoreIntegrationRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
) -> List(OnlineStoreIntegrationRecord) {
  let args = graphql_helpers.field_args(field, variables)
  case kind {
    "theme" -> {
      let roles = input_string_list(args, "roles")
      let names = input_string_list(args, "names")
      records
      |> list.filter(fn(record) {
        list.is_empty(roles)
        || list.contains(
          roles,
          source_string_field(captured_to_source(record.data), "role", ""),
        )
      })
      |> list.filter(fn(record) {
        list.is_empty(names)
        || list.contains(
          names,
          source_string_field(captured_to_source(record.data), "name", ""),
        )
      })
    }
    _ -> records
  }
}

fn project_content_record(
  store: Store,
  record: OnlineStoreContentRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source = captured_to_source(record.data)
  let entries =
    list.map(
      get_selected_child_fields(
        field,
        graphql_helpers.SelectedFieldOptions(True),
      ),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "articles" -> #(
                key,
                nested_content_connection(
                  store,
                  child,
                  fragments,
                  variables,
                  "article",
                  record.id,
                ),
              )
              "comments" -> #(
                key,
                nested_content_connection(
                  store,
                  child,
                  fragments,
                  variables,
                  "comment",
                  record.id,
                ),
              )
              "articlesCount" -> #(
                key,
                count_json(
                  list.length(children_for_parent(store, "article", record.id)),
                ),
              )
              "commentsCount" -> #(
                key,
                count_json(
                  list.length(children_for_parent(store, "comment", record.id)),
                ),
              )
              "blog" -> #(key, case record.parent_id {
                Some(id) ->
                  case
                    store.get_effective_online_store_content_by_id(store, id)
                  {
                    Some(blog) ->
                      project_content_record(
                        store,
                        blog,
                        child,
                        fragments,
                        variables,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              })
              "article" -> #(key, case record.parent_id {
                Some(id) ->
                  case
                    store.get_effective_online_store_content_by_id(store, id)
                  {
                    Some(article) ->
                      project_content_record(
                        store,
                        article,
                        child,
                        fragments,
                        variables,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              })
              "metafield" -> #(
                key,
                project_first_metafield(source, child, fragments),
              )
              "metafields" -> #(
                key,
                project_metafields_connection(
                  source,
                  child,
                  fragments,
                  variables,
                ),
              )
              _ -> #(
                key,
                project_graphql_value(
                  source_field(source, name.value, SrcNull),
                  child_selections(child),
                  fragments,
                ),
              )
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn project_integration_record(
  record: OnlineStoreIntegrationRecord,
  field: Selection,
  fragments: FragmentMap,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source = integration_projection_source(record)
  let entries =
    list.map(
      get_selected_child_fields(
        field,
        graphql_helpers.SelectedFieldOptions(True),
      ),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "files" -> #(
                key,
                theme_files_connection(source, child, fragments),
              )
              "settings" -> #(
                key,
                source_to_json(source_field(source, "settings", SrcNull)),
              )
              _ -> #(
                key,
                project_graphql_value(
                  source_field(source, name.value, SrcNull),
                  child_selections(child),
                  fragments,
                ),
              )
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn integration_projection_source(
  record: OnlineStoreIntegrationRecord,
) -> graphql_helpers.SourceValue {
  let source = captured_to_source(record.data)
  case record.kind {
    "webPixel" ->
      base_source(without_source_field(source, "webhookEndpointAddress"), [
        #(
          "status",
          web_pixel_status_source(source_field(source, "settings", SrcNull)),
        ),
      ])
    _ -> source
  }
}

fn project_content_payload(
  store: Store,
  record: OnlineStoreContentRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_key: String,
) -> Json {
  project_content_record(
    store,
    record,
    payload_field_selection(field, payload_key),
    fragments,
    variables,
  )
}

fn project_integration_payload(
  record: OnlineStoreIntegrationRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_key: String,
) -> Json {
  project_integration_record(
    record,
    payload_field_selection(field, payload_key),
    fragments,
    variables,
  )
}

fn payload_field_selection(field: Selection, payload_key: String) -> Selection {
  case
    get_selected_child_fields(field, graphql_helpers.SelectedFieldOptions(True))
    |> list.find(fn(child) {
      case child {
        Field(name: name, ..) -> name.value == payload_key
        _ -> False
      }
    })
  {
    Ok(child) -> child
    Error(_) -> field
  }
}

fn content_payload_source(
  store: Store,
  record: OnlineStoreContentRecord,
) -> graphql_helpers.SourceValue {
  let source = captured_to_source(record.data)
  let extras = case record.kind {
    "blog" -> [
      #(
        "articlesCount",
        count_source(
          list.length(children_for_parent(store, "article", record.id)),
        ),
      ),
    ]
    "article" -> [
      #(
        "commentsCount",
        count_source(
          list.length(children_for_parent(store, "comment", record.id)),
        ),
      ),
      #("blog", case record.parent_id {
        Some(id) ->
          case store.get_effective_online_store_content_by_id(store, id) {
            Some(blog) -> captured_to_source(blog.data)
            None -> SrcNull
          }
        None -> SrcNull
      }),
      #("metafield", case source_field(source, "metafields", SrcList([])) {
        SrcList([first, ..]) -> first
        _ -> SrcNull
      }),
    ]
    _ -> []
  }
  base_source(source, extras)
}

fn nested_content_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
  parent_id: String,
) -> Json {
  let records = children_for_parent(store, kind, parent_id)
  let window =
    paginate_connection_items(
      records,
      field,
      variables,
      fn(record, _index) { option_string(record.cursor, record.id) },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(record, _index) {
        option_string(record.cursor, record.id)
      },
      serialize_node: fn(record, node_field, _index) {
        project_content_record(store, record, node_field, fragments, variables)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn root_connection_visible(
  kind: String,
  record: OnlineStoreContentRecord,
) -> Bool {
  case kind {
    "article" ->
      source_bool_field(captured_to_source(record.data), "isPublished", False)
    _ -> True
  }
}

fn children_for_parent(
  store: Store,
  kind: String,
  parent_id: String,
) -> List(OnlineStoreContentRecord) {
  store.list_effective_online_store_content(store, kind)
  |> list.filter(fn(record) { record.parent_id == Some(parent_id) })
}

fn article_authors_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let authors =
    store.list_effective_online_store_content(store, "article")
    |> list.filter_map(fn(record) {
      case source_field(captured_to_source(record.data), "author", SrcNull) {
        SrcObject(author) ->
          case dict.get(author, "name") {
            Ok(SrcString(name)) -> Ok(src_object([#("name", SrcString(name))]))
            _ -> Error(Nil)
          }
        _ -> Error(Nil)
      }
    })
  let window =
    paginate_connection_items(
      authors,
      field,
      variables,
      fn(author, _index) { source_string_field(author, "name", "") },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(author, _index) {
        source_string_field(author, "name", "")
      },
      serialize_node: fn(author, node_field, _index) {
        project_graphql_value(author, child_selections(node_field), fragments)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn article_tags(store: Store) -> List(String) {
  store.list_effective_online_store_content(store, "article")
  |> list.flat_map(fn(record) {
    case source_field(captured_to_source(record.data), "tags", SrcList([])) {
      SrcList(items) ->
        list.filter_map(items, fn(item) {
          case item {
            SrcString(tag) -> Ok(tag)
            _ -> Error(Nil)
          }
        })
      _ -> []
    }
  })
  |> dedupe()
}

fn project_shop(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source = src_object([#("id", SrcString(synthetic_shop_id))])
  let entries =
    list.map(
      get_selected_child_fields(
        field,
        graphql_helpers.SelectedFieldOptions(True),
      ),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "storefrontAccessTokens" -> #(
                key,
                integration_connection(
                  store,
                  child,
                  fragments,
                  variables,
                  "storefrontAccessToken",
                ),
              )
              _ -> #(
                key,
                project_graphql_value(
                  source_field(source, name.value, SrcNull),
                  child_selections(child),
                  fragments,
                ),
              )
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn filter_content_by_query(
  records: List(OnlineStoreContentRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(OnlineStoreContentRecord) {
  let query =
    input_string(graphql_helpers.field_args(field, variables), "query")
  case query {
    None -> records
    Some(query) ->
      list.filter(records, fn(record) {
        matches_query(captured_to_source(record.data), query)
      })
  }
}

fn matches_query(source: graphql_helpers.SourceValue, query: String) -> Bool {
  let q = string.lowercase(query)
  let title = string.lowercase(source_string_field(source, "title", ""))
  let body = string.lowercase(source_string_field(source, "body", ""))
  let author = string.lowercase(nested_string(source, "author", "name"))
  let tags =
    string.lowercase(string.join(source_string_list(source, "tags"), " "))
  let published = source_bool_field(source, "isPublished", False)
  let text_match =
    string.contains(title, unquote_query_value(q))
    || string.contains(body, unquote_query_value(q))
    || string.contains(tags, unquote_query_value(q))
  case string.contains(q, "published_status:published") && !published {
    True -> False
    False ->
      case string.contains(q, "published_status:unpublished") && published {
        True -> False
        False ->
          case string.contains(q, "tag:") {
            True -> string.contains(tags, value_after(q, "tag:"))
            False ->
              case string.contains(q, "author:") {
                True ->
                  string.contains(
                    author,
                    unquote_query_value(value_after(q, "author:")),
                  )
                False ->
                  case string.contains(q, "title:") {
                    True ->
                      string.contains(
                        title,
                        unquote_query_value(value_after(q, "title:")),
                      )
                    False -> text_match
                  }
              }
          }
      }
  }
}

fn mutation_payload(
  field: Selection,
  fragments: FragmentMap,
  payload_key: String,
  value: Json,
  errors: List(graphql_helpers.SourceValue),
) -> Json {
  json.object(
    child_selections(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            name if name == payload_key -> #(key, value)
            "userErrors" -> #(
              key,
              project_graphql_value(
                user_errors_source(errors),
                child_selections(child),
                fragments,
              ),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn project_payload_source(
  field: Selection,
  source: graphql_helpers.SourceValue,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, child_selections(field), fragments)
}

fn count_json(count: Int) -> Json {
  json.object([
    #("count", json.int(count)),
    #("precision", json.string("EXACT")),
  ])
}

fn content_count_json(
  store_in: Store,
  kind: String,
  upstream: UpstreamContext,
  operation_name: String,
  query: String,
  root: String,
) -> Json {
  let local_count =
    store.list_effective_online_store_content(store_in, kind)
    |> list.length
  let overlay_count = new_staged_online_store_content_count(store_in, kind)
  case should_fetch_count_baseline(store_in, kind, overlay_count) {
    True ->
      case fetch_upstream_content_count(upstream, operation_name, query, root) {
        Some(upstream_count) -> count_json(upstream_count + overlay_count)
        None -> count_json(local_count)
      }
    False -> count_json(local_count)
  }
}

fn should_fetch_count_baseline(
  store_in: Store,
  kind: String,
  overlay_count: Int,
) -> Bool {
  overlay_count > 0 && base_online_store_content_count(store_in, kind) == 0
}

fn base_online_store_content_count(store_in: Store, kind: String) -> Int {
  dict.values(store_in.base_state.online_store_content)
  |> list.filter(fn(record) { record.kind == kind })
  |> list.length
}

fn new_staged_online_store_content_count(store_in: Store, kind: String) -> Int {
  dict.values(store_in.staged_state.online_store_content)
  |> list.filter(fn(record) {
    record.kind == kind
    && !dict.has_key(store_in.base_state.online_store_content, record.id)
  })
  |> list.length
}

fn fetch_upstream_content_count(
  upstream: UpstreamContext,
  operation_name: String,
  query: String,
  root: String,
) -> Option(Int) {
  // Pattern 2: lifecycle reads with staged content need Shopify's existing
  // count baseline, but the surrounding document contains local synthetic IDs
  // and cannot be forwarded verbatim.
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      operation_name,
      query,
      json.object([]),
    )
  {
    Ok(value) ->
      json_get(value, "data")
      |> option.then(json_get(_, root))
      |> option.then(json_get(_, "count"))
      |> option.then(json_int)
    Error(_) -> None
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn json_int(value: commit.JsonValue) -> Option(Int) {
  case value {
    commit.JsonInt(n) -> Some(n)
    _ -> None
  }
}

fn count_source(count: Int) -> graphql_helpers.SourceValue {
  src_object([
    #("count", SrcInt(count)),
    #("precision", SrcString("EXACT")),
  ])
}

fn user_error(
  field: List(String),
  message: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
  ])
}

fn web_pixel_taken_error() -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("WebPixelUserError")),
    #("field", SrcNull),
    #("message", SrcString("Web pixel is taken.")),
    #("code", SrcString("TAKEN")),
  ])
}

fn web_pixel_user_error(
  field: List(String),
  message: String,
  code: Option(String),
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("WebPixelUserError")),
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
    #("code", case code {
      Some(code) -> SrcString(code)
      None -> SrcNull
    }),
  ])
}

fn integration_not_found_error(kind: String) -> graphql_helpers.SourceValue {
  case kind {
    "webPixel" ->
      web_pixel_user_error(["id"], "Integration does not exist", None)
    _ -> user_error(["id"], "Integration does not exist")
  }
}

fn web_pixel_status_source(
  settings: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  case settings {
    SrcNull -> SrcString("NEEDS_CONFIGURATION")
    _ -> SrcString("CONNECTED")
  }
}

fn same_current_app_web_pixel(record: OnlineStoreIntegrationRecord) -> Bool {
  current_app_key(captured_to_source(record.data)) == current_app_key(SrcNull)
}

fn current_app_key(source: graphql_helpers.SourceValue) -> Option(String) {
  case source_optional_string_field(source, "apiPermission") {
    Some(value) -> Some(value)
    None ->
      case source_optional_string_field(source, "api_permission") {
        Some(value) -> Some(value)
        None -> None
      }
  }
}

fn user_error_with_code(
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

fn article_user_error(
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList([SrcString("article")])),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

fn required_title_error(
  payload_key: String,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(graphql_helpers.SourceValue) {
  case input_non_blank_string(input, "title") {
    Some(_) -> None
    None ->
      Some(user_error_with_code(
        [payload_key, "title"],
        "Title can't be blank",
        "BLANK",
      ))
  }
}

fn user_errors_source(
  errors: List(graphql_helpers.SourceValue),
) -> graphql_helpers.SourceValue {
  SrcList(errors)
}

fn input_list(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(root_field.ResolvedValue) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(items)) -> items
    _ -> []
  }
}

fn input_string_list(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  input_list(args, name)
  |> list.filter_map(fn(value) {
    case value {
      root_field.StringVal(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

fn input_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn input_non_blank_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case input_string(args, name) {
    Some(value) -> {
      let trimmed = string.trim(value)
      case trimmed == "" {
        True -> None
        False -> Some(trimmed)
      }
    }
    None -> None
  }
}

fn input_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn value_source_from_dict(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> graphql_helpers.SourceValue {
  case dict.get(args, name) {
    Ok(value) -> graphql_helpers.resolved_value_to_source(value)
    Error(_) -> SrcNull
  }
}

fn value_or_default(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
  default: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  case dict.get(args, name) {
    Ok(value) -> graphql_helpers.resolved_value_to_source(value)
    Error(_) -> default
  }
}

fn option_source(
  value: Option(String),
  default: String,
) -> graphql_helpers.SourceValue {
  SrcString(option_string(value, default))
}

fn bool_source(
  value: Option(Bool),
  default: Bool,
) -> graphql_helpers.SourceValue {
  SrcBool(option_bool(value, default))
}

fn option_string(value: Option(String), default: String) -> String {
  case value {
    Some(value) -> value
    None -> default
  }
}

fn option_bool(value: Option(Bool), default: Bool) -> Bool {
  case value {
    Some(value) -> value
    None -> default
  }
}

fn option_list(value: Option(a)) -> List(a) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

fn first_option(items: List(a)) -> Option(a) {
  case items {
    [first, ..] -> Some(first)
    [] -> None
  }
}

fn option_then(value: Option(a), fun: fn(a) -> Option(b)) -> Option(b) {
  case value {
    Some(value) -> fun(value)
    None -> None
  }
}

fn child_selections(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, graphql_helpers.SelectedFieldOptions(True))
}

fn source_field(
  source: graphql_helpers.SourceValue,
  name: String,
  default: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  case source {
    SrcObject(fields) ->
      case dict.get(fields, name) {
        Ok(value) -> value
        Error(_) -> default
      }
    _ -> default
  }
}

fn without_source_field(
  source: graphql_helpers.SourceValue,
  name: String,
) -> graphql_helpers.SourceValue {
  case source {
    SrcObject(fields) -> SrcObject(dict.delete(fields, name))
    _ -> source
  }
}

fn source_string_field(
  source: graphql_helpers.SourceValue,
  name: String,
  default: String,
) -> String {
  case source_field(source, name, SrcNull) {
    SrcString(value) -> value
    _ -> default
  }
}

fn source_optional_string_field(
  source: graphql_helpers.SourceValue,
  name: String,
) -> Option(String) {
  case source_field(source, name, SrcNull) {
    SrcString(value) -> Some(value)
    _ -> None
  }
}

fn source_bool_field(
  source: graphql_helpers.SourceValue,
  name: String,
  default: Bool,
) -> Bool {
  case source_field(source, name, SrcNull) {
    SrcBool(value) -> value
    _ -> default
  }
}

fn source_string_list(
  source: graphql_helpers.SourceValue,
  name: String,
) -> List(String) {
  case source_field(source, name, SrcList([])) {
    SrcList(items) ->
      list.filter_map(items, fn(item) {
        case item {
          SrcString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn nested_string(
  source: graphql_helpers.SourceValue,
  object_key: String,
  key: String,
) -> String {
  case source_field(source, object_key, SrcNull) {
    SrcObject(fields) ->
      case dict.get(fields, key) {
        Ok(SrcString(value)) -> value
        _ -> ""
      }
    _ -> ""
  }
}

fn maybe_insert_string(
  data: CapturedJsonValue,
  key: String,
  value: Option(String),
) -> CapturedJsonValue {
  case value {
    Some(value) -> captured_object_insert(data, key, CapturedString(value))
    None -> data
  }
}

fn maybe_insert_bool(
  data: CapturedJsonValue,
  key: String,
  value: Option(Bool),
) -> CapturedJsonValue {
  case value {
    Some(value) -> captured_object_insert(data, key, CapturedBool(value))
    None -> data
  }
}

fn captured_object_insert(
  data: CapturedJsonValue,
  key: String,
  value: CapturedJsonValue,
) -> CapturedJsonValue {
  case data {
    CapturedObject(entries) ->
      CapturedObject([
        #(key, value),
        ..list.filter(entries, fn(pair) { pair.0 != key })
      ])
    _ -> CapturedObject([#(key, value)])
  }
}

fn base_source(
  prior: graphql_helpers.SourceValue,
  entries: List(#(String, graphql_helpers.SourceValue)),
) -> graphql_helpers.SourceValue {
  let base = case prior {
    SrcObject(fields) -> fields
    _ -> dict.new()
  }
  SrcObject(
    list.fold(entries, base, fn(acc, entry) {
      dict.insert(acc, entry.0, entry.1)
    }),
  )
}

fn captured_to_source(value: CapturedJsonValue) -> graphql_helpers.SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> graphql_helpers.SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_to_source))
    CapturedObject(entries) ->
      SrcObject(
        list.fold(entries, dict.new(), fn(acc, entry) {
          dict.insert(acc, entry.0, captured_to_source(entry.1))
        }),
      )
  }
}

fn source_to_captured(value: graphql_helpers.SourceValue) -> CapturedJsonValue {
  case value {
    SrcNull -> CapturedNull
    SrcBool(value) -> CapturedBool(value)
    SrcInt(value) -> CapturedInt(value)
    graphql_helpers.SrcFloat(value) -> CapturedFloat(value)
    SrcString(value) -> CapturedString(value)
    SrcList(items) -> CapturedArray(list.map(items, source_to_captured))
    SrcObject(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) { #(pair.0, source_to_captured(pair.1)) }),
      )
  }
}

fn project_first_metafield(
  source: graphql_helpers.SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case source_field(source, "metafields", SrcList([])) {
    SrcList([first, ..]) ->
      project_graphql_value(first, child_selections(field), fragments)
    _ -> json.null()
  }
}

fn project_metafields_connection(
  source: graphql_helpers.SourceValue,
  field: Selection,
  fragments: FragmentMap,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = case source_field(source, "metafields", SrcList([])) {
    SrcList(items) -> items
    _ -> []
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(_item, index) { int.to_string(index) },
      serialize_node: fn(item, node_field, _index) {
        project_graphql_value(item, child_selections(node_field), fragments)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn theme_files_connection(
  source: graphql_helpers.SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let items = case source_field(source, "files", SrcList([])) {
    SrcList(items) -> items
    _ -> []
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(_item, index) { int.to_string(index) },
      serialize_node: fn(item, node_field, _index) {
        project_graphql_value(item, child_selections(node_field), fragments)
      },
      selected_field_options: graphql_helpers.SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(True, True, True, None, None),
    ),
  )
}

fn make_theme_files(
  files: List(root_field.ResolvedValue),
) -> List(graphql_helpers.SourceValue) {
  list.filter_map(files, fn(file) {
    case file {
      root_field.ObjectVal(fields) -> {
        let filename = option_string(input_string(fields, "filename"), "")
        let body =
          graphql_helpers.read_arg_object(fields, "body")
          |> option.unwrap(dict.new())
        let content = option_string(input_string(body, "value"), "")
        Ok(make_theme_file(filename, content))
      }
      _ -> Error(Nil)
    }
  })
}

fn make_copied_theme_files(
  files: List(root_field.ResolvedValue),
  current_files: List(graphql_helpers.SourceValue),
) -> List(graphql_helpers.SourceValue) {
  list.filter_map(files, fn(file) {
    case file {
      root_field.ObjectVal(fields) -> {
        let src = option_string(input_string(fields, "srcFilename"), "")
        let dst = option_string(input_string(fields, "dstFilename"), "")
        case find_theme_file(current_files, src) {
          Some(source_file) ->
            Ok(make_theme_file(dst, theme_file_content(source_file)))
          None -> Error(Nil)
        }
      }
      _ -> Error(Nil)
    }
  })
}

fn make_theme_file(
  filename: String,
  content: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("OnlineStoreThemeFile")),
    #("filename", SrcString(filename)),
    #("size", SrcInt(string.byte_size(content))),
    #("checksumMd5", SrcString(crypto.md5_hex(content))),
    #(
      "body",
      src_object([
        #("__typename", SrcString("OnlineStoreThemeFileBodyText")),
        #("content", SrcString(content)),
      ]),
    ),
  ])
}

fn theme_record_files(
  theme: OnlineStoreIntegrationRecord,
) -> List(graphql_helpers.SourceValue) {
  case source_field(captured_to_source(theme.data), "files", SrcList([])) {
    SrcList(files) -> files
    _ -> []
  }
}

fn theme_with_files(
  theme: OnlineStoreIntegrationRecord,
  files: List(graphql_helpers.SourceValue),
) -> OnlineStoreIntegrationRecord {
  OnlineStoreIntegrationRecord(
    ..theme,
    data: captured_object_insert(
      theme.data,
      "files",
      source_to_captured(SrcList(files)),
    ),
  )
}

fn replace_theme_file(
  files: List(graphql_helpers.SourceValue),
  file: graphql_helpers.SourceValue,
) -> List(graphql_helpers.SourceValue) {
  let filename = theme_file_filename(file)
  list.filter(files, fn(existing) { theme_file_filename(existing) != filename })
  |> list.append([file])
}

fn find_theme_file(
  files: List(graphql_helpers.SourceValue),
  filename: String,
) -> Option(graphql_helpers.SourceValue) {
  files
  |> list.find(fn(file) { theme_file_filename(file) == filename })
  |> option.from_result
}

fn theme_file_filename(file: graphql_helpers.SourceValue) -> String {
  source_string_field(file, "filename", "")
}

fn theme_file_content(file: graphql_helpers.SourceValue) -> String {
  case source_field(file, "body", SrcNull) {
    SrcObject(fields) ->
      case dict.get(fields, "content") {
        Ok(SrcString(content)) -> content
        _ -> ""
      }
    _ -> ""
  }
}

fn theme_file_input_filename_errors(
  files: List(root_field.ResolvedValue),
  field_name: String,
) -> List(graphql_helpers.SourceValue) {
  files
  |> list.index_map(fn(file, index) {
    case file {
      root_field.ObjectVal(fields) ->
        case input_string(fields, field_name) {
          Some(filename) ->
            case valid_theme_file_filename(filename) {
              True -> []
              False -> [
                theme_file_user_error(
                  ["files", int.to_string(index), field_name],
                  "Filename is invalid",
                  "INVALID",
                ),
              ]
            }
          None -> [
            theme_file_user_error(
              ["files", int.to_string(index), field_name],
              "Filename is invalid",
              "INVALID",
            ),
          ]
        }
      _ -> [
        theme_file_user_error(
          ["files", int.to_string(index), field_name],
          "Filename is invalid",
          "INVALID",
        ),
      ]
    }
  })
  |> list.flatten
}

fn theme_file_copy_source_errors(
  files: List(root_field.ResolvedValue),
  current_files: List(graphql_helpers.SourceValue),
) -> List(graphql_helpers.SourceValue) {
  files
  |> list.index_map(fn(file, index) {
    case file {
      root_field.ObjectVal(fields) ->
        case input_string(fields, "srcFilename") {
          Some(filename) ->
            case find_theme_file(current_files, filename) {
              Some(_) -> []
              None -> [
                theme_file_user_error(
                  ["files", int.to_string(index), "srcFilename"],
                  "File not found",
                  "NOT_FOUND",
                ),
              ]
            }
          None -> [
            theme_file_user_error(
              ["files", int.to_string(index), "srcFilename"],
              "File not found",
              "NOT_FOUND",
            ),
          ]
        }
      _ -> [
        theme_file_user_error(
          ["files", int.to_string(index), "srcFilename"],
          "File not found",
          "NOT_FOUND",
        ),
      ]
    }
  })
  |> list.flatten
}

fn required_theme_file_delete_errors(
  filenames: List(String),
) -> List(graphql_helpers.SourceValue) {
  filenames
  |> list.index_map(fn(filename, index) {
    case filename {
      "config/settings_data.json" | "config/settings_schema.json" -> [
        theme_file_user_error(
          ["files", int.to_string(index)],
          "File is required and can't be deleted",
          "INVALID",
        ),
      ]
      _ -> []
    }
  })
  |> list.flatten
}

fn input_string_values(values: List(root_field.ResolvedValue)) -> List(String) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.StringVal(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

fn valid_theme_file_filename(filename: String) -> Bool {
  case string.split(filename, "/") {
    [directory, basename] ->
      basename != ""
      && list.contains(
        [
          "templates",
          "sections",
          "snippets",
          "layout",
          "config",
          "locales",
          "assets",
        ],
        directory,
      )
    _ -> False
  }
}

fn theme_file_user_error(
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

fn content_gid_type(kind: String) -> String {
  case kind {
    "blog" -> "Blog"
    "page" -> "Page"
    "comment" -> "Comment"
    _ -> "Article"
  }
}

fn content_typename(kind: String) -> String {
  content_gid_type(kind)
}

fn resolve_content_handle(
  store: Store,
  kind: String,
  input: Dict(String, root_field.ResolvedValue),
  parent_id: Option(String),
  existing: Option(OnlineStoreContentRecord),
) -> Result(String, graphql_helpers.SourceValue) {
  let existing_id = case existing {
    Some(record) -> Some(record.id)
    None -> None
  }
  let prior = case existing {
    Some(record) -> captured_to_source(record.data)
    None -> src_object([])
  }
  case input_string(input, "handle") {
    Some(raw_handle) -> {
      let handle = slugify(raw_handle)
      case handle_exists_in_scope(store, kind, parent_id, handle, existing_id) {
        True -> Error(handle_taken_error(kind))
        False -> Ok(handle)
      }
    }
    None ->
      case source_optional_string_field(prior, "handle") {
        Some(handle) -> Ok(handle)
        None -> {
          let title =
            option_string(
              input_string(input, "title"),
              source_string_field(prior, "title", ""),
            )
          Ok(unique_content_handle(
            store,
            kind,
            parent_id,
            slugify(title),
            existing_id,
          ))
        }
      }
  }
}

fn unique_content_handle(
  store: Store,
  kind: String,
  parent_id: Option(String),
  base: String,
  existing_id: Option(String),
) -> String {
  case handle_exists_in_scope(store, kind, parent_id, base, existing_id) {
    False -> base
    True ->
      unique_content_handle_loop(store, kind, parent_id, base, existing_id, 1)
  }
}

fn unique_content_handle_loop(
  store: Store,
  kind: String,
  parent_id: Option(String),
  base: String,
  existing_id: Option(String),
  suffix: Int,
) -> String {
  let candidate = base <> "-" <> int.to_string(suffix)
  case handle_exists_in_scope(store, kind, parent_id, candidate, existing_id) {
    False -> candidate
    True ->
      unique_content_handle_loop(
        store,
        kind,
        parent_id,
        base,
        existing_id,
        suffix + 1,
      )
  }
}

fn handle_exists_in_scope(
  store: Store,
  kind: String,
  parent_id: Option(String),
  handle: String,
  existing_id: Option(String),
) -> Bool {
  store.list_effective_online_store_content(store, kind)
  |> list.any(fn(record) {
    !same_content_id(record.id, existing_id)
    && content_record_in_handle_scope(record, kind, parent_id)
    && content_record_handle(record) == handle
  })
}

fn same_content_id(id: String, existing_id: Option(String)) -> Bool {
  case existing_id {
    Some(existing_id) -> id == existing_id
    None -> False
  }
}

fn content_record_in_handle_scope(
  record: OnlineStoreContentRecord,
  kind: String,
  parent_id: Option(String),
) -> Bool {
  case kind {
    "article" -> record.parent_id == parent_id
    _ -> True
  }
}

fn content_record_handle(record: OnlineStoreContentRecord) -> String {
  record.data
  |> captured_to_source
  |> source_string_field("handle", "")
}

fn handle_taken_error(kind: String) -> graphql_helpers.SourceValue {
  user_error_with_code(
    [kind, "handle"],
    "Handle has already been taken",
    "TAKEN",
  )
}

fn integration_gid_type(kind: String) -> String {
  case kind {
    "theme" -> "OnlineStoreTheme"
    "scriptTag" -> "ScriptTag"
    "webPixel" -> "WebPixel"
    "serverPixel" -> "ServerPixel"
    "storefrontAccessToken" -> "StorefrontAccessToken"
    _ -> "MobilePlatformApplication"
  }
}

fn slugify(title: String) -> String {
  let lowered = string.lowercase(string.trim(title))
  let #(chars, _) =
    string.to_graphemes(lowered)
    |> list.fold(#([], False), fn(acc, char) {
      let #(out, in_bad_run) = acc
      case is_slug_char(char) {
        True -> #(list.append(out, [char]), False)
        False ->
          case in_bad_run {
            True -> #(out, True)
            False -> #(list.append(out, ["-"]), True)
          }
      }
    })
  chars
  |> string.join("")
  |> trim_dashes
}

fn is_slug_char(char: String) -> Bool {
  case char {
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn trim_dashes(value: String) -> String {
  let chars = string.to_graphemes(value)
  let dropped_left = list.drop_while(chars, fn(char) { char == "-" })
  list.reverse(dropped_left)
  |> list.drop_while(fn(char) { char == "-" })
  |> list.reverse()
  |> string.join("")
}

fn strip_html(value: String) -> String {
  strip_html_loop(string.to_graphemes(value), False, [])
}

fn strip_html_loop(
  chars: List(String),
  in_tag: Bool,
  acc: List(String),
) -> String {
  case chars {
    [] -> string.join(list.reverse(acc), "")
    [first, ..rest] ->
      case first {
        "<" -> strip_html_loop(rest, True, acc)
        ">" -> strip_html_loop(rest, False, acc)
        _ ->
          case in_tag {
            True -> strip_html_loop(rest, in_tag, acc)
            False -> strip_html_loop(rest, in_tag, [first, ..acc])
          }
      }
  }
}

fn scrub_body_html(value: String) -> String {
  scrub_body_html_loop(string.to_graphemes(value), [])
}

fn scrub_body_html_loop(chars: List(String), acc: List(String)) -> String {
  case chars {
    [] -> string.join(list.reverse(acc), "")
    ["<", ..rest] -> {
      let #(tag, after_tag, found_end) = read_html_tag(rest, [])
      case found_end {
        False -> string.join(list.reverse(acc), "") <> "<" <> tag
        True -> {
          let tag_name = html_tag_name(tag)
          case is_disallowed_body_tag(tag_name), is_closing_html_tag(tag) {
            True, False ->
              drop_disallowed_body_element(after_tag, tag_name, 1)
              |> scrub_body_html_loop(acc)
            _, _ -> {
              let scrubbed_tag = strip_event_handler_attributes(tag)
              scrub_body_html_loop(after_tag, [">", scrubbed_tag, "<", ..acc])
            }
          }
        }
      }
    }
    [first, ..rest] -> scrub_body_html_loop(rest, [first, ..acc])
  }
}

fn read_html_tag(
  chars: List(String),
  acc: List(String),
) -> #(String, List(String), Bool) {
  case chars {
    [] -> #(string.join(list.reverse(acc), ""), [], False)
    [">", ..rest] -> #(string.join(list.reverse(acc), ""), rest, True)
    [first, ..rest] -> read_html_tag(rest, [first, ..acc])
  }
}

fn drop_disallowed_body_element(
  chars: List(String),
  tag_name: String,
  depth: Int,
) -> List(String) {
  case chars {
    [] -> []
    ["<", ..rest] -> {
      let #(tag, after_tag, found_end) = read_html_tag(rest, [])
      case found_end {
        False -> []
        True -> {
          let nested_name = html_tag_name(tag)
          case nested_name == tag_name {
            True ->
              case is_closing_html_tag(tag), is_self_closing_html_tag(tag) {
                True, _ ->
                  case depth <= 1 {
                    True -> after_tag
                    False ->
                      drop_disallowed_body_element(
                        after_tag,
                        tag_name,
                        depth - 1,
                      )
                  }
                False, True ->
                  drop_disallowed_body_element(after_tag, tag_name, depth)
                False, False ->
                  drop_disallowed_body_element(after_tag, tag_name, depth + 1)
              }
            False -> drop_disallowed_body_element(after_tag, tag_name, depth)
          }
        }
      }
    }
    [_, ..rest] -> drop_disallowed_body_element(rest, tag_name, depth)
  }
}

fn html_tag_name(tag: String) -> String {
  tag
  |> string.trim_start
  |> drop_leading_slash
  |> string.trim_start
  |> take_html_name
  |> string.lowercase
}

fn drop_leading_slash(value: String) -> String {
  case string.starts_with(value, "/") {
    True -> string.drop_start(value, 1)
    False -> value
  }
}

fn take_html_name(value: String) -> String {
  take_html_name_loop(string.to_graphemes(value), [])
}

fn take_html_name_loop(chars: List(String), acc: List(String)) -> String {
  case chars {
    [] -> string.join(list.reverse(acc), "")
    [first, ..rest] ->
      case is_html_name_stop(first) {
        True -> string.join(list.reverse(acc), "")
        False -> take_html_name_loop(rest, [first, ..acc])
      }
  }
}

fn is_html_name_stop(char: String) -> Bool {
  is_html_space(char) || char == "/" || char == "="
}

fn is_disallowed_body_tag(tag_name: String) -> Bool {
  tag_name == "script" || tag_name == "iframe"
}

fn is_closing_html_tag(tag: String) -> Bool {
  string.starts_with(string.trim_start(tag), "/")
}

fn is_self_closing_html_tag(tag: String) -> Bool {
  case list.reverse(string.to_graphemes(string.trim(tag))) {
    ["/", ..] -> True
    _ -> False
  }
}

fn strip_event_handler_attributes(tag: String) -> String {
  case is_closing_html_tag(tag) || is_special_html_tag(tag) {
    True -> tag
    False -> {
      let #(prefix, rest) = split_html_tag_prefix(string.to_graphemes(tag), [])
      prefix <> strip_event_attrs_loop(rest, [])
    }
  }
}

fn is_special_html_tag(tag: String) -> Bool {
  let trimmed = string.trim_start(tag)
  string.starts_with(trimmed, "!") || string.starts_with(trimmed, "?")
}

fn split_html_tag_prefix(
  chars: List(String),
  acc: List(String),
) -> #(String, List(String)) {
  case chars {
    [] -> #(string.join(list.reverse(acc), ""), [])
    [first, ..rest] ->
      case is_html_space(first) {
        True -> #(string.join(list.reverse(acc), ""), chars)
        False -> split_html_tag_prefix(rest, [first, ..acc])
      }
  }
}

fn strip_event_attrs_loop(chars: List(String), acc: List(String)) -> String {
  case chars {
    [] -> string.join(list.reverse(acc), "")
    _ -> {
      let #(spaces, rest) = take_html_spaces(chars, [])
      case rest {
        [] -> string.join(list.reverse(list.append(spaces, acc)), "")
        ["/", ..] ->
          string.join(list.reverse(acc), "")
          <> string.join(spaces, "")
          <> string.join(rest, "")
        _ -> {
          let #(name, after_name) = take_attr_name(rest, [])
          case name {
            "" ->
              string.join(list.reverse(acc), "")
              <> string.join(spaces, "")
              <> string.join(rest, "")
            _ -> {
              let #(suffix, after_attr) = take_attr_suffix(after_name, [])
              let segment =
                list.append(
                  spaces,
                  list.append(string.to_graphemes(name), suffix),
                )
              case string.starts_with(string.lowercase(name), "on") {
                True -> strip_event_attrs_loop(after_attr, acc)
                False ->
                  strip_event_attrs_loop(
                    after_attr,
                    list.append(list.reverse(segment), acc),
                  )
              }
            }
          }
        }
      }
    }
  }
}

fn take_html_spaces(
  chars: List(String),
  acc: List(String),
) -> #(List(String), List(String)) {
  case chars {
    [first, ..rest] ->
      case is_html_space(first) {
        True -> take_html_spaces(rest, list.append(acc, [first]))
        False -> #(acc, chars)
      }
    [] -> #(acc, [])
  }
}

fn take_attr_name(
  chars: List(String),
  acc: List(String),
) -> #(String, List(String)) {
  case chars {
    [] -> #(string.join(acc, ""), [])
    [first, ..rest] ->
      case is_attr_name_stop(first) {
        True -> #(string.join(acc, ""), chars)
        False -> take_attr_name(rest, list.append(acc, [first]))
      }
  }
}

fn is_attr_name_stop(char: String) -> Bool {
  is_html_space(char) || char == "=" || char == "/"
}

fn take_attr_suffix(
  chars: List(String),
  acc: List(String),
) -> #(List(String), List(String)) {
  let #(spaces, rest) = take_html_spaces(chars, [])
  let acc = list.append(acc, spaces)
  case rest {
    ["=", ..after_equals] -> {
      let acc = list.append(acc, ["="])
      let #(value_spaces, value_start) = take_html_spaces(after_equals, [])
      let acc = list.append(acc, value_spaces)
      let #(value, after_value) = take_attr_value(value_start, [])
      #(list.append(acc, value), after_value)
    }
    _ -> #(acc, rest)
  }
}

fn take_attr_value(
  chars: List(String),
  acc: List(String),
) -> #(List(String), List(String)) {
  case chars {
    [] -> #(acc, [])
    ["\"", ..rest] -> take_quoted_attr_value(rest, "\"", ["\"", ..acc])
    ["'", ..rest] -> take_quoted_attr_value(rest, "'", ["'", ..acc])
    [first, ..rest] ->
      case is_html_space(first) || first == "/" {
        True -> #(acc, chars)
        False -> take_attr_value(rest, list.append(acc, [first]))
      }
  }
}

fn take_quoted_attr_value(
  chars: List(String),
  quote: String,
  acc: List(String),
) -> #(List(String), List(String)) {
  case chars {
    [] -> #(list.reverse(acc), [])
    [first, ..rest] ->
      case first == quote {
        True -> #(list.reverse([first, ..acc]), rest)
        False -> take_quoted_attr_value(rest, quote, [first, ..acc])
      }
  }
}

fn is_html_space(char: String) -> Bool {
  char == " "
  || char == "\n"
  || char == "\t"
  || char == "\r"
  || char == "\u{000C}"
}

fn value_after(query: String, prefix: String) -> String {
  case string.split_once(query, prefix) {
    Ok(#(_, tail)) ->
      case string.split(tail, " ") {
        [first, ..] -> first
        [] -> tail
      }
    Error(_) -> query
  }
}

fn unquote_query_value(value: String) -> String {
  value
  |> string.replace("\"", "")
  |> string.replace("'", "")
}

fn dedupe(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

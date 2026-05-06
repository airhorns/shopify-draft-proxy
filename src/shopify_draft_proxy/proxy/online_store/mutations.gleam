//// Mutation handling for online-store roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/regexp
import gleam/string
import gleam/uri
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull, SrcObject,
  SrcString, get_document_fragments, get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/online_store/serializers
import shopify_draft_proxy/proxy/online_store/server_pixel_validation
import shopify_draft_proxy/proxy/online_store/types as online_store_types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type OnlineStoreContentRecord,
  type OnlineStoreIntegrationRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  OnlineStoreContentRecord, OnlineStoreIntegrationRecord,
}

@internal
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

@internal
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
  case serializers.required_title_error(payload_key, input) {
    Some(error) ->
      serializers.content_validation_error_payload(
        outcome,
        field,
        fragments,
        root,
        payload_key,
        error,
      )
    None ->
      case future_publish_date_error(payload_key, input, None) {
        Some(error) ->
          serializers.content_validation_error_payload(
            outcome,
            field,
            fragments,
            root,
            payload_key,
            error,
          )
        None -> {
          case
            serializers.resolve_content_handle(
              outcome.store,
              kind,
              input,
              None,
              None,
            )
          {
            Error(error) ->
              serializers.content_validation_error_payload(
                outcome,
                field,
                fragments,
                root,
                payload_key,
                error,
              )
            Ok(handle) -> {
              let #(record, identity) =
                serializers.make_content(
                  outcome.identity,
                  kind,
                  input,
                  None,
                  None,
                  handle,
                )
              let #(_, store) =
                store.upsert_staged_online_store_content(outcome.store, record)
              let payload =
                serializers.mutation_payload(
                  field,
                  fragments,
                  payload_key,
                  serializers.project_content_payload(
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
                serializers.mutation_outcome(outcome, store, identity, root, [
                  record.id,
                ]),
              )
            }
          }
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
      serializers.content_validation_error_payload(
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
          serializers.content_validation_error_payload(
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
            serializers.resolve_content_handle(
              outcome.store,
              "article",
              article_input,
              Some(blog_id),
              None,
            )
          {
            Error(error) ->
              serializers.content_validation_error_payload(
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
                serializers.make_content(
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
                serializers.mutation_payload(
                  field,
                  fragments,
                  "article",
                  serializers.project_content_payload(
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
                serializers.mutation_outcome(
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
  case serializers.input_string(article_input, "blogId") {
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
        serializers.resolve_content_handle(
          outcome.store,
          "blog",
          blog_input,
          None,
          None,
        )
      {
        Error(error) -> Error(error)
        Ok(handle) -> {
          let #(blog, identity) =
            serializers.make_content(
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
  let has_blog_id =
    option.is_some(serializers.input_string(article_input, "blogId"))
  let has_inline_blog = case graphql_helpers.read_arg_object(args, "blog") {
    Some(_) -> True
    None -> False
  }
  case serializers.required_title_error("article", article_input) {
    Some(error) -> Some(error)
    None ->
      case has_blog_id, has_inline_blog {
        True, True ->
          Some(serializers.article_user_error(
            "Can't create a blog from input if a blog ID is supplied.",
            "AMBIGUOUS_BLOG",
          ))
        False, False ->
          Some(serializers.article_user_error(
            "Must reference or create a blog when creating an article.",
            "BLOG_REFERENCE_REQUIRED",
          ))
        _, _ ->
          case article_author_validation_error(article_input) {
            Some(error) -> Some(error)
            None -> future_publish_date_error("article", article_input, None)
          }
      }
  }
}

fn article_author_validation_error(
  article_input: Dict(String, root_field.ResolvedValue),
) -> Option(graphql_helpers.SourceValue) {
  case dict.get(article_input, "author") {
    Ok(root_field.ObjectVal(author)) -> {
      let has_name =
        option.is_some(serializers.input_non_blank_string(author, "name"))
      let has_user_id =
        option.is_some(serializers.input_non_blank_string(author, "userId"))
      case has_name, has_user_id {
        True, True ->
          Some(serializers.article_user_error(
            "Can't create an article author if both author name and user ID are supplied.",
            "AMBIGUOUS_AUTHOR",
          ))
        False, False ->
          Some(serializers.article_user_error(
            "Can't create an article if both author name and user ID are blank.",
            "AUTHOR_FIELD_REQUIRED",
          ))
        _, _ -> None
      }
    }
    _ ->
      Some(serializers.article_user_error(
        "Can't create an article if both author name and user ID are blank.",
        "AUTHOR_FIELD_REQUIRED",
      ))
  }
}

const invalid_publish_date_message: String = "Can’t set isPublished to true and also set a future publish date."

fn future_publish_date_error(
  payload_key: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(OnlineStoreContentRecord),
) -> Option(graphql_helpers.SourceValue) {
  case payload_key {
    "page" | "article" ->
      case effective_is_published(input, existing) {
        False -> None
        True ->
          case serializers.input_string(input, "publishDate") {
            Some(publish_date) ->
              case iso_timestamp_after(publish_date, iso_timestamp.now_iso()) {
                True ->
                  Some(serializers.user_error_with_code(
                    [payload_key],
                    invalid_publish_date_message,
                    "INVALID_PUBLISH_DATE",
                  ))
                False -> None
              }
            None -> None
          }
      }
    _ -> None
  }
}

fn effective_is_published(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(OnlineStoreContentRecord),
) -> Bool {
  let default = case existing {
    Some(record) ->
      serializers.source_bool_field(
        serializers.captured_to_source(record.data),
        "isPublished",
        True,
      )
    None -> True
  }
  serializers.option_bool(serializers.input_bool(input, "isPublished"), default)
}

fn iso_timestamp_after(value: String, timestamp: String) -> Bool {
  case iso_timestamp.parse_iso(value), iso_timestamp.parse_iso(timestamp) {
    Ok(value_ms), Ok(timestamp_ms) -> value_ms > timestamp_ms
    _, _ -> False
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
  let args = graphql_helpers.field_args(field, variables)
  let id = serializers.input_string(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, payload_key)
    |> option.unwrap(dict.new())
  case id {
    Some(id) ->
      case store.get_effective_online_store_content_by_id(outcome.store, id) {
        Some(existing) -> {
          case normalize_blog_commentable(input, kind, payload_key) {
            Error(error) ->
              serializers.content_validation_error_payload(
                outcome,
                field,
                fragments,
                root,
                payload_key,
                error,
              )
            Ok(input) -> {
              case
                article_update_validation_errors(
                  outcome.store,
                  kind,
                  input,
                  existing,
                )
              {
                [_, ..] as errors ->
                  serializers.content_validation_errors_payload(
                    outcome,
                    field,
                    fragments,
                    root,
                    payload_key,
                    errors,
                  )
                [] ->
                  update_content_after_validation(
                    outcome,
                    field,
                    fragments,
                    variables,
                    root,
                    kind,
                    payload_key,
                    id,
                    input,
                    existing,
                  )
              }
            }
          }
        }
        None ->
          serializers.not_found_payload(
            outcome,
            field,
            root,
            payload_key,
            "Content does not exist",
          )
      }
    None ->
      serializers.not_found_payload(
        outcome,
        field,
        root,
        payload_key,
        "Content does not exist",
      )
  }
}

fn update_content_after_validation(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  root: String,
  kind: String,
  payload_key: String,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: OnlineStoreContentRecord,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  case future_publish_date_error(payload_key, input, Some(existing)) {
    Some(error) ->
      serializers.content_validation_error_payload(
        outcome,
        field,
        fragments,
        root,
        payload_key,
        error,
      )
    None -> {
      case
        serializers.resolve_content_handle(
          outcome.store,
          kind,
          input,
          existing.parent_id,
          Some(existing),
        )
      {
        Error(error) ->
          serializers.content_validation_error_payload(
            outcome,
            field,
            fragments,
            root,
            payload_key,
            error,
          )
        Ok(handle) -> {
          let #(record, identity) =
            serializers.make_content(
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
            serializers.mutation_payload(
              field,
              fragments,
              payload_key,
              serializers.project_content_payload(
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
            serializers.mutation_outcome(outcome, store, identity, root, [id]),
          )
        }
      }
    }
  }
}

fn article_update_validation_errors(
  store: Store,
  kind: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: OnlineStoreContentRecord,
) -> List(graphql_helpers.SourceValue) {
  case kind {
    "article" ->
      list.append(
        article_update_author_errors(store, input),
        list.append(
          article_update_blog_errors(input),
          article_update_image_errors(input, existing),
        ),
      )
    _ -> []
  }
}

fn article_update_author_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(graphql_helpers.SourceValue) {
  let author = graphql_helpers.read_arg_object(input, "author")
  let author_v2 = graphql_helpers.read_arg_object(input, "authorV2")
  case author, author_v2 {
    Some(_), Some(_) -> [
      serializers.user_error_with_code(
        ["article", "author"],
        "You must specify either an author name or an author user, not both.",
        "AMBIGUOUS_AUTHOR",
      ),
      serializers.user_error_with_code(
        ["article", "authorV2"],
        "You must specify either an author name or an author user, not both.",
        "AMBIGUOUS_AUTHOR",
      ),
    ]
    Some(author), None -> {
      case serializers.input_non_blank_string(author, "userId") {
        Some(user_id) ->
          case
            option.is_some(serializers.input_non_blank_string(author, "name"))
          {
            True -> [
              serializers.user_error_with_code(
                ["article"],
                "Can't update an article author if both author name and user ID are supplied.",
                "AMBIGUOUS_AUTHOR",
              ),
            ]
            False -> article_update_author_user_id_errors(store, user_id)
          }
        None -> []
      }
    }
    None, Some(author_v2) ->
      case serializers.input_non_blank_string(author_v2, "userId") {
        Some(user_id) -> article_update_author_v2_user_id_errors(store, user_id)
        None -> []
      }
    None, None -> []
  }
}

fn article_update_author_user_id_errors(
  store: Store,
  user_id: String,
) -> List(graphql_helpers.SourceValue) {
  case staff_member_exists(store, user_id) {
    True -> []
    False -> [
      serializers.user_error_with_code(
        ["article"],
        "User must exist if a user ID is supplied.",
        "AUTHOR_MUST_EXIST",
      ),
    ]
  }
}

fn article_update_author_v2_user_id_errors(
  store: Store,
  user_id: String,
) -> List(graphql_helpers.SourceValue) {
  case staff_member_exists(store, user_id) {
    True -> []
    False -> [
      serializers.user_error_with_code(
        ["article", "authorV2", "userId"],
        "Author must exist",
        "NOT_FOUND",
      ),
    ]
  }
}

fn staff_member_exists(store: Store, staff_id: String) -> Bool {
  case store.get_effective_admin_platform_generic_node_by_id(store, staff_id) {
    Some(record) -> record.typename == "StaffMember"
    None -> False
  }
}

fn article_update_blog_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(graphql_helpers.SourceValue) {
  let has_blog_id = option.is_some(serializers.input_string(input, "blogId"))
  let has_inline_blog =
    option.is_some(graphql_helpers.read_arg_object(input, "blog"))
  case has_blog_id, has_inline_blog {
    True, True -> [
      serializers.user_error_with_code(
        ["article", "blogId"],
        "You must specify either a blogId or a blog, not both.",
        "AMBIGUOUS_BLOG",
      ),
      serializers.user_error_with_code(
        ["article", "blog"],
        "You must specify either a blogId or a blog, not both.",
        "AMBIGUOUS_BLOG",
      ),
    ]
    _, _ -> []
  }
}

fn article_update_image_errors(
  input: Dict(String, root_field.ResolvedValue),
  existing: OnlineStoreContentRecord,
) -> List(graphql_helpers.SourceValue) {
  case graphql_helpers.read_arg_object(input, "image") {
    Some(image) -> {
      let has_alt = option.is_some(serializers.input_string(image, "altText"))
      let has_url =
        option.is_some(serializers.input_non_blank_string(image, "url"))
      let existing_has_image = case
        serializers.source_field(
          serializers.captured_to_source(existing.data),
          "image",
          SrcNull,
        )
      {
        SrcNull -> False
        _ -> True
      }
      case has_alt, has_url, existing_has_image {
        True, False, False -> [
          serializers.user_error_with_code(
            ["article", "image"],
            "Cannot update image alt text without an existing image or providing a new image URL",
            "INVALID",
          ),
        ]
        _, _, _ -> []
      }
    }
    None -> []
  }
}

fn normalize_blog_commentable(
  input: Dict(String, root_field.ResolvedValue),
  kind: String,
  payload_key: String,
) -> Result(Dict(String, root_field.ResolvedValue), graphql_helpers.SourceValue) {
  case kind, serializers.input_string(input, "commentable") {
    "blog", Some(value) ->
      case blog_commentable_to_comment_policy(value) {
        Some(comment_policy) ->
          Ok(dict.insert(
            input,
            "commentPolicy",
            root_field.StringVal(comment_policy),
          ))
        None ->
          Error(serializers.user_error_with_code(
            [payload_key, "commentable"],
            "Commentable is not included in the list",
            "INCLUSION",
          ))
      }
    _, _ -> Ok(input)
  }
}

fn blog_commentable_to_comment_policy(value: String) -> Option(String) {
  case value {
    "MODERATE" | "MODERATED" -> Some("MODERATED")
    "AUTO_PUBLISHED" -> Some("AUTO_PUBLISHED")
    "CLOSED" -> Some("CLOSED")
    _ -> None
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
  let id =
    serializers.input_string(graphql_helpers.field_args(field, variables), "id")
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
          [serializers.user_error(["id"], "Content does not exist")],
          outcome.store,
        )
      }
    None -> #(
      SrcNull,
      [serializers.user_error(["id"], "Content does not exist")],
      outcome.store,
    )
  }
  let payload =
    serializers.project_payload_source(
      field,
      src_object([
        #(deleted_key, deleted),
        #("userErrors", serializers.user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    serializers.mutation_outcome(
      outcome,
      store,
      outcome.identity,
      root,
      case errors {
        [] -> serializers.option_list(id)
        _ -> []
      },
    ),
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
  let id =
    serializers.input_string(graphql_helpers.field_args(field, variables), "id")
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
                serializers.content_payload_source(next_store, record),
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
    serializers.project_payload_source(
      field,
      src_object([
        #("comment", comment),
        #("userErrors", serializers.user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    serializers.mutation_outcome(outcome, store, identity, root, []),
  )
}

fn delete_comment(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let id =
    serializers.input_string(graphql_helpers.field_args(field, variables), "id")
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
    serializers.project_payload_source(
      field,
      src_object([
        #("deletedCommentId", deleted),
        #("userErrors", serializers.user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    serializers.mutation_outcome(
      outcome,
      store,
      outcome.identity,
      "commentDelete",
      case errors {
        [] -> serializers.option_list(id)
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
      online_store_types.online_store_comment_hydrate_query,
      variables,
    )
  {
    Ok(value) ->
      serializers.json_get(value, "data")
      |> option.then(serializers.json_get(_, "comment"))
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
  serializers.json_get(value, "article")
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
  serializers.source_string_field(
    serializers.captured_to_source(record.data),
    "status",
    "",
  )
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
  let source = serializers.captured_to_source(data)
  let data =
    serializers.captured_object_insert(data, "status", CapturedString(status))
  case status {
    "PUBLISHED" -> {
      let data =
        serializers.captured_object_insert(
          data,
          "isPublished",
          CapturedBool(True),
        )
      case serializers.source_optional_string_field(source, "publishedAt") {
        Some(_) -> #(data, identity)
        None -> {
          let #(timestamp, identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          #(
            serializers.captured_object_insert(
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
      serializers.captured_object_insert(
        data,
        "isPublished",
        CapturedBool(False),
      ),
      identity,
    )
    _ -> #(data, identity)
  }
}

fn comment_not_found_user_error() -> graphql_helpers.SourceValue {
  serializers.user_error(["id"], "Comment does not exist")
}

fn comment_removed_user_error() -> graphql_helpers.SourceValue {
  serializers.user_error_with_code(
    ["id"],
    "Comment has been removed",
    "INVALID",
  )
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case serializers.json_get(value, key) {
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
  let source = serializers.input_string(args, "source")
  let errors = case source {
    Some(_) -> []
    None -> [serializers.user_error(["source"], "Source can't be blank")]
  }
  let #(record, identity, store, staged_ids) = case errors {
    [] -> {
      let #(record, identity) =
        serializers.make_integration(outcome.identity, "theme", [
          #("__typename", SrcString("OnlineStoreTheme")),
          #(
            "name",
            serializers.option_source(
              serializers.input_string(args, "name"),
              "Draft proxy theme",
            ),
          ),
          #(
            "role",
            serializers.option_source(
              serializers.input_string(args, "role"),
              "UNPUBLISHED",
            ),
          ),
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
  let id = serializers.input_string(args, "id")
  case serializers.lookup_integration_by_id(outcome.store, "theme", id) {
    serializers.IntegrationFound(existing) -> {
      let id = existing.id
      let current_role =
        serializers.source_string_field(
          serializers.captured_to_source(existing.data),
          "role",
          "",
        )
      let publish_blocked =
        root == "themePublish" && is_publish_blocked_theme_role(current_role)
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let role = case root {
        "themePublish" -> Some("MAIN")
        _ -> serializers.input_string(input, "role")
      }
      let name = serializers.input_string(input, "name")
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
              serializers.user_error(
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
            |> serializers.maybe_insert_string("name", name)
            |> serializers.maybe_insert_string("role", role)
          let record = OnlineStoreIntegrationRecord(..existing, data: data)
          let target_store = case root {
            "themePublish" -> demote_previous_main_themes(outcome.store, id)
            _ -> outcome.store
          }
          let #(_, store) =
            store.upsert_staged_online_store_integration(target_store, record)
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
    serializers.IntegrationInvalidId ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        "theme",
        None,
        [serializers.integration_invalid_id_error("theme")],
        outcome.store,
        outcome.identity,
        [],
      )
    serializers.IntegrationMissing ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        "theme",
        None,
        [serializers.integration_not_found_error("theme")],
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
    let role =
      serializers.source_string_field(
        serializers.captured_to_source(record.data),
        "role",
        "",
      )
    case record.id != published_id && role == "MAIN" {
      True -> {
        let demoted =
          OnlineStoreIntegrationRecord(
            ..record,
            data: serializers.maybe_insert_string(
              record.data,
              "role",
              Some("UNPUBLISHED"),
            ),
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
  let theme_id = case serializers.input_string(args, "themeId") {
    Some(id) -> Some(id)
    None -> serializers.input_string(args, "id")
  }
  let existing =
    serializers.option_then(theme_id, fn(id) {
      store.get_effective_online_store_integration_by_id(outcome.store, id)
    })
  let errors = case existing {
    Some(_) -> []
    None -> [serializers.user_error(["themeId"], "Theme does not exist")]
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
        #("userErrors", serializers.user_errors_source(result.errors)),
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
        #("userErrors", serializers.user_errors_source(result.errors)),
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
        #("userErrors", serializers.user_errors_source(result.errors)),
      ])
  }
  #(
    key,
    serializers.project_payload_source(field, payload, dict.new()),
    serializers.mutation_outcome(
      outcome,
      result.store,
      outcome.identity,
      root,
      [],
    ),
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
  let inputs = serializers.input_list(args, "files")
  let errors = serializers.theme_file_input_filename_errors(inputs, "filename")
  case errors {
    [] -> {
      let files = serializers.make_theme_files(inputs)
      let updated_files =
        list.fold(
          files,
          serializers.theme_record_files(theme),
          serializers.replace_theme_file,
        )
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          serializers.theme_with_files(theme, updated_files),
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
  let current_files = serializers.theme_record_files(theme)
  let inputs = serializers.input_list(args, "files")
  let errors =
    serializers.theme_file_input_filename_errors(inputs, "dstFilename")
    |> list.append(serializers.theme_file_copy_source_errors(
      inputs,
      current_files,
    ))
  case errors {
    [] -> {
      let files = serializers.make_copied_theme_files(inputs, current_files)
      let updated_files =
        list.fold(files, current_files, serializers.replace_theme_file)
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          serializers.theme_with_files(theme, updated_files),
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
  let filenames =
    serializers.input_string_values(serializers.input_list(args, "files"))
  let errors = serializers.required_theme_file_delete_errors(filenames)
  case errors {
    [] -> {
      let current_files = serializers.theme_record_files(theme)
      let deleted_files =
        list.filter(current_files, fn(file) {
          list.contains(filenames, serializers.theme_file_filename(file))
        })
      let updated_files =
        list.filter(current_files, fn(file) {
          !list.contains(filenames, serializers.theme_file_filename(file))
        })
      let #(_, store) =
        store.upsert_staged_online_store_integration(
          store_in,
          serializers.theme_with_files(theme, updated_files),
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
  let errors = script_tag_input_errors(input, True, ["input"])
  case errors {
    [] -> {
      let display_scope =
        normalized_script_tag_display_scope(
          serializers.input_string(input, "displayScope"),
          "online_store",
        )
      let #(record, identity) =
        serializers.make_integration(outcome.identity, "scriptTag", [
          #("__typename", SrcString("ScriptTag")),
          #(
            "src",
            serializers.option_source(
              serializers.input_string(input, "src"),
              "",
            ),
          ),
          #("displayScope", SrcString(display_scope)),
          #("event", SrcString("onload")),
          #(
            "cache",
            serializers.bool_source(
              serializers.input_bool(input, "cache"),
              False,
            ),
          ),
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
    _ ->
      integration_validation_error_payload(
        outcome,
        field,
        fragments,
        "scriptTagCreate",
        "scriptTag",
        errors,
      )
  }
}

fn update_script_tag(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = serializers.input_string(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case serializers.lookup_integration_by_id(outcome.store, "scriptTag", id) {
    serializers.IntegrationFound(existing) -> {
      let errors = script_tag_input_errors(input, False, [])
      case errors {
        [] -> {
          let display_scope =
            normalize_optional_script_tag_display_scope(
              serializers.input_string(input, "displayScope"),
            )
          let data =
            existing.data
            |> serializers.maybe_insert_string(
              "src",
              serializers.input_string(input, "src"),
            )
            |> serializers.maybe_insert_string("displayScope", display_scope)
            |> serializers.captured_object_insert(
              "event",
              CapturedString("onload"),
            )
            |> serializers.maybe_insert_bool(
              "cache",
              serializers.input_bool(input, "cache"),
            )
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
        _ ->
          integration_validation_error_payload(
            outcome,
            field,
            fragments,
            "scriptTagUpdate",
            "scriptTag",
            errors,
          )
      }
    }
    serializers.IntegrationInvalidId ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "scriptTagUpdate",
        "scriptTag",
        None,
        [serializers.integration_invalid_id_error("scriptTag")],
        outcome.store,
        outcome.identity,
        [],
      )
    serializers.IntegrationMissing ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "scriptTagUpdate",
        "scriptTag",
        None,
        [serializers.integration_not_found_error("scriptTag")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn script_tag_input_errors(
  input: Dict(String, root_field.ResolvedValue),
  require_src: Bool,
  field_prefix: List(String),
) -> List(graphql_helpers.SourceValue) {
  list.append(
    script_tag_src_errors(
      serializers.input_string(input, "src"),
      require_src,
      field_prefix,
    ),
    script_tag_display_scope_errors(
      serializers.input_string(input, "displayScope"),
      field_prefix,
    ),
  )
}

fn script_tag_src_errors(
  src: Option(String),
  require_src: Bool,
  field_prefix: List(String),
) -> List(graphql_helpers.SourceValue) {
  case src {
    None if require_src -> [
      script_tag_user_error(
        script_tag_field_path(field_prefix, "src"),
        "Source can't be blank",
        "BLANK",
      ),
    ]
    None -> []
    Some(value) ->
      case string.trim(value) {
        "" -> [
          script_tag_user_error(
            script_tag_field_path(field_prefix, "src"),
            "Source can't be blank",
            "BLANK",
          ),
        ]
        _ -> validate_non_blank_script_tag_src(value, field_prefix)
      }
  }
}

fn validate_non_blank_script_tag_src(
  value: String,
  field_prefix: List(String),
) -> List(graphql_helpers.SourceValue) {
  case string.length(value) > 255 {
    True -> [
      script_tag_user_error(
        script_tag_field_path(field_prefix, "src"),
        "Source is too long (maximum is 255 characters)",
        "TOO_LONG",
      ),
    ]
    False ->
      case script_tag_src_is_https_url(value) {
        True -> []
        False -> [
          script_tag_user_error(
            script_tag_field_path(field_prefix, "src"),
            "Source is invalid",
            "INVALID",
          ),
        ]
      }
  }
}

fn script_tag_src_is_https_url(value: String) -> Bool {
  case uri.parse(value) {
    Ok(uri.Uri(scheme: Some(scheme), host: Some(host), ..)) ->
      scheme == "https" && string.trim(host) != ""
    _ -> False
  }
}

fn script_tag_display_scope_errors(
  display_scope: Option(String),
  field_prefix: List(String),
) -> List(graphql_helpers.SourceValue) {
  case display_scope {
    None -> []
    Some(value) ->
      case normalize_optional_script_tag_display_scope(Some(value)) {
        Some(_) -> []
        None -> [
          script_tag_user_error(
            script_tag_field_path(field_prefix, "displayScope"),
            "Display scope is not included in the list",
            "INCLUSION",
          ),
        ]
      }
  }
}

fn script_tag_field_path(
  field_prefix: List(String),
  field: String,
) -> List(String) {
  list.append(field_prefix, [field])
}

fn normalized_script_tag_display_scope(
  display_scope: Option(String),
  default: String,
) -> String {
  normalize_optional_script_tag_display_scope(display_scope)
  |> option.unwrap(default)
}

fn normalize_optional_script_tag_display_scope(
  display_scope: Option(String),
) -> Option(String) {
  case display_scope {
    Some("ALL") | Some("all") -> Some("all")
    Some("ONLINE_STORE") | Some("online_store") -> Some("online_store")
    Some("ORDER_STATUS") | Some("order_status") -> Some("order_status")
    _ -> None
  }
}

fn script_tag_user_error(
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  serializers.integration_user_error("scriptTag", field, message, code)
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
      serializers.value_source_from_dict(
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
      serializers.same_current_app_web_pixel,
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
        [serializers.web_pixel_taken_error()],
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
      #("status", serializers.web_pixel_status_source(settings)),
    ]
    _ -> [
      #("__typename", SrcString(type_name)),
      #("settings", settings),
      #("status", SrcString("CONNECTED")),
      #("webhookEndpointAddress", SrcNull),
    ]
  }
  let #(record, identity) =
    serializers.make_integration(outcome.identity, kind, entries)
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
  let id = serializers.input_string(args, "id")
  let lookup = case id {
    Some(_) -> serializers.lookup_integration_by_id(outcome.store, kind, id)
    None ->
      case
        serializers.first_option(store.list_effective_online_store_integrations(
          outcome.store,
          kind,
        ))
      {
        Some(record) -> serializers.IntegrationFound(record)
        None -> serializers.IntegrationMissing
      }
  }
  case lookup {
    serializers.IntegrationFound(record) -> {
      let input =
        graphql_helpers.read_arg_object(args, kind)
        |> option.unwrap(dict.new())
      let prior = serializers.captured_to_source(record.data)

      case kind {
        "webPixel" -> {
          case web_pixel_update_settings(input, prior) {
            Error(error) ->
              integration_validation_error_payload(
                outcome,
                field,
                fragments,
                root,
                kind,
                [error],
              )
            Ok(settings) ->
              case web_pixel_update_validation_error(input, prior, settings) {
                Some(error) ->
                  integration_validation_error_payload(
                    outcome,
                    field,
                    fragments,
                    root,
                    kind,
                    [error],
                  )
                None -> {
                  let entries = [
                    #("settings", settings),
                    #("status", serializers.web_pixel_status_source(settings)),
                    ..web_pixel_runtime_context_entry(input)
                  ]
                  let record =
                    OnlineStoreIntegrationRecord(
                      ..record,
                      data: serializers.base_source(prior, entries)
                        |> serializers.source_to_captured,
                    )
                  let #(_, store) =
                    store.upsert_staged_online_store_integration(
                      outcome.store,
                      record,
                    )
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
              }
          }
        }
        _ -> {
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
      }
    }
    serializers.IntegrationInvalidId ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        None,
        [serializers.integration_invalid_id_error(kind)],
        outcome.store,
        outcome.identity,
        [],
      )
    serializers.IntegrationMissing ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        root,
        kind,
        None,
        [serializers.integration_not_found_error(kind)],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn web_pixel_update_settings(
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
) -> Result(graphql_helpers.SourceValue, graphql_helpers.SourceValue) {
  let current = serializers.source_field(prior, "settings", SrcNull)
  case dict.get(input, "settings") {
    Error(_) -> Ok(current)
    Ok(root_field.NullVal) -> Ok(SrcNull)
    Ok(root_field.StringVal(raw)) ->
      case json.parse(raw, commit.json_value_decoder()) {
        Ok(value) -> Ok(json_value_to_source(value))
        Error(_) ->
          Error(web_pixel_user_error(
            ["settings"],
            "Settings must be valid JSON",
            "INVALID_CONFIGURATION_JSON",
          ))
      }
    Ok(value) -> Ok(graphql_helpers.resolved_value_to_source(value))
  }
}

fn web_pixel_update_validation_error(
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
  settings: graphql_helpers.SourceValue,
) -> Option(graphql_helpers.SourceValue) {
  case web_pixel_runtime_context_error(input, prior) {
    Some(error) -> Some(error)
    None -> web_pixel_settings_error(prior, settings)
  }
}

fn web_pixel_runtime_context_entry(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, graphql_helpers.SourceValue)) {
  case serializers.input_string(input, "runtimeContext") {
    Some(value) -> [#("runtimeContext", SrcString(value))]
    None -> []
  }
}

fn web_pixel_runtime_context_error(
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
) -> Option(graphql_helpers.SourceValue) {
  case serializers.input_string(input, "runtimeContext") {
    None -> None
    Some(runtime_context) -> {
      let allowed = web_pixel_runtime_contexts(prior)
      case allowed {
        [] -> None
        _ ->
          case list.contains(allowed, runtime_context) {
            True -> None
            False ->
              Some(web_pixel_user_error(
                ["webPixel", "runtimeContext"],
                "Runtime context is invalid",
                "INVALID_RUNTIME_CONTEXT",
              ))
          }
      }
    }
  }
}

fn web_pixel_runtime_contexts(
  source: graphql_helpers.SourceValue,
) -> List(String) {
  let camel = serializers.source_string_list(source, "runtimeContexts")
  case camel {
    [] -> serializers.source_string_list(source, "runtime_contexts")
    _ -> camel
  }
}

fn web_pixel_settings_error(
  prior: graphql_helpers.SourceValue,
  settings: graphql_helpers.SourceValue,
) -> Option(graphql_helpers.SourceValue) {
  case web_pixel_settings_definition(prior) {
    Error(error) -> Some(error)
    Ok(None) -> None
    Ok(Some(definition)) ->
      case settings_definition_validates(settings, definition) {
        True -> None
        False ->
          Some(web_pixel_user_error(
            ["settings"],
            "Settings are invalid",
            "INVALID_SETTINGS",
          ))
      }
  }
}

fn web_pixel_settings_definition(
  source: graphql_helpers.SourceValue,
) -> Result(Option(graphql_helpers.SourceValue), graphql_helpers.SourceValue) {
  let definition = case
    serializers.source_field(source, "settingsDefinition", SrcNull)
  {
    SrcNull -> serializers.source_field(source, "settings_definition", SrcNull)
    value -> value
  }

  case definition {
    SrcNull -> Ok(None)
    SrcObject(_) -> Ok(Some(definition))
    _ ->
      Error(web_pixel_user_error(
        ["settings"],
        "Settings definition is invalid",
        "INVALID_SETTINGS_DEFINITION",
      ))
  }
}

fn settings_definition_validates(
  settings: graphql_helpers.SourceValue,
  definition: graphql_helpers.SourceValue,
) -> Bool {
  case settings, definition {
    SrcObject(settings_fields), SrcObject(definition_fields) ->
      dict.to_list(definition_fields)
      |> list.all(fn(entry) {
        let #(key, rules) = entry
        case dict.get(settings_fields, key) {
          Error(_) -> True
          Ok(value) -> setting_value_validates(value, rules)
        }
      })
    _, SrcObject(_) -> False
    _, _ -> True
  }
}

fn setting_value_validates(
  value: graphql_helpers.SourceValue,
  rules: graphql_helpers.SourceValue,
) -> Bool {
  setting_type_validates(value, rules)
  && setting_range_validates(value, rules)
  && setting_regex_validates(value, rules)
}

fn setting_type_validates(
  value: graphql_helpers.SourceValue,
  rules: graphql_helpers.SourceValue,
) -> Bool {
  case serializers.source_optional_string_field(rules, "type") {
    None -> True
    Some(type_) -> {
      let normalized = string.lowercase(type_)
      case normalized {
        "string" | "single_line_text_field" | "multi_line_text_field" ->
          case value {
            SrcString(_) -> True
            _ -> False
          }
        "number" | "float" | "decimal" ->
          case value {
            SrcInt(_) | SrcFloat(_) -> True
            _ -> False
          }
        "integer" ->
          case value {
            SrcInt(_) -> True
            _ -> False
          }
        "boolean" | "bool" ->
          case value {
            SrcBool(_) -> True
            _ -> False
          }
        "object" ->
          case value {
            SrcObject(_) -> True
            _ -> False
          }
        "array" | "list" ->
          case value {
            SrcList(_) -> True
            _ -> False
          }
        _ -> True
      }
    }
  }
}

fn setting_range_validates(
  value: graphql_helpers.SourceValue,
  rules: graphql_helpers.SourceValue,
) -> Bool {
  case value {
    SrcString(value) ->
      min_max_int_validates(
        string.length(value),
        source_first_int(rules, ["minLength", "min"]),
        source_first_int(rules, ["maxLength", "max"]),
      )
    SrcInt(value) ->
      min_max_float_validates(
        int.to_float(value),
        source_first_float(rules, ["minimum", "min"]),
        source_first_float(rules, ["maximum", "max"]),
      )
    SrcFloat(value) ->
      min_max_float_validates(
        value,
        source_first_float(rules, ["minimum", "min"]),
        source_first_float(rules, ["maximum", "max"]),
      )
    _ -> True
  }
}

fn min_max_int_validates(
  value: Int,
  min: Option(Int),
  max: Option(Int),
) -> Bool {
  case min {
    Some(minimum) if value < minimum -> False
    _ ->
      case max {
        Some(maximum) if value > maximum -> False
        _ -> True
      }
  }
}

fn min_max_float_validates(
  value: Float,
  min: Option(Float),
  max: Option(Float),
) -> Bool {
  case min {
    Some(minimum) if value <. minimum -> False
    _ ->
      case max {
        Some(maximum) if value >. maximum -> False
        _ -> True
      }
  }
}

fn setting_regex_validates(
  value: graphql_helpers.SourceValue,
  rules: graphql_helpers.SourceValue,
) -> Bool {
  case serializers.source_optional_string_field(rules, "regex") {
    None -> True
    Some(pattern) ->
      case value {
        SrcString(value) ->
          case regexp.from_string(pattern) {
            Ok(compiled) -> regexp.check(with: compiled, content: value)
            Error(_) -> False
          }
        _ -> False
      }
  }
}

fn source_first_int(
  source: graphql_helpers.SourceValue,
  keys: List(String),
) -> Option(Int) {
  list.find_map(keys, fn(key) {
    case serializers.source_field(source, key, SrcNull) {
      SrcInt(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn source_first_float(
  source: graphql_helpers.SourceValue,
  keys: List(String),
) -> Option(Float) {
  list.find_map(keys, fn(key) {
    case serializers.source_field(source, key, SrcNull) {
      SrcFloat(value) -> Ok(value)
      SrcInt(value) -> Ok(int.to_float(value))
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn json_value_to_source(
  value: commit.JsonValue,
) -> graphql_helpers.SourceValue {
  case value {
    commit.JsonNull -> SrcNull
    commit.JsonBool(value) -> SrcBool(value)
    commit.JsonInt(value) -> SrcInt(value)
    commit.JsonFloat(value) -> SrcFloat(value)
    commit.JsonString(value) -> SrcString(value)
    commit.JsonArray(items) -> SrcList(list.map(items, json_value_to_source))
    commit.JsonObject(fields) ->
      SrcObject(
        list.fold(fields, dict.new(), fn(acc, entry) {
          dict.insert(acc, entry.0, json_value_to_source(entry.1))
        }),
      )
  }
}

fn web_pixel_user_error(
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  serializers.integration_user_error("webPixel", field, message, code)
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
    serializers.first_option(store.list_effective_online_store_integrations(
      outcome.store,
      "serverPixel",
    ))
  let args = graphql_helpers.field_args(field, variables)
  let #(address, validation_errors) = server_pixel_endpoint_address(mode, args)
  case validation_errors {
    [_, ..] ->
      integration_validation_error_payload(
        outcome,
        field,
        fragments,
        root,
        "serverPixel",
        validation_errors,
      )
    [] ->
      case existing {
        Some(existing) -> {
          let record =
            OnlineStoreIntegrationRecord(
              ..existing,
              data: serializers.maybe_insert_string(
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
            [serializers.integration_not_found_error("serverPixel")],
            outcome.store,
            outcome.identity,
            [],
          )
      }
  }
}

fn server_pixel_endpoint_address(
  mode: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), List(graphql_helpers.SourceValue)) {
  case mode {
    "arn" -> {
      let arn = serializers.input_string(args, "arn")
      case arn {
        Some(value) ->
          case server_pixel_validation.valid_eventbridge_arn(value) {
            True -> #(Some(value), [])
            False -> #(None, [eventbridge_endpoint_error()])
          }
        _ -> #(None, [eventbridge_endpoint_error()])
      }
    }
    _ -> {
      let project = serializers.input_string(args, "pubSubProject")
      let topic = serializers.input_string(args, "pubSubTopic")
      let errors =
        list.append(
          pubsub_endpoint_blank_errors(project, "pubSubProject"),
          pubsub_endpoint_blank_errors(topic, "pubSubTopic"),
        )
      case project, topic, errors {
        Some(p), Some(t), [] -> #(Some(p <> "/" <> t), [])
        _, _, _ -> #(None, errors)
      }
    }
  }
}

fn pubsub_endpoint_blank_errors(
  value: Option(String),
  field: String,
) -> List(graphql_helpers.SourceValue) {
  case value {
    Some(value) ->
      case server_pixel_validation.non_blank(value) {
        True -> []
        False -> [pubsub_endpoint_error(field)]
      }
    _ -> [pubsub_endpoint_error(field)]
  }
}

fn eventbridge_endpoint_error() -> graphql_helpers.SourceValue {
  server_pixel_endpoint_error(
    "arn",
    "EventBridge server pixel endpoint is invalid",
    "EVENT_BRIDGE_ERROR",
  )
}

fn pubsub_endpoint_error(field: String) -> graphql_helpers.SourceValue {
  server_pixel_endpoint_error(
    field,
    "Pub/Sub server pixel endpoint is invalid",
    "PUB_SUB_ERROR",
  )
}

fn server_pixel_endpoint_error(
  field: String,
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  serializers.integration_user_error("serverPixel", [field], message, code)
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
  case serializers.input_non_blank_string(input, "title") {
    None ->
      storefront_token_create_error_payload(outcome, field, fragments, [
        serializers.user_error_with_code(
          ["input", "title"],
          "Title can't be blank",
          "BLANK",
        ),
      ])
    Some(title) ->
      case serializers.storefront_token_limit_reached(outcome.store) {
        True ->
          storefront_token_create_error_payload(outcome, field, fragments, [
            serializers.user_error_with_code(
              ["input"],
              "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit",
              "REACHED_LIMIT",
            ),
          ])
        False -> {
          let access_scopes =
            serializers.storefront_access_scope_sources(outcome.store)
          let #(record, identity) =
            serializers.make_integration(
              outcome.identity,
              "storefrontAccessToken",
              [
                #("__typename", SrcString("StorefrontAccessToken")),
                #("title", SrcString(title)),
                #("accessToken", SrcString("shpat_redacted")),
                #("accessScopes", SrcList(access_scopes)),
              ],
            )
          let raw_token =
            serializers.synthetic_storefront_access_token(record.id)
          let #(_, store) =
            store.upsert_staged_online_store_integration(outcome.store, record)
          let key = get_field_response_key(field)
          let payload =
            storefront_token_create_payload(
              field,
              fragments,
              record,
              raw_token,
              [],
            )
          #(
            key,
            payload,
            serializers.mutation_outcome(
              outcome,
              store,
              identity,
              "storefrontAccessTokenCreate",
              [record.id],
            ),
          )
        }
      }
  }
}

fn delete_storefront_token(
  outcome: MutationOutcome,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let id = serializers.input_string(input, "id")
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
          [
            serializers.user_error(
              ["id"],
              "Storefront access token does not exist",
            ),
          ],
          outcome.store,
        )
      }
    None -> #(
      SrcNull,
      [serializers.user_error(["id"], "Storefront access token does not exist")],
      outcome.store,
    )
  }
  let payload =
    serializers.project_payload_source(
      field,
      src_object([
        #("deletedStorefrontAccessTokenId", deleted),
        #("userErrors", serializers.user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    serializers.mutation_outcome(
      outcome,
      store,
      outcome.identity,
      "storefrontAccessTokenDelete",
      case errors {
        [] -> serializers.option_list(id)
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
  let raw_input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let input = mobile_platform_create_input(raw_input)
  let android = mobile_platform_branch(input, "android")
  let apple = mobile_platform_branch(input, "apple")
  case android, apple {
    Some(_), Some(_) ->
      mobile_platform_create_error_payload(outcome, field, fragments, [
        mobile_platform_requires_one_platform_error(),
      ])
    None, None ->
      mobile_platform_create_error_payload(outcome, field, fragments, [
        mobile_platform_requires_one_platform_error(),
      ])
    Some(app_input), None ->
      create_mobile_app_for_platform(
        outcome,
        field,
        fragments,
        variables,
        app_input,
        "android",
        "AndroidApplication",
        "applicationId",
      )
    None, Some(app_input) ->
      create_mobile_app_for_platform(
        outcome,
        field,
        fragments,
        variables,
        app_input,
        "apple",
        "AppleApplication",
        "appId",
      )
  }
}

fn create_mobile_app_for_platform(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  app_input: Dict(String, root_field.ResolvedValue),
  platform: String,
  typename: String,
  id_field: String,
) -> #(String, Json, MutationOutcome) {
  case serializers.input_non_blank_string(app_input, id_field) {
    None ->
      mobile_platform_create_error_payload(outcome, field, fragments, [
        mobile_platform_blank_id_error(platform, id_field),
      ])
    Some(platform_id) ->
      case mobile_platform_has_platform(outcome.store, platform) {
        True ->
          mobile_platform_create_error_payload(outcome, field, fragments, [
            mobile_platform_taken_error(platform),
          ])
        False ->
          stage_mobile_app(
            outcome,
            field,
            fragments,
            variables,
            app_input,
            platform_id,
            typename,
            id_field,
          )
      }
  }
}

fn stage_mobile_app(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  app_input: Dict(String, root_field.ResolvedValue),
  platform_id: String,
  typename: String,
  id_field: String,
) -> #(String, Json, MutationOutcome) {
  let #(record, identity) =
    serializers.make_integration(
      outcome.identity,
      "mobilePlatformApplication",
      mobile_platform_create_entries(typename, app_input, platform_id, id_field),
    )
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

fn mobile_platform_create_entries(
  typename: String,
  app_input: Dict(String, root_field.ResolvedValue),
  platform_id: String,
  id_field: String,
) -> List(#(String, graphql_helpers.SourceValue)) {
  case typename {
    "AppleApplication" -> [
      #("__typename", SrcString(typename)),
      #("applicationId", SrcNull),
      #("appId", case id_field {
        "appId" -> SrcString(platform_id)
        _ -> SrcNull
      }),
      #(
        "universalLinksEnabled",
        serializers.bool_source(
          serializers.input_bool(app_input, "universalLinksEnabled"),
          True,
        ),
      ),
      #(
        "sharedWebCredentialsEnabled",
        serializers.bool_source(
          serializers.input_bool(app_input, "sharedWebCredentialsEnabled"),
          True,
        ),
      ),
      #(
        "appClipsEnabled",
        serializers.bool_source(
          serializers.input_bool(app_input, "appClipsEnabled"),
          False,
        ),
      ),
      #(
        "appClipApplicationId",
        serializers.option_source(
          serializers.input_string(app_input, "appClipApplicationId"),
          "",
        ),
      ),
    ]
    _ -> [
      #("__typename", SrcString("AndroidApplication")),
      #("applicationId", case id_field {
        "applicationId" -> SrcString(platform_id)
        _ -> SrcNull
      }),
      #("appId", SrcNull),
      #(
        "appLinksEnabled",
        serializers.bool_source(
          serializers.input_bool(app_input, "appLinksEnabled"),
          True,
        ),
      ),
      #(
        "sha256CertFingerprints",
        serializers.value_source_from_dict(app_input, "sha256CertFingerprints"),
      ),
    ]
  }
}

fn mobile_platform_create_input(
  raw_input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case mobile_platform_branch(raw_input, "mobilePlatformApplication") {
    Some(input) -> input
    None -> raw_input
  }
}

fn mobile_platform_branch(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
  }
}

fn mobile_platform_has_object(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(_)) -> True
    _ -> False
  }
}

fn mobile_platform_has_platform(store: Store, platform: String) -> Bool {
  store.list_effective_online_store_integrations(
    store,
    "mobilePlatformApplication",
  )
  |> list.any(fn(record) {
    mobile_platform_record_platform(record) == Some(platform)
  })
}

fn mobile_platform_record_platform(
  record: OnlineStoreIntegrationRecord,
) -> Option(String) {
  let typename =
    record.data
    |> serializers.captured_to_source
    |> serializers.source_optional_string_field("__typename")
  case typename {
    Some("AndroidApplication") -> Some("android")
    Some("AppleApplication") -> Some("apple")
    _ -> None
  }
}

fn mobile_platform_create_error_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  errors: List(graphql_helpers.SourceValue),
) -> #(String, Json, MutationOutcome) {
  integration_validation_error_payload(
    outcome,
    field,
    fragments,
    "mobilePlatformApplicationCreate",
    "mobilePlatformApplication",
    errors,
  )
}

fn mobile_platform_requires_one_platform_error() -> graphql_helpers.SourceValue {
  serializers.user_error_with_code(
    ["mobilePlatformApplication"],
    "Specify either android or apple, not both.",
    "INVALID",
  )
}

fn mobile_platform_blank_id_error(
  platform: String,
  field: String,
) -> graphql_helpers.SourceValue {
  let message = case platform {
    "android" -> "Application can't be blank"
    _ -> "App can't be blank"
  }
  serializers.user_error_with_code(
    ["mobilePlatformApplication", platform, field],
    message,
    "BLANK",
  )
}

fn mobile_platform_taken_error(
  platform: String,
) -> graphql_helpers.SourceValue {
  let message = case platform {
    "android" -> "Android has already been taken"
    _ -> "Apple has already been taken"
  }
  serializers.user_error_with_code(
    ["mobilePlatformApplication", platform],
    message,
    "TAKEN",
  )
}

fn update_mobile_app(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let args = graphql_helpers.field_args(field, variables)
  let id = serializers.input_string(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    serializers.lookup_integration_by_id(
      outcome.store,
      "mobilePlatformApplication",
      id,
    )
  {
    serializers.IntegrationFound(record) -> {
      let typename = mobile_platform_record_typename(record)
      case mobile_platform_has_wrong_platform_input(input, typename) {
        True ->
          integration_validation_error_payload(
            outcome,
            field,
            fragments,
            "mobilePlatformApplicationUpdate",
            "mobilePlatformApplication",
            [
              mobile_platform_user_error(
                ["mobilePlatformApplication"],
                "Mobile platform application platform is invalid",
                "INVALID",
              ),
            ],
          )
        False -> {
          let platform_input = mobile_platform_update_payload(input, typename)
          let errors = mobile_platform_update_errors(platform_input, typename)
          case errors {
            [] -> {
              let record =
                mobile_platform_updated_record(record, platform_input, typename)
              let #(_, store) =
                store.upsert_staged_online_store_integration(
                  outcome.store,
                  record,
                )
              integration_payload_result(
                outcome,
                field,
                fragments,
                variables,
                "mobilePlatformApplicationUpdate",
                "mobilePlatformApplication",
                Some(record),
                [],
                store,
                outcome.identity,
                [record.id],
              )
            }
            _ ->
              integration_validation_error_payload(
                outcome,
                field,
                fragments,
                "mobilePlatformApplicationUpdate",
                "mobilePlatformApplication",
                errors,
              )
          }
        }
      }
    }
    serializers.IntegrationInvalidId ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "mobilePlatformApplicationUpdate",
        "mobilePlatformApplication",
        None,
        [serializers.integration_invalid_id_error("mobilePlatformApplication")],
        outcome.store,
        outcome.identity,
        [],
      )
    serializers.IntegrationMissing ->
      integration_payload_result(
        outcome,
        field,
        fragments,
        variables,
        "mobilePlatformApplicationUpdate",
        "mobilePlatformApplication",
        None,
        [serializers.integration_not_found_error("mobilePlatformApplication")],
        outcome.store,
        outcome.identity,
        [],
      )
  }
}

fn mobile_platform_record_typename(
  record: OnlineStoreIntegrationRecord,
) -> String {
  serializers.source_string_field(
    serializers.captured_to_source(record.data),
    "__typename",
    "AndroidApplication",
  )
}

fn mobile_platform_has_wrong_platform_input(
  input: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> Bool {
  case typename {
    "AppleApplication" -> mobile_platform_has_object(input, "android")
    "AndroidApplication" -> mobile_platform_has_object(input, "apple")
    _ -> False
  }
}

fn mobile_platform_update_payload(
  input: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> Dict(String, root_field.ResolvedValue) {
  let key = case typename {
    "AppleApplication" -> "apple"
    _ -> "android"
  }
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(fields)) -> fields
    _ -> input
  }
}

fn mobile_platform_update_errors(
  input: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> List(graphql_helpers.SourceValue) {
  case typename {
    "AppleApplication" ->
      mobile_platform_blank_string_errors(
        input,
        "appId",
        ["mobilePlatformApplication", "apple", "appId"],
        "App ID can't be blank",
      )
    _ ->
      mobile_platform_blank_string_errors(
        input,
        "applicationId",
        ["mobilePlatformApplication", "android", "applicationId"],
        "Application ID can't be blank",
      )
  }
}

fn mobile_platform_blank_string_errors(
  input: Dict(String, root_field.ResolvedValue),
  input_key: String,
  field: List(String),
  message: String,
) -> List(graphql_helpers.SourceValue) {
  case serializers.input_string(input, input_key) {
    Some(value) ->
      case string.trim(value) {
        "" -> [mobile_platform_user_error(field, message, "BLANK")]
        _ -> []
      }
    _ -> []
  }
}

fn mobile_platform_updated_record(
  record: OnlineStoreIntegrationRecord,
  input: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> OnlineStoreIntegrationRecord {
  let prior = serializers.captured_to_source(record.data)
  let entries = case typename {
    "AppleApplication" -> mobile_platform_updated_apple_entries(input, prior)
    _ -> mobile_platform_updated_android_entries(input, prior)
  }
  OnlineStoreIntegrationRecord(
    ..record,
    data: serializers.base_source(prior, entries)
      |> serializers.source_to_captured,
  )
}

fn mobile_platform_updated_android_entries(
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
) -> List(#(String, graphql_helpers.SourceValue)) {
  [
    #(
      "applicationId",
      serializers.value_or_default(
        input,
        "applicationId",
        serializers.source_field(prior, "applicationId", SrcNull),
      ),
    ),
    #(
      "appLinksEnabled",
      serializers.value_or_default(
        input,
        "appLinksEnabled",
        serializers.source_field(prior, "appLinksEnabled", SrcNull),
      ),
    ),
    #(
      "sha256CertFingerprints",
      serializers.value_or_default(
        input,
        "sha256CertFingerprints",
        serializers.source_field(prior, "sha256CertFingerprints", SrcNull),
      ),
    ),
  ]
}

fn mobile_platform_updated_apple_entries(
  input: Dict(String, root_field.ResolvedValue),
  prior: graphql_helpers.SourceValue,
) -> List(#(String, graphql_helpers.SourceValue)) {
  [
    #(
      "appId",
      serializers.value_or_default(
        input,
        "appId",
        serializers.source_field(prior, "appId", SrcNull),
      ),
    ),
    #(
      "universalLinksEnabled",
      serializers.value_or_default(
        input,
        "universalLinksEnabled",
        serializers.source_field(prior, "universalLinksEnabled", SrcNull),
      ),
    ),
    #(
      "sharedWebCredentialsEnabled",
      serializers.value_or_default(
        input,
        "sharedWebCredentialsEnabled",
        serializers.source_field(prior, "sharedWebCredentialsEnabled", SrcNull),
      ),
    ),
    #(
      "appClipsEnabled",
      serializers.value_or_default(
        input,
        "appClipsEnabled",
        serializers.source_field(prior, "appClipsEnabled", SrcNull),
      ),
    ),
    #(
      "appClipApplicationId",
      serializers.value_or_default(
        input,
        "appClipApplicationId",
        serializers.source_field(prior, "appClipApplicationId", SrcNull),
      ),
    ),
  ]
}

fn mobile_platform_user_error(
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  serializers.user_error_with_code(field, message, code)
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
  let id =
    serializers.input_string(graphql_helpers.field_args(field, variables), "id")
  let #(deleted, errors, store) = case
    serializers.lookup_integration_by_id(outcome.store, kind, id)
  {
    serializers.IntegrationFound(record) -> #(
      SrcString(record.id),
      [],
      store.delete_staged_online_store_integration(outcome.store, record.id),
    )
    serializers.IntegrationInvalidId -> #(
      SrcNull,
      [serializers.integration_invalid_id_error(kind)],
      outcome.store,
    )
    serializers.IntegrationMissing -> #(
      SrcNull,
      [serializers.integration_not_found_error(kind)],
      outcome.store,
    )
  }
  let payload =
    serializers.project_payload_source(
      field,
      src_object([
        #(deleted_key, deleted),
        #("userErrors", serializers.user_errors_source(errors)),
      ]),
      dict.new(),
    )
  #(
    key,
    payload,
    serializers.mutation_outcome(
      outcome,
      store,
      outcome.identity,
      root,
      case errors {
        [] -> serializers.option_list(id)
        _ -> []
      },
    ),
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
      serializers.project_integration_payload(
        record,
        field,
        fragments,
        variables,
        payload_key,
      )
    None -> json.null()
  }
  let payload =
    serializers.mutation_payload(field, fragments, payload_key, value, errors)
  #(
    key,
    payload,
    serializers.mutation_outcome(outcome, store, identity, root, staged_ids),
  )
}

fn integration_validation_error_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  root: String,
  payload_key: String,
  errors: List(graphql_helpers.SourceValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let payload =
    serializers.mutation_payload(
      field,
      fragments,
      payload_key,
      json.null(),
      errors,
    )
  #(
    key,
    payload,
    serializers.mutation_outcome_with_status(
      outcome,
      outcome.store,
      outcome.identity,
      root,
      [],
      store_types.Failed,
      Some("Rejected " <> root <> " validation in shopify-draft-proxy."),
    ),
  )
}

fn storefront_token_create_error_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  errors: List(graphql_helpers.SourceValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let payload =
    storefront_token_create_payload(
      field,
      fragments,
      empty_storefront_token_record(),
      "",
      errors,
    )
  #(
    key,
    payload,
    serializers.mutation_outcome_with_status(
      outcome,
      outcome.store,
      outcome.identity,
      "storefrontAccessTokenCreate",
      [],
      store_types.Failed,
      Some(
        "Rejected storefrontAccessTokenCreate validation in shopify-draft-proxy.",
      ),
    ),
  )
}

fn storefront_token_create_payload(
  field: Selection,
  fragments: FragmentMap,
  record: OnlineStoreIntegrationRecord,
  raw_token: String,
  errors: List(graphql_helpers.SourceValue),
) -> Json {
  let token_source = case errors {
    [] ->
      serializers.base_source(serializers.captured_to_source(record.data), [
        #("accessToken", SrcString(raw_token)),
      ])
    _ -> SrcNull
  }
  serializers.project_payload_source(
    field,
    src_object([
      #("storefrontAccessToken", token_source),
      #("shop", serializers.storefront_token_shop_source()),
      #("userErrors", serializers.user_errors_source(errors)),
    ]),
    fragments,
  )
}

fn empty_storefront_token_record() -> OnlineStoreIntegrationRecord {
  OnlineStoreIntegrationRecord(
    id: "",
    kind: "storefrontAccessToken",
    cursor: None,
    created_at: None,
    updated_at: None,
    data: CapturedNull,
  )
}

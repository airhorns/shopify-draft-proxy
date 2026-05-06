//// Shared internal online-store serializers and projection helpers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, SerializeConnectionConfig,
  SrcBool, SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_window_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/online_store/types as online_store_types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
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
pub fn script_tag_display_scope_graphql(value: String) -> String {
  case value {
    "all" | "ALL" -> "ALL"
    "order_status" | "ORDER_STATUS" -> "ORDER_STATUS"
    "online_store" | "ONLINE_STORE" -> "ONLINE_STORE"
    _ -> value
  }
}

@internal
pub fn mutation_outcome(
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
    store_types.Staged,
    Some("Locally staged " <> root <> " in shopify-draft-proxy."),
  )
}

@internal
pub fn mutation_outcome_with_status(
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

@internal
pub fn not_found_payload(
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

@internal
pub fn content_validation_error_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  root: String,
  payload_key: String,
  error: graphql_helpers.SourceValue,
) -> #(String, Json, MutationOutcome) {
  content_validation_errors_payload(
    outcome,
    field,
    fragments,
    root,
    payload_key,
    [error],
  )
}

@internal
pub fn content_validation_errors_payload(
  outcome: MutationOutcome,
  field: Selection,
  fragments: FragmentMap,
  root: String,
  payload_key: String,
  errors: List(graphql_helpers.SourceValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let payload =
    mutation_payload(field, fragments, payload_key, json.null(), errors)
  #(
    key,
    payload,
    mutation_outcome_with_status(
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

@internal
pub fn make_content(
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
    Some(value) -> value
    None -> source_string_field(prior, "body", "")
  }
  let is_published =
    option_bool(
      input_bool(input, "isPublished"),
      source_bool_field(prior, "isPublished", True),
    )
  let input_publish_date = input_string(input, "publishDate")
  let published_at = case input_publish_date {
    Some(value) -> value
    None ->
      option_string(
        source_optional_string_field(prior, "publishedAt"),
        timestamp,
      )
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
        value_or_default(
          input,
          "summary",
          source_field(prior, "summary", SrcNull),
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
        False ->
          case input_publish_date {
            Some(_) -> SrcString(published_at)
            None -> SrcNull
          }
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

@internal
pub fn make_integration(
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

@internal
pub fn mobile_platform_payload(
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

@internal
pub fn content_metafields_source(
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

@internal
pub fn owner_type_for_content(kind: String) -> Option(String) {
  case kind {
    "article" -> Some("ARTICLE")
    "blog" -> Some("BLOG")
    "page" -> Some("PAGE")
    "comment" -> Some("COMMENT")
    _ -> None
  }
}

@internal
pub fn enrich_metafields(
  value: graphql_helpers.SourceValue,
  owner_type: String,
) -> graphql_helpers.SourceValue {
  case value {
    SrcList(items) -> SrcList(list.map(items, enrich_metafield(_, owner_type)))
    _ -> value
  }
}

@internal
pub fn enrich_metafield(
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

@internal
pub fn singular_content(
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

@internal
pub fn content_connection(
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

@internal
pub fn singular_integration(
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

@internal
pub fn first_integration(
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

@internal
pub fn integration_connection(
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

@internal
pub fn filter_integration_connection_records(
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

@internal
pub fn project_content_record(
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

@internal
pub fn project_integration_record(
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

@internal
pub fn integration_projection_source(
  record: OnlineStoreIntegrationRecord,
) -> graphql_helpers.SourceValue {
  let source = captured_to_source(record.data)
  case record.kind {
    "scriptTag" ->
      base_source(source, [
        #(
          "displayScope",
          SrcString(
            script_tag_display_scope_graphql(source_string_field(
              source,
              "displayScope",
              "online_store",
            )),
          ),
        ),
        #("event", SrcString(source_string_field(source, "event", "onload"))),
      ])
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

@internal
pub fn project_content_payload(
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

@internal
pub fn project_integration_payload(
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

@internal
pub fn payload_field_selection(
  field: Selection,
  payload_key: String,
) -> Selection {
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

@internal
pub fn content_payload_source(
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

@internal
pub fn nested_content_connection(
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

@internal
pub fn root_connection_visible(
  kind: String,
  record: OnlineStoreContentRecord,
) -> Bool {
  case kind {
    "article" ->
      source_bool_field(captured_to_source(record.data), "isPublished", False)
    _ -> True
  }
}

@internal
pub fn children_for_parent(
  store: Store,
  kind: String,
  parent_id: String,
) -> List(OnlineStoreContentRecord) {
  store.list_effective_online_store_content(store, kind)
  |> list.filter(fn(record) { record.parent_id == Some(parent_id) })
}

@internal
pub fn article_authors_connection(
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

@internal
pub fn article_tags(store: Store) -> List(String) {
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

@internal
pub fn project_shop(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source =
    src_object([#("id", SrcString(online_store_types.synthetic_shop_id))])
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

@internal
pub fn filter_content_by_query(
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

@internal
pub fn matches_query(
  source: graphql_helpers.SourceValue,
  query: String,
) -> Bool {
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

@internal
pub fn mutation_payload(
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

@internal
pub fn project_payload_source(
  field: Selection,
  source: graphql_helpers.SourceValue,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, child_selections(field), fragments)
}

@internal
pub fn count_json(count: Int) -> Json {
  json.object([
    #("count", json.int(count)),
    #("precision", json.string("EXACT")),
  ])
}

@internal
pub fn content_count_json(
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

@internal
pub fn should_fetch_count_baseline(
  store_in: Store,
  kind: String,
  overlay_count: Int,
) -> Bool {
  overlay_count > 0 && base_online_store_content_count(store_in, kind) == 0
}

@internal
pub fn base_online_store_content_count(store_in: Store, kind: String) -> Int {
  dict.values(store_in.base_state.online_store_content)
  |> list.filter(fn(record) { record.kind == kind })
  |> list.length
}

@internal
pub fn new_staged_online_store_content_count(
  store_in: Store,
  kind: String,
) -> Int {
  dict.values(store_in.staged_state.online_store_content)
  |> list.filter(fn(record) {
    record.kind == kind
    && !dict.has_key(store_in.base_state.online_store_content, record.id)
  })
  |> list.length
}

@internal
pub fn fetch_upstream_content_count(
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

@internal
pub fn json_get(
  value: commit.JsonValue,
  key: String,
) -> Option(commit.JsonValue) {
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

@internal
pub fn json_int(value: commit.JsonValue) -> Option(Int) {
  case value {
    commit.JsonInt(n) -> Some(n)
    _ -> None
  }
}

@internal
pub fn count_source(count: Int) -> graphql_helpers.SourceValue {
  src_object([
    #("count", SrcInt(count)),
    #("precision", SrcString("EXACT")),
  ])
}

@internal
pub fn user_error(
  field: List(String),
  message: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
  ])
}

@internal
pub fn web_pixel_taken_error() -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("WebPixelUserError")),
    #("field", SrcNull),
    #("message", SrcString("Web pixel is taken.")),
    #("code", SrcString("TAKEN")),
  ])
}

@internal
pub type IntegrationLookup {
  IntegrationFound(OnlineStoreIntegrationRecord)
  IntegrationInvalidId
  IntegrationMissing
}

@internal
pub fn lookup_integration_by_id(
  store_in: Store,
  kind: String,
  id: Option(String),
) -> IntegrationLookup {
  case id {
    Some(id) ->
      case valid_integration_gid(kind, id) {
        False -> IntegrationInvalidId
        True ->
          case
            store.get_effective_online_store_integration_by_id(store_in, id)
          {
            Some(record) if record.kind == kind -> IntegrationFound(record)
            _ -> IntegrationMissing
          }
      }
    None -> IntegrationMissing
  }
}

@internal
pub fn valid_integration_gid(kind: String, id: String) -> Bool {
  case string.split(id, on: "/") {
    ["gid:", "", "shopify", type_name, tail] ->
      type_name == integration_gid_type(kind) && tail != ""
    _ -> False
  }
}

@internal
pub fn integration_invalid_id_error(
  kind: String,
) -> graphql_helpers.SourceValue {
  integration_user_error(kind, ["id"], "Invalid global id", "INVALID")
}

@internal
pub fn integration_not_found_error(
  kind: String,
) -> graphql_helpers.SourceValue {
  integration_user_error(
    kind,
    ["id"],
    integration_not_found_message(kind),
    "NOT_FOUND",
  )
}

@internal
pub fn integration_user_error(
  kind: String,
  field: List(String),
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  case integration_user_error_typename(kind) {
    Some(typename) ->
      src_object([
        #("__typename", SrcString(typename)),
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", SrcString(code)),
      ])
    None -> user_error_with_code(field, message, code)
  }
}

@internal
pub fn integration_user_error_typename(kind: String) -> Option(String) {
  case kind {
    "webPixel" -> Some("WebPixelUserError")
    "serverPixel" -> Some("ServerPixelUserError")
    "scriptTag" -> Some("ScriptTagUserError")
    "theme" -> Some("ThemeUserError")
    _ -> None
  }
}

@internal
pub fn integration_not_found_message(kind: String) -> String {
  case kind {
    "webPixel" -> "Pixel not found"
    "scriptTag" -> "Script tag not found"
    "theme" -> "Theme not found"
    "serverPixel" -> "Server pixel not found"
    "mobilePlatformApplication" -> "Mobile platform application not found"
    "storefrontAccessToken" -> "Storefront access token not found"
    _ -> "Integration not found"
  }
}

@internal
pub fn web_pixel_status_source(
  settings: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  case settings {
    SrcNull -> SrcString("NEEDS_CONFIGURATION")
    _ -> SrcString("CONNECTED")
  }
}

@internal
pub fn same_current_app_web_pixel(
  record: OnlineStoreIntegrationRecord,
) -> Bool {
  current_app_key(captured_to_source(record.data)) == current_app_key(SrcNull)
}

@internal
pub fn storefront_token_limit_reached(store: Store) -> Bool {
  store.list_effective_online_store_integrations(store, "storefrontAccessToken")
  |> list.length
  >= 100
}

@internal
pub fn storefront_access_scope_sources(
  store: Store,
) -> List(graphql_helpers.SourceValue) {
  storefront_access_scope_handles(store)
  |> list.map(access_scope_source)
}

@internal
pub fn storefront_access_scope_handles(store: Store) -> List(String) {
  let handles = case store.get_current_app_installation(store) {
    Some(installation) ->
      installation.access_scopes
      |> list.map(fn(scope) { scope.handle })
      |> list.filter(is_storefront_access_scope)
    None -> []
  }
  case handles {
    [] -> default_storefront_access_scope_handles()
    _ -> dedupe(handles)
  }
}

@internal
pub fn is_storefront_access_scope(handle: String) -> Bool {
  string.starts_with(handle, "unauthenticated_")
}

@internal
pub fn default_storefront_access_scope_handles() -> List(String) {
  [
    "unauthenticated_read_product_listings",
    "unauthenticated_read_product_inventory",
  ]
}

@internal
pub fn access_scope_source(handle: String) -> graphql_helpers.SourceValue {
  src_object([#("handle", SrcString(handle)), #("description", SrcNull)])
}

@internal
pub fn synthetic_storefront_access_token(id: String) -> String {
  "shpat_" <> string.slice(crypto.sha256_hex(id), 0, 16)
}

@internal
pub fn storefront_token_shop_source() -> graphql_helpers.SourceValue {
  src_object([#("id", SrcString(online_store_types.synthetic_shop_id))])
}

@internal
pub fn current_app_key(source: graphql_helpers.SourceValue) -> Option(String) {
  case source_optional_string_field(source, "apiPermission") {
    Some(value) -> Some(value)
    None ->
      case source_optional_string_field(source, "api_permission") {
        Some(value) -> Some(value)
        None -> None
      }
  }
}

@internal
pub fn user_error_with_code(
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

@internal
pub fn article_user_error(
  message: String,
  code: String,
) -> graphql_helpers.SourceValue {
  src_object([
    #("field", SrcList([SrcString("article")])),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

@internal
pub fn required_title_error(
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

@internal
pub fn user_errors_source(
  errors: List(graphql_helpers.SourceValue),
) -> graphql_helpers.SourceValue {
  SrcList(errors)
}

@internal
pub fn input_list(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(root_field.ResolvedValue) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(items)) -> items
    _ -> []
  }
}

@internal
pub fn input_string_list(
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

@internal
pub fn input_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn input_non_blank_string(
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

@internal
pub fn input_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn value_source_from_dict(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> graphql_helpers.SourceValue {
  case dict.get(args, name) {
    Ok(value) -> graphql_helpers.resolved_value_to_source(value)
    Error(_) -> SrcNull
  }
}

@internal
pub fn value_or_default(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
  default: graphql_helpers.SourceValue,
) -> graphql_helpers.SourceValue {
  case dict.get(args, name) {
    Ok(value) -> graphql_helpers.resolved_value_to_source(value)
    Error(_) -> default
  }
}

@internal
pub fn option_source(
  value: Option(String),
  default: String,
) -> graphql_helpers.SourceValue {
  SrcString(option_string(value, default))
}

@internal
pub fn bool_source(
  value: Option(Bool),
  default: Bool,
) -> graphql_helpers.SourceValue {
  SrcBool(option_bool(value, default))
}

@internal
pub fn option_string(value: Option(String), default: String) -> String {
  case value {
    Some(value) -> value
    None -> default
  }
}

@internal
pub fn option_bool(value: Option(Bool), default: Bool) -> Bool {
  case value {
    Some(value) -> value
    None -> default
  }
}

@internal
pub fn option_list(value: Option(a)) -> List(a) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

@internal
pub fn first_option(items: List(a)) -> Option(a) {
  case items {
    [first, ..] -> Some(first)
    [] -> None
  }
}

@internal
pub fn option_then(value: Option(a), fun: fn(a) -> Option(b)) -> Option(b) {
  case value {
    Some(value) -> fun(value)
    None -> None
  }
}

@internal
pub fn child_selections(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, graphql_helpers.SelectedFieldOptions(True))
}

@internal
pub fn source_field(
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

@internal
pub fn without_source_field(
  source: graphql_helpers.SourceValue,
  name: String,
) -> graphql_helpers.SourceValue {
  case source {
    SrcObject(fields) -> SrcObject(dict.delete(fields, name))
    _ -> source
  }
}

@internal
pub fn source_string_field(
  source: graphql_helpers.SourceValue,
  name: String,
  default: String,
) -> String {
  case source_field(source, name, SrcNull) {
    SrcString(value) -> value
    _ -> default
  }
}

@internal
pub fn source_optional_string_field(
  source: graphql_helpers.SourceValue,
  name: String,
) -> Option(String) {
  case source_field(source, name, SrcNull) {
    SrcString(value) -> Some(value)
    _ -> None
  }
}

@internal
pub fn source_bool_field(
  source: graphql_helpers.SourceValue,
  name: String,
  default: Bool,
) -> Bool {
  case source_field(source, name, SrcNull) {
    SrcBool(value) -> value
    _ -> default
  }
}

@internal
pub fn source_string_list(
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

@internal
pub fn nested_string(
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

@internal
pub fn maybe_insert_string(
  data: CapturedJsonValue,
  key: String,
  value: Option(String),
) -> CapturedJsonValue {
  case value {
    Some(value) -> captured_object_insert(data, key, CapturedString(value))
    None -> data
  }
}

@internal
pub fn maybe_insert_bool(
  data: CapturedJsonValue,
  key: String,
  value: Option(Bool),
) -> CapturedJsonValue {
  case value {
    Some(value) -> captured_object_insert(data, key, CapturedBool(value))
    None -> data
  }
}

@internal
pub fn captured_object_insert(
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

@internal
pub fn base_source(
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

@internal
pub fn captured_to_source(
  value: CapturedJsonValue,
) -> graphql_helpers.SourceValue {
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

@internal
pub fn source_to_captured(
  value: graphql_helpers.SourceValue,
) -> CapturedJsonValue {
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

@internal
pub fn project_first_metafield(
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

@internal
pub fn project_metafields_connection(
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

@internal
pub fn theme_files_connection(
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

@internal
pub fn make_theme_files(
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

@internal
pub fn make_copied_theme_files(
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

@internal
pub fn make_theme_file(
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

@internal
pub fn theme_record_files(
  theme: OnlineStoreIntegrationRecord,
) -> List(graphql_helpers.SourceValue) {
  case source_field(captured_to_source(theme.data), "files", SrcList([])) {
    SrcList(files) -> files
    _ -> []
  }
}

@internal
pub fn theme_with_files(
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

@internal
pub fn replace_theme_file(
  files: List(graphql_helpers.SourceValue),
  file: graphql_helpers.SourceValue,
) -> List(graphql_helpers.SourceValue) {
  let filename = theme_file_filename(file)
  list.filter(files, fn(existing) { theme_file_filename(existing) != filename })
  |> list.append([file])
}

@internal
pub fn find_theme_file(
  files: List(graphql_helpers.SourceValue),
  filename: String,
) -> Option(graphql_helpers.SourceValue) {
  files
  |> list.find(fn(file) { theme_file_filename(file) == filename })
  |> option.from_result
}

@internal
pub fn theme_file_filename(file: graphql_helpers.SourceValue) -> String {
  source_string_field(file, "filename", "")
}

@internal
pub fn theme_file_content(file: graphql_helpers.SourceValue) -> String {
  case source_field(file, "body", SrcNull) {
    SrcObject(fields) ->
      case dict.get(fields, "content") {
        Ok(SrcString(content)) -> content
        _ -> ""
      }
    _ -> ""
  }
}

@internal
pub fn theme_file_input_filename_errors(
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

@internal
pub fn theme_file_copy_source_errors(
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

@internal
pub fn required_theme_file_delete_errors(
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

@internal
pub fn input_string_values(
  values: List(root_field.ResolvedValue),
) -> List(String) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.StringVal(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

@internal
pub fn valid_theme_file_filename(filename: String) -> Bool {
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

@internal
pub fn theme_file_user_error(
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

@internal
pub fn content_gid_type(kind: String) -> String {
  case kind {
    "blog" -> "Blog"
    "page" -> "Page"
    "comment" -> "Comment"
    _ -> "Article"
  }
}

@internal
pub fn content_typename(kind: String) -> String {
  content_gid_type(kind)
}

@internal
pub fn resolve_content_handle(
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

@internal
pub fn unique_content_handle(
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

@internal
pub fn unique_content_handle_loop(
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

@internal
pub fn handle_exists_in_scope(
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

@internal
pub fn same_content_id(id: String, existing_id: Option(String)) -> Bool {
  case existing_id {
    Some(existing_id) -> id == existing_id
    None -> False
  }
}

@internal
pub fn content_record_in_handle_scope(
  record: OnlineStoreContentRecord,
  kind: String,
  parent_id: Option(String),
) -> Bool {
  case kind {
    "article" -> record.parent_id == parent_id
    _ -> True
  }
}

@internal
pub fn content_record_handle(record: OnlineStoreContentRecord) -> String {
  record.data
  |> captured_to_source
  |> source_string_field("handle", "")
}

@internal
pub fn handle_taken_error(kind: String) -> graphql_helpers.SourceValue {
  user_error_with_code(
    [kind, "handle"],
    "Handle has already been taken",
    "TAKEN",
  )
}

@internal
pub fn integration_gid_type(kind: String) -> String {
  case kind {
    "theme" -> "OnlineStoreTheme"
    "scriptTag" -> "ScriptTag"
    "webPixel" -> "WebPixel"
    "serverPixel" -> "ServerPixel"
    "storefrontAccessToken" -> "StorefrontAccessToken"
    _ -> "MobilePlatformApplication"
  }
}

@internal
pub fn slugify(title: String) -> String {
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

@internal
pub fn is_slug_char(char: String) -> Bool {
  case char {
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

@internal
pub fn trim_dashes(value: String) -> String {
  let chars = string.to_graphemes(value)
  let dropped_left = list.drop_while(chars, fn(char) { char == "-" })
  list.reverse(dropped_left)
  |> list.drop_while(fn(char) { char == "-" })
  |> list.reverse()
  |> string.join("")
}

@internal
pub fn strip_html(value: String) -> String {
  strip_html_loop(string.to_graphemes(value), False, [])
}

@internal
pub fn strip_html_loop(
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

@internal
pub fn value_after(query: String, prefix: String) -> String {
  case string.split_once(query, prefix) {
    Ok(#(_, tail)) ->
      case string.split(tail, " ") {
        [first, ..] -> first
        [] -> tail
      }
    Error(_) -> query
  }
}

@internal
pub fn unquote_query_value(value: String) -> String {
  value
  |> string.replace("\"", "")
  |> string.replace("'", "")
}

@internal
pub fn dedupe(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

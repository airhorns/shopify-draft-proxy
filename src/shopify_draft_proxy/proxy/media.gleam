//// Files API and staged-upload port of `src/proxy/media.ts`.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal, StringVal,
}
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions,
  SerializeConnectionConfig, SrcList, SrcNull, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, respond_to_query,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type FileRecord, type ProductMediaRecord, type ProductRecord, FileRecord,
  ProductMediaRecord, ProductRecord, ProductSeoRecord,
}

pub type MediaError {
  ParseFailed(root_field.RootFieldError)
}

type FilesUserError {
  FilesUserError(field: List(String), message: String, code: String)
}

const google_form_upload_parameter_names: List(String) = [
  "Content-Type",
  "success_action_status",
  "acl",
  "key",
  "x-goog-date",
  "x-goog-credential",
  "x-goog-algorithm",
  "x-goog-signature",
  "policy",
]

const google_signed_upload_parameter_names: List(String) = [
  "GoogleAccessId",
  "key",
  "policy",
  "signature",
]

const file_like_gid_types: List(String) = [
  "File",
  "MediaImage",
  "Video",
  "ExternalVideo",
  "Model3d",
  "GenericFile",
]

const inline_selected_field_options = SelectedFieldOptions(
  include_inline_fragments: True,
)

pub fn is_media_query_root(name: String) -> Bool {
  case name {
    "files" -> True
    _ -> False
  }
}

pub fn is_media_mutation_root(name: String) -> Bool {
  case name {
    "fileCreate"
    | "fileUpdate"
    | "fileDelete"
    | "fileAcknowledgeUpdateFailed"
    | "stagedUploadsCreate" -> True
    _ -> False
  }
}

pub fn handle_media_query(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, MediaError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "files" ->
              serialize_files_connection(store, field, fragments, variables)
            "fileSavedSearches" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, MediaError) {
  case handle_media_query(store, document, variables) {
    Ok(data) -> Ok(graphql_helpers.wrap_data(data))
    Error(e) -> Error(e)
  }
}

/// Uniform query entrypoint matching the dispatcher's signature.
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process(proxy.store, document, variables),
    "Failed to handle media query",
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        upstream,
      )
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let #(result, next_store, next_identity) = case name.value {
            "fileCreate" ->
              handle_file_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              )
            "fileUpdate" ->
              handle_file_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                upstream,
              )
            "fileDelete" ->
              handle_file_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                upstream,
              )
            "fileAcknowledgeUpdateFailed" ->
              handle_file_acknowledge_update_failed(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              )
            "stagedUploadsCreate" ->
              handle_staged_uploads_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              )
            _ -> #(
              MutationFieldResult(
                key: get_field_response_key(field),
                payload: json.null(),
                staged_resource_ids: [],
              ),
              current_store,
              current_identity,
            )
          }
          let draft =
            mutation_helpers.single_root_log_draft(
              name.value,
              result.staged_resource_ids,
              store.Staged,
              "media",
              "stage-locally",
              Some(
                "Locally staged " <> name.value <> " in shopify-draft-proxy.",
              ),
            )
          #(
            list.append(entries, [#(result.key, result.payload)]),
            next_store,
            next_identity,
            list.append(staged_ids, result.staged_resource_ids),
            list.append(drafts, [draft]),
          )
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: all_drafts,
  )
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

fn handle_file_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let inputs =
    graphql_helpers.field_args(field, variables)
    |> read_object_list_arg("files")
  let errors =
    inputs
    |> enumerate_objects
    |> list.flat_map(fn(entry) {
      let #(input, index) = entry
      validate_file_input(input, index)
    })
  case errors {
    [] -> {
      let #(_reserved_log_id, identity) =
        synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
      let #(_reserved_received_at, identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(created, next_identity) =
        map_with_identity(inputs, identity, fn(current_identity, input) {
          make_file_record(current_identity, input)
        })
      let next_store = store.upsert_staged_files(store, created)
      #(
        MutationFieldResult(
          key: key,
          payload: file_create_payload(created, [], field, fragments),
          staged_resource_ids: list.map(created, fn(file) { file.id }),
        ),
        next_store,
        next_identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: file_create_payload([], errors, field, fragments),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
  }
}

fn handle_file_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let inputs =
    graphql_helpers.field_args(field, variables)
    |> read_object_list_arg("files")
  // Pattern 2: `fileUpdate.referencesToAdd` validates product existence before
  // staging. In LiveHybrid, hydrate those product ids from upstream so attaching
  // an existing Shopify product stays local-only after the read; Snapshot mode
  // or a missing cassette preserves the current local validation failure.
  let store = maybe_hydrate_referenced_products(store, inputs, upstream)
  let errors =
    inputs
    |> enumerate_objects
    |> list.flat_map(fn(entry) {
      let #(input, index) = entry
      validate_file_update_input(store, input, index)
    })
  case errors {
    [] -> {
      let updated =
        inputs
        |> list.filter_map(fn(input) {
          use id <- result.try(read_string_field_result(input, "id"))
          use existing <- result.try(
            case get_effective_file_like_record(store, id) {
              Some(file) -> Ok(file)
              None -> Error(Nil)
            },
          )
          Ok(update_file_record(existing, input))
        })
      let next_store = store.upsert_staged_files(store, updated)
      let next_store =
        list.fold(inputs, next_store, fn(current, input) {
          case read_string_field(input, "id") {
            Some(id) ->
              case get_effective_file_like_record(current, id) {
                Some(file) ->
                  stage_product_media_file_update(current, file, input)
                None -> current
              }
            None -> current
          }
        })
      #(
        MutationFieldResult(
          key: key,
          payload: file_update_payload(updated, [], field, fragments),
          staged_resource_ids: list.map(updated, fn(file) { file.id }),
        ),
        next_store,
        identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: file_update_payload([], errors, field, fragments),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
  }
}

fn handle_file_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let file_ids =
    graphql_helpers.field_args(field, variables)
    |> read_string_list_arg("fileIds")
  // Pattern 2: a Files API delete can target a Product media id whose ownership
  // is only known upstream. Hydrate the owning product/media rows first, then
  // stage the delete locally so downstream Product media reads see the removal.
  let store = maybe_hydrate_file_product_media(store, file_ids, upstream)
  let resolved =
    file_ids
    |> list.map(fn(file_id) {
      #(file_id, resolve_file_delete_record(store, file_id))
    })
  let missing =
    resolved
    |> list.find_map(fn(entry) {
      let #(file_id, file) = entry
      case file {
        Some(_) -> Error(Nil)
        None -> Ok(file_id)
      }
    })
    |> option.from_result
  case missing {
    Some(file_id) -> {
      let error =
        FilesUserError(
          ["fileIds"],
          "File id " <> file_id <> " does not exist.",
          "FILE_DOES_NOT_EXIST",
        )
      #(
        MutationFieldResult(
          key: key,
          payload: file_delete_payload(SrcNull, [error], field, fragments),
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
    None -> {
      let deleted_files =
        resolved
        |> list.filter_map(fn(entry) {
          let #(_, file) = entry
          case file {
            Some(file) -> Ok(file)
            None -> Error(Nil)
          }
        })
      let deleted_file_ids =
        deleted_files
        |> list.map(fn(file) { resolved_file_gid(file) })
      let actual_file_ids =
        deleted_files
        |> list.map(fn(file) { file.id })
        |> unique_strings
      let next_store = store.delete_staged_files(store, actual_file_ids)
      #(
        MutationFieldResult(
          key: key,
          payload: file_delete_payload(
            SrcList(list.map(deleted_file_ids, SrcString)),
            [],
            field,
            fragments,
          ),
          staged_resource_ids: actual_file_ids,
        ),
        next_store,
        identity,
      )
    }
  }
}

fn handle_file_acknowledge_update_failed(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let file_ids =
    graphql_helpers.field_args(field, variables)
    |> read_string_list_arg("fileIds")
  let errors =
    file_ids
    |> list.flat_map(fn(file_id) {
      case get_effective_file_like_record(store, file_id) {
        None -> [
          FilesUserError(
            ["fileIds"],
            "File id " <> file_id <> " does not exist.",
            "FILE_DOES_NOT_EXIST",
          ),
        ]
        Some(file) ->
          case file.file_status {
            "READY" -> []
            _ -> [
              FilesUserError(
                ["fileIds"],
                "File with id " <> file_id <> " is not in the READY state.",
                "NON_READY_STATE",
              ),
            ]
          }
      }
    })
  case errors {
    [] -> {
      let acknowledged =
        list.filter_map(file_ids, fn(file_id) {
          case get_effective_file_like_record(store, file_id) {
            Some(file) -> Ok(file)
            None -> Error(Nil)
          }
        })
      #(
        MutationFieldResult(
          key: key,
          payload: file_ack_payload(
            SrcList(list.map(acknowledged, file_source)),
            [],
            field,
            fragments,
          ),
          staged_resource_ids: list.map(acknowledged, fn(file) { file.id }),
        ),
        store,
        identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: file_ack_payload(SrcNull, errors, field, fragments),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
  }
}

fn handle_staged_uploads_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let inputs =
    graphql_helpers.field_args(field, variables)
    |> read_object_list_arg("input")
  let errors =
    inputs
    |> enumerate_objects
    |> list.flat_map(fn(entry) {
      let #(input, index) = entry
      validate_staged_upload_input(input, index)
    })
  case errors {
    [] -> {
      let #(targets, next_identity) =
        map_with_identity(
          enumerate_objects(inputs),
          identity,
          fn(current_identity, entry) {
            let #(input, index) = entry
            make_staged_target(current_identity, input, index)
          },
        )
      #(
        MutationFieldResult(
          key: key,
          payload: staged_uploads_payload(targets, [], field, fragments),
          staged_resource_ids: [],
        ),
        store,
        next_identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: staged_uploads_payload([], errors, field, fragments),
        staged_resource_ids: [],
      ),
      store,
      identity,
    )
  }
}

type StagedTarget {
  StagedTarget(
    url: String,
    resource_url: String,
    parameters: List(#(String, String)),
  )
}

fn serialize_files_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let files = store.list_effective_files(store)
  let window =
    paginate_connection_items(
      files,
      field,
      variables,
      fn(file, _index) { file.id },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(file, _index) { file.id },
      serialize_node: fn(file, selection, _index) {
        project_graphql_value(
          file_source(file),
          get_selected_child_fields(selection, inline_selected_field_options),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn file_create_payload(
  files: List(FileRecord),
  errors: List(FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("FileCreatePayload")),
      #("files", SrcList(list.map(files, file_source))),
      #("userErrors", files_user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn file_update_payload(
  files: List(FileRecord),
  errors: List(FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("FileUpdatePayload")),
      #("files", SrcList(list.map(files, file_source))),
      #("userErrors", files_user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn file_delete_payload(
  deleted_file_ids: SourceValue,
  errors: List(FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("FileDeletePayload")),
      #("deletedFileIds", deleted_file_ids),
      #("userErrors", files_user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn file_ack_payload(
  files: SourceValue,
  errors: List(FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("FileAcknowledgeUpdateFailedPayload")),
      #("files", files),
      #("userErrors", files_user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn staged_uploads_payload(
  targets: List(StagedTarget),
  errors: List(FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("StagedUploadsCreatePayload")),
      #("stagedTargets", SrcList(list.map(targets, staged_target_source))),
      #("userErrors", files_user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn file_source(file: FileRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString(file_typename(file))),
    #("id", SrcString(file.id)),
    #("alt", graphql_helpers.option_string_source(file.alt)),
    #("contentType", graphql_helpers.option_string_source(file.content_type)),
    #("createdAt", SrcString(file.created_at)),
    #("fileStatus", SrcString(file.file_status)),
    #("filename", graphql_helpers.option_string_source(file.filename)),
    #("image", file_image_source(file)),
    #("preview", file_preview_source(file)),
    #("mediaErrors", SrcList([])),
    #("mediaWarnings", SrcList([])),
  ])
}

fn file_image_source(file: FileRecord) -> SourceValue {
  case file.image_url {
    Some(url) ->
      src_object([
        #("url", SrcString(url)),
        #("width", graphql_helpers.option_int_source(file.image_width)),
        #("height", graphql_helpers.option_int_source(file.image_height)),
      ])
    None -> SrcNull
  }
}

fn file_preview_source(file: FileRecord) -> SourceValue {
  src_object([#("image", file_image_source(file))])
}

fn staged_target_source(target: StagedTarget) -> SourceValue {
  src_object([
    #("url", SrcString(target.url)),
    #("resourceUrl", SrcString(target.resource_url)),
    #(
      "parameters",
      SrcList(
        list.map(target.parameters, fn(parameter) {
          let #(name, value) = parameter
          src_object([#("name", SrcString(name)), #("value", SrcString(value))])
        }),
      ),
    ),
  ])
}

fn files_user_errors_source(errors: List(FilesUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let FilesUserError(field: field, message: message, code: code) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", SrcString(code)),
      ])
    }),
  )
}

fn validate_file_input(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(FilesUserError) {
  let original_source = read_string_field(input, "originalSource")
  let alt = read_string_field(input, "alt")
  let source_errors = case original_source {
    Some(value) if value != "" ->
      case is_valid_url(value) {
        True -> []
        False -> [
          FilesUserError(
            ["files", int.to_string(index), "originalSource"],
            "Image URL is invalid",
            "INVALID",
          ),
        ]
      }
    _ -> [
      FilesUserError(
        ["files", int.to_string(index), "originalSource"],
        "Original source is required",
        "REQUIRED",
      ),
    ]
  }
  let alt_errors = case alt {
    Some(value) ->
      case string.length(value) > 512 {
        True -> [
          FilesUserError(
            ["files", int.to_string(index), "alt"],
            "The alt value exceeds the maximum limit of 512 characters.",
            "ALT_VALUE_LIMIT_EXCEEDED",
          ),
        ]
        False -> []
      }
    _ -> []
  }
  list.append(source_errors, alt_errors)
}

fn validate_file_update_input(
  store: Store,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(FilesUserError) {
  let id_errors = case read_string_field(input, "id") {
    Some(id) if id != "" ->
      case store.has_effective_file_by_id(store, id) {
        True -> []
        False -> [
          FilesUserError(
            ["files", int.to_string(index), "id"],
            "File id " <> id <> " does not exist.",
            "FILE_DOES_NOT_EXIST",
          ),
        ]
      }
    _ -> [
      FilesUserError(
        ["files", int.to_string(index), "id"],
        "File id is required",
        "REQUIRED",
      ),
    ]
  }
  let alt_errors = case read_string_field(input, "alt") {
    Some(value) ->
      case string.length(value) > 512 {
        True -> [
          FilesUserError(
            ["files", int.to_string(index), "alt"],
            "The alt value exceeds the maximum limit of 512 characters.",
            "ALT_VALUE_LIMIT_EXCEEDED",
          ),
        ]
        False -> []
      }
    _ -> []
  }
  let source_errors =
    validate_optional_url(input, index, "originalSource")
    |> list.append(validate_optional_url(input, index, "previewImageSource"))
  let conflict_errors = case
    read_string_field(input, "originalSource"),
    read_string_field(input, "previewImageSource")
  {
    Some(original), Some(preview) if original != "" && preview != "" -> [
      FilesUserError(
        ["files", int.to_string(index)],
        "Specify either originalSource or previewImageSource, not both.",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
  let reference_errors =
    list.append(
      read_string_list_field(input, "referencesToAdd"),
      read_string_list_field(input, "referencesToRemove"),
    )
    |> list.flat_map(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> []
        None -> [
          FilesUserError(
            ["files", int.to_string(index), "references"],
            "Product id " <> product_id <> " does not exist.",
            "INVALID",
          ),
        ]
      }
    })
  id_errors
  |> list.append(alt_errors)
  |> list.append(source_errors)
  |> list.append(conflict_errors)
  |> list.append(reference_errors)
}

fn validate_optional_url(
  input: Dict(String, ResolvedValue),
  index: Int,
  field_name: String,
) -> List(FilesUserError) {
  case read_string_field(input, field_name) {
    Some(value) if value != "" ->
      case is_valid_url(value) {
        True -> []
        False -> [
          FilesUserError(
            ["files", int.to_string(index), field_name],
            "Image URL is invalid",
            "INVALID",
          ),
        ]
      }
    _ -> []
  }
}

fn validate_staged_upload_input(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(FilesUserError) {
  ["filename", "mimeType", "resource"]
  |> list.flat_map(fn(field_name) {
    case read_string_field(input, field_name) {
      Some(value) if value != "" -> []
      _ -> [
        FilesUserError(
          ["input", int.to_string(index), field_name],
          field_name <> " is required",
          "REQUIRED",
        ),
      ]
    }
  })
}

fn make_file_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(FileRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    make_synthetic_file_id(identity, read_string_field(input, "contentType"))
  let #(created_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let original_source =
    read_string_field(input, "originalSource") |> option.unwrap("")
  let content_type = read_string_field(input, "contentType")
  #(
    FileRecord(
      id: id,
      alt: read_string_field(input, "alt"),
      content_type: content_type,
      created_at: created_at,
      file_status: "UPLOADED",
      filename: read_string_field(input, "filename")
        |> option.or(derive_filename(original_source)),
      original_source: original_source,
      image_url: case content_type {
        Some("IMAGE") -> Some(original_source)
        _ -> None
      },
      image_width: None,
      image_height: None,
      update_failure_acknowledged_at: None,
    ),
    next_identity,
  )
}

fn update_file_record(
  file: FileRecord,
  input: Dict(String, ResolvedValue),
) -> FileRecord {
  let original_source = read_string_field(input, "originalSource")
  let preview_source = read_string_field(input, "previewImageSource")
  let image_url =
    option.then(preview_source, non_empty_string)
    |> option.or(option.then(original_source, non_empty_string))
    |> option.or(file.image_url)
  FileRecord(
    ..file,
    alt: read_string_field(input, "alt") |> option.or(file.alt),
    file_status: "READY",
    image_url: image_url,
    original_source: original_source
      |> option.or(Some(file.original_source))
      |> option.unwrap(""),
    filename: read_string_field(input, "filename") |> option.or(file.filename),
  )
}

fn make_staged_target(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> #(StagedTarget, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity,
      "StagedUploadTarget" <> int.to_string(index),
    )
  let filename =
    read_string_field(input, "filename")
    |> option.unwrap("upload-" <> int.to_string(index))
  let mime_type =
    read_string_field(input, "mimeType")
    |> option.unwrap("application/octet-stream")
  let resource = read_string_field(input, "resource") |> option.unwrap("FILE")
  let method = read_string_field(input, "httpMethod") |> option.unwrap("POST")
  let key = "shopify-draft-proxy/" <> id <> "/" <> filename
  let parameters = case resource {
    "IMAGE" | "FILE" ->
      list.map(google_form_upload_parameter_names, fn(name) {
        case name {
          "Content-Type" -> #(name, mime_type)
          "success_action_status" -> #(name, "201")
          "acl" -> #(name, "private")
          "key" -> #(name, key)
          "x-goog-algorithm" -> #(name, "GOOG4-RSA-SHA256")
          _ -> #(name, "shopify-draft-proxy-placeholder-" <> name)
        }
      })
    "VIDEO" | "MODEL_3D" ->
      list.map(google_signed_upload_parameter_names, fn(name) {
        case name {
          "key" -> #(name, key)
          _ -> #(name, "shopify-draft-proxy-placeholder-" <> name)
        }
      })
    _ -> [
      #("key", key),
      #("Content-Type", mime_type),
      #("x-shopify-draft-proxy-resource", resource),
      #("x-shopify-draft-proxy-http-method", method),
    ]
  }
  let encoded_id = encode_upload_segment(id)
  let encoded_filename = encode_upload_segment(filename)
  #(
    StagedTarget(
      url: "https://shopify-draft-proxy.local/staged-uploads/" <> encoded_id,
      resource_url: "https://shopify-draft-proxy.local/staged-uploads/"
        <> encoded_id
        <> "/"
        <> encoded_filename,
      parameters: parameters,
    ),
    next_identity,
  )
}

fn make_synthetic_file_id(
  identity: SyntheticIdentityRegistry,
  content_type: Option(String),
) -> #(String, SyntheticIdentityRegistry) {
  case content_type {
    Some("IMAGE") ->
      synthetic_identity.make_synthetic_gid(identity, "MediaImage")
    Some("VIDEO") -> synthetic_identity.make_synthetic_gid(identity, "Video")
    Some("EXTERNAL_VIDEO") ->
      synthetic_identity.make_synthetic_gid(identity, "ExternalVideo")
    Some("MODEL_3D") ->
      synthetic_identity.make_synthetic_gid(identity, "Model3d")
    Some("FILE") ->
      synthetic_identity.make_synthetic_gid(identity, "GenericFile")
    _ -> synthetic_identity.make_synthetic_gid(identity, "File")
  }
}

fn stage_product_media_file_update(
  store: Store,
  file: FileRecord,
  input: Dict(String, ResolvedValue),
) -> Store {
  let add_ids = read_string_list_field(input, "referencesToAdd")
  let remove_ids = read_string_list_field(input, "referencesToRemove")
  let existing_product_ids =
    store.list_effective_product_media(store)
    |> list.filter_map(fn(media) {
      case media.id {
        Some(id) ->
          case id == file.id {
            True -> Ok(media.product_id)
            False -> Error(Nil)
          }
        _ -> Error(Nil)
      }
    })
  let product_ids =
    list.append(existing_product_ids, list.append(add_ids, remove_ids))
    |> dedupe_strings
  list.fold(product_ids, store, fn(current, product_id) {
    let current_media =
      store.get_effective_media_by_product_id(current, product_id)
    let without_removed =
      current_media
      |> list.filter(fn(media) {
        case media.id {
          Some(id) if id == file.id -> !list.contains(remove_ids, product_id)
          _ -> True
        }
      })
      |> list.map(fn(media) {
        case media.id {
          Some(id) if id == file.id -> product_media_from_file(media, file)
          _ -> media
        }
      })
    let next_media = case
      list.contains(add_ids, product_id)
      && !list.any(without_removed, fn(media) { media.id == Some(file.id) })
    {
      True ->
        list.append(without_removed, [
          new_product_media_from_file(
            product_id,
            file,
            list.length(without_removed),
          ),
        ])
      False -> without_removed
    }
    store.replace_staged_media_for_product(current, product_id, next_media)
  })
}

fn new_product_media_from_file(
  product_id: String,
  file: FileRecord,
  position: Int,
) -> ProductMediaRecord {
  ProductMediaRecord(
    key: product_id <> ":media:" <> int.to_string(position),
    product_id: product_id,
    position: position,
    id: Some(file.id),
    media_content_type: file_content_type_to_media_content_type(
      file.content_type,
    ),
    alt: file.alt,
    status: Some(file.file_status),
    product_image_id: None,
    image_url: file.image_url,
    image_width: file.image_width,
    image_height: file.image_height,
    preview_image_url: file.image_url,
    source_url: Some(file.original_source),
  )
}

fn product_media_from_file(
  media: ProductMediaRecord,
  file: FileRecord,
) -> ProductMediaRecord {
  ProductMediaRecord(
    ..media,
    media_content_type: file_content_type_to_media_content_type(
      file.content_type,
    ),
    alt: file.alt,
    status: Some(file.file_status),
    image_url: file.image_url,
    image_width: file.image_width,
    image_height: file.image_height,
    preview_image_url: file.image_url,
    source_url: Some(file.original_source),
  )
}

fn get_effective_file_like_record(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  case store.get_effective_file_by_id(store, file_id) {
    Some(file) -> Some(file)
    None -> get_effective_product_media_file_record(store, file_id)
  }
}

fn get_effective_product_media_file_record(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  store.list_effective_product_media(store)
  |> list.find_map(fn(media) {
    case media.id {
      Some(id) ->
        case id == file_id {
          True -> Ok(file_record_from_product_media(media))
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn resolve_file_delete_record(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  case get_effective_file_like_record(store, file_id) {
    Some(file) -> Some(file)
    None -> get_effective_file_like_record_by_gid_tail(store, file_id)
  }
}

fn get_effective_file_like_record_by_gid_tail(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  case shopify_file_gid_tail(file_id) {
    Some(requested_tail) ->
      list.append(
        store.list_effective_files(store),
        list_effective_product_media_file_records(store),
      )
      |> list.find_map(fn(file) {
        case shopify_file_gid_tail(file.id) {
          Some(tail) if tail == requested_tail -> Ok(file)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    None -> None
  }
}

fn list_effective_product_media_file_records(store: Store) -> List(FileRecord) {
  store.list_effective_product_media(store)
  |> list.filter_map(fn(media) {
    case media.id {
      Some(_) -> Ok(file_record_from_product_media(media))
      None -> Error(Nil)
    }
  })
}

fn file_record_from_product_media(media: ProductMediaRecord) -> FileRecord {
  let original_source =
    media.source_url
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
    |> option.unwrap("")
  FileRecord(
    id: media.id |> option.unwrap(""),
    alt: media.alt,
    content_type: media_content_type_to_file_content_type(
      media.media_content_type,
    ),
    created_at: "2024-01-01T00:00:00.000Z",
    file_status: media.status |> option.unwrap("READY"),
    filename: derive_filename(original_source),
    original_source: original_source,
    image_url: media.image_url |> option.or(media.preview_image_url),
    image_width: media.image_width,
    image_height: media.image_height,
    update_failure_acknowledged_at: None,
  )
}

fn file_typename(file: FileRecord) -> String {
  case file.content_type {
    Some("IMAGE") -> "MediaImage"
    Some("VIDEO") -> "Video"
    Some("EXTERNAL_VIDEO") -> "ExternalVideo"
    Some("MODEL_3D") -> "Model3d"
    Some("FILE") -> "GenericFile"
    _ -> "File"
  }
}

fn resolved_file_gid(file: FileRecord) -> String {
  case shopify_gid_tail(file.id) {
    Some(tail) -> "gid://shopify/" <> file_typename(file) <> "/" <> tail
    None -> file.id
  }
}

fn shopify_file_gid_tail(id: String) -> Option(String) {
  case shopify_gid_type(id), shopify_gid_tail(id) {
    Some(type_), Some(tail) ->
      case list.contains(file_like_gid_types, type_) {
        True -> Some(tail)
        False -> None
      }
    _, _ -> None
  }
}

fn shopify_gid_type(id: String) -> Option(String) {
  case string.split(id, "/") {
    ["gid:", "", "shopify", type_, ..] -> Some(type_)
    _ -> None
  }
}

fn shopify_gid_tail(id: String) -> Option(String) {
  let without_query = case string.split(id, "?") {
    [head, ..] -> head
    [] -> id
  }
  case list.last(string.split(without_query, "/")) {
    Ok(tail) if tail != "" -> Some(tail)
    _ -> None
  }
}

fn unique_strings(values: List(String)) -> List(String) {
  list.fold(values, [], fn(unique, value) {
    case list.contains(unique, value) {
      True -> unique
      False -> list.append(unique, [value])
    }
  })
}

fn media_content_type_to_file_content_type(
  media_content_type: Option(String),
) -> Option(String) {
  media_content_type
}

fn file_content_type_to_media_content_type(
  content_type: Option(String),
) -> Option(String) {
  content_type
}

fn maybe_hydrate_referenced_products(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
  upstream: UpstreamContext,
) -> Store {
  inputs
  |> list.flat_map(fn(input) {
    list.append(
      read_string_list_field(input, "referencesToAdd"),
      read_string_list_field(input, "referencesToRemove"),
    )
  })
  |> dedupe_strings
  |> list.fold(store, fn(current, product_id) {
    maybe_hydrate_product(current, product_id, upstream)
  })
}

fn maybe_hydrate_product(
  store: Store,
  product_id: String,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_product_by_id(store, product_id) {
    Some(_) -> store
    None -> {
      let query =
        "query MediaProductHydrate($id: ID!) {
  product(id: $id) { id title handle status }
}
"
      let variables = json.object([#("id", json.string(product_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "MediaProductHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          case product_record_from_hydrate(value, product_id) {
            Some(product) -> store.upsert_base_products(store, [product])
            None -> store
          }
        Error(_) -> store
      }
    }
  }
}

fn maybe_hydrate_file_product_media(
  store: Store,
  file_ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  let missing_file_ids =
    file_ids
    |> list.filter(fn(file_id) {
      !store.has_effective_file_by_id(store, file_id)
    })
  case missing_file_ids {
    [] -> store
    _ -> {
      let query =
        "query MediaFileReferencesHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on MediaImage {
      alt
      mediaContentType
      status
      preview { image { url width height } }
      image { url width height }
      references(first: 10) { nodes { ... on Product { id title handle status } } }
    }
  }
}
"
      let variables =
        json.object([
          #("fileIds", json.array(missing_file_ids, json.string)),
        ])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "MediaFileReferencesHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          hydrate_file_product_media_from_response(
            store,
            value,
            missing_file_ids,
          )
        Error(_) -> store
      }
    }
  }
}

fn hydrate_file_product_media_from_response(
  store: Store,
  value: commit.JsonValue,
  file_ids: List(String),
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      json_array(json_get(data, "nodes"))
      |> list.fold(store, fn(current, node) {
        case json_get_string(node, "id") {
          Some(file_id) ->
            case list.contains(file_ids, file_id) {
              True -> hydrate_file_product_media_node(current, node, file_id)
              False -> current
            }
          _ -> current
        }
      })
    None -> store
  }
}

fn hydrate_file_product_media_node(
  store: Store,
  node: commit.JsonValue,
  file_id: String,
) -> Store {
  referenced_product_nodes(node)
  |> list.fold(store, fn(current, product_node) {
    case product_record_from_node(product_node, None) {
      Some(product) -> {
        let current = store.upsert_base_products(current, [product])
        let existing_media =
          store.get_effective_media_by_product_id(current, product.id)
        case product_media_list_has_file(existing_media, file_id) {
          True -> current
          False -> {
            let media =
              product_media_record_from_node(
                product.id,
                file_id,
                node,
                list.length(existing_media),
              )
            store.replace_base_media_for_product(
              current,
              product.id,
              list.append(existing_media, [media]),
            )
          }
        }
      }
      None -> current
    }
  })
}

fn referenced_product_nodes(node: commit.JsonValue) -> List(commit.JsonValue) {
  case json_get(node, "references") {
    Some(references) -> json_array(json_get(references, "nodes"))
    None -> []
  }
}

fn product_media_list_has_file(
  media: List(ProductMediaRecord),
  file_id: String,
) -> Bool {
  list.any(media, fn(record) { record.id == Some(file_id) })
}

fn product_media_record_from_node(
  product_id: String,
  file_id: String,
  node: commit.JsonValue,
  position: Int,
) -> ProductMediaRecord {
  let image = image_value(node)
  ProductMediaRecord(
    key: product_id <> ":media:" <> int.to_string(position),
    product_id: product_id,
    position: position,
    id: Some(file_id),
    media_content_type: json_get_string(node, "mediaContentType")
      |> option.or(Some("IMAGE")),
    alt: json_get_string(node, "alt"),
    status: json_get_string(node, "status") |> option.or(Some("READY")),
    product_image_id: None,
    image_url: image |> option.then(fn(value) { json_get_string(value, "url") }),
    image_width: image
      |> option.then(fn(value) { json_get_int(value, "width") }),
    image_height: image
      |> option.then(fn(value) { json_get_int(value, "height") }),
    preview_image_url: preview_image_value(node)
      |> option.then(fn(value) { json_get_string(value, "url") }),
    source_url: image
      |> option.then(fn(value) { json_get_string(value, "url") })
      |> option.or(
        preview_image_value(node)
        |> option.then(fn(value) { json_get_string(value, "url") }),
      ),
  )
}

fn product_record_from_hydrate(
  value: commit.JsonValue,
  fallback_id: String,
) -> Option(ProductRecord) {
  case json_get(value, "data") {
    Some(data) ->
      case non_null_node(json_get(data, "product")) {
        Some(product) -> product_record_from_node(product, Some(fallback_id))
        None -> None
      }
    None -> None
  }
}

fn product_record_from_node(
  node: commit.JsonValue,
  fallback_id: Option(String),
) -> Option(ProductRecord) {
  case json_get_string(node, "id") |> option.or(fallback_id) {
    Some(id) -> {
      let title = json_get_string(node, "title") |> option.unwrap("Product")
      Some(ProductRecord(
        id: id,
        legacy_resource_id: None,
        title: title,
        handle: json_get_string(node, "handle") |> option.unwrap("product"),
        status: json_get_string(node, "status") |> option.unwrap("ACTIVE"),
        vendor: None,
        product_type: None,
        tags: [],
        total_inventory: Some(0),
        tracks_inventory: Some(False),
        created_at: None,
        updated_at: None,
        published_at: None,
        description_html: "",
        online_store_preview_url: None,
        template_suffix: None,
        seo: ProductSeoRecord(title: None, description: None),
        category: None,
        publication_ids: [],
        contextual_pricing: None,
        cursor: None,
      ))
    }
    None -> None
  }
}

fn image_value(node: commit.JsonValue) -> Option(commit.JsonValue) {
  non_null_node(json_get(node, "image"))
  |> option.or(preview_image_value(node))
}

fn preview_image_value(node: commit.JsonValue) -> Option(commit.JsonValue) {
  case json_get(node, "preview") {
    Some(preview) -> non_null_node(json_get(preview, "image"))
    None -> None
  }
}

fn non_null_node(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(node) -> Some(node)
    None -> None
  }
}

fn json_array(value: Option(commit.JsonValue)) -> List(commit.JsonValue) {
  case value {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    _ -> None
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

fn read_object_list_arg(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, name) {
    Ok(ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_string_list_arg(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(args, name) {
    Ok(ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_string_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_string_field_result(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Result(String, Nil) {
  case read_string_field(input, name) {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

fn read_string_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(input, name) {
    Ok(ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn is_valid_url(value: String) -> Bool {
  string.starts_with(value, "https://") || string.starts_with(value, "http://")
}

fn non_empty_string(value: String) -> Option(String) {
  case value {
    "" -> None
    _ -> Some(value)
  }
}

fn derive_filename(url: String) -> Option(String) {
  let parts = string.split(url, "/")
  parts
  |> reverse_strings
  |> list.find(fn(part) { part != "" })
  |> option.from_result
}

fn encode_upload_segment(value: String) -> String {
  value
  |> string.replace(":", "%3A")
  |> string.replace("/", "%2F")
}

fn enumerate_objects(
  items: List(Dict(String, ResolvedValue)),
) -> List(#(Dict(String, ResolvedValue), Int)) {
  enumerate_objects_loop(items, 0, [])
}

fn enumerate_objects_loop(
  items: List(Dict(String, ResolvedValue)),
  index: Int,
  acc: List(#(Dict(String, ResolvedValue), Int)),
) -> List(#(Dict(String, ResolvedValue), Int)) {
  case items {
    [] -> reverse_object_entries(acc)
    [item, ..rest] ->
      enumerate_objects_loop(rest, index + 1, [#(item, index), ..acc])
  }
}

fn reverse_object_entries(
  entries: List(#(Dict(String, ResolvedValue), Int)),
) -> List(#(Dict(String, ResolvedValue), Int)) {
  list.fold(entries, [], fn(acc, item) { [item, ..acc] })
}

fn reverse_strings(items: List(String)) -> List(String) {
  list.fold(items, [], fn(acc, item) { [item, ..acc] })
}

fn dedupe_strings(items: List(String)) -> List(String) {
  dedupe_strings_loop(items, [])
}

fn dedupe_strings_loop(
  items: List(String),
  seen: List(String),
) -> List(String) {
  case items {
    [] -> reverse_strings(seen)
    [item, ..rest] ->
      case list.contains(seen, item) {
        True -> dedupe_strings_loop(rest, seen)
        False -> dedupe_strings_loop(rest, [item, ..seen])
      }
  }
}

fn map_with_identity(
  items: List(a),
  identity: SyntheticIdentityRegistry,
  f: fn(SyntheticIdentityRegistry, a) -> #(b, SyntheticIdentityRegistry),
) -> #(List(b), SyntheticIdentityRegistry) {
  let #(mapped_reversed, final_identity) =
    list.fold(items, #([], identity), fn(acc, item) {
      let #(mapped, current_identity) = acc
      let #(next_item, next_identity) = f(current_identity, item)
      #([next_item, ..mapped], next_identity)
    })
  #(reverse_items(mapped_reversed), final_identity)
}

fn reverse_items(items: List(a)) -> List(a) {
  list.fold(items, [], fn(acc, item) { [item, ..acc] })
}

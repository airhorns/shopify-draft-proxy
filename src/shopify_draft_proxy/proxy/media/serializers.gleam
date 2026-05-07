//// Files API GraphQL source and payload serializers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions,
  SerializeConnectionConfig, SrcList, SrcNull, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/media/types as media_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{type FileRecord}

const inline_selected_field_options = SelectedFieldOptions(
  include_inline_fragments: True,
)

@internal
pub fn serialize_files_connection(
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

@internal
pub fn file_create_payload(
  files: List(FileRecord),
  errors: List(media_types.FilesUserError),
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

@internal
pub fn file_update_payload(
  files: List(FileRecord),
  errors: List(media_types.FilesUserError),
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

@internal
pub fn file_delete_payload(
  deleted_file_ids: SourceValue,
  errors: List(media_types.FilesUserError),
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

@internal
pub fn file_ack_payload(
  files: SourceValue,
  errors: List(media_types.FilesUserError),
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

@internal
pub fn staged_uploads_payload(
  targets: List(media_types.StagedTarget),
  errors: List(media_types.FilesUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("StagedUploadsCreatePayload")),
      #("stagedTargets", SrcList(list.map(targets, staged_target_source))),
      #("userErrors", user_errors_source(errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn file_typename(file: FileRecord) -> String {
  case file.content_type {
    Some("IMAGE") -> "MediaImage"
    Some("VIDEO") -> "Video"
    Some("EXTERNAL_VIDEO") -> "ExternalVideo"
    Some("MODEL_3D") -> "Model3d"
    Some("FILE") -> "GenericFile"
    _ -> "File"
  }
}

@internal
pub fn file_source(file: FileRecord) -> SourceValue {
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
  src_object([#("image", file_preview_image_source(file))])
}

fn file_preview_image_source(file: FileRecord) -> SourceValue {
  case file.preview_image_url {
    Some(url) ->
      src_object([
        #("url", SrcString(url)),
        #("width", graphql_helpers.option_int_source(file.preview_image_width)),
        #(
          "height",
          graphql_helpers.option_int_source(file.preview_image_height),
        ),
      ])
    None -> SrcNull
  }
}

fn staged_target_source(target: media_types.StagedTarget) -> SourceValue {
  src_object([
    #("url", graphql_helpers.option_string_source(target.url)),
    #("resourceUrl", graphql_helpers.option_string_source(target.resource_url)),
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

fn files_user_errors_source(
  errors: List(media_types.FilesUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let media_types.FilesUserError(field: field, message: message, code: code) =
        error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", SrcString(code)),
      ])
    }),
  )
}

fn user_errors_source(errors: List(media_types.FilesUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let media_types.FilesUserError(field: field, message: message, ..) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
      ])
    }),
  )
}

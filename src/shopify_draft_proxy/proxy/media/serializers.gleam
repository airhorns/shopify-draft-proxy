//// Files API GraphQL source and payload serializers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcList,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  field_raw_selections, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/media/types as media_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{type FileRecord}

@internal
pub fn serialize_files_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let files = case
    graphql_helpers.field_args(field, variables)
    |> read_bool_arg("reverse")
  {
    Some(True) -> store.list_effective_files(store) |> list.reverse
    _ -> store.list_effective_files(store)
  }
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
          field_raw_selections(selection),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn read_bool_arg(
  args: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
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
  let fields = [
    #("__typename", SrcString(file_typename(file))),
    #("id", SrcString(file.id)),
    #("alt", graphql_helpers.option_string_source(file.alt)),
    #("contentType", graphql_helpers.option_string_source(file.content_type)),
    #("createdAt", SrcString(file.created_at)),
    #("updatedAt", SrcString(file.updated_at)),
    #("fileStatus", SrcString(file.file_status)),
    #("filename", graphql_helpers.option_string_source(file.filename)),
    #("displayName", SrcString(file_display_name(file))),
    #("updateStatus", SrcString(file.file_status)),
    #("fileErrors", SrcList([])),
    #("fileWarnings", SrcList([])),
  ]
  let fields = list.append(fields, file_mime_type_fields(file))
  let fields = list.append(fields, generic_file_url_fields(file))
  let fields =
    list.append(fields, [
      #("image", file_image_source(file)),
      #("preview", file_preview_source(file)),
    ])
  let fields = list.append(fields, media_error_fields(file))
  src_object(fields)
}

fn file_display_name(file: FileRecord) -> String {
  case non_empty_option(file.filename) {
    Some(filename) -> filename
    None ->
      case derive_filename(file.original_source) {
        Some(filename) -> filename
        None ->
          case non_empty_option(file.alt) {
            Some(alt) -> alt
            None -> shopify_gid_tail(file.id) |> option.unwrap(file.id)
          }
      }
  }
}

fn file_mime_type_fields(file: FileRecord) -> List(#(String, SourceValue)) {
  case file_typename(file) {
    "MediaImage" | "Video" | "GenericFile" -> [
      #("mimeType", SrcString(derive_mime_type(file))),
    ]
    _ -> []
  }
}

fn media_error_fields(file: FileRecord) -> List(#(String, SourceValue)) {
  case file_typename(file) {
    "MediaImage" | "Video" | "ExternalVideo" | "Model3d" -> [
      #("mediaErrors", SrcList([])),
      #("mediaWarnings", SrcList([])),
    ]
    _ -> []
  }
}

fn derive_mime_type(file: FileRecord) -> String {
  let source =
    non_empty_option(file.filename)
    |> option.or(derive_filename(file.original_source))
    |> option.unwrap(file.original_source)
  let extension = file_extension(source)
  case extension {
    "gif" -> "image/gif"
    "heic" -> "image/heic"
    "heif" -> "image/heif"
    "jpg" | "jpeg" -> "image/jpeg"
    "png" -> "image/png"
    "webp" -> "image/webp"
    "m4v" -> "video/x-m4v"
    "mov" -> "video/quicktime"
    "mp4" -> "video/mp4"
    "webm" -> "video/webm"
    "glb" -> "model/gltf-binary"
    "gltf" -> "model/gltf+json"
    "usdz" -> "model/vnd.usdz+zip"
    "csv" -> "text/csv"
    "json" -> "application/json"
    "pdf" -> "application/pdf"
    "txt" -> "text/plain"
    "zip" -> "application/zip"
    _ ->
      case file.content_type {
        Some("IMAGE") -> "image/jpeg"
        Some("VIDEO") -> "video/mp4"
        Some("MODEL_3D") -> "model/gltf-binary"
        _ -> "application/octet-stream"
      }
  }
}

fn derive_filename(source: String) -> Option(String) {
  let path =
    source
    |> before_delimiter("?")
    |> before_delimiter("#")
  let last_segment =
    path
    |> string.split("/")
    |> reverse_items
    |> list.find(fn(part) { part != "" })
    |> option.from_result
  non_empty_option(last_segment)
}

fn file_extension(source: String) -> String {
  let filename = derive_filename(source) |> option.unwrap("")
  case string.split(filename, ".") {
    [_] -> ""
    parts ->
      case list.last(parts) {
        Ok(extension) -> string.lowercase(extension)
        Error(_) -> ""
      }
  }
}

fn before_delimiter(value: String, delimiter: String) -> String {
  case string.split_once(value, on: delimiter) {
    Ok(#(before, _)) -> before
    Error(_) -> value
  }
}

fn non_empty_option(value: Option(String)) -> Option(String) {
  case value {
    Some(value) if value != "" -> Some(value)
    _ -> None
  }
}

fn shopify_gid_tail(id: String) -> Option(String) {
  case list.last(string.split(id, "/")) {
    Ok(tail) if tail != "" -> Some(tail)
    _ -> None
  }
}

fn reverse_items(items: List(a)) -> List(a) {
  list.fold(items, [], fn(acc, item) { [item, ..acc] })
}

fn generic_file_url_fields(file: FileRecord) -> List(#(String, SourceValue)) {
  case file.content_type, file.original_source {
    Some("FILE"), "" -> [#("url", SrcNull)]
    Some("FILE"), url -> [#("url", SrcString(url))]
    _, _ -> []
  }
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

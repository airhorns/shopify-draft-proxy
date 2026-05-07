//// Files API mutation handling.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal, StringVal,
}
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcNull, SrcString, get_document_fragments,
  get_field_response_key,
}
import shopify_draft_proxy/proxy/media/serializers
import shopify_draft_proxy/proxy/media/types as media_types
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, MutationFieldResult,
  MutationOutcome,
}
import shopify_draft_proxy/proxy/products/hydration as product_hydration
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type FileRecord, type ProductMediaRecord, type ProductRecord,
  type ProductVariantRecord, FileRecord, ProductMediaRecord, ProductRecord,
  ProductSeoRecord,
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

const google_put_upload_parameter_names: List(String) = [
  "content_type",
  "acl",
]

const staged_upload_resource_values: List(String) = [
  "COLLECTION_IMAGE",
  "FILE",
  "IMAGE",
  "MODEL_3D",
  "PRODUCT_IMAGE",
  "SHOP_IMAGE",
  "VIDEO",
  "BULK_MUTATION_VARIABLES",
  "RETURN_LABEL",
  "URL_REDIRECT_IMPORT",
  "DISPUTE_FILE_UPLOAD",
]

const staged_upload_image_mime_types: List(String) = [
  "image/png",
  "image/jpeg",
  "image/jpg",
  "image/gif",
  "image/webp",
  "image/heic",
  "image/heif",
]

const staged_upload_video_mime_types: List(String) = [
  "video/mp4",
  "video/quicktime",
  "video/webm",
  "video/x-m4v",
]

const staged_upload_model_mime_types: List(String) = [
  "model/gltf-binary",
  "model/gltf+json",
  "model/vnd.usdz+zip",
  "application/octet-stream",
]

const file_like_gid_types: List(String) = [
  "File",
  "MediaImage",
  "Video",
  "ExternalVideo",
  "Model3d",
  "GenericFile",
]

type FileVersionEvidence {
  FileVersionEvidence(id: String, file_id: String)
}

type FileVersionHydration {
  FileVersionHydration(
    validation_enabled: Bool,
    evidence: List(FileVersionEvidence),
  )
}

@internal
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

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
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
              store_types.Staged,
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
          payload: serializers.file_create_payload(
            created,
            [],
            field,
            fragments,
          ),
          staged_resource_ids: list.map(created, fn(file) { file.id }),
        ),
        next_store,
        next_identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: serializers.file_create_payload([], errors, field, fragments),
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
  let store = maybe_hydrate_file_update_targets(store, inputs, upstream)
  let version_hydration =
    maybe_hydrate_file_update_revert_versions(inputs, upstream)
  let errors =
    validate_file_update_inputs(store, inputs)
    |> list.append(validate_file_update_revert_versions(
      inputs,
      version_hydration,
    ))
    |> list.append(validate_file_update_reference_targets(store, inputs))
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
          payload: serializers.file_update_payload(
            updated,
            [],
            field,
            fragments,
          ),
          staged_resource_ids: list.map(updated, fn(file) { file.id }),
        ),
        next_store,
        identity,
      )
    }
    _ -> #(
      MutationFieldResult(
        key: key,
        payload: serializers.file_update_payload([], errors, field, fragments),
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
  let missing_file_ids =
    resolved
    |> list.filter_map(fn(entry) {
      let #(file_id, file) = entry
      case file {
        Some(_) -> Error(Nil)
        None -> Ok(file_id)
      }
    })
    |> dedupe_strings
  case missing_file_ids {
    [_, ..] -> {
      #(
        MutationFieldResult(
          key: key,
          payload: serializers.file_delete_payload(
            SrcNull,
            file_does_not_exist_count_errors(["fileIds"], missing_file_ids),
            field,
            fragments,
          ),
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
    [] -> {
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
          payload: serializers.file_delete_payload(
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
  let missing_file_ids =
    file_ids
    |> list.filter_map(fn(file_id) {
      case get_effective_file_like_record(store, file_id) {
        None -> Ok(file_id)
        Some(_) -> Error(Nil)
      }
    })
    |> dedupe_strings
  let errors = case missing_file_ids {
    [] -> {
      let non_ready_file_ids =
        file_ids
        |> list.filter_map(fn(file_id) {
          case get_effective_file_like_record(store, file_id) {
            Some(file) ->
              case file.file_status {
                "READY" -> Error(Nil)
                _ -> Ok(file_id)
              }
            None -> Error(Nil)
          }
        })
        |> dedupe_strings
      file_non_ready_state_count_errors(["fileIds"], non_ready_file_ids)
    }
    _ -> file_does_not_exist_count_errors(["fileIds"], missing_file_ids)
  }
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
          payload: serializers.file_ack_payload(
            SrcList(list.map(acknowledged, serializers.file_source)),
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
        payload: serializers.file_ack_payload(SrcNull, errors, field, fragments),
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
  let #(targets, next_identity) =
    map_with_identity(
      enumerate_objects(inputs),
      identity,
      fn(current_identity, entry) {
        let #(input, index) = entry
        case validate_staged_upload_input(input, index) {
          [] -> make_staged_target(current_identity, input, index)
          _ -> #(empty_staged_target(), current_identity)
        }
      },
    )
  #(
    MutationFieldResult(
      key: key,
      payload: serializers.staged_uploads_payload(
        targets,
        errors,
        field,
        fragments,
      ),
      staged_resource_ids: [],
    ),
    store,
    next_identity,
  )
}

fn validate_file_input(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  let original_source = read_string_field(input, "originalSource")
  let source_errors = case original_source {
    Some(value) -> validate_original_source(value, index)
    None -> [
      media_types.FilesUserError(
        ["files", int.to_string(index), "originalSource"],
        "Original source is required",
        "REQUIRED",
      ),
    ]
  }
  source_errors
  |> list.append(validate_references_to_add(input, index))
  |> list.append(validate_create_filename_extension(input, index))
  |> list.append(validate_duplicate_resolution_mode(input, index))
}

fn validate_original_source(
  value: String,
  index: Int,
) -> List(media_types.FilesUserError) {
  case value {
    "" -> [
      media_types.FilesUserError(
        ["files", int.to_string(index), "originalSource"],
        "originalSource is too short (minimum is 1)",
        "INVALID",
      ),
    ]
    _ ->
      case string.length(value) > 2048 {
        True -> [
          media_types.FilesUserError(
            ["files", int.to_string(index), "originalSource"],
            "originalSource is too long (maximum is 2048)",
            "INVALID",
          ),
        ]
        False -> validate_original_source_url(value, index)
      }
  }
}

fn validate_original_source_url(
  value: String,
  index: Int,
) -> List(media_types.FilesUserError) {
  case is_valid_url(value) {
    True -> []
    False -> [
      media_types.FilesUserError(
        ["files", int.to_string(index), "originalSource"],
        "File URL is invalid",
        case has_non_http_uri_scheme(value) {
          True -> "INVALID_IMAGE_SOURCE_URL"
          False -> "INVALID"
        },
      ),
    ]
  }
}

fn validate_references_to_add(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case list.length(read_string_list_field(input, "referencesToAdd")) > 1 {
    True -> [
      media_types.FilesUserError(
        ["files", int.to_string(index), "referencesToAdd"],
        "Too many product ids specified.",
        "TOO_MANY_PRODUCT_IDS_SPECIFIED",
      ),
    ]
    False -> []
  }
}

fn validate_create_filename_extension(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case
    read_string_field(input, "originalSource"),
    read_string_field(input, "filename")
  {
    Some(source), Some(filename) if source != "" && filename != "" -> {
      let source_extension = file_extension(source)
      let filename_extension = file_extension(filename)
      case source_extension != filename_extension {
        True -> [
          media_types.FilesUserError(
            ["files", int.to_string(index), "filename"],
            "Provided filename extension must match original source.",
            "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE",
          ),
        ]
        False -> []
      }
    }
    _, _ -> []
  }
}

fn validate_duplicate_resolution_mode(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, "duplicateResolutionMode") {
    Some("REPLACE" as mode) ->
      validate_non_append_duplicate_mode(input, index, mode)
    Some("RAISE_ERROR" as mode) ->
      validate_non_append_duplicate_mode(input, index, mode)
    _ -> []
  }
}

fn validate_non_append_duplicate_mode(
  input: Dict(String, ResolvedValue),
  index: Int,
  mode: String,
) -> List(media_types.FilesUserError) {
  let content_type = read_string_field(input, "contentType")
  case duplicate_mode_allowed(mode, content_type) {
    False -> [
      media_types.FilesUserError(
        ["files", int.to_string(index), "duplicateResolutionMode"],
        "Duplicate resolution mode '"
          <> mode
          <> "' is not supported for '"
          <> duplicate_media_type_name(content_type)
          <> "' media type.",
        "INVALID_DUPLICATE_MODE_FOR_TYPE",
      ),
    ]
    True ->
      case mode, read_string_field(input, "filename") {
        "REPLACE", Some(filename) if filename != "" -> []
        "REPLACE", _ -> [
          media_types.FilesUserError(
            ["files", int.to_string(index), "filename"],
            "Missing filename argument when attempting to use REPLACE duplicate mode.",
            "MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE",
          ),
        ]
        _, _ -> []
      }
  }
}

fn duplicate_mode_allowed(mode: String, content_type: Option(String)) -> Bool {
  case mode, content_type {
    "REPLACE", Some("IMAGE") -> True
    "RAISE_ERROR", Some("IMAGE") -> True
    "RAISE_ERROR", Some("FILE") -> True
    _, _ -> False
  }
}

fn duplicate_media_type_name(content_type: Option(String)) -> String {
  case content_type {
    Some("FILE") -> "GENERIC_FILE"
    Some(value) -> value
    None -> "MISSING"
  }
}

fn validate_file_update_inputs(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
) -> List(media_types.FilesUserError) {
  let #(field_errors, missing_file_ids, non_ready_file_ids, target_errors) =
    inputs
    |> enumerate_objects
    |> list.fold(#([], [], [], []), fn(acc, entry) {
      let #(field_errors, missing_file_ids, non_ready_file_ids, target_errors) =
        acc
      let #(input, index) = entry
      let input_field_errors = validate_file_update_input_fields(input, index)
      case input_field_errors {
        [] -> {
          let #(missing_ids, non_ready_ids, input_target_errors) =
            validate_file_update_target(store, input, index)
          #(
            field_errors,
            list.append(missing_file_ids, missing_ids),
            list.append(non_ready_file_ids, non_ready_ids),
            list.append(target_errors, input_target_errors),
          )
        }
        _ -> #(
          list.append(field_errors, input_field_errors),
          missing_file_ids,
          non_ready_file_ids,
          target_errors,
        )
      }
    })
  field_errors
  |> list.append(
    file_update_does_not_exist_count_errors(dedupe_strings(missing_file_ids)),
  )
  |> list.append(
    file_update_non_ready_errors(dedupe_strings(non_ready_file_ids)),
  )
  |> list.append(target_errors)
}

fn validate_file_update_input_fields(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  let id_errors = case read_string_field(input, "id") {
    Some(id) if id != "" -> []
    _ -> [
      media_types.FilesUserError(
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
          media_types.FilesUserError(
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
      media_types.FilesUserError(
        ["files", int.to_string(index)],
        "Specify either originalSource or previewImageSource, not both.",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
  let source_version_errors = case
    has_source_update(input),
    read_string_field(input, "revertToVersionId")
  {
    True, Some(version_id) if version_id != "" -> [
      media_types.FilesUserError(
        ["files", int.to_string(index)],
        "Specify either a source or revertToVersionId, not both.",
        "CANNOT_SPECIFY_SOURCE_AND_VERSION_ID",
      ),
    ]
    _, _ -> []
  }
  let field_errors =
    id_errors
    |> list.append(alt_errors)
    |> list.append(source_errors)
    |> list.append(conflict_errors)
    |> list.append(source_version_errors)
  field_errors
}

fn validate_file_update_revert_versions(
  inputs: List(Dict(String, ResolvedValue)),
  hydration: FileVersionHydration,
) -> List(media_types.FilesUserError) {
  case hydration.validation_enabled {
    False -> []
    True -> {
      let missing_version_ids =
        inputs
        |> list.filter_map(fn(input) {
          case
            read_string_field(input, "id"),
            has_source_update(input),
            read_string_field(input, "revertToVersionId")
          {
            Some(file_id), False, Some(version_id)
              if file_id != "" && version_id != ""
            ->
              case
                file_version_evidence_matches(
                  hydration.evidence,
                  version_id,
                  file_id,
                )
              {
                True -> Error(Nil)
                False -> Ok(version_id)
              }
            _, _, _ -> Error(Nil)
          }
        })
        |> dedupe_strings
      case missing_version_ids {
        [] -> []
        _ -> [
          media_types.FilesUserError(
            ["files"],
            file_version_does_not_exist_message(missing_version_ids),
            "MEDIA_VERSION_DOES_NOT_EXIST",
          ),
        ]
      }
    }
  }
}

fn file_version_evidence_matches(
  evidence: List(FileVersionEvidence),
  version_id: String,
  file_id: String,
) -> Bool {
  list.any(evidence, fn(item) {
    item.id == version_id && same_shopify_gid_tail(item.file_id, file_id)
  })
}

fn file_version_does_not_exist_message(version_ids: List(String)) -> String {
  let ids = list.map(version_ids, shopify_gid_tail_or_value)
  case ids {
    [id] -> "File version id " <> id <> " does not exist"
    _ -> "File version ids " <> string.join(ids, ", ") <> " do not exist"
  }
}

fn validate_file_update_reference_targets(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
) -> List(media_types.FilesUserError) {
  let missing_by_file =
    inputs
    |> list.filter_map(fn(input) {
      let missing_product_ids =
        list.append(
          read_string_list_field(input, "referencesToAdd"),
          read_string_list_field(input, "referencesToRemove"),
        )
        |> dedupe_strings
        |> list.filter(fn(product_id) {
          case store.get_effective_product_by_id(store, product_id) {
            Some(_) -> False
            None -> True
          }
        })
      case missing_product_ids {
        [] -> Error(Nil)
        _ ->
          Ok(#(
            read_string_field(input, "id") |> option.unwrap(""),
            missing_product_ids,
          ))
      }
    })
  case missing_by_file {
    [] -> []
    _ -> [
      media_types.FilesUserError(
        ["files"],
        "The reference target does not exist",
        "REFERENCE_TARGET_DOES_NOT_EXIST",
      ),
    ]
  }
}

fn validate_file_update_target(
  store: Store,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> #(List(String), List(String), List(media_types.FilesUserError)) {
  case read_string_field(input, "id") {
    Some(file_id) ->
      case get_effective_file_like_record(store, file_id) {
        Some(file) -> {
          case file_update_identity_matches(file, file_id) {
            True -> {
              case file.file_status {
                "READY" -> #(
                  [],
                  [],
                  validate_file_update_supported_changes(file, input, index),
                )
                _ -> #([], [file_id], [])
              }
            }
            False -> #([file_id], [], [])
          }
        }
        None -> #([file_id], [], [])
      }
    _ -> #([], [], [])
  }
}

fn file_update_identity_matches(file: FileRecord, file_id: String) -> Bool {
  case shopify_gid_type(file_id), serializers.file_typename(file) {
    Some(type_), actual_type if type_ != actual_type -> False
    _, _ -> True
  }
}

fn file_does_not_exist_count_errors(
  field: List(String),
  file_ids: List(String),
) -> List(media_types.FilesUserError) {
  case file_ids {
    [] -> []
    [file_id] -> [
      media_types.FilesUserError(
        field,
        "File id " <> file_id <> " does not exist.",
        "FILE_DOES_NOT_EXIST",
      ),
    ]
    _ -> [
      media_types.FilesUserError(
        field,
        "File ids " <> string.join(file_ids, ",") <> " do not exist.",
        "FILE_DOES_NOT_EXIST",
      ),
    ]
  }
}

fn file_update_does_not_exist_count_errors(
  file_ids: List(String),
) -> List(media_types.FilesUserError) {
  case file_ids {
    [] -> []
    [file_id] -> [
      media_types.FilesUserError(
        ["files"],
        "File id " <> quoted_id_list([file_id]) <> " does not exist.",
        "FILE_DOES_NOT_EXIST",
      ),
    ]
    _ -> [
      media_types.FilesUserError(
        ["files"],
        "File ids " <> quoted_id_list(file_ids) <> " do not exist.",
        "FILE_DOES_NOT_EXIST",
      ),
    ]
  }
}

fn quoted_id_list(file_ids: List(String)) -> String {
  "[\"" <> string.join(file_ids, "\", \"") <> "\"]"
}

fn file_update_non_ready_errors(
  file_ids: List(String),
) -> List(media_types.FilesUserError) {
  case file_ids {
    [] -> []
    _ -> [
      media_types.FilesUserError(
        ["files"],
        "Non-ready files cannot be updated.",
        "NON_READY_STATE",
      ),
    ]
  }
}

fn file_non_ready_state_count_errors(
  field: List(String),
  file_ids: List(String),
) -> List(media_types.FilesUserError) {
  case file_ids {
    [] -> []
    [file_id] -> [
      media_types.FilesUserError(
        field,
        "File with id " <> file_id <> " is not in the READY state.",
        "NON_READY_STATE",
      ),
    ]
    _ -> [
      media_types.FilesUserError(
        field,
        "Files with ids "
          <> string.join(file_ids, ", ")
          <> " are not in the READY state.",
        "NON_READY_STATE",
      ),
    ]
  }
}

fn validate_file_update_supported_changes(
  file: FileRecord,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  []
  |> list.append(validate_original_source_update(file, input, index))
  |> list.append(validate_filename_update(file, input, index))
}

fn validate_original_source_update(
  file: FileRecord,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, "originalSource") {
    Some(value) if value != "" ->
      case file_allows_source_or_filename_update(file) {
        True -> []
        False -> [
          media_types.FilesUserError(
            ["files", int.to_string(index), "originalSource"],
            "Updating the original source is not supported for this media type.",
            "INVALID",
          ),
        ]
      }
    _ -> []
  }
}

fn validate_filename_update(
  file: FileRecord,
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, "filename") {
    Some(filename) if filename != "" ->
      case file_allows_source_or_filename_update(file) {
        False -> [
          media_types.FilesUserError(
            ["files"],
            "Updating the filename is only supported on images and generic files",
            "UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE",
          ),
        ]
        True -> validate_filename_extension(file, filename, index)
      }
    _ -> []
  }
}

fn validate_filename_extension(
  file: FileRecord,
  filename: String,
  _index: Int,
) -> List(media_types.FilesUserError) {
  case file.filename {
    Some(existing) ->
      case filename_extension(existing) == filename_extension(filename) {
        True -> []
        False -> [
          media_types.FilesUserError(
            ["files"],
            "The filename extension provided must match the original filename.",
            "INVALID_FILENAME_EXTENSION",
          ),
        ]
      }
    None -> []
  }
}

fn file_allows_source_or_filename_update(file: FileRecord) -> Bool {
  case file.content_type {
    Some("IMAGE") | Some("FILE") -> True
    _ -> False
  }
}

fn validate_optional_url(
  input: Dict(String, ResolvedValue),
  index: Int,
  field_name: String,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, field_name) {
    Some(value) if value != "" ->
      case is_valid_url(value) {
        True -> []
        False -> [
          media_types.FilesUserError(
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
) -> List(media_types.FilesUserError) {
  let required_errors =
    ["filename", "mimeType", "resource"]
    |> list.flat_map(fn(field_name) {
      case read_string_field(input, field_name) {
        Some(value) if value != "" -> []
        _ -> [
          media_types.FilesUserError(
            ["input", int.to_string(index), field_name],
            field_name <> " is required",
            "REQUIRED",
          ),
        ]
      }
    })
  let resource_errors = validate_staged_upload_resource(input, index)
  let file_size_errors = validate_staged_upload_file_size(input, index)
  let mime_errors = validate_staged_upload_mime_type(input, index)
  required_errors
  |> list.append(resource_errors)
  |> list.append(file_size_errors)
  |> list.append(mime_errors)
}

fn validate_staged_upload_resource(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, "resource") {
    Some(resource) if resource != "" ->
      case list.contains(staged_upload_resource_values, resource) {
        True -> []
        False -> [
          media_types.FilesUserError(
            ["input", int.to_string(index), "resource"],
            "resource is not supported",
            "INVALID",
          ),
        ]
      }
    _ -> []
  }
}

fn validate_staged_upload_file_size(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case read_string_field(input, "resource") {
    Some("VIDEO") | Some("MODEL_3D") ->
      case has_non_null_field(input, "fileSize") {
        True -> []
        False -> [
          media_types.FilesUserError(
            ["input", int.to_string(index), "fileSize"],
            "file size is required for video resources",
            "REQUIRED",
          ),
        ]
      }
    _ -> []
  }
}

fn validate_staged_upload_mime_type(
  input: Dict(String, ResolvedValue),
  index: Int,
) -> List(media_types.FilesUserError) {
  case
    read_string_field(input, "resource"),
    read_string_field(input, "mimeType")
  {
    _, None | _, Some("") -> []
    Some(resource), Some(mime_type) ->
      case staged_upload_mime_type_allowed(resource, mime_type) {
        True -> []
        False -> [
          media_types.FilesUserError(
            ["input", int.to_string(index), "mimeType"],
            staged_upload_unrecognized_format_message(input, mime_type),
            "INVALID",
          ),
        ]
      }
    _, _ -> []
  }
}

fn staged_upload_mime_type_allowed(
  resource: String,
  mime_type: String,
) -> Bool {
  case resource {
    "IMAGE" | "COLLECTION_IMAGE" | "PRODUCT_IMAGE" | "SHOP_IMAGE" ->
      list.contains(staged_upload_image_mime_types, mime_type)
    "VIDEO" -> list.contains(staged_upload_video_mime_types, mime_type)
    "MODEL_3D" -> list.contains(staged_upload_model_mime_types, mime_type)
    _ -> True
  }
}

fn staged_upload_unrecognized_format_message(
  input: Dict(String, ResolvedValue),
  mime_type: String,
) -> String {
  let filename = read_string_field(input, "filename") |> option.unwrap("file")
  filename <> ": (" <> mime_type <> ") is not a recognized format"
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
) -> #(media_types.StagedTarget, SyntheticIdentityRegistry) {
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
    "IMAGE"
    | "FILE"
    | "COLLECTION_IMAGE"
    | "PRODUCT_IMAGE"
    | "SHOP_IMAGE"
    | "BULK_MUTATION_VARIABLES"
    | "RETURN_LABEL"
    | "URL_REDIRECT_IMPORT"
    | "DISPUTE_FILE_UPLOAD" ->
      make_google_upload_parameters(method, mime_type, key)
    "VIDEO" | "MODEL_3D" -> make_signed_upload_parameters(key)
    _ -> make_google_upload_parameters(method, mime_type, key)
  }
  let encoded_id = encode_upload_segment(id)
  let encoded_filename = encode_upload_segment(filename)
  #(
    media_types.StagedTarget(
      url: Some(
        "https://shopify-draft-proxy.local/staged-uploads/" <> encoded_id,
      ),
      resource_url: Some(
        "https://shopify-draft-proxy.local/staged-uploads/"
        <> encoded_id
        <> "/"
        <> encoded_filename,
      ),
      parameters: parameters,
    ),
    next_identity,
  )
}

fn make_google_upload_parameters(
  method: String,
  mime_type: String,
  key: String,
) -> List(#(String, String)) {
  case method {
    "PUT" ->
      list.map(google_put_upload_parameter_names, fn(name) {
        case name {
          "content_type" -> #(name, mime_type)
          "acl" -> #(name, "private")
          _ -> #(name, "shopify-draft-proxy-placeholder-" <> name)
        }
      })
    _ ->
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
  }
}

fn make_signed_upload_parameters(key: String) -> List(#(String, String)) {
  list.map(google_signed_upload_parameter_names, fn(name) {
    case name {
      "key" -> #(name, key)
      _ -> #(name, "shopify-draft-proxy-placeholder-" <> name)
    }
  })
}

fn empty_staged_target() -> media_types.StagedTarget {
  media_types.StagedTarget(url: None, resource_url: None, parameters: [])
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
  |> store.remove_media_ids_from_variants_for_products(remove_ids, [file.id])
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

fn resolved_file_gid(file: FileRecord) -> String {
  case shopify_gid_tail(file.id) {
    Some(tail) ->
      "gid://shopify/" <> serializers.file_typename(file) <> "/" <> tail
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

fn shopify_gid_tail_or_value(id: String) -> String {
  shopify_gid_tail(id) |> option.unwrap(id)
}

fn same_shopify_gid_tail(left: String, right: String) -> Bool {
  case shopify_gid_tail(left), shopify_gid_tail(right) {
    Some(left_tail), Some(right_tail) -> left_tail == right_tail
    _, _ -> left == right
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
  product(id: $id) {
    id
    title
    handle
    status
    media(first: 50) {
      nodes {
        id
        alt
        mediaContentType
        status
        preview { image { url width height } }
        ... on MediaImage { image { url width height } }
      }
    }
    variants(first: 50) {
      nodes {
        id
        title
        media(first: 10) { nodes { id } }
      }
    }
  }
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
          hydrate_product_relations_from_response(store, value, product_id)
        Error(_) -> store
      }
    }
  }
}

fn maybe_hydrate_file_update_targets(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
  upstream: UpstreamContext,
) -> Store {
  let missing_file_ids =
    inputs
    |> list.filter_map(fn(input) {
      case read_string_field(input, "id") {
        Some(id) if id != "" ->
          case get_effective_file_like_record(store, id) {
            Some(_) -> Error(Nil)
            None -> Ok(id)
          }
        _ -> Error(Nil)
      }
    })
    |> dedupe_strings
  case missing_file_ids {
    [] -> store
    _ -> {
      let query =
        "query MediaFileUpdateHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on File {
      alt
      createdAt
      fileStatus
    }
    ... on MediaImage {
      image { url width height }
      preview { image { url width height } }
    }
    ... on GenericFile {
      url
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
          "MediaFileUpdateHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          hydrate_file_update_targets_from_response(
            store,
            value,
            missing_file_ids,
          )
        Error(_) -> store
      }
    }
  }
}

fn maybe_hydrate_file_update_revert_versions(
  inputs: List(Dict(String, ResolvedValue)),
  upstream: UpstreamContext,
) -> FileVersionHydration {
  let version_ids =
    inputs
    |> list.filter_map(fn(input) {
      case
        has_source_update(input),
        read_string_field(input, "revertToVersionId")
      {
        False, Some(version_id) if version_id != "" -> Ok(version_id)
        _, _ -> Error(Nil)
      }
    })
    |> dedupe_strings
  case upstream.allow_upstream_reads, version_ids {
    False, _ -> FileVersionHydration(validation_enabled: False, evidence: [])
    True, [] -> FileVersionHydration(validation_enabled: False, evidence: [])
    True, _ -> {
      let query =
        "query MediaFileVersionHydrate($versionIds: [ID!]!) {
  nodes(ids: $versionIds) {
    id
    __typename
    ... on MediaVersion {
      file { id }
      media { id }
    }
  }
}
"
      let variables =
        json.object([
          #("versionIds", json.array(version_ids, json.string)),
        ])
      let evidence = case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "MediaFileVersionHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> file_version_evidence_from_response(value, version_ids)
        Error(_) -> []
      }
      FileVersionHydration(validation_enabled: True, evidence: evidence)
    }
  }
}

fn file_version_evidence_from_response(
  value: commit.JsonValue,
  version_ids: List(String),
) -> List(FileVersionEvidence) {
  case json_get(value, "data") {
    Some(data) ->
      json_array(json_get(data, "nodes"))
      |> list.filter_map(fn(node) {
        case json_get_string(node, "id") {
          Some(version_id) ->
            case list.contains(version_ids, version_id) {
              True -> file_version_evidence_from_node(node, version_id)
              False -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    None -> []
  }
}

fn file_version_evidence_from_node(
  node: commit.JsonValue,
  version_id: String,
) -> Result(FileVersionEvidence, Nil) {
  case json_get_string(node, "__typename") {
    Some("MediaVersion") -> {
      let file_id =
        json_get(node, "file")
        |> option.then(fn(file) { json_get_string(file, "id") })
        |> option.or(
          json_get(node, "media")
          |> option.then(fn(media) { json_get_string(media, "id") }),
        )
      case file_id {
        Some(id) if id != "" ->
          Ok(FileVersionEvidence(id: version_id, file_id: id))
        _ -> Error(Nil)
      }
    }
    _ -> Error(Nil)
  }
}

fn hydrate_file_update_targets_from_response(
  store: Store,
  value: commit.JsonValue,
  file_ids: List(String),
) -> Store {
  case json_get(value, "data") {
    Some(data) -> {
      let files =
        json_array(json_get(data, "nodes"))
        |> list.filter_map(fn(node) {
          case json_get_string(node, "id") {
            Some(file_id) ->
              case list.contains(file_ids, file_id) {
                True -> file_record_from_file_node(node, file_id)
                False -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        })
      store.upsert_base_files(store, files)
    }
    None -> store
  }
}

fn file_record_from_file_node(
  node: commit.JsonValue,
  file_id: String,
) -> Result(FileRecord, Nil) {
  let image = image_value(node)
  let preview = preview_image_value(node)
  let source_url =
    json_get_string(node, "url")
    |> option.or(
      image |> option.then(fn(value) { json_get_string(value, "url") }),
    )
    |> option.or(
      preview |> option.then(fn(value) { json_get_string(value, "url") }),
    )
  case json_get_string(node, "__typename") {
    Some(typename) ->
      Ok(FileRecord(
        id: file_id,
        alt: json_get_string(node, "alt"),
        content_type: file_typename_to_content_type(typename),
        created_at: json_get_string(node, "createdAt")
          |> option.unwrap("2024-01-01T00:00:00.000Z"),
        file_status: json_get_string(node, "fileStatus")
          |> option.or(json_get_string(node, "status"))
          |> option.unwrap("READY"),
        filename: source_url |> option.then(derive_filename),
        original_source: source_url |> option.unwrap(""),
        image_url: image
          |> option.then(fn(value) { json_get_string(value, "url") })
          |> option.or(
            preview |> option.then(fn(value) { json_get_string(value, "url") }),
          ),
        image_width: image
          |> option.then(fn(value) { json_get_int(value, "width") }),
        image_height: image
          |> option.then(fn(value) { json_get_int(value, "height") }),
        update_failure_acknowledged_at: None,
      ))
    None -> Error(Nil)
  }
}

fn file_typename_to_content_type(typename: String) -> Option(String) {
  case typename {
    "MediaImage" -> Some("IMAGE")
    "Video" -> Some("VIDEO")
    "ExternalVideo" -> Some("EXTERNAL_VIDEO")
    "Model3d" -> Some("MODEL_3D")
    "GenericFile" -> Some("FILE")
    _ -> None
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
      references(first: 10) {
        nodes {
          ... on Product {
            id
            title
            handle
            status
            variants(first: 50) {
              nodes {
                id
                title
                media(first: 10) { nodes { id } }
              }
            }
          }
        }
      }
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
        let variants =
          product_variants_from_product_node(product.id, product_node)
        let current = case variants {
          [] -> current
          _ -> store.upsert_base_product_variants(current, variants)
        }
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

fn hydrate_product_relations_from_response(
  store: Store,
  value: commit.JsonValue,
  fallback_id: String,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case non_null_node(json_get(data, "product")) {
        Some(product_node) ->
          case product_record_from_node(product_node, Some(fallback_id)) {
            Some(product) -> {
              let media =
                product_media_from_product_node(product.id, product_node)
              let variants =
                product_variants_from_product_node(product.id, product_node)
              store
              |> store.upsert_base_products([product])
              |> store.replace_base_media_for_product(product.id, media)
              |> store.upsert_base_product_variants(variants)
            }
            None -> store
          }
        None -> store
      }
    None -> store
  }
}

fn referenced_product_nodes(node: commit.JsonValue) -> List(commit.JsonValue) {
  case json_get(node, "references") {
    Some(references) -> json_array(json_get(references, "nodes"))
    None -> []
  }
}

fn product_media_from_product_node(
  product_id: String,
  node: commit.JsonValue,
) -> List(ProductMediaRecord) {
  case json_get(node, "media") {
    Some(media_connection) ->
      json_array(json_get(media_connection, "nodes"))
      |> enumerate_json
      |> list.filter_map(fn(entry) {
        let #(media_node, index) = entry
        case
          product_hydration.product_media_from_json(
            product_id,
            media_node,
            index,
          )
        {
          Some(media) -> Ok(media)
          None -> Error(Nil)
        }
      })
    None -> []
  }
}

fn product_variants_from_product_node(
  product_id: String,
  node: commit.JsonValue,
) -> List(ProductVariantRecord) {
  case json_get(node, "variants") {
    Some(variant_connection) ->
      json_array(json_get(variant_connection, "nodes"))
      |> list.filter_map(fn(variant_node) {
        case
          product_hydration.product_variant_from_json(product_id, variant_node)
        {
          Some(variant) -> Ok(variant)
          None -> Error(Nil)
        }
      })
    None -> []
  }
}

fn enumerate_json(
  items: List(commit.JsonValue),
) -> List(#(commit.JsonValue, Int)) {
  enumerate_json_loop(items, 0, [])
}

fn enumerate_json_loop(
  items: List(commit.JsonValue),
  index: Int,
  acc: List(#(commit.JsonValue, Int)),
) -> List(#(commit.JsonValue, Int)) {
  case items {
    [] -> list.fold(acc, [], fn(reversed, item) { [item, ..reversed] })
    [item, ..rest] ->
      enumerate_json_loop(rest, index + 1, [#(item, index), ..acc])
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

fn product_record_from_node(
  node: commit.JsonValue,
  fallback_id: Option(String),
) -> Option(ProductRecord) {
  case json_get_string(node, "id") |> option.or(fallback_id) {
    Some(id) -> {
      let title = json_get_string(node, "title") |> option.unwrap("Product")
      Some(
        ProductRecord(
          id: id,
          legacy_resource_id: None,
          title: title,
          handle: json_get_string(node, "handle") |> option.unwrap("product"),
          status: json_get_string(node, "status") |> option.unwrap("ACTIVE"),
          vendor: None,
          product_type: None,
          tags: [],
          price_range_min: None,
          price_range_max: None,
          total_variants: None,
          has_only_default_variant: None,
          has_out_of_stock_variants: None,
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
          combined_listing_role: None,
          combined_listing_parent_id: None,
          combined_listing_child_ids: [],
        ),
      )
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

fn has_non_null_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.NullVal) | Error(_) -> False
    _ -> True
  }
}

fn is_valid_url(value: String) -> Bool {
  string.starts_with(value, "https://") || string.starts_with(value, "http://")
}

fn has_non_http_uri_scheme(value: String) -> Bool {
  case string.split_once(value, on: ":") {
    Ok(#(scheme, _)) if scheme != "" -> scheme != "http" && scheme != "https"
    _ -> False
  }
}

fn file_extension(value: String) -> String {
  let path =
    value
    |> before_delimiter("?")
    |> before_delimiter("#")
  let last_segment =
    path
    |> string.split("/")
    |> reverse_strings
    |> list.find(fn(part) { part != "" })
    |> option.from_result
    |> option.unwrap("")
  case string.split(last_segment, ".") {
    [_] -> ""
    parts ->
      case list.last(parts) {
        Ok(extension) -> "." <> extension
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

fn non_empty_string(value: String) -> Option(String) {
  case value {
    "" -> None
    _ -> Some(value)
  }
}

fn has_source_update(input: Dict(String, ResolvedValue)) -> Bool {
  case
    read_string_field(input, "originalSource"),
    read_string_field(input, "previewImageSource")
  {
    Some(value), _ if value != "" -> True
    _, Some(value) if value != "" -> True
    _, _ -> False
  }
}

fn derive_filename(url: String) -> Option(String) {
  let parts = string.split(url, "/")
  parts
  |> reverse_strings
  |> list.find(fn(part) { part != "" })
  |> option.from_result
}

fn filename_extension(filename: String) -> Option(String) {
  let without_query = case string.split(filename, "?") {
    [head, ..] -> head
    [] -> filename
  }
  let without_fragment = case string.split(without_query, "#") {
    [head, ..] -> head
    [] -> without_query
  }
  let parts = string.split(without_fragment, ".")
  case reverse_strings(parts) {
    [extension, _, ..] if extension != "" -> Some(string.lowercase(extension))
    _ -> None
  }
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

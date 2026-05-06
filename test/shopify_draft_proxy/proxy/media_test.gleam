//// Read-path tests for the minimal `proxy/media` stub. The single
//// `files` connection root returns the empty-connection shape — this
//// guards that contract on both compile targets.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/proxy_state.{DraftProxy, Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{type FileRecord, FileRecord}

fn run(query: String) -> String {
  let assert Ok(data) = media.handle_media_query(store.new(), query, dict.new())
  json.to_string(data)
}

fn registry_proxy() {
  draft_proxy.new()
  |> draft_proxy.with_default_registry
}

fn registry_proxy_with_files(files: List(FileRecord)) {
  let proxy = registry_proxy()
  DraftProxy(..proxy, store: store.upsert_staged_files(proxy.store, files))
}

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
}

fn ready_image() -> FileRecord {
  FileRecord(
    id: "gid://shopify/MediaImage/1",
    alt: Some("Seed"),
    content_type: Some("IMAGE"),
    created_at: "2026-05-05T00:00:00.000Z",
    file_status: "READY",
    filename: Some("seed.jpg"),
    original_source: "https://cdn.example.com/seed.jpg",
    image_url: Some("https://cdn.example.com/seed.jpg"),
    image_width: None,
    image_height: None,
    update_failure_acknowledged_at: None,
  )
}

fn ready_video() -> FileRecord {
  FileRecord(
    id: "gid://shopify/Video/2",
    alt: None,
    content_type: Some("VIDEO"),
    created_at: "2026-05-05T00:00:00.000Z",
    file_status: "READY",
    filename: Some("clip.mp4"),
    original_source: "https://cdn.example.com/clip.mp4",
    image_url: None,
    image_width: None,
    image_height: None,
    update_failure_acknowledged_at: None,
  )
}

fn processing_image() -> FileRecord {
  FileRecord(..ready_image(), file_status: "PROCESSING")
}

pub fn is_media_query_root_test() {
  assert media.is_media_query_root("files")
  assert !media.is_media_query_root("fileSavedSearches")
  assert !media.is_media_query_root("fileCreate")
  assert !media.is_media_query_root("shop")
}

pub fn files_returns_empty_connection_test() {
  let result =
    run(
      "{ files(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"files\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn files_with_edges_returns_empty_test() {
  let result = run("{ files(first: 10) { edges { cursor } } }")
  assert result == "{\"files\":{\"edges\":[]}}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(data) =
    media.process(
      store.new(),
      "{ files(first: 10) { nodes { id } } }",
      dict.new(),
    )
  assert json.to_string(data) == "{\"data\":{\"files\":{\"nodes\":[]}}}"
}

pub fn file_create_image_is_readable_while_uploaded_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", contentType: IMAGE, alt: \"Foo\" }]) { files { id fileStatus alt ... on MediaImage { image { url width height } preview { image { url } } } } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"fileStatus\":\"UPLOADED\"")
  assert string.contains(
    create_json,
    "\"image\":{\"url\":\"https://cdn.example.com/foo.png\",\"width\":null,\"height\":null}",
  )
  assert string.contains(
    create_json,
    "\"preview\":{\"image\":{\"url\":\"https://cdn.example.com/foo.png\"}}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { files(first: 5) { nodes { id fileStatus alt ... on MediaImage { image { url width height } preview { image { url } } } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"fileStatus\":\"UPLOADED\"")
  assert string.contains(
    read_json,
    "\"image\":{\"url\":\"https://cdn.example.com/foo.png\",\"width\":null,\"height\":null}",
  )
  assert string.contains(
    read_json,
    "\"preview\":{\"image\":{\"url\":\"https://cdn.example.com/foo.png\"}}",
  )
}

pub fn file_acknowledge_update_failed_rejects_non_ready_file_test() {
  let #(_, proxy) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/non-ready.png\", contentType: IMAGE }]) { files { id fileStatus } userErrors { code } } }",
    )

  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { fileAcknowledgeUpdateFailed(fileIds: [\"gid://shopify/MediaImage/2\"]) { files { id fileStatus } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileAcknowledgeUpdateFailed\":{\"files\":null,\"userErrors\":[{\"field\":[\"fileIds\"],\"message\":\"File with id gid://shopify/MediaImage/2 is not in the READY state.\",\"code\":\"NON_READY_STATE\"}]}}}"
}

pub fn file_acknowledge_update_failed_ready_file_is_state_noop_test() {
  let proxy = registry_proxy_with_files([ready_image()])

  let #(Response(status: status, body: body, ..), proxy) =
    graphql(
      proxy,
      "mutation { fileAcknowledgeUpdateFailed(fileIds: [\"gid://shopify/MediaImage/1\"]) { files { id fileStatus __typename mediaErrors { code message } mediaWarnings { code message } } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileAcknowledgeUpdateFailed\":{\"files\":[{\"id\":\"gid://shopify/MediaImage/1\",\"fileStatus\":\"READY\",\"__typename\":\"MediaImage\",\"mediaErrors\":[],\"mediaWarnings\":[]}],\"userErrors\":[]}}}"

  let state_json =
    draft_proxy.dump_state(proxy, "2026-05-05T10:15:00.000Z")
    |> json.to_string
  assert string.contains(state_json, "\"updateFailureAcknowledgedAt\":null")
  assert !string.contains(state_json, "\"updateFailureAcknowledgedAt\":\"")
}

pub fn file_acknowledge_update_failed_after_rejected_update_keeps_state_test() {
  let #(_, proxy) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/ack-source.png\", contentType: IMAGE }]) { files { id fileStatus } userErrors { code } } }",
    )
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/MediaImage/2\", originalSource: \"https://cdn.example.com/ack-ready.png\" }]) { files { id fileStatus } userErrors { code } } }",
    )

  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"code\":\"NON_READY_STATE\"}]}}}"

  let #(Response(status: status, body: body, ..), proxy) =
    graphql(
      proxy,
      "mutation { fileAcknowledgeUpdateFailed(fileIds: [\"gid://shopify/MediaImage/2\"]) { files { id fileStatus __typename mediaErrors { code message } mediaWarnings { code message } } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileAcknowledgeUpdateFailed\":{\"files\":null,\"userErrors\":[{\"field\":[\"fileIds\"],\"message\":\"File with id gid://shopify/MediaImage/2 is not in the READY state.\",\"code\":\"NON_READY_STATE\"}]}}}"

  let state_json =
    draft_proxy.dump_state(proxy, "2026-05-05T10:15:00.000Z")
    |> json.to_string
  assert string.contains(state_json, "\"updateFailureAcknowledgedAt\":null")
  assert !string.contains(state_json, "\"updateFailureAcknowledgedAt\":\"")
}

pub fn file_create_rejects_shopify_validation_branches_test() {
  let #(Response(status: references_status, body: references_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", referencesToAdd: [\"gid://shopify/Product/1\", \"gid://shopify/Product/2\"] }]) { files { id } userErrors { field message code } } }",
    )
  assert references_status == 200
  assert json.to_string(references_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"referencesToAdd\"],\"message\":\"Too many product ids specified.\",\"code\":\"TOO_MANY_PRODUCT_IDS_SPECIFIED\"}]}}}"

  let #(Response(status: data_status, body: data_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"data:image/png;base64,iVBORw0KGgo=\" }]) { files { id } userErrors { field message code } } }",
    )
  assert data_status == 200
  assert json.to_string(data_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"originalSource\"],\"message\":\"File URL is invalid\",\"code\":\"INVALID_IMAGE_SOURCE_URL\"}]}}}"

  let #(Response(status: extension_status, body: extension_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", filename: \"bar.jpg\" }]) { files { id } userErrors { field message code } } }",
    )
  assert extension_status == 200
  assert json.to_string(extension_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"filename\"],\"message\":\"Provided filename extension must match original source.\",\"code\":\"MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE\"}]}}}"
}

pub fn file_create_validates_length_and_duplicate_modes_test() {
  let #(Response(status: empty_status, body: empty_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"\" }]) { files { id } userErrors { field message code } } }",
    )
  assert empty_status == 200
  assert json.to_string(empty_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"originalSource\"],\"message\":\"originalSource is too short (minimum is 1)\",\"code\":\"INVALID\"}]}}}"

  let long_source =
    "https://cdn.example.com/" <> string.repeat("a", times: 2050) <> ".png"
  let #(Response(status: long_status, body: long_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \""
        <> long_source
        <> "\" }]) { files { id } userErrors { field message code } } }",
    )
  assert long_status == 200
  assert json.to_string(long_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"originalSource\"],\"message\":\"originalSource is too long (maximum is 2048)\",\"code\":\"INVALID\"}]}}}"

  let #(Response(status: mode_status, body: mode_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", duplicateResolutionMode: REPLACE }]) { files { id } userErrors { field message code } } }",
    )
  assert mode_status == 200
  assert json.to_string(mode_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"duplicateResolutionMode\"],\"message\":\"Duplicate resolution mode 'REPLACE' is not supported for 'MISSING' media type.\",\"code\":\"INVALID_DUPLICATE_MODE_FOR_TYPE\"}]}}}"

  let #(Response(status: replace_status, body: replace_body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", contentType: IMAGE, duplicateResolutionMode: REPLACE }]) { files { id } userErrors { field message code } } }",
    )
  assert replace_status == 200
  assert json.to_string(replace_body)
    == "{\"data\":{\"fileCreate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"filename\"],\"message\":\"Missing filename argument when attempting to use REPLACE duplicate mode.\",\"code\":\"MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE\"}]}}}"
}

pub fn file_create_accepts_long_alt_and_valid_duplicate_mode_test() {
  let long_alt = string.repeat("a", times: 513)
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/foo.png\", filename: \"foo.png\", contentType: IMAGE, duplicateResolutionMode: RAISE_ERROR, alt: \""
        <> long_alt
        <> "\" }]) { files { id alt } userErrors { field message code } } }",
    )

  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"userErrors\":[]")
  assert string.contains(body_json, "\"alt\":\"" <> long_alt <> "\"")
}

pub fn file_delete_re_resolves_wrong_typed_gid_to_actual_file_type_test() {
  let #(_, proxy) =
    graphql(
      registry_proxy(),
      "mutation { fileCreate(files: [{ originalSource: \"https://cdn.example.com/delete-me.png\", contentType: IMAGE }]) { files { id } userErrors { code } } }",
    )
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { fileDelete(fileIds: [\"gid://shopify/Video/2\"]) { deletedFileIds userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileDelete\":{\"deletedFileIds\":[\"gid://shopify/MediaImage/2\"],\"userErrors\":[]}}}"
}

pub fn file_update_rejects_non_ready_file_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([processing_image()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/MediaImage/1\", alt: \"New alt\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\"],\"message\":\"Non-ready files cannot be updated.\",\"code\":\"NON_READY_STATE\"}]}}}"
}

pub fn file_update_rejects_video_original_source_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_video()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/Video/2\", originalSource: \"https://cdn.example.com/new.mp4\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"originalSource\"],\"message\":\"Updating the original source is not supported for this media type.\",\"code\":\"INVALID\"}]}}}"
}

pub fn file_update_rejects_video_filename_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_video()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/Video/2\", filename: \"clip-new.mp4\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\"],\"message\":\"Updating the filename is only supported on images and generic files\",\"code\":\"UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE\"}]}}}"
}

pub fn file_update_rejects_filename_extension_mismatch_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_image()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/MediaImage/1\", filename: \"seed.png\" }]) { files { id fileStatus alt filename __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\"],\"message\":\"The filename extension provided must match the original filename.\",\"code\":\"INVALID_FILENAME_EXTENSION\"}]}}}"
}

pub fn file_update_rejects_source_and_revert_version_conflict_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_image()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/MediaImage/1\", originalSource: \"https://cdn.example.com/v2.jpg\", revertToVersionId: \"gid://shopify/FileVersion/9\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\",\"0\"],\"message\":\"Specify either a source or revertToVersionId, not both.\",\"code\":\"CANNOT_SPECIFY_SOURCE_AND_VERSION_ID\"}]}}}"
}

pub fn file_update_rejects_mismatched_gid_type_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_image()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/Video/1\", alt: \"Wrong type\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[],\"userErrors\":[{\"field\":[\"files\"],\"message\":\"File id [\\\"gid://shopify/Video/1\\\"] does not exist.\",\"code\":\"FILE_DOES_NOT_EXIST\"}]}}}"
}

pub fn file_update_preserves_ready_status_after_success_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy_with_files([ready_image()]),
      "mutation { fileUpdate(files: [{ id: \"gid://shopify/MediaImage/1\", alt: \"Updated alt\" }]) { files { id fileStatus alt __typename } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"fileUpdate\":{\"files\":[{\"id\":\"gid://shopify/MediaImage/1\",\"fileStatus\":\"READY\",\"alt\":\"Updated alt\",\"__typename\":\"MediaImage\"}],\"userErrors\":[]}}}"
}

pub fn staged_uploads_create_requires_video_file_size_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { stagedUploadsCreate(input: [{ resource: VIDEO, filename: \"x.mp4\", mimeType: \"video/mp4\" }]) { stagedTargets { url resourceUrl parameters { name } } userErrors { field message } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"stagedUploadsCreate\":{\"stagedTargets\":[{\"url\":null,\"resourceUrl\":null,\"parameters\":[]}],\"userErrors\":[{\"field\":[\"input\",\"0\",\"fileSize\"],\"message\":\"file size is required for video resources\"}]}}}"
}

pub fn staged_uploads_create_rejects_image_unsupported_mime_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { stagedUploadsCreate(input: [{ resource: IMAGE, filename: \"x.exe\", mimeType: \"application/x-msdownload\" }]) { stagedTargets { url resourceUrl parameters { name } } userErrors { field message } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"stagedUploadsCreate\":{\"stagedTargets\":[{\"url\":null,\"resourceUrl\":null,\"parameters\":[]}],\"userErrors\":[{\"field\":[\"input\",\"0\",\"mimeType\"],\"message\":\"x.exe: (application/x-msdownload) is not a recognized format\"}]}}}"
}

pub fn staged_uploads_create_user_errors_rejects_code_selection_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { stagedUploadsCreate(input: [{ resource: VIDEO, filename: \"x.mp4\", mimeType: \"video/mp4\" }]) { userErrors { field message code } } }",
    )

  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(
    body_json,
    "\"message\":\"Field 'code' doesn't exist on type 'UserError'\"",
  )
  assert string.contains(
    body_json,
    "\"extensions\":{\"code\":\"undefinedField\",\"typeName\":\"UserError\",\"fieldName\":\"code\"}",
  )
  assert !string.contains(body_json, "\"data\"")
}

pub fn staged_uploads_create_rejects_unknown_resource_variable_test() {
  let query =
    "mutation Repro($input: [StagedUploadInput!]!) { stagedUploadsCreate(input: $input) { stagedTargets { url } userErrors { field message } } }"
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\""
        <> escape(query)
        <> "\",\"variables\":{\"input\":[{\"resource\":\"BANANA\",\"filename\":\"x\",\"mimeType\":\"x/x\"}]}}",
    )
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(registry_proxy(), request)

  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    body_json,
    "Expected \\\"BANANA\\\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD",
  )
  assert !string.contains(body_json, "\"stagedTargets\"")
}

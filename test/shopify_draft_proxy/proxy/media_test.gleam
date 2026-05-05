//// Read-path tests for the minimal `proxy/media` stub. The single
//// `files` connection root returns the empty-connection shape — this
//// guards that contract on both compile targets.

import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store

fn run(query: String) -> String {
  let assert Ok(data) = media.handle_media_query(store.new(), query, dict.new())
  json.to_string(data)
}

fn registry_proxy() {
  draft_proxy.new()
  |> draft_proxy.with_default_registry
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

pub fn staged_uploads_create_requires_video_file_size_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { stagedUploadsCreate(input: [{ resource: VIDEO, filename: \"x.mp4\", mimeType: \"video/mp4\" }]) { stagedTargets { url resourceUrl parameters { name } } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"stagedUploadsCreate\":{\"stagedTargets\":[{\"url\":null,\"resourceUrl\":null,\"parameters\":[]}],\"userErrors\":[{\"field\":[\"input\",\"0\",\"fileSize\"],\"message\":\"file size is required for video resources\",\"code\":\"REQUIRED\"}]}}}"
}

pub fn staged_uploads_create_rejects_image_unsupported_mime_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      registry_proxy(),
      "mutation { stagedUploadsCreate(input: [{ resource: IMAGE, filename: \"x.exe\", mimeType: \"application/x-msdownload\" }]) { stagedTargets { url resourceUrl parameters { name } } userErrors { field message code } } }",
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"stagedUploadsCreate\":{\"stagedTargets\":[{\"url\":null,\"resourceUrl\":null,\"parameters\":[]}],\"userErrors\":[{\"field\":[\"input\",\"0\",\"mimeType\"],\"message\":\"x.exe: (application/x-msdownload) is not a recognized format\",\"code\":\"INVALID\"}]}}}"
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

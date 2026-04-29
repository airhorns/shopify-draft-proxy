//// Read-path tests for the minimal `proxy/media` stub. The single
//// `files` connection root returns the empty-connection shape — this
//// guards that contract on both compile targets.

import gleam/json
import shopify_draft_proxy/proxy/media

fn run(query: String) -> String {
  let assert Ok(data) = media.handle_media_query(query)
  json.to_string(data)
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
    media.process("{ files(first: 10) { nodes { id } } }")
  assert json.to_string(data) == "{\"data\":{\"files\":{\"nodes\":[]}}}"
}

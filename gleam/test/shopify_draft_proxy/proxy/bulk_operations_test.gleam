//// Read-path tests for the minimal `proxy/bulk_operations` stub. Every
//// singular root returns null, every connection root returns the
//// empty-connection shape — this guards that contract on both
//// compile targets.

import gleam/json
import shopify_draft_proxy/proxy/bulk_operations

fn run(query: String) -> String {
  let assert Ok(data) = bulk_operations.handle_bulk_operations_query(query)
  json.to_string(data)
}

pub fn is_bulk_operations_query_root_test() {
  assert bulk_operations.is_bulk_operations_query_root("bulkOperation")
  assert bulk_operations.is_bulk_operations_query_root("bulkOperations")
  assert bulk_operations.is_bulk_operations_query_root("currentBulkOperation")
  assert !bulk_operations.is_bulk_operations_query_root("bulkOperationCancel")
  assert !bulk_operations.is_bulk_operations_query_root("shop")
}

pub fn bulk_operation_returns_null_test() {
  let result =
    run("{ bulkOperation(id: \"gid://shopify/BulkOperation/1\") { id } }")
  assert result == "{\"bulkOperation\":null}"
}

pub fn current_bulk_operation_returns_null_test() {
  let result = run("{ currentBulkOperation { id status } }")
  assert result == "{\"currentBulkOperation\":null}"
}

pub fn bulk_operations_returns_empty_connection_test() {
  let result =
    run(
      "{ bulkOperations(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"bulkOperations\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn bulk_operations_with_edges_returns_empty_test() {
  let result = run("{ bulkOperations(first: 10) { edges { cursor } } }")
  assert result == "{\"bulkOperations\":{\"edges\":[]}}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(data) =
    bulk_operations.process(
      "{ bulkOperation(id: \"gid://shopify/BulkOperation/1\") { id } }",
    )
  assert json.to_string(data) == "{\"data\":{\"bulkOperation\":null}}"
}

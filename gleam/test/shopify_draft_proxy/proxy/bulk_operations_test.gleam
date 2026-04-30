//// Read and mutation tests for the Gleam BulkOperations domain port.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/bulk_operations
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{BulkOperationRecord}

fn empty_vars() {
  dict.new()
}

/// Apply the dispatcher-level `record_log_drafts` to the outcome. Tests that
/// exercise `bulk_operations.process_mutation` directly need this so log-buffer
/// assertions still see the drafts the module emitted; centralized recording
/// is the dispatcher's responsibility post-refactor.
fn record_drafts(
  outcome: bulk_operations.MutationOutcome,
  request_path: String,
  document: String,
) -> bulk_operations.MutationOutcome {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      outcome.log_drafts,
    )
  bulk_operations.MutationOutcome(
    ..outcome,
    store: logged_store,
    identity: logged_identity,
  )
}

fn run(source: store.Store, query: String) -> String {
  let assert Ok(data) =
    bulk_operations.handle_bulk_operations_query(source, query, empty_vars())
  json.to_string(data)
}

fn bulk_operation(
  id: String,
  status: String,
  type_: String,
  created_at: String,
) {
  BulkOperationRecord(
    id: id,
    status: status,
    type_: type_,
    error_code: None,
    created_at: created_at,
    completed_at: None,
    object_count: "0",
    root_object_count: "0",
    file_size: None,
    url: None,
    partial_data_url: None,
    query: None,
    cursor: None,
    result_jsonl: None,
  )
}

pub fn root_predicates_test() {
  assert bulk_operations.is_bulk_operations_query_root("bulkOperation")
  assert bulk_operations.is_bulk_operations_query_root("bulkOperations")
  assert bulk_operations.is_bulk_operations_query_root("currentBulkOperation")
  assert bulk_operations.is_bulk_operations_mutation_root("bulkOperationCancel")
  assert bulk_operations.is_bulk_operations_mutation_root(
    "bulkOperationRunQuery",
  )
  assert !bulk_operations.is_bulk_operations_mutation_root(
    "bulkOperationRunMutation",
  )
  assert !bulk_operations.is_bulk_operations_query_root("shop")
}

pub fn empty_reads_keep_shopify_like_shapes_test() {
  let source = store.new()
  let result =
    run(
      source,
      "{ bulkOperation(id: \"gid://shopify/BulkOperation/1\") { id } bulkOperations(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } currentBulkOperation { id } }",
    )
  assert result
    == "{\"bulkOperation\":null,\"bulkOperations\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"currentBulkOperation\":null}"
}

pub fn reads_lists_filters_paginates_and_derives_current_test() {
  let base =
    bulk_operation(
      "gid://shopify/BulkOperation/101",
      "COMPLETED",
      "QUERY",
      "2026-04-27T00:00:01Z",
    )
  let running_mutation =
    bulk_operation(
      "gid://shopify/BulkOperation/202",
      "RUNNING",
      "MUTATION",
      "2026-04-27T00:00:03Z",
    )
  let running_query =
    bulk_operation(
      "gid://shopify/BulkOperation/303",
      "RUNNING",
      "QUERY",
      "2026-04-27T00:00:04Z",
    )
  let source = store.upsert_base_bulk_operations(store.new(), [base])
  let #(_, source) = store.stage_bulk_operation(source, running_mutation)
  let #(_, source) = store.stage_bulk_operation(source, running_query)

  let result =
    run(
      source,
      "{ byId: bulkOperation(id: \"gid://shopify/BulkOperation/202\") { id status type } firstPage: bulkOperations(first: 1) { edges { cursor node { id createdAt } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } secondPage: bulkOperations(first: 1, after: \"cursor:gid://shopify/BulkOperation/303\") { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } runningMutations: bulkOperations(first: 5, query: \"status:RUNNING operation_type:MUTATION\") { nodes { id type status } } reversedById: bulkOperations(first: 5, sortKey: ID, reverse: true) { nodes { id } } currentQuery: currentBulkOperation(type: QUERY) { id } currentMutation: currentBulkOperation(type: MUTATION) { id } }",
    )

  assert string.contains(
    result,
    "\"byId\":{\"id\":\"gid://shopify/BulkOperation/202\",\"status\":\"RUNNING\",\"type\":\"MUTATION\"}",
  )
  assert string.contains(
    result,
    "\"firstPage\":{\"edges\":[{\"cursor\":\"cursor:gid://shopify/BulkOperation/303\",\"node\":{\"id\":\"gid://shopify/BulkOperation/303\",\"createdAt\":\"2026-04-27T00:00:04Z\"}}],\"nodes\":[{\"id\":\"gid://shopify/BulkOperation/303\"}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/BulkOperation/303\",\"endCursor\":\"cursor:gid://shopify/BulkOperation/303\"}}",
  )
  assert string.contains(
    result,
    "\"runningMutations\":{\"nodes\":[{\"id\":\"gid://shopify/BulkOperation/202\",\"type\":\"MUTATION\",\"status\":\"RUNNING\"}]}",
  )
  assert string.contains(
    result,
    "\"currentQuery\":{\"id\":\"gid://shopify/BulkOperation/303\"}",
  )
  assert string.contains(
    result,
    "\"currentMutation\":{\"id\":\"gid://shopify/BulkOperation/202\"}",
  )
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(data) =
    bulk_operations.process(
      store.new(),
      "{ bulkOperation(id: \"gid://shopify/BulkOperation/1\") { id } }",
      empty_vars(),
    )
  assert json.to_string(data) == "{\"data\":{\"bulkOperation\":null}}"
}

pub fn run_query_stages_completed_operation_and_log_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ products { edges { node { id } } } }\") { bulkOperation { id status type objectCount rootObjectCount fileSize url partialDataUrl query } userErrors { field message code } } }"
  let assert Ok(outcome) =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
    )
  let outcome = record_drafts(outcome, request_path, document)
  let response = json.to_string(outcome.data)
  assert string.contains(
    response,
    "\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/1\",\"status\":\"COMPLETED\",\"type\":\"QUERY\"",
  )
  assert string.contains(response, "\"objectCount\":\"0\"")
  assert string.contains(response, "\"rootObjectCount\":\"0\"")
  assert string.contains(response, "\"userErrors\":[]")
  assert outcome.staged_resource_ids == ["gid://shopify/BulkOperation/1"]
  assert list.length(store.get_log(outcome.store)) == 1
}

pub fn cancel_staged_terminal_and_missing_operations_test() {
  let running =
    bulk_operation(
      "gid://shopify/BulkOperation/401",
      "RUNNING",
      "QUERY",
      "2026-04-27T00:00:04Z",
    )
  let terminal =
    BulkOperationRecord(
      ..bulk_operation(
        "gid://shopify/BulkOperation/402",
        "COMPLETED",
        "QUERY",
        "2026-04-27T00:00:05Z",
      ),
      completed_at: Some("2026-04-27T00:01:00Z"),
    )
  let #(_, source) = store.stage_bulk_operation(store.new(), running)
  let #(_, source) = store.stage_bulk_operation(source, terminal)
  let assert Ok(outcome) =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { running: bulkOperationCancel(id: \"gid://shopify/BulkOperation/401\") { bulkOperation { id status completedAt } userErrors { field message } } terminal: bulkOperationCancel(id: \"gid://shopify/BulkOperation/402\") { bulkOperation { id status } userErrors { field message } } missing: bulkOperationCancel(id: \"gid://shopify/BulkOperation/0\") { bulkOperation { id } userErrors { field message } } }",
      empty_vars(),
    )
  let response = json.to_string(outcome.data)
  assert string.contains(
    response,
    "\"running\":{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/401\",\"status\":\"CANCELING\",\"completedAt\":null},\"userErrors\":[]}",
  )
  assert string.contains(
    response,
    "\"terminal\":{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/402\",\"status\":\"COMPLETED\"},\"userErrors\":[{\"field\":null,\"message\":\"A bulk operation cannot be canceled when it is completed\"}]}",
  )
  assert string.contains(
    response,
    "\"missing\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Bulk operation does not exist\"}]}",
  )
  let read_after =
    run(
      outcome.store,
      "{ bulkOperation(id: \"gid://shopify/BulkOperation/401\") { id status } }",
    )
  assert read_after
    == "{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/401\",\"status\":\"CANCELING\"}}"
}

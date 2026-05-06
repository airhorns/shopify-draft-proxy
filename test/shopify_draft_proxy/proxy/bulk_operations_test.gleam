//// Read and mutation tests for the Gleam BulkOperations domain port.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/bulk_operations
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type ProductRecord, BulkOperationRecord, ProductRecord, ProductSeoRecord,
}

fn empty_vars() {
  dict.new()
}

/// Apply the dispatcher-level `record_log_drafts` to the outcome. Tests that
/// exercise `bulk_operations.process_mutation` directly need this so log-buffer
/// assertions still see the drafts the module emitted; centralized recording
/// is the dispatcher's responsibility post-refactor.
fn record_drafts(
  outcome: mutation_helpers.MutationOutcome,
  request_path: String,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      empty_vars(),
      outcome.log_drafts,
    )
  mutation_helpers.MutationOutcome(
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

fn run_mutation_import_validator(inner_mutation: String) {
  let request_path = "/admin/api/2026-04/graphql.json"
  let upload_path = "/bulk/validators.jsonl"
  let source =
    store.stage_staged_upload_content(
      store.new(),
      upload_path,
      "{\"input\":{}}\n",
    )
  let document =
    "mutation BulkImport($mutation: String!, $path: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) { bulkOperation { id status type } userErrors { field message code } } }"
  let variables =
    dict.from_list([
      #("mutation", root_field.StringVal(inner_mutation)),
      #("path", root_field.StringVal(upload_path)),
    ])
  bulk_operations.process_mutation(
    source,
    synthetic_identity.new(),
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

fn run_query_mutation(query_string: String) -> String {
  let document =
    "mutation { bulkOperationRunQuery(query: \""
    <> query_string
    <> "\") { bulkOperation { id status } userErrors { field message code } } }"
  let outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      empty_vars(),
      empty_upstream_context(),
    )
  json.to_string(outcome.data)
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
  assert bulk_operations.is_bulk_operations_mutation_root(
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

pub fn run_query_returns_created_operation_and_stages_terminal_log_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ products { edges { node { id } } } }\") { bulkOperation { id status type objectCount rootObjectCount fileSize url partialDataUrl query } userErrors { field message code } } }"
  let outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
      empty_upstream_context(),
    )
  let outcome = record_drafts(outcome, request_path, document)
  let response = json.to_string(outcome.data)
  assert string.contains(
    response,
    "\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/1\",\"status\":\"CREATED\",\"type\":\"QUERY\"",
  )
  assert string.contains(response, "\"objectCount\":\"0\"")
  assert string.contains(response, "\"rootObjectCount\":\"0\"")
  assert string.contains(response, "\"fileSize\":null")
  assert string.contains(response, "\"url\":null")
  assert string.contains(response, "\"partialDataUrl\":null")
  assert string.contains(response, "\"userErrors\":[]")
  assert outcome.staged_resource_ids == ["gid://shopify/BulkOperation/1"]
  let read_after =
    run(
      outcome.store,
      "{ bulkOperation(id: \"gid://shopify/BulkOperation/1\") { id status type objectCount rootObjectCount fileSize url partialDataUrl query } }",
    )
  assert string.contains(
    read_after,
    "\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/1\",\"status\":\"COMPLETED\",\"type\":\"QUERY\"",
  )
  assert string.contains(read_after, "\"fileSize\":\"0\"")
  assert string.contains(
    read_after,
    "\"url\":\"https://shopify-draft-proxy.local",
  )
  assert list.length(store.get_log(outcome.store)) == 1
}

pub fn run_query_returns_operation_in_progress_for_non_terminal_query_test() {
  let running =
    bulk_operation(
      "gid://shopify/BulkOperation/701",
      "RUNNING",
      "QUERY",
      "2026-04-27T00:00:04Z",
    )
  let #(_, source) = store.stage_bulk_operation(store.new(), running)
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ products { edges { node { id } } } }\") { bulkOperation { id status type } userErrors { field message code } } }"
  let outcome =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      empty_vars(),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/701.\",\"code\":\"OPERATION_IN_PROGRESS\"}]}}}"
  assert outcome.staged_resource_ids == []
  let assert [_] = store.list_effective_bulk_operations(outcome.store)
}

pub fn run_query_canceling_operation_still_blocks_new_query_test() {
  let running =
    bulk_operation(
      "gid://shopify/BulkOperation/711",
      "RUNNING",
      "QUERY",
      "2026-04-27T00:00:04Z",
    )
  let #(_, source) = store.stage_bulk_operation(store.new(), running)
  let cancel =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { bulkOperationCancel(id: \"gid://shopify/BulkOperation/711\") { bulkOperation { id status } userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ products { edges { node { id } } } }\") { bulkOperation { id status type } userErrors { field message code } } }"
  let next =
    bulk_operations.process_mutation(
      cancel.store,
      cancel.identity,
      "/admin/api/2026-04/graphql.json",
      document,
      empty_vars(),
      empty_upstream_context(),
    )

  assert json.to_string(next.data)
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/711.\",\"code\":\"OPERATION_IN_PROGRESS\"}]}}}"
  let assert Some(operation) =
    store.get_effective_bulk_operation_by_id(
      next.store,
      "gid://shopify/BulkOperation/711",
    )
  assert operation.status == "CANCELING"
}

pub fn run_query_accepts_group_objects_true_false_and_default_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let fields =
    "bulkOperation { id status type objectCount rootObjectCount fileSize url partialDataUrl query } userErrors { field message code }"
  let query = "{ products { edges { node { id } } } }"
  let document =
    "mutation { default: bulkOperationRunQuery(query: \""
    <> query
    <> "\") { "
    <> fields
    <> " } explicitTrue: bulkOperationRunQuery(query: \""
    <> query
    <> "\", groupObjects: true) { "
    <> fields
    <> " } explicitFalse: bulkOperationRunQuery(query: \""
    <> query
    <> "\", groupObjects: false) { "
    <> fields
    <> " } }"
  let outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(outcome.data)
  assert string.contains(
    response,
    "\"default\":{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/1\",\"status\":\"CREATED\",\"type\":\"QUERY\"",
  )
  assert string.contains(
    response,
    "\"explicitTrue\":{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/2\",\"status\":\"CREATED\",\"type\":\"QUERY\"",
  )
  assert string.contains(
    response,
    "\"explicitFalse\":{\"bulkOperation\":{\"id\":\"gid://shopify/BulkOperation/3\",\"status\":\"CREATED\",\"type\":\"QUERY\"",
  )
  assert string.contains(response, "\"default\":{")
  assert string.contains(response, "\"explicitTrue\":{")
  assert string.contains(response, "\"explicitFalse\":{")
  assert string.contains(response, "\"userErrors\":[]")
  assert !string.contains(response, "groupObjects is not supported")
  assert outcome.staged_resource_ids
    == [
      "gid://shopify/BulkOperation/1",
      "gid://shopify/BulkOperation/2",
      "gid://shopify/BulkOperation/3",
    ]
}

pub fn run_query_exports_product_jsonl_and_metadata_test() {
  let source =
    store.new()
    |> store.upsert_base_products([
      product_record("gid://shopify/Product/1", "First Board"),
      product_record("gid://shopify/Product/2", "Second Board"),
    ])
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ products { edges { node { id title } } } }\") { bulkOperation { id status type objectCount rootObjectCount fileSize url query } userErrors { field message } } }"
  let outcome =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(outcome.data)
  assert string.contains(response, "\"status\":\"CREATED\"")
  assert string.contains(response, "\"objectCount\":\"0\"")
  assert string.contains(response, "\"rootObjectCount\":\"0\"")
  assert string.contains(response, "\"fileSize\":null")
  assert string.contains(response, "\"url\":null")
  let assert [operation_id, ..] = outcome.staged_resource_ids
  let read_after =
    run(
      outcome.store,
      "{ bulkOperation(id: \""
        <> operation_id
        <> "\") { id status objectCount rootObjectCount fileSize url query } }",
    )
  assert string.contains(read_after, "\"status\":\"COMPLETED\"")
  assert string.contains(read_after, "\"objectCount\":\"2\"")
  assert string.contains(read_after, "\"rootObjectCount\":\"2\"")
  let assert Some(jsonl) =
    store.get_effective_bulk_operation_result_jsonl(outcome.store, operation_id)
  assert jsonl
    == "{\"id\":\"gid://shopify/Product/1\",\"title\":\"First Board\"}\n{\"id\":\"gid://shopify/Product/2\",\"title\":\"Second Board\"}\n"
}

pub fn run_query_without_connection_returns_shopify_error_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { bulkOperationRunQuery(query: \"{ shop { id } }\") { bulkOperation { id } userErrors { field message code } } }"
  let outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Bulk queries must contain at least one connection.\",\"code\":\"INVALID\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_query_empty_string_returns_invalid_bulk_query_error_test() {
  let response = run_query_mutation("")

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Invalid bulk query: syntax error, unexpected end of file\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_rejects_top_level_node_with_shopify_error_test() {
  let response =
    run_query_mutation("{ node(id: \\\"gid://shopify/Product/1\\\") { id } }")

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Bulk queries cannot contain a top level `node` field.\",\"code\":\"INVALID\"},{\"field\":[\"query\"],\"message\":\"Bulk queries must contain at least one connection.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_rejects_nodes_selection_with_shopify_error_test() {
  let response = run_query_mutation("{ products { nodes { id title } } }")

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: 'products { edges { node {'. Selecting via 'nodes' is not supported. Invalid connection fields: 'products'.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_rejects_more_than_five_connections_test() {
  let response =
    run_query_mutation(
      "{ products { edges { node { id variants { edges { node { id } } } metafields { edges { node { id } } } collections { edges { node { id } } } media { edges { node { id } } } sellingPlanGroups { edges { node { id } } } } } } }",
    )

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Bulk queries cannot contain more than 5 connections.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_rejects_connection_nesting_depth_greater_than_two_test() {
  let response =
    run_query_mutation(
      "{ collections { edges { node { id products { edges { node { id variants { edges { node { id metafields { edges { node { id } } } } } } } } } } } } }",
    )

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Bulk queries cannot contain connections with a nesting depth greater than 2.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_rejects_nested_connection_without_parent_id_test() {
  let response =
    run_query_mutation(
      "{ products { edges { node { title variants { edges { node { id } } } } } } }",
    )

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': products.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_query_accumulates_multiple_admin_query_errors_test() {
  let response =
    run_query_mutation(
      "{ products { nodes { variants { edges { node { id } } } } } }",
    )

  assert response
    == "{\"data\":{\"bulkOperationRunQuery\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: 'products { edges { node {'. Selecting via 'nodes' is not supported. Invalid connection fields: 'products'.\",\"code\":\"INVALID\"},{\"field\":[\"query\"],\"message\":\"The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': products.\",\"code\":\"INVALID\"}]}}}"
}

pub fn run_mutation_missing_args_use_valid_base_user_error_codes_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let missing_mutation =
    "mutation { bulkOperationRunMutation(stagedUploadPath: \"/bulk/missing.jsonl\") { bulkOperation { id status } userErrors { field message code } } }"
  let missing_path =
    "mutation { bulkOperationRunMutation(mutation: \"mutation { productCreate(product: $product) { product { id } } }\") { bulkOperation { id status } userErrors { field message code } } }"

  let missing_mutation_outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      missing_mutation,
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_path_outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      missing_path,
      empty_vars(),
      empty_upstream_context(),
    )

  assert json.to_string(missing_mutation_outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Bulk mutation is required.\",\"code\":\"INVALID_MUTATION\"}]}}}"
  assert missing_mutation_outcome.staged_resource_ids == []
  assert json.to_string(missing_path_outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Staged upload path is required.\",\"code\":\"INVALID_STAGED_UPLOAD_FILE\"}]}}}"
  assert missing_path_outcome.staged_resource_ids == []
}

pub fn run_mutation_missing_upload_returns_no_such_file_user_error_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let inner =
    "mutation($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }"
  let document =
    "mutation BulkImport($mutation: String!, $path: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) { bulkOperation { id status type objectCount rootObjectCount fileSize url query } userErrors { field message code } } }"
  let variables =
    dict.from_list([
      #("mutation", root_field.StringVal(inner)),
      #("path", root_field.StringVal("/missing.jsonl")),
    ])
  let outcome =
    bulk_operations.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      variables,
      empty_upstream_context(),
    )
  let response = json.to_string(outcome.data)
  assert response
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.\",\"code\":\"NO_SUCH_FILE\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_inner_parse_error_matches_shopify_validator_test() {
  let outcome = run_mutation_import_validator("mutation { not parseable")
  let response = json.to_string(outcome.data)
  assert string.contains(response, "\"bulkOperation\":null")
  assert string.contains(
    response,
    "\"field\":null,\"message\":\"Failed to parse the mutation - ",
  )
  assert string.contains(response, "\"code\":\"INVALID_MUTATION\"")
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_query_document_matches_shopify_validator_test() {
  let outcome =
    run_mutation_import_validator(
      "query { products { edges { node { id } } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Invalid operation type. Only `mutation` operations are supported.\",\"code\":\"INVALID_MUTATION\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_multiple_top_level_fields_matches_shopify_validator_test() {
  let outcome =
    run_mutation_import_validator(
      "mutation { productCreate(input: $i) { product { id } } productUpdate(input: $j) { product { id } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"mutation\"],\"message\":\"You must specify a single top level mutation.\",\"code\":null}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_disallowed_inner_root_matches_shopify_validator_test() {
  let inner =
    "mutation Probe($mutation: String!, $stagedUploadPath: String!, $clientIdentifier: String) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath, clientIdentifier: $clientIdentifier) { bulkOperation { id } userErrors { field message } } }"
  let outcome = run_mutation_import_validator(inner)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"mutation\"],\"message\":\"You must use an allowed mutation name.\",\"code\":null}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_multiple_connections_matches_shopify_validator_test() {
  let outcome =
    run_mutation_import_validator(
      "mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { variants(first: 1) { edges { node { id } } } media(first: 1) { nodes { id } } } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"mutation\"],\"message\":\"Bulk mutations cannot contain more than 1 connection.\",\"code\":null}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_nested_connections_matches_shopify_validator_test() {
  let outcome =
    run_mutation_import_validator(
      "mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { variants(first: 1) { edges { node { metafields(first: 1) { nodes { id } } } } } } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":[\"mutation\"],\"message\":\"Bulk mutations cannot contain more than 1 connection.\",\"code\":null},{\"field\":[\"mutation\"],\"message\":\"Bulk mutations cannot contain connections with a nesting depth greater than 1.\",\"code\":null}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn run_mutation_product_create_import_stages_product_and_result_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let inner =
    "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle status } userErrors { field message } } }"
  let upload_path = "/bulk/products.jsonl"
  let source =
    store.stage_staged_upload_content(
      store.new(),
      upload_path,
      "{\"product\":{\"title\":\"Bulk Created Board\",\"vendor\":\"Hermes\",\"status\":\"DRAFT\"}}\n{\"product\":{\"title\":\"\",\"vendor\":\"Hermes\",\"status\":\"DRAFT\"}}\n",
    )
  let document =
    "mutation BulkImport($mutation: String!, $path: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) { bulkOperation { id status type objectCount rootObjectCount fileSize url query } userErrors { field message } } }"
  let variables =
    dict.from_list([
      #("mutation", root_field.StringVal(inner)),
      #("path", root_field.StringVal(upload_path)),
    ])
  let outcome =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      document,
      variables,
      empty_upstream_context(),
    )
  let response = json.to_string(outcome.data)
  assert string.contains(response, "\"status\":\"CREATED\"")
  assert string.contains(response, "\"objectCount\":\"0\"")
  assert string.contains(response, "\"rootObjectCount\":\"0\"")
  assert string.contains(response, "\"fileSize\":null")
  assert string.contains(response, "\"url\":null")
  assert string.contains(response, "\"userErrors\":[]")
  let assert [operation_id, ..] = outcome.staged_resource_ids
  let read_after =
    run(
      outcome.store,
      "{ bulkOperation(id: \""
        <> operation_id
        <> "\") { id status type objectCount rootObjectCount fileSize url query } }",
    )
  assert string.contains(read_after, "\"status\":\"COMPLETED\"")
  assert string.contains(read_after, "\"objectCount\":\"1\"")
  assert string.contains(read_after, "\"rootObjectCount\":\"1\"")
  let assert Ok(product) =
    store.list_effective_products(outcome.store)
    |> list.find(fn(record) { record.title == "Bulk Created Board" })
  assert product.title == "Bulk Created Board"
  let assert Some(jsonl) =
    store.get_effective_bulk_operation_result_jsonl(outcome.store, operation_id)
  assert string.contains(jsonl, "\"line\":1")
  assert string.contains(jsonl, "\"line\":2")
  assert string.contains(jsonl, "\"productCreate\"")
  assert string.contains(jsonl, "\"Bulk Created Board\"")
  assert string.contains(jsonl, "Title can't be blank")
  let assert [
    mutation_helpers.LogDraft(
      operation_name: Some("ProductCreate"),
      root_fields: ["productCreate"],
      primary_root_field: Some("productCreate"),
      query: Some(log_query),
      variables: Some(log_variables),
      staged_resource_ids: [product_id, ..],
      status: store.Staged,
      ..,
    ),
  ] = outcome.log_drafts
  assert log_query == inner
  assert product_id == product.id
  let assert Ok(root_field.ObjectVal(log_product)) =
    dict.get(log_variables, "product")
  let assert Ok(root_field.StringVal("Bulk Created Board")) =
    dict.get(log_product, "title")
}

pub fn run_mutation_returns_operation_in_progress_for_non_terminal_mutation_test() {
  let running =
    bulk_operation(
      "gid://shopify/BulkOperation/801",
      "RUNNING",
      "MUTATION",
      "2026-04-27T00:00:04Z",
    )
  let upload_path = "/bulk/in-progress-products.jsonl"
  let source =
    store.new()
    |> store.stage_bulk_operation(running)
    |> fn(pair) { pair.1 }
    |> store.stage_staged_upload_content(
      upload_path,
      "{\"product\":{\"title\":\"Blocked Bulk Create\"}}\n",
    )
  let inner =
    "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }"
  let document =
    "mutation BulkImport($mutation: String!, $path: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) { bulkOperation { id status type } userErrors { field message code } } }"
  let variables =
    dict.from_list([
      #("mutation", root_field.StringVal(inner)),
      #("path", root_field.StringVal(upload_path)),
    ])
  let outcome =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      document,
      variables,
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"bulkOperationRunMutation\":{\"bulkOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"A bulk mutation operation for this app and shop is already in progress: gid://shopify/BulkOperation/801.\",\"code\":\"OPERATION_IN_PROGRESS\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert store.list_effective_products(outcome.store) == []
  let assert [_] = store.list_effective_bulk_operations(outcome.store)
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
  let outcome =
    bulk_operations.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { running: bulkOperationCancel(id: \"gid://shopify/BulkOperation/401\") { bulkOperation { id status completedAt } userErrors { field message } } terminal: bulkOperationCancel(id: \"gid://shopify/BulkOperation/402\") { bulkOperation { id status } userErrors { field message } } missing: bulkOperationCancel(id: \"gid://shopify/BulkOperation/0\") { bulkOperation { id } userErrors { field message } } }",
      empty_vars(),
      empty_upstream_context(),
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

fn product_record(id: String, title: String) -> ProductRecord {
  ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: string.lowercase(string.replace(title, " ", "-")),
    status: "ACTIVE",
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
  )
}

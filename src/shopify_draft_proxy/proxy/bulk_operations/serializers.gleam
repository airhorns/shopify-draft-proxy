//// Shared bulk-operations serializers.
//// Mirrors the locally staged foundation of `src/proxy/bulk-operations.ts`.
////
//// This pass ports the BulkOperation state/read/cancel/run-query/import
//// foundation: singular reads, catalog reads with cursor windows, current
//// operation derivation, local `bulkOperationCancel`, product/productVariant
//// JSONL query exports, and local `bulkOperationRunMutation` replay for
//// product-domain inner mutations.

import gleam/json.{type Json}
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcString, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/state/types.{
  type BulkOperationRecord, BulkOperationRecord,
}

@internal
pub fn project_bulk_operation(
  operation: BulkOperationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(ss), ..) -> {
      let SelectionSet(selections: selections, ..) = ss
      project_graphql_value(
        bulk_operation_source(operation),
        selections,
        fragments,
      )
    }
    _ -> json.object([])
  }
}

fn bulk_operation_source(operation: BulkOperationRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("BulkOperation")),
    #("id", SrcString(operation.id)),
    #("status", SrcString(operation.status)),
    #("type", SrcString(operation.type_)),
    #("errorCode", graphql_helpers.option_string_source(operation.error_code)),
    #("createdAt", SrcString(operation.created_at)),
    #(
      "completedAt",
      graphql_helpers.option_string_source(operation.completed_at),
    ),
    #("objectCount", SrcString(operation.object_count)),
    #("rootObjectCount", SrcString(operation.root_object_count)),
    #("fileSize", graphql_helpers.option_string_source(operation.file_size)),
    #("url", graphql_helpers.option_string_source(operation.url)),
    #(
      "partialDataUrl",
      graphql_helpers.option_string_source(operation.partial_data_url),
    ),
    #("query", graphql_helpers.option_string_source(operation.query)),
  ])
}

@internal
pub fn created_bulk_operation_response(
  operation: BulkOperationRecord,
) -> BulkOperationRecord {
  BulkOperationRecord(
    ..operation,
    status: "CREATED",
    error_code: None,
    completed_at: None,
    object_count: "0",
    root_object_count: "0",
    file_size: None,
    url: None,
    partial_data_url: None,
  )
}

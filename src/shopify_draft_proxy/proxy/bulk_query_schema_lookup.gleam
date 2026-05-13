//// Indexed lookup for the captured Admin GraphQL output schema subset that
//// bulkOperationRunQuery validation needs.

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/proxy/bulk_query_schema.{
  type BulkSchemaFieldKind, BulkSchemaField, BulkSchemaLeaf,
}
import shopify_draft_proxy/proxy/bulk_query_schema_data

pub opaque type BulkQuerySchema {
  BulkQuerySchema(
    captured_at: String,
    captured_api_version: String,
    fields: Dict(String, BulkSchemaFieldKind),
  )
}

pub fn default_schema() -> BulkQuerySchema {
  let fields =
    bulk_query_schema_data.fields()
    |> list.fold(dict.new(), fn(acc, field) {
      let BulkSchemaField(parent_type:, name:, kind:) = field
      dict.insert(acc, key(parent_type, name), kind)
    })

  BulkQuerySchema(
    captured_at: bulk_query_schema_data.captured_at,
    captured_api_version: bulk_query_schema_data.captured_api_version,
    fields:,
  )
}

pub fn captured_at(schema: BulkQuerySchema) -> String {
  schema.captured_at
}

pub fn captured_api_version(schema: BulkQuerySchema) -> String {
  schema.captured_api_version
}

pub fn get_field_kind(
  schema: BulkQuerySchema,
  parent_type: String,
  field_name: String,
) -> Option(BulkSchemaFieldKind) {
  case dict.get(schema.fields, key(parent_type, field_name)) {
    Ok(kind) -> Some(kind)
    Error(_) -> None
  }
}

pub fn field_kind(
  schema: BulkQuerySchema,
  parent_type: String,
  field_name: String,
) -> BulkSchemaFieldKind {
  case get_field_kind(schema, parent_type, field_name) {
    Some(kind) -> kind
    None -> BulkSchemaLeaf
  }
}

fn key(parent_type: String, field_name: String) -> String {
  parent_type <> "\n" <> field_name
}

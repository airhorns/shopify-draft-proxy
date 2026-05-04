//// Indexed lookup for the captured mutation/input-object schema.
////
//// The captured data lives in `mutation_schema_data.gleam` (a
//// generated source module). That module exposes the schema as plain
//// `List(SchemaMutation)` / `List(SchemaInputObject)` values. This
//// module turns those lists into `Dict`-backed lookups keyed by name,
//// so callers can resolve a mutation or input-object shape in O(1).
////
//// Kept separate from `mutation_schema.gleam` to avoid an import
//// cycle: the generated data module has to import the type
//// definitions, so the type-defining module cannot itself import the
//// data module.

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/proxy/mutation_schema.{
  type SchemaInputObject, type SchemaMutation,
}
import shopify_draft_proxy/proxy/mutation_schema_data

pub opaque type MutationSchema {
  MutationSchema(
    captured_at: String,
    captured_api_version: String,
    mutations: Dict(String, SchemaMutation),
    input_objects: Dict(String, SchemaInputObject),
  )
}

/// Build the default schema lookup from the bundled generated data.
/// Cheap to call — a couple of hundred dict inserts. Callers that
/// validate every request should hold onto the returned value rather
/// than rebuilding it per request.
pub fn default_schema() -> MutationSchema {
  let mutations =
    list.fold(mutation_schema_data.mutations(), dict.new(), fn(acc, m) {
      dict.insert(acc, m.name, m)
    })
  let input_objects =
    list.fold(mutation_schema_data.input_objects(), dict.new(), fn(acc, io) {
      dict.insert(acc, io.name, io)
    })
  MutationSchema(
    captured_at: mutation_schema_data.captured_at,
    captured_api_version: mutation_schema_data.captured_api_version,
    mutations:,
    input_objects:,
  )
}

pub fn captured_at(schema: MutationSchema) -> String {
  schema.captured_at
}

pub fn captured_api_version(schema: MutationSchema) -> String {
  schema.captured_api_version
}

/// Look up a Mutation field by its API name (e.g. `"productCreate"`).
pub fn get_mutation(
  schema: MutationSchema,
  name: String,
) -> Option(SchemaMutation) {
  case dict.get(schema.mutations, name) {
    Ok(m) -> Some(m)
    Error(_) -> None
  }
}

/// Look up an input object by its captured name (e.g. `"ProductInput"`).
pub fn get_input_object(
  schema: MutationSchema,
  name: String,
) -> Option(SchemaInputObject) {
  case dict.get(schema.input_objects, name) {
    Ok(io) -> Some(io)
    Error(_) -> None
  }
}

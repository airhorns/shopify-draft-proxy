//// Read-path tests for the minimal `proxy/metaobject_definitions`
//// stub. The stub returns null for every singular root and an empty
//// connection for every plural root — this guards that contract on
//// both compile targets.

import gleam/json
import shopify_draft_proxy/proxy/metaobject_definitions

fn run(query: String) -> String {
  let assert Ok(data) =
    metaobject_definitions.handle_metaobject_definitions_query(query)
  json.to_string(data)
}

// ---------- predicate ----------

pub fn is_metaobject_definitions_query_root_test() {
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobject",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectByHandle",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjects",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectDefinition",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectDefinitionByType",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectDefinitions",
  )
  assert !metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectCreate",
  )
  assert !metaobject_definitions.is_metaobject_definitions_query_root("shop")
}

// ---------- singular root nulls ----------

pub fn metaobject_returns_null_test() {
  let result = run("{ metaobject(id: \"gid://shopify/Metaobject/1\") { id } }")
  assert result == "{\"metaobject\":null}"
}

pub fn metaobject_by_handle_returns_null_test() {
  let result =
    run(
      "{ metaobjectByHandle(handle: { type: \"feature\", handle: \"x\" }) { id } }",
    )
  assert result == "{\"metaobjectByHandle\":null}"
}

pub fn metaobject_definition_returns_null_test() {
  let result =
    run(
      "{ metaobjectDefinition(id: \"gid://shopify/MetaobjectDefinition/1\") { id } }",
    )
  assert result == "{\"metaobjectDefinition\":null}"
}

pub fn metaobject_definition_by_type_returns_null_test() {
  let result = run("{ metaobjectDefinitionByType(type: \"feature\") { id } }")
  assert result == "{\"metaobjectDefinitionByType\":null}"
}

// ---------- connection roots empty ----------

pub fn metaobjects_returns_empty_connection_test() {
  let result =
    run(
      "{ metaobjects(first: 10, type: \"feature\") { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"metaobjects\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn metaobject_definitions_returns_empty_connection_test() {
  let result =
    run(
      "{ metaobjectDefinitions(first: 10) { nodes { id type } edges { cursor } } }",
    )
  assert result == "{\"metaobjectDefinitions\":{\"nodes\":[],\"edges\":[]}}"
}

// ---------- envelope ----------

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(data) =
    metaobject_definitions.process(
      "{ metaobject(id: \"gid://shopify/Metaobject/1\") { id } }",
    )
  assert json.to_string(data) == "{\"data\":{\"metaobject\":null}}"
}

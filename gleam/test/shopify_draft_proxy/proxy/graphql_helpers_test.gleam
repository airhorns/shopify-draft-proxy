import gleam/dict
import gleam/json
import gleam/option.{Some}
import shopify_draft_proxy/graphql/ast.{Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  SrcBool, SrcInt, SrcList, SrcNull, SrcString,
}

/// Pull the selection set from the first root field of `{ root { … } }`.
/// Tests project against the inner selections.
fn inner_selections(document: String) -> List(ast.Selection) {
  let assert Ok(fields) = root_field.get_root_fields(document)
  let assert [first, ..] = fields
  case first {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> panic as "first root field has no selection set"
  }
}

fn project(source: graphql_helpers.SourceValue, document: String) -> String {
  let selections = inner_selections(document)
  let fragments = graphql_helpers.get_document_fragments(document)
  json.to_string(graphql_helpers.project_graphql_value(
    source,
    selections,
    fragments,
  ))
}

pub fn projects_scalar_fields_test() {
  let source =
    graphql_helpers.src_object([
      #("id", SrcString("gid://Foo/1")),
      #("count", SrcInt(7)),
      #("active", SrcBool(True)),
    ])
  assert project(source, "{ root { id count active } }")
    == "{\"id\":\"gid://Foo/1\",\"count\":7,\"active\":true}"
}

pub fn missing_field_becomes_null_test() {
  let source = graphql_helpers.src_object([#("id", SrcString("a"))])
  assert project(source, "{ root { id missing } }")
    == "{\"id\":\"a\",\"missing\":null}"
}

pub fn typename_falls_back_to_null_test() {
  let source = graphql_helpers.src_object([])
  assert project(source, "{ root { __typename } }") == "{\"__typename\":null}"
}

pub fn typename_uses_source_when_present_test() {
  let source =
    graphql_helpers.src_object([#("__typename", SrcString("Widget"))])
  assert project(source, "{ root { __typename } }")
    == "{\"__typename\":\"Widget\"}"
}

pub fn aliases_become_response_keys_test() {
  let source = graphql_helpers.src_object([#("id", SrcString("xyz"))])
  assert project(source, "{ root { renamed: id } }") == "{\"renamed\":\"xyz\"}"
}

pub fn nested_object_is_projected_test() {
  let source =
    graphql_helpers.src_object([
      #(
        "child",
        graphql_helpers.src_object([
          #("inner", SrcInt(1)),
          #("hidden", SrcString("nope")),
        ]),
      ),
    ])
  assert project(source, "{ root { child { inner } } }")
    == "{\"child\":{\"inner\":1}}"
}

pub fn list_is_projected_elementwise_test() {
  let source =
    graphql_helpers.src_object([
      #(
        "items",
        SrcList([
          graphql_helpers.src_object([#("id", SrcInt(1))]),
          graphql_helpers.src_object([#("id", SrcInt(2))]),
        ]),
      ),
    ])
  assert project(source, "{ root { items { id } } }")
    == "{\"items\":[{\"id\":1},{\"id\":2}]}"
}

pub fn nodes_synthesised_from_edges_test() {
  let source =
    graphql_helpers.src_object([
      #(
        "edges",
        SrcList([
          graphql_helpers.src_object([
            #("node", graphql_helpers.src_object([#("id", SrcInt(11))])),
          ]),
          graphql_helpers.src_object([
            #("node", graphql_helpers.src_object([#("id", SrcInt(22))])),
          ]),
        ]),
      ),
    ])
  assert project(source, "{ root { nodes { id } } }")
    == "{\"nodes\":[{\"id\":11},{\"id\":22}]}"
}

pub fn inline_fragment_with_matching_typename_inlines_test() {
  let source =
    graphql_helpers.src_object([
      #("__typename", SrcString("Widget")),
      #("id", SrcString("w-1")),
    ])
  assert project(source, "{ root { ... on Widget { id } } }")
    == "{\"id\":\"w-1\"}"
}

pub fn inline_fragment_with_mismatched_typename_skipped_test() {
  let source =
    graphql_helpers.src_object([
      #("__typename", SrcString("Widget")),
      #("id", SrcString("w-1")),
    ])
  assert project(source, "{ root { ... on Sprocket { id } } }") == "{}"
}

pub fn inline_fragment_without_typename_in_source_inlines_test() {
  let source = graphql_helpers.src_object([#("id", SrcString("w-1"))])
  assert project(source, "{ root { ... on Widget { id } } }")
    == "{\"id\":\"w-1\"}"
}

pub fn fragment_spread_inlines_definition_test() {
  let source =
    graphql_helpers.src_object([
      #("__typename", SrcString("Widget")),
      #("id", SrcString("w-1")),
      #("name", SrcString("ratchet")),
    ])
  assert project(
      source,
      "fragment Bits on Widget { id name } { root { ...Bits } }",
    )
    == "{\"id\":\"w-1\",\"name\":\"ratchet\"}"
}

pub fn fragment_spread_with_unknown_name_drops_test() {
  let source = graphql_helpers.src_object([#("id", SrcString("w-1"))])
  assert project(source, "{ root { ...UnknownFragment id } }")
    == "{\"id\":\"w-1\"}"
}

pub fn null_source_passes_through_test() {
  assert project(SrcNull, "{ root { id } }") == "null"
}

pub fn non_object_source_passes_through_test() {
  assert project(SrcString("scalar"), "{ root { id } }") == "\"scalar\""
}

pub fn document_fragments_index_by_name_test() {
  let fragments =
    graphql_helpers.get_document_fragments(
      "fragment A on T { x } fragment B on T { y }",
    )
  assert dict.size(fragments) == 2
  assert dict.has_key(fragments, "A")
  assert dict.has_key(fragments, "B")
}

pub fn document_fragments_empty_when_no_fragments_test() {
  let fragments = graphql_helpers.get_document_fragments("{ root { id } }")
  assert dict.size(fragments) == 0
}

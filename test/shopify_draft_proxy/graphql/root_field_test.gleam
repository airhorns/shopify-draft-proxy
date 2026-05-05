import gleam/dict
import shopify_draft_proxy/graphql/ast.{Field}
import shopify_draft_proxy/graphql/root_field.{
  BoolVal, FloatVal, IntVal, ListVal, NoOperationFound, NoRootField, NullVal,
  ObjectVal, ParseFailed, StringVal,
}

pub fn root_field_returns_first_field_test() {
  let assert Ok(Field(name: name, ..)) =
    root_field.get_root_field("{ products { id } }")
  assert name.value == "products"
}

pub fn root_fields_returns_all_top_level_fields_test() {
  let assert Ok(fields) = root_field.get_root_fields("{ a b c }")
  let names =
    fields
    |> root_field_names()
  assert names == ["a", "b", "c"]
}

pub fn root_fields_drops_fragment_spreads_test() {
  let assert Ok(fields) =
    root_field.get_root_fields("fragment F on T { id } query Q { real ...F }")
  let names = root_field_names(fields)
  assert names == ["real"]
}

pub fn root_field_arguments_resolves_literals_test() {
  let assert Ok(args) =
    root_field.get_root_field_arguments(
      "{ products(first: 10, query: \"foo\", active: true, ratio: 1.5, tags: [\"a\", \"b\"], filter: { id: \"1\", limit: null }) { id } }",
      dict.new(),
    )
  assert dict.get(args, "first") == Ok(IntVal(10))
  assert dict.get(args, "query") == Ok(StringVal("foo"))
  assert dict.get(args, "active") == Ok(BoolVal(True))
  assert dict.get(args, "ratio") == Ok(FloatVal(1.5))
  assert dict.get(args, "tags") == Ok(ListVal([StringVal("a"), StringVal("b")]))

  let assert Ok(ObjectVal(filter)) = dict.get(args, "filter")
  assert dict.get(filter, "id") == Ok(StringVal("1"))
  assert dict.get(filter, "limit") == Ok(NullVal)
}

pub fn enum_values_resolve_as_strings_test() {
  let assert Ok(args) =
    root_field.get_root_field_arguments(
      "{ orders(sort: ASCENDING) { id } }",
      dict.new(),
    )
  assert dict.get(args, "sort") == Ok(StringVal("ASCENDING"))
}

pub fn variable_substitution_test() {
  let vars =
    dict.new()
    |> dict.insert("first", IntVal(25))
    |> dict.insert("after", StringVal("cursor-1"))
  let assert Ok(args) =
    root_field.get_root_field_arguments(
      "query Q($first: Int!, $after: String) { products(first: $first, after: $after) { id } }",
      vars,
    )
  assert dict.get(args, "first") == Ok(IntVal(25))
  assert dict.get(args, "after") == Ok(StringVal("cursor-1"))
}

pub fn missing_variable_resolves_to_null_test() {
  let assert Ok(args) =
    root_field.get_root_field_arguments(
      "query Q($x: String) { f(x: $x) }",
      dict.new(),
    )
  assert dict.get(args, "x") == Ok(NullVal)
}

pub fn float_with_exponent_resolves_test() {
  let assert Ok(args) =
    root_field.get_root_field_arguments("{ f(big: 1e10) }", dict.new())
  // The lexer reports "1e10" as a FloatValue; `float.parse` rejects no-decimal
  // forms so we fall back to int parsing. Either way it should be a FloatVal.
  let assert Ok(FloatVal(_)) = dict.get(args, "big")
}

pub fn selection_names_returns_subselection_field_names_test() {
  let assert Ok(field) =
    root_field.get_root_field("{ products { id title handle } }")
  assert root_field.get_selection_names(field) == ["id", "title", "handle"]
}

pub fn selection_names_drops_fragment_spreads_test() {
  let assert Ok(field) =
    root_field.get_root_field("{ products { id ...Pf title } }")
  assert root_field.get_selection_names(field) == ["id", "title"]
}

pub fn selection_names_empty_for_field_without_selection_set_test() {
  let assert Ok(field) = root_field.get_root_field("{ name }")
  assert root_field.get_selection_names(field) == []
}

pub fn no_operation_is_an_error_test() {
  let assert Error(NoOperationFound) =
    root_field.get_root_field("fragment F on T { id }")
}

pub fn empty_root_selection_is_an_error_test() {
  // graphql-js requires a non-empty selection set, so the parser will error
  // out before we even look — surface that as ParseFailed, not NoRootField.
  let assert Error(ParseFailed(_)) = root_field.get_root_field("query Q { }")
}

pub fn invalid_input_propagates_parse_error_test() {
  let assert Error(ParseFailed(_)) = root_field.get_root_field("{ foo(")
}

pub fn no_root_field_when_only_fragment_spread_test() {
  let assert Error(NoRootField) =
    root_field.get_root_field("fragment F on T { id } query Q { ...F }")
}

fn root_field_names(fields) -> List(String) {
  case fields {
    [] -> []
    [Field(name: name, ..), ..rest] -> [name.value, ..root_field_names(rest)]
    [_, ..rest] -> root_field_names(rest)
  }
}

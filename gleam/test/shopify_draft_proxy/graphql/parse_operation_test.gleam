import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/parse_operation.{
  MutationOperation, NoOperationFound, ParseFailed, ParsedOperation,
  QueryOperation, UnsupportedOperation,
}

pub fn shorthand_query_test() {
  let assert Ok(ParsedOperation(
      type_: QueryOperation,
      name: None,
      root_fields: ["me"],
    )) = parse_operation.parse_operation("{ me { id } }")
}

pub fn named_query_test() {
  let assert Ok(ParsedOperation(
      type_: QueryOperation,
      name: Some("GetProducts"),
      root_fields: ["products"],
    )) =
    parse_operation.parse_operation(
      "query GetProducts { products(first: 1) { edges { node { id } } } }",
    )
}

pub fn mutation_test() {
  let assert Ok(ParsedOperation(
      type_: MutationOperation,
      name: Some("ProductCreate"),
      root_fields: ["productCreate"],
    )) =
    parse_operation.parse_operation(
      "mutation ProductCreate($input: ProductInput!) { productCreate(input: $input) { product { id } } }",
    )
}

pub fn multiple_root_fields_test() {
  let assert Ok(ParsedOperation(root_fields: ["a", "b", "c"], ..)) =
    parse_operation.parse_operation("{ a b c }")
}

pub fn fragment_spreads_are_dropped_test() {
  // Mirrors the TS `.filter(kind === Kind.FIELD)`: fragment spreads at the
  // root level do not contribute names.
  let assert Ok(ParsedOperation(root_fields: ["onlyMe"], ..)) =
    parse_operation.parse_operation(
      "fragment Pf on Product { id } query Q { onlyMe ...Pf }",
    )
}

pub fn aliased_root_field_uses_underlying_name_test() {
  // graphql-js `field.name.value` is the unaliased name; we mirror that.
  let assert Ok(ParsedOperation(root_fields: ["foo"], ..)) =
    parse_operation.parse_operation("{ a: foo }")
}

pub fn subscription_is_unsupported_test() {
  let assert Error(UnsupportedOperation("subscription")) =
    parse_operation.parse_operation("subscription S { ticks }")
}

pub fn missing_operation_is_an_error_test() {
  // A document with only a fragment definition has no operation.
  let assert Error(NoOperationFound) =
    parse_operation.parse_operation("fragment F on Product { id }")
}

pub fn parse_failure_is_propagated_test() {
  let assert Error(ParseFailed(_)) =
    parse_operation.parse_operation("{ foo(")
}

pub fn first_operation_wins_when_multiple_definitions_test() {
  // graphql-js's `.find` returns the first OperationDefinition, even when
  // a fragment definition appears earlier in the document.
  let assert Ok(ParsedOperation(
      type_: QueryOperation,
      name: Some("First"),
      root_fields: ["a"],
    )) =
    parse_operation.parse_operation(
      "fragment F on T { id } query First { a } query Second { b }",
    )
}

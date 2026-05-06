//// Tests for the lifted helpers in `proxy/mutation_helpers`.
////
//// These exercise the AST-vs-resolved-arg-dict split, which is the
//// reason the helpers exist as a pair in the first place: only the AST
//// can distinguish "argument omitted" from "literal null" from "unbound
//// variable", and each of those produces a distinct top-level GraphQL
//// error code.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{
  RequiredArgument, build_missing_required_argument_error,
  build_missing_variable_error, build_null_argument_error, read_optional_string,
  read_optional_string_array, validate_mutation_field_against_schema,
  validate_required_field_arguments, validate_required_id_argument,
}
import shopify_draft_proxy/proxy/mutation_schema_lookup

fn parse_field(document: String) -> Selection {
  let assert Ok(field) = root_field.get_root_field(document)
  field
}

fn field_loc(field: Selection) {
  case field {
    Field(loc: loc, ..) -> loc
    _ -> None
  }
}

// ---------- validate_required_field_arguments ----------

pub fn validate_required_arguments_happy_path_test() {
  let document = "mutation { foo(topic: \"x\", uri: \"https://e\") { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [
        RequiredArgument(name: "topic", expected_type: "String!"),
        RequiredArgument(name: "uri", expected_type: "String!"),
      ],
      "mutation",
      document,
    )
  assert errors == []
}

pub fn validate_required_arguments_missing_arg_test() {
  // No `topic` argument supplied at all.
  let document = "mutation { foo(uri: \"https://e\") { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation",
      document,
    )
  assert errors
    == [
      build_missing_required_argument_error(
        "foo",
        "topic",
        "mutation",
        field_loc(field),
        document,
      ),
    ]
}

pub fn validate_required_arguments_multiple_missing_joined_test() {
  let document = "mutation { foo { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [
        RequiredArgument(name: "topic", expected_type: "String!"),
        RequiredArgument(name: "uri", expected_type: "String!"),
      ],
      "mutation",
      document,
    )
  // Joined with ", " in the order the required-arguments list was
  // supplied — matches the TS error envelope.
  assert errors
    == [
      build_missing_required_argument_error(
        "foo",
        "topic, uri",
        "mutation",
        field_loc(field),
        document,
      ),
    ]
}

pub fn validate_required_arguments_null_literal_test() {
  let document = "mutation { foo(topic: null) { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation",
      document,
    )
  assert errors
    == [
      build_null_argument_error(
        "foo",
        "topic",
        "String!",
        "mutation",
        field_loc(field),
        document,
      ),
    ]
}

pub fn validate_required_arguments_unbound_variable_test() {
  // Variable `$t` is referenced but the variables dict has no entry,
  // so it resolves to "missing"/null.
  let document = "mutation Op($t: String!) { foo(topic: $t) { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
      document,
    )
  assert errors == [build_missing_variable_error("t", "String!")]
}

pub fn validate_required_arguments_null_variable_test() {
  // Variable supplied but with a NullVal.
  let document = "mutation Op($t: String!) { foo(topic: $t) { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.from_list([#("t", root_field.NullVal)]),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
      document,
    )
  assert errors == [build_missing_variable_error("t", "String!")]
}

pub fn validate_required_arguments_bound_variable_ok_test() {
  let document = "mutation Op($t: String!) { foo(topic: $t) { id } }"
  let field = parse_field(document)
  let errors =
    validate_required_field_arguments(
      field,
      dict.from_list([#("t", root_field.StringVal("ORDERS_CREATE"))]),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
      document,
    )
  assert errors == []
}

// ---------- validate_required_id_argument ----------

pub fn validate_required_id_argument_literal_id_test() {
  let document = "mutation { fooDelete(id: \"gid://shopify/Foo/1\") { id } }"
  let field = parse_field(document)
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.new(),
      "fooDelete",
      "mutation",
      document,
    )
  assert id == Some("gid://shopify/Foo/1")
  assert errs == []
}

pub fn validate_required_id_argument_missing_test() {
  let document = "mutation { fooDelete { id } }"
  let field = parse_field(document)
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.new(),
      "fooDelete",
      "mutation",
      document,
    )
  assert id == None
  assert errs
    == [
      build_missing_required_argument_error(
        "fooDelete",
        "id",
        "mutation",
        field_loc(field),
        document,
      ),
    ]
}

pub fn validate_required_id_argument_null_literal_test() {
  let document = "mutation { fooDelete(id: null) { id } }"
  let field = parse_field(document)
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.new(),
      "fooDelete",
      "mutation",
      document,
    )
  assert id == None
  assert errs
    == [
      build_null_argument_error(
        "fooDelete",
        "id",
        "ID!",
        "mutation",
        field_loc(field),
        document,
      ),
    ]
}

pub fn validate_required_id_argument_bound_variable_test() {
  let document = "mutation Op($x: ID!) { fooDelete(id: $x) { id } }"
  let field = parse_field(document)
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.from_list([#("x", root_field.StringVal("gid://shopify/Foo/2"))]),
      "fooDelete",
      "mutation Op",
      document,
    )
  assert id == Some("gid://shopify/Foo/2")
  assert errs == []
}

pub fn validate_required_id_argument_unbound_variable_test() {
  let document = "mutation Op($x: ID!) { fooDelete(id: $x) { id } }"
  let field = parse_field(document)
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.new(),
      "fooDelete",
      "mutation Op",
      document,
    )
  assert id == None
  assert errs == [build_missing_variable_error("x", "ID!")]
}

// ---------- error builders ----------

pub fn build_missing_required_argument_error_shape_test() {
  // Without field location info, no `locations` field is emitted.
  let err =
    build_missing_required_argument_error(
      "foo",
      "topic, uri",
      "mutation",
      None,
      "",
    )
  let s = json.to_string(err)
  assert s
    == "{\"message\":\"Field 'foo' is missing required arguments: topic, uri\",\"path\":[\"mutation\",\"foo\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"foo\",\"arguments\":\"topic, uri\"}}"
}

pub fn build_missing_required_argument_error_with_location_test() {
  // With a field location and source body, `locations: [{line, column}]`
  // appears between `message` and `path` — matches live Shopify shape.
  let document = "mutation Op {\n  foo {\n    id\n  }\n}"
  let field = parse_field(document)
  let err =
    build_missing_required_argument_error(
      "foo",
      "topic",
      "mutation Op",
      field_loc(field),
      document,
    )
  let s = json.to_string(err)
  assert s
    == "{\"message\":\"Field 'foo' is missing required arguments: topic\",\"locations\":[{\"line\":2,\"column\":3}],\"path\":[\"mutation Op\",\"foo\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"foo\",\"arguments\":\"topic\"}}"
}

pub fn build_null_argument_error_shape_test() {
  let err =
    build_null_argument_error("foo", "topic", "String!", "mutation", None, "")
  let s = json.to_string(err)
  assert s
    == "{\"message\":\"Argument 'topic' on Field 'foo' has an invalid value (null). Expected type 'String!'.\",\"path\":[\"mutation\",\"foo\",\"topic\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"topic\"}}"
}

pub fn build_missing_variable_error_shape_test() {
  let err = build_missing_variable_error("t", "String!")
  let s = json.to_string(err)
  assert s
    == "{\"message\":\"Variable $t of type String! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":null,\"problems\":[{\"path\":[],\"explanation\":\"Expected value to not be null\"}]}}"
}

// ---------- read_optional_string ----------

pub fn read_optional_string_present_test() {
  let d = dict.from_list([#("name", root_field.StringVal("Alice"))])
  assert read_optional_string(d, "name") == Some("Alice")
}

pub fn read_optional_string_absent_test() {
  assert read_optional_string(dict.new(), "name") == None
}

pub fn read_optional_string_wrong_type_test() {
  // Non-string values should become None — they are silently ignored.
  let d = dict.from_list([#("name", root_field.IntVal(42))])
  assert read_optional_string(d, "name") == None
}

// ---------- read_optional_string_array ----------

pub fn read_optional_string_array_present_test() {
  let d =
    dict.from_list([
      #(
        "tags",
        root_field.ListVal([
          root_field.StringVal("a"),
          root_field.StringVal("b"),
        ]),
      ),
    ])
  assert read_optional_string_array(d, "tags") == Some(["a", "b"])
}

pub fn read_optional_string_array_filters_non_strings_test() {
  // Mixed list — non-strings dropped, mirrors TS filter→map.
  let d =
    dict.from_list([
      #(
        "tags",
        root_field.ListVal([
          root_field.StringVal("a"),
          root_field.IntVal(7),
          root_field.StringVal("b"),
        ]),
      ),
    ])
  assert read_optional_string_array(d, "tags") == Some(["a", "b"])
}

pub fn read_optional_string_array_absent_test() {
  assert read_optional_string_array(dict.new(), "tags") == None
}

pub fn read_optional_string_array_wrong_type_test() {
  let d = dict.from_list([#("tags", root_field.StringVal("not-a-list"))])
  assert read_optional_string_array(d, "tags") == None
}

// ---------- validate_mutation_field_against_schema ----------
//
// These exercise the schema-driven validator against the bundled
// captured schema (`mutation_schema_lookup.default_schema()`).
// We pick mutations whose required-argument shape is stable across
// API versions:
//   - `metafieldsSet(metafields: [MetafieldsSetInput!]!)` —
//     required list-of-required-objects with required `key`, `ownerId`,
//     `value` fields. Drives every list/element-required test.
//   - `priceListCreate(input: PriceListCreateInput!)` —
//     required input arg whose `parent` field is NON_NULL but
//     leniently accepted by real Shopify on a top-level variable.
//   - `productCreateMedia(productId: ID!, media: [CreateMediaInput!]!)` —
//     two required top-level args; used for the "missing args joined
//     with ", "" path.
//   - `productDelete(input: ProductDeleteInput!, synchronous: Boolean = true)` —
//     a NON_NULL arg next to one with a default, to confirm
//     defaults make an arg optional.

fn run_schema_validator(
  document: String,
  variables: List(#(String, root_field.ResolvedValue)),
  operation_name: String,
) -> List(json.Json) {
  let assert Ok(field) = root_field.get_root_field(document)
  validate_mutation_field_against_schema(
    field,
    dict.from_list(variables),
    operation_name,
    "mutation",
    document,
    mutation_schema_lookup.default_schema(),
  )
}

fn rendered(errors: List(json.Json)) -> List(String) {
  list.map(errors, json.to_string)
}

pub fn schema_validator_unknown_mutation_returns_empty_test() {
  // Mutation not in the captured schema: validator stays out of the
  // way — per-handler logic still runs.
  let errors =
    run_schema_validator(
      "mutation { madeUpMutationFoo(input: {}) { id } }",
      [],
      "madeUpMutationFoo",
    )
  assert errors == []
}

pub fn schema_validator_required_arg_missing_test() {
  // metafieldsSet has one required arg `metafields`. Omitting it
  // produces a missingRequiredArguments envelope.
  let errors =
    run_schema_validator(
      "mutation { metafieldsSet { metafields { id } userErrors { field message } } }",
      [],
      "metafieldsSet",
    )
  assert list.length(errors) == 1
  let assert [s] = rendered(errors)
  assert string.contains(
    s,
    "\"message\":\"Field 'metafieldsSet' is missing required arguments: metafields\"",
  )
  assert string.contains(s, "\"code\":\"missingRequiredArguments\"")
  assert string.contains(s, "\"arguments\":\"metafields\"")
}

pub fn schema_validator_multiple_required_args_missing_joined_test() {
  // productCreateMedia has two required args: productId, media.
  // Both omitted ⇒ one envelope, names joined with ", ".
  let errors =
    run_schema_validator(
      "mutation { productCreateMedia { media { id } } }",
      [],
      "productCreateMedia",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"arguments\":\"productId, media\"")
}

pub fn schema_validator_required_arg_null_literal_test() {
  // A literal `null` for a NON_NULL arg ⇒ argumentLiteralsIncompatible.
  let errors =
    run_schema_validator(
      "mutation { metafieldsSet(metafields: null) { metafields { id } userErrors { field message } } }",
      [],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"code\":\"argumentLiteralsIncompatible\"")
  assert string.contains(s, "Expected type '[MetafieldsSetInput!]!'")
}

pub fn schema_validator_default_value_makes_arg_optional_test() {
  // productDelete.synchronous is `Boolean = true` (default) — omitting
  // it should NOT produce an error even though the schema lists it.
  // The required `input` arg is supplied as a literal so we only
  // measure the optional-arg behavior.
  let errors =
    run_schema_validator(
      "mutation { productDelete(input: { id: \"gid://shopify/Product/1\" }) { deletedProductId userErrors { field message } } }",
      [],
      "productDelete",
    )
  assert errors == []
}

pub fn schema_validator_required_variable_non_null_declared_unbound_test() {
  // Variable declared NON_NULL but never supplied ⇒ INVALID_VARIABLE.
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(s, "Variable $m of type [MetafieldsSetInput!]!")
  assert string.contains(s, "\"explanation\":\"Expected value to not be null\"")
}

pub fn schema_validator_required_variable_non_null_declared_null_test() {
  // Variable declared NON_NULL, bound to NullVal ⇒ INVALID_VARIABLE.
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.NullVal)],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"code\":\"INVALID_VARIABLE\"")
}

pub fn schema_validator_nullable_variable_unbound_lenient_test() {
  // A nullable-declared variable bound to a NON_NULL arg passes through
  // to the resolver — Shopify reports it via userErrors, not a top-level
  // INVALID_VARIABLE. Validator must NOT fabricate one.
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [],
      "metafieldsSet",
    )
  assert errors == []
}

pub fn schema_validator_nullable_variable_null_lenient_test() {
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.NullVal)],
      "metafieldsSet",
    )
  assert errors == []
}

pub fn schema_validator_list_element_missing_required_field_test() {
  // Variable bound to a list whose element is missing a NON_NULL field.
  // Real Shopify is strict here ⇒ INVALID_VARIABLE with path [0, "key"].
  let element =
    root_field.ObjectVal(
      dict.from_list([
        #("ownerId", root_field.StringVal("gid://shopify/Product/1")),
        #("value", root_field.StringVal("v")),
        // `key` deliberately missing.
      ]),
    )
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.ListVal([element]))],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(s, "\"path\":[0,\"key\"]")
  assert string.contains(s, "\"explanation\":\"Expected value to not be null\"")
}

pub fn schema_validator_list_element_multiple_missing_fields_aggregated_test() {
  // Multiple required fields missing across one element ⇒ one envelope
  // with multiple `problems` entries.
  let element =
    root_field.ObjectVal(
      dict.from_list([
        #("namespace", root_field.StringVal("custom")),
        // ownerId, key, value all missing.
      ]),
    )
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.ListVal([element]))],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"path\":[0,\"ownerId\"]")
  assert string.contains(s, "\"path\":[0,\"key\"]")
  assert string.contains(s, "\"path\":[0,\"value\"]")
}

pub fn schema_validator_list_multiple_elements_paths_indexed_test() {
  // Two elements, each missing a different required field ⇒ paths
  // carry distinct list indices.
  let element_zero =
    root_field.ObjectVal(
      dict.from_list([
        #("ownerId", root_field.StringVal("gid://shopify/Product/1")),
        #("value", root_field.StringVal("v")),
        // missing `key`
      ]),
    )
  let element_one =
    root_field.ObjectVal(
      dict.from_list([
        #("key", root_field.StringVal("k")),
        #("value", root_field.StringVal("v")),
        // missing `ownerId`
      ]),
    )
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.ListVal([element_zero, element_one]))],
      "metafieldsSet",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "\"path\":[0,\"key\"]")
  assert string.contains(s, "\"path\":[1,\"ownerId\"]")
}

pub fn schema_validator_price_list_parent_missing_invalid_variable_test() {
  // Current Shopify 2026-04 rejects variable-bound PriceListCreateInput
  // without parent before resolver execution.
  let input =
    root_field.ObjectVal(
      dict.from_list([
        #("name", root_field.StringVal("Wholesale")),
        #("currency", root_field.StringVal("USD")),
        // `parent` deliberately missing.
      ]),
    )
  let errors =
    run_schema_validator(
      "mutation Op($input: PriceListCreateInput!) { priceListCreate(input: $input) { priceList { id } userErrors { field message } } }",
      [#("input", input)],
      "priceListCreate",
    )
  let assert [s] = rendered(errors)
  assert string.contains(s, "PriceListCreateInput!")
  assert string.contains(s, "\"path\":[\"parent\"]")
  assert string.contains(s, "Expected value to not be null")
}

pub fn schema_validator_well_formed_variable_no_errors_test() {
  let element =
    root_field.ObjectVal(
      dict.from_list([
        #("ownerId", root_field.StringVal("gid://shopify/Product/1")),
        #("namespace", root_field.StringVal("custom")),
        #("key", root_field.StringVal("note")),
        #("value", root_field.StringVal("hello")),
        #("type", root_field.StringVal("single_line_text_field")),
      ]),
    )
  let errors =
    run_schema_validator(
      "mutation Op($m: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $m) { metafields { id } } }",
      [#("m", root_field.ListVal([element]))],
      "metafieldsSet",
    )
  assert errors == []
}

pub fn schema_validator_literal_inline_object_skipped_test() {
  // Strict NON_NULL checks on inline literal input objects produce
  // false positives against the live runtime (which is more permissive
  // than the introspection schema advertises). The validator skips them
  // by design — the per-handler logic owns shape validation of literals.
  // Confirm: `metafieldsSet` with a literal element missing `key` is
  // NOT flagged here.
  let errors =
    run_schema_validator(
      "mutation { metafieldsSet(metafields: [{ ownerId: \"gid://shopify/Product/1\", value: \"v\" }]) { metafields { id } userErrors { field message } } }",
      [],
      "metafieldsSet",
    )
  assert errors == []
}

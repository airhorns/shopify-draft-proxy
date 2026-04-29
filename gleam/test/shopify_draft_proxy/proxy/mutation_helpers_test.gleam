//// Tests for the lifted helpers in `proxy/mutation_helpers`.
////
//// These exercise the AST-vs-resolved-arg-dict split, which is the
//// reason the helpers exist as a pair in the first place: only the AST
//// can distinguish "argument omitted" from "literal null" from "unbound
//// variable", and each of those produces a distinct top-level GraphQL
//// error code.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{
  RequiredArgument, build_missing_required_argument_error,
  build_missing_variable_error, build_null_argument_error, read_optional_string,
  read_optional_string_array, validate_required_field_arguments,
  validate_required_id_argument,
}

fn parse_field(document: String) -> Selection {
  let assert Ok(field) = root_field.get_root_field(document)
  field
}

// ---------- validate_required_field_arguments ----------

pub fn validate_required_arguments_happy_path_test() {
  let field =
    parse_field(
      "mutation { foo(topic: \"x\", uri: \"https://e\") { id } }",
    )
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
    )
  assert errors == []
}

pub fn validate_required_arguments_missing_arg_test() {
  // No `topic` argument supplied at all.
  let field = parse_field("mutation { foo(uri: \"https://e\") { id } }")
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation",
    )
  assert errors
    == [
      build_missing_required_argument_error("foo", "topic", "mutation"),
    ]
}

pub fn validate_required_arguments_multiple_missing_joined_test() {
  let field = parse_field("mutation { foo { id } }")
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
    )
  // Joined with ", " in the order the required-arguments list was
  // supplied — matches the TS error envelope.
  assert errors
    == [
      build_missing_required_argument_error(
        "foo",
        "topic, uri",
        "mutation",
      ),
    ]
}

pub fn validate_required_arguments_null_literal_test() {
  let field = parse_field("mutation { foo(topic: null) { id } }")
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation",
    )
  assert errors
    == [
      build_null_argument_error("foo", "topic", "String!", "mutation"),
    ]
}

pub fn validate_required_arguments_unbound_variable_test() {
  // Variable `$t` is referenced but the variables dict has no entry,
  // so it resolves to "missing"/null.
  let field =
    parse_field("mutation Op($t: String!) { foo(topic: $t) { id } }")
  let errors =
    validate_required_field_arguments(
      field,
      dict.new(),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
    )
  assert errors == [build_missing_variable_error("t", "String!")]
}

pub fn validate_required_arguments_null_variable_test() {
  // Variable supplied but with a NullVal.
  let field =
    parse_field("mutation Op($t: String!) { foo(topic: $t) { id } }")
  let errors =
    validate_required_field_arguments(
      field,
      dict.from_list([#("t", root_field.NullVal)]),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
    )
  assert errors == [build_missing_variable_error("t", "String!")]
}

pub fn validate_required_arguments_bound_variable_ok_test() {
  let field =
    parse_field("mutation Op($t: String!) { foo(topic: $t) { id } }")
  let errors =
    validate_required_field_arguments(
      field,
      dict.from_list([#("t", root_field.StringVal("ORDERS_CREATE"))]),
      "foo",
      [RequiredArgument(name: "topic", expected_type: "String!")],
      "mutation Op",
    )
  assert errors == []
}

// ---------- validate_required_id_argument ----------

pub fn validate_required_id_argument_literal_id_test() {
  let field =
    parse_field("mutation { fooDelete(id: \"gid://shopify/Foo/1\") { id } }")
  let #(id, errs) =
    validate_required_id_argument(field, dict.new(), "fooDelete", "mutation")
  assert id == Some("gid://shopify/Foo/1")
  assert errs == []
}

pub fn validate_required_id_argument_missing_test() {
  let field = parse_field("mutation { fooDelete { id } }")
  let #(id, errs) =
    validate_required_id_argument(field, dict.new(), "fooDelete", "mutation")
  assert id == None
  assert errs
    == [build_missing_required_argument_error("fooDelete", "id", "mutation")]
}

pub fn validate_required_id_argument_null_literal_test() {
  let field = parse_field("mutation { fooDelete(id: null) { id } }")
  let #(id, errs) =
    validate_required_id_argument(field, dict.new(), "fooDelete", "mutation")
  assert id == None
  assert errs
    == [build_null_argument_error("fooDelete", "id", "ID!", "mutation")]
}

pub fn validate_required_id_argument_bound_variable_test() {
  let field =
    parse_field("mutation Op($x: ID!) { fooDelete(id: $x) { id } }")
  let #(id, errs) =
    validate_required_id_argument(
      field,
      dict.from_list([#("x", root_field.StringVal("gid://shopify/Foo/2"))]),
      "fooDelete",
      "mutation Op",
    )
  assert id == Some("gid://shopify/Foo/2")
  assert errs == []
}

pub fn validate_required_id_argument_unbound_variable_test() {
  let field =
    parse_field("mutation Op($x: ID!) { fooDelete(id: $x) { id } }")
  let #(id, errs) =
    validate_required_id_argument(field, dict.new(), "fooDelete", "mutation Op")
  assert id == None
  assert errs == [build_missing_variable_error("x", "ID!")]
}

// ---------- error builders ----------

pub fn build_missing_required_argument_error_shape_test() {
  let err =
    build_missing_required_argument_error("foo", "topic, uri", "mutation")
  // Spot-check the JSON shape: code + arguments string + path.
  let s = json.to_string(err)
  assert s
    == "{\"message\":\"Field 'foo' is missing required arguments: topic, uri\",\"path\":[\"mutation\",\"foo\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"foo\",\"arguments\":\"topic, uri\"}}"
}

pub fn build_null_argument_error_shape_test() {
  let err = build_null_argument_error("foo", "topic", "String!", "mutation")
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

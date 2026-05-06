//// Helpers shared across mutation handlers.
////
//// Pass 13 introduced AST-level argument validation in
//// `proxy/webhooks.gleam` to mirror the structured top-level GraphQL
//// error envelope TS emits (`extensions.code` =
//// `missingRequiredArguments` / `argumentLiteralsIncompatible` /
//// `INVALID_VARIABLE`). Pass 14 lifts the validator + its three error
//// builders + the resolved-value readers here so future domain
//// handlers don't have to copy them.
////
//// What's here:
//// - `RequiredArgument` and `validate_required_field_arguments` —
////   the generic AST validator. Mirrors `validateRequiredFieldArguments`
////   in `src/proxy/webhooks.ts`.
//// - `validate_required_id_argument` — single-`id`-arg variant used
////   by `*Delete` mutations whose only top-level requirement is `id`.
//// - `build_missing_required_argument_error` /
////   `build_null_argument_error` / `build_missing_variable_error` —
////   the three error builders, directly reusable.
//// - `read_optional_string` / `read_optional_string_array` —
////   resolved-arg readers that ignore non-matching variants. Both
////   `webhooks` and `saved_searches` use these.
//// - `MutationFieldResult` — the shared `{key, payload, staged_resource_ids}`
////   shape used by simple-shape mutation handlers (segments, functions,
////   privacy, gift_cards, localization, media). Domains with extra fields
////   (top-level errors, log-draft toggles, store/identity threading) keep
////   their local types.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Location, type Selection, type Value, Argument,
  BooleanValue, EnumValue, Field, FloatValue, IntValue, ListValue, NullValue,
  ObjectValue, StringValue, VariableValue,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/parser as graphql_parser
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/mutation_schema.{type SchemaMutation}
import shopify_draft_proxy/proxy/mutation_schema_lookup.{type MutationSchema}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Response, Response,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// One required top-level argument on a mutation field. `name` is the
/// argument name as it appears in the schema; `expected_type` is the
/// type string used in error messages (e.g. `"WebhookSubscriptionTopic!"`).
pub type RequiredArgument {
  RequiredArgument(name: String, expected_type: String)
}

/// Validate the AST-level arguments on a mutation field. Returns one
/// JSON error object per problem; an empty list means "all good".
///
/// Mirrors TS `validateRequiredFieldArguments`. The split between
/// "validate against AST" and "execute against resolved arg dict" is
/// intentional and necessary: only the AST distinguishes
/// `omitted` / `literal null` / `unbound variable`, each of which maps
/// to a distinct GraphQL error code.
///
/// `operation_path` is the operation path label
/// (e.g. `"mutation"` / `"mutation Foo"`) — formed from the parsed
/// operation upstream and threaded down here.
pub fn validate_required_field_arguments(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: String,
  required_arguments: List(RequiredArgument),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  let field_loc = field_location(field)
  let #(missing_names, errors) =
    list.fold(required_arguments, #([], []), fn(acc, required) {
      let #(missing, errs) = acc
      case find_argument(arguments, required.name) {
        None -> #(list.append(missing, [required.name]), errs)
        Some(argument) ->
          case argument.value {
            NullValue(..) -> #(
              missing,
              list.append(errs, [
                build_null_argument_error(
                  operation_name,
                  required.name,
                  required.expected_type,
                  operation_path,
                  field_loc,
                  source_body,
                ),
              ]),
            )
            VariableValue(variable: var) ->
              case dict.get(variables, var.name.value) {
                Ok(root_field.NullVal) | Error(_) -> #(
                  missing,
                  list.append(errs, [
                    build_missing_variable_error(
                      var.name.value,
                      required.expected_type,
                    ),
                  ]),
                )
                _ -> #(missing, errs)
              }
            _ -> #(missing, errs)
          }
      }
    })
  case missing_names {
    [] -> errors
    _ -> [
      build_missing_required_argument_error(
        operation_name,
        string.join(missing_names, ", "),
        operation_path,
        field_loc,
        source_body,
      ),
      ..errors
    ]
  }
}

/// Validate the single `id` argument on a `*Delete` mutation field
/// and return the resolved id alongside any top-level errors.
/// Mirrors the per-mutation pattern in `*Delete` TS handlers, where
/// the validator both surfaces structured errors *and* hands the
/// caller the resolved id when validation passed.
///
/// On a literal string id with no errors, returns `#(Some(id), [])`.
/// On any validation failure or unresolvable variable, returns
/// `#(None, [errors...])`.
pub fn validate_required_id_argument(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: String,
  operation_path: String,
  source_body: String,
) -> #(Option(String), List(Json)) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  let field_loc = field_location(field)
  case find_argument(arguments, "id") {
    None -> #(None, [
      build_missing_required_argument_error(
        operation_name,
        "id",
        operation_path,
        field_loc,
        source_body,
      ),
    ])
    Some(argument) ->
      case argument.value {
        NullValue(..) -> #(None, [
          build_null_argument_error(
            operation_name,
            "id",
            "ID!",
            operation_path,
            field_loc,
            source_body,
          ),
        ])
        VariableValue(variable: var) ->
          case dict.get(variables, var.name.value) {
            Ok(root_field.NullVal) | Error(_) -> #(None, [
              build_missing_variable_error(var.name.value, "ID!"),
            ])
            Ok(root_field.StringVal(s)) -> #(Some(s), [])
            _ -> #(None, [])
          }
        _ -> {
          // Literal value or coercible — fall back to the resolved-arg
          // dict to pick up the string.
          let args = case root_field.get_field_arguments(field, variables) {
            Ok(d) -> d
            Error(_) -> dict.new()
          }
          let id = case dict.get(args, "id") {
            Ok(root_field.StringVal(s)) -> Some(s)
            _ -> None
          }
          #(id, [])
        }
      }
  }
}

/// Look up a named argument in a list. Public so domain handlers can
/// inspect specific arguments after validation has passed.
pub fn find_argument(
  arguments: List(Argument),
  name: String,
) -> Option(Argument) {
  case arguments {
    [] -> None
    [first, ..rest] ->
      case first {
        Argument(name: arg_name, ..) ->
          case arg_name.value == name {
            True -> Some(first)
            False -> find_argument(rest, name)
          }
      }
  }
}

/// Build the structured error for one or more missing required
/// arguments. `argument_names_joined` is comma-separated per TS.
pub fn build_missing_required_argument_error(
  operation_name: String,
  argument_names_joined: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '"
        <> operation_name
        <> "' is missing required arguments: "
        <> argument_names_joined,
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", json.array([operation_path, operation_name], json.string)),
      #(
        "extensions",
        json.object([
          #("code", json.string("missingRequiredArguments")),
          #("className", json.string("Field")),
          #("name", json.string(operation_name)),
          #("arguments", json.string(argument_names_joined)),
        ]),
      ),
    ]),
  )
}

/// Build the structured error for an argument bound to a literal
/// `null` AST node.
pub fn build_null_argument_error(
  operation_name: String,
  argument_name: String,
  expected_type: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> argument_name
        <> "' on Field '"
        <> operation_name
        <> "' has an invalid value (null). Expected type '"
        <> expected_type
        <> "'.",
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array([operation_path, operation_name, argument_name], json.string),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("Field")),
          #("argumentName", json.string(argument_name)),
        ]),
      ),
    ]),
  )
}

fn build_invalid_global_id_literal_error(
  operation_name: String,
  argument_name: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [#("message", json.string("Invalid global id ''"))]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array([operation_path, operation_name, argument_name], json.string),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("CoercionError")),
        ]),
      ),
    ]),
  )
}

fn schema_type_is_id(type_) -> Bool {
  case type_ {
    mutation_schema.NonNullType(of: inner) -> schema_type_is_id(inner)
    mutation_schema.NamedType(name: "ID") -> True
    _ -> False
  }
}

/// Build the structured error for an argument bound to a variable
/// that resolved to `null` or wasn't supplied.
pub fn build_missing_variable_error(
  variable_name: String,
  variable_type: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type "
        <> variable_type
        <> " was provided invalid value",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", json.null()),
        #(
          "problems",
          json.preprocessed_array([
            json.object([
              #("path", json.preprocessed_array([])),
              #("explanation", json.string("Expected value to not be null")),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

/// Read an optional string from a resolved-arg dict. Returns `None`
/// if the key is absent or bound to a non-string value.
pub fn read_optional_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

/// Read an optional `[String]` array from a resolved-arg dict.
/// Non-string list elements are dropped silently to mirror the TS
/// `filter`-then-`map` pattern. Returns `None` for absent or
/// non-list values.
pub fn read_optional_string_array(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(List(String)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) ->
      Some(
        list.filter_map(items, fn(value) {
          case value {
            root_field.StringVal(s) -> Ok(s)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn field_location(field: Selection) -> Option(Location) {
  case field {
    Field(loc: loc, ..) -> loc
    _ -> None
  }
}

fn locations_payload(
  field_loc: Option(Location),
  source_body: String,
) -> Option(Json) {
  case field_loc {
    None -> None
    Some(loc) -> {
      let source = graphql_source.new(source_body)
      let computed = graphql_location.get_location(source, position: loc.start)
      Some(
        json.preprocessed_array([
          json.object([
            #("line", json.int(computed.line)),
            #("column", json.int(computed.column)),
          ]),
        ]),
      )
    }
  }
}

// ---------------------------------------------------------------------------
// Schema-driven required-field validation
// ---------------------------------------------------------------------------

/// Validate one mutation field against the captured Admin GraphQL
/// schema. Returns one JSON error object per problem; an empty list
/// means "all good".
///
/// Two layers run in one pass:
///
///   1. Top-level required arguments — driven by every schema arg
///      whose type is `NON_NULL` and has no server `defaultValue`.
///      Emits `missingRequiredArguments` /
///      `argumentLiteralsIncompatible` / `INVALID_VARIABLE` for
///      missing top-level args.
///
///   2. Variable-bound input object coercion — for every arg whose
///      AST value is a `VariableValue`, walk the resolved variable
///      value against the schema arg type and aggregate every
///      "missing required field" / "literal null on NON_NULL" into
///      a single `INVALID_VARIABLE` error per offending variable
///      (matching real Shopify's variable coercion envelope, which
///      groups problems by variable rather than per-field).
///
/// Literal input-object internals are deliberately *not* validated
/// here. Real Shopify is lenient at literal coercion time — missing
/// NON_NULL fields on inline input objects come back as
/// per-mutation `userErrors`, not GraphQL coercion errors — so
/// strict introspection-driven validation produces false positives
/// (`priceListCreate.parent`, `delegateAccessTokenCreate` legacy
/// `accessScopes` alias, etc.). The handler is still authoritative
/// for shape validation of literal input objects.
///
/// When the mutation is not in the captured schema, returns the
/// empty list and lets the per-handler logic run unchanged. (Newer
/// API versions or unsupported mutations will fall back to the
/// existing per-handler validation.)
pub fn validate_mutation_field_against_schema(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: String,
  operation_path: String,
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  case mutation_schema_lookup.get_mutation(schema, operation_name) {
    None -> []
    Some(mutation) -> {
      let arguments = case field {
        Field(arguments: args, ..) -> args
        _ -> []
      }
      let field_loc = field_location(field)
      let var_defs = extract_variable_definitions(source_body)
      let top_level_errors =
        validate_top_level_args(
          mutation,
          arguments,
          variables,
          var_defs,
          operation_name,
          operation_path,
          field_loc,
          source_body,
        )
      let variable_errors =
        validate_variable_bound_args(
          mutation,
          arguments,
          variables,
          var_defs,
          source_body,
          schema,
        )
      let literal_input_errors =
        validate_literal_input_object_fields(
          mutation,
          arguments,
          operation_name,
          operation_path,
          source_body,
          schema,
        )
      let literal_errors =
        validate_literal_bound_args(
          mutation,
          arguments,
          operation_path,
          operation_name,
          source_body,
          schema,
        )
      top_level_errors
      |> list.append(variable_errors)
      |> list.append(literal_errors)
      |> list.append(literal_input_errors)
    }
  }
}

fn validate_literal_input_object_fields(
  mutation: SchemaMutation,
  arguments: List(Argument),
  operation_name: String,
  operation_path: String,
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  case operation_name {
    "backupRegionUpdate" ->
      validate_backup_region_update_literal_region(
        arguments,
        operation_path,
        source_body,
      )
    // Most Shopify mutations are resolver-lenient for missing fields
    // inside top-level inline inputs. Live locationAdd is stricter for
    // LocationAddInput.name/address, so mirror that targeted parser
    // behavior without broadening this validator across all inputs.
    "locationAdd" ->
      validate_direct_literal_input_fields(
        mutation,
        arguments,
        "input",
        "LocationAddInput",
        operation_name,
        operation_path,
        source_body,
        schema,
      )
    "carrierServiceCreate" ->
      validate_direct_literal_input_fields(
        mutation,
        arguments,
        "input",
        "DeliveryCarrierServiceCreateInput",
        operation_name,
        operation_path,
        source_body,
        schema,
      )
    "orderEditAddCustomItem" ->
      validate_direct_literal_input_fields(
        mutation,
        arguments,
        "price",
        "MoneyInput",
        operation_name,
        operation_path,
        source_body,
        schema,
      )
    _ -> []
  }
}

fn validate_backup_region_update_literal_region(
  arguments: List(Argument),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  case find_argument(arguments, "region") {
    Some(Argument(value: ObjectValue(fields: fields, loc: object_loc), ..)) ->
      validate_backup_region_country_code_literal(
        fields,
        object_loc,
        operation_path,
        source_body,
      )
    _ -> []
  }
}

fn validate_backup_region_country_code_literal(
  fields: List(ast.ObjectField),
  object_loc: Option(Location),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  case find_object_field(fields, "countryCode") {
    None -> [
      build_missing_required_input_object_attribute_error_with_location(
        operation_path,
        "backupRegionUpdate",
        "region",
        "BackupRegionUpdateInput",
        "countryCode",
        "CountryCode!",
        object_loc,
        source_body,
      ),
    ]
    Some(ast.ObjectField(value: NullValue(..), ..)) -> [
      build_invalid_input_object_attribute_error_with_location(
        operation_path,
        "backupRegionUpdate",
        "region",
        "BackupRegionUpdateInput",
        "countryCode",
        "CountryCode!",
        "null",
        object_loc,
        source_body,
      ),
    ]
    Some(ast.ObjectField(value: EnumValue(..), ..)) -> []
    Some(ast.ObjectField(value: StringValue(..), ..)) -> []
    Some(ast.ObjectField(value: VariableValue(..), ..)) -> []
    Some(ast.ObjectField(value: value, ..)) -> [
      build_invalid_input_object_attribute_error_with_location(
        operation_path,
        "backupRegionUpdate",
        "region",
        "BackupRegionUpdateInput",
        "countryCode",
        "CountryCode!",
        literal_value_preview(value),
        object_loc,
        source_body,
      ),
    ]
  }
}

fn validate_direct_literal_input_fields(
  mutation: SchemaMutation,
  arguments: List(Argument),
  argument_name: String,
  input_object_name: String,
  operation_name: String,
  operation_path: String,
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  case find_argument(arguments, argument_name) {
    Some(Argument(value: ast.ObjectValue(fields: fields, loc: loc), ..)) ->
      case
        find_schema_arg(mutation.args, argument_name),
        mutation_schema_lookup.get_input_object(schema, input_object_name)
      {
        Some(_), Some(input_object) ->
          list.flat_map(input_object.input_fields, fn(input_field) {
            case
              mutation_schema.is_non_null(input_field.type_),
              input_field.default_value
            {
              True, None ->
                validate_direct_literal_input_field(
                  fields,
                  argument_name,
                  input_object_name,
                  input_field.name,
                  mutation_schema.render_signature(input_field.type_),
                  operation_name,
                  operation_path,
                  loc,
                  source_body,
                )
              _, _ -> []
            }
          })
        _, _ -> []
      }
    _ -> []
  }
}

fn validate_direct_literal_input_field(
  fields: List(ast.ObjectField),
  argument_name: String,
  input_object_name: String,
  input_field_name: String,
  input_field_type: String,
  operation_name: String,
  operation_path: String,
  loc: Option(Location),
  source_body: String,
) -> List(Json) {
  case find_object_field(fields, input_field_name) {
    None -> [
      build_missing_required_input_object_attribute_error_with_location(
        operation_path,
        operation_name,
        argument_name,
        input_object_name,
        input_field_name,
        input_field_type,
        loc,
        source_body,
      ),
    ]
    Some(ast.ObjectField(value: ast.NullValue(..), ..)) -> [
      build_null_input_object_attribute_error_with_location(
        operation_path,
        operation_name,
        argument_name,
        input_object_name,
        input_field_name,
        input_field_type,
        loc,
        source_body,
      ),
    ]
    _ -> []
  }
}

fn find_object_field(
  fields: List(ast.ObjectField),
  name: String,
) -> Option(ast.ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ast.ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

fn build_missing_required_input_object_attribute_error_with_location(
  operation_path: String,
  operation_name: String,
  argument_name: String,
  input_object_name: String,
  input_field_name: String,
  input_field_type: String,
  loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> input_field_name
        <> "' on InputObject '"
        <> input_object_name
        <> "' is required. Expected type "
        <> input_field_type,
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, operation_name, argument_name, input_field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("missingRequiredInputObjectAttribute")),
          #("argumentName", json.string(input_field_name)),
          #("argumentType", json.string(input_field_type)),
          #("inputObjectType", json.string(input_object_name)),
        ]),
      ),
    ]),
  )
}

fn build_null_input_object_attribute_error_with_location(
  operation_path: String,
  operation_name: String,
  argument_name: String,
  input_object_name: String,
  input_field_name: String,
  input_field_type: String,
  loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> input_field_name
        <> "' on InputObject '"
        <> input_object_name
        <> "' has an invalid value (null). Expected type '"
        <> input_field_type
        <> "'.",
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, operation_name, argument_name, input_field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("InputObject")),
          #("argumentName", json.string(input_field_name)),
        ]),
      ),
    ]),
  )
}

fn build_invalid_input_object_attribute_error_with_location(
  operation_path: String,
  operation_name: String,
  argument_name: String,
  input_object_name: String,
  input_field_name: String,
  input_field_type: String,
  value: String,
  loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> input_field_name
        <> "' on InputObject '"
        <> input_object_name
        <> "' has an invalid value ("
        <> value
        <> "). Expected type '"
        <> input_field_type
        <> "'.",
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, operation_name, argument_name, input_field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("InputObject")),
          #("argumentName", json.string(input_field_name)),
        ]),
      ),
    ]),
  )
}

fn literal_value_preview(value: Value) -> String {
  case value {
    IntValue(value: value, ..) -> value
    FloatValue(value: value, ..) -> value
    StringValue(value: value, ..) -> "\"" <> value <> "\""
    BooleanValue(value: True, ..) -> "true"
    BooleanValue(value: False, ..) -> "false"
    NullValue(..) -> "null"
    EnumValue(value: value, ..) -> value
    ListValue(..) -> "[]"
    ObjectValue(..) -> "{}"
    VariableValue(variable: variable) -> "$" <> variable.name.value
  }
}

fn validate_top_level_args(
  mutation: SchemaMutation,
  arguments: List(Argument),
  variables: Dict(String, root_field.ResolvedValue),
  var_defs: Dict(String, VariableDef),
  operation_name: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> List(Json) {
  let #(missing_names, errors) =
    list.fold(mutation.args, #([], []), fn(acc, schema_arg) {
      let #(missing, errs) = acc
      case
        mutation_schema.is_non_null(schema_arg.type_),
        schema_arg.default_value
      {
        False, _ -> acc
        True, Some(_) -> acc
        True, None -> {
          let expected = mutation_schema.render_signature(schema_arg.type_)
          case find_argument(arguments, schema_arg.name) {
            None -> #(list.append(missing, [schema_arg.name]), errs)
            Some(argument) ->
              case argument.value {
                NullValue(..) -> #(
                  missing,
                  list.append(errs, [
                    build_null_argument_error(
                      operation_name,
                      schema_arg.name,
                      expected,
                      operation_path,
                      field_loc,
                      source_body,
                    ),
                  ]),
                )
                VariableValue(variable: var) -> {
                  // Real Shopify only rejects a missing/null variable here
                  // when the *variable's declared type* is NON_NULL — a
                  // nullable-declared variable bound to a NON_NULL arg
                  // passes through to the resolver, which surfaces the
                  // problem as a `userErrors` entry. Mirror that.
                  let declared_non_null = case
                    dict.get(var_defs, var.name.value)
                  {
                    Ok(def) -> def.declared_non_null
                    Error(_) -> True
                  }
                  case declared_non_null, dict.get(variables, var.name.value) {
                    True, Ok(root_field.NullVal) | True, Error(_) -> #(
                      missing,
                      list.append(errs, [
                        build_missing_variable_error(var.name.value, expected),
                      ]),
                    )
                    _, _ -> acc
                  }
                }
                StringValue(value: value, ..) ->
                  case value == "" && schema_type_is_id(schema_arg.type_) {
                    True -> #(
                      missing,
                      list.append(errs, [
                        build_invalid_global_id_literal_error(
                          operation_name,
                          schema_arg.name,
                          operation_path,
                          field_loc,
                          source_body,
                        ),
                      ]),
                    )
                    False -> acc
                  }
                _ -> acc
              }
          }
        }
      }
    })
  case missing_names {
    [] -> errors
    _ -> [
      build_missing_required_argument_error(
        operation_name,
        string.join(missing_names, ", "),
        operation_path,
        field_loc,
        source_body,
      ),
      ..errors
    ]
  }
}

/// One segment of a problem path. List indices appear as JSON
/// integers in Shopify's `extensions.problems[].path` (e.g.
/// `[0, "key"]`), so we keep ints distinct from string field names.
type PathSegment {
  StringSegment(String)
  IntSegment(Int)
}

/// One missing-or-null leaf inside a variable's resolved value.
/// `path` is rooted at the variable (NOT including the variable
/// name itself) and matches the shape Shopify emits in
/// `extensions.problems[]`.
type ValueProblem {
  ValueProblem(
    path: List(PathSegment),
    explanation: String,
    message: Option(String),
  )
}

fn validate_literal_bound_args(
  mutation: SchemaMutation,
  arguments: List(Argument),
  operation_path: String,
  operation_name: String,
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  list.flat_map(arguments, fn(arg) {
    case arg {
      Argument(name: arg_name, value: value, ..) ->
        case find_schema_arg(mutation.args, arg_name.value) {
          None -> []
          Some(schema_arg) -> {
            let path = [
              StringSegment(operation_path),
              StringSegment(operation_name),
              StringSegment(arg_name.value),
            ]
            collect_literal_unknown_field_errors(
              value,
              schema_arg.type_,
              schema,
              path,
              source_body,
            )
            |> list.append(collect_literal_enum_value_errors(
              value,
              schema_arg.type_,
              schema,
              path,
              source_body,
            ))
          }
        }
    }
  })
}

fn collect_literal_unknown_field_errors(
  value: ast.Value,
  schema_type: mutation_schema.SchemaType,
  schema: MutationSchema,
  path: List(PathSegment),
  source_body: String,
) -> List(Json) {
  case schema_type {
    mutation_schema.NonNullType(of: inner) ->
      collect_literal_unknown_field_errors(
        value,
        inner,
        schema,
        path,
        source_body,
      )
    mutation_schema.ListType(of: inner) ->
      case value {
        ListValue(values: values, ..) ->
          list.index_map(values, fn(item, index) {
            collect_literal_unknown_field_errors(
              item,
              inner,
              schema,
              list.append(path, [IntSegment(index)]),
              source_body,
            )
          })
          |> list.flatten
        _ -> []
      }
    mutation_schema.NamedType(name: io_name) ->
      case mutation_schema_lookup.get_input_object(schema, io_name) {
        None -> []
        Some(io) ->
          case value {
            ObjectValue(fields: fields, ..) ->
              list.flat_map(fields, fn(object_field) {
                let ast.ObjectField(name: field_name, value: child, loc: loc) =
                  object_field
                let field_path =
                  list.append(path, [StringSegment(field_name.value)])
                case
                  find_schema_input_field(io.input_fields, field_name.value)
                {
                  None ->
                    case io.name {
                      "ValidationUpdateInput" -> [
                        build_unknown_input_object_field_error(
                          field_name.value,
                          io.name,
                          field_path,
                          loc,
                          source_body,
                        ),
                      ]
                      "OnlineStoreThemeInput" -> [
                        build_input_object_argument_not_accepted_error(
                          field_name.value,
                          io.name,
                          field_path,
                          loc,
                          source_body,
                        ),
                      ]
                      _ -> []
                    }
                  Some(schema_field) ->
                    collect_literal_unknown_field_errors(
                      child,
                      schema_field.type_,
                      schema,
                      field_path,
                      source_body,
                    )
                }
              })
            _ -> []
          }
      }
  }
}

fn collect_literal_enum_value_errors(
  value: ast.Value,
  schema_type: mutation_schema.SchemaType,
  schema: MutationSchema,
  path: List(PathSegment),
  source_body: String,
) -> List(Json) {
  case schema_type {
    mutation_schema.NonNullType(of: inner) ->
      collect_literal_enum_value_errors(value, inner, schema, path, source_body)
    mutation_schema.ListType(of: inner) ->
      case value {
        ListValue(values: values, ..) ->
          list.index_map(values, fn(item, index) {
            collect_literal_enum_value_errors(
              item,
              inner,
              schema,
              list.append(path, [IntSegment(index)]),
              source_body,
            )
          })
          |> list.flatten
        _ -> []
      }
    mutation_schema.NamedType(name: type_name) ->
      case dict.get(enum_value_sets(), type_name) {
        Ok(allowed) ->
          validate_literal_enum_value(value, allowed, path, source_body)
        Error(_) ->
          case
            mutation_schema_lookup.get_input_object(schema, type_name),
            value
          {
            Some(io), ObjectValue(fields: fields, ..) ->
              list.flat_map(fields, fn(object_field) {
                let ast.ObjectField(name: field_name, value: child, ..) =
                  object_field
                case
                  find_schema_input_field(io.input_fields, field_name.value)
                {
                  Some(schema_field) ->
                    collect_literal_enum_value_errors(
                      child,
                      schema_field.type_,
                      schema,
                      list.append(path, [StringSegment(field_name.value)]),
                      source_body,
                    )
                  None -> []
                }
              })
            _, _ -> []
          }
      }
  }
}

fn validate_literal_enum_value(
  value: ast.Value,
  allowed: List(String),
  path: List(PathSegment),
  source_body: String,
) -> List(Json) {
  case value {
    StringValue(value: raw, loc: loc, ..) | EnumValue(value: raw, loc: loc) ->
      case list.contains(allowed, raw) {
        True -> []
        False -> [
          build_literal_coercion_error(
            "Expected \""
              <> raw
              <> "\" to be one of: "
              <> string.join(allowed, ", "),
            path,
            loc,
            source_body,
          ),
        ]
      }
    _ -> []
  }
}

fn build_literal_coercion_error(
  message: String,
  path: List(PathSegment),
  loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [#("message", json.string(message))]
  let with_locations = case locations_payload(loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", path_segments_to_json(path)),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("CoercionError")),
        ]),
      ),
    ]),
  )
}

/// For each top-level arg whose AST value is a `VariableValue`,
/// validate the resolved variable value against the schema arg type
/// and emit one INVALID_VARIABLE error per problematic variable.
fn validate_variable_bound_args(
  mutation: SchemaMutation,
  arguments: List(Argument),
  variables: Dict(String, root_field.ResolvedValue),
  var_defs: Dict(String, VariableDef),
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  list.flat_map(arguments, fn(arg) {
    case arg {
      Argument(name: arg_name, value: VariableValue(variable: var), ..) ->
        case find_schema_arg(mutation.args, arg_name.value) {
          None -> []
          Some(schema_arg) ->
            invalid_variable_errors_for(
              var.name.value,
              schema_arg.type_,
              variables,
              var_defs,
              source_body,
              schema,
            )
        }
      _ -> []
    }
  })
}

fn invalid_variable_errors_for(
  variable_name: String,
  declared_type: mutation_schema.SchemaType,
  variables: Dict(String, root_field.ResolvedValue),
  var_defs: Dict(String, VariableDef),
  source_body: String,
  schema: MutationSchema,
) -> List(Json) {
  case dict.get(variables, variable_name) {
    // Unbound variable: validate_top_level_args already emits the
    // INVALID_VARIABLE-for-null envelope. Don't double-count.
    Error(_) -> []
    Ok(root_field.NullVal) -> []
    Ok(resolved) -> {
      let problems =
        collect_value_problems(resolved, declared_type, schema, [])
        |> list.append(top_level_required_input_field_problems(
          resolved,
          declared_type,
          schema,
        ))
      case problems {
        [] -> []
        _ -> {
          let loc = case dict.get(var_defs, variable_name) {
            Ok(def) -> def.loc
            Error(_) -> None
          }
          [
            build_invalid_variable_problems_error(
              variable_name,
              mutation_schema.render_signature(declared_type),
              resolved,
              problems,
              loc,
              source_body,
            ),
          ]
        }
      }
    }
  }
}

fn top_level_required_input_field_problems(
  resolved: root_field.ResolvedValue,
  declared_type: mutation_schema.SchemaType,
  schema: MutationSchema,
) -> List(ValueProblem) {
  case mutation_schema.named_leaf(declared_type), resolved {
    Some(input_type_name), root_field.ObjectVal(fields) -> {
      case
        list.contains(
          top_level_required_input_field_strict_types(),
          input_type_name,
        )
      {
        True ->
          required_input_object_field_problems(input_type_name, fields, schema)
        False -> []
      }
    }
    _, _ -> []
  }
}

fn top_level_required_input_field_strict_types() -> List(String) {
  [
    "DeliveryCarrierServiceCreateInput",
    "CatalogCreateInput",
    "PriceListCreateInput",
  ]
}

fn required_input_object_field_problems(
  input_type_name: String,
  fields: Dict(String, root_field.ResolvedValue),
  schema: MutationSchema,
) -> List(ValueProblem) {
  case mutation_schema_lookup.get_input_object(schema, input_type_name) {
    Some(input_object) ->
      list.filter_map(input_object.input_fields, fn(field) {
        let required =
          mutation_schema.is_non_null(field.type_)
          && option.is_none(field.default_value)
        case required, dict.has_key(fields, field.name) {
          True, False ->
            Ok(ValueProblem(
              path: [StringSegment(field.name)],
              explanation: "Expected value to not be null",
              message: None,
            ))
          _, _ -> Error(Nil)
        }
      })
    None -> []
  }
}

/// Walk a resolved variable value against its declared schema type,
/// collecting "missing required field" problems Shopify would emit
/// in the INVALID_VARIABLE envelope.
///
/// `inside_list` flips to True the first time we descend into a
/// `ListType`. Real Shopify is strict about NON_NULL fields on input
/// objects that appear *as list elements* (e.g.
/// `[MetafieldsSetInput!]!.key` missing → INVALID_VARIABLE), but
/// lenient about most NON_NULL fields on a top-level single-object
/// variable. Mirror that asymmetry — flagging all top-level objects
/// produces false positives against the live runtime that is more
/// permissive than the introspection schema advertises. Narrow
/// top-level strict exceptions are handled before this traversal.
fn collect_value_problems(
  resolved: root_field.ResolvedValue,
  schema_type: mutation_schema.SchemaType,
  schema: MutationSchema,
  path: List(PathSegment),
) -> List(ValueProblem) {
  collect_value_problems_inner(
    resolved,
    schema_type,
    schema,
    path,
    inside_list: False,
  )
}

fn collect_value_problems_inner(
  resolved: root_field.ResolvedValue,
  schema_type: mutation_schema.SchemaType,
  schema: MutationSchema,
  path: List(PathSegment),
  inside_list inside_list: Bool,
) -> List(ValueProblem) {
  case schema_type {
    mutation_schema.NonNullType(of: inner) ->
      case resolved {
        root_field.NullVal -> [
          ValueProblem(
            path: path,
            explanation: "Expected value to not be null",
            message: None,
          ),
        ]
        _ ->
          collect_value_problems_inner(
            resolved,
            inner,
            schema,
            path,
            inside_list:,
          )
      }
    mutation_schema.ListType(of: inner) ->
      case resolved {
        root_field.ListVal(items) ->
          list.index_map(items, fn(item, idx) {
            collect_value_problems_inner(
              item,
              inner,
              schema,
              list.append(path, [IntSegment(idx)]),
              inside_list: True,
            )
          })
          |> list.flatten
        _ -> []
      }
    mutation_schema.NamedType(name: type_name) ->
      case scalar_value_problems(type_name, resolved, path) {
        [_, ..] as problems -> problems
        [] ->
          case enum_value_problems(type_name, resolved, path) {
            [_, ..] as problems -> problems
            [] ->
              case mutation_schema_lookup.get_input_object(schema, type_name) {
                None -> []
                Some(io) ->
                  case resolved {
                    root_field.ObjectVal(fields) ->
                      list.append(
                        list.flat_map(io.input_fields, fn(field) {
                          let field_path =
                            list.append(path, [StringSegment(field.name)])
                          let required =
                            mutation_schema.is_non_null(field.type_)
                            && option.is_none(field.default_value)
                            && inside_list
                          case dict.get(fields, field.name), required {
                            Error(_), True -> [
                              ValueProblem(
                                path: field_path,
                                explanation: "Expected value to not be null",
                                message: None,
                              ),
                            ]
                            Error(_), False -> []
                            Ok(child), _ ->
                              collect_value_problems_inner(
                                child,
                                field.type_,
                                schema,
                                field_path,
                                inside_list:,
                              )
                          }
                        }),
                        collect_unknown_variable_fields(fields, io, path),
                      )
                    _ -> []
                  }
              }
          }
      }
  }
}

fn scalar_value_problems(
  type_name: String,
  resolved: root_field.ResolvedValue,
  path: List(PathSegment),
) -> List(ValueProblem) {
  case type_name {
    "Decimal" -> decimal_value_problems(resolved, path)
    "URL" -> url_value_problems(resolved, path)
    _ -> []
  }
}

fn url_value_problems(
  resolved: root_field.ResolvedValue,
  path: List(PathSegment),
) -> List(ValueProblem) {
  case resolved {
    root_field.StringVal(value) ->
      case string.split_once(value, "://") {
        Ok(#("", _)) -> [invalid_url_problem(value, "empty scheme", path)]
        Ok(#(_, "")) -> [invalid_url_problem(value, "missing host", path)]
        Ok(#(_, after_scheme)) ->
          case url_authority(after_scheme) {
            "" -> [invalid_url_problem(value, "missing host", path)]
            _ -> []
          }
        Error(_) -> [invalid_url_problem(value, "missing scheme", path)]
      }
    _ -> []
  }
}

fn invalid_url_problem(
  value: String,
  reason: String,
  path: List(PathSegment),
) -> ValueProblem {
  let message = "Invalid url '" <> value <> "', " <> reason
  ValueProblem(path: path, explanation: message, message: Some(message))
}

fn url_authority(after_scheme: String) -> String {
  after_scheme
  |> string_before("/")
  |> string_before("?")
  |> string_before("#")
}

fn string_before(value: String, delimiter: String) -> String {
  case string.split(value, on: delimiter) {
    [head, ..] -> head
    [] -> value
  }
}

fn decimal_value_problems(
  resolved: root_field.ResolvedValue,
  path: List(PathSegment),
) -> List(ValueProblem) {
  case resolved {
    root_field.StringVal(value) ->
      case float.parse(value) {
        Ok(_) -> []
        Error(_) ->
          case int.parse(value) {
            Ok(_) -> []
            Error(_) -> [
              ValueProblem(
                path: path,
                explanation: "invalid decimal '" <> value <> "'",
                message: Some("invalid decimal '" <> value <> "'"),
              ),
            ]
          }
      }
    root_field.IntVal(_) | root_field.FloatVal(_) -> []
    _ -> []
  }
}

fn collect_unknown_variable_fields(
  fields: Dict(String, root_field.ResolvedValue),
  io: mutation_schema.SchemaInputObject,
  path: List(PathSegment),
) -> List(ValueProblem) {
  dict.keys(fields)
  |> list.filter_map(fn(field_name) {
    case io.name, find_schema_input_field(io.input_fields, field_name) {
      _, Some(_) -> Error(Nil)
      "ValidationUpdateInput", None ->
        Ok(ValueProblem(
          path: list.append(path, [StringSegment(field_name)]),
          explanation: "Field is not defined on " <> io.name,
          message: None,
        ))
      "OnlineStoreThemeInput", None ->
        Ok(ValueProblem(
          path: list.append(path, [StringSegment(field_name)]),
          explanation: "Field is not defined on " <> io.name,
          message: None,
        ))
      "DiscountCustomerSelectionInput", None ->
        Ok(ValueProblem(
          path: list.append(path, [StringSegment(field_name)]),
          explanation: "Field is not defined on " <> io.name,
          message: None,
        ))
      _, None -> Error(Nil)
    }
  })
}

fn build_unknown_input_object_field_error(
  field_name: String,
  input_object_type: String,
  path: List(PathSegment),
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '" <> field_name <> "' is not defined on " <> input_object_type,
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", path_segments_to_json(path)),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("InputObject")),
          #("argumentName", json.string(field_name)),
        ]),
      ),
    ]),
  )
}

fn build_input_object_argument_not_accepted_error(
  field_name: String,
  input_object_type: String,
  path: List(PathSegment),
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "InputObject '"
        <> input_object_type
        <> "' doesn't accept argument '"
        <> field_name
        <> "'",
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", path_segments_to_json(path)),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentNotAccepted")),
          #("name", json.string(input_object_type)),
          #("typeName", json.string("InputObject")),
          #("argumentName", json.string(field_name)),
        ]),
      ),
    ]),
  )
}

fn enum_value_problems(
  type_name: String,
  resolved: root_field.ResolvedValue,
  path: List(PathSegment),
) -> List(ValueProblem) {
  case dict.get(enum_value_sets(), type_name), resolved {
    Ok(allowed), root_field.StringVal(value) ->
      case list.contains(allowed, value) {
        True -> []
        False -> [
          ValueProblem(
            path: path,
            explanation: "Expected \""
              <> value
              <> "\" to be one of: "
              <> string.join(allowed, ", "),
            message: None,
          ),
        ]
      }
    _, _ -> []
  }
}

fn enum_value_sets() -> Dict(String, List(String)) {
  dict.from_list([
    #("CollectionSortOrder", [
      "ALPHA_ASC",
      "ALPHA_DESC",
      "BEST_SELLING",
      "CREATED",
      "CREATED_DESC",
      "MANUAL",
      "PRICE_ASC",
      "PRICE_DESC",
    ]),
    #("CountryCode", country_code_values()),
    #("CurrencyCode", currency_code_values()),
    #("WeightUnit", ["KILOGRAMS", "GRAMS", "POUNDS", "OUNCES"]),
    #("FulfillmentEventStatus", [
      "LABEL_PURCHASED",
      "LABEL_PRINTED",
      "READY_FOR_PICKUP",
      "CONFIRMED",
      "IN_TRANSIT",
      "OUT_FOR_DELIVERY",
      "ATTEMPTED_DELIVERY",
      "DELAYED",
      "DELIVERED",
      "FAILURE",
      "CARRIER_PICKED_UP",
    ]),
    #("ResourceFeedbackState", ["ACCEPTED", "REQUIRES_ACTION"]),
    #("PublicationCreateInputPublicationDefaultState", [
      "EMPTY",
      "ALL_PRODUCTS",
    ]),
    #("TaxExemption", tax_exemption_values()),
  ])
}

fn country_code_values() -> List(String) {
  string.split(country_code_values_message(), ", ")
}

fn country_code_values_message() -> String {
  "AF, AX, AL, DZ, AD, AO, AI, AG, AR, AM, AW, AC, AU, AT, AZ, BS, BH, BD, BB, BY, BE, BZ, BJ, BM, BT, BO, BA, BW, BV, BR, IO, BN, BG, BF, BI, KH, CA, CV, BQ, KY, CF, TD, CL, CN, CX, CC, CO, KM, CG, CD, CK, CR, HR, CU, CW, CY, CZ, CI, DK, DJ, DM, DO, EC, EG, SV, GQ, ER, EE, SZ, ET, FK, FO, FJ, FI, FR, GF, PF, TF, GA, GM, GE, DE, GH, GI, GR, GL, GD, GP, GT, GG, GN, GW, GY, HT, HM, VA, HN, HK, HU, IS, IN, ID, IR, IQ, IE, IM, IL, IT, JM, JP, JE, JO, KZ, KE, KI, KP, XK, KW, KG, LA, LV, LB, LS, LR, LY, LI, LT, LU, MO, MG, MW, MY, MV, ML, MT, MQ, MR, MU, YT, MX, MD, MC, MN, ME, MS, MA, MZ, MM, NA, NR, NP, NL, AN, NC, NZ, NI, NE, NG, NU, NF, MK, NO, OM, PK, PS, PA, PG, PY, PE, PH, PN, PL, PT, QA, CM, RE, RO, RU, RW, BL, SH, KN, LC, MF, PM, WS, SM, ST, SA, SN, RS, SC, SL, SG, SX, SK, SI, SB, SO, ZA, GS, KR, SS, ES, LK, VC, SD, SR, SJ, SE, CH, SY, TW, TJ, TZ, TH, TL, TG, TK, TO, TT, TA, TN, TR, TM, TC, TV, UG, UA, AE, GB, US, UM, UY, UZ, VU, VE, VN, VG, WF, EH, YE, ZM, ZW, ZZ"
}

fn currency_code_values() -> List(String) {
  string.split(currency_code_values_message(), ", ")
}

fn currency_code_values_message() -> String {
  "USD, EUR, GBP, CAD, AFN, ALL, DZD, AOA, ARS, AMD, AWG, AUD, BBD, AZN, BDT, BSD, BHD, BIF, BYN, BZD, BMD, BTN, BAM, BRL, BOB, BWP, BND, BGN, MMK, KHR, CVE, KYD, XAF, CLP, CNY, COP, KMF, CDF, CRC, HRK, CZK, DKK, DJF, DOP, XCD, EGP, ERN, ETB, FKP, XPF, FJD, GIP, GMD, GHS, GTQ, GYD, GEL, GNF, HTG, HNL, HKD, HUF, ISK, INR, IDR, ILS, IRR, IQD, JMD, JPY, JEP, JOD, KZT, KES, KID, KWD, KGS, LAK, LVL, LBP, LSL, LRD, LYD, LTL, MGA, MKD, MOP, MWK, MVR, MRU, MXN, MYR, MUR, MDL, MAD, MNT, MZN, NAD, NPR, ANG, NZD, NIO, NGN, NOK, OMR, PAB, PKR, PGK, PYG, PEN, PHP, PLN, QAR, RON, RUB, RWF, WST, SHP, SAR, RSD, SCR, SLL, SGD, SDG, SOS, SYP, ZAR, KRW, SSP, SBD, LKR, SRD, SZL, SEK, CHF, TWD, THB, TJS, TZS, TOP, TTD, TND, TRY, TMT, UGX, UAH, AED, UYU, UZS, VUV, VES, VND, XOF, YER, ZMW, USDC, BYR, STD, STN, VED, VEF, XXX"
}

pub fn tax_exemption_values() -> List(String) {
  [
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
  ]
}

fn build_invalid_variable_problems_error(
  variable_name: String,
  variable_type: String,
  value: root_field.ResolvedValue,
  problems: List(ValueProblem),
  var_def_loc: Option(Location),
  source_body: String,
) -> Json {
  let message_suffix = case problems {
    [] -> ""
    [_, ..] -> {
      let rendered_problems =
        problems
        |> list.map(fn(problem) {
          let path_str =
            list.map(problem.path, path_segment_to_string) |> string.join(".")
          path_str <> " (" <> problem.explanation <> ")"
        })
      " for " <> string.join(rendered_problems, ", ")
    }
  }
  let base = [
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type "
        <> variable_type
        <> " was provided invalid value"
        <> message_suffix,
      ),
    ),
  ]
  let with_locations = case locations_payload(var_def_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", root_field.resolved_value_to_json(value)),
          #(
            "problems",
            json.preprocessed_array(
              list.map(problems, fn(p) {
                let fields = [
                  #("path", path_segments_to_json(p.path)),
                  #("explanation", json.string(p.explanation)),
                ]
                let fields = case p.message {
                  Some(message) ->
                    list.append(fields, [#("message", json.string(message))])
                  None -> fields
                }
                json.object(fields)
              }),
            ),
          ),
        ]),
      ),
    ]),
  )
}

fn path_segment_to_string(segment: PathSegment) -> String {
  case segment {
    StringSegment(s) -> s
    IntSegment(i) -> int.to_string(i)
  }
}

fn path_segments_to_json(path: List(PathSegment)) -> Json {
  json.preprocessed_array(
    list.map(path, fn(segment) {
      case segment {
        StringSegment(s) -> json.string(s)
        IntSegment(i) -> json.int(i)
      }
    }),
  )
}

/// Carries the bits of a parsed `$var: Type` declaration the
/// validator cares about: where it sits in the source (for
/// `locations` in the error envelope) and whether the declared type
/// is `NON_NULL`. The latter drives whether a null/missing variable
/// causes a top-level INVALID_VARIABLE error or is allowed through
/// to the resolver — Shopify only rejects when the *declared* type
/// is NON_NULL, regardless of the bound argument's nullability.
type VariableDef {
  VariableDef(loc: Option(Location), declared_non_null: Bool)
}

fn extract_variable_definitions(
  source_body: String,
) -> Dict(String, VariableDef) {
  case graphql_parser.parse(graphql_source.new(source_body)) {
    Error(_) -> dict.new()
    Ok(doc) -> {
      let var_defs = case find_first_operation(doc.definitions) {
        Some(ast.OperationDefinition(variable_definitions: vds, ..)) -> vds
        _ -> []
      }
      list.fold(var_defs, dict.new(), fn(acc, vd) {
        case vd {
          ast.VariableDefinition(
            variable: var,
            type_ref: type_ref,
            loc: loc,
            ..,
          ) ->
            dict.insert(
              acc,
              var.name.value,
              VariableDef(
                loc: loc,
                declared_non_null: type_ref_is_non_null(type_ref),
              ),
            )
        }
      })
    }
  }
}

fn type_ref_is_non_null(type_ref: ast.TypeRef) -> Bool {
  case type_ref {
    ast.NonNullType(..) -> True
    _ -> False
  }
}

fn find_first_operation(
  definitions: List(ast.Definition),
) -> Option(ast.Definition) {
  case definitions {
    [] -> None
    [d, ..rest] ->
      case d {
        ast.OperationDefinition(..) -> Some(d)
        _ -> find_first_operation(rest)
      }
  }
}

fn find_schema_arg(
  schema_args: List(mutation_schema.SchemaArg),
  name: String,
) -> Option(mutation_schema.SchemaArg) {
  case list.find(schema_args, fn(a) { a.name == name }) {
    Ok(v) -> Some(v)
    Error(_) -> None
  }
}

fn find_schema_input_field(
  schema_fields: List(mutation_schema.SchemaInputField),
  name: String,
) -> Option(mutation_schema.SchemaInputField) {
  case list.find(schema_fields, fn(field) { field.name == name }) {
    Ok(field) -> Some(field)
    Error(_) -> None
  }
}

// ---------------------------------------------------------------------------
// Mutation log drafts
// ---------------------------------------------------------------------------

/// Per-domain description of a mutation log entry that needs to be
/// recorded. The dispatcher (`draft_proxy.route_mutation`) is the only
/// site that actually mints the synthetic id/timestamp and calls
/// `store.record_mutation_log_entry`. Domain `process_mutation`
/// implementations return one `LogDraft` per logical entry they want
/// in the buffer (typically one per root mutation field).
///
/// Centralising the record call here means a domain that forgets to
/// build a draft has its mutation invisible — but the four-domain
/// regression that prompted this design (gift_cards / localization /
/// metafield_definitions / segments shipping no log entries) cannot
/// recur without a structurally-empty `MutationOutcome.log_drafts`,
/// which is much easier to spot in code review and in domain unit
/// tests than a missing per-handler `record_mutation_log_entry` call.
pub type LogDraft {
  LogDraft(
    operation_name: Option(String),
    root_fields: List(String),
    primary_root_field: Option(String),
    domain: String,
    execution: String,
    query: Option(String),
    variables: Option(Dict(String, root_field.ResolvedValue)),
    staged_resource_ids: List(String),
    status: store.EntryStatus,
    notes: Option(String),
  )
}

/// Standard shape returned by every domain `process_mutation`
/// implementation. Bundles the JSON response body, the next `Store`,
/// the next `SyntheticIdentityRegistry`, the resource ids that were
/// staged this turn (used by the dispatcher to thread through the log
/// entry), and the per-mutation `LogDraft` list that the dispatcher
/// records via `record_log_drafts`.
///
/// Defined once here so all domain modules share one constructor; the
/// dispatcher's `finalize_mutation_outcome` only needs to know about
/// this single type.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// Build a `MutationOutcome` carrying a top-level `{"errors":[...]}`
/// envelope for a `root_field.RootFieldError`. Domain
/// `process_mutation` implementations call this when the document
/// fails to re-parse — in practice unreachable, since the dispatcher
/// already parsed the document via `parse_operation.parse_operation`
/// before routing — so this exists to keep the return type a plain
/// `MutationOutcome` instead of forcing every caller through a
/// phantom `Result` wrapper.
pub fn parse_failed_outcome(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _err: root_field.RootFieldError,
) -> MutationOutcome {
  MutationOutcome(
    data: json.object([
      #(
        "errors",
        json.preprocessed_array([
          json.object([
            #("message", json.string("Could not parse GraphQL operation")),
          ]),
        ]),
      ),
    ]),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    log_drafts: [],
  )
}

/// Per-field result returned by simple mutation handlers. Shared by
/// domains whose mutation roots fit a uniform shape: a JSON-serialised
/// payload keyed under the response alias, plus the resource ids that
/// were staged this turn. Domains with extra cross-cutting concerns
/// (top-level errors, log-draft toggles, etc.) keep their own local
/// type.
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

/// Build a `LogDraft` for a single-root-field mutation. Mirrors the
/// shape that webhooks/apps/saved_searches/functions all
/// historically produced inline. `domain` and `execution` are the
/// `Capability` fields the entry should record; they're domain
/// constants like `"webhooks"` / `"stage-locally"`.
pub fn single_root_log_draft(
  root_field: String,
  staged_resource_ids: List(String),
  status: store.EntryStatus,
  domain: String,
  execution: String,
  notes: Option(String),
) -> LogDraft {
  LogDraft(
    operation_name: Some(root_field),
    root_fields: [root_field],
    primary_root_field: Some(root_field),
    domain: domain,
    execution: execution,
    query: None,
    variables: None,
    staged_resource_ids: staged_resource_ids,
    status: status,
    notes: notes,
  )
}

/// Record each draft into the store, threading the synthetic-identity
/// registry through `make_synthetic_gid` + `make_synthetic_timestamp`
/// for the entry id and `received_at`. Returns the updated store and
/// identity registry.
pub fn record_log_drafts(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  drafts: List(LogDraft),
) -> #(Store, SyntheticIdentityRegistry) {
  list.fold(drafts, #(store, identity), fn(acc, draft) {
    let #(current_store, current_identity) = acc
    let #(log_id, identity_after_id) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "MutationLogEntry",
      )
    let #(received_at, identity_after_ts) =
      synthetic_identity.make_synthetic_timestamp(identity_after_id)
    let entry =
      store_types.MutationLogEntry(
        id: log_id,
        received_at: received_at,
        operation_name: draft.operation_name,
        path: request_path,
        query: option.unwrap(draft.query, document),
        variables: option.unwrap(draft.variables, variables),
        staged_resource_ids: draft.staged_resource_ids,
        status: draft.status,
        interpreted: store_types.InterpretedMetadata(
          operation_type: store_types.Mutation,
          operation_name: draft.operation_name,
          root_fields: draft.root_fields,
          primary_root_field: draft.primary_root_field,
          capability: store_types.Capability(
            operation_name: draft.operation_name,
            domain: draft.domain,
            execution: draft.execution,
          ),
        ),
        notes: draft.notes,
      )
    #(store.record_mutation_log_entry(current_store, entry), identity_after_ts)
  })
}

/// Combine independent validation outputs into a single error list.
/// Use at sites that previously chained `|> list.append(other_errors)`
/// repeatedly — the flat shape reads as a checklist of independent
/// validators and removes the syntactic distinction between the seed
/// list and the appended ones.
///
/// Eager: every validator must already have run, so all errors surface
/// in one response (matching Shopify's userErrors semantics).
pub fn combine_error_lists(error_lists: List(List(error))) -> List(error) {
  list.flatten(error_lists)
}

/// Wrap a domain `process` result into the dispatcher's
/// `#(Response, DraftProxy)` shape. `Ok(envelope)` becomes a 200 with
/// the JSON body; `Error(_)` becomes a 400 carrying `error_message` in
/// the standard `{ errors: [...] }` envelope. Centralised so each
/// simple-domain `handle_query_request` is a one-liner.
pub fn respond_to_query(
  proxy: DraftProxy,
  result: Result(Json, a),
  error_message: String,
) -> #(Response, DraftProxy) {
  case result {
    Ok(envelope) -> #(Response(status: 200, body: envelope, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([
          #(
            "errors",
            json.preprocessed_array([
              json.object([#("message", json.string(error_message))]),
            ]),
          ),
        ]),
        headers: [],
      ),
      proxy,
    )
  }
}

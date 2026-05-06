//// Products-domain submodule: shared.
//// Combines layered files: shared_l00, shared_l01.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Location, type ObjectField, type Selection, type VariableDefinition,
  Argument, Directive, Field, ObjectField, StringValue, VariableDefinition,
  VariableValue,
}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, BoolVal, FloatVal, IntVal, ListVal, NullVal, ObjectVal,
  StringVal, get_field_arguments,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/mutation_helpers.{find_argument}

import shopify_draft_proxy/proxy/products/product_types.{
  type MutationFieldResult, type NullableFieldUserError, type NumericRead,
  type ProductUserError, MutationFieldResult, NullableFieldUserError,
  NumericMissing, NumericNotANumber, NumericNull, NumericValue, ProductUserError,
  max_product_variants,
}

import shopify_draft_proxy/search_query_parser

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
}

// ===== from shared_l00 =====
@internal
pub fn resource_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

@internal
pub fn bool_string(value: Bool) -> String {
  case value {
    True -> "true"
    False -> "false"
  }
}

@internal
pub fn normalize_string_catalog(values: List(String)) -> List(String) {
  values
  |> list.filter(fn(value) { string.length(string.trim(value)) > 0 })
  |> list.fold(dict.new(), fn(seen, value) { dict.insert(seen, value, True) })
  |> dict.keys()
  |> list.sort(string.compare)
}

@internal
pub fn string_cursor(value: String, _index: Int) -> String {
  value
}

@internal
pub fn serialize_exact_count(field: Selection, count: Int) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(count))
              "precision" -> #(key, json.string("EXACT"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

@internal
pub fn read_identifier_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Dict(String, ResolvedValue)) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, "identifier") {
        Ok(ObjectVal(identifier)) -> Some(identifier)
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn read_string_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(StringVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn read_bool_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(BoolVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn read_string_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(fields, name) {
    Ok(StringVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn count_source(count: Int) -> SourceValue {
  src_object([
    #("__typename", SrcString("Count")),
    #("count", SrcInt(count)),
    #("precision", SrcString("EXACT")),
  ])
}

@internal
pub fn connection_start_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case items {
    [first, ..] -> SrcString(get_cursor(first, 0))
    [] -> SrcNull
  }
}

@internal
pub fn connection_end_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case list.last(items) {
    Ok(last) -> SrcString(get_cursor(last, list.length(items) - 1))
    Error(_) -> SrcNull
  }
}

@internal
pub fn empty_connection_source() -> SourceValue {
  src_object([
    #("edges", SrcList([])),
    #("nodes", SrcList([])),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

@internal
pub fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_source(item))
        })
        |> dict.from_list,
      )
  }
}

@internal
pub fn legacy_resource_id_from_gid(id: String) -> String {
  case string.split(id, "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

@internal
pub fn parse_admin_api_version(version: String) -> Option(#(Int, Int)) {
  case string.split(version, "-") {
    [year, month] ->
      case int.parse(year), int.parse(month) {
        Ok(parsed_year), Ok(parsed_month) -> Some(#(parsed_year, parsed_month))
        _, _ -> None
      }
    _ -> None
  }
}

@internal
pub fn compare_admin_api_versions(version: #(Int, Int), minimum: #(Int, Int)) {
  let #(year, month) = version
  let #(minimum_year, minimum_month) = minimum
  year > minimum_year || { year == minimum_year && month >= minimum_month }
}

@internal
pub fn input_list_has_object_missing_field(
  input: Dict(String, ResolvedValue),
  list_field: String,
  required_field: String,
) -> Bool {
  case dict.get(input, list_field) {
    Ok(ListVal(values)) ->
      list.any(values, fn(value) {
        case value {
          ObjectVal(fields) -> !dict.has_key(fields, required_field)
          _ -> False
        }
      })
    _ -> False
  }
}

@internal
pub fn missing_idempotency_key_error(field: Selection) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The @idempotent directive is required for this mutation but was not provided.",
      ),
    ),
    #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

@internal
pub fn non_empty_string(value: String) -> Option(String) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Some(trimmed)
    False -> None
  }
}

@internal
pub fn max_input_size_error(
  length: Int,
  maximum: Int,
  path: List(String),
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(length)
        <> " is greater than the maximum allowed of "
        <> int.to_string(maximum)
        <> ".",
      ),
    ),
    #("path", json.array(path, json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

@internal
pub fn read_arg_bool_default_true(
  args: Dict(String, ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(args, name) {
    Ok(BoolVal(False)) -> False
    _ -> True
  }
}

@internal
pub fn is_decimal_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

@internal
pub fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

@internal
pub fn find_variable_definition(
  definitions: List(VariableDefinition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] -> {
      let VariableDefinition(variable: variable, loc: loc, ..) = definition
      case variable.name.value == variable_name {
        True -> loc
        False -> find_variable_definition(rest, variable_name)
      }
    }
  }
}

@internal
pub fn resolved_value_to_json(value: ResolvedValue) -> Json {
  case value {
    StringVal(value) -> json.string(value)
    IntVal(value) -> json.int(value)
    FloatVal(value) -> json.float(value)
    BoolVal(value) -> json.bool(value)
    NullVal -> json.null()
    ListVal(values) -> json.array(values, resolved_value_to_json)
    ObjectVal(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
  }
}

@internal
pub fn resolved_value_to_captured(value: ResolvedValue) -> CapturedJsonValue {
  case value {
    NullVal -> CapturedNull
    BoolVal(value) -> CapturedBool(value)
    IntVal(value) -> CapturedInt(value)
    FloatVal(value) -> CapturedFloat(value)
    StringVal(value) -> CapturedString(value)
    ListVal(values) ->
      CapturedArray(list.map(values, resolved_value_to_captured))
    ObjectVal(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, resolved_value_to_captured(value))
        }),
      )
  }
}

@internal
pub fn read_number_captured_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(CapturedJsonValue) {
  case dict.get(input, name) {
    Ok(IntVal(value)) -> Some(CapturedInt(value))
    Ok(FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
  }
}

@internal
pub fn captured_object_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        let #(key, item) = pair
        case key == name {
          True -> Ok(item)
          False -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn job_source(id: String, done: Bool) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(id)),
    #("done", SrcBool(done)),
    #("query", src_object([#("__typename", SrcString("QueryRoot"))])),
  ])
}

@internal
pub fn read_arg_object_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_arg_string_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(args, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          StringVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn trimmed_non_empty(value: String) -> Result(String, Nil) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Ok(trimmed)
    False -> Error(Nil)
  }
}

@internal
pub fn host_from_origin(origin: String) -> String {
  let without_scheme = case string.split(origin, "://") {
    [_, rest] -> rest
    [rest] -> rest
    _ -> origin
  }
  case string.split(without_scheme, "/") {
    [host, ..] -> host
    [] -> without_scheme
  }
}

@internal
pub fn segment_after_store(segments: List(String)) -> Option(String) {
  case segments {
    ["store", slug, ..] -> Some(slug)
    [_, ..rest] -> segment_after_store(rest)
    [] -> None
  }
}

@internal
pub fn dedupe_preserving_order(values: List(String)) -> List(String) {
  let #(reversed, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.has_key(seen, value) {
        True -> #(items, seen)
        False -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(reversed)
}

@internal
pub fn is_known_missing_shopify_gid(id: String) -> Bool {
  string.contains(id, "/999999999999")
}

@internal
pub fn read_object_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(Dict(String, ResolvedValue)) {
  case dict.get(fields, name) {
    Ok(ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_bool_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(input, name) {
    Ok(BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_string_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(List(String)) {
  case dict.get(input, name) {
    Ok(ListVal(values)) ->
      Some(
        list.filter_map(values, fn(value) {
          case value {
            StringVal(item) -> Ok(item)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

@internal
pub fn read_list_field_length(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(input, name) {
    Ok(ListVal(values)) -> Some(list.length(values))
    _ -> None
  }
}

@internal
pub fn read_object_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(input, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_int_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(input, name) {
    Ok(IntVal(value)) -> Some(value)
    _ -> None
  }
}

// ===== from shared_l01 =====
@internal
pub fn resource_id_matches(
  resource_id: String,
  legacy_resource_id: Option(String),
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  case normalized {
    "" -> True
    _ -> {
      resource_id == normalized
      || option.unwrap(legacy_resource_id, "") == normalized
      || resource_tail(resource_id) == normalized
      || resource_tail(normalized) == resource_tail(resource_id)
    }
  }
}

@internal
pub fn serialize_string_connection(
  values: List(String),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let sorted_values = normalize_string_catalog(values)
  let ordered_values = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted_values)
    _ -> sorted_values
  }
  let window =
    paginate_connection_items(
      ordered_values,
      field,
      variables,
      string_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: string_cursor,
      serialize_node: fn(value, _node_field, _index) { json.string(value) },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn read_include_inactive_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_bool_argument(field, variables, "includeInactive") {
    Some(True) -> True
    _ -> False
  }
}

@internal
pub fn connection_page_info_source(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  src_object([
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", connection_start_cursor(items, get_cursor)),
    #("endCursor", connection_end_cursor(items, get_cursor)),
  ])
}

@internal
pub fn admin_api_version_from_path(path: String) -> Option(#(Int, Int)) {
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] ->
      parse_admin_api_version(version)
    _ -> None
  }
}

@internal
pub fn max_input_size_exceeded_error(
  root_name: String,
  argument_name: String,
  actual_size: Int,
  field: Selection,
  document: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(actual_size)
        <> " is greater than the maximum allowed of "
        <> int.to_string(max_product_variants)
        <> ".",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("path", json.array([root_name, argument_name], json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

@internal
pub fn read_idempotency_key(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(String) {
  let directive_arguments = case field {
    Field(directives: directives, ..) ->
      directives
      |> list.filter_map(fn(directive) {
        case directive {
          Directive(name: name, arguments: arguments, ..)
            if name.value == "idempotent"
          -> Ok(arguments)
          _ -> Error(Nil)
        }
      })
      |> list.first
      |> option.from_result
    _ -> None
  }
  case directive_arguments {
    None -> None
    Some(arguments) -> {
      let argument = case find_argument(arguments, "key") {
        Some(argument) -> Some(argument)
        None -> find_argument(arguments, "idempotencyKey")
      }
      case argument {
        Some(Argument(value: StringValue(value: value, ..), ..)) ->
          non_empty_string(value)
        Some(Argument(value: VariableValue(variable: variable), ..)) ->
          case dict.get(variables, variable.name.value) {
            Ok(StringVal(value)) -> non_empty_string(value)
            _ -> None
          }
        _ -> None
      }
    }
  }
}

@internal
pub fn parse_unsigned_int_string(value: String) -> Option(Int) {
  let trimmed = string.trim(value)
  case
    string.length(trimmed) > 0
    && list.all(string.to_graphemes(trimmed), is_decimal_digit)
  {
    False -> None
    True ->
      case int.parse(trimmed) {
        Ok(parsed) -> Some(parsed)
        Error(_) -> None
      }
  }
}

@internal
pub fn resolved_input_to_json(
  input: Option(Dict(String, ResolvedValue)),
) -> Json {
  case input {
    Some(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
    None -> json.null()
  }
}

@internal
pub fn captured_object_or_null(
  value: Option(Dict(String, ResolvedValue)),
) -> CapturedJsonValue {
  case value {
    Some(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, resolved_value_to_captured(value))
        }),
      )
    None -> CapturedNull
  }
}

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_int_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Int) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_string_array_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(List(String)) {
  case captured_object_field(value, name) {
    Some(CapturedArray(items)) ->
      Some(
        list.filter_map(items, fn(item) {
          case item {
            CapturedString(value) -> Ok(value)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

@internal
pub fn captured_number_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_string(value))
    Some(CapturedFloat(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

@internal
pub fn mutation_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_resource_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

@internal
pub fn mutation_rejected_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: True,
  )
}

@internal
pub fn mutation_error_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

@internal
pub fn mutation_error_with_null_data_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [#(key, json.null())],
    staging_failed: False,
  )
}

@internal
pub fn user_errors_source(errors: List(ProductUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let ProductUserError(field: field, message: message, code: code) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", graphql_helpers.option_string_source(code)),
      ])
    }),
  )
}

@internal
pub fn nullable_field_user_errors_source(
  errors: List(NullableFieldUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let NullableFieldUserError(field: field, message: message) = error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
      ])
    }),
  )
}

@internal
pub fn store_slug_from_admin_origin(origin: String) -> Option(String) {
  origin
  |> string.split("/")
  |> segment_after_store
  |> option.then(fn(slug) { trimmed_non_empty(slug) |> option.from_result })
}

@internal
pub fn read_numeric_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> NumericRead {
  case dict.get(input, name) {
    Error(_) -> NumericMissing
    Ok(NullVal) -> NumericNull
    Ok(IntVal(value)) -> NumericValue(int.to_float(value))
    Ok(FloatVal(value)) -> NumericValue(value)
    Ok(StringVal(value)) ->
      case int.parse(value) {
        Ok(parsed) -> NumericValue(int.to_float(parsed))
        Error(_) ->
          case float.parse(value) {
            Ok(parsed) -> NumericValue(parsed)
            Error(_) -> NumericNotANumber
          }
      }
    _ -> NumericNotANumber
  }
}

@internal
pub fn read_non_empty_string_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case read_string_field(input, name) {
    Some(value) ->
      case string.length(string.trim(value)) > 0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
}

//// Mirrors the operation-time helpers from `src/proxy/graphql-helpers.ts`.
////
//// The TS module is a grab bag covering value resolution, scalar reads,
//// projection, pagination, and connection serialization. Keep this
//// module broad enough that endpoint groups can share Shopify-like
//// GraphQL behavior instead of rebuilding local loops and projectors.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type Selection, type SelectionSet, Argument,
  Field, FragmentDefinition, FragmentSpread, InlineFragment, IntValue, NamedType,
  SelectionSet,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source

/// Whether `get_selected_child_fields` should also flatten field
/// selections that appear inside inline fragments. Mirrors the TS
/// `SelectedFieldOptions.includeInlineFragments`.
pub type SelectedFieldOptions {
  SelectedFieldOptions(include_inline_fragments: Bool)
}

/// Default options matching the TS `{}` argument.
pub fn default_selected_field_options() -> SelectedFieldOptions {
  SelectedFieldOptions(include_inline_fragments: False)
}

/// Response key for a `Field` selection: the alias if present, otherwise
/// the underlying field name. Mirrors `getFieldResponseKey`.
///
/// Returns the field's `name.value` for non-Field selections, which lets
/// callers thread the helper through fragment-flattening flows without
/// needing to special-case shape mismatches. The TS helper isn't
/// generic, but Gleam's exhaustive pattern matching forces us to handle
/// every variant — and the TS code never calls it on non-Field nodes.
pub fn get_field_response_key(field: Selection) -> String {
  case field {
    Field(alias: Some(alias), ..) -> alias.value
    Field(name: name, ..) -> name.value
    FragmentSpread(name: name, ..) -> name.value
    InlineFragment(..) -> ""
  }
}

/// Direct-child `Field` selections of a parent `Field`. Optionally
/// flattens `Field` selections from inline fragments. Mirrors
/// `getSelectedChildFields`.
pub fn get_selected_child_fields(
  field: Selection,
  options: SelectedFieldOptions,
) -> List(Selection) {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  flatten_field_selections(selections, options)
}

/// Direct child selections of a field, without inline-fragment flattening.
/// Returns `[]` when the selection is not a `Field` or has no selection set.
pub fn field_raw_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

/// Selections inside an optional selection set, with empty fallback when None.
pub fn selection_set_selections(ss: Option(SelectionSet)) -> List(Selection) {
  case ss {
    Some(SelectionSet(selections: selections, ..)) -> selections
    None -> []
  }
}

fn flatten_field_selections(
  selections: List(Selection),
  options: SelectedFieldOptions,
) -> List(Selection) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(..) -> [selection]
      InlineFragment(selection_set: ss, ..) ->
        case options.include_inline_fragments {
          True -> {
            let SelectionSet(selections: inner, ..) = ss
            list.filter(inner, fn(s) {
              case s {
                Field(..) -> True
                _ -> False
              }
            })
          }
          False -> []
        }
      _ -> []
    }
  })
}

/// Serialize a connection's `pageInfo` block when there are no items —
/// the only shape the events handler ever needs. Mirrors
/// `serializeEmptyConnectionPageInfo` (which delegates to
/// `serializeConnectionPageInfo` with empty items).
pub fn serialize_empty_connection_page_info(
  page_info_field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  let entries =
    list.map(get_selected_child_fields(page_info_field, options), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(False))
            "hasPreviousPage" -> #(key, json.bool(False))
            "startCursor" -> #(key, json.null())
            "endCursor" -> #(key, json.null())
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

/// Serialize an empty connection: `nodes` and `edges` become empty
/// arrays, `pageInfo` reflects the empty page, anything else becomes
/// `null`. Equivalent to calling the TS `serializeConnection` with
/// `items: []`, but specialised for the empty case to avoid pulling
/// in the rest of the pagination machinery.
pub fn serialize_empty_connection(
  field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  let entries =
    list.map(get_selected_child_fields(field, options), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "nodes" -> #(key, json.array([], fn(x) { x }))
            "edges" -> #(key, json.array([], fn(x) { x }))
            "pageInfo" -> #(
              key,
              serialize_empty_connection_page_info(child, options),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

/// Read a nullable integer argument from a field, resolving variables
/// with the same semantics as `root_field.get_field_arguments`.
pub fn read_nullable_int_argument(
  field: Selection,
  argument_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Int) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, argument_name) {
        Ok(root_field.IntVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

/// Read a nullable string argument from a field, resolving variables
/// with the same semantics as `root_field.get_field_arguments`.
pub fn read_nullable_string_argument(
  field: Selection,
  argument_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, argument_name) {
        Ok(root_field.StringVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

/// Resolved argument dictionary for a field, falling back to an empty
/// dict when arguments cannot be resolved. Wraps
/// `root_field.get_field_arguments` so callers can `dict.get` without
/// thinking about the outer Result.
pub fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

/// Read a string argument from a resolved-arg dict. Returns `None`
/// when the argument is absent, null, or not a string. An empty
/// string is preserved as `Some("")` — handlers that distinguish
/// `""` from missing (e.g. to reproduce Shopify's "invalid global id
/// ''" variable error) need this faithful read. Callers that prefer
/// "empty as missing" should use `read_arg_string_nonempty`.
pub fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

/// Read a string argument, but treat empty strings as missing. Returns
/// `None` when the argument is absent, null, not a string, or `""`.
/// This is the convenience semantic many handlers use when an empty
/// value is never meaningful (e.g. ids, handles, locale codes).
pub fn read_arg_string_nonempty(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal("")) -> None
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

/// Read an integer argument from a resolved-arg dict. Returns `None`
/// when the argument is absent, null, or not an integer.
pub fn read_arg_int(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(args, name) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

/// Read a boolean argument from a resolved-arg dict. Returns `None`
/// when the argument is absent, null, or not a boolean. Callers that
/// want a defaulted `Bool` should pipe through `option.unwrap(False)`.
pub fn read_arg_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

/// Read a string-list argument from a resolved-arg dict. Returns
/// `None` when the argument is absent, null, or not a list. Non-string
/// entries are filtered out, matching the local helper semantics used
/// by domain mutation handlers.
pub fn read_arg_string_list(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(List(String)) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(values)) ->
      Some(
        list.filter_map(values, fn(value) {
          case value {
            root_field.StringVal(item) -> Ok(item)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

/// Read an object argument from a resolved-arg dict. Returns `None`
/// when the argument is absent, null, or not an object. Callers that
/// want an empty-default `Dict` should pipe through
/// `option.unwrap(dict.new())`.
pub fn read_arg_object(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, name) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

/// Encode an `Option(String)` as JSON: the string when present, `null`
/// when absent. Common shape for nullable scalar fields in mutation
/// payloads.
pub fn option_string_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

/// Wrap a successful response payload in the standard `{"data": ...}`
/// GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// `Option(String)` → `SourceValue`: `SrcString` when present, `SrcNull`
/// when absent.
pub fn option_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

/// `Option(Bool)` → `SourceValue`.
pub fn option_bool_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(b) -> SrcBool(b)
    None -> SrcNull
  }
}

/// `Option(Int)` → `SourceValue`.
pub fn option_int_source(value: Option(Int)) -> SourceValue {
  case value {
    Some(i) -> SrcInt(i)
    None -> SrcNull
  }
}

/// One-element JSON array describing the line/column of `loc` in
/// `document`. Mirrors GraphQL's `errors[].locations` shape.
pub fn locations_json(loc: Location, document: String) -> Json {
  let computed =
    graphql_location.get_location(source.new(document), position: loc.start)
  json.preprocessed_array([
    json.object([
      #("line", json.int(computed.line)),
      #("column", json.int(computed.column)),
    ]),
  ])
}

/// `locations_json` for a field selection: extract the location from
/// the field, or return an empty array when the field carries no
/// location. Mirrors how a few handlers attach error locations to the
/// originating selection without unwrapping manually.
pub fn field_locations_json(field: Selection, document: String) -> Json {
  case field {
    Field(loc: Some(loc), ..) -> locations_json(loc, document)
    _ -> json.preprocessed_array([])
  }
}

// ---------------------------------------------------------------------------
// Projection helpers
// ---------------------------------------------------------------------------

/// JSON-shaped source value the projector walks. Mirrors the
/// `Record<string, unknown>` source argument in the TS projector. Kept
/// local so `graphql_helpers` doesn't depend on `root_field` (which
/// happens to define an identical type for argument resolution).
pub type SourceValue {
  SrcNull
  SrcString(String)
  SrcBool(Bool)
  SrcInt(Int)
  SrcFloat(Float)
  SrcList(List(SourceValue))
  SrcObject(Dict(String, SourceValue))
}

/// Lookup table from fragment name to its definition. Mirrors the
/// `FragmentMap` returned by `getDocumentFragments`.
pub type FragmentMap =
  Dict(String, Definition)

/// Build a fragment map from a parsed document. Returns an empty map
/// when the document fails to parse — this matches the spirit of the
/// TS code where a parse failure short-circuits the caller before this
/// helper is invoked.
pub fn get_document_fragments(document: String) -> FragmentMap {
  case parser.parse(source.new(document)) {
    Error(_) -> dict.new()
    Ok(doc) ->
      list.fold(doc.definitions, dict.new(), fn(acc, def) {
        case def {
          FragmentDefinition(name: name, ..) ->
            dict.insert(acc, name.value, def)
          _ -> acc
        }
      })
  }
}

/// Default `__typename`-based gate: a type condition applies when the
/// source object lacks a `__typename` or its `__typename` matches the
/// condition. Mirrors `defaultGraphqlTypeConditionApplies`.
pub fn default_type_condition_applies(
  source: Dict(String, SourceValue),
  type_condition: Option(String),
) -> Bool {
  case type_condition {
    None -> True
    Some(cond) ->
      case dict.get(source, "__typename") {
        Ok(SrcString(name)) ->
          name == cond || interface_type_applies(cond, name)
        // No __typename ⇒ apply (TS uses `!sourceTypename || sourceTypename === typeCondition`).
        _ -> True
      }
  }
}

fn interface_type_applies(type_condition: String, typename: String) -> Bool {
  case type_condition {
    "Catalog" ->
      list.contains(
        ["AppCatalog", "CompanyLocationCatalog", "MarketCatalog"],
        typename,
      )
    _ -> False
  }
}

/// Project a value through a selection set. The TS version short-circuits
/// arrays element-wise and lets non-objects pass through unchanged;
/// Gleam mirrors that exactly via the `SourceValue` constructors.
pub fn project_graphql_value(
  value: SourceValue,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case value {
    SrcList(items) ->
      json.array(items, fn(item) {
        project_graphql_value(item, selections, fragments)
      })
    SrcObject(source) -> project_graphql_object(source, selections, fragments)
    other -> source_to_json(other)
  }
}

/// Project an object through a list of selections, honouring inline
/// fragments and fragment spreads. Mirrors `projectGraphqlObject`. Type
/// conditions use the default `__typename` gate; the TS overload that
/// lets callers customise the projector or the gate is intentionally
/// out of scope for this slice.
pub fn project_graphql_object(
  source: Dict(String, SourceValue),
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.flat_map(selections, fn(selection) {
      case selection {
        InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
          let cond = case tc {
            Some(NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case default_type_condition_applies(source, cond) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              flatten_object_entries(source, inner, fragments)
            }
            False -> []
          }
        }
        FragmentSpread(name: name, ..) ->
          case dict.get(fragments, name.value) {
            Ok(FragmentDefinition(
              type_condition: NamedType(name: cond_name, ..),
              selection_set: SelectionSet(selections: inner, ..),
              ..,
            )) ->
              case
                default_type_condition_applies(source, Some(cond_name.value))
              {
                True -> flatten_object_entries(source, inner, fragments)
                False -> []
              }
            _ -> []
          }
        Field(..) -> [project_field(source, selection, fragments)]
      }
    })
  json.object(entries)
}

fn flatten_object_entries(
  source: Dict(String, SourceValue),
  selections: List(Selection),
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  list.flat_map(selections, fn(selection) {
    case selection {
      InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
        let cond = case tc {
          Some(NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        case default_type_condition_applies(source, cond) {
          True -> {
            let SelectionSet(selections: inner, ..) = ss
            flatten_object_entries(source, inner, fragments)
          }
          False -> []
        }
      }
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            type_condition: NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case default_type_condition_applies(source, Some(cond_name.value)) {
              True -> flatten_object_entries(source, inner, fragments)
              False -> []
            }
          _ -> []
        }
      Field(..) -> [project_field(source, selection, fragments)]
    }
  })
}

fn project_field(
  source: Dict(String, SourceValue),
  field: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  #(key, project_graphql_object_field_value(source, field, fragments))
}

/// Project one selected field from a `SourceValue`, returning the field value
/// rather than an enclosing object. Domain-specific projectors can use this
/// when a few argument-aware fields need custom handling while all sibling
/// fields should retain the shared projection semantics.
pub fn project_graphql_field_value(
  value: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case value {
    SrcObject(source) ->
      project_graphql_object_field_value(source, field, fragments)
    _ -> source_to_json(value)
  }
}

fn project_graphql_object_field_value(
  source: Dict(String, SourceValue),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> lookup_typename(source)
        field_name -> {
          let raw = lookup_or_synthesise(source, field_name)
          let raw = apply_literal_first_window(raw, field)
          case ss {
            Some(SelectionSet(selections: selections, ..)) ->
              project_graphql_value(raw, selections, fragments)
            None -> source_to_json(raw)
          }
        }
      }
    _ -> json.null()
  }
}

fn apply_literal_first_window(
  value: SourceValue,
  field: Selection,
) -> SourceValue {
  case value, literal_first_arg(field) {
    SrcObject(source), Some(first) -> {
      let source = limit_source_list(source, "nodes", first)
      let source = limit_source_list(source, "edges", first)
      SrcObject(source)
    }
    _, _ -> value
  }
}

fn literal_first_arg(field: Selection) -> Option(Int) {
  case field {
    Field(arguments: arguments, ..) ->
      arguments
      |> list.find_map(fn(argument) {
        case argument {
          Argument(name: name, value: IntValue(value: value, ..), ..)
            if name.value == "first"
          ->
            case int.parse(value) {
              Ok(parsed) -> Ok(parsed)
              Error(_) -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn limit_source_list(
  source: Dict(String, SourceValue),
  key: String,
  first: Int,
) -> Dict(String, SourceValue) {
  case dict.get(source, key) {
    Ok(SrcList(items)) ->
      dict.insert(source, key, SrcList(list.take(items, int.max(0, first))))
    _ -> source
  }
}

fn lookup_typename(source: Dict(String, SourceValue)) -> Json {
  case dict.get(source, "__typename") {
    Ok(value) -> source_to_json(value)
    Error(_) -> json.null()
  }
}

/// Mirror the `nodes` ⇄ `edges` synthesis the TS projector does when a
/// connection only carries `edges`. Returns `SrcNull` for plain misses.
fn lookup_or_synthesise(
  source: Dict(String, SourceValue),
  field_name: String,
) -> SourceValue {
  case dict.get(source, field_name) {
    Ok(value) -> value
    Error(_) ->
      case field_name == "nodes" {
        True -> synthesise_nodes_from_edges(source)
        False -> SrcNull
      }
  }
}

fn synthesise_nodes_from_edges(
  source: Dict(String, SourceValue),
) -> SourceValue {
  case dict.get(source, "edges") {
    Ok(SrcList(edges)) ->
      SrcList(
        list.map(edges, fn(edge) {
          case edge {
            SrcObject(fields) ->
              dict.get(fields, "node")
              |> result.unwrap(SrcNull)
            _ -> SrcNull
          }
        }),
      )
    _ -> SrcNull
  }
}

/// Emit a `SourceValue` as JSON. Used both for terminal projection
/// results and as the fallback when a leaf field has no selection set.
pub fn source_to_json(value: SourceValue) -> Json {
  case value {
    SrcNull -> json.null()
    SrcString(s) -> json.string(s)
    SrcBool(b) -> json.bool(b)
    SrcInt(i) -> json.int(i)
    SrcFloat(f) -> json.float(f)
    SrcList(items) -> json.array(items, source_to_json)
    SrcObject(fields) ->
      json.object(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, source_to_json(value))
        }),
      )
  }
}

/// Convenience constructor matching the literal-object shorthand the TS
/// handlers use. Each entry is `(name, value)` and is inserted in
/// list order; later entries with the same key win, matching JS object
/// literal semantics.
pub fn src_object(entries: List(#(String, SourceValue))) -> SourceValue {
  SrcObject(
    list.fold(entries, dict.new(), fn(acc, pair) {
      let #(key, value) = pair
      dict.insert(acc, key, value)
    }),
  )
}

/// Null-on-mismatch string reader for source payload boundaries.
pub fn read_string_value(value: SourceValue) -> Option(String) {
  case value {
    SrcString(value) -> Some(value)
    _ -> None
  }
}

/// Null-on-mismatch number reader for source payload boundaries.
pub fn read_number_value(value: SourceValue) -> Option(Float) {
  case value {
    SrcFloat(value) -> Some(value)
    SrcInt(value) -> Some(int.to_float(value))
    _ -> None
  }
}

/// Null-on-mismatch boolean reader for source payload boundaries.
pub fn read_boolean_value(value: SourceValue) -> Option(Bool) {
  case value {
    SrcBool(value) -> Some(value)
    _ -> None
  }
}

/// Filter a source value down to object-shaped array entries.
pub fn read_plain_object_array(
  value: SourceValue,
) -> List(Dict(String, SourceValue)) {
  case value {
    SrcList(items) ->
      list.filter_map(items, fn(item) {
        case item {
          SrcObject(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

/// Read `payload.data[response_key]`, returning `SrcNull` for malformed
/// or absent data to preserve Shopify-like no-data behavior.
pub fn read_graphql_data_response_payload(
  payload: SourceValue,
  response_key: String,
) -> SourceValue {
  case payload {
    SrcObject(root) ->
      case dict.get(root, "data") {
        Ok(SrcObject(data)) ->
          case dict.get(data, response_key) {
            Ok(value) -> value
            Error(_) -> SrcNull
          }
        _ -> SrcNull
      }
    _ -> SrcNull
  }
}

/// Convert an argument-resolution value into the source projector shape.
/// This keeps JSON-shaped values moving through shared helper APIs
/// without resource-local conversion trees.
pub fn resolved_value_to_source(
  value: root_field.ResolvedValue,
) -> SourceValue {
  case value {
    root_field.NullVal -> SrcNull
    root_field.StringVal(value) -> SrcString(value)
    root_field.BoolVal(value) -> SrcBool(value)
    root_field.IntVal(value) -> SrcInt(value)
    root_field.FloatVal(value) -> SrcFloat(value)
    root_field.ListVal(items) ->
      SrcList(list.map(items, resolved_value_to_source))
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.map_values(fields, fn(_key, item) {
          resolved_value_to_source(item)
        }),
      )
  }
}

// ---------------------------------------------------------------------------
// Pagination machinery
// ---------------------------------------------------------------------------

/// The post-paginate slice of a connection: which items survived
/// `first/last/after/before`, and whether the caller should advertise
/// next/previous pages on `pageInfo`. Mirrors `ConnectionWindow<T>`.
pub type ConnectionWindow(a) {
  ConnectionWindow(items: List(a), has_next_page: Bool, has_previous_page: Bool)
}

/// How `paginate_connection_items` should interpret the `after` /
/// `before` cursor arguments. The TS version takes an optional
/// `parseCursor`; in Gleam we always pass a function, defaulting to the
/// `cursor:`-stripping behaviour.
pub type ConnectionWindowOptions {
  ConnectionWindowOptions(parse_cursor: fn(String) -> Option(String))
}

/// Default options matching the TS `{}` argument: cursor strings of the
/// form `cursor:<value>` are unwrapped; otherwise the raw string is
/// returned.
pub fn default_connection_window_options() -> ConnectionWindowOptions {
  ConnectionWindowOptions(parse_cursor: read_connection_cursor_default)
}

/// How `serialize_connection_page_info` formats the cursors and
/// hasNextPage/hasPreviousPage. Mirrors `ConnectionPageInfoOptions`,
/// merged with the underlying `SelectedFieldOptions` for inline-fragment
/// flattening.
pub type ConnectionPageInfoOptions {
  ConnectionPageInfoOptions(
    include_inline_fragments: Bool,
    prefix_cursors: Bool,
    include_cursors: Bool,
    fallback_start_cursor: Option(String),
    fallback_end_cursor: Option(String),
  )
}

/// Defaults: cursors prefixed with `cursor:`, cursors included in
/// pageInfo, no fallback when the connection is empty, no inline
/// fragments flattened.
pub fn default_connection_page_info_options() -> ConnectionPageInfoOptions {
  ConnectionPageInfoOptions(
    include_inline_fragments: False,
    prefix_cursors: True,
    include_cursors: True,
    fallback_start_cursor: None,
    fallback_end_cursor: None,
  )
}

/// Configuration for `serialize_connection`. Mirrors
/// `SerializeConnectionOptions<T>` but uses an explicit record rather
/// than an options object — Gleam doesn't have optional fields, so the
/// record is the natural shape.
pub type SerializeConnectionConfig(a) {
  SerializeConnectionConfig(
    items: List(a),
    has_next_page: Bool,
    has_previous_page: Bool,
    get_cursor_value: fn(a, Int) -> String,
    serialize_node: fn(a, Selection, Int) -> Json,
    selected_field_options: SelectedFieldOptions,
    page_info_options: ConnectionPageInfoOptions,
  )
}

/// Synthetic cursor mirror of `buildSyntheticCursor`. Public because
/// some handlers build cursors out of band before paginating.
pub fn build_synthetic_cursor(id: String) -> String {
  "cursor:" <> id
}

/// Apply `first`/`last`/`after`/`before` to a list of items, returning
/// the windowed slice plus next/previous-page flags. Mirrors
/// `paginateConnectionItems`. The TS version does index arithmetic over
/// `Array.prototype.findIndex`; Gleam's recursive walk produces the
/// same indices.
pub fn paginate_connection_items(
  items: List(a),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  get_cursor_value: fn(a, Int) -> String,
  options: ConnectionWindowOptions,
) -> ConnectionWindow(a) {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let first = read_connection_size_argument(dict.get(args, "first"))
  let last = read_connection_size_argument(dict.get(args, "last"))
  let after =
    read_cursor_argument(dict.get(args, "after"), options.parse_cursor)
  let before =
    read_cursor_argument(dict.get(args, "before"), options.parse_cursor)
  let total = list.length(items)
  let start_index = case after {
    None -> 0
    Some(cursor) ->
      case find_cursor_index(items, get_cursor_value, cursor) {
        Some(idx) -> idx + 1
        None -> 0
      }
  }
  let before_index = case before {
    None -> total
    Some(cursor) ->
      case find_cursor_index(items, get_cursor_value, cursor) {
        Some(idx) -> idx
        None -> total
      }
  }
  let window_start = max_int(0, start_index)
  let window_end = max_int(window_start, before_index)
  let paginated = slice(items, window_start, window_end)
  let has_next_after_window = window_end < total
  let has_prev_before_window = window_start > 0

  // Apply `first` (head limit)
  let #(after_first, has_next_page) = case first {
    None -> #(paginated, has_next_after_window)
    Some(n) -> {
      let limited = list.take(paginated, n)
      let exceeded = list.length(paginated) > n
      #(limited, has_next_after_window || exceeded)
    }
  }

  // Apply `last` (tail limit)
  let #(after_last, has_previous_page) = case last {
    None -> #(after_first, has_prev_before_window)
    Some(n) -> {
      let len = list.length(after_first)
      let limited = list.drop(after_first, max_int(0, len - n))
      let exceeded = len > n
      #(limited, has_prev_before_window || exceeded)
    }
  }

  ConnectionWindow(
    items: after_last,
    has_next_page: has_next_page,
    has_previous_page: has_previous_page,
  )
}

/// Serialize a connection's `pageInfo` block. Mirrors
/// `serializeConnectionPageInfo`. When `include_cursors` is false, both
/// `startCursor` and `endCursor` are emitted as `null`.
pub fn serialize_connection_page_info(
  selection: Selection,
  items: List(a),
  has_next_page: Bool,
  has_previous_page: Bool,
  get_cursor_value: fn(a, Int) -> String,
  options: ConnectionPageInfoOptions,
) -> Json {
  let selected_options =
    SelectedFieldOptions(
      include_inline_fragments: options.include_inline_fragments,
    )
  let entries =
    list.map(get_selected_child_fields(selection, selected_options), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(has_next_page))
            "hasPreviousPage" -> #(key, json.bool(has_previous_page))
            "startCursor" -> #(
              key,
              page_info_start_cursor(items, get_cursor_value, options),
            )
            "endCursor" -> #(
              key,
              page_info_end_cursor(items, get_cursor_value, options),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

/// Serialize a connection: walks the field's selection set and emits
/// `nodes`, `edges`, or `pageInfo` per the items + flags. Mirrors
/// `serializeConnection`.
pub fn serialize_connection(
  field: Selection,
  config: SerializeConnectionConfig(a),
) -> Json {
  serialize_connection_with_field_serializers(
    field,
    config,
    fn(_page_info_field) { None },
    fn(_unknown_field) { json.null() },
  )
}

/// Serialize a connection whose pagination is fully resolved upstream:
/// `hasNextPage` / `hasPreviousPage` are always false, cursors are not
/// surfaced, and inline-fragment flattening is enabled. Bakes in the
/// boilerplate `SerializeConnectionConfig` that captured-list call sites
/// previously rebuilt verbatim.
pub fn serialize_static_connection(
  field: Selection,
  items: List(a),
  cursor_of: fn(a) -> String,
  serialize_node: fn(a, Selection) -> Json,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: False,
      has_previous_page: False,
      get_cursor_value: fn(item, _index) { cursor_of(item) },
      serialize_node: fn(item, selection, _index) {
        serialize_node(item, selection)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: True,
        prefix_cursors: False,
        include_cursors: False,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

/// Full connection serializer with hooks matching the TS helper's
/// `serializePageInfo` and `serializeUnknownField` options.
pub fn serialize_connection_with_field_serializers(
  field: Selection,
  config: SerializeConnectionConfig(a),
  serialize_page_info: fn(Selection) -> Option(Json),
  serialize_unknown_field: fn(Selection) -> Json,
) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, config.selected_field_options),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "nodes" -> #(
                key,
                json.array(enumerate(config.items), fn(pair) {
                  config.serialize_node(pair.0, child, pair.1)
                }),
              )
              "edges" -> #(key, serialize_edges(child, config))
              "pageInfo" -> #(key, case serialize_page_info(child) {
                Some(value) -> value
                None ->
                  serialize_connection_page_info(
                    child,
                    config.items,
                    config.has_next_page,
                    config.has_previous_page,
                    config.get_cursor_value,
                    config.page_info_options,
                  )
              })
              _ -> #(key, serialize_unknown_field(child))
            }
          _ -> #(key, serialize_unknown_field(child))
        }
      },
    )
  json.object(entries)
}

fn serialize_edges(
  edges_field: Selection,
  config: SerializeConnectionConfig(a),
) -> Json {
  json.array(enumerate(config.items), fn(pair) {
    let item = pair.0
    let index = pair.1
    let edge_entries =
      list.map(
        get_selected_child_fields(edges_field, config.selected_field_options),
        fn(edge_child) {
          let edge_key = get_field_response_key(edge_child)
          case edge_child {
            Field(name: name, ..) ->
              case name.value {
                "cursor" -> #(
                  edge_key,
                  json.string(format_connection_cursor(
                    item,
                    index,
                    config.get_cursor_value,
                    config.page_info_options,
                  )),
                )
                "node" -> #(
                  edge_key,
                  config.serialize_node(item, edge_child, index),
                )
                _ -> #(edge_key, json.null())
              }
            _ -> #(edge_key, json.null())
          }
        },
      )
    json.object(edge_entries)
  })
}

fn page_info_start_cursor(
  items: List(a),
  get_cursor_value: fn(a, Int) -> String,
  options: ConnectionPageInfoOptions,
) -> Json {
  case options.include_cursors {
    False -> json.null()
    True ->
      case items {
        [first, ..] ->
          json.string(format_connection_cursor(
            first,
            0,
            get_cursor_value,
            options,
          ))
        [] ->
          case options.fallback_start_cursor {
            Some(s) -> json.string(s)
            None -> json.null()
          }
      }
  }
}

fn page_info_end_cursor(
  items: List(a),
  get_cursor_value: fn(a, Int) -> String,
  options: ConnectionPageInfoOptions,
) -> Json {
  case options.include_cursors {
    False -> json.null()
    True ->
      case list.length(items) {
        0 ->
          case options.fallback_end_cursor {
            Some(s) -> json.string(s)
            None -> json.null()
          }
        len ->
          case last_item(items) {
            Some(item) ->
              json.string(format_connection_cursor(
                item,
                len - 1,
                get_cursor_value,
                options,
              ))
            None -> json.null()
          }
      }
  }
}

fn format_connection_cursor(
  item: a,
  index: Int,
  get_cursor_value: fn(a, Int) -> String,
  options: ConnectionPageInfoOptions,
) -> String {
  let raw = get_cursor_value(item, index)
  case options.prefix_cursors {
    True -> build_synthetic_cursor(raw)
    False -> raw
  }
}

fn read_connection_size_argument(
  result_value: Result(root_field.ResolvedValue, Nil),
) -> Option(Int) {
  case result_value {
    Ok(root_field.IntVal(n)) ->
      case n >= 0 {
        True -> Some(n)
        False -> None
      }
    _ -> None
  }
}

fn read_cursor_argument(
  result_value: Result(root_field.ResolvedValue, Nil),
  parse_cursor: fn(String) -> Option(String),
) -> Option(String) {
  case result_value {
    Ok(root_field.StringVal(s)) -> parse_cursor(s)
    _ -> None
  }
}

fn read_connection_cursor_default(raw: String) -> Option(String) {
  case string.starts_with(raw, "cursor:") {
    True -> {
      let stripped = string.drop_start(raw, 7)
      case stripped {
        "" -> None
        other -> Some(other)
      }
    }
    False ->
      case raw {
        "" -> None
        other -> Some(other)
      }
  }
}

fn find_cursor_index(
  items: List(a),
  get_cursor_value: fn(a, Int) -> String,
  cursor: String,
) -> Option(Int) {
  find_cursor_index_loop(items, get_cursor_value, cursor, 0)
}

fn find_cursor_index_loop(
  items: List(a),
  get_cursor_value: fn(a, Int) -> String,
  cursor: String,
  index: Int,
) -> Option(Int) {
  case items {
    [] -> None
    [item, ..rest] ->
      case get_cursor_value(item, index) == cursor {
        True -> Some(index)
        False ->
          find_cursor_index_loop(rest, get_cursor_value, cursor, index + 1)
      }
  }
}

fn slice(items: List(a), start: Int, end: Int) -> List(a) {
  case end <= start {
    True -> []
    False -> list.take(list.drop(items, start), end - start)
  }
}

fn enumerate(items: List(a)) -> List(#(a, Int)) {
  list.index_map(items, fn(item, index) { #(item, index) })
}

fn last_item(items: List(a)) -> Option(a) {
  case list.last(items) {
    Ok(item) -> Some(item)
    Error(_) -> None
  }
}

fn max_int(a: Int, b: Int) -> Int {
  case a >= b {
    True -> a
    False -> b
  }
}

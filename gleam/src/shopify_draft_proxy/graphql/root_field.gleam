//// Mirrors `src/graphql/root-field.ts`.
////
//// Helpers for pulling structured data out of a parsed operation: the
//// root `Field` node, its arguments resolved against a variable map,
//// and the names of its sub-selections.
////
//// `ResolvedValue` is the Gleam analogue of TypeScript's `unknown`
//// here — a tagged union of the JSON-shaped values an argument can
//// take after variable substitution. The TS version returns
//// `Record<string, unknown>`, which the proxy then feeds straight into
//// endpoint handlers; this enum gives us the same coverage with
//// type safety.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Definition, type Document, type Selection, type Value,
  BooleanValue, EnumValue, Field, FloatValue, IntValue, ListValue, NullValue,
  ObjectValue, OperationDefinition, SelectionSet, StringValue, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/source

/// JSON-shaped value an argument resolves to once variables have been
/// substituted. Mirrors the `unknown` returned by
/// `resolveValueNode` in the TS version.
pub type ResolvedValue {
  NullVal
  StringVal(String)
  BoolVal(Bool)
  IntVal(Int)
  FloatVal(Float)
  ListVal(List(ResolvedValue))
  ObjectVal(Dict(String, ResolvedValue))
}

/// Errors `root_field` helpers can produce.
pub type RootFieldError {
  ParseFailed(parser.ParseError)
  NoOperationFound
  NoRootField
  /// graphql-js silently allows `Number.parseInt("x")` to return NaN,
  /// but the lexer guarantees `IntValue.value` is digits-only. If we
  /// somehow see a value that doesn't parse, surface it explicitly
  /// rather than fabricating zero.
  InvalidNumberLiteral(String)
}

/// Return the first root `Field` selection of the document's first
/// operation. Mirrors `getRootField`.
pub fn get_root_field(document: String) -> Result(Selection, RootFieldError) {
  use fields <- result.try(get_root_fields(document))
  case fields {
    [] -> Error(NoRootField)
    [f, ..] -> Ok(f)
  }
}

/// Return every root-level `Field` selection of the first operation.
/// Mirrors `getRootFields`. Fragment spreads / inline fragments are
/// dropped, matching the TS `.filter(kind === Kind.FIELD)`.
pub fn get_root_fields(
  document: String,
) -> Result(List(Selection), RootFieldError) {
  use op <- result.try(get_operation(document))
  let assert OperationDefinition(selection_set: ss, ..) = op
  let SelectionSet(selections: selections, ..) = ss
  Ok(only_fields(selections))
}

/// Resolve a single field's argument list to a `Dict` keyed by argument
/// name. Variable references are looked up in `variables`; missing
/// variables resolve to `NullVal` (graphql-js's `?? null`).
pub fn get_field_arguments(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Result(Dict(String, ResolvedValue), RootFieldError) {
  case field {
    Field(arguments: args, ..) -> resolve_arguments(args, variables)
    _ -> Error(NoRootField)
  }
}

/// Convenience wrapper around `get_root_field` + `get_field_arguments`.
/// Mirrors `getRootFieldArguments`.
pub fn get_root_field_arguments(
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Dict(String, ResolvedValue), RootFieldError) {
  use field <- result.try(get_root_field(document))
  get_field_arguments(field, variables)
}

/// Names of the top-level field selections inside a `Field`'s
/// selection set. Mirrors `getSelectionNames`.
pub fn get_selection_names(field: Selection) -> List(String) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      list.filter_map(selections, fn(sel) {
        case sel {
          Field(name: name, ..) -> Ok(name.value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn get_operation(document: String) -> Result(Definition, RootFieldError) {
  case parser.parse(source.new(document)) {
    Error(err) -> Error(ParseFailed(err))
    Ok(doc) -> first_operation(doc)
  }
}

fn first_operation(doc: Document) -> Result(Definition, RootFieldError) {
  case find_operation(doc.definitions) {
    None -> Error(NoOperationFound)
    Some(op) -> Ok(op)
  }
}

fn find_operation(definitions: List(Definition)) -> Option(Definition) {
  case definitions {
    [] -> None
    [d, ..rest] ->
      case d {
        OperationDefinition(..) -> Some(d)
        _ -> find_operation(rest)
      }
  }
}

fn only_fields(selections: List(Selection)) -> List(Selection) {
  list.filter(selections, fn(sel) {
    case sel {
      Field(..) -> True
      _ -> False
    }
  })
}

fn resolve_arguments(
  args: List(Argument),
  variables: Dict(String, ResolvedValue),
) -> Result(Dict(String, ResolvedValue), RootFieldError) {
  list.try_fold(args, dict.new(), fn(acc, arg) {
    use value <- result.try(resolve_value(arg.value, variables))
    Ok(dict.insert(acc, arg.name.value, value))
  })
}

fn resolve_value(
  value: Value,
  variables: Dict(String, ResolvedValue),
) -> Result(ResolvedValue, RootFieldError) {
  case value {
    NullValue(..) -> Ok(NullVal)
    StringValue(value: s, ..) -> Ok(StringVal(s))
    EnumValue(value: s, ..) -> Ok(StringVal(s))
    BooleanValue(value: b, ..) -> Ok(BoolVal(b))
    IntValue(value: raw, ..) ->
      case int.parse(raw) {
        Ok(n) -> Ok(IntVal(n))
        Error(_) -> Error(InvalidNumberLiteral(raw))
      }
    FloatValue(value: raw, ..) ->
      case parse_float_literal(raw) {
        Ok(f) -> Ok(FloatVal(f))
        Error(_) -> Error(InvalidNumberLiteral(raw))
      }
    ListValue(values: values, ..) -> {
      use resolved <- result.try(
        list.try_map(values, fn(v) { resolve_value(v, variables) }),
      )
      Ok(ListVal(resolved))
    }
    ObjectValue(fields: fields, ..) -> {
      use entries <- result.try(
        list.try_fold(fields, dict.new(), fn(acc, f) {
          use v <- result.try(resolve_value(f.value, variables))
          Ok(dict.insert(acc, f.name.value, v))
        }),
      )
      Ok(ObjectVal(entries))
    }
    VariableValue(variable: var) -> {
      case dict.get(variables, var.name.value) {
        Ok(v) -> Ok(v)
        Error(_) -> Ok(NullVal)
      }
    }
  }
}

/// Gleam's `float.parse` rejects exponent forms without a decimal
/// point ("1e10"), but graphql-js's lexer happily produces them.
/// Inject ".0" before the exponent marker so the parser accepts it.
fn parse_float_literal(raw: String) -> Result(Float, Nil) {
  case float.parse(raw) {
    Ok(f) -> Ok(f)
    Error(_) -> {
      let normalized = inject_decimal_before_exponent(raw)
      case float.parse(normalized) {
        Ok(f) -> Ok(f)
        Error(_) ->
          case int.parse(raw) {
            Ok(n) -> Ok(int.to_float(n))
            Error(_) -> Error(Nil)
          }
      }
    }
  }
}

fn inject_decimal_before_exponent(raw: String) -> String {
  let lowered = string.lowercase(raw)
  case string.split_once(lowered, "e") {
    Error(_) -> raw
    Ok(#(mantissa, _)) ->
      case string.contains(mantissa, ".") {
        True -> raw
        False -> {
          let prefix_len = string.length(mantissa)
          let #(left, right) = case string.length(raw) >= prefix_len {
            True -> {
              let l = string.slice(raw, 0, prefix_len)
              let r =
                string.slice(raw, prefix_len, string.length(raw) - prefix_len)
              #(l, r)
            }
            False -> #(raw, "")
          }
          left <> ".0" <> right
        }
      }
  }
}

// Re-export so callers don't need to import parse_operation just for this.
pub fn parse_operation_summary(
  document: String,
) -> Result(parse_operation.ParsedOperation, parse_operation.ParseOperationError) {
  parse_operation.parse_operation(document)
}

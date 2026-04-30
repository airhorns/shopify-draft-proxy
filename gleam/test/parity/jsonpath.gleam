//// Minimal JSONPath evaluator used by the parity runner.
////
//// Supported syntax:
////   $                   – the root
////   $.foo               – object key
////   $.foo.bar           – nested key
////   $[3]                – array index
////   $["odd.key"]        – object key
////   $.foo[*]            – wildcard step, for delete/exclude traversal
////   $.foo[3].bar        – mixed
////
//// We deliberately do not implement filters or recursive descent.

import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import parity/json_value.{type JsonValue}

pub type Step {
  FieldStep(name: String)
  IndexStep(index: Int)
  WildcardStep
}

pub type ParseError {
  ParseError(message: String)
}

pub fn parse(path: String) -> Result(List(Step), ParseError) {
  case path {
    "$" -> Ok([])
    "$" <> rest -> parse_steps(rest, [])
    _ ->
      Error(ParseError(
        message: "JSONPath must start with '$' but got: " <> path,
      ))
  }
}

fn parse_steps(
  rest: String,
  acc: List(Step),
) -> Result(List(Step), ParseError) {
  case rest {
    "" -> Ok(list.reverse(acc))
    "." <> tail -> parse_field(tail, acc)
    "[" <> tail -> parse_index(tail, acc)
    _ ->
      Error(ParseError(
        message: "Unexpected character in JSONPath segment: " <> rest,
      ))
  }
}

fn parse_field(
  rest: String,
  acc: List(Step),
) -> Result(List(Step), ParseError) {
  let #(name, tail) = take_field_name(rest, "")
  case name {
    "" -> Error(ParseError(message: "Empty field name in JSONPath"))
    _ -> parse_steps(tail, [FieldStep(name: name), ..acc])
  }
}

fn take_field_name(input: String, acc: String) -> #(String, String) {
  case string.pop_grapheme(input) {
    Error(_) -> #(acc, "")
    Ok(#(grapheme, rest)) ->
      case grapheme {
        "." | "[" -> #(acc, grapheme <> rest)
        _ -> take_field_name(rest, acc <> grapheme)
      }
  }
}

fn parse_index(
  rest: String,
  acc: List(Step),
) -> Result(List(Step), ParseError) {
  case rest {
    "*]" <> tail -> parse_steps(tail, [WildcardStep, ..acc])
    "\"" <> quoted -> parse_quoted_field(quoted, acc)
    _ -> {
      let #(digits, after_digits) = take_digits(rest, "")
      case digits, after_digits {
        "", _ -> Error(ParseError(message: "Empty index in JSONPath"))
        _, "]" <> tail -> {
          let assert Ok(n) = parse_int(digits)
          parse_steps(tail, [IndexStep(index: n), ..acc])
        }
        _, _ -> Error(ParseError(message: "Unterminated index in JSONPath"))
      }
    }
  }
}

fn parse_quoted_field(
  rest: String,
  acc: List(Step),
) -> Result(List(Step), ParseError) {
  let #(name, tail) = take_quoted(rest, "")
  case tail {
    "\"]" <> after -> parse_steps(after, [FieldStep(name: name), ..acc])
    _ -> Error(ParseError(message: "Unterminated quoted field in JSONPath"))
  }
}

fn take_quoted(input: String, acc: String) -> #(String, String) {
  case string.pop_grapheme(input) {
    Error(_) -> #(acc, "")
    Ok(#(grapheme, rest)) ->
      case grapheme {
        "\"" -> #(acc, grapheme <> rest)
        "\\" ->
          case string.pop_grapheme(rest) {
            Ok(#(escaped, tail)) -> take_quoted(tail, acc <> escaped)
            Error(_) -> #(acc, rest)
          }
        _ -> take_quoted(rest, acc <> grapheme)
      }
  }
}

fn take_digits(input: String, acc: String) -> #(String, String) {
  case string.pop_grapheme(input) {
    Error(_) -> #(acc, "")
    Ok(#(grapheme, rest)) ->
      case is_digit(grapheme) {
        True -> take_digits(rest, acc <> grapheme)
        False -> #(acc, grapheme <> rest)
      }
  }
}

fn is_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn parse_int(s: String) -> Result(Int, Nil) {
  do_parse_int(s, 0)
}

fn do_parse_int(s: String, acc: Int) -> Result(Int, Nil) {
  case string.pop_grapheme(s) {
    Error(_) ->
      case s {
        "" -> Ok(acc)
        _ -> Error(Nil)
      }
    Ok(#(grapheme, rest)) ->
      case grapheme {
        "0" -> do_parse_int(rest, acc * 10 + 0)
        "1" -> do_parse_int(rest, acc * 10 + 1)
        "2" -> do_parse_int(rest, acc * 10 + 2)
        "3" -> do_parse_int(rest, acc * 10 + 3)
        "4" -> do_parse_int(rest, acc * 10 + 4)
        "5" -> do_parse_int(rest, acc * 10 + 5)
        "6" -> do_parse_int(rest, acc * 10 + 6)
        "7" -> do_parse_int(rest, acc * 10 + 7)
        "8" -> do_parse_int(rest, acc * 10 + 8)
        "9" -> do_parse_int(rest, acc * 10 + 9)
        _ -> Error(Nil)
      }
  }
}

/// Evaluate a parsed path against a JsonValue. Returns None if any step
/// fails to resolve (missing field, out-of-bounds index, etc).
pub fn evaluate(value: JsonValue, steps: List(Step)) -> Option(JsonValue) {
  case steps {
    [] -> Some(value)
    [step, ..rest] ->
      case step_into(value, step) {
        Some(next) -> evaluate(next, rest)
        None -> None
      }
  }
}

fn step_into(value: JsonValue, step: Step) -> Option(JsonValue) {
  case step {
    FieldStep(name: name) -> json_value.field(value, name)
    IndexStep(index: idx) -> json_value.index(value, idx)
    WildcardStep -> None
  }
}

/// One-shot: parse the path expression and evaluate it. Returns None
/// for an invalid path or a path that doesn't resolve.
pub fn lookup(value: JsonValue, path: String) -> Option(JsonValue) {
  case parse(path) {
    Ok(steps) -> evaluate(value, steps)
    Error(_) -> None
  }
}

//// Self-describing JSON ADT used by the parity runner.
////
//// `gleam/json` returns its parsed result as a dynamic value, which is
//// awkward to walk recursively for jsonpath eval and structural diffs.
//// We re-parse the dynamic into a strongly-typed `JsonValue` so the rest
//// of the parity layer can pattern-match without fighting decoders.

import gleam/dict
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode
import gleam/float
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string

pub type JsonValue {
  JNull
  JBool(Bool)
  JInt(Int)
  JFloat(Float)
  JString(String)
  JArray(List(JsonValue))
  JObject(List(#(String, JsonValue)))
}

pub type ParseError {
  ParseError(message: String)
}

/// Parse a UTF-8 JSON string into the strongly-typed ADT.
pub fn parse(source: String) -> Result(JsonValue, ParseError) {
  case json.parse(source, decode.dynamic) {
    Ok(dyn) ->
      case from_dynamic(dyn) {
        Ok(value) -> Ok(value)
        Error(message) -> Error(ParseError(message: message))
      }
    Error(_) -> Error(ParseError(message: "invalid JSON"))
  }
}

/// Convert a dynamic value (produced by `json.parse`) into the typed ADT.
/// The conversion is total: every JSON node maps cleanly.
pub fn from_dynamic(value: Dynamic) -> Result(JsonValue, String) {
  case decode.run(value, decode.bool) {
    Ok(b) -> Ok(JBool(b))
    Error(_) -> from_dynamic_non_bool(value)
  }
}

fn from_dynamic_non_bool(value: Dynamic) -> Result(JsonValue, String) {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> Ok(JNull)
    Ok(Some(_)) -> from_dynamic_present(value)
    Error(_) -> from_dynamic_present(value)
  }
}

fn from_dynamic_present(value: Dynamic) -> Result(JsonValue, String) {
  case decode.run(value, decode.int) {
    Ok(n) -> Ok(JInt(n))
    Error(_) ->
      case decode.run(value, decode.float) {
        Ok(f) -> Ok(JFloat(f))
        Error(_) ->
          case decode.run(value, decode.string) {
            Ok(s) -> Ok(JString(s))
            Error(_) ->
              case decode.run(value, decode.list(decode.dynamic)) {
                Ok(items) ->
                  items
                  |> list.try_map(from_dynamic)
                  |> result.map(JArray)
                Error(_) ->
                  case
                    decode.run(
                      value,
                      decode.dict(decode.string, decode.dynamic),
                    )
                  {
                    Ok(d) -> object_from_dict(d)
                    Error(_) -> Error("unsupported JSON shape")
                  }
              }
          }
      }
  }
}

fn object_from_dict(
  d: dict.Dict(String, Dynamic),
) -> Result(JsonValue, String) {
  d
  |> dict.to_list
  |> list.try_map(fn(pair) {
    let #(key, dyn) = pair
    case from_dynamic(dyn) {
      Ok(v) -> Ok(#(key, v))
      Error(e) -> Error(e)
    }
  })
  |> result.map(JObject)
}

/// Render a JsonValue back to canonical JSON. Object keys are emitted
/// in insertion order; this is fine for deterministic diff strings.
pub fn to_string(value: JsonValue) -> String {
  case value {
    JNull -> "null"
    JBool(True) -> "true"
    JBool(False) -> "false"
    JInt(n) -> int.to_string(n)
    JFloat(f) -> float.to_string(f)
    JString(s) -> json.to_string(json.string(s))
    JArray(items) -> "[" <> string.join(list.map(items, to_string), ",") <> "]"
    JObject(entries) ->
      "{"
      <> string.join(
        list.map(entries, fn(pair) {
          let #(k, v) = pair
          json.to_string(json.string(k)) <> ":" <> to_string(v)
        }),
        ",",
      )
      <> "}"
  }
}

/// Lookup a top-level object field. Returns None if `value` is not an
/// object or the field is absent.
pub fn field(value: JsonValue, name: String) -> Option(JsonValue) {
  case value {
    JObject(entries) ->
      case list.find(entries, fn(pair) { pair.0 == name }) {
        Ok(pair) -> Some(pair.1)
        Error(_) -> None
      }
    _ -> None
  }
}

/// Lookup an array index. Returns None if `value` is not an array or
/// the index is out of bounds.
pub fn index(value: JsonValue, idx: Int) -> Option(JsonValue) {
  case value {
    JArray(items) ->
      case list.drop(items, idx) {
        [head, ..] -> Some(head)
        [] -> None
      }
    _ -> None
  }
}

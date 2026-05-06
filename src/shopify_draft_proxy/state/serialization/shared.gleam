import gleam/dict.{type Dict}
import gleam/dynamic/decode.{type Decoder}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

@internal
pub fn dump_field_names(fields: List(#(String, Json))) -> List(String) {
  list.map(fields, fn(field) {
    let #(name, _) = field
    name
  })
}

@internal
pub fn optional_to_json(value: Option(a), encode: fn(a) -> Json) -> Json {
  case value {
    Some(inner) -> encode(inner)
    None -> json.null()
  }
}

@internal
pub fn optional_string(value: Option(String)) -> Json {
  optional_to_json(value, json.string)
}

@internal
pub fn optional_int(value: Option(Int)) -> Json {
  optional_to_json(value, json.int)
}

@internal
pub fn optional_float(value: Option(Float)) -> Json {
  optional_to_json(value, json.float)
}

@internal
pub fn optional_bool(value: Option(Bool)) -> Json {
  optional_to_json(value, json.bool)
}

@internal
pub fn dict_to_json(records: Dict(String, a), encode: fn(a) -> Json) -> Json {
  json.object(
    dict.to_list(records)
    |> list.map(fn(pair) {
      let #(key, value) = pair
      #(key, encode(value))
    }),
  )
}

@internal
pub fn bool_dict_to_json(records: Dict(String, Bool)) -> Json {
  dict_to_json(records, json.bool)
}

@internal
pub fn optional_field(
  name: String,
  default: a,
  decoder: Decoder(a),
  next: fn(a) -> Decoder(b),
) -> Decoder(b) {
  decode.optional_field(name, default, decoder, next)
}

@internal
pub fn optional_string_field(
  name: String,
  next: fn(Option(String)) -> Decoder(a),
) -> Decoder(a) {
  optional_field(name, None, decode.optional(decode.string), next)
}

@internal
pub fn string_list_field(
  name: String,
  next: fn(List(String)) -> Decoder(a),
) -> Decoder(a) {
  optional_field(name, [], decode.list(of: decode.string), next)
}

@internal
pub fn dict_field(
  name: String,
  item_decoder: Decoder(a),
  next: fn(Dict(String, a)) -> Decoder(b),
) -> Decoder(b) {
  optional_field(
    name,
    dict.new(),
    decode.dict(decode.string, item_decoder),
    next,
  )
}

@internal
pub fn bool_dict_field(
  name: String,
  next: fn(Dict(String, Bool)) -> Decoder(a),
) -> Decoder(a) {
  dict_field(name, decode.bool, next)
}

@internal
pub fn require_object_fields(names: List(String)) -> Decoder(Nil) {
  list.fold(names, decode.success(Nil), fn(decoder, name) {
    use _ <- decode.then(decoder)
    use _ <- decode.field(name, decode.dynamic)
    decode.success(Nil)
  })
}

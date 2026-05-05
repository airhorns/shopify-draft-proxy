//// Mirrors the read path of `src/proxy/metafields.ts`.
////
//// This module carries the shared projection helpers
//// (`serializeMetafieldSelection*`), `compareDigest` builder, and
//// Shopify-like value normalization/`jsonValue` parsing used by
//// owner-scoped metafield staging.
////
//// `MetafieldRecordCore` is the projection-shaped record. Mutation
//// passes will likely wrap it in an owner-scoped variant; the read
//// path here treats every metafield uniformly.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SelectedFieldOptions, ConnectionPageInfoOptions,
  SerializeConnectionConfig, default_connection_page_info_options,
  default_connection_window_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, serialize_connection,
}

/// Mirrors `MetafieldRecordCore`. Optional fields are present when the
/// owning domain populates them (mutation-side); read paths from a
/// snapshot may omit any of them. `json_value` is carried as
/// `Option(Json)` so callers can pass through whatever shape the
/// upstream record holds without us having to reify it.
pub type MetafieldRecordCore {
  MetafieldRecordCore(
    id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    json_value: Option(Json),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
  )
}

/// Build the `compareDigest` string for a metafield. Mirrors
/// `makeMetafieldCompareDigest`.
///
/// The TS encodes `JSON.stringify([namespace, key, type, value,
/// jsonValue ?? null, updatedAt ?? null])` as base64url. We mirror
/// that exactly: build a JSON array with the same six elements, render
/// it to a string, then base64url-encode the bytes (no padding —
/// `Buffer.toString('base64url')` strips it).
pub fn make_metafield_compare_digest(metafield: MetafieldRecordCore) -> String {
  let payload =
    json.array(
      [
        json.string(metafield.namespace),
        json.string(metafield.key),
        option_string_to_json(metafield.type_),
        option_string_to_json(metafield.value),
        option_json_to_json(metafield.json_value),
        option_string_to_json(metafield.updated_at),
      ],
      fn(x) { x },
    )
  let body =
    payload
    |> json.to_string
    |> bit_array.from_string
    |> bit_array.base64_url_encode(False)
  "draft:" <> body
}

fn option_string_to_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

fn option_json_to_json(value: Option(Json)) -> Json {
  case value {
    Some(j) -> j
    None -> json.null()
  }
}

/// Project a metafield onto a list of selection nodes. Mirrors
/// `serializeMetafieldSelectionSet`. Unknown fields fall through to
/// `null`, matching the TS default branch. `definition` still returns
/// `null` here because owner-specific callers handle definition-backed
/// behavior at their own boundary.
pub fn serialize_metafield_selection_set(
  metafield: MetafieldRecordCore,
  selections: List(Selection),
) -> Json {
  let entries =
    list.filter_map(selections, fn(selection) {
      case selection {
        Field(name: name, ..) -> {
          let key = get_field_response_key(selection)
          let value = case name.value {
            "__typename" -> json.string("Metafield")
            "id" -> json.string(metafield.id)
            "namespace" -> json.string(metafield.namespace)
            "key" -> json.string(metafield.key)
            "type" -> option_string_to_json(metafield.type_)
            "value" -> option_string_to_json(metafield.value)
            "compareDigest" ->
              case metafield.compare_digest {
                Some(d) -> json.string(d)
                None -> json.string(make_metafield_compare_digest(metafield))
              }
            "jsonValue" -> option_json_to_json(metafield.json_value)
            "createdAt" -> option_string_to_json(metafield.created_at)
            "updatedAt" ->
              case metafield.updated_at, metafield.created_at {
                Some(u), _ -> json.string(u)
                None, Some(c) -> json.string(c)
                None, None -> json.null()
              }
            "ownerType" -> option_string_to_json(metafield.owner_type)
            "definition" -> json.null()
            _ -> json.null()
          }
          Ok(#(key, value))
        }
        _ -> Error(Nil)
      }
    })
  json.object(entries)
}

/// Project a metafield against the child fields of a `Field`.
/// Mirrors `serializeMetafieldSelection`.
pub fn serialize_metafield_selection(
  metafield: MetafieldRecordCore,
  field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  serialize_metafield_selection_set(
    metafield,
    get_selected_child_fields(field, options),
  )
}

/// Serialize a metafields connection. Mirrors
/// `serializeMetafieldsConnection`. Pagination uses the metafield's
/// `id` as the cursor source — same as the TS
/// `(metafield) => metafield.id`.
pub fn serialize_metafields_connection(
  metafields: List(MetafieldRecordCore),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  options: SelectedFieldOptions,
) -> Json {
  let window =
    paginate_connection_items(
      metafields,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: options.include_inline_fragments,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: cursor_value,
      serialize_node: fn(record, node_field, _index) {
        serialize_metafield_selection(record, node_field, options)
      },
      selected_field_options: options,
      page_info_options: page_info_options,
    ),
  )
}

fn cursor_value(record: MetafieldRecordCore, _index: Int) -> String {
  record.id
}

pub fn normalize_metafield_value(
  type_: Option(String),
  value: Option(String),
) -> Option(String) {
  case value {
    None -> None
    Some(raw) ->
      case type_ {
        Some("date_time") -> Some(normalize_date_time_value(raw))
        Some("rating") -> Some(normalize_rating_value_string(raw))
        Some(type_name) ->
          case string.starts_with(type_name, "list.") {
            True ->
              Some(normalize_list_metafield_value_string(
                string.drop_start(type_name, 5),
                raw,
              ))
            False ->
              case is_measurement_metafield_type_name(type_name) {
                True -> Some(normalize_measurement_value_string(raw))
                False -> Some(raw)
              }
          }
        None -> Some(raw)
      }
  }
}

pub fn parse_metafield_json_value(
  type_: Option(String),
  value: Option(String),
) -> Option(Json) {
  case value {
    None -> Some(json.null())
    Some(raw) ->
      case type_ {
        Some("date_time") -> Some(json.string(normalize_date_time_value(raw)))
        Some("number_decimal") | Some("float") -> Some(json.string(raw))
        Some("rating") -> Some(parse_rating_json_value(raw))
        Some(type_name) ->
          case string.starts_with(type_name, "list.") {
            True ->
              Some(parse_list_metafield_json_value(
                string.drop_start(type_name, 5),
                raw,
              ))
            False ->
              case is_measurement_metafield_type_name(type_name) {
                True -> Some(parse_measurement_json_value(type_name, raw))
                False ->
                  case should_parse_metafield_json_value(type_name) {
                    True -> Some(parse_json_or_string(raw))
                    False ->
                      case type_name {
                        "number_integer" | "integer" ->
                          case int.parse(raw) {
                            Ok(n) -> Some(json.int(n))
                            Error(_) -> Some(json.string(raw))
                          }
                        "boolean" -> Some(json.bool(raw == "true"))
                        _ -> Some(json.string(raw))
                      }
                  }
              }
          }
        None -> Some(json.string(raw))
      }
  }
}

pub fn valid_type_names() -> List(String) {
  string.split(valid_type_names_message(), ", ")
}

pub fn valid_type_names_message() -> String {
  "antenna_gain, area, battery_charge_capacity, battery_energy_capacity, boolean, capacitance, color, concentration, data_storage_capacity, data_transfer_rate, date_time, date, dimension, display_density, distance, duration, electric_current, electrical_resistance, energy, float, frequency, id, illuminance, inductance, integer, json_string, json, language, link, list.antenna_gain, list.area, list.battery_charge_capacity, list.battery_energy_capacity, list.boolean, list.capacitance, list.color, list.concentration, list.data_storage_capacity, list.data_transfer_rate, list.date_time, list.date, list.dimension, list.display_density, list.distance, list.duration, list.electric_current, list.electrical_resistance, list.energy, list.frequency, list.illuminance, list.inductance, list.link, list.luminous_flux, list.mass_flow_rate, list.multi_line_text_field, list.number_decimal, list.number_integer, list.power, list.pressure, list.rating, list.resolution, list.rotational_speed, list.single_line_text_field, list.sound_level, list.speed, list.temperature, list.thermal_power, list.url, list.voltage, list.volume, list.volumetric_flow_rate, list.weight, luminous_flux, mass_flow_rate, money, multi_line_text_field, number_decimal, number_integer, power, pressure, rating, resolution, rich_text_field, rotational_speed, single_line_text_field, sound_level, speed, string, temperature, thermal_power, url, voltage, volume, volumetric_flow_rate, weight, company_reference, list.company_reference, customer_reference, list.customer_reference, product_reference, list.product_reference, collection_reference, list.collection_reference, variant_reference, list.variant_reference, file_reference, list.file_reference, product_taxonomy_value_reference, list.product_taxonomy_value_reference, metaobject_reference, list.metaobject_reference, mixed_reference, list.mixed_reference, page_reference, list.page_reference, article_reference, list.article_reference, order_reference, list.order_reference"
}

fn parse_json_or_string(raw: String) -> Json {
  commit.parse_json_value(raw)
  |> commit.json_value_to_json
}

fn normalize_date_time_value(value: String) -> String {
  let lowered = string.lowercase(value)
  case string.ends_with(lowered, "z") {
    True -> string.drop_end(value, 1) <> "+00:00"
    False ->
      case has_timezone_offset(value) {
        True -> value
        False -> value <> "+00:00"
      }
  }
}

fn has_timezone_offset(value: String) -> Bool {
  let len = string.length(value)
  case len >= 6 {
    False -> False
    True -> {
      let sign = string.slice(value, len - 6, 1)
      let colon = string.slice(value, len - 3, 1)
      let sign_ok = sign == "+" || sign == "-"
      sign_ok && colon == ":"
    }
  }
}

fn should_parse_metafield_json_value(type_name: String) -> Bool {
  string.starts_with(type_name, "list.")
  || list.contains(json_object_metafield_types(), type_name)
}

fn json_object_metafield_types() -> List(String) {
  [
    "antenna_gain", "area", "battery_charge_capacity", "battery_energy_capacity",
    "capacitance", "concentration", "data_storage_capacity",
    "data_transfer_rate", "dimension", "display_density", "distance", "duration",
    "electric_current", "electrical_resistance", "energy", "frequency",
    "illuminance", "inductance", "json", "json_string", "link", "luminous_flux",
    "mass_flow_rate", "money", "power", "pressure", "rating", "resolution",
    "rich_text_field", "rotational_speed", "sound_level", "speed", "temperature",
    "thermal_power", "voltage", "volume", "volumetric_flow_rate", "weight",
  ]
}

fn measurement_metafield_types() -> List(String) {
  [
    "antenna_gain", "area", "battery_charge_capacity", "battery_energy_capacity",
    "capacitance", "concentration", "data_storage_capacity",
    "data_transfer_rate", "dimension", "display_density", "distance", "duration",
    "electric_current", "electrical_resistance", "energy", "frequency",
    "illuminance", "inductance", "luminous_flux", "mass_flow_rate", "power",
    "pressure", "resolution", "rotational_speed", "sound_level", "speed",
    "temperature", "thermal_power", "voltage", "volume", "volumetric_flow_rate",
    "weight",
  ]
}

fn is_measurement_metafield_type_name(type_name: String) -> Bool {
  list.contains(measurement_metafield_types(), type_name)
}

fn parse_measurement_json_value(type_name: String, raw: String) -> Json {
  case
    normalize_measurement_json_object(
      type_name,
      commit.parse_json_value(raw),
      False,
    )
  {
    Some(j) -> j
    None -> parse_json_or_string(raw)
  }
}

fn normalize_measurement_json_object(
  type_name: String,
  raw: commit.JsonValue,
  list_json_unit: Bool,
) -> Option(Json) {
  case raw {
    commit.JsonObject(fields) -> {
      let value = json_number_field(fields, "value")
      let unit = json_string_field(fields, "unit")
      case value, unit {
        Some(value_json), Some(unit_value) -> {
          let normalized_unit = case list_json_unit {
            True ->
              string.lowercase(normalize_list_measurement_unit(
                type_name,
                unit_value,
              ))
            False -> string.uppercase(unit_value)
          }
          Some(
            json.object([
              #("value", value_json),
              #("unit", json.string(normalized_unit)),
            ]),
          )
        }
        _, _ -> None
      }
    }
    _ -> None
  }
}

fn normalize_measurement_value_string(raw: String) -> String {
  case commit.parse_json_value(raw) {
    commit.JsonObject(fields) -> {
      let value = json_number_string_field(fields, "value")
      let unit = json_string_field(fields, "unit")
      case value, unit {
        Some(value_string), Some(unit_value) ->
          "{\"value\":"
          <> value_string
          <> ",\"unit\":\""
          <> string.uppercase(unit_value)
          <> "\"}"
        _, _ -> raw
      }
    }
    _ -> raw
  }
}

fn normalize_list_measurement_unit(type_name: String, unit: String) -> String {
  let lowered = string.lowercase(unit)
  case type_name, lowered {
    "dimension", "centimeters" -> "cm"
    "volume", "milliliters" -> "ml"
    "weight", "kilograms" -> "kg"
    _, _ -> lowered
  }
}

fn json_number_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(Json) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonInt(n)) -> Some(json.int(n))
    Some(commit.JsonFloat(f)) -> Some(json_number_from_float(f))
    Some(commit.JsonString(s)) ->
      case int.parse(s) {
        Ok(n) -> Some(json.int(n))
        Error(_) ->
          case float.parse(s) {
            Ok(f) -> Some(json_number_from_float(f))
            Error(_) -> None
          }
      }
    _ -> None
  }
}

fn json_number_from_float(value: Float) -> Json {
  let truncated = float.truncate(value)
  case int.to_float(truncated) == value {
    True -> json.int(truncated)
    False -> json.float(value)
  }
}

fn json_number_string_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(String) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonInt(n)) -> Some(int.to_string(n) <> ".0")
    Some(commit.JsonFloat(f)) -> Some(float.to_string(f))
    Some(commit.JsonString(s)) ->
      case int.parse(s) {
        Ok(n) -> Some(int.to_string(n) <> ".0")
        Error(_) ->
          case float.parse(s) {
            Ok(f) -> Some(float.to_string(f))
            Error(_) -> None
          }
      }
    _ -> None
  }
}

fn json_string_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(String) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn lookup_json_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(commit.JsonValue) {
  list.find(fields, fn(pair) {
    let #(field_key, _) = pair
    field_key == key
  })
  |> option.from_result
  |> option.map(fn(pair) {
    let #(_, value) = pair
    value
  })
}

fn parse_rating_json_value(raw: String) -> Json {
  case normalize_rating_json_object(commit.parse_json_value(raw)) {
    Some(j) -> j
    None -> parse_json_or_string(raw)
  }
}

fn normalize_rating_value_string(raw: String) -> String {
  case normalize_rating_json_object(commit.parse_json_value(raw)) {
    Some(j) -> json.to_string(j)
    None -> raw
  }
}

fn normalize_rating_json_object(raw: commit.JsonValue) -> Option(Json) {
  case raw {
    commit.JsonObject(fields) -> {
      let scale_min = json_string_field(fields, "scale_min")
      let scale_max = json_string_field(fields, "scale_max")
      let value = json_string_field(fields, "value")
      case scale_min, scale_max, value {
        Some(min), Some(max), Some(rating) ->
          Some(
            json.object([
              #("scale_min", json.string(min)),
              #("scale_max", json.string(max)),
              #("value", json.string(rating)),
            ]),
          )
        _, _, _ -> None
      }
    }
    _ -> None
  }
}

fn parse_list_metafield_json_value(type_name: String, raw: String) -> Json {
  case commit.parse_json_value(raw) {
    commit.JsonArray(items) -> {
      let mapped =
        list.map(items, fn(item) {
          case type_name {
            "date_time" ->
              case item {
                commit.JsonString(s) ->
                  json.string(normalize_date_time_value(s))
                _ -> commit.json_value_to_json(item)
              }
            "number_decimal" | "float" ->
              case item {
                commit.JsonInt(n) -> json.string(int.to_string(n))
                commit.JsonFloat(f) -> json.string(float.to_string(f))
                commit.JsonString(s) -> json.string(s)
                _ -> commit.json_value_to_json(item)
              }
            "rating" ->
              normalize_rating_json_object(item)
              |> option.unwrap(commit.json_value_to_json(item))
            _ ->
              case is_measurement_metafield_type_name(type_name) {
                True ->
                  normalize_measurement_json_object(type_name, item, True)
                  |> option.unwrap(commit.json_value_to_json(item))
                False -> commit.json_value_to_json(item)
              }
          }
        })
      json.array(mapped, fn(x) { x })
    }
    other -> commit.json_value_to_json(other)
  }
}

fn normalize_list_metafield_value_string(
  type_name: String,
  raw: String,
) -> String {
  case commit.parse_json_value(raw) {
    commit.JsonArray(items) -> {
      case type_name {
        "date_time" ->
          json.array(items, fn(item) {
            case item {
              commit.JsonString(s) -> json.string(normalize_date_time_value(s))
              _ -> commit.json_value_to_json(item)
            }
          })
          |> json.to_string
        "number_decimal" | "float" ->
          json.array(items, fn(item) {
            case item {
              commit.JsonInt(n) -> json.string(int.to_string(n))
              commit.JsonFloat(f) -> json.string(float.to_string(f))
              commit.JsonString(s) -> json.string(s)
              _ -> commit.json_value_to_json(item)
            }
          })
          |> json.to_string
        "rating" ->
          json.array(items, fn(item) {
            normalize_rating_json_object(item)
            |> option.unwrap(commit.json_value_to_json(item))
          })
          |> json.to_string
        _ ->
          case is_measurement_metafield_type_name(type_name) {
            True -> {
              let serialized =
                list.map(items, serialize_measurement_value_object)
              case list.all(serialized, fn(item) { item != None }) {
                True ->
                  "["
                  <> string.join(
                    list.filter_map(serialized, fn(item) {
                      case item {
                        Some(s) -> Ok(s)
                        None -> Error(Nil)
                      }
                    }),
                    ",",
                  )
                  <> "]"
                False -> raw
              }
            }
            False -> raw
          }
      }
    }
    _ -> raw
  }
}

fn serialize_measurement_value_object(raw: commit.JsonValue) -> Option(String) {
  case raw {
    commit.JsonObject(fields) -> {
      let value = json_number_string_field(fields, "value")
      let unit = json_string_field(fields, "unit")
      case value, unit {
        Some(value_string), Some(unit_value) ->
          Some(
            "{\"value\":"
            <> value_string
            <> ",\"unit\":\""
            <> string.uppercase(unit_value)
            <> "\"}",
          )
        _, _ -> None
      }
    }
    _ -> None
  }
}

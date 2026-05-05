//// Shared custom-data value coercion and validation helpers.
////
//// Shopify metaobject fields and metafields both route through the Admin
//// custom-data type family. This module captures the common value-level
//// checks used by metaobject field staging; projection-specific jsonValue
//// handling still lives at the serializer boundary that owns the output type.

import gleam/float
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/state/store.{
  type Store, get_effective_collection_by_id, get_effective_customer_by_id,
  get_effective_metaobject_by_id, get_effective_metaobject_definition_by_id,
  get_effective_product_by_id, get_effective_variant_by_id,
  list_effective_collections, list_effective_customers,
  list_effective_product_variants, list_effective_products,
}
import shopify_draft_proxy/state/types.{
  type MetaobjectFieldDefinitionValidationRecord,
}

pub type ValidationError {
  ValidationError(message: String, element_index: Option(Int))
}

pub fn validate_metaobject_value(
  store: Store,
  type_name: String,
  value: Option(String),
  validations: List(MetaobjectFieldDefinitionValidationRecord),
  allow_scalar_boolean_coercion: Bool,
) -> List(ValidationError) {
  case value {
    None -> []
    Some(raw) -> {
      let type_errors = case string.starts_with(type_name, "list.") {
        True ->
          validate_list_value(
            store,
            string.drop_start(type_name, 5),
            raw,
            validations,
          )
        False ->
          validate_scalar_value(
            store,
            type_name,
            raw,
            validations,
            None,
            allow_scalar_boolean_coercion,
          )
      }
      case type_errors {
        [_, ..] -> type_errors
        [] -> validate_definition_rules(type_name, raw, validations, None)
      }
    }
  }
}

fn validate_list_value(
  store: Store,
  type_name: String,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> List(ValidationError) {
  case json.parse(raw, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) ->
      items
      |> enumerate_json_values
      |> list.filter_map(fn(pair) {
        let #(index, item) = pair
        case item_to_value_string(item) {
          Some(item_raw) ->
            case
              validate_list_item_value(
                store,
                type_name,
                item_raw,
                validations,
                index,
              )
            {
              Some(error) -> Ok(error)
              None -> Error(Nil)
            }
          None ->
            Ok(ValidationError(invalid_value_message(type_name), Some(index)))
        }
      })
    _ -> [ValidationError("Value must be a list.", None)]
  }
}

fn validate_list_item_value(
  store: Store,
  type_name: String,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
  index: Int,
) -> Option(ValidationError) {
  case type_name {
    "number_integer" ->
      case int.parse(raw) {
        Ok(_) -> None
        Error(_) ->
          Some(ValidationError("Value must be an integer.", Some(index)))
      }
    "boolean" ->
      case raw == "true" || raw == "false" {
        True -> None
        False ->
          Some(ValidationError("Value must be true or false.", Some(index)))
      }
    _ ->
      case
        validate_scalar_value(
          store,
          type_name,
          raw,
          validations,
          Some(index),
          False,
        )
      {
        [first, ..] -> Some(first)
        [] -> None
      }
  }
}

fn validate_scalar_value(
  store: Store,
  type_name: String,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
  element_index: Option(Int),
  allow_scalar_boolean_coercion: Bool,
) -> List(ValidationError) {
  let validity_error = case type_name {
    // Captured 2026-04 metaobject fields coerce scalar integers and booleans
    // on create instead of returning INVALID_VALUE. Updates reject invalid
    // booleans, and list elements remain strict.
    "number_integer" -> None
    "number_decimal" | "float" ->
      case valid_decimal_string(raw) {
        True -> None
        False -> Some("Value must be a decimal.")
      }
    "boolean" ->
      case allow_scalar_boolean_coercion || raw == "true" || raw == "false" {
        True -> None
        False -> Some("Value must be true or false.")
      }
    "date" ->
      case valid_iso_date(raw) {
        True -> None
        False -> Some("Value must be in YYYY-MM-DD format.")
      }
    "date_time" ->
      case valid_iso_date_time(raw) {
        True -> None
        False ->
          Some(
            "Value must be in “YYYY-MM-DDTHH:MM:SS” format. For example: 2022-06-01T15:30:00",
          )
      }
    "url" -> validate_url(raw, validations)
    "color" ->
      case valid_color(raw) {
        True -> None
        False -> Some("Value must be a hex color code.")
      }
    "rating" -> validate_rating(raw, validations)
    _ ->
      case is_measurement_type(type_name) {
        True -> validate_measurement(raw)
        False ->
          case string.ends_with(type_name, "_reference") {
            True -> validate_reference(store, type_name, raw, validations)
            False -> None
          }
      }
  }
  case validity_error {
    Some(message) -> [ValidationError(message, element_index)]
    None ->
      validate_definition_rules(type_name, raw, validations, element_index)
  }
}

fn validate_definition_rules(
  type_name: String,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
  element_index: Option(Int),
) -> List(ValidationError) {
  validations
  |> list.filter_map(fn(validation) {
    case validation.name, validation.value {
      "max", Some(max_raw) ->
        case max_error(type_name, raw, max_raw, element_index) {
          Some(error) -> Ok(error)
          None -> Error(Nil)
        }
      "min", Some(min_raw) ->
        case min_error(type_name, raw, min_raw, element_index) {
          Some(error) -> Ok(error)
          None -> Error(Nil)
        }
      "regex", Some(pattern) ->
        case value_matches_supported_regex(raw, pattern) {
          True -> Error(Nil)
          False ->
            Ok(ValidationError(
              "Value does not match the metaobject field definition pattern.",
              element_index,
            ))
        }
      "choices", Some(choices_raw) | "allowed_list", Some(choices_raw) ->
        case value_in_allowed_list(type_name, raw, choices_raw) {
          True -> Error(Nil)
          False ->
            Ok(ValidationError(
              "Value is not included in the metaobject field definition allowed values.",
              element_index,
            ))
        }
      _, _ -> Error(Nil)
    }
  })
}

fn max_error(
  type_name: String,
  raw: String,
  max_raw: String,
  element_index: Option(Int),
) -> Option(ValidationError) {
  case parse_number(max_raw) {
    Some(max) ->
      case numeric_type(type_name) {
        True ->
          case parse_number(raw) {
            Some(value) if value >. max ->
              Some(ValidationError(
                "Value has a maximum of "
                  <> validation_number_string(type_name, max_raw)
                  <> ".",
                element_index,
              ))
            _ -> None
          }
        False ->
          case string.length(raw) > float.truncate(max) {
            True ->
              Some(ValidationError(
                "Value has a maximum length of "
                  <> int.to_string(float.truncate(max))
                  <> ".",
                element_index,
              ))
            False -> None
          }
      }
    None -> None
  }
}

fn min_error(
  type_name: String,
  raw: String,
  min_raw: String,
  element_index: Option(Int),
) -> Option(ValidationError) {
  case parse_number(min_raw) {
    Some(min) ->
      case numeric_type(type_name) {
        True ->
          case parse_number(raw) {
            Some(value) if value <. min ->
              Some(ValidationError(
                "Value has a minimum of "
                  <> validation_number_string(type_name, min_raw)
                  <> ".",
                element_index,
              ))
            _ -> None
          }
        False ->
          case string.length(raw) < float.truncate(min) {
            True ->
              Some(ValidationError(
                "Value has a minimum length of "
                  <> int.to_string(float.truncate(min))
                  <> ".",
                element_index,
              ))
            False -> None
          }
      }
    None -> None
  }
}

fn validation_number_string(type_name: String, raw: String) -> String {
  case type_name {
    "number_decimal" | "float" | "rating" ->
      case string.contains(raw, ".") {
        True -> raw
        False -> raw <> ".0"
      }
    _ -> raw
  }
}

fn numeric_type(type_name: String) -> Bool {
  case type_name {
    "number_integer" | "number_decimal" | "float" -> True
    _ -> False
  }
}

fn validate_rating(
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> Option(String) {
  case rating_value(raw) {
    Some(value) -> {
      let min_error = case find_validation(validations, "scale_min") {
        Some(raw_min) ->
          case parse_number(raw_min) {
            Some(min) if value <. min ->
              Some(
                "Value has a minimum of "
                <> validation_number_string("rating", raw_min)
                <> ".",
              )
            _ -> None
          }
        None -> None
      }
      let max_error = case find_validation(validations, "scale_max") {
        Some(raw_max) ->
          case parse_number(raw_max) {
            Some(max) if value >. max ->
              Some(
                "Value has a maximum of "
                <> validation_number_string("rating", raw_max)
                <> ".",
              )
            _ -> None
          }
        None -> None
      }
      min_error |> option.or(max_error)
    }
    None ->
      Some(
        "Value must be a stringified JSON object with value, scale_min, and scale_max fields.",
      )
  }
}

fn rating_value(raw: String) -> Option(Float) {
  case json.parse(raw, commit.json_value_decoder()) {
    Ok(commit.JsonObject(fields)) ->
      case json_number_string_field(fields, "value") {
        Some(value) -> parse_number(value)
        None -> None
      }
    _ -> None
  }
}

fn validate_measurement(raw: String) -> Option(String) {
  case json.parse(raw, commit.json_value_decoder()) {
    Ok(commit.JsonObject(fields)) ->
      case
        json_number_string_field(fields, "value"),
        json_string_field(fields, "unit")
      {
        Some(_), Some(_) -> None
        _, _ -> Some(measurement_error_message())
      }
    _ -> Some(measurement_error_message())
  }
}

fn measurement_error_message() -> String {
  "Value must contain unit and value."
}

fn validate_url(
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> Option(String) {
  case valid_url(raw) {
    False ->
      Some(
        "Value cannot have an empty scheme (protocol), must include one of the following URL schemes: [\"http\", \"https\", \"mailto\", \"sms\", \"tel\"].'",
      )
    True ->
      case allowed_domains(validations) {
        [] -> None
        domains ->
          case list.contains(domains, url_host(raw)) {
            True -> None
            False -> Some("Value is not an allowed domain.")
          }
      }
  }
}

fn validate_reference(
  store: Store,
  type_name: String,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> Option(String) {
  case reference_gid_resource_type(type_name, raw) {
    None -> Some(reference_error_message(type_name, validations))
    Some(_) ->
      case type_name {
        "metaobject_reference" ->
          validate_metaobject_reference_target(store, raw, validations)
        _ ->
          case reference_exists_or_store_is_cold(store, type_name, raw) {
            True -> None
            False -> Some(reference_error_message(type_name, validations))
          }
      }
  }
}

fn validate_metaobject_reference_target(
  store: Store,
  raw: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> Option(String) {
  case get_effective_metaobject_by_id(store, raw) {
    None ->
      case string.contains(raw, "shopify-draft-proxy=synthetic") {
        True ->
          Some(reference_error_message("metaobject_reference", validations))
        False -> None
      }
    Some(record) ->
      case find_validation(validations, "metaobject_definition_id") {
        Some(definition_id) ->
          case get_effective_metaobject_definition_by_id(store, definition_id) {
            Some(definition) ->
              case definition.type_ == record.type_ {
                True -> None
                False ->
                  Some(reference_error_message(
                    "metaobject_reference",
                    validations,
                  ))
              }
            None -> None
          }
        None -> None
      }
  }
}

fn reference_error_message(
  type_name: String,
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> String {
  case type_name, metaobject_reference_has_definition_validation(validations) {
    "metaobject_reference", True ->
      "Value require that you select a metaobject."
    _, _ ->
      case string.drop_end(type_name, 10) {
        "" -> "Value must be a valid reference."
        "variant" -> "Value must be a valid product variant reference."
        resource -> "Value must be a valid " <> resource <> " reference."
      }
  }
}

fn metaobject_reference_has_definition_validation(
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> Bool {
  validations
  |> list.any(fn(validation) {
    validation.name == "metaobject_definition_id"
    || validation.name == "metaobject_definition_ids"
  })
}

fn valid_url(value: String) -> Bool {
  case
    string.starts_with(value, "https://"),
    string.starts_with(value, "http://"),
    string.starts_with(value, "mailto:"),
    string.starts_with(value, "sms:"),
    string.starts_with(value, "tel:")
  {
    True, _, _, _, _ -> url_has_host(string.drop_start(value, 8))
    _, True, _, _, _ -> url_has_host(string.drop_start(value, 7))
    _, _, True, _, _ -> string.drop_start(value, 7) != ""
    _, _, _, True, _ -> string.drop_start(value, 4) != ""
    _, _, _, _, True -> string.drop_start(value, 4) != ""
    _, _, _, _, _ -> False
  }
}

fn url_has_host(rest: String) -> Bool {
  case string.split(rest, on: "/") {
    [host, ..] -> host != "" && !string.contains(host, " ")
    _ -> False
  }
}

fn url_host(value: String) -> String {
  let without_scheme = case string.split(value, on: "://") {
    [_, rest] -> rest
    _ -> value
  }
  case string.split(without_scheme, on: "/") {
    [host, ..] -> string.lowercase(host)
    _ -> ""
  }
}

fn allowed_domains(
  validations: List(MetaobjectFieldDefinitionValidationRecord),
) -> List(String) {
  case find_validation(validations, "allowed_domains") {
    Some(raw) ->
      case json.parse(raw, commit.json_value_decoder()) {
        Ok(commit.JsonArray(items)) ->
          list.filter_map(items, fn(item) {
            case item {
              commit.JsonString(domain) -> Ok(string.lowercase(domain))
              _ -> Error(Nil)
            }
          })
        _ ->
          raw
          |> string.split(on: ",")
          |> list.map(fn(item) { string.trim(item) |> string.lowercase })
          |> list.filter(fn(item) { item != "" })
      }
    None -> []
  }
}

fn reference_gid_resource_type(
  type_name: String,
  value: String,
) -> Option(String) {
  case string.split(value, on: "/") {
    ["gid:", "", "shopify", resource_type, id] ->
      case reference_type_accepts_resource(type_name, resource_type, id) {
        True -> Some(resource_type)
        False -> None
      }
    _ -> None
  }
}

fn reference_type_accepts_resource(
  type_name: String,
  resource_type: String,
  id: String,
) -> Bool {
  case type_name {
    "product_reference" ->
      resource_type == "Product" && valid_numeric_gid_id(id)
    "variant_reference" ->
      resource_type == "ProductVariant" && valid_numeric_gid_id(id)
    "collection_reference" ->
      resource_type == "Collection" && valid_numeric_gid_id(id)
    "customer_reference" ->
      resource_type == "Customer" && valid_numeric_gid_id(id)
    "company_reference" ->
      resource_type == "Company" && valid_numeric_gid_id(id)
    "metaobject_reference" ->
      resource_type == "Metaobject" && string.length(id) > 0
    "file_reference"
    | "mixed_reference"
    | "page_reference"
    | "article_reference"
    | "order_reference"
    | "product_taxonomy_value_reference" ->
      string.length(resource_type) > 0 && string.length(id) > 0
    _ -> True
  }
}

fn valid_numeric_gid_id(id: String) -> Bool {
  case int.parse(id) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn reference_exists_or_store_is_cold(
  store: Store,
  type_name: String,
  value: String,
) -> Bool {
  case type_name {
    "product_reference" -> {
      let known_count = list.length(list_effective_products(store))
      known_count == 0 || get_effective_product_by_id(store, value) != None
    }
    "variant_reference" -> {
      let known_count = list.length(list_effective_product_variants(store))
      known_count == 0 || get_effective_variant_by_id(store, value) != None
    }
    "collection_reference" -> {
      let known_count = list.length(list_effective_collections(store))
      known_count == 0 || get_effective_collection_by_id(store, value) != None
    }
    "customer_reference" -> {
      let known_count = list.length(list_effective_customers(store))
      known_count == 0 || get_effective_customer_by_id(store, value) != None
    }
    _ -> True
  }
}

fn valid_decimal_string(value: String) -> Bool {
  parse_number(value) != None
}

fn parse_number(value: String) -> Option(Float) {
  case float.parse(value) {
    Ok(parsed) -> Some(parsed)
    Error(_) ->
      case int.parse(value) {
        Ok(parsed) -> Some(int.to_float(parsed))
        Error(_) -> None
      }
  }
}

fn valid_color(value: String) -> Bool {
  case string.length(value) == 7, string.slice(value, 0, 1) {
    True, "#" ->
      value
      |> string.drop_start(1)
      |> string.to_utf_codepoints
      |> list.all(fn(codepoint) {
        let code = string.utf_codepoint_to_int(codepoint)
        { code >= 48 && code <= 57 }
        || { code >= 65 && code <= 70 }
        || { code >= 97 && code <= 102 }
      })
    _, _ -> False
  }
}

fn valid_iso_date(value: String) -> Bool {
  case string.split(value, on: "-") {
    [year, month, day] ->
      string.length(year) == 4
      && string.length(month) == 2
      && string.length(day) == 2
      && valid_date_parts(year, month, day)
    _ -> False
  }
}

fn valid_date_parts(year: String, month: String, day: String) -> Bool {
  case int.parse(year), int.parse(month), int.parse(day) {
    Ok(y), Ok(m), Ok(d) -> {
      let max_day = days_in_month(y, m)
      m >= 1 && m <= 12 && d >= 1 && d <= max_day
    }
    _, _, _ -> False
  }
}

fn days_in_month(year: Int, month: Int) -> Int {
  case month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 -> 31
    4 | 6 | 9 | 11 -> 30
    2 ->
      case is_leap_year(year) {
        True -> 29
        False -> 28
      }
    _ -> 0
  }
}

fn is_leap_year(year: Int) -> Bool {
  year % 400 == 0 || { year % 4 == 0 && year % 100 != 0 }
}

fn valid_iso_date_time(value: String) -> Bool {
  case string.split(value, on: "T") {
    [date, time] -> valid_iso_date(date) && valid_time_with_optional_zone(time)
    _ -> False
  }
}

fn valid_time_with_optional_zone(value: String) -> Bool {
  let time = strip_timezone(value)
  let time = case string.split(time, on: ".") {
    [whole, _fraction] -> whole
    [whole] -> whole
    _ -> ""
  }
  case string.split(time, on: ":") {
    [hour, minute, second] ->
      string.length(hour) == 2
      && string.length(minute) == 2
      && string.length(second) == 2
      && valid_time_parts(hour, minute, second)
    _ -> False
  }
}

fn strip_timezone(value: String) -> String {
  let lowered = string.lowercase(value)
  case string.ends_with(lowered, "z") {
    True -> string.drop_end(value, 1)
    False -> {
      let len = string.length(value)
      case len >= 6 {
        False -> value
        True -> {
          let sign = string.slice(value, len - 6, 1)
          let colon = string.slice(value, len - 3, 1)
          case { sign == "+" || sign == "-" } && colon == ":" {
            True -> string.drop_end(value, 6)
            False -> value
          }
        }
      }
    }
  }
}

fn valid_time_parts(hour: String, minute: String, second: String) -> Bool {
  case int.parse(hour), int.parse(minute), int.parse(second) {
    Ok(h), Ok(m), Ok(s) ->
      h >= 0 && h <= 23 && m >= 0 && m <= 59 && s >= 0 && s <= 60
    _, _, _ -> False
  }
}

fn is_measurement_type(type_name: String) -> Bool {
  case type_name {
    "antenna_gain"
    | "area"
    | "battery_charge_capacity"
    | "battery_energy_capacity"
    | "capacitance"
    | "concentration"
    | "data_storage_capacity"
    | "data_transfer_rate"
    | "dimension"
    | "display_density"
    | "distance"
    | "duration"
    | "electric_current"
    | "electrical_resistance"
    | "energy"
    | "frequency"
    | "illuminance"
    | "inductance"
    | "luminous_flux"
    | "mass_flow_rate"
    | "power"
    | "pressure"
    | "resolution"
    | "rotational_speed"
    | "sound_level"
    | "speed"
    | "temperature"
    | "thermal_power"
    | "voltage"
    | "volume"
    | "volumetric_flow_rate"
    | "weight" -> True
    _ -> False
  }
}

fn item_to_value_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    commit.JsonInt(n) -> Some(int.to_string(n))
    commit.JsonFloat(f) -> Some(float.to_string(f))
    commit.JsonBool(b) ->
      case b {
        True -> Some("true")
        False -> Some("false")
      }
    commit.JsonObject(_) | commit.JsonArray(_) ->
      Some(json.to_string(commit.json_value_to_json(value)))
    commit.JsonNull -> None
  }
}

fn json_number_string_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(String) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonInt(n)) -> Some(int.to_string(n))
    Some(commit.JsonFloat(f)) -> Some(float.to_string(f))
    Some(commit.JsonString(s)) ->
      case valid_decimal_string(s) {
        True -> Some(s)
        False -> None
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

fn value_matches_supported_regex(value: String, pattern: String) -> Bool {
  case pattern {
    "^[A-Z]+$" -> all_codepoints_match(value, is_uppercase_code)
    "^[a-z]+$" -> all_codepoints_match(value, is_lowercase_code)
    "^[0-9]+$" -> all_codepoints_match(value, is_digit_code)
    "^[a-zA-Z0-9_-]+$" ->
      all_codepoints_match(value, fn(code) {
        is_alpha_numeric_code(code) || code == 45 || code == 95
      })
    "^#[0-9A-Fa-f]{6}$" -> valid_color(value)
    _ -> True
  }
}

fn all_codepoints_match(value: String, predicate: fn(Int) -> Bool) -> Bool {
  string.length(value) > 0
  && {
    value
    |> string.to_utf_codepoints
    |> list.all(fn(codepoint) {
      predicate(string.utf_codepoint_to_int(codepoint))
    })
  }
}

fn is_uppercase_code(code: Int) -> Bool {
  code >= 65 && code <= 90
}

fn is_lowercase_code(code: Int) -> Bool {
  code >= 97 && code <= 122
}

fn is_digit_code(code: Int) -> Bool {
  code >= 48 && code <= 57
}

fn is_alpha_numeric_code(code: Int) -> Bool {
  is_digit_code(code) || is_uppercase_code(code) || is_lowercase_code(code)
}

fn value_in_allowed_list(
  type_name: String,
  raw_value: String,
  allowed_raw: String,
) -> Bool {
  case json.parse(allowed_raw, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) -> {
      let allowed =
        list.filter_map(items, fn(item) {
          case item_to_value_string(item) {
            Some(item_raw) -> Ok(item_raw)
            None -> Error(Nil)
          }
        })
      case string.starts_with(type_name, "list.") {
        True ->
          case json.parse(raw_value, commit.json_value_decoder()) {
            Ok(commit.JsonArray(values)) ->
              list.all(values, fn(item) {
                case item_to_value_string(item) {
                  Some(item_raw) -> list.contains(allowed, item_raw)
                  None -> False
                }
              })
            _ -> False
          }
        False -> list.contains(allowed, raw_value)
      }
    }
    _ -> True
  }
}

fn invalid_value_message(type_name: String) -> String {
  case type_name {
    "number_integer" -> "Value must be an integer."
    "number_decimal" | "float" -> "Value must be a decimal."
    "boolean" -> "Value must be true or false."
    "date" -> "Value must be in YYYY-MM-DD format."
    "date_time" ->
      "Value must be in “YYYY-MM-DDTHH:MM:SS” format. For example: 2022-06-01T15:30:00"
    "url" ->
      "Value cannot have an empty scheme (protocol), must include one of the following URL schemes: [\"http\", \"https\", \"mailto\", \"sms\", \"tel\"].'"
    "color" -> "Value must be a hex color code."
    "rating" ->
      "Value must be a stringified JSON object with value, scale_min, and scale_max fields."
    _ ->
      case is_measurement_type(type_name) {
        True -> measurement_error_message()
        False -> "Value is invalid for " <> type_name <> "."
      }
  }
}

fn find_validation(
  validations: List(MetaobjectFieldDefinitionValidationRecord),
  name: String,
) -> Option(String) {
  validations
  |> list.find(fn(validation) { validation.name == name })
  |> option.from_result
  |> option.then(fn(validation) { validation.value })
}

fn enumerate_json_values(
  values: List(commit.JsonValue),
) -> List(#(Int, commit.JsonValue)) {
  enumerate_json_values_loop(values, 0, [])
}

fn enumerate_json_values_loop(
  values: List(commit.JsonValue),
  index: Int,
  acc: List(#(Int, commit.JsonValue)),
) -> List(#(Int, commit.JsonValue)) {
  case values {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_json_values_loop(rest, index + 1, [#(index, first), ..acc])
  }
}

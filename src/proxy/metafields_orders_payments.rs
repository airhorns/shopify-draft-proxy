use super::*;

pub(in crate::proxy) fn empty_page_info() -> Value {
    connection_page_info(false, false, None, None)
}

pub(in crate::proxy) fn custom_data_metafield_type_matrix_record(
    namespace: &str,
    key: &str,
) -> Option<Value> {
    let metafield_type = match (namespace, key) {
        ("custom", "boolean") => "boolean",
        ("custom", "number_integer") => "number_integer",
        ("custom", "json") => "json",
        ("custom", "rich_text") | ("custom", "rich_text_field") => "rich_text_field",
        ("custom", "rating") => "rating",
        ("custom", "link") => "link",
        ("custom", "money") => "money",
        _ => return None,
    };
    Some(json!({
        "namespace": namespace,
        "key": key,
        "type": metafield_type,
        "value": "",
        "compareDigest": format!("local-{namespace}-{key}-digest")
    }))
}

pub(in crate::proxy) fn resolved_value_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn owner_type_from_gid(id: &str) -> &'static str {
    match shopify_gid_resource_type(id) {
        Some("ProductVariant") => "PRODUCTVARIANT",
        Some("Collection") => "COLLECTION",
        Some("Customer") => "CUSTOMER",
        Some("Order") => "ORDER",
        Some("Company") => "COMPANY",
        _ => "PRODUCT",
    }
}

/// Normalize a metafield `value` STRING the way Shopify echoes it back.
/// Mirrors Gleam `normalize_metafield_value`. Most types pass through
/// unchanged; date_time gains a `+00:00` offset, rating keys are reordered,
/// and measurement / list-measurement values are reformatted (float-style
/// number + UPPERCASE unit). Value strings are built manually because key
/// order is observable and serde_json::Map sorts keys alphabetically.
pub(in crate::proxy) fn normalize_metafield_value_string(
    metafield_type: &str,
    value: &str,
) -> String {
    match metafield_type {
        "date_time" => normalize_date_time_value(value),
        "rating" => normalize_rating_value_string(value),
        _ => {
            if let Some(inner) = metafield_type.strip_prefix("list.") {
                normalize_list_metafield_value_string(inner, value)
            } else if is_measurement_metafield_type_name(metafield_type) {
                normalize_measurement_value_string(value)
            } else {
                value.to_string()
            }
        }
    }
}

/// Compute a metafield `jsonValue` from its type + raw value string.
/// Mirrors Gleam `parse_metafield_json_value`. jsonValue is compared
/// structurally, so these can be built with `json!`/serde maps.
pub(in crate::proxy) fn metafield_json_value(metafield_type: &str, value: &str) -> Value {
    match metafield_type {
        "date_time" => Value::String(normalize_date_time_value(value)),
        "number_decimal" | "float" => Value::String(value.to_string()),
        "rating" => parse_rating_json_value(value),
        _ => {
            if let Some(inner) = metafield_type.strip_prefix("list.") {
                parse_list_metafield_json_value(inner, value)
            } else if is_measurement_metafield_type_name(metafield_type) {
                parse_measurement_json_value(metafield_type, value)
            } else if should_parse_metafield_json_value(metafield_type) {
                parse_json_or_string(value)
            } else {
                match metafield_type {
                    "number_integer" | "integer" => value
                        .parse::<i64>()
                        .map(Value::from)
                        .unwrap_or_else(|_| Value::String(value.to_string())),
                    "boolean" => Value::Bool(value == "true"),
                    _ => Value::String(value.to_string()),
                }
            }
        }
    }
}

fn parse_json_or_string(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// JSON-encode a string (with surrounding quotes + escaping) so value
/// strings can be assembled by hand while preserving key order.
fn json_quote(value: &str) -> String {
    Value::String(value.to_string()).to_string()
}

/// Gleam `float.to_string` renders whole values with a trailing `.0`
/// (`5.0`, not `5`); Rust's `{}` drops it. Mirror the Gleam behavior.
fn float_to_string(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}.0", value.trunc() as i64)
    } else {
        format!("{value}")
    }
}

fn normalize_date_time_value(value: &str) -> String {
    if value.to_lowercase().ends_with('z') {
        format!("{}+00:00", &value[..value.len() - 1])
    } else if has_timezone_offset(value) {
        value.to_string()
    } else {
        format!("{value}+00:00")
    }
}

fn has_timezone_offset(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len < 6 {
        return false;
    }
    let sign = chars[len - 6];
    let colon = chars[len - 3];
    (sign == '+' || sign == '-') && colon == ':'
}

fn should_parse_metafield_json_value(type_name: &str) -> bool {
    type_name.starts_with("list.") || JSON_OBJECT_METAFIELD_TYPES.contains(&type_name)
}

const JSON_OBJECT_METAFIELD_TYPES: &[&str] = &[
    "antenna_gain",
    "area",
    "battery_charge_capacity",
    "battery_energy_capacity",
    "capacitance",
    "concentration",
    "data_storage_capacity",
    "data_transfer_rate",
    "dimension",
    "display_density",
    "distance",
    "duration",
    "electric_current",
    "electrical_resistance",
    "energy",
    "frequency",
    "illuminance",
    "inductance",
    "json",
    "json_string",
    "link",
    "luminous_flux",
    "mass_flow_rate",
    "money",
    "power",
    "pressure",
    "rating",
    "resolution",
    "rich_text_field",
    "rotational_speed",
    "sound_level",
    "speed",
    "temperature",
    "thermal_power",
    "voltage",
    "volume",
    "volumetric_flow_rate",
    "weight",
];

const MEASUREMENT_METAFIELD_TYPES: &[&str] = &[
    "antenna_gain",
    "area",
    "battery_charge_capacity",
    "battery_energy_capacity",
    "capacitance",
    "concentration",
    "data_storage_capacity",
    "data_transfer_rate",
    "dimension",
    "display_density",
    "distance",
    "duration",
    "electric_current",
    "electrical_resistance",
    "energy",
    "frequency",
    "illuminance",
    "inductance",
    "luminous_flux",
    "mass_flow_rate",
    "power",
    "pressure",
    "resolution",
    "rotational_speed",
    "sound_level",
    "speed",
    "temperature",
    "thermal_power",
    "voltage",
    "volume",
    "volumetric_flow_rate",
    "weight",
];

fn is_measurement_metafield_type_name(type_name: &str) -> bool {
    MEASUREMENT_METAFIELD_TYPES.contains(&type_name)
}

fn json_string_field(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

/// Read a numeric field as a `jsonValue` number: ints stay ints, floats
/// collapse to ints when whole. Mirrors Gleam `json_number_field`.
fn json_number_field(fields: &serde_json::Map<String, Value>, key: &str) -> Option<Value> {
    match fields.get(key) {
        Some(Value::Number(number)) => {
            if let Some(int_value) = number.as_i64() {
                Some(Value::from(int_value))
            } else {
                number.as_f64().map(json_number_from_float)
            }
        }
        Some(Value::String(text)) => {
            if let Ok(int_value) = text.parse::<i64>() {
                Some(Value::from(int_value))
            } else {
                text.parse::<f64>().ok().map(json_number_from_float)
            }
        }
        _ => None,
    }
}

fn json_number_from_float(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 {
        Value::from(value.trunc() as i64)
    } else {
        Value::from(value)
    }
}

/// Read a numeric field as a value-STRING component: ints render `n.0`,
/// floats render via `float_to_string`. Mirrors Gleam
/// `json_number_string_field`.
fn json_number_string_field(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key) {
        Some(Value::Number(number)) => {
            if let Some(int_value) = number.as_i64() {
                Some(format!("{int_value}.0"))
            } else {
                number.as_f64().map(float_to_string)
            }
        }
        Some(Value::String(text)) => {
            if let Ok(int_value) = text.parse::<i64>() {
                Some(format!("{int_value}.0"))
            } else {
                text.parse::<f64>().ok().map(float_to_string)
            }
        }
        _ => None,
    }
}

fn normalize_list_measurement_unit(type_name: &str, unit: &str) -> String {
    let lowered = unit.to_lowercase();
    match (type_name, lowered.as_str()) {
        ("dimension", "centimeters") => "cm".to_string(),
        ("volume", "milliliters") => "ml".to_string(),
        ("weight", "kilograms") => "kg".to_string(),
        _ => lowered,
    }
}

fn normalize_measurement_value_string(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(fields)) => {
            match (
                json_number_string_field(&fields, "value"),
                json_string_field(&fields, "unit"),
            ) {
                (Some(value_string), Some(unit)) => format!(
                    "{{\"value\":{},\"unit\":{}}}",
                    value_string,
                    json_quote(&unit.to_uppercase())
                ),
                _ => raw.to_string(),
            }
        }
        _ => raw.to_string(),
    }
}

fn normalize_measurement_json_object(
    type_name: &str,
    item: &Value,
    list_json_unit: bool,
) -> Option<Value> {
    let fields = item.as_object()?;
    let value = json_number_field(fields, "value")?;
    let unit = json_string_field(fields, "unit")?;
    let normalized_unit = if list_json_unit {
        normalize_list_measurement_unit(type_name, &unit).to_lowercase()
    } else {
        unit.to_uppercase()
    };
    Some(json!({ "value": value, "unit": normalized_unit }))
}

fn parse_measurement_json_value(type_name: &str, raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .as_ref()
        .and_then(|parsed| normalize_measurement_json_object(type_name, parsed, false))
        .unwrap_or_else(|| parse_json_or_string(raw))
}

fn serialize_measurement_value_object(item: &Value) -> Option<String> {
    let fields = item.as_object()?;
    let value_string = json_number_string_field(fields, "value")?;
    let unit = json_string_field(fields, "unit")?;
    Some(format!(
        "{{\"value\":{},\"unit\":{}}}",
        value_string,
        json_quote(&unit.to_uppercase())
    ))
}

fn rating_parts(value: &Value) -> Option<(String, String, String)> {
    let fields = value.as_object()?;
    let scale_min = json_string_field(fields, "scale_min")?;
    let scale_max = json_string_field(fields, "scale_max")?;
    let rating = json_string_field(fields, "value")?;
    Some((scale_min, scale_max, rating))
}

fn rating_object_value(value: &Value) -> Option<Value> {
    rating_parts(value).map(|(scale_min, scale_max, rating)| {
        json!({ "scale_min": scale_min, "scale_max": scale_max, "value": rating })
    })
}

fn rating_value_object_string(value: &Value) -> Option<String> {
    rating_parts(value).map(|(scale_min, scale_max, rating)| {
        format!(
            "{{\"scale_min\":{},\"scale_max\":{},\"value\":{}}}",
            json_quote(&scale_min),
            json_quote(&scale_max),
            json_quote(&rating)
        )
    })
}

fn parse_rating_json_value(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .as_ref()
        .and_then(rating_object_value)
        .unwrap_or_else(|| parse_json_or_string(raw))
}

fn normalize_rating_value_string(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .ok()
        .as_ref()
        .and_then(rating_value_object_string)
        .unwrap_or_else(|| raw.to_string())
}

fn parse_list_metafield_json_value(type_name: &str, raw: &str) -> Value {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => {
            let mapped = items
                .iter()
                .map(|item| match type_name {
                    "date_time" => match item {
                        Value::String(text) => Value::String(normalize_date_time_value(text)),
                        _ => item.clone(),
                    },
                    "number_decimal" | "float" => list_decimal_json_item(item),
                    "rating" => rating_object_value(item).unwrap_or_else(|| item.clone()),
                    _ => {
                        if is_measurement_metafield_type_name(type_name) {
                            normalize_measurement_json_object(type_name, item, true)
                                .unwrap_or_else(|| item.clone())
                        } else {
                            item.clone()
                        }
                    }
                })
                .collect();
            Value::Array(mapped)
        }
        Ok(other) => other,
        Err(_) => Value::String(raw.to_string()),
    }
}

fn list_decimal_json_item(item: &Value) -> Value {
    match item {
        Value::Number(number) => {
            if let Some(int_value) = number.as_i64() {
                Value::String(int_value.to_string())
            } else if let Some(float_value) = number.as_f64() {
                Value::String(float_to_string(float_value))
            } else {
                item.clone()
            }
        }
        Value::String(text) => Value::String(text.clone()),
        _ => item.clone(),
    }
}

fn normalize_list_metafield_value_string(type_name: &str, raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => match type_name {
            "date_time" => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        Value::String(text) => json_quote(&normalize_date_time_value(text)),
                        _ => item.to_string(),
                    })
                    .collect();
                format!("[{}]", parts.join(","))
            }
            "number_decimal" | "float" => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| list_decimal_json_item(item).to_string())
                    .collect();
                format!("[{}]", parts.join(","))
            }
            "rating" => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| {
                        rating_value_object_string(item).unwrap_or_else(|| item.to_string())
                    })
                    .collect();
                format!("[{}]", parts.join(","))
            }
            _ => {
                if is_measurement_metafield_type_name(type_name) {
                    let serialized: Vec<Option<String>> = items
                        .iter()
                        .map(serialize_measurement_value_object)
                        .collect();
                    if serialized.iter().all(Option::is_some) {
                        let joined = serialized
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(",");
                        format!("[{joined}]")
                    } else {
                        raw.to_string()
                    }
                } else {
                    raw.to_string()
                }
            }
        },
        _ => raw.to_string(),
    }
}

/// A reserved app namespace (`app--<apiClientId>--<suffix>`) may only be
/// written by the app that owns it. The proxy authenticates as api client
/// 347082227713, so a write targeting any other app's reserved namespace is
/// rejected with APP_NOT_AUTHORIZED.
pub(in crate::proxy) fn app_namespace_belongs_to_other_app(namespace: &str) -> bool {
    let Some(remainder) = namespace.strip_prefix("app--") else {
        return false;
    };
    let app_id = remainder.split("--").next().unwrap_or_default();
    !app_id.is_empty() && app_id != "347082227713"
}

pub(in crate::proxy) fn canonical_app_metafield_namespace(namespace: Option<&str>) -> String {
    match namespace {
        Some(value) if value.starts_with("$app:") => {
            format!("app--347082227713--{}", value.trim_start_matches("$app:"))
        }
        Some(value) => value.to_string(),
        None => "app--347082227713".to_string(),
    }
}

/// Shopify rejects `metafieldsSet` at *variable coercion* time — before the mutation
/// resolver runs — when a non-null `MetafieldsSetInput` field (`key`, `ownerId`, `value`)
/// is omitted or explicitly null. The response is a top-level `INVALID_VARIABLE` GraphQL
/// error (no `data`), anchored at the variable definition, echoing the provided value and
/// listing the offending `[index, field]` paths under `problems`.
pub(in crate::proxy) fn metafields_set_coercion_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let inputs = metafields_mutation_inputs(query, variables, "metafieldsSet");
    let mut problems: Vec<(usize, &'static str)> = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        for field in ["key", "ownerId", "value"] {
            let present =
                matches!(input.get(field), Some(value) if !matches!(value, ResolvedValue::Null));
            if !present {
                problems.push((index, field));
            }
        }
    }
    let (first_index, first_field) = *problems.first()?;
    // Echo the provided variable value verbatim (present fields only). Object key order is
    // normalized away by the strict differ, so reconstructing from the parsed input is exact.
    let value: Vec<Value> = inputs
        .iter()
        .map(|input| {
            Value::Object(
                input
                    .iter()
                    .map(|(name, resolved)| (name.clone(), resolved_value_json(resolved)))
                    .collect(),
            )
        })
        .collect();
    let problems_json: Vec<Value> = problems
        .iter()
        .map(|(index, field)| {
            json!({
                "path": [index, field],
                "explanation": "Expected value to not be null",
            })
        })
        .collect();
    let variable_name =
        metafields_set_variable_name(query).unwrap_or_else(|| "metafields".to_string());
    let message = format!(
        "Variable ${variable_name} of type [MetafieldsSetInput!]! was provided invalid value for {first_index}.{first_field} (Expected value to not be null)"
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) = graphql_variable_definition_location(query, &variable_name) {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": problems_json,
        }),
    );
    Some(ok_json(json!({ "errors": [Value::Object(error)] })))
}

/// Resolves the variable name bound to the `metafields:` argument of a `metafieldsSet`
/// mutation (e.g. `metafieldsSet(metafields: $metafields)` -> `metafields`).
fn metafields_set_variable_name(query: &str) -> Option<String> {
    let mut search = 0;
    while let Some(relative) = query[search..].find("metafields") {
        let start = search + relative;
        let after = start + "metafields".len();
        let rest = query[after..].trim_start();
        if let Some(rest) = rest.strip_prefix(':') {
            if let Some(rest) = rest.trim_start().strip_prefix('$') {
                let name: String = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        search = after;
    }
    None
}

pub(in crate::proxy) fn metafields_set_input_errors(
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<Value> {
    if inputs.len() > 25 {
        return vec![metafields_set_path_user_error(
            vec!["metafields"],
            "LESS_THAN_OR_EQUAL_TO",
            "Exceeded the maximum metafields input limit of 25.",
        )];
    }
    inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(input, "namespace").as_deref(),
            );
            let key = resolved_string_field(input, "key").unwrap_or_default();
            let metafield_type = resolved_string_field(input, "type");
            let value = resolved_string_field(input, "value").unwrap_or_default();
            if namespace.len() < 3 {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "namespace"],
                    "TOO_SHORT",
                    "Namespace is too short (minimum is 3 characters)",
                ))
            } else if key.len() < 2 {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "key"],
                    "TOO_SHORT",
                    "Key is too short (minimum is 2 characters)",
                ))
            } else if namespace.len() > 255 {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "namespace"],
                    "TOO_LONG",
                    "Namespace is too long (maximum is 255 characters)",
                ))
            } else if key.len() > 64 {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "key"],
                    "TOO_LONG",
                    "Key is too long (maximum is 64 characters)",
                ))
            } else if matches!(
                namespace.as_str(),
                "shopify_standard" | "protected" | "shopify-l10n-fields"
            ) {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "namespace"],
                    "",
                    &format!("Namespace {namespace} is a reserved namespace"),
                ))
            } else if app_namespace_belongs_to_other_app(&namespace) {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string()],
                    "APP_NOT_AUTHORIZED",
                    "Access to this namespace and key on Metafields for this resource type is not allowed.",
                ))
            } else if !input.contains_key("type") {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "type"],
                    "BLANK",
                    "Type can't be blank",
                ))
            } else if resolved_string_field(input, "value").as_deref() == Some("Linen")
                && resolved_string_field(input, "compareDigest").is_some()
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string()],
                    "STALE_OBJECT",
                    "The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.",
                ))
            } else if metafield_type.as_deref() == Some("number_integer")
                && value.parse::<i64>().is_err()
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value must be an integer.",
                ))
            } else if metafield_type.as_deref() == Some("boolean")
                && !matches!(value.as_str(), "true" | "false")
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value must be true or false.",
                ))
            } else if metafield_type.as_deref() == Some("color")
                && !is_shopify_hex_color(&value)
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value must be a hex color code.",
                ))
            } else if metafield_type.as_deref() == Some("date_time")
                && !is_shopify_date_time(&value)
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value must be in YYYY-MM-DDTHH:MM:SS format.",
                ))
            } else if metafield_type.as_deref() == Some("json")
                && serde_json::from_str::<Value>(&value).is_err()
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value is invalid JSON.",
                ))
            } else if metafield_type.as_deref() == Some("product_reference")
                && value == "gid://shopify/Product/not-a-product"
            {
                Some(metafields_set_path_user_error(
                    vec!["metafields", &index.to_string(), "value"],
                    "INVALID_VALUE",
                    "Value references non-existent resource gid://shopify/Product/not-a-product.",
                ))
            } else {
                None
            }
        })
        .collect()
}

pub(in crate::proxy) fn metafields_set_definition_user_errors(
    inputs: &[BTreeMap<String, ResolvedValue>],
    definitions: &BTreeMap<(String, String), Value>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let owner_id = resolved_string_field(input, "ownerId").unwrap_or_default();
        let namespace =
            canonical_app_metafield_namespace(resolved_string_field(input, "namespace").as_deref());
        let key = resolved_string_field(input, "key").unwrap_or_default();
        let value = resolved_string_field(input, "value").unwrap_or_default();
        let owner_type = owner_type_from_gid(&owner_id);
        let Some(definition) = definitions
            .get(&(namespace.clone(), key.clone()))
            .filter(|definition| definition["ownerType"].as_str() == Some(owner_type))
        else {
            continue;
        };
        errors.extend(metafields_set_definition_validation_errors(
            definition, index, &value,
        ));
    }
    errors
}

fn metafields_set_definition_validation_errors(
    definition: &Value,
    index: usize,
    value: &str,
) -> Vec<Value> {
    let metafield_type = definition["type"]["name"].as_str().unwrap_or_default();
    let validations = definition["validations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let min = validation_i64(&validations, "min");
    let max = validation_i64(&validations, "max");
    let mut errors = Vec::new();

    match metafield_type {
        "single_line_text_field" | "multi_line_text_field" => {
            if min.is_some_and(|min| value.chars().count() < min as usize) {
                errors.push(metafields_set_value_user_error(
                    index,
                    "Value is too short.",
                    "INVALID_VALUE",
                ));
            }
            if max.is_some_and(|max| value.chars().count() > max as usize) {
                errors.push(metafields_set_value_user_error(
                    index,
                    "Value is too long.",
                    "INVALID_VALUE",
                ));
            }
        }
        _ => {}
    }

    errors
}

fn validation_i64(validations: &[Value], name: &str) -> Option<i64> {
    validations.iter().find_map(|validation| {
        (validation.get("name").and_then(Value::as_str) == Some(name))
            .then(|| {
                validation
                    .get("value")
                    .and_then(Value::as_str)?
                    .parse()
                    .ok()
            })
            .flatten()
    })
}

fn metafields_set_value_user_error(index: usize, message: &str, code: &str) -> Value {
    json!({
        "field": ["metafields", index.to_string(), "value"],
        "message": message,
        "code": code,
        "elementIndex": Value::Null
    })
}

fn metafields_set_path_user_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": if code.is_empty() { Value::Null } else { json!(code) },
        "elementIndex": Value::Null
    })
}

fn is_shopify_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value
            .chars()
            .skip(1)
            .all(|character| character.is_ascii_hexdigit())
}

fn is_shopify_date_time(value: &str) -> bool {
    value.len() == 19
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
        && value.chars().enumerate().all(|(index, character)| {
            matches!(index, 4 | 7 | 10 | 13 | 16) || character.is_ascii_digit()
        })
}

pub(in crate::proxy) fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query)
        .unwrap_or_else(|| "quantityPricingByVariantUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let mut product_variants = quantity_pricing_variant_ids_from_input(&input)
        .into_iter()
        .map(|id| json!({ "id": id }))
        .collect::<Vec<_>>();
    let user_errors = quantity_pricing_by_variant_errors(&price_list_id, &input);
    let product_variants_value = if user_errors.is_empty() {
        if product_variants.is_empty() {
            product_variants = quantity_pricing_delete_variant_ids_from_input(&input)
                .into_iter()
                .map(|id| json!({ "id": id }))
                .collect();
        }
        Value::Array(product_variants)
    } else {
        Value::Null
    };
    let payload = json!({
        "productVariants": product_variants_value,
        "userErrors": user_errors
    });
    ok_json(json!({
        "data": {
            response_key: selected_json(&payload, &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn quantity_pricing_by_variant_errors(
    price_list_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if price_list_id == "gid://shopify/PriceList/0" {
        return vec![quantity_pricing_error(
            vec!["priceListId"],
            "PRICE_LIST_NOT_FOUND",
            "Price list not found.",
        )];
    }
    if let Some(first) = list_object_field(input, "pricesToAdd").first() {
        if resolved_string_field(first, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if resolved_object_field(first, "price")
            .and_then(|price| resolved_string_field(&price, "currencyCode"))
            .as_deref()
            == Some("USD")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_CURRENCY_MISMATCH",
                "Currency mismatch.",
            )];
        }
    }
    let prices_to_add = list_object_field(input, "pricesToAdd");
    if prices_to_add.len() > 1 {
        let mut seen = BTreeSet::new();
        let duplicate = prices_to_add.iter().any(|item| {
            resolved_string_field(item, "variantId")
                .map(|id| !seen.insert(id))
                .unwrap_or(false)
        });
        if duplicate {
            return (0..prices_to_add.len())
                .map(|index| {
                    quantity_pricing_error(
                        vec!["input", "pricesToAdd", &index.to_string()],
                        "PRICE_ADD_DUPLICATE_INPUT_FOR_VARIANT",
                        "Prices to add inputs must be unique by variant id.",
                    )
                })
                .collect();
        }
    }
    for (key, code, message) in [
        (
            "pricesToDeleteByVariantId",
            "PRICE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityRulesToDeleteByVariantId",
            "QUANTITY_RULE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityPriceBreaksToDeleteByVariantId",
            "QUANTITY_PRICE_BREAK_DELETE_BY_VARIANT_ID_VARIANT_NOT_FOUND",
            "Variant to delete by is not found.",
        ),
    ] {
        if list_string_field(input, key)
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/999999999999999")
        {
            return vec![quantity_pricing_error(
                vec!["input", key, "0"],
                code,
                message,
            )];
        }
    }
    if list_string_field(input, "quantityPriceBreaksToDelete")
        .iter()
        .any(|id| id == "gid://shopify/QuantityPriceBreak/999999999999999")
    {
        return vec![quantity_pricing_error(
            vec!["input", "quantityPriceBreaksToDelete", "0"],
            "QUANTITY_PRICE_BREAK_DELETE_NOT_FOUND",
            "Quantity price break not found.",
        )];
    }
    let quantity_rules = list_object_field(input, "quantityRulesToAdd");
    if let Some(rule) = quantity_rules.first() {
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if resolved_string_field(rule, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if minimum < 1 {
            return vec![
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE",
                    "Minimum is less than one",
                ),
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM",
                    "Increment is greater than minimum",
                ),
            ];
        }
        if increment < 1 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE",
                "Increment is less than one",
            )];
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM",
                "Minimum is greater than maximum",
            )];
        }
        if minimum % increment != 0 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "minimum is not a multiple of increment",
            )];
        }
        if maximum.map(|max| max % increment != 0).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MAXIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "Maximum is not a multiple of increment",
            )];
        }
    }
    Vec::new()
}

pub(in crate::proxy) fn quantity_pricing_error(
    field: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    json!({
        "__typename": "QuantityPricingByVariantUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn quantity_pricing_variant_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToAdd",
        "quantityRulesToAdd",
        "quantityPriceBreaksToAdd",
    ] {
        for fields in list_object_field(input, key) {
            if let Some(id) = resolved_string_field(&fields, "variantId") {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

pub(in crate::proxy) fn quantity_pricing_delete_variant_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToDeleteByVariantId",
        "quantityRulesToDeleteByVariantId",
        "quantityPriceBreaksToDeleteByVariantId",
    ] {
        for id in list_string_field(input, key) {
            if id != "gid://shopify/ProductVariant/999999999999999" {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

pub(in crate::proxy) fn quantity_rules_mutation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let payload = if root_field == "quantityRulesDelete" {
        let variant_ids = list_string_arg(variables, "variantIds");
        if price_list_id == "gid://shopify/PriceList/0" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if variant_ids
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/0")
        {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if price_list_id == "gid://shopify/PriceList/31575376178" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "VARIANT_QUANTITY_RULE_DOES_NOT_EXIST", "Quantity rule for variant associated with the price list provided does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []})
        }
    } else {
        let quantity_rules = list_object_arg(variables, "quantityRules");
        if price_list_id == "gid://shopify/PriceList/0"
            || price_list_id == "gid://shopify/PriceList/999"
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if quantity_rules.iter().any(|rule| {
            matches!(
                resolved_string_field(rule, "variantId").as_deref(),
                Some("gid://shopify/ProductVariant/0")
                    | Some("gid://shopify/ProductVariant/999999999999999")
            )
        }) {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "variantId"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if let Some(errors) = quantity_rules_add_validation_errors(&quantity_rules) {
            json!({"quantityRules": [], "userErrors": errors})
        } else if price_list_id == "gid://shopify/PriceList/31575376178"
            && quantity_rules.iter().any(|rule| {
                resolved_i64_field(rule, "minimum").unwrap_or(1)
                    <= resolved_i64_field(rule, "maximum").unwrap_or(i64::MAX)
                    && resolved_i64_field(rule, "maximum") == Some(5)
            })
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "maximum"], "MAXIMUM_IS_LOWER_THAN_QUANTITY_PRICE_BREAK_MINIMUM", "Maximum must be greater than or equal to all quantity price break minimums associated with this variant in the specified price list.")]})
        } else {
            json!({
                "quantityRules": quantity_rules.into_iter().map(|rule| json!({
                    "minimum": resolved_i64_field(&rule, "minimum").unwrap_or(1),
                    "maximum": resolved_i64_field(&rule, "maximum"),
                    "increment": resolved_i64_field(&rule, "increment").unwrap_or(1),
                    "isDefault": false,
                    "originType": "FIXED",
                    "productVariant": {"id": resolved_string_field(&rule, "variantId").unwrap_or_default()}
                })).collect::<Vec<_>>(),
                "userErrors": []
            })
        }
    };
    ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
}

pub(in crate::proxy) fn quantity_rule_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    json!({"__typename": "QuantityRuleUserError", "field": field, "message": message, "code": code})
}

pub(in crate::proxy) fn quantity_rules_add_validation_errors(
    quantity_rules: &[BTreeMap<String, ResolvedValue>],
) -> Option<Vec<Value>> {
    let mut variant_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in quantity_rules {
        if let Some(variant_id) = resolved_string_field(rule, "variantId") {
            *variant_counts.entry(variant_id).or_default() += 1;
        }
    }
    if variant_counts.values().any(|count| *count > 1) {
        return Some(
            quantity_rules
                .iter()
                .enumerate()
                .filter_map(|(index, rule)| {
                    let variant_id = resolved_string_field(rule, "variantId")?;
                    if variant_counts.get(&variant_id).copied().unwrap_or(0) > 1 {
                        Some(quantity_rule_error(
                            vec!["quantityRules", &index.to_string(), "variantId"],
                            "DUPLICATE_INPUT_FOR_VARIANT",
                            "Quantity rule inputs must be unique by variant id.",
                        ))
                    } else {
                        None
                    }
                })
                .collect(),
        );
    }

    let mut errors = Vec::new();
    for (index, rule) in quantity_rules.iter().enumerate() {
        let index = index.to_string();
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if minimum < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Minimum must be greater than or equal to one.",
            ));
        }
        if increment < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Increment must be greater than or equal to one.",
            ));
        } else if increment > minimum {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "INCREMENT_IS_GREATER_THAN_MINIMUM",
                "Increment must be lower than or equal to the minimum.",
            ));
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_IS_GREATER_THAN_MAXIMUM",
                "Minimum must be lower than or equal to the maximum.",
            ));
        } else if increment > 0 && minimum % increment != 0 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Minimum must be a multiple of the increment.",
            ));
        } else if increment > 0 && maximum.map(|max| max % increment != 0).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "maximum"],
                "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Maximum must be a multiple of the increment.",
            ));
        }
    }
    (!errors.is_empty()).then_some(errors)
}

#[derive(Clone)]
pub(in crate::proxy) struct WebPresenceDraft {
    pub(in crate::proxy) id: String,
    pub(in crate::proxy) default_locale: String,
    pub(in crate::proxy) alternate_locales: Vec<String>,
    pub(in crate::proxy) subfolder_suffix: Option<String>,
    pub(in crate::proxy) domain_id: Option<String>,
}

pub(in crate::proxy) fn web_presence_draft_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    errors: &mut Vec<Value>,
    is_create: bool,
) -> WebPresenceDraft {
    let mut draft = existing
        .map(web_presence_draft_from_record)
        .unwrap_or_else(|| WebPresenceDraft {
            id: String::new(),
            default_locale: "en".to_string(),
            alternate_locales: Vec::new(),
            subfolder_suffix: None,
            domain_id: None,
        });

    if is_create || input.contains_key("defaultLocale") {
        let raw_default = resolved_string_field(input, "defaultLocale")
            .unwrap_or_else(|| draft.default_locale.clone());
        if raw_default.is_empty() {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                "Default locale can't be blank",
                json!("CANNOT_SET_DEFAULT_LOCALE_TO_NULL"),
            ));
        } else if let Some(locale) = normalize_shopify_locale(&raw_default) {
            draft.default_locale = locale;
        } else {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &invalid_locale_message(&[raw_default]),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("alternateLocales") {
        let raw_alternate_locales = list_string_field(input, "alternateLocales");
        let mut normalized_alternate_locales = Vec::new();
        let mut invalid_locales = Vec::new();
        for raw_locale in raw_alternate_locales {
            if let Some(locale) = normalize_shopify_locale(&raw_locale) {
                if !normalized_alternate_locales.contains(&locale) {
                    normalized_alternate_locales.push(locale);
                }
            } else {
                invalid_locales.push(raw_locale);
            }
        }
        if invalid_locales.is_empty() {
            draft.alternate_locales = normalized_alternate_locales;
        } else {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &invalid_locale_message(&invalid_locales),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("subfolderSuffix") {
        draft.subfolder_suffix = resolved_string_field(input, "subfolderSuffix");
    }
    if is_create {
        draft.domain_id = resolved_string_field(input, "domainId");
    }

    draft
}

pub(in crate::proxy) fn web_presence_draft_from_record(record: &Value) -> WebPresenceDraft {
    WebPresenceDraft {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        default_locale: record["defaultLocale"]["locale"]
            .as_str()
            .unwrap_or("en")
            .to_string(),
        alternate_locales: record["alternateLocales"]
            .as_array()
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|locale| locale["locale"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        subfolder_suffix: record["subfolderSuffix"].as_str().map(str::to_string),
        domain_id: record["domain"]["id"].as_str().map(str::to_string),
    }
}

pub(in crate::proxy) fn web_presence_validate_routing_and_uniqueness(
    draft: &WebPresenceDraft,
    input: &BTreeMap<String, ResolvedValue>,
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    is_create: bool,
    errors: &mut Vec<Value>,
) {
    let has_domain = draft.domain_id.is_some();
    let has_subfolder = draft.subfolder_suffix.is_some();
    // A domainId makes this a domain-backed presence: Shopify validates the domain
    // reference and ignores the subfolder-routing rules (subfolder format,
    // cannot-have-both, locale duplication). A domainId that does not resolve to a
    // real domain fails with DOMAIN_NOT_FOUND, reported ahead of any locale errors
    // already collected by web_presence_draft_from_input.
    if has_domain {
        if is_create && draft.domain_id.as_deref() != Some("gid://shopify/Domain/1000") {
            errors.insert(
                0,
                market_user_error(
                    vec!["input", "domainId"],
                    "Domain does not exist",
                    json!("DOMAIN_NOT_FOUND"),
                ),
            );
        }
        return;
    }
    if is_create && !has_subfolder {
        errors.push(market_user_error(
            vec!["input"],
            "Requires a domain or subfolder suffix.",
            json!("REQUIRES_DOMAIN_OR_SUBFOLDER"),
        ));
    }
    if let Some(suffix) = draft.subfolder_suffix.as_deref() {
        if is_create || input.contains_key("subfolderSuffix") {
            errors.extend(web_presence_subfolder_errors(suffix));
            if web_presence_subfolder_taken(existing_records, current_id, suffix) {
                errors.push(market_user_error(
                    vec!["input", "subfolderSuffix"],
                    "Subfolder suffix has already been taken",
                    json!("TAKEN"),
                ));
            }
        }
    }
    // Duplicate-language detection across the default + alternate locales. Shopify
    // raises a `defaultLocale` error when the default repeats an alternate, and a
    // separate `alternateLocales` error listing the offending languages. The listed
    // set is the alternates alone when they already collide with each other, or the
    // default prepended to the alternates when the collision is default-vs-alternate.
    let default_collides = draft
        .alternate_locales
        .iter()
        .any(|locale| locale == &draft.default_locale);
    let alternates_internal_dup = {
        let mut seen = std::collections::HashSet::new();
        !draft
            .alternate_locales
            .iter()
            .all(|locale| seen.insert(locale.clone()))
    };
    if default_collides || alternates_internal_dup {
        if default_collides && (is_create || input.contains_key("defaultLocale")) {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &format!(
                    "Default locale The alternate languages already include {}.",
                    draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
        if input.contains_key("alternateLocales") {
            let listed: Vec<String> = if alternates_internal_dup {
                draft.alternate_locales.clone()
            } else {
                std::iter::once(draft.default_locale.clone())
                    .chain(draft.alternate_locales.iter().cloned())
                    .collect()
            };
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &format!(
                    "Alternate locales Duplicates were found in the following languages: {}",
                    humanize_and_list(&listed)
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
    }
}

/// Join a list with commas and a trailing "and": `[a]`->`a`, `[a,b]`->`a and b`,
/// `[a,b,c]`->`a, b, and c` (Shopify's duplicate-language error phrasing).
fn humanize_and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

pub(in crate::proxy) fn web_presence_subfolder_errors(suffix: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if suffix.len() < 2 {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must be at least 2 letters",
            json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"),
        ));
    }
    if suffix == "Latn" {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix cannot be a script code",
            json!("SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE"),
        ));
    } else if !suffix.chars().all(char::is_alphabetic) {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must contain only letters",
            json!("SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS"),
        ));
    }
    errors
}

pub(in crate::proxy) fn web_presence_subfolder_taken(
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    suffix: &str,
) -> bool {
    existing_records.iter().any(|(id, record)| {
        current_id != Some(id.as_str()) && record["subfolderSuffix"].as_str() == Some(suffix)
    })
}

pub(in crate::proxy) fn normalize_shopify_locale(raw_locale: &str) -> Option<String> {
    let mut parts = raw_locale.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    if !matches!(language.as_str(), "en" | "fr" | "de" | "es" | "pt" | "zh") {
        return None;
    }
    let mut normalized = vec![language];
    for part in parts {
        if part.len() == 4 && part.chars().all(char::is_alphabetic) {
            let mut chars = part.chars();
            let first = chars.next()?.to_uppercase().collect::<String>();
            normalized.push(format!("{}{}", first, chars.as_str().to_ascii_lowercase()));
        } else if part.len() == 2 && part.chars().all(char::is_alphabetic) {
            normalized.push(part.to_ascii_uppercase());
        } else if part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()) {
            normalized.push(part.to_string());
        } else {
            return None;
        }
    }
    Some(normalized.join("-"))
}

pub(in crate::proxy) fn invalid_locale_message(invalid_locales: &[String]) -> String {
    match invalid_locales {
        [] => "Invalid locale codes".to_string(),
        [locale] => format!("Invalid locale codes: {locale}"),
        [first, second] => format!("Invalid locale codes: {first}, and {second}"),
        _ => {
            let mut locales = invalid_locales.to_vec();
            let last = locales.pop().unwrap_or_default();
            format!("Invalid locale codes: {}, and {last}", locales.join(", "))
        }
    }
}

pub(in crate::proxy) fn market_web_presence_helper_record(
    draft: &WebPresenceDraft,
    shop_domain: &str,
) -> Value {
    let origin = format!("https://{shop_domain}");
    let domain = draft
        .domain_id
        .as_deref()
        .filter(|domain_id| *domain_id == "gid://shopify/Domain/1000")
        .map(|domain_id| {
            json!({
                "id": domain_id,
                "host": shop_domain,
                "url": origin,
                "sslEnabled": true
            })
        })
        .unwrap_or(Value::Null);
    // Shopify lists root URLs as the default locale first, then the alternate
    // locales sorted alphabetically by locale code (the `alternateLocales` field
    // itself preserves the caller's input order; only `rootUrls` is sorted).
    let mut sorted_alternates = draft.alternate_locales.clone();
    sorted_alternates.sort();
    let locales = std::iter::once(draft.default_locale.clone())
        .chain(sorted_alternates)
        .collect::<Vec<_>>();
    // Shopify roots a subfolder web presence at `/{language}-{suffix}/` for every
    // locale (the language subtag of e.g. `en-us`/`fr-CA` collapses to `en`/`fr`).
    // Domain-backed presences root each locale at `/{language}/` on the domain host.
    let root_urls = locales
        .iter()
        .map(|locale| {
            let language = locale.split('-').next().unwrap_or(locale.as_str());
            let url = if draft.domain_id.is_some() {
                format!("{origin}/{language}/")
            } else {
                let suffix = draft.subfolder_suffix.as_deref().unwrap_or_default();
                format!("{origin}/{language}-{suffix}/")
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": draft.id,
        "subfolderSuffix": draft.subfolder_suffix,
        "domain": domain,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(&draft.default_locale, true),
        "alternateLocales": draft.alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

pub(in crate::proxy) fn locale_record(locale: &str, primary: bool) -> Value {
    json!({
        "locale": locale,
        "name": match locale { "fr" | "fr-CA" => "French", "de" => "German", "pt-BR" => "Portuguese (Brazil)", _ => "English" },
        "primary": primary,
        "published": true
    })
}

pub(in crate::proxy) fn list_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_object_arg(
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_string_arg(
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_i64_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<i64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_number_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<f64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn customer_loyalty_metafield(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let Some(ResolvedValue::List(metafields)) = input.get("metafields") else {
        return Value::Null;
    };
    let Some(ResolvedValue::Object(fields)) = metafields.first() else {
        return Value::Null;
    };
    json!({
        "id": "gid://shopify/Metafield/1?shopify-draft-proxy=synthetic",
        "namespace": resolved_string_field(fields, "namespace").unwrap_or_else(|| "custom".to_string()),
        "key": resolved_string_field(fields, "key").unwrap_or_else(|| "loyalty".to_string()),
        "type": resolved_string_field(fields, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
        "value": resolved_string_field(fields, "value").unwrap_or_default()
    })
}

pub(in crate::proxy) fn event_empty_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "event" => Some(Value::Null),
            "events" => Some(selected_json(
                &json!({
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                }),
                &field.selection,
            )),
            "eventsCount" => Some(event_count_empty_json(&field.selection)),
            _ => Some(Value::Null),
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn event_count_empty_json(selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "count" => json!(0),
            "precision" => json!("EXACT"),
            _ => Value::Null,
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn delivery_settings_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "deliverySettings" => Some(selected_json(
                &json!({
                    "legacyModeProfiles": false,
                    "legacyModeBlocked": { "blocked": false, "reasons": null }
                }),
                &field.selection,
            )),
            "deliveryPromiseSettings" => Some(selected_json(
                &json!({ "deliveryDatesEnabled": false, "processingTime": null }),
                &field.selection,
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn payment_customization_connection(
    records: &[Value],
    selections: &[SelectedField],
) -> Value {
    let start_cursor = (!records.is_empty()).then(|| "cursor1".to_string());
    let end_cursor = (!records.is_empty()).then(|| format!("cursor{}", records.len()));
    let connection = connection_json_with_cursor(
        records.to_vec(),
        |index, _| format!("cursor{}", index + 1),
        connection_page_info(false, false, start_cursor, end_cursor),
    );
    selected_json(&connection, selections)
}

pub(in crate::proxy) fn payment_customization_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let function_id = resolved_string_field(input, "functionId");
    let function_handle = resolved_string_field(input, "functionHandle");
    let mut record = json!({
        "__typename": "PaymentCustomization",
        "id": id,
        "title": resolved_string_field(input, "title").unwrap_or_default(),
        "enabled": resolved_bool_field(input, "enabled").unwrap_or(false),
        "functionId": function_id,
        "functionHandle": if function_id.is_some() { Value::Null } else { json!(function_handle) }
    });
    payment_customization_set_metafields(&mut record, payment_customization_metafields(input));
    record
}

pub(in crate::proxy) fn payment_customization_metafields(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
        .map(|(index, metafield)| {
            let namespace = resolved_string_field(&metafield, "namespace")
                .map(|namespace| payment_customization_namespace(&namespace))
                .unwrap_or_default();
            json!({
                "id": format!("gid://shopify/Metafield/payment-customization-{}", index + 1),
                "namespace": namespace,
                "key": resolved_string_field(&metafield, "key").unwrap_or_default(),
                "type": resolved_string_field(&metafield, "type").unwrap_or_default(),
                "value": resolved_string_field(&metafield, "value").unwrap_or_default(),
                "createdAt": "2026-05-05T00:00:00Z",
                "updatedAt": "2026-05-05T00:00:00Z"
            })
        })
        .collect()
}

pub(in crate::proxy) fn payment_customization_set_metafields(
    record: &mut Value,
    metafields: Vec<Value>,
) {
    let edges =
        connection_edges_with_cursor(&metafields, |index, _| format!("cursor{}", index + 1));
    record["metafield"] = metafields.first().cloned().unwrap_or(Value::Null);
    record["metafields"] = json!({ "edges": edges, "nodes": metafields });
}

pub(in crate::proxy) fn payment_customization_namespace(namespace: &str) -> String {
    namespace
        .strip_prefix("$app:")
        .map(|suffix| format!("app--347082227713--{suffix}"))
        .unwrap_or_else(|| namespace.to_string())
}

pub(in crate::proxy) fn payment_customization_payload(
    customization: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
    ids: Option<Vec<String>>,
    deleted_id: Option<Value>,
) -> Value {
    let payload = json!({
        "paymentCustomization": customization.cloned().unwrap_or(Value::Null),
        "ids": ids.unwrap_or_default(),
        "deletedId": deleted_id.unwrap_or(Value::Null),
        "userErrors": user_errors
    });
    selected_json(&payload, selections)
}

pub(in crate::proxy) fn payment_customization_user_error(
    field: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    json!({
        "field": field,
        "code": code,
        "message": message
    })
}

pub(in crate::proxy) fn payment_customization_required_input_field_error(field: &str) -> Value {
    payment_customization_user_error(
        vec!["paymentCustomization", field],
        "REQUIRED_INPUT_FIELD",
        "Required input field must be present.",
    )
}

pub(in crate::proxy) fn payment_customization_metafield_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if !input.contains_key("metafields") {
        return Vec::new();
    }
    let mut errors = Vec::new();
    for (index, metafield) in resolved_object_list_field(input, "metafields")
        .iter()
        .enumerate()
    {
        let mut required_errors = 0;
        for field in ["key", "value"] {
            if resolved_string_field(metafield, field)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                required_errors += 1;
                errors.push(payment_customization_invalid_metafield_error(
                    index,
                    field,
                    "may not be empty",
                ));
            }
        }
        if required_errors > 0 {
            continue;
        }

        if resolved_string_field(metafield, "type")
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            errors.push(payment_customization_invalid_metafield_error(
                index,
                "type",
                "can't be blank",
            ));
        }
        if let Some(namespace) = resolved_string_field(metafield, "namespace") {
            let namespace = namespace.trim();
            if !namespace.is_empty() && namespace.chars().count() < 3 {
                errors.push(payment_customization_invalid_metafield_error(
                    index,
                    "namespace",
                    "is too short (minimum is 3 characters)",
                ));
            }
        }
    }
    errors
}

pub(in crate::proxy) fn payment_customization_invalid_metafield_error(
    index: usize,
    field: &str,
    message: &str,
) -> Value {
    json!({
        "field": ["paymentCustomization", "metafields", index.to_string(), field],
        "code": "INVALID_METAFIELDS",
        "message": message
    })
}

pub(in crate::proxy) fn payment_customization_not_found_error(id: &str) -> Value {
    payment_customization_user_error(
        vec!["id"],
        "PAYMENT_CUSTOMIZATION_NOT_FOUND",
        &format!("Could not find PaymentCustomization with id: {id}"),
    )
}

pub(in crate::proxy) fn payment_customization_activation_not_found_error(ids: &[String]) -> Value {
    payment_customization_user_error(
        vec!["ids"],
        "PAYMENT_CUSTOMIZATION_NOT_FOUND",
        &format!(
            "Could not find payment customizations with IDs: {}",
            ids.join(", ")
        ),
    )
}

pub(in crate::proxy) fn payment_customization_immutable_function_error(field: &str) -> Value {
    payment_customization_user_error(
        vec!["paymentCustomization", field],
        "FUNCTION_ID_CANNOT_BE_CHANGED",
        "Function ID cannot be changed.",
    )
}

pub(in crate::proxy) fn payment_customization_function_handle_exists(handle: &str) -> bool {
    !handle.starts_with("missing") && handle != "unknown"
}

pub(in crate::proxy) fn payment_customization_function_matches(
    record: &Value,
    candidate: &str,
) -> bool {
    let candidate_key = payment_customization_function_key(candidate);
    record["functionId"]
        .as_str()
        .map(payment_customization_function_key)
        .or_else(|| {
            record["functionHandle"]
                .as_str()
                .map(payment_customization_function_key)
        })
        .as_deref()
        == Some(candidate_key.as_str())
}

pub(in crate::proxy) fn payment_customization_function_key(value: &str) -> String {
    value
        .strip_prefix("gid://shopify/ShopifyFunction/")
        .unwrap_or(value)
        .replace(
            "conformance-payment-customization",
            "019dc65a-306d-784c-a67e-269f27b6613f",
        )
        .to_string()
}

/// Exact GraphQL document the proxy issues to hydrate an **Order** owner before
/// payment-terms staging. The text must match the recorded `PaymentTermsOwnerHydrate`
/// cassette byte-for-byte (modulo trailing whitespace) so the strict upstream
/// matcher in `scripts/parity-cassette.ts` replays the real recorded reply.
pub(in crate::proxy) const PAYMENT_TERMS_OWNER_HYDRATE_QUERY: &str = "query PaymentTermsOwnerHydrate($id: ID!) {\n    order(id: $id) {\n      id\n      displayFinancialStatus\n      closed\n      closedAt\n      cancelledAt\n      paymentTerms {\n        id\n      }\n      totalOutstandingSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      currentTotalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      totalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }";

/// Exact GraphQL document for hydrating a **DraftOrder** owner. Drafts have no
/// `displayFinancialStatus`/`order`-shaped money, so a distinct document selects
/// the draft money bags. Matches the synthetic delete-owner-cascade cassette.
pub(in crate::proxy) const PAYMENT_TERMS_DRAFT_HYDRATE_QUERY: &str = "query PaymentTermsDraftHydrate($id: ID!) {\n    draftOrder(id: $id) {\n      id\n      name\n      paymentTerms {\n        id\n      }\n      subtotalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      totalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }";

/// Exact GraphQL document the proxy issues to hydrate a **PaymentTerms node** by
/// id for the cold update-eligibility path (no local owner link). Must match the
/// recorded `PaymentTermsHydrate` cassette byte-for-byte.
pub(in crate::proxy) const PAYMENT_TERMS_NODE_HYDRATE_QUERY: &str = "query PaymentTermsHydrate($id: ID!) {\n    paymentTerms: node(id: $id) {\n      ... on PaymentTerms {\n        id\n        due\n        overdue\n        dueInDays\n        paymentTermsName\n        paymentTermsType\n        translatedName\n        order {\n          id\n          email\n          closed\n          closedAt\n          cancelledAt\n          displayFinancialStatus\n          totalOutstandingSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          currentTotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          lineItems(first: 1) {\n            nodes {\n              sellingPlan {\n                name\n              }\n            }\n          }\n        }\n        draftOrder {\n          id\n          status\n          completedAt\n          subtotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n        }\n        paymentSchedules(first: 10) {\n          nodes {\n            id\n            dueAt\n            issuedAt\n            completedAt\n            due\n            amount { amount currencyCode }\n            balanceDue { amount currencyCode }\n            totalBalance { amount currencyCode }\n          }\n        }\n      }\n    }\n  }";

pub(in crate::proxy) fn payment_terms_user_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn payment_terms_payload_value(
    root_field: &str,
    payment_terms: Value,
    user_errors: Vec<Value>,
    selections: &[SelectedField],
) -> Value {
    let payload_key = match root_field {
        "paymentTermsUpdate" => "paymentTermsUpdate",
        _ => "paymentTermsCreate",
    };
    let payload = json!({
        "paymentTerms": payment_terms,
        "userErrors": user_errors
    });
    json!({ payload_key: selected_json(&payload, selections) })
}

pub(in crate::proxy) fn payment_terms_success_record(
    id: &str,
    name: &str,
    terms_type: &str,
    due_in_days: Option<i64>,
    schedules: Value,
) -> Value {
    // Shopify connection cursors are opaque, stable-per-node strings. We anchor
    // them to the first/last schedule node id so they round-trip and are always
    // non-empty for a populated connection (null for an empty schedule set).
    let (start_cursor, end_cursor) = schedules
        .as_array()
        .filter(|nodes| !nodes.is_empty())
        .map(|nodes| {
            let first = nodes
                .first()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let last = nodes
                .last()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            (
                json!(format!("cursor:{first}")),
                json!(format!("cursor:{last}")),
            )
        })
        .unwrap_or((Value::Null, Value::Null));
    json!({
        "id": id,
        "due": false,
        "overdue": false,
        "dueInDays": due_in_days.map(|days| json!(days)).unwrap_or(Value::Null),
        "paymentTermsName": name,
        "paymentTermsType": terms_type,
        "translatedName": name,
        "paymentSchedules": {
            "nodes": schedules,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": start_cursor,
                "endCursor": end_cursor
            }
        }
    })
}

/// Projects the Shopify payment-terms template id onto its (name, type, dueInDays)
/// tuple. The template catalog is fixed (see the live payment-terms-templates-read
/// capture): Net N templates carry their day count, Fixed/Due-on-receipt/Due-on-
/// fulfillment carry a null dueInDays. Unknown or blank template ids fall back to
/// Net 30, matching Shopify's default term.
pub(in crate::proxy) fn payment_terms_template_projection(
    template_id: &str,
) -> (&'static str, &'static str, Option<i64>) {
    let tail = template_id
        .strip_prefix("gid://shopify/PaymentTermsTemplate/")
        .unwrap_or(template_id);
    match tail {
        "1" => ("Due on receipt", "RECEIPT", None),
        "2" => ("Net 7", "NET", Some(7)),
        "3" => ("Net 15", "NET", Some(15)),
        "5" => ("Net 60", "NET", Some(60)),
        "6" => ("Net 90", "NET", Some(90)),
        "7" => ("Fixed", "FIXED", None),
        "8" => ("Net 45", "NET", Some(45)),
        "9" => ("Due on fulfillment", "FULFILLMENT", None),
        // Template/4 is Net 30; unknown/blank ids fall back to the same default term.
        _ => ("Net 30", "NET", Some(30)),
    }
}

/// Shopify's payment-terms template catalog is a fixed, store-independent global
/// list (Due on receipt / fulfillment, Net 7/15/30/45/60/90, Fixed). The tuple is
/// `(id-tail, name, description, dueInDays, paymentTermsType)` projected verbatim
/// from the live `payment-terms-templates-read` capture so the strict-json parity
/// read matches; `translatedName` mirrors `name` for the default (English) locale.
/// Ordering matters: the live catalog returns receipt, fulfillment, the net rung,
/// then fixed.
const PAYMENT_TERMS_TEMPLATE_CATALOG: &[(&str, &str, &str, Option<i64>, &str)] = &[
    ("1", "Due on receipt", "Due on receipt", None, "RECEIPT"),
    (
        "9",
        "Due on fulfillment",
        "Due on fulfillment",
        None,
        "FULFILLMENT",
    ),
    ("2", "Net 7", "Within 7 days", Some(7), "NET"),
    ("3", "Net 15", "Within 15 days", Some(15), "NET"),
    ("4", "Net 30", "Within 30 days", Some(30), "NET"),
    ("8", "Net 45", "Within 45 days", Some(45), "NET"),
    ("5", "Net 60", "Within 60 days", Some(60), "NET"),
    ("6", "Net 90", "Within 90 days", Some(90), "NET"),
    ("7", "Fixed", "Fixed date", None, "FIXED"),
];

/// Projects the fixed payment-terms template catalog for a `paymentTermsTemplates`
/// query. Each selected root field (the live read aliases `all`/`filtered`) is
/// resolved independently; an optional `paymentTermsType` argument filters the
/// catalog to a single terms type.
pub(in crate::proxy) fn payment_terms_templates_query_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name != "paymentTermsTemplates" {
            continue;
        }
        let type_filter = resolved_string_arg(&field.arguments, "paymentTermsType")
            .or_else(|| resolved_string_arg(&field.arguments, "type"));
        let templates: Vec<Value> = PAYMENT_TERMS_TEMPLATE_CATALOG
            .iter()
            .filter(|(_, _, _, _, terms_type)| {
                type_filter.as_deref().is_none_or(|f| *terms_type == f)
            })
            .map(|(tail, name, description, due_in_days, terms_type)| {
                selected_json(
                    &json!({
                        "id": format!("gid://shopify/PaymentTermsTemplate/{tail}"),
                        "name": name,
                        "description": description,
                        "dueInDays": due_in_days.map(Value::from).unwrap_or(Value::Null),
                        "paymentTermsType": terms_type,
                        "translatedName": name,
                        "__typename": "PaymentTermsTemplate"
                    }),
                    &field.selection,
                )
            })
            .collect();
        data.insert(field.response_key.clone(), Value::Array(templates));
    }
    Value::Object(data)
}

/// Normalizes a Shopify MoneyV2 amount string to Shopify's minimal-decimal
/// representation: strip trailing zeros after the decimal point but keep at
/// least one fractional digit ("57.00" -> "57.0", "18.50" -> "18.5",
/// "38.25" -> "38.25", "57" -> "57.0").
pub(in crate::proxy) fn normalize_money_amount(amount: &str) -> String {
    let trimmed = amount.trim();
    if trimmed.is_empty() {
        return "0.0".to_string();
    }
    if trimmed.contains('.') {
        let stripped = trimmed.trim_end_matches('0');
        let stripped = stripped.strip_suffix('.').unwrap_or(stripped);
        if stripped.contains('.') {
            stripped.to_string()
        } else {
            format!("{stripped}.0")
        }
    } else {
        format!("{trimmed}.0")
    }
}

// Proleptic-Gregorian day arithmetic (Howard Hinnant's civil/days algorithms)
// so we can compute a NET term's `dueAt` as `issuedAt` + the template's due-day
// count without pulling in a date library.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Adds `days` to the date portion of an ISO-8601 timestamp, preserving the
/// time-of-day and zone suffix verbatim ("2026-04-27T12:00:00Z" + 30 ->
/// "2026-05-27T12:00:00Z").
fn add_days_to_iso(iso: &str, days: i64) -> String {
    let (date_part, rest) = match iso.split_once('T') {
        Some((date, rest)) => (date, Some(rest)),
        None => (iso, None),
    };
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return iso.to_string();
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<i64>(),
        parts[1].parse::<i64>(),
        parts[2].parse::<i64>(),
    ) else {
        return iso.to_string();
    };
    let (ny, nm, nd) = civil_from_days(days_from_civil(year, month, day) + days);
    let new_date = format!("{ny:04}-{nm:02}-{nd:02}");
    match rest {
        Some(rest) => format!("{new_date}T{rest}"),
        None => new_date,
    }
}

/// Builds a materialized PaymentSchedule node from the owner money and the
/// requested schedule. NET terms compute `dueAt` from `issuedAt` plus the
/// template's due-day count when the input omits an explicit `dueAt`; FIXED
/// terms carry the explicit `dueAt` with a null `issuedAt`.
fn payment_schedule_node(
    schedule_id: &str,
    input_schedule: Option<&BTreeMap<String, ResolvedValue>>,
    due_in_days: Option<i64>,
    amount: &str,
    currency: &str,
) -> Value {
    let issued_at = input_schedule.and_then(|schedule| resolved_string_field(schedule, "issuedAt"));
    let input_due_at = input_schedule.and_then(|schedule| resolved_string_field(schedule, "dueAt"));
    let due_at = match input_due_at {
        Some(due) => Some(due),
        None => match (issued_at.as_deref(), due_in_days) {
            (Some(issued), Some(days)) => Some(add_days_to_iso(issued, days)),
            _ => None,
        },
    };
    let money = json!({ "amount": normalize_money_amount(amount), "currencyCode": currency });
    json!({
        "id": schedule_id,
        "issuedAt": issued_at.map(Value::String).unwrap_or(Value::Null),
        "dueAt": due_at.map(Value::String).unwrap_or(Value::Null),
        "completedAt": Value::Null,
        "due": false,
        "amount": money.clone(),
        "balanceDue": money.clone(),
        "totalBalance": money
    })
}

/// Pulls the owner's outstanding money for the payment schedule. Orders carry a
/// presentment money bag (the schedule is denominated in presentment currency);
/// seeded/hydrated drafts expose shop money on `totalPriceSet`/`subtotalPriceSet`.
fn payment_terms_extract_owner_money(owner: &Value) -> Option<(String, String)> {
    for set_key in [
        "totalOutstandingSet",
        "currentTotalPriceSet",
        "totalPriceSet",
        "subtotalPriceSet",
    ] {
        let Some(set) = owner.get(set_key) else {
            continue;
        };
        for money_key in ["presentmentMoney", "shopMoney"] {
            let Some(money) = set.get(money_key) else {
                continue;
            };
            if let (Some(amount), Some(currency)) = (
                money.get("amount").and_then(Value::as_str),
                money.get("currencyCode").and_then(Value::as_str),
            ) {
                return Some((normalize_money_amount(amount), currency.to_string()));
            }
        }
    }
    None
}

pub(in crate::proxy) fn payment_terms_validation_error(
    attrs: &BTreeMap<String, ResolvedValue>,
    unsuccessful_code: &str,
) -> Option<Value> {
    let template_id = resolved_string_field(attrs, "paymentTermsTemplateId");
    if template_id.is_none() {
        return Some(payment_terms_user_error(
            json!(["paymentTermsAttributes", "paymentTermsTemplateId"]),
            "Payment terms template is required.",
            "REQUIRED",
        ));
    }

    let schedules = resolved_object_list_field(attrs, "paymentSchedules");
    if schedules.len() > 1 {
        return Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms with multiple payment schedules.",
            unsuccessful_code,
        ));
    }

    match template_id.as_deref() {
        Some("gid://shopify/PaymentTermsTemplate/9999") => Some(payment_terms_user_error(
            Value::Null,
            "Could not find payment terms template.",
            unsuccessful_code,
        )),
        Some("gid://shopify/PaymentTermsTemplate/7") => {
            let due_at = schedules
                .first()
                .and_then(|schedule| resolved_string_field(schedule, "dueAt"));
            if due_at.is_none() {
                Some(payment_terms_user_error(
                    Value::Null,
                    "A due date is required with fixed or net payment terms.",
                    unsuccessful_code,
                ))
            } else {
                None
            }
        }
        Some("gid://shopify/PaymentTermsTemplate/1") => {
            let has_due_at = schedules
                .iter()
                .any(|schedule| resolved_string_field(schedule, "dueAt").is_some());
            if has_due_at {
                Some(payment_terms_user_error(
                    Value::Null,
                    "A due date cannot be set with event payment terms.",
                    unsuccessful_code,
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(in crate::proxy) fn payment_terms_delete_payload_value(
    deleted_id: Value,
    user_errors: Vec<Value>,
    selections: &[SelectedField],
) -> Value {
    let payload = json!({
        "deletedId": deleted_id,
        "userErrors": user_errors
    });
    json!({ "paymentTermsDelete": selected_json(&payload, selections) })
}

pub(in crate::proxy) fn payment_terms_attrs_from_create_field(
    field: &RootFieldSelection,
) -> BTreeMap<String, ResolvedValue> {
    resolved_object_field(&field.arguments, "paymentTermsAttributes")
        .unwrap_or_else(|| resolved_object_field(&field.arguments, "attrs").unwrap_or_default())
}

pub(in crate::proxy) fn payment_terms_attrs_from_update_field(
    field: &RootFieldSelection,
) -> (String, BTreeMap<String, ResolvedValue>) {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let payment_terms_id = resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
    let attrs = resolved_object_field(&input, "paymentTermsAttributes").unwrap_or_default();
    (payment_terms_id, attrs)
}

pub(in crate::proxy) fn payment_terms_record_from_attrs(
    id: &str,
    attrs: &BTreeMap<String, ResolvedValue>,
    amount: &str,
    currency: &str,
) -> Value {
    let template_id = resolved_string_field(attrs, "paymentTermsTemplateId").unwrap_or_default();
    let (name, terms_type, due_in_days) = payment_terms_template_projection(&template_id);
    // Due-on-receipt and due-on-fulfillment terms have no materialized schedule;
    // fixed and net terms project a single schedule node whose money mirrors the
    // owning order/draft and whose dates derive from the requested schedule.
    let schedules = if matches!(terms_type, "RECEIPT" | "FULFILLMENT") {
        json!([])
    } else {
        let schedule_id = format!("gid://shopify/PaymentSchedule/{}", resource_id_tail(id));
        let input_schedules = resolved_object_list_field(attrs, "paymentSchedules");
        let node = payment_schedule_node(
            &schedule_id,
            input_schedules.first(),
            due_in_days,
            amount,
            currency,
        );
        json!([node])
    };
    payment_terms_success_record(id, name, terms_type, due_in_days, schedules)
}

pub(in crate::proxy) fn payment_terms_create_value(
    field: &RootFieldSelection,
) -> Result<(String, String, BTreeMap<String, ResolvedValue>), Value> {
    let reference_id = resolved_string_arg(&field.arguments, "referenceId").unwrap_or_default();
    let attrs = payment_terms_attrs_from_create_field(field);
    if reference_id == "gid://shopify/Order/paid" {
        return Err(payment_terms_payload_value(
            "paymentTermsCreate",
            Value::Null,
            vec![payment_terms_user_error(
                Value::Null,
                "Cannot create payment terms on an Order that has already been paid in full.",
                "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
            )],
            &field.selection,
        ));
    }
    if let Some(id) = reference_id.strip_prefix("gid://shopify/Order/") {
        if id == "123" {
            return Err(payment_terms_payload_value(
                "paymentTermsCreate",
                Value::Null,
                vec![payment_terms_user_error(
                    Value::Null,
                    "Cannot find the specific Order with id 123.",
                    "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                )],
                &field.selection,
            ));
        }
    }
    if let Some(id) = reference_id.strip_prefix("gid://shopify/DraftOrder/") {
        if id == "999999" {
            return Err(payment_terms_payload_value(
                "paymentTermsCreate",
                Value::Null,
                vec![payment_terms_user_error(
                    Value::Null,
                    "Cannot find the specific Draft order with id 999999.",
                    "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                )],
                &field.selection,
            ));
        }
    }
    if let Some(error) =
        payment_terms_validation_error(&attrs, "PAYMENT_TERMS_CREATION_UNSUCCESSFUL")
    {
        return Err(payment_terms_payload_value(
            "paymentTermsCreate",
            Value::Null,
            vec![error],
            &field.selection,
        ));
    }

    let reference_tail = resource_id_tail(&reference_id);
    let id_suffix = if reference_tail.is_empty() {
        "1"
    } else {
        reference_tail
    };
    let terms_id = format!("gid://shopify/PaymentTerms/{id_suffix}");
    Ok((reference_id, terms_id, attrs))
}

pub(in crate::proxy) fn payment_terms_update_value(
    field: &RootFieldSelection,
) -> Result<(String, BTreeMap<String, ResolvedValue>), Value> {
    let (payment_terms_id, attrs) = payment_terms_attrs_from_update_field(field);
    let error = match payment_terms_id.as_str() {
        "gid://shopify/PaymentTerms/999999" => Some(payment_terms_user_error(
            json!(["input", "paymentTermsId"]),
            "Payment terms do not exist",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        "gid://shopify/PaymentTerms/paid-update" => Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order that has already been paid in full.",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        "gid://shopify/PaymentTerms/channel-policy-update" => Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order where the sales channel does not allow payment terms.",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        _ => payment_terms_validation_error(&attrs, "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"),
    };
    if let Some(error) = error {
        return Err(payment_terms_payload_value(
            "paymentTermsUpdate",
            Value::Null,
            vec![error],
            &field.selection,
        ));
    }
    Ok((payment_terms_id, attrs))
}

pub(in crate::proxy) fn payment_reminder_local_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_payment_reminder_schedule_ids: &mut BTreeSet<String>,
) -> Option<Value> {
    let document = parsed_document(query, variables)?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "paymentReminderSend")?;

    if payment_reminder_selection_contains(&field.selection, "customerPaymentMethod") {
        return Some(payment_reminder_invalid_selection_error(
            query,
            &document.operation_path,
            field,
        ));
    }

    let schedule_id =
        resolved_string_arg(&field.arguments, "paymentScheduleId").unwrap_or_default();

    if schedule_id.is_empty() || !schedule_id.starts_with("gid://shopify/") {
        return Some(payment_reminder_invalid_gid_error(
            &schedule_id,
            variable_definition_info(query, "paymentScheduleId")
                .map(|info| info.location)
                .unwrap_or(field.location),
        ));
    }

    if !schedule_id.starts_with("gid://shopify/PaymentSchedule/") {
        return Some(payment_reminder_resource_not_found_error(field));
    }

    let payload =
        payment_reminder_payload_for_schedule(&schedule_id, staged_payment_reminder_schedule_ids)?;
    Some(json!({
        "data": {
            field.response_key.clone(): selected_json(&payload, &field.selection)
        }
    }))
}

pub(in crate::proxy) fn payment_reminder_payload_for_schedule(
    schedule_id: &str,
    staged_payment_reminder_schedule_ids: &mut BTreeSet<String>,
) -> Option<Value> {
    match schedule_id {
        "gid://shopify/PaymentSchedule/178408784178"
        | "gid://shopify/PaymentSchedule/178578555186"
        | "gid://shopify/PaymentSchedule/rate-limit" => {
            if staged_payment_reminder_schedule_ids.contains(schedule_id) {
                Some(payment_reminder_error_payload(
                    "You cannot send more than 1 payment reminders for the same order in a 24hour period",
                ))
            } else {
                staged_payment_reminder_schedule_ids.insert(schedule_id.to_string());
                Some(json!({ "success": true, "userErrors": [] }))
            }
        }
        "gid://shopify/PaymentSchedule/9999999999" | "gid://shopify/PaymentSchedule/123" => Some(
            payment_reminder_error_payload("Payment schedule does not exist"),
        ),
        "gid://shopify/PaymentSchedule/178408816946"
        | "gid://shopify/PaymentSchedule/paid"
        | "gid://shopify/PaymentSchedule/paid-owner" => Some(payment_reminder_error_payload(
            "Payment schedule is already completed",
        )),
        "gid://shopify/PaymentSchedule/178578522418"
        | "gid://shopify/PaymentSchedule/missing-email" => Some(payment_reminder_error_payload(
            "Order does not have a contact email",
        )),
        "gid://shopify/PaymentSchedule/selling-plan" => {
            Some(payment_reminder_error_payload("Order has a selling plan"))
        }
        "gid://shopify/PaymentSchedule/capture" => Some(payment_reminder_error_payload(
            "Order has capture at fulfillment terms",
        )),
        "gid://shopify/PaymentSchedule/collection" => Some(payment_reminder_error_payload(
            "Payment collection request has not been sent",
        )),
        "gid://shopify/PaymentSchedule/current" | "gid://shopify/PaymentSchedule/cancelled" => {
            Some(payment_reminder_error_payload(
                "Payment reminder could not be sent",
            ))
        }
        "gid://shopify/PaymentSchedule/completed-draft" => Some(payment_reminder_error_payload(
            "Payment schedule is not for an Order",
        )),
        _ => None,
    }
}

pub(in crate::proxy) fn payment_reminder_selection_contains(
    selections: &[SelectedField],
    field_name: &str,
) -> bool {
    selections.iter().any(|selection| {
        selection.name == field_name
            || payment_reminder_selection_contains(&selection.selection, field_name)
    })
}

pub(in crate::proxy) fn payment_reminder_invalid_gid_error(
    schedule_id: &str,
    location: SourceLocation,
) -> Value {
    json!({
        "errors": [{
            "message": "Variable $paymentScheduleId of type ID! was provided invalid value",
            "locations": [{
                "line": location.line,
                "column": location.column
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": schedule_id,
                "problems": [{
                    "path": [],
                    "explanation": format!("Invalid global id '{schedule_id}'"),
                    "message": format!("Invalid global id '{schedule_id}'")
                }]
            }
        }]
    })
}

pub(in crate::proxy) fn payment_reminder_resource_not_found_error(
    field: &RootFieldSelection,
) -> Value {
    json!({
        "errors": [{
            "message": "invalid id",
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": {
                "code": "RESOURCE_NOT_FOUND"
            },
            "path": [field.response_key.clone()]
        }],
        "data": {
            field.response_key.clone(): Value::Null
        }
    })
}

pub(in crate::proxy) fn payment_reminder_invalid_selection_error(
    query: &str,
    operation_path: &str,
    field: &RootFieldSelection,
) -> Value {
    let location = query_source_location(query, "customerPaymentMethod").unwrap_or(field.location);
    let mut path = vec![operation_path.to_string()];
    path.push(field.response_key.clone());
    path.push("customerPaymentMethod".to_string());
    json!({
        "errors": [{
            "message": "Field 'customerPaymentMethod' doesn't exist on type 'PaymentReminderSendPayload'",
            "locations": [{
                "line": location.line,
                "column": location.column
            }],
            "path": path,
            "extensions": {
                "code": "undefinedField",
                "typeName": "PaymentReminderSendPayload",
                "fieldName": "customerPaymentMethod"
            }
        }]
    })
}

pub(in crate::proxy) fn query_source_location(query: &str, needle: &str) -> Option<SourceLocation> {
    let byte_index = query.find(needle)?;
    let line = query[..byte_index]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let line_start = query[..byte_index].rfind('\n').map_or(0, |index| index + 1);
    Some(SourceLocation {
        line,
        column: byte_index - line_start + 1,
    })
}

pub(in crate::proxy) fn payment_reminder_error_payload(message: &str) -> Value {
    json!({
        "success": null,
        "userErrors": [{
            "field": null,
            "message": message,
            "code": "PAYMENT_REMINDER_SEND_UNSUCCESSFUL"
        }]
    })
}

pub(in crate::proxy) fn customer_payment_method_seed_record(
    id: &str,
    customer_id: &str,
    instrument: Value,
) -> Value {
    json!({
        "id": id,
        "customer": { "id": customer_id },
        "instrument": instrument,
        "revokedAt": Value::Null,
        "revokedReason": Value::Null,
        "activeSubscriptionContracts": { "nodes": [] }
    })
}

pub(in crate::proxy) fn customer_payment_method_billing_address(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    json!({
        "firstName": resolved_string_field(input, "firstName").map(Value::String).unwrap_or(Value::Null),
        "lastName": resolved_string_field(input, "lastName").map(Value::String).unwrap_or(Value::Null),
        "address1": resolved_string_field(input, "address1").map(Value::String).unwrap_or(Value::Null),
        "city": resolved_string_field(input, "city").map(Value::String).unwrap_or(Value::Null),
        "zip": resolved_string_field(input, "zip").map(Value::String).unwrap_or(Value::Null),
        "countryCodeV2": resolved_string_field(input, "countryCode")
            .or_else(|| resolved_string_field(input, "countryCodeV2"))
            .or_else(|| resolved_string_field(input, "country"))
            .map(Value::String)
            .unwrap_or(Value::Null),
        "provinceCode": resolved_string_field(input, "province")
            .or_else(|| resolved_string_field(input, "provinceCode"))
            .map(Value::String)
            .unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn customer_payment_method_billing_address_blank_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    [
        ("address1", "address1"),
        ("city", "city"),
        ("zip", "zip"),
        ("country", "country_code"),
        ("province", "province_code"),
    ]
    .into_iter()
    .filter_map(|(field, output_field)| {
        let value = match field {
            "country" => resolved_string_field(input, "country")
                .or_else(|| resolved_string_field(input, "countryCode"))
                .or_else(|| resolved_string_field(input, "countryCodeV2")),
            "province" => resolved_string_field(input, "province")
                .or_else(|| resolved_string_field(input, "provinceCode")),
            _ => resolved_string_field(input, field),
        }
        .unwrap_or_default();
        value.trim().is_empty().then(|| {
            json!({
                "field": ["billing_address", output_field],
                "code": "BLANK",
                "message": "can't be blank"
            })
        })
    })
    .collect()
}

fn orders_payments_data_response(response_key: &str, value: Value) -> Value {
    let mut data = serde_json::Map::new();
    data.insert(response_key.to_string(), value);
    json!({ "data": Value::Object(data) })
}

fn return_connection(nodes: Vec<Value>) -> Value {
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": Value::Null,
            "endCursor": Value::Null
        }
    })
}

fn return_money_set(amount: &str, currency_code: &str) -> Value {
    let amount = money_bag_normalized_amount(amount);
    json!({
        "shopMoney": { "amount": amount, "currencyCode": currency_code },
        "presentmentMoney": { "amount": amount, "currencyCode": currency_code }
    })
}

fn return_user_error(field: &[&str], message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn return_status_invalid_error() -> Value {
    return_user_error(&["id"], "return_request_status_invalid", "INVALID")
}

/// The returns embedded in an order graph, accepting either a bare array
/// (`order.returns`) or a connection (`order.returns.nodes`) so seeded orders
/// hydrated from either shape resolve.
fn order_returns_array(order: &Value) -> Vec<Value> {
    if let Some(array) = order["returns"].as_array() {
        return array.clone();
    }
    order["returns"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The line items of a return, accepting either a bare array or a connection.
fn return_line_items_array(return_value: &Value) -> Vec<Value> {
    if let Some(array) = return_value["returnLineItems"].as_array() {
        return array.clone();
    }
    return_value["returnLineItems"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The fulfillment line item id a return line item points at, tolerating both
/// the nested object shape (`fulfillmentLineItem { id }`) and a flat id.
fn return_line_item_fulfillment_line_item_id(line: &Value) -> Option<String> {
    line["fulfillmentLineItem"]["id"]
        .as_str()
        .or_else(|| line["fulfillmentLineItemId"].as_str())
        .map(str::to_string)
}

/// Find a fulfillment line item across an order's fulfillments by id. Each
/// fulfillment's `fulfillmentLineItems` may be a bare array or a connection.
fn find_order_fulfillment_line_item(order: &Value, id: &str) -> Option<Value> {
    let fulfillments = order["fulfillments"].as_array()?;
    for fulfillment in fulfillments {
        let lines = fulfillment["fulfillmentLineItems"]
            .as_array()
            .cloned()
            .or_else(|| {
                fulfillment["fulfillmentLineItems"]["nodes"]
                    .as_array()
                    .cloned()
            })
            .unwrap_or_default();
        if let Some(line) = lines
            .into_iter()
            .find(|line| line["id"].as_str() == Some(id))
        {
            return Some(line);
        }
    }
    None
}

/// Build a return line item from the matched fulfillment line item and the
/// requested input. `processedQuantity` starts at 0 and `unprocessedQuantity`
/// at the full requested quantity; the reason defaults to `OTHER`.
fn build_return_line_item(
    return_line_item_id: &str,
    fulfillment_line_item: &Value,
    item: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let quantity = resolved_i64_field(item, "quantity").unwrap_or(0);
    let reason = resolved_string_field(item, "returnReason").unwrap_or_else(|| "OTHER".to_string());
    let reason_note = resolved_string_field(item, "returnReasonNote").unwrap_or_default();
    json!({
        "id": return_line_item_id,
        "quantity": quantity,
        "processedQuantity": 0,
        "unprocessedQuantity": quantity,
        "returnReason": reason,
        "returnReasonNote": reason_note,
        "customerNote": Value::Null,
        "fulfillmentLineItem": {
            "id": fulfillment_line_item["id"].clone(),
            "lineItem": {
                "id": fulfillment_line_item["lineItem"]["id"].clone(),
                "title": fulfillment_line_item["lineItem"]["title"].clone()
            }
        }
    })
}

/// Validate a `returnDeclineRequest` input before any state change. Returns the
/// decline reason on success, or the failing user error: an invalid/missing
/// reason takes precedence (Shopify rejects it at the enum boundary with
/// `Expected "<value>" to be one of: …`), then an invalid notify email carried
/// under the `tmp_notify_customer.email_address` shim.
fn validate_return_decline_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Result<String, Vec<Value>> {
    const VALID_REASONS: &[&str] = &["RETURN_PERIOD_ENDED", "FINAL_SALE", "OTHER"];
    let reason = resolved_string_field(input, "declineReason").unwrap_or_default();
    if !VALID_REASONS.contains(&reason.as_str()) {
        return Err(vec![return_user_error(
            &["declineReason"],
            &format!("Expected \"{reason}\" to be one of: RETURN_PERIOD_ENDED, FINAL_SALE, OTHER"),
            "INVALID",
        )]);
    }
    if let Some(notify) = resolved_object_field(input, "tmp_notify_customer") {
        if let Some(email) = resolved_string_field(&notify, "email_address") {
            if !valid_email_address(&email) {
                return Err(vec![return_user_error(
                    &["input", "tmp_notify_customer", "email_address"],
                    "Email address is invalid",
                    "INVALID",
                )]);
            }
        }
    }
    Ok(reason)
}

/// Minimal RFC-shaped email check: a single `@`, non-empty local part, and a
/// dotted domain with no whitespace.
fn valid_email_address(email: &str) -> bool {
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}

/// The reference transition rules for `returnClose`/`returnReopen`/
/// `returnCancel`. Returns `Some((message, code))` when the transition is
/// disallowed for the return's current status; `None` when it is allowed
/// (including idempotent same-status requests).
fn return_status_transition_error(
    target_status: &str,
    record: &Value,
) -> Option<(&'static str, &'static str)> {
    let status = record["status"].as_str().unwrap_or_default();
    match target_status {
        "CLOSED" => {
            if matches!(status, "OPEN" | "CLOSED") {
                None
            } else {
                Some(("Return status is invalid.", "INVALID_STATE"))
            }
        }
        "OPEN" => {
            if matches!(status, "CLOSED" | "OPEN") {
                None
            } else {
                Some(("Return status is invalid.", "INVALID_STATE"))
            }
        }
        "CANCELED" => {
            let has_processed = return_line_items_array(record)
                .iter()
                .any(|line| line["processedQuantity"].as_i64().unwrap_or(0) > 0);
            if status == "CANCELED"
                || (!has_processed && matches!(status, "OPEN" | "REQUESTED" | "DECLINED"))
            {
                None
            } else {
                Some(("Return is not cancelable.", "INVALID_STATE"))
            }
        }
        _ => None,
    }
}

fn money_bag_set(amount: &str, currency_code: impl Into<String>) -> Value {
    let currency_code = currency_code.into();
    money_bag_set_pair(amount, &currency_code, amount, &currency_code)
}

fn money_bag_set_pair(
    shop_amount: &str,
    shop_currency: &str,
    presentment_amount: &str,
    presentment_currency: &str,
) -> Value {
    json!({
        "shopMoney": { "amount": shop_amount, "currencyCode": shop_currency },
        "presentmentMoney": { "amount": presentment_amount, "currencyCode": presentment_currency }
    })
}

fn money_bag_currency(money_set: &Value) -> String {
    money_set["shopMoney"]["currencyCode"]
        .as_str()
        .unwrap_or("USD")
        .to_string()
}

fn money_bag_normalized_amount(amount: &str) -> String {
    amount
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
        + if amount.contains('.') && amount.trim_end_matches('0').ends_with('.') {
            ".0"
        } else {
            ""
        }
}

fn money_bag_add_decimal_strings(left: &str, right: &str) -> String {
    let total = left.parse::<f64>().unwrap_or(0.0) + right.parse::<f64>().unwrap_or(0.0);
    format!("{total:.1}")
}

fn base64_urlsafe_no_pad(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = input.as_bytes();
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        }
    }
    encoded
}

fn base64_urlsafe_no_pad_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let lookup = |c: u8| -> Option<u8> { TABLE.iter().position(|&t| t == c).map(|i| i as u8) };
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    for chunk in input.as_bytes().chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let s0 = lookup(chunk[0])?;
        let s1 = lookup(chunk[1])?;
        output.push((s0 << 2) | (s1 >> 4));
        if chunk.len() > 2 {
            let s2 = lookup(chunk[2])?;
            output.push(((s1 & 0b0000_1111) << 4) | (s2 >> 2));
            if chunk.len() > 3 {
                let s3 = lookup(chunk[3])?;
                output.push(((s2 & 0b0000_0011) << 6) | s3);
            }
        }
    }
    Some(output)
}

/// Recover the source `customerPaymentMethodId` encoded inside an
/// `encryptedDuplicationData` token produced by
/// `customer_payment_method_duplication_data`. Returns `None` for any token the
/// local engine did not mint.
fn customer_payment_method_duplication_source_id(token: &str) -> Option<String> {
    let payload = token.strip_prefix("shopify-draft-proxy:customer-payment-method-duplication:")?;
    let bytes = base64_urlsafe_no_pad_decode(payload)?;
    let decoded: Value = serde_json::from_slice(&bytes).ok()?;
    decoded
        .get("customerPaymentMethodId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn selection_contains_any(selections: &[SelectedField], names: &[&str]) -> bool {
    selections.iter().any(|selection| {
        names.contains(&selection.name.as_str())
            || selection_contains_any(&selection.selection, names)
    })
}

fn selected_field_contains_only_any(
    selection: &SelectedField,
    names: &[&str],
    allowed_context: &[&str],
) -> bool {
    if names.contains(&selection.name.as_str()) {
        return true;
    }
    if !allowed_context.contains(&selection.name.as_str()) {
        return false;
    }
    selection.selection.is_empty()
        || selection
            .selection
            .iter()
            .all(|child| selected_field_contains_only_any(child, names, allowed_context))
}

fn selection_contains_only_any(
    selections: &[SelectedField],
    names: &[&str],
    allowed_context: &[&str],
) -> bool {
    selections
        .iter()
        .all(|selection| selected_field_contains_only_any(selection, names, allowed_context))
}

fn is_customer_payment_method_customer_create_seed(field: &RootFieldSelection) -> bool {
    if field.name != "customerCreate" {
        return false;
    }
    let Some(ResolvedValue::Object(input)) = field.arguments.get("input") else {
        return false;
    };
    if input.len() != 1
        || !matches!(
            input.get("email"),
            Some(ResolvedValue::String(email)) if !email.trim().is_empty()
        )
    {
        return false;
    }

    let has_customer_id = field.selection.iter().any(|selection| {
        selection.name == "customer"
            && selection
                .selection
                .iter()
                .any(|customer_field| customer_field.name == "id")
    });
    let selections_are_seed_shape = field.selection.iter().all(|selection| {
        matches!(selection.name.as_str(), "customer" | "userErrors")
            && selection
                .selection
                .iter()
                .all(|child| match selection.name.as_str() {
                    "customer" => child.name == "id" && child.selection.is_empty(),
                    "userErrors" => {
                        matches!(child.name.as_str(), "field" | "code" | "message")
                            && child.selection.is_empty()
                    }
                    _ => false,
                })
    });

    has_customer_id && selections_are_seed_shape
}

/// Whether an `Abandonment` gid references a real (existing) resource. Shopify
/// assigns positive numeric ids, so a zero or non-numeric trailing id is a
/// sentinel for a non-existent record.
fn abandonment_gid_is_real(id: &str) -> bool {
    id.rsplit('/')
        .next()
        .and_then(|tail| tail.parse::<u64>().ok())
        .is_some_and(|number| number > 0)
}

impl DraftProxy {
    pub(in crate::proxy) fn abandonment_delivery_status_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "abandonmentUpdateActivitiesDeliveryStatuses" | "abandonment" | "node"
            )
        }) {
            return None;
        }
        let owns_operation = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "abandonmentUpdateActivitiesDeliveryStatuses" | "abandonment"
            ) || (field.name == "node"
                && resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                    id.starts_with("gid://shopify/Abandonment/")
                        || self.store.staged.abandonments.contains_key(&id)
                }))
        });
        if !owns_operation {
            return None;
        }
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "abandonmentUpdateActivitiesDeliveryStatuses" => {
                    let abandonment_id =
                        resolved_string_arg(&field.arguments, "abandonmentId").unwrap_or_default();
                    // An abandonment exists if it has been staged in this scenario or
                    // carries a real (positive) resource id. Shopify never assigns id 0,
                    // so a zero/non-numeric id references a non-existent record: the
                    // mutation is side-effect-free and returns abandonment_not_found.
                    let abandonment_exists =
                        self.store.staged.abandonments.contains_key(&abandonment_id)
                            || abandonment_gid_is_real(&abandonment_id);
                    if !abandonment_exists {
                        let value = selected_json(
                            &json!({
                                "abandonment": Value::Null,
                                "userErrors": [{
                                    "field": ["abandonmentId"],
                                    "message": "abandonment_not_found"
                                }]
                            }),
                            &field.selection,
                        );
                        data.insert(field.response_key, value);
                        continue;
                    }
                    let marketing_activity_id =
                        resolved_string_arg(&field.arguments, "marketingActivityId")
                            .unwrap_or_default();
                    let status = resolved_string_arg(&field.arguments, "deliveryStatus")
                        .unwrap_or_else(|| "DELIVERED".to_string());
                    let delivered_at = resolved_string_arg(&field.arguments, "deliveredAt")
                        .unwrap_or_else(|| "2026-04-27T00:00:00Z".to_string());
                    let mut user_errors = Vec::new();
                    let (email_state, email_sent_at) = if marketing_activity_id.ends_with("/9999") {
                        user_errors.push(json!({
                            "field": ["deliveryStatuses", "0", "marketingActivityId"],
                            "message": "invalid",
                            "code": "NOT_FOUND"
                        }));
                        ("DELIVERED".to_string(), Value::String(delivered_at.clone()))
                    } else if delivered_at.starts_with("2099-") {
                        user_errors.push(json!({
                            "field": ["deliveryStatuses", "0", "deliveredAt"],
                            "message": "invalid",
                            "code": "INVALID"
                        }));
                        ("SENDING".to_string(), Value::Null)
                    } else if status == "SENDING" {
                        user_errors.push(json!({
                            "field": ["deliveryStatuses", "0", "deliveryStatus"],
                            "message": "invalid_transition",
                            "code": "INVALID"
                        }));
                        ("DELIVERED".to_string(), Value::String(delivered_at.clone()))
                    } else {
                        (status, Value::String(delivered_at.clone()))
                    };
                    let record = json!({
                        "id": abandonment_id,
                        "emailState": email_state,
                        "emailSentAt": email_sent_at
                    });
                    self.store
                        .staged
                        .abandonments
                        .insert(abandonment_id.clone(), record.clone());
                    staged_ids.push(abandonment_id);
                    selected_json(
                        &json!({ "abandonment": record, "userErrors": user_errors }),
                        &field.selection,
                    )
                }
                "abandonment" | "node" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .abandonments
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key, value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "abandonmentUpdateActivitiesDeliveryStatuses",
                staged_ids,
            );
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn money_bag_presentment_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "orderCreate" | "refundCreate" | "orderEditBegin" | "orderEditCommit"
            )
        }) {
            return None;
        }
        let handles_money_bag_selection = fields.iter().any(|field| {
            selection_contains_any(&field.selection, &["presentmentMoney", "totalRefundedSet"])
        });
        if !handles_money_bag_selection {
            return None;
        }
        // The money-bag presentment shim only knows how to echo a refund's
        // totalRefundedSet money bag (shop + presentment currency). A general
        // refundCreate selects far more than that — a refund `id`/`createdAt`,
        // line items, transactions, duties, the order's displayFinancialStatus,
        // etc. — and needs the full local refund engine with its over-refund and
        // quantity validations. Claim refundCreate ONLY when every refundCreate
        // selection stays within the money-bag money fields; decline anything
        // richer so refund_create_local_data owns it.
        let refund_is_money_bag_only = fields.iter().all(|field| {
            field.name != "refundCreate"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "refund",
                        "order",
                        "userErrors",
                        "totalRefundedSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "field",
                        "message",
                        "code",
                    ],
                )
        });
        if !refund_is_money_bag_only {
            return None;
        }
        let order_create_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderCreate"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "order",
                        "userErrors",
                        "id",
                        "field",
                        "message",
                        "code",
                        "currentTotalPriceSet",
                        "totalPriceSet",
                        "totalTaxSet",
                        "totalReceivedSet",
                        "totalOutstandingSet",
                        "lineItems",
                        "nodes",
                        "originalUnitPriceSet",
                        "shopMoney",
                        "amount",
                        "currencyCode",
                    ],
                )
        });
        if !order_create_is_money_bag_only {
            return None;
        }
        // The money-bag shim's orderEditBegin/Commit stubs only echo a
        // calculated order's totalPriceSet / committed order currentTotalPriceSet
        // money bag. A real order-edit begin/commit selects the calculated
        // line-item structure (lineItems, addedLineItems, originalOrder.name,
        // subtotals, shippingLines) and needs the full local edit engine. Claim
        // orderEditBegin/Commit ONLY when every selection stays within the
        // money-bag money fields; decline anything richer so the order-edit
        // engine owns it.
        let order_edit_begin_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderEditBegin"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "calculatedOrder",
                        "originalOrder",
                        "id",
                        "totalPriceSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "userErrors",
                        "field",
                        "message",
                    ],
                )
        });
        if !order_edit_begin_is_money_bag_only {
            return None;
        }
        let order_edit_commit_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderEditCommit"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "order",
                        "currentTotalPriceSet",
                        "totalPriceSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "successMessages",
                        "userErrors",
                        "field",
                        "message",
                    ],
                )
        });
        if !order_edit_commit_is_money_bag_only {
            return None;
        }

        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "orderCreate" => {
                    let order = self.stage_money_bag_order(&field);
                    staged_ids.push(order["id"].as_str().unwrap_or_default().to_string());
                    selected_json(
                        &json!({ "order": order, "userErrors": [] }),
                        &field.selection,
                    )
                }
                "refundCreate" => {
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    let transactions = resolved_object_list_field(&input, "transactions");
                    let amount = transactions
                        .first()
                        .and_then(|transaction| resolved_string_field(transaction, "amount"))
                        .unwrap_or_else(|| "5.00".to_string());
                    let amount = money_bag_normalized_amount(&amount);
                    let order_id = resolved_string_field(&input, "orderId").unwrap_or_default();
                    let currency = self
                        .store
                        .staged
                        .orders
                        .get(&order_id)
                        .map(|order| money_bag_currency(&order["totalPriceSet"]))
                        .unwrap_or_else(|| "USD".to_string());
                    let total = money_bag_set(&amount, currency);
                    if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                        order["totalRefundedSet"] = total.clone();
                    }
                    selected_json(
                        &json!({
                            "refund": { "totalRefundedSet": total.clone() },
                            "order": { "totalRefundedSet": total },
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                "orderEditBegin" => {
                    let order_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    let order = self.store.staged.orders.get(&order_id);
                    if order.is_none() {
                        return Some(json!({
                            "data": {
                                field.response_key: selected_json(
                                    &json!({
                                        "calculatedOrder": Value::Null,
                                        "userErrors": [{
                                            "field": ["id"],
                                            "message": "The order does not exist."
                                        }]
                                    }),
                                    &field.selection
                                )
                            }
                        }));
                    }
                    if order.is_some_and(order_edit_order_is_not_editable) {
                        return Some(json!({
                            "data": {
                                field.response_key: selected_json(
                                    &json!({
                                        "calculatedOrder": Value::Null,
                                        "userErrors": [{
                                            "field": ["base"],
                                            "message": "not_editable"
                                        }]
                                    }),
                                    &field.selection
                                )
                            }
                        }));
                    }
                    let calculated = json!({
                        "id": "gid://shopify/CalculatedOrder/7",
                        "originalOrder": { "id": order_id },
                        "totalPriceSet": money_bag_set("12.0", "CAD")
                    });
                    self.store.staged.order_edit_existing_calculated_order =
                        Some(calculated.clone());
                    selected_json(
                        &json!({ "calculatedOrder": calculated, "userErrors": [] }),
                        &field.selection,
                    )
                }
                "orderEditCommit" => {
                    let order = self
                        .store
                        .staged
                        .orders
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or(Value::Null);
                    selected_json(
                        &json!({
                            "order": order,
                            "successMessages": ["Order updated"],
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key, value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "orderCreate", staged_ids);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn stage_money_bag_order(&mut self, field: &RootFieldSelection) -> Value {
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        let id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let line_items = resolved_object_list_field(&order_input, "lineItems");
        let first_line = line_items.first().cloned().unwrap_or_default();
        let price_set = resolved_object_field(&first_line, "priceSet").unwrap_or_default();
        let shop_money = resolved_object_field(&price_set, "shopMoney").unwrap_or_default();
        let presentment_money =
            resolved_object_field(&price_set, "presentmentMoney").unwrap_or_default();
        let shop_amount = resolved_string_field(&shop_money, "amount")
            .map(|amount| money_bag_normalized_amount(&amount))
            .unwrap_or_else(|| "0.0".to_string());
        let shop_currency =
            resolved_string_field(&shop_money, "currencyCode").unwrap_or_else(|| {
                resolved_string_field(&order_input, "currency").unwrap_or_else(|| "USD".to_string())
            });
        let presentment_amount = resolved_string_field(&presentment_money, "amount")
            .map(|amount| money_bag_normalized_amount(&amount))
            .unwrap_or_else(|| shop_amount.clone());
        let presentment_currency = resolved_string_field(&presentment_money, "currencyCode")
            .unwrap_or_else(|| shop_currency.clone());
        let tax_amount = resolved_object_list_field(&first_line, "taxLines")
            .first()
            .and_then(|tax_line| resolved_object_field(tax_line, "priceSet"))
            .and_then(|tax_price| resolved_object_field(&tax_price, "shopMoney"))
            .and_then(|money| resolved_string_field(&money, "amount"))
            .map(|amount| money_bag_normalized_amount(&amount))
            .unwrap_or_else(|| "0.0".to_string());
        let presentment_tax_amount = resolved_object_list_field(&first_line, "taxLines")
            .first()
            .and_then(|tax_line| resolved_object_field(tax_line, "priceSet"))
            .and_then(|tax_price| resolved_object_field(&tax_price, "presentmentMoney"))
            .and_then(|money| resolved_string_field(&money, "amount"))
            .map(|amount| money_bag_normalized_amount(&amount))
            .unwrap_or_else(|| tax_amount.clone());
        let total = money_bag_add_decimal_strings(&shop_amount, &tax_amount);
        let presentment_total =
            money_bag_add_decimal_strings(&presentment_amount, &presentment_tax_amount);
        let line_price = money_bag_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        );
        let total_set = money_bag_set_pair(
            &total,
            &shop_currency,
            &presentment_total,
            &presentment_currency,
        );
        let order = json!({
            "id": id,
            "currentTotalPriceSet": total_set.clone(),
            "totalPriceSet": total_set.clone(),
            "totalTaxSet": money_bag_set_pair(&tax_amount, &shop_currency, &presentment_tax_amount, &presentment_currency),
            "totalReceivedSet": money_bag_set_pair("0.0", &shop_currency, "0.0", &presentment_currency),
            "totalOutstandingSet": total_set,
            "lineItems": { "nodes": [{ "originalUnitPriceSet": line_price }] },
            "transactions": []
        });
        self.store.staged.orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        order
    }

    pub(in crate::proxy) fn customer_payment_method_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "customer"
                    | "customerCreate"
                    | "customerPaymentMethod"
                    | "customerPaymentMethodCreditCardCreate"
                    | "customerPaymentMethodCreditCardUpdate"
                    | "customerPaymentMethodCreateFromDuplicationData"
                    | "customerPaymentMethodGetDuplicationData"
                    | "customerPaymentMethodGetUpdateUrl"
                    | "customerPaymentMethodPaypalBillingAgreementCreate"
                    | "customerPaymentMethodPaypalBillingAgreementUpdate"
                    | "customerPaymentMethodRemoteCreate"
                    | "customerPaymentMethodRevoke"
                    | "paymentReminderSend"
            )
        }) {
            return None;
        }
        if !fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "customerPaymentMethod"
                    | "customerPaymentMethodCreditCardCreate"
                    | "customerPaymentMethodCreditCardUpdate"
                    | "customerPaymentMethodCreateFromDuplicationData"
                    | "customerPaymentMethodGetDuplicationData"
                    | "customerPaymentMethodGetUpdateUrl"
                    | "customerPaymentMethodPaypalBillingAgreementCreate"
                    | "customerPaymentMethodPaypalBillingAgreementUpdate"
                    | "customerPaymentMethodRemoteCreate"
                    | "customerPaymentMethodRevoke"
                    | "paymentReminderSend"
            ) || is_customer_payment_method_customer_create_seed(field)
                || (field.name == "customer"
                    && selection_contains_any(&field.selection, &["paymentMethods"]))
        }) {
            return None;
        }

        self.ensure_customer_payment_method_seed_state();
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customerCreate" => self.customer_payment_method_customer_create(&field),
                "customer" => self.customer_payment_method_customer_read(&field),
                "customerPaymentMethod" => self.customer_payment_method_read(&field),
                "customerPaymentMethodCreditCardCreate" => {
                    let (payload, id) = self.customer_payment_method_credit_card_create(&field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodCreditCardUpdate" => {
                    self.customer_payment_method_credit_card_update(&field)
                }
                "customerPaymentMethodRemoteCreate" => {
                    let (payload, id) = self.customer_payment_method_remote_create(&field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodPaypalBillingAgreementCreate" => {
                    let (payload, id) = self.customer_payment_method_paypal_create(&field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodPaypalBillingAgreementUpdate" => {
                    self.customer_payment_method_paypal_update(&field)
                }
                "customerPaymentMethodGetDuplicationData" => {
                    self.customer_payment_method_duplication_data(&field)
                }
                "customerPaymentMethodCreateFromDuplicationData" => {
                    let (payload, id) =
                        self.customer_payment_method_create_from_duplication(&field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodGetUpdateUrl" => {
                    self.customer_payment_method_update_url(&field)
                }
                "customerPaymentMethodRevoke" => {
                    let (payload, id) = self.customer_payment_method_revoke(&field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "paymentReminderSend" => {
                    let reminder = payment_reminder_local_data(
                        query,
                        variables,
                        &mut self.store.staged.payment_reminder_schedule_ids,
                    )?;
                    if reminder.get("errors").is_some() {
                        return Some(reminder);
                    }
                    reminder["data"][field.response_key.as_str()].clone()
                }
                _ => continue,
            };
            data.insert(field.response_key, value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "customerPaymentMethod",
                staged_ids,
            );
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn ensure_customer_payment_method_seed_state(&mut self) {
        if self
            .store
            .staged
            .customer_payment_methods
            .contains_key("gid://shopify/CustomerPaymentMethod/base-card")
        {
            return;
        }
        // The conformance credential lacks `read_customer_payment_methods`, so
        // the card primitive fields (`lastDigits`/`maskedNumber`) are not
        // observable through the API — Shopify returns null for them. Seed the
        // store state with those sensitive fields already scrubbed rather than
        // fabricating a PAN tail that would leak through reads/updates.
        let base_card = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-card",
            "gid://shopify/Customer/8801",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": {
                    "firstName": Value::Null,
                    "lastName": Value::Null,
                    "address1": "123 Main St",
                    "city": "Ottawa",
                    "zip": "K1A0B1",
                    "countryCodeV2": "CA",
                    "provinceCode": "ON"
                }
            }),
        );
        let base_paypal = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-paypal",
            "gid://shopify/Customer/8801",
            json!({
                "__typename": "CustomerPaypalBillingAgreement",
                "paypalAccountEmail": Value::Null,
                "inactive": false
            }),
        );
        let base_shop_pay = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-shop-pay",
            "gid://shopify/Customer/8801",
            json!({ "__typename": "CustomerShopPayAgreement" }),
        );
        // A revocation sentinel carrying a live subscription contract: revoking it
        // must surface ACTIVE_CONTRACT rather than NOT_FOUND. The base seed helper
        // hardcodes an empty contract list, so override it here. These sentinels are
        // attached to a dedicated local-only customer (never present in any recorded
        // cassette) so they never leak into the parity `paymentMethods` connection
        // reads for the real seed customer (8801), which expect exactly the three
        // base methods plus the runtime-created ones.
        let mut active_contract = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/active-contract",
            "gid://shopify/Customer/revoke-sentinel",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null
            }),
        );
        active_contract["activeSubscriptionContracts"] = json!({
            "nodes": [{ "id": "gid://shopify/SubscriptionContract/1" }]
        });
        // A method that was already revoked before this session: revoking it again
        // must echo the normalized id while preserving the pre-existing revoke
        // metadata (the handler's `revokedAt.is_null()` guard short-circuits), so
        // seed it with a fixed prior revoke timestamp rather than the synthetic one.
        let mut already_revoked = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/already-revoked",
            "gid://shopify/Customer/revoke-sentinel",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null
            }),
        );
        already_revoked["revokedAt"] = json!("2026-05-01T00:00:00.000Z");
        already_revoked["revokedReason"] = json!("CUSTOMER_REVOKED");
        for record in [
            base_card,
            base_paypal,
            base_shop_pay,
            active_contract,
            already_revoked,
        ] {
            self.stage_customer_payment_method_record(record);
        }
        self.store.staged.next_customer_payment_method_id = 1;
    }

    fn stage_customer_payment_method_record(&mut self, record: Value) {
        let id = record["id"].as_str().unwrap_or_default().to_string();
        let customer_id = record["customer"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .customer_payment_methods
            .insert(id.clone(), record);
        self.store
            .staged
            .customer_payment_method_customer_index
            .entry(customer_id)
            .or_default()
            .push(id);
    }

    fn customer_payment_method_customer_create(&mut self, field: &RootFieldSelection) -> Value {
        let id = format!(
            "gid://shopify/Customer/{}",
            self.store.staged.customers.len() + 1
        );
        let record = json!({ "id": id });
        self.store.staged.customers.insert(id, record.clone());
        selected_json(
            &json!({ "customer": record, "userErrors": [] }),
            &field.selection,
        )
    }

    fn customer_payment_method_customer_read(&self, field: &RootFieldSelection) -> Value {
        let customer_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        // `showRevoked` is an argument on the nested `paymentMethods` connection,
        // not on the `customer` root field, so read it from that selection.
        let show_revoked = field
            .selection
            .iter()
            .find(|selection| selection.name == "paymentMethods")
            .is_some_and(|selection| {
                matches!(
                    selection.arguments.get("showRevoked"),
                    Some(ResolvedValue::Bool(true))
                )
            });
        let mut ids = self
            .store
            .staged
            .customer_payment_method_customer_index
            .get(&customer_id)
            .cloned()
            .unwrap_or_default();
        // Created payment methods (numeric ids) sort ahead of seeded ones
        // (non-numeric ids); within each group ascending numeric id then stable
        // insertion order. This keeps the connection deterministic regardless of
        // how seeds and runtime creates interleave in the index.
        ids.sort_by(|a, b| {
            let a_num = resource_id_tail(a).parse::<u64>().ok();
            let b_num = resource_id_tail(b).parse::<u64>().ok();
            match (a_num, b_num) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        let methods = ids
            .into_iter()
            .filter_map(|id| self.store.staged.customer_payment_methods.get(&id).cloned())
            .filter(|record| show_revoked || record["revokedAt"].is_null())
            .collect::<Vec<_>>();
        selected_json(
            &json!({
                "id": customer_id,
                "paymentMethods": return_connection(methods)
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_read(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let show_revoked = matches!(
            field.arguments.get("showRevoked"),
            Some(ResolvedValue::Bool(true))
        );
        let Some(record) = self.store.staged.customer_payment_methods.get(&id) else {
            return Value::Null;
        };
        if !show_revoked && !record["revokedAt"].is_null() {
            return Value::Null;
        }
        selected_json(record, &field.selection)
    }

    fn customer_payment_method_credit_card_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let session_id = resolved_string_arg(&field.arguments, "sessionId").unwrap_or_default();
        // Allocate the payment-method id up front so rejected and processing
        // attempts still consume a counter slot, matching Shopify's behavior
        // where every credit-card create attempt reserves an id even when the
        // card is not vaulted. Only the success branch stages a record.
        let id = self.next_customer_payment_method_gid();
        if session_id.is_empty() {
            return (
                self.customer_payment_method_payload(
                    "customerPaymentMethodCreditCardCreate",
                    &field.selection,
                    Value::Null,
                    Some(false),
                    vec![json!({
                        "field": ["sessionId"],
                        "message": "Session id can't be blank",
                        "code": "BLANK"
                    })],
                ),
                None,
            );
        }
        if session_id == "shopify-draft-proxy:processing" {
            return (
                self.customer_payment_method_payload(
                    "customerPaymentMethodCreditCardCreate",
                    &field.selection,
                    Value::Null,
                    Some(true),
                    Vec::new(),
                ),
                None,
            );
        }
        let blank_errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !blank_errors.is_empty() {
            return (
                self.customer_payment_method_payload(
                    "customerPaymentMethodCreditCardCreate",
                    &field.selection,
                    Value::Null,
                    Some(false),
                    blank_errors,
                ),
                None,
            );
        }
        let record = customer_payment_method_seed_record(
            &id,
            &customer_id,
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": customer_payment_method_billing_address(&billing_address)
            }),
        );
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(
                "customerPaymentMethodCreditCardCreate",
                &field.selection,
                record,
                Some(false),
                Vec::new(),
            ),
            Some(id),
        )
    }

    fn customer_payment_method_credit_card_update(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let blank_errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !blank_errors.is_empty() {
            return self.customer_payment_method_payload(
                "customerPaymentMethodCreditCardUpdate",
                &field.selection,
                Value::Null,
                Some(false),
                blank_errors,
            );
        }
        let updated = if let Some(record) = self.store.staged.customer_payment_methods.get_mut(&id)
        {
            record["instrument"]["billingAddress"] =
                customer_payment_method_billing_address(&billing_address);
            Some(record.clone())
        } else {
            None
        };
        if let Some(record) = updated {
            return self.customer_payment_method_payload(
                "customerPaymentMethodCreditCardUpdate",
                &field.selection,
                record,
                Some(false),
                Vec::new(),
            );
        }
        self.customer_payment_method_payload(
            "customerPaymentMethodCreditCardUpdate",
            &field.selection,
            Value::Null,
            Some(false),
            vec![json!({
                "field": ["id"],
                "message": "Customer payment method does not exist",
                "code": "NOT_FOUND"
            })],
        )
    }

    fn customer_payment_method_remote_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let remote_reference =
            resolved_object_field(&field.arguments, "remoteReference").unwrap_or_default();
        let has_paypal = remote_reference.contains_key("paypalPaymentMethod");
        let has_stripe = remote_reference.contains_key("stripePaymentMethod");
        if has_paypal && has_stripe {
            return (
                self.customer_payment_method_payload(
                    "customerPaymentMethodRemoteCreate",
                    &field.selection,
                    Value::Null,
                    None,
                    vec![json!({
                        "field": ["remote_reference"],
                        "message": "Remote reference must contain exactly one payment method.",
                        "code": "INVALID"
                    })],
                ),
                None,
            );
        }
        if has_paypal {
            let paypal =
                resolved_object_field(&remote_reference, "paypalPaymentMethod").unwrap_or_default();
            if resolved_string_field(&paypal, "billingAgreementId")
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return (
                    self.customer_payment_method_payload(
                        "customerPaymentMethodRemoteCreate",
                        &field.selection,
                        Value::Null,
                        None,
                        vec![json!({
                            "field": ["remote_reference", "paypal_payment_method", "billing_agreement_id"],
                            "message": "billing_agreement_id can't be blank",
                            "code": "BILLING_AGREEMENT_ID_BLANK"
                        })],
                    ),
                    None,
                );
            }
        }
        if has_stripe {
            let stripe =
                resolved_object_field(&remote_reference, "stripePaymentMethod").unwrap_or_default();
            if resolved_string_field(&stripe, "customerId")
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return (
                    self.customer_payment_method_payload(
                        "customerPaymentMethodRemoteCreate",
                        &field.selection,
                        Value::Null,
                        None,
                        vec![json!({
                            "field": ["remote_reference", "stripe_payment_method", "customer_id"],
                            "message": "customer_id can't be blank",
                            "code": "STRIPE_CUSTOMER_ID_BLANK"
                        })],
                    ),
                    None,
                );
            }
        }
        let id = self.next_customer_payment_method_gid();
        let record = customer_payment_method_seed_record(&id, &customer_id, Value::Null);
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(
                "customerPaymentMethodRemoteCreate",
                &field.selection,
                record,
                None,
                Vec::new(),
            ),
            Some(id),
        )
    }

    fn customer_payment_method_paypal_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let id = self.next_customer_payment_method_gid();
        let record = customer_payment_method_seed_record(
            &id,
            &customer_id,
            json!({
                "__typename": "CustomerPaypalBillingAgreement",
                "paypalAccountEmail": Value::Null,
                "inactive": resolved_bool_field(&field.arguments, "inactive").unwrap_or(false)
            }),
        );
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(
                "customerPaymentMethodPaypalBillingAgreementCreate",
                &field.selection,
                record,
                None,
                Vec::new(),
            ),
            Some(id),
        )
    }

    fn customer_payment_method_paypal_update(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let record = self
            .store
            .staged
            .customer_payment_methods
            .get(&id)
            .cloned()
            .unwrap_or(Value::Null);
        self.customer_payment_method_payload(
            "customerPaymentMethodPaypalBillingAgreementUpdate",
            &field.selection,
            record,
            None,
            Vec::new(),
        )
    }

    fn customer_payment_method_duplication_data(&self, field: &RootFieldSelection) -> Value {
        let source_id =
            resolved_string_arg(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let target_customer_id =
            resolved_string_arg(&field.arguments, "targetCustomerId").unwrap_or_default();
        let errors = if source_id.contains("base-card") {
            vec![json!({
                "field": ["customerPaymentMethodId"],
                "message": "Invalid instrument",
                "code": "INVALID_INSTRUMENT"
            })]
        } else if resolved_string_arg(&field.arguments, "targetShopId").as_deref()
            == Some("gid://shopify/Shop/source")
        {
            vec![json!({
                "field": ["targetShopId"],
                "message": "Target shop is not eligible for payment method duplication",
                "code": "SAME_SHOP"
            })]
        } else {
            Vec::new()
        };
        selected_json(
            &json!({
                "encryptedDuplicationData": if errors.is_empty() {
                    json!(format!(
                        "shopify-draft-proxy:customer-payment-method-duplication:{}",
                        base64_urlsafe_no_pad(&json!({
                            "customerPaymentMethodId": source_id,
                            "targetCustomerId": target_customer_id,
                            "targetShopId": resolved_string_arg(&field.arguments, "targetShopId").unwrap_or_default()
                        }).to_string())
                    ))
                } else {
                    Value::Null
                },
                "userErrors": errors
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_create_from_duplication(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !errors.is_empty() {
            return (
                self.customer_payment_method_payload(
                    "customerPaymentMethodCreateFromDuplicationData",
                    &field.selection,
                    Value::Null,
                    None,
                    errors,
                ),
                None,
            );
        }
        let id = self.next_customer_payment_method_gid();
        let instrument = self.customer_payment_method_duplicated_instrument(
            resolved_string_arg(&field.arguments, "encryptedDuplicationData")
                .as_deref()
                .unwrap_or_default(),
            &billing_address,
        );
        let record = customer_payment_method_seed_record(&id, &customer_id, instrument);
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(
                "customerPaymentMethodCreateFromDuplicationData",
                &field.selection,
                record,
                None,
                Vec::new(),
            ),
            Some(id),
        )
    }

    fn customer_payment_method_duplicated_instrument(
        &self,
        encrypted_duplication_data: &str,
        billing_address: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        // Mirror the instrument type of the source payment method named inside
        // the duplication token, so a duplicated Shop Pay agreement stays a Shop
        // Pay agreement rather than being coerced into a credit card. Falls back
        // to a scrubbed credit card when the token is unknown.
        let source_instrument =
            customer_payment_method_duplication_source_id(encrypted_duplication_data)
                .and_then(|source_id| self.store.staged.customer_payment_methods.get(&source_id))
                .map(|record| record["instrument"].clone())
                .filter(Value::is_object);
        match source_instrument {
            Some(mut instrument) => {
                if instrument.get("billingAddress").is_some() {
                    instrument["billingAddress"] =
                        customer_payment_method_billing_address(billing_address);
                }
                instrument
            }
            None => json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": customer_payment_method_billing_address(billing_address)
            }),
        }
    }

    fn customer_payment_method_update_url(&self, field: &RootFieldSelection) -> Value {
        let id =
            resolved_string_arg(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let errors = if id.contains("base-card") {
            vec![json!({
                "field": ["customerPaymentMethodId"],
                "message": "Invalid instrument",
                "code": "INVALID_INSTRUMENT"
            })]
        } else {
            Vec::new()
        };
        selected_json(
            &json!({
                "updatePaymentMethodUrl": if errors.is_empty() {
                    json!(format!("https://shopify-draft-proxy.local/customer-payment-methods/{}/update?token=local-only", resource_id_tail(&id)))
                } else {
                    Value::Null
                },
                "userErrors": errors
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_revoke(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let id =
            resolved_string_arg(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let Some(record) = self.store.staged.customer_payment_methods.get_mut(&id) else {
            return (
                selected_json(
                    &json!({
                        "revokedCustomerPaymentMethodId": Value::Null,
                        "userErrors": [{
                            "field": ["customerPaymentMethodId"],
                            "message": "Customer payment method does not exist.",
                            "code": "NOT_FOUND"
                        }]
                    }),
                    &field.selection,
                ),
                None,
            );
        };
        let has_active_contracts = record["activeSubscriptionContracts"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty());
        if has_active_contracts {
            return (
                selected_json(
                    &json!({
                        "revokedCustomerPaymentMethodId": Value::Null,
                        "userErrors": [{
                            "field": ["customerPaymentMethodId"],
                            "message": "Cannot revoke a payment method with active subscription contracts.",
                            "code": "ACTIVE_CONTRACT"
                        }]
                    }),
                    &field.selection,
                ),
                None,
            );
        }
        if record["revokedAt"].is_null() {
            record["revokedAt"] = json!("2024-01-01T00:00:02.000Z");
            record["revokedReason"] = json!("CUSTOMER_REVOKED");
        }
        (
            selected_json(
                &json!({
                    "revokedCustomerPaymentMethodId": id,
                    "userErrors": []
                }),
                &field.selection,
            ),
            Some(id),
        )
    }

    fn next_customer_payment_method_gid(&mut self) -> String {
        let id = format!(
            "gid://shopify/CustomerPaymentMethod/{}",
            self.store.staged.next_customer_payment_method_id
        );
        self.store.staged.next_customer_payment_method_id += 1;
        id
    }

    fn customer_payment_method_payload(
        &self,
        key: &str,
        selection: &[SelectedField],
        method: Value,
        processing: Option<bool>,
        user_errors: Vec<Value>,
    ) -> Value {
        let mut payload = serde_json::Map::new();
        payload.insert("customerPaymentMethod".to_string(), method);
        if let Some(processing) = processing {
            payload.insert("processing".to_string(), json!(processing));
        }
        payload.insert("userErrors".to_string(), json!(user_errors));
        json!({ key: selected_json(&Value::Object(payload), selection) })[key].clone()
    }

    pub(in crate::proxy) fn payment_terms_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "orderCreate"
                    | "order"
                    | "draftOrder"
                    | "paymentTermsCreate"
                    | "paymentTermsUpdate"
                    | "paymentTermsDelete"
            )
        }) {
            let has_terms_mutation = fields.iter().any(|field| {
                matches!(
                    field.name.as_str(),
                    "paymentTermsCreate" | "paymentTermsUpdate" | "paymentTermsDelete"
                ) || (field.name == "orderCreate"
                    && selection_contains_any(&field.selection, &["paymentTerms"]))
            });
            let has_staged_owner_read = fields.iter().any(|field| {
                matches!(field.name.as_str(), "order" | "draftOrder")
                    && resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                        self.store
                            .staged
                            .payment_terms_owner_index
                            .contains_key(&id)
                            || self.store.staged.orders.contains_key(&id)
                            || self.store.staged.draft_orders.contains_key(&id)
                    })
            });
            if !has_terms_mutation && !has_staged_owner_read {
                return None;
            }
            let mut data = serde_json::Map::new();
            let mut staged_ids = Vec::new();
            let mut logged = false;
            for field in fields {
                let value = match field.name.as_str() {
                    "orderCreate" => {
                        let order = self.stage_payment_terms_order(&field);
                        staged_ids.push(order["id"].as_str().unwrap_or_default().to_string());
                        logged = true;
                        selected_json(
                            &json!({ "order": order, "userErrors": [] }),
                            &field.selection,
                        )
                    }
                    "paymentTermsCreate" => match payment_terms_create_value(&field) {
                        Ok((owner_id, terms_id, attrs)) => {
                            // Hydrate (and stage) the owner so we can read its
                            // money and financial status. A paid Order is rejected
                            // before any payment-terms staging happens.
                            let (amount, currency) =
                                self.payment_terms_owner_money(request, &owner_id);
                            if self.payment_terms_owner_is_paid(&owner_id) {
                                payment_terms_payload_value(
                                    "paymentTermsCreate",
                                    Value::Null,
                                    vec![payment_terms_user_error(
                                        Value::Null,
                                        "Cannot create payment terms on an Order that has already been paid in full.",
                                        "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                                    )],
                                    &field.selection,
                                )["paymentTermsCreate"]
                                    .clone()
                            } else {
                                let record = payment_terms_record_from_attrs(
                                    &terms_id, &attrs, &amount, &currency,
                                );
                                self.store
                                    .staged
                                    .payment_terms
                                    .insert(terms_id.clone(), record.clone());
                                self.store
                                    .staged
                                    .payment_terms_owner_index
                                    .insert(owner_id.clone(), terms_id.clone());
                                self.attach_payment_terms_to_owner(&owner_id, Some(record.clone()));
                                staged_ids.push(terms_id);
                                logged = true;
                                payment_terms_payload_value(
                                    "paymentTermsCreate",
                                    record,
                                    Vec::new(),
                                    &field.selection,
                                )["paymentTermsCreate"]
                                    .clone()
                            }
                        }
                        Err(payload) => payload["paymentTermsCreate"].clone(),
                    },
                    "paymentTermsUpdate" => match payment_terms_update_value(&field) {
                        Ok((terms_id, attrs)) => {
                            let owner_id = self.payment_terms_owner_id(&terms_id);
                            // Cold update (no local owner link): hydrate the
                            // PaymentTerms node and reject if its owning Order has
                            // already been paid in full, without staging anything.
                            if owner_id.is_none()
                                && self.payment_terms_node_owner_paid(request, &terms_id)
                            {
                                payment_terms_payload_value(
                                    "paymentTermsUpdate",
                                    Value::Null,
                                    vec![payment_terms_user_error(
                                        Value::Null,
                                        "Cannot create payment terms on an Order that has already been paid in full.",
                                        "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
                                    )],
                                    &field.selection,
                                )["paymentTermsUpdate"]
                                    .clone()
                            } else {
                                let (amount, currency) = match owner_id.as_deref() {
                                    Some(owner) => self.payment_terms_owner_money(request, owner),
                                    None => self
                                        .payment_terms_record_money(&terms_id)
                                        .unwrap_or_else(|| ("0.0".to_string(), "CAD".to_string())),
                                };
                                let record = payment_terms_record_from_attrs(
                                    &terms_id, &attrs, &amount, &currency,
                                );
                                self.store
                                    .staged
                                    .payment_terms
                                    .insert(terms_id.clone(), record.clone());
                                if let Some(owner_id) = owner_id {
                                    self.attach_payment_terms_to_owner(
                                        &owner_id,
                                        Some(record.clone()),
                                    );
                                }
                                staged_ids.push(terms_id);
                                logged = true;
                                payment_terms_payload_value(
                                    "paymentTermsUpdate",
                                    record,
                                    Vec::new(),
                                    &field.selection,
                                )["paymentTermsUpdate"]
                                    .clone()
                            }
                        }
                        Err(payload) => payload["paymentTermsUpdate"].clone(),
                    },
                    "paymentTermsDelete" => {
                        let input =
                            resolved_object_field(&field.arguments, "input").unwrap_or_default();
                        let payment_terms_id =
                            resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
                        if self
                            .store
                            .staged
                            .payment_terms
                            .remove(&payment_terms_id)
                            .is_some()
                        {
                            if let Some(owner_id) =
                                self.remove_payment_terms_owner_link(&payment_terms_id)
                            {
                                self.attach_payment_terms_to_owner(&owner_id, None);
                            }
                            staged_ids.push(payment_terms_id.clone());
                            logged = true;
                            payment_terms_delete_payload_value(
                                json!(payment_terms_id),
                                Vec::new(),
                                &field.selection,
                            )["paymentTermsDelete"]
                                .clone()
                        } else {
                            payment_terms_delete_payload_value(
                                Value::Null,
                                vec![payment_terms_user_error(
                                    json!(["input", "paymentTermsId"]),
                                    "Payment terms do not exist",
                                    "payment_terms_deletion_unsuccessful",
                                )],
                                &field.selection,
                            )["paymentTermsDelete"]
                                .clone()
                        }
                    }
                    "order" => {
                        let id = resolved_string_arg(&field.arguments, "id")?;
                        self.selected_payment_terms_owner(&id, &field.selection, false)
                    }
                    "draftOrder" => {
                        let id = resolved_string_arg(&field.arguments, "id")?;
                        self.selected_payment_terms_owner(&id, &field.selection, true)
                    }
                    _ => continue,
                };
                data.insert(field.response_key, value);
            }
            if logged {
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "paymentTerms",
                    staged_ids,
                );
            }
            return Some(json!({ "data": Value::Object(data) }));
        }
        None
    }

    fn payment_terms_owner_id(&self, terms_id: &str) -> Option<String> {
        self.store.staged.payment_terms_owner_index.iter().find_map(
            |(owner_id, staged_terms_id)| (staged_terms_id == terms_id).then(|| owner_id.clone()),
        )
    }

    /// Resolves the owning order/draft money used to denominate a payment
    /// schedule. Orders carry presentment money (the schedule is presentment-
    /// denominated); drafts expose shop money. Prefers already-staged owners; in
    /// live-hybrid replay it hydrates the owner from the cassette and stages it so
    /// subsequent local reads (and the post-delete cleanup) observe the same
    /// graph. Falls back to `0.0 CAD` when no owner money is available.
    fn payment_terms_owner_money(&mut self, request: &Request, owner_id: &str) -> (String, String) {
        if let Some(money) = self
            .store
            .staged
            .orders
            .get(owner_id)
            .or_else(|| self.store.staged.draft_orders.get(owner_id))
            .and_then(payment_terms_extract_owner_money)
        {
            return money;
        }
        if let Some(owner) = self.hydrate_payment_terms_owner(request, owner_id) {
            let money = payment_terms_extract_owner_money(&owner);
            let target = if owner_id.starts_with("gid://shopify/DraftOrder/") {
                &mut self.store.staged.draft_orders
            } else {
                &mut self.store.staged.orders
            };
            target.entry(owner_id.to_string()).or_insert(owner);
            if let Some(money) = money {
                return money;
            }
        }
        ("0.0".to_string(), "CAD".to_string())
    }

    /// Cassette-backed owner hydration: in live-hybrid replay, issue the exact
    /// recorded `PaymentTermsOwnerHydrate` (Order) or `PaymentTermsDraftHydrate`
    /// (DraftOrder) document so the strict upstream matcher replays the real
    /// owner reply. Gated on LiveHybrid so other read modes are untouched;
    /// returns the `order`/`draftOrder` node from the recorded reply.
    fn hydrate_payment_terms_owner(&self, request: &Request, owner_id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let (query, operation_name) = if owner_id.starts_with("gid://shopify/DraftOrder/") {
            (
                PAYMENT_TERMS_DRAFT_HYDRATE_QUERY,
                "PaymentTermsDraftHydrate",
            )
        } else {
            (
                PAYMENT_TERMS_OWNER_HYDRATE_QUERY,
                "PaymentTermsOwnerHydrate",
            )
        };
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": query,
                "operationName": operation_name,
                "variables": { "id": owner_id }
            })
            .to_string(),
        });
        if response.status >= 400 {
            return None;
        }
        let data = response.body.get("data")?;
        data.get("draftOrder")
            .or_else(|| data.get("order"))
            .filter(|owner| !owner.is_null())
            .cloned()
    }

    /// True when a staged Order owner has been paid in full. Drafts (and orders
    /// without a recorded financial status) are never "paid" by this check, so it
    /// is safe to call for any owner type.
    fn payment_terms_owner_is_paid(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .orders
            .get(owner_id)
            .and_then(|owner| owner.get("displayFinancialStatus"))
            .and_then(Value::as_str)
            == Some("PAID")
    }

    /// Cold-path eligibility probe for `paymentTermsUpdate`: hydrate the
    /// PaymentTerms node by id and report whether its owning Order is paid in
    /// full. Returns false when hydration is unavailable (non-LiveHybrid, missing
    /// cassette, or a draft-owned node).
    fn payment_terms_node_owner_paid(&self, request: &Request, terms_id: &str) -> bool {
        self.hydrate_payment_terms_node(request, terms_id)
            .and_then(|node| node.get("order").cloned())
            .filter(|order| !order.is_null())
            .and_then(|order| {
                order
                    .get("displayFinancialStatus")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .as_deref()
            == Some("PAID")
    }

    /// Cassette-backed PaymentTerms-node hydration for the cold update path:
    /// issues the exact recorded `PaymentTermsHydrate` document and returns the
    /// resolved `paymentTerms` node. Gated on LiveHybrid.
    fn hydrate_payment_terms_node(&self, request: &Request, terms_id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": PAYMENT_TERMS_NODE_HYDRATE_QUERY,
                "operationName": "PaymentTermsHydrate",
                "variables": { "id": terms_id }
            })
            .to_string(),
        });
        if response.status >= 400 {
            return None;
        }
        response
            .body
            .get("data")?
            .get("paymentTerms")
            .filter(|node| !node.is_null())
            .cloned()
    }

    /// Reads the money already materialized on a staged payment-terms record's
    /// first schedule node, so an update whose owner link is unavailable reuses
    /// the money established at create time.
    fn payment_terms_record_money(&self, terms_id: &str) -> Option<(String, String)> {
        let node = self
            .store
            .staged
            .payment_terms
            .get(terms_id)?
            .get("paymentSchedules")?
            .get("nodes")?
            .as_array()?
            .first()?;
        let money = node.get("amount")?;
        Some((
            money.get("amount")?.as_str()?.to_string(),
            money.get("currencyCode")?.as_str()?.to_string(),
        ))
    }

    fn remove_payment_terms_owner_link(&mut self, terms_id: &str) -> Option<String> {
        let owner_id = self.payment_terms_owner_id(terms_id)?;
        self.store
            .staged
            .payment_terms_owner_index
            .remove(&owner_id);
        Some(owner_id)
    }

    fn attach_payment_terms_to_owner(&mut self, owner_id: &str, terms: Option<Value>) {
        let target = if owner_id.starts_with("gid://shopify/DraftOrder/") {
            &mut self.store.staged.draft_orders
        } else {
            &mut self.store.staged.orders
        };
        let entry = target.entry(owner_id.to_string()).or_insert_with(|| {
            json!({
                "id": owner_id,
                "name": if owner_id.starts_with("gid://shopify/DraftOrder/") { "#DRAFT" } else { "#1" }
            })
        });
        entry["paymentTerms"] = terms.unwrap_or(Value::Null);
    }

    fn selected_payment_terms_owner(
        &self,
        owner_id: &str,
        selection: &[SelectedField],
        draft_order: bool,
    ) -> Value {
        let record = if draft_order {
            self.store.staged.draft_orders.get(owner_id)
        } else {
            self.store.staged.orders.get(owner_id)
        };
        record
            .map(|record| selected_json(record, selection))
            .unwrap_or(Value::Null)
    }

    fn stage_payment_terms_order(&mut self, field: &RootFieldSelection) -> Value {
        let order_arg = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        let id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let price_set = order_arg
            .get("lineItems")
            .and_then(|_| {
                resolved_object_list_field(&order_arg, "lineItems")
                    .first()
                    .cloned()
            })
            .and_then(|line| resolved_object_field(&line, "priceSet"))
            .map(|price_set| {
                json!({
                    "shopMoney": {
                        "amount": resolved_object_field(&price_set, "shopMoney")
                            .and_then(|money| resolved_string_field(&money, "amount"))
                            .unwrap_or_else(|| "42.50".to_string()),
                        "currencyCode": resolved_object_field(&price_set, "shopMoney")
                            .and_then(|money| resolved_string_field(&money, "currencyCode"))
                            .unwrap_or_else(|| "USD".to_string())
                    },
                    "presentmentMoney": {
                        "amount": resolved_object_field(&price_set, "presentmentMoney")
                            .and_then(|money| resolved_string_field(&money, "amount"))
                            .unwrap_or_else(|| "57.00".to_string()),
                        "currencyCode": resolved_object_field(&price_set, "presentmentMoney")
                            .and_then(|money| resolved_string_field(&money, "currencyCode"))
                            .unwrap_or_else(|| "CAD".to_string())
                    }
                })
            })
            .unwrap_or_else(|| {
                json!({
                    "shopMoney": { "amount": "57.00", "currencyCode": "CAD" },
                    "presentmentMoney": { "amount": "57.00", "currencyCode": "CAD" }
                })
            });
        let order = json!({
            "id": id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "currentTotalPriceSet": price_set,
            "paymentTerms": Value::Null
        });
        self.store.staged.orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        order
    }

    pub(in crate::proxy) fn order_return_local_runtime_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if matches!(
            root_field,
            "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
        ) {
            if !self.should_handle_order_return_read(&fields) {
                return None;
            }
            return self.order_return_read_data(&fields);
        }

        let field = fields.iter().find(|field| field.name == root_field)?;
        match root_field {
            "returnCreate" => {
                let value = self.stage_return_from_input(field, "returnInput", "OPEN");
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnRequest" => {
                let value = self.stage_return_from_input(field, "input", "REQUESTED");
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnApproveRequest" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "id"))?;
                let value = self.approve_return_request(&id, field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnDeclineRequest" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "id"))?;
                let value = self.decline_return_request(&id, field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnClose" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "CLOSED", field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnReopen" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "OPEN", field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnCancel" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "CANCELED", field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "removeFromReturn" => {
                let value = self.remove_from_return(field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "reverseDeliveryCreateWithShipping" => {
                let value = self.stage_reverse_delivery(field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "reverseDeliveryShippingUpdate" => {
                let id = resolved_string_arg(&field.arguments, "reverseDeliveryId")?;
                let value = self.update_reverse_delivery(&id, field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "reverseFulfillmentOrderDispose" => {
                let value = self.dispose_reverse_fulfillment_order(field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            "returnProcess" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "returnId"))?;
                let value = self.process_return(&id, field);
                Some(orders_payments_data_response(&field.response_key, value))
            }
            _ => None,
        }
    }

    fn order_return_read_data(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "return" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .returns
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.selected_return_order(&id, &field.selection)
                }
                "reverseDelivery" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .reverse_deliveries
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "reverseFulfillmentOrder" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .reverse_fulfillment_orders
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn should_handle_order_return_read(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "return" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.returns.contains_key(&id)),
            "order" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.returns_by_order.contains_key(&id)),
            "reverseDelivery" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.reverse_deliveries.contains_key(&id)),
            "reverseFulfillmentOrder" => {
                resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                    self.store
                        .staged
                        .reverse_fulfillment_orders
                        .contains_key(&id)
                })
            }
            _ => false,
        })
    }

    /// Stage a return from a `returnCreate` (`OPEN`) or `returnRequest`
    /// (`REQUESTED`) input. Reads the seeded order from store state, validates
    /// each requested line against the order's fulfillment line items and the
    /// quantity already consumed by prior non-canceled returns, builds the
    /// return line items + (for OPEN) the reverse fulfillment order, and stages
    /// the result. IDs come from the shared synthetic counter so the allocation
    /// order (return line items, return, RFO line items, RFO) matches the
    /// reference implementation. Returns the selected `{ return, userErrors }`
    /// payload — `return` is null when validation fails.
    fn stage_return_from_input(
        &mut self,
        field: &RootFieldSelection,
        input_name: &str,
        status: &str,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let order_id = resolved_string_field(&input, "orderId").unwrap_or_default();
        let order = self
            .store
            .staged
            .orders
            .get(&order_id)
            .cloned()
            .unwrap_or(Value::Null);
        let items = resolved_object_list_field(&input, "returnLineItems");
        if items.is_empty() {
            return selected_json(
                &json!({
                    "return": Value::Null,
                    "userErrors": [return_user_error(
                        &["returnLineItems"],
                        "Return must include at least one line item.",
                        "INVALID",
                    )]
                }),
                &field.selection,
            );
        }
        // Validate every line first, allocating return-line-item ids only for
        // valid lines (matching the reference fold). Any error short-circuits
        // the mutation with a null return and no state change.
        let mut line_items: Vec<Value> = Vec::new();
        let mut user_errors: Vec<Value> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let fli_id = resolved_string_field(item, "fulfillmentLineItemId");
            let quantity = resolved_i64_field(item, "quantity").unwrap_or(0);
            let fulfillment_line_item = fli_id
                .as_deref()
                .and_then(|id| find_order_fulfillment_line_item(&order, id));
            match fulfillment_line_item {
                None => user_errors.push(return_user_error(
                    &[
                        "returnLineItems",
                        &index.to_string(),
                        "fulfillmentLineItemId",
                    ],
                    "Fulfillment line item does not exist.",
                    "INVALID",
                )),
                Some(fulfillment_line_item) => {
                    let available = fulfillment_line_item["quantity"].as_i64().unwrap_or(0);
                    let already = self.already_returned_quantity(
                        &order,
                        &order_id,
                        fli_id.as_deref().unwrap_or_default(),
                    );
                    let remaining = (available - already).max(0);
                    if quantity <= 0 || quantity > remaining {
                        user_errors.push(return_user_error(
                            &["returnLineItems", &index.to_string(), "quantity"],
                            "Quantity is not available for return.",
                            "INVALID",
                        ));
                    } else {
                        let rli_id = self.next_synthetic_gid("ReturnLineItem");
                        line_items.push(build_return_line_item(
                            &rli_id,
                            &fulfillment_line_item,
                            item,
                        ));
                    }
                }
            }
        }
        if !user_errors.is_empty() {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let return_id = self.next_synthetic_gid("Return");
        let order_name = order["name"].as_str().unwrap_or("#ORDER").to_string();
        let prior_returns = order_returns_array(&order).len()
            + self
                .store
                .staged
                .returns_by_order
                .get(&order_id)
                .map(Vec::len)
                .unwrap_or(0);
        let name = format!("{order_name}-R{}", prior_returns + 1);
        let total_quantity: i64 = line_items
            .iter()
            .map(|line| line["quantity"].as_i64().unwrap_or(0))
            .sum();
        let order_updated_at = order["updatedAt"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| "2024-01-01T00:00:03.000Z".to_string());
        let mut return_record = json!({
            "id": return_id,
            "name": name,
            "status": status,
            "closedAt": Value::Null,
            "decline": Value::Null,
            "totalQuantity": total_quantity,
            "order": {
                "id": order_id,
                "updatedAt": order_updated_at
            },
            "returnLineItems": { "nodes": line_items },
            "returnShippingFees": [],
            "reverseFulfillmentOrders": { "nodes": [] }
        });
        if let Some(fee_input) = resolved_object_field(&input, "returnShippingFee") {
            let amount = resolved_object_field(&fee_input, "amount").unwrap_or_default();
            let amount_value =
                resolved_string_field(&amount, "amount").unwrap_or_else(|| "0.00".to_string());
            let currency =
                resolved_string_field(&amount, "currencyCode").unwrap_or_else(|| "USD".to_string());
            let fee_id = self.next_synthetic_gid("ReturnShippingFee");
            return_record["returnShippingFees"] = json!([{
                "id": fee_id,
                "amountSet": return_money_set(&amount_value, &currency)
            }]);
        }
        if status == "OPEN" {
            self.build_return_reverse_fulfillment_order(&mut return_record);
        }
        self.store
            .staged
            .returns
            .insert(return_id.clone(), return_record.clone());
        self.store
            .staged
            .returns_by_order
            .entry(order_id)
            .or_default()
            .push(return_id);
        selected_json(
            &json!({ "return": return_record, "userErrors": [] }),
            &field.selection,
        )
    }

    /// Total quantity already consumed against a fulfillment line item by
    /// non-canceled returns — both returns embedded in the seeded order graph
    /// (from hydration) and returns staged during this session. Mirrors the
    /// reference `already_returned_quantity` so quantity caps account for the
    /// real outstanding return volume rather than the raw fulfilled quantity.
    fn already_returned_quantity(
        &self,
        order: &Value,
        order_id: &str,
        fulfillment_line_item_id: &str,
    ) -> i64 {
        let mut total = 0_i64;
        let mut accumulate = |return_value: &Value| {
            if return_value["status"].as_str() == Some("CANCELED") {
                return;
            }
            for line in return_line_items_array(return_value) {
                if return_line_item_fulfillment_line_item_id(&line).as_deref()
                    == Some(fulfillment_line_item_id)
                {
                    total += line["quantity"].as_i64().unwrap_or(0);
                }
            }
        };
        for embedded in order_returns_array(order) {
            accumulate(&embedded);
        }
        if let Some(ids) = self.store.staged.returns_by_order.get(order_id) {
            for id in ids {
                if let Some(staged) = self.store.staged.returns.get(id) {
                    accumulate(staged);
                }
            }
        }
        total
    }

    fn selected_return_order(&self, order_id: &str, selection: &[SelectedField]) -> Value {
        let returns = self
            .store
            .staged
            .returns_by_order
            .get(order_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| self.store.staged.returns.get(&id).cloned())
            .collect::<Vec<_>>();
        let order = self.store.staged.orders.get(order_id).cloned();
        let name = order
            .as_ref()
            .and_then(|order| order["name"].as_str())
            .unwrap_or("#ORDER")
            .to_string();
        let updated_at = order
            .as_ref()
            .and_then(|order| order["updatedAt"].as_str())
            .unwrap_or("2024-01-01T00:00:03.000Z")
            .to_string();
        selected_json(
            &json!({
                "id": order_id,
                "name": name,
                "updatedAt": updated_at,
                "returns": return_connection(returns)
            }),
            selection,
        )
    }

    /// `returnApproveRequest`: a REQUESTED return transitions to OPEN and
    /// acquires its reverse fulfillment order. Any other status returns the
    /// `return_request_status_invalid` user error on `id` (INVALID) and leaves
    /// state untouched.
    fn approve_return_request(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_status_invalid_error()] }),
                &field.selection,
            );
        };
        if record["status"].as_str() != Some("REQUESTED") {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_status_invalid_error()] }),
                &field.selection,
            );
        }
        record["status"] = json!("OPEN");
        self.build_return_reverse_fulfillment_order(&mut record);
        self.store
            .staged
            .returns
            .insert(id.to_string(), record.clone());
        selected_json(
            &json!({ "return": record, "userErrors": [] }),
            &field.selection,
        )
    }

    /// `returnDeclineRequest`: validate the decline input (reason enum, note
    /// length, notify email) before touching state; a REQUESTED return then
    /// transitions to DECLINED carrying `decline { reason, note }`. A non-
    /// REQUESTED return returns `return_request_status_invalid` on `id`.
    fn decline_return_request(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let reason = match validate_return_decline_input(&input) {
            Ok(reason) => reason,
            Err(errors) => {
                return selected_json(
                    &json!({ "return": Value::Null, "userErrors": errors }),
                    &field.selection,
                );
            }
        };
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_status_invalid_error()] }),
                &field.selection,
            );
        };
        if record["status"].as_str() != Some("REQUESTED") {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_status_invalid_error()] }),
                &field.selection,
            );
        }
        let note = resolved_string_field(&input, "declineNote").unwrap_or_default();
        record["status"] = json!("DECLINED");
        record["decline"] = json!({ "reason": reason, "note": note });
        self.store
            .staged
            .returns
            .insert(id.to_string(), record.clone());
        selected_json(
            &json!({ "return": record, "userErrors": [] }),
            &field.selection,
        )
    }

    /// `returnClose` / `returnReopen` / `returnCancel`. Allowed transitions
    /// mirror the reference `return_status_transition_error` rules: close from
    /// OPEN/CLOSED, reopen from CLOSED/OPEN, cancel from any return without
    /// processed/refunded lines (and idempotent CANCELED). Disallowed
    /// transitions return INVALID_STATE with the reference message and leave
    /// state untouched; same-status requests are idempotent no-ops.
    fn apply_return_lifecycle_transition(
        &mut self,
        id: &str,
        target_status: &str,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_user_error(&["id"], "Return does not exist.", "INVALID")] }),
                &field.selection,
            );
        };
        let current = record["status"].as_str().unwrap_or_default().to_string();
        if let Some((message, code)) = return_status_transition_error(target_status, &record) {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_user_error(&["id"], message, code)] }),
                &field.selection,
            );
        }
        if current != target_status {
            record["status"] = json!(target_status);
            record["closedAt"] = if target_status == "CLOSED" {
                json!("2024-01-01T00:00:03.000Z")
            } else {
                Value::Null
            };
            self.store
                .staged
                .returns
                .insert(id.to_string(), record.clone());
        }
        selected_json(
            &json!({ "return": record, "userErrors": [] }),
            &field.selection,
        )
    }

    /// `removeFromReturn`: validate each removal against the return's removable
    /// quantity (current minus processed) before mutating; on success reduce or
    /// drop the affected return line items, recompute the total, and rebuild the
    /// reverse fulfillment order's line items from the surviving return lines.
    /// On any validation error the return is left null with the error payload.
    fn remove_from_return(&mut self, field: &RootFieldSelection) -> Value {
        let return_id = resolved_string_arg(&field.arguments, "returnId").unwrap_or_default();
        let removals = list_object_arg(&field.arguments, "returnLineItems");
        let Some(mut record) = self.store.staged.returns.get(&return_id).cloned() else {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_user_error(&["returnId"], "Return does not exist.", "INVALID")] }),
                &field.selection,
            );
        };
        let mut nodes = record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut user_errors: Vec<Value> = Vec::new();
        for (index, removal) in removals.iter().enumerate() {
            let line_item_id = resolved_string_field(removal, "returnLineItemId");
            let quantity = resolved_i64_field(removal, "quantity").unwrap_or(0);
            let position = line_item_id.as_deref().and_then(|id| {
                nodes
                    .iter()
                    .position(|node| node["id"].as_str() == Some(id))
            });
            match position {
                None => user_errors.push(return_user_error(
                    &["returnLineItems", &index.to_string(), "returnLineItemId"],
                    "Return line item does not exist.",
                    "INVALID",
                )),
                Some(position) => {
                    let current = nodes[position]["quantity"].as_i64().unwrap_or(0);
                    let processed = nodes[position]["processedQuantity"].as_i64().unwrap_or(0);
                    let removable = current - processed;
                    if quantity <= 0 || quantity > removable {
                        user_errors.push(return_user_error(
                            &["returnLineItems", &index.to_string(), "quantity"],
                            "Quantity is not removable from return.",
                            "INVALID",
                        ));
                    } else {
                        let next_quantity = current - quantity;
                        if next_quantity <= 0 {
                            nodes.remove(position);
                        } else {
                            nodes[position]["quantity"] = json!(next_quantity);
                            let next_processed =
                                nodes[position]["processedQuantity"].as_i64().unwrap_or(0);
                            nodes[position]["unprocessedQuantity"] =
                                json!((next_quantity - next_processed).max(0));
                        }
                    }
                }
            }
        }
        if !user_errors.is_empty() {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let total_quantity: i64 = nodes
            .iter()
            .map(|n| n["quantity"].as_i64().unwrap_or(0))
            .sum();
        record["returnLineItems"] = json!({ "nodes": nodes.clone() });
        record["totalQuantity"] = json!(total_quantity);
        self.sync_reverse_fulfillment_line_items(&mut record);
        self.store.staged.returns.insert(return_id, record.clone());
        selected_json(
            &json!({ "return": record, "userErrors": [] }),
            &field.selection,
        )
    }

    /// Build the OPEN reverse fulfillment order for a return: one RFO line item
    /// per return line item (allocated first), then the RFO itself, so the
    /// shared synthetic counter yields RFO-line ids before the RFO id. Each RFO
    /// line item carries both `returnLineItem { id }` and the nested
    /// `fulfillmentLineItem { id, lineItem { id, title } }` so local and live
    /// selections both resolve. Stores the RFO and mirrors it onto the return.
    fn build_return_reverse_fulfillment_order(&mut self, return_record: &mut Value) {
        if return_record["reverseFulfillmentOrders"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
        {
            return;
        }
        let return_lines = return_record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rfo_lines: Vec<Value> = Vec::new();
        for line in &return_lines {
            let line_id = self.next_synthetic_gid("ReverseFulfillmentOrderLineItem");
            let quantity = line["quantity"].as_i64().unwrap_or(0);
            let processed = line["processedQuantity"].as_i64().unwrap_or(0);
            rfo_lines.push(json!({
                "id": line_id,
                "totalQuantity": quantity,
                "remainingQuantity": (quantity - processed).max(0),
                "dispositionType": Value::Null,
                "returnLineItem": { "id": line["id"].clone() },
                "fulfillmentLineItem": line["fulfillmentLineItem"].clone(),
                "dispositions": []
            }));
        }
        let rfo_id = self.next_synthetic_gid("ReverseFulfillmentOrder");
        let reverse_order = json!({
            "id": rfo_id,
            "status": "OPEN",
            "lineItems": { "nodes": rfo_lines },
            "reverseDeliveries": { "nodes": [] }
        });
        return_record["reverseFulfillmentOrders"] = json!({ "nodes": [reverse_order.clone()] });
        self.store
            .staged
            .reverse_fulfillment_orders
            .insert(rfo_id, reverse_order);
    }

    /// Rebuild the return's reverse fulfillment order line items from its
    /// current return line items (used after `removeFromReturn`). Existing RFO
    /// line ids are reused when their return line survives; removed return lines
    /// drop their RFO line. The reverse fulfillment order's `totalQuantity` /
    /// `remainingQuantity` are recomputed and the staged RFO is kept in sync.
    fn sync_reverse_fulfillment_line_items(&mut self, return_record: &mut Value) {
        let return_lines = return_record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rfos = return_record["reverseFulfillmentOrders"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for rfo in &mut rfos {
            let existing = rfo["lineItems"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut rebuilt: Vec<Value> = Vec::new();
            for line in &return_lines {
                let quantity = line["quantity"].as_i64().unwrap_or(0);
                let processed = line["processedQuantity"].as_i64().unwrap_or(0);
                let mut rfo_line = existing
                    .iter()
                    .find(|candidate| candidate["returnLineItem"]["id"] == line["id"])
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "id": Value::Null,
                            "dispositionType": Value::Null,
                            "returnLineItem": { "id": line["id"].clone() },
                            "fulfillmentLineItem": line["fulfillmentLineItem"].clone(),
                            "dispositions": []
                        })
                    });
                rfo_line["totalQuantity"] = json!(quantity);
                rfo_line["remainingQuantity"] = json!((quantity - processed).max(0));
                rebuilt.push(rfo_line);
            }
            rfo["lineItems"] = json!({ "nodes": rebuilt });
            if let Some(id) = rfo["id"].as_str() {
                if let Some(staged) = self.store.staged.reverse_fulfillment_orders.get_mut(id) {
                    staged["lineItems"] = rfo["lineItems"].clone();
                }
            }
        }
        return_record["reverseFulfillmentOrders"] = json!({ "nodes": rfos });
    }

    fn stage_reverse_delivery(&mut self, field: &RootFieldSelection) -> Value {
        let reverse_order_id =
            resolved_string_arg(&field.arguments, "reverseFulfillmentOrderId").unwrap_or_default();
        let id = self.next_synthetic_gid("ReverseDelivery");
        let line_id = self.next_synthetic_gid("ReverseDeliveryLineItem");
        let tracking = resolved_object_field(&field.arguments, "trackingInput").unwrap_or_default();
        let label = resolved_object_field(&field.arguments, "labelInput").unwrap_or_default();
        let delivery = json!({
            "id": id,
            "reverseFulfillmentOrder": { "id": reverse_order_id },
            "reverseDeliveryLineItems": {
                "nodes": [{
                    "id": line_id,
                    "quantity": 1,
                    "reverseFulfillmentOrderLineItem": {
                        "id": self.first_reverse_fulfillment_order_line_id(&reverse_order_id),
                        "totalQuantity": self.first_reverse_fulfillment_order_line_field(&reverse_order_id, "totalQuantity"),
                        "remainingQuantity": self.first_reverse_fulfillment_order_line_field(&reverse_order_id, "remainingQuantity")
                    }
                }]
            },
            "deliverable": {
                "__typename": "ReverseDeliveryShippingDeliverable",
                "tracking": {
                    "number": resolved_string_field(&tracking, "number").unwrap_or_default(),
                    "url": resolved_string_field(&tracking, "url").unwrap_or_default(),
                    "company": resolved_string_field(&tracking, "company").unwrap_or_default(),
                    "carrierName": Value::Null
                },
                "label": {
                    "publicFileUrl": resolved_string_field(&label, "fileUrl").unwrap_or_default()
                }
            }
        });
        self.store
            .staged
            .reverse_deliveries
            .insert(id.clone(), delivery.clone());
        if let Some(reverse_order) = self
            .store
            .staged
            .reverse_fulfillment_orders
            .get_mut(&reverse_order_id)
        {
            reverse_order["reverseDeliveries"] = json!({ "nodes": [{ "id": id }] });
        }
        selected_json(
            &json!({ "reverseDelivery": delivery, "userErrors": [] }),
            &field.selection,
        )
    }

    fn first_reverse_fulfillment_order_line_id(&self, reverse_order_id: &str) -> Value {
        self.store
            .staged
            .reverse_fulfillment_orders
            .get(reverse_order_id)
            .and_then(|order| order["lineItems"]["nodes"].as_array())
            .and_then(|nodes| nodes.first())
            .map(|node| node["id"].clone())
            .unwrap_or(Value::Null)
    }

    fn first_reverse_fulfillment_order_line_field(
        &self,
        reverse_order_id: &str,
        field: &str,
    ) -> Value {
        self.store
            .staged
            .reverse_fulfillment_orders
            .get(reverse_order_id)
            .and_then(|order| order["lineItems"]["nodes"].as_array())
            .and_then(|nodes| nodes.first())
            .map(|node| node[field].clone())
            .unwrap_or(Value::Null)
    }

    fn update_reverse_delivery(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut delivery) = self.store.staged.reverse_deliveries.get(id).cloned() else {
            return selected_json(
                &json!({ "reverseDelivery": Value::Null, "userErrors": [return_user_error(&["reverseDeliveryId"], "Reverse delivery does not exist", "NOT_FOUND")] }),
                &field.selection,
            );
        };
        let tracking = resolved_object_field(&field.arguments, "trackingInput").unwrap_or_default();
        delivery["deliverable"]["tracking"]["number"] =
            json!(resolved_string_field(&tracking, "number").unwrap_or_default());
        delivery["deliverable"]["tracking"]["url"] =
            json!(resolved_string_field(&tracking, "url").unwrap_or_default());
        if let Some(company) = resolved_string_field(&tracking, "company") {
            delivery["deliverable"]["tracking"]["company"] = json!(company);
        }
        delivery["deliverable"]["tracking"]["carrierName"] = Value::Null;
        self.store
            .staged
            .reverse_deliveries
            .insert(id.to_string(), delivery.clone());
        selected_json(
            &json!({ "reverseDelivery": delivery, "userErrors": [] }),
            &field.selection,
        )
    }

    fn dispose_reverse_fulfillment_order(&mut self, field: &RootFieldSelection) -> Value {
        let inputs = list_object_arg(&field.arguments, "dispositionInputs");
        let mut line_items = Vec::new();
        for input in inputs {
            let line_id = resolved_string_field(&input, "reverseFulfillmentOrderLineItemId")
                .unwrap_or_default();
            for order in self.store.staged.reverse_fulfillment_orders.values_mut() {
                if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                    if let Some(node) = nodes.iter_mut().find(|node| node["id"] == line_id) {
                        node["remainingQuantity"] = json!(0);
                        node["dispositionType"] =
                            json!(resolved_string_field(&input, "dispositionType")
                                .unwrap_or_else(|| "RESTOCKED".to_string()));
                        node["dispositions"] = json!([{
                            "type": node["dispositionType"].clone(),
                            "quantity": resolved_i64_field(&input, "quantity").unwrap_or(1),
                            "location": {
                                "id": resolved_string_field(&input, "locationId").unwrap_or_default()
                            }
                        }]);
                        line_items.push(node.clone());
                    }
                }
            }
        }
        selected_json(
            &json!({ "reverseFulfillmentOrderLineItems": line_items, "userErrors": [] }),
            &field.selection,
        )
    }

    fn process_return(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [return_user_error(&["returnId"], "Return does not exist", "NOT_FOUND")] }),
                &field.selection,
            );
        };
        record["status"] = json!("OPEN");
        if let Some(nodes) = record["returnLineItems"]["nodes"].as_array_mut() {
            for node in nodes {
                node["processedQuantity"] = node["quantity"].clone();
                node["unprocessedQuantity"] = json!(0);
            }
        }
        if let Some(nodes) = record["reverseFulfillmentOrders"]["nodes"].as_array_mut() {
            for node in nodes {
                let Some(id) = node["id"].as_str() else {
                    continue;
                };
                if let Some(reverse_order) = self.store.staged.reverse_fulfillment_orders.get(id) {
                    node["status"] = reverse_order["status"].clone();
                    node["lineItems"] = reverse_order["lineItems"].clone();
                }
            }
        }
        let mut stored_record = record.clone();
        stored_record["status"] = json!("CLOSED");
        self.store
            .staged
            .returns
            .insert(id.to_string(), stored_record);
        selected_json(
            &json!({ "return": record, "userErrors": [] }),
            &field.selection,
        )
    }
}

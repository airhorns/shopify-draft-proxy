use super::*;
use sha2::{Digest, Sha256};

mod abandonments;
mod customer_payment_methods;
mod delivery_settings;
mod events;
mod money_bag;
mod payment_customizations;
mod payment_reminders;
mod payment_terms;
mod quantity_pricing;
mod quantity_rules;
mod returns;

pub(in crate::proxy) use self::delivery_settings::*;
pub(in crate::proxy) use self::events::*;
pub(in crate::proxy) use self::payment_customizations::*;
pub(in crate::proxy) use self::payment_reminders::*;
pub(in crate::proxy) use self::payment_terms::*;
pub(in crate::proxy) use self::quantity_pricing::*;
pub(in crate::proxy) use self::quantity_rules::*;

pub(in crate::proxy) fn metafield_compare_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(in crate::proxy) fn owner_type_from_gid(id: &str) -> String {
    metafield_owner_gid_resource_type(id).to_ascii_uppercase()
}

pub(in crate::proxy) type MetafieldNamespaceKeyValidation = (
    &'static str, // field path segment
    &'static str, // code
    &'static str, // message
);

pub(in crate::proxy) fn metafield_namespace_key_validation(
    namespace: &str,
    key: &str,
) -> Vec<MetafieldNamespaceKeyValidation> {
    let mut errors = Vec::new();
    if namespace.chars().count() < 3 {
        errors.push((
            "namespace",
            "TOO_SHORT",
            "Namespace is too short (minimum is 3 characters)",
        ));
    } else if namespace.chars().count() > 255 {
        errors.push((
            "namespace",
            "TOO_LONG",
            "Namespace is too long (maximum is 255 characters)",
        ));
    }
    if key.chars().count() < 2 {
        errors.push((
            "key",
            "TOO_SHORT",
            "Key is too short (minimum is 2 characters)",
        ));
    } else if key.chars().count() > 64 {
        errors.push((
            "key",
            "TOO_LONG",
            "Key is too long (maximum is 64 characters)",
        ));
    }
    errors
}

fn metafields_set_namespace_key_validation(
    namespace: &str,
    key: &str,
) -> Option<MetafieldNamespaceKeyValidation> {
    let errors = metafield_namespace_key_validation(namespace, key);
    [
        ("namespace", "TOO_SHORT"),
        ("key", "TOO_SHORT"),
        ("namespace", "TOO_LONG"),
        ("key", "TOO_LONG"),
    ]
    .into_iter()
    .find_map(|(field, code)| {
        errors
            .iter()
            .copied()
            .find(|error| error.0 == field && error.1 == code)
    })
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

fn normalize_date_time_value(value: &str) -> String {
    let (without_offset, offset) =
        if let Some(value) = value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) {
            (value, "+00:00")
        } else if has_timezone_offset(value) {
            (&value[..value.len() - 6], &value[value.len() - 6..])
        } else {
            (value, "+00:00")
        };
    let without_fraction = without_offset
        .split_once('.')
        .map(|(head, _)| head)
        .unwrap_or(without_offset);
    format!("{without_fraction}{offset}")
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

pub(in crate::proxy) fn is_measurement_metafield_type_name(type_name: &str) -> bool {
    !measurement_units_for_type(type_name).is_empty()
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
/// floats render through Shopify's decimal text normalization. Mirrors Gleam
/// `json_number_string_field`.
fn json_number_string_field(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key) {
        Some(Value::Number(number)) => {
            if let Some(int_value) = number.as_i64() {
                Some(format!("{int_value}.0"))
            } else {
                number
                    .as_f64()
                    .map(|value| shopify_decimal_text(&value.to_string()))
            }
        }
        Some(Value::String(text)) => {
            if let Ok(int_value) = text.parse::<i64>() {
                Some(format!("{int_value}.0"))
            } else {
                text.parse::<f64>()
                    .ok()
                    .map(|value| shopify_decimal_text(&value.to_string()))
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
                Value::String(shopify_decimal_text(&float_value.to_string()))
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

pub(in crate::proxy) fn metafields_set_reference_values(
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<String> {
    let mut ids = Vec::new();
    for input in inputs {
        let Some(metafield_type) = resolved_string_field(input, "type") else {
            continue;
        };
        let value = resolved_string_field(input, "value").unwrap_or_default();
        if let Some(inner_type) = metafield_type.strip_prefix("list.") {
            if metafield_reference_type_name(inner_type).is_some() {
                ids.extend(metafield_list_items(&value).into_iter().filter_map(|item| {
                    item.as_str()
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                }));
            }
        } else if metafield_reference_type_name(&metafield_type).is_some() && !value.is_empty() {
            ids.push(value);
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

pub(in crate::proxy) fn metafields_set_input_errors<F>(
    inputs: &[BTreeMap<String, ResolvedValue>],
    mut reference_exists: F,
) -> Vec<Value>
where
    F: FnMut(&str) -> bool,
{
    if inputs.len() > 25 {
        return vec![metafields_set_path_user_error(
            vec!["metafields"],
            "LESS_THAN_OR_EQUAL_TO",
            "Exceeded the maximum metafields input limit of 25.",
        )];
    }

    let mut errors = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        if let Some(error) = metafields_set_input_shape_error(index, input) {
            errors.push(error);
            continue;
        }
        let Some(metafield_type) = resolved_string_field(input, "type") else {
            continue;
        };
        let value = resolved_string_field(input, "value").unwrap_or_default();
        errors.extend(metafield_value_user_errors(
            index,
            &metafield_type,
            &value,
            &mut reference_exists,
        ));
    }
    errors
}

fn metafields_set_input_shape_error(
    index: usize,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let namespace =
        canonical_app_metafield_namespace(resolved_string_field(input, "namespace").as_deref());
    let key = resolved_string_field(input, "key").unwrap_or_default();
    if let Some(error) = metafields_set_namespace_key_validation(&namespace, &key) {
        let index = index.to_string();
        Some(metafields_set_path_user_error(
            vec!["metafields", &index, error.0],
            error.1,
            error.2,
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
    } else {
        None
    }
}

fn metafield_value_user_errors<F>(
    index: usize,
    metafield_type: &str,
    value: &str,
    reference_exists: &mut F,
) -> Vec<Value>
where
    F: FnMut(&str) -> bool,
{
    if let Some(inner_type) = metafield_type.strip_prefix("list.") {
        return list_metafield_value_user_errors(index, inner_type, value, reference_exists);
    }
    metafield_scalar_value_error(metafield_type, value, reference_exists)
        .map(|message| metafields_set_value_user_error(index, &message, "INVALID_VALUE"))
        .into_iter()
        .collect()
}

fn list_metafield_value_user_errors<F>(
    index: usize,
    inner_type: &str,
    value: &str,
    reference_exists: &mut F,
) -> Vec<Value>
where
    F: FnMut(&str) -> bool,
{
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(value) else {
        return vec![metafields_set_value_user_error(
            index,
            "Value must be a JSON array.",
            "INVALID_VALUE",
        )];
    };
    if items.len() > 128 {
        return vec![metafields_set_value_user_error(
            index,
            "Value has more than 128 elements.",
            "INVALID_VALUE",
        )];
    }
    if metafield_reference_type_name(inner_type).is_some() {
        return items
            .iter()
            .find_map(|item| list_metafield_item_error(inner_type, item, reference_exists))
            .map(|message| metafields_set_value_user_error(index, &message, "INVALID_VALUE"))
            .into_iter()
            .collect();
    }
    let mut errors = Vec::new();
    for (element_index, item) in items.iter().enumerate() {
        if let Some(message) = list_metafield_item_error(inner_type, item, reference_exists) {
            errors.push(metafields_set_value_user_error_with_element_index(
                index,
                &message,
                "INVALID_VALUE",
                Some(element_index),
            ));
        }
    }
    errors
}

fn list_metafield_item_error<F>(
    inner_type: &str,
    item: &Value,
    reference_exists: &mut F,
) -> Option<String>
where
    F: FnMut(&str) -> bool,
{
    match inner_type {
        "number_integer" | "integer" => match item {
            Value::Number(number) if number.as_i64().is_some() => None,
            Value::String(value) if value.parse::<i64>().is_ok() => None,
            _ => Some("Value must be an integer.".to_string()),
        },
        "boolean" => match item {
            Value::Bool(_) => None,
            Value::String(value) if matches!(value.as_str(), "true" | "false") => None,
            _ => Some("Value must be true or false.".to_string()),
        },
        "link" | "rating" if item.is_object() => {
            metafield_scalar_json_value_error(inner_type, item, reference_exists)
        }
        _ if is_measurement_metafield_type_name(inner_type) && item.is_object() => {
            metafield_scalar_json_value_error(inner_type, item, reference_exists)
        }
        _ => {
            let Some(value) = list_item_string_value(item) else {
                return Some("Value is invalid.".to_string());
            };
            metafield_scalar_value_error(inner_type, &value, reference_exists)
        }
    }
}

fn metafield_scalar_value_error<F>(
    metafield_type: &str,
    value: &str,
    reference_exists: &mut F,
) -> Option<String>
where
    F: FnMut(&str) -> bool,
{
    match metafield_type {
        "number_integer" | "integer" => value
            .parse::<i64>()
            .is_err()
            .then(|| "Value must be an integer.".to_string()),
        "number_decimal" | "float" => shopify_decimal_error(value),
        "boolean" => (!matches!(value, "true" | "false"))
            .then(|| "Value must be true or false.".to_string()),
        "color" => {
            (!is_shopify_hex_color(value)).then(|| "Value must be a hex color code.".to_string())
        }
        "date" => (!is_shopify_date(value))
            .then(|| "Value must be in YYYY-MM-DD format.".to_string()),
        "date_time" => (!is_shopify_date_time(value)).then(|| {
            "Value must be in YYYY-MM-DDTHH:MM:SS format.".to_string()
        }),
        "json" => serde_json::from_str::<Value>(value)
            .is_err()
            .then(|| "Value is invalid JSON.".to_string()),
        "money" | "link" | "rating" => serde_json::from_str::<Value>(value)
            .ok()
            .as_ref()
            .and_then(|parsed| metafield_scalar_json_value_error(metafield_type, parsed, reference_exists))
            .or_else(|| {
                serde_json::from_str::<Value>(value)
                    .is_err()
                    .then(|| metafield_json_object_message(metafield_type).to_string())
            }),
        "url" => (!is_shopify_metafield_url(value)).then(|| {
            "Value cannot have an empty scheme (protocol), must include one of the following URL schemes: [\"http\", \"https\", \"mailto\", \"sms\", \"tel\"].'".to_string()
        }),
        "single_line_text_field" => {
            if value.trim().is_empty() {
                Some("Value can't be blank.".to_string())
            } else if value.contains('\n') || value.contains('\r') {
                Some("Value must be a single line text string.".to_string())
            } else {
                None
            }
        }
        "multi_line_text_field" => value
            .trim()
            .is_empty()
            .then(|| "Value can't be blank.".to_string()),
        _ if is_measurement_metafield_type_name(metafield_type) => serde_json::from_str::<Value>(value)
            .ok()
            .as_ref()
            .and_then(|parsed| metafield_scalar_json_value_error(metafield_type, parsed, reference_exists))
            .or_else(|| {
                serde_json::from_str::<Value>(value)
                    .is_err()
                    .then(|| "Value must be a non-negative number.".to_string())
            }),
        _ => match metafield_reference_type_name(metafield_type) {
            Some(_) if !reference_exists(value) => Some(format!(
                "Value references non-existent resource {value}."
            )),
            _ => None,
        },
    }
}

fn metafield_scalar_json_value_error<F>(
    metafield_type: &str,
    parsed: &Value,
    reference_exists: &mut F,
) -> Option<String>
where
    F: FnMut(&str) -> bool,
{
    match metafield_type {
        "money" => (!is_shopify_money_value(parsed))
            .then(|| metafield_json_object_message(metafield_type).to_string()),
        "link" => (!is_shopify_link_value(parsed))
            .then(|| metafield_json_object_message(metafield_type).to_string()),
        "rating" => shopify_rating_value_error(parsed),
        _ if is_measurement_metafield_type_name(metafield_type) => {
            shopify_measurement_value_error(metafield_type, parsed)
        }
        _ => metafield_scalar_value_error(
            metafield_type,
            parsed.as_str().unwrap_or_default(),
            reference_exists,
        ),
    }
}

fn metafield_json_object_message(metafield_type: &str) -> &'static str {
    match metafield_type {
        "money" => "Value must be a stringified JSON object with amount (numeric) and currency_code (string matching the shop's currency) fields.",
        "link" => "Value must be a valid link.",
        _ => "Value is invalid.",
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn metafields_set_definition_user_errors(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let owner_id = resolved_string_field(input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(input, "namespace").as_deref(),
            );
            let key = resolved_string_field(input, "key").unwrap_or_default();
            let value = resolved_string_field(input, "value").unwrap_or_default();
            let owner_type = owner_type_from_gid(&owner_id);
            let Some(definition) =
                self.store
                    .staged
                    .metafield_definitions
                    .get(&metafield_definition_store_key(
                        &owner_type,
                        &namespace,
                        &key,
                    ))
            else {
                continue;
            };
            errors.extend(
                self.metafields_set_definition_validation_errors(definition, index, &value),
            );
        }
        errors
    }

    fn metafields_set_definition_validation_errors(
        &self,
        definition: &Value,
        index: usize,
        value: &str,
    ) -> Vec<Value> {
        let metafield_type = definition["type"]["name"].as_str().unwrap_or_default();
        let validations = definition["validations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut errors = Vec::new();

        if let Some(message) =
            metafield_definition_text_length_error(metafield_type, &validations, value)
                .or_else(|| {
                    metafield_definition_numeric_range_error(metafield_type, &validations, value)
                })
                .or_else(|| metafield_definition_regex_error(&validations, value))
                .or_else(|| metafield_definition_choices_error(&validations, value))
                .or_else(|| {
                    metafield_definition_rating_scale_error(metafield_type, &validations, value)
                })
                .or_else(|| {
                    metafield_definition_date_range_error(metafield_type, &validations, value)
                })
                .or_else(|| {
                    self.metafield_definition_metaobject_reference_error(
                        metafield_type,
                        &validations,
                        value,
                    )
                })
        {
            errors.push(metafields_set_value_user_error(
                index,
                &message,
                "INVALID_VALUE",
            ));
        }

        errors
    }

    fn metafield_definition_metaobject_reference_error(
        &self,
        metafield_type: &str,
        validations: &[Value],
        value: &str,
    ) -> Option<String> {
        let allowed_definition_ids =
            metafield_definition_allowed_metaobject_definition_ids(validations);
        if allowed_definition_ids.is_empty() {
            return None;
        }
        let invalid = if metafield_type == "metaobject_reference" {
            !self.metaobject_reference_matches_allowed_definition(value, &allowed_definition_ids)
        } else if metafield_type == "list.metaobject_reference" {
            let Ok(Value::Array(items)) = serde_json::from_str::<Value>(value) else {
                return None;
            };
            items.iter().filter_map(Value::as_str).any(|item| {
                !self.metaobject_reference_matches_allowed_definition(item, &allowed_definition_ids)
            })
        } else {
            false
        };
        invalid.then(|| "Value must belong to the configured metaobject definition.".to_string())
    }

    fn metaobject_reference_matches_allowed_definition(
        &self,
        metaobject_id: &str,
        allowed_definition_ids: &[String],
    ) -> bool {
        let Some(record) = self.metaobject_by_id(metaobject_id) else {
            return false;
        };
        if record
            .get("definition")
            .and_then(|definition| definition.get("id"))
            .and_then(Value::as_str)
            .is_some_and(|id| allowed_definition_ids.iter().any(|allowed| allowed == id))
        {
            return true;
        }
        let Some(meta_type) = record.get("type").and_then(Value::as_str) else {
            return false;
        };
        self.store
            .staged
            .metaobject_definitions
            .values()
            .any(|definition| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
                    && definition
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| {
                            allowed_definition_ids.iter().any(|allowed| allowed == id)
                                && !self.store.staged.metaobject_definitions.is_tombstoned(id)
                        })
            })
    }
}

fn metafield_definition_text_length_error(
    metafield_type: &str,
    validations: &[Value],
    value: &str,
) -> Option<String> {
    if !matches!(
        metafield_type,
        "single_line_text_field" | "multi_line_text_field"
    ) {
        return None;
    }
    let length = value.chars().count();
    if validation_i64(validations, "min").is_some_and(|min| length < min as usize) {
        Some("Value is too short.".to_string())
    } else if validation_i64(validations, "max").is_some_and(|max| length > max as usize) {
        Some("Value is too long.".to_string())
    } else {
        None
    }
}

fn metafield_definition_numeric_range_error(
    metafield_type: &str,
    validations: &[Value],
    value: &str,
) -> Option<String> {
    if !matches!(
        metafield_type,
        "number_integer" | "integer" | "number_decimal" | "float"
    ) {
        return None;
    }
    let parsed = value.parse::<f64>().ok()?;
    if let Some((min, min_text)) = validation_f64_with_text(validations, "min") {
        if parsed < min {
            return Some(format!("Value has a minimum of {min_text}."));
        }
    }
    if let Some((max, max_text)) = validation_f64_with_text(validations, "max") {
        if parsed > max {
            return Some(format!("Value has a maximum of {max_text}."));
        }
    }
    None
}

fn metafield_definition_regex_error(validations: &[Value], value: &str) -> Option<String> {
    let pattern = validation_string(validations, "regex")?;
    regex::Regex::new(&pattern)
        .ok()
        .filter(|regex| regex.is_match(value))
        .map(|_| ())
        .is_none()
        .then(|| "Value does not match the required pattern.".to_string())
}

fn metafield_definition_choices_error(validations: &[Value], value: &str) -> Option<String> {
    let choices = validation_string_list(validations, "choices");
    (!choices.is_empty() && !choices.iter().any(|choice| choice == value))
        .then(|| "Value must be one of the allowed choices.".to_string())
}

fn metafield_definition_rating_scale_error(
    metafield_type: &str,
    validations: &[Value],
    value: &str,
) -> Option<String> {
    if metafield_type != "rating" {
        return None;
    }
    let parsed = serde_json::from_str::<Value>(value).ok()?;
    let rating = parsed.get("value").and_then(json_f64_value)?;
    if let Some((scale_min, scale_min_text)) = validation_f64_with_text(validations, "scale_min") {
        if rating < scale_min {
            return Some(format!("Value has a minimum of {scale_min_text}."));
        }
    }
    if let Some((scale_max, scale_max_text)) = validation_f64_with_text(validations, "scale_max") {
        if rating > scale_max {
            return Some(format!("Value has a maximum of {scale_max_text}."));
        }
    }
    None
}

fn metafield_definition_date_range_error(
    metafield_type: &str,
    validations: &[Value],
    value: &str,
) -> Option<String> {
    match metafield_type {
        "date" if is_shopify_date(value) => {
            if let Some(min) =
                validation_string(validations, "min").filter(|min| is_shopify_date(min))
            {
                if value < min.as_str() {
                    return Some(format!("Value has a minimum date of {min}."));
                }
            }
            if let Some(max) =
                validation_string(validations, "max").filter(|max| is_shopify_date(max))
            {
                if value > max.as_str() {
                    return Some(format!("Value has a maximum date of {max}."));
                }
            }
            None
        }
        "date_time" if is_shopify_date_time(value) => {
            let value_key = parse_shopify_date_time_sort_key(value)?;
            if let Some(min) =
                validation_string(validations, "min").filter(|min| is_shopify_date_time(min))
            {
                let min_key = parse_shopify_date_time_sort_key(&min)?;
                if value_key < min_key {
                    return Some(format!("Value has a minimum date-time of {min}."));
                }
            }
            if let Some(max) =
                validation_string(validations, "max").filter(|max| is_shopify_date_time(max))
            {
                let max_key = parse_shopify_date_time_sort_key(&max)?;
                if value_key > max_key {
                    return Some(format!("Value has a maximum date-time of {max}."));
                }
            }
            None
        }
        _ => None,
    }
}

fn metafield_definition_allowed_metaobject_definition_ids(validations: &[Value]) -> Vec<String> {
    let mut ids = validation_string_list(validations, "metaobject_definition_id");
    ids.extend(validation_string_list(
        validations,
        "metaobject_definition_ids",
    ));
    ids.sort();
    ids.dedup();
    ids
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

fn validation_string(validations: &[Value], name: &str) -> Option<String> {
    validations.iter().find_map(|validation| {
        (validation.get("name").and_then(Value::as_str) == Some(name))
            .then(|| {
                validation
                    .get("value")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten()
    })
}

fn validation_string_list(validations: &[Value], name: &str) -> Vec<String> {
    let Some(value) = validation_string(validations, name) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(&value) {
        Ok(Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Ok(Value::String(value)) => vec![value],
        _ => vec![value],
    }
}

fn validation_f64_with_text(validations: &[Value], name: &str) -> Option<(f64, String)> {
    let value = validation_string(validations, name)?;
    value.parse::<f64>().ok().map(|parsed| (parsed, value))
}

fn metafields_set_value_user_error(index: usize, message: &str, code: &str) -> Value {
    metafields_set_value_user_error_with_element_index(index, message, code, None)
}

fn metafields_set_value_user_error_with_element_index(
    index: usize,
    message: &str,
    code: &str,
    element_index: Option<usize>,
) -> Value {
    user_error_with_element_index(
        vec![
            "metafields".to_string(),
            index.to_string(),
            "value".to_string(),
        ],
        message,
        Some(code),
        element_index.map(Value::from).unwrap_or(Value::Null),
    )
}

fn metafields_set_path_user_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    user_error_with_element_index(
        field,
        message,
        (!code.is_empty()).then_some(code),
        Value::Null,
    )
}

pub(in crate::proxy) fn metafields_set_row_user_error(
    index: usize,
    code: &str,
    message: &str,
) -> Value {
    metafields_set_path_user_error(vec!["metafields", &index.to_string()], code, message)
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
    parse_shopify_date_time_sort_key(value).is_some()
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ShopifyDateTimeSortKey {
    seconds_utc: i64,
    nanosecond: u32,
}

fn parse_shopify_date_time_sort_key(value: &str) -> Option<ShopifyDateTimeSortKey> {
    if !value.is_ascii() {
        return None;
    }
    let (date_part, time_part) = value.split_once(['T', ' '])?;
    let (year, month, day) = parse_shopify_date_parts(date_part)?;
    let (time_part, offset_seconds) = split_shopify_time_offset(time_part)?;
    let (time_core, nanosecond) = parse_shopify_time_fraction(time_part)?;
    let mut segments = time_core.split(':');
    let hour = parse_ascii_u32(segments.next()?)?;
    let minute = parse_ascii_u32(segments.next()?)?;
    let second = parse_ascii_u32(segments.next().unwrap_or("0"))?;
    if segments.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let seconds_utc = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second)
        - i64::from(offset_seconds);
    Some(ShopifyDateTimeSortKey {
        seconds_utc,
        nanosecond,
    })
}

fn parse_shopify_date_parts(value: &str) -> Option<(i32, u32, u32)> {
    if !is_shopify_date(value) {
        return None;
    }
    Some((
        value[0..4].parse().ok()?,
        value[5..7].parse().ok()?,
        value[8..10].parse().ok()?,
    ))
}

fn split_shopify_time_offset(value: &str) -> Option<(&str, i32)> {
    if let Some(time) = value.strip_suffix(['Z', 'z']) {
        return Some((time, 0));
    }
    if value.len() >= 6 {
        let offset_start = value.len() - 6;
        let offset = &value[offset_start..];
        let sign = offset.as_bytes()[0];
        if matches!(sign, b'+' | b'-') && offset.as_bytes()[3] == b':' {
            let hours = parse_ascii_u32(&offset[1..3])?;
            let minutes = parse_ascii_u32(&offset[4..6])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let offset_seconds = (hours * 3_600 + minutes * 60) as i32;
            return Some((
                &value[..offset_start],
                if sign == b'+' {
                    offset_seconds
                } else {
                    -offset_seconds
                },
            ));
        }
    }
    Some((value, 0))
}

fn parse_shopify_time_fraction(value: &str) -> Option<(&str, u32)> {
    let Some((time, fraction)) = value.split_once('.') else {
        return Some((value, 0));
    };
    if fraction.is_empty() || !fraction.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let mut nanoseconds = String::with_capacity(9);
    nanoseconds.extend(fraction.chars().take(9));
    while nanoseconds.len() < 9 {
        nanoseconds.push('0');
    }
    Some((time, nanoseconds.parse().ok()?))
}

fn parse_ascii_u32(value: &str) -> Option<u32> {
    (!value.is_empty() && value.chars().all(|character| character.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn metafield_reference_type_name(type_name: &str) -> Option<&str> {
    type_name.strip_suffix("_reference")
}

fn metafield_list_items(value: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(value) {
        Ok(Value::Array(items)) => items,
        _ => Vec::new(),
    }
}

fn list_item_string_value(item: &Value) -> Option<String> {
    match item {
        Value::String(value) => Some(value.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn is_shopify_decimal(value: &str) -> bool {
    shopify_decimal_error(value).is_none()
}

fn shopify_decimal_error(value: &str) -> Option<String> {
    if value.is_empty() || value.trim() != value {
        return Some("Value must be a decimal.".to_string());
    }
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    if unsigned.is_empty() {
        return Some("Value must be a decimal.".to_string());
    }
    let mut parts = unsigned.split('.');
    let integer_part = parts.next().unwrap_or_default();
    let fractional_part = parts.next();
    if parts.next().is_some()
        || integer_part.is_empty()
        || !integer_part.chars().all(|ch| ch.is_ascii_digit())
    {
        return Some("Value must be a decimal.".to_string());
    }
    if let Some(fractional_part) = fractional_part {
        if fractional_part.is_empty() || !fractional_part.chars().all(|ch| ch.is_ascii_digit()) {
            return Some("Value must be a decimal.".to_string());
        }
    }
    let significant_integer = integer_part.trim_start_matches('0');
    if significant_integer.len() > 13
        || (significant_integer.len() == 13 && significant_integer > "9999999999999")
    {
        if value.starts_with('-') {
            Some("Value can't be less than -9999999999999.".to_string())
        } else {
            Some("Value can't exceed 9999999999999.".to_string())
        }
    } else {
        None
    }
}

fn is_shopify_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || !value
            .chars()
            .enumerate()
            .all(|(index, character)| matches!(index, 4 | 7) || character.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<i32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    if !(1..=12).contains(&month) {
        return false;
    }
    (1..=days_in_month(year, month)).contains(&day)
}

pub(in crate::proxy) fn is_shopify_metafield_url(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return url::Url::parse(value)
            .ok()
            .and_then(|url| url.host_str().map(|host| !host.is_empty()))
            .unwrap_or(false);
    }
    for scheme in ["mailto:", "sms:", "tel:"] {
        if lowered.starts_with(scheme) {
            let rest = &value[scheme.len()..];
            return !rest.trim().is_empty() && !rest.chars().any(char::is_whitespace);
        }
    }
    false
}

pub(in crate::proxy) fn is_shopify_money_value(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    let Some(amount) = fields
        .get("amount")
        .and_then(json_number_or_string_value)
        .filter(|amount| is_shopify_decimal(amount))
    else {
        return false;
    };
    let Some(currency_code) = fields.get("currency_code").and_then(Value::as_str) else {
        return false;
    };
    !amount.starts_with('-')
        && currency_code.len() == 3
        && currency_code.chars().all(|ch| ch.is_ascii_uppercase())
}

pub(in crate::proxy) fn is_shopify_link_value(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    let Some(label) = fields
        .get("label")
        .or_else(|| fields.get("text"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(url) = fields.get("url").and_then(Value::as_str) else {
        return false;
    };
    !label.trim().is_empty() && is_shopify_metafield_url(url)
}

pub(in crate::proxy) fn shopify_measurement_value_error(
    type_name: &str,
    value: &Value,
) -> Option<String> {
    let Some(fields) = value.as_object() else {
        return Some("Value must contain unit and value.".to_string());
    };
    let Some(number) = fields
        .get("value")
        .and_then(json_f64_value)
        .filter(|number| number.is_finite() && *number >= 0.0)
    else {
        return Some("Value must be a non-negative number.".to_string());
    };
    let Some(unit) = fields.get("unit").and_then(Value::as_str) else {
        return Some("Value must contain unit and value.".to_string());
    };
    if number.is_finite() && measurement_unit_is_supported(type_name, unit) {
        None
    } else {
        Some("Value must be a supported unit.".to_string())
    }
}

fn shopify_rating_value_error(value: &Value) -> Option<String> {
    let Some(fields) = value.as_object() else {
        return Some("Value must be a rating.".to_string());
    };
    let Some((rating, rating_text)) = fields.get("value").and_then(json_f64_value_with_original)
    else {
        return Some("Value must be a rating.".to_string());
    };
    let Some((scale_min, scale_min_text)) = fields
        .get("scale_min")
        .and_then(json_f64_value_with_original)
    else {
        return Some("Value must be a rating.".to_string());
    };
    let Some((scale_max, scale_max_text)) = fields
        .get("scale_max")
        .and_then(json_f64_value_with_original)
    else {
        return Some("Value must be a rating.".to_string());
    };
    if !(rating.is_finite() && scale_min.is_finite() && scale_max.is_finite()) {
        return Some("Value must be a rating.".to_string());
    }
    if scale_min >= scale_max {
        return Some("Value must be a rating.".to_string());
    }
    if rating < scale_min {
        Some(format!("Value has a minimum of {scale_min_text}."))
    } else if rating > scale_max {
        Some(format!("Value has a maximum of {scale_max_text}."))
    } else {
        let _ = rating_text;
        None
    }
}

fn json_number_or_string_value(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn json_f64_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn json_f64_value_with_original(value: &Value) -> Option<(f64, String)> {
    match value {
        Value::Number(number) => number.as_f64().map(|parsed| (parsed, number.to_string())),
        Value::String(text) => text
            .parse::<f64>()
            .ok()
            .map(|parsed| (parsed, text.clone())),
        _ => None,
    }
}

pub(in crate::proxy) fn measurement_unit_is_supported(type_name: &str, unit: &str) -> bool {
    let normalized = measurement_unit_alias(unit);
    measurement_units_for_type(type_name).contains(&normalized.as_str())
}

fn measurement_unit_alias(unit: &str) -> String {
    match unit.to_ascii_lowercase().as_str() {
        "cm" | "centimeter" | "centimeters" => "CENTIMETERS".to_string(),
        "mm" | "millimeter" | "millimeters" => "MILLIMETERS".to_string(),
        "m" | "meter" | "meters" => "METERS".to_string(),
        "in" | "inch" | "inches" => "INCHES".to_string(),
        "ft" | "foot" | "feet" => "FEET".to_string(),
        "yd" | "yard" | "yards" => "YARDS".to_string(),
        "km" | "kilometer" | "kilometers" => "KILOMETERS".to_string(),
        "mi" | "mile" | "miles" => "MILES".to_string(),
        "kg" | "kilogram" | "kilograms" => "KILOGRAMS".to_string(),
        "g" | "gram" | "grams" => "GRAMS".to_string(),
        "lb" | "lbs" | "pound" | "pounds" => "POUNDS".to_string(),
        "oz" | "ounce" | "ounces" => "OUNCES".to_string(),
        "ml" | "milliliter" | "milliliters" => "MILLILITERS".to_string(),
        "l" | "liter" | "liters" => "LITERS".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

pub(in crate::proxy) fn measurement_units_for_type(type_name: &str) -> &'static [&'static str] {
    match type_name {
        "antenna_gain" => &["DECIBELS_ISOTROPIC"],
        "area" => &[
            "SQUARE_CENTIMETERS",
            "SQUARE_FEET",
            "SQUARE_INCHES",
            "SQUARE_METERS",
        ],
        "battery_charge_capacity" => &["MILLIAMP_HOURS"],
        "battery_energy_capacity" => &["WATT_HOURS"],
        "capacitance" => &["MICROFARADS", "FARADS", "NANOFARADS", "PICOFARADS"],
        "concentration" => &["MILLIGRAMS_PER_MILLILITER"],
        "data_storage_capacity" => &["BYTES", "KILOBYTES", "MEGABYTES", "GIGABYTES", "TERABYTES"],
        "data_transfer_rate" => &[
            "BITS_PER_SECOND",
            "KILOBITS_PER_SECOND",
            "MEGABITS_PER_SECOND",
            "GIGABITS_PER_SECOND",
        ],
        "dimension" | "distance" => &[
            "MILLIMETERS",
            "CENTIMETERS",
            "METERS",
            "KILOMETERS",
            "INCHES",
            "FEET",
            "YARDS",
            "MILES",
        ],
        "display_density" => &["PIXELS_PER_INCH"],
        "duration" => &["MILLISECONDS", "SECONDS", "MINUTES", "HOURS", "DAYS"],
        "electric_current" => &["AMPERES", "MILLIAMPERES"],
        "electrical_resistance" => &["OHMS", "KILOHMS", "MEGOHMS"],
        "energy" => &["JOULES", "KILOJOULES", "CALORIES", "KILOCALORIES"],
        "frequency" => &["HERTZ", "KILOHERTZ", "MEGAHERTZ", "GIGAHERTZ"],
        "illuminance" => &["LUX"],
        "inductance" => &["MILLIHENRIES", "HENRIES"],
        "luminous_flux" => &["LUMENS"],
        "mass_flow_rate" => &["KILOGRAMS_PER_HOUR"],
        "power" => &["WATTS", "KILOWATTS"],
        "pressure" => &["PASCALS", "KILOPASCALS", "BAR", "POUNDS_PER_SQUARE_INCH"],
        "resolution" => &["MEGAPIXELS"],
        "rotational_speed" => &["REVOLUTIONS_PER_MINUTE"],
        "sound_level" => &["DECIBELS"],
        "speed" => &["METERS_PER_SECOND", "KILOMETERS_PER_HOUR", "MILES_PER_HOUR"],
        "temperature" => &["CELSIUS", "FAHRENHEIT", "KELVIN"],
        "thermal_power" => &["BRITISH_THERMAL_UNITS_PER_HOUR"],
        "voltage" => &["VOLTS", "MILLIVOLTS", "KILOVOLTS"],
        "volume" => &[
            "MILLILITERS",
            "LITERS",
            "CUBIC_CENTIMETERS",
            "CUBIC_METERS",
            "FLUID_OUNCES",
            "GALLONS",
        ],
        "volumetric_flow_rate" => &["LITERS_PER_MINUTE"],
        "weight" => &["GRAMS", "KILOGRAMS", "OUNCES", "POUNDS"],
        _ => &[],
    }
}

fn money_bag_currency(money_set: &Value) -> String {
    money_set["shopMoney"]["currencyCode"]
        .as_str()
        .unwrap_or("USD")
        .to_string()
}

fn money_bag_add_decimal_strings(left: &str, right: &str) -> String {
    let total = left.parse::<f64>().unwrap_or(0.0) + right.parse::<f64>().unwrap_or(0.0);
    format!("{total:.1}")
}

fn resolved_money_pair(
    money: Option<BTreeMap<String, ResolvedValue>>,
    defaults: [&str; 2],
) -> [String; 2] {
    let money = money.unwrap_or_default();
    [
        resolved_string_field(&money, "amount").unwrap_or_else(|| defaults[0].to_string()),
        resolved_string_field(&money, "currencyCode").unwrap_or_else(|| defaults[1].to_string()),
    ]
}

fn line_item_price_set_values(
    first_line: &BTreeMap<String, ResolvedValue>,
    absent_price_set: [&str; 4],
    shop_defaults: [&str; 2],
    presentment_defaults: Option<[&str; 2]>,
) -> [String; 4] {
    let Some(price_set) = resolved_object_field(first_line, "priceSet") else {
        return absent_price_set.map(str::to_string);
    };
    let [shop_amount, shop_currency] = resolved_money_pair(
        resolved_object_field(&price_set, "shopMoney"),
        shop_defaults,
    );
    let presentment_default = presentment_defaults
        .map(|defaults| defaults.map(str::to_string))
        .unwrap_or_else(|| [shop_amount.clone(), shop_currency.clone()]);
    let [presentment_amount, presentment_currency] = resolved_money_pair(
        resolved_object_field(&price_set, "presentmentMoney"),
        [&presentment_default[0], &presentment_default[1]],
    );
    [
        shop_amount,
        shop_currency,
        presentment_amount,
        presentment_currency,
    ]
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

impl DraftProxy {
    pub(in crate::proxy) fn staged_order_input_and_first_line(
        &mut self,
        field: &RootFieldSelection,
    ) -> (
        String,
        BTreeMap<String, ResolvedValue>,
        BTreeMap<String, ResolvedValue>,
    ) {
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        let id = shopify_gid("Order", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let first_line = resolved_object_list_field(&order_input, "lineItems")
            .first()
            .cloned()
            .unwrap_or_default();
        (id, order_input, first_line)
    }
}

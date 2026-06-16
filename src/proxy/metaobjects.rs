use super::*;

fn metaobject_create_duplicate_field_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for (index, field) in fields.iter().enumerate() {
            let ResolvedValue::Object(field) = field else {
                continue;
            };
            let Some(key) = resolved_string_field(field, "key") else {
                continue;
            };
            if seen.insert(key.clone()) {
                continue;
            }

            let field_index = index.to_string();
            let is_required_title = key == "title";
            errors.push(json!({
                "field": ["metaobject", "fields", field_index.clone()],
                "message": format!("Field \"{key}\" duplicates other inputs"),
                "code": "DUPLICATE_FIELD_INPUT",
                "elementKey": key.clone(),
                "elementIndex": null
            }));
            if is_required_title {
                errors.push(json!({
                    "field": ["metaobject", "fields", field_index],
                    "message": "Title can't be blank",
                    "code": "OBJECT_FIELD_REQUIRED",
                    "elementKey": key,
                    "elementIndex": null
                }));
            }
        }
    }
    errors
}

fn metaobject_field_record_from_definition(
    field_definition: &Value,
    value: Option<&String>,
) -> Value {
    let field_type = field_definition["type"]["name"]
        .as_str()
        .unwrap_or("single_line_text_field");
    let value = value.map(String::as_str).unwrap_or_default();
    json!({
        "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
        "type": field_type,
        "value": value,
        "jsonValue": metaobject_field_json_value(field_type, Some(value)),
        "definition": field_definition
    })
}

fn metaobject_field_record_from_existing_value(
    field_definition: &Value,
    value: Option<&Value>,
) -> Value {
    let field_type = field_definition["type"]["name"]
        .as_str()
        .unwrap_or("single_line_text_field");
    let value = value.cloned().unwrap_or(Value::Null);
    let json_value = value
        .as_str()
        .map(|value| metaobject_field_json_value(field_type, Some(value)))
        .unwrap_or(Value::Null);
    json!({
        "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
        "type": field_type,
        "value": value,
        "jsonValue": json_value,
        "definition": field_definition
    })
}

fn metaobject_field_name(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn metaobject_definition_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
) -> Value {
    let name = resolved_string_field(input, "name").unwrap_or_else(|| meta_type.to_string());
    let display_name_key = resolved_string_field(input, "displayNameKey");
    let field_definitions = resolved_object_list_field(input, "fieldDefinitions")
        .into_iter()
        .map(metaobject_field_definition_record)
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "type": meta_type,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "displayNameKey": display_name_key,
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"},
        "capabilities": metaobject_definition_capabilities(input),
        "fieldDefinitions": field_definitions,
        "hasThumbnailField": false,
        "metaobjectsCount": 0,
        "standardTemplate": Value::Null,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z"
    })
}

fn metaobject_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let publishable = resolved_object_field(&capabilities, "publishable")
        .and_then(|publishable| resolved_bool_field(&publishable, "enabled"))
        .unwrap_or(false);
    let online_store_input =
        resolved_object_field(&capabilities, "onlineStore").unwrap_or_default();
    let online_store = resolved_bool_field(&online_store_input, "enabled").unwrap_or(false);
    let renderable = resolved_object_field(&capabilities, "renderable")
        .and_then(|renderable| resolved_bool_field(&renderable, "enabled"))
        .unwrap_or(false);
    let translatable = resolved_object_field(&capabilities, "translatable")
        .and_then(|translatable| resolved_bool_field(&translatable, "enabled"))
        .unwrap_or(false);
    json!({
        "publishable": {"enabled": publishable},
        "onlineStore": {
            "enabled": online_store,
            "data": if online_store {
                metaobject_online_store_data_from_input(&online_store_input, true)
            } else {
                Value::Null
            }
        },
        "renderable": {"enabled": renderable},
        "translatable": {"enabled": translatable}
    })
}

fn metaobject_online_store_data_from_input(
    online_store: &BTreeMap<String, ResolvedValue>,
    can_create_redirects: bool,
) -> Value {
    let Some(data) = resolved_object_field(online_store, "data") else {
        return Value::Null;
    };
    let Some(url_handle) = resolved_string_field(&data, "urlHandle") else {
        return Value::Null;
    };
    json!({
        "urlHandle": url_handle,
        "canCreateRedirects": can_create_redirects
    })
}

fn metaobject_field_definition_record(input: BTreeMap<String, ResolvedValue>) -> Value {
    let key = resolved_string_field(&input, "key").unwrap_or_default();
    let name = resolved_string_field(&input, "name").unwrap_or_else(|| metaobject_field_name(&key));
    let field_type = metaobject_field_definition_type(&input);
    json!({
        "key": key,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "required": resolved_bool_field(&input, "required").unwrap_or(false),
        "type": {"name": field_type, "category": metaobject_field_type_category(&field_type)},
        "validations": resolved_object_list_field(&input, "validations")
            .into_iter()
            .map(|validation| {
                json!({
                    "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                })
            })
            .collect::<Vec<_>>()
    })
}

fn metaobject_field_definition_type(input: &BTreeMap<String, ResolvedValue>) -> String {
    match input.get("type") {
        Some(ResolvedValue::String(value)) => value.clone(),
        Some(ResolvedValue::Object(value)) => resolved_string_field(value, "name")
            .unwrap_or_else(|| "single_line_text_field".to_string()),
        _ => "single_line_text_field".to_string(),
    }
}

fn metaobject_updated_field_definition(
    mut field_definition: Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    if let Some(name) = resolved_string_field(input, "name") {
        field_definition["name"] = json!(name);
    }
    if input.contains_key("description") {
        field_definition["description"] = input
            .get("description")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |description| json!(description));
    }
    if let Some(required) = resolved_bool_field(input, "required") {
        field_definition["required"] = json!(required);
    }
    if let Some(field_type) = resolved_string_field(input, "type") {
        field_definition["type"] = json!({
            "name": field_type,
            "category": metaobject_field_type_category(&field_type)
        });
    }
    if input.contains_key("validations") {
        field_definition["validations"] = json!(resolved_object_list_field(input, "validations")
            .into_iter()
            .map(|validation| {
                json!({
                    "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                })
            })
            .collect::<Vec<_>>());
    }
    field_definition
}

fn metaobject_field_type_category(field_type: &str) -> &'static str {
    match field_type {
        "number_integer" | "number_decimal" => "NUMBER",
        "boolean" => "TRUE_FALSE",
        "date" | "date_time" => "DATE_TIME",
        "json" | "rich_text_field" => "JSON",
        "link" | "list.link" => "LINK",
        value if value.ends_with("_reference") || value.starts_with("list.") => "REFERENCE",
        _ => "TEXT",
    }
}

fn metaobject_field_json_value(field_type: &str, value: Option<&str>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match field_type {
        "number_integer" => value
            .parse::<i64>()
            .map_or(Value::Null, |number| json!(number)),
        "number_decimal" => value
            .parse::<f64>()
            .map_or(Value::Null, |number| json!(number)),
        "boolean" => match value {
            "true" => json!(true),
            "false" => json!(false),
            _ => Value::Null,
        },
        "json" | "rich_text_field" => serde_json::from_str(value).unwrap_or_else(|_| json!(value)),
        value_type if value_type.starts_with("list.") => {
            serde_json::from_str(value).unwrap_or_else(|_| json!([value]))
        }
        _ => json!(value),
    }
}

fn metaobject_value_matches_type(field_type: &str, value: &str) -> bool {
    match field_type {
        "number_integer" => value.parse::<i64>().is_ok(),
        "number_decimal" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false"),
        "json" | "rich_text_field" => serde_json::from_str::<Value>(value).is_ok(),
        value_type if value_type.starts_with("list.") => serde_json::from_str::<Value>(value)
            .ok()
            .is_some_and(|value| value.is_array()),
        _ => true,
    }
}

fn metaobject_create_input_values(
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for field in fields {
            if let ResolvedValue::Object(field) = field {
                if let (Some(key), Some(value)) = (
                    resolved_string_field(field, "key"),
                    resolved_string_field(field, "value"),
                ) {
                    values.insert(key, value);
                }
            }
        }
    }
    if let Some(ResolvedValue::Object(object)) = input.get("values") {
        for (key, value) in object {
            match value {
                ResolvedValue::String(value) => {
                    values.insert(key.clone(), value.clone());
                }
                ResolvedValue::Null => {
                    values.insert(key.clone(), String::new());
                }
                _ => {
                    values.insert(key.clone(), resolved_value_json(value).to_string());
                }
            }
        }
    }
    values
}

fn metaobject_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
) -> Vec<Value> {
    let mut errors = metaobject_create_duplicate_field_errors(input);
    let definition_keys = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field.get("key").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();

    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for (index, field) in fields.iter().enumerate() {
            let ResolvedValue::Object(field) = field else {
                continue;
            };
            let key = resolved_string_field(field, "key").unwrap_or_default();
            if !definition_keys.contains(key.as_str()) {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "fields", &index.to_string()],
                    &format!("Field key \"{key}\" is not defined on this metaobject definition."),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    json!(index),
                ));
            } else if let Some(field_definition) = definition["fieldDefinitions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|definition| {
                    definition.get("key").and_then(Value::as_str) == Some(key.as_str())
                })
            {
                let value = resolved_string_field(field, "value").unwrap_or_default();
                if !metaobject_value_matches_type(
                    field_definition["type"]["name"]
                        .as_str()
                        .unwrap_or_default(),
                    &value,
                ) {
                    errors.push(metaobject_user_error(
                        vec!["metaobject", "fields", &index.to_string()],
                        &format!("Value is invalid for field \"{key}\"."),
                        "INVALID_VALUE",
                        json!(key),
                        json!(index),
                    ));
                }
            }
        }
    }

    for key in input_values.keys() {
        if !definition_keys.contains(key.as_str()) {
            errors.push(metaobject_user_error(
                vec!["metaobject", "values", key],
                &format!("Field key \"{key}\" is not defined on this metaobject definition."),
                "UNDEFINED_OBJECT_FIELD",
                json!(key),
                Value::Null,
            ));
        }
    }

    for field_definition in definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = field_definition
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if field_definition
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && input_values
                .get(key)
                .is_none_or(|value| value.trim().is_empty())
        {
            errors.push(metaobject_user_error(
                vec!["metaobject", "fields"],
                &format!("Field \"{key}\" is required."),
                "OBJECT_FIELD_REQUIRED",
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(capabilities) = resolved_object_field(input, "capabilities") {
        for key in capabilities.keys() {
            let enabled = definition["capabilities"][key]["enabled"]
                .as_bool()
                .unwrap_or(false);
            if !enabled {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "capabilities", key],
                    "Capability is not enabled for this metaobject definition.",
                    "CAPABILITY_NOT_ENABLED",
                    json!(key),
                    Value::Null,
                ));
            }
        }
    }

    errors
}

fn metaobject_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
    element_key: Value,
    element_index: Value,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "elementKey": element_key,
        "elementIndex": element_index
    })
}

fn metaobject_user_error_owned(
    field: Vec<String>,
    message: &str,
    code: &str,
    element_key: Value,
    element_index: Value,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "elementKey": element_key,
        "elementIndex": element_index
    })
}

fn metaobject_display_name(definition: &Value, input_values: &BTreeMap<String, String>) -> String {
    definition
        .get("displayNameKey")
        .and_then(Value::as_str)
        .and_then(|key| input_values.get(key))
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .or_else(|| {
            input_values
                .values()
                .find(|value| !value.trim().is_empty())
                .cloned()
        })
        .unwrap_or_else(|| {
            definition["type"]
                .as_str()
                .unwrap_or("Metaobject")
                .to_string()
        })
}

fn metaobject_display_name_from_existing_values(
    definition: &Value,
    values: &BTreeMap<String, Value>,
    handle: &str,
) -> String {
    definition
        .get("displayNameKey")
        .and_then(Value::as_str)
        .and_then(|key| values.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| metaobject_display_name_from_handle(handle, definition))
}

fn metaobject_display_name_from_handle(handle: &str, definition: &Value) -> String {
    let words = handle
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>();
    if words.is_empty() {
        definition["type"]
            .as_str()
            .unwrap_or("Metaobject")
            .to_string()
    } else {
        words.join(" ")
    }
}

fn metaobject_publishable_status(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
) -> String {
    let publishable_enabled = definition["capabilities"]["publishable"]["enabled"]
        .as_bool()
        .unwrap_or(false);
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "publishable"))
        .and_then(|publishable| resolved_string_field(&publishable, "status"))
        .unwrap_or_else(|| {
            if publishable_enabled {
                "DRAFT".to_string()
            } else {
                "ACTIVE".to_string()
            }
        })
}

fn metaobject_record_from_definition(
    id: &str,
    handle: &str,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    display_name: &str,
    publishable_status: &str,
) -> Value {
    let meta_type = definition["type"].as_str().unwrap_or_default();
    let fields = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|field_definition| {
            let key = field_definition
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            metaobject_field_record_from_definition(field_definition, input_values.get(key))
        })
        .collect::<Vec<_>>();
    let title_field = definition["displayNameKey"]
        .as_str()
        .and_then(|key| {
            fields
                .iter()
                .find(|field| field.get("key").and_then(Value::as_str) == Some(key))
                .cloned()
        })
        .or_else(|| fields.first().cloned());
    json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": display_name,
        "updatedAt": "2026-01-01T00:00:00Z",
        "capabilities": {
            "publishable": {"status": publishable_status},
            "onlineStore": if definition["capabilities"]["onlineStore"]["enabled"].as_bool().unwrap_or(false) {
                json!({"templateSuffix": Value::Null})
            } else {
                Value::Null
            }
        },
        "fields": fields,
        "titleField": title_field
    })
}

fn selected_metaobject_value(value: &Value, selection: &[SelectedField]) -> Value {
    if let Some(values) = value.as_array() {
        Value::Array(
            values
                .iter()
                .map(|item| selected_json(item, selection))
                .collect(),
        )
    } else {
        nullable_selected_json(value, selection)
    }
}

fn metaobject_nodes_from_upstream_data(data: &serde_json::Map<String, Value>) -> Vec<Value> {
    let mut nodes = Vec::new();
    for value in data.values() {
        if let Some(connection_nodes) = value.get("nodes").and_then(Value::as_array) {
            nodes.extend(
                connection_nodes
                    .iter()
                    .filter(|node| node.is_object())
                    .cloned(),
            );
        }
        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            nodes.extend(
                edges
                    .iter()
                    .filter_map(|edge| edge.get("node").filter(|node| node.is_object()).cloned()),
            );
        }
    }
    nodes
}

fn standard_metaobject_template(meta_type: &str) -> Option<Value> {
    match meta_type {
        "shopify--qa-pair" => Some(json!({
            "type": "shopify--qa-pair",
            "name": "Question and Answer Pairs",
            "displayNameKey": "question",
            "standardTemplate": {
                "type": "shopify--qa-pair",
                "name": "Question and Answer Pairs"
            },
            "fieldDefinitions": [
                standard_metaobject_field_definition(
                    "question",
                    "Question",
                    true,
                    "single_line_text_field",
                    "TEXT",
                    Vec::new(),
                ),
                standard_metaobject_field_definition(
                    "answer",
                    "Answer",
                    true,
                    "multi_line_text_field",
                    "TEXT",
                    Vec::new(),
                ),
                standard_metaobject_field_definition(
                    "sources",
                    "Sources",
                    false,
                    "list.link",
                    "LINK",
                    Vec::new(),
                ),
            ]
        })),
        meta_type if meta_type.starts_with("shopify--") => None,
        _ => None,
    }
}

fn standard_metaobject_field_definition(
    key: &str,
    name: &str,
    required: bool,
    type_name: &str,
    category: &str,
    validations: Vec<Value>,
) -> Value {
    json!({
        "key": key,
        "name": name,
        "description": Value::Null,
        "required": required,
        "type": {
            "name": type_name,
            "category": category
        },
        "validations": validations
    })
}

fn metaobject_definition_type_is_reserved(meta_type: &str) -> bool {
    meta_type.starts_with("shopify--") || standard_metaobject_template(meta_type).is_some()
}

fn metaobject_field_type_allowed(field_type: &str) -> bool {
    metafield_definition_type_allowed(field_type) || field_type == "list.link"
}

fn metaobject_field_key_valid(key: &str) -> bool {
    key.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn metaobject_field_key_validation_errors(path: Vec<String>, key: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if key.trim().is_empty() {
        errors.push(metaobject_user_error_owned(
            path.clone(),
            "Key can't be blank",
            "BLANK",
            json!(key),
            Value::Null,
        ));
    }
    if key.chars().count() < 2 {
        errors.push(metaobject_user_error_owned(
            path.clone(),
            "Key is too short (minimum is 2 characters)",
            "TOO_SHORT",
            json!(key),
            Value::Null,
        ));
    }
    if !metaobject_field_key_valid(key) {
        errors.push(metaobject_user_error_owned(
            path,
            "Key contains one or more invalid characters.",
            "INVALID",
            json!(key),
            Value::Null,
        ));
    }
    errors
}

fn metaobject_definition_create_field_count(input: &BTreeMap<String, ResolvedValue>) -> usize {
    resolved_object_list_field(input, "fieldDefinitions").len()
}

fn metaobject_definition_update_created_field_count(
    input: &BTreeMap<String, ResolvedValue>,
) -> usize {
    resolved_object_list_field(input, "fieldDefinitions")
        .iter()
        .filter(|operation| resolved_object_field(operation, "create").is_some())
        .count()
}

fn metaobject_field_type_validation_error(
    path: Vec<String>,
    key: &str,
    field_type: &str,
) -> Option<Value> {
    (!metaobject_field_type_allowed(field_type)).then(|| {
        metaobject_user_error_owned(
            path,
            &format!(
                "Type name {field_type} is not a valid type. Valid types are: {}.",
                metafield_definition_valid_type_message()
            ),
            "INCLUSION",
            json!(key),
            Value::Null,
        )
    })
}

pub(in crate::proxy) fn metaobject_cursor(record: &Value) -> String {
    format!(
        "cursor:{}",
        record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("metaobject")
    )
}

impl DraftProxy {
    pub(in crate::proxy) fn has_local_metaobject_entry_state(&self) -> bool {
        !self.store.staged.metaobjects.is_empty()
            || !self.store.staged.deleted_metaobject_ids.is_empty()
    }

    pub(in crate::proxy) fn metaobject_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjects" => self.metaobject_connection(field),
                "metaobject" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_by_id(&id)
                        .map(|record| self.selected_metaobject(&record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
                "metaobjectDefinition" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_definition_by_id(&id)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectDefinitionByType" => {
                    let meta_type =
                        resolved_string_arg(&field.arguments, "type").unwrap_or_default();
                    self.metaobject_definition_by_type(&meta_type)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectDefinitions" => self.metaobject_definition_connection(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn metaobject_live_hybrid_read(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        let mut response = (self.upstream_transport)(request.clone());
        let Some(data) = response.body.get_mut("data").and_then(Value::as_object_mut) else {
            return response;
        };
        for field in fields {
            if data.contains_key(&field.response_key) {
                continue;
            }
            if let Some(value) = data.get(&field.name).cloned() {
                data.insert(field.response_key.clone(), value);
            }
        }
        let upstream_nodes = metaobject_nodes_from_upstream_data(data);
        for field in fields {
            if data.contains_key(&field.response_key) {
                continue;
            }
            let value = match field.name.as_str() {
                "metaobject" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| node.get("id").and_then(Value::as_str) == Some(&id))
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => {
                    let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
                        continue;
                    };
                    let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
                    let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| {
                            node.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                                && node.get("handle").and_then(Value::as_str)
                                    == Some(meta_handle.as_str())
                        })
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        response
    }

    pub(in crate::proxy) fn metaobject_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjectCreate" => self.metaobject_create(field, request, &mut staged_ids),
                "metaobjectDelete" => self.metaobject_delete(field, request, &mut staged_ids),
                "metaobjectDefinitionCreate" => {
                    self.metaobject_definition_create(field, &mut staged_ids)
                }
                "metaobjectDefinitionUpdate" => {
                    self.metaobject_definition_update(field, request, &mut staged_ids)
                }
                "metaobjectDefinitionDelete" => {
                    self.metaobject_definition_delete(field, &mut staged_ids)
                }
                "standardMetaobjectDefinitionEnable" => {
                    self.standard_metaobject_definition_enable(field, &mut staged_ids)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("metaobject"),
                staged_ids,
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn metaobject_by_id(&self, id: &str) -> Option<Value> {
        if self.store.staged.deleted_metaobject_ids.contains(id) {
            return None;
        }
        if let Some(record) = self.store.staged.metaobjects.get(id) {
            return Some(record.clone());
        }
        None
    }

    fn hydrate_metaobject_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return None;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": r#"
                    query MetaobjectHydrateById($id: ID!) {
                      node(id: $id) { __typename }
                      metaobject(id: $id) {
                        id
                        handle
                        type
                        displayName
                        updatedAt
                        capabilities {
                          publishable { status }
                          onlineStore { templateSuffix }
                        }
                        fields {
                          key
                          type
                          value
                          jsonValue
                          definition {
                            key
                            name
                            required
                            type { name category }
                          }
                        }
                        titleField: field(key: "title") {
                          key
                          type
                          value
                          jsonValue
                          definition {
                            key
                            name
                            required
                            type { name category }
                          }
                        }
                      }
                    }
                "#,
                "variables": {"id": id}
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        let record = response.body["data"]["metaobject"].clone();
        if !record.is_object() {
            return None;
        }
        self.store
            .staged
            .metaobjects
            .insert(id.to_string(), record.clone());
        Some(record)
    }

    pub(in crate::proxy) fn metaobject_by_handle_arg(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
            return None;
        };
        let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
        self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
            .map(|record| self.selected_metaobject(&record, &field.selection))
    }

    pub(in crate::proxy) fn metaobject_by_type_and_handle(
        &self,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .metaobjects
            .values()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("handle").and_then(Value::as_str) == Some(meta_handle)
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
    }

    pub(in crate::proxy) fn metaobject_connection(&self, field: &RootFieldSelection) -> Value {
        let meta_type = resolved_string_arg(&field.arguments, "type").unwrap_or_default();
        let mut records: Vec<Value> = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
            .collect();
        records.sort_by(|left, right| {
            left.get("id")
                .and_then(Value::as_str)
                .cmp(&right.get("id").and_then(Value::as_str))
        });
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |record, selection| self.selected_metaobject(record, selection),
            metaobject_cursor,
        )
    }

    pub(in crate::proxy) fn metaobject_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("metaobject") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = resolved_string_field(input, "type").unwrap_or_default();
        let definition = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type));
        let Some(definition) = definition else {
            let user_errors = metaobject_create_duplicate_field_errors(input);
            if !user_errors.is_empty() {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": user_errors}),
                    &field.selection,
                );
            }
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(
                        vec!["metaobject", "type"],
                        &format!("No metaobject definition exists for type \"{meta_type}\""),
                        "UNDEFINED_OBJECT_TYPE",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        };
        if definition["access"]["admin"].as_str() == Some("MERCHANT_READ") {
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(
                        vec!["metaobject", "type"],
                        "Not authorized to create metaobjects for this type.",
                        "NOT_AUTHORIZED",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        }
        let input_values = metaobject_create_input_values(input);
        let validation_errors =
            metaobject_create_validation_errors(input, &definition, &input_values);
        if !validation_errors.is_empty() {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": validation_errors}),
                &field.selection,
            );
        }

        let id = self.next_proxy_synthetic_gid("Metaobject");
        let display_name = metaobject_display_name(&definition, &input_values);
        let fallback_handle = if display_name.trim().is_empty() {
            format!("{}-{}", slugify_handle(&meta_type), resource_id_tail(&id))
        } else {
            slugify_handle(&display_name)
        };
        let requested_handle = resolved_string_field(input, "handle").unwrap_or(fallback_handle);
        let handle = self.available_metaobject_handle(&meta_type, &requested_handle);
        let publishable_status = metaobject_publishable_status(input, &definition);
        let record = metaobject_record_from_definition(
            &id,
            &handle,
            &definition,
            &input_values,
            &display_name,
            &publishable_status,
        );
        self.store.staged.deleted_metaobject_ids.remove(&id);
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        self.increment_metaobject_definition_count(&meta_type, 1);
        staged_ids.push(id);
        self.selected_metaobject_payload(
            &json!({"metaobject": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn metaobject_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self.metaobject_by_id(&id).is_none()
            && self.hydrate_metaobject_by_id(request, &id).is_none()
        {
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Record not found",
                        "code": "RECORD_NOT_FOUND",
                        "elementKey": null,
                        "elementIndex": null
                    }]
                }),
                &field.selection,
            );
        }
        let record = self.metaobject_by_id(&id).unwrap_or(Value::Null);
        self.store.staged.metaobjects.remove(&id);
        self.store.staged.deleted_metaobject_ids.insert(id.clone());
        if let Some(meta_type) = record.get("type").and_then(Value::as_str) {
            self.increment_metaobject_definition_count(meta_type, -1);
        }
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn selected_metaobject(&self, record: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "field" => {
                let key = resolved_string_arg(&field.arguments, "key").unwrap_or_default();
                let value = record["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|candidate| {
                        candidate.get("key").and_then(Value::as_str) == Some(key.as_str())
                    })
                    .cloned()
                    .unwrap_or(Value::Null);
                Some(nullable_selected_json(&value, &field.selection))
            }
            "definition" => {
                let meta_type = record
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(
                    self.metaobject_definition_by_type(meta_type)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null),
                )
            }
            _ => record
                .get(&field.name)
                .map(|value| selected_metaobject_value(value, &field.selection)),
        })
    }

    fn selected_metaobject_payload(&self, payload: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "metaobject" => {
                let metaobject = &payload["metaobject"];
                Some(if metaobject.is_null() {
                    Value::Null
                } else {
                    self.selected_metaobject(metaobject, &field.selection)
                })
            }
            _ => payload
                .get(&field.name)
                .map(|value| selected_metaobject_value(value, &field.selection)),
        })
    }

    fn metaobject_definition_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let definition_input = match field.arguments.get("definition") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobjectDefinition": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = resolved_string_field(definition_input, "type")
            .unwrap_or_default()
            .to_lowercase();
        if meta_type.is_empty() {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["definition", "type"], "Type can't be blank", "BLANK", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        }
        if self.metaobject_definition_by_type(&meta_type).is_some() {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["definition", "type"], "Type has already been taken", "TAKEN", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        }
        let id = self.next_proxy_synthetic_gid("MetaobjectDefinition");
        let definition = metaobject_definition_record(&id, definition_input, &meta_type);
        for _ in 0..metaobject_definition_create_field_count(definition_input) {
            self.next_proxy_synthetic_gid("MetaobjectFieldDefinition");
        }
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), definition.clone());
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&id);
        staged_ids.push(id);
        selected_json(
            &json!({"metaobjectDefinition": definition, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let definition_input = match field.arguments.get("definition") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobjectDefinition": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let Some(existing) = self
            .metaobject_definition_by_id(&id)
            .or_else(|| self.hydrate_metaobject_definition_by_id(request, &id))
        else {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        if self.metaobject_definition_update_is_immutable(&existing) {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(
                        vec!["definition"],
                        "Standard metaobject definitions can't be updated",
                        "IMMUTABLE",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        }
        if self.metaobject_definition_display_name_update_is_linked_immutable(
            &existing,
            definition_input,
        ) {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(
                        vec!["definition", "displayNameKey"],
                        "Cannot change display name field when metaobject is used in product options",
                        "IMMUTABLE",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        }

        let mut errors = self.metaobject_definition_update_scalar_errors(definition_input);
        let (field_errors, updated_fields) =
            self.metaobject_definition_updated_fields(&existing, definition_input);
        errors.extend(field_errors);
        if errors.is_empty() {
            let mut prospective = existing.clone();
            prospective["fieldDefinitions"] = Value::Array(updated_fields.clone());
            errors.extend(
                self.metaobject_definition_update_capability_errors(&prospective, definition_input),
            );
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"metaobjectDefinition": null, "userErrors": errors}),
                &field.selection,
            );
        }

        let mut definition = existing.clone();
        let definition_id = definition
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let old_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let old_online_store_url_handle = definition["capabilities"]["onlineStore"]["data"]
            ["urlHandle"]
            .as_str()
            .map(str::to_string);

        if let Some(name) = resolved_string_field(definition_input, "name") {
            definition["name"] = json!(name);
        }
        if definition_input.contains_key("description") {
            definition["description"] = definition_input
                .get("description")
                .and_then(resolved_value_string)
                .map_or(Value::Null, |description| json!(description));
        }
        if definition_input.contains_key("displayNameKey") {
            definition["displayNameKey"] = definition_input
                .get("displayNameKey")
                .and_then(resolved_value_string)
                .map_or(Value::Null, |display_name_key| json!(display_name_key));
        }
        if let Some(raw_type) = resolved_string_field(definition_input, "type") {
            definition["type"] =
                json!(self.resolved_metaobject_definition_type(&raw_type, request));
        }
        self.apply_metaobject_definition_access(&mut definition, definition_input);
        let create_redirects =
            self.apply_metaobject_definition_capabilities(&mut definition, definition_input);
        definition["fieldDefinitions"] = Value::Array(updated_fields);
        if definition.get("updatedAt").is_some() {
            definition["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        }
        for _ in 0..metaobject_definition_update_created_field_count(definition_input) {
            self.next_proxy_synthetic_gid("MetaobjectFieldDefinition");
        }

        self.store
            .staged
            .metaobject_definitions
            .insert(definition_id.clone(), definition.clone());
        self.reproject_metaobjects_for_definition(&definition);

        let new_online_store_url_handle = definition["capabilities"]["onlineStore"]["data"]
            ["urlHandle"]
            .as_str()
            .map(str::to_string);
        if create_redirects
            && definition["capabilities"]["onlineStore"]["enabled"]
                .as_bool()
                .unwrap_or(false)
            && old_online_store_url_handle.is_some()
            && new_online_store_url_handle.is_some()
            && old_online_store_url_handle != new_online_store_url_handle
        {
            self.stage_metaobject_definition_url_handle_redirects(
                definition
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or(&old_type),
                old_online_store_url_handle.as_deref().unwrap_or_default(),
                new_online_store_url_handle.as_deref().unwrap_or_default(),
            );
        }

        staged_ids.push(definition_id);
        selected_json(
            &json!({"metaobjectDefinition": definition, "userErrors": []}),
            &field.selection,
        )
    }

    fn standard_metaobject_definition_enable(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let meta_type = resolved_string_arg(&field.arguments, "type").unwrap_or_default();
        if let Some(existing) = self.metaobject_definition_by_type(&meta_type) {
            return selected_json(
                &json!({"metaobjectDefinition": existing, "userErrors": []}),
                &field.selection,
            );
        }
        let Some(mut definition) = standard_metaobject_template(&meta_type) else {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["type"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        let id = self.next_proxy_synthetic_gid("MetaobjectDefinition");
        definition["id"] = json!(id.clone());
        let field_count = definition["fieldDefinitions"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default();
        for _ in 0..field_count {
            self.next_proxy_synthetic_gid("MetaobjectFieldDefinition");
        }
        if resolved_bool_field(&field.arguments, "enabledByShopify").is_some() {
            definition["enabledByShopify"] =
                json!(resolved_bool_field(&field.arguments, "enabledByShopify").unwrap_or(false));
        }
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), definition.clone());
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&id);
        staged_ids.push(id);
        selected_json(
            &json!({"metaobjectDefinition": definition, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(definition) = self.metaobject_definition_by_id(&id) else {
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [metaobject_user_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        let meta_type = definition["type"].as_str().unwrap_or_default().to_string();
        let ids_to_delete = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| record.get("type").and_then(Value::as_str) == Some(meta_type.as_str()))
            .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        for metaobject_id in ids_to_delete {
            self.store.staged.metaobjects.remove(&metaobject_id);
            self.store
                .staged
                .deleted_metaobject_ids
                .insert(metaobject_id);
        }
        self.store.staged.metaobject_definitions.remove(&id);
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .insert(id.clone());
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_update_is_immutable(&self, definition: &Value) -> bool {
        definition
            .get("appConfigManaged")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || definition
                .get("standardTemplate")
                .is_some_and(|value| !value.is_null())
            || definition
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(metaobject_definition_type_is_reserved)
    }

    fn metaobject_definition_display_name_update_is_linked_immutable(
        &self,
        definition: &Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(next_display_name_key) =
            input.get("displayNameKey").and_then(resolved_value_string)
        else {
            return false;
        };
        if definition.get("displayNameKey").and_then(Value::as_str)
            == Some(next_display_name_key.as_str())
        {
            return false;
        }
        definition
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                self.store
                    .staged
                    .product_option_linked_metaobject_definition_ids
                    .contains(id)
            })
    }

    fn metaobject_definition_update_scalar_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(name) = input.get("name").and_then(resolved_value_string) {
            if name.trim().is_empty() {
                errors.push(metaobject_user_error(
                    vec!["definition", "name"],
                    "Name can't be blank",
                    "BLANK",
                    Value::Null,
                    Value::Null,
                ));
            } else if name.chars().count() > 255 {
                errors.push(metaobject_user_error(
                    vec!["definition", "name"],
                    "Name is too long (maximum is 255 characters)",
                    "TOO_LONG",
                    Value::Null,
                    Value::Null,
                ));
            }
        }
        if let Some(description) = input.get("description").and_then(resolved_value_string) {
            if description.chars().count() > 255 {
                errors.push(metaobject_user_error(
                    vec!["definition", "description"],
                    "Description is too long (maximum is 255 characters)",
                    "TOO_LONG",
                    Value::Null,
                    Value::Null,
                ));
            }
        }
        if let Some(meta_type) = resolved_string_field(input, "type") {
            let resolved_type = self.resolved_metaobject_definition_type_for_validation(&meta_type);
            if resolved_type.trim().is_empty() {
                errors.push(metaobject_user_error(
                    vec!["definition", "type"],
                    "Type can't be blank",
                    "BLANK",
                    Value::Null,
                    Value::Null,
                ));
            } else if resolved_type.chars().count() < 3 {
                errors.push(metaobject_user_error(
                    vec!["definition", "type"],
                    "Type is too short (minimum is 3 characters)",
                    "TOO_SHORT",
                    Value::Null,
                    Value::Null,
                ));
            } else if resolved_type.chars().count() > 255 {
                errors.push(metaobject_user_error(
                    vec!["definition", "type"],
                    "Type is too long (maximum is 255 characters)",
                    "TOO_LONG",
                    Value::Null,
                    Value::Null,
                ));
            } else if !resolved_type
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                errors.push(metaobject_user_error(
                    vec!["definition", "type"],
                    "Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.",
                    "INVALID",
                    Value::Null,
                    Value::Null,
                ));
            }
        }
        errors
    }

    fn metaobject_definition_updated_fields(
        &self,
        definition: &Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> (Vec<Value>, Vec<Value>) {
        let existing_fields = definition["fieldDefinitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut by_key = existing_fields
            .iter()
            .filter_map(|field| {
                field
                    .get("key")
                    .and_then(Value::as_str)
                    .map(|key| (key.to_string(), field.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let mut order = existing_fields
            .iter()
            .filter_map(|field| field.get("key").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let mut touched_order = Vec::new();
        let reset_field_order = resolved_bool_field(input, "resetFieldOrder").unwrap_or(false);
        let operations = resolved_object_list_field(input, "fieldDefinitions");
        let mut errors = Vec::new();

        for (index, operation) in operations.into_iter().enumerate() {
            let index = index.to_string();
            if let Some(create) = resolved_object_field(&operation, "create") {
                let key = resolved_string_field(&create, "key").unwrap_or_default();
                let create_path = vec![
                    "definition".to_string(),
                    "fieldDefinitions".to_string(),
                    index.clone(),
                    "create".to_string(),
                ];
                let key_errors = metaobject_field_key_validation_errors(create_path.clone(), &key);
                if !key_errors.is_empty() {
                    errors.extend(key_errors);
                    continue;
                }
                if by_key.contains_key(&key) {
                    errors.push(metaobject_user_error_owned(
                        vec![
                            "definition".to_string(),
                            "fieldDefinitions".to_string(),
                            index,
                            "create".to_string(),
                            "key".to_string(),
                        ],
                        &format!("Field definition \"{key}\" is already taken"),
                        "OBJECT_FIELD_TAKEN",
                        json!(key),
                        Value::Null,
                    ));
                    continue;
                }
                let field_type = metaobject_field_definition_type(&create);
                if let Some(error) =
                    metaobject_field_type_validation_error(create_path, &key, &field_type)
                {
                    errors.push(error);
                    continue;
                }
                by_key.insert(key.clone(), metaobject_field_definition_record(create));
                if !order.iter().any(|candidate| candidate == &key) {
                    order.push(key.clone());
                }
                if !touched_order.iter().any(|candidate| candidate == &key) {
                    touched_order.push(key);
                }
            } else if let Some(update) = resolved_object_field(&operation, "update") {
                let key = resolved_string_field(&update, "key").unwrap_or_default();
                let Some(existing) = by_key.get(&key).cloned() else {
                    errors.push(metaobject_user_error_owned(
                        vec![
                            "definition".to_string(),
                            "fieldDefinitions".to_string(),
                            index,
                            "update".to_string(),
                            "key".to_string(),
                        ],
                        &format!("Field definition \"{key}\" does not exist"),
                        "UNDEFINED_OBJECT_FIELD",
                        json!(key),
                        Value::Null,
                    ));
                    continue;
                };
                if let Some(field_type) = resolved_string_field(&update, "type") {
                    if let Some(error) = metaobject_field_type_validation_error(
                        vec![
                            "definition".to_string(),
                            "fieldDefinitions".to_string(),
                            index.clone(),
                            "update".to_string(),
                        ],
                        &key,
                        &field_type,
                    ) {
                        errors.push(error);
                        continue;
                    }
                }
                let updated = metaobject_updated_field_definition(existing, &update);
                by_key.insert(key.clone(), updated);
                if !touched_order.iter().any(|candidate| candidate == &key) {
                    touched_order.push(key);
                }
            } else if let Some(delete) = resolved_object_field(&operation, "delete") {
                let key = resolved_string_field(&delete, "key").unwrap_or_default();
                if !by_key.contains_key(&key) {
                    errors.push(metaobject_user_error_owned(
                        vec![
                            "definition".to_string(),
                            "fieldDefinitions".to_string(),
                            index,
                            "delete".to_string(),
                            "key".to_string(),
                        ],
                        &format!("Field definition \"{key}\" does not exist"),
                        "UNDEFINED_OBJECT_FIELD",
                        json!(key),
                        Value::Null,
                    ));
                    continue;
                }
                by_key.remove(&key);
                order.retain(|candidate| candidate != &key);
            }
        }

        let next_order = if reset_field_order {
            touched_order
                .into_iter()
                .filter(|key| by_key.contains_key(key))
                .chain(order.into_iter().filter(|key| by_key.contains_key(key)))
                .fold(Vec::<String>::new(), |mut acc, key| {
                    if !acc.iter().any(|candidate| candidate == &key) {
                        acc.push(key);
                    }
                    acc
                })
        } else {
            order
        };
        let updated_fields = next_order
            .into_iter()
            .filter_map(|key| by_key.remove(&key))
            .collect::<Vec<_>>();
        (errors, updated_fields)
    }

    fn metaobject_definition_update_capability_errors(
        &self,
        prospective_definition: &Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let Some(capabilities) = resolved_object_field(input, "capabilities") else {
            return errors;
        };
        let Some(renderable) = resolved_object_field(&capabilities, "renderable") else {
            return errors;
        };
        if resolved_bool_field(&renderable, "enabled") != Some(true) {
            return errors;
        }
        let Some(data) = resolved_object_field(&renderable, "data") else {
            return errors;
        };
        for data_key in ["metaTitleKey", "metaDescriptionKey"] {
            let Some(field_key) = resolved_string_field(&data, data_key) else {
                continue;
            };
            let field_definition = prospective_definition["fieldDefinitions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|field| field.get("key").and_then(Value::as_str) == Some(field_key.as_str()));
            let Some(field_definition) = field_definition else {
                errors.push(metaobject_user_error(
                    vec!["definition", "capabilities", "renderable"],
                    &format!("Field definition \"{field_key}\" does not exist"),
                    "INVALID",
                    Value::Null,
                    Value::Null,
                ));
                continue;
            };
            let field_type = field_definition["type"]["name"]
                .as_str()
                .unwrap_or_default();
            if !matches!(
                field_type,
                "single_line_text_field" | "multi_line_text_field" | "rich_text_field"
            ) {
                let shopify_key = match data_key {
                    "metaTitleKey" => "meta_title_key",
                    "metaDescriptionKey" => "meta_description_key",
                    _ => data_key,
                };
                errors.push(metaobject_user_error(
                    vec!["definition", "capabilities", "renderable"],
                    &format!(
                        "Renderable Capability \"{shopify_key}\" cannot reference the field definition \"{field_key}\" of type \"{field_type}\". Only single_line_text_field, multi_line_text_field, rich_text_field definitions are allowed."
                    ),
                    "FIELD_TYPE_INVALID",
                    Value::Null,
                    Value::Null,
                ));
            }
        }
        errors
    }

    fn apply_metaobject_definition_access(
        &self,
        definition: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let Some(access) = resolved_object_field(input, "access") else {
            return;
        };
        if !definition.get("access").is_some_and(Value::is_object) {
            definition["access"] = json!({});
        }
        for key in ["admin", "storefront", "customerAccount"] {
            if let Some(value) = resolved_string_field(&access, key) {
                definition["access"][key] = json!(value);
            }
        }
    }

    fn apply_metaobject_definition_capabilities(
        &self,
        definition: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(capabilities) = resolved_object_field(input, "capabilities") else {
            return false;
        };
        if !definition.get("capabilities").is_some_and(Value::is_object) {
            definition["capabilities"] = metaobject_definition_capabilities(&BTreeMap::new());
        }
        for key in ["publishable", "translatable", "renderable"] {
            let Some(capability) = resolved_object_field(&capabilities, key) else {
                continue;
            };
            if !definition["capabilities"]
                .get(key)
                .is_some_and(Value::is_object)
            {
                definition["capabilities"][key] = json!({"enabled": false});
            }
            if let Some(enabled) = resolved_bool_field(&capability, "enabled") {
                definition["capabilities"][key]["enabled"] = json!(enabled);
            }
            if let Some(data) = capability.get("data") {
                definition["capabilities"][key]["data"] = resolved_value_json(data);
            }
        }

        let Some(online_store) = resolved_object_field(&capabilities, "onlineStore") else {
            return false;
        };
        if !definition["capabilities"]
            .get("onlineStore")
            .is_some_and(Value::is_object)
        {
            definition["capabilities"]["onlineStore"] = json!({"enabled": false, "data": null});
        }
        if let Some(enabled) = resolved_bool_field(&online_store, "enabled") {
            definition["capabilities"]["onlineStore"]["enabled"] = json!(enabled);
            if !enabled {
                definition["capabilities"]["onlineStore"]["data"] = Value::Null;
            }
        }
        let create_redirects = resolved_object_field(&online_store, "data")
            .and_then(|data| resolved_bool_field(&data, "createRedirects"))
            .unwrap_or(false);
        if definition["capabilities"]["onlineStore"]["enabled"]
            .as_bool()
            .unwrap_or(false)
        {
            let can_create_redirects = self.metaobject_definition_active_row_count(
                definition
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ) <= 1000;
            let data = metaobject_online_store_data_from_input(&online_store, can_create_redirects);
            if !data.is_null() {
                definition["capabilities"]["onlineStore"]["data"] = data;
            }
        }
        create_redirects
    }

    fn resolved_metaobject_definition_type(&self, raw_type: &str, request: &Request) -> String {
        if let Some(suffix) = raw_type.strip_prefix("$app:") {
            let client_id = request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .map(String::as_str)
                .unwrap_or("347082227713");
            format!("app--{client_id}--{suffix}").to_lowercase()
        } else {
            raw_type.to_lowercase()
        }
    }

    fn resolved_metaobject_definition_type_for_validation(&self, raw_type: &str) -> String {
        if let Some(suffix) = raw_type.strip_prefix("$app:") {
            format!("app--347082227713--{suffix}").to_lowercase()
        } else {
            raw_type.to_lowercase()
        }
    }

    fn metaobject_definition_by_id(&self, id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_metaobject_definition_ids
            .contains(id)
        {
            return None;
        }
        self.store.staged.metaobject_definitions.get(id).cloned()
    }

    fn metaobject_definition_by_type(&self, meta_type: &str) -> Option<Value> {
        self.store
            .staged
            .metaobject_definitions
            .values()
            .find(|definition| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_definition_ids
                        .contains(
                            definition
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        )
            })
            .cloned()
    }

    fn hydrate_metaobject_definition_by_id(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || id.trim().is_empty() {
            return None;
        }
        let query = "query MetaobjectDefinitionHydrateById($id: ID!) { metaobjectDefinition(id: $id) { id type name description displayNameKey access { admin storefront customerAccount } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled data { urlHandle canCreateRedirects } } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }";
        let body = serde_json::to_string(&json!({
            "query": query,
            "variables": {"id": id}
        }))
        .ok()?;
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body,
        });
        if response.status < 200 || response.status >= 300 {
            return None;
        }
        let definition = response
            .body
            .get("data")
            .and_then(|data| data.get("metaobjectDefinition"))
            .filter(|definition| definition.is_object())?
            .clone();
        let definition_id = definition
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&definition_id);
        self.store
            .staged
            .metaobject_definitions
            .insert(definition_id, definition.clone());
        Some(definition)
    }

    fn hydrate_metaobject_definition_by_type(
        &mut self,
        request: &Request,
        meta_type: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || meta_type.trim().is_empty() {
            return None;
        }
        let query = "query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }";
        let body = serde_json::to_string(&json!({
            "query": query,
            "variables": {"type": meta_type}
        }))
        .ok()?;
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body,
        });
        if response.status < 200 || response.status >= 300 {
            return None;
        }
        let definition = response
            .body
            .get("data")
            .and_then(|data| data.get("metaobjectDefinitionByType"))
            .filter(|definition| definition.is_object())?
            .clone();
        let id = definition
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return Some(definition);
        }
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&id);
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition.clone());
        Some(definition)
    }

    fn metaobject_definition_connection(&self, field: &RootFieldSelection) -> Value {
        let mut records = self
            .store
            .staged
            .metaobject_definitions
            .values()
            .filter(|definition| {
                !self
                    .store
                    .staged
                    .deleted_metaobject_definition_ids
                    .contains(
                        definition
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.get("type")
                .and_then(Value::as_str)
                .cmp(&right.get("type").and_then(Value::as_str))
        });
        selected_connection_json_with_args(
            records,
            &field.arguments,
            &field.selection,
            |definition| {
                format!(
                    "cursor:{}",
                    definition
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("metaobject-definition")
                )
            },
        )
    }

    fn increment_metaobject_definition_count(&mut self, meta_type: &str, delta: i64) {
        let Some((id, mut definition)) = self
            .store
            .staged
            .metaobject_definitions
            .iter()
            .find(|(_, definition)| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
            })
            .map(|(id, definition)| (id.clone(), definition.clone()))
        else {
            return;
        };
        let current = definition
            .get("metaobjectsCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        definition["metaobjectsCount"] = json!((current + delta).max(0));
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition);
    }

    fn available_metaobject_handle(&self, meta_type: &str, requested: &str) -> String {
        let base = slugify_handle(requested);
        let base = if base.is_empty() {
            format!("{meta_type}-{}", self.next_synthetic_id)
        } else {
            base
        };
        if self
            .metaobject_by_type_and_handle(meta_type, &base)
            .is_none()
        {
            return base;
        }
        for suffix in 1.. {
            let candidate = format!("{base}-{suffix}");
            if self
                .metaobject_by_type_and_handle(meta_type, &candidate)
                .is_none()
            {
                return candidate;
            }
        }
        unreachable!("infinite suffix search must return")
    }

    fn metaobject_definition_active_row_count(&self, meta_type: &str) -> usize {
        self.store
            .staged
            .metaobjects
            .values()
            .filter(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
                    && record["capabilities"]["publishable"]["status"].as_str() == Some("ACTIVE")
            })
            .count()
    }

    fn reproject_metaobjects_for_definition(&mut self, definition: &Value) {
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let ids = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| record.get("type").and_then(Value::as_str) == Some(meta_type.as_str()))
            .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        for id in ids {
            let Some(mut record) = self.store.staged.metaobjects.get(&id).cloned() else {
                continue;
            };
            let handle = record
                .get("handle")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let values = record["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|field| {
                    field.get("key").and_then(Value::as_str).map(|key| {
                        (
                            key.to_string(),
                            field.get("value").cloned().unwrap_or(Value::Null),
                        )
                    })
                })
                .collect::<BTreeMap<_, _>>();
            let fields = definition["fieldDefinitions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|field_definition| {
                    let key = field_definition
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    metaobject_field_record_from_existing_value(field_definition, values.get(key))
                })
                .collect::<Vec<_>>();
            let title_field = definition["displayNameKey"]
                .as_str()
                .and_then(|key| {
                    fields
                        .iter()
                        .find(|field| field.get("key").and_then(Value::as_str) == Some(key))
                        .cloned()
                })
                .or_else(|| fields.first().cloned());
            record["fields"] = Value::Array(fields);
            record["titleField"] = title_field.unwrap_or(Value::Null);
            record["displayName"] = json!(metaobject_display_name_from_existing_values(
                definition, &values, &handle
            ));
            record["capabilities"]["onlineStore"] = if definition["capabilities"]["onlineStore"]
                ["enabled"]
                .as_bool()
                .unwrap_or(false)
            {
                json!({"templateSuffix": Value::Null})
            } else {
                Value::Null
            };
            self.store.staged.metaobjects.insert(id, record);
        }
    }

    fn stage_metaobject_definition_url_handle_redirects(
        &mut self,
        meta_type: &str,
        old_url_handle: &str,
        new_url_handle: &str,
    ) {
        let rows = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| record.get("type").and_then(Value::as_str) == Some(meta_type))
            .filter(|record| {
                record["capabilities"]["publishable"]["status"].as_str() == Some("ACTIVE")
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .filter_map(|record| {
                record
                    .get("handle")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        for handle in rows {
            let path = format!(
                "/pages/{}/{}",
                old_url_handle.trim_matches('/'),
                handle.trim_matches('/')
            );
            let target = format!(
                "/pages/{}/{}",
                new_url_handle.trim_matches('/'),
                handle.trim_matches('/')
            );
            if self.store.staged.url_redirects.values().any(|redirect| {
                redirect.get("path").and_then(Value::as_str) == Some(path.as_str())
                    && redirect.get("target").and_then(Value::as_str) == Some(target.as_str())
            }) {
                continue;
            }
            let id = self.next_proxy_synthetic_gid("UrlRedirect");
            self.store.staged.url_redirect_order.push(id.clone());
            self.store.staged.url_redirects.insert(
                id.clone(),
                json!({
                    "id": id,
                    "path": path,
                    "target": target
                }),
            );
        }
    }

    pub(in crate::proxy) fn url_redirect_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "urlRedirect" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .url_redirects
                        .get(&id)
                        .map(|redirect| selected_json(redirect, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "urlRedirects" => self.url_redirect_connection(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn url_redirect_connection(&self, field: &RootFieldSelection) -> Value {
        let query = resolved_string_arg(&field.arguments, "query");
        let mut records = self
            .store
            .staged
            .url_redirect_order
            .iter()
            .filter_map(|id| self.store.staged.url_redirects.get(id))
            .filter(|redirect| {
                query
                    .as_deref()
                    .is_none_or(|query| url_redirect_matches_query(redirect, query))
            })
            .cloned()
            .collect::<Vec<_>>();
        if records.is_empty() && self.store.staged.url_redirect_order.is_empty() {
            records = self
                .store
                .staged
                .url_redirects
                .values()
                .filter(|redirect| {
                    query
                        .as_deref()
                        .is_none_or(|query| url_redirect_matches_query(redirect, query))
                })
                .cloned()
                .collect();
        }
        selected_connection_json_with_args(
            records,
            &field.arguments,
            &field.selection,
            |redirect| {
                redirect
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("url-redirect")
                    .to_string()
            },
        )
    }
}

fn url_redirect_matches_query(redirect: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if let Some(path) = query.strip_prefix("path:") {
        let path = path.trim_matches('"').trim_matches('\'');
        return redirect.get("path").and_then(Value::as_str) == Some(path);
    }
    redirect
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| path.contains(query))
        || redirect
            .get("target")
            .and_then(Value::as_str)
            .is_some_and(|target| target.contains(query))
}

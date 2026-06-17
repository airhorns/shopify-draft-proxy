use super::*;

fn source_location_for_token_after(
    query: &str,
    start: SourceLocation,
    token: &str,
) -> Option<SourceLocation> {
    for (line_index, line) in query.lines().enumerate().skip(start.line.saturating_sub(1)) {
        let start_column = if line_index + 1 == start.line {
            start.column.saturating_sub(1)
        } else {
            0
        };
        let Some(search_slice) = line.get(start_column..) else {
            continue;
        };
        if let Some(offset) = search_slice.find(token) {
            return Some(SourceLocation {
                line: line_index + 1,
                column: start_column + offset + 1,
            });
        }
    }
    None
}

fn metaobject_bulk_delete_selector_error(field: &RootFieldSelection, query: &str) -> Option<Value> {
    let where_input = field.arguments.get("where").and_then(|value| match value {
        ResolvedValue::Object(input) => Some(input),
        _ => None,
    });
    let ids_present = where_input.is_some_and(|input| input.contains_key("ids"))
        || field.arguments.contains_key("ids");
    let type_present = where_input
        .and_then(|input| resolved_string_field(input, "type"))
        .is_some_and(|value| !value.is_empty());

    if ids_present != type_present {
        return None;
    }

    let mut locations = vec![json!({
        "line": field.location.line,
        "column": field.location.column
    })];
    if ids_present {
        if let Some(location) = source_location_for_token_after(query, field.location, "ids") {
            locations.push(json!({
                "line": location.line,
                "column": location.column
            }));
        }
    }

    Some(json!({
        "message": "MetaobjectBulkDeleteWhereCondition requires exactly one of type, ids",
        "locations": locations,
        "extensions": {"code": "INVALID_FIELD_ARGUMENTS"},
        "path": [field.response_key.clone()]
    }))
}

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
    let name = resolved_string_field(input, "name").unwrap_or_default();
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
        "access": metaobject_definition_access(input, meta_type),
        "capabilities": metaobject_definition_capabilities(input),
        "fieldDefinitions": field_definitions,
        "hasThumbnailField": false,
        "metaobjectsCount": 0,
        "standardTemplate": Value::Null,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z"
    })
}

fn metaobject_definition_from_record(record: &Value) -> Option<Value> {
    let meta_type = record.get("type").and_then(Value::as_str)?;
    let field_definitions = record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| {
            field
                .get("definition")
                .filter(|definition| definition.is_object())
        })
        .cloned()
        .collect::<Vec<_>>();
    if field_definitions.is_empty() {
        return None;
    }
    let display_name_key = record["titleField"]["key"]
        .as_str()
        .or_else(|| {
            field_definitions
                .first()?
                .get("key")
                .and_then(Value::as_str)
        })
        .map_or(Value::Null, |key| json!(key));
    Some(json!({
        "id": Value::Null,
        "type": meta_type,
        "name": metaobject_field_name(meta_type),
        "description": Value::Null,
        "displayNameKey": display_name_key,
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"},
        "capabilities": {
            "publishable": {"enabled": !record["capabilities"]["publishable"].is_null()},
            "onlineStore": {"enabled": !record["capabilities"]["onlineStore"].is_null(), "data": Value::Null},
            "renderable": {"enabled": false},
            "translatable": {"enabled": false}
        },
        "fieldDefinitions": field_definitions,
        "hasThumbnailField": false,
        "metaobjectsCount": Value::Null,
        "standardTemplate": Value::Null,
        "createdAt": Value::Null,
        "updatedAt": Value::Null
    }))
}

fn metaobject_definition_access(input: &BTreeMap<String, ResolvedValue>, meta_type: &str) -> Value {
    let access = resolved_object_field(input, "access").unwrap_or_default();
    let admin = match resolved_string_field(&access, "admin").as_deref() {
        Some("MERCHANT_READ_WRITE") if metaobject_definition_is_app_reserved_type(meta_type) => {
            "MERCHANT_READ_WRITE"
        }
        Some("MERCHANT_READ_WRITE") | Some("PUBLIC_READ_WRITE") => "PUBLIC_READ_WRITE",
        Some("PUBLIC_READ") | Some("MERCHANT_READ") => "MERCHANT_READ",
        _ => "PUBLIC_READ_WRITE",
    };
    json!({
        "admin": admin,
        "storefront": resolved_string_field(&access, "storefront").unwrap_or_else(|| "NONE".to_string()),
        "customerAccount": resolved_string_field(&access, "customerAccount").unwrap_or_else(|| "NONE".to_string())
    })
}

fn metaobject_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let publishable = resolved_object_field(&capabilities, "publishable")
        .and_then(|publishable| resolved_bool_field(&publishable, "enabled"))
        .unwrap_or(false);
    let online_store = resolved_object_field(&capabilities, "onlineStore")
        .and_then(|online_store| resolved_bool_field(&online_store, "enabled"))
        .unwrap_or(false);
    let renderable = resolved_object_field(&capabilities, "renderable")
        .and_then(|renderable| resolved_bool_field(&renderable, "enabled"))
        .unwrap_or(false);
    let translatable = resolved_object_field(&capabilities, "translatable")
        .and_then(|translatable| resolved_bool_field(&translatable, "enabled"))
        .unwrap_or(false);
    json!({
        "publishable": {"enabled": publishable},
        "onlineStore": {"enabled": online_store, "data": Value::Null},
        "renderable": {"enabled": renderable},
        "translatable": {"enabled": translatable}
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
        "capabilities": metaobject_field_definition_capabilities(&input),
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

fn metaobject_field_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let admin_filterable = resolved_object_field(&capabilities, "adminFilterable")
        .and_then(|admin_filterable| resolved_bool_field(&admin_filterable, "enabled"))
        .unwrap_or(false);
    json!({
        "adminFilterable": {"enabled": admin_filterable}
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

fn metaobject_field_type_category(field_type: &str) -> &'static str {
    match field_type {
        "number_integer" | "number_decimal" => "NUMBER",
        "boolean" => "TRUE_FALSE",
        "date" | "date_time" => "DATE_TIME",
        "json" | "rich_text_field" => "JSON",
        value if value.ends_with("_reference") || value.starts_with("list.") => "REFERENCE",
        _ => "TEXT",
    }
}

fn metaobject_definition_type_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    request: &Request,
) -> String {
    canonical_metaobject_definition_type(
        &resolved_string_field(input, "type").unwrap_or_default(),
        request,
    )
}

fn resolved_metaobject_definition_type_arg(
    value: Option<&ResolvedValue>,
    request: &Request,
) -> String {
    canonical_metaobject_definition_type(
        &value.and_then(resolved_value_string).unwrap_or_default(),
        request,
    )
}

fn canonical_metaobject_definition_type(raw: &str, request: &Request) -> String {
    let resolved = if let Some(suffix) = raw.strip_prefix("$app:") {
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "347082227713".to_string());
        format!("app--{api_client_id}--{suffix}")
    } else {
        raw.to_string()
    };
    resolved.to_lowercase()
}

fn metaobject_definition_type_token_chars_valid(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn metaobject_definition_field_key_chars_valid(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn metaobject_definition_is_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("shopify--")
}

fn metaobject_definition_is_app_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("app--")
}

fn metaobject_definition_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    existing_definitions: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if metaobject_definition_is_reserved_type(meta_type) {
        errors.push(metaobject_user_error(
            vec!["definition"],
            "Not authorized. This type is reserved for use by another application.",
            "NOT_AUTHORIZED",
            Value::Null,
            Value::Null,
        ));
        return errors;
    }

    let name = resolved_string_field(input, "name").unwrap_or_default();
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

    if meta_type.is_empty() {
        errors.push(metaobject_user_error(
            vec!["definition", "type"],
            "Type can't be blank",
            "BLANK",
            Value::Null,
            Value::Null,
        ));
    } else if meta_type.chars().count() < 3 {
        errors.push(metaobject_user_error(
            vec!["definition", "type"],
            "Type is too short (minimum is 3 characters)",
            "TOO_SHORT",
            Value::Null,
            Value::Null,
        ));
    } else if meta_type.chars().count() > 255 {
        errors.push(metaobject_user_error(
            vec!["definition", "type"],
            "Type is too long (maximum is 255 characters)",
            "TOO_LONG",
            Value::Null,
            Value::Null,
        ));
    } else if !metaobject_definition_type_token_chars_valid(meta_type) {
        errors.push(metaobject_user_error(
            vec!["definition", "type"],
            "Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.",
            "INVALID",
            Value::Null,
            Value::Null,
        ));
    }

    if let Some(description) = resolved_string_field(input, "description") {
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

    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").is_some()
            && !metaobject_definition_is_app_reserved_type(meta_type)
        {
            errors.push(metaobject_user_error(
                vec!["definition", "access", "admin"],
                "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
                "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
                Value::Null,
                Value::Null,
            ));
        }
    }

    let field_definitions = resolved_object_list_field(input, "fieldDefinitions");
    if field_definitions.len() > 40 {
        errors.push(metaobject_user_error(
            vec!["definition", "fieldDefinitions"],
            "Maximum 40 fields per metaobject definition",
            "INVALID",
            Value::Null,
            Value::Null,
        ));
    }

    let admin_filterable_count = field_definitions
        .iter()
        .filter(|definition| {
            resolved_object_field(definition, "capabilities")
                .and_then(|capabilities| resolved_object_field(&capabilities, "adminFilterable"))
                .and_then(|admin_filterable| resolved_bool_field(&admin_filterable, "enabled"))
                .unwrap_or(false)
        })
        .count();
    if admin_filterable_count > 40 {
        errors.push(metaobject_user_error(
            vec!["definition", "fieldDefinitions"],
            "Maximum 40 admin filterable fields per metaobject definition",
            "INVALID",
            Value::Null,
            Value::Null,
        ));
    }

    let mut seen_keys = BTreeSet::new();
    for (index, field_definition) in field_definitions.iter().enumerate() {
        let key = resolved_string_field(field_definition, "key").unwrap_or_default();
        let index_string = index.to_string();
        if matches!(key.as_str(), "id" | "handle" | "system" | "metafields") {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                &format!("The name \"{key}\" is reserved for system use"),
                "RESERVED_NAME",
                json!(key),
                Value::Null,
            ));
            continue;
        }
        if key.chars().count() < 2 {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                "Key is too short (minimum is 2 characters)",
                "TOO_SHORT",
                json!(key),
                Value::Null,
            ));
            continue;
        }
        if !metaobject_definition_field_key_chars_valid(&key) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                "Key contains one or more invalid characters. Only lowercase alphanumeric characters, underscores, and dashes are allowed.",
                "INVALID",
                json!(key),
                Value::Null,
            ));
            continue;
        }
        if !seen_keys.insert(key.clone()) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                &format!("Field \"{key}\" duplicates other inputs"),
                "DUPLICATE_FIELD_INPUT",
                json!(key),
                Value::Null,
            ));
            continue;
        }
        let field_type = metaobject_field_definition_type(field_definition);
        if !metafield_definition_type_allowed(&field_type) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                &format!(
                    "Type name {field_type} is not a valid type. Valid types are: {}.",
                    metafield_definition_valid_type_message()
                ),
                "INCLUSION",
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(display_name_key) = resolved_string_field(input, "displayNameKey") {
        if !field_definitions.iter().any(|definition| {
            resolved_string_field(definition, "key") == Some(display_name_key.clone())
        }) {
            errors.push(metaobject_user_error(
                vec!["definition", "displayNameKey"],
                &format!("Field definition \"{display_name_key}\" does not exist"),
                "UNDEFINED_OBJECT_FIELD",
                Value::Null,
                Value::Null,
            ));
        }
    }

    if existing_definitions >= 128 {
        errors.push(metaobject_user_error(
            vec!["definition"],
            "Maximum number of metaobject definitions exceeded",
            "MAX_DEFINITIONS_EXCEEDED",
            Value::Null,
            Value::Null,
        ));
    }

    errors
}

fn metaobject_definition_update_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(name) = resolved_string_field(input, "name") {
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
    if let Some(description) = resolved_string_field(input, "description") {
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
    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").is_some()
            && !metaobject_definition_is_app_reserved_type(meta_type)
        {
            errors.push(metaobject_user_error(
                vec!["definition", "access", "admin"],
                "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
                "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
                Value::Null,
                Value::Null,
            ));
        }
    }
    errors
}

fn update_metaobject_definition_record(
    mut definition: Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    if let Some(name) = resolved_string_field(input, "name") {
        definition["name"] = json!(name);
    }
    if input.contains_key("description") {
        definition["description"] = input
            .get("description")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |description| json!(description));
    }
    if input.contains_key("displayNameKey") {
        definition["displayNameKey"] = input
            .get("displayNameKey")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |display_name_key| json!(display_name_key));
    }
    if let Some(access_input) = resolved_object_field(input, "access") {
        let mut access = definition
            .get("access")
            .cloned()
            .unwrap_or_else(|| json!({"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"}));
        if let Some(admin) = resolved_string_field(&access_input, "admin") {
            access["admin"] = json!(admin);
        }
        if let Some(storefront) = resolved_string_field(&access_input, "storefront") {
            access["storefront"] = json!(storefront);
        }
        if let Some(customer_account) = resolved_string_field(&access_input, "customerAccount") {
            access["customerAccount"] = json!(customer_account);
        }
        definition["access"] = access;
    }
    definition
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

fn metaobject_input_field_by_key(
    input: &BTreeMap<String, ResolvedValue>,
    target_key: &str,
) -> Option<(usize, BTreeMap<String, ResolvedValue>)> {
    let Some(ResolvedValue::List(fields)) = input.get("fields") else {
        return None;
    };
    fields.iter().enumerate().find_map(|(index, field)| {
        let ResolvedValue::Object(field) = field else {
            return None;
        };
        (resolved_string_field(field, "key").as_deref() == Some(target_key))
            .then(|| (index, field.clone()))
    })
}

fn metaobject_existing_field_values(record: &Value) -> BTreeMap<String, String> {
    record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| {
            Some((
                field.get("key")?.as_str()?.to_string(),
                field
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect()
}

fn metaobject_merged_input_values(
    record: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, String> {
    let mut values = metaobject_existing_field_values(record);
    values.extend(metaobject_create_input_values(input));
    values
}

fn metaobject_handle_validation_errors(handle: &str, field: Vec<&str>) -> Vec<Value> {
    let mut errors = Vec::new();
    if handle.is_empty() {
        errors.push(metaobject_user_error(
            field.clone(),
            "Handle can't be blank",
            "BLANK",
            Value::Null,
            Value::Null,
        ));
    }
    if handle.len() > 255 {
        errors.push(metaobject_user_error(
            field.clone(),
            "Handle is too long (maximum is 255 characters)",
            "TOO_LONG",
            Value::Null,
            Value::Null,
        ));
    }
    if handle.is_empty()
        || handle.starts_with('-')
        || handle.ends_with('-')
        || !handle
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        errors.push(metaobject_user_error(
            field,
            "Handle is invalid",
            "INVALID",
            Value::Null,
            Value::Null,
        ));
    }
    errors
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

fn metaobject_updated_publishable_status(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    existing: &Value,
) -> String {
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "publishable"))
        .and_then(|publishable| resolved_string_field(&publishable, "status"))
        .or_else(|| {
            existing["capabilities"]["publishable"]["status"]
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_else(|| metaobject_publishable_status(input, definition))
}

fn metaobject_required_field_errors_for_upsert(
    errors: Vec<Value>,
    definition: &Value,
) -> Vec<Value> {
    errors
        .into_iter()
        .map(|mut error| {
            if error.get("code").and_then(Value::as_str) == Some("OBJECT_FIELD_REQUIRED") {
                let key = error
                    .get("elementKey")
                    .and_then(Value::as_str)
                    .or_else(|| definition.get("displayNameKey").and_then(Value::as_str))
                    .unwrap_or("field")
                    .to_string();
                error["field"] = json!([]);
                error["message"] = json!(format!("{} can't be blank", metaobject_field_name(&key)));
            }
            error
        })
        .collect()
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

fn metaobject_record_from_definition_with_options(
    id: &str,
    handle: &str,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    display_name: &str,
    publishable_status: &str,
    updated_at: &str,
) -> Value {
    let mut record = metaobject_record_from_definition(
        id,
        handle,
        definition,
        input_values,
        display_name,
        publishable_status,
    );
    record["updatedAt"] = json!(updated_at);
    if !definition["capabilities"]["publishable"]["enabled"]
        .as_bool()
        .unwrap_or(false)
    {
        record["capabilities"]["publishable"] = Value::Null;
    }
    record
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

    pub(in crate::proxy) fn metaobject_query_data(
        &self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Value {
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
                    let meta_type = resolved_metaobject_definition_type_arg(
                        field.arguments.get("type"),
                        request,
                    );
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
        let mut errors = Vec::new();
        for field in fields {
            if field.name == "metaobjectBulkDelete" {
                if let Some(error) = metaobject_bulk_delete_selector_error(field, query) {
                    errors.push(error);
                    data.insert(field.response_key.clone(), Value::Null);
                    continue;
                }
            }
            let value = match field.name.as_str() {
                "metaobjectCreate" => self.metaobject_create(field, request, &mut staged_ids),
                "metaobjectUpdate" => self.metaobject_update(field, request, &mut staged_ids),
                "metaobjectUpsert" => self.metaobject_upsert(field, request, &mut staged_ids),
                "metaobjectDelete" => self.metaobject_delete(field, request, &mut staged_ids),
                "metaobjectBulkDelete" => {
                    self.metaobject_bulk_delete(field, request, &mut staged_ids)
                }
                "metaobjectDefinitionCreate" => {
                    self.metaobject_definition_create(field, request, &mut staged_ids)
                }
                "metaobjectDefinitionUpdate" => {
                    self.metaobject_definition_update(field, &mut staged_ids)
                }
                "metaobjectDefinitionDelete" => {
                    self.metaobject_definition_delete(field, &mut staged_ids)
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
        let mut body = json!({"data": Value::Object(data)});
        if !errors.is_empty() {
            body["errors"] = Value::Array(errors);
        }
        ok_json(body)
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

    pub(in crate::proxy) fn metaobject_definition_update_targets_local_definition(
        &self,
        field: &RootFieldSelection,
    ) -> bool {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        self.store.staged.metaobject_definitions.contains_key(&id)
            && !self
                .store
                .staged
                .deleted_metaobject_definition_ids
                .contains(&id)
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
                "query": "query MetaobjectHydrateById($id: ID!) { node(id: $id) { __typename } metaobject(id: $id) { id handle type displayName updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } titleField: field(key: \"title\") { key type value jsonValue definition { key name required type { name category } } } } }",
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

    fn hydrate_metaobject_by_handle(
        &mut self,
        request: &Request,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || meta_type.is_empty()
            || meta_handle.is_empty()
        {
            return None;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": "query MetaobjectHydrateByHandle($type: String!, $handle: String!) { metaobjectByHandle(handle: { type: $type, handle: $handle }) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }",
                "variables": {"type": meta_type, "handle": meta_handle}
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        let mut record = response.body["data"]["metaobjectByHandle"].clone();
        if !record.is_object() {
            return None;
        }
        if let Some(definition) = record
            .get("definition")
            .filter(|definition| definition.is_object())
        {
            if let Some(definition_id) = definition.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .deleted_metaobject_definition_ids
                    .remove(definition_id);
                self.store
                    .staged
                    .metaobject_definitions
                    .insert(definition_id.to_string(), definition.clone());
            }
        }
        if let Some(record_object) = record.as_object_mut() {
            record_object.remove("definition");
        }
        let id = record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return Some(record);
        }
        self.store.staged.deleted_metaobject_ids.remove(id.as_str());
        self.store.staged.metaobjects.insert(id, record.clone());
        Some(record)
    }

    fn hydrate_metaobjects_by_type(&mut self, request: &Request, meta_type: &str) -> Vec<Value> {
        if self.config.read_mode == ReadMode::Snapshot || meta_type.is_empty() {
            return Vec::new();
        }
        let query = "#graphql
  query MetaobjectBulkDeleteHydrateByType($type: String!) {
    catalog: metaobjects(type: $type, first: 250) {
      nodes {
        id
        handle
        type
        displayName
        createdAt
        updatedAt
        capabilities {
          publishable {
            status
          }
          onlineStore {
            templateSuffix
          }
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
            type {
              name
              category
            }
          }
        }
      }
    }
    definition: metaobjectDefinitionByType(type: $type) {
      id
      type
      name
      description
      displayNameKey
      access {
        admin
        storefront
      }
      capabilities {
        publishable {
          enabled
        }
        translatable {
          enabled
        }
        renderable {
          enabled
        }
        onlineStore {
          enabled
        }
      }
      fieldDefinitions {
        key
        name
        description
        required
        type {
          name
          category
        }
        validations {
          name
          value
        }
      }
      hasThumbnailField
      metaobjectsCount
      standardTemplate {
        type
        name
      }
      createdAt
      updatedAt
    }
  }
";
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": query,
                "variables": {"type": meta_type}
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        if let Some(definition) = response.body["data"]
            .get("definition")
            .or_else(|| response.body["data"].get("metaobjectDefinitionByType"))
            .filter(|value| value.is_object())
            .cloned()
        {
            if let Some(id) = definition.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .deleted_metaobject_definition_ids
                    .remove(id);
                self.store
                    .staged
                    .metaobject_definitions
                    .insert(id.to_string(), definition);
            }
        }
        let nodes = response.body["data"]
            .get("catalog")
            .or_else(|| response.body["data"].get("metaobjects"))
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for record in &nodes {
            if let Some(id) = record.get("id").and_then(Value::as_str) {
                self.store.staged.deleted_metaobject_ids.remove(id);
                self.store
                    .staged
                    .metaobjects
                    .insert(id.to_string(), record.clone());
            }
        }
        nodes
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
        self.stage_metaobject_create_from_input(input, request, staged_ids, &field.selection, false)
    }

    fn stage_metaobject_create_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        staged_ids: &mut Vec<String>,
        selection: &[SelectedField],
        upsert_required_errors: bool,
    ) -> Value {
        let meta_type = resolved_string_field(input, "type").unwrap_or_default();
        let definition = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type));
        let Some(definition) = definition else {
            let user_errors = metaobject_create_duplicate_field_errors(input);
            if !user_errors.is_empty() {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": user_errors}),
                    selection,
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
                selection,
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
                selection,
            );
        }
        let input_values = metaobject_create_input_values(input);
        let mut validation_errors =
            metaobject_create_validation_errors(input, &definition, &input_values);
        if upsert_required_errors {
            validation_errors =
                metaobject_required_field_errors_for_upsert(validation_errors, &definition);
        }
        if !validation_errors.is_empty() {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": validation_errors}),
                selection,
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
        let record = metaobject_record_from_definition_with_options(
            &id,
            &handle,
            &definition,
            &input_values,
            &display_name,
            &publishable_status,
            "2026-01-01T00:00:00Z",
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
            selection,
        )
    }

    fn metaobject_display_name_conflict_errors(
        &self,
        existing_id: &str,
        definition: &Value,
        input: &BTreeMap<String, ResolvedValue>,
        input_values: &BTreeMap<String, String>,
    ) -> Vec<Value> {
        let display_name_key = definition
            .get("displayNameKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if display_name_key.is_empty() {
            return Vec::new();
        }
        let Some((field_index, _)) = metaobject_input_field_by_key(input, display_name_key) else {
            return Vec::new();
        };
        let display_name = metaobject_display_name(definition, input_values);
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conflicts_linked_option_value = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| {
                record.get("id").and_then(Value::as_str) != Some(existing_id)
                    && record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("displayName").and_then(Value::as_str)
                        == Some(display_name.as_str())
            })
            .filter_map(|record| record.get("id").and_then(Value::as_str))
            .any(|other_id| {
                self.store
                    .staged
                    .linked_product_option_metaobject_sets
                    .iter()
                    .any(|ids| ids.contains(existing_id) && ids.contains(other_id))
            });
        if !conflicts_linked_option_value {
            return Vec::new();
        }
        let index = field_index.to_string();
        vec![metaobject_user_error(
            vec!["metaobject", "fields", &index],
            "The display name you have chosen is already in use as an option value. Choose a different name to avoid conflicts.",
            "DISPLAY_NAME_CONFLICT",
            Value::Null,
            Value::Null,
        )]
    }

    pub(in crate::proxy) fn metaobject_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .metaobject_by_id(&id)
            .or_else(|| self.hydrate_metaobject_by_id(request, &id))
        else {
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        let input = match field.arguments.get("metaobject") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": existing, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = existing
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(definition) = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type))
            .or_else(|| metaobject_definition_from_record(&existing))
        else {
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

        let input_values = metaobject_merged_input_values(&existing, input);
        let mut validation_errors =
            metaobject_create_validation_errors(input, &definition, &input_values);
        validation_errors.extend(self.metaobject_display_name_conflict_errors(
            &id,
            &definition,
            input,
            &input_values,
        ));
        let requested_handle = resolved_string_field(input, "handle");
        let next_handle = if let Some(handle) = requested_handle.as_deref() {
            validation_errors.extend(metaobject_handle_validation_errors(
                handle,
                vec!["metaobject", "handle"],
            ));
            let normalized = slugify_handle(handle);
            if self
                .metaobject_by_type_and_handle(&meta_type, &normalized)
                .as_ref()
                .and_then(|record| record.get("id").and_then(Value::as_str))
                .is_some_and(|other_id| other_id != id)
            {
                validation_errors.push(metaobject_user_error(
                    vec!["metaobject", "handle"],
                    "Handle has already been taken",
                    "TAKEN",
                    Value::Null,
                    Value::Null,
                ));
            }
            normalized
        } else {
            existing
                .get("handle")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if !validation_errors.is_empty() {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": validation_errors}),
                &field.selection,
            );
        }

        let display_name = metaobject_display_name(&definition, &input_values);
        let publishable_status =
            metaobject_updated_publishable_status(input, &definition, &existing);
        let record = metaobject_record_from_definition(
            &id,
            &next_handle,
            &definition,
            &input_values,
            &display_name,
            &publishable_status,
        );
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        self.selected_metaobject_payload(
            &json!({"metaobject": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn metaobject_upsert(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": []}),
                &field.selection,
            );
        };
        let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
        let Some(input) = field
            .arguments
            .get("metaobject")
            .and_then(|value| match value {
                ResolvedValue::Object(input) => Some(input),
                _ => None,
            })
        else {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": []}),
                &field.selection,
            );
        };
        if let Some(existing) = self
            .metaobject_by_type_and_handle(&meta_type, &meta_handle)
            .or_else(|| self.hydrate_metaobject_by_handle(request, &meta_type, &meta_handle))
        {
            let Some(definition) = self
                .metaobject_definition_by_type(&meta_type)
                .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type))
                .or_else(|| metaobject_definition_from_record(&existing))
            else {
                return self.selected_metaobject_payload(
                    &json!({
                        "metaobject": null,
                        "userErrors": [metaobject_user_error(
                            vec!["handle", "type"],
                            &format!("No metaobject definition exists for type \"{meta_type}\""),
                            "UNDEFINED_OBJECT_TYPE",
                            Value::Null,
                            Value::Null
                        )]
                    }),
                    &field.selection,
                );
            };
            let mut update_input = input.clone();
            if let Some(handle) = resolved_string_field(input, "handle") {
                update_input.insert("handle".to_string(), ResolvedValue::String(handle));
            }
            let input_values = metaobject_merged_input_values(&existing, &update_input);
            let mut validation_errors = metaobject_required_field_errors_for_upsert(
                metaobject_create_validation_errors(&update_input, &definition, &input_values),
                &definition,
            );
            validation_errors.extend(
                self.metaobject_display_name_conflict_errors(
                    existing
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    &definition,
                    &update_input,
                    &input_values,
                ),
            );
            let next_handle = if let Some(handle) = resolved_string_field(&update_input, "handle") {
                validation_errors.extend(metaobject_handle_validation_errors(
                    &handle,
                    vec!["handle", "handle"],
                ));
                let normalized = slugify_handle(&handle);
                if self
                    .metaobject_by_type_and_handle(&meta_type, &normalized)
                    .is_none()
                {
                    self.hydrate_metaobject_by_handle(request, &meta_type, &normalized);
                }
                if self
                    .metaobject_by_type_and_handle(&meta_type, &normalized)
                    .as_ref()
                    .and_then(|record| record.get("id").and_then(Value::as_str))
                    .is_some_and(|other_id| {
                        Some(other_id) != existing.get("id").and_then(Value::as_str)
                    })
                {
                    validation_errors.push(metaobject_user_error(
                        vec!["handle", "handle"],
                        "Handle has already been taken",
                        "TAKEN",
                        Value::Null,
                        Value::Null,
                    ));
                }
                normalized
            } else {
                meta_handle.clone()
            };
            if !validation_errors.is_empty() {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": validation_errors}),
                    &field.selection,
                );
            }
            let id = existing
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let display_name = metaobject_display_name(&definition, &input_values);
            let publishable_status =
                metaobject_updated_publishable_status(&update_input, &definition, &existing);
            let updated_at = existing
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or("2026-01-01T00:00:00Z");
            let record = metaobject_record_from_definition_with_options(
                &id,
                &next_handle,
                &definition,
                &input_values,
                &display_name,
                &publishable_status,
                updated_at,
            );
            self.store
                .staged
                .metaobjects
                .insert(id.clone(), record.clone());
            staged_ids.push(id);
            return self.selected_metaobject_payload(
                &json!({"metaobject": record, "userErrors": []}),
                &field.selection,
            );
        }

        let Some(_) = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type))
        else {
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(
                        vec!["handle", "type"],
                        &format!("No metaobject definition exists for type \"{meta_type}\""),
                        "UNDEFINED_OBJECT_TYPE",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        };
        let mut create_input = input.clone();
        create_input.insert("type".to_string(), ResolvedValue::String(meta_type));
        create_input.insert("handle".to_string(), ResolvedValue::String(meta_handle));
        self.stage_metaobject_create_from_input(
            &create_input,
            request,
            staged_ids,
            &field.selection,
            true,
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

    pub(in crate::proxy) fn metaobject_bulk_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let where_input = field.arguments.get("where").and_then(|value| match value {
            ResolvedValue::Object(input) => Some(input),
            _ => None,
        });
        let ids = where_input
            .and_then(|input| input.get("ids"))
            .or_else(|| field.arguments.get("ids"))
            .and_then(|value| match value {
                ResolvedValue::List(values) => Some(
                    values
                        .iter()
                        .filter_map(resolved_value_string)
                        .take(250)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            });
        let meta_type = where_input
            .and_then(|input| resolved_string_field(input, "type"))
            .filter(|value| !value.is_empty());
        if ids.is_some() == meta_type.is_some() {
            return Value::Null;
        }

        let mut user_errors = Vec::new();
        let mut touched_ids = Vec::new();
        if let Some(ids) = ids {
            for (index, id) in ids.into_iter().enumerate() {
                let record = self
                    .metaobject_by_id(&id)
                    .or_else(|| self.hydrate_metaobject_by_id(request, &id));
                if let Some(record) = record {
                    self.store.staged.metaobjects.remove(&id);
                    self.store.staged.deleted_metaobject_ids.insert(id.clone());
                    if let Some(meta_type) = record.get("type").and_then(Value::as_str) {
                        self.increment_metaobject_definition_count(meta_type, -1);
                    }
                    touched_ids.push(id);
                } else {
                    user_errors.push(metaobject_user_error(
                        vec!["where", "ids", &index.to_string()],
                        "Record not found",
                        "RECORD_NOT_FOUND",
                        json!(id),
                        json!(index),
                    ));
                }
            }
        } else if let Some(meta_type) = meta_type {
            if self.metaobject_definition_by_type(&meta_type).is_none() {
                self.hydrate_metaobjects_by_type(request, &meta_type);
            }
            if self.metaobject_definition_by_type(&meta_type).is_none() {
                return selected_json(
                    &json!({
                        "job": null,
                        "userErrors": [metaobject_user_error(
                            vec!["where", "type"],
                            &format!("No metaobject definition exists for type \"{meta_type}\""),
                            "RECORD_NOT_FOUND",
                            Value::Null,
                            Value::Null
                        )]
                    }),
                    &field.selection,
                );
            }
            let ids_to_delete = self
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
                .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
                .collect::<Vec<_>>();
            for id in ids_to_delete {
                self.store.staged.metaobjects.remove(&id);
                self.store.staged.deleted_metaobject_ids.insert(id.clone());
                touched_ids.push(id);
            }
            self.increment_metaobject_definition_count(&meta_type, -(touched_ids.len() as i64));
        }

        staged_ids.extend(touched_ids);
        selected_json(
            &json!({
                "job": {"id": self.next_proxy_synthetic_gid("Job"), "done": false},
                "userErrors": user_errors
            }),
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
        request: &Request,
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
        let meta_type = metaobject_definition_type_from_input(definition_input, request);
        let existing_definitions = self
            .store
            .staged
            .metaobject_definitions
            .iter()
            .filter(|(id, _)| {
                !self
                    .store
                    .staged
                    .deleted_metaobject_definition_ids
                    .contains(*id)
            })
            .count();
        let validation_errors = metaobject_definition_create_validation_errors(
            definition_input,
            &meta_type,
            existing_definitions,
        );
        if !validation_errors.is_empty() {
            return selected_json(
                &json!({"metaobjectDefinition": null, "userErrors": validation_errors}),
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
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(definition) = self.metaobject_definition_by_id(&id) else {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        let definition_input = match field.arguments.get("definition") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobjectDefinition": definition, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let validation_errors =
            metaobject_definition_update_validation_errors(definition_input, meta_type);
        if !validation_errors.is_empty() {
            return selected_json(
                &json!({"metaobjectDefinition": null, "userErrors": validation_errors}),
                &field.selection,
            );
        }
        let updated = update_metaobject_definition_record(definition, definition_input);
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), updated.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"metaobjectDefinition": updated, "userErrors": []}),
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
}

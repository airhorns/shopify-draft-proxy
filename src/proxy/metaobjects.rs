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
            errors.push(metaobject_indexed_user_error(
                vec![
                    "metaobject".to_string(),
                    "fields".to_string(),
                    field_index.clone(),
                ],
                &format!("Field \"{key}\" duplicates other inputs"),
                Some("DUPLICATE_FIELD_INPUT"),
                json!(key.clone()),
                Value::Null,
            ));
            if is_required_title {
                errors.push(metaobject_indexed_user_error(
                    vec!["metaobject".to_string(), "fields".to_string(), field_index],
                    "Title can't be blank",
                    Some("OBJECT_FIELD_REQUIRED"),
                    json!(key),
                    Value::Null,
                ));
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
    // A field defined on the type but never given a value for this entry reads back
    // as `null`/`null` (not empty string) — e.g. a field omitted at create time, or a
    // field the entry predates after a schema change adds it.
    let Some(raw_value) = value.map(String::as_str) else {
        return json!({
            "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
            "type": field_type,
            "value": Value::Null,
            "jsonValue": Value::Null,
            "definition": field_definition
        });
    };
    // Most fields echo their stored string verbatim; measurement and date_time fields
    // are normalized to Shopify's canonical form (uppercased units / decimal values /
    // explicit UTC offset). `json`/`rich_text_field` are free-form and never reshaped.
    let stored_value = if raw_value.is_empty() {
        raw_value.to_string()
    } else if field_type == "date_time" {
        metaobject_normalize_date_time_value(raw_value)
    } else if field_type == "rating" {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(metaobject_rating_value_string)
            .unwrap_or_else(|| raw_value.to_string())
    } else if matches!(field_type, "json" | "rich_text_field") {
        raw_value.to_string()
    } else if field_type.starts_with("list.") {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(|parsed| metaobject_list_value_string(field_type, parsed))
            .unwrap_or_else(|| raw_value.to_string())
    } else {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(metaobject_measurement_value_string)
            .unwrap_or_else(|| raw_value.to_string())
    };
    // jsonValue derives from the raw stored string so list-measurement units stay in
    // their verbatim (lowercase) form; only date_time reflects the normalized offset.
    let json_value_source = if field_type == "date_time" {
        stored_value.as_str()
    } else {
        raw_value
    };
    json!({
        "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
        "type": field_type,
        "value": stored_value,
        "jsonValue": metaobject_field_json_value(field_type, Some(json_value_source)),
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
    let online_store_input = resolved_object_field(&capabilities, "onlineStore");
    let online_store = online_store_input
        .as_ref()
        .and_then(|online_store| resolved_bool_field(online_store, "enabled"))
        .unwrap_or(false);
    let online_store_data = online_store_input
        .as_ref()
        .and_then(|online_store| resolved_object_field(online_store, "data"))
        .map_or(Value::Null, |data| {
            metaobject_online_store_capability_data(&data)
        });
    let renderable = resolved_object_field(&capabilities, "renderable")
        .and_then(|renderable| resolved_bool_field(&renderable, "enabled"))
        .unwrap_or(false);
    let translatable = resolved_object_field(&capabilities, "translatable")
        .and_then(|translatable| resolved_bool_field(&translatable, "enabled"))
        .unwrap_or(false);
    json!({
        "publishable": {"enabled": publishable},
        "onlineStore": {"enabled": online_store, "data": online_store_data},
        "renderable": {"enabled": renderable},
        "translatable": {"enabled": translatable}
    })
}

/// Normalises an onlineStore capability `data` input into its stored shape. The
/// admin API accepts `createRedirects` on input but echoes it back as
/// `canCreateRedirects`, so we translate the field name here.
fn metaobject_online_store_capability_data(data: &BTreeMap<String, ResolvedValue>) -> Value {
    let can_create_redirects = resolved_bool_field(data, "createRedirects")
        .or_else(|| resolved_bool_field(data, "canCreateRedirects"))
        .unwrap_or(false);
    json!({
        "urlHandle": resolved_string_field(data, "urlHandle")
            .map_or(Value::Null, |url_handle| json!(url_handle)),
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

const MIN_FIELD_KEY_LENGTH: usize = 2;
const MAX_FIELD_KEY_LENGTH: usize = 64;
const FIELD_KEY_INVALID_MESSAGE: &str = "Key contains one or more invalid characters.";

fn metaobject_definition_field_key_chars_valid(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn metaobject_definition_is_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("shopify--")
}

fn metaobject_definition_is_app_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("app--")
}

fn metaobject_definition_field_limit(meta_type: &str) -> usize {
    if meta_type.starts_with("shopify--form-") {
        100
    } else {
        40
    }
}

fn metaobject_definition_max_fields_error(max_fields: usize) -> Value {
    metaobject_user_error(
        vec!["definition", "fieldDefinitions"],
        &format!("Maximum {max_fields} fields per metaobject definition"),
        "INVALID",
        Value::Null,
        Value::Null,
    )
}

fn metaobject_definition_field_key_is_reserved(key: &str) -> bool {
    matches!(key, "id" | "handle" | "system" | "metafields")
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
    let max_fields = metaobject_definition_field_limit(meta_type);
    if field_definitions.len() > max_fields {
        errors.push(metaobject_definition_max_fields_error(max_fields));
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
        let field_path = ["definition", "fieldDefinitions", &index_string];
        if push_metaobject_field_key_errors(&mut errors, &field_path, &field_path, &key) {
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

fn push_metaobject_field_key_errors(
    errors: &mut Vec<Value>,
    index_path: &[&str],
    validation_path: &[&str],
    key: &str,
) -> bool {
    if metaobject_definition_field_key_is_reserved(key) {
        errors.push(metaobject_user_error(
            index_path.to_vec(),
            &format!("The name \"{key}\" is reserved for system use"),
            "RESERVED_NAME",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.trim().is_empty() {
        let path = validation_path.to_vec();
        // Blank keys surface Shopify's presence, length, and format errors in order.
        errors.push(metaobject_user_error(
            path.clone(),
            "Key can't be blank",
            "BLANK",
            json!(key),
            Value::Null,
        ));
        errors.push(metaobject_user_error(
            path.clone(),
            &format!("Key is too short (minimum is {MIN_FIELD_KEY_LENGTH} characters)"),
            "TOO_SHORT",
            json!(key),
            Value::Null,
        ));
        errors.push(metaobject_user_error(
            path,
            FIELD_KEY_INVALID_MESSAGE,
            "INVALID",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.chars().count() < MIN_FIELD_KEY_LENGTH {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            &format!("Key is too short (minimum is {MIN_FIELD_KEY_LENGTH} characters)"),
            "TOO_SHORT",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.chars().count() > MAX_FIELD_KEY_LENGTH {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            &format!("Key is too long (maximum is {MAX_FIELD_KEY_LENGTH} characters)"),
            "TOO_LONG",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if !metaobject_definition_field_key_chars_valid(key) {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            FIELD_KEY_INVALID_MESSAGE,
            "INVALID",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    false
}

fn metaobject_renderable_capability_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_definitions: &[Value],
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
    // The renderable capability's meta-title/meta-description keys must reference
    // existing text-typed field definitions. Shopify validates them in a fixed
    // order and reports a single error anchored at ["definition","capabilities","renderable"].
    for (input_key, capability_key) in [
        ("metaTitleKey", "meta_title_key"),
        ("metaDescriptionKey", "meta_description_key"),
    ] {
        let Some(field_key) = resolved_string_field(&data, input_key) else {
            continue;
        };
        match field_definitions
            .iter()
            .find(|definition| definition["key"].as_str() == Some(field_key.as_str()))
        {
            None => errors.push(metaobject_user_error(
                vec!["definition", "capabilities", "renderable"],
                &format!("Field definition \"{field_key}\" does not exist"),
                "INVALID",
                Value::Null,
                Value::Null,
            )),
            Some(field_definition) => {
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or_default();
                if !matches!(
                    field_type,
                    "single_line_text_field" | "multi_line_text_field" | "rich_text_field"
                ) {
                    errors.push(metaobject_user_error(
                        vec!["definition", "capabilities", "renderable"],
                        &format!(
                            "Renderable Capability \"{capability_key}\" cannot reference the field definition \"{field_key}\" of type \"{field_type}\". Only single_line_text_field, multi_line_text_field, rich_text_field definitions are allowed."
                        ),
                        "FIELD_TYPE_INVALID",
                        Value::Null,
                        Value::Null,
                    ));
                }
            }
        }
    }
    errors
}

fn metaobject_definition_update_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    field_definitions: &[Value],
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
    errors.extend(metaobject_renderable_capability_errors(
        input,
        field_definitions,
    ));
    let MetaobjectFieldOperationValidation {
        errors: field_operation_errors,
        resulting_keys,
    } = metaobject_field_operation_validation(input, meta_type, field_definitions);
    errors.extend(field_operation_errors);
    if let Some(display_name_key) = resolved_string_field(input, "displayNameKey") {
        if !resulting_keys.contains(&display_name_key) {
            errors.push(metaobject_user_error(
                vec!["definition", "displayNameKey"],
                &format!("Field definition \"{display_name_key}\" does not exist"),
                "UNDEFINED_OBJECT_FIELD",
                Value::Null,
                Value::Null,
            ));
        }
    }
    errors
}

struct MetaobjectFieldOperationValidation {
    errors: Vec<Value>,
    resulting_keys: BTreeSet<String>,
}

/// Validates the `fieldDefinitions` operation list on a definition update. Each
/// entry is a one-of `{create|update|delete}` operation; Shopify reports errors
/// per operation index. Most operation-specific errors are nested under
/// `{create|update|delete}`, while reserved/duplicate create keys are anchored at
/// the operation index itself.
fn metaobject_field_operation_validation(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    field_definitions: &[Value],
) -> MetaobjectFieldOperationValidation {
    let mut errors = Vec::new();
    let operations = resolved_object_list_field(input, "fieldDefinitions");
    let mut known_keys: BTreeSet<String> = field_definitions
        .iter()
        .filter_map(|definition| definition["key"].as_str().map(str::to_string))
        .collect();
    let mut seen_create_keys = BTreeSet::new();
    for (index, operation) in operations.iter().enumerate() {
        let index_string = index.to_string();
        if let Some(create) = resolved_object_field(operation, "create") {
            let key = resolved_string_field(&create, "key").unwrap_or_default();
            // Presence, length, and format validators anchor at the `create` object;
            // the already-taken validator anchors one level deeper at `create.key`.
            let index_path = ["definition", "fieldDefinitions", &index_string];
            let create_path = ["definition", "fieldDefinitions", &index_string, "create"];
            if push_metaobject_field_key_errors(&mut errors, &index_path, &create_path, &key) {
                continue;
            }
            if !seen_create_keys.insert(key.clone()) {
                errors.push(metaobject_user_error(
                    vec!["definition", "fieldDefinitions", &index_string],
                    &format!("Field \"{key}\" duplicates other inputs"),
                    "DUPLICATE_FIELD_INPUT",
                    json!(key),
                    Value::Null,
                ));
                continue;
            }
            if known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "create",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" is already taken"),
                    "OBJECT_FIELD_TAKEN",
                    json!(key),
                    Value::Null,
                ));
                continue;
            }
            known_keys.insert(key);
        } else if let Some(update) = resolved_object_field(operation, "update") {
            let key = resolved_string_field(&update, "key").unwrap_or_default();
            if !known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "update",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" does not exist"),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    Value::Null,
                ));
            }
        } else if let Some(delete) = resolved_object_field(operation, "delete") {
            let key = resolved_string_field(&delete, "key").unwrap_or_default();
            if !known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "delete",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" does not exist"),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    Value::Null,
                ));
            } else {
                known_keys.remove(&key);
            }
        }
    }
    let max_fields = metaobject_definition_field_limit(meta_type);
    if known_keys.len() > max_fields {
        errors.push(metaobject_definition_max_fields_error(max_fields));
    }
    MetaobjectFieldOperationValidation {
        errors,
        resulting_keys: known_keys,
    }
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
    apply_metaobject_definition_capability_updates(&mut definition, input);
    apply_metaobject_definition_field_operations(&mut definition, input);
    definition
}

/// Merges the capability changes from a definition-update input into the stored
/// capabilities, preserving capabilities the caller did not mention.
fn apply_metaobject_definition_capability_updates(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    let Some(input_capabilities) = resolved_object_field(input, "capabilities") else {
        return;
    };
    for key in ["publishable", "onlineStore", "renderable", "translatable"] {
        let Some(capability) = resolved_object_field(&input_capabilities, key) else {
            continue;
        };
        if let Some(enabled) = resolved_bool_field(&capability, "enabled") {
            definition["capabilities"][key]["enabled"] = json!(enabled);
        }
        if key == "onlineStore" {
            if let Some(data) = resolved_object_field(&capability, "data") {
                definition["capabilities"]["onlineStore"]["data"] =
                    metaobject_online_store_capability_data(&data);
            }
        }
    }
}

/// Applies the `fieldDefinitions` create/update/delete operations from a
/// definition-update input to the stored field-definition list. Validation has
/// already run, so every operation here is known to be applicable.
fn apply_metaobject_definition_field_operations(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    let operations = resolved_object_list_field(input, "fieldDefinitions");
    let reset_field_order = resolved_bool_field(input, "resetFieldOrder").unwrap_or(false);
    if operations.is_empty() {
        return;
    }
    let mut fields = definition["fieldDefinitions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // When `resetFieldOrder` is set, Shopify reorders the surviving field definitions
    // to follow the order the caller listed their create/update operations (deletes
    // drop out). We record that intended order as we apply each operation.
    let mut intended_order: Vec<String> = Vec::new();
    for operation in &operations {
        if let Some(create) = resolved_object_field(operation, "create") {
            if let Some(key) = resolved_string_field(&create, "key") {
                intended_order.push(key);
            }
            fields.push(metaobject_field_definition_record(create));
        } else if let Some(update) = resolved_object_field(operation, "update") {
            let key = resolved_string_field(&update, "key").unwrap_or_default();
            if !key.is_empty() {
                intended_order.push(key.clone());
            }
            if let Some(field) = fields
                .iter_mut()
                .find(|field| field["key"].as_str() == Some(key.as_str()))
            {
                apply_metaobject_field_definition_update(field, &update);
            }
        } else if let Some(delete) = resolved_object_field(operation, "delete") {
            let key = resolved_string_field(&delete, "key").unwrap_or_default();
            fields.retain(|field| field["key"].as_str() != Some(key.as_str()));
        }
    }
    if reset_field_order {
        // Stable sort by position in the intended order; any field the caller did not
        // mention keeps its relative position after the reordered ones.
        fields.sort_by_key(|field| {
            field["key"]
                .as_str()
                .and_then(|key| intended_order.iter().position(|ordered| ordered == key))
                .unwrap_or(usize::MAX)
        });
    }
    definition["fieldDefinitions"] = json!(fields);
}

fn apply_metaobject_field_definition_update(
    field: &mut Value,
    update: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(name) = resolved_string_field(update, "name") {
        field["name"] = json!(name);
    }
    if update.contains_key("description") {
        field["description"] = update
            .get("description")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |description| json!(description));
    }
    if let Some(required) = resolved_bool_field(update, "required") {
        field["required"] = json!(required);
    }
    if update.contains_key("validations") {
        field["validations"] = json!(resolved_object_list_field(update, "validations")
            .into_iter()
            .map(|validation| json!({
                "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                "value": resolved_string_field(&validation, "value").unwrap_or_default()
            }))
            .collect::<Vec<_>>());
    }
    if update.contains_key("type") {
        let field_type = metaobject_field_definition_type(update);
        field["type"] = json!({
            "name": field_type,
            "category": metaobject_field_type_category(&field_type)
        });
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
        // Shopify returns number_decimal jsonValue as the verbatim decimal string,
        // not a JSON number.
        "number_decimal" => json!(value),
        "boolean" => match value {
            "true" => json!(true),
            "false" => json!(false),
            _ => Value::Null,
        },
        "json" | "rich_text_field" => serde_json::from_str(value).unwrap_or_else(|_| json!(value)),
        value_type if value_type.starts_with("list.") => {
            let parsed = serde_json::from_str(value).unwrap_or_else(|_| json!([value]));
            metaobject_normalize_list_json_value(value_type, parsed)
        }
        // Structured scalar types (money, link, rating, and the measurement family)
        // serialize jsonValue as the parsed JSON object. Measurement objects also
        // uppercase their unit. Plain scalar strings (color, date, references, etc.)
        // fall through to the verbatim string.
        _ => match serde_json::from_str::<Value>(value) {
            Ok(parsed) if parsed.is_object() || parsed.is_array() => {
                metaobject_measurement_unit_uppercased(parsed)
            }
            _ => json!(value),
        },
    }
}

/// If `value` is a measurement-shaped object (`{"value": <number>, "unit": "<string>"}`)
/// returns the value and unit. Distinguishes measurements from money/link/rating which
/// have different key sets.
fn metaobject_measurement_parts(value: &Value) -> Option<(&Value, &str)> {
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let number = object.get("value")?;
    if !number.is_number() {
        return None;
    }
    let unit = object.get("unit")?.as_str()?;
    Some((number, unit))
}

/// Uppercases the `unit` of a scalar measurement object, leaving every other shape
/// (money, link, rating, json, arrays) untouched. Scalar measurements normalize their
/// unit in jsonValue; list measurements echo the parsed input verbatim.
fn metaobject_measurement_unit_uppercased(value: Value) -> Value {
    if let Some((number, unit)) = metaobject_measurement_parts(&value) {
        return json!({"value": number, "unit": unit.to_uppercase()});
    }
    value
}

/// Shopify's classic `dimension`/`weight`/`volume` measurement families store their
/// unit as an abbreviation in jsonValue (e.g. `centimeters` -> `cm`). All newer
/// measurement families echo the verbatim lowercase unit. Returns the abbreviation when
/// one exists, otherwise the lowercase unit unchanged.
fn metaobject_measurement_storage_unit(field_type: &str, unit: &str) -> String {
    let lower = unit.to_lowercase();
    let abbreviation = match field_type {
        "dimension" | "list.dimension" => match lower.as_str() {
            "millimeters" => Some("mm"),
            "centimeters" => Some("cm"),
            "meters" => Some("m"),
            "inches" => Some("in"),
            "feet" => Some("ft"),
            "yards" => Some("yd"),
            _ => None,
        },
        "weight" | "list.weight" => match lower.as_str() {
            "grams" => Some("g"),
            "kilograms" => Some("kg"),
            "ounces" => Some("oz"),
            "pounds" => Some("lb"),
            _ => None,
        },
        "volume" | "list.volume" => match lower.as_str() {
            "milliliters" => Some("ml"),
            "centiliters" => Some("cl"),
            "liters" => Some("l"),
            "cubic_meters" => Some("m3"),
            "fluid_ounces" => Some("fl oz"),
            "imperial_fluid_ounces" => Some("imp fl oz"),
            "pints" => Some("pt"),
            "imperial_pints" => Some("imp pt"),
            "quarts" => Some("qt"),
            "imperial_quarts" => Some("imp qt"),
            "gallons" => Some("gal"),
            "imperial_gallons" => Some("imp gal"),
            _ => None,
        },
        _ => None,
    };
    abbreviation.map(str::to_string).unwrap_or(lower)
}

/// Normalizes a list field's jsonValue array per element: date_time strings gain an
/// explicit UTC offset, and dimension/weight/volume measurements use abbreviated units.
/// Every other list element is echoed verbatim from the parsed input.
fn metaobject_normalize_list_json_value(field_type: &str, parsed: Value) -> Value {
    let Some(items) = parsed.as_array() else {
        return parsed;
    };
    let normalized = items
        .iter()
        .map(|item| {
            if field_type == "list.date_time" {
                if let Some(text) = item.as_str() {
                    return json!(metaobject_normalize_date_time_value(text));
                }
            }
            if matches!(field_type, "list.dimension" | "list.weight" | "list.volume") {
                if let Some((number, unit)) = metaobject_measurement_parts(item) {
                    return json!({
                        "value": number,
                        "unit": metaobject_measurement_storage_unit(field_type, unit),
                    });
                }
            }
            // number_decimal elements echo as decimal strings, not JSON numbers.
            if field_type == "list.number_decimal" && item.is_number() {
                return json!(item.to_string());
            }
            item.clone()
        })
        .collect::<Vec<_>>();
    Value::Array(normalized)
}

/// Renders one element of a list field's `value` string with Shopify's canonical
/// formatting (measurement floats + uppercased units, date_time offsets, decimal
/// stringification, rating key order). Other elements serialize verbatim.
fn metaobject_list_value_token(field_type: &str, item: &Value) -> String {
    if let Some((number, unit)) = metaobject_measurement_parts(item) {
        return format!(
            "{{\"value\":{},\"unit\":\"{}\"}}",
            metaobject_format_measurement_number(number),
            unit.to_uppercase()
        );
    }
    match field_type {
        "list.date_time" => {
            if let Some(text) = item.as_str() {
                return Value::String(metaobject_normalize_date_time_value(text)).to_string();
            }
        }
        // number_decimal elements are stored as decimal strings ([10.4] -> ["10.4"]).
        "list.number_decimal" => {
            if item.is_number() {
                return Value::String(item.to_string()).to_string();
            }
        }
        "list.rating" => {
            if let Some(rendered) = metaobject_rating_value_string(item) {
                return rendered;
            }
        }
        _ => {}
    }
    serde_json::to_string(item).unwrap_or_else(|_| item.to_string())
}

/// Renders the full `value` string of any `list.*` field. Returns `None` when the parsed
/// JSON is not an array.
fn metaobject_list_value_string(field_type: &str, parsed: &Value) -> Option<String> {
    let items = parsed.as_array()?;
    let rendered = items
        .iter()
        .map(|item| metaobject_list_value_token(field_type, item))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{rendered}]"))
}

fn metaobject_format_measurement_number(number: &Value) -> String {
    match number.as_f64() {
        Some(number) if number.fract() == 0.0 => format!("{number:.1}"),
        Some(number) => format!("{number}"),
        None => number.to_string(),
    }
}

/// Renders a measurement object's `value` field with Shopify's canonical formatting:
/// the numeric value is always emitted as a decimal (`5` -> `5.0`) and the unit is
/// uppercased. Returns `None` when the parsed JSON is not measurement-shaped.
fn metaobject_measurement_value_string(parsed: &Value) -> Option<String> {
    if let Some((number, unit)) = metaobject_measurement_parts(parsed) {
        return Some(format!(
            "{{\"value\":{},\"unit\":\"{}\"}}",
            metaobject_format_measurement_number(number),
            unit.to_uppercase()
        ));
    }
    let items = parsed.as_array()?;
    if items.is_empty()
        || !items
            .iter()
            .all(|item| metaobject_measurement_parts(item).is_some())
    {
        return None;
    }
    let rendered = items
        .iter()
        .map(|item| {
            let (number, unit) = metaobject_measurement_parts(item).expect("checked above");
            format!(
                "{{\"value\":{},\"unit\":\"{}\"}}",
                metaobject_format_measurement_number(number),
                unit.to_uppercase()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{rendered}]"))
}

/// Shopify re-emits a `rating` field's value with its keys in canonical order
/// (`scale_min`, `scale_max`, `value`) regardless of the order they were submitted in.
fn metaobject_rating_value_string(parsed: &Value) -> Option<String> {
    let object = parsed.as_object()?;
    if object.len() != 3 {
        return None;
    }
    let scale_min = object.get("scale_min")?;
    let scale_max = object.get("scale_max")?;
    let value = object.get("value")?;
    Some(format!(
        "{{\"scale_min\":{scale_min},\"scale_max\":{scale_max},\"value\":{value}}}"
    ))
}

/// Shopify normalizes a date_time without an explicit offset to UTC (`+00:00`).
fn metaobject_normalize_date_time_value(value: &str) -> String {
    let Some((_, time)) = value.split_once('T') else {
        return value.to_string();
    };
    let has_offset =
        time.contains('+') || time.contains('-') || time.ends_with('Z') || time.ends_with('z');
    if has_offset {
        value.to_string()
    } else {
        format!("{value}+00:00")
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

fn metaobject_field_validation_value(validations: &[Value], name: &str) -> Option<String> {
    validations
        .iter()
        .find(|validation| validation["name"].as_str() == Some(name))
        .and_then(|validation| validation["value"].as_str())
        .map(str::to_string)
}

fn metaobject_value_is_valid_date(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    parts[0].len() == 4
        && (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
}

fn metaobject_value_is_valid_date_time(value: &str) -> bool {
    let Some((date_part, time_part)) = value.split_once(['T', ' ']) else {
        return false;
    };
    if !metaobject_value_is_valid_date(date_part) {
        return false;
    }
    let time_core = time_part.split(['+', 'Z', '.']).next().unwrap_or(time_part);
    let segments: Vec<&str> = time_core.split(':').collect();
    if !(2..=3).contains(&segments.len()) {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty() && segment.chars().all(|character| character.is_ascii_digit())
    })
}

const METAOBJECT_MONEY_INVALID_MESSAGE: &str = "Value must be a stringified JSON object with amount (numeric) and currency_code (string matching the shop's currency) fields.";
const METAOBJECT_LINK_SCHEME_INVALID_MESSAGE: &str =
    "Value must be one of the following URL schemes: http, https, mailto, sms, tel.";
const METAOBJECT_LINK_DOMAIN_INVALID_MESSAGE: &str =
    "Value must conform to the domain restriction you set.";
const METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE: &str =
    "Value must be a stringified JSON object with a value (numeric) and unit (string from one the supported measurement units) fields.";

struct MetaobjectFieldValueValidationContext<'a> {
    proxy: &'a DraftProxy,
    existing_id: Option<&'a str>,
    validate_existing_values: bool,
}

struct MetaobjectFieldValueError {
    message: String,
    code: &'static str,
    element_index: Value,
}

impl MetaobjectFieldValueError {
    fn invalid_value(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "INVALID_VALUE",
            element_index: Value::Null,
        }
    }

    fn with_element_index(mut self, element_index: Value) -> Self {
        self.element_index = element_index;
        self
    }

    fn taken(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "TAKEN",
            element_index: Value::Null,
        }
    }
}

fn metaobject_classic_measurement_value_error(field_type: &str, value: &str) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some("Value must contain unit and value.".to_string());
    };
    let Some(object) = parsed.as_object() else {
        return Some("Value must contain unit and value.".to_string());
    };
    let has_numeric_value = object.get("value").is_some_and(|value| {
        value.is_number()
            || value
                .as_str()
                .is_some_and(|value| value.parse::<f64>().is_ok())
    });
    if !has_numeric_value {
        return Some("Value must contain unit and value.".to_string());
    }
    let Some(unit) = object.get("unit").and_then(Value::as_str) else {
        return Some("Value must contain unit and value.".to_string());
    };
    if unit.is_empty() {
        return Some("Value must contain unit and value.".to_string());
    }
    (!measurement_unit_is_supported(field_type, unit))
        .then(|| "Value must be a supported unit.".to_string())
}

fn metaobject_value_is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|character| character.is_ascii_hexdigit())
}

fn metaobject_value_is_valid_url(value: &str) -> bool {
    let lowercased = value.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "sms:", "tel:"]
        .iter()
        .any(|scheme| lowercased.starts_with(scheme))
}

fn metaobject_reference_value_error(
    value: &str,
    gid_types: &[&str],
    message: &str,
) -> Option<String> {
    if shopify_gid_resource_type(value).is_some_and(|resource_type| {
        gid_types
            .iter()
            .any(|gid_type| resource_type.eq_ignore_ascii_case(gid_type))
    }) {
        None
    } else {
        Some(message.to_string())
    }
}

fn metaobject_validation_string_list(validations: &[Value], name: &str) -> Vec<String> {
    let Some(value) = metaobject_field_validation_value(validations, name) else {
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

fn metaobject_link_url(value: &Value) -> Option<&str> {
    value.as_object()?.get("url")?.as_str()
}

fn metaobject_link_allowed_domains_match(url: &str, validations: &[Value]) -> bool {
    let allowed_domains = metaobject_validation_string_list(validations, "allowed_domains")
        .into_iter()
        .filter_map(|domain| {
            let trimmed = domain.trim().trim_start_matches("*.").to_ascii_lowercase();
            if trimmed.is_empty() {
                None
            } else if let Ok(parsed) = url::Url::parse(&trimmed) {
                parsed.host_str().map(|host| host.to_ascii_lowercase())
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>();
    if allowed_domains.is_empty() {
        return true;
    }
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
    else {
        return false;
    };
    allowed_domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

fn metaobject_link_value_error(value: &str, validations: &[Value]) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some("Value must be a valid link.".to_string());
    };
    if !is_shopify_link_value(&parsed) {
        if let Some(url) = metaobject_link_url(&parsed) {
            if !is_shopify_metafield_url(url) {
                return Some(METAOBJECT_LINK_SCHEME_INVALID_MESSAGE.to_string());
            }
        }
        return Some("Value must be a valid link.".to_string());
    }
    let Some(url) = metaobject_link_url(&parsed) else {
        return Some("Value must be a valid link.".to_string());
    };
    if metaobject_link_allowed_domains_match(url, validations) {
        None
    } else {
        Some(METAOBJECT_LINK_DOMAIN_INVALID_MESSAGE.to_string())
    }
}

fn metaobject_structured_measurement_value_error(field_type: &str, value: &str) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some(METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string());
    };
    if !parsed.is_object() {
        return Some(METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string());
    }
    shopify_measurement_value_error(field_type, &parsed).map(|message| {
        if matches!(
            message.as_str(),
            "Value must contain unit and value." | "Value must be a non-negative number."
        ) {
            METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string()
        } else {
            message
        }
    })
}

fn metaobject_language_value_error(value: &str) -> Option<String> {
    if default_available_locales().contains_key(value) {
        None
    } else {
        Some("Value must be in ISO 639-1 format.".to_string())
    }
}

fn metaobject_id_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    value: &str,
) -> Option<MetaobjectFieldValueError> {
    if value.is_empty() {
        return None;
    }
    let context = context?;
    let taken = context
        .proxy
        .store
        .staged
        .metaobjects
        .values()
        .any(|record| {
            record.get("id").and_then(Value::as_str) != context.existing_id
                && record["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|field| {
                        field.get("key").and_then(Value::as_str) == Some(field_key)
                            && field.get("type").and_then(Value::as_str) == Some("id")
                            && field.get("value").and_then(Value::as_str) == Some(value)
                    })
        });
    taken.then(|| {
        MetaobjectFieldValueError::taken(
            "Value is already assigned to another metafield. Choose a different value to ensure it remains unique.",
        )
    })
}

fn metaobject_mixed_reference_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    value: &str,
    validations: &[Value],
) -> Option<String> {
    if shopify_gid_resource_type(value) != Some("Metaobject") {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    }
    let allowed_definition_ids = {
        let mut values =
            metaobject_validation_string_list(validations, "metaobject_definition_ids");
        values.extend(metaobject_validation_string_list(
            validations,
            "metaobject_definition_id",
        ));
        values
    };
    if allowed_definition_ids.is_empty() {
        return None;
    }
    let context = context?;
    let Some(record) = context.proxy.metaobject_by_id(value) else {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    };
    let Some(meta_type) = record.get("type").and_then(Value::as_str) else {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    };
    let Some(definition) = context
        .proxy
        .metaobject_definition_by_type(meta_type)
        .or_else(|| metaobject_definition_from_record(&record))
    else {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    };
    let target_definition_id = definition.get("id").and_then(Value::as_str);
    if target_definition_id
        .is_some_and(|id| allowed_definition_ids.iter().any(|allowed| allowed == id))
    {
        None
    } else {
        Some("Value must belong to one of the specified metaobject definitions.".to_string())
    }
}

/// Validates a single (non-list) metaobject field value against its type and
/// declared validations, returning Shopify's specific error message when the
/// value is unacceptable. `is_update` captures create/update asymmetry (a
/// malformed boolean is tolerated on create but rejected on update).
fn metaobject_scalar_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    field_type: &str,
    value: &str,
    validations: &[Value],
    is_update: bool,
) -> Option<MetaobjectFieldValueError> {
    if field_type == "id" {
        return metaobject_id_value_error(context, field_key, value);
    }

    let message = match field_type {
        "number_integer" => {
            let Ok(parsed) = value.parse::<i64>() else {
                return Some(MetaobjectFieldValueError::invalid_value(
                    "Value must be an integer.",
                ));
            };
            if let Some(max) = metaobject_field_validation_value(validations, "max")
                .and_then(|max| max.parse::<i64>().ok())
            {
                if parsed > max {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum of {max}."
                    )));
                }
            }
            if let Some(min) = metaobject_field_validation_value(validations, "min")
                .and_then(|min| min.parse::<i64>().ok())
            {
                if parsed < min {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a minimum of {min}."
                    )));
                }
            }
            None
        }
        "number_decimal" => {
            if value.parse::<f64>().is_err() {
                Some("Value must be a decimal.".to_string())
            } else {
                None
            }
        }
        "boolean" => {
            if matches!(value, "true" | "false") || !is_update {
                None
            } else {
                Some("Value must be true or false.".to_string())
            }
        }
        "date" => {
            if metaobject_value_is_valid_date(value) {
                None
            } else {
                Some("Value must be in YYYY-MM-DD format.".to_string())
            }
        }
        "date_time" => {
            if metaobject_value_is_valid_date_time(value) {
                None
            } else {
                Some("Value must be in “YYYY-MM-DDTHH:MM:SS” format. For example: 2022-06-01T15:30:00".to_string())
            }
        }
        "money" => serde_json::from_str::<Value>(value)
            .ok()
            .as_ref()
            .filter(|parsed| is_shopify_money_value(parsed))
            .map(|_| ())
            .is_none()
            .then(|| METAOBJECT_MONEY_INVALID_MESSAGE.to_string()),
        "link" => metaobject_link_value_error(value, validations),
        "language" => metaobject_language_value_error(value),
        "dimension" | "volume" | "weight" => {
            metaobject_classic_measurement_value_error(field_type, value)
        }
        "rating" => {
            let parsed = serde_json::from_str::<Value>(value).ok()?;
            let rating = parsed.get("value").and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str()?.parse::<f64>().ok())
            })?;
            if let Some(scale_max) = metaobject_field_validation_value(validations, "scale_max") {
                if scale_max
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|max| rating > max)
                {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum of {scale_max}."
                    )));
                }
            }
            if let Some(scale_min) = metaobject_field_validation_value(validations, "scale_min") {
                if scale_min
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|min| rating < min)
                {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a minimum of {scale_min}."
                    )));
                }
            }
            None
        }
        "color" => {
            if metaobject_value_is_hex_color(value) {
                None
            } else {
                Some("Value must be a hex color code.".to_string())
            }
        }
        "url" => {
            if metaobject_value_is_valid_url(value) {
                None
            } else {
                Some("Value cannot have an empty scheme (protocol), must include one of the following URL schemes: [\"http\", \"https\", \"mailto\", \"sms\", \"tel\"].'".to_string())
            }
        }
        "product_reference" => metaobject_reference_value_error(
            value,
            &["Product"],
            "Value must be a valid product reference.",
        ),
        "variant_reference" => metaobject_reference_value_error(
            value,
            &["ProductVariant"],
            "Value must be a valid product variant reference.",
        ),
        "collection_reference" => metaobject_reference_value_error(
            value,
            &["Collection"],
            "Value must be a valid collection reference.",
        ),
        "customer_reference" => metaobject_reference_value_error(
            value,
            &["Customer"],
            "Value must be a valid customer reference.",
        ),
        "company_reference" => metaobject_reference_value_error(
            value,
            &["Company"],
            "Value must be a valid company reference.",
        ),
        "metaobject_reference" => metaobject_reference_value_error(
            value,
            &["Metaobject"],
            "Value require that you select a metaobject.",
        ),
        "file_reference" => metaobject_reference_value_error(
            value,
            &[
                "MediaImage",
                "GenericFile",
                "Video",
                "ExternalVideo",
                "Model3d",
                "File",
            ],
            "Value must be a file reference string.",
        ),
        "page_reference" => metaobject_reference_value_error(
            value,
            &["Page"],
            "Value must be a valid page reference.",
        ),
        "order_reference" => metaobject_reference_value_error(
            value,
            &["Order"],
            "Value must be a valid order reference.",
        ),
        "article_reference" => metaobject_reference_value_error(
            value,
            &["Article"],
            "Value must be a valid article reference.",
        ),
        "product_taxonomy_value_reference" => metaobject_reference_value_error(
            value,
            &["ProductTaxonomyValue", "TaxonomyValue"],
            "Value require that you select a product taxonomy value.",
        ),
        "mixed_reference" => metaobject_mixed_reference_value_error(context, value, validations),
        "single_line_text_field" | "multi_line_text_field" => {
            if let Some(max) = metaobject_field_validation_value(validations, "max")
                .and_then(|max| max.parse::<usize>().ok())
            {
                if value.chars().count() > max {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum length of {max}."
                    )));
                }
            }
            None
        }
        _ if is_measurement_metafield_type_name(field_type) => {
            metaobject_structured_measurement_value_error(field_type, value)
        }
        _ => None,
    };
    message.map(MetaobjectFieldValueError::invalid_value)
}

/// Validates a metaobject field value (scalar or `list.<type>`), returning the
/// error message and the `elementIndex` Shopify reports (null for scalars, the
/// offending element's index for list values).
/// Types whose structural validity is checked by `metaobject_value_matches_type`
/// rather than the typed value validator (which returns no opinion for them).
fn metaobject_value_uses_generic_fallback(field_type: &str) -> bool {
    field_type.starts_with("list.") || matches!(field_type, "json" | "rich_text_field")
}

fn metaobject_field_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    field_type: &str,
    value: &str,
    validations: &[Value],
    is_update: bool,
) -> Option<MetaobjectFieldValueError> {
    if let Some(base_type) = field_type.strip_prefix("list.") {
        let parsed = serde_json::from_str::<Value>(value).ok()?;
        let elements = parsed.as_array()?;
        for (index, element) in elements.iter().enumerate() {
            let element_value = match element {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            if let Some(error) = metaobject_scalar_value_error(
                context,
                field_key,
                base_type,
                &element_value,
                validations,
                is_update,
            ) {
                return Some(error.with_element_index(json!(index)));
            }
        }
        None
    } else {
        metaobject_scalar_value_error(
            context,
            field_key,
            field_type,
            value,
            validations,
            is_update,
        )
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
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    is_update: bool,
) -> Vec<Value> {
    let mut errors = metaobject_create_duplicate_field_errors(input);
    let definition_keys = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field.get("key").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut provided_keys = BTreeSet::new();

    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for (index, field) in fields.iter().enumerate() {
            let ResolvedValue::Object(field) = field else {
                continue;
            };
            let key = resolved_string_field(field, "key").unwrap_or_default();
            provided_keys.insert(key.clone());
            if !definition_keys.contains(key.as_str()) {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "fields", &index.to_string()],
                    &format!("Field definition \"{key}\" does not exist"),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    Value::Null,
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
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or_default();
                let validations = field_definition["validations"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if let Some(error) = metaobject_field_value_error(
                    context,
                    &key,
                    field_type,
                    &value,
                    validations,
                    is_update,
                ) {
                    errors.push(metaobject_user_error(
                        vec!["metaobject", "fields", &index.to_string()],
                        &error.message,
                        error.code,
                        json!(key),
                        error.element_index,
                    ));
                } else if metaobject_value_uses_generic_fallback(field_type)
                    && !metaobject_value_matches_type(field_type, &value)
                {
                    // json/rich-text/list-shape validation that the typed
                    // validator intentionally defers to the structural check.
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

    // Undefined keys are flagged only for fields the caller explicitly supplied in
    // this request (handled in the `fields` loop above). Stale values merged from a
    // pre-existing entry whose definition later dropped the field are NOT re-flagged
    // here — Shopify only errors on keys present in the current input.

    for field_definition in definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = field_definition
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if context.is_some_and(|context| context.validate_existing_values)
            && !provided_keys.contains(key)
        {
            let Some(value) = input_values.get(key).filter(|value| !value.is_empty()) else {
                continue;
            };
            let field_type = field_definition["type"]["name"]
                .as_str()
                .unwrap_or_default();
            let validations = field_definition["validations"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Some(error) = metaobject_field_value_error(
                context,
                key,
                field_type,
                value,
                validations,
                is_update,
            ) {
                errors.push(metaobject_user_error(
                    vec!["metaobject"],
                    &error.message,
                    error.code,
                    json!(key),
                    error.element_index,
                ));
            } else if metaobject_value_uses_generic_fallback(field_type)
                && !metaobject_value_matches_type(field_type, value)
            {
                errors.push(metaobject_user_error(
                    vec!["metaobject"],
                    &format!("Value is invalid for field \"{key}\"."),
                    "INVALID_VALUE",
                    json!(key),
                    Value::Null,
                ));
            }
        }
        if field_definition
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && input_values
                .get(key)
                .is_none_or(|value| value.trim().is_empty())
        {
            // A required field reads as blank either because the caller supplied it
            // empty (anchor at that input field's index) or omitted it entirely
            // (anchor at the metaobject root). The message uses the field's display
            // name: "Summary can't be blank".
            let provided_index = input
                .get("fields")
                .and_then(|fields| match fields {
                    ResolvedValue::List(fields) => Some(fields),
                    _ => None,
                })
                .and_then(|fields| {
                    fields.iter().rposition(|field| match field {
                        ResolvedValue::Object(field) => {
                            resolved_string_field(field, "key").as_deref() == Some(key)
                        }
                        _ => false,
                    })
                });
            let field_path = match provided_index {
                Some(index) => json!(["metaobject", "fields", index.to_string()]),
                None => json!(["metaobject"]),
            };
            let field_name = field_definition
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| metaobject_field_name(key));
            errors.push(metaobject_indexed_user_error(
                field_path,
                &format!("{field_name} can't be blank"),
                Some("OBJECT_FIELD_REQUIRED"),
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(capabilities) = resolved_object_field(input, "capabilities") {
        // Shopify reports capability guard errors in a fixed capability order
        // (publishable, onlineStore, renderable, translatable) regardless of the
        // order the caller supplied them, anchored at ["capabilities", <name>]
        // with a null elementKey.
        for key in ["publishable", "onlineStore", "renderable", "translatable"] {
            if !capabilities.contains_key(key) {
                continue;
            }
            let enabled = definition["capabilities"][key]["enabled"]
                .as_bool()
                .unwrap_or(false);
            if !enabled {
                errors.push(metaobject_user_error(
                    vec!["capabilities", key],
                    "Capability is not enabled on this definition",
                    "CAPABILITY_NOT_ENABLED",
                    Value::Null,
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
    metaobject_indexed_user_error(field, message, Some(code), element_key, element_index)
}

fn metaobject_no_definition_error(path_root: &str, meta_type: &str, code: &str) -> Value {
    metaobject_user_error(
        vec![path_root, "type"],
        &format!("No metaobject definition exists for type \"{meta_type}\""),
        code,
        Value::Null,
        Value::Null,
    )
}

fn metaobject_keyed_display_name(
    definition: &Value,
    input_values: &BTreeMap<String, String>,
) -> Option<String> {
    definition
        .get("displayNameKey")
        .and_then(Value::as_str)
        .and_then(|key| input_values.get(key))
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn metaobject_display_name(
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    handle_display_source: &str,
) -> String {
    metaobject_keyed_display_name(definition, input_values)
        .unwrap_or_else(|| metaobject_display_name_from_handle(handle_display_source))
}

fn metaobject_display_name_from_handle(handle: &str) -> String {
    let handle = handle.trim();
    if let Some((base, code)) = metaobject_random_handle_parts(handle) {
        return format!(
            "{} #{}",
            titleize_metaobject_handle(base),
            code.to_ascii_uppercase()
        );
    }
    titleize_metaobject_handle(handle)
}

fn metaobject_random_handle_parts(handle: &str) -> Option<(&str, &str)> {
    let (base, code) = handle.rsplit_once('-')?;
    if base.is_empty()
        || code.len() != 8
        || !code
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((base, code))
}

fn titleize_metaobject_handle(handle: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_was_lower_or_digit = false;
    for character in handle.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                words.push(current);
                current = String::new();
            }
            previous_was_lower_or_digit = false;
            continue;
        }
        if character.is_ascii_uppercase() && previous_was_lower_or_digit && !current.is_empty() {
            words.push(current);
            current = String::new();
        }
        previous_was_lower_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
        current.push(character);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .into_iter()
        .map(|word| {
            let mut lowercase = word.to_ascii_lowercase();
            if let Some(first) = lowercase.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            lowercase
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct MetaobjectHandleChoice {
    handle: String,
    display_source: String,
}

fn metaobject_random_handle_suffix(meta_type: &str, id: &str, attempt: u64) -> String {
    let seed = format!("{meta_type}:{id}:{attempt}");
    let digest = md5::compute(seed.as_bytes());
    format!("{digest:x}").chars().take(8).collect()
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

fn metaobject_record_from_definition_with_options(
    id: &str,
    handle: &str,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    display_name: &str,
    publishable_status: &str,
    updated_at: &str,
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
        "updatedAt": updated_at,
        "capabilities": {
            "publishable": if definition["capabilities"]["publishable"]["enabled"].as_bool().unwrap_or(false) {
                json!({"status": publishable_status})
            } else {
                Value::Null
            },
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
            || !self.store.staged.metaobjects.tombstones.is_empty()
    }

    /// Decides whether a metaobject mutation request should be staged locally or
    /// forwarded upstream. Create/Delete and definition Create/Delete are always
    /// emulated locally. Update/Upsert/DefinitionUpdate are emulated locally only
    /// when their target already exists in local staged state (i.e. it was created
    /// in this scenario): a backend that staged the resource locally also expects
    /// the proxy to mutate it locally. When the target lives upstream (seeded or
    /// live-captured records the proxy never created), the request is forwarded so
    /// the real backend response is used instead of a synthetic one.
    pub(in crate::proxy) fn metaobject_mutation_is_local(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().all(|field| match field.name.as_str() {
            "metaobjectUpdate" => resolved_string_field(&field.arguments, "id")
                .map(|id| self.metaobject_by_id(&id).is_some())
                .unwrap_or(false),
            "metaobjectUpsert" => match field.arguments.get("handle") {
                Some(ResolvedValue::Object(handle)) => resolved_string_field(handle, "type")
                    .map(|meta_type| self.metaobject_definition_by_type(&meta_type).is_some())
                    .unwrap_or(false),
                _ => false,
            },
            "metaobjectDefinitionUpdate" => resolved_string_field(&field.arguments, "id")
                .map(|id| self.metaobject_definition_by_id(&id).is_some())
                .unwrap_or(false),
            // Creates and deletes are always emulated locally.
            _ => true,
        })
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
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_by_id(&id)
                        .map(|record| self.project_metaobject_against_definition(&record))
                        .map(|record| self.selected_metaobject(&record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
                "metaobjectDefinition" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
            // Each successful metaobject mutation reserves one synthetic id for its
            // mutation-log entry after allocating the resources it creates, matching
            // the Gleam reference's id bookkeeping (e.g. a definition lands on /1 and
            // the next entry on /3 because the definition's log entry consumed /2).
            self.reserve_synthetic_log_id();
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
        if self.store.staged.metaobjects.is_tombstoned(id) {
            return None;
        }
        if let Some(record) = self.store.staged.metaobjects.get(id) {
            return Some(record.clone());
        }
        None
    }

    /// Resolve a linked metaobject reference (the gid stored in a product option's
    /// `linkedMetafieldValue`) to its display name, projected against the current
    /// definition. Used to render product option values whose names mirror the linked
    /// metaobject entry (e.g. "One"/"Two") rather than echoing the raw gid.
    pub(in crate::proxy) fn linked_metaobject_display_name(&self, id: &str) -> Option<String> {
        let record = self.metaobject_by_id(id)?;
        self.project_metaobject_against_definition(&record)
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn hydrate_metaobject_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetaobjectHydrateById($id: ID!) { node(id: $id) { __typename } metaobject(id: $id) { id handle type displayName updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } titleField: field(key: \"title\") { key type value jsonValue definition { key name required type { name category } } } } }",
                "variables": {"id": id}
            }),
        );
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
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetaobjectHydrateByHandle($type: String!, $handle: String!) { metaobjectByHandle(handle: { type: $type, handle: $handle }) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }",
                "variables": {"type": meta_type, "handle": meta_handle}
            }),
        );
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
                    .metaobject_definitions
                    .tombstones
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
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "variables": {"type": meta_type}
            }),
        );
        if let Some(definition) = response.body["data"]
            .get("definition")
            .or_else(|| response.body["data"].get("metaobjectDefinitionByType"))
            .filter(|value| value.is_object())
            .cloned()
        {
            if let Some(id) = definition.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .metaobject_definitions
                    .tombstones
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
            .map(|record| self.project_metaobject_against_definition(&record))
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
                        .metaobjects
                        .is_tombstoned(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
    }

    pub(in crate::proxy) fn metaobject_connection(&self, field: &RootFieldSelection) -> Value {
        let meta_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        let mut records: Vec<Value> =
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .map(|record| self.project_metaobject_against_definition(record))
                // A row whose required display field has no value is not yet surfaced
                // by the Admin search index that backs `metaobjects(type:)`.
                .filter(|record| self.metaobject_visible_in_catalog(record))
                .collect();
        // Shopify's default `metaobjects(type:)` ordering is ascending by
        // creation, which corresponds to ascending numeric id. A lexicographic
        // sort on the full gid is wrong once ids cross a digit boundary
        // (".../10" sorts before ".../8" as strings), so compare the trailing
        // numeric id, falling back to the full string when it is not numeric.
        fn metaobject_id_sort_key(record: &Value) -> (u64, String) {
            let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
            let numeric = id
                .parse::<u64>()
                .ok()
                .or_else(|| resource_id_tail(id).parse::<u64>().ok())
                .unwrap_or(u64::MAX);
            (numeric, id.to_string())
        }
        records.sort_by(|left, right| {
            metaobject_id_sort_key(left).cmp(&metaobject_id_sort_key(right))
        });
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |record, selection| self.selected_metaobject(record, selection),
            metaobject_cursor,
        )
    }

    pub(in crate::proxy) fn url_redirect_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "urlRedirect" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
        let query = resolved_string_field(&field.arguments, "query");
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
                    "userErrors": [metaobject_no_definition_error(
                        "metaobject",
                        &meta_type,
                        "UNDEFINED_OBJECT_TYPE",
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
        let validation_context = MetaobjectFieldValueValidationContext {
            proxy: self,
            existing_id: None,
            validate_existing_values: false,
        };
        let mut validation_errors = metaobject_create_validation_errors(
            Some(&validation_context),
            input,
            &definition,
            &input_values,
            false,
        );
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
        let handle_choice = if let Some(requested_handle) = resolved_string_field(input, "handle") {
            self.available_metaobject_handle(&meta_type, &requested_handle)
        } else {
            self.available_generated_metaobject_handle(&meta_type, &id)
        };
        let display_name =
            metaobject_display_name(&definition, &input_values, &handle_choice.display_source);
        let publishable_status = metaobject_publishable_status(input, &definition);
        let record = metaobject_record_from_definition_with_options(
            &id,
            &handle_choice.handle,
            &definition,
            &input_values,
            &display_name,
            &publishable_status,
            "2026-01-01T00:00:00Z",
        );
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
        handle_display_source: &str,
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
        let display_name = metaobject_display_name(definition, input_values, handle_display_source);
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                    "userErrors": [metaobject_no_definition_error(
                        "metaobject",
                        &meta_type,
                        "UNDEFINED_OBJECT_TYPE",
                    )]
                }),
                &field.selection,
            );
        };

        let input_values = metaobject_merged_input_values(&existing, input);
        let validation_context = MetaobjectFieldValueValidationContext {
            proxy: self,
            existing_id: Some(&id),
            validate_existing_values: true,
        };
        let mut validation_errors = metaobject_create_validation_errors(
            Some(&validation_context),
            input,
            &definition,
            &input_values,
            true,
        );
        let requested_handle = resolved_string_field(input, "handle");
        let (next_handle, handle_display_source) = if let Some(handle) = requested_handle.as_deref()
        {
            validation_errors.extend(metaobject_handle_validation_errors(
                handle,
                vec!["metaobject", "handle"],
            ));
            let normalized = slugify_handle(handle);
            if self.metaobject_handle_belongs_to_other_case_insensitive(
                &meta_type,
                &normalized,
                &id,
            ) {
                validation_errors.push(metaobject_user_error(
                    vec!["metaobject", "handle"],
                    "Handle has already been taken",
                    "TAKEN",
                    Value::Null,
                    Value::Null,
                ));
            }
            (normalized, handle.to_string())
        } else {
            let existing_handle = existing
                .get("handle")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            (existing_handle.clone(), existing_handle)
        };
        validation_errors.extend(self.metaobject_display_name_conflict_errors(
            &id,
            &definition,
            input,
            &input_values,
            &handle_display_source,
        ));
        if !validation_errors.is_empty() {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": validation_errors}),
                &field.selection,
            );
        }

        let display_name =
            metaobject_display_name(&definition, &input_values, &handle_display_source);
        let publishable_status =
            metaobject_updated_publishable_status(input, &definition, &existing);
        // `_with_options` nulls the publishable capability when the definition has it
        // disabled (e.g. after a schema change turned it off), matching how Shopify
        // reads back entries whose definition no longer exposes the capability.
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
        let meta_handle_input = resolved_string_field(handle, "handle").unwrap_or_default();
        let meta_handle = slugify_handle(&meta_handle_input);
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
                        "userErrors": [metaobject_no_definition_error(
                            "handle",
                            &meta_type,
                            "UNDEFINED_OBJECT_TYPE",
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
            let existing_id = existing
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let validation_context = MetaobjectFieldValueValidationContext {
                proxy: self,
                existing_id: Some(&existing_id),
                validate_existing_values: true,
            };
            let mut validation_errors = metaobject_required_field_errors_for_upsert(
                metaobject_create_validation_errors(
                    Some(&validation_context),
                    &update_input,
                    &definition,
                    &input_values,
                    true,
                ),
                &definition,
            );
            let (next_handle, handle_display_source) =
                if let Some(handle) = resolved_string_field(&update_input, "handle") {
                    validation_errors.extend(metaobject_handle_validation_errors(
                        &handle,
                        vec!["handle", "handle"],
                    ));
                    let normalized = slugify_handle(&handle);
                    if !self.metaobject_handle_exists_case_insensitive(&meta_type, &normalized) {
                        self.hydrate_metaobject_by_handle(request, &meta_type, &normalized);
                    }
                    if self.metaobject_handle_belongs_to_other_case_insensitive(
                        &meta_type,
                        &normalized,
                        &existing_id,
                    ) {
                        validation_errors.push(metaobject_user_error(
                            vec!["handle", "handle"],
                            "Handle has already been taken",
                            "TAKEN",
                            Value::Null,
                            Value::Null,
                        ));
                    }
                    (normalized, handle)
                } else {
                    let existing_handle = existing
                        .get("handle")
                        .and_then(Value::as_str)
                        .unwrap_or(meta_handle.as_str())
                        .to_string();
                    (existing_handle, meta_handle_input.clone())
                };
            validation_errors.extend(self.metaobject_display_name_conflict_errors(
                &existing_id,
                &definition,
                &update_input,
                &input_values,
                &handle_display_source,
            ));
            if !validation_errors.is_empty() {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": validation_errors}),
                    &field.selection,
                );
            }
            let id = existing_id;
            let display_name =
                metaobject_display_name(&definition, &input_values, &handle_display_source);
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
                    "userErrors": [metaobject_no_definition_error(
                        "handle",
                        &meta_type,
                        "UNDEFINED_OBJECT_TYPE",
                    )]
                }),
                &field.selection,
            );
        };
        let mut create_input = input.clone();
        create_input.insert("type".to_string(), ResolvedValue::String(meta_type));
        create_input.insert(
            "handle".to_string(),
            ResolvedValue::String(meta_handle_input),
        );
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self.metaobject_by_id(&id).is_none()
            && self.hydrate_metaobject_by_id(request, &id).is_none()
        {
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [metaobject_indexed_user_error(
                        ["id"],
                        "Record not found",
                        Some("RECORD_NOT_FOUND"),
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        }
        let record = self.metaobject_by_id(&id).unwrap_or(Value::Null);
        self.store.staged.metaobjects.remove(&id);
        self.store.staged.metaobjects.tombstone(id.clone());
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
                    self.store.staged.metaobjects.tombstone(id.clone());
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
                        "userErrors": [metaobject_no_definition_error(
                            "where",
                            &meta_type,
                            "RECORD_NOT_FOUND",
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
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
                .collect::<Vec<_>>();
            for id in ids_to_delete {
                self.store.staged.metaobjects.remove(&id);
                self.store.staged.metaobjects.tombstone(id.clone());
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
                let key = resolved_string_field(&field.arguments, "key").unwrap_or_default();
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

    /// Re-projects a stored metaobject entry against the current local definition for
    /// its type, so reads reflect schema changes applied after the entry was created:
    /// field definitions (key/name/required/type), field order, newly-added fields
    /// (read as `null`), dropped fields, the recomputed `displayName`, and the
    /// `publishable` capability all follow the live definition. Stored field VALUES
    /// are preserved verbatim. When no local definition is staged for the type (e.g.
    /// an upstream-hydrated entry), the record is returned unchanged.
    fn project_metaobject_against_definition(&self, record: &Value) -> Value {
        let Some(meta_type) = record.get("type").and_then(Value::as_str) else {
            return record.clone();
        };
        let Some(definition) = self.metaobject_definition_by_type(meta_type) else {
            return record.clone();
        };
        let mut stored: BTreeMap<String, (Value, Value)> = BTreeMap::new();
        if let Some(fields) = record["fields"].as_array() {
            for entry in fields {
                if let Some(key) = entry.get("key").and_then(Value::as_str) {
                    stored.insert(
                        key.to_string(),
                        (
                            entry.get("value").cloned().unwrap_or(Value::Null),
                            entry.get("jsonValue").cloned().unwrap_or(Value::Null),
                        ),
                    );
                }
            }
        }
        let fields = definition["fieldDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|field_definition| {
                let key = field_definition
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or("single_line_text_field");
                let (value, json_value) = stored
                    .get(key)
                    .cloned()
                    .unwrap_or((Value::Null, Value::Null));
                json!({
                    "key": key,
                    "type": field_type,
                    "value": value,
                    "jsonValue": json_value,
                    "definition": field_definition
                })
            })
            .collect::<Vec<_>>();

        let display_field_present = definition
            .get("displayNameKey")
            .and_then(Value::as_str)
            .and_then(|key| stored.get(key))
            .and_then(|(value, _)| value.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        let display_name = if display_field_present {
            // Keep the displayName the write path already computed for this field.
            // Shopify renders displayName from the raw input, which can differ from
            // the normalized stored field value (e.g. a measurement `60` vs `60.0`,
            // `kilometers_per_hour` vs `KILOMETERS_PER_HOUR`), so re-deriving it from
            // the stored field value here would corrupt it.
            record.get("displayName").cloned().unwrap_or(Value::Null)
        } else {
            // A blank display field (e.g. a schema change moved displayNameKey onto a
            // field this row never set) falls back to the entry's handle, title-cased
            // ("codex-har-245-pre-..." -> "Codex Har 245 Pre ...").
            json!(metaobject_field_name(
                record
                    .get("handle")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ))
        };

        let publishable_enabled = definition["capabilities"]["publishable"]["enabled"]
            .as_bool()
            .unwrap_or(false);
        let publishable = if publishable_enabled {
            record["capabilities"]["publishable"].clone()
        } else {
            Value::Null
        };

        let mut projected = record.clone();
        projected["fields"] = json!(fields);
        projected["displayName"] = display_name;
        if let Some(capabilities) = projected
            .get_mut("capabilities")
            .and_then(Value::as_object_mut)
        {
            capabilities.insert("publishable".to_string(), publishable);
        }
        projected
    }

    /// Whether a (already definition-projected) entry is visible in an immediate
    /// `metaobjects(type:)` catalog read. Rows missing a value for a required display
    /// field are omitted, matching Shopify's behaviour where such rows are not yet
    /// surfaced by the Admin search index.
    fn metaobject_visible_in_catalog(&self, projected: &Value) -> bool {
        let Some(meta_type) = projected.get("type").and_then(Value::as_str) else {
            return true;
        };
        let Some(definition) = self.metaobject_definition_by_type(meta_type) else {
            return true;
        };
        let Some(display_key) = definition.get("displayNameKey").and_then(Value::as_str) else {
            return true;
        };
        let display_required = definition["fieldDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field.get("key").and_then(Value::as_str) == Some(display_key))
            .and_then(|field| field.get("required").and_then(Value::as_bool))
            .unwrap_or(false);
        if !display_required {
            return true;
        }
        projected["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field.get("key").and_then(Value::as_str) == Some(display_key))
            .and_then(|field| field.get("value").and_then(Value::as_str))
            .is_some_and(|value| !value.trim().is_empty())
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
            .filter(|(id, _)| !self.store.staged.metaobject_definitions.is_tombstoned(id))
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
            .metaobject_definitions
            .tombstones
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
        // A metaobject definition that backs a product option's linked metafield has
        // an immutable display name field: Shopify forbids re-pointing displayNameKey
        // while the definition is linked, because the option values are surfaced by
        // their resolved display name. The link set is populated by
        // `record_product_option_linked_metaobject_definitions` during
        // productOptionsCreate.
        if self
            .store
            .staged
            .product_option_linked_metaobject_definition_ids
            .contains(&id)
        {
            let current_display_name_key = definition.get("displayNameKey").and_then(Value::as_str);
            let changes_display_name_key =
                resolved_string_field(definition_input, "displayNameKey")
                    .is_some_and(|next| Some(next.as_str()) != current_display_name_key);
            if changes_display_name_key {
                return selected_json(
                    &json!({
                        "metaobjectDefinition": null,
                        "userErrors": [metaobject_user_error(
                            vec!["definition", "displayNameKey"],
                            "Cannot change display name field when metaobject is used in product options",
                            "IMMUTABLE",
                            Value::Null,
                            Value::Null,
                        )]
                    }),
                    &field.selection,
                );
            }
        }
        let existing_field_definitions = definition["fieldDefinitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let validation_errors = metaobject_definition_update_validation_errors(
            definition_input,
            meta_type,
            &existing_field_definitions,
        );
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
            self.store.staged.metaobjects.tombstone(metaobject_id);
        }
        self.store.staged.metaobject_definitions.remove(&id);
        self.store
            .staged
            .metaobject_definitions
            .tombstone(id.clone());
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_by_id(&self, id: &str) -> Option<Value> {
        if self.store.staged.metaobject_definitions.is_tombstoned(id) {
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
                    && !self.store.staged.metaobject_definitions.is_tombstoned(
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
        let body = json!({
            "query": query,
            "variables": {"type": meta_type}
        });
        let response = self.upstream_post(request, body);
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
            .metaobject_definitions
            .tombstones
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
                !self.store.staged.metaobject_definitions.is_tombstoned(
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

    fn available_generated_metaobject_handle(
        &self,
        meta_type: &str,
        id: &str,
    ) -> MetaobjectHandleChoice {
        let base = slugify_handle(meta_type);
        let base = if base.is_empty() {
            "metaobject".to_string()
        } else {
            base
        };
        for attempt in 0.. {
            let suffix = metaobject_random_handle_suffix(meta_type, id, attempt);
            let candidate = format!("{base}-{suffix}");
            if !self.metaobject_handle_exists_case_insensitive(meta_type, &candidate) {
                return MetaobjectHandleChoice {
                    handle: candidate.clone(),
                    display_source: candidate,
                };
            }
        }
        unreachable!("infinite random handle search must return")
    }

    fn available_metaobject_handle(
        &self,
        meta_type: &str,
        requested: &str,
    ) -> MetaobjectHandleChoice {
        let base = slugify_handle(requested);
        let base = if base.is_empty() {
            format!("{meta_type}-{}", self.next_synthetic_id)
        } else {
            base
        };
        let display_base = if requested.trim().is_empty() {
            base.clone()
        } else {
            requested.trim().to_string()
        };
        if !self.metaobject_handle_exists_case_insensitive(meta_type, &base) {
            return MetaobjectHandleChoice {
                handle: base,
                display_source: display_base,
            };
        }
        for suffix in 1.. {
            let candidate = format!("{base}-{suffix}");
            if !self.metaobject_handle_exists_case_insensitive(meta_type, &candidate) {
                return MetaobjectHandleChoice {
                    handle: candidate,
                    display_source: format!("{display_base}-{suffix}"),
                };
            }
        }
        unreachable!("infinite suffix search must return")
    }

    fn metaobject_handle_exists_case_insensitive(&self, meta_type: &str, handle: &str) -> bool {
        self.store.staged.metaobjects.values().any(|record| {
            record.get("type").and_then(Value::as_str) == Some(meta_type)
                && record
                    .get("handle")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(handle))
                && !self
                    .store
                    .staged
                    .metaobjects
                    .is_tombstoned(record.get("id").and_then(Value::as_str).unwrap_or_default())
        })
    }

    fn metaobject_handle_belongs_to_other_case_insensitive(
        &self,
        meta_type: &str,
        handle: &str,
        current_id: &str,
    ) -> bool {
        self.store.staged.metaobjects.values().any(|record| {
            record.get("type").and_then(Value::as_str) == Some(meta_type)
                && record
                    .get("handle")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(handle))
                && record.get("id").and_then(Value::as_str) != Some(current_id)
                && !self
                    .store
                    .staged
                    .metaobjects
                    .is_tombstoned(record.get("id").and_then(Value::as_str).unwrap_or_default())
        })
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

use super::*;

use base64::Engine as _;

fn record_matches_type(record: &Value, typename: &str) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some(typename)
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| is_shopify_gid_of_type(id, typename))
}

pub(in crate::proxy) fn is_online_store_theme_record(record: &Value) -> bool {
    record_matches_type(record, "OnlineStoreTheme")
}

pub(in crate::proxy) fn is_online_store_script_tag_record(record: &Value) -> bool {
    record_matches_type(record, "ScriptTag")
}

pub(in crate::proxy) fn is_web_pixel_record(record: &Value) -> bool {
    record_matches_type(record, "WebPixel")
}

pub(in crate::proxy) fn is_server_pixel_record(record: &Value) -> bool {
    record_matches_type(record, "ServerPixel")
}

pub(in crate::proxy) fn is_mobile_platform_application_record(record: &Value) -> bool {
    matches!(
        record.get("__typename").and_then(Value::as_str),
        Some("AppleApplication" | "AndroidApplication")
    ) || record_matches_type(record, "MobilePlatformApplication")
}

pub(in crate::proxy) fn is_storefront_access_token_record(record: &Value) -> bool {
    record_matches_type(record, "StorefrontAccessToken")
}

pub(in crate::proxy) fn web_pixel_settings_from_resolved(value: &ResolvedValue) -> Option<Value> {
    match value {
        ResolvedValue::String(raw) => serde_json::from_str::<Value>(raw).ok(),
        ResolvedValue::Object(_) | ResolvedValue::List(_) => Some(resolved_value_json(value)),
        ResolvedValue::Null => None,
        _ => Some(resolved_value_json(value)),
    }
}

pub(in crate::proxy) fn synthetic_storefront_access_token(id: &str) -> String {
    let suffix = resource_id_tail(id).parse::<u64>().ok().unwrap_or(0);
    format!(
        "shpat_{:016x}",
        0xbcc6_fd83_f411_23b4u64.wrapping_add(suffix)
    )
}

pub(in crate::proxy) fn storefront_access_scopes_for_request(request: &Request) -> Vec<Value> {
    let scopes = request
        .headers
        .get("x-shopify-draft-proxy-access-scopes")
        .map(|header| {
            header
                .split(',')
                .map(str::trim)
                .filter(|scope| scope.starts_with("unauthenticated_"))
                .map(|scope| json!({"handle": scope}))
                .collect::<Vec<_>>()
        })
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| {
            vec![
                json!({"handle": "unauthenticated_read_product_listings"}),
                json!({"handle": "unauthenticated_read_product_inventory"}),
            ]
        });
    scopes
}

pub(in crate::proxy) fn theme_file_nodes(theme: &Value) -> Vec<Value> {
    theme["files"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(in crate::proxy) fn set_theme_file_nodes(theme: &mut Value, nodes: Vec<Value>) {
    if let Some(object) = theme.as_object_mut() {
        object.insert("files".to_string(), json!({"nodes": nodes}));
    }
}

pub(in crate::proxy) fn theme_file_arg_string(
    value: &ResolvedValue,
    field: &str,
) -> Option<String> {
    match value {
        ResolvedValue::Object(input) => resolved_string_field(input, field),
        _ => None,
    }
}

pub(in crate::proxy) fn theme_file_record_from_input(
    value: &ResolvedValue,
) -> Result<Option<Value>, ()> {
    let ResolvedValue::Object(input) = value else {
        return Ok(None);
    };
    let Some(filename) = resolved_string_field(input, "filename") else {
        return Ok(None);
    };
    let Some(body) = input.get("body") else {
        return Err(());
    };
    Ok(Some(theme_file_record_from_body(&filename, body)?))
}

pub(in crate::proxy) fn theme_file_input_uses_url_body(value: &ResolvedValue) -> bool {
    let ResolvedValue::Object(input) = value else {
        return false;
    };
    let Some(ResolvedValue::Object(body)) = input.get("body") else {
        return false;
    };
    resolved_string_field(body, "type").is_some_and(|body_type| body_type == "URL")
}

pub(in crate::proxy) fn theme_file_record(filename: &str, content: &str) -> Value {
    json!({
        "filename": filename,
        "checksumMd5": theme_file_checksum_md5(content),
        "size": content.len(),
        "body": {"content": content}
    })
}

pub(in crate::proxy) fn theme_file_record_from_body(
    filename: &str,
    body: &ResolvedValue,
) -> Result<Value, ()> {
    let ResolvedValue::Object(body) = body else {
        return Err(());
    };
    let body_type = resolved_string_field(body, "type").unwrap_or_else(|| "TEXT".to_string());
    let value = resolved_string_field(body, "value").unwrap_or_default();
    match body_type.as_str() {
        "TEXT" => Ok(theme_file_record(filename, &value)),
        "BASE64" => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(value.as_bytes())
                .map_err(|_| ())?;
            let content = String::from_utf8(bytes).map_err(|_| ())?;
            Ok(theme_file_record(filename, &content))
        }
        "URL" => Ok(theme_file_url_record(filename)),
        _ => Err(()),
    }
}

pub(in crate::proxy) fn theme_file_url_record(filename: &str) -> Value {
    json!({
        "filename": filename,
        "checksumMd5": theme_file_checksum_md5(""),
        "size": 0,
        "body": {"type": "URL", "value": Value::Null}
    })
}

pub(in crate::proxy) fn theme_file_operation_result(record: &Value) -> Value {
    json!({
        "filename": record["filename"],
        "createdAt": record
            .get("createdAt")
            .cloned()
            .unwrap_or(Value::Null),
        "updatedAt": record
            .get("updatedAt")
            .cloned()
            .unwrap_or(Value::Null),
        "checksumMd5": record["checksumMd5"],
        "size": record["size"],
        "body": record
            .get("body")
            .cloned()
            .unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn theme_file_checksum_md5(content: &str) -> String {
    format!("{:x}", md5::compute(content.as_bytes()))
}

pub(in crate::proxy) fn mobile_app_error<const N: usize>(
    code: &str,
    field: [&str; N],
    message: &str,
) -> Value {
    user_error(field, message, Some(code))
}

pub(in crate::proxy) fn mobile_app_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    resource_payload(
        selection,
        "mobilePlatformApplication",
        record.unwrap_or(Value::Null),
        errors,
    )
}

pub(in crate::proxy) fn script_tag_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    resource_payload(
        selection,
        "scriptTag",
        record.unwrap_or(Value::Null),
        errors,
    )
}

pub(in crate::proxy) fn resource_payload(
    selection: &[SelectedField],
    resource_key: &str,
    resource: Value,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            resource_key: resource,
            "userErrors": user_errors
        }),
        selection,
    )
}

pub(in crate::proxy) fn theme_payload(
    selection: &[SelectedField],
    record: Value,
    errors: Vec<Value>,
) -> Value {
    resource_payload(selection, "theme", record, errors)
}

pub(in crate::proxy) fn deleted_theme_payload(
    selection: &[SelectedField],
    deleted_id: Value,
    errors: Vec<Value>,
) -> Value {
    resource_payload(selection, "deletedThemeId", deleted_id, errors)
}

pub(in crate::proxy) fn deleted_script_tag_payload(
    selection: &[SelectedField],
    deleted_id: Value,
    errors: Vec<Value>,
) -> Value {
    resource_payload(selection, "deletedScriptTagId", deleted_id, errors)
}

pub(in crate::proxy) fn web_pixel_payload(
    selection: &[SelectedField],
    record: Value,
    errors: Vec<Value>,
) -> Value {
    resource_payload(selection, "webPixel", record, errors)
}

pub(in crate::proxy) fn server_pixel_payload(
    selection: &[SelectedField],
    record: Value,
    errors: Vec<Value>,
) -> Value {
    resource_payload(selection, "serverPixel", record, errors)
}

pub(in crate::proxy) fn storefront_access_token_payload(
    selection: &[SelectedField],
    record: Value,
    shop: Value,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"storefrontAccessToken": record, "shop": shop, "userErrors": errors}),
        selection,
    )
}

pub(in crate::proxy) fn validate_script_src(
    input: &BTreeMap<String, ResolvedValue>,
    create: bool,
) -> Option<Value> {
    let src = resolved_string_field(input, "src")?;
    let field = if create {
        json!(["input", "src"])
    } else {
        json!(["src"])
    };
    if src.trim().is_empty() {
        return Some(presence_user_error(field, "Source"));
    }
    if src.len() > 255 {
        return Some(length_user_error(
            field,
            "Source",
            LengthUserErrorBound::TooLong { maximum: 255 },
        ));
    }
    if !(src.starts_with("https://") && src.contains('.')) {
        return Some(user_error(field, "Source is invalid", Some("INVALID")));
    }
    None
}

pub(in crate::proxy) const MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH: usize = 100;
pub(in crate::proxy) const MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH: usize = 255;
pub(in crate::proxy) const THEME_FILES_MAX_FILE_INPUT: usize = 50;
pub(in crate::proxy) const THEME_FILES_MAX_FILE_LIMIT: usize = 100;
pub(in crate::proxy) const THEME_UNDELETABLE_FILES: &[&str] = &[
    "config/settings_data.json",
    "config/settings_schema.json",
    "layout/theme.liquid",
];

pub(in crate::proxy) fn mobile_app_id_length_error(
    platform: &str,
    field_name: &str,
    value: &str,
) -> Option<Value> {
    (value.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH).then(|| {
        length_user_error(
            vec![
                "input".to_string(),
                platform.to_string(),
                field_name.to_string(),
            ],
            "Application ID",
            LengthUserErrorBound::TooLong {
                maximum: MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH,
            },
        )
    })
}

pub(in crate::proxy) fn theme_file_user_error(
    field: Vec<String>,
    message: &str,
    code: &str,
) -> Value {
    user_error(field, message, Some(code))
}

pub(in crate::proxy) fn theme_file_limit_error() -> Value {
    theme_file_user_error(
        vec!["files".to_string()],
        "Exceeded maximum number of files",
        "INVALID",
    )
}

pub(in crate::proxy) fn theme_file_duplicate_error(index: usize, field_name: &str) -> Value {
    theme_file_user_error(
        vec![
            "files".to_string(),
            index.to_string(),
            field_name.to_string(),
        ],
        "duplicate-file-input",
        "INVALID",
    )
}

pub(in crate::proxy) fn theme_file_field_error(
    index: usize,
    field_name: &str,
    message: &str,
    code: &str,
) -> Value {
    theme_file_user_error(
        vec![
            "files".to_string(),
            index.to_string(),
            field_name.to_string(),
        ],
        message,
        code,
    )
}

pub(in crate::proxy) fn theme_file_delete_error(index: usize, message: &str, code: &str) -> Value {
    theme_file_user_error(vec!["files".to_string(), index.to_string()], message, code)
}

pub(in crate::proxy) fn theme_file_filename_allowed(filename: &str) -> bool {
    let Some((root, rest)) = filename.split_once('/') else {
        return false;
    };
    matches!(
        root,
        "assets" | "config" | "layout" | "locales" | "sections" | "snippets" | "templates"
    ) && !rest.is_empty()
        && !rest.ends_with('/')
        && !rest.contains("//")
        && !filename.split('/').any(|segment| segment == "..")
}

pub(in crate::proxy) fn theme_file_filename_error(index: usize, filename: &str) -> Option<Value> {
    if filename.trim().is_empty() {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Filename can't be blank",
            "INVALID",
        ));
    }
    if filename == "_drafts" || filename.starts_with("_drafts/") || filename.contains("/_drafts/") {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Access denied",
            "ACCESS_DENIED",
        ));
    }
    if !theme_file_filename_allowed(filename) {
        return Some(theme_file_field_error(
            index,
            "filename",
            "Filename is invalid",
            "INVALID",
        ));
    }
    None
}
pub(in crate::proxy) fn validate_mobile_app_clip_application_id(
    apple: &BTreeMap<String, ResolvedValue>,
    update_input: bool,
) -> Option<Value> {
    let app_clips_enabled = resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false);
    let app_clip_application_id = resolved_string_field(apple, "appClipApplicationId");
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Some(presence_user_error(
            ["input", "apple", "appClipApplicationId"],
            "App clip application",
        ));
    }
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.len() > MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH)
    {
        return Some(length_user_error(
            ["input", "apple", "appClipApplicationId"],
            "App clip application",
            LengthUserErrorBound::TooLong {
                maximum: MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH,
            },
        ));
    }
    if update_input
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Some(presence_user_error(
            ["input", "apple", "appClipApplicationId"],
            "App clip application",
        ));
    }
    None
}
/// Validates the arguments of a server-pixel endpoint mutation
/// (`eventBridgeServerPixelUpdate` / `pubSubServerPixelUpdate`), returning the top-level
/// GraphQL error Shopify raises before executing the mutation, if any.
pub(in crate::proxy) fn server_pixel_endpoint_argument_error(
    field: &RootFieldSelection,
) -> Option<Value> {
    match field.name.as_str() {
        "eventBridgeServerPixelUpdate" => match resolved_string_field(&field.arguments, "arn") {
            None => Some(server_pixel_missing_argument_error(field, "arn")),
            Some(arn) if !is_valid_event_bridge_arn(&arn) => {
                Some(server_pixel_arn_coercion_error(&arn))
            }
            Some(_) => None,
        },
        "pubSubServerPixelUpdate" => {
            let project = resolved_string_field(&field.arguments, "pubSubProject");
            if project.is_none() {
                return Some(server_pixel_missing_argument_error(field, "pubSubProject"));
            }
            let topic = resolved_string_field(&field.arguments, "pubSubTopic");
            if topic.is_none() {
                return Some(server_pixel_missing_argument_error(field, "pubSubTopic"));
            }
            if project.as_deref().unwrap_or_default().trim().is_empty() {
                return Some(server_pixel_blank_argument_error(field, "pubSubProject"));
            }
            if topic.as_deref().unwrap_or_default().trim().is_empty() {
                return Some(server_pixel_blank_argument_error(field, "pubSubTopic"));
            }
            None
        }
        _ => None,
    }
}

pub(in crate::proxy) fn is_valid_event_bridge_arn(arn: &str) -> bool {
    !arn.trim().is_empty() && arn.starts_with("arn:aws:events:")
}

pub(in crate::proxy) fn server_pixel_missing_argument_error(
    field: &RootFieldSelection,
    argument_name: &str,
) -> Value {
    json!({
        "message": format!(
            "Field '{}' is missing required arguments: {}",
            field.name, argument_name
        ),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": [field.response_key],
        "extensions": {
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": field.name,
            "arguments": argument_name
        }
    })
}

pub(in crate::proxy) fn server_pixel_blank_argument_error(
    field: &RootFieldSelection,
    argument_name: &str,
) -> Value {
    json!({
        "message": format!("{argument_name} can't be blank"),
        "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
        "path": [field.response_key]
    })
}

pub(in crate::proxy) fn server_pixel_arn_coercion_error(arn: &str) -> Value {
    json!({
        "message": format!("Invalid ARN '{arn}'"),
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

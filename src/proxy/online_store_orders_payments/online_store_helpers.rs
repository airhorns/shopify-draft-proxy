use super::*;

use base64::Engine as _;

pub(in crate::proxy) fn is_online_store_theme_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("OnlineStoreTheme")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/OnlineStoreTheme/"))
}

pub(in crate::proxy) fn is_online_store_script_tag_record(record: &Value) -> bool {
    record
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with("gid://shopify/ScriptTag/"))
}

pub(in crate::proxy) fn is_web_pixel_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("WebPixel")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/WebPixel/"))
}

pub(in crate::proxy) fn is_server_pixel_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("ServerPixel")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/ServerPixel/"))
}

pub(in crate::proxy) fn is_storefront_access_token_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("StorefrontAccessToken")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/StorefrontAccessToken/"))
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
    let token = match suffix {
        1 => "bcc6fd83f41123b4",
        3 => "43199f7763e24d2f",
        5 => "5ceddc5ce1576036",
        _ => {
            return format!(
                "shpat_{:016x}",
                0xbcc6_fd83_f411_23b4u64.wrapping_add(suffix)
            )
        }
    };
    format!("shpat_{token}")
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

pub(in crate::proxy) fn theme_user_error(
    field: Vec<&str>,
    message: &str,
    code: Option<&str>,
) -> Value {
    let field: Vec<&str> = field.into_iter().collect();
    let mut error = json!({"field": field, "message": message});
    if let Some(code) = code {
        error["code"] = json!(code);
    }
    error
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
            .unwrap_or_else(|| json!("2024-01-01T00:00:00.000Z")),
        "updatedAt": record
            .get("updatedAt")
            .cloned()
            .unwrap_or_else(|| json!("2024-01-01T00:00:00.000Z")),
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
    let field: Vec<&str> = field.into_iter().collect();
    json!({"code": code, "field": field, "message": message})
}

pub(in crate::proxy) fn mobile_app_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"mobilePlatformApplication": record, "userErrors": errors}),
        selection,
    )
}

pub(in crate::proxy) fn script_tag_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"scriptTag": record, "userErrors": errors}),
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
        return Some(json!({"code": "BLANK", "field": field, "message": "Source can't be blank"}));
    }
    if src.len() > 255 {
        return Some(
            json!({"code": "TOO_LONG", "field": field, "message": "Source is too long (maximum is 255 characters)"}),
        );
    }
    if !(src.starts_with("https://") && src.contains('.')) {
        return Some(json!({"code": "INVALID", "field": field, "message": "Source is invalid"}));
    }
    None
}

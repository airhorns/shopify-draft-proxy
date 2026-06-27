use super::*;

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
                .map(|namespace| canonical_app_metafield_namespace(Some(&namespace)))
                .unwrap_or_default();
            json!({
                "id": shopify_gid("Metafield", format_args!("payment-customization-{}", index + 1)),
                "namespace": namespace,
                "key": resolved_string_field(&metafield, "key").unwrap_or_default(),
                "type": resolved_string_field(&metafield, "type").unwrap_or_default(),
                "value": resolved_string_field(&metafield, "value").unwrap_or_default(),
                "createdAt": format!("2024-01-01T00:00:{:02}.000Z", (index as u64 + 1) % 60),
                "updatedAt": format!("2024-01-01T00:00:{:02}.000Z", (index as u64 + 1) % 60)
            })
        })
        .collect()
}

pub(in crate::proxy) fn payment_customization_set_metafields(
    record: &mut Value,
    metafields: Vec<Value>,
) {
    let mut connection = connection_json_with_cursor(
        metafields.clone(),
        |index, _| format!("cursor{}", index + 1),
        empty_page_info(),
    );
    if let Some(connection) = connection.as_object_mut() {
        connection.remove("pageInfo");
    }
    record["metafield"] = metafields.first().cloned().unwrap_or(Value::Null);
    record["metafields"] = connection;
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
    user_error(field, message, Some(code))
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
    user_error(
        json!([
            "paymentCustomization",
            "metafields",
            index.to_string(),
            field
        ]),
        message,
        Some("INVALID_METAFIELDS"),
    )
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

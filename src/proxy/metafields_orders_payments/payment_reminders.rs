use super::*;

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
        resolved_string_field(&field.arguments, "paymentScheduleId").unwrap_or_default();

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
        "userErrors": [user_error(Value::Null, message, Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"))]
    })
}

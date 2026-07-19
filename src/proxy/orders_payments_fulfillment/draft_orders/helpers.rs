use super::*;

pub(in crate::proxy) fn normalize_draft_order_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

pub(in crate::proxy) fn draft_order_invoice_recipient(
    args: &BTreeMap<String, ResolvedValue>,
    draft_order: &Value,
) -> Option<String> {
    let recipient = resolved_object_field(args, "email")
        .and_then(|email| resolved_string_field(&email, "to"))
        .or_else(|| draft_order["email"].as_str().map(str::to_string))?;
    let trimmed = recipient.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(in crate::proxy) fn draft_order_invoice_send_metadata(
    args: &BTreeMap<String, ResolvedValue>,
    draft_order: &Value,
) -> Value {
    let email_arg = resolved_object_field(args, "email");
    let recipient = email_arg
        .as_ref()
        .and_then(|email| resolved_string_field(email, "to"))
        .or_else(|| draft_order["email"].as_str().map(str::to_string));

    let mut email = serde_json::Map::new();
    if let Some(value) = recipient {
        email.insert("to".to_string(), json!(value));
    }
    if let Some(email_arg) = email_arg {
        for field in ["subject", "customMessage", "from"] {
            if let Some(value) = resolved_string_field(&email_arg, field) {
                email.insert(field.to_string(), json!(value));
            }
        }
        let bcc = list_string_field(&email_arg, "bcc");
        if !bcc.is_empty() {
            email.insert("bcc".to_string(), json!(bcc));
        }
    }

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "templateName".to_string(),
        json!(resolved_string_field(args, "templateName")
            .unwrap_or_else(|| "DRAFT_ORDER_INVOICE".to_string())),
    );
    if let Some(currency) = resolved_string_field(args, "presentmentCurrencyCode") {
        metadata.insert("presentmentCurrencyCode".to_string(), json!(currency));
    }
    metadata.insert("email".to_string(), Value::Object(email));
    Value::Object(metadata)
}

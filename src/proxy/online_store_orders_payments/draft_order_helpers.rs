use super::*;

pub(in crate::proxy) fn normalize_draft_order_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

pub(in crate::proxy) fn is_valid_draft_order_invoice_template(template: &str) -> bool {
    template.starts_with("DRAFT_ORDER_") && template != "NOT_A_REAL_TEMPLATE"
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
        let bcc = resolved_string_list_field_unsorted(&email_arg, "bcc");
        if !bcc.is_empty() {
            email.insert("bcc".to_string(), json!(bcc));
        }
    }

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "templateName".to_string(),
        json!(resolved_string_arg(args, "templateName")
            .unwrap_or_else(|| "DRAFT_ORDER_INVOICE".to_string())),
    );
    if let Some(currency) = resolved_string_arg(args, "presentmentCurrencyCode") {
        metadata.insert("presentmentCurrencyCode".to_string(), json!(currency));
    }
    metadata.insert("email".to_string(), Value::Object(email));
    Value::Object(metadata)
}

pub(in crate::proxy) fn draft_order_invoice_line_item() -> Value {
    json!({
        "id": "gid://shopify/DraftOrderLineItem/2",
        "title": "Invoice error parity item",
        "name": "Invoice error parity item",
        "quantity": 1,
        "sku": Value::Null,
        "variantTitle": Value::Null,
        "custom": true,
        "requiresShipping": true,
        "taxable": true,
        "customAttributes": [],
        "appliedDiscount": Value::Null,
        "originalUnitPriceSet": money_set_pair("1.0", "CAD", "1.0", "CAD"),
        "originalTotalSet": money_set_pair("1.0", "CAD", "1.0", "CAD"),
        "discountedTotalSet": money_set_pair("1.0", "CAD", "1.0", "CAD"),
        "totalDiscountSet": money_set_pair("0.0", "CAD", "0.0", "CAD"),
        "variant": Value::Null
    })
}

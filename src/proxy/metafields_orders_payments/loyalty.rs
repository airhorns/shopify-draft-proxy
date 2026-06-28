use super::*;

pub(in crate::proxy) fn customer_loyalty_metafield(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let Some(ResolvedValue::List(metafields)) = input.get("metafields") else {
        return Value::Null;
    };
    let Some(ResolvedValue::Object(fields)) = metafields.first() else {
        return Value::Null;
    };
    json!({
        "id": "gid://shopify/Metafield/1?shopify-draft-proxy=synthetic",
        "namespace": resolved_string_field(fields, "namespace").unwrap_or_else(|| "custom".to_string()),
        "key": resolved_string_field(fields, "key").unwrap_or_else(|| "loyalty".to_string()),
        "type": resolved_string_field(fields, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
        "value": resolved_string_field(fields, "value").unwrap_or_default()
    })
}

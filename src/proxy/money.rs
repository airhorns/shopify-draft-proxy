use super::*;

pub(in crate::proxy) fn money_value(amount: &str, currency_code: &str) -> Value {
    json!({
        "amount": amount,
        "currencyCode": currency_code
    })
}

pub(in crate::proxy) fn money_set(amount: &str, currency_code: &str) -> Value {
    json!({
        "shopMoney": money_value(amount, currency_code)
    })
}

pub(in crate::proxy) fn money_set_pair(
    shop_amount: &str,
    shop_currency: &str,
    presentment_amount: &str,
    presentment_currency: &str,
) -> Value {
    json!({
        "shopMoney": money_value(shop_amount, shop_currency),
        "presentmentMoney": money_value(presentment_amount, presentment_currency)
    })
}

pub(in crate::proxy) fn money_bag_from_amount(
    amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    let amount = format_money_amount(amount);
    money_set_pair(&amount, shop_currency, &amount, presentment_currency)
}

pub(in crate::proxy) fn money_bag(amount: f64, currency_code: &str) -> Value {
    money_bag_from_amount(amount, currency_code, currency_code)
}

pub(in crate::proxy) fn money_amount(money_set: &Value, money_key: &str) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("amount"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::proxy) fn money_currency(money_set: &Value, money_key: &str) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("currencyCode"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::proxy) fn money_set_presentment_or_shop_amount(money_set: &Value) -> Option<String> {
    money_amount(money_set, "presentmentMoney").or_else(|| money_amount(money_set, "shopMoney"))
}

pub(in crate::proxy) fn money_set_presentment_or_shop_currency(
    money_set: &Value,
) -> Option<String> {
    money_currency(money_set, "presentmentMoney").or_else(|| money_currency(money_set, "shopMoney"))
}

pub(in crate::proxy) fn money_set_presentment_or_shop_amount_value(money_set: &Value) -> f64 {
    money_set_presentment_or_shop_amount(money_set)
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0)
}

pub(in crate::proxy) fn money_set_amount(value: &Value) -> Option<f64> {
    value["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| {
            value["amount"]
                .as_str()
                .and_then(|amount| amount.parse::<f64>().ok())
        })
}

pub(in crate::proxy) fn money_set_shop_currency(value: &Value) -> Option<String> {
    value["shopMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

pub(in crate::proxy) fn money_set_presentment_currency(value: &Value) -> Option<String> {
    value["presentmentMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

pub(in crate::proxy) fn sum_money_set(values: &[Value], key: &str) -> f64 {
    values
        .iter()
        .filter_map(|value| money_set_amount(&value[key]))
        .sum()
}

/// Normalizes a Shopify MoneyV2 amount string to Shopify's minimal-decimal
/// representation: strip trailing zeros after the decimal point but keep at
/// least one fractional digit ("57.00" -> "57.0", "18.50" -> "18.5",
/// "38.25" -> "38.25", "57" -> "57.0").
pub(in crate::proxy) fn normalize_money_amount(amount: &str) -> String {
    let trimmed = amount.trim();
    if trimmed.is_empty() {
        return "0.0".to_string();
    }
    if trimmed.contains('.') {
        let stripped = trimmed.trim_end_matches('0');
        let stripped = stripped.strip_suffix('.').unwrap_or(stripped);
        if stripped.contains('.') {
            stripped.to_string()
        } else {
            format!("{stripped}.0")
        }
    } else {
        format!("{trimmed}.0")
    }
}

pub(in crate::proxy) fn format_money_amount(amount: f64) -> String {
    let rounded = (amount * 100.0).round() / 100.0;
    normalize_money_amount(&format!("{rounded:.2}"))
}

pub(in crate::proxy) fn shopify_decimal_text(value: &str) -> String {
    let Ok(parsed) = value.parse::<f64>() else {
        return value.to_string();
    };
    let mut formatted = parsed.to_string();
    if !formatted.contains('.') {
        formatted.push_str(".0");
    }
    formatted
}

pub(in crate::proxy) fn resolved_decimal_text(value: Option<&ResolvedValue>) -> Option<String> {
    match value {
        Some(ResolvedValue::String(value)) => Some(shopify_decimal_text(value)),
        Some(ResolvedValue::Float(value)) => Some(shopify_decimal_text(&value.to_string())),
        Some(ResolvedValue::Int(value)) => Some(shopify_decimal_text(&value.to_string())),
        _ => None,
    }
}

pub(in crate::proxy) fn maybe_money_amount_string_from_resolved(
    value: Option<&ResolvedValue>,
) -> Option<String> {
    let raw = match value? {
        ResolvedValue::Int(value) => value.to_string(),
        ResolvedValue::Float(value) => value.to_string(),
        ResolvedValue::String(value) => value.clone(),
        _ => return None,
    };
    Some(normalize_money_amount(&raw))
}

pub(in crate::proxy) fn money_amount_string_from_resolved_or(
    value: Option<&ResolvedValue>,
    default_amount: &str,
) -> String {
    maybe_money_amount_string_from_resolved(value)
        .unwrap_or_else(|| normalize_money_amount(default_amount))
}

pub(in crate::proxy) fn money_amount_string_from_resolved(value: Option<&ResolvedValue>) -> String {
    money_amount_string_from_resolved_or(value, "100")
}

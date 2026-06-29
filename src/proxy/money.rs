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
    let formatted = format!("{rounded:.2}");
    let trimmed = formatted.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    }
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

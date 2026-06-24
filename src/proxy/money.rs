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

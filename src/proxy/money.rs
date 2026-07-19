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

pub(in crate::proxy) fn format_money_summary_amount(
    amount: &str,
    currency_code: Option<&str>,
    shop_money_format: Option<&str>,
) -> String {
    let parsed = amount.trim().parse::<f64>().unwrap_or(0.0).abs();
    let formatted_amount = format!("{parsed:.2}");
    format_money_summary_formatted_amount(&formatted_amount, currency_code, shop_money_format)
}

pub(in crate::proxy) fn format_money_summary_formatted_amount(
    formatted_amount: &str,
    currency_code: Option<&str>,
    shop_money_format: Option<&str>,
) -> String {
    if let Some(format) = shop_money_format {
        if let Some(rendered) = render_shop_money_format(format, formatted_amount) {
            return rendered;
        }
    }
    match currency_code {
        Some("USD") => format!("${formatted_amount}"),
        Some(code) if !code.is_empty() => format!("{formatted_amount} {code}"),
        _ => formatted_amount.to_string(),
    }
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

fn render_shop_money_format(format: &str, formatted_amount: &str) -> Option<String> {
    let no_decimals = rounded_no_decimals(formatted_amount);
    let comma_separator = amount_with_comma_separator(formatted_amount);
    let no_decimals_comma_separator = grouped_integer(&no_decimals, '.');
    let apostrophe_separator = amount_with_apostrophe_separator(formatted_amount);
    let replacements = [
        ("amount", formatted_amount.to_string()),
        ("amount_no_decimals", no_decimals),
        ("amount_with_comma_separator", comma_separator),
        (
            "amount_no_decimals_with_comma_separator",
            no_decimals_comma_separator,
        ),
        ("amount_with_apostrophe_separator", apostrophe_separator),
    ];

    let mut rendered = format.to_string();
    let mut changed = false;
    for (token, value) in replacements {
        for pattern in [format!("{{{{{token}}}}}"), format!("{{{{ {token} }}}}")] {
            if rendered.contains(&pattern) {
                rendered = rendered.replace(&pattern, &value);
                changed = true;
            }
        }
    }
    changed.then_some(rendered)
}

fn rounded_no_decimals(formatted_amount: &str) -> String {
    let parsed = formatted_amount.parse::<f64>().unwrap_or(0.0);
    format!("{:.0}", parsed.round())
}

fn amount_with_comma_separator(formatted_amount: &str) -> String {
    let (integer, fraction) = formatted_amount
        .split_once('.')
        .unwrap_or((formatted_amount, ""));
    let mut amount = grouped_integer(integer, '.');
    if !fraction.is_empty() {
        amount.push(',');
        amount.push_str(fraction);
    }
    amount
}

fn amount_with_apostrophe_separator(formatted_amount: &str) -> String {
    let (integer, fraction) = formatted_amount
        .split_once('.')
        .unwrap_or((formatted_amount, ""));
    let mut amount = grouped_integer(integer, '\'');
    if !fraction.is_empty() {
        amount.push('.');
        amount.push_str(fraction);
    }
    amount
}

fn grouped_integer(integer: &str, separator: char) -> String {
    let (sign, digits) = integer
        .strip_prefix('-')
        .map(|digits| ("-", digits))
        .unwrap_or(("", integer));
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped}")
}

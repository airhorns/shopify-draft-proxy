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

// Proleptic-Gregorian day arithmetic (Howard Hinnant's civil/days algorithms)
// for Shopify date fields that need civil-date offsets without a date library.
pub(in crate::proxy) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

pub(in crate::proxy) fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = (if days >= 0 { days } else { days - 146_096 }) / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        (if month <= 2 { year + 1 } else { year }) as i32,
        month as u32,
        day as u32,
    )
}

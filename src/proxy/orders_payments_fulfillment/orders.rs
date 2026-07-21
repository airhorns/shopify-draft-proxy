use super::*;

mod order_customer_paths;
mod order_edit;
use self::order_customer_paths::*;
use self::order_edit::*;
use crate::proxy::storefront::storefront_customer_email_key;

const ORDER_CREATE_INVENTORY_PREFLIGHT_QUERY: &str =
    include_str!("../../../config/parity-requests/orders/order-create-inventory-preflight.graphql");

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderCreateInventoryEffect {
    inventory_item_id: String,
    location_id: String,
    quantity: i64,
}

fn order_edit_error_payload(errors: Vec<Value>) -> Value {
    oe_error_payload(errors)
}

fn order_edit_error_outcome(errors: Vec<Value>) -> Option<ResolverOutcome<Value>> {
    Some(ResolverOutcome::value(order_edit_error_payload(errors)))
}

pub(in crate::proxy) fn order_create_inventory_behaviour(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    resolved_object_field(arguments, "options")
        .and_then(|options| resolved_string_field(&options, "inventoryBehaviour"))
        .unwrap_or_else(|| "DECREMENT_IGNORING_POLICY".to_string())
}

fn order_create_unresolved_variant_payload() -> Value {
    json!({
        "order": Value::Null,
        "userErrors": [
            {
                "field": ["order"],
                "message": "Order Line items is invalid"
            },
            {
                "field": ["order", "lineItems"],
                "message": "Line items Name can't be blank"
            },
            {
                "field": ["order", "lineItems"],
                "message": "Line items Title can't be blank"
            }
        ]
    })
}

fn order_create_input_needs_shop_currency_default(
    order_input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if resolved_string_field(order_input, "currency")
        .or_else(|| resolved_string_field(order_input, "currencyCode"))
        .is_some()
    {
        return false;
    }
    let line_has_money_currency = resolved_object_list_field(order_input, "lineItems")
        .into_iter()
        .any(|line_item| {
            resolved_object_field(&line_item, "priceSet")
                .or_else(|| resolved_object_field(&line_item, "originalUnitPriceSet"))
                .and_then(|price_set| input_money_currency(&price_set))
                .is_some()
        });
    if line_has_money_currency {
        return false;
    }
    !resolved_object_list_field(order_input, "shippingLines")
        .into_iter()
        .any(|shipping_line| {
            resolved_object_field(&shipping_line, "priceSet")
                .and_then(|price_set| input_money_currency(&price_set))
                .is_some()
        })
}

pub(in crate::proxy) fn order_lifecycle_input_id(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_object_field(arguments, "input").and_then(|input| resolved_string_field(&input, "id"))
}

pub(in crate::proxy) fn normalize_order_lifecycle_defaults(order: &mut Value) {
    if order.get("closed").is_none() {
        order["closed"] = json!(false);
    }
    if order.get("closedAt").is_none() {
        order["closedAt"] = Value::Null;
    }
    if order.get("updatedAt").is_none() {
        order["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
    }
    if order.get("cancelledAt").is_none() {
        order["cancelledAt"] = Value::Null;
    }
    if order.get("cancelReason").is_none() {
        order["cancelReason"] = Value::Null;
    }
    if order.get("paymentGatewayNames").is_none() {
        order["paymentGatewayNames"] = json!([]);
    }
    if order.get("transactions").is_none() {
        order["transactions"] = json!([]);
    }
    if order.get("customer").is_none() {
        order["customer"] = Value::Null;
    }
    if order.get("displayFinancialStatus").is_none() {
        order["displayFinancialStatus"] = Value::Null;
    }
}

pub(in crate::proxy) fn order_money_set_with_presentment_fallback(
    money_set: &Value,
    order: &Value,
    shop_currency_code: &str,
) -> Value {
    let shop_amount = money_amount(money_set, "shopMoney").unwrap_or_else(|| "0.0".to_string());
    let shop_currency = money_currency(money_set, "shopMoney")
        .or_else(|| order["currencyCode"].as_str().map(ToString::to_string))
        .unwrap_or_else(|| shop_currency_code.to_string());
    let presentment_amount =
        money_set_presentment_or_shop_amount(money_set).unwrap_or_else(|| shop_amount.clone());
    let presentment_currency = money_currency(money_set, "presentmentMoney")
        .or_else(|| {
            order["presentmentCurrencyCode"]
                .as_str()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair(
        &shop_amount,
        &shop_currency,
        &presentment_amount,
        &presentment_currency,
    )
}

pub(in crate::proxy) fn order_money_amount_value(money_set: &Value) -> f64 {
    money_set_presentment_or_shop_amount_value(money_set)
}

pub(in crate::proxy) fn add_order_money_sets(
    left: &Value,
    right: &Value,
    order: &Value,
    shop_currency_code: &str,
) -> Value {
    let left = order_money_set_with_presentment_fallback(left, order, shop_currency_code);
    let right = order_money_set_with_presentment_fallback(right, order, shop_currency_code);
    let left_shop = money_amount(&left, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let right_shop = money_amount(&right, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let left_presentment = money_amount(&left, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(left_shop);
    let right_presentment = money_amount(&right, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(right_shop);
    let shop_currency = money_currency(&right, "shopMoney")
        .or_else(|| money_currency(&left, "shopMoney"))
        .unwrap_or_else(|| shop_currency_code.to_string());
    let presentment_currency = money_currency(&right, "presentmentMoney")
        .or_else(|| money_currency(&left, "presentmentMoney"))
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair(
        &format_money_amount(left_shop + right_shop),
        &shop_currency,
        &format_money_amount(left_presentment + right_presentment),
        &presentment_currency,
    )
}

pub(in crate::proxy) fn zero_order_money_set_like(
    money_set: &Value,
    order: &Value,
    shop_currency_code: &str,
) -> Value {
    let shop_currency = money_currency(money_set, "shopMoney")
        .or_else(|| order["currencyCode"].as_str().map(ToString::to_string))
        .unwrap_or_else(|| shop_currency_code.to_string());
    let presentment_currency = money_currency(money_set, "presentmentMoney")
        .or_else(|| {
            order["presentmentCurrencyCode"]
                .as_str()
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| shop_currency.clone());
    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency)
}

pub(in crate::proxy) fn order_customer_id(order: &Value) -> Option<String> {
    order["customer"]["id"].as_str().map(ToString::to_string)
}

pub(in crate::proxy) fn order_mark_as_paid_cannot_mark_error() -> Value {
    user_error_omit_code(json!(["id"]), "Order cannot be marked as paid.", None)
}

pub(in crate::proxy) fn order_mark_as_paid_not_found_error() -> Value {
    user_error_omit_code(json!(["id"]), "Order does not exist", None)
}

pub(in crate::proxy) fn order_read_selects_order_edit_existing_fields(
    requested_field_paths: &BTreeSet<Vec<String>>,
) -> bool {
    requested_field_paths.iter().any(|path| {
        path.first().is_some_and(|field| {
            matches!(
                field.as_str(),
                "merchantEditable" | "merchantEditableErrors" | "currentSubtotalLineItemsQuantity"
            )
        })
    })
}

/// Normalize an order name for comparison (`#1331` and `1331` are equivalent in
/// Shopify's `name:` search term), lower-cased so matching is case-insensitive.
pub(in crate::proxy) fn normalize_order_name(name: &str) -> String {
    name.trim().trim_start_matches('#').to_ascii_lowercase()
}

/// Evaluate one `key:value` search term against an order projection. Returns
/// `None` for terms we do not model so the staged connection engine can make
/// unsupported predicate handling explicit instead of silently keeping rows.
pub(in crate::proxy) fn order_matches_term(order: &Value, key: &str, value: &str) -> Option<bool> {
    let value = value.trim();
    match key {
        "id" => Some(order_matches_id(order, value)),
        "tag" => {
            let want = value.trim_matches('"').trim_matches('\'');
            Some(
                order
                    .get("tags")
                    .and_then(Value::as_array)
                    .is_some_and(|tags| {
                        tags.iter()
                            .filter_map(Value::as_str)
                            .any(|tag| tag.eq_ignore_ascii_case(want))
                    }),
            )
        }
        "name" => {
            let want = normalize_order_name(value);
            Some(
                order
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| normalize_order_name(name) == want),
            )
        }
        "email" => {
            let want = value.to_ascii_lowercase();
            Some(
                order
                    .get("email")
                    .and_then(Value::as_str)
                    .is_some_and(|email| email.to_ascii_lowercase() == want),
            )
        }
        "financial_status" => Some(
            order
                .get("displayFinancialStatus")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case(value)),
        ),
        "fulfillment_status" => Some(
            order
                .get("displayFulfillmentStatus")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case(value)),
        ),
        "status" => Some(order_matches_status(order, value)),
        "created_at" => Some(order_matches_datetime_comparator(
            order.get("createdAt").and_then(Value::as_str),
            value,
        )),
        "updated_at" => Some(order_matches_datetime_comparator(
            order.get("updatedAt").and_then(Value::as_str),
            value,
        )),
        "processed_at" => Some(order_matches_datetime_comparator(
            order.get("processedAt").and_then(Value::as_str),
            value,
        )),
        _ => None,
    }
}

fn order_matches_id(order: &Value, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    order
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| resource_id_tail(id) == value || resource_id_path_tail(id) == value)
}

fn order_matches_status(order: &Value, value: &str) -> bool {
    let cancelled = order
        .get("cancelledAt")
        .is_some_and(|cancelled_at| !cancelled_at.is_null())
        || order
            .get("cancelReason")
            .is_some_and(|cancel_reason| !cancel_reason.is_null());
    let closed = order
        .get("closed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || order
            .get("closedAt")
            .is_some_and(|closed_at| !closed_at.is_null());
    match value.to_ascii_lowercase().as_str() {
        "any" => true,
        "cancelled" | "canceled" => cancelled,
        "closed" => closed && !cancelled,
        "open" => !closed && !cancelled,
        _ => false,
    }
}

fn order_matches_datetime_comparator(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = search_comparator(query_value);
    if expected.is_empty() {
        return false;
    }
    let actual = search_datetime_value(actual, expected);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

fn order_matches_free_text(order: &Value, value: &str) -> bool {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return true;
    }
    order_matches_id(order, value)
        || order_search_string_matches(order.get("name").and_then(Value::as_str), value)
        || order_search_string_matches(order.get("email").and_then(Value::as_str), value)
        || order
            .get("tags")
            .and_then(Value::as_array)
            .is_some_and(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .any(|tag| order_search_string_matches(Some(tag), value))
            })
}

fn order_search_string_matches(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let actual = actual.to_ascii_lowercase();
    let query_value = query_value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if query_value.is_empty() {
        return true;
    }
    if let Some(prefix) = query_value.strip_suffix('*') {
        return actual
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&query_value)
}

pub(in crate::proxy) fn order_search_decision(
    order: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    for term in query.split_whitespace() {
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        if let Some((key, value)) = term.split_once(':') {
            match order_matches_term(order, key, value) {
                Some(true) => {}
                Some(false) => return StagedSearchDecision::NoMatch,
                None => return StagedSearchDecision::Unsupported,
            }
        } else if !order_matches_free_text(order, term) {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

fn order_matches_count_query(order: &Value, query: Option<&str>) -> bool {
    matches!(
        order_search_decision(order, query),
        StagedSearchDecision::Match
    )
}

fn order_count_baseline_key(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    let query = resolved_string_field(arguments, "query").unwrap_or_default();
    let limit = match arguments.get("limit") {
        None | Some(ResolvedValue::Int(10_000)) => "default".to_string(),
        Some(ResolvedValue::Int(limit)) => limit.to_string(),
        Some(ResolvedValue::Null) => "null".to_string(),
        Some(_) => "other".to_string(),
    };
    format!("query:{query}\nlimit:{limit}")
}

fn order_gid_tail_sort_value(order: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(order.get("id").and_then(Value::as_str))
}

fn order_string_sort_value(order: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        order
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn order_money_sort_value(order: &Value, field: &str) -> StagedSortValue {
    money_set_amount(&order[field])
        .map(|amount| StagedSortValue::I64((amount * 1_000_000.0).round() as i64))
        .unwrap_or(StagedSortValue::Null)
}

fn order_customer_name_sort_value(order: &Value) -> StagedSortValue {
    let value = order
        .get("customer")
        .and_then(|customer| customer.get("displayName"))
        .and_then(Value::as_str)
        .or_else(|| {
            order
                .get("customer")
                .and_then(|customer| customer.get("email"))
                .and_then(Value::as_str)
        })
        .or_else(|| order.get("email").and_then(Value::as_str))
        .unwrap_or_default();
    StagedSortValue::String(value.to_ascii_lowercase())
}

fn order_destination_sort_value(order: &Value) -> StagedSortValue {
    let Some(address) = order.get("shippingAddress").and_then(Value::as_object) else {
        return StagedSortValue::Null;
    };
    let value = [
        "countryCodeV2",
        "country",
        "provinceCode",
        "province",
        "city",
        "zip",
        "lastName",
    ]
    .iter()
    .filter_map(|field| address.get(*field).and_then(Value::as_str))
    .collect::<Vec<_>>()
    .join("|");
    StagedSortValue::String(value.to_ascii_lowercase())
}

fn order_number_sort_value(order: &Value) -> StagedSortValue {
    order
        .get("name")
        .and_then(Value::as_str)
        .map(|name| name.trim_start_matches('#'))
        .and_then(|value| value.parse::<i64>().ok())
        .map(StagedSortValue::I64)
        .unwrap_or_else(|| order_string_sort_value(order, "name"))
}

fn order_total_items_quantity_sort_value(order: &Value) -> StagedSortValue {
    let total = order
        .get("lineItems")
        .and_then(|line_items| line_items.get("nodes"))
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|line| line.get("quantity").and_then(Value::as_i64))
                .sum::<i64>()
        })
        .unwrap_or_default();
    StagedSortValue::I64(total)
}

/// Sort key for the orders connection: `(documented key, numeric id)`, both
/// ascending. ISO-8601 timestamps order lexicographically, so string comparison
/// matches chronological order. Callers reverse the sorted vector for
/// `reverse: true`.
pub(in crate::proxy) fn order_staged_sort_key(
    order: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let primary = match sort_key {
        None | Some("CREATED_AT") => order_string_sort_value(order, "createdAt"),
        Some("CURRENT_TOTAL_PRICE") => order_money_sort_value(order, "currentTotalPriceSet"),
        Some("CUSTOMER_NAME") => order_customer_name_sort_value(order),
        Some("DESTINATION") => order_destination_sort_value(order),
        Some("FINANCIAL_STATUS") => order_string_sort_value(order, "displayFinancialStatus"),
        Some("FULFILLMENT_STATUS") => order_string_sort_value(order, "displayFulfillmentStatus"),
        Some("ID") | Some("RELEVANCE") => order_gid_tail_sort_value(order),
        Some("ORDER_NUMBER") => order_number_sort_value(order),
        Some("PO_NUMBER") => order_string_sort_value(order, "poNumber"),
        Some("PROCESSED_AT") => order_string_sort_value(order, "processedAt"),
        Some("TOTAL_ITEMS_QUANTITY") => order_total_items_quantity_sort_value(order),
        Some("TOTAL_PRICE") => order_money_sort_value(order, "totalPriceSet"),
        Some("UPDATED_AT") => order_string_sort_value(order, "updatedAt"),
        Some(_) => order_gid_tail_sort_value(order),
    };
    vec![primary, order_gid_tail_sort_value(order)]
}

pub(in crate::proxy) fn resolved_money_amount(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<f64> {
    resolved_string_field(input, "amount")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "amount"))
}

pub(in crate::proxy) fn resolved_money_currency(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_string_field(input, "currencyCode")
}

pub(in crate::proxy) fn money_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    resolved_object_field(input, "shopMoney").or_else(|| {
        let amount = resolved_money_amount(input)?;
        let currency = resolved_money_currency(input)?;
        Some(BTreeMap::from([
            (
                "amount".to_string(),
                ResolvedValue::String(format_money_amount(amount)),
            ),
            ("currencyCode".to_string(), ResolvedValue::String(currency)),
        ]))
    })
}

pub(in crate::proxy) fn input_money_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    money_input(input).and_then(|money| resolved_money_amount(&money))
}

pub(in crate::proxy) fn input_money_currency(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    money_input(input).and_then(|money| resolved_money_currency(&money))
}

pub(in crate::proxy) fn order_create_address(
    input: Option<BTreeMap<String, ResolvedValue>>,
) -> Value {
    let Some(input) = input else {
        return Value::Null;
    };
    json!({
        "firstName": resolved_string_field(&input, "firstName").unwrap_or_default(),
        "lastName": resolved_string_field(&input, "lastName").unwrap_or_default(),
        "address1": resolved_string_field(&input, "address1").unwrap_or_default(),
        "address2": resolved_string_field(&input, "address2"),
        "company": resolved_string_field(&input, "company"),
        "city": resolved_string_field(&input, "city").unwrap_or_default(),
        "province": resolved_string_field(&input, "province"),
        "provinceCode": resolved_string_field(&input, "provinceCode").unwrap_or_default(),
        "country": resolved_string_field(&input, "country"),
        "countryCodeV2": resolved_string_field(&input, "countryCode")
            .or_else(|| resolved_string_field(&input, "countryCodeV2"))
            .unwrap_or_default(),
        "zip": resolved_string_field(&input, "zip").unwrap_or_default(),
        "phone": resolved_string_field(&input, "phone")
    })
}

pub(in crate::proxy) fn order_mutation_timestamp(ordinal: u64) -> String {
    format!("2024-01-01T00:00:{:02}.000Z", ordinal % 60)
}

pub(in crate::proxy) fn order_update_has_mutable_fields(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    [
        "note",
        "tags",
        "customAttributes",
        "email",
        "phone",
        "poNumber",
        "shippingAddress",
        "metafields",
        "localizedFields",
        "localizationExtensions",
    ]
    .iter()
    .any(|field| input.contains_key(*field))
}

pub(in crate::proxy) fn order_update_phone_is_valid(phone: &str) -> bool {
    let digits = phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    phone.starts_with('+')
        && digits >= 8
        && phone
            .chars()
            .all(|character| character == '+' || character.is_ascii_digit())
}

const CANADIAN_PROVINCE_CODES: &[&str] = &[
    "AB", "BC", "MB", "NB", "NL", "NS", "NT", "NU", "ON", "PE", "QC", "SK", "YT",
];
const UNITED_STATES_PROVINCE_CODES: &[&str] = &[
    "AK", "AL", "AR", "AS", "AZ", "CA", "CO", "CT", "DC", "DE", "FL", "FM", "GA", "GU", "HI", "IA",
    "ID", "IL", "IN", "KS", "KY", "LA", "MA", "MD", "ME", "MH", "MI", "MN", "MO", "MP", "MS", "MT",
    "NC", "ND", "NE", "NH", "NJ", "NM", "NV", "NY", "OH", "OK", "OR", "PA", "PR", "PW", "RI", "SC",
    "SD", "TN", "TX", "UT", "VA", "VI", "VT", "WA", "WI", "WV", "WY",
];
const AUSTRALIAN_PROVINCE_CODES: &[&str] = &["ACT", "NSW", "NT", "QLD", "SA", "TAS", "VIC", "WA"];

fn country_province_rule(
    country_code: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match country_code {
        "AU" => Some(("State", "Australia", AUSTRALIAN_PROVINCE_CODES)),
        "CA" => Some(("Province", "Canada", CANADIAN_PROVINCE_CODES)),
        "US" => Some(("State", "United States", UNITED_STATES_PROVINCE_CODES)),
        _ => None,
    }
}

fn order_update_invalid_province_message(
    country_code: &str,
    province_code: &str,
) -> Option<String> {
    if province_code.is_empty() {
        return None;
    }
    let (label, country_name, valid_codes) = country_province_rule(country_code)?;
    (!valid_codes.contains(&province_code)).then(|| {
        format!(
            "{label} is not a valid {} in {country_name}",
            label.to_ascii_lowercase()
        )
    })
}

pub(in crate::proxy) fn order_update_shipping_address_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_non_blank_string_field(input, "lastName").is_none() {
        errors.push(user_error_omit_code(
            ["shippingAddress", "lastName"],
            "Enter a last name",
            None,
        ));
    }
    if resolved_non_blank_string_field(input, "zip").is_none() {
        errors.push(user_error_omit_code(
            ["shippingAddress", "zip"],
            "Enter a ZIP code",
            None,
        ));
    }
    let country_code = resolved_string_field(input, "countryCode")
        .or_else(|| resolved_string_field(input, "countryCodeV2"))
        .unwrap_or_default();
    let province_code = resolved_string_field(input, "provinceCode").unwrap_or_default();
    if let Some(message) = order_update_invalid_province_message(&country_code, &province_code) {
        errors.push(user_error_omit_code(
            ["shippingAddress", "province"],
            &message,
            None,
        ));
    }
    errors
}

pub(in crate::proxy) fn order_update_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if !order_update_has_mutable_fields(input) {
        errors.push(user_error_omit_code(
            Value::Null,
            "No valid update parameters have been provided",
            None,
        ));
    }
    if let Some(phone) = resolved_string_field(input, "phone") {
        if !order_update_phone_is_valid(&phone) {
            errors.push(user_error_omit_code(["phone"], "Phone is invalid", None));
        }
    }
    if let Some(shipping_address) = resolved_object_field(input, "shippingAddress") {
        errors.extend(order_update_shipping_address_errors(&shipping_address));
    }
    errors
}

fn order_update_staged_overlay(order: &Value, input: &BTreeMap<String, ResolvedValue>) -> Value {
    let mut overlay = serde_json::Map::new();
    if let Some(id) = order.get("id") {
        overlay.insert("id".to_string(), id.clone());
    }
    for field in [
        "note",
        "tags",
        "customAttributes",
        "email",
        "phone",
        "poNumber",
        "shippingAddress",
    ] {
        if input.contains_key(field) {
            overlay.insert(field.to_string(), order[field].clone());
        }
    }
    if input.contains_key("metafields") {
        overlay.insert("metafield".to_string(), order["metafield"].clone());
        overlay.insert("metafields".to_string(), order["metafields"].clone());
    }
    if input.contains_key("localizedFields") || input.contains_key("localizationExtensions") {
        overlay.insert(
            "localizedFields".to_string(),
            order["localizedFields"].clone(),
        );
        overlay.insert(
            "localizationExtensions".to_string(),
            order["localizationExtensions"].clone(),
        );
    }
    overlay.insert("updatedAt".to_string(), order["updatedAt"].clone());
    Value::Object(overlay)
}

fn order_hydration_profile_for_requested_fields(
    requested_field_paths: &BTreeSet<Vec<String>>,
) -> OrderHydrationProfile {
    if requested_field_paths
        .iter()
        .any(|path| path.iter().any(|field| field == "lineItems"))
    {
        return OrderHydrationProfile::CompleteLineItems;
    }
    const BROAD_SUMMARY_FIELDS: &[&str] = &[
        "billingAddress",
        "currencyCode",
        "presentmentCurrencyCode",
        "displayFinancialStatus",
        "displayFulfillmentStatus",
        "currentTotalPriceSet",
        "totalPriceSet",
        "totalTaxSet",
        "totalDiscountsSet",
        "discountCodes",
        "localizedFields",
        "localizationExtensions",
    ];
    if requested_field_paths.iter().any(|path| {
        path.iter()
            .any(|field| BROAD_SUMMARY_FIELDS.contains(&field.as_str()))
    }) {
        OrderHydrationProfile::BroadSummary
    } else {
        OrderHydrationProfile::Summary
    }
}

pub(in crate::proxy) fn order_update_metafields(
    order_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: &[Value],
) -> Vec<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
        .filter_map(|(index, metafield)| {
            let namespace = resolved_string_field(&metafield, "namespace")?;
            let key = resolved_string_field(&metafield, "key")?;
            // Reuse the backing metafield id when the order already carries a
            // metafield at this namespace/key (an update, not a create), so the
            // identifier stays stable across the mutation and downstream reads.
            let metafield_id = existing
                .iter()
                .find(|m| {
                    m["namespace"].as_str() == Some(namespace.as_str())
                        && m["key"].as_str() == Some(key.as_str())
                })
                .and_then(|m| m["id"].as_str().map(str::to_string))
                .unwrap_or_else(|| {
                    shopify_gid("Metafield", format!("{}{}", resource_id_tail(order_id), index + 1))
                });
            Some(json!({
                "id": metafield_id,
                "namespace": namespace,
                "key": key,
                "type": resolved_string_field(&metafield, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
                "value": resolved_string_field(&metafield, "value").unwrap_or_default()
            }))
        })
        .collect()
}

pub(in crate::proxy) fn order_create_custom_attributes(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .filter_map(|attribute| {
            let key = resolved_string_field(&attribute, "key")
                .or_else(|| resolved_string_field(&attribute, "name"))?;
            let value = resolved_string_field(&attribute, "value").unwrap_or_default();
            Some(json!({ "key": key, "value": value }))
        })
        .collect()
}

pub(in crate::proxy) fn order_create_tax_lines(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
    currency_code: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .map(|tax_line| {
            let price = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_amount(&price))
                .unwrap_or(0.0);
            let price_currency = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_currency(&price))
                .unwrap_or_else(|| currency_code.to_string());
            json!({
                "title": resolved_string_field(&tax_line, "title").unwrap_or_default(),
                "rate": resolved_number_field(&tax_line, "rate").unwrap_or(0.0),
                "priceSet": money_bag(price, &price_currency)
            })
        })
        .collect()
}

pub(in crate::proxy) fn order_create_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    currency_code: &str,
) -> (f64, Vec<String>) {
    let Some(discount_code) = resolved_object_field(input, "discountCode") else {
        return (0.0, Vec::new());
    };
    let Some(fixed) = resolved_object_field(&discount_code, "itemFixedDiscountCode")
        .or_else(|| resolved_object_field(&discount_code, "fixedAmountDiscountCode"))
    else {
        return (0.0, Vec::new());
    };
    let code = resolved_string_field(&fixed, "code").unwrap_or_default();
    let amount = resolved_object_field(&fixed, "amountSet")
        .and_then(|amount| input_money_amount(&amount))
        .or_else(|| {
            resolved_object_field(&fixed, "amount").and_then(|amount| input_money_amount(&amount))
        })
        .unwrap_or(0.0);
    let codes = if code.is_empty() {
        Vec::new()
    } else {
        vec![code]
    };
    let _ = currency_code;
    (amount, codes)
}

pub(in crate::proxy) fn order_create_line_item_record(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    currency_code: &str,
    presentment_currency_code: &str,
) -> (Value, f64, f64) {
    let quantity = resolved_int_field(input, "quantity").unwrap_or(1).max(0);
    let price_input = resolved_object_field(input, "priceSet")
        .or_else(|| resolved_object_field(input, "originalUnitPriceSet"))
        .unwrap_or_default();
    let unit_amount = input_money_amount(&price_input).unwrap_or(0.0);
    let line_currency =
        input_money_currency(&price_input).unwrap_or_else(|| currency_code.to_string());
    let presentment_input = resolved_object_field(&price_input, "presentmentMoney");
    let presentment_amount = presentment_input
        .as_ref()
        .and_then(resolved_money_amount)
        .unwrap_or(unit_amount);
    let presentment_currency = presentment_input
        .as_ref()
        .and_then(resolved_money_currency)
        .unwrap_or_else(|| presentment_currency_code.to_string());
    let tax_lines = order_create_tax_lines(input, "taxLines", currency_code);
    let tax_total = sum_money_set(&tax_lines, "priceSet");
    let custom_attributes = order_create_custom_attributes(input, "properties");
    let product_id = resolved_string_field(input, "productId");
    let variant_id = resolved_string_field(input, "variantId");
    let variant = variant_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let product = product_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let selling_plan = resolved_string_field(input, "sellingPlanId")
        .or_else(|| resolved_string_field(input, "sellingPlanName"))
        .map(|name| json!({ "name": name }))
        .unwrap_or(Value::Null);
    let fulfillment_service = resolved_string_field(input, "fulfillmentService")
        .map(|handle| json!({ "handle": handle }))
        .unwrap_or(Value::Null);
    let unit_amount_text = format_money_amount(unit_amount);
    let presentment_amount_text = format_money_amount(presentment_amount);
    let line = json!({
        "id": shopify_gid("LineItem", index + 1),
        "title": resolved_string_field(input, "title").unwrap_or_else(|| "Custom Item".to_string()),
        "quantity": quantity,
        "currentQuantity": quantity,
        "fulfillableQuantity": quantity,
        "refundableQuantity": quantity,
        "sku": resolved_string_field(input, "sku").unwrap_or_default(),
        "variantTitle": resolved_string_field(input, "variantTitle"),
        "variantId": variant_id,
        "variant": variant,
        "productId": product_id,
        "product": product,
        "sellingPlan": selling_plan,
        "customAttributes": custom_attributes,
        "requiresShipping": resolved_bool_field(input, "requiresShipping").unwrap_or(true),
        "taxable": resolved_bool_field(input, "taxable").unwrap_or(true),
        "isGiftCard": resolved_bool_field(input, "giftCard").unwrap_or(false),
        "vendor": resolved_string_field(input, "vendor"),
        "fulfillmentService": fulfillment_service,
        "fulfillmentStatus": "unfulfilled",
        "discountAllocations": [],
        "originalUnitPriceSet": money_set_pair(
            &unit_amount_text,
            &line_currency,
            &presentment_amount_text,
            &presentment_currency
        ),
        "priceSet": money_set_pair(
            &unit_amount_text,
            currency_code,
            &presentment_amount_text,
            presentment_currency_code
        ),
        "taxLines": tax_lines
    });
    (line, unit_amount * quantity as f64, tax_total)
}

pub(in crate::proxy) fn order_fulfillment_order_line_item_record(
    line_item: &Value,
    index: usize,
) -> Value {
    let order_line_item_id = line_item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id_tail = if order_line_item_id.is_empty() {
        (index + 1).to_string()
    } else {
        resource_id_tail(order_line_item_id).to_string()
    };
    let quantity = line_item
        .get("quantity")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(0);
    json!({
        "id": shopify_gid("FulfillmentOrderLineItem", id_tail),
        "totalQuantity": quantity,
        "remainingQuantity": quantity,
        "lineItem": line_item
    })
}

pub(in crate::proxy) fn order_default_fulfillment_order(
    order_id: &str,
    line_items: &[Value],
) -> Value {
    let tail = resource_id_tail(order_id);
    let fulfillment_order_line_items = line_items
        .iter()
        .enumerate()
        .map(|(index, line_item)| order_fulfillment_order_line_item_record(line_item, index))
        .collect::<Vec<_>>();
    json!({
        "id": shopify_gid("FulfillmentOrder", tail),
        "status": "OPEN",
        "requestStatus": "UNSUBMITTED",
        "fulfillAt": Value::Null,
        "fulfillBy": Value::Null,
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "assignedLocation": {
            "name": "",
            "location": Value::Null
        },
        "fulfillmentHolds": [],
        "supportedActions": [],
        "lineItems": order_connection(fulfillment_order_line_items)
    })
}

pub(in crate::proxy) fn order_create_transaction_record(
    input: &BTreeMap<String, ResolvedValue>,
    id: String,
    currency_code: &str,
) -> Value {
    let amount_input = resolved_object_field(input, "amountSet").unwrap_or_default();
    let amount = input_money_amount(&amount_input).unwrap_or(0.0);
    let currency = input_money_currency(&amount_input).unwrap_or_else(|| currency_code.to_string());
    json!({
        "id": id,
        "kind": resolved_string_field(input, "kind").unwrap_or_else(|| "SALE".to_string()),
        "status": resolved_string_field(input, "status").unwrap_or_else(|| "SUCCESS".to_string()),
        "gateway": resolved_string_field(input, "gateway").unwrap_or_else(|| "manual".to_string()),
        "paymentId": Value::Null,
        "paymentReferenceId": Value::Null,
        "parentTransaction": Value::Null,
        "amountSet": money_bag(amount, &currency)
    })
}

pub(in crate::proxy) fn order_create_financial_status(
    input: &BTreeMap<String, ResolvedValue>,
    transactions: &[Value],
    total: f64,
) -> Option<String> {
    if let Some(status) = resolved_string_field(input, "financialStatus") {
        return Some(status);
    }
    if transactions.is_empty() {
        return None;
    }
    if transactions
        .iter()
        .any(|transaction| transaction["kind"] == "AUTHORIZATION")
    {
        return Some("AUTHORIZED".to_string());
    }
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| money_set_amount(&transaction["amountSet"]))
        .sum::<f64>();
    Some(if received <= 0.0 || received + 0.005 >= total {
        "PAID".to_string()
    } else {
        "PARTIALLY_PAID".to_string()
    })
}

pub(in crate::proxy) fn order_create_payment_fields(
    order: &mut Value,
    transactions: &[Value],
    total: f64,
    currency_code: &str,
    presentment_currency_code: &str,
) {
    let authorization = transactions
        .iter()
        .find(|transaction| transaction["kind"] == "AUTHORIZATION");
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| money_set_amount(&transaction["amountSet"]))
        .sum::<f64>();
    let capturable = authorization
        .and_then(|transaction| money_set_amount(&transaction["amountSet"]))
        .unwrap_or(0.0);
    let outstanding = if authorization.is_some() {
        0.0
    } else {
        (total - received).max(0.0)
    };
    order["capturable"] = json!(capturable > 0.0);
    order["totalCapturable"] = json!(format_money_amount(capturable));
    order["totalCapturableSet"] =
        money_bag_from_amount(capturable, currency_code, presentment_currency_code);
    order["totalOutstandingSet"] =
        money_bag_from_amount(outstanding, currency_code, presentment_currency_code);
    order["totalReceivedSet"] =
        money_bag_from_amount(received, currency_code, presentment_currency_code);
    order["netPaymentSet"] =
        money_bag_from_amount(received, currency_code, presentment_currency_code);
    order["paymentGatewayNames"] = Value::Array(
        transactions
            .iter()
            .filter_map(|transaction| transaction["gateway"].as_str())
            .map(|gateway| json!(gateway))
            .collect(),
    );
}

pub(in crate::proxy) fn order_create_validation_error(
    order: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if resolved_string_field(order, "processedAt")
        .as_deref()
        .is_some_and(|value| value.starts_with("2099-"))
    {
        return Some(user_error(
            vec![json!("order"), json!("processedAt")],
            "Processed at is invalid",
            Some("PROCESSED_AT_INVALID"),
        ));
    }
    if order.contains_key("customerId") && order.contains_key("customer") {
        return Some(user_error(
            vec![json!("order")],
            "Customer fields are redundant",
            Some("REDUNDANT_CUSTOMER_FIELDS"),
        ));
    }
    let line_items = resolved_object_list_field(order, "lineItems");
    if line_items.is_empty() {
        return Some(user_error(
            vec![json!("order"), json!("lineItems")],
            "Line items must have at least one line item",
            Some("INVALID"),
        ));
    }
    for (line_index, line_item) in line_items.iter().enumerate() {
        if let Some(service) = resolved_string_field(line_item, "fulfillmentService") {
            if service != "manual" && service != "gift_card" {
                return Some(user_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("fulfillmentService"),
                    ],
                    "Fulfillment service is invalid",
                    Some("FULFILLMENT_SERVICE_INVALID"),
                ));
            }
        }
        for (tax_index, tax_line) in resolved_object_list_field(line_item, "taxLines")
            .iter()
            .enumerate()
        {
            if resolved_number_field(tax_line, "rate").is_none() {
                return Some(user_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    Some("TAX_LINE_RATE_MISSING"),
                ));
            }
        }
    }
    for (shipping_index, shipping_line) in resolved_object_list_field(order, "shippingLines")
        .iter()
        .enumerate()
    {
        for (tax_index, tax_line) in resolved_object_list_field(shipping_line, "taxLines")
            .iter()
            .enumerate()
        {
            if resolved_number_field(tax_line, "rate").is_none() {
                return Some(user_error(
                    vec![
                        json!("order"),
                        json!("shippingLines"),
                        json!(shipping_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    Some("TAX_LINE_RATE_MISSING"),
                ));
            }
        }
    }
    None
}

pub(in crate::proxy) fn order_edit_order_is_not_editable(order: &Value) -> bool {
    if matches!(order["merchantEditable"].as_bool(), Some(false)) {
        return true;
    }
    if order["cancelledAt"].is_string() || order["cancelReason"].is_string() {
        return true;
    }
    matches!(
        order["displayFinancialStatus"].as_str(),
        Some("REFUNDED" | "VOIDED")
    )
}

pub(in crate::proxy) fn order_edit_order_is_closed(order: &Value) -> bool {
    order["closed"].as_bool().unwrap_or(false) || order["closedAt"].is_string()
}

pub(in crate::proxy) fn order_edit_commit_success_messages(
    order: &Value,
    notify_customer: bool,
    order_unarchived: bool,
) -> Value {
    let mut messages = vec![json!("Order updated")];
    if order_unarchived {
        messages.push(json!("Order unarchived"));
    }
    if notify_customer {
        let notify_message = if order_money_amount_value(&order["totalOutstandingSet"]) > 0.000_001
        {
            "Invoice sent"
        } else {
            "Notification sent"
        };
        messages.push(json!(notify_message));
    }
    Value::Array(messages)
}

pub(in crate::proxy) fn order_connection(nodes: Vec<Value>) -> Value {
    connection_json_with_boundary_cursors(nodes, |node| {
        node.get("id")
            .and_then(Value::as_str)
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_string)
    })
}

fn order_invoice_send_hydrate_query(requests_management_fields: bool) -> &'static str {
    if requests_management_fields {
        ORDER_LIFECYCLE_HYDRATE_QUERY
    } else {
        ORDER_INVOICE_SEND_EMAIL_HYDRATE_QUERY
    }
}

fn order_invoice_send_recipient(
    args: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<String> {
    resolved_object_field(args, "email")
        .and_then(|email| resolved_string_field(&email, "to"))
        .or_else(|| order["email"].as_str().map(str::to_string))
        .or_else(|| order["customer"]["email"].as_str().map(str::to_string))
        .filter(|recipient| !recipient.trim().is_empty())
}

fn order_invoice_send_metadata(
    args: &BTreeMap<String, ResolvedValue>,
    recipient: &str,
    staged_at: &str,
) -> Value {
    let email_arg = resolved_object_field(args, "email");
    let mut email = serde_json::Map::new();
    email.insert("to".to_string(), json!(recipient));
    if let Some(email_arg) = email_arg {
        for field in ["from", "subject", "customMessage"] {
            if let Some(value) = resolved_string_field(&email_arg, field) {
                email.insert(field.to_string(), json!(value));
            }
        }
        let bcc = list_string_field(&email_arg, "bcc");
        if !bcc.is_empty() {
            email.insert("bcc".to_string(), json!(bcc));
        }
    }
    json!({
        "email": Value::Object(email),
        "stagedAt": staged_at,
        "deliveryStatus": "STAGED_NO_DELIVERY",
        "delivered": false
    })
}

fn order_invoice_send_user_error(message: &str) -> Value {
    user_error_omit_code(
        Value::Null,
        message,
        Some("ORDER_INVOICE_SEND_UNSUCCESSFUL"),
    )
}

fn order_invoice_send_payload(order: Value, user_errors: Vec<Value>) -> Value {
    json!({ "order": order, "userErrors": user_errors })
}

pub(in crate::proxy) fn normalize_hydrated_order(order: &mut Value) {
    if order
        .get("fulfillments")
        .is_some_and(|fulfillments| fulfillments.is_null())
    {
        order["fulfillments"] = json!([]);
    }
    if let Some(nodes) = order
        .get("fulfillments")
        .and_then(|fulfillments| fulfillments.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
    {
        order["fulfillments"] = Value::Array(nodes);
    }
    if !order
        .get("fulfillments")
        .is_some_and(|fulfillments| fulfillments.is_array())
    {
        order["fulfillments"] = json!([]);
    }
    if !order
        .get("fulfillmentOrders")
        .and_then(|connection| connection.get("nodes"))
        .is_some_and(|nodes| nodes.is_array())
    {
        order["fulfillmentOrders"] = order_connection(Vec::new());
    }
}

impl DraftProxy {
    fn observe_live_hybrid_order_read(&mut self, request: &Request) {
        let response = self
            .execution_session
            .upstream_query_response
            .clone()
            .unwrap_or_else(|| (self.upstream_transport)(request.clone()));
        self.observe_order_read_response(request, &response);
    }

    pub(in crate::proxy) fn observe_order_read_response(
        &mut self,
        request: &Request,
        response: &Response,
    ) {
        if response.status >= 400 {
            return;
        }
        self.observe_order_read_data(request, &response.body["data"]);
    }

    pub(in crate::proxy) fn observe_order_read_data(&mut self, request: &Request, data: &Value) {
        let body = json!({ "data": data });
        if let Some(graphql_request) = parse_graphql_request_body(&request.body) {
            if let Some(fields) = root_fields(&graphql_request.query, &graphql_request.variables) {
                self.observe_order_count_baselines(&fields, &body);
                self.observe_singular_order_roots(&fields, &body);
            }
        }
        self.observe_order_value(&body);
    }

    fn observe_singular_order_roots(&mut self, fields: &[RootFieldSelection], body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for field in fields {
            if field.name != "order" {
                continue;
            }
            let Some(id) = resolved_string_field(&field.arguments, "id") else {
                continue;
            };
            if !is_shopify_gid_of_type(&id, "Order") {
                continue;
            }
            let Some(value) = data.get(&field.response_key) else {
                continue;
            };
            if !value.is_object() {
                continue;
            }
            let mut order = value.clone();
            if order.get("id").and_then(Value::as_str).is_none() {
                if let Some(object) = order.as_object_mut() {
                    object.insert("id".to_string(), json!(id));
                }
            }
            self.observe_order_value(&order);
        }
    }

    fn observe_order_count_baselines(&mut self, fields: &[RootFieldSelection], body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for field in fields {
            if field.name != "ordersCount" {
                continue;
            }
            let Some(count) = data.get(&field.response_key) else {
                continue;
            };
            if count.get("count").and_then(Value::as_u64).is_some() {
                self.store.observe_order_count_baseline(
                    order_count_baseline_key(&field.arguments),
                    count.clone(),
                );
            }
        }
    }

    fn observe_order_value(&mut self, value: &Value) {
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if is_shopify_gid_of_type(id, "Order") {
                let mut order = value.clone();
                normalize_hydrated_order(&mut order);
                self.store.observe_base_order(order);
                return;
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    self.observe_order_value(value);
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    self.observe_order_value(value);
                }
            }
            _ => {}
        }
    }

    pub(in crate::proxy) fn order_invoice_send_local_outcome(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
        requested_field_paths: &BTreeSet<Vec<String>>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let requests_management_fields = requested_field_paths.iter().any(|path| {
            path.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "closed"
                        | "closedAt"
                        | "cancelledAt"
                        | "cancelReason"
                        | "displayFinancialStatus"
                        | "paymentGatewayNames"
                        | "totalOutstandingSet"
                        | "currentTotalPriceSet"
                        | "transactions"
                )
            })
        });
        let (payload, staged_ids) =
            self.stage_order_invoice_send(request, arguments, requests_management_fields);
        if !staged_ids.is_empty() {
            self.record_staged_orders_log_entry(
                request,
                query,
                variables,
                "orderInvoiceSend",
                staged_ids,
            );
        }
        Some(ResolverOutcome::value(payload))
    }

    fn stage_order_invoice_send(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
        requests_management_fields: bool,
    ) -> (Value, Vec<String>) {
        if let Some(email) = resolved_object_field(arguments, "email") {
            if resolved_string_field(&email, "to")
                .is_some_and(|to| !shopify_email_is_valid(&to, EmailValidationMode::Basic))
            {
                return (
                    order_invoice_send_payload(
                        Value::Null,
                        vec![order_invoice_send_user_error("To is invalid")],
                    ),
                    Vec::new(),
                );
            }
        }

        let Some(order_id) = resolved_string_field(arguments, "id") else {
            return (
                order_invoice_send_payload(
                    Value::Null,
                    vec![order_invoice_send_user_error("Order does not exist")],
                ),
                Vec::new(),
            );
        };
        let Some(mut order) = self
            .store
            .staged
            .orders
            .get(&order_id)
            .cloned()
            .or_else(|| {
                self.hydrate_order_for_invoice_send(request, &order_id, requests_management_fields)
            })
        else {
            return (
                order_invoice_send_payload(
                    Value::Null,
                    vec![order_invoice_send_user_error("Order does not exist")],
                ),
                Vec::new(),
            );
        };

        normalize_order_lifecycle_defaults(&mut order);
        let Some(recipient) = order_invoice_send_recipient(arguments, &order) else {
            return (
                order_invoice_send_payload(
                    Value::Null,
                    vec![order_invoice_send_user_error(
                        "No recipient email address was provided",
                    )],
                ),
                Vec::new(),
            );
        };

        let staged_at = order_mutation_timestamp(self.mutation_log_ordinal() as u64);
        order["updatedAt"] = json!(staged_at.clone());
        order["__draftProxyInvoiceSend"] =
            order_invoice_send_metadata(arguments, &recipient, &staged_at);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        for orders in self.store.staged.customer_orders.values_mut() {
            for customer_order in orders {
                if customer_order["id"].as_str() == Some(order_id.as_str()) {
                    *customer_order = order.clone();
                }
            }
        }
        (
            order_invoice_send_payload(order, Vec::new()),
            vec![order_id],
        )
    }

    fn hydrate_order_for_invoice_send(
        &mut self,
        request: &Request,
        order_id: &str,
        requests_management_fields: bool,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": order_invoice_send_hydrate_query(requests_management_fields),
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": order_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let order = response.body["data"]["order"].clone();
        order.is_object().then_some(order)
    }

    pub(in crate::proxy) fn next_order_number(&mut self) -> u64 {
        let number = self
            .store
            .staged
            .orders
            .values()
            .filter_map(|order| {
                order
                    .get("orderNumber")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        order
                            .get("name")
                            .and_then(Value::as_str)
                            .and_then(|name| name.strip_prefix('#'))
                            .and_then(|suffix| suffix.parse::<u64>().ok())
                    })
            })
            .fold(
                self.store.staged.next_order_number.max(1),
                |next, number| next.max(number.saturating_add(1)),
            );
        self.store.staged.next_order_number = number.saturating_add(1);
        number
    }

    pub(in crate::proxy) fn next_order_transaction_id(&mut self) -> String {
        let number = self.store.staged.order_payment_next_transaction_id.max(3);
        self.store.staged.order_payment_next_transaction_id = number.saturating_add(1);
        shopify_gid("OrderTransaction", number)
    }

    fn hydrate_order_create_inventory_variants(
        &mut self,
        request: &Request,
        variant_ids: &[String],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid || variant_ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_CREATE_INVENTORY_PREFLIGHT_QUERY.trim_end(),
                "operationName": "OrdersOrderCreateInventoryPreflight",
                "variables": { "ids": variant_ids }
            }),
        );
        if !response_is_success(&response) {
            return;
        }
        let requested = variant_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for variant in response.body["data"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(variant_id) = variant.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !requested.contains(variant_id)
                || variant.get("__typename").and_then(Value::as_str) != Some("ProductVariant")
            {
                continue;
            }
            let Some(mut inventory_item) = variant.get("inventoryItem").cloned() else {
                continue;
            };
            let relationship_is_complete = inventory_item
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| is_shopify_gid_of_type(id, "InventoryItem"))
                && inventory_item
                    .get("inventoryLevels")
                    .and_then(|connection| connection.get("nodes"))
                    .is_some_and(Value::is_array);
            if !relationship_is_complete {
                continue;
            }
            inventory_item["variant"] = variant.clone();
            self.observe_inventory_item_node(&inventory_item);
        }
    }

    fn order_create_inventory_effects(
        &mut self,
        request: &Request,
        order_input: &BTreeMap<String, ResolvedValue>,
    ) -> Result<Vec<OrderCreateInventoryEffect>, Value> {
        let line_items = resolved_object_list_field(order_input, "lineItems");
        let mut seen_variant_ids = BTreeSet::new();
        let variant_ids = line_items
            .iter()
            .filter_map(|line_item| resolved_string_field(line_item, "variantId"))
            .filter(|variant_id| seen_variant_ids.insert(variant_id.clone()))
            .collect::<Vec<_>>();
        let variants_to_hydrate = variant_ids
            .iter()
            .filter(|variant_id| {
                if is_synthetic_gid(variant_id) {
                    return false;
                }
                self.store
                    .product_variant_by_id(variant_id)
                    .is_none_or(|variant| {
                        !is_shopify_gid_of_type(&variant.inventory_item.id, "InventoryItem")
                            || self
                                .inventory_levels_for_item(&variant.inventory_item.id)
                                .is_empty()
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        self.hydrate_order_create_inventory_variants(request, &variants_to_hydrate);

        if variant_ids
            .iter()
            .any(|variant_id| self.store.product_variant_by_id(variant_id).is_none())
        {
            return Err(order_create_unresolved_variant_payload());
        }

        let mut quantities_by_item = BTreeMap::<String, i64>::new();
        for line_item in line_items {
            let quantity = resolved_int_field(&line_item, "quantity").unwrap_or(1);
            if quantity <= 0 {
                continue;
            }
            let inventory_item_id =
                if let Some(variant_id) = resolved_string_field(&line_item, "variantId") {
                    let variant = self.store.product_variant_by_id(&variant_id).expect(
                        "all order variants were resolved before planning inventory effects",
                    );
                    if !variant.inventory_item.tracked {
                        continue;
                    }
                    variant.inventory_item.id.clone()
                } else if let Some(inventory_item_id) =
                    resolved_string_field(&line_item, "inventoryItemId")
                {
                    if self
                        .store
                        .product_variant_by_inventory_item_id(&inventory_item_id)
                        .is_none()
                    {
                        continue;
                    }
                    inventory_item_id
                } else {
                    continue;
                };
            if !is_shopify_gid_of_type(&inventory_item_id, "InventoryItem") {
                return Err(order_create_unresolved_variant_payload());
            }
            *quantities_by_item.entry(inventory_item_id).or_default() += quantity;
        }

        Ok(quantities_by_item
            .into_iter()
            .filter_map(|(inventory_item_id, quantity)| {
                let location_id =
                    self.inventory_item_decrement_location(&inventory_item_id, quantity)?;
                Some(OrderCreateInventoryEffect {
                    inventory_item_id,
                    location_id,
                    quantity,
                })
            })
            .collect())
    }

    pub(in crate::proxy) fn order_create_local_outcome(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        requested_field_paths: &BTreeSet<Vec<String>>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        if !matches!(
            root_field,
            "orderCreate"
                | "orderUpdate"
                | "orderClose"
                | "orderOpen"
                | "order"
                | "orders"
                | "ordersCount"
        ) {
            return None;
        }
        if matches!(root_field, "order" | "orders" | "ordersCount") {
            let staged_order_read = match root_field {
                "order" => resolved_string_field(arguments, "id")
                    .is_some_and(|id| self.store.order_has_staged_effect(&id)),
                "orders" | "ordersCount" => {
                    !self.store.staged.orders.is_empty()
                        || !self.store.staged.orders.tombstones.is_empty()
                        || !self.store.staged.order_overlays.is_empty()
                }
                _ => false,
            };
            if !staged_order_read {
                return None;
            }
            if self.config.read_mode == ReadMode::LiveHybrid {
                match root_field {
                    "order" => {
                        if let Some(id) = resolved_string_field(arguments, "id") {
                            self.ensure_order_hydrated(
                                request,
                                &id,
                                order_hydration_profile_for_requested_fields(requested_field_paths),
                            );
                        }
                    }
                    "orders" | "ordersCount" => self.observe_live_hybrid_order_read(request),
                    _ => {}
                }
            }
        }
        let value = match root_field {
            "orderCreate" => self.stage_order_create(request, query, variables, arguments),
            "orderUpdate" => self.stage_order_update(
                request,
                query,
                variables,
                arguments,
                requested_field_paths,
            )?,
            "orderClose" | "orderOpen" => {
                self.stage_order_lifecycle(request, query, variables, root_field, arguments)
            }
            "order" => {
                let id = resolved_string_field(arguments, "id")?;
                let order = self
                    .store
                    .observed_order_by_id(&id)
                    .map(|order| self.payment_terms_owner_record_with_effective_due(&order))
                    .unwrap_or(Value::Null);
                self.order_with_return_status_value(&order)
            }
            "orders" => self.staged_orders_connection(arguments),
            "ordersCount" => self.staged_orders_count(arguments),
            _ => return None,
        };
        Some(ResolverOutcome::value(value))
    }

    /// Full order projections from the seeded catalog that match a connection's
    /// `query:` filter, ordered by `sortKey`/`reverse`. The returned values are
    /// whole orders (not yet selection-projected) so the caller can window them
    /// and then project both `nodes` and `pageInfo` through the field selection.
    pub(super) fn matching_orders_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        staged_connection_query(
            self.store.effective_orders(),
            arguments,
            order_search_decision,
            order_staged_sort_key,
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn staged_orders_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let result = self.matching_orders_query(arguments);
        // Window with the order id as the opaque cursor. The next-page request in
        // the catalog scenario feeds this connection's own `endCursor` back as
        // `after`, so the cursor only needs to round-trip with itself — it is not
        // compared against Shopify's recorded opaque cursors.
        let records = result
            .records
            .into_iter()
            .map(|order| self.payment_terms_owner_record_with_effective_due(&order))
            .collect::<Vec<_>>();
        typed_connection_value(
            &records,
            |order| self.order_with_return_status_value(order),
            value_id_cursor,
            result.page_info,
        )
    }

    /// `ordersCount` over the seeded catalog: count matches, then apply Shopify's
    /// `limit` precision semantics — capped at `limit` and reported `AT_LEAST`
    /// when more matches exist than the limit, otherwise the exact total.
    pub(super) fn staged_orders_count(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        self.effective_orders_count_value(arguments)
    }

    fn effective_orders_count_value(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let baseline_key = order_count_baseline_key(arguments);
        let Some(baseline) = self.store.order_count_baseline(&baseline_key) else {
            return snapshot_count_with_limit_precision(
                self.matching_orders_query(arguments).total_count,
                arguments,
            );
        };
        let Some(base_count) = baseline.get("count").and_then(Value::as_u64) else {
            return snapshot_count_with_limit_precision(
                self.matching_orders_query(arguments).total_count,
                arguments,
            );
        };
        let delta = self.staged_order_count_delta(arguments);
        let effective_total = if delta.is_negative() {
            (base_count as usize).saturating_sub(delta.unsigned_abs())
        } else {
            (base_count as usize).saturating_add(delta as usize)
        };
        snapshot_count_with_limit_precision(effective_total, arguments)
    }

    fn staged_order_count_delta(&self, arguments: &BTreeMap<String, ResolvedValue>) -> isize {
        let query = resolved_string_field(arguments, "query");
        let mut delta = 0isize;
        for id in &self.store.staged.orders.tombstones {
            if let Some(base_order) = self.store.base.orders.records.get(id) {
                if order_matches_count_query(base_order, query.as_deref()) {
                    delta -= 1;
                }
            }
        }
        let staged_ids = self
            .store
            .staged
            .orders
            .records
            .keys()
            .chain(self.store.staged.order_overlays.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for id in staged_ids {
            if self.store.staged.orders.is_tombstoned(&id) {
                continue;
            }
            let Some(staged_order) = self.store.observed_order_by_id(&id) else {
                continue;
            };
            let staged_matches = order_matches_count_query(&staged_order, query.as_deref());
            if let Some(base_order) = self.store.base.orders.records.get(&id) {
                let base_matches = order_matches_count_query(base_order, query.as_deref());
                delta += staged_matches as isize - base_matches as isize;
            } else if staged_matches {
                delta += 1;
            }
        }
        delta
    }

    pub(super) fn stage_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
        requested_field_paths: &BTreeSet<Vec<String>>,
    ) -> Option<Value> {
        let input = resolved_object_field(arguments, "input")?;
        if resolved_string_field(&input, "staffMemberId").is_some() {
            return Some(json!({
                "order": Value::Null,
                "userErrors": [user_error_omit_code(["input", "staffMemberId"], "Staff member does not exist", None)]
            }));
        }

        let order_id = resolved_string_field(&input, "id")?;
        if !self.store.staged.orders.contains_key(&order_id) {
            let profile = order_hydration_profile_for_requested_fields(requested_field_paths);
            self.ensure_order_hydrated(request, &order_id, profile);
        }
        let Some(existing_order) = self.store.observed_order_by_id(&order_id) else {
            return Some(json!({
                "order": Value::Null,
                "userErrors": [user_error_omit_code(["id"], "Order does not exist", None)]
            }));
        };

        let validation_errors = order_update_validation_errors(&input);
        if !validation_errors.is_empty() {
            return Some(json!({
                "order": existing_order,
                "userErrors": validation_errors
            }));
        }

        let mut order = existing_order;
        if input.contains_key("note") {
            order["note"] = resolved_nullable_string_field(&input, "note");
        }
        if input.contains_key("tags") {
            order["tags"] = json!(resolved_string_list_field(&input, "tags"));
        }
        if input.contains_key("customAttributes") {
            order["customAttributes"] =
                json!(order_create_custom_attributes(&input, "customAttributes"));
        }
        if input.contains_key("email") {
            let email = resolved_nullable_string_field(&input, "email");
            order["email"] = email.clone();
        }
        if input.contains_key("phone") {
            order["phone"] = resolved_nullable_string_field(&input, "phone");
        }
        if input.contains_key("poNumber") {
            order["poNumber"] = resolved_nullable_string_field(&input, "poNumber");
        }
        if input.contains_key("shippingAddress") {
            order["shippingAddress"] =
                order_create_address(resolved_object_field(&input, "shippingAddress"));
        }
        if input.contains_key("metafields") {
            let existing_metafields = order["metafields"]["nodes"]
                .as_array()
                .cloned()
                .or_else(|| self.store.staged.owner_metafields.get(&order_id).cloned())
                .unwrap_or_default();
            let metafields = order_update_metafields(&order_id, &input, &existing_metafields);
            self.store
                .staged
                .owner_metafields
                .insert(order_id.clone(), metafields.clone());
            order["metafield"] = metafields.first().cloned().unwrap_or(Value::Null);
            order["metafields"] = order_connection(metafields);
        }
        // Shopify mirrors order localization between `localizedFields` and
        // `localizationExtensions`: a value submitted through either input
        // surfaces under both connections, and successive updates accumulate
        // (deduped by key) rather than replacing the prior set.
        let localization_input: Vec<Value> = resolved_object_list_field(&input, "localizedFields")
            .into_iter()
            .chain(resolved_object_list_field(&input, "localizationExtensions"))
            .filter_map(|entry| {
                let key = resolved_string_field(&entry, "key")?;
                let value = resolved_string_field(&entry, "value")?;
                Some(json!({ "key": key, "value": value }))
            })
            .collect();
        if !localization_input.is_empty() {
            let mut entries = order["localizedFields"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for entry in localization_input {
                let key = entry["key"].as_str().unwrap_or_default().to_string();
                if let Some(slot) = entries
                    .iter_mut()
                    .find(|existing| existing["key"].as_str() == Some(key.as_str()))
                {
                    *slot = entry;
                } else {
                    entries.push(entry);
                }
            }
            order["localizedFields"] = order_connection(entries.clone());
            order["localizationExtensions"] = order_connection(entries);
        }
        order["updatedAt"] = json!(order_mutation_timestamp(self.mutation_log_ordinal() as u64));

        let overlay = order_update_staged_overlay(&order, &input);
        self.store.stage_order_overlay(order_id.clone(), overlay);
        for orders in self.store.staged.customer_orders.values_mut() {
            for customer_order in orders {
                if customer_order["id"].as_str() == Some(order_id.as_str()) {
                    *customer_order = order.clone();
                }
            }
        }
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "orderUpdate",
            vec![order_id],
        );

        Some(json!({ "order": order, "userErrors": [] }))
    }

    pub(super) fn stage_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let order_input = resolved_object_field(arguments, "order").unwrap_or_default();
        if let Some(error) = order_create_validation_error(&order_input) {
            return json!({ "order": Value::Null, "userErrors": [error] });
        }
        if order_create_input_needs_shop_currency_default(&order_input) {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
        }
        if order_create_inventory_behaviour(arguments) != "BYPASS" {
            let inventory_effects = match self.order_create_inventory_effects(request, &order_input)
            {
                Ok(effects) => effects,
                Err(payload) => return payload,
            };
            for effect in inventory_effects {
                self.decrement_inventory_item_available_at_location(
                    &effect.inventory_item_id,
                    &effect.location_id,
                    effect.quantity,
                );
            }
        }

        let order_id = shopify_gid("Order", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        if order_create_updates_customer_email_from_order(request) {
            if let (Some(customer_id), Some(order_email)) = (
                resolved_string_field(&order_input, "customerId"),
                resolved_string_field(&order_input, "email"),
            ) {
                let updated_at = self.next_product_timestamp();
                let old_email = self
                    .store
                    .staged
                    .customers
                    .get(&customer_id)
                    .and_then(|customer| customer.get("email"))
                    .and_then(Value::as_str)
                    .map(storefront_customer_email_key);
                if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
                    customer["email"] = json!(order_email.clone());
                    customer["defaultEmailAddress"] = json!({
                        "emailAddress": order_email.clone(),
                        "marketingState": "NOT_SUBSCRIBED",
                        "marketingOptInLevel": "SINGLE_OPT_IN",
                        "marketingUpdatedAt": Value::Null
                    });
                    customer["emailMarketingConsent"] = json!({
                        "marketingState": "NOT_SUBSCRIBED",
                        "marketingOptInLevel": "SINGLE_OPT_IN",
                        "consentUpdatedAt": Value::Null
                    });
                    customer["updatedAt"] = json!(updated_at);
                    if let Some(object) = customer.as_object_mut() {
                        object.remove("acceptsMarketing");
                    }
                    if let Some(old_email) = old_email {
                        self.store
                            .staged
                            .storefront_customer_email_index
                            .remove(&old_email);
                    }
                    self.store
                        .staged
                        .storefront_customer_email_index
                        .insert(storefront_customer_email_key(&order_email), customer_id);
                }
            }
        }
        let order = self.build_order_create_record(&order_id, &order_input);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        if let Some(customer_id) = resolved_string_field(&order_input, "customerId") {
            self.store
                .staged
                .customer_orders
                .entry(customer_id)
                .or_default()
                .push(order.clone());
        }
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "orderCreate",
            vec![order_id],
        );
        json!({
            "order": self.order_with_return_status_value(&order),
            "userErrors": []
        })
    }

    pub(super) fn stage_order_lifecycle(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = order_lifecycle_input_id(arguments).unwrap_or_default();
        let Some(mut order) = self.order_lifecycle_order(&id, request, root_field) else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field,
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled order lifecycle mutation for an unknown order.",
                },
            });
            return json!({
                "order": Value::Null,
                "userErrors": [user_error_omit_code(["id"], "Order does not exist", None)]
            });
        };

        normalize_order_lifecycle_defaults(&mut order);
        let currently_closed = order["closed"].as_bool().unwrap_or(false);
        match root_field {
            "orderClose" if !currently_closed => {
                order["closed"] = json!(true);
                order["closedAt"] = json!("2024-01-01T00:00:01.000Z");
                order["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
            }
            "orderOpen" if currently_closed => {
                order["closed"] = json!(false);
                order["closedAt"] = Value::Null;
                order["updatedAt"] = json!("2024-01-01T00:00:02.000Z");
            }
            _ => {}
        }

        self.store.staged.orders.insert(id.clone(), order.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field,
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged order lifecycle mutation in shopify-draft-proxy.",
            },
        });
        json!({ "order": order, "userErrors": [] })
    }

    pub(super) fn order_lifecycle_order(
        &self,
        id: &str,
        request: &Request,
        root_field: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .orders
            .get(id)
            .cloned()
            .or_else(|| self.store.observed_order_by_id(id))
            .or_else(|| self.hydrate_order_lifecycle_order(id, request, root_field))
    }

    pub(super) fn hydrate_order_lifecycle_order(
        &self,
        id: &str,
        request: &Request,
        root_field: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_LIFECYCLE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let mut order = response.body["data"]["order"].clone();
        if order.is_null() {
            order = response.body["data"][root_field]["order"].clone();
        }
        if order.is_null() {
            None
        } else {
            Some(order)
        }
    }

    /// Stage the live lifecycle/summary projection of `id` into `staged.orders`
    /// if it is not already present. Used by order-customer mutations
    /// (orderCancel / orderCustomerSet / orderCustomerRemove) so their happy
    /// path earns the order from the backend rather than 404-ing when no
    /// precondition seed exists.
    pub(super) fn ensure_order_lifecycle_hydrated(&mut self, request: &Request, id: &str) {
        if id.is_empty()
            || self.store.staged.orders.contains_key(id)
            || self.store.staged.orders.is_tombstoned(id)
        {
            return;
        }
        if let Some(order) = self.hydrate_order_lifecycle_order(id, request, "") {
            self.store.staged.orders.insert(id.to_string(), order);
        }
    }

    /// Confirm an order exists on the backend without staging it. Used by the
    /// refundMethod orderCancel path, which acknowledges the cancel but defers the
    /// authoritative refunded/restocked order projection to the backend by leaving
    /// the order unstaged (the downstream read then forwards upstream).
    pub(super) fn order_exists_upstream(&self, request: &Request, id: &str) -> bool {
        !id.is_empty()
            && self
                .hydrate_order_lifecycle_order(id, request, "")
                .is_some()
    }

    /// Hydrate the summary customer projection used by orderCustomerSet and
    /// stage it under `staged.customers`. Issues the canonical `CustomerHydrate`
    /// query so a live backend returns the id/email/displayName the mutation
    /// then re-projects.
    pub(super) fn ensure_order_customer_hydrated(&mut self, request: &Request, id: &str) {
        if id.is_empty() || self.store.staged.customers.contains_key(id) {
            return;
        }
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_CUSTOMER_SUMMARY_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if !response_is_success(&response) {
            return;
        }
        let customer = response.body["data"]["customer"].clone();
        if customer.is_object() {
            self.store.staged.customers.insert(id.to_string(), customer);
        }
    }

    pub(super) fn staged_order_id_for_fulfillment_order(
        &self,
        fulfillment_order_id: &str,
    ) -> Option<String> {
        self.store
            .staged
            .orders
            .iter()
            .find_map(|(order_id, order)| {
                order["fulfillmentOrders"]["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|node| node["id"].as_str() == Some(fulfillment_order_id))
                    .then(|| order_id.clone())
            })
    }

    pub(in crate::proxy) fn staged_order_record_for_id(&self, order_id: &str) -> Option<Value> {
        self.store.observed_order_by_id(order_id).or_else(|| {
            self.store
                .staged
                .orders
                .values()
                .into_iter()
                .find(|order| order["id"].as_str() == Some(order_id))
                .cloned()
        })
    }

    pub(super) fn order_id_for_fulfillment_order(
        &mut self,
        fulfillment_order_id: &str,
        request: &Request,
    ) -> Option<String> {
        self.staged_order_id_for_fulfillment_order(fulfillment_order_id)
            .or_else(|| self.hydrate_order_for_fulfillment_order(fulfillment_order_id, request))
    }

    pub(super) fn stage_hydrated_order(&mut self, mut order: Value) -> Option<String> {
        normalize_hydrated_order(&mut order);
        let id = order.get("id").and_then(Value::as_str)?.to_string();
        self.store.staged.orders.insert(id.clone(), order);
        Some(id)
    }

    pub(super) fn hydrate_order_for_fulfillment_order(
        &mut self,
        fulfillment_order_id: &str,
        request: &Request,
    ) -> Option<String> {
        self.hydrate_order_for_fulfillment_order_with_query(
            fulfillment_order_id,
            request,
            ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY,
        )
    }

    pub(super) fn hydrate_order_for_fulfillment_order_with_query(
        &mut self,
        fulfillment_order_id: &str,
        request: &Request,
        hydrate_query: &str,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": hydrate_query,
                "variables": { "id": fulfillment_order_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let fulfillment_order = response.body["data"]["fulfillmentOrder"].clone();
        if fulfillment_order.is_object() {
            return self.merge_hydrated_fulfillment_order_into_order(fulfillment_order);
        }
        let order = response.body["data"]["fulfillmentOrder"]["order"].clone();
        if !order.is_object() {
            return None;
        }
        self.stage_hydrated_order(order)
    }

    pub(super) fn hydrate_order_for_edit(
        &mut self,
        order_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_EDIT_HYDRATE_QUERY,
                "operationName": "OrdersOrderEditHydrate",
                "variables": { "id": order_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let order = response.body["data"]["order"].clone();
        if !order.is_object() {
            return None;
        }
        self.stage_hydrated_order(order)
    }

    /// Forward a cold product-variant hydrate for `orderEditAddVariant` and
    /// observe the store-state fields the local edit engine resolves the added
    /// calculated line item against (title / sku / unit price). The order-edit
    /// variant catalog was previously established by a precondition seed; this
    /// forwards+observes the same projection the seed mirrored
    /// (`{ id, title, sku, price }`, title preferring the product title) so the
    /// cold path is byte-identical to the removed seed. Returns the observed
    /// entry, or None on a miss so the caller emits the canonical
    /// "variant does not exist" userError.
    pub(super) fn hydrate_order_edit_variant(
        &mut self,
        variant_id: &str,
        request: &Request,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_VARIANT_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderVariantHydrate",
                "variables": { "id": variant_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let variant = response.body["data"]["productVariant"].clone();
        if !variant.is_object() {
            return None;
        }
        let title = variant["product"]["title"]
            .as_str()
            .or_else(|| variant["title"].as_str())
            .map(|title| Value::String(title.to_string()))
            .unwrap_or(Value::Null);
        let entry = json!({
            "id": variant_id,
            "title": title,
            "sku": variant.get("sku").cloned().unwrap_or(Value::Null),
            "price": variant.get("price").cloned().unwrap_or(Value::Null),
        });
        if let Some(catalog) = self.store.staged.order_edit_variant_catalog.as_object_mut() {
            catalog.insert(variant_id.to_string(), entry.clone());
        }
        Some(entry)
    }

    pub(super) fn hydrate_order_for_mark_as_paid(
        &mut self,
        order_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_MARK_AS_PAID_HYDRATE_QUERY,
                "variables": { "id": order_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let order = response.body["data"]["order"].clone();
        if !order.is_object() {
            return None;
        }
        self.stage_hydrated_order(order)
    }

    pub(super) fn hydrate_order_for_fulfillment(
        &mut self,
        fulfillment_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDERS_FULFILLMENT_HYDRATE_QUERY,
                "variables": { "id": fulfillment_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let fulfillment = response.body["data"]["fulfillment"].clone();
        let mut order = fulfillment["order"].clone();
        if !order.is_object() {
            return None;
        }
        if !order["fulfillments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|record| record["id"].as_str() == Some(fulfillment_id))
            && fulfillment.is_object()
        {
            let mut fulfillment_record = fulfillment.clone();
            if let Some(object) = fulfillment_record.as_object_mut() {
                object.remove("order");
            }
            normalize_hydrated_order(&mut order);
            if let Some(fulfillments) = order_fulfillments_mut(&mut order) {
                fulfillments.push(fulfillment_record);
            }
        }
        self.stage_hydrated_order(order)
    }

    pub(super) fn hydrate_order_for_fulfillment_lifecycle(
        &mut self,
        fulfillment_id: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        // Stage one: resolve the fulfillment's owning order and the sibling
        // fulfillment states needed for the cancel/tracking preconditions.
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDERS_FULFILLMENT_LIFECYCLE_HYDRATE_QUERY,
                "variables": { "id": fulfillment_id }
            }),
        );
        if !response_is_success(&response) {
            return None;
        }
        let fulfillment = response.body["data"]["fulfillment"].clone();
        let mut order = fulfillment["order"].clone();
        if !order.is_object() {
            return None;
        }
        let order_id = order.get("id").and_then(Value::as_str)?.to_string();
        // Stage two (best-effort): enrich with the full fulfillment line-item view so a
        // downstream order read observes line items. A cassette miss here is non-fatal.
        let enriched = self.upstream_post(
            request,
            json!({
                "query": ORDER_FULFILLMENT_LIFECYCLE_READ_QUERY,
                "variables": { "id": order_id }
            }),
        );
        if response_is_success(&enriched) {
            let enriched_order = enriched.body["data"]["order"].clone();
            if enriched_order.is_object() {
                order = enriched_order;
            }
        }
        // Guarantee the target fulfillment is present in the staged list even when only the
        // stage-one projection was available.
        if !order["fulfillments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|record| record["id"].as_str() == Some(fulfillment_id))
            && fulfillment.is_object()
        {
            let mut fulfillment_record = fulfillment.clone();
            if let Some(object) = fulfillment_record.as_object_mut() {
                object.remove("order");
            }
            normalize_hydrated_order(&mut order);
            if let Some(fulfillments) = order_fulfillments_mut(&mut order) {
                fulfillments.push(fulfillment_record);
            }
        }
        self.stage_hydrated_order(order)
    }

    pub(super) fn build_order_create_record(
        &mut self,
        order_id: &str,
        order_input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let default_currency_code = self.b2b_order_input_currency_default(order_input);
        let currency_code = resolved_string_field(order_input, "currency")
            .or_else(|| resolved_string_field(order_input, "currencyCode"))
            .unwrap_or(default_currency_code);
        let presentment_currency_code = resolved_string_field(order_input, "presentmentCurrency")
            .or_else(|| resolved_string_field(order_input, "presentmentCurrencyCode"))
            .unwrap_or_else(|| currency_code.clone());
        let mut subtotal = 0.0;
        let mut tax_total = 0.0;
        let line_items = resolved_object_list_field(order_input, "lineItems")
            .into_iter()
            .enumerate()
            .map(|(index, line_item)| {
                let (line, line_subtotal, line_tax_total) = order_create_line_item_record(
                    &line_item,
                    index,
                    &currency_code,
                    &presentment_currency_code,
                );
                subtotal += line_subtotal;
                tax_total += line_tax_total;
                line
            })
            .collect::<Vec<_>>();
        let fulfillment_orders = if line_items.is_empty() {
            Vec::new()
        } else {
            let mut fulfillment_order = order_default_fulfillment_order(order_id, &line_items);
            if let Some(assigned_location) = self.default_fulfillment_assigned_location() {
                fulfillment_order["assignedLocation"] = assigned_location;
            }
            set_fulfillment_order_status_from_lines(&mut fulfillment_order);
            vec![fulfillment_order]
        };
        let shipping_lines = resolved_object_list_field(order_input, "shippingLines")
            .into_iter()
            .map(|shipping_line| {
                let price_input =
                    resolved_object_field(&shipping_line, "priceSet").unwrap_or_default();
                let amount = input_money_amount(&price_input).unwrap_or(0.0);
                let shipping_currency =
                    input_money_currency(&price_input).unwrap_or_else(|| currency_code.clone());
                let tax_lines = order_create_tax_lines(&shipping_line, "taxLines", &currency_code);
                tax_total += sum_money_set(&tax_lines, "priceSet");
                json!({
                    "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
                    "code": resolved_string_field(&shipping_line, "code").unwrap_or_default(),
                    "source": resolved_string_field(&shipping_line, "source").unwrap_or_default(),
                    "originalPriceSet": money_bag(amount, &shipping_currency),
                    "priceSet": money_bag(amount, &shipping_currency),
                    "taxLines": tax_lines
                })
            })
            .collect::<Vec<_>>();
        let shipping_total = sum_money_set(&shipping_lines, "originalPriceSet");
        let (discount_total, discount_codes) =
            order_create_discount_amount(order_input, &currency_code);
        let total = (subtotal + shipping_total + tax_total - discount_total).max(0.0);
        let transactions = resolved_object_list_field(order_input, "transactions")
            .into_iter()
            .map(|transaction| {
                let transaction_id = self.next_order_transaction_id();
                order_create_transaction_record(&transaction, transaction_id, &currency_code)
            })
            .collect::<Vec<_>>();
        let financial_status = order_create_financial_status(order_input, &transactions, total);
        let order_number = self.next_order_number();
        let order_name = resolved_string_field(order_input, "name")
            .unwrap_or_else(|| format!("#{order_number}"));
        let created_at = self.next_mutation_timestamp();
        let mut order = json!({
            "id": order_id,
            "name": order_name,
            "orderNumber": order_number,
            "email": resolved_string_field(order_input, "email"),
            "phone": resolved_string_field(order_input, "phone"),
            // Retain the purchasing entity (B2B purchasing company/contact) the
            // order was placed under, the way a real Order exposes it — both so it
            // reads back and so a company delete can detect the order still
            // references it.
            "purchasingEntity": draft_order_purchasing_entity(order_input),
            "companyLocationId": resolved_string_field(order_input, "companyLocationId"),
            "closed": false,
            "closedAt": Value::Null,
            "cancelledAt": Value::Null,
            "cancelReason": Value::Null,
            "createdAt": created_at,
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "processedAt": resolved_string_field(order_input, "processedAt")
                .unwrap_or_else(|| "2024-01-01T00:00:00.000Z".to_string()),
            "customer": resolved_string_field(order_input, "customerId")
                .map(|id| {
                    // A locally-staged customer carries the authoritative identity
                    // (its own email/displayName, which differ from the order's
                    // contact email). Mirror that record so reads of
                    // order.customer reflect the customer, not the order email.
                    if let Some(customer) = self.store.staged.customers.get(&id) {
                        let mut customer = customer.clone();
                        customer["canDelete"] = json!(false);
                        customer
                    } else {
                        json!({
                            "id": id,
                            "email": resolved_string_field(order_input, "email"),
                            "displayName": Value::Null
                        })
                    }
                })
                .unwrap_or(Value::Null),
            "note": resolved_string_field(order_input, "note"),
            "tags": resolved_string_list_field(order_input, "tags"),
            "currencyCode": currency_code,
            "presentmentCurrencyCode": presentment_currency_code,
            "displayFinancialStatus": financial_status,
            "displayFulfillmentStatus": resolved_string_field(order_input, "fulfillmentStatus")
                .unwrap_or_else(|| "UNFULFILLED".to_string()),
            "customAttributes": order_create_custom_attributes(order_input, "customAttributes"),
            "billingAddress": order_create_address(resolved_object_field(order_input, "billingAddress")),
            "shippingAddress": order_create_address(resolved_object_field(order_input, "shippingAddress")),
            "subtotalPriceSet": money_bag_from_amount(subtotal, &currency_code, &presentment_currency_code),
            "currentSubtotalPriceSet": money_bag_from_amount(subtotal, &currency_code, &presentment_currency_code),
            "totalShippingPriceSet": money_bag_from_amount(shipping_total, &currency_code, &presentment_currency_code),
            "totalTaxSet": money_bag_from_amount(tax_total, &currency_code, &presentment_currency_code),
            "currentTotalTaxSet": money_bag_from_amount(tax_total, &currency_code, &presentment_currency_code),
            "totalDiscountsSet": money_bag_from_amount(discount_total, &currency_code, &presentment_currency_code),
            "currentTotalDiscountsSet": money_bag_from_amount(discount_total, &currency_code, &presentment_currency_code),
            "currentTotalPriceSet": money_bag_from_amount(total, &currency_code, &presentment_currency_code),
            "totalPriceSet": money_bag_from_amount(total, &currency_code, &presentment_currency_code),
            "discountCodes": discount_codes,
            "shippingLines": order_connection(shipping_lines),
            "lineItems": order_connection(line_items),
            "fulfillments": [],
            "fulfillmentOrders": order_connection(fulfillment_orders),
            "transactions": transactions
        });
        let fulfillment_order_parent = json!({
            "id": order["id"].clone(),
            "name": order["name"].clone(),
            "displayFulfillmentStatus": order["displayFulfillmentStatus"].clone()
        });
        if let Some(fulfillment_orders) = fulfillment_order_nodes_mut(&mut order) {
            for fulfillment_order in fulfillment_orders {
                fulfillment_order["order"] = fulfillment_order_parent.clone();
            }
        }
        order_create_payment_fields(
            &mut order,
            &transactions,
            total,
            &currency_code,
            &presentment_currency_code,
        );
        order
    }

    pub(super) fn record_orders_local_log_entry(&mut self, entry: OrdersLocalLogEntry<'_>) {
        let root_fields = parse_operation(entry.query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![entry.root_field.to_string()]);
        self.log_entries.push(json!({
            "id": shopify_gid("MutationLogEntry", self.log_entries.len() + 1),
            "operationName": entry.root_field,
            "path": entry.request.path,
            "query": entry.query,
            "variables": resolved_variables_json(entry.variables),
            "rawBody": entry.request.body,
            "stagedResourceIds": entry.staged_resource_ids,
            "status": entry.outcome.status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": entry.root_field,
                "rootFields": root_fields,
                "primaryRootField": entry.root_field,
                "capability": {
                    "operationName": entry.root_field,
                    "domain": "orders",
                    "execution": "stage-locally"
                }
            },
            "notes": entry.outcome.notes
        }));
    }

    pub(super) fn record_staged_orders_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let notes = format!("Locally staged {root_field} in shopify-draft-proxy.");
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field,
            staged_resource_ids,
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: &notes,
            },
        });
    }

    pub(in crate::proxy) fn remaining_order_local_outcome(
        &mut self,
        context: &OrderRootContext<'_>,
    ) -> Option<ResolverOutcome<Value>> {
        let request = context.request;
        let root_field = context.root_field;
        let response_key = context.response_key;
        let arguments = context.arguments;
        let requested_field_paths = context.requested_field_paths;
        let query = context.query;
        let variables = context.variables;
        if root_field == "fulfillment" {
            let payload = self.staged_fulfillment_read_payload(arguments)?;
            return Some(ResolverOutcome::value(payload));
        }
        if root_field == "fulfillmentCreate" || root_field == "fulfillmentCreateV2" {
            if let Some(error) = fulfillment_create_invalid_id_error(arguments, response_key) {
                return Some(error);
            }
            return Some(ResolverOutcome::value(self.staged_fulfillment_payload(
                request, query, variables, root_field, arguments,
            )));
        }
        if root_field == "fulfillmentEventCreate" {
            return Some(ResolverOutcome::value(
                self.staged_fulfillment_event_create_payload(request, query, variables, arguments),
            ));
        }
        if root_field == "fulfillmentCancel" {
            let payload =
                self.cancel_staged_fulfillment_payload(request, query, variables, arguments)?;
            return Some(ResolverOutcome::value(payload));
        }
        if matches!(
            root_field,
            "fulfillmentTrackingInfoUpdate" | "fulfillmentTrackingInfoUpdateV2"
        ) {
            let payload = self.update_staged_fulfillment_tracking_payload(
                request, query, variables, arguments, root_field,
            )?;
            return Some(ResolverOutcome::value(payload));
        }
        if root_field == "ordersCount" {
            return Some(ResolverOutcome::value(
                self.effective_orders_count_value(arguments),
            ));
        }
        if root_field == "orderCreate" {
            let order_input = resolved_object_field(arguments, "order")?;
            let purchasing_entity = self.order_create_b2b_purchasing_entity(&order_input);
            if !order_customer_purchasing_entity_is_b2b(&purchasing_entity) {
                return None;
            }
            let order = self.order_customer_paths_order_create(arguments)?;
            return Some(ResolverOutcome::value(order));
        }
        if root_field == "orderDelete" {
            let payload = self.stage_order_delete(request, query, variables, arguments)?;
            return Some(ResolverOutcome::value(payload));
        }
        if root_field == "orderUpdate"
            && resolved_object_field(arguments, "input")
                .and_then(|input| resolved_string_field(&input, "staffMemberId"))
                .is_some()
        {
            return Some(ResolverOutcome::value(json!({
                "order": Value::Null,
                "userErrors": [user_error_omit_code(["input", "staffMemberId"], "Staff member does not exist", None)]
            })));
        }
        match root_field {
            "orderEditBegin" => {
                return self.order_edit_begin_local(request, query, variables, arguments);
            }
            "orderEditAddVariant" => {
                return self.order_edit_add_variant_local(request, query, variables, arguments);
            }
            "orderEditSetQuantity" => {
                return self.order_edit_set_quantity_local(request, query, variables, arguments);
            }
            "orderEditAddCustomItem" => {
                return self.order_edit_add_custom_item_local(request, query, variables, arguments);
            }
            "orderEditAddLineItemDiscount" => {
                return self
                    .order_edit_add_line_item_discount_local(request, query, variables, arguments);
            }
            "orderEditRemoveDiscount" => {
                return self.order_edit_remove_discount_local(request, query, variables, arguments);
            }
            "orderEditAddShippingLine" => {
                return self
                    .order_edit_add_shipping_line_local(request, query, variables, arguments);
            }
            "orderEditUpdateShippingLine" => {
                return self
                    .order_edit_update_shipping_line_local(request, query, variables, arguments);
            }
            "orderEditRemoveShippingLine" => {
                return self
                    .order_edit_remove_shipping_line_local(request, query, variables, arguments);
            }
            "orderEditCommit" => {
                return self.order_edit_commit_local(request, query, variables, arguments);
            }
            _ => {}
        }
        if root_field == "order"
            && order_read_selects_order_edit_existing_fields(requested_field_paths)
        {
            let order = self.store.staged.order_edit_existing_order.as_ref()?;
            return Some(ResolverOutcome::value(order.clone()));
        }
        None
    }

    pub(super) fn require_calculated_order(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Result<String, Value> {
        self.require_calculated_order_with_code(arguments, None)
    }

    pub(super) fn require_calculated_order_with_code(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        code: Option<&str>,
    ) -> Result<String, Value> {
        let calculated_id = resolved_string_field(arguments, "id").unwrap_or_default();
        if self
            .store
            .staged
            .order_edit_existing_calculated_order_id
            .as_deref()
            != Some(calculated_id.as_str())
        {
            return Err(order_edit_error_payload(vec![user_error_omit_code(
                ["id"],
                "The calculated order does not exist.",
                code,
            )]));
        }
        Ok(calculated_id)
    }

    fn order_edit_session_context(&self, calculated_id: &str) -> (Value, String, String) {
        let session = self
            .store
            .staged
            .order_edit_existing_calculated_order
            .clone()
            .unwrap_or_else(|| json!({}));
        let currency = oe_session_currency(&session).to_string();
        let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
        (session, currency, session_id)
    }

    pub(super) fn order_edit_begin_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let order_id = resolved_string_field(arguments, "id").unwrap_or_default();
        // The edit targets an order that lives in the backend, not one created
        // locally in this scenario. Forward a hydrate read and observe it so the
        // edit session is built from real order state instead of requiring a
        // precondition seed. A record produced by an earlier local mutation is
        // more current than the backend snapshot, so only hydrate on a cold miss.
        // On a cassette miss this is a no-op and we fall through to the
        // "order does not exist" guard below.
        if !self.store.staged.orders.contains_key(&order_id) {
            self.hydrate_order_for_edit(&order_id, request);
        }
        let order = match self.store.staged.orders.get(&order_id) {
            Some(order) => order.clone(),
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["id"],
                    "The order does not exist.",
                    None,
                )]);
            }
        };
        if order_edit_order_is_not_editable(&order) {
            return order_edit_error_outcome(vec![user_error_omit_code(
                Value::Null,
                "The order cannot be edited.",
                None,
            )]);
        }
        // Shopify allows only one open order edit per order: beginning a
        // second edit while a prior session is still uncommitted is rejected.
        // The slot is cleared on commit, so post-commit re-edits are allowed.
        if self
            .store
            .staged
            .order_edit_existing_session_order_id
            .as_deref()
            == Some(order_id.as_str())
        {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["base"],
                "This order already has an order edit in progress.",
                None,
            )]);
        }
        let calculated_id = shopify_gid("CalculatedOrder", resource_id_tail(&order_id));
        let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
        let session = oe_build_session(&order, &calculated_id, &session_id);
        let view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_order = Some(order);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.store.staged.order_edit_existing_calculated_order_id = Some(calculated_id.clone());
        self.store.staged.order_edit_existing_session_order_id = Some(order_id);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditBegin",
            vec![calculated_id],
        );
        Some(ResolverOutcome::value(json!({
            "calculatedOrder": view,
            "orderEditSession": { "id": session_id },
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_add_variant_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id = match self.require_calculated_order(arguments) {
            Ok(calculated_id) => calculated_id,
            Err(payload) => return Some(ResolverOutcome::value(payload)),
        };
        let variant_id = resolved_string_field(arguments, "variantId").unwrap_or_default();
        if resource_id_tail(&variant_id) == "0" {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["variantId"],
                "can't convert Integer[0] to a positive Integer to use as an untrusted id",
                None,
            )]);
        }
        let quantity = resolved_int_field(arguments, "quantity").unwrap_or(0);
        if quantity == 0 {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["quantity"],
                "must be greater than 0",
                None,
            )]);
        }
        if quantity < 0 {
            return order_edit_error_outcome(vec![
                user_error_omit_code(["quantity"], "must be greater than 0", None),
                user_error_omit_code(["quantity"], "must be greater than or equal to 0", None),
            ]);
        }
        let allow_duplicates = resolved_bool_field(arguments, "allowDuplicates").unwrap_or(false);
        let (mut session, currency, session_id) = self.order_edit_session_context(&calculated_id);
        // When the variant is already on the order and the caller did not opt
        // into duplicates, Shopify rejects the add: every payload resource is
        // null and a title-bearing userError is anchored on `id`.
        if !allow_duplicates {
            let existing = session
                .get("lines")
                .and_then(Value::as_array)
                .and_then(|lines| {
                    lines
                        .iter()
                        .find(|line| line["variant"]["id"].as_str() == Some(variant_id.as_str()))
                        .cloned()
                });
            if let Some(line) = existing {
                let title = line.get("title").and_then(Value::as_str).unwrap_or("");
                let message = format!("{title} was not added because it's already on the order.");
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["id"],
                    &message,
                    None,
                )]);
            }
        }
        let catalog_entry = self
            .store
            .staged
            .order_edit_variant_catalog
            .get(variant_id.as_str())
            .cloned();
        // The variant lives in the backend, not in a precondition seed.
        // Forward a cold variant hydrate and observe it into the catalog so
        // the added calculated line is built from real store state.
        let catalog_entry = match catalog_entry {
            Some(entry) => Some(entry),
            None => self.hydrate_order_edit_variant(&variant_id, request),
        };
        let catalog_entry = match catalog_entry {
            Some(entry) => entry,
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["variantId"],
                    "The variant does not exist in the shop.",
                    None,
                )]);
            }
        };
        let seq = oe_next_seq(&mut session);
        let unit = oe_amount_to_cents(
            catalog_entry
                .get("price")
                .and_then(Value::as_str)
                .unwrap_or("0"),
        );
        let line = json!({
            "calcId": shopify_gid("CalculatedLineItem", format_args!("oe-{seq}")),
            "orderLineId": Value::Null,
            "kind": "added",
            "title": catalog_entry.get("title").cloned().unwrap_or(Value::Null),
            "sku": catalog_entry.get("sku").cloned().unwrap_or(Value::Null),
            "variant": { "id": variant_id },
            "unitCents": unit,
            "histQty": quantity,
            "curQty": quantity,
            "taxLines": [],
            "discounts": []
        });
        if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
            lines.push(line.clone());
        }
        let view = oe_line_view(&line, &currency);
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditAddVariant",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(json!({
            "calculatedOrder": order_view,
            "calculatedLineItem": view,
            "orderEditSession": { "id": session_id },
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_set_quantity_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id = match self.require_calculated_order(arguments) {
            Ok(calculated_id) => calculated_id,
            Err(payload) => return Some(ResolverOutcome::value(payload)),
        };
        let quantity = resolved_int_field(arguments, "quantity").unwrap_or(0);
        if quantity < 0 {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["quantity"],
                "must be greater than or equal to 0",
                None,
            )]);
        }
        let line_item_id = resolved_string_field(arguments, "lineItemId").unwrap_or_default();
        let (mut session, currency, session_id) = self.order_edit_session_context(&calculated_id);
        let index = match oe_line_index(&session, &line_item_id) {
            Some(index) => index,
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["lineItemId"],
                    "The line item does not exist on the order.",
                    None,
                )]);
            }
        };
        session["lines"][index]["curQty"] = json!(quantity);
        let line = session["lines"][index].clone();
        let view = oe_line_view(&line, &currency);
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditSetQuantity",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(json!({
            "calculatedOrder": order_view,
            "calculatedLineItem": view,
            "orderEditSession": { "id": session_id },
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_add_custom_item_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id = match self.require_calculated_order(arguments) {
            Ok(calculated_id) => calculated_id,
            Err(payload) => return Some(ResolverOutcome::value(payload)),
        };
        let (mut session, currency, session_id) = self.order_edit_session_context(&calculated_id);
        let title = resolved_string_field(arguments, "title").unwrap_or_default();
        if title.trim().is_empty() {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["title"],
                "can't be blank",
                None,
            )]);
        }
        if title.chars().count() > 255 {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["title"],
                "is too long (maximum is 255 characters)",
                None,
            )]);
        }
        let quantity = resolved_int_field(arguments, "quantity").unwrap_or(0);
        if quantity <= 0 {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["quantity"],
                "must be greater than 0",
                None,
            )]);
        }
        let price = resolved_object_field(arguments, "price").unwrap_or_default();
        if resolved_money_currency(&price).as_deref() != Some(currency.as_str()) {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["price", "amount"],
                &format!("Currency must be {currency}."),
                None,
            )]);
        }
        let price_cents = oe_money_obj_cents(&price).unwrap_or(0);
        if price_cents < 0 {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["price", "amount"],
                "must be greater than or equal to 0",
                None,
            )]);
        }
        let seq = oe_next_seq(&mut session);
        let line = json!({
            "calcId": shopify_gid("CalculatedLineItem", format_args!("oe-{seq}")),
            "orderLineId": Value::Null,
            "kind": "custom",
            "title": title,
            "sku": Value::Null,
            "variant": Value::Null,
            "unitCents": price_cents,
            "histQty": quantity,
            "curQty": quantity,
            "taxLines": [],
            "discounts": []
        });
        if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
            lines.push(line.clone());
        }
        let view = oe_line_view(&line, &currency);
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditAddCustomItem",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(json!({
            "calculatedOrder": order_view,
            "calculatedLineItem": view,
            "orderEditSession": { "id": session_id },
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_add_line_item_discount_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id = match self.require_calculated_order(arguments) {
            Ok(calculated_id) => calculated_id,
            Err(payload) => return Some(ResolverOutcome::value(payload)),
        };
        let (mut session, currency, _) = self.order_edit_session_context(&calculated_id);
        let line_item_id = resolved_string_field(arguments, "lineItemId").unwrap_or_default();
        let index = match oe_line_index(&session, &line_item_id) {
            Some(index) => index,
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["id"],
                    "The line item does not exist on the order.",
                    None,
                )]);
            }
        };
        let discount = resolved_object_field(arguments, "discount").unwrap_or_default();
        let description = resolved_string_field(&discount, "description");
        let per_unit = resolved_object_field(&discount, "fixedValue")
            .as_ref()
            .and_then(oe_money_obj_cents)
            .or_else(|| {
                resolved_number_field(&discount, "percentValue").map(|percent| {
                    let unit = oe_int(&session["lines"][index], "unitCents");
                    ((unit as f64 * percent) / 100.0).round() as i64
                })
            })
            .unwrap_or(0);
        let seq = oe_next_seq(&mut session);
        let app_id = shopify_gid(
            "CalculatedManualDiscountApplication",
            format_args!("oe-disc-{seq}"),
        );
        let staged_change_id = shopify_gid(
            "OrderStagedChangeAddLineItemDiscount",
            format_args!("oe-disc-{seq}"),
        );
        let discount_entry = json!({
            "perUnitCents": per_unit,
            "description": description.clone(),
            "appId": app_id
        });
        if let Some(discounts) = session
            .get_mut("lines")
            .and_then(Value::as_array_mut)
            .and_then(|lines| lines.get_mut(index))
            .and_then(|line| line.get_mut("discounts"))
            .and_then(Value::as_array_mut)
        {
            discounts.push(discount_entry);
        }
        let line = session["lines"][index].clone();
        let view = oe_line_view(&line, &currency);
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditAddLineItemDiscount",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(json!({
            "addedDiscountStagedChange": {
                "id": staged_change_id,
                "description": description
            },
            "calculatedOrder": order_view,
            "calculatedLineItem": view,
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_remove_discount_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id =
            match self.require_calculated_order_with_code(arguments, Some("INVALID")) {
                Ok(calculated_id) => calculated_id,
                Err(payload) => return Some(ResolverOutcome::value(payload)),
            };
        let (mut session, _, _) = self.order_edit_session_context(&calculated_id);
        let discount_application_id =
            resolved_string_field(arguments, "discountApplicationId").unwrap_or_default();
        if let Some(lines) = session.get_mut("lines").and_then(Value::as_array_mut) {
            for line in lines.iter_mut() {
                if let Some(discounts) = line.get_mut("discounts").and_then(Value::as_array_mut) {
                    discounts.retain(|discount| {
                        discount.get("appId").and_then(Value::as_str)
                            != Some(discount_application_id.as_str())
                    });
                }
            }
        }
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditRemoveDiscount",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(
            json!({ "calculatedOrder": order_view, "userErrors": [] }),
        ))
    }

    pub(super) fn order_edit_add_shipping_line_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id =
            match self.require_calculated_order_with_code(arguments, Some("INVALID")) {
                Ok(calculated_id) => calculated_id,
                Err(payload) => return Some(ResolverOutcome::value(payload)),
            };
        let (mut session, currency, _) = self.order_edit_session_context(&calculated_id);
        let shipping_line = resolved_object_field(arguments, "shippingLine").unwrap_or_default();
        let title = resolved_string_field(&shipping_line, "title");
        let price = resolved_object_field(&shipping_line, "price").unwrap_or_default();
        if resolved_money_currency(&price).as_deref() != Some(currency.as_str()) {
            return order_edit_error_outcome(vec![user_error_omit_code(
                ["shippingLine", "price"],
                &format!("The price must be in {currency}."),
                Some("INVALID"),
            )]);
        }
        let price_cents = oe_money_obj_cents(&price).unwrap_or(0);
        let seq = oe_next_seq(&mut session);
        let shipping = json!({
            "id": shopify_gid("CalculatedShippingLine", format_args!("oe-ship-{seq}")),
            "title": title,
            "stagedStatus": "ADDED",
            "priceCents": price_cents
        });
        if let Some(lines) = session
            .get_mut("shippingLines")
            .and_then(Value::as_array_mut)
        {
            lines.push(shipping.clone());
        }
        let view = oe_shipping_view(&shipping, &currency);
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditAddShippingLine",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(json!({
            "calculatedOrder": order_view,
            "calculatedShippingLine": view,
            "userErrors": []
        })))
    }

    pub(super) fn order_edit_update_shipping_line_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id =
            match self.require_calculated_order_with_code(arguments, Some("INVALID")) {
                Ok(calculated_id) => calculated_id,
                Err(payload) => return Some(ResolverOutcome::value(payload)),
            };
        let (mut session, currency, _) = self.order_edit_session_context(&calculated_id);
        let shipping_line_id =
            resolved_string_field(arguments, "shippingLineId").unwrap_or_default();
        let index = match oe_shipping_index(&session, &shipping_line_id) {
            Some(index) => index,
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                            ["shippingLineId"],
                                "The shipping line can't be updated because it doesn't exist or wasn't added during this edit.",
                                Some("INVALID"),
                            )]);
            }
        };
        let shipping_line = resolved_object_field(arguments, "shippingLine").unwrap_or_default();
        let price = resolved_object_field(&shipping_line, "price");
        if let Some(price) = price.as_ref() {
            if resolved_money_currency(price).as_deref() != Some(currency.as_str()) {
                return order_edit_error_outcome(vec![user_error_omit_code(
                    ["shippingLine", "price"],
                    &format!("The price must be in {currency}."),
                    Some("INVALID"),
                )]);
            }
        }
        let new_title = resolved_string_field(&shipping_line, "title");
        let new_price = price.as_ref().and_then(oe_money_obj_cents);
        if let Some(node) = session
            .get_mut("shippingLines")
            .and_then(Value::as_array_mut)
            .and_then(|lines| lines.get_mut(index))
        {
            if let Some(title) = new_title {
                node["title"] = json!(title);
            }
            if let Some(cents) = new_price {
                node["priceCents"] = json!(cents);
            }
        }
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditUpdateShippingLine",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(
            json!({ "calculatedOrder": order_view, "userErrors": [] }),
        ))
    }

    pub(super) fn order_edit_remove_shipping_line_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id =
            match self.require_calculated_order_with_code(arguments, Some("INVALID")) {
                Ok(calculated_id) => calculated_id,
                Err(payload) => return Some(ResolverOutcome::value(payload)),
            };
        let (mut session, _, _) = self.order_edit_session_context(&calculated_id);
        let shipping_line_id =
            resolved_string_field(arguments, "shippingLineId").unwrap_or_default();
        let index = match oe_shipping_index(&session, &shipping_line_id) {
            Some(index) => index,
            None => {
                return order_edit_error_outcome(vec![user_error_omit_code(
                            ["shippingLineId"],
                                "The shipping line can't be removed because it doesn't exist or has already been removed.",
                                Some("INVALID"),
                            )]);
            }
        };
        if let Some(lines) = session
            .get_mut("shippingLines")
            .and_then(Value::as_array_mut)
        {
            lines.remove(index);
        }
        let order_view = oe_calc_order_view(&session);
        self.store.staged.order_edit_existing_calculated_order = Some(session);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "orderEditRemoveShippingLine",
            vec![calculated_id.clone()],
        );
        Some(ResolverOutcome::value(
            json!({ "calculatedOrder": order_view, "userErrors": [] }),
        ))
    }

    pub(super) fn order_edit_commit_local(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        let calculated_id = match self.require_calculated_order(arguments) {
            Ok(calculated_id) => calculated_id,
            Err(payload) => return Some(ResolverOutcome::value(payload)),
        };
        let (session, _, _) = self.order_edit_session_context(&calculated_id);
        let base = self
            .store
            .staged
            .order_edit_existing_order
            .clone()
            .unwrap_or_else(|| json!({}));
        // The edited-order event names the acting app in "<author> edited this
        // order." That attribution string is opaque store/app state Shopify
        // renders server-side and exposes via no queryable Admin API field (not
        // even the event's own `appTitle`), so the proxy cannot reproduce it
        // without a seed. The author is left unresolved here (event message is
        // the empty string required by the non-null schema); captured parity
        // excludes the un-reproducible attribution text.
        let author = self.store.staged.order_edit_author.clone();
        let order_unarchived = order_edit_order_is_closed(&base);
        let committed = oe_commit_order(&base, &session, author.as_deref());
        let notify_customer = resolved_bool_field(arguments, "notifyCustomer").unwrap_or(false);
        let success_messages =
            order_edit_commit_success_messages(&committed, notify_customer, order_unarchived);
        if let Some(order_id) = committed["id"].as_str() {
            self.store
                .staged
                .orders
                .insert(order_id.to_string(), committed.clone());
        }
        let staged_ids = committed["id"]
            .as_str()
            .map(str::to_string)
            .into_iter()
            .collect();
        self.record_mutation_log_entry(request, query, variables, "orderEditCommit", staged_ids);
        self.store.staged.order_edit_existing_order = Some(committed.clone());
        self.store.staged.order_edit_existing_calculated_order = None;
        self.store.staged.order_edit_existing_calculated_order_id = None;
        self.store.staged.order_edit_existing_session_order_id = None;
        Some(ResolverOutcome::value(json!({
            "order": committed,
            "successMessages": success_messages,
            "userErrors": []
        })))
    }

    pub(super) fn stage_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let order_id = resolved_string_field(arguments, "orderId")?;
        if !self.store.staged.orders.contains_key(&order_id)
            && !self.store.staged.orders.is_tombstoned(&order_id)
            && self.store.observed_order_by_id(&order_id).is_none()
        {
            self.ensure_order_hydrated(request, &order_id, OrderHydrationProfile::Identity);
        }
        if self.store.observed_order_by_id(&order_id).is_none() {
            return Some(json!({
                "deletedId": Value::Null,
                "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
            }));
        }

        self.delete_staged_order(&order_id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "orderDelete",
            vec![order_id.clone()],
        );
        Some(json!({
            "deletedId": order_id,
            "userErrors": []
        }))
    }

    pub(super) fn delete_staged_order(&mut self, order_id: &str) {
        self.store.staged.orders.remove(order_id);
        self.store.staged.order_overlays.remove(order_id);
        self.store.staged.orders.tombstone(order_id.to_string());

        for orders in self.store.staged.customer_orders.values_mut() {
            orders.retain(|order| order["id"].as_str() != Some(order_id));
        }
        self.store
            .staged
            .customer_orders
            .retain(|_, orders| !orders.is_empty());

        if let Some(terms_id) = self.store.staged.payment_terms_owner_index.remove(order_id) {
            self.store.staged.payment_terms.remove(&terms_id);
        }

        if let Some(return_ids) = self.store.staged.returns_by_order.remove(order_id) {
            for return_id in return_ids {
                if let Some(record) = self.store.staged.returns.remove(&return_id) {
                    if let Some(nodes) = record["reverseFulfillmentOrders"]["nodes"].as_array() {
                        for node in nodes {
                            if let Some(reverse_id) = node["id"].as_str() {
                                self.remove_reverse_fulfillment_order(reverse_id);
                            }
                        }
                    }
                }
            }
        }

        self.store.staged.order_customer_orders.remove(order_id);
        self.store
            .staged
            .order_customer_cancelled_ids
            .remove(order_id);
        self.store
            .staged
            .order_customer_b2b_order_ids
            .remove(order_id);
    }

    pub(super) fn remove_reverse_fulfillment_order(&mut self, reverse_id: &str) {
        self.store
            .staged
            .reverse_fulfillment_orders
            .remove(reverse_id);
        let delivery_ids = self
            .store
            .staged
            .reverse_deliveries
            .iter()
            .filter(|(_, delivery)| {
                delivery["reverseFulfillmentOrder"]["id"].as_str() == Some(reverse_id)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for delivery_id in delivery_ids {
            self.store.staged.reverse_deliveries.remove(&delivery_id);
        }
    }
}

fn order_create_updates_customer_email_from_order(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
}

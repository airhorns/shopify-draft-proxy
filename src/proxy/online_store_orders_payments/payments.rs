use super::*;

// Cold-order hydrate the proxy forwards on a `refundCreate` against an order it has
// not yet staged. Extracted to a shared `.graphql` so the de-seeded refund capture
// scripts can forward the byte-identical query (`readRequestRaw`) and record a
// cassette that matches this `include_str!` const exactly.
const REFUND_ORDER_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/orders/refund-order-hydrate.graphql");

pub(in crate::proxy) fn refund_user_error(
    field: Value,
    message: impl Into<String>,
    code: &str,
) -> Value {
    let message = message.into();
    user_error(field, &message, Some(code))
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

pub(in crate::proxy) fn order_currency(order: &Value) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_shop_currency)
    .or_else(|| {
        order["transactions"]
            .as_array()
            .and_then(|transactions| transactions.first())
            .and_then(|transaction| money_set_shop_currency(&transaction["amountSet"]))
    })
    .unwrap_or_else(|| "CAD".to_string())
}

pub(in crate::proxy) fn order_presentment_currency(order: &Value, fallback: &str) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_presentment_currency)
    .unwrap_or_else(|| fallback.to_string())
}

pub(in crate::proxy) fn order_transactions(order: &Value) -> Vec<Value> {
    if let Some(transactions) = order["transactions"].as_array() {
        return transactions.clone();
    }
    order["transactions"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(in crate::proxy) fn order_line_items(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["lineItems"].as_array().cloned().unwrap_or_default()
}

pub(in crate::proxy) fn order_shipping_lines(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["shippingLines"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["shippingLines"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(in crate::proxy) fn order_received_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalReceivedSet"]).unwrap_or_else(|| {
        order_transactions(order)
            .iter()
            .filter(|transaction| {
                matches!(transaction["kind"].as_str(), Some("SALE") | Some("CAPTURE"))
                    && transaction["status"].as_str() == Some("SUCCESS")
            })
            .filter_map(|transaction| money_set_amount(&transaction["amountSet"]))
            .sum::<f64>()
    })
}

pub(in crate::proxy) fn order_refunded_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedSet"]).unwrap_or(0.0)
}

pub(in crate::proxy) fn order_refunded_shipping_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedShippingSet"]).unwrap_or(0.0)
}

pub(in crate::proxy) fn order_shipping_refundable_amount(order: &Value) -> f64 {
    order_shipping_lines(order)
        .iter()
        .filter_map(|line| {
            money_set_amount(&line["originalPriceSet"])
                .or_else(|| money_set_amount(&line["priceSet"]))
        })
        .sum()
}

pub(in crate::proxy) fn order_line_item_by_id(order: &Value, line_item_id: &str) -> Option<Value> {
    order_line_items(order)
        .into_iter()
        .find(|line| line["id"].as_str() == Some(line_item_id))
}

pub(in crate::proxy) fn order_transaction_by_id(
    order: &Value,
    transaction_id: &str,
) -> Option<Value> {
    order_transactions(order)
        .into_iter()
        .find(|transaction| transaction["id"].as_str() == Some(transaction_id))
}

pub(in crate::proxy) fn order_line_unit_amount(line: &Value) -> f64 {
    money_set_amount(&line["originalUnitPriceSet"])
        .or_else(|| money_set_amount(&line["priceSet"]))
        .unwrap_or(0.0)
}

pub(in crate::proxy) fn refund_line_item_quantity(input: &BTreeMap<String, ResolvedValue>) -> i64 {
    resolved_int_field(input, "quantity").unwrap_or(1).max(0)
}

pub(in crate::proxy) fn refund_input_transaction_amount(
    input: &BTreeMap<String, ResolvedValue>,
) -> f64 {
    resolved_object_list_field(input, "transactions")
        .iter()
        .filter_map(|transaction| resolved_string_field(transaction, "amount"))
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum()
}

pub(in crate::proxy) fn refund_input_shipping_amount(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> f64 {
    let Some(shipping) = resolved_object_field(input, "shipping") else {
        return 0.0;
    };
    if matches!(shipping.get("fullRefund"), Some(ResolvedValue::Bool(true))) {
        return order_shipping_refundable_amount(order);
    }
    resolved_string_field(&shipping, "amount")
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| resolved_number_field(&shipping, "amount"))
        .unwrap_or(0.0)
}

pub(in crate::proxy) fn refund_input_line_amount(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> f64 {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let quantity = refund_line_item_quantity(line_input);
            resolved_string_field(line_input, "subtotal")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or_else(|| {
                    resolved_string_field(line_input, "lineItemId")
                        .and_then(|id| order_line_item_by_id(order, &id))
                        .map(|line| order_line_unit_amount(&line) * quantity as f64)
                        .unwrap_or(0.0)
                })
        })
        .sum()
}

pub(in crate::proxy) fn refund_input_total_amount(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> f64 {
    let transaction_amount = refund_input_transaction_amount(input);
    if transaction_amount > 0.0 {
        transaction_amount
    } else {
        refund_input_line_amount(input, order) + refund_input_shipping_amount(input, order)
    }
}

pub(in crate::proxy) fn refund_order_with_defaults(mut order: Value) -> Value {
    let shop_currency = order_currency(&order);
    let presentment_currency = order_presentment_currency(&order, &shop_currency);
    if order.get("totalRefundedSet").is_none_or(Value::is_null) {
        order["totalRefundedSet"] =
            money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if order
        .get("totalRefundedShippingSet")
        .is_none_or(Value::is_null)
    {
        order["totalRefundedShippingSet"] =
            money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if !order.get("refunds").is_some_and(Value::is_array) {
        order["refunds"] = json!([]);
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    if !order.get("transactions").is_some_and(Value::is_array) {
        order["transactions"] = json!(order_transactions(&order));
    }
    order
}

pub(in crate::proxy) fn refund_order_payload(order: Option<Value>) -> Value {
    order.map(refund_order_with_defaults).unwrap_or(Value::Null)
}

pub(in crate::proxy) fn refund_validation_payload(
    field: &RootFieldSelection,
    refund: Value,
    order: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            "refund": refund,
            "order": refund_order_payload(order),
            "userErrors": user_errors
        }),
        &field.selection,
    )
}

pub(in crate::proxy) fn refund_input_error(
    field: &RootFieldSelection,
    order: Option<Value>,
    user_error: Value,
) -> Value {
    refund_validation_payload(field, Value::Null, order, vec![user_error])
}

pub(in crate::proxy) fn refund_transaction_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let has_identifiable_parent_transactions = order_transactions(order)
        .iter()
        .any(|transaction| transaction["id"].as_str().is_some_and(|id| !id.is_empty()));
    for transaction in resolved_object_list_field(input, "transactions") {
        let kind =
            resolved_string_field(&transaction, "kind").unwrap_or_else(|| "REFUND".to_string());
        if !kind.eq_ignore_ascii_case("REFUND") {
            return Some(refund_user_error(
                Value::Null,
                format!(
                    "Kind {} is not a valid transaction",
                    kind.to_ascii_lowercase()
                ),
                "INVALID",
            ));
        }
        let parent_id = resolved_string_field(&transaction, "parentId").unwrap_or_default();
        if (parent_id.is_empty() && has_identifiable_parent_transactions)
            || (!parent_id.is_empty() && order_transaction_by_id(order, &parent_id).is_none())
        {
            return Some(refund_user_error(
                json!(["transactions"]),
                "Transactions require a parent_id associated with the order",
                "INVALID",
            ));
        }
    }
    None
}

pub(in crate::proxy) fn refund_quantity_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    for (index, line_input) in resolved_object_list_field(input, "refundLineItems")
        .iter()
        .enumerate()
    {
        let Some(line_item_id) = resolved_string_field(line_input, "lineItemId") else {
            continue;
        };
        let Some(line) = order_line_item_by_id(order, &line_item_id) else {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "lineItemId"]),
                "Line item does not exist",
                "NOT_FOUND",
            ));
        };
        let quantity = refund_line_item_quantity(line_input);
        let refundable_quantity = line["currentQuantity"]
            .as_i64()
            .or_else(|| line["quantity"].as_i64())
            .unwrap_or(0);
        if quantity > refundable_quantity {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "quantity"]),
                "Quantity cannot refund more items than were purchased",
                "INVALID",
            ));
        }
    }
    None
}

pub(in crate::proxy) fn refund_amount_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let refund_amount = refund_input_total_amount(input, order);
    let refundable = (order_received_amount(order) - order_refunded_amount(order)).max(0.0);
    if refund_amount > refundable + 0.005 {
        return Some(refund_user_error(
            Value::Null,
            format!(
                "Refund amount ${:.2} is greater than net payment received ${:.2}",
                refund_amount, refundable
            ),
            "OVER_REFUND",
        ));
    }
    None
}

pub(in crate::proxy) fn next_refund_transaction_id(order: &Value, next: u64) -> (String, u64) {
    let highest = order_transactions(order)
        .iter()
        .filter_map(|transaction| transaction["id"].as_str())
        .map(resource_id_path_tail)
        .filter_map(|tail| tail.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let number = next.max(highest + 1);
    (shopify_gid("OrderTransaction", number), number + 1)
}

pub(in crate::proxy) fn build_refund_line_items(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    shop_currency: &str,
    presentment_currency: &str,
    next_refund_line_item_id: &mut u64,
) -> Vec<Value> {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let id = format!("gid://shopify/RefundLineItem/{}", *next_refund_line_item_id);
            *next_refund_line_item_id += 1;
            let quantity = refund_line_item_quantity(line_input);
            let restock_type = resolved_string_field(line_input, "restockType")
                .unwrap_or_else(|| "NO_RESTOCK".to_string());
            let line_item_id = resolved_string_field(line_input, "lineItemId").unwrap_or_default();
            let line = order_line_item_by_id(order, &line_item_id).unwrap_or(Value::Null);
            let subtotal = order_line_unit_amount(&line) * quantity as f64;
            json!({
                "id": id,
                "quantity": quantity,
                "restockType": restock_type,
                "restocked": restock_type != "NO_RESTOCK",
                "lineItem": {
                    "id": if line_item_id.is_empty() { Value::Null } else { json!(line_item_id) },
                    "title": line["title"].clone()
                },
                "subtotalSet": money_bag_from_amount(subtotal, shop_currency, presentment_currency)
            })
        })
        .collect()
}

pub(in crate::proxy) fn build_refund_transactions(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
    transaction_id: &str,
) -> Vec<Value> {
    let inputs = resolved_object_list_field(input, "transactions");
    if inputs.is_empty() {
        return vec![json!({
            "id": transaction_id,
            "kind": "REFUND",
            "status": "SUCCESS",
            "gateway": "manual",
            "amountSet": money_bag_from_amount(refund_amount, shop_currency, presentment_currency)
        })];
    }
    inputs
        .iter()
        .enumerate()
        .map(|(index, transaction)| {
            let amount = resolved_string_field(transaction, "amount")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(refund_amount);
            let parent = resolved_string_field(transaction, "parentId")
                .and_then(|id| order_transaction_by_id(order, &id));
            let gateway = parent
                .as_ref()
                .and_then(|transaction| transaction["gateway"].as_str().map(str::to_string))
                .or_else(|| resolved_string_field(transaction, "gateway"))
                .unwrap_or_else(|| "manual".to_string());
            let id = if index == 0 {
                transaction_id.to_string()
            } else {
                format!("{transaction_id}-{index}")
            };
            json!({
                "id": id,
                "kind": "REFUND",
                "status": "SUCCESS",
                "gateway": gateway,
                "amountSet": money_bag_from_amount(amount, shop_currency, presentment_currency)
            })
        })
        .collect()
}

pub(in crate::proxy) fn update_order_after_refund(
    mut order: Value,
    refund: &Value,
    refund_transactions: &[Value],
    refund_amount: f64,
    shipping_refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    order = refund_order_with_defaults(order);
    let total_refunded = order_refunded_amount(&order) + refund_amount;
    let total_refunded_shipping = order_refunded_shipping_amount(&order) + shipping_refund_amount;
    let received = order_received_amount(&order);
    order["totalRefundedSet"] =
        money_bag_from_amount(total_refunded, shop_currency, presentment_currency);
    order["totalRefundedShippingSet"] =
        money_bag_from_amount(total_refunded_shipping, shop_currency, presentment_currency);
    order["displayFinancialStatus"] = if total_refunded + 0.005 >= received && received > 0.0 {
        json!("REFUNDED")
    } else {
        json!("PARTIALLY_REFUNDED")
    };
    if let Some(refunds) = order["refunds"].as_array_mut() {
        refunds.push(refund.clone());
    }
    if let Some(transactions) = order["transactions"].as_array_mut() {
        transactions.extend(refund_transactions.iter().cloned());
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    order
}

pub(in crate::proxy) fn payment_money_amount(money_set: &Value, money_key: &str) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("amount"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::proxy) fn payment_money_currency(
    money_set: &Value,
    money_key: &str,
) -> Option<String> {
    money_set
        .get(money_key)
        .and_then(|money| money.get("currencyCode"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::proxy) fn payment_money_set_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let amount_set = resolved_object_field(input, "amountSet")?;
    let shop_money = resolved_object_field(&amount_set, "shopMoney")?;
    let shop_amount = resolved_string_field(&shop_money, "amount")
        .map(|amount| normalized_order_payment_amount(Some(amount)))?;
    let shop_currency = resolved_string_field(&shop_money, "currencyCode")?;
    if let Some(presentment_money) = resolved_object_field(&amount_set, "presentmentMoney") {
        let presentment_amount = resolved_string_field(&presentment_money, "amount")
            .map(|amount| normalized_order_payment_amount(Some(amount)))
            .unwrap_or_else(|| shop_amount.clone());
        let presentment_currency = resolved_string_field(&presentment_money, "currencyCode")
            .unwrap_or_else(|| {
                resolved_string_field(input, "currency").unwrap_or_else(|| shop_currency.clone())
            });
        Some(money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        ))
    } else {
        Some(money_set(&shop_amount, &shop_currency))
    }
}

pub(in crate::proxy) fn payment_money_set_value(amount_set: Value) -> Value {
    let shop_amount =
        payment_money_amount(&amount_set, "shopMoney").unwrap_or_else(|| "0.0".to_string());
    let shop_currency =
        payment_money_currency(&amount_set, "shopMoney").unwrap_or_else(|| "CAD".to_string());
    if amount_set.get("presentmentMoney").is_some() {
        let presentment_amount = payment_money_amount(&amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_amount.clone());
        let presentment_currency = payment_money_currency(&amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_currency.clone());
        money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        )
    } else {
        money_set(&shop_amount, &shop_currency)
    }
}

pub(in crate::proxy) fn payment_money_set_for_capture(
    parent_amount_set: &Value,
    requested_amount: &str,
    requested_currency: &str,
) -> Value {
    let shop_currency = payment_money_currency(parent_amount_set, "shopMoney")
        .unwrap_or_else(|| requested_currency.to_string());
    let parent_shop_amount = payment_money_amount(parent_amount_set, "shopMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let parent_presentment_amount = payment_money_amount(parent_amount_set, "presentmentMoney")
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(parent_shop_amount);
    let requested = requested_amount.parse::<f64>().unwrap_or(0.0);
    let shop_amount = if requested_currency == shop_currency {
        requested
    } else if parent_presentment_amount > 0.0 {
        requested * parent_shop_amount / parent_presentment_amount
    } else {
        requested
    };
    let shop_amount = format_money_amount(shop_amount);
    if parent_amount_set.get("presentmentMoney").is_some() || requested_currency != shop_currency {
        money_set_pair(
            &shop_amount,
            &shop_currency,
            &normalized_order_payment_amount(Some(requested_amount.to_string())),
            requested_currency,
        )
    } else {
        money_set(
            &normalized_order_payment_amount(Some(requested_amount.to_string())),
            requested_currency,
        )
    }
}

pub(in crate::proxy) fn payment_money_set_for_order_totals(
    parent_amount_set: &Value,
    remaining_amount: f64,
    received_amount: f64,
) -> (Value, Value, Value) {
    let shop_currency =
        payment_money_currency(parent_amount_set, "shopMoney").unwrap_or_else(|| "CAD".to_string());
    if parent_amount_set.get("presentmentMoney").is_some() {
        let presentment_currency = payment_money_currency(parent_amount_set, "presentmentMoney")
            .unwrap_or_else(|| shop_currency.clone());
        (
            money_set_pair(
                &format_money_amount(remaining_amount),
                &shop_currency,
                &format_money_amount(remaining_amount),
                &presentment_currency,
            ),
            money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency),
            money_set_pair(
                &format_money_amount(received_amount),
                &shop_currency,
                &format_money_amount(received_amount),
                &presentment_currency,
            ),
        )
    } else {
        (
            money_set(&format_money_amount(remaining_amount), &shop_currency),
            money_set(&format_money_amount(remaining_amount), &shop_currency),
            money_set(&format_money_amount(received_amount), &shop_currency),
        )
    }
}

pub(in crate::proxy) fn payment_transaction_record_from_amount_set(
    id: &str,
    kind: &str,
    status: &str,
    amount_set: Value,
    parent_transaction: Value,
) -> Value {
    let transaction_number = id
        .parse::<u64>()
        .ok()
        .or_else(|| resource_id_path_tail(id).parse::<u64>().ok());
    let payment_id = match (kind, transaction_number) {
        ("AUTHORIZATION", _) => Value::Null,
        (_, Some(number)) => json!(shopify_gid("Payment", number + 1)),
        _ => Value::Null,
    };
    let payment_reference_id = match (kind, transaction_number) {
        ("CAPTURE", Some(number)) if number > 0 => {
            json!(shopify_gid("PaymentReference", number - 1))
        }
        _ => Value::Null,
    };
    json!({
        "id": id,
        "kind": kind,
        "status": status,
        "gateway": "manual",
        "paymentId": payment_id,
        "paymentReferenceId": payment_reference_id,
        "parentTransaction": parent_transaction,
        "amountSet": payment_money_set_value(amount_set)
    })
}

pub(in crate::proxy) fn payment_transaction_public_parent(transaction: &Value) -> Value {
    json!({
        "id": transaction.get("id").cloned().unwrap_or(Value::Null),
        "kind": transaction.get("kind").cloned().unwrap_or(Value::Null),
        "status": transaction.get("status").cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn payment_transaction_matches_parent(
    transaction: &Value,
    parent_id: &str,
) -> bool {
    transaction
        .get("parentTransaction")
        .and_then(|parent| parent.get("id"))
        .and_then(Value::as_str)
        == Some(parent_id)
}

pub(in crate::proxy) fn payment_user_error(
    field: Value,
    message: &str,
    code: Option<&str>,
) -> Value {
    user_error_omit_code(field, message, code)
}

pub(in crate::proxy) fn payment_order_record(
    id: &str,
    display_financial_status: &str,
    capturable_amount: &str,
    outstanding_amount: &str,
    received_amount: &str,
    currency_code: &str,
    transactions: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "displayFinancialStatus": display_financial_status,
        "capturable": capturable_amount != "0.00",
        "totalCapturable": capturable_amount,
        "totalCapturableSet": money_set(capturable_amount, currency_code),
        "totalOutstandingSet": money_set(outstanding_amount, currency_code),
        "totalReceivedSet": money_set(received_amount, currency_code),
        "netPaymentSet": money_set(received_amount, currency_code),
        "paymentGatewayNames": ["manual"],
        "transactions": transactions
    })
}

pub(in crate::proxy) fn normalized_order_payment_amount(value: Option<String>) -> String {
    let value = value.unwrap_or_else(|| "25.00".to_string());
    // Shopify renders money amounts with trailing zeros trimmed to a single
    // decimal place (e.g. "31.90" -> "31.9", "25.00" -> "25.0"). Reformat any
    // parseable amount through the canonical money formatter; leave non-numeric
    // values (e.g. already-symbolic) untouched.
    match value.parse::<f64>() {
        Ok(amount) => format_money_amount(amount),
        Err(_) => value,
    }
}

pub(in crate::proxy) fn mandate_payment_order_record(
    order_id: &str,
    idempotency_key: &str,
    amount: &str,
    currency_code: &str,
    auto_capture: bool,
) -> Value {
    let payment_reference_id = format!("{order_id}/{idempotency_key}");
    let transaction_kind = if auto_capture {
        "SALE"
    } else {
        "AUTHORIZATION"
    };
    let display_financial_status = if auto_capture { "PAID" } else { "AUTHORIZED" };
    let total_capturable = if auto_capture { "0.0" } else { amount };
    let outstanding_amount = if auto_capture { "0.0" } else { amount };
    let received_amount = if auto_capture { amount } else { "0.0" };
    let transaction = json!({
        "id": "gid://shopify/OrderTransaction/4",
        "kind": transaction_kind,
        "status": "SUCCESS",
        "gateway": "mandate",
        "paymentReferenceId": payment_reference_id,
        "amountSet": money_set(amount, currency_code)
    });
    json!({
        "id": order_id,
        "displayFinancialStatus": display_financial_status,
        "capturable": !auto_capture,
        "totalCapturable": total_capturable,
        "totalCapturableSet": money_set(total_capturable, currency_code),
        "totalOutstandingSet": money_set(outstanding_amount, currency_code),
        "totalReceivedSet": money_set(received_amount, currency_code),
        "netPaymentSet": money_set(received_amount, currency_code),
        "paymentGatewayNames": ["mandate"],
        "transactions": [transaction]
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn refund_create_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if root_field != "refundCreate" {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| field.name == "refundCreate") {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let (value, staged_ids) = self.stage_refund_create(request, query, variables, &field);
            if !staged_ids.is_empty() {
                self.record_staged_orders_log_entry(
                    request,
                    query,
                    variables,
                    "refundCreate",
                    staged_ids,
                );
            }
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(super) fn stage_refund_create(
        &mut self,
        request: &Request,
        _query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["input"]), "Input is required", "INVALID"),
                ),
                Vec::new(),
            );
        };
        let Some(order_id) = resolved_string_field(&input, "orderId") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };

        self.hydrate_order_for_refund(request, &order_id);
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };
        let order = refund_order_with_defaults(order);

        if let Some(error) = refund_transaction_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_quantity_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_amount_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }

        let shop_currency = order_currency(&order);
        let presentment_currency = order_presentment_currency(&order, &shop_currency);
        let refund_amount = refund_input_total_amount(&input, &order);
        let shipping_refund_amount = refund_input_shipping_amount(&input, &order);
        let refund_id = shopify_gid("Refund", self.store.staged.next_refund_id);
        self.store.staged.next_refund_id += 1;
        let mut next_line_item_id = self.store.staged.next_refund_line_item_id;
        let refund_line_items = build_refund_line_items(
            &input,
            &order,
            &shop_currency,
            &presentment_currency,
            &mut next_line_item_id,
        );
        self.store.staged.next_refund_line_item_id = next_line_item_id;
        let (transaction_id, next_transaction_id) =
            next_refund_transaction_id(&order, self.store.staged.order_payment_next_transaction_id);
        self.store.staged.order_payment_next_transaction_id = next_transaction_id;
        let refund_transactions = build_refund_transactions(
            &input,
            &order,
            refund_amount,
            &shop_currency,
            &presentment_currency,
            &transaction_id,
        );
        let refund = json!({
            "id": refund_id,
            "note": resolved_string_field(&input, "note"),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "totalRefundedSet": money_bag_from_amount(refund_amount, &shop_currency, &presentment_currency),
            "refundLineItems": order_connection(refund_line_items),
            "transactions": order_connection(refund_transactions.clone())
        });
        let updated_order = update_order_after_refund(
            order,
            &refund,
            &refund_transactions,
            refund_amount,
            shipping_refund_amount,
            &shop_currency,
            &presentment_currency,
        );
        self.store
            .staged
            .orders
            .insert(order_id.clone(), updated_order.clone());

        (
            selected_json(
                &json!({
                    "refund": refund,
                    "order": updated_order,
                    "userErrors": []
                }),
                &field.selection,
            ),
            vec![refund_id, order_id],
        )
    }

    pub(super) fn hydrate_order_for_refund(&mut self, request: &Request, order_id: &str) {
        if self.store.staged.orders.contains_key(order_id)
            || self.config.read_mode == ReadMode::Snapshot
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": REFUND_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": order_id }
            }),
        );
        let order = response.body["data"]["order"].clone();
        if order.is_object() {
            self.store
                .staged
                .orders
                .insert(order_id.to_string(), refund_order_with_defaults(order));
        }
    }

    /// Hydrate the order a `returnCreate` / `returnRequest` runs against when it
    /// was not created locally in this scenario. Forwards
    /// `RETURN_ORDER_HYDRATE_QUERY` verbatim on a cold miss and observes the
    /// order graph (fulfillment line items + any outstanding returns) into staged
    /// state so the return engine validates against real store state rather than a
    /// precondition seed. No-op when the order is already staged or reads are
    /// snapshot-only.
    pub(in crate::proxy) fn hydrate_order_for_return(&mut self, request: &Request, order_id: &str) {
        if order_id.is_empty()
            || self.store.staged.orders.contains_key(order_id)
            || self.config.read_mode == ReadMode::Snapshot
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": RETURN_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersReturnOrderHydrate",
                "variables": { "id": order_id }
            }),
        );
        let order = response.body["data"]["order"].clone();
        if order.is_object() {
            self.store.staged.orders.insert(order_id.to_string(), order);
        }
    }

    pub(in crate::proxy) fn order_payment_transaction_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        match root_field {
            "orderCreate"
                if field
                    .as_ref()
                    .is_some_and(order_create_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let order = self.stage_payment_order(&field);
                let order_id = order["id"].as_str().unwrap_or_default().to_string();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    root_field,
                    vec![order_id],
                );
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "order": order, "userErrors": [] }),
                        &field.selection,
                    ),
                ))
            }
            "orderCapture" => {
                let field = field?;
                let input = resolved_object_field(variables, "input")?;
                let order_id = resolved_string_field(&input, "id")?;
                let outcome = self.stage_payment_capture(&order_id, &input);
                let (transaction, order, user_errors, staged_ids) = match outcome {
                    Some(outcome) => outcome,
                    None => {
                        let order = self
                            .store
                            .staged
                            .orders
                            .get(&order_id)
                            .cloned()
                            .unwrap_or(Value::Null);
                        (
                            Value::Null,
                            order,
                            vec![payment_user_error(
                                Value::Null,
                                "Unable to find parent transaction",
                                None,
                            )],
                            Vec::new(),
                        )
                    }
                };
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "order": order, "userErrors": user_errors }),
                        &field.selection,
                    ),
                ))
            }
            "orderMarkAsPaid" => {
                let field = field?;
                let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
                let order_id = resolved_string_field(&input, "id").unwrap_or_default();
                // Orders not created locally in this scenario are hydrated from the
                // backend so the mutation operates on real money-bag state.
                if !order_id.is_empty() && !self.store.staged.orders.contains_key(&order_id) {
                    self.hydrate_order_for_mark_as_paid(&order_id, request);
                }
                let (order, user_errors, staged_ids) = self.stage_order_mark_as_paid(&order_id);
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "order": order, "userErrors": user_errors }),
                        &field.selection,
                    ),
                ))
            }
            "transactionVoid" => {
                let field = field?;
                let parent_id = resolved_string_field(&field.arguments, "parentTransactionId")
                    .or_else(|| resolved_string_field(variables, "id"))?;
                let (transaction, user_errors, staged_ids) = self.stage_payment_void(&parent_id);
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, query, variables, root_field, staged_ids,
                    );
                }
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "userErrors": user_errors }),
                        &field.selection,
                    ),
                ))
            }
            "order"
                if field
                    .as_ref()
                    .is_some_and(order_read_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let id = resolved_string_field(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orderCreateMandatePayment" => {
                let field = field?;
                if !field.arguments.contains_key("mandateId") {
                    let operation_path = parsed_document(query, variables)
                        .map(|document| document.operation_path)
                        .unwrap_or_else(|| "mutation".to_string());
                    return Some(json!({
                        "errors": [{
                            "message": "Field 'orderCreateMandatePayment' is missing required arguments: mandateId",
                            "locations": [{
                                "line": field.location.line,
                                "column": field.location.column
                            }],
                            "path": [operation_path, "orderCreateMandatePayment"],
                            "extensions": {
                                "code": "missingRequiredArguments",
                                "className": "Field",
                                "name": "orderCreateMandatePayment",
                                "arguments": "mandateId"
                            }
                        }]
                    }));
                }
                let order = resolved_string_field(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .and_then(|id| self.store.staged.orders.get(&id).cloned())
                    .unwrap_or(Value::Null);
                let idempotency_key = resolved_string_field(&field.arguments, "idempotencyKey")
                    .or_else(|| resolved_string_field(variables, "idempotencyKey"));
                let Some(idempotency_key) = idempotency_key else {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "job": Value::Null,
                                "paymentReferenceId": Value::Null,
                                "order": order,
                                "userErrors": [{
                                    "field": ["idempotencyKey"],
                                    "message": "Idempotency key is required"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                };
                let order_id = resolved_string_field(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .unwrap_or_else(|| "gid://shopify/Order/1".to_string());
                let amount_input = resolved_object_field(&field.arguments, "amount")
                    .or_else(|| resolved_object_field(variables, "amount"))
                    .unwrap_or_default();
                let amount =
                    normalized_order_payment_amount(resolved_string_field(&amount_input, "amount"));
                let currency = resolved_string_field(&amount_input, "currencyCode")
                    .unwrap_or_else(|| "CAD".to_string());
                let auto_capture =
                    resolved_bool_field(&field.arguments, "autoCapture").unwrap_or(true);
                let key = format!("{order_id}:{idempotency_key}");
                if !self.store.staged.mandate_payment_keys.contains(&key)
                    || !self.store.staged.orders.contains_key(&order_id)
                {
                    let order = mandate_payment_order_record(
                        &order_id,
                        &idempotency_key,
                        &amount,
                        &currency,
                        auto_capture,
                    );
                    self.store.staged.orders.insert(order_id.clone(), order);
                    self.store.staged.mandate_payment_keys.insert(key);
                }
                let order = self
                    .store
                    .staged
                    .orders
                    .get(&order_id)
                    .cloned()
                    .unwrap_or(Value::Null);
                let payment_reference_id = format!("{order_id}/{idempotency_key}");
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({
                            "job": {
                                "id": "gid://shopify/Job/6",
                                "done": true
                            },
                            "paymentReferenceId": payment_reference_id,
                            "order": order,
                            "userErrors": []
                        }),
                        &field.selection,
                    ),
                ))
            }
            _ => None,
        }
    }

    pub(super) fn stage_payment_order(&mut self, field: &RootFieldSelection) -> Value {
        let id = shopify_gid("Order", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        let currency =
            resolved_string_field(&order_input, "currency").unwrap_or_else(|| "CAD".to_string());
        // Base projection: full order math (line items + taxLines, shipping lines +
        // totalShippingPriceSet, subtotals, taxes, discounts). The payment view is
        // layered on top so a payment-field selection still receives the complete
        // order shape rather than the totals-only subset.
        let mut order = self.build_order_create_record(&id, &order_input);
        let transaction_inputs = resolved_object_list_field(&order_input, "transactions");
        let first_transaction = transaction_inputs.first().cloned().unwrap_or_default();
        let amount_set = payment_money_set_from_input(&first_transaction)
            .unwrap_or_else(|| money_set("25.0", &currency));
        let amount = payment_money_amount(&amount_set, "presentmentMoney")
            .or_else(|| payment_money_amount(&amount_set, "shopMoney"))
            .unwrap_or_else(|| "25.0".to_string());
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let kind = resolved_string_field(&first_transaction, "kind")
            .unwrap_or_else(|| "AUTHORIZATION".to_string());
        let status = resolved_string_field(&first_transaction, "status")
            .unwrap_or_else(|| "SUCCESS".to_string());
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            &kind,
            &status,
            amount_set.clone(),
            Value::Null,
        );
        let (display_status, capturable_amount, outstanding_amount, received_amount) =
            if kind == "AUTHORIZATION" && status == "SUCCESS" {
                ("AUTHORIZED", amount.as_str(), "0.0", "0.0")
            } else if matches!(kind.as_str(), "CAPTURE" | "SALE") && status == "SUCCESS" {
                ("PAID", "0.0", "0.0", amount.as_str())
            } else {
                ("PENDING", "0.0", amount.as_str(), "0.0")
            };
        let payment_view = payment_order_record(
            &id,
            display_status,
            capturable_amount,
            outstanding_amount,
            received_amount,
            payment_money_currency(&amount_set, "presentmentMoney")
                .or_else(|| payment_money_currency(&amount_set, "shopMoney"))
                .as_deref()
                .unwrap_or(&currency),
            vec![transaction],
        );
        // Override the payment-derived projection onto the full order base.
        for key in [
            "displayFinancialStatus",
            "capturable",
            "totalCapturable",
            "totalCapturableSet",
            "totalOutstandingSet",
            "totalReceivedSet",
            "netPaymentSet",
            "paymentGatewayNames",
            "transactions",
        ] {
            if let Some(value) = payment_view.get(key) {
                order[key] = value.clone();
            }
        }
        if amount_set.get("presentmentMoney").is_some() {
            let captured_amount = if capturable_amount == "0.0" {
                amount.as_str()
            } else {
                "0.0"
            };
            let (capturable_set, outstanding_set, received_set) =
                payment_money_set_for_order_totals(
                    &amount_set,
                    capturable_amount.parse::<f64>().unwrap_or(0.0),
                    captured_amount.parse::<f64>().unwrap_or(0.0),
                );
            order["totalCapturableSet"] = capturable_set;
            order["totalOutstandingSet"] = outstanding_set;
            order["totalReceivedSet"] = received_set.clone();
            order["netPaymentSet"] = received_set;
        }
        self.store.staged.orders.insert(id, order.clone());
        order
    }

    pub(super) fn stage_order_mark_as_paid(
        &mut self,
        order_id: &str,
    ) -> (Value, Vec<Value>, Vec<String>) {
        let Some(order_before) = self.store.staged.orders.get(order_id).cloned() else {
            return (
                Value::Null,
                vec![order_mark_as_paid_not_found_error()],
                Vec::new(),
            );
        };
        let outstanding_set = order_money_set_with_presentment_fallback(
            &order_before["totalOutstandingSet"],
            &order_before,
        );
        if order_before["cancelledAt"].is_string()
            || matches!(
                order_before["displayFinancialStatus"].as_str(),
                Some("PAID" | "REFUNDED" | "PARTIALLY_REFUNDED" | "VOIDED")
            )
            || order_money_amount_value(&outstanding_set) <= 0.000_001
        {
            return (
                order_before,
                vec![order_mark_as_paid_cannot_mark_error()],
                Vec::new(),
            );
        }

        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "SALE",
            "SUCCESS",
            outstanding_set.clone(),
            Value::Null,
        );

        let mut order = order_before;
        if let Some(transactions) = order["transactions"].as_array_mut() {
            transactions.push(transaction.clone());
        } else {
            order["transactions"] = json!([transaction.clone()]);
        }
        order["displayFinancialStatus"] = json!("PAID");
        order["capturable"] = json!(false);
        order["totalCapturable"] = json!("0.0");
        order["totalCapturableSet"] = zero_order_money_set_like(&outstanding_set, &order);
        order["totalOutstandingSet"] = zero_order_money_set_like(&outstanding_set, &order);
        let received_set =
            add_order_money_sets(&order["totalReceivedSet"], &outstanding_set, &order);
        order["totalReceivedSet"] = received_set.clone();
        order["netPaymentSet"] = received_set;
        order["paymentGatewayNames"] = json!(["manual"]);

        self.store
            .staged
            .orders
            .insert(order_id.to_string(), order.clone());
        if let Some(customer_id) = order_customer_id(&order) {
            if let Some(customer_orders) = self.store.staged.customer_orders.get_mut(&customer_id) {
                for customer_order in customer_orders {
                    if customer_order["id"].as_str() == Some(order_id) {
                        *customer_order = order.clone();
                    }
                }
            }
        }
        (
            order,
            Vec::new(),
            vec![order_id.to_string(), transaction_id],
        )
    }

    pub(super) fn stage_payment_capture(
        &mut self,
        order_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<(Value, Value, Vec<Value>, Vec<String>)> {
        let requested_amount = resolved_string_field(input, "amount")?;
        let requested_amount_normalized =
            normalized_order_payment_amount(Some(requested_amount.clone()));
        let requested_amount_value = requested_amount.parse::<f64>().ok()?;
        let parent_id = resolved_string_field(input, "parentTransactionId");
        let final_capture = matches!(input.get("finalCapture"), Some(ResolvedValue::Bool(true)));
        let order = self.store.staged.orders.get(order_id)?;
        let transactions = order["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let parent_transaction = parent_id
            .as_deref()
            .and_then(|parent_id| {
                transactions
                    .iter()
                    .find(|transaction| transaction["id"].as_str() == Some(parent_id))
                    .cloned()
            })
            .or_else(|| {
                transactions
                    .iter()
                    .find(|transaction| {
                        transaction["kind"].as_str() == Some("AUTHORIZATION")
                            && transaction["status"].as_str() == Some("SUCCESS")
                    })
                    .cloned()
            });
        let Some(parent_transaction) = parent_transaction else {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    Value::Null,
                    "Unable to find parent transaction",
                    None,
                )],
                Vec::new(),
            ));
        };
        let parent_id = parent_transaction["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let parent_amount_set = parent_transaction["amountSet"].clone();
        let expected_currency = payment_money_currency(&parent_amount_set, "presentmentMoney")
            .or_else(|| payment_money_currency(&parent_amount_set, "shopMoney"))
            .unwrap_or_else(|| "CAD".to_string());
        let shop_currency = order["currencyCode"]
            .as_str()
            .map(str::to_string)
            .or_else(|| payment_money_currency(&parent_amount_set, "shopMoney"))
            .unwrap_or_else(|| expected_currency.clone());
        let expected_currency = order["presentmentCurrencyCode"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| payment_money_currency(&parent_amount_set, "presentmentMoney"))
            .unwrap_or(expected_currency);
        let requires_currency = expected_currency != shop_currency;
        let currency = resolved_string_field(input, "currency");
        if (requires_currency || currency.is_some())
            && currency.as_deref() != Some(expected_currency.as_str())
        {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    json!(["currency"]),
                    &format!("Currency Currency must match parent transaction {expected_currency}"),
                    None,
                )],
                Vec::new(),
            ));
        }
        if requested_amount_value <= 0.0 {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    Value::Null,
                    "Amount must be greater than zero for capture transactions",
                    None,
                )],
                Vec::new(),
            ));
        }
        if parent_transaction["kind"].as_str() != Some("AUTHORIZATION")
            || parent_transaction["status"].as_str() != Some("SUCCESS")
        {
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    json!(["parent_transaction_id"]),
                    "Parent transaction must be a successful authorization",
                    Some("INVALID_TRANSACTION_STATE"),
                )],
                Vec::new(),
            ));
        }
        let already_captured: f64 = transactions
            .iter()
            .filter(|transaction| {
                transaction["kind"].as_str() == Some("CAPTURE")
                    && transaction["status"].as_str() == Some("SUCCESS")
                    && payment_transaction_matches_parent(transaction, &parent_id)
            })
            .filter_map(|transaction| {
                payment_money_amount(&transaction["amountSet"], "presentmentMoney")
                    .or_else(|| payment_money_amount(&transaction["amountSet"], "shopMoney"))
                    .and_then(|amount| amount.parse::<f64>().ok())
            })
            .sum();
        let parent_amount = payment_money_amount(&parent_amount_set, "presentmentMoney")
            .or_else(|| payment_money_amount(&parent_amount_set, "shopMoney"))
            .and_then(|amount| amount.parse::<f64>().ok())
            .unwrap_or(0.0);
        let capturable_amount = (parent_amount - already_captured).max(0.0);
        if requested_amount_value > capturable_amount + 0.000_001 {
            let message = if parent_amount_set.get("presentmentMoney").is_some() {
                format!(
                    "Cannot capture more than the authorized {} for this payment.",
                    format_money_amount(capturable_amount)
                )
            } else {
                "Amount exceeds capturable amount".to_string()
            };
            return Some((
                Value::Null,
                order.clone(),
                vec![payment_user_error(
                    if parent_amount_set.get("presentmentMoney").is_some() {
                        Value::Null
                    } else {
                        json!(["amount"])
                    },
                    &message,
                    Some("OVER_CAPTURE"),
                )],
                Vec::new(),
            ));
        }
        let remaining_amount = if final_capture {
            0.0
        } else {
            (capturable_amount - requested_amount_value).max(0.0)
        };
        let total_received = already_captured + requested_amount_value;
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let transaction_amount_set = payment_money_set_for_capture(
            &parent_amount_set,
            &requested_amount_normalized,
            currency.as_deref().unwrap_or(&expected_currency),
        );
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "CAPTURE",
            "SUCCESS",
            transaction_amount_set,
            payment_transaction_public_parent(&parent_transaction),
        );
        let order = self.store.staged.orders.get_mut(order_id)?;
        if let Some(transactions) = order["transactions"].as_array_mut() {
            transactions.push(transaction.clone());
        }
        let (capturable_set, outstanding_set, received_set) = payment_money_set_for_order_totals(
            &parent_amount_set,
            remaining_amount,
            total_received,
        );
        order["displayFinancialStatus"] = if remaining_amount <= 0.000_001 {
            json!("PAID")
        } else {
            json!("PARTIALLY_PAID")
        };
        order["capturable"] = json!(remaining_amount > 0.000_001);
        order["totalCapturable"] = json!(format_money_amount(remaining_amount));
        order["totalCapturableSet"] = capturable_set;
        order["totalOutstandingSet"] = outstanding_set;
        order["totalReceivedSet"] = received_set.clone();
        order["netPaymentSet"] = received_set;
        Some((
            transaction.clone(),
            order.clone(),
            Vec::new(),
            vec![order_id.to_string(), transaction_id],
        ))
    }

    pub(super) fn stage_payment_void(
        &mut self,
        parent_id: &str,
    ) -> (Value, Vec<Value>, Vec<String>) {
        let located = self
            .store
            .staged
            .orders
            .iter()
            .find_map(|(order_id, order)| {
                order["transactions"]
                    .as_array()
                    .and_then(|transactions| {
                        transactions
                            .iter()
                            .find(|transaction| transaction["id"].as_str() == Some(parent_id))
                            .cloned()
                    })
                    .map(|transaction| (order_id.clone(), order.clone(), transaction))
            });
        let Some((order_id, order, parent_transaction)) = located else {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Transaction does not exist",
                    Some("TRANSACTION_NOT_FOUND"),
                )],
                Vec::new(),
            );
        };
        if parent_transaction["kind"].as_str() != Some("AUTHORIZATION")
            || parent_transaction["status"].as_str() != Some("SUCCESS")
        {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Parent transaction must be a successful authorization",
                    Some("AUTH_NOT_SUCCESSFUL"),
                )],
                Vec::new(),
            );
        }
        let transactions = order["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let has_successful_capture = transactions.iter().any(|transaction| {
            transaction["kind"].as_str() == Some("CAPTURE")
                && transaction["status"].as_str() == Some("SUCCESS")
                && payment_transaction_matches_parent(transaction, parent_id)
        });
        let has_successful_void = transactions.iter().any(|transaction| {
            transaction["kind"].as_str() == Some("VOID")
                && transaction["status"].as_str() == Some("SUCCESS")
                && payment_transaction_matches_parent(transaction, parent_id)
        });
        if has_successful_capture || has_successful_void {
            return (
                Value::Null,
                vec![payment_user_error(
                    json!(["parentTransactionId"]),
                    "Parent transaction require a parent_id referring to a voidable transaction",
                    Some("AUTH_NOT_VOIDABLE"),
                )],
                Vec::new(),
            );
        }
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let amount_set = parent_transaction["amountSet"].clone();
        let transaction = payment_transaction_record_from_amount_set(
            &transaction_id,
            "VOID",
            "SUCCESS",
            amount_set.clone(),
            payment_transaction_public_parent(&parent_transaction),
        );
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            let shop_currency = payment_money_currency(&amount_set, "shopMoney")
                .unwrap_or_else(|| "CAD".to_string());
            order["displayFinancialStatus"] = json!("VOIDED");
            order["capturable"] = json!(false);
            order["totalCapturable"] = json!("0.0");
            if amount_set.get("presentmentMoney").is_some() {
                let presentment_currency = payment_money_currency(&amount_set, "presentmentMoney")
                    .unwrap_or_else(|| shop_currency.clone());
                order["totalCapturableSet"] =
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["totalOutstandingSet"] = amount_set.clone();
                order["totalReceivedSet"] =
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
                order["netPaymentSet"] =
                    money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency);
            } else {
                order["totalCapturableSet"] = money_set("0.0", &shop_currency);
                order["totalOutstandingSet"] = amount_set;
                order["totalReceivedSet"] = money_set("0.0", &shop_currency);
                order["netPaymentSet"] = money_set("0.0", &shop_currency);
            }
            if let Some(transactions) = order["transactions"].as_array_mut() {
                transactions.push(transaction.clone());
            }
        }
        (transaction, Vec::new(), vec![order_id, transaction_id])
    }

    pub(in crate::proxy) fn payment_customization_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomization" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    match self.store.staged.payment_customizations.get(&id) {
                        Some(record) => selected_json(record, &field.selection),
                        None => Value::Null,
                    }
                }
                "paymentCustomizations" => {
                    let mut records = self
                        .store
                        .staged
                        .payment_customizations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    payment_customization_connection(&records, &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomizationCreate" => self.payment_customization_create_payload(field),
                "paymentCustomizationUpdate" => self.payment_customization_update_payload(field),
                "paymentCustomizationActivation" => {
                    self.payment_customization_activation_payload(field)
                }
                "paymentCustomizationDelete" => self.payment_customization_delete_payload(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let function_id = resolved_string_field(&input, "functionId");
        let function_handle = resolved_string_field(&input, "functionHandle");
        let mut required_errors = Vec::new();
        if resolved_string_field(&input, "title")
            .map(|title| title.trim().is_empty())
            .unwrap_or(true)
        {
            required_errors.push(payment_customization_required_input_field_error("title"));
        }
        if !input.contains_key("enabled") {
            required_errors.push(payment_customization_required_input_field_error("enabled"));
        }
        if !required_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                required_errors,
                None,
                None,
            );
        }
        if function_id.is_some() && function_handle.is_some() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization"],
                    "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "Only one of function_id or function_handle can be provided, not both.",
                )],
                None,
                None,
            );
        }
        if function_id.is_none() && function_handle.is_none() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization", "functionHandle"],
                    "MISSING_FUNCTION_IDENTIFIER",
                    "Either function_id or function_handle must be provided.",
                )],
                None,
                None,
            );
        }
        if let Some(handle) = function_handle.as_deref() {
            if !payment_customization_function_handle_exists(handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let id = format!(
            "gid://shopify/PaymentCustomization/{}",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let record = payment_customization_record(&id, &input);
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), record.clone());
        payment_customization_payload(Some(&record), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let Some(existing) = self.store.staged.payment_customizations.get(&id).cloned() else {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                None,
            );
        };

        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_required_input_field_error("title")],
                None,
                None,
            );
        }
        if let Some(handle) = resolved_string_field(&input, "functionHandle") {
            if !payment_customization_function_handle_exists(&handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
            if !payment_customization_function_matches(&existing, &handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error(
                        "functionHandle",
                    )],
                    None,
                    None,
                );
            }
        }
        if let Some(function_id) = resolved_string_field(&input, "functionId") {
            if !payment_customization_function_matches(&existing, &function_id) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error("functionId")],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let mut updated = existing;
        if let Some(title) = resolved_string_field(&input, "title") {
            updated["title"] = json!(title);
        }
        if let Some(enabled) = resolved_bool_field(&input, "enabled") {
            updated["enabled"] = json!(enabled);
        }
        if input.contains_key("metafields") {
            let metafields = payment_customization_metafields(&input);
            payment_customization_set_metafields(&mut updated, metafields);
        }
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), updated.clone());
        payment_customization_payload(Some(&updated), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_activation_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let enabled = match field.arguments.get("enabled") {
            Some(ResolvedValue::Bool(value)) => *value,
            _ => false,
        };
        let mut valid_ids = Vec::new();
        let mut missing_ids = Vec::new();
        for id in ids {
            match self.store.staged.payment_customizations.get_mut(&id) {
                Some(record) => {
                    if record["enabled"].as_bool() != Some(enabled) {
                        record["enabled"] = json!(enabled);
                    }
                    valid_ids.push(id);
                }
                None => missing_ids.push(id),
            }
        }
        let errors = if missing_ids.is_empty() {
            Vec::new()
        } else {
            vec![payment_customization_activation_not_found_error(
                &missing_ids,
            )]
        };
        payment_customization_payload(None, &field.selection, errors, Some(valid_ids), None)
    }

    pub(in crate::proxy) fn payment_customization_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self
            .store
            .staged
            .payment_customizations
            .remove(&id)
            .is_some()
        {
            payment_customization_payload(None, &field.selection, Vec::new(), None, Some(json!(id)))
        } else {
            payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                Some(Value::Null),
            )
        }
    }
}

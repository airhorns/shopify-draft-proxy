use super::*;

// ===== Order-edit calculated engine =====
//
// The order-edit mutations (`orderEditBegin` → add/setQuantity/discount/shipping
// → `orderEditCommit` → downstream read) are modelled as a small data-driven
// engine over the seeded store order. `begin` snapshots the order's line items
// into an edit *session* (stored, round-tripped, in
// `order_edit_existing_calculated_order`); each subsequent mutation transforms
// that session; `commit` projects the session back onto the staged order. All
// money totals are recomputed from the session, so the responses are computed
// from store state rather than echoed from the recording. Opaque allocated ids
// (CalculatedOrder / CalculatedLineItem / CalculatedShippingLine / discount
// application) are excluded from parity comparison and only need to be
// internally consistent so a later step can thread one back as an argument.

/// Parse a Money `amount` string (e.g. "29.0", "949.95") into integer cents.
pub(super) fn oe_amount_to_cents(amount: &str) -> i64 {
    let parsed: f64 = amount.trim().parse().unwrap_or(0.0);
    (parsed * 100.0).round() as i64
}

/// Render integer cents the way the Admin API renders a Money `amount`: a
/// decimal with the minimum number of fractional digits but always at least one
/// (1000 -> "10.0", 250 -> "2.5", 94995 -> "949.95").
pub(super) fn oe_format_cents(cents: i64) -> String {
    let negative = cents < 0;
    let magnitude = cents.abs();
    let dollars = magnitude / 100;
    let remainder = magnitude % 100;
    let body = if remainder == 0 {
        format!("{dollars}.0")
    } else if remainder % 10 == 0 {
        format!("{dollars}.{}", remainder / 10)
    } else {
        format!("{dollars}.{remainder:02}")
    };
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

pub(super) fn oe_shop_money(cents: i64, currency: &str) -> Value {
    money_set(&oe_format_cents(cents), currency)
}

pub(super) fn oe_shop_presentment_money(cents: i64, currency: &str) -> Value {
    let amount = oe_format_cents(cents);
    money_set_pair(&amount, currency, &amount, currency)
}

pub(super) fn oe_int(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// Total per-unit discount staged against a session line.
pub(super) fn oe_line_discount_per_unit(line: &Value) -> i64 {
    line.get("discounts")
        .and_then(Value::as_array)
        .map(|discounts| {
            discounts
                .iter()
                .map(|discount| {
                    discount
                        .get("perUnitCents")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Render a session line as a CalculatedLineItem (the requested selection
/// narrows this down, so it always emits the full shape).
pub(super) fn oe_line_view(line: &Value, currency: &str) -> Value {
    let unit = oe_int(line, "unitCents");
    let current_quantity = oe_int(line, "curQty");
    let per_unit_discount = oe_line_discount_per_unit(line);
    let empty = Vec::new();
    let discounts = line
        .get("discounts")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let allocations: Vec<Value> = discounts
        .iter()
        .map(|discount| {
            let per_unit = discount
                .get("perUnitCents")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            json!({
                "allocatedAmountSet": oe_shop_money(per_unit * current_quantity, currency),
                "discountApplication": {
                    "id": discount.get("appId").cloned().unwrap_or(Value::Null),
                    "description": discount.get("description").cloned().unwrap_or(Value::Null)
                }
            })
        })
        .collect();
    json!({
        "id": line.get("calcId").cloned().unwrap_or(Value::Null),
        "title": line.get("title").cloned().unwrap_or(Value::Null),
        "quantity": current_quantity,
        "currentQuantity": current_quantity,
        "sku": line.get("sku").cloned().unwrap_or(Value::Null),
        "variant": line.get("variant").cloned().unwrap_or(Value::Null),
        "originalUnitPriceSet": oe_shop_presentment_money(unit, currency),
        "discountedUnitPriceSet": oe_shop_presentment_money(unit - per_unit_discount, currency),
        "hasStagedLineItemDiscount": !discounts.is_empty(),
        "calculatedDiscountAllocations": allocations
    })
}

pub(super) fn oe_shipping_view(shipping: &Value, currency: &str) -> Value {
    json!({
        "id": shipping.get("id").cloned().unwrap_or(Value::Null),
        "title": shipping.get("title").cloned().unwrap_or(Value::Null),
        "stagedStatus": shipping.get("stagedStatus").cloned().unwrap_or(Value::Null),
        "price": oe_shop_money(oe_int(shipping, "priceCents"), currency)
    })
}

/// (subtotal cents, total cents, total current quantity) over a session.
pub(super) fn oe_session_totals(session: &Value) -> (i64, i64, i64) {
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut subtotal = 0_i64;
    let mut discount = 0_i64;
    let mut quantity = 0_i64;
    for line in lines {
        let current_quantity = oe_int(line, "curQty");
        subtotal += oe_int(line, "unitCents") * current_quantity;
        discount += oe_line_discount_per_unit(line) * current_quantity;
        quantity += current_quantity;
    }
    let shipping: i64 = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().map(|line| oe_int(line, "priceCents")).sum())
        .unwrap_or(0);
    (subtotal, subtotal - discount + shipping, quantity)
}

pub(super) fn oe_session_currency(session: &Value) -> &str {
    session
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("CAD")
}

pub(super) fn oe_calc_order_view(session: &Value) -> Value {
    let currency = oe_session_currency(session);
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut existing = Vec::new();
    let mut added = Vec::new();
    for line in lines {
        let view = oe_line_view(line, currency);
        if line.get("kind").and_then(Value::as_str) == Some("existing") {
            existing.push(view);
        } else {
            added.push(view);
        }
    }
    let shipping: Vec<Value> = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|line| oe_shipping_view(line, currency))
        .collect();
    let (subtotal, total, quantity) = oe_session_totals(session);
    json!({
        "id": session.get("id").cloned().unwrap_or(Value::Null),
        "originalOrder": {
            "id": session.get("originalOrderId").cloned().unwrap_or(Value::Null),
            "name": session.get("originalOrderName").cloned().unwrap_or(Value::Null)
        },
        "lineItems": { "nodes": existing },
        "addedLineItems": { "nodes": added },
        "shippingLines": shipping,
        "subtotalLineItemsQuantity": quantity,
        "subtotalPriceSet": oe_shop_money(subtotal, currency),
        "totalPriceSet": oe_shop_money(total, currency)
    })
}

/// Allocate the next opaque-id sequence number for a session.
pub(super) fn oe_next_seq(session: &mut Value) -> i64 {
    let next = session.get("seq").and_then(Value::as_i64).unwrap_or(0) + 1;
    session["seq"] = json!(next);
    next
}

/// The order's working currency, derived from its line items / totals.
pub(super) fn oe_order_currency(order: &Value) -> String {
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        for node in nodes {
            if let Some(currency) = money_set_shop_currency(&node["originalUnitPriceSet"]) {
                return currency;
            }
        }
    }
    for key in [
        "currentTotalPriceSet",
        "totalPriceSet",
        "currentSubtotalPriceSet",
    ] {
        if let Some(currency) = money_set_shop_currency(&order[key]) {
            return currency;
        }
    }
    "CAD".to_string()
}

/// Snapshot an order's line items into a fresh edit session.
pub(super) fn oe_build_session(order: &Value, calculated_id: &str, session_id: &str) -> Value {
    let currency = oe_order_currency(order);
    let mut lines = Vec::new();
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        for node in nodes {
            let order_line_id = node["id"].as_str().unwrap_or_default();
            let tail = resource_id_tail(order_line_id);
            let unit = (money_set_amount(&node["originalUnitPriceSet"]).unwrap_or(0.0) * 100.0)
                .round() as i64;
            let historical = node["quantity"].as_i64().unwrap_or(0);
            let current = node["currentQuantity"].as_i64().unwrap_or(historical);
            lines.push(json!({
                "calcId": shopify_gid("CalculatedLineItem", tail),
                "orderLineId": node["id"].clone(),
                "kind": "existing",
                "title": node["title"].clone(),
                "sku": node.get("sku").cloned().unwrap_or(Value::Null),
                "variant": node.get("variant").cloned().unwrap_or(Value::Null),
                "unitCents": unit,
                "histQty": historical,
                "curQty": current,
                "discounts": []
            }));
        }
    }
    json!({
        "id": calculated_id,
        "sessionId": session_id,
        "originalOrderId": order["id"].clone(),
        "originalOrderName": order["name"].clone(),
        "currency": currency,
        "seq": 0,
        "lines": lines,
        "shippingLines": []
    })
}

/// Read a MoneyInput object's `amount` as integer cents (accepts string or
/// numeric scalar).
pub(super) fn oe_money_obj_cents(input: &BTreeMap<String, ResolvedValue>) -> Option<i64> {
    resolved_money_amount(input).map(|amount| (amount * 100.0).round() as i64)
}

fn oe_money_set_cents(value: &Value) -> Option<i64> {
    value["presentmentMoney"]["amount"]
        .as_str()
        .or_else(|| value["shopMoney"]["amount"].as_str())
        .or_else(|| value["amount"].as_str())
        .and_then(|amount| amount.parse::<f64>().ok())
        .map(|amount| (amount * 100.0).round() as i64)
}

/// A failed order-edit mutation payload: every resource field is null and the
/// given userErrors are attached. The kitchen-sink shape is narrowed by the
/// caller's field selection, so each mutation emits only the fields it asked
/// for.
pub(super) fn oe_error_payload(errors: Vec<Value>, selection: &[SelectedField]) -> Value {
    let payload = json!({
        "calculatedOrder": Value::Null,
        "calculatedLineItem": Value::Null,
        "calculatedShippingLine": Value::Null,
        "addedDiscountStagedChange": Value::Null,
        "orderEditSession": Value::Null,
        "order": Value::Null,
        "successMessages": [],
        "userErrors": errors
    });
    selected_json(&payload, selection)
}

/// Find a session line index by its allocated CalculatedLineItem id.
pub(super) fn oe_line_index(session: &Value, calc_id: &str) -> Option<usize> {
    session
        .get("lines")
        .and_then(Value::as_array)
        .and_then(|lines| {
            lines
                .iter()
                .position(|line| line.get("calcId").and_then(Value::as_str) == Some(calc_id))
        })
}

/// Find a session shipping-line index by its allocated CalculatedShippingLine
/// id.
pub(super) fn oe_shipping_index(session: &Value, shipping_id: &str) -> Option<usize> {
    session
        .get("shippingLines")
        .and_then(Value::as_array)
        .and_then(|lines| {
            lines
                .iter()
                .position(|line| line.get("id").and_then(Value::as_str) == Some(shipping_id))
        })
}

/// Project an edit session back onto a committed order: existing lines keep
/// their historical `quantity` but adopt the edited `currentQuantity`; added
/// lines are materialised as new line items. Current totals, the edit history
/// event, and per-line fulfillment orders are recomputed from the session.
pub(super) fn oe_commit_order(base: &Value, session: &Value, author: Option<&str>) -> Value {
    let currency = oe_session_currency(session);
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let (subtotal, total, quantity) = oe_session_totals(session);
    let mut line_nodes = Vec::new();
    let mut fulfillment_orders = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let unit = oe_int(line, "unitCents");
        let historical = oe_int(line, "histQty");
        let current = oe_int(line, "curQty");
        let line_id = match line.get("orderLineId").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => shopify_gid("LineItem", format_args!("oe-{index}")),
        };
        line_nodes.push(json!({
            "id": line_id,
            "title": line.get("title").cloned().unwrap_or(Value::Null),
            "quantity": historical,
            "currentQuantity": current,
            "sku": line.get("sku").cloned().unwrap_or(Value::Null),
            "variant": line.get("variant").cloned().unwrap_or(Value::Null),
            "originalUnitPriceSet": oe_shop_money(unit, currency)
        }));
        if current > 0 {
            fulfillment_orders.push(json!({
                "id": shopify_gid("FulfillmentOrder", format_args!("oe-{index}")),
                "status": "OPEN",
                "lineItems": {
                    "nodes": [{
                        "id": shopify_gid(
                            "FulfillmentOrderLineItem",
                            format_args!("oe-{index}"),
                        ),
                        "totalQuantity": current,
                        "remainingQuantity": current,
                        "lineItem": {
                            "id": line_id,
                            "title": line.get("title").cloned().unwrap_or(Value::Null),
                            "quantity": historical,
                            "currentQuantity": current,
                            "fulfillableQuantity": current
                        }
                    }]
                }
            }));
        }
    }
    let message = author.map(|author| format!("{author} edited this order."));
    let base_total = oe_money_set_cents(&base["currentTotalPriceSet"])
        .or_else(|| oe_money_set_cents(&base["totalPriceSet"]))
        .unwrap_or(total);
    let received = oe_money_set_cents(&base["totalReceivedSet"]).unwrap_or_else(|| {
        let outstanding = oe_money_set_cents(&base["totalOutstandingSet"]).unwrap_or(0);
        (base_total - outstanding).max(0)
    });
    let outstanding = total - received;
    let mut committed = if base.is_object() {
        base.clone()
    } else {
        json!({})
    };
    committed["id"] = base.get("id").cloned().unwrap_or(Value::Null);
    committed["name"] = base.get("name").cloned().unwrap_or(Value::Null);
    committed["note"] = base.get("note").cloned().unwrap_or(Value::Null);
    committed["updatedAt"] = base
        .get("updatedAt")
        .cloned()
        .unwrap_or(json!("2026-01-01T00:00:00Z"));
    committed["closed"] = json!(false);
    committed["closedAt"] = Value::Null;
    committed["merchantEditable"] = json!(true);
    committed["merchantEditableErrors"] = json!([]);
    committed["currentSubtotalLineItemsQuantity"] = json!(quantity);
    committed["currentSubtotalPriceSet"] = oe_shop_money(subtotal, currency);
    committed["currentTotalPriceSet"] = oe_shop_money(total, currency);
    committed["totalOutstandingSet"] = oe_shop_presentment_money(outstanding, currency);
    committed["currentTaxLines"] = json!([]);
    committed["lineItems"] = json!({ "nodes": line_nodes });
    committed["events"] = json!({
        "nodes": [{
            "id": "gid://shopify/BasicEvent/oe-edited",
            "action": "edited",
            "message": message.map(Value::String).unwrap_or(Value::Null),
            "createdAt": "2026-01-01T00:00:00Z"
        }]
    });
    committed["fulfillmentOrders"] = json!({ "nodes": fulfillment_orders });
    committed
}

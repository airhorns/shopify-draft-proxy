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

fn oe_line_net_subtotal_cents(line: &Value, quantity_key: &str) -> i64 {
    let quantity = oe_int(line, quantity_key);
    (oe_int(line, "unitCents") - oe_line_discount_per_unit(line)) * quantity
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

#[derive(Debug, Clone, Copy)]
pub(super) struct OeSessionTotals {
    pub subtotal: i64,
    pub tax: i64,
    pub shipping: i64,
    pub total: i64,
    pub quantity: i64,
}

fn oe_tax_line_cents(line: &Value, tax_line: &Value, quantity_key: &str) -> i64 {
    let subtotal = oe_line_net_subtotal_cents(line, quantity_key);
    if subtotal == 0 {
        return 0;
    }
    if let Some(rate) = tax_line.get("rate").and_then(Value::as_f64) {
        return ((subtotal as f64) * rate).round() as i64;
    }
    let base_tax = oe_int(tax_line, "__draftProxyBasePriceCents");
    let base_taxable = oe_int(tax_line, "__draftProxyBaseTaxableCents");
    if base_taxable == 0 {
        return 0;
    }
    ((base_tax as f64) * (subtotal as f64) / (base_taxable as f64)).round() as i64
}

fn oe_public_tax_line(tax_line: &Value, cents: i64, currency: &str) -> Value {
    let mut output = tax_line.clone();
    if let Some(object) = output.as_object_mut() {
        object.remove("__draftProxyBasePriceCents");
        object.remove("__draftProxyBaseTaxableCents");
    }
    output["priceSet"] = oe_shop_presentment_money(cents, currency);
    output
}

fn oe_line_tax_lines(line: &Value, currency: &str, quantity_key: &str) -> Vec<Value> {
    let empty = Vec::new();
    line.get("taxLines")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|tax_line| {
            let cents = oe_tax_line_cents(line, tax_line, quantity_key);
            (cents != 0).then(|| oe_public_tax_line(tax_line, cents, currency))
        })
        .collect()
}

fn oe_session_tax_lines(session: &Value, quantity_key: &str) -> Vec<Value> {
    let currency = oe_session_currency(session);
    let empty = Vec::new();
    let mut aggregated: BTreeMap<String, (Value, i64)> = BTreeMap::new();
    for line in session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        for tax_line in line
            .get("taxLines")
            .and_then(Value::as_array)
            .unwrap_or(&empty)
        {
            let cents = oe_tax_line_cents(line, tax_line, quantity_key);
            if cents == 0 {
                continue;
            }
            let title = tax_line
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let rate = tax_line
                .get("rate")
                .map(Value::to_string)
                .unwrap_or_default();
            let key = format!("{title}\u{1f}{rate}\u{1f}{currency}");
            aggregated
                .entry(key)
                .and_modify(|(_, total)| *total += cents)
                .or_insert_with(|| (tax_line.clone(), cents));
        }
    }
    aggregated
        .into_values()
        .map(|(tax_line, cents)| oe_public_tax_line(&tax_line, cents, currency))
        .collect()
}

/// Money totals over a session. `subtotal` is net of staged line-item discounts;
/// `total` includes tax, shipping, and cart-level discounts captured from the
/// base order.
fn oe_session_totals_for_quantity(session: &Value, quantity_key: &str) -> OeSessionTotals {
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut subtotal = 0_i64;
    let mut tax = 0_i64;
    let mut quantity = 0_i64;
    for line in lines {
        let line_quantity = oe_int(line, quantity_key);
        subtotal += oe_line_net_subtotal_cents(line, quantity_key);
        tax += line
            .get("taxLines")
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .iter()
            .map(|tax_line| oe_tax_line_cents(line, tax_line, quantity_key))
            .sum::<i64>();
        quantity += line_quantity;
    }
    let shipping: i64 = session
        .get("shippingLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().map(|line| oe_int(line, "priceCents")).sum())
        .unwrap_or(0);
    let shipping = shipping + oe_int(session, "baseShippingCents");
    let cart_discount = oe_int(session, "cartDiscountCents");
    let total = (subtotal + tax + shipping - cart_discount).max(0);
    OeSessionTotals {
        subtotal,
        tax,
        shipping,
        total,
        quantity,
    }
}

pub(super) fn oe_session_totals(session: &Value) -> OeSessionTotals {
    oe_session_totals_for_quantity(session, "curQty")
}

fn oe_session_historical_totals(session: &Value) -> OeSessionTotals {
    oe_session_totals_for_quantity(session, "histQty")
}

fn oe_session_original_total(session: &Value) -> i64 {
    let empty = Vec::new();
    session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|line| oe_int(line, "unitCents") * oe_int(line, "histQty"))
        .sum()
}

pub(super) fn oe_session_currency(session: &Value) -> &str {
    session
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("CAD")
}

pub(super) fn oe_session_has_changes(session: &Value) -> bool {
    let lines_changed = session
        .get("lines")
        .and_then(Value::as_array)
        .is_some_and(|lines| {
            lines.iter().any(|line| {
                line.get("kind").and_then(Value::as_str) != Some("existing")
                    || oe_int(line, "curQty") != oe_int(line, "histQty")
                    || line
                        .get("discounts")
                        .and_then(Value::as_array)
                        .is_some_and(|discounts| !discounts.is_empty())
            })
        });
    lines_changed
        || session
            .get("shippingLines")
            .and_then(Value::as_array)
            .is_some_and(|lines| !lines.is_empty())
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
    let totals = oe_session_totals(session);
    let total_price_set = if oe_session_has_changes(session) {
        oe_shop_presentment_money(totals.total, currency)
    } else {
        session
            .get("originalTotalPriceSet")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| oe_shop_presentment_money(totals.total, currency))
    };
    json!({
        "id": session.get("id").cloned().unwrap_or(Value::Null),
        "originalOrder": {
            "id": session.get("originalOrderId").cloned().unwrap_or(Value::Null),
            "name": session.get("originalOrderName").cloned().unwrap_or(Value::Null)
        },
        "lineItems": { "nodes": existing },
        "addedLineItems": { "nodes": added },
        "shippingLines": shipping,
        "subtotalLineItemsQuantity": totals.quantity,
        "subtotalPriceSet": oe_shop_presentment_money(totals.subtotal, currency),
        "totalPriceSet": total_price_set
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
            let tax_lines = node
                .get("taxLines")
                .and_then(Value::as_array)
                .map(|tax_lines| {
                    tax_lines
                        .iter()
                        .map(|tax_line| {
                            let mut tax_line = tax_line.clone();
                            tax_line["__draftProxyBasePriceCents"] =
                                json!(oe_money_set_cents(&tax_line["priceSet"]).unwrap_or(0));
                            tax_line["__draftProxyBaseTaxableCents"] = json!(unit * historical);
                            tax_line
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
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
                "taxLines": tax_lines,
                "discounts": []
            }));
        }
    }
    json!({
        "id": calculated_id,
        "sessionId": session_id,
        "originalOrderId": order["id"].clone(),
        "originalOrderName": order["name"].clone(),
        "originalTotalPriceSet": order["totalPriceSet"].clone(),
        "currency": currency,
        "seq": 0,
        "lines": lines,
        "baseShippingCents": oe_money_set_cents(&order["totalShippingPriceSet"]).unwrap_or(0),
        "cartDiscountCents": oe_money_set_cents(&order["currentTotalDiscountsSet"])
            .or_else(|| oe_money_set_cents(&order["totalDiscountsSet"]))
            .unwrap_or(0),
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
pub(super) fn oe_error_payload(errors: Vec<Value>) -> Value {
    json!({
        "calculatedOrder": Value::Null,
        "calculatedLineItem": Value::Null,
        "calculatedShippingLine": Value::Null,
        "addedDiscountStagedChange": Value::Null,
        "orderEditSession": Value::Null,
        "order": Value::Null,
        "successMessages": [],
        "userErrors": errors
    })
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

fn oe_transaction_received_cents(base: &Value) -> Option<i64> {
    base["transactions"].as_array().map(|transactions| {
        transactions
            .iter()
            .filter(|transaction| {
                matches!(transaction["kind"].as_str(), Some("SALE" | "CAPTURE"))
                    && transaction["status"].as_str() == Some("SUCCESS")
            })
            .filter_map(|transaction| oe_money_set_cents(&transaction["amountSet"]))
            .sum()
    })
}

fn oe_order_received_cents(base: &Value, base_total: i64) -> i64 {
    oe_money_set_cents(&base["totalReceivedSet"])
        .or_else(|| oe_transaction_received_cents(base))
        .or_else(|| {
            oe_money_set_cents(&base["totalOutstandingSet"])
                .map(|outstanding| (base_total - outstanding).max(0))
        })
        .unwrap_or_else(|| match base["displayFinancialStatus"].as_str() {
            Some("PENDING" | "AUTHORIZED") => 0,
            _ => base_total,
        })
}

fn oe_display_financial_status(base: &Value, received: i64, outstanding: i64) -> String {
    let current = base["displayFinancialStatus"].as_str();
    if matches!(current, Some("REFUNDED" | "PARTIALLY_REFUNDED" | "VOIDED")) {
        return current.unwrap_or_default().to_string();
    }
    if outstanding > 0 {
        if received > 0 {
            "PARTIALLY_PAID".to_string()
        } else if current == Some("AUTHORIZED") {
            "AUTHORIZED".to_string()
        } else {
            "PENDING".to_string()
        }
    } else if current == Some("AUTHORIZED") && received <= 0 {
        "AUTHORIZED".to_string()
    } else {
        "PAID".to_string()
    }
}

fn oe_fulfillment_quantity_for_line(base_fulfilled: bool, line: &Value) -> i64 {
    let current = oe_int(line, "curQty");
    if !base_fulfilled {
        return current;
    }
    if line.get("kind").and_then(Value::as_str) == Some("existing") {
        (current - oe_int(line, "histQty")).max(0)
    } else {
        current
    }
}

fn oe_has_new_fulfillment_demand(base_fulfilled: bool, lines: &[Value]) -> bool {
    lines
        .iter()
        .any(|line| oe_fulfillment_quantity_for_line(base_fulfilled, line) > 0)
}

fn oe_display_fulfillment_status(base: &Value, session: &Value, quantity: i64) -> String {
    if quantity <= 0 {
        return "FULFILLED".to_string();
    }
    let current = base["displayFulfillmentStatus"].as_str();
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let base_fulfilled = current == Some("FULFILLED");
    if base_fulfilled && oe_has_new_fulfillment_demand(base_fulfilled, lines) {
        "PARTIALLY_FULFILLED".to_string()
    } else {
        current.unwrap_or("UNFULFILLED").to_string()
    }
}

/// Project an edit session back onto a committed order: existing lines keep
/// their historical `quantity` but adopt the edited `currentQuantity`; added
/// lines are materialised as new line items. Current totals, derived display
/// statuses, the edit history event, and per-line fulfillment orders are
/// recomputed from the session.
pub(super) fn oe_commit_order(base: &Value, session: &Value, author: Option<&str>) -> Value {
    let currency = oe_session_currency(session);
    let empty = Vec::new();
    let lines = session
        .get("lines")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let current_totals = oe_session_totals(session);
    let historical_totals = oe_session_historical_totals(session);
    let mut line_nodes = Vec::new();
    let mut fulfillment_orders = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let unit = oe_int(line, "unitCents");
        let historical = oe_int(line, "histQty");
        let current = oe_int(line, "curQty");
        let per_unit_discount = oe_line_discount_per_unit(line);
        let line_tax_lines = oe_line_tax_lines(line, currency, "histQty");
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
            "originalUnitPriceSet": oe_shop_money(unit, currency),
            "discountedUnitPriceSet": oe_shop_money(unit - per_unit_discount, currency),
            "taxLines": line_tax_lines
        }));
        let fulfillment_quantity = oe_fulfillment_quantity_for_line(
            base["displayFulfillmentStatus"].as_str() == Some("FULFILLED"),
            line,
        );
        if fulfillment_quantity > 0 {
            fulfillment_orders.push(json!({
                "id": shopify_gid("FulfillmentOrder", format_args!("oe-{index}")),
                "status": "OPEN",
                "lineItems": {
                    "nodes": [{
                        "id": shopify_gid(
                            "FulfillmentOrderLineItem",
                            format_args!("oe-{index}"),
                        ),
                        "totalQuantity": fulfillment_quantity,
                        "remainingQuantity": fulfillment_quantity,
                        "lineItem": {
                            "id": line_id,
                            "title": line.get("title").cloned().unwrap_or(Value::Null),
                            "quantity": historical,
                            "currentQuantity": current,
                            "fulfillableQuantity": fulfillment_quantity
                        }
                    }]
                }
            }));
        }
    }
    let message = author
        .map(|author| format!("{author} edited this order."))
        .unwrap_or_default();
    let base_total = oe_money_set_cents(&base["currentTotalPriceSet"])
        .or_else(|| oe_money_set_cents(&base["totalPriceSet"]))
        .unwrap_or_else(|| oe_session_original_total(session));
    let received = oe_order_received_cents(base, base_total);
    let outstanding = (current_totals.total - received).max(0);
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
    committed["currentSubtotalLineItemsQuantity"] = json!(current_totals.quantity);
    committed["displayFinancialStatus"] =
        json!(oe_display_financial_status(base, received, outstanding));
    committed["displayFulfillmentStatus"] = json!(oe_display_fulfillment_status(
        base,
        session,
        current_totals.quantity
    ));
    committed["currentSubtotalPriceSet"] =
        oe_shop_presentment_money(current_totals.subtotal, currency);
    committed["subtotalPriceSet"] = oe_shop_presentment_money(historical_totals.subtotal, currency);
    committed["currentTotalPriceSet"] = oe_shop_presentment_money(current_totals.total, currency);
    committed["totalPriceSet"] = oe_shop_presentment_money(historical_totals.total, currency);
    committed["currentTotalTaxSet"] = oe_shop_presentment_money(current_totals.tax, currency);
    committed["totalTaxSet"] = oe_shop_presentment_money(historical_totals.tax, currency);
    committed["currentTotalDiscountsSet"] =
        oe_shop_presentment_money(oe_int(session, "cartDiscountCents"), currency);
    committed["totalDiscountsSet"] =
        oe_shop_presentment_money(oe_int(session, "cartDiscountCents"), currency);
    committed["totalShippingPriceSet"] =
        oe_shop_presentment_money(current_totals.shipping, currency);
    committed["totalReceivedSet"] = oe_shop_presentment_money(received, currency);
    committed["totalOutstandingSet"] = oe_shop_presentment_money(outstanding, currency);
    committed["currentTaxLines"] = json!(oe_session_tax_lines(session, "curQty"));
    committed["lineItems"] = json!({ "nodes": line_nodes });
    committed["events"] = json!({
        "nodes": [{
            "id": "gid://shopify/BasicEvent/oe-edited",
            "action": "edited",
            "message": message,
            "createdAt": "2026-01-01T00:00:00Z"
        }]
    });
    committed["fulfillmentOrders"] = json!({ "nodes": fulfillment_orders });
    committed
}

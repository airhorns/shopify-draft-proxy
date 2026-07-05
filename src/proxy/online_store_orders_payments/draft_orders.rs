use super::*;

mod helpers;
pub(in crate::proxy) use self::helpers::*;

pub(in crate::proxy) fn draft_order_create_first_line_title(
    field: &RootFieldSelection,
) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first()?;
    resolved_string_field(first_line, "title")
}

pub(in crate::proxy) fn draft_order_input_custom_attributes(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let attributes = order_create_custom_attributes(input, "customAttributes");
    if attributes.is_empty() {
        order_create_custom_attributes(input, "properties")
    } else {
        attributes
    }
}

pub(in crate::proxy) fn draft_order_record_skeleton(
    id: &str,
    name: &str,
    line_item_nodes: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "status": "OPEN",
        "ready": true,
        "email": Value::Null,
        "sourceName": Value::Null,
        "note": Value::Null,
        "purchasingEntity": Value::Null,
        "customer": Value::Null,
        "taxExempt": false,
        "taxesIncluded": false,
        "reserveInventoryUntil": Value::Null,
        "paymentTerms": Value::Null,
        "tags": [],
        "invoiceUrl": draft_order_invoice_url(id),
        "customAttributes": [],
        "appliedDiscount": Value::Null,
        "billingAddress": Value::Null,
        "shippingAddress": Value::Null,
        "shippingLine": Value::Null,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "completedAt": Value::Null,
        "invoiceSentAt": Value::Null,
        "order": Value::Null,
        "orderId": Value::Null,
        "lineItems": order_connection(line_item_nodes.clone()),
        "__draftProxyLineItems": line_item_nodes
    })
}

pub(in crate::proxy) fn draft_order_base_record(
    id: &str,
    name: &str,
    input: &BTreeMap<String, ResolvedValue>,
    customer: Option<Value>,
    variant_hydrations: &BTreeMap<String, Value>,
    shop_currency_code: &str,
) -> Value {
    let currency = draft_order_input_currency(input, shop_currency_code);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes = draft_order_line_items(&line_items, id, &currency, variant_hydrations);
    let original_subtotal = sum_money_set(&line_item_nodes, "originalTotalSet");
    let mut record = draft_order_record_skeleton(id, name, line_item_nodes);
    record["currencyCode"] = json!(currency);
    record["email"] = resolved_string_field(input, "email")
        .map(Value::String)
        .unwrap_or(Value::Null);
    record["sourceName"] = resolved_string_field(input, "sourceName")
        .map(Value::String)
        .unwrap_or(Value::Null);
    record["note"] = resolved_string_field(input, "note")
        .map(Value::String)
        .unwrap_or(Value::Null);
    record["purchasingEntity"] = draft_order_purchasing_entity(input);
    record["customer"] = customer.unwrap_or_else(|| draft_order_customer(input));
    record["taxExempt"] = json!(resolved_bool_field(input, "taxExempt").unwrap_or(false));
    record["taxesIncluded"] = json!(resolved_bool_field(input, "taxesIncluded").unwrap_or(false));
    record["reserveInventoryUntil"] = resolved_string_field(input, "reserveInventoryUntil")
        .map(Value::String)
        .unwrap_or(Value::Null);
    record["paymentTerms"] = draft_order_payment_terms(input);
    record["tags"] = json!(normalize_taggable_tags(list_string_field(input, "tags")));
    record["customAttributes"] = json!(draft_order_input_custom_attributes(input));
    record["appliedDiscount"] =
        draft_order_applied_discount(input, shop_currency_code, original_subtotal);
    record["billingAddress"] = order_create_address(resolved_object_field(input, "billingAddress"));
    record["shippingAddress"] =
        order_create_address(resolved_object_field(input, "shippingAddress"));
    record["shippingLine"] = draft_order_shipping_line(input, shop_currency_code);
    record
}

pub(in crate::proxy) fn draft_order_calculated_record(
    input: &BTreeMap<String, ResolvedValue>,
    variant_hydrations: &BTreeMap<String, Value>,
    shop_currency_code: &str,
) -> Value {
    let currency = draft_order_input_currency(input, shop_currency_code);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes =
        draft_order_line_items(&line_items, "calculated", &currency, variant_hydrations);
    let original_subtotal = sum_money_set(&line_item_nodes, "originalTotalSet");
    let line_discount_total = draft_order_line_discount_total(&line_item_nodes);
    let shipping_line = draft_order_shipping_line(input, shop_currency_code);
    let shipping_total = money_set_amount(&shipping_line["originalPriceSet"]).unwrap_or(0.0);
    let applied_discount =
        draft_order_applied_discount(input, shop_currency_code, original_subtotal);
    let discount_total = line_discount_total + draft_order_discount_amount(&applied_discount);
    let subtotal = (original_subtotal - discount_total).max(0.0);
    let total = subtotal + shipping_total;
    json!({
        "currencyCode": currency,
        "totalQuantityOfLineItems": line_item_nodes
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>(),
        "subtotalPriceSet": money_bag(subtotal, &currency),
        "totalDiscountsSet": money_bag(discount_total, &currency),
        "totalShippingPriceSet": money_bag(shipping_total, &currency),
        "totalPriceSet": money_bag(total, &currency),
        "lineItems": line_item_nodes,
        "availableShippingRates": []
    })
}

pub(in crate::proxy) fn draft_order_line_items(
    line_items: &[BTreeMap<String, ResolvedValue>],
    draft_order_id: &str,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Vec<Value> {
    line_items
        .iter()
        .enumerate()
        .map(|(index, line_item)| {
            draft_order_line_item(
                line_item,
                draft_order_id,
                index,
                currency,
                variant_hydrations,
            )
        })
        .collect()
}

pub(in crate::proxy) fn draft_order_line_item(
    input: &BTreeMap<String, ResolvedValue>,
    draft_order_id: &str,
    index: usize,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Value {
    let quantity = resolved_int_field(input, "quantity").unwrap_or(1).max(0);
    let variant_id = resolved_string_field(input, "variantId");
    let hydrated_variant = variant_id
        .as_ref()
        .and_then(|id| variant_hydrations.get(id));
    let variant_unit_amount = hydrated_variant.and_then(|variant| {
        variant["price"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
    });
    let unit_amount = if variant_id.is_some() {
        variant_unit_amount
    } else {
        draft_order_line_unit_amount(input)
    }
    .unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    let discount_amount = draft_order_applied_discount_amount(input, line_total);
    let discounted_total = (line_total - discount_amount).max(0.0);
    let tax_lines = order_create_tax_lines(input, "taxLines", currency);
    let title = if variant_id.is_some() {
        hydrated_variant
            .and_then(|variant| variant["product"]["title"].as_str().map(str::to_string))
            .or_else(|| {
                variant_id
                    .as_ref()
                    .map(|id| format!("Variant {}", resource_id_tail(id)))
            })
    } else {
        resolved_string_field(input, "title")
    }
    .unwrap_or_else(|| "Custom Item".to_string());
    let sku = if variant_id.is_some() {
        hydrated_variant.and_then(|variant| variant["sku"].as_str().map(str::to_string))
    } else {
        resolved_string_field(input, "sku")
    }
    .unwrap_or_default();
    let variant_title = resolved_string_field(input, "variantTitle").or_else(|| {
        hydrated_variant.and_then(|variant| variant["title"].as_str().map(str::to_string))
    });
    let variant = variant_id
        .as_ref()
        .map(|id| {
            json!({
                "id": id,
                "title": variant_title,
                "sku": if sku.is_empty() { Value::Null } else { json!(sku) }
            })
        })
        .unwrap_or(Value::Null);
    json!({
        "id": shopify_gid(
            "DraftOrderLineItem",
            format!("{}{}", resource_id_tail(draft_order_id), index + 1),
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": sku,
        "variantTitle": Value::Null,
        "custom": variant_id.is_none(),
        "requiresShipping": if variant_id.is_some() {
            hydrated_variant.and_then(|variant| variant["inventoryItem"]["requiresShipping"].as_bool())
        } else {
            resolved_bool_field(input, "requiresShipping")
        }.unwrap_or(true),
        "taxable": if variant_id.is_some() {
            hydrated_variant.and_then(|variant| variant["taxable"].as_bool())
        } else {
            resolved_bool_field(input, "taxable")
        }.unwrap_or(true),
        "customAttributes": draft_order_input_custom_attributes(input),
        "appliedDiscount": draft_order_applied_discount_from_line(input, currency, line_total),
        "originalUnitPriceSet": money_bag(unit_amount, currency),
        "originalTotalSet": money_bag(line_total, currency),
        "discountedTotalSet": money_bag(discounted_total, currency),
        "totalDiscountSet": money_bag(discount_amount, currency),
        "taxLines": tax_lines,
        "variant": variant
    })
}

pub(in crate::proxy) fn draft_order_line_from_order_line(
    draft_order_id: &str,
    index: usize,
    line: &Value,
    currency: &str,
) -> Value {
    let title = line["title"].as_str().unwrap_or("Order item").to_string();
    let quantity = line["quantity"].as_i64().unwrap_or(1);
    let unit_amount = money_set_amount(&line["originalUnitPriceSet"]).unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    json!({
        "id": shopify_gid(
            "DraftOrderLineItem",
            format!("{}{}", resource_id_tail(draft_order_id), index + 1),
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": line["sku"].clone(),
        "variantTitle": line["variantTitle"].clone(),
        "custom": line["variant"].is_null(),
        "requiresShipping": line["requiresShipping"].as_bool().unwrap_or(true),
        "taxable": line["taxable"].as_bool().unwrap_or(true),
        "customAttributes": line["customAttributes"].as_array().cloned().unwrap_or_default(),
        "appliedDiscount": Value::Null,
        "originalUnitPriceSet": money_bag(unit_amount, currency),
        "originalTotalSet": money_bag(line_total, currency),
        "discountedTotalSet": money_bag(line_total, currency),
        "totalDiscountSet": money_bag(0.0, currency),
        "taxLines": line["taxLines"].clone(),
        "variant": line["variant"].clone()
    })
}

pub(in crate::proxy) fn draft_order_total_from_order(order: &Value) -> Option<f64> {
    money_set_amount(&order["totalPriceSet"])
        .or_else(|| money_set_amount(&order["currentTotalPriceSet"]))
        .filter(|amount| *amount > 0.0)
}

pub(in crate::proxy) fn draft_order_reassign_line_item_ids(
    draft_order: &mut Value,
    draft_order_id: &str,
) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for (index, line) in nodes.iter_mut().enumerate() {
            line["id"] = json!(shopify_gid(
                "DraftOrderLineItem",
                format!("{}{}", resource_id_tail(draft_order_id), index + 1),
            ));
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

pub(in crate::proxy) fn draft_order_clear_line_discounts(
    draft_order: &mut Value,
    shop_currency_code: &str,
) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for line in &mut *nodes {
            let original_total = line["originalTotalSet"].clone();
            line["appliedDiscount"] = Value::Null;
            line["discountedTotalSet"] = original_total;
            let currency = money_set_shop_currency(&line["originalTotalSet"])
                .unwrap_or_else(|| shop_currency_code.to_string());
            line["totalDiscountSet"] = money_bag(0.0, &currency);
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

pub(in crate::proxy) fn draft_order_shipping_line(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let Some(shipping_line) = resolved_object_field(input, "shippingLine") else {
        return Value::Null;
    };
    let price_input = resolved_object_field(&shipping_line, "priceWithCurrency")
        .or_else(|| resolved_object_field(&shipping_line, "priceSet"))
        .or_else(|| resolved_object_field(&shipping_line, "originalPriceSet"))
        .unwrap_or_default();
    let currency = input_money_currency(&price_input)
        .unwrap_or_else(|| draft_order_input_currency(input, shop_currency_code));
    let amount = input_money_amount(&price_input).unwrap_or(0.0);
    json!({
        "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
        "code": resolved_string_field(&shipping_line, "code").unwrap_or_else(|| "custom".to_string()),
        "custom": true,
        "originalPriceSet": money_bag(amount, &currency),
        "discountedPriceSet": money_bag(amount, &currency)
    })
}

pub(in crate::proxy) fn draft_order_applied_discount(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
    line_total: f64,
) -> Value {
    draft_order_applied_discount_from_line(
        input,
        &draft_order_input_currency(input, shop_currency_code),
        line_total,
    )
}

pub(in crate::proxy) fn draft_order_applied_discount_from_line(
    input: &BTreeMap<String, ResolvedValue>,
    currency: &str,
    line_total: f64,
) -> Value {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return Value::Null;
    };
    draft_order_discount_record(
        &discount,
        currency,
        draft_order_discount_amount_from_discount(&discount, line_total),
    )
}

fn draft_order_not_found_payload(resource_field: &str) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(resource_field.to_string(), Value::Null);
    payload.insert(
        "userErrors".to_string(),
        json!([user_error(
            ["id"],
            "Draft order does not exist",
            Some("NOT_FOUND")
        )]),
    );
    Value::Object(payload)
}

pub(in crate::proxy) fn draft_order_discount_record(
    discount: &BTreeMap<String, ResolvedValue>,
    currency: &str,
    amount: f64,
) -> Value {
    json!({
        "title": resolved_string_field(discount, "title"),
        "description": resolved_string_field(discount, "description"),
        "value": resolved_number_field(discount, "value").unwrap_or(amount),
        "valueType": resolved_string_field(discount, "valueType").unwrap_or_else(|| "FIXED_AMOUNT".to_string()),
        "amountSet": money_bag(amount, currency)
    })
}

pub(in crate::proxy) fn draft_order_discount_amount(discount: &Value) -> f64 {
    money_set_amount(&discount["amountSet"]).unwrap_or(0.0)
}

pub(in crate::proxy) fn draft_order_line_discount_total(line_items: &[Value]) -> f64 {
    sum_money_set(line_items, "totalDiscountSet")
}

pub(in crate::proxy) fn draft_order_tax_total(draft_order: &Value, line_items: &[Value]) -> f64 {
    if draft_order["taxExempt"].as_bool().unwrap_or(false) {
        return 0.0;
    }
    let has_line_tax_lines = line_items.iter().any(|line| line.get("taxLines").is_some());
    let line_tax_total = line_items
        .iter()
        .flat_map(|line| {
            line["taxLines"]
                .as_array()
                .into_iter()
                .flat_map(|tax_lines| tax_lines.iter())
        })
        .filter_map(|tax_line| money_set_amount(&tax_line["priceSet"]))
        .sum::<f64>();
    if has_line_tax_lines {
        line_tax_total
    } else {
        money_set_amount(&draft_order["totalTaxSet"]).unwrap_or(0.0)
    }
}

pub(in crate::proxy) fn draft_order_discount_amount_from_discount(
    discount: &BTreeMap<String, ResolvedValue>,
    line_total: f64,
) -> f64 {
    if resolved_string_field(discount, "valueType").as_deref() == Some("PERCENTAGE") {
        let percent = resolved_number_field(discount, "value").unwrap_or(0.0);
        return line_total * percent / 100.0;
    }
    resolved_number_field(discount, "amount").unwrap_or(0.0)
}

pub(in crate::proxy) fn draft_order_applied_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    line_total: f64,
) -> f64 {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return 0.0;
    };
    draft_order_discount_amount_from_discount(&discount, line_total)
}

pub(in crate::proxy) fn draft_order_line_unit_amount(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<f64> {
    resolved_string_field(input, "originalUnitPrice")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "originalUnitPrice"))
        .or_else(|| {
            resolved_object_field(input, "originalUnitPriceWithCurrency")
                .and_then(|money| input_money_amount(&money))
        })
        .or_else(|| {
            resolved_object_field(input, "priceSet").and_then(|money| input_money_amount(&money))
        })
}

pub(in crate::proxy) fn draft_order_input_currency(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> String {
    resolved_string_field(input, "currencyCode")
        .or_else(|| {
            resolved_object_field(input, "shippingLine")
                .and_then(|shipping_line| {
                    resolved_object_field(&shipping_line, "priceWithCurrency")
                })
                .and_then(|money| input_money_currency(&money))
        })
        .or_else(|| {
            resolved_object_list_field(input, "lineItems")
                .into_iter()
                .find(|line| resolved_string_field(line, "variantId").is_none())
                .and_then(|line| {
                    resolved_object_field(&line, "originalUnitPriceWithCurrency")
                        .and_then(|money| input_money_currency(&money))
                })
        })
        .unwrap_or_else(|| shop_currency_code.to_string())
}

pub(in crate::proxy) fn draft_order_applied_discount_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    update: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(discount) = resolved_object_field(input, "appliedDiscount") {
        if let Some(error) = draft_order_applied_discount_value_error(
            &discount,
            draft_order_discount_field(update, None),
        ) {
            errors.push(error);
        }
    }
    for (index, line_item) in resolved_object_list_field(input, "lineItems")
        .iter()
        .enumerate()
    {
        let Some(discount) = resolved_object_field(line_item, "appliedDiscount") else {
            continue;
        };
        if let Some(error) = draft_order_applied_discount_value_error(
            &discount,
            draft_order_discount_field(update, Some(index)),
        ) {
            errors.push(error);
        }
    }
    errors
}

pub(in crate::proxy) fn draft_order_applied_discount_value_error(
    discount: &BTreeMap<String, ResolvedValue>,
    field: Value,
) -> Option<Value> {
    let value = resolved_number_field(discount, "value")?;
    if resolved_string_field(discount, "valueType").as_deref() != Some("PERCENTAGE") {
        return None;
    }
    if draft_order_discount_value_has_more_than_two_decimals(value) {
        return Some(user_error_omit_code(
            field,
            "Applied discount value can have at most 2 digits after decimal point",
            None,
        ));
    }
    if value > 100.0 {
        return Some(user_error_omit_code(
            field,
            "Applied discount value must be less than or equal to 100%",
            None,
        ));
    }
    None
}

pub(in crate::proxy) fn draft_order_discount_value_has_more_than_two_decimals(value: f64) -> bool {
    let shifted = value * 100.0;
    (shifted - shifted.round()).abs() > 1e-9
}

pub(in crate::proxy) fn draft_order_discount_field(
    update: bool,
    line_index: Option<usize>,
) -> Value {
    let mut segments = Vec::new();
    if update {
        segments.push(json!("input"));
    }
    if let Some(index) = line_index {
        segments.push(json!("lineItems"));
        segments.push(json!(index.to_string()));
    }
    segments.push(json!("appliedDiscount"));
    segments.push(json!("value"));
    Value::Array(segments)
}

pub(in crate::proxy) fn draft_order_currency(
    draft_order: &Value,
    shop_currency_code: &str,
) -> String {
    draft_order["currencyCode"]
        .as_str()
        .map(str::to_string)
        .or_else(|| draft_order_money_set_currency(&draft_order["totalPriceSet"]))
        .or_else(|| draft_order_money_set_currency(&draft_order["subtotalPriceSet"]))
        .or_else(|| draft_order_money_set_currency(&draft_order["totalShippingPriceSet"]))
        .or_else(|| {
            draft_order_money_set_currency(&draft_order["shippingLine"]["originalPriceSet"])
        })
        .or_else(|| {
            draft_order_money_set_currency(&draft_order["shippingLine"]["discountedPriceSet"])
        })
        .or_else(|| draft_order_line_items_currency(draft_order))
        .unwrap_or_else(|| shop_currency_code.to_string())
}

fn draft_order_money_set_currency(money_set: &Value) -> Option<String> {
    money_set_shop_currency(money_set)
}

fn draft_order_line_items_currency(draft_order: &Value) -> Option<String> {
    let nodes = draft_order["lineItems"]["nodes"]
        .as_array()
        .or_else(|| draft_order["__draftProxyLineItems"].as_array())?;
    nodes.iter().find_map(|line| {
        draft_order_money_set_currency(&line["originalTotalSet"])
            .or_else(|| draft_order_money_set_currency(&line["discountedTotalSet"]))
            .or_else(|| draft_order_money_set_currency(&line["originalUnitPriceSet"]))
    })
}

pub(in crate::proxy) fn draft_order_payment_terms(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let Some(payment_terms) = resolved_object_field(input, "paymentTerms") else {
        return Value::Null;
    };
    resolved_string_field(&payment_terms, "paymentTermsTemplateId")
        .map(|id| {
            json!({
                "id": id,
                "overdue": false,
                "dueInDays": Value::Null,
                "paymentTermsName": Value::Null,
                "paymentTermsType": Value::Null,
                "translatedName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn draft_order_complete_implicit_payment_pending(draft_order: &Value) -> bool {
    let has_payment_terms = draft_order
        .get("paymentTerms")
        .is_some_and(|payment_terms| !payment_terms.is_null());
    let has_deposit_configuration = ["depositConfiguration", "depositConfigurationId"]
        .iter()
        .any(|field| {
            draft_order
                .get(*field)
                .is_some_and(|deposit_configuration| !deposit_configuration.is_null())
        });
    has_payment_terms && !has_deposit_configuration
}

pub(in crate::proxy) fn draft_order_purchasing_entity(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .map(|entity| resolved_value_json(&ResolvedValue::Object(entity)))
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn draft_order_customer(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
        .map(|id| {
            json!({
                "id": id,
                "email": resolved_string_field(input, "email"),
                "displayName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn draft_order_customer_id(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
}

pub(in crate::proxy) fn draft_order_line_item_variant_ids(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = resolved_object_list_field(input, "lineItems")
        .into_iter()
        .filter_map(|line_item| resolved_string_field(&line_item, "variantId"))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

pub(in crate::proxy) fn draft_order_invoice_url(id: &str) -> String {
    format!(
        "https://shopify-draft-proxy.local/draft_orders/{}/invoice",
        resource_id_tail(id)
    )
}

pub(in crate::proxy) fn draft_order_search_decision(
    draft_order: &Value,
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
            match draft_order_matches_query_term(draft_order, key, value) {
                Some(true) => {}
                Some(false) => return StagedSearchDecision::NoMatch,
                // Shopify returns invalid-field search warnings and ignores
                // unknown draft-order fields instead of narrowing to empty.
                None => continue,
            }
        } else if !draft_order_matches_free_text(draft_order, term) {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

fn draft_order_matches_query_term(draft_order: &Value, key: &str, value: &str) -> Option<bool> {
    match key.to_ascii_lowercase().as_str() {
        "id" => Some(draft_order_matches_id(draft_order, value)),
        "name" => Some(draft_order_search_string_matches(
            draft_order.get("name").and_then(Value::as_str),
            value,
        )),
        "email" => Some(draft_order_search_string_matches(
            draft_order.get("email").and_then(Value::as_str),
            value,
        )),
        "status" => Some(draft_order_matches_status(draft_order, value)),
        "customer_id" => Some(draft_order_matches_customer_id(draft_order, value)),
        "tag" => Some(draft_order_matches_tag(draft_order, value)),
        "created_at" => Some(draft_order_matches_datetime_comparator(
            draft_order.get("createdAt").and_then(Value::as_str),
            value,
        )),
        "updated_at" => Some(draft_order_matches_datetime_comparator(
            draft_order.get("updatedAt").and_then(Value::as_str),
            value,
        )),
        "total_price" => Some(draft_order_matches_money_comparator(
            money_set_amount(&draft_order["totalPriceSet"]),
            value,
        )),
        _ => None,
    }
}

fn draft_order_matches_id(draft_order: &Value, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    draft_order
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| {
            id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
        })
}

fn draft_order_matches_status(draft_order: &Value, value: &str) -> bool {
    let status = draft_order
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .as_str()
    {
        "any" => true,
        "open" => status.eq_ignore_ascii_case("OPEN"),
        "invoice_sent" | "invoice-sent" | "invoice sent" => {
            status.eq_ignore_ascii_case("INVOICE_SENT")
        }
        "completed" => status.eq_ignore_ascii_case("COMPLETED"),
        other => status.eq_ignore_ascii_case(other),
    }
}

fn draft_order_matches_customer_id(draft_order: &Value, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    [
        draft_order
            .get("customer")
            .and_then(|customer| customer.get("id"))
            .and_then(Value::as_str),
        draft_order
            .get("purchasingEntity")
            .and_then(|entity| entity.get("customerId"))
            .and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .any(|id| id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value)
}

fn draft_order_matches_tag(draft_order: &Value, value: &str) -> bool {
    draft_order
        .get("tags")
        .and_then(Value::as_array)
        .is_some_and(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| draft_order_search_token_matches(tag, value))
        })
}

fn draft_order_matches_datetime_comparator(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = draft_order_search_comparator(query_value);
    if expected.is_empty() {
        return false;
    }
    let actual = draft_order_search_datetime_value(actual, expected);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

fn draft_order_matches_money_comparator(actual: Option<f64>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = draft_order_search_comparator(query_value);
    let Some(expected) = expected.parse::<f64>().ok() else {
        return false;
    };
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => (actual - expected).abs() < f64::EPSILON,
    }
}

fn draft_order_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}

fn draft_order_search_datetime_value<'a>(actual: &'a str, expected: &str) -> &'a str {
    if expected.contains('T') {
        actual
    } else {
        actual
            .split_once('T')
            .map(|(date, _)| date)
            .unwrap_or(actual)
    }
}

fn draft_order_matches_free_text(draft_order: &Value, value: &str) -> bool {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return true;
    }
    draft_order_matches_id(draft_order, value)
        || draft_order_search_string_matches(draft_order.get("name").and_then(Value::as_str), value)
        || draft_order_search_string_matches(
            draft_order.get("email").and_then(Value::as_str),
            value,
        )
        || draft_order_search_string_matches(draft_order.get("note").and_then(Value::as_str), value)
        || draft_order
            .get("tags")
            .and_then(Value::as_array)
            .is_some_and(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .any(|tag| draft_order_search_token_matches(tag, value))
            })
        || connection_nodes(&draft_order["lineItems"])
            .iter()
            .any(|line| {
                draft_order_search_string_matches(line.get("title").and_then(Value::as_str), value)
                    || draft_order_search_string_matches(
                        line.get("sku").and_then(Value::as_str),
                        value,
                    )
            })
}

fn draft_order_search_string_matches(actual: Option<&str>, query_value: &str) -> bool {
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

fn draft_order_search_token_matches(actual: &str, query_value: &str) -> bool {
    let query_value = query_value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if query_value.is_empty() {
        return true;
    }
    let actual = actual.to_ascii_lowercase();
    if let Some(prefix) = query_value.strip_suffix('*') {
        return actual
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part == query_value)
        || actual == query_value
}

fn draft_order_gid_tail_sort_value(draft_order: &Value) -> StagedSortValue {
    let tail = draft_order
        .get("id")
        .and_then(Value::as_str)
        .map(resource_id_tail)
        .unwrap_or_default();
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn draft_order_string_sort_value(draft_order: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        draft_order
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn draft_order_customer_name_sort_value(draft_order: &Value) -> StagedSortValue {
    let value = draft_order
        .get("customer")
        .and_then(|customer| customer.get("displayName"))
        .and_then(Value::as_str)
        .or_else(|| {
            draft_order
                .get("customer")
                .and_then(|customer| customer.get("email"))
                .and_then(Value::as_str)
        })
        .or_else(|| draft_order.get("email").and_then(Value::as_str))
        .unwrap_or_default();
    StagedSortValue::String(value.to_ascii_lowercase())
}

fn draft_order_total_price_sort_value(draft_order: &Value) -> StagedSortValue {
    money_set_amount(&draft_order["totalPriceSet"])
        .map(|amount| StagedSortValue::I64((amount * 100.0).round() as i64))
        .unwrap_or(StagedSortValue::Null)
}

pub(in crate::proxy) fn draft_order_staged_sort_key(
    draft_order: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("ID") {
        "CUSTOMER_NAME" => draft_order_customer_name_sort_value(draft_order),
        "NUMBER" => draft_order_gid_tail_sort_value(draft_order),
        "STATUS" => draft_order_string_sort_value(draft_order, "status"),
        "TOTAL_PRICE" => draft_order_total_price_sort_value(draft_order),
        "UPDATED_AT" => draft_order_string_sort_value(draft_order, "updatedAt"),
        "ID" | "RELEVANCE" => draft_order_gid_tail_sort_value(draft_order),
        _ => draft_order_gid_tail_sort_value(draft_order),
    };
    vec![primary, draft_order_gid_tail_sort_value(draft_order)]
}

pub(in crate::proxy) fn draft_order_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    update: bool,
) -> Option<Vec<Value>> {
    let tags = list_string_field(input, "tags");
    let long_tag_errors = tags
        .iter()
        .enumerate()
        .filter(|(_, tag)| tag.chars().count() > 40)
        .map(|(index, _)| {
            let field = if update {
                json!(["input", "tags", (index + 1).to_string()])
            } else {
                json!(["tags", index.to_string()])
            };
            user_error_omit_code(
                field,
                "Title Tag exceeds the maximum length of 40 characters",
                None,
            )
        })
        .collect::<Vec<_>>();
    if !long_tag_errors.is_empty() {
        return Some(long_tag_errors);
    }

    let line_items = resolved_object_list_field(input, "lineItems");
    if !update {
        if line_items.is_empty() {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Add at least 1 product",
                None,
            )]);
        }
        if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
            return Some(vec![user_error_omit_code(
                ["email"],
                "Email is invalid",
                None,
            )]);
        }
    }
    for (index, line_item) in line_items.iter().enumerate() {
        if resolved_int_field(line_item, "quantity").is_some_and(|quantity| quantity < 1) {
            return Some(vec![user_error_omit_code(
                vec![
                    "lineItems".to_string(),
                    index.to_string(),
                    "quantity".to_string(),
                ],
                "Quantity must be greater than or equal to 1",
                None,
            )]);
        }
        if resolved_string_field(line_item, "title").is_none()
            && resolved_string_field(line_item, "variantId").is_none()
        {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Merchandise title is empty.",
                None,
            )]);
        }
        if resolved_string_field(line_item, "variantId").is_none()
            && draft_order_line_unit_amount(line_item).is_some_and(|amount| amount < 0.0)
        {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "Cannot send negative price for line_item",
                None,
            )]);
        }
    }
    let discount_errors = draft_order_applied_discount_user_errors(input, update);
    if !discount_errors.is_empty() {
        return Some(discount_errors);
    }
    if resolved_object_field(input, "paymentTerms").is_some_and(|payment_terms| {
        resolved_string_field(&payment_terms, "paymentTermsTemplateId").is_none()
            && !resolved_object_list_field(&payment_terms, "paymentSchedules").is_empty()
    }) {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Payment terms template id can not be empty.",
            None,
        )]);
    }
    if resolved_string_field(input, "reserveInventoryUntil")
        .as_deref()
        .is_some_and(|value| value < "2024-01-01T00:00:00Z")
    {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Reserve until can't be in the past",
            None,
        )]);
    }
    None
}

pub(in crate::proxy) fn draft_order_unavailable_variant_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    unavailable_variant_ids: &BTreeSet<String>,
) -> Option<Vec<Value>> {
    for line_item in resolved_object_list_field(input, "lineItems") {
        let Some(variant_id) = resolved_string_field(&line_item, "variantId") else {
            continue;
        };
        if unavailable_variant_ids.contains(&variant_id) {
            let message = format!(
                "Product with ID {} is no longer available.",
                resource_id_tail(&variant_id)
            );
            return Some(vec![user_error_omit_code(Value::Null, &message, None)]);
        }
    }
    None
}

pub(in crate::proxy) fn draft_order_calculate_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let line_items = resolved_object_list_field(input, "lineItems");
    if line_items.is_empty() {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Add at least 1 product",
            None,
        )]);
    }
    if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
        return Some(vec![user_error_omit_code(
            ["email"],
            "Email is invalid",
            None,
        )]);
    }
    None
}

pub(in crate::proxy) fn draft_order_top_level_validation_response(
    fields: &[RootFieldSelection],
) -> Option<Response> {
    let mut errors = Vec::new();
    for field in fields {
        if !matches!(
            field.name.as_str(),
            "draftOrderCreate" | "draftOrderUpdate" | "draftOrderCalculate"
        ) {
            continue;
        }
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            continue;
        };
        let line_item_count = resolved_list_len(&input, "lineItems");
        if line_item_count > 499 {
            errors.push(draft_order_max_input_error(
                field,
                "lineItems",
                line_item_count,
                499,
            ));
        }
        let tag_count = resolved_list_len(&input, "tags");
        if tag_count > 250 {
            errors.push(draft_order_max_input_error(field, "tags", tag_count, 250));
        }
    }
    (!errors.is_empty()).then(|| ok_json(json!({ "data": Value::Null, "errors": errors })))
}

pub(in crate::proxy) fn draft_order_max_input_error(
    field: &RootFieldSelection,
    argument: &str,
    count: usize,
    max: usize,
) -> Value {
    json!({
        "message": format!(
            "The input array size of {count} is greater than the maximum allowed of {max}."
        ),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": [field.response_key.clone(), "input", argument],
        "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn draft_order_complete_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let field = fields.iter().find(|field| field.name == root_field);

        // Forward a hydrate + observe for a draft not created locally this scenario
        // so completion settles the real draft instead of a precondition seed.
        if root_field == "draftOrderComplete" {
            if let Some(id) = field.and_then(|field| resolved_string_field(&field.arguments, "id"))
            {
                self.ensure_draft_order_hydrated(request, &id);
            }
        }

        match root_field {
            "draftOrderComplete" => {
                let field = field?;
                Some(data_response(
                    &field.response_key,
                    self.complete_staged_draft_order(request, field),
                ))
            }
            "order" => {
                let field = field?;
                let id = resolved_string_field(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orders" => {
                let field = field?;
                let query_arg =
                    resolved_string_field(&field.arguments, "query").unwrap_or_default();
                // This local overlay can only resolve a single `name:` look-up against
                // orders staged in this scenario (the draft-complete read-back). Catalog
                // reads — tag/status filters, sort/window/count, multi-alias windows, or a
                // cold empty catalog — must forward upstream and observe, so decline
                // anything that isn't a lone name look-up.
                if fields.len() > 1 || !query_arg.starts_with("name:") {
                    return None;
                }
                let nodes = self
                    .store
                    .staged
                    .orders
                    .values()
                    .filter(|order| {
                        order["name"]
                            .as_str()
                            .is_some_and(|name| query_arg == format!("name:{name}"))
                    })
                    .map(|order| {
                        selected_json(order, &nested_selected_fields(&field.selection, &["nodes"]))
                    })
                    .collect::<Vec<_>>();
                Some(data_response(&field.response_key, order_connection(nodes)))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn draft_order_lifecycle_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
                    | "draftOrder"
                    | "draftOrders"
                    | "draftOrdersCount"
            )
        }) {
            return None;
        }
        let has_lifecycle_root = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
            )
        });
        // List/count reads are only served locally once at least one draft order
        // has been staged in this scenario; otherwise they fall through to the
        // upstream passthrough so the recorded live catalog replays verbatim.
        let has_staged_read = fields.iter().any(|field| match field.name.as_str() {
            "draftOrder" => resolved_string_field(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.draft_orders.contains_key(&id)),
            // List/count reads resolve locally once any draft order has existed this
            // scenario (counter advanced past its base) — a session that created then
            // bulk-deleted every draft must still report `{count: 0}` rather than
            // falling through to the upstream catalog.
            "draftOrders" | "draftOrdersCount" => {
                !self.store.staged.draft_orders.is_empty()
                    || self.store.staged.next_draft_order_id > 1
            }
            _ => false,
        });
        if !has_lifecycle_root && !has_staged_read {
            return None;
        }

        if let Some(response) = draft_order_top_level_validation_response(&fields) {
            return Some(response);
        }

        for field in &fields {
            match field.name.as_str() {
                "draftOrderUpdate" | "draftOrderDuplicate" => {
                    if let Some(id) = resolved_string_field(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderDelete" => {
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    if let Some(id) = resolved_string_field(&input, "id")
                        .or_else(|| resolved_string_field(&field.arguments, "id"))
                    {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderBulkDelete" => {
                    for id in self.draft_order_bulk_target_ids(field) {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderCreateFromOrder" => {
                    if let Some(order_id) = resolved_string_field(&field.arguments, "orderId") {
                        self.ensure_order_hydrated(request, &order_id);
                    }
                }
                "draftOrderInvoicePreview" | "draftOrder" => {
                    if let Some(id) = resolved_string_field(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                _ => {}
            }
        }

        let mut declined = false;
        let data = root_payload_json(&fields, |field| {
            if declined {
                return None;
            }
            let value = match field.name.as_str() {
                "draftOrderCreate" => {
                    self.stage_draft_order_create(request, query, variables, field)
                }
                "draftOrderUpdate" => {
                    self.stage_draft_order_update(request, query, variables, field)
                }
                "draftOrderCalculate" => self.calculate_draft_order_payload(request, field),
                "draftOrderDuplicate" => {
                    self.stage_draft_order_duplicate(request, query, variables, field)
                }
                "draftOrderDelete" => {
                    self.stage_draft_order_delete(request, query, variables, field)
                }
                "draftOrderBulkDelete" => {
                    self.stage_draft_order_bulk_delete(request, query, variables, field)
                }
                "draftOrderCreateFromOrder" => {
                    self.stage_draft_order_create_from_order(request, query, variables, field)
                }
                "draftOrderInvoicePreview" => {
                    self.draft_order_invoice_preview_payload(request, query, variables, field)
                }
                "draftOrder" => self.staged_draft_order_read(field),
                "draftOrders" => self.staged_draft_orders_connection(field),
                "draftOrdersCount" => self.staged_draft_orders_count(field),
                _ => {
                    declined = true;
                    return None;
                }
            };
            Some(value)
        });
        if declined {
            return None;
        }
        Some(ok_json(json!({ "data": data })))
    }

    pub(super) fn stage_draft_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let (variant_hydrations, unavailable_variant_ids) =
            self.draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
        if let Some(user_errors) =
            draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
        {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let customer = draft_order_customer_id(&input)
            .and_then(|customer_id| self.hydrate_draft_order_customer(request, &customer_id));
        let mut draft_order =
            self.build_draft_order_record(&id, &name, &input, customer, &variant_hydrations);
        let timestamp = self.next_product_timestamp();
        draft_order["createdAt"] = json!(timestamp.clone());
        draft_order["updatedAt"] = json!(timestamp);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderCreate",
            vec![id],
        );
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn stage_draft_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, true) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let Some(existing) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &draft_order_not_found_payload("draftOrder"),
                &field.selection,
            );
        };
        let variant_hydrations = if input.contains_key("lineItems") {
            let (variant_hydrations, unavailable_variant_ids) = self
                .draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
            if let Some(user_errors) =
                draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
            {
                return selected_json(
                    &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                    &field.selection,
                );
            }
            variant_hydrations
        } else {
            BTreeMap::new()
        };
        let current_updated_at = existing
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut updated = self.merge_draft_order_input(existing, &input, &variant_hydrations);
        updated["updatedAt"] = json!(self.next_product_updated_at(&current_updated_at));
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), updated.clone());
        self.sync_draft_order_tags(&id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderUpdate",
            vec![id],
        );
        selected_json(
            &json!({ "draftOrder": updated, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn calculate_draft_order_payload(
        &self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        if let Some(user_errors) = draft_order_calculate_user_errors(&input) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let (variant_hydrations, unavailable_variant_ids) =
            self.draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
        if let Some(user_errors) =
            draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
        {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let calculated = draft_order_calculated_record(
            &input,
            &variant_hydrations,
            &self.store.shop_currency_code(),
        );
        selected_json(
            &json!({ "calculatedDraftOrder": calculated, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn stage_draft_order_duplicate(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(source) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &draft_order_not_found_payload("draftOrder"),
                &field.selection,
            );
        };
        let new_id = self.next_draft_order_id();
        let new_name = self.draft_order_name_for_id(&new_id);
        let mut duplicate = source;
        duplicate["id"] = json!(new_id.clone());
        duplicate["name"] = json!(new_name);
        duplicate["status"] = json!("OPEN");
        duplicate["ready"] = json!(true);
        duplicate["completedAt"] = Value::Null;
        duplicate["invoiceSentAt"] = Value::Null;
        duplicate["order"] = Value::Null;
        duplicate["orderId"] = Value::Null;
        duplicate["invoiceUrl"] = json!(draft_order_invoice_url(&new_id));
        duplicate["taxExempt"] = json!(false);
        duplicate["reserveInventoryUntil"] = Value::Null;
        duplicate["appliedDiscount"] = Value::Null;
        duplicate["shippingLine"] = Value::Null;
        let timestamp = self.next_product_timestamp();
        duplicate["createdAt"] = json!(timestamp.clone());
        duplicate["updatedAt"] = json!(timestamp);
        draft_order_clear_line_discounts(&mut duplicate, &self.store.shop_currency_code());
        draft_order_reassign_line_item_ids(&mut duplicate, &new_id);
        self.recalculate_draft_order_totals(&mut duplicate);
        self.store
            .staged
            .draft_orders
            .insert(new_id.clone(), duplicate.clone());
        self.sync_draft_order_tags(&new_id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderDuplicate",
            vec![new_id],
        );
        selected_json(
            &json!({ "draftOrder": duplicate, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn stage_draft_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .unwrap_or_default();
        if self.store.staged.draft_orders.remove(&id).is_none() {
            return selected_json(
                &draft_order_not_found_payload("deletedId"),
                &field.selection,
            );
        }
        self.store.staged.draft_order_tags.remove(&id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderDelete",
            vec![id.clone()],
        );
        selected_json(
            &json!({ "deletedId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn stage_draft_order_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = self.draft_order_bulk_target_ids(field);
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_orders.remove(id).is_some() {
                self.store.staged.draft_order_tags.remove(id);
                deleted_ids.push(id.clone());
            } else {
                user_errors.push(user_error(
                    vec!["input".to_string(), "ids".to_string(), index.to_string()],
                    "Draft order does not exist",
                    Some("NOT_FOUND"),
                ));
            }
        }
        if !deleted_ids.is_empty() {
            self.record_staged_orders_log_entry(
                request,
                query,
                variables,
                "draftOrderBulkDelete",
                deleted_ids,
            );
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(super) fn stage_draft_order_create_from_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_field(&field.arguments, "orderId").unwrap_or_default();
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let mut draft_order = self.build_draft_order_from_order_record(&id, &name, &order);
        let timestamp = self.next_product_timestamp();
        draft_order["createdAt"] = json!(timestamp.clone());
        draft_order["updatedAt"] = json!(timestamp);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderCreateFromOrder",
            vec![id],
        );
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn draft_order_invoice_preview_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "previewSubject": Value::Null,
                    "previewHtml": Value::Null,
                    "userErrors": [user_error_omit_code(["id"], "Draft order not found", None)]
                }),
                &field.selection,
            );
        };
        let email = resolved_object_field(&field.arguments, "email").unwrap_or_default();
        let subject = resolved_string_field(&email, "subject")
            .unwrap_or_else(|| "Complete your purchase".to_string());
        let custom_message = resolved_string_field(&email, "customMessage").unwrap_or_default();
        let name = draft_order["name"].as_str().unwrap_or("#DRAFT");
        let html = format!(
            "<!DOCTYPE html><html><body><h1>Complete your purchase</h1><p>{custom_message}</p><p>Invoice {name}</p></body></html>"
        );
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoicePreview",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoicePreview without sending email.",
            },
        });
        selected_json(
            &json!({ "previewSubject": subject, "previewHtml": html, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(super) fn staged_draft_order_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .staged
            .draft_orders
            .get(&id)
            .map(|draft_order| {
                selected_json(
                    &self.payment_terms_owner_record_with_effective_due(draft_order),
                    &field.selection,
                )
            })
            .unwrap_or(Value::Null)
    }

    pub(super) fn matching_draft_orders_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        staged_connection_query(
            self.store
                .staged
                .draft_orders
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            arguments,
            draft_order_search_decision,
            draft_order_staged_sort_key,
            value_id_cursor,
        )
    }

    pub(super) fn staged_draft_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let result = self.matching_draft_orders_query(&field.arguments);
        let records = result
            .records
            .iter()
            .map(|draft_order| self.payment_terms_owner_record_with_effective_due(draft_order))
            .collect::<Vec<_>>();
        selected_json(
            &connection_json_with_cursor(
                records,
                |_, node| value_id_cursor(node),
                result.page_info,
            ),
            &field.selection,
        )
    }

    pub(super) fn staged_draft_orders_count(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &count_object(
                self.matching_draft_orders_query(&field.arguments)
                    .total_count,
            ),
            &field.selection,
        )
    }

    pub(super) fn next_draft_order_id(&mut self) -> String {
        let id = shopify_gid("DraftOrder", self.store.staged.next_draft_order_id);
        self.store.staged.next_draft_order_id += 1;
        id
    }

    pub(super) fn draft_order_name_for_id(&self, id: &str) -> String {
        format!("#D{}", resource_id_tail(id))
    }

    pub(super) fn build_draft_order_record(
        &self,
        id: &str,
        name: &str,
        input: &BTreeMap<String, ResolvedValue>,
        customer: Option<Value>,
        variant_hydrations: &BTreeMap<String, Value>,
    ) -> Value {
        let mut draft_order = draft_order_base_record(
            id,
            name,
            input,
            customer,
            variant_hydrations,
            &self.store.shop_currency_code(),
        );
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    pub(super) fn ensure_draft_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.draft_orders.contains_key(id)
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let draft_order = response.body["data"]["draftOrder"].clone();
        if !draft_order.is_object() {
            return;
        }
        self.store
            .staged
            .draft_orders
            .insert(id.to_string(), draft_order);
        self.sync_draft_order_tags(id);
    }

    pub(super) fn ensure_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return;
        }
        // Always attempt a fresh upstream read so the order reflects its live
        // state at the time of this operation. A precondition seed may hold an
        // earlier snapshot of the same order (e.g. the total captured the moment
        // a draft was completed in setup, before the store recalculated
        // tax/shipping), so the recorded hydrate is authoritative when present.
        // On a cassette miss / non-2xx response we keep whatever record is
        // already staged rather than dropping it.
        let response = self.upstream_post(
            request,
            json!({
                "query": ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let order = response.body["data"]["order"].clone();
        if !order.is_object() {
            return;
        }
        self.store.staged.orders.insert(id.to_string(), order);
    }

    pub(super) fn hydrate_draft_order_customer(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if id.is_empty() {
            return None;
        }
        if let Some(customer) = self.store.staged.customers.get(id) {
            return Some(customer.clone());
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderCustomerHydrate",
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if !customer.is_object() {
            return None;
        }
        self.store
            .staged
            .customers
            .insert(id.to_string(), customer.clone());
        Some(customer)
    }

    pub(super) fn draft_order_variant_hydrations(
        &self,
        request: &Request,
        ids: Vec<String>,
    ) -> (BTreeMap<String, Value>, BTreeSet<String>) {
        let mut hydrations = BTreeMap::new();
        let mut unavailable_ids = BTreeSet::new();
        for id in ids {
            if let Some(variant) = self.draft_order_variant_hydration_from_store(&id) {
                hydrations.insert(id, variant);
                continue;
            }
            if self.config.read_mode == ReadMode::Snapshot {
                unavailable_ids.insert(id);
                continue;
            }
            let response = self.upstream_post(
                request,
                json!({
                    "query": DRAFT_ORDER_VARIANT_HYDRATE_QUERY,
                    "operationName": "OrdersDraftOrderVariantHydrate",
                    "variables": { "id": id }
                }),
            );
            if !(200..300).contains(&response.status) {
                continue;
            }
            let variant = response.body["data"]["productVariant"].clone();
            if variant.is_object() {
                hydrations.insert(id, variant);
            } else {
                unavailable_ids.insert(id);
            }
        }
        (hydrations, unavailable_ids)
    }

    pub(super) fn draft_order_variant_hydration_from_store(&self, id: &str) -> Option<Value> {
        let variant = self.store.product_variant_by_id(id)?;
        let product_title = self
            .store
            .product_staged_or_base(&variant.product_id)
            .map(|product| product.title)
            .unwrap_or_else(|| format!("Product {}", resource_id_tail(&variant.product_id)));
        Some(json!({
            "id": variant.id.clone(),
            "title": variant.title.clone(),
            "sku": variant.sku.clone(),
            "taxable": variant.taxable,
            "price": variant.price.clone(),
            "inventoryItem": { "requiresShipping": variant.inventory_item.requires_shipping },
            "product": { "title": product_title }
        }))
    }

    pub(super) fn merge_draft_order_input(
        &self,
        mut draft_order: Value,
        input: &BTreeMap<String, ResolvedValue>,
        variant_hydrations: &BTreeMap<String, Value>,
    ) -> Value {
        if input.contains_key("email") {
            draft_order["email"] = resolved_string_field(input, "email")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("note") {
            draft_order["note"] = resolved_string_field(input, "note")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("sourceName") {
            draft_order["sourceName"] = resolved_string_field(input, "sourceName")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("tags") {
            draft_order["tags"] = json!(normalize_taggable_tags(list_string_field(input, "tags")));
        }
        if input.contains_key("customAttributes") || input.contains_key("properties") {
            draft_order["customAttributes"] = json!(draft_order_input_custom_attributes(input));
        }
        if input.contains_key("shippingLine") {
            draft_order["shippingLine"] =
                draft_order_shipping_line(input, &self.store.shop_currency_code());
        }
        if input.contains_key("billingAddress") {
            draft_order["billingAddress"] =
                order_create_address(resolved_object_field(input, "billingAddress"));
        }
        if input.contains_key("shippingAddress") {
            draft_order["shippingAddress"] =
                order_create_address(resolved_object_field(input, "shippingAddress"));
        }
        if input.contains_key("lineItems") {
            draft_order["lineItems"] = order_connection(draft_order_line_items(
                &resolved_object_list_field(input, "lineItems"),
                draft_order["id"].as_str().unwrap_or_default(),
                &draft_order_currency(&draft_order, &self.store.shop_currency_code()),
                variant_hydrations,
            ));
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        if input.contains_key("appliedDiscount") {
            let line_items = connection_nodes(&draft_order["lineItems"]);
            let original_subtotal = sum_money_set(&line_items, "originalTotalSet");
            draft_order["appliedDiscount"] = draft_order_applied_discount(
                input,
                &self.store.shop_currency_code(),
                original_subtotal,
            );
        }
        if input.contains_key("taxExempt") {
            draft_order["taxExempt"] =
                json!(resolved_bool_field(input, "taxExempt").unwrap_or(false));
        }
        if input.contains_key("taxesIncluded") {
            draft_order["taxesIncluded"] =
                json!(resolved_bool_field(input, "taxesIncluded").unwrap_or(false));
        }
        if input.contains_key("reserveInventoryUntil") {
            draft_order["reserveInventoryUntil"] =
                resolved_string_field(input, "reserveInventoryUntil")
                    .map(Value::String)
                    .unwrap_or(Value::Null);
        }
        if input.contains_key("paymentTerms") {
            draft_order["paymentTerms"] = draft_order_payment_terms(input);
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    pub(super) fn recalculate_draft_order_totals(&self, draft_order: &mut Value) {
        let currency = draft_order_currency(draft_order, &self.store.shop_currency_code());
        let line_items = connection_nodes(&draft_order["lineItems"]);
        let original_subtotal = sum_money_set(&line_items, "originalTotalSet");
        let line_discount_total = draft_order_line_discount_total(&line_items);
        let shipping_total =
            money_set_amount(&draft_order["shippingLine"]["originalPriceSet"]).unwrap_or(0.0);
        let tax_total = draft_order_tax_total(draft_order, &line_items);
        let discount_total =
            line_discount_total + draft_order_discount_amount(&draft_order["appliedDiscount"]);
        let subtotal = (original_subtotal - discount_total).max(0.0);
        let total = subtotal + shipping_total + tax_total;
        draft_order["lineItemsSubtotalPrice"] = money_bag(original_subtotal, &currency);
        draft_order["subtotalPriceSet"] = money_bag(subtotal, &currency);
        draft_order["totalTax"] = json!(format!("{:.2}", (tax_total * 100.0).round() / 100.0));
        draft_order["totalTaxSet"] = money_bag(tax_total, &currency);
        draft_order["totalDiscountsSet"] = money_bag(discount_total, &currency);
        draft_order["totalShippingPriceSet"] = money_bag(shipping_total, &currency);
        draft_order["totalPriceSet"] = money_bag(total, &currency);
        draft_order["totalQuantityOfLineItems"] = json!(line_items
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>());
    }

    pub(super) fn build_draft_order_from_order_record(
        &self,
        id: &str,
        name: &str,
        order: &Value,
    ) -> Value {
        let shop_currency_code = self.store.shop_currency_code();
        let currency = order["currencyCode"]
            .as_str()
            .map(str::to_string)
            .or_else(|| money_set_shop_currency(&order["totalPriceSet"]))
            .unwrap_or(shop_currency_code);
        let line_items = order["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, line)| draft_order_line_from_order_line(id, index, &line, &currency))
            .collect::<Vec<_>>();
        let mut draft_order = draft_order_record_skeleton(id, name, line_items);
        draft_order["email"] = order["email"].clone();
        draft_order["sourceName"] = order["sourceName"].clone();
        draft_order["note"] = order["note"].clone();
        draft_order["customer"] = order["customer"].clone();
        draft_order["tags"] = json!(order["tags"].as_array().cloned().unwrap_or_default());
        draft_order["customAttributes"] = json!(order["customAttributes"]
            .as_array()
            .cloned()
            .unwrap_or_default());
        draft_order["billingAddress"] = order["billingAddress"].clone();
        draft_order["shippingAddress"] = order["shippingAddress"].clone();
        if draft_order["customer"].is_null() {
            draft_order["customer"] = Value::Null;
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        // A draft created from an order mirrors the source order's monetary
        // totals: Shopify carries the order's grand total onto the new draft
        // rather than recomputing from copied line items (the hydrated order's
        // line items frequently omit per-unit prices, so a recalculation can't
        // reproduce the order's discounts/shipping). Prefer the order total when
        // it's available, falling back to the per-line recalculation otherwise.
        if let Some(order_total) = draft_order_total_from_order(order) {
            draft_order["subtotalPriceSet"] = money_bag(order_total, &currency);
            draft_order["totalPriceSet"] = money_bag(order_total, &currency);
        }
        draft_order
    }

    pub(super) fn draft_order_bulk_target_ids(&self, field: &RootFieldSelection) -> Vec<String> {
        let mut ids = resolved_string_list_arg(&field.arguments, "ids");
        if ids.is_empty() && resolved_string_field(&field.arguments, "search").is_some() {
            ids = self.store.staged.draft_orders.keys().cloned().collect();
        }
        ids
    }

    pub(in crate::proxy) fn sync_draft_order_tags(&mut self, id: &str) {
        if let Some(draft_order) = self.store.staged.draft_orders.get(id) {
            let tags = draft_order["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tag| tag.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            self.store
                .staged
                .draft_order_tags
                .insert(id.to_string(), tags);
        }
    }

    pub(super) fn sync_draft_order_record_tags(&mut self, id: &str) {
        let Some(tags) = self.store.staged.draft_order_tags.get(id).cloned() else {
            return;
        };
        if let Some(draft_order) = self.store.staged.draft_orders.get_mut(id) {
            draft_order["tags"] = json!(tags);
        }
    }

    pub(super) fn complete_staged_draft_order(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error_omit_code(["id"], "ID is required", None)]
                }),
                &field.selection,
            );
        };
        let Some(mut draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error_omit_code(["id"], "Draft order does not exist", None)]
                }),
                &field.selection,
            );
        };
        if draft_order.get("__draftProxyLineItems").is_none() {
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        if draft_order["status"].as_str() == Some("COMPLETED") {
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [user_error_omit_code(Value::Null, "This order has been paid", None)]
                }),
                &field.selection,
            );
        }
        let payment_gateway_id = resolved_string_field(&field.arguments, "paymentGatewayId");
        if payment_gateway_id.is_some() {
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [user_error_omit_code(Value::Null, "Invalid payment gateway", None)]
                }),
                &field.selection,
            );
        }
        let order_id = shopify_gid("Order", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let shop_currency_code = self.store.shop_currency_code();
        let currency_code =
            money_set_shop_currency(&draft_order["totalPriceSet"]).unwrap_or(shop_currency_code);
        let total_amount = money_set_amount(&draft_order["totalPriceSet"]).unwrap_or(0.0);
        let subtotal_amount = money_set_amount(&draft_order["subtotalPriceSet"])
            .or_else(|| money_set_amount(&draft_order["lineItemsSubtotalPrice"]))
            .unwrap_or(0.0);
        let tax_amount = money_set_amount(&draft_order["totalTaxSet"]).unwrap_or(0.0);
        let payment_pending = if field.raw_arguments.contains_key("paymentPending") {
            matches!(
                field.arguments.get("paymentPending"),
                Some(ResolvedValue::Bool(true))
            )
        } else {
            draft_order_complete_implicit_payment_pending(&draft_order)
        };
        // Completing a draft materializes order line items: they move into the
        // LineItem id namespace and an absent SKU is reported as null (Shopify
        // surfaces order line items distinctly from their draft counterparts).
        let order_line_items = draft_order["__draftProxyLineItems"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut line| {
                if let Some(tail) = line["id"].as_str().map(resource_id_path_tail) {
                    line["id"] = json!(shopify_gid("LineItem", tail));
                }
                if line["sku"].as_str() == Some("") {
                    line["sku"] = Value::Null;
                }
                line
            })
            .collect::<Vec<_>>();
        // The completed order inherits the draft's merchant-facing note and tags.
        // It is settled through the manual payment gateway unless the caller
        // explicitly marks it pending or payment terms implicitly leave payment
        // outstanding.
        let order_note = draft_order["note"].clone();
        let order_tags = draft_order["tags"].as_array().cloned().unwrap_or_default();
        let source_name = draft_order["sourceName"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| request_api_client_id(request));
        let payment_gateway_names = vec![json!("manual")];
        // Shopify records both settled and pending draft-order completions as a
        // manual SALE transaction. The transaction status, not its presence,
        // distinguishes captured payment from outstanding invoice payment.
        let transaction_status = if payment_pending {
            "PENDING"
        } else {
            "SUCCESS"
        };
        let order_transactions = vec![json!({
                "kind": "SALE",
                "status": transaction_status,
                "gateway": "manual",
                "amountSet": money_bag(total_amount, &currency_code)
        })];
        let order_name = self.next_order_name();
        let mut order = json!({
            "id": order_id.clone(),
            "name": order_name,
            "sourceName": source_name,
            "note": order_note,
            "tags": order_tags,
            "paymentGatewayNames": payment_gateway_names,
            "transactions": order_transactions,
            "currencyCode": currency_code,
            "presentmentCurrencyCode": currency_code,
            "displayFinancialStatus": if payment_pending { "PENDING" } else { "PAID" },
            "displayFulfillmentStatus": "UNFULFILLED",
            "subtotalPriceSet": money_bag(subtotal_amount, &currency_code),
            "currentSubtotalPriceSet": money_bag(subtotal_amount, &currency_code),
            "totalTaxSet": money_bag(tax_amount, &currency_code),
            "currentTotalTaxSet": money_bag(tax_amount, &currency_code),
            "totalPriceSet": money_bag(total_amount, &currency_code),
            "currentTotalPriceSet": money_bag(total_amount, &currency_code),
            "lineItems": {
                "nodes": order_line_items
            }
        });
        let order_transactions = order["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        order_create_payment_fields(
            &mut order,
            &order_transactions,
            total_amount,
            &currency_code,
            &currency_code,
        );
        if let Some(purchasing_entity) = draft_order.get("__draftProxyPurchasingEntity") {
            if !purchasing_entity.is_null() {
                order["purchasingEntity"] = purchasing_entity.clone();
            }
        } else if !draft_order["purchasingEntity"].is_null() {
            order["purchasingEntity"] = draft_order["purchasingEntity"].clone();
        }
        draft_order["status"] = json!("COMPLETED");
        draft_order["completedAt"] = json!("2024-01-01T00:00:02.000Z");
        draft_order["order"] = order.clone();
        draft_order["orderId"] = json!(order_id.clone());
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.store.staged.orders.insert(order_id, order);
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_send_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| {
            field.name == "draftOrderInvoiceSend"
                || (field.name == "draftOrderCreate"
                    && draft_order_create_first_line_title(field).as_deref()
                        == Some("Invoice error parity item"))
        }) {
            return None;
        }

        for field in &fields {
            if field.name != "draftOrderInvoiceSend" {
                continue;
            }
            // Forward a hydrate + observe for a draft not created locally this
            // scenario so the invoice send operates on the real draft instead of a
            // precondition seed.
            if let Some(id) = resolved_string_field(&field.arguments, "id") {
                self.ensure_draft_order_hydrated(request, &id);
            }
            if let Some(template) = resolved_string_field(&field.arguments, "templateName") {
                if !is_valid_draft_order_invoice_template(&template) {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!(
                                "Variable $template of type DraftOrderEmailTemplate was provided invalid value {template}"
                            )
                        }]
                    })));
                }
            }
        }

        let mut declined = false;
        let data = root_payload_json(&fields, |field| {
            if declined {
                return None;
            }
            let value = match field.name.as_str() {
                "draftOrderCreate"
                    if draft_order_create_first_line_title(field).as_deref()
                        == Some("Invoice error parity item") =>
                {
                    Some(self.draft_order_invoice_errors_create(field, request, query, variables))
                }
                "draftOrderInvoiceSend" => {
                    Some(self.draft_order_invoice_errors_send(field, request, query, variables))
                }
                _ => None,
            };
            let Some(value) = value else {
                declined = true;
                return None;
            };
            Some(value)
        });
        if declined {
            return None;
        }
        Some(ok_json(json!({ "data": data })))
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = shopify_gid("DraftOrder", self.store.staged.next_draft_order_id);
        self.store.staged.next_draft_order_id += 1;
        let email = resolved_string_field(&input, "email")
            .filter(|email| !email.trim().is_empty())
            .map(Value::String)
            .unwrap_or(Value::Null);
        let shop_currency_code = self.store.shop_currency_code();
        let record = json!({
            "id": id,
            "name": "#D1",
            "status": "OPEN",
            "ready": true,
            "email": email,
            "note": Value::Null,
            "purchasingEntity": Value::Null,
            "customer": Value::Null,
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": [],
            "invoiceUrl": format!("https://shopify-draft-proxy.local/draft_orders/{id}/invoice"),
            "customAttributes": [],
            "appliedDiscount": Value::Null,
            "billingAddress": Value::Null,
            "shippingAddress": Value::Null,
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "subtotalPriceSet": money_set_pair("1.0", &shop_currency_code, "1.0", &shop_currency_code),
            "totalDiscountsSet": money_set_pair("0.0", &shop_currency_code, "0.0", &shop_currency_code),
            "totalShippingPriceSet": money_set_pair("0.0", &shop_currency_code, "0.0", &shop_currency_code),
            "totalPriceSet": money_set_pair("1.0", &shop_currency_code, "1.0", &shop_currency_code),
            "totalQuantityOfLineItems": 1,
            "lineItems": { "nodes": [draft_order_invoice_line_item()] }
        });
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), record.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderCreate",
            vec![id],
        );
        selected_json(
            &json!({
                "draftOrder": record,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_send(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [user_error_omit_code(Value::Null, "Draft order not found", None)],
                    "invoiceErrors": []
                }),
                &field.selection,
            );
        };

        // Invoice-send validation: a missing recipient yields "To can't be
        // blank", and a draft that has already been completed (paid) can no
        // longer have an invoice sent. Both conditions are checked so a
        // completed draft with no recipient surfaces both userErrors, in the
        // order Shopify reports them (recipient first, then the paid guard).
        let recipient_missing =
            draft_order_invoice_recipient(&field.arguments, &draft_order).is_none();
        let already_paid = draft_order["status"].as_str() == Some("COMPLETED");
        if recipient_missing || already_paid {
            let mut user_errors = Vec::new();
            let mut invoice_errors = Vec::new();
            if recipient_missing {
                user_errors.push(user_error_omit_code(Value::Null, "To can't be blank", None));
                invoice_errors.push(json!({
                    "code": "CUSTOMER_NO_EMAIL",
                    "message": "Customer email can't be blank"
                }));
            }
            if already_paid {
                user_errors.push(user_error_omit_code(
                    Value::Null,
                    "Draft order Invoice can't be sent. This draft order is already paid.",
                    None,
                ));
            }
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": user_errors,
                    "invoiceErrors": invoice_errors
                }),
                &field.selection,
            );
        }

        let mut updated = draft_order.clone();
        let invoice_sent_at = order_mutation_timestamp(self.log_entries.len() as u64);
        updated["status"] = json!("INVOICE_SENT");
        updated["invoiceSentAt"] = json!(invoice_sent_at.clone());
        updated["updatedAt"] = json!(invoice_sent_at);
        updated["__draftProxyInvoiceSend"] =
            draft_order_invoice_send_metadata(&field.arguments, &draft_order);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), updated.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoiceSend",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoiceSend safety validation.",
            },
        });
        selected_json(
            &json!({
                "draftOrder": updated,
                "userErrors": [],
                "invoiceErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_local_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        // Only claim bulk-tag mutations or a `draftOrder` read whose id is
        // actually tracked in this handler's tag state. A bare `draftOrder`
        // detail read of an untracked id must fall through to the lifecycle
        // handler / upstream passthrough rather than being shadowed with a
        // tags-only (or null) projection.
        let has_bulk_tag_root = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
            )
        });
        let has_managed_read = fields.iter().any(|field| {
            field.name == "draftOrder"
                && resolved_string_field(&field.arguments, "id")
                    .or_else(|| resolved_string_field(&field.arguments, "draftOrderId"))
                    .is_some_and(|id| {
                        self.store.staged.taggable_resources.contains_key(&id)
                            || self.store.staged.draft_order_tags.contains_key(&id)
                    })
        });
        if !has_bulk_tag_root && !has_managed_read {
            return None;
        }
        let mut declined = false;
        let data = root_payload_json(&fields, |field| {
            if declined {
                return None;
            }
            let value = match field.name.as_str() {
                "draftOrder" => Some(self.draft_order_bulk_tag_read(field)),
                "draftOrderBulkAddTags" => Some(self.draft_order_bulk_add_tags(field)),
                "draftOrderBulkRemoveTags" => Some(self.draft_order_bulk_remove_tags(field)),
                _ => None,
            };
            let Some(value) = value else {
                declined = true;
                return None;
            };
            Some(value)
        });
        if declined {
            return None;
        }
        Some(json!({ "data": data }))
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id")
            .or_else(|| resolved_string_field(&field.arguments, "draftOrderId"))
        else {
            return Value::Null;
        };
        if let Some(record) = self.store.staged.taggable_resources.get(&id) {
            return selected_json(record, &field.selection);
        }
        let value = self
            .store
            .staged
            .draft_order_tags
            .get(&id)
            .map(|tags| json!({ "id": id, "tags": tags }))
            .unwrap_or(Value::Null);
        selected_json(&value, &field.selection)
    }

    pub(in crate::proxy) fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.store.staged.next_draft_order_bulk_tag_job_id;
        self.store.staged.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": shopify_gid("Job", id), "done": false })
    }

    pub(in crate::proxy) fn draft_order_bulk_add_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags = resolved_string_list_arg(&field.arguments, "tags");
        let normalized_tags: Vec<(String, String)> = tags
            .iter()
            .map(|tag| (normalize_draft_order_tag(tag), tag.trim().to_string()))
            .collect();

        let mut user_errors = Vec::new();
        for (index, (_, tag)) in normalized_tags.iter().enumerate() {
            if tag.chars().count() >= 256 {
                user_errors.push(user_error(
                    vec!["input".to_string(), "tags".to_string(), index.to_string()],
                    "tag_too_long",
                    Some("INVALID"),
                ));
            }
        }

        let mut valid_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_order_tags.contains_key(id) {
                valid_ids.push(id.clone());
            } else {
                user_errors.push(user_error(
                    vec!["input".to_string(), "ids".to_string(), index.to_string()],
                    "Draft order does not exist",
                    Some("NOT_FOUND"),
                ));
            }
        }

        let too_many = valid_ids.iter().any(|id| {
            let current = self
                .store
                .staged
                .draft_order_tags
                .get(id)
                .cloned()
                .unwrap_or_default();
            let mut identities: BTreeSet<String> = current
                .iter()
                .map(|tag| normalize_draft_order_tag(tag))
                .collect();
            for (identity, _) in &normalized_tags {
                identities.insert(identity.clone());
            }
            identities.len() > 250
        });
        if too_many {
            user_errors.clear();
            user_errors.push(user_error(
                ["input", "tags"],
                "too_many_tags",
                Some("INVALID"),
            ));
            return selected_json(
                &json!({ "job": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }

        if !normalized_tags
            .iter()
            .any(|(_, tag)| tag.chars().count() >= 256)
        {
            let mut updated_ids = Vec::new();
            for id in valid_ids {
                if let Some(current) = self.store.staged.draft_order_tags.get_mut(&id) {
                    let mut existing: BTreeSet<String> = current
                        .iter()
                        .map(|tag| normalize_draft_order_tag(tag))
                        .collect();
                    for (identity, tag) in &normalized_tags {
                        if existing.insert(identity.clone()) {
                            current.push(tag.clone());
                        }
                    }
                    current.sort_by_key(|tag| normalize_draft_order_tag(tag));
                    updated_ids.push(id);
                }
            }
            for id in updated_ids {
                self.sync_draft_order_record_tags(&id);
            }
        }

        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_remove_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags: BTreeSet<String> = resolved_string_list_arg(&field.arguments, "tags")
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();
        let mut user_errors = Vec::new();
        let mut updated_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(current) = self.store.staged.draft_order_tags.get_mut(id) {
                current.retain(|tag| !tags.contains(&normalize_draft_order_tag(tag)));
                updated_ids.push(id.clone());
            } else {
                user_errors.push(user_error(
                    vec!["input".to_string(), "ids".to_string(), index.to_string()],
                    "Draft order does not exist",
                    Some("NOT_FOUND"),
                ));
            }
        }
        for id in updated_ids {
            self.sync_draft_order_record_tags(&id);
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }
}

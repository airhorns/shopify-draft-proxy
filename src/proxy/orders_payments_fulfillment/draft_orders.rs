use super::*;

mod helpers;
pub(in crate::proxy) use self::helpers::*;

fn merge_draft_order_string_field(
    draft_order: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) {
    if input.contains_key(field) {
        draft_order[field] = resolved_string_field(input, field)
            .map(Value::String)
            .unwrap_or(Value::Null);
    }
}

fn draft_order_complete_payload(draft_order: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "draftOrder": draft_order,
        "userErrors": user_errors
    })
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
    let presentment_currency = draft_order_input_currency(input, shop_currency_code);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes =
        draft_order_line_items(&line_items, id, &presentment_currency, variant_hydrations);
    let original_subtotal = sum_money_set(&line_item_nodes, "originalTotalSet");
    let mut record = draft_order_record_skeleton(id, name, line_item_nodes);
    record["currencyCode"] = json!(shop_currency_code);
    record["presentmentCurrencyCode"] = json!(presentment_currency);
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
    draft_order_explicit_input_currency(input).unwrap_or_else(|| shop_currency_code.to_string())
}

fn draft_order_explicit_input_currency(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(input, "presentmentCurrencyCode")
        .or_else(|| resolved_string_field(input, "currencyCode"))
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
    let Some(entity) = resolved_object_field(input, "purchasingEntity") else {
        return Value::Null;
    };
    if let Some(company) = resolved_object_field(&entity, "purchasingCompany") {
        return json!({
            "__typename": "PurchasingCompany",
            "company": resolved_string_field(&company, "companyId")
                .map(|id| json!({ "id": id }))
                .unwrap_or(Value::Null),
            "contact": resolved_string_field(&company, "companyContactId")
                .map(|id| json!({ "id": id }))
                .unwrap_or(Value::Null),
            "location": resolved_string_field(&company, "companyLocationId")
                .map(|id| json!({ "id": id }))
                .unwrap_or(Value::Null)
        });
    }
    resolved_string_field(&entity, "customerId")
        .map(|id| json!({ "__typename": "Customer", "id": id }))
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn draft_order_customer(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let display_name = resolved_string_field(input, "email").unwrap_or_default();
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
        .map(|id| {
            json!({
                "id": id,
                "email": resolved_string_field(input, "email"),
                "displayName": display_name
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

fn draft_order_matches_count_query(draft_order: &Value, query: Option<&str>) -> bool {
    matches!(
        draft_order_search_decision(draft_order, query),
        StagedSearchDecision::Match
    )
}

fn draft_order_count_baseline_key(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    if arguments.is_empty() {
        return "args:{}".to_string();
    }
    arguments
        .iter()
        .map(|(name, value)| format!("{name}:{}", resolved_value_json(value)))
        .collect::<Vec<_>>()
        .join("\n")
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
        .is_some_and(|id| resource_id_matches_gid_or_tail(id, value))
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
    .any(|id| resource_id_matches_gid_or_tail(id, value))
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

fn draft_order_matches_money_comparator(actual: Option<f64>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = search_comparator(query_value);
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
    let query_value = normalized_search_query_value(query_value);
    if query_value.is_empty() {
        return true;
    }
    if let Some(prefix) = query_value.strip_suffix('*') {
        return ascii_word_starts_with(&actual, prefix);
    }
    actual.contains(&query_value)
}

fn draft_order_search_token_matches(actual: &str, query_value: &str) -> bool {
    let query_value = normalized_search_query_value(query_value);
    if query_value.is_empty() {
        return true;
    }
    let actual = actual.to_ascii_lowercase();
    if let Some(prefix) = query_value.strip_suffix('*') {
        return ascii_word_starts_with(&actual, prefix);
    }
    actual
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part == query_value)
        || actual == query_value
}

fn draft_order_gid_tail_sort_value(draft_order: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(draft_order.get("id").and_then(Value::as_str))
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
    now: time::OffsetDateTime,
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
        if resolved_string_field(input, "email")
            .is_some_and(|email| !shopify_email_is_valid(&email, EmailValidationMode::AtSign))
        {
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
        .is_some_and(|value| draft_order_reserve_inventory_until_is_past(value, now))
    {
        return Some(vec![user_error_omit_code(
            Value::Null,
            "Reserve until can't be in the past",
            None,
        )]);
    }
    None
}

fn draft_order_reserve_inventory_until_is_past(value: &str, now: time::OffsetDateTime) -> bool {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .is_ok_and(|reserve_until| reserve_until < now)
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
    if resolved_string_field(input, "email")
        .is_some_and(|email| !shopify_email_is_valid(&email, EmailValidationMode::AtSign))
    {
        return Some(vec![user_error_omit_code(
            ["email"],
            "Email is invalid",
            None,
        )]);
    }
    None
}

pub(in crate::proxy) fn draft_order_top_level_validation_errors(
    root_field: &str,
    response_key: &str,
    root_location: SourceLocation,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if !matches!(
        root_field,
        "draftOrderCreate" | "draftOrderUpdate" | "draftOrderCalculate"
    ) {
        return errors;
    }
    let Some(input) = resolved_object_field(arguments, "input") else {
        return errors;
    };
    let line_item_count = resolved_list_len(&input, "lineItems");
    if line_item_count > 499 {
        errors.push(draft_order_max_input_error(
            response_key,
            root_location,
            "lineItems",
            line_item_count,
            499,
        ));
    }
    let tag_count = resolved_list_len(&input, "tags");
    if tag_count > 250 {
        errors.push(draft_order_max_input_error(
            response_key,
            root_location,
            "tags",
            tag_count,
            250,
        ));
    }
    errors
}

pub(in crate::proxy) fn draft_order_max_input_error(
    response_key: &str,
    root_location: SourceLocation,
    argument: &str,
    count: usize,
    max: usize,
) -> Value {
    max_input_size_exceeded_error(
        vec![
            response_key.to_string(),
            "input".to_string(),
            argument.to_string(),
        ],
        count,
        max,
        Some(json!([{ "line": root_location.line, "column": root_location.column }])),
    )
}

impl DraftProxy {
    fn observe_live_hybrid_draft_order_read(&mut self, request: &Request) {
        let response = self
            .execution_session
            .upstream_query_response
            .clone()
            .unwrap_or_else(|| (self.upstream_transport)(request.clone()));
        self.observe_draft_order_read_response(request, &response);
    }

    fn observe_live_hybrid_draft_order_root(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        match root_field {
            "draftOrder" => {
                let Some(id) = resolved_string_field(arguments, "id") else {
                    return;
                };
                if !self.store.staged.draft_orders.contains_key(&id)
                    && !self.store.staged.draft_orders.is_tombstoned(&id)
                    && !self.store.base.draft_orders.records.contains_key(&id)
                {
                    self.ensure_draft_order_hydrated(request, &id);
                }
            }
            "draftOrdersCount" => {
                let key = draft_order_count_baseline_key(arguments);
                if self.store.draft_order_count_baseline(&key).is_none() {
                    self.observe_live_hybrid_draft_order_read(request);
                }
            }
            "draftOrders" => self.observe_live_hybrid_draft_order_read(request),
            _ => {}
        }
    }

    pub(in crate::proxy) fn observe_draft_order_read_response(
        &mut self,
        request: &Request,
        response: &Response,
    ) {
        if response.status >= 400 {
            return;
        }
        self.observe_draft_order_read_data(request, &response.body["data"]);
    }

    pub(in crate::proxy) fn observe_draft_order_read_data(
        &mut self,
        request: &Request,
        data: &Value,
    ) {
        let body = json!({ "data": data });
        if let Some(graphql_request) = parse_graphql_request_body(&request.body) {
            if let Some(fields) = root_fields(&graphql_request.query, &graphql_request.variables) {
                self.observe_draft_order_count_baselines(&fields, &body);
                self.observe_singular_draft_order_roots(&fields, &body);
            }
        }
        self.observe_draft_order_value(data);
    }

    fn observe_singular_draft_order_roots(&mut self, fields: &[RootFieldSelection], body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for field in fields {
            if field.name != "draftOrder" {
                continue;
            }
            let Some(id) = resolved_string_field(&field.arguments, "id") else {
                continue;
            };
            if !is_shopify_gid_of_type(&id, "DraftOrder") {
                continue;
            }
            let Some(value) = data.get(&field.response_key) else {
                continue;
            };
            if !value.is_object() {
                continue;
            }
            let mut draft_order = value.clone();
            if draft_order.get("id").and_then(Value::as_str).is_none() {
                if let Some(object) = draft_order.as_object_mut() {
                    object.insert("id".to_string(), json!(id));
                }
            }
            self.observe_draft_order_value(&draft_order);
        }
    }

    fn observe_draft_order_count_baselines(&mut self, fields: &[RootFieldSelection], body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for field in fields {
            if field.name != "draftOrdersCount" {
                continue;
            }
            let Some(count) = data.get(&field.response_key) else {
                continue;
            };
            if count.get("count").and_then(Value::as_u64).is_some() {
                self.store.observe_draft_order_count_baseline(
                    draft_order_count_baseline_key(&field.arguments),
                    count.clone(),
                );
            }
        }
    }

    fn observe_draft_order_value(&mut self, value: &Value) {
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if is_shopify_gid_of_type(id, "DraftOrder") {
                self.store.observe_base_draft_order(value.clone());
                return;
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    self.observe_draft_order_value(value);
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    self.observe_draft_order_value(value);
                }
            }
            _ => {}
        }
    }

    fn tombstone_observed_draft_order(&mut self, id: &str) -> bool {
        if self.store.observed_draft_order_by_id(id).is_none() {
            return false;
        }
        self.store.staged.draft_orders.remove_staged(id);
        self.store.staged.draft_orders.tombstone(id.to_string());
        true
    }

    pub(in crate::proxy) fn draft_order_complete_local_outcome(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        raw_arguments: &BTreeMap<String, RawArgumentValue>,
    ) -> Option<ResolverOutcome<Value>> {
        // Forward a hydrate + observe for a draft not created locally this scenario
        // so completion settles the real draft instead of a precondition seed.
        if root_field == "draftOrderComplete" {
            if let Some(id) = resolved_string_field(arguments, "id") {
                self.ensure_draft_order_hydrated(request, &id);
            }
        }

        match root_field {
            "draftOrderComplete" => Some(ResolverOutcome::value(self.complete_staged_draft_order(
                request,
                arguments,
                raw_arguments,
            ))),
            "order" => {
                let id = resolved_string_field(arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(ResolverOutcome::value(order.clone()))
            }
            "orders" => {
                let query_arg = resolved_string_field(arguments, "query").unwrap_or_default();
                // This local overlay can only resolve a single `name:` look-up against
                // orders staged in this scenario (the draft-complete read-back). Catalog
                // reads — tag/status filters, sort/window/count, or a cold empty catalog —
                // must forward upstream and observe, so decline anything that isn't a
                // name look-up.
                if !query_arg.starts_with("name:") {
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
                    .cloned()
                    .collect::<Vec<_>>();
                Some(ResolverOutcome::value(order_connection(nodes)))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn draft_order_lifecycle_local_outcome(
        &mut self,
        context: &OrderRootContext<'_>,
    ) -> Option<ResolverOutcome<Value>> {
        let request = context.request;
        let root_field = context.root_field;
        let arguments = context.arguments;
        let root_location = context.root_location;
        let query = context.query;
        let variables = context.variables;
        let response_key = context.response_key;
        let is_lifecycle_root = matches!(
            root_field,
            "draftOrderCreate"
                | "draftOrderUpdate"
                | "draftOrderCalculate"
                | "draftOrderDuplicate"
                | "draftOrderDelete"
                | "draftOrderBulkDelete"
                | "draftOrderCreateFromOrder"
                | "draftOrderInvoicePreview"
        );
        let is_read = matches!(
            root_field,
            "draftOrder" | "draftOrders" | "draftOrdersCount"
        );
        if !is_lifecycle_root && !is_read {
            return None;
        }
        if is_read {
            let has_local_read = match root_field {
                "draftOrder" => resolved_string_field(arguments, "id").is_some_and(|id| {
                    self.store.staged.draft_orders.contains_key(&id)
                        || self.store.staged.draft_orders.is_tombstoned(&id)
                        || self.store.observed_draft_order_by_id(&id).is_some()
                        || !self.store.staged.draft_orders.is_empty()
                }),
                "draftOrders" | "draftOrdersCount" => !self.store.staged.draft_orders.is_empty(),
                _ => false,
            };
            if !has_local_read {
                return None;
            }
            self.observe_live_hybrid_draft_order_root(request, root_field, arguments);
        }

        let validation_errors = draft_order_top_level_validation_errors(
            root_field,
            response_key,
            root_location,
            arguments,
        );
        if !validation_errors.is_empty() {
            return Some(ResolverOutcome::value(Value::Null).with_errors(
                root_field_errors_from_json(&validation_errors, response_key),
            ));
        }

        match root_field {
            "draftOrderUpdate" | "draftOrderDuplicate" => {
                if let Some(id) = resolved_string_field(arguments, "id") {
                    self.ensure_draft_order_hydrated(request, &id);
                }
            }
            "draftOrderDelete" => {
                let input = resolved_object_field(arguments, "input").unwrap_or_default();
                if let Some(id) = resolved_string_field(&input, "id")
                    .or_else(|| resolved_string_field(arguments, "id"))
                {
                    self.ensure_draft_order_hydrated(request, &id);
                }
            }
            "draftOrderBulkDelete" => {
                for id in self.draft_order_bulk_target_ids(arguments) {
                    self.ensure_draft_order_hydrated(request, &id);
                }
            }
            "draftOrderCreateFromOrder" => {
                if let Some(order_id) = resolved_string_field(arguments, "orderId") {
                    self.ensure_order_hydrated(request, &order_id);
                }
            }
            "draftOrderInvoicePreview" | "draftOrder" => {
                if let Some(id) = resolved_string_field(arguments, "id") {
                    self.ensure_draft_order_hydrated(request, &id);
                }
            }
            _ => {}
        }

        let value = match root_field {
            "draftOrderCreate" => {
                self.stage_draft_order_create(request, query, variables, arguments)
            }
            "draftOrderUpdate" => {
                self.stage_draft_order_update(request, query, variables, arguments)
            }
            "draftOrderCalculate" => self.calculate_draft_order_payload(request, arguments),
            "draftOrderDuplicate" => {
                self.stage_draft_order_duplicate(request, query, variables, arguments)
            }
            "draftOrderDelete" => {
                self.stage_draft_order_delete(request, query, variables, arguments)
            }
            "draftOrderBulkDelete" => {
                self.stage_draft_order_bulk_delete(request, query, variables, arguments)
            }
            "draftOrderCreateFromOrder" => {
                self.stage_draft_order_create_from_order(request, query, variables, arguments)
            }
            "draftOrderInvoicePreview" => {
                self.draft_order_invoice_preview_payload(request, query, variables, arguments)
            }
            "draftOrder" => self.staged_draft_order_read(arguments),
            "draftOrders" => self.staged_draft_orders_connection(arguments),
            "draftOrdersCount" => self.staged_draft_orders_count(arguments),
            _ => return None,
        };
        Some(ResolverOutcome::value(value))
    }

    pub(super) fn stage_draft_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false, self.current_time())
        {
            return json!({ "draftOrder": Value::Null, "userErrors": user_errors });
        }
        let (variant_hydrations, unavailable_variant_ids) =
            self.draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
        if let Some(user_errors) =
            draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
        {
            return json!({ "draftOrder": Value::Null, "userErrors": user_errors });
        }
        // Even when the input supplies a presentment currency, Shopify's
        // `shopMoney` fields are denominated in the shop currency. A cold
        // LiveHybrid proxy therefore needs the shop pricing slice before it can
        // build a valid draft-order money graph.
        self.hydrate_shop_pricing_state_if_missing(request, true, false);
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
        json!({ "draftOrder": draft_order, "userErrors": [] })
    }

    pub(super) fn stage_draft_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, true, self.current_time())
        {
            return json!({ "draftOrder": Value::Null, "userErrors": user_errors });
        }
        let Some(existing) = self.store.observed_draft_order_by_id(&id).cloned() else {
            return draft_order_not_found_payload("draftOrder");
        };
        let variant_hydrations = if input.contains_key("lineItems") {
            let (variant_hydrations, unavailable_variant_ids) = self
                .draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
            if let Some(user_errors) =
                draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
            {
                return json!({ "draftOrder": Value::Null, "userErrors": user_errors });
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
        json!({ "draftOrder": updated, "userErrors": [] })
    }

    pub(super) fn calculate_draft_order_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false, self.current_time())
        {
            return json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors });
        }
        if let Some(user_errors) = draft_order_calculate_user_errors(&input) {
            return json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors });
        }
        let (variant_hydrations, unavailable_variant_ids) =
            self.draft_order_variant_hydrations(request, draft_order_line_item_variant_ids(&input));
        if let Some(user_errors) =
            draft_order_unavailable_variant_user_errors(&input, &unavailable_variant_ids)
        {
            return json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors });
        }
        self.hydrate_shop_pricing_state_if_missing(request, true, false);
        let calculated = draft_order_calculated_record(
            &input,
            &variant_hydrations,
            &self.b2b_order_input_currency_default(&input),
        );
        json!({ "calculatedDraftOrder": calculated, "userErrors": [] })
    }

    pub(super) fn stage_draft_order_duplicate(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(source) = self.store.observed_draft_order_by_id(&id).cloned() else {
            return draft_order_not_found_payload("draftOrder");
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
        json!({ "draftOrder": duplicate, "userErrors": [] })
    }

    pub(super) fn stage_draft_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .or_else(|| resolved_string_field(arguments, "id"))
            .unwrap_or_default();
        if !self.tombstone_observed_draft_order(&id) {
            return draft_order_not_found_payload("deletedId");
        }
        self.store.staged.draft_order_tags.remove(&id);
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "draftOrderDelete",
            vec![id.clone()],
        );
        json!({ "deletedId": id, "userErrors": [] })
    }

    pub(super) fn stage_draft_order_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let ids = self.draft_order_bulk_target_ids(arguments);
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.tombstone_observed_draft_order(id) {
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
        json!({ "job": job, "userErrors": user_errors })
    }

    pub(super) fn stage_draft_order_create_from_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let order_id = resolved_string_field(arguments, "orderId").unwrap_or_default();
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return json!({
                "draftOrder": Value::Null,
                "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
            });
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
        json!({ "draftOrder": draft_order, "userErrors": [] })
    }

    pub(super) fn draft_order_invoice_preview_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.observed_draft_order_by_id(&id).cloned() else {
            return json!({
                "previewSubject": Value::Null,
                "previewHtml": Value::Null,
                "userErrors": [user_error_omit_code(["id"], "Draft order not found", None)]
            });
        };
        let email = resolved_object_field(arguments, "email").unwrap_or_default();
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
        json!({ "previewSubject": subject, "previewHtml": html, "userErrors": [] })
    }

    pub(super) fn staged_draft_order_read(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(id) = resolved_string_field(arguments, "id") else {
            return Value::Null;
        };
        self.store
            .observed_draft_order_by_id(&id)
            .map(|draft_order| self.payment_terms_owner_record_with_effective_due(draft_order))
            .unwrap_or(Value::Null)
    }

    pub(super) fn matching_draft_orders_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        let arguments = self.draft_order_arguments_with_saved_search_query(arguments);
        staged_connection_query(
            self.store.effective_draft_orders(),
            &arguments,
            draft_order_search_decision,
            draft_order_staged_sort_key,
            value_id_cursor,
        )
    }

    fn draft_order_arguments_with_saved_search_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> BTreeMap<String, ResolvedValue> {
        let Some(saved_search_id) = resolved_string_field(arguments, "savedSearchId") else {
            return arguments.clone();
        };
        let mut merged = arguments.clone();
        merged.remove("savedSearchId");
        let Some(saved_search_query) = self
            .store
            .saved_search_by_id(&saved_search_id)
            .filter(|record| record.resource_type == "DRAFT_ORDER")
            .map(|record| record.query)
        else {
            return merged;
        };
        let argument_query = resolved_string_field(arguments, "query").unwrap_or_default();
        let query = match (
            saved_search_query.trim().is_empty(),
            argument_query.trim().is_empty(),
        ) {
            (true, true) => String::new(),
            (true, false) => argument_query,
            (false, true) => saved_search_query,
            (false, false) => format!("{saved_search_query} {argument_query}"),
        };
        merged.insert("query".to_string(), ResolvedValue::String(query));
        merged
    }

    pub(super) fn staged_draft_orders_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let result = self.matching_draft_orders_query(arguments);
        let records = result
            .records
            .iter()
            .map(|draft_order| self.payment_terms_owner_record_with_effective_due(draft_order))
            .collect::<Vec<_>>();
        connection_json_with_cursor(records, |_, node| value_id_cursor(node), result.page_info)
    }

    pub(super) fn staged_draft_orders_count(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.effective_draft_orders_count_value(arguments)
    }

    fn effective_draft_orders_count_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let baseline_key = draft_order_count_baseline_key(arguments);
        let Some(baseline) = self.store.draft_order_count_baseline(&baseline_key) else {
            return count_object(self.matching_draft_orders_query(arguments).total_count);
        };
        let Some(base_count) = baseline.get("count").and_then(Value::as_u64) else {
            return count_object(self.matching_draft_orders_query(arguments).total_count);
        };
        let delta = self.staged_draft_order_count_delta(
            &self.draft_order_arguments_with_saved_search_query(arguments),
        );
        let effective_total = if delta.is_negative() {
            (base_count as usize).saturating_sub(delta.unsigned_abs())
        } else {
            (base_count as usize).saturating_add(delta as usize)
        };
        let mut count = count_object(effective_total);
        if baseline.get("precision").and_then(Value::as_str) == Some("AT_LEAST") {
            count["precision"] = json!("AT_LEAST");
        }
        count
    }

    fn staged_draft_order_count_delta(&self, arguments: &BTreeMap<String, ResolvedValue>) -> isize {
        let query = resolved_string_field(arguments, "query");
        let mut delta = 0isize;
        for id in &self.store.staged.draft_orders.tombstones {
            if let Some(base_draft_order) = self.store.base.draft_orders.records.get(id) {
                if draft_order_matches_count_query(base_draft_order, query.as_deref()) {
                    delta -= 1;
                }
            }
        }
        for (id, staged_draft_order) in &self.store.staged.draft_orders.records {
            if self.store.staged.draft_orders.is_tombstoned(id) {
                continue;
            }
            let staged_matches =
                draft_order_matches_count_query(staged_draft_order, query.as_deref());
            if let Some(base_draft_order) = self.store.base.draft_orders.records.get(id) {
                let base_matches =
                    draft_order_matches_count_query(base_draft_order, query.as_deref());
                delta += staged_matches as isize - base_matches as isize;
            } else if let Some(base_draft_order) = self
                .store
                .base_draft_order_logical_duplicate_for_staged(id, staged_draft_order)
            {
                let base_matches =
                    draft_order_matches_count_query(base_draft_order, query.as_deref());
                delta += staged_matches as isize - base_matches as isize;
            } else if staged_matches {
                delta += 1;
            }
        }
        delta
    }

    pub(super) fn next_draft_order_id(&mut self) -> String {
        let mut id = shopify_gid("DraftOrder", self.store.staged.next_draft_order_id);
        while self.store.base.draft_orders.records.contains_key(&id)
            || self.store.staged.draft_orders.contains_staged(&id)
            || self.store.staged.draft_orders.is_tombstoned(&id)
        {
            self.store.staged.next_draft_order_id += 1;
            id = shopify_gid("DraftOrder", self.store.staged.next_draft_order_id);
        }
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
            &self.b2b_order_input_currency_default(input),
        );
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    pub(super) fn ensure_draft_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.draft_orders.contains_key(id)
            || self.store.staged.draft_orders.is_tombstoned(id)
            || self.store.base.draft_orders.records.contains_key(id)
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
        self.store.observe_base_draft_order(draft_order);
    }

    pub(super) fn ensure_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.orders.is_tombstoned(id)
        {
            return;
        }
        // Always attempt a fresh upstream read so the order reflects its live
        // state at the time of this operation. A precondition seed may hold an
        // earlier snapshot of the same order (e.g. the total captured the moment
        // a draft was completed in setup, before the store recalculated
        // tax/shipping), so the recorded hydrate is authoritative when present.
        // On a cassette miss / non-2xx response we keep whatever record is
        // already staged rather than dropping it.
        let mut line_items_after: Option<String> = None;
        let mut seen_cursors = BTreeSet::new();
        let mut hydrated_order: Option<Value> = None;
        let mut hydrated_line_items = Vec::new();
        let mut first_line_item_cursor: Option<String> = None;
        let mut last_line_item_cursor: Option<String>;

        loop {
            let response = self.upstream_post(
                request,
                json!({
                    "query": ORDER_HYDRATE_QUERY,
                    "operationName": "OrdersOrderHydrate",
                    "variables": { "id": id, "lineItemsAfter": line_items_after.clone() }
                }),
            );
            if !(200..300).contains(&response.status) {
                return;
            }
            let order = response.body["data"]["order"].clone();
            if !order.is_object() {
                return;
            }

            let line_items = order["lineItems"].clone();
            let mut page_nodes = connection_nodes(&line_items);
            hydrated_line_items.append(&mut page_nodes);

            let page_info = &line_items["pageInfo"];
            if first_line_item_cursor.is_none() {
                first_line_item_cursor = page_info["startCursor"].as_str().map(str::to_string);
            }
            last_line_item_cursor = page_info["endCursor"].as_str().map(str::to_string);
            let has_next_page = page_info["hasNextPage"].as_bool().unwrap_or(false);

            if hydrated_order.is_none() {
                hydrated_order = Some(order);
            }

            if !has_next_page {
                break;
            }

            let Some(next_cursor) = page_info["endCursor"].as_str().map(str::to_string) else {
                return;
            };
            if next_cursor.is_empty() || !seen_cursors.insert(next_cursor.clone()) {
                return;
            }
            line_items_after = Some(next_cursor);
        }

        let Some(mut order) = hydrated_order else {
            return;
        };
        order["lineItems"]["nodes"] = json!(hydrated_line_items);
        order["lineItems"]["pageInfo"] =
            connection_page_info(false, false, first_line_item_cursor, last_line_item_cursor);
        normalize_hydrated_order(&mut order);
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
        let mut upstream_ids = Vec::new();
        for id in ids {
            if let Some(variant) = self.draft_order_variant_hydration_from_store(&id) {
                hydrations.insert(id, variant);
                continue;
            }
            if self.config.read_mode == ReadMode::Snapshot {
                unavailable_ids.insert(id);
                continue;
            }
            upstream_ids.push(id);
        }
        if upstream_ids.is_empty() {
            return (hydrations, unavailable_ids);
        }
        if upstream_ids.len() == 1 {
            let id = upstream_ids.remove(0);
            let response = self.upstream_post(
                request,
                json!({
                    "query": DRAFT_ORDER_VARIANT_HYDRATE_QUERY,
                    "operationName": "OrdersDraftOrderVariantHydrate",
                    "variables": { "id": id }
                }),
            );
            if !(200..300).contains(&response.status) {
                return (hydrations, unavailable_ids);
            }
            let variant = response.body["data"]["productVariant"].clone();
            if variant.is_object() {
                hydrations.insert(id, variant);
            } else {
                unavailable_ids.insert(id);
            }
            return (hydrations, unavailable_ids);
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DRAFT_ORDER_VARIANTS_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderVariantsHydrate",
                "variables": { "ids": upstream_ids.clone() }
            }),
        );
        if !(200..300).contains(&response.status) {
            return (hydrations, unavailable_ids);
        }
        let Some(nodes) = response.body["data"]["nodes"].as_array() else {
            unavailable_ids.extend(upstream_ids);
            return (hydrations, unavailable_ids);
        };
        for (index, id) in upstream_ids.iter().enumerate() {
            let Some(variant) = nodes.get(index) else {
                unavailable_ids.insert(id.clone());
                continue;
            };
            if variant["__typename"].as_str() == Some("ProductVariant") && variant.is_object() {
                hydrations.insert(id.clone(), variant.clone());
            } else {
                unavailable_ids.insert(id.clone());
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
        for field in ["email", "note", "sourceName"] {
            merge_draft_order_string_field(&mut draft_order, input, field);
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
        let shop_currency = draft_order_currency(draft_order, &self.store.shop_currency_code());
        let presentment_currency = draft_order["presentmentCurrencyCode"]
            .as_str()
            .unwrap_or(&shop_currency)
            .to_string();
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
        draft_order["lineItemsSubtotalPrice"] =
            money_bag_from_amount(original_subtotal, &shop_currency, &presentment_currency);
        draft_order["subtotalPriceSet"] =
            money_bag_from_amount(subtotal, &shop_currency, &presentment_currency);
        draft_order["totalTax"] = json!(format!("{:.2}", (tax_total * 100.0).round() / 100.0));
        draft_order["totalTaxSet"] =
            money_bag_from_amount(tax_total, &shop_currency, &presentment_currency);
        draft_order["totalDiscountsSet"] =
            money_bag_from_amount(discount_total, &shop_currency, &presentment_currency);
        draft_order["totalShippingPriceSet"] =
            money_bag_from_amount(shipping_total, &shop_currency, &presentment_currency);
        draft_order["totalPriceSet"] =
            money_bag_from_amount(total, &shop_currency, &presentment_currency);
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

    pub(super) fn draft_order_bulk_target_ids(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<String> {
        let mut ids = resolved_string_list_arg(arguments, "ids");
        if ids.is_empty() && resolved_string_field(arguments, "search").is_some() {
            ids = self
                .store
                .effective_draft_orders()
                .iter()
                .filter_map(|draft_order| draft_order.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
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
        arguments: &BTreeMap<String, ResolvedValue>,
        raw_arguments: &BTreeMap<String, RawArgumentValue>,
    ) -> Value {
        let Some(id) = resolved_string_field(arguments, "id") else {
            return draft_order_complete_payload(
                Value::Null,
                vec![user_error_omit_code(["id"], "ID is required", None)],
            );
        };
        let Some(mut draft_order) = self.store.observed_draft_order_by_id(&id).cloned() else {
            return draft_order_complete_payload(
                Value::Null,
                vec![user_error_omit_code(
                    ["id"],
                    "Draft order does not exist",
                    None,
                )],
            );
        };
        if draft_order.get("__draftProxyLineItems").is_none() {
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        if draft_order["status"].as_str() == Some("COMPLETED") {
            return draft_order_complete_payload(
                draft_order,
                vec![user_error_omit_code(
                    Value::Null,
                    "This order has been paid",
                    None,
                )],
            );
        }
        let payment_gateway_id = resolved_string_field(arguments, "paymentGatewayId");
        if payment_gateway_id.is_some() {
            return draft_order_complete_payload(
                draft_order,
                vec![user_error_omit_code(
                    Value::Null,
                    "Invalid payment gateway",
                    None,
                )],
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
        let payment_pending = if raw_arguments.contains_key("paymentPending") {
            matches!(
                arguments.get("paymentPending"),
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
            "email": draft_order["email"].clone(),
            "sourceName": source_name,
            "note": order_note,
            "tags": order_tags,
            "currencyCode": currency_code,
            "presentmentCurrencyCode": currency_code,
            "paymentGatewayNames": payment_gateway_names,
            "transactions": order_transactions,
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
        draft_order_complete_payload(draft_order, vec![])
    }

    pub(in crate::proxy) fn draft_order_invoice_send_local_outcome(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        if root_field != "draftOrderInvoiceSend" {
            return None;
        }
        // Forward a hydrate + observe for a draft not created locally this
        // scenario so the invoice send operates on the real draft instead of a
        // precondition seed.
        if let Some(id) = resolved_string_field(arguments, "id") {
            self.ensure_draft_order_hydrated(request, &id);
        }

        Some(ResolverOutcome::value(
            self.draft_order_invoice_errors_send(arguments, request, query, variables),
        ))
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_send(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.observed_draft_order_by_id(&id).cloned() else {
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
            return json!({
                "draftOrder": Value::Null,
                "userErrors": [user_error_omit_code(Value::Null, "Draft order not found", None)],
                "invoiceErrors": []
            });
        };

        // Invoice-send validation: a missing recipient yields "To can't be
        // blank", and a draft that has already been completed (paid) can no
        // longer have an invoice sent. Both conditions are checked so a
        // completed draft with no recipient surfaces both userErrors, in the
        // order Shopify reports them (recipient first, then the paid guard).
        let recipient_missing = draft_order_invoice_recipient(arguments, &draft_order).is_none();
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
            return json!({
                "draftOrder": draft_order,
                "userErrors": user_errors,
                "invoiceErrors": invoice_errors
            });
        }

        let mut updated = draft_order.clone();
        let invoice_sent_at = order_mutation_timestamp(self.mutation_log_ordinal() as u64);
        updated["status"] = json!("INVOICE_SENT");
        updated["invoiceSentAt"] = json!(invoice_sent_at.clone());
        updated["updatedAt"] = json!(invoice_sent_at);
        updated["__draftProxyInvoiceSend"] =
            draft_order_invoice_send_metadata(arguments, &draft_order);
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
        json!({
            "draftOrder": updated,
            "userErrors": [],
            "invoiceErrors": []
        })
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_local_outcome(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<ResolverOutcome<Value>> {
        // Only claim bulk-tag mutations or a `draftOrder` read whose id is
        // actually tracked in this handler's tag state. A bare `draftOrder`
        // detail read of an untracked id must fall through to the lifecycle
        // handler / upstream passthrough rather than being shadowed with a
        // tags-only (or null) projection.
        let has_bulk_tag_root = matches!(
            root_field,
            "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
        );
        let has_managed_read = root_field == "draftOrder"
            && resolved_string_field(arguments, "id")
                .or_else(|| resolved_string_field(arguments, "draftOrderId"))
                .is_some_and(|id| {
                    self.store.staged.taggable_resources.contains_key(&id)
                        || self.store.staged.draft_order_tags.contains_key(&id)
                });
        if !has_bulk_tag_root && !has_managed_read {
            return None;
        }
        let value = match root_field {
            "draftOrder" => self.draft_order_bulk_tag_read(arguments),
            "draftOrderBulkAddTags" => self.draft_order_bulk_add_tags(arguments),
            "draftOrderBulkRemoveTags" => self.draft_order_bulk_remove_tags(arguments),
            _ => return None,
        };
        Some(ResolverOutcome::value(value))
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_read(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(id) = resolved_string_field(arguments, "id")
            .or_else(|| resolved_string_field(arguments, "draftOrderId"))
        else {
            return Value::Null;
        };
        if let Some(record) = self.store.staged.taggable_resources.get(&id) {
            return record.clone();
        }
        let value = self
            .store
            .staged
            .draft_order_tags
            .get(&id)
            .map(|tags| json!({ "id": id, "tags": tags }))
            .unwrap_or(Value::Null);
        value
    }

    pub(in crate::proxy) fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.store.staged.next_draft_order_bulk_tag_job_id;
        self.store.staged.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": shopify_gid("Job", id), "done": false })
    }

    pub(in crate::proxy) fn draft_order_bulk_add_tags(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let ids = resolved_string_list_arg(arguments, "ids");
        let tags = resolved_string_list_arg(arguments, "tags");
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
            return json!({ "job": Value::Null, "userErrors": user_errors });
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
        json!({ "job": job, "userErrors": user_errors })
    }

    pub(in crate::proxy) fn draft_order_bulk_remove_tags(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let ids = resolved_string_list_arg(arguments, "ids");
        let tags: BTreeSet<String> = resolved_string_list_arg(arguments, "tags")
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
        json!({ "job": job, "userErrors": user_errors })
    }
}

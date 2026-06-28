use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn money_bag_presentment_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "orderCreate" | "refundCreate" | "orderEditBegin" | "orderEditCommit"
            )
        }) {
            return None;
        }
        let handles_money_bag_selection = fields.iter().any(|field| {
            selection_contains_any(&field.selection, &["presentmentMoney", "totalRefundedSet"])
        });
        if !handles_money_bag_selection {
            return None;
        }
        // The money-bag presentment shim only knows how to echo a refund's
        // totalRefundedSet money bag (shop + presentment currency). A general
        // refundCreate selects far more than that — a refund `id`/`createdAt`,
        // line items, transactions, duties, the order's displayFinancialStatus,
        // etc. — and needs the full local refund engine with its over-refund and
        // quantity validations. Claim refundCreate ONLY when every refundCreate
        // selection stays within the money-bag money fields; decline anything
        // richer so refund_create_local_data owns it.
        let refund_is_money_bag_only = fields.iter().all(|field| {
            field.name != "refundCreate"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "refund",
                        "order",
                        "userErrors",
                        "totalRefundedSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "field",
                        "message",
                        "code",
                    ],
                )
        });
        if !refund_is_money_bag_only {
            return None;
        }
        let order_create_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderCreate"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "order",
                        "userErrors",
                        "id",
                        "field",
                        "message",
                        "code",
                        "currentTotalPriceSet",
                        "totalPriceSet",
                        "totalTaxSet",
                        "totalReceivedSet",
                        "totalOutstandingSet",
                        "lineItems",
                        "nodes",
                        "originalUnitPriceSet",
                        "shopMoney",
                        "amount",
                        "currencyCode",
                    ],
                )
        });
        if !order_create_is_money_bag_only {
            return None;
        }
        // The money-bag shim's orderEditBegin/Commit stubs only echo a
        // calculated order's totalPriceSet / committed order currentTotalPriceSet
        // money bag. A real order-edit begin/commit selects the calculated
        // line-item structure (lineItems, addedLineItems, originalOrder.name,
        // subtotals, shippingLines) and needs the full local edit engine. Claim
        // orderEditBegin/Commit ONLY when every selection stays within the
        // money-bag money fields; decline anything richer so the order-edit
        // engine owns it.
        let order_edit_begin_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderEditBegin"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "calculatedOrder",
                        "originalOrder",
                        "id",
                        "totalPriceSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "userErrors",
                        "field",
                        "message",
                    ],
                )
        });
        if !order_edit_begin_is_money_bag_only {
            return None;
        }
        let order_edit_commit_is_money_bag_only = fields.iter().all(|field| {
            field.name != "orderEditCommit"
                || selection_contains_only_any(
                    &field.selection,
                    &["presentmentMoney", "totalRefundedSet"],
                    &[
                        "order",
                        "currentTotalPriceSet",
                        "totalPriceSet",
                        "shopMoney",
                        "presentmentMoney",
                        "amount",
                        "currencyCode",
                        "successMessages",
                        "userErrors",
                        "field",
                        "message",
                    ],
                )
        });
        if !order_edit_commit_is_money_bag_only {
            return None;
        }

        let mut staged_ids = Vec::new();
        let mut early_response = None;
        let data = root_payload_json(&fields, |field| {
            if early_response.is_some() {
                return None;
            }
            let value = match field.name.as_str() {
                "orderCreate" => {
                    let order = self.stage_money_bag_order(field);
                    staged_ids.push(order["id"].as_str().unwrap_or_default().to_string());
                    selected_json(
                        &json!({ "order": order, "userErrors": [] }),
                        &field.selection,
                    )
                }
                "refundCreate" => {
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    let transactions = resolved_object_list_field(&input, "transactions");
                    let amount = transactions
                        .first()
                        .and_then(|transaction| resolved_string_field(transaction, "amount"))
                        .unwrap_or_else(|| "5.00".to_string());
                    let amount = normalize_money_amount(&amount);
                    let order_id = resolved_string_field(&input, "orderId").unwrap_or_default();
                    let currency = self
                        .store
                        .staged
                        .orders
                        .get(&order_id)
                        .map(|order| money_bag_currency(&order["totalPriceSet"]))
                        .unwrap_or_else(|| "USD".to_string());
                    let total = money_set_pair(&amount, &currency, &amount, &currency);
                    if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                        order["totalRefundedSet"] = total.clone();
                    }
                    selected_json(
                        &json!({
                            "refund": { "totalRefundedSet": total.clone() },
                            "order": { "totalRefundedSet": total },
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                "orderEditBegin" => {
                    let order_id =
                        resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    let order = self.store.staged.orders.get(&order_id);
                    if order.is_none() {
                        early_response = Some(json!({
                            "data": {
                                field.response_key.clone(): selected_json(
                                    &json!({
                                        "calculatedOrder": Value::Null,
                                        "userErrors": [user_error_omit_code(["id"], "The order does not exist.", None)]
                                    }),
                                    &field.selection
                                )
                            }
                        }));
                        return None;
                    }
                    if order.is_some_and(order_edit_order_is_not_editable) {
                        early_response = Some(json!({
                            "data": {
                                field.response_key.clone(): selected_json(
                                    &json!({
                                        "calculatedOrder": Value::Null,
                                        "userErrors": [user_error_omit_code(["base"], "not_editable", None)]
                                    }),
                                    &field.selection
                                )
                            }
                        }));
                        return None;
                    }
                    let calculated = json!({
                        "id": "gid://shopify/CalculatedOrder/7",
                        "originalOrder": { "id": order_id },
                        "totalPriceSet": money_set_pair("12.0", "CAD", "12.0", "CAD")
                    });
                    self.store.staged.order_edit_existing_calculated_order =
                        Some(calculated.clone());
                    selected_json(
                        &json!({ "calculatedOrder": calculated, "userErrors": [] }),
                        &field.selection,
                    )
                }
                "orderEditCommit" => {
                    let order = self
                        .store
                        .staged
                        .orders
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or(Value::Null);
                    selected_json(
                        &json!({
                            "order": order,
                            "successMessages": ["Order updated"],
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                _ => return None,
            };
            Some(value)
        });
        if let Some(response) = early_response {
            return Some(response);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "orderCreate", staged_ids);
        }
        Some(json!({ "data": data }))
    }

    fn stage_money_bag_order(&mut self, field: &RootFieldSelection) -> Value {
        let (id, order_input, first_line) = self.staged_order_input_and_first_line(field);
        let default_currency =
            resolved_string_field(&order_input, "currency").unwrap_or_else(|| "USD".to_string());
        let [shop_amount, shop_currency, presentment_amount, presentment_currency] =
            line_item_price_set_values(
                &first_line,
                ["0.0", &default_currency, "0.0", &default_currency],
                ["0.0", &default_currency],
                None,
            );
        let shop_amount = normalize_money_amount(&shop_amount);
        let presentment_amount = normalize_money_amount(&presentment_amount);
        let tax_amount = resolved_object_list_field(&first_line, "taxLines")
            .first()
            .and_then(|tax_line| resolved_object_field(tax_line, "priceSet"))
            .and_then(|tax_price| resolved_object_field(&tax_price, "shopMoney"))
            .and_then(|money| resolved_string_field(&money, "amount"))
            .map(|amount| normalize_money_amount(&amount))
            .unwrap_or_else(|| "0.0".to_string());
        let presentment_tax_amount = resolved_object_list_field(&first_line, "taxLines")
            .first()
            .and_then(|tax_line| resolved_object_field(tax_line, "priceSet"))
            .and_then(|tax_price| resolved_object_field(&tax_price, "presentmentMoney"))
            .and_then(|money| resolved_string_field(&money, "amount"))
            .map(|amount| normalize_money_amount(&amount))
            .unwrap_or_else(|| tax_amount.clone());
        let total = money_bag_add_decimal_strings(&shop_amount, &tax_amount);
        let presentment_total =
            money_bag_add_decimal_strings(&presentment_amount, &presentment_tax_amount);
        let line_price = money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        );
        let total_set = money_set_pair(
            &total,
            &shop_currency,
            &presentment_total,
            &presentment_currency,
        );
        let order = json!({
            "id": id,
            "currentTotalPriceSet": total_set.clone(),
            "totalPriceSet": total_set.clone(),
            "totalTaxSet": money_set_pair(&tax_amount, &shop_currency, &presentment_tax_amount, &presentment_currency),
            "totalReceivedSet": money_set_pair("0.0", &shop_currency, "0.0", &presentment_currency),
            "totalOutstandingSet": total_set,
            "lineItems": { "nodes": [{ "originalUnitPriceSet": line_price }] },
            "transactions": []
        });
        self.store.staged.orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        order
    }
}

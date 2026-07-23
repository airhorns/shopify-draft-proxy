use super::*;

fn requested_paths_only_use(
    requested_field_paths: &BTreeSet<Vec<String>>,
    allowed_fields: &[&str],
) -> bool {
    requested_field_paths.iter().all(|path| {
        path.iter()
            .all(|field| allowed_fields.contains(&field.as_str()))
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn money_bag_presentment_local_outcome(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        requested_field_paths: &BTreeSet<Vec<String>>,
    ) -> Option<ResolverOutcome<Value>> {
        if !matches!(
            root_name,
            "orderCreate" | "refundCreate" | "orderEditBegin" | "orderEditCommit"
        ) {
            return None;
        }
        let handles_money_bag_selection = requested_field_paths.iter().any(|path| {
            path.iter()
                .any(|field| matches!(field.as_str(), "presentmentMoney" | "totalRefundedSet"))
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
        let selection_is_money_bag_only = match root_name {
            "refundCreate" => requested_paths_only_use(
                requested_field_paths,
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
            ),
            "orderCreate" => requested_paths_only_use(
                requested_field_paths,
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
                    "presentmentMoney",
                    "amount",
                    "currencyCode",
                ],
            ),
            // The money-bag handler's orderEditBegin/Commit path only projects a
            // calculated order's totalPriceSet / committed order currentTotalPriceSet
            // money bag. A real order-edit begin/commit selects the calculated
            // line-item structure (lineItems, addedLineItems, originalOrder.name,
            // subtotals, shippingLines) and needs the full local edit engine. Claim
            // orderEditBegin/Commit ONLY when every selection stays within the
            // money-bag money fields; decline anything richer so the order-edit
            // engine owns it.
            "orderEditBegin" => requested_paths_only_use(
                requested_field_paths,
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
                    "code",
                ],
            ),
            "orderEditCommit" => requested_paths_only_use(
                requested_field_paths,
                &[
                    "order",
                    "id",
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
                    "code",
                ],
            ),
            _ => false,
        };
        if !selection_is_money_bag_only {
            return None;
        }
        if root_name == "orderCreate"
            && resolved_object_field(arguments, "order")
                .is_some_and(|input| order_create_input_needs_shop_currency_default(&input))
        {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
        }
        let shop_currency_code = self.store.shop_currency_code();

        let (value, staged_ids) = match root_name {
            "orderCreate" => {
                let order = self.stage_money_bag_order(arguments, &shop_currency_code);
                let id = order["id"].as_str().unwrap_or_default().to_string();
                (json!({ "order": order, "userErrors": [] }), vec![id])
            }
            "refundCreate" => {
                let input = resolved_object_field(arguments, "input").unwrap_or_default();
                let transactions = resolved_object_list_field(&input, "transactions");
                let order_id = resolved_string_field(&input, "orderId").unwrap_or_default();
                let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
                    return Some(ResolverOutcome::value(json!({
                        "refund": Value::Null,
                        "order": refund_order_payload(None, &shop_currency_code),
                        "userErrors": [user_error_omit_code(
                            json!(["orderId"]),
                            "Order does not exist",
                            Some("NOT_FOUND"),
                        )]
                    })));
                };
                let total = transactions
                    .first()
                    .and_then(|transaction| {
                        money_bag_refund_transaction_total(
                            &order,
                            &input,
                            transaction,
                            &shop_currency_code,
                        )
                    })
                    .unwrap_or_else(|| {
                        money_bag_order_money_set(
                            &order,
                            "totalOutstandingSet",
                            &shop_currency_code,
                        )
                    });
                if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                    order["totalRefundedSet"] = total.clone();
                }
                (
                    json!({
                        "refund": { "totalRefundedSet": total.clone() },
                        "order": { "totalRefundedSet": total },
                        "userErrors": []
                    }),
                    Vec::new(),
                )
            }
            "orderEditBegin" => {
                let order_id = resolved_string_field(arguments, "id").unwrap_or_default();
                let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
                    return Some(ResolverOutcome::value(json!({
                        "calculatedOrder": Value::Null,
                        "userErrors": [user_error_omit_code(["id"], "The order does not exist.", None)]
                    })));
                };
                if order_edit_order_is_not_editable(&order) {
                    return Some(ResolverOutcome::value(json!({
                        "calculatedOrder": Value::Null,
                        "userErrors": [user_error_omit_code(Value::Null, "The order cannot be edited.", None)]
                    })));
                }
                let calculated_id = self.next_proxy_synthetic_gid("CalculatedOrder");
                let calculated = json!({
                    "id": calculated_id,
                    "originalOrder": { "id": order_id },
                    "totalPriceSet": money_bag_order_money_set(&order, "totalPriceSet", &shop_currency_code)
                });
                self.store
                    .staged
                    .order_edit_money_bag_calculated_order_ids
                    .insert(
                        calculated["id"].as_str().unwrap_or_default().to_string(),
                        calculated["originalOrder"]["id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    );
                self.store.staged.order_edit_existing_calculated_order = Some(calculated.clone());
                (
                    json!({ "calculatedOrder": calculated, "userErrors": [] }),
                    Vec::new(),
                )
            }
            "orderEditCommit" => {
                let calculated_id = resolved_string_field(arguments, "id").unwrap_or_default();
                let message = if self
                    .store
                    .staged
                    .order_edit_money_bag_calculated_order_ids
                    .contains_key(&calculated_id)
                {
                    "There must be at least one change to be made."
                } else {
                    "The calculated order does not exist."
                };
                (
                    json!({
                        "order": Value::Null,
                        "successMessages": [],
                        "userErrors": [user_error_omit_code(["id"], message, None)]
                    }),
                    Vec::new(),
                )
            }
            _ => return None,
        };
        let mut outcome = ResolverOutcome::value(value);
        if !staged_ids.is_empty() {
            outcome = outcome.with_log_draft(LogDraft::staged("orderCreate", "orders", staged_ids));
        }
        Some(outcome)
    }

    fn stage_money_bag_order(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        shop_currency_code: &str,
    ) -> Value {
        let order_input = resolved_object_field(arguments, "order").unwrap_or_default();
        let id = self.next_synthetic_gid("Order");
        let first_line = resolved_object_list_field(&order_input, "lineItems")
            .first()
            .cloned()
            .unwrap_or_default();
        let default_currency = resolved_string_field(&order_input, "currency")
            .or_else(|| resolved_string_field(&order_input, "currencyCode"))
            .unwrap_or_else(|| shop_currency_code.to_string());
        let [shop_amount, shop_currency, presentment_amount, presentment_currency] =
            line_items_price_set_values(
                &order_input,
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

fn money_bag_order_money_set(order: &Value, key: &str, shop_currency_code: &str) -> Value {
    for candidate in [
        key,
        "currentTotalPriceSet",
        "totalPriceSet",
        "totalOutstandingSet",
    ] {
        let Some(value) = order.get(candidate).filter(|value| value.is_object()) else {
            continue;
        };
        let shop_amount = money_amount(value, "shopMoney")
            .or_else(|| value["amount"].as_str().map(ToString::to_string))
            .unwrap_or_else(|| "0.0".to_string());
        let shop_currency =
            money_set_shop_currency(value).unwrap_or_else(|| shop_currency_code.to_string());
        let presentment_amount =
            money_set_presentment_or_shop_amount(value).unwrap_or_else(|| shop_amount.clone());
        let presentment_currency =
            money_set_presentment_currency(value).unwrap_or_else(|| shop_currency.clone());
        return money_set_pair(
            &shop_amount,
            &shop_currency,
            &presentment_amount,
            &presentment_currency,
        );
    }
    let currency = money_bag_currency(&order["totalPriceSet"])
        .unwrap_or_else(|| shop_currency_code.to_string());
    money_set_pair("0.0", &currency, "0.0", &currency)
}

fn money_bag_refund_transaction_total(
    order: &Value,
    input: &BTreeMap<String, ResolvedValue>,
    transaction: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Option<Value> {
    let amount = resolved_string_field(transaction, "amount")?;
    let amount = normalize_money_amount(&amount);
    let shop_currency = order_currency(order, shop_currency_code);
    let parent_amount_set = resolved_string_field(transaction, "parentId")
        .and_then(|parent_id| order_transaction_by_id(order, &parent_id))
        .map(|parent| parent["amountSet"].clone());
    let presentment_currency = resolved_string_field(input, "currency")
        .or_else(|| resolved_string_field(transaction, "currency"))
        .or_else(|| resolved_string_field(transaction, "currencyCode"))
        .or_else(|| {
            parent_amount_set
                .as_ref()
                .and_then(money_set_presentment_or_shop_currency)
        })
        .unwrap_or_else(|| order_presentment_currency(order, &shop_currency));
    let conversion_basis = parent_amount_set
        .unwrap_or_else(|| money_bag_order_money_set(order, "totalPriceSet", shop_currency_code));
    Some(payment_money_set_for_capture(
        &conversion_basis,
        &amount,
        &presentment_currency,
    ))
}

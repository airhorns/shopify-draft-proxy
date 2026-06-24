use super::*;

pub(super) fn return_connection(nodes: Vec<Value>) -> Value {
    json!({
        "nodes": nodes,
        "pageInfo": empty_page_info()
    })
}

fn return_money_set(amount: &str, currency_code: &str) -> Value {
    let amount = money_bag_normalized_amount(amount);
    money_set_pair(&amount, currency_code, &amount, currency_code)
}

fn return_status_invalid_error() -> Value {
    user_error(["id"], "return_request_status_invalid", Some("INVALID"))
}

fn blank_return_line_string(value: Option<String>) -> bool {
    value.as_deref().is_none_or(|raw| raw.trim().is_empty())
}

fn validate_return_line_item_reason(
    input_name: &str,
    index: usize,
    item: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let reason = resolved_string_field(item, "returnReason");
    let reason_definition_id = resolved_string_field(item, "returnReasonDefinitionId");
    let reason_missing = blank_return_line_string(reason.clone());
    let definition_missing = blank_return_line_string(reason_definition_id);
    if reason_missing && definition_missing {
        return Some(match input_name {
            "returnInput" => user_error(vec![
                    "returnInput".to_string(),
                    "returnLineItems".to_string(),
                    index.to_string(),
                ], "Return line items Either return reason or return reason definition must be provided", Some("NOT_FOUND")),
            _ => presence_user_error(vec![
                    "input".to_string(),
                    "returnLineItems".to_string(),
                    index.to_string(),
                    "returnReason".to_string(),
                ], "Return reason"),
        });
    }

    if input_name == "returnInput"
        && reason.as_deref() == Some("OTHER")
        && blank_return_line_string(resolved_string_field(item, "returnReasonNote"))
    {
        return Some(user_error(vec![
                "returnInput".to_string(),
                "returnLineItems".to_string(),
                index.to_string(),
                "returnReasonNote".to_string(),
            ], "Return line items return reason note The note is required when the return reason is \"Other\"", Some("BLANK")));
    }

    None
}

/// The returns embedded in an order graph, accepting either a bare array
/// (`order.returns`) or a connection (`order.returns.nodes`) so seeded orders
/// hydrated from either shape resolve.
fn order_returns_array(order: &Value) -> Vec<Value> {
    if let Some(array) = order["returns"].as_array() {
        return array.clone();
    }
    order["returns"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The line items of a return, accepting either a bare array or a connection.
fn return_line_items_array(return_value: &Value) -> Vec<Value> {
    if let Some(array) = return_value["returnLineItems"].as_array() {
        return array.clone();
    }
    return_value["returnLineItems"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The fulfillment line item id a return line item points at, tolerating both
/// the nested object shape (`fulfillmentLineItem { id }`) and a flat id.
fn return_line_item_fulfillment_line_item_id(line: &Value) -> Option<String> {
    line["fulfillmentLineItem"]["id"]
        .as_str()
        .or_else(|| line["fulfillmentLineItemId"].as_str())
        .map(str::to_string)
}

/// Find a fulfillment line item across an order's fulfillments by id. Each
/// fulfillment's `fulfillmentLineItems` may be a bare array or a connection.
fn find_order_fulfillment_line_item(order: &Value, id: &str) -> Option<Value> {
    let fulfillments = order["fulfillments"].as_array()?;
    for fulfillment in fulfillments {
        let lines = fulfillment["fulfillmentLineItems"]
            .as_array()
            .cloned()
            .or_else(|| {
                fulfillment["fulfillmentLineItems"]["nodes"]
                    .as_array()
                    .cloned()
            })
            .unwrap_or_default();
        if let Some(line) = lines
            .into_iter()
            .find(|line| line["id"].as_str() == Some(id))
        {
            return Some(line);
        }
    }
    None
}

/// Build a return line item from the matched fulfillment line item and the
/// requested input. `processedQuantity` starts at 0 and `unprocessedQuantity`
/// at the full requested quantity; the reason defaults to `OTHER`.
fn build_return_line_item(
    return_line_item_id: &str,
    fulfillment_line_item: &Value,
    item: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let quantity = resolved_i64_field(item, "quantity").unwrap_or(0);
    let reason = resolved_string_field(item, "returnReason").unwrap_or_else(|| "OTHER".to_string());
    let reason_note = resolved_string_field(item, "returnReasonNote").unwrap_or_default();
    let line_item = if fulfillment_line_item["lineItem"].is_object() {
        fulfillment_line_item["lineItem"].clone()
    } else {
        json!({
            "id": fulfillment_line_item["lineItem"]["id"].clone(),
            "title": fulfillment_line_item["lineItem"]["title"].clone()
        })
    };
    json!({
        "id": return_line_item_id,
        "quantity": quantity,
        "processedQuantity": 0,
        "unprocessedQuantity": quantity,
        "returnReason": reason,
        "returnReasonNote": reason_note,
        "customerNote": Value::Null,
        "fulfillmentLineItem": {
            "id": fulfillment_line_item["id"].clone(),
            "lineItem": line_item
        }
    })
}

/// Validate a `returnDeclineRequest` input before any state change. Returns the
/// decline reason on success, or the failing user error: an invalid/missing
/// reason takes precedence (Shopify rejects it at the enum boundary with
/// `Expected "<value>" to be one of: …`), then an invalid notify email carried
/// under the `tmp_notify_customer.email_address` shim.
fn validate_return_decline_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Result<String, Vec<Value>> {
    const VALID_REASONS: &[&str] = &["RETURN_PERIOD_ENDED", "FINAL_SALE", "OTHER"];
    let reason = resolved_string_field(input, "declineReason").unwrap_or_default();
    if !VALID_REASONS.contains(&reason.as_str()) {
        return Err(vec![user_error(
            ["declineReason"],
            &format!("Expected \"{reason}\" to be one of: RETURN_PERIOD_ENDED, FINAL_SALE, OTHER"),
            Some("INVALID"),
        )]);
    }
    if let Some(notify) = resolved_object_field(input, "tmp_notify_customer") {
        if let Some(email) = resolved_string_field(&notify, "email_address") {
            if !valid_email_address(&email) {
                return Err(vec![user_error(
                    ["input", "tmp_notify_customer", "email_address"],
                    "Email address is invalid",
                    Some("INVALID"),
                )]);
            }
        }
    }
    Ok(reason)
}

/// Minimal RFC-shaped email check: a single `@`, non-empty local part, and a
/// dotted domain with no whitespace.
fn valid_email_address(email: &str) -> bool {
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}

/// The reference transition rules for `returnClose`/`returnReopen`/
/// `returnCancel`. Returns `Some((message, code))` when the transition is
/// disallowed for the return's current status; `None` when it is allowed
/// (including idempotent same-status requests).
fn return_status_transition_error(
    target_status: &str,
    record: &Value,
) -> Option<(&'static str, &'static str)> {
    let status = record["status"].as_str().unwrap_or_default();
    match target_status {
        "CLOSED" => {
            if matches!(status, "OPEN" | "CLOSED") {
                None
            } else {
                Some(("Return status is invalid.", "INVALID_STATE"))
            }
        }
        "OPEN" => {
            if matches!(status, "CLOSED" | "OPEN") {
                None
            } else {
                Some(("Return status is invalid.", "INVALID_STATE"))
            }
        }
        "CANCELED" => {
            let has_processed = return_line_items_array(record)
                .iter()
                .any(|line| line["processedQuantity"].as_i64().unwrap_or(0) > 0);
            if status == "CANCELED"
                || (!has_processed && matches!(status, "OPEN" | "REQUESTED" | "DECLINED"))
            {
                None
            } else {
                Some(("Return is not cancelable.", "INVALID_STATE"))
            }
        }
        _ => None,
    }
}

impl DraftProxy {
    fn return_payload(
        &self,
        return_value: Value,
        user_errors: Vec<Value>,
        selection: &[SelectedField],
    ) -> Value {
        selected_json(
            &json!({ "return": return_value, "userErrors": user_errors }),
            selection,
        )
    }

    pub(in crate::proxy) fn order_return_local_runtime_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if matches!(
            root_field,
            "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
        ) {
            if !self.should_handle_order_return_read(&fields) {
                return None;
            }
            return self.order_return_read_data(&fields);
        }

        let field = fields.iter().find(|field| field.name == root_field)?;
        match root_field {
            "returnCreate" => {
                let value = self.stage_return_from_input(request, field, "returnInput", "OPEN");
                Some(data_response(&field.response_key, value))
            }
            "returnRequest" => {
                let value = self.stage_return_from_input(request, field, "input", "REQUESTED");
                Some(data_response(&field.response_key, value))
            }
            "returnApproveRequest" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "id"))?;
                let value = self.approve_return_request(&id, field);
                Some(data_response(&field.response_key, value))
            }
            "returnDeclineRequest" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "id"))?;
                let value = self.decline_return_request(&id, field);
                Some(data_response(&field.response_key, value))
            }
            "returnClose" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "CLOSED", field);
                Some(data_response(&field.response_key, value))
            }
            "returnReopen" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "OPEN", field);
                Some(data_response(&field.response_key, value))
            }
            "returnCancel" => {
                let id = resolved_string_arg(&field.arguments, "id")?;
                let value = self.apply_return_lifecycle_transition(&id, "CANCELED", field);
                Some(data_response(&field.response_key, value))
            }
            "removeFromReturn" => {
                let value = self.remove_from_return(field);
                Some(data_response(&field.response_key, value))
            }
            "reverseDeliveryCreateWithShipping" => {
                let value = self.stage_reverse_delivery(field);
                Some(data_response(&field.response_key, value))
            }
            "reverseDeliveryShippingUpdate" => {
                let id = resolved_string_arg(&field.arguments, "reverseDeliveryId")?;
                let value = self.update_reverse_delivery(&id, field);
                Some(data_response(&field.response_key, value))
            }
            "reverseFulfillmentOrderDispose" => {
                let value = self.dispose_reverse_fulfillment_order(field);
                Some(data_response(&field.response_key, value))
            }
            "returnProcess" => {
                let id = resolved_object_field(&field.arguments, "input")
                    .and_then(|input| resolved_string_field(&input, "returnId"))?;
                let value = self.process_return(&id, field);
                Some(data_response(&field.response_key, value))
            }
            _ => None,
        }
    }

    fn order_return_read_data(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "return" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .returns
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.selected_return_order(&id, &field.selection)
                }
                "reverseDelivery" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .reverse_deliveries
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "reverseFulfillmentOrder" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.store
                        .staged
                        .reverse_fulfillment_orders
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn should_handle_order_return_read(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "return" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.returns.contains_key(&id)),
            "order" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.returns_by_order.contains_key(&id)),
            "reverseDelivery" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.reverse_deliveries.contains_key(&id)),
            "reverseFulfillmentOrder" => {
                resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                    self.store
                        .staged
                        .reverse_fulfillment_orders
                        .contains_key(&id)
                })
            }
            _ => false,
        })
    }

    /// Stage a return from a `returnCreate` (`OPEN`) or `returnRequest`
    /// (`REQUESTED`) input. Reads the seeded order from store state, validates
    /// each requested line against the order's fulfillment line items and the
    /// quantity already consumed by prior non-canceled returns, builds the
    /// return line items + (for OPEN) the reverse fulfillment order, and stages
    /// the result. IDs come from the shared synthetic counter so the allocation
    /// order (return line items, return, RFO line items, RFO) matches the
    /// reference implementation. Returns the selected `{ return, userErrors }`
    /// payload — `return` is null when validation fails.
    fn stage_return_from_input(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        input_name: &str,
        status: &str,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let items = resolved_object_list_field(&input, "returnLineItems");
        if items.is_empty() {
            return self.return_payload(
                Value::Null,
                vec![user_error(
                    ["returnLineItems"],
                    "Return must include at least one line item.",
                    Some("INVALID"),
                )],
                &field.selection,
            );
        }
        let reason_errors = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| validate_return_line_item_reason(input_name, index, item))
            .collect::<Vec<_>>();
        if !reason_errors.is_empty() {
            return self.return_payload(Value::Null, reason_errors, &field.selection);
        }
        // Validate every line first, allocating return-line-item ids only for
        // valid lines (matching the reference fold). Any error short-circuits
        // the mutation with a null return and no state change.
        let order_id = resolved_string_field(&input, "orderId").unwrap_or_default();
        // The order a return runs against is a precondition that may not have been
        // created locally in this scenario; forward+observe it on a cold miss so
        // line validation and quantity caps run against real store state.
        self.hydrate_order_for_return(request, &order_id);
        let order = self
            .store
            .staged
            .orders
            .get(&order_id)
            .cloned()
            .unwrap_or(Value::Null);
        let mut line_items: Vec<Value> = Vec::new();
        let mut user_errors: Vec<Value> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let fli_id = resolved_string_field(item, "fulfillmentLineItemId");
            let quantity = resolved_i64_field(item, "quantity").unwrap_or(0);
            let fulfillment_line_item = fli_id
                .as_deref()
                .and_then(|id| find_order_fulfillment_line_item(&order, id));
            match fulfillment_line_item {
                None => user_errors.push(user_error(
                    [
                        "returnLineItems",
                        &index.to_string(),
                        "fulfillmentLineItemId",
                    ],
                    "Fulfillment line item does not exist.",
                    Some("INVALID"),
                )),
                Some(fulfillment_line_item) => {
                    let available = fulfillment_line_item["quantity"].as_i64().unwrap_or(0);
                    let already = self.already_returned_quantity(
                        &order,
                        &order_id,
                        fli_id.as_deref().unwrap_or_default(),
                    );
                    let remaining = (available - already).max(0);
                    if quantity <= 0 || quantity > remaining {
                        user_errors.push(user_error(
                            ["returnLineItems", &index.to_string(), "quantity"],
                            "Quantity is not available for return.",
                            Some("INVALID"),
                        ));
                    } else {
                        let rli_id = self.next_synthetic_gid("ReturnLineItem");
                        line_items.push(build_return_line_item(
                            &rli_id,
                            &fulfillment_line_item,
                            item,
                        ));
                    }
                }
            }
        }
        if !user_errors.is_empty() {
            return self.return_payload(Value::Null, user_errors, &field.selection);
        }
        let return_id = self.next_synthetic_gid("Return");
        let order_name = order["name"].as_str().unwrap_or("#ORDER").to_string();
        let prior_returns = order_returns_array(&order).len()
            + self
                .store
                .staged
                .returns_by_order
                .get(&order_id)
                .map(Vec::len)
                .unwrap_or(0);
        let name = format!("{order_name}-R{}", prior_returns + 1);
        let total_quantity: i64 = line_items
            .iter()
            .map(|line| line["quantity"].as_i64().unwrap_or(0))
            .sum();
        let order_updated_at = order["updatedAt"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| "2024-01-01T00:00:03.000Z".to_string());
        let mut return_record = json!({
            "id": return_id,
            "name": name,
            "status": status,
            "closedAt": Value::Null,
            "decline": Value::Null,
            "totalQuantity": total_quantity,
            "order": {
                "id": order_id,
                "updatedAt": order_updated_at
            },
            "returnLineItems": { "nodes": line_items },
            "returnShippingFees": [],
            "reverseFulfillmentOrders": { "nodes": [] }
        });
        if let Some(fee_input) = resolved_object_field(&input, "returnShippingFee") {
            let amount = resolved_object_field(&fee_input, "amount").unwrap_or_default();
            let amount_value =
                resolved_string_field(&amount, "amount").unwrap_or_else(|| "0.00".to_string());
            let currency =
                resolved_string_field(&amount, "currencyCode").unwrap_or_else(|| "USD".to_string());
            let fee_id = self.next_synthetic_gid("ReturnShippingFee");
            return_record["returnShippingFees"] = json!([{
                "id": fee_id,
                "amountSet": return_money_set(&amount_value, &currency)
            }]);
        }
        if status == "OPEN" {
            self.build_return_reverse_fulfillment_order(&mut return_record);
        }
        self.store
            .staged
            .returns
            .insert(return_id.clone(), return_record.clone());
        self.store
            .staged
            .returns_by_order
            .entry(order_id)
            .or_default()
            .push(return_id);
        self.return_payload(return_record, Vec::new(), &field.selection)
    }

    /// Total quantity already consumed against a fulfillment line item by
    /// non-canceled returns — both returns embedded in the seeded order graph
    /// (from hydration) and returns staged during this session. Mirrors the
    /// reference `already_returned_quantity` so quantity caps account for the
    /// real outstanding return volume rather than the raw fulfilled quantity.
    fn already_returned_quantity(
        &self,
        order: &Value,
        order_id: &str,
        fulfillment_line_item_id: &str,
    ) -> i64 {
        let mut total = 0_i64;
        let mut accumulate = |return_value: &Value| {
            if return_value["status"].as_str() == Some("CANCELED") {
                return;
            }
            for line in return_line_items_array(return_value) {
                if return_line_item_fulfillment_line_item_id(&line).as_deref()
                    == Some(fulfillment_line_item_id)
                {
                    total += line["quantity"].as_i64().unwrap_or(0);
                }
            }
        };
        for embedded in order_returns_array(order) {
            accumulate(&embedded);
        }
        if let Some(ids) = self.store.staged.returns_by_order.get(order_id) {
            for id in ids {
                if let Some(staged) = self.store.staged.returns.get(id) {
                    accumulate(staged);
                }
            }
        }
        total
    }

    fn selected_return_order(&self, order_id: &str, selection: &[SelectedField]) -> Value {
        let returns = self
            .store
            .staged
            .returns_by_order
            .get(order_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| self.store.staged.returns.get(&id).cloned())
            .collect::<Vec<_>>();
        let order = self.store.staged.orders.get(order_id).cloned();
        let name = order
            .as_ref()
            .and_then(|order| order["name"].as_str())
            .unwrap_or("#ORDER")
            .to_string();
        let updated_at = order
            .as_ref()
            .and_then(|order| order["updatedAt"].as_str())
            .unwrap_or("2024-01-01T00:00:03.000Z")
            .to_string();
        selected_json(
            &json!({
                "id": order_id,
                "name": name,
                "updatedAt": updated_at,
                "returns": return_connection(returns)
            }),
            selection,
        )
    }

    /// `returnApproveRequest`: a REQUESTED return transitions to OPEN and
    /// acquires its reverse fulfillment order. Any other status returns the
    /// `return_request_status_invalid` user error on `id` (INVALID) and leaves
    /// state untouched.
    fn approve_return_request(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return self.return_payload(
                Value::Null,
                vec![return_status_invalid_error()],
                &field.selection,
            );
        };
        if record["status"].as_str() != Some("REQUESTED") {
            return self.return_payload(
                Value::Null,
                vec![return_status_invalid_error()],
                &field.selection,
            );
        }
        record["status"] = json!("OPEN");
        self.build_return_reverse_fulfillment_order(&mut record);
        self.store
            .staged
            .returns
            .insert(id.to_string(), record.clone());
        self.return_payload(record, Vec::new(), &field.selection)
    }

    /// `returnDeclineRequest`: validate the decline input (reason enum, note
    /// length, notify email) before touching state; a REQUESTED return then
    /// transitions to DECLINED carrying `decline { reason, note }`. A non-
    /// REQUESTED return returns `return_request_status_invalid` on `id`.
    fn decline_return_request(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let reason = match validate_return_decline_input(&input) {
            Ok(reason) => reason,
            Err(errors) => {
                return self.return_payload(Value::Null, errors, &field.selection);
            }
        };
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return self.return_payload(
                Value::Null,
                vec![return_status_invalid_error()],
                &field.selection,
            );
        };
        if record["status"].as_str() != Some("REQUESTED") {
            return self.return_payload(
                Value::Null,
                vec![return_status_invalid_error()],
                &field.selection,
            );
        }
        let note = resolved_string_field(&input, "declineNote").unwrap_or_default();
        record["status"] = json!("DECLINED");
        record["decline"] = json!({ "reason": reason, "note": note });
        self.store
            .staged
            .returns
            .insert(id.to_string(), record.clone());
        self.return_payload(record, Vec::new(), &field.selection)
    }

    /// `returnClose` / `returnReopen` / `returnCancel`. Allowed transitions
    /// mirror the reference `return_status_transition_error` rules: close from
    /// OPEN/CLOSED, reopen from CLOSED/OPEN, cancel from any return without
    /// processed/refunded lines (and idempotent CANCELED). Disallowed
    /// transitions return INVALID_STATE with the reference message and leave
    /// state untouched; same-status requests are idempotent no-ops.
    fn apply_return_lifecycle_transition(
        &mut self,
        id: &str,
        target_status: &str,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return self.return_payload(
                Value::Null,
                vec![user_error(
                    ["id"],
                    "Return does not exist.",
                    Some("INVALID"),
                )],
                &field.selection,
            );
        };
        let current = record["status"].as_str().unwrap_or_default().to_string();
        if let Some((message, code)) = return_status_transition_error(target_status, &record) {
            return self.return_payload(
                Value::Null,
                vec![user_error(["id"], message, Some(code))],
                &field.selection,
            );
        }
        if current != target_status {
            record["status"] = json!(target_status);
            record["closedAt"] = if target_status == "CLOSED" {
                json!("2024-01-01T00:00:03.000Z")
            } else {
                Value::Null
            };
            self.store
                .staged
                .returns
                .insert(id.to_string(), record.clone());
        }
        self.return_payload(record, Vec::new(), &field.selection)
    }

    /// `removeFromReturn`: validate the return is still editable, then validate
    /// each removal against the return's removable quantity (current minus
    /// processed) before mutating; on success reduce or drop the affected return
    /// line items, recompute the total, and rebuild the reverse fulfillment
    /// order's line items from the surviving return lines. On any validation
    /// error the return is left null with the error payload.
    fn remove_from_return(&mut self, field: &RootFieldSelection) -> Value {
        let return_id = resolved_string_arg(&field.arguments, "returnId").unwrap_or_default();
        let removals = list_object_field(&field.arguments, "returnLineItems");
        let Some(mut record) = self.store.staged.returns.get(&return_id).cloned() else {
            return self.return_payload(
                Value::Null,
                vec![user_error(
                    ["returnId"],
                    "Return does not exist.",
                    Some("INVALID"),
                )],
                &field.selection,
            );
        };
        let status = record["status"].as_str().unwrap_or_default();
        if !matches!(status, "OPEN" | "REQUESTED") {
            return selected_json(
                &json!({ "return": Value::Null, "userErrors": [user_error(["returnId"], "Return status is invalid.", Some("INVALID_STATE"))] }),
                &field.selection,
            );
        }
        let mut nodes = record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut user_errors: Vec<Value> = Vec::new();
        for (index, removal) in removals.iter().enumerate() {
            let line_item_id = resolved_string_field(removal, "returnLineItemId");
            let quantity = resolved_i64_field(removal, "quantity").unwrap_or(0);
            let position = line_item_id.as_deref().and_then(|id| {
                nodes
                    .iter()
                    .position(|node| node["id"].as_str() == Some(id))
            });
            match position {
                None => user_errors.push(user_error(
                    ["returnLineItems", &index.to_string(), "returnLineItemId"],
                    "Return line item does not exist.",
                    Some("INVALID"),
                )),
                Some(position) => {
                    let current = nodes[position]["quantity"].as_i64().unwrap_or(0);
                    let processed = nodes[position]["processedQuantity"].as_i64().unwrap_or(0);
                    let removable = current - processed;
                    if quantity <= 0 || quantity > removable {
                        user_errors.push(user_error(
                            ["returnLineItems", &index.to_string(), "quantity"],
                            "Quantity is not removable from return.",
                            Some("INVALID"),
                        ));
                    } else {
                        let next_quantity = current - quantity;
                        if next_quantity <= 0 {
                            nodes.remove(position);
                        } else {
                            nodes[position]["quantity"] = json!(next_quantity);
                            let next_processed =
                                nodes[position]["processedQuantity"].as_i64().unwrap_or(0);
                            nodes[position]["unprocessedQuantity"] =
                                json!((next_quantity - next_processed).max(0));
                        }
                    }
                }
            }
        }
        if !user_errors.is_empty() {
            return self.return_payload(Value::Null, user_errors, &field.selection);
        }
        let total_quantity: i64 = nodes
            .iter()
            .map(|n| n["quantity"].as_i64().unwrap_or(0))
            .sum();
        record["returnLineItems"] = json!({ "nodes": nodes.clone() });
        record["totalQuantity"] = json!(total_quantity);
        self.sync_reverse_fulfillment_line_items(&mut record);
        self.store.staged.returns.insert(return_id, record.clone());
        self.return_payload(record, Vec::new(), &field.selection)
    }

    /// Build the OPEN reverse fulfillment order for a return: one RFO line item
    /// per return line item (allocated first), then the RFO itself, so the
    /// shared synthetic counter yields RFO-line ids before the RFO id. Each RFO
    /// line item carries both `returnLineItem { id }` and the nested
    /// `fulfillmentLineItem { id, lineItem { id, title } }` so local and live
    /// selections both resolve. Stores the RFO and mirrors it onto the return.
    fn build_return_reverse_fulfillment_order(&mut self, return_record: &mut Value) {
        if return_record["reverseFulfillmentOrders"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
        {
            return;
        }
        let return_lines = return_record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rfo_lines: Vec<Value> = Vec::new();
        for line in &return_lines {
            let line_id = self.next_synthetic_gid("ReverseFulfillmentOrderLineItem");
            let quantity = line["quantity"].as_i64().unwrap_or(0);
            let processed = line["processedQuantity"].as_i64().unwrap_or(0);
            rfo_lines.push(json!({
                "id": line_id,
                "totalQuantity": quantity,
                "remainingQuantity": (quantity - processed).max(0),
                "dispositionType": Value::Null,
                "returnLineItem": { "id": line["id"].clone() },
                "fulfillmentLineItem": line["fulfillmentLineItem"].clone(),
                "dispositions": []
            }));
        }
        let rfo_id = self.next_synthetic_gid("ReverseFulfillmentOrder");
        let reverse_order = json!({
            "id": rfo_id,
            "status": "OPEN",
            "lineItems": { "nodes": rfo_lines },
            "reverseDeliveries": { "nodes": [] }
        });
        return_record["reverseFulfillmentOrders"] = json!({ "nodes": [reverse_order.clone()] });
        self.store
            .staged
            .reverse_fulfillment_orders
            .insert(rfo_id, reverse_order);
    }

    /// Rebuild the return's reverse fulfillment order line items from its
    /// current return line items (used after `removeFromReturn`). Existing RFO
    /// line ids are reused when their return line survives; removed return lines
    /// drop their RFO line. The reverse fulfillment order's `totalQuantity` /
    /// `remainingQuantity` are recomputed and the staged RFO is kept in sync.
    fn sync_reverse_fulfillment_line_items(&mut self, return_record: &mut Value) {
        let return_lines = return_record["returnLineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rfos = return_record["reverseFulfillmentOrders"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for rfo in &mut rfos {
            let existing = rfo["lineItems"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut rebuilt: Vec<Value> = Vec::new();
            for line in &return_lines {
                let quantity = line["quantity"].as_i64().unwrap_or(0);
                let processed = line["processedQuantity"].as_i64().unwrap_or(0);
                let mut rfo_line = existing
                    .iter()
                    .find(|candidate| candidate["returnLineItem"]["id"] == line["id"])
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "id": Value::Null,
                            "dispositionType": Value::Null,
                            "returnLineItem": { "id": line["id"].clone() },
                            "fulfillmentLineItem": line["fulfillmentLineItem"].clone(),
                            "dispositions": []
                        })
                    });
                rfo_line["totalQuantity"] = json!(quantity);
                rfo_line["remainingQuantity"] = json!((quantity - processed).max(0));
                rebuilt.push(rfo_line);
            }
            rfo["lineItems"] = json!({ "nodes": rebuilt });
            if let Some(id) = rfo["id"].as_str() {
                if let Some(staged) = self.store.staged.reverse_fulfillment_orders.get_mut(id) {
                    staged["lineItems"] = rfo["lineItems"].clone();
                }
            }
        }
        return_record["reverseFulfillmentOrders"] = json!({ "nodes": rfos });
    }

    fn stage_reverse_delivery(&mut self, field: &RootFieldSelection) -> Value {
        let reverse_order_id =
            resolved_string_arg(&field.arguments, "reverseFulfillmentOrderId").unwrap_or_default();
        let id = self.next_synthetic_gid("ReverseDelivery");
        let tracking = resolved_object_field(&field.arguments, "trackingInput").unwrap_or_default();
        let label = resolved_object_field(&field.arguments, "labelInput").unwrap_or_default();
        let rfo_lines = self
            .store
            .staged
            .reverse_fulfillment_orders
            .get(&reverse_order_id)
            .and_then(|order| order["lineItems"]["nodes"].as_array())
            .cloned()
            .unwrap_or_default();
        let input_lines = list_object_field(&field.arguments, "reverseDeliveryLineItems");
        let delivery_line_sources = if input_lines.is_empty() {
            rfo_lines
                .iter()
                .map(|line| (line.clone(), line["totalQuantity"].as_i64().unwrap_or(0)))
                .collect::<Vec<_>>()
        } else {
            input_lines
                .iter()
                .map(|input| {
                    let line_id = resolved_string_field(input, "reverseFulfillmentOrderLineItemId")
                        .unwrap_or_default();
                    let quantity = resolved_i64_field(input, "quantity").unwrap_or(0);
                    let line = rfo_lines
                        .iter()
                        .find(|line| line["id"].as_str() == Some(line_id.as_str()))
                        .cloned()
                        .unwrap_or_else(|| {
                            json!({
                                "id": line_id,
                                "totalQuantity": Value::Null,
                                "remainingQuantity": Value::Null
                            })
                        });
                    (line, quantity)
                })
                .collect::<Vec<_>>()
        };
        let reverse_delivery_line_items = delivery_line_sources
            .into_iter()
            .map(|(line, quantity)| {
                json!({
                    "id": self.next_synthetic_gid("ReverseDeliveryLineItem"),
                    "quantity": quantity,
                    "reverseFulfillmentOrderLineItem": {
                        "id": line["id"].clone(),
                        "totalQuantity": line["totalQuantity"].clone(),
                        "remainingQuantity": line["remainingQuantity"].clone()
                    }
                })
            })
            .collect::<Vec<_>>();
        let delivery = json!({
            "id": id,
            "reverseFulfillmentOrder": { "id": reverse_order_id },
            "reverseDeliveryLineItems": {
                "nodes": reverse_delivery_line_items
            },
            "deliverable": {
                "__typename": "ReverseDeliveryShippingDeliverable",
                "tracking": {
                    "number": resolved_string_field(&tracking, "number").unwrap_or_default(),
                    "url": resolved_string_field(&tracking, "url").unwrap_or_default(),
                    "company": resolved_string_field(&tracking, "company").unwrap_or_default(),
                    "carrierName": Value::Null
                },
                "label": {
                    "publicFileUrl": resolved_string_field(&label, "fileUrl").unwrap_or_default()
                }
            }
        });
        self.store
            .staged
            .reverse_deliveries
            .insert(id.clone(), delivery.clone());
        if let Some(reverse_order) = self
            .store
            .staged
            .reverse_fulfillment_orders
            .get_mut(&reverse_order_id)
        {
            if let Some(nodes) = reverse_order["reverseDeliveries"]["nodes"].as_array_mut() {
                if !nodes
                    .iter()
                    .any(|node| node["id"].as_str() == Some(id.as_str()))
                {
                    nodes.push(json!({ "id": id }));
                }
            } else {
                reverse_order["reverseDeliveries"] = json!({ "nodes": [{ "id": id }] });
            }
        }
        selected_json(
            &json!({ "reverseDelivery": delivery, "userErrors": [] }),
            &field.selection,
        )
    }

    fn update_reverse_delivery(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut delivery) = self.store.staged.reverse_deliveries.get(id).cloned() else {
            return selected_json(
                &json!({ "reverseDelivery": Value::Null, "userErrors": [user_error(["reverseDeliveryId"], "Reverse delivery does not exist", Some("NOT_FOUND"))] }),
                &field.selection,
            );
        };
        let tracking = resolved_object_field(&field.arguments, "trackingInput").unwrap_or_default();
        delivery["deliverable"]["tracking"]["number"] =
            json!(resolved_string_field(&tracking, "number").unwrap_or_default());
        delivery["deliverable"]["tracking"]["url"] =
            json!(resolved_string_field(&tracking, "url").unwrap_or_default());
        if let Some(company) = resolved_string_field(&tracking, "company") {
            delivery["deliverable"]["tracking"]["company"] = json!(company);
        }
        delivery["deliverable"]["tracking"]["carrierName"] = Value::Null;
        self.store
            .staged
            .reverse_deliveries
            .insert(id.to_string(), delivery.clone());
        selected_json(
            &json!({ "reverseDelivery": delivery, "userErrors": [] }),
            &field.selection,
        )
    }

    fn dispose_reverse_fulfillment_order(&mut self, field: &RootFieldSelection) -> Value {
        let inputs = list_object_field(&field.arguments, "dispositionInputs");
        if inputs.is_empty() {
            return selected_json(
                &json!({
                    "reverseFulfillmentOrderLineItems": Value::Null,
                    "userErrors": [user_error(["dispositionInputs"], "The array cannot be empty.", Some("BLANK"))]
                }),
                &field.selection,
            );
        }

        struct DispositionPlan {
            order_id: String,
            line_id: String,
            quantity: i64,
            disposition_type: String,
            location_id: String,
        }

        let mut plans = Vec::new();
        let mut user_errors = Vec::new();
        let mut reverse_fulfillment_order_ids = BTreeSet::new();

        for (index, input) in inputs.iter().enumerate() {
            let index = index.to_string();
            let line_id = resolved_string_field(input, "reverseFulfillmentOrderLineItemId")
                .unwrap_or_default();
            let Some((order_id, line_item)) = self
                .store
                .staged
                .reverse_fulfillment_orders
                .iter()
                .find_map(|(order_id, order)| {
                    order["lineItems"]["nodes"]
                        .as_array()
                        .and_then(|nodes| {
                            nodes
                                .iter()
                                .find(|node| node["id"].as_str() == Some(line_id.as_str()))
                        })
                        .map(|line_item| (order_id.clone(), line_item.clone()))
                })
            else {
                user_errors.push(user_error(
                    vec![
                        "dispositionInputs".to_string(),
                        index,
                        "reverseFulfillmentOrderLineItemId".to_string(),
                    ],
                    "Reverse fulfillment order line item was not found.",
                    Some("NOT_FOUND"),
                ));
                continue;
            };

            reverse_fulfillment_order_ids.insert(order_id.clone());
            let quantity = resolved_i64_field(input, "quantity").unwrap_or(0);
            let disposable_quantity = line_item["remainingQuantity"]
                .as_i64()
                .or_else(|| line_item["totalQuantity"].as_i64())
                .unwrap_or(0);
            if quantity <= 0 || quantity > disposable_quantity {
                user_errors.push(user_error(
                    vec![
                        "dispositionInputs".to_string(),
                        index,
                        "quantity".to_string(),
                    ],
                    "Quantity is invalid.",
                    Some("INVALID"),
                ));
                continue;
            }

            let disposition_type =
                resolved_string_field(input, "dispositionType").unwrap_or_default();
            let explicitly_custom_line_item = line_item
                .pointer("/fulfillmentLineItem/lineItem/variant")
                .is_some_and(Value::is_null);
            if disposition_type == "RESTOCKED" && explicitly_custom_line_item {
                user_errors.push(user_error(
                    vec![
                        "dispositionInputs".to_string(),
                        index,
                        "dispositionType".to_string(),
                    ],
                    "RESTOCKED is an invalid disposition type for a custom line item.",
                    Some("INVALID"),
                ));
                continue;
            }

            plans.push(DispositionPlan {
                order_id,
                line_id,
                quantity,
                disposition_type,
                location_id: resolved_string_field(input, "locationId").unwrap_or_default(),
            });
        }

        if user_errors.is_empty() && reverse_fulfillment_order_ids.len() > 1 {
            user_errors.push(user_error(
                ["dispositionInputs"],
                "Cannot dispose items from more than one reverse fulfillment order.",
                Some("INVALID"),
            ));
        }

        if !user_errors.is_empty() {
            return selected_json(
                &json!({
                    "reverseFulfillmentOrderLineItems": Value::Null,
                    "userErrors": user_errors
                }),
                &field.selection,
            );
        }

        let mut line_items = Vec::new();
        for plan in plans {
            let Some(order) = self
                .store
                .staged
                .reverse_fulfillment_orders
                .get_mut(&plan.order_id)
            else {
                continue;
            };
            if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                if let Some(node) = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some(plan.line_id.as_str()))
                {
                    let remaining = node["remainingQuantity"].as_i64().unwrap_or(0);
                    node["remainingQuantity"] = json!((remaining - plan.quantity).max(0));
                    node["dispositionType"] = json!(plan.disposition_type);
                    node["dispositions"] = json!([{
                        "type": node["dispositionType"].clone(),
                        "quantity": plan.quantity,
                        "location": {
                            "id": plan.location_id
                        }
                    }]);
                    line_items.push(node.clone());
                }
            }
        }
        selected_json(
            &json!({ "reverseFulfillmentOrderLineItems": line_items, "userErrors": [] }),
            &field.selection,
        )
    }

    fn process_return(&mut self, id: &str, field: &RootFieldSelection) -> Value {
        let Some(mut record) = self.store.staged.returns.get(id).cloned() else {
            return self.return_payload(
                Value::Null,
                vec![user_error(
                    ["returnId"],
                    "Return does not exist",
                    Some("NOT_FOUND"),
                )],
                &field.selection,
            );
        };
        record["status"] = json!("OPEN");
        if let Some(nodes) = record["returnLineItems"]["nodes"].as_array_mut() {
            for node in nodes {
                node["processedQuantity"] = node["quantity"].clone();
                node["unprocessedQuantity"] = json!(0);
            }
        }
        if let Some(nodes) = record["reverseFulfillmentOrders"]["nodes"].as_array_mut() {
            for node in nodes {
                let Some(id) = node["id"].as_str() else {
                    continue;
                };
                if let Some(reverse_order) = self.store.staged.reverse_fulfillment_orders.get(id) {
                    node["status"] = reverse_order["status"].clone();
                    node["lineItems"] = reverse_order["lineItems"].clone();
                }
            }
        }
        let mut stored_record = record.clone();
        stored_record["status"] = json!("CLOSED");
        self.store
            .staged
            .returns
            .insert(id.to_string(), stored_record);
        self.return_payload(record, Vec::new(), &field.selection)
    }
}

use super::*;

const APP_BILLING_VALIDATION_NOW_TIMESTAMP: &str = "2026-04-28T02:10:00.000Z";

impl DraftProxy {
    pub(in crate::proxy) fn app_subscription_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "appSubscriptionCreate".to_string()
            });
        let subscription_selection =
            selected_child_selection(&payload_selection, "appSubscription").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("AppSubscription");
        let name =
            resolved_string_field(&arguments, "name").unwrap_or_else(|| "Local plan".to_string());
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(user_error(["name"], "Name can't be blank", None));
        }
        if !arguments.contains_key("returnUrl") {
            user_errors.push(user_error(["returnUrl"], "Return url can't be blank", None));
        }
        if !arguments.contains_key("lineItems")
            || matches!(arguments.get("lineItems"), Some(ResolvedValue::List(items)) if items.is_empty())
        {
            user_errors.push(user_error(
                ["lineItems"],
                "At least one plan must be selected",
                None,
            ));
        }
        let trial_days = arguments
            .get("trialDays")
            .and_then(|value| match value {
                ResolvedValue::Int(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(0);
        let test = arguments
            .get("test")
            .and_then(|value| match value {
                ResolvedValue::Bool(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);
        let line_items = app_subscription_line_items_from_arguments(&arguments, &[]);
        if app_subscription_line_item_currency_codes(&line_items).len() > 1 {
            user_errors.push(user_error(
                ["lineItems"],
                "All pricing plans must use the same currency.",
                None,
            ));
        }
        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: app_subscription_payload_json(
                        Value::Null,
                        &payload_selection,
                        &subscription_selection,
                        user_errors,
                    )
                }
            }));
        }
        let line_item_ids = line_items
            .iter()
            .map(|_| self.next_proxy_synthetic_gid("AppSubscriptionLineItem"))
            .collect::<Vec<_>>();
        let line_items = app_subscription_line_items_from_arguments(&arguments, &line_item_ids);
        let confirmation_url = app_domain_confirmation_url_from_arguments(&arguments);
        let subscription = json!({
            "__typename": "AppSubscription",
            "id": id,
            "name": name,
            "status": if test { "ACTIVE" } else { "PENDING" },
            "test": test,
            "trialDays": trial_days,
            "currentPeriodEnd": app_subscription_current_period_end(trial_days),
            "lineItems": line_items
        });
        self.store
            .staged
            .app_subscriptions
            .insert(id.clone(), subscription.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "appSubscriptionCreate",
            vec![id],
        );

        ok_json(json!({
            "data": {
                response_key: app_subscription_create_payload_json(
                    &subscription,
                    &payload_selection,
                    &subscription_selection,
                    json!(confirmation_url),
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_subscription_cancel(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "appSubscriptionCancel".to_string()
            });
        let subscription_selection =
            selected_child_selection(&payload_selection, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();

        let (subscription, user_errors) = match self.store.staged.app_subscriptions.get_mut(&id) {
            Some(record) if record["status"] == "CANCELLED" => (
                Value::Null,
                vec![user_error_omit_code(
                    ["id"],
                    "Cannot transition status via :cancel from :cancelled",
                    None,
                )],
            ),
            Some(record) => {
                if let Value::Object(fields) = record {
                    fields.insert("status".to_string(), json!("CANCELLED"));
                }
                let updated = record.clone();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "appSubscriptionCancel",
                    vec![id],
                );
                (updated, vec![])
            }
            None => (
                Value::Null,
                vec![user_error_omit_code(
                    ["id"],
                    "Couldn't find RecurringApplicationCharge",
                    None,
                )],
            ),
        };

        ok_json(json!({
            "data": {
                response_key: app_subscription_payload_json(
                    subscription,
                    &payload_selection,
                    &subscription_selection,
                    user_errors,
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_subscription_trial_extend(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "appSubscriptionTrialExtend".to_string()
            });
        let subscription_selection =
            selected_child_selection(&payload_selection, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let days = resolved_int_field(&arguments, "days").unwrap_or(0);

        let (subscription, user_errors) = if days <= 0 {
            (
                Value::Null,
                vec![user_error(["days"], "Days must be greater than 0", None)],
            )
        } else if days > 1000 {
            (
                Value::Null,
                vec![user_error(
                    ["days"],
                    "Days must be less than or equal to 1000",
                    None,
                )],
            )
        } else {
            match self.store.staged.app_subscriptions.get_mut(&id) {
                None => (
                    Value::Null,
                    vec![user_error(
                        ["id"],
                        "The app subscription wasn't found.",
                        Some("SUBSCRIPTION_NOT_FOUND"),
                    )],
                ),
                Some(record) if record["status"] != "ACTIVE" => (
                    Value::Null,
                    vec![user_error(
                        ["id"],
                        "The trial can't be extended on inactive app subscriptions.",
                        Some("SUBSCRIPTION_NOT_ACTIVE"),
                    )],
                ),
                Some(record) if !app_subscription_trial_is_active(record) => (
                    Value::Null,
                    vec![user_error_omit_code(
                        ["id"],
                        "The trial can't be extended after expiration.",
                        None,
                    )],
                ),
                Some(record) => {
                    let current = record["trialDays"].as_i64().unwrap_or(0);
                    let updated_trial_days = current + days;
                    if let Value::Object(fields) = record {
                        fields.insert("trialDays".to_string(), json!(updated_trial_days));
                        fields.insert(
                            "currentPeriodEnd".to_string(),
                            json!(app_subscription_current_period_end(updated_trial_days)),
                        );
                    }
                    let updated = record.clone();
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        "appSubscriptionTrialExtend",
                        vec![id],
                    );
                    (updated, vec![])
                }
            }
        };

        ok_json(json!({
            "data": {
                response_key: app_subscription_payload_json(
                    subscription,
                    &payload_selection,
                    &subscription_selection,
                    user_errors,
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_subscription_line_item_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for root in self
            .execution_root_fields(query, variables)
            .unwrap_or_default()
            .into_iter()
            .filter(|root| root.name == "appSubscriptionLineItemUpdate")
        {
            let subscription_selection =
                selected_child_selection(&root.selection, "appSubscription").unwrap_or_default();
            let id = resolved_string_field(&root.arguments, "id").unwrap_or_default();
            let capped = match root.arguments.get("cappedAmount") {
                Some(ResolvedValue::Object(value)) => value,
                _ => {
                    data.insert(
                        root.response_key,
                        app_subscription_payload_json(
                            Value::Null,
                            &root.selection,
                            &subscription_selection,
                            vec![user_error_omit_code(
                                ["cappedAmount"],
                                "Capped amount is required",
                                None,
                            )],
                        ),
                    );
                    continue;
                }
            };
            let requested_amount = money_amount_string_from_resolved(capped.get("amount"));
            let requested_currency = match capped.get("currencyCode") {
                Some(ResolvedValue::String(value)) => value.clone(),
                _ => "USD".to_string(),
            };
            let require_approval = match root.arguments.get("requireApproval") {
                Some(ResolvedValue::Bool(value)) => *value,
                _ => true,
            };

            let mut matched_subscription_id = None;
            let mut matched_line_item = None;
            let mut matched_line_item_index = None;
            for (subscription_id, subscription) in &self.store.staged.app_subscriptions {
                if let Some(line_items) = subscription["lineItems"].as_array() {
                    if let Some((index, line_item)) = line_items
                        .iter()
                        .enumerate()
                        .find(|(_, line_item)| line_item["id"] == id)
                    {
                        matched_subscription_id = Some(subscription_id.clone());
                        matched_line_item = Some(line_item.clone());
                        matched_line_item_index = Some(index);
                        break;
                    }
                }
            }

            let (subscription, user_errors) = match (
                matched_subscription_id,
                matched_line_item,
                matched_line_item_index,
            ) {
                (Some(subscription_id), Some(line_item), Some(line_item_index)) => {
                    let pricing = &line_item["plan"]["pricingDetails"];
                    if pricing["__typename"] != "AppUsagePricing" {
                        (
                            Value::Null,
                            vec![user_error_omit_code(
                                Value::Null,
                                "Only variable subscriptions can be updated.",
                                None,
                            )],
                        )
                    } else {
                        let existing_currency = pricing["cappedAmount"]["currencyCode"]
                            .as_str()
                            .unwrap_or("USD");
                        let existing_amount = pricing["cappedAmount"]["amount"]
                            .as_str()
                            .and_then(|amount| amount.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let requested_amount_number =
                            requested_amount.parse::<f64>().unwrap_or(0.0);
                        if requested_currency != existing_currency {
                            (
                                Value::Null,
                                vec![user_error_omit_code(
                                    Value::Null,
                                    &format!("Currency code must be {existing_currency}"),
                                    None,
                                )],
                            )
                        } else if requested_amount_number <= existing_amount {
                            (
                                Value::Null,
                                vec![user_error_omit_code(["cappedAmount"], "Spending limit can only be increased. Please contact the app developer to decrease spending limit.", None)],
                            )
                        } else {
                            let subscription = if require_approval {
                                self.store
                                    .staged
                                    .app_subscriptions
                                    .get(&subscription_id)
                                    .cloned()
                                    .unwrap_or(Value::Null)
                            } else {
                                let subscription = self
                                    .store
                                    .staged
                                    .app_subscriptions
                                    .get_mut(&subscription_id)
                                    .expect("located subscription must still exist");
                                if let Some(line_item) = subscription["lineItems"]
                                    .as_array_mut()
                                    .and_then(|line_items| line_items.get_mut(line_item_index))
                                {
                                    line_item["plan"]["pricingDetails"]["cappedAmount"] = json!({
                                        "amount": requested_amount,
                                        "currencyCode": requested_currency
                                    });
                                }
                                subscription.clone()
                            };
                            self.record_mutation_log_entry(
                                request,
                                query,
                                variables,
                                "appSubscriptionLineItemUpdate",
                                vec![subscription_id],
                            );
                            (subscription, vec![])
                        }
                    }
                }
                _ => (
                    Value::Null,
                    vec![user_error_omit_code(["id"], "Invalid id", None)],
                ),
            };

            data.insert(
                root.response_key,
                app_subscription_payload_json_with_confirmation_url(
                    subscription,
                    &root.selection,
                    &subscription_selection,
                    user_errors,
                    require_approval.then(|| {
                        json!(app_domain_confirmation_url_for_request(
                            request,
                            &self.config.shopify_admin_origin,
                        ))
                    }),
                ),
            );
        }

        ok_json(json!({ "data": data }))
    }

    pub(super) fn find_staged_app_subscription_line_item(
        &self,
        line_item_id: &str,
    ) -> Option<(String, usize)> {
        self.store
            .staged
            .app_subscriptions
            .iter()
            .find_map(|(subscription_id, subscription)| {
                subscription["lineItems"]
                    .as_array()
                    .and_then(|items| {
                        items
                            .iter()
                            .position(|line_item| line_item["id"] == line_item_id)
                    })
                    .map(|index| (subscription_id.clone(), index))
            })
    }
    pub(in crate::proxy) fn app_usage_record_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "appUsageRecordCreate".to_string()
            });
        let usage_record_selection =
            selected_child_selection(&payload_selection, "appUsageRecord").unwrap_or_default();
        let line_item_id =
            resolved_string_field(&arguments, "subscriptionLineItemId").unwrap_or_default();
        let idempotency_key =
            resolved_string_field(&arguments, "idempotencyKey").unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price,
            _ => {
                return ok_json(json!({
                    "data": { response_key: app_usage_record_payload_json(
                        Value::Null,
                        &payload_selection,
                        &usage_record_selection,
                        vec![user_error(["price"], "Price is required", None)],
                    ) }
                }));
            }
        };
        let amount = money_amount_string_from_resolved(price.get("amount"));
        let currency = match price.get("currencyCode") {
            Some(ResolvedValue::String(value)) => value.clone(),
            _ => "USD".to_string(),
        };
        let description = resolved_string_field(&arguments, "description").unwrap_or_default();

        let mut usage_record = Value::Null;
        let mut user_errors = Vec::new();
        let mut should_record_success = false;
        let mut created_usage_record_id = None;
        if idempotency_key.len() > 255 {
            user_errors.push(user_error(
                ["idempotencyKey"],
                "Idempotency key exceeds the maximum length.",
                None,
            ));
        } else if description.trim().is_empty() {
            user_errors.push(user_error(
                ["description"],
                "Description can't be blank",
                None,
            ));
        } else if shopify_gid_resource_type(&line_item_id) != Some("AppSubscriptionLineItem") {
            user_errors.push(user_error(["subscriptionLineItemId"], "Invalid id", None));
        } else if let Some((subscription_id, line_item_index)) =
            self.find_staged_app_subscription_line_item(&line_item_id)
        {
            let candidate_usage_record_id = self.next_proxy_synthetic_gid("AppUsageRecord");
            let subscription = self
                .store
                .staged
                .app_subscriptions
                .get_mut(&subscription_id)
                .expect("located subscription must still exist");
            let line_item = subscription["lineItems"]
                .as_array_mut()
                .and_then(|items| items.get_mut(line_item_index))
                .expect("located line item must still exist");
            let pricing = &line_item["plan"]["pricingDetails"];
            let existing_currency = pricing["cappedAmount"]["currencyCode"]
                .as_str()
                .unwrap_or("USD")
                .to_string();
            let capped_amount = pricing["cappedAmount"]["amount"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let current_balance = pricing["balanceUsed"]["amount"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let requested_amount = amount.parse::<f64>().unwrap_or(0.0);
            let existing = line_item["usageRecords"]["nodes"]
                .as_array()
                .and_then(|records| {
                    records
                        .iter()
                        .find(|record| {
                            record["idempotencyKey"] == idempotency_key
                                && record["apiClientId"] == request_api_client_id(request)
                        })
                        .cloned()
                });
            if let Some(record) = existing {
                usage_record = record;
            } else if currency != existing_currency
                || current_balance + requested_amount > capped_amount
            {
                user_errors.push(user_error_omit_code(
                    Value::Null,
                    "Total price exceeds balance remaining",
                    None,
                ));
            } else {
                let new_balance = if current_balance == 0.0 {
                    amount.clone()
                } else {
                    format_money_amount(current_balance + requested_amount)
                };
                line_item["plan"]["pricingDetails"]["balanceUsed"] = json!({
                    "amount": new_balance,
                    "currencyCode": existing_currency
                });
                let subscription_line_item = line_item.clone();
                usage_record = json!({
                    "id": candidate_usage_record_id,
                    "description": description,
                    "price": money_value(&amount, &currency),
                    "idempotencyKey": idempotency_key,
                    "apiClientId": request_api_client_id(request),
                    "subscriptionLineItem": subscription_line_item
                });
                if !line_item["usageRecords"].is_object() {
                    line_item["usageRecords"] = json!({ "nodes": [] });
                }
                if let Some(records) = line_item["usageRecords"]["nodes"].as_array_mut() {
                    records.push(usage_record.clone());
                }
                created_usage_record_id = usage_record["id"].as_str().map(str::to_string);
                should_record_success = true;
            }
        } else {
            user_errors.push(user_error(["subscriptionLineItemId"], "Invalid id", None));
        }

        if should_record_success {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "appUsageRecordCreate",
                vec![created_usage_record_id.unwrap_or(line_item_id)],
            );
        }

        ok_json(json!({
            "data": {
                response_key: app_usage_record_payload_json(
                    usage_record,
                    &payload_selection,
                    &usage_record_selection,
                    user_errors,
                )
            }
        }))
    }
}

fn app_subscription_trial_is_active(subscription: &Value) -> bool {
    let Some(trial_days) = subscription.get("trialDays").and_then(Value::as_i64) else {
        return false;
    };
    if trial_days <= 0 {
        return false;
    }
    subscription
        .get("currentPeriodEnd")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_epoch_seconds)
        .is_some_and(|period_end| {
            parse_rfc3339_epoch_seconds(APP_BILLING_VALIDATION_NOW_TIMESTAMP)
                .is_some_and(|now| period_end > now)
        })
}

fn app_subscription_current_period_end(trial_days: i64) -> String {
    let now = parse_rfc3339_epoch_seconds(APP_BILLING_VALIDATION_NOW_TIMESTAMP).unwrap_or(0);
    format_epoch_seconds_utc_millis(now + trial_days.max(0) * 86_400)
}

fn format_epoch_seconds_utc_millis(seconds: i64) -> String {
    let days = epoch_seconds_to_utc_epoch_days(seconds);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000Z")
}

use crate::proxy::*;

const APP_DOMAIN_SYNTHETIC_NOW: &str = "2026-04-28T02:10:00.000Z";

impl DraftProxy {
    pub(in crate::proxy) fn current_app_installation_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "currentAppInstallation" {
                return None;
            }
            let value = if self.store.staged.app_uninstalled {
                Value::Null
            } else {
                current_app_installation_json(
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &self.store.staged.revoked_app_access_scopes,
                    &field.selection,
                )
            };
            Some(value)
        })
    }

    pub(in crate::proxy) fn find_staged_app_usage_record(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .app_subscriptions
            .values()
            .find_map(|subscription| {
                subscription["lineItems"].as_array().and_then(|line_items| {
                    line_items.iter().find_map(|line_item| {
                        line_item["usageRecords"]["nodes"]
                            .as_array()
                            .and_then(|records| {
                                records.iter().find(|record| record["id"] == id).cloned()
                            })
                    })
                })
            })
    }

    pub(in crate::proxy) fn app_uninstall(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appUninstall".to_string());
        let app_selection = selected_child_selection(&payload_selection, "app").unwrap_or_default();
        let requested_id = resolved_object_field(&arguments, "input")
            .and_then(|input| resolved_string_field(&input, "id"));

        let (app, user_errors) = match requested_id.as_deref() {
            Some("gid://shopify/App/expected") if self.store.staged.app_uninstalled => (
                Value::Null,
                vec![user_error(
                    ["id"],
                    "App is not installed on this shop.",
                    Some("APP_NOT_INSTALLED"),
                )],
            ),
            Some(id) if id != "gid://shopify/App/expected" && id != "gid://shopify/App/2" => (
                Value::Null,
                vec![user_error(
                    ["id"],
                    "The app cannot be found.",
                    Some("APP_NOT_FOUND"),
                )],
            ),
            _ => {
                self.store.staged.app_uninstalled = true;
                for subscription in self.store.staged.app_subscriptions.values_mut() {
                    if let Value::Object(fields) = subscription {
                        fields.insert("status".to_string(), json!("CANCELLED"));
                    }
                }
                self.store.staged.delegate_access_tokens.clear();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "appUninstall",
                    vec!["gid://shopify/App/expected".to_string()],
                );
                (local_app_json(), vec![])
            }
        };
        ok_json(json!({
            "data": {
                response_key: app_uninstall_payload_json(
                    app,
                    &payload_selection,
                    &app_selection,
                    user_errors,
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_subscription_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appSubscriptionCreate".to_string());
        let subscription_selection =
            selected_child_selection(&payload_selection, "appSubscription").unwrap_or_default();
        let id = LOCAL_APP_SUBSCRIPTION_ACTIVATION_ID.to_string();
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
        let line_items = app_subscription_line_items_from_arguments(&arguments);
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
        let subscription = json!({
            "__typename": "AppSubscription",
            "id": id,
            "name": name,
            "status": if test { "ACTIVE" } else { "PENDING" },
            "test": test,
            "trialDays": trial_days,
            "currentPeriodEnd": "2024-02-07T00:00:00.000Z",
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
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appSubscriptionCancel".to_string());
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
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
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
                    if let Value::Object(fields) = record {
                        fields.insert("trialDays".to_string(), json!(current + days));
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
        for root in root_fields(query, variables)
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
            let requested_amount = resolved_money_amount_string(capped.get("amount"));
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
                    require_approval.then(|| json!("https://app.example.test/local-confirmation")),
                ),
            );
        }

        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn find_staged_app_subscription_line_item(
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
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appUsageRecordCreate".to_string());
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
        let amount = resolved_money_amount_string(price.get("amount"));
        let currency = match price.get("currencyCode") {
            Some(ResolvedValue::String(value)) => value.clone(),
            _ => "USD".to_string(),
        };
        let description = resolved_string_field(&arguments, "description").unwrap_or_default();

        let mut usage_record = Value::Null;
        let mut user_errors = Vec::new();
        let mut should_record_success = false;
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
                    "id": "gid://shopify/AppUsageRecord/expected",
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
                vec![line_item_id],
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

    pub(in crate::proxy) fn delegate_access_token_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "delegateAccessTokenCreate".to_string()
            });
        let token_selection =
            selected_child_selection(&payload_selection, "delegateAccessToken").unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let scopes = input
            .get("delegateAccessScope")
            .or_else(|| input.get("accessScopes"))
            .map(resolved_string_list)
            .unwrap_or_default();
        let expires_in = match input.get("expiresIn") {
            Some(ResolvedValue::Int(value)) => *value,
            _ => 3600,
        };

        let mut user_errors = Vec::new();
        if scopes.is_empty() {
            user_errors.push(user_error(
                Value::Null,
                "The access scope can't be empty.",
                Some("EMPTY_ACCESS_SCOPE"),
            ));
        } else if expires_in <= 0 {
            user_errors.push(user_error(
                Value::Null,
                "The expires_in value must be greater than 0.",
                Some("NEGATIVE_EXPIRES_IN"),
            ));
        } else if delegate_expires_after_parent(request, expires_in) {
            user_errors.push(user_error(
                Value::Null,
                "The delegate token can't expire after the parent token.",
                Some("EXPIRES_AFTER_PARENT"),
            ));
        } else if let Some(scope) = scopes.iter().find(|scope| {
            !matches!(
                scope.as_str(),
                "read_products" | "write_products" | "read_markets" | "write_markets"
            )
        }) {
            user_errors.push(user_error(
                Value::Null,
                &format!("The access scope is invalid: {scope}"),
                Some("UNKNOWN_SCOPES"),
            ));
        }

        if !user_errors.is_empty() {
            if user_errors.iter().any(|error| {
                error.get("code").and_then(Value::as_str) == Some("EXPIRES_AFTER_PARENT")
            }) {
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "delegateAccessTokenCreate",
                    vec![],
                );
                if let Some(entry) = self.log_entries.last_mut() {
                    set_log_status(entry, "failed");
                }
            }
            return ok_json(json!({
                "data": {
                    response_key: delegate_access_token_create_payload_json(
                        Value::Null,
                        &payload_selection,
                        &token_selection,
                        user_errors,
                    )
                }
            }));
        }

        let token = format!(
            "shpat_delegate_proxy_{}",
            self.store.staged.delegate_access_tokens.len() + 1
        );
        let parent_access_token =
            request_access_token(request).unwrap_or_else(|| "shpat_parent_default".to_string());
        let api_client_id = request_api_client_id(request);
        let record = json!({
            "accessToken": token,
            "accessScopes": scopes,
            "createdAt": APP_DOMAIN_SYNTHETIC_NOW,
            "expiresIn": expires_in,
            "parentAccessToken": parent_access_token,
            "apiClientId": api_client_id
        });
        self.store
            .staged
            .delegate_access_tokens
            .insert(token.clone(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "delegateAccessTokenCreate",
            vec![token],
        );

        ok_json(json!({
            "data": {
                response_key: delegate_access_token_create_payload_json(
                    record,
                    &payload_selection,
                    &token_selection,
                    vec![],
                )
            }
        }))
    }

    pub(in crate::proxy) fn delegate_access_token_destroy(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "delegateAccessTokenDestroy".to_string()
            });
        let token = resolved_string_field(&arguments, "accessToken").unwrap_or_default();
        let caller_token = request_access_token(request).unwrap_or_default();
        let caller_api_client_id = request_api_client_id(request);

        let mut status = false;
        let mut user_errors = Vec::new();
        if !caller_token.is_empty()
            && caller_token == token
            && !token.starts_with("shpat_delegate_proxy_")
        {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Can only delete delegate tokens.",
                "CAN_ONLY_DELETE_DELEGATE_TOKENS",
            ));
        } else if caller_token.starts_with("shpat_delegate_proxy_") && caller_token != token {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access denied.",
                "ACCESS_DENIED",
            ));
        } else if self.store.staged.app_uninstalled {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
            ));
        } else if let Some(record) = self.store.staged.delegate_access_tokens.get(&token) {
            let token_api_client_id = record
                .get("apiClientId")
                .and_then(Value::as_str)
                .unwrap_or("gid://shopify/App/local");
            if token_api_client_id != caller_api_client_id {
                user_errors.push(delegate_access_token_destroy_user_error(
                    "Access denied.",
                    "ACCESS_DENIED",
                ));
            } else {
                self.store.staged.delegate_access_tokens.remove(&token);
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "delegateAccessTokenDestroy",
                    vec![token],
                );
                status = true;
            }
        } else {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
            ));
        }

        ok_json(json!({
            "data": {
                response_key: delegate_access_token_destroy_payload_json(
                    status,
                    user_errors,
                    &payload_selection,
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_revoke_access_scopes(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appRevokeAccessScopes".to_string());
        let scopes = arguments
            .get("scopes")
            .map(resolved_string_list)
            .unwrap_or_default();

        let mut user_errors = Vec::new();
        if app_revoke_access_scopes_missing_source_app(request) {
            user_errors.push(user_error(
                ["id"],
                "No app found on the access token.",
                Some("MISSING_SOURCE_APP"),
            ));
        } else {
            if scopes.iter().any(|scope| scope == "read_products") {
                user_errors.push(user_error(
                    ["scopes"],
                    "Scopes that are declared as required cannot be revoked.",
                    Some("CANNOT_REVOKE_REQUIRED_SCOPES"),
                ));
            }
            if scopes
                .iter()
                .any(|scope| !matches!(scope.as_str(), "read_products" | "write_products"))
            {
                user_errors.push(user_error(
                    ["scopes"],
                    "The requested list of scopes to revoke includes invalid handles.",
                    Some("UNKNOWN_SCOPES"),
                ));
            }
        }

        let revoked = if user_errors.is_empty() {
            for scope in &scopes {
                self.store
                    .staged
                    .revoked_app_access_scopes
                    .insert(scope.clone());
            }
            scopes
                .iter()
                .map(|scope| json!({ "handle": scope, "description": null }))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if user_errors.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "appRevokeAccessScopes",
                scopes.clone(),
            );
        }

        ok_json(json!({
            "data": {
                response_key: app_revoke_access_scopes_payload_json(
                    revoked,
                    user_errors,
                    &payload_selection,
                )
            }
        }))
    }

    pub(in crate::proxy) fn app_purchase_one_time_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "appPurchaseOneTimeCreate".to_string()
            });
        let purchase_selection =
            selected_child_selection(&payload_selection, "appPurchaseOneTime").unwrap_or_default();

        if !arguments.contains_key("returnUrl") {
            return ok_json(json!({
                "errors": [{
                    "message": "Field 'appPurchaseOneTimeCreate' is missing required arguments: returnUrl",
                    "locations": [{ "line": 2, "column": 3 }],
                    "path": ["mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl", "appPurchaseOneTimeCreate"],
                    "extensions": {
                        "code": "missingRequiredArguments",
                        "className": "Field",
                        "name": "appPurchaseOneTimeCreate",
                        "arguments": "returnUrl"
                    }
                }]
            }));
        }

        let name = arguments
            .get("name")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price.clone(),
            _ => BTreeMap::new(),
        };
        let amount = resolved_money_amount_string(price.get("amount"));
        let currency_code = resolved_string_field(&price, "currencyCode").unwrap_or_default();
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(user_error(["name"], "Name can't be blank", None));
        } else if amount.parse::<f64>().unwrap_or(0.0) < 0.50 {
            user_errors.push(user_error(
                Value::Null,
                "Validation failed: Price must be greater than or equal to 0.5",
                None,
            ));
        } else if currency_code != "USD" {
            user_errors.push(user_error(["price"], "Currency code must be USD", None));
        }

        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: app_purchase_one_time_payload_json(
                        Value::Null,
                        &payload_selection,
                        &purchase_selection,
                        user_errors,
                    )
                }
            }));
        }

        let purchase = json!({
            "id": LOCAL_APP_PURCHASE_ONE_TIME_ID,
            "name": name,
            "status": "ACTIVE",
            "test": resolved_bool_field(&arguments, "test").unwrap_or(false),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "price": money_value(&amount, &currency_code)
        });
        self.store
            .staged
            .app_one_time_purchases
            .insert(LOCAL_APP_PURCHASE_ONE_TIME_ID.to_string(), purchase.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "appPurchaseOneTimeCreate",
            vec![LOCAL_APP_PURCHASE_ONE_TIME_ID.to_string()],
        );

        ok_json(json!({
            "data": {
                response_key: app_purchase_one_time_payload_json(
                    purchase,
                    &payload_selection,
                    &purchase_selection,
                    vec![],
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
            parse_rfc3339_epoch_seconds(APP_DOMAIN_SYNTHETIC_NOW)
                .is_some_and(|now| period_end > now)
        })
}

fn delegate_expires_after_parent(request: &Request, expires_in: i64) -> bool {
    let Some(parent_expires_at) =
        request_header(request, "x-shopify-draft-proxy-access-token-expires-at")
            .and_then(|value| parse_rfc3339_epoch_seconds(&value))
    else {
        return false;
    };
    let Some(created_at) = parse_rfc3339_epoch_seconds(APP_DOMAIN_SYNTHETIC_NOW) else {
        return false;
    };
    created_at + expires_in > parent_expires_at
}

fn app_revoke_access_scopes_missing_source_app(request: &Request) -> bool {
    request_header(request, "x-shopify-draft-proxy-source-app-missing")
        .as_deref()
        .is_some_and(|value| matches!(value, "1" | "true" | "TRUE" | "True"))
}

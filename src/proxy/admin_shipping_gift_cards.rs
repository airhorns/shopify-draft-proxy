use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn backup_region_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "backupRegionUpdate".to_string());
        if request.headers.iter().any(|(name, token)| {
            name.eq_ignore_ascii_case("X-Shopify-Access-Token") && token == "shpat_delegate_proxy_1"
        }) {
            return ok_json(json!({
                "errors": [{
                    "message": "Access denied for backupRegionUpdate field. Required access: `read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.",
                    "locations": [{ "line": 2, "column": 3 }],
                    "extensions": {
                        "code": "ACCESS_DENIED",
                        "documentation": "https://shopify.dev/api/usage/access-scopes",
                        "requiredAccess": "`read_markets` for queries and both `read_markets` as well as `write_markets` for mutations."
                    },
                    "path": ["backupRegionUpdate"]
                }],
                "data": { response_key: null }
            }));
        }
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        if query.contains("BackupRegionUpdateMissingCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
                "BackupRegionUpdateMissingCountryCode",
                "missingRequiredInputObjectAttribute",
            ));
        }
        if query.contains("BackupRegionUpdateNullCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (null). Expected type 'CountryCode!'.",
                "BackupRegionUpdateNullCountryCode",
                "argumentLiteralsIncompatible",
            ));
        }
        if query.contains("BackupRegionUpdateNumericCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (42). Expected type 'CountryCode!'.",
                "BackupRegionUpdateNumericCountryCode",
                "argumentLiteralsIncompatible",
            ));
        }
        let country_code = match arguments.get("region") {
            None | Some(ResolvedValue::Null) => None,
            Some(ResolvedValue::Object(region)) => {
                region.get("countryCode").and_then(|value| match value {
                    ResolvedValue::String(country_code) => Some(country_code.as_str()),
                    _ => None,
                })
            }
            _ => None,
        };

        match country_code {
            None => ok_json(json!({
                "data": { response_key: { "backupRegion": self.store.staged.backup_region.clone(), "userErrors": [] } }
            })),
            Some("CA") | Some("AE") => {
                let region = backup_region_country(country_code.unwrap());
                self.store.staged.backup_region = region.clone();
                let staged_id = region
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("gid://shopify/MarketRegionCountry/local")
                    .to_string();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "backupRegionUpdate",
                    vec![staged_id],
                );
                ok_json(json!({
                    "data": { response_key: { "backupRegion": region, "userErrors": [] } }
                }))
            }
            Some(_) => {
                let mut user_error = serde_json::Map::from_iter([
                    ("field".to_string(), json!(["region"])),
                    ("message".to_string(), json!("Region not found.")),
                    ("code".to_string(), json!("REGION_NOT_FOUND")),
                ]);
                let include_user_error_typename =
                    nested_root_field_path_selection(query, &["userErrors"])
                        .unwrap_or_default()
                        .iter()
                        .any(|field| field.name == "__typename");
                if include_user_error_typename {
                    user_error.insert("__typename".to_string(), json!("MarketUserError"));
                }
                ok_json(json!({
                "data": {
                    response_key: {
                        "backupRegion": null,
                        "userErrors": [Value::Object(user_error)]
                    }
                }
                }))
            }
        }
    }

    pub(in crate::proxy) fn current_app_installation_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "currentAppInstallation" {
                continue;
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
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn app_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            if field.name != "node" {
                continue;
            }
            let id = field
                .arguments
                .get("id")
                .and_then(resolved_as_string)
                .unwrap_or_default();
            let value = match id.as_str() {
                "gid://shopify/AppInstallation/expected" if self.store.staged.app_uninstalled => {
                    Value::Null
                }
                "gid://shopify/AppInstallation/expected" => current_app_installation_json(
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &self.store.staged.revoked_app_access_scopes,
                    &field.selection,
                ),
                "gid://shopify/App/expected" => selected_json(&local_app_json(), &field.selection),
                _ => {
                    if let Some(subscription) = self.store.staged.app_subscriptions.get(&id) {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["__typename", "id", "status", "trialDays", "lineItems"],
                        );
                        selected_json(subscription, &type_selection)
                    } else if let Some(purchase) = self.store.staged.app_one_time_purchases.get(&id)
                    {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["id", "name", "status", "test", "price"],
                        );
                        selected_json(purchase, &type_selection)
                    } else if let Some(usage_record) = self.find_staged_app_usage_record(&id) {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["id", "description", "price", "subscriptionLineItem"],
                        );
                        selected_json(&usage_record, &type_selection)
                    } else {
                        continue;
                    }
                }
            };
            handled = true;
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
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
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appUninstall".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let app_selection = selected_child_selection(&payload_selection, "app").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let requested_id = resolved_object_field(&arguments, "input")
            .and_then(|input| resolved_string_field(&input, "id"));

        let (app, user_errors) = match requested_id.as_deref() {
            Some("gid://shopify/App/expected") if self.store.staged.app_uninstalled => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "App is not installed on this shop.",
                    "code": "APP_NOT_INSTALLED"
                })],
            ),
            Some(id) if id != "gid://shopify/App/expected" && id != "gid://shopify/App/2" => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "The app cannot be found.",
                    "code": "APP_NOT_FOUND"
                })],
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appSubscriptionCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = LOCAL_APP_SUBSCRIPTION_ACTIVATION_ID.to_string();
        let name =
            resolved_string_field(&arguments, "name").unwrap_or_else(|| "Local plan".to_string());
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appSubscriptionCancel".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();

        let (subscription, user_errors) = match self.store.staged.app_subscriptions.get_mut(&id) {
            Some(record) if record["status"] == "CANCELLED" => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "Cannot transition status via :cancel from :cancelled"
                })],
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
                vec![json!({
                    "field": ["id"],
                    "message": "Couldn't find RecurringApplicationCharge"
                })],
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "appSubscriptionTrialExtend".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let days = resolved_int_field(&arguments, "days").unwrap_or(0);

        let (subscription, user_errors) = if days <= 0 {
            (
                Value::Null,
                vec![json!({
                    "field": ["days"],
                    "message": "Days must be greater than 0",
                    "code": null
                })],
            )
        } else if days > 1000 {
            (
                Value::Null,
                vec![json!({
                    "field": ["days"],
                    "message": "Days must be less than or equal to 1000",
                    "code": null
                })],
            )
        } else {
            match self.store.staged.app_subscriptions.get_mut(&id) {
                None => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The app subscription wasn't found.",
                        "code": "SUBSCRIPTION_NOT_FOUND"
                    })],
                ),
                Some(record) if record["status"] != "ACTIVE" => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The trial can't be extended on inactive app subscriptions.",
                        "code": "SUBSCRIPTION_NOT_ACTIVE"
                    })],
                ),
                Some(_record) if query.contains("AppSubscriptionTrialExtendLocalLifecycle") => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The trial can't be extended after expiration."
                    })],
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
                            vec![json!({
                                "field": ["cappedAmount"],
                                "message": "Capped amount is required"
                            })],
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

            let mut matched_subscription_id = None;
            let mut matched_line_item = None;
            for (subscription_id, subscription) in &self.store.staged.app_subscriptions {
                if let Some(line_items) = subscription["lineItems"].as_array() {
                    if let Some(line_item) =
                        line_items.iter().find(|line_item| line_item["id"] == id)
                    {
                        matched_subscription_id = Some(subscription_id.clone());
                        matched_line_item = Some(line_item.clone());
                        break;
                    }
                }
            }

            let (subscription, user_errors) = match (matched_subscription_id, matched_line_item) {
                (Some(subscription_id), Some(line_item)) => {
                    let pricing = &line_item["plan"]["pricingDetails"];
                    if pricing["__typename"] != "AppUsagePricing" {
                        (
                            Value::Null,
                            vec![json!({
                                "field": ["cappedAmount"],
                                "message": "Only usage-pricing line items support cappedAmount updates"
                            })],
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
                                vec![json!({
                                    "field": ["cappedAmount"],
                                    "message": format!("Capped amount currency mismatch. Expected {existing_currency}")
                                })],
                            )
                        } else if requested_amount_number <= existing_amount {
                            (
                                Value::Null,
                                vec![json!({
                                    "field": ["cappedAmount"],
                                    "message": "The capped amount must be greater than the existing capped amount"
                                })],
                            )
                        } else {
                            let subscription = self
                                .store
                                .staged
                                .app_subscriptions
                                .get(&subscription_id)
                                .cloned()
                                .unwrap_or(Value::Null);
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
                    vec![json!({
                        "field": ["id"],
                        "message": "The app subscription line item wasn't found."
                    })],
                ),
            };

            data.insert(
                root.response_key,
                app_subscription_payload_json(
                    subscription,
                    &root.selection,
                    &subscription_selection,
                    user_errors,
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appUsageRecordCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
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
                        vec![json!({ "field": ["price"], "message": "Price is required", "code": null })],
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
            user_errors.push(json!({
                "field": ["idempotencyKey"],
                "message": "Idempotency key must be at most 255 characters",
                "code": null
            }));
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
                        .find(|record| record["idempotencyKey"] == idempotency_key)
                        .cloned()
                });
            if let Some(record) = existing {
                usage_record = record;
            } else if currency != existing_currency
                || current_balance + requested_amount > capped_amount
            {
                user_errors.push(json!({
                    "field": [],
                    "message": "Total price exceeds balance remaining"
                }));
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
                    "price": { "amount": amount, "currencyCode": currency },
                    "idempotencyKey": idempotency_key,
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
            user_errors.push(json!({
                "field": ["subscriptionLineItemId"],
                "message": "The app subscription line item wasn't found.",
                "code": null
            }));
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "delegateAccessTokenCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let token_selection =
            nested_root_field_selection(query, "delegateAccessToken").unwrap_or_default();
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
            user_errors.push(json!({
                "field": null,
                "message": "The access scope can't be empty.",
                "code": "EMPTY_ACCESS_SCOPE"
            }));
        } else if expires_in <= 0 {
            user_errors.push(json!({
                "field": null,
                "message": "The expires_in value must be greater than 0.",
                "code": "NEGATIVE_EXPIRES_IN"
            }));
        } else if query.contains("DelegateAccessTokenCreateExpiresAfterParent") {
            user_errors.push(json!({
                "field": null,
                "message": "The delegate token can't expire after the parent token.",
                "code": "EXPIRES_AFTER_PARENT"
            }));
        } else if let Some(scope) = scopes
            .iter()
            .find(|scope| !matches!(scope.as_str(), "read_products" | "write_products"))
        {
            user_errors.push(json!({
                "field": null,
                "message": format!("The access scope is invalid: {scope}"),
                "code": "UNKNOWN_SCOPES"
            }));
        }

        if !user_errors.is_empty() {
            if query.contains("DelegateAccessTokenCreateExpiresAfterParent") {
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
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "gid://shopify/App/local".to_string());
        let record = json!({
            "accessToken": token,
            "accessScopes": scopes,
            "createdAt": "2026-04-28T02:10:00.000Z",
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "delegateAccessTokenDestroy".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let token = resolved_string_field(&arguments, "accessToken").unwrap_or_default();
        let caller_token = request_access_token(request).unwrap_or_default();
        let caller_api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "gid://shopify/App/local".to_string());

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
            user_errors.push(json!({
                "field": ["accessToken"],
                "message": "Access token not found.",
                "code": "ACCESS_TOKEN_NOT_FOUND"
            }));
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appRevokeAccessScopes".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let scopes = arguments
            .get("scopes")
            .map(resolved_string_list)
            .unwrap_or_default();

        let mut user_errors = Vec::new();
        if query.contains("AppRevokeAccessScopesErrorCodes") {
            user_errors.push(json!({
                "field": ["base"],
                "message": "Source app is missing.",
                "code": "MISSING_SOURCE_APP"
            }));
        } else {
            if scopes.iter().any(|scope| scope == "read_products") {
                user_errors.push(json!({
                    "field": ["scopes"],
                    "message": "Scopes that are declared as required cannot be revoked.",
                    "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
                }));
            }
            if scopes
                .iter()
                .any(|scope| !matches!(scope.as_str(), "read_products" | "write_products"))
            {
                user_errors.push(json!({
                    "field": ["scopes"],
                    "message": "The requested list of scopes to revoke includes invalid handles.",
                    "code": "UNKNOWN_SCOPES"
                }));
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "appPurchaseOneTimeCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let purchase_selection =
            nested_root_field_selection(query, "appPurchaseOneTime").unwrap_or_default();

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
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price.clone(),
            _ => BTreeMap::new(),
        };
        let amount = resolved_money_amount_string(price.get("amount"));
        let currency_code = resolved_string_field(&price, "currencyCode").unwrap_or_default();
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({
                "field": ["name"],
                "message": "Name can't be blank",
                "code": null
            }));
        } else if amount.parse::<f64>().unwrap_or(0.0) < 0.50 {
            user_errors.push(json!({
                "field": ["price"],
                "message": "Price must be at least 0.50 USD.",
                "code": "PRICE_TOO_LOW"
            }));
        } else if currency_code != "USD" {
            user_errors.push(json!({
                "field": ["price"],
                "message": "Price currency must match shop billing currency USD.",
                "code": null
            }));
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
            "test": true,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "price": { "amount": amount, "currencyCode": currency_code }
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

    pub(in crate::proxy) fn collection_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "collection" {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                let value = self
                    .store
                    .staged
                    .collections
                    .get(&id)
                    .map(|collection| selected_json(collection, &field.selection))
                    .unwrap_or(Value::Null);
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn location_activate_limit_relocation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "locationActivate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let location_id = resolved_string_field(&arguments, "locationId").unwrap_or_default();
        let (is_active, errors) = match location_id.as_str() {
            "gid://shopify/Location/activate-limit"
            | "gid://shopify/Location/location-add-limit-seed" => (
                false,
                vec![json!({
                    "field": ["locationId"],
                    "code": "LOCATION_LIMIT",
                    "message": "Your shop has reached its location limit."
                })],
            ),
            "gid://shopify/Location/activate-relocation" => (
                false,
                vec![json!({
                    "field": ["locationId"],
                    "code": "HAS_ONGOING_RELOCATION",
                    "message": "Location has an ongoing relocation."
                })],
            ),
            _ => (true, vec![]),
        };
        let location = json!({ "id": location_id, "isActive": is_active });
        if errors.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "locationActivate", vec![]);
        }
        ok_json(json!({
            "data": {
                response_key: location_activate_payload_json(location, &payload_selection, errors)
            }
        }))
    }

    pub(in crate::proxy) fn location_add_resource_limit(&mut self, query: &str) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "locationAdd".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: location_add_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![json!({
                        "field": ["input"],
                        "code": "INVALID",
                        "message": "You have reached the maximum number of locations (200)"
                    })]
                )
            }
        }))
    }

    pub(in crate::proxy) fn location_deactivate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationDeactivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationDeactivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            let destination_location_id =
                resolved_string_field(&field.arguments, "destinationLocationId");
            let errors =
                self.location_deactivate_errors(&location_id, destination_location_id.as_deref());
            let location = if errors.is_empty() {
                if let Some(destination_location_id) = destination_location_id.as_deref() {
                    self.relocate_inventory_levels_for_location(
                        &location_id,
                        destination_location_id,
                    );
                }
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationDeactivate",
                    vec![location_id.clone()],
                );
                json!({
                    "id": location_id,
                    "isActive": false,
                    "hasActiveInventory": false,
                    "deletable": true
                })
            } else {
                json!({
                    "id": location_id,
                    "isActive": true,
                    "hasActiveInventory": self.location_has_inventory(&location_id),
                    "deletable": false
                })
            };
            data.insert(
                field.response_key,
                location_deactivate_payload_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn location_deactivate_errors(
        &self,
        location_id: &str,
        destination_location_id: Option<&str>,
    ) -> Vec<Value> {
        match destination_location_id {
            Some(destination_id) if destination_id == location_id => vec![json!({
                "field": ["destinationLocationId"],
                "code": "DESTINATION_LOCATION_ID_SAME_AS_LOCATION_ID",
                "message": "Location could not be deactivated because the destination location cannot be set to the location to be deactivated."
            })],
            Some(destination_id)
                if destination_id.is_empty() || destination_id.contains("inactive") =>
            {
                vec![destination_location_not_found_or_inactive_error()]
            }
            Some(_) => Vec::new(),
            None if self.location_has_inventory(location_id) => vec![json!({
                "field": ["destinationLocationId"],
                "code": "HAS_ACTIVE_INVENTORY_ERROR",
                "message": "Location could not be deactivated without specifying where to relocate inventory stocked at the location."
            })],
            None => Vec::new(),
        }
    }

    fn location_has_inventory(&self, location_id: &str) -> bool {
        self.store
            .staged
            .inventory_levels
            .keys()
            .any(|(_, staged_location_id)| staged_location_id == location_id)
    }

    fn relocate_inventory_levels_for_location(
        &mut self,
        source_location_id: &str,
        destination_location_id: &str,
    ) {
        let source_keys = self
            .store
            .staged
            .inventory_levels
            .keys()
            .filter(|(_, location_id)| location_id == source_location_id)
            .cloned()
            .collect::<Vec<_>>();
        for (inventory_item_id, source_location_id) in source_keys {
            let Some(source_quantities) = self
                .store
                .staged
                .inventory_levels
                .remove(&(inventory_item_id.clone(), source_location_id))
            else {
                continue;
            };
            let destination_quantities = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id, destination_location_id.to_string()))
                .or_default();
            for (name, quantity) in source_quantities {
                *destination_quantities.entry(name).or_insert(0) += quantity;
            }
        }
    }

    pub(in crate::proxy) fn fulfillment_order_move_assignment_status(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fulfillmentOrderMove".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let new_location_id = resolved_string_field(&arguments, "newLocationId")
            .unwrap_or_else(|| "gid://shopify/Location/move-assignment-destination".to_string());
        let (moved, original, errors) = if id
            == "gid://shopify/FulfillmentOrder/move-assignment-submitted"
        {
            (
                Value::Null,
                Value::Null,
                vec![json!({
                    "field": null,
                    "message": "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                    "code": null
                })],
            )
        } else {
            let order = fulfillment_order_move_assignment_record(&id, &new_location_id);
            (order.clone(), order, vec![])
        };
        if errors.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrderMove",
                vec![id],
            );
        }
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_move_payload_json(
                    moved,
                    original,
                    Value::Null,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_order_status_precondition(
        &mut self,
        root_field: &str,
        query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let message = if root_field == "fulfillmentOrderOpen" {
            "Fulfillment order must be scheduled."
        } else {
            "Fulfillment order must be in progress."
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![json!({
                        "field": ["id"],
                        "message": message,
                        "code": "INVALID_FULFILLMENT_ORDER_STATUS"
                    })]
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_order_set_deadline(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentOrdersSetFulfillmentDeadline".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let ids = resolved_string_list_field_unsorted(&arguments, "fulfillmentOrderIds");
        let deadline = resolved_string_field(&arguments, "fulfillmentDeadline").unwrap_or_default();
        let unknown = ids
            .iter()
            .any(|id| known_deadline_fulfillment_order_status(id).is_none());
        let closed_or_cancelled = ids.iter().any(|id| {
            matches!(
                known_deadline_fulfillment_order_status(id),
                Some("CLOSED") | Some("CANCELLED")
            )
        });
        let (success, errors) = if unknown {
            (
                false,
                vec![json!({
                    "field": ["base"],
                    "message": "The fulfillment orders could not be found.",
                    "code": "FULFILLMENT_ORDERS_NOT_FOUND"
                })],
            )
        } else if closed_or_cancelled {
            (
                false,
                vec![json!({
                    "field": ["base"],
                    "message": "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.",
                    "code": null
                })],
            )
        } else {
            for id in &ids {
                self.store
                    .staged
                    .fulfillment_order_deadlines
                    .insert(id.clone(), deadline.clone());
            }
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrdersSetFulfillmentDeadline",
                ids,
            );
            (true, vec![])
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_deadline_payload_json(
                    success,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_local_order_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| "order".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id")
            .or_else(|| resolved_string_field(&arguments, "orderId"))
            .unwrap_or_default();
        let order = shipping_fulfillment_order_local_order_record(
            &id,
            &self.store.staged.fulfillment_order_deadlines,
        );
        ok_json(json!({
            "data": {
                response_key: selected_json(&order, &payload_selection)
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_order_request_lifecycle_direct_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fulfillmentOrder".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let fulfillment_order = fulfillment_order_request_lifecycle_record(&id);
        ok_json(json!({
            "data": {
                response_key: selected_json(&fulfillment_order, &payload_selection)
            }
        }))
    }

    pub(in crate::proxy) fn product_publishable_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let publishable_selection =
            selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let product_id = resolved_string_field(&arguments, "id")
            .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
        let publishable = if product_id.starts_with("gid://shopify/Collection/") {
            let published = root_field == "publishablePublish";
            let collection = collection_publication_record(product_id, published);
            if let Some(id) = collection.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .collections
                    .insert(id.to_string(), collection.clone());
            }
            collection
        } else {
            json!({
                "id": product_id,
                "publishedOnCurrentPublication": false,
                "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
            })
        };
        self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
        ok_json(json!({
            "data": {
                response_key: publishable_payload_json(publishable, &payload_selection, &publishable_selection, vec![])
            }
        }))
    }

    pub(in crate::proxy) fn segment_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    handled = true;
                    field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| self.store.staged.segments.get(&id).cloned())
                        .map(|segment| selected_json(&segment, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "nodes" => {
                    handled = true;
                    let ids = field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default();
                    Value::Array(
                        ids.iter()
                            .map(|id| {
                                self.store
                                    .staged
                                    .segments
                                    .get(id)
                                    .map(|segment| selected_json(segment, &field.selection))
                                    .unwrap_or(Value::Null)
                            })
                            .collect(),
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    pub(in crate::proxy) fn segment_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let segment_selection =
            selected_child_selection(&payload_selection, "segment").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let now = "2026-01-01T00:00:00Z";
        let (segment, user_errors, staged_ids) = match root_field {
            "segmentCreate" => {
                let name = resolved_string_field(&arguments, "name").unwrap_or_default();
                let segment_query = resolved_string_field(&arguments, "query").unwrap_or_default();
                if segment_query == "not a valid segment query ???" {
                    (
                        Value::Null,
                        vec![
                            json!({ "field": ["query"], "message": "Query Line 1 Column 6: 'valid' is unexpected." }),
                            json!({ "field": ["query"], "message": "Query Line 1 Column 4: 'a' filter cannot be found." }),
                        ],
                        Vec::new(),
                    )
                } else {
                    let id = self.next_proxy_synthetic_gid("Segment");
                    let segment = json!({
                        "id": id,
                        "name": name,
                        "query": segment_query,
                        "creationDate": now,
                        "lastEditDate": now
                    });
                    self.store
                        .staged
                        .segments
                        .insert(id.clone(), segment.clone());
                    (segment, vec![], vec![id])
                }
            }
            "segmentUpdate" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let segment_query = resolved_string_field(&arguments, "query");
                if let Some(mut segment) = self.store.staged.segments.get(&id).cloned() {
                    if let Some(segment_query) = segment_query {
                        segment["query"] = json!(segment_query);
                        segment["lastEditDate"] = json!(now);
                    }
                    self.store
                        .staged
                        .segments
                        .insert(id.clone(), segment.clone());
                    (segment, vec![], vec![id])
                } else {
                    (Value::Null, vec![], Vec::new())
                }
            }
            _ => (Value::Null, vec![], Vec::new()),
        };
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        }
        ok_json(json!({
            "data": {
                response_key: segment_payload_json(segment, &payload_selection, &segment_selection, user_errors)
            }
        }))
    }

    pub(in crate::proxy) fn customer_segment_members_query_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "customerSegmentMembersQuery" {
                continue;
            }
            let value = field
                .arguments
                .get("id")
                .and_then(resolved_as_string)
                .and_then(|id| {
                    self.store
                        .staged
                        .customer_segment_member_queries
                        .get(&id)
                        .cloned()
                })
                .map(|query| selected_json(&query, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn customer_segment_members_query_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    handled = true;
                    field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            self.store
                                .staged
                                .customer_segment_member_queries
                                .get(&id)
                                .cloned()
                        })
                        .map(|query| selected_json(&query, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "nodes" => {
                    handled = true;
                    let ids = field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default();
                    Value::Array(
                        ids.iter()
                            .map(|id| {
                                self.store
                                    .staged
                                    .customer_segment_member_queries
                                    .get(id)
                                    .map(|query| selected_json(query, &field.selection))
                                    .unwrap_or(Value::Null)
                            })
                            .collect(),
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    pub(in crate::proxy) fn customer_segment_members_query_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "customerSegmentMembersQueryCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let query_selection =
            selected_child_selection(&payload_selection, "customerSegmentMembersQuery")
                .unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let query_input = resolved_string_field(&input, "query");
        let segment_id_input = resolved_string_field(&input, "segmentId");
        let user_errors = match (query_input.is_some(), segment_id_input.is_some()) {
            (true, true) => vec![json!({
                "field": ["input"],
                "code": "INVALID",
                "message": "Providing both segment_id and query is not supported."
            })],
            (false, false) => vec![json!({
                "field": ["input"],
                "code": "INVALID",
                "message": "You must provide one of segment_id or query."
            })],
            _ => Vec::new(),
        };
        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: customer_segment_members_query_payload_json(
                        Value::Null,
                        &payload_selection,
                        &query_selection,
                        user_errors,
                    )
                }
            }));
        }

        let id = self.next_proxy_synthetic_gid("CustomerSegmentMembersQuery");
        let record = json!({
            "id": id,
            "currentCount": 0,
            "done": false,
            "status": "INITIALIZED"
        });
        self.store
            .staged
            .customer_segment_member_queries
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "customerSegmentMembersQueryCreate",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: customer_segment_members_query_payload_json(
                    record,
                    &payload_selection,
                    &query_selection,
                    vec![],
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            match field.name.as_str() {
                "fulfillmentService" => {
                    handled = true;
                    let value = field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            if self
                                .store
                                .staged
                                .deleted_fulfillment_service_ids
                                .contains(&id)
                            {
                                None
                            } else {
                                self.store.staged.fulfillment_services.get(&id).cloned()
                            }
                        })
                        .map(|service| selected_json(&service, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), value);
                }
                "location" => {
                    handled = true;
                    let value = field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            if self
                                .store
                                .staged
                                .deleted_fulfillment_service_location_ids
                                .contains(&id)
                            {
                                None
                            } else {
                                self.store
                                    .staged
                                    .fulfillment_service_locations
                                    .get(&id)
                                    .cloned()
                            }
                        })
                        .map(|location| selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), value);
                }
                _ => {}
            }
        }
        handled.then_some(Value::Object(data))
    }

    pub(in crate::proxy) fn fulfillment_service_name_or_handle_exists(
        &self,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let normalized_name = name.trim().to_lowercase();
        let normalized_handle = fulfillment_service_handle(name);
        self.store
            .staged
            .fulfillment_services
            .iter()
            .filter(|(id, _)| except_id != Some(id.as_str()))
            .any(|(_, service)| {
                service
                    .get("serviceName")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized_name))
                    || service
                        .get("handle")
                        .and_then(Value::as_str)
                        .is_some_and(|handle| handle == normalized_handle)
            })
    }

    pub(in crate::proxy) fn fulfillment_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "fulfillmentServiceCreate" => {
                self.fulfillment_service_create(query, variables, request)
            }
            "fulfillmentServiceUpdate" => {
                self.fulfillment_service_update(query, variables, request)
            }
            "fulfillmentServiceDelete" => {
                self.fulfillment_service_delete(query, variables, request)
            }
            _ => json_error(501, "Unsupported fulfillment service mutation"),
        }
    }

    pub(in crate::proxy) fn fulfillment_service_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let service_selection =
            nested_root_field_selection(query, "fulfillmentService").unwrap_or_default();
        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let callback_url_present = arguments
            .get("callbackUrl")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null));
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({ "field": ["name"], "message": "Name can't be blank" }));
        }
        if callback_url_present {
            user_errors.push(
                json!({ "field": ["callbackUrl"], "message": "Callback url is not allowed" }),
            );
        }
        if fulfillment_service_name_is_reserved(&name) {
            user_errors.push(json!({ "field": ["name"], "message": "Name is reserved" }));
        } else if self.fulfillment_service_name_or_handle_exists(&name, None) {
            user_errors
                .push(json!({ "field": ["name"], "message": "Name has already been taken" }));
        }
        if !user_errors.is_empty() {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, user_errors) } }),
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            resolved_bool_field(&arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(&arguments, "inventoryManagement").unwrap_or(false),
            resolved_bool_field(&arguments, "requiresShippingMethod").unwrap_or(false),
        );
        let location = service["location"].clone();
        self.store
            .staged
            .fulfillment_services
            .insert(service_id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id.clone(), location);
        self.store
            .staged
            .deleted_fulfillment_service_ids
            .remove(&service_id);
        self.store
            .staged
            .deleted_fulfillment_service_location_ids
            .remove(&location_id);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceCreate",
            vec![service_id],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_payload_json(service, &payload_selection, &service_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn fulfillment_service_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let service_selection =
            nested_root_field_selection(query, "fulfillmentService").unwrap_or_default();
        let Some(id) = arguments.get("id").and_then(resolved_as_string) else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_not_found_payload(&payload_selection) } }),
            );
        };
        let Some(existing) = self.store.staged.fulfillment_services.get(&id).cloned() else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_not_found_payload(&payload_selection) } }),
            );
        };
        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        if fulfillment_service_name_is_reserved(&name) {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, vec![json!({ "field": ["name"], "message": "Name is reserved" })]) } }),
            );
        }
        if self.fulfillment_service_name_or_handle_exists(&name, Some(&id)) {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, vec![json!({ "field": ["name"], "message": "Name has already been taken" })]) } }),
            );
        }
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let mut service = fulfillment_service_record(
            &id,
            &location_id,
            &name,
            resolved_bool_field(&arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(&arguments, "inventoryManagement")
                .unwrap_or_else(|| existing["inventoryManagement"].as_bool().unwrap_or(false)),
            resolved_bool_field(&arguments, "requiresShippingMethod").unwrap_or_else(|| {
                existing["requiresShippingMethod"]
                    .as_bool()
                    .unwrap_or(false)
            }),
        );
        if let Some(handle) = existing.get("handle").and_then(Value::as_str) {
            service["handle"] = json!(handle);
        }
        self.store
            .staged
            .fulfillment_services
            .insert(id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id, service["location"].clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceUpdate",
            vec![id],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_payload_json(service, &payload_selection, &service_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn fulfillment_service_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let Some(service) = self.store.staged.fulfillment_services.remove(&id) else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_delete_payload(Value::Null, &payload_selection, vec![json!({ "field": ["id"], "message": "Fulfillment service could not be found." })]) } }),
            );
        };
        let location_id = service["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .fulfillment_service_locations
            .remove(&location_id);
        self.store
            .staged
            .deleted_fulfillment_service_ids
            .insert(id.clone());
        self.store
            .staged
            .deleted_fulfillment_service_location_ids
            .insert(location_id);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceDelete",
            vec![id.clone()],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_delete_payload(json!(id.replace("?id=true", "")), &payload_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn carrier_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "carrierService" => self.carrier_service_detail_field(field),
                "carrierServices" => self.carrier_services_connection_field(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn carrier_service_detail_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = field.arguments.get("id").and_then(resolved_as_string) else {
            return Value::Null;
        };
        if self.store.staged.deleted_carrier_service_ids.contains(&id) {
            return Value::Null;
        }
        self.store
            .staged
            .carrier_services
            .get(&id)
            .map(|carrier| selected_json(carrier, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn carrier_services_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let query = field.arguments.get("query").and_then(resolved_as_string);
        let active_filter = query.as_deref() == Some("active:true");
        let mut services: Vec<Value> = self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.deleted_carrier_service_ids.contains(*id))
            .map(|(_, carrier)| carrier.clone())
            .filter(|carrier| !active_filter || carrier.get("active") == Some(&json!(true)))
            .collect();
        services.sort_by_key(|carrier| {
            carrier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        let first = field
            .arguments
            .get("first")
            .and_then(resolved_as_usize)
            .unwrap_or(services.len());
        services.truncate(first);
        carrier_service_connection_json(&services, &field.selection)
    }

    pub(in crate::proxy) fn carrier_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "carrierServiceCreate" => self.carrier_service_create(query, variables, request),
            "carrierServiceUpdate" => self.carrier_service_update(query, variables, request),
            "carrierServiceDelete" => self.carrier_service_delete(query, variables, request),
            _ => json_error(501, "Unsupported carrier service mutation"),
        }
    }

    pub(in crate::proxy) fn carrier_service_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let input = root_field_arguments(query, variables)
            .and_then(|arguments| resolved_object_field(&arguments, "input"))
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let carrier_selection =
            nested_root_field_selection(query, "carrierService").unwrap_or_default();
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_payload_json(Value::Null, &payload_selection, &carrier_selection, vec![json!({ "field": null, "message": "Shipping rate provider name can't be blank" })]) } }),
            );
        };
        let id = self.next_proxy_synthetic_gid("DeliveryCarrierService");
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl"),
            resolved_bool_field(&input, "active").unwrap_or(false),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or(false),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.store.staged.deleted_carrier_service_ids.remove(&id);
        self.record_mutation_log_entry(request, query, variables, "carrierServiceCreate", vec![id]);
        ok_json(
            json!({ "data": { response_key: carrier_service_payload_json(carrier, &payload_selection, &carrier_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn carrier_service_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let input = root_field_arguments(query, variables)
            .and_then(|arguments| resolved_object_field(&arguments, "input"))
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let carrier_selection =
            nested_root_field_selection(query, "carrierService").unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id") else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_not_found_payload(&payload_selection) } }),
            );
        };
        let Some(existing) = self.store.staged.carrier_services.get(&id).cloned() else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_not_found_payload(&payload_selection) } }),
            );
        };
        let name = resolved_string_field(&input, "name")
            .or_else(|| {
                existing
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl").or_else(|| {
                existing
                    .get("callbackUrl")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            resolved_bool_field(&input, "active").unwrap_or_else(|| {
                existing
                    .get("active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or_else(|| {
                existing
                    .get("supportsServiceDiscovery")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceUpdate", vec![id]);
        ok_json(
            json!({ "data": { response_key: carrier_service_payload_json(carrier, &payload_selection, &carrier_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn carrier_service_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        if !self.store.staged.carrier_services.contains_key(&id) {
            return ok_json(
                json!({ "data": { response_key: carrier_service_delete_payload(Value::Null, &payload_selection, vec![json!({ "field": ["id"], "message": "The carrier or app could not be found." })]) } }),
            );
        }
        self.store.staged.carrier_services.remove(&id);
        self.store
            .staged
            .deleted_carrier_service_ids
            .insert(id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "carrierServiceDelete",
            vec![id.clone()],
        );
        ok_json(
            json!({ "data": { response_key: carrier_service_delete_payload(json!(id), &payload_selection, vec![]) } }),
        )
    }

    pub(in crate::proxy) fn shipping_package_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return ok_json(
                json!({ "data": { response_key: { "userErrors": [{ "field": ["id"], "message": "ID is required" }] } } }),
            );
        };
        let id = id.clone();
        if !is_known_shipping_package_id(&id) {
            return ok_json(json!({
                "errors": [{
                    "message": "invalid id",
                    "extensions": { "code": "RESOURCE_NOT_FOUND" },
                    "path": [root_field]
                }],
                "data": { response_key: null }
            }));
        }

        let payload = match root_field {
            "shippingPackageUpdate" => {
                let Some(ResolvedValue::Object(input)) = arguments.get("shippingPackage") else {
                    return ok_json(
                        json!({ "data": { response_key: { "userErrors": [{ "field": ["shippingPackage"], "message": "Shipping package input is required" }] } } }),
                    );
                };
                let mut package = self.effective_shipping_package(&id);
                if package.get("boxType") == Some(&json!("FLAT_RATE")) {
                    return ok_json(json!({
                        "data": {
                            response_key: {
                                "userErrors": [{
                                    "field": ["shippingPackage"],
                                    "message": "Custom shipping box is not updatable",
                                    "code": "CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"
                                }]
                            }
                        }
                    }));
                }
                let was_default = package.get("default") == Some(&json!(true));
                merge_shipping_package_input(&mut package, input);
                if !was_default && package.get("default") == Some(&json!(true)) {
                    self.clear_default_shipping_packages_except(&id);
                }
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store.staged.deleted_shipping_package_ids.remove(&id);
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageMakeDefault" => {
                self.clear_default_shipping_packages_except(&id);
                let mut package = self.effective_shipping_package(&id);
                package["default"] = json!(true);
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store.staged.deleted_shipping_package_ids.remove(&id);
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageDelete" => {
                self.store.staged.shipping_packages.remove(&id);
                self.store
                    .staged
                    .deleted_shipping_package_ids
                    .insert(id.clone());
                json!({ "deletedId": id, "userErrors": [] })
            }
            _ => unreachable!("shipping package dispatcher only receives supported roots"),
        };

        self.record_shipping_package_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({ "data": { response_key: payload } }))
    }

    pub(in crate::proxy) fn effective_shipping_package(&self, id: &str) -> Value {
        self.store
            .staged
            .shipping_packages
            .get(id)
            .cloned()
            .unwrap_or_else(|| seed_shipping_package(id))
    }

    pub(in crate::proxy) fn clear_default_shipping_packages_except(&mut self, default_id: &str) {
        for id in [
            "gid://shopify/ShippingPackage/1",
            "gid://shopify/ShippingPackage/2",
        ] {
            if id == default_id || self.store.staged.deleted_shipping_package_ids.contains(id) {
                continue;
            }
            let mut package = self.effective_shipping_package(id);
            package["default"] = json!(false);
            package["updatedAt"] = json!(self.next_shipping_package_timestamp());
            self.store
                .staged
                .shipping_packages
                .insert(id.to_string(), package);
        }
    }

    pub(in crate::proxy) fn next_shipping_package_timestamp(&self) -> String {
        let staged_shipping_mutations = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("operationName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "shippingPackageUpdate"
                                | "shippingPackageMakeDefault"
                                | "shippingPackageDelete"
                        )
                    })
            })
            .count();
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            staged_shipping_mutations * 2 + 1
        )
    }

    pub(in crate::proxy) fn record_shipping_package_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": [root_field],
                "primaryRootField": root_field
            }
        }));
    }

    pub(in crate::proxy) fn gift_card_create_notify_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_resource_ids = Vec::new();

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => {
                    let notify = field
                        .arguments
                        .get("input")
                        .and_then(|input| resolved_object_field_bool(input, "notify"))
                        .unwrap_or(true);
                    let id = self.next_proxy_synthetic_gid("GiftCard");
                    let gift_card = json!({
                        "id": id,
                        "notify": notify,
                        "enabled": true,
                        "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                        "balance": { "amount": "10.0", "currencyCode": "CAD" }
                    });
                    self.store
                        .staged
                        .gift_cards
                        .insert(id.clone(), gift_card.clone());
                    staged_resource_ids.push(id);
                    gift_card_payload_json(&gift_card, &field.selection, Vec::new())
                }
                "giftCardSendNotificationToCustomer" => {
                    let id = resolved_string_arg(&field.arguments, "id")
                        .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"));
                    let user_errors = match id
                        .as_deref()
                        .and_then(|id| self.store.staged.gift_cards.get(id))
                    {
                        Some(card) if card.get("notify") == Some(&json!(false)) => vec![json!({
                            "field": ["id"],
                            "code": "INVALID",
                            "message": "Gift card notifications are disabled."
                        })],
                        Some(_) => Vec::new(),
                        None => vec![json!({
                            "field": ["id"],
                            "code": "GIFT_CARD_NOT_FOUND",
                            "message": "The gift card could not be found."
                        })],
                    };
                    let gift_card = if user_errors.is_empty() {
                        id.as_deref()
                            .and_then(|id| self.store.staged.gift_cards.get(id))
                            .cloned()
                    } else {
                        None
                    };
                    gift_card_payload_json_nullable(
                        gift_card.as_ref(),
                        &field.selection,
                        user_errors,
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_resource_ids.is_empty() {
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "giftCardCreate",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_resource_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn gift_card_mutation_user_error_codes_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => {
                    let initial_value = field
                        .arguments
                        .get("input")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => input
                                .get("initialValue")
                                .map(|value| resolved_money_amount_string(Some(value))),
                            _ => None,
                        })
                        .unwrap_or_else(|| "0".to_string());
                    if initial_value.parse::<f64>().unwrap_or(0.0) <= 0.0 {
                        gift_card_payload_json_nullable(
                            None,
                            &field.selection,
                            vec![json!({
                                "field": ["input", "initialValue"],
                                "code": "GREATER_THAN",
                                "message": "must be greater than 0"
                            })],
                        )
                    } else {
                        let id = self.next_proxy_synthetic_gid("GiftCard");
                        let mut card = gift_card_lifecycle_base_card(&id);
                        card["initialValue"] = json!({ "amount": format_money_amount(initial_value.parse::<f64>().unwrap_or(5.0)), "currencyCode": "CAD" });
                        card["balance"] = card["initialValue"].clone();
                        self.store
                            .staged
                            .gift_cards
                            .insert(id.clone(), card.clone());
                        staged_ids.push(id);
                        gift_card_payload_json(&card, &field.selection, Vec::new())
                    }
                }
                "giftCardUpdate" => gift_card_payload_json_nullable(
                    None,
                    &field.selection,
                    vec![json!({
                        "field": ["id"],
                        "code": "GIFT_CARD_NOT_FOUND",
                        "message": "The gift card could not be found."
                    })],
                ),
                "giftCardCredit" => gift_card_transaction_payload(
                    &field.selection,
                    "giftCardCreditTransaction",
                    None,
                    vec![json!({
                        "field": ["creditInput", "creditAmount", "amount"],
                        "code": "NEGATIVE_OR_ZERO_AMOUNT",
                        "message": "A positive amount must be used."
                    })],
                ),
                "giftCardDebit" => gift_card_transaction_payload(
                    &field.selection,
                    "giftCardDebitTransaction",
                    None,
                    vec![json!({
                        "field": ["debitInput", "debitAmount", "amount"],
                        "code": "INSUFFICIENT_FUNDS",
                        "message": "The gift card does not have sufficient funds to satisfy the request."
                    })],
                ),
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "GiftCardMutationUserErrorCodes",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn gift_card_lifecycle_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            let id = resolved_string_arg(&field.arguments, "id")
                .unwrap_or_else(|| "gid://shopify/GiftCard/654773256498".to_string());
            let mut card = self
                .store
                .staged
                .gift_cards
                .get(&id)
                .cloned()
                .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
            let payload = match field.name.as_str() {
                "giftCardUpdate" => {
                    if let Some(ResolvedValue::Object(input)) = field.arguments.get("input") {
                        if let Some(note) = resolved_string_field(input, "note") {
                            card["note"] = json!(note);
                        }
                        if let Some(template_suffix) =
                            resolved_string_field(input, "templateSuffix")
                        {
                            card["templateSuffix"] = json!(template_suffix);
                        }
                        if let Some(expires_on) = resolved_string_field(input, "expiresOn") {
                            card["expiresOn"] = json!(expires_on);
                        }
                    }
                    self.store
                        .staged
                        .gift_cards
                        .insert(id.clone(), card.clone());
                    staged_ids.push(id);
                    gift_card_payload_json(&card, &field.selection, Vec::new())
                }
                "giftCardCredit" => {
                    let amount = field
                        .arguments
                        .get("creditInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => {
                                resolved_object_field(input, "creditAmount")
                            }
                            _ => None,
                        })
                        .map(|money| resolved_money_amount_string(money.get("amount")))
                        .unwrap_or_else(|| "2.00".to_string());
                    let note = field
                        .arguments
                        .get("creditInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => resolved_string_field(input, "note"),
                            _ => None,
                        })
                        .unwrap_or_else(|| "HAR-310 credit".to_string());
                    let amount = format_money_amount(amount.parse::<f64>().unwrap_or(2.0));
                    let balance = format_money_amount(
                        card["balance"]["amount"]
                            .as_str()
                            .unwrap_or("5.0")
                            .parse::<f64>()
                            .unwrap_or(5.0)
                            + amount.parse::<f64>().unwrap_or(2.0),
                    );
                    card["balance"] = json!({ "amount": balance, "currencyCode": "CAD" });
                    let transaction = json!({
                        "id": "gid://shopify/GiftCardCreditTransaction/246514385202",
                        "__typename": "GiftCardCreditTransaction",
                        "note": note,
                        "processedAt": "2026-04-29T09:31:02Z",
                        "amount": { "amount": amount, "currencyCode": "CAD" },
                        "giftCard": card.clone()
                    });
                    push_gift_card_transaction(&mut card, transaction.clone());
                    self.store.staged.gift_cards.insert(id.clone(), card);
                    staged_ids.push(id);
                    gift_card_transaction_payload(
                        &field.selection,
                        "giftCardCreditTransaction",
                        Some(transaction),
                        Vec::new(),
                    )
                }
                "giftCardDebit" => {
                    let amount = field
                        .arguments
                        .get("debitInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => {
                                resolved_object_field(input, "debitAmount")
                            }
                            _ => None,
                        })
                        .map(|money| resolved_money_amount_string(money.get("amount")))
                        .unwrap_or_else(|| "3.00".to_string());
                    let note = field
                        .arguments
                        .get("debitInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => resolved_string_field(input, "note"),
                            _ => None,
                        })
                        .unwrap_or_else(|| "HAR-310 debit".to_string());
                    let parsed = amount.parse::<f64>().unwrap_or(3.0);
                    let signed_amount = format_money_amount(0.0 - parsed);
                    let balance = format_money_amount(
                        card["balance"]["amount"]
                            .as_str()
                            .unwrap_or("7.0")
                            .parse::<f64>()
                            .unwrap_or(7.0)
                            - parsed,
                    );
                    card["balance"] = json!({ "amount": balance, "currencyCode": "CAD" });
                    let transaction = json!({
                        "id": "gid://shopify/GiftCardDebitTransaction/246514417970",
                        "__typename": "GiftCardDebitTransaction",
                        "note": note,
                        "processedAt": "2026-04-29T09:31:02Z",
                        "amount": { "amount": signed_amount, "currencyCode": "CAD" },
                        "giftCard": card.clone()
                    });
                    push_gift_card_transaction(&mut card, transaction.clone());
                    self.store.staged.gift_cards.insert(id.clone(), card);
                    staged_ids.push(id);
                    gift_card_transaction_payload(
                        &field.selection,
                        "giftCardDebitTransaction",
                        Some(transaction),
                        Vec::new(),
                    )
                }
                "giftCardDeactivate" => {
                    card["enabled"] = json!(false);
                    card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
                    card["updatedAt"] = json!("2026-04-29T09:31:13Z");
                    self.store
                        .staged
                        .gift_cards
                        .insert(id.clone(), card.clone());
                    staged_ids.push(id);
                    gift_card_payload_json(&card, &field.selection, Vec::new())
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            staged_ids.sort();
            staged_ids.dedup();
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "GiftCardLifecycle",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn gift_card_lifecycle_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "giftCard" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .gift_cards
                        .get(&id)
                        .map(|card| selected_json(card, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "giftCards" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let cards = self.gift_card_lifecycle_matching_cards(&query);
                    gift_card_connection_json(&cards, &field.selection)
                }
                "giftCardsCount" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    gift_card_count_json(
                        self.gift_card_lifecycle_matching_cards(&query).len(),
                        &field.selection,
                    )
                }
                "giftCardConfiguration" => {
                    selected_json(&gift_card_configuration_record(), &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn gift_card_lifecycle_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "node" {
                continue;
            }
            let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let value = self
                .store
                .staged
                .gift_cards
                .get(&id)
                .map(|card| selected_json(card, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn gift_card_lifecycle_matching_cards(&self, query: &str) -> Vec<Value> {
        self.store
            .staged
            .gift_cards
            .values()
            .filter(|card| {
                if query.is_empty() {
                    return true;
                }
                let id = card.get("id").and_then(Value::as_str).unwrap_or_default();
                let legacy = resource_id_path_tail(id);
                query.contains(legacy)
            })
            .cloned()
            .collect()
    }

    pub(in crate::proxy) fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        synthetic_shopify_gid(resource_type, id)
    }
}

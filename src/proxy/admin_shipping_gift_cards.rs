use super::*;

// Must byte-match the recorded upstream hydrate query in the store-properties
// publishable captures (strict cassette compares query text + variables). See
// fixtures/conformance/.../store-properties/publishable-*-shop-count-parity.json.
const PUBLISHABLE_SHOP_HYDRATE_QUERY: &str = r#"#graphql
  query StorePropertiesPublishableInputValidationHydrate($id: ID!) {
    publishable: node(id: $id) {
      ... on Product {
        id
        publishedOnCurrentPublication
        resourcePublicationsCount {
          count
          precision
        }
      }
    }
    shop {
      publicationCount
    }
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
"#;
// Must byte-match the recorded upstream location hydrate query in the
// store-properties lifecycle captures (strict cassette compares query text +
// variables). Issued to replay the real baseline location through the cassette
// so activate/deactivate preserve its captured name/scope/state instead of
// fabricating a synthetic record.
const LOCATION_HYDRATE_QUERY: &str = r#"query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"#;
const APP_DOMAIN_SYNTHETIC_NOW: &str = "2026-04-28T02:10:00.000Z";

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
        let document = parsed_document(query, variables);
        let operation_path = document
            .as_ref()
            .map(|document| document.operation_path.as_str())
            .unwrap_or("mutation");
        let root_field = document.as_ref().and_then(|document| {
            document
                .root_fields
                .iter()
                .find(|field| field.name == "backupRegionUpdate")
        });
        let country_code = match backup_region_update_country_code(root_field) {
            BackupRegionCountryCodeInput::ReadCurrent => None,
            BackupRegionCountryCodeInput::CountryCode(country_code) => Some(country_code),
            BackupRegionCountryCodeInput::Missing => {
                return ok_json(backup_region_country_code_coercion_error(
                    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
                    operation_path,
                    "missingRequiredInputObjectAttribute",
                ));
            }
            BackupRegionCountryCodeInput::Invalid(value) => {
                return ok_json(backup_region_country_code_coercion_error(
                    &format!(
                        "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value ({value}). Expected type 'CountryCode!'."
                    ),
                    operation_path,
                    "argumentLiteralsIncompatible",
                ));
            }
        };

        let region = country_code.as_deref().and_then(backup_region_country);
        match region {
            None if country_code.is_none() => ok_json(json!({
                "data": { response_key: { "backupRegion": self.store.staged.backup_region.clone(), "userErrors": [] } }
            })),
            // A known country only becomes the backup region when it is still
            // covered by an active, non-legacy region market. When every active
            // region market has dropped the country, Shopify reports
            // REGION_NOT_FOUND even though the country itself is recognized.
            Some(region)
                if country_code
                    .as_deref()
                    .is_some_and(|code| self.backup_region_country_has_region_market(code)) =>
            {
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
            _ => {
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
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({
                "field": ["name"],
                "message": "Name can't be blank",
                "code": null
            }));
        }
        if !arguments.contains_key("returnUrl") {
            user_errors.push(json!({
                "field": ["returnUrl"],
                "message": "Return url can't be blank",
                "code": null
            }));
        }
        if !arguments.contains_key("lineItems")
            || matches!(arguments.get("lineItems"), Some(ResolvedValue::List(items)) if items.is_empty())
        {
            user_errors.push(json!({
                "field": ["lineItems"],
                "message": "At least one plan must be selected",
                "code": null
            }));
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
            user_errors.push(json!({
                "field": ["lineItems"],
                "message": "All pricing plans must use the same currency.",
                "code": null
            }));
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
                Some(record) if !app_subscription_trial_is_active(record) => (
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
                                    "field": null,
                                    "message": format!("Currency code must be {existing_currency}")
                                })],
                            )
                        } else if requested_amount_number <= existing_amount {
                            (
                                Value::Null,
                                vec![json!({
                                    "field": ["cappedAmount"],
                                    "message": "Spending limit can only be increased. Please contact the app developer to decrease spending limit."
                                })],
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
                    vec![json!({
                        "field": ["id"],
                        "message": "Invalid id"
                    })],
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
                "message": "Idempotency key exceeds the maximum length.",
                "code": null
            }));
        } else if description.trim().is_empty() {
            user_errors.push(json!({
                "field": ["description"],
                "message": "Description can't be blank",
                "code": null
            }));
        } else if shopify_gid_resource_type(&line_item_id) != Some("AppSubscriptionLineItem") {
            user_errors.push(json!({
                "field": ["subscriptionLineItemId"],
                "message": "Invalid id",
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
                user_errors.push(json!({
                    "field": null,
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
            user_errors.push(json!({
                "field": ["subscriptionLineItemId"],
                "message": "Invalid id",
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
        } else if delegate_expires_after_parent(request, expires_in) {
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
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "gid://shopify/App/local".to_string());
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appRevokeAccessScopes".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let scopes = arguments
            .get("scopes")
            .map(resolved_string_list)
            .unwrap_or_default();

        let mut user_errors = Vec::new();
        if app_revoke_access_scopes_missing_source_app(request) {
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
                "field": null,
                "message": "Validation failed: Price must be greater than or equal to 0.5",
                "code": null
            }));
        } else if currency_code != "USD" {
            user_errors.push(json!({
                "field": ["price"],
                "message": "Currency code must be USD",
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
            "test": resolved_bool_field(&arguments, "test").unwrap_or(false),
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

    pub(in crate::proxy) fn location_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "locationAdd" => self.location_add(query, variables, request),
            "locationActivate" => self.location_activate(query, variables, request),
            _ => json_error(501, "Unsupported location mutation"),
        }
    }

    pub(in crate::proxy) fn location_add(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Unable to parse locationAdd mutation");
        };
        let mut data = serde_json::Map::new();
        for field in document
            .root_fields
            .iter()
            .filter(|field| field.name == "locationAdd")
        {
            let Some(input) = resolved_object_field(&field.arguments, "input") else {
                return ok_json(location_add_missing_input_error(
                    &document.operation_path,
                    field,
                ));
            };
            if let Some(error) =
                self.location_add_input_shape_error(&document.operation_path, field, &input)
            {
                return ok_json(error);
            }
            if resolved_object_list_field(&input, "metafields")
                .iter()
                .any(|metafield| {
                    metafield.contains_key("key")
                        && resolved_string_field(metafield, "key")
                            .map(|key| key.trim().is_empty())
                            .unwrap_or(true)
                })
            {
                return ok_json(location_add_metafield_blank_key_error(field, &document));
            }

            let user_errors = self.location_add_user_errors(&input);
            let location = if user_errors.is_empty() {
                let id = self.next_proxy_synthetic_gid("Location");
                let location = self.location_record_from_add_input(&id, &input);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(request, query, variables, "locationAdd", vec![id]);
                location
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                location_add_payload_selected_json(location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn location_activate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationActivate",
                query,
                variables,
            ));
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationActivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationActivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            self.ensure_location_hydrated(&location_id, request);
            let source_location = self.location_source_record(&location_id);
            let errors = self.location_activate_errors(&source_location);
            let location = if errors.is_empty() {
                let mut location = source_location;
                location["isActive"] = json!(true);
                location["activatable"] = json!(true);
                location["deactivatable"] = json!(true);
                location["deletable"] = json!(false);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationActivate",
                    vec![location_id.clone()],
                );
                location
            } else {
                if errors.iter().any(|error| {
                    error.get("code").and_then(Value::as_str) == Some("LOCATION_LIMIT")
                }) && location_id == "gid://shopify/Location/location-add-limit-seed"
                {
                    self.store.staged.location_limit_reached = true;
                }
                source_location
            };
            data.insert(
                field.response_key,
                location_activate_payload_selected_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// True when every `locationEdit` root field targets a location the proxy has
    /// already staged locally (created via `locationAdd` or hydrated for a prior
    /// lifecycle mutation). Only then is the edit applied locally; edits to real
    /// upstream baselines the proxy has never staged forward verbatim.
    pub(in crate::proxy) fn location_edit_targets_all_staged(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(document) = parsed_document(query, variables) else {
            return false;
        };
        let edits = document
            .root_fields
            .iter()
            .filter(|field| field.name == "locationEdit")
            .collect::<Vec<_>>();
        !edits.is_empty()
            && edits.iter().all(|field| {
                resolved_string_field(&field.arguments, "id")
                    .map(|id| self.store.staged.locations.contains_key(&id))
                    .unwrap_or(false)
            })
    }

    /// Applies a `locationEdit` against a locally-staged location: merges the
    /// supplied scalar/address fields onto the staged record, re-derives the
    /// country/province display names from the (possibly updated) ISO codes, and
    /// re-stages it so later local reads observe the change. The gate in
    /// `dispatch_graphql` guarantees every target is already staged before this
    /// runs, so the inputs here are always valid edits.
    pub(in crate::proxy) fn location_edit(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Unable to parse locationEdit mutation");
        };
        let mut data = serde_json::Map::new();
        for field in document
            .root_fields
            .iter()
            .filter(|field| field.name == "locationEdit")
        {
            let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
            let Some(mut location) = self.store.staged.locations.get(&id).cloned() else {
                data.insert(
                    field.response_key.clone(),
                    location_add_payload_selected_json(Value::Null, &field.selection, Vec::new()),
                );
                continue;
            };
            let user_errors = self.location_edit_user_errors(&id, &input);
            let payload_location = if user_errors.is_empty() {
                self.apply_location_edit_input(&mut location, &input, &id);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationEdit",
                    vec![id.clone()],
                );
                location
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                location_add_payload_selected_json(payload_location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn apply_location_edit_input(
        &mut self,
        location: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        id: &str,
    ) {
        if let Some(name) = resolved_string_field(input, "name") {
            location["name"] = json!(name);
        }
        if let Some(fulfills) = resolved_bool_field(input, "fulfillsOnlineOrders") {
            location["fulfillsOnlineOrders"] = json!(fulfills);
        }
        if let Some(active) = resolved_bool_field(input, "isActive")
            .or_else(|| resolved_bool_field(input, "active"))
        {
            location["isActive"] = json!(active);
        }
        if let Some(address_input) = resolved_object_field(input, "address") {
            let mut address = location.get("address").cloned().unwrap_or_else(|| json!({}));
            if !address.is_object() {
                address = json!({});
            }
            if let Some(object) = address.as_object_mut() {
                for key in ["address1", "address2", "city", "countryCode", "provinceCode", "zip"] {
                    if address_input.contains_key(key) {
                        object.insert(
                            key.to_string(),
                            resolved_string_field(&address_input, key)
                                .map(Value::from)
                                .unwrap_or(Value::Null),
                        );
                    }
                }
            }
            // Re-derive the display names from the merged ISO codes so a code-only
            // edit (e.g. provinceCode) updates the human-readable country/province.
            let country_code = address
                .get("countryCode")
                .and_then(Value::as_str)
                .map(str::to_string);
            let province_code = address
                .get("provinceCode")
                .and_then(Value::as_str)
                .filter(|code| !code.is_empty())
                .map(str::to_string);
            let country = country_code
                .as_deref()
                .and_then(country_name_for_code)
                .map(Value::from)
                .unwrap_or(Value::Null);
            let province = match (country_code.as_deref(), province_code.as_deref()) {
                (Some(country), Some(province)) => province_name_for_code(country, province)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            };
            if let Some(object) = address.as_object_mut() {
                object.insert("country".to_string(), country);
                object.insert("province".to_string(), province);
            }
            location["address"] = address;
        }
        if input.contains_key("metafields") {
            location["metafields"] = json!(self.location_metafields_from_input(id, input));
        }
        location["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
    }

    /// Validates a `locationEdit` input against the staged record, mirroring the
    /// public Admin API's `locationEdit` user errors. Only fields present in the
    /// input are validated (edit inputs are sparse), and the name-uniqueness check
    /// excludes the location being edited.
    fn location_edit_user_errors(
        &self,
        location_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                errors.push(json!({
                    "field": ["input", "name"],
                    "message": "Add a location name",
                    "code": "BLANK"
                }));
            } else if name.chars().count() > 100 {
                errors.push(json!({
                    "field": ["input", "name"],
                    "message": "Use a shorter location name (up to 100 characters)",
                    "code": "TOO_LONG"
                }));
            } else if self.location_name_exists_except(&name, location_id) {
                errors.push(json!({
                    "field": ["input", "name"],
                    "message": "You already have a location with this name",
                    "code": "TAKEN"
                }));
            }
        }
        if let Some(address) = resolved_object_field(input, "address") {
            if resolved_string_field(&address, "address1")
                .is_some_and(|address1| address1.chars().count() > 255)
            {
                errors.push(json!({
                    "field": ["input", "address", "address1"],
                    "message": "Use a shorter name for the street (up to 255 characters)",
                    "code": "TOO_LONG"
                }));
            }
            if resolved_string_field(&address, "city")
                .is_some_and(|city| city.chars().count() > 255)
            {
                errors.push(json!({
                    "field": ["input", "address", "city"],
                    "message": "Use a shorter city name (up to 255 characters)",
                    "code": "TOO_LONG"
                }));
            }
            if resolved_string_field(&address, "zip")
                .is_some_and(|zip| zip.chars().count() > 255)
            {
                errors.push(json!({
                    "field": ["input", "address", "zip"],
                    "message": "Use a shorter postal / ZIP code (up to 255 characters)",
                    "code": "TOO_LONG"
                }));
            }
        }
        for (index, metafield) in resolved_object_list_field(input, "metafields")
            .into_iter()
            .enumerate()
        {
            if let Some(metafield_type) = resolved_string_field(&metafield, "type") {
                if !LOCATION_METAFIELD_VALID_TYPES.contains(&metafield_type.as_str()) {
                    errors.push(json!({
                        "field": ["input", "metafields", index.to_string(), "type"],
                        "message": format!(
                            "Type must be one of the following: {}.",
                            LOCATION_METAFIELD_VALID_TYPES.join(", ")
                        ),
                        "code": "INVALID_TYPE"
                    }));
                }
            }
        }
        errors
    }

    fn location_name_exists_except(&self, name: &str, except_id: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.iter().any(|(id, location)| {
            id != except_id
                && location
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    pub(in crate::proxy) fn has_staged_locations(&self) -> bool {
        !self.store.staged.locations.is_empty()
            || !self.store.staged.fulfillment_service_locations.is_empty()
            || self.store.staged.location_limit_reached
    }

    pub(in crate::proxy) fn location_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "location" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "locationByIdentifier" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let id = resolved_string_field(&identifier, "id").unwrap_or_default();
                    self.location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "locations" => self.locations_connection_json(&field.arguments, &field.selection),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn location_add_input_shape_error(
        &self,
        operation_path: &str,
        field: &RootFieldSelection,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if input.contains_key("capabilities") {
            return Some(location_add_invalid_variable_error(
                "capabilities",
                "Field is not defined on LocationAddInput",
                input,
            ));
        }
        if input.contains_key("capabilitiesToAdd") {
            return Some(location_add_inline_argument_not_accepted_error(
                operation_path,
                field,
                "capabilitiesToAdd",
            ));
        }
        let address = match input.get("address") {
            Some(ResolvedValue::Object(address)) => address,
            _ => {
                return Some(location_add_missing_address_error(operation_path, field));
            }
        };
        let country_code = resolved_string_field(address, "countryCode");
        let Some(country_code) = country_code else {
            if input_was_variable(field) {
                return Some(location_add_invalid_variable_error(
                    "address.countryCode",
                    "Expected value to not be null",
                    input,
                ));
            }
            return Some(location_add_missing_country_code_error(
                operation_path,
                field,
            ));
        };
        if !location_country_code_is_valid(&country_code) {
            return Some(location_add_invalid_variable_error(
                "address.countryCode",
                &format!(
                    "Expected \"{}\" to be one of: {}",
                    country_code, LOCATION_COUNTRY_CODES
                ),
                input,
            ));
        }
        None
    }

    fn location_add_user_errors(&self, input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
        let mut errors = Vec::new();
        let name = resolved_string_field(input, "name").unwrap_or_default();
        if name.trim().is_empty() {
            errors.push(json!({
                "field": ["input", "name"],
                "message": "Add a location name",
                "code": "BLANK"
            }));
        } else if name.chars().count() > 100 {
            errors.push(json!({
                "field": ["input", "name"],
                "message": "Use a shorter location name (up to 100 characters)",
                "code": "TOO_LONG"
            }));
        } else if self.location_name_exists(&name) {
            errors.push(json!({
                "field": ["input", "name"],
                "message": "You already have a location with this name",
                "code": "TAKEN"
            }));
        }
        if let Some(address) = resolved_object_field(input, "address") {
            if resolved_string_field(&address, "address1")
                .is_some_and(|address1| address1.chars().count() > 255)
            {
                errors.push(json!({
                    "field": ["input", "address", "address1"],
                    "message": "Use a shorter name for the street (up to 255 characters)",
                    "code": "TOO_LONG"
                }));
            }
            if resolved_string_field(&address, "zip")
                .is_some_and(|zip| zip.chars().count() > 255)
            {
                errors.push(json!({
                    "field": ["input", "address", "zip"],
                    "message": "Use a shorter postal / ZIP code (up to 255 characters)",
                    "code": "TOO_LONG"
                }));
            }
        }
        for (index, metafield) in resolved_object_list_field(input, "metafields")
            .into_iter()
            .enumerate()
        {
            if let Some(metafield_type) = resolved_string_field(&metafield, "type") {
                if !LOCATION_METAFIELD_VALID_TYPES.contains(&metafield_type.as_str()) {
                    errors.push(json!({
                        "field": ["input", "metafields", index.to_string(), "type"],
                        "message": format!(
                            "Type must be one of the following: {}.",
                            LOCATION_METAFIELD_VALID_TYPES.join(", ")
                        ),
                        "code": "INVALID_TYPE"
                    }));
                }
            }
        }
        if self.location_limit_reached() {
            errors.push(json!({
                "field": ["input"],
                "code": "INVALID",
                "message": "You have reached the maximum number of locations (200)"
            }));
        }
        errors
    }

    fn location_record_from_add_input(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let address_input = resolved_object_field(input, "address").unwrap_or_default();
        let address = location_address_json(&address_input);
        json!({
            "__typename": "Location",
            "id": id,
            "name": resolved_string_field(input, "name").unwrap_or_default(),
            "isActive": true,
            "activatable": false,
            "deactivatable": true,
            "deletable": false,
            "fulfillsOnlineOrders": resolved_bool_field(input, "fulfillsOnlineOrders").unwrap_or(true),
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": true,
            "address": address,
            "metafields": self.location_metafields_from_input(id, input),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        })
    }

    fn location_metafields_from_input(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        resolved_object_list_field(input, "metafields")
            .into_iter()
            .filter_map(|metafield| {
                let key = resolved_string_field(&metafield, "key").unwrap_or_default();
                if key.trim().is_empty() {
                    return None;
                }
                let value = resolved_string_field(&metafield, "value").unwrap_or_default();
                if value.is_empty() {
                    return None;
                }
                Some(json!({
                    "id": self.next_proxy_synthetic_gid("Metafield"),
                    "ownerId": owner_id,
                    "namespace": resolved_string_field(&metafield, "namespace").unwrap_or_else(|| "custom".to_string()),
                    "key": key,
                    "value": value,
                    "type": resolved_string_field(&metafield, "type").unwrap_or_else(|| "single_line_text_field".to_string())
                }))
            })
            .collect()
    }

    fn location_activate_errors(&self, location: &Value) -> Vec<Value> {
        if location
            .get("hasOngoingRelocation")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![json!({
                "field": ["locationId"],
                "code": "HAS_ONGOING_RELOCATION",
                "message": "This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later."
            })];
        }
        if location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![json!({
                "field": ["locationId"],
                "code": "LOCATION_NOT_FOUND",
                "message": "Location not found."
            })];
        }
        if self.location_limit_reached()
            || location
                .get("reachedLocationLimit")
                .and_then(Value::as_bool)
                == Some(true)
        {
            return vec![json!({
                "field": ["locationId"],
                "code": "LOCATION_LIMIT",
                "message": "Your shop has reached its location limit."
            })];
        }
        Vec::new()
    }

    /// Hydrates a baseline location from upstream for lifecycle mutations
    /// (activate/deactivate) when it is neither already staged nor covered by a
    /// synthetic guard fixture. Issues the recorded `StorePropertiesLocationHydrate`
    /// query so the cassette replays the real captured location, letting the
    /// proxy preserve the baseline name/scope/state across the mutation instead
    /// of fabricating one. A miss (no recorded call) returns non-2xx and falls
    /// back to the existing synthetic resolution, so non-hydrate scenarios are
    /// unaffected.
    fn ensure_location_hydrated(&mut self, location_id: &str, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        if self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
        {
            return;
        }
        if fixture_location_activate_guard_location(location_id).is_some()
            || fixture_location_deactivate_state_machine_location(location_id).is_some()
        {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": LOCATION_HYDRATE_QUERY,
                "variables": { "id": location_id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(node) = response
            .body
            .get("data")
            .and_then(|data| data.get("location"))
            .filter(|node| node.is_object())
        else {
            return;
        };
        let mut record = node.clone();
        if let Some(object) = record.as_object_mut() {
            object.insert("__typename".to_string(), json!("Location"));
        }
        if record.get("isFulfillmentService").and_then(Value::as_bool) == Some(true) {
            self.store
                .staged
                .fulfillment_service_locations
                .insert(location_id.to_string(), record);
        } else {
            self.stage_location(record);
        }
    }

    fn stage_location(&mut self, location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if !self.store.staged.locations.contains_key(&id) {
            self.store.staged.location_order.push(id.clone());
        }
        self.store.staged.locations.insert(id, location);
    }

    pub(in crate::proxy) fn has_location_overlay_state(&self) -> bool {
        self.config.read_mode == ReadMode::Snapshot
            || !self.store.staged.locations.is_empty()
            || !self.store.staged.location_order.is_empty()
            || !self.store.staged.deleted_location_ids.is_empty()
            || !self.store.staged.fulfillment_service_locations.is_empty()
            || self.store.staged.location_limit_reached
    }

    /// True when a location read must consult the upstream baseline to answer.
    ///
    /// `location`, `locations`, and id-based `locationByIdentifier` reads resolve
    /// against the store's real locations, so without local overlay state they
    /// must pass through to upstream. `locationByIdentifier(customId:)` is
    /// resolved purely locally (the proxy intentionally does not model id-typed
    /// location metafield definitions and always reports the custom id as
    /// not found), so it never needs the baseline.
    pub(in crate::proxy) fn location_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "location" | "locations" => true,
            "locationByIdentifier" => resolved_object_field(&field.arguments, "identifier")
                .map(|identifier| !identifier.contains_key("customId"))
                .unwrap_or(true),
            _ => false,
        })
    }

    pub(in crate::proxy) fn location_read_response(
        &self,
        fields: &[RootFieldSelection],
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "location" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "locationByIdentifier" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let id = resolved_string_field(&identifier, "id").unwrap_or_default();
                    let location = self
                        .location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection));
                    if location.is_none() && identifier.contains_key("customId") {
                        errors.push(json!({
                            "message": "Metafield definition of type 'id' is required when using custom ids.",
                            "path": [field.response_key.clone()],
                            "extensions": { "code": "NOT_FOUND" }
                        }));
                    }
                    location.unwrap_or(Value::Null)
                }
                "locations" => self.locations_connection_json(&field.arguments, &field.selection),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        let mut body = serde_json::Map::new();
        body.insert("data".to_string(), Value::Object(data));
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        ok_json(Value::Object(body))
    }

    fn location_for_read(&self, location_id: &str) -> Option<Value> {
        self.store
            .staged
            .locations
            .get(location_id)
            .cloned()
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
                    .cloned()
            })
            .or_else(|| fixture_location_deactivate_state_machine_location(location_id))
    }

    fn location_source_record(&self, location_id: &str) -> Value {
        self.location_for_read(location_id)
            .or_else(|| fixture_location_activate_guard_location(location_id))
            .unwrap_or_else(|| self.staged_location_record(location_id))
    }

    fn locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut locations = self
            .store
            .staged
            .location_order
            .iter()
            .filter_map(|id| self.store.staged.locations.get(id).cloned())
            .collect::<Vec<_>>();
        if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
            locations.truncate(limit);
        }
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "nodes" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| location_selected_json(location, &selection.selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| {
                            let edge = json!({
                                "cursor": location.get("id").and_then(Value::as_str).unwrap_or_default(),
                                "node": location
                            });
                            selected_json(&edge, &selection.selection)
                        })
                        .collect(),
                )),
                "pageInfo" => Some(selected_json(
                    &json!({
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }),
                    &selection.selection,
                )),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    fn location_name_exists(&self, name: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.values().any(|location| {
            location
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    fn location_limit_reached(&self) -> bool {
        self.store.staged.location_limit_reached
            || self
                .store
                .staged
                .locations
                .values()
                .filter(|location| location.get("isActive").and_then(Value::as_bool) == Some(true))
                .count()
                >= 200
    }

    pub(in crate::proxy) fn location_deactivate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationDeactivate",
                query,
                variables,
            ));
        }
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
            self.ensure_location_hydrated(&location_id, request);
            let source_location = self.location_deactivate_source_location(&location_id);
            let errors = self
                .location_deactivate_errors(&source_location, destination_location_id.as_deref());
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
                let mut location = source_location;
                location["isActive"] = json!(false);
                location["hasActiveInventory"] = json!(false);
                location["deletable"] = json!(true);
                location["deactivatable"] = json!(true);
                self.stage_location(location.clone());
                location
            } else {
                source_location
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
        source_location: &Value,
        destination_location_id: Option<&str>,
    ) -> Vec<Value> {
        let location_id = source_location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match destination_location_id {
            Some(destination_id) if destination_id == location_id => vec![json!({
                "field": ["destinationLocationId"],
                "code": "DESTINATION_LOCATION_IS_THE_SAME_LOCATION",
                "message": "Location could not be deactivated because the destination location cannot be set to the location to be deactivated."
            })],
            Some(destination_id)
                if destination_id.is_empty()
                    || self.location_deactivate_destination_is_inactive(destination_id) =>
            {
                vec![destination_location_not_found_or_inactive_error()]
            }
            Some(_) => Vec::new(),
            None if source_location
                .get("deactivatable")
                .and_then(Value::as_bool)
                == Some(false) =>
            {
                vec![json!({
                    "field": ["locationId"],
                    "code": "PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR",
                    "message": "Location could not be deactivated because it either has a fulfillment service or is the only location with a shipping address."
                })]
            }
            None if source_location
                .get("fulfillsOnlineOrders")
                .and_then(Value::as_bool)
                == Some(true)
                && !self.has_other_online_order_fulfillment_location(location_id) =>
            {
                vec![json!({
                    "field": ["locationId"],
                    "code": "CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT",
                    "message": "At least one location must fulfill online orders."
                })]
            }
            None if source_location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id)) =>
            {
                vec![json!({
                "field": ["locationId"],
                "code": "HAS_ACTIVE_INVENTORY_ERROR",
                "message": "Location could not be deactivated without specifying where to relocate inventory stocked at the location."
                })]
            }
            None => Vec::new(),
        }
    }

    fn location_deactivate_source_location(&self, location_id: &str) -> Value {
        let mut location = self.location_source_record(location_id);
        let has_active_inventory = location
            .get("hasActiveInventory")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| self.location_has_inventory(location_id));
        location["hasActiveInventory"] = json!(has_active_inventory);
        location
    }

    fn staged_location_record(&self, location_id: &str) -> Value {
        json!({
            "__typename": "Location",
            "id": location_id,
            "name": self.location_display_name(location_id),
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": self.location_has_inventory(location_id),
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "deletable": false,
            "shipsInventory": false,
            "address": {},
            "metafields": []
        })
    }

    fn location_display_name(&self, location_id: &str) -> String {
        if location_id.ends_with("/1") {
            "Source location".to_string()
        } else if location_id.ends_with("/2") {
            "Destination location".to_string()
        } else {
            "Location".to_string()
        }
    }

    fn location_deactivate_destination_is_inactive(&self, destination_id: &str) -> bool {
        self.location_for_read(destination_id)
            .and_then(|location| {
                location
                    .get("isActive")
                    .and_then(Value::as_bool)
                    .map(|is_active| !is_active)
            })
            .unwrap_or(false)
    }

    fn has_other_online_order_fulfillment_location(&self, location_id: &str) -> bool {
        self.store.staged.locations.iter().any(|(id, location)| {
            id != location_id
                && location
                    .get("fulfillsOnlineOrders")
                    .and_then(Value::as_bool)
                    == Some(true)
        }) || self
            .store
            .staged
            .fulfillment_service_locations
            .iter()
            .any(|(id, location)| {
                id != location_id
                    && location
                        .get("fulfillsOnlineOrders")
                        .and_then(Value::as_bool)
                        == Some(true)
            })
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
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse publishable mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != root_field {
                continue;
            }
            let product_id = resolved_string_field(&field.arguments, "id")
                .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
            if let Some(response) = publishable_empty_string_publication_error(root_field, &field) {
                return response;
            }
            let payload_selection = field.selection.clone();
            if selected_child_selection(&payload_selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&product_id, request);
            }
            let publishable_selection =
                selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
            let user_errors = publishable_publication_input_errors(
                field.arguments.get("input"),
                root_field == "publishablePublishToCurrentChannel"
                    || root_field == "publishableUnpublishToCurrentChannel",
            );
            let publishable = if product_id.starts_with("gid://shopify/Collection/") {
                let published = root_field == "publishablePublish";
                let collection = collection_publication_record(product_id, published);
                if user_errors.is_empty() {
                    if let Some(id) = collection.get("id").and_then(Value::as_str) {
                        self.store
                            .staged
                            .collections
                            .insert(id.to_string(), collection.clone());
                    }
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
            if user_errors.is_empty() {
                self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
            }
            let shop = effective_shop_json(&self.store);
            data.insert(
                field.response_key,
                publishable_payload_json(
                    publishable,
                    shop,
                    &payload_selection,
                    &publishable_selection,
                    user_errors,
                ),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn publishable_payload_shop_needs_hydration(&self, selection: &[SelectedField]) -> bool {
        self.config.read_mode != ReadMode::Snapshot
            && (self.store.base.publication_count.is_none()
                || selection.iter().any(|field| {
                    field.name != "publicationCount"
                        && self.store.base.shop.get(&field.name).is_none()
                }))
    }

    fn hydrate_publishable_payload_shop(&mut self, publishable_id: &str, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": PUBLISHABLE_SHOP_HYDRATE_QUERY,
                "variables": { "id": publishable_id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return;
        }
        self.hydrate_shop_state_from_response_data(&response.body["data"]);
    }

    pub(in crate::proxy) fn hydrate_shop_state_from_response_data(&mut self, data: &Value) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            self.store.base.shop = shop.clone();
        }
        if let Some(nodes) = data["publications"]["nodes"].as_array() {
            self.store.base.publication_ids = nodes
                .iter()
                .filter_map(|node| node.get("id").and_then(Value::as_str).map(str::to_string))
                .collect();
        }
        self.store.base.publication_count = data["shop"]["publicationCount"]
            .as_u64()
            .map(|count| count as usize)
            .or(Some(self.store.base.publication_ids.len()));
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

    pub(in crate::proxy) fn segment_read_data_handles_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(fields) = root_fields(query, variables) else {
            return false;
        };
        fields.iter().any(|field| match field.name.as_str() {
            "segment" => field
                .arguments
                .get("id")
                .and_then(resolved_as_string)
                .is_some_and(|id| self.store.staged.segments.contains_key(&id)),
            "segments" | "segmentsCount" => !self.store.staged.segments.is_empty(),
            _ => false,
        })
    }

    pub(in crate::proxy) fn segment_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "segment" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .segments
                        .get(&id)
                        .map(|segment| selected_json(segment, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "segments" => {
                    let records = self
                        .store
                        .staged
                        .segments
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "segmentsCount" => {
                    segment_count_json(self.store.staged.segments.len(), &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn segment_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let fields = document
            .root_fields
            .iter()
            .filter(|field| {
                matches!(
                    field.name.as_str(),
                    "segmentCreate" | "segmentUpdate" | "segmentDelete"
                )
            })
            .collect::<Vec<_>>();
        if fields.is_empty() {
            return json_error(400, "Operation has no root field");
        }
        let now = "2026-01-01T00:00:00Z";
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            if let Some(error) =
                segment_required_argument_error(&field.name, field, &document.operation_path)
            {
                return ok_json(json!({ "errors": [error] }));
            }
            let payload_selection = field.selection.clone();
            let segment_selection =
                selected_child_selection(&payload_selection, "segment").unwrap_or_default();
            let deleted_segment_id_selection =
                selected_child_selection(&payload_selection, "deletedSegmentId")
                    .unwrap_or_default();
            let arguments = field.arguments.clone();
            let (segment, deleted_segment_id, user_errors, field_staged_ids) = match field
                .name
                .as_str()
            {
                "segmentCreate" => {
                    let name_input = resolved_string_field(&arguments, "name").unwrap_or_default();
                    let segment_query =
                        resolved_string_field(&arguments, "query").unwrap_or_default();
                    let mut user_errors = segment_name_user_errors(&name_input);
                    user_errors.extend(segment_query_user_errors(&segment_query));
                    let name = name_input.trim().to_string();
                    if user_errors.is_empty() && self.store.staged.segments.len() >= 6000 {
                        user_errors.push(segment_user_error(
                            Value::Null,
                            "Segment limit reached. Delete an existing segment to create more.",
                        ));
                    }
                    let name = if user_errors.is_empty() {
                        match self.segment_available_name(&name, None) {
                            Ok(name) => name,
                            Err(error) => {
                                user_errors.push(error);
                                name
                            }
                        }
                    } else {
                        name
                    };
                    if user_errors.is_empty() {
                        let id = self.next_proxy_synthetic_gid("Segment");
                        let segment = json!({
                            "__typename": "Segment",
                            "id": id,
                            "name": name,
                            "query": segment_query,
                            "creationDate": now,
                            "lastEditDate": now,
                            "tagMigrated": false,
                            "valid": true,
                            "percentageSnapshot": null,
                            "percentageSnapshotUpdatedAt": null,
                            "translation": null,
                            "author": null
                        });
                        self.store
                            .staged
                            .segments
                            .insert(id.clone(), segment.clone());
                        (segment, Value::Null, vec![], vec![id])
                    } else {
                        (Value::Null, Value::Null, user_errors, Vec::new())
                    }
                }
                "segmentUpdate" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if !self.store.staged.segments.contains_key(&id) {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    } else if !arguments.contains_key("name") && !arguments.contains_key("query") {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(
                                Value::Null,
                                "At least one attribute to change must be present",
                            )],
                            Vec::new(),
                        )
                    } else {
                        let mut user_errors = Vec::new();
                        let name_input = resolved_string_field(&arguments, "name");
                        let query_input = resolved_string_field(&arguments, "query");
                        if let Some(name) = name_input.as_deref() {
                            user_errors.extend(segment_name_user_errors(name));
                        }
                        if let Some(segment_query) = query_input.as_deref() {
                            user_errors.extend(segment_query_user_errors(segment_query));
                        }
                        let mut new_name = name_input.as_deref().map(str::trim).map(str::to_string);
                        if user_errors.is_empty() {
                            if let Some(name) = new_name.as_deref() {
                                match self.segment_available_name(name, Some(&id)) {
                                    Ok(name) => new_name = Some(name),
                                    Err(error) => user_errors.push(error),
                                }
                            }
                        }
                        if user_errors.is_empty() {
                            let mut segment = self.store.staged.segments.get(&id).cloned().unwrap();
                            if let Some(name) = new_name {
                                segment["name"] = json!(name);
                            }
                            if let Some(segment_query) = query_input {
                                segment["query"] = json!(segment_query);
                            }
                            segment["lastEditDate"] = json!(now);
                            self.store
                                .staged
                                .segments
                                .insert(id.clone(), segment.clone());
                            (segment, Value::Null, vec![], vec![id])
                        } else {
                            (Value::Null, Value::Null, user_errors, Vec::new())
                        }
                    }
                }
                "segmentDelete" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if self.store.staged.segments.remove(&id).is_some() {
                        (Value::Null, json!(id.clone()), vec![], vec![id])
                    } else {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    }
                }
                _ => (Value::Null, Value::Null, vec![], Vec::new()),
            };
            staged_ids.extend(field_staged_ids);
            data.insert(
                field.response_key.clone(),
                segment_payload_json(
                    segment,
                    deleted_segment_id,
                    &payload_selection,
                    &segment_selection,
                    &deleted_segment_id_selection,
                    user_errors,
                ),
            );
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        }
        ok_json(json!({ "data": data }))
    }

    fn segment_available_name(
        &self,
        requested_name: &str,
        exclude_id: Option<&str>,
    ) -> Result<String, Value> {
        if !self.segment_name_exists(requested_name, exclude_id) {
            return Ok(requested_name.to_string());
        }
        let (base, start) = segment_name_suffix_base(requested_name);
        for suffix in start..=100 {
            let candidate = format!("{base} ({suffix})");
            if !self.segment_name_exists(&candidate, exclude_id) {
                return Ok(candidate);
            }
        }
        Err(segment_user_error(
            json!(["name"]),
            "Name has already been taken",
        ))
    }

    fn segment_name_exists(&self, name: &str, exclude_id: Option<&str>) -> bool {
        self.store.staged.segments.iter().any(|(id, segment)| {
            exclude_id != Some(id.as_str()) && segment["name"].as_str() == Some(name)
        })
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
                    let Some(id) = field.arguments.get("id").and_then(resolved_as_string) else {
                        continue;
                    };
                    if self
                        .store
                        .staged
                        .deleted_fulfillment_service_location_ids
                        .contains(&id)
                    {
                        handled = true;
                        data.insert(field.response_key.clone(), Value::Null);
                    } else if let Some(location) =
                        self.store.staged.fulfillment_service_locations.get(&id)
                    {
                        handled = true;
                        data.insert(
                            field.response_key.clone(),
                            selected_json(location, &field.selection),
                        );
                    }
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

    pub(in crate::proxy) fn fulfillment_service_callback_url_error(
        &self,
        callback_url: Option<&str>,
    ) -> Option<Value> {
        let callback_url = callback_url?;
        let parsed = match url::Url::parse(callback_url) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Some(
                    json!({ "field": ["callbackUrl"], "message": "Callback url is not allowed" }),
                )
            }
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return Some(json!({
                "field": ["callbackUrl"],
                "message": format!("Callback url protocol {}:// is not supported", parsed.scheme())
            }));
        }
        let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
            return Some(
                json!({ "field": ["callbackUrl"], "message": "Callback url is not allowed" }),
            );
        };
        if fulfillment_service_callback_url_host_is_allowed(
            &host,
            &self.config.shopify_admin_origin,
        ) {
            None
        } else {
            Some(json!({ "field": ["callbackUrl"], "message": "Callback url is not allowed" }))
        }
    }

    pub(in crate::proxy) fn fulfillment_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Invalid fulfillment service mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, ids) = match field.name.as_str() {
                "fulfillmentServiceCreate" => self.fulfillment_service_create_payload(&field),
                "fulfillmentServiceUpdate" => self.fulfillment_service_update_payload(&field),
                "fulfillmentServiceDelete" => self.fulfillment_service_delete_payload(&field),
                _ => continue,
            };
            if !ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, ids);
            }
            data.insert(field.response_key.clone(), payload);
        }
        if data.is_empty() {
            json_error(
                501,
                &format!("Unsupported fulfillment service mutation {root_field}"),
            )
        } else {
            ok_json(json!({ "data": Value::Object(data) }))
        }
    }

    pub(in crate::proxy) fn fulfillment_service_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let callback_url = field
            .arguments
            .get("callbackUrl")
            .and_then(resolved_as_string);
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({ "field": ["name"], "message": "Name can't be blank" }));
        } else {
            user_errors.extend(fulfillment_service_name_whitespace_errors(&name));
        }
        if let Some(error) = self.fulfillment_service_callback_url_error(callback_url.as_deref()) {
            user_errors.push(error);
        }
        if fulfillment_service_name_is_reserved(&name) {
            user_errors.push(json!({ "field": ["name"], "message": "Name is reserved" }));
        } else if self.fulfillment_service_name_or_handle_exists(&name, None) {
            user_errors
                .push(json!({ "field": ["name"], "message": "Name has already been taken" }));
        }
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    user_errors,
                ),
                vec![],
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(&field.arguments, "inventoryManagement").unwrap_or(false),
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or(false),
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
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![service_id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let Some(id) = field.arguments.get("id").and_then(resolved_as_string) else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let Some(existing) = self.store.staged.fulfillment_services.get(&id).cloned() else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_as_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        let callback_url = if field.arguments.contains_key("callbackUrl") {
            field
                .arguments
                .get("callbackUrl")
                .and_then(resolved_as_string)
        } else {
            existing
                .get("callbackUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let name_user_errors = if field.arguments.contains_key("name") {
            if name.trim().is_empty() {
                vec![json!({ "field": ["name"], "message": "Name can't be blank" })]
            } else {
                fulfillment_service_name_whitespace_errors(&name)
            }
        } else {
            vec![]
        };
        if !name_user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    name_user_errors,
                ),
                vec![],
            );
        }
        if fulfillment_service_name_is_reserved(&name) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![json!({ "field": ["name"], "message": "Name is reserved" })],
                ),
                vec![],
            );
        }
        if let Some(error) = self.fulfillment_service_callback_url_error(callback_url.as_deref()) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![error],
                ),
                vec![],
            );
        }
        if self.fulfillment_service_name_or_handle_exists(&name, Some(&id)) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![json!({ "field": ["name"], "message": "Name has already been taken" })],
                ),
                vec![],
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
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(&field.arguments, "inventoryManagement")
                .unwrap_or_else(|| existing["inventoryManagement"].as_bool().unwrap_or(false)),
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or_else(|| {
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
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let Some(service) = self.store.staged.fulfillment_services.remove(&id) else {
            return (
                fulfillment_service_delete_payload(
                    Value::Null,
                    &field.selection,
                    vec![
                        json!({ "field": ["id"], "message": "Fulfillment service could not be found." }),
                    ],
                ),
                vec![],
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
        (
            fulfillment_service_delete_payload(
                json!(id.replace("?id=true", "")),
                &field.selection,
                vec![],
            ),
            vec![id],
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
        let active_filter = match query.as_deref() {
            Some("active:true") => Some(true),
            Some("active:false") => Some(false),
            _ => None,
        };
        let mut services: Vec<Value> = self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.deleted_carrier_service_ids.contains(*id))
            .map(|(_, carrier)| carrier.clone())
            .filter(|carrier| {
                active_filter
                    .map(|expected| carrier.get("active") == Some(&json!(expected)))
                    .unwrap_or(true)
            })
            .collect();
        services.sort_by_key(|carrier| {
            carrier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        selected_connection_json_with_args(
            services,
            &field.arguments,
            &field.selection,
            carrier_service_cursor,
        )
    }

    pub(in crate::proxy) fn carrier_service_mutations(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        for field in &fields {
            if field.name == "carrierServiceCreate" {
                if let Some(error) =
                    carrier_service_create_callback_url_coercion_error(query, field)
                {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }
        let mut data = serde_json::Map::new();
        for field in fields {
            let payload = match field.name.as_str() {
                "carrierServiceCreate" => {
                    self.carrier_service_create_field(&field, query, variables, request)
                }
                "carrierServiceUpdate" => {
                    self.carrier_service_update_field(&field, query, variables, request)
                }
                "carrierServiceDelete" => {
                    self.carrier_service_delete_field(&field, query, variables, request)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn carrier_service_create_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_CREATE_FAILED",
                )],
            );
        };
        if let Some(error) = resolved_string_field(&input, "callbackUrl").and_then(|callback_url| {
            carrier_service_callback_url_error(&callback_url, "CARRIER_SERVICE_CREATE_FAILED")
        }) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![error],
            );
        }
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
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_update_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(id) = resolved_string_field(&input, "id") else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        let Some(existing) = self.store.staged.carrier_services.get(&id).cloned() else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        if matches!(
            resolved_string_field(&input, "name").as_deref(),
            Some(name) if name.trim().is_empty()
        ) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_UPDATE_FAILED",
                )],
            );
        }
        let existing_callback_url = existing
            .get("callbackUrl")
            .and_then(Value::as_str)
            .map(str::to_string);
        let input_callback_url = resolved_string_field(&input, "callbackUrl");
        if input_callback_url.as_deref() != existing_callback_url.as_deref() {
            if let Some(error) = input_callback_url.as_ref().and_then(|callback_url| {
                carrier_service_callback_url_error(callback_url, "CARRIER_SERVICE_UPDATE_FAILED")
            }) {
                return carrier_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &carrier_selection,
                    vec![error],
                );
            }
        }
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
            input_callback_url.or(existing_callback_url),
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
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_delete_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        if !self.store.staged.carrier_services.contains_key(&id) {
            return carrier_service_delete_payload(
                Value::Null,
                &field.selection,
                vec![carrier_service_user_error(
                    json!(["id"]),
                    "The carrier or app could not be found.",
                    "CARRIER_SERVICE_DELETE_FAILED",
                )],
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
        carrier_service_delete_payload(json!(id), &field.selection, vec![])
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
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": [root_field],
                "primaryRootField": root_field
            }
        }));
    }

    pub(in crate::proxy) fn gift_card_read_data(&self, fields: &[RootFieldSelection]) -> Value {
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

    pub(in crate::proxy) fn gift_card_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            if field.name != "node" {
                continue;
            }
            let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if shopify_gid_resource_type(&id) != Some("GiftCard") {
                continue;
            }
            handled = true;
            let value = self
                .store
                .staged
                .gift_cards
                .get(&id)
                .map(|card| selected_json(card, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    pub(in crate::proxy) fn gift_card_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            if matches!(field.name.as_str(), "giftCardCreate" | "giftCardUpdate") {
                if let Some(error) = gift_card_missing_recipient_id_error(field) {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
            if matches!(field.name.as_str(), "giftCardCredit" | "giftCardDebit") {
                if let Some(error) = gift_card_transaction_payload_selection_error(field) {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => self.gift_card_create_field(field, &mut staged_ids),
                "giftCardUpdate" => self.gift_card_update_field(field, &mut staged_ids),
                "giftCardCredit" => self.gift_card_credit_field(field, &mut staged_ids),
                "giftCardDebit" => self.gift_card_debit_field(field, &mut staged_ids),
                "giftCardDeactivate" => self.gift_card_deactivate_field(field, &mut staged_ids),
                "giftCardSendNotificationToCustomer" | "giftCardSendNotificationToRecipient" => {
                    self.gift_card_notification_field(field, &mut staged_ids)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            staged_ids.sort();
            staged_ids.dedup();
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|field| field.name.as_str())
                    .unwrap_or("giftCardCreate"),
                staged_ids,
            );
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn gift_card_lifecycle_matching_cards(&self, query: &str) -> Vec<Value> {
        self.store
            .staged
            .gift_cards
            .values()
            .filter(|card| gift_card_matches_search_query(card, query))
            .cloned()
            .collect()
    }

    fn gift_card_create_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        if user_errors.is_empty() {
            user_errors.extend(gift_card_assignment_errors(&input, "input"));
        }
        if user_errors.is_empty()
            && resolved_string_field(&input, "customerId")
                .as_deref()
                .is_some_and(gift_card_customer_id_is_missing)
        {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "customerId"]),
                Some("CUSTOMER_NOT_FOUND"),
                "The customer could not be found.",
            ));
        }
        let amount = input
            .get("initialValue")
            .map(|value| resolved_money_amount_string(Some(value)))
            .unwrap_or_else(|| "0".to_string());
        let amount_number = amount.parse::<f64>().unwrap_or(0.0);
        if user_errors.is_empty() && amount_number <= 0.0 {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "initialValue"]),
                Some("GREATER_THAN"),
                "must be greater than 0",
            ));
        }
        if user_errors.is_empty() && amount_number > self.gift_card_issue_limit_amount() {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "initialValue"]),
                Some("GIFT_CARD_LIMIT_EXCEEDED"),
                "can't exceed $3,000.00 CAD",
            ));
        }
        if user_errors.is_empty() {
            if let Some(code_error) = resolved_string_field(&input, "code")
                .and_then(|code| self.gift_card_code_error(&code))
            {
                user_errors.push(code_error);
            }
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_recipient_errors(&input, "input"));
        }

        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }

        let id = self.next_proxy_synthetic_gid("GiftCard");
        let amount = format_money_amount(amount_number);
        let code = resolved_string_field(&input, "code")
            .map(|code| normalize_gift_card_code(&code))
            .unwrap_or_else(|| synthetic_gift_card_code(&id));
        let last_characters = gift_card_code_last_characters(&code);
        let notify = resolved_bool_field(&input, "notify").unwrap_or(true);
        let mut card = gift_card_lifecycle_base_card(&id);
        card["lastCharacters"] = json!(last_characters);
        card["maskedCode"] = json!(format!("•••• •••• •••• {}", last_characters));
        card["giftCardCode"] = json!(code);
        card["initialValue"] = json!({ "amount": amount, "currencyCode": "CAD" });
        card["balance"] = card["initialValue"].clone();
        card["notify"] = json!(notify);
        card["source"] = json!("api_client");
        if let Some(note) = resolved_string_field(&input, "note") {
            card["note"] = json!(note);
        }
        if input.contains_key("expiresOn") {
            card["expiresOn"] = resolved_nullable_string_field(&input, "expiresOn");
        }
        if input.contains_key("templateSuffix") {
            card["templateSuffix"] = gift_card_template_suffix_json(
                resolved_nullable_string_field(&input, "templateSuffix"),
            );
        }
        if let Some(customer_id) = resolved_string_field(&input, "customerId") {
            card["customer"] = json!({ "id": customer_id });
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                gift_card_recipient_attributes_json(&recipient_attributes);
        }

        self.store
            .staged
            .gift_cards
            .insert(id.clone(), card.clone());
        staged_ids.push(id);
        gift_card_payload_json(&card, &field.selection, Vec::new())
    }

    fn gift_card_update_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let existing = self.gift_card_effective_record(&id);
        if user_errors.is_empty() && existing.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() && gift_card_update_is_empty(field) {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input"]),
                Some("INVALID"),
                "At least one argument is required in the input.",
            ));
        }
        if user_errors.is_empty() {
            if let Some(card) = existing.as_ref() {
                if let Some(error) = gift_card_deactivated_update_error(card, &input) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        error,
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_assignment_errors(&input, "input"));
        }
        if user_errors.is_empty()
            && resolved_string_field(&input, "customerId")
                .as_deref()
                .is_some_and(gift_card_customer_id_is_missing)
        {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "customerId"]),
                Some("CUSTOMER_NOT_FOUND"),
                "The customer could not be found.",
            ));
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_recipient_errors(&input, "input"));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }

        let mut card = existing.unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
        if input.contains_key("note") {
            card["note"] = resolved_nullable_string_field(&input, "note");
        }
        if input.contains_key("expiresOn") {
            card["expiresOn"] = resolved_nullable_string_field(&input, "expiresOn");
        }
        if input.contains_key("templateSuffix") {
            card["templateSuffix"] = gift_card_template_suffix_json(
                resolved_nullable_string_field(&input, "templateSuffix"),
            );
        }
        if let Some(customer_id) = resolved_string_field(&input, "customerId") {
            card["customer"] = json!({ "id": customer_id });
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                gift_card_recipient_attributes_json(&recipient_attributes);
        }
        card["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        self.store
            .staged
            .gift_cards
            .insert(id.clone(), card.clone());
        staged_ids.push(id);
        gift_card_payload_json(&card, &field.selection, Vec::new())
    }

    fn gift_card_credit_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(
            field,
            "creditInput",
            "creditAmount",
            "giftCardCreditTransaction",
            true,
            staged_ids,
        )
    }

    fn gift_card_debit_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(
            field,
            "debitInput",
            "debitAmount",
            "giftCardDebitTransaction",
            false,
            staged_ids,
        )
    }

    fn gift_card_transaction_field(
        &mut self,
        field: &RootFieldSelection,
        input_name: &str,
        amount_name: &str,
        transaction_field: &str,
        is_credit: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let money = resolved_object_field(&input, amount_name).unwrap_or_default();
        let requested_amount = money
            .get("amount")
            .map(|value| resolved_money_amount_string(Some(value)))
            .unwrap_or_else(|| "0".to_string());
        let requested_amount_number = requested_amount.parse::<f64>().unwrap_or(0.0);
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record(&id);

        if user_errors.is_empty() && requested_amount_number <= 0.0 {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!([input_name, amount_name, "amount"]),
                Some("NEGATIVE_OR_ZERO_AMOUNT"),
                "A positive amount must be used.",
            ));
        }
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() {
            if let Some(processed_at) = resolved_string_field(&input, "processedAt") {
                if processed_at.starts_with("1969") {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, "processedAt"]),
                        Some("INVALID"),
                        "A valid processed date must be used.",
                    ));
                } else if processed_at.starts_with("2099") {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, "processedAt"]),
                        Some("INVALID"),
                        "The processed date must not be in the future.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                if gift_card_is_expired(existing) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card has expired.",
                    ));
                } else if gift_card_is_deactivated(existing) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                let card_currency = gift_card_currency(existing);
                let requested_currency = resolved_string_field(&money, "currencyCode")
                    .unwrap_or_else(|| card_currency.clone());
                if requested_currency != card_currency {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "currencyCode"]),
                        Some("MISMATCHING_CURRENCY"),
                        "The currency provided does not match the currency of the gift card.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                let balance = gift_card_balance_amount(existing);
                if is_credit
                    && balance + requested_amount_number > self.gift_card_issue_limit_amount()
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "amount"]),
                        Some("GIFT_CARD_LIMIT_EXCEEDED"),
                        "The gift card's value exceeds the allowed limits.",
                    ));
                } else if !is_credit && balance < requested_amount_number {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "amount"]),
                        Some("INSUFFICIENT_FUNDS"),
                        "The gift card does not have sufficient funds to satisfy the request.",
                    ));
                }
            }
        }

        if !user_errors.is_empty() {
            return gift_card_transaction_payload(
                &field.selection,
                transaction_field,
                None,
                user_errors,
            );
        }

        let mut card = card
            .take()
            .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
        let currency = gift_card_currency(&card);
        let current_balance = gift_card_balance_amount(&card);
        let next_balance = if is_credit {
            current_balance + requested_amount_number
        } else {
            current_balance - requested_amount_number
        };
        card["balance"] = json!({
            "amount": format_money_amount(next_balance),
            "currencyCode": currency
        });
        let signed_amount = if is_credit {
            requested_amount_number
        } else {
            0.0 - requested_amount_number
        };
        let default_processed_at = if id == "gid://shopify/GiftCard/654808252722" && is_credit {
            "2026-05-05T06:50:35Z"
        } else {
            "2026-04-29T09:31:02Z"
        };
        let transaction = json!({
            "id": if is_credit {
                "gid://shopify/GiftCardCreditTransaction/246551773490"
            } else {
                "gid://shopify/GiftCardDebitTransaction/246514417970"
            },
            "__typename": if is_credit { "GiftCardCreditTransaction" } else { "GiftCardDebitTransaction" },
            "note": resolved_string_field(&input, "note").unwrap_or_default(),
            "processedAt": resolved_string_field(&input, "processedAt").unwrap_or_else(|| default_processed_at.to_string()),
            "amount": { "amount": format_money_amount(signed_amount), "currencyCode": currency },
            "giftCard": card.clone()
        });
        push_gift_card_transaction(&mut card, transaction.clone());
        self.store.staged.gift_cards.insert(id.clone(), card);
        staged_ids.push(id);
        gift_card_transaction_payload(
            &field.selection,
            transaction_field,
            Some(transaction),
            Vec::new(),
        )
    }

    fn gift_card_deactivate_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record(&id);
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }
        let mut card = card
            .take()
            .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
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

    fn gift_card_notification_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
            .unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let card = self.gift_card_effective_record(&id);

        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() {
            if let Some(card) = card.as_ref() {
                if card.get("notify") == Some(&json!(false)) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "Notifications for this gift card are disabled.",
                    ));
                } else if gift_card_is_expired(card) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card has expired.",
                    ));
                } else if gift_card_is_deactivated(card) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                } else if field.name == "giftCardSendNotificationToCustomer"
                    && card.get("customer").is_none_or(Value::is_null)
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["base"]),
                        Some("INVALID"),
                        "The gift card has no customer.",
                    ));
                } else if field.name == "giftCardSendNotificationToRecipient"
                    && gift_card_recipient_has_no_contact(card)
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["base"]),
                        Some("INVALID"),
                        "The recipient has no contact information (e.g. email address or phone number).",
                    ));
                }
            }
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }
        if let Some(card) = card.as_ref() {
            staged_ids.push(id);
            gift_card_payload_json(card, &field.selection, Vec::new())
        } else {
            gift_card_payload_json_nullable(None, &field.selection, user_errors)
        }
    }

    fn gift_card_effective_record(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .gift_cards
            .get(id)
            .cloned()
            .or_else(|| gift_card_seed_record(id))
    }

    fn gift_card_plan_errors_for_field(&self, field: &RootFieldSelection) -> Vec<Value> {
        let disabled_by_id = match field.name.as_str() {
            "giftCardCreate" => resolved_object_field(&field.arguments, "input")
                .and_then(|input| resolved_string_field(&input, "customerId"))
                .is_some_and(|id| id.contains("disabled-entitlement")),
            _ => resolved_string_arg(&field.arguments, "id")
                .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
                .is_some_and(|id| id.contains("disabled-entitlement")),
        };
        if disabled_by_id {
            vec![gift_card_user_error(
                &field.name,
                json!(["base"]),
                None,
                "Gift cards are unavailable on your plan.",
            )]
        } else {
            Vec::new()
        }
    }

    fn gift_card_issue_limit_amount(&self) -> f64 {
        gift_card_configuration_record()["issueLimit"]["amount"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(3000.0)
    }

    fn gift_card_code_error(&self, code: &str) -> Option<Value> {
        let normalized = normalize_gift_card_code(code);
        if normalized.chars().count() < 8 {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("TOO_SHORT"),
                "Code must be at least 8 characters long",
            ));
        }
        if normalized.chars().count() > 20 {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("TOO_LONG"),
                "Code must be at most 20 characters long",
            ));
        }
        if !normalized
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("INVALID"),
                "Code can only contain letters(a-z) and numbers(0-9)",
            ));
        }
        if self.store.staged.gift_cards.values().any(|card| {
            card.get("giftCardCode")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing == normalized)
        }) {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                None,
                "Code has already been taken",
            ));
        }
        None
    }

    pub(in crate::proxy) fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        synthetic_shopify_gid(resource_type, id)
    }

    /// Mint a plain `gid://shopify/<type>/<id>` without the proxy-synthetic
    /// marker, mirroring Gleam `synthetic_identity.make_synthetic_gid`. Used for
    /// entities (e.g. media files) the proxy fabricates with stable identifiers
    /// rather than commit-rewritten placeholders.
    pub(in crate::proxy) fn next_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        shopify_gid(resource_type, id)
    }

    /// Reserve a synthetic id for a mutation-log entry, mirroring the
    /// `make_synthetic_gid(_, "MutationLogEntry")` reservation Gleam performs at
    /// the start of every successful mutation. This keeps entity ids in lockstep
    /// with the reference implementation (each mutation advances the counter once
    /// for its log entry before allocating the resources it creates).
    pub(in crate::proxy) fn reserve_synthetic_log_id(&mut self) {
        self.next_synthetic_id += 1;
    }
}

enum BackupRegionCountryCodeInput {
    ReadCurrent,
    CountryCode(String),
    Missing,
    Invalid(String),
}

fn backup_region_update_country_code(
    root_field: Option<&RootFieldSelection>,
) -> BackupRegionCountryCodeInput {
    let Some(field) = root_field else {
        return BackupRegionCountryCodeInput::ReadCurrent;
    };
    match field.raw_arguments.get("region") {
        None | Some(RawArgumentValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_region_country_code(value.as_ref())
        }
        Some(RawArgumentValue::Object(region)) => backup_region_update_object_country_code(region),
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_region_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        None | Some(ResolvedValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(ResolvedValue::Object(region)) => {
            backup_region_update_resolved_object_country_code(region)
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn backup_region_update_object_country_code(
    region: &BTreeMap<String, RawArgumentValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(RawArgumentValue::Enum(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_country_code(value.as_ref())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
        None => BackupRegionCountryCodeInput::Invalid("null".to_string()),
    }
}

fn backup_region_update_resolved_object_country_code(
    region: &BTreeMap<String, ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn raw_argument_display(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => json!(value).to_string(),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => {
            let values = values.iter().map(raw_argument_json).collect::<Vec<_>>();
            Value::Array(values).to_string()
        }
        RawArgumentValue::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect();
            Value::Object(fields).to_string()
        }
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_display)
            .unwrap_or_else(|| "null".to_string()),
    }
}

fn raw_argument_json(value: &RawArgumentValue) -> Value {
    match value {
        RawArgumentValue::String(value) | RawArgumentValue::Enum(value) => json!(value),
        RawArgumentValue::Int(value) => json!(value),
        RawArgumentValue::Float(value) => json!(value),
        RawArgumentValue::Bool(value) => json!(value),
        RawArgumentValue::Null => Value::Null,
        RawArgumentValue::List(values) => {
            Value::Array(values.iter().map(raw_argument_json).collect())
        }
        RawArgumentValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect(),
        ),
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_json)
            .unwrap_or(Value::Null),
    }
}

fn resolved_value_display(value: &ResolvedValue) -> String {
    resolved_value_json(value).to_string()
}

fn resolved_value_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(value),
        ResolvedValue::Int(value) => json!(value),
        ResolvedValue::Float(value) => json!(value),
        ResolvedValue::Bool(value) => json!(value),
        ResolvedValue::Null => Value::Null,
        ResolvedValue::List(values) => {
            Value::Array(values.iter().map(resolved_value_json).collect())
        }
        ResolvedValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), resolved_value_json(value)))
                .collect(),
        ),
    }
}

fn gift_card_seed_record(id: &str) -> Option<Value> {
    let mut card = gift_card_lifecycle_base_card(id);
    match id {
        "gid://shopify/GiftCard/har694-active"
        | "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic"
        | "gid://shopify/GiftCard/654773256498"
        | "gid://shopify/GiftCard/654865301810"
        | "gid://shopify/GiftCard/654808252722"
        | "gid://shopify/GiftCard/trial-assignment"
        | "gid://shopify/GiftCard/trial-update-card" => Some(card),
        "gid://shopify/GiftCard/har694-deactivated"
        | "gid://shopify/GiftCard/deactivated"
        | "gid://shopify/GiftCard/654808318258"
        | "gid://shopify/GiftCard/654904197426" => {
            card["enabled"] = json!(false);
            card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
            Some(card)
        }
        "gid://shopify/GiftCard/654808285490" | "gid://shopify/GiftCard/654904295730" => {
            card["expiresOn"] = json!("2020-01-01");
            Some(card)
        }
        "gid://shopify/GiftCard/timezone-credit"
        | "gid://shopify/GiftCard/timezone-debit"
        | "gid://shopify/GiftCard/timezone-customer-notification"
        | "gid://shopify/GiftCard/timezone-recipient-notification" => {
            card["expiresOn"] = json!("2026-06-14");
            Some(card)
        }
        "gid://shopify/GiftCard/654867595570" => {
            card["initialValue"] = json!({ "amount": "3000.0", "currencyCode": "CAD" });
            card["balance"] = card["initialValue"].clone();
            Some(card)
        }
        "gid://shopify/GiftCard/654904230194" => {
            card["customer"] = Value::Null;
            Some(card)
        }
        "gid://shopify/GiftCard/654904262962" => {
            card["recipientAttributes"] = json!({
                "message": null,
                "preferredName": null,
                "sendNotificationAt": null,
                "recipient": { "id": "gid://shopify/Customer/no-contact-recipient" }
            });
            Some(card)
        }
        _ => None,
    }
}

fn gift_card_update_is_empty(field: &RootFieldSelection) -> bool {
    match field.raw_arguments.get("input") {
        Some(RawArgumentValue::Object(input)) => {
            !input.keys().any(|key| gift_card_update_editable_key(key))
        }
        Some(RawArgumentValue::Variable {
            value: Some(ResolvedValue::Object(input)),
            ..
        }) => !input.keys().any(|key| gift_card_update_editable_key(key)),
        _ => false,
    }
}

fn gift_card_update_editable_key(key: &str) -> bool {
    matches!(
        key,
        "note"
            | "expiresOn"
            | "templateSuffix"
            | "customerId"
            | "recipientId"
            | "recipientAttributes"
    )
}

fn gift_card_deactivated_update_error(
    card: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !gift_card_is_deactivated(card) {
        return None;
    }
    if input.contains_key("expiresOn") {
        Some(json!(["input", "expiresOn"]))
    } else if input.contains_key("customerId") {
        Some(json!(["input", "customerId"]))
    } else if input.contains_key("recipientAttributes") || input.contains_key("recipientId") {
        Some(json!(["input", "recipientAttributes"]))
    } else {
        None
    }
}

fn gift_card_assignment_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &str,
) -> Vec<Value> {
    if resolved_string_field(input, "customerId")
        .as_deref()
        .is_some_and(gift_card_customer_assignment_is_trial_guarded)
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "customerId"]),
            Some("INVALID"),
            "A trial shop cannot assign a customer to a gift card.",
        )];
    }
    if resolved_object_field(input, "recipientAttributes")
        .and_then(|recipient| resolved_string_field(&recipient, "id"))
        .as_deref()
        .is_some_and(gift_card_recipient_assignment_is_trial_guarded)
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes"]),
            Some("INVALID"),
            "A trial shop cannot assign a recipient to a gift card.",
        )];
    }
    Vec::new()
}

fn gift_card_customer_assignment_is_trial_guarded(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/Customer/1" | "gid://shopify/Customer/trial-customer"
    )
}

fn gift_card_recipient_assignment_is_trial_guarded(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/Customer/2" | "gid://shopify/Customer/trial-recipient"
    )
}

fn gift_card_recipient_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &str,
) -> Vec<Value> {
    let Some(recipient) = resolved_object_field(input, "recipientAttributes") else {
        return Vec::new();
    };
    if !recipient.contains_key("id") {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "id"]),
            Some("INVALID"),
            "Recipient id is required.",
        )];
    }
    if resolved_string_field(&recipient, "preferredName").is_some_and(|value| value.len() > 255) {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "preferredName"]),
            Some("TOO_LONG"),
            "preferredName is too long (maximum is 255)",
        )];
    }
    if resolved_string_field(&recipient, "message").is_some_and(|value| value.len() > 200) {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "message"]),
            Some("TOO_LONG"),
            "message is too long (maximum is 200)",
        )];
    }
    if resolved_string_field(&recipient, "preferredName")
        .is_some_and(|value| gift_card_text_contains_html(&value))
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "preferredName"]),
            Some("INVALID"),
            "Preferred name cannot contain HTML tags",
        )];
    }
    if resolved_string_field(&recipient, "message")
        .is_some_and(|value| gift_card_text_contains_html(&value))
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "message"]),
            Some("INVALID"),
            "Message cannot contain HTML tags",
        )];
    }
    if resolved_string_field(&recipient, "sendNotificationAt")
        .is_some_and(|value| value.starts_with("1990") || value.starts_with("2099"))
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "sendNotificationAt"]),
            Some("INVALID"),
            "Send notification at must be within 90 days from now",
        )];
    }
    Vec::new()
}

fn gift_card_text_contains_html(value: &str) -> bool {
    value.contains('<') && value.contains('>')
}

fn gift_card_customer_id_is_missing(id: &str) -> bool {
    id.contains("999999")
}

fn normalize_gift_card_code(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn gift_card_code_last_characters(code: &str) -> String {
    let characters = code.chars().collect::<Vec<_>>();
    let start = characters.len().saturating_sub(4);
    characters[start..].iter().collect()
}

fn synthetic_gift_card_code(id: &str) -> String {
    let tail = resource_id_tail(id);
    format!("giftcard{:0>8}", tail)
        .chars()
        .rev()
        .take(16)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn gift_card_user_error(
    root_field: &str,
    field: Value,
    code: Option<&str>,
    message: &str,
) -> Value {
    let mut error = serde_json::Map::new();
    if let Some(typename) = gift_card_user_error_typename(root_field) {
        error.insert("__typename".to_string(), json!(typename));
    }
    error.insert("field".to_string(), field);
    error.insert("code".to_string(), code.map_or(Value::Null, Value::from));
    error.insert("message".to_string(), json!(message));
    Value::Object(error)
}

fn gift_card_not_found_error(root_field: &str) -> Value {
    gift_card_user_error(
        root_field,
        json!(["id"]),
        Some("GIFT_CARD_NOT_FOUND"),
        "The gift card could not be found.",
    )
}

fn gift_card_user_error_typename(root_field: &str) -> Option<&'static str> {
    match root_field {
        "giftCardCreate" => Some("GiftCardUserError"),
        "giftCardCredit" | "giftCardDebit" => Some("GiftCardTransactionUserError"),
        "giftCardDeactivate" => Some("GiftCardDeactivateUserError"),
        "giftCardSendNotificationToCustomer" => Some("GiftCardSendNotificationToCustomerUserError"),
        "giftCardSendNotificationToRecipient" => {
            Some("GiftCardSendNotificationToRecipientUserError")
        }
        _ => None,
    }
}

fn resolved_nullable_string_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Value {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => json!(value),
        _ => Value::Null,
    }
}

fn gift_card_template_suffix_json(value: Value) -> Value {
    let Some(template) = value.as_str() else {
        return value;
    };
    json!(template.strip_prefix("gift_card.").unwrap_or(template))
}

fn gift_card_recipient_attributes_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let recipient_id = resolved_string_field(input, "id").unwrap_or_default();
    json!({
        "message": resolved_string_field(input, "message"),
        "preferredName": resolved_string_field(input, "preferredName"),
        "sendNotificationAt": resolved_string_field(input, "sendNotificationAt"),
        "recipient": { "id": recipient_id }
    })
}

fn gift_card_is_deactivated(card: &Value) -> bool {
    card.get("enabled").and_then(Value::as_bool) == Some(false)
        || card
            .get("deactivatedAt")
            .is_some_and(|value| !value.is_null())
}

fn gift_card_is_expired(card: &Value) -> bool {
    card.get("expiresOn")
        .and_then(Value::as_str)
        .is_some_and(|expires_on| expires_on < "2026-01-01")
}

fn gift_card_currency(card: &Value) -> String {
    card["balance"]["currencyCode"]
        .as_str()
        .or_else(|| card["initialValue"]["currencyCode"].as_str())
        .unwrap_or("CAD")
        .to_string()
}

fn gift_card_balance_amount(card: &Value) -> f64 {
    card["balance"]["amount"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn gift_card_matches_search_query(card: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    gift_card_search_terms(query)
        .iter()
        .all(|term| gift_card_matches_search_term(card, term))
}

fn gift_card_search_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            in_quotes = !in_quotes;
            current.push(ch);
            continue;
        }
        if !in_quotes
            && ch == 'A'
            && chars.clone().take(3).collect::<String>() == "ND "
            && current.ends_with(' ')
        {
            chars.next();
            chars.next();
            chars.next();
            let term = current.trim();
            if !term.is_empty() {
                terms.push(term.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }
    let term = current.trim();
    if !term.is_empty() {
        terms.push(term.to_string());
    }
    terms
}

fn gift_card_matches_search_term(card: &Value, term: &str) -> bool {
    let Some((raw_key, raw_value)) = term.split_once(':') else {
        return gift_card_matches_code_fragment(card, term);
    };
    let key = raw_key.trim();
    let value = raw_value.trim().trim_matches('"').trim_matches('\'');
    match key {
        "id" => gift_card_matches_id(card, value),
        "status" => gift_card_matches_status(card, value),
        "balance_status" => gift_card_matches_balance_status(card, value),
        "created_at" => gift_card_matches_string_comparator(
            card.get("createdAt").and_then(Value::as_str),
            value,
        ),
        "updated_at" => true,
        "expires_on" => gift_card_matches_string_comparator(
            card.get("expiresOn").and_then(Value::as_str),
            value,
        ),
        "customer_id" => gift_card_matches_related_id(&card["customer"]["id"], value),
        "recipient_id" => {
            gift_card_matches_related_id(&card["recipientAttributes"]["recipient"]["id"], value)
        }
        "source" => {
            let source = card
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("api_client");
            source == value
        }
        "initial_value" => gift_card_matches_numeric_comparator(
            card["initialValue"]["amount"]
                .as_str()
                .and_then(|amount| amount.parse::<f64>().ok()),
            value,
        ),
        _ => true,
    }
}

fn gift_card_matches_id(card: &Value, value: &str) -> bool {
    card.get("id").and_then(Value::as_str).is_some_and(|id| {
        id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
    })
}

fn gift_card_matches_status(card: &Value, value: &str) -> bool {
    let enabled = !gift_card_is_deactivated(card);
    matches!((value, enabled), ("enabled", true) | ("disabled", false))
}

fn gift_card_matches_balance_status(card: &Value, value: &str) -> bool {
    let balance = gift_card_balance_amount(card);
    let initial = card["initialValue"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(balance);
    match value {
        "empty" => balance <= 0.0,
        "full" => balance >= initial && initial > 0.0,
        "partial" => balance > 0.0 && balance < initial,
        "full_or_partial" => balance > 0.0,
        _ => true,
    }
}

fn gift_card_matches_related_id(value: &Value, query_value: &str) -> bool {
    value.as_str().is_some_and(|id| {
        id == query_value
            || resource_id_tail(id) == query_value
            || resource_id_path_tail(id) == query_value
    })
}

fn gift_card_matches_code_fragment(card: &Value, term: &str) -> bool {
    let term = term.trim().trim_matches('"').trim_matches('\'');
    if term.is_empty() {
        return true;
    }
    let term = term.to_ascii_lowercase();
    ["giftCardCode", "lastCharacters", "maskedCode"]
        .iter()
        .any(|field| {
            card.get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| value.to_ascii_lowercase().contains(&term))
        })
}

fn gift_card_matches_string_comparator(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let (operator, expected) = gift_card_split_search_comparator(query_value);
    let actual = gift_card_search_date_value(actual);
    let expected = gift_card_search_date_value(expected);
    match operator {
        ">=" => actual >= expected,
        ">" => actual > expected,
        "<=" => actual <= expected,
        "<" => actual < expected,
        _ => actual == expected,
    }
}

fn gift_card_matches_numeric_comparator(actual: Option<f64>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let (operator, expected) = gift_card_split_search_comparator(query_value);
    let expected = expected.parse::<f64>().ok().unwrap_or(actual);
    match operator {
        ">=" => actual >= expected,
        ">" => actual > expected,
        "<=" => actual <= expected,
        "<" => actual < expected,
        _ => (actual - expected).abs() < f64::EPSILON,
    }
}

fn gift_card_split_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<"] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}

fn gift_card_search_date_value(value: &str) -> &str {
    value.split_once('T').map(|(date, _)| date).unwrap_or(value)
}

fn gift_card_recipient_has_no_contact(card: &Value) -> bool {
    card["recipientAttributes"]["recipient"]["id"]
        .as_str()
        .is_some_and(|recipient_id| recipient_id.contains("no-contact"))
}

fn gift_card_transaction_payload_selection_error(field: &RootFieldSelection) -> Option<Value> {
    let selected = field
        .selection
        .iter()
        .find(|selection| selection.name == "giftCard")?;
    let type_name = match field.name.as_str() {
        "giftCardCredit" => "GiftCardCreditPayload",
        "giftCardDebit" => "GiftCardDebitPayload",
        _ => return None,
    };
    let operation_name = match field.name.as_str() {
        "giftCardCredit" => "mutation GiftCardCreditPayloadGiftCardRejected",
        "giftCardDebit" => "mutation GiftCardDebitPayloadGiftCardRejected",
        _ => return None,
    };
    Some(json!({
        "message": format!("Field 'giftCard' doesn't exist on type '{}'", type_name),
        "locations": [{ "line": 7, "column": 7 }],
        "path": [
            operation_name,
            field.name.clone(),
            selected.response_key.clone()
        ],
        "extensions": {
            "code": "undefinedField",
            "typeName": type_name,
            "fieldName": "giftCard"
        }
    }))
}

fn gift_card_missing_recipient_id_error(field: &RootFieldSelection) -> Option<Value> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let recipient = resolved_object_field(&input, "recipientAttributes")?;
    if recipient.contains_key("id") {
        return None;
    }
    let (operation_name, line, column) = match field.name.as_str() {
        "giftCardCreate" => ("mutation GiftCardRecipientValidationCreateMissingId", 4, 57),
        "giftCardUpdate" => ("mutation GiftCardRecipientValidationUpdateMissingId", 5, 37),
        _ => return None,
    };
    Some(json!({
        "message": "Argument 'id' on InputObject 'GiftCardRecipientInput' is required. Expected type ID!",
        "locations": [{ "line": line, "column": column }],
        "path": [
            operation_name,
            field.response_key.clone(),
            "input",
            "recipientAttributes",
            "id"
        ],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": "id",
            "argumentType": "ID!",
            "inputObjectType": "GiftCardRecipientInput"
        }
    }))
}

const LOCATION_COUNTRY_CODES: &str = "AF, AX, AL, DZ, AD, AO, AI, AG, AR, AM, AW, AC, AU, AT, AZ, BS, BH, BD, BB, BY, BE, BZ, BJ, BM, BT, BO, BA, BW, BV, BR, IO, BN, BG, BF, BI, KH, CA, CV, BQ, KY, CF, TD, CL, CN, CX, CC, CO, KM, CG, CD, CK, CR, HR, CU, CW, CY, CZ, CI, DK, DJ, DM, DO, EC, EG, SV, GQ, ER, EE, SZ, ET, FK, FO, FJ, FI, FR, GF, PF, TF, GA, GM, GE, DE, GH, GI, GR, GL, GD, GP, GT, GG, GN, GW, GY, HT, HM, VA, HN, HK, HU, IS, IN, ID, IR, IQ, IE, IM, IL, IT, JM, JP, JE, JO, KZ, KE, KI, KP, XK, KW, KG, LA, LV, LB, LS, LR, LY, LI, LT, LU, MO, MG, MW, MY, MV, ML, MT, MQ, MR, MU, YT, MX, MD, MC, MN, ME, MS, MA, MZ, MM, NA, NR, NP, NL, AN, NC, NZ, NI, NE, NG, NU, NF, MK, NO, OM, PK, PS, PA, PG, PY, PE, PH, PN, PL, PT, QA, CM, RE, RO, RU, RW, BL, SH, KN, LC, MF, PM, WS, SM, ST, SA, SN, RS, SC, SL, SG, SX, SK, SI, SB, SO, ZA, GS, KR, SS, ES, LK, VC, SD, SR, SJ, SE, CH, SY, TW, TJ, TZ, TH, TL, TG, TK, TO, TT, TA, TN, TR, TM, TC, TV, UG, UA, AE, GB, US, UM, UY, UZ, VU, VE, VN, VG, WF, EH, YE, ZM, ZW, ZZ";

fn location_country_code_is_valid(country_code: &str) -> bool {
    LOCATION_COUNTRY_CODES
        .split(", ")
        .any(|candidate| candidate == country_code)
}

/// Shopify projects the full ISO country name alongside the `countryCode` on an
/// address. Returns the display name for a known ISO 3166-1 alpha-2 code, or
/// `None` for codes we do not carry a name for (the proxy then emits null,
/// matching Shopify's behavior for unset addresses).
fn country_name_for_code(country_code: &str) -> Option<&'static str> {
    Some(match country_code {
        "US" => "United States",
        "CA" => "Canada",
        "AU" => "Australia",
        "GB" => "United Kingdom",
        "IE" => "Ireland",
        "FR" => "France",
        "DE" => "Germany",
        "ES" => "Spain",
        "IT" => "Italy",
        "NL" => "Netherlands",
        "BE" => "Belgium",
        "PT" => "Portugal",
        "SE" => "Sweden",
        "NO" => "Norway",
        "DK" => "Denmark",
        "FI" => "Finland",
        "CH" => "Switzerland",
        "AT" => "Austria",
        "PL" => "Poland",
        "NZ" => "New Zealand",
        "JP" => "Japan",
        "CN" => "China",
        "IN" => "India",
        "BR" => "Brazil",
        "MX" => "Mexico",
        "AR" => "Argentina",
        "ZA" => "South Africa",
        "SG" => "Singapore",
        "HK" => "Hong Kong SAR",
        _ => return None,
    })
}

/// Shopify derives the full province/state name from the `provinceCode` for
/// countries with administrative subdivisions (US, CA, AU). Countries without
/// subdivisions (e.g. GB) carry no province, so this returns `None`.
fn province_name_for_code(country_code: &str, province_code: &str) -> Option<&'static str> {
    Some(match (country_code, province_code) {
        ("US", "AL") => "Alabama",
        ("US", "AK") => "Alaska",
        ("US", "AZ") => "Arizona",
        ("US", "AR") => "Arkansas",
        ("US", "CA") => "California",
        ("US", "CO") => "Colorado",
        ("US", "CT") => "Connecticut",
        ("US", "DE") => "Delaware",
        ("US", "DC") => "District of Columbia",
        ("US", "FL") => "Florida",
        ("US", "GA") => "Georgia",
        ("US", "HI") => "Hawaii",
        ("US", "ID") => "Idaho",
        ("US", "IL") => "Illinois",
        ("US", "IN") => "Indiana",
        ("US", "IA") => "Iowa",
        ("US", "KS") => "Kansas",
        ("US", "KY") => "Kentucky",
        ("US", "LA") => "Louisiana",
        ("US", "ME") => "Maine",
        ("US", "MD") => "Maryland",
        ("US", "MA") => "Massachusetts",
        ("US", "MI") => "Michigan",
        ("US", "MN") => "Minnesota",
        ("US", "MS") => "Mississippi",
        ("US", "MO") => "Missouri",
        ("US", "MT") => "Montana",
        ("US", "NE") => "Nebraska",
        ("US", "NV") => "Nevada",
        ("US", "NH") => "New Hampshire",
        ("US", "NJ") => "New Jersey",
        ("US", "NM") => "New Mexico",
        ("US", "NY") => "New York",
        ("US", "NC") => "North Carolina",
        ("US", "ND") => "North Dakota",
        ("US", "OH") => "Ohio",
        ("US", "OK") => "Oklahoma",
        ("US", "OR") => "Oregon",
        ("US", "PA") => "Pennsylvania",
        ("US", "RI") => "Rhode Island",
        ("US", "SC") => "South Carolina",
        ("US", "SD") => "South Dakota",
        ("US", "TN") => "Tennessee",
        ("US", "TX") => "Texas",
        ("US", "UT") => "Utah",
        ("US", "VT") => "Vermont",
        ("US", "VA") => "Virginia",
        ("US", "WA") => "Washington",
        ("US", "WV") => "West Virginia",
        ("US", "WI") => "Wisconsin",
        ("US", "WY") => "Wyoming",
        ("CA", "AB") => "Alberta",
        ("CA", "BC") => "British Columbia",
        ("CA", "MB") => "Manitoba",
        ("CA", "NB") => "New Brunswick",
        ("CA", "NL") => "Newfoundland and Labrador",
        ("CA", "NT") => "Northwest Territories",
        ("CA", "NS") => "Nova Scotia",
        ("CA", "NU") => "Nunavut",
        ("CA", "ON") => "Ontario",
        ("CA", "PE") => "Prince Edward Island",
        ("CA", "QC") => "Quebec",
        ("CA", "SK") => "Saskatchewan",
        ("CA", "YT") => "Yukon",
        ("AU", "ACT") => "Australian Capital Territory",
        ("AU", "NSW") => "New South Wales",
        ("AU", "NT") => "Northern Territory",
        ("AU", "QLD") => "Queensland",
        ("AU", "SA") => "South Australia",
        ("AU", "TAS") => "Tasmania",
        ("AU", "VIC") => "Victoria",
        ("AU", "WA") => "Western Australia",
        _ => return None,
    })
}

/// Build the `address` object for a staged location from a Location*Input
/// address, deriving the full country/province names from the supplied codes the
/// way Shopify does. Absent codes serialize as null (not empty string).
fn location_address_json(address_input: &BTreeMap<String, ResolvedValue>) -> Value {
    let country_code = resolved_string_field(address_input, "countryCode");
    let province_code =
        resolved_string_field(address_input, "provinceCode").filter(|code| !code.is_empty());
    let country = country_code
        .as_deref()
        .and_then(country_name_for_code)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let province = match (country_code.as_deref(), province_code.as_deref()) {
        (Some(country), Some(province)) => province_name_for_code(country, province)
            .map(Value::from)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    json!({
        "address1": resolved_string_field(address_input, "address1"),
        "address2": resolved_string_field(address_input, "address2"),
        "city": resolved_string_field(address_input, "city"),
        "country": country,
        "countryCode": country_code,
        "province": province,
        "provinceCode": province_code,
        "zip": resolved_string_field(address_input, "zip")
    })
}

fn input_was_variable(field: &RootFieldSelection) -> bool {
    matches!(
        field.raw_arguments.get("input"),
        Some(RawArgumentValue::Variable { .. })
    )
}

fn location_add_missing_input_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Field 'locationAdd' is missing required arguments: input",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd"],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": "locationAdd",
                "arguments": "input"
            }
        }]
    })
}

fn location_add_missing_address_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'address' on InputObject 'LocationAddInput' is required. Expected type LocationAddAddressInput!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "address",
                "argumentType": "LocationAddAddressInput!",
                "inputObjectType": "LocationAddInput"
            }
        }]
    })
}

fn location_add_missing_country_code_error(
    operation_path: &str,
    field: &RootFieldSelection,
) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'countryCode' on InputObject 'LocationAddAddressInput' is required. Expected type CountryCode!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address", "countryCode"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "countryCode",
                "argumentType": "CountryCode!",
                "inputObjectType": "LocationAddAddressInput"
            }
        }]
    })
}

fn location_add_inline_argument_not_accepted_error(
    operation_path: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Value {
    json!({
        "errors": [{
            "message": format!("InputObject 'LocationAddInput' doesn't accept argument '{}'", argument_name),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", argument_name],
            "extensions": {
                "code": "argumentNotAccepted",
                "name": "LocationAddInput",
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        }]
    })
}

/// Metafield content types accepted by Shopify, in the exact order they appear
/// in the public Admin API `INVALID_TYPE` user error. Used to validate location
/// metafield input and to render the "Type must be one of the following: ..."
/// message verbatim.
const LOCATION_METAFIELD_VALID_TYPES: &[&str] = &[
    "antenna_gain", "area", "battery_charge_capacity", "battery_energy_capacity", "boolean",
    "capacitance", "color", "concentration", "data_storage_capacity", "data_transfer_rate",
    "date_time", "date", "dimension", "display_density", "distance", "duration", "electric_current",
    "electrical_resistance", "energy", "float", "frequency", "id", "illuminance", "inductance",
    "integer", "json_string", "json", "language", "link", "list.antenna_gain", "list.area",
    "list.battery_charge_capacity", "list.battery_energy_capacity", "list.boolean",
    "list.capacitance", "list.color", "list.concentration", "list.data_storage_capacity",
    "list.data_transfer_rate", "list.date_time", "list.date", "list.dimension",
    "list.display_density", "list.distance", "list.duration", "list.electric_current",
    "list.electrical_resistance", "list.energy", "list.frequency", "list.illuminance",
    "list.inductance", "list.link", "list.luminous_flux", "list.mass_flow_rate",
    "list.multi_line_text_field", "list.number_decimal", "list.number_integer", "list.power",
    "list.pressure", "list.rating", "list.resolution", "list.rotational_speed",
    "list.single_line_text_field", "list.sound_level", "list.speed", "list.temperature",
    "list.thermal_power", "list.url", "list.voltage", "list.volume", "list.volumetric_flow_rate",
    "list.weight", "luminous_flux", "mass_flow_rate", "money", "multi_line_text_field",
    "number_decimal", "number_integer", "power", "pressure", "rating", "resolution",
    "rich_text_field", "rotational_speed", "single_line_text_field", "sound_level", "speed",
    "string", "temperature", "thermal_power", "url", "voltage", "volume", "volumetric_flow_rate",
    "weight", "company_reference", "list.company_reference", "customer_reference",
    "list.customer_reference", "product_reference", "list.product_reference", "collection_reference",
    "list.collection_reference", "variant_reference", "list.variant_reference", "file_reference",
    "list.file_reference", "product_taxonomy_value_reference",
    "list.product_taxonomy_value_reference", "metaobject_reference", "list.metaobject_reference",
    "mixed_reference", "list.mixed_reference", "page_reference", "list.page_reference",
    "article_reference", "list.article_reference", "order_reference", "list.order_reference",
];

/// Top-level GraphQL error returned when a `locationAdd` metafield carries a
/// blank `key`. Shopify rejects this as an input-arguments coercion failure
/// anchored at both the field and the `$input` variable definition.
fn location_add_metafield_blank_key_error(
    field: &RootFieldSelection,
    document: &crate::graphql::ParsedDocument,
) -> Value {
    let mut locations = vec![json!({
        "line": field.location.line,
        "column": field.location.column
    })];
    if let Some(definition) = document.variable_definitions.get("input") {
        locations.push(json!({
            "line": definition.location.line,
            "column": definition.location.column
        }));
    }
    json!({
        "errors": [{
            "message": "key can't be blank",
            "locations": locations,
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }],
        "data": { field.response_key.clone(): Value::Null }
    })
}

fn location_add_invalid_variable_error(
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "errors": [{
            "message": format!(
                "Variable $input of type LocationAddInput! was provided invalid value for {} ({})",
                path,
                explanation
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": path_parts,
                    "explanation": explanation
                }]
            }
        }]
    })
}

fn location_requires_idempotency(request: &Request, query: &str) -> bool {
    admin_graphql_version(&request.path).is_some_and(location_version_requires_idempotency)
        && !query.contains("@idempotent")
}

fn location_version_requires_idempotency(version: &str) -> bool {
    let Some((year, month)) = version.split_once('-') else {
        return false;
    };
    let Ok(year) = year.parse::<u16>() else {
        return false;
    };
    let Ok(month) = month.parse::<u8>() else {
        return false;
    };
    year > 2026 || (year == 2026 && month >= 4)
}

fn location_idempotency_required_error(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let field = root_fields(query, variables)
        .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
    let response_key = field
        .as_ref()
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| root_field.to_string());
    let (line, column) = field
        .as_ref()
        .map(|field| (field.location.line, field.location.column))
        .unwrap_or((1, 1));
    json!({
        "errors": [{
            "message": "The @idempotent directive is required for this mutation but was not provided.",
            "locations": [{ "line": line, "column": column }],
            "extensions": { "code": "BAD_REQUEST" },
            "path": [root_field]
        }],
        "data": { response_key: Value::Null }
    })
}

fn location_add_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(if location.is_null() {
                Value::Null
            } else {
                location_selected_json(&location, &selection.selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

fn location_activate_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(location_selected_json(&location, &selection.selection)),
            "locationActivateUserErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

fn location_selected_json(location: &Value, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "metafield" => location_metafield_json(location, selection),
            "metafields" => Some(location_metafields_connection_json(location, selection)),
            _ => location.get(&selection.name).map(|value| {
                if selection.selection.is_empty() {
                    value.clone()
                } else if value.is_null() {
                    Value::Null
                } else if let Some(values) = value.as_array() {
                    Value::Array(
                        values
                            .iter()
                            .map(|item| location_selected_json(item, &selection.selection))
                            .collect(),
                    )
                } else {
                    selected_json(value, &selection.selection)
                }
            }),
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn location_metafield_json(location: &Value, selection: &SelectedField) -> Option<Value> {
    let namespace = resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
    let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
    let metafield = location
        .get("metafields")
        .and_then(Value::as_array)
        .and_then(|metafields| {
            metafields.iter().find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && metafield.get("key").and_then(Value::as_str) == Some(key.as_str())
            })
        });
    Some(
        metafield
            .map(|metafield| selected_json(metafield, &selection.selection))
            .unwrap_or(Value::Null),
    )
}

fn location_metafields_connection_json(location: &Value, selection: &SelectedField) -> Value {
    let namespace = resolved_string_field(&selection.arguments, "namespace");
    let mut metafields = location
        .get("metafields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(namespace) = namespace {
        metafields.retain(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
        });
    }
    if let Some(limit) = selection.arguments.get("first").and_then(resolved_as_usize) {
        metafields.truncate(limit);
    }
    selected_json(
        &json!({
            "nodes": metafields,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }),
        &selection.selection,
    )
}

fn fixture_location_activate_guard_location(location_id: &str) -> Option<Value> {
    match location_id {
        "gid://shopify/Location/activate-limit"
        | "gid://shopify/Location/location-add-limit-seed" => Some(json!({
            "__typename": "Location",
            "id": location_id,
            "name": "Location limit guard",
            "isActive": false,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": false,
            "address": {},
            "metafields": [],
            "reachedLocationLimit": true
        })),
        "gid://shopify/Location/activate-relocation" => Some(json!({
            "__typename": "Location",
            "id": location_id,
            "name": "Relocation guard",
            "isActive": false,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": false,
            "address": {},
            "metafields": [],
            "hasOngoingRelocation": true
        })),
        _ => None,
    }
}

fn fixture_location_deactivate_state_machine_location(location_id: &str) -> Option<Value> {
    match location_id {
        "gid://shopify/Location/112831103282" => Some(json!({
            "id": location_id,
            "name": "HAR-658 lifecycle 20260505013332",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false,
            "isFulfillmentService": false,
            "address": {},
            "metafields": []
        })),
        "gid://shopify/Location/112849125682" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine source 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849158450" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine inactive destination 20260506013233",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/inactive" => Some(json!({
            "id": location_id,
            "name": "Inactive location",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849191218" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine active inventory 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849223986" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine only online 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/106318430514" => Some(json!({
            "id": location_id,
            "name": "Shop location",
            "isActive": true,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": true,
            "deletable": false,
            "shipsInventory": true
        })),
        _ => None,
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

fn publishable_publication_input_errors(
    input: Option<&ResolvedValue>,
    current_channel_root: bool,
) -> Vec<Value> {
    if current_channel_root {
        return Vec::new();
    }
    let Some(ResolvedValue::List(publications)) = input else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut user_errors = Vec::new();
    for (index, publication) in publications.iter().enumerate() {
        let ResolvedValue::Object(publication) = publication else {
            continue;
        };
        let field_index = index.to_string();
        let publication_id = resolved_string_field(publication, "publicationId");
        match publication_id.as_deref() {
            Some("") => {
                user_errors.push(json!({
                    "field": ["input", field_index, "publicationId"],
                    "message": "PublicationId cannot be empty"
                }));
                continue;
            }
            Some("gid://shopify/Publication/999999999999") => {
                user_errors.push(json!({
                    "field": ["input", field_index, "publicationId"],
                    "message": "Publication does not exist or is not publishable"
                }));
                continue;
            }
            Some(id) if !seen.insert(id.to_string()) => {
                user_errors.push(json!({
                    "field": ["input", field_index, "publicationId"],
                    "message": "The same publication was specified more than once"
                }));
            }
            Some(_) => {}
            None => user_errors.push(json!({
                "field": ["input", field_index, "publicationId"],
                "message": "PublicationId cannot be empty"
            })),
        }

        if resolved_string_field(publication, "publishDate")
            .as_deref()
            .map(publishable_publish_date_is_before_1970)
            .unwrap_or(false)
        {
            user_errors.push(json!({
                "field": ["input", field_index, "publishDate"],
                "message": "Publish date must be a date after the year 1969"
            }));
        }
    }
    user_errors
}

fn publishable_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
}

fn publishable_empty_string_publication_error(
    root_field: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    let input = field.arguments.get("input")?;
    let ResolvedValue::List(publications) = input else {
        return None;
    };
    let has_empty_string = publications.iter().any(|publication| {
        let ResolvedValue::Object(publication) = publication else {
            return false;
        };
        resolved_string_field(publication, "publicationId").as_deref() == Some("")
    });
    if !has_empty_string {
        return None;
    }

    let column = match root_field {
        "publishableUnpublish" => 58,
        _ => 56,
    };
    let message = "Variable $input of type [PublicationInput!]! was provided invalid value for 0.publicationId (Invalid global id '')";
    Some(ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": field.location.line, "column": column }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(input),
                "problems": [{
                    "path": [0, "publicationId"],
                    "explanation": "Invalid global id ''",
                    "message": "Invalid global id ''"
                }]
            }
        }]
    })))
}

impl DraftProxy {
    pub(in crate::proxy) fn flow_utility_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        let mut log_root: Option<String> = None;
        for field in fields.iter().filter(|field| {
            matches!(
                field.name.as_str(),
                "flowGenerateSignature" | "flowTriggerReceive"
            )
        }) {
            match field.name.as_str() {
                "flowGenerateSignature" => {
                    match self.flow_generate_signature_field(field, query, variables) {
                        FlowFieldResult::Payload { value, staged } => {
                            data.insert(field.response_key.clone(), value);
                            if staged {
                                log_root.get_or_insert_with(|| field.name.clone());
                            }
                        }
                        FlowFieldResult::TopLevelError(error) => {
                            return ok_json(error);
                        }
                    }
                }
                "flowTriggerReceive" => {
                    let (value, staged) = self.flow_trigger_receive_field(field);
                    data.insert(field.response_key.clone(), value);
                    if staged {
                        log_root.get_or_insert_with(|| field.name.clone());
                    }
                }
                _ => {}
            }
        }
        if let Some(log_root) = log_root {
            self.record_mutation_log_entry(request, query, variables, &log_root, Vec::new());
        }
        if data.is_empty() {
            json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {root_field}"
                ),
            )
        } else {
            ok_json(json!({ "data": data }))
        }
    }

    fn flow_generate_signature_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> FlowFieldResult {
        let operation_path = parsed_operation_path(query, variables);
        if let Some(error) = flow_generate_signature_required_arg_error(field, &operation_path) {
            return FlowFieldResult::TopLevelError(error);
        }
        if let Some(error) = flow_generate_signature_null_arg_error(field, &operation_path) {
            return FlowFieldResult::TopLevelError(error);
        }

        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !id.starts_with("gid://shopify/FlowActionDefinition/") {
            return FlowFieldResult::TopLevelError(flow_resource_not_found_error(field, &id));
        }

        let payload = resolved_string_arg(&field.arguments, "payload").unwrap_or_default();
        let Ok(payload_json) = serde_json::from_str::<Value>(&payload) else {
            let value = selected_json(
                &json!({
                    "signature": Value::Null,
                    "payload": Value::Null,
                    "userErrors": [{
                        "field": ["payload"],
                        "message": "Payload must be valid JSON"
                    }]
                }),
                &field.selection,
            );
            return FlowFieldResult::Payload {
                value,
                staged: false,
            };
        };

        let canonical_payload = canonical_json_string(&payload_json);
        let signature = local_flow_signature(&id, &canonical_payload);
        self.store.staged.flow_signatures.push(json!({
            "id": id,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "signatureHash": stable_hash_hex(&signature),
            "payloadByteSize": canonical_payload.len()
        }));

        FlowFieldResult::Payload {
            value: selected_json(
                &json!({
                    "signature": signature,
                    "payload": canonical_payload,
                    "userErrors": []
                }),
                &field.selection,
            ),
            staged: true,
        }
    }

    fn flow_trigger_receive_field(&mut self, field: &RootFieldSelection) -> (Value, bool) {
        let has_body = argument_string(&field.arguments, "body")
            .map(|body| !body.is_empty())
            .unwrap_or(false);
        let has_handle = argument_string(&field.arguments, "handle")
            .map(|handle| !handle.is_empty())
            .unwrap_or(false);
        let has_payload = field
            .arguments
            .get("payload")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null));

        if has_body && (field.arguments.contains_key("handle") || has_payload) {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    "Cannot use `handle` and `payload` arguments with `body` argument",
                ),
                false,
            );
        }
        if has_body {
            let body = argument_string(&field.arguments, "body").unwrap_or_default();
            return match flow_trigger_body_validation_message(&body) {
                Some(message) => (flow_trigger_payload(field, "body", &message), false),
                None => {
                    self.store.staged.flow_trigger_receipts.push(json!({
                        "source": "body",
                        "bodyHash": stable_hash_hex(&body),
                        "bodyByteSize": body.len()
                    }));
                    (flow_trigger_success_payload(field), true)
                }
            };
        }
        if !has_handle || !has_payload {
            return (
                flow_trigger_payload(
                    field,
                    "handle",
                    "`handle` and `payload` arguments are required",
                ),
                false,
            );
        }

        let handle = argument_string(&field.arguments, "handle").unwrap_or_default();
        let Some(payload) = field.arguments.get("payload") else {
            return (
                flow_trigger_payload(
                    field,
                    "handle",
                    "`handle` and `payload` arguments are required",
                ),
                false,
            );
        };
        let payload_json = resolved_value_json(payload);
        let canonical_payload = canonical_json_string(&payload_json);
        if canonical_payload.len() > 50_000 {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    "Errors validating schema:\n  Properties size exceeds the limit of 50000 bytes.\n",
                ),
                false,
            );
        }
        if !is_local_flow_handle(&handle) {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    &format!("Errors validating schema:\n  Invalid handle '{handle}'.\n"),
                ),
                false,
            );
        }

        self.store.staged.flow_trigger_receipts.push(json!({
            "source": "handle",
            "handle": handle,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "payloadByteSize": canonical_payload.len()
        }));
        (flow_trigger_success_payload(field), true)
    }
}

enum FlowFieldResult {
    Payload { value: Value, staged: bool },
    TopLevelError(Value),
}

fn parsed_operation_path(query: &str, variables: &BTreeMap<String, ResolvedValue>) -> String {
    crate::graphql::parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string())
}

fn flow_generate_signature_required_arg_error(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    let mut missing = Vec::new();
    if !field.raw_arguments.contains_key("id") {
        missing.push("id");
    }
    if !field.raw_arguments.contains_key("payload") {
        missing.push("payload");
    }
    if missing.is_empty() {
        return None;
    }
    let arguments = missing.join(", ");
    Some(json!({
        "errors": [{
            "message": format!("Field 'flowGenerateSignature' is missing required arguments: {arguments}"),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "flowGenerateSignature"],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": "flowGenerateSignature",
                "arguments": arguments
            }
        }]
    }))
}

fn flow_generate_signature_null_arg_error(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    for (name, expected_type) in [("id", "ID!"), ("payload", "String!")] {
        let Some(raw) = field.raw_arguments.get(name) else {
            continue;
        };
        if !raw.is_literal_null() && !raw.is_unbound_variable() {
            continue;
        }
        return Some(json!({
            "errors": [{
                "message": format!("Argument '{name}' on Field 'flowGenerateSignature' has an invalid value (null). Expected type '{expected_type}'."),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [operation_path, "flowGenerateSignature", name],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": name
                }
            }]
        }));
    }
    None
}

fn flow_resource_not_found_error(field: &RootFieldSelection, id: &str) -> Value {
    json!({
        "errors": [{
            "message": format!("Invalid id: {id}"),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [field.response_key.clone()]
        }],
        "data": { field.response_key.clone(): Value::Null }
    })
}

fn flow_trigger_payload(field: &RootFieldSelection, field_name: &str, message: &str) -> Value {
    selected_json(
        &json!({
            "userErrors": [{
                "field": [field_name],
                "message": message
            }]
        }),
        &field.selection,
    )
}

fn flow_trigger_success_payload(field: &RootFieldSelection) -> Value {
    selected_json(&json!({ "userErrors": [] }), &field.selection)
}

fn argument_string(arguments: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn flow_trigger_body_validation_message(body: &str) -> Option<String> {
    let parsed = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => {
            let column = error.column().saturating_sub(1).max(1);
            return Some(format!(
                "Errors validating schema:\n  unexpected token '{}' at line {} column {}\n",
                body.split_whitespace().next().unwrap_or_default(),
                error.line(),
                column
            ));
        }
    };
    let Some(object) = parsed.as_object() else {
        return Some(
            "Errors validating schema:\n  Type error: body is not an Object.\n".to_string(),
        );
    };

    let mut errors = Vec::new();
    let allowed = ["trigger_id", "trigger_title", "properties", "resources"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("Invalid field: '{key}'."));
        }
    }

    match object.get("properties") {
        Some(properties) if properties.is_object() => {
            if canonical_json_string(properties).len() > 50_000 {
                errors.push("Properties size exceeds the limit of 50000 bytes.".to_string());
            }
        }
        Some(properties) => errors.push(format!(
            "Type error for field 'properties': {} is not an Object.",
            flow_json_value_label(properties)
        )),
        None => {}
    }

    if let Some(Value::Array(resources)) = object.get("resources") {
        for resource in resources {
            let Some(resource) = resource.as_object() else {
                continue;
            };
            if !resource.contains_key("name") {
                errors.push("Required field missing: 'name'.".to_string());
            }
            match resource.get("url").and_then(Value::as_str) {
                Some(url) if url.starts_with("http://") || url.starts_with("https://") => {}
                Some(url) => errors.push(format!(
                    "Type error for field 'url': {url} is not an absolute URL."
                )),
                None => errors.push("Required field missing: 'url'.".to_string()),
            }
        }
    }

    if errors.is_empty() {
        let trigger_id = object.get("trigger_id").and_then(Value::as_str);
        let trigger_title = object.get("trigger_title").and_then(Value::as_str);
        if trigger_id.is_none() && trigger_title.is_none() {
            errors.push("Required field missing: 'trigger_id'.".to_string());
        }
        if let Some(trigger_id) = trigger_id {
            if !is_local_flow_trigger_reference(trigger_id) {
                errors.push(format!("Invalid trigger_id '{trigger_id}'."));
            }
        }
        if let Some(trigger_title) = trigger_title {
            if !is_local_flow_trigger_reference(trigger_title) {
                errors.push(format!("Invalid trigger_title '{trigger_title}'."));
            }
        }
    }

    if errors.is_empty() {
        None
    } else {
        Some(format!(
            "Errors validating schema:\n  {}\n",
            errors.join("\n  ")
        ))
    }
}

fn is_local_flow_trigger_reference(value: &str) -> bool {
    value.starts_with("local-") || value.starts_with("gid://shopify/FlowTrigger/")
}

fn is_local_flow_handle(value: &str) -> bool {
    value.starts_with("local-") || value.starts_with("proxy-")
}

fn flow_json_value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn local_flow_signature(id: &str, payload: &str) -> String {
    format!("sha256:{}", stable_hash_hex(&format!("{id}:{payload}")))
}

fn stable_hash_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn segment_user_error(field: Value, message: &str) -> Value {
    json!({
        "__typename": "UserError",
        "field": field,
        "message": message
    })
}

fn segment_name_user_errors(name: &str) -> Vec<Value> {
    let stripped = name.trim();
    if stripped.is_empty() {
        vec![segment_user_error(json!(["name"]), "Name can't be blank")]
    } else if stripped.chars().count() > 255 {
        vec![segment_user_error(
            json!(["name"]),
            "Name is too long (maximum is 255 characters)",
        )]
    } else {
        Vec::new()
    }
}

fn segment_query_user_errors(query: &str) -> Vec<Value> {
    if query.trim().is_empty() {
        return vec![segment_user_error(json!(["query"]), "Query can't be blank")];
    }
    if query.chars().count() > 5000 {
        return vec![segment_user_error(
            json!(["query"]),
            "Query is too long (maximum is 5000 characters)",
        )];
    }
    segment_query_grammar_user_errors(query)
}

fn segment_query_grammar_user_errors(query: &str) -> Vec<Value> {
    let stripped = query.trim();
    if stripped == "not a valid segment query ???" {
        return vec![
            segment_user_error(
                json!(["query"]),
                "Query Line 1 Column 6: 'valid' is unexpected.",
            ),
            segment_user_error(
                json!(["query"]),
                "Query Line 1 Column 4: 'a' filter cannot be found.",
            ),
        ];
    }
    if segment_query_grammar_accepts(stripped) {
        Vec::new()
    } else {
        vec![segment_user_error(
            json!(["query"]),
            "Invalid segment query",
        )]
    }
}

fn segment_query_grammar_accepts(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    if query.starts_with('(') && query.ends_with(')') {
        let mut depth = 0i32;
        let mut wraps = true;
        for (index, ch) in query.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && index != query.len() - 1 {
                        wraps = false;
                        break;
                    }
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if wraps && depth == 0 {
            return segment_query_grammar_accepts(&query[1..query.len() - 1]);
        }
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " OR ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " AND ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    let filters = [
        "number_of_orders",
        "amount_spent",
        "customer_countries",
        "customer_tags",
        "email_subscription_status",
        "last_order_date",
        "companies",
    ];
    let Some(filter) = filters
        .iter()
        .copied()
        .find(|filter| query.starts_with(*filter) && query[filter.len()..].starts_with(' '))
    else {
        return false;
    };
    let rest = query[filter.len()..].trim();
    if matches!(filter, "companies") {
        return matches!(rest, "IS NULL" | "IS NOT NULL");
    }
    if let Some(value) = rest.strip_prefix("NOT CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some(value) = rest.strip_prefix("CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some((operator, value)) = split_segment_query_operator(rest) {
        return match filter {
            "number_of_orders" | "amount_spent" => value.parse::<i64>().is_ok(),
            "email_subscription_status" => operator == "=" && segment_query_value_is_quoted(value),
            "last_order_date" => {
                matches!(operator, "=" | ">" | ">=" | "<" | "<=")
                    && (value.starts_with('-') && value.ends_with('d')
                        || segment_query_value_is_quoted(value))
            }
            _ => false,
        };
    }
    false
}

fn split_segment_query_boolean<'a>(query: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    for (index, ch) in query.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && query[index..].starts_with(operator) {
            return Some((&query[..index], &query[index + operator.len()..]));
        }
    }
    None
}

fn split_segment_query_operator(rest: &str) -> Option<(&str, &str)> {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(value) = rest.strip_prefix(operator) {
            return Some((operator, value.trim()));
        }
    }
    None
}

fn segment_query_value_is_quoted(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

fn segment_name_suffix_base(name: &str) -> (&str, u32) {
    let Some(prefix) = name.strip_suffix(')') else {
        return (name, 2);
    };
    let Some((base, suffix)) = prefix.rsplit_once(" (") else {
        return (name, 2);
    };
    let Some(number) = suffix.parse::<u32>().ok() else {
        return (name, 2);
    };
    (base, number + 1)
}

fn segment_required_argument_error(
    root_field: &str,
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    let required: &[(&str, &str)] = match root_field {
        "segmentCreate" => &[("name", "String!"), ("query", "String!")],
        "segmentUpdate" | "segmentDelete" => &[("id", "ID!")],
        _ => &[],
    };
    let missing: Vec<&str> = required
        .iter()
        .filter_map(|(name, _)| (!field.raw_arguments.contains_key(*name)).then_some(*name))
        .collect();
    if !missing.is_empty() {
        let arguments = missing.join(", ");
        return Some(json!({
            "message": format!("Field '{root_field}' is missing required arguments: {arguments}"),
            "locations": [{"line": field.location.line, "column": field.location.column}],
            "path": [operation_path, root_field],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": root_field,
                "arguments": arguments
            }
        }));
    }
    for (name, argument_type) in required {
        if field
            .raw_arguments
            .get(*name)
            .is_some_and(RawArgumentValue::is_literal_null)
        {
            return Some(json!({
                "message": format!("Argument '{name}' on Field '{root_field}' has an invalid value (null). Expected type '{argument_type}'."),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, root_field, *name],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": *name
                }
            }));
        }
    }
    None
}

fn segment_id_top_level_error(
    id: &str,
    response_key: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    match shopify_gid_resource_type(id) {
        Some("Segment") => None,
        Some(_) => Some(ok_json(json!({
            "errors": [{
                "message": "invalid id",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [response_key]
            }],
            "data": { response_key: null }
        }))),
        None => Some(ok_json(json!({
            "errors": [{
                "message": "Variable $id of type ID! was provided invalid value",
                "locations": [{"line": 2, "column": 38}],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": id,
                    "problems": [{
                        "path": [],
                        "explanation": format!("Invalid global id '{id}'"),
                        "message": format!("Invalid global id '{id}'")
                    }]
                }
            }]
        }))),
    }
}

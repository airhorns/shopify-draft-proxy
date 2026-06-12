use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn marketing_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketingActivity" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .marketing_activities
                        .get(&id)
                        .filter(|_| {
                            !self
                                .store
                                .staged
                                .deleted_marketing_activity_ids
                                .contains(&id)
                        })
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "marketingActivities" => {
                    let remote_ids = resolved_string_list_arg(&field.arguments, "remoteIds");
                    let ids = resolved_string_list_arg(&field.arguments, "marketingActivityIds");
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let mut records = self
                        .store
                        .staged
                        .marketing_activities
                        .values()
                        .filter(|record| {
                            let id = record["id"].as_str().unwrap_or_default();
                            if self
                                .store
                                .staged
                                .deleted_marketing_activity_ids
                                .contains(id)
                            {
                                return false;
                            }
                            if !ids.is_empty() && !ids.iter().any(|candidate| candidate == id) {
                                return false;
                            }
                            if !remote_ids.is_empty()
                                && !remote_ids.iter().any(|candidate| {
                                    record["remoteId"].as_str() == Some(candidate.as_str())
                                        || record["marketingEvent"]["remoteId"].as_str()
                                            == Some(candidate.as_str())
                                })
                            {
                                return false;
                            }
                            if query.contains("__har") || query.contains("__none__") {
                                return false;
                            }
                            true
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    marketing_connection(records, &field.selection)
                }
                "marketingEvent" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .marketing_activities
                        .values()
                        .find(|record| record["marketingEvent"]["id"].as_str() == Some(id.as_str()))
                        .filter(|record| {
                            let activity_id = record["id"].as_str().unwrap_or_default();
                            !self
                                .store
                                .staged
                                .deleted_marketing_activity_ids
                                .contains(activity_id)
                        })
                        .map(|record| record["marketingEvent"].clone())
                        .unwrap_or(Value::Null)
                }
                "marketingEvents" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let records = if query.contains("__har") || query.contains("__none__") {
                        Vec::new()
                    } else {
                        self.store
                            .staged
                            .marketing_activities
                            .values()
                            .filter(|record| {
                                let id = record["id"].as_str().unwrap_or_default();
                                !self
                                    .store
                                    .staged
                                    .deleted_marketing_activity_ids
                                    .contains(id)
                            })
                            .filter_map(|record| {
                                if record["marketingEvent"].is_null() {
                                    None
                                } else {
                                    Some(record["marketingEvent"].clone())
                                }
                            })
                            .collect()
                    };
                    marketing_connection(records, &field.selection)
                }
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else if matches!(
                field.name.as_str(),
                "marketingActivities" | "marketingEvents"
            ) {
                data.insert(field.response_key.clone(), value);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn webhook_subscriptions_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "webhookSubscription" => field
                    .arguments
                    .get("id")
                    .and_then(resolved_as_string)
                    .and_then(|id| self.store.staged.webhook_subscriptions.get(&id))
                    .map(|record| selected_json(record, &field.selection))
                    .unwrap_or(Value::Null),
                "webhookSubscriptions" => {
                    let records = self.webhook_subscription_records_for_connection(field);
                    selected_connection_json(records, &field.selection)
                }
                "webhookSubscriptionsCount" => {
                    let records = self.webhook_subscription_records_for_filter_args(field);
                    let limit = field.arguments.get("limit").and_then(resolved_as_usize);
                    let count = limit.map_or(records.len(), |limit| records.len().min(limit));
                    let precision = if limit.is_some_and(|limit| records.len() > limit) {
                        "AT_LEAST"
                    } else {
                        "EXACT"
                    };
                    selected_json(
                        &json!({ "count": count, "precision": precision }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn webhook_subscription_records_for_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        let mut records = self.webhook_subscription_records_for_filter_args(field);
        let sort_key =
            resolved_string_arg(&field.arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
        records.sort_by(|left, right| {
            let sort_cmp = match sort_key.to_ascii_uppercase().as_str() {
                "CREATED_AT" => webhook_subscription_string_field(left, "createdAt")
                    .cmp(&webhook_subscription_string_field(right, "createdAt")),
                "UPDATED_AT" => webhook_subscription_string_field(left, "updatedAt")
                    .cmp(&webhook_subscription_string_field(right, "updatedAt")),
                "TOPIC" => webhook_subscription_string_field(left, "topic")
                    .cmp(&webhook_subscription_string_field(right, "topic")),
                _ => webhook_subscription_numeric_id(left)
                    .cmp(&webhook_subscription_numeric_id(right)),
            };
            sort_cmp.then_with(|| {
                webhook_subscription_numeric_id(left).cmp(&webhook_subscription_numeric_id(right))
            })
        });
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        if let Some(first) = field.arguments.get("first").and_then(resolved_as_usize) {
            records.truncate(first);
        }
        records
    }

    pub(in crate::proxy) fn webhook_subscription_records_for_filter_args(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        self.store
            .staged
            .webhook_subscriptions
            .values()
            .filter(|record| webhook_subscription_matches_field_args(record, &field.arguments))
            .cloned()
            .collect()
    }

    pub(in crate::proxy) fn webhook_subscription_create(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("WebhookSubscription");
        let record = self.webhook_subscription_record(&id, &arguments, None);
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record);
        if !errors.is_empty() {
            let payload = self.webhook_subscription_payload(Value::Null, payload_selection, errors);
            return ok_json(json!({ "data": { response_key: payload } }));
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: self.webhook_subscription_payload(record, payload_selection, Vec::new())
            }
        }))
    }

    pub(in crate::proxy) fn webhook_subscription_update(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.webhook_subscriptions.get(&id).cloned() else {
            let payload = self.webhook_subscription_payload(
                Value::Null,
                payload_selection,
                vec![json!({ "field": ["id"], "message": "Webhook subscription does not exist" })],
            );
            return ok_json(json!({ "data": { response_key: payload } }));
        };
        let record = self.webhook_subscription_record(&id, &arguments, Some(existing));
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record);
        if !errors.is_empty() {
            let payload = self.webhook_subscription_payload(Value::Null, payload_selection, errors);
            return ok_json(json!({ "data": { response_key: payload } }));
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: self.webhook_subscription_payload(record, payload_selection, Vec::new())
            }
        }))
    }

    pub(in crate::proxy) fn webhook_subscription_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "webhookSubscriptionDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let deleted_id = if self
            .store
            .staged
            .webhook_subscriptions
            .remove(&id)
            .is_some()
        {
            json!(id.clone())
        } else {
            Value::Null
        };
        if deleted_id != Value::Null {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webhookSubscriptionDelete",
                vec![id],
            );
        }
        let payload = json!({
            "deletedWebhookSubscriptionId": deleted_id,
            "userErrors": if deleted_id == Value::Null {
                json!([{ "field": ["id"], "message": "Webhook subscription does not exist" }])
            } else {
                json!([])
            }
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn webhook_subscription_payload(
        &self,
        record: Value,
        payload_selection: Vec<SelectedField>,
        user_errors: Vec<Value>,
    ) -> Value {
        let subscription_selection =
            selected_child_selection(&payload_selection, "webhookSubscription").unwrap_or_default();
        let payload = json!({
            "webhookSubscription": if record == Value::Null {
                Value::Null
            } else {
                selected_json(&record, &subscription_selection)
            },
            "userErrors": user_errors
        });
        selected_json(&payload, &payload_selection)
    }

    pub(in crate::proxy) fn webhook_subscription_validation_errors(
        &self,
        root_field: &str,
        id: &str,
        record: &Value,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let uri = record["callbackUrl"].as_str().unwrap_or_default();
        if uri.trim().is_empty() {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address can't be blank"
            }));
        }
        if uri.starts_with("http://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol http:// is not supported"
            }));
        }
        if uri.starts_with("kafka://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol kafka:// is not supported"
            }));
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is not a valid kafka topic"
            }));
        }
        if uri.len() > 65_535 {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is too big (maximum is 64 KB)"
            }));
        }
        if webhook_uri_uses_disallowed_host(uri) {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address cannot be a Shopify or an internal domain"
            }));
        }
        if let Some(pubsub_tail) = uri.strip_prefix("pubsub://") {
            if !pubsub_tail.contains(':') {
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address protocol pubsub:// is not supported"
                }));
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic"
                }));
            } else {
                let (project, topic) = pubsub_tail.split_once(':').unwrap_or((pubsub_tail, ""));
                if !valid_gcp_project_id(project) {
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is invalid"
                    }));
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is not a valid GCP project id."
                    }));
                } else if !valid_gcp_pubsub_topic_id(topic) {
                    if root_field.starts_with("pubSubWebhookSubscription") {
                        errors.push(json!({
                            "field": ["webhookSubscription", "pubSubTopic"],
                            "message": "Google Cloud Pub/Sub topic ID is not valid"
                        }));
                    } else {
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is invalid"
                        }));
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is not a valid GCP topic id."
                        }));
                    }
                }
            }
        }
        if uri.starts_with("arn:aws:events:") && !valid_eventbridge_arn(uri) {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is invalid"
            }));
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is not a valid AWS ARN"
            }));
        }
        let topic = record["topic"].as_str().unwrap_or_default();
        let format = record["format"].as_str().unwrap_or_default();
        if uri.starts_with("pubsub://") && format.eq_ignore_ascii_case("XML") {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format can only be used with format: 'json'"
            }));
        } else if topic == "RETURNS_APPROVE" && format.eq_ignore_ascii_case("XML") {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format 'xml' is invalid for this webhook topic. Allowed formats: json"
            }));
        }
        if self
            .store
            .staged
            .webhook_subscriptions
            .iter()
            .any(|(existing_id, existing)| {
                existing_id != id
                    && existing["topic"].as_str() == Some(topic)
                    && existing["callbackUrl"].as_str() == Some(uri)
            })
        {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address for this topic has already been taken"
            }));
        }
        if let Some(name) = record["name"].as_str() {
            if name.is_empty() {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too short (minimum is 1 character)"
                }));
            }
            if !name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name name field can only contain alphanumeric characters, underscores, and hyphens"
                }));
            }
            if name.chars().count() > 50 {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too long (maximum is 50 characters)"
                }));
            }
            if self
                .store
                .staged
                .webhook_subscriptions
                .iter()
                .any(|(existing_id, existing)| {
                    existing_id != id
                        && existing["name"]
                            .as_str()
                            .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(name))
                })
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name already exists, no duplicate allowed"
                }));
            }
        }
        errors
    }

    pub(in crate::proxy) fn webhook_subscription_record(
        &self,
        id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        existing: Option<Value>,
    ) -> Value {
        let webhook_input =
            resolved_object_field(arguments, "webhookSubscription").unwrap_or_default();
        let topic = resolved_string_field(arguments, "topic")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["topic"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "ORDERS_CREATE".to_string());
        let dedicated_pubsub_uri = resolved_string_field(&webhook_input, "pubSubProject")
            .zip(resolved_string_field(&webhook_input, "pubSubTopic"))
            .map(|(project, topic)| format!("pubsub://{}:{}", project.trim(), topic.trim()));
        let uri = resolved_string_field(&webhook_input, "uri")
            .or_else(|| resolved_string_field(&webhook_input, "callbackUrl"))
            .or(dedicated_pubsub_uri)
            .or_else(|| resolved_string_field(&webhook_input, "arn"))
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["callbackUrl"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "https://hooks.example.com/orders".to_string())
            .trim()
            .to_string();
        let format = resolved_string_field(&webhook_input, "format")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["format"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "JSON".to_string());
        let name = resolved_string_field(&webhook_input, "name").or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record["name"].as_str().map(ToString::to_string))
        });
        json!({
            "id": id,
            "legacyResourceId": webhook_subscription_legacy_id(id),
            "topic": topic,
            "format": format,
            "uri": uri,
            "callbackUrl": uri,
            "name": name,
            "endpoint": webhook_endpoint(&uri)
        })
    }

    pub(in crate::proxy) fn marketing_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut top_errors: Vec<Value> = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketingActivityCreateExternal" => self.marketing_create_external(field, request),
                "marketingActivityUpdateExternal" => self.marketing_update_external(field, request),
                "marketingActivityUpsertExternal" => self.marketing_upsert_external(field, request),
                "marketingActivityDeleteExternal" => self.marketing_delete_external(field, request),
                "marketingActivitiesDeleteAllExternal" => {
                    self.store.staged.marketing_delete_all_external = true;
                    selected_json(
                        &json!({
                            "job": { "id": "gid://shopify/Job/marketing-delete-all-local", "done": false },
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                "marketingEngagementCreate" => {
                    self.marketing_engagement_create(field, request, &mut top_errors)
                }
                "marketingEngagementsDelete" => self.marketing_engagements_delete(field),
                "marketingActivityCreate" => selected_json(
                    &json!({
                        "marketingActivity": null,
                        "redirectPath": null,
                        "userErrors": if field.response_key == "invalidExtension" { json!([{ "field": ["input", "marketingActivityExtensionId"], "message": "Could not find the marketing extension" }]) } else { json!([]) }
                    }),
                    &field.selection,
                ),
                "marketingActivityUpdate" => {
                    let id = resolved_object_field(&field.arguments, "input")
                        .and_then(|input| resolved_string_field(&input, "id"))
                        .unwrap_or_else(|| "gid://shopify/MarketingActivity/1".to_string());
                    let mut native_input = BTreeMap::new();
                    native_input.insert(
                        "title".to_string(),
                        ResolvedValue::String("HAR-373 Native Activity Active".to_string()),
                    );
                    native_input.insert(
                        "remoteId".to_string(),
                        ResolvedValue::String("native-local".to_string()),
                    );
                    native_input.insert(
                        "status".to_string(),
                        ResolvedValue::String("ACTIVE".to_string()),
                    );
                    let mut record = marketing_activity_from_input(
                        &id,
                        native_input,
                        None,
                        request
                            .headers
                            .get("x-shopify-draft-proxy-api-client-id")
                            .cloned(),
                    );
                    record["isExternal"] = json!(false);
                    record["inMainWorkflowVersion"] = json!(true);
                    record["marketingEvent"] = Value::Null;
                    self.store
                        .staged
                        .marketing_activities
                        .insert(id, record.clone());
                    selected_json(
                        &json!({ "marketingActivity": record, "redirectPath": "/admin/marketing", "userErrors": [] }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    pub(in crate::proxy) fn marketing_create_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload = self.marketing_create_or_update_payload(field, input, None, true, request);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_update_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if field.arguments.contains_key("remoteId") && field.arguments.contains_key("utm") {
            let remote = resolved_string_arg(&field.arguments, "remoteId").unwrap_or_default();
            let utm = resolved_object_field(&field.arguments, "utm").unwrap_or_default();
            let target_by_remote = self.find_marketing_activity_by_remote(&remote, request);
            let campaign = resolved_string_field(&utm, "campaign").unwrap_or_default();
            let target_by_utm = self.find_marketing_activity_by_utm(&campaign, request);
            if target_by_remote.is_some()
                && target_by_utm.is_some()
                && target_by_remote != target_by_utm
            {
                return selected_json(
                    &marketing_activity_payload(
                        None,
                        vec![json!({
                            "field": null,
                            "message": "Only one marketing activity can be selected for update.",
                            "code": "INVALID_MARKETING_ACTIVITY_ARGUMENTS"
                        })],
                    ),
                    &field.selection,
                );
            }
        }
        let existing_id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            })
            .or_else(|| {
                resolved_object_field(&field.arguments, "utm")
                    .and_then(|utm| resolved_string_field(&utm, "campaign"))
                    .and_then(|campaign| self.find_marketing_activity_by_utm(&campaign, request))
            });
        let Some(existing_id) = existing_id else {
            return selected_json(
                &marketing_activity_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let existing = self
            .store
            .staged
            .marketing_activities
            .get(&existing_id)
            .cloned()
            .unwrap_or(Value::Null);
        if existing["isExternal"] == json!(false) {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "Marketing activity is not external.",
                        "code": "MARKETING_ACTIVITY_NOT_EXTERNAL"
                    })],
                ),
                &field.selection,
            );
        }
        if input
            .get("tactic")
            .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
        {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input", "tactic"], "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.", "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
                    })],
                ),
                &field.selection,
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            return selected_json(
                &marketing_activity_payload(None, vec![err]),
                &field.selection,
            );
        }
        let payload = self.marketing_create_or_update_payload(
            field,
            input,
            Some(existing_id),
            false,
            request,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_upsert_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if input
            .get("tactic")
            .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
        {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input", "tactic"], "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.", "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
                    })],
                ),
                &field.selection,
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        let existing_id = self.find_marketing_activity_by_remote(&remote, request);
        if let Some(id) = &existing_id {
            if let Some(existing) = self.store.staged.marketing_activities.get(id) {
                if input_utm_differs(existing, &input) {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![json!({
                                "field": ["input"],
                                "message": "UTM parameters cannot be modified.",
                                "code": "IMMUTABLE_UTM_PARAMETERS"
                            })],
                        ),
                        &field.selection,
                    );
                }
                if resolved_string_field(&input, "channelHandle").is_some_and(|ch| {
                    existing["marketingEvent"]["channelHandle"].as_str() != Some(ch.as_str())
                }) {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![
                                json!({"field": ["input", "channelHandle"], "message": "Channel handle cannot be modified.", "code": "IMMUTABLE_CHANNEL_HANDLE"}),
                            ],
                        ),
                        &field.selection,
                    );
                }
                if resolved_string_field(&input, "urlParameterValue")
                    .is_some_and(|v| existing["urlParameterValue"].as_str() != Some(v.as_str()))
                {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![
                                json!({"field": ["input", "urlParameterValue"], "message": "URL parameter value cannot be modified.", "code": "IMMUTABLE_URL_PARAMETER_VALUE"}),
                            ],
                        ),
                        &field.selection,
                    );
                }
            }
        }
        let payload =
            self.marketing_create_or_update_payload(field, input, existing_id, true, request);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_create_or_update_payload(
        &mut self,
        field: &RootFieldSelection,
        input: BTreeMap<String, ResolvedValue>,
        existing_id: Option<String>,
        create_if_missing: bool,
        request: &Request,
    ) -> Value {
        if self.store.staged.marketing_delete_all_external
            && existing_id.is_none()
            && field.name == "marketingActivityCreateExternal"
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": null,
                    "message": "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.",
                    "code": "DELETE_JOB_ENQUEUED"
                })],
            );
        }
        if !input.contains_key("utm")
            && !input.contains_key("urlParameterValue")
            && create_if_missing
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.",
                    "code": "NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"
                })],
            );
        }
        if has_marketing_currency_mismatch(&input) {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Currency codes in the input do not match.",
                    "code": null
                })],
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            // Top-level GraphQL coercion in Shopify; parity compares errors for these cases.
            return marketing_activity_payload(None, vec![err]);
        }
        if create_if_missing
            && existing_id.is_none()
            && resolved_string_field(&input, "channelHandle")
                .is_some_and(|handle| handle != "email")
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                    "code": "INVALID_CHANNEL_HANDLE"
                })],
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        if create_if_missing
            && existing_id.is_none()
            && !remote.is_empty()
            && self
                .find_marketing_activity_by_remote(&remote, request)
                .is_some()
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                "message": "Validation failed: Remote ID has already been taken",
                "code": null
                })],
            );
        }
        let id = existing_id.unwrap_or_else(|| {
            format!("gid://shopify/MarketingActivity/{}", self.next_synthetic_id)
        });
        if !self.store.staged.marketing_activities.contains_key(&id) {
            self.next_synthetic_id += 2;
        }
        let existing = self.store.staged.marketing_activities.get(&id).cloned();
        let activity = marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
        );
        self.store.staged.deleted_marketing_activity_ids.remove(&id);
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        marketing_activity_payload(Some(activity), Vec::new())
    }

    pub(in crate::proxy) fn marketing_delete_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        if !field.arguments.contains_key("marketingActivityId")
            && !field.arguments.contains_key("id")
            && !field.arguments.contains_key("remoteId")
        {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [{
                "field": null,
                "message": "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.",
                "code": "INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"
            }] }),
                &field.selection,
            );
        }
        let id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(id) = id else {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] }),
                &field.selection,
            );
        };
        self.store
            .staged
            .deleted_marketing_activity_ids
            .insert(id.clone());
        selected_json(
            &json!({ "deletedMarketingActivityId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn marketing_engagement_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        _top_errors: &mut Vec<Value>,
    ) -> Value {
        let has_activity_id = field.arguments.contains_key("marketingActivityId");
        let has_remote = field.arguments.contains_key("remoteId");
        let has_channel = field.arguments.contains_key("channelHandle");
        let selector_count = [has_activity_id, has_remote, has_channel]
            .iter()
            .filter(|v| **v)
            .count();
        if selector_count == 0 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"
                    })],
                ),
                &field.selection,
            );
        }
        if selector_count > 1 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"
                    })],
                ),
                &field.selection,
            );
        }
        if let Some(channel) = resolved_string_arg(&field.arguments, "channelHandle") {
            if channel != "email" {
                return selected_json(
                    &marketing_engagement_payload(
                        None,
                        vec![json!({
                            "field": ["channelHandle"],
                            "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                            "code": "INVALID_CHANNEL_HANDLE"
                        })],
                    ),
                    &field.selection,
                );
            }
        }
        let activity_id =
            resolved_string_arg(&field.arguments, "marketingActivityId").or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(activity_id) = activity_id else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let engagement_input =
            resolved_object_field(&field.arguments, "marketingEngagement").unwrap_or_default();
        if has_engagement_currency_mismatch(&engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Currency codes in the marketing engagement input do not match.",
                        "code": "CURRENCY_CODE_MISMATCH_INPUT"
                    })],
                ),
                &field.selection,
            );
        }
        if self.engagement_currency_mismatches_activity(&activity_id, &engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Marketing activity currency code does not match the currency code in the marketing engagement input.",
                        "code": "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"
                    })],
                ),
                &field.selection,
            );
        }
        let engagement = marketing_engagement_from_input(
            &engagement_input,
            self.store.staged.marketing_activities.get(&activity_id),
        );
        if let Some(_activity) = self.store.staged.marketing_activities.get_mut(&activity_id) {
            // Shopify accepts engagement metrics but does not fold engagement ad spend
            // back into the MarketingActivity.adSpend field in these captures.
        }
        selected_json(
            &marketing_engagement_payload(Some(engagement), Vec::new()),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn marketing_engagements_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let errors = if !field.arguments.contains_key("channelHandle")
            && !matches!(
                field.arguments.get("deleteEngagementsForAllChannels"),
                Some(ResolvedValue::Bool(true))
            ) {
            vec![json!({
                "field": null,
                "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
                "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"
            })]
        } else {
            Vec::new()
        };
        let result = if errors.is_empty() {
            json!("Engagement data marked for deletion for 0 channel(s)")
        } else {
            Value::Null
        };
        selected_json(
            &json!({ "result": result, "userErrors": errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn find_marketing_activity_by_remote(
        &self,
        remote: &str,
        request: &Request,
    ) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self
                    .store
                    .staged
                    .deleted_marketing_activity_ids
                    .contains(id)
                {
                    return None;
                }
                if record["remoteId"].as_str() != Some(remote)
                    && record["marketingEvent"]["remoteId"].as_str() != Some(remote)
                {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    pub(in crate::proxy) fn find_marketing_activity_by_utm(
        &self,
        campaign: &str,
        request: &Request,
    ) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self
                    .store
                    .staged
                    .deleted_marketing_activity_ids
                    .contains(id)
                {
                    return None;
                }
                if record["utmParameters"]["campaign"].as_str() != Some(campaign) {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    pub(in crate::proxy) fn engagement_currency_mismatches_activity(
        &self,
        activity_id: &str,
        engagement: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(activity) = self.store.staged.marketing_activities.get(activity_id) else {
            return false;
        };
        let Some(activity_currency) = activity["budget"]["total"]["currencyCode"].as_str() else {
            return false;
        };
        marketing_money_currency(engagement, "adSpend").is_some_and(|c| c != activity_currency)
            || marketing_money_currency(engagement, "sales").is_some_and(|c| c != activity_currency)
    }

    pub(in crate::proxy) fn inventory_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventoryItems" => inventory_empty_connection(&field.selection),
                "inventoryProperties" => {
                    selected_json(&inventory_properties_json(), &field.selection)
                }
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_item_selected_json(&id, &field.selection)
                }
                "product" => selected_json(&json!({ "totalInventory": 0 }), &field.selection),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn inventory_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventorySetQuantities" => self.inventory_set_quantities(field),
                "inventoryMoveQuantities" => self.inventory_move_quantities(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn selling_plan_downstream_read_data(
        &mut self,
        query: &str,
    ) -> Option<Value> {
        if query.contains("DownstreamSellingPlanRead") {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/selling-plan-group-lifecycle.json"
            ))
            .expect("selling plan group lifecycle fixture must parse");
            let capture_index = match self.store.staged.selling_plan_group_downstream_step {
                0 => 4,
                1 => 6,
                _ => 10,
            };
            self.store.staged.selling_plan_group_downstream_step += 1;
            return Some(fixture["captures"][capture_index]["response"]["data"].clone());
        }
        if query.contains("ProductRelationshipSellingPlanMembershipRead") {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
            ))
            .expect("product relationship roots fixture must parse");
            return Some(fixture["sellingPlanDownstreamRead"]["response"]["data"].clone());
        }
        None
    }

    fn inventory_item_selected_json(
        &self,
        inventory_item_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        let inventory_quantity = self.inventory_total(inventory_item_id, "available");
        let item_levels = self.inventory_levels_for_item(inventory_item_id);
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "id" => Some(json!(inventory_item_id)),
                "variant" => Some(selected_json(
                    &json!({
                        "inventoryQuantity": inventory_quantity,
                        "product": { "totalInventory": 0 }
                    }),
                    &selection.selection,
                )),
                "locationsCount" => Some(selected_json(
                    &json!({
                        "count": item_levels.len(),
                        "precision": "EXACT"
                    }),
                    &selection.selection,
                )),
                "inventoryLevel" => {
                    let location_id = resolved_string_field(&selection.arguments, "locationId");
                    let level = location_id.and_then(|location_id| {
                        item_levels.iter().find(|(candidate_location_id, _)| {
                            *candidate_location_id == location_id
                        })
                    });
                    Some(level.map_or(Value::Null, |(location_id, quantities)| {
                        inventory_level_selected_json(
                            inventory_item_id,
                            location_id,
                            quantities,
                            &selection.selection,
                        )
                    }))
                }
                "inventoryLevels" => Some(inventory_levels_connection_selected_json(
                    inventory_item_id,
                    &item_levels,
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

    fn inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .map(|((_, location_id), quantities)| (location_id.clone(), quantities.clone()))
            .collect()
    }

    pub(in crate::proxy) fn inventory_total(&self, inventory_item_id: &str, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    pub(in crate::proxy) fn inventory_set_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if !ignore_compare
            && quantities
                .iter()
                .any(|quantity| !quantity.contains_key("compareQuantity"))
        {
            return selected_json(
                &json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [{
                        "field": ["input", "ignoreCompareQuantity"],
                        "message": "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity."
                    }]
                }),
                &field.selection,
            );
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        for quantity in quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let new_quantity = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id, location_id.clone());
            let level = self.store.staged.inventory_levels.entry(key).or_default();
            let old = level.get(&name).copied().unwrap_or(0);
            let delta = new_quantity - old;
            level.insert(name.clone(), new_quantity);
            if name == "available" {
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                level.insert("on_hand".to_string(), old_on_hand + delta);
                level.entry("damaged".to_string()).or_insert(0);
                on_hand_changes.push(inventory_change_json("on_hand", delta, None, &location_id));
            }
            changes.push(inventory_change_json(&name, delta, None, &location_id));
        }
        changes.extend(on_hand_changes);
        selected_json(
            &json!({
                "inventoryAdjustmentGroup": {
                    "reason": reason,
                    "referenceDocumentUri": reference,
                    "changes": changes
                },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn inventory_move_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return selected_json(
                    &json!({
                        "inventoryAdjustmentGroup": null,
                        "userErrors": [{
                            "field": ["input", "changes", index.to_string()],
                            "message": "The quantities can't be moved between different locations."
                        }]
                    }),
                    &field.selection,
                );
            }
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        for change in changes_input {
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(&change, "quantity").unwrap_or(0);
            let from = resolved_object_field(&change, "from").unwrap_or_default();
            let to = resolved_object_field(&change, "to").unwrap_or_default();
            let location_id = resolved_string_field(&from, "locationId").unwrap_or_default();
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            let ledger = resolved_string_field(&to, "ledgerDocumentUri");
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((item_id, location_id.clone()))
                .or_default();
            *level.entry(from_name.clone()).or_insert(0) -= quantity;
            *level.entry(to_name.clone()).or_insert(0) += quantity;
            level.entry("on_hand".to_string()).or_insert(0);
            changes.push(inventory_change_json(
                &from_name,
                -quantity,
                None,
                &location_id,
            ));
            changes.push(inventory_change_json(
                &to_name,
                quantity,
                ledger.as_deref(),
                &location_id,
            ));
        }
        selected_json(
            &json!({
                "inventoryAdjustmentGroup": {
                    "reason": reason,
                    "referenceDocumentUri": reference,
                    "changes": changes
                },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn functions_metadata_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = if field.name == "node" {
                resolved_field_string_arg(field, "id")
                    .and_then(|id| self.store.staged.function_cart_transforms.get(&id).cloned())
                    .or_else(|| self.store.staged.function_cart_transform.clone())
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }
}

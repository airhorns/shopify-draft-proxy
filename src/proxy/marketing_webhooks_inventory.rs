use super::*;
use crate::graphql::{parsed_document, ParsedDocument, RawArgumentValue};

const INVENTORY_VALID_REASONS: &[&str] = &[
    "correction",
    "cycle_count_available",
    "damaged",
    "movement_canceled",
    "movement_created",
    "movement_received",
    "movement_updated",
    "other",
    "promotion",
    "quality_control",
    "received",
    "reservation_created",
    "reservation_deleted",
    "reservation_updated",
    "restock",
    "safety_stock",
    "shrinkage",
];
const DEFAULT_INVENTORY_LOCATION_ID: &str = "gid://shopify/Location/106318430514";

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
                            if !marketing_record_matches_query(record, &query) {
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
                    let records = self
                        .store
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
                        .filter(|record| marketing_record_matches_query(record, &query))
                        .filter_map(|record| {
                            if record["marketingEvent"].is_null() {
                                None
                            } else {
                                Some(record["marketingEvent"].clone())
                            }
                        })
                        .collect();
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
        let document = parsed_document(query, variables);
        let root_selection = document
            .as_ref()
            .and_then(|document| document.root_fields.first());
        if let Some(error) = root_selection
            .and_then(|field| webhook_subscription_topic_coercion_error(field, document.as_ref()))
        {
            return ok_json(json!({ "errors": [error] }));
        }
        if let Some(error) = root_selection
            .and_then(|field| dedicated_pubsub_required_field_error(root_field, field))
        {
            return ok_json(json!({ "errors": [error] }));
        }
        let arguments = root_selection
            .map(|field| field.arguments.clone())
            .unwrap_or_else(|| root_field_arguments(query, variables).unwrap_or_default());
        let id = self.next_proxy_synthetic_gid("WebhookSubscription");
        let record = self.webhook_subscription_record(&id, &arguments, None);
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record, request);
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
        let document = parsed_document(query, variables);
        let root_selection = document
            .as_ref()
            .and_then(|document| document.root_fields.first());
        if let Some(error) = root_selection
            .and_then(|field| webhook_subscription_topic_coercion_error(field, document.as_ref()))
        {
            return ok_json(json!({ "errors": [error] }));
        }
        if let Some(error) = root_selection
            .and_then(|field| dedicated_pubsub_required_field_error(root_field, field))
        {
            return ok_json(json!({ "errors": [error] }));
        }
        let arguments = root_selection
            .map(|field| field.arguments.clone())
            .unwrap_or_else(|| root_field_arguments(query, variables).unwrap_or_default());
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
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record, request);
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
        request: &Request,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let uri = record["callbackUrl"].as_str().unwrap_or_default();
        let address_field = webhook_subscription_address_error_field(root_field);
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
                    if root_field.starts_with("pubSubWebhookSubscription") {
                        errors.push(json!({
                            "field": ["webhookSubscription", "pubSubProject"],
                            "message": "Google Cloud Pub/Sub project ID is not valid"
                        }));
                    } else {
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is invalid"
                        }));
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is not a valid GCP project id."
                        }));
                    }
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
        if uri.starts_with("arn:aws:events:") {
            if let Some(arn_api_client_id) = eventbridge_arn_api_client_id(uri) {
                if let Some(caller_api_client_id) =
                    request.headers.get("x-shopify-draft-proxy-api-client-id")
                {
                    if arn_api_client_id != caller_api_client_id {
                        errors.push(json!({
                            "field": address_field,
                            "message": "Address is invalid"
                        }));
                        errors.push(json!({
                            "field": address_field,
                            "message": format!(
                                "Address is an AWS ARN and includes api_client_id '{}' instead of '{}'",
                                arn_api_client_id, caller_api_client_id
                            )
                        }));
                    }
                }
            } else {
                errors.push(json!({
                    "field": address_field,
                    "message": "Address is invalid"
                }));
                errors.push(json!({
                    "field": address_field,
                    "message": "Address is not a valid AWS ARN"
                }));
            }
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
                    && existing["format"].as_str() == Some(format)
                    && webhook_subscription_optional_string_key(existing, "filter")
                        == webhook_subscription_optional_string_key(record, "filter")
                    && webhook_subscription_optional_string_key(existing, "apiPermissionId")
                        == webhook_subscription_optional_string_key(record, "apiPermissionId")
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
        let api_permission_id =
            resolved_string_field(&webhook_input, "apiPermissionId").or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["apiPermissionId"].as_str().map(ToString::to_string))
            });
        let name = resolved_string_field(&webhook_input, "name").or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record["name"].as_str().map(ToString::to_string))
        });
        let include_fields = if webhook_input.contains_key("includeFields") {
            json!(resolved_string_list_field_unsorted(
                &webhook_input,
                "includeFields"
            ))
        } else {
            existing
                .as_ref()
                .map(|record| record["includeFields"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let metafield_namespaces = if webhook_input.contains_key("metafieldNamespaces") {
            json!(resolved_string_list_field_unsorted(
                &webhook_input,
                "metafieldNamespaces"
            ))
        } else {
            existing
                .as_ref()
                .map(|record| record["metafieldNamespaces"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let filter = match webhook_input.get("filter") {
            Some(ResolvedValue::String(value)) => json!(value),
            Some(ResolvedValue::Null) => Value::Null,
            Some(_) => Value::Null,
            None => existing
                .as_ref()
                .map(|record| record["filter"].clone())
                .unwrap_or(Value::Null),
        };
        let created_at = existing
            .as_ref()
            .and_then(|record| record["createdAt"].as_str())
            .unwrap_or("2024-01-01T00:00:00.000Z");
        let webhook_mutation_count = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("interpreted")
                    .and_then(|interpreted| interpreted.get("primaryRootField"))
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "webhookSubscriptionCreate"
                                | "webhookSubscriptionUpdate"
                                | "pubSubWebhookSubscriptionCreate"
                                | "pubSubWebhookSubscriptionUpdate"
                                | "eventBridgeWebhookSubscriptionCreate"
                                | "eventBridgeWebhookSubscriptionUpdate"
                        )
                    })
            })
            .count();
        let updated_at = if existing.is_some() {
            format!("2024-01-01T00:00:{:02}.000Z", webhook_mutation_count + 1)
        } else {
            created_at.to_string()
        };
        json!({
            "id": id,
            "legacyResourceId": webhook_subscription_legacy_id(id),
            "topic": topic,
            "format": format,
            "uri": uri,
            "callbackUrl": uri,
            "name": name,
            "apiPermissionId": api_permission_id,
            "includeFields": include_fields,
            "metafieldNamespaces": metafield_namespaces,
            "filter": filter,
            "createdAt": created_at,
            "updatedAt": updated_at,
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
                "marketingEngagementsDelete" => self.marketing_engagements_delete(field, request),
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
        let selector_utm = resolved_object_field(&field.arguments, "utm");
        if let Some(err) = self.marketing_external_immutable_update_error(
            &existing,
            &input,
            selector_utm.as_ref(),
            request,
        ) {
            return selected_json(
                &marketing_activity_payload(None, vec![err]),
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
        if marketing_input_tactic_is_storefront_app(&input) {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![marketing_activity_cannot_update_tactic_to_storefront_error()],
                ),
                &field.selection,
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        let existing_id = self.find_marketing_activity_by_remote(&remote, request);
        if let Some(id) = &existing_id {
            if let Some(existing) = self.store.staged.marketing_activities.get(id) {
                if let Some(err) =
                    self.marketing_external_immutable_update_error(existing, &input, None, request)
                {
                    return selected_json(
                        &marketing_activity_payload(None, vec![err]),
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
                    "message": "Currency code is not matching between budget and ad spend",
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
        if create_if_missing && existing_id.is_none() {
            if !remote.is_empty()
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
            if resolved_object_field(&input, "utm")
                .and_then(|utm| resolved_string_field(&utm, "campaign"))
                .is_some_and(|campaign| {
                    self.find_marketing_activity_by_utm(&campaign, request)
                        .is_some()
                })
            {
                return marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input"],
                        "message": "Validation failed: Utm campaign has already been taken",
                        "code": null
                    })],
                );
            }
            if resolved_string_field(&input, "urlParameterValue").is_some_and(|value| {
                self.find_marketing_activity_by_url_parameter(&value, request)
                    .is_some()
            }) {
                let message = if field.name == "marketingActivityUpsertExternal" {
                    "Validation failed: Url parameter value has already been taken, Url parameter value has already been taken"
                } else {
                    "Validation failed: Url parameter value has already been taken"
                };
                return marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input"],
                        "message": message,
                        "code": null
                    })],
                );
            }
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
        let Some(activity) = self.marketing_activity_for_delete(&id, request) else {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] }),
                &field.selection,
            );
        };
        if activity["isExternal"] == json!(false) {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_delete_not_external_error()] }),
                &field.selection,
            );
        }
        if self.marketing_activity_has_child_events(activity) {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_child_events_error()] }),
                &field.selection,
            );
        }
        self.store
            .staged
            .deleted_marketing_activity_ids
            .insert(id.clone());
        selected_json(
            &json!({ "deletedMarketingActivityId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    fn marketing_activity_for_delete(&self, id: &str, request: &Request) -> Option<&Value> {
        if self
            .store
            .staged
            .deleted_marketing_activity_ids
            .contains(id)
        {
            return None;
        }
        let activity = self.store.staged.marketing_activities.get(id)?;
        let request_app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        if activity["apiClientId"].as_str() == request_app.map(String::as_str) {
            Some(activity)
        } else {
            None
        }
    }

    fn marketing_activity_has_child_events(&self, activity: &Value) -> bool {
        let parent_remote = activity["remoteId"]
            .as_str()
            .or_else(|| activity["marketingEvent"]["remoteId"].as_str());
        let Some(parent_remote) = parent_remote else {
            return false;
        };
        let parent_app = activity["apiClientId"].as_str();
        self.store
            .staged
            .marketing_activities
            .iter()
            .any(|(id, candidate)| {
                if self
                    .store
                    .staged
                    .deleted_marketing_activity_ids
                    .contains(id)
                {
                    return false;
                }
                candidate["id"].as_str() != activity["id"].as_str()
                    && candidate["apiClientId"].as_str() == parent_app
                    && candidate["parentRemoteId"].as_str() == Some(parent_remote)
            })
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
        if has_channel {
            let engagement = marketing_engagement_from_input(&engagement_input, None);
            return selected_json(
                &marketing_engagement_payload(Some(engagement), Vec::new()),
                &field.selection,
            );
        }
        let activity_id = if has_activity_id {
            resolved_string_arg(&field.arguments, "marketingActivityId")
        } else {
            resolved_string_arg(&field.arguments, "remoteId")
                .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
        };
        let Some(activity_id) = activity_id else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let Some(activity) = self
            .store
            .staged
            .marketing_activities
            .get(&activity_id)
            .filter(|_| {
                !self
                    .store
                    .staged
                    .deleted_marketing_activity_ids
                    .contains(&activity_id)
            })
        else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        if activity["marketingEvent"].is_null() {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_event_missing_error()]),
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
        let engagement = marketing_engagement_from_input(&engagement_input, Some(activity));
        // Shopify accepts engagement metrics but does not fold engagement ad spend
        // back into the MarketingActivity.adSpend field in these captures.
        selected_json(
            &marketing_engagement_payload(Some(engagement), Vec::new()),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn marketing_engagements_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let has_channel_handle = field.arguments.contains_key("channelHandle");
        let delete_all_channels = matches!(
            field.arguments.get("deleteEngagementsForAllChannels"),
            Some(ResolvedValue::Bool(true))
        );
        let known_handles = self.marketing_channel_handles_for_request(request);
        let (result, errors) = if has_channel_handle == delete_all_channels {
            (
                Value::Null,
                vec![json!({
                    "field": null,
                    "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
                    "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"
                })],
            )
        } else if let Some(channel_handle) = resolved_string_arg(&field.arguments, "channelHandle")
        {
            if known_handles.contains(&channel_handle) {
                (
                    json!(format!(
                        "Engagement data associated to channel handle '{channel_handle}' marked for deletion"
                    )),
                    Vec::new(),
                )
            } else {
                (
                    Value::Null,
                    vec![json!({
                        "field": ["channelHandle"],
                        "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                        "code": "INVALID_CHANNEL_HANDLE"
                    })],
                )
            }
        } else {
            (
                json!(format!(
                    "Engagement data marked for deletion for {} channel(s)",
                    known_handles.len()
                )),
                Vec::new(),
            )
        };
        selected_json(
            &json!({ "result": result, "userErrors": errors }),
            &field.selection,
        )
    }

    fn marketing_channel_handles_for_request(&self, request: &Request) -> BTreeSet<String> {
        let request_app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .filter_map(|(id, record)| {
                if self
                    .store
                    .staged
                    .deleted_marketing_activity_ids
                    .contains(id)
                {
                    return None;
                }
                if let Some(app) = request_app {
                    if record["apiClientId"].as_str() != Some(app.as_str()) {
                        return None;
                    }
                }
                record["marketingEvent"]["channelHandle"]
                    .as_str()
                    .filter(|handle| !handle.is_empty())
                    .map(str::to_string)
            })
            .collect()
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

    pub(in crate::proxy) fn find_marketing_activity_by_url_parameter(
        &self,
        url_parameter_value: &str,
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
                if record["urlParameterValue"].as_str() != Some(url_parameter_value) {
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

    fn marketing_external_immutable_update_error(
        &self,
        existing: &Value,
        input: &BTreeMap<String, ResolvedValue>,
        selector_utm: Option<&BTreeMap<String, ResolvedValue>>,
        request: &Request,
    ) -> Option<Value> {
        if existing["isExternal"] == json!(false) {
            return Some(marketing_activity_not_external_error());
        }
        if existing["marketingEvent"].is_null() {
            return Some(json!({
                "field": null,
                "message": "Marketing activity is not valid, the associated marketing event does not exist.",
                "code": "MARKETING_EVENT_DOES_NOT_EXIST"
            }));
        }
        if marketing_input_tactic_is_storefront_app(input) {
            return Some(marketing_activity_cannot_update_tactic_to_storefront_error());
        }
        if marketing_input_has_tactic(input)
            && marketing_activity_tactic_is_storefront_app(existing)
        {
            return Some(marketing_activity_cannot_update_tactic_from_storefront_error());
        }
        if resolved_string_field(input, "channelHandle").is_some_and(|channel_handle| {
            existing["marketingEvent"]["channelHandle"].as_str() != Some(channel_handle.as_str())
        }) {
            return Some(json!({
                "field": ["input"],
                "message": "Channel handle cannot be modified.",
                "code": "IMMUTABLE_CHANNEL_HANDLE"
            }));
        }
        if input_string_field_value(input, "urlParameterValue")
            != json_string_value(&existing["urlParameterValue"])
        {
            return Some(json!({
                "field": ["input"],
                "message": "URL parameter value cannot be modified.",
                "code": "IMMUTABLE_URL_PARAMETER"
            }));
        }
        if (input.contains_key("utm") || selector_utm.is_some())
            && (input_utm_value(input, selector_utm, "campaign")
                != json_string_value(&existing["utmParameters"]["campaign"])
                || input_utm_value(input, selector_utm, "source")
                    != json_string_value(&existing["utmParameters"]["source"])
                || input_utm_value(input, selector_utm, "medium")
                    != json_string_value(&existing["utmParameters"]["medium"]))
        {
            return Some(json!({
                "field": ["input"],
                "message": "UTM parameters cannot be modified.",
                "code": "IMMUTABLE_UTM_PARAMETERS"
            }));
        }
        if let Some(parent_remote_id) = resolved_string_field(input, "parentRemoteId") {
            let Some(parent_id) =
                self.find_marketing_activity_by_remote(&parent_remote_id, request)
            else {
                return Some(json!({
                    "field": ["input"],
                    "message": "Remote ID does not correspond to an activity.",
                    "code": "INVALID_REMOTE_ID"
                }));
            };
            let existing_parent_remote_id = existing["parentRemoteId"].as_str().unwrap_or("");
            let existing_parent_id = if existing_parent_remote_id.is_empty() {
                None
            } else {
                self.find_marketing_activity_by_remote(existing_parent_remote_id, request)
            };
            if existing_parent_id.as_deref() != Some(parent_id.as_str()) {
                return Some(json!({
                    "field": ["input"],
                    "message": "Parent ID cannot be modified.",
                    "code": "IMMUTABLE_PARENT_ID"
                }));
            }
        }
        if resolved_string_field(input, "hierarchyLevel").is_some_and(|hierarchy_level| {
            existing["hierarchyLevel"].as_str() != Some(hierarchy_level.as_str())
        }) {
            return Some(json!({
                "field": ["input"],
                "message": "Hierarchy level cannot be modified.",
                "code": "IMMUTABLE_HIERARCHY_LEVEL"
            }));
        }
        None
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

    pub(in crate::proxy) fn inventory_query_data(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventoryItems" => inventory_empty_connection(&field.selection),
                "inventoryProperties" => {
                    selected_json(&inventory_properties_json(), &field.selection)
                }
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_item_selected_json(&id, variables, &field.selection)
                }
                "inventoryLevel" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_level_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfer" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_transfer_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfers" => self.inventory_transfers_connection_selected_json(
                    self.store.staged.inventory_transfers.values().collect(),
                    &field.selection,
                ),
                "product" => {
                    let id = resolved_string_arg(&field.arguments, "id")
                        .or_else(|| resolved_string_field(variables, "productId"))
                        .unwrap_or_default();
                    self.inventory_product_selected_json(&id, &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn inventory_mutation_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> MutationOutcome {
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in fields {
            let outcome = match field.name.as_str() {
                "inventoryAdjustQuantities" => self.inventory_adjust_quantities(request, field),
                "inventorySetQuantities" => self.inventory_set_quantities(request, field),
                "inventoryMoveQuantities" => self.inventory_move_quantities(field),
                "inventoryTransferCreate" => self.inventory_transfer_create(field, false),
                "inventoryTransferCreateAsReadyToShip" => {
                    self.inventory_transfer_create(field, true)
                }
                "inventoryTransferMarkAsReadyToShip" => self.inventory_transfer_mark_ready(field),
                "inventoryTransferSetItems" => self.inventory_transfer_set_items(field),
                "inventoryTransferRemoveItems" => self.inventory_transfer_remove_items(field),
                "inventoryTransferCancel" => self.inventory_transfer_cancel(field),
                "inventoryTransferDelete" => self.inventory_transfer_delete(field),
                _ => MutationFieldOutcome::unlogged(Value::Null),
            };
            if let Some(errors) = outcome.value.get("__topLevelErrors") {
                return MutationOutcome::response(ok_json(json!({
                    "errors": errors,
                    "data": { field.response_key.clone(): Value::Null }
                })));
            }
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        MutationOutcome::with_log_drafts(
            ok_json(json!({ "data": Value::Object(data) })),
            log_drafts,
        )
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
        variables: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        if let Some(variant) = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
        {
            return product_variant_inventory_item_json(variant, selections);
        }

        let inventory_quantity = self.inventory_total(inventory_item_id, "available");
        let item_levels = self.inventory_levels_for_item(inventory_item_id);
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let variant_id = resolved_string_field(variables, "variantId").unwrap_or_else(|| {
            format!(
                "gid://shopify/ProductVariant/{}",
                resource_id_tail(inventory_item_id)
            )
        });
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "id" => Some(json!(inventory_item_id)),
                "tracked" => Some(json!(true)),
                "requiresShipping" => Some(json!(true)),
                "variant" => Some(selected_json(
                    &json!({
                        "id": variant_id,
                        "inventoryQuantity": inventory_quantity,
                        "product": {
                            "id": product_id,
                            "totalInventory": self.inventory_total_all("available")
                        }
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
                            &self.store.staged.inventory_quantity_updated_at,
                            &selection.selection,
                            Some(&self.store.staged.locations),
                        )
                    }))
                }
                "inventoryLevels" => Some(inventory_levels_connection_selected_json(
                    inventory_item_id,
                    &item_levels,
                    &self.store.staged.inventory_quantity_updated_at,
                    &selection.arguments,
                    &selection.selection,
                    Some(&self.store.staged.locations),
                )),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    fn inventory_level_by_id_selected_json(&self, id: &str, selections: &[SelectedField]) -> Value {
        let Some((inventory_item_id, location_id)) = inventory_level_parts_from_id(id) else {
            return Value::Null;
        };
        let Some(quantities) = self
            .store
            .staged
            .inventory_levels
            .get(&(inventory_item_id.clone(), location_id.clone()))
        else {
            return Value::Null;
        };
        inventory_level_selected_json(
            &inventory_item_id,
            &location_id,
            quantities,
            &self.store.staged.inventory_quantity_updated_at,
            selections,
            Some(&self.store.staged.locations),
        )
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

    fn next_inventory_quantity_timestamp(&mut self) -> String {
        let sequence = self.store.staged.next_inventory_quantity_timestamp;
        self.store.staged.next_inventory_quantity_timestamp += 1;
        format!("2024-01-01T00:00:{sequence:02}.000Z")
    }

    fn stamp_inventory_quantity(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        name: &str,
        updated_at: &str,
    ) {
        self.store.staged.inventory_quantity_updated_at.insert(
            (
                inventory_item_id.to_string(),
                location_id.to_string(),
                name.to_string(),
            ),
            updated_at.to_string(),
        );
    }

    pub(in crate::proxy) fn decrement_inventory_item_available(
        &mut self,
        inventory_item_id: &str,
        quantity: i64,
    ) {
        if quantity <= 0 {
            return;
        }
        let location_id = self
            .store
            .staged
            .inventory_levels
            .keys()
            .find(|(item_id, _)| item_id == inventory_item_id)
            .map(|(_, location_id)| location_id.clone())
            .unwrap_or_else(|| "gid://shopify/Location/1".to_string());
        let updated_at = self.next_inventory_quantity_timestamp();
        {
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id.to_string(), location_id.clone()))
                .or_default();
            *level.entry("available".to_string()).or_insert(0) -= quantity;
            *level.entry("on_hand".to_string()).or_insert(0) -= quantity;
            level.entry("damaged".to_string()).or_insert(0);
        }
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "available", &updated_at);
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "on_hand", &updated_at);
    }

    fn inventory_total_all(&self, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .values()
            .map(|quantities| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    fn inventory_product_selected_json(
        &self,
        product_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        selected_json(
            &json!({
                "id": product_id,
                "totalInventory": self.inventory_total_all("available"),
                "tracksInventory": true
            }),
            selections,
        )
    }

    pub(in crate::proxy) fn inventory_set_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if inventory_set_requires_change_from(request, field) && !ignore_compare {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventorySetQuantities",
                "InventoryQuantityInput",
                &quantities,
                "quantity",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if !ignore_compare
            && quantities.iter().any(|quantity| {
                !quantity.contains_key("compareQuantity")
                    && !quantity.contains_key("changeFromQuantity")
            })
        {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [{
                        "field": ["input", "ignoreCompareQuantity"],
                        "message": "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity."
                    }]
                }),
                &field.selection,
            ));
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for quantity in quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let new_quantity = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id.clone(), location_id.clone());
            let level = self.store.staged.inventory_levels.entry(key).or_default();
            let old = level.get(&name).copied().unwrap_or(0);
            let delta = new_quantity - old;
            level.insert(name.clone(), new_quantity);
            if name == "available" {
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                let on_hand_after_change = old_on_hand + delta;
                level.insert("on_hand".to_string(), on_hand_after_change);
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    on_hand_after_change,
                    None,
                    &location_id,
                ));
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                new_quantity,
                None,
                &location_id,
            ));
        }
        changes.extend(on_hand_changes);
        MutationFieldOutcome::staged(
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
            ),
            LogDraft::staged("inventorySetQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_adjust_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if inventory_adjust_requires_change_from(request) {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventoryAdjustQuantities",
                "InventoryChangeInput",
                &changes_input,
                "delta",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for change in changes_input {
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&change, "locationId").unwrap_or_default();
            let delta = resolved_int_field(&change, "delta").unwrap_or(0);
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((item_id.clone(), location_id.clone()))
                .or_default();
            let after_change = {
                let quantity = level.entry(name.clone()).or_insert(0);
                *quantity += delta;
                *quantity
            };
            if name == "available" {
                let on_hand_after_change = {
                    let on_hand = level.entry("on_hand".to_string()).or_insert(0);
                    *on_hand += delta;
                    *on_hand
                };
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    on_hand_after_change,
                    None,
                    &location_id,
                ));
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                after_change,
                None,
                &location_id,
            ));
        }
        changes.extend(on_hand_changes);
        MutationFieldOutcome::staged(
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
            ),
            LogDraft::staged("inventoryAdjustQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_move_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return MutationFieldOutcome::unlogged(selected_json(
                    &json!({
                        "inventoryAdjustmentGroup": null,
                        "userErrors": [{
                            "field": ["input", "changes", index.to_string()],
                            "message": "The quantities can't be moved between different locations."
                        }]
                    }),
                    &field.selection,
                ));
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
            let updated_at = self.next_inventory_quantity_timestamp();
            let (from_after_change, to_after_change) = {
                let level = self
                    .store
                    .staged
                    .inventory_levels
                    .entry((item_id.clone(), location_id.clone()))
                    .or_default();
                let from_after_change = {
                    let from_quantity = level.entry(from_name.clone()).or_insert(0);
                    *from_quantity -= quantity;
                    *from_quantity
                };
                let to_after_change = {
                    let to_quantity = level.entry(to_name.clone()).or_insert(0);
                    *to_quantity += quantity;
                    *to_quantity
                };
                level.entry("on_hand".to_string()).or_insert(0);
                (from_after_change, to_after_change)
            };
            self.stamp_inventory_quantity(&item_id, &location_id, &from_name, &updated_at);
            self.stamp_inventory_quantity(&item_id, &location_id, &to_name, &updated_at);
            changes.push(inventory_change_json(
                &item_id,
                &from_name,
                -quantity,
                from_after_change,
                None,
                &location_id,
            ));
            changes.push(inventory_change_json(
                &item_id,
                &to_name,
                quantity,
                to_after_change,
                ledger.as_deref(),
                &location_id,
            ));
        }
        MutationFieldOutcome::staged(
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
            ),
            LogDraft::staged("inventoryMoveQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_create(
        &mut self,
        field: &RootFieldSelection,
        ready_to_ship: bool,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let origin_location_id =
            resolved_string_field(&input, "originLocationId").unwrap_or_default();
        let destination_location_id =
            resolved_string_field(&input, "destinationLocationId").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store
                .staged
                .inventory_transfers
                .len()
                .saturating_add(1)
        );
        let mut line_items = Vec::new();
        for item_input in resolved_object_list_field(&input, "lineItems") {
            line_items.push(InventoryTransferLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                inventory_item_id: resolved_string_field(&item_input, "inventoryItemId")
                    .unwrap_or_default(),
                quantity: resolved_int_field(&item_input, "quantity").unwrap_or(0),
            });
        }
        let record = InventoryTransferRecord {
            id: id.clone(),
            name,
            status: if ready_to_ship {
                "READY_TO_SHIP".to_string()
            } else {
                "DRAFT".to_string()
            },
            origin_location_id,
            destination_location_id,
            line_items,
        };
        self.ensure_transfer_inventory_levels(&record);
        if ready_to_ship {
            self.apply_transfer_reservations(&record, 1);
        }
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(field.name.clone(), "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_mark_ready(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "DRAFT" {
            self.apply_transfer_reservations(&record, 1);
        }
        record.status = "READY_TO_SHIP".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferMarkAsReadyToShip", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_set_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        let mut updated = Vec::new();
        for item_input in resolved_object_list_field(&input, "lineItems") {
            let item_id = resolved_string_field(&item_input, "inventoryItemId").unwrap_or_default();
            let new_quantity = resolved_int_field(&item_input, "quantity").unwrap_or(0);
            let mut old_quantity = 0;
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.inventory_item_id == item_id)
            {
                old_quantity = line_item.quantity;
                line_item.quantity = new_quantity;
            } else {
                record.line_items.push(InventoryTransferLineItemRecord {
                    id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                    inventory_item_id: item_id.clone(),
                    quantity: new_quantity,
                });
            }
            let delta = new_quantity - old_quantity;
            if record.status == "READY_TO_SHIP" {
                self.apply_inventory_reservation(&item_id, &record.origin_location_id, delta);
            }
            updated.push(json!({
                "inventoryItemId": item_id,
                "newQuantity": new_quantity,
                "deltaQuantity": delta
            }));
        }
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "updatedLineItems": updated,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferSetItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_remove_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        let remove_ids = resolved_string_list_field(&input, "transferLineItemIds");
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for line_item in record.line_items {
            if remove_ids.iter().any(|id| id == &line_item.id) {
                if record.status == "READY_TO_SHIP" {
                    self.apply_inventory_reservation(
                        &line_item.inventory_item_id,
                        &record.origin_location_id,
                        -line_item.quantity,
                    );
                }
                removed.push(json!({
                    "inventoryItemId": line_item.inventory_item_id,
                    "newQuantity": 0,
                    "deltaQuantity": -line_item.quantity
                }));
            } else {
                kept.push(line_item);
            }
        }
        record.line_items = kept;
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "removedQuantities": removed,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferRemoveItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_cancel(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "READY_TO_SHIP" {
            self.apply_transfer_reservations(&record, -1);
        }
        record.status = "CANCELED".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferCancel", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [{"field": ["id"], "message": "Inventory transfer not found."}]
                }),
                &field.selection,
            ));
        };
        if record.status != "DRAFT" {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Can't delete the transfer if it's not in the draft status."
                    }]
                }),
                &field.selection,
            ));
        }
        self.store.staged.inventory_transfers.remove(&id);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({ "deletedId": id, "userErrors": [] }),
                &field.selection,
            ),
            LogDraft::staged("inventoryTransferDelete", "products", Vec::new()),
        )
    }

    fn inventory_transfer_payload_json(
        &self,
        record: &InventoryTransferRecord,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: self.inventory_transfer_full_json(record),
                "userErrors": []
            }),
            selection,
        )
    }

    fn inventory_transfer_missing_payload(
        &self,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: Value::Null,
                "userErrors": [{"field": ["id"], "message": "Inventory transfer not found."}]
            }),
            selection,
        )
    }

    fn inventory_transfer_by_id_selected_json(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        self.store
            .staged
            .inventory_transfers
            .get(id)
            .map(|record| selected_json(&self.inventory_transfer_full_json(record), selection))
            .unwrap_or(Value::Null)
    }

    fn inventory_transfers_connection_selected_json(
        &self,
        transfers: Vec<&InventoryTransferRecord>,
        selection: &[SelectedField],
    ) -> Value {
        let nodes = transfers
            .into_iter()
            .map(|record| self.inventory_transfer_full_json(record))
            .collect::<Vec<_>>();
        selected_json(
            &json!({
                "nodes": nodes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }),
            selection,
        )
    }

    fn inventory_transfer_full_json(&self, record: &InventoryTransferRecord) -> Value {
        let nodes = record
            .line_items
            .iter()
            .map(|line_item| {
                let shippable = if record.status == "READY_TO_SHIP" {
                    line_item.quantity
                } else {
                    0
                };
                json!({
                    "id": line_item.id,
                    "inventoryItem": { "id": line_item.inventory_item_id },
                    "totalQuantity": line_item.quantity,
                    "shippableQuantity": shippable,
                    "shippedQuantity": 0,
                    "processableQuantity": line_item.quantity,
                    "pickedForShipmentQuantity": 0
                })
            })
            .collect::<Vec<_>>();
        json!({
            "id": record.id,
            "name": record.name,
            "status": record.status,
            "totalQuantity": record.line_items.iter().map(|line_item| line_item.quantity).sum::<i64>(),
            "lineItems": {
                "nodes": nodes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }
        })
    }

    fn ensure_transfer_inventory_levels(&mut self, record: &InventoryTransferRecord) {
        for line_item in &record.line_items {
            if record.origin_location_id != DEFAULT_INVENTORY_LOCATION_ID
                && record.destination_location_id != DEFAULT_INVENTORY_LOCATION_ID
            {
                self.store
                    .staged
                    .inventory_levels
                    .entry((
                        line_item.inventory_item_id.clone(),
                        DEFAULT_INVENTORY_LOCATION_ID.to_string(),
                    ))
                    .or_insert_with(empty_inventory_quantities);
            }
            let origin = self
                .store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.origin_location_id.clone(),
                ))
                .or_default();
            if origin.is_empty() {
                origin.insert("available".to_string(), 5);
                origin.insert("reserved".to_string(), 0);
                origin.insert("on_hand".to_string(), 5);
            }
            self.store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.destination_location_id.clone(),
                ))
                .or_insert_with(empty_inventory_quantities);
        }
    }

    fn apply_transfer_reservations(&mut self, record: &InventoryTransferRecord, direction: i64) {
        for line_item in &record.line_items {
            self.apply_inventory_reservation(
                &line_item.inventory_item_id,
                &record.origin_location_id,
                direction * line_item.quantity,
            );
        }
    }

    fn apply_inventory_reservation(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        reserved_delta: i64,
    ) {
        let level = self
            .store
            .staged
            .inventory_levels
            .entry((inventory_item_id.to_string(), location_id.to_string()))
            .or_insert_with(|| {
                BTreeMap::from([
                    ("available".to_string(), 5),
                    ("reserved".to_string(), 0),
                    ("on_hand".to_string(), 5),
                ])
            });
        *level.entry("available".to_string()).or_insert(0) -= reserved_delta;
        *level.entry("reserved".to_string()).or_insert(0) += reserved_delta;
        let available = level.get("available").copied().unwrap_or(0);
        let reserved = level.get("reserved").copied().unwrap_or(0);
        level
            .entry("on_hand".to_string())
            .or_insert(available + reserved);
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

fn input_string_field_value(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn input_utm_value(
    input: &BTreeMap<String, ResolvedValue>,
    selector_utm: Option<&BTreeMap<String, ResolvedValue>>,
    field: &str,
) -> Option<String> {
    match input.get("utm") {
        Some(ResolvedValue::Object(utm)) => input_string_field_value(utm, field),
        _ => selector_utm.and_then(|utm| input_string_field_value(utm, field)),
    }
}

fn json_string_value(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn marketing_activity_not_external_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing activity is not external.",
        "code": "ACTIVITY_NOT_EXTERNAL"
    })
}

fn marketing_activity_delete_not_external_error() -> Value {
    json!({
        "field": null,
        "message": "The marketing activity must be an external activity.",
        "code": "ACTIVITY_NOT_EXTERNAL"
    })
}

fn webhook_subscription_topic_coercion_error(
    field: &RootFieldSelection,
    document: Option<&ParsedDocument>,
) -> Option<Value> {
    let raw_topic = field.raw_arguments.get("topic")?;
    let topic = match raw_topic {
        RawArgumentValue::Enum(topic) => topic.as_str(),
        RawArgumentValue::Variable {
            value: Some(ResolvedValue::String(topic)),
            ..
        } => topic.as_str(),
        _ => return None,
    };
    if is_known_webhook_subscription_topic(topic) {
        return None;
    }
    Some(match raw_topic {
        RawArgumentValue::Enum(_) => json!({
            "message": format!("Argument 'topic' on Field '{}' has an invalid value ({}). Expected type 'WebhookSubscriptionTopic!'.", field.name, topic),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [
                document
                    .map(|document| document.operation_path.clone())
                    .unwrap_or_else(|| "mutation".to_string()),
                field.name.clone(),
                "topic"
            ],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "Field",
                "argumentName": "topic"
            }
        }),
        RawArgumentValue::Variable { name, .. } => json!({
            "message": format!("Variable ${} of type WebhookSubscriptionTopic! was provided invalid value", name),
            "locations": [{
                "line": document.map_or(field.location.line, |document| document.location.line),
                "column": document.map_or(field.location.column, |document| document.location.column)
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": topic,
                "problems": [{
                    "path": [],
                    "explanation": format!("Expected \"{}\" to be one of: {}", topic, WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES)
                }]
            }
        }),
        _ => unreachable!(),
    })
}

fn dedicated_pubsub_required_field_error(
    root_field: &str,
    field: &RootFieldSelection,
) -> Option<Value> {
    if !root_field.starts_with("pubSubWebhookSubscription") {
        return None;
    }
    match field.raw_arguments.get("webhookSubscription")? {
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(value)),
        } => dedicated_pubsub_variable_required_field_error(name, value, field),
        RawArgumentValue::Object(value) => {
            dedicated_pubsub_inline_required_field_error(value, field)
        }
        _ => None,
    }
}

fn dedicated_pubsub_variable_required_field_error(
    variable_name: &str,
    value: &BTreeMap<String, ResolvedValue>,
    field: &RootFieldSelection,
) -> Option<Value> {
    let missing = missing_pubsub_resolved_fields(value);
    if missing.is_empty() {
        return None;
    }
    let message_detail = missing
        .iter()
        .map(|key| format!("{key} (Expected value to not be null)"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(json!({
        "message": format!("Variable ${} of type PubSubWebhookSubscriptionInput! was provided invalid value for {}", variable_name, message_detail),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&ResolvedValue::Object(value.clone())),
            "problems": missing
                .iter()
                .map(|key| json!({
                    "path": [key],
                    "explanation": "Expected value to not be null"
                }))
                .collect::<Vec<_>>()
        }
    }))
}

fn dedicated_pubsub_inline_required_field_error(
    value: &BTreeMap<String, RawArgumentValue>,
    field: &RootFieldSelection,
) -> Option<Value> {
    let missing = ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key)
                || value
                    .get(*key)
                    .is_some_and(RawArgumentValue::is_literal_null)
        })
        .collect::<Vec<_>>();
    let first_missing = missing.first()?;
    Some(json!({
        "message": format!("Argument '{}' on InputObject 'PubSubWebhookSubscriptionInput' is required. Expected type String!", first_missing),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": ["mutation", field.name.clone(), "webhookSubscription", first_missing],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": first_missing,
            "argumentType": "String!",
            "inputObjectType": "PubSubWebhookSubscriptionInput"
        }
    }))
}

fn missing_pubsub_resolved_fields(value: &BTreeMap<String, ResolvedValue>) -> Vec<&'static str> {
    ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key) || matches!(value.get(*key), Some(ResolvedValue::Null))
        })
        .collect()
}

fn webhook_subscription_address_error_field(root_field: &str) -> Value {
    if root_field.starts_with("eventBridgeWebhookSubscription") {
        json!(["webhookSubscription", "arn"])
    } else {
        json!(["webhookSubscription", "callbackUrl"])
    }
}

fn webhook_subscription_optional_string_key(record: &Value, key: &str) -> Option<String> {
    record[key].as_str().map(ToString::to_string)
}

fn is_known_webhook_subscription_topic(topic: &str) -> bool {
    WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES
        .split(", ")
        .any(|known| known == topic)
}

fn inventory_quantity_missing_change_from_payload(
    field: &RootFieldSelection,
    root_field: &str,
    input_type: &str,
    rows: &[BTreeMap<String, ResolvedValue>],
    quantity_field: &str,
) -> Option<Value> {
    if rows
        .iter()
        .any(|row| row.contains_key("changeFromQuantity"))
        || rows.iter().any(|row| row.contains_key("compareQuantity"))
    {
        return None;
    }
    if rows.iter().any(|row| row.contains_key(quantity_field)) {
        return Some(json!({
            "__topLevelErrors": [{
                "message": format!("{input_type} must include the following argument: changeFromQuantity."),
                "locations": [
                    { "line": field.location.line, "column": field.location.column },
                    { "line": field.location.line.saturating_sub(1).max(1), "column": 1 }
                ],
                "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
                "path": [root_field]
            }]
        }));
    }
    None
}

fn inventory_adjust_requires_change_from(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
}

fn inventory_set_requires_change_from(request: &Request, field: &RootFieldSelection) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
        && field
            .directives
            .iter()
            .any(|directive| directive == "idempotent")
}

fn empty_inventory_quantities() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("available".to_string(), 0),
        ("reserved".to_string(), 0),
        ("on_hand".to_string(), 0),
    ])
}

fn inventory_invalid_reason_payload(
    field: &RootFieldSelection,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let reason = resolved_string_field(input, "reason").unwrap_or_else(|| "correction".to_string());
    if INVENTORY_VALID_REASONS.iter().any(|valid| *valid == reason) {
        return None;
    }
    Some(selected_json(
        &json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "reason"],
                "message": format!(
                    "The specified reason is invalid. Valid values are: {}.",
                    INVENTORY_VALID_REASONS.join(", ")
                ),
                "code": "INVALID_REASON"
            }]
        }),
        &field.selection,
    ))
}

#[cfg(test)]
#[test]
fn immutable_external_activity_validator_rejects_missing_marketing_event() {
    let proxy = DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    });
    let err = proxy.marketing_external_immutable_update_error(
        &json!({
            "isExternal": true,
            "marketingEvent": null,
            "urlParameterValue": "url",
            "utmParameters": {
                "campaign": "campaign",
                "source": "source",
                "medium": "medium"
            }
        }),
        &BTreeMap::new(),
        None,
        &Request::default(),
    );

    assert_eq!(
        err,
        Some(json!({
            "field": null,
            "message": "Marketing activity is not valid, the associated marketing event does not exist.",
            "code": "MARKETING_EVENT_DOES_NOT_EXIST"
        }))
    );
}

const WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES: &str = "TAX_SUMMARIES_CREATE, APP_UNINSTALLED, APP_SCOPES_UPDATE, CARTS_CREATE, CARTS_UPDATE, CHANNELS_DELETE, CHECKOUTS_CREATE, CHECKOUTS_DELETE, CHECKOUTS_UPDATE, CUSTOMER_PAYMENT_METHODS_CREATE, CUSTOMER_PAYMENT_METHODS_UPDATE, CUSTOMER_PAYMENT_METHODS_REVOKE, COLLECTION_LISTINGS_ADD, COLLECTION_LISTINGS_REMOVE, COLLECTION_LISTINGS_UPDATE, COLLECTION_PUBLICATIONS_CREATE, COLLECTION_PUBLICATIONS_DELETE, COLLECTION_PUBLICATIONS_UPDATE, COLLECTIONS_CREATE, COLLECTIONS_DELETE, COLLECTIONS_UPDATE, CUSTOMER_GROUPS_CREATE, CUSTOMER_GROUPS_DELETE, CUSTOMER_GROUPS_UPDATE, CUSTOMERS_CREATE, CUSTOMERS_DELETE, CUSTOMERS_DISABLE, CUSTOMERS_ENABLE, CUSTOMERS_UPDATE, CUSTOMERS_PURCHASING_SUMMARY, CUSTOMERS_MARKETING_CONSENT_UPDATE, CUSTOMER_TAGS_ADDED, CUSTOMER_TAGS_REMOVED, CUSTOMERS_EMAIL_MARKETING_CONSENT_UPDATE, DISPUTES_CREATE, DISPUTES_UPDATE, DRAFT_ORDERS_CREATE, DRAFT_ORDERS_DELETE, DRAFT_ORDERS_UPDATE, FULFILLMENT_EVENTS_CREATE, FULFILLMENT_EVENTS_DELETE, FULFILLMENTS_CREATE, FULFILLMENTS_UPDATE, ATTRIBUTED_SESSIONS_FIRST, ATTRIBUTED_SESSIONS_LAST, ORDER_TRANSACTIONS_CREATE, ORDERS_CANCELLED, ORDERS_CREATE, ORDERS_DELETE, ORDERS_EDITED, ORDERS_FULFILLED, ORDERS_PAID, ORDERS_PARTIALLY_FULFILLED, ORDERS_UPDATED, ORDERS_LINK_REQUESTED, FULFILLMENT_ORDERS_MOVED, FULFILLMENT_ORDERS_HOLD_RELEASED, FULFILLMENT_ORDERS_SCHEDULED_FULFILLMENT_ORDER_READY, FULFILLMENT_HOLDS_RELEASED, FULFILLMENT_ORDERS_ORDER_ROUTING_COMPLETE, FULFILLMENT_ORDERS_CANCELLED, FULFILLMENT_ORDERS_FULFILLMENT_SERVICE_FAILED_TO_COMPLETE, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_REJECTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_ACCEPTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_REJECTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_ACCEPTED, FULFILLMENT_HOLDS_ADDED, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_LOCAL_DELIVERY, FULFILLMENT_ORDERS_PLACED_ON_HOLD, FULFILLMENT_ORDERS_MERGED, FULFILLMENT_ORDERS_SPLIT, FULFILLMENT_ORDERS_PROGRESS_REPORTED, FULFILLMENT_ORDERS_MANUALLY_REPORTED_PROGRESS_STOPPED, PRODUCT_LISTINGS_ADD, PRODUCT_LISTINGS_REMOVE, PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_ADD, SCHEDULED_PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_REMOVE, PRODUCT_PUBLICATIONS_CREATE, PRODUCT_PUBLICATIONS_DELETE, PRODUCT_PUBLICATIONS_UPDATE, PRODUCTS_CREATE, PRODUCTS_DELETE, PRODUCTS_UPDATE, REFUNDS_CREATE, SEGMENTS_CREATE, SEGMENTS_DELETE, SEGMENTS_UPDATE, SHIPPING_ADDRESSES_CREATE, SHIPPING_ADDRESSES_UPDATE, SHOP_UPDATE, TAX_PARTNERS_UPDATE, TAX_SERVICES_CREATE, TAX_SERVICES_UPDATE, THEMES_CREATE, THEMES_DELETE, THEMES_PUBLISH, THEMES_UPDATE, VARIANTS_IN_STOCK, VARIANTS_OUT_OF_STOCK, INVENTORY_LEVELS_CONNECT, INVENTORY_LEVELS_UPDATE, INVENTORY_LEVELS_DISCONNECT, INVENTORY_ITEMS_CREATE, INVENTORY_ITEMS_UPDATE, INVENTORY_ITEMS_DELETE, LOCATIONS_ACTIVATE, LOCATIONS_DEACTIVATE, LOCATIONS_CREATE, LOCATIONS_UPDATE, LOCATIONS_DELETE, TENDER_TRANSACTIONS_CREATE, APP_PURCHASES_ONE_TIME_UPDATE, APP_SUBSCRIPTIONS_APPROACHING_CAPPED_AMOUNT, APP_SUBSCRIPTIONS_UPDATE, LOCALES_CREATE, LOCALES_UPDATE, LOCALES_DESTROY, DOMAINS_CREATE, DOMAINS_UPDATE, DOMAINS_DESTROY, SUBSCRIPTION_CONTRACTS_CREATE, SUBSCRIPTION_CONTRACTS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_CREATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_DELETE, PROFILES_CREATE, PROFILES_UPDATE, PROFILES_DELETE, SUBSCRIPTION_BILLING_ATTEMPTS_SUCCESS, SUBSCRIPTION_BILLING_ATTEMPTS_FAILURE, SUBSCRIPTION_BILLING_ATTEMPTS_CHALLENGED, RETURNS_CANCEL, RETURNS_CLOSE, RETURNS_REOPEN, RETURNS_REQUEST, RETURNS_APPROVE, RETURNS_UPDATE, RETURNS_PROCESS, RETURNS_DECLINE, REVERSE_DELIVERIES_ATTACH_DELIVERABLE, REVERSE_FULFILLMENT_ORDERS_DISPOSE, PAYMENT_TERMS_CREATE, PAYMENT_TERMS_DELETE, PAYMENT_TERMS_UPDATE, PAYMENT_SCHEDULES_DUE, SELLING_PLAN_GROUPS_CREATE, SELLING_PLAN_GROUPS_UPDATE, SELLING_PLAN_GROUPS_DELETE, BULK_OPERATIONS_FINISH, PRODUCT_FEEDS_CREATE, PRODUCT_FEEDS_UPDATE, PRODUCT_FEEDS_INCREMENTAL_SYNC, PRODUCT_FEEDS_FULL_SYNC, PRODUCT_FEEDS_FULL_SYNC_FINISH, MARKETS_CREATE, MARKETS_UPDATE, MARKETS_DELETE, ORDERS_RISK_ASSESSMENT_CHANGED, ORDERS_SHOPIFY_PROTECT_ELIGIBILITY_CHANGED, FINANCE_KYC_INFORMATION_UPDATE, FULFILLMENT_ORDERS_RESCHEDULED, PUBLICATIONS_DELETE, AUDIT_EVENTS_ADMIN_API_ACTIVITY, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_PICKUP, COMPANIES_CREATE, COMPANIES_UPDATE, COMPANIES_DELETE, COMPANY_LOCATIONS_CREATE, COMPANY_LOCATIONS_UPDATE, COMPANY_LOCATIONS_DELETE, COMPANY_CONTACTS_CREATE, COMPANY_CONTACTS_UPDATE, COMPANY_CONTACTS_DELETE, CUSTOMERS_MERGE, INVENTORY_TRANSFERS_ADD_ITEMS, INVENTORY_TRANSFERS_UPDATE_ITEM_QUANTITIES, INVENTORY_TRANSFERS_REMOVE_ITEMS, INVENTORY_TRANSFERS_READY_TO_SHIP, INVENTORY_TRANSFERS_CANCEL, INVENTORY_TRANSFERS_COMPLETE, INVENTORY_SHIPMENTS_DELETE, INVENTORY_SHIPMENTS_CREATE, INVENTORY_SHIPMENTS_MARK_IN_TRANSIT, INVENTORY_SHIPMENTS_UPDATE_TRACKING, INVENTORY_SHIPMENTS_ADD_ITEMS, INVENTORY_SHIPMENTS_UPDATE_ITEM_QUANTITIES, INVENTORY_SHIPMENTS_REMOVE_ITEMS, INVENTORY_SHIPMENTS_RECEIVE_ITEMS, CUSTOMER_ACCOUNT_SETTINGS_UPDATE, CUSTOMER_JOINED_SEGMENT, CUSTOMER_LEFT_SEGMENT, COMPANY_CONTACT_ROLES_ASSIGN, COMPANY_CONTACT_ROLES_REVOKE, SUBSCRIPTION_CONTRACTS_ACTIVATE, SUBSCRIPTION_CONTRACTS_PAUSE, SUBSCRIPTION_CONTRACTS_CANCEL, SUBSCRIPTION_CONTRACTS_FAIL, SUBSCRIPTION_CONTRACTS_EXPIRE, SUBSCRIPTION_BILLING_CYCLES_SKIP, SUBSCRIPTION_BILLING_CYCLES_UNSKIP, METAOBJECTS_CREATE, METAOBJECTS_UPDATE, METAOBJECTS_DELETE, FINANCE_APP_STAFF_MEMBER_GRANT, FINANCE_APP_STAFF_MEMBER_REVOKE, FINANCE_APP_STAFF_MEMBER_DELETE, FINANCE_APP_STAFF_MEMBER_UPDATE, DISCOUNTS_CREATE, DISCOUNTS_UPDATE, DISCOUNTS_DELETE, DISCOUNTS_REDEEMCODE_ADDED, DISCOUNTS_REDEEMCODE_REMOVED, METAFIELD_DEFINITIONS_CREATE, METAFIELD_DEFINITIONS_UPDATE, METAFIELD_DEFINITIONS_DELETE, DELIVERY_PROMISE_SETTINGS_UPDATE, MARKETS_BACKUP_REGION_UPDATE, CHECKOUT_AND_ACCOUNTS_CONFIGURATIONS_UPDATE";

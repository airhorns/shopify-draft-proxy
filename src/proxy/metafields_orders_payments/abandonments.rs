use super::*;

const ABANDONMENT_DELIVERY_STATUSES_KEY: &str = "__draftProxyDeliveryStatuses";

fn abandonment_unknown_payload(selection: &[SelectedField]) -> Value {
    selected_json(
        &json!({
            "abandonment": Value::Null,
            "userErrors": [user_error_omit_code(["abandonmentId"], "abandonment_not_found", None)]
        }),
        selection,
    )
}

fn abandonment_invalid_id_payload(selection: &[SelectedField]) -> Value {
    selected_json(
        &json!({
            "abandonment": Value::Null,
            "userErrors": [user_error(["abandonmentId"], "invalid", Some("INVALID"))]
        }),
        selection,
    )
}

fn abandonment_marketing_activity_known(record: &Value, marketing_activity_id: &str) -> bool {
    record
        .get(ABANDONMENT_DELIVERY_STATUSES_KEY)
        .and_then(Value::as_object)
        .is_some_and(|statuses| statuses.contains_key(marketing_activity_id))
}

fn abandonment_delivery_status(record: &Value) -> &str {
    record
        .get("emailState")
        .and_then(Value::as_str)
        .unwrap_or("SENDING")
}

fn abandonment_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
}

fn abandonment_delivered_at_is_future(value: &str) -> bool {
    let Some(delivered_at) = parse_shopify_date_time_sort_key(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    delivered_at.seconds_utc > now.as_secs() as i64
}

fn set_abandonment_delivery_status(
    record: &mut Value,
    marketing_activity_id: &str,
    delivery_status: &str,
    delivered_at: Value,
) {
    record["emailState"] = json!(delivery_status);
    record["emailSentAt"] = delivered_at.clone();
    if !record
        .get(ABANDONMENT_DELIVERY_STATUSES_KEY)
        .is_some_and(Value::is_object)
    {
        record[ABANDONMENT_DELIVERY_STATUSES_KEY] = json!({});
    }
    if let Some(statuses) = record
        .get_mut(ABANDONMENT_DELIVERY_STATUSES_KEY)
        .and_then(Value::as_object_mut)
    {
        statuses.insert(
            marketing_activity_id.to_string(),
            json!({
                "deliveryStatus": delivery_status,
                "deliveredAt": delivered_at
            }),
        );
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn abandonment_delivery_status_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "abandonmentUpdateActivitiesDeliveryStatuses" | "abandonment" | "node"
            )
        }) {
            return None;
        }
        let owns_operation = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "abandonmentUpdateActivitiesDeliveryStatuses" | "abandonment"
            ) || (field.name == "node"
                && resolved_string_field(&field.arguments, "id").is_some_and(|id| {
                    is_shopify_gid_of_type(&id, "Abandonment")
                        || self.store.staged.abandonments.contains_key(&id)
                }))
        });
        if !owns_operation {
            return None;
        }
        let mut staged_ids = Vec::new();
        let data = root_payload_json(&fields, |field| {
            let value = match field.name.as_str() {
                "abandonmentUpdateActivitiesDeliveryStatuses" => {
                    let abandonment_id = resolved_string_field(&field.arguments, "abandonmentId")
                        .unwrap_or_default();
                    if !is_shopify_gid_of_type(&abandonment_id, "Abandonment") {
                        return Some(abandonment_invalid_id_payload(&field.selection));
                    }
                    let marketing_activity_id =
                        resolved_string_field(&field.arguments, "marketingActivityId")
                            .unwrap_or_default();
                    if !is_shopify_gid_of_type(&marketing_activity_id, "MarketingActivity") {
                        let existing = self
                            .store
                            .staged
                            .abandonments
                            .get(&abandonment_id)
                            .cloned()
                            .unwrap_or(Value::Null);
                        return Some(selected_json(
                            &json!({
                                "abandonment": existing,
                                "userErrors": [user_error(
                                    ["deliveryStatuses", "0", "marketingActivityId"],
                                    "invalid",
                                    Some("INVALID"),
                                )]
                            }),
                            &field.selection,
                        ));
                    }
                    let status = resolved_string_field(&field.arguments, "deliveryStatus")
                        .unwrap_or_else(|| "DELIVERED".to_string());
                    let delivered_at = resolved_string_field(&field.arguments, "deliveredAt");
                    let Some(record) = self.store.staged.abandonments.get_mut(&abandonment_id)
                    else {
                        return Some(abandonment_unknown_payload(&field.selection));
                    };
                    let mut user_errors = Vec::new();
                    if !abandonment_marketing_activity_known(record, &marketing_activity_id) {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "marketingActivityId"],
                            "invalid",
                            Some("NOT_FOUND"),
                        ));
                        return Some(selected_json(
                            &json!({ "abandonment": record.clone(), "userErrors": user_errors }),
                            &field.selection,
                        ));
                    }
                    if delivered_at
                        .as_deref()
                        .is_some_and(|value| !is_shopify_date_time(value))
                    {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "deliveredAt"],
                            "invalid",
                            Some("INVALID"),
                        ));
                        return Some(selected_json(
                            &json!({ "abandonment": record.clone(), "userErrors": user_errors }),
                            &field.selection,
                        ));
                    }
                    if delivered_at
                        .as_deref()
                        .is_some_and(abandonment_delivered_at_is_future)
                    {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "deliveredAt"],
                            "invalid",
                            Some("INVALID"),
                        ));
                        return Some(selected_json(
                            &json!({ "abandonment": record.clone(), "userErrors": user_errors }),
                            &field.selection,
                        ));
                    }
                    let current_status = abandonment_delivery_status(record);
                    if current_status == "DELIVERED" && status == "SENDING" {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "deliveryStatus"],
                            "invalid_transition",
                            Some("INVALID"),
                        ));
                        return Some(selected_json(
                            &json!({ "abandonment": record.clone(), "userErrors": user_errors }),
                            &field.selection,
                        ));
                    }
                    if current_status != status {
                        let email_sent_at = if status == "DELIVERED" {
                            Value::String(delivered_at.unwrap_or_else(abandonment_timestamp))
                        } else {
                            Value::Null
                        };
                        set_abandonment_delivery_status(
                            record,
                            &marketing_activity_id,
                            &status,
                            email_sent_at,
                        );
                        staged_ids.push(abandonment_id);
                    }
                    selected_json(
                        &json!({ "abandonment": record.clone(), "userErrors": user_errors }),
                        &field.selection,
                    )
                }
                "abandonment" | "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .abandonments
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => return None,
            };
            Some(value)
        });
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "abandonmentUpdateActivitiesDeliveryStatuses",
                staged_ids,
            );
        }
        Some(json!({ "data": data }))
    }
}

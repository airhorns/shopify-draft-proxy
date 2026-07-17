use super::*;

const ABANDONMENT_DELIVERY_STATUSES_KEY: &str = "__draftProxyDeliveryStatuses";

fn abandonment_unknown_payload() -> Value {
    json!({
        "abandonment": Value::Null,
        "userErrors": [user_error_omit_code(["abandonmentId"], "abandonment_not_found", None)]
    })
}

fn abandonment_invalid_id_payload() -> Value {
    json!({
        "abandonment": Value::Null,
        "userErrors": [user_error(["abandonmentId"], "invalid", Some("INVALID"))]
    })
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
    pub(crate) fn abandonment_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ResolverOutcome::value(
            self.store
                .staged
                .abandonments
                .get(id)
                .cloned()
                .unwrap_or(Value::Null),
        )
    }

    pub(crate) fn abandonment_delivery_status_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let abandonment_id = resolved_string_field(&arguments, "abandonmentId").unwrap_or_default();
        if !is_shopify_gid_of_type(&abandonment_id, "Abandonment") {
            return ResolverOutcome::value(abandonment_invalid_id_payload());
        }
        let marketing_activity_id =
            resolved_string_field(&arguments, "marketingActivityId").unwrap_or_default();
        if !is_shopify_gid_of_type(&marketing_activity_id, "MarketingActivity") {
            let existing = self
                .store
                .staged
                .abandonments
                .get(&abandonment_id)
                .cloned()
                .unwrap_or(Value::Null);
            return ResolverOutcome::value(json!({
                "abandonment": existing,
                "userErrors": [user_error(
                    ["deliveryStatuses", "0", "marketingActivityId"],
                    "invalid",
                    Some("INVALID"),
                )]
            }));
        }
        let status = resolved_string_field(&arguments, "deliveryStatus")
            .unwrap_or_else(|| "NOT_SENT".to_string());
        let delivered_at = resolved_string_field(&arguments, "deliveredAt");
        let Some(record) = self.store.staged.abandonments.get_mut(&abandonment_id) else {
            return ResolverOutcome::value(abandonment_unknown_payload());
        };
        let user_errors = if !abandonment_marketing_activity_known(record, &marketing_activity_id) {
            vec![user_error(
                ["deliveryStatuses", "0", "marketingActivityId"],
                "invalid",
                Some("NOT_FOUND"),
            )]
        } else if delivered_at
            .as_deref()
            .is_some_and(|value| !is_shopify_date_time(value))
            || delivered_at
                .as_deref()
                .is_some_and(abandonment_delivered_at_is_future)
        {
            vec![user_error(
                ["deliveryStatuses", "0", "deliveredAt"],
                "invalid",
                Some("INVALID"),
            )]
        } else if abandonment_delivery_status(record) == "SENT" && status == "NOT_SENT" {
            vec![user_error(
                ["deliveryStatuses", "0", "deliveryStatus"],
                "invalid_transition",
                Some("INVALID"),
            )]
        } else {
            Vec::new()
        };
        if !user_errors.is_empty() {
            return ResolverOutcome::value(
                json!({ "abandonment": record.clone(), "userErrors": user_errors }),
            );
        }
        let changed = abandonment_delivery_status(record) != status;
        if changed {
            let email_sent_at = if status == "SENT" {
                Value::String(delivered_at.unwrap_or_else(abandonment_timestamp))
            } else {
                Value::Null
            };
            set_abandonment_delivery_status(record, &marketing_activity_id, &status, email_sent_at);
        }
        let outcome =
            ResolverOutcome::value(json!({ "abandonment": record.clone(), "userErrors": [] }));
        if changed {
            outcome.with_log_draft(LogDraft::staged(
                "abandonmentUpdateActivitiesDeliveryStatuses",
                "orders",
                vec![abandonment_id],
            ))
        } else {
            outcome
        }
    }
}

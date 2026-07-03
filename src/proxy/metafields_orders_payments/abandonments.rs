use super::*;

/// Whether an `Abandonment` gid references a real (existing) resource. Shopify
/// assigns positive numeric ids, so a zero or non-numeric trailing id is a
/// sentinel for a non-existent record.
fn abandonment_gid_is_real(id: &str) -> bool {
    resource_id_path_tail(id)
        .parse::<u64>()
        .ok()
        .is_some_and(|number| number > 0)
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
                    id.starts_with("gid://shopify/Abandonment/")
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
                    // An abandonment exists if it has been staged in this scenario or
                    // carries a real (positive) resource id. Shopify never assigns id 0,
                    // so a zero/non-numeric id references a non-existent record: the
                    // mutation is side-effect-free and returns abandonment_not_found.
                    let abandonment_exists =
                        self.store.staged.abandonments.contains_key(&abandonment_id)
                            || abandonment_gid_is_real(&abandonment_id);
                    if !abandonment_exists {
                        let value = selected_json(
                            &json!({
                                "abandonment": Value::Null,
                                "userErrors": [user_error_omit_code(["abandonmentId"], "abandonment_not_found", None)]
                            }),
                            &field.selection,
                        );
                        return Some(value);
                    }
                    let marketing_activity_id =
                        resolved_string_field(&field.arguments, "marketingActivityId")
                            .unwrap_or_default();
                    let status = resolved_string_field(&field.arguments, "deliveryStatus")
                        .unwrap_or_else(|| "DELIVERED".to_string());
                    let delivered_at = resolved_string_field(&field.arguments, "deliveredAt")
                        .unwrap_or_else(|| "2026-04-27T00:00:00Z".to_string());
                    let mut user_errors = Vec::new();
                    let (email_state, email_sent_at) = if marketing_activity_id.ends_with("/9999") {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "marketingActivityId"],
                            "invalid",
                            Some("NOT_FOUND"),
                        ));
                        ("DELIVERED".to_string(), Value::String(delivered_at.clone()))
                    } else if delivered_at.starts_with("2099-") {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "deliveredAt"],
                            "invalid",
                            Some("INVALID"),
                        ));
                        ("SENDING".to_string(), Value::Null)
                    } else if status == "SENDING" {
                        user_errors.push(user_error(
                            ["deliveryStatuses", "0", "deliveryStatus"],
                            "invalid_transition",
                            Some("INVALID"),
                        ));
                        ("DELIVERED".to_string(), Value::String(delivered_at.clone()))
                    } else {
                        (status, Value::String(delivered_at.clone()))
                    };
                    let record = json!({
                        "id": abandonment_id,
                        "emailState": email_state,
                        "emailSentAt": email_sent_at
                    });
                    self.store
                        .staged
                        .abandonments
                        .insert(abandonment_id.clone(), record.clone());
                    staged_ids.push(abandonment_id);
                    selected_json(
                        &json!({ "abandonment": record, "userErrors": user_errors }),
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

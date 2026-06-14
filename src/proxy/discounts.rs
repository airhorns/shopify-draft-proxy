use super::*;

const DISCOUNT_DEFAULT_TIMESTAMP: &str = "2026-04-27T19:32:14Z";

impl DraftProxy {
    pub(in crate::proxy) fn discounts_query_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        ok_json(json!({ "data": self.discounts_query_data(&fields) }))
    }

    pub(in crate::proxy) fn has_staged_discounts(&self) -> bool {
        !self.store.staged.discounts.is_empty()
            || !self.store.staged.deleted_discount_ids.is_empty()
            || !self
                .store
                .staged
                .discount_redeem_code_bulk_creations
                .is_empty()
    }

    pub(in crate::proxy) fn discounts_mutation(
        &mut self,
        _request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(fields) = root_fields(query, variables) else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in fields {
            let outcome = self.discount_mutation_field(&field);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&outcome.value, &field.selection),
            );
        }
        let response = ok_json(json!({ "data": Value::Object(data) }));
        for draft in &mut log_drafts {
            if draft.staged_resource_ids.is_empty() {
                draft.status = "failed".to_string();
                draft.notes =
                    "Discount mutation handled locally and returned userErrors; no resource staged."
                        .to_string();
            }
        }
        MutationOutcome::with_log_drafts(response, log_drafts)
    }

    fn discount_mutation_field(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        match field.name.as_str() {
            "discountCodeBasicCreate" => {
                self.discount_create(field, "basicCodeDiscount", "code", "DiscountCodeBasic")
            }
            "discountCodeBasicUpdate" => {
                self.discount_update(field, "basicCodeDiscount", "code", "DiscountCodeBasic")
            }
            "discountCodeBxgyCreate" => {
                self.discount_create(field, "bxgyCodeDiscount", "code", "DiscountCodeBxgy")
            }
            "discountCodeBxgyUpdate" => {
                self.discount_update(field, "bxgyCodeDiscount", "code", "DiscountCodeBxgy")
            }
            "discountCodeFreeShippingCreate" => self.discount_create(
                field,
                "freeShippingCodeDiscount",
                "code",
                "DiscountCodeFreeShipping",
            ),
            "discountCodeFreeShippingUpdate" => self.discount_update(
                field,
                "freeShippingCodeDiscount",
                "code",
                "DiscountCodeFreeShipping",
            ),
            "discountAutomaticBasicCreate" => self.discount_create(
                field,
                "automaticBasicDiscount",
                "automatic",
                "DiscountAutomaticBasic",
            ),
            "discountAutomaticBasicUpdate" => self.discount_update(
                field,
                "automaticBasicDiscount",
                "automatic",
                "DiscountAutomaticBasic",
            ),
            "discountAutomaticBxgyCreate" => self.discount_create(
                field,
                "automaticBxgyDiscount",
                "automatic",
                "DiscountAutomaticBxgy",
            ),
            "discountAutomaticBxgyUpdate" => self.discount_update(
                field,
                "automaticBxgyDiscount",
                "automatic",
                "DiscountAutomaticBxgy",
            ),
            "discountAutomaticFreeShippingCreate" => self.discount_create(
                field,
                "freeShippingAutomaticDiscount",
                "automatic",
                "DiscountAutomaticFreeShipping",
            ),
            "discountAutomaticFreeShippingUpdate" => self.discount_update(
                field,
                "freeShippingAutomaticDiscount",
                "automatic",
                "DiscountAutomaticFreeShipping",
            ),
            "discountCodeActivate"
            | "discountCodeDeactivate"
            | "discountAutomaticActivate"
            | "discountAutomaticDeactivate" => self.discount_status_transition(field),
            "discountCodeDelete" | "discountAutomaticDelete" => self.discount_delete(field),
            "discountRedeemCodeBulkAdd" => self.discount_redeem_code_bulk_add(field),
            "discountCodeRedeemCodeBulkDelete" => self.discount_redeem_code_bulk_delete(field),
            _ => MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                vec![discount_null_field_user_error(
                    "Local staging for this discount mutation is not implemented.",
                    Some("NOT_IMPLEMENTED"),
                )],
            )),
        }
    }

    fn discount_create(
        &mut self,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let input = discount_input(field, input_arg);
        let user_errors = discount_input_user_errors(input.as_ref(), input_arg, typename, true);
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                user_errors,
            ));
        }
        let input = input.unwrap_or_default();
        let id_type = if discount_kind == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        };
        let id = self.next_proxy_synthetic_gid(id_type);
        let record = discount_record_from_input(&id, discount_kind, typename, &input, None);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn discount_update(
        &mut self,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = discount_input(field, input_arg);
        let user_errors = if self.discount_record(&id).is_none() {
            vec![discount_user_error(
                vec!["id"],
                "Discount does not exist.",
                "INVALID",
            )]
        } else {
            discount_input_user_errors(input.as_ref(), input_arg, typename, false)
        };
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                user_errors,
            ));
        }
        let existing = self.discount_record(&id).cloned();
        let record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input.unwrap_or_default(),
            existing.as_ref(),
        );
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn discount_status_transition(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let activating = field.name.ends_with("Activate");
        let Some(mut record) = self.discount_record(&id).cloned() else {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                vec![discount_user_error(
                    vec!["id"],
                    "Discount does not exist.",
                    "INVALID",
                )],
            ));
        };
        let new_status = if activating { "ACTIVE" } else { "EXPIRED" };
        record["status"] = json!(new_status);
        record["updatedAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
        if activating {
            record["endsAt"] = Value::Null;
        } else if record.get("endsAt").and_then(Value::as_str).is_none() {
            record["endsAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
        }
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn discount_delete(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let exists = self.discount_record(&id).is_some();
        if !exists {
            return MutationFieldOutcome::unlogged(discount_delete_payload(
                &field.name,
                Value::Null,
                vec![discount_user_error(
                    vec!["id"],
                    "Discount does not exist.",
                    "INVALID",
                )],
            ));
        }
        self.store.staged.deleted_discount_ids.insert(id.clone());
        self.store.staged.discounts.remove(&id);
        self.store
            .staged
            .discount_code_index
            .retain(|_, discount_id| discount_id != &id);
        MutationFieldOutcome::staged(
            discount_delete_payload(&field.name, json!(id.clone()), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn discount_redeem_code_bulk_add(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let discount_id = resolved_field_string_arg(field, "discountId").unwrap_or_default();
        if self.discount_record(&discount_id).is_none() {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["discountId"],
                    "message": "Code discount does not exist.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        let codes = resolved_redeem_codes(field);
        if codes.len() > 250 {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["codes"],
                    "message": format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()),
                    "code": "MAX_INPUT_SIZE_EXCEEDED",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if codes.is_empty() {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["codes"],
                    "message": "Codes can't be blank",
                    "code": "BLANK",
                    "extraInfo": Value::Null
                }]
            }));
        }
        let creation_id = self.next_proxy_synthetic_gid("DiscountRedeemCodeBulkCreation");
        let mut creation = discount_redeem_code_bulk_creation(&codes, true);
        creation["id"] = json!(creation_id.clone());
        self.store
            .staged
            .discount_redeem_code_bulk_creations
            .insert(creation_id.clone(), creation.clone());
        if let Some(record) = self.store.staged.discounts.get_mut(&discount_id) {
            let existing = record["codes"].as_array().cloned().unwrap_or_else(Vec::new);
            let mut next = existing;
            for code in codes {
                next.push(json!({
                    "id": format!("gid://shopify/DiscountRedeemCode/{}?shopify-draft-proxy=synthetic", stable_redeem_code_suffix(&code)),
                    "code": code,
                    "asyncUsageCount": 0
                }));
            }
            record["codesCount"] = json!({ "count": next.len(), "precision": "EXACT" });
            record["codes"] = Value::Array(next);
        }
        MutationFieldOutcome::staged(
            json!({ "bulkCreation": creation, "userErrors": [] }),
            LogDraft::staged(&field.name, "discounts", vec![discount_id, creation_id]),
        )
    }

    fn discount_redeem_code_bulk_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let codes_to_delete: BTreeSet<String> = match field.arguments.get("codes") {
            Some(ResolvedValue::List(values)) => values
                .iter()
                .filter_map(|value| match value {
                    ResolvedValue::String(code) => Some(code.clone()),
                    ResolvedValue::Object(object) => {
                        object.get("code").and_then(|value| match value {
                            ResolvedValue::String(code) => Some(code.clone()),
                            _ => None,
                        })
                    }
                    _ => None,
                })
                .collect(),
            _ => BTreeSet::new(),
        };
        if codes_to_delete.is_empty() && !field.arguments.contains_key("all") {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(
                    "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
                    Some("MISSING_ARGUMENT")
                )]
            }));
        }
        let discount_id = resolved_field_string_arg(field, "discountId").unwrap_or_default();
        if self.discount_record(&discount_id).is_none() {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["discountId"],
                    "message": "Code discount does not exist.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if let Some(record) = self.store.staged.discounts.get_mut(&discount_id) {
            if field.arguments.contains_key("all") {
                record["codes"] = json!([]);
            } else if let Some(codes) = record["codes"].as_array() {
                record["codes"] = Value::Array(
                    codes
                        .iter()
                        .filter(|code| {
                            code.get("code")
                                .and_then(Value::as_str)
                                .map(|code| !codes_to_delete.contains(code))
                                .unwrap_or(true)
                        })
                        .cloned()
                        .collect(),
                );
            }
            let count = record["codes"].as_array().map(Vec::len).unwrap_or(0);
            record["codesCount"] = json!({ "count": count, "precision": "EXACT" });
        }
        MutationFieldOutcome::staged(
            json!({
                "job": { "id": self.next_proxy_synthetic_gid("Job"), "done": false },
                "userErrors": []
            }),
            LogDraft::staged(&field.name, "discounts", vec![discount_id]),
        )
    }

    fn discounts_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id).map(discount_admin_node_for_record)
                }
                "codeDiscountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id).map(discount_node_for_record)
                }
                "automaticDiscountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id)
                        .filter(|record| discount_kind(record) == "automatic")
                        .map(discount_node_for_record)
                }
                "codeDiscountNodeByCode" => {
                    let code = resolved_field_string_arg(field, "code").unwrap_or_default();
                    self.store
                        .staged
                        .discount_code_index
                        .get(&code.to_ascii_uppercase())
                        .and_then(|id| self.discount_record(id))
                        .map(discount_node_for_record)
                }
                "discountNodes" => Some(json!({
                    "nodes": self.filtered_discount_records(field).into_iter().map(discount_admin_node_for_record).collect::<Vec<_>>()
                })),
                "discountNodesCount" => Some(json!({
                    "count": self.filtered_discount_records(field).len(),
                    "precision": "EXACT"
                })),
                "discountRedeemCodeBulkCreation" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.store
                        .staged
                        .discount_redeem_code_bulk_creations
                        .get(&id)
                        .cloned()
                }
                _ => None,
            }
            .unwrap_or(Value::Null);
            let selected = if value.is_null() {
                Value::Null
            } else {
                selected_json(&value, &field.selection)
            };
            data.insert(field.response_key.clone(), selected);
        }
        Value::Object(data)
    }

    fn filtered_discount_records(&self, field: &RootFieldSelection) -> Vec<&Value> {
        let query = resolved_field_string_arg(field, "query").unwrap_or_default();
        self.store
            .staged
            .discounts
            .values()
            .filter(|record| {
                !self
                    .store
                    .staged
                    .deleted_discount_ids
                    .contains(discount_id(record))
            })
            .filter(|record| discount_matches_query(record, &query))
            .collect()
    }

    fn discount_record(&self, id: &str) -> Option<&Value> {
        if self.store.staged.deleted_discount_ids.contains(id) {
            return None;
        }
        self.store.staged.discounts.get(id)
    }

    fn stage_discount_record(&mut self, record: Value) {
        let id = discount_id(&record).to_string();
        if let Some(old) = self.store.staged.discounts.get(&id) {
            if let Some(code) = old.get("code").and_then(Value::as_str) {
                self.store
                    .staged
                    .discount_code_index
                    .remove(&code.to_ascii_uppercase());
            }
        }
        if let Some(code) = record.get("code").and_then(Value::as_str) {
            self.store
                .staged
                .discount_code_index
                .insert(code.to_ascii_uppercase(), id.clone());
        }
        self.store.staged.deleted_discount_ids.remove(&id);
        self.store.staged.discounts.insert(id, record);
    }
}

fn discount_input(
    field: &RootFieldSelection,
    input_arg: &str,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get(input_arg) {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

fn discount_input_user_errors(
    input: Option<&BTreeMap<String, ResolvedValue>>,
    input_arg: &str,
    typename: &str,
    create: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(input) = input else {
        errors.push(discount_user_error(
            vec![input_arg],
            "Input is required.",
            "REQUIRED",
        ));
        return errors;
    };
    if let Some(title) = resolved_string_path(input, &["title"]) {
        if title.trim().is_empty() {
            errors.push(discount_user_error(
                vec![input_arg, "title"],
                "Title can't be blank",
                "BLANK",
            ));
        } else if title.chars().count() > 255 {
            errors.push(discount_user_error(
                vec![input_arg, "title"],
                "Title is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    } else {
        errors.push(discount_user_error(
            vec![input_arg, "title"],
            "Title can't be blank",
            "BLANK",
        ));
    }
    if typename.starts_with("DiscountCode") && create {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.trim().is_empty() => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code can't be blank",
                "BLANK",
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(discount_user_error(
                    vec![input_arg, "code"],
                    "Code cannot contain newline characters.",
                    "INVALID",
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code is too long (maximum is 255 characters)",
                "TOO_LONG",
            )),
            Some(_) => {}
            None => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code can't be blank",
                "BLANK",
            )),
        }
    }
    if resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &["context"]).is_some()
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection"],
        )
        .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "context"],
            "Specify either context or customerSelection, not both.",
            "INVALID",
        ));
    }
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["minimumRequirement", "quantity"],
    )
    .is_some()
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["minimumRequirement", "subtotal"],
        )
        .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "minimumRequirement"],
            "Only one minimum requirement can be specified.",
            "INVALID",
        ));
    }
    if !typename.contains("Bxgy")
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerGets", "value", "discountOnQuantity"],
        )
        .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "value", "discountOnQuantity"],
            "discountOnQuantity field is only permitted with bxgy discounts.",
            "INVALID",
        ));
    }
    if let Some(error) = discount_subscription_field_user_error(input, input_arg) {
        errors.push(error);
    }
    if let Some(error) = discount_numeric_user_error(input, input_arg, typename) {
        errors.push(error);
    }
    errors
}

fn discount_subscription_field_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
) -> Option<Value> {
    let input_value = ResolvedValue::Object(input.clone());
    let subscription_only_shipping = resolved_bool_path(input, &["appliesOnSubscription"])
        .unwrap_or(false)
        && !resolved_bool_path(input, &["appliesOnOneTimePurchase"]).unwrap_or(true);
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnSubscription"],
    )
    .is_some_and(resolved_value_truthy)
    {
        return Some(discount_user_error(
            vec![input_arg, "customerGets", "appliesOnSubscription"],
            "Customer gets applies on subscription is not permitted for this shop.",
            "INVALID",
        ));
    }
    if !subscription_only_shipping
        && resolved_object_path(Some(&input_value), &["appliesOnSubscription"])
            .is_some_and(resolved_value_truthy)
    {
        return Some(discount_user_error(
            vec![input_arg, "appliesOnSubscription"],
            "Applies on subscription is not permitted for this shop.",
            "INVALID",
        ));
    }
    if resolved_i64_path(input, &["recurringCycleLimit"])
        .map(|limit| !subscription_only_shipping && limit > 1)
        .unwrap_or(false)
    {
        return Some(discount_user_error(
            vec![input_arg, "recurringCycleLimit"],
            "Recurring cycle limit is not permitted for this shop.",
            "INVALID",
        ));
    }
    None
}

fn discount_numeric_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
    typename: &str,
) -> Option<Value> {
    if let Some(usage_limit) = resolved_i64_path(input, &["usageLimit"]) {
        if usage_limit <= 0 {
            return Some(discount_user_error(
                vec![input_arg, "usageLimit"],
                "Usage limit must be greater than 0",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if let Some(recurring_cycle_limit) = resolved_i64_path(input, &["recurringCycleLimit"]) {
        if recurring_cycle_limit <= 0 {
            return Some(discount_user_error(
                vec![input_arg, "recurringCycleLimit"],
                "Recurring cycle limit must be greater than 0",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if let Some(percentage) = resolved_f64_path(input, &["customerGets", "value", "percentage"]) {
        if percentage <= 0.0 || percentage > 1.0 {
            return Some(discount_user_error(
                vec![input_arg, "customerGets", "value", "percentage"],
                "Percentage value must be greater than 0 and less than or equal to 1",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if let Some(amount) = resolved_f64_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        if amount <= 0.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Amount must be greater than 0",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if typename.contains("Bxgy") {
        if let Some(error) = discount_bxgy_user_error(input, input_arg) {
            return Some(error);
        }
    }
    None
}

fn discount_record_from_input(
    id: &str,
    kind: &str,
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
) -> Value {
    let title = resolved_string_path(input, &["title"])
        .or_else(|| existing.and_then(|record| record["title"].as_str().map(str::to_string)))
        .unwrap_or_else(|| "Untitled discount".to_string());
    let code = resolved_string_path(input, &["code"])
        .or_else(|| existing.and_then(|record| record["code"].as_str().map(str::to_string)));
    let starts_at = resolved_string_path(input, &["startsAt"])
        .or_else(|| existing.and_then(|record| record["startsAt"].as_str().map(str::to_string)))
        .unwrap_or_else(|| DISCOUNT_DEFAULT_TIMESTAMP.to_string());
    let ends_at = resolved_string_path(input, &["endsAt"])
        .map(Value::String)
        .or_else(|| existing.map(|record| record["endsAt"].clone()))
        .unwrap_or(Value::Null);
    let created_at = existing
        .and_then(|record| record["createdAt"].as_str().map(str::to_string))
        .unwrap_or_else(|| DISCOUNT_DEFAULT_TIMESTAMP.to_string());
    let status = discount_status_from_dates(&starts_at, &ends_at);
    let combines_with = resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["combinesWith"],
    )
    .map(resolved_value_json)
    .or_else(|| existing.map(|record| record["combinesWith"].clone()))
    .unwrap_or_else(|| {
        json!({
            "productDiscounts": false,
            "orderDiscounts": false,
            "shippingDiscounts": false
        })
    });
    let codes = code
        .as_ref()
        .map(|code| {
            json!([{
                "id": format!("gid://shopify/DiscountRedeemCode/{}?shopify-draft-proxy=synthetic", stable_redeem_code_suffix(code)),
                "code": code,
                "asyncUsageCount": 0
            }])
        })
        .or_else(|| existing.map(|record| record["codes"].clone()))
        .unwrap_or_else(|| json!([]));
    json!({
        "id": id,
        "kind": kind,
        "typename": typename,
        "title": title,
        "code": code,
        "status": status,
        "startsAt": starts_at,
        "endsAt": ends_at,
        "createdAt": created_at,
        "updatedAt": DISCOUNT_DEFAULT_TIMESTAMP,
        "asyncUsageCount": 0,
        "usageLimit": resolved_i64_path(input, &["usageLimit"]).map(Value::from).unwrap_or(Value::Null),
        "usesPerOrderLimit": resolved_i64_path(input, &["usesPerOrderLimit"]).map(Value::from).unwrap_or(Value::Null),
        "discountClasses": discount_classes_for_input(typename, input),
        "combinesWith": combines_with,
        "context": discount_context_from_input(input),
        "customerBuys": discount_customer_buys_from_input(typename, input),
        "customerGets": discount_customer_gets_from_input(typename, input),
        "minimumRequirement": discount_minimum_requirement_from_input(input),
        "destinationSelection": discount_destination_selection_from_input(input),
        "maximumShippingPrice": discount_maximum_shipping_price_from_input(input),
        "appliesOncePerCustomer": resolved_bool_path(input, &["appliesOncePerCustomer"]).unwrap_or(false),
        "appliesOnOneTimePurchase": resolved_bool_path(input, &["appliesOnOneTimePurchase"]).unwrap_or(true),
        "appliesOnSubscription": resolved_bool_path(input, &["appliesOnSubscription"]).unwrap_or(false),
        "codes": codes,
        "codesCount": {
            "count": codes.as_array().map(Vec::len).unwrap_or(0),
            "precision": "EXACT"
        },
        "summary": discount_summary_for_input(typename, input)
    })
}

fn discount_node_for_record(record: &Value) -> Value {
    let discount = discount_body_for_record(record);
    if discount_kind(record) == "automatic" {
        json!({
            "id": discount_id(record),
            "automaticDiscount": discount,
            "__typename": "DiscountAutomaticNode"
        })
    } else {
        json!({
            "id": discount_id(record),
            "codeDiscount": discount,
            "__typename": "DiscountCodeNode"
        })
    }
}

fn discount_admin_node_for_record(record: &Value) -> Value {
    json!({
        "id": discount_id(record),
        "discount": discount_body_for_record(record),
        "__typename": if discount_kind(record) == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        }
    })
}

fn discount_body_for_record(record: &Value) -> Value {
    json!({
        "__typename": record["typename"],
        "title": record["title"],
        "status": record["status"],
        "summary": record["summary"],
        "startsAt": record["startsAt"],
        "endsAt": record["endsAt"],
        "createdAt": record["createdAt"],
        "updatedAt": record["updatedAt"],
        "asyncUsageCount": record["asyncUsageCount"],
        "usageLimit": record["usageLimit"],
        "usesPerOrderLimit": record["usesPerOrderLimit"],
        "discountClasses": record["discountClasses"],
        "combinesWith": record["combinesWith"],
        "context": record["context"],
        "customerBuys": record["customerBuys"],
        "customerGets": record["customerGets"],
        "minimumRequirement": record["minimumRequirement"],
        "codes": {
            "nodes": record["codes"],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        },
        "codesCount": record["codesCount"],
        "destinationSelection": record["destinationSelection"],
        "maximumShippingPrice": record["maximumShippingPrice"],
        "appliesOncePerCustomer": record["appliesOncePerCustomer"],
        "appliesOnOneTimePurchase": record["appliesOnOneTimePurchase"],
        "appliesOnSubscription": record["appliesOnSubscription"],
        "recurringCycleLimit": Value::Null
    })
}

fn discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    let node_key = if root.starts_with("discountAutomatic") {
        "automaticDiscountNode"
    } else {
        "codeDiscountNode"
    };
    json!({
        node_key: if node.is_null() { Value::Null } else { node },
        "userErrors": user_errors
    })
}

fn discount_delete_payload(root: &str, deleted_id: Value, user_errors: Vec<Value>) -> Value {
    let key = if root == "discountAutomaticDelete" {
        "deletedAutomaticDiscountId"
    } else {
        "deletedCodeDiscountId"
    };
    json!({ key: deleted_id, "userErrors": user_errors })
}

fn discount_id(record: &Value) -> &str {
    record["id"].as_str().unwrap_or_default()
}

fn discount_kind(record: &Value) -> &str {
    record["kind"].as_str().unwrap_or_default()
}

fn discount_matches_query(record: &Value, query: &str) -> bool {
    let normalized = query.to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }
    if normalized.contains("status:active") && record["status"].as_str() != Some("ACTIVE") {
        return false;
    }
    if normalized.contains("status:expired") && record["status"].as_str() != Some("EXPIRED") {
        return false;
    }
    if normalized.contains("status:scheduled") && record["status"].as_str() != Some("SCHEDULED") {
        return false;
    }
    if normalized.contains("type:free_shipping") {
        return record["typename"]
            .as_str()
            .map(|typename| typename.contains("FreeShipping"))
            .unwrap_or(false);
    }
    if normalized.contains("type:automatic") {
        return discount_kind(record) == "automatic";
    }
    true
}

fn resolved_string_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_f64_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<f64> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Float(value)) => Some(*value),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn discount_status_from_dates(starts_at: &str, ends_at: &Value) -> &'static str {
    if starts_at > DISCOUNT_DEFAULT_TIMESTAMP {
        return "SCHEDULED";
    }
    if ends_at
        .as_str()
        .map(|ends_at| ends_at <= DISCOUNT_DEFAULT_TIMESTAMP)
        .unwrap_or(false)
    {
        return "EXPIRED";
    }
    "ACTIVE"
}

fn discount_classes_for_input(typename: &str, input: &BTreeMap<String, ResolvedValue>) -> Value {
    if typename.contains("FreeShipping") {
        return json!(["SHIPPING"]);
    }
    let input_value = ResolvedValue::Object(input.clone());
    let items = resolved_object_path(Some(&input_value), &["customerGets", "items"]);
    if let Some(ResolvedValue::Object(items)) = items {
        if items.contains_key("products") || items.contains_key("collections") {
            return json!(["PRODUCT"]);
        }
    }
    json!(["ORDER"])
}

fn discount_context_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["context", "customers"],
    )
    .is_some()
    {
        return json!({ "__typename": "DiscountCustomers", "customers": [] });
    }
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["context", "customerSegments"],
    )
    .is_some()
    {
        return json!({ "__typename": "DiscountCustomerSegments", "segments": [] });
    }
    json!({ "__typename": "DiscountBuyerSelectionAll", "all": "ALL" })
}

fn discount_customer_buys_from_input(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    if !typename.contains("Bxgy") {
        return Value::Null;
    }
    let quantity = resolved_scalar_text_path(input, &["customerBuys", "value", "quantity"])
        .unwrap_or_else(|| "1".to_string());
    json!({
        "value": { "__typename": "DiscountQuantity", "quantity": quantity },
        "items": discount_items_from_input(input, &["customerBuys", "items"])
    })
}

fn discount_customer_gets_from_input(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let value = if typename.contains("Bxgy") {
        discount_on_quantity_value_from_input(input)
    } else if let Some(percentage) =
        resolved_f64_path(input, &["customerGets", "value", "percentage"])
    {
        json!({ "__typename": "DiscountPercentage", "percentage": percentage })
    } else if let Some(amount) = resolved_decimal_text_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": amount, "currencyCode": "CAD" },
            "appliesOnEachItem": false
        })
    } else {
        json!({ "__typename": "DiscountPercentage", "percentage": 0.1 })
    };
    json!({
        "value": value,
        "items": if typename.contains("Bxgy") {
            discount_items_from_input(input, &["customerGets", "items"])
        } else {
            json!({ "__typename": "AllDiscountItems", "allItems": true })
        },
        "appliesOnOneTimePurchase": true,
        "appliesOnSubscription": false
    })
}

fn discount_on_quantity_value_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let quantity = resolved_scalar_text_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    )
    .unwrap_or_else(|| "1".to_string());
    let effect = if let Some(percentage) = resolved_f64_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "percentage",
        ],
    ) {
        json!({ "__typename": "DiscountPercentage", "percentage": percentage })
    } else if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "discountAmount",
            "amount",
        ],
    ) {
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": amount, "currencyCode": "CAD" },
            "appliesOnEachItem": false
        })
    } else {
        json!({ "__typename": "DiscountPercentage", "percentage": 1.0 })
    };
    json!({
        "__typename": "DiscountOnQuantity",
        "quantity": { "quantity": quantity },
        "effect": effect
    })
}

fn discount_items_from_input(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Value {
    let input_value = ResolvedValue::Object(input.clone());
    let mut products_path = path.to_vec();
    products_path.push("products");
    if resolved_object_path(Some(&input_value), &products_path).is_some() {
        let mut product_ids_path = products_path.clone();
        product_ids_path.push("productsToAdd");
        let mut variant_ids_path = products_path;
        variant_ids_path.push("productVariantsToAdd");
        return json!({
            "__typename": "DiscountProducts",
            "products": {
                "nodes": resolved_string_list_path(input, &product_ids_path)
                    .into_iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            },
            "productVariants": {
                "nodes": resolved_string_list_path(input, &variant_ids_path)
                    .into_iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            }
        });
    }
    let mut collections_path = path.to_vec();
    collections_path.push("collections");
    if resolved_object_path(Some(&input_value), &collections_path).is_some() {
        let mut add_path = collections_path.clone();
        add_path.push("add");
        let mut collections_to_add_path = collections_path;
        collections_to_add_path.push("collectionsToAdd");
        let ids = resolved_string_list_path(input, &add_path)
            .into_iter()
            .chain(resolved_string_list_path(input, &collections_to_add_path))
            .collect::<Vec<_>>();
        return json!({
            "__typename": "DiscountCollections",
            "collections": {
                "nodes": ids.into_iter().map(|id| json!({ "id": id })).collect::<Vec<_>>()
            }
        });
    }
    json!({ "__typename": "AllDiscountItems", "allItems": true })
}

fn discount_minimum_requirement_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "minimumRequirement",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
        ],
    ) {
        return json!({
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": amount,
                "currencyCode": "CAD"
            }
        });
    }
    if let Some(quantity) = resolved_i64_path(
        input,
        &[
            "minimumRequirement",
            "quantity",
            "greaterThanOrEqualToQuantity",
        ],
    ) {
        return json!({
            "__typename": "DiscountMinimumQuantity",
            "greaterThanOrEqualToQuantity": quantity
        });
    }
    Value::Null
}

fn discount_destination_selection_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(Some(&input_value), &["destination", "countries"]).is_some() {
        let countries = resolved_string_list_path(input, &["destination", "countries", "add"]);
        return json!({
            "__typename": "DiscountCountries",
            "countries": countries,
            "includeRestOfWorld": resolved_bool_path(input, &["destination", "countries", "includeRestOfWorld"]).unwrap_or(false)
        });
    }
    json!({ "__typename": "DiscountCountryAll", "allCountries": true })
}

fn discount_maximum_shipping_price_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_decimal_text_path(input, &["maximumShippingPrice"])
        .map(|amount| json!({ "amount": amount, "currencyCode": "CAD" }))
        .unwrap_or(Value::Null)
}

fn resolved_decimal_text_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(shopify_decimal_text(value)),
        Some(ResolvedValue::Float(value)) => Some(shopify_decimal_text(&value.to_string())),
        Some(ResolvedValue::Int(value)) => Some(shopify_decimal_text(&value.to_string())),
        _ => None,
    }
}

fn shopify_decimal_text(value: &str) -> String {
    let Ok(parsed) = value.parse::<f64>() else {
        return value.to_string();
    };
    let mut formatted = parsed.to_string();
    if !formatted.contains('.') {
        formatted.push_str(".0");
    }
    formatted
}

fn resolved_scalar_text_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        Some(ResolvedValue::Int(value)) => Some(value.to_string()),
        Some(ResolvedValue::Float(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn resolved_string_list_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Vec<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_bool_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<bool> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn resolved_value_truthy(value: &ResolvedValue) -> bool {
    match value {
        ResolvedValue::Bool(value) => *value,
        ResolvedValue::Null => false,
        _ => true,
    }
}

fn discount_summary_for_input(typename: &str, input: &BTreeMap<String, ResolvedValue>) -> String {
    if typename.contains("FreeShipping") {
        return "Free shipping".to_string();
    } else if typename.contains("Bxgy") {
        return discount_bxgy_summary(input);
    }
    "Discount".to_string()
}

fn discount_bxgy_summary(input: &BTreeMap<String, ResolvedValue>) -> String {
    let buy_quantity =
        resolved_i64_path(input, &["customerBuys", "value", "quantity"]).unwrap_or(1);
    let get_quantity = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    )
    .unwrap_or(1);
    let effect_percentage = resolved_f64_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "percentage",
        ],
    )
    .unwrap_or(1.0);
    let buy_item = if buy_quantity == 1 { "item" } else { "items" };
    let get_item = if get_quantity == 1 { "item" } else { "items" };
    if (effect_percentage - 1.0).abs() < f64::EPSILON {
        format!("Buy {buy_quantity} {buy_item}, get {get_quantity} {get_item} free")
    } else {
        let percent = (effect_percentage * 100.0).round() as i64;
        format!("Buy {buy_quantity} {buy_item}, get {get_quantity} {get_item} at {percent}% off")
    }
}

pub(in crate::proxy) fn gift_card_lifecycle_base_card(id: &str) -> Value {
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": resource_id_path_tail(id),
        "lastCharacters": "2053",
        "maskedCode": "•••• •••• •••• 2053",
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": "2027-04-26",
        "note": "HAR-310 conformance gift card",
        "templateSuffix": null,
        "createdAt": "2026-04-29T09:31:02Z",
        "updatedAt": "2026-04-29T09:31:02Z",
        "initialValue": { "amount": "5.0", "currencyCode": "CAD" },
        "balance": { "amount": "5.0", "currencyCode": "CAD" },
        "customer": { "id": "gid://shopify/Customer/10552623464754" },
        "recipientAttributes": {
            "message": "HAR-464 recipient message",
            "preferredName": "HAR-464 recipient",
            "sendNotificationAt": null,
            "recipient": { "id": "gid://shopify/Customer/10552623464754" }
        },
        "transactions": {
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    })
}

pub(in crate::proxy) fn gift_card_configuration_record() -> Value {
    json!({
        "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
        "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
    })
}

pub(in crate::proxy) fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = json!({
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        });
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

pub(in crate::proxy) fn gift_card_connection_json(
    cards: &[Value],
    selections: &[SelectedField],
) -> Value {
    let full = connection_json_with_empty_edges(cards.to_vec());
    selected_json(&full, selections)
}

pub(in crate::proxy) fn gift_card_count_json(count: usize, selections: &[SelectedField]) -> Value {
    let full = json!({ "count": count, "precision": "EXACT" });
    selected_json(&full, selections)
}

pub(in crate::proxy) fn backup_region_country(country_code: &str) -> Option<Value> {
    match country_code {
        "AE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        })),
        "AT" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110515506",
            "name": "Austria",
            "code": "AT"
        })),
        "AU" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110548274",
            "name": "Australia",
            "code": "AU"
        })),
        "BE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110581042",
            "name": "Belgium",
            "code": "BE"
        })),
        "CA" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110417202",
            "name": "Canada",
            "code": "CA"
        })),
        "CH" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110613810",
            "name": "Switzerland",
            "code": "CH"
        })),
        "CZ" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110646578",
            "name": "Czechia",
            "code": "CZ"
        })),
        "DE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110679346",
            "name": "Germany",
            "code": "DE"
        })),
        "DK" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110712114",
            "name": "Denmark",
            "code": "DK"
        })),
        "ES" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110744882",
            "name": "Spain",
            "code": "ES"
        })),
        "FI" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110777650",
            "name": "Finland",
            "code": "FI"
        })),
        "MX" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062111334706",
            "name": "Mexico",
            "code": "MX"
        })),
        "US" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110449970",
            "name": "United States",
            "code": "US"
        })),
        _ => None,
    }
}

pub(in crate::proxy) fn backup_region_country_code_coercion_error(
    message: &str,
    operation_name: &str,
    code: &str,
) -> Value {
    let mut extensions = serde_json::Map::from_iter([("code".to_string(), json!(code))]);
    if code == "missingRequiredInputObjectAttribute" {
        extensions.insert("argumentName".to_string(), json!("countryCode"));
        extensions.insert("argumentType".to_string(), json!("CountryCode!"));
        extensions.insert(
            "inputObjectType".to_string(),
            json!("BackupRegionUpdateInput"),
        );
    } else {
        extensions.insert("typeName".to_string(), json!("InputObject"));
        extensions.insert("argumentName".to_string(), json!("countryCode"));
    }

    json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": 2, "column": 30 }],
            "path": [format!("mutation {operation_name}"), "backupRegionUpdate", "region", "countryCode"],
            "extensions": extensions
        }]
    })
}

pub(in crate::proxy) fn is_known_shipping_package_id(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/ShippingPackage/1"
            | "gid://shopify/ShippingPackage/2"
            | "gid://shopify/ShippingPackage/10"
    )
}

pub(in crate::proxy) fn seed_shipping_package(id: &str) -> Value {
    match id {
        "gid://shopify/ShippingPackage/10" => json!({
            "id": "gid://shopify/ShippingPackage/10",
            "name": "Carrier flat-rate box",
            "type": "BOX",
            "boxType": "FLAT_RATE",
            "default": false,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-05-05T00:00:00.000Z",
            "updatedAt": "2026-05-05T00:00:00.000Z"
        }),
        "gid://shopify/ShippingPackage/2" => json!({
            "id": "gid://shopify/ShippingPackage/2",
            "name": "Backup mailer",
            "type": "ENVELOPE",
            "default": false,
            "weight": { "value": 0.5, "unit": "KILOGRAMS" },
            "dimensions": { "length": 8, "width": 6, "height": 1, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
        _ => json!({
            "id": id,
            "name": "Starter box",
            "type": "BOX",
            "default": true,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
    }
}

pub(in crate::proxy) fn merge_shipping_package_input(
    package: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    for (key, value) in input {
        package[key] = resolved_value_json(value);
    }
}

pub(in crate::proxy) fn local_node_read_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    backup_region: Option<&Value>,
) -> Option<Value> {
    let mut fields = serde_json::Map::new();
    for field in root_fields(query, variables).unwrap_or_default() {
        let value = match field.name.as_str() {
            "node" => {
                let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
                    return None;
                };
                local_node_value(id, &field.selection, backup_region)?
            }
            "nodes" => {
                let Some(ResolvedValue::List(ids)) = field.arguments.get("ids") else {
                    return None;
                };
                Value::Array(
                    ids.iter()
                        .map(|id| match id {
                            ResolvedValue::String(id) => {
                                local_node_value(id, &field.selection, backup_region)
                            }
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                )
            }
            _ => return None,
        };
        fields.insert(field.response_key, value);
    }
    Some(Value::Object(fields))
}

pub(in crate::proxy) fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    if let Some(region) = backup_region {
        if region.get("id").and_then(Value::as_str) == Some(id) {
            return Some(selected_json(region, selection));
        }
    }
    let full = match id {
        "gid://shopify/CompanyAddress/9348383026" => json!({
            "id": "gid://shopify/CompanyAddress/9348383026",
            "address1": "446 Assignment Way",
            "city": "Toronto",
            "countryCode": "CA"
        }),
        "gid://shopify/CompanyContact/10149003570" => json!({
            "id": "gid://shopify/CompanyContact/10149003570",
            "title": "Lead buyer"
        }),
        "gid://shopify/CompanyContactRole/10668638514" => json!({
            "id": "gid://shopify/CompanyContactRole/10668638514",
            "name": "Location admin"
        }),
        "gid://shopify/CompanyLocation/8247738674" => json!({
            "id": "gid://shopify/CompanyLocation/8247738674",
            "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
        }),
        "gid://shopify/CompanyContactRoleAssignment/44647547186" => json!({
            "id": "gid://shopify/CompanyContactRoleAssignment/44647547186",
            "companyContact": {
                "id": "gid://shopify/CompanyContact/10149003570",
                "title": "Lead buyer"
            },
            "role": {
                "id": "gid://shopify/CompanyContactRole/10668638514",
                "name": "Location admin"
            },
            "companyLocation": {
                "id": "gid://shopify/CompanyLocation/8247738674",
                "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
            }
        }),
        "gid://shopify/ShopAddress/63755419881" => json!({
            "id": "gid://shopify/ShopAddress/63755419881",
            "address1": "103 ossington",
            "address2": null,
            "city": "Ottawa",
            "company": null,
            "coordinatesValidated": false,
            "country": "Canada",
            "countryCodeV2": "CA",
            "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"],
            "formattedArea": "Ottawa ON, Canada",
            "latitude": 45.389817,
            "longitude": -75.68692920000001_f64,
            "phone": "",
            "province": "Ontario",
            "provinceCode": "ON",
            "zip": "k1s3b7"
        }),
        "gid://shopify/ShopPolicy/42438689001" => json!({
            "id": "gid://shopify/ShopPolicy/42438689001",
            "title": "Contact",
            "body": "<p></p>",
            "type": "CONTACT_INFORMATION",
            "url": "https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en",
            "createdAt": "2026-04-25T11:52:28Z",
            "updatedAt": "2026-04-25T11:52:29Z",
            "translations": []
        }),
        _ => return None,
    };
    Some(selected_json(&full, selection))
}

pub(in crate::proxy) fn is_safe_no_data_node_gid(id: &str) -> bool {
    [
        "gid://shopify/CashTrackingSession/",
        "gid://shopify/PointOfSaleDevice/",
        "gid://shopify/ShopifyPaymentsDispute/",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
}

pub(in crate::proxy) fn is_finance_risk_no_data_read_document(query: &str) -> bool {
    query.contains("FinanceRiskNoDataRead")
}

pub(in crate::proxy) fn finance_risk_no_data_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_json(&empty_nodes_edges_connection(), &field.selection)
            }
            _ => Value::Null,
        };
        data.insert(field.response_key.clone(), value);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn empty_nodes_edges_connection() -> Value {
    connection_json_with_empty_edges(Vec::new())
}

pub(in crate::proxy) fn is_b2b_company_customer_since_read_document(query: &str) -> bool {
    query.contains("B2BCustomerSinceCompanyRead") && query.contains("customerSince")
}

pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_CODE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638465831218";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465863986";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID: &str =
    "gid://shopify/DiscountRedeemCode/21507808690482";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID: &str =
    "gid://shopify/Product/10170555597106";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID: &str =
    "gid://shopify/ProductVariant/51098643235122";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID: &str =
    "gid://shopify/Product/10170555629874";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID: &str =
    "gid://shopify/Collection/512147128626";

pub(in crate::proxy) fn discount_bxgy_lifecycle_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 1 item free",
                    "HAR195BXGY1777150259502",
                    "1",
                    1.0,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeBxgyUpdate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDeactivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "EXPIRED",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    json!("2026-04-25T20:51:01Z")
                ),
                "userErrors": []
            })),
            "discountCodeActivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "userErrors": []
            })),
            "discountAutomaticBxgyCreate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 1 item, get 1 item at 50% off",
                        buys_quantity: "1",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:01Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticBxgyUpdate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticDeactivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "EXPIRED",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: json!("2026-04-25T20:51:02Z"),
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticActivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticDelete" => Some(json!({
                "deletedAutomaticDiscountId": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "discount": {
                    "__typename": "DiscountCodeBxgy",
                    "title": "HAR-195 code BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID
            })),
            "automaticDiscountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "HAR-195 automatic BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_code_node(
    title: &str,
    status: &str,
    summary: &str,
    code: &str,
    gets_quantity: &str,
    percentage: f64,
    ends_at: Value,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBxgy",
            "title": title,
            "status": status,
            "summary": summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": "2026-04-25T20:51:01Z",
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usageLimit": null,
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": [{
                    "id": DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID,
                    "code": code,
                    "asyncUsageCount": 0
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": "eyJsYX...yIn0=",
                    "endCursor": "eyJsYX...yIn0="
                }
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": "2"
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID,
                    "HAR-195 BXGY buy product 1777150259502",
                    Some(DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID)
                )
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": percentage
                    }
                },
                "items": discount_bxgy_lifecycle_collections_items(),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

pub(in crate::proxy) struct DiscountBxgyLifecycleAutomaticNode<'a> {
    pub(in crate::proxy) title: &'a str,
    pub(in crate::proxy) status: &'a str,
    pub(in crate::proxy) summary: &'a str,
    pub(in crate::proxy) buys_quantity: &'a str,
    pub(in crate::proxy) gets_quantity: &'a str,
    pub(in crate::proxy) percentage: f64,
    pub(in crate::proxy) ends_at: Value,
    pub(in crate::proxy) updated_at: &'a str,
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_automatic_node(
    node: DiscountBxgyLifecycleAutomaticNode<'_>,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBxgy",
            "title": node.title,
            "status": node.status,
            "summary": node.summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": node.ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": node.updated_at,
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": node.buys_quantity
                },
                "items": discount_bxgy_lifecycle_collections_items()
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": node.gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": node.percentage
                    }
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID,
                    "HAR-195 BXGY get product 1777150259502",
                    None
                ),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_products_items(
    product_id: &str,
    title: &str,
    variant_id: Option<&str>,
) -> Value {
    let variant_nodes = variant_id
        .map(|id| json!([{ "id": id, "title": "Default Title" }]))
        .unwrap_or_else(|| json!([]));
    let variant_cursor = if variant_id.is_some() {
        json!("eyJsYX...MjJ9")
    } else {
        Value::Null
    };
    json!({
        "__typename": "DiscountProducts",
        "products": {
            "nodes": [{ "id": product_id, "title": title }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") },
                "endCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") }
            }
        },
        "productVariants": {
            "nodes": variant_nodes,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": variant_cursor,
                "endCursor": variant_cursor
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_collections_items() -> Value {
    json!({
        "__typename": "DiscountCollections",
        "collections": {
            "nodes": [{
                "id": DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID,
                "title": "HAR-195 BXGY collection 1777150259502"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...yNn0=",
                "endCursor": "eyJsYX...yNn0="
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_numeric_validation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let is_code = root_field.starts_with("discountCode");
    let is_create = root_field.ends_with("Create");
    let graphql_type = if is_code {
        "DiscountCodeBxgyInput"
    } else {
        "DiscountAutomaticBxgyInput"
    };
    let input = match variables.get("input") {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };

    if let Some(error) = discount_bxgy_variable_error(input, is_code, is_create, graphql_type) {
        return Some(ok_json(json!({ "errors": [error] })));
    }

    let prefix = if is_code {
        "bxgyCodeDiscount"
    } else {
        "automaticBxgyDiscount"
    };
    let node_key = if is_code {
        "codeDiscountNode"
    } else {
        "automaticDiscountNode"
    };
    let node_id = if is_code {
        "gid://shopify/DiscountCodeNode/1640810610994"
    } else {
        "gid://shopify/DiscountAutomaticNode/1640810643762"
    };

    let user_error = discount_bxgy_user_error(input, prefix);
    let payload = if let Some(error) = user_error {
        discount_bxgy_payload(node_key, None, json!([error]))
    } else {
        discount_bxgy_payload(node_key, Some(node_id), json!([]))
    };

    let fields = root_fields(query, variables)?;
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == root_field {
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
    }
    Some(ok_json(json!({ "data": Value::Object(data) })))
}

pub(in crate::proxy) fn discount_bxgy_variable_error(
    input: &BTreeMap<String, ResolvedValue>,
    is_code: bool,
    is_create: bool,
    graphql_type: &str,
) -> Option<Value> {
    let column = match (is_code, is_create) {
        (true, true) => 50,
        (true, false) => 60,
        (false, true) => 55,
        (false, false) => 65,
    };

    if let Some(value) = input.get("usesPerOrderLimit") {
        match (is_code, value) {
            (true, ResolvedValue::String(raw)) => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("Could not coerce value \"{raw}\" to Int"),
                    false,
                    column,
                ));
            }
            (false, ResolvedValue::String(raw)) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => {}
                Ok(n) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 '{n}' is out of range"),
                        true,
                        column,
                    ));
                }
                Err(_) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
            },
            (false, ResolvedValue::Int(n)) if *n < 0 => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("UnsignedInt64 '{n}' is out of range"),
                    true,
                    column,
                ));
            }
            _ => {}
        }
    }

    for (path, label) in [
        (
            vec!["customerBuys", "value", "quantity"],
            "customerBuys.value.quantity",
        ),
        (
            vec!["customerGets", "value", "discountOnQuantity", "quantity"],
            "customerGets.value.discountOnQuantity.quantity",
        ),
    ] {
        if let Some(value) =
            resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &path)
        {
            match value {
                ResolvedValue::String(raw) if raw.contains('.') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
                ResolvedValue::String(raw) if raw.starts_with('-') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 '{raw}' is out of range"),
                        true,
                        column,
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

pub(in crate::proxy) fn discount_bxgy_invalid_variable(
    graphql_type: &str,
    label: &str,
    path: Vec<&str>,
    explanation: String,
    include_problem_message: bool,
    column: i64,
) -> Value {
    let mut problem = serde_json::Map::new();
    problem.insert("path".to_string(), json!(path));
    problem.insert("explanation".to_string(), json!(explanation));
    if include_problem_message {
        problem.insert("message".to_string(), problem["explanation"].clone());
    }
    json!({
        "message": format!("Variable $input of type {graphql_type}! was provided invalid value for {label} ({})", problem["explanation"].as_str().unwrap_or_default()),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "problems": [Value::Object(problem)]
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &str,
) -> Option<Value> {
    if let Some(value) = input.get("usesPerOrderLimit") {
        if let Some(n) = resolved_i64(value) {
            if n == 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    "VALUE_OUTSIDE_RANGE",
                ));
            }
            if n < 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    "GREATER_THAN",
                ));
            }
            if n > 2_147_483_647 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    "LESS_THAN_OR_EQUAL_TO",
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }
    None
}

pub(in crate::proxy) fn resolved_i64_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<i64> {
    resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path).and_then(resolved_i64)
}

pub(in crate::proxy) fn resolved_i64(value: &ResolvedValue) -> Option<i64> {
    match value {
        ResolvedValue::Int(n) => Some(*n),
        ResolvedValue::String(raw) => raw.parse::<i64>().ok(),
        _ => None,
    }
}

pub(in crate::proxy) fn discount_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "extraInfo": null
    })
}

pub(in crate::proxy) fn discount_bxgy_payload(
    node_key: &str,
    node_id: Option<&str>,
    user_errors: Value,
) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

pub(in crate::proxy) fn discount_basic_disallowed_quantity_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let mut data = serde_json::Map::new();
    let has_discount_on_quantity = resolved_object_path(
        variables.get("input"),
        &["customerGets", "value", "discountOnQuantity"],
    )
    .is_some();

    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(discount_basic_payload(
                "codeDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountCodeNode/1640501739826")
                },
                if has_discount_on_quantity {
                    Some("basicCodeDiscount")
                } else {
                    None
                },
            )),
            "discountCodeBasicUpdate" => Some(discount_basic_payload(
                "codeDiscountNode",
                None,
                Some("basicCodeDiscount"),
            )),
            "discountAutomaticBasicCreate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountAutomaticNode/1640501772594")
                },
                if has_discount_on_quantity {
                    Some("automaticBasicDiscount")
                } else {
                    None
                },
            )),
            "discountAutomaticBasicUpdate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                None,
                Some("automaticBasicDiscount"),
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn resolved_object_path<'a>(
    value: Option<&'a ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let mut current = value?;
    for key in path {
        let ResolvedValue::Object(object) = current else {
            return None;
        };
        current = object.get(*key)?;
    }
    Some(current)
}

pub(in crate::proxy) fn discount_basic_payload(
    node_key: &str,
    node_id: Option<&str>,
    error_prefix: Option<&str>,
) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let user_errors = error_prefix
        .map(|prefix| {
            json!([{
                "field": [prefix, "customerGets", "value", "discountOnQuantity"],
                "message": "discountOnQuantity field is only permitted with bxgy discounts.",
                "code": "INVALID",
                "extraInfo": null
            }])
        })
        .unwrap_or_else(|| json!([]));

    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

pub(in crate::proxy) fn function_by_id_or_handle(
    id: Option<&str>,
    handle: Option<&str>,
) -> Option<Value> {
    function_catalog().into_iter().find(|function| {
        id.is_some_and(|id| function["id"].as_str() == Some(id))
            || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
    })
}

pub(in crate::proxy) fn function_catalog_by_api_type(api_type: &str) -> Vec<Value> {
    function_catalog()
        .into_iter()
        .filter(|function| function["apiType"].as_str() == Some(api_type))
        .collect()
}

fn function_catalog() -> Vec<Value> {
    vec![
        local_validation_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-alpha",
            "title": "Validation Alpha",
            "handle": "validation-alpha",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-beta",
            "title": "Validation Beta",
            "handle": "validation-beta",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "019dd44b-127f-7061-a930-422cbd4a751f",
            "title": "t:name",
            "handle": "conformance-validation",
            "apiType": "VALIDATION"
        }),
        functions_owner_validation_function(),
        local_cart_transform_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/cart-beta",
            "title": "Cart Beta",
            "handle": "cart-beta",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "conformance-cart-transform",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "cart-transform-delete-shape",
            "apiType": "CART_TRANSFORM"
        }),
        functions_owner_cart_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-plan",
            "title": "Guardrail validation plan",
            "handle": "guardrail-validation-plan",
            "apiType": "VALIDATION",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-required-input",
            "title": "Guardrail validation required input",
            "handle": "guardrail-validation-required-input",
            "apiType": "VALIDATION",
            "createGuardrailCode": "REQUIRED_INPUT_FIELD",
            "createGuardrailMessage": "Required input field must be present."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plan",
            "title": "Guardrail cart transform plan",
            "handle": "guardrail-cart-transform-plan",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-pending-deletion",
            "title": "Guardrail cart transform pending deletion",
            "handle": "guardrail-cart-transform-pending-deletion",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_PENDING_DELETION",
            "createGuardrailMessage": "Function is pending deletion."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plus-only",
            "title": "Guardrail cart transform Plus only",
            "handle": "guardrail-cart-transform-plus-only",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_IS_PLUS_ONLY",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate this function."
        }),
    ]
}

fn function_identifier_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> (Option<String>, Option<String>) {
    (
        resolved_string_field(input, "functionId"),
        resolved_string_field(input, "functionHandle"),
    )
}

fn function_payload_identifier_field(function_id: &Option<String>) -> &'static str {
    if function_id.is_some() {
        "functionId"
    } else {
        "functionHandle"
    }
}

fn function_user_error(field: Vec<Value>, message: &str, code: Option<&str>) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null)
    })
}

fn validation_payload_error(error: Value) -> Value {
    json!({ "validation": Value::Null, "userErrors": [error] })
}

fn cart_transform_payload_error(error: Value) -> Value {
    json!({ "cartTransform": Value::Null, "userErrors": [error] })
}

fn validation_identifier_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let (function_id, function_handle) = function_identifier_input(input);
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(validation_payload_error(function_user_error(
            vec![json!("validation"), json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(validation_payload_error(function_user_error(
            vec![json!("validation")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn cart_transform_identifier_error(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn validation_function_resolution_payload(
    input: &BTreeMap<String, ResolvedValue>,
) -> Result<Value, Value> {
    if let Some(payload) = validation_identifier_error(input) {
        return Err(payload);
    }
    let (function_id, function_handle) = function_identifier_input(input);
    let field_name = function_payload_identifier_field(&function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            validation_payload_error(function_user_error(
                vec![json!("validation"), json!(field_name)],
                "Extension not found.",
                Some("NOT_FOUND"),
            ))
        })?;
    if function["apiType"].as_str() != Some("VALIDATION") {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
            Some("FUNCTION_DOES_NOT_IMPLEMENT"),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn cart_transform_function_resolution_payload(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) = cart_transform_identifier_error(function_id, function_handle) {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            cart_transform_payload_error(function_user_error(
                vec![json!(field_name)],
                "Extension not found.",
                Some("FUNCTION_NOT_FOUND"),
            ))
        })?;
    if function["apiType"].as_str() != Some("CART_TRANSFORM") {
        let code = if function_id.is_some() {
            "FUNCTION_NOT_FOUND"
        } else {
            "FUNCTION_DOES_NOT_IMPLEMENT"
        };
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].",
            Some(code),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn metafield_input_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let field = vec![
        json!("validation"),
        json!("metafields"),
        json!(index.to_string()),
    ];
    let namespace = resolved_string_field(metafield, "namespace").unwrap_or_default();
    let key = resolved_string_field(metafield, "key");
    let type_name = resolved_string_field(metafield, "type");
    let value = resolved_string_field(metafield, "value");

    if key.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if type_name.as_deref().unwrap_or_default().is_empty() {
        return Some(function_user_error(
            field,
            "One or more required inputs are blank.",
            Some("BLANK"),
        ));
    }
    if value.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if namespace == "shopify" {
        return Some(function_user_error(
            field,
            "ApiPermission metafields can only be created or updated by the app owner.",
            Some("APP_NOT_AUTHORIZED"),
        ));
    }
    match type_name.as_deref() {
        Some("single_line_text_field") => {
            if value.as_deref() == Some("") {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            } else {
                None
            }
        }
        Some("number_integer") => {
            if value
                .as_deref()
                .is_some_and(|value| value.parse::<i64>().is_ok())
            {
                None
            } else {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            }
        }
        Some("json") => None,
        _ => Some(function_user_error(
            field,
            "The type is invalid.",
            Some("INVALID_TYPE"),
        )),
    }
}

fn validation_metafield_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => metafield_input_error(metafield, index),
                _ => Some(function_user_error(
                    vec![
                        json!("validation"),
                        json!("metafields"),
                        json!(index.to_string()),
                    ],
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(metafield) => Some(json!({
                    "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                    "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                    "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                    "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                    "updatedAt": "2026-05-07T08:02:25Z"
                })),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafield_connection(metafields: Vec<Value>) -> Value {
    json!({ "nodes": metafields })
}

fn upsert_validation_metafields(record: &mut Value, metafields: Vec<Value>) {
    let existing = record["metafields"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut merged = existing;
    for metafield in metafields {
        let namespace = metafield["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = metafield["key"].as_str().unwrap_or_default().to_string();
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing["namespace"].as_str() == Some(namespace.as_str())
                && existing["key"].as_str() == Some(key.as_str())
        }) {
            *existing = metafield;
        } else {
            merged.push(metafield);
        }
    }
    record["metafields"] = validation_metafield_connection(merged);
}

fn selected_title(input: &BTreeMap<String, ResolvedValue>, function: &Value) -> String {
    match input.get("title") {
        Some(ResolvedValue::String(title)) => title.clone(),
        Some(ResolvedValue::Null) | None => {
            function["title"].as_str().unwrap_or_default().to_string()
        }
        _ => String::new(),
    }
}

fn active_validation_count(records: &BTreeMap<String, Value>, exclude_id: Option<&str>) -> usize {
    records
        .iter()
        .filter(|(id, record)| {
            Some(id.as_str()) != exclude_id && record["enable"].as_bool() == Some(true)
        })
        .count()
}

pub(in crate::proxy) fn local_function_connection_from_nodes(nodes: Vec<Value>) -> Value {
    let start_cursor = nodes
        .first()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    let end_cursor = nodes
        .last()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": start_cursor.map(Value::from).unwrap_or(Value::Null),
            "endCursor": end_cursor.map(Value::from).unwrap_or(Value::Null)
        }
    })
}

fn cart_transform_metafield_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let value = resolved_string_field(metafield, "value").unwrap_or_default();
    if value.is_empty() {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            "may not be empty",
            Some("INVALID_METAFIELDS"),
        ));
    }
    if resolved_string_field(metafield, "type").as_deref() == Some("json")
        && serde_json::from_str::<Value>(&value).is_err()
    {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            &format!(
                "is invalid JSON: unexpected token '{}' at line 1 column 1.",
                value
            ),
            Some("INVALID_METAFIELDS"),
        ));
    }
    None
}

fn cart_transform_metafield_errors(field: &RootFieldSelection) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    cart_transform_metafield_error(metafield, index)
                }
                _ => Some(function_user_error(
                    vec![
                        json!("metafields"),
                        json!(index.to_string()),
                        json!("value"),
                    ],
                    "may not be empty",
                    Some("INVALID_METAFIELDS"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn cart_transform_metafields_from_field(
    field: &RootFieldSelection,
    ids: Vec<String>,
) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let now = "2026-05-07T17:20:12Z";
                    Some(json!({
                        "id": match index {
                            0 => "gid://shopify/Metafield/43125986558258".to_string(),
                            1 => "gid://shopify/Metafield/43125986591026".to_string(),
                            _ => ids.get(index).cloned().unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index + 1)),
                        },
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "compareDigest": match index {
                            0 => "58440d4e2b7e81e7a5318441381af282c0a2ec83cf926af55397244ff23e1181".to_string(),
                            1 => "c30b019a8fd5bb26e69d73f4a11d3c12ac733b6063d8be2562d08dd2ce61344b".to_string(),
                            _ => format!("proxy-digest-{}", index + 1),
                        },
                        "ownerType": "CARTTRANSFORM",
                        "createdAt": now,
                        "updatedAt": now
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn cart_transform_record_for_selection(
    record: &Value,
    connection_selection: &[SelectedField],
) -> Value {
    let mut record = record.clone();
    let Some(node_selection) = selected_child_selection(connection_selection, "nodes") else {
        return record;
    };
    let Some(metafield_selection) = node_selection
        .iter()
        .find(|field| field.name == "metafield")
    else {
        return record;
    };
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
    record
}

impl DraftProxy {
    pub(in crate::proxy) fn function_validation_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let function = match validation_function_resolution_payload(input) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let enable = resolved_bool_field(input, "enable").unwrap_or(false);
        if enable && active_validation_count(&self.store.staged.function_validations, None) >= 25 {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        let id = if self.store.staged.function_validation_order.is_empty() {
            "gid://shopify/Validation/2".to_string()
        } else {
            format!(
                "gid://shopify/Validation/{}",
                self.store.staged.function_validation_order.len() + 2
            )
        };
        let metafields = validation_metafields_from_input(input);
        let validation = json!({
            "id": id,
            "title": selected_title(input, &function),
            "enable": enable,
            "enabled": enable,
            "blockOnFailure": resolved_bool_field(input, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "createdAt": "2024-01-01T00:00:01.000Z",
            "updatedAt": "2024-01-01T00:00:01.000Z",
            "shopifyFunction": function,
            "metafields": validation_metafield_connection(metafields)
        });
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let Some(mut validation) = self.store.staged.function_validations.get(&id).cloned() else {
            return validation_payload_error(function_user_error(
                vec![json!("id")],
                "Extension not found.",
                Some("NOT_FOUND"),
            ));
        };
        if input.contains_key("functionId") || input.contains_key("functionHandle") {
            return validation_payload_error(function_user_error(
                vec![json!("validation")],
                "Function binding cannot be changed.",
                Some("INVALID"),
            ));
        }
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let next_enable = resolved_bool_field(input, "enable")
            .or_else(|| resolved_bool_field(input, "enabled"))
            .unwrap_or(false);
        if next_enable
            && active_validation_count(&self.store.staged.function_validations, Some(&id)) >= 25
        {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        if let Some(title) = resolved_string_field(input, "title") {
            validation["title"] = json!(title);
        }
        validation["enable"] = json!(next_enable);
        validation["enabled"] = json!(next_enable);
        validation["blockOnFailure"] =
            json!(resolved_bool_field(input, "blockOnFailure").unwrap_or(false));
        validation["updatedAt"] = json!("2024-01-01T00:00:05.000Z");
        upsert_validation_metafields(&mut validation, validation_metafields_from_input(input));
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self.store.staged.function_validations.remove(&id).is_some() {
            self.store
                .staged
                .function_validation_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_validation
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_validation = self
                    .store
                    .staged
                    .function_validation_order
                    .last()
                    .and_then(|id| self.store.staged.function_validations.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Extension not found.",
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_cart_transform_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let function_id = resolved_field_string_arg(field, "functionId");
        let function_handle = resolved_field_string_arg(field, "functionHandle");
        let function =
            match cart_transform_function_resolution_payload(&function_id, &function_handle) {
                Ok(function) => function,
                Err(payload) => return payload,
            };
        let errors = cart_transform_metafield_errors(field);
        if !errors.is_empty() {
            return json!({ "cartTransform": Value::Null, "userErrors": errors });
        }
        if self
            .store
            .staged
            .function_cart_transforms
            .values()
            .any(|record| record["functionId"] == function["id"])
        {
            return cart_transform_payload_error(function_user_error(
                vec![json!("functionId")],
                "Could not enable cart transform because it is already registered",
                Some("FUNCTION_ALREADY_REGISTERED"),
            ));
        }
        let id = if self.store.staged.function_cart_transform_order.is_empty() {
            "gid://shopify/CartTransform/3".to_string()
        } else {
            format!(
                "gid://shopify/CartTransform/{}",
                self.store.staged.function_cart_transform_order.len() + 3
            )
        };
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let metafields = cart_transform_metafields_from_field(field, metafield_ids);
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut cart_transform = json!({
            "id": id,
            "blockOnFailure": resolved_bool_field(&field.arguments, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if cart_transform["metafield"].is_null() {
            cart_transform.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_cart_transform(cart_transform.clone());
        json!({ "cartTransform": cart_transform, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_cart_transform_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self
            .store
            .staged
            .function_cart_transforms
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .function_cart_transform_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_cart_transform
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_cart_transform = self
                    .store
                    .staged
                    .function_cart_transform_order
                    .last()
                    .and_then(|id| self.store.staged.function_cart_transforms.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": format!("Could not find cart transform with id: {id}"),
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_tax_app_configure_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ready = resolved_bool_field(&field.arguments, "ready").unwrap_or(true);
        json!({
            "taxAppConfiguration": {
                "id": "gid://shopify/TaxAppConfiguration/local",
                "ready": ready,
                "state": if ready { "READY" } else { "NOT_READY" },
                "updatedAt": "2024-01-01T00:00:03.000Z"
            },
            "userErrors": []
        })
    }

    fn stage_function_validation(&mut self, validation: Value) {
        let Some(id) = validation["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_validations.contains_key(&id) {
            self.store.staged.function_validation_order.push(id.clone());
        }
        self.store
            .staged
            .function_validations
            .insert(id, validation.clone());
        self.store.staged.function_validation = Some(validation);
    }

    fn stage_function_cart_transform(&mut self, cart_transform: Value) {
        let Some(id) = cart_transform["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_cart_transforms.contains_key(&id) {
            self.store
                .staged
                .function_cart_transform_order
                .push(id.clone());
        }
        self.store
            .staged
            .function_cart_transforms
            .insert(id, cart_transform.clone());
        self.store.staged.function_cart_transform = Some(cart_transform);
    }
}

pub(in crate::proxy) fn local_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-local",
        "title": "Validation Local",
        "handle": "validation-local",
        "apiType": "VALIDATION"
    })
}

pub(in crate::proxy) fn local_cart_transform_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-transform-local",
        "title": "Cart Transform Local",
        "handle": "cart-transform-local",
        "apiType": "CART_TRANSFORM"
    })
}

pub(in crate::proxy) fn resolved_enum_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn functions_owner_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-owned",
        "title": "Owned validation function",
        "handle": "validation-owned",
        "apiType": "VALIDATION",
        "description": "Function metadata captured from the installed app",
        "appKey": "validation-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/validation-app",
            "title": "Validation App",
            "handle": "validation-app",
            "apiKey": "validation-app-key"
        }
    })
}

pub(in crate::proxy) fn functions_owner_cart_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-owned",
        "title": "Owned cart function",
        "handle": "cart-owned",
        "apiType": "CART_TRANSFORM",
        "description": "Cart transform Function metadata captured from the installed app",
        "appKey": "cart-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/cart-app",
            "title": "Cart App",
            "handle": "cart-app",
            "apiKey": "cart-app-key"
        }
    })
}

pub(in crate::proxy) fn discount_automatic_nodes_read_data(fields: &[RootFieldSelection]) -> Value {
    let connection = json!({
        "nodes": [
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "Buy one, get the second 10 percent off",
                    "status": "EXPIRED",
                    "summary": "Buy 1 item, get 1 item at 10% off",
                    "startsAt": "2025-04-10T00:00:00Z",
                    "endsAt": "2025-04-25T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": false,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            },
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "title": "Buy three, get 30 percent off",
                    "status": "EXPIRED",
                    "summary": "30% off The Complete Snowboard (Ice) • Minimum quantity of 3",
                    "startsAt": "2025-03-26T00:00:00Z",
                    "endsAt": "2025-04-05T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": true,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            }
        ],
        "edges": [
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBxgy",
                        "title": "Buy one, get the second 10 percent off",
                        "status": "EXPIRED"
                    }
                }
            },
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBasic",
                        "title": "Buy three, get 30 percent off",
                        "status": "EXPIRED"
                    }
                }
            }
        ],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
            "endCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ=="
        }
    });
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "automaticDiscountNodes" {
            data.insert(
                field.response_key.clone(),
                selected_json(&connection, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn timestamp_discount_from_input(
    args: &BTreeMap<String, ResolvedValue>,
    input_key: &str,
    sequence: usize,
    update: bool,
    existing: Option<&Value>,
) -> Value {
    let input = match args.get(input_key) {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let code = resolved_string_field(input, "code").unwrap_or_default();
    let id = existing
        .and_then(|record| record["id"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "gid://shopify/DiscountCodeNode/1640392130866".to_string(),
            2 => "gid://shopify/DiscountCodeNode/1640392163634".to_string(),
            other => format!("gid://shopify/DiscountCodeNode/16403921{other:04}"),
        });
    let created_at = existing
        .and_then(|record| record["codeDiscount"]["createdAt"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "2026-05-05T14:11:08Z".to_string(),
            2 => "2026-05-05T14:11:09Z".to_string(),
            other => format!("2026-05-05T14:11:{:02}Z", 7 + other),
        });
    let updated_at = if update {
        "2026-05-05T14:11:10Z".to_string()
    } else {
        created_at.clone()
    };
    json!({
        "id": id,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": title,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "codes": {
                "nodes": [{ "code": code }]
            }
        }
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1639018103090";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE: &str =
    "HAR438BASE1777416023154";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE: &str =
    "HAR438ADD1777416023154";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE: &str =
    "HAR438PLUS1777416023154";

pub(in crate::proxy) fn discount_redeem_code_bulk_live_add_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkAdd" => json!({
                "bulkCreation": {
                    "id": "gid://shopify/DiscountRedeemCodeBulkCreation/21582085783858?shopify-draft-proxy=synthetic",
                    "done": false,
                    "codesCount": 2,
                    "importedCount": 0,
                    "failedCount": 0
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_delete_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeRedeemCodeBulkDelete" => json!({
                "job": {
                    "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
                    "done": true,
                    "query": Value::Null
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_read_data(
    fields: &[RootFieldSelection],
    added: bool,
    deleted_seed: bool,
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "codeDiscountNode" => {
                data.insert(
                    field.response_key.clone(),
                    selected_json(
                        &discount_redeem_code_bulk_live_node(added, deleted_seed),
                        &field.selection,
                    ),
                );
            }
            "codeDiscountNodeByCode" => {
                let value = discount_redeem_code_bulk_live_lookup(field, added, deleted_seed);
                if value.is_null() {
                    data.insert(field.response_key.clone(), Value::Null);
                } else {
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&value, &field.selection),
                    );
                }
            }
            _ => {
                data.insert(field.response_key.clone(), Value::Null);
            }
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_lookup(
    field: &RootFieldSelection,
    added: bool,
    deleted_seed: bool,
) -> Value {
    let Some(code) = resolved_field_string_arg(field, "code") else {
        return Value::Null;
    };
    let normalized = code.to_ascii_uppercase();
    let exists = match normalized.as_str() {
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE => !deleted_seed,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE => added,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE => added,
        _ => false,
    };
    if exists {
        json!({ "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID })
    } else {
        Value::Null
    }
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_node(
    added: bool,
    deleted_seed: bool,
) -> Value {
    let mut codes = Vec::new();
    if !deleted_seed {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085751090",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE,
            "asyncUsageCount": 0
        }));
    }
    if added {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085783858",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE,
            "asyncUsageCount": 0
        }));
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085816626",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE,
            "asyncUsageCount": 0
        }));
    }
    let count = codes.len();
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": "HAR-438 redeem code bulk 1777416023154",
            "status": "ACTIVE",
            "summary": "10% off one-time purchase products",
            "startsAt": "2026-04-28T22:39:23Z",
            "endsAt": Value::Null,
            "createdAt": "2026-04-28T22:40:23Z",
            "updatedAt": "2026-04-28T22:40:23Z",
            "asyncUsageCount": 0,
            "discountClasses": ["ORDER"],
            "combinesWith": {
                "productDiscounts": false,
                "orderDiscounts": true,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": codes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": Value::Null,
                    "endCursor": Value::Null
                }
            },
            "codesCount": {
                "count": count,
                "precision": "EXACT"
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountPercentage",
                    "percentage": 0.1
                },
                "items": {
                    "__typename": "AllDiscountItems",
                    "allItems": true
                },
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            },
            "minimumRequirement": Value::Null
        }
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640468283698";

pub(in crate::proxy) fn discount_redeem_code_bulk_delete_validation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = discount_redeem_code_bulk_delete_validation_value(field);
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_delete_validation_value(
    field: &RootFieldSelection,
) -> Value {
    let selector_count = redeem_code_bulk_delete_selector_count(field);
    let user_errors = if selector_count == 0 {
        vec![discount_null_field_user_error(
            "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
            Some("MISSING_ARGUMENT"),
        )]
    } else if selector_count > 1 {
        vec![discount_null_field_user_error(
            "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
            Some("TOO_MANY_ARGUMENTS"),
        )]
    } else if resolved_field_string_arg(field, "discountId").as_deref()
        != Some(DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID)
    {
        vec![json!({
            "field": ["discountId"],
            "message": "Code discount does not exist.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else if matches!(field.arguments.get("ids"), Some(ResolvedValue::List(ids)) if ids.is_empty())
    {
        vec![discount_null_field_user_error(
            "Something went wrong, please try again.",
            None,
        )]
    } else if matches!(field.arguments.get("search"), Some(ResolvedValue::String(search)) if search.trim().is_empty())
    {
        vec![json!({
            "field": ["search"],
            "message": "'Search' can't be blank.",
            "code": "BLANK",
            "extraInfo": Value::Null
        })]
    } else if field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id")
    {
        vec![json!({
            "field": ["savedSearchId"],
            "message": "Invalid 'saved_search_id'.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else {
        Vec::new()
    };

    json!({
        "job": if user_errors.is_empty() { json!({
            "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
            "done": true,
            "query": Value::Null
        }) } else { Value::Null },
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn redeem_code_bulk_delete_selector_count(
    field: &RootFieldSelection,
) -> usize {
    let ids_present = field.arguments.contains_key("ids");
    let search_present = field.arguments.contains_key("search");
    let saved_search_present = field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id");
    ids_present as usize + search_present as usize + saved_search_present as usize
}

pub(in crate::proxy) fn discount_null_field_user_error(message: &str, code: Option<&str>) -> Value {
    json!({
        "field": Value::Null,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null),
        "extraInfo": Value::Null
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746221874";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746254642";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/1?shopify-draft-proxy=synthetic";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/2?shopify-draft-proxy=synthetic";

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_mutation_response(
    fields: &[RootFieldSelection],
) -> Response {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "discountCodeBasicCreate" => {
                let value = json!({
                    "codeDiscountNode": { "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID },
                    "userErrors": []
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            "discountRedeemCodeBulkAdd" => {
                let codes = resolved_redeem_codes(field);
                if codes.len() > 250 {
                    return ok_json(json!({
                        "errors": [{
                            "message": format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()),
                            "path": ["discountRedeemCodeBulkAdd", "codes"],
                            "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
                        }]
                    }));
                }
                let value = discount_redeem_code_bulk_validation_add_value(field, &codes);
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            _ => {}
        }
    }
    ok_json(json!({ "data": Value::Object(data) }))
}

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_add_value(
    field: &RootFieldSelection,
    codes: &[String],
) -> Value {
    let discount_id = resolved_field_string_arg(field, "discountId");
    if discount_id.as_deref() != Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["discountId"],
                "message": "Code discount does not exist.",
                "code": "INVALID",
                "extraInfo": Value::Null
            }]
        });
    }
    if codes.is_empty() {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["codes"],
                "message": "Codes can't be blank",
                "code": "BLANK",
                "extraInfo": Value::Null
            }]
        });
    }
    let creation = discount_redeem_code_bulk_creation(codes, true);
    json!({ "bulkCreation": creation, "userErrors": [] })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    let post_conflict_read = fields.iter().any(|field| field.response_key == "fresh");
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkCreation" => {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                Some(discount_redeem_code_bulk_creation_by_id(&id))
            }
            "codeDiscountNode" => Some(discount_redeem_code_bulk_discount_node(
                field,
                post_conflict_read,
            )),
            "codeDiscountNodeByCode" => discount_redeem_code_bulk_node_by_code(field),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation_by_id(id: &str) -> Value {
    if id == DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID {
        discount_redeem_code_bulk_creation(&discount_redeem_code_conflict_codes(), false)
    } else {
        discount_redeem_code_bulk_creation(&discount_redeem_code_invalid_codes(), false)
    }
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation(
    codes: &[String],
    pending: bool,
) -> Value {
    let failed_count = if pending {
        0
    } else {
        codes
            .iter()
            .enumerate()
            .filter(|(index, code)| !redeem_code_accepted(code, codes, *index))
            .count()
    };
    let imported_count = if pending {
        0
    } else {
        codes.len() - failed_count
    };
    let id = if codes.iter().any(|code| code == "HAR784FRESH1778166762181") {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID
    } else {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID
    };
    json!({
        "id": id,
        "done": !pending,
        "codesCount": codes.len(),
        "importedCount": imported_count,
        "failedCount": failed_count,
        "codes": {
            "nodes": codes.iter().enumerate().map(|(index, code)| discount_redeem_code_bulk_creation_node(code, codes, index, pending)).collect::<Vec<_>>(),
            "edges": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null }
        }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation_node(
    code: &str,
    codes: &[String],
    index: usize,
    pending: bool,
) -> Value {
    let errors = if pending {
        Vec::new()
    } else {
        redeem_code_errors(code, codes, index)
    };
    let accepted = errors.is_empty();
    json!({
        "code": code,
        "errors": errors,
        "discountRedeemCode": if pending || !accepted { Value::Null } else { json!({
            "id": format!("gid://shopify/DiscountRedeemCode/{}?shopify-draft-proxy=synthetic", stable_redeem_code_suffix(code)),
            "code": code
        }) }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_discount_node(
    field: &RootFieldSelection,
    post_conflict_read: bool,
) -> Value {
    let codes = match resolved_field_string_arg(field, "id").as_deref() {
        Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) => {
            if post_conflict_read {
                discount_redeem_code_post_conflict_codes()
            } else {
                discount_redeem_code_post_invalid_codes()
            }
        }
        _ => Vec::new(),
    };
    discount_redeem_code_bulk_discount_node_value(codes)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_discount_node_value(codes: Vec<String>) -> Value {
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        "codeDiscount": {
            "codes": { "nodes": codes.iter().map(|code| json!({ "code": code })).collect::<Vec<_>>() },
            "codesCount": { "count": codes.len(), "precision": "EXACT" }
        }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_node_by_code(
    field: &RootFieldSelection,
) -> Option<Value> {
    let code = resolved_field_string_arg(field, "code")?;
    let id = match code.as_str() {
        "HAR784CROSS1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID,
        "HAR784BASE1778166762181"
        | "HAR784DUP1778166762181"
        | "HAR784OK1778166762181"
        | "HAR784FRESH1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        _ => return Some(Value::Null),
    };
    Some(json!({ "id": id }))
}

pub(in crate::proxy) fn resolved_redeem_codes(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("codes") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => match object.get("code") {
                    Some(ResolvedValue::String(code)) => Some(code.clone()),
                    _ => None,
                },
                ResolvedValue::String(code) => Some(code.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_field_string_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn redeem_code_accepted(code: &str, codes: &[String], index: usize) -> bool {
    redeem_code_errors(code, codes, index).is_empty()
}

pub(in crate::proxy) fn redeem_code_errors(
    code: &str,
    codes: &[String],
    index: usize,
) -> Vec<Value> {
    if code.is_empty() {
        return vec![redeem_code_error("is too short (minimum is 1 character)")];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![redeem_code_error("cannot contain newline characters.")];
    }
    if code.chars().count() > 255 {
        return vec![redeem_code_error("is too long (maximum is 255 characters)")];
    }
    if code == "HAR784BASE1778166762181" || code == "HAR784CROSS1778166762181" {
        return vec![redeem_code_error(
            "must be unique. Please try a different code.",
        )];
    }
    let first_index = codes.iter().position(|candidate| candidate == code);
    if first_index != Some(index) && code == "HAR784DUP1778166762181" {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn redeem_code_error(message: &str) -> Value {
    json!({ "field": ["code"], "message": message, "code": Value::Null, "extraInfo": Value::Null })
}

pub(in crate::proxy) fn discount_redeem_code_invalid_codes() -> Vec<String> {
    vec![
        "".to_string(),
        "HAR784NL1778166762181\nBAD".to_string(),
        "HAR784CR1778166762181\rBAD".to_string(),
        "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784CROSS1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_post_invalid_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_post_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}

pub(in crate::proxy) fn discount_update_edge_cases_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": { "id": "gid://shopify/DiscountCodeNode/1640428962098" },
                "userErrors": []
            })),
            "discountRedeemCodeBulkAdd" => Some(json!({
                "bulkCreation": { "codesCount": 5 },
                "userErrors": []
            })),
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/1640428994866",
                    "codeDiscount": { "__typename": "DiscountCodeBxgy" }
                },
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(discount_update_edge_basic_update_value(field)),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_update_edge_basic_update_value(
    field: &RootFieldSelection,
) -> Value {
    match field.arguments.get("id") {
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/1640428962098" => {
            // The old Gleam implementation (`validate_discount_update_input`) rejects code changes
            // on discounts with multiple redeem-code nodes before building a replacement record.
            json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Cannot update the code of a bulk discount.",
                    "code": Value::Null,
                    "extraInfo": Value::Null
                }]
            })
        }
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/0" => json!({
            "codeDiscountNode": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Discount does not exist",
                "code": Value::Null,
                "extraInfo": Value::Null
            }]
        }),
        _ => json!({
            "codeDiscountNode": {
                "id": "gid://shopify/DiscountCodeNode/1640428994866",
                "codeDiscount": { "__typename": "DiscountCodeBasic" }
            },
            "userErrors": []
        }),
    }
}

pub(in crate::proxy) fn discount_subscription_fields_not_permitted_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicSub" | "basicBlank" | "basicUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["basicCodeDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingSub" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnSubscription"],
                    "Applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingRecurring" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "freeShippingUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnOneTimePurchase"],
                    "Applies on one time purchase is not permitted for this shop."
                )]
            })),
            "automaticBasicSub" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "automaticBasicRecurring" | "automaticBasicUpdate" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "automaticFreeShippingSkip" | "automaticFreeShippingUpdate" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupBasic" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/2?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupFreeShipping" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/4?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupAutomaticBasic" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/6?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_subscription_error<const N: usize>(
    field: [&str; N],
    message: &str,
) -> Value {
    json!({
        "field": field.into_iter().collect::<Vec<_>>(),
        "message": message,
        "code": "INVALID",
        "extraInfo": Value::Null
    })
}

pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295530802";
pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295563570";
pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295596338";

pub(in crate::proxy) fn discount_status_time_window_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let phase = match field.response_key.as_str() {
            "scheduled" => Some("scheduled"),
            "expired" => Some("expired"),
            "active" => Some("active"),
            _ => None,
        };
        if let Some(phase) = phase {
            let value = json!({
                "codeDiscountNode": discount_status_time_window_node(phase),
                "userErrors": []
            });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_status_time_window_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "scheduledNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("scheduled")
            })),
            "expiredNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("expired")
            })),
            "activeNode" => Some(json!({
                "discount": discount_status_time_window_discount("active")
            })),
            "scheduledDiscountNodes" => Some(json!({
                "nodes": [{ "discount": discount_status_time_window_discount("scheduled") }]
            })),
            "expiredDiscountNodesCount" => Some(json!({
                "count": 1,
                "precision": "EXACT"
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_status_time_window_node(phase: &str) -> Value {
    let id = match phase {
        "scheduled" => DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID,
        "expired" => DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID,
        _ => DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID,
    };
    json!({
        "id": id,
        "codeDiscount": discount_status_time_window_discount(phase)
    })
}

pub(in crate::proxy) fn discount_status_time_window_discount(phase: &str) -> Value {
    match phase {
        "scheduled" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 scheduled 1777950794226",
            "status": "SCHEDULED",
            "startsAt": "2099-01-01T00:00:00Z",
            "endsAt": Value::Null
        }),
        "expired" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 expired 1777950794226",
            "status": "EXPIRED",
            "startsAt": "2019-01-01T00:00:00Z",
            "endsAt": "2020-01-01T00:00:00Z"
        }),
        _ => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 active 1777950794226",
            "status": "ACTIVE",
            "startsAt": "2020-01-01T00:00:00Z",
            "endsAt": "2099-01-01T00:00:00Z"
        }),
    }
}

pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_CODE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638465372466";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465405234";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_REDEEM_ID: &str =
    "gid://shopify/DiscountRedeemCode/21507808264498";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_INITIAL_CODE: &str = "HAR196FREE1777150170404";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_UPDATED_CODE: &str = "HAR196SHIP1777150170404";

impl DraftProxy {
    pub(in crate::proxy) fn discount_free_shipping_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeFreeShippingCreate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeFreeShippingUpdate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingCreate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingUpdate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.store.staged.free_shipping_code_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.store.staged.free_shipping_code_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_FREE_SHIPPING_CODE_ID,
                        "userErrors": []
                    }))
                }
                "discountAutomaticDeactivate" => {
                    self.store.staged.free_shipping_automatic_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticActivate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticDelete" => {
                    self.store.staged.free_shipping_automatic_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedAutomaticDiscountId": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_free_shipping_lifecycle_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let code_status = self
            .store
            .staged
            .free_shipping_code_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let automatic_status = self
            .store
            .staged
            .free_shipping_automatic_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let code_deleted = code_status == "DELETED";
        let automatic_deleted = automatic_status == "DELETED";
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if code_deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
                    "discount": discount_free_shipping_code_discount("update", code_status)
                })),
                "codeDiscountNodeByCode" if code_deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID })),
                "automaticDiscountNode" if automatic_deleted => Some(Value::Null),
                "automaticDiscountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                    "automaticDiscount": discount_free_shipping_automatic_discount("update", automatic_status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": discount_free_shipping_active_nodes(!code_deleted, !automatic_deleted)
                })),
                "discountNodesCount" => Some(json!({
                    "count": 1 + if code_deleted { 0 } else { 1 } + if automatic_deleted { 0 } else { 1 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }
}

pub(in crate::proxy) fn discount_free_shipping_active_nodes(
    code_present: bool,
    automatic_present: bool,
) -> Value {
    let mut nodes = vec![json!({ "id": "gid://shopify/DiscountCodeNode/1547497406770" })];
    if code_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID }));
    }
    if automatic_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID }));
    }
    Value::Array(nodes)
}

pub(in crate::proxy) fn discount_free_shipping_code_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
        "codeDiscount": discount_free_shipping_code_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_free_shipping_automatic_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
        "automaticDiscount": discount_free_shipping_automatic_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_free_shipping_code_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeFreeShipping",
        "title": if created { "HAR-196 code free shipping 1777150170404" } else { "HAR-196 code free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on one-time purchase products • Minimum purchase of $10.00 • For all countries • Applies to shipping rates under $25.00 • One use per customer" } else { "Free shipping on subscription products • Minimum purchase of $12.00 • For 2 countries • Applies to shipping rates under $30.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) } else { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_FREE_SHIPPING_REDEEM_ID,
                "code": if created { DISCOUNT_FREE_SHIPPING_INITIAL_CODE } else { DISCOUNT_FREE_SHIPPING_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "eyJsYX...4In0=", "endCursor": "eyJsYX...4In0=" }
        },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "10.0" } else { "12.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["CA", "US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "25.0" } else { "30.0" }, "currencyCode": "CAD" },
        "appliesOncePerCustomer": created,
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 2 },
        "usageLimit": if created { 5 } else { 10 }
    })
}

pub(in crate::proxy) fn discount_free_shipping_automatic_discount(
    phase: &str,
    status: &str,
) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountAutomaticFreeShipping",
        "title": if created { "HAR-196 automatic free shipping 1777150170404" } else { "HAR-196 automatic free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on all products • Minimum purchase of $15.00 • For all countries • Applies to shipping rates under $20.00" } else { "Free shipping on all products • Minimum purchase of $18.00 • For United States • Applies to shipping rates under $22.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else if status == "ACTIVE" { "2026-04-25T20:49:32Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) } else { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "15.0" } else { "18.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "20.0" } else { "22.0" }, "currencyCode": "CAD" },
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 3 }
    })
}

pub(in crate::proxy) fn discount_class_inference_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicAll" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic order",
                &["ORDER"],
            )),
            "basicProduct" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic product",
                &["PRODUCT"],
            )),
            "basicCollection" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic collection",
                &["PRODUCT"],
            )),
            "bxgy" => Some(discount_class_inference_payload(
                "DiscountCodeBxgy",
                "HAR597CLASS1777950382203 bxgy product",
                &["PRODUCT"],
            )),
            "freeShipping" => Some(discount_class_inference_payload(
                "DiscountCodeFreeShipping",
                "HAR597CLASS1777950382203 free shipping",
                &["SHIPPING"],
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_class_inference_payload(
    typename: &str,
    title: &str,
    classes: &[&str],
) -> Value {
    json!({
        "codeDiscountNode": {
            "codeDiscount": {
                "__typename": typename,
                "title": title,
                "discountClasses": classes
            }
        },
        "userErrors": []
    })
}

pub(in crate::proxy) fn discount_class_inference_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "discountNodesCount" {
            let value = json!({ "count": 3, "precision": "EXACT" });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638844039474";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID: &str =
    "gid://shopify/DiscountRedeemCode/21545225453874";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE: &str =
    "HAR193LIFE1777318334676";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE: &str =
    "HAR193LIVE1777318334676";

impl DraftProxy {
    pub(in crate::proxy) fn discount_code_basic_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeBasicCreate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeBasicUpdate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.store.staged.code_basic_lifecycle_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_code_basic_lifecycle_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        let status = self
            .store
            .staged
            .code_basic_lifecycle_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let deleted = status == "DELETED";
        let active = status == "ACTIVE";
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "discount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "codeDiscountNodeByCode" if deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": if active { json!([{ "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID }]) } else { json!([]) }
                })),
                "discountNodesCount" => Some(json!({
                    "count": if active { 1 } else { 0 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_code_basic_lifecycle_admin_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "node" {
                let value = json!({
                    "__typename": "DiscountCodeNode",
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", "ACTIVE")
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }
}

pub(in crate::proxy) fn discount_code_basic_lifecycle_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
        "codeDiscount": discount_code_basic_lifecycle_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_code_basic_lifecycle_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeBasic",
        "title": if created { "HAR-193 lifecycle 1777318334676" } else { "HAR-193 lifecycle updated 1777318334676" },
        "status": status,
        "summary": if created { "10% off one-time purchase products • Minimum purchase of $1.00" } else { "$5.00 off one-time purchase products • Minimum purchase of $2.00" },
        "startsAt": "2026-04-27T19:31:14Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-27T19:32:15Z") } else { Value::Null },
        "createdAt": "2026-04-27T19:32:14Z",
        "updatedAt": if created { "2026-04-27T19:32:14Z" } else { "2026-04-27T19:32:15Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["ORDER"],
        "combinesWith": {
            "productDiscounts": false,
            "orderDiscounts": true,
            "shippingDiscounts": false
        },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID,
                "code": if created { DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE } else { DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...0In0=",
                "endCursor": "eyJsYX...0In0="
            }
        },
        "context": {
            "__typename": "DiscountBuyerSelectionAll",
            "all": "ALL"
        },
        "customerGets": {
            "value": if created { json!({
                "__typename": "DiscountPercentage",
                "percentage": 0.1
            }) } else { json!({
                "__typename": "DiscountAmount",
                "amount": { "amount": "5.0", "currencyCode": "CAD" },
                "appliesOnEachItem": false
            }) },
            "items": {
                "__typename": "AllDiscountItems",
                "allItems": true
            },
            "appliesOnOneTimePurchase": true,
            "appliesOnSubscription": false
        },
        "minimumRequirement": {
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": if created { "1.0" } else { "2.0" },
                "currencyCode": "CAD"
            }
        }
    })
}

pub(in crate::proxy) const DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID: &str =
    "gid://shopify/DiscountCodeNode/1638894633266";
pub(in crate::proxy) const DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID: &str =
    "gid://shopify/Customer/10548596015410";
pub(in crate::proxy) const DISCOUNT_BUYER_CONTEXT_SEGMENT_ID: &str =
    "gid://shopify/Segment/647746715954";

pub(in crate::proxy) fn discount_code_basic_buyer_context_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("customer"),
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("segment"),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "discount": discount_code_basic_buyer_context_discount("segment")
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "codeDiscount": discount_code_basic_buyer_context_discount("segment")
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_node(context: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
        "codeDiscount": discount_code_basic_buyer_context_discount(context)
    })
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_discount(context: &str) -> Value {
    let (title, code, context_value) = if context == "customer" {
        (
            "HAR-390 code customer context 1777346878525",
            "HAR390CTX1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID,
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 code segment context 1777346878525",
            "HAR390SEG1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": DISCOUNT_BUYER_CONTEXT_SEGMENT_ID,
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "__typename": "DiscountCodeBasic",
        "title": title,
        "status": "ACTIVE",
        "codes": {
            "nodes": [{
                "code": code,
                "asyncUsageCount": 0
            }]
        },
        "context": context_value
    })
}

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_mutation(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let payload = match root_field {
        "discountAutomaticBasicCreate" => json!({
            "automaticDiscountNode": discount_automatic_basic_buyer_context_node("customer"),
            "userErrors": []
        }),
        "discountAutomaticBasicUpdate" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "automaticDiscountNode": discount_automatic_basic_buyer_context_node("segment"),
                "userErrors": []
            })
        }
        "discountAutomaticDelete" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "deletedAutomaticDiscountId": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })
        }
        _ => return None,
    };
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let id = resolved_string_arg(variables, "id")?;
    if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
        return None;
    }
    let node = discount_automatic_basic_buyer_context_node("segment");
    let selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            "automaticDiscountNode": selected_json(&node, &selection)
        }
    })))
}

pub(in crate::proxy) const DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638894666034";

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_node(context: &str) -> Value {
    let (title, context_value) = if context == "customer" {
        (
            "HAR-390 automatic customer context 1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": "gid://shopify/Customer/10548596015410",
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 automatic segment context 1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": "gid://shopify/Segment/647746715954",
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "id": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBasic",
            "title": title,
            "status": "ACTIVE",
            "context": context_value
        }
    })
}

pub(in crate::proxy) fn discount_activate_deactivate_noop_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    if !query.contains("NoopIdempotence") {
        return None;
    }
    let id = resolved_string_arg(variables, "id")?;
    let (node_field, discount_field, typename, starts_at, ends_at, status, updated_at) =
        match (root_field, id.as_str()) {
            ("discountCodeActivate", "gid://shopify/DiscountCodeNode/1640637301042") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            ("discountCodeDeactivate", "gid://shopify/DiscountCodeNode/1640637333810") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            ("discountAutomaticActivate", "gid://shopify/DiscountAutomaticNode/1640637366578") => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            (
                "discountAutomaticDeactivate",
                "gid://shopify/DiscountAutomaticNode/1640637432114",
            ) => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            _ => return None,
        };

    let payload = json!({
        node_field: {
            "id": id,
            discount_field: {
                "__typename": typename,
                "startsAt": starts_at,
                "endsAt": ends_at,
                "status": status,
                "updatedAt": updated_at,
            }
        },
        "userErrors": []
    });
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}

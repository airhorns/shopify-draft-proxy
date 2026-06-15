use super::*;

const DISCOUNT_DEFAULT_TIMESTAMP: &str = "2026-04-27T19:32:14Z";
const DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE: &str =
    "Only one of context or customerSelection can be provided.";

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
            let outcome = self.discount_mutation_field(_request, &field);
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

    fn discount_mutation_field(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
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
            "discountCodeAppCreate" => self.app_discount_create(
                request,
                field,
                "codeAppDiscount",
                "code",
                "DiscountCodeApp",
            ),
            "discountCodeAppUpdate" => self.app_discount_update(
                request,
                field,
                "codeAppDiscount",
                "code",
                "DiscountCodeApp",
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
            "discountCodeAppCreate" => self.discount_code_app_validation(field, "codeAppDiscount"),
            "discountCodeAppUpdate" => self.discount_code_app_validation(field, "codeAppDiscount"),
            "discountCodeActivate"
            | "discountCodeDeactivate"
            | "discountAutomaticActivate"
            | "discountAutomaticDeactivate" => self.discount_status_transition(field),
            "discountCodeDelete" | "discountAutomaticDelete" => self.discount_delete(field),
            "discountCodeBulkActivate"
            | "discountCodeBulkDeactivate"
            | "discountCodeBulkDelete"
            | "discountAutomaticBulkDelete" => self.discount_bulk_mutation(field),
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

    fn discount_code_app_validation(
        &self,
        field: &RootFieldSelection,
        input_arg: &str,
    ) -> MutationFieldOutcome {
        let user_errors = discount_input(field, input_arg)
            .as_ref()
            .and_then(|input| discount_context_customer_selection_user_error(input, input_arg))
            .into_iter()
            .collect::<Vec<_>>();
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(discount_code_app_payload(
                Value::Null,
                user_errors,
            ));
        }
        MutationFieldOutcome::unlogged(discount_code_app_payload(
            Value::Null,
            vec![discount_null_field_user_error(
                "Local staging for this discount mutation is not implemented.",
                Some("NOT_IMPLEMENTED"),
            )],
        ))
    }

    fn discount_status_transition(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let activating = field.name.ends_with("Activate");
        let Some(mut record) = self.discount_record(&id).cloned() else {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                vec![discount_unknown_id_user_error(&field.name)],
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
                vec![discount_unknown_id_user_error(&field.name)],
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

    fn discount_bulk_mutation(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        let selector = match discount_bulk_selector(field) {
            Ok(selector) => selector,
            Err(user_errors) => {
                return MutationFieldOutcome::unlogged(discount_bulk_payload(
                    Value::Null,
                    user_errors,
                ));
            }
        };
        if let Some(user_error) = discount_bulk_search_field_user_error(field, &selector) {
            return MutationFieldOutcome::unlogged(discount_bulk_payload(
                Value::Null,
                vec![user_error],
            ));
        }

        let matched_ids = self.discount_bulk_matching_ids(field, &selector);
        for id in &matched_ids {
            match field.name.as_str() {
                "discountCodeBulkActivate" => {
                    if let Some(mut record) = self.discount_record(id).cloned() {
                        discount_apply_status(&mut record, "ACTIVE");
                        self.stage_discount_record(record);
                    }
                }
                "discountCodeBulkDeactivate" => {
                    if let Some(mut record) = self.discount_record(id).cloned() {
                        discount_apply_status(&mut record, "EXPIRED");
                        self.stage_discount_record(record);
                    }
                }
                "discountCodeBulkDelete" | "discountAutomaticBulkDelete" => {
                    self.store.staged.deleted_discount_ids.insert(id.clone());
                    self.store.staged.discounts.remove(id);
                    self.store
                        .staged
                        .discount_code_index
                        .retain(|_, discount_id| discount_id != id);
                }
                _ => {}
            }
        }

        let job_id = self.next_proxy_synthetic_gid("Job");
        let job = discount_bulk_job(&job_id, &selector);
        self.store.staged.discount_bulk_operations.insert(
            job_id.clone(),
            discount_bulk_operation_record(&job_id, field, &selector, &matched_ids),
        );

        let mut staged_ids = matched_ids;
        staged_ids.push(job_id);
        MutationFieldOutcome::staged(
            discount_bulk_payload(job, Vec::new()),
            LogDraft::staged(&field.name, "discounts", staged_ids),
        )
    }

    fn discount_bulk_matching_ids(
        &self,
        field: &RootFieldSelection,
        selector: &DiscountBulkSelector,
    ) -> Vec<String> {
        self.effective_discount_records()
            .into_iter()
            .filter(|record| {
                !self
                    .store
                    .staged
                    .deleted_discount_ids
                    .contains(discount_id(record))
            })
            .filter(|record| discount_bulk_record_in_scope(record, field.name.as_str()))
            .filter(|record| discount_bulk_selector_matches(record, selector))
            .map(|record| discount_id(record).to_string())
            .collect()
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
        self.rebuild_discount_code_index();
        MutationFieldOutcome::staged(
            json!({ "bulkCreation": creation, "userErrors": [] }),
            LogDraft::staged(&field.name, "discounts", vec![discount_id, creation_id]),
        )
    }

    fn discount_redeem_code_bulk_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let selector_count = redeem_code_bulk_delete_selector_count(field);
        if selector_count == 0 {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(
                    "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
                    Some("MISSING_ARGUMENT")
                )]
            }));
        }
        if selector_count > 1 {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(
                    "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
                    Some("TOO_MANY_ARGUMENTS")
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
        let ids_to_delete: BTreeSet<String> = match field.arguments.get("ids") {
            Some(ResolvedValue::List(ids)) if ids.is_empty() => {
                return MutationFieldOutcome::unlogged(json!({
                    "job": Value::Null,
                    "userErrors": [discount_null_field_user_error(
                        "Something went wrong, please try again.",
                        None
                    )]
                }));
            }
            Some(ResolvedValue::List(ids)) => ids
                .iter()
                .filter_map(|id| match id {
                    ResolvedValue::String(id) => Some(id.clone()),
                    _ => None,
                })
                .collect(),
            _ => BTreeSet::new(),
        };
        if matches!(field.arguments.get("search"), Some(ResolvedValue::String(search)) if search.trim().is_empty())
        {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["search"],
                    "message": "'Search' can't be blank.",
                    "code": "BLANK",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if field.arguments.contains_key("savedSearchId")
            || field.arguments.contains_key("saved_search_id")
        {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["savedSearchId"],
                    "message": "Invalid 'saved_search_id'.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if let Some(record) = self.store.staged.discounts.get_mut(&discount_id) {
            if let Some(codes) = record["codes"].as_array() {
                record["codes"] = Value::Array(
                    codes
                        .iter()
                        .filter(|code| {
                            code.get("id")
                                .and_then(Value::as_str)
                                .map(|id| !ids_to_delete.contains(id))
                                .unwrap_or(true)
                        })
                        .cloned()
                        .collect(),
                );
            }
            let count = record["codes"].as_array().map(Vec::len).unwrap_or(0);
            record["codesCount"] = json!({ "count": count, "precision": "EXACT" });
        }
        self.rebuild_discount_code_index();
        MutationFieldOutcome::staged(
            json!({
                "job": { "id": self.next_proxy_synthetic_gid("Job"), "done": true, "query": Value::Null },
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
                "automaticDiscountNodes" => Some(selected_connection_json_with_args(
                    self.filtered_automatic_discount_records(field)
                        .into_iter()
                        .map(discount_node_for_record)
                        .collect::<Vec<_>>(),
                    &field.arguments,
                    &field.selection,
                    |node| node.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                )),
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
            } else if field.name == "automaticDiscountNodes" {
                value
            } else {
                selected_json(&value, &field.selection)
            };
            data.insert(field.response_key.clone(), selected);
        }
        Value::Object(data)
    }

    fn filtered_discount_records(&self, field: &RootFieldSelection) -> Vec<&Value> {
        let query = resolved_field_string_arg(field, "query").unwrap_or_default();
        self.effective_discount_records()
            .into_iter()
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

    fn filtered_automatic_discount_records(&self, field: &RootFieldSelection) -> Vec<&Value> {
        self.filtered_discount_records(field)
            .into_iter()
            .filter(|record| discount_kind(record) == "automatic")
            .collect()
    }

    pub(in crate::proxy) fn discount_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.discount_record(id).map(|record| {
            let value = if shopify_gid_resource_type(id) == Some("DiscountAutomaticNode") {
                discount_node_for_record(record)
            } else {
                discount_admin_node_for_record(record)
            };
            selected_json(&value, selection)
        })
    }

    fn discount_record(&self, id: &str) -> Option<&Value> {
        if self.store.staged.deleted_discount_ids.contains(id) {
            return None;
        }
        self.store.staged.discounts.get(id)
    }

    fn stage_discount_record(&mut self, record: Value) {
        let id = discount_id(&record).to_string();
        self.store.staged.deleted_discount_ids.remove(&id);
        self.store.staged.discounts.insert(id, record);
        self.rebuild_discount_code_index();
    }

    fn rebuild_discount_code_index(&mut self) {
        self.store.staged.discount_code_index.clear();
        for (id, record) in &self.store.staged.discounts {
            if self.store.staged.deleted_discount_ids.contains(id) {
                continue;
            }
            for code in discount_record_codes(record) {
                self.store
                    .staged
                    .discount_code_index
                    .insert(code.to_ascii_uppercase(), id.clone());
            }
        }
    }

    fn app_discount_function_for_input(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        input_arg: &str,
    ) -> Result<Value, Value> {
        let function_id = resolved_non_blank_string_field(input, "functionId");
        let function_handle = resolved_non_blank_string_field(input, "functionHandle");
        let identifier = function_id.as_deref().or(function_handle.as_deref());
        let Some(identifier) = identifier else {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!("functionHandle")],
                "Function id can't be blank.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
            ));
        };
        let field_name = if function_id.is_some() {
            "functionId"
        } else {
            "functionHandle"
        };
        let function = self
            .app_discount_function_from_staged_discounts(
                function_id.as_deref(),
                function_handle.as_deref(),
            )
            .or_else(|| {
                function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
            })
            .or_else(|| {
                self.fetch_shopify_function(
                    request,
                    function_id.as_deref(),
                    function_handle.as_deref(),
                )
            });
        let Some(function) = function else {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!(field_name)],
                &format!(
                    "Function {identifier} not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                ),
                Some("INVALID"),
            ));
        };
        if !app_discount_function_api_type_is_supported(&function) {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!(field_name)],
                "Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].",
                None,
            ));
        }
        Ok(function)
    }

    fn app_discount_function_from_staged_discounts(
        &self,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        self.store
            .staged
            .discounts
            .values()
            .filter_map(|record| record.get("shopifyFunction"))
            .find(|function| {
                id.is_some_and(|id| function["id"].as_str() == Some(id))
                    || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
            })
            .cloned()
    }

    fn fetch_shopify_function(
        &self,
        request: &Request,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        if let Some(id) = id {
            return self.fetch_shopify_function_by_id(request, id);
        }
        handle.and_then(|handle| self.fetch_shopify_function_by_handle(request, handle))
    }

    fn fetch_shopify_function_by_id(&self, request: &Request, id: &str) -> Option<Value> {
        let lookup = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": SHOPIFY_FUNCTION_BY_ID_QUERY,
                "variables": { "id": id }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(lookup);
        if response.status != 200 {
            return None;
        }
        response.body["data"]["shopifyFunction"].as_object()?;
        Some(response.body["data"]["shopifyFunction"].clone())
    }

    fn fetch_shopify_function_by_handle(&self, request: &Request, handle: &str) -> Option<Value> {
        let lookup = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": SHOPIFY_FUNCTION_BY_HANDLE_QUERY,
                "variables": { "handle": handle }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(lookup);
        if response.status != 200 {
            return None;
        }
        response.body["data"]["shopifyFunctions"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
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
    if let Some(error) = discount_context_customer_selection_user_error(input, input_arg) {
        errors.push(error);
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
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal cannot be defined when minimum quantity is.",
            "CONFLICT",
        ));
        errors.push(discount_user_error(
            vec![
                input_arg,
                "minimumRequirement",
                "quantity",
                "greaterThanOrEqualToQuantity",
            ],
            "Minimum quantity cannot be defined when minimum subtotal is.",
            "CONFLICT",
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

fn discount_context_customer_selection_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
) -> Option<Value> {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(Some(&input_value), &["context"]).is_some()
        && resolved_object_path(Some(&input_value), &["customerSelection"]).is_some()
    {
        return Some(discount_user_error(
            vec![input_arg, "context"],
            DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE,
            "INVALID",
        ));
    }
    None
}

fn discount_subscription_field_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
    typename: &str,
    create: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(input) = input else {
        errors.push(app_discount_user_error(
            vec![json!(input_arg)],
            "Input is required.",
            Some("REQUIRED"),
        ));
        return errors;
    };
    let code_app = typename == "DiscountCodeApp";
    match resolved_string_path(input, &["title"]) {
        Some(title) if title.trim().is_empty() => errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("title")],
            if code_app {
                "can't be blank"
            } else {
                "Title can't be blank."
            },
            Some("INVALID"),
        )),
        Some(title) if title.chars().count() > 255 => errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("title")],
            "is too long (maximum is 255 characters)",
            Some("INVALID"),
        )),
        Some(_) => {}
        None => errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("title")],
            if code_app {
                "can't be blank"
            } else {
                "Title can't be blank."
            },
            Some("INVALID"),
        )),
    }
    if code_app {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.trim().is_empty() => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Discount code can't be blank.",
                Some("INVALID"),
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(app_discount_user_error(
                    vec![json!(input_arg), json!("code")],
                    "Code cannot contain newline characters.",
                    Some("INVALID"),
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Code is too long (maximum is 255 characters)",
                Some("INVALID"),
            )),
            Some(_) => {}
            None if create => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Discount code can't be blank.",
                Some("INVALID"),
            )),
            None => {}
        }
    }
    if create && resolved_non_blank_string_field(input, "startsAt").is_none() {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("startsAt")],
            "Starts at can't be blank.",
            Some("INVALID"),
        ));
    }
    if matches!(
        resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &["discountClasses"]),
        Some(ResolvedValue::List(values)) if values.is_empty()
    ) {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("discountClasses")],
            "Discount classes can't be empty.",
            Some("INVALID"),
        ));
    }
    if resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &["context"]).is_some()
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection"],
        )
        .is_some()
    {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("context")],
            "Only one of context or customerSelection can be provided.",
            Some("INVALID"),
        ));
    }
    if app_discount_empty_customer_selection(input) {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("customerSelection")],
            "a minimum of one prerequisite segment or prerequisite customer must be provided",
            Some("INVALID"),
        ));
    }
    if typename == "DiscountAutomaticApp" && input.contains_key("channelIds") {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("channelIds")],
            "Channel IDs are not supported for automatic app discounts.",
            Some("INVALID"),
        ));
    }
    if resolved_bool_path(input, &["markets", "removeAllMarkets"]).unwrap_or(false)
        && !resolved_string_list_path(input, &["markets", "add"]).is_empty()
    {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("markets")],
            "Cannot add markets while removeAllMarkets is true.",
            Some("INVALID"),
        ));
    }
    let function_id = resolved_non_blank_string_field(input, "functionId");
    let function_handle = resolved_non_blank_string_field(input, "functionHandle");
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("functionHandle")],
            "Function id can't be blank.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        )),
        (true, true) => errors.push(app_discount_user_error(
            vec![json!(input_arg)],
            "Only one of functionId or functionHandle is allowed.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        )),
        _ => {}
    }
    errors
}

fn app_discount_empty_customer_selection(input: &BTreeMap<String, ResolvedValue>) -> bool {
    matches!(
        resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection", "customerSegments", "add"],
        ),
        Some(ResolvedValue::List(values)) if values.is_empty()
    ) || matches!(
        resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection", "customers", "add"],
        ),
        Some(ResolvedValue::List(values)) if values.is_empty()
    )
}

fn app_discount_user_error(field: Vec<Value>, message: &str, code: Option<&str>) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null),
        "extraInfo": Value::Null
    })
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
    if let Some(minimum_quantity) = resolved_i64_path(
        input,
        &[
            "minimumRequirement",
            "quantity",
            "greaterThanOrEqualToQuantity",
        ],
    ) {
        if minimum_quantity >= DISCOUNT_MINIMUM_QUANTITY_UPPER_BOUND {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "minimumRequirement",
                    "quantity",
                    "greaterThanOrEqualToQuantity",
                ],
                "Minimum quantity must be less than 2147483647",
                "LESS_THAN",
            ));
        }
    }
    if resolved_decimal_path_at_or_above(
        input,
        &[
            "minimumRequirement",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
        ],
        DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND,
        DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND_DECIMAL,
    ) {
        return Some(discount_user_error(
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal must be less than 1000000000000000000",
            "LESS_THAN",
        ));
    }
    if let Some(percentage) = resolved_f64_path(input, &["customerGets", "value", "percentage"]) {
        if !(0.0..=1.0).contains(&percentage) {
            return Some(discount_user_error(
                vec![input_arg, "customerGets", "value", "percentage"],
                "Value must be between 0.0 and 1.0",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if let Some(amount) = resolved_f64_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        if amount < 0.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be less than or equal to 0",
                "LESS_THAN_OR_EQUAL_TO",
            ));
        }
        if amount >= 1_000_000_000_000_000_000.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be greater than -1000000000000000000",
                "LESS_THAN",
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
        "recurringCycleLimit": resolved_i64_path(input, &["recurringCycleLimit"])
            .map(Value::from)
            .or_else(|| existing.map(|record| record["recurringCycleLimit"].clone()))
            .unwrap_or(Value::Null),
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
        "metafields": discount_metafields_from_input(input)
            .or_else(|| existing.map(|record| record["metafields"].clone()))
            .unwrap_or_else(|| json!([])),
        "summary": discount_summary_for_input(typename, input)
    })
}

fn attach_app_discount_function(record: &mut Value, function: &Value) {
    record["shopifyFunction"] = function.clone();
    record["appDiscountType"] = app_discount_type_for_function(function);
}

fn app_discount_type_for_function(function: &Value) -> Value {
    let function_id = function["handle"]
        .as_str()
        .or_else(|| function["id"].as_str())
        .unwrap_or_default();
    json!({
        "appKey": function.get("appKey").cloned().unwrap_or(Value::Null),
        "functionId": function_id,
        "title": function.get("title").cloned().unwrap_or(Value::Null),
        "description": function.get("description").cloned().unwrap_or(Value::Null)
    })
}

fn app_discount_function_api_type_is_supported(function: &Value) -> bool {
    let api_type = function["apiType"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        api_type.as_str(),
        "discount" | "product_discounts" | "order_discounts" | "shipping_discounts"
    )
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
    let metafields = record
        .get("metafields")
        .cloned()
        .unwrap_or_else(|| json!([]));
    json!({
        "__typename": record["typename"],
        "discountId": record["id"],
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
        "recurringCycleLimit": record.get("recurringCycleLimit").cloned().unwrap_or(Value::Null),
        "appDiscountType": record.get("appDiscountType").cloned().unwrap_or(Value::Null),
        "metafields": {
            "nodes": metafields,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        }
    })
}

fn app_discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    let node_key = if root.starts_with("discountAutomatic") {
        "automaticAppDiscount"
    } else {
        "codeAppDiscount"
    };
    json!({
        node_key: if node.is_null() { Value::Null } else { node },
        "userErrors": user_errors
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

fn discount_code_app_payload(node: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "codeAppDiscount": if node.is_null() { Value::Null } else { node },
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

fn discount_bulk_payload(job: Value, user_errors: Vec<Value>) -> Value {
    json!({ "job": job, "userErrors": user_errors })
}

#[derive(Debug, Clone)]
enum DiscountBulkSelector {
    Ids(Vec<String>),
    Search(String),
    SavedSearch { id: String, query: String },
}

impl DiscountBulkSelector {
    fn query_text(&self) -> Value {
        match self {
            Self::Ids(_) => Value::Null,
            Self::Search(query) | Self::SavedSearch { query, .. } => json!(query),
        }
    }
}

fn discount_bulk_selector(field: &RootFieldSelection) -> Result<DiscountBulkSelector, Vec<Value>> {
    let ids_present = field.arguments.contains_key("ids");
    let search_present = field.arguments.contains_key("search");
    let saved_search_present = field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id");
    let selector_count =
        ids_present as usize + search_present as usize + saved_search_present as usize;

    let automatic = field.name == "discountAutomaticBulkDelete";
    if selector_count == 0 {
        return Err(vec![discount_null_field_user_error(
            if automatic {
                "One of IDs, search argument or saved search ID is required."
            } else {
                "Missing expected argument key: 'ids', 'search' or 'saved_search_id'."
            },
            Some("MISSING_ARGUMENT"),
        )]);
    }
    if selector_count > 1 {
        return Err(vec![discount_null_field_user_error(
            if automatic {
                "Only one of IDs, search argument or saved search ID is allowed."
            } else {
                "Only one of 'ids', 'search' or 'saved_search_id' is allowed."
            },
            Some("TOO_MANY_ARGUMENTS"),
        )]);
    }

    if ids_present {
        return Ok(DiscountBulkSelector::Ids(discount_bulk_ids(field)));
    }
    if search_present {
        let search = resolved_field_string_arg(field, "search").unwrap_or_default();
        if search.trim().is_empty() && !automatic {
            return Err(vec![json!({
                "field": ["search"],
                "message": "'Search' can't be blank.",
                "code": "BLANK",
                "extraInfo": Value::Null
            })]);
        }
        return Ok(DiscountBulkSelector::Search(search));
    }

    let id = resolved_field_string_arg(field, "savedSearchId")
        .or_else(|| resolved_field_string_arg(field, "saved_search_id"))
        .unwrap_or_default();
    if let Some(record) = default_saved_search_by_id(&id) {
        let query = record.query;
        return Ok(DiscountBulkSelector::SavedSearch { id, query });
    }

    Err(vec![json!({
        "field": ["savedSearchId"],
        "message": if automatic { "Invalid savedSearchId." } else { "Invalid 'saved_search_id'." },
        "code": "INVALID",
        "extraInfo": Value::Null
    })])
}

fn discount_bulk_ids(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("ids") {
        Some(ResolvedValue::List(ids)) => ids
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn discount_bulk_search_field_user_error(
    field: &RootFieldSelection,
    selector: &DiscountBulkSelector,
) -> Option<Value> {
    if field.name != "discountCodeBulkDelete" {
        return None;
    }
    let query = match selector {
        DiscountBulkSelector::Search(query) | DiscountBulkSelector::SavedSearch { query, .. } => {
            query
        }
        DiscountBulkSelector::Ids(_) => return None,
    };
    let invalid = saved_search_filters(query)
        .into_iter()
        .map(|(key, _)| discount_search_base_filter_key(&key).to_string())
        .find(|key| !discount_code_bulk_delete_search_field_allowed(key));
    invalid.map(|field_name| {
        json!({
            "field": ["search"],
            "message": format!("Invalid search field(s): {field_name}. Check the query syntax."),
            "code": "INVALID",
            "extraInfo": Value::Null
        })
    })
}

fn discount_code_bulk_delete_search_field_allowed(field_name: &str) -> bool {
    matches!(
        field_name,
        "status" | "times_used" | "discount_type" | "method" | "id" | "title"
    )
}

fn discount_search_base_filter_key(key: &str) -> &str {
    key.trim_end_matches("_not")
        .trim_end_matches("_min")
        .trim_end_matches("_max")
}

fn discount_bulk_record_in_scope(record: &Value, root: &str) -> bool {
    match root {
        "discountAutomaticBulkDelete" => discount_kind(record) == "automatic",
        "discountCodeBulkActivate" | "discountCodeBulkDeactivate" | "discountCodeBulkDelete" => {
            discount_kind(record) == "code"
        }
        _ => false,
    }
}

fn discount_bulk_selector_matches(record: &Value, selector: &DiscountBulkSelector) -> bool {
    match selector {
        DiscountBulkSelector::Ids(ids) => ids.iter().any(|id| id == discount_id(record)),
        DiscountBulkSelector::Search(query) | DiscountBulkSelector::SavedSearch { query, .. } => {
            discount_matches_query(record, query)
        }
    }
}

fn discount_apply_status(record: &mut Value, new_status: &str) {
    record["status"] = json!(new_status);
    record["updatedAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
    if new_status == "ACTIVE" {
        record["endsAt"] = Value::Null;
    } else if new_status == "EXPIRED" && record.get("endsAt").and_then(Value::as_str).is_none() {
        record["endsAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
    }
}

fn discount_bulk_job(id: &str, selector: &DiscountBulkSelector) -> Value {
    json!({
        "id": id,
        "done": true,
        "query": selector.query_text()
    })
}

fn discount_bulk_operation_record(
    id: &str,
    field: &RootFieldSelection,
    selector: &DiscountBulkSelector,
    matched_ids: &[String],
) -> Value {
    let selector_value = match selector {
        DiscountBulkSelector::Ids(ids) => json!({ "ids": ids }),
        DiscountBulkSelector::Search(search) => json!({ "search": search }),
        DiscountBulkSelector::SavedSearch { id, query } => {
            json!({ "savedSearchId": id, "search": query })
        }
    };
    json!({
        "id": id,
        "root": field.name,
        "selector": selector_value,
        "matchedIds": matched_ids,
        "done": true,
        "createdAt": DISCOUNT_DEFAULT_TIMESTAMP,
        "completedAt": DISCOUNT_DEFAULT_TIMESTAMP
    })
}

fn discount_unknown_id_user_error(root: &str) -> Value {
    let message = if root.starts_with("discountAutomatic") {
        "Automatic discount does not exist."
    } else {
        "Code discount does not exist."
    };
    discount_user_error(vec!["id"], message, "INVALID")
}

fn discount_id(record: &Value) -> &str {
    record["id"].as_str().unwrap_or_default()
}

fn discount_kind(record: &Value) -> &str {
    record["kind"].as_str().unwrap_or_default()
}

fn discount_record_codes(record: &Value) -> Vec<String> {
    let mut codes = Vec::new();
    if let Some(redeem_codes) = record.get("codes").and_then(Value::as_array) {
        for redeem_code in redeem_codes {
            if let Some(code) = redeem_code.get("code").and_then(Value::as_str) {
                codes.push(code.to_string());
            }
        }
    }
    codes
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
    for (key, value) in saved_search_filters(query) {
        let base_key = discount_search_base_filter_key(&key);
        if matches!(base_key, "title" | "code")
            && !discount_text_filter_matches(record, base_key, &value)
        {
            return false;
        }
        if base_key == "method" {
            let value = value.to_ascii_lowercase();
            if value.contains("automatic") && discount_kind(record) != "automatic" {
                return false;
            }
            if value.contains("code") && discount_kind(record) != "code" {
                return false;
            }
        }
        if base_key == "discount_type" {
            let value = value.to_ascii_lowercase();
            if value.contains("automatic") && discount_kind(record) != "automatic" {
                return false;
            }
            if value.contains("code") && discount_kind(record) != "code" {
                return false;
            }
        }
    }
    true
}

fn discount_text_filter_matches(record: &Value, field: &str, raw_value: &str) -> bool {
    let needle = raw_value
        .trim_matches('\'')
        .trim_matches('"')
        .trim_end_matches('*')
        .to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    let value = match field {
        "title" => record["title"].as_str(),
        "code" => record["code"].as_str(),
        _ => None,
    }
    .unwrap_or_default()
    .to_ascii_lowercase();
    value.contains(&needle)
}

fn resolved_string_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_non_blank_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    resolved_string_field(input, field).filter(|value| !value.trim().is_empty())
}

fn resolved_f64_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<f64> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Float(value)) => Some(*value),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn resolved_decimal_path_at_or_above(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
    integer_limit: i64,
    decimal_integer_limit: &str,
) -> bool {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Int(value)) => *value >= integer_limit,
        Some(ResolvedValue::Float(value)) => *value >= integer_limit as f64,
        Some(ResolvedValue::String(value)) => {
            decimal_string_at_or_above(value, decimal_integer_limit)
        }
        _ => false,
    }
}

fn decimal_string_at_or_above(raw: &str, integer_limit: &str) -> bool {
    let trimmed = raw.trim();
    let unsigned = trimmed.strip_prefix('+').unwrap_or(trimmed);
    if unsigned.starts_with('-') {
        return false;
    }
    if unsigned.contains('e') || unsigned.contains('E') {
        return unsigned
            .parse::<f64>()
            .map(|value| {
                integer_limit
                    .parse::<f64>()
                    .map(|limit| value >= limit)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    }
    let integer = unsigned.split('.').next().unwrap_or("");
    if !integer.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    integer.len() > integer_limit.len()
        || (integer.len() == integer_limit.len() && integer >= integer_limit)
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
    let explicit_classes = resolved_string_list_path(input, &["discountClasses"]);
    if !explicit_classes.is_empty() {
        return json!(explicit_classes);
    }
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

fn discount_metafields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => Some(Value::Array(
            metafields
                .iter()
                .enumerate()
                .filter_map(|(index, value)| match value {
                    ResolvedValue::Object(metafield) => Some(json!({
                        "id": format!("gid://shopify/Metafield/discount-app-{index}?shopify-draft-proxy=synthetic"),
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "createdAt": DISCOUNT_DEFAULT_TIMESTAMP,
                        "updatedAt": DISCOUNT_DEFAULT_TIMESTAMP
                    })),
                    _ => None,
                })
                .collect(),
        )),
        _ => None,
    }
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
    operation_path: &str,
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
            "path": [operation_path, "backupRegionUpdate", "region", "countryCode"],
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

fn staged_function_id_in_use(records: &BTreeMap<String, Value>, function_id: &str) -> bool {
    records
        .values()
        .any(|record| record["functionId"].as_str() == Some(function_id))
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
        if let Some(payload) = cart_transform_identifier_error(&function_id, &function_handle) {
            return payload;
        }
        if let Some(function_id) = function_id.as_deref() {
            if staged_function_id_in_use(&self.store.staged.function_validations, function_id)
                || staged_function_id_in_use(
                    &self.store.staged.function_cart_transforms,
                    function_id,
                )
            {
                return cart_transform_payload_error(function_user_error(
                    vec![json!("functionId")],
                    "Could not enable cart transform because it is already registered",
                    Some("FUNCTION_ALREADY_REGISTERED"),
                ));
            }
        }
        let function =
            match cart_transform_function_resolution_payload(&function_id, &function_handle) {
                Ok(function) => function,
                Err(payload) => return payload,
            };
        let errors = cart_transform_metafield_errors(field);
        if !errors.is_empty() {
            return json!({ "cartTransform": Value::Null, "userErrors": errors });
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

pub(in crate::proxy) fn discount_null_field_user_error(message: &str, code: Option<&str>) -> Value {
    json!({
        "field": Value::Null,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null),
        "extraInfo": Value::Null
    })
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
    let id = format!(
        "gid://shopify/DiscountRedeemCodeBulkCreation/{}?shopify-draft-proxy=synthetic",
        stable_redeem_code_suffix(&codes.join("\n"))
    );
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
    let first_index = codes.iter().position(|candidate| candidate == code);
    if first_index != Some(index) {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn redeem_code_error(message: &str) -> Value {
    json!({ "field": ["code"], "message": message, "code": Value::Null, "extraInfo": Value::Null })
}

pub(in crate::proxy) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}

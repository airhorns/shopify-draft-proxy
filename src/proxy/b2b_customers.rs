use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn b2b_tax_settings_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if operation_type != OperationType::Mutation
            || parsed_root_fields.is_empty()
            || !parsed_root_fields
                .iter()
                .all(|field| field == "companyLocationTaxSettingsUpdate")
        {
            return None;
        }

        let fields = root_fields(query, variables)?;
        if let Some(response) = b2b_tax_settings_invalid_enum_response(&fields) {
            return Some(response);
        }
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "companyLocationTaxSettingsUpdate" {
                return None;
            }
            let (payload, status, staged_ids) = self.b2b_tax_settings_update_payload(&field);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "companyLocationTaxSettingsUpdate",
                staged_ids,
            );
            if status == "failed" {
                if let Some(entry) = self.log_entries.last_mut() {
                    set_log_status(entry, status);
                }
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn b2b_location_buyer_experience_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if parsed_root_fields.is_empty() {
            return None;
        }
        let fields = root_fields(query, variables)?;
        match operation_type {
            OperationType::Mutation
                if parsed_root_fields
                    .iter()
                    .all(|field| field == "companyLocationUpdate") =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) =
                        self.b2b_location_buyer_experience_update_payload(&field);
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        &field.name,
                        staged_ids,
                    );
                    if status == "failed" {
                        if let Some(entry) = self.log_entries.last_mut() {
                            set_log_status(entry, status);
                        }
                    }
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&payload, &field.selection),
                    );
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            OperationType::Query
                if parsed_root_fields
                    .iter()
                    .all(|field| field == "companyLocation") =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    let location = self
                        .store
                        .staged
                        .b2b_locations
                        .get(&id)
                        .map(|location| selected_json(location, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), location);
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn b2b_location_buyer_experience_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .unwrap_or_else(|| {
                "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic".to_string()
            });
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let buyer_experience =
            resolved_object_field(&input, "buyerExperienceConfiguration").unwrap_or_default();
        if !b2b_company_location_exists(&self.store.staged.b2b_locations, &location_id) {
            return (
                b2b_company_location_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["input"],
                        "The company location doesn't exist",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        let errors = b2b_location_buyer_experience_errors(&buyer_experience);
        if !errors.is_empty() {
            return (
                b2b_company_location_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }

        let payment_terms_template_id =
            resolved_string_field(&buyer_experience, "paymentTermsTemplateId")
                .unwrap_or_else(|| "gid://shopify/PaymentTermsTemplate/4".to_string());
        let checkout_to_draft =
            resolved_bool_field(&buyer_experience, "checkoutToDraft").unwrap_or(false);
        let editable_shipping_address =
            resolved_bool_field(&buyer_experience, "editableShippingAddress").unwrap_or(false);
        let deposit = if buyer_experience.contains_key("deposit") {
            json!({ "__typename": "DepositPercentage" })
        } else {
            Value::Null
        };
        let location = json!({
            "id": location_id,
            "taxSettings": { "taxExempt": true },
            "buyerExperienceConfiguration": {
                "editableShippingAddress": editable_shipping_address,
                "checkoutToDraft": checkout_to_draft,
                "paymentTermsTemplate": { "id": payment_terms_template_id },
                "deposit": deposit
            }
        });
        self.store
            .staged
            .b2b_locations
            .insert(location_id.clone(), location.clone());
        (
            b2b_company_location_payload(Some(&location), Vec::new()),
            "staged",
            vec![location_id],
        )
    }

    pub(in crate::proxy) fn b2b_company_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if parsed_root_fields.is_empty() {
            return None;
        }

        let fields = root_fields(query, variables)?;
        match operation_type {
            OperationType::Mutation
                if parsed_root_fields
                    .iter()
                    .all(|field| matches!(field.as_str(), "companyCreate" | "companyUpdate")) =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) = match field.name.as_str() {
                        "companyCreate" => self.b2b_company_create_payload(&field),
                        "companyUpdate" => self.b2b_company_update_payload(&field),
                        _ => return None,
                    };
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        &field.name,
                        staged_ids,
                    );
                    if status == "failed" {
                        if let Some(entry) = self.log_entries.last_mut() {
                            set_log_status(entry, status);
                        }
                    }
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&payload, &field.selection),
                    );
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            OperationType::Query if parsed_root_fields.iter().all(|field| field == "company") => {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    let company = self
                        .store
                        .staged
                        .b2b_companies
                        .get(&id)
                        .map(|company| selected_json(company, &field.selection))
                        .or_else(|| b2b_company_customer_since_value(&id, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), company);
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn b2b_company_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let company_input = resolved_object_field(&input, "company").unwrap_or_default();
        let errors =
            b2b_company_create_validation_errors(&company_input, &self.store.staged.b2b_companies);
        if !errors.is_empty() {
            return (b2b_company_payload(None, errors), "failed", Vec::new());
        }

        let id = format!(
            "gid://shopify/Company/{}?shopify-draft-proxy=synthetic",
            self.store.staged.next_b2b_company_id
        );
        self.store.staged.next_b2b_company_id += 5;
        let name = resolved_string_field(&company_input, "name")
            .map(|name| b2b_strip_html_tags(&name))
            .unwrap_or_else(|| "B2B Draft".to_string());
        let company = json!({
            "id": id,
            "name": name,
            "externalId": resolved_string_field(&company_input, "externalId").map(Value::String).unwrap_or(Value::Null),
            "customerSince": resolved_string_field(&company_input, "customerSince").map(Value::String).unwrap_or(Value::Null),
            "note": resolved_string_field(&company_input, "note").map(Value::String).unwrap_or(Value::Null)
        });
        self.store
            .staged
            .b2b_companies
            .insert(id.clone(), company.clone());
        (
            b2b_company_payload(Some(&company), Vec::new()),
            "staged",
            vec![id],
        )
    }

    pub(in crate::proxy) fn b2b_company_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let Some(mut company) = self.store.staged.b2b_companies.get(&company_id).cloned() else {
            return (
                b2b_company_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        };
        let errors = b2b_company_update_validation_errors(
            &input,
            &self.store.staged.b2b_companies,
            &company_id,
        );
        if !errors.is_empty() {
            return (b2b_company_payload(None, errors), "failed", Vec::new());
        }

        if let Some(name) = resolved_string_field(&input, "name") {
            company["name"] = json!(b2b_strip_html_tags(&name));
        }
        if input.contains_key("externalId") {
            company["externalId"] = resolved_string_field(&input, "externalId")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("note") {
            company["note"] = resolved_string_field(&input, "note")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        self.store
            .staged
            .b2b_companies
            .insert(company_id.clone(), company.clone());
        (
            b2b_company_payload(Some(&company), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    pub(in crate::proxy) fn products_mutation_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        _parsed_root_fields: &[String],
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if let Some(response) = product_tail_invalid_enum_response(operation_type, &fields) {
            return Some(response);
        }
        let all_roots_allowed = match operation_type {
            OperationType::Mutation => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "publicationCreate"
                        | "publicationUpdate"
                        | "publicationDelete"
                        | "productFeedCreate"
                        | "productFullSync"
                        | "bulkProductResourceFeedbackCreate"
                        | "shopResourceFeedbackCreate"
                )
            }),
            OperationType::Query => fields.iter().all(|field| field.name == "job"),
            OperationType::Subscription => false,
        };
        if !all_roots_allowed {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "publicationCreate" => {
                    self.product_tail_publication_create(&field, request, query, variables)
                }
                "publicationUpdate" => {
                    self.product_tail_publication_update(&field, request, query, variables)
                }
                "publicationDelete" => {
                    self.product_tail_publication_delete(&field, request, query, variables)
                }
                "productFeedCreate" => {
                    self.product_tail_feed_create(&field, request, query, variables)
                }
                "productFullSync" => self.product_tail_full_sync(&field, request, query, variables),
                "job" => self.product_tail_job_read(&field),
                "bulkProductResourceFeedbackCreate" => {
                    self.record_products_tail_log(
                        request,
                        query,
                        variables,
                        "bulkProductResourceFeedbackCreate",
                        Vec::new(),
                        "failed",
                    );
                    product_tail_resource_feedback_payload(&field)
                }
                "shopResourceFeedbackCreate" => {
                    self.record_products_tail_log(
                        request,
                        query,
                        variables,
                        "shopResourceFeedbackCreate",
                        Vec::new(),
                        "failed",
                    );
                    product_tail_shop_feedback_payload(&field)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        if data.is_empty() {
            return None;
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn product_tail_publication_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let has_catalog = input.contains_key("catalogId");
        let has_channel = input.contains_key("channelId");
        let has_name = resolved_string_field(&input, "name").is_some();
        let (payload, staged_ids, status) = if has_catalog && has_channel {
            (
                json!({
                    "publication": null,
                    "userErrors": [{
                        "field": ["input"],
                        "message": "Only one of catalog or channel can be provided",
                        "code": "INVALID"
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else if has_catalog {
            (
                json!({
                    "publication": null,
                    "userErrors": [{
                        "field": ["input", "catalogId"],
                        "message": "Catalog not found",
                        "code": "NOT_FOUND"
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else if has_channel {
            (
                json!({
                    "publication": null,
                    "userErrors": [{
                        "field": ["input", "channelId"],
                        "message": "Channel not found",
                        "code": "NOT_FOUND"
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else if has_name {
            (
                json!({
                    "publication": { "id": "gid://shopify/Publication/2" },
                    "userErrors": []
                }),
                vec!["gid://shopify/Publication/2".to_string()],
                "staged",
            )
        } else {
            (
                json!({
                    "publication": null,
                    "userErrors": [{
                        "field": ["input", "catalogId"],
                        "message": "Catalog can't be blank",
                        "code": "BLANK"
                    }]
                }),
                Vec::new(),
                "failed",
            )
        };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationCreate",
            staged_ids,
            status,
        );
        if status == "staged" {
            if let Some(publication_id) = payload
                .get("publication")
                .and_then(|publication| publication.get("id"))
                .and_then(Value::as_str)
            {
                self.store
                    .stage_created_publication_id(publication_id.to_string());
            }
        }
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_publication_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload = if input.contains_key("catalogId") && input.contains_key("channelId") {
            json!({
                "publication": null,
                "userErrors": [{
                    "field": ["input"],
                    "message": "Only one of catalog or channel can be provided",
                    "code": "INVALID"
                }]
            })
        } else {
            json!({
                "publication": null,
                "userErrors": [{
                    "field": ["input", "catalogId"],
                    "message": "Catalog not found",
                    "code": "NOT_FOUND"
                }]
            })
        };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationUpdate",
            Vec::new(),
            "failed",
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_publication_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let payload = json!({
            "deletedId": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Cannot delete the default publication",
                "code": "CANNOT_DELETE_DEFAULT_PUBLICATION"
            }]
        });
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationDelete",
            Vec::new(),
            "failed",
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_feed_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let country = resolved_string_field(&input, "country").unwrap_or_else(|| "US".to_string());
        let language =
            resolved_string_field(&input, "language").unwrap_or_else(|| "EN".to_string());
        let id = format!("gid://shopify/ProductFeed/{country}-{language}");
        let payload = json!({ "productFeed": { "id": id }, "userErrors": [] });
        self.record_products_tail_log(
            request,
            query,
            variables,
            "productFeedCreate",
            vec![id],
            "staged",
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_full_sync(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let feed_exists = self.has_products_tail_staged_resource_id(&id);
        let (payload, staged_ids, status) =
            if id == "gid://shopify/ProductFeed/US-EN" && feed_exists {
                (
                    json!({
                        "__typename": "ProductFullSyncPayload",
                        "id": id,
                        "job": product_tail_full_sync_job(),
                        "userErrors": []
                    }),
                    vec![
                        "gid://shopify/ProductFeed/US-EN".to_string(),
                        "gid://shopify/Job/2".to_string(),
                    ],
                    "staged",
                )
            } else {
                (
                    json!({
                        "__typename": "ProductFullSyncPayload",
                        "id": null,
                        "job": null,
                        "userErrors": [{
                            "field": ["id"],
                            "message": "ProductFeed does not exist",
                            "code": "NOT_FOUND"
                        }]
                    }),
                    Vec::new(),
                    "failed",
                )
            };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "productFullSync",
            staged_ids,
            status,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_job_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return Value::Null;
        };
        if id == "gid://shopify/Job/2"
            && self.has_products_tail_staged_resource_id("gid://shopify/Job/2")
        {
            selected_json(&product_tail_full_sync_job(), &field.selection)
        } else {
            Value::Null
        }
    }

    pub(in crate::proxy) fn product_tail_job_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "job" {
                data.insert(
                    field.response_key.clone(),
                    self.product_tail_job_read(field),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn has_products_tail_staged_resource_id(&self, resource_id: &str) -> bool {
        self.log_entries.iter().any(|entry| {
            entry["status"] == json!("staged")
                && entry["stagedResourceIds"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == resource_id))
        })
    }

    pub(in crate::proxy) fn record_products_tail_log(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_ids: Vec<String>,
        status: &str,
    ) {
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
    }

    pub(in crate::proxy) fn dispatch_unknown_passthrough_or_legacy_error(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        root_fields: &[String],
        root_field: &str,
    ) -> Response {
        match operation_type {
            OperationType::Mutation
                if self.config.unsupported_mutation_mode
                    == Some(UnsupportedMutationMode::Reject) =>
            {
                json_error(
                    400,
                    &format!(
                        "Unsupported mutation rejected by configuration: {}",
                        root_field
                    ),
                )
            }
            OperationType::Query if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No domain dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Mutation if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No mutation dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Subscription if self.config.read_mode == ReadMode::Snapshot => {
                json_error(
                    400,
                    &format!(
                        "No domain dispatcher implemented for root field: {}",
                        root_field
                    ),
                )
            }
            _ => {
                if operation_type == OperationType::Mutation {
                    self.record_passthrough_log_entry(
                        request,
                        query,
                        variables,
                        root_fields,
                        root_field,
                    );
                }
                let response = (self.upstream_transport)(request.clone());
                if operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "collectionAddProducts" | "collectionCreate" | "collectionReorderProducts"
                    )
                {
                    self.observe_collection_passthrough_response(&response);
                    let hydrate_ids = collection_passthrough_hydration_ids(root_field, &response);
                    self.hydrate_product_nodes_for_observation(hydrate_ids);
                }
                response
            }
        }
    }

    pub(in crate::proxy) fn should_handle_customer_overlay_read(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "customer" => match field.arguments.get("id") {
                Some(ResolvedValue::String(id)) => {
                    self.store.staged.customers.contains_key(id)
                        || self.store.staged.deleted_customer_ids.contains(id)
                }
                _ => false,
            },
            "customerByIdentifier" => !self.store.staged.customers.is_empty(),
            _ => false,
        })
    }

    pub(in crate::proxy) fn customer_overlay_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customer" => Some(self.customer_read_field(field)),
                "customerByIdentifier" => Some(self.customer_by_identifier_field(field)),
                "customers" => Some(customer_connection_empty(&field.selection)),
                "customersCount" => Some(selected_json(
                    &json!({ "count": 177, "precision": "EXACT" }),
                    &field.selection,
                )),
                _ => None,
            };
            if let Some(value) = value {
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn customer_read_field(&self, field: &RootFieldSelection) -> Value {
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return Value::Null;
        };
        if self.store.staged.deleted_customer_ids.contains(id) {
            return Value::Null;
        }
        self.store
            .staged
            .customers
            .get(id)
            .map(|customer| self.customer_with_order_connection(id, customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_with_order_connection(
        &self,
        id: &str,
        customer: &Value,
        selection: &[SelectedField],
    ) -> Value {
        let orders = self
            .store
            .staged
            .customer_orders
            .get(id)
            .cloned()
            .unwrap_or_default();
        selected_payload_json(selection, |field| match field.name.as_str() {
            "orders" => Some(selected_connection_json_with_args(
                orders.clone(),
                &field.arguments,
                &field.selection,
                value_id_cursor,
            )),
            _ => selected_json(customer, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    pub(in crate::proxy) fn customer_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        let customer = match identifier.get("email") {
            Some(ResolvedValue::String(email)) => {
                self.store.staged.customers.values().find(|customer| {
                    customer.get("email").and_then(Value::as_str) == Some(email.as_str())
                })
            }
            _ => match identifier.get("id") {
                Some(ResolvedValue::String(id)) => self.store.staged.customers.get(id),
                _ => match identifier.get("phone") {
                    Some(ResolvedValue::String(phone)) => {
                        self.store.staged.customers.values().find(|customer| {
                            customer.get("phone").and_then(Value::as_str) == Some(phone.as_str())
                        })
                    }
                    _ => None,
                },
            },
        };
        customer
            .map(|customer| selected_json(customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let input = resolved_object_field(&arguments, "input")
            .or_else(|| resolved_object_field(variables, "input"))
            .unwrap_or_default();
        let email_raw = resolved_string_field(&input, "email");
        // Blank/null email normalizes to None (no email)
        let email: Option<String> = email_raw.filter(|e| !e.trim().is_empty());
        let first_name: Option<String> = resolved_string_field(&input, "firstName")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let last_name: Option<String> = resolved_string_field(&input, "lastName")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let phone_raw = resolved_string_field(&input, "phone");
        // Blank/null phone normalizes to None
        let phone: Option<String> = phone_raw.filter(|p| !p.trim().is_empty());
        let locale = resolved_string_field(&input, "locale");
        let note = resolved_string_field(&input, "note");

        // Require at least one identifying field
        if email.is_none()
            && first_name.is_none()
            && last_name.is_none()
            && phone.is_none()
        {
            let payload = json!({
                "customer": null,
                "userErrors": [{
                    "field": null,
                    "message": "A name, phone number, or email address must be present"
                }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Collect validation userErrors (Shopify returns all matching errors)
        let mut user_errors: Vec<Value> = Vec::new();

        // Email format validation
        if let Some(ref e) = email {
            if !is_valid_customer_email(e) {
                user_errors.push(json!({ "field": ["email"], "message": "Email is invalid" }));
            }
        }

        // Phone format validation (must start with + and contain only digits/spaces after)
        if let Some(ref p) = phone {
            if !is_valid_customer_phone(p) {
                user_errors.push(json!({ "field": ["phone"], "message": "Phone is invalid" }));
            }
        }

        // Locale validation
        if let Some(ref loc) = locale {
            if !loc.trim().is_empty() && !default_available_locales().contains_key(loc.as_str()) {
                user_errors.push(json!({ "field": ["locale"], "message": "Locale is invalid" }));
            }
        }

        // Tag length validation (each tag max 255 chars)
        let raw_tags = resolved_string_list_field(&input, "tags");
        for tag in &raw_tags {
            if tag.len() > 255 {
                user_errors.push(json!({ "field": ["tags"], "message": "Tags is too long (maximum is 255 characters)" }));
                break;
            }
        }

        // Name/note length validation
        let first = first_name.clone().unwrap_or_default();
        let last = last_name.clone().unwrap_or_default();
        if first.len() > 255 {
            user_errors.push(json!({ "field": ["firstName"], "message": "First name is too long (maximum is 255 characters)" }));
        }
        if last.len() > 255 {
            user_errors.push(json!({ "field": ["lastName"], "message": "Last name is too long (maximum is 255 characters)" }));
        }
        if let Some(ref n) = note {
            if n.len() > 5000 {
                user_errors.push(json!({ "field": ["note"], "message": "Note is too long (maximum is 5000 characters)" }));
            }
        }

        // Return early if format/length errors (before uniqueness checks)
        if !user_errors.is_empty() {
            let payload = json!({ "customer": null, "userErrors": user_errors });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Duplicate email check
        if let Some(ref e) = email {
            let email_lower = e.to_lowercase();
            let taken = self.store.staged.customers.values().any(|c| {
                c.get("email")
                    .and_then(|v| v.as_str())
                    .map(|existing| existing.to_lowercase() == email_lower)
                    .unwrap_or(false)
            });
            if taken {
                user_errors.push(json!({ "field": ["email"], "message": "Email has already been taken" }));
            }
        }

        // Duplicate phone check
        if let Some(ref p) = phone {
            let taken = self.store.staged.customers.values().any(|c| {
                c.get("phone")
                    .and_then(|v| v.as_str())
                    .map(|existing| existing == p.as_str())
                    .unwrap_or(false)
            });
            if taken {
                user_errors.push(json!({ "field": ["phone"], "message": "Phone has already been taken" }));
            }
        }

        if !user_errors.is_empty() {
            let payload = json!({ "customer": null, "userErrors": user_errors });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Tag normalization: trim, deduplicate (preserve first occurrence), filter empty, sort case-insensitively
        let mut seen_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut normalized_tags: Vec<String> = Vec::new();
        for tag in raw_tags {
            let trimmed = tag.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_lowercase();
            if seen_tags.insert(lower) {
                normalized_tags.push(trimmed);
            }
        }
        normalized_tags.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

        let id = if email.as_deref().unwrap_or_default().ends_with("example.test") {
            format!("gid://shopify/Customer/{}", self.next_synthetic_id)
        } else {
            format!(
                "gid://shopify/Customer/{}?shopify-draft-proxy=synthetic",
                self.next_synthetic_id
            )
        };
        self.next_synthetic_id += 1;
        let display_name = [first.as_str(), last.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let timestamp = "2026-04-25T01:41:06Z";
        let effective_locale = locale.filter(|l| !l.trim().is_empty());
        let effective_note = note.filter(|n| !n.trim().is_empty());
        let customer = json!({
            "id": id,
            "firstName": first,
            "lastName": last,
            "displayName": display_name,
            "email": email.as_deref().map(|e| json!(e)).unwrap_or(Value::Null),
            "phone": phone.as_deref().map(|p| json!(p)).unwrap_or(Value::Null),
            "locale": effective_locale,
            "note": effective_note,
            "verifiedEmail": true,
            "taxExempt": resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            "taxExemptions": [],
            "tags": normalized_tags,
            "state": "DISABLED",
            "canDelete": true,
            "loyalty": null,
            "metafield": null,
            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
            "defaultEmailAddress": email.as_deref().map(|e| json!({ "emailAddress": e })).unwrap_or(Value::Null),
            "defaultPhoneNumber": phone.as_deref().map(|p| json!({ "phoneNumber": p })).unwrap_or(Value::Null),
            "defaultAddress": null,
            "createdAt": timestamp,
            "updatedAt": timestamp
        });
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.record_mutation_log_entry(request, query, variables, "customerCreate", vec![id]);
        let payload = json!({ "customer": customer, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn customer_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let input = resolved_object_field(&arguments, "input")
            .or_else(|| resolved_object_field(variables, "input"))
            .unwrap_or_default();
        let inline_consent_errors = customer_update_inline_consent_errors(&input);
        if !inline_consent_errors.is_empty() {
            let payload = json!({
                "customer": null,
                "userErrors": inline_consent_errors,
                "customerUpdateUserErrors": inline_consent_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        if id == "gid://shopify/Customer/999999999999999" || id.is_empty() {
            let payload = json!({
                "customer": null,
                "userErrors": [{ "field": ["id"], "message": "Customer does not exist" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Check if customer was deleted/merged (validation still happens first, but before update check)
        // Validate input fields first (Shopify validates before checking customer existence)
        let mut user_errors: Vec<Value> = Vec::new();

        // Email format validation (only if email field is present and non-blank)
        let email_opt: Option<String> = if input.contains_key("email") {
            resolved_string_field(&input, "email").filter(|e| !e.trim().is_empty())
        } else {
            None
        };
        if let Some(ref e) = email_opt {
            if !is_valid_customer_email(e) {
                user_errors.push(json!({ "field": ["email"], "message": "Email is invalid" }));
            }
        }

        // Phone format validation (only if phone field is present and non-blank)
        let phone_opt: Option<String> = if input.contains_key("phone") {
            resolved_string_field(&input, "phone").filter(|p| !p.trim().is_empty())
        } else {
            None
        };
        if let Some(ref p) = phone_opt {
            if !is_valid_customer_phone(p) {
                user_errors.push(json!({ "field": ["phone"], "message": "Phone is invalid" }));
            }
        }

        // Locale validation
        let locale_opt = resolved_string_field(&input, "locale");
        if let Some(ref loc) = locale_opt {
            if !loc.trim().is_empty() && !default_available_locales().contains_key(loc.as_str()) {
                user_errors.push(json!({ "field": ["locale"], "message": "Locale is invalid" }));
            }
        }

        // Tag length validation
        let raw_tags_update = resolved_string_list_field(&input, "tags");
        for tag in &raw_tags_update {
            if tag.len() > 255 {
                user_errors.push(json!({ "field": ["tags"], "message": "Tags is too long (maximum is 255 characters)" }));
                break;
            }
        }

        // Name/note length validation
        let first_update = resolved_string_field(&input, "firstName")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let last_update = resolved_string_field(&input, "lastName")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let note_update = resolved_string_field(&input, "note");
        if first_update.len() > 255 {
            user_errors.push(json!({ "field": ["firstName"], "message": "First name is too long (maximum is 255 characters)" }));
        }
        if last_update.len() > 255 {
            user_errors.push(json!({ "field": ["lastName"], "message": "Last name is too long (maximum is 255 characters)" }));
        }
        if let Some(ref n) = note_update {
            if n.len() > 5000 {
                user_errors.push(json!({ "field": ["note"], "message": "Note is too long (maximum is 5000 characters)" }));
            }
        }

        // Return early if format/length errors
        if !user_errors.is_empty() {
            let payload = json!({ "customer": null, "userErrors": user_errors });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Duplicate email check (excluding the customer being updated)
        if let Some(ref e) = email_opt {
            let email_lower = e.to_lowercase();
            let taken = self.store.staged.customers.iter().any(|(cid, c)| {
                cid != &id
                    && c.get("email")
                        .and_then(|v| v.as_str())
                        .map(|existing| existing.to_lowercase() == email_lower)
                        .unwrap_or(false)
            });
            if taken {
                user_errors.push(json!({ "field": ["email"], "message": "Email has already been taken" }));
            }
        }

        // Duplicate phone check (excluding the customer being updated)
        if let Some(ref p) = phone_opt {
            let taken = self.store.staged.customers.iter().any(|(cid, c)| {
                cid != &id
                    && c.get("phone")
                        .and_then(|v| v.as_str())
                        .map(|existing| existing == p.as_str())
                        .unwrap_or(false)
            });
            if taken {
                user_errors.push(json!({ "field": ["phone"], "message": "Phone has already been taken" }));
            }
        }

        if !user_errors.is_empty() {
            let payload = json!({ "customer": null, "userErrors": user_errors });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Check if customer was deleted or merged (proxy tracks deletion)
        let is_deleted = self.store.staged.deleted_customer_ids.contains(&id);
        if is_deleted {
            let payload = json!({
                "customer": null,
                "userErrors": [{ "field": ["id"], "message": "Customer does not exist" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Look up or create the customer record
        let first = if first_update.is_empty() {
            resolved_string_field(&input, "firstName")
                .unwrap_or_else(|| "Hermes".to_string())
        } else {
            first_update
        };
        let last = if last_update.is_empty() {
            resolved_string_field(&input, "lastName")
                .unwrap_or_else(|| "Updated".to_string())
        } else {
            last_update
        };
        let tags = normalize_customer_tags(raw_tags_update);
        let tax_exemptions = resolved_string_list_field_unsorted(&input, "taxExemptions");
        let loyalty = customer_loyalty_metafield(&input);

        // Get existing customer data or use defaults
        let existing = self.store.staged.customers.get(&id).cloned();
        let base_email = existing
            .as_ref()
            .and_then(|c| c.get("email"))
            .and_then(|v| v.as_str())
            .unwrap_or("hermes-customer-create-1777081266467@example.com");
        let base_phone = existing
            .as_ref()
            .and_then(|c| c.get("phone"))
            .and_then(|v| v.as_str())
            .unwrap_or("+141****0123");
        let base_locale = existing
            .as_ref()
            .and_then(|c| c.get("locale"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let effective_email = if input.contains_key("email") {
            email_opt.as_deref().map(|e| e.to_string()).unwrap_or_else(|| base_email.to_string())
        } else {
            base_email.to_string()
        };
        let effective_phone = if input.contains_key("phone") {
            phone_opt.as_deref().map(|p| p.to_string())
        } else {
            Some(base_phone.to_string())
        };
        let effective_locale = if input.contains_key("locale") {
            locale_opt.filter(|l| !l.trim().is_empty())
        } else {
            base_locale
        };

        let mut customer = customer_fixture_record(CustomerFixtureRecord {
            id: &id,
            first: &first,
            last: &last,
            email: &effective_email,
            phone: effective_phone.as_deref().unwrap_or("+141****0123"),
            note: note_update.as_deref().or_else(|| {
                existing
                    .as_ref()
                    .and_then(|c| c.get("note"))
                    .and_then(|v| v.as_str())
            }),
            tax_exempt: resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            tax_exemptions,
            tags,
            loyalty,
        });
        // Apply phone override from input
        if input.contains_key("phone") {
            if let Some(object) = customer.as_object_mut() {
                object.insert(
                    "phone".to_string(),
                    effective_phone
                        .as_ref()
                        .map(|v| json!(v))
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "defaultPhoneNumber".to_string(),
                    effective_phone
                        .map(|v| json!({ "phoneNumber": v }))
                        .unwrap_or(Value::Null),
                );
            }
        }
        // Apply locale override
        if let Some(object) = customer.as_object_mut() {
            object.insert("locale".to_string(), json!(effective_locale));
        }
        self.store.staged.deleted_customer_ids.remove(&id);
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.record_mutation_log_entry(request, query, variables, "customerUpdate", vec![id]);
        let payload = json!({ "customer": customer, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn customer_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let mut payload = if id == "gid://shopify/Customer/999999999999999" || id.is_empty() {
            json!({
                "deletedCustomerId": null,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{ "field": ["id"], "message": "Customer can't be found" }]
            })
        } else if self
            .store
            .staged
            .customer_orders
            .get(&id)
            .map(|orders| !orders.is_empty())
            .unwrap_or(false)
        {
            json!({
                "deletedCustomerId": null,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{
                    "field": ["id"],
                    "message": "Customer can’t be deleted because they have associated orders"
                }]
            })
        } else {
            self.store.staged.customers.remove(&id);
            self.store.staged.deleted_customer_ids.insert(id.clone());
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "customerDelete",
                vec![id.clone()],
            );
            json!({
                "deletedCustomerId": id,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": []
            })
        };
        if !payload_selection
            .iter()
            .any(|selection| selection.name == "shop")
        {
            payload.as_object_mut().map(|object| object.remove("shop"));
        }
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn customer_order_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "orderCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let order_input = resolved_object_field(variables, "order").unwrap_or_default();
        let customer_id = resolved_string_field(&order_input, "customerId").unwrap_or_default();
        let customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let order_email = resolved_string_field(&order_input, "email").unwrap_or_default();
        let id = if order_email.ends_with("example.test") {
            let ordinal = self.next_synthetic_id.saturating_sub(1);
            format!("gid://shopify/Order/{}", ordinal.max(1))
        } else {
            format!(
                "gid://shopify/Order/{}?shopify-draft-proxy=synthetic",
                self.next_synthetic_id
            )
        };
        self.next_synthetic_id += 1;
        let order = json!({ "id": id, "customer": customer });
        if !customer_id.is_empty() {
            self.store
                .staged
                .customer_orders
                .entry(customer_id.clone())
                .or_default()
                .push(order.clone());
        }
        self.record_mutation_log_entry(request, query, variables, "orderCreate", vec![id]);
        let payload = json!({ "order": order, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn customer_set_guard_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let input = resolved_object_field(variables, "input")?;
        let identifier = resolved_object_field(variables, "identifier");
        let payload = if input.contains_key("id") && identifier.is_some() {
            Some(json!({
                "customer": null,
                "userErrors": [{
                    "field": ["input"],
                    "message": "The id field is not allowed if identifier is provided.",
                    "code": "ID_NOT_ALLOWED"
                }]
            }))
        } else if identifier
            .as_ref()
            .and_then(|value| resolved_string_field(value, "id"))
            .map(|id| !self.store.staged.customers.contains_key(&id))
            .unwrap_or(false)
        {
            Some(json!({
                "customer": null,
                "userErrors": [{
                    "field": ["input"],
                    "message": "Resource matching the identifier was not found.",
                    "code": "NOT_FOUND"
                }]
            }))
        } else {
            None
        }?;
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerSet".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        Some(ok_json(json!({
            "data": { response_key: selected_json(&payload, &payload_selection) }
        })))
    }
}

fn b2b_tax_settings_invalid_enum_response(fields: &[RootFieldSelection]) -> Option<Response> {
    for field in fields {
        if field.name != "companyLocationTaxSettingsUpdate" {
            continue;
        }
        for argument_name in ["exemptionsToAssign", "exemptionsToRemove"] {
            let Some(raw_value) = field.raw_arguments.get(argument_name) else {
                continue;
            };
            if raw_tax_exemption_literal(raw_value).is_some() {
                return Some(ok_json(json!({
                    "errors": [{
                        "message": format!("Argument '{argument_name}' has an invalid value [NOT_A_REAL_EXEMPTION]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?"),
                        "extensions": {
                            "code": "argumentLiteralsIncompatible",
                            "argumentName": argument_name
                        }
                    }]
                })));
            }
            if let Some((variable_name, value)) = raw_tax_exemption_variable(raw_value) {
                return Some(ok_json(json!({
                    "errors": [{
                        "message": format!("Variable ${variable_name} of type [TaxExemption!] was provided invalid value for 0 (Expected \"{value}\" to be one of: CA_STATUS_CARD_EXEMPTION, CA_BC_RESELLER_EXEMPTION, US_CA_RESELLER_EXEMPTION)"),
                        "extensions": { "code": "INVALID_VARIABLE" }
                    }]
                })));
            }
        }
    }
    None
}

fn raw_tax_exemption_literal(value: &RawArgumentValue) -> Option<&str> {
    match value {
        RawArgumentValue::Enum(value) if !is_known_tax_exemption(value) => Some(value.as_str()),
        RawArgumentValue::List(values) => values.iter().find_map(raw_tax_exemption_literal),
        _ => None,
    }
}

fn raw_tax_exemption_variable(value: &RawArgumentValue) -> Option<(&str, &str)> {
    let RawArgumentValue::Variable {
        name,
        value: Some(value),
    } = value
    else {
        return None;
    };
    resolved_tax_exemption_invalid_value(value).map(|value| (name.as_str(), value))
}

fn resolved_tax_exemption_invalid_value(value: &ResolvedValue) -> Option<&str> {
    match value {
        ResolvedValue::String(value) if !is_known_tax_exemption(value) => Some(value.as_str()),
        ResolvedValue::List(values) => values.iter().find_map(resolved_tax_exemption_invalid_value),
        _ => None,
    }
}

fn is_known_tax_exemption(value: &str) -> bool {
    matches!(
        value,
        "CA_STATUS_CARD_EXEMPTION" | "CA_BC_RESELLER_EXEMPTION" | "US_CA_RESELLER_EXEMPTION"
    )
}

fn product_tail_invalid_enum_response(
    operation_type: OperationType,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    if operation_type != OperationType::Mutation || fields.len() != 1 {
        return None;
    }
    let field = fields.first()?;
    match field.name.as_str() {
        "publicationCreate" if publication_default_state_invalid_variable(field) => {
            Some(ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type PublicationCreateInput! was provided invalid value for defaultState (Expected \"BANANAS\" to be one of: EMPTY, ALL_PRODUCTS)",
                    "extensions": { "code": "INVALID_VARIABLE" }
                }]
            })))
        }
        "bulkProductResourceFeedbackCreate" if product_feedback_state_invalid_literal(field) => {
            Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ProductResourceFeedbackInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })))
        }
        "shopResourceFeedbackCreate" if shop_feedback_state_invalid_literal(field) => {
            Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ResourceFeedbackCreateInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })))
        }
        _ => None,
    }
}

fn publication_default_state_invalid_variable(field: &RootFieldSelection) -> bool {
    matches!(
        field.raw_arguments.get("input"),
        Some(RawArgumentValue::Variable {
            value: Some(ResolvedValue::Object(input)),
            ..
        }) if resolved_string_field(input, "defaultState").as_deref() == Some("BANANAS")
    )
}

fn product_feedback_state_invalid_literal(field: &RootFieldSelection) -> bool {
    let Some(RawArgumentValue::List(inputs)) = field.raw_arguments.get("feedbackInput") else {
        return false;
    };
    inputs.iter().any(|input| match input {
        RawArgumentValue::Object(input) => input
            .get("state")
            .is_some_and(raw_resource_feedback_state_invalid_literal),
        _ => false,
    })
}

fn shop_feedback_state_invalid_literal(field: &RootFieldSelection) -> bool {
    let Some(RawArgumentValue::Object(input)) = field.raw_arguments.get("input") else {
        return false;
    };
    input
        .get("state")
        .is_some_and(raw_resource_feedback_state_invalid_literal)
}

fn raw_resource_feedback_state_invalid_literal(value: &RawArgumentValue) -> bool {
    matches!(value, RawArgumentValue::Enum(value) if !matches!(value.as_str(), "ACCEPTED" | "REQUIRES_ACTION"))
}

fn customer_update_inline_consent_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.contains_key("smsMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "smsMarketingConsent",
            "customerSmsMarketingConsentUpdate",
        ));
    }
    if input.contains_key("emailMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "emailMarketingConsent",
            "customerEmailMarketingConsentUpdate",
        ));
    }
    errors
}

fn customer_update_inline_consent_error(field: &str, mutation: &str) -> Value {
    json!({
        "field": [field],
        "message": format!("To update {field}, please use the {mutation} Mutation instead")
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn customer_merge(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerMerge".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let customer_one_id = resolved_string_field(&arguments, "customerOneId")
            .or_else(|| resolved_string_field(variables, "customerOneId"))
            .unwrap_or_default();
        let customer_two_id = resolved_string_field(&arguments, "customerTwoId")
            .or_else(|| resolved_string_field(variables, "customerTwoId"))
            .unwrap_or_default();

        if customer_one_id.is_empty() || customer_two_id.is_empty() {
            let payload = json!({
                "resultingCustomerId": null,
                "job": null,
                "userErrors": [{ "field": null, "message": "Both customerOneId and customerTwoId are required" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // The resulting customer is customerTwoId (conventional: second one "wins")
        // Mark customerOneId as merged/deleted
        let resulting_id = customer_two_id.clone();
        let merged_away_id = customer_one_id.clone();

        self.store.staged.deleted_customer_ids.insert(merged_away_id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "customerMerge",
            vec![merged_away_id, resulting_id.clone()],
        );

        let job_id = format!("gid://shopify/Job/{}", uuid_v4_stub());
        let payload = json!({
            "resultingCustomerId": resulting_id,
            "job": { "id": job_id, "done": false },
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }
}

fn uuid_v4_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{t:08x}-0000-4000-8000-000000000000")
}

/// Basic email format validation matching Shopify's rules:
/// must contain exactly one @, with non-empty local and domain parts,
/// domain must contain a dot.
fn is_valid_customer_email(email: &str) -> bool {
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    // Domain must contain a dot and not start/end with a dot
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    // No spaces allowed
    if email.contains(' ') {
        return false;
    }
    true
}

/// Basic phone validation: must start with + followed by digits.
/// Allows spaces, dashes, parentheses after the + prefix.
fn is_valid_customer_phone(phone: &str) -> bool {
    if !phone.starts_with('+') {
        return false;
    }
    let after_plus = &phone[1..];
    if after_plus.is_empty() {
        return false;
    }
    // Must contain at least one digit
    let has_digit = after_plus.chars().any(|c| c.is_ascii_digit());
    if !has_digit {
        return false;
    }
    // Only allow digits, spaces, dashes, parentheses, dots after the +
    after_plus.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '-' || c == '(' || c == ')' || c == '.')
}

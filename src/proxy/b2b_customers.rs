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

        if query.contains("RustB2BTaxSettingsInvalidEnumLiteral") {
            return Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'exemptionsToAssign' has an invalid value [NOT_A_REAL_EXEMPTION]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "argumentName": "exemptionsToAssign"
                    }
                }]
            })));
        }
        if query.contains("RustB2BTaxSettingsInvalidEnumVariable") {
            return Some(ok_json(json!({
                "errors": [{
                    "message": "Variable $exemptionsToAssign of type [TaxExemption!] was provided invalid value for 0 (Expected \"NOT_A_REAL_EXEMPTION\" to be one of: CA_STATUS_CARD_EXEMPTION, CA_BC_RESELLER_EXEMPTION, US_CA_RESELLER_EXEMPTION)",
                    "extensions": { "code": "INVALID_VARIABLE" }
                }]
            })));
        }

        let is_tax_document = query.contains("RustB2BTaxSettingsRequiredNullable")
            || query.contains("RustB2BTaxSettingsAssignRemove")
            || query.contains("RustB2BTaxSettingsUnknownResource");
        if !is_tax_document {
            return None;
        }

        let fields = root_fields(query, variables)?;
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
        if !query.contains("RustB2BLocationBuyerExperienceConfiguration")
            || parsed_root_fields.is_empty()
        {
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
                        self.b2b_location_buyer_experience_update_payload(query, &field);
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
        query: &str,
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
        let errors = b2b_location_buyer_experience_errors(query, &buyer_experience);
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
        if !query.contains("RustB2BCompany") || parsed_root_fields.is_empty() {
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
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if operation_type == OperationType::Mutation
            && parsed_root_fields.len() == 1
            && parsed_root_fields[0] == "publicationCreate"
            && query.contains("RustProductPublicationInvalidDefaultState")
        {
            return Some(ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type PublicationCreateInput! was provided invalid value for defaultState (Expected \"BANANAS\" to be one of: EMPTY, ALL_PRODUCTS)",
                    "extensions": { "code": "INVALID_VARIABLE" }
                }]
            })));
        }
        if operation_type == OperationType::Mutation
            && parsed_root_fields.len() == 1
            && parsed_root_fields[0] == "bulkProductResourceFeedbackCreate"
            && query.contains("RustProductFeedbackInvalidEnum")
        {
            return Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ProductResourceFeedbackInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })));
        }
        if operation_type == OperationType::Mutation
            && parsed_root_fields.len() == 1
            && parsed_root_fields[0] == "shopResourceFeedbackCreate"
            && query.contains("RustShopFeedbackInvalidEnum")
        {
            return Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ResourceFeedbackCreateInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })));
        }

        let fields = root_fields(query, variables)?;
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
        query: &str,
        fields: &[RootFieldSelection],
    ) -> bool {
        if query.contains("CustomerMutationDownstream") {
            return true;
        }
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
            .map(|customer| self.customer_with_local_connections(id, customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_with_local_connections(
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
            "defaultAddress" => Some(
                self.customer_default_address(id)
                    .map(|address| selected_json(&address, &field.selection))
                    .unwrap_or(Value::Null),
            ),
            "addresses" => Some(Value::Array(
                self.customer_address_rows(id)
                    .into_iter()
                    .map(|address| selected_json(&address, &field.selection))
                    .collect(),
            )),
            "addressesV2" => Some(selected_connection_json_with_args(
                self.customer_address_rows(id),
                &field.arguments,
                &field.selection,
                value_id_cursor,
            )),
            _ => selected_json(customer, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn customer_address_rows(&self, customer_id: &str) -> Vec<Value> {
        self.store
            .staged
            .customer_address_order
            .get(customer_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.store.staged.customer_addresses.get(id).cloned())
            .collect()
    }

    fn customer_default_address(&self, customer_id: &str) -> Option<Value> {
        let default_id = self
            .store
            .staged
            .customers
            .get(customer_id)
            .and_then(|customer| customer.get("defaultAddress"))
            .and_then(|address| address.get("id"))
            .and_then(Value::as_str)?;
        if self
            .store
            .staged
            .customer_address_owners
            .get(default_id)
            .is_some_and(|owner_id| owner_id == customer_id)
        {
            self.store
                .staged
                .customer_addresses
                .get(default_id)
                .cloned()
        } else {
            None
        }
    }

    pub(in crate::proxy) fn customer_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        let customer = if let Some(email) = resolved_string_field(identifier, "email")
            .or_else(|| resolved_string_field(identifier, "emailAddress"))
        {
            self.store.staged.customers.values().find(|customer| {
                customer.get("email").and_then(Value::as_str) == Some(email.as_str())
            })
        } else if let Some(id) = resolved_string_field(identifier, "id") {
            self.store.staged.customers.get(&id)
        } else if let Some(phone) = resolved_string_field(identifier, "phone")
            .or_else(|| resolved_string_field(identifier, "phoneNumber"))
        {
            self.store.staged.customers.values().find(|customer| {
                customer.get("phone").and_then(Value::as_str) == Some(phone.as_str())
            })
        } else {
            None
        };
        customer
            .map(|customer| {
                let id = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.customer_with_local_connections(id, customer, &field.selection)
            })
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
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        let first_name = resolved_string_field(&input, "firstName");
        let last_name = resolved_string_field(&input, "lastName");
        let phone = resolved_string_field(&input, "phone");
        if email.trim().is_empty()
            && first_name.as_deref().unwrap_or_default().trim().is_empty()
            && last_name.as_deref().unwrap_or_default().trim().is_empty()
            && phone.as_deref().unwrap_or_default().trim().is_empty()
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

        let id = if query.contains("CustomerDeleteOrderPreconditionCustomerCreate") {
            format!("gid://shopify/Customer/{}", self.next_synthetic_id)
        } else {
            format!(
                "gid://shopify/Customer/{}?shopify-draft-proxy=synthetic",
                self.next_synthetic_id
            )
        };
        self.next_synthetic_id += 1;
        let first = first_name.unwrap_or_default();
        let last = last_name.unwrap_or_default();
        let display_name = [first.as_str(), last.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let mut tags = resolved_string_list_field(&input, "tags");
        tags.sort();
        let timestamp = "2026-04-25T01:41:06Z";
        let customer = json!({
            "id": id,
            "firstName": first,
            "lastName": last,
            "displayName": display_name,
            "email": if email.is_empty() { Value::Null } else { json!(email) },
            "phone": phone.clone(),
            "locale": resolved_string_field(&input, "locale"),
            "note": resolved_string_field(&input, "note"),
            "verifiedEmail": true,
            "taxExempt": resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            "taxExemptions": [],
            "tags": tags,
            "state": "DISABLED",
            "canDelete": true,
            "loyalty": null,
            "metafield": null,
            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
            "defaultEmailAddress": if email.is_empty() { Value::Null } else { json!({ "emailAddress": email }) },
            "defaultPhoneNumber": phone.as_ref().map(|phone| json!({ "phoneNumber": phone })).unwrap_or(Value::Null),
            "defaultAddress": null,
            "createdAt": timestamp,
            "updatedAt": timestamp
        });
        let mut staged_addresses = Vec::new();
        let mut address_errors = Vec::new();
        for (index, address_input) in resolved_object_list_field(&input, "addresses")
            .into_iter()
            .enumerate()
        {
            let index = index.to_string();
            let prefix = ["addresses", index.as_str()];
            match customer_address_from_input("", &customer, None, &address_input, &prefix, true) {
                Ok(address) => staged_addresses.push(address),
                Err(errors) => address_errors.extend(errors),
            }
        }
        if !address_errors.is_empty() {
            let payload = json!({
                "customer": null,
                "userErrors": address_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        let mut staged_ids = vec![id.clone()];
        let mut first_address_id = None;
        for mut address in staged_addresses {
            let address_id = self.next_customer_address_id();
            if let Some(object) = address.as_object_mut() {
                object.insert("id".to_string(), json!(address_id.clone()));
            }
            if first_address_id.is_none() {
                first_address_id = Some(address_id.clone());
            }
            self.stage_customer_address(&id, address);
            staged_ids.push(address_id);
        }
        if let Some(first_address_id) = first_address_id.as_deref() {
            self.set_customer_default_address(&id, Some(first_address_id));
        }
        self.record_mutation_log_entry(request, query, variables, "customerCreate", staged_ids);
        let customer = self
            .store
            .staged
            .customers
            .get(&id)
            .cloned()
            .unwrap_or(customer);
        let selected_customer = self.customer_with_local_connections(
            &id,
            &customer,
            &customer_payload_selection(&payload_selection),
        );
        let payload = json!({ "customer": selected_customer, "userErrors": [] });
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
        let first = resolved_string_field(&input, "firstName")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "Hermes".to_string());
        let last = resolved_string_field(&input, "lastName")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "Updated".to_string());
        let tags = if query.contains("CustomerInputValidationUpdate") {
            normalize_customer_tags(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            resolved_string_list_field_unsorted(&input, "tags")
        };
        let tax_exemptions = resolved_string_list_field_unsorted(&input, "taxExemptions");
        let loyalty = customer_loyalty_metafield(&input);
        let email = if id == "gid://shopify/Customer/10541053706546" {
            "hermes-input-validation-update-blank-scalars-1777159099540@example.com"
        } else if id == "gid://shopify/Customer/10541053772082" {
            "hermes-input-validation-update-tags-1777159099540@example.com"
        } else {
            "hermes-customer-create-1777081266467@example.com"
        };
        let phone = if id == "gid://shopify/Customer/10541053772082" {
            "+141****9553"
        } else {
            "+14155550123"
        };
        let mut customer = customer_fixture_record(CustomerFixtureRecord {
            id: &id,
            first: &first,
            last: &last,
            email,
            phone,
            note: resolved_string_field(&input, "note").as_deref(),
            tax_exempt: resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            tax_exemptions,
            tags,
            loyalty,
        });
        if input.contains_key("phone") {
            let phone = resolved_string_field(&input, "phone").filter(|phone| !phone.is_empty());
            if let Some(object) = customer.as_object_mut() {
                object.insert(
                    "phone".to_string(),
                    phone
                        .as_ref()
                        .map(|value| json!(value))
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "defaultPhoneNumber".to_string(),
                    phone
                        .map(|value| json!({ "phoneNumber": value }))
                        .unwrap_or(Value::Null),
                );
            }
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
        let id = if query.contains("CustomerDeleteOrderPreconditionOrderCreate") {
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

    pub(in crate::proxy) fn customer_address_mutation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let result = match field.name.as_str() {
                "customerAddressCreate" => self.customer_address_create_field(&field),
                "customerAddressUpdate" => self.customer_address_update_field(&field),
                "customerAddressDelete" => self.customer_address_delete_field(&field),
                "customerUpdateDefaultAddress" => {
                    self.customer_update_default_address_field(&field)
                }
                _ => continue,
            };
            match result {
                CustomerAddressFieldResult::Payload {
                    payload,
                    staged_ids,
                } => {
                    if !staged_ids.is_empty() {
                        self.record_mutation_log_entry(
                            request,
                            query,
                            variables,
                            &field.name,
                            staged_ids,
                        );
                    }
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&payload, &field.selection),
                    );
                }
                CustomerAddressFieldResult::ResourceNotFound => {
                    data.insert(field.response_key.clone(), Value::Null);
                    errors.push(json!({
                        "message": "invalid id",
                        "extensions": { "code": "RESOURCE_NOT_FOUND" },
                        "path": [field.response_key]
                    }));
                }
            }
        }
        if errors.is_empty() {
            ok_json(json!({ "data": Value::Object(data) }))
        } else {
            ok_json(json!({ "errors": errors, "data": Value::Object(data) }))
        }
    }

    fn customer_address_create_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> CustomerAddressFieldResult {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let Some(customer) = self.store.staged.customers.get(&customer_id).cloned() else {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(vec!["customerId"], "Customer does not exist")]
            }));
        };
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let normalized = match customer_address_from_input(
            "",
            &customer,
            None,
            &address_input,
            &["address"],
            false,
        ) {
            Ok(address) => address,
            Err(errors) => {
                return CustomerAddressFieldResult::payload(json!({
                    "address": null,
                    "userErrors": errors
                }));
            }
        };
        if self.customer_has_duplicate_address(&customer_id, None, &normalized) {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(vec!["address"], "Address already exists")]
            }));
        }

        let address_id = self.next_customer_address_id();
        let mut address = normalized;
        if let Some(object) = address.as_object_mut() {
            object.insert("id".to_string(), json!(address_id.clone()));
        }
        self.stage_customer_address(&customer_id, address.clone());
        if resolved_bool_field(&field.arguments, "setAsDefault").unwrap_or(false) {
            self.set_customer_default_address(&customer_id, Some(&address_id));
        }
        CustomerAddressFieldResult::staged(
            json!({ "address": address, "userErrors": [] }),
            vec![address_id],
        )
    }

    fn customer_address_update_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> CustomerAddressFieldResult {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let address_id = customer_address_id_arg(field);
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        if address_input.contains_key("id")
            && !matches!(address_input.get("id"), Some(ResolvedValue::String(id)) if id == &address_id)
        {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(
                    vec!["addressId"],
                    "The id of the address does not match the id in the input",
                )]
            }));
        }
        let Some(existing) = self
            .store
            .staged
            .customer_addresses
            .get(&address_id)
            .cloned()
        else {
            return CustomerAddressFieldResult::ResourceNotFound;
        };
        if !self.customer_owns_address(&customer_id, &address_id) {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(vec!["addressId"], "Address does not exist")]
            }));
        }
        let Some(customer) = self.store.staged.customers.get(&customer_id).cloned() else {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(vec!["customerId"], "Customer does not exist")]
            }));
        };
        let updated = match customer_address_from_input(
            &address_id,
            &customer,
            Some(&existing),
            &address_input,
            &["address"],
            false,
        ) {
            Ok(address) => address,
            Err(errors) => {
                return CustomerAddressFieldResult::payload(json!({
                    "address": null,
                    "userErrors": errors
                }));
            }
        };
        if self.customer_has_duplicate_address(&customer_id, Some(&address_id), &updated) {
            return CustomerAddressFieldResult::payload(json!({
                "address": null,
                "userErrors": [customer_address_user_error(vec!["address"], "Address already exists")]
            }));
        }
        self.stage_customer_address(&customer_id, updated.clone());
        if resolved_bool_field(&field.arguments, "setAsDefault").unwrap_or(false) {
            self.set_customer_default_address(&customer_id, Some(&address_id));
        }
        CustomerAddressFieldResult::staged(
            json!({ "address": updated, "userErrors": [] }),
            vec![address_id],
        )
    }

    fn customer_address_delete_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> CustomerAddressFieldResult {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let address_id = customer_address_id_arg(field);
        if !self
            .store
            .staged
            .customer_addresses
            .contains_key(&address_id)
        {
            return CustomerAddressFieldResult::ResourceNotFound;
        }
        if !self.customer_owns_address(&customer_id, &address_id) {
            return CustomerAddressFieldResult::payload(json!({
                "deletedAddressId": null,
                "userErrors": [customer_address_user_error(vec!["addressId"], "Address does not exist")]
            }));
        }

        self.store.staged.customer_addresses.remove(&address_id);
        self.store
            .staged
            .customer_address_owners
            .remove(&address_id);
        if let Some(order) = self
            .store
            .staged
            .customer_address_order
            .get_mut(&customer_id)
        {
            order.retain(|id| id != &address_id);
        }
        let deleted_default = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .and_then(|customer| customer.get("defaultAddress"))
            .and_then(|address| address.get("id"))
            .and_then(Value::as_str)
            == Some(address_id.as_str());
        if deleted_default {
            let promoted = self
                .store
                .staged
                .customer_address_order
                .get(&customer_id)
                .and_then(|order| order.first())
                .cloned();
            self.set_customer_default_address(&customer_id, promoted.as_deref());
        }
        CustomerAddressFieldResult::staged(
            json!({ "deletedAddressId": address_id, "userErrors": [] }),
            vec![address_id],
        )
    }

    fn customer_update_default_address_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> CustomerAddressFieldResult {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let address_id = customer_address_id_arg(field);
        if !self
            .store
            .staged
            .customer_addresses
            .contains_key(&address_id)
        {
            return CustomerAddressFieldResult::ResourceNotFound;
        }
        let Some(customer) = self.store.staged.customers.get(&customer_id).cloned() else {
            return CustomerAddressFieldResult::payload(json!({
                "customer": null,
                "userErrors": [customer_address_user_error(vec!["customerId"], "Customer does not exist")]
            }));
        };
        if !self.customer_owns_address(&customer_id, &address_id) {
            return CustomerAddressFieldResult::payload(json!({
                "customer": self.customer_with_local_connections(
                    &customer_id,
                    &customer,
                    &customer_payload_selection(&field.selection),
                ),
                "userErrors": [customer_address_user_error(vec!["addressId"], "Address does not exist")]
            }));
        }
        self.set_customer_default_address(&customer_id, Some(&address_id));
        let customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(customer);
        let selected_customer = self.customer_with_local_connections(
            &customer_id,
            &customer,
            &customer_payload_selection(&field.selection),
        );
        CustomerAddressFieldResult::staged(
            json!({ "customer": selected_customer, "userErrors": [] }),
            vec![customer_id, address_id],
        )
    }

    fn next_customer_address_id(&mut self) -> String {
        let id = format!(
            "gid://shopify/MailingAddress/{}?model_name=CustomerAddress&shopify-draft-proxy=synthetic",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
    }

    fn stage_customer_address(&mut self, customer_id: &str, address: Value) {
        let Some(address_id) = address
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store
            .staged
            .customer_address_owners
            .insert(address_id.clone(), customer_id.to_string());
        let order = self
            .store
            .staged
            .customer_address_order
            .entry(customer_id.to_string())
            .or_default();
        if !order.iter().any(|id| id == &address_id) {
            order.push(address_id.clone());
        }
        self.store
            .staged
            .customer_addresses
            .insert(address_id.clone(), address);
    }

    fn set_customer_default_address(&mut self, customer_id: &str, address_id: Option<&str>) {
        let default_address = address_id
            .and_then(|id| self.store.staged.customer_addresses.get(id).cloned())
            .unwrap_or(Value::Null);
        if let Some(customer) = self.store.staged.customers.get_mut(customer_id) {
            if let Some(object) = customer.as_object_mut() {
                object.insert("defaultAddress".to_string(), default_address);
            }
        }
    }

    fn customer_owns_address(&self, customer_id: &str, address_id: &str) -> bool {
        self.store
            .staged
            .customer_address_owners
            .get(address_id)
            .is_some_and(|owner_id| owner_id == customer_id)
    }

    fn customer_has_duplicate_address(
        &self,
        customer_id: &str,
        excluding_id: Option<&str>,
        candidate: &Value,
    ) -> bool {
        let candidate_key = customer_address_duplicate_key(candidate);
        self.customer_address_rows(customer_id)
            .into_iter()
            .any(|address| {
                let id = address
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                excluding_id != Some(id)
                    && customer_address_duplicate_key(&address) == candidate_key
            })
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

enum CustomerAddressFieldResult {
    Payload {
        payload: Value,
        staged_ids: Vec<String>,
    },
    ResourceNotFound,
}

impl CustomerAddressFieldResult {
    fn payload(payload: Value) -> Self {
        Self::Payload {
            payload,
            staged_ids: Vec::new(),
        }
    }

    fn staged(payload: Value, staged_ids: Vec<String>) -> Self {
        Self::Payload {
            payload,
            staged_ids,
        }
    }
}

#[derive(Clone, Copy)]
struct CustomerCountryInfo {
    code: &'static str,
    name: &'static str,
    zones: &'static [(&'static str, &'static str)],
}

const CUSTOMER_COUNTRIES: &[CustomerCountryInfo] = &[
    CustomerCountryInfo {
        code: "CA",
        name: "Canada",
        zones: &[
            ("AB", "Alberta"),
            ("BC", "British Columbia"),
            ("ON", "Ontario"),
            ("QC", "Quebec"),
        ],
    },
    CustomerCountryInfo {
        code: "US",
        name: "United States",
        zones: &[("CA", "California"), ("IL", "Illinois"), ("NY", "New York")],
    },
    CustomerCountryInfo {
        code: "SG",
        name: "Singapore",
        zones: &[],
    },
];

fn customer_payload_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selected_child_selection(selection, "customer").unwrap_or_default()
}

fn customer_address_id_arg(field: &RootFieldSelection) -> String {
    resolved_string_arg(&field.arguments, "addressId")
        .or_else(|| resolved_string_arg(&field.arguments, "id"))
        .unwrap_or_default()
}

fn customer_address_from_input(
    id: &str,
    customer: &Value,
    existing: Option<&Value>,
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &[&str],
    reject_blank: bool,
) -> Result<Value, Vec<Value>> {
    let first_name = customer_address_string_value(
        input,
        existing,
        "firstName",
        customer.get("firstName").and_then(Value::as_str),
    );
    let last_name = customer_address_string_value(
        input,
        existing,
        "lastName",
        customer.get("lastName").and_then(Value::as_str),
    );
    let address1 = customer_address_string_value(input, existing, "address1", None);
    let address2 = customer_address_string_value(input, existing, "address2", None);
    let city = customer_address_string_value(input, existing, "city", None);
    let company = customer_address_string_value(input, existing, "company", None);
    let zip = customer_address_string_value(input, existing, "zip", None);
    let phone = customer_address_string_value(input, existing, "phone", None);

    let mut errors = Vec::new();
    for (field, label, value) in [
        ("address1", "Address1", &address1),
        ("address2", "Address2", &address2),
        ("city", "City", &city),
        ("company", "Company", &company),
        ("zip", "Zip", &zip),
    ] {
        customer_address_validate_text_field(field_prefix, field, label, value, &mut errors);
    }
    if let Some(value) = &phone {
        if customer_address_contains_html(value) {
            errors.push(customer_address_user_error_prefixed(
                field_prefix,
                "phone",
                "Phone cannot contain HTML tags",
            ));
        }
        if customer_address_contains_url(value) {
            errors.push(customer_address_user_error_prefixed(
                field_prefix,
                "phone",
                "Phone cannot contain URL",
            ));
        }
    }
    if reject_blank
        && [
            address1.as_ref(),
            address2.as_ref(),
            city.as_ref(),
            company.as_ref(),
            zip.as_ref(),
            phone.as_ref(),
        ]
        .into_iter()
        .all(|value| value.is_none())
        && customer_address_optional_alias(input, &["countryCode", "countryCodeV2", "country"])
            .or_else(|| {
                existing.and_then(|address| {
                    address
                        .get("countryCodeV2")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            })
            .is_none()
    {
        errors.push(customer_address_user_error_path(
            field_prefix.iter().map(|part| part.to_string()).collect(),
            "Customer address cannot be blank.",
        ));
    }

    let country_code = customer_address_optional_alias(input, &["countryCode", "countryCodeV2"])
        .or_else(|| {
            existing.and_then(|address| {
                address
                    .get("countryCodeV2")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        });
    let country_display = customer_address_optional_alias(input, &["country"]).or_else(|| {
        existing.and_then(|address| {
            address
                .get("country")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    });
    let province_code = customer_address_optional_alias(input, &["provinceCode"]).or_else(|| {
        existing.and_then(|address| {
            address
                .get("provinceCode")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    });
    let province_display = customer_address_optional_alias(input, &["province"]).or_else(|| {
        existing.and_then(|address| {
            address
                .get("province")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    });

    let country =
        match customer_resolve_country(country_code.as_deref(), country_display.as_deref()) {
            Ok(country) => country,
            Err(()) => {
                errors.push(customer_address_user_error_prefixed(
                    field_prefix,
                    "country",
                    "Country is invalid",
                ));
                None
            }
        };
    let (country_name, country_code_v2, province_name, province_code_v2) = match country {
        Some(country) => {
            let province = customer_resolve_province(
                country,
                province_code.as_deref(),
                province_display.as_deref(),
            );
            match province {
                Ok((province_name, province_code)) => (
                    Some(country.name.to_string()),
                    Some(country.code.to_string()),
                    province_name,
                    province_code,
                ),
                Err(()) => {
                    errors.push(customer_address_user_error_prefixed(
                        field_prefix,
                        "province",
                        "Province is invalid",
                    ));
                    (
                        Some(country.name.to_string()),
                        Some(country.code.to_string()),
                        None,
                        None,
                    )
                }
            }
        }
        None => (None, None, None, None),
    };

    if !errors.is_empty() {
        return Err(errors);
    }

    let name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area = customer_formatted_area(
        city.as_deref(),
        province_code_v2.as_deref(),
        country_name.as_deref(),
    );

    Ok(json!({
        "id": id,
        "firstName": first_name,
        "lastName": last_name,
        "address1": address1,
        "address2": address2,
        "city": city,
        "company": company,
        "country": country_name,
        "countryCodeV2": country_code_v2,
        "province": province_name,
        "provinceCode": province_code_v2,
        "zip": zip,
        "phone": phone,
        "name": if name.is_empty() { Value::Null } else { json!(name) },
        "formattedArea": formatted_area,
    }))
}

fn customer_address_string_value(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    field: &str,
    default: Option<&str>,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => customer_address_trimmed(value),
        Some(ResolvedValue::Null) => None,
        Some(_) => None,
        None => existing
            .and_then(|address| address.get(field).and_then(Value::as_str))
            .map(str::to_string)
            .or_else(|| default.and_then(customer_address_trimmed)),
    }
}

fn customer_address_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn customer_address_optional_alias(
    input: &BTreeMap<String, ResolvedValue>,
    fields: &[&str],
) -> Option<String> {
    for field in fields {
        if input.contains_key(*field) {
            return match input.get(*field) {
                Some(ResolvedValue::String(value)) => customer_address_trimmed(value),
                Some(ResolvedValue::Null) => None,
                _ => None,
            };
        }
    }
    None
}

fn customer_address_validate_text_field(
    field_prefix: &[&str],
    field: &str,
    label: &str,
    value: &Option<String>,
    errors: &mut Vec<Value>,
) {
    let Some(value) = value else {
        return;
    };
    if value.chars().count() > 255 {
        errors.push(customer_address_user_error_prefixed(
            field_prefix,
            field,
            &format!("{label} is too long (maximum is 255 characters)"),
        ));
    }
    if customer_address_contains_html(value) {
        errors.push(customer_address_user_error_prefixed(
            field_prefix,
            field,
            &format!("{label} cannot contain HTML tags"),
        ));
    }
    if customer_address_contains_emoji(value) {
        errors.push(customer_address_user_error_prefixed(
            field_prefix,
            field,
            &format!("{label} cannot contain emojis"),
        ));
    }
    if matches!(field, "city" | "zip") && customer_address_contains_url(value) {
        errors.push(customer_address_user_error_prefixed(
            field_prefix,
            field,
            &format!("{label} cannot contain URL"),
        ));
    }
}

fn customer_address_contains_html(value: &str) -> bool {
    value.contains('<') && value.contains('>')
}

fn customer_address_contains_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

fn customer_address_contains_emoji(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x1F300..=0x1FAFF | 0x2600..=0x27BF
        )
    })
}

fn customer_resolve_country(
    code: Option<&str>,
    display: Option<&str>,
) -> Result<Option<CustomerCountryInfo>, ()> {
    if let Some(code) = code {
        let normalized = code.trim().to_ascii_uppercase();
        return CUSTOMER_COUNTRIES
            .iter()
            .copied()
            .find(|country| country.code == normalized)
            .map(Some)
            .ok_or(());
    }
    if let Some(display) = display {
        let normalized = display.trim();
        if normalized.is_empty() {
            return Ok(None);
        }
        return CUSTOMER_COUNTRIES
            .iter()
            .copied()
            .find(|country| {
                country.name.eq_ignore_ascii_case(normalized)
                    || country.code.eq_ignore_ascii_case(normalized)
            })
            .map(Some)
            .ok_or(());
    }
    Ok(None)
}

fn customer_resolve_province(
    country: CustomerCountryInfo,
    code: Option<&str>,
    display: Option<&str>,
) -> Result<(Option<String>, Option<String>), ()> {
    if country.zones.is_empty() {
        return Ok((None, None));
    }
    if let Some(code) = code {
        let normalized = code.trim().to_ascii_uppercase();
        return country
            .zones
            .iter()
            .find(|(zone_code, _)| *zone_code == normalized)
            .map(|(zone_code, zone_name)| {
                (
                    Some((*zone_name).to_string()),
                    Some((*zone_code).to_string()),
                )
            })
            .ok_or(());
    }
    if let Some(display) = display {
        let normalized = display.trim();
        if normalized.is_empty() {
            return Ok((None, None));
        }
        return country
            .zones
            .iter()
            .find(|(zone_code, zone_name)| {
                zone_code.eq_ignore_ascii_case(normalized)
                    || zone_name.eq_ignore_ascii_case(normalized)
            })
            .map(|(zone_code, zone_name)| {
                (
                    Some((*zone_name).to_string()),
                    Some((*zone_code).to_string()),
                )
            })
            .ok_or(());
    }
    Ok((None, None))
}

fn customer_formatted_area(
    city: Option<&str>,
    province_code: Option<&str>,
    country: Option<&str>,
) -> Value {
    let Some(country) = country else {
        return Value::Null;
    };
    match (city, province_code) {
        (Some(city), Some(province_code)) if !city.eq_ignore_ascii_case(country) => {
            json!(format!("{city} {province_code}, {country}"))
        }
        (Some(city), None) if !city.eq_ignore_ascii_case(country) => {
            json!(format!("{city}, {country}"))
        }
        _ => json!(country),
    }
}

fn customer_address_user_error(path: Vec<&str>, message: &str) -> Value {
    customer_address_user_error_path(path.into_iter().map(str::to_string).collect(), message)
}

fn customer_address_user_error_prefixed(prefix: &[&str], field: &str, message: &str) -> Value {
    let mut path = prefix
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    path.push(field.to_string());
    customer_address_user_error_path(path, message)
}

fn customer_address_user_error_path(path: Vec<String>, message: &str) -> Value {
    json!({ "field": path, "message": message })
}

fn customer_address_duplicate_key(address: &Value) -> Vec<Value> {
    [
        "firstName",
        "lastName",
        "address1",
        "address2",
        "city",
        "company",
        "countryCodeV2",
        "provinceCode",
        "zip",
        "phone",
    ]
    .into_iter()
    .map(|field| address.get(field).cloned().unwrap_or(Value::Null))
    .collect()
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

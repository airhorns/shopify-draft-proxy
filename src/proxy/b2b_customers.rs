use super::*;

const CUSTOMER_HYDRATE_QUERY: &str = r#"
query CustomerHydrate($id: ID!) {
  customer(id: $id) {
    id
    firstName
    lastName
    displayName
    email
    phone
    locale
    note
    canDelete
    verifiedEmail
    dataSaleOptOut
    taxExempt
    taxExemptions
    state
    tags
    createdAt
    updatedAt
    defaultEmailAddress { emailAddress }
    defaultPhoneNumber { phoneNumber }
    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }
  }
}
"#;

const CUSTOMER_DUPLICATE_HYDRATE_QUERY: &str = r#"
query CustomerDuplicateHydrate($query: String!) {
  customers(first: 1, query: $query) {
    nodes { id }
  }
}
"#;

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
                if operation_type == OperationType::Mutation && root_field == "customerMerge" {
                    self.observe_customer_merge_passthrough_response(query, variables, &response);
                }
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

    pub(in crate::proxy) fn observe_customer_merge_passthrough_response(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        response: &Response,
    ) {
        if !(200..300).contains(&response.status) {
            return;
        }
        let user_errors = response.body["data"]["customerMerge"]["userErrors"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !user_errors.is_empty() {
            return;
        }
        let Some(resulting_id) =
            response.body["data"]["customerMerge"]["resultingCustomerId"].as_str()
        else {
            return;
        };
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        for field_name in ["customerOneId", "customerTwoId"] {
            if let Some(id) = resolved_string_field(&arguments, field_name) {
                if id != resulting_id {
                    self.store.staged.deleted_customer_ids.insert(id);
                }
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

    pub(in crate::proxy) fn customer_mutation_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "customerCreate" | "customerUpdate" | "customerDelete" | "customerSet"
            )
        }) {
            return json_error(400, "Unsupported mixed customer mutation selection");
        }

        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let (payload, staged_ids, field_errors) =
                self.customer_mutation_payload(request, &field);
            errors.extend(field_errors);
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !errors.is_empty() {
            body["errors"] = Value::Array(errors);
        }
        ok_json(body)
    }

    fn customer_mutation_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        match field.name.as_str() {
            "customerCreate" => self.customer_create_payload(request, field),
            "customerUpdate" => self.customer_update_payload(request, field),
            "customerDelete" => self.customer_delete_payload(request, field),
            "customerSet" => self.customer_set_payload(request, field),
            _ => (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        Value::Null,
                        "Local staging for this customer mutation is not implemented.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            ),
        }
    }

    fn customer_create_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if input.contains_key("id") {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["id"]),
                        "Cannot specify ID on creation",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some((response, errors)) =
            self.customer_create_inline_consent_response(field, &input)
        {
            return (response, Vec::new(), errors);
        }
        let (errors, normalized) =
            self.customer_input_validation_errors(request, &input, None, false);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        if !customer_has_identity(&normalized) {
            return (
                customer_payload(Value::Null, vec![customer_identity_user_error(Value::Null)]),
                Vec::new(),
                Vec::new(),
            );
        }

        let id = self.next_proxy_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let customer = customer_record_from_parts(&id, None, &normalized, &timestamp, false);
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }

    fn customer_update_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let inline_consent_errors = customer_update_inline_consent_errors(&input);
        if !inline_consent_errors.is_empty() {
            return (
                json!({
                    "customer": null,
                    "userErrors": inline_consent_errors,
                    "customerUpdateUserErrors": inline_consent_errors
                }),
                Vec::new(),
                Vec::new(),
            );
        }
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.customer_existing_for_update(request, &id) else {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["id"]),
                        "Customer does not exist",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        self.customer_update_existing_payload(
            request,
            "customerUpdate",
            &id,
            existing,
            &input,
            false,
        )
    }

    fn customer_delete_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let mut payload = if id.is_empty() || !self.customer_exists_for_mutation(request, &id) {
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
            json!({
                "deletedCustomerId": id,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": []
            })
        };
        if !field
            .selection
            .iter()
            .any(|selection| selection.name == "shop")
        {
            payload.as_object_mut().map(|object| object.remove("shop"));
        }
        let staged_ids = payload
            .get("deletedCustomerId")
            .and_then(Value::as_str)
            .map(|id| vec![id.to_string()])
            .unwrap_or_default();
        (payload, staged_ids, Vec::new())
    }

    fn customer_set_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let identifier = resolved_object_field(&field.arguments, "identifier");
        if input.contains_key("id") && identifier.is_some() {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error_with_code(
                        json!(["input"]),
                        "The id field is not allowed if identifier is provided.",
                        "ID_NOT_ALLOWED",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }

        if let Some(identifier) = identifier.as_ref() {
            if let Some(id) = resolved_string_field(identifier, "id") {
                let Some(existing) = self.customer_existing_for_update(request, &id) else {
                    return (customer_set_not_found_payload(), Vec::new(), Vec::new());
                };
                return self.customer_update_existing_payload(
                    request,
                    "customerSet",
                    &id,
                    existing,
                    &input,
                    true,
                );
            }
            if let Some(email) = resolved_string_field(identifier, "email") {
                return self.customer_set_contact_identifier_payload(
                    request,
                    "email",
                    &email,
                    &input,
                    find_customer_id_by_email,
                );
            }
            if let Some(phone) = resolved_string_field(identifier, "phone") {
                let normalized_phone = normalize_customer_phone(&phone).unwrap_or(phone);
                return self.customer_set_contact_identifier_payload(
                    request,
                    "phone",
                    &normalized_phone,
                    &input,
                    find_customer_id_by_phone,
                );
            }
            if identifier.contains_key("customId") {
                return (
                    Value::Null,
                    Vec::new(),
                    vec![json!({
                            "message": "Resource matching the identifier was not found.",
                            "path": ["customerSet"],
                            "extensions": { "code": "NOT_FOUND" }
                    })],
                );
            }
        }

        self.customer_set_create_payload(request, &input)
    }

    fn customer_set_contact_identifier_payload(
        &mut self,
        request: &Request,
        identifier_field: &str,
        identifier_value: &str,
        input: &BTreeMap<String, ResolvedValue>,
        find: fn(&BTreeMap<String, Value>, &str) -> Option<String>,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input_value = resolved_string_field(input, identifier_field);
        let Some(input_value) = input_value else {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["input"]),
                        "The input field corresponding to the identifier is required.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        let normalized_input_value = if identifier_field == "phone" {
            normalize_customer_phone(&input_value).unwrap_or(input_value)
        } else {
            normalize_customer_email(&input_value).unwrap_or(input_value)
        };
        if normalized_input_value != identifier_value {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["input"]),
                        "The identifier value does not match the value of the corresponding field in the input.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some(id) = find(&self.store.staged.customers, identifier_value) {
            let Some(existing) = self.customer_existing_for_update(request, &id) else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            self.customer_update_existing_payload(
                request,
                "customerSet",
                &id,
                existing,
                input,
                true,
            )
        } else if let Some(id) = self.customer_upstream_contact_identifier_id(
            identifier_field,
            identifier_value,
            request,
        ) {
            let Some(existing) = self.customer_existing_for_update(request, &id) else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            self.customer_update_existing_payload(
                request,
                "customerSet",
                &id,
                existing,
                input,
                true,
            )
        } else {
            self.customer_set_create_payload(request, input)
        }
    }

    fn customer_set_create_payload(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let (errors, normalized) =
            self.customer_input_validation_errors(request, input, None, true);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        if !customer_has_identity(&normalized) {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_identity_user_error(json!(["input"]))],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let id = self.next_proxy_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let customer = customer_record_from_parts(&id, None, &normalized, &timestamp, true);
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }

    fn customer_update_existing_payload(
        &mut self,
        request: &Request,
        _root_field: &str,
        id: &str,
        existing: Value,
        input: &BTreeMap<String, ResolvedValue>,
        customer_set: bool,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let (errors, normalized) =
            self.customer_input_validation_errors(request, input, Some(id), customer_set);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let timestamp = self.next_product_timestamp();
        let customer =
            customer_record_from_parts(id, Some(&existing), &normalized, &timestamp, customer_set);
        if !customer_has_identity_json(&customer) {
            let field = if customer_set {
                json!(["input"])
            } else {
                Value::Null
            };
            return (
                customer_payload(Value::Null, vec![customer_identity_user_error(field)]),
                Vec::new(),
                Vec::new(),
            );
        }
        self.store.staged.deleted_customer_ids.remove(id);
        self.store
            .staged
            .customers
            .insert(id.to_string(), customer.clone());
        (
            customer_payload(customer, Vec::new()),
            vec![id.to_string()],
            Vec::new(),
        )
    }

    fn customer_existing_for_update(&mut self, request: &Request, id: &str) -> Option<Value> {
        if id.is_empty() || self.store.staged.deleted_customer_ids.contains(id) {
            return None;
        }
        self.store
            .staged
            .customers
            .get(id)
            .cloned()
            .or_else(|| self.hydrate_customer_for_mutation(request, id))
    }

    fn customer_exists_for_mutation(&mut self, request: &Request, id: &str) -> bool {
        self.customer_existing_for_update(request, id).is_some()
    }

    fn customer_input_validation_errors(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        current_id: Option<&str>,
        customer_set: bool,
    ) -> (Vec<Value>, NormalizedCustomerInput) {
        let mut errors = Vec::new();
        let mut normalized = NormalizedCustomerInput::default();

        if let Some(raw_email) = resolved_string_field(input, "email") {
            let email = normalize_customer_email(&raw_email);
            if raw_email.trim().is_empty() {
                normalized.email = Some(None);
            } else if let Some(email) = email {
                if self.customer_email_taken(request, current_id, &email) {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "email"),
                        "Email has already been taken",
                    ));
                }
                normalized.email = Some(Some(email));
            } else {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "email"),
                    "Email is invalid",
                ));
            }
        } else if input
            .get("email")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.email = Some(None);
        }

        if let Some(raw_phone) = resolved_string_field(input, "phone") {
            if raw_phone.trim().is_empty() {
                normalized.phone = Some(None);
            } else if let Some(phone) = normalize_customer_phone(&raw_phone) {
                if self.customer_phone_taken(request, current_id, &phone) {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "phone"),
                        "Phone has already been taken",
                    ));
                }
                normalized.phone = Some(Some(phone));
            } else {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "phone"),
                    "Phone is invalid",
                ));
            }
        } else if input
            .get("phone")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.phone = Some(None);
        }

        if let Some(raw_locale) = resolved_string_field(input, "locale") {
            if raw_locale.trim().is_empty() {
                normalized.locale = Some(None);
            } else if let Some(locale) = normalize_shopify_locale(raw_locale.trim()) {
                normalized.locale = Some(Some(locale));
            } else {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "locale"),
                    "Locale is invalid",
                ));
            }
        } else if input
            .get("locale")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.locale = Some(None);
        }

        for field in ["firstName", "lastName"] {
            if let Some(value) = resolved_string_field(input, field) {
                if value.chars().count() > 255 {
                    let message = if field == "firstName" {
                        "First name is too long (maximum is 255 characters)"
                    } else {
                        "Last name is too long (maximum is 255 characters)"
                    };
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, field),
                        message,
                    ));
                }
                let normalized_value = blank_string_to_option(value.trim().to_string());
                if field == "firstName" {
                    normalized.first_name = Some(normalized_value);
                } else {
                    normalized.last_name = Some(normalized_value);
                }
            } else if input
                .get(field)
                .is_some_and(|value| matches!(value, ResolvedValue::Null))
            {
                if field == "firstName" {
                    normalized.first_name = Some(None);
                } else {
                    normalized.last_name = Some(None);
                }
            }
        }

        if let Some(note) = resolved_string_field(input, "note") {
            if note.chars().count() > 5000 {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "note"),
                    "Note is too long (maximum is 5000 characters)",
                ));
            }
            normalized.note = Some(Some(note));
        } else if input
            .get("note")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.note = Some(None);
        }

        if input.contains_key("tags") {
            let tags = raw_taggable_tags_argument(input.get("tags"));
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "tags"),
                    "Tags is too long (maximum is 255 characters)",
                ));
            }
            let normalized_tags = normalize_taggable_tags(tags);
            if normalized_tags.len() > 250 {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "tags"),
                    "Tags cannot be more than 250",
                ));
            }
            normalized.tags = Some(normalized_tags);
        }

        if input.contains_key("taxExempt") {
            match input.get("taxExempt") {
                Some(ResolvedValue::Bool(value)) => normalized.tax_exempt = Some(*value),
                Some(ResolvedValue::Null) if customer_set => errors.push(customer_user_error(
                    json!(["input", "taxExempt"]),
                    "Tax exempt is of unexpected type NilClass",
                )),
                _ => {}
            }
        }
        if input.contains_key("taxExemptions") {
            normalized.tax_exemptions =
                Some(resolved_string_list_field_unsorted(input, "taxExemptions"));
        }
        if input.contains_key("metafields") {
            normalized.loyalty = Some(customer_loyalty_metafield(input));
        }
        if let Some(ResolvedValue::List(address_values)) = input.get("addresses") {
            let (addresses, address_errors) =
                customer_mailing_addresses(address_values, customer_set);
            errors.extend(address_errors);
            normalized.addresses = Some(addresses);
        }
        (errors, normalized)
    }

    fn customer_email_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        email: &str,
    ) -> bool {
        self.store.staged.customers.iter().any(|(id, customer)| {
            current_id != Some(id.as_str())
                && customer
                    .get("email")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| {
                        customer_email_key(existing) == customer_email_key(email)
                    })
        }) || self.customer_upstream_contact_taken(request, current_id, "email", email)
    }

    fn customer_phone_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        phone: &str,
    ) -> bool {
        self.store.staged.customers.iter().any(|(id, customer)| {
            current_id != Some(id.as_str())
                && customer
                    .get("phone")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing == phone)
        }) || self.customer_upstream_contact_taken(request, current_id, "phone", phone)
    }

    fn hydrate_customer_for_mutation(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": CUSTOMER_HYDRATE_QUERY,
                "operationName": "CustomerHydrate",
                "variables": { "id": id },
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if customer.is_null() {
            None
        } else {
            Some(normalize_hydrated_customer_record(customer))
        }
    }

    fn customer_upstream_contact_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        field: &str,
        value: &str,
    ) -> bool {
        self.customer_upstream_contact_identifier_id(field, value, request)
            .is_some_and(|id| current_id != Some(id.as_str()))
    }

    fn customer_upstream_contact_identifier_id(
        &self,
        field: &str,
        value: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let query_value = format!("{field}:{value}");
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": CUSTOMER_DUPLICATE_HYDRATE_QUERY,
                "operationName": "CustomerDuplicateHydrate",
                "variables": { "query": query_value },
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        response.body["data"]["customers"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .and_then(|node| node["id"].as_str())
            .map(str::to_string)
    }

    fn customer_create_inline_consent_response(
        &self,
        field: &RootFieldSelection,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<(Value, Vec<Value>)> {
        for field_name in ["emailMarketingConsent", "smsMarketingConsent"] {
            let Some(consent) = resolved_object_field(input, field_name) else {
                continue;
            };
            if resolved_string_field(&consent, "marketingState").as_deref() == Some("REDACTED") {
                return Some((
                    customer_payload(Value::Null, Vec::new()),
                    vec![json!({
                        "message": "Cannot specify REDACTED as a marketing state input",
                        "path": [field.response_key.clone()],
                        "extensions": { "code": "INVALID" }
                    })],
                ));
            }
        }
        if input.contains_key("emailMarketingConsent")
            && resolved_string_field(input, "email").is_none_or(|email| email.trim().is_empty())
        {
            return Some((
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["emailMarketingConsent"]),
                        "An email address is required to set the email marketing consent state.",
                    )],
                ),
                Vec::new(),
            ));
        }
        if input.contains_key("smsMarketingConsent")
            && resolved_string_field(input, "phone").is_none_or(|phone| phone.trim().is_empty())
        {
            return Some((
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["smsMarketingConsent"]),
                        "A phone number is required to set the SMS consent state.",
                    )],
                ),
                Vec::new(),
            ));
        }
        None
    }
}

#[derive(Default)]
struct NormalizedCustomerInput {
    first_name: Option<Option<String>>,
    last_name: Option<Option<String>>,
    email: Option<Option<String>>,
    phone: Option<Option<String>>,
    locale: Option<Option<String>>,
    note: Option<Option<String>>,
    tags: Option<Vec<String>>,
    tax_exempt: Option<bool>,
    tax_exemptions: Option<Vec<String>>,
    loyalty: Option<Value>,
    addresses: Option<Vec<Value>>,
}

fn customer_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({ "customer": customer, "userErrors": user_errors })
}

fn customer_user_error(field: Value, message: &str) -> Value {
    json!({ "field": field, "message": message })
}

fn customer_user_error_with_code(field: Value, message: &str, code: &str) -> Value {
    json!({ "field": field, "message": message, "code": code })
}

fn customer_identity_user_error(field: Value) -> Value {
    customer_user_error(
        field,
        "A name, phone number, or email address must be present",
    )
}

fn customer_set_not_found_payload() -> Value {
    customer_payload(
        Value::Null,
        vec![customer_user_error_with_code(
            json!(["input"]),
            "Resource matching the identifier was not found.",
            "NOT_FOUND",
        )],
    )
}

fn customer_field_path(customer_set: bool, field: &str) -> Value {
    if customer_set {
        json!(["input", field])
    } else {
        json!([field])
    }
}

fn normalize_customer_email(raw: &str) -> Option<String> {
    let email = raw.split_whitespace().collect::<String>().to_lowercase();
    if email.len() > 255 || email.is_empty() {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return None;
    }
    Some(email)
}

fn customer_email_key(email: &str) -> String {
    email.split_whitespace().collect::<String>().to_lowercase()
}

fn normalize_customer_phone(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 255 {
        return None;
    }
    if trimmed.contains('*') {
        return Some(trimmed.to_string());
    }
    let has_plus = trimmed.starts_with('+');
    let digits = trimmed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    if digits.len() < 10 || digits.len() > 15 {
        return None;
    }
    if has_plus {
        return Some(format!("+{digits}"));
    }
    if !has_plus && digits.len() == 10 {
        Some(format!("+1{digits}"))
    } else {
        Some(format!("+{digits}"))
    }
}

fn blank_string_to_option(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn raw_taggable_tags_argument(value: Option<&ResolvedValue>) -> Vec<String> {
    match value {
        Some(ResolvedValue::String(value)) => value.split(',').map(str::to_string).collect(),
        Some(ResolvedValue::List(values)) => values
            .iter()
            .flat_map(|value| match value {
                ResolvedValue::String(value) => value.split(',').map(str::to_string).collect(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn customer_has_identity(input: &NormalizedCustomerInput) -> bool {
    input
        .first_name
        .as_ref()
        .and_then(|value| value.as_ref())
        .is_some_and(|value| !value.trim().is_empty())
        || input
            .last_name
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
        || input
            .email
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
        || input
            .phone
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
}

fn customer_has_identity_json(customer: &Value) -> bool {
    ["firstName", "lastName", "email", "phone"]
        .iter()
        .any(|field| {
            customer
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn customer_record_from_parts(
    id: &str,
    existing: Option<&Value>,
    input: &NormalizedCustomerInput,
    timestamp: &str,
    customer_set: bool,
) -> Value {
    let first = customer_string_value(input.first_name.as_ref(), existing, "firstName");
    let last = customer_string_value(input.last_name.as_ref(), existing, "lastName");
    let email = customer_string_value(input.email.as_ref(), existing, "email");
    let phone = customer_string_value(input.phone.as_ref(), existing, "phone");
    let locale = customer_string_value(input.locale.as_ref(), existing, "locale")
        .or_else(|| Some("en".to_string()));
    let note = customer_string_value(input.note.as_ref(), existing, "note");
    let tags = input
        .tags
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["tags"].as_array().map(|tags| {
                    tags.iter()
                        .filter_map(|tag| tag.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default();
    let tax_exempt = input
        .tax_exempt
        .or_else(|| existing.and_then(|customer| customer["taxExempt"].as_bool()))
        .unwrap_or(false);
    let tax_exemptions = input
        .tax_exemptions
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["taxExemptions"].as_array().map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default();
    let loyalty = input
        .loyalty
        .clone()
        .or_else(|| existing.and_then(|customer| customer.get("loyalty").cloned()))
        .unwrap_or(Value::Null);
    let addresses = input
        .addresses
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["addressesV2"]["nodes"]
                    .as_array()
                    .map(|addresses| addresses.to_vec())
            })
        })
        .unwrap_or_default();
    let created_at = existing
        .and_then(|customer| customer["createdAt"].as_str())
        .unwrap_or(timestamp);
    let verified_email = existing
        .and_then(|customer| customer["verifiedEmail"].as_bool())
        .unwrap_or(customer_set);
    customer_record(CustomerRecordInput {
        id,
        first: first.as_deref(),
        last: last.as_deref(),
        email: email.as_deref(),
        phone: phone.as_deref(),
        locale: locale.as_deref(),
        note: note.as_deref(),
        verified_email,
        tax_exempt,
        tax_exemptions,
        tags,
        loyalty,
        addresses,
        created_at,
        updated_at: timestamp,
    })
}

fn customer_string_value(
    input: Option<&Option<String>>,
    existing: Option<&Value>,
    field: &str,
) -> Option<String> {
    match input {
        Some(value) => value.clone(),
        None => existing
            .and_then(|customer| customer.get(field))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

struct CustomerRecordInput<'a> {
    id: &'a str,
    first: Option<&'a str>,
    last: Option<&'a str>,
    email: Option<&'a str>,
    phone: Option<&'a str>,
    locale: Option<&'a str>,
    note: Option<&'a str>,
    verified_email: bool,
    tax_exempt: bool,
    tax_exemptions: Vec<String>,
    tags: Vec<String>,
    loyalty: Value,
    addresses: Vec<Value>,
    created_at: &'a str,
    updated_at: &'a str,
}

fn customer_record(input: CustomerRecordInput<'_>) -> Value {
    let first_value = input.first.filter(|value| !value.is_empty());
    let last_value = input.last.filter(|value| !value.is_empty());
    let display_name = customer_display_name(first_value, last_value, input.email);
    let metafields = if input.loyalty.is_null() {
        json!({ "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } })
    } else {
        json!({ "nodes": [input.loyalty.clone()], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:customer-metafield:1", "endCursor": "cursor:customer-metafield:1" } })
    };
    let default_address = input.addresses.first().cloned().unwrap_or(Value::Null);
    let start_cursor = input.addresses.first().and_then(customer_address_cursor);
    let end_cursor = input.addresses.last().and_then(customer_address_cursor);
    let address_edges = input
        .addresses
        .iter()
        .map(|address| json!({ "cursor": customer_address_cursor(address), "node": address }))
        .collect::<Vec<_>>();
    json!({
        "id": input.id,
        "firstName": first_value,
        "lastName": last_value,
        "displayName": display_name,
        "email": input.email,
        "phone": input.phone,
        "locale": input.locale,
        "note": input.note,
        "verifiedEmail": input.verified_email,
        "taxExempt": input.tax_exempt,
        "taxExemptions": input.tax_exemptions,
        "tags": input.tags,
        "state": "DISABLED",
        "canDelete": true,
        "loyalty": input.loyalty.clone(),
        "metafield": input.loyalty,
        "metafields": metafields,
        "defaultEmailAddress": input.email.map(|email| json!({ "emailAddress": email })).unwrap_or(Value::Null),
        "defaultPhoneNumber": input.phone.map(|phone| json!({ "phoneNumber": phone })).unwrap_or(Value::Null),
        "defaultAddress": default_address,
        "addressesV2": {
            "nodes": input.addresses,
            "edges": address_edges,
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": start_cursor, "endCursor": end_cursor }
        },
        "createdAt": input.created_at,
        "updatedAt": input.updated_at
    })
}

fn customer_address_cursor(address: &Value) -> Option<String> {
    address
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
}

fn customer_mailing_addresses(
    values: &[ResolvedValue],
    customer_set: bool,
) -> (Vec<Value>, Vec<Value>) {
    let mut addresses = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let ResolvedValue::Object(input) = value else {
            continue;
        };
        let (address, mut address_errors) = customer_mailing_address(input, index, customer_set);
        if !address_errors.is_empty() {
            errors.append(&mut address_errors);
            continue;
        }
        let mut address_key = address.clone();
        if let Some(object) = address_key.as_object_mut() {
            object.remove("id");
        }
        let key = serde_json::to_string(&address_key).unwrap_or_default();
        if seen.insert(key) {
            addresses.push(address);
        }
    }
    (addresses, errors)
}

fn customer_mailing_address(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    customer_set: bool,
) -> (Value, Vec<Value>) {
    let mut errors = Vec::new();
    for field in [
        "firstName",
        "lastName",
        "address1",
        "address2",
        "city",
        "company",
        "zip",
        "phone",
    ] {
        if let Some(value) = customer_address_string(input, field) {
            let label = customer_address_field_label(field);
            if value.chars().count() > 255 {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} is too long (maximum is 255 characters)"),
                ));
            }
            if customer_address_contains_html(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain HTML tags"),
                ));
            }
            if matches!(field, "city" | "zip" | "phone") && customer_address_contains_url(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain URL"),
                ));
            }
            if customer_address_contains_emoji(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain emojis"),
                ));
            }
        }
    }

    let country_input = customer_address_string(input, "countryCode")
        .or_else(|| customer_address_string(input, "countryCodeV2"))
        .or_else(|| customer_address_string(input, "country"));
    let province_input = customer_address_string(input, "provinceCode")
        .or_else(|| customer_address_string(input, "province"));
    let country = match country_input
        .as_deref()
        .and_then(customer_country_from_input)
    {
        Some(country) => Some(country),
        None if country_input.is_some() => {
            errors.push(customer_user_error(
                customer_address_field_path(customer_set, index, Some("country")),
                "Country is invalid",
            ));
            None
        }
        None => None,
    };
    let province = match (&country, province_input.as_deref()) {
        (Some(country), Some(raw_province)) => {
            match customer_province_from_input(country.code, raw_province) {
                Some(province) => province,
                None => {
                    errors.push(customer_user_error(
                        customer_address_field_path(customer_set, index, Some("province")),
                        "Province is invalid",
                    ));
                    None
                }
            }
        }
        _ => None,
    };
    let country = country.cloned();
    let province = province.cloned();

    if !errors.is_empty() {
        return (Value::Null, errors);
    }

    let first_name = customer_address_string(input, "firstName");
    let last_name = customer_address_string(input, "lastName");
    let address1 = customer_address_string(input, "address1");
    let address2 = customer_address_string(input, "address2");
    let city = customer_address_string(input, "city");
    let company = customer_address_string(input, "company");
    let zip = customer_address_string(input, "zip");
    let phone = customer_address_string(input, "phone");
    let is_blank = [
        first_name.as_deref(),
        last_name.as_deref(),
        address1.as_deref(),
        address2.as_deref(),
        city.as_deref(),
        company.as_deref(),
        zip.as_deref(),
        phone.as_deref(),
        country.as_ref().map(|country| country.code),
        province.as_ref().map(|province| province.code),
    ]
    .into_iter()
    .flatten()
    .all(str::is_empty);
    if is_blank && !customer_set {
        return (
            Value::Null,
            vec![customer_user_error(
                customer_address_field_path(customer_set, index, None),
                "Customer address cannot be blank.",
            )],
        );
    }

    let name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area =
        customer_formatted_area(city.as_deref(), country.as_ref(), province.as_ref());
    let id = format!(
        "gid://shopify/MailingAddress/{}?shopify-draft-proxy=synthetic",
        index + 1
    );
    (
        json!({
            "id": id,
            "firstName": first_name,
            "lastName": last_name,
            "address1": address1,
            "address2": address2,
            "city": city,
            "company": company,
            "province": province.as_ref().map(|province| province.name),
            "provinceCode": province.as_ref().map(|province| province.code),
            "country": country.as_ref().map(|country| country.name),
            "countryCodeV2": country.as_ref().map(|country| country.code),
            "zip": zip,
            "phone": phone,
            "name": if name.is_empty() { Value::Null } else { json!(name) },
            "formattedArea": formatted_area,
        }),
        Vec::new(),
    )
}

#[derive(Clone, Copy)]
struct CustomerCountry {
    code: &'static str,
    name: &'static str,
}

#[derive(Clone, Copy)]
struct CustomerProvince {
    code: &'static str,
    name: &'static str,
}

fn customer_address_string(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<String> {
    resolved_string_field(input, field).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn customer_address_field_label(field: &str) -> &'static str {
    match field {
        "firstName" => "First name",
        "lastName" => "Last name",
        "address1" => "Address1",
        "address2" => "Address2",
        "city" => "City",
        "company" => "Company",
        "zip" => "Zip",
        "phone" => "Phone",
        "country" | "countryCode" | "countryCodeV2" => "Country",
        "province" | "provinceCode" => "Province",
        _ => "Address",
    }
}

fn customer_address_field_path(customer_set: bool, index: usize, field: Option<&str>) -> Value {
    let mut path = if customer_set {
        vec![
            "input".to_string(),
            "addresses".to_string(),
            index.to_string(),
        ]
    } else {
        vec!["addresses".to_string(), index.to_string()]
    };
    if let Some(field) = field {
        let field = match field {
            "countryCode" | "countryCodeV2" => "country",
            "provinceCode" => "province",
            other => other,
        };
        path.push(field.to_string());
    }
    json!(path)
}

fn customer_address_contains_html(value: &str) -> bool {
    value.contains('<') || value.contains('>')
}

fn customer_address_contains_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

fn customer_address_contains_emoji(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF))
}

fn customer_country_from_input(value: &str) -> Option<&'static CustomerCountry> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    CUSTOMER_COUNTRIES.iter().find(|country| {
        country.code.eq_ignore_ascii_case(normalized)
            || country.name.eq_ignore_ascii_case(normalized)
    })
}

fn customer_province_from_input(
    country_code: &str,
    value: &str,
) -> Option<Option<&'static CustomerProvince>> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Some(None);
    }
    let provinces = customer_country_provinces(country_code);
    if provinces.is_empty() {
        return Some(None);
    }
    provinces
        .iter()
        .find(|province| {
            province.code.eq_ignore_ascii_case(normalized)
                || province.name.eq_ignore_ascii_case(normalized)
        })
        .map(Some)
}

fn customer_country_provinces(country_code: &str) -> &'static [CustomerProvince] {
    match country_code {
        "CA" => CUSTOMER_CANADIAN_PROVINCES,
        "US" => CUSTOMER_US_PROVINCES,
        _ => &[],
    }
}

fn customer_formatted_area(
    city: Option<&str>,
    country: Option<&CustomerCountry>,
    province: Option<&CustomerProvince>,
) -> Value {
    let Some(country) = country else {
        return Value::Null;
    };
    let city = city.filter(|city| !city.is_empty());
    let province_code = province.map(|province| province.code);
    let value = match (city, province_code) {
        (Some(city), Some(province_code)) => format!("{city} {province_code}, {}", country.name),
        (Some(city), None) if country.code == "SG" => city.to_string(),
        (Some(city), None) => format!("{city}, {}", country.name),
        (None, Some(province_code)) => format!("{province_code}, {}", country.name),
        (None, None) => country.name.to_string(),
    };
    if value.is_empty() {
        Value::Null
    } else {
        json!(value)
    }
}

const CUSTOMER_COUNTRIES: &[CustomerCountry] = &[
    CustomerCountry {
        code: "CA",
        name: "Canada",
    },
    CustomerCountry {
        code: "SG",
        name: "Singapore",
    },
    CustomerCountry {
        code: "US",
        name: "United States",
    },
];

const CUSTOMER_CANADIAN_PROVINCES: &[CustomerProvince] = &[
    CustomerProvince {
        code: "AB",
        name: "Alberta",
    },
    CustomerProvince {
        code: "BC",
        name: "British Columbia",
    },
    CustomerProvince {
        code: "MB",
        name: "Manitoba",
    },
    CustomerProvince {
        code: "NB",
        name: "New Brunswick",
    },
    CustomerProvince {
        code: "NL",
        name: "Newfoundland and Labrador",
    },
    CustomerProvince {
        code: "NS",
        name: "Nova Scotia",
    },
    CustomerProvince {
        code: "NT",
        name: "Northwest Territories",
    },
    CustomerProvince {
        code: "NU",
        name: "Nunavut",
    },
    CustomerProvince {
        code: "ON",
        name: "Ontario",
    },
    CustomerProvince {
        code: "PE",
        name: "Prince Edward Island",
    },
    CustomerProvince {
        code: "QC",
        name: "Quebec",
    },
    CustomerProvince {
        code: "SK",
        name: "Saskatchewan",
    },
    CustomerProvince {
        code: "YT",
        name: "Yukon",
    },
];

const CUSTOMER_US_PROVINCES: &[CustomerProvince] = &[
    CustomerProvince {
        code: "CA",
        name: "California",
    },
    CustomerProvince {
        code: "IL",
        name: "Illinois",
    },
    CustomerProvince {
        code: "NY",
        name: "New York",
    },
];

fn normalize_hydrated_customer_record(mut customer: Value) -> Value {
    if let Some(object) = customer.as_object_mut() {
        if !object.contains_key("phone") {
            let phone = object
                .get("defaultPhoneNumber")
                .and_then(|default| default.get("phoneNumber"))
                .cloned()
                .unwrap_or(Value::Null);
            object.insert("phone".to_string(), phone);
        }
        if !object.contains_key("firstName") {
            object.insert("firstName".to_string(), Value::Null);
        }
        if !object.contains_key("lastName") {
            object.insert("lastName".to_string(), Value::Null);
        }
        if !object.contains_key("note") {
            object.insert("note".to_string(), Value::Null);
        }
        if !object.contains_key("tags") {
            object.insert("tags".to_string(), json!([]));
        }
        if !object.contains_key("taxExemptions") {
            object.insert("taxExemptions".to_string(), json!([]));
        }
    }
    customer
}

fn customer_display_name(first: Option<&str>, last: Option<&str>, email: Option<&str>) -> String {
    let name = [first, last]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        name
    } else {
        email.unwrap_or_default().to_string()
    }
}

fn find_customer_id_by_email(customers: &BTreeMap<String, Value>, email: &str) -> Option<String> {
    customers.iter().find_map(|(id, customer)| {
        customer
            .get("email")
            .and_then(Value::as_str)
            .is_some_and(|existing| customer_email_key(existing) == customer_email_key(email))
            .then(|| id.clone())
    })
}

fn find_customer_id_by_phone(customers: &BTreeMap<String, Value>, phone: &str) -> Option<String> {
    customers.iter().find_map(|(id, customer)| {
        customer
            .get("phone")
            .and_then(Value::as_str)
            .is_some_and(|existing| existing == phone)
            .then(|| id.clone())
    })
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

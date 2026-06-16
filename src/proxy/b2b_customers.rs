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
        if let Some(error) = b2b_tax_exemption_coercion_error(query, &fields) {
            return Some(ok_json(json!({ "errors": [error] })));
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
        if parsed_root_fields.is_empty() {
            return None;
        }

        let fields = root_fields(query, variables)?;
        match operation_type {
            OperationType::Mutation
                if parsed_root_fields.iter().all(|field| {
                    matches!(
                        field.as_str(),
                        "companyCreate"
                            | "companyUpdate"
                            | "companyDelete"
                            | "companiesDelete"
                            | "companyLocationCreate"
                            | "storeCreditAccountCredit"
                    )
                }) =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) = match field.name.as_str() {
                        "companyCreate" => self.b2b_company_create_payload(&field),
                        "companyUpdate" => self.b2b_company_update_payload(&field),
                        "companyDelete" => self.b2b_company_delete_payload(&field),
                        "companiesDelete" => self.b2b_companies_delete_payload(&field),
                        "companyLocationCreate" => self.b2b_company_location_create_payload(&field),
                        "storeCreditAccountCredit"
                            if self.b2b_store_credit_account_credit_should_handle(&field) =>
                        {
                            self.b2b_store_credit_account_credit_payload(&field)
                        }
                        "storeCreditAccountCredit" => return None,
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
            OperationType::Query
                if self.has_b2b_contact_state()
                    && parsed_root_fields.iter().all(|field| field == "company") =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    let company = self
                        .b2b_company_materialized(&id)
                        .map(|company| selected_json(&company, &field.selection))
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
        let default_role = self.ensure_b2b_contact_role(&id, "Ordering only", true);
        let admin_role = self.ensure_b2b_contact_role(&id, "Location admin", false);
        let company = json!({
            "id": id,
            "name": name,
            "externalId": resolved_string_field(&company_input, "externalId").map(Value::String).unwrap_or(Value::Null),
            "customerSince": resolved_string_field(&company_input, "customerSince").map(Value::String).unwrap_or(Value::Null),
            "note": resolved_string_field(&company_input, "note").map(Value::String).unwrap_or(Value::Null),
            "mainContactId": Value::Null,
            "mainContact": Value::Null,
            "contactIds": [],
            "contacts": connection_json(Vec::new()),
            "contactsCount": { "count": 0, "precision": "EXACT" },
            "locationIds": [],
            "locations": connection_json(Vec::new()),
            "locationsCount": { "count": 0, "precision": "EXACT" },
            "contactRoles": connection_json(vec![admin_role.clone(), default_role.clone()]),
            "defaultRole": default_role
        });
        self.store
            .staged
            .b2b_companies
            .insert(id.clone(), company.clone());
        let mut staged_ids = vec![id.clone()];

        if let Some(contact_input) = resolved_object_field(&input, "companyContact") {
            let contact_id =
                synthetic_shopify_gid("CompanyContact", self.store.staged.next_b2b_contact_id);
            self.store.staged.next_b2b_contact_id += 1;
            let customer_id = synthetic_shopify_gid(
                "Customer",
                format!("contact-{}", resource_id_tail(&contact_id)),
            );
            let contact = self.b2b_contact_from_input(
                contact_id.clone(),
                id.clone(),
                &contact_input,
                customer_id,
            );
            self.store
                .staged
                .b2b_contacts
                .insert(contact_id.clone(), contact);
            self.append_b2b_company_contact(&id, &contact_id);
            if let Some(company) = self.store.staged.b2b_companies.get_mut(&id) {
                company["mainContactId"] = json!(contact_id.clone());
            }
            staged_ids.push(contact_id);
        }

        let mut location_input =
            resolved_object_field(&input, "companyLocation").unwrap_or_default();
        if !location_input.contains_key("name") {
            location_input.insert("name".to_string(), ResolvedValue::String(name));
        }
        let location_id = self.stage_b2b_company_location(&id, &location_input);
        staged_ids.push(location_id.clone());
        if let Some(main_contact_id) = self
            .store
            .staged
            .b2b_companies
            .get(&id)
            .and_then(|company| company.get("mainContactId"))
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if let Ok(assignment_id) = self.b2b_create_role_assignment(
                &main_contact_id,
                default_role
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                &location_id,
                None,
            ) {
                staged_ids.push(assignment_id);
            }
        }

        let materialized = self.b2b_company_materialized(&id).unwrap_or(company);
        (
            b2b_company_payload(Some(&materialized), Vec::new()),
            "staged",
            staged_ids,
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
        let materialized = self
            .b2b_company_materialized(&company_id)
            .unwrap_or(company.clone());
        (
            b2b_company_payload(Some(&materialized), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    fn b2b_company_location_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_location_payload(
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
        }
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(name) = resolved_string_field(&input, "name") {
            if name.chars().count() > 255 {
                return (
                    b2b_company_location_payload(
                        None,
                        vec![b2b_company_user_error(
                            vec!["input", "name"],
                            "Name is too long (maximum is 255 characters)",
                            "TOO_LONG",
                            None,
                        )],
                    ),
                    "failed",
                    Vec::new(),
                );
            }
        }
        let id = self.stage_b2b_company_location(&company_id, &input);
        let materialized = self.b2b_company_location_materialized(&id);
        (
            b2b_company_location_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![id],
        )
    }

    pub(in crate::proxy) fn b2b_company_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                Self::b2b_company_delete_payload_value(
                    None,
                    vec![b2b_company_user_error(
                        vec!["id"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if self.b2b_company_has_delete_blocker(&company_id) {
            return (
                Self::b2b_company_delete_payload_value(
                    None,
                    vec![b2b_company_user_error(
                        vec!["id"],
                        "Failed to delete the company.",
                        "FAILED_TO_DELETE",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }

        self.b2b_remove_company_graph(&company_id);
        (
            Self::b2b_company_delete_payload_value(Some(&company_id), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    pub(in crate::proxy) fn b2b_companies_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_ids = resolved_string_list_field_unsorted(&field.arguments, "companyIds");
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, company_id) in company_ids.iter().enumerate() {
            if !self.store.staged.b2b_companies.contains_key(company_id) {
                user_errors.push(b2b_company_user_error(
                    vec!["companyIds", &index.to_string()],
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                    None,
                ));
            } else if self.b2b_company_has_delete_blocker(company_id) {
                user_errors.push(b2b_company_user_error(
                    vec!["companyIds", &index.to_string()],
                    "Failed to delete the company.",
                    "FAILED_TO_DELETE",
                    None,
                ));
            } else {
                deleted_ids.push(company_id.clone());
            }
        }
        for company_id in &deleted_ids {
            self.b2b_remove_company_graph(company_id);
        }
        let status = if deleted_ids.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "deletedCompanyIds": deleted_ids,
                "userErrors": user_errors
            }),
            status,
            deleted_ids,
        )
    }

    pub(in crate::proxy) fn b2b_store_credit_account_credit_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let owner_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.b2b_locations.contains_key(&owner_id) {
            return (
                Self::b2b_store_credit_payload_value(
                    None,
                    vec![b2b_company_user_error(
                        vec!["id"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        let input = resolved_object_field(&field.arguments, "creditInput").unwrap_or_default();
        let money = resolved_object_field(&input, "creditAmount").unwrap_or_default();
        let amount = resolved_money_amount_string(money.get("amount"));
        let normalized_amount = normalized_money_amount(&amount);
        let currency =
            resolved_string_field(&money, "currencyCode").unwrap_or_else(|| "USD".to_string());
        let account_id = format!(
            "gid://shopify/StoreCreditAccount/{}?shopify-draft-proxy=synthetic",
            resource_id_tail(&owner_id)
        );
        let account = json!({
            "id": account_id,
            "balance": { "amount": normalized_amount, "currencyCode": currency },
            "owner": { "id": owner_id }
        });
        let transaction = json!({
            "amount": { "amount": normalized_amount, "currencyCode": currency },
            "balanceAfterTransaction": { "amount": normalized_amount, "currencyCode": currency },
            "event": "ADJUSTMENT",
            "origin": Value::Null,
            "account": account
        });
        if let Some(location) = self.store.staged.b2b_locations.get_mut(&owner_id) {
            location["storeCreditAccount"] = transaction["account"].clone();
        }
        (
            Self::b2b_store_credit_payload_value(Some(&transaction), Vec::new()),
            "staged",
            vec![owner_id],
        )
    }

    fn b2b_store_credit_account_credit_should_handle(&self, field: &RootFieldSelection) -> bool {
        resolved_string_arg(&field.arguments, "id")
            .is_some_and(|id| self.store.staged.b2b_locations.contains_key(&id))
    }

    fn b2b_company_has_delete_blocker(&self, company_id: &str) -> bool {
        self.store
            .staged
            .orders
            .values()
            .any(|order| Self::b2b_record_references_company(order, company_id))
            || self
                .store
                .staged
                .draft_orders
                .values()
                .any(|draft_order| Self::b2b_record_references_company(draft_order, company_id))
            || self
                .store
                .staged
                .order_customer_orders
                .values()
                .any(|order| Self::b2b_record_references_company(order, company_id))
            || self
                .b2b_company_location_ids(company_id)
                .iter()
                .any(|location_id| {
                    self.store
                        .staged
                        .b2b_locations
                        .get(location_id)
                        .is_some_and(Self::b2b_location_has_store_credit_balance)
                })
    }

    fn b2b_company_location_ids(&self, company_id: &str) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .b2b_companies
            .get(company_id)
            .and_then(|company| company.get("locationIds"))
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ids.extend(
            self.store
                .staged
                .b2b_locations
                .iter()
                .filter(|(_, location)| {
                    location.get("companyId").and_then(Value::as_str) == Some(company_id)
                        || location["company"]["id"].as_str() == Some(company_id)
                })
                .map(|(id, _)| id.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    }

    fn b2b_remove_company_graph(&mut self, company_id: &str) {
        let location_ids = self.b2b_company_location_ids(company_id);
        self.store.staged.b2b_companies.remove(company_id);
        for location_id in location_ids {
            self.store.staged.b2b_locations.remove(&location_id);
        }
    }

    fn b2b_company_delete_payload_value(
        deleted_company_id: Option<&str>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "deletedCompanyId": deleted_company_id.map(|id| json!(id)).unwrap_or(Value::Null),
            "userErrors": user_errors
        })
    }

    fn b2b_store_credit_payload_value(
        transaction: Option<&Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "storeCreditAccountTransaction": transaction.cloned().unwrap_or(Value::Null),
            "userErrors": user_errors
        })
    }

    fn b2b_record_references_company(record: &Value, company_id: &str) -> bool {
        Self::b2b_value_contains_company_id(record.get("purchasingEntity"), company_id)
            || Self::b2b_value_contains_company_id(
                record
                    .get("order")
                    .and_then(|order| order.get("purchasingEntity")),
                company_id,
            )
            || Self::b2b_value_contains_company_id(record.get("purchasingCompany"), company_id)
    }

    fn b2b_value_contains_company_id(value: Option<&Value>, company_id: &str) -> bool {
        let Some(value) = value else {
            return false;
        };
        if value["companyId"].as_str() == Some(company_id)
            || value["company"]["id"].as_str() == Some(company_id)
            || value["company"].as_str() == Some(company_id)
        {
            return true;
        }
        if let Some(object) = value.as_object() {
            return object
                .values()
                .any(|nested| Self::b2b_value_contains_company_id(Some(nested), company_id));
        }
        if let Some(values) = value.as_array() {
            return values
                .iter()
                .any(|nested| Self::b2b_value_contains_company_id(Some(nested), company_id));
        }
        false
    }

    fn b2b_location_has_store_credit_balance(location: &Value) -> bool {
        location["storeCreditAccount"]["balance"]["amount"]
            .as_str()
            .and_then(|amount| amount.parse::<f64>().ok())
            .is_some_and(|amount| amount > 0.0)
    }

    pub(in crate::proxy) fn b2b_contact_role_response(
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
            OperationType::Query
                if parsed_root_fields
                    .iter()
                    .all(|field| b2b_contact_query_root(field)) =>
            {
                if self.config.read_mode != ReadMode::Snapshot && !self.has_b2b_contact_state() {
                    return None;
                }
                if fields
                    .iter()
                    .any(|field| field.name == "node" && !b2b_node_query_field(field))
                {
                    return None;
                }
                let mut data = serde_json::Map::new();
                for field in fields {
                    let value = match field.name.as_str() {
                        "company" => resolved_string_arg(&field.arguments, "id")
                            .and_then(|id| self.b2b_company_materialized(&id))
                            .map(|company| selected_json(&company, &field.selection))
                            .unwrap_or(Value::Null),
                        "companyContact" => resolved_string_arg(&field.arguments, "id")
                            .and_then(|id| self.b2b_contact_materialized(&id))
                            .map(|contact| selected_json(&contact, &field.selection))
                            .unwrap_or(Value::Null),
                        "companyContactRole" => resolved_string_arg(&field.arguments, "id")
                            .and_then(|id| self.b2b_contact_role_materialized(&id))
                            .map(|role| selected_json(&role, &field.selection))
                            .unwrap_or(Value::Null),
                        "companyLocation" => resolved_string_arg(&field.arguments, "id")
                            .and_then(|id| self.b2b_company_location_materialized(&id))
                            .map(|location| selected_json(&location, &field.selection))
                            .unwrap_or(Value::Null),
                        "node" => resolved_string_arg(&field.arguments, "id")
                            .and_then(|id| self.b2b_node_materialized(&id))
                            .map(|node| selected_json(&node, &field.selection))
                            .unwrap_or(Value::Null),
                        _ => return None,
                    };
                    data.insert(field.response_key.clone(), value);
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            OperationType::Mutation
                if parsed_root_fields
                    .iter()
                    .all(|field| b2b_contact_mutation_root(field)) =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) = match field.name.as_str() {
                        "companyContactCreate" => self.b2b_company_contact_create_payload(&field),
                        "companyContactUpdate" => self.b2b_company_contact_update_payload(&field),
                        "companyContactDelete" => self.b2b_company_contact_delete_payload(&field),
                        "companyContactsDelete" => self.b2b_company_contacts_delete_payload(&field),
                        "companyAssignCustomerAsContact" => {
                            self.b2b_company_assign_customer_as_contact_payload(&field)
                        }
                        "companyContactRemoveFromCompany" => {
                            self.b2b_company_contact_remove_from_company_payload(&field)
                        }
                        "companyAssignMainContact" => {
                            self.b2b_company_assign_main_contact_payload(&field)
                        }
                        "companyRevokeMainContact" => {
                            self.b2b_company_revoke_main_contact_payload(&field)
                        }
                        "companyContactAssignRole" => {
                            self.b2b_company_contact_assign_role_payload(&field)
                        }
                        "companyContactAssignRoles" => {
                            self.b2b_company_contact_assign_roles_payload(&field)
                        }
                        "companyContactRevokeRole" => {
                            self.b2b_company_contact_revoke_role_payload(&field)
                        }
                        "companyContactRevokeRoles" => {
                            self.b2b_company_contact_revoke_roles_payload(&field)
                        }
                        "companyLocationAssignRoles" => {
                            self.b2b_company_location_assign_roles_payload(&field)
                        }
                        "companyLocationRevokeRoles" => {
                            self.b2b_company_location_revoke_roles_payload(&field)
                        }
                        "companyLocationUpdate" => self.b2b_company_location_update_payload(&field),
                        "companyLocationAssignAddress" => {
                            self.b2b_company_location_assign_address_payload(&field)
                        }
                        "companyAddressDelete" => self.b2b_company_address_delete_payload(&field),
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
            _ => None,
        }
    }

    pub(in crate::proxy) fn has_b2b_contact_state(&self) -> bool {
        !self.store.staged.b2b_companies.is_empty()
            || !self.store.staged.b2b_locations.is_empty()
            || !self.store.staged.b2b_contacts.is_empty()
            || !self.store.staged.b2b_contact_roles.is_empty()
            || !self.store.staged.b2b_contact_role_assignments.is_empty()
    }

    fn b2b_company_contact_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_contact_payload(
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
        }
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let errors = b2b_company_contact_input_errors(&input, &["input"]);
        if !errors.is_empty() {
            return (
                b2b_company_contact_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }

        let id = synthetic_shopify_gid("CompanyContact", self.store.staged.next_b2b_contact_id);
        self.store.staged.next_b2b_contact_id += 1;
        let customer_id =
            synthetic_shopify_gid("Customer", format!("contact-{}", resource_id_tail(&id)));
        let contact =
            self.b2b_contact_from_input(id.clone(), company_id.clone(), &input, customer_id);
        self.store.staged.deleted_b2b_contact_ids.remove(&id);
        self.store.staged.b2b_contacts.insert(id.clone(), contact);
        self.append_b2b_company_contact(&company_id, &id);
        let materialized = self.b2b_contact_materialized(&id);
        (
            b2b_company_contact_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![id],
        )
    }

    fn b2b_company_contact_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.b2b_contact_exists(&contact_id) {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let errors = b2b_company_contact_input_errors(&input, &["input"]);
        if !errors.is_empty() {
            return (
                b2b_company_contact_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }

        let mut contact = self
            .store
            .staged
            .b2b_contacts
            .get(&contact_id)
            .cloned()
            .unwrap_or(Value::Null);
        self.apply_b2b_contact_input(&mut contact, &input);
        self.store
            .staged
            .b2b_contacts
            .insert(contact_id.clone(), contact);
        let materialized = self.b2b_contact_materialized(&contact_id);
        (
            b2b_company_contact_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![contact_id],
        )
    }

    fn b2b_company_contact_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.b2b_delete_contact(&contact_id) {
            return (
                b2b_company_contact_delete_payload(
                    None,
                    "deletedCompanyContactId",
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        (
            b2b_company_contact_delete_payload(
                Some(&contact_id),
                "deletedCompanyContactId",
                Vec::new(),
            ),
            "staged",
            vec![contact_id],
        )
    }

    fn b2b_company_contacts_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_ids =
            resolved_string_list_field_unsorted(&field.arguments, "companyContactIds");
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        for (index, contact_id) in contact_ids.iter().enumerate() {
            if self.b2b_delete_contact(contact_id) {
                deleted.push(contact_id.clone());
            } else {
                errors.push(json!({
                    "field": ["companyContactIds", index.to_string()],
                    "message": "The company contact doesn't exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }));
            }
        }
        let status = if deleted.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "deletedCompanyContactIds": deleted,
                "userErrors": errors
            }),
            status,
            contact_ids,
        )
    }

    fn b2b_company_assign_customer_as_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_contact_payload(
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
        }
        let Some(customer) = self.store.staged.customers.get(&customer_id).cloned() else {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["customerId"],
                        "Customer does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        };
        if customer
            .get("email")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyId"],
                        "Customer must have an email address.",
                        "INVALID_INPUT",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if self.store.staged.b2b_contacts.values().any(|contact| {
            contact.get("customerId").and_then(Value::as_str) == Some(customer_id.as_str())
                && !self.store.staged.deleted_b2b_contact_ids.contains(
                    contact
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
        }) {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyId"],
                        "Customer is already associated with a company contact.",
                        "INVALID_INPUT",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }

        let id = synthetic_shopify_gid("CompanyContact", self.store.staged.next_b2b_contact_id);
        self.store.staged.next_b2b_contact_id += 1;
        let contact = self.b2b_contact_from_customer(id.clone(), company_id.clone(), &customer);
        self.store.staged.b2b_contacts.insert(id.clone(), contact);
        self.append_b2b_company_contact(&company_id, &id);
        let materialized = self.b2b_contact_materialized(&id);
        (
            b2b_company_contact_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![id],
        )
    }

    fn b2b_company_contact_remove_from_company_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.b2b_delete_contact(&contact_id) {
            return (
                b2b_company_contact_delete_payload(
                    None,
                    "removedCompanyContactId",
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        (
            b2b_company_contact_delete_payload(
                Some(&contact_id),
                "removedCompanyContactId",
                Vec::new(),
            ),
            "staged",
            vec![contact_id],
        )
    }

    fn b2b_company_assign_main_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let Some(contact) = self.store.staged.b2b_contacts.get(&contact_id) else {
            return (
                b2b_company_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        };
        if contact.get("companyId").and_then(Value::as_str) != Some(company_id.as_str()) {
            return (
                b2b_company_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "The company contact does not belong to the company.",
                        "INVALID_INPUT",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        let Some(company) = self.store.staged.b2b_companies.get_mut(&company_id) else {
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
        company["mainContactId"] = json!(contact_id);
        let materialized = self.b2b_company_materialized(&company_id);
        (
            b2b_company_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![company_id, contact_id],
        )
    }

    fn b2b_company_revoke_main_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let Some(company) = self.store.staged.b2b_companies.get_mut(&company_id) else {
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
        company["mainContactId"] = Value::Null;
        let materialized = self.b2b_company_materialized(&company_id);
        (
            b2b_company_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    fn b2b_company_contact_assign_role_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let role_id =
            resolved_string_arg(&field.arguments, "companyContactRoleId").unwrap_or_default();
        let location_id =
            resolved_string_arg(&field.arguments, "companyLocationId").unwrap_or_default();
        match self.b2b_create_role_assignment(&contact_id, &role_id, &location_id, None) {
            Ok(assignment_id) => {
                let assignment = self.b2b_role_assignment_materialized(&assignment_id);
                (
                    json!({
                        "companyContactRoleAssignment": assignment.unwrap_or(Value::Null),
                        "userErrors": []
                    }),
                    "staged",
                    vec![assignment_id],
                )
            }
            Err(error) => (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [error]
                }),
                "failed",
                Vec::new(),
            ),
        }
    }

    fn b2b_company_contact_assign_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let roles = resolved_object_list_field(&field.arguments, "rolesToAssign");
        let mut assignments = Vec::new();
        let mut assignment_ids = Vec::new();
        let mut errors = Vec::new();
        if !self.b2b_contact_exists(&contact_id) {
            return (
                json!({
                    "roleAssignments": [],
                    "userErrors": [{
                        "field": ["companyContactId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        for (index, role) in roles.iter().enumerate() {
            let role_id = resolved_string_field(role, "companyContactRoleId").unwrap_or_default();
            let location_id = resolved_string_field(role, "companyLocationId").unwrap_or_default();
            match self.b2b_create_role_assignment(&contact_id, &role_id, &location_id, Some(index))
            {
                Ok(assignment_id) => {
                    if let Some(assignment) = self.b2b_role_assignment_materialized(&assignment_id)
                    {
                        assignments.push(assignment);
                    }
                    assignment_ids.push(assignment_id);
                }
                Err(error) => errors.push(error),
            }
        }
        let status = if assignment_ids.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "roleAssignments": assignments,
                "userErrors": errors
            }),
            status,
            assignment_ids,
        )
    }

    fn b2b_company_contact_revoke_role_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let assignment_id = resolved_string_arg(&field.arguments, "companyContactRoleAssignmentId")
            .or_else(|| resolved_string_arg(&field.arguments, "roleAssignmentId"))
            .unwrap_or_default();
        match self.b2b_revoke_role_assignment(&contact_id, &assignment_id, None) {
            Ok(id) => (
                json!({
                    "revokedCompanyContactRoleAssignmentId": id,
                    "userErrors": []
                }),
                "staged",
                vec![id],
            ),
            Err(error) => (
                json!({
                    "revokedCompanyContactRoleAssignmentId": Value::Null,
                    "userErrors": [error]
                }),
                "failed",
                Vec::new(),
            ),
        }
    }

    fn b2b_company_contact_revoke_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let assignment_ids =
            resolved_string_list_field_unsorted(&field.arguments, "roleAssignmentIds");
        let mut revoked = Vec::new();
        let mut errors = Vec::new();
        for (index, assignment_id) in assignment_ids.iter().enumerate() {
            match self.b2b_revoke_role_assignment(&contact_id, assignment_id, Some(index)) {
                Ok(id) => revoked.push(id),
                Err(error) => errors.push(error),
            }
        }
        let status = if revoked.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "revokedRoleAssignmentIds": revoked,
                "userErrors": errors
            }),
            status,
            assignment_ids,
        )
    }

    fn b2b_company_location_assign_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id =
            resolved_string_arg(&field.arguments, "companyLocationId").unwrap_or_default();
        let roles = resolved_object_list_field(&field.arguments, "rolesToAssign");
        let mut assignments = Vec::new();
        let mut assignment_ids = Vec::new();
        let mut errors = Vec::new();
        if self
            .b2b_company_location_materialized(&location_id)
            .is_none()
        {
            return (
                json!({
                    "roleAssignments": [],
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        for (index, role) in roles.iter().enumerate() {
            let contact_id = resolved_string_field(role, "companyContactId").unwrap_or_default();
            let role_id = resolved_string_field(role, "companyContactRoleId").unwrap_or_default();
            match self.b2b_create_role_assignment(&contact_id, &role_id, &location_id, Some(index))
            {
                Ok(assignment_id) => {
                    if let Some(assignment) = self.b2b_role_assignment_materialized(&assignment_id)
                    {
                        assignments.push(assignment);
                    }
                    assignment_ids.push(assignment_id);
                }
                Err(error) => {
                    let field = error
                        .get("field")
                        .and_then(Value::as_array)
                        .and_then(|path| path.get(1))
                        .and_then(Value::as_str)
                        .map(|_| json!(["rolesToAssign", index.to_string()]))
                        .unwrap_or_else(|| json!(["rolesToAssign", index.to_string()]));
                    let message = if !self.b2b_contact_exists(&contact_id) {
                        "Company contact does not exist."
                    } else if self.b2b_contact_role_materialized(&role_id).is_none() {
                        "Company role does not exist."
                    } else {
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Resource requested does not exist.")
                    };
                    errors.push(json!({
                        "field": field,
                        "message": message,
                        "code": error.get("code").cloned().unwrap_or_else(|| json!("RESOURCE_NOT_FOUND"))
                    }));
                }
            }
        }
        let status = if assignment_ids.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "roleAssignments": assignments,
                "userErrors": errors
            }),
            status,
            assignment_ids,
        )
    }

    fn b2b_company_location_revoke_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id =
            resolved_string_arg(&field.arguments, "companyLocationId").unwrap_or_default();
        let assignment_ids = resolved_string_list_field_unsorted(&field.arguments, "rolesToRevoke");
        let mut revoked = Vec::new();
        let mut errors = Vec::new();
        for (index, assignment_id) in assignment_ids.iter().enumerate() {
            let exists = self
                .store
                .staged
                .b2b_contact_role_assignments
                .get(assignment_id)
                .is_some_and(|assignment| {
                    assignment.get("companyLocationId").and_then(Value::as_str)
                        == Some(location_id.as_str())
                });
            if exists {
                self.store
                    .staged
                    .b2b_contact_role_assignments
                    .remove(assignment_id);
                self.store
                    .staged
                    .deleted_b2b_contact_role_assignment_ids
                    .insert(assignment_id.clone());
                revoked.push(assignment_id.clone());
            } else {
                errors.push(json!({
                    "field": ["rolesToRevoke", index.to_string()],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }));
            }
        }
        let status = if revoked.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "revokedRoleAssignmentIds": revoked,
                "userErrors": errors
            }),
            status,
            assignment_ids,
        )
    }

    fn b2b_company_location_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id =
            resolved_string_arg(&field.arguments, "companyLocationId").unwrap_or_default();
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
            return (
                b2b_company_location_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyLocationId"],
                        "Resource requested does not exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(name) = resolved_string_field(&input, "name") {
            let stripped = b2b_strip_html_tags(&name);
            if stripped.trim().is_empty() {
                return (
                    b2b_company_location_payload(
                        None,
                        vec![b2b_company_user_error(
                            vec!["input", "name"],
                            "Name can't be blank",
                            "BLANK",
                            None,
                        )],
                    ),
                    "failed",
                    Vec::new(),
                );
            }
            location["name"] = json!(stripped);
        }
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

    fn b2b_company_address_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let address_id = resolved_string_arg(&field.arguments, "addressId").unwrap_or_default();
        for location in self.store.staged.b2b_locations.values_mut() {
            if location
                .get("billingAddress")
                .and_then(|address| address.get("id"))
                .and_then(Value::as_str)
                == Some(address_id.as_str())
            {
                location["billingAddress"] = Value::Null;
                return (
                    json!({
                        "deletedAddressId": address_id,
                        "userErrors": []
                    }),
                    "staged",
                    vec![address_id],
                );
            }
        }
        (
            json!({
                "deletedAddressId": Value::Null,
                "userErrors": [{
                    "field": ["addressId"],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }]
            }),
            "failed",
            Vec::new(),
        )
    }

    fn b2b_company_location_assign_address_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "locationId").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let Some(location) = self.store.staged.b2b_locations.get_mut(&location_id) else {
            return (
                json!({
                    "addresses": Value::Null,
                    "userErrors": [{
                        "field": ["locationId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        };
        let address = b2b_company_address_json(&location_id, &address_input);
        location["billingAddress"] = address.clone();
        (
            json!({
                "addresses": [address.clone()],
                "userErrors": []
            }),
            "staged",
            vec![
                location_id,
                address
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ],
        )
    }

    fn b2b_contact_from_input(
        &self,
        id: String,
        company_id: String,
        input: &BTreeMap<String, ResolvedValue>,
        customer_id: String,
    ) -> Value {
        let first_name = resolved_string_field(input, "firstName").unwrap_or_default();
        let last_name = resolved_string_field(input, "lastName").unwrap_or_default();
        let email = resolved_string_field(input, "email").unwrap_or_default();
        let phone = resolved_string_field(input, "phone").map(|phone| b2b_normalize_phone(&phone));
        let title = resolved_string_field(input, "title").unwrap_or_default();
        let customer = b2b_contact_customer_json(
            &customer_id,
            &first_name,
            &last_name,
            &email,
            phone.as_deref(),
        );
        json!({
            "id": id,
            "companyId": company_id,
            "customerId": customer_id,
            "title": title,
            "locale": resolved_string_field(input, "locale").map(Value::String).unwrap_or(Value::Null),
            "customer": customer,
            "isMainContact": false,
            "roleAssignments": connection_json(Vec::new())
        })
    }

    fn b2b_contact_from_customer(&self, id: String, company_id: String, customer: &Value) -> Value {
        let customer_id = customer
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        json!({
            "id": id,
            "companyId": company_id,
            "customerId": customer_id,
            "title": Value::Null,
            "locale": customer.get("locale").cloned().unwrap_or(Value::Null),
            "customer": customer.clone(),
            "isMainContact": false,
            "roleAssignments": connection_json(Vec::new())
        })
    }

    fn apply_b2b_contact_input(
        &self,
        contact: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        if input.contains_key("title") {
            contact["title"] = resolved_string_field(input, "title")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("locale") {
            contact["locale"] = resolved_string_field(input, "locale")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        let mut customer = contact.get("customer").cloned().unwrap_or(Value::Null);
        for (input_field, customer_field) in [
            ("firstName", "firstName"),
            ("lastName", "lastName"),
            ("email", "email"),
            ("phone", "phone"),
        ] {
            if input.contains_key(input_field) {
                customer[customer_field] = resolved_string_field(input, input_field)
                    .map(|value| {
                        if input_field == "phone" {
                            b2b_normalize_phone(&value)
                        } else {
                            value
                        }
                    })
                    .map(Value::String)
                    .unwrap_or(Value::Null);
            }
        }
        let first = customer
            .get("firstName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let last = customer
            .get("lastName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        customer["displayName"] = json!(b2b_display_name(first, last));
        contact["customer"] = customer;
    }

    fn b2b_company_materialized(&self, company_id: &str) -> Option<Value> {
        let mut company = self.store.staged.b2b_companies.get(company_id)?.clone();
        let contacts = self.b2b_contacts_for_company(company_id);
        let main_contact = company
            .get("mainContactId")
            .and_then(Value::as_str)
            .and_then(|id| self.b2b_contact_materialized(id))
            .unwrap_or(Value::Null);
        let default_role = self
            .b2b_contact_roles_for_company(company_id)
            .into_iter()
            .find(|role| {
                role.get("default")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| b2b_default_contact_role(company_id));
        company["mainContact"] = main_contact;
        company["contacts"] = connection_json(contacts.clone());
        company["contactsCount"] = json!({ "count": contacts.len(), "precision": "EXACT" });
        let locations = self.b2b_locations_for_company(company_id);
        company["locations"] = connection_json(locations.clone());
        company["locationsCount"] = json!({ "count": locations.len(), "precision": "EXACT" });
        company["contactRoles"] = connection_json(self.b2b_contact_roles_for_company(company_id));
        company["defaultRole"] = default_role;
        Some(company)
    }

    fn b2b_contact_materialized(&self, contact_id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_b2b_contact_ids
            .contains(contact_id)
        {
            return None;
        }
        let mut contact = self.store.staged.b2b_contacts.get(contact_id)?.clone();
        let company_id = contact
            .get("companyId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        contact["company"] = self.b2b_company_summary(&company_id).unwrap_or(Value::Null);
        let is_main = self
            .store
            .staged
            .b2b_companies
            .get(&company_id)
            .and_then(|company| company.get("mainContactId"))
            .and_then(Value::as_str)
            == Some(contact_id);
        contact["isMainContact"] = json!(is_main);
        contact["roleAssignments"] =
            connection_json(self.b2b_role_assignments_for_contact(contact_id));
        Some(contact)
    }

    fn b2b_contact_role_materialized(&self, role_id: &str) -> Option<Value> {
        self.store
            .staged
            .b2b_contact_roles
            .get(role_id)
            .cloned()
            .or_else(|| {
                self.store
                    .staged
                    .b2b_companies
                    .keys()
                    .map(|company_id| b2b_default_contact_role(company_id))
                    .find(|role| role.get("id").and_then(Value::as_str) == Some(role_id))
            })
    }

    fn b2b_company_location_materialized(&self, location_id: &str) -> Option<Value> {
        let mut location = self
            .store
            .staged
            .b2b_locations
            .get(location_id)
            .cloned()
            .or_else(|| {
                (location_id == b2b_synthetic_seed_company_location_id())
                    .then(|| b2b_synthetic_seed_company_location(location_id))
            })?;
        location["roleAssignments"] =
            connection_json(self.b2b_role_assignments_for_location(location_id));
        if let Some(company_id) = location.get("companyId").and_then(Value::as_str) {
            location["company"] = self.b2b_company_summary(company_id).unwrap_or(Value::Null);
        }
        Some(location)
    }

    fn b2b_role_assignment_materialized(&self, assignment_id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_b2b_contact_role_assignment_ids
            .contains(assignment_id)
        {
            return None;
        }
        let mut assignment = self
            .store
            .staged
            .b2b_contact_role_assignments
            .get(assignment_id)?
            .clone();
        let contact_id = assignment
            .get("companyContactId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let role_id = assignment
            .get("companyContactRoleId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let location_id = assignment
            .get("companyLocationId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        assignment["companyContact"] = self.b2b_contact_summary(&contact_id).unwrap_or(Value::Null);
        assignment["role"] = self
            .b2b_contact_role_materialized(&role_id)
            .unwrap_or(Value::Null);
        assignment["companyLocation"] = self
            .b2b_company_location_summary(&location_id)
            .unwrap_or(Value::Null);
        Some(assignment)
    }

    fn b2b_node_materialized(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("CompanyContact") => self.b2b_contact_materialized(id),
            Some("CompanyContactRole") => self.b2b_contact_role_materialized(id),
            Some("CompanyContactRoleAssignment") => self.b2b_role_assignment_materialized(id),
            Some("CompanyLocation") => self.b2b_company_location_materialized(id),
            Some("CompanyAddress") => self.b2b_company_address_materialized(id),
            _ => None,
        }
    }

    pub(in crate::proxy) fn b2b_company_node_for_id(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .b2b_companies
            .get(id)
            .and_then(|_| self.b2b_company_materialized(id))
            .or_else(|| {
                self.store
                    .staged
                    .b2b_locations
                    .get(id)
                    .and_then(|location| {
                        location
                            .get("companyId")
                            .and_then(Value::as_str)
                            .or_else(|| location["company"]["id"].as_str())
                    })
                    .and_then(|company_id| self.b2b_company_materialized(company_id))
            })
    }

    fn b2b_company_address_materialized(&self, address_id: &str) -> Option<Value> {
        self.store
            .staged
            .b2b_locations
            .values()
            .filter_map(|location| location.get("billingAddress"))
            .find(|address| address.get("id").and_then(Value::as_str) == Some(address_id))
            .cloned()
    }

    fn b2b_company_summary(&self, company_id: &str) -> Option<Value> {
        let company = self.store.staged.b2b_companies.get(company_id)?;
        Some(json!({
            "id": company.get("id").cloned().unwrap_or_else(|| json!(company_id)),
            "name": company.get("name").cloned().unwrap_or(Value::Null)
        }))
    }

    fn b2b_contact_summary(&self, contact_id: &str) -> Option<Value> {
        let contact = self.store.staged.b2b_contacts.get(contact_id)?;
        Some(json!({
            "id": contact.get("id").cloned().unwrap_or_else(|| json!(contact_id)),
            "title": contact.get("title").cloned().unwrap_or(Value::Null)
        }))
    }

    fn b2b_company_location_summary(&self, location_id: &str) -> Option<Value> {
        let location = self
            .store
            .staged
            .b2b_locations
            .get(location_id)
            .cloned()
            .or_else(|| {
                (location_id == b2b_synthetic_seed_company_location_id())
                    .then(|| b2b_synthetic_seed_company_location(location_id))
            })?;
        Some(json!({
            "id": location.get("id").cloned().unwrap_or_else(|| json!(location_id)),
            "name": location.get("name").cloned().unwrap_or(Value::Null)
        }))
    }

    fn b2b_contacts_for_company(&self, company_id: &str) -> Vec<Value> {
        let ordered_ids = self
            .store
            .staged
            .b2b_companies
            .get(company_id)
            .and_then(|company| company.get("contactIds"))
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut contacts = Vec::new();
        for contact_id in ordered_ids {
            if let Some(contact) = self.b2b_contact_materialized(&contact_id) {
                contacts.push(contact);
            }
        }
        for contact in self.store.staged.b2b_contacts.values() {
            let Some(contact_id) = contact.get("id").and_then(Value::as_str) else {
                continue;
            };
            if contact.get("companyId").and_then(Value::as_str) == Some(company_id)
                && !contacts
                    .iter()
                    .any(|existing| existing.get("id").and_then(Value::as_str) == Some(contact_id))
            {
                if let Some(materialized) = self.b2b_contact_materialized(contact_id) {
                    contacts.push(materialized);
                }
            }
        }
        contacts
    }

    fn b2b_locations_for_company(&self, company_id: &str) -> Vec<Value> {
        let ordered_ids = self
            .store
            .staged
            .b2b_companies
            .get(company_id)
            .and_then(|company| company.get("locationIds"))
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut locations = Vec::new();
        for location_id in ordered_ids {
            if let Some(location) = self.b2b_company_location_materialized(&location_id) {
                locations.push(location);
            }
        }
        for location in self.store.staged.b2b_locations.values() {
            let Some(location_id) = location.get("id").and_then(Value::as_str) else {
                continue;
            };
            if location.get("companyId").and_then(Value::as_str) == Some(company_id)
                && !locations
                    .iter()
                    .any(|existing| existing.get("id").and_then(Value::as_str) == Some(location_id))
            {
                if let Some(materialized) = self.b2b_company_location_materialized(location_id) {
                    locations.push(materialized);
                }
            }
        }
        locations
    }

    fn b2b_contact_roles_for_company(&self, company_id: &str) -> Vec<Value> {
        let mut roles = self
            .store
            .staged
            .b2b_contact_roles
            .values()
            .filter(|role| role.get("companyId").and_then(Value::as_str) == Some(company_id))
            .cloned()
            .collect::<Vec<_>>();
        if roles.is_empty() {
            roles.push(b2b_default_contact_role(company_id));
        }
        roles.sort_by_key(|role| {
            match role.get("name").and_then(Value::as_str).unwrap_or_default() {
                "Location admin" => 0,
                "Ordering only" => 1,
                _ => 2,
            }
        });
        roles
    }

    fn b2b_role_assignments_for_contact(&self, contact_id: &str) -> Vec<Value> {
        self.store
            .staged
            .b2b_contact_role_assignments
            .values()
            .filter(|assignment| {
                assignment.get("companyContactId").and_then(Value::as_str) == Some(contact_id)
            })
            .filter_map(|assignment| {
                assignment
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.b2b_role_assignment_materialized(id))
            })
            .collect()
    }

    fn b2b_role_assignments_for_location(&self, location_id: &str) -> Vec<Value> {
        self.store
            .staged
            .b2b_contact_role_assignments
            .values()
            .filter(|assignment| {
                assignment.get("companyLocationId").and_then(Value::as_str) == Some(location_id)
            })
            .filter_map(|assignment| {
                assignment
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.b2b_role_assignment_materialized(id))
            })
            .collect()
    }

    fn ensure_b2b_contact_role(&mut self, company_id: &str, name: &str, default: bool) -> Value {
        let role = b2b_contact_role(company_id, name, default);
        let id = role
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .b2b_contact_roles
            .entry(id)
            .or_insert_with(|| role.clone())
            .clone()
    }

    fn stage_b2b_company_location(
        &mut self,
        company_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> String {
        let id = synthetic_shopify_gid("CompanyLocation", self.store.staged.next_b2b_company_id);
        self.store.staged.next_b2b_company_id += 1;
        let fallback_name = self
            .store
            .staged
            .b2b_companies
            .get(company_id)
            .and_then(|company| company.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("HQ")
            .to_string();
        let name = resolved_string_field(input, "name")
            .map(|name| b2b_strip_html_tags(&name))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(fallback_name);
        let billing_address = resolved_object_field(input, "billingAddress")
            .map(|address| b2b_company_address_json(&id, &address))
            .unwrap_or(Value::Null);
        let location = json!({
            "id": id,
            "companyId": company_id,
            "name": name,
            "note": resolved_string_field(input, "note").map(Value::String).unwrap_or(Value::Null),
            "phone": resolved_string_field(input, "phone").map(Value::String).unwrap_or(Value::Null),
            "billingAddress": billing_address,
            "taxSettings": {
                "taxRegistrationId": Value::Null,
                "taxExempt": false,
                "taxExemptions": []
            },
            "roleAssignments": connection_json(Vec::new())
        });
        self.store.staged.b2b_locations.insert(id.clone(), location);
        if let Some(company) = self.store.staged.b2b_companies.get_mut(company_id) {
            let mut ids = company
                .get("locationIds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !ids.iter().any(|existing| existing.as_str() == Some(&id)) {
                ids.push(json!(id.clone()));
            }
            company["locationIds"] = Value::Array(ids);
        }
        id
    }

    fn append_b2b_company_contact(&mut self, company_id: &str, contact_id: &str) {
        let Some(company) = self.store.staged.b2b_companies.get_mut(company_id) else {
            return;
        };
        let mut ids = company
            .get("contactIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !ids.iter().any(|id| id.as_str() == Some(contact_id)) {
            ids.push(json!(contact_id));
        }
        company["contactIds"] = Value::Array(ids);
    }

    fn remove_b2b_company_contact(&mut self, company_id: &str, contact_id: &str) {
        let Some(company) = self.store.staged.b2b_companies.get_mut(company_id) else {
            return;
        };
        if company.get("mainContactId").and_then(Value::as_str) == Some(contact_id) {
            company["mainContactId"] = Value::Null;
        }
        let ids = company
            .get("contactIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|id| id.as_str() != Some(contact_id))
            .collect::<Vec<_>>();
        company["contactIds"] = Value::Array(ids);
    }

    fn b2b_contact_exists(&self, contact_id: &str) -> bool {
        self.store.staged.b2b_contacts.contains_key(contact_id)
            && !self
                .store
                .staged
                .deleted_b2b_contact_ids
                .contains(contact_id)
    }

    fn b2b_delete_contact(&mut self, contact_id: &str) -> bool {
        if !self.b2b_contact_exists(contact_id) {
            return false;
        }
        let company_id = self
            .store
            .staged
            .b2b_contacts
            .get(contact_id)
            .and_then(|contact| contact.get("companyId"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();
        self.remove_b2b_company_contact(&company_id, contact_id);
        self.store.staged.b2b_contacts.remove(contact_id);
        self.store
            .staged
            .deleted_b2b_contact_ids
            .insert(contact_id.to_string());
        let assignment_ids = self
            .store
            .staged
            .b2b_contact_role_assignments
            .values()
            .filter(|assignment| {
                assignment.get("companyContactId").and_then(Value::as_str) == Some(contact_id)
            })
            .filter_map(|assignment| assignment.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        for assignment_id in assignment_ids {
            self.store
                .staged
                .b2b_contact_role_assignments
                .remove(&assignment_id);
            self.store
                .staged
                .deleted_b2b_contact_role_assignment_ids
                .insert(assignment_id);
        }
        true
    }

    fn b2b_create_role_assignment(
        &mut self,
        contact_id: &str,
        role_id: &str,
        location_id: &str,
        bulk_index: Option<usize>,
    ) -> Result<String, Value> {
        if !self.b2b_contact_exists(contact_id) {
            return Err(b2b_company_user_error(
                vec!["companyContactId"],
                "Resource requested does not exist.",
                "RESOURCE_NOT_FOUND",
                None,
            ));
        }
        if self
            .b2b_company_location_materialized(location_id)
            .is_none()
        {
            return Err(b2b_indexed_role_assignment_error(
                bulk_index,
                "companyLocationId",
            ));
        }
        if self.b2b_contact_role_materialized(role_id).is_none() {
            return Err(b2b_indexed_role_assignment_error(
                bulk_index,
                "companyContactRoleId",
            ));
        }
        let duplicate = self
            .store
            .staged
            .b2b_contact_role_assignments
            .values()
            .any(|assignment| {
                assignment.get("companyContactId").and_then(Value::as_str) == Some(contact_id)
                    && assignment
                        .get("companyContactRoleId")
                        .and_then(Value::as_str)
                        == Some(role_id)
                    && assignment.get("companyLocationId").and_then(Value::as_str)
                        == Some(location_id)
            });
        if duplicate {
            return Err(json!({
                "field": Value::Null,
                "message": "Contact already has this role at this location.",
                "code": "LIMIT_REACHED"
            }));
        }
        let id = synthetic_shopify_gid(
            "CompanyContactRoleAssignment",
            self.store.staged.next_b2b_contact_role_assignment_id,
        );
        self.store.staged.next_b2b_contact_role_assignment_id += 1;
        self.store
            .staged
            .deleted_b2b_contact_role_assignment_ids
            .remove(&id);
        self.store.staged.b2b_contact_role_assignments.insert(
            id.clone(),
            json!({
                "id": id,
                "companyContactId": contact_id,
                "companyContactRoleId": role_id,
                "companyLocationId": location_id
            }),
        );
        Ok(id)
    }

    fn b2b_revoke_role_assignment(
        &mut self,
        contact_id: &str,
        assignment_id: &str,
        bulk_index: Option<usize>,
    ) -> Result<String, Value> {
        let exists = self
            .store
            .staged
            .b2b_contact_role_assignments
            .get(assignment_id)
            .is_some_and(|assignment| {
                assignment.get("companyContactId").and_then(Value::as_str) == Some(contact_id)
            });
        if !exists {
            let field = bulk_index
                .map(|index| json!(["roleAssignmentIds", index.to_string()]))
                .unwrap_or_else(|| json!(["companyContactRoleAssignmentId"]));
            return Err(json!({
                "field": field,
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }));
        }
        self.store
            .staged
            .b2b_contact_role_assignments
            .remove(assignment_id);
        self.store
            .staged
            .deleted_b2b_contact_role_assignment_ids
            .insert(assignment_id.to_string());
        Ok(assignment_id.to_string())
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
                if operation_type == OperationType::Query
                    && root_fields.iter().any(|field| {
                        matches!(
                            field.as_str(),
                            "node" | "nodes" | "product" | "products" | "productVariant"
                        )
                    })
                {
                    self.observe_product_passthrough_response(&response);
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
            "customerByIdentifier" => !self.customer_effective_records().is_empty(),
            "customers" | "customersCount" => {
                !self.store.staged.customers.is_empty()
                    || !self.store.staged.deleted_customer_ids.is_empty()
            }
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
                "customers" => Some(self.customer_connection_field(field)),
                "customersCount" => Some(selected_json(
                    &json!({ "count": 177usize.saturating_sub(self.store.staged.deleted_customer_ids.len()), "precision": "EXACT" }),
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
            Some(ResolvedValue::String(email)) => self
                .customer_effective_records()
                .into_iter()
                .find(|customer| {
                    customer.get("email").and_then(Value::as_str) == Some(email.as_str())
                }),
            _ if matches!(
                identifier.get("emailAddress"),
                Some(ResolvedValue::String(_))
            ) =>
            {
                let Some(ResolvedValue::String(email)) = identifier.get("emailAddress") else {
                    return Value::Null;
                };
                self.customer_effective_records()
                    .into_iter()
                    .find(|customer| {
                        customer.get("email").and_then(Value::as_str) == Some(email.as_str())
                            || customer["defaultEmailAddress"]["emailAddress"].as_str()
                                == Some(email.as_str())
                    })
            }
            _ => match identifier.get("id") {
                Some(ResolvedValue::String(id)) => self
                    .store
                    .staged
                    .customers
                    .get(id)
                    .filter(|_| !self.store.staged.deleted_customer_ids.contains(id))
                    .cloned(),
                _ => match identifier.get("phone") {
                    Some(ResolvedValue::String(phone)) => self
                        .customer_effective_records()
                        .into_iter()
                        .find(|customer| {
                            customer.get("phone").and_then(Value::as_str) == Some(phone.as_str())
                        }),
                    _ if matches!(
                        identifier.get("phoneNumber"),
                        Some(ResolvedValue::String(_))
                    ) =>
                    {
                        let Some(ResolvedValue::String(phone)) = identifier.get("phoneNumber")
                        else {
                            return Value::Null;
                        };
                        self.customer_effective_records()
                            .into_iter()
                            .find(|customer| {
                                customer.get("phone").and_then(Value::as_str)
                                    == Some(phone.as_str())
                                    || customer["defaultPhoneNumber"]["phoneNumber"].as_str()
                                        == Some(phone.as_str())
                            })
                    }
                    _ => None,
                },
            },
        };
        customer
            .as_ref()
            .map(|customer| selected_json(customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_connection_field(&self, field: &RootFieldSelection) -> Value {
        selected_connection_json_with_args(
            self.customer_effective_records()
                .into_iter()
                .filter(|customer| customer_matches_search_query(customer, &field.arguments))
                .collect(),
            &field.arguments,
            &field.selection,
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn customer_effective_records(&self) -> Vec<Value> {
        self.store
            .staged
            .customers
            .iter()
            .filter(|(id, _)| !self.store.staged.deleted_customer_ids.contains(*id))
            .map(|(_, customer)| customer.clone())
            .collect()
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

    pub(in crate::proxy) fn customer_merge_erasure_mutation_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if operation_type != OperationType::Mutation
            || parsed_root_fields.is_empty()
            || !parsed_root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "customerMerge" | "customerRequestDataErasure" | "customerCancelDataErasure"
                )
            })
        {
            return None;
        }

        let fields = root_fields(query, variables)?;
        if let Some(errors) = customer_merge_top_level_errors(query, variables, &fields) {
            return Some(ok_json(json!({ "errors": errors })));
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, status, staged_ids) = match field.name.as_str() {
                "customerMerge" => self.customer_merge_payload(&field),
                "customerRequestDataErasure" => self.customer_data_erasure_payload(&field, true),
                "customerCancelDataErasure" => self.customer_data_erasure_payload(&field, false),
                _ => return None,
            };
            self.record_customer_side_effect_log(
                request,
                query,
                variables,
                &field.name,
                staged_ids,
                status,
            );
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn customer_merge_query_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
    ) -> Option<Response> {
        if operation_type != OperationType::Query || parsed_root_fields.is_empty() {
            return None;
        }
        let fields = root_fields(query, variables)?;
        let has_customer_merge_job_reference = self.has_customer_merge_job_reference(&fields);
        if parsed_root_fields.iter().all(|field| {
            matches!(
                field.as_str(),
                "customer"
                    | "customers"
                    | "customersCount"
                    | "customerByIdentifier"
                    | "customerMergeJobStatus"
                    | "job"
                    | "node"
            )
        }) && (parsed_root_fields
            .iter()
            .any(|field| field == "customerMergeJobStatus")
            || has_customer_merge_job_reference
            || self.should_handle_customer_overlay_read(query, &fields))
        {
            let mut data = serde_json::Map::new();
            for field in fields {
                let value = match field.name.as_str() {
                    "customer" => self.customer_read_field(&field),
                    "customerByIdentifier" => self.customer_by_identifier_field(&field),
                    "customers" => self.customer_connection_field(&field),
                    "customersCount" => selected_json(
                        &json!({ "count": 177usize.saturating_sub(self.store.staged.deleted_customer_ids.len()), "precision": "EXACT" }),
                        &field.selection,
                    ),
                    "customerMergeJobStatus" => self.customer_merge_job_status_field(&field),
                    "job" => self.customer_merge_job_node_field(&field),
                    "node" if customer_merge_job_node_field(&field) => {
                        self.customer_merge_job_node_field(&field)
                    }
                    "node" => return None,
                    _ => return None,
                };
                data.insert(field.response_key.clone(), value);
            }
            return Some(ok_json(json!({ "data": Value::Object(data) })));
        }
        if parsed_root_fields
            .iter()
            .all(|field| field == "customerMergeJobStatus")
        {
            let mut data = serde_json::Map::new();
            for field in fields {
                data.insert(
                    field.response_key.clone(),
                    self.customer_merge_job_status_field(&field),
                );
            }
            return Some(ok_json(json!({ "data": Value::Object(data) })));
        }
        if parsed_root_fields
            .iter()
            .all(|field| matches!(field.as_str(), "job" | "node"))
        {
            if !has_customer_merge_job_reference {
                return None;
            }
            let data = self.customer_merge_job_node_fields(&fields)?;
            return Some(ok_json(json!({ "data": data })));
        }
        None
    }

    fn customer_merge_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let one_id = resolved_string_arg(&field.arguments, "customerOneId").unwrap_or_default();
        let two_id = resolved_string_arg(&field.arguments, "customerTwoId").unwrap_or_default();
        if one_id == two_id {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Customers IDs should not match",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if let Some(error) = self.customer_merge_unknown_error(&one_id, "customerOneId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                "failed",
                Vec::new(),
            );
        }
        if let Some(error) = self.customer_merge_unknown_error(&two_id, "customerTwoId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                "failed",
                Vec::new(),
            );
        }

        let blocker_errors = self.customer_merge_blocker_errors(&one_id, &two_id);
        if !blocker_errors.is_empty() {
            return (
                customer_merge_payload_json(None, None, blocker_errors),
                "failed",
                Vec::new(),
            );
        }

        let result_id = two_id.clone();
        let source_id = one_id.clone();
        let mut result = self
            .store
            .staged
            .customers
            .get(&result_id)
            .cloned()
            .unwrap_or(Value::Null);
        let source = self
            .store
            .staged
            .customers
            .get(&source_id)
            .cloned()
            .unwrap_or(Value::Null);
        let override_fields =
            resolved_object_field(&field.arguments, "overrideFields").unwrap_or_default();
        apply_customer_merge_overrides(&mut result, &source, &override_fields);
        normalize_merged_customer_defaults(&mut result);

        self.store
            .staged
            .customers
            .insert(result_id.clone(), result);
        self.store.staged.customers.remove(&source_id);
        self.store
            .staged
            .deleted_customer_ids
            .insert(source_id.clone());
        self.store
            .staged
            .merged_customer_ids
            .insert(source_id.clone(), result_id.clone());
        if let Some(source_orders) = self.store.staged.customer_orders.remove(&source_id) {
            self.store
                .staged
                .customer_orders
                .entry(result_id.clone())
                .or_default()
                .extend(source_orders);
        }

        let job_id = self.next_proxy_synthetic_gid("Job");
        let request = customer_merge_request_json(&job_id, &result_id, Vec::new());
        self.store
            .staged
            .customer_merge_requests
            .insert(job_id.clone(), request);
        (
            customer_merge_payload_json(Some(result_id.as_str()), Some(&job_id), Vec::new()),
            "staged",
            vec![source_id, result_id, job_id],
        )
    }

    fn customer_data_erasure_payload(
        &mut self,
        field: &RootFieldSelection,
        request_erasure: bool,
    ) -> (Value, &'static str, Vec<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        if !self.customer_exists(&customer_id) {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer does not exist",
                        "DOES_NOT_EXIST",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if request_erasure {
            self.store.staged.customer_data_erasure_requests.insert(
                customer_id.clone(),
                json!({ "customerId": customer_id, "status": "REQUESTED" }),
            );
            return (
                customer_data_erasure_payload_json(Some(&customer_id), Vec::new()),
                "staged",
                vec![customer_id],
            );
        }
        let is_pending = self
            .store
            .staged
            .customer_data_erasure_requests
            .get(&customer_id)
            .and_then(|request| request["status"].as_str())
            == Some("REQUESTED");
        if !is_pending {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer's data is not scheduled for erasure",
                        "NOT_BEING_ERASED",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        self.store.staged.customer_data_erasure_requests.insert(
            customer_id.clone(),
            json!({ "customerId": customer_id, "status": "CANCELED" }),
        );
        (
            customer_data_erasure_payload_json(Some(&customer_id), Vec::new()),
            "staged",
            vec![customer_id],
        )
    }

    fn customer_merge_unknown_error(&self, id: &str, field: &str) -> Option<Value> {
        if self.customer_exists(id) {
            return None;
        }
        Some(customer_merge_user_error(
            json!([field]),
            &format!("Customer does not exist with ID {}", resource_id_tail(id)),
            "INVALID_CUSTOMER_ID",
        ))
    }

    fn customer_exists(&self, id: &str) -> bool {
        !id.is_empty()
            && self.store.staged.customers.contains_key(id)
            && !self.store.staged.deleted_customer_ids.contains(id)
    }

    fn customer_merge_blocker_errors(&self, one_id: &str, two_id: &str) -> Vec<Value> {
        let one = self.store.staged.customers.get(one_id);
        let two = self.store.staged.customers.get(two_id);
        let mut errors = Vec::new();
        let combined_tags = one
            .into_iter()
            .chain(two)
            .flat_map(customer_tags)
            .collect::<BTreeSet<_>>();
        if combined_tags.len() > 250 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
        }
        let combined_note_len = one
            .and_then(|customer| customer["note"].as_str())
            .unwrap_or_default()
            .chars()
            .count()
            + two
                .and_then(|customer| customer["note"].as_str())
                .unwrap_or_default()
                .chars()
                .count();
        if combined_note_len > 5000 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
        }
        for (id, field_name) in [(one_id, "customerOneId"), (two_id, "customerTwoId")] {
            if self.customer_has_assigned_gift_card(id) {
                let name = self
                    .store
                    .staged
                    .customers
                    .get(id)
                    .and_then(|customer| customer["displayName"].as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Customer");
                errors.push(customer_merge_user_error(
                    json!([field_name]),
                    &format!("{name} has gift cards and can’t be merged."),
                    "INVALID_CUSTOMER",
                ));
            }
            if self.customer_has_subscription_contract(id) {
                errors.push(customer_merge_user_error(
                    json!([field_name]),
                    "Customers with subscription contracts can’t be merged.",
                    "INVALID_CUSTOMER",
                ));
            }
        }
        errors
    }

    fn customer_has_assigned_gift_card(&self, customer_id: &str) -> bool {
        self.store.staged.gift_cards.values().any(|card| {
            card["customer"]["id"].as_str() == Some(customer_id)
                || card["customerId"].as_str() == Some(customer_id)
        })
    }

    fn customer_has_subscription_contract(&self, customer_id: &str) -> bool {
        self.store
            .staged
            .customer_payment_method_customer_index
            .get(customer_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.store.staged.customer_payment_methods.get(id))
            .any(|method| {
                method["activeSubscriptionContracts"]["nodes"]
                    .as_array()
                    .is_some_and(|nodes| !nodes.is_empty())
            })
    }

    fn customer_merge_job_status_field(&self, field: &RootFieldSelection) -> Value {
        let job_id = resolved_string_arg(&field.arguments, "jobId").unwrap_or_default();
        self.store
            .staged
            .customer_merge_requests
            .get(&job_id)
            .map(|request| selected_json(request, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn customer_merge_job_node_fields(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut handled_any = false;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = self.customer_merge_job_node_field(field);
            if !value.is_null() {
                handled_any = true;
            }
            data.insert(field.response_key.clone(), value);
        }
        handled_any.then_some(Value::Object(data))
    }

    fn has_customer_merge_job_reference(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "customerMergeJobStatus" => resolved_string_arg(&field.arguments, "jobId")
                .as_deref()
                .is_some_and(|id| self.store.staged.customer_merge_requests.contains_key(id)),
            "job" | "node" => resolved_string_arg(&field.arguments, "id")
                .as_deref()
                .is_some_and(|id| self.store.staged.customer_merge_requests.contains_key(id)),
            _ => false,
        })
    }

    fn customer_merge_job_node_field(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        self.store
            .staged
            .customer_merge_requests
            .get(&id)
            .map(customer_merge_job_from_request)
            .map(|job| selected_json(&job, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn record_customer_side_effect_log(
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

    fn customer_owns_address(&self, customer_id: &str, address_id: &str) -> bool {
        self.store
            .staged
            .customer_address_owners
            .get(address_id)
            .is_some_and(|owner_id| owner_id == customer_id)
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

    fn next_customer_address_id(&mut self) -> String {
        let id = format!(
            "gid://shopify/MailingAddress/{}?model_name=CustomerAddress&shopify-draft-proxy=synthetic",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
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


    fn create_store_credit_account_for_owner(&mut self, owner_id: &str, currency: &str) -> String {
        let account_id = self.next_store_credit_account_gid();
        let owner = self.store_credit_owner_json(owner_id);
        let account = json!({
            "id": account_id,
            "balance": store_credit_money("0.0", currency),
            "owner": owner,
            "transactions": connection_json(Vec::new())
        });
        self.store
            .staged
            .store_credit_account_order
            .push(account_id.clone());
        self.store
            .staged
            .store_credit_accounts
            .insert(account_id.clone(), account);
        account_id
    }

    fn next_store_credit_account_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_account_id;
        self.store.staged.next_store_credit_account_id += 1;
        format!("gid://shopify/StoreCreditAccount/{id}?shopify-draft-proxy=synthetic")
    }

    fn next_store_credit_transaction_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_transaction_id;
        self.store.staged.next_store_credit_transaction_id += 1;
        format!("gid://shopify/StoreCreditAccountTransaction/{id}?shopify-draft-proxy=synthetic")
    }

    fn resolve_store_credit_account_id_for_mutation(
        &mut self,
        id: &str,
        currency: &str,
        allow_create: bool,
    ) -> Option<String> {
        match shopify_gid_resource_type(id) {
            Some("StoreCreditAccount") => self
                .store
                .staged
                .store_credit_accounts
                .contains_key(id)
                .then(|| id.to_string()),
            Some("Customer") | Some("CompanyLocation") => {
                if !self.store_credit_owner_exists(id) {
                    return None;
                }
                if let Some(account_id) =
                    self.store_credit_account_id_for_owner_currency(id, currency)
                {
                    return Some(account_id);
                }
                if allow_create {
                    Some(self.create_store_credit_account_for_owner(id, currency))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn selected_store_credit_account(&self, account: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "transactions" => {
                let account_id = account
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let transactions = self
                    .store
                    .staged
                    .store_credit_transaction_order
                    .iter()
                    .filter_map(|id| self.store.staged.store_credit_transactions.get(id))
                    .filter(|transaction| transaction["account"]["id"].as_str() == Some(account_id))
                    .cloned()
                    .collect::<Vec<_>>();
                Some(selected_connection_json_with_args(
                    transactions,
                    &field.arguments,
                    &field.selection,
                    value_id_cursor,
                ))
            }
            _ => selected_json(account, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn selected_store_credit_transaction(
        &self,
        transaction: &Value,
        selection: &[SelectedField],
    ) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "account" => transaction
                .get("account")
                .map(|account| self.selected_store_credit_account(account, &field.selection)),
            _ => selected_json(transaction, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn store_credit_account_id_for_owner_currency(
        &self,
        owner_id: &str,
        currency: &str,
    ) -> Option<String> {
        self.store
            .staged
            .store_credit_account_order
            .iter()
            .filter_map(|id| self.store.staged.store_credit_accounts.get(id))
            .find(|account| {
                account["owner"]["id"].as_str() == Some(owner_id)
                    && account["balance"]["currencyCode"].as_str() == Some(currency)
            })
            .and_then(|account| account["id"].as_str().map(str::to_string))
    }

    pub(in crate::proxy) fn store_credit_account_mutation(
        &mut self,
        root_field: &str,
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
            if !matches!(
                field.name.as_str(),
                "storeCreditAccountCredit" | "storeCreditAccountDebit"
            ) {
                continue;
            }
            let outcome = self.store_credit_account_mutation_field(&field);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        if data.is_empty() {
            return MutationOutcome::response(json_error(501, "Unsupported store credit mutation"));
        }
        let response = ok_json(json!({ "data": Value::Object(data) }));
        if log_drafts.is_empty() {
            MutationOutcome::response(response)
        } else if root_field == "storeCreditAccountCredit"
            || root_field == "storeCreditAccountDebit"
        {
            MutationOutcome::with_log_drafts(response, log_drafts)
        } else {
            MutationOutcome::response(response)
        }
    }

    fn store_credit_account_mutation_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let is_credit = field.name == "storeCreditAccountCredit";
        let input_name = if is_credit {
            "creditInput"
        } else {
            "debitInput"
        };
        let amount_name = if is_credit {
            "creditAmount"
        } else {
            "debitAmount"
        };
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let amount_input = resolved_object_field(&input, amount_name).unwrap_or_default();
        let currency = resolved_string_field(&amount_input, "currencyCode").unwrap_or_default();
        let amount_text = resolved_money_amount_text(&amount_input, "amount");
        let amount = amount_text
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);

        if amount <= 0.0 {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    if is_credit {
                        "A positive amount must be used to credit a store credit account"
                    } else {
                        "A positive amount must be used to debit a store credit account"
                    },
                    "NEGATIVE_OR_ZERO_AMOUNT",
                )],
            ));
        }
        if !store_credit_supported_currency(&currency) {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, amount_name, "currencyCode"],
                    "Currency is not supported",
                    "UNSUPPORTED_CURRENCY",
                )],
            ));
        }
        if is_credit
            && resolved_string_field(&input, "expiresAt")
                .as_deref()
                .map(store_credit_expires_at_in_past)
                .unwrap_or(false)
        {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, "expiresAt"],
                    "The expiry date must be in the future",
                    "EXPIRES_AT_IN_PAST",
                )],
            ));
        }

        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(account_id) =
            self.resolve_store_credit_account_id_for_mutation(&id, &currency, is_credit)
        else {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &["id"],
                    if shopify_gid_resource_type(&id) == Some("StoreCreditAccount") {
                        "Store credit account does not exist"
                    } else {
                        "Owner does not exist"
                    },
                    "NOT_FOUND",
                )],
            ));
        };

        let Some(existing) = self
            .store
            .staged
            .store_credit_accounts
            .get(&account_id)
            .cloned()
        else {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &["id"],
                    "Store credit account does not exist",
                    "NOT_FOUND",
                )],
            ));
        };
        let account_currency = existing["balance"]["currencyCode"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if currency != account_currency {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, amount_name, "currencyCode"],
                    "The currency provided does not match the currency of the store credit account",
                    "MISMATCHING_CURRENCY",
                )],
            ));
        }

        let current_balance = existing["balance"]["amount"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        if is_credit && current_balance + amount >= STORE_CREDIT_LIMIT {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    "The operation would cause the account's credit limit to be exceeded",
                    "CREDIT_LIMIT_EXCEEDED",
                )],
            ));
        }
        if !is_credit && amount > current_balance {
            return MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
                &field.selection,
                &field.name,
                None,
                vec![store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    "The store credit account does not have sufficient funds to satisfy the request",
                    "INSUFFICIENT_FUNDS",
                )],
            ));
        }

        let delta = if is_credit { amount } else { -amount };
        let balance_after = current_balance + delta;
        let amount_display = shopify_decimal_text(delta);
        let balance_display = shopify_decimal_text(balance_after);
        let transaction_id = self.next_store_credit_transaction_gid();
        let mut account = existing;
        account["balance"] = store_credit_money(&balance_display, &currency);
        let transaction = json!({
            "id": transaction_id,
            "__typename": if is_credit { "StoreCreditAccountCreditTransaction" } else { "StoreCreditAccountDebitTransaction" },
            "amount": store_credit_money(&amount_display, &currency),
            "balanceAfterTransaction": store_credit_money(&balance_display, &currency),
            "createdAt": "2026-04-25T01:41:06Z",
            "event": "ADJUSTMENT",
            "origin": Value::Null,
            "notify": resolved_bool_field(&input, "notify").unwrap_or(false),
            "account": account.clone()
        });
        let transaction_order_id = transaction["id"].as_str().unwrap_or_default().to_string();
        if !self
            .store
            .staged
            .store_credit_transaction_order
            .iter()
            .any(|id| id == &transaction_order_id)
        {
            self.store
                .staged
                .store_credit_transaction_order
                .push(transaction_order_id.clone());
        }
        self.store
            .staged
            .store_credit_transactions
            .insert(transaction_order_id, transaction.clone());
        self.store
            .staged
            .store_credit_accounts
            .insert(account_id.clone(), account);

        let payload = self.store_credit_payload_for_selection(
            &field.selection,
            &field.name,
            Some(&transaction),
            Vec::new(),
        );
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(&field.name, "customers", vec![account_id]),
        )
    }

    pub(in crate::proxy) fn store_credit_account_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "storeCreditAccount" {
                continue;
            }
            let value = resolved_string_arg(&field.arguments, "id")
                .and_then(|id| self.store.staged.store_credit_accounts.get(&id))
                .map(|account| self.selected_store_credit_account(account, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn store_credit_accounts_connection_for_owner(
        &self,
        owner_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let accounts = self
            .store
            .staged
            .store_credit_account_order
            .iter()
            .filter_map(|id| self.store.staged.store_credit_accounts.get(id))
            .filter(|account| account["owner"]["id"].as_str() == Some(owner_id))
            .cloned()
            .collect::<Vec<_>>();
        selected_connection_json_with_args(accounts, arguments, selection, value_id_cursor)
    }

    fn store_credit_owner_exists(&self, owner_id: &str) -> bool {
        match shopify_gid_resource_type(owner_id) {
            Some("Customer") => {
                self.store.staged.customers.contains_key(owner_id)
                    && !self.store.staged.deleted_customer_ids.contains(owner_id)
            }
            Some("CompanyLocation") => {
                b2b_company_location_exists(&self.store.staged.b2b_locations, owner_id)
            }
            _ => false,
        }
    }

    fn store_credit_owner_has_accounts(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .store_credit_accounts
            .values()
            .any(|account| account["owner"]["id"].as_str() == Some(owner_id))
    }

    fn store_credit_owner_json(&self, owner_id: &str) -> Value {
        match shopify_gid_resource_type(owner_id) {
            Some("Customer") => self
                .store
                .staged
                .customers
                .get(owner_id)
                .cloned()
                .unwrap_or_else(|| json!({ "id": owner_id })),
            Some("CompanyLocation") => self
                .store
                .staged
                .b2b_locations
                .get(owner_id)
                .cloned()
                .unwrap_or_else(|| b2b_synthetic_seed_company_location(owner_id)),
            _ => json!({ "id": owner_id }),
        }
    }

    fn store_credit_payload_for_selection(
        &self,
        selection: &[SelectedField],
        root_field: &str,
        transaction: Option<&Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        let payload = json!({
            "__typename": if root_field == "storeCreditAccountCredit" {
                "StoreCreditAccountCreditPayload"
            } else {
                "StoreCreditAccountDebitPayload"
            },
            "storeCreditAccountTransaction": transaction.cloned().unwrap_or(Value::Null),
            "userErrors": user_errors
        });
        selected_payload_json(selection, |field| match field.name.as_str() {
            "storeCreditAccountTransaction" => Some(
                transaction
                    .map(|transaction| {
                        self.selected_store_credit_transaction(transaction, &field.selection)
                    })
                    .unwrap_or(Value::Null),
            ),
            _ => selected_json(&payload, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn b2b_build_company_location(
        &mut self,
        company_id: &str,
        company: &Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Vec<String>) {
        let id = self.next_proxy_synthetic_gid("CompanyLocation");
        let mut staged_ids = vec![id.clone()];
        let shipping_address = resolved_object_field(input, "shippingAddress").map(|address| {
            let address_id = self.next_proxy_synthetic_gid("CompanyAddress");
            staged_ids.push(address_id.clone());
            b2b_company_address_json(&address_id, &address)
        });
        let billing_same_as_shipping =
            resolved_bool_field(input, "billingSameAsShipping").unwrap_or(false);
        let billing_address = if billing_same_as_shipping {
            shipping_address.clone()
        } else {
            resolved_object_field(input, "billingAddress").map(|address| {
                let address_id = self.next_proxy_synthetic_gid("CompanyAddress");
                staged_ids.push(address_id.clone());
                b2b_company_address_json(&address_id, &address)
            })
        };
        let name = b2b_location_name(input, company, shipping_address.as_ref());
        let buyer_experience = resolved_object_field(input, "buyerExperienceConfiguration")
            .map(|buyer_experience| b2b_buyer_experience_configuration_json(&buyer_experience))
            .unwrap_or(Value::Null);
        let location = json!({
            "id": id,
            "name": name,
            "companyId": company_id,
            "externalId": resolved_string_field(input, "externalId").map(Value::String).unwrap_or(Value::Null),
            "locale": resolved_string_field(input, "locale").map(Value::String).unwrap_or(Value::Null),
            "phone": resolved_string_field(input, "phone").map(Value::String).unwrap_or(Value::Null),
            "shippingAddress": shipping_address.unwrap_or(Value::Null),
            "billingAddress": billing_address.unwrap_or(Value::Null),
            "billingSameAsShipping": billing_same_as_shipping,
            "taxSettings": {
                "taxExempt": resolved_bool_field(input, "taxExempt").unwrap_or(false),
                "taxExemptions": []
            },
            "buyerExperienceConfiguration": buyer_experience,
            "roleAssignmentIds": [],
            "staffAssignmentIds": []
        });
        (location, staged_ids)
    }

    pub(in crate::proxy) fn b2b_company_location_assign_staff_members_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_arg(&field.arguments, "locationId"))
            .unwrap_or_default();
        let staff_ids = resolved_string_list_field_unsorted(&field.arguments, "staffMemberIds");
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
            return (
                json!({
                    "companyLocationStaffMemberAssignments": Value::Null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        };

        let mut assignments = Vec::new();
        let mut user_errors = Vec::new();
        let mut seen_input = BTreeSet::new();
        for (index, staff_id) in staff_ids.iter().enumerate() {
            if !b2b_valid_staff_member_id(staff_id) {
                user_errors.push(b2b_indexed_user_error(
                    "staffMemberIds",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
                continue;
            }
            if !seen_input.insert(staff_id.clone()) {
                continue;
            }
            if let Some(existing) = self.b2b_staff_assignment_for(&location_id, staff_id) {
                assignments.push(existing);
                continue;
            }
            if b2b_json_id_list(&location, "staffAssignmentIds").len() >= 10 {
                user_errors.push(b2b_indexed_user_error(
                    "staffMemberIds",
                    index,
                    "Cannot assign more than 10 staff members to a company location.",
                    "LIMIT_REACHED",
                ));
                continue;
            }
            let assignment_id =
                self.next_proxy_synthetic_gid("CompanyLocationStaffMemberAssignment");
            let assignment = json!({
                "id": assignment_id,
                "companyLocationId": location_id,
                "staffMember": { "id": staff_id },
                "staffMemberId": staff_id
            });
            self.store
                .staged
                .b2b_staff_assignments
                .insert(assignment_id.clone(), assignment.clone());
            b2b_push_json_id(&mut location, "staffAssignmentIds", &assignment_id);
            assignments.push(assignment);
        }
        self.store
            .staged
            .b2b_locations
            .insert(location_id.clone(), location);
        let status = if assignments.is_empty() && !user_errors.is_empty() {
            "failed"
        } else {
            "staged"
        };
        let staged_ids = assignments
            .iter()
            .filter_map(|assignment| assignment["id"].as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        (
            json!({
                "companyLocationStaffMemberAssignments": if assignments.is_empty() && !user_errors.is_empty() {
                    Value::Null
                } else {
                    Value::Array(assignments)
                },
                "userErrors": user_errors
            }),
            status,
            staged_ids,
        )
    }

    pub(in crate::proxy) fn b2b_company_location_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        if !self.store.staged.b2b_locations.contains_key(&location_id) {
            return (
                json!({
                    "deletedCompanyLocationId": Value::Null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_delete_company_location(&location_id);
        (
            json!({
                "deletedCompanyLocationId": location_id,
                "userErrors": []
            }),
            "staged",
            vec![location_id],
        )
    }

    pub(in crate::proxy) fn b2b_company_location_remove_staff_members_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let assignment_ids = resolved_string_list_field_unsorted(
            &field.arguments,
            "companyLocationStaffMemberAssignmentIds",
        );
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, assignment_id) in assignment_ids.iter().enumerate() {
            if let Some(assignment) = self
                .store
                .staged
                .b2b_staff_assignments
                .remove(assignment_id)
            {
                if let Some(location_id) = assignment["companyLocationId"].as_str() {
                    self.b2b_remove_location_assignment_id(
                        location_id,
                        "staffAssignmentIds",
                        assignment_id,
                    );
                }
                deleted_ids.push(assignment_id.clone());
            } else {
                user_errors.push(b2b_indexed_user_error(
                    "companyLocationStaffMemberAssignmentIds",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
            }
        }
        let status = if deleted_ids.is_empty() && !user_errors.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "deletedCompanyLocationStaffMemberAssignmentIds": if deleted_ids.is_empty() && !user_errors.is_empty() {
                    Value::Null
                } else {
                    json!(deleted_ids)
                },
                "userErrors": user_errors
            }),
            status,
            deleted_ids,
        )
    }

    fn b2b_company_location_selected_json(
        &self,
        location: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "company" => {
                let company = location["companyId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_companies.get(id));
                Some(
                    company
                        .map(|company| {
                            self.b2b_company_selected_json(company, &selection.selection)
                        })
                        .unwrap_or(Value::Null),
                )
            }
            "roleAssignments" => {
                let assignments = b2b_json_id_list(location, "roleAssignmentIds")
                    .into_iter()
                    .filter_map(|id| self.store.staged.b2b_role_assignments.get(&id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &assignments,
                    &selection.arguments,
                    &selection.selection,
                    |assignment, fields| self.b2b_role_assignment_selected_json(assignment, fields),
                    value_id_cursor,
                ))
            }
            "staffMemberAssignments" => {
                let assignments = b2b_json_id_list(location, "staffAssignmentIds")
                    .into_iter()
                    .filter_map(|id| self.store.staged.b2b_staff_assignments.get(&id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &assignments,
                    &selection.arguments,
                    &selection.selection,
                    |assignment, fields| {
                        self.b2b_staff_assignment_selected_json(assignment, fields)
                    },
                    value_id_cursor,
                ))
            }
            _ => location
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    pub(in crate::proxy) fn b2b_company_locations_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_ids =
            resolved_string_list_field_unsorted(&field.arguments, "companyLocationIds");
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, location_id) in location_ids.iter().enumerate() {
            if self.store.staged.b2b_locations.contains_key(location_id) {
                self.b2b_delete_company_location(location_id);
                deleted_ids.push(location_id.clone());
            } else {
                user_errors.push(b2b_indexed_user_error(
                    "companyLocationIds",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
            }
        }
        let status = if deleted_ids.is_empty() && !user_errors.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "deletedCompanyLocationIds": deleted_ids,
                "userErrors": user_errors
            }),
            status,
            deleted_ids,
        )
    }

    fn b2b_company_selected_json(&self, company: &Value, selections: &[SelectedField]) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "locations" => {
                let locations = b2b_json_id_list(company, "locationIds")
                    .into_iter()
                    .filter_map(|id| self.store.staged.b2b_locations.get(&id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &locations,
                    &selection.arguments,
                    &selection.selection,
                    |location, fields| self.b2b_company_location_selected_json(location, fields),
                    value_id_cursor,
                ))
            }
            "contacts" => {
                let contacts = b2b_json_id_list(company, "contactIds")
                    .into_iter()
                    .filter_map(|id| self.store.staged.b2b_contacts.get(&id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &contacts,
                    &selection.arguments,
                    &selection.selection,
                    selected_json,
                    value_id_cursor,
                ))
            }
            "contactRoles" => {
                let roles = b2b_json_id_list(company, "contactRoleIds")
                    .into_iter()
                    .filter_map(|id| self.store.staged.b2b_contact_roles.get(&id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &roles,
                    &selection.arguments,
                    &selection.selection,
                    selected_json,
                    value_id_cursor,
                ))
            }
            "mainContact" => {
                let contact = company["mainContactId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_contacts.get(id));
                Some(
                    contact
                        .map(|contact| selected_json(contact, &selection.selection))
                        .unwrap_or(Value::Null),
                )
            }
            _ => company
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn b2b_delete_company_location(&mut self, location_id: &str) {
        let Some(location) = self.store.staged.b2b_locations.remove(location_id) else {
            return;
        };
        self.store
            .staged
            .b2b_location_order
            .retain(|id| id != location_id);
        if let Some(company_id) = location["companyId"].as_str() {
            if let Some(mut company) = self.store.staged.b2b_companies.get(company_id).cloned() {
                b2b_retain_json_ids(&mut company, "locationIds", |id| id != location_id);
                self.store
                    .staged
                    .b2b_companies
                    .insert(company_id.to_string(), company);
            }
        }
        for assignment_id in b2b_json_id_list(&location, "roleAssignmentIds") {
            self.store
                .staged
                .b2b_role_assignments
                .remove(&assignment_id);
        }
        for assignment_id in b2b_json_id_list(&location, "staffAssignmentIds") {
            self.store
                .staged
                .b2b_staff_assignments
                .remove(&assignment_id);
        }
    }

    fn b2b_ordered_locations(&self) -> Vec<Value> {
        self.store
            .staged
            .b2b_location_order
            .iter()
            .filter_map(|id| self.store.staged.b2b_locations.get(id).cloned())
            .collect()
    }

    fn b2b_payload_selected_json(&self, payload: &Value, selections: &[SelectedField]) -> Value {
        selected_payload_json(selections, |selection| {
            let value = payload.get(&selection.name)?;
            Some(match selection.name.as_str() {
                "company" if !value.is_null() => {
                    self.b2b_company_selected_json(value, &selection.selection)
                }
                "companyLocation" if !value.is_null() => {
                    self.b2b_company_location_selected_json(value, &selection.selection)
                }
                "addresses" => {
                    b2b_selected_array(value, &selection.selection, |address, fields| {
                        selected_json(address, fields)
                    })
                }
                "roleAssignments" => {
                    b2b_selected_array(value, &selection.selection, |assignment, fields| {
                        self.b2b_role_assignment_selected_json(assignment, fields)
                    })
                }
                "companyLocationStaffMemberAssignments" => {
                    b2b_selected_array(value, &selection.selection, |assignment, fields| {
                        self.b2b_staff_assignment_selected_json(assignment, fields)
                    })
                }
                "userErrors" => b2b_selected_array(value, &selection.selection, selected_json),
                _ => nullable_selected_json(value, &selection.selection),
            })
        })
    }

    fn b2b_query_has_staged_match(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "company" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.b2b_companies.contains_key(&id)),
            "companyLocation" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.b2b_locations.contains_key(&id)),
            "companyLocations" => !self.store.staged.b2b_locations.is_empty(),
            _ => false,
        })
    }

    fn b2b_remove_location_assignment_id(
        &mut self,
        location_id: &str,
        list_field: &str,
        assignment_id: &str,
    ) {
        if let Some(mut location) = self.store.staged.b2b_locations.get(location_id).cloned() {
            b2b_retain_json_ids(&mut location, list_field, |id| id != assignment_id);
            self.store
                .staged
                .b2b_locations
                .insert(location_id.to_string(), location);
        }
    }

    fn b2b_role_assignment_for(
        &self,
        location_id: &str,
        contact_id: &str,
        role_id: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .b2b_role_assignments
            .values()
            .find(|assignment| {
                assignment["companyLocationId"].as_str() == Some(location_id)
                    && assignment["companyContactId"].as_str() == Some(contact_id)
                    && assignment["companyContactRoleId"].as_str() == Some(role_id)
            })
            .cloned()
    }

    fn b2b_role_assignment_selected_json(
        &self,
        assignment: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "companyContact" => {
                let contact = assignment["companyContactId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_contacts.get(id));
                Some(
                    contact
                        .map(|contact| selected_json(contact, &selection.selection))
                        .unwrap_or(Value::Null),
                )
            }
            "role" => {
                let role = assignment["companyContactRoleId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_contact_roles.get(id));
                Some(
                    role.map(|role| selected_json(role, &selection.selection))
                        .unwrap_or(Value::Null),
                )
            }
            "companyLocation" => {
                let location = assignment["companyLocationId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_locations.get(id));
                Some(
                    location
                        .map(|location| {
                            self.b2b_company_location_selected_json(location, &selection.selection)
                        })
                        .unwrap_or(Value::Null),
                )
            }
            _ => assignment
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn b2b_staff_assignment_for(&self, location_id: &str, staff_id: &str) -> Option<Value> {
        self.store
            .staged
            .b2b_staff_assignments
            .values()
            .find(|assignment| {
                assignment["companyLocationId"].as_str() == Some(location_id)
                    && assignment["staffMemberId"].as_str() == Some(staff_id)
            })
            .cloned()
    }

    fn b2b_staff_assignment_selected_json(
        &self,
        assignment: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "companyLocation" => {
                let location = assignment["companyLocationId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_locations.get(id));
                Some(
                    location
                        .map(|location| {
                            self.b2b_company_location_selected_json(location, &selection.selection)
                        })
                        .unwrap_or(Value::Null),
                )
            }
            "staffMember" => Some(nullable_selected_json(
                &assignment["staffMember"],
                &selection.selection,
            )),
            _ => assignment
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn b2b_stage_location(&mut self, company: &mut Value, location: Value, location_id: &str) {
        b2b_push_json_id(company, "locationIds", location_id);
        let company_id = company["id"]
            .as_str()
            .expect("company must have an id")
            .to_string();
        self.store
            .staged
            .b2b_locations
            .insert(location_id.to_string(), location);
        if !self
            .store
            .staged
            .b2b_location_order
            .iter()
            .any(|id| id == location_id)
        {
            self.store
                .staged
                .b2b_location_order
                .push(location_id.to_string());
        }
        self.store
            .staged
            .b2b_companies
            .insert(company_id, company.clone());
    }


}

fn customer_merge_top_level_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    fields: &[RootFieldSelection],
) -> Option<Vec<Value>> {
    let operation_path = parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string());
    let mut errors = Vec::new();
    for field in fields.iter().filter(|field| field.name == "customerMerge") {
        let missing = ["customerOneId", "customerTwoId"]
            .into_iter()
            .filter(|argument| !field.raw_arguments.contains_key(*argument))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            errors.push(json!({
                "message": format!(
                    "Field 'customerMerge' is missing required arguments: {}",
                    missing.join(", ")
                ),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [operation_path.clone(), field.response_key.clone()],
                "extensions": {
                    "code": "missingRequiredArguments",
                    "className": "Field",
                    "name": "customerMerge",
                    "arguments": missing.join(", ")
                }
            }));
        }
        for argument in ["customerOneId", "customerTwoId"] {
            if matches!(
                field.raw_arguments.get(argument),
                Some(RawArgumentValue::String(value)) if value.is_empty()
            ) {
                errors.push(json!({
                    "message": "Invalid global id ''",
                    "locations": [{ "line": field.location.line, "column": field.location.column }],
                    "path": [operation_path.clone(), field.response_key.clone(), argument],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "CoercionError"
                    }
                }));
            }
        }
    }
    (!errors.is_empty()).then_some(errors)
}

fn customer_merge_payload_json(
    resulting_customer_id: Option<&str>,
    job_id: Option<&str>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "resultingCustomerId": resulting_customer_id.map(Value::from).unwrap_or(Value::Null),
        "job": job_id.map(|id| json!({ "__typename": "Job", "id": id, "done": false, "query": Value::Null })).unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_merge_request_json(
    job_id: &str,
    resulting_customer_id: &str,
    errors: Vec<Value>,
) -> Value {
    json!({
        "__typename": "CustomerMergeRequest",
        "jobId": job_id,
        "resultingCustomerId": resulting_customer_id,
        "status": "COMPLETED",
        "customerMergeErrors": errors
    })
}

fn customer_merge_job_from_request(request: &Value) -> Value {
    json!({
        "__typename": "Job",
        "id": request["jobId"].clone(),
        "done": true,
        "query": { "__typename": "QueryRoot" }
    })
}

fn customer_merge_user_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "errorFields": field,
        "block_type": code
    })
}

fn customer_data_erasure_payload_json(customer_id: Option<&str>, user_errors: Vec<Value>) -> Value {
    json!({
        "customerId": customer_id.map(Value::from).unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_data_erasure_user_error(message: &str, code: &str) -> Value {
    json!({
        "field": ["customerId"],
        "message": message,
        "code": code
    })
}

fn customer_tags(customer: &Value) -> Vec<String> {
    customer["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str().map(str::to_string))
        .collect()
}

fn customer_matches_search_query(
    customer: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(query) = resolved_string_arg(arguments, "query") else {
        return true;
    };
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if let Some(email) = query.strip_prefix("email:") {
        return customer.get("email").and_then(Value::as_str) == Some(email.trim_matches('"'));
    }
    if let Some(tag) = query.strip_prefix("tag:") {
        let tag = tag.trim_matches('"');
        return customer_tags(customer).iter().any(|value| value == tag);
    }
    customer.get("email").and_then(Value::as_str) == Some(query)
        || customer_tags(customer).iter().any(|value| value == query)
}

fn customer_merge_job_node_field(field: &RootFieldSelection) -> bool {
    resolved_string_arg(&field.arguments, "id")
        .as_deref()
        .is_some_and(|id| shopify_gid_resource_type(id) == Some("Job"))
}

fn apply_customer_merge_overrides(
    result: &mut Value,
    source: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) {
    for (override_key, target_field) in [
        ("customerIdOfEmailToKeep", "email"),
        ("customerIdOfPhoneNumberToKeep", "phone"),
        ("customerIdOfFirstNameToKeep", "firstName"),
        ("customerIdOfLastNameToKeep", "lastName"),
    ] {
        let Some(target_id) = resolved_string_field(override_fields, override_key) else {
            continue;
        };
        let target = if result["id"].as_str() == Some(target_id.as_str()) {
            result.clone()
        } else if source["id"].as_str() == Some(target_id.as_str()) {
            source.clone()
        } else {
            continue;
        };
        if let Some(value) = target.get(target_field).cloned() {
            result[target_field] = value;
        }
    }
    if let Some(note) = resolved_string_field(override_fields, "note") {
        result["note"] = json!(note);
    } else if result["note"].is_null() && !source["note"].is_null() {
        result["note"] = source["note"].clone();
    }
    if let Some(tags) = override_fields.get("tags") {
        if let ResolvedValue::List(tags) = tags {
            let mut tags = tags
                .iter()
                .filter_map(|tag| match tag {
                    ResolvedValue::String(tag) => Some(tag.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            tags.sort();
            result["tags"] = json!(tags);
        }
    } else {
        let mut tags = customer_tags(result)
            .into_iter()
            .chain(customer_tags(source))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
    }
    let first = result["firstName"].as_str().unwrap_or_default();
    let last = result["lastName"].as_str().unwrap_or_default();
    result["displayName"] = json!([first, last]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" "));
    if let Some(email) = result["email"].as_str() {
        result["defaultEmailAddress"] = json!({ "emailAddress": email });
    }
    if let Some(phone) = result["phone"].as_str() {
        result["defaultPhoneNumber"] = json!({ "phoneNumber": phone });
    }
}

fn normalize_merged_customer_defaults(customer: &mut Value) {
    if customer["numberOfOrders"].is_null() {
        customer["numberOfOrders"] = json!("0");
    }
    if customer["lastOrder"].is_null() {
        customer["lastOrder"] = Value::Null;
    }
    if customer["addressesV2"].is_null() {
        customer["addressesV2"] = empty_nodes_connection();
    }
    if customer["metafields"].is_null() {
        customer["metafields"] = empty_nodes_connection();
    }
    customer["updatedAt"] = json!("2026-05-05T15:13:52Z");
}

fn empty_nodes_connection() -> Value {
    json!({
        "nodes": [],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": Value::Null,
            "endCursor": Value::Null
        }
    })
}

fn b2b_contact_query_root(field: &str) -> bool {
    matches!(
        field,
        "company" | "companyContact" | "companyContactRole" | "companyLocation" | "node"
    )
}

fn b2b_contact_mutation_root(field: &str) -> bool {
    matches!(
        field,
        "companyContactCreate"
            | "companyContactUpdate"
            | "companyContactDelete"
            | "companyContactsDelete"
            | "companyAssignCustomerAsContact"
            | "companyContactRemoveFromCompany"
            | "companyAssignMainContact"
            | "companyRevokeMainContact"
            | "companyContactAssignRole"
            | "companyContactAssignRoles"
            | "companyContactRevokeRole"
            | "companyContactRevokeRoles"
            | "companyLocationAssignRoles"
            | "companyLocationRevokeRoles"
            | "companyLocationUpdate"
            | "companyLocationAssignAddress"
            | "companyAddressDelete"
    )
}

fn b2b_node_query_field(field: &RootFieldSelection) -> bool {
    let Some(id) = resolved_string_arg(&field.arguments, "id") else {
        return false;
    };
    matches!(
        shopify_gid_resource_type(&id),
        Some(
            "CompanyContact"
                | "CompanyContactRole"
                | "CompanyContactRoleAssignment"
                | "CompanyLocation"
                | "CompanyAddress"
        )
    )
}

const B2B_TAX_EXEMPTION_VALUES: &[&str] = &[
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
];

fn b2b_tax_exemption_coercion_error(query: &str, fields: &[RootFieldSelection]) -> Option<Value> {
    fields.iter().find_map(|field| {
        ["exemptionsToAssign", "exemptionsToRemove"]
            .into_iter()
            .find_map(|argument_name| {
                b2b_tax_exemption_argument_coercion_error(query, field, argument_name)
            })
    })
}

fn b2b_tax_exemption_argument_coercion_error(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<Value> {
    let raw_argument = field.raw_arguments.get(argument_name)?;
    match raw_argument {
        RawArgumentValue::Variable { name, value } => {
            let ResolvedValue::List(values) = value.as_ref()? else {
                return None;
            };
            let (index, invalid) =
                values
                    .iter()
                    .enumerate()
                    .find_map(|(index, value)| match value {
                        ResolvedValue::String(value)
                            if !B2B_TAX_EXEMPTION_VALUES.contains(&value.as_str()) =>
                        {
                            Some((index, value.as_str()))
                        }
                        _ => None,
                    })?;
            Some(b2b_tax_exemption_invalid_variable_error(
                query, name, values, index, invalid,
            ))
        }
        RawArgumentValue::List(values) => {
            let invalid = values.iter().find_map(|value| match value {
                RawArgumentValue::Enum(value)
                    if !B2B_TAX_EXEMPTION_VALUES.contains(&value.as_str()) =>
                {
                    Some(value.as_str())
                }
                _ => None,
            })?;
            Some(b2b_tax_exemption_invalid_literal_error(
                argument_name,
                invalid,
            ))
        }
        _ => None,
    }
}

fn b2b_tax_exemption_invalid_variable_error(
    query: &str,
    variable_name: &str,
    values: &[ResolvedValue],
    index: usize,
    invalid: &str,
) -> Value {
    let expected = B2B_TAX_EXEMPTION_VALUES.join(", ");
    let explanation = format!("Expected \"{invalid}\" to be one of: {expected}");
    let variable = variable_definition_info(query, variable_name);
    let variable_type = variable
        .as_ref()
        .map(|definition| definition.type_display.as_str())
        .unwrap_or("[TaxExemption!]");
    let location = variable
        .as_ref()
        .map(|definition| definition.location)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    json!({
        "message": format!(
            "Variable ${variable_name} of type {variable_type} was provided invalid value for {index} ({explanation})"
        ),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": Value::Array(values.iter().map(resolved_value_json).collect()),
            "problems": [{
                "path": [index],
                "explanation": explanation
            }]
        }
    })
}

fn b2b_tax_exemption_invalid_literal_error(argument_name: &str, invalid: &str) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' has an invalid value [{invalid}]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?"
        ),
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "argumentName": argument_name
        }
    })
}

fn b2b_company_contact_payload(company_contact: Option<&Value>, user_errors: Vec<Value>) -> Value {
    json!({
        "companyContact": company_contact.cloned().unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn b2b_company_contact_delete_payload(
    id: Option<&str>,
    id_field: &str,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        id_field: id.map(|id| json!(id)).unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn b2b_company_contact_input_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (field, label) in [
        ("firstName", "First name"),
        ("lastName", "Last name"),
        ("title", "Title"),
    ] {
        let Some(value) = resolved_string_field(input, field) else {
            continue;
        };
        let mut path = field_prefix.to_vec();
        path.push(field);
        if value.chars().count() > 255 {
            errors.push(b2b_company_user_error(
                path.clone(),
                &format!("{label} is too long"),
                "TOO_LONG",
                None,
            ));
        }
        if b2b_contains_html_tags(&value) {
            errors.push(b2b_company_user_error(
                path,
                &format!("{label} contains HTML tags"),
                "CONTAINS_HTML_TAGS",
                None,
            ));
        }
    }
    errors
}

fn b2b_indexed_role_assignment_error(index: Option<usize>, field: &str) -> Value {
    let error_field = index
        .map(|index| json!(["rolesToAssign", index.to_string(), field]))
        .unwrap_or_else(|| json!([field]));
    json!({
        "field": error_field,
        "message": "Resource requested does not exist.",
        "code": "RESOURCE_NOT_FOUND"
    })
}

fn b2b_default_contact_role(company_id: &str) -> Value {
    b2b_contact_role(company_id, "Ordering only", true)
}

fn b2b_contact_role(company_id: &str, name: &str, default: bool) -> Value {
    let suffix = if name == "Ordering only" {
        resource_id_tail(company_id).to_string()
    } else {
        format!(
            "{}-{}",
            resource_id_tail(company_id),
            name.to_ascii_lowercase().replace(' ', "-")
        )
    };
    let role_id = synthetic_shopify_gid("CompanyContactRole", suffix);
    json!({
        "id": role_id,
        "companyId": company_id,
        "name": name,
        "note": Value::Null,
        "default": default
    })
}

fn b2b_company_address_json(location_id: &str, address: &BTreeMap<String, ResolvedValue>) -> Value {
    let id = synthetic_shopify_gid(
        "CompanyAddress",
        format!("{}-billing", resource_id_tail(location_id)),
    );
    json!({
        "id": id,
        "address1": resolved_string_field(address, "address1").unwrap_or_default(),
        "address2": resolved_string_field(address, "address2").unwrap_or_default(),
        "city": resolved_string_field(address, "city").unwrap_or_default(),
        "province": resolved_string_field(address, "province").unwrap_or_default(),
        "provinceCode": resolved_string_field(address, "provinceCode").unwrap_or_default(),
        "country": resolved_string_field(address, "country").unwrap_or_default(),
        "countryCode": resolved_string_field(address, "countryCode")
            .or_else(|| resolved_string_field(address, "countryCodeV2"))
            .unwrap_or_default(),
        "zip": resolved_string_field(address, "zip").unwrap_or_default(),
        "phone": resolved_string_field(address, "phone").map(Value::String).unwrap_or(Value::Null)
    })
}

fn b2b_contact_customer_json(
    customer_id: &str,
    first_name: &str,
    last_name: &str,
    email: &str,
    phone: Option<&str>,
) -> Value {
    json!({
        "id": customer_id,
        "firstName": first_name,
        "lastName": last_name,
        "displayName": b2b_display_name(first_name, last_name),
        "email": if email.is_empty() { Value::Null } else { json!(email) },
        "phone": phone.map(|phone| json!(phone)).unwrap_or(Value::Null)
    })
}

fn b2b_normalize_phone(phone: &str) -> String {
    if phone.starts_with('+') {
        return phone.to_string();
    }
    let digits = phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.len() == 10 {
        format!("+1{digits}")
    } else if digits.len() == 11 && digits.starts_with('1') {
        format!("+{digits}")
    } else {
        phone.to_string()
    }
}

fn b2b_display_name(first_name: &str, last_name: &str) -> String {
    [first_name, last_name]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

pub(in crate::proxy) fn default_email_address_value(email: &str) -> Value {
    if email.is_empty() {
        return Value::Null;
    }
    json!({
        "emailAddress": email,
        "marketingState": "NOT_SUBSCRIBED",
        "marketingOptInLevel": "SINGLE_OPT_IN",
        "marketingUpdatedAt": Value::Null
    })
}

pub(in crate::proxy) fn default_phone_number_value(phone: &str) -> Value {
    json!({
        "phoneNumber": phone,
        "marketingState": "NOT_SUBSCRIBED",
        "marketingOptInLevel": "SINGLE_OPT_IN",
        "marketingUpdatedAt": Value::Null,
        "marketingCollectedFrom": Value::Null
    })
}

pub(in crate::proxy) fn email_marketing_consent_value(email: &str) -> Value {
    if email.is_empty() {
        return Value::Null;
    }
    json!({
        "marketingState": "NOT_SUBSCRIBED",
        "marketingOptInLevel": "SINGLE_OPT_IN",
        "consentUpdatedAt": Value::Null
    })
}

pub(in crate::proxy) fn sms_marketing_consent_value(phone: &str) -> Value {
    if phone.is_empty() {
        return Value::Null;
    }
    json!({
        "marketingState": "NOT_SUBSCRIBED",
        "marketingOptInLevel": "SINGLE_OPT_IN",
        "consentUpdatedAt": Value::Null,
        "consentCollectedFrom": Value::Null
    })
}


#[derive(Clone, Copy)]
struct CustomerCountryInfo {
    code: &'static str,
    name: &'static str,
    zones: &'static [(&'static str, &'static str)],
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


const STORE_CREDIT_LIMIT: f64 = 100000.0;


fn customer_address_contains_emoji(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x1F300..=0x1FAFF | 0x2600..=0x27BF
        )
    })
}


fn customer_address_contains_html(value: &str) -> bool {
    value.contains('<') && value.contains('>')
}


fn customer_address_contains_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
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


fn customer_address_id_arg(field: &RootFieldSelection) -> String {
    resolved_string_arg(&field.arguments, "addressId")
        .or_else(|| resolved_string_arg(&field.arguments, "id"))
        .unwrap_or_default()
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


fn customer_address_user_error(path: Vec<&str>, message: &str) -> Value {
    customer_address_user_error_path(path.into_iter().map(str::to_string).collect(), message)
}


fn customer_address_user_error_path(path: Vec<String>, message: &str) -> Value {
    json!({ "field": path, "message": message })
}


fn customer_address_user_error_prefixed(prefix: &[&str], field: &str, message: &str) -> Value {
    let mut path = prefix
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    path.push(field.to_string());
    customer_address_user_error_path(path, message)
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


fn customer_payload_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selected_child_selection(selection, "customer").unwrap_or_default()
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


fn resolved_money_amount_text(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        Some(ResolvedValue::Int(value)) => Some(value.to_string()),
        Some(ResolvedValue::Float(value)) => Some(value.to_string()),
        _ => None,
    }
}


fn shopify_decimal_text(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{rounded:.1}")
    } else {
        let text = format!("{rounded:.2}");
        text.trim_end_matches('0').to_string()
    }
}


fn store_credit_expires_at_in_past(expires_at: &str) -> bool {
    !expires_at.is_empty() && expires_at < "2026-06-15T00:00:00Z"
}


fn store_credit_money(amount: &str, currency: &str) -> Value {
    json!({
        "amount": amount,
        "currencyCode": currency
    })
}


fn store_credit_supported_currency(currency: &str) -> bool {
    matches!(
        currency,
        "USD" | "CAD" | "AUD" | "EUR" | "GBP" | "JPY" | "NZD"
    )
}


fn store_credit_user_error(field: &[&str], message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}


fn b2b_address_type_response_order(address_types: &[String]) -> Vec<String> {
    let mut ordered = Vec::new();
    if address_types
        .iter()
        .any(|address_type| address_type == "SHIPPING")
    {
        ordered.push("SHIPPING".to_string());
    }
    if address_types
        .iter()
        .any(|address_type| address_type == "BILLING")
    {
        ordered.push("BILLING".to_string());
    }
    for address_type in address_types {
        if address_type != "SHIPPING"
            && address_type != "BILLING"
            && !ordered.iter().any(|known| known == address_type)
        {
            ordered.push(address_type.clone());
        }
    }
    ordered
}


fn b2b_buyer_experience_configuration_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "editableShippingAddress": resolved_bool_field(input, "editableShippingAddress").unwrap_or(false),
        "checkoutToDraft": resolved_bool_field(input, "checkoutToDraft").unwrap_or(false),
        "paymentTermsTemplate": resolved_string_field(input, "paymentTermsTemplateId")
            .map(|id| json!({ "id": id }))
            .unwrap_or(Value::Null),
        "deposit": if input.contains_key("deposit") {
            json!({ "__typename": "DepositPercentage" })
        } else {
            Value::Null
        }
    })
}


fn b2b_company_location_create_validation_errors(
    _input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    Vec::new()
}


fn b2b_indexed_user_error(field: &str, index: usize, message: &str, code: &str) -> Value {
    json!({
        "field": [field, index.to_string()],
        "message": message,
        "code": code
    })
}


fn b2b_json_id_list(record: &Value, field: &str) -> Vec<String> {
    record[field]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}


fn b2b_location_address_slot(address_type: &str) -> &'static str {
    match address_type {
        "SHIPPING" => "shippingAddress",
        _ => "billingAddress",
    }
}


fn b2b_location_name(
    input: &BTreeMap<String, ResolvedValue>,
    company: &Value,
    shipping_address: Option<&Value>,
) -> String {
    resolved_string_field(input, "name")
        .map(|name| b2b_strip_html_tags(&name))
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            shipping_address
                .and_then(|address| address["address1"].as_str())
                .map(str::to_string)
                .filter(|address1| !address1.trim().is_empty())
        })
        .or_else(|| company["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "B2B Draft".to_string())
}


fn b2b_push_json_id(record: &mut Value, field: &str, id: &str) {
    if !record[field].is_array() {
        record[field] = json!([]);
    }
    let ids = record[field]
        .as_array_mut()
        .expect("JSON id list must be an array");
    if !ids.iter().any(|existing| existing.as_str() == Some(id)) {
        ids.push(json!(id));
    }
}


fn b2b_unique_strings(values: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().all(|value| seen.insert(value))
}


fn b2b_valid_staff_member_id(id: &str) -> bool {
    shopify_gid_resource_type(id) == Some("StaffMember")
        && resource_id_tail(id)
            .parse::<u64>()
            .is_ok_and(|tail| (1..=100).contains(&tail))
}



fn b2b_selected_array<F>(value: &Value, selections: &[SelectedField], mut item_json: F) -> Value
where
    F: FnMut(&Value, &[SelectedField]) -> Value,
{
    if value.is_null() {
        return Value::Null;
    }
    value
        .as_array()
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .map(|item| item_json(item, selections))
                    .collect(),
            )
        })
        .unwrap_or_else(|| nullable_selected_json(value, selections))
}


fn b2b_retain_json_ids<F>(record: &mut Value, field: &str, mut retain: F)
where
    F: FnMut(&str) -> bool,
{
    if let Some(ids) = record[field].as_array_mut() {
        ids.retain(|id| id.as_str().is_some_and(&mut retain));
    }
}

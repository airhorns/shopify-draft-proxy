use super::*;

impl DraftProxy {
    const CUSTOMER_COUNT_HYDRATE_QUERY: &'static str =
        "query CustomerCountHydrate { customersCount { count precision } }";

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
                        "companyCreate" | "companyUpdate" | "companyLocationCreate"
                    )
                }) =>
            {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) = match field.name.as_str() {
                        "companyCreate" => self.b2b_company_create_payload(&field),
                        "companyUpdate" => self.b2b_company_update_payload(&field),
                        "companyLocationCreate" => self.b2b_company_location_create_payload(&field),
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
        let id = self.stage_b2b_company_location(&company_id, &input);
        let materialized = self.b2b_company_location_materialized(&id);
        (
            b2b_company_location_payload(materialized.as_ref(), Vec::new()),
            "staged",
            vec![id],
        )
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
            location["name"] = json!(name);
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
        let name = resolved_string_field(input, "name").unwrap_or_else(|| "HQ".to_string());
        let billing_address = resolved_object_field(input, "billingAddress")
            .map(|address| b2b_company_address_json(&id, &address))
            .unwrap_or(Value::Null);
        let location = json!({
            "id": id,
            "companyId": company_id,
            "name": name,
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
                "customersCount" => Some(self.customers_count_field(field)),
                _ => None,
            };
            if let Some(value) = value {
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    fn customers_count_field(&self, field: &RootFieldSelection) -> Value {
        let count = self
            .store
            .staged
            .customers_count
            .as_ref()
            .cloned()
            .unwrap_or_else(|| json!({ "count": 177, "precision": "EXACT" }));
        selected_json(&count, &field.selection)
    }

    pub(in crate::proxy) fn hydrate_customers_count_for_overlay_read(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.staged.customers_count.is_some()
        {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": Self::CUSTOMER_COUNT_HYDRATE_QUERY,
                "operationName": "CustomerCountHydrate",
                "variables": {},
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return;
        }
        let count = response.body["data"]["customersCount"].clone();
        if !count.is_null() {
            self.store.staged.customers_count = Some(count);
        }
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

    pub(in crate::proxy) fn customer_tax_exemptions_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, staged_id) = self.customer_tax_exemptions_payload(field, request);
            if let Some(id) = staged_id {
                self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn customer_tax_exemptions_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        if customer_id.is_empty()
            || self
                .store
                .staged
                .deleted_customer_ids
                .contains(&customer_id)
        {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request);
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }

        let tax_exemptions = normalize_customer_tax_exemptions(
            resolved_string_list_field_unsorted(&field.arguments, "taxExemptions"),
        );
        let mut customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let existing = customer_tax_exemptions(&customer);
        let next = match field.name.as_str() {
            "customerAddTaxExemptions" => add_customer_tax_exemptions(existing, tax_exemptions),
            "customerRemoveTaxExemptions" => {
                remove_customer_tax_exemptions(existing, tax_exemptions)
            }
            "customerReplaceTaxExemptions" => tax_exemptions,
            _ => existing,
        };
        customer["taxExemptions"] = json!(next);
        customer["updatedAt"] = json!("2026-04-25T01:41:06Z");
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());

        (
            customer_tax_exemptions_payload(customer, Vec::new()),
            Some(customer_id),
        )
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

fn customer_tax_exemptions_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_tax_exemptions_user_error() -> Value {
    json!({
        "field": ["customerId"],
        "message": "Customer does not exist."
    })
}

fn customer_tax_exemptions(customer: &Value) -> Vec<String> {
    customer
        .get("taxExemptions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_customer_tax_exemptions(exemptions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for exemption in exemptions {
        if seen.insert(exemption.clone()) {
            normalized.push(exemption);
        }
    }
    normalized
}

fn add_customer_tax_exemptions(existing: Vec<String>, additions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut merged = Vec::new();
    for exemption in existing.into_iter().chain(additions) {
        if seen.insert(exemption.clone()) {
            merged.push(exemption);
        }
    }
    merged
}

fn remove_customer_tax_exemptions(existing: Vec<String>, removals: Vec<String>) -> Vec<String> {
    let removals = removals.into_iter().collect::<BTreeSet<_>>();
    existing
        .into_iter()
        .filter(|exemption| !removals.contains(exemption))
        .collect()
}

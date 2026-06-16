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
        let all_roots_allowed = match operation_type {
            OperationType::Mutation => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "companyCreate"
                        | "companyUpdate"
                        | "companyLocationCreate"
                        | "companyLocationUpdate"
                        | "companyLocationDelete"
                        | "companyLocationsDelete"
                        | "companyLocationAssignAddress"
                        | "companyAddressDelete"
                        | "companyLocationAssignStaffMembers"
                        | "companyLocationRemoveStaffMembers"
                        | "companyLocationAssignRoles"
                        | "companyLocationRevokeRoles"
                )
            }),
            OperationType::Query => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "company" | "companyLocation" | "companyLocations"
                )
            }),
            OperationType::Subscription => false,
        };
        if !all_roots_allowed {
            return None;
        }
        if operation_type == OperationType::Query
            && self.config.read_mode != ReadMode::Snapshot
            && !self.b2b_query_has_staged_match(&fields)
        {
            return None;
        }

        match operation_type {
            OperationType::Mutation => {
                let mut data = serde_json::Map::new();
                for field in fields {
                    let (payload, status, staged_ids) = match field.name.as_str() {
                        "companyCreate" => self.b2b_company_create_payload(&field),
                        "companyUpdate" => self.b2b_company_update_payload(&field),
                        "companyLocationCreate" => self.b2b_company_location_create_payload(&field),
                        "companyLocationUpdate" => self.b2b_company_location_update_payload(&field),
                        "companyLocationDelete" => self.b2b_company_location_delete_payload(&field),
                        "companyLocationsDelete" => {
                            self.b2b_company_locations_delete_payload(&field)
                        }
                        "companyLocationAssignAddress" => {
                            self.b2b_company_location_assign_address_payload(&field)
                        }
                        "companyAddressDelete" => self.b2b_company_address_delete_payload(&field),
                        "companyLocationAssignStaffMembers" => {
                            self.b2b_company_location_assign_staff_members_payload(&field)
                        }
                        "companyLocationRemoveStaffMembers" => {
                            self.b2b_company_location_remove_staff_members_payload(&field)
                        }
                        "companyLocationAssignRoles" => {
                            self.b2b_company_location_assign_roles_payload(&field)
                        }
                        "companyLocationRevokeRoles" => {
                            self.b2b_company_location_revoke_roles_payload(&field)
                        }
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
                        self.b2b_payload_selected_json(&payload, &field.selection),
                    );
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            OperationType::Query => {
                let mut data = serde_json::Map::new();
                for field in fields {
<<<<<<< ours
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
=======
                    let value = match field.name.as_str() {
                        "company" => {
                            let id =
                                resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                            self.store
                                .staged
                                .b2b_companies
                                .get(&id)
                                .map(|company| {
                                    self.b2b_company_selected_json(company, &field.selection)
                                })
                                .unwrap_or(Value::Null)
                        }
                        "companyLocation" => {
                            let id =
                                resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                            self.store
                                .staged
                                .b2b_locations
                                .get(&id)
                                .map(|location| {
                                    self.b2b_company_location_selected_json(
                                        location,
                                        &field.selection,
                                    )
                                })
                                .unwrap_or(Value::Null)
                        }
                        "companyLocations" => {
                            let locations = self.b2b_ordered_locations();
                            selected_typed_connection_with_args(
                                &locations,
                                &field.arguments,
                                &field.selection,
                                |location, selections| {
                                    self.b2b_company_location_selected_json(location, selections)
                                },
                                value_id_cursor,
                            )
                        }
                        _ => return None,
                    };
                    data.insert(field.response_key.clone(), value);
>>>>>>> theirs
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
        let mut company = json!({
            "id": id,
            "name": name,
            "externalId": resolved_string_field(&company_input, "externalId").map(Value::String).unwrap_or(Value::Null),
            "customerSince": resolved_string_field(&company_input, "customerSince").map(Value::String).unwrap_or(Value::Null),
            "note": resolved_string_field(&company_input, "note").map(Value::String).unwrap_or(Value::Null),
            "locationIds": [],
            "contactIds": [],
            "contactRoleIds": [],
            "mainContactId": Value::Null
        });

        if let Some(contact_input) = resolved_object_field(&input, "companyContact") {
            let contact_id = self.next_proxy_synthetic_gid("CompanyContact");
            let contact = json!({
                "id": contact_id,
                "title": resolved_string_field(&contact_input, "title")
                    .or_else(|| resolved_string_field(&contact_input, "name"))
                    .unwrap_or_else(|| "Buyer".to_string()),
                "companyId": id,
                "locale": resolved_string_field(&contact_input, "locale").map(Value::String).unwrap_or(Value::Null)
            });
            self.store
                .staged
                .b2b_contacts
                .insert(contact_id.clone(), contact);
            company["contactIds"]
                .as_array_mut()
                .expect("contactIds must be an array")
                .push(json!(contact_id.clone()));
            company["mainContactId"] = json!(contact_id);
        }

        if let Some(role_input) = resolved_object_field(&input, "companyContactRole") {
            let role_id = self.next_proxy_synthetic_gid("CompanyContactRole");
            let role = json!({
                "id": role_id,
                "name": resolved_string_field(&role_input, "name")
                    .unwrap_or_else(|| "Ordering only".to_string()),
                "companyId": id
            });
            self.store
                .staged
                .b2b_contact_roles
                .insert(role_id.clone(), role);
            company["contactRoleIds"]
                .as_array_mut()
                .expect("contactRoleIds must be an array")
                .push(json!(role_id));
        }

        let mut staged_ids = vec![id.clone()];
        if let Some(location_input) = resolved_object_field(&input, "companyLocation") {
            let (location, location_staged_ids) =
                self.b2b_build_company_location(&id, &company, &location_input);
            let location_id = location["id"]
                .as_str()
                .expect("location must have an id")
                .to_string();
            self.b2b_stage_location(&mut company, location, &location_id);
            staged_ids.extend(location_staged_ids);
        }

        self.store
            .staged
            .b2b_companies
            .insert(id.clone(), company.clone());
        (
            b2b_company_payload(Some(&company), Vec::new()),
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
        (
            b2b_company_payload(Some(&company), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    pub(in crate::proxy) fn b2b_company_location_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let Some(mut company) = self.store.staged.b2b_companies.get(&company_id).cloned() else {
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
        };

        let errors = b2b_company_location_create_validation_errors(&input);
        if !errors.is_empty() {
            return (
                b2b_company_location_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }

        let (location, staged_ids) = self.b2b_build_company_location(&company_id, &company, &input);
        let location_id = location["id"]
            .as_str()
            .expect("location must have an id")
            .to_string();
        self.b2b_stage_location(&mut company, location.clone(), &location_id);
        (
            b2b_company_location_payload(Some(&location), Vec::new()),
            "staged",
            staged_ids,
        )
    }

    pub(in crate::proxy) fn b2b_company_location_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
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
        };
        if resolved_string_field(&input, "name").is_some_and(|name| name.trim().is_empty()) {
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

        if let Some(name) = resolved_string_field(&input, "name") {
            location["name"] = json!(b2b_strip_html_tags(&name));
        }
        if let Some(external_id) = resolved_string_field(&input, "externalId") {
            location["externalId"] = json!(external_id);
        }
        if let Some(locale) = resolved_string_field(&input, "locale") {
            location["locale"] = json!(locale);
        }
        if let Some(phone) = resolved_string_field(&input, "phone") {
            location["phone"] = json!(phone);
        }
        if let Some(shipping_input) = resolved_object_field(&input, "shippingAddress") {
            let address_id = location["shippingAddress"]["id"]
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| self.next_proxy_synthetic_gid("CompanyAddress"));
            location["shippingAddress"] = b2b_company_address_json(&address_id, &shipping_input);
        }
        if let Some(billing_input) = resolved_object_field(&input, "billingAddress") {
            let address_id = location["billingAddress"]["id"]
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| self.next_proxy_synthetic_gid("CompanyAddress"));
            location["billingAddress"] = b2b_company_address_json(&address_id, &billing_input);
            location["billingSameAsShipping"] = json!(false);
        }
        if resolved_bool_field(&input, "billingSameAsShipping") == Some(true) {
            location["billingAddress"] = location["shippingAddress"].clone();
            location["billingSameAsShipping"] = json!(true);
        } else if resolved_bool_field(&input, "billingSameAsShipping") == Some(false) {
            location["billingSameAsShipping"] = json!(false);
        }
        if let Some(buyer_experience) =
            resolved_object_field(&input, "buyerExperienceConfiguration")
        {
            location["buyerExperienceConfiguration"] =
                b2b_buyer_experience_configuration_json(&buyer_experience);
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

    pub(in crate::proxy) fn b2b_company_location_assign_address_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "locationId")
            .or_else(|| resolved_string_arg(&field.arguments, "companyLocationId"))
            .unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let address_types = resolved_string_list_field_unsorted(&field.arguments, "addressTypes");
        if !b2b_unique_strings(&address_types) {
            return (
                json!({
                    "addresses": Value::Null,
                    "userErrors": [{
                        "field": Value::Null,
                        "message": "Invalid input.",
                        "code": "INVALID_INPUT"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
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

        let mut changed_addresses = Vec::new();
        let response_order = b2b_address_type_response_order(&address_types);
        for address_type in &response_order {
            let slot = b2b_location_address_slot(address_type);
            let address_id = location[slot]["id"]
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| self.next_proxy_synthetic_gid("CompanyAddress"));
            let address = b2b_company_address_json(&address_id, &address_input);
            location[slot] = address.clone();
            changed_addresses.push(address);
        }
        if response_order.len() == 2 {
            location["billingSameAsShipping"] = json!(false);
        } else if response_order
            .iter()
            .any(|address_type| matches!(address_type.as_str(), "BILLING" | "SHIPPING"))
        {
            let billing_id = location["billingAddress"]["id"].as_str();
            let shipping_id = location["shippingAddress"]["id"].as_str();
            location["billingSameAsShipping"] =
                json!(billing_id.is_some() && billing_id == shipping_id);
        }
        self.store
            .staged
            .b2b_locations
            .insert(location_id.clone(), location);
        (
            json!({
                "addresses": changed_addresses,
                "userErrors": []
            }),
            "staged",
            vec![location_id],
        )
    }

    pub(in crate::proxy) fn b2b_company_address_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let address_id = resolved_string_arg(&field.arguments, "addressId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        let mut touched_location_ids = Vec::new();
        let location_ids = self.store.staged.b2b_location_order.clone();
        for location_id in location_ids {
            let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned()
            else {
                continue;
            };
            let billing_matches = location["billingAddress"]["id"].as_str() == Some(&address_id);
            let shipping_matches = location["shippingAddress"]["id"].as_str() == Some(&address_id);
            if !billing_matches && !shipping_matches {
                continue;
            }
            let shared = location["billingSameAsShipping"].as_bool().unwrap_or(false)
                || (location["billingAddress"]["id"].as_str().is_some()
                    && location["billingAddress"]["id"].as_str()
                        == location["shippingAddress"]["id"].as_str());
            if shared {
                location["billingAddress"] = Value::Null;
                location["shippingAddress"] = Value::Null;
                location["billingSameAsShipping"] = json!(false);
            } else {
                if billing_matches {
                    location["billingAddress"] = Value::Null;
                    location["billingSameAsShipping"] = json!(false);
                }
                if shipping_matches {
                    location["shippingAddress"] = Value::Null;
                    location["billingSameAsShipping"] = json!(false);
                }
            }
            self.store
                .staged
                .b2b_locations
                .insert(location_id.clone(), location);
            touched_location_ids.push(location_id);
        }
        if touched_location_ids.is_empty() {
            return (
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
            );
        }
        (
            json!({
                "deletedAddressId": address_id,
                "userErrors": []
            }),
            "staged",
            touched_location_ids,
        )
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

    pub(in crate::proxy) fn b2b_company_location_assign_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let location_id = resolved_string_arg(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_arg(&field.arguments, "locationId"))
            .unwrap_or_default();
        let roles_to_assign = resolved_object_list_field(&field.arguments, "rolesToAssign");
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
            return (
                json!({
                    "roleAssignments": Value::Null,
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
        for (index, input) in roles_to_assign.iter().enumerate() {
            let contact_id = resolved_string_field(input, "companyContactId").unwrap_or_default();
            let role_id = resolved_string_field(input, "companyContactRoleId")
                .or_else(|| resolved_string_field(input, "companyRoleId"))
                .unwrap_or_default();
            if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
                user_errors.push(b2b_indexed_user_error(
                    "rolesToAssign",
                    index,
                    "Company contact does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
                continue;
            }
            if !self.store.staged.b2b_contact_roles.contains_key(&role_id) {
                user_errors.push(b2b_indexed_user_error(
                    "rolesToAssign",
                    index,
                    "Company role does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
                continue;
            }
            if let Some(existing) =
                self.b2b_role_assignment_for(&location_id, &contact_id, &role_id)
            {
                assignments.push(existing);
                continue;
            }
            let assignment_id = self.next_proxy_synthetic_gid("CompanyContactRoleAssignment");
            let assignment = json!({
                "id": assignment_id,
                "companyLocationId": location_id,
                "companyContactId": contact_id,
                "companyContactRoleId": role_id
            });
            self.store
                .staged
                .b2b_role_assignments
                .insert(assignment_id.clone(), assignment.clone());
            b2b_push_json_id(&mut location, "roleAssignmentIds", &assignment_id);
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
                "roleAssignments": if assignments.is_empty() && !user_errors.is_empty() {
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

    pub(in crate::proxy) fn b2b_company_location_revoke_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let assignment_ids = resolved_string_list_field_unsorted(&field.arguments, "rolesToRevoke");
        let mut revoked_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, assignment_id) in assignment_ids.iter().enumerate() {
            if let Some(assignment) = self.store.staged.b2b_role_assignments.remove(assignment_id) {
                if let Some(location_id) = assignment["companyLocationId"].as_str() {
                    self.b2b_remove_location_assignment_id(
                        location_id,
                        "roleAssignmentIds",
                        assignment_id,
                    );
                }
                revoked_ids.push(assignment_id.clone());
            } else {
                user_errors.push(b2b_indexed_user_error(
                    "rolesToRevoke",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
            }
        }
        let status = if revoked_ids.is_empty() && !user_errors.is_empty() {
            "failed"
        } else {
            "staged"
        };
        (
            json!({
                "revokedRoleAssignmentIds": if revoked_ids.is_empty() && !user_errors.is_empty() {
                    Value::Null
                } else {
                    json!(revoked_ids)
                },
                "revokedCompanyContactRoleAssignmentIds": if revoked_ids.is_empty() && !user_errors.is_empty() {
                    Value::Null
                } else {
                    json!(revoked_ids)
                },
                "userErrors": user_errors
            }),
            status,
            revoked_ids,
        )
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

    fn b2b_ordered_locations(&self) -> Vec<Value> {
        self.store
            .staged
            .b2b_location_order
            .iter()
            .filter_map(|id| self.store.staged.b2b_locations.get(id).cloned())
            .collect()
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

        let id = if email.ends_with("example.test") {
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
        let tags = if matches!(
            id.as_str(),
            "gid://shopify/Customer/10541053706546" | "gid://shopify/Customer/10541053772082"
        ) {
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

    pub(in crate::proxy) fn customer_outbound_side_effect_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if fields.is_empty()
            || !fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "customerGenerateAccountActivationUrl"
                        | "customerSendAccountInviteEmail"
                        | "customerPaymentMethodSendUpdateEmail"
                )
            })
        {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, should_log) = match field.name.as_str() {
                "customerGenerateAccountActivationUrl" => {
                    self.customer_generate_account_activation_url_payload(&field)
                }
                "customerSendAccountInviteEmail" => {
                    self.customer_send_account_invite_email_payload(&field)
                }
                "customerPaymentMethodSendUpdateEmail" => {
                    self.customer_payment_method_send_update_email_payload(&field)
                }
                _ => return None,
            };
            if should_log {
                let customer_id = resolved_string_arg(&field.arguments, "customerId")
                    .map(|id| vec![id])
                    .unwrap_or_default();
                self.record_mutation_log_entry(request, query, variables, &field.name, customer_id);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }

        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    fn customer_generate_account_activation_url_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, bool) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let Some(customer) = self.store.staged.customers.get_mut(&customer_id) else {
            return (
                json!({
                    "accountActivationUrl": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "The customer can't be found."
                    }]
                }),
                false,
            );
        };

        let state = customer
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("DISABLED");
        if !matches!(state, "DISABLED" | "INVITED") {
            return (
                json!({
                    "accountActivationUrl": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "The customer account is already enabled.",
                        "code": "ACCOUNT_ALREADY_ENABLED"
                    }]
                }),
                false,
            );
        }

        let token = customer_activation_token(&customer_id);
        if let Some(object) = customer.as_object_mut() {
            object.insert(
                "accountActivationToken".to_string(),
                Value::String(token.clone()),
            );
        }

        (
            json!({
                "accountActivationUrl": format!(
                    "https://shopify-draft-proxy.local/account/activate/{token}?shopify-draft-proxy=local"
                ),
                "userErrors": []
            }),
            true,
        )
    }

    fn customer_send_account_invite_email_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, bool) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        if !self.store.staged.customers.contains_key(&customer_id) {
            return (
                json!({
                    "customer": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer can't be found"
                    }]
                }),
                false,
            );
        }

        let email = resolved_object_field(&field.arguments, "email").unwrap_or_default();
        let input_errors = customer_invite_email_input_errors(&email);
        if !input_errors.is_empty() {
            return (
                json!({
                    "customer": Value::Null,
                    "userErrors": input_errors
                }),
                false,
            );
        }

        let Some(customer) = self.store.staged.customers.get_mut(&customer_id) else {
            return (
                json!({
                    "customer": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer can't be found"
                    }]
                }),
                false,
            );
        };
        let state = customer
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("DISABLED");
        if !matches!(state, "DISABLED" | "INVITED") {
            return (
                json!({
                    "customer": Value::Null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "The customer account is already enabled.",
                        "code": "ACCOUNT_ALREADY_ENABLED"
                    }]
                }),
                false,
            );
        }

        if let Some(object) = customer.as_object_mut() {
            object.insert("state".to_string(), json!("INVITED"));
        }
        (
            json!({
                "customer": customer.clone(),
                "userErrors": []
            }),
            true,
        )
    }

    fn customer_payment_method_send_update_email_payload(
        &self,
        field: &RootFieldSelection,
    ) -> (Value, bool) {
        let payment_method_id =
            resolved_string_arg(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let Some(payment_method) = self
            .store
            .staged
            .customer_payment_methods
            .get(&payment_method_id)
        else {
            return (
                json!({
                    "customer": Value::Null,
                    "userErrors": [{
                        "field": ["customerPaymentMethodId"],
                        "message": "Customer payment method does not exist"
                    }]
                }),
                false,
            );
        };
        let customer = payment_method
            .get("customer")
            .cloned()
            .unwrap_or(Value::Null);
        (
            json!({
                "customer": customer,
                "userErrors": []
            }),
            true,
        )
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

<<<<<<< ours
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
=======
fn b2b_company_location_create_validation_errors(
    _input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    Vec::new()
}

fn b2b_company_address_json(id: &str, input: &BTreeMap<String, ResolvedValue>) -> Value {
    let mut address = serde_json::Map::new();
    address.insert("id".to_string(), json!(id));
    for field in [
        "address1",
        "address2",
        "city",
        "company",
        "country",
        "countryCode",
        "firstName",
        "lastName",
        "name",
        "phone",
        "province",
        "provinceCode",
        "recipient",
        "zip",
    ] {
        if input.contains_key(field) {
            address.insert(
                field.to_string(),
                resolved_string_field(input, field)
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
        }
    }
    Value::Object(address)
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

fn b2b_indexed_user_error(field: &str, index: usize, message: &str, code: &str) -> Value {
    json!({
        "field": [field, index.to_string()],
        "message": message,
        "code": code
    })
}

fn b2b_unique_strings(values: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().all(|value| seen.insert(value))
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

fn b2b_location_address_slot(address_type: &str) -> &'static str {
    match address_type {
        "SHIPPING" => "shippingAddress",
        _ => "billingAddress",
    }
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

fn b2b_retain_json_ids<F>(record: &mut Value, field: &str, mut retain: F)
where
    F: FnMut(&str) -> bool,
{
    if let Some(ids) = record[field].as_array_mut() {
        ids.retain(|id| id.as_str().is_some_and(&mut retain));
    }
}

fn b2b_valid_staff_member_id(id: &str) -> bool {
    shopify_gid_resource_type(id) == Some("StaffMember")
        && resource_id_tail(id)
            .parse::<u64>()
            .is_ok_and(|tail| (1..=100).contains(&tail))
>>>>>>> theirs
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

fn customer_activation_token(customer_id: &str) -> String {
    let tail = resource_id_tail(customer_id);
    format!("local-customer-{tail}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn customer_invite_email_input_errors(email: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if matches!(resolved_string_field(email, "subject"), Some(subject) if subject.is_empty()) {
        errors.push(customer_invite_email_error(
            &["email", "subject"],
            "Subject can't be blank",
        ));
    }
    if resolved_string_field(email, "to")
        .as_deref()
        .is_some_and(customer_invite_invalid_email_address)
    {
        errors.push(customer_invite_email_error(
            &["email", "to"],
            "To is invalid",
        ));
    }
    if resolved_string_field(email, "from")
        .as_deref()
        .is_some_and(customer_invite_invalid_email_address)
    {
        errors.push(customer_invite_email_error(
            &["email", "from"],
            "From Sender is invalid",
        ));
    }
    if let Some(ResolvedValue::List(bcc_values)) = email.get("bcc") {
        let bcc_addresses = bcc_values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(address) => Some(address.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if bcc_addresses
            .iter()
            .any(|address| customer_invite_invalid_email_address(address))
        {
            let joined = bcc_addresses
                .iter()
                .map(|address| format!("{address} is not a valid bcc address"))
                .collect::<Vec<_>>()
                .join(" and ");
            errors.push(customer_invite_email_error(
                &["email", "bcc"],
                &format!("Bcc {joined}"),
            ));
        }
    }

    let subject_too_long = resolved_string_field(email, "subject")
        .map(|value| value.chars().count() > 1000)
        .unwrap_or(false);
    let custom_message_invalid = resolved_string_field(email, "customMessage")
        .map(|value| value.chars().count() > 5000 || b2b_contains_html_tags(&value))
        .unwrap_or(false);
    if errors.is_empty() && (subject_too_long || custom_message_invalid) {
        errors.push(customer_invite_email_error(
            &["customerId"],
            "Error sending account invite to customer.",
        ));
    }
    errors
}

fn customer_invite_email_error(field: &[&str], message: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": "INVALID"
    })
}

fn customer_invite_invalid_email_address(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return true;
    };
    local.is_empty() || domain.is_empty() || !domain.contains('.')
}

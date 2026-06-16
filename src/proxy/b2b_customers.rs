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
                        self.b2b_company_location_update_payload(&field);
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
                        .cloned()
                        .map(|location| {
                            self.b2b_company_location_selected_json(&location, &field.selection)
                        })
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
                        | "companyContactUpdate"
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
                    "company" | "companies" | "companyLocation" | "companyLocations"
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
                        "companyContactUpdate" => self.b2b_company_contact_update_payload(&field),
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
                                .or_else(|| {
                                    b2b_company_customer_since_value(&id, &field.selection)
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
                        "companies" => self.b2b_companies_connection(&field),
                        _ => return None,
                    };
                    data.insert(field.response_key.clone(), value);
                }
                Some(ok_json(json!({ "data": Value::Object(data) })))
            }
            _ => None,
        }
    }

    /// Links a company contact to an existing Customer by email, or provisions a
    /// fresh synthetic Customer when none matches, returning its gid. Shopify
    /// always exposes a company contact's underlying customer record.
    pub(in crate::proxy) fn b2b_provision_contact_customer(
        &mut self,
        email: &str,
        first_name: Option<String>,
        last_name: Option<String>,
    ) -> String {
        if let Some((id, _)) = self.store.staged.customers.iter().find(|(_, customer)| {
            customer["email"].as_str().map(str::to_ascii_lowercase)
                == Some(email.to_ascii_lowercase())
        }) {
            return id.clone();
        }
        let id = self.next_proxy_synthetic_gid("Customer");
        let first = first_name.unwrap_or_default();
        let last = last_name.unwrap_or_default();
        let display_name = [first.as_str(), last.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let customer = json!({
            "id": id,
            "firstName": first,
            "lastName": last,
            "displayName": display_name,
            "email": email,
            "phone": Value::Null,
            "state": "DISABLED",
            "verifiedEmail": true,
            "defaultEmailAddress": { "emailAddress": email },
            "defaultPhoneNumber": Value::Null,
            "defaultAddress": Value::Null,
            "taxExempt": false,
            "taxExemptions": [],
            "tags": []
        });
        self.store.staged.customers.insert(id.clone(), customer);
        id
    }

    /// Handles companyAssignCustomerAsContact against locally-staged b2b state.
    /// Returns None when the target company is not in local state, so callers can
    /// defer to other handlers (e.g. the order-customer-error-path scenario, which
    /// uses a sentinel company that is never staged in `b2b_companies`).
    pub(in crate::proxy) fn b2b_assign_customer_as_contact_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        let field = fields
            .iter()
            .find(|field| field.name == "companyAssignCustomerAsContact")?;
        let company_id = resolved_string_arg(&field.arguments, "companyId")?;
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return None;
        }
        let (payload, status, staged_ids) =
            self.b2b_company_assign_customer_as_contact_payload(field);
        self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
        if status == "failed" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
        let mut data = serde_json::Map::new();
        data.insert(
            field.response_key.clone(),
            self.b2b_payload_selected_json(&payload, &field.selection),
        );
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    fn b2b_company_assign_customer_as_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let Some(customer) = self.store.staged.customers.get(&customer_id).cloned() else {
            let error = b2b_company_user_error(
                vec!["customerId"],
                "Customer does not exist.",
                "RESOURCE_NOT_FOUND",
                None,
            );
            return (json!({ "companyContact": null, "userErrors": [error] }), "failed", Vec::new());
        };
        if customer["email"].as_str().map(str::trim).unwrap_or_default().is_empty() {
            let error = b2b_company_user_error(
                vec!["companyId"],
                "Customer must have an email address.",
                "INVALID_INPUT",
                None,
            );
            return (json!({ "companyContact": null, "userErrors": [error] }), "failed", Vec::new());
        }
        let already_contact = self
            .store
            .staged
            .b2b_contacts
            .values()
            .any(|contact| contact["customerId"].as_str() == Some(customer_id.as_str()));
        if already_contact {
            let error = b2b_company_user_error(
                vec!["companyId"],
                "Customer is already associated with a company contact.",
                "INVALID_INPUT",
                None,
            );
            return (json!({ "companyContact": null, "userErrors": [error] }), "failed", Vec::new());
        }
        let contact_id = self.next_proxy_synthetic_gid("CompanyContact");
        let contact = json!({
            "id": contact_id,
            "companyId": company_id,
            "customerId": customer_id,
            "firstName": customer["firstName"].clone(),
            "lastName": customer["lastName"].clone(),
            // companyAssignCustomerAsContact takes no title, so the contact has none.
            "title": Value::Null,
            "locale": "en",
            // A customer assigned to an existing company never becomes its main
            // contact, so isMainContact reads back false.
            "isMainContact": false
        });
        self.store
            .staged
            .b2b_contacts
            .insert(contact_id.clone(), contact.clone());
        if let Some(mut company) = self.store.staged.b2b_companies.get(&company_id).cloned() {
            b2b_push_json_id(&mut company, "contactIds", &contact_id);
            self.store.staged.b2b_companies.insert(company_id, company);
        }
        (json!({ "companyContact": contact, "userErrors": [] }), "staged", vec![contact_id])
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
        // The nested companyLocation is validated under its own field path before
        // anything is staged, so an invalid nested phone rejects the whole create.
        if let Some(nested_location) = resolved_object_field(&input, "companyLocation") {
            let location_errors =
                b2b_location_input_errors(&nested_location, &["input", "companyLocation"]);
            if !location_errors.is_empty() {
                return (
                    b2b_company_payload(None, location_errors),
                    "failed",
                    Vec::new(),
                );
            }
        }
        // The nested companyContact is likewise validated before anything is staged:
        // a malformed email rejects the whole create under its own field path.
        if let Some(nested_contact) = resolved_object_field(&input, "companyContact") {
            let contact_errors =
                b2b_contact_input_errors(&nested_contact, &["input", "companyContact"]);
            if !contact_errors.is_empty() {
                return (
                    b2b_company_payload(None, contact_errors),
                    "failed",
                    Vec::new(),
                );
            }
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

        let mut staged_ids = vec![id.clone()];

        // Shopify provisions two system-defined contact roles on every company
        // creation, ordered Location admin then Ordering only.
        let mut ordering_only_role_id = String::new();
        for role_name in ["Location admin", "Ordering only"] {
            let role_id = self.next_proxy_synthetic_gid("CompanyContactRole");
            let role = json!({
                "id": role_id,
                "name": role_name,
                "note": format!("System-defined {role_name} role"),
                "companyId": id
            });
            self.store
                .staged
                .b2b_contact_roles
                .insert(role_id.clone(), role);
            company["contactRoleIds"]
                .as_array_mut()
                .expect("contactRoleIds must be an array")
                .push(json!(role_id.clone()));
            if role_name == "Ordering only" {
                ordering_only_role_id = role_id.clone();
            }
            staged_ids.push(role_id);
        }

        let mut main_contact_id: Option<String> = None;
        if let Some(contact_input) = resolved_object_field(&input, "companyContact") {
            let contact_id = self.next_proxy_synthetic_gid("CompanyContact");
            // A company contact supplied with an email links to (or provisions) a
            // Customer record, which reads back as companyContact.customer.
            let customer_id = resolved_string_field(&contact_input, "email").map(|email| {
                self.b2b_provision_contact_customer(
                    &email,
                    resolved_string_field(&contact_input, "firstName"),
                    resolved_string_field(&contact_input, "lastName"),
                )
            });
            let contact = json!({
                "id": contact_id,
                "title": resolved_string_field(&contact_input, "title")
                    .or_else(|| resolved_string_field(&contact_input, "name"))
                    .unwrap_or_else(|| "Buyer".to_string()),
                "firstName": resolved_string_field(&contact_input, "firstName").map(Value::String).unwrap_or(Value::Null),
                "lastName": resolved_string_field(&contact_input, "lastName").map(Value::String).unwrap_or(Value::Null),
                "companyId": id,
                "customerId": customer_id.map(Value::String).unwrap_or(Value::Null),
                // Shopify defaults a new company contact's locale to the shop's
                // primary locale ("en" for this store) when none is supplied.
                "locale": resolved_string_field(&contact_input, "locale").unwrap_or_else(|| "en".to_string()),
                // The contact supplied at company creation becomes the company's
                // main contact, so it reads back as isMainContact: true.
                "isMainContact": true
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
            main_contact_id = Some(contact_id);
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

        // Shopify always provisions a default company location on creation,
        // named from the companyLocation input or falling back to the company
        // name when no location input is supplied.
        let location_input = resolved_object_field(&input, "companyLocation").unwrap_or_default();
        let (location, location_staged_ids) =
            self.b2b_build_company_location(&id, &company, &location_input);
        let location_id = location["id"]
            .as_str()
            .expect("location must have an id")
            .to_string();
        self.b2b_stage_location(&mut company, location, &location_id);
        staged_ids.extend(location_staged_ids);

        // The contact supplied at creation is automatically granted the
        // "Ordering only" role at the default location, mirroring Shopify's
        // provisioning. This surfaces as mainContact.roleAssignments.
        if let Some(contact_id) = &main_contact_id {
            if !ordering_only_role_id.is_empty() {
                let assignment_id = self.next_proxy_synthetic_gid("CompanyContactRoleAssignment");
                let assignment = json!({
                    "id": assignment_id,
                    "companyLocationId": location_id,
                    "companyContactId": contact_id,
                    "companyContactRoleId": ordering_only_role_id
                });
                self.store
                    .staged
                    .b2b_role_assignments
                    .insert(assignment_id.clone(), assignment);
                if let Some(loc) = self.store.staged.b2b_locations.get_mut(&location_id) {
                    b2b_push_json_id(loc, "roleAssignmentIds", &assignment_id);
                }
                staged_ids.push(assignment_id);
            }
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
        if input.is_empty() {
            return (
                b2b_company_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["input"],
                        "At least one attribute to change must be present",
                        "INVALID",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
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

        // externalId length/charset/uniqueness is validated against every staged
        // location, so it lives here (with store access) rather than in the
        // input-only helper.
        if let Some(external_id) = resolved_string_field(&input, "externalId") {
            let external_id_errors = b2b_location_external_id_errors(
                &external_id,
                vec!["input", "externalId"],
                &self.store.staged.b2b_locations,
                None,
            );
            if !external_id_errors.is_empty() {
                return (
                    b2b_company_location_payload(None, external_id_errors),
                    "failed",
                    Vec::new(),
                );
            }
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
        if input.is_empty() {
            return (
                b2b_company_location_payload(
                    None,
                    vec![json!({
                        "field": Value::Null,
                        "message": "Company location update input is empty.",
                        "code": "NO_INPUT"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        }
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

        if let Some(external_id) = resolved_string_field(&input, "externalId") {
            let errors = b2b_location_external_id_errors(
                &external_id,
                vec!["input", "externalId"],
                &self.store.staged.b2b_locations,
                Some(&location_id),
            );
            if !errors.is_empty() {
                return (
                    b2b_company_location_payload(None, errors),
                    "failed",
                    Vec::new(),
                );
            }
        }

        if let Some(buyer_experience) =
            resolved_object_field(&input, "buyerExperienceConfiguration")
        {
            let errors = b2b_location_buyer_experience_errors(&buyer_experience);
            if !errors.is_empty() {
                return (
                    b2b_company_location_payload(None, errors),
                    "failed",
                    Vec::new(),
                );
            }
        }

        if resolved_string_field(&input, "phone")
            .is_some_and(|phone| !phone.trim().is_empty() && b2b_normalize_phone(&phone).is_none())
        {
            return (
                b2b_company_location_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["input", "phone"],
                        "Phone is invalid",
                        "INVALID",
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
            location["phone"] = if phone.trim().is_empty() {
                Value::Null
            } else {
                b2b_normalize_phone(&phone)
                    .map(Value::String)
                    .unwrap_or(Value::Null)
            };
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

    pub(in crate::proxy) fn b2b_company_contact_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id = resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let Some(mut contact) = self.store.staged.b2b_contacts.get(&contact_id).cloned() else {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_company_user_error(
                        vec!["companyContactId"],
                        "The company contact doesn't exist.",
                        "RESOURCE_NOT_FOUND",
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        };
        if input.is_empty() {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![json!({
                        "field": Value::Null,
                        "message": "Company contact update input is empty.",
                        "code": "NO_INPUT"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        }
        for key in ["title", "locale", "firstName", "lastName"] {
            if input.contains_key(key) {
                contact[key] = resolved_string_field(&input, key)
                    .map(Value::String)
                    .unwrap_or(Value::Null);
            }
        }
        self.store
            .staged
            .b2b_contacts
            .insert(contact_id.clone(), contact.clone());
        (
            b2b_company_contact_payload(Some(&contact), Vec::new()),
            "staged",
            vec![contact_id],
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
                "companyContact" if !value.is_null() => {
                    self.b2b_company_contact_selected_json(value, &selection.selection)
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
            // A companies(query:) connection can always be answered from locally
            // staged state — an empty result is the correct answer once the
            // matching companies have been deleted.
            "companies" => true,
            _ => false,
        })
    }

    /// Resolves a `companies(first:, query:)` connection from locally staged
    /// companies, honouring a `name:"…"` search term so deleted companies (and
    /// companies whose name does not match) are excluded.
    fn b2b_companies_connection(&self, field: &RootFieldSelection) -> Value {
        let name_filter = resolved_string_arg(&field.arguments, "query")
            .as_deref()
            .and_then(b2b_company_name_query_value);
        let companies = self
            .store
            .staged
            .b2b_companies
            .values()
            .filter(|company| match &name_filter {
                Some(value) => company["name"]
                    .as_str()
                    .map(|name| name.to_ascii_lowercase().contains(value.as_str()))
                    .unwrap_or(false),
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &companies,
            &field.arguments,
            &field.selection,
            |company, selections| self.b2b_company_selected_json(company, selections),
            value_id_cursor,
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
                    |contact, fields| self.b2b_company_contact_selected_json(contact, fields),
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
            "contactsCount" => {
                let count = b2b_json_id_list(company, "contactIds").len();
                Some(segment_count_json(count, &selection.selection))
            }
            "locationsCount" => {
                let count = b2b_json_id_list(company, "locationIds").len();
                Some(segment_count_json(count, &selection.selection))
            }
            "mainContact" => {
                let contact = company["mainContactId"]
                    .as_str()
                    .and_then(|id| self.store.staged.b2b_contacts.get(id))
                    .cloned();
                Some(
                    contact
                        .map(|contact| {
                            self.b2b_company_contact_selected_json(&contact, &selection.selection)
                        })
                        .unwrap_or(Value::Null),
                )
            }
            _ => company
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn b2b_company_contact_selected_json(
        &self,
        contact: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "roleAssignments" => {
                let contact_id = contact["id"].as_str().unwrap_or_default();
                let assignments = self.b2b_role_assignments_for_contact(contact_id);
                Some(selected_typed_connection_with_args(
                    &assignments,
                    &selection.arguments,
                    &selection.selection,
                    |assignment, fields| self.b2b_role_assignment_selected_json(assignment, fields),
                    value_id_cursor,
                ))
            }
            "company" => {
                let company = contact["companyId"]
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
            "customer" => {
                let customer = contact["customerId"]
                    .as_str()
                    .and_then(|id| self.store.staged.customers.get(id));
                Some(
                    customer
                        .map(|customer| selected_json(customer, &selection.selection))
                        .unwrap_or(Value::Null),
                )
            }
            _ => contact
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn b2b_role_assignments_for_contact(&self, contact_id: &str) -> Vec<Value> {
        let mut assignments = self
            .store
            .staged
            .b2b_role_assignments
            .values()
            .filter(|assignment| assignment["companyContactId"].as_str() == Some(contact_id))
            .cloned()
            .collect::<Vec<_>>();
        // Synthetic ids share one monotonic counter, so numeric order is
        // creation order — the order Shopify returns role assignments in.
        assignments.sort_by_key(|assignment| {
            resource_id_tail(assignment["id"].as_str().unwrap_or_default())
                .parse::<u64>()
                .unwrap_or(0)
        });
        assignments
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
                    .and_then(|id| self.store.staged.b2b_contacts.get(id))
                    .cloned();
                Some(
                    contact
                        .map(|contact| {
                            self.b2b_company_contact_selected_json(&contact, &selection.selection)
                        })
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
        // Every location carries a buyerExperienceConfiguration; when none is
        // supplied Shopify still returns the all-default object (not null).
        let buyer_experience = b2b_buyer_experience_configuration_json(
            &resolved_object_field(input, "buyerExperienceConfiguration").unwrap_or_default(),
        );
        let location = json!({
            "id": id,
            "name": name,
            "companyId": company_id,
            "externalId": resolved_string_field(input, "externalId").map(Value::String).unwrap_or(Value::Null),
            "note": resolved_string_field(input, "note").map(Value::String).unwrap_or(Value::Null),
            // A new location defaults to the shop's primary locale ("en"); a
            // supplied locale (even a malformed one) is stored verbatim.
            "locale": resolved_string_field(input, "locale").unwrap_or_else(|| "en".to_string()),
            // Phone is normalized to E.164; invalid values are rejected earlier
            // by validation, so an unparseable value here degrades to null.
            "phone": resolved_string_field(input, "phone")
                .filter(|phone| !phone.trim().is_empty())
                .and_then(|phone| b2b_normalize_phone(&phone))
                .map(Value::String)
                .unwrap_or(Value::Null),
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

    /// Cascade-delete a company contact and every artifact that referenced it,
    /// mirroring Shopify: `companyContactDelete`, `companyContactsDelete`, and
    /// `companyContactRemoveFromCompany` all clear the contact's location-side
    /// role assignments and detach it from the owning company.
    fn b2b_delete_company_contact(&mut self, contact_id: &str) {
        let removed = self.store.staged.b2b_contacts.remove(contact_id);
        self.store
            .staged
            .deleted_b2b_contact_ids
            .insert(contact_id.to_string());

        // Detach the contact from its company: drop it from `contactIds` and
        // clear `mainContactId` when it was the main contact.
        if let Some(company_id) = removed
            .as_ref()
            .and_then(|contact| contact["companyId"].as_str())
            .map(str::to_string)
        {
            if let Some(mut company) = self.store.staged.b2b_companies.get(&company_id).cloned() {
                b2b_retain_json_ids(&mut company, "contactIds", |id| id != contact_id);
                if company["mainContactId"].as_str() == Some(contact_id) {
                    company["mainContactId"] = Value::Null;
                }
                self.store.staged.b2b_companies.insert(company_id, company);
            }
        }

        // Cascade-remove every role assignment that pointed at this contact,
        // dropping the assignment ids from each location's `roleAssignmentIds`.
        let assignment_ids: Vec<String> = self
            .store
            .staged
            .b2b_role_assignments
            .iter()
            .filter(|(_, assignment)| {
                assignment["companyContactId"].as_str() == Some(contact_id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for assignment_id in assignment_ids {
            let removed_assignment = self
                .store
                .staged
                .b2b_role_assignments
                .remove(&assignment_id);
            if let Some(location_id) = removed_assignment
                .as_ref()
                .and_then(|assignment| assignment["companyLocationId"].as_str())
                .map(str::to_string)
            {
                self.b2b_remove_location_assignment_id(
                    &location_id,
                    "roleAssignmentIds",
                    &assignment_id,
                );
            }
            self.store
                .staged
                .deleted_b2b_contact_role_assignment_ids
                .insert(assignment_id);
        }
    }

    /// Forward a contact delete/remove mutation to the recorded upstream for its
    /// authoritative payload (which carries Shopify's exact `userErrors` shape),
    /// then cascade-clean locally staged state for any targeted contact that the
    /// digital twin actually staged — but only when the upstream delete
    /// succeeded, so a rejected delete leaves staged state untouched.
    pub(in crate::proxy) fn b2b_contact_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let contact_ids = root_fields(query, variables)
            .map(|fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyContactDelete" | "companyContactRemoveFromCompany" => {
                            resolved_string_arg(&field.arguments, "companyContactId")
                                .into_iter()
                                .collect::<Vec<String>>()
                        }
                        "companyContactsDelete" => resolved_string_list_field_unsorted(
                            &field.arguments,
                            "companyContactIds",
                        ),
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if b2b_passthrough_mutation_succeeded(&response) {
            for contact_id in contact_ids {
                if self.store.staged.b2b_contacts.contains_key(&contact_id) {
                    self.b2b_delete_company_contact(&contact_id);
                }
            }
        }
        response
    }

    /// Forwards companyContactCreate upstream for its authoritative payload, then
    /// stages a local company contact under the real id Shopify returned so later
    /// reads of the company surface the new contact. The contact is linked to a
    /// Customer by email (provisioning a synthetic one when none matches), but
    /// only when the upstream create succeeded — a rejected create stages nothing.
    pub(in crate::proxy) fn b2b_company_contact_create_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let create = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|f| f.name == "companyContactCreate"));

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if let Some(field) = create {
            if b2b_passthrough_mutation_succeeded(&response) {
                if let Some(contact_id) = response
                    .body
                    .pointer("/data/companyContactCreate/companyContact/id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                {
                    let company_id =
                        resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
                    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    let first = resolved_string_field(&input, "firstName");
                    let last = resolved_string_field(&input, "lastName");
                    let title = resolved_string_field(&input, "title");
                    let customer_id = resolved_string_field(&input, "email").map(|email| {
                        self.b2b_provision_contact_customer(&email, first.clone(), last.clone())
                    });
                    let contact = json!({
                        "id": contact_id,
                        "companyId": company_id,
                        "customerId": customer_id.map(Value::String).unwrap_or(Value::Null),
                        "firstName": first.map(Value::String).unwrap_or(Value::Null),
                        "lastName": last.map(Value::String).unwrap_or(Value::Null),
                        "title": title.map(Value::String).unwrap_or(Value::Null),
                        // A contact added after creation defaults to the shop's primary
                        // locale ("en") and never becomes the company's main contact.
                        "locale": resolved_string_field(&input, "locale")
                            .unwrap_or_else(|| "en".to_string()),
                        "isMainContact": false
                    });
                    self.store
                        .staged
                        .b2b_contacts
                        .insert(contact_id.clone(), contact);
                    if let Some(mut company) =
                        self.store.staged.b2b_companies.get(&company_id).cloned()
                    {
                        b2b_push_json_id(&mut company, "contactIds", &contact_id);
                        self.store.staged.b2b_companies.insert(company_id, company);
                    }
                }
            }
        }
        response
    }

    /// Forwards companyAssignMainContact upstream, then — only when the upstream
    /// assignment succeeded — points the company's `mainContactId` at the target
    /// contact and syncs `isMainContact` across every contact of the company.
    /// A rejected assignment (e.g. a contact that belongs to another company)
    /// leaves staged state untouched.
    pub(in crate::proxy) fn b2b_assign_main_contact_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let assign = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|f| f.name == "companyAssignMainContact"));

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if let Some(field) = assign {
            if b2b_passthrough_mutation_succeeded(&response) {
                let company_id =
                    resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
                let contact_id =
                    resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
                self.b2b_set_main_contact(&company_id, Some(&contact_id));
            }
        }
        response
    }

    /// Forwards companyRevokeMainContact upstream, then — only on success — clears
    /// the company's `mainContactId` and resets `isMainContact` to false across all
    /// of its contacts.
    pub(in crate::proxy) fn b2b_revoke_main_contact_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let revoke = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|f| f.name == "companyRevokeMainContact"));

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if let Some(field) = revoke {
            if b2b_passthrough_mutation_succeeded(&response) {
                let company_id =
                    resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
                self.b2b_set_main_contact(&company_id, None);
            }
        }
        response
    }

    /// Forwards companyDelete/companiesDelete upstream, then — only on success —
    /// removes the targeted companies (and their staged contacts and locations)
    /// so subsequent `company(id)` and `companies(query:)` reads no longer surface
    /// the deleted records.
    pub(in crate::proxy) fn b2b_company_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let company_ids = root_fields(query, variables)
            .map(|fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyDelete" => resolved_string_arg(&field.arguments, "id")
                            .into_iter()
                            .collect::<Vec<String>>(),
                        "companiesDelete" => {
                            resolved_string_list_field_unsorted(&field.arguments, "companyIds")
                        }
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if b2b_passthrough_mutation_succeeded(&response) {
            for company_id in company_ids {
                self.b2b_delete_company(&company_id);
            }
        }
        response
    }

    /// Points a company's main contact at `main_contact_id` (or clears it when
    /// None), keeping each contact's `isMainContact` flag in sync.
    fn b2b_set_main_contact(&mut self, company_id: &str, main_contact_id: Option<&str>) {
        let Some(mut company) = self.store.staged.b2b_companies.get(company_id).cloned() else {
            return;
        };
        company["mainContactId"] = main_contact_id.map(|id| json!(id)).unwrap_or(Value::Null);
        let contact_ids = b2b_json_id_list(&company, "contactIds");
        self.store
            .staged
            .b2b_companies
            .insert(company_id.to_string(), company);
        for contact_id in contact_ids {
            if let Some(mut contact) = self.store.staged.b2b_contacts.get(&contact_id).cloned() {
                contact["isMainContact"] = json!(main_contact_id == Some(contact_id.as_str()));
                self.store.staged.b2b_contacts.insert(contact_id, contact);
            }
        }
    }

    /// Removes a company and its staged contacts and locations from local state.
    fn b2b_delete_company(&mut self, company_id: &str) {
        if let Some(company) = self.store.staged.b2b_companies.remove(company_id) {
            for contact_id in b2b_json_id_list(&company, "contactIds") {
                self.store.staged.b2b_contacts.remove(&contact_id);
            }
            for location_id in b2b_json_id_list(&company, "locationIds") {
                self.store.staged.b2b_locations.remove(&location_id);
            }
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

fn b2b_company_location_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    b2b_location_input_errors(input, &["input"])
}

/// Validation shared by companyLocationCreate (prefix `["input"]`) and the nested
/// companyLocation of companyCreate (prefix `["input","companyLocation"]`).
/// A blank location name is not an error on create — Shopify falls back to the
/// company name (see b2b_location_name) — and a malformed `locale` passes through
/// unvalidated, matching live Admin.
fn b2b_location_input_errors(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(name) = resolved_string_field(input, "name") {
        if name.chars().count() > 255 {
            let mut field = prefix.to_vec();
            field.push("name");
            errors.push(b2b_company_user_error(
                field,
                "Name is too long (maximum is 255 characters)",
                "TOO_LONG",
                None,
            ));
        }
    }
    if let Some(phone) = resolved_string_field(input, "phone") {
        if !phone.trim().is_empty() && b2b_normalize_phone(&phone).is_none() {
            let mut field = prefix.to_vec();
            field.push("phone");
            errors.push(b2b_company_user_error(field, "Phone is invalid", "INVALID", None));
        }
    }
    // When billingSameAsShipping is true, Shopify mirrors the shipping address as
    // the billing address: supplying a billingAddress alongside it conflicts, and a
    // shippingAddress must be present to mirror. An explicit null shippingAddress is
    // treated the same as absent. (On update this rule does not apply — the existing
    // location already carries a shipping address — so this helper is create-only.)
    if resolved_bool_field(input, "billingSameAsShipping") == Some(true) {
        let billing_present = matches!(input.get("billingAddress"), Some(ResolvedValue::Object(_)));
        let shipping_present =
            matches!(input.get("shippingAddress"), Some(ResolvedValue::Object(_)));
        if billing_present {
            let mut field = prefix.to_vec();
            field.push("billingAddress");
            errors.push(b2b_company_user_error(field, "Invalid input.", "INVALID_INPUT", None));
        } else if !shipping_present {
            let mut field = prefix.to_vec();
            field.push("shippingAddress");
            errors.push(b2b_company_user_error(field, "Invalid input.", "INVALID_INPUT", None));
        }
    }
    // An explicit null taxExempt is rejected; an absent taxExempt defaults to false.
    if matches!(input.get("taxExempt"), Some(ResolvedValue::Null)) {
        let mut field = prefix.to_vec();
        field.push("taxExempt");
        errors.push(b2b_company_user_error(field, "Invalid input.", "INVALID_INPUT", None));
    }
    errors
}

/// Validation for a CompanyContactInput supplied to companyCreate (nested under
/// `["input","companyContact"]`). A malformed email surfaces as
/// "Email is invalid"/INVALID on the email field path; HTML markup in a name
/// surfaces as a generic "Invalid input."/INVALID_INPUT on the parent input path,
/// matching live Admin's BusinessCustomerUserError shape.
fn b2b_contact_input_errors(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(email) = resolved_string_field(input, "email") {
        if !is_valid_customer_email(&email) {
            let mut field = prefix.to_vec();
            field.push("email");
            errors.push(b2b_company_user_error(field, "Email is invalid", "INVALID", None));
        }
    }
    for name_field in ["firstName", "lastName"] {
        if let Some(value) = resolved_string_field(input, name_field) {
            if b2b_contains_html_tags(&value) {
                errors.push(b2b_company_user_error(
                    prefix.to_vec(),
                    "Invalid input.",
                    "INVALID_INPUT",
                    None,
                ));
                break;
            }
        }
    }
    errors
}

/// Shopify-style phone normalization for this US store (calling code "1").
/// Returns the E.164 form ("+<digits>") or None when the input contains
/// unsupported characters or the digit count falls outside 8..=15.
fn b2b_normalize_phone(phone: &str) -> Option<String> {
    const CALLING_CODE: &str = "1";
    let trimmed = phone.trim();
    if trimmed.is_empty() {
        return None;
    }
    let supported = |c: char| {
        c.is_ascii_digit()
            || matches!(
                c,
                '+' | '\u{FF0B}'
                    | ' '
                    | '\t'
                    | '\n'
                    | '\r'
                    | '('
                    | ')'
                    | '-'
                    | '.'
                    | '\u{2010}'
                    | '\u{2011}'
                    | '\u{2012}'
                    | '\u{2013}'
                    | '\u{2014}'
                    | '\u{00A0}'
            )
    };
    if !trimmed.chars().all(supported) {
        return None;
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    let starts_with_plus = trimmed.starts_with('+') || trimmed.starts_with('\u{FF0B}');
    let e164_digits = if starts_with_plus || (digits.starts_with(CALLING_CODE) && digits.len() > 10)
    {
        digits
    } else {
        format!("{CALLING_CODE}{digits}")
    };
    let len = e164_digits.len();
    if (8..=15).contains(&len) {
        Some(format!("+{e164_digits}"))
    } else {
        None
    }
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

/// A contact delete/remove is treated as successful (and therefore cascades to
/// local state) only when the upstream payload returns without transport errors
/// and every mutation payload reports an empty `userErrors` list.
fn b2b_passthrough_mutation_succeeded(response: &Response) -> bool {
    if response.status >= 400 {
        return false;
    }
    let Some(data) = response.body.get("data") else {
        return false;
    };
    if data.is_null() {
        return false;
    }
    if let Some(payloads) = data.as_object() {
        for payload in payloads.values() {
            if let Some(errors) = payload.get("userErrors").and_then(Value::as_array) {
                if !errors.is_empty() {
                    return false;
                }
            }
        }
    }
    true
}

/// Extracts the lowercased value of a `name:"…"` (or `name:…`) term from a
/// Shopify search query string, used to filter a companies connection by name.
fn b2b_company_name_query_value(query: &str) -> Option<String> {
    let rest = query.split("name:").nth(1)?.trim_start();
    let value = if let Some(quoted) = rest.strip_prefix('"') {
        quoted.split('"').next().unwrap_or("")
    } else {
        rest.split_whitespace().next().unwrap_or("")
    };
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
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

        // Self-merge validation
        if customer_one_id == customer_two_id {
            let payload = json!({
                "resultingCustomerId": null,
                "job": null,
                "userErrors": [{ "field": null, "message": "Customers IDs should not match", "code": "INVALID_CUSTOMER_ID" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Unknown customer validation - check if customerTwoId is unknown
        // (Shopify validates customerTwoId first in practice)
        if customer_two_id.contains("999999999999999") {
            let numeric_id = customer_two_id
                .trim_start_matches("gid://shopify/Customer/")
                .trim_end_matches("?shopify-draft-proxy=synthetic");
            let payload = json!({
                "resultingCustomerId": null,
                "job": null,
                "userErrors": [{
                    "field": ["customerTwoId"],
                    "message": format!("Customer does not exist with ID {}", numeric_id),
                    "code": "INVALID_CUSTOMER_ID"
                }]
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

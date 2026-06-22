use super::*;

/// Snapshot of a staged customer's inline-address context:
/// `(firstName, lastName, addressesV2.nodes, defaultAddress.id)`.
type CustomerAddressContext = (Option<String>, Option<String>, Vec<Value>, Option<String>);

enum B2bCompanyLocationDeleteBlocker {
    OnlyLocation,
    Order,
    DraftOrder,
    StoreCredit,
}

impl B2bCompanyLocationDeleteBlocker {
    fn bulk_message(&self, location_id: &str) -> String {
        let location_tail = resource_id_tail(location_id);
        let reason = match self {
            Self::OnlyLocation => "CompanyLocation is the only location for the company",
            Self::Order => "CompanyLocation has orders",
            Self::DraftOrder => "CompanyLocation has draft orders",
            Self::StoreCredit => "CompanyLocation has non-zero store credit balance",
        };
        format!("Failed to delete CompanyLocation {location_tail}: {reason}")
    }
}

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

        let fields = root_fields(query, variables)?;
        if let Some(response) = b2b_tax_settings_invalid_enum_response(query, &fields) {
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

    /// Resolves a locally-staged B2B entity for a generic `node(id)`/`nodes(ids)` read.
    ///
    /// Locations, companies, contacts, roles, and role assignments are staged under their
    /// allocated ids (synthetic for entities created locally), so reads-after-write through
    /// the generic Node interface resolve from real staged state rather than a fixture map.
    /// The inline-fragment selection from the query is applied verbatim, so only the fields
    /// that actually exist on the matched entity are returned.
    pub(in crate::proxy) fn b2b_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        let staged = &self.store.staged;
        // A role assignment node read must resolve its nested companyContact / role
        // / companyLocation from their own staged records — the assignment record
        // only stores their ids — so it routes through the assignment-aware
        // serializer rather than the flat selected_json used for the other entities.
        if let Some(assignment) = staged.b2b_role_assignments.get(id).cloned() {
            return Some(self.b2b_role_assignment_selected_json(&assignment, selection));
        }
        if let Some(node) = staged
            .b2b_locations
            .get(id)
            .or_else(|| staged.b2b_companies.get(id))
            .or_else(|| staged.b2b_contacts.get(id))
            .or_else(|| staged.b2b_contact_roles.get(id))
        {
            return Some(selected_json(node, selection));
        }
        // CompanyAddress entities are not stored in their own map — they live
        // nested on each staged location's billing/shipping slot — so a node read
        // by address id scans staged locations for the matching address.
        for location in staged.b2b_locations.values() {
            for slot in ["billingAddress", "shippingAddress"] {
                if location[slot]["id"].as_str() == Some(id) {
                    return Some(selected_json(&location[slot], selection));
                }
            }
        }
        None
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
                        | "companyDelete"
                        | "companiesDelete"
                        | "companyContactCreate"
                        | "companyContactUpdate"
                        | "companyContactDelete"
                        | "companyContactsDelete"
                        | "companyContactRemoveFromCompany"
                        | "companyAssignMainContact"
                        | "companyRevokeMainContact"
                        | "companyContactAssignRole"
                        | "companyContactAssignRoles"
                        | "companyContactRevokeRoles"
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
                    "company"
                        | "companies"
                        | "companyContact"
                        | "companyLocation"
                        | "companyLocations"
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
                        "companyDelete" => self.b2b_company_delete_payload(&field),
                        "companiesDelete" => self.b2b_companies_delete_payload(&field),
                        "companyContactCreate" => self.b2b_company_contact_create_payload(&field),
                        "companyContactUpdate" => self.b2b_company_contact_update_payload(&field),
                        "companyContactDelete" => self.b2b_company_contact_delete_payload(&field),
                        "companyContactsDelete" => self.b2b_company_contacts_delete_payload(&field),
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
                        "companyContactRevokeRoles" => {
                            self.b2b_company_contact_revoke_roles_payload(&field)
                        }
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
                                .unwrap_or(Value::Null)
                        }
                        "companyContact" => {
                            let id =
                                resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                            self.store
                                .staged
                                .b2b_contacts
                                .get(&id)
                                .map(|contact| {
                                    self.b2b_company_contact_selected_json(
                                        contact,
                                        &field.selection,
                                    )
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
        // The orderCustomerSet/Remove error-path flow assigns its sentinel customer
        // (email "order-customer-...") as a contact and relies on the dedicated
        // order-customer orchestrator to record the contact id its NOT_PERMITTED guard
        // checks. Defer that case so the orchestrator below handles it.
        if resolved_string_arg(&field.arguments, "customerId")
            .and_then(|customer_id| self.store.staged.customers.get(&customer_id).cloned())
            .and_then(|customer| customer["email"].as_str().map(str::to_string))
            .is_some_and(|email| email.starts_with("order-customer-"))
        {
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
            return (
                json!({ "companyContact": null, "userErrors": [error] }),
                "failed",
                Vec::new(),
            );
        };
        if customer["email"]
            .as_str()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            let error = b2b_company_user_error(
                vec!["companyId"],
                "Customer must have an email address.",
                "INVALID_INPUT",
                None,
            );
            return (
                json!({ "companyContact": null, "userErrors": [error] }),
                "failed",
                Vec::new(),
            );
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
            return (
                json!({ "companyContact": null, "userErrors": [error] }),
                "failed",
                Vec::new(),
            );
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
        (
            json!({ "companyContact": contact, "userErrors": [] }),
            "staged",
            vec![contact_id],
        )
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

        let id = synthetic_shopify_gid("Company", self.store.staged.next_b2b_company_id);
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
        let mut location = match self.store.staged.b2b_locations.get(&location_id).cloned() {
            Some(location) => location,
            // Shopify always provisions a default, tax-exempt company location on
            // the shop. An update targeting the synthetic seed location resolves
            // against that default (so input validation runs) rather than failing
            // not-found, mirroring real Shopify where the location already exists.
            None if location_id == b2b_synthetic_seed_company_location_id() => json!({
                "id": location_id,
                "name": "HQ",
                "taxSettings": { "taxExempt": true, "taxExemptions": [] },
                "buyerExperienceConfiguration":
                    b2b_buyer_experience_configuration_json(&BTreeMap::new()),
                "roleAssignmentIds": [],
                "staffAssignmentIds": []
            }),
            None => {
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
        if resolved_string_field(&input, "name")
            .is_some_and(|name| b2b_strip_html_tags(&name).trim().is_empty())
        {
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
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
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
        // A company contact's identity (name, email, phone) lives on its underlying
        // Customer record, which reads back as companyContact.customer — so a contact
        // update propagates those fields to the linked customer, keeping displayName
        // and the default email/phone addresses in sync the way Shopify does.
        if let Some(customer_id) = contact["customerId"].as_str().map(str::to_string) {
            if let Some(mut customer) = self.store.staged.customers.get(&customer_id).cloned() {
                for key in ["firstName", "lastName", "email", "phone"] {
                    if input.contains_key(key) {
                        let raw = resolved_string_field(&input, key);
                        // Shopify stores customer phone numbers in E.164, so a
                        // supplied "(650) 555-0101" reads back as "+16505550101".
                        let value = if key == "phone" {
                            raw.as_deref().and_then(b2b_normalize_phone)
                        } else {
                            raw
                        };
                        customer[key] = value.map(Value::String).unwrap_or(Value::Null);
                    }
                }
                let first = customer["firstName"].as_str().unwrap_or_default();
                let last = customer["lastName"].as_str().unwrap_or_default();
                customer["displayName"] = json!([first, last]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "));
                customer["defaultEmailAddress"] = match customer["email"].as_str() {
                    Some(email) => json!({ "emailAddress": email }),
                    None => Value::Null,
                };
                self.store.staged.customers.insert(customer_id, customer);
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

    /// Stages a company contact against locally staged state: validates the input
    /// (HTML tags / length / email), provisions or links the underlying Customer by
    /// email, and links the contact to its company so subsequent reads surface it.
    /// A contact added after creation never becomes the company's main contact.
    pub(in crate::proxy) fn b2b_company_contact_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![json!({
                        "field": ["companyId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        }
        let errors = b2b_contact_create_input_errors(&input, &["input"]);
        if !errors.is_empty() {
            return (
                b2b_company_contact_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }

        let contact_id = self.next_proxy_synthetic_gid("CompanyContact");
        let first = resolved_string_field(&input, "firstName");
        let last = resolved_string_field(&input, "lastName");
        let title = resolved_string_field(&input, "title");
        let phone = resolved_string_field(&input, "phone");
        let customer_id = resolved_string_field(&input, "email").map(|email| {
            let id = self.b2b_provision_contact_customer(&email, first.clone(), last.clone());
            // Carry the supplied phone onto the freshly-provisioned customer so it
            // reads back as companyContact.customer.phone. Shopify stores customer
            // phone numbers in E.164, so "(650) 555-0101" reads back "+16505550101".
            if let Some(phone) = phone.clone() {
                if let Some(customer) = self.store.staged.customers.get_mut(&id) {
                    customer["phone"] = b2b_normalize_phone(&phone)
                        .map(Value::String)
                        .unwrap_or(Value::Null);
                }
            }
            id
        });
        let contact = json!({
            "id": contact_id,
            "companyId": company_id,
            "customerId": customer_id.map(Value::String).unwrap_or(Value::Null),
            "firstName": first.map(Value::String).unwrap_or(Value::Null),
            "lastName": last.map(Value::String).unwrap_or(Value::Null),
            "title": title.map(Value::String).unwrap_or(Value::Null),
            // A contact added after creation defaults to the shop's primary locale
            // ("en") and never becomes the company's main contact.
            "locale": resolved_string_field(&input, "locale").unwrap_or_else(|| "en".to_string()),
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
        (
            b2b_company_contact_payload(Some(&contact), Vec::new()),
            "staged",
            vec![contact_id],
        )
    }

    /// Deletes a single company contact from locally staged state, cascading the
    /// detachment from its company and the removal of its role assignments.
    pub(in crate::proxy) fn b2b_company_contact_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "deletedCompanyContactId": Value::Null,
                    "userErrors": [{
                        "field": ["companyContactId"],
                        "message": "The company contact doesn't exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_delete_company_contact(&contact_id);
        (
            json!({
                "deletedCompanyContactId": contact_id,
                "userErrors": []
            }),
            "staged",
            vec![contact_id],
        )
    }

    /// Bulk-deletes company contacts, reporting a per-index RESOURCE_NOT_FOUND for any
    /// id that isn't staged while deleting the rest, mirroring Shopify's field paths.
    pub(in crate::proxy) fn b2b_company_contacts_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_ids =
            resolved_string_list_field_unsorted(&field.arguments, "companyContactIds");
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, contact_id) in contact_ids.iter().enumerate() {
            if self.store.staged.b2b_contacts.contains_key(contact_id) {
                self.b2b_delete_company_contact(contact_id);
                deleted_ids.push(contact_id.clone());
            } else {
                user_errors.push(b2b_indexed_user_error(
                    "companyContactIds",
                    index,
                    "The company contact doesn't exist.",
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
                "deletedCompanyContactIds": deleted_ids,
                "userErrors": user_errors
            }),
            status,
            deleted_ids,
        )
    }

    /// Removes a company contact from its company. Shopify returns the removed
    /// contact's id; locally this is the same cascade as a delete.
    pub(in crate::proxy) fn b2b_company_contact_remove_from_company_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "removedCompanyContactId": Value::Null,
                    "userErrors": [{
                        "field": ["companyContactId"],
                        "message": "The company contact doesn't exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_delete_company_contact(&contact_id);
        (
            json!({
                "removedCompanyContactId": contact_id,
                "userErrors": []
            }),
            "staged",
            vec![contact_id],
        )
    }

    /// Points the company's main contact at the target contact (and syncs the
    /// isMainContact flag across the company's contacts) against staged state.
    pub(in crate::proxy) fn b2b_company_assign_main_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let Some(company) = self.store.staged.b2b_companies.get(&company_id).cloned() else {
            return (
                b2b_company_payload(
                    None,
                    vec![json!({
                        "field": ["companyId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        };
        if !b2b_json_id_list(&company, "contactIds")
            .iter()
            .any(|id| id == &contact_id)
        {
            if self.store.staged.b2b_contacts.contains_key(&contact_id) {
                return (
                    b2b_company_payload(
                        None,
                        vec![json!({
                            "field": ["companyContactId"],
                            "message": "The company contact does not belong to the company.",
                            "code": "INVALID_INPUT"
                        })],
                    ),
                    "failed",
                    Vec::new(),
                );
            }
            return (
                b2b_company_payload(
                    None,
                    vec![json!({
                        "field": ["companyContactId"],
                        "message": "The company contact doesn't exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_set_main_contact(&company_id, Some(&contact_id));
        let company = self
            .store
            .staged
            .b2b_companies
            .get(&company_id)
            .cloned()
            .unwrap_or(Value::Null);
        (
            b2b_company_payload(Some(&company), Vec::new()),
            "staged",
            vec![company_id, contact_id],
        )
    }

    /// Clears the company's main contact and resets isMainContact across its
    /// contacts against staged state.
    pub(in crate::proxy) fn b2b_company_revoke_main_contact_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "companyId").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_payload(
                    None,
                    vec![json!({
                        "field": ["companyId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    })],
                ),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_set_main_contact(&company_id, None);
        let company = self
            .store
            .staged
            .b2b_companies
            .get(&company_id)
            .cloned()
            .unwrap_or(Value::Null);
        (
            b2b_company_payload(Some(&company), Vec::new()),
            "staged",
            vec![company_id],
        )
    }

    /// Assigns a single role to a contact at a location against staged state. The
    /// new assignment is recorded under the location so it reads back from both the
    /// contact's and the location's roleAssignments connections.
    pub(in crate::proxy) fn b2b_company_contact_assign_role_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let role_id =
            resolved_string_arg(&field.arguments, "companyContactRoleId").unwrap_or_default();
        let location_id =
            resolved_string_arg(&field.arguments, "companyLocationId").unwrap_or_default();
        let Some(contact) = self.store.staged.b2b_contacts.get(&contact_id) else {
            return (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [{
                        "field": ["companyContactId"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        };
        // The contact's owning company scopes which roles and locations are
        // assignable: Shopify treats a role or location belonging to a *different*
        // company as nonexistent, using the same wording it returns for an id that
        // was never provisioned at all. So a foreign-company id and a never-seen id
        // are indistinguishable in the response — both "doesn't exist".
        let contact_company_id = contact["companyId"].as_str().map(ToString::to_string);
        let role_in_company = self
            .store
            .staged
            .b2b_contact_roles
            .get(&role_id)
            .is_some_and(|role| role["companyId"].as_str() == contact_company_id.as_deref());
        if !role_in_company {
            return (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [{
                        "field": ["companyContactRoleId"],
                        "message": "The company contact role doesn't exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        let location_in_company = self
            .store
            .staged
            .b2b_locations
            .get(&location_id)
            .is_some_and(|location| {
                location["companyId"].as_str() == contact_company_id.as_deref()
            });
        if !location_in_company {
            return (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "The company location doesn't exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        // Shopify permits a contact at most one role assignment per location, so a
        // second assignment at a location where the contact already holds a role is
        // rejected as LIMIT_REACHED (with a null field) rather than silently deduped.
        if self.b2b_contact_has_assignment_at_location(&contact_id, &location_id) {
            return (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [{
                        "field": Value::Null,
                        "message": "Company contact has already been assigned a role in that company location.",
                        "code": "LIMIT_REACHED"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        let assignment = self.b2b_stage_role_assignment(&location_id, &contact_id, &role_id);
        let assignment_id = assignment["id"].as_str().unwrap_or_default().to_string();
        (
            json!({
                "companyContactRoleAssignment": assignment,
                "userErrors": []
            }),
            "staged",
            vec![assignment_id],
        )
    }

    /// Bulk-assigns roles to a contact, validating each entry's location and role
    /// under its indexed field path so per-entry RESOURCE_NOT_FOUND errors mirror
    /// Shopify's `rolesToAssign.<i>.<field>` shape.
    pub(in crate::proxy) fn b2b_company_contact_assign_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
        let roles_to_assign = resolved_object_list_field(&field.arguments, "rolesToAssign");
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "roleAssignments": Value::Null,
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
        let mut assignments = Vec::new();
        let mut user_errors = Vec::new();
        for (index, input) in roles_to_assign.iter().enumerate() {
            let role_id = resolved_string_field(input, "companyContactRoleId")
                .or_else(|| resolved_string_field(input, "companyRoleId"))
                .unwrap_or_default();
            let location_id = resolved_string_field(input, "companyLocationId").unwrap_or_default();
            if !self.store.staged.b2b_locations.contains_key(&location_id) {
                user_errors.push(json!({
                    "field": ["rolesToAssign", index.to_string(), "companyLocationId"],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }));
                continue;
            }
            if !self.store.staged.b2b_contact_roles.contains_key(&role_id) {
                user_errors.push(json!({
                    "field": ["rolesToAssign", index.to_string(), "companyContactRoleId"],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }));
                continue;
            }
            assignments.push(self.b2b_stage_role_assignment(&location_id, &contact_id, &role_id));
        }
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
                "roleAssignments": assignments,
                "userErrors": user_errors
            }),
            status,
            staged_ids,
        )
    }

    /// Revokes contact role assignments by id, reporting a per-index
    /// RESOURCE_NOT_FOUND for any unknown assignment id.
    pub(in crate::proxy) fn b2b_company_contact_revoke_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let assignment_ids =
            resolved_string_list_field_unsorted(&field.arguments, "roleAssignmentIds");
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
                    "roleAssignmentIds",
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
                "revokedRoleAssignmentIds": revoked_ids,
                "userErrors": user_errors
            }),
            status,
            revoked_ids,
        )
    }

    /// Creates (or returns the existing) role assignment linking a contact to a role
    /// at a location, recording the assignment id under the location.
    fn b2b_stage_role_assignment(
        &mut self,
        location_id: &str,
        contact_id: &str,
        role_id: &str,
    ) -> Value {
        if let Some(existing) = self.b2b_role_assignment_for(location_id, contact_id, role_id) {
            return existing;
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
        if let Some(mut location) = self.store.staged.b2b_locations.get(location_id).cloned() {
            b2b_push_json_id(&mut location, "roleAssignmentIds", &assignment_id);
            self.store
                .staged
                .b2b_locations
                .insert(location_id.to_string(), location);
        }
        assignment
    }

    /// Deletes a company and its locations from staged state, refusing the delete
    /// when the company is still referenced (e.g. by an order's purchasing company)
    /// the way Shopify guards an in-use company.
    pub(in crate::proxy) fn b2b_company_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                json!({
                    "deletedCompanyId": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Resource requested does not exist.",
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        if self.b2b_company_has_delete_blocker(&company_id) {
            return (
                json!({
                    "deletedCompanyId": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Failed to delete the company.",
                        "code": "FAILED_TO_DELETE"
                    }]
                }),
                "failed",
                Vec::new(),
            );
        }
        self.b2b_remove_company_graph(&company_id);
        (
            json!({
                "deletedCompanyId": company_id,
                "userErrors": []
            }),
            "staged",
            vec![company_id],
        )
    }

    /// Bulk company delete: a per-index RESOURCE_NOT_FOUND for unknown ids, a
    /// FAILED_TO_DELETE for ids blocked by an in-use reference, and the rest deleted.
    pub(in crate::proxy) fn b2b_companies_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let company_ids = resolved_string_list_field_unsorted(&field.arguments, "companyIds");
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, company_id) in company_ids.iter().enumerate() {
            if !self.store.staged.b2b_companies.contains_key(company_id) {
                user_errors.push(b2b_indexed_user_error(
                    "companyIds",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
            } else if self.b2b_company_has_delete_blocker(company_id) {
                user_errors.push(b2b_indexed_user_error(
                    "companyIds",
                    index,
                    "Failed to delete the company.",
                    "FAILED_TO_DELETE",
                ));
            } else {
                deleted_ids.push(company_id.clone());
            }
        }
        for company_id in &deleted_ids {
            self.b2b_remove_company_graph(company_id);
        }
        let status = if deleted_ids.is_empty() && !user_errors.is_empty() {
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

    /// True when a company cannot be deleted because it is still referenced by an
    /// order/draft-order's purchasing company, or one of its locations carries a
    /// positive store-credit balance.
    fn b2b_company_has_delete_blocker(&self, company_id: &str) -> bool {
        self.store
            .staged
            .orders
            .values()
            .any(|order| b2b_record_references_company(order, company_id))
            || self
                .store
                .staged
                .draft_orders
                .values()
                .any(|draft_order| b2b_record_references_company(draft_order, company_id))
            || self
                .store
                .staged
                .order_customer_orders
                .values()
                .any(|order| b2b_record_references_company(order, company_id))
            || self
                .b2b_company_location_ids(company_id)
                .iter()
                .any(|location_id| self.b2b_location_has_store_credit_balance(location_id))
    }

    /// Every staged location id that belongs to a company, whether tracked via the
    /// company's `locationIds` or back-referenced from the location's `companyId`.
    fn b2b_company_location_ids(&self, company_id: &str) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .b2b_companies
            .get(company_id)
            .map(|company| b2b_json_id_list(company, "locationIds"))
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

    /// Removes a company and all of its staged locations from local state.
    fn b2b_remove_company_graph(&mut self, company_id: &str) {
        let location_ids = self.b2b_company_location_ids(company_id);
        self.store.staged.b2b_companies.remove(company_id);
        for location_id in location_ids {
            self.store.staged.b2b_locations.remove(&location_id);
        }
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
        if self
            .b2b_company_location_delete_blocker(&location_id)
            .is_some()
        {
            return (
                json!({
                    "deletedCompanyLocationId": Value::Null,
                    "userErrors": [{
                        "field": ["companyLocationId"],
                        "message": "Failed to delete the company location.",
                        "code": "FAILED_TO_DELETE"
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
            if !self.store.staged.b2b_locations.contains_key(location_id) {
                user_errors.push(b2b_indexed_user_error(
                    "companyLocationIds",
                    index,
                    "Resource requested does not exist.",
                    "RESOURCE_NOT_FOUND",
                ));
            } else if let Some(blocker) = self.b2b_company_location_delete_blocker(location_id) {
                user_errors.push(b2b_indexed_user_error(
                    "companyLocationIds",
                    index,
                    &blocker.bulk_message(location_id),
                    "INTERNAL_ERROR",
                ));
            } else {
                deleted_ids.push(location_id.clone());
            }
        }
        for location_id in &deleted_ids {
            self.b2b_delete_company_location(location_id);
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

    fn b2b_company_location_delete_blocker(
        &self,
        location_id: &str,
    ) -> Option<B2bCompanyLocationDeleteBlocker> {
        let location = self.store.staged.b2b_locations.get(location_id)?;
        if self.b2b_company_location_is_only_location(location_id, location) {
            return Some(B2bCompanyLocationDeleteBlocker::OnlyLocation);
        }
        if self
            .store
            .staged
            .orders
            .values()
            .any(|order| b2b_record_references_company_location(order, location_id))
            || self
                .store
                .staged
                .order_customer_orders
                .values()
                .any(|order| b2b_record_references_company_location(order, location_id))
        {
            return Some(B2bCompanyLocationDeleteBlocker::Order);
        }
        if self
            .store
            .staged
            .draft_orders
            .values()
            .any(|draft_order| b2b_record_references_company_location(draft_order, location_id))
        {
            return Some(B2bCompanyLocationDeleteBlocker::DraftOrder);
        }
        self.b2b_location_has_store_credit_balance(location_id)
            .then_some(B2bCompanyLocationDeleteBlocker::StoreCredit)
    }

    fn b2b_company_location_is_only_location(&self, location_id: &str, location: &Value) -> bool {
        let Some(company_id) = b2b_location_company_id(location) else {
            return false;
        };
        self.b2b_company_location_ids(company_id)
            .iter()
            .filter(|id| self.store.staged.b2b_locations.contains_key(id.as_str()))
            .count()
            <= 1
            && self.store.staged.b2b_locations.contains_key(location_id)
    }

    fn b2b_location_has_store_credit_balance(&self, location_id: &str) -> bool {
        self.store
            .staged
            .b2b_locations
            .get(location_id)
            .is_some_and(b2b_location_has_embedded_store_credit_balance)
            || self
                .store
                .staged
                .store_credit_accounts
                .values()
                .any(|account| {
                    account["owner"]["id"].as_str() == Some(location_id)
                        && store_credit_account_has_positive_balance(account)
                })
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

        // The assigned CompanyAddressInput is validated the same way as on
        // create, under the `["address"]` field path, so a malformed country/zone/
        // zip/free-text value is rejected before it mutates staged state.
        let address_errors = b2b_address_input_errors(&address_input, &["address"]);
        if !address_errors.is_empty() {
            return (
                json!({ "addresses": Value::Null, "userErrors": address_errors }),
                "failed",
                Vec::new(),
            );
        }

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

    /// Nulls any staged location address (billing and/or shipping) that references
    /// `address_id`. When a location shares one address record across both billing and
    /// shipping, deleting it nulls BOTH sides. Returns the ids of the touched locations.
    pub(in crate::proxy) fn b2b_delete_company_address(&mut self, address_id: &str) -> Vec<String> {
        let mut touched_location_ids = Vec::new();
        let location_ids = self.store.staged.b2b_location_order.clone();
        for location_id in location_ids {
            let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned()
            else {
                continue;
            };
            let billing_matches = location["billingAddress"]["id"].as_str() == Some(address_id);
            let shipping_matches = location["shippingAddress"]["id"].as_str() == Some(address_id);
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
        touched_location_ids
    }

    pub(in crate::proxy) fn b2b_company_address_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let address_id = resolved_string_arg(&field.arguments, "addressId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        let touched_location_ids = self.b2b_delete_company_address(&address_id);
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
        // Once the top-level location resolves, Shopify returns an (empty) array
        // even when every per-entry assignment failed validation — not null. Only
        // an unresolved top-level companyLocationId yields a null connection (the
        // early return above), mirroring the companyContactAssignRoles shape.
        (
            json!({
                "roleAssignments": Value::Array(assignments),
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
                "companyContactRoleAssignment" if !value.is_null() => {
                    self.b2b_role_assignment_selected_json(value, &selection.selection)
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
            "companyContact" => resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                self.store.staged.b2b_contacts.contains_key(&id)
                    || self.store.staged.deleted_b2b_contact_ids.contains(&id)
            }),
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
        self.store
            .staged
            .b2b_role_assignments
            .retain(|_, assignment| assignment["companyLocationId"].as_str() != Some(location_id));
        for assignment_id in b2b_json_id_list(&location, "staffAssignmentIds") {
            self.store
                .staged
                .b2b_staff_assignments
                .remove(&assignment_id);
        }
        self.store
            .staged
            .b2b_staff_assignments
            .retain(|_, assignment| assignment["companyLocationId"].as_str() != Some(location_id));
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
            .filter(|(_, assignment)| assignment["companyContactId"].as_str() == Some(contact_id))
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

    /// Forwards companyAddressDelete upstream for its authoritative `deletedAddressId`,
    /// then — only when the upstream delete succeeded — nulls the matching billing/shipping
    /// address on every staged location, so a read-after-delete reflects the removal.
    /// The argument carries the locally-staged (synthetic) address id, which is what the
    /// staged locations reference, so the side-effect targets local state directly.
    pub(in crate::proxy) fn b2b_company_address_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let address_ids = root_fields(query, variables)
            .map(|fields| {
                fields
                    .iter()
                    .filter(|field| field.name == "companyAddressDelete")
                    .filter_map(|field| {
                        resolved_string_arg(&field.arguments, "addressId")
                            .or_else(|| resolved_string_arg(&field.arguments, "id"))
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
            for address_id in &address_ids {
                self.b2b_delete_company_address(address_id);
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
        let create = root_fields(query, variables).and_then(|fields| {
            fields
                .into_iter()
                .find(|f| f.name == "companyContactCreate")
        });

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
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
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

    /// Forwards companyContactUpdate upstream for its authoritative payload (which
    /// carries the real Customer id), then — only when the upstream update
    /// succeeded — applies the same staging side-effect the snapshot path uses:
    /// the contact's title/locale/name fields and the linked Customer's
    /// name/email/phone are updated in place so a later read of the contact (or
    /// its customer) reflects the change. The contact is keyed by the real id
    /// Shopify returned at create time, and its linked customer is the synthetic
    /// one provisioned then, so the in-place update lands on the records reads see.
    pub(in crate::proxy) fn b2b_company_contact_update_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let update = root_fields(query, variables).and_then(|fields| {
            fields
                .into_iter()
                .find(|f| f.name == "companyContactUpdate")
        });

        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );

        if let Some(field) = update {
            if b2b_passthrough_mutation_succeeded(&response) {
                // Reuse the snapshot payload builder purely for its staging
                // side-effect; the authoritative response is the upstream one.
                let _ = self.b2b_company_contact_update_payload(&field);
                // The contact is staged under the synthetic id minted at company
                // create time, but a node(id) read after the update threads the
                // real id Shopify returned. Mirror the now-updated contact under
                // that real id so the generic Node read resolves it, keeping the
                // synthetic-keyed record intact for connection reads that still
                // address it by the create-time id.
                let synthetic_id =
                    resolved_string_arg(&field.arguments, "companyContactId").unwrap_or_default();
                let real_id = response
                    .body
                    .get("data")
                    .and_then(|data| data.get("companyContactUpdate"))
                    .and_then(|payload| payload.get("companyContact"))
                    .and_then(|contact| contact.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(real_id) = real_id {
                    if real_id != synthetic_id {
                        if let Some(mut contact) =
                            self.store.staged.b2b_contacts.get(&synthetic_id).cloned()
                        {
                            contact["id"] = json!(real_id);
                            self.store.staged.b2b_contacts.insert(real_id, contact);
                        }
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
        let assign = root_fields(query, variables).and_then(|fields| {
            fields
                .into_iter()
                .find(|f| f.name == "companyAssignMainContact")
        });

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
        let revoke = root_fields(query, variables).and_then(|fields| {
            fields
                .into_iter()
                .find(|f| f.name == "companyRevokeMainContact")
        });

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

        for company_id in b2b_passthrough_deleted_request_ids(&response, &company_ids) {
            self.b2b_delete_company(&company_id);
        }
        response
    }

    /// Forwards companyLocationDelete/companyLocationsDelete upstream, then removes only
    /// the locations the authoritative response reports as actually deleted (skipping
    /// those blocked by deletable checks or reported as not found) so subsequent reads
    /// stop surfacing the deleted locations while retaining the blocked ones.
    pub(in crate::proxy) fn b2b_company_locations_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let location_ids = root_fields(query, variables)
            .map(|fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyLocationDelete" => {
                            resolved_string_arg(&field.arguments, "companyLocationId")
                                .or_else(|| resolved_string_arg(&field.arguments, "id"))
                                .into_iter()
                                .collect::<Vec<String>>()
                        }
                        "companyLocationsDelete" => resolved_string_list_field_unsorted(
                            &field.arguments,
                            "companyLocationIds",
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

        for location_id in b2b_passthrough_deleted_request_ids(&response, &location_ids) {
            self.b2b_delete_company_location(&location_id);
        }
        response
    }

    /// Forwards companyContactRevokeRole / companyContactRevokeRoles /
    /// companyLocationRevokeRoles upstream, then removes only the role assignments
    /// the authoritative response reports as actually revoked — skipping any id it
    /// rejects via an indexed userError — so subsequent reads of the contact's and
    /// location's roleAssignments connections drop the revoked assignments while a
    /// partial revoke leaves the surviving ones in place.
    pub(in crate::proxy) fn b2b_revoke_roles_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        let assignment_ids = root_fields(query, variables)
            .map(|fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyContactRevokeRole" => {
                            resolved_string_arg(&field.arguments, "companyContactRoleAssignmentId")
                                .into_iter()
                                .collect::<Vec<String>>()
                        }
                        "companyContactRevokeRoles" => resolved_string_list_field_unsorted(
                            &field.arguments,
                            "roleAssignmentIds",
                        ),
                        "companyLocationRevokeRoles" => {
                            resolved_string_list_field_unsorted(&field.arguments, "rolesToRevoke")
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

        for assignment_id in b2b_passthrough_deleted_request_ids(&response, &assignment_ids) {
            self.b2b_remove_role_assignment(&assignment_id);
        }
        response
    }

    /// Removes a single role assignment from staged state and detaches it from its
    /// location's `roleAssignmentIds` list. A contact's roleAssignments connection
    /// is resolved by filtering the assignment map, so dropping the entry here is
    /// enough to clear it from the contact view too.
    fn b2b_remove_role_assignment(&mut self, assignment_id: &str) {
        if let Some(assignment) = self.store.staged.b2b_role_assignments.remove(assignment_id) {
            if let Some(location_id) = assignment["companyLocationId"].as_str() {
                self.b2b_remove_location_assignment_id(
                    location_id,
                    "roleAssignmentIds",
                    assignment_id,
                );
            }
        }
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

    /// True when the contact already holds *any* role assignment at the location,
    /// regardless of which role. Shopify caps a contact at one assignment per
    /// location, so this gates the LIMIT_REACHED rejection on the singular assign.
    fn b2b_contact_has_assignment_at_location(&self, contact_id: &str, location_id: &str) -> bool {
        self.store
            .staged
            .b2b_role_assignments
            .values()
            .any(|assignment| {
                assignment["companyContactId"].as_str() == Some(contact_id)
                    && assignment["companyLocationId"].as_str() == Some(location_id)
            })
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
        if let Some(response) = product_tail_invalid_enum_response(query, operation_type, &fields) {
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

        for field in &fields {
            if field.name == "productFullSync" {
                if let Some(error) = product_full_sync_payload_selection_error(field) {
                    return Some(ok_json(json!({ "errors": [error] })));
                }
            }
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
                    let missing_product_ids = self.feedback_missing_product_ids(&field, request);
                    product_tail_resource_feedback_payload(&field, &missing_product_ids)
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

    /// Next publication gid: one past the largest staged publication suffix, so
    /// id allocation is derived from store state rather than a fixed literal.
    fn next_publication_id(&self) -> String {
        // `Publication/1` is Shopify's implicit default (Online Store) channel, so
        // synthetically-created publications begin at `/2`. Number above the highest
        // numeric publication id already staged, with that default reserved as the
        // floor, so the first locally-created publication is `gid://shopify/Publication/2`
        // regardless of whether the baseline seeded non-numeric publication ids.
        let max = self
            .store
            .staged
            .publications
            .keys()
            .map(|id| resource_id_path_tail(id.as_str()))
            .filter_map(|suffix| suffix.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            .max(1);
        format!("gid://shopify/Publication/{}", max + 1)
    }

    pub(in crate::proxy) fn product_tail_publication_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let catalog_id = resolved_string_field(&input, "catalogId");
        let auto_publish = resolved_bool_field(&input, "autoPublish").unwrap_or(false);
        let catalog = catalog_id
            .as_deref()
            .and_then(|catalog_id| self.store.staged.catalogs.get(catalog_id).cloned());
        let (payload, staged_ids, status) =
            if let (Some(catalog_id), None) = (catalog_id.as_deref(), catalog.as_ref()) {
                (
                    publication_catalog_not_found_payload(catalog_id),
                    Vec::new(),
                    "failed",
                )
            } else {
                let id = self.next_publication_id();
                let name = publication_create_name(&id, catalog.as_ref());
                let record = publication_record_json(&id, &name, auto_publish);
                (
                    json!({ "publication": record, "userErrors": [] }),
                    vec![id],
                    "staged",
                )
            };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationCreate",
            staged_ids.clone(),
            status,
        );
        if status == "staged" {
            if let Some(id) = staged_ids.first() {
                self.store.stage_created_publication_id(id.clone());
                if let Some(record) = payload.get("publication") {
                    self.store
                        .staged
                        .publications
                        .insert(id.clone(), record.clone());
                }
                if let Some(catalog_id) = catalog_id.as_deref() {
                    if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
                        set_catalog_publication_relation(catalog, Some(id));
                    }
                }
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
        let id = resolved_string_field(&field.arguments, "id");
        let record = id
            .as_deref()
            .and_then(|id| self.store.staged.publications.get(id).cloned());
        let (Some(id), Some(mut record)) = (id, record) else {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationUpdate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &publication_not_found_payload("publication"),
                &field.selection,
            );
        };
        let publishables_to_add = resolved_string_list_field_unsorted(&input, "publishablesToAdd");
        let publishables_to_remove =
            resolved_string_list_field_unsorted(&input, "publishablesToRemove");
        let user_errors = self
            .publication_update_publishable_errors(&publishables_to_add, &publishables_to_remove);
        if !user_errors.is_empty() {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationUpdate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "publication": null,
                    "userErrors": user_errors
                }),
                &field.selection,
            );
        };
        if let Some(auto_publish) = resolved_bool_field(&input, "autoPublish") {
            record["autoPublish"] = json!(auto_publish);
        }
        for publishable_id in &publishables_to_add {
            self.store
                .staged
                .resource_publications
                .entry(publishable_id.clone())
                .or_default()
                .insert(id.clone());
        }
        for publishable_id in &publishables_to_remove {
            if let Some(publications) = self
                .store
                .staged
                .resource_publications
                .get_mut(publishable_id)
            {
                publications.remove(&id);
            }
        }
        self.apply_publication_update_product_entries(
            &id,
            &publishables_to_add,
            &publishables_to_remove,
        );
        self.store
            .staged
            .publications
            .insert(id.clone(), record.clone());
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationUpdate",
            vec![id],
            "staged",
        );
        selected_json(
            &json!({ "publication": record, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn product_tail_publication_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationDelete",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &publication_not_found_payload("deletedId"),
                &field.selection,
            );
        };
        // Only publications staged this scenario can be deleted; the base/default
        // publication (and any unknown id) cannot be removed.
        if !self.store.staged.created_publication_ids.contains(&id) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationDelete",
                Vec::new(),
                "failed",
            );
            if id != "gid://shopify/Publication/1"
                && !self.store.staged.publications.contains_key(&id)
            {
                return selected_json(
                    &publication_not_found_payload("deletedId"),
                    &field.selection,
                );
            }
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Cannot delete the default publication",
                        "code": "CANNOT_DELETE_DEFAULT_PUBLICATION"
                    }]
                }),
                &field.selection,
            );
        }
        self.store.staged.publications.remove(&id);
        self.store.staged.created_publication_ids.remove(&id);
        self.store.staged.publication_ids.remove(&id);
        // Cascade: a deleted publication is no longer a membership target, so any
        // product/collection published on it is no longer published there.
        for pubs in self.store.staged.resource_publications.values_mut() {
            pubs.remove(&id);
        }
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationDelete",
            vec![id.clone()],
            "staged",
        );
        selected_json(
            &json!({
                "deletedId": id,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn publication_update_publishable_errors(
        &self,
        publishables_to_add: &[String],
        publishables_to_remove: &[String],
    ) -> Vec<Value> {
        if publishables_to_add.len() + publishables_to_remove.len() > PUBLICATION_UPDATE_LIMIT {
            return vec![publication_error(
                publication_update_limit_field(publishables_to_add, publishables_to_remove),
                "The limit for simultaneous publication updates has been exceeded.",
                "PUBLICATION_UPDATE_LIMIT_EXCEEDED",
            )];
        }

        let mut user_errors = Vec::new();
        let mut has_product = false;
        let mut has_variant = false;
        for (field_name, ids) in [
            ("publishablesToAdd", publishables_to_add),
            ("publishablesToRemove", publishables_to_remove),
        ] {
            for (index, id) in ids.iter().enumerate() {
                match shopify_gid_resource_type(id) {
                    Some("Product") => has_product = true,
                    Some("ProductVariant") => has_variant = true,
                    _ => {}
                }
                if !self.publication_update_publishable_exists(id) {
                    user_errors.push(publication_indexed_error(
                        field_name,
                        index,
                        "Publishable ID not found.",
                        "INVALID_PUBLISHABLE_ID",
                    ));
                }
            }
        }
        if user_errors.is_empty() && has_product && has_variant {
            user_errors.push(publication_error(
                vec!["input"],
                "Cannot combine products and variants in the same publication update",
                "CANNOT_COMBINE_PRODUCTS_AND_VARIANTS",
            ));
        }
        user_errors
    }

    fn publication_update_publishable_exists(&self, id: &str) -> bool {
        match shopify_gid_resource_type(id) {
            Some("Product") => self.product_record_by_id(id).is_some(),
            Some("ProductVariant") => {
                self.store.product_variant_by_id(id).is_some()
                    || self
                        .store
                        .products()
                        .iter()
                        .flat_map(|product| product.variants.iter())
                        .any(|variant| variant.get("id").and_then(Value::as_str) == Some(id))
            }
            _ => false,
        }
    }

    fn apply_publication_update_product_entries(
        &mut self,
        publication_id: &str,
        publishables_to_add: &[String],
        publishables_to_remove: &[String],
    ) {
        let add_product_ids = publishables_to_add
            .iter()
            .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
            .cloned()
            .collect::<BTreeSet<_>>();
        let remove_product_ids = publishables_to_remove
            .iter()
            .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
            .cloned()
            .collect::<BTreeSet<_>>();
        let affected_product_ids = add_product_ids
            .union(&remove_product_ids)
            .cloned()
            .collect::<Vec<_>>();
        for product_id in affected_product_ids {
            let Some(mut product) = self.store.product_staged_or_base(&product_id) else {
                continue;
            };
            let mut entries = product_publication_entries(&product);
            if add_product_ids.contains(&product_id)
                && !entries
                    .iter()
                    .any(|entry| entry.publication_id == publication_id)
            {
                entries.push(ProductPublicationEntry {
                    publication_id: publication_id.to_string(),
                    publish_date: None,
                    published_at: Some(self.next_product_timestamp()),
                });
            }
            if remove_product_ids.contains(&product_id) {
                entries.retain(|entry| entry.publication_id != publication_id);
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            set_product_publication_entries(&mut product, entries);
            self.store.stage_product(product);
        }
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
        // ProductFeed.country is a CountryCode and .language a LanguageCode; Shopify rejects
        // values outside those enums at the resolver with a field-scoped INVALID userError.
        if !is_valid_product_feed_country(&country) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(["country"], "Country is invalid", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        if !is_valid_product_feed_language(&language) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(["language"], "Language is invalid", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        let id = format!("gid://shopify/ProductFeed/{country}-{language}");
        // A feed is unique per country/language pair; re-creating an existing one is rejected.
        if self.has_products_tail_staged_resource_id(&id) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [{
                        "field": ["country"],
                        "message": "Product feed already exists for this country/language pair",
                        "code": "TAKEN"
                    }]
                }),
                &field.selection,
            );
        }
        let payload = json!({
            "productFeed": {
                "id": id,
                "country": country,
                "language": language,
                "status": "ACTIVE"
            },
            "userErrors": []
        });
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
        let feed_exists = shopify_gid_resource_type(&id) == Some("ProductFeed")
            && self.has_products_tail_staged_resource_id(&id);
        let before_updated_at = resolved_string_arg(&field.arguments, "beforeUpdatedAt");
        let updated_at_since = resolved_string_arg(&field.arguments, "updatedAtSince");
        let (payload, staged_ids, status) = if !feed_exists {
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "ProductFeed does not exist",
                        "code": Value::Null
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else if product_full_sync_updated_at_range_invalid(
            before_updated_at.as_deref(),
            updated_at_since.as_deref(),
        ) {
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": null,
                    "userErrors": [{
                        "field": ["updatedAtSince"],
                        "message": "updatedAtSince must be before beforeUpdatedAt",
                        "code": Value::Null
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else {
            let operation_id = self.next_proxy_synthetic_gid("ProductFullSyncOperation");
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": operation_id,
                    "userErrors": []
                }),
                vec![id, operation_id],
                "staged",
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
        if let Some(job) = self.store.staged.collection_jobs.get(&id) {
            return selected_json(job, &field.selection);
        }
        // A job enqueued locally (e.g. a metafield-definition validation job)
        // is addressed by a synthetic Job gid. Reading it back returns a
        // freshly-enqueued, not-yet-complete Job with no backing bulk query —
        // matching Shopify's shape for a pending async job.
        if is_synthetic_gid(&id) && shopify_gid_resource_type(&id) == Some("Job") {
            let job = json!({
                "__typename": "Job",
                "id": id,
                "done": false,
                "query": Value::Null,
            });
            return selected_json(&job, &field.selection);
        }
        Value::Null
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
                if operation_type == OperationType::Mutation && root_field == "customerMerge" {
                    self.observe_customer_merge_passthrough_response(query, variables, &response);
                }
                if operation_type == OperationType::Query
                    && root_fields
                        .iter()
                        .all(|field| matches!(field.as_str(), "node" | "nodes"))
                {
                    self.observe_collection_passthrough_response(&response);
                }
                if operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "collectionAddProducts" | "collectionCreate" | "collectionReorderProducts"
                    )
                {
                    self.observe_collection_passthrough_response(&response);
                    let hydrate_ids =
                        collection_passthrough_hydration_ids(root_field, &response, variables);
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
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "customer" => match field.arguments.get("id") {
                Some(ResolvedValue::String(id)) => {
                    self.store.staged.customers.contains_key(id)
                        || self.store.staged.deleted_customer_ids.contains(id)
                        || self.store_credit_owner_has_accounts(id)
                }
                _ => false,
            },
            "customerByIdentifier" => !self.store.staged.customers.is_empty(),
            // A standalone `customers(query:)` / `customersCount` list read is
            // served from the staged overlay once this scenario has staged at
            // least one customer (e.g. a customerCreate or a privacy
            // dataSaleOptOut synthetic). With no staged customers there is
            // nothing local to serve, so the read forwards upstream unchanged.
            "customers" | "customersCount" => !self.store.staged.customers.is_empty(),
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
                "customers" => Some(self.customers_list_field(field)),
                "customersCount" => Some(selected_json(
                    &json!({ "count": self.customers_count_value(), "precision": "EXACT" }),
                    &field.selection,
                )),
                "customerMergeJobStatus" => Some(self.customer_merge_job_status_field(field)),
                "job" => Some(self.customer_merge_job_node_field(field)),
                "node" if self.customer_merge_job_reference(field) => {
                    Some(self.customer_merge_job_node_field(field))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    /// The store-wide total customer count: the seeded live baseline (or the
    /// legacy default) reduced by the number of customers deleted/merged-away in
    /// this scenario, so `customersCount` tracks merges generically.
    pub(in crate::proxy) fn customers_count_value(&self) -> u64 {
        self.store
            .staged
            .customers_count_base
            .unwrap_or(177)
            .saturating_sub(self.store.staged.deleted_customer_ids.len() as u64)
    }

    /// `customerMergeJobStatus(jobId:)` read: project the requested selection over
    /// the locally recorded merge request (keyed by the synthetic job id minted by
    /// `customerMerge`). Returns null for unknown job ids.
    pub(in crate::proxy) fn customer_merge_job_status_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(job_id) = resolved_string_field(&field.arguments, "jobId") else {
            return Value::Null;
        };
        self.store
            .staged
            .customer_merge_requests
            .get(&job_id)
            .map(|request| selected_json(request, &field.selection))
            .unwrap_or(Value::Null)
    }

    /// Resolve `job(id:)` / `node(id:)` for a synthetic merge job id minted by
    /// `customer_merge`. Returns a completed `Job` projection from the staged
    /// merge request, or null for ids the proxy did not mint.
    pub(in crate::proxy) fn customer_merge_job_node_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .staged
            .customer_merge_requests
            .get(&id)
            .map(customer_merge_job_from_request)
            .map(|job| selected_json(&job, &field.selection))
            .unwrap_or(Value::Null)
    }

    /// True iff `node(id:)` targets a `Job` id we minted for a staged merge
    /// request, so the overlay read may serve it instead of forwarding.
    pub(in crate::proxy) fn customer_merge_job_reference(
        &self,
        field: &RootFieldSelection,
    ) -> bool {
        resolved_string_field(&field.arguments, "id")
            .as_deref()
            .is_some_and(|id| self.store.staged.customer_merge_requests.contains_key(id))
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
        // The per-customer order connection is resolved from the staged
        // `customer_orders` index when present (orders created/transferred in the
        // scenario), windowing + cursoring generically. When a customer has no staged
        // orders but carries a recorded inline `orders` connection (a seeded read
        // baseline whose opaque cursors / pageInfo cannot be reconstructed locally),
        // that recorded page is projected verbatim instead.
        let mapped_orders = self.store.staged.customer_orders.get(id);
        selected_payload_json(selection, |field| match field.name.as_str() {
            "orders" => Some(match mapped_orders {
                Some(orders) => selected_connection_json_with_args(
                    orders.clone(),
                    &field.arguments,
                    &field.selection,
                    order_connection_cursor,
                ),
                None if connection_has_nodes(&customer["orders"]) => project_seeded_connection(
                    &customer["orders"],
                    &field.arguments,
                    &field.selection,
                ),
                None => selected_connection_json_with_args(
                    Vec::new(),
                    &field.arguments,
                    &field.selection,
                    order_connection_cursor,
                ),
            }),
            // The `storeCreditAccounts` connection is resolved from the staged
            // store-credit accounts indexed by owner, so a customer read reflects
            // credit/debit mutations (and locally minted accounts) immediately.
            "storeCreditAccounts" => Some(self.store_credit_accounts_connection_for_owner(
                id,
                &field.arguments,
                &field.selection,
            )),
            _ => selected_json(customer, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
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
        if is_credit && !store_credit_supported_currency(&currency) {
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
        account["balance"] = money_value(&balance_display, &currency);
        let transaction = json!({
            "id": transaction_id,
            "__typename": if is_credit { "StoreCreditAccountCreditTransaction" } else { "StoreCreditAccountDebitTransaction" },
            "amount": money_value(&amount_display, &currency),
            "balanceAfterTransaction": money_value(&balance_display, &currency),
            "createdAt": self.next_product_timestamp(),
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

    fn create_store_credit_account_for_owner(&mut self, owner_id: &str, currency: &str) -> String {
        let account_id = self.next_store_credit_account_gid();
        let owner = self.store_credit_owner_json(owner_id);
        let account = json!({
            "id": account_id,
            "balance": money_value("0.0", currency),
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

    fn store_credit_owner_has_accounts(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .store_credit_accounts
            .values()
            .any(|account| account["owner"]["id"].as_str() == Some(owner_id))
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

    fn next_store_credit_account_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_account_id;
        self.store.staged.next_store_credit_account_id += 1;
        synthetic_shopify_gid("StoreCreditAccount", id)
    }

    fn next_store_credit_transaction_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_transaction_id;
        self.store.staged.next_store_credit_transaction_id += 1;
        synthetic_shopify_gid("StoreCreditAccountTransaction", id)
    }

    /// `customers(first:, query:)` list root. Filters the live staged customers
    /// (excluding merged-away / deleted records) by the optional `query` (currently
    /// `tag:<value>` plus a generic substring fallback over email/display name) and
    /// projects each node through the shared customer renderer so nested
    /// `orders`/`addressesV2`/`metafields` connections resolve from store state
    /// exactly as the singular `customer`/`customerByIdentifier` reads do.
    pub(in crate::proxy) fn customers_list_field(&self, field: &RootFieldSelection) -> Value {
        let query = resolved_string_field(&field.arguments, "query");
        let mut records: Vec<Value> = self
            .store
            .staged
            .customers
            .values()
            .filter(|customer| {
                let id = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                !self.store.staged.deleted_customer_ids.contains(id)
            })
            .filter(|customer| customer_matches_query(customer, query.as_deref()))
            .cloned()
            .collect();
        records.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |customer, selection| {
                let id = customer["id"].as_str().unwrap_or_default().to_string();
                self.customer_with_order_connection(&id, customer, selection)
            },
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn customer_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        // A merged-away / deleted customer must not resolve through identifier
        // lookups even though its record may briefly linger in the map.
        let is_live = |customer: &&Value| {
            customer
                .get("id")
                .and_then(Value::as_str)
                .map(|id| !self.store.staged.deleted_customer_ids.contains(id))
                .unwrap_or(true)
        };
        let customer = if let Some(raw_email) = resolved_string_field(identifier, "email")
            .or_else(|| resolved_string_field(identifier, "emailAddress"))
        {
            let needle = normalize_customer_email(&raw_email);
            self.store.staged.customers.values().find(|customer| {
                if !is_live(customer) {
                    return false;
                }
                let stored = customer.get("email").and_then(Value::as_str);
                let stored_default = customer["defaultEmailAddress"]["emailAddress"].as_str();
                match needle.as_deref() {
                    Some(needle) => stored == Some(needle) || stored_default == Some(needle),
                    None => {
                        stored == Some(raw_email.as_str())
                            || stored_default == Some(raw_email.as_str())
                    }
                }
            })
        } else if let Some(id) = resolved_string_field(identifier, "id") {
            self.store
                .staged
                .customers
                .get(&id)
                .filter(|_| !self.store.staged.deleted_customer_ids.contains(&id))
        } else if let Some(raw_phone) = resolved_string_field(identifier, "phone")
            .or_else(|| resolved_string_field(identifier, "phoneNumber"))
        {
            let needle = normalize_customer_phone(&raw_phone);
            self.store.staged.customers.values().find(|customer| {
                if !is_live(customer) {
                    return false;
                }
                let stored = customer.get("phone").and_then(Value::as_str);
                let stored_default = customer["defaultPhoneNumber"]["phoneNumber"].as_str();
                match needle.as_deref() {
                    Some(needle) => stored == Some(needle) || stored_default == Some(needle),
                    None => {
                        stored == Some(raw_phone.as_str())
                            || stored_default == Some(raw_phone.as_str())
                    }
                }
            })
        } else {
            None
        };
        customer
            .map(|customer| {
                let id = customer["id"].as_str().unwrap_or_default().to_string();
                self.customer_with_order_connection(&id, customer, &field.selection)
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_order_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| ("orderCreate".to_string(), Vec::new()));
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
            synthetic_shopify_gid("Order", self.next_synthetic_id)
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
            // A top-level GraphQL error whose path points at this root field means
            // the field itself resolves to `null` in `data` (GraphQL error
            // propagation), not `{customer:null,userErrors:[]}`. This mirrors
            // Shopify's REDACTED inline-consent rejection, which surfaces a
            // top-level error AND `customerCreate: null`.
            let has_top_error = !field_errors.is_empty();
            errors.extend(field_errors);
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            let rendered = if has_top_error {
                Value::Null
            } else {
                selected_json(&payload, &field.selection)
            };
            data.insert(field.response_key.clone(), rendered);
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

    /// Fabricated customers always receive a stable plain `gid://shopify/Customer/N`
    /// id. The local-runtime conformance fixtures compare these ids strictly and
    /// expect the plain form, while every live-hybrid scenario that surfaces a
    /// fabricated customer id matches it with the lenient `shopify-gid:Customer`
    /// matcher (which accepts any `gid://shopify/Customer/...`). Read routing keys
    /// on `staged.customers.contains_key(id)`, so the proxy stays internally
    /// consistent without needing the `?shopify-draft-proxy=synthetic` marker.
    fn next_customer_gid(&mut self, _normalized: &NormalizedCustomerInput) -> String {
        self.next_synthetic_gid("Customer")
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
        if let Some(error) = customer_create_nested_id_error(&input) {
            return (
                customer_payload(Value::Null, vec![error]),
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

        let id = self.next_customer_gid(&normalized);
        let timestamp = self.next_product_timestamp();
        let verified_email_default = customer_create_verified_email_default(request, &normalized);
        let mut customer =
            customer_record_from_parts(&id, None, &normalized, &timestamp, verified_email_default);
        // `customerCreate` accepts inline `emailMarketingConsent` /
        // `smsMarketingConsent` and immediately reflects them on the staged
        // record's compatibility consent fields and on
        // `defaultEmailAddress` / `defaultPhoneNumber`. Validation (missing
        // contact, REDACTED state) already ran above, so any consent present
        // here is applicable.
        apply_inline_consent_from_input(&mut customer, &input);
        // A freshly created customer has no orders yet. Surface Shopify's
        // order-summary defaults (string-zero `numberOfOrders`, null `lastOrder`,
        // empty `orders` connection) so create payloads and subsequent reads that
        // select the order summary match without inventing order state. The
        // per-customer `orders` connection on reads is recomputed from the staged
        // `customer_orders` index, so this stored empty connection only backs the
        // mutation payload projection. `amountSpent` needs the shop currency (not
        // known locally) and remains the one acknowledged representation gap.
        apply_customer_order_summary_defaults(&mut customer);
        // A freshly created customer also has no store-credit accounts. Bake the
        // empty connection so a create payload selecting `storeCreditAccounts`
        // matches; reads recompute it from staged store-credit state via
        // `customer_with_order_connection`.
        if customer
            .get("storeCreditAccounts")
            .is_none_or(Value::is_null)
        {
            customer["storeCreditAccounts"] = empty_orders_connection();
        }
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }

    /// Standalone `customerAddress*` / `customerUpdateDefaultAddress` mutations.
    ///
    /// HEAD stores customer addresses *inline* on the staged customer record at
    /// `addressesV2.nodes` / `defaultAddress`; these handlers operate directly on
    /// that inline model so reads (`customer`, `customerByIdentifier`) reflect
    /// every mutation via the same `selected_json` path. Address ids are minted
    /// from the shared synthetic counter (`next_proxy_synthetic_gid`) so they are
    /// globally unique across customers — this is what lets cross-owner address
    /// references resolve to "Address does not exist" rather than colliding with a
    /// different customer's per-customer index. The parity comparison matches
    /// these synthetic ids and cursors with `any-string`, so only their
    /// uniqueness and read-after-write consistency matter, never their values.
    pub(in crate::proxy) fn customer_address_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        let mut top_errors = Vec::new();
        for field in &fields {
            let (payload, staged_ids, field_top_errors) = match field.name.as_str() {
                "customerAddressCreate" => self.customer_address_create(field),
                "customerAddressUpdate" => self.customer_address_update(field),
                "customerAddressDelete" => self.customer_address_delete(field),
                "customerUpdateDefaultAddress" => self.customer_update_default_address(field),
                _ => (Value::Null, Vec::new(), Vec::new()),
            };
            top_errors.extend(field_top_errors);
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            // A null payload signals a top-level RESOURCE_NOT_FOUND (the data
            // field itself is null); a non-null payload renders through the
            // selection set like every other mutation result.
            let rendered = if payload.is_null() {
                Value::Null
            } else {
                selected_json(&payload, &field.selection)
            };
            data.insert(field.response_key.clone(), rendered);
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    fn customer_address_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let set_as_default = resolved_bool_field(&field.arguments, "setAsDefault");
        let Some((customer_first, customer_last, existing_nodes, current_default)) =
            self.customer_address_context(&customer_id)
        else {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["customerId"]),
                        "Customer does not exist",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        let new_id = self.next_proxy_synthetic_gid("MailingAddress");
        let (node, errors) = customer_address_input_node(
            &address_input,
            None,
            customer_first.as_deref(),
            customer_last.as_deref(),
            &new_id,
        );
        if !errors.is_empty() {
            return (
                customer_address_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let node = node.unwrap_or(Value::Null);
        let new_key = customer_address_dedup_key(&node);
        if existing_nodes
            .iter()
            .any(|existing| customer_address_dedup_key(existing) == new_key)
        {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["address"]),
                        "Address already exists",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let was_empty = existing_nodes.is_empty();
        let mut nodes = existing_nodes;
        nodes.push(node.clone());
        let default_id = if set_as_default == Some(true) || was_empty {
            Some(new_id.clone())
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            customer_address_payload(node, Vec::new()),
            vec![new_id],
            Vec::new(),
        )
    }

    fn customer_address_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let set_as_default = resolved_bool_field(&field.arguments, "setAsDefault");
        // A nested `address.id` that is present must equal the top-level
        // `addressId`. An explicit null (key present, value null) counts as a
        // mismatch, matching Shopify; an omitted key skips the check.
        if address_input.contains_key("id")
            && resolved_string_field(&address_input, "id").as_deref() != Some(address_id.as_str())
        {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["addressId"]),
                        "The id of the address does not match the id in the input",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        let Some((customer_first, customer_last, existing_nodes, current_default)) = context else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| customer_address_payload(Value::Null, errors),
            );
        };
        let Some(index) = index else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| customer_address_payload(Value::Null, errors),
            );
        };
        let (node, errors) = customer_address_input_node(
            &address_input,
            Some(&existing_nodes[index]),
            customer_first.as_deref(),
            customer_last.as_deref(),
            &address_id,
        );
        if !errors.is_empty() {
            return (
                customer_address_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let node = node.unwrap_or(Value::Null);
        let mut nodes = existing_nodes;
        nodes[index] = node.clone();
        let default_id = if set_as_default == Some(true) {
            Some(address_id.clone())
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            customer_address_payload(node, Vec::new()),
            vec![address_id],
            Vec::new(),
        )
    }

    fn customer_address_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        let Some((_, _, existing_nodes, current_default)) = context else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "deletedAddressId": Value::Null, "userErrors": errors }),
            );
        };
        let Some(index) = index else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "deletedAddressId": Value::Null, "userErrors": errors }),
            );
        };
        let was_default = current_default.as_deref() == Some(address_id.as_str());
        let mut nodes = existing_nodes;
        nodes.remove(index);
        // Deleting the default promotes the first remaining address; deleting a
        // non-default leaves the default untouched.
        let default_id = if was_default {
            nodes
                .first()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            json!({ "deletedAddressId": address_id, "userErrors": [] }),
            Vec::new(),
            Vec::new(),
        )
    }

    fn customer_update_default_address(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        // Return the full staged customer record; the field's `customer`
        // sub-selection is applied by `selected_json` at the call site.
        let render_customer = |me: &Self| {
            me.store
                .staged
                .customers
                .get(&customer_id)
                .cloned()
                .unwrap_or(Value::Null)
        };
        let Some((_, _, existing_nodes, _)) = context else {
            // Unknown customer: treat the address as not found.
            if self.customer_address_exists_anywhere(&address_id) {
                let customer = render_customer(self);
                return (
                    json!({
                        "customer": customer,
                        "userErrors": [customer_user_error(json!(["addressId"]), "Address does not exist")]
                    }),
                    Vec::new(),
                    Vec::new(),
                );
            }
            return (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(
                    &field.response_key,
                )],
            );
        };
        let Some(index) = index else {
            // Address belongs to another customer (exists somewhere) → userError,
            // but the customer record is still returned. Truly unknown ids return
            // a null payload with a RESOURCE_NOT_FOUND top-level error.
            if self.customer_address_exists_anywhere(&address_id) {
                let customer = render_customer(self);
                return (
                    json!({
                        "customer": customer,
                        "userErrors": [customer_user_error(json!(["addressId"]), "Address does not exist")]
                    }),
                    Vec::new(),
                    Vec::new(),
                );
            }
            return (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(
                    &field.response_key,
                )],
            );
        };
        let default_id = existing_nodes[index]
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            let nodes = existing_nodes;
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        let customer = render_customer(self);
        (
            json!({ "customer": customer, "userErrors": [] }),
            Vec::new(),
            Vec::new(),
        )
    }

    /// Snapshot the inline-address context for a staged customer:
    /// `(firstName, lastName, addressesV2.nodes, defaultAddress.id)`. Returns
    /// `None` when the customer is not staged locally. Extracting clones here
    /// ends the immutable borrow so callers can subsequently mint ids / take a
    /// mutable borrow of the same customer.
    fn customer_address_context(&self, customer_id: &str) -> Option<CustomerAddressContext> {
        let customer = self.store.staged.customers.get(customer_id)?;
        let first = customer
            .get("firstName")
            .and_then(Value::as_str)
            .map(str::to_string);
        let last = customer
            .get("lastName")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some((
            first,
            last,
            customer_address_nodes(customer),
            customer_default_address_id(customer),
        ))
    }

    fn customer_address_exists_anywhere(&self, address_id: &str) -> bool {
        self.store.staged.customers.values().any(|customer| {
            customer_address_node_index(&customer_address_nodes(customer), address_id).is_some()
        })
    }

    /// Shared "addressId not present on this customer" branch for update/delete.
    /// An address that exists on *another* customer yields an "Address does not
    /// exist" user error in the payload shape built by `build_payload`; an id
    /// that exists nowhere yields a null payload + RESOURCE_NOT_FOUND.
    fn customer_address_missing_result(
        &self,
        address_id: &str,
        response_key: &str,
        build_payload: impl Fn(Vec<Value>) -> Value,
    ) -> (Value, Vec<String>, Vec<Value>) {
        if self.customer_address_exists_anywhere(address_id) {
            (
                build_payload(vec![customer_user_error(
                    json!(["addressId"]),
                    "Address does not exist",
                )]),
                Vec::new(),
                Vec::new(),
            )
        } else {
            (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(response_key)],
            )
        }
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
        let id = self.next_customer_gid(&normalized);
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
                if raw_email
                    .split_whitespace()
                    .collect::<String>()
                    .chars()
                    .count()
                    > 255
                {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "email"),
                        "Email is too long (maximum is 255 characters)",
                    ));
                }
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
                if raw_phone.trim().chars().count() > 255 {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "phone"),
                        "Phone is too long (maximum is 255 characters)",
                    ));
                }
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
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_HYDRATE_QUERY,
                "operationName": "CustomerHydrate",
                "variables": { "id": id },
            }),
        );
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
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_DUPLICATE_HYDRATE_QUERY,
                "operationName": "CustomerDuplicateHydrate",
                "variables": { "query": query_value },
            }),
        );
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

/// Hydration query for the store-wide `customersCount` baseline used by the
/// `customer*TaxExemptions` / marketing-consent downstream reads in LiveHybrid
/// mode. Mirrors the per-resource hydrate queries; the count is cached into
/// `customers_count_base` so subsequent reads track deletions generically.
const CUSTOMER_COUNT_HYDRATE_QUERY: &str =
    "query CustomerCountHydrate { customersCount { count precision } }";

impl DraftProxy {
    /// `customerAddTaxExemptions` / `customerRemoveTaxExemptions` /
    /// `customerReplaceTaxExemptions`: stage the resulting tax-exemption set onto
    /// the staged (or hydrated) customer and project the requested selection.
    /// Enum validation (`customer_tax_exemptions_invalid_enum_response`) runs in
    /// the dispatcher before this, so every field here carries valid exemptions.
    pub(in crate::proxy) fn customer_tax_exemptions_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, staged_id) = self.customer_tax_exemptions_field_payload(field, request);
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

    fn customer_tax_exemptions_field_payload(
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

    /// In LiveHybrid mode, hydrate the store-wide `customersCount` baseline from
    /// upstream once (cached into `customers_count_base`) so a downstream
    /// `customersCount` read served from the staged overlay reports the live
    /// total. No-op in Snapshot mode or when the baseline is already known.
    pub(in crate::proxy) fn hydrate_customers_count_for_overlay_read(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.staged.customers_count_base.is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_COUNT_HYDRATE_QUERY,
                "operationName": "CustomerCountHydrate",
                "variables": {},
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(count) = response.body["data"]["customersCount"]["count"].as_u64() {
            self.store.staged.customers_count_base = Some(count);
        }
    }

    /// `customerEmailMarketingConsentUpdate` / `customerSmsMarketingConsentUpdate`:
    /// apply the resolved consent state onto the staged (or hydrated) customer and
    /// project the requested selection, mirroring Shopify's resolver-error shapes.
    pub(in crate::proxy) fn customer_marketing_consent_update(
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
            let outcome =
                self.customer_marketing_consent_update_field(&field, request, query, variables);
            if let Some(error) = outcome.top_level_error {
                errors.push(error);
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&outcome.payload, &field.selection),
                );
            }
        }

        let mut response = serde_json::Map::new();
        if !errors.is_empty() {
            response.insert("errors".to_string(), Value::Array(errors));
        }
        response.insert("data".to_string(), Value::Object(data));
        ok_json(Value::Object(response))
    }

    fn customer_marketing_consent_update_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> CustomerConsentOutcome {
        let is_email = field.name == "customerEmailMarketingConsentUpdate";
        let consent_key = if is_email {
            "emailMarketingConsent"
        } else {
            "smsMarketingConsent"
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let customer_id = resolved_string_field(&input, "customerId").unwrap_or_default();
        let consent = resolved_object_field(&input, consent_key).unwrap_or_default();
        let marketing_state = resolved_string_field(&consent, "marketingState").unwrap_or_default();

        if matches!(marketing_state.as_str(), "NOT_SUBSCRIBED" | "REDACTED")
            || (is_email && marketing_state == "INVALID")
        {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: Value::Null,
                top_level_error: Some(customer_consent_invalid_state_error(
                    field,
                    &marketing_state,
                )),
            };
        }

        let Some(existing_customer) =
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request)
        else {
            let user_error = if is_email {
                json!({
                    "field": ["input", "customerId"],
                    "message": "Customer not found",
                    "code": "INVALID"
                })
            } else {
                json!({
                    "field": Value::Null,
                    "message": "Customer not found",
                    "code": Value::Null
                })
            };
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(Value::Null, vec![user_error]),
                top_level_error: None,
            };
        };

        let marketing_opt_in_level = resolved_string_field(&consent, "marketingOptInLevel")
            .unwrap_or_else(|| current_consent_opt_in_level(&existing_customer, is_email));
        let consent_updated_at = resolved_string_field(&consent, "consentUpdatedAt");

        if let Some(consent_updated_at) = consent_updated_at.as_deref() {
            if customer_consent_updated_at_is_future(consent_updated_at) {
                self.record_customer_consent_log(
                    request,
                    query,
                    variables,
                    &field.name,
                    Vec::new(),
                    "failed",
                );
                let customer = if is_email {
                    existing_customer.clone()
                } else {
                    Value::Null
                };
                return CustomerConsentOutcome {
                    payload: customer_consent_payload(
                        customer,
                        vec![customer_consent_user_error(
                            vec!["input", consent_key, "consentUpdatedAt"],
                            "Consent updated at must not be in the future",
                            "INVALID",
                        )],
                    ),
                    top_level_error: None,
                };
            }
        }

        if marketing_state == "PENDING" && marketing_opt_in_level != "CONFIRMED_OPT_IN" {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            let customer = if is_email {
                existing_customer.clone()
            } else {
                Value::Null
            };
            return CustomerConsentOutcome {
                payload: customer_consent_payload(
                    customer,
                    vec![customer_consent_user_error(
                        vec!["input", consent_key, "marketingOptInLevel"],
                        "Marketing opt in level must be confirmed opt-in for pending consent state",
                        "INVALID",
                    )],
                ),
                top_level_error: None,
            };
        }

        if !is_email && !customer_has_default_phone(&existing_customer) {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(
                    Value::Null,
                    vec![customer_consent_user_error(
                        vec!["input", "smsMarketingConsent"],
                        "A phone number is required to set the SMS consent state.",
                        "INVALID",
                    )],
                ),
                top_level_error: None,
            };
        }

        if is_email && !customer_has_default_email(&existing_customer) {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                vec![customer_id],
                "staged",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(existing_customer, Vec::new()),
                top_level_error: None,
            };
        }

        let updated_at = consent_updated_at
            .or_else(|| current_consent_updated_at(&existing_customer, is_email))
            .unwrap_or_else(|| "2026-04-25T01:41:06Z".to_string());
        let mut customer = existing_customer;
        apply_customer_marketing_consent(
            &mut customer,
            is_email,
            &marketing_state,
            &marketing_opt_in_level,
            Some(updated_at.as_str()),
        );
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());
        self.record_customer_consent_log(
            request,
            query,
            variables,
            &field.name,
            vec![customer_id],
            "staged",
        );
        CustomerConsentOutcome {
            payload: customer_consent_payload(customer, Vec::new()),
            top_level_error: None,
        }
    }

    fn record_customer_consent_log(
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
}

/// Validates the `taxExemptions` argument of the `customer*TaxExemptions`
/// mutations before any staging, mirroring Shopify's enum coercion errors:
/// invalid literals raise `argumentLiteralsIncompatible`, invalid variable
/// values raise `INVALID_VARIABLE`. Returns `None` when every value is known.
pub(in crate::proxy) fn customer_tax_exemptions_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    for field in fields {
        if !matches!(
            field.name.as_str(),
            "customerAddTaxExemptions"
                | "customerRemoveTaxExemptions"
                | "customerReplaceTaxExemptions"
        ) {
            continue;
        }
        let Some(raw_value) = field.raw_arguments.get("taxExemptions") else {
            continue;
        };
        if let Some(literal) = raw_tax_exemption_literal(raw_value) {
            return Some(ok_json(json!({
                "errors": [{
                    "message": format!("Argument 'taxExemptions' has an invalid value [{literal}]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?"),
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "argumentName": "taxExemptions"
                    }
                }]
            })));
        }
        if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
            return Some(tax_exemption_invalid_variable_response(query, &invalid));
        }
    }
    None
}

fn customer_tax_exemptions_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_tax_exemptions_user_error() -> Value {
    user_error_omit_code(["customerId"], "Customer does not exist.", None)
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

/// Outcome of a single `customer*MarketingConsentUpdate` root field: either a
/// projected payload (with field-level `userErrors`) or a top-level GraphQL
/// error (Shopify raises these for disallowed marketing states).
struct CustomerConsentOutcome {
    payload: Value,
    top_level_error: Option<Value>,
}

fn customer_consent_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_consent_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn customer_consent_invalid_state_error(field: &RootFieldSelection, state: &str) -> Value {
    json!({
        "message": format!("Cannot specify {state} as a marketing state input"),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "extensions": { "code": "INVALID" },
        "path": [field.response_key.clone()]
    })
}

fn customer_has_default_email(customer: &Value) -> bool {
    customer
        .get("defaultEmailAddress")
        .and_then(|contact| contact.get("emailAddress"))
        .and_then(Value::as_str)
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_has_default_phone(customer: &Value) -> bool {
    customer
        .get("defaultPhoneNumber")
        .and_then(|contact| contact.get("phoneNumber"))
        .and_then(Value::as_str)
        .is_some_and(|phone| !phone.trim().is_empty())
}

fn current_consent_opt_in_level(customer: &Value, is_email: bool) -> String {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingOptInLevel"))
        .and_then(Value::as_str)
        .unwrap_or("SINGLE_OPT_IN")
        .to_string()
}

fn current_consent_updated_at(customer: &Value, is_email: bool) -> Option<String> {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingUpdatedAt"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn customer_consent_updated_at_is_future(value: &str) -> bool {
    let Some(updated_at) = parse_rfc3339_epoch_seconds(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    updated_at > now.as_secs() as i64
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
    user_error_omit_code(field, message, None)
}

fn customer_user_error_with_code(field: Value, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
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
    verified_email_default: bool,
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
        .unwrap_or(verified_email_default);
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

/// `customerCreate` rejects nested resource ids on creation: an `id` key inside
/// any `addresses[]` or `metafields[]` input object yields a user error and a
/// null customer. Addresses are checked before metafields so the surfaced error
/// path matches Shopify's ordering when both are present.
fn customer_create_nested_id_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    for (collection, label) in [("addresses", "address"), ("metafields", "metafield")] {
        if let Some(ResolvedValue::List(entries)) = input.get(collection) {
            for (index, entry) in entries.iter().enumerate() {
                if let ResolvedValue::Object(object) = entry {
                    if object.contains_key("id") {
                        return Some(customer_user_error(
                            json!([collection, index.to_string(), "id"]),
                            &format!("Cannot specify {label} ID on creation"),
                        ));
                    }
                }
            }
        }
    }
    None
}

fn customer_create_verified_email_default(
    request: &Request,
    input: &NormalizedCustomerInput,
) -> bool {
    if input
        .email
        .as_ref()
        .and_then(|value| value.as_ref())
        .is_none()
    {
        return false;
    }
    admin_graphql_version(&request.path).is_some_and(|version| !version_at_least(version, 2026, 4))
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

/// Default `Customer.defaultEmailAddress` shape. Real Shopify always returns a
/// `CustomerEmailAddress` (with `NOT_SUBSCRIBED` marketing defaults) whenever an
/// email is present, and `null` otherwise. Inline consent overwrites the
/// marketing fields via [`apply_customer_marketing_consent`].
fn default_email_address_value(email: Option<&str>) -> Value {
    match email.filter(|value| !value.is_empty()) {
        Some(email) => json!({
            "emailAddress": email,
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.defaultPhoneNumber` shape (see [`default_email_address_value`]).
fn default_phone_number_value(phone: Option<&str>) -> Value {
    match phone.filter(|value| !value.is_empty()) {
        Some(phone) => json!({
            "phoneNumber": phone,
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": Value::Null,
            "marketingCollectedFrom": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.emailMarketingConsent` compatibility object.
fn email_marketing_consent_value(email: Option<&str>) -> Value {
    match email.filter(|value| !value.is_empty()) {
        Some(_) => json!({
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "consentUpdatedAt": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.smsMarketingConsent` compatibility object.
fn sms_marketing_consent_value(phone: Option<&str>) -> Value {
    match phone.filter(|value| !value.is_empty()) {
        Some(_) => json!({
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "consentUpdatedAt": Value::Null,
            "consentCollectedFrom": Value::Null
        }),
        None => Value::Null,
    }
}

/// Overwrite the marketing-consent fields of a staged customer record from a
/// resolved consent state. `is_email` selects email vs SMS; the latter also
/// carries `consentCollectedFrom` / `marketingCollectedFrom` defaulting to
/// `"OTHER"` (the value Shopify reports for API-set consent).
fn apply_customer_marketing_consent(
    customer: &mut Value,
    is_email: bool,
    marketing_state: &str,
    marketing_opt_in_level: &str,
    updated_at: Option<&str>,
) {
    let Some(object) = customer.as_object_mut() else {
        return;
    };
    if is_email {
        if let Some(contact) = object
            .get_mut("defaultEmailAddress")
            .and_then(Value::as_object_mut)
        {
            contact.insert("marketingState".to_string(), json!(marketing_state));
            contact.insert(
                "marketingOptInLevel".to_string(),
                json!(marketing_opt_in_level),
            );
            contact.insert("marketingUpdatedAt".to_string(), json!(updated_at));
        }
        object.insert(
            "emailMarketingConsent".to_string(),
            json!({
                "marketingState": marketing_state,
                "marketingOptInLevel": marketing_opt_in_level,
                "consentUpdatedAt": updated_at
            }),
        );
    } else {
        if let Some(contact) = object
            .get_mut("defaultPhoneNumber")
            .and_then(Value::as_object_mut)
        {
            contact.insert("marketingState".to_string(), json!(marketing_state));
            contact.insert(
                "marketingOptInLevel".to_string(),
                json!(marketing_opt_in_level),
            );
            contact.insert("marketingUpdatedAt".to_string(), json!(updated_at));
            contact.insert("marketingCollectedFrom".to_string(), json!("OTHER"));
        }
        object.insert(
            "smsMarketingConsent".to_string(),
            json!({
                "marketingState": marketing_state,
                "marketingOptInLevel": marketing_opt_in_level,
                "consentUpdatedAt": updated_at,
                "consentCollectedFrom": "OTHER"
            }),
        );
    }
}

/// Apply inline `emailMarketingConsent` / `smsMarketingConsent` from a
/// `CustomerInput` onto a freshly built customer record. Callers must have
/// already validated that the matching contact (email/phone) is present and
/// that the marketing state is not `REDACTED`.
fn apply_inline_consent_from_input(customer: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    for (key, is_email) in [
        ("emailMarketingConsent", true),
        ("smsMarketingConsent", false),
    ] {
        let Some(consent) = resolved_object_field(input, key) else {
            continue;
        };
        let Some(marketing_state) = resolved_string_field(&consent, "marketingState") else {
            continue;
        };
        if marketing_state.is_empty() {
            continue;
        }
        let opt_in = resolved_string_field(&consent, "marketingOptInLevel")
            .unwrap_or_else(|| "SINGLE_OPT_IN".to_string());
        let updated_at = resolved_string_field(&consent, "consentUpdatedAt");
        apply_customer_marketing_consent(
            customer,
            is_email,
            &marketing_state,
            &opt_in,
            updated_at.as_deref(),
        );
    }
}

fn customer_record(input: CustomerRecordInput<'_>) -> Value {
    let first_value = input.first.filter(|value| !value.is_empty());
    let last_value = input.last.filter(|value| !value.is_empty());
    let display_name = customer_display_name(first_value, last_value, input.email);
    let metafields = if input.loyalty.is_null() {
        json!({ "nodes": [], "pageInfo": empty_page_info() })
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
        "defaultEmailAddress": default_email_address_value(input.email),
        "defaultPhoneNumber": default_phone_number_value(input.phone),
        "emailMarketingConsent": email_marketing_consent_value(input.email),
        "smsMarketingConsent": sms_marketing_consent_value(input.phone),
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
    let id = synthetic_shopify_gid("MailingAddress", index + 1);
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

fn customer_address_payload(address: Value, user_errors: Vec<Value>) -> Value {
    json!({ "address": address, "userErrors": user_errors })
}

fn customer_address_resource_not_found_error(response_key: &str) -> Value {
    json!({
        "message": "invalid id",
        "extensions": { "code": "RESOURCE_NOT_FOUND" },
        "path": [response_key]
    })
}

fn customer_address_nodes(customer: &Value) -> Vec<Value> {
    customer
        .get("addressesV2")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn customer_default_address_id(customer: &Value) -> Option<String> {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn customer_address_node_index(nodes: &[Value], address_id: &str) -> Option<usize> {
    nodes
        .iter()
        .position(|node| node.get("id").and_then(Value::as_str) == Some(address_id))
}

/// Identity key for duplicate detection: the full node minus its synthetic id.
/// Derived fields (`name`, `formattedArea`, `country`/`province` names) are a
/// deterministic function of the inputs, so comparing the whole node is
/// equivalent to comparing the input field-set.
fn customer_address_dedup_key(node: &Value) -> String {
    let mut node = node.clone();
    if let Some(object) = node.as_object_mut() {
        object.remove("id");
    }
    serde_json::to_string(&node).unwrap_or_default()
}

/// Rebuild a customer's inline `addressesV2` connection (nodes/edges/pageInfo)
/// and `defaultAddress` from the given ordered node list. `default_id` selects
/// which node (if any) is the default. Cursors are the deterministic
/// `cursor:<id>` form, matched leniently as `any-string` by the parity rules.
fn customer_rebuild_addresses(customer: &mut Value, nodes: Vec<Value>, default_id: Option<&str>) {
    let edges = nodes
        .iter()
        .map(|node| json!({ "cursor": customer_address_cursor(node), "node": node.clone() }))
        .collect::<Vec<_>>();
    let start_cursor = nodes.first().and_then(customer_address_cursor);
    let end_cursor = nodes.last().and_then(customer_address_cursor);
    let default_address = default_id
        .and_then(|id| {
            nodes
                .iter()
                .find(|node| node.get("id").and_then(Value::as_str) == Some(id))
        })
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(object) = customer.as_object_mut() {
        object.insert("defaultAddress".to_string(), default_address);
        object.insert(
            "addressesV2".to_string(),
            json!({
                "nodes": nodes,
                "edges": edges,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": start_cursor,
                    "endCursor": end_cursor
                }
            }),
        );
    }
}

/// Build a single mailing-address node for the standalone address mutations.
///
/// Unlike `customer_mailing_address` (used for inline `customerCreate`/`Set`
/// address arrays, which key errors on `addresses[i]` and never blank-defaults),
/// this:
///   * keys validation errors on `["address", field]`,
///   * never rejects a blank address (Shopify accepts `{}`),
///   * defaults `firstName`/`lastName` to the owning customer's name when absent,
///   * merges over an `existing` node for updates (input fields override; absent
///     fields keep the stored value).
///
/// Returns `(Some(node), [])` on success or `(None, errors)` on validation
/// failure.
fn customer_address_input_node(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    customer_first: Option<&str>,
    customer_last: Option<&str>,
    id: &str,
) -> (Option<Value>, Vec<Value>) {
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
                    json!(["address", field]),
                    &format!("{label} is too long (maximum is 255 characters)"),
                ));
            }
            if customer_address_contains_html(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain HTML tags"),
                ));
            }
            if matches!(field, "city" | "zip" | "phone") && customer_address_contains_url(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain URL"),
                ));
            }
            if customer_address_contains_emoji(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain emojis"),
                ));
            }
        }
    }

    // Effective string value for a field: input value when the key is present
    // (trimmed; empty → None), otherwise the existing node's stored value.
    let field_value = |key: &str| -> Option<String> {
        if input.contains_key(key) {
            customer_address_string(input, key)
        } else {
            existing
                .and_then(|node| node.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        }
    };

    let country_present = input.contains_key("countryCode")
        || input.contains_key("countryCodeV2")
        || input.contains_key("country");
    let country_raw = if country_present {
        customer_address_string(input, "countryCode")
            .or_else(|| customer_address_string(input, "countryCodeV2"))
            .or_else(|| customer_address_string(input, "country"))
    } else {
        existing
            .and_then(|node| node.get("countryCodeV2"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let country = match country_raw.as_deref().and_then(customer_country_from_input) {
        Some(country) => Some(country),
        None if country_raw.is_some() => {
            errors.push(customer_user_error(
                json!(["address", "country"]),
                "Country is invalid",
            ));
            None
        }
        None => None,
    };

    let province_present = input.contains_key("provinceCode") || input.contains_key("province");
    let province_raw = if province_present {
        customer_address_string(input, "provinceCode")
            .or_else(|| customer_address_string(input, "province"))
    } else {
        existing
            .and_then(|node| node.get("provinceCode"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let province = match (&country, province_raw.as_deref()) {
        (Some(country), Some(raw_province)) => {
            match customer_province_from_input(country.code, raw_province) {
                Some(province) => province,
                None => {
                    errors.push(customer_user_error(
                        json!(["address", "province"]),
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
        return (None, errors);
    }

    let first_name = field_value("firstName").or_else(|| customer_first.map(str::to_string));
    let last_name = field_value("lastName").or_else(|| customer_last.map(str::to_string));
    let address1 = field_value("address1");
    let address2 = field_value("address2");
    let city = field_value("city");
    let company = field_value("company");
    let zip = field_value("zip");
    let phone = field_value("phone");
    let name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area =
        customer_formatted_area(city.as_deref(), country.as_ref(), province.as_ref());
    (
        Some(json!({
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
        })),
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

/// The full `TaxExemption` enum exposed by the Shopify Admin GraphQL schema. This is the
/// authoritative set of accepted values for `companyLocationTaxSettingsUpdate`'s exemption
/// arguments, and is also what Shopify echoes back (verbatim, comma-joined) inside the
/// `INVALID_VARIABLE` coercion error when an unknown value is supplied.
const TAX_EXEMPTION_VALUES: &[&str] = &[
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

/// An invalid `[TaxExemption!]` variable value detected during request validation.
struct InvalidTaxExemptionVariable {
    variable_name: String,
    /// The full provided value, echoed back in `extensions.value`.
    provided: Value,
    /// `(list index, invalid value)` for every element that is not a known exemption.
    problems: Vec<(usize, String)>,
}

fn b2b_tax_settings_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
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
            if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
                return Some(tax_exemption_invalid_variable_response(query, &invalid));
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

fn tax_exemption_invalid_variable(value: &RawArgumentValue) -> Option<InvalidTaxExemptionVariable> {
    let RawArgumentValue::Variable {
        name,
        value: Some(resolved),
    } = value
    else {
        return None;
    };
    let mut problems = Vec::new();
    if let ResolvedValue::List(items) = resolved {
        for (index, item) in items.iter().enumerate() {
            if let ResolvedValue::String(item) = item {
                if !is_known_tax_exemption(item) {
                    problems.push((index, item.clone()));
                }
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    Some(InvalidTaxExemptionVariable {
        variable_name: name.clone(),
        provided: resolved_value_json(resolved),
        problems,
    })
}

fn tax_exemption_invalid_variable_response(
    query: &str,
    invalid: &InvalidTaxExemptionVariable,
) -> Response {
    let one_of = TAX_EXEMPTION_VALUES.join(", ");
    let problems: Vec<Value> = invalid
        .problems
        .iter()
        .map(|(index, value)| {
            json!({
                "path": [index],
                "explanation": format!("Expected \"{value}\" to be one of: {one_of}"),
            })
        })
        .collect();
    let (first_index, first_value) = &invalid.problems[0];
    let declared_type = graphql_variable_definition_type(query, &invalid.variable_name)
        .unwrap_or_else(|| "[TaxExemption!]".to_string());
    let message = format!(
        "Variable ${} of type {declared_type} was provided invalid value for {first_index} (Expected \"{first_value}\" to be one of: {one_of})",
        invalid.variable_name
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) =
        graphql_variable_definition_location(query, &invalid.variable_name)
    {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": invalid.provided,
            "problems": problems,
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

/// Members of the `CustomerSmsMarketingState` GraphQL enum. Values outside this set
/// (e.g. `INVALID`) fail variable coercion *before* the resolver checks for
/// valid-but-disallowed input states (`NOT_SUBSCRIBED`, `REDACTED`). `INVALID` is a
/// real member of the *email* enum but not the SMS one, hence the channel-specific set.
const SMS_MARKETING_STATES: &[&str] = &[
    "NOT_SUBSCRIBED",
    "PENDING",
    "SUBSCRIBED",
    "UNSUBSCRIBED",
    "REDACTED",
];

/// Validates the `smsMarketingConsent.marketingState` enum value of
/// `customerSmsMarketingConsentUpdate` before any staging. Shopify rejects values
/// outside `CustomerSmsMarketingState` at variable-coercion time with an
/// `INVALID_VARIABLE` error, returning `None` when the state is a known member.
pub(in crate::proxy) fn customer_sms_consent_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    for field in fields {
        if field.name != "customerSmsMarketingConsentUpdate" {
            continue;
        }
        let Some(RawArgumentValue::Variable {
            name,
            value: Some(resolved),
        }) = field.raw_arguments.get("input")
        else {
            continue;
        };
        let ResolvedValue::Object(input) = resolved else {
            continue;
        };
        let Some(ResolvedValue::Object(consent)) = input.get("smsMarketingConsent") else {
            continue;
        };
        let Some(ResolvedValue::String(state)) = consent.get("marketingState") else {
            continue;
        };
        if SMS_MARKETING_STATES.contains(&state.as_str()) {
            continue;
        }
        return Some(sms_consent_invalid_variable_response(
            query, name, resolved, state,
        ));
    }
    None
}

fn sms_consent_invalid_variable_response(
    query: &str,
    variable_name: &str,
    input: &ResolvedValue,
    state: &str,
) -> Response {
    let one_of = SMS_MARKETING_STATES.join(", ");
    let declared_type = graphql_variable_definition_type(query, variable_name)
        .unwrap_or_else(|| "CustomerSmsMarketingConsentUpdateInput!".to_string());
    let explanation = format!("Expected \"{state}\" to be one of: {one_of}");
    let message = format!(
        "Variable ${variable_name} of type {declared_type} was provided invalid value for smsMarketingConsent.marketingState ({explanation})"
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) = graphql_variable_definition_location(query, variable_name) {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(input),
            "problems": [{
                "path": ["smsMarketingConsent", "marketingState"],
                "explanation": explanation,
            }],
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

/// Resolves the 1-based (line, column) of a variable *definition* (`$name`) in the query
/// document. Shopify anchors `INVALID_VARIABLE` coercion errors to the variable definition,
/// which is always the first `$name` occurrence (definitions precede usages).
pub(in crate::proxy) fn graphql_variable_definition_location(
    query: &str,
    variable_name: &str,
) -> Option<(usize, usize)> {
    let needle = format!("${variable_name}");
    let bytes = query.as_bytes();
    let mut search_from = 0;
    while let Some(relative) = query[search_from..].find(&needle) {
        let start = search_from + relative;
        let after = start + needle.len();
        let is_boundary = match bytes.get(after) {
            None => true,
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
        };
        if is_boundary {
            let mut line = 1usize;
            let mut column = 1usize;
            for (index, ch) in query.char_indices() {
                if index == start {
                    return Some((line, column));
                }
                if ch == '\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
            }
            return Some((line, column));
        }
        search_from = after;
    }
    None
}

/// Resolves the declared GraphQL type of a variable (`$name: <TYPE>`) from the query
/// document, e.g. `[TaxExemption!]!` or `CustomerSmsMarketingConsentUpdateInput!`.
/// Shopify echoes the exact declared type in `INVALID_VARIABLE` coercion messages, so
/// we parse it from the variable definition rather than hardcoding a single shape.
pub(in crate::proxy) fn graphql_variable_definition_type(
    query: &str,
    variable_name: &str,
) -> Option<String> {
    let needle = format!("${variable_name}");
    let bytes = query.as_bytes();
    let mut search_from = 0;
    while let Some(relative) = query[search_from..].find(&needle) {
        let start = search_from + relative;
        let after = start + needle.len();
        let is_boundary = match bytes.get(after) {
            None => true,
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
        };
        if is_boundary {
            // A variable *definition* is `$name: <TYPE>`; a *usage* (`field(arg: $name)`)
            // has no `:` immediately following. Only the definition yields a type.
            if let Some(type_part) = query[after..].trim_start().strip_prefix(':') {
                let declared: String = type_part
                    .trim_start()
                    .chars()
                    .take_while(|c| !matches!(c, ',' | ')' | '=' | '\n' | '\r' | '{'))
                    .collect();
                let declared = declared.trim();
                if !declared.is_empty() {
                    return Some(declared.to_string());
                }
            }
        }
        search_from = after;
    }
    None
}

fn is_known_tax_exemption(value: &str) -> bool {
    TAX_EXEMPTION_VALUES.contains(&value)
}

fn product_tail_invalid_enum_response(
    query: &str,
    operation_type: OperationType,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    if operation_type != OperationType::Mutation || fields.len() != 1 {
        return None;
    }
    let field = fields.first()?;
    match field.name.as_str() {
        "publicationCreate" => publication_default_state_invalid_variable(field).map(
            |(variable_name, provided, state)| {
                publication_default_state_invalid_response(query, &variable_name, &provided, &state)
            },
        ),
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

/// Valid values for `PublicationDefaultState` (the enum behind
/// `PublicationCreateInput.defaultState`).
const PUBLICATION_DEFAULT_STATE_VALUES: &[&str] = &["EMPTY", "ALL_PRODUCTS"];
const PUBLICATION_UPDATE_LIMIT: usize = 50;

fn publication_create_name(id: &str, catalog: Option<&Value>) -> String {
    catalog
        .and_then(|catalog| catalog.get("title"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            let suffix = resource_id_path_tail(id);
            format!("Publication {suffix}")
        })
}

fn publication_catalog_not_found_payload(catalog_id: &str) -> Value {
    json!({
        "publication": null,
        "userErrors": [publication_error(
            vec!["input", "catalogId"],
            &format!("A catalog was not found for id= {catalog_id}."),
            "CATALOG_NOT_FOUND",
        )]
    })
}

fn publication_not_found_payload(root_field: &str) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(root_field.to_string(), Value::Null);
    payload.insert(
        "userErrors".to_string(),
        json!([publication_error(
            vec!["id"],
            "Publication was not found",
            "PUBLICATION_NOT_FOUND",
        )]),
    );
    Value::Object(payload)
}

fn publication_update_limit_field(
    publishables_to_add: &[String],
    publishables_to_remove: &[String],
) -> Vec<&'static str> {
    let field_name = if publishables_to_add.len() > PUBLICATION_UPDATE_LIMIT {
        "publishablesToAdd"
    } else if publishables_to_remove.len() > PUBLICATION_UPDATE_LIMIT
        || publishables_to_add.is_empty()
    {
        "publishablesToRemove"
    } else {
        "publishablesToAdd"
    };
    vec!["input", field_name, "51"]
}

fn publication_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn publication_indexed_error(field_name: &str, index: usize, message: &str, code: &str) -> Value {
    json!({
        "field": ["input", field_name, index.to_string()],
        "message": message,
        "code": code
    })
}

/// When `publicationCreate`'s `$input.defaultState` is not a valid
/// `PublicationDefaultState`, returns the `(variable_name, provided_input,
/// invalid_value)` needed to build the `INVALID_VARIABLE` coercion error.
fn publication_default_state_invalid_variable(
    field: &RootFieldSelection,
) -> Option<(String, Value, String)> {
    let Some(RawArgumentValue::Variable {
        name,
        value: Some(ResolvedValue::Object(input)),
    }) = field.raw_arguments.get("input")
    else {
        return None;
    };
    let state = resolved_string_field(input, "defaultState")?;
    if PUBLICATION_DEFAULT_STATE_VALUES.contains(&state.as_str()) {
        return None;
    }
    Some((
        name.clone(),
        resolved_value_json(&ResolvedValue::Object(input.clone())),
        state,
    ))
}

/// Builds the GraphQL `INVALID_VARIABLE` coercion error Shopify returns for an
/// out-of-range `publicationCreate` `defaultState`, anchored to the `$input`
/// variable definition.
fn publication_default_state_invalid_response(
    query: &str,
    variable_name: &str,
    provided: &Value,
    state: &str,
) -> Response {
    let one_of = PUBLICATION_DEFAULT_STATE_VALUES.join(", ");
    let message = format!(
        "Variable ${variable_name} of type PublicationCreateInput! was provided invalid value for defaultState (Expected \"{state}\" to be one of: {one_of})"
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) = graphql_variable_definition_location(query, variable_name) {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": provided,
            "problems": [{
                "path": ["defaultState"],
                "explanation": format!("Expected \"{state}\" to be one of: {one_of}"),
            }],
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

fn product_full_sync_payload_selection_error(field: &RootFieldSelection) -> Option<Value> {
    let selected = field
        .selection
        .iter()
        .find(|selection| selection.name == "job")?;
    Some(json!({
        "message": "Field 'job' doesn't exist on type 'ProductFullSyncPayload'",
        "path": [
            field.response_key.clone(),
            selected.response_key.clone()
        ],
        "extensions": {
            "code": "undefinedField",
            "typeName": "ProductFullSyncPayload",
            "fieldName": "job"
        }
    }))
}

fn product_full_sync_updated_at_range_invalid(
    before_updated_at: Option<&str>,
    updated_at_since: Option<&str>,
) -> bool {
    let (Some(before_updated_at), Some(updated_at_since)) = (before_updated_at, updated_at_since)
    else {
        return false;
    };
    let Some(before_updated_at) = parse_rfc3339_epoch_seconds(before_updated_at) else {
        return false;
    };
    let Some(updated_at_since) = parse_rfc3339_epoch_seconds(updated_at_since) else {
        return false;
    };
    updated_at_since > before_updated_at
}

/// ProductFeed `country` is a Shopify `CountryCode` — an ISO 3166-1 alpha-2 code
/// (two uppercase letters). Anything else is rejected at the resolver.
fn is_valid_product_feed_country(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// ProductFeed `language` is a Shopify `LanguageCode` — an ISO 639-1 alpha-2 code,
/// optionally with an alpha-2 region suffix (e.g. `EN`, `ZH_CN`).
fn is_valid_product_feed_language(code: &str) -> bool {
    let mut parts = code.split('_');
    let valid_segment =
        |segment: &str| segment.len() == 2 && segment.bytes().all(|byte| byte.is_ascii_uppercase());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(language), None, None) => valid_segment(language),
        (Some(language), Some(region), None) => valid_segment(language) && valid_segment(region),
        _ => false,
    }
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
            errors.push(b2b_company_user_error(
                field,
                "Phone is invalid",
                "INVALID",
                None,
            ));
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
            errors.push(b2b_company_user_error(
                field,
                "Invalid input.",
                "INVALID_INPUT",
                None,
            ));
        } else if !shipping_present {
            let mut field = prefix.to_vec();
            field.push("shippingAddress");
            errors.push(b2b_company_user_error(
                field,
                "Invalid input.",
                "INVALID_INPUT",
                None,
            ));
        }
    }
    // An explicit null taxExempt is rejected; an absent taxExempt defaults to false.
    if matches!(input.get("taxExempt"), Some(ResolvedValue::Null)) {
        let mut field = prefix.to_vec();
        field.push("taxExempt");
        errors.push(b2b_company_user_error(
            field,
            "Invalid input.",
            "INVALID_INPUT",
            None,
        ));
    }
    // Each nested CompanyAddressInput is validated under its own field path, so a
    // malformed shipping/billing address is rejected before the location is staged
    // (matching live Admin's read-after-write contract) rather than only failing
    // later at Shopify commit time.
    for address_field in ["shippingAddress", "billingAddress"] {
        if let Some(address) = resolved_object_field(input, address_field) {
            let mut address_prefix = prefix.to_vec();
            address_prefix.push(address_field);
            errors.extend(b2b_address_input_errors(&address, &address_prefix));
        }
    }
    errors
}

/// Validates a CompanyAddressInput (shippingAddress/billingAddress) the way live
/// Admin does before staging: country code against the supported country catalog,
/// zone code against the country's subdivisions, postal code shape per country,
/// and free-text fields for HTML markup/emoji (plus embedded URLs in name fields).
/// `prefix` is the full field path to the address object, e.g.
/// `["input", "shippingAddress"]` or `["input", "companyLocation", "shippingAddress"]`.
fn b2b_address_input_errors(
    address: &BTreeMap<String, ResolvedValue>,
    prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();

    // Country: an unknown code is rejected, and the resolved catalog gates the
    // zone and zip checks below (an absent or invalid country skips them).
    let country = match resolved_string_field(address, "countryCode") {
        Some(code) => match b2b_country_catalog_by_code(&code) {
            Some(catalog) => Some(catalog),
            None => {
                let mut field = prefix.to_vec();
                field.push("countryCode");
                errors.push(b2b_company_user_error(
                    field,
                    "Country code is invalid",
                    "INVALID",
                    None,
                ));
                None
            }
        },
        None => None,
    };

    if let Some((country_code, zones)) = country {
        // Zone: only validated when the country publishes subdivisions (e.g. SG
        // has none) and a zoneCode was supplied.
        if let Some(zone_code) = resolved_string_field(address, "zoneCode") {
            if !zones.is_empty() && b2b_zone_name_by_code(zones, &zone_code).is_none() {
                let mut field = prefix.to_vec();
                field.push("zoneCode");
                errors.push(b2b_company_user_error(
                    field,
                    "Zone code is invalid",
                    "INVALID",
                    None,
                ));
            }
        }
        // Zip: postal-code shape (and the US zone-prefix range) per country.
        if let Some(zip) = resolved_string_field(address, "zip") {
            let zone_code = resolved_string_field(address, "zoneCode");
            if !b2b_postal_code_valid(country_code, zone_code.as_deref(), &zip) {
                let mut field = prefix.to_vec();
                field.push("zip");
                errors.push(b2b_company_user_error(
                    field,
                    "Zip is invalid",
                    "INVALID",
                    None,
                ));
            }
        }
    }

    // Free-text fields: HTML markup and emoji are always rejected; name fields
    // additionally reject embedded URLs. Field order matches live Admin.
    for (field_name, label, reject_url) in [
        ("recipient", "Recipient", false),
        ("address1", "Address1", false),
        ("address2", "Address2", false),
        ("city", "City", false),
        ("firstName", "First name", true),
        ("lastName", "Last name", true),
    ] {
        if let Some(value) = resolved_string_field(address, field_name) {
            let invalid = b2b_contains_html_tags(&value)
                || b2b_contains_emoji(&value)
                || (reject_url && b2b_contains_url_substring(&value));
            if invalid {
                let mut field = prefix.to_vec();
                field.push(field_name);
                errors.push(b2b_company_user_error(
                    field,
                    &format!("{label} is invalid"),
                    "INVALID",
                    None,
                ));
            }
        }
    }

    errors
}

/// Empty subdivision list for countries with no zone catalog (e.g. Singapore).
const B2B_NO_ZONES: &[(&str, &str)] = &[];

/// The supported B2B country catalog: a country code resolves to its canonical
/// code and subdivision (zone) list. Countries outside this set are reported as
/// invalid, matching the captured live-Admin validation boundary for the B2B
/// address scenarios.
fn b2b_country_catalog_by_code(
    code: &str,
) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
    match code.to_ascii_uppercase().as_str() {
        "CA" => Some(("CA", B2B_CANADA_ZONES)),
        "US" => Some(("US", B2B_UNITED_STATES_ZONES)),
        "SG" => Some(("SG", B2B_NO_ZONES)),
        _ => None,
    }
}

/// Resolves a zone code (case-insensitively) to its subdivision name, or `None`
/// when the code is not a subdivision of the country.
fn b2b_zone_name_by_code<'a>(zones: &'a [(&str, &str)], code: &str) -> Option<&'a str> {
    let normalized = code.to_ascii_uppercase();
    zones
        .iter()
        .find(|(zone_code, _)| *zone_code == normalized)
        .map(|(_, name)| *name)
}

/// Validates a postal code's shape against the country (and, for the US, the
/// zone-specific prefix range). Countries without a known format accept any zip.
fn b2b_postal_code_valid(country_code: &str, zone_code: Option<&str>, zip: &str) -> bool {
    match country_code.to_ascii_uppercase().as_str() {
        "CA" => b2b_canada_postal_code_valid(zip),
        "US" => b2b_us_postal_code_valid(zip, zone_code),
        "SG" => b2b_singapore_postal_code_valid(zip),
        _ => true,
    }
}

fn b2b_us_postal_code_valid(zip: &str, zone_code: Option<&str>) -> bool {
    let normalized = zip.trim();
    if !b2b_us_postal_code_shape_valid(normalized) {
        return false;
    }
    b2b_us_zone_postal_code_valid(normalized, zone_code)
}

fn b2b_us_postal_code_shape_valid(zip: &str) -> bool {
    let chars: Vec<char> = zip.chars().collect();
    match chars.len() {
        5 => chars.iter().all(char::is_ascii_digit),
        10 => {
            chars[5] == '-'
                && chars
                    .iter()
                    .enumerate()
                    .all(|(index, character)| index == 5 || character.is_ascii_digit())
        }
        _ => false,
    }
}

fn b2b_us_zone_postal_code_valid(zip: &str, zone_code: Option<&str>) -> bool {
    match zone_code {
        Some(code) => match code.to_ascii_uppercase().as_str() {
            "CA" => b2b_zip_prefix_between(zip, 900, 961),
            _ => true,
        },
        None => true,
    }
}

fn b2b_zip_prefix_between(zip: &str, min: i64, max: i64) -> bool {
    match zip.get(0..3).and_then(|prefix| prefix.parse::<i64>().ok()) {
        Some(prefix) => prefix >= min && prefix <= max,
        None => false,
    }
}

fn b2b_canada_postal_code_valid(zip: &str) -> bool {
    let compact: Vec<char> = zip
        .to_ascii_uppercase()
        .chars()
        .filter(|character| *character != ' ' && *character != '-')
        .collect();
    if compact.len() != 6 {
        return false;
    }
    b2b_canada_postal_alpha(compact[0])
        && compact[1].is_ascii_digit()
        && b2b_canada_postal_alpha(compact[2])
        && compact[3].is_ascii_digit()
        && b2b_canada_postal_alpha(compact[4])
        && compact[5].is_ascii_digit()
}

fn b2b_canada_postal_alpha(character: char) -> bool {
    matches!(
        character,
        'A' | 'B'
            | 'C'
            | 'E'
            | 'G'
            | 'H'
            | 'J'
            | 'K'
            | 'L'
            | 'M'
            | 'N'
            | 'P'
            | 'R'
            | 'S'
            | 'T'
            | 'V'
            | 'X'
            | 'Y'
    )
}

fn b2b_singapore_postal_code_valid(zip: &str) -> bool {
    let compact = zip.trim();
    compact.chars().count() == 6 && compact.chars().all(|character| character.is_ascii_digit())
}

fn b2b_contains_emoji(value: &str) -> bool {
    value.chars().any(|character| {
        let code = character as u32;
        (0x1f000..=0x1faff).contains(&code)
            || (0x2600..=0x27bf).contains(&code)
            || (0xfe00..=0xfe0f).contains(&code)
            || code == 0x200d
    })
}

fn b2b_contains_url_substring(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("http://") || lowered.contains("https://") || lowered.contains("www.")
}

/// Canada subdivision (province/territory) catalog.
const B2B_CANADA_ZONES: &[(&str, &str)] = &[
    ("AB", "Alberta"),
    ("BC", "British Columbia"),
    ("MB", "Manitoba"),
    ("NB", "New Brunswick"),
    ("NL", "Newfoundland and Labrador"),
    ("NT", "Northwest Territories"),
    ("NS", "Nova Scotia"),
    ("NU", "Nunavut"),
    ("ON", "Ontario"),
    ("PE", "Prince Edward Island"),
    ("QC", "Quebec"),
    ("SK", "Saskatchewan"),
    ("YT", "Yukon"),
];

/// United States subdivision (state/territory) catalog.
const B2B_UNITED_STATES_ZONES: &[(&str, &str)] = &[
    ("AL", "Alabama"),
    ("AK", "Alaska"),
    ("AZ", "Arizona"),
    ("AR", "Arkansas"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("DC", "District of Columbia"),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("IA", "Iowa"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("ME", "Maine"),
    ("MD", "Maryland"),
    ("MA", "Massachusetts"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MS", "Mississippi"),
    ("MO", "Missouri"),
    ("MT", "Montana"),
    ("NE", "Nebraska"),
    ("NV", "Nevada"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NY", "New York"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VT", "Vermont"),
    ("VA", "Virginia"),
    ("WA", "Washington"),
    ("WV", "West Virginia"),
    ("WI", "Wisconsin"),
    ("WY", "Wyoming"),
];

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
            errors.push(b2b_company_user_error(
                field,
                "Email is invalid",
                "INVALID",
                None,
            ));
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

/// Validates a `companyContactCreate` input: a title carrying HTML, a name that
/// exceeds Shopify's 255-character limit, or an invalid email each produces a
/// user error anchored at its own `input.<field>` path.
fn b2b_contact_create_input_errors(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();
    let field_path = |name: &str| -> Vec<String> {
        prefix
            .iter()
            .map(|part| part.to_string())
            .chain(std::iter::once(name.to_string()))
            .collect()
    };
    if let Some(title) = resolved_string_field(input, "title") {
        if b2b_contains_html_tags(&title) {
            errors.push(json!({
                "field": field_path("title"),
                "message": "Title contains HTML tags",
                "code": "CONTAINS_HTML_TAGS"
            }));
        }
    }
    for (name_field, label) in [("firstName", "First name"), ("lastName", "Last name")] {
        if let Some(value) = resolved_string_field(input, name_field) {
            if value.chars().count() > 255 {
                errors.push(json!({
                    "field": field_path(name_field),
                    "message": format!("{label} is too long"),
                    "code": "TOO_LONG"
                }));
            }
        }
    }
    if let Some(email) = resolved_string_field(input, "email") {
        if !is_valid_customer_email(&email) {
            errors.push(json!({
                "field": field_path("email"),
                "message": "Email is invalid",
                "code": "INVALID"
            }));
        }
    }
    errors
}

/// True when a staged order/draft-order record references the given company via
/// its purchasing entity (directly, or through a draft order's nested completed
/// order) — i.e. the company is still in use and cannot be deleted.
fn b2b_record_references_company(record: &Value, company_id: &str) -> bool {
    if let Some(entity) = record.get("purchasingEntity") {
        if b2b_value_contains_company_id(entity, company_id) {
            return true;
        }
    }
    if let Some(entity) = record.get("__draftProxyPurchasingEntity") {
        if b2b_value_contains_company_id(entity, company_id) {
            return true;
        }
    }
    if let Some(order) = record.get("order") {
        if order
            .get("purchasingEntity")
            .is_some_and(|entity| b2b_value_contains_company_id(entity, company_id))
        {
            return true;
        }
    }
    false
}

/// True when a staged order/draft-order record references the given company
/// location through its purchasing entity.
fn b2b_record_references_company_location(record: &Value, location_id: &str) -> bool {
    if let Some(entity) = record.get("purchasingEntity") {
        if b2b_value_contains_company_location_id(entity, location_id) {
            return true;
        }
    }
    if let Some(entity) = record.get("__draftProxyPurchasingEntity") {
        if b2b_value_contains_company_location_id(entity, location_id) {
            return true;
        }
    }
    if let Some(order) = record.get("order") {
        if order
            .get("purchasingEntity")
            .is_some_and(|entity| b2b_value_contains_company_location_id(entity, location_id))
        {
            return true;
        }
    }
    false
}

/// Recursively searches a value for a reference to a company id, matching a
/// `companyId` field, a nested `company.id`, or the bare id as a string.
fn b2b_value_contains_company_id(value: &Value, company_id: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map.get("companyId").and_then(Value::as_str) == Some(company_id) {
                return true;
            }
            if map
                .get("company")
                .and_then(|company| company.get("id"))
                .and_then(Value::as_str)
                == Some(company_id)
            {
                return true;
            }
            map.values()
                .any(|value| b2b_value_contains_company_id(value, company_id))
        }
        Value::Array(items) => items
            .iter()
            .any(|item| b2b_value_contains_company_id(item, company_id)),
        Value::String(string) => string == company_id,
        _ => false,
    }
}

/// Recursively searches a purchasing entity for a company-location reference,
/// covering both public input (`companyLocationId`) and staged read shapes
/// (`location.id` / `companyLocation.id`).
fn b2b_value_contains_company_location_id(value: &Value, location_id: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map.get("companyLocationId").and_then(Value::as_str) == Some(location_id) {
                return true;
            }
            if map
                .get("location")
                .and_then(|location| location.get("id"))
                .and_then(Value::as_str)
                == Some(location_id)
            {
                return true;
            }
            if map
                .get("companyLocation")
                .and_then(|location| location.get("id"))
                .and_then(Value::as_str)
                == Some(location_id)
            {
                return true;
            }
            map.values()
                .any(|value| b2b_value_contains_company_location_id(value, location_id))
        }
        Value::Array(items) => items
            .iter()
            .any(|item| b2b_value_contains_company_location_id(item, location_id)),
        Value::String(string) => string == location_id,
        _ => false,
    }
}

/// True when a company location carries a positive store-credit balance on any
/// embedded account nodes.
fn b2b_location_has_embedded_store_credit_balance(location: &Value) -> bool {
    location["storeCreditAccounts"]["nodes"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().any(store_credit_account_has_positive_balance))
}

fn store_credit_account_has_positive_balance(account: &Value) -> bool {
    account["balance"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .is_some_and(|amount| amount > 0.0)
}

fn b2b_location_company_id(location: &Value) -> Option<&str> {
    location
        .get("companyId")
        .and_then(Value::as_str)
        .or_else(|| location["company"]["id"].as_str())
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
/// Given an authoritative upstream delete response and the request-ordered ids that
/// were submitted, returns the subset the response reports as actually deleted — the
/// request indices that carry no `userErrors` entry. Bulk deletes report per-index
/// failures via a numeric tail on the error `field` (e.g. `["companyIds", "2"]`), so a
/// partially-rejected bulk delete (some blocked by deletable checks, some succeeding)
/// only removes the indices that survived. Single-id deletes have no positional index
/// and are treated as all-or-nothing.
fn b2b_passthrough_deleted_request_ids(
    response: &Response,
    requested_ids: &[String],
) -> Vec<String> {
    if response.status >= 400 {
        return Vec::new();
    }
    let Some(data) = response.body.get("data").filter(|data| !data.is_null()) else {
        return Vec::new();
    };
    let mut failed_indices = std::collections::HashSet::new();
    let mut unindexed_error = false;
    if let Some(payloads) = data.as_object() {
        for payload in payloads.values() {
            let Some(errors) = payload.get("userErrors").and_then(Value::as_array) else {
                continue;
            };
            for error in errors {
                match error
                    .get("field")
                    .and_then(Value::as_array)
                    .and_then(|field| field.last())
                    .and_then(Value::as_str)
                    .and_then(|last| last.parse::<usize>().ok())
                {
                    Some(index) => {
                        failed_indices.insert(index);
                    }
                    None => unindexed_error = true,
                }
            }
        }
    }
    if requested_ids.len() <= 1 {
        return if failed_indices.is_empty() && !unindexed_error {
            requested_ids.to_vec()
        } else {
            Vec::new()
        };
    }
    requested_ids
        .iter()
        .enumerate()
        .filter(|(index, _)| !failed_indices.contains(index))
        .map(|(_, id)| id.clone())
        .collect()
}

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
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| ("customerMerge".to_string(), Vec::new(), BTreeMap::new()));
        let one_id = resolved_string_field(&arguments, "customerOneId")
            .or_else(|| resolved_string_field(variables, "customerOneId"))
            .unwrap_or_default();
        let two_id = resolved_string_field(&arguments, "customerTwoId")
            .or_else(|| resolved_string_field(variables, "customerTwoId"))
            .unwrap_or_default();

        // Compute the payload generically from staged state. State only mutates on
        // the success branch; each early return mirrors a live customerMerge
        // userError branch (self-merge, unknown customer, merge blockers).
        let (payload, staged_ids) = self.customer_merge_payload(&arguments, &one_id, &two_id);
        self.record_mutation_log_entry(request, query, variables, "customerMerge", staged_ids);
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    /// Stage a `customerRequestDataErasure` / `customerCancelDataErasure`
    /// privacy side effect locally. `request_erasure == true` is the request
    /// root; `false` is the cancel root. Records the raw mutation in the log
    /// (status `staged` on success, `failed` on userError) and never forwards
    /// upstream. Returns `{ <responseKey>: { customerId, userErrors } }`.
    pub(in crate::proxy) fn customer_data_erasure(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_field: &str,
        request_erasure: bool,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new(), BTreeMap::new()));
        let customer_id = resolved_string_field(&arguments, "customerId")
            .or_else(|| resolved_string_field(variables, "customerId"))
            .unwrap_or_default();

        let (payload, status, staged_ids) =
            self.customer_data_erasure_payload(&customer_id, request_erasure);
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_data_erasure_payload(
        &mut self,
        customer_id: &str,
        request_erasure: bool,
    ) -> (Value, &'static str, Vec<String>) {
        if !self.customer_exists(customer_id) {
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
                customer_id.to_string(),
                json!({ "customerId": customer_id, "status": "REQUESTED" }),
            );
            return (
                customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
                "staged",
                vec![customer_id.to_string()],
            );
        }
        let is_pending = self
            .store
            .staged
            .customer_data_erasure_requests
            .get(customer_id)
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
            customer_id.to_string(),
            json!({ "customerId": customer_id, "status": "CANCELED" }),
        );
        (
            customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
            "staged",
            vec![customer_id.to_string()],
        )
    }

    fn customer_merge_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        one_id: &str,
        two_id: &str,
    ) -> (Value, Vec<String>) {
        if one_id.is_empty() || two_id.is_empty() {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Both customerOneId and customerTwoId are required",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                Vec::new(),
            );
        }
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
                Vec::new(),
            );
        }
        // Shopify validates customerOneId then customerTwoId.
        if let Some(error) = self.customer_merge_unknown_error(one_id, "customerOneId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        if let Some(error) = self.customer_merge_unknown_error(two_id, "customerTwoId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        let blockers = self.customer_merge_blocker_errors(one_id, two_id);
        if !blockers.is_empty() {
            return (
                customer_merge_payload_json(None, None, blockers),
                Vec::new(),
            );
        }

        let override_fields =
            resolved_object_field(arguments, "overrideFields").unwrap_or_default();
        let one = self
            .store
            .staged
            .customers
            .get(one_id)
            .cloned()
            .unwrap_or(Value::Null);
        let two = self
            .store
            .staged
            .customers
            .get(two_id)
            .cloned()
            .unwrap_or(Value::Null);
        let (result_id, source_id) =
            customer_merge_result_source_ids(one_id, &one, two_id, &two, &override_fields);
        let mut result = if result_id == one_id {
            one.clone()
        } else {
            two.clone()
        };
        let source = if source_id == one_id { one } else { two };
        apply_customer_merge_overrides(&mut result, &source, &override_fields);
        merge_customer_attached_resources(&mut result, &source);
        normalize_merged_customer_defaults(&mut result);
        // The resulting customer inherits the earliest creation date of the two
        // merged customers (it represents the older identity). ISO-8601 timestamps
        // order lexicographically, so the string min is the earlier instant.
        if let Some(source_created) = source["createdAt"].as_str() {
            let earliest = match result["createdAt"].as_str() {
                Some(result_created) => source_created.min(result_created),
                None => source_created,
            }
            .to_string();
            result["createdAt"] = json!(earliest);
        }
        result["updatedAt"] = json!(self.next_product_timestamp());

        // The resulting customer's final email (post-override) is stamped onto every
        // order transferred from the merged-away source, mirroring Shopify reparenting
        // the source's orders under the resulting customer's identity.
        let result_email = result["email"].as_str().map(str::to_string);

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
        if let Some(mut source_orders) = self.store.staged.customer_orders.remove(&source_id) {
            if let Some(email) = &result_email {
                for order in &mut source_orders {
                    if order.get("email").is_some() {
                        order["email"] = json!(email);
                    }
                }
            }
            self.store
                .staged
                .customer_orders
                .entry(result_id.clone())
                .or_default()
                .extend(source_orders);
        }

        let job_id = self.next_proxy_synthetic_gid("Job");
        let merge_request = customer_merge_request_json(&job_id, &result_id, Vec::new());
        self.store
            .staged
            .customer_merge_requests
            .insert(job_id.clone(), merge_request);
        (
            customer_merge_payload_json(Some(&result_id), Some(&job_id), Vec::new()),
            vec![source_id, result_id, job_id],
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
                    &format!("{name} has gift cards and can\u{2019}t be merged."),
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
}

fn customer_merge_payload_json(
    resulting_customer_id: Option<&str>,
    job_id: Option<&str>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "resultingCustomerId": resulting_customer_id.map(Value::from).unwrap_or(Value::Null),
        "job": job_id
            .map(|id| json!({ "__typename": "Job", "id": id, "done": false, "query": Value::Null }))
            .unwrap_or(Value::Null),
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
        "field": field.clone(),
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
    user_error(["customerId"], message, Some(code))
}

fn customer_tags(customer: &Value) -> Vec<String> {
    customer["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str().map(str::to_string))
        .collect()
}

fn customer_merge_result_source_ids(
    one_id: &str,
    one: &Value,
    two_id: &str,
    two: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) -> (String, String) {
    if let Some(email_customer_id) =
        resolved_string_field(override_fields, "customerIdOfEmailToKeep")
    {
        if email_customer_id == one_id {
            return (one_id.to_string(), two_id.to_string());
        }
        if email_customer_id == two_id {
            return (two_id.to_string(), one_id.to_string());
        }
    }

    let one_has_email = customer_merge_has_email(one);
    let two_has_email = customer_merge_has_email(two);
    match (one_has_email, two_has_email) {
        (true, false) => return (one_id.to_string(), two_id.to_string()),
        (false, true) => return (two_id.to_string(), one_id.to_string()),
        (false, false) => return (two_id.to_string(), one_id.to_string()),
        (true, true) => {}
    }

    let one_consent = customer_merge_email_consent_priority(one);
    let two_consent = customer_merge_email_consent_priority(two);
    match one_consent.cmp(&two_consent) {
        std::cmp::Ordering::Greater => return (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less => return (two_id.to_string(), one_id.to_string()),
        std::cmp::Ordering::Equal => {}
    }

    let one_state = customer_merge_account_state_priority(one);
    let two_state = customer_merge_account_state_priority(two);
    match one_state.cmp(&two_state) {
        std::cmp::Ordering::Greater => (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
            (two_id.to_string(), one_id.to_string())
        }
    }
}

fn customer_merge_has_email(customer: &Value) -> bool {
    customer
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/defaultEmailAddress/emailAddress")
                .and_then(Value::as_str)
        })
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_merge_email_consent_priority(customer: &Value) -> u8 {
    let state = customer
        .pointer("/defaultEmailAddress/marketingState")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/emailMarketingConsent/marketingState")
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("SUBSCRIBED") {
        2
    } else if state.eq_ignore_ascii_case("PENDING") {
        1
    } else {
        0
    }
}

fn customer_merge_account_state_priority(customer: &Value) -> u8 {
    let state = customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("ENABLED") {
        2
    } else if state.eq_ignore_ascii_case("INVITED") {
        1
    } else {
        0
    }
}

/// Apply `customerMerge` override selections onto the resulting customer record.
/// `customerIdOf<Field>ToKeep` picks the source/result value for that field; note
/// and tags follow the captured precedence (explicit override, else union); the
/// display name and default contact projections are rebuilt from the resolved
/// scalar fields so downstream reads observe a consistent merged identity.
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
    if let Some(ResolvedValue::List(tags)) = override_fields.get("tags") {
        let mut tags = tags
            .iter()
            .filter_map(|tag| match tag {
                ResolvedValue::String(tag) => Some(tag.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
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

/// Merge the source customer's inline attached resources (addresses / metafields)
/// into the resulting customer. Addresses concatenate source-first then result;
/// metafields union by `namespace`+`key` with the resulting customer winning
/// conflicts. No-op when the source carries no such resources.
fn merge_customer_attached_resources(result: &mut Value, source: &Value) {
    let source_addresses = connection_nodes(&source["addressesV2"]);
    if !source_addresses.is_empty() {
        let mut nodes = source_addresses;
        nodes.extend(connection_nodes(&result["addressesV2"]));
        result["addressesV2"] = nodes_connection(nodes);
        if result["defaultAddress"].is_null() && !source["defaultAddress"].is_null() {
            result["defaultAddress"] = source["defaultAddress"].clone();
        }
    }
    let source_metafields = connection_nodes(&source["metafields"]);
    if !source_metafields.is_empty() {
        let existing_keys = connection_nodes(&result["metafields"])
            .iter()
            .map(metafield_identity)
            .collect::<BTreeSet<_>>();
        let mut nodes = connection_nodes(&result["metafields"]);
        for node in source_metafields {
            if !existing_keys.contains(&metafield_identity(&node)) {
                nodes.push(node);
            }
        }
        result["metafields"] = nodes_connection(nodes);
    }
}

fn connection_has_nodes(connection: &Value) -> bool {
    connection
        .get("nodes")
        .and_then(Value::as_array)
        .map(|nodes| !nodes.is_empty())
        .unwrap_or(false)
}

fn metafield_identity(node: &Value) -> String {
    format!(
        "{}:{}",
        node["namespace"].as_str().unwrap_or_default(),
        node["key"].as_str().unwrap_or_default()
    )
}

fn nodes_connection(nodes: Vec<Value>) -> Value {
    // A non-empty connection reports opaque (non-null) boundary cursors; Shopify's
    // are base64 blobs the local engine can't reconstruct, but downstream parity
    // matchers treat connection cursors as opaque (`any-string`), so a deterministic
    // per-node string (the node id) is a faithful stand-in. An empty connection
    // reports null boundary cursors, matching Shopify.
    let start_cursor = nodes.first().map(node_connection_cursor);
    let end_cursor = nodes.last().map(node_connection_cursor);
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": start_cursor,
            "endCursor": end_cursor
        }
    })
}

fn node_connection_cursor(node: &Value) -> String {
    node.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Cursor for an order node within a customer's `orders` connection. Prefers a
/// seeded opaque `__cursor` (the live Shopify connection cursor a scenario captured
/// and re-seeded, which downstream reads compare verbatim) and otherwise falls back
/// to the order id.
fn order_connection_cursor(record: &Value) -> String {
    record
        .get("__cursor")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value_id_cursor(record))
}

/// Evaluate a (small subset of a) customer search `query` against a staged customer.
/// Supports `tag:<value>` exact tag membership and a generic case-insensitive
/// substring fallback over email / display name / first name. An absent or blank
/// query matches every customer.
fn customer_matches_query(customer: &Value, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if let Some(tag) = query.strip_prefix("tag:") {
        let tag = tag.trim().trim_matches('\'').trim_matches('"');
        return customer["tags"]
            .as_array()
            .map(|tags| tags.iter().any(|value| value.as_str() == Some(tag)))
            .unwrap_or(false);
    }
    let needle = query.to_lowercase();
    let haystack = format!(
        "{} {} {}",
        customer["email"].as_str().unwrap_or_default(),
        customer["displayName"].as_str().unwrap_or_default(),
        customer["firstName"].as_str().unwrap_or_default()
    )
    .to_lowercase();
    haystack.contains(&needle)
}

/// Surface Shopify's order-summary defaults on a freshly staged customer record:
/// `numberOfOrders` is the string `"0"`, `lastOrder` is explicitly null, and
/// `orders` is an empty connection (with the `pageInfo` shape a `first:`/`last:`
/// page selection expects). Only fills fields that are absent/null so a record
/// that already carries real order state (e.g. a seeded customer) is untouched.
fn apply_customer_order_summary_defaults(customer: &mut Value) {
    if customer.get("numberOfOrders").is_none_or(Value::is_null) {
        customer["numberOfOrders"] = json!("0");
    }
    if customer.get("lastOrder").is_none() {
        customer["lastOrder"] = Value::Null;
    }
    if customer.get("orders").is_none_or(Value::is_null) {
        customer["orders"] = empty_orders_connection();
    }
}

/// An empty `Customer.orders` connection page: no nodes/edges and null boundary
/// cursors, matching how Shopify renders the summary connection for a customer
/// with zero orders.
fn empty_orders_connection() -> Value {
    json!({
        "nodes": [],
        "edges": [],
        "pageInfo": empty_page_info()
    })
}

/// Shopify rejects a credit/debit that would push an account past this hard cap.
const STORE_CREDIT_LIMIT: f64 = 100000.0;

fn store_credit_user_error(field: &[&str], message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

/// Read a money `amount` field from a resolved input map, accepting either the
/// canonical string form or a numeric literal (GraphQL `Decimal` is serialized
/// as a string but some callers send numbers).
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

/// Render an `f64` money amount the way Shopify serializes `MoneyV2.amount`:
/// whole values keep a single trailing zero (`"10.0"`), fractional values drop
/// trailing zeros beyond two decimal places (`"6.12"`, `"2.5"`).
fn shopify_decimal_text(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{rounded:.1}")
    } else {
        let text = format!("{rounded:.2}");
        text.trim_end_matches('0').to_string()
    }
}

fn store_credit_supported_currency(currency: &str) -> bool {
    matches!(
        currency,
        "USD" | "CAD" | "AUD" | "EUR" | "GBP" | "JPY" | "NZD"
    )
}

fn store_credit_expires_at_in_past(expires_at: &str) -> bool {
    !expires_at.is_empty() && expires_at < "2026-06-15T00:00:00Z"
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
}

fn empty_nodes_connection() -> Value {
    nodes_connection(Vec::new())
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

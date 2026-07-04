use crate::proxy::*;

enum B2bCompanyLocationDeleteBlocker {
    OnlyLocation,
    Order,
    DraftOrder,
    StoreCredit,
}

enum B2bLocationNameFallback {
    CompanyName,
    ShippingAddressThenCompanyName,
}

type B2bCompanyPayloadHandler =
    fn(&mut DraftProxy, &RootFieldSelection) -> (Value, &'static str, Vec<String>);
type B2bPassthroughCascadeArgs<'a> = (
    &'a Request,
    &'a str,
    &'a BTreeMap<String, ResolvedValue>,
    OperationType,
    &'a [String],
    &'a str,
);

const B2B_BULK_ACTIONS_MAX_SIZE: usize = 50;
const B2B_BULK_ACTION_LIMIT_REACHED_MESSAGE: &str =
    "Exceeded max input size of 50. Consider using BulkOperation.";

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

fn b2b_company_mutation_handler(name: &str) -> Option<B2bCompanyPayloadHandler> {
    Some(match name {
        "companyCreate" => DraftProxy::b2b_company_create_payload,
        "companyUpdate" => DraftProxy::b2b_company_update_payload,
        "companyDelete" => DraftProxy::b2b_company_delete_payload,
        "companiesDelete" => DraftProxy::b2b_companies_delete_payload,
        "companyContactCreate" => DraftProxy::b2b_company_contact_create_payload,
        "companyContactUpdate" => DraftProxy::b2b_company_contact_update_payload,
        "companyContactDelete" => DraftProxy::b2b_company_contact_delete_payload,
        "companyContactsDelete" => DraftProxy::b2b_company_contacts_delete_payload,
        "companyContactRemoveFromCompany" => {
            DraftProxy::b2b_company_contact_remove_from_company_payload
        }
        "companyAssignMainContact" => DraftProxy::b2b_company_assign_main_contact_payload,
        "companyRevokeMainContact" => DraftProxy::b2b_company_revoke_main_contact_payload,
        "companyContactAssignRole" => DraftProxy::b2b_company_contact_assign_role_payload,
        "companyContactAssignRoles" => DraftProxy::b2b_company_contact_assign_roles_payload,
        "companyContactRevokeRole" => DraftProxy::b2b_company_contact_revoke_role_payload,
        "companyContactRevokeRoles" => DraftProxy::b2b_company_contact_revoke_roles_payload,
        "companyLocationCreate" => DraftProxy::b2b_company_location_create_payload,
        "companyLocationUpdate" => DraftProxy::b2b_company_location_update_payload,
        "companyLocationDelete" => DraftProxy::b2b_company_location_delete_payload,
        "companyLocationsDelete" => DraftProxy::b2b_company_locations_delete_payload,
        "companyLocationAssignAddress" => DraftProxy::b2b_company_location_assign_address_payload,
        "companyAddressDelete" => DraftProxy::b2b_company_address_delete_payload,
        "companyLocationAssignStaffMembers" => {
            DraftProxy::b2b_company_location_assign_staff_members_payload
        }
        "companyLocationRemoveStaffMembers" => {
            DraftProxy::b2b_company_location_remove_staff_members_payload
        }
        "companyLocationAssignRoles" => DraftProxy::b2b_company_location_assign_roles_payload,
        "companyLocationRevokeRoles" => DraftProxy::b2b_company_location_revoke_roles_payload,
        _ => return None,
    })
}

fn b2b_bulk_status<T>(staged_items: &[T], user_errors: &[Value]) -> &'static str {
    if staged_items.is_empty() && !user_errors.is_empty() {
        "failed"
    } else {
        "staged"
    }
}

fn b2b_null_when_failed(status: &str, value: Value) -> Value {
    if status == "failed" {
        Value::Null
    } else {
        value
    }
}

impl DraftProxy {
    fn b2b_passthrough_cascade<Extracted, Extract, Cascade>(
        &mut self,
        args: B2bPassthroughCascadeArgs<'_>,
        extract: Extract,
        cascade: Cascade,
    ) -> Response
    where
        Extract: FnOnce(&[RootFieldSelection]) -> Extracted,
        Cascade: FnOnce(&mut Self, Extracted, &Response),
    {
        let (request, query, variables, operation_type, parsed_root_fields, root_field) = args;
        let extracted = root_fields(query, variables).map(|fields| extract(&fields));
        let response = self.dispatch_unknown_passthrough_or_legacy_error(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
            root_field,
        );
        if let Some(extracted) = extracted {
            cascade(self, extracted, &response);
        }
        response
    }

    fn b2b_passthrough_with_success_cascade<Extracted, Extract, Cascade>(
        &mut self,
        args: B2bPassthroughCascadeArgs<'_>,
        extract: Extract,
        cascade: Cascade,
    ) -> Response
    where
        Extract: FnOnce(&[RootFieldSelection]) -> Extracted,
        Cascade: FnOnce(&mut Self, Extracted, &Response),
    {
        self.b2b_passthrough_cascade(args, extract, |proxy, extracted, response| {
            if b2b_passthrough_mutation_succeeded(response) {
                cascade(proxy, extracted, response);
            }
        })
    }

    fn b2b_passthrough_with_deleted_cascade<Extract, Cascade>(
        &mut self,
        args: B2bPassthroughCascadeArgs<'_>,
        extract: Extract,
        mut cascade: Cascade,
    ) -> Response
    where
        Extract: FnOnce(&[RootFieldSelection]) -> Vec<String>,
        Cascade: FnMut(&mut Self, &str),
    {
        self.b2b_passthrough_cascade(args, extract, |proxy, request_ids, response| {
            for deleted_id in b2b_passthrough_deleted_request_ids(response, &request_ids) {
                cascade(proxy, &deleted_id);
            }
        })
    }

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
        let mut declined = false;
        let data = root_payload_json(&fields, |field| {
            if declined {
                return None;
            }
            if field.name != "companyLocationTaxSettingsUpdate" {
                declined = true;
                return None;
            }
            let (payload, status, staged_ids) = self.b2b_tax_settings_update_payload(field);
            self.record_mutation_log_with_status(
                request,
                query,
                variables,
                "companyLocationTaxSettingsUpdate",
                staged_ids,
                status,
            );
            Some(selected_json(&payload, &field.selection))
        });
        if declined {
            return None;
        }
        Some(ok_json(json!({ "data": data })))
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
                let data = root_payload_json(&fields, |field| {
                    let (payload, status, staged_ids) =
                        self.b2b_company_location_update_payload(field);
                    self.record_mutation_log_with_status(
                        request,
                        query,
                        variables,
                        &field.name,
                        staged_ids,
                        status,
                    );
                    Some(selected_json(&payload, &field.selection))
                });
                Some(ok_json(json!({ "data": data })))
            }
            OperationType::Query
                if parsed_root_fields
                    .iter()
                    .all(|field| field == "companyLocation") =>
            {
                let data = root_payload_json(&fields, |field| {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                    Some(location)
                });
                Some(ok_json(json!({ "data": data })))
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
            OperationType::Mutation => fields
                .iter()
                .all(|field| b2b_company_mutation_handler(&field.name).is_some()),
            OperationType::Query => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "company"
                        | "companies"
                        | "companiesCount"
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
                let mut declined = false;
                let data = root_payload_json(&fields, |field| {
                    if declined {
                        return None;
                    }
                    let Some(handler) = b2b_company_mutation_handler(&field.name) else {
                        declined = true;
                        return None;
                    };
                    let (payload, status, staged_ids) = handler(self, field);
                    self.record_mutation_log_with_status(
                        request,
                        query,
                        variables,
                        &field.name,
                        staged_ids,
                        status,
                    );
                    Some(self.b2b_payload_selected_json(&payload, &field.selection))
                });
                if declined {
                    return None;
                }
                Some(ok_json(json!({ "data": data })))
            }
            OperationType::Query => {
                let mut declined = false;
                let data = root_payload_json(&fields, |field| {
                    if declined {
                        return None;
                    }
                    let value = match field.name.as_str() {
                        "company" => {
                            let id =
                                resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                                resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                                resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                        "companyLocations" => self.b2b_company_locations_connection(field),
                        "companies" => self.b2b_companies_connection(field),
                        "companiesCount" => self.b2b_companies_count(field),
                        _ => {
                            declined = true;
                            return None;
                        }
                    };
                    Some(value)
                });
                if declined {
                    return None;
                }
                Some(ok_json(json!({ "data": data })))
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
    /// defer to other handlers that may own non-B2B company fixtures.
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
        let company_id = resolved_string_field(&field.arguments, "companyId")?;
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return None;
        }
        let (payload, status, staged_ids) =
            self.b2b_company_assign_customer_as_contact_payload(field);
        if status == "staged" {
            if let Some(customer_id) = resolved_string_field(&field.arguments, "customerId") {
                self.store
                    .staged
                    .order_customer_contact_customer_ids
                    .insert(customer_id);
            }
        }
        self.record_mutation_log_with_status(
            request,
            query,
            variables,
            &field.name,
            staged_ids,
            status,
        );
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
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
                "note": Value::Null,
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
        let (location, location_staged_ids) = self.b2b_build_company_location(
            &id,
            &company,
            &location_input,
            B2bLocationNameFallback::CompanyName,
        );
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
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

        let errors = b2b_location_input_errors(&input, &["input"]);
        if !errors.is_empty() {
            return (
                b2b_company_location_payload(None, errors),
                "failed",
                Vec::new(),
            );
        }
        if !b2b_location_create_has_meaningful_non_address_input(&input) {
            return (
                b2b_company_location_payload(
                    None,
                    vec![user_error(
                        Value::Null,
                        "Company location create input is empty.",
                        Some("NO_INPUT"),
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }

        // externalId length/charset/uniqueness is validated against every staged
        // location, so it lives here (with store access) rather than in the
        // input-only helper.
        if let Some(external_id) = resolved_string_field(&input, "externalId") {
            let external_id_errors = b2b_external_id_errors(
                &external_id,
                vec!["input", "externalId"],
                &self.store.staged.b2b_locations.records,
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

        let (location, staged_ids) = self.b2b_build_company_location(
            &company_id,
            &company,
            &input,
            B2bLocationNameFallback::ShippingAddressThenCompanyName,
        );
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
        let location_id = resolved_string_field(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
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
                    vec![user_error(
                        Value::Null,
                        "Company location update input is empty.",
                        Some("NO_INPUT"),
                    )],
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
                        BLANK_USER_ERROR_CODE,
                        None,
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }

        if let Some(external_id) = resolved_string_field(&input, "externalId") {
            let errors = b2b_external_id_errors(
                &external_id,
                vec!["input", "externalId"],
                &self.store.staged.b2b_locations.records,
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
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
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
                    vec![user_error(
                        Value::Null,
                        "Company contact update input is empty.",
                        Some("NO_INPUT"),
                    )],
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![b2b_not_found(["companyId"], "Company does not exist.")],
                ),
                "failed",
                Vec::new(),
            );
        }
        if input.is_empty() {
            return (
                b2b_company_contact_payload(
                    None,
                    vec![user_error(
                        Value::Null,
                        "Company contact create input is empty.",
                        Some("NO_INPUT"),
                    )],
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
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "deletedCompanyContactId": Value::Null,
                    "userErrors": [b2b_not_found(["companyContactId"], "The company contact doesn't exist.")]
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
        let contact_ids = list_string_field(&field.arguments, "companyContactIds");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            contact_ids.len(),
            "companyContactIds",
            &["deletedCompanyContactIds"],
        ) {
            return payload;
        }
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
        let status = b2b_bulk_status(&deleted_ids, &user_errors);
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
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "removedCompanyContactId": Value::Null,
                    "userErrors": [b2b_not_found(["companyContactId"], "The company contact doesn't exist.")]
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
        let contact_id =
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        let Some(company) = self.store.staged.b2b_companies.get(&company_id).cloned() else {
            return (
                b2b_company_payload(None, vec![b2b_resource_not_found(["companyId"])]),
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
                        vec![user_error(
                            ["companyContactId"],
                            "The company contact does not belong to the company.",
                            Some("INVALID_INPUT"),
                        )],
                    ),
                    "failed",
                    Vec::new(),
                );
            }
            return (
                b2b_company_payload(
                    None,
                    vec![b2b_not_found(
                        ["companyContactId"],
                        "The company contact doesn't exist.",
                    )],
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
        let company_id = resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                b2b_company_payload(None, vec![b2b_resource_not_found(["companyId"])]),
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
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        let role_id =
            resolved_string_field(&field.arguments, "companyContactRoleId").unwrap_or_default();
        let location_id =
            resolved_string_field(&field.arguments, "companyLocationId").unwrap_or_default();
        let Some(contact) = self.store.staged.b2b_contacts.get(&contact_id) else {
            return (
                json!({
                    "companyContactRoleAssignment": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyContactId"])]
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
                    "userErrors": [b2b_not_found(["companyLocationId"], "The company location doesn't exist.")]
                }),
                "failed",
                Vec::new(),
            );
        }
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
                    "userErrors": [b2b_not_found(["companyContactRoleId"], "The company contact role doesn't exist.")]
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
                    "userErrors": [user_error(Value::Null, "Company contact has already been assigned a role in that company location.", Some("LIMIT_REACHED"))]
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
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        let roles_to_assign = resolved_object_list_field(&field.arguments, "rolesToAssign");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            roles_to_assign.len(),
            "rolesToAssign",
            &["roleAssignments"],
        ) {
            return payload;
        }
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "roleAssignments": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyContactId"])]
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
                user_errors.push(b2b_resource_not_found(json!([
                    "rolesToAssign",
                    index.to_string(),
                    "companyLocationId"
                ])));
                continue;
            }
            if !self.store.staged.b2b_contact_roles.contains_key(&role_id) {
                user_errors.push(b2b_resource_not_found(json!([
                    "rolesToAssign",
                    index.to_string(),
                    "companyContactRoleId"
                ])));
                continue;
            }
            if self.b2b_contact_has_assignment_at_location(&contact_id, &location_id) {
                user_errors.push(b2b_bulk_role_already_assigned_error(index));
                continue;
            }
            assignments.push(self.b2b_stage_role_assignment(&location_id, &contact_id, &role_id));
        }
        let status = b2b_bulk_status(&assignments, &user_errors);
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

    /// Revokes one contact role assignment by id, scoped to the supplied contact.
    pub(in crate::proxy) fn b2b_company_contact_revoke_role_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        let assignment_id =
            resolved_string_field(&field.arguments, "companyContactRoleAssignmentId")
                .unwrap_or_default();
        let Some(company_contact) = self.store.staged.b2b_contacts.get(&contact_id).cloned() else {
            return (
                json!({
                    "revokedCompanyContactRoleAssignmentId": Value::Null,
                    "companyContact": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyContactId"])]
                }),
                "failed",
                Vec::new(),
            );
        };
        let assignment_matches_contact = self
            .store
            .staged
            .b2b_role_assignments
            .get(&assignment_id)
            .and_then(|assignment| assignment["companyContactId"].as_str())
            == Some(contact_id.as_str());

        if !assignment_matches_contact {
            return (
                json!({
                    "revokedCompanyContactRoleAssignmentId": Value::Null,
                    "companyContact": Value::Null,
                    "userErrors": [b2b_not_found(["companyContactRoleAssignmentId"], "The role assignment doesn't exist.")]
                }),
                "failed",
                Vec::new(),
            );
        }

        let _ = self.b2b_remove_role_assignment(&assignment_id);
        (
            json!({
                "revokedCompanyContactRoleAssignmentId": assignment_id,
                "companyContact": company_contact,
                "userErrors": []
            }),
            "staged",
            vec![assignment_id],
        )
    }

    /// Revokes contact role assignments by id, validating the parent contact
    /// first and reporting a per-index RESOURCE_NOT_FOUND for unknown or
    /// differently-scoped assignment ids.
    pub(in crate::proxy) fn b2b_company_contact_revoke_roles_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, &'static str, Vec<String>) {
        let contact_id =
            resolved_string_field(&field.arguments, "companyContactId").unwrap_or_default();
        let assignment_ids = list_string_field(&field.arguments, "roleAssignmentIds");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            assignment_ids.len(),
            "roleAssignmentIds",
            &["revokedRoleAssignmentIds"],
        ) {
            return payload;
        }
        let revoke_all = resolved_bool_field(&field.arguments, "revokeAll").unwrap_or(false);
        if assignment_ids.is_empty() && !revoke_all {
            return (
                json!({
                    "revokedRoleAssignmentIds": Value::Null,
                    "userErrors": [user_error(Value::Null, "Invalid input.", Some("INVALID_INPUT"))]
                }),
                "failed",
                Vec::new(),
            );
        }
        if !self.store.staged.b2b_contacts.contains_key(&contact_id) {
            return (
                json!({
                    "revokedRoleAssignmentIds": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyContactId"])]
                }),
                "failed",
                Vec::new(),
            );
        }
        let ids_to_revoke = if revoke_all {
            self.store
                .staged
                .b2b_role_assignments
                .iter()
                .filter(|(_, assignment)| {
                    assignment["companyContactId"].as_str() == Some(contact_id.as_str())
                })
                .map(|(assignment_id, _)| assignment_id.clone())
                .collect::<Vec<String>>()
        } else {
            assignment_ids.clone()
        };
        let mut revoked_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, assignment_id) in ids_to_revoke.iter().enumerate() {
            let assignment_matches_contact = self
                .store
                .staged
                .b2b_role_assignments
                .get(assignment_id)
                .and_then(|assignment| assignment["companyContactId"].as_str())
                == Some(contact_id.as_str());
            if assignment_matches_contact {
                let _ = self.b2b_remove_role_assignment(assignment_id);
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
        let status = b2b_bulk_status(&revoked_ids, &user_errors);
        (
            json!({
                "revokedRoleAssignmentIds": b2b_null_when_failed(status, json!(revoked_ids)),
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
        let company_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.b2b_companies.contains_key(&company_id) {
            return (
                json!({
                    "deletedCompanyId": Value::Null,
                    "userErrors": [b2b_not_found(["id"], "Company does not exist.")]
                }),
                "failed",
                Vec::new(),
            );
        }
        if self.b2b_company_has_delete_blocker(&company_id) {
            return (
                json!({
                    "deletedCompanyId": Value::Null,
                    "userErrors": [user_error(["id"], "Failed to delete the company.", Some("FAILED_TO_DELETE"))]
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
        let company_ids = list_string_field(&field.arguments, "companyIds");
        if let Some(payload) =
            b2b_bulk_action_limit_payload(company_ids.len(), "companyIds", &["deletedCompanyIds"])
        {
            return payload;
        }
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
        let status = b2b_bulk_status(&deleted_ids, &user_errors);
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

    /// Removes a locally-staged company and all staged locations that point at it.
    /// Keep this separate from `b2b_delete_company`: the passthrough cascade trusts the
    /// removed company's explicit graph ids and also deletes contacts, while this local
    /// path orphan-scans locations by company reference. Merge only with parity evidence.
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
        let location_id = resolved_string_field(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .unwrap_or_default();
        if !self.store.staged.b2b_locations.contains_key(&location_id) {
            return (
                json!({
                    "deletedCompanyLocationId": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyLocationId"])]
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
                    "userErrors": [user_error(["companyLocationId"], "Failed to delete the company location.", Some("FAILED_TO_DELETE"))]
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
        let location_ids = list_string_field(&field.arguments, "companyLocationIds");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            location_ids.len(),
            "companyLocationIds",
            &["deletedCompanyLocationIds"],
        ) {
            return payload;
        }
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
        let status = b2b_bulk_status(&deleted_ids, &user_errors);
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
        let location_id = resolved_string_field(&field.arguments, "locationId")
            .or_else(|| resolved_string_field(&field.arguments, "companyLocationId"))
            .unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let address_types = list_string_field(&field.arguments, "addressTypes");
        if !b2b_unique_strings(&address_types) {
            return (
                json!({
                    "addresses": Value::Null,
                    "userErrors": [user_error(Value::Null, "Invalid input.", Some("INVALID_INPUT"))]
                }),
                "failed",
                Vec::new(),
            );
        }
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
            return (
                json!({
                    "addresses": Value::Null,
                    "userErrors": [b2b_resource_not_found(["locationId"])]
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
        let location_ids = self.store.staged.b2b_locations.order.clone();
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
        let address_id = resolved_string_field(&field.arguments, "addressId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .unwrap_or_default();
        let touched_location_ids = self.b2b_delete_company_address(&address_id);
        if touched_location_ids.is_empty() {
            return (
                json!({
                    "deletedAddressId": Value::Null,
                    "userErrors": [b2b_not_found(["addressId"], "Company address was not found.")]
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
        let location_id = resolved_string_field(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_field(&field.arguments, "locationId"))
            .unwrap_or_default();
        let staff_ids = list_string_field(&field.arguments, "staffMemberIds");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            staff_ids.len(),
            "staffMemberIds",
            &["companyLocationStaffMemberAssignments"],
        ) {
            return payload;
        }
        let Some(mut location) = self.store.staged.b2b_locations.get(&location_id).cloned() else {
            return (
                json!({
                    "companyLocationStaffMemberAssignments": Value::Null,
                    "userErrors": [b2b_resource_not_found(["companyLocationId"])]
                }),
                "failed",
                Vec::new(),
            );
        };

        let mut assignments = Vec::new();
        let mut user_errors = Vec::new();
        let mut seen_input = BTreeSet::new();
        for (index, staff_id) in staff_ids.iter().enumerate() {
            if !self.b2b_valid_staff_member_id(staff_id) {
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
        let status = b2b_bulk_status(&assignments, &user_errors);
        let staged_ids = assignments
            .iter()
            .filter_map(|assignment| assignment["id"].as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        (
            json!({
                "companyLocationStaffMemberAssignments": b2b_null_when_failed(status, Value::Array(assignments)),
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
        let assignment_ids =
            list_string_field(&field.arguments, "companyLocationStaffMemberAssignmentIds");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            assignment_ids.len(),
            "companyLocationStaffMemberAssignmentIds",
            &["deletedCompanyLocationStaffMemberAssignmentIds"],
        ) {
            return payload;
        }
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
        let status = b2b_bulk_status(&deleted_ids, &user_errors);
        (
            json!({
                "deletedCompanyLocationStaffMemberAssignmentIds": b2b_null_when_failed(status, json!(deleted_ids)),
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
        let location_id = resolved_string_field(&field.arguments, "companyLocationId")
            .or_else(|| resolved_string_field(&field.arguments, "locationId"))
            .unwrap_or_default();
        let roles_to_assign = resolved_object_list_field(&field.arguments, "rolesToAssign");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            roles_to_assign.len(),
            "rolesToAssign",
            &["roleAssignments"],
        ) {
            return payload;
        }
        if !self.store.staged.b2b_locations.contains_key(&location_id) {
            return (
                json!({
                    "roleAssignments": Value::Null,
                    "userErrors": [b2b_not_found(["companyLocationId"], "Location does not exist.")]
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
            if self.b2b_contact_has_assignment_at_location(&contact_id, &location_id) {
                user_errors.push(b2b_bulk_role_already_assigned_error(index));
                continue;
            }
            let assignment = self.b2b_stage_role_assignment(&location_id, &contact_id, &role_id);
            assignments.push(assignment);
        }
        let status = b2b_bulk_status(&assignments, &user_errors);
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
        let location_id =
            resolved_string_field(&field.arguments, "companyLocationId").unwrap_or_default();
        let assignment_ids = list_string_field(&field.arguments, "rolesToRevoke");
        if let Some(payload) = b2b_bulk_action_limit_payload(
            assignment_ids.len(),
            "rolesToRevoke",
            &[
                "revokedRoleAssignmentIds",
                "revokedCompanyContactRoleAssignmentIds",
            ],
        ) {
            return payload;
        }
        if !self.store.staged.b2b_locations.contains_key(&location_id) {
            return (
                json!({
                    "revokedRoleAssignmentIds": Value::Null,
                    "revokedCompanyContactRoleAssignmentIds": Value::Null,
                    "userErrors": [b2b_not_found(["companyLocationId"], "Location does not exist.")]
                }),
                "failed",
                Vec::new(),
            );
        }
        let mut revoked_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, assignment_id) in assignment_ids.iter().enumerate() {
            let assignment_matches_location = self
                .store
                .staged
                .b2b_role_assignments
                .get(assignment_id)
                .and_then(|assignment| assignment["companyLocationId"].as_str())
                == Some(location_id.as_str());
            if assignment_matches_location {
                let _ = self.b2b_remove_role_assignment(assignment_id);
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
        let status = b2b_bulk_status(&revoked_ids, &user_errors);
        (
            json!({
                "revokedRoleAssignmentIds": revoked_ids,
                "revokedCompanyContactRoleAssignmentIds": revoked_ids,
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
            "company" => resolved_string_field(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.b2b_companies.contains_key(&id)),
            "companyContact" => resolved_string_field(&field.arguments, "id").is_some_and(|id| {
                self.store.staged.b2b_contacts.contains_key(&id)
                    || self.store.staged.deleted_b2b_contact_ids.contains(&id)
            }),
            "companyLocation" => resolved_string_field(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.b2b_locations.contains_key(&id)),
            "companyLocations" => !self.store.staged.b2b_locations.is_empty(),
            // A companies(query:) connection can always be answered from locally
            // staged state — an empty result is the correct answer once the
            // matching companies have been deleted.
            "companies" => true,
            "companiesCount" => true,
            _ => false,
        })
    }

    fn b2b_selected_reference_json<Resolve, Render>(
        &self,
        source: &Value,
        id_field: &str,
        selection: &SelectedField,
        resolve: Resolve,
        render: Render,
    ) -> Value
    where
        Resolve: Fn(&Self, &str) -> Option<Value>,
        Render: Fn(&Self, &Value, &[SelectedField]) -> Value,
    {
        source[id_field]
            .as_str()
            .and_then(|id| resolve(self, id))
            .map(|value| render(self, &value, &selection.selection))
            .unwrap_or(Value::Null)
    }

    fn b2b_selected_id_connection_json<Resolve, Render>(
        &self,
        source: &Value,
        id_list_field: &str,
        selection: &SelectedField,
        resolve: Resolve,
        render: Render,
    ) -> Value
    where
        Resolve: Fn(&Self, &str) -> Option<Value>,
        Render: Fn(&Self, &Value, &[SelectedField]) -> Value,
    {
        let nodes = b2b_json_id_list(source, id_list_field)
            .into_iter()
            .filter_map(|id| resolve(self, &id))
            .collect::<Vec<_>>();
        selected_staged_connection_with_args(
            nodes,
            &selection.arguments,
            &selection.selection,
            b2b_nested_connection_search_decision,
            |node, sort_key| b2b_nested_connection_sort_key(id_list_field, node, sort_key),
            |node, fields| render(self, node, fields),
            value_id_cursor,
        )
    }

    /// Resolves a `companies(first:, query:)` connection from locally staged
    /// companies. Supported field-scoped query terms match the staged company
    /// graph; unsupported terms produce an empty local connection.
    fn b2b_companies_connection(&self, field: &RootFieldSelection) -> Value {
        let companies = self
            .store
            .staged
            .b2b_companies
            .values()
            .cloned()
            .collect::<Vec<_>>();
        selected_staged_connection_with_args(
            companies,
            &field.arguments,
            &field.selection,
            b2b_company_search_decision,
            b2b_company_sort_key,
            |company, selections| self.b2b_company_selected_json(company, selections),
            value_id_cursor,
        )
    }

    fn b2b_companies_count(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &staged_count_with_limit_precision(
                self.store.staged.b2b_companies.len(),
                &field.arguments,
            ),
            &field.selection,
        )
    }

    fn b2b_company_locations_connection(&self, field: &RootFieldSelection) -> Value {
        selected_staged_connection_with_args(
            self.b2b_ordered_locations(),
            &field.arguments,
            &field.selection,
            b2b_company_location_search_decision,
            b2b_company_location_sort_key,
            |location, selections| self.b2b_company_location_selected_json(location, selections),
            value_id_cursor,
        )
    }

    fn b2b_company_selected_json(&self, company: &Value, selections: &[SelectedField]) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "locations" => Some(self.b2b_selected_id_connection_json(
                company,
                "locationIds",
                selection,
                |proxy, id| proxy.store.staged.b2b_locations.get(id).cloned(),
                |proxy, location, fields| {
                    proxy.b2b_company_location_selected_json(location, fields)
                },
            )),
            "contacts" => Some(self.b2b_selected_id_connection_json(
                company,
                "contactIds",
                selection,
                |proxy, id| proxy.store.staged.b2b_contacts.get(id).cloned(),
                |proxy, contact, fields| proxy.b2b_company_contact_selected_json(contact, fields),
            )),
            "contactRoles" => Some(self.b2b_selected_id_connection_json(
                company,
                "contactRoleIds",
                selection,
                |proxy, id| proxy.store.staged.b2b_contact_roles.get(id).cloned(),
                |_, role, fields| selected_json(role, fields),
            )),
            "contactsCount" => {
                let count = b2b_json_id_list(company, "contactIds").len();
                Some(selected_count_json(count, &selection.selection))
            }
            "locationsCount" => {
                let count = b2b_json_id_list(company, "locationIds").len();
                Some(selected_count_json(count, &selection.selection))
            }
            "mainContact" => Some(self.b2b_selected_reference_json(
                company,
                "mainContactId",
                selection,
                |proxy, id| proxy.store.staged.b2b_contacts.get(id).cloned(),
                |proxy, contact, fields| proxy.b2b_company_contact_selected_json(contact, fields),
            )),
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
                Some(selected_staged_connection_with_args(
                    assignments,
                    &selection.arguments,
                    &selection.selection,
                    b2b_nested_connection_search_decision,
                    b2b_role_assignment_sort_key,
                    |assignment, fields| self.b2b_role_assignment_selected_json(assignment, fields),
                    value_id_cursor,
                ))
            }
            "company" => Some(self.b2b_selected_reference_json(
                contact,
                "companyId",
                selection,
                |proxy, id| proxy.store.staged.b2b_companies.get(id).cloned(),
                |proxy, company, fields| proxy.b2b_company_selected_json(company, fields),
            )),
            "customer" => Some(self.b2b_selected_reference_json(
                contact,
                "customerId",
                selection,
                |proxy, id| proxy.store.staged.customers.get(id).cloned(),
                |_, customer, fields| selected_json(customer, fields),
            )),
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
            "company" => Some(self.b2b_selected_reference_json(
                location,
                "companyId",
                selection,
                |proxy, id| proxy.store.staged.b2b_companies.get(id).cloned(),
                |proxy, company, fields| proxy.b2b_company_selected_json(company, fields),
            )),
            "roleAssignments" => Some(self.b2b_selected_id_connection_json(
                location,
                "roleAssignmentIds",
                selection,
                |proxy, id| proxy.store.staged.b2b_role_assignments.get(id).cloned(),
                |proxy, assignment, fields| {
                    proxy.b2b_role_assignment_selected_json(assignment, fields)
                },
            )),
            "staffMemberAssignments" => Some(self.b2b_selected_id_connection_json(
                location,
                "staffAssignmentIds",
                selection,
                |proxy, id| proxy.store.staged.b2b_staff_assignments.get(id).cloned(),
                |proxy, assignment, fields| {
                    proxy.b2b_staff_assignment_selected_json(assignment, fields)
                },
            )),
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
            "companyContact" => Some(self.b2b_selected_reference_json(
                assignment,
                "companyContactId",
                selection,
                |proxy, id| proxy.store.staged.b2b_contacts.get(id).cloned(),
                |proxy, contact, fields| proxy.b2b_company_contact_selected_json(contact, fields),
            )),
            "role" => Some(self.b2b_selected_reference_json(
                assignment,
                "companyContactRoleId",
                selection,
                |proxy, id| proxy.store.staged.b2b_contact_roles.get(id).cloned(),
                |_, role, fields| selected_json(role, fields),
            )),
            "companyLocation" => Some(self.b2b_selected_reference_json(
                assignment,
                "companyLocationId",
                selection,
                |proxy, id| proxy.store.staged.b2b_locations.get(id).cloned(),
                |proxy, location, fields| {
                    proxy.b2b_company_location_selected_json(location, fields)
                },
            )),
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
            "companyLocation" => Some(self.b2b_selected_reference_json(
                assignment,
                "companyLocationId",
                selection,
                |proxy, id| proxy.store.staged.b2b_locations.get(id).cloned(),
                |proxy, location, fields| {
                    proxy.b2b_company_location_selected_json(location, fields)
                },
            )),
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
            .b2b_locations
            .order
            .iter()
            .filter_map(|id| self.store.staged.b2b_locations.get(id).cloned())
            .collect()
    }

    fn b2b_build_company_location(
        &mut self,
        company_id: &str,
        company: &Value,
        input: &BTreeMap<String, ResolvedValue>,
        name_fallback: B2bLocationNameFallback,
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
        let shipping_address_name_fallback = match name_fallback {
            B2bLocationNameFallback::CompanyName => None,
            B2bLocationNameFallback::ShippingAddressThenCompanyName => shipping_address.as_ref(),
        };
        let name = b2b_location_name(input, company, shipping_address_name_fallback);
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
            .b2b_locations
            .order
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

    pub(in crate::proxy) fn b2b_contact_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_success_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyContactDelete" | "companyContactRemoveFromCompany" => {
                            resolved_string_field(&field.arguments, "companyContactId")
                                .into_iter()
                                .collect::<Vec<String>>()
                        }
                        "companyContactsDelete" => {
                            list_string_field(&field.arguments, "companyContactIds")
                        }
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            },
            |proxy, contact_ids, _| {
                for contact_id in contact_ids {
                    if proxy.store.staged.b2b_contacts.contains_key(&contact_id) {
                        proxy.b2b_delete_company_contact(&contact_id);
                    }
                }
            },
        )
    }

    pub(in crate::proxy) fn b2b_company_address_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_success_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .filter(|field| field.name == "companyAddressDelete")
                    .filter_map(|field| {
                        resolved_string_field(&field.arguments, "addressId")
                            .or_else(|| resolved_string_field(&field.arguments, "id"))
                    })
                    .collect::<Vec<String>>()
            },
            |proxy, address_ids, _| {
                for address_id in &address_ids {
                    proxy.b2b_delete_company_address(address_id);
                }
            },
        )
    }

    pub(in crate::proxy) fn b2b_company_contact_create_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_company_tail_helper_response(
            request,
            query,
            variables,
            operation_type,
            parsed_root_fields,
        )
        .unwrap_or_else(|| {
            self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            )
        })
    }

    pub(in crate::proxy) fn b2b_company_contact_update_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_success_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .find(|field| field.name == "companyContactUpdate")
                    .cloned()
            },
            |proxy, update, response| {
                if let Some(field) = update {
                    // Reuse the snapshot payload builder purely for its staging
                    // side-effect; the authoritative response is the upstream one.
                    let _ = proxy.b2b_company_contact_update_payload(&field);
                    // The contact is staged under the synthetic id minted at company
                    // create time, but a node(id) read after the update threads the
                    // real id Shopify returned. Mirror the now-updated contact under
                    // that real id so the generic Node read resolves it, keeping the
                    // synthetic-keyed record intact for connection reads that still
                    // address it by the create-time id.
                    let synthetic_id = resolved_string_field(&field.arguments, "companyContactId")
                        .unwrap_or_default();
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
                                proxy.store.staged.b2b_contacts.get(&synthetic_id).cloned()
                            {
                                contact["id"] = json!(real_id);
                                proxy.store.staged.b2b_contacts.insert(real_id, contact);
                            }
                        }
                    }
                }
            },
        )
    }

    pub(in crate::proxy) fn b2b_assign_main_contact_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_success_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .find(|field| field.name == "companyAssignMainContact")
                    .cloned()
            },
            |proxy, assign, _| {
                if let Some(field) = assign {
                    let company_id =
                        resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
                    let contact_id = resolved_string_field(&field.arguments, "companyContactId")
                        .unwrap_or_default();
                    proxy.b2b_set_main_contact(&company_id, Some(&contact_id));
                }
            },
        )
    }

    pub(in crate::proxy) fn b2b_revoke_main_contact_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_success_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .find(|field| field.name == "companyRevokeMainContact")
                    .cloned()
            },
            |proxy, revoke, _| {
                if let Some(field) = revoke {
                    let company_id =
                        resolved_string_field(&field.arguments, "companyId").unwrap_or_default();
                    proxy.b2b_set_main_contact(&company_id, None);
                }
            },
        )
    }

    pub(in crate::proxy) fn b2b_company_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_deleted_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyDelete" => resolved_string_field(&field.arguments, "id")
                            .into_iter()
                            .collect::<Vec<String>>(),
                        "companiesDelete" => list_string_field(&field.arguments, "companyIds"),
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            },
            |proxy, company_id| proxy.b2b_delete_company(company_id),
        )
    }

    pub(in crate::proxy) fn b2b_company_locations_delete_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_deleted_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyLocationDelete" => {
                            resolved_string_field(&field.arguments, "companyLocationId")
                                .or_else(|| resolved_string_field(&field.arguments, "id"))
                                .into_iter()
                                .collect::<Vec<String>>()
                        }
                        "companyLocationsDelete" => {
                            list_string_field(&field.arguments, "companyLocationIds")
                        }
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            },
            |proxy, location_id| proxy.b2b_delete_company_location(location_id),
        )
    }

    pub(in crate::proxy) fn b2b_revoke_roles_with_cascade(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        parsed_root_fields: &[String],
        root_field: &str,
    ) -> Response {
        self.b2b_passthrough_with_deleted_cascade(
            (
                request,
                query,
                variables,
                operation_type,
                parsed_root_fields,
                root_field,
            ),
            |fields| {
                fields
                    .iter()
                    .flat_map(|field| match field.name.as_str() {
                        "companyContactRevokeRole" => resolved_string_field(
                            &field.arguments,
                            "companyContactRoleAssignmentId",
                        )
                        .into_iter()
                        .collect::<Vec<String>>(),
                        "companyContactRevokeRoles" => {
                            list_string_field(&field.arguments, "roleAssignmentIds")
                        }
                        "companyLocationRevokeRoles" => {
                            list_string_field(&field.arguments, "rolesToRevoke")
                        }
                        _ => Vec::new(),
                    })
                    .collect::<Vec<String>>()
            },
            |proxy, assignment_id| {
                let _ = proxy.b2b_remove_role_assignment(assignment_id);
            },
        )
    }

    /// Removes a single role assignment from staged state and detaches it from its
    /// location's `roleAssignmentIds` list. A contact's roleAssignments connection
    /// is resolved by filtering the assignment map, so dropping the entry here is
    /// enough to clear it from the contact view too.
    fn b2b_remove_role_assignment(&mut self, assignment_id: &str) -> Option<Value> {
        let assignment = self
            .store
            .staged
            .b2b_role_assignments
            .remove(assignment_id)?;
        if let Some(location_id) = assignment["companyLocationId"].as_str() {
            self.b2b_remove_location_assignment_id(location_id, "roleAssignmentIds", assignment_id);
        }
        Some(assignment)
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

    /// Removes an upstream-confirmed company and the staged contacts/locations listed on it.
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

    fn b2b_valid_staff_member_id(&self, id: &str) -> bool {
        shopify_gid_resource_type(id) == Some("StaffMember")
            && self
                .store
                .staged
                .b2b_staff_assignments
                .values()
                .any(|assignment| {
                    assignment["staffMemberId"].as_str() == Some(id)
                        || assignment["staffMember"]["id"].as_str() == Some(id)
                })
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
                || (reject_url && super::customer_address_contains_url(&value));
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
/// `["input","companyContact"]`). Missing email rejects the nested contact
/// before the company tree is staged. A malformed email surfaces as
/// "Email is invalid"/INVALID on the email field path; HTML markup in a name
/// surfaces as a generic "Invalid input."/INVALID_INPUT on the parent input path,
/// matching live Admin's BusinessCustomerUserError shape.
fn b2b_contact_input_errors(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &[&str],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "email").is_none() {
        errors.push(b2b_missing_contact_customer_reference_error(
            prefix.to_vec(),
        ));
        return errors;
    }
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

fn b2b_location_create_has_meaningful_non_address_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    input.iter().any(|(field, value)| {
        !matches!(
            field.as_str(),
            "billingAddress" | "shippingAddress" | "billingSameAsShipping"
        ) && !matches!(value, ResolvedValue::Null)
    })
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
    user_error(json!([field, index.to_string()]), message, Some(code))
}

fn b2b_bulk_role_already_assigned_error(index: usize) -> Value {
    b2b_indexed_user_error(
        "rolesToAssign",
        index,
        "Company contact has already been assigned a role in that company location.",
        "LIMIT_REACHED",
    )
}

fn b2b_bulk_action_limit_payload(
    input_size: usize,
    argument_field: &str,
    empty_result_fields: &[&str],
) -> Option<(Value, &'static str, Vec<String>)> {
    if input_size <= B2B_BULK_ACTIONS_MAX_SIZE {
        return None;
    }
    let mut payload = serde_json::Map::new();
    for field in empty_result_fields {
        payload.insert((*field).to_string(), json!([]));
    }
    payload.insert(
        "userErrors".to_string(),
        json!([user_error(
            vec![argument_field],
            B2B_BULK_ACTION_LIMIT_REACHED_MESSAGE,
            Some("LIMIT_REACHED")
        )]),
    );
    Some((Value::Object(payload), "failed", Vec::new()))
}

fn b2b_resource_not_found(field: impl Into<UserErrorField>) -> Value {
    user_error(
        field,
        "Resource requested does not exist.",
        Some("RESOURCE_NOT_FOUND"),
    )
}

fn b2b_not_found(field: impl Into<UserErrorField>, message: &str) -> Value {
    user_error(field, message, Some("RESOURCE_NOT_FOUND"))
}

/// Validates a `companyContactCreate` input: a name carrying HTML, a name that
/// exceeds Shopify's 255-character limit, a missing email, or an invalid email
/// each produces a Shopify-shaped user error. Title values are stored verbatim.
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
    for (name_field, label) in [("firstName", "First name"), ("lastName", "Last name")] {
        if let Some(value) = resolved_string_field(input, name_field) {
            if b2b_contains_html_tags(&value) {
                errors.push(user_error(
                    json!(prefix),
                    "Invalid input.",
                    Some("INVALID_INPUT"),
                ));
                break;
            } else if value.chars().count() > 255 {
                errors.push(user_error(
                    json!(field_path(name_field)),
                    &format!("{label} is too long"),
                    Some("TOO_LONG"),
                ));
            }
        }
    }
    if resolved_string_field(input, "email").is_none() {
        errors.push(b2b_missing_contact_customer_reference_error(
            prefix.to_vec(),
        ));
        return errors;
    }
    if let Some(email) = resolved_string_field(input, "email") {
        if !is_valid_customer_email(&email) {
            errors.push(user_error(
                json!(field_path("email")),
                "Email is invalid",
                Some("INVALID"),
            ));
        }
    }
    errors
}

fn b2b_missing_contact_customer_reference_error(field: Vec<&str>) -> Value {
    b2b_company_user_error(
        field,
        "Either the attribute email or customer_id must be provided",
        "INVALID",
        None,
    )
}

/// True when a staged order/draft-order record references the given company via
/// its purchasing entity (directly, or through a draft order's nested completed
/// order) — i.e. the company is still in use and cannot be deleted.
fn b2b_record_references_company(record: &Value, company_id: &str) -> bool {
    b2b_record_references(record, company_id, b2b_value_contains_company_id)
}

/// True when a staged order/draft-order record references the given company
/// location through its purchasing entity.
fn b2b_record_references_company_location(record: &Value, location_id: &str) -> bool {
    b2b_record_references(record, location_id, b2b_value_contains_company_location_id)
}

fn b2b_record_references<F>(record: &Value, id: &str, contains_id: F) -> bool
where
    F: Fn(&Value, &str) -> bool,
{
    if let Some(entity) = record.get("purchasingEntity") {
        if contains_id(entity, id) {
            return true;
        }
    }
    if let Some(entity) = record.get("__draftProxyPurchasingEntity") {
        if contains_id(entity, id) {
            return true;
        }
    }
    if let Some(order) = record.get("order") {
        if order
            .get("purchasingEntity")
            .is_some_and(|entity| contains_id(entity, id))
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

fn b2b_company_search_decision(company: &Value, query: Option<&str>) -> StagedSearchDecision {
    b2b_field_scoped_search_decision(query, |field, value| match field {
        "id" => Some(b2b_search_id_matches(company["id"].as_str(), value)),
        "name" => Some(b2b_search_string_matches(company["name"].as_str(), value)),
        "external_id" | "externalid" => Some(b2b_search_string_matches(
            company["externalId"].as_str(),
            value,
        )),
        _ => None,
    })
}

fn b2b_company_location_search_decision(
    location: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    b2b_field_scoped_search_decision(query, |field, value| match field {
        "id" => Some(b2b_search_id_matches(location["id"].as_str(), value)),
        "name" => Some(b2b_search_string_matches(location["name"].as_str(), value)),
        "external_id" | "externalid" => Some(b2b_search_string_matches(
            location["externalId"].as_str(),
            value,
        )),
        "company_id" | "companyid" => {
            Some(b2b_search_id_matches(location["companyId"].as_str(), value))
        }
        _ => None,
    })
}

fn b2b_nested_connection_search_decision(_: &Value, query: Option<&str>) -> StagedSearchDecision {
    match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(_) => StagedSearchDecision::Unsupported,
        None => StagedSearchDecision::Match,
    }
}

fn b2b_field_scoped_search_decision<F>(query: Option<&str>, mut matches: F) -> StagedSearchDecision
where
    F: FnMut(&str, &str) -> Option<bool>,
{
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    let Some(terms) = b2b_field_scoped_query_terms(query) else {
        return StagedSearchDecision::Unsupported;
    };
    let mut matched_any = false;
    for (field, value) in terms {
        match matches(&field, &value) {
            Some(true) => matched_any = true,
            Some(false) => return StagedSearchDecision::NoMatch,
            None => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::from_bool(matched_any)
}

fn b2b_field_scoped_query_terms(query: &str) -> Option<Vec<(String, String)>> {
    let mut rest = query.trim();
    let mut terms = Vec::new();
    while !rest.is_empty() {
        let colon = rest.find(':')?;
        let field = rest[..colon].trim();
        if field.is_empty() || field.chars().any(char::is_whitespace) {
            return None;
        }
        rest = rest[colon + 1..].trim_start();
        let (raw_value, remaining) = if let Some(quote) = rest
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''))
        {
            let value_start = quote.len_utf8();
            let after_quote = &rest[value_start..];
            let end = after_quote.find(quote)?;
            (&after_quote[..end], &after_quote[end + quote.len_utf8()..])
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (&rest[..end], &rest[end..])
        };
        let value = raw_value.trim();
        if value.is_empty() {
            return None;
        }
        terms.push((field.to_ascii_lowercase(), value.to_ascii_lowercase()));
        rest = remaining.trim_start();
    }
    Some(terms)
}

fn b2b_search_string_matches(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let actual = actual.to_ascii_lowercase();
    if let Some(prefix) = query_value.strip_suffix('*') {
        !prefix.is_empty() && actual.starts_with(prefix)
    } else {
        actual.contains(query_value)
    }
}

fn b2b_search_id_matches(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let actual = actual.to_ascii_lowercase();
    let tail = resource_id_tail(&actual);
    let path_tail = resource_id_path_tail(&actual);
    actual == query_value || tail == query_value || path_tail == query_value
}

fn b2b_nested_connection_sort_key(
    id_list_field: &str,
    record: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    match id_list_field {
        "locationIds" => b2b_company_location_sort_key(record, sort_key),
        "contactIds" => b2b_company_contact_sort_key(record, sort_key),
        "contactRoleIds" => b2b_company_contact_role_sort_key(record, sort_key),
        "roleAssignmentIds" => b2b_role_assignment_sort_key(record, sort_key),
        "staffAssignmentIds" => b2b_staff_assignment_sort_key(record, sort_key),
        _ => b2b_id_sort_key(record),
    }
}

fn b2b_company_sort_key(company: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "NAME" => b2b_sort_key_with_id(b2b_string_sort_value(company, "name"), company),
        "CREATED_AT" => b2b_sort_key_with_id(b2b_string_sort_value(company, "createdAt"), company),
        "UPDATED_AT" => b2b_sort_key_with_id(b2b_string_sort_value(company, "updatedAt"), company),
        _ => b2b_id_sort_key(company),
    }
}

fn b2b_company_location_sort_key(location: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "NAME" => b2b_sort_key_with_id(b2b_string_sort_value(location, "name"), location),
        "CREATED_AT" => {
            b2b_sort_key_with_id(b2b_string_sort_value(location, "createdAt"), location)
        }
        "UPDATED_AT" => {
            b2b_sort_key_with_id(b2b_string_sort_value(location, "updatedAt"), location)
        }
        _ => b2b_id_sort_key(location),
    }
}

fn b2b_company_contact_sort_key(contact: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "TITLE" | "NAME" => b2b_sort_key_with_id(b2b_string_sort_value(contact, "title"), contact),
        "CREATED_AT" => b2b_sort_key_with_id(b2b_string_sort_value(contact, "createdAt"), contact),
        "UPDATED_AT" => b2b_sort_key_with_id(b2b_string_sort_value(contact, "updatedAt"), contact),
        _ => b2b_id_sort_key(contact),
    }
}

fn b2b_company_contact_role_sort_key(role: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "NAME" => b2b_sort_key_with_id(b2b_string_sort_value(role, "name"), role),
        _ => b2b_id_sort_key(role),
    }
}

fn b2b_role_assignment_sort_key(assignment: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "COMPANY_CONTACT_ID" | "COMPANYCONTACTID" => b2b_sort_key_with_id(
            b2b_string_sort_value(assignment, "companyContactId"),
            assignment,
        ),
        "COMPANY_CONTACT_ROLE_ID" | "COMPANYCONTACTROLEID" => b2b_sort_key_with_id(
            b2b_string_sort_value(assignment, "companyContactRoleId"),
            assignment,
        ),
        "COMPANY_LOCATION_ID" | "COMPANYLOCATIONID" => b2b_sort_key_with_id(
            b2b_string_sort_value(assignment, "companyLocationId"),
            assignment,
        ),
        _ => b2b_id_sort_key(assignment),
    }
}

fn b2b_staff_assignment_sort_key(assignment: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match b2b_normalized_sort_key(sort_key).as_str() {
        "COMPANY_LOCATION_ID" | "COMPANYLOCATIONID" => b2b_sort_key_with_id(
            b2b_string_sort_value(assignment, "companyLocationId"),
            assignment,
        ),
        "STAFF_MEMBER_ID" | "STAFFMEMBERID" => b2b_sort_key_with_id(
            b2b_string_sort_value(assignment, "staffMemberId"),
            assignment,
        ),
        _ => b2b_id_sort_key(assignment),
    }
}

fn b2b_normalized_sort_key(sort_key: Option<&str>) -> String {
    sort_key.unwrap_or("ID").to_ascii_uppercase()
}

fn b2b_sort_key_with_id(mut first: StagedSortValue, record: &Value) -> StagedSortKey {
    if matches!(first, StagedSortValue::String(ref value) if value.is_empty()) {
        first = StagedSortValue::Null;
    }
    let mut key = vec![first];
    key.extend(b2b_id_sort_key(record));
    key
}

fn b2b_id_sort_key(record: &Value) -> StagedSortKey {
    let id = record["id"].as_str().unwrap_or_default();
    vec![
        resource_id_tail(id)
            .parse::<i64>()
            .map(StagedSortValue::I64)
            .unwrap_or(StagedSortValue::Null),
        StagedSortValue::String(id.to_ascii_lowercase()),
    ]
}

fn b2b_string_sort_value(record: &Value, field: &str) -> StagedSortValue {
    record[field]
        .as_str()
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
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

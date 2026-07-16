use super::*;

/// Snapshot of a staged customer's inline-address context:
/// `(firstName, lastName, addressesV2.nodes, defaultAddress.id)`.
type CustomerAddressContext = (Option<String>, Option<String>, Vec<Value>, Option<String>);

impl DraftProxy {
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
    pub(in crate::proxy) fn customer_address_mutation_outcome(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return resolver_http_error_outcome(400, "Could not parse GraphQL operation");
        };
        let Some(field) = fields
            .iter()
            .find(|field| field.response_key == response_key)
        else {
            return resolver_http_error_outcome(400, "Unsupported customer address mutation");
        };
        let (payload, staged_ids, top_errors) = match field.name.as_str() {
            "customerAddressCreate" => self.customer_address_create(field),
            "customerAddressUpdate" => self.customer_address_update(field),
            "customerAddressDelete" => self.customer_address_delete(field),
            "customerUpdateDefaultAddress" => self.customer_update_default_address(field),
            _ => {
                return resolver_http_error_outcome(400, "Unsupported customer address mutation");
            }
        };
        let value = if payload.is_null() {
            Value::Null
        } else {
            selected_json(&payload, &field.selection)
        };
        let mut outcome = ResolverOutcome::value(value)
            .with_errors(root_field_errors_from_json(&top_errors, response_key));
        if !staged_ids.is_empty() {
            outcome
                .log_drafts
                .push(LogDraft::staged(&field.name, "customers", staged_ids));
        }
        outcome
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
                    vec![user_error_omit_code(
                        json!(["customerId"]),
                        "Customer does not exist",
                        None,
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
                    vec![user_error_omit_code(
                        json!(["address"]),
                        "Address already exists",
                        None,
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
                    vec![user_error_omit_code(
                        json!(["addressId"]),
                        "The id of the address does not match the id in the input",
                        None,
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let Some((customer_first, customer_last, existing_nodes, current_default)) =
            self.customer_address_context(&customer_id)
        else {
            return self.customer_address_missing_customer_result(
                &address_id,
                &field.response_key,
                |errors| customer_address_payload(Value::Null, errors),
            );
        };
        let index = customer_address_node_index(&existing_nodes, &address_id);
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
        let Some((_, _, existing_nodes, current_default)) =
            self.customer_address_context(&customer_id)
        else {
            return self.customer_address_missing_customer_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "deletedAddressId": Value::Null, "userErrors": errors }),
            );
        };
        let index = customer_address_node_index(&existing_nodes, &address_id);
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
        let missing_address_result = |me: &Self| {
            if me.customer_address_exists_anywhere(&address_id) {
                let customer = render_customer(me);
                return (
                    json!({
                        "customer": customer,
                        "userErrors": [user_error_omit_code(json!(["addressId"]), "Address does not exist", None)]
                    }),
                    Vec::new(),
                    Vec::new(),
                );
            }
            (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(
                    &field.response_key,
                )],
            )
        };
        let Some((_, _, existing_nodes, _)) = self.customer_address_context(&customer_id) else {
            return self.customer_address_missing_customer_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "customer": Value::Null, "userErrors": errors }),
            );
        };
        let index = customer_address_node_index(&existing_nodes, &address_id);
        let Some(index) = index else {
            // Address belongs to another customer (exists somewhere) → userError,
            // but the customer record is still returned. Truly unknown ids return
            // a null payload with a RESOURCE_NOT_FOUND top-level error.
            return missing_address_result(self);
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

    fn customer_address_missing_customer_result(
        &self,
        address_id: &str,
        response_key: &str,
        build_payload: impl Fn(Vec<Value>) -> Value,
    ) -> (Value, Vec<String>, Vec<Value>) {
        if self.customer_address_exists_anywhere(address_id) {
            (
                build_payload(vec![user_error_omit_code(
                    json!(["customerId"]),
                    "Customer does not exist",
                    None,
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
                build_payload(vec![user_error_omit_code(
                    json!(["addressId"]),
                    "Address does not exist",
                    None,
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
}

pub(in crate::proxy) fn customer_address_cursor(address: &Value) -> Option<String> {
    address
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
}

pub(super) fn selected_customer_addresses_connection(
    customer: &Value,
    field: &SelectedField,
) -> Value {
    selected_connection_json_with_args(
        connection_nodes(&customer["addressesV2"]),
        &field.arguments,
        &field.selection,
        |address| customer_address_cursor(address).unwrap_or_default(),
    )
}

pub(super) fn customer_mailing_addresses(
    values: &[ResolvedValue],
    customer_set: bool,
) -> (Vec<Value>, Vec<Value>) {
    let mut addresses = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let Some(input) = resolved_value_object(value) else {
            continue;
        };
        let (address, mut address_errors) = customer_mailing_address(&input, index, customer_set);
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

const CUSTOMER_ADDRESS_FREE_TEXT_FIELDS: &[&str] = &[
    "firstName",
    "lastName",
    "address1",
    "address2",
    "city",
    "company",
    "zip",
    "phone",
];

struct CustomerAddressNodeFields {
    id: String,
    first_name: Option<String>,
    last_name: Option<String>,
    address1: Option<String>,
    address2: Option<String>,
    city: Option<String>,
    company: Option<String>,
    zip: Option<String>,
    phone: Option<String>,
    country: Option<CustomerCountry>,
    province: Option<CustomerProvince>,
}

fn customer_resolve_address_region(
    country_input: Option<String>,
    province_input: Option<String>,
    country_error_path: Value,
    province_error_path: Value,
    errors: &mut Vec<Value>,
) -> (Option<CustomerCountry>, Option<CustomerProvince>) {
    let country = match country_input
        .as_deref()
        .and_then(customer_country_from_input)
    {
        Some(country) => Some(country),
        None if country_input.is_some() => {
            errors.push(user_error_omit_code(
                country_error_path,
                "Country is invalid",
                None,
            ));
            None
        }
        None => None,
    };
    let province = match (country.as_ref(), province_input.as_deref()) {
        (Some(country), Some(raw_province)) => {
            match customer_province_from_input(country.code.as_str(), raw_province) {
                Some(province) => province,
                None => {
                    errors.push(user_error_omit_code(
                        province_error_path,
                        "Province is invalid",
                        None,
                    ));
                    None
                }
            }
        }
        _ => None,
    };
    (country, province)
}

fn customer_address_node_json(fields: CustomerAddressNodeFields) -> Value {
    let name = [fields.first_name.as_deref(), fields.last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area = customer_formatted_area(
        fields.city.as_deref(),
        fields.country.as_ref(),
        fields.province.as_ref(),
    );
    json!({
        "id": fields.id,
        "firstName": fields.first_name,
        "lastName": fields.last_name,
        "address1": fields.address1,
        "address2": fields.address2,
        "city": fields.city,
        "company": fields.company,
        "province": fields.province.as_ref().map(|province| province.name.as_str()),
        "provinceCode": fields.province.as_ref().map(|province| province.code.as_str()),
        "country": fields.country.as_ref().map(|country| country.name.as_str()),
        "countryCodeV2": fields.country.as_ref().map(|country| country.code.as_str()),
        "zip": fields.zip,
        "phone": fields.phone,
        "name": if name.is_empty() { Value::Null } else { json!(name) },
        "formattedArea": formatted_area,
    })
}

fn customer_address_free_text_errors<F>(
    input: &BTreeMap<String, ResolvedValue>,
    path_for: F,
) -> Vec<Value>
where
    F: Fn(&str) -> Value,
{
    let mut errors = Vec::new();
    for field in CUSTOMER_ADDRESS_FREE_TEXT_FIELDS {
        if let Some(value) = customer_address_string(input, field) {
            let label = customer_address_field_label(field);
            if value.chars().count() > 255 {
                errors.push(user_error_omit_code(
                    path_for(field),
                    &format!("{label} is too long (maximum is 255 characters)"),
                    None,
                ));
            }
            if customer_address_contains_html(&value) {
                errors.push(user_error_omit_code(
                    path_for(field),
                    &format!("{label} cannot contain HTML tags"),
                    None,
                ));
            }
            if matches!(*field, "city" | "zip" | "phone") && customer_address_contains_url(&value) {
                errors.push(user_error_omit_code(
                    path_for(field),
                    &format!("{label} cannot contain URL"),
                    None,
                ));
            }
            if customer_address_contains_emoji(&value) {
                errors.push(user_error_omit_code(
                    path_for(field),
                    &format!("{label} cannot contain emojis"),
                    None,
                ));
            }
        }
    }
    errors
}

fn customer_mailing_address(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    customer_set: bool,
) -> (Value, Vec<Value>) {
    let mut errors = customer_address_free_text_errors(input, |field| {
        customer_address_field_path(customer_set, index, Some(field))
    });

    let country_input = customer_address_string(input, "countryCode")
        .or_else(|| customer_address_string(input, "countryCodeV2"))
        .or_else(|| customer_address_string(input, "country"));
    let province_input = customer_address_string(input, "provinceCode")
        .or_else(|| customer_address_string(input, "province"));
    let (country, province) = customer_resolve_address_region(
        country_input,
        province_input,
        customer_address_field_path(customer_set, index, Some("country")),
        customer_address_field_path(customer_set, index, Some("province")),
        &mut errors,
    );

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
        country.as_ref().map(|country| country.code.as_str()),
        province.as_ref().map(|province| province.code.as_str()),
    ]
    .into_iter()
    .flatten()
    .all(str::is_empty);
    if is_blank && !customer_set {
        return (
            Value::Null,
            vec![user_error_omit_code(
                customer_address_field_path(customer_set, index, None),
                "Customer address cannot be blank.",
                None,
            )],
        );
    }

    (
        customer_address_node_json(CustomerAddressNodeFields {
            id: synthetic_shopify_gid("MailingAddress", index + 1),
            first_name,
            last_name,
            address1,
            address2,
            city,
            company,
            zip,
            phone,
            country,
            province,
        }),
        Vec::new(),
    )
}

pub(super) fn customer_update_mailing_address(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    existing: Option<&Value>,
    id: &str,
) -> (Value, Vec<Value>) {
    let mut errors = customer_address_free_text_errors(input, |field| {
        customer_address_field_path(false, index, Some(field))
    });

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
    let country_input = if country_present {
        customer_address_string(input, "countryCode")
            .or_else(|| customer_address_string(input, "countryCodeV2"))
            .or_else(|| customer_address_string(input, "country"))
    } else {
        existing
            .and_then(|node| node.get("countryCodeV2"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let province_present = input.contains_key("provinceCode") || input.contains_key("province");
    let province_input = if province_present {
        customer_address_string(input, "provinceCode")
            .or_else(|| customer_address_string(input, "province"))
    } else {
        existing
            .and_then(|node| node.get("provinceCode"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let (country, province) = customer_resolve_address_region(
        country_input,
        province_input,
        customer_address_field_path(false, index, Some("country")),
        customer_address_field_path(false, index, Some("province")),
        &mut errors,
    );

    if !errors.is_empty() {
        return (Value::Null, errors);
    }

    let first_name = field_value("firstName");
    let last_name = field_value("lastName");
    let address1 = field_value("address1");
    let address2 = field_value("address2");
    let city = field_value("city");
    let company = field_value("company");
    let zip = field_value("zip");
    let phone = field_value("phone");
    let is_blank = [
        first_name.as_deref(),
        last_name.as_deref(),
        address1.as_deref(),
        address2.as_deref(),
        city.as_deref(),
        company.as_deref(),
        zip.as_deref(),
        phone.as_deref(),
        country.as_ref().map(|country| country.code.as_str()),
        province.as_ref().map(|province| province.code.as_str()),
    ]
    .into_iter()
    .flatten()
    .all(str::is_empty);
    if is_blank {
        return (
            Value::Null,
            vec![user_error_omit_code(
                customer_address_field_path(false, index, None),
                "Customer address cannot be blank.",
                None,
            )],
        );
    }

    (
        customer_address_node_json(CustomerAddressNodeFields {
            id: id.to_string(),
            first_name,
            last_name,
            address1,
            address2,
            city,
            company,
            zip,
            phone,
            country,
            province,
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

pub(in crate::proxy) fn customer_address_nodes(customer: &Value) -> Vec<Value> {
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
pub(in crate::proxy) fn customer_address_dedup_key(node: &Value) -> String {
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
pub(in crate::proxy) fn customer_rebuild_addresses(
    customer: &mut Value,
    nodes: Vec<Value>,
    default_id: Option<&str>,
) {
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
                "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
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
pub(in crate::proxy) fn customer_address_input_node(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    customer_first: Option<&str>,
    customer_last: Option<&str>,
    id: &str,
) -> (Option<Value>, Vec<Value>) {
    let mut errors = customer_address_free_text_errors(input, |field| json!(["address", field]));

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
    let (country, province) = customer_resolve_address_region(
        country_raw,
        province_raw,
        json!(["address", "country"]),
        json!(["address", "province"]),
        &mut errors,
    );

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
    let phone = if input.contains_key("phone") {
        customer_address_string(input, "phone").map(|phone| {
            normalize_customer_address_phone(
                &phone,
                country.as_ref().map(|country| country.code.as_str()),
            )
            .unwrap_or(phone)
        })
    } else {
        field_value("phone")
    };
    (
        Some(customer_address_node_json(CustomerAddressNodeFields {
            id: id.to_string(),
            first_name,
            last_name,
            address1,
            address2,
            city,
            company,
            zip,
            phone,
            country,
            province,
        })),
        Vec::new(),
    )
}

fn normalize_customer_address_phone(raw: &str, country_code: Option<&str>) -> Option<String> {
    normalize_phone_with_country_context(raw, country_code, false)
}

#[derive(Clone)]
pub(super) struct CustomerCountry {
    pub(super) code: String,
    pub(super) name: String,
}

#[derive(Clone)]
struct CustomerProvince {
    code: String,
    name: String,
}

pub(super) fn customer_address_string(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
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

pub(super) fn customer_address_field_path(
    customer_set: bool,
    index: usize,
    field: Option<&str>,
) -> Value {
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

pub(super) fn customer_address_contains_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

fn customer_address_contains_emoji(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF))
}

pub(super) fn customer_country_from_input(value: &str) -> Option<CustomerCountry> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some((code, name)) = CUSTOMER_COUNTRIES.iter().find(|(code, name)| {
        code.eq_ignore_ascii_case(normalized) || name.eq_ignore_ascii_case(normalized)
    }) {
        return Some(CustomerCountry {
            code: (*code).to_string(),
            name: (*name).to_string(),
        });
    }
    let code = normalized.to_ascii_uppercase();
    if !location_country_code_is_valid(&code) {
        return None;
    }
    let name = country_name_for_code(&code)
        .map(str::to_string)
        .unwrap_or_else(|| code.clone());
    Some(CustomerCountry { code, name })
}

fn customer_province_from_input(
    country_code: &str,
    value: &str,
) -> Option<Option<CustomerProvince>> {
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
        .find(|(code, name)| {
            code.eq_ignore_ascii_case(normalized) || name.eq_ignore_ascii_case(normalized)
        })
        .map(|(code, name)| {
            Some(CustomerProvince {
                code: (*code).to_string(),
                name: (*name).to_string(),
            })
        })
}

fn customer_country_provinces(country_code: &str) -> &'static [(&'static str, &'static str)] {
    match country_code {
        "CA" => CUSTOMER_CANADIAN_PROVINCES,
        "US" => CUSTOMER_US_PROVINCES,
        "AU" => CUSTOMER_AUSTRALIAN_PROVINCES,
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
    let province_code = province.map(|province| province.code.as_str());
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

const CUSTOMER_COUNTRIES: &[(&str, &str)] = &[
    ("AR", "Argentina"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("BE", "Belgium"),
    ("BR", "Brazil"),
    ("CA", "Canada"),
    ("CH", "Switzerland"),
    ("CN", "China"),
    ("DE", "Germany"),
    ("DK", "Denmark"),
    ("ES", "Spain"),
    ("FI", "Finland"),
    ("FR", "France"),
    ("GB", "United Kingdom"),
    ("HK", "Hong Kong SAR"),
    ("IE", "Ireland"),
    ("IN", "India"),
    ("IT", "Italy"),
    ("JP", "Japan"),
    ("MX", "Mexico"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NZ", "New Zealand"),
    ("PL", "Poland"),
    ("PT", "Portugal"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("US", "United States"),
    ("ZA", "South Africa"),
];

const CUSTOMER_CANADIAN_PROVINCES: &[(&str, &str)] = &[
    ("AB", "Alberta"),
    ("BC", "British Columbia"),
    ("MB", "Manitoba"),
    ("NB", "New Brunswick"),
    ("NL", "Newfoundland and Labrador"),
    ("NS", "Nova Scotia"),
    ("NT", "Northwest Territories"),
    ("NU", "Nunavut"),
    ("ON", "Ontario"),
    ("PE", "Prince Edward Island"),
    ("QC", "Quebec"),
    ("SK", "Saskatchewan"),
    ("YT", "Yukon"),
];

const CUSTOMER_US_PROVINCES: &[(&str, &str)] = &[
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

const CUSTOMER_AUSTRALIAN_PROVINCES: &[(&str, &str)] = &[
    ("ACT", "Australian Capital Territory"),
    ("NSW", "New South Wales"),
    ("NT", "Northern Territory"),
    ("QLD", "Queensland"),
    ("SA", "South Australia"),
    ("TAS", "Tasmania"),
    ("VIC", "Victoria"),
    ("WA", "Western Australia"),
];

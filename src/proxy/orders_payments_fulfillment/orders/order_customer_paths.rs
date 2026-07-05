use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn order_customer_error_paths_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let mut declined = false;
        let data = root_payload_json(&fields, |field| {
            if declined {
                return None;
            }
            let value = match field.name.as_str() {
                "customerCreate" => self.order_customer_paths_customer_create(field),
                "companyCreate" => self.order_customer_paths_company_create(field),
                "companyAssignCustomerAsContact" => {
                    self.order_customer_paths_assign_customer(field)
                }
                "orderCreate" => self.order_customer_paths_order_create(field),
                "orderCancel" => {
                    self.order_customer_paths_cancel_order(request, query, variables, field)
                }
                "orderCustomerSet" => Some(self.order_customer_set_error_paths(request, field)),
                "orderCustomerRemove" => {
                    Some(self.order_customer_remove_error_paths(request, field))
                }
                _ => None,
            };
            let Some(value) = value else {
                declined = true;
                return None;
            };
            Some(value)
        });
        if declined {
            return None;
        }
        Some(json!({ "data": data }))
    }

    pub(in crate::proxy) fn order_customer_paths_customer_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        let first_name = resolved_string_field(&input, "firstName");
        let last_name = resolved_string_field(&input, "lastName");
        let display_name =
            order_customer_display_name(first_name.as_deref(), last_name.as_deref(), &email);
        let id = self.next_synthetic_gid("Customer");
        let customer = json!({
            "id": id,
            "email": email,
            "firstName": first_name.map(Value::String).unwrap_or(Value::Null),
            "lastName": last_name.map(Value::String).unwrap_or(Value::Null),
            "displayName": display_name
        });
        self.store.staged.customers.insert(
            customer["id"].as_str().unwrap_or_default().to_string(),
            customer.clone(),
        );
        Some(selected_json(
            &json!({ "customer": customer, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_company_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let company_input = resolved_object_field(&input, "company").unwrap_or_default();
        let name = resolved_string_field(&company_input, "name")
            .or_else(|| resolved_string_field(&input, "name"))
            .unwrap_or_else(|| "B2B Draft".to_string());
        let id = synthetic_shopify_gid("Company", self.store.staged.next_b2b_company_id);
        self.store.staged.next_b2b_company_id += 5;
        let company = json!({
            "__typename": "Company",
            "id": id,
            "name": name,
            "externalId": Value::Null,
            "customerSince": Value::Null,
            "note": Value::Null,
            "locationIds": [],
            "contactIds": [],
            "contactRoleIds": [],
            "mainContactId": Value::Null
        });
        self.store.staged.b2b_companies.insert(
            company["id"].as_str().unwrap_or_default().to_string(),
            company.clone(),
        );
        Some(selected_json(
            &json!({ "company": company, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_assign_customer(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let company_id = resolved_string_field(&field.arguments, "companyId")?;
        let customer_id = resolved_string_field(&field.arguments, "customerId")?;
        let customer = self.store.staged.customers.get(&customer_id)?.clone();
        let company = self.store.staged.b2b_companies.get(&company_id)?.clone();
        self.store
            .staged
            .order_customer_contact_customer_ids
            .insert(customer_id.clone());
        let contact_id = self.next_proxy_synthetic_gid("CompanyContact");
        let contact = json!({
            "id": contact_id,
            "companyId": company_id,
            "customerId": customer_id,
            "firstName": customer["firstName"].clone(),
            "lastName": customer["lastName"].clone(),
            "title": Value::Null,
            "locale": "en",
            "isMainContact": false
        });
        self.store
            .staged
            .b2b_contacts
            .insert(contact_id.clone(), contact.clone());
        if let Some(mut company_record) = self.store.staged.b2b_companies.get(&company_id).cloned()
        {
            if let Some(contact_ids) = company_record
                .get_mut("contactIds")
                .and_then(Value::as_array_mut)
            {
                contact_ids.push(json!(contact_id.clone()));
            }
            self.store
                .staged
                .b2b_companies
                .insert(company_id.clone(), company_record);
        }
        Some(selected_json(
            &json!({
                "companyContact": {
                    "id": contact_id,
                    "isMainContact": false,
                    "customer": { "id": customer_id },
                    "company": { "id": company_id, "name": company["name"].clone() }
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_order_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_input = resolved_object_field(&field.arguments, "order")?;
        let id = self.next_proxy_synthetic_gid("Order");
        let customer_id = resolved_string_field(&order_input, "customerId");
        // Retain purchasing entity so company delete detects B2B references.
        let purchasing_entity = self.order_create_b2b_purchasing_entity(&order_input);
        if order_customer_purchasing_entity_is_b2b(&purchasing_entity) {
            self.store
                .staged
                .order_customer_b2b_order_ids
                .insert(id.clone());
        }
        let mut order = self.build_order_create_record(&id, &order_input);
        order["purchasingEntity"] = purchasing_entity;
        if let Some(customer_id) = customer_id {
            order["customer"] = self
                .store
                .staged
                .customers
                .get(&customer_id)
                .cloned()
                .unwrap_or_else(|| json!({ "id": customer_id }));
        }
        self.store.staged.order_customer_orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        Some(selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(super) fn order_create_b2b_purchasing_entity(
        &self,
        order_input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let purchasing_entity = draft_order_purchasing_entity(order_input);
        if order_customer_purchasing_entity_is_b2b(&purchasing_entity) {
            return purchasing_entity;
        }
        let Some(location_id) = resolved_string_field(order_input, "companyLocationId") else {
            return purchasing_entity;
        };
        let company_id = self
            .store
            .staged
            .b2b_locations
            .get(&location_id)
            .and_then(|location| location["companyId"].as_str())
            .map(str::to_string);
        let contact_id = company_id.as_ref().and_then(|id| {
            self.store
                .staged
                .b2b_companies
                .get(id)
                .and_then(|company| company["mainContactId"].as_str())
                .map(str::to_string)
        });
        json!({
            "companyId": company_id,
            "companyLocationId": location_id,
            "company": company_id.as_ref().map(|id| json!({ "id": id })).unwrap_or(Value::Null),
            "contact": contact_id.as_ref().map(|id| json!({ "id": id })).unwrap_or(Value::Null),
            "location": { "id": location_id }
        })
    }

    pub(in crate::proxy) fn order_customer_paths_cancel_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_id = resolved_string_field(&field.arguments, "orderId")?;
        let argument_present = |name: &str| {
            field
                .arguments
                .get(name)
                .is_some_and(|value| !matches!(value, ResolvedValue::Null))
        };
        let refund_present = argument_present("refund");
        let refund_method_cancel = argument_present("refundMethod");
        let order_locally_known = self.store.staged.orders.contains_key(&order_id)
            || self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id);
        // Earn the order from the backend when no precondition seed staged it.
        // Synthetic order-customer ids (seeded by orderCreate error-paths) live
        // in `order_customer_orders` and must not trigger an upstream read.
        //
        // A `refundMethod` (full original-payment-method refund) cancel is the one
        // case we deliberately do NOT stage: that mutation's authoritative
        // downstream order projection (the refund ledger and the restocked
        // fulfillment orders) is computed by the backend, not modelled in the
        // local overlay. We confirm the order exists upstream below, acknowledge
        // the cancel, and leave it unstaged so the downstream `order` read forwards
        // to the backend for the real refunded/restocked state instead of serving
        // a stale locally-projected copy.
        if !order_id.contains(SYNTHETIC_MARKER) && !order_locally_known && !refund_method_cancel {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        let error_payload = |field_name: &str, message: &str, code: &str| {
            let error = user_error([field_name], message, Some(code));
            json!({
                "order": Value::Null,
                "job": Value::Null,
                "orderCancelUserErrors": [error.clone()],
                "userErrors": [error]
            })
        };
        if let Some(staff_note) = resolved_string_field(&field.arguments, "staffNote") {
            if staff_note.chars().count() > 255 {
                return Some(selected_json(
                    &error_payload(
                        "staffNote",
                        "Staff note is too long. Maximum length is 255 characters.",
                        "INVALID",
                    ),
                    &field.selection,
                ));
            }
        }
        if refund_present && refund_method_cancel {
            return Some(selected_json(
                &error_payload(
                    "refund",
                    "Only one of the arguments `refund` or `refund_method` is allowed.",
                    "INVALID",
                ),
                &field.selection,
            ));
        }

        // refundMethod cancel of an order not held in local overlay state: confirm
        // it exists upstream, acknowledge the cancel, and leave it unstaged so the
        // downstream order read forwards to the backend for the authoritative
        // refunded/restocked projection (see the staging note above).
        if refund_method_cancel && !order_locally_known {
            if !self.order_exists_upstream(request, &order_id) {
                return Some(selected_json(
                    &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                    &field.selection,
                ));
            }
            let job_id = synthetic_shopify_gid("Job", self.log_entries.len() + 1);
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "orderCancel",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "forwarded",
                    notes: "Acknowledged refundMethod orderCancel; downstream order read forwards upstream for the refunded/restocked projection.",
                },
            });
            return Some(selected_json(
                &json!({
                    "order": Value::Null,
                    "job": { "id": job_id, "done": false },
                    "orderCancelUserErrors": [],
                    "userErrors": []
                }),
                &field.selection,
            ));
        }

        if self.store.staged.orders.contains_key(&order_id) {
            let already_cancelled = self
                .store
                .staged
                .orders
                .get(&order_id)
                .and_then(|order| order.get("cancelledAt"))
                .is_some_and(|cancelled_at| !cancelled_at.is_null());
            if already_cancelled {
                return Some(selected_json(
                    &error_payload(
                        "orderId",
                        "Cannot cancel an order that has already been canceled",
                        "INVALID",
                    ),
                    &field.selection,
                ));
            }

            let reason =
                resolved_string_field(&field.arguments, "reason").unwrap_or_else(|| "OTHER".into());
            let timestamp = self.order_cancel_timestamp();
            let job_id = synthetic_shopify_gid("Job", self.log_entries.len() + 1);
            let order = self
                .store
                .staged
                .orders
                .get_mut(&order_id)
                .expect("staged order existence was checked before mutation");
            order["closed"] = json!(true);
            order["closedAt"] = json!(timestamp.clone());
            order["cancelledAt"] = json!(timestamp);
            order["cancelReason"] = json!(reason);
            order["updatedAt"] = order["cancelledAt"].clone();
            let order = order.clone();
            if let Some(customer_id) = order["customer"]["id"].as_str() {
                if let Some(customer_orders) =
                    self.store.staged.customer_orders.get_mut(customer_id)
                {
                    for customer_order in customer_orders {
                        if customer_order["id"].as_str() == Some(order_id.as_str()) {
                            *customer_order = order.clone();
                        }
                    }
                }
            }
            self.record_staged_orders_log_entry(
                request,
                query,
                variables,
                "orderCancel",
                vec![order_id],
            );
            return Some(selected_json(
                &json!({
                    "order": order,
                    "job": { "id": job_id, "done": false },
                    "orderCancelUserErrors": [],
                    "userErrors": []
                }),
                &field.selection,
            ));
        }

        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return Some(selected_json(
                &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                &field.selection,
            ));
        };
        if self
            .store
            .staged
            .order_customer_cancelled_ids
            .contains(&order_id)
        {
            return Some(selected_json(
                &error_payload(
                    "orderId",
                    "Cannot cancel an order that has already been canceled",
                    "INVALID",
                ),
                &field.selection,
            ));
        }
        self.store
            .staged
            .order_customer_cancelled_ids
            .insert(order_id.clone());
        let reason =
            resolved_string_field(&field.arguments, "reason").unwrap_or_else(|| "OTHER".into());
        let timestamp = self.order_cancel_timestamp();
        order["closed"] = json!(true);
        order["closedAt"] = json!(timestamp.clone());
        order["cancelledAt"] = json!(timestamp);
        order["cancelReason"] = json!(reason);
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        self.record_staged_orders_log_entry(
            request,
            query,
            variables,
            "orderCancel",
            vec![order_id.clone()],
        );
        let job_id = self.next_proxy_synthetic_gid("Job");
        Some(selected_json(
            &json!({
                "order": order,
                "job": { "id": job_id, "done": false },
                "orderCancelUserErrors": [],
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(super) fn order_cancel_timestamp(&self) -> String {
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            (self.log_entries.len() + 1) % 60
        )
    }

    pub(in crate::proxy) fn order_customer_set_error_paths(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_field(&field.arguments, "orderId").unwrap_or_default();
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        // Earn order + customer from the backend on the happy path (no seed).
        // Synthetic error-path ids stay local-only.
        if !order_id.is_empty()
            && !order_id.contains(SYNTHETIC_MARKER)
            && !self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id)
            && !self.store.staged.orders.contains_key(&order_id)
        {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        if !customer_id.is_empty() && !customer_id.contains(SYNTHETIC_MARKER) {
            self.ensure_order_customer_hydrated(request, &customer_id);
        }
        let customer = self.store.staged.customers.get(&customer_id).cloned();
        let from_customer_map = self
            .store
            .staged
            .order_customer_orders
            .contains_key(&order_id);
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
            .or_else(|| self.store.staged.orders.get(&order_id).cloned())
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        let Some(customer) = customer else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["customerId"], "Customer does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        if self.order_customer_order_is_b2b(&order_id, &order)
            && self.order_customer_customer_is_b2b_contact_for_order(&customer_id, &order)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(
                        ["customerId"],
                        "Customer does not have the permissions to place this order",
                        Some("NOT_PERMITTED"),
                    )]
                }),
                &field.selection,
            );
        }
        order["customer"] = customer;
        if from_customer_map {
            self.store
                .staged
                .order_customer_orders
                .insert(order_id.clone(), order.clone());
        } else {
            self.store
                .staged
                .orders
                .insert(order_id.clone(), order.clone());
        }
        // Maintain the per-customer order index so the b2b `customer.orders`
        // connection reflects the association immediately (read-after-write):
        // detach the order from any prior owner, then attach the full (now
        // customer-bearing) order node to the new customer.
        self.detach_order_from_customer_orders(&order_id);
        self.store
            .staged
            .customer_orders
            .entry(customer_id.clone())
            .or_default()
            .push(order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    /// Remove an order from every per-customer order index entry. Used when an
    /// order's customer association changes (set to a new owner / removed) so a
    /// later `customer.orders` read does not surface a stale link.
    pub(super) fn detach_order_from_customer_orders(&mut self, order_id: &str) {
        for orders in self.store.staged.customer_orders.values_mut() {
            orders.retain(|order| order.get("id").and_then(Value::as_str) != Some(order_id));
        }
    }

    pub(in crate::proxy) fn order_customer_remove_error_paths(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_field(&field.arguments, "orderId").unwrap_or_default();
        if !order_id.is_empty()
            && !order_id.contains(SYNTHETIC_MARKER)
            && !self
                .store
                .staged
                .order_customer_orders
                .contains_key(&order_id)
            && !self.store.staged.orders.contains_key(&order_id)
        {
            self.ensure_order_lifecycle_hydrated(request, &order_id);
        }
        let from_customer_map = self
            .store
            .staged
            .order_customer_orders
            .contains_key(&order_id);
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
            .or_else(|| self.store.staged.orders.get(&order_id).cloned())
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(["orderId"], "Order does not exist", Some("NOT_FOUND"))]
                }),
                &field.selection,
            );
        };
        if self.order_customer_order_is_b2b(&order_id, &order) {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [user_error(
                        ["orderId"],
                        "Action not permitted on B2B Orders",
                        Some("INVALID"),
                    )]
                }),
                &field.selection,
            );
        }
        order["customer"] = Value::Null;
        if from_customer_map {
            self.store
                .staged
                .order_customer_orders
                .insert(order_id.clone(), order.clone());
        } else {
            self.store
                .staged
                .orders
                .insert(order_id.clone(), order.clone());
        }
        // The order is no longer attached to any customer: drop it from every
        // per-customer order index entry so `customer.orders` reads reflect the
        // removal.
        self.detach_order_from_customer_orders(&order_id);
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn order_customer_order_is_b2b(&self, order_id: &str, order: &Value) -> bool {
        self.store
            .staged
            .order_customer_b2b_order_ids
            .contains(order_id)
            || order_customer_purchasing_entity_is_b2b(&order["purchasingEntity"])
    }

    fn order_customer_customer_is_b2b_contact_for_order(
        &self,
        customer_id: &str,
        order: &Value,
    ) -> bool {
        if self
            .store
            .staged
            .order_customer_contact_customer_ids
            .contains(customer_id)
        {
            return true;
        }
        let company_ids = order_customer_purchasing_entity_company_ids(&order["purchasingEntity"]);
        self.store.staged.b2b_contacts.values().any(|contact| {
            contact["customerId"].as_str() == Some(customer_id)
                && contact["companyId"].as_str().is_some_and(|company_id| {
                    company_ids.is_empty() || company_ids.iter().any(|id| id == company_id)
                })
        })
    }
}

fn order_customer_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    email: &str,
) -> String {
    let name = [first_name, last_name]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        email.to_string()
    } else {
        name
    }
}

fn order_customer_purchasing_entity_company_ids(entity: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    order_customer_collect_purchasing_entity_company_ids(entity, &mut ids);
    ids.sort();
    ids.dedup();
    ids
}

fn order_customer_collect_purchasing_entity_company_ids(entity: &Value, ids: &mut Vec<String>) {
    match entity {
        Value::Object(map) => {
            if let Some(company_id) = map.get("companyId").and_then(Value::as_str) {
                ids.push(company_id.to_string());
            }
            if let Some(company_id) = map
                .get("company")
                .and_then(|company| company.get("id"))
                .and_then(Value::as_str)
            {
                ids.push(company_id.to_string());
            }
            for value in map.values() {
                order_customer_collect_purchasing_entity_company_ids(value, ids);
            }
        }
        Value::Array(items) => {
            for item in items {
                order_customer_collect_purchasing_entity_company_ids(item, ids);
            }
        }
        _ => {}
    }
}

pub(super) fn order_customer_purchasing_entity_is_b2b(entity: &Value) -> bool {
    match entity {
        Value::Object(map) => {
            map.get("purchasingCompany")
                .is_some_and(|purchasing_company| !purchasing_company.is_null())
                || map
                    .get("company")
                    .and_then(|company| company.get("id"))
                    .is_some_and(Value::is_string)
                || map.get("companyId").is_some_and(Value::is_string)
                || map.get("companyLocationId").is_some_and(Value::is_string)
                || map.values().any(order_customer_purchasing_entity_is_b2b)
        }
        Value::Array(items) => items.iter().any(order_customer_purchasing_entity_is_b2b),
        _ => false,
    }
}

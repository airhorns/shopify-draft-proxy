use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn shipping_settings_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot
            && self.store.staged.observed_shipping_locations.is_empty()
            && self.store.staged.carrier_services.is_empty()
        {
            let response = (self.upstream_transport)(request.clone());
            self.observe_shipping_settings_response(&response);
            return response;
        }
        ok_json(json!({ "data": self.shipping_settings_read_data(fields) }))
    }

    fn shipping_settings_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = self.delivery_profile_locations_read_data(fields);
        if let Value::Object(data) = &mut data {
            for field in fields {
                if field.name == "availableCarrierServices" {
                    data.insert(
                        field.response_key.clone(),
                        self.available_carrier_services_json(&field.selection),
                    );
                }
            }
        }
        data
    }

    fn available_carrier_services_json(&self, selection: &[SelectedField]) -> Value {
        Value::Array(
            self.store
                .staged
                .carrier_services
                .values()
                .map(|carrier| {
                    selected_json(
                        &json!({
                            "carrierService": carrier
                        }),
                        selection,
                    )
                })
                .collect(),
        )
    }

    fn observe_shipping_settings_response(&mut self, response: &Response) {
        self.observe_delivery_profile_locations_response(response);
        if let Some(services) = response.body["data"]["availableCarrierServices"].as_array() {
            for service_entry in services {
                if let Some(carrier) = service_entry.get("carrierService") {
                    self.stage_observed_carrier_service(carrier.clone());
                }
            }
        }
    }

    fn stage_observed_carrier_service(&mut self, carrier: Value) {
        let Some(id) = carrier
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.carrier_services.insert(id, carrier);
    }

    pub(in crate::proxy) fn fulfillment_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut handled = false;
        let data = root_payload_json(fields, |field| match field.name.as_str() {
            "fulfillmentService" => {
                handled = true;
                let value = field
                    .arguments
                    .get("id")
                    .and_then(resolved_value_string)
                    .and_then(|id| {
                        if self.store.staged.fulfillment_services.is_tombstoned(&id) {
                            None
                        } else {
                            self.store.staged.fulfillment_services.get(&id).cloned()
                        }
                    })
                    .map(|service| selected_json(&service, &field.selection))
                    .unwrap_or(Value::Null);
                Some(value)
            }
            "location" => {
                let id = field.arguments.get("id").and_then(resolved_value_string)?;
                if self
                    .store
                    .staged
                    .fulfillment_service_locations
                    .is_tombstoned(&id)
                {
                    handled = true;
                    Some(Value::Null)
                } else if let Some(location) =
                    self.store.staged.fulfillment_service_locations.get(&id)
                {
                    handled = true;
                    Some(selected_json(location, &field.selection))
                } else {
                    None
                }
            }
            _ => None,
        });
        handled.then_some(data)
    }

    pub(in crate::proxy) fn fulfillment_service_name_or_handle_exists(
        &self,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let normalized_name = name.trim().to_lowercase();
        let normalized_handle = fulfillment_service_handle(name);
        self.store
            .staged
            .fulfillment_services
            .iter()
            .filter(|(id, _)| except_id != Some(id.as_str()))
            .any(|(_, service)| {
                service
                    .get("serviceName")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized_name))
                    || service
                        .get("handle")
                        .and_then(Value::as_str)
                        .is_some_and(|handle| handle == normalized_handle)
            })
    }

    pub(in crate::proxy) fn fulfillment_service_callback_url_error(
        &self,
        callback_url: Option<&str>,
    ) -> Option<Value> {
        let callback_url = callback_url?;
        let parsed = match url::Url::parse(callback_url) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Some(user_error_omit_code(
                    ["callbackUrl"],
                    "Callback url is not allowed",
                    None,
                ));
            }
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return Some(user_error_omit_code(
                ["callbackUrl"],
                &format!(
                    "Callback url protocol {}:// is not supported",
                    parsed.scheme()
                ),
                None,
            ));
        }
        let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
            return Some(user_error_omit_code(
                ["callbackUrl"],
                "Callback url is not allowed",
                None,
            ));
        };
        if fulfillment_service_callback_url_host_is_allowed(
            &host,
            &self.config.shopify_admin_origin,
        ) {
            None
        } else {
            Some(user_error_omit_code(
                ["callbackUrl"],
                "Callback url is not allowed",
                None,
            ))
        }
    }

    fn fulfillment_service_validation_errors(
        &self,
        name: &str,
        callback_url: Option<&str>,
        except_id: Option<&str>,
        validate_name_shape: bool,
    ) -> Vec<Value> {
        let mut user_errors = Vec::new();
        if validate_name_shape {
            user_errors.extend(fulfillment_service_name_user_errors(name));
        }
        if let Some(error) = self.fulfillment_service_callback_url_error(callback_url) {
            user_errors.push(error);
        }
        if fulfillment_service_name_is_reserved(name) {
            user_errors.push(user_error_omit_code(["name"], "Name is reserved", None));
        } else if self.fulfillment_service_name_or_handle_exists(name, except_id) {
            user_errors.push(user_error_omit_code(
                ["name"],
                "Name has already been taken",
                None,
            ));
        }
        user_errors
    }

    pub(in crate::proxy) fn fulfillment_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Invalid fulfillment service mutation");
        };
        let data = root_payload_json(&fields, |field| {
            let (payload, ids) = match field.name.as_str() {
                "fulfillmentServiceCreate" => self.fulfillment_service_create_payload(field),
                "fulfillmentServiceUpdate" => self.fulfillment_service_update_payload(field),
                "fulfillmentServiceDelete" => self.fulfillment_service_delete_payload(field),
                _ => return None,
            };
            if !ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, ids);
            }
            Some(payload)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            json_error(
                501,
                &format!("Unsupported fulfillment service mutation {root_field}"),
            )
        } else {
            ok_json(json!({ "data": data }))
        }
    }

    pub(in crate::proxy) fn fulfillment_service_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let callback_url = field
            .arguments
            .get("callbackUrl")
            .and_then(resolved_value_string);
        let user_errors =
            self.fulfillment_service_validation_errors(&name, callback_url.as_deref(), None, true);
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    user_errors,
                ),
                vec![],
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let requires_shipping_method = if field.arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or(false)
        } else {
            true
        };
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(&field.arguments, "inventoryManagement").unwrap_or(false),
            requires_shipping_method,
        );
        let location = service["location"].clone();
        self.store
            .staged
            .fulfillment_services
            .insert(service_id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id.clone(), location);
        self.store
            .staged
            .fulfillment_services
            .tombstones
            .remove(&service_id);
        self.store
            .staged
            .fulfillment_service_locations
            .tombstones
            .remove(&location_id);
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![service_id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let Some(id) = field.arguments.get("id").and_then(resolved_value_string) else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let Some(existing) = self.store.staged.fulfillment_services.get(&id).cloned() else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_value_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        let callback_url = if field.arguments.contains_key("callbackUrl") {
            field
                .arguments
                .get("callbackUrl")
                .and_then(resolved_value_string)
        } else {
            existing
                .get("callbackUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let user_errors = self.fulfillment_service_validation_errors(
            &name,
            callback_url.as_deref(),
            Some(&id),
            field.arguments.contains_key("name"),
        );
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    user_errors,
                ),
                vec![],
            );
        }
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let requires_shipping_method = if field.arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or_else(|| {
                existing["requiresShippingMethod"]
                    .as_bool()
                    .unwrap_or(false)
            })
        } else {
            true
        };
        let mut service = fulfillment_service_record(
            &id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(&field.arguments, "inventoryManagement")
                .unwrap_or_else(|| existing["inventoryManagement"].as_bool().unwrap_or(false)),
            requires_shipping_method,
        );
        if let Some(handle) = existing.get("handle").and_then(Value::as_str) {
            service["handle"] = json!(handle);
        }
        self.store
            .staged
            .fulfillment_services
            .insert(id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id, service["location"].clone());
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let inventory_action = field
            .arguments
            .get("inventoryAction")
            .and_then(resolved_value_string);
        let destination_location_id = field
            .arguments
            .get("destinationLocationId")
            .and_then(resolved_value_string)
            .filter(|value| !value.trim().is_empty());
        if !self.store.staged.fulfillment_services.contains_key(&id) {
            return (
                fulfillment_service_delete_payload(
                    Value::Null,
                    &field.selection,
                    vec![user_error_omit_code(
                        ["id"],
                        "Fulfillment service could not be found.",
                        None,
                    )],
                ),
                vec![],
            );
        }
        // KEEP/DELETE must not carry a destination location; TRANSFER must name a real one.
        match inventory_action.as_deref() {
            Some("KEEP") | Some("DELETE") if destination_location_id.is_some() => {
                return (
                    fulfillment_service_delete_payload(
                        Value::Null,
                        &field.selection,
                        vec![user_error_omit_code(["inventoryAction"], "Inventory action Destination location id should not be present when deleting/keeping the inventory of the fulfillment service.", None)],
                    ),
                    vec![],
                );
            }
            Some("TRANSFER") => {
                if let Some(destination) = destination_location_id.as_ref() {
                    if !self.store.staged.locations.contains_key(destination) {
                        return (
                            fulfillment_service_delete_payload(
                                Value::Null,
                                &field.selection,
                                vec![user_error_omit_code(
                                    Value::Null,
                                    "Invalid destination location.",
                                    None,
                                )],
                            ),
                            vec![],
                        );
                    }
                }
            }
            _ => {}
        }
        let service = self
            .store
            .staged
            .fulfillment_services
            .remove(&id)
            .expect("fulfillment service existence checked above");
        let location_id = service["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .fulfillment_service_locations
            .remove(&location_id);
        self.store.staged.fulfillment_services.tombstone(id.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .tombstone(location_id);
        (
            fulfillment_service_delete_payload(
                json!(id.replace("?id=true", "")),
                &field.selection,
                vec![],
            ),
            vec![id],
        )
    }

    pub(in crate::proxy) fn carrier_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "carrierService" => self.carrier_service_detail_field(field),
                "carrierServices" => self.carrier_services_connection_field(field),
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn carrier_service_detail_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = field.arguments.get("id").and_then(resolved_value_string) else {
            return Value::Null;
        };
        if self.store.staged.carrier_services.is_tombstoned(&id) {
            return Value::Null;
        }
        self.store
            .staged
            .carrier_services
            .get(&id)
            .map(|carrier| selected_json(carrier, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn carrier_services_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let query = field.arguments.get("query").and_then(resolved_value_string);
        let active_filter = match query.as_deref() {
            Some("active:true") => Some(true),
            Some("active:false") => Some(false),
            _ => None,
        };
        let mut services: Vec<Value> = self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.carrier_services.is_tombstoned(id))
            .map(|(_, carrier)| carrier.clone())
            .filter(|carrier| {
                active_filter
                    .map(|expected| carrier.get("active") == Some(&json!(expected)))
                    .unwrap_or(true)
            })
            .collect();
        services.sort_by_key(|carrier| {
            carrier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        selected_connection_json_with_args(
            services,
            &field.arguments,
            &field.selection,
            carrier_service_cursor,
        )
    }

    pub(in crate::proxy) fn carrier_service_mutations(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        for field in &fields {
            if field.name == "carrierServiceCreate" {
                if let Some(error) =
                    carrier_service_create_callback_url_coercion_error(query, field)
                {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }
        let data = root_payload_json(&fields, |field| {
            let payload = match field.name.as_str() {
                "carrierServiceCreate" => {
                    self.carrier_service_create_field(field, query, variables, request)
                }
                "carrierServiceUpdate" => {
                    self.carrier_service_update_field(field, query, variables, request)
                }
                "carrierServiceDelete" => {
                    self.carrier_service_delete_field(field, query, variables, request)
                }
                _ => return None,
            };
            Some(payload)
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn carrier_service_create_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_CREATE_FAILED",
                )],
            );
        };
        if let Some(error) = resolved_string_field(&input, "callbackUrl").and_then(|callback_url| {
            carrier_service_callback_url_error(&callback_url, "CARRIER_SERVICE_CREATE_FAILED")
        }) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![error],
            );
        }
        // A carrier service name is unique per app/shop: a second create with the same
        // (trimmed) name returns a base CARRIER_SERVICE_CREATE_FAILED userError naming the
        // already-configured service and stages no additional record.
        let trimmed_name = name.trim();
        if self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.carrier_services.is_tombstoned(id))
            .any(|(_, carrier)| {
                carrier.get("name").and_then(Value::as_str).map(str::trim) == Some(trimmed_name)
            })
        {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    &format!("{trimmed_name} is already configured"),
                    "CARRIER_SERVICE_CREATE_FAILED",
                )],
            );
        }
        let id = self.next_proxy_synthetic_gid("DeliveryCarrierService");
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl"),
            resolved_bool_field(&input, "active").unwrap_or(false),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or(false),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceCreate", vec![id]);
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_update_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(id) = resolved_string_field(&input, "id") else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        let Some(existing) = self.store.staged.carrier_services.get(&id).cloned() else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        if matches!(
            resolved_string_field(&input, "name").as_deref(),
            Some(name) if name.trim().is_empty()
        ) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_UPDATE_FAILED",
                )],
            );
        }
        let existing_callback_url = existing
            .get("callbackUrl")
            .and_then(Value::as_str)
            .map(str::to_string);
        let input_callback_url = resolved_string_field(&input, "callbackUrl");
        if input_callback_url.as_deref() != existing_callback_url.as_deref() {
            if let Some(error) = input_callback_url.as_ref().and_then(|callback_url| {
                carrier_service_callback_url_error(callback_url, "CARRIER_SERVICE_UPDATE_FAILED")
            }) {
                return carrier_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &carrier_selection,
                    vec![error],
                );
            }
        }
        let name = resolved_string_field(&input, "name")
            .or_else(|| {
                existing
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let carrier = carrier_service_record(
            &id,
            &name,
            input_callback_url.or(existing_callback_url),
            resolved_bool_field(&input, "active").unwrap_or_else(|| {
                existing
                    .get("active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or_else(|| {
                existing
                    .get("supportsServiceDiscovery")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceUpdate", vec![id]);
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_delete_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        if !self.store.staged.carrier_services.contains_key(&id) {
            return carrier_service_delete_payload(
                Value::Null,
                &field.selection,
                vec![carrier_service_user_error(
                    json!(["id"]),
                    "The carrier or app could not be found.",
                    "CARRIER_SERVICE_DELETE_FAILED",
                )],
            );
        }
        self.store.staged.carrier_services.remove(&id);
        self.store.staged.carrier_services.tombstone(id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "carrierServiceDelete",
            vec![id.clone()],
        );
        carrier_service_delete_payload(json!(id), &field.selection, vec![])
    }

    pub(in crate::proxy) fn shipping_package_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), BTreeMap::new()));
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return ok_json(
                json!({ "data": { response_key: { "userErrors": [user_error_omit_code(["id"], "ID is required", None)] } } }),
            );
        };
        let id = id.clone();
        let payload = match root_field {
            "shippingPackageUpdate" => {
                let Some(ResolvedValue::Object(input)) = arguments.get("shippingPackage") else {
                    return ok_json(
                        json!({ "data": { response_key: { "userErrors": [user_error_omit_code(["shippingPackage"], "Shipping package input is required", None)] } } }),
                    );
                };
                let Some(mut package) = self.effective_shipping_package(&id) else {
                    return shipping_package_not_found_response(root_field, &response_key, &id);
                };
                if package.get("boxType") == Some(&json!("FLAT_RATE")) {
                    return ok_json(json!({
                        "data": {
                            response_key: {
                                "userErrors": [user_error(["shippingPackage"], "Custom shipping box is not updatable", Some("CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"))]
                            }
                        }
                    }));
                }
                let was_default = package.get("default") == Some(&json!(true));
                merge_shipping_package_input(&mut package, input);
                if !was_default && package.get("default") == Some(&json!(true)) {
                    self.clear_default_shipping_packages_except(&id);
                }
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageMakeDefault" => {
                let Some(mut package) = self.effective_shipping_package(&id) else {
                    return shipping_package_not_found_response(root_field, &response_key, &id);
                };
                self.clear_default_shipping_packages_except(&id);
                package["default"] = json!(true);
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageDelete" => {
                if self.effective_shipping_package(&id).is_none() {
                    return shipping_package_not_found_response(root_field, &response_key, &id);
                }
                self.store.staged.shipping_packages.remove(&id);
                self.store.staged.shipping_packages.tombstone(id.clone());
                json!({ "deletedId": id, "userErrors": [] })
            }
            _ => unreachable!("shipping package dispatcher only receives supported roots"),
        };

        self.record_shipping_package_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({ "data": { response_key: payload } }))
    }

    pub(in crate::proxy) fn effective_shipping_package(&self, id: &str) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("ShippingPackage") {
            return None;
        }
        self.store.staged.shipping_packages.get(id).cloned()
    }

    pub(in crate::proxy) fn clear_default_shipping_packages_except(&mut self, default_id: &str) {
        let package_ids: Vec<String> = self
            .store
            .staged
            .shipping_packages
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        for id in package_ids {
            if id == default_id || self.store.staged.shipping_packages.is_tombstoned(&id) {
                continue;
            }
            let Some(mut package) = self.effective_shipping_package(&id) else {
                continue;
            };
            package["default"] = json!(false);
            package["updatedAt"] = json!(self.next_shipping_package_timestamp());
            self.store.staged.shipping_packages.insert(id, package);
        }
    }

    pub(in crate::proxy) fn next_shipping_package_timestamp(&self) -> String {
        let staged_shipping_mutations = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("operationName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "shippingPackageUpdate"
                                | "shippingPackageMakeDefault"
                                | "shippingPackageDelete"
                        )
                    })
            })
            .count();
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            staged_shipping_mutations * 2 + 1
        )
    }

    pub(in crate::proxy) fn record_shipping_package_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": [root_field],
                "primaryRootField": root_field
            }
        }));
    }
}

fn shipping_package_not_found_response(root_field: &str, response_key: &str, id: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Invalid id: {id}"),
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [root_field]
        }],
        "data": { response_key: null }
    }))
}

fn fulfillment_service_name_user_errors(name: &str) -> Vec<Value> {
    if name.trim().is_empty() {
        vec![user_error_omit_code(["name"], "Name can't be blank", None)]
    } else {
        fulfillment_service_name_whitespace_errors(name)
    }
}

use super::*;

const LOCATION_HYDRATE_QUERY: &str = r#"query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"#;
const LOCATION_LIMIT_STATUS_QUERY: &str = r#"query StorePropertiesLocationLimitStatus($first: Int!) { shop { resourceLimits { locationLimit } } locations(first: $first, includeInactive: true) { nodes { id isActive isFulfillmentService } pageInfo { hasNextPage } } }"#;

impl DraftProxy {
    pub(in crate::proxy) fn location_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "locationAdd" => self.location_add(query, variables, request),
            "locationEdit" => self.location_edit(query, variables, request),
            "locationActivate" => self.location_activate(query, variables, request),
            "locationDelete" => self.location_delete(query, variables, request),
            _ => json_error(501, "Unsupported location mutation"),
        }
    }

    pub(in crate::proxy) fn location_local_pickup_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let data = root_payload_json(&fields, |field| {
            let payload = match field.name.as_str() {
                "locationLocalPickupEnable" => {
                    self.location_local_pickup_enable_payload(field, request, query, variables)
                }
                "locationLocalPickupDisable" => {
                    self.location_local_pickup_disable_payload(field, request, query, variables)
                }
                _ => return None,
            };
            Some(payload)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            return json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {}",
                    root_field
                ),
            );
        }
        ok_json(json!({ "data": data }))
    }

    fn location_local_pickup_enable_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "localPickupSettings")
            .unwrap_or_else(|| field.arguments.clone());
        let location_id = resolved_string_field(&input, "locationId").unwrap_or_default();
        let pickup_time = resolved_string_field(&input, "pickupTime").unwrap_or_default();
        let user_errors = self.location_local_pickup_enable_user_errors(
            &location_id,
            &pickup_time,
            field.name.as_str(),
        );
        if !user_errors.is_empty() {
            return location_local_pickup_enable_payload_selected_json(
                Value::Null,
                &field.selection,
                user_errors,
            );
        }

        let instructions = input
            .get("instructions")
            .and_then(|value| match value {
                ResolvedValue::String(value) => Some(Value::String(value.clone())),
                ResolvedValue::Null => Some(Value::Null),
                _ => None,
            })
            .unwrap_or(Value::Null);
        let settings = json!({
            "pickupTime": pickup_time,
            "instructions": instructions
        });
        let mut location = self
            .active_local_pickup_location(&location_id)
            .unwrap_or_else(|| self.staged_location_record(&location_id));
        location["isActive"] = json!(true);
        location["isFulfillmentService"] = json!(false);
        location["localPickupSettingsV2"] = settings.clone();
        location["localPickupSettings"] = settings.clone();
        self.stage_local_pickup_location(location);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "locationLocalPickupEnable",
            vec![location_id],
        );

        location_local_pickup_enable_payload_selected_json(settings, &field.selection, Vec::new())
    }

    fn location_local_pickup_disable_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let location_id = resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
        let user_errors =
            self.location_local_pickup_location_user_errors(&location_id, field.name.as_str());
        if user_errors.is_empty() {
            let mut location = self
                .active_local_pickup_location(&location_id)
                .unwrap_or_else(|| self.staged_location_record(&location_id));
            location["isActive"] = json!(true);
            location["isFulfillmentService"] = json!(false);
            location["localPickupSettingsV2"] = Value::Null;
            location["localPickupSettings"] = Value::Null;
            self.stage_local_pickup_location(location);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "locationLocalPickupDisable",
                vec![location_id.clone()],
            );
        }
        let payload_location_id = if user_errors.is_empty() {
            json!(location_id)
        } else {
            Value::Null
        };
        location_local_pickup_disable_payload_selected_json(
            payload_location_id,
            &field.selection,
            user_errors,
        )
    }

    fn location_local_pickup_enable_user_errors(
        &self,
        location_id: &str,
        pickup_time: &str,
        root_field: &str,
    ) -> Vec<Value> {
        let location_errors =
            self.location_local_pickup_location_user_errors(location_id, root_field);
        if !location_errors.is_empty() {
            return location_errors;
        }
        if !local_pickup_time_is_standard(pickup_time) {
            return vec![user_error(
                ["localPickupSettings"],
                "Custom pickup time is not allowed for local pickup settings.",
                Some("CUSTOM_PICKUP_TIME_NOT_ALLOWED"),
            )];
        }
        Vec::new()
    }

    fn location_local_pickup_location_user_errors(
        &self,
        location_id: &str,
        root_field: &str,
    ) -> Vec<Value> {
        if self.active_local_pickup_location(location_id).is_some() {
            return Vec::new();
        }
        let field_name = if root_field == "locationLocalPickupDisable" {
            "locationId"
        } else {
            "localPickupSettings"
        };
        vec![user_error_with_code_value(
            [field_name],
            &format!(
                "Unable to find an active location for location ID {}",
                resource_id_path_tail(location_id)
            ),
            json!("ACTIVE_LOCATION_NOT_FOUND"),
        )]
    }

    pub(in crate::proxy) fn location_add(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Unable to parse locationAdd mutation");
        };
        let mut data = serde_json::Map::new();
        for field in document
            .root_fields
            .iter()
            .filter(|field| field.name == "locationAdd")
        {
            let Some(input) = resolved_object_field(&field.arguments, "input") else {
                return ok_json(location_add_missing_input_error(
                    &document.operation_path,
                    field,
                ));
            };
            if let Some(error) =
                self.location_add_input_shape_error(&document.operation_path, field, &input)
            {
                return ok_json(error);
            }
            if resolved_object_list_field(&input, "metafields")
                .iter()
                .any(|metafield| {
                    metafield.contains_key("key")
                        && resolved_string_field(metafield, "key")
                            .map(|key| key.trim().is_empty())
                            .unwrap_or(true)
                })
            {
                return ok_json(location_add_metafield_blank_key_error(field, &document));
            }

            let user_errors = self.location_add_user_errors(&input, request);
            let location = if user_errors.is_empty() {
                let id = self.next_proxy_synthetic_gid("Location");
                let location = self.location_record_from_add_input(&id, &input);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(request, query, variables, "locationAdd", vec![id]);
                location
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                location_payload_selected_json(location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn location_activate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationActivate",
                query,
                variables,
            ));
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationActivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationActivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            self.ensure_location_hydrated(&location_id, request);
            self.hydrate_location_limit_status(request);
            let source_location = self.location_source_record(&location_id);
            let errors = self.location_activate_errors(&source_location);
            let location = if errors.is_empty() {
                let mut location = source_location;
                location["isActive"] = json!(true);
                location["activatable"] = json!(true);
                location["deactivatable"] = json!(true);
                location["deletable"] = json!(false);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationActivate",
                    vec![location_id.clone()],
                );
                location
            } else {
                source_location
            };
            data.insert(
                field.response_key,
                location_activate_payload_selected_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Applies a `locationDelete`. The target is resolved through the local overlay
    /// first, falling back to an upstream hydrate (live-hybrid only); unknown ids
    /// surface `LOCATION_NOT_FOUND`. On success the location is tombstoned (so
    /// later reads return null and the connection omits it) and its inventory
    /// levels are dropped.
    pub(in crate::proxy) fn location_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationDelete mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationDelete" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            let location = self
                .location_for_read(&location_id)
                .or_else(|| self.hydrate_location_for_mutation(request, &location_id));
            let errors = self.location_delete_errors(&location_id, location.as_ref());
            let deleted_location_id = if errors.is_empty() {
                self.delete_location_inventory_levels(&location_id);
                self.delete_staged_location(&location_id);
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationDelete",
                    vec![location_id.clone()],
                );
                Value::String(location_id)
            } else {
                Value::Null
            };
            data.insert(
                field.response_key,
                location_delete_payload_selected_json(
                    deleted_location_id,
                    &field.selection,
                    errors,
                ),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Resolves the user errors Shopify raises for a `locationDelete`, mirroring
    /// the public Admin API. For staged locations inventory presence is read from
    /// the local overlay; for hydrated baselines it falls back to the upstream
    /// `hasActiveInventory`/`deletable` fields.
    fn location_delete_errors(&self, location_id: &str, location: Option<&Value>) -> Vec<Value> {
        let Some(location) = location else {
            return vec![location_delete_user_error(
                "LOCATION_NOT_FOUND",
                "Location not found.",
            )];
        };
        if location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![location_delete_user_error(
                "LOCATION_NOT_FOUND",
                "Location not found.",
            )];
        }

        let mut errors = Vec::new();
        if location.get("isActive").and_then(Value::as_bool) == Some(true) {
            errors.push(location_delete_user_error(
                "LOCATION_IS_ACTIVE",
                "The location cannot be deleted while it is active.",
            ));
        }
        let has_inventory = if self.store.staged.locations.contains_key(location_id) {
            self.location_has_inventory(location_id)
        } else {
            location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id))
                || self.location_has_inventory(location_id)
        };
        if has_inventory {
            errors.push(location_delete_user_error(
                "LOCATION_HAS_INVENTORY",
                "The location cannot be deleted while it has inventory.",
            ));
        }
        if location
            .get("hasUnfulfilledOrders")
            .and_then(Value::as_bool)
            == Some(true)
        {
            errors.push(location_delete_user_error(
                "LOCATION_HAS_PENDING_ORDERS",
                "The location cannot be deleted while it has pending orders.",
            ));
        }
        if !self.store.staged.locations.contains_key(location_id)
            && location.get("deletable").and_then(Value::as_bool) == Some(false)
            && errors.is_empty()
        {
            errors.push(location_delete_user_error(
                "LOCATION_NOT_DELETABLE",
                "The location cannot be deleted.",
            ));
        }
        errors
    }

    fn delete_staged_location(&mut self, location_id: &str) {
        self.store.staged.locations.remove(location_id);
        self.store
            .staged
            .observed_shipping_locations
            .remove(location_id);
        self.store
            .staged
            .fulfillment_service_locations
            .remove(location_id);
        self.store
            .staged
            .locations
            .tombstone(location_id.to_string());
    }

    fn delete_location_inventory_levels(&mut self, location_id: &str) {
        let keys = self
            .store
            .staged
            .inventory_levels
            .keys()
            .filter(|(_, staged_location_id)| staged_location_id == location_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.store.staged.inventory_levels.remove(&key);
        }
        self.store
            .staged
            .inventory_level_order
            .retain(|(_, staged_location_id)| staged_location_id != location_id);
        self.store
            .staged
            .inventory_quantity_updated_at
            .retain(|(_, staged_location_id, _), _| staged_location_id != location_id);
    }

    /// Fetches a baseline location from upstream so an edit/delete on a location
    /// the proxy never staged can validate against its real state (live-hybrid
    /// only). Returns `None` under snapshot reads, for an empty id, or for a
    /// tombstoned location. On a 2xx with a `location` object the record is mirrored
    /// into the observed overlay and returned.
    fn hydrate_location_for_mutation(
        &mut self,
        request: &Request,
        location_id: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || location_id.is_empty()
            || self.store.staged.locations.is_tombstoned(location_id)
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCATION_HYDRATE_QUERY,
                "operationName": "StorePropertiesLocationHydrate",
                "variables": { "id": location_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let location = response.body["data"]["location"].clone();
        if !location.is_object() {
            return None;
        }
        self.stage_observed_shipping_location(location.clone());
        Some(location)
    }

    /// Applies a `locationEdit`. The target is resolved through the local overlay
    /// first; when it is not staged the proxy hydrates it from upstream (live-hybrid
    /// only) so edits to real baseline locations validate against their actual
    /// state, and unknown ids surface the "Location not found." user error. The
    /// merged record is re-staged so subsequent local reads observe the change.
    pub(in crate::proxy) fn location_edit(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationEdit mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationEdit" {
                continue;
            }
            let location_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
            if let Some(error) = self.location_edit_input_shape_error(&input) {
                return ok_json(error);
            }

            let source_location = self
                .location_for_read(&location_id)
                .or_else(|| self.hydrate_location_for_mutation(request, &location_id));
            let mut user_errors = Vec::new();
            if source_location.is_none() {
                user_errors.push(user_error_omit_code(["id"], "Location not found.", None));
            } else {
                user_errors.extend(self.location_edit_user_errors(&location_id, &input));
            }

            let location = if user_errors.is_empty() {
                let mut location =
                    source_location.unwrap_or_else(|| self.staged_location_record(&location_id));
                self.apply_location_edit_input(&mut location, &input);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationEdit",
                    vec![location_id.clone()],
                );
                location
            } else {
                Value::Null
            };

            data.insert(
                field.response_key,
                location_payload_selected_json(location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Surfaces the `LocationEditInput!` coercion error Shopify raises for an
    /// unknown `address.countryCode` before any staging happens, anchoring it at
    /// the variable definition like the live API.
    fn location_edit_input_shape_error(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if let Some(address) = resolved_object_field(input, "address") {
            if let Some(country_code) = resolved_string_field(&address, "countryCode") {
                if !location_country_code_is_valid(&country_code) {
                    return Some(location_edit_invalid_variable_error(
                        "address.countryCode",
                        &format!(
                            "Expected \"{}\" to be one of: {}",
                            country_code, LOCATION_COUNTRY_CODES
                        ),
                        input,
                    ));
                }
            }
        }
        None
    }

    fn apply_location_edit_input(
        &mut self,
        location: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let location_id = location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(name) = resolved_string_field(input, "name") {
            location["name"] = json!(name);
        }
        if let Some(is_active) =
            resolved_bool_field(input, "isActive").or_else(|| resolved_bool_field(input, "active"))
        {
            location["isActive"] = json!(is_active);
            location["deletable"] = json!(!is_active && !self.location_has_inventory(&location_id));
        }
        if let Some(fulfills_online_orders) = resolved_bool_field(input, "fulfillsOnlineOrders") {
            location["fulfillsOnlineOrders"] = json!(fulfills_online_orders);
        }
        if let Some(address_input) = resolved_object_field(input, "address") {
            let mut address = location
                .get("address")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !address.is_object() {
                address = json!({});
            }
            for field in [
                "address1",
                "address2",
                "city",
                "countryCode",
                "provinceCode",
                "zip",
            ] {
                if let Some(value) = resolved_string_field(&address_input, field) {
                    address[field] = json!(value);
                }
            }
            if let Some(country_code) = resolved_string_field(&address_input, "countryCode") {
                if let Some(country) = location_country_name(&country_code) {
                    address["country"] = json!(country);
                }
            }
            // Shopify derives the full province name from the effective
            // country + province codes whenever the address is edited. A
            // province-only edit (no countryCode in the input) still re-derives
            // the name from the country code already on the record.
            let effective_country_code = address
                .get("countryCode")
                .and_then(Value::as_str)
                .map(str::to_string);
            let effective_province_code = address
                .get("provinceCode")
                .and_then(Value::as_str)
                .filter(|code| !code.is_empty())
                .map(str::to_string);
            address["province"] = match (
                effective_country_code.as_deref(),
                effective_province_code.as_deref(),
            ) {
                (Some(country), Some(province)) => province_name_for_code(country, province)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            };
            location["address"] = address;
        }
        let metafields = self.location_metafields_from_input(&location_id, input);
        if !metafields.is_empty() {
            location["metafields"] = Value::Array(metafields);
        }
        location["hasActiveInventory"] = json!(self.location_has_inventory(&location_id));
        location["updatedAt"] = json!(self.next_product_timestamp());
    }

    /// Validates a `locationEdit` input against the staged record, mirroring the
    /// public Admin API's `locationEdit` user errors. Only fields present in the
    /// input are validated (edit inputs are sparse), and the name-uniqueness check
    /// excludes the location being edited.
    fn location_edit_user_errors(
        &self,
        location_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(name) = resolved_string_field(input, "name") {
            if let Some(error) = self.location_name_user_error(&name, Some(location_id)) {
                errors.push(error);
            }
        }
        errors.extend(location_address_length_user_errors(input, true));
        errors.extend(location_metafield_type_user_errors(input, 1));
        // Shopify refuses to disable online-order fulfillment on the last
        // location that still fulfills online orders.
        if resolved_bool_field(input, "fulfillsOnlineOrders") == Some(false)
            && !self.has_other_online_order_fulfillment_location(location_id)
        {
            errors.push(user_error(["input"], "Online order fulfillment could not be disabled for this location as it is the only location that fulfills online orders.", Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT")));
        }
        errors
    }

    fn location_name_exists_except(&self, name: &str, except_id: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.iter().any(|(id, location)| {
            id != except_id
                && location
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    fn location_name_user_error(&self, name: &str, except_id: Option<&str>) -> Option<Value> {
        if name.trim().is_empty() {
            return Some(user_error(
                ["input", "name"],
                "Add a location name",
                Some("BLANK"),
            ));
        }
        if name.chars().count() > 100 {
            return Some(user_error(
                ["input", "name"],
                "Use a shorter location name (up to 100 characters)",
                Some("TOO_LONG"),
            ));
        }
        let name_exists = match except_id {
            Some(except_id) => self.location_name_exists_except(name, except_id),
            None => self.location_name_exists(name),
        };
        name_exists.then(|| {
            user_error(
                ["input", "name"],
                "You already have a location with this name",
                Some("TAKEN"),
            )
        })
    }

    fn location_add_input_shape_error(
        &self,
        operation_path: &str,
        field: &RootFieldSelection,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if input.contains_key("capabilities") {
            return Some(location_add_invalid_variable_error(
                "capabilities",
                "Field is not defined on LocationAddInput",
                input,
            ));
        }
        if input.contains_key("capabilitiesToAdd") {
            return Some(location_add_inline_argument_not_accepted_error(
                operation_path,
                field,
                "capabilitiesToAdd",
            ));
        }
        let address = match input.get("address") {
            Some(ResolvedValue::Object(address)) => address,
            _ => {
                return Some(location_add_missing_address_error(operation_path, field));
            }
        };
        let country_code = resolved_string_field(address, "countryCode");
        let Some(country_code) = country_code else {
            if input_was_variable(field) {
                return Some(location_add_invalid_variable_error(
                    "address.countryCode",
                    "Expected value to not be null",
                    input,
                ));
            }
            return Some(location_add_missing_country_code_error(
                operation_path,
                field,
            ));
        };
        if !location_country_code_is_valid(&country_code) {
            return Some(location_add_invalid_variable_error(
                "address.countryCode",
                &format!(
                    "Expected \"{}\" to be one of: {}",
                    country_code, LOCATION_COUNTRY_CODES
                ),
                input,
            ));
        }
        None
    }

    fn location_add_user_errors(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let name = resolved_string_field(input, "name").unwrap_or_default();
        if let Some(error) = self.location_name_user_error(&name, None) {
            errors.push(error);
        }
        errors.extend(location_address_length_user_errors(input, false));
        errors.extend(location_metafield_type_user_errors(input, 0));
        self.hydrate_location_limit_status(request);
        if self.location_limit_reached() {
            errors.push(user_error(
                ["input"],
                "You have reached the maximum number of locations (200)",
                Some("INVALID"),
            ));
        }
        errors
    }

    fn location_record_from_add_input(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let address_input = resolved_object_field(input, "address").unwrap_or_default();
        let address = location_address_json(&address_input);
        let timestamp = self.next_product_timestamp();
        json!({
            "__typename": "Location",
            "id": id,
            "name": resolved_string_field(input, "name").unwrap_or_default(),
            "isActive": true,
            "activatable": false,
            "deactivatable": true,
            "deletable": false,
            "fulfillsOnlineOrders": resolved_bool_field(input, "fulfillsOnlineOrders").unwrap_or(true),
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": true,
            "address": address,
            "metafields": self.location_metafields_from_input(id, input),
            "createdAt": timestamp.clone(),
            "updatedAt": timestamp
        })
    }

    fn location_metafields_from_input(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        resolved_object_list_field(input, "metafields")
            .into_iter()
            .filter_map(|metafield| {
                let key = resolved_string_field(&metafield, "key").unwrap_or_default();
                if key.trim().is_empty() {
                    return None;
                }
                let value = resolved_string_field(&metafield, "value").unwrap_or_default();
                if value.is_empty() {
                    return None;
                }
                Some(json!({
                    "id": self.next_proxy_synthetic_gid("Metafield"),
                    "ownerId": owner_id,
                    "namespace": resolved_string_field(&metafield, "namespace").unwrap_or_else(|| "custom".to_string()),
                    "key": key,
                    "value": value,
                    "type": resolved_string_field(&metafield, "type").unwrap_or_else(|| "single_line_text_field".to_string())
                }))
            })
            .collect()
    }

    fn location_activate_errors(&self, location: &Value) -> Vec<Value> {
        if location
            .get("hasOngoingRelocation")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![user_error(["locationId"], "This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later.", Some("HAS_ONGOING_RELOCATION"))];
        }
        if location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![user_error(
                ["locationId"],
                "Location not found.",
                Some("LOCATION_NOT_FOUND"),
            )];
        }
        if self.location_limit_reached() {
            return vec![user_error(
                ["locationId"],
                "Shop has reached its location limit.",
                Some("LOCATION_LIMIT"),
            )];
        }
        if self.location_has_non_unique_active_name(location) {
            return vec![user_error(["locationId"], "This location currently cannot be activated because there exists an active location with the same name.", Some("HAS_NON_UNIQUE_NAME"))];
        }
        Vec::new()
    }

    fn location_has_non_unique_active_name(&self, location: &Value) -> bool {
        if location.get("isActive").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let Some(target_id) = location.get("id").and_then(Value::as_str) else {
            return false;
        };
        let Some(target_name) = location.get("name").and_then(Value::as_str) else {
            return false;
        };

        let mut location_ids = BTreeSet::new();
        for (id, _) in self.store.staged.locations.iter() {
            location_ids.insert(id.clone());
        }
        for id in self.store.staged.observed_shipping_locations.keys() {
            location_ids.insert(id.clone());
        }
        for (id, _) in self.store.staged.fulfillment_service_locations.iter() {
            location_ids.insert(id.clone());
        }

        location_ids.iter().any(|id| {
            if id == target_id {
                return false;
            }
            self.location_for_read(id).is_some_and(|candidate| {
                candidate.get("isActive").and_then(Value::as_bool) == Some(true)
                    && candidate.get("name").and_then(Value::as_str) == Some(target_name)
            })
        })
    }

    /// Hydrates a baseline location from upstream for lifecycle mutations
    /// (activate/deactivate) when it is neither already staged nor covered by a
    /// fixture-backed deactivation state-machine record. Issues the recorded
    /// `StorePropertiesLocationHydrate` query so the cassette replays the real
    /// captured location, letting the proxy preserve the baseline
    /// name/scope/state across the mutation instead of fabricating one. A miss
    /// (no recorded call) returns non-2xx and falls back to the default staged
    /// record for non-hydrate scenarios.
    pub(in crate::proxy) fn ensure_location_hydrated(
        &mut self,
        location_id: &str,
        request: &Request,
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        if self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
        {
            return;
        }
        if fixture_location_deactivate_state_machine_location(location_id).is_some() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCATION_HYDRATE_QUERY,
                "variables": { "id": location_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(node) = response
            .body
            .get("data")
            .and_then(|data| data.get("location"))
            .filter(|node| node.is_object())
        else {
            return;
        };
        let mut record = node.clone();
        if let Some(object) = record.as_object_mut() {
            object.insert("__typename".to_string(), json!("Location"));
        }
        if record.get("isFulfillmentService").and_then(Value::as_bool) == Some(true) {
            self.store
                .staged
                .fulfillment_service_locations
                .insert(location_id.to_string(), record);
        } else {
            self.stage_location(record);
        }
    }

    fn hydrate_location_limit_status(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid || self.store.staged.location_limit_reached
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCATION_LIMIT_STATUS_QUERY,
                "operationName": "StorePropertiesLocationLimitStatus",
                "variables": { "first": 250 }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if location_limit_reached_in_response(&response.body).unwrap_or(false) {
            self.store.staged.location_limit_reached = true;
        }
    }

    fn stage_location(&mut self, location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.locations.insert(id, location);
    }

    /// Stage a location whose local-pickup settings were just mutated. The
    /// canonical record lives in `staged.locations` (so direct `location(id:)`
    /// reads resolve it); when the same id was previously observed from an
    /// upstream `locationsAvailableForDeliveryProfilesConnection` response, the
    /// observed mirror is updated in lockstep so the connection read reflects the
    /// new settings too. `localPickupSettings` is kept in sync with the V2 field.
    fn stage_local_pickup_location(&mut self, mut location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        location["localPickupSettings"] = location
            .get("localPickupSettingsV2")
            .cloned()
            .unwrap_or(Value::Null);
        if self
            .store
            .staged
            .observed_shipping_locations
            .contains_key(&id)
        {
            self.store
                .staged
                .observed_shipping_locations
                .insert(id.clone(), location.clone());
        }
        self.stage_location(location);
    }

    pub(in crate::proxy) fn has_location_overlay_state(&self) -> bool {
        self.config.read_mode == ReadMode::Snapshot
            || !self.store.staged.locations.is_empty()
            || !self.store.staged.locations.order.is_empty()
            || !self.store.staged.locations.tombstones.is_empty()
            || !self.store.staged.fulfillment_service_locations.is_empty()
            || self.store.staged.location_limit_reached
    }

    /// True when a location read must consult the upstream baseline to answer.
    ///
    /// `location`, `locations`, and id-based `locationByIdentifier` reads resolve
    /// against the store's real locations, so without local overlay state they
    /// must pass through to upstream. `locationByIdentifier(customId:)` is
    /// resolved purely locally (the proxy intentionally does not model id-typed
    /// location metafield definitions and always reports the custom id as
    /// not found), so it never needs the baseline.
    pub(in crate::proxy) fn location_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "location" | "locations" => true,
            "locationByIdentifier" => resolved_object_field(&field.arguments, "identifier")
                .map(|identifier| !identifier.contains_key("customId"))
                .unwrap_or(true),
            _ => false,
        })
    }

    pub(in crate::proxy) fn location_read_response(
        &self,
        fields: &[RootFieldSelection],
    ) -> Response {
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "location" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "locationByIdentifier" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let id = resolved_string_field(&identifier, "id").unwrap_or_default();
                    let location = self
                        .location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection));
                    if location.is_none() && identifier.contains_key("customId") {
                        errors.push(json!({
                            "message": "Metafield definition of type 'id' is required when using custom ids.",
                            "path": [field.response_key.clone()],
                            "extensions": { "code": "NOT_FOUND" }
                        }));
                    }
                    location.unwrap_or(Value::Null)
                }
                "locations" => self.locations_connection_json(&field.arguments, &field.selection),
                _ => return None,
            })
        });
        let mut body = serde_json::Map::new();
        body.insert("data".to_string(), data);
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        ok_json(Value::Object(body))
    }

    pub(in crate::proxy) fn location_for_read(&self, location_id: &str) -> Option<Value> {
        if self.store.staged.locations.is_tombstoned(location_id) {
            return None;
        }
        self.store
            .staged
            .locations
            .get(location_id)
            .cloned()
            .or_else(|| {
                self.store
                    .staged
                    .observed_shipping_locations
                    .get(location_id)
                    .cloned()
            })
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
                    .cloned()
            })
            .or_else(|| fixture_location_deactivate_state_machine_location(location_id))
    }

    /// A location is eligible for local-pickup mutations only when it resolves
    /// to an active, non-fulfillment-service location (staged, observed, or
    /// fixture-backed). Unknown ids and inactive/fulfillment-service locations
    /// are filtered out so the caller can raise `ACTIVE_LOCATION_NOT_FOUND`.
    fn active_local_pickup_location(&self, location_id: &str) -> Option<Value> {
        self.location_for_read(location_id).filter(|location| {
            location
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(true)
                && !location
                    .get("isFulfillmentService")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
    }

    fn location_source_record(&self, location_id: &str) -> Value {
        self.location_for_read(location_id)
            .unwrap_or_else(|| self.staged_location_record(location_id))
    }

    fn locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let locations = self
            .store
            .staged
            .locations
            .order
            .iter()
            .filter(|id| !self.store.staged.locations.is_tombstoned(id))
            .filter_map(|id| self.store.staged.locations.get(id).cloned())
            .collect::<Vec<_>>();
        location_connection_json(locations, arguments, selections)
    }

    fn location_name_exists(&self, name: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.values().any(|location| {
            location
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    fn location_limit_reached(&self) -> bool {
        self.store.staged.location_limit_reached
            || self
                .store
                .staged
                .locations
                .values()
                .filter(|location| location.get("isActive").and_then(Value::as_bool) == Some(true))
                .count()
                >= 200
    }

    pub(in crate::proxy) fn location_deactivate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationDeactivate",
                query,
                variables,
            ));
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationDeactivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationDeactivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            let destination_location_id =
                resolved_string_field(&field.arguments, "destinationLocationId");
            self.ensure_location_hydrated(&location_id, request);
            let source_location = self.location_deactivate_source_location(&location_id);
            let errors = self
                .location_deactivate_errors(&source_location, destination_location_id.as_deref());
            let location = if errors.is_empty() {
                if let Some(destination_location_id) = destination_location_id.as_deref() {
                    self.relocate_inventory_levels_for_location(
                        &location_id,
                        destination_location_id,
                    );
                }
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationDeactivate",
                    vec![location_id.clone()],
                );
                let mut location = source_location;
                location["isActive"] = json!(false);
                location["hasActiveInventory"] = json!(false);
                location["deletable"] = json!(true);
                location["deactivatable"] = json!(true);
                self.stage_location(location.clone());
                location
            } else {
                source_location
            };
            data.insert(
                field.response_key,
                location_deactivate_payload_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn location_deactivate_errors(
        &self,
        source_location: &Value,
        destination_location_id: Option<&str>,
    ) -> Vec<Value> {
        let location_id = source_location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match destination_location_id {
            Some(destination_id) if destination_id == location_id => vec![user_error(["destinationLocationId"], "Location could not be deactivated because the destination location cannot be set to the location to be deactivated.", Some("DESTINATION_LOCATION_IS_THE_SAME_LOCATION"))],
            Some(destination_id)
                if destination_id.is_empty()
                    || self.location_deactivate_destination_is_inactive(destination_id) =>
            {
                vec![destination_location_not_found_or_inactive_error()]
            }
            Some(_) => Vec::new(),
            None if source_location
                .get("deactivatable")
                .and_then(Value::as_bool)
                == Some(false) =>
            {
                vec![user_error(["locationId"], "Location could not be deactivated because it either has a fulfillment service or is the only location with a shipping address.", Some("PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR"))]
            }
            None if source_location
                .get("fulfillsOnlineOrders")
                .and_then(Value::as_bool)
                == Some(true)
                && !self.has_other_online_order_fulfillment_location(location_id) =>
            {
                vec![user_error(["locationId"], "At least one location must fulfill online orders.", Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT"))]
            }
            None if source_location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id)) =>
            {
                vec![user_error(["locationId"], "Location could not be deactivated without specifying where to relocate inventory stocked at the location.", Some("HAS_ACTIVE_INVENTORY_ERROR"))]
            }
            None => Vec::new(),
        }
    }

    fn location_deactivate_source_location(&self, location_id: &str) -> Value {
        let mut location = self.location_source_record(location_id);
        let has_active_inventory = if self.store.staged.locations.contains_key(location_id) {
            self.location_has_inventory(location_id)
        } else {
            location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id))
                || self.location_has_inventory(location_id)
        };
        location["hasActiveInventory"] = json!(has_active_inventory);
        location
    }

    fn staged_location_record(&self, location_id: &str) -> Value {
        json!({
            "__typename": "Location",
            "id": location_id,
            "name": self.location_display_name(location_id),
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": self.location_has_inventory(location_id),
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "deletable": false,
            "shipsInventory": false,
            "address": {},
            "metafields": []
        })
    }

    fn location_display_name(&self, location_id: &str) -> String {
        if location_id.ends_with("/1") {
            "Source location".to_string()
        } else if location_id.ends_with("/2") {
            "Destination location".to_string()
        } else {
            "Location".to_string()
        }
    }

    fn location_deactivate_destination_is_inactive(&self, destination_id: &str) -> bool {
        self.location_for_read(destination_id)
            .and_then(|location| {
                location
                    .get("isActive")
                    .and_then(Value::as_bool)
                    .map(|is_active| !is_active)
            })
            .unwrap_or(false)
    }

    fn has_other_online_order_fulfillment_location(&self, location_id: &str) -> bool {
        self.store.staged.locations.iter().any(|(id, location)| {
            id != location_id
                && location
                    .get("fulfillsOnlineOrders")
                    .and_then(Value::as_bool)
                    == Some(true)
        }) || self
            .store
            .staged
            .fulfillment_service_locations
            .iter()
            .any(|(id, location)| {
                id != location_id
                    && location
                        .get("fulfillsOnlineOrders")
                        .and_then(Value::as_bool)
                        == Some(true)
            })
    }

    fn location_has_inventory(&self, location_id: &str) -> bool {
        self.store
            .staged
            .inventory_levels
            .iter()
            .any(|((_, staged_location_id), quantities)| {
                staged_location_id == location_id
                    && quantities.values().any(|quantity| *quantity > 0)
            })
    }

    fn relocate_inventory_levels_for_location(
        &mut self,
        source_location_id: &str,
        destination_location_id: &str,
    ) {
        let source_keys = self
            .store
            .staged
            .inventory_levels
            .keys()
            .filter(|(_, location_id)| location_id == source_location_id)
            .cloned()
            .collect::<Vec<_>>();
        for (inventory_item_id, source_location_id) in source_keys {
            let Some(source_quantities) = self
                .store
                .staged
                .inventory_levels
                .remove(&(inventory_item_id.clone(), source_location_id))
            else {
                continue;
            };
            let destination_quantities = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id, destination_location_id.to_string()))
                .or_default();
            for (name, quantity) in source_quantities {
                *destination_quantities.entry(name).or_insert(0) += quantity;
            }
        }
    }
}

pub(in crate::proxy) fn location_connection_json(
    mut locations: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
        locations.truncate(limit);
    }
    selected_typed_connection(
        &locations,
        selections,
        location_selected_json,
        value_id_cursor,
        |selections| selected_json(&empty_page_info(), selections),
    )
}

fn location_address_length_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    include_city: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(address) = resolved_object_field(input, "address") {
        if resolved_string_field(&address, "address1")
            .is_some_and(|address1| address1.chars().count() > 255)
        {
            errors.push(user_error(
                ["input", "address", "address1"],
                "Use a shorter name for the street (up to 255 characters)",
                Some("TOO_LONG"),
            ));
        }
        if include_city
            && resolved_string_field(&address, "city")
                .is_some_and(|city| city.chars().count() > 255)
        {
            errors.push(user_error(
                ["input", "address", "city"],
                "Use a shorter city name (up to 255 characters)",
                Some("TOO_LONG"),
            ));
        }
        if resolved_string_field(&address, "zip").is_some_and(|zip| zip.chars().count() > 255) {
            errors.push(user_error(
                ["input", "address", "zip"],
                "Use a shorter postal / ZIP code (up to 255 characters)",
                Some("TOO_LONG"),
            ));
        }
    }
    errors
}

fn location_metafield_type_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    index_base: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let invalid_type_message = format!(
        "Type must be one of the following: {}.",
        LOCATION_METAFIELD_VALID_TYPES.join(", ")
    );
    for (index, metafield) in resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
    {
        if let Some(metafield_type) = resolved_string_field(&metafield, "type") {
            if !LOCATION_METAFIELD_VALID_TYPES.contains(&metafield_type.as_str()) {
                errors.push(user_error(
                    json!([
                        "input",
                        "metafields",
                        (index + index_base).to_string(),
                        "type"
                    ]),
                    &invalid_type_message,
                    Some("INVALID_TYPE"),
                ));
            }
        }
    }
    errors
}

const LOCATION_COUNTRY_CODES: &str = "AF, AX, AL, DZ, AD, AO, AI, AG, AR, AM, AW, AC, AU, AT, AZ, BS, BH, BD, BB, BY, BE, BZ, BJ, BM, BT, BO, BA, BW, BV, BR, IO, BN, BG, BF, BI, KH, CA, CV, BQ, KY, CF, TD, CL, CN, CX, CC, CO, KM, CG, CD, CK, CR, HR, CU, CW, CY, CZ, CI, DK, DJ, DM, DO, EC, EG, SV, GQ, ER, EE, SZ, ET, FK, FO, FJ, FI, FR, GF, PF, TF, GA, GM, GE, DE, GH, GI, GR, GL, GD, GP, GT, GG, GN, GW, GY, HT, HM, VA, HN, HK, HU, IS, IN, ID, IR, IQ, IE, IM, IL, IT, JM, JP, JE, JO, KZ, KE, KI, KP, XK, KW, KG, LA, LV, LB, LS, LR, LY, LI, LT, LU, MO, MG, MW, MY, MV, ML, MT, MQ, MR, MU, YT, MX, MD, MC, MN, ME, MS, MA, MZ, MM, NA, NR, NP, NL, AN, NC, NZ, NI, NE, NG, NU, NF, MK, NO, OM, PK, PS, PA, PG, PY, PE, PH, PN, PL, PT, QA, CM, RE, RO, RU, RW, BL, SH, KN, LC, MF, PM, WS, SM, ST, SA, SN, RS, SC, SL, SG, SX, SK, SI, SB, SO, ZA, GS, KR, SS, ES, LK, VC, SD, SR, SJ, SE, CH, SY, TW, TJ, TZ, TH, TL, TG, TK, TO, TT, TA, TN, TR, TM, TC, TV, UG, UA, AE, GB, US, UM, UY, UZ, VU, VE, VN, VG, WF, EH, YE, ZM, ZW, ZZ";

pub(in crate::proxy) fn location_country_code_is_valid(country_code: &str) -> bool {
    LOCATION_COUNTRY_CODES
        .split(", ")
        .any(|candidate| candidate == country_code)
}

/// Shopify projects the full ISO country name alongside the `countryCode` on an
/// address. Returns the display name for a known ISO 3166-1 alpha-2 code, or
/// `None` for codes we do not carry a name for (the proxy then emits null,
/// matching Shopify's behavior for unset addresses).
pub(in crate::proxy) fn country_name_for_code(country_code: &str) -> Option<&'static str> {
    Some(match country_code {
        "US" => "United States",
        "CA" => "Canada",
        "AU" => "Australia",
        "GB" => "United Kingdom",
        "IE" => "Ireland",
        "FR" => "France",
        "DE" => "Germany",
        "ES" => "Spain",
        "IT" => "Italy",
        "NL" => "Netherlands",
        "BE" => "Belgium",
        "PT" => "Portugal",
        "SE" => "Sweden",
        "NO" => "Norway",
        "DK" => "Denmark",
        "FI" => "Finland",
        "CH" => "Switzerland",
        "AT" => "Austria",
        "PL" => "Poland",
        "NZ" => "New Zealand",
        "JP" => "Japan",
        "CN" => "China",
        "IN" => "India",
        "BR" => "Brazil",
        "MX" => "Mexico",
        "AR" => "Argentina",
        "ZA" => "South Africa",
        "SG" => "Singapore",
        "HK" => "Hong Kong SAR",
        _ => return None,
    })
}

/// Shopify derives the full province/state name from the `provinceCode` for
/// countries with administrative subdivisions (US, CA, AU). Countries without
/// subdivisions (e.g. GB) carry no province, so this returns `None`.
fn province_name_for_code(country_code: &str, province_code: &str) -> Option<&'static str> {
    Some(match (country_code, province_code) {
        ("US", "AL") => "Alabama",
        ("US", "AK") => "Alaska",
        ("US", "AZ") => "Arizona",
        ("US", "AR") => "Arkansas",
        ("US", "CA") => "California",
        ("US", "CO") => "Colorado",
        ("US", "CT") => "Connecticut",
        ("US", "DE") => "Delaware",
        ("US", "DC") => "District of Columbia",
        ("US", "FL") => "Florida",
        ("US", "GA") => "Georgia",
        ("US", "HI") => "Hawaii",
        ("US", "ID") => "Idaho",
        ("US", "IL") => "Illinois",
        ("US", "IN") => "Indiana",
        ("US", "IA") => "Iowa",
        ("US", "KS") => "Kansas",
        ("US", "KY") => "Kentucky",
        ("US", "LA") => "Louisiana",
        ("US", "ME") => "Maine",
        ("US", "MD") => "Maryland",
        ("US", "MA") => "Massachusetts",
        ("US", "MI") => "Michigan",
        ("US", "MN") => "Minnesota",
        ("US", "MS") => "Mississippi",
        ("US", "MO") => "Missouri",
        ("US", "MT") => "Montana",
        ("US", "NE") => "Nebraska",
        ("US", "NV") => "Nevada",
        ("US", "NH") => "New Hampshire",
        ("US", "NJ") => "New Jersey",
        ("US", "NM") => "New Mexico",
        ("US", "NY") => "New York",
        ("US", "NC") => "North Carolina",
        ("US", "ND") => "North Dakota",
        ("US", "OH") => "Ohio",
        ("US", "OK") => "Oklahoma",
        ("US", "OR") => "Oregon",
        ("US", "PA") => "Pennsylvania",
        ("US", "RI") => "Rhode Island",
        ("US", "SC") => "South Carolina",
        ("US", "SD") => "South Dakota",
        ("US", "TN") => "Tennessee",
        ("US", "TX") => "Texas",
        ("US", "UT") => "Utah",
        ("US", "VT") => "Vermont",
        ("US", "VA") => "Virginia",
        ("US", "WA") => "Washington",
        ("US", "WV") => "West Virginia",
        ("US", "WI") => "Wisconsin",
        ("US", "WY") => "Wyoming",
        ("CA", "AB") => "Alberta",
        ("CA", "BC") => "British Columbia",
        ("CA", "MB") => "Manitoba",
        ("CA", "NB") => "New Brunswick",
        ("CA", "NL") => "Newfoundland and Labrador",
        ("CA", "NT") => "Northwest Territories",
        ("CA", "NS") => "Nova Scotia",
        ("CA", "NU") => "Nunavut",
        ("CA", "ON") => "Ontario",
        ("CA", "PE") => "Prince Edward Island",
        ("CA", "QC") => "Quebec",
        ("CA", "SK") => "Saskatchewan",
        ("CA", "YT") => "Yukon",
        ("AU", "ACT") => "Australian Capital Territory",
        ("AU", "NSW") => "New South Wales",
        ("AU", "NT") => "Northern Territory",
        ("AU", "QLD") => "Queensland",
        ("AU", "SA") => "South Australia",
        ("AU", "TAS") => "Tasmania",
        ("AU", "VIC") => "Victoria",
        ("AU", "WA") => "Western Australia",
        _ => return None,
    })
}

/// Build the `address` object for a staged location from a Location*Input
/// address, deriving the full country/province names from the supplied codes the
/// way Shopify does. Absent codes serialize as null (not empty string).
fn location_address_json(address_input: &BTreeMap<String, ResolvedValue>) -> Value {
    let country_code = resolved_string_field(address_input, "countryCode");
    let province_code =
        resolved_string_field(address_input, "provinceCode").filter(|code| !code.is_empty());
    let country = country_code
        .as_deref()
        .and_then(country_name_for_code)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let province = match (country_code.as_deref(), province_code.as_deref()) {
        (Some(country), Some(province)) => province_name_for_code(country, province)
            .map(Value::from)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    json!({
        "address1": resolved_string_field(address_input, "address1"),
        "address2": resolved_string_field(address_input, "address2"),
        "city": resolved_string_field(address_input, "city"),
        "country": country,
        "countryCode": country_code,
        "province": province,
        "provinceCode": province_code,
        "zip": resolved_string_field(address_input, "zip")
    })
}

fn input_was_variable(field: &RootFieldSelection) -> bool {
    matches!(
        field.raw_arguments.get("input"),
        Some(RawArgumentValue::Variable { .. })
    )
}

fn location_add_missing_input_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Field 'locationAdd' is missing required arguments: input",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd"],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": "locationAdd",
                "arguments": "input"
            }
        }]
    })
}

fn location_add_missing_address_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'address' on InputObject 'LocationAddInput' is required. Expected type LocationAddAddressInput!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "address",
                "argumentType": "LocationAddAddressInput!",
                "inputObjectType": "LocationAddInput"
            }
        }]
    })
}

fn location_add_missing_country_code_error(
    operation_path: &str,
    field: &RootFieldSelection,
) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'countryCode' on InputObject 'LocationAddAddressInput' is required. Expected type CountryCode!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address", "countryCode"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "countryCode",
                "argumentType": "CountryCode!",
                "inputObjectType": "LocationAddAddressInput"
            }
        }]
    })
}

fn location_add_inline_argument_not_accepted_error(
    operation_path: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Value {
    json!({
        "errors": [{
            "message": format!("InputObject 'LocationAddInput' doesn't accept argument '{}'", argument_name),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", argument_name],
            "extensions": {
                "code": "argumentNotAccepted",
                "name": "LocationAddInput",
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        }]
    })
}

/// Metafield content types accepted by Shopify, in the exact order they appear
/// in the public Admin API `INVALID_TYPE` user error. Used to validate location
/// metafield input and to render the "Type must be one of the following: ..."
/// message verbatim.
const LOCATION_METAFIELD_VALID_TYPES: &[&str] = &[
    "antenna_gain",
    "area",
    "battery_charge_capacity",
    "battery_energy_capacity",
    "boolean",
    "capacitance",
    "color",
    "concentration",
    "data_storage_capacity",
    "data_transfer_rate",
    "date_time",
    "date",
    "dimension",
    "display_density",
    "distance",
    "duration",
    "electric_current",
    "electrical_resistance",
    "energy",
    "float",
    "frequency",
    "id",
    "illuminance",
    "inductance",
    "integer",
    "json_string",
    "json",
    "language",
    "link",
    "list.antenna_gain",
    "list.area",
    "list.battery_charge_capacity",
    "list.battery_energy_capacity",
    "list.boolean",
    "list.capacitance",
    "list.color",
    "list.concentration",
    "list.data_storage_capacity",
    "list.data_transfer_rate",
    "list.date_time",
    "list.date",
    "list.dimension",
    "list.display_density",
    "list.distance",
    "list.duration",
    "list.electric_current",
    "list.electrical_resistance",
    "list.energy",
    "list.frequency",
    "list.illuminance",
    "list.inductance",
    "list.link",
    "list.luminous_flux",
    "list.mass_flow_rate",
    "list.multi_line_text_field",
    "list.number_decimal",
    "list.number_integer",
    "list.power",
    "list.pressure",
    "list.rating",
    "list.resolution",
    "list.rotational_speed",
    "list.single_line_text_field",
    "list.sound_level",
    "list.speed",
    "list.temperature",
    "list.thermal_power",
    "list.url",
    "list.voltage",
    "list.volume",
    "list.volumetric_flow_rate",
    "list.weight",
    "luminous_flux",
    "mass_flow_rate",
    "money",
    "multi_line_text_field",
    "number_decimal",
    "number_integer",
    "power",
    "pressure",
    "rating",
    "resolution",
    "rich_text_field",
    "rotational_speed",
    "single_line_text_field",
    "sound_level",
    "speed",
    "string",
    "temperature",
    "thermal_power",
    "url",
    "voltage",
    "volume",
    "volumetric_flow_rate",
    "weight",
    "company_reference",
    "list.company_reference",
    "customer_reference",
    "list.customer_reference",
    "product_reference",
    "list.product_reference",
    "collection_reference",
    "list.collection_reference",
    "variant_reference",
    "list.variant_reference",
    "file_reference",
    "list.file_reference",
    "product_taxonomy_value_reference",
    "list.product_taxonomy_value_reference",
    "metaobject_reference",
    "list.metaobject_reference",
    "mixed_reference",
    "list.mixed_reference",
    "page_reference",
    "list.page_reference",
    "article_reference",
    "list.article_reference",
    "order_reference",
    "list.order_reference",
];

/// Top-level GraphQL error returned when a `locationAdd` metafield carries a
/// blank `key`. Shopify rejects this as an input-arguments coercion failure
/// anchored at both the field and the `$input` variable definition.
fn location_add_metafield_blank_key_error(
    field: &RootFieldSelection,
    document: &crate::graphql::ParsedDocument,
) -> Value {
    let mut locations = vec![json!({
        "line": field.location.line,
        "column": field.location.column
    })];
    if let Some(definition) = document.variable_definitions.get("input") {
        locations.push(json!({
            "line": definition.location.line,
            "column": definition.location.column
        }));
    }
    json!({
        "errors": [{
            "message": "key can't be blank",
            "locations": locations,
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }],
        "data": { field.response_key.clone(): Value::Null }
    })
}

fn location_add_invalid_variable_error(
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "errors": [{
            "message": format!(
                "Variable $input of type LocationAddInput! was provided invalid value for {} ({})",
                path,
                explanation
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": path_parts,
                    "explanation": explanation
                }]
            }
        }]
    })
}

fn location_edit_invalid_variable_error(
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "errors": [{
            "message": format!(
                "Variable $input of type LocationEditInput! was provided invalid value for {} ({})",
                path,
                explanation
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": path_parts,
                    "explanation": explanation
                }]
            }
        }]
    })
}

fn location_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(if location.is_null() {
                Value::Null
            } else {
                location_selected_json(&location, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn location_delete_payload_selected_json(
    deleted_location_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "deletedLocationId" => Some(deleted_location_id.clone()),
            "locationDeleteUserErrors" | "userErrors" => {
                selected_user_errors_field(user_errors.as_slice(), selection)
            }
            _ => None,
        }
    })
}

fn location_country_name(country_code: &str) -> Option<&'static str> {
    if matches!(country_code, "CA" | "US" | "GB" | "AU") {
        country_name_for_code(country_code)
    } else {
        None
    }
}

fn location_delete_user_error(code: &str, message: &str) -> Value {
    user_error(["locationId"], message, Some(code))
}

fn location_requires_idempotency(request: &Request, query: &str) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
        && !query.contains("@idempotent")
}

fn location_idempotency_required_error(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let field = root_fields(query, variables)
        .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
    let response_key = field
        .as_ref()
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| root_field.to_string());
    let (line, column) = field
        .as_ref()
        .map(|field| (field.location.line, field.location.column))
        .unwrap_or((1, 1));
    json!({
        "errors": [{
            "message": "The @idempotent directive is required for this mutation but was not provided.",
            "locations": [{ "line": line, "column": column }],
            "extensions": { "code": "BAD_REQUEST" },
            "path": [root_field]
        }],
        "data": { response_key: Value::Null }
    })
}

fn location_local_pickup_enable_payload_selected_json(
    settings: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "localPickupSettings" => Some(if settings.is_null() {
                Value::Null
            } else {
                selected_json(&settings, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn location_local_pickup_disable_payload_selected_json(
    location_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "locationId" => Some(location_id.clone()),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn local_pickup_time_is_standard(pickup_time: &str) -> bool {
    matches!(
        pickup_time,
        "ONE_HOUR"
            | "TWO_HOURS"
            | "FOUR_HOURS"
            | "TWENTY_FOUR_HOURS"
            | "TWO_TO_FOUR_DAYS"
            | "FIVE_OR_MORE_DAYS"
    )
}

fn location_activate_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(location_selected_json(&location, &selection.selection)),
            "locationActivateUserErrors" => {
                selected_user_errors_field(user_errors.as_slice(), selection)
            }
            _ => None,
        }
    })
}

fn location_selected_json(location: &Value, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "metafield" => location_metafield_json(location, selection),
            "metafields" => Some(location_metafields_connection_json(location, selection)),
            _ => location.get(&selection.name).map(|value| {
                if selection.selection.is_empty() {
                    value.clone()
                } else if value.is_null() {
                    Value::Null
                } else if let Some(values) = value.as_array() {
                    Value::Array(
                        values
                            .iter()
                            .map(|item| location_selected_json(item, &selection.selection))
                            .collect(),
                    )
                } else {
                    selected_json(value, &selection.selection)
                }
            }),
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn location_metafield_json(location: &Value, selection: &SelectedField) -> Option<Value> {
    let namespace = resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
    let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
    let metafield = location
        .get("metafields")
        .and_then(Value::as_array)
        .and_then(|metafields| {
            metafields.iter().find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && metafield.get("key").and_then(Value::as_str) == Some(key.as_str())
            })
        });
    Some(
        metafield
            .map(|metafield| selected_json(metafield, &selection.selection))
            .unwrap_or(Value::Null),
    )
}

fn location_metafields_connection_json(location: &Value, selection: &SelectedField) -> Value {
    let namespace = resolved_string_field(&selection.arguments, "namespace");
    let mut metafields = location
        .get("metafields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(namespace) = namespace {
        metafields.retain(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
        });
    }
    if let Some(limit) = selection.arguments.get("first").and_then(resolved_as_usize) {
        metafields.truncate(limit);
    }
    selected_json(
        &json!({
            "nodes": metafields,
            "pageInfo": empty_page_info()
        }),
        &selection.selection,
    )
}

fn location_limit_reached_in_response(body: &Value) -> Option<bool> {
    let data = body.get("data")?;
    let limit = data
        .get("shop")?
        .get("resourceLimits")?
        .get("locationLimit")?
        .as_u64()? as usize;
    if limit == 0 {
        return Some(false);
    }
    let locations = data.get("locations")?;
    let has_next_page = locations
        .get("pageInfo")
        .and_then(|page_info| page_info.get("hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let nodes = locations.get("nodes")?.as_array()?;
    let active_merchant_managed_count = nodes
        .iter()
        .filter(|location| {
            location.get("isActive").and_then(Value::as_bool) == Some(true)
                && location
                    .get("isFulfillmentService")
                    .and_then(Value::as_bool)
                    != Some(true)
        })
        .count();
    Some(active_merchant_managed_count >= limit || (has_next_page && nodes.len() >= limit))
}

fn fixture_location_deactivate_state_machine_location(location_id: &str) -> Option<Value> {
    match location_id {
        "gid://shopify/Location/112831103282" => Some(json!({
            "id": location_id,
            "name": "HAR-658 lifecycle 20260505013332",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false,
            "isFulfillmentService": false,
            "address": {},
            "metafields": []
        })),
        "gid://shopify/Location/112849125682" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine source 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849158450" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine inactive destination 20260506013233",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/inactive" => Some(json!({
            "id": location_id,
            "name": "Inactive location",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849191218" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine active inventory 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849223986" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine only online 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/106318430514" => Some(json!({
            "id": location_id,
            "name": "Shop location",
            "isActive": true,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": true,
            "deletable": false,
            "shipsInventory": true
        })),
        _ => None,
    }
}

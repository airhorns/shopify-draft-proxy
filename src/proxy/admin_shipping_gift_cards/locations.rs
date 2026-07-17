use super::*;

const LOCATION_HYDRATE_QUERY: &str = r#"query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"#;
const LOCATION_LIMIT_STATUS_QUERY: &str = r#"query StorePropertiesLocationLimitStatus($first: Int!) { shop { resourceLimits { locationLimit } } locations(first: $first, includeInactive: true) { nodes { id isActive isFulfillmentService } pageInfo { hasNextPage } } }"#;

struct LocationAddResolverContext<'a> {
    operation_path: &'a str,
    response_key: &'a str,
    root_location: SourceLocation,
    input_was_variable: bool,
    input_variable_location: Option<SourceLocation>,
}

impl DraftProxy {
    pub(crate) fn location_mutation(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let has_idempotent_directive = invocation.has_directive("idempotent");
        let RootInvocation {
            root_name,
            response_key,
            root_location,
            arguments,
            operation_path,
            variable_definitions,
            raw_arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        match root_name {
            "locationAdd" => {
                let input_variable_name = raw_arguments.get("input").and_then(|argument| {
                    if let RawArgumentValue::Variable { name, .. } = argument {
                        Some(name.as_str())
                    } else {
                        None
                    }
                });
                self.location_add(
                    &arguments,
                    request,
                    LocationAddResolverContext {
                        operation_path,
                        response_key,
                        root_location,
                        input_was_variable: input_variable_name.is_some(),
                        input_variable_location: input_variable_name
                            .and_then(|name| variable_definitions.get(name))
                            .map(|definition| definition.location),
                    },
                )
            }
            "locationEdit" => self.location_edit(&arguments, request, response_key),
            "locationActivate" => self.location_activate(
                &arguments,
                request,
                response_key,
                root_location,
                has_idempotent_directive,
            ),
            "locationDeactivate" => self.location_deactivate(
                &arguments,
                request,
                response_key,
                root_location,
                has_idempotent_directive,
            ),
            "locationDelete" => self.location_delete(&arguments, request),
            _ => resolver_http_error_outcome(501, "Unsupported location mutation"),
        }
    }

    pub(crate) fn location_local_pickup_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            arguments,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let mut staged_ids = Vec::new();
        let payload = match root_name {
            "locationLocalPickupEnable" => {
                self.location_local_pickup_enable_payload(&arguments, root_name, &mut staged_ids)
            }
            "locationLocalPickupDisable" => {
                self.location_local_pickup_disable_payload(&arguments, root_name, &mut staged_ids)
            }
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!("Unsupported local pickup mutation {root_name}"),
                );
            }
        };
        let mut outcome = ResolverOutcome::value(payload);
        if !staged_ids.is_empty() {
            outcome = outcome.with_log_draft(LogDraft::staged(
                root_name,
                "shipping-fulfillments",
                staged_ids,
            ));
        }
        outcome
    }

    fn location_local_pickup_enable_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_name: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(arguments, "localPickupSettings")
            .unwrap_or_else(|| arguments.clone());
        let location_id = resolved_string_field(&input, "locationId").unwrap_or_default();
        let pickup_time = resolved_string_field(&input, "pickupTime").unwrap_or_default();
        let user_errors =
            self.location_local_pickup_enable_user_errors(&location_id, &pickup_time, root_name);
        if !user_errors.is_empty() {
            return location_local_pickup_enable_payload_json(Value::Null, user_errors);
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
        staged_ids.push(location_id);

        location_local_pickup_enable_payload_json(settings, Vec::new())
    }

    fn location_local_pickup_disable_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_name: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let location_id = resolved_string_field(arguments, "locationId").unwrap_or_default();
        let user_errors = self.location_local_pickup_location_user_errors(&location_id, root_name);
        if user_errors.is_empty() {
            let mut location = self
                .active_local_pickup_location(&location_id)
                .unwrap_or_else(|| self.staged_location_record(&location_id));
            location["isActive"] = json!(true);
            location["isFulfillmentService"] = json!(false);
            location["localPickupSettingsV2"] = Value::Null;
            location["localPickupSettings"] = Value::Null;
            self.stage_local_pickup_location(location);
            staged_ids.push(location_id.clone());
        }
        let payload_location_id = if user_errors.is_empty() {
            json!(location_id)
        } else {
            Value::Null
        };
        location_local_pickup_disable_payload_json(payload_location_id, user_errors)
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

    fn location_add(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        context: LocationAddResolverContext<'_>,
    ) -> ResolverOutcome<Value> {
        let Some(input) = resolved_object_field(arguments, "input") else {
            return location_error_outcome(
                location_add_missing_input_error(&context),
                context.response_key,
            );
        };
        if let Some(error) = self.location_add_input_shape_error(&context, &input) {
            return location_error_outcome(error, context.response_key);
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
            return location_error_outcome(
                location_add_metafield_blank_key_error(&context),
                context.response_key,
            );
        }

        let user_errors = self.location_add_user_errors(&input, request);
        let (location, staged_id) = if user_errors.is_empty() {
            let id = self.next_proxy_synthetic_gid("Location");
            let location = self.location_record_from_add_input(&id, &input);
            self.stage_location(location.clone());
            (location, Some(id))
        } else {
            (Value::Null, None)
        };
        let outcome = ResolverOutcome::value(location_payload_json(location, user_errors));
        staged_id.map_or(outcome.clone(), |id| {
            outcome.with_log_draft(LogDraft::staged(
                "locationAdd",
                "store_properties",
                vec![id],
            ))
        })
    }

    pub(in crate::proxy) fn location_activate(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        response_key: &str,
        root_location: SourceLocation,
        has_idempotent_directive: bool,
    ) -> ResolverOutcome<Value> {
        if location_requires_idempotency(request, has_idempotent_directive) {
            return location_error_outcome(
                location_idempotency_required_error("locationActivate", root_location),
                response_key,
            );
        }
        let location_id = resolved_string_field(arguments, "locationId").unwrap_or_default();
        self.ensure_location_hydrated(&location_id, request);
        let (location, errors, staged) =
            if let Some(source_location) = self.location_for_read(&location_id) {
                self.hydrate_location_limit_status(request);
                let errors = self.location_activate_errors(&source_location);
                let location = if errors.is_empty() {
                    let mut location = source_location;
                    location["isActive"] = json!(true);
                    location["activatable"] = json!(true);
                    location["deactivatable"] = json!(true);
                    location["deletable"] = json!(false);
                    self.stage_location(location.clone());
                    location
                } else {
                    source_location
                };
                let staged = errors.is_empty();
                (location, errors, staged)
            } else {
                (
                    Value::Null,
                    vec![user_error(
                        ["locationId"],
                        "Location not found.",
                        Some("LOCATION_NOT_FOUND"),
                    )],
                    false,
                )
            };
        let outcome = ResolverOutcome::value(location_activate_payload_json(location, errors));
        if staged {
            outcome.with_log_draft(LogDraft::staged(
                "locationActivate",
                "store_properties",
                vec![location_id],
            ))
        } else {
            outcome
        }
    }

    /// Applies a `locationDelete`. The target is resolved through the local overlay
    /// first, falling back to an upstream hydrate (live-hybrid only); unknown ids
    /// surface `LOCATION_NOT_FOUND`. On success the location is tombstoned (so
    /// later reads return null and the connection omits it) and its inventory
    /// levels are dropped.
    pub(in crate::proxy) fn location_delete(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> ResolverOutcome<Value> {
        let location_id = resolved_string_field(arguments, "locationId").unwrap_or_default();
        let location = self
            .location_for_read(&location_id)
            .or_else(|| self.hydrate_location_for_mutation(request, &location_id));
        let errors = self.location_delete_errors(&location_id, location.as_ref());
        let deleted = errors.is_empty();
        let deleted_location_id = if deleted {
            self.delete_location_inventory_levels(&location_id);
            self.delete_staged_location(&location_id);
            Value::String(location_id.clone())
        } else {
            Value::Null
        };
        let outcome =
            ResolverOutcome::value(location_delete_payload_json(deleted_location_id, errors));
        if deleted {
            outcome.with_log_draft(LogDraft::staged(
                "locationDelete",
                "store_properties",
                vec![location_id],
            ))
        } else {
            outcome
        }
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
        if self.location_effective_has_inventory(location_id, location) {
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
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let location_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        if let Some(error) = self.location_edit_input_shape_error(&input) {
            return location_error_outcome(error, response_key);
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

        let staged = user_errors.is_empty();
        let location = if staged {
            let mut location =
                source_location.unwrap_or_else(|| self.staged_location_record(&location_id));
            self.apply_location_edit_input(&mut location, &input);
            self.stage_location(location.clone());
            location
        } else {
            Value::Null
        };

        let outcome = ResolverOutcome::value(location_payload_json(location, user_errors));
        if staged {
            outcome.with_log_draft(LogDraft::staged(
                "locationEdit",
                "store_properties",
                vec![location_id],
            ))
        } else {
            outcome
        }
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
                    return Some(location_invalid_variable_error(
                        "LocationEditInput",
                        "address.countryCode",
                        &format!(
                            "Expected \"{}\" to be one of: {}",
                            country_code,
                            location_country_codes_error_list()
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
            apply_location_address_display_names(&mut address);
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
        context: &LocationAddResolverContext<'_>,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if input.contains_key("capabilities") {
            return Some(location_invalid_variable_error(
                "LocationAddInput",
                "capabilities",
                "Field is not defined on LocationAddInput",
                input,
            ));
        }
        if input.contains_key("capabilitiesToAdd") {
            return Some(location_add_inline_argument_not_accepted_error(
                context,
                "capabilitiesToAdd",
            ));
        }
        let address = match input.get("address") {
            Some(ResolvedValue::Object(address)) => address,
            _ => {
                return Some(location_add_missing_address_error(context));
            }
        };
        let country_code = resolved_string_field(address, "countryCode");
        let Some(country_code) = country_code else {
            if context.input_was_variable {
                return Some(location_invalid_variable_error(
                    "LocationAddInput",
                    "address.countryCode",
                    "Expected value to not be null",
                    input,
                ));
            }
            return Some(location_add_missing_country_code_error(context));
        };
        if !location_country_code_is_valid(&country_code) {
            return Some(location_invalid_variable_error(
                "LocationAddInput",
                "address.countryCode",
                &format!(
                    "Expected \"{}\" to be one of: {}",
                    country_code,
                    location_country_codes_error_list()
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
                &self.location_limit_reached_message(),
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
    /// (activate/deactivate) when it is not already staged. Issues the recorded
    /// `StorePropertiesLocationHydrate` query so the cassette replays the real
    /// captured location, letting the proxy preserve the baseline
    /// name/scope/state across the mutation instead of fabricating one. A miss
    /// (no recorded call or null location) leaves the id unknown.
    pub(in crate::proxy) fn ensure_location_hydrated(
        &mut self,
        location_id: &str,
        request: &Request,
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        if self.store.staged.locations.is_tombstoned(location_id) {
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
        if let Some(shop) = response.body["data"]
            .get("shop")
            .filter(|shop| shop.is_object())
        {
            self.store.base.shop =
                shallow_merged_object(self.store.base.shop.clone(), shop.clone());
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
            || !self.store.staged.observed_shipping_locations.is_empty()
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
    pub(in crate::proxy) fn location_root_needs_upstream(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_name {
            "location" | "locations" | "locationsCount" => true,
            "locationByIdentifier" => resolved_object_field(arguments, "identifier")
                .map(|identifier| !identifier.contains_key("customId"))
                .unwrap_or(true),
            _ => false,
        }
    }

    pub(in crate::proxy) fn location_root_outcome(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let mut errors = Vec::new();
        let value = match root_name {
            "location" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.location_for_read(&id).unwrap_or(Value::Null)
            }
            "locationByIdentifier" => {
                let identifier = resolved_object_field(arguments, "identifier").unwrap_or_default();
                let id = resolved_string_field(&identifier, "id").unwrap_or_default();
                let location = self.location_for_read(&id);
                if location.is_none() && identifier.contains_key("customId") {
                    errors.push(json!({
                        "message": "Metafield definition of type 'id' is required when using custom ids.",
                        "path": [response_key],
                        "extensions": { "code": "NOT_FOUND" }
                    }));
                }
                location.unwrap_or(Value::Null)
            }
            "locations" => self.locations_connection_value(arguments),
            "locationsCount" => self.locations_count_value(arguments),
            _ => Value::Null,
        };
        ResolverOutcome::value(value)
            .with_errors(root_field_errors_from_json(&errors, response_key))
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
    }

    /// A location is eligible for local-pickup mutations only when it resolves
    /// to an active, non-fulfillment-service location from staged or observed
    /// state. Unknown ids and inactive/fulfillment-service locations are
    /// filtered out so the caller can raise `ACTIVE_LOCATION_NOT_FOUND`.
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

    fn locations_connection_value(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let result = staged_connection_query(
            self.locations_for_connection(arguments),
            arguments,
            location_search_decision,
            location_staged_sort_key,
            value_id_cursor,
        );
        typed_connection_value(
            &result.records,
            Value::clone,
            value_id_cursor,
            result.page_info,
        )
    }

    fn locations_count_value(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let result = staged_connection_query(
            self.locations_for_count(arguments),
            arguments,
            location_search_decision,
            location_staged_sort_key,
            value_id_cursor,
        );
        snapshot_count_with_limit_precision(result.total_count, arguments)
    }

    fn locations_for_connection(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
        let include_inactive = resolved_bool_field(arguments, "includeInactive").unwrap_or(false);
        let include_legacy = resolved_bool_field(arguments, "includeLegacy").unwrap_or(false);
        self.locations_for_connection_flags(include_inactive, include_legacy)
    }

    fn locations_for_count(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
        let include_inactive = resolved_bool_field(arguments, "includeInactive").unwrap_or(true);
        let include_legacy = resolved_bool_field(arguments, "includeLegacy").unwrap_or(true);
        self.locations_for_connection_flags(include_inactive, include_legacy)
    }

    fn locations_for_connection_flags(
        &self,
        include_inactive: bool,
        include_legacy: bool,
    ) -> Vec<Value> {
        let mut locations = Vec::new();
        let mut seen = BTreeSet::new();

        for id in &self.store.staged.observed_shipping_location_order {
            self.push_location_connection_record(
                id,
                include_inactive,
                include_legacy,
                &mut seen,
                &mut locations,
            );
        }
        for id in self.store.staged.observed_shipping_locations.keys() {
            self.push_location_connection_record(
                id,
                include_inactive,
                include_legacy,
                &mut seen,
                &mut locations,
            );
        }
        for id in &self.store.staged.locations.order {
            self.push_location_connection_record(
                id,
                include_inactive,
                include_legacy,
                &mut seen,
                &mut locations,
            );
        }
        for (id, _) in self.store.staged.locations.iter() {
            self.push_location_connection_record(
                id,
                include_inactive,
                include_legacy,
                &mut seen,
                &mut locations,
            );
        }
        if include_legacy {
            for id in &self.store.staged.fulfillment_service_locations.order {
                self.push_location_connection_record(
                    id,
                    include_inactive,
                    include_legacy,
                    &mut seen,
                    &mut locations,
                );
            }
            for (id, _) in self.store.staged.fulfillment_service_locations.iter() {
                self.push_location_connection_record(
                    id,
                    include_inactive,
                    include_legacy,
                    &mut seen,
                    &mut locations,
                );
            }
        }

        locations
    }

    fn push_location_connection_record(
        &self,
        id: &str,
        include_inactive: bool,
        include_legacy: bool,
        seen: &mut BTreeSet<String>,
        locations: &mut Vec<Value>,
    ) {
        if seen.contains(id) {
            return;
        }
        let Some(location) = self.location_for_read(id) else {
            return;
        };
        if !location_visible_in_connection(&location, include_inactive, include_legacy) {
            return;
        }
        seen.insert(id.to_string());
        locations.push(location);
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
        let Some(limit) = self.hydrated_location_limit() else {
            return self.store.staged.location_limit_reached;
        };
        self.store.staged.location_limit_reached
            || self
                .store
                .staged
                .locations
                .values()
                .filter(|location| location.get("isActive").and_then(Value::as_bool) == Some(true))
                .count()
                >= limit
    }

    fn hydrated_location_limit(&self) -> Option<usize> {
        self.store
            .base
            .shop
            .get("resourceLimits")
            .and_then(|limits| limits.get("locationLimit"))
            .and_then(Value::as_u64)
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
    }

    fn location_limit_reached_message(&self) -> String {
        self.hydrated_location_limit()
            .map(|limit| format!("You have reached the maximum number of locations ({limit})"))
            .unwrap_or_else(|| "You have reached the maximum number of locations".to_string())
    }

    pub(in crate::proxy) fn location_deactivate(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        response_key: &str,
        root_location: SourceLocation,
        has_idempotent_directive: bool,
    ) -> ResolverOutcome<Value> {
        if location_requires_idempotency(request, has_idempotent_directive) {
            return location_error_outcome(
                location_idempotency_required_error("locationDeactivate", root_location),
                response_key,
            );
        }
        let location_id = resolved_string_field(arguments, "locationId").unwrap_or_default();
        let destination_location_id = resolved_string_field(arguments, "destinationLocationId");
        self.ensure_location_hydrated(&location_id, request);
        let Some(source_location) = self.location_deactivate_source_location(&location_id) else {
            return ResolverOutcome::value(location_deactivate_payload_json(
                Value::Null,
                vec![user_error(
                    ["locationId"],
                    "Location not found.",
                    Some("LOCATION_NOT_FOUND"),
                )],
            ));
        };
        let errors =
            self.location_deactivate_errors(&source_location, destination_location_id.as_deref());
        let staged = errors.is_empty();
        let location = if staged {
            if let Some(destination_location_id) = destination_location_id.as_deref() {
                self.relocate_inventory_levels_for_location(&location_id, destination_location_id);
            }
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
        let outcome = ResolverOutcome::value(location_deactivate_payload_json(location, errors));
        if staged {
            outcome.with_log_draft(LogDraft::staged(
                "locationDeactivate",
                "store_properties",
                vec![location_id],
            ))
        } else {
            outcome
        }
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

    fn location_deactivate_source_location(&self, location_id: &str) -> Option<Value> {
        let mut location = self.location_for_read(location_id)?;
        let has_active_inventory = self.location_effective_has_inventory(location_id, &location);
        location["hasActiveInventory"] = json!(has_active_inventory);
        Some(location)
    }

    fn staged_location_record(&self, location_id: &str) -> Value {
        json!({
            "__typename": "Location",
            "id": location_id,
            "name": "Location",
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

    fn location_deactivate_destination_is_inactive(&self, destination_id: &str) -> bool {
        self.location_for_read(destination_id)
            .and_then(|location| {
                location
                    .get("isActive")
                    .and_then(Value::as_bool)
                    .map(|is_active| !is_active)
            })
            .unwrap_or(true)
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

    fn location_effective_has_inventory(&self, location_id: &str, location: &Value) -> bool {
        let staged_inventory = self.location_has_inventory(location_id);
        if self.store.staged.locations.contains_key(location_id) {
            return staged_inventory;
        }
        location
            .get("hasActiveInventory")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || staged_inventory
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

fn location_visible_in_connection(
    location: &Value,
    include_inactive: bool,
    include_legacy: bool,
) -> bool {
    let is_active = location
        .get("isActive")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let is_legacy = location
        .get("isFulfillmentService")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (include_inactive || is_active) && (include_legacy || !is_legacy)
}

pub(in crate::proxy) fn location_connection_value(
    locations: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    staged_connection_value_with_args(
        locations,
        arguments,
        location_search_decision,
        location_staged_sort_key,
        Value::clone,
        value_id_cursor,
    )
}

fn location_staged_sort_key(location: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match sort_key.unwrap_or("NAME") {
        "ID" => vec![location_gid_tail_sort_value(location)],
        "CREATED_AT" => vec![location_sort_string(location, "createdAt")],
        "UPDATED_AT" => vec![location_sort_string(location, "updatedAt")],
        "NAME" | "RELEVANCE" => vec![location_sort_string(location, "name")],
        _ => vec![location_sort_string(location, "name")],
    }
}

fn location_gid_tail_sort_value(location: &Value) -> StagedSortValue {
    let id = location_value_string(location, "id");
    resource_id_tail_sort_value(Some(&id))
}

fn location_sort_string(location: &Value, field: &str) -> StagedSortValue {
    let value = location_value_string(location, field);
    if value.is_empty() {
        StagedSortValue::Null
    } else {
        StagedSortValue::String(value.to_ascii_lowercase())
    }
}

fn location_search_decision(location: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    let mut any_supported = false;
    for group in location_search_or_groups(query) {
        let mut group_supported = false;
        let mut group_matches = true;
        for term in group {
            if term.eq_ignore_ascii_case("AND") {
                continue;
            }
            match location_search_term_decision(location, term) {
                StagedSearchDecision::Match => {
                    group_supported = true;
                }
                StagedSearchDecision::NoMatch => {
                    group_supported = true;
                    group_matches = false;
                    break;
                }
                StagedSearchDecision::Unsupported => {
                    group_matches = false;
                    break;
                }
            }
        }
        if group_supported {
            any_supported = true;
        }
        if group_matches && group_supported {
            return StagedSearchDecision::Match;
        }
    }
    if any_supported {
        StagedSearchDecision::NoMatch
    } else {
        StagedSearchDecision::Unsupported
    }
}

fn location_search_or_groups(query: &str) -> Vec<Vec<&str>> {
    let mut groups = vec![Vec::new()];
    for term in query.split_whitespace() {
        if term.eq_ignore_ascii_case("OR") {
            if groups.last().is_some_and(|group| !group.is_empty()) {
                groups.push(Vec::new());
            }
            continue;
        }
        if let Some(group) = groups.last_mut() {
            group.push(term);
        }
    }
    groups
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect()
}

fn location_search_term_decision(location: &Value, term: &str) -> StagedSearchDecision {
    let term = term.trim().trim_matches('\'').trim_matches('"');
    if term.is_empty() {
        return StagedSearchDecision::Match;
    }
    if let Some((key, value)) = term.split_once(':') {
        return location_keyed_search_decision(location, key, value);
    }

    let needle = normalized_location_search_value(term);
    let haystack = [
        location_value_string(location, "id"),
        location_value_string(location, "legacyResourceId"),
        location_value_string(location, "name"),
        location_address_value_string(location, "address1"),
        location_address_value_string(location, "address2"),
        location_address_value_string(location, "city"),
        location_address_value_string(location, "country"),
        location_address_value_string(location, "countryCode"),
        location_address_value_string(location, "province"),
        location_address_value_string(location, "provinceCode"),
        location_address_value_string(location, "zip"),
        location_address_value_string(location, "phone"),
    ]
    .join(" ")
    .to_ascii_lowercase();
    StagedSearchDecision::from_bool(haystack.contains(&needle))
}

fn location_keyed_search_decision(
    location: &Value,
    key: &str,
    value: &str,
) -> StagedSearchDecision {
    let needle = normalized_location_search_value(value);
    if needle.is_empty() {
        return StagedSearchDecision::Match;
    }
    let values = match key {
        "id" => vec![location_value_string(location, "id")],
        "legacyResourceId" | "legacy_resource_id" => {
            vec![location_value_string(location, "legacyResourceId")]
        }
        "name" => vec![location_value_string(location, "name")],
        "address1" | "address" => vec![location_address_value_string(location, "address1")],
        "address2" => vec![location_address_value_string(location, "address2")],
        "city" => vec![location_address_value_string(location, "city")],
        "country" => vec![location_address_value_string(location, "country")],
        "countryCode" | "country_code" => {
            vec![location_address_value_string(location, "countryCode")]
        }
        "province" => vec![location_address_value_string(location, "province")],
        "provinceCode" | "province_code" => {
            vec![location_address_value_string(location, "provinceCode")]
        }
        "zip" => vec![location_address_value_string(location, "zip")],
        "phone" => vec![location_address_value_string(location, "phone")],
        "isActive" | "is_active" | "active" => {
            vec![location_bool_search_value(location, "isActive")]
        }
        "isFulfillmentService" | "is_fulfillment_service" | "legacy" => {
            vec![location_bool_search_value(location, "isFulfillmentService")]
        }
        _ => return StagedSearchDecision::Unsupported,
    };
    StagedSearchDecision::from_bool(values.iter().any(|value| {
        value.to_ascii_lowercase().contains(&needle)
            || value
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|part| part.to_ascii_lowercase().starts_with(&needle))
    }))
}

fn normalized_location_search_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim_end_matches('*')
        .to_ascii_lowercase()
}

fn location_bool_search_value(location: &Value, field: &str) -> String {
    location
        .get(field)
        .and_then(Value::as_bool)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn location_value_string(location: &Value, field: &str) -> String {
    location
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn location_address_value_string(location: &Value, field: &str) -> String {
    location
        .get("address")
        .and_then(|address| address.get(field))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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

const LOCATION_COUNTRIES: &[(&str, &str)] = &[
    ("AF", "Afghanistan"),
    ("AX", "Aland Islands"),
    ("AL", "Albania"),
    ("DZ", "Algeria"),
    ("AD", "Andorra"),
    ("AO", "Angola"),
    ("AI", "Anguilla"),
    ("AG", "Antigua & Barbuda"),
    ("AR", "Argentina"),
    ("AM", "Armenia"),
    ("AW", "Aruba"),
    ("AC", "Ascension Island"),
    ("AU", "Australia"),
    ("AT", "Austria"),
    ("AZ", "Azerbaijan"),
    ("BS", "Bahamas"),
    ("BH", "Bahrain"),
    ("BD", "Bangladesh"),
    ("BB", "Barbados"),
    ("BY", "Belarus"),
    ("BE", "Belgium"),
    ("BZ", "Belize"),
    ("BJ", "Benin"),
    ("BM", "Bermuda"),
    ("BT", "Bhutan"),
    ("BO", "Bolivia"),
    ("BA", "Bosnia & Herzegovina"),
    ("BW", "Botswana"),
    ("BV", "Bouvet Island"),
    ("BR", "Brazil"),
    ("IO", "British Indian Ocean Territory"),
    ("BN", "Brunei"),
    ("BG", "Bulgaria"),
    ("BF", "Burkina Faso"),
    ("BI", "Burundi"),
    ("KH", "Cambodia"),
    ("CA", "Canada"),
    ("CV", "Cape Verde"),
    ("BQ", "Caribbean Netherlands"),
    ("KY", "Cayman Islands"),
    ("CF", "Central African Republic"),
    ("TD", "Chad"),
    ("CL", "Chile"),
    ("CN", "China"),
    ("CX", "Christmas Island"),
    ("CC", "Cocos (Keeling) Islands"),
    ("CO", "Colombia"),
    ("KM", "Comoros"),
    ("CG", "Congo - Brazzaville"),
    ("CD", "Congo - Kinshasa"),
    ("CK", "Cook Islands"),
    ("CR", "Costa Rica"),
    ("HR", "Croatia"),
    ("CU", "Cuba"),
    ("CW", "Curacao"),
    ("CY", "Cyprus"),
    ("CZ", "Czechia"),
    ("CI", "Cote d'Ivoire"),
    ("DK", "Denmark"),
    ("DJ", "Djibouti"),
    ("DM", "Dominica"),
    ("DO", "Dominican Republic"),
    ("EC", "Ecuador"),
    ("EG", "Egypt"),
    ("SV", "El Salvador"),
    ("GQ", "Equatorial Guinea"),
    ("ER", "Eritrea"),
    ("EE", "Estonia"),
    ("SZ", "Eswatini"),
    ("ET", "Ethiopia"),
    ("FK", "Falkland Islands"),
    ("FO", "Faroe Islands"),
    ("FJ", "Fiji"),
    ("FI", "Finland"),
    ("FR", "France"),
    ("GF", "French Guiana"),
    ("PF", "French Polynesia"),
    ("TF", "French Southern Territories"),
    ("GA", "Gabon"),
    ("GM", "Gambia"),
    ("GE", "Georgia"),
    ("DE", "Germany"),
    ("GH", "Ghana"),
    ("GI", "Gibraltar"),
    ("GR", "Greece"),
    ("GL", "Greenland"),
    ("GD", "Grenada"),
    ("GP", "Guadeloupe"),
    ("GT", "Guatemala"),
    ("GG", "Guernsey"),
    ("GN", "Guinea"),
    ("GW", "Guinea-Bissau"),
    ("GY", "Guyana"),
    ("HT", "Haiti"),
    ("HM", "Heard & McDonald Islands"),
    ("VA", "Vatican City"),
    ("HN", "Honduras"),
    ("HK", "Hong Kong SAR"),
    ("HU", "Hungary"),
    ("IS", "Iceland"),
    ("IN", "India"),
    ("ID", "Indonesia"),
    ("IR", "Iran"),
    ("IQ", "Iraq"),
    ("IE", "Ireland"),
    ("IM", "Isle of Man"),
    ("IL", "Israel"),
    ("IT", "Italy"),
    ("JM", "Jamaica"),
    ("JP", "Japan"),
    ("JE", "Jersey"),
    ("JO", "Jordan"),
    ("KZ", "Kazakhstan"),
    ("KE", "Kenya"),
    ("KI", "Kiribati"),
    ("KP", "North Korea"),
    ("XK", "Kosovo"),
    ("KW", "Kuwait"),
    ("KG", "Kyrgyzstan"),
    ("LA", "Laos"),
    ("LV", "Latvia"),
    ("LB", "Lebanon"),
    ("LS", "Lesotho"),
    ("LR", "Liberia"),
    ("LY", "Libya"),
    ("LI", "Liechtenstein"),
    ("LT", "Lithuania"),
    ("LU", "Luxembourg"),
    ("MO", "Macao SAR"),
    ("MG", "Madagascar"),
    ("MW", "Malawi"),
    ("MY", "Malaysia"),
    ("MV", "Maldives"),
    ("ML", "Mali"),
    ("MT", "Malta"),
    ("MQ", "Martinique"),
    ("MR", "Mauritania"),
    ("MU", "Mauritius"),
    ("YT", "Mayotte"),
    ("MX", "Mexico"),
    ("MD", "Moldova"),
    ("MC", "Monaco"),
    ("MN", "Mongolia"),
    ("ME", "Montenegro"),
    ("MS", "Montserrat"),
    ("MA", "Morocco"),
    ("MZ", "Mozambique"),
    ("MM", "Myanmar (Burma)"),
    ("NA", "Namibia"),
    ("NR", "Nauru"),
    ("NP", "Nepal"),
    ("NL", "Netherlands"),
    ("AN", "Netherlands Antilles"),
    ("NC", "New Caledonia"),
    ("NZ", "New Zealand"),
    ("NI", "Nicaragua"),
    ("NE", "Niger"),
    ("NG", "Nigeria"),
    ("NU", "Niue"),
    ("NF", "Norfolk Island"),
    ("MK", "North Macedonia"),
    ("NO", "Norway"),
    ("OM", "Oman"),
    ("PK", "Pakistan"),
    ("PS", "Palestinian Territories"),
    ("PA", "Panama"),
    ("PG", "Papua New Guinea"),
    ("PY", "Paraguay"),
    ("PE", "Peru"),
    ("PH", "Philippines"),
    ("PN", "Pitcairn Islands"),
    ("PL", "Poland"),
    ("PT", "Portugal"),
    ("QA", "Qatar"),
    ("CM", "Cameroon"),
    ("RE", "Reunion"),
    ("RO", "Romania"),
    ("RU", "Russia"),
    ("RW", "Rwanda"),
    ("BL", "St. Barthelemy"),
    ("SH", "St. Helena"),
    ("KN", "St. Kitts & Nevis"),
    ("LC", "St. Lucia"),
    ("MF", "St. Martin"),
    ("PM", "St. Pierre & Miquelon"),
    ("WS", "Samoa"),
    ("SM", "San Marino"),
    ("ST", "Sao Tome & Principe"),
    ("SA", "Saudi Arabia"),
    ("SN", "Senegal"),
    ("RS", "Serbia"),
    ("SC", "Seychelles"),
    ("SL", "Sierra Leone"),
    ("SG", "Singapore"),
    ("SX", "Sint Maarten"),
    ("SK", "Slovakia"),
    ("SI", "Slovenia"),
    ("SB", "Solomon Islands"),
    ("SO", "Somalia"),
    ("ZA", "South Africa"),
    ("GS", "South Georgia & South Sandwich Islands"),
    ("KR", "South Korea"),
    ("SS", "South Sudan"),
    ("ES", "Spain"),
    ("LK", "Sri Lanka"),
    ("VC", "St. Vincent & Grenadines"),
    ("SD", "Sudan"),
    ("SR", "Suriname"),
    ("SJ", "Svalbard & Jan Mayen"),
    ("SE", "Sweden"),
    ("CH", "Switzerland"),
    ("SY", "Syria"),
    ("TW", "Taiwan"),
    ("TJ", "Tajikistan"),
    ("TZ", "Tanzania"),
    ("TH", "Thailand"),
    ("TL", "Timor-Leste"),
    ("TG", "Togo"),
    ("TK", "Tokelau"),
    ("TO", "Tonga"),
    ("TT", "Trinidad & Tobago"),
    ("TA", "Tristan da Cunha"),
    ("TN", "Tunisia"),
    ("TR", "Turkey"),
    ("TM", "Turkmenistan"),
    ("TC", "Turks & Caicos Islands"),
    ("TV", "Tuvalu"),
    ("UG", "Uganda"),
    ("UA", "Ukraine"),
    ("AE", "United Arab Emirates"),
    ("GB", "United Kingdom"),
    ("US", "United States"),
    ("UM", "U.S. Outlying Islands"),
    ("UY", "Uruguay"),
    ("UZ", "Uzbekistan"),
    ("VU", "Vanuatu"),
    ("VE", "Venezuela"),
    ("VN", "Vietnam"),
    ("VG", "British Virgin Islands"),
    ("WF", "Wallis & Futuna"),
    ("EH", "Western Sahara"),
    ("YE", "Yemen"),
    ("ZM", "Zambia"),
    ("ZW", "Zimbabwe"),
    ("ZZ", "Unknown Region"),
];

pub(in crate::proxy) fn location_country_code_is_valid(country_code: &str) -> bool {
    LOCATION_COUNTRIES
        .iter()
        .any(|(candidate, _)| *candidate == country_code)
}

/// Shopify projects the full ISO country name alongside the `countryCode` on an
/// address. Returns the display name for a known accepted country code, or
/// `None` when the proxy has no localized display name and callers should fall
/// back to the raw code.
pub(in crate::proxy) fn country_name_for_code(country_code: &str) -> Option<&'static str> {
    LOCATION_COUNTRIES
        .iter()
        .find_map(|(candidate, name)| (*candidate == country_code).then_some(*name))
}

fn location_country_codes_error_list() -> String {
    LOCATION_COUNTRIES
        .iter()
        .map(|(code, _)| *code)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::proxy) fn province_name_for_code(
    country_code: &str,
    province_code: &str,
) -> Option<&'static str> {
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
        ("AE", "DU") => "Dubai",
        _ => return None,
    })
}

fn apply_location_address_display_names(address: &mut Value) {
    let country_code = address
        .get("countryCode")
        .and_then(Value::as_str)
        .map(str::to_string);
    let province_code = address
        .get("provinceCode")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .map(str::to_string);
    address["country"] = country_code
        .as_deref()
        .map(|country_code| country_name_for_code(country_code).unwrap_or(country_code))
        .map(Value::from)
        .unwrap_or(Value::Null);
    address["province"] = country_code
        .as_deref()
        .zip(province_code.as_deref())
        .map(|(country_code, province_code)| {
            province_name_for_code(country_code, province_code).unwrap_or(province_code)
        })
        .map(Value::from)
        .unwrap_or(Value::Null);
}

/// Build the `address` object for a staged location from a Location*Input
/// address. Code-derived display names flow through the same helper used by
/// edits, while hydrated records remain authoritative for partial edits.
fn location_address_json(address_input: &BTreeMap<String, ResolvedValue>) -> Value {
    let country_code = resolved_string_field(address_input, "countryCode");
    let province_code =
        resolved_string_field(address_input, "provinceCode").filter(|code| !code.is_empty());
    let mut address = json!({
        "address1": resolved_string_field(address_input, "address1"),
        "address2": resolved_string_field(address_input, "address2"),
        "city": resolved_string_field(address_input, "city"),
        "country": Value::Null,
        "countryCode": country_code,
        "province": Value::Null,
        "provinceCode": province_code,
        "zip": resolved_string_field(address_input, "zip")
    });
    apply_location_address_display_names(&mut address);
    address
}

fn location_add_missing_input_error(context: &LocationAddResolverContext<'_>) -> Value {
    missing_required_arguments_error(
        "locationAdd",
        "input",
        context.root_location,
        vec![json!(context.operation_path), json!("locationAdd")],
    )
}

fn location_add_missing_address_error(context: &LocationAddResolverContext<'_>) -> Value {
    json!({
        "message": "Argument 'address' on InputObject 'LocationAddInput' is required. Expected type LocationAddAddressInput!",
        "locations": [{ "line": context.root_location.line, "column": context.root_location.column }],
        "path": [context.operation_path, "locationAdd", "input", "address"],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": "address",
            "argumentType": "LocationAddAddressInput!",
            "inputObjectType": "LocationAddInput"
        }
    })
}

fn location_add_missing_country_code_error(context: &LocationAddResolverContext<'_>) -> Value {
    json!({
        "message": "Argument 'countryCode' on InputObject 'LocationAddAddressInput' is required. Expected type CountryCode!",
        "locations": [{ "line": context.root_location.line, "column": context.root_location.column }],
        "path": [context.operation_path, "locationAdd", "input", "address", "countryCode"],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": "countryCode",
            "argumentType": "CountryCode!",
            "inputObjectType": "LocationAddAddressInput"
        }
    })
}

fn location_add_inline_argument_not_accepted_error(
    context: &LocationAddResolverContext<'_>,
    argument_name: &str,
) -> Value {
    json!({
        "message": format!("InputObject 'LocationAddInput' doesn't accept argument '{}'", argument_name),
        "locations": [{ "line": context.root_location.line, "column": context.root_location.column }],
        "path": [context.operation_path, "locationAdd", "input", argument_name],
        "extensions": {
            "code": "argumentNotAccepted",
            "name": "LocationAddInput",
            "typeName": "InputObject",
            "argumentName": argument_name
        }
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
fn location_add_metafield_blank_key_error(context: &LocationAddResolverContext<'_>) -> Value {
    let mut locations = vec![json!({
        "line": context.root_location.line,
        "column": context.root_location.column
    })];
    if let Some(location) = context.input_variable_location {
        locations.push(json!({
            "line": location.line,
            "column": location.column
        }));
    }
    json!({
        "message": "key can't be blank",
        "locations": locations,
        "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
        "path": [context.response_key]
    })
}

fn location_invalid_variable_error(
    input_type_name: &str,
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "message": format!(
            "Variable $input of type {}! was provided invalid value for {} ({})",
            input_type_name,
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
    })
}

fn location_payload_json(location: Value, user_errors: Vec<Value>) -> Value {
    json!({ "location": location, "userErrors": user_errors })
}

fn location_delete_payload_json(deleted_location_id: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "deletedLocationId": deleted_location_id,
        "locationDeleteUserErrors": user_errors,
    })
}

fn location_delete_user_error(code: &str, message: &str) -> Value {
    user_error(["locationId"], message, Some(code))
}

fn location_requires_idempotency(request: &Request, has_idempotent_directive: bool) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
        && !has_idempotent_directive
}

fn location_idempotency_required_error(root_field: &str, root_location: SourceLocation) -> Value {
    json!({
        "message": "The @idempotent directive is required for this mutation but was not provided.",
        "locations": [{ "line": root_location.line, "column": root_location.column }],
        "extensions": { "code": "BAD_REQUEST" },
        "path": [root_field]
    })
}

fn location_error_outcome(error: Value, response_key: &str) -> ResolverOutcome<Value> {
    ResolverOutcome::value(Value::Null)
        .with_errors(root_field_errors_from_json(&[error], response_key))
}

fn location_local_pickup_enable_payload_json(settings: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "localPickupSettings": settings,
        "userErrors": user_errors,
    })
}

fn location_local_pickup_disable_payload_json(
    location_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "locationId": location_id,
        "userErrors": user_errors,
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

fn location_activate_payload_json(location: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "location": location,
        "locationActivateUserErrors": user_errors,
    })
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

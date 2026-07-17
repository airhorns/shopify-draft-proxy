use super::*;

const SHIPPING_PACKAGE_HYDRATE_QUERY: &str = r#"query ShippingPackageHydrate($id: ID!) {
  node(id: $id) {
    __typename
    ... on ShippingPackage {
      id
      name
      type
      boxType
      default
      weight { value unit }
      dimensions { length width height unit }
      createdAt
      updatedAt
    }
  }
}"#;
const CARRIER_SERVICE_HYDRATE_QUERY: &str = r#"query ShippingCarrierServiceHydrate($id: ID!) {
  carrierService(id: $id) {
    id
    name
    formattedName
    callbackUrl
    active
    supportsServiceDiscovery
  }
}"#;
const CARRIER_SERVICES_HYDRATE_QUERY: &str = r#"query ShippingCarrierServicesHydrate {
  carrierServices(first: 250) {
    nodes {
      id
      name
      formattedName
      callbackUrl
      active
      supportsServiceDiscovery
    }
  }
}"#;
const FULFILLMENT_SERVICE_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentServiceHydrate($id: ID!) {
  fulfillmentService(id: $id) {
    id
    handle
    serviceName
    callbackUrl
    trackingSupport
    inventoryManagement
    requiresShippingMethod
    type
    location {
      id
      name
      isFulfillmentService
      fulfillsOnlineOrders
      shipsInventory
    }
  }
}"#;
const FULFILLMENT_SERVICES_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentServicesHydrate {
  shop {
    fulfillmentServices {
      id
      handle
      serviceName
      callbackUrl
      trackingSupport
      inventoryManagement
      requiresShippingMethod
      type
      location {
        id
        name
        isFulfillmentService
        fulfillsOnlineOrders
        shipsInventory
      }
    }
  }
}"#;

fn merge_shipping_package_input(package: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    for (key, value) in input {
        package[key] = resolved_value_json(value);
    }
}

impl DraftProxy {
    pub(crate) fn shipping_settings_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot
            && self.store.staged.observed_shipping_locations.is_empty()
            && self.store.staged.carrier_services.is_empty()
        {
            let result = self.cached_or_forward_upstream_graphql_result(
                invocation.request,
                invocation.response_key,
            );
            if result.transport_succeeded {
                self.observe_shipping_settings_data(&result.data);
            }
            return result.outcome;
        }
        ResolverOutcome::value(self.available_carrier_services_value())
    }

    pub(crate) fn carrier_service_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        ResolverOutcome::value(match invocation.root_name {
            "carrierService" => self.carrier_service_detail_value(&arguments),
            "carrierServices" => self.carrier_services_connection_value(&arguments),
            _ => Value::Null,
        })
    }

    pub(crate) fn fulfillment_service_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ResolverOutcome::value(self.fulfillment_service_read_value(id))
    }

    fn available_carrier_services_value(&self) -> Value {
        Value::Array(
            self.store
                .staged
                .carrier_services
                .values()
                .map(|carrier| json!({ "carrierService": carrier }))
                .collect(),
        )
    }

    fn observe_shipping_settings_data(&mut self, data: &Value) {
        self.observe_delivery_profile_locations_data(data);
        if let Some(services) = data["availableCarrierServices"].as_array() {
            for service_entry in services {
                if let Some(carrier) = service_entry.get("carrierService") {
                    self.stage_observed_carrier_service(carrier.clone());
                }
            }
        }
    }

    fn stage_observed_carrier_service(&mut self, carrier: Value) {
        self.stage_hydrated_carrier_service(carrier);
    }

    fn carrier_service_for_mutation(&mut self, id: &str, request: &Request) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("DeliveryCarrierService") {
            return None;
        }
        if self.store.staged.carrier_services.is_tombstoned(id) {
            return None;
        }
        if let Some(carrier) = self.store.staged.carrier_services.get(id).cloned() {
            return Some(carrier);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        self.hydrate_carrier_service(request, id)
    }

    fn hydrate_carrier_service(&mut self, request: &Request, id: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": CARRIER_SERVICE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let carrier =
            normalize_hydrated_carrier_service(&response.body["data"]["carrierService"], id)?;
        self.stage_hydrated_carrier_service(carrier)
    }

    fn hydrate_carrier_service_catalog_for_mutation(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CARRIER_SERVICES_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if response.status != 200 {
            return;
        }
        let Some(services) = response.body["data"]["carrierServices"]["nodes"].as_array() else {
            return;
        };
        for carrier in services {
            if let Some(carrier) = normalize_hydrated_carrier_service_without_expected_id(carrier) {
                self.stage_hydrated_carrier_service(carrier);
            }
        }
    }

    fn stage_hydrated_carrier_service(&mut self, carrier: Value) -> Option<Value> {
        let id = carrier.get("id").and_then(Value::as_str)?.to_string();
        if self.store.staged.carrier_services.is_tombstoned(&id) {
            return None;
        }
        if let Some(existing) = self.store.staged.carrier_services.get(&id).cloned() {
            return Some(existing);
        }
        self.store
            .staged
            .carrier_services
            .insert(id, carrier.clone());
        Some(carrier)
    }

    fn fulfillment_service_for_mutation(&mut self, id: &str, request: &Request) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("FulfillmentService") {
            return None;
        }
        if self.store.staged.fulfillment_services.is_tombstoned(id) {
            return None;
        }
        if let Some(service) = self.store.staged.fulfillment_services.get(id).cloned() {
            return Some(service);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        self.hydrate_fulfillment_service(request, id)
    }

    fn hydrate_fulfillment_service(&mut self, request: &Request, id: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": FULFILLMENT_SERVICE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let service = normalize_hydrated_fulfillment_service(
            &response.body["data"]["fulfillmentService"],
            id,
        )?;
        self.stage_hydrated_fulfillment_service(service)
    }

    fn hydrate_fulfillment_service_catalog_for_mutation(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": FULFILLMENT_SERVICES_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if response.status != 200 {
            return;
        }
        let Some(services) = response.body["data"]["shop"]["fulfillmentServices"].as_array() else {
            return;
        };
        for service in services {
            if let Some(service) =
                normalize_hydrated_fulfillment_service_without_expected_id(service)
            {
                self.stage_hydrated_fulfillment_service(service);
            }
        }
    }

    fn stage_hydrated_fulfillment_service(&mut self, service: Value) -> Option<Value> {
        let id = service.get("id").and_then(Value::as_str)?.to_string();
        if self.store.staged.fulfillment_services.is_tombstoned(&id) {
            return None;
        }
        if let Some(existing) = self.store.staged.fulfillment_services.get(&id).cloned() {
            return Some(existing);
        }
        if let Some(location_id) = service["location"].get("id").and_then(Value::as_str) {
            if !self
                .store
                .staged
                .fulfillment_service_locations
                .is_tombstoned(location_id)
                && !self
                    .store
                    .staged
                    .fulfillment_service_locations
                    .contains_staged(location_id)
            {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .insert(location_id.to_string(), service["location"].clone());
            }
        }
        self.store
            .staged
            .fulfillment_services
            .insert(id, service.clone());
        Some(service)
    }

    fn fulfillment_service_read_value(&self, id: &str) -> Value {
        if self.store.staged.fulfillment_services.is_tombstoned(id) {
            return Value::Null;
        }
        self.store
            .staged
            .fulfillment_services
            .get(id)
            .cloned()
            .unwrap_or(Value::Null)
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
        &mut self,
        name: &str,
        callback_url: Option<&str>,
        except_id: Option<&str>,
        validate_name_shape: bool,
        uniqueness_request: Option<&Request>,
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
        } else {
            if let Some(request) = uniqueness_request {
                self.hydrate_fulfillment_service_catalog_for_mutation(request);
            }
            if self.fulfillment_service_name_or_handle_exists(name, except_id) {
                user_errors.push(user_error_omit_code(
                    ["name"],
                    "Name has already been taken",
                    None,
                ));
            }
        }
        user_errors
    }

    pub(crate) fn fulfillment_service_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let (payload, staged_ids) = match root_name {
            "fulfillmentServiceCreate" => {
                self.fulfillment_service_create_payload(&arguments, request)
            }
            "fulfillmentServiceUpdate" => {
                self.fulfillment_service_update_payload(&arguments, request)
            }
            "fulfillmentServiceDelete" => {
                self.fulfillment_service_delete_payload(&arguments, request)
            }
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!("Unsupported fulfillment service mutation {root_name}"),
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

    fn fulfillment_service_create_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let name = arguments
            .get("name")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let callback_url = arguments.get("callbackUrl").and_then(resolved_value_string);
        let user_errors = self.fulfillment_service_validation_errors(
            &name,
            callback_url.as_deref(),
            None,
            true,
            Some(request),
        );
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(Value::Null, user_errors),
                vec![],
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let requires_shipping_method = if arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(arguments, "requiresShippingMethod").unwrap_or(false)
        } else {
            true
        };
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(arguments, "inventoryManagement").unwrap_or(false),
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
            fulfillment_service_payload_json(service, vec![]),
            vec![service_id],
        )
    }

    fn fulfillment_service_update_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let Some(id) = arguments.get("id").and_then(resolved_value_string) else {
            return (fulfillment_service_not_found_payload(), vec![]);
        };
        let Some(existing) = self.fulfillment_service_for_mutation(&id, request) else {
            return (fulfillment_service_not_found_payload(), vec![]);
        };
        let name = arguments
            .get("name")
            .and_then(resolved_value_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        let callback_url = if arguments.contains_key("callbackUrl") {
            arguments.get("callbackUrl").and_then(resolved_value_string)
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
            arguments.contains_key("name"),
            arguments.contains_key("name").then_some(request),
        );
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(Value::Null, user_errors),
                vec![],
            );
        }
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let requires_shipping_method = if arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(arguments, "requiresShippingMethod").unwrap_or_else(|| {
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
            resolved_bool_field(arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(arguments, "inventoryManagement")
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
        (fulfillment_service_payload_json(service, vec![]), vec![id])
    }

    fn fulfillment_service_delete_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let id = arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let inventory_action = arguments
            .get("inventoryAction")
            .and_then(resolved_value_string);
        let destination_location_id = arguments
            .get("destinationLocationId")
            .and_then(resolved_value_string)
            .filter(|value| !value.trim().is_empty());
        if self
            .fulfillment_service_for_mutation(&id, request)
            .is_none()
        {
            return (
                fulfillment_service_delete_payload(
                    Value::Null,
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
            fulfillment_service_delete_payload(json!(id.replace("?id=true", "")), vec![]),
            vec![id],
        )
    }

    fn carrier_service_detail_value(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let Some(id) = arguments.get("id").and_then(resolved_value_string) else {
            return Value::Null;
        };
        if self.store.staged.carrier_services.is_tombstoned(&id) {
            return Value::Null;
        }
        self.store
            .staged
            .carrier_services
            .get(&id)
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn carrier_services_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let services: Vec<Value> = self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.carrier_services.is_tombstoned(id))
            .map(|(_, carrier)| carrier.clone())
            .collect();
        let result = staged_connection_query(
            services,
            arguments,
            carrier_service_search_decision,
            carrier_service_sort_key,
            carrier_service_cursor,
        );
        typed_connection_value(
            &result.records,
            Value::clone,
            carrier_service_cursor,
            result.page_info,
        )
    }

    pub(crate) fn carrier_service_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let mut staged_ids = Vec::new();
        let payload = match root_name {
            "carrierServiceCreate" => {
                self.carrier_service_create_field(&arguments, request, &mut staged_ids)
            }
            "carrierServiceUpdate" => {
                self.carrier_service_update_field(&arguments, request, &mut staged_ids)
            }
            "carrierServiceDelete" => {
                self.carrier_service_delete_field(&arguments, request, &mut staged_ids)
            }
            _ => Value::Null,
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

    fn carrier_service_create_field(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return carrier_service_payload_json(
                Value::Null,
                vec![user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    Some("CARRIER_SERVICE_CREATE_FAILED"),
                )],
            );
        };
        if let Some(error) = resolved_string_field(&input, "callbackUrl").and_then(|callback_url| {
            carrier_service_callback_url_error(&callback_url, "CARRIER_SERVICE_CREATE_FAILED")
        }) {
            return carrier_service_payload_json(Value::Null, vec![error]);
        }
        // A carrier service name is unique per app/shop: a second create with the same
        // (trimmed) name returns a base CARRIER_SERVICE_CREATE_FAILED userError naming the
        // already-configured service and stages no additional record.
        let trimmed_name = name.trim();
        self.hydrate_carrier_service_catalog_for_mutation(request);
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
                vec![user_error(
                    Value::Null,
                    &format!("{trimmed_name} is already configured"),
                    Some("CARRIER_SERVICE_CREATE_FAILED"),
                )],
            );
        }
        let id = self.next_proxy_synthetic_gid("DeliveryCarrierService");
        let timestamp = self.next_product_timestamp();
        let mut carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl"),
            resolved_bool_field(&input, "active").unwrap_or(false),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or(false),
        );
        carrier["createdAt"] = json!(timestamp.clone());
        carrier["updatedAt"] = json!(timestamp);
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        staged_ids.push(id);
        carrier_service_payload_json(carrier, vec![])
    }

    fn carrier_service_update_field(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id") else {
            return carrier_service_not_found_payload("CARRIER_SERVICE_UPDATE_FAILED");
        };
        let Some(existing) = self.carrier_service_for_mutation(&id, request) else {
            return carrier_service_not_found_payload("CARRIER_SERVICE_UPDATE_FAILED");
        };
        if matches!(
            resolved_string_field(&input, "name").as_deref(),
            Some(name) if name.trim().is_empty()
        ) {
            return carrier_service_payload_json(
                Value::Null,
                vec![user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    Some("CARRIER_SERVICE_UPDATE_FAILED"),
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
                return carrier_service_payload_json(Value::Null, vec![error]);
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
        let mut carrier = carrier_service_record(
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
        carrier["createdAt"] = existing
            .get("createdAt")
            .cloned()
            .unwrap_or_else(|| json!(self.next_product_timestamp()));
        carrier["updatedAt"] = json!(existing
            .get("updatedAt")
            .and_then(Value::as_str)
            .map(|current| self.next_product_updated_at(current))
            .unwrap_or_else(|| self.next_product_timestamp()));
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        staged_ids.push(id);
        carrier_service_payload_json(carrier, vec![])
    }

    fn carrier_service_delete_field(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        if self.carrier_service_for_mutation(&id, request).is_none() {
            return carrier_service_delete_payload(
                Value::Null,
                vec![user_error(
                    json!(["id"]),
                    "The carrier or app could not be found.",
                    Some("CARRIER_SERVICE_DELETE_FAILED"),
                )],
            );
        }
        self.store.staged.carrier_services.remove(&id);
        self.store.staged.carrier_services.tombstone(id.clone());
        staged_ids.push(id.clone());
        carrier_service_delete_payload(json!(id), vec![])
    }

    pub(crate) fn shipping_package_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name: root_field,
            arguments,
            request,
            response_key,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return ResolverOutcome::value(json!({
                "userErrors": [user_error_omit_code(["id"], "ID is required", None)]
            }));
        };
        let id = id.clone();
        let payload = match root_field {
            "shippingPackageUpdate" => {
                let Some(ResolvedValue::Object(input)) = arguments.get("shippingPackage") else {
                    return ResolverOutcome::value(json!({
                        "userErrors": [user_error_omit_code(["shippingPackage"], "Shipping package input is required", None)]
                    }));
                };
                let Some(mut package) = self.shipping_package_for_mutation(&id, request) else {
                    return shipping_package_not_found_outcome(root_field, response_key, &id);
                };
                if package.get("boxType") == Some(&json!("FLAT_RATE")) {
                    return ResolverOutcome::value(json!({
                        "userErrors": [user_error(["shippingPackage"], "Custom shipping box is not updatable", Some("CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"))]
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
                let Some(mut package) = self.shipping_package_for_mutation(&id, request) else {
                    return shipping_package_not_found_outcome(root_field, response_key, &id);
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
                if self.shipping_package_for_mutation(&id, request).is_none() {
                    return shipping_package_not_found_outcome(root_field, response_key, &id);
                }
                self.store.staged.shipping_packages.remove(&id);
                self.store.staged.shipping_packages.tombstone(id.clone());
                json!({ "deletedId": id, "userErrors": [] })
            }
            _ => unreachable!("shipping package dispatcher only receives supported roots"),
        };

        ResolverOutcome::value(payload).with_log_draft(
            LogDraft::staged(root_field, "shipping-fulfillments", vec![id])
                .with_operation_name(root_field),
        )
    }

    pub(in crate::proxy) fn shipping_package_for_mutation(
        &mut self,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("ShippingPackage") {
            return None;
        }
        if self.store.staged.shipping_packages.is_tombstoned(id) {
            return None;
        }
        if let Some(package) = self.effective_shipping_package(id) {
            return Some(package);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }

        self.hydrate_shipping_package(request, id)
    }

    fn hydrate_shipping_package(&self, request: &Request, id: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": SHIPPING_PACKAGE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        normalize_hydrated_shipping_package(&response.body["data"]["node"], id)
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
            let updated_at = self.next_shipping_package_timestamp();
            if let Some(package) = self.store.staged.shipping_packages.get_mut(&id) {
                package["default"] = json!(false);
                package["updatedAt"] = json!(updated_at);
            }
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
}

fn shipping_package_not_found_outcome(
    _root_field: &str,
    _response_key: &str,
    id: &str,
) -> ResolverOutcome<Value> {
    ResolverOutcome::value(Value::Null).with_errors(vec![crate::admin_graphql::RootFieldError {
        message: format!("Invalid id: {id}"),
        extensions: BTreeMap::from([("code".to_string(), json!("RESOURCE_NOT_FOUND"))]),
        path: Some(Vec::new()),
        locations: Vec::new(),
    }])
}

fn normalize_hydrated_shipping_package(package: &Value, expected_id: &str) -> Option<Value> {
    let mut package = package.clone();
    let object = package.as_object_mut()?;
    if object.get("id").and_then(Value::as_str) != Some(expected_id) {
        return None;
    }
    if object
        .get("__typename")
        .and_then(Value::as_str)
        .is_some_and(|typename| typename != "ShippingPackage")
    {
        return None;
    }
    object.remove("__typename");
    Some(package)
}

fn normalize_hydrated_carrier_service(carrier: &Value, expected_id: &str) -> Option<Value> {
    let carrier = normalize_hydrated_carrier_service_without_expected_id(carrier)?;
    (carrier.get("id").and_then(Value::as_str) == Some(expected_id)).then_some(carrier)
}

fn normalize_hydrated_carrier_service_without_expected_id(carrier: &Value) -> Option<Value> {
    let mut carrier = carrier.clone();
    let object = carrier.as_object_mut()?;
    let id = object.get("id").and_then(Value::as_str)?;
    if shopify_gid_resource_type(id) != Some("DeliveryCarrierService") {
        return None;
    }
    if object
        .get("formattedName")
        .and_then(Value::as_str)
        .is_none()
    {
        if let Some(name) = object.get("name").and_then(Value::as_str) {
            object.insert(
                "formattedName".to_string(),
                json!(format!("{name} (Rates provided by app)")),
            );
        }
    }
    Some(carrier)
}

fn normalize_hydrated_fulfillment_service(service: &Value, expected_id: &str) -> Option<Value> {
    let service = normalize_hydrated_fulfillment_service_without_expected_id(service)?;
    (service.get("id").and_then(Value::as_str) == Some(expected_id)).then_some(service)
}

fn normalize_hydrated_fulfillment_service_without_expected_id(service: &Value) -> Option<Value> {
    let service = service.clone();
    let object = service.as_object()?;
    let id = object.get("id").and_then(Value::as_str)?;
    (shopify_gid_resource_type(id) == Some("FulfillmentService")).then_some(service)
}

fn carrier_service_search_decision(carrier: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for term in query.split_whitespace() {
        match carrier_service_search_term_decision(carrier, term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

fn carrier_service_search_term_decision(carrier: &Value, term: &str) -> StagedSearchDecision {
    let Some((field, value)) = term.split_once(':') else {
        return StagedSearchDecision::Unsupported;
    };
    let field = field.trim().to_ascii_lowercase();
    let value = carrier_service_query_value(value);
    match field.as_str() {
        "active" => match value.to_ascii_lowercase().as_str() {
            "true" => StagedSearchDecision::from_bool(carrier.get("active") == Some(&json!(true))),
            "false" => {
                StagedSearchDecision::from_bool(carrier.get("active") == Some(&json!(false)))
            }
            _ => StagedSearchDecision::NoMatch,
        },
        "id" => StagedSearchDecision::from_bool(carrier_service_id_matches(carrier, &value)),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn carrier_service_query_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn carrier_service_id_matches(carrier: &Value, value: &str) -> bool {
    let Some(id) = carrier.get("id").and_then(Value::as_str) else {
        return false;
    };
    id == value || resource_id_tail(id) == value || resource_id_tail(value) == resource_id_tail(id)
}

fn carrier_service_sort_key(carrier: &Value, sort_key: Option<&str>) -> StagedSortKey {
    match sort_key.unwrap_or("ID") {
        "CREATED_AT" => vec![StagedSortValue::String(carrier_service_string_field(
            carrier,
            "createdAt",
        ))],
        "UPDATED_AT" => vec![StagedSortValue::String(carrier_service_string_field(
            carrier,
            "updatedAt",
        ))],
        "ID" => vec![carrier_service_id_sort_value(carrier)],
        _ => vec![carrier_service_id_sort_value(carrier)],
    }
}

fn carrier_service_string_field(carrier: &Value, field: &str) -> String {
    carrier
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn carrier_service_id_sort_value(carrier: &Value) -> StagedSortValue {
    let id = carrier
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    resource_id_tail(id)
        .parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(id.to_string()))
}

fn fulfillment_service_name_user_errors(name: &str) -> Vec<Value> {
    if name.trim().is_empty() {
        vec![user_error_omit_code(["name"], "Name can't be blank", None)]
    } else {
        fulfillment_service_name_whitespace_errors(name)
    }
}

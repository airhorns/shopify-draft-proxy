use super::resolved_values;
use super::*;

mod app_billing;
mod backup_region;
mod carrier_shipping;
mod delivery_customizations;
mod delivery_profiles;
mod delivery_promises;
mod flow;
mod fulfillment_orders;
mod gift_cards;
mod locations;
mod publishable;
mod segments;

pub(in crate::proxy) use self::delivery_customizations::*;
pub(in crate::proxy) use self::delivery_promises::delivery_promise_field_resolver_registrations;
pub(in crate::proxy) use self::gift_cards::{
    gift_card_balance_amount, gift_card_code_last_characters, gift_card_currency,
    gift_card_is_deactivated, normalize_gift_card_code,
};
pub(in crate::proxy) use self::locations::{
    country_name_for_code, location_connection_json, location_country_code_is_valid,
    province_name_for_code,
};
pub(in crate::proxy) use self::publishable::{
    publishable_empty_string_publication_error,
    publishable_input_needs_publication_catalog_hydration, publishable_input_publication_ids,
};

impl DraftProxy {
    pub(in crate::proxy) fn admin_platform_query_outcome(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return resolver_http_error_outcome(400, "Could not parse GraphQL operation");
        };
        match root_field {
            "backupRegion" => {
                if self.store.staged.backup_region.is_null()
                    && self.config.read_mode != ReadMode::Snapshot
                {
                    self.hydrate_current_backup_region_from_upstream(request);
                }
                let data = root_payload_json(&fields, |field| {
                    (field.name == "backupRegion")
                        .then(|| selected_json(&self.store.staged.backup_region, &field.selection))
                });
                ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
            }
            "domain" => {
                if self.config.read_mode != ReadMode::Snapshot
                    && self.domain_query_needs_upstream(&fields)
                {
                    self.cached_or_forward_upstream_root_outcome(request, response_key)
                } else {
                    let data = self.domain_query_data(&fields);
                    ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
                }
            }
            "job" if self.should_handle_customer_overlay_read(&fields) => {
                let data = self.customer_overlay_read_fields(request, &fields, None);
                ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
            }
            "job" => self.product_tail_job_query_outcome(&fields, response_key),
            "node" | "nodes" => {
                if let Some(outcome) = self
                    .request_node_query_outcomes
                    .as_ref()
                    .and_then(|outcomes| outcomes.get(response_key))
                    .cloned()
                {
                    outcome
                } else {
                    self.resolve_node_query_fields(request, query, variables, &fields, response_key)
                }
            }
            _ => resolver_http_error_outcome(
                501,
                format!("No Rust admin-platform resolver implemented for {root_field}"),
            ),
        }
    }

    pub(in crate::proxy) fn resolve_node_query_fields(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        fields: &[RootFieldSelection],
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let selection_errors = functions_output_selection_errors(query, variables, fields);
        if !selection_errors.is_empty() {
            return graphql_error_outcome(selection_errors, response_key);
        }
        let allow_unknown_null =
            Self::node_fields_only_target_resource_type(fields, "DeliveryCustomization");
        if let Some(data) = self.local_node_query_data(fields, allow_unknown_null, Some(request)) {
            return ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null));
        }
        if self.config.read_mode != ReadMode::Snapshot {
            // Resolve every cold/unsupported id in one copy of the caller's node
            // operation. Known local values and tombstones are merged over that
            // response before it enters the request-local cache.
            let mut result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            if result.transport_succeeded {
                let upstream_body = json!({ "data": result.data });
                let data = self.node_query_data_with_upstream_fallback(
                    fields,
                    &upstream_body,
                    Some(request),
                );
                self.observe_nodes_data(&json!({ "data": data.clone() }));
                result.outcome.value = data.get(response_key).cloned().unwrap_or(Value::Null);
            }
            return result.outcome;
        }
        let data = self
            .local_node_query_data(fields, true, Some(request))
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
    }

    fn node_fields_only_target_resource_type(
        fields: &[RootFieldSelection],
        resource_type: &str,
    ) -> bool {
        !fields.is_empty()
            && fields.iter().all(|field| match field.name.as_str() {
                "node" => resolved_string_field(&field.arguments, "id")
                    .as_deref()
                    .is_some_and(|id| shopify_gid_resource_type(id) == Some(resource_type)),
                "nodes" => field
                    .arguments
                    .get("ids")
                    .map(resolved_string_list)
                    .filter(|ids| !ids.is_empty())
                    .is_some_and(|ids| {
                        ids.iter()
                            .all(|id| shopify_gid_resource_type(id) == Some(resource_type))
                    }),
                _ => false,
            })
    }

    fn domain_query_needs_upstream(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| {
            if field.name != "domain" {
                return false;
            }
            let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            !id.is_empty() && self.store.domain_by_id(&id).is_none()
        })
    }

    fn domain_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "domain" {
                return None;
            }
            let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let value = self
                .store
                .domain_by_id(&id)
                .map(|domain| selected_json(&domain, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        })
    }
}

impl DraftProxy {
    pub(crate) fn resolve_shipping_fulfillments_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
            ..
        } = invocation;
        let mut fields = match self.root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(_) => return resolver_http_error_outcome(400, "Could not parse GraphQL operation"),
        };
        fields.retain(|field| field.response_key == response_key);
        if let Some(field) = fields.first_mut() {
            field.arguments = arguments
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect();
        }
        let field = fields.first();
        match mode {
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                if matches!(root_field, "reverseDelivery" | "reverseFulfillmentOrder") {
                    if let Some(outcome) = self.order_return_local_runtime_outcome(
                        request,
                        root_field,
                        query,
                        variables,
                        response_key,
                    ) {
                        outcome
                    } else {
                        let data = delivery_settings_read_data(&fields);
                        ResolverOutcome::value(
                            data.get(response_key).cloned().unwrap_or(Value::Null),
                        )
                    }
                } else if fields.iter().all(|field| {
                    matches!(
                        field.name.as_str(),
                        "deliveryCustomization" | "deliveryCustomizations"
                    )
                }) {
                    let data = self.delivery_customization_query_data(&fields, Some(request));
                    ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
                } else if matches!(root_field, "carrierService" | "carrierServices") {
                    let data = self.carrier_service_read_data(&fields);
                    ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
                } else if matches!(root_field, "deliveryProfile" | "deliveryProfiles") {
                    self.delivery_profile_read_outcome(request, &fields, response_key)
                } else if matches!(
                    root_field,
                    "deliveryPromiseProvider" | "deliveryPromiseParticipants"
                ) {
                    self.delivery_promise_read_outcome(request, &fields, response_key)
                } else if root_field == "availableCarrierServices" {
                    // The shipping-settings availability read combines
                    // `availableCarrierServices` with the shipping-locations
                    // connection. Serve from observed/staged state, or (in live
                    // modes with no observed state yet) forward upstream and
                    // observe both carrier services and locations so later
                    // local-pickup mutations and reads resolve them locally.
                    self.shipping_settings_read_outcome(request, &fields, response_key)
                } else if root_field == "locationsAvailableForDeliveryProfilesConnection" {
                    // A standalone shipping-locations connection read: serve from
                    // observed/staged shipping locations, or (in live modes with no
                    // observed state yet) forward upstream and observe the result so
                    // later pickup mutations and reads resolve locally.
                    self.delivery_profile_locations_read_outcome(request, &fields, response_key)
                } else if matches!(root_field, "deliverySettings" | "deliveryPromiseSettings") {
                    self.delivery_settings_read_outcome(request, &fields, response_key)
                } else if let Some(data) = self.fulfillment_service_read_data(&fields) {
                    ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
                } else if matches!(
                    root_field,
                    "fulfillmentOrder"
                        | "fulfillmentOrders"
                        | "assignedFulfillmentOrders"
                        | "manualHoldsFulfillmentOrders"
                ) {
                    self.shipping_fulfillment_order_read_outcome(request, &fields, response_key)
                } else {
                    let data = delivery_settings_read_data(&fields);
                    ResolverOutcome::value(data.get(response_key).cloned().unwrap_or(Value::Null))
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "reverseDeliveryCreateWithShipping"
                            | "reverseDeliveryShippingUpdate"
                            | "reverseFulfillmentOrderDispose"
                    ) =>
            {
                if let Some(outcome) = self.order_return_local_runtime_outcome(
                    request,
                    root_field,
                    query,
                    variables,
                    response_key,
                ) {
                    // Reverse-logistics mutations are recorded in the mutation log so
                    // the staged session can be introspected/replayed; the return*
                    // lifecycle mutations (Orders domain) intentionally do not log.
                    outcome.with_log_draft(LogDraft::staged(
                        root_field,
                        "shipping-fulfillments",
                        Vec::new(),
                    ))
                } else {
                    resolver_http_error_outcome(
                        501,
                        format!(
                            "No Rust shipping-fulfillments resolver implemented for {root_field}"
                        ),
                    )
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "deliveryCustomizationActivation"
                            | "deliveryCustomizationCreate"
                            | "deliveryCustomizationDelete"
                            | "deliveryCustomizationUpdate"
                    ) =>
            {
                let result = self.delivery_customization_mutation_data(request, &fields);
                let mut outcome = ResolverOutcome::value(
                    result
                        .data
                        .get(response_key)
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                if !result.staged_ids.is_empty() {
                    outcome = outcome.with_log_draft(LogDraft::staged(
                        root_field,
                        "shipping-fulfillments",
                        result.staged_ids,
                    ));
                }
                outcome
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "deliveryPromiseProviderUpsert" | "deliveryPromiseParticipantsUpdate"
                    ) =>
            {
                self.delivery_promise_mutation_fields(fields, request, response_key)
                    .remove(response_key)
                    .unwrap_or_else(|| ResolverOutcome::value(Value::Null))
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "shippingPackageUpdate"
                            | "shippingPackageMakeDefault"
                            | "shippingPackageDelete"
                    ) =>
            {
                self.shipping_package_mutation(
                    root_field,
                    field.expect("shipping-package root field should be prepared"),
                    request,
                    response_key,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
                    ) =>
            {
                self.carrier_service_mutations(&fields, request, response_key)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentServiceCreate"
                            | "fulfillmentServiceUpdate"
                            | "fulfillmentServiceDelete"
                    ) =>
            {
                self.fulfillment_service_mutation(root_field, &fields, request, response_key)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrderMove" =>
            {
                self.shipping_fulfillment_order_mutation_outcome(
                    root_field,
                    request,
                    query,
                    variables,
                    response_key,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentOrderOpen" | "fulfillmentOrderReportProgress"
                    ) =>
            {
                self.shipping_fulfillment_order_mutation_outcome(
                    root_field,
                    request,
                    query,
                    variables,
                    response_key,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrdersSetFulfillmentDeadline" =>
            {
                self.shipping_fulfillment_order_mutation_outcome(
                    root_field,
                    request,
                    query,
                    variables,
                    response_key,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "deliveryProfileCreate" | "deliveryProfileUpdate" | "deliveryProfileRemove"
                    ) =>
            {
                self.delivery_profile_mutation(root_field, &fields, request, response_key)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "locationLocalPickupEnable" | "locationLocalPickupDisable"
                    ) =>
            {
                self.location_local_pickup_mutation(root_field, &fields, response_key)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentOrderHold"
                            | "fulfillmentOrderReleaseHold"
                            | "fulfillmentOrderCancel"
                            | "fulfillmentOrderClose"
                            | "fulfillmentOrderLineItemsPreparedForPickup"
                            | "fulfillmentOrderReschedule"
                            | "fulfillmentOrdersReroute"
                            | "fulfillmentOrderSplit"
                            | "fulfillmentOrderMerge"
                            | "fulfillmentOrderSubmitFulfillmentRequest"
                            | "fulfillmentOrderAcceptFulfillmentRequest"
                            | "fulfillmentOrderRejectFulfillmentRequest"
                            | "fulfillmentOrderSubmitCancellationRequest"
                            | "fulfillmentOrderAcceptCancellationRequest"
                            | "fulfillmentOrderRejectCancellationRequest"
                    ) =>
            {
                self.shipping_fulfillment_order_mutation_outcome(
                    root_field,
                    request,
                    query,
                    variables,
                    response_key,
                )
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                resolver_http_error_outcome(
                    501,
                    format!(
                        "No Rust {} resolver implemented for root field: {root_field}",
                        mode.registry_name()
                    ),
                )
            }
        }
    }
}

impl DraftProxy {
    pub(crate) fn resolve_admin_platform_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = invocation;
        match mode {
            LocalResolverMode::OverlayRead => self.admin_platform_query_outcome(
                request,
                query,
                variables,
                root_name,
                response_key,
            ),
            LocalResolverMode::StageLocally if root_name == "backupRegionUpdate" => {
                self.backup_region_update(request, query, variables)
            }
            LocalResolverMode::StageLocally
                if matches!(root_name, "flowGenerateSignature" | "flowTriggerReceive") =>
            {
                self.flow_utility_mutation(root_name, query, variables, response_key)
            }
            LocalResolverMode::StageLocally => resolver_http_error_outcome(
                501,
                format!(
                    "No Rust {} dispatcher implemented for root field: {root_name}",
                    mode.registry_name()
                ),
            ),
        }
    }
}

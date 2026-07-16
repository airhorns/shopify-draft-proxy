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
pub(in crate::proxy) use self::locations::{
    country_name_for_code, location_connection_json, location_country_code_is_valid,
    province_name_for_code,
};
pub(in crate::proxy) use self::publishable::{
    publishable_empty_string_publication_error,
    publishable_input_needs_publication_catalog_hydration, publishable_input_publication_ids,
};

impl DraftProxy {
    pub(in crate::proxy) fn admin_platform_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        let fields = match self.root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(response) => return response,
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
                ok_json(json!({ "data": data }))
            }
            "domain" => {
                if self.config.read_mode != ReadMode::Snapshot
                    && self.domain_query_needs_upstream(&fields)
                {
                    (self.upstream_transport)(request.clone())
                } else {
                    ok_json(json!({ "data": self.domain_query_data(&fields) }))
                }
            }
            "job" if self.should_handle_customer_overlay_read(&fields) => ok_json(json!({
                "data": self.customer_overlay_read_fields(request, &fields, None)
            })),
            "job" => ok_json(self.product_tail_job_query_body(&fields)),
            "node" | "nodes" => {
                let selection_errors = functions_output_selection_errors(query, variables, &fields);
                if !selection_errors.is_empty() {
                    return ok_json(json!({ "errors": selection_errors }));
                }
                let allow_unknown_null =
                    Self::node_fields_only_target_resource_type(&fields, "DeliveryCustomization");
                if let Some(data) =
                    self.local_node_query_data(&fields, allow_unknown_null, Some(request))
                {
                    ok_json(json!({ "data": data }))
                } else if self.config.read_mode != ReadMode::Snapshot {
                    // Resolve every cold/unsupported id in one copy of the caller's
                    // node operation. Known local values and tombstones are merged
                    // back over that response, so a mixed `nodes(ids:)` batch never
                    // loses staged cross-domain state merely because one id was cold.
                    let mut response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        response.body["data"] = self.node_query_data_with_upstream_fallback(
                            &fields,
                            &response.body,
                            Some(request),
                        );
                        // Merge against the pre-hydration store first. Observing
                        // Shopify's raw batch before this point would let stale
                        // upstream rows overwrite staged records or resurrect a
                        // local tombstone. Cache only the authoritative merged view.
                        self.observe_nodes_response(&response);
                    }
                    response
                } else {
                    ok_json(
                        json!({ "data": self.local_node_query_data(&fields, true, Some(request)).unwrap_or_else(|| Value::Object(serde_json::Map::new())) }),
                    )
                }
            }
            _ => unimplemented_root_response("admin-platform", root_field),
        }
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
    pub(in crate::proxy) fn resolve_shipping_fulfillments_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            operation,
            root_name: root_field,
            mode,
        } = context;
        match mode {
            LocalResolverMode::OverlayRead if operation.operation_type == OperationType::Query => {
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                if matches!(root_field, "reverseDelivery" | "reverseFulfillmentOrder") {
                    if let Some(data) =
                        self.order_return_local_runtime_data(request, root_field, query, variables)
                    {
                        ok_json(data)
                    } else {
                        ok_json(json!({ "data": delivery_settings_read_data(&fields) }))
                    }
                } else if fields.iter().all(|field| {
                    matches!(
                        field.name.as_str(),
                        "deliveryCustomization" | "deliveryCustomizations"
                    )
                }) {
                    ok_json(json!({
                        "data": self.delivery_customization_query_data(&fields, Some(request))
                    }))
                } else if matches!(root_field, "carrierService" | "carrierServices") {
                    ok_json(json!({ "data": self.carrier_service_read_data(&fields) }))
                } else if matches!(root_field, "deliveryProfile" | "deliveryProfiles") {
                    self.delivery_profile_read_response(request, &fields)
                } else if matches!(
                    root_field,
                    "deliveryPromiseProvider" | "deliveryPromiseParticipants"
                ) {
                    self.delivery_promise_read_response(request, &fields)
                } else if root_field == "availableCarrierServices" {
                    // The shipping-settings availability read combines
                    // `availableCarrierServices` with the shipping-locations
                    // connection. Serve from observed/staged state, or (in live
                    // modes with no observed state yet) forward upstream and
                    // observe both carrier services and locations so later
                    // local-pickup mutations and reads resolve them locally.
                    self.shipping_settings_read_response(request, &fields)
                } else if root_field == "locationsAvailableForDeliveryProfilesConnection" {
                    // A standalone shipping-locations connection read: serve from
                    // observed/staged shipping locations, or (in live modes with no
                    // observed state yet) forward upstream and observe the result so
                    // later pickup mutations and reads resolve locally.
                    self.delivery_profile_locations_read_response(request, &fields)
                } else if matches!(root_field, "deliverySettings" | "deliveryPromiseSettings") {
                    self.delivery_settings_read_response(request, &fields)
                } else if let Some(data) = self.fulfillment_service_read_data(&fields) {
                    ok_json(json!({ "data": data }))
                } else if matches!(
                    root_field,
                    "fulfillmentOrder"
                        | "fulfillmentOrders"
                        | "assignedFulfillmentOrders"
                        | "manualHoldsFulfillmentOrders"
                ) {
                    self.shipping_fulfillment_order_read_response(request, query, variables)
                } else {
                    ok_json(json!({ "data": delivery_settings_read_data(&fields) }))
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
                if let Some(data) =
                    self.order_return_local_runtime_data(request, root_field, query, variables)
                {
                    // Reverse-logistics mutations are recorded in the mutation log so
                    // the staged session can be introspected/replayed; the return*
                    // lifecycle mutations (Orders domain) intentionally do not log.
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        root_field,
                        Vec::new(),
                    );
                    ok_json(data)
                } else {
                    unimplemented_root_response("shipping-fulfillments", root_field)
                }
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "deliveryCustomizationActivation"
                                | "deliveryCustomizationCreate"
                                | "deliveryCustomizationDelete"
                                | "deliveryCustomizationUpdate"
                        )
                    }) =>
            {
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                let result = self.delivery_customization_mutation_data(request, &fields);
                if !result.staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        root_field,
                        result.staged_ids,
                    );
                }
                ok_json(json!({ "data": result.data }))
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "deliveryPromiseProviderUpsert" | "deliveryPromiseParticipantsUpdate"
                        )
                    }) =>
            {
                self.delivery_promise_mutation(query, variables, request)
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
                self.shipping_package_mutation(root_field, query, variables, request)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
                    ) =>
            {
                self.carrier_service_mutations(query, variables, request)
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
                self.fulfillment_service_mutation(root_field, query, variables, request)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrderMove" =>
            {
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentOrderOpen" | "fulfillmentOrderReportProgress"
                    ) =>
            {
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrdersSetFulfillmentDeadline" =>
            {
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "deliveryProfileCreate" | "deliveryProfileUpdate" | "deliveryProfileRemove"
                    ) =>
            {
                self.delivery_profile_mutation(root_field, query, variables, request)
            }
            LocalResolverMode::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "locationLocalPickupEnable" | "locationLocalPickupDisable"
                    ) =>
            {
                self.location_local_pickup_mutation(root_field, query, variables, request)
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
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            LocalResolverMode::OverlayRead | LocalResolverMode::StageLocally => {
                Self::unimplemented_resolver_response(mode, root_field)
            }
        }
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn resolve_admin_platform_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = context;
        match mode {
            LocalResolverMode::OverlayRead => {
                self.admin_platform_query_response(request, query, variables, root_name)
            }
            LocalResolverMode::StageLocally if root_name == "backupRegionUpdate" => {
                self.backup_region_update(request, query, variables)
            }
            LocalResolverMode::StageLocally
                if matches!(root_name, "flowGenerateSignature" | "flowTriggerReceive") =>
            {
                self.flow_utility_mutation(root_name, request, query, variables)
            }
            LocalResolverMode::StageLocally => {
                Self::unimplemented_resolver_response(mode, root_name)
            }
        }
    }
}

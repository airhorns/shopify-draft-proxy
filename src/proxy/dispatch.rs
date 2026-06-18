use super::*;

/// Predicates the local product-overlay search (`products_connection_value`)
/// can evaluate against partial staged state: an empty query, `status:`
/// filters, `sku:` lookups, and tag filters. Anything else (notably catalog
/// aggregates like `inventory_total:` or store-wide `vendor:` filters) requires
/// the full catalog and must be answered upstream.
fn catalog_search_predicate_is_locally_servable(predicate: &str) -> bool {
    let trimmed = predicate.trim();
    trimmed.is_empty()
        || trimmed.contains("status:")
        || trimmed.starts_with("sku:")
        || product_tag_query_value(trimmed).is_some()
}

impl DraftProxy {
    pub(in crate::proxy) fn finalize_mutation_outcome(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        outcome: MutationOutcome,
    ) -> Response {
        for draft in outcome.log_drafts {
            self.record_mutation_log_draft(request, query, variables, draft);
        }
        outcome.response
    }

    fn root_fields_or_error(
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Result<Vec<RootFieldSelection>, Response> {
        root_fields(query, variables)
            .ok_or_else(|| json_error(400, "Could not parse GraphQL operation"))
    }

    /// A `products`/`productsCount`/`productVariants` root carrying a `query:`
    /// search predicate the local overlay cannot faithfully evaluate (anything
    /// beyond `status:`/`sku:`/tag filters — e.g. `inventory_total:` or
    /// `vendor:`) needs the full store catalog. Answering it from partial
    /// overlay state would fabricate wrong matches, so such a query is forwarded
    /// upstream where the real backend (or a recorded cassette) resolves it
    /// authoritatively, even when unrelated overlay state has been staged.
    fn product_query_needs_upstream_catalog_search(
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(fields) = root_fields(query, variables) else {
            return false;
        };
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "products" | "productsCount" | "productVariants" | "productVariantsCount"
            ) && matches!(
                field.arguments.get("query"),
                Some(ResolvedValue::String(predicate))
                    if !catalog_search_predicate_is_locally_servable(predicate)
            )
        })
    }

    fn should_route_owner_metafields_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        self.should_handle_owner_metafields_read(query, variables)
            && root_fields(query, variables)
                .map(|fields| {
                    fields.iter().all(|field| {
                        matches!(
                            field.name.as_str(),
                            "product"
                                | "productVariant"
                                | "collection"
                                | "customer"
                                | "order"
                                | "company"
                        )
                    })
                })
                .unwrap_or(false)
    }

    fn admin_platform_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        let fields = match Self::root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(response) => return response,
        };
        match root_field {
            "backupRegion" => {
                let mut data = serde_json::Map::new();
                for field in fields {
                    if field.name == "backupRegion" {
                        data.insert(field.response_key, self.store.staged.backup_region.clone());
                    }
                }
                ok_json(json!({ "data": Value::Object(data) }))
            }
            "domain" => ok_json(json!({ "data": self.domain_query_data(&fields) })),
            "job" => ok_json(json!({ "data": self.product_tail_job_query_data(&fields) })),
            "node" | "nodes" => {
                if let Some(data) = self.local_node_query_data(&fields, false) {
                    ok_json(json!({ "data": data }))
                } else if self.config.read_mode != ReadMode::Snapshot {
                    // Cold read: forward upstream and hydrate the observed
                    // products/variants/collections into the base store so
                    // subsequent local mutations (e.g. productOptionsCreate)
                    // operate on a known owner — a read-through cache.
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.observe_nodes_response(&response);
                    }
                    response
                } else {
                    ok_json(
                        json!({ "data": self.local_node_query_data(&fields, true).unwrap_or_else(|| Value::Object(serde_json::Map::new())) }),
                    )
                }
            }
            _ => json_error(
                501,
                &format!(
                    "No Rust admin-platform dispatcher implemented for root field: {root_field}"
                ),
            ),
        }
    }

    fn orders_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        if is_shipping_fulfillment_order_local_order_read(query, variables) {
            return self.shipping_fulfillment_order_local_order_read(query, variables);
        }
        if let Some(data) = self.order_create_local_data(request, root_field, query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.draft_order_complete_local_data(root_field, query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.payment_terms_local_data(request, query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.draft_order_bulk_tag_local_data(query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.order_return_local_runtime_data(root_field, query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.abandonment_read_data(query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.remaining_order_local_data(request, root_field, query, variables) {
            return ok_json(data);
        }
        if self.config.read_mode != ReadMode::Snapshot {
            return (self.upstream_transport)(request.clone());
        }

        match Self::root_fields_or_error(query, variables) {
            Ok(fields) => {
                let mut data = serde_json::Map::new();
                for field in fields {
                    match field.name.as_str() {
                        "order" | "draftOrder" | "return" | "abandonment" => {
                            data.insert(field.response_key, Value::Null);
                        }
                        "orders" => {
                            data.insert(field.response_key, connection_json(Vec::new()));
                        }
                        "ordersCount" => {
                            data.insert(
                                field.response_key,
                                selected_json(
                                    &json!({
                                        "count": 0,
                                        "precision": "EXACT"
                                    }),
                                    &field.selection,
                                ),
                            );
                        }
                        _ => {}
                    }
                }
                ok_json(json!({ "data": Value::Object(data) }))
            }
            Err(response) => response,
        }
    }

    fn domain_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "domain" {
                continue;
            }
            let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let value = if id == "gid://shopify/Domain/1000" {
                selected_json(
                    &json!({
                        "id": "gid://shopify/Domain/1000",
                        "host": "acme.myshopify.com",
                        "url": "https://acme.myshopify.com",
                        "sslEnabled": true
                    }),
                    &field.selection,
                )
            } else {
                Value::Null
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn local_node_query_data(
        &self,
        fields: &[RootFieldSelection],
        allow_unknown_null: bool,
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.local_node_value_by_id(&id, &field.selection)
                        .or_else(|| allow_unknown_null.then_some(Value::Null))?
                }
                "nodes" => Value::Array(
                    field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| {
                            self.local_node_value_by_id(&id, &field.selection)
                                .or_else(|| allow_unknown_null.then_some(Value::Null))
                        })
                        .collect::<Option<Vec<_>>>()?,
                ),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(Value::Object(data))
    }

    fn abandonment_read_data(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| field.name == "abandonment") {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "abandonment" {
                continue;
            }
            let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let value = self
                .store
                .staged
                .abandonments
                .get(&id)
                .map(|record| selected_json(record, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn orders_stage_locally_unmodeled_shape_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        self.record_mutation_log_entry(request, query, variables, root_field, Vec::new());
        if let Some(entry) = self.log_entries.last_mut() {
            set_log_status(entry, "failed");
            entry["notes"] = json!(
                "Orders mutation root is registered for local staging, but this argument/selection shape is not modeled yet."
            );
            entry["interpreted"]["capability"] = json!({
                "operationName": root_field,
                "domain": "orders",
                "execution": "stage-locally"
            });
        }

        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        let response_key = field
            .as_ref()
            .map(|field| field.response_key.clone())
            .unwrap_or_else(|| root_field.to_string());
        let selection = field.map(|field| field.selection).unwrap_or_default();
        let payload = json!({
            "draftOrder": Value::Null,
            "calculatedDraftOrder": Value::Null,
            "order": Value::Null,
            "calculatedOrder": Value::Null,
            "refund": Value::Null,
            "return": Value::Null,
            "fulfillment": Value::Null,
            "fulfillmentOrder": Value::Null,
            "reverseFulfillmentOrder": Value::Null,
            "reverseDelivery": Value::Null,
            "job": Value::Null,
            "bulkOperation": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": format!(
                    "Local staging for {root_field} is not implemented for this request shape"
                ),
                "code": "NOT_IMPLEMENTED"
            }]
        });

        ok_json(json!({
            "data": {
                response_key: selected_json(&payload, &selection)
            }
        }))
    }

    fn local_node_value_by_id(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        if let Some(data) = local_node_value(id, selection, Some(&self.store.staged.backup_region))
        {
            return Some(data);
        }
        if shopify_gid_resource_type(id) == Some("ProductVariant") {
            let value = self.product_variant_by_id_value(id, selection);
            if !value.is_null() {
                return Some(value);
            }
        }
        if let Some(operation) = self.product_delete_operation_value_by_id(id, selection) {
            return Some(operation);
        }
        if let Some(segment) = self.store.staged.segments.get(id) {
            return Some(selected_json(segment, selection));
        }
        if let Some(query) = self.store.staged.customer_segment_member_queries.get(id) {
            return Some(selected_json(query, selection));
        }
        if let Some(abandonment) = self.store.staged.abandonments.get(id) {
            return Some(selected_json(abandonment, selection));
        }
        if let Some(value) = self.app_node_value_by_id(id, selection) {
            return Some(value);
        }
        if shopify_gid_resource_type(id) == Some("GiftCard") {
            return Some(
                self.store
                    .staged
                    .gift_cards
                    .get(id)
                    .map(|card| selected_json(card, selection))
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(cart_transform) = self.store.staged.function_cart_transforms.get(id) {
            return Some(selected_json(cart_transform, selection));
        }
        if let Some(cart_transform) = self
            .store
            .staged
            .function_cart_transform
            .as_ref()
            .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
        {
            return Some(selected_json(cart_transform, selection));
        }
        if let Some(discount) = self.discount_node_value_by_id(id, selection) {
            return Some(discount);
        }
        if let Some(b2b) = self.b2b_node_value_by_id(id, selection) {
            return Some(b2b);
        }
        None
    }

    fn app_node_value_by_id(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        match id {
            "gid://shopify/AppInstallation/expected" if self.store.staged.app_uninstalled => {
                Some(Value::Null)
            }
            "gid://shopify/AppInstallation/expected" => Some(current_app_installation_json(
                &self.store.staged.app_subscriptions,
                &self.store.staged.app_one_time_purchases,
                &self.store.staged.revoked_app_access_scopes,
                selection,
            )),
            "gid://shopify/App/expected" => Some(selected_json(&local_app_json(), selection)),
            _ => self
                .store
                .staged
                .app_subscriptions
                .get(id)
                .map(|subscription| {
                    selected_json(
                        subscription,
                        &selected_fields_named(
                            selection,
                            &["__typename", "id", "status", "trialDays", "lineItems"],
                        ),
                    )
                })
                .or_else(|| {
                    self.store
                        .staged
                        .app_one_time_purchases
                        .get(id)
                        .map(|purchase| {
                            selected_json(
                                purchase,
                                &selected_fields_named(
                                    selection,
                                    &["id", "name", "status", "test", "price"],
                                ),
                            )
                        })
                })
                .or_else(|| {
                    self.find_staged_app_usage_record(id).map(|usage_record| {
                        selected_json(
                            &usage_record,
                            &selected_fields_named(
                                selection,
                                &["id", "description", "price", "subscriptionLineItem"],
                            ),
                        )
                    })
                }),
        }
    }

    pub(in crate::proxy) fn record_mutation_log_draft(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        draft: LogDraft,
    ) {
        let root_field = draft.root_field;
        let staged_resource_ids = draft.staged_resource_ids;
        let status = draft.status;
        let capability_domain = draft.capability_domain;
        let capability_execution = draft.capability_execution;
        let notes = draft.notes;
        let root_fields = parse_operation(query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.clone()]);
        self.log_entries.push(json!({
            "id": format!("log-{}", self.log_entries.len() + 1),
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": root_field.clone(),
                "rootFields": root_fields,
                "primaryRootField": root_field.clone(),
                "capability": {
                    "operationName": root_field,
                    "domain": capability_domain,
                    "execution": capability_execution
                }
            },
            "notes": notes
        }));
    }

    pub(in crate::proxy) fn dispatch_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let query = graphql_request.query;
        let variables = graphql_request.variables;

        let Some(operation) = parse_operation(&query) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(root_field) = operation.primary_root_field() else {
            return json_error(400, "Operation has no root field");
        };

        let schema_input_errors = public_admin_schema_input_errors(&query, &variables);
        if !schema_input_errors.is_empty() {
            return ok_json(json!({ "errors": schema_input_errors }));
        }

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
        let has_local_dispatch = local_dispatch_root(
            operation.operation_type,
            capability.domain,
            capability.execution,
            root_field,
        )
        .is_some();
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if has_local_dispatch
                    && matches!(
                        root_field,
                        "product"
                            | "products"
                            | "productsCount"
                            | "productByIdentifier"
                            | "productOperation"
                            | "productVariant"
                    ) =>
            {
                if self.should_route_owner_metafields_read(&query, &variables) {
                    return self.owner_metafields_read(request, &query, &variables);
                }
                let has_inventory_fields = operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "inventoryItem"
                            | "inventoryItems"
                            | "inventoryLevel"
                            | "inventoryProperties"
                            | "inventoryTransfer"
                            | "inventoryTransfers"
                    )
                });
                let has_product_overlay_fields = operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "product"
                            | "products"
                            | "productsCount"
                            | "productByIdentifier"
                            | "productOperation"
                            | "productVariant"
                    )
                });
                if has_inventory_fields && !has_product_overlay_fields {
                    if let Some(fields) = root_fields(&query, &variables) {
                        ok_json(json!({ "data": self.inventory_query_data(&fields, &variables) }))
                    } else {
                        json_error(400, "Could not parse GraphQL operation")
                    }
                } else if Self::product_query_needs_upstream_catalog_search(&query, &variables) {
                    (self.upstream_transport)(request.clone())
                } else if self.has_product_overlay_state()
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    ok_json(json!({
                        "data": self.product_overlay_read_fields(&query, &variables)
                    }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && root_field == "productOperation" =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.product_operation_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "inventoryItem"
                            | "inventoryItems"
                            | "inventoryLevel"
                            | "inventoryProperties"
                            | "inventoryTransfer"
                            | "inventoryTransfers"
                    ) =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.inventory_query_data(&fields, &variables) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && matches!(root_field, "sellingPlanGroup" | "sellingPlanGroups") =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.selling_plan_group_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "publicationCreate"
                            | "publicationUpdate"
                            | "publicationDelete"
                            | "productFeedCreate"
                            | "productFullSync"
                            | "bulkProductResourceFeedbackCreate"
                            | "shopResourceFeedbackCreate"
                    ) =>
            {
                self.products_mutation_tail_helper_response(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| {
                    json_error(
                        501,
                        &format!(
                            "No Rust products dispatcher implemented for root field: {root_field}"
                        ),
                    )
                })
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productCreate" =>
            {
                let outcome = self.product_create(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productUpdate" =>
            {
                let outcome = self.product_update(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productDelete" =>
            {
                let outcome = self.product_delete(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productSet" =>
            {
                let outcome = self.product_set(&query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productDuplicate" =>
            {
                let outcome = self.product_duplicate(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch
                    && matches!(root_field, "productBundleCreate" | "productBundleUpdate") =>
            {
                let outcome = self.product_bundle_mutation(root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && root_field == "productChangeStatus" =>
            {
                let outcome = self.product_change_status(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch
                    && matches!(
                        root_field,
                        "productCreateMedia"
                            | "productUpdateMedia"
                            | "productDeleteMedia"
                            | "productReorderMedia"
                    ) =>
            {
                match root_fields(&query, &variables) {
                    Some(fields) => match self.product_media_mutation_data(request, &fields) {
                        Some(data) => {
                            self.record_mutation_log_entry(
                                request,
                                &query,
                                &variables,
                                root_field,
                                Vec::new(),
                            );
                            ok_json(json!({ "data": data }))
                        }
                        // Error scenarios (e.g. unstaged live products) fall
                        // through to the real upstream rather than a 501.
                        None => (self.upstream_transport)(request.clone()),
                    },
                    None => (self.upstream_transport)(request.clone()),
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch
                    && matches!(
                        root_field,
                        "collectionCreate"
                            | "collectionUpdate"
                            | "collectionDelete"
                            | "collectionAddProducts"
                            | "collectionAddProductsV2"
                            | "collectionRemoveProducts"
                            | "collectionReorderProducts"
                    ) =>
            {
                let outcome = self.collection_mutation(root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "productVariantCreate"
                            | "productVariantUpdate"
                            | "productVariantDelete"
                            | "productVariantAppendMedia"
                            | "productVariantDetachMedia"
                            | "productVariantsBulkCreate"
                            | "productVariantsBulkUpdate"
                            | "productVariantsBulkDelete"
                            | "productVariantsBulkReorder"
                    ) =>
            {
                let outcome =
                    self.product_variant_mutation(request, root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "sellingPlanGroupCreate"
                            | "sellingPlanGroupUpdate"
                            | "sellingPlanGroupDelete"
                            | "sellingPlanGroupAddProducts"
                            | "sellingPlanGroupRemoveProducts"
                            | "sellingPlanGroupAddProductVariants"
                            | "sellingPlanGroupRemoveProductVariants"
                            | "productJoinSellingPlanGroups"
                            | "productLeaveSellingPlanGroups"
                            | "productVariantJoinSellingPlanGroups"
                            | "productVariantLeaveSellingPlanGroups"
                    ) =>
            {
                // Validation scenarios reference live-store products/groups that
                // were never staged here; serve those from upstream rather than
                // fabricate an inaccurate userError from empty local state.
                if !self.selling_plan_mutation_serves_locally(root_field, &query, &variables) {
                    return (self.upstream_transport)(request.clone());
                }
                let outcome = self.selling_plan_group_mutation(root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "productOptionsCreate"
                            | "productOptionUpdate"
                            | "productOptionsDelete"
                            | "productOptionsReorder"
                    ) =>
            {
                let outcome = self.product_option_mutation(root_field, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if has_local_dispatch && matches!(root_field, "tagsAdd" | "tagsRemove") =>
            {
                let outcome = self.product_tags_mutation(root_field, &query, &variables, request);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "metafieldsSet" =>
            {
                match metafields_set_coercion_error(&query, &variables) {
                    Some(response) => response,
                    None => {
                        let outcome = self.owner_metafields_set(request, &query, &variables);
                        self.finalize_mutation_outcome(request, &query, &variables, outcome)
                    }
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "metafieldsDelete" =>
            {
                let outcome = self.owner_metafields_delete(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "inventoryAdjustQuantities"
                            | "inventorySetQuantities"
                            | "inventoryMoveQuantities"
                            | "inventoryActivate"
                            | "inventoryDeactivate"
                            | "inventoryBulkToggleActivation"
                            | "inventoryItemUpdate"
                            | "inventoryTransferCreate"
                            | "inventoryTransferCreateAsReadyToShip"
                            | "inventoryTransferMarkAsReadyToShip"
                            | "inventoryTransferEdit"
                            | "inventoryTransferSetItems"
                            | "inventoryTransferRemoveItems"
                            | "inventoryTransferDuplicate"
                            | "inventoryTransferCancel"
                            | "inventoryTransferDelete"
                    ) =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let outcome = self.inventory_mutation_data(request, &fields);
                    self.finalize_mutation_outcome(request, &query, &variables, outcome)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::OverlayRead)
                if has_local_dispatch =>
            {
                ok_json(json!({
                    "data": self.saved_search_overlay_read_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::StageLocally)
                if has_local_dispatch =>
            {
                if let Some(response) = saved_search_required_input_error(&query, &variables) {
                    return response;
                }
                let outcome = self.saved_search_mutation_fields(&query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.admin_platform_query_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "backupRegionUpdate" =>
            {
                self.backup_region_update(request, &query, &variables)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(root_field, "flowGenerateSignature" | "flowTriggerReceive") =>
            {
                self.flow_utility_mutation(root_field, request, &query, &variables)
            }
            (CapabilityDomain::Apps, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && root_field == "currentAppInstallation" =>
            {
                if self.store.staged.app_uninstalled
                    || !self.store.staged.app_subscriptions.is_empty()
                    || !self.store.staged.app_one_time_purchases.is_empty()
                    || !self.store.staged.revoked_app_access_scopes.is_empty()
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    if let Some(fields) = root_fields(&query, &variables) {
                        ok_json(json!({
                            "data": self.current_app_installation_read_data(&fields)
                        }))
                    } else {
                        json_error(400, "Could not parse GraphQL operation")
                    }
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Apps, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                match root_field {
                    "appSubscriptionCreate" => {
                        self.app_subscription_create(&query, &variables, request)
                    }
                    "appSubscriptionCancel" => {
                        self.app_subscription_cancel(&query, &variables, request)
                    }
                    "appSubscriptionTrialExtend" => {
                        self.app_subscription_trial_extend(&query, &variables, request)
                    }
                    "appSubscriptionLineItemUpdate" => {
                        self.app_subscription_line_item_update(&query, &variables, request)
                    }
                    "appUsageRecordCreate" => {
                        self.app_usage_record_create(&query, &variables, request)
                    }
                    "appPurchaseOneTimeCreate" => {
                        self.app_purchase_one_time_create(&query, &variables, request)
                    }
                    "appRevokeAccessScopes" => {
                        self.app_revoke_access_scopes(&query, &variables, request)
                    }
                    "delegateAccessTokenCreate" => {
                        self.delegate_access_token_create(&query, &variables, request)
                    }
                    "delegateAccessTokenDestroy" => {
                        self.delegate_access_token_destroy(&query, &variables, request)
                    }
                    "appUninstall" => self.app_uninstall(&query, &variables, request),
                    _ => json_error(
                        501,
                        &format!("No Rust apps dispatcher implemented for root field: {root_field}"),
                    ),
                }
            }
            (CapabilityDomain::OnlineStore, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.online_store_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::OnlineStore, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    self.online_store_mutation(&fields, request, &query, &variables)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if self.config.read_mode != ReadMode::Snapshot
                        && !self.has_local_metaobject_entry_state()
                    {
                        self.metaobject_live_hybrid_read(request, &fields)
                    } else {
                        ok_json(json!({ "data": self.metaobject_query_data(&fields, request) }))
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if self.metaobject_mutation_is_local(&fields) {
                        self.metaobject_mutation(&fields, request, &query, &variables)
                    } else {
                        // Target lives upstream (seeded/live-captured): forward so the
                        // real backend response is replayed instead of a synthetic one.
                        (self.upstream_transport)(request.clone())
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.bulk_operation_read_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "bulkOperationRunQuery" =>
            {
                self.bulk_operation_run_query(request, &query, &variables)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "bulkOperationRunMutation" =>
            {
                self.bulk_operation_run_mutation(request, &query, &variables)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "bulkOperationCancel" =>
            {
                self.bulk_operation_cancel(request, &query, &variables)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                self.discounts_query_response(request, &query, &variables)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                let outcome = self.discounts_mutation(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.gift_card_read_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    self.gift_card_mutation_response(&fields, request, &query, &variables)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if self.should_route_owner_metafields_read(&query, &variables) {
                    return self.owner_metafields_read(request, &query, &variables);
                }
                self.orders_query_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "abandonmentUpdateActivitiesDeliveryStatuses"
                    ) =>
            {
                if let Some(data) =
                    self.abandonment_delivery_status_local_data(request, &query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "orderCancel" =>
            {
                if let Some(data) = self.order_customer_error_paths_data(request, &query, &variables) {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "orderMarkAsPaid" | "refundCreate" | "orderEditBegin" | "orderEditCommit"
                    ) =>
            {
                if let Some(data) =
                    self.money_bag_presentment_local_data(request, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.order_payment_transaction_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) = self.remaining_order_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    self.orders_stage_locally_unmodeled_shape_response(
                        request, &query, &variables, root_field,
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "orderCreate" =>
            {
                if let Some(data) =
                    self.payment_terms_local_data(request, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.money_bag_presentment_local_data(request, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.order_payment_transaction_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.draft_order_complete_local_data(root_field, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.remaining_order_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) = self.order_create_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    self.customer_order_create(&query, &variables, request)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "orderUpdate" =>
            {
                if let Some(data) =
                    self.order_create_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(root_field, "draftOrderCreate" | "draftOrderInvoiceSend") =>
            {
                if let Some(response) =
                    self.draft_order_invoice_send_local_response(request, &query, &variables)
                {
                    response
                } else if let Some(data) =
                    self.draft_order_complete_local_data(root_field, &query, &variables)
                {
                    ok_json(data)
                } else if let Some(data) = self.draft_order_bulk_tag_local_data(&query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "draftOrderComplete" =>
            {
                if let Some(data) =
                    self.draft_order_complete_local_data(root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(root_field, "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags") =>
            {
                if let Some(data) = self.draft_order_bulk_tag_local_data(&query, &variables) {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "fulfillmentCancel"
                            | "fulfillmentTrackingInfoUpdate"
                            | "orderEditAddVariant"
                            | "orderEditSetQuantity"
                    ) =>
            {
                if let Some(data) = self.remaining_order_local_data(request, root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "returnCreate"
                            | "returnRequest"
                            | "returnApproveRequest"
                            | "returnDeclineRequest"
                            | "returnCancel"
                            | "returnClose"
                            | "returnReopen"
                            | "removeFromReturn"
                            | "returnProcess"
                    ) =>
            {
                if let Some(data) =
                    self.order_return_local_runtime_data(root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust orders dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(root_field, "orderCustomerSet" | "orderCustomerRemove") =>
            {
                if let Some(data) = self.order_customer_error_paths_data(request, &query, &variables) {
                    ok_json(data)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Payments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if root_field == "customerPaymentMethod" {
                        if let Some(data) =
                            self.customer_payment_method_local_data(request, &query, &variables)
                        {
                            ok_json(data)
                        } else {
                            ok_json(json!({ "data": finance_risk_no_data_read_data(&fields) }))
                        }
                    } else if operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "paymentCustomization" | "paymentCustomizations"
                        )
                    }) {
                        ok_json(json!({
                            "data": self.payment_customization_query_data(&fields)
                        }))
                    } else {
                        ok_json(json!({ "data": finance_risk_no_data_read_data(&fields) }))
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Payments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if matches!(
                        root_field,
                        "customerPaymentMethodCreditCardCreate"
                            | "customerPaymentMethodCreditCardUpdate"
                            | "customerPaymentMethodCreateFromDuplicationData"
                            | "customerPaymentMethodGetDuplicationData"
                            | "customerPaymentMethodGetUpdateUrl"
                            | "customerPaymentMethodPaypalBillingAgreementCreate"
                            | "customerPaymentMethodPaypalBillingAgreementUpdate"
                            | "customerPaymentMethodRemoteCreate"
                            | "customerPaymentMethodRevoke"
                            | "paymentReminderSend"
                    ) {
                        if root_field == "paymentReminderSend" {
                            if let Some(data) = payment_reminder_local_data(
                                &query,
                                &variables,
                                &mut self.store.staged.payment_reminder_schedule_ids,
                            ) {
                                return ok_json(data);
                            }
                        }
                        if let Some(data) =
                            self.customer_payment_method_local_data(request, &query, &variables)
                        {
                            return ok_json(data);
                        }
                        return json_error(
                            501,
                            &format!(
                                "No Rust payments dispatcher implemented for root field: {root_field}"
                            ),
                        );
                    }
                    if matches!(
                        root_field,
                        "paymentTermsCreate" | "paymentTermsUpdate" | "paymentTermsDelete"
                    ) {
                        if let Some(data) =
                            self.payment_terms_local_data(request, &query, &variables)
                        {
                            return ok_json(data);
                        }
                        return json_error(
                            501,
                            &format!(
                                "No Rust payments dispatcher implemented for root field: {root_field}"
                            ),
                        );
                    }
                    if matches!(
                        root_field,
                        "orderCapture" | "transactionVoid" | "orderCreateMandatePayment"
                    ) {
                        if let Some(data) =
                            self.order_payment_transaction_local_data(request, root_field, &query, &variables)
                        {
                            return ok_json(data);
                        }
                        return json_error(
                            501,
                            &format!(
                                "No Rust payments dispatcher implemented for root field: {root_field}"
                            ),
                        );
                    }
                    if operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "paymentCustomizationActivation"
                                | "paymentCustomizationCreate"
                                | "paymentCustomizationDelete"
                                | "paymentCustomizationUpdate"
                        )
                    }) {
                        let data = self.payment_customization_mutation_data(&fields);
                        let staged_ids = fields
                            .iter()
                            .filter_map(|field| {
                                data[field.response_key.as_str()]["paymentCustomization"]["id"]
                                    .as_str()
                                    .map(ToString::to_string)
                                    .or_else(|| {
                                        data[field.response_key.as_str()]["deletedId"]
                                            .as_str()
                                            .map(ToString::to_string)
                                    })
                            })
                            .collect();
                        self.record_mutation_log_entry(
                            request, &query, &variables, root_field, staged_ids,
                        );
                        ok_json(json!({ "data": data }))
                    } else {
                        json_error(
                            501,
                            &format!(
                                "No Rust payments dispatcher implemented for root field: {root_field}"
                            ),
                        )
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Marketing, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.marketing_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Marketing, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let response = self.marketing_mutation(&fields, request);
                    let staged_ids: Vec<String> = fields
                        .iter()
                        .filter_map(|field| {
                            response.body["data"][field.response_key.as_str()]
                                ["marketingActivity"]["id"]
                                .as_str()
                                .map(ToString::to_string)
                        })
                        .collect();
                    if !staged_ids.is_empty() {
                        self.record_mutation_log_entry(
                            request, &query, &variables, root_field, staged_ids,
                        );
                    }
                    response
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Webhooks, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.webhook_subscriptions_query_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Webhooks, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                self.webhook_mutation(request, &query, &variables)
            }
            (CapabilityDomain::Events, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": event_empty_read_data(&fields) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Localization, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                // Cold LiveHybrid reads forward verbatim upstream and hydrate the
                // base stores as a side effect (product existence, shop locales);
                // once a lifecycle has staged localization records we serve
                // locally (read-after-write).
                if self.config.read_mode == ReadMode::LiveHybrid
                    && self.localization_should_fetch_upstream(root_field)
                {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.hydrate_localization_from_upstream(&response.body);
                    }
                    return response;
                }
                if let Some(fields) = root_fields(&query, &variables) {
                    ok_json(json!({ "data": self.localization_query_data(&fields, request) }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Localization, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let data = self.localization_mutation_data(&fields);
                    self.record_mutation_log_entry(
                        request,
                        &query,
                        &variables,
                        root_field,
                        fields.iter().map(|field| field.response_key.clone()).collect(),
                    );
                    ok_json(json!({ "data": data }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Markets, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                // Cold LiveHybrid reads forward verbatim upstream and hydrate the
                // staged stores as a side effect; once a lifecycle has staged
                // markets-domain records we serve locally (read-after-write).
                if self.config.read_mode == ReadMode::LiveHybrid
                    && self.markets_should_fetch_upstream(root_field, &variables)
                {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.hydrate_markets_from_upstream(&response.body);
                    }
                    return response;
                }
                if let Some(fields) = root_fields(&query, &variables) {
                    if operation
                        .root_fields
                        .iter()
                        .all(|field| field == "webPresences")
                    {
                        return self.web_presence_helper_query(&query);
                    }
                    // A market-localizable resource read carries request-scoped
                    // staging (content/digest hydration), so it keeps its
                    // dedicated handler. Every other markets-domain read — even
                    // when it selects several entity roots at once (market +
                    // catalog + webPresences) — projects each field from its
                    // staged store via the unified overlay handler.
                    let data = if operation.root_fields.iter().any(|field| {
                        matches!(
                            field.as_str(),
                            "marketLocalizableResource"
                                | "marketLocalizableResources"
                                | "marketLocalizableResourcesByIds"
                        )
                    }) {
                        self.market_localization_query_data(&fields, request)
                    } else {
                        self.markets_overlay_query_data(&fields)
                    };
                    ok_json(json!({ "data": data }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Markets, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let data = if operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "marketLocalizationsRegister" | "marketLocalizationsRemove"
                        )
                    }) {
                        self.market_localization_mutation_preflight(&variables, request);
                        self.market_localization_mutation_data(&fields)
                    } else if operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete"
                        )
                    }) {
                        self.web_presence_mutation_preflight(&variables, request);
                        return self.web_presence_helper_mutation(
                            root_field,
                            &query,
                            &variables,
                            request,
                        );
                    } else if operation
                        .root_fields
                        .iter()
                        .all(|field| field == "quantityPricingByVariantUpdate")
                    {
                        return quantity_pricing_by_variant_update_response(&query, &variables);
                    } else if operation.root_fields.iter().all(|field| {
                        matches!(field.as_str(), "quantityRulesAdd" | "quantityRulesDelete")
                    }) {
                        return quantity_rules_mutation_response(root_field, &query, &variables);
                    } else if operation.root_fields.iter().any(|field| {
                        matches!(
                            field.as_str(),
                            "priceListCreate"
                                | "priceListUpdate"
                                | "priceListDelete"
                                | "priceListFixedPricesByProductUpdate"
                                | "priceListFixedPricesAdd"
                                | "priceListFixedPricesUpdate"
                                | "priceListFixedPricesDelete"
                        )
                    }) {
                        self.price_list_mutation_data(&fields, request, &query, &variables)
                    } else if operation.root_fields.iter().any(|field| {
                        matches!(
                            field.as_str(),
                            "catalogCreate" | "catalogUpdate" | "catalogDelete" | "catalogContextUpdate"
                        )
                    }) {
                        self.catalog_mutation_data(&fields, request, &query, &variables)
                    } else {
                        self.market_create_mutation_data(&fields, request, &query, &variables)
                    };
                    if operation.root_fields.iter().all(|field| {
                        matches!(
                            field.as_str(),
                            "marketLocalizationsRegister" | "marketLocalizationsRemove"
                        )
                    }) {
                        self.record_mutation_log_entry(
                            request,
                            &query,
                            &variables,
                            root_field,
                            fields.iter().map(|field| field.response_key.clone()).collect(),
                        );
                    }
                    ok_json(json!({ "data": data }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Functions, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    let data = self.functions_metadata_mutation_data(&fields);
                    self.record_mutation_log_entry(
                        request,
                        &query,
                        &variables,
                        root_field,
                        Vec::new(),
                    );
                    ok_json(json!({ "data": data }))
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Functions, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                // A cold function read (no validation/cart-transform staged this
                // session) forwards to the upstream so `shopifyFunctions` /
                // `shopifyFunction` reflect the shop's real installed functions
                // and their app ownership metadata. Once a lifecycle is staged we
                // serve locally (read-after-write / read-after-delete).
                if self.config.read_mode != ReadMode::Snapshot && !self.local_has_function_state() {
                    (self.upstream_transport)(request.clone())
                } else if let Some(fields) = root_fields(&query, &variables) {
                    let selection_errors =
                        cart_transform_selection_errors(&query, &variables, &fields);
                    if selection_errors.is_empty() {
                        ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }))
                    } else {
                        ok_json(json!({ "errors": selection_errors }))
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Metafields, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                // Cold LiveHybrid definition reads forward verbatim to the
                // upstream; only once a lifecycle has staged definitions do we
                // serve locally (read-after-write / read-after-delete).
                if self.config.read_mode != ReadMode::Snapshot
                    && !self.local_has_metafield_definition_state(&variables)
                {
                    (self.upstream_transport)(request.clone())
                } else {
                    self.metafield_definition_pinning_read(&query, &variables)
                }
            }
            (CapabilityDomain::Metafields, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "standardMetafieldDefinitionEnable" =>
            {
                self.standard_metafield_definition_enable(request, &query, &variables)
            }
            (CapabilityDomain::Metafields, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                self.metafield_definition_pinning_mutation(request, &query, &variables)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if root_field == "collection" {
                        if self.should_route_owner_metafields_read(&query, &variables) {
                            self.owner_metafields_read(request, &query, &variables)
                        } else if self.collection_read_needs_upstream(&fields) {
                            (self.upstream_transport)(request.clone())
                        } else {
                            ok_json(json!({
                                "data": self.collection_membership_downstream_read_data(&fields)
                            }))
                        }
                    } else if self.has_location_overlay_state()
                        || !self.location_read_needs_upstream(&fields)
                    {
                        self.location_read_response(&fields)
                    } else {
                        (self.upstream_transport)(request.clone())
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "publishablePublish"
                            | "publishableUnpublish"
                            | "publishablePublishToCurrentChannel"
                            | "publishableUnpublishToCurrentChannel"
                    ) =>
            {
                self.product_publishable_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(root_field, "locationAdd" | "locationActivate") =>
            {
                self.location_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "locationEdit" =>
            {
                // Edits to a location the proxy created/staged locally are applied
                // locally (address-code derivation, field merges) so subsequent
                // local reads reflect the change. Edits targeting a real upstream
                // location the proxy has not staged forward verbatim, preserving
                // the existing passthrough/replay behavior for those baselines.
                if self.location_edit_targets_all_staged(&query, &variables) {
                    self.location_edit(&query, &variables, request)
                } else {
                    self.dispatch_unknown_passthrough_or_legacy_error(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    )
                }
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "locationDeactivate" =>
            {
                self.location_deactivate(&query, &variables, request)
            }
            (CapabilityDomain::Segments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if root_field == "customerSegmentMembersQuery" {
                        ok_json(json!({
                            "data": self.customer_segment_members_query_read_data(&fields)
                        }))
                    } else {
                        ok_json(json!({ "data": self.segment_read_data(&fields) }))
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Segments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerSegmentMembersQueryCreate" =>
            {
                self.customer_segment_members_query_create(&query, &variables, request)
            }
            (CapabilityDomain::Segments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                self.segment_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if let Some(fields) = root_fields(&query, &variables) {
                    if matches!(root_field, "reverseDelivery" | "reverseFulfillmentOrder") {
                        if let Some(data) =
                            self.order_return_local_runtime_data(root_field, &query, &variables)
                        {
                            ok_json(data)
                        } else {
                            ok_json(json!({ "data": delivery_settings_read_data(&fields) }))
                        }
                    } else if matches!(root_field, "carrierService" | "carrierServices") {
                        ok_json(json!({ "data": self.carrier_service_read_data(&fields) }))
                    } else if let Some(data) = self.fulfillment_service_read_data(&fields) {
                        ok_json(json!({ "data": data }))
                    } else if root_field == "fulfillmentOrder"
                        && is_fulfillment_order_request_lifecycle_direct_read(&query, &variables)
                    {
                        self.fulfillment_order_request_lifecycle_direct_read(&query, &variables)
                    } else {
                        ok_json(json!({ "data": delivery_settings_read_data(&fields) }))
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "reverseDeliveryCreateWithShipping"
                            | "reverseDeliveryShippingUpdate"
                            | "reverseFulfillmentOrderDispose"
                    ) =>
            {
                if let Some(data) =
                    self.order_return_local_runtime_data(root_field, &query, &variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust shipping-fulfillments dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "shippingPackageUpdate" | "shippingPackageMakeDefault" | "shippingPackageDelete"
                    ) =>
            {
                self.shipping_package_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
                    ) =>
            {
                self.carrier_service_mutations(&query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "fulfillmentServiceCreate"
                            | "fulfillmentServiceUpdate"
                            | "fulfillmentServiceDelete"
                    ) =>
            {
                self.fulfillment_service_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "fulfillmentOrderMove" =>
            {
                self.fulfillment_order_move_assignment_status(&query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && matches!(
                        root_field,
                        "fulfillmentOrderOpen" | "fulfillmentOrderReportProgress"
                    ) =>
            {
                self.fulfillment_order_status_precondition(root_field, &query, &variables)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "fulfillmentOrdersSetFulfillmentDeadline" =>
            {
                self.fulfillment_order_set_deadline(&query, &variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if self.should_route_owner_metafields_read(&query, &variables) {
                    return self.owner_metafields_read(request, &query, &variables);
                }
                if let Some(fields) = root_fields(&query, &variables) {
                    if self.should_handle_customer_overlay_read(&fields) {
                        ok_json(json!({ "data": self.customer_overlay_read_fields(&fields) }))
                    } else {
                        (self.upstream_transport)(request.clone())
                    }
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerCreate" =>
            {
                self.customer_create(&query, &variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerUpdate" =>
            {
                self.customer_update(&query, &variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerDelete" =>
            {
                self.customer_delete(&query, &variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerMerge" =>
            {
                self.customer_merge(&query, &variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && has_local_dispatch
                    && root_field == "customerSet" =>
            {
                if let Some(response) = self.customer_set_guard_response(&query, &variables) {
                    response
                } else {
                    self.dispatch_unknown_passthrough_or_legacy_error(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    )
                }
            }
            (CapabilityDomain::B2b, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                match root_field {
                    "companyCreate"
                    | "companyUpdate"
                    | "companyLocationCreate"
                    | "companyLocationAssignAddress" => self
                        .b2b_company_tail_helper_response(
                            request,
                            &query,
                            &variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| {
                            json_error(
                                501,
                                &format!(
                                    "No Rust b2b dispatcher implemented for root field: {root_field}"
                                ),
                            )
                        }),
                    "companyLocationUpdate" => self
                        .b2b_location_buyer_experience_tail_helper_response(
                            request,
                            &query,
                            &variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| {
                            json_error(
                                501,
                                &format!(
                                    "No Rust b2b dispatcher implemented for root field: {root_field}"
                                ),
                            )
                        }),
                    "companyLocationTaxSettingsUpdate" => self
                        .b2b_tax_settings_tail_helper_response(
                            request,
                            &query,
                            &variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| {
                            json_error(
                                501,
                                &format!(
                                    "No Rust b2b dispatcher implemented for root field: {root_field}"
                                ),
                            )
                        }),
                    "companyAssignCustomerAsContact" => {
                        if let Some(response) = self
                            .b2b_assign_customer_as_contact_response(request, &query, &variables)
                        {
                            response
                        } else if let Some(data) =
                            self.order_customer_error_paths_data(request, &query, &variables)
                        {
                            ok_json(data)
                        } else {
                            json_error(
                                501,
                                &format!(
                                    "No Rust b2b dispatcher implemented for root field: {root_field}"
                                ),
                            )
                        }
                    }
                    "companyContactDelete" | "companyContactsDelete"
                    | "companyContactRemoveFromCompany" => self.b2b_contact_delete_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyContactCreate" => self.b2b_company_contact_create_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyAssignMainContact" => self.b2b_assign_main_contact_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyRevokeMainContact" => self.b2b_revoke_main_contact_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyDelete" | "companiesDelete" => self.b2b_company_delete_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyAddressDelete" => self.b2b_company_address_delete_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyLocationsDelete" => self.b2b_company_locations_delete_with_cascade(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    _ => json_error(
                        501,
                        &format!("No Rust b2b dispatcher implemented for root field: {root_field}"),
                    ),
                }
            }
            (CapabilityDomain::B2b, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && has_local_dispatch =>
            {
                if self.should_route_owner_metafields_read(&query, &variables) {
                    return self.owner_metafields_read(request, &query, &variables);
                }
                self
                    .b2b_location_buyer_experience_tail_helper_response(
                        request,
                        &query,
                        &variables,
                        operation.operation_type,
                        &operation.root_fields,
                    )
                    .or_else(|| {
                        self.b2b_company_tail_helper_response(
                            request,
                            &query,
                            &variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                    })
                    .unwrap_or_else(|| {
                        json_error(
                            501,
                            &format!(
                                "No Rust b2b overlay-read dispatcher implemented for root field: {root_field}"
                            ),
                        )
                    })
            }
            (CapabilityDomain::Media, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && has_local_dispatch
                    && root_field == "files" =>
            {
                self.media_files_read(&query, &variables)
            }
            (CapabilityDomain::Media, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation && has_local_dispatch =>
            {
                let outcome = self.media_mutation(root_field, request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::Unknown, CapabilityExecution::Passthrough) => self
                .dispatch_unknown_passthrough_or_legacy_error(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                    root_field,
                ),
            (_, CapabilityExecution::OverlayRead) => json_error(
                501,
                &format!(
                    "No Rust overlay-read dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::StageLocally) => json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::Passthrough) => json_error(
                501,
                &format!(
                    "No Rust passthrough dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
        }
    }
}

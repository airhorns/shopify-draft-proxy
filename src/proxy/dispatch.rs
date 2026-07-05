use super::*;

macro_rules! try_root_fields {
    ($query:expr, $variables:expr) => {
        match Self::root_fields_or_error($query, $variables) {
            Ok(fields) => fields,
            Err(response) => return response,
        }
    };
}

/// Catalog search predicates that the local product overlay cannot faithfully
/// evaluate from observed/staged state alone. Store-wide aggregate predicates
/// such as `inventory_total:` and `variants.price:` need Shopify's full catalog
/// index; serving them from a partial overlay fabricates wrong matches.
fn catalog_search_predicate_requires_full_catalog(predicate: &str) -> bool {
    let predicate = predicate.to_ascii_lowercase();
    predicate.contains("inventory_total:")
        || predicate.contains("variants.price:")
        || predicate.contains("metafields.")
}

fn no_dispatcher(domain: &str, root_field: &str) -> Response {
    json_error(
        501,
        &format!("No Rust {domain} dispatcher implemented for root field: {root_field}"),
    )
}

fn customer_payment_methods_only_read(fields: &[RootFieldSelection]) -> bool {
    !fields.is_empty()
        && fields.iter().all(|field| {
            field.name == "customer"
                && field
                    .selection
                    .iter()
                    .any(|selection| selection.name == "paymentMethods")
                && field
                    .selection
                    .iter()
                    .all(|selection| matches!(selection.name.as_str(), "id" | "paymentMethods"))
        })
}

fn observed_node_values(response: &Response) -> Vec<Value> {
    let mut nodes = response
        .body
        .pointer("/data/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(node) = response
        .body
        .pointer("/data/node")
        .filter(|node| node.is_object())
    {
        nodes.push(node.clone());
    }
    nodes
}

fn changed_draft_order_tag_ids(
    before: &BTreeMap<String, Vec<String>>,
    after: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    after
        .iter()
        .filter(|(id, tags)| before.get(*id) != Some(*tags))
        .map(|(id, _)| id.clone())
        .collect()
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
    /// search predicate the local overlay cannot faithfully evaluate — a
    /// store-wide catalog aggregate such as `inventory_total:` (see
    /// [`catalog_search_predicate_requires_full_catalog`]) — needs the full
    /// store catalog. Answering it from partial overlay state would fabricate
    /// wrong matches, so such a query is forwarded upstream where the real
    /// backend (or a recorded cassette) resolves it authoritatively, even when
    /// unrelated overlay state has been staged.
    fn product_query_needs_upstream_catalog_search(fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "products" | "productsCount" | "productVariants" | "productVariantsCount"
            ) && matches!(
                field.arguments.get("query"),
                Some(ResolvedValue::String(predicate))
                    if catalog_search_predicate_requires_full_catalog(predicate)
            )
        })
    }

    fn product_read_needs_upstream(&self, fields: &[RootFieldSelection]) -> bool {
        if Self::product_query_needs_upstream_catalog_search(fields) {
            return true;
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        fields
            .iter()
            .any(|field| self.live_hybrid_product_field_needs_upstream(field))
    }

    fn live_hybrid_product_field_needs_upstream(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "products" | "productsCount" => true,
            "product" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                id.is_empty()
                    || (!self.store.has_product(&id) && !self.store.product_is_tombstoned(&id))
            }
            "productByIdentifier" => !self.product_identifier_has_local_answer(field),
            _ => false,
        }
    }

    fn product_identifier_has_local_answer(&self, field: &RootFieldSelection) -> bool {
        let Some(identifier) = resolved_object_field(&field.arguments, "identifier") else {
            return false;
        };
        if let Some(id) = resolved_string_field(&identifier, "id") {
            return self.store.has_product(&id) || self.store.product_is_tombstoned(&id);
        }
        if let Some(handle) = resolved_string_field(&identifier, "handle") {
            return self.store.product_by_handle(&handle).is_some();
        }
        false
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
                                | "shop"
                        )
                    })
                })
                .unwrap_or(false)
    }

    fn products_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        if self.should_route_owner_metafields_read(query, variables) {
            return self.owner_metafields_read(request, query, variables);
        }
        match root_field {
            "product"
            | "products"
            | "productsCount"
            | "productByIdentifier"
            | "productOperation"
            | "productFeed"
            | "productFeeds"
            | "productVariant" => {
                let fields = root_fields(query, variables).unwrap_or_default();
                if self.product_read_needs_upstream(&fields) {
                    (self.upstream_transport)(request.clone())
                } else if self.has_product_overlay_state()
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    let fields = root_fields(query, variables).unwrap_or_default();
                    if product_root_fields_select_shop_currency_money(&fields) {
                        self.hydrate_shop_pricing_state_if_missing(request, true, false);
                    }
                    // An overlay read reproduces staged inventory levels but not the
                    // opaque pagination cursors Shopify assigns each level edge: the
                    // node-hydrate warm path selects `inventoryLevels { nodes }`, never
                    // `edges { cursor }`, so cursors are never observed. When the client
                    // selects level edge/pageInfo cursors and none have been observed,
                    // forward this exact read upstream once and observe the real cursors
                    // before serving, so the overlay read can fill them in for real
                    // instead of relying on seeded cursor state.
                    self.hydrate_inventory_level_cursors_for_read(request, query);
                    ok_json(json!({
                        "data": self.product_overlay_read_data(&fields)
                    }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            "inventoryItem"
            | "inventoryItems"
            | "inventoryLevel"
            | "inventoryProperties"
            | "inventoryTransfer"
            | "inventoryTransfers"
            | "inventoryShipment" => {
                let fields = try_root_fields!(query, variables);
                ok_json(json!({ "data": self.inventory_query_data(&fields, variables) }))
            }
            "sellingPlanGroup" | "sellingPlanGroups" => {
                let fields = try_root_fields!(query, variables);
                if product_root_fields_select_shop_currency_money(&fields) {
                    self.hydrate_shop_pricing_state_if_missing(request, true, false);
                }
                ok_json(json!({ "data": self.selling_plan_group_query_data(&fields) }))
            }
            "collections" | "collectionsCount" => {
                if self.config.read_mode == ReadMode::LiveHybrid
                    && self.store.has_collection_state()
                {
                    self.hydrate_collections_for_read(request);
                }
                if self.store.has_collection_state() {
                    let fields = try_root_fields!(query, variables);
                    ok_json(json!({ "data": self.product_overlay_read_data(&fields) }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            "publication"
            | "channel"
            | "channels"
            | "publicationsCount"
            | "publishedProductsCount" => {
                // Only a scenario that seeded publications is served locally; the
                // whole multi-root publication read (publication/channel/channels/
                // counts plus any product/collection publication fields) is
                // rendered from local state. Otherwise these roots forward upstream
                // as before.
                if !self.publication_engine_active() {
                    (self.upstream_transport)(request.clone())
                } else {
                    let fields = try_root_fields!(query, variables);
                    ok_json(json!({ "data": self.publication_roots_read_data(&fields) }))
                }
            }
            _ => no_dispatcher("overlay-read", root_field),
        }
    }

    fn admin_platform_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        let fields = try_root_fields!(query, variables);
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
            "job" => ok_json(self.product_tail_job_query_body(&fields)),
            "node" | "nodes" => {
                let selection_errors = functions_output_selection_errors(query, variables, &fields);
                if !selection_errors.is_empty() {
                    return ok_json(json!({ "errors": selection_errors }));
                }
                if let Some(data) = self.local_node_query_data(&fields, false, Some(request)) {
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
                        json!({ "data": self.local_node_query_data(&fields, true, Some(request)).unwrap_or_else(|| Value::Object(serde_json::Map::new())) }),
                    )
                }
            }
            _ => no_dispatcher("admin-platform", root_field),
        }
    }

    fn orders_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        if root_field == "order"
            && self.should_handle_shipping_fulfillment_order_local_order_read(query, variables)
        {
            return self.shipping_fulfillment_order_local_order_read(query, variables);
        }
        if let Some(data) = self.order_create_local_data(request, root_field, query, variables) {
            return ok_json(data);
        }
        if let Some(response) = self.draft_order_lifecycle_local_response(request, query, variables)
        {
            return response;
        }
        if let Some(data) =
            self.draft_order_complete_local_data(request, root_field, query, variables)
        {
            return ok_json(data);
        }
        if let Some(data) = self.payment_terms_local_data(request, query, variables) {
            return ok_json(data);
        }
        if let Some(data) = self.draft_order_bulk_tag_local_data(query, variables) {
            return ok_json(data);
        }
        if let Some(data) =
            self.order_return_local_runtime_data(request, root_field, query, variables)
        {
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

        let fields = try_root_fields!(query, variables);
        let data = root_payload_json(&fields, |field| match field.name.as_str() {
            "order" | "draftOrder" | "return" | "abandonment" => Some(Value::Null),
            "orders" => Some(connection_json(Vec::new())),
            "ordersCount" => Some(selected_json(&count_object(0), &field.selection)),
            _ => None,
        });
        ok_json(json!({ "data": data }))
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

    pub(in crate::proxy) fn local_node_query_data(
        &self,
        fields: &[RootFieldSelection],
        allow_unknown_null: bool,
        request: Option<&Request>,
    ) -> Option<Value> {
        let mut missing_required = false;
        let data = root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    match self
                        .local_node_value_by_id_with_request(&id, &field.selection, request)
                        .or_else(|| allow_unknown_null.then_some(Value::Null))
                    {
                        Some(value) => value,
                        None => {
                            missing_required = true;
                            return None;
                        }
                    }
                }
                "nodes" => Value::Array(
                    field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| {
                            self.local_node_value_by_id_with_request(&id, &field.selection, request)
                                .or_else(|| allow_unknown_null.then_some(Value::Null))
                        })
                        .collect::<Option<Vec<_>>>()
                        .unwrap_or_else(|| {
                            missing_required = true;
                            Vec::new()
                        }),
                ),
                _ => return None,
            };
            Some(value)
        });
        (!missing_required).then_some(data)
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

        let data = root_payload_json(&fields, |field| {
            if field.name != "abandonment" {
                return None;
            }
            let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let value = self
                .store
                .staged
                .abandonments
                .get(&id)
                .map(|record| selected_json(record, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        });
        Some(json!({ "data": data }))
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

    pub(in crate::proxy) fn local_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.local_node_value_by_id_with_request(id, selection, None)
    }

    fn local_node_value_by_id_with_request(
        &self,
        id: &str,
        selection: &[SelectedField],
        request: Option<&Request>,
    ) -> Option<Value> {
        if let Some(data) = self.shop_property_node_value_by_id(id, selection) {
            return Some(data);
        }
        if let Some(data) = local_node_value(id, selection, Some(&self.store.staged.backup_region))
        {
            return Some(data);
        }
        if shopify_gid_resource_type(id) == Some("Product") {
            if self.store.product_is_tombstoned(id) {
                return Some(Value::Null);
            }
            if let Some(product) = self.store.product_by_id(id) {
                let variants = self.store.product_variants_for_product(id);
                let shop_currency_code = self.store.shop_currency_code();
                return Some(self.product_json_with_variants_and_currency_context(
                    product,
                    &variants,
                    selection,
                    &shop_currency_code,
                ));
            }
        }
        if shopify_gid_resource_type(id) == Some("Collection") {
            if self.store.collection_is_deleted(id) {
                return Some(Value::Null);
            }
            if let Some(collection) = self.store.collection_by_id(id) {
                return Some(self.collection_json_with_publication_fields(collection, selection));
            }
        }
        if shopify_gid_resource_type(id) == Some("ProductVariant") {
            let value = self.product_variant_by_id_value(id, selection);
            if !value.is_null() {
                return Some(value);
            }
        }
        if shopify_gid_resource_type(id) == Some("Location") {
            if self.store.staged.locations.is_tombstoned(id) {
                return Some(Value::Null);
            }
            if let Some(location) = self.location_for_read(id) {
                return Some(selected_json(&location, selection));
            }
        }
        if shopify_gid_resource_type(id) == Some("ProductFeed") {
            if let Some(value) = self.product_tail_feed_node_value(id, selection) {
                return Some(value);
            }
        }
        if let Some(operation) = self.product_delete_operation_value_by_id(id, selection) {
            return Some(operation);
        }
        if is_product_operation_gid(id) {
            return Some(
                self.store
                    .staged
                    .product_operations
                    .get(id)
                    .map(|operation| self.product_operation_json(operation, selection))
                    .unwrap_or(Value::Null),
            );
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
        if let Some(value) = self.app_node_value_by_id(id, selection, request) {
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
        if let Some(function) = self.store.staged.function_metadata.get(id) {
            return Some(selected_json(function, selection));
        }
        if let Some(validation) = self.store.staged.function_validations.get(id) {
            return Some(selected_json(
                &validation_record_for_selection(validation, selection),
                selection,
            ));
        }
        if let Some(validation) = self
            .store
            .staged
            .function_validation
            .as_ref()
            .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
        {
            return Some(selected_json(
                &validation_record_for_selection(validation, selection),
                selection,
            ));
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
        if let Some(rule) = self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .get(id)
        {
            return Some(selected_json(
                &fulfillment_constraint_rule_record_for_selection(rule, selection),
                selection,
            ));
        }
        if let Some(configuration) = self
            .store
            .staged
            .tax_app_configuration
            .as_ref()
            .filter(|configuration| configuration["id"].as_str() == Some(id))
        {
            return Some(selected_json(configuration, selection));
        }
        if let Some(discount) = self.discount_node_value_by_id(id, selection) {
            return Some(discount);
        }
        if let Some(file) = self.store.staged.media_files.get(id) {
            return Some(selected_json(file, selection));
        }
        if matches!(
            shopify_gid_resource_type(id),
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile")
        ) && self.store.staged.media_files.is_tombstoned(id)
        {
            return Some(Value::Null);
        }
        if let Some(b2b) = self.b2b_node_value_by_id(id, selection) {
            return Some(b2b);
        }
        if let Some(value) = self.online_store_content_node_value(id, selection) {
            return Some(value);
        }
        None
    }

    pub(in crate::proxy) fn observe_nodes_response(&mut self, response: &Response) {
        let nodes = observed_node_values(response);
        for node in &nodes {
            self.observe_node_response_value(node);
        }
        for node in nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
            if is_shopify_gid_of_type(id, "Collection") {
                self.stage_collection_from_observed_json(&node);
            }
        }
    }

    fn observe_node_response_value(&mut self, node: &Value) {
        let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
        if is_shopify_gid_of_type(id, "Product") {
            self.store.stage_observed_product_json(node);
            if let Some(product_id) = node.get("id").and_then(Value::as_str) {
                for variant in node
                    .get("variants")
                    .and_then(|connection| connection.get("nodes"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let mut variant_value = variant.clone();
                    if let Some(object) = variant_value.as_object_mut() {
                        object.insert("productId".to_string(), json!(product_id));
                    }
                    if let Some(mut variant) =
                        product_variant_state_from_observed_json(&variant_value)
                    {
                        variant.product_id = product_id.to_string();
                        self.store.stage_product_variant(variant);
                    }
                }
            }
        } else if is_shopify_gid_of_type(id, "Collection") {
            self.stage_collection_from_observed_json(node);
        } else if is_shopify_gid_of_type(id, "ProductVariant") {
            if let Some(variant) = product_variant_state_from_observed_json(node) {
                self.store.stage_product_variant(variant);
            }
            if let Some(product) = node.get("product").and_then(product_state_from_json) {
                self.store.stage_observed_product(product);
            }
        } else if is_shopify_gid_of_type(id, "InventoryItem") {
            self.observe_inventory_item_node(node);
        } else if is_shopify_gid_of_type(id, "InventoryLevel") {
            self.observe_inventory_level_node(node);
        } else if shopify_gid_resource_type(id) == Some("Location") {
            self.merge_staged_location(node, &[]);
        } else if matches!(
            shopify_gid_resource_type(id),
            Some("ShopAddress" | "ShopPolicy")
        ) {
            self.observe_shop_property_node(node);
        }
    }

    fn app_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
        request: Option<&Request>,
    ) -> Option<Value> {
        for (app_id, installation) in &self.store.staged.installed_apps {
            if app_installation_id(installation).as_deref() == Some(id) {
                if self.store.staged.uninstalled_app_ids.contains(app_id) {
                    return Some(Value::Null);
                }
                let revoked_access_scopes = self
                    .store
                    .staged
                    .revoked_app_access_scopes
                    .get(app_id)
                    .cloned()
                    .unwrap_or_default();
                return Some(current_app_installation_json(
                    installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                    selection,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation
                    .get("app")
                    .map(|app| selected_json(app, selection));
            }
        }
        if let Some(request) = request {
            let app_id = request_app_gid(request);
            let installation = current_app_installation_from_request(request);
            if app_installation_id(&installation).as_deref() == Some(id) {
                if self.store.staged.uninstalled_app_ids.contains(&app_id) {
                    return Some(Value::Null);
                }
                let revoked_access_scopes = self
                    .store
                    .staged
                    .revoked_app_access_scopes
                    .get(&app_id)
                    .cloned()
                    .unwrap_or_default();
                return Some(current_app_installation_json(
                    &installation,
                    &self.store.staged.app_subscriptions,
                    &self.store.staged.app_one_time_purchases,
                    &revoked_access_scopes,
                    selection,
                ));
            }
            if installation.pointer("/app/id").and_then(Value::as_str) == Some(id) {
                return installation
                    .get("app")
                    .map(|app| selected_json(app, selection));
            }
        }
        self.store
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
            })
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

    fn dispatch_capability_fallback(execution: CapabilityExecution, root_field: &str) -> Response {
        no_dispatcher(execution.registry_name(), root_field)
    }

    fn dispatch_products_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match (CapabilityDomain::Products, execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                self.products_query_response(request, query, variables, root_field)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "publicationCreate"
                            | "publicationUpdate"
                            | "publicationDelete"
                            | "productFeedCreate"
                            | "productFeedDelete"
                            | "productFullSync"
                            | "combinedListingUpdate"
                            | "productVariantRelationshipBulkUpdate"
                            | "bulkProductResourceFeedbackCreate"
                            | "shopResourceFeedbackCreate"
                    ) =>
            {
                self.products_mutation_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| no_dispatcher("products", root_field))
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productCreate" =>
            {
                let outcome = self.product_create(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productUpdate" =>
            {
                let outcome = self.product_update(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productDelete" =>
            {
                let outcome = self.product_delete(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productSet" =>
            {
                let outcome = self.product_set(query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productDuplicate" =>
            {
                let outcome = self.product_duplicate(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(root_field, "productBundleCreate" | "productBundleUpdate") =>
            {
                let outcome = self.product_bundle_mutation(root_field, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(root_field, "productPublish" | "productUnpublish") =>
            {
                let outcome =
                    self.product_publication_mutation(root_field, query, variables, request);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productChangeStatus" =>
            {
                let outcome = self.product_change_status(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(
                    root_field,
                    "productCreateMedia"
                        | "productUpdateMedia"
                        | "productDeleteMedia"
                        | "productReorderMedia"
                ) =>
            {
                // Media staging is store-backed: in Snapshot mode (unit tests) no
                // upstream product has been observed, so there is nothing to stage
                // media onto. Fail closed exactly like an unrouted mutation rather
                // than fabricate a baked media payload from empty local state.
                if self.config.read_mode == ReadMode::Snapshot {
                    self.dispatch_unknown_passthrough_or_legacy_error(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    )
                } else {
                    match root_fields(query, variables) {
                        Some(fields) => match self.product_media_mutation_data(request, &fields) {
                            Some(data) => {
                                self.record_mutation_log_entry(
                                    request,
                                    query,
                                    variables,
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
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "productVariantAppendMedia" | "productVariantDetachMedia"
                    ) =>
            {
                // Variant media attach/detach is store-backed against the owning
                // product's staged variants. Snapshot mode has nothing staged, so
                // fail closed; LiveHybrid stages through the variant mutation path.
                if self.config.read_mode == ReadMode::Snapshot {
                    self.dispatch_unknown_passthrough_or_legacy_error(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    )
                } else {
                    let outcome =
                        self.product_variant_mutation(request, root_field, query, variables);
                    self.finalize_mutation_outcome(request, query, variables, outcome)
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(
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
                let outcome = self.collection_mutation(root_field, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "productVariantCreate"
                            | "productVariantUpdate"
                            | "productVariantDelete"
                            | "productVariantsBulkCreate"
                            | "productVariantsBulkUpdate"
                            | "productVariantsBulkDelete"
                            | "productVariantsBulkReorder"
                    ) =>
            {
                let outcome = self.product_variant_mutation(request, root_field, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
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
                if !self.selling_plan_mutation_serves_locally(root_field, query, variables) {
                    return (self.upstream_transport)(request.clone());
                }
                let outcome = self.selling_plan_group_mutation(root_field, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "productOptionsCreate"
                            | "productOptionUpdate"
                            | "productOptionsDelete"
                            | "productOptionsReorder"
                    ) =>
            {
                let outcome = self.product_option_mutation(root_field, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(root_field, "tagsAdd" | "tagsRemove") =>
            {
                let outcome = self.product_tags_mutation(root_field, query, variables, request);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "metafieldsSet" =>
            {
                match metafields_set_coercion_error(query, variables) {
                    Some(response) => response,
                    None => {
                        let outcome = self.owner_metafields_set(request, query, variables);
                        self.finalize_mutation_outcome(request, query, variables, outcome)
                    }
                }
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "metafieldsDelete" =>
            {
                let outcome = self.owner_metafields_delete(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "metafieldDelete" =>
            {
                let outcome = self.owner_metafield_delete(request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "inventoryAdjustQuantities"
                            | "inventorySetQuantities"
                            | "inventorySetOnHandQuantities"
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
                            | "inventoryShipmentCreate"
                            | "inventoryShipmentCreateInTransit"
                            | "inventoryShipmentAddItems"
                            | "inventoryShipmentRemoveItems"
                            | "inventoryShipmentUpdateItemQuantities"
                            | "inventoryShipmentSetTracking"
                            | "inventoryShipmentMarkInTransit"
                            | "inventoryShipmentReceive"
                            | "inventoryShipmentDelete"
                    ) =>
            {
                let fields = try_root_fields!(query, variables);
                let outcome = self.inventory_mutation_data(request, &fields);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (_, CapabilityExecution::OverlayRead) | (_, CapabilityExecution::StageLocally) => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }

    fn dispatch_orders_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match (CapabilityDomain::Orders, execution) {
            (CapabilityDomain::Orders, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                if let Some(data) =
                    self.order_return_local_runtime_data(request, root_field, query, variables)
                {
                    return ok_json(data);
                }
                if self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables);
                }
                self.orders_query_response(request, query, variables, root_field)
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "abandonmentUpdateActivitiesDeliveryStatuses") =>
            {
                if let Some(data) =
                    self.abandonment_delivery_status_local_data(request, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderCancel" =>
            {
                if let Some(data) = self.order_customer_error_paths_data(request, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderDelete" =>
            {
                if let Some(data) =
                    self.remaining_order_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "orderMarkAsPaid" | "refundCreate" | "orderEditBegin" | "orderEditCommit"
                    ) =>
            {
                if let Some(data) = self.money_bag_presentment_local_data(request, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.refund_create_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.order_payment_transaction_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.remaining_order_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    self.orders_stage_locally_unmodeled_shape_response(
                        request, query, variables, root_field,
                    )
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderCreate" =>
            {
                if let Some(data) = self.payment_terms_local_data(request, query, variables) {
                    ok_json(data)
                } else if let Some(data) =
                    self.money_bag_presentment_local_data(request, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.order_payment_transaction_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.draft_order_complete_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.remaining_order_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(data) =
                    self.order_create_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    self.customer_order_create(query, variables, request)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderUpdate" =>
            {
                if let Some(data) =
                    self.order_create_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "orderClose" | "orderOpen") =>
            {
                if let Some(data) =
                    self.order_create_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "draftOrderCreate"
                            | "draftOrderInvoiceSend"
                            | "draftOrderUpdate"
                            | "draftOrderCalculate"
                            | "draftOrderDuplicate"
                            | "draftOrderDelete"
                            | "draftOrderBulkDelete"
                            | "draftOrderCreateFromOrder"
                            | "draftOrderInvoicePreview"
                    ) =>
            {
                if let Some(response) =
                    self.draft_order_invoice_send_local_response(request, query, variables)
                {
                    response
                } else if let Some(data) =
                    self.draft_order_complete_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else if let Some(response) =
                    self.draft_order_lifecycle_local_response(request, query, variables)
                {
                    response
                } else if let Some(data) = self.draft_order_bulk_tag_local_data(query, variables) {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "draftOrderComplete" =>
            {
                if let Some(data) =
                    self.draft_order_complete_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
                    ) =>
            {
                let before_tags = self.store.staged.draft_order_tags.clone();
                if let Some(data) = self.draft_order_bulk_tag_local_data(query, variables) {
                    let staged_ids = changed_draft_order_tag_ids(
                        &before_tags,
                        &self.store.staged.draft_order_tags,
                    );
                    if !staged_ids.is_empty() {
                        self.record_mutation_log_entry(
                            request, query, variables, root_field, staged_ids,
                        );
                    }
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentCreate"
                            | "fulfillmentCreateV2"
                            | "fulfillmentCancel"
                            | "fulfillmentTrackingInfoUpdate"
                            | "fulfillmentEventCreate"
                            | "orderEditAddVariant"
                            | "orderEditSetQuantity"
                            | "orderEditAddCustomItem"
                            | "orderEditAddLineItemDiscount"
                            | "orderEditRemoveDiscount"
                            | "orderEditAddShippingLine"
                            | "orderEditUpdateShippingLine"
                            | "orderEditRemoveShippingLine"
                    ) =>
            {
                if let Some(data) =
                    self.remaining_order_local_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
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
                    self.order_return_local_runtime_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
                }
            }
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "orderCustomerSet" | "orderCustomerRemove") =>
            {
                if let Some(data) = self.order_customer_error_paths_data(request, query, variables)
                {
                    ok_json(data)
                } else {
                    json_error(400, "Could not parse GraphQL operation")
                }
            }
            (_, CapabilityExecution::OverlayRead) | (_, CapabilityExecution::StageLocally) => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }

    fn dispatch_shipping_fulfillments_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match (CapabilityDomain::ShippingFulfillments, execution) {
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(query, variables);
                if matches!(root_field, "reverseDelivery" | "reverseFulfillmentOrder") {
                    if let Some(data) =
                        self.order_return_local_runtime_data(request, root_field, query, variables)
                    {
                        ok_json(data)
                    } else {
                        ok_json(json!({ "data": delivery_settings_read_data(&fields) }))
                    }
                } else if matches!(root_field, "carrierService" | "carrierServices") {
                    ok_json(json!({ "data": self.carrier_service_read_data(&fields) }))
                } else if matches!(root_field, "deliveryProfile" | "deliveryProfiles") {
                    self.delivery_profile_read_response(request, &fields)
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
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
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
                    no_dispatcher("shipping-fulfillments", root_field)
                }
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
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
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
                    ) =>
            {
                self.carrier_service_mutations(query, variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
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
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrderMove" =>
            {
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
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
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "fulfillmentOrdersSetFulfillmentDeadline" =>
            {
                self.shipping_fulfillment_order_mutation_response(
                    root_field, request, query, variables,
                )
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "deliveryProfileCreate" | "deliveryProfileUpdate" | "deliveryProfileRemove"
                    ) =>
            {
                self.delivery_profile_mutation(root_field, query, variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "locationLocalPickupEnable" | "locationLocalPickupDisable"
                    ) =>
            {
                self.location_local_pickup_mutation(root_field, query, variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "fulfillmentOrderHold"
                            | "fulfillmentOrderReleaseHold"
                            | "fulfillmentOrderCancel"
                            | "fulfillmentOrderClose"
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
            (_, CapabilityExecution::OverlayRead) | (_, CapabilityExecution::StageLocally) => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }

    fn dispatch_customers_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match (CapabilityDomain::Customers, execution) {
            (CapabilityDomain::Customers, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(query, variables);
                if customer_payment_methods_only_read(&fields) {
                    if let Some(data) =
                        self.customer_payment_method_local_data(request, query, variables)
                    {
                        return ok_json(data);
                    }
                }
                // A query may combine `customer*` reads with a standalone
                // `storeCreditAccount(id:)` read (or carry only the latter).
                // Each is served from its own staged overlay and the two field
                // maps are merged into one `data` object.
                let handle_customers = self.should_handle_customer_overlay_read(&fields);
                if !handle_customers && self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables);
                }
                let handle_store_credit = fields
                    .iter()
                    .any(|field| field.name == "storeCreditAccount");
                if handle_customers || handle_store_credit {
                    // A `customersCount` read served from the staged overlay
                    // needs the live store-wide baseline; hydrate it once in
                    // LiveHybrid mode before projecting.
                    if handle_customers && fields.iter().any(|field| field.name == "customersCount")
                    {
                        self.hydrate_customers_count_for_overlay_read(request);
                    }
                    let data = root_payload_json(&fields, |field| {
                        if handle_customers {
                            if let Value::Object(object) =
                                self.customer_overlay_read_fields(std::slice::from_ref(field))
                            {
                                if let Some(value) = object.get(field.response_key.as_str()) {
                                    return Some(value.clone());
                                }
                            }
                        }
                        if handle_store_credit {
                            if let Value::Object(object) =
                                self.store_credit_account_read_fields(std::slice::from_ref(field))
                            {
                                if let Some(value) = object.get(field.response_key.as_str()) {
                                    return Some(value.clone());
                                }
                            }
                        }
                        None
                    });
                    ok_json(json!({ "data": data }))
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerCreate" | "customerUpdate" | "customerDelete" | "customerSet"
                    ) =>
            {
                self.customer_mutation_response(request, query, variables)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "customerMerge" =>
            {
                self.customer_merge(query, variables, request)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerRequestDataErasure" | "customerCancelDataErasure"
                    ) =>
            {
                self.customer_data_erasure(
                    query,
                    variables,
                    request,
                    root_field,
                    root_field == "customerRequestDataErasure",
                )
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerAddressCreate"
                            | "customerAddressUpdate"
                            | "customerAddressDelete"
                            | "customerUpdateDefaultAddress"
                    ) =>
            {
                self.customer_address_mutation(request, query, variables)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "storeCreditAccountCredit" | "storeCreditAccountDebit"
                    ) =>
            {
                let outcome =
                    self.store_credit_account_mutation(root_field, request, query, variables);
                self.finalize_mutation_outcome(request, query, variables, outcome)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerAddTaxExemptions"
                            | "customerRemoveTaxExemptions"
                            | "customerReplaceTaxExemptions"
                    ) =>
            {
                let fields = try_root_fields!(query, variables);
                // Enum coercion errors (invalid `taxExemptions`) are raised before
                // any staging, matching Shopify's request-validation ordering.
                if let Some(response) =
                    customer_tax_exemptions_invalid_enum_response(query, &fields)
                {
                    return response;
                }
                self.customer_tax_exemptions_mutation_response(&fields, request, query, variables)
            }
            (CapabilityDomain::Customers, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "customerEmailMarketingConsentUpdate" | "customerSmsMarketingConsentUpdate"
                    ) =>
            {
                let fields = try_root_fields!(query, variables);
                // SMS marketingState values outside `CustomerSmsMarketingState` fail
                // enum coercion before any staging, matching Shopify's ordering.
                if let Some(response) = customer_sms_consent_invalid_enum_response(query, &fields) {
                    return response;
                }
                self.customer_marketing_consent_update(query, variables, request)
            }
            (_, CapabilityExecution::OverlayRead) | (_, CapabilityExecution::StageLocally) => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }

    fn dispatch_b2b_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match (CapabilityDomain::B2b, execution) {
            (CapabilityDomain::B2b, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "companyLocationUpdate"
                            | "companyLocationTaxSettingsUpdate"
                            | "companyAssignCustomerAsContact"
                    ) =>
            {
                match root_field {
                    "companyLocationUpdate" => self
                        .b2b_location_buyer_experience_tail_helper_response(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| no_dispatcher("b2b", root_field)),
                    "companyLocationTaxSettingsUpdate" => self
                        .b2b_tax_settings_tail_helper_response(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| no_dispatcher("b2b", root_field)),
                    "companyAssignCustomerAsContact" => {
                        if let Some(response) =
                            self.b2b_assign_customer_as_contact_response(request, query, variables)
                        {
                            response
                        } else if let Some(data) =
                            self.order_customer_error_paths_data(request, query, variables)
                        {
                            ok_json(data)
                        } else {
                            no_dispatcher("b2b", root_field)
                        }
                    }
                    _ => no_dispatcher("b2b", root_field),
                }
            }
            (CapabilityDomain::B2b, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && self.config.read_mode == ReadMode::Snapshot =>
            {
                // Snapshot mode (unit tests) has no upstream to forward to, so every
                // remaining B2B mutations stage locally through the company tail
                // helper.
                self.b2b_company_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| no_dispatcher("b2b", root_field))
            }
            (CapabilityDomain::B2b, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                // Live/hybrid mode: apply the local cascade side-effects and forward
                // upstream so the recorded Shopify response is returned. Roots
                // without a dedicated cascade handler fall through to a plain
                // passthrough (never a hard 501), keeping parity intact.
                match root_field {
                    "companyCreate"
                    | "companyUpdate"
                    | "companyLocationCreate"
                    | "companyLocationAssignAddress"
                    | "companyContactAssignRole"
                    | "companyContactAssignRoles"
                    | "companyLocationAssignRoles" => self
                        .b2b_company_tail_helper_response(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                        )
                        .unwrap_or_else(|| no_dispatcher("b2b", root_field)),
                    "companyContactDelete"
                    | "companyContactsDelete"
                    | "companyContactRemoveFromCompany" => self.b2b_contact_delete_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyContactCreate" => self.b2b_company_contact_create_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyContactUpdate" => self.b2b_company_contact_update_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyAssignMainContact" => self.b2b_assign_main_contact_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyRevokeMainContact" => self.b2b_revoke_main_contact_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyDelete" | "companiesDelete" => self.b2b_company_delete_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyAddressDelete" => self.b2b_company_address_delete_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    "companyLocationDelete" | "companyLocationsDelete" => self
                        .b2b_company_locations_delete_with_cascade(
                            request,
                            query,
                            variables,
                            operation.operation_type,
                            &operation.root_fields,
                            root_field,
                        ),
                    "companyContactRevokeRole"
                    | "companyContactRevokeRoles"
                    | "companyLocationRevokeRoles" => self.b2b_revoke_roles_with_cascade(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                    _ => self.dispatch_unknown_passthrough_or_legacy_error(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                        root_field,
                    ),
                }
            }
            (CapabilityDomain::B2b, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                if self.should_route_owner_metafields_read(query, variables) {
                    return self.owner_metafields_read(request, query, variables);
                }
                self.b2b_location_buyer_experience_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .or_else(|| {
                    self.b2b_company_tail_helper_response(
                        request,
                        query,
                        variables,
                        operation.operation_type,
                        &operation.root_fields,
                    )
                })
                .unwrap_or_else(|| {
                    // Cold read: the query touches no locally-staged B2B graph
                    // (e.g. a pure read of a pre-existing company catalog, or a
                    // multi-root read whose roots the local serializer does not
                    // cover). Forward verbatim upstream as a read-through so the
                    // real recorded Shopify response is replayed. Staged
                    // read-after-write reads short-circuit above by returning
                    // Some, so this never masks local overlay state. Snapshot
                    // mode has no upstream, so it keeps the explicit 501.
                    if self.config.read_mode != ReadMode::Snapshot {
                        (self.upstream_transport)(request.clone())
                    } else {
                        no_dispatcher("b2b overlay-read", root_field)
                    }
                })
            }
            (_, CapabilityExecution::OverlayRead) | (_, CapabilityExecution::StageLocally) => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }

    pub(in crate::proxy) fn dispatch_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let query = graphql_request.query;
        let variables = graphql_request.variables;

        if let Some(response) = public_admin_graphql_validation_response(
            &query,
            &variables,
            admin_graphql_version(&request.path),
        ) {
            return response;
        }

        let Some(operation) = parse_operation(&query) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(root_field) = operation.primary_root_field() else {
            return json_error(400, "Operation has no root field");
        };

        let schema_input_errors = public_admin_schema_input_errors(
            &query,
            &variables,
            &request.body,
            admin_graphql_version(&request.path),
        );
        if !schema_input_errors.is_empty() {
            return ok_json(json!({ "errors": schema_input_errors }));
        }

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
        if capability.domain == CapabilityDomain::Products
            && operation.operation_type == OperationType::Mutation
            && product_operation_selects_shop_currency_money(&query, &variables)
        {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
        }
        // Discount bulk activate/deactivate/delete jobs run upstream (the async
        // `job` is the real recorded one), but the proxy must mirror their effect
        // onto its local overlay so later reads in the same scenario see the
        // transition. Forward byte-for-byte, then apply the overlay side effect
        // when the job was accepted. Bulk fields embedded in a locally-dispatched
        // omnibus mutation do not reach here (their primary root field is the
        // create), so this only affects standalone bulk requests.
        if operation.operation_type == OperationType::Mutation
            && is_discount_bulk_action_root(root_field)
        {
            let response = (self.upstream_transport)(request.clone());
            if response.status == 200 {
                self.apply_discount_bulk_overlay_effects(&query, &variables, &response.body);
            }
            return response;
        }
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, execution) => self.dispatch_products_graphql(
                request, &query, &variables, &operation, root_field, execution,
            ),
            (CapabilityDomain::SavedSearches, CapabilityExecution::OverlayRead) => {
                self.saved_search_overlay_read_response(request, &query, &variables)
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::StageLocally) => {
                if let Some(response) = saved_search_required_input_error(&query, &variables) {
                    return response;
                }
                let outcome = self.saved_search_mutation_fields(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                self.admin_platform_query_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "backupRegionUpdate" =>
            {
                self.backup_region_update(request, &query, &variables)
            }
            (CapabilityDomain::AdminPlatform, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(root_field, "flowGenerateSignature" | "flowTriggerReceive") =>
            {
                self.flow_utility_mutation(root_field, request, &query, &variables)
            }
            (CapabilityDomain::Apps, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query
                    && root_field == "currentAppInstallation" =>
            {
                let request_app_id = request_app_gid(request);
                if self
                    .store
                    .staged
                    .uninstalled_app_ids
                    .contains(&request_app_id)
                    || self
                        .current_app_installation_app_id_for_request(&request_app_id)
                        .is_some()
                    || !self.store.staged.app_subscriptions.is_empty()
                    || !self.store.staged.app_one_time_purchases.is_empty()
                    || self
                        .store
                        .staged
                        .revoked_app_access_scopes
                        .get(&request_app_id)
                        .is_some_and(|scopes| !scopes.is_empty())
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    let fields = try_root_fields!(&query, &variables);
                    ok_json(json!({
                        "data": self.current_app_installation_read_data(request, &fields)
                    }))
                } else {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.observe_current_app_installation_response(request, &response);
                    }
                    response
                }
            }
            (CapabilityDomain::Apps, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
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
                    _ => no_dispatcher("apps", root_field),
                }
            }
            (CapabilityDomain::OnlineStore, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.online_store_query_response(request, &fields)
            }
            (CapabilityDomain::OnlineStore, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.online_store_mutation(&fields, request, &query, &variables)
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
                if self.config.read_mode != ReadMode::Snapshot
                    && !self.has_local_metaobject_entry_state()
                {
                    self.metaobject_live_hybrid_read(request, &fields)
                } else {
                    ok_json(json!({ "data": self.metaobject_query_data(&fields, request) }))
                }
            }
            (CapabilityDomain::Metaobjects, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                if self.metaobject_mutation_is_local(&fields) {
                    self.metaobject_mutation(&fields, request, &query, &variables)
                } else {
                    // Target lives upstream (seeded/live-captured): forward so the
                    // real backend response is replayed instead of a synthetic one.
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                self.bulk_operation_read_response(request, &query, &variables, root_field)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationRunQuery" =>
            {
                self.bulk_operation_run_query(request, &query, &variables)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationRunMutation" =>
            {
                self.bulk_operation_run_mutation(request, &query, &variables)
            }
            (CapabilityDomain::BulkOperations, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationCancel" =>
            {
                self.bulk_operation_cancel(request, &query, &variables)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                self.discounts_query_response(request, &query, &variables)
            }
            (CapabilityDomain::Discounts, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let outcome = self.discounts_mutation(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.gift_card_read_response(request, &fields)
            }
            (CapabilityDomain::GiftCards, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.gift_card_mutation_response(&fields, request, &query, &variables)
            }
            (CapabilityDomain::Orders, execution) => self.dispatch_orders_graphql(
                request, &query, &variables, &operation, root_field, execution,
            ),
            (CapabilityDomain::Payments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
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
                } else if root_field == "paymentTermsTemplates" {
                    ok_json(json!({ "data": payment_terms_templates_query_data(&fields) }))
                } else {
                    ok_json(json!({ "data": finance_risk_no_data_read_data(&fields) }))
                }
            }
            (CapabilityDomain::Payments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
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
                    let payment_reminder = fields
                        .iter()
                        .any(|field| field.name == "paymentReminderSend")
                        .then(|| self.payment_reminder_local_data(request, &query, &variables))
                        .flatten();
                    if root_field == "paymentReminderSend" {
                        if let Some(data) = payment_reminder {
                            return ok_json(data);
                        }
                    }
                    if let Some(reminder) = &payment_reminder {
                        if reminder.get("errors").is_some() {
                            return ok_json(reminder.clone());
                        }
                    }
                    if let Some(data) =
                        self.customer_payment_method_local_data(request, &query, &variables)
                    {
                        let mut data = data;
                        if let Some(reminder) = payment_reminder {
                            if let (Some(data), Some(reminder)) = (
                                data.get_mut("data").and_then(Value::as_object_mut),
                                reminder.get("data").and_then(Value::as_object),
                            ) {
                                data.extend(reminder.clone());
                            }
                        }
                        return ok_json(data);
                    }
                    return no_dispatcher("payments", root_field);
                }
                if matches!(
                    root_field,
                    "paymentTermsCreate" | "paymentTermsUpdate" | "paymentTermsDelete"
                ) {
                    if let Some(data) = self.payment_terms_local_data(request, &query, &variables) {
                        return ok_json(data);
                    }
                    return no_dispatcher("payments", root_field);
                }
                if matches!(
                    root_field,
                    "orderCapture" | "transactionVoid" | "orderCreateMandatePayment"
                ) {
                    if let Some(data) = self.order_payment_transaction_local_data(
                        request, root_field, &query, &variables,
                    ) {
                        return ok_json(data);
                    }
                    return no_dispatcher("payments", root_field);
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
                    let data = self.payment_customization_mutation_data(request, &fields);
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
                    no_dispatcher("payments", root_field)
                }
            }
            (CapabilityDomain::Marketing, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
                ok_json(json!({ "data": self.marketing_query_data(&fields) }))
            }
            (CapabilityDomain::Marketing, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                let response = self.marketing_mutation(&fields, request);
                let staged_ids: Vec<String> = fields
                    .iter()
                    .filter_map(|field| {
                        response.body["data"][field.response_key.as_str()]["marketingActivity"]
                            ["id"]
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
            }
            (CapabilityDomain::Webhooks, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let Some(document) = parsed_document(&query, &variables) else {
                    return json_error(400, "Could not parse GraphQL operation");
                };
                if let Some(error) = webhook_subscription_sort_key_validation_error(&document) {
                    ok_json(json!({ "errors": [error] }))
                } else {
                    ok_json(json!({
                        "data": self.webhook_subscriptions_query_data(&document.root_fields)
                    }))
                }
            }
            (CapabilityDomain::Webhooks, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                self.webhook_mutation(request, &query, &variables)
            }
            (CapabilityDomain::Events, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                if self.config.read_mode == ReadMode::LiveHybrid {
                    return (self.upstream_transport)(request.clone());
                }
                let fields = try_root_fields!(&query, &variables);
                ok_json(json!({ "data": event_empty_read_data(&fields) }))
            }
            (CapabilityDomain::Localization, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
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
                let fields = try_root_fields!(&query, &variables);
                ok_json(json!({ "data": self.localization_query_data(&fields, request) }))
            }
            (CapabilityDomain::Localization, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.localization_mutation_preflight(&fields, request);
                let data = self.localization_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    root_field,
                    fields
                        .iter()
                        .map(|field| field.response_key.clone())
                        .collect(),
                );
                ok_json(json!({ "data": data }))
            }
            (CapabilityDomain::Markets, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
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
                        // A single verbatim forward returns whatever the client
                        // selected, which can span domains (e.g. a localization
                        // source read selects `markets` alongside `shopLocales`
                        // in one document). Hydrate the localization stores from
                        // the same response so a later market-scoped
                        // translationsRegister sees the observed shop locales.
                        // No-ops on pure markets responses (their connections are
                        // objects, not locale arrays).
                        self.hydrate_localization_from_upstream(&response.body);
                    }
                    return response;
                }
                let fields = try_root_fields!(&query, &variables);
                if operation
                    .root_fields
                    .iter()
                    .all(|field| field == "webPresences")
                {
                    return self.web_presence_helper_query(&query);
                }
                self.hydrate_markets_resolved_values_pricing_if_selected(request, &fields);
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
            }
            (CapabilityDomain::Markets, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                self.hydrate_market_currency_defaults_if_needed(request, &fields);
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
                    return self
                        .web_presence_helper_mutation(root_field, &query, &variables, request);
                } else if operation
                    .root_fields
                    .iter()
                    .all(|field| field == "quantityPricingByVariantUpdate")
                {
                    self.quantity_pricing_rules_mutation_preflight(request, &variables);
                    return quantity_pricing_by_variant_update_response(
                        &query,
                        &variables,
                        &self.store,
                    );
                } else if operation.root_fields.iter().all(|field| {
                    matches!(field.as_str(), "quantityRulesAdd" | "quantityRulesDelete")
                }) {
                    self.quantity_pricing_rules_mutation_preflight(request, &variables);
                    return quantity_rules_mutation_response(
                        root_field,
                        &query,
                        &variables,
                        &self.store,
                    );
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
                    return ok_json(
                        self.price_list_mutation_data(&fields, request, &query, &variables),
                    );
                } else if operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "catalogCreate"
                            | "catalogUpdate"
                            | "catalogDelete"
                            | "catalogContextUpdate"
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
                        fields
                            .iter()
                            .map(|field| field.response_key.clone())
                            .collect(),
                    );
                }
                ok_json(json!({ "data": data }))
            }
            (CapabilityDomain::Functions, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                let fields = try_root_fields!(&query, &variables);
                let (data, errors) = self.functions_metadata_mutation_data(request, &fields);
                if data
                    .as_object()
                    .is_some_and(|fields| fields.values().any(|value| !value.is_null()))
                {
                    self.record_mutation_log_entry(
                        request,
                        &query,
                        &variables,
                        root_field,
                        Vec::new(),
                    );
                }
                if errors.is_empty() {
                    ok_json(json!({ "data": data }))
                } else {
                    ok_json(json!({ "data": data, "errors": errors }))
                }
            }
            (CapabilityDomain::Functions, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                // A cold function read (no validation/cart-transform staged this
                // session) forwards to the upstream so `shopifyFunctions` /
                // `shopifyFunction` reflect the shop's real installed functions
                // and their app ownership metadata. Once a lifecycle is staged we
                // serve locally (read-after-write / read-after-delete).
                if self.config.read_mode != ReadMode::Snapshot && !self.local_has_function_state() {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status == 200 {
                        self.hydrate_function_metadata_from_response_data(&response.body["data"]);
                    }
                    response
                } else {
                    let fields = try_root_fields!(&query, &variables);
                    let mut selection_errors =
                        cart_transform_selection_errors(&query, &variables, &fields);
                    selection_errors.extend(functions_output_selection_errors(
                        &query, &variables, &fields,
                    ));
                    if selection_errors.is_empty() {
                        ok_json(
                            json!({ "data": self.functions_metadata_read_data(request, &fields) }),
                        )
                    } else {
                        ok_json(json!({ "errors": selection_errors }))
                    }
                }
            }
            (CapabilityDomain::Metafields, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                // Cold LiveHybrid definition reads forward verbatim to the
                // upstream; only once a lifecycle has staged definitions do we
                // serve locally (read-after-write / read-after-delete).
                if self.config.read_mode != ReadMode::Snapshot
                    && !self.local_has_metafield_definition_state(&variables)
                {
                    (self.upstream_transport)(request.clone())
                } else {
                    self.metafield_definition_pinning_read(request, &query, &variables)
                }
            }
            (CapabilityDomain::Metafields, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "standardMetafieldDefinitionEnable" =>
            {
                self.standard_metafield_definition_enable(request, &query, &variables)
            }
            (CapabilityDomain::Metafields, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                self.metafield_definition_pinning_mutation(request, &query, &variables)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
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
                } else if root_field == "shop" {
                    if self.should_route_owner_metafields_read(&query, &variables) {
                        return self.owner_metafields_read(request, &query, &variables);
                    }
                    // `shop` reads are served locally only when the proxy is
                    // holding shop-policy overlay state (snapshot mode, or staged
                    // / tombstoned policies); otherwise the live shop response is
                    // replayed verbatim so unrelated shop fields stay authentic.
                    if self.should_handle_shop_policy_query_locally() {
                        if let Some(data) = self.shop_query_data(&fields, Some(request)) {
                            ok_json(json!({ "data": data }))
                        } else {
                            let response = (self.upstream_transport)(request.clone());
                            if (200..300).contains(&response.status) {
                                self.hydrate_shop_state_from_response_data(&response.body["data"]);
                                self.observe_nodes_response(&response);
                            }
                            response
                        }
                    } else {
                        let response = (self.upstream_transport)(request.clone());
                        if (200..300).contains(&response.status) {
                            self.hydrate_shop_state_from_response_data(&response.body["data"]);
                            self.observe_nodes_response(&response);
                        }
                        response
                    }
                } else if self.has_location_overlay_state()
                    || !self.location_read_needs_upstream(&fields)
                {
                    // A `location(id:)`/`locations` read may be combined in one
                    // operation with `locationsAvailableForDeliveryProfilesConnection`
                    // (the shipping-locations connection). Serve the location
                    // fields from the location overlay, then merge the
                    // delivery-profile locations connection into the same `data`
                    // object so both resolve from staged/observed state.
                    let mut response = self.location_read_response(&fields);
                    if fields.iter().any(|field| {
                        field.name == "locationsAvailableForDeliveryProfilesConnection"
                    }) {
                        shallow_merge_object(
                            &mut response.body["data"],
                            self.delivery_profile_locations_read_data(&fields),
                        );
                    }
                    response
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
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
                    && root_field == "shopPolicyUpdate" =>
            {
                self.shop_policy_update(request, &query, &variables)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "locationAdd" | "locationEdit" | "locationActivate" | "locationDelete"
                    ) =>
            {
                // `locationEdit`/`locationDelete` resolve the target through the
                // local overlay first and fall back to an upstream hydrate when the
                // location is not staged (live-hybrid), so unknown ids surface the
                // real "Location not found." / guardrail user errors rather than
                // passing through. Staged targets never touch upstream.
                self.location_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::StoreProperties, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "locationDeactivate" =>
            {
                self.location_deactivate(&query, &variables, request)
            }
            (CapabilityDomain::Segments, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query =>
            {
                let fields = try_root_fields!(&query, &variables);
                if root_field == "customerSegmentMembersQuery" {
                    ok_json(json!({
                        "data": self.customer_segment_members_query_read_data(&fields)
                    }))
                } else if self.store.staged.segments.is_empty()
                    && self.store.staged.segment_catalog.is_empty()
                {
                    // De-seeded cold read of pre-existing segment state. The
                    // segment catalog's opaque cursors and server-side query
                    // filtering, plus the filter / filter-suggestion /
                    // value-suggestion / migration taxonomy, encode
                    // Shopify-internal state that cannot be reconstructed from
                    // local store state, so the proxy forwards the read upstream
                    // and returns Shopify's response verbatim (it reads the real
                    // segment catalog instead of replaying a `/__meta/seed`
                    // snapshot). A scenario that has staged segments locally (via
                    // `segmentCreate`) or still seeds the catalog is served
                    // locally below.
                    (self.upstream_transport)(request.clone())
                } else {
                    let upstream_catalog_response = self
                        .segment_read_needs_upstream_catalog(&fields)
                        .then(|| (self.upstream_transport)(request.clone()));
                    let (mut data, mut errors) = self.segment_read_data(&fields);
                    if let Some(response) = upstream_catalog_response {
                        if response.status != 200 {
                            return response;
                        }
                        self.merge_upstream_segment_catalog_data(
                            &mut data,
                            &mut errors,
                            &fields,
                            &response.body,
                        );
                    }
                    if errors.is_empty() {
                        ok_json(json!({ "data": data }))
                    } else {
                        ok_json(json!({ "data": data, "errors": errors }))
                    }
                }
            }
            (CapabilityDomain::Segments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "customerSegmentMembersQueryCreate" =>
            {
                self.customer_segment_members_query_create(&query, &variables, request)
            }
            (CapabilityDomain::Segments, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
            {
                self.segment_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::ShippingFulfillments, execution) => self
                .dispatch_shipping_fulfillments_graphql(
                    request, &query, &variables, &operation, root_field, execution,
                ),
            (CapabilityDomain::Customers, execution) => self.dispatch_customers_graphql(
                request, &query, &variables, &operation, root_field, execution,
            ),
            (CapabilityDomain::Privacy, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "dataSaleOptOut" =>
            {
                let outcome = self.data_sale_opt_out(request, &query, &variables);
                self.finalize_mutation_outcome(request, &query, &variables, outcome)
            }
            (CapabilityDomain::B2b, execution) => self.dispatch_b2b_graphql(
                request, &query, &variables, &operation, root_field, execution,
            ),
            (CapabilityDomain::Media, CapabilityExecution::OverlayRead)
                if operation.operation_type == OperationType::Query && root_field == "files" =>
            {
                self.media_files_read(request, &query, &variables)
            }
            (CapabilityDomain::Media, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation =>
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
            (_, CapabilityExecution::OverlayRead) => no_dispatcher("overlay-read", root_field),
            (_, CapabilityExecution::StageLocally) => no_dispatcher("stage-locally", root_field),
            _ => unreachable!("non-unknown passthrough capabilities are not registered"),
        }
    }
}

use super::*;
use crate::admin_graphql::{
    self, AdminApiVersion, RootExecutionContext, RootFieldError, RootFieldExecutor, RootFieldResult,
};
use crate::graphql::{
    directive_invocations, DirectiveSelection, ParsedDocument, VariableDefinitionInfo,
};

struct ProxyRootExecutor {
    proxy: Arc<std::sync::Mutex<DraftProxy>>,
    root_requests: BTreeMap<String, Request>,
    root_locations: BTreeMap<String, SourceLocation>,
    discount_preflight: Option<(Request, Vec<RootFieldSelection>)>,
    discount_preflight_done: std::sync::Mutex<bool>,
    grouped_local_request: Option<Request>,
    grouped_local_response: std::sync::Mutex<Option<Response>>,
    full_passthrough_request: Option<Request>,
    full_passthrough_response: Arc<std::sync::Mutex<Option<Response>>>,
    reject_mixed_mutation: bool,
    resolved_responses: Arc<std::sync::Mutex<BTreeMap<String, Response>>>,
}

fn shared_root_response(responses: &BTreeMap<String, Response>) -> Option<&Response> {
    let mut responses = responses.values();
    let first = responses.next()?;
    responses.all(|response| response == first).then_some(first)
}

impl RootFieldExecutor for ProxyRootExecutor {
    fn execute_root(&self, response_key: &str, root_name: &str) -> Result<RootFieldResult, String> {
        if self.reject_mixed_mutation {
            return Err(
                "A mutation operation cannot mix locally staged and passthrough root fields."
                    .to_string(),
            );
        }
        if let Some((request, fields)) = &self.discount_preflight {
            let mut done = self
                .discount_preflight_done
                .lock()
                .map_err(|_| "Admin GraphQL discount preflight lock was poisoned".to_string())?;
            if !*done {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                proxy.hydrate_discount_item_refs(request, fields);
                proxy.hydrate_discount_context_refs(request, fields);
                proxy.engine_discount_refs_preflighted = true;
                *done = true;
            }
        }
        let response = if let Some(request) = &self.full_passthrough_request {
            let mut cached = self
                .full_passthrough_response
                .lock()
                .map_err(|_| "Admin GraphQL passthrough response lock was poisoned".to_string())?;
            if cached.is_none() {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                *cached = Some(proxy.dispatch_passthrough_graphql(request));
            }
            cached
                .as_ref()
                .expect("passthrough response should be cached")
                .clone()
        } else if let Some(request) = &self.grouped_local_request {
            let mut cached = self
                .grouped_local_response
                .lock()
                .map_err(|_| "Admin GraphQL grouped response lock was poisoned".to_string())?;
            if cached.is_none() {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                *cached = Some(proxy.resolve_prevalidated_graphql_root(request));
            }
            cached
                .as_ref()
                .expect("grouped local response should be cached")
                .clone()
        } else {
            let request = self.root_requests.get(response_key).ok_or_else(|| {
                format!(
                    "No request-scoped resolver input was prepared for GraphQL root `{root_name}`"
                )
            })?;
            let mut proxy = self
                .proxy
                .lock()
                .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
            proxy.resolve_prevalidated_graphql_root(request)
        };
        self.resolved_responses
            .lock()
            .map_err(|_| "Admin GraphQL resolved response lock was poisoned".to_string())?
            .insert(response_key.to_string(), response.clone());
        let value = response
            .body
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get(response_key))
            .cloned()
            .unwrap_or(Value::Null);
        let mut errors = response
            .body
            .get("errors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|error| {
                let error_path = error.get("path").and_then(Value::as_array);
                if error_path
                    .and_then(|path| path.first())
                    .and_then(Value::as_str)
                    .is_some_and(|root| root != response_key)
                {
                    return None;
                }
                let error_code = error.pointer("/extensions/code").and_then(Value::as_str);
                let locations =
                    if matches!(error_code, Some("BAD_REQUEST" | "MAX_INPUT_SIZE_EXCEEDED")) {
                        self.root_locations
                            .get(response_key)
                            .map(|location| {
                                vec![async_graphql::Pos {
                                    line: location.line,
                                    column: location.column,
                                }]
                            })
                            .unwrap_or_default()
                    } else {
                        error
                            .get("locations")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|location| {
                                Some(async_graphql::Pos {
                                    line: location.get("line")?.as_u64()? as usize,
                                    column: location.get("column")?.as_u64()? as usize,
                                })
                            })
                            .collect()
                    };
                Some(RootFieldError {
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("GraphQL root resolver failed")
                        .to_string(),
                    extensions: error
                        .get("extensions")
                        .and_then(Value::as_object)
                        .map(|extensions| {
                            extensions
                                .iter()
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    path: error_path
                        .map(|path| {
                            path.iter()
                                .skip(1)
                                .filter_map(|segment| match segment {
                                    Value::String(field) => {
                                        Some(async_graphql::PathSegment::Field(field.clone()))
                                    }
                                    Value::Number(index) => index.as_u64().map(|index| {
                                        async_graphql::PathSegment::Index(index as usize)
                                    }),
                                    _ => None,
                                })
                                .collect()
                        })
                        // HTTP/dispatcher failures historically gained the current
                        // root path at the GraphQL execution boundary. Status-200
                        // resolver errors without a path are intentionally pathless.
                        .or_else(|| (response.status >= 400).then(Vec::new)),
                    locations,
                })
            })
            .collect::<Vec<_>>();
        if errors.is_empty() && response.status >= 400 {
            errors.push(RootFieldError {
                message: format!(
                    "GraphQL root `{root_name}` failed with status {}",
                    response.status
                ),
                extensions: BTreeMap::new(),
                path: Some(Vec::new()),
                locations: Vec::new(),
            });
        }
        Ok(RootFieldResult { value, errors })
    }
}

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

pub(in crate::proxy) fn operation_selection_error_response(
    error: OperationSelectionError,
) -> Response {
    match error {
        OperationSelectionError::MultipleOperationsRequireOperationName => ok_json(json!({
            "errors": [{ "message": "An operation name is required" }]
        })),
        OperationSelectionError::UnknownOperationName(operation_name) => ok_json(json!({
            "errors": [{ "message": format!("No operation named \"{operation_name}\"") }]
        })),
        OperationSelectionError::Parse => json_error(400, "Could not parse GraphQL operation"),
    }
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
    for pointer in ["/data/productByIdentifier", "/data/productByHandle"] {
        if let Some(node) = response
            .body
            .pointer(pointer)
            .filter(|node| node.is_object())
        {
            nodes.push(node.clone());
        }
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
                    let api_client_id = request_app_namespace_api_client_id(request);
                    ok_json(json!({
                        "data": self.product_overlay_read_data(&fields, api_client_id.as_deref())
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
                self.hydrate_selling_plan_groups_for_read(request, &fields);
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
                    let api_client_id = request_app_namespace_api_client_id(request);
                    ok_json(
                        json!({ "data": self.product_overlay_read_data(&fields, api_client_id.as_deref()) }),
                    )
                } else {
                    (self.upstream_transport)(request.clone())
                }
            }
            "collectionByIdentifier" | "collectionByHandle" => {
                let fields = try_root_fields!(query, variables);
                if self.collection_identifier_read_needs_upstream(&fields) {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.observe_collections_read_response(&response);
                    }
                    response
                } else {
                    ok_json(
                        json!({ "data": self.collection_membership_downstream_read_data(&fields) }),
                    )
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
            "job" if self.should_handle_customer_overlay_read(&fields) => ok_json(json!({
                "data": self.customer_overlay_read_fields(&fields)
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
            let response = (self.upstream_transport)(request.clone());
            if self.config.read_mode == ReadMode::LiveHybrid {
                self.observe_order_read_response(request, &response);
            }
            return response;
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
        registered_node_value(self, id, selection, request).flatten()
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

    pub(in crate::proxy) fn app_node_value_by_id(
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
        let root_fields = parse_operation_with_variables(query, variables)
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
                let outcome = self.product_set(request, query, variables);
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
                        "productVariantsBulkCreate"
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
                self.hydrate_selling_plan_mutation_targets(request, root_field, query, variables);
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
                        "orderMarkAsPaid"
                            | "orderCreateManualPayment"
                            | "refundCreate"
                            | "orderEditBegin"
                            | "orderEditCommit"
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
                    self.orders_stage_locally_unmodeled_shape_response(
                        request, query, variables, root_field,
                    )
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
                            | "fulfillmentTrackingInfoUpdateV2"
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
            (CapabilityDomain::Orders, CapabilityExecution::StageLocally)
                if operation.operation_type == OperationType::Mutation
                    && root_field == "orderInvoiceSend" =>
            {
                if let Some(data) = self.order_invoice_send_local_data(request, query, variables) {
                    ok_json(data)
                } else {
                    no_dispatcher("orders", root_field)
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
                let fields = try_root_fields!(query, variables);
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
                    if handle_customers && self.customer_read_selects_amount_spent(&fields) {
                        self.hydrate_shop_pricing_state_if_missing(request, true, false);
                    }
                    let customer_upstream_data = (handle_customers
                        && self.customer_overlay_needs_upstream_data(&fields))
                    .then(|| self.customer_overlay_upstream_data(request))
                    .flatten();
                    let data = root_payload_json(&fields, |field| {
                        if handle_customers {
                            if let Value::Object(object) = self.customer_overlay_read_fields(
                                request,
                                std::slice::from_ref(field),
                                customer_upstream_data.as_ref(),
                            ) {
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
                    && matches!(
                        root_field,
                        "customerGenerateAccountActivationUrl" | "customerSendAccountInviteEmail"
                    ) =>
            {
                self.customer_outbound_lifecycle_response(request, query, variables)
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
                // LiveHybrid still stages B2B mutations locally. Cold existing
                // resources may need fuller hydration in future work, but the
                // caller's mutation must never be forwarded as the fallback.
                self.b2b_company_tail_helper_response(
                    request,
                    query,
                    variables,
                    operation.operation_type,
                    &operation.root_fields,
                )
                .unwrap_or_else(|| no_dispatcher("b2b", root_field))
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

    /// Execute an Admin GraphQL request through the captured versioned schema.
    /// Domain code is reached only through root field resolvers; the GraphQL
    /// engine owns the executable language and response projection.
    pub(in crate::proxy) fn dispatch_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let Some(version) = AdminApiVersion::from_route(&request.path) else {
            return json_error(404, "No captured Admin GraphQL schema for this route");
        };
        let schema = match admin_graphql::schema(version) {
            Ok(schema) => schema,
            Err(error) => {
                return json_error(
                    500,
                    &format!("Could not initialize Admin GraphQL {version}: {error}"),
                );
            }
        };

        let selected_query = selected_operation_query(
            &graphql_request.query,
            graphql_request.operation_name.as_deref(),
        )
        .ok();
        let prepared = selected_query.as_deref().and_then(|query| {
            let variables =
                variables_with_operation_defaults(query, &graphql_request.variables, None).ok()?;
            let document = parsed_document(query, &variables)?;
            let single_root = document.root_fields.len() == 1;
            let root_requests = document
                .root_fields
                .iter()
                .map(|field| {
                    if single_root {
                        return (field.response_key.clone(), request.clone());
                    }
                    let field_query = single_root_operation_query(
                        document.operation_type,
                        field,
                        &document.variable_definitions,
                    );
                    (
                        field.response_key.clone(),
                        Request {
                            method: request.method.clone(),
                            path: request.path.clone(),
                            headers: request.headers.clone(),
                            body: json!({
                                "query": field_query,
                                "variables": resolved_variables_json(&variables)
                            })
                            .to_string(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            Some((document, variables, root_requests))
        });

        let (operation_type, root_names, root_requests) = prepared
            .as_ref()
            .map(|(document, _, root_requests)| {
                (
                    Some(document.operation_type),
                    document
                        .root_fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>(),
                    root_requests.clone(),
                )
            })
            .unwrap_or((None, Vec::new(), BTreeMap::new()));
        let capabilities = operation_type.map_or_else(Vec::new, |operation_type| {
            root_names
                .iter()
                .map(|root| self.registry.resolve(operation_type, root))
                .collect::<Vec<_>>()
        });
        let has_local_root = capabilities.iter().any(|capability| {
            capability.domain != CapabilityDomain::Unknown
                && matches!(
                    capability.execution,
                    CapabilityExecution::OverlayRead | CapabilityExecution::StageLocally
                )
        });
        let has_passthrough_root = capabilities.iter().any(|capability| {
            capability.domain == CapabilityDomain::Unknown
                || capability.execution == CapabilityExecution::Passthrough
        });

        // A mixed mutation cannot be split without changing its atomicity or
        // risking a supported write upstream. Reject it before any resolver is
        // invoked. Queries can safely combine an upstream read with local
        // overlay roots.
        let reject_mixed_mutation = operation_type == Some(OperationType::Mutation)
            && has_local_root
            && has_passthrough_root;

        let all_passthrough = !root_names.is_empty() && !has_local_root && has_passthrough_root;
        if let Some((document, _, _)) = prepared.as_ref() {
            if let Some(error) = required_variable_error(document, &graphql_request.variables) {
                return ok_json(json!({ "errors": [error] }));
            }
            if let Some(body) = product_create_argument_arity_error(document) {
                return ok_json(body);
            }
            if let Some(error) = directive_variable_mismatch_error(
                document,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                &graphql_request.variables,
            ) {
                return ok_json(json!({ "errors": [error] }));
            }
            let id_errors = shopify_root_id_errors(
                version,
                document,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                &graphql_request.variables,
            );
            if !id_errors.is_empty() {
                return ok_json(json!({ "errors": id_errors }));
            }
        }
        let grouped_local_request = prepared.as_ref().and_then(|(document, variables, _)| {
            selected_query.as_deref().and_then(|query| {
                let owner_metafields = document.operation_type == OperationType::Query
                    && self.should_route_owner_metafields_read(query, variables);
                let grouped_domain_read = document.operation_type == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities.first().is_some_and(|first| {
                        first.domain != CapabilityDomain::Unknown
                            && first.execution == CapabilityExecution::OverlayRead
                            && capabilities.iter().all(|capability| capability == first)
                    });
                let grouped_media_saved_search_read =
                    document.operation_type == OperationType::Query
                        && document.root_fields.len() > 1
                        && document.root_fields.iter().all(|field| {
                            matches!(field.name.as_str(), "files" | "fileSavedSearches")
                        });
                let grouped_localization_markets_read = document.operation_type
                    == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities.first().is_some_and(|capability| {
                        capability.domain == CapabilityDomain::Localization
                            && capability.execution == CapabilityExecution::OverlayRead
                    })
                    && document
                        .root_fields
                        .iter()
                        .any(|field| field.name == "markets")
                    && capabilities.iter().all(|capability| {
                        capability.execution == CapabilityExecution::OverlayRead
                            && matches!(
                                capability.domain,
                                CapabilityDomain::Localization | CapabilityDomain::Markets
                            )
                    });
                let live_events = self.config.read_mode == ReadMode::LiveHybrid
                    && !capabilities.is_empty()
                    && capabilities
                        .iter()
                        .all(|capability| capability.domain == CapabilityDomain::Events);
                (owner_metafields
                    || grouped_domain_read
                    || grouped_media_saved_search_read
                    || grouped_localization_markets_read
                    || live_events)
                    .then(|| request.clone())
            })
        });
        let root_locations = prepared
            .as_ref()
            .map(|(document, _, _)| {
                document
                    .root_fields
                    .iter()
                    .map(|field| (field.response_key.clone(), field.location))
                    .collect()
            })
            .unwrap_or_default();
        let discount_preflight = prepared.as_ref().and_then(|(document, _, _)| {
            (document.operation_type == OperationType::Mutation
                && capabilities
                    .iter()
                    .any(|capability| capability.domain == CapabilityDomain::Discounts))
            .then(|| (request.clone(), document.root_fields.clone()))
        });
        let placeholder = DraftProxy::new(self.config.clone());
        let mut owned_proxy = std::mem::replace(self, placeholder);
        let log_start = owned_proxy.log_entries.len();
        if operation_type == Some(OperationType::Mutation) && has_local_root {
            owned_proxy.engine_mutation_log_start = Some(log_start);
        }
        let shared_proxy = Arc::new(std::sync::Mutex::new(owned_proxy));
        let resolved_responses = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
        let full_passthrough_response = Arc::new(std::sync::Mutex::new(None));
        let root_executor: Arc<dyn RootFieldExecutor> = Arc::new(ProxyRootExecutor {
            proxy: Arc::clone(&shared_proxy),
            root_requests,
            root_locations,
            discount_preflight,
            discount_preflight_done: std::sync::Mutex::new(false),
            grouped_local_request,
            grouped_local_response: std::sync::Mutex::new(None),
            full_passthrough_request: all_passthrough.then(|| request.clone()),
            full_passthrough_response: Arc::clone(&full_passthrough_response),
            reject_mixed_mutation,
            resolved_responses: Arc::clone(&resolved_responses),
        });

        // `async-graphql`'s dynamic builder cannot register custom directive
        // definitions. Preserve Shopify's executable `@idempotent` contract in
        // the domain request, while removing only that directive from the copy
        // validated/executed by the engine. All other directives remain under
        // normal GraphQL validation.
        let engine_query = expand_bare_store_credit_origin_selections(
            &strip_idempotent_directives(&graphql_request.query),
        );
        let mut engine_request = async_graphql::Request::new(engine_query)
            .variables(async_graphql::Variables::from_json(
                resolved_variables_json(&graphql_request.variables),
            ))
            .data(RootExecutionContext {
                executor: Arc::clone(&root_executor),
            });
        if let Some(operation_name) = graphql_request.operation_name {
            engine_request = engine_request.operation_name(operation_name);
        }
        let engine_response = futures_executor::block_on(schema.execute(engine_request));
        drop(root_executor);
        let resolved_responses = resolved_responses
            .lock()
            .map(|responses| responses.clone())
            .unwrap_or_default();

        let mut owned_proxy = match Arc::try_unwrap(shared_proxy) {
            Ok(proxy) => match proxy.into_inner() {
                Ok(proxy) => proxy,
                Err(poisoned) => poisoned.into_inner(),
            },
            Err(_) => {
                return json_error(
                    500,
                    "Admin GraphQL execution retained a proxy state reference",
                );
            }
        };
        owned_proxy.engine_mutation_log_start = None;
        owned_proxy.engine_discount_refs_preflighted = false;
        *self = owned_proxy;

        if operation_type == Some(OperationType::Mutation) && has_local_root {
            let variables = prepared
                .as_ref()
                .map(|(_, variables, _)| variables)
                .unwrap_or(&graphql_request.variables);
            self.normalize_engine_mutation_log(
                log_start,
                request,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                variables,
                &root_names,
            );
        }

        if let Some(response) = full_passthrough_response
            .lock()
            .ok()
            .and_then(|response| response.clone())
        {
            return response;
        }

        let authoritative_upstream_response =
            shared_root_response(&resolved_responses).filter(|response| {
                (200..300).contains(&response.status)
                    && response.body.get("errors").is_none()
                    && response.body.pointer("/extensions/cost").is_some()
            });
        let authoritative_passthrough_omission = authoritative_upstream_response.is_some()
            && engine_response.errors.iter().any(|error| {
                error
                    .message
                    .starts_with("Local resolver did not implement `")
                    || (error.message == "internal: non-null types require a return value"
                        && error.path.first().is_some_and(|segment| {
                            let async_graphql::PathSegment::Field(root) = segment else {
                                return false;
                            };
                            authoritative_upstream_response.is_some_and(|response| {
                                response
                                    .body
                                    .get("data")
                                    .and_then(Value::as_object)
                                    .is_some_and(|data| !data.contains_key(root))
                            })
                        }))
            });
        let body = if authoritative_passthrough_omission {
            // A read-through resolver can return Shopify's already-executed
            // response verbatim. Shopify occasionally omits selected roots or
            // nested fields from that response without reporting an error. Do
            // not reinterpret an otherwise successful, cost-bearing upstream
            // envelope as a local resolver failure.
            authoritative_upstream_response
                .map(|response| response.body.clone())
                .unwrap_or_else(|| json!({ "data": Value::Null }))
        } else if engine_response.errors.iter().any(|error| {
            (error.message.contains("expected \"FieldValue::WithType\"")
                && (error.message.contains("invalid value for interface")
                    || error.message.contains("invalid value for union")))
                || error
                    .message
                    .contains("\"null\" is not of the expected type")
        }) {
            // async-graphql's dynamic API cannot represent a null list element
            // whose item type is an interface/union: `FieldValue::NULL` is
            // rejected because abstract values normally require `with_type`.
            // The request has already passed full engine validation. Preserve
            // the correctly projected resolver payload for this narrow library
            // limitation so `nodes(ids:)` can retain null placeholders.
            let mut body = merge_resolved_root_responses(&resolved_responses);
            if let Some((document, _, _)) = prepared.as_ref() {
                strip_unselected_typenames_from_response(&mut body, document);
            }
            body
        } else {
            shopify_engine_response(
                engine_response,
                version,
                prepared.as_ref().map(|(document, _, _)| document),
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                prepared
                    .as_ref()
                    .map(|(_, variables, _)| variables)
                    .unwrap_or(&graphql_request.variables),
                &graphql_request.variable_input_orders,
            )
        };
        let mut body = body;
        strip_cloud_webhook_callback_urls(&mut body);
        merge_resolved_extensions(&mut body, &resolved_responses);
        if let Some(response) = shared_root_response(&resolved_responses) {
            if let (Some(projected), Some(resolved)) =
                (body.as_object_mut(), response.body.as_object())
            {
                for (name, value) in resolved {
                    if !matches!(name.as_str(), "data" | "errors") {
                        projected
                            .entry(name.clone())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
            return Response {
                status: response.status,
                headers: response.headers.clone(),
                body,
            };
        }
        ok_json(body)
    }

    fn normalize_engine_mutation_log(
        &mut self,
        log_start: usize,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_fields: &[String],
    ) {
        if log_start >= self.log_entries.len() {
            return;
        }
        let mut entries = self.log_entries.drain(log_start..).collect::<Vec<_>>();
        if entries.len() == 1 {
            let entry = &mut entries[0];
            entry["query"] = json!(query);
            entry["variables"] = resolved_variables_json(variables);
            entry["rawBody"] = json!(request.body.clone());
            entry["path"] = json!(request.path.clone());
            entry["interpreted"]["rootFields"] = json!(root_fields);
            self.log_entries.extend(entries);
            return;
        }

        let staged_resource_ids = entries
            .iter()
            .filter_map(|entry| entry.get("stagedResourceIds").and_then(Value::as_array))
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let status = if entries
            .iter()
            .any(|entry| entry.get("status") == Some(&json!("failed")))
        {
            "failed"
        } else if entries
            .iter()
            .any(|entry| entry.get("status") == Some(&json!("staged")))
        {
            "staged"
        } else {
            "proxied"
        };
        let primary_root = root_fields.first().cloned().unwrap_or_default();
        self.log_entries.push(json!({
            "id": format!("log-{}", log_start + 1),
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": status,
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": primary_root,
                "execution": "schema-resolvers"
            },
            "notes": "Executed serially as one validated GraphQL mutation operation."
        }));
    }

    pub(in crate::proxy) fn dispatch_passthrough_graphql(&mut self, request: &Request) -> Response {
        self.resolve_registered_graphql(request)
    }

    pub(in crate::proxy) fn resolve_prevalidated_graphql_root(
        &mut self,
        request: &Request,
    ) -> Response {
        self.resolve_registered_graphql(request)
    }

    fn resolve_registered_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let raw_query = graphql_request.query;
        let requested_operation_name = graphql_request.operation_name.as_deref();

        let selection = match selected_operation(&raw_query, requested_operation_name) {
            Ok(selection) => selection,
            Err(error) => return operation_selection_error_response(error),
        };
        let query = if selection.requires_filtered_document {
            match selected_operation_query(&raw_query, requested_operation_name) {
                Ok(query) => query,
                Err(error) => return operation_selection_error_response(error),
            }
        } else {
            raw_query
        };
        let variables =
            match variables_with_operation_defaults(&query, &graphql_request.variables, None) {
                Ok(variables) => variables,
                Err(error) => return operation_selection_error_response(error),
            };

        let Some(operation) = parse_operation_with_variables(&query, &variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(root_field) = operation.primary_root_field() else {
            return ok_json(json!({ "data": {} }));
        };

        if operation.root_fields.len() > 1
            && operation.operation_type == OperationType::Query
            && self.should_route_owner_metafields_read(&query, &variables)
        {
            return self.owner_metafields_read(request, &query, &variables);
        }

        let capability = self.registry.resolve(operation.operation_type, root_field);
        if capability.domain == CapabilityDomain::Products
            && operation.operation_type == OperationType::Mutation
            && product_operation_selects_shop_currency_money(&query, &variables)
        {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
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
                if self.config.read_mode != ReadMode::Snapshot {
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
                        "data": self.payment_customization_query_data(request, &fields)
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
                self.marketing_query_response(request, &fields)
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
                let fields = try_root_fields!(&query, &variables);
                let localization_needs_upstream =
                    self.localization_should_fetch_upstream(root_field);
                let grouped_markets_need_upstream = fields.len() > 1
                    && fields.iter().any(|field| field.name == "markets")
                    && self.markets_should_fetch_upstream(&fields, &variables);
                if self.config.read_mode == ReadMode::LiveHybrid && grouped_markets_need_upstream {
                    // The client's mixed localization/markets document is the
                    // authoritative hydration request. Forward it once, observe
                    // both stores, and then render locally when staged
                    // localization state must overlay the response.
                    let response = (self.upstream_transport)(request.clone());
                    if response.status >= 400 || response.body.get("errors").is_some() {
                        return response;
                    }
                    self.hydrate_markets_from_upstream_for_fields(&response.body, &fields);
                    self.hydrate_localization_from_upstream(&response.body);
                    if localization_needs_upstream {
                        return response;
                    }
                }
                // Cold LiveHybrid reads forward verbatim upstream and hydrate the
                // base stores as a side effect (product existence, shop locales);
                // once a lifecycle has staged localization records we serve
                // locally (read-after-write).
                if self.config.read_mode == ReadMode::LiveHybrid && localization_needs_upstream {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.hydrate_localization_from_upstream(&response.body);
                    }
                    return response;
                }
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
                let fields = try_root_fields!(&query, &variables);
                // Cold LiveHybrid reads forward verbatim upstream and hydrate the
                // staged stores as a side effect. If local markets-family rows
                // already exist, keep the upstream response as hydration input
                // and render from the effective local graph so staged deltas are
                // merged instead of replacing unrelated families.
                if self.config.read_mode == ReadMode::LiveHybrid
                    && self.markets_should_fetch_upstream(&fields, &variables)
                {
                    let had_markets_overlay_state = self.has_markets_overlay_state();
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.hydrate_markets_from_upstream_for_fields(&response.body, &fields);
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
                    if !had_markets_overlay_state {
                        return response;
                    }
                }
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
                if let Some(response) = self.market_mutation_wrong_resource_response(&fields) {
                    return response;
                }
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
                    return self
                        .quantity_pricing_by_variant_update_response(&query, &variables, request);
                } else if operation.root_fields.iter().all(|field| {
                    matches!(field.as_str(), "quantityRulesAdd" | "quantityRulesDelete")
                }) {
                    self.quantity_pricing_rules_mutation_preflight(request, &variables);
                    return self
                        .quantity_rules_mutation_response(root_field, &query, &variables, request);
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
                    self.market_mutation_target_preflight(&fields, request);
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
                let fields = try_root_fields!(&query, &variables);
                // A cold function read forwards to the upstream so
                // `shopifyFunctions` / `shopifyFunction` / lifecycle catalogs
                // reflect the shop's real installed Functions. Once the
                // requested roots intersect known base or staged overlay state,
                // hydrate the relevant upstream families and resolve from the
                // effective catalog.
                if self.config.read_mode != ReadMode::Snapshot
                    && !self.function_read_has_local_overlay(&fields)
                {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status == 200 {
                        self.hydrate_function_metadata_from_response_data(&response.body["data"]);
                        self.mark_function_read_fields_hydrated(&fields);
                    }
                    response
                } else {
                    let selection_errors =
                        functions_output_selection_errors(&query, &variables, &fields);
                    if selection_errors.is_empty() {
                        if self.config.read_mode != ReadMode::Snapshot {
                            self.hydrate_function_read_fields(request, &fields);
                        }
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

fn merge_resolved_extensions(body: &mut Value, responses: &BTreeMap<String, Response>) {
    let Some(body_object) = body.as_object_mut() else {
        return;
    };
    for response in responses.values() {
        let Some(source) = response.body.get("extensions").and_then(Value::as_object) else {
            continue;
        };
        let target = body_object.entry("extensions").or_insert_with(|| json!({}));
        let Some(target) = target.as_object_mut() else {
            continue;
        };
        for (name, value) in source {
            match (target.get_mut(name), value) {
                (Some(Value::Array(existing)), Value::Array(additional)) => {
                    for item in additional {
                        if !existing.contains(item) {
                            existing.push(item.clone());
                        }
                    }
                }
                (Some(_), _) => {}
                (None, _) => {
                    target.insert(name.clone(), value.clone());
                }
            }
        }
    }
}

fn strip_cloud_webhook_callback_urls(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                strip_cloud_webhook_callback_urls(value);
            }
        }
        Value::Object(object) => {
            let cloud_endpoint = object.get("endpoint").is_some_and(|endpoint| {
                matches!(
                    endpoint.get("__typename").and_then(Value::as_str),
                    Some("WebhookPubSubEndpoint" | "WebhookEventBridgeEndpoint")
                ) || endpoint.get("pubSubProject").is_some()
                    || endpoint.get("pubSubTopic").is_some()
                    || endpoint.get("arn").is_some()
            });
            if cloud_endpoint {
                // Shopify omits the deprecated non-null callbackUrl field for
                // cloud webhook destinations. The local record carries a
                // placeholder only long enough for GraphQL non-null execution;
                // it must not escape in the wire response.
                object.remove("callbackUrl");
            }
            for value in object.values_mut() {
                strip_cloud_webhook_callback_urls(value);
            }
        }
        _ => {}
    }
}

fn shopify_engine_response(
    engine_response: async_graphql::Response,
    version: AdminApiVersion,
    document: Option<&ParsedDocument>,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
) -> Value {
    let mut body = serde_json::to_value(engine_response)
        .unwrap_or_else(|error| json!({ "errors": [{ "message": error.to_string() }] }));
    if let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) {
        let explicit_error_roots = errors
            .iter()
            .filter(|error| {
                error.get("message").and_then(Value::as_str)
                    != Some("internal: non-null types require a return value")
            })
            .filter_map(|error| error.get("path")?.as_array()?.first().cloned())
            .collect::<Vec<_>>();
        errors.retain(|error| {
            if error.get("message").and_then(Value::as_str)
                != Some("internal: non-null types require a return value")
            {
                return true;
            }
            let root = error
                .get("path")
                .and_then(Value::as_array)
                .and_then(|path| path.first());
            !root.is_some_and(|root| explicit_error_roots.contains(root))
        });
    }
    normalize_engine_error_paths(&mut body);
    let validation_only = body
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| {
            !errors.is_empty() && errors.iter().all(|error| error.get("path").is_none())
        });
    if !validation_only {
        return body;
    }

    // Shopify omits `data` for parse/validation failures. async-graphql emits
    // `data: null`, so normalize only this pre-execution response envelope.
    if let Some(object) = body.as_object_mut() {
        object.remove("data");
    }
    if let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) {
        for error in errors {
            let Some(message_text) = error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            match message_text.as_str() {
                "Operation name required in request." => {
                    error["message"] = json!("An operation name is required");
                }
                message_text
                    if message_text.starts_with("Unknown operation named \"")
                        && (message_text.ends_with('"') || message_text.ends_with("\".")) =>
                {
                    error["message"] = json!(format!(
                        "No operation named {}",
                        message_text
                            .trim_start_matches("Unknown operation named ")
                            .trim_end_matches('.')
                    ));
                }
                message_text if document.is_none() && message_text.contains("expected ") => {
                    let engine_location = error_location(error).unwrap_or((1, 1));
                    let (line, column) = shopify_parse_error_location(query, engine_location);
                    *error = json!({
                        "message": format!(
                            "syntax error, unexpected end of file at [{line}, {column}]"
                        ),
                        "locations": [{ "line": line, "column": column }],
                        "extensions": { "code": "PARSE_ERROR" }
                    });
                }
                message_text => {
                    if let Some(variable_error) = shopify_variable_input_error(
                        version,
                        message_text,
                        document,
                        variables,
                        variable_input_orders,
                        error,
                    ) {
                        *error = variable_error;
                        continue;
                    }
                    if let Some(input_error) =
                        shopify_input_literal_error(version, message_text, document, query, error)
                    {
                        *error = input_error;
                        continue;
                    }
                    if let Some(directive_error) = shopify_unknown_directive_argument_error(
                        message_text,
                        query,
                        variables,
                        error,
                    ) {
                        *error = directive_error;
                        continue;
                    }
                    if let Some(directive_error) = shopify_directive_literal_error(
                        message_text,
                        query,
                        variables,
                        error_location(error),
                    ) {
                        *error = directive_error;
                        continue;
                    }
                    if let Some(argument_error) =
                        shopify_unknown_field_argument_error(message_text, document, error)
                    {
                        *error = argument_error;
                        continue;
                    }
                    if let Some((directive_name, argument_name)) =
                        async_graphql_missing_directive_argument(message_text)
                    {
                        let path = document
                            .and_then(|document| {
                                error_location(error).and_then(|location| {
                                    response_path_for_location(document, location)
                                })
                            })
                            .unwrap_or_default();
                        let locations =
                            error.get("locations").cloned().unwrap_or_else(|| json!([]));
                        *error = json!({
                            "message": format!(
                                "Directive '{directive_name}' is missing required arguments: {argument_name}"
                            ),
                            "locations": locations,
                            "path": path,
                            "extensions": {
                                "code": "missingRequiredArguments",
                                "className": "Directive",
                                "name": directive_name,
                                "arguments": argument_name
                            }
                        });
                        continue;
                    }
                    if let Some((field_name, type_name)) =
                        async_graphql_missing_selection(message_text)
                    {
                        let path = document
                            .and_then(|document| {
                                error_location(error).and_then(|location| {
                                    response_path_for_location(document, location)
                                })
                            })
                            .unwrap_or_else(|| vec![json!(field_name)]);
                        let locations =
                            error.get("locations").cloned().unwrap_or_else(|| json!([]));
                        *error = json!({
                            "message": format!(
                                "Field must have selections (field '{field_name}' returns {type_name} but has no selections. Did you mean '{field_name} {{ ... }}'?)"
                            ),
                            "locations": locations,
                            "path": path,
                            "extensions": {
                                "code": "selectionMismatch",
                                "nodeName": format!("field '{field_name}'"),
                                "typeName": type_name
                            }
                        });
                        continue;
                    }
                    let Some((field_name, type_name)) = async_graphql_unknown_field(message_text)
                    else {
                        continue;
                    };
                    let path = document
                        .and_then(|document| {
                            error_location(error)
                                .and_then(|location| response_path_for_location(document, location))
                        })
                        .unwrap_or_else(|| vec![json!(field_name)]);
                    let locations = error.get("locations").cloned().unwrap_or_else(|| json!([]));
                    *error = json!({
                        "message": format!(
                            "Field '{field_name}' doesn't exist on type '{type_name}'"
                        ),
                        "locations": locations,
                        "path": path,
                        "extensions": {
                            "code": "undefinedField",
                            "typeName": type_name,
                            "fieldName": field_name
                        }
                    });
                }
            }
        }
    }

    if let (Some(document), Some(errors)) = (
        document,
        body.get_mut("errors").and_then(Value::as_array_mut),
    ) {
        *errors = errors
            .drain(..)
            .flat_map(|error| {
                expand_inline_unknown_input_errors(version, document, query, &error)
                    .or_else(|| expand_inline_missing_input_errors(version, document, &error))
                    .unwrap_or_else(|| vec![error])
            })
            .collect();
    }

    let Some(document) = document else {
        return body;
    };
    let Some(errors) = body.get("errors").and_then(Value::as_array) else {
        return body;
    };
    let mut grouped = Vec::<(String, Vec<String>)>::new();
    for error in errors {
        let Some((field_name, argument_name)) = error
            .get("message")
            .and_then(Value::as_str)
            .and_then(async_graphql_missing_field_argument)
        else {
            continue;
        };
        if let Some((_, arguments)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing == &field_name)
        {
            arguments.push(argument_name);
        } else {
            grouped.push((field_name, vec![argument_name]));
        }
    }
    if grouped.is_empty()
        || grouped.iter().map(|(_, args)| args.len()).sum::<usize>() != errors.len()
    {
        return body;
    }

    let normalized = grouped
        .into_iter()
        .map(|(field_name, arguments)| {
            let arguments = arguments.join(", ");
            let root = document
                .root_fields
                .iter()
                .find(|field| field.name == field_name);
            let location = root
                .map(|field| field.location)
                .unwrap_or(document.location);
            let response_key = root
                .map(|field| field.response_key.as_str())
                .unwrap_or(field_name.as_str());
            missing_required_arguments_error(
                &field_name,
                &arguments,
                location,
                document_field_path(document, response_key),
            )
        })
        .collect::<Vec<_>>();
    body["errors"] = Value::Array(normalized);
    body
}

fn normalize_engine_error_paths(body: &mut Value) {
    let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) else {
        return;
    };
    for error in errors {
        let Some(path) = error.get_mut("path").and_then(Value::as_array_mut) else {
            continue;
        };
        let duplicated_root = path.len() >= 3
            && path.first() == path.get(2)
            && path.get(1).and_then(Value::as_str).is_some_and(|segment| {
                segment == "query"
                    || segment == "mutation"
                    || segment == "subscription"
                    || segment.starts_with("query ")
                    || segment.starts_with("mutation ")
                    || segment.starts_with("subscription ")
            });
        if duplicated_root {
            path.remove(0);
        }
    }
}

fn shopify_root_id_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let mut reported_variables = BTreeSet::new();
    for field in &document.root_fields {
        for (argument_name, raw_value) in &field.raw_arguments {
            let Some(metadata) = admin_graphql::input_field_at_path(
                version,
                document.operation_type,
                &field.name,
                &[argument_name],
            ) else {
                continue;
            };
            match raw_value {
                RawArgumentValue::Variable { name, .. }
                    if reported_variables.insert(name.clone()) =>
                {
                    let Some(value) = variables.get(name) else {
                        continue;
                    };
                    let problems = variable_global_id_problems(
                        version,
                        &metadata,
                        value,
                        &[],
                        metadata.named_type == "ID",
                    );
                    if problems.is_empty() {
                        continue;
                    }
                    let Some(definition) = document.variable_definitions.get(name) else {
                        continue;
                    };
                    let context = VariableValidationContext {
                        variable_name: name,
                        variable_type: &definition.type_display,
                        location: definition.location,
                    };
                    if problems.iter().any(|problem| {
                        problem
                            .get("path")
                            .and_then(Value::as_array)
                            .is_some_and(|path| !path.is_empty())
                    }) {
                        errors.push(invalid_variable_error(context, value, problems));
                    } else {
                        errors.push(invalid_variable_error_envelope(
                            format!(
                                "Variable ${name} of type {} was provided invalid value",
                                definition.type_display
                            ),
                            definition.location,
                            resolved_value_json(value),
                            Value::Array(problems),
                        ));
                    }
                }
                _ => collect_inline_global_id_errors(
                    version,
                    document,
                    query,
                    field,
                    raw_value,
                    &mut vec![argument_name.clone()],
                    &mut errors,
                ),
            }
        }
    }
    errors
}

fn collect_inline_global_id_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    field: &RootFieldSelection,
    value: &RawArgumentValue,
    path: &mut Vec<String>,
    errors: &mut Vec<Value>,
) {
    let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
    let Some(metadata) = admin_graphql::input_field_at_path(
        version,
        document.operation_type,
        &field.name,
        &path_refs,
    ) else {
        return;
    };
    if metadata.named_type == "ID" {
        let strict_root_id = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &[path.first().map(String::as_str).unwrap_or_default()],
        )
        .is_some_and(|root| root.named_type == "ID");
        let invalid = match value {
            RawArgumentValue::String(value)
                if shopify_gid_resource_type(value).is_none()
                    && (strict_root_id || value.is_empty()) =>
            {
                Some(value.clone())
            }
            RawArgumentValue::Int(value) if strict_root_id => Some(value.to_string()),
            _ => None,
        };
        if let Some(invalid) = invalid {
            errors.push(invalid_global_id_literal_error(
                version, document, field, query, path, &invalid,
            ));
        }
        if let RawArgumentValue::List(values) = value {
            for (index, value) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        return;
    }
    match value {
        RawArgumentValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        RawArgumentValue::Object(fields) => {
            for (name, value) in fields {
                path.push(name.clone());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        _ => {}
    }
}

fn variable_global_id_problems(
    version: AdminApiVersion,
    field: &admin_graphql::InputFieldMetadata,
    value: &ResolvedValue,
    path: &[Value],
    strict: bool,
) -> Vec<Value> {
    if field.list {
        let ResolvedValue::List(values) = value else {
            return Vec::new();
        };
        return values
            .iter()
            .enumerate()
            .flat_map(|(index, value)| {
                let mut item_path = path.to_vec();
                item_path.push(json!(index));
                variable_named_global_id_problems(
                    version,
                    &field.named_type,
                    value,
                    &item_path,
                    strict,
                )
            })
            .collect();
    }
    variable_named_global_id_problems(version, &field.named_type, value, path, strict)
}

fn variable_named_global_id_problems(
    version: AdminApiVersion,
    named_type: &str,
    value: &ResolvedValue,
    path: &[Value],
    strict: bool,
) -> Vec<Value> {
    if named_type == "ID" {
        let invalid_id = match value {
            ResolvedValue::String(value)
                if shopify_gid_resource_type(value).is_none() && (strict || value.is_empty()) =>
            {
                Some(value.clone())
            }
            ResolvedValue::Int(value) if strict => Some(value.to_string()),
            _ => None,
        };
        return invalid_id
            .map(|invalid_id| {
                let explanation = format!("Invalid global id '{invalid_id}'");
                vec![variable_problem_with_message_value_path(path, &explanation)]
            })
            .unwrap_or_default();
    }
    let Some(input_fields) = admin_graphql::input_object_fields(version, named_type) else {
        return Vec::new();
    };
    let ResolvedValue::Object(values) = value else {
        return Vec::new();
    };
    input_fields
        .into_iter()
        .filter_map(|field| values.get(&field.name).map(|value| (field, value)))
        .flat_map(|(field, value)| {
            let mut field_path = path.to_vec();
            field_path.push(json!(field.name));
            variable_global_id_problems(version, &field, value, &field_path, false)
        })
        .collect()
}

fn invalid_global_id_literal_error(
    version: AdminApiVersion,
    document: &ParsedDocument,
    field: &RootFieldSelection,
    query: &str,
    input_path: &[String],
    invalid_id: &str,
) -> Value {
    let mut path = document_field_path(document, &field.response_key);
    let root_is_id_list = input_path.first().is_some_and(|argument_name| {
        admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &[argument_name],
        )
        .is_some_and(|metadata| metadata.named_type == "ID" && metadata.list)
    });
    let error_input_path = if root_is_id_list {
        &input_path[..1]
    } else {
        input_path
    };
    path.extend(input_path_values(
        error_input_path.iter().map(String::as_str),
    ));
    let location = input_path
        .iter()
        .enumerate()
        .rev()
        .find_map(|(position, segment)| {
            segment.parse::<usize>().ok().and_then(|index| {
                inline_argument_list_item_object_location(
                    query,
                    field,
                    input_path.first().map(String::as_str).unwrap_or_default(),
                    index,
                )
                .filter(|_| position + 1 < input_path.len())
            })
        })
        .unwrap_or(field.location);
    json!({
        "message": format!("Invalid global id '{invalid_id}'"),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": path,
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

fn error_location(error: &Value) -> Option<(usize, usize)> {
    let location = error.get("locations")?.as_array()?.first()?;
    Some((
        location.get("line")?.as_u64()? as usize,
        location.get("column")?.as_u64()? as usize,
    ))
}

fn shopify_parse_error_location(query: &str, engine_location: (usize, usize)) -> (usize, usize) {
    let last_token_start = query
        .lines()
        .enumerate()
        .filter_map(|(line, source)| {
            source
                .chars()
                .position(|character| !character.is_whitespace())
                .map(|column| (line + 1, column + 1))
        })
        .last();

    match last_token_start {
        // async-graphql locates an unexpected EOF after a trailing newline on
        // the following empty line. Shopify reports the start of the final
        // token instead, which is also what its captured PARSE_ERROR message
        // embeds.
        Some(location) if engine_location.0 > location.0 => location,
        _ => engine_location,
    }
}

fn async_graphql_unknown_field(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Unknown field \"")?;
    let (field_name, rest) = rest.split_once("\" on type \"")?;
    let (type_name, _) = rest.split_once('"')?;
    Some((field_name.to_string(), type_name.to_string()))
}

fn async_graphql_missing_selection(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Field \"")?;
    let (field_name, rest) = rest.split_once("\" of type \"")?;
    let (type_name, rest) = rest.split_once('"')?;
    rest.contains("must have a selection of subfields")
        .then(|| (field_name.to_string(), type_name.to_string()))
}

fn async_graphql_missing_directive_argument(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Directive \"@")?;
    let (directive_name, rest) = rest.split_once("\" argument \"")?;
    let (argument_name, rest) = rest.split_once('"')?;
    rest.contains("is required but not provided")
        .then(|| (directive_name.to_string(), argument_name.to_string()))
}

fn shopify_unknown_field_argument_error(
    message: &str,
    document: Option<&ParsedDocument>,
    engine_error: &Value,
) -> Option<Value> {
    let rest = message.strip_prefix("Unknown argument \"")?;
    let (argument_name, rest) = rest.split_once("\" on field \"")?;
    let (field_name, _) = rest.split_once('"')?;
    let document = document?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == field_name)?;
    let mut path = document_field_path(document, &field.response_key);
    path.push(json!(argument_name));
    Some(json!({
        "message": format!("Field '{field_name}' doesn't accept argument '{argument_name}'"),
        "locations": engine_error
            .get("locations")
            .cloned()
            .unwrap_or_else(|| json!([{ "line": field.location.line, "column": field.location.column }])),
        "path": path,
        "extensions": {
            "code": "argumentNotAccepted",
            "name": field_name,
            "typeName": "Field",
            "argumentName": argument_name
        }
    }))
}

fn shopify_directive_literal_error(
    message: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    error_location: Option<(usize, usize)>,
) -> Option<Value> {
    let rest = message.strip_prefix("Invalid value for argument \"")?;
    let (argument_name, _) = rest.split_once('"')?;
    let invocations = directive_invocations(query, variables)?;
    let invocation = invocations
        .iter()
        .filter(|invocation| invocation.raw_arguments.contains_key(argument_name))
        .filter(|invocation| {
            error_location.is_none_or(|location| {
                (invocation.location.line, invocation.location.column) <= location
            })
        })
        .max_by_key(|invocation| (invocation.location.line, invocation.location.column))?;
    let expected_type = match (invocation.name.as_str(), argument_name) {
        ("include" | "skip", "if") => "Boolean!",
        _ => return None,
    };
    if let Some(RawArgumentValue::Variable { name, .. }) =
        invocation.raw_arguments.get(argument_name)
    {
        let value = supplied_variable(variables, name)?;
        if matches!(value, ResolvedValue::Bool(_)) {
            return None;
        }
        let definition = variable_definition_info(query, name)?;
        let json_value = resolved_value_json(value);
        return Some(json!({
            "message": format!(
                "Variable ${name} of type {} was provided invalid value",
                definition.type_display
            ),
            "locations": [{
                "line": definition.location.line,
                "column": definition.location.column
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": json_value,
                "problems": [{
                    "path": [],
                    "explanation": format!(
                        "Could not coerce value {} to Boolean",
                        resolved_value_json(value)
                    )
                }]
            }
        }));
    }
    if invocation.raw_arguments.get(argument_name) != Some(&RawArgumentValue::Null) {
        return None;
    }
    let mut path = invocation
        .path
        .iter()
        .map(|segment| json!(segment))
        .collect::<Vec<_>>();
    path.push(json!(argument_name));
    Some(json!({
        "message": format!(
            "Argument '{argument_name}' on Directive '{}' has an invalid value (null). Expected type '{expected_type}'.",
            invocation.name
        ),
        "locations": [{
            "line": invocation.location.line,
            "column": invocation.location.column
        }],
        "path": path,
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Directive",
            "argumentName": argument_name
        }
    }))
}

fn shopify_variable_input_error(
    version: AdminApiVersion,
    message: &str,
    document: Option<&ParsedDocument>,
    variables: &BTreeMap<String, ResolvedValue>,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
    engine_error: &Value,
) -> Option<Value> {
    let document = document?;
    let argument_path = async_graphql_input_argument_path(message)?;
    let path = argument_path.split('.').collect::<Vec<_>>();
    let field = input_error_root_field(document, &path, error_location(engine_error))?;
    let (variable_name, variable_field, variable_path) =
        input_error_variable(version, document, field, &path)?;
    let definition = document.variable_definitions.get(variable_name)?;
    let value = variables.get(variable_name)?;
    let mut problems = variable_value_problems(version, &variable_field, value, &variable_path);
    if problems.is_empty() {
        return None;
    }
    sort_variable_problems_by_input_order(&mut problems, variable_name, variable_input_orders);
    let context = VariableValidationContext {
        variable_name,
        variable_type: &definition.type_display,
        location: definition.location,
    };
    let has_nested_path = problems.iter().any(|problem| {
        problem
            .get("path")
            .and_then(Value::as_array)
            .is_some_and(|path| !path.is_empty())
    });
    Some(if has_nested_path {
        invalid_variable_error(context, value, problems)
    } else {
        invalid_variable_error_envelope(
            format!(
                "Variable ${variable_name} of type {} was provided invalid value",
                definition.type_display
            ),
            definition.location,
            resolved_value_json(value),
            Value::Array(problems),
        )
    })
}

fn sort_variable_problems_by_input_order(
    problems: &mut [Value],
    variable_name: &str,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
) {
    problems.sort_by_key(|problem| {
        let mut parent_path = vec![variable_name.to_string()];
        problem
            .get("path")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|segment| {
                let segment = segment
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| segment.as_u64().map(|value| value.to_string()))
                    .unwrap_or_else(|| segment.to_string());
                let rank = segment.parse::<usize>().ok().unwrap_or_else(|| {
                    variable_input_orders
                        .get(&parent_path)
                        .and_then(|fields| fields.iter().position(|field| field == &segment))
                        .unwrap_or(usize::MAX)
                });
                parent_path.push(segment.clone());
                rank
            })
            .collect::<Vec<_>>()
    });
}

fn async_graphql_input_argument_path(message: &str) -> Option<&str> {
    message
        .strip_prefix("Invalid value for argument \"")?
        .split_once("\",")
        .map(|(path, _)| path)
}

fn input_error_root_field<'a>(
    document: &'a ParsedDocument,
    path: &[&str],
    location: Option<(usize, usize)>,
) -> Option<&'a RootFieldSelection> {
    let root_argument = path.first()?;
    let candidates = document
        .root_fields
        .iter()
        .filter(|field| field.raw_arguments.contains_key(*root_argument));
    if let Some(location) = location {
        candidates
            .filter(|field| (field.location.line, field.location.column) <= location)
            .max_by_key(|field| (field.location.line, field.location.column))
            .or_else(|| {
                document
                    .root_fields
                    .iter()
                    .find(|field| field.raw_arguments.contains_key(*root_argument))
            })
    } else {
        candidates.into_iter().next()
    }
}

fn input_error_variable<'a>(
    version: AdminApiVersion,
    document: &ParsedDocument,
    field: &'a RootFieldSelection,
    path: &[&str],
) -> Option<(&'a str, admin_graphql::InputFieldMetadata, Vec<Value>)> {
    let root_argument = path.first()?;
    let root_value = field.raw_arguments.get(*root_argument)?;
    let root_field = admin_graphql::input_field_at_path(
        version,
        document.operation_type,
        &field.name,
        &path[..1],
    )?;
    if let RawArgumentValue::Variable { name, .. } = root_value {
        return Some((name, root_field, Vec::new()));
    }

    let mut raw_value = root_value;
    for (index, segment) in path.iter().enumerate().skip(1) {
        raw_value = match raw_value {
            RawArgumentValue::Object(fields) => fields.get(*segment)?,
            RawArgumentValue::List(items) => items.get(segment.parse::<usize>().ok()?)?,
            RawArgumentValue::Variable { name, .. } => {
                let metadata = admin_graphql::input_field_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path[..index],
                )?;
                return Some((
                    name,
                    metadata,
                    input_path_values(path.iter().skip(index).copied()),
                ));
            }
            _ => return None,
        };
    }
    let RawArgumentValue::Variable { name, .. } = raw_value else {
        return None;
    };
    Some((
        name,
        admin_graphql::input_field_at_path(version, document.operation_type, &field.name, path)?,
        Vec::new(),
    ))
}

fn input_path_values<'a>(segments: impl Iterator<Item = &'a str>) -> Vec<Value> {
    segments
        .map(|segment| {
            segment
                .parse::<u64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(segment.to_string()))
        })
        .collect()
}

fn variable_value_problems(
    version: AdminApiVersion,
    field: &admin_graphql::InputFieldMetadata,
    value: &ResolvedValue,
    path: &[Value],
) -> Vec<Value> {
    if field.list {
        let ResolvedValue::List(values) = value else {
            return Vec::new();
        };
        return values
            .iter()
            .enumerate()
            .flat_map(|(index, value)| {
                let mut item_path = path.to_vec();
                item_path.push(json!(index));
                if matches!(value, ResolvedValue::Null) && field.list_item_required {
                    vec![variable_problem_value_path(
                        &item_path,
                        "Expected value to not be null",
                    )]
                } else {
                    variable_named_value_problems(version, &field.named_type, value, &item_path)
                }
            })
            .collect();
    }
    variable_named_value_problems(version, &field.named_type, value, path)
}

fn variable_named_value_problems(
    version: AdminApiVersion,
    named_type: &str,
    value: &ResolvedValue,
    path: &[Value],
) -> Vec<Value> {
    let scalar_problem = match (named_type, value) {
        ("Int", ResolvedValue::Int(_))
        | ("Float", ResolvedValue::Int(_) | ResolvedValue::Float(_))
        | ("String", ResolvedValue::String(_))
        | ("Boolean", ResolvedValue::Bool(_)) => None,
        ("Int", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Int",
                resolved_value_json(value)
            ),
        )),
        ("Float", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Float",
                resolved_value_json(value)
            ),
        )),
        ("String", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to String",
                resolved_value_json(value)
            ),
        )),
        ("Boolean", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Boolean",
                resolved_value_json(value)
            ),
        )),
        ("Decimal", ResolvedValue::String(raw))
            if raw.parse::<f64>().is_err() || !raw.parse::<f64>().is_ok_and(f64::is_finite) =>
        {
            let explanation = format!("invalid decimal '{raw}'");
            Some(variable_problem_with_message_value_path(path, &explanation))
        }
        ("Decimal", ResolvedValue::Int(_) | ResolvedValue::Float(_)) => None,
        _ => None,
    };
    if let Some(problem) = scalar_problem {
        return vec![problem];
    }
    if named_type == "ID" {
        let invalid_id = match value {
            ResolvedValue::String(value) if shopify_gid_resource_type(value).is_none() => {
                Some(value.clone())
            }
            ResolvedValue::Int(value) => Some(value.to_string()),
            _ => None,
        };
        if let Some(invalid_id) = invalid_id {
            let explanation = format!("Invalid global id '{invalid_id}'");
            return vec![variable_problem_with_message_value_path(path, &explanation)];
        }
    }
    if let Some(input_fields) = admin_graphql::input_object_fields(version, named_type) {
        let ResolvedValue::Object(values) = value else {
            return Vec::new();
        };
        let known_fields = input_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut problems = values
            .keys()
            .filter(|name| !known_fields.contains(name.as_str()))
            .map(|name| {
                let mut field_path = path.to_vec();
                field_path.push(json!(name));
                variable_problem_value_path(
                    &field_path,
                    &format!("Field is not defined on {named_type}"),
                )
            })
            .collect::<Vec<_>>();
        for input_field in input_fields {
            let provided = values.get(&input_field.name);
            if input_field.required
                && provided.is_none_or(|value| matches!(value, ResolvedValue::Null))
            {
                let mut field_path = path.to_vec();
                field_path.push(json!(input_field.name));
                problems.push(variable_problem_value_path(
                    &field_path,
                    "Expected value to not be null",
                ));
                continue;
            }
            let Some(value) = provided else {
                continue;
            };
            let mut field_path = path.to_vec();
            field_path.push(json!(input_field.name));
            problems.extend(variable_value_problems(
                version,
                &input_field,
                value,
                &field_path,
            ));
        }
        return problems;
    }

    if let (Some(values), ResolvedValue::String(value)) =
        (admin_graphql::enum_values(version, named_type), value)
    {
        if !values.iter().any(|candidate| candidate == value) {
            return vec![variable_problem_value_path(
                path,
                &format!("Expected \"{value}\" to be one of: {}", values.join(", ")),
            )];
        }
    }
    Vec::new()
}

fn shopify_input_literal_error(
    version: AdminApiVersion,
    message: &str,
    document: Option<&ParsedDocument>,
    query: &str,
    engine_error: &Value,
) -> Option<Value> {
    let document = document?;
    let argument_path = async_graphql_input_argument_path(message)?;
    let path = argument_path.split('.').collect::<Vec<_>>();
    let field = input_error_root_field(document, &path, error_location(engine_error))?;
    if input_error_variable(version, document, field, &path).is_some() {
        return None;
    }
    let path_values = input_path_values(path.iter().copied());
    let locations =
        |location: SourceLocation| json!([{ "line": location.line, "column": location.column }]);

    let rest = message
        .strip_prefix("Invalid value for argument \"")?
        .split_once("\",")?
        .1
        .trim_start();
    if let Some(rest) = rest.strip_prefix("unknown field \"") {
        let (argument_name, rest) = rest.split_once("\" of type \"")?;
        let (input_type, _) = rest.split_once('"')?;
        let location = inline_input_field_name_location(
            query,
            field.location,
            1 + path.len() as i32,
            argument_name,
        )
        .unwrap_or(field.location);
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        error_path.push(json!(argument_name));
        return Some(json!({
            "message": format!("InputObject '{input_type}' doesn't accept argument '{argument_name}'"),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentNotAccepted",
                "name": input_type,
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        }));
    }
    if let Some(rest) = rest.strip_prefix("field \"") {
        let (argument_name, rest) = rest.split_once("\" of type \"")?;
        let (argument_type, rest) = rest.split_once('"')?;
        if !rest.contains("is required but not provided") {
            return None;
        }
        let input_type = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?
        .named_type;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        error_path.push(json!(argument_name));
        let location = if path.len() == 1 {
            inline_argument_value_location(query, field, path[0])
        } else if let Some(index) = path
            .last()
            .and_then(|segment| segment.parse::<usize>().ok())
        {
            inline_argument_list_item_object_location(query, field, path[0], index)
        } else {
            inline_input_field_value_location(
                query,
                field.location,
                path.len() as i32,
                path.last().copied().unwrap_or(path[0]),
            )
            .or_else(|| inline_argument_value_location(query, field, path[0]))
        }
        .unwrap_or(field.location);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on InputObject '{input_type}' is required. Expected type {argument_type}"
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": argument_name,
                "argumentType": argument_type,
                "inputObjectType": input_type
            }
        }));
    }
    if let Some(rest) = rest.strip_prefix("enumeration type \"") {
        let (_enum_type, rest) = rest.split_once("\" does not contain the value \"")?;
        let leaf_invalid_value = rest.strip_suffix('"')?;
        let semantic_path = path
            .iter()
            .copied()
            .rev()
            .skip_while(|segment| segment.parse::<usize>().is_ok())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        let invalid_value = if semantic_path.len() != path.len() {
            raw_input_value_at_path(field, &semantic_path)
                .map(raw_input_display)
                .unwrap_or_else(|| leaf_invalid_value.to_string())
        } else {
            leaf_invalid_value.to_string()
        };
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &semantic_path,
        )?;
        let (owner_kind, owner_name, location) = if semantic_path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &semantic_path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = semantic_path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(input_path_values(semantic_path.iter().copied()));
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value ({invalid_value}). Expected type '{}'.",
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    if rest.starts_with("expected type \"")
        && matches!(
            raw_input_value_at_path(field, &path),
            Some(RawArgumentValue::Null)
        )
    {
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?;
        let (owner_kind, owner_name, location) = if path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value (null). Expected type '{}'.",
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    if rest == "expected type \"ID\"" {
        let raw_value = raw_input_value_at_path(field, &path)?;
        let invalid_id = match raw_value {
            RawArgumentValue::String(value) => value.clone(),
            RawArgumentValue::Int(value) => value.to_string(),
            _ => return None,
        };
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!("Invalid global id '{invalid_id}'"),
            "locations": locations(field.location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "CoercionError"
            }
        }));
    }
    if rest.starts_with("expected type \"") {
        let raw_value = raw_input_value_at_path(field, &path)?;
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?;
        let (owner_kind, owner_name, location) = if path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value ({}). Expected type '{}'.",
                raw_input_display(raw_value),
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    None
}

fn expand_inline_unknown_input_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    error: &Value,
) -> Option<Vec<Value>> {
    let extensions = error.get("extensions")?.as_object()?;
    if extensions.get("code").and_then(Value::as_str) != Some("argumentNotAccepted")
        || extensions.get("typeName").and_then(Value::as_str) != Some("InputObject")
    {
        return None;
    }
    let input_type = extensions.get("name")?.as_str()?;
    let error_path = error.get("path")?.as_array()?;
    if error_path.len() < 4 {
        return None;
    }
    let response_key = error_path.get(1)?.as_str()?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.response_key == response_key)?;
    let parent_path = error_path[2..error_path.len() - 1]
        .iter()
        .map(|segment| {
            segment
                .as_str()
                .map(str::to_string)
                .or_else(|| segment.as_u64().map(|value| value.to_string()))
        })
        .collect::<Option<Vec<_>>>()?;
    let parent_path_refs = parent_path.iter().map(String::as_str).collect::<Vec<_>>();
    let RawArgumentValue::Object(input_fields) = raw_input_value_at_path(field, &parent_path_refs)?
    else {
        return None;
    };
    let known_fields = admin_graphql::input_object_fields(version, input_type)?
        .into_iter()
        .map(|field| field.name)
        .collect::<BTreeSet<_>>();
    let target_depth = 1 + parent_path.len() as i32;
    let fallback_location = error_location(error)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(field.location);
    let mut unknown_fields = input_fields
        .keys()
        .filter(|name| !known_fields.contains(*name))
        .map(|name| {
            let location =
                inline_input_field_name_location(query, field.location, target_depth, name)
                    .unwrap_or(fallback_location);
            (location, name.clone())
        })
        .collect::<Vec<_>>();
    if unknown_fields.len() <= 1 {
        return None;
    }
    unknown_fields.sort_by_key(|(location, name)| (location.line, location.column, name.clone()));

    Some(
        unknown_fields
            .into_iter()
            .map(|(location, argument_name)| {
                let mut path = document_field_path(document, response_key);
                path.extend(input_path_values(parent_path_refs.iter().copied()));
                path.push(json!(argument_name));
                json!({
                    "message": format!(
                        "InputObject '{input_type}' doesn't accept argument '{argument_name}'"
                    ),
                    "locations": [{ "line": location.line, "column": location.column }],
                    "path": path,
                    "extensions": {
                        "code": "argumentNotAccepted",
                        "name": input_type,
                        "typeName": "InputObject",
                        "argumentName": argument_name
                    }
                })
            })
            .collect(),
    )
}

fn expand_inline_missing_input_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    error: &Value,
) -> Option<Vec<Value>> {
    let extensions = error.get("extensions")?.as_object()?;
    if extensions.get("code").and_then(Value::as_str) != Some("missingRequiredInputObjectAttribute")
    {
        return None;
    }
    let input_type = extensions.get("inputObjectType")?.as_str()?;
    let error_path = error.get("path")?.as_array()?;
    if error_path.len() < 4 {
        return None;
    }
    let response_key = error_path.get(1)?.as_str()?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.response_key == response_key)?;
    let parent_path = error_path[2..error_path.len() - 1]
        .iter()
        .map(|segment| {
            segment
                .as_str()
                .map(str::to_string)
                .or_else(|| segment.as_u64().map(|value| value.to_string()))
        })
        .collect::<Option<Vec<_>>>()?;
    let parent_path_refs = parent_path.iter().map(String::as_str).collect::<Vec<_>>();
    let RawArgumentValue::Object(provided_fields) =
        raw_input_value_at_path(field, &parent_path_refs)?
    else {
        return None;
    };
    let mut missing_fields = admin_graphql::input_object_fields(version, input_type)?
        .into_iter()
        .filter(|field| field.required && !provided_fields.contains_key(&field.name))
        .collect::<Vec<_>>();
    // async-graphql reports missing inline input fields in its registry order,
    // which is lexical. Expand the complete set in that same order while
    // variable coercion continues to follow the incoming JSON object order.
    missing_fields.sort_by(|left, right| left.name.cmp(&right.name));
    if missing_fields.len() <= 1 {
        return None;
    }
    let location = error_location(error)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(field.location);

    Some(
        missing_fields
            .into_iter()
            .map(|missing| {
                let mut path = document_field_path(document, response_key);
                path.extend(input_path_values(parent_path_refs.iter().copied()));
                path.push(json!(missing.name));
                json!({
                    "message": format!(
                        "Argument '{}' on InputObject '{input_type}' is required. Expected type {}",
                        missing.name, missing.type_display
                    ),
                    "locations": [{ "line": location.line, "column": location.column }],
                    "path": path,
                    "extensions": {
                        "code": "missingRequiredInputObjectAttribute",
                        "argumentName": missing.name,
                        "argumentType": missing.type_display,
                        "inputObjectType": input_type
                    }
                })
            })
            .collect(),
    )
}

fn raw_input_value_at_path<'a>(
    field: &'a RootFieldSelection,
    path: &[&str],
) -> Option<&'a RawArgumentValue> {
    let mut value = field.raw_arguments.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        value = match value {
            RawArgumentValue::Object(fields) => fields.get(*segment)?,
            RawArgumentValue::List(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn raw_input_display(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => json!(value).to_string(),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(raw_input_display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", raw_input_display(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Variable { name, .. } => format!("${name}"),
    }
}

fn shopify_unknown_directive_argument_error(
    message: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    engine_error: &Value,
) -> Option<Value> {
    let rest = message.strip_prefix("Unknown argument \"")?;
    let (argument_name, rest) = rest.split_once("\" on directive \"")?;
    let (directive_name, _) = rest.split_once('"')?;
    let invocation = directive_invocations(query, variables)?
        .into_iter()
        .find(|invocation| {
            invocation.name == directive_name
                && invocation.raw_arguments.contains_key(argument_name)
        })?;
    let mut path = invocation
        .path
        .iter()
        .map(|segment| json!(segment))
        .collect::<Vec<_>>();
    path.push(json!(argument_name));
    Some(json!({
        "message": format!(
            "Directive '{directive_name}' doesn't accept argument '{argument_name}'"
        ),
        "locations": engine_error
            .get("locations")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "path": path,
        "extensions": {
            "code": "argumentNotAccepted",
            "name": directive_name,
            "typeName": "Directive",
            "argumentName": argument_name
        }
    }))
}

fn supplied_variable<'a>(
    variables: &'a BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Option<&'a ResolvedValue> {
    variables.get(name)
}

fn response_path_for_location(
    document: &ParsedDocument,
    location: (usize, usize),
) -> Option<Vec<Value>> {
    for field in &document.root_fields {
        let mut path = document_field_path(document, &field.response_key);
        if (field.location.line, field.location.column) == location {
            return Some(path);
        }
        if selected_response_path_for_location(&field.selection, location, &mut path) {
            return Some(path);
        }
    }
    nearest_response_path(document, location)
}

fn nearest_response_path(
    document: &ParsedDocument,
    location: (usize, usize),
) -> Option<Vec<Value>> {
    let mut best: Option<((usize, usize), Vec<Value>)> = None;
    for field in &document.root_fields {
        let path = document_field_path(document, &field.response_key);
        consider_nearest_path(field.location, &path, location, &mut best);
        nearest_selected_response_path(&field.selection, location, path, &mut best);
    }
    best.map(|(_, path)| path)
}

fn document_field_path(document: &ParsedDocument, response_key: &str) -> Vec<Value> {
    vec![json!(document.operation_path), json!(response_key)]
}

fn nearest_selected_response_path(
    fields: &[SelectedField],
    location: (usize, usize),
    path: Vec<Value>,
    best: &mut Option<((usize, usize), Vec<Value>)>,
) {
    for field in fields {
        let mut field_path = path.clone();
        field_path.push(json!(field.response_key));
        consider_nearest_path(field.location, &field_path, location, best);
        nearest_selected_response_path(&field.selection, location, field_path, best);
    }
}

fn consider_nearest_path(
    candidate: SourceLocation,
    path: &[Value],
    location: (usize, usize),
    best: &mut Option<((usize, usize), Vec<Value>)>,
) {
    let position = (candidate.line, candidate.column);
    if position > location
        || best
            .as_ref()
            .is_some_and(|(current, _)| *current >= position)
    {
        return;
    }
    *best = Some((position, path.to_vec()));
}

fn selected_response_path_for_location(
    fields: &[SelectedField],
    location: (usize, usize),
    path: &mut Vec<Value>,
) -> bool {
    for field in fields {
        path.push(json!(field.response_key));
        if (field.location.line, field.location.column) == location
            || selected_response_path_for_location(&field.selection, location, path)
        {
            return true;
        }
        path.pop();
    }
    false
}

fn required_variable_error(
    document: &ParsedDocument,
    supplied_variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let definition = document.variable_definitions.values().find(|definition| {
        definition.type_display.ends_with('!')
            && match supplied_variables.get(&definition.name) {
                Some(ResolvedValue::Null) => true,
                Some(_) => false,
                None => definition.default_value.is_none(),
            }
    })?;
    Some(json!({
        "message": format!(
            "Variable ${} of type {} was provided invalid value",
            definition.name, definition.type_display
        ),
        "locations": [{
            "line": definition.location.line,
            "column": definition.location.column
        }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": null,
            "problems": [{
                "path": [],
                "explanation": "Expected value to not be null"
            }]
        }
    }))
}

fn product_create_argument_arity_error(document: &ParsedDocument) -> Option<Value> {
    if document.operation_type != OperationType::Mutation {
        return None;
    }
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "productCreate")?;
    let accepted_argument_count = usize::from(field.raw_arguments.contains_key("input"))
        + usize::from(field.raw_arguments.contains_key("product"));
    if accepted_argument_count == 1 {
        return None;
    }
    Some(json!({
        "data": { field.response_key.clone(): Value::Null },
        "errors": [{
            "message": "productCreate must include exactly one of the following arguments: input, product.",
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }]
    }))
}

fn directive_variable_mismatch_error(
    document: &ParsedDocument,
    query: &str,
    supplied_variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let invocations = directive_invocations(query, supplied_variables)?;
    for invocation in invocations {
        for (argument_name, argument) in &invocation.raw_arguments {
            let RawArgumentValue::Variable { name, .. } = argument else {
                continue;
            };
            let Some(definition) = document.variable_definitions.get(name) else {
                let location =
                    argument_value_location_after(query, invocation.location, argument_name)
                        .unwrap_or(invocation.location);
                let mut path = invocation
                    .path
                    .iter()
                    .map(|segment| json!(segment))
                    .collect::<Vec<_>>();
                path.push(json!(argument_name));
                let operation = document.operation_name.as_deref().map_or_else(
                    || format!("anonymous {}", document.operation_type.keyword()),
                    |name| format!("{} {name}", document.operation_type.keyword()),
                );
                return Some(json!({
                    "message": format!(
                        "Variable ${name} is used by {operation} but not declared"
                    ),
                    "locations": [{
                        "line": location.line,
                        "column": location.column
                    }],
                    "path": path,
                    "extensions": {
                        "code": "variableNotDefined",
                        "variableName": name
                    }
                }));
            };
            let expected_type = match (invocation.name.as_str(), argument_name.as_str()) {
                ("include" | "skip", "if") => "Boolean!",
                _ => continue,
            };
            if definition.type_display.ends_with('!') || definition.default_value.is_some() {
                continue;
            }
            let location = argument_name_location_after(query, invocation.location, argument_name)
                .unwrap_or(invocation.location);
            let mut path = invocation
                .path
                .iter()
                .map(|segment| json!(segment))
                .collect::<Vec<_>>();
            path.push(json!(argument_name));
            return Some(json!({
                "message": format!(
                    "Nullability mismatch on variable ${name} and argument {argument_name} ({} / {expected_type})",
                    definition.type_display
                ),
                "locations": [{
                    "line": location.line,
                    "column": location.column
                }],
                "path": path,
                "extensions": {
                    "code": "variableMismatch",
                    "variableName": name,
                    "typeName": definition.type_display,
                    "argumentName": argument_name,
                    "errorMessage": "Nullability mismatch"
                }
            }));
        }
    }
    None
}

fn async_graphql_missing_field_argument(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Field \"")?;
    let (field_name, rest) = rest.split_once("\" argument \"")?;
    let (argument_name, rest) = rest.split_once('"')?;
    rest.contains("is required but not provided")
        .then(|| (field_name.to_string(), argument_name.to_string()))
}

fn merge_resolved_root_responses(responses: &BTreeMap<String, Response>) -> Value {
    let mut data = serde_json::Map::new();
    let mut errors = Vec::new();
    for response in responses.values() {
        if let Some(response_data) = response.body.get("data").and_then(Value::as_object) {
            data.extend(response_data.clone());
        }
        if let Some(response_errors) = response.body.get("errors").and_then(Value::as_array) {
            errors.extend(response_errors.iter().cloned());
        }
    }
    let mut body = serde_json::Map::new();
    body.insert("data".to_string(), Value::Object(data));
    if !errors.is_empty() {
        body.insert("errors".to_string(), Value::Array(errors));
    }
    Value::Object(body)
}

fn strip_unselected_typenames_from_response(body: &mut Value, document: &ParsedDocument) {
    let Some(data) = body.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    for field in &document.root_fields {
        if let Some(value) = data.get_mut(&field.response_key) {
            strip_unselected_typenames(value, &field.selection);
        }
    }
}

fn strip_unselected_typenames(value: &mut Value, selection: &[SelectedField]) {
    if let Some(values) = value.as_array_mut() {
        for value in values {
            strip_unselected_typenames(value, selection);
        }
        return;
    }
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if !selection.iter().any(|field| field.name == "__typename") {
        object.remove("__typename");
    }
    for field in selection {
        if field.selection.is_empty() {
            continue;
        }
        if let Some(value) = object.get_mut(&field.response_key) {
            strip_unselected_typenames(value, &field.selection);
        }
    }
}

fn strip_idempotent_directives(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'#' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'"' if bytes.get(index..index + 3) == Some(b"\"\"\"") => {
                index += 3;
                while index < bytes.len() {
                    if bytes.get(index..index + 3) == Some(b"\"\"\"") {
                        index += 3;
                        break;
                    }
                    index += 1;
                }
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'@' if bytes.get(index + 1..index + 11) == Some(b"idempotent")
                && bytes
                    .get(index + 11)
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_') =>
            {
                let start = index;
                index += 11;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                if bytes.get(index) == Some(&b'(') {
                    let mut depth = 0usize;
                    while index < bytes.len() {
                        match bytes[index] {
                            b'"' => {
                                index += 1;
                                while index < bytes.len() {
                                    match bytes[index] {
                                        b'\\' => index = (index + 2).min(bytes.len()),
                                        b'"' => {
                                            index += 1;
                                            break;
                                        }
                                        _ => index += 1,
                                    }
                                }
                            }
                            b'(' => {
                                depth += 1;
                                index += 1;
                            }
                            b')' => {
                                depth = depth.saturating_sub(1);
                                index += 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => index += 1,
                        }
                    }
                }
                for byte in &mut output[start..index] {
                    if !matches!(*byte, b'\n' | b'\r') {
                        *byte = b' ';
                    }
                }
            }
            _ => index += 1,
        }
    }
    blank_unused_idempotency_key_definition(&mut output);
    // Every replaced byte is ASCII and untouched spans retain their original
    // UTF-8, so this conversion cannot fail.
    String::from_utf8(output).expect("directive stripping should preserve UTF-8")
}

/// Shopify accepts a bare `origin` selection on store-credit transactions even
/// though introspection exposes `StoreCreditAccountTransactionOrigin` as a
/// union. Captured responses currently return `null` for that selection. Keep
/// the executable schema honest for ordinary union selections, but add the
/// smallest valid selection to the engine-only document for this observed
/// Shopify exception. Domain handlers and response cleanup still use the
/// caller's original document, so the synthetic `__typename` never leaks.
fn expand_bare_store_credit_origin_selections(query: &str) -> String {
    if !(query.contains("storeCreditAccountCredit")
        || query.contains("storeCreditAccountDebit")
        || query.contains("StoreCreditAccountTransaction")
        || query.contains("StoreCreditAccountCreditTransaction")
        || query.contains("StoreCreditAccountDebitTransaction"))
    {
        return query.to_string();
    }

    let bytes = query.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'#' => {
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            b'"' if bytes.get(index..index + 3) == Some(b"\"\"\"") => {
                let start = index;
                index += 3;
                while index < bytes.len() {
                    if bytes.get(index..index + 3) == Some(b"\"\"\"") {
                        index += 3;
                        break;
                    }
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
                if &bytes[start..index] != b"origin" {
                    continue;
                }
                let mut next = index;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes.get(next).is_some_and(|next| {
                    matches!(*next, b'}' | b',' | b'.')
                        || next.is_ascii_alphabetic()
                        || *next == b'_'
                }) {
                    output.extend_from_slice(b" { __typename }");
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).expect("store-credit query expansion should preserve UTF-8")
}

fn blank_unused_idempotency_key_definition(output: &mut [u8]) {
    const VARIABLE: &[u8] = b"$idempotencyKey";
    let positions = output
        .windows(VARIABLE.len())
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == VARIABLE).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != 1 {
        return;
    }
    let start = positions[0];
    let mut end = start + VARIABLE.len();
    while end < output.len() && !matches!(output[end], b',' | b')') {
        end += 1;
    }
    for byte in &mut output[start..end] {
        if !matches!(*byte, b'\n' | b'\r') {
            *byte = b' ';
        }
    }
}

fn single_root_operation_query(
    operation_type: OperationType,
    field: &RootFieldSelection,
    variable_definitions: &BTreeMap<String, VariableDefinitionInfo>,
) -> String {
    let variable_definitions = serialize_used_variable_definitions(field, variable_definitions);
    format!(
        "{}{} {{ {} }}",
        operation_type.keyword(),
        variable_definitions,
        serialize_root_field(field)
    )
}

fn serialize_used_variable_definitions(
    field: &RootFieldSelection,
    variable_definitions: &BTreeMap<String, VariableDefinitionInfo>,
) -> String {
    let mut used_variables = std::collections::BTreeSet::new();
    for value in field.raw_arguments.values() {
        collect_raw_argument_variables(value, &mut used_variables);
    }
    for directive in &field.raw_directives {
        for value in directive.raw_arguments.values() {
            collect_raw_argument_variables(value, &mut used_variables);
        }
    }
    if used_variables.is_empty() {
        return String::new();
    }

    let definitions = used_variables
        .iter()
        .filter_map(|name| {
            variable_definitions
                .get(name.as_str())
                .map(|definition| format!("${}: {}", definition.name, definition.type_display))
        })
        .collect::<Vec<_>>();
    if definitions.is_empty() {
        String::new()
    } else {
        format!("({})", definitions.join(", "))
    }
}

fn collect_raw_argument_variables(
    value: &RawArgumentValue,
    variables: &mut std::collections::BTreeSet<String>,
) {
    match value {
        RawArgumentValue::List(values) => {
            for value in values {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Object(fields) => {
            for value in fields.values() {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Variable { name, .. } => {
            variables.insert(name.clone());
        }
        RawArgumentValue::String(_)
        | RawArgumentValue::Int(_)
        | RawArgumentValue::Float(_)
        | RawArgumentValue::Bool(_)
        | RawArgumentValue::Null
        | RawArgumentValue::Enum(_) => {}
    }
}

fn serialize_root_field(field: &RootFieldSelection) -> String {
    let mut output = String::new();
    if field.response_key != field.name {
        output.push_str(&field.response_key);
        output.push_str(": ");
    }
    output.push_str(&field.name);
    output.push_str(&serialize_raw_arguments(&field.raw_arguments));
    if field.raw_directives.is_empty() {
        for directive in &field.directives {
            output.push_str(" @");
            output.push_str(directive);
        }
    } else {
        for directive in &field.raw_directives {
            output.push_str(&serialize_raw_directive(directive));
        }
    }
    output.push_str(&serialize_selection_set(&field.selection));
    output
}

fn serialize_raw_directive(directive: &DirectiveSelection) -> String {
    format!(
        " @{}{}",
        directive.name,
        serialize_raw_arguments(&directive.raw_arguments)
    )
}

fn serialize_selected_field(field: &SelectedField) -> String {
    let mut output = String::new();
    if field.response_key != field.name {
        output.push_str(&field.response_key);
        output.push_str(": ");
    }
    output.push_str(&field.name);
    output.push_str(&serialize_resolved_arguments(&field.arguments));
    output.push_str(&serialize_selection_set(&field.selection));

    match field.type_condition.as_deref() {
        Some(type_condition) => format!("... on {type_condition} {{ {output} }}"),
        None => output,
    }
}

fn serialize_selection_set(selection: &[SelectedField]) -> String {
    if selection.is_empty() {
        return String::new();
    }
    format!(
        " {{ {} }}",
        selection
            .iter()
            .map(serialize_selected_field)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn serialize_raw_arguments(arguments: &BTreeMap<String, RawArgumentValue>) -> String {
    if arguments.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        arguments
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_raw_argument_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_resolved_arguments(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    if arguments.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        arguments
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_resolved_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_raw_argument_value(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => quote_graphql_string(value),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(serialize_raw_argument_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Object(fields) => serialize_raw_object(fields),
        RawArgumentValue::Variable { name, .. } => format!("${name}"),
    }
}

fn serialize_raw_object(fields: &BTreeMap<String, RawArgumentValue>) -> String {
    format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_raw_argument_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_resolved_value(value: &ResolvedValue) -> String {
    match value {
        ResolvedValue::String(value) => quote_graphql_string(value),
        ResolvedValue::Int(value) => value.to_string(),
        ResolvedValue::Float(value) => value.to_string(),
        ResolvedValue::Bool(value) => value.to_string(),
        ResolvedValue::Null => "null".to_string(),
        ResolvedValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(serialize_resolved_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ResolvedValue::Object(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", serialize_resolved_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn quote_graphql_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

pub(in crate::proxy) fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    if let Some(region) = backup_region {
        if region.get("id").and_then(Value::as_str) == Some(id) {
            return Some(selected_json(region, selection));
        }
    }
    None
}

fn is_safe_no_data_node_gid(id: &str) -> bool {
    [
        "gid://shopify/CashTrackingSession/",
        "gid://shopify/PointOfSaleDevice/",
        "gid://shopify/ShopifyPaymentsDispute/",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
}

fn finance_risk_no_data_read_data(fields: &[RootFieldSelection]) -> Value {
    root_payload_json(fields, |field| {
        Some(match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_empty_connection_json(&field.selection)
            }
            _ => Value::Null,
        })
    })
}

#[cfg(test)]
mod graphql_compatibility_tests {
    use super::expand_bare_store_credit_origin_selections;

    #[test]
    fn expands_only_bare_store_credit_origin_fields_for_engine_validation() {
        let query = r#"
            mutation StoreCredit {
              storeCreditAccountCredit(id: "gid://shopify/Customer/1", creditInput: { creditAmount: { amount: "1", currencyCode: USD } }) {
                storeCreditAccountTransaction {
                  origin
                  account { id }
                }
              }
            }
        "#;
        let expanded = expand_bare_store_credit_origin_selections(query);
        assert!(expanded.contains("origin { __typename }"));
        assert!(expanded.contains("account { id }"));

        let selected = query.replace("origin\n", "origin { __typename }\n");
        assert_eq!(
            expand_bare_store_credit_origin_selections(&selected),
            selected
        );

        let node_query =
            "query { nodes(ids: []) { ... on StoreCreditAccountCreditTransaction { origin } } }";
        assert!(expand_bare_store_credit_origin_selections(node_query)
            .contains("origin { __typename }"));
    }

    #[test]
    fn does_not_rewrite_origin_inside_inputs_strings_or_other_operations() {
        let store_credit = r#"
            mutation StoreCredit($input: ExampleInput = { origin: "origin" }) {
              storeCreditAccountDebit(id: "gid://shopify/StoreCreditAccount/1", debitInput: { debitAmount: { amount: "1", currencyCode: USD } }) {
                userErrors { message }
              }
            }
        "#;
        assert_eq!(
            expand_bare_store_credit_origin_selections(store_credit),
            store_credit
        );
        let unrelated = "query Inventory { inventoryTransfers(first: 1) { nodes { origin } } }";
        assert_eq!(
            expand_bare_store_credit_origin_selections(unrelated),
            unrelated
        );
    }
}

use super::*;
use crate::storefront_graphql::{self, StorefrontApiVersion};

fn format_runtime_timestamp(timestamp: time::OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RustStateDumpV2 {
    schema: String,
    created_at: String,
    /// Consumer-readable inspection view. Restore reads only `runtime_state`,
    /// which is the exhaustive structural representation.
    state: Value,
    runtime_state: PersistedRuntimeState,
    log: PersistedLog,
    next_synthetic_id: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRuntimeState {
    /// `Store` derives serde directly, so adding a store field cannot silently
    /// omit it from dump/restore the way the legacy hand-written mirrors did.
    store: Store,
    shop_sells_subscriptions: Option<bool>,
    product_catalog_base_records: BTreeMap<String, ProductRecord>,
    last_mutation_timestamp: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PersistedLog {
    entries: Vec<Value>,
}

#[cfg(test)]
pub(in crate::proxy) fn guarded_upstream_transport(
    transport: impl Fn(Request) -> Response + Send + Sync + 'static,
) -> UpstreamTransport {
    guarded_upstream_transport_from_arc(Arc::new(transport))
}

pub(in crate::proxy) fn guarded_upstream_transport_from_arc(
    transport: UpstreamTransport,
) -> UpstreamTransport {
    Arc::new(move |request| {
        if let Some(root_field) = registered_stage_locally_mutation_upstream_root(&request) {
            return json_error(
                400,
                &format!(
                    "Registered stage-locally mutation '{root_field}' cannot be forwarded upstream before POST /__meta/commit"
                ),
            );
        }
        transport(request)
    })
}

fn registered_stage_locally_mutation_upstream_root(request: &Request) -> Option<String> {
    let graphql_request = parse_graphql_request_body(&request.body)?;
    let operation = parse_operation_with_variables_and_operation_name(
        &graphql_request.query,
        &graphql_request.variables,
        graphql_request.operation_name.as_deref(),
    )
    .ok()?;
    if operation.operation_type != OperationType::Mutation {
        return None;
    }
    let api_surface = if storefront_graphql_version(&request.path).is_some() {
        ApiSurface::Storefront
    } else {
        ApiSurface::Admin
    };
    let registry = upstream_guard_registry();
    operation.root_fields.iter().find_map(|root_field| {
        let capability = operation_capability_for_surface(
            registry,
            api_surface,
            OperationType::Mutation,
            Some(root_field),
        );
        (capability.execution == CapabilityExecution::StageLocally
            && capability.domain != CapabilityDomain::Unknown)
            .then(|| root_field.clone())
    })
}

fn upstream_guard_registry() -> &'static [OperationRegistryEntry] {
    static REGISTRY: std::sync::OnceLock<Vec<OperationRegistryEntry>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(default_registry).as_slice()
}

impl DraftProxy {
    pub fn new(config: Config) -> Self {
        let upstream_transport: UpstreamTransport = Arc::new(default_upstream_transport);
        Self {
            config,
            log_entries: Vec::new(),
            registry: ResolverRegistry::new(default_registry()),
            store: Store::with_default_baseline(),
            shop_sells_subscriptions: None,
            product_catalog_base_records: BTreeMap::new(),
            clock: Arc::new(default_runtime_clock),
            last_mutation_timestamp: None,
            execution_session: ExecutionSession::default(),
            commit_transport: Arc::new(default_commit_transport),
            upstream_transport: guarded_upstream_transport_from_arc(Arc::clone(
                &upstream_transport,
            )),
            storefront_upstream_transport: guarded_upstream_transport_from_arc(upstream_transport),
        }
    }

    pub fn with_registry(mut self, registry: Vec<OperationRegistryEntry>) -> Self {
        self.registry = ResolverRegistry::new(registry);
        self
    }

    pub fn with_base_products(mut self, products: Vec<ProductRecord>) -> Self {
        self.store.replace_base_products(products);
        self
    }

    pub fn with_commit_transport(
        mut self,
        transport: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.commit_transport = Arc::new(transport);
        self
    }

    pub fn with_upstream_transport(
        mut self,
        transport: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        let transport: UpstreamTransport = Arc::new(transport);
        self.upstream_transport = guarded_upstream_transport_from_arc(Arc::clone(&transport));
        self.storefront_upstream_transport = guarded_upstream_transport_from_arc(transport);
        self
    }

    pub fn with_clock(
        mut self,
        clock: impl Fn() -> time::OffsetDateTime + Send + Sync + 'static,
    ) -> Self {
        self.clock = Arc::new(clock);
        self.last_mutation_timestamp = None;
        self
    }

    pub(in crate::proxy) fn current_time(&self) -> time::OffsetDateTime {
        (self.clock)()
    }

    pub(in crate::proxy) fn current_epoch_seconds(&self) -> i64 {
        self.current_time().unix_timestamp()
    }

    pub(in crate::proxy) fn mutation_log_ordinal(&self) -> usize {
        self.execution_session
            .mutation_log_start
            .unwrap_or(self.log_entries.len())
    }

    pub(in crate::proxy) fn next_mutation_timestamp(&mut self) -> String {
        let mut timestamp = self.current_time();
        if let Some(previous) = self.last_mutation_timestamp {
            if timestamp <= previous {
                timestamp = previous + time::Duration::nanoseconds(1);
            }
        }
        self.last_mutation_timestamp = Some(timestamp);
        format_runtime_timestamp(timestamp)
    }

    pub(in crate::proxy) fn upstream_post(&self, request: &Request, body: Value) -> Response {
        self.store.invalidate_synthetic_identity_cache();
        (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: body.to_string(),
        })
    }

    pub fn process_request(&mut self, request: Request) -> Response {
        let log_start = self.log_entries.len();
        let mut response = self.dispatch_route(request);
        // Successful local mutations can introduce authoritative relationship
        // IDs through variables or inline GraphQL literals. Their log entries
        // retain the raw request, so fold only the new entries into the
        // broker's reservation set without rescanning the complete Store at
        // the start of every request. Missing IDs used only by reads are not
        // identities and therefore do not reserve allocator slots. Upstream
        // hydration invalidates the cache in `upstream_post`, so the next
        // allocation still refreshes from all newly observed state.
        if let Some(new_entries) = self.log_entries.get(log_start..) {
            self.store.observe_shopify_gid_identities(new_entries);
        }
        // Stamp a cheap "has persistable state changed?" signal on every
        // response so embedders (e.g. the Ruby storage adapter) can decide
        // whether to persist without diffing or re-dumping the whole state on
        // reads. The tuple advances on any staged mutation (`log_entries` grows),
        // on commit (staged entries become `settled`), on reset (all reset to
        // `0:0:1`), and on restore (fields adopt the dumped values).
        response
            .headers
            .insert("x-sdp-state-version".to_string(), self.state_version());
        response
    }

    /// Opaque monotonic-ish token that changes iff persistable proxy state
    /// changed. Not an ordering guarantee — only equality is meaningful.
    pub(in crate::proxy) fn state_version(&self) -> String {
        let settled = self
            .log_entries
            .iter()
            .filter(|entry| entry.get("status") != Some(&json!("staged")))
            .count();
        format!(
            "{}:{}:{}",
            self.log_entries.len(),
            settled,
            self.store.synthetic_id_sequence()
        )
    }

    fn dispatch_route(&mut self, request: Request) -> Response {
        match route(&request) {
            Route::Health => ok_json(json!({
                "ok": true,
                "message": "shopify-draft-proxy is running"
            })),
            Route::MetaConfig => ok_json(self.config_snapshot()),
            Route::MetaLog => ok_json(json!({ "entries": self.log_entries })),
            Route::MetaState => ok_json(self.state_snapshot()),
            Route::MetaReset => {
                self.log_entries.clear();
                self.store.clear_staged();
                self.store.reset_synthetic_id_sequence();
                self.shop_sells_subscriptions = None;
                self.product_catalog_base_records.clear();
                self.last_mutation_timestamp = None;
                self.execution_session = ExecutionSession::default();
                ok_json(json!({ "ok": true, "message": "state reset" }))
            }
            Route::MetaDump => self.dump_state(&request),
            Route::MetaRestore => self.restore_state(&request),
            Route::MetaCommit => self.commit_staged_mutations(&request),
            Route::BulkOperationResult { artifact_id } => {
                self.bulk_operation_result_jsonl(&artifact_id)
            }
            Route::Graphql => self.execute_graphql(&request),
            Route::StorefrontGraphql => self.execute_storefront_graphql(&request),
            Route::NotFound => json_error(404, "Not found"),
            Route::MethodNotAllowed => json_error(405, "Method not allowed"),
        }
    }

    pub(in crate::proxy) fn record_storefront_log_entry(
        &mut self,
        request: &Request,
        status: &str,
        execution: &str,
        notes: &str,
    ) {
        let parsed_body = parse_graphql_request_body(&request.body);
        let parsed_operation = parsed_body
            .as_ref()
            .and_then(|body| parse_operation(&body.query));
        let id = format!("log-{}", self.log_entries.len() + 1);
        let root_fields = parsed_operation
            .as_ref()
            .map(|operation| operation.root_fields.clone())
            .unwrap_or_default();
        let primary_root_field = root_fields.first().cloned().unwrap_or_default();
        let operation_type = parsed_operation
            .as_ref()
            .map(|operation| operation.operation_type.keyword())
            .unwrap_or("unknown");
        let cart_sensitive = root_fields
            .iter()
            .any(|root| storefront_cart::storefront_cart_root_is_sensitive(root));
        let raw_query = parsed_body
            .as_ref()
            .map(|body| body.query.clone())
            .unwrap_or_default();
        let raw_variables = parsed_body
            .as_ref()
            .map(|body| resolved_variables_json(&body.variables))
            .unwrap_or_else(|| json!({}));
        let variables = if cart_sensitive {
            json!({ "redacted": true })
        } else {
            super::storefront::storefront_redact_sensitive_json(raw_variables.clone(), None)
        };
        let contains_sensitive_context = cart_sensitive
            || variables != raw_variables
            || raw_query.contains("customerAccessToken")
            || raw_query.contains("multipassToken")
            || raw_query.contains("resetToken")
            || raw_query.contains("activationToken");
        let query = if cart_sensitive {
            json!("<redacted:storefront-cart-query>")
        } else if contains_sensitive_context {
            json!("<redacted:storefront-sensitive-query>")
        } else if raw_query.is_empty() {
            Value::Null
        } else {
            json!(raw_query)
        };
        let raw_body = if cart_sensitive {
            json!("<redacted:storefront-cart-request>")
        } else if contains_sensitive_context {
            json!("<redacted:storefront-sensitive-request>")
        } else {
            json!(request.body)
        };
        self.log_entries.push(json!({
            "id": id,
            "operationName": Value::Null,
            "apiSurface": "storefront",
            "status": status,
            "path": request.path,
            "query": query,
            "variables": variables,
            "rawBody": raw_body,
            "interpreted": {
                "operationType": operation_type,
                "rootFields": root_fields,
                "primaryRootField": primary_root_field,
                "capability": {
                    "domain": "storefront",
                    "execution": execution
                }
            },
            "notes": notes
        }));
    }

    pub(in crate::proxy) fn storefront_snapshot_graphql_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        api_version: Option<StorefrontApiVersion>,
    ) -> Response {
        let Some(operation) = parse_operation_with_variables(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if operation.operation_type != OperationType::Query {
            return json_error(
                501,
                "Storefront API mutations are not locally implemented in snapshot mode",
            );
        }

        let fields = root_fields(query, variables).unwrap_or_default();
        let mut data = serde_json::Map::new();
        for field in fields {
            data.insert(
                field.response_key.clone(),
                self.storefront_snapshot_root_value(&field, api_version),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn storefront_snapshot_root_value(
        &self,
        field: &RootFieldSelection,
        api_version: Option<StorefrontApiVersion>,
    ) -> Value {
        let named_type = api_version
            .and_then(|version| {
                storefront_graphql::root_field_named_type(
                    version,
                    OperationType::Query,
                    &field.name,
                )
            })
            .unwrap_or_default();
        if named_type.ends_with("Connection") {
            connection_json(Vec::new())
        } else if matches!(field.name.as_str(), "nodes" | "publicApiVersions")
            || (field.name.ends_with('s') && field.name != "shop")
        {
            Value::Array(Vec::new())
        } else {
            Value::Null
        }
    }

    pub(in crate::proxy) fn config_snapshot(&self) -> Value {
        let unsupported_mode = self
            .config
            .unsupported_mutation_mode
            .clone()
            .unwrap_or(UnsupportedMutationMode::Passthrough);
        let max_size = self
            .config
            .bulk_operation_run_mutation_max_input_file_size_bytes
            .unwrap_or(DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES);

        json!({
            "runtime": {
                "readMode": self.config.read_mode.as_json_str(),
                "unsupportedMutationMode": unsupported_mode.as_json_str(),
                "bulkOperationRunMutationMaxInputFileSizeBytes": max_size
            },
            "proxy": {
                "port": self.config.port,
                "shopifyAdminOrigin": self.config.shopify_admin_origin
            },
            "snapshot": {
                "enabled": self.config.snapshot_path.is_some(),
                "path": self.config.snapshot_path
            }
        })
    }

    pub(in crate::proxy) fn state_snapshot(&self) -> Value {
        let available_locales = self
            .store
            .base
            .available_locales
            .iter()
            .map(|(locale, name)| (locale.clone(), json!(name)))
            .collect::<serde_json::Map<_, _>>();
        let deleted_shipping_package_ids = self
            .store
            .staged
            .shipping_packages
            .tombstones
            .iter()
            .map(|id| (id.clone(), json!(true)))
            .collect::<serde_json::Map<_, _>>();
        let deleted_owner_metafields = self
            .store
            .staged
            .deleted_owner_metafields
            .iter()
            .map(|(owner_id, namespace, key)| {
                json!({
                    "ownerId": owner_id,
                    "namespace": namespace,
                    "key": key
                })
            })
            .collect::<Vec<_>>();
        let base_metafield_definitions = self
            .store
            .base
            .metafield_definitions
            .iter()
            .map(|((owner_type, namespace, key), definition)| {
                (
                    format!("{owner_type}\u{1f}{namespace}\u{1f}{key}"),
                    definition.clone(),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let base_metafield_definition_observed_identities = self
            .store
            .base
            .metafield_definition_observed_identities
            .iter()
            .map(|(owner_type, namespace, key)| {
                json!({
                    "ownerType": owner_type,
                    "namespace": namespace,
                    "key": key
                })
            })
            .collect::<Vec<_>>();
        let base_metafield_definition_observed_ids = self
            .store
            .base
            .metafield_definition_observed_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let deleted_metafield_definitions = self
            .store
            .staged
            .deleted_metafield_definitions
            .iter()
            .map(|(owner_type, namespace, key)| {
                json!({
                    "ownerType": owner_type,
                    "namespace": namespace,
                    "key": key
                })
            })
            .collect::<Vec<_>>();
        let base_metafield_definitions_value = Value::Object(base_metafield_definitions);
        let base_metafield_definition_observed_identities_value =
            json!(base_metafield_definition_observed_identities);
        let base_metafield_definition_observed_ids_value =
            json!(base_metafield_definition_observed_ids);
        let deleted_metafield_definitions_value = json!(deleted_metafield_definitions);
        let base_state = json!({
                "products": product_state_map_json(&self.store.products.base.records),
                "productOrder": self.store.products.base.order,
                "productVariants": product_variant_state_map_json(&self.store.product_variants.base.records),
                "productVariantOrder": self.store.product_variants.base.order,
                "savedSearches": saved_search_state_map_json(&self.store.saved_searches.base.records),
                "savedSearchOrder": self.store.saved_searches.base.order,
                "shopPolicies": shop_policy_state_map_json(&self.store.shop_policies.base.records),
                "shopPolicyOrder": self.store.shop_policies.base.order,
                "deliveryProfiles": self.store.base.delivery_profiles.records.clone(),
                "deliveryProfileOrder": self.store.base.delivery_profiles.order,
                "deliveryPromiseProviders": self.store.base.delivery_promise_providers.records.clone(),
                "deliveryPromiseProviderOrder": self.store.base.delivery_promise_providers.order,
                "deliveryPromiseProviderCompleteLocationIds": self.store.base.delivery_promise_provider_complete_location_ids.iter().cloned().collect::<Vec<_>>(),
                "deliveryPromiseParticipants": self.store.base.delivery_promise_participants.records.clone(),
                "deliveryPromiseParticipantOrder": self.store.base.delivery_promise_participants.order,
                "deliveryPromiseParticipantBaselineOrders": self.store.base.delivery_promise_participant_baseline_orders.clone(),
                "deliveryPromiseParticipantCursorIds": self.store.base.delivery_promise_participant_cursor_ids.clone(),
                "deliveryPromiseParticipantCompleteScopes": self.store.base.delivery_promise_participant_complete_scopes.iter().cloned().collect::<Vec<_>>(),
                "deliveryPromiseParticipantNextCursors": self.store.base.delivery_promise_participant_next_cursors.clone(),
                "deliveryPromiseParticipantPreviousCursors": self.store.base.delivery_promise_participant_previous_cursors.clone(),
                "deliveryPromiseCompleteNodeIds": self.store.base.delivery_promise_complete_node_ids.iter().cloned().collect::<Vec<_>>(),
                "orders": self.store.base.orders.records.clone(),
                "orderOrder": self.store.base.orders.order,
                "returnPreconditionHydratedOrderIds": self.store.base.return_precondition_hydrated_order_ids.iter().cloned().collect::<Vec<_>>(),
                "orderCountBaselines": self.store.base.order_count_baselines.clone(),
                "returns": self.store.base.returns.clone(),
                "returnsByOrder": self.store.base.returns_by_order.clone(),
                "returnMissingIds": self.store.base.return_missing_ids.iter().cloned().collect::<Vec<_>>(),
                "reverseFulfillmentOrders": self.store.base.reverse_fulfillment_orders.clone(),
                "discounts": self.store.base.discounts.records.clone(),
                "discountOrder": self.store.base.discounts.order,
                "discountCountBaselines": self.store.base.discount_count_baselines.clone(),
                "segments": self.store.base.segments.records.clone(),
                "segmentOrder": self.store.base.segments.order,
                "segmentNameIds": self.store.base.segment_name_ids.clone(),
                "segmentCompleteNameProbes": self.store.base.segment_complete_name_probes.iter().cloned().collect::<Vec<_>>(),
                "segmentKnownMissingIds": self.store.base.segment_known_missing_ids.iter().cloned().collect::<Vec<_>>(),
                "segmentCountBaseline": self.store.base.segment_count_baseline.clone().unwrap_or(Value::Null),
                "segmentCatalogComplete": self.store.base.segment_catalog_complete,
                "customerSegmentMemberQueries": self.store.base.customer_segment_member_queries.clone(),
                "customerSegmentMemberQueryKnownMissingIds": self.store.base.customer_segment_member_query_known_missing_ids.iter().cloned().collect::<Vec<_>>(),
                "bulkOperations": self.store.base.bulk_operations.records.clone(),
                "bulkOperationOrder": self.store.base.bulk_operations.order.clone(),
                "bulkOperationsObserved": self.store.base.bulk_operations_observed,
                "locations": self.store.base.locations.records.clone(),
                "locationOrder": self.store.base.locations.order,
                "inventoryLevels": inventory_levels_json(&self.store.base.inventory_levels),
                "inventoryLevelIds": inventory_level_ids_json(&self.store.base.inventory_level_ids),
                "inventoryLevelOrder": inventory_level_order_json(&self.store.base.inventory_level_order),
                "inventoryLevelCursors": self.store.base.inventory_level_cursors.clone(),
                "inventoryItemCursors": self.store.base.inventory_item_cursors.clone(),
                "inventoryItemsCatalogHydrated": self.store.base.inventory_items_catalog_hydrated,
                "inactiveInventoryLevels": inactive_inventory_levels_json(&self.store.base.inactive_inventory_levels),
                "inventoryQuantityUpdatedAt": inventory_quantity_updated_at_json(&self.store.base.inventory_quantity_updated_at),
                "giftCards": self.store.base.gift_cards.clone(),
                "giftCardConfiguration": self.store.base.gift_card_configuration.clone().unwrap_or(Value::Null),
                "giftCardCompleteQueries": self.store.base.gift_card_complete_queries.iter().cloned().collect::<Vec<_>>(),
                "apps": self.store.base.apps.records.clone(),
                "appOrder": self.store.base.apps.order.clone(),
                "appInstallations": self.store.base.app_installations.records.clone(),
                "appInstallationOrder": self.store.base.app_installations.order.clone(),
                "currentAppIdsByRequestContext": self.store.base.current_app_ids_by_request_context.clone(),
                "backupRegionAccessScopesByRequestContext": self.store.base.backup_region_access_scopes_by_request_context.clone(),
                "shop": self.store.base.shop.clone(),
                "storefrontShop": self.store.base.storefront_shop.clone(),
                "storefrontLocalizations": self.store.base.storefront_localizations.clone(),
                "storefrontProductTags": self.store.base.storefront_product_tags.clone(),
                "storefrontProductTypes": self.store.base.storefront_product_types.clone(),
                "storefrontPaymentSettings": self.store.base.storefront_payment_settings.clone(),
                "storefrontLocations": self.store.base.storefront_locations.records.clone(),
                "storefrontLocationOrder": self.store.base.storefront_locations.order.clone(),
                "storefrontLocationCursors": self.store.base.storefront_location_cursors.clone(),
                "storefrontPublicApiVersions": self.store.base.storefront_public_api_versions.clone(),
                "storefrontMenus": self.store.base.storefront_menus.records.clone(),
                "storefrontMenuOrder": self.store.base.storefront_menus.order.clone(),
                "publicationIds": self.store.base.publication_ids.iter().cloned().collect::<Vec<_>>(),
                "publicationCount": self.store.base.publication_count,
                "availableLocales": available_locales,
                "shopLocales": self.store.base.shop_locales.clone(),
                "localizationProductIds": self.store.base.localization_product_ids.iter().cloned().collect::<Vec<_>>(),
                "localizationSourceResources": self.store.base.localization_source_resources.clone()
        });
        let staged_state = json!({
                "products": product_state_map_json(&self.store.products.staged.records),
                "productOrder": self.store.products.staged.order,
                "deletedProductIds": self.store.products.staged.tombstones.iter().cloned().collect::<Vec<_>>(),
                "productVariants": product_variant_state_map_json(&self.store.product_variants.staged.records),
                "productVariantOrder": self.store.product_variants.staged.order,
                "deletedProductVariantIds": self.store.product_variants.staged.tombstones.iter().cloned().collect::<Vec<_>>(),
                "productFeeds": self.store.staged.product_feeds.records.clone(),
                "productFeedOrder": self.store.staged.product_feeds.order,
                "deletedProductFeedIds": self.store.staged.product_feeds.tombstones.iter().cloned().collect::<Vec<_>>(),
                "collections": self.store.staged.collections.records.clone(),
                "deletedCollectionIds": self.store.staged.collections.tombstones.iter().cloned().collect::<Vec<_>>(),
                "deletedCollectionHandles": self.store.staged.deleted_collection_handles.iter().cloned().collect::<Vec<_>>(),
                "collectionJobs": self.store.staged.collection_jobs.clone(),
                "savedSearches": saved_search_state_map_json(&self.store.saved_searches.staged.records),
                "savedSearchOrder": self.store.saved_searches.staged.order,
                "deletedSavedSearchIds": self.store.saved_searches.staged.tombstones.iter().cloned().collect::<Vec<_>>(),
                "shopPolicies": shop_policy_state_map_json(&self.store.shop_policies.staged.records),
                "shopPolicyOrder": self.store.shop_policies.staged.order,
                "deletedShopPolicyIds": self.store.shop_policies.staged.tombstones.iter().cloned().collect::<Vec<_>>(),
                "shippingPackages": self.store.staged.shipping_packages.records.clone(),
                "deletedShippingPackageIds": deleted_shipping_package_ids,
                "installedApps": self.store.staged.installed_apps.clone(),
                "revokedAppAccessScopes": self.store.staged.revoked_app_access_scopes.iter().map(|(app_id, scopes)| {
                    (app_id.clone(), scopes.iter().cloned().collect::<Vec<_>>())
                }).collect::<BTreeMap<_, _>>(),
                "uninstalledAppIds": self.store.staged.uninstalled_app_ids.iter().cloned().collect::<Vec<_>>(),
                "delegatedAccessTokens": self.store.staged.delegate_access_tokens.clone(),
                "customers": self.store.staged.customers.records.clone(),
                "deletedCustomerIds": self.store.staged.customers.tombstones.iter().cloned().collect::<Vec<_>>(),
                "customerAddresses": self.store.staged.customer_addresses.clone(),
                "customerAddressOrder": self.store.staged.customer_address_order.clone(),
                "customerAddressOwners": self.store.staged.customer_address_owners.clone(),
                "customerOrders": self.store.staged.customer_orders.clone(),
                "mergedCustomerIds": self.store.staged.merged_customer_ids.clone(),
                "customerMergeRequests": self.store.staged.customer_merge_requests.clone(),
                "customerDataErasureRequests": self.store.staged.customer_data_erasure_requests.clone(),
                "locallyCreatedCustomerIds": self.store.staged.locally_created_customer_ids.iter().cloned().collect::<Vec<_>>(),
                "storefrontCustomerEmailIndex": self.store.staged.storefront_customer_email_index.clone(),
                "storefrontCustomerAccessTokens": self.store.staged.storefront_customer_access_tokens.clone(),
                "nextStorefrontCustomerAccessTokenId": self.store.staged.next_storefront_customer_access_token_id,
                "nextStorefrontCustomerResetTokenId": self.store.staged.next_storefront_customer_reset_token_id,
                "storefrontCarts": self.store.staged.storefront_carts.clone(),
                "storefrontCartOrder": self.store.staged.storefront_cart_order.clone(),
                "storefrontCartLines": self.store.staged.storefront_cart_lines.clone(),
                "storefrontCartLineOrder": self.store.staged.storefront_cart_line_order.clone(),
                "nextStorefrontCartId": self.store.staged.next_storefront_cart_id,
                "nextStorefrontCartLineId": self.store.staged.next_storefront_cart_line_id,
                "nextStorefrontCartAppliedGiftCardId": self.store.staged.next_storefront_cart_applied_gift_card_id,
                "nextStorefrontCartMetafieldId": self.store.staged.next_storefront_cart_metafield_id,
                "nextStorefrontCartDeliveryAddressId": self.store.staged.next_storefront_cart_delivery_address_id,
                "customersCountBase": self.store.staged.customers_count_base,
                "storeCreditAccounts": self.store.staged.store_credit_accounts.records.clone(),
                "storeCreditAccountOrder": self.store.staged.store_credit_accounts.order.clone(),
                "storeCreditTransactions": self.store.staged.store_credit_transactions.clone(),
                "storeCreditTransactionOrder": self.store.staged.store_credit_transaction_order.clone(),
                "giftCards": self.store.staged.gift_cards.clone(),
                "taggableResources": self.store.staged.taggable_resources.clone(),
                "abandonments": self.store.staged.abandonments.clone(),
                "orders": self.store.staged.orders.records.clone(),
                "deletedOrderIds": self.store.staged.orders.tombstones.iter().cloned().collect::<Vec<_>>(),
                "draftOrderTags": self.store.staged.draft_order_tags.clone(),
                "returns": self.store.staged.returns.clone(),
                "returnsByOrder": self.store.staged.returns_by_order.clone(),
                "reverseDeliveries": self.store.staged.reverse_deliveries.clone(),
                "reverseFulfillmentOrders": self.store.staged.reverse_fulfillment_orders.clone(),
                "reverseFulfillmentOrderLineItems": self.store.staged.reverse_fulfillment_order_line_items.clone(),
                "observedShippingLocations": self.store.staged.observed_shipping_locations.clone(),
                "observedShippingLocationOrder": self.store.staged.observed_shipping_location_order.clone(),
                "locations": self.store.staged.locations.records.clone(),
                "locationOrder": self.store.staged.locations.order.clone(),
                "deletedLocationIds": self.store.staged.locations.tombstones.iter().cloned().collect::<Vec<_>>(),
                "deliveryProfiles": self.store.staged.delivery_profiles.records.clone(),
                "deliveryProfileOrder": self.store.staged.delivery_profiles.order.clone(),
                "deletedDeliveryProfileIds": self.store.staged.delivery_profiles.tombstones.iter().cloned().collect::<Vec<_>>(),
                "deliveryPromiseProviders": self.store.staged.delivery_promise_providers.records.clone(),
                "deliveryPromiseProviderOrder": self.store.staged.delivery_promise_providers.order.clone(),
                "deletedDeliveryPromiseProviderIds": self.store.staged.delivery_promise_providers.tombstones.iter().cloned().collect::<Vec<_>>(),
                "deliveryPromiseParticipants": self.store.staged.delivery_promise_participants.records.clone(),
                "deliveryPromiseParticipantOrder": self.store.staged.delivery_promise_participants.order.clone(),
                "deletedDeliveryPromiseParticipantIds": self.store.staged.delivery_promise_participants.tombstones.iter().cloned().collect::<Vec<_>>(),
                "deliveryCustomizations": self.store.staged.delivery_customizations.records.clone(),
                "deliveryCustomizationOrder": self.store.staged.delivery_customizations.order.clone(),
                "deletedDeliveryCustomizationIds": self.store.staged.delivery_customizations.tombstones.iter().cloned().collect::<Vec<_>>(),
                "segments": self.store.staged.segments.records.clone(),
                "segmentOrder": self.store.staged.segments.order.clone(),
                "deletedSegmentIds": self.store.staged.segments.tombstones.iter().cloned().collect::<Vec<_>>(),
                "customerSegmentMemberQueries": self.store.staged.customer_segment_member_queries.clone(),
                "publicationIds": self.store.staged.publication_ids.iter().cloned().collect::<Vec<_>>(),
                "createdPublicationIds": self.store.staged.created_publication_ids.iter().cloned().collect::<Vec<_>>(),
                "publications": self.store.staged.publications.clone(),
                "currentChannelPublicationId": self.store.staged.current_channel_publication_id.clone(),
                "currentChannelPublicationResolved": self.store.staged.current_channel_publication_resolved,
                "resourcePublications": self.store.staged.resource_publications.iter().map(|(resource, pubs)| {
                    (resource.clone(), pubs.iter().cloned().collect::<Vec<String>>())
                }).collect::<std::collections::BTreeMap<String, Vec<String>>>(),
                "locationLimitReached": self.store.staged.location_limit_reached,
                "discounts": self.store.staged.discounts.records.clone(),
                "discountCodeIndex": self.store.staged.discount_code_index.clone(),
                "deletedDiscountIds": self.store.staged.discounts.tombstones.iter().cloned().collect::<Vec<_>>(),
                "discountRedeemCodeBulkCreations": self.store.staged.discount_redeem_code_bulk_creations.clone(),
                "ownerMetafields": self.store.staged.owner_metafields.clone(),
                "deletedOwnerMetafields": deleted_owner_metafields,
                "paymentTerms": self.store.staged.payment_terms.clone(),
                "paymentTermsOwnerIndex": self.store.staged.payment_terms_owner_index.clone(),
                "deletedPaymentTermsIds": self.store.staged.deleted_payment_terms_ids.iter().cloned().collect::<Vec<_>>(),
                "deletedPaymentScheduleIds": self.store.staged.deleted_payment_schedule_ids.iter().cloned().collect::<Vec<_>>()
        });
        let mut snapshot = json!({
            "baseState": base_state,
            "stagedState": staged_state
        });
        if self.store.staged.observed_shipping_locations_complete {
            snapshot["stagedState"]["observedShippingLocationsComplete"] = json!(true);
        }
        if let Some(cursor) = &self.store.staged.observed_shipping_locations_next_cursor {
            snapshot["stagedState"]["observedShippingLocationsNextCursor"] = json!(cursor);
        }
        snapshot["baseState"]["draftOrders"] = json!(self.store.base.draft_orders.records.clone());
        snapshot["baseState"]["draftOrderOrder"] = json!(self.store.base.draft_orders.order);
        snapshot["baseState"]["draftOrderCountBaselines"] =
            json!(self.store.base.draft_order_count_baselines.clone());
        snapshot["baseState"]["metafieldDefinitions"] = base_metafield_definitions_value;
        snapshot["baseState"]["metafieldDefinitionObservedIdentities"] =
            base_metafield_definition_observed_identities_value;
        snapshot["baseState"]["metafieldDefinitionObservedIds"] =
            base_metafield_definition_observed_ids_value;
        snapshot["baseState"]["metafieldDefinitionResourceScopes"] = json!(self
            .store
            .base
            .metafield_definition_resource_scopes
            .iter()
            .cloned()
            .collect::<Vec<_>>());
        snapshot["baseState"]["metafieldDefinitionPinnedOwnerScopes"] = json!(self
            .store
            .base
            .metafield_definition_pinned_owner_scopes
            .iter()
            .cloned()
            .collect::<Vec<_>>());
        snapshot["baseState"]["metafieldDefinitionWindows"] =
            json!(self.store.base.metafield_definition_windows.clone());
        snapshot["stagedState"]["deletedMetafieldDefinitions"] =
            deleted_metafield_definitions_value;
        if !self.store.base.product_operations.is_empty()
            || !self
                .store
                .base
                .product_operation_observed_field_paths
                .is_empty()
            || !self.store.base.missing_product_operation_ids.is_empty()
        {
            snapshot["baseState"]["productOperations"] =
                json!(self.store.base.product_operations.clone());
            snapshot["baseState"]["productOperationObservedFieldPaths"] = json!(self
                .store
                .base
                .product_operation_observed_field_paths
                .clone());
            snapshot["baseState"]["missingProductOperationIds"] = json!(self
                .store
                .base
                .missing_product_operation_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.base.b2b_companies.records.is_empty()
            || !self.store.base.b2b_companies.order.is_empty()
            || !self.store.base.b2b_company_count_baselines.is_empty()
        {
            snapshot["baseState"]["b2bCompanies"] =
                json!(self.store.base.b2b_companies.records.clone());
            snapshot["baseState"]["b2bCompanyOrder"] =
                json!(self.store.base.b2b_companies.order.clone());
            snapshot["baseState"]["b2bCompanyCountBaselines"] =
                json!(self.store.base.b2b_company_count_baselines.clone());
        }
        if !self.store.base.b2b_locations.records.is_empty()
            || !self.store.base.b2b_locations.order.is_empty()
        {
            snapshot["baseState"]["b2bLocations"] =
                json!(self.store.base.b2b_locations.records.clone());
            snapshot["baseState"]["b2bLocationOrder"] =
                json!(self.store.base.b2b_locations.order.clone());
        }
        if !self.store.base.b2b_contacts.records.is_empty()
            || !self.store.base.b2b_contacts.order.is_empty()
        {
            snapshot["baseState"]["b2bContacts"] =
                json!(self.store.base.b2b_contacts.records.clone());
            snapshot["baseState"]["b2bContactOrder"] =
                json!(self.store.base.b2b_contacts.order.clone());
        }
        if !self.store.base.b2b_contact_roles.records.is_empty()
            || !self.store.base.b2b_contact_roles.order.is_empty()
        {
            snapshot["baseState"]["b2bContactRoles"] =
                json!(self.store.base.b2b_contact_roles.records.clone());
            snapshot["baseState"]["b2bContactRoleOrder"] =
                json!(self.store.base.b2b_contact_roles.order.clone());
        }
        if !self.store.base.b2b_role_assignments.records.is_empty()
            || !self.store.base.b2b_role_assignments.order.is_empty()
        {
            snapshot["baseState"]["b2bRoleAssignments"] =
                json!(self.store.base.b2b_role_assignments.records.clone());
            snapshot["baseState"]["b2bRoleAssignmentOrder"] =
                json!(self.store.base.b2b_role_assignments.order.clone());
        }
        if !self.store.base.b2b_staff_assignments.records.is_empty()
            || !self.store.base.b2b_staff_assignments.order.is_empty()
        {
            snapshot["baseState"]["b2bStaffAssignments"] =
                json!(self.store.base.b2b_staff_assignments.records.clone());
            snapshot["baseState"]["b2bStaffAssignmentOrder"] =
                json!(self.store.base.b2b_staff_assignments.order.clone());
        }
        if !self.store.base.b2b_staff_member_ids.is_empty() {
            snapshot["baseState"]["b2bStaffMemberIds"] = json!(self
                .store
                .base
                .b2b_staff_member_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.base.b2b_customers.records.is_empty()
            || !self.store.base.b2b_customers.order.is_empty()
        {
            snapshot["baseState"]["b2bCustomers"] =
                json!(self.store.base.b2b_customers.records.clone());
            snapshot["baseState"]["b2bCustomerOrder"] =
                json!(self.store.base.b2b_customers.order.clone());
        }
        if !self.store.base.b2b_relationship_completeness.is_empty() {
            snapshot["baseState"]["b2bRelationshipCompleteness"] =
                json!(self.store.base.b2b_relationship_completeness.clone());
        }
        if !self.store.base.b2b_address_ids.is_empty() {
            snapshot["baseState"]["b2bAddressIds"] = json!(self
                .store
                .base
                .b2b_address_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.base.b2b_address_location_ids.is_empty() {
            snapshot["baseState"]["b2bAddressLocationIds"] =
                json!(self.store.base.b2b_address_location_ids.clone());
        }
        if !self.store.base.function_metadata.is_empty() {
            snapshot["baseState"]["functionMetadata"] =
                json!(self.store.base.function_metadata.clone());
            snapshot["baseState"]["functionMetadataOrder"] =
                json!(self.store.base.function_metadata_order.clone());
        }
        if !self.store.base.function_validations.is_empty() {
            snapshot["baseState"]["functionValidations"] =
                json!(self.store.base.function_validations.clone());
            snapshot["baseState"]["functionValidationOrder"] =
                json!(self.store.base.function_validation_order.clone());
        }
        if !self
            .store
            .base
            .function_validation_decision_records
            .is_empty()
        {
            snapshot["baseState"]["functionValidationDecisionRecords"] =
                json!(self.store.base.function_validation_decision_records.clone());
        }
        if let Some(cursor) = &self.store.base.function_validation_decision_next_cursor {
            snapshot["baseState"]["functionValidationDecisionNextCursor"] = json!(cursor);
        }
        if self
            .store
            .base
            .function_validation_decision_catalog_complete
        {
            snapshot["baseState"]["functionValidationDecisionCatalogComplete"] = json!(true);
        }
        if !self.store.base.function_cart_transforms.is_empty() {
            snapshot["baseState"]["functionCartTransforms"] =
                json!(self.store.base.function_cart_transforms.clone());
            snapshot["baseState"]["functionCartTransformOrder"] =
                json!(self.store.base.function_cart_transform_order.clone());
        }
        if let Some(decision) = &self.store.base.function_cart_transform_decision {
            snapshot["baseState"]["functionCartTransformDecision"] = decision.clone();
        }
        if self.store.base.function_cart_transform_decision_hydrated {
            snapshot["baseState"]["functionCartTransformDecisionHydrated"] = json!(true);
        }
        if !self
            .store
            .base
            .function_fulfillment_constraint_rules
            .is_empty()
        {
            snapshot["baseState"]["functionFulfillmentConstraintRules"] = json!(self
                .store
                .base
                .function_fulfillment_constraint_rules
                .clone());
            snapshot["baseState"]["functionFulfillmentConstraintRuleOrder"] = json!(self
                .store
                .base
                .function_fulfillment_constraint_rule_order
                .clone());
        }
        if self
            .store
            .base
            .function_fulfillment_constraint_rule_catalog_complete
        {
            snapshot["baseState"]["functionFulfillmentConstraintRuleCatalogComplete"] = json!(true);
        }
        if !self.store.base.function_connection_observations.is_empty() {
            snapshot["baseState"]["functionConnectionObservations"] =
                json!(self.store.base.function_connection_observations.clone());
        }
        if !self
            .store
            .base
            .function_fulfillment_constraint_rule_known_missing_ids
            .is_empty()
        {
            snapshot["baseState"]["functionFulfillmentConstraintRuleKnownMissingIds"] = json!(self
                .store
                .base
                .function_fulfillment_constraint_rule_known_missing_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.media_ready_on_read.is_empty() {
            snapshot["stagedState"]["mediaReadyOnReadIds"] = json!(self
                .store
                .staged
                .media_ready_on_read
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.product_operations.is_empty() {
            snapshot["stagedState"]["productOperations"] =
                json!(self.store.staged.product_operations);
        }
        if !self.store.staged.online_store_integrations.is_empty() {
            snapshot["stagedState"]["onlineStoreIntegrations"] =
                json!(self.store.staged.online_store_integrations.clone());
        }
        if !self.store.staged.online_store_blogs.is_empty() {
            snapshot["stagedState"]["onlineStoreBlogs"] =
                json!(self.store.staged.online_store_blogs.clone());
            snapshot["stagedState"]["onlineStoreBlogOrder"] =
                json!(self.store.staged.online_store_blog_order.clone());
        }
        if !self.store.staged.deleted_online_store_blog_ids.is_empty() {
            snapshot["stagedState"]["deletedOnlineStoreBlogIds"] = json!(self
                .store
                .staged
                .deleted_online_store_blog_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if let Some(count) = self.store.staged.online_store_blogs_count_base {
            snapshot["stagedState"]["onlineStoreBlogsCountBase"] = json!(count);
        }
        if !self
            .store
            .staged
            .observed_online_store_blog_handle_owners
            .is_empty()
        {
            snapshot["stagedState"]["observedOnlineStoreBlogHandleOwners"] = json!(self
                .store
                .staged
                .observed_online_store_blog_handle_owners
                .clone());
        }
        if !self.store.staged.online_store_pages.is_empty() {
            snapshot["stagedState"]["onlineStorePages"] =
                json!(self.store.staged.online_store_pages.clone());
            snapshot["stagedState"]["onlineStorePageOrder"] =
                json!(self.store.staged.online_store_page_order.clone());
        }
        if !self.store.staged.deleted_online_store_page_ids.is_empty() {
            snapshot["stagedState"]["deletedOnlineStorePageIds"] = json!(self
                .store
                .staged
                .deleted_online_store_page_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if let Some(count) = self.store.staged.online_store_pages_count_base {
            snapshot["stagedState"]["onlineStorePagesCountBase"] = json!(count);
        }
        if !self
            .store
            .staged
            .observed_online_store_page_handle_owners
            .is_empty()
        {
            snapshot["stagedState"]["observedOnlineStorePageHandleOwners"] = json!(self
                .store
                .staged
                .observed_online_store_page_handle_owners
                .clone());
        }
        if !self.store.staged.online_store_articles.is_empty() {
            snapshot["stagedState"]["onlineStoreArticles"] =
                json!(self.store.staged.online_store_articles.clone());
            snapshot["stagedState"]["onlineStoreArticleOrder"] =
                json!(self.store.staged.online_store_article_order.clone());
        }
        if !self
            .store
            .staged
            .deleted_online_store_article_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedOnlineStoreArticleIds"] = json!(self
                .store
                .staged
                .deleted_online_store_article_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self
            .store
            .staged
            .observed_online_store_article_handle_owners
            .is_empty()
        {
            snapshot["stagedState"]["observedOnlineStoreArticleHandleOwners"] = json!(self
                .store
                .staged
                .observed_online_store_article_handle_owners
                .clone());
        }
        if !self.store.staged.online_store_comments.is_empty() {
            snapshot["stagedState"]["onlineStoreComments"] =
                json!(self.store.staged.online_store_comments.clone());
            snapshot["stagedState"]["onlineStoreCommentOrder"] =
                json!(self.store.staged.online_store_comment_order.clone());
        }
        if !self
            .store
            .staged
            .deleted_online_store_comment_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedOnlineStoreCommentIds"] = json!(self
                .store
                .staged
                .deleted_online_store_comment_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.bulk_operations.is_empty() {
            snapshot["stagedState"]["bulkOperations"] =
                json!(self.store.staged.bulk_operations.records.clone());
            snapshot["stagedState"]["bulkOperationOrder"] =
                json!(self.store.staged.bulk_operations.order.clone());
        }
        if !self.store.staged.bulk_operation_staged_uploads.is_empty() {
            snapshot["stagedState"]["bulkOperationStagedUploads"] =
                json!(self.store.staged.bulk_operation_staged_uploads.clone());
        }
        if !self
            .store
            .staged
            .bulk_operation_staged_upload_bodies
            .is_empty()
        {
            snapshot["stagedState"]["bulkOperationStagedUploadBodies"] = json!(self
                .store
                .staged
                .bulk_operation_staged_upload_bodies
                .clone());
        }
        if !self.store.staged.bulk_operation_results.is_empty() {
            snapshot["stagedState"]["bulkOperationResults"] =
                json!(self.store.staged.bulk_operation_results.clone());
        }
        if !self.store.staged.customer_payment_methods.is_empty() {
            snapshot["stagedState"]["customerPaymentMethods"] =
                json!(self.store.staged.customer_payment_methods.clone());
            snapshot["stagedState"]["customerPaymentMethodCustomerIndex"] = json!(self
                .store
                .staged
                .customer_payment_method_customer_index
                .clone());
        }
        if !self.store.staged.payment_customizations.is_empty() {
            snapshot["stagedState"]["paymentCustomizations"] =
                json!(self.store.staged.payment_customizations.clone());
        }
        if !self
            .store
            .staged
            .deleted_payment_customization_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedPaymentCustomizationIds"] = json!(self
                .store
                .staged
                .deleted_payment_customization_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if self.store.staged.payment_customization_catalog_hydrated {
            snapshot["stagedState"]["paymentCustomizationCatalogHydrated"] = json!(true);
        }
        if !self.store.staged.order_customer_orders.is_empty() {
            snapshot["stagedState"]["orderCustomerOrders"] =
                json!(self.store.staged.order_customer_orders.clone());
        }
        if !self.store.staged.order_customer_cancelled_ids.is_empty() {
            snapshot["stagedState"]["orderCustomerCancelledIds"] = json!(self
                .store
                .staged
                .order_customer_cancelled_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.order_customer_b2b_order_ids.is_empty() {
            snapshot["stagedState"]["orderCustomerB2bOrderIds"] = json!(self
                .store
                .staged
                .order_customer_b2b_order_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self
            .store
            .staged
            .order_customer_contact_customer_ids
            .is_empty()
        {
            snapshot["stagedState"]["orderCustomerContactCustomerIds"] = json!(self
                .store
                .staged
                .order_customer_contact_customer_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if self.store.staged.next_order_number != 1 {
            snapshot["stagedState"]["nextOrderNumber"] = json!(self.store.staged.next_order_number);
        }
        if self.has_staged_b2b_state() {
            snapshot["stagedState"]["b2bCompanies"] =
                json!(self.store.staged.b2b_companies.clone());
            snapshot["stagedState"]["b2bLocations"] =
                json!(self.store.staged.b2b_locations.records.clone());
            snapshot["stagedState"]["b2bLocationOrder"] =
                json!(self.store.staged.b2b_locations.order.clone());
            snapshot["stagedState"]["b2bContacts"] = json!(self.store.staged.b2b_contacts.clone());
            snapshot["stagedState"]["b2bContactRoles"] =
                json!(self.store.staged.b2b_contact_roles.clone());
            snapshot["stagedState"]["b2bRoleAssignments"] =
                json!(self.store.staged.b2b_role_assignments.clone());
            snapshot["stagedState"]["b2bStaffAssignments"] =
                json!(self.store.staged.b2b_staff_assignments.clone());
            snapshot["stagedState"]["b2bAddressLocationIds"] =
                json!(self.store.staged.b2b_address_location_ids.clone());
            snapshot["stagedState"]["deletedB2bCompanyIds"] = json!(self
                .store
                .staged
                .deleted_b2b_company_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
            snapshot["stagedState"]["deletedB2bLocationIds"] = json!(self
                .store
                .staged
                .b2b_locations
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
            snapshot["stagedState"]["deletedB2bContactIds"] = json!(self
                .store
                .staged
                .deleted_b2b_contact_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
            snapshot["stagedState"]["deletedB2bContactRoleAssignmentIds"] = json!(self
                .store
                .staged
                .deleted_b2b_contact_role_assignment_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
            snapshot["stagedState"]["deletedB2bStaffAssignmentIds"] = json!(self
                .store
                .staged
                .deleted_b2b_staff_assignment_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
            snapshot["stagedState"]["deletedB2bAddressIds"] = json!(self
                .store
                .staged
                .deleted_b2b_address_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.inventory_levels.is_empty() {
            snapshot["stagedState"]["inventoryLevels"] =
                inventory_levels_json(&self.store.staged.inventory_levels);
        }
        if !self.store.staged.inventory_level_ids.is_empty() {
            snapshot["stagedState"]["inventoryLevelIds"] =
                inventory_level_ids_json(&self.store.staged.inventory_level_ids);
        }
        if !self.store.staged.inventory_level_order.is_empty() {
            snapshot["stagedState"]["inventoryLevelOrder"] =
                inventory_level_order_json(&self.store.staged.inventory_level_order);
        }
        if !self.store.staged.fulfillment_order_cursors.is_empty() {
            snapshot["stagedState"]["fulfillmentOrderCursors"] =
                serde_json::to_value(&self.store.staged.fulfillment_order_cursors)
                    .unwrap_or_default();
        }
        if !self.store.staged.inventory_level_cursors.is_empty() {
            snapshot["stagedState"]["inventoryLevelCursors"] =
                serde_json::to_value(&self.store.staged.inventory_level_cursors)
                    .unwrap_or_default();
        }
        if !self.store.staged.inactive_inventory_levels.is_empty() {
            snapshot["stagedState"]["inactiveInventoryLevels"] =
                inactive_inventory_levels_json(&self.store.staged.inactive_inventory_levels);
        }
        if !self.store.staged.active_inventory_levels.is_empty() {
            snapshot["stagedState"]["activeInventoryLevels"] =
                inactive_inventory_levels_json(&self.store.staged.active_inventory_levels);
        }
        if !self.store.base.inventory_transfers.records.is_empty() {
            snapshot["baseState"]["inventoryTransfers"] =
                serde_json::to_value(&self.store.base.inventory_transfers.records)
                    .unwrap_or_default();
            snapshot["baseState"]["inventoryTransferOrder"] =
                json!(self.store.base.inventory_transfers.order);
        }
        if !self.store.staged.inventory_transfers.is_empty() {
            snapshot["stagedState"]["inventoryTransfers"] =
                serde_json::to_value(&self.store.staged.inventory_transfers.records)
                    .unwrap_or_default();
            snapshot["stagedState"]["inventoryTransferOrder"] =
                json!(self.store.staged.inventory_transfers.order);
        }
        if !self.store.staged.inventory_transfers.tombstones.is_empty() {
            snapshot["stagedState"]["deletedInventoryTransferIds"] = json!(self
                .store
                .staged
                .inventory_transfers
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.base.inventory_shipments.records.is_empty() {
            snapshot["baseState"]["inventoryShipments"] =
                serde_json::to_value(&self.store.base.inventory_shipments.records)
                    .unwrap_or_default();
            snapshot["baseState"]["inventoryShipmentOrder"] =
                json!(self.store.base.inventory_shipments.order);
        }
        if !self.store.staged.inventory_shipments.is_empty() {
            snapshot["stagedState"]["inventoryShipments"] =
                serde_json::to_value(&self.store.staged.inventory_shipments.records)
                    .unwrap_or_default();
            snapshot["stagedState"]["inventoryShipmentOrder"] =
                json!(self.store.staged.inventory_shipments.order);
        }
        if !self.store.staged.inventory_shipments.tombstones.is_empty() {
            snapshot["stagedState"]["deletedInventoryShipmentIds"] = json!(self
                .store
                .staged
                .inventory_shipments
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.inventory_quantity_updated_at.is_empty() {
            snapshot["stagedState"]["inventoryQuantityUpdatedAt"] =
                inventory_quantity_updated_at_json(
                    &self.store.staged.inventory_quantity_updated_at,
                );
        }
        if self.store.staged.next_inventory_quantity_timestamp != 0 {
            snapshot["stagedState"]["nextInventoryQuantityTimestamp"] =
                json!(self.store.staged.next_inventory_quantity_timestamp);
        }
        if !self.store.staged.inventory_adjustment_groups.is_empty() {
            snapshot["stagedState"]["inventoryAdjustmentGroups"] =
                json!(self.store.staged.inventory_adjustment_groups);
        }
        if !self.store.staged.metaobject_definitions.records.is_empty() {
            snapshot["stagedState"]["metaobjectDefinitions"] =
                json!(self.store.staged.metaobject_definitions.records);
        }
        if !self
            .store
            .staged
            .metaobject_definitions
            .tombstones
            .is_empty()
        {
            snapshot["stagedState"]["deletedMetaobjectDefinitionIds"] = json!(self
                .store
                .staged
                .metaobject_definitions
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.metaobjects.records.is_empty() {
            snapshot["stagedState"]["metaobjects"] = json!(self.store.staged.metaobjects.records);
        }
        if !self.store.staged.metaobjects.tombstones.is_empty() {
            snapshot["stagedState"]["deletedMetaobjectIds"] = json!(self
                .store
                .staged
                .metaobjects
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.deleted_metaobject_types.is_empty() {
            snapshot["stagedState"]["deletedMetaobjectTypes"] = json!(self
                .store
                .staged
                .deleted_metaobject_types
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.url_redirects.is_empty() {
            snapshot["stagedState"]["urlRedirects"] =
                json!(self.store.staged.url_redirects.clone());
            snapshot["stagedState"]["urlRedirectOrder"] =
                json!(self.store.staged.url_redirect_order.clone());
        }
        // Linked product-option metaobject entry sets feed DISPLAY_NAME_CONFLICT
        // detection on metaobjectUpdate/Upsert. The runner restores mainState
        // before every downstream target, so the set staged by the
        // productOptionsCreate target must round-trip to reach the later
        // rename targets.
        if !self
            .store
            .staged
            .linked_product_option_metaobject_sets
            .is_empty()
        {
            snapshot["stagedState"]["linkedProductOptionMetaobjectSets"] = json!(self
                .store
                .staged
                .linked_product_option_metaobject_sets
                .iter()
                .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
                .collect::<Vec<_>>());
        }
        if !self.store.staged.flow_signatures.is_empty() {
            snapshot["stagedState"]["flowSignatures"] = json!(self.store.staged.flow_signatures);
        }
        if !self.store.staged.flow_trigger_receipts.is_empty() {
            snapshot["stagedState"]["flowTriggerReceipts"] =
                json!(self.store.staged.flow_trigger_receipts);
        }
        if !self.store.staged.metafield_definitions.is_empty() {
            snapshot["stagedState"]["metafieldDefinitions"] = Value::Object(
                self.store
                    .staged
                    .metafield_definitions
                    .iter()
                    .map(|((owner_type, namespace, key), definition)| {
                        (
                            format!("{owner_type}\u{1f}{namespace}\u{1f}{key}"),
                            definition.clone(),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>(),
            );
        }
        if !self.store.staged.metafield_reference_ids.is_empty() {
            snapshot["stagedState"]["metafieldReferenceIds"] = json!(self
                .store
                .staged
                .metafield_reference_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.draft_orders.is_empty() {
            snapshot["stagedState"]["draftOrders"] = Value::Object(
                self.store
                    .staged
                    .draft_orders
                    .iter()
                    .map(|(id, record)| {
                        (
                            id.clone(),
                            json!({ "id": id, "cursor": Value::Null, "data": record }),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>(),
            );
            snapshot["stagedState"]["draftOrderOrder"] =
                json!(self.store.staged.draft_orders.order.to_vec());
            snapshot["stagedState"]["deletedDraftOrderIds"] = json!(self
                .store
                .staged
                .draft_orders
                .tombstones
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        // Markets-domain staged maps. The parity runner restores mainState
        // before every downstream target, so these MUST round-trip or
        // read-after-write across targets (catalog delete, price-list
        // lifecycle, web-presence update, market localization, etc.) wipes
        // the record staged by the primary op. Emit conditionally (only when
        // non-empty) so specs asserting on the whole proxy state ($) don't see
        // spurious empty keys.
        if !self.store.staged.markets.is_empty() {
            snapshot["stagedState"]["markets"] = json!(self.store.staged.markets.clone());
        }
        if !self.store.staged.deleted_market_ids.is_empty() {
            snapshot["stagedState"]["deletedMarketIds"] = json!(self
                .store
                .staged
                .deleted_market_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.catalogs.is_empty() {
            snapshot["stagedState"]["catalogs"] = json!(self.store.staged.catalogs.clone());
        }
        if !self.store.staged.created_catalog_ids.is_empty() {
            snapshot["stagedState"]["createdCatalogIds"] = json!(self
                .store
                .staged
                .created_catalog_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.price_lists.is_empty() {
            snapshot["stagedState"]["priceLists"] = json!(self.store.staged.price_lists.clone());
        }
        if !self.store.staged.web_presences.is_empty() {
            snapshot["stagedState"]["webPresences"] =
                json!(self.store.staged.web_presences.clone());
        }
        if !self.store.staged.markets_upstream_counts.is_empty() {
            snapshot["stagedState"]["marketsUpstreamCounts"] =
                json!(self.store.staged.markets_upstream_counts.clone());
        }
        if !self.store.staged.available_backup_regions.is_empty() {
            snapshot["stagedState"]["availableBackupRegions"] =
                json!(self.store.staged.available_backup_regions.clone());
        }
        if !self.store.staged.shop_locales.is_empty() {
            snapshot["stagedState"]["stagedShopLocales"] =
                json!(self.store.staged.shop_locales.clone());
        }
        if !self.store.staged.localization_translations.is_empty() {
            snapshot["stagedState"]["localizationTranslations"] =
                json!(self.store.staged.localization_translations.clone());
        }
        if !self.store.staged.localization_source_resources.is_empty() {
            snapshot["stagedState"]["localizationSourceResources"] =
                json!(self.store.staged.localization_source_resources.clone());
        }
        if !self.store.staged.localization_resources.is_empty() {
            snapshot["stagedState"]["localizationResources"] =
                json!(self.store.staged.localization_resources.clone());
        }
        if self.store.staged.localization_dirty {
            snapshot["stagedState"]["localizationDirty"] = json!(true);
        }
        if !self.store.staged.function_metadata.is_empty() {
            snapshot["stagedState"]["functionMetadata"] =
                json!(self.store.staged.function_metadata.clone());
            snapshot["stagedState"]["functionMetadataOrder"] =
                json!(self.store.staged.function_metadata_order.clone());
        }
        if !self.store.staged.function_validations.is_empty() {
            snapshot["stagedState"]["functionValidations"] =
                json!(self.store.staged.function_validations.clone());
            snapshot["stagedState"]["functionValidationOrder"] =
                json!(self.store.staged.function_validation_order.clone());
        }
        if !self.store.staged.deleted_function_validation_ids.is_empty() {
            snapshot["stagedState"]["deletedFunctionValidationIds"] = json!(self
                .store
                .staged
                .deleted_function_validation_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if self.store.staged.function_validations_dirty {
            snapshot["stagedState"]["functionValidationsDirty"] = json!(true);
        }
        if !self.store.staged.function_cart_transforms.is_empty() {
            snapshot["stagedState"]["functionCartTransforms"] =
                json!(self.store.staged.function_cart_transforms.clone());
            snapshot["stagedState"]["functionCartTransformOrder"] =
                json!(self.store.staged.function_cart_transform_order.clone());
        }
        if !self
            .store
            .staged
            .deleted_function_cart_transform_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedFunctionCartTransformIds"] = json!(self
                .store
                .staged
                .deleted_function_cart_transform_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if self.store.staged.function_cart_transforms_dirty {
            snapshot["stagedState"]["functionCartTransformsDirty"] = json!(true);
        }
        if self.store.staged.functions_dirty {
            snapshot["stagedState"]["functionsDirty"] = json!(true);
        }
        if !self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .is_empty()
        {
            snapshot["stagedState"]["functionFulfillmentConstraintRules"] = json!(self
                .store
                .staged
                .function_fulfillment_constraint_rules
                .clone());
            snapshot["stagedState"]["functionFulfillmentConstraintRuleOrder"] = json!(self
                .store
                .staged
                .function_fulfillment_constraint_rule_order
                .clone());
        }
        if !self
            .store
            .staged
            .deleted_function_fulfillment_constraint_rule_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedFunctionFulfillmentConstraintRuleIds"] = json!(self
                .store
                .staged
                .deleted_function_fulfillment_constraint_rule_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if self
            .store
            .staged
            .function_fulfillment_constraint_rules_dirty
        {
            snapshot["stagedState"]["functionFulfillmentConstraintRulesDirty"] = json!(true);
        }
        if let Some(configuration) = &self.store.staged.tax_app_configuration {
            snapshot["stagedState"]["taxAppConfiguration"] = configuration.clone();
        }
        if let Some(order) = &self.store.staged.order_edit_existing_order {
            snapshot["stagedState"]["orderEditExistingOrder"] = order.clone();
        }
        if let Some(calculated_order) = &self.store.staged.order_edit_existing_calculated_order {
            snapshot["stagedState"]["orderEditExistingCalculatedOrder"] = calculated_order.clone();
        }
        if let Some(calculated_order_id) =
            &self.store.staged.order_edit_existing_calculated_order_id
        {
            snapshot["stagedState"]["orderEditExistingCalculatedOrderId"] =
                json!(calculated_order_id);
        }
        if let Some(session_order_id) = &self.store.staged.order_edit_existing_session_order_id {
            snapshot["stagedState"]["orderEditExistingSessionOrderId"] = json!(session_order_id);
        }
        if !self
            .store
            .staged
            .order_edit_money_bag_calculated_order_ids
            .is_empty()
        {
            snapshot["stagedState"]["orderEditMoneyBagCalculatedOrderIds"] = json!(self
                .store
                .staged
                .order_edit_money_bag_calculated_order_ids
                .clone());
        }
        if let Some(mode) = &self.store.staged.order_edit_existing_mode {
            snapshot["stagedState"]["orderEditExistingMode"] = json!(mode);
        }
        if self
            .store
            .staged
            .order_edit_variant_catalog
            .as_object()
            .is_some_and(|catalog| !catalog.is_empty())
        {
            snapshot["stagedState"]["orderEditVariantCatalog"] =
                self.store.staged.order_edit_variant_catalog.clone();
        }
        if let Some(author) = &self.store.staged.order_edit_author {
            snapshot["stagedState"]["orderEditAuthor"] = json!(author);
        }
        snapshot
    }

    fn has_staged_b2b_state(&self) -> bool {
        !self.store.staged.b2b_companies.is_empty()
            || !self.store.staged.deleted_b2b_company_ids.is_empty()
            || !self.store.staged.b2b_locations.is_empty()
            || !self.store.staged.b2b_contacts.is_empty()
            || !self.store.staged.deleted_b2b_contact_ids.is_empty()
            || !self.store.staged.b2b_contact_roles.is_empty()
            || !self.store.staged.b2b_role_assignments.is_empty()
            || !self
                .store
                .staged
                .deleted_b2b_contact_role_assignment_ids
                .is_empty()
            || !self.store.staged.b2b_staff_assignments.is_empty()
            || !self
                .store
                .staged
                .deleted_b2b_staff_assignment_ids
                .is_empty()
            || !self.store.staged.b2b_address_location_ids.is_empty()
            || !self.store.staged.deleted_b2b_address_ids.is_empty()
    }

    pub(in crate::proxy) fn dump_state(&self, request: &Request) -> Response {
        let created_at = serde_json::from_str::<Value>(&request.body)
            .ok()
            .and_then(|body| {
                body.get("createdAt")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string());
        let dump = RustStateDumpV2 {
            schema: RUST_STATE_DUMP_SCHEMA.to_string(),
            created_at,
            state: self.state_snapshot(),
            runtime_state: PersistedRuntimeState {
                store: self.store.clone(),
                shop_sells_subscriptions: self.shop_sells_subscriptions,
                product_catalog_base_records: self.product_catalog_base_records.clone(),
                last_mutation_timestamp: self.last_mutation_timestamp.map(format_runtime_timestamp),
            },
            log: PersistedLog {
                entries: self.log_entries.clone(),
            },
            next_synthetic_id: self.store.synthetic_id_sequence(),
        };
        ok_json(
            serde_json::to_value(dump)
                .expect("the structurally serializable Rust state dump should encode as JSON"),
        )
    }

    pub(in crate::proxy) fn restore_state(&mut self, request: &Request) -> Response {
        let Ok(dump) = serde_json::from_str::<Value>(&request.body) else {
            return json_error(400, "Invalid Rust state dump JSON");
        };
        match dump.get("schema").and_then(Value::as_str) {
            Some(RUST_STATE_DUMP_SCHEMA) => self.restore_v2_state(dump),
            _ => json_error(400, "Unsupported Rust state dump schema"),
        }
    }

    fn restore_v2_state(&mut self, dump: Value) -> Response {
        let Ok(dump) = serde_json::from_value::<RustStateDumpV2>(dump) else {
            return json_error(400, "Invalid Rust v2 state dump");
        };
        if dump.schema != RUST_STATE_DUMP_SCHEMA || !dump.state.is_object() {
            return json_error(400, "Invalid Rust v2 state dump");
        }
        let store_synthetic_id = dump.runtime_state.store.synthetic_id_sequence();
        if dump.next_synthetic_id == 0
            || store_synthetic_id == 0
            || dump.next_synthetic_id != store_synthetic_id
        {
            return json_error(400, "Invalid Rust synthetic identity");
        }
        let last_mutation_timestamp = match dump.runtime_state.last_mutation_timestamp.as_deref() {
            Some(timestamp) => match time::OffsetDateTime::parse(
                timestamp,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(timestamp) => Some(timestamp),
                Err(_) => return json_error(400, "Invalid Rust mutation timestamp"),
            },
            None => None,
        };

        self.store = dump.runtime_state.store;
        self.log_entries = dump.log.entries;
        self.shop_sells_subscriptions = dump.runtime_state.shop_sells_subscriptions;
        self.product_catalog_base_records = dump.runtime_state.product_catalog_base_records;
        self.last_mutation_timestamp = last_mutation_timestamp;
        self.execution_session = ExecutionSession::default();

        ok_json(json!({ "ok": true, "message": "state restored" }))
    }
}

fn inventory_levels_json(levels: &BTreeMap<(String, String), BTreeMap<String, i64>>) -> Value {
    json!(levels
        .iter()
        .map(|((inventory_item_id, location_id), quantities)| {
            json!({
                "inventoryItemId": inventory_item_id,
                "locationId": location_id,
                "quantities": quantities
            })
        })
        .collect::<Vec<_>>())
}

fn inventory_level_ids_json(ids: &BTreeMap<(String, String), String>) -> Value {
    json!(ids
        .iter()
        .map(|((inventory_item_id, location_id), id)| {
            json!({
                "inventoryItemId": inventory_item_id,
                "locationId": location_id,
                "id": id
            })
        })
        .collect::<Vec<_>>())
}

fn inventory_level_order_json(order: &[(String, String)]) -> Value {
    json!(order
        .iter()
        .map(|(inventory_item_id, location_id)| {
            json!({
                "inventoryItemId": inventory_item_id,
                "locationId": location_id
            })
        })
        .collect::<Vec<_>>())
}

fn inactive_inventory_levels_json(levels: &BTreeSet<(String, String)>) -> Value {
    json!(levels
        .iter()
        .map(|(inventory_item_id, location_id)| {
            json!({
                "inventoryItemId": inventory_item_id,
                "locationId": location_id
            })
        })
        .collect::<Vec<_>>())
}

fn inventory_quantity_updated_at_json(
    timestamps: &BTreeMap<(String, String, String), String>,
) -> Value {
    json!(timestamps
        .iter()
        .map(|((inventory_item_id, location_id, name), updated_at)| {
            json!({
                "inventoryItemId": inventory_item_id,
                "locationId": location_id,
                "name": name,
                "updatedAt": updated_at
            })
        })
        .collect::<Vec<_>>())
}

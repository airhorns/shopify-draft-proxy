use super::*;
use crate::storefront_graphql::{self, StorefrontApiVersion};

fn format_runtime_timestamp(timestamp: time::OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
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
            next_synthetic_id: 1,
            shop_sells_subscriptions: None,
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
        (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: body.to_string(),
        })
    }

    pub fn process_request(&mut self, request: Request) -> Response {
        let mut response = self.dispatch_route(request);
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
            self.next_synthetic_id
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
                self.next_synthetic_id = 1;
                self.shop_sells_subscriptions = None;
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
        let base_metafield_definition_namespaces = self
            .store
            .base
            .metafield_definition_namespaces
            .iter()
            .map(|(owner_type, namespace)| {
                json!({
                    "ownerType": owner_type,
                    "namespace": namespace
                })
            })
            .collect::<Vec<_>>();
        let base_metafield_definition_owner_catalogs = self
            .store
            .base
            .metafield_definition_owner_catalogs
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
        let base_metafield_definition_owner_catalogs_value =
            json!(base_metafield_definition_owner_catalogs);
        let base_metafield_definition_namespaces_value =
            json!(base_metafield_definition_namespaces);
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
                "orderCountBaselines": self.store.base.order_count_baselines.clone(),
                "discounts": self.store.base.discounts.records.clone(),
                "discountOrder": self.store.base.discounts.order,
                "discountCountBaselines": self.store.base.discount_count_baselines.clone(),
                "segments": self.store.base.segments.records.clone(),
                "segmentOrder": self.store.base.segments.order,
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
                "localizationProductIds": self.store.base.localization_product_ids.iter().cloned().collect::<Vec<_>>()
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
                "customersCountBase": self.store.staged.customer_count_baselines
                    .get(&customer_count_baseline_key(&BTreeMap::new()))
                    .and_then(|count| count.get("count"))
                    .and_then(Value::as_u64),
                "customerCountBaselines": self.store.staged.customer_count_baselines.clone(),
                "storeCreditAccounts": self.store.staged.store_credit_accounts.records.clone(),
                "storeCreditAccountOrder": self.store.staged.store_credit_accounts.order.clone(),
                "storeCreditTransactions": self.store.staged.store_credit_transactions.clone(),
                "storeCreditTransactionOrder": self.store.staged.store_credit_transaction_order.clone(),
                "nextStoreCreditAccountId": self.store.staged.next_store_credit_account_id,
                "nextStoreCreditTransactionId": self.store.staged.next_store_credit_transaction_id,
                "giftCards": self.store.staged.gift_cards.clone(),
                "taggableResources": self.store.staged.taggable_resources.clone(),
                "abandonments": self.store.staged.abandonments.clone(),
                "orders": self.store.staged.orders.records.clone(),
                "deletedOrderIds": self.store.staged.orders.tombstones.iter().cloned().collect::<Vec<_>>(),
                "nextDraftOrderId": self.store.staged.next_draft_order_id,
                "draftOrderTags": self.store.staged.draft_order_tags.clone(),
                "returns": self.store.staged.returns.clone(),
                "returnsByOrder": self.store.staged.returns_by_order.clone(),
                "reverseDeliveries": self.store.staged.reverse_deliveries.clone(),
                "reverseFulfillmentOrders": self.store.staged.reverse_fulfillment_orders.clone(),
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
                "deletedOwnerMetafields": deleted_owner_metafields
        });
        let mut snapshot = json!({
            "baseState": base_state,
            "stagedState": staged_state
        });
        snapshot["baseState"]["draftOrders"] = json!(self.store.base.draft_orders.records.clone());
        snapshot["baseState"]["draftOrderOrder"] = json!(self.store.base.draft_orders.order);
        snapshot["baseState"]["draftOrderCountBaselines"] =
            json!(self.store.base.draft_order_count_baselines.clone());
        snapshot["baseState"]["metafieldDefinitions"] = base_metafield_definitions_value;
        snapshot["baseState"]["metafieldDefinitionOwnerCatalogs"] =
            base_metafield_definition_owner_catalogs_value;
        snapshot["baseState"]["metafieldDefinitionNamespaces"] =
            base_metafield_definition_namespaces_value;
        snapshot["stagedState"]["deletedMetafieldDefinitions"] =
            deleted_metafield_definitions_value;
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
        if !self.store.base.function_metadata.is_empty() {
            snapshot["baseState"]["functionMetadata"] =
                json!(self.store.base.function_metadata.clone());
            snapshot["baseState"]["functionMetadataOrder"] =
                json!(self.store.base.function_metadata_order.clone());
        }
        if self.store.base.function_metadata_catalog_hydrated {
            snapshot["baseState"]["functionMetadataCatalogHydrated"] = json!(true);
        }
        if !self
            .store
            .base
            .function_metadata_hydrated_api_types
            .is_empty()
        {
            snapshot["baseState"]["functionMetadataHydratedApiTypes"] = json!(self
                .store
                .base
                .function_metadata_hydrated_api_types
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.base.function_validations.is_empty() {
            snapshot["baseState"]["functionValidations"] =
                json!(self.store.base.function_validations.clone());
            snapshot["baseState"]["functionValidationOrder"] =
                json!(self.store.base.function_validation_order.clone());
        }
        if self.store.base.function_validations_catalog_hydrated {
            snapshot["baseState"]["functionValidationsCatalogHydrated"] = json!(true);
        }
        if !self.store.base.function_cart_transforms.is_empty() {
            snapshot["baseState"]["functionCartTransforms"] =
                json!(self.store.base.function_cart_transforms.clone());
            snapshot["baseState"]["functionCartTransformOrder"] =
                json!(self.store.base.function_cart_transform_order.clone());
        }
        if self.store.base.function_cart_transforms_catalog_hydrated {
            snapshot["baseState"]["functionCartTransformsCatalogHydrated"] = json!(true);
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
            .function_fulfillment_constraint_rules_catalog_hydrated
        {
            snapshot["baseState"]["functionFulfillmentConstraintRulesCatalogHydrated"] =
                json!(true);
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
        if self.store.staged.next_customer_payment_method_id != 1 {
            snapshot["stagedState"]["nextCustomerPaymentMethodId"] =
                json!(self.store.staged.next_customer_payment_method_id);
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
        if self.store.staged.next_order_customer_order_id != 1 {
            snapshot["stagedState"]["nextOrderCustomerOrderId"] =
                json!(self.store.staged.next_order_customer_order_id);
        }
        if self.store.staged.next_order_number != 1 {
            snapshot["stagedState"]["nextOrderNumber"] = json!(self.store.staged.next_order_number);
        }
        if self.store.staged.next_draft_order_bulk_tag_job_id != 1 {
            snapshot["stagedState"]["nextDraftOrderBulkTagJobId"] =
                json!(self.store.staged.next_draft_order_bulk_tag_job_id);
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
            // These synthetic id counters MUST round-trip through dump/restore.
            // The parity runner restores mainState before every target, so if a
            // counter resets to 1 here a later companyCreate reuses an existing
            // id and silently overwrites a previously-staged company/contact.
            snapshot["stagedState"]["nextB2bCompanyId"] =
                json!(self.store.staged.next_b2b_company_id);
            snapshot["stagedState"]["nextB2bContactId"] =
                json!(self.store.staged.next_b2b_contact_id);
            snapshot["stagedState"]["nextB2bContactRoleAssignmentId"] =
                json!(self.store.staged.next_b2b_contact_role_assignment_id);
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
        if !self.store.staged.inventory_shipments.is_empty() {
            snapshot["stagedState"]["inventoryShipments"] =
                serde_json::to_value(&self.store.staged.inventory_shipments).unwrap_or_default();
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
        if !self.store.staged.markets_dirty_ids.is_empty() {
            snapshot["stagedState"]["marketsDirtyIds"] =
                json!(self.store.staged.markets_dirty_ids.clone());
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
        ok_json(json!({
            "schema": RUST_STATE_DUMP_SCHEMA,
            "createdAt": created_at,
            "state": self.state_snapshot(),
            "log": { "entries": self.log_entries },
            "nextSyntheticId": self.next_synthetic_id
        }))
    }

    pub(in crate::proxy) fn restore_state(&mut self, request: &Request) -> Response {
        let Ok(dump) = serde_json::from_str::<Value>(&request.body) else {
            return json_error(400, "Invalid Rust state dump JSON");
        };
        if dump.get("schema").and_then(Value::as_str) != Some(RUST_STATE_DUMP_SCHEMA) {
            return json_error(400, "Unsupported Rust state dump schema");
        }
        let Some(state) = dump.get("state") else {
            return json_error(400, "Rust state dump is missing state");
        };
        if !state.is_object() {
            return json_error(400, "Rust state dump is missing state");
        }
        for path in [
            "state.baseState",
            "state.baseState.products",
            "state.baseState.productOrder",
            "state.baseState.savedSearches",
            "state.baseState.savedSearchOrder",
            "state.stagedState",
            "state.stagedState.products",
            "state.stagedState.productOrder",
            "state.stagedState.deletedProductIds",
            "state.stagedState.savedSearches",
            "state.stagedState.savedSearchOrder",
            "state.stagedState.deletedSavedSearchIds",
            "state.stagedState.shippingPackages",
            "state.stagedState.deletedShippingPackageIds",
            "state.stagedState.delegatedAccessTokens",
            "state.stagedState.customers",
            "state.stagedState.deletedCustomerIds",
            "state.stagedState.customerOrders",
            "log.entries",
        ] {
            if !rust_state_dump_path_exists(&dump, path) {
                return json_error(400, &format!("Rust state dump is missing {path}"));
            }
        }
        let Some(next_synthetic_id) = dump.get("nextSyntheticId").and_then(Value::as_u64) else {
            return json_error(400, "Invalid Rust synthetic identity");
        };
        if next_synthetic_id == 0 {
            return json_error(400, "Invalid Rust synthetic identity");
        }

        self.store.products.base.replace_with_order(
            product_state_map_from_json(&state["baseState"]["products"]),
            string_array_from_json(&state["baseState"]["productOrder"]),
        );
        self.store.product_variants.base.replace_with_order(
            product_variant_state_map_from_json(&state["baseState"]["productVariants"]),
            string_array_from_json(&state["baseState"]["productVariantOrder"]),
        );
        self.store.base.orders.replace_with_order(
            value_map_from_json(state["baseState"].get("orders")),
            state["baseState"]
                .get("orderOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.order_count_baselines =
            value_map_from_json(state["baseState"].get("orderCountBaselines"));
        self.store.base.draft_orders.replace_with_order(
            value_map_from_json(state["baseState"].get("draftOrders")),
            state["baseState"]
                .get("draftOrderOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.draft_order_count_baselines =
            value_map_from_json(state["baseState"].get("draftOrderCountBaselines"));
        self.store.base.discount_count_baselines =
            value_map_from_json(state["baseState"].get("discountCountBaselines"));
        self.store.base.inventory_transfers.replace_with_order(
            state["baseState"]
                .get("inventoryTransfers")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            state["baseState"]
                .get("inventoryTransferOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.bulk_operations.replace_with_order(
            value_map_from_json(state["baseState"].get("bulkOperations")),
            state["baseState"]
                .get("bulkOperationOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.bulk_operations_observed = state["baseState"]
            .get("bulkOperationsObserved")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.store.products.staged.replace_with_order(
            product_state_map_from_json(&state["stagedState"]["products"]),
            string_array_from_json(&state["stagedState"]["productOrder"]),
        );
        self.store.product_variants.staged.replace_with_order(
            product_variant_state_map_from_json(&state["stagedState"]["productVariants"]),
            string_array_from_json(&state["stagedState"]["productVariantOrder"]),
        );
        self.store.products.staged.replace_tombstones(
            string_array_from_json(&state["stagedState"]["deletedProductIds"])
                .into_iter()
                .collect(),
        );
        self.store.product_variants.staged.replace_tombstones(
            string_array_from_json(&state["stagedState"]["deletedProductVariantIds"])
                .into_iter()
                .collect(),
        );
        replace_staged_value_records(
            &mut self.store.staged.product_feeds,
            &state["stagedState"],
            "productFeeds",
            Some("productFeedOrder"),
            Some("deletedProductFeedIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.collections,
            &state["stagedState"],
            "collections",
            None,
            Some("deletedCollectionIds"),
        );
        self.store.staged.deleted_collection_handles =
            string_array_from_json(&state["stagedState"]["deletedCollectionHandles"])
                .into_iter()
                .collect();
        self.store.staged.collection_jobs =
            value_map_from_json(state["stagedState"].get("collectionJobs"));
        self.store.staged.installed_apps =
            value_map_from_json(state["stagedState"].get("installedApps"));
        self.store.staged.revoked_app_access_scopes = state["stagedState"]
            .get("revokedAppAccessScopes")
            .and_then(Value::as_object)
            .map(|records| {
                records
                    .iter()
                    .map(|(app_id, scopes)| {
                        (
                            app_id.clone(),
                            string_array_from_json(scopes).into_iter().collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.uninstalled_app_ids = state["stagedState"]
            .get("uninstalledAppIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.delegate_access_tokens =
            value_map_from_json(state["stagedState"].get("delegatedAccessTokens"));
        self.store.staged.online_store_integrations =
            value_map_from_json(state["stagedState"].get("onlineStoreIntegrations"));
        self.store.staged.online_store_blogs =
            value_map_from_json(state["stagedState"].get("onlineStoreBlogs"));
        self.store.staged.online_store_blog_order = state["stagedState"]
            .get("onlineStoreBlogOrder")
            .map(string_array_from_json)
            .unwrap_or_default();
        self.store.staged.deleted_online_store_blog_ids = state["stagedState"]
            .get("deletedOnlineStoreBlogIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.online_store_blogs_count_base = state["stagedState"]
            .get("onlineStoreBlogsCountBase")
            .and_then(Value::as_u64)
            .map(|count| count as usize);
        self.store.staged.online_store_pages =
            value_map_from_json(state["stagedState"].get("onlineStorePages"));
        self.store.staged.online_store_page_order = state["stagedState"]
            .get("onlineStorePageOrder")
            .map(string_array_from_json)
            .unwrap_or_default();
        self.store.staged.deleted_online_store_page_ids = state["stagedState"]
            .get("deletedOnlineStorePageIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.online_store_pages_count_base = state["stagedState"]
            .get("onlineStorePagesCountBase")
            .and_then(Value::as_u64)
            .map(|count| count as usize);
        self.store.staged.online_store_articles =
            value_map_from_json(state["stagedState"].get("onlineStoreArticles"));
        self.store.staged.online_store_article_order = state["stagedState"]
            .get("onlineStoreArticleOrder")
            .map(string_array_from_json)
            .unwrap_or_default();
        self.store.staged.deleted_online_store_article_ids = state["stagedState"]
            .get("deletedOnlineStoreArticleIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.online_store_comments =
            value_map_from_json(state["stagedState"].get("onlineStoreComments"));
        self.store.staged.online_store_comment_order = state["stagedState"]
            .get("onlineStoreCommentOrder")
            .map(string_array_from_json)
            .unwrap_or_default();
        self.store.staged.deleted_online_store_comment_ids = state["stagedState"]
            .get("deletedOnlineStoreCommentIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.bulk_operations.replace_with_order(
            value_map_from_json(state["stagedState"].get("bulkOperations")),
            state["stagedState"]
                .get("bulkOperationOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.staged.bulk_operation_staged_uploads = state["stagedState"]
            .get("bulkOperationStagedUploads")
            .and_then(Value::as_object)
            .map(|uploads| {
                uploads
                    .iter()
                    .map(|(path, size)| (path.clone(), size.as_u64()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.bulk_operation_staged_upload_bodies = state["stagedState"]
            .get("bulkOperationStagedUploadBodies")
            .and_then(Value::as_object)
            .map(|uploads| {
                uploads
                    .iter()
                    .filter_map(|(path, body)| {
                        body.as_str().map(|body| (path.clone(), body.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.bulk_operation_results = state["stagedState"]
            .get("bulkOperationResults")
            .and_then(Value::as_object)
            .map(|results| {
                results
                    .iter()
                    .filter_map(|(id, result)| {
                        result
                            .as_str()
                            .map(|result| (id.clone(), result.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.saved_searches.base.replace_with_order(
            saved_search_state_map_from_json(&state["baseState"]["savedSearches"]),
            string_array_from_json(&state["baseState"]["savedSearchOrder"]),
        );
        self.store.base.discounts.replace_with_order(
            value_map_from_json(state["baseState"].get("discounts")),
            state["baseState"]
                .get("discountOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.segments.replace_with_order(
            value_map_from_json(state["baseState"].get("segments")),
            state["baseState"]
                .get("segmentOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.gift_cards = value_map_from_json(state["baseState"].get("giftCards"));
        self.store.base.gift_card_configuration = state["baseState"]
            .get("giftCardConfiguration")
            .filter(|configuration| configuration.is_object())
            .cloned();
        self.store.base.gift_card_complete_queries = state["baseState"]
            .get("giftCardCompleteQueries")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let base_shop = state["baseState"]
            .get("shop")
            .filter(|shop| shop.is_object() || shop.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        let mut base_shop_policies =
            shop_policy_state_map_from_json(&state["baseState"]["shopPolicies"]);
        let mut base_shop_policy_order =
            string_array_from_json(&state["baseState"]["shopPolicyOrder"]);
        if base_shop_policies.is_empty() {
            (base_shop_policies, base_shop_policy_order) = shop_policy_state_from_shop(&base_shop);
        }
        self.store
            .shop_policies
            .base
            .replace_with_order(base_shop_policies, base_shop_policy_order);
        self.store.base.storefront_shop = state["baseState"]
            .get("storefrontShop")
            .filter(|shop| shop.is_object() || shop.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        self.store.base.storefront_localizations = state["baseState"]
            .get("storefrontLocalizations")
            .and_then(Value::as_object)
            .map(|contexts| {
                contexts
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.base.storefront_product_tags = state["baseState"]
            .get("storefrontProductTags")
            .filter(|connection| connection.is_object() || connection.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        self.store.base.storefront_product_types = state["baseState"]
            .get("storefrontProductTypes")
            .filter(|connection| connection.is_object() || connection.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        self.store.base.storefront_payment_settings = state["baseState"]
            .get("storefrontPaymentSettings")
            .filter(|settings| settings.is_object() || settings.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        self.store.base.storefront_locations.replace_with_order(
            value_map_from_json(state["baseState"].get("storefrontLocations")),
            state["baseState"]
                .get("storefrontLocationOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.storefront_location_cursors = state["baseState"]
            .get("storefrontLocationCursors")
            .and_then(Value::as_object)
            .map(|cursors| {
                cursors
                    .iter()
                    .filter_map(|(id, cursor)| {
                        cursor
                            .as_str()
                            .map(|cursor| (id.clone(), cursor.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.base.storefront_public_api_versions = state["baseState"]
            .get("storefrontPublicApiVersions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.store.base.storefront_menus.replace_with_order(
            value_map_from_json(state["baseState"].get("storefrontMenus")),
            state["baseState"]
                .get("storefrontMenuOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        let base_delivery_profiles =
            value_map_from_json(state["baseState"].get("deliveryProfiles"));
        let base_delivery_profile_order = state["baseState"]
            .get("deliveryProfileOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| base_delivery_profiles.keys().cloned().collect());
        self.store
            .base
            .delivery_profiles
            .replace_with_order(base_delivery_profiles, base_delivery_profile_order);
        let base_delivery_promise_providers =
            value_map_from_json(state["baseState"].get("deliveryPromiseProviders"));
        let base_delivery_promise_provider_order = state["baseState"]
            .get("deliveryPromiseProviderOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| base_delivery_promise_providers.keys().cloned().collect());
        self.store
            .base
            .delivery_promise_providers
            .replace_with_order(
                base_delivery_promise_providers,
                base_delivery_promise_provider_order,
            );
        self.store
            .base
            .delivery_promise_provider_complete_location_ids = string_set_from_json(
            state["baseState"].get("deliveryPromiseProviderCompleteLocationIds"),
        );
        let base_delivery_promise_participants =
            value_map_from_json(state["baseState"].get("deliveryPromiseParticipants"));
        let base_delivery_promise_participant_order = state["baseState"]
            .get("deliveryPromiseParticipantOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| base_delivery_promise_participants.keys().cloned().collect());
        self.store
            .base
            .delivery_promise_participants
            .replace_with_order(
                base_delivery_promise_participants,
                base_delivery_promise_participant_order,
            );
        self.store.base.delivery_promise_participant_baseline_orders = string_array_map_from_json(
            state["baseState"].get("deliveryPromiseParticipantBaselineOrders"),
        );
        self.store.base.delivery_promise_participant_cursor_ids =
            string_map_map_from_json(state["baseState"].get("deliveryPromiseParticipantCursorIds"));
        self.store.base.delivery_promise_participant_complete_scopes = string_set_from_json(
            state["baseState"].get("deliveryPromiseParticipantCompleteScopes"),
        );
        self.store.base.delivery_promise_participant_next_cursors =
            string_map_from_json(state["baseState"].get("deliveryPromiseParticipantNextCursors"));
        self.store
            .base
            .delivery_promise_participant_previous_cursors = string_map_from_json(
            state["baseState"].get("deliveryPromiseParticipantPreviousCursors"),
        );
        self.store.base.delivery_promise_complete_node_ids =
            string_set_from_json(state["baseState"].get("deliveryPromiseCompleteNodeIds"));
        let base_locations = value_map_from_json(state["baseState"].get("locations"));
        let base_location_order = state["baseState"]
            .get("locationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| base_locations.keys().cloned().collect());
        self.store
            .base
            .locations
            .replace_with_order(base_locations, base_location_order);
        self.store.base.inventory_levels =
            inventory_levels_from_json(&state["baseState"]["inventoryLevels"]);
        self.store.base.inventory_level_ids =
            inventory_level_ids_from_json(&state["baseState"]["inventoryLevelIds"]);
        self.store.base.inventory_level_order =
            inventory_level_order_from_json(&state["baseState"]["inventoryLevelOrder"]);
        self.store.base.inventory_level_cursors = state["baseState"]
            .get("inventoryLevelCursors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.base.inventory_item_cursors = state["baseState"]
            .get("inventoryItemCursors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.base.inventory_items_catalog_hydrated = state["baseState"]
            ["inventoryItemsCatalogHydrated"]
            .as_bool()
            .unwrap_or(false);
        self.store.base.inactive_inventory_levels =
            inactive_inventory_levels_from_json(&state["baseState"]["inactiveInventoryLevels"]);
        self.store.base.inventory_quantity_updated_at = inventory_quantity_updated_at_from_json(
            &state["baseState"]["inventoryQuantityUpdatedAt"],
        );
        self.store.base.shop = base_shop;
        self.store.base.publication_ids =
            string_array_from_json(&state["baseState"]["publicationIds"])
                .into_iter()
                .collect();
        self.store.base.publication_count = state["baseState"]["publicationCount"]
            .as_u64()
            .map(|count| count as usize);
        self.store.base.available_locales = state["baseState"]["availableLocales"]
            .as_object()
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|(locale, name)| {
                        name.as_str().map(|name| (locale.clone(), name.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_else(default_available_locales);
        self.store.base.shop_locales = state["baseState"]["shopLocales"]
            .as_object()
            .map(|locales| {
                locales
                    .iter()
                    .map(|(locale, record)| (locale.clone(), record.clone()))
                    .collect()
            })
            .unwrap_or_else(|| {
                BTreeMap::from([(
                    "en".to_string(),
                    json!({
                        "locale": "en",
                        "name": "English",
                        "primary": true,
                        "published": true,
                        "marketWebPresences": []
                    }),
                )])
            });
        self.store.base.localization_product_ids = state["baseState"]
            .get("localizationProductIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.base.function_metadata =
            value_map_from_json(state["baseState"].get("functionMetadata"));
        self.store.base.function_metadata_order = state["baseState"]
            .get("functionMetadataOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| self.store.base.function_metadata.keys().cloned().collect());
        self.store.base.function_metadata_catalog_hydrated = state["baseState"]
            ["functionMetadataCatalogHydrated"]
            .as_bool()
            .unwrap_or(false);
        self.store.base.function_metadata_hydrated_api_types =
            string_set_from_json(state["baseState"].get("functionMetadataHydratedApiTypes"));
        self.store.base.function_validations =
            value_map_from_json(state["baseState"].get("functionValidations"));
        self.store.base.function_validation_order = state["baseState"]
            .get("functionValidationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .base
                    .function_validations
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.base.function_validations_catalog_hydrated = state["baseState"]
            ["functionValidationsCatalogHydrated"]
            .as_bool()
            .unwrap_or(false);
        self.store.base.function_cart_transforms =
            value_map_from_json(state["baseState"].get("functionCartTransforms"));
        self.store.base.function_cart_transform_order = state["baseState"]
            .get("functionCartTransformOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .base
                    .function_cart_transforms
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.base.function_cart_transforms_catalog_hydrated = state["baseState"]
            ["functionCartTransformsCatalogHydrated"]
            .as_bool()
            .unwrap_or(false);
        self.store.base.function_fulfillment_constraint_rules =
            value_map_from_json(state["baseState"].get("functionFulfillmentConstraintRules"));
        self.store.base.function_fulfillment_constraint_rule_order = state["baseState"]
            .get("functionFulfillmentConstraintRuleOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .base
                    .function_fulfillment_constraint_rules
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store
            .base
            .function_fulfillment_constraint_rules_catalog_hydrated = state["baseState"]
            ["functionFulfillmentConstraintRulesCatalogHydrated"]
            .as_bool()
            .unwrap_or(false);
        self.store.base.metafield_definitions =
            metafield_definition_map_from_json(state["baseState"].get("metafieldDefinitions"));
        self.store.base.metafield_definition_owner_catalogs = state["baseState"]
            .get("metafieldDefinitionOwnerCatalogs")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.base.metafield_definition_namespaces =
            metafield_definition_namespace_set_from_json(
                state["baseState"].get("metafieldDefinitionNamespaces"),
            );
        self.store.base.b2b_companies.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bCompanies")),
            state["baseState"]
                .get("b2bCompanyOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_company_count_baselines =
            value_map_from_json(state["baseState"].get("b2bCompanyCountBaselines"));
        self.store.base.b2b_locations.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bLocations")),
            state["baseState"]
                .get("b2bLocationOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_contacts.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bContacts")),
            state["baseState"]
                .get("b2bContactOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_contact_roles.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bContactRoles")),
            state["baseState"]
                .get("b2bContactRoleOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_role_assignments.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bRoleAssignments")),
            state["baseState"]
                .get("b2bRoleAssignmentOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_staff_assignments.replace_with_order(
            value_map_from_json(state["baseState"].get("b2bStaffAssignments")),
            state["baseState"]
                .get("b2bStaffAssignmentOrder")
                .map(string_array_from_json)
                .unwrap_or_default(),
        );
        self.store.base.b2b_staff_member_ids = state["baseState"]
            .get("b2bStaffMemberIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.publication_ids =
            string_array_from_json(&state["stagedState"]["publicationIds"])
                .into_iter()
                .collect();
        self.store.staged.created_publication_ids =
            string_array_from_json(&state["stagedState"]["createdPublicationIds"])
                .into_iter()
                .collect();
        self.store.staged.publications =
            value_map_from_json(state["stagedState"].get("publications"));
        self.store.staged.current_channel_publication_id = state["stagedState"]
            .get("currentChannelPublicationId")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.store.staged.current_channel_publication_resolved = state["stagedState"]
            .get("currentChannelPublicationResolved")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.store.staged.resource_publications = state["stagedState"]["resourcePublications"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(resource, pubs)| {
                        (
                            resource.clone(),
                            string_array_from_json(pubs).into_iter().collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.saved_searches.staged.replace_with_order(
            saved_search_state_map_from_json(&state["stagedState"]["savedSearches"]),
            string_array_from_json(&state["stagedState"]["savedSearchOrder"]),
        );
        self.store.saved_searches.staged.replace_tombstones(
            string_array_from_json(&state["stagedState"]["deletedSavedSearchIds"])
                .into_iter()
                .collect(),
        );
        self.store.staged.media_ready_on_read = state["stagedState"]
            .get("mediaReadyOnReadIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.shop_policies.staged.replace_with_order(
            shop_policy_state_map_from_json(&state["stagedState"]["shopPolicies"]),
            string_array_from_json(&state["stagedState"]["shopPolicyOrder"]),
        );
        self.store.shop_policies.staged.replace_tombstones(
            string_array_from_json(&state["stagedState"]["deletedShopPolicyIds"])
                .into_iter()
                .collect(),
        );
        replace_staged_value_records(
            &mut self.store.staged.shipping_packages,
            &state["stagedState"],
            "shippingPackages",
            None,
            Some("deletedShippingPackageIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.customers,
            &state["stagedState"],
            "customers",
            None,
            Some("deletedCustomerIds"),
        );
        self.store.staged.customer_addresses =
            value_map_from_json(state["stagedState"].get("customerAddresses"));
        self.store.staged.customer_address_order = state["stagedState"]["customerAddressOrder"]
            .as_object()
            .map(|addresses_by_customer| {
                addresses_by_customer
                    .iter()
                    .map(|(customer_id, ids)| {
                        (
                            customer_id.clone(),
                            ids.as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|value| value.as_str().map(str::to_string))
                                .collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.customer_address_owners = state["stagedState"]["customerAddressOwners"]
            .as_object()
            .map(|owners| {
                owners
                    .iter()
                    .filter_map(|(address_id, customer_id)| {
                        customer_id
                            .as_str()
                            .map(|customer_id| (address_id.clone(), customer_id.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.customer_orders = state["stagedState"]["customerOrders"]
            .as_object()
            .map(|orders_by_customer| {
                orders_by_customer
                    .iter()
                    .map(|(id, orders)| {
                        (id.clone(), orders.as_array().cloned().unwrap_or_default())
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.merged_customer_ids = state["stagedState"]["mergedCustomerIds"]
            .as_object()
            .map(|merged| {
                merged
                    .iter()
                    .filter_map(|(source_id, result_id)| {
                        result_id
                            .as_str()
                            .map(|result_id| (source_id.clone(), result_id.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.customer_merge_requests =
            value_map_from_json(state["stagedState"].get("customerMergeRequests"));
        self.store.staged.customer_data_erasure_requests =
            value_map_from_json(state["stagedState"].get("customerDataErasureRequests"));
        self.store.staged.locally_created_customer_ids = state["stagedState"]
            .get("locallyCreatedCustomerIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.storefront_customer_email_index = state["stagedState"]
            .get("storefrontCustomerEmailIndex")
            .and_then(Value::as_object)
            .map(|index| {
                index
                    .iter()
                    .filter_map(|(email, customer_id)| {
                        customer_id
                            .as_str()
                            .map(|customer_id| (email.clone(), customer_id.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.storefront_customer_access_tokens =
            value_map_from_json(state["stagedState"].get("storefrontCustomerAccessTokens"));
        self.store.staged.next_storefront_customer_access_token_id = counter_from_json_with_floor(
            &state["stagedState"],
            "nextStorefrontCustomerAccessTokenId",
            1,
        );
        self.store.staged.next_storefront_customer_reset_token_id = counter_from_json_with_floor(
            &state["stagedState"],
            "nextStorefrontCustomerResetTokenId",
            1,
        );
        self.store.staged.storefront_carts = state["stagedState"]
            .get("storefrontCarts")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.storefront_cart_order = state["stagedState"]
            .get("storefrontCartOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| self.store.staged.storefront_carts.keys().cloned().collect());
        self.store.staged.storefront_cart_lines = state["stagedState"]
            .get("storefrontCartLines")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.storefront_cart_line_order = state["stagedState"]
            .get("storefrontCartLineOrder")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.next_storefront_cart_id =
            counter_from_json_with_floor(&state["stagedState"], "nextStorefrontCartId", 1);
        self.store.staged.next_storefront_cart_line_id =
            counter_from_json_with_floor(&state["stagedState"], "nextStorefrontCartLineId", 1);
        self.store.staged.next_storefront_cart_applied_gift_card_id = counter_from_json_with_floor(
            &state["stagedState"],
            "nextStorefrontCartAppliedGiftCardId",
            1,
        );
        self.store.staged.next_storefront_cart_metafield_id =
            counter_from_json_with_floor(&state["stagedState"], "nextStorefrontCartMetafieldId", 1);
        self.store.staged.next_storefront_cart_delivery_address_id = counter_from_json_with_floor(
            &state["stagedState"],
            "nextStorefrontCartDeliveryAddressId",
            1,
        );
        self.store.staged.customer_count_baselines =
            value_map_from_json(state["stagedState"].get("customerCountBaselines"));
        if self.store.staged.customer_count_baselines.is_empty() {
            if let Some(count) = state["stagedState"]["customersCountBase"].as_u64() {
                self.store.staged.customer_count_baselines.insert(
                    customer_count_baseline_key(&BTreeMap::new()),
                    count_object(count),
                );
            }
        }
        replace_staged_value_records(
            &mut self.store.staged.store_credit_accounts,
            &state["stagedState"],
            "storeCreditAccounts",
            Some("storeCreditAccountOrder"),
            None,
        );
        self.store.staged.store_credit_transactions =
            value_map_from_json(state["stagedState"].get("storeCreditTransactions"));
        self.store.staged.store_credit_transaction_order = state["stagedState"]
            .get("storeCreditTransactionOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .store_credit_transactions
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.staged.next_store_credit_account_id =
            counter_from_json_with_floor(&state["stagedState"], "nextStoreCreditAccountId", 1);
        self.store.staged.next_store_credit_transaction_id =
            counter_from_json_with_floor(&state["stagedState"], "nextStoreCreditTransactionId", 1);
        self.store.staged.gift_cards = value_map_from_json(state["stagedState"].get("giftCards"));
        self.store.staged.taggable_resources =
            value_map_from_json(state["stagedState"].get("taggableResources"));
        self.store.staged.customer_payment_methods =
            value_map_from_json(state["stagedState"].get("customerPaymentMethods"));
        self.store.staged.customer_payment_method_customer_index = state["stagedState"]
            .get("customerPaymentMethodCustomerIndex")
            .and_then(Value::as_object)
            .map(|index| {
                index
                    .iter()
                    .map(|(customer_id, ids)| (customer_id.clone(), string_array_from_json(ids)))
                    .collect()
            })
            .unwrap_or_else(|| {
                customer_payment_method_index_from_records(
                    &self.store.staged.customer_payment_methods,
                )
            });
        self.store.staged.payment_customizations =
            value_map_from_json(state["stagedState"].get("paymentCustomizations"));
        self.store.staged.deleted_payment_customization_ids = state["stagedState"]
            .get("deletedPaymentCustomizationIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.payment_customization_catalog_hydrated = state["stagedState"]
            .get("paymentCustomizationCatalogHydrated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.store.staged.next_customer_payment_method_id =
            counter_from_json_with_floor(&state["stagedState"], "nextCustomerPaymentMethodId", 1);
        self.store.staged.abandonments =
            value_map_from_json(state["stagedState"].get("abandonments"));
        self.store.staged.order_customer_orders =
            value_map_from_json(state["stagedState"].get("orderCustomerOrders"));
        self.store.staged.order_customer_cancelled_ids = state["stagedState"]
            .get("orderCustomerCancelledIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.order_customer_b2b_order_ids = state["stagedState"]
            .get("orderCustomerB2bOrderIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.order_customer_contact_customer_ids = state["stagedState"]
            .get("orderCustomerContactCustomerIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.next_order_customer_order_id =
            counter_from_json_with_floor(&state["stagedState"], "nextOrderCustomerOrderId", 1);
        replace_staged_value_records(
            &mut self.store.staged.orders,
            &state["stagedState"],
            "orders",
            None,
            Some("deletedOrderIds"),
        );
        self.store.staged.next_order_id =
            counter_from_json_with_floor(&state["stagedState"], "nextOrderId", 1);
        self.store.staged.next_order_number =
            counter_from_json_with_floor(&state["stagedState"], "nextOrderNumber", 1);
        self.store.staged.next_refund_id =
            counter_from_json_with_floor(&state["stagedState"], "nextRefundId", 1);
        self.store.staged.next_refund_line_item_id =
            counter_from_json_with_floor(&state["stagedState"], "nextRefundLineItemId", 1);
        self.store.staged.order_payment_next_transaction_id =
            counter_from_json_with_floor(&state["stagedState"], "orderPaymentNextTransactionId", 3);
        self.advance_order_counters_from_staged_orders();
        // Draft orders are dumped in the cursor-wrapped overlay format
        // ({ "id", "cursor", "data" }); unwrap `data` back to the staged record.
        // These MUST round-trip because the parity runner restores mainState
        // before every downstream target, so read-after-write across targets
        // (draftOrder reads, duplicate/delete chains) would otherwise lose state.
        let staged_draft_orders = state["stagedState"]["draftOrders"]
            .as_object()
            .map(|drafts| {
                drafts
                    .iter()
                    .map(|(id, wrapper)| {
                        let record = wrapper
                            .get("data")
                            .cloned()
                            .unwrap_or_else(|| wrapper.clone());
                        (id.clone(), record)
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store
            .staged
            .draft_orders
            .replace_with_order_and_tombstones(
                staged_draft_orders,
                state["stagedState"]
                    .get("draftOrderOrder")
                    .map(string_array_from_json)
                    .unwrap_or_default(),
                state["stagedState"]
                    .get("deletedDraftOrderIds")
                    .map(string_array_from_json)
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            );
        self.store.staged.next_draft_order_id =
            counter_from_json_with_floor(&state["stagedState"], "nextDraftOrderId", 1);
        self.advance_draft_order_counter_from_staged_draft_orders();
        self.store.staged.next_draft_order_bulk_tag_job_id =
            counter_from_json_with_floor(&state["stagedState"], "nextDraftOrderBulkTagJobId", 1);
        self.store.staged.draft_order_tags = state["stagedState"]["draftOrderTags"]
            .as_object()
            .map(|tags| {
                tags.iter()
                    .map(|(id, list)| {
                        (
                            id.clone(),
                            list.as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|value| value.as_str().map(str::to_string))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.returns = value_map_from_json(state["stagedState"].get("returns"));
        self.store.staged.returns_by_order = state["stagedState"]["returnsByOrder"]
            .as_object()
            .map(|returns_by_order| {
                returns_by_order
                    .iter()
                    .map(|(id, returns)| {
                        (
                            id.clone(),
                            returns
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|value| value.as_str().map(str::to_string))
                                .collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.reverse_deliveries =
            value_map_from_json(state["stagedState"].get("reverseDeliveries"));
        self.store.staged.reverse_fulfillment_orders =
            value_map_from_json(state["stagedState"].get("reverseFulfillmentOrders"));
        self.store.staged.observed_shipping_locations =
            value_map_from_json(state["stagedState"].get("observedShippingLocations"));
        self.store.staged.observed_shipping_location_order = state["stagedState"]
            .get("observedShippingLocationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .observed_shipping_locations
                    .keys()
                    .cloned()
                    .collect()
            });
        replace_staged_value_records(
            &mut self.store.staged.locations,
            &state["stagedState"],
            "locations",
            Some("locationOrder"),
            Some("deletedLocationIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.delivery_profiles,
            &state["stagedState"],
            "deliveryProfiles",
            Some("deliveryProfileOrder"),
            Some("deletedDeliveryProfileIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.delivery_promise_providers,
            &state["stagedState"],
            "deliveryPromiseProviders",
            Some("deliveryPromiseProviderOrder"),
            Some("deletedDeliveryPromiseProviderIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.delivery_promise_participants,
            &state["stagedState"],
            "deliveryPromiseParticipants",
            Some("deliveryPromiseParticipantOrder"),
            Some("deletedDeliveryPromiseParticipantIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.delivery_customizations,
            &state["stagedState"],
            "deliveryCustomizations",
            Some("deliveryCustomizationOrder"),
            Some("deletedDeliveryCustomizationIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.segments,
            &state["stagedState"],
            "segments",
            Some("segmentOrder"),
            Some("deletedSegmentIds"),
        );
        self.store.staged.fulfillment_order_cursors = state["stagedState"]
            .get("fulfillmentOrderCursors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.inventory_levels =
            inventory_levels_from_json(&state["stagedState"]["inventoryLevels"]);
        self.store.staged.inventory_level_ids =
            inventory_level_ids_from_json(&state["stagedState"]["inventoryLevelIds"]);
        self.store.staged.inventory_level_order =
            inventory_level_order_from_json(&state["stagedState"]["inventoryLevelOrder"]);
        self.store.staged.inventory_level_cursors = state["stagedState"]
            .get("inventoryLevelCursors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.inactive_inventory_levels =
            inactive_inventory_levels_from_json(&state["stagedState"]["inactiveInventoryLevels"]);
        self.store.staged.active_inventory_levels =
            inactive_inventory_levels_from_json(&state["stagedState"]["activeInventoryLevels"]);
        self.store
            .staged
            .inventory_transfers
            .replace_with_order_and_tombstones(
                state["stagedState"]
                    .get("inventoryTransfers")
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
                    .unwrap_or_default(),
                state["stagedState"]
                    .get("inventoryTransferOrder")
                    .map(string_array_from_json)
                    .unwrap_or_default(),
                state["stagedState"]
                    .get("deletedInventoryTransferIds")
                    .map(string_array_from_json)
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            );
        self.store.staged.inventory_shipments = state["stagedState"]
            .get("inventoryShipments")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.inventory_quantity_updated_at = inventory_quantity_updated_at_from_json(
            &state["stagedState"]["inventoryQuantityUpdatedAt"],
        );
        self.store.staged.next_inventory_quantity_timestamp = counter_from_json_with_floor(
            &state["stagedState"],
            "nextInventoryQuantityTimestamp",
            0,
        );
        self.store.staged.inventory_adjustment_groups =
            value_map_from_json(state["stagedState"].get("inventoryAdjustmentGroups"));
        self.store.staged.location_limit_reached = state["stagedState"]
            .get("locationLimitReached")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.store.staged.order_edit_existing_order = state["stagedState"]
            .get("orderEditExistingOrder")
            .filter(|order| order.is_object())
            .cloned();
        self.store.staged.order_edit_existing_calculated_order = state["stagedState"]
            .get("orderEditExistingCalculatedOrder")
            .filter(|order| order.is_object())
            .cloned();
        self.store.staged.order_edit_existing_calculated_order_id = state["stagedState"]
            .get("orderEditExistingCalculatedOrderId")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.store.staged.order_edit_existing_session_order_id = state["stagedState"]
            .get("orderEditExistingSessionOrderId")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.store.staged.order_edit_money_bag_calculated_order_ids =
            string_map_from_json(state["stagedState"].get("orderEditMoneyBagCalculatedOrderIds"));
        self.store.staged.order_edit_existing_mode = state["stagedState"]
            .get("orderEditExistingMode")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.store.staged.order_edit_variant_catalog = state["stagedState"]
            .get("orderEditVariantCatalog")
            .filter(|catalog| catalog.is_object())
            .cloned()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        self.store.staged.order_edit_author = state["stagedState"]
            .get("orderEditAuthor")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.store.staged.b2b_companies =
            value_map_from_json(state["stagedState"].get("b2bCompanies"));
        replace_staged_value_records(
            &mut self.store.staged.b2b_locations,
            &state["stagedState"],
            "b2bLocations",
            Some("b2bLocationOrder"),
            Some("deletedB2bLocationIds"),
        );
        self.store.staged.b2b_contacts =
            value_map_from_json(state["stagedState"].get("b2bContacts"));
        self.store.staged.b2b_contact_roles =
            value_map_from_json(state["stagedState"].get("b2bContactRoles"));
        self.store.staged.b2b_role_assignments =
            value_map_from_json(state["stagedState"].get("b2bRoleAssignments"));
        self.store.staged.b2b_staff_assignments =
            value_map_from_json(state["stagedState"].get("b2bStaffAssignments"));
        replace_staged_value_records(
            &mut self.store.staged.metaobject_definitions,
            &state["stagedState"],
            "metaobjectDefinitions",
            None,
            Some("deletedMetaobjectDefinitionIds"),
        );
        replace_staged_value_records(
            &mut self.store.staged.metaobjects,
            &state["stagedState"],
            "metaobjects",
            None,
            Some("deletedMetaobjectIds"),
        );
        self.store.staged.url_redirects =
            value_map_from_json(state["stagedState"].get("urlRedirects"));
        self.store.staged.url_redirect_order = state["stagedState"]
            .get("urlRedirectOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| self.store.staged.url_redirects.keys().cloned().collect());
        self.store.staged.linked_product_option_metaobject_sets = state["stagedState"]
            .get("linkedProductOptionMetaobjectSets")
            .and_then(Value::as_array)
            .map(|sets| {
                sets.iter()
                    .map(|set| string_array_from_json(set).into_iter().collect())
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.metafield_definitions =
            metafield_definition_map_from_json(state["stagedState"].get("metafieldDefinitions"));
        self.store.staged.deleted_metafield_definitions = metafield_definition_key_set_from_json(
            state["stagedState"].get("deletedMetafieldDefinitions"),
        );
        self.store.staged.metafield_reference_ids = state["stagedState"]
            .get("metafieldReferenceIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.owner_metafields = state["stagedState"]
            .get("ownerMetafields")
            .and_then(Value::as_object)
            .map(|owners| {
                owners
                    .iter()
                    .map(|(owner_id, metafields)| {
                        (
                            owner_id.clone(),
                            metafields.as_array().cloned().unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_owner_metafields = state["stagedState"]
            .get("deletedOwnerMetafields")
            .and_then(Value::as_array)
            .map(|tombstones| {
                tombstones
                    .iter()
                    .filter_map(|tombstone| {
                        Some((
                            tombstone.get("ownerId")?.as_str()?.to_string(),
                            tombstone.get("namespace")?.as_str()?.to_string(),
                            tombstone.get("key")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.product_operations = state["stagedState"]
            .get("productOperations")
            .and_then(Value::as_object)
            .map(|operations| {
                operations
                    .iter()
                    .filter_map(|(id, operation)| {
                        serde_json::from_value::<ProductOperationRecord>(operation.clone())
                            .ok()
                            .map(|operation| (id.clone(), operation))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.flow_signatures = state["stagedState"]["flowSignatures"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.store.staged.flow_trigger_receipts = state["stagedState"]["flowTriggerReceipts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        replace_staged_value_records(
            &mut self.store.staged.discounts,
            &state["stagedState"],
            "discounts",
            None,
            Some("deletedDiscountIds"),
        );
        self.store.staged.discount_code_index = state["stagedState"]["discountCodeIndex"]
            .as_object()
            .map(|index| {
                index
                    .iter()
                    .filter_map(|(code, id)| id.as_str().map(|id| (code.clone(), id.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.discount_redeem_code_bulk_creations =
            value_map_from_json(state["stagedState"].get("discountRedeemCodeBulkCreations"));
        self.store.staged.deleted_b2b_contact_ids = state["stagedState"]
            .get("deletedB2bContactIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.deleted_b2b_company_ids = state["stagedState"]
            .get("deletedB2bCompanyIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.b2b_contact_role_assignments = state["stagedState"]
            .get("b2bContactRoleAssignments")
            .and_then(Value::as_object)
            .map(|assignments| {
                assignments
                    .iter()
                    .map(|(id, assignment)| (id.clone(), assignment.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_b2b_contact_role_assignment_ids = state["stagedState"]
            .get("deletedB2bContactRoleAssignmentIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.deleted_b2b_staff_assignment_ids = state["stagedState"]
            .get("deletedB2bStaffAssignmentIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.next_b2b_company_id =
            counter_from_json_with_floor(&state["stagedState"], "nextB2bCompanyId", 1);
        self.store.staged.next_b2b_contact_id =
            counter_from_json_with_floor(&state["stagedState"], "nextB2bContactId", 1);
        self.store.staged.next_b2b_contact_role_assignment_id = counter_from_json_with_floor(
            &state["stagedState"],
            "nextB2bContactRoleAssignmentId",
            1,
        );
        // Markets-domain staged maps — symmetric with the conditional emit in
        // state_snapshot. Missing keys restore to empty (the default).
        self.store.staged.markets = value_map_from_json(state["stagedState"].get("markets"));
        self.store.staged.deleted_market_ids = state["stagedState"]
            .get("deletedMarketIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.catalogs = value_map_from_json(state["stagedState"].get("catalogs"));
        self.store.staged.created_catalog_ids = state["stagedState"]
            .get("createdCatalogIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.price_lists = value_map_from_json(state["stagedState"].get("priceLists"));
        self.store.staged.web_presences =
            value_map_from_json(state["stagedState"].get("webPresences"));
        self.store.staged.markets_upstream_counts =
            value_map_from_json(state["stagedState"].get("marketsUpstreamCounts"));
        self.store.staged.markets_dirty_ids = state["stagedState"]
            .get("marketsDirtyIds")
            .and_then(Value::as_object)
            .map(|families| {
                families
                    .iter()
                    .map(|(family, ids)| {
                        (
                            family.clone(),
                            string_array_from_json(ids).into_iter().collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.markets_dirty_families = self
            .store
            .staged
            .markets_dirty_ids
            .keys()
            .cloned()
            .collect();
        self.store.staged.available_backup_regions =
            value_map_from_json(state["stagedState"].get("availableBackupRegions"));
        self.store.staged.shop_locales = state["stagedState"]
            .get("stagedShopLocales")
            .and_then(Value::as_object)
            .map(|locales| {
                locales
                    .iter()
                    .map(|(locale, record)| (locale.clone(), record.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.localization_translations = state["stagedState"]
            ["localizationTranslations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.store.staged.localization_resources = state["stagedState"]["localizationResources"]
            .as_object()
            .map(|resources| {
                resources
                    .iter()
                    .map(|(resource_id, content)| (resource_id.clone(), content.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.localization_dirty = state["stagedState"]["localizationDirty"]
            .as_bool()
            .unwrap_or(false);
        self.store.staged.function_metadata =
            value_map_from_json(state["stagedState"].get("functionMetadata"));
        self.store.staged.function_metadata_order = state["stagedState"]
            .get("functionMetadataOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .function_metadata
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.staged.function_validations =
            value_map_from_json(state["stagedState"].get("functionValidations"));
        self.store.staged.function_validation_order = state["stagedState"]
            .get("functionValidationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .function_validations
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.staged.deleted_function_validation_ids =
            string_set_from_json(state["stagedState"].get("deletedFunctionValidationIds"));
        self.store.staged.function_validations_dirty = state["stagedState"]
            ["functionValidationsDirty"]
            .as_bool()
            .unwrap_or(false);
        self.store.staged.function_cart_transforms =
            value_map_from_json(state["stagedState"].get("functionCartTransforms"));
        self.store.staged.function_cart_transform_order = state["stagedState"]
            .get("functionCartTransformOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .function_cart_transforms
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.staged.deleted_function_cart_transform_ids =
            string_set_from_json(state["stagedState"].get("deletedFunctionCartTransformIds"));
        self.store.staged.function_cart_transforms_dirty = state["stagedState"]
            ["functionCartTransformsDirty"]
            .as_bool()
            .unwrap_or(false);
        self.store.staged.functions_dirty = state["stagedState"]["functionsDirty"]
            .as_bool()
            .unwrap_or(false);
        self.store.staged.function_fulfillment_constraint_rules =
            value_map_from_json(state["stagedState"].get("functionFulfillmentConstraintRules"));
        self.store.staged.function_fulfillment_constraint_rule_order = state["stagedState"]
            .get("functionFulfillmentConstraintRuleOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .function_fulfillment_constraint_rules
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store
            .staged
            .deleted_function_fulfillment_constraint_rule_ids = string_set_from_json(
            state["stagedState"].get("deletedFunctionFulfillmentConstraintRuleIds"),
        );
        self.store
            .staged
            .function_fulfillment_constraint_rules_dirty = state["stagedState"]
            ["functionFulfillmentConstraintRulesDirty"]
            .as_bool()
            .unwrap_or(false);
        self.store.staged.tax_app_configuration = state["stagedState"]
            .get("taxAppConfiguration")
            .filter(|value| value.is_object())
            .cloned();
        self.log_entries = dump["log"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.next_synthetic_id = next_synthetic_id;

        ok_json(json!({ "ok": true, "message": "state restored" }))
    }

    fn advance_order_counters_from_staged_orders(&mut self) {
        let mut next_order_id = self.store.staged.next_order_id.max(1);
        let mut next_refund_id = self.store.staged.next_refund_id.max(1);
        let mut next_refund_line_item_id = self.store.staged.next_refund_line_item_id.max(1);
        let mut next_transaction_id = self.store.staged.order_payment_next_transaction_id.max(3);
        let mut next_order_number = self.store.staged.next_order_number.max(1);

        for (order_id, order) in &self.store.staged.orders {
            advance_counter_past_gid_tail(&mut next_order_id, order_id);
            if let Some(record_id) = order.get("id").and_then(Value::as_str) {
                advance_counter_past_gid_tail(&mut next_order_id, record_id);
            }
            if let Some(number) = order.get("orderNumber").and_then(Value::as_u64) {
                next_order_number = next_order_number.max(number.saturating_add(1));
            } else if let Some(name) = order.get("name").and_then(Value::as_str) {
                advance_order_number_past_order_name(&mut next_order_number, name);
            }
            for transaction in json_records(&order["transactions"]) {
                advance_counter_past_value_id(&mut next_transaction_id, transaction);
            }
            for refund in json_records(&order["refunds"]) {
                advance_counter_past_value_id(&mut next_refund_id, refund);
                for refund_line_item in json_records(&refund["refundLineItems"]) {
                    advance_counter_past_value_id(&mut next_refund_line_item_id, refund_line_item);
                }
                for transaction in json_records(&refund["transactions"]) {
                    advance_counter_past_value_id(&mut next_transaction_id, transaction);
                }
            }
        }

        self.store.staged.next_order_id = next_order_id;
        self.store.staged.next_order_number = next_order_number;
        self.store.staged.next_refund_id = next_refund_id;
        self.store.staged.next_refund_line_item_id = next_refund_line_item_id;
        self.store.staged.order_payment_next_transaction_id = next_transaction_id;
    }

    fn advance_draft_order_counter_from_staged_draft_orders(&mut self) {
        let mut next_draft_order_id = self.store.staged.next_draft_order_id.max(1);
        for (draft_order_id, draft_order) in &self.store.base.draft_orders.records {
            advance_counter_past_gid_tail(&mut next_draft_order_id, draft_order_id);
            if let Some(record_id) = draft_order.get("id").and_then(Value::as_str) {
                advance_counter_past_gid_tail(&mut next_draft_order_id, record_id);
            }
        }
        for (draft_order_id, draft_order) in &self.store.staged.draft_orders {
            advance_counter_past_gid_tail(&mut next_draft_order_id, draft_order_id);
            if let Some(record_id) = draft_order.get("id").and_then(Value::as_str) {
                advance_counter_past_gid_tail(&mut next_draft_order_id, record_id);
            }
        }
        self.store.staged.next_draft_order_id = next_draft_order_id;
    }
}

fn string_set_from_json(value: Option<&Value>) -> BTreeSet<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Some(Value::Object(values)) => values.keys().cloned().collect(),
        _ => BTreeSet::new(),
    }
}

fn value_map_from_json(value: Option<&Value>) -> BTreeMap<String, Value> {
    value
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(id, record)| (id.clone(), record.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn metafield_definition_map_from_json(
    value: Option<&Value>,
) -> BTreeMap<MetafieldDefinitionKey, Value> {
    value
        .and_then(Value::as_object)
        .map(|definitions| {
            definitions
                .iter()
                .filter_map(|(encoded_key, definition)| {
                    let parts = encoded_key.split('\u{1f}').collect::<Vec<_>>();
                    match parts.as_slice() {
                        [owner_type, namespace, key] => Some((
                            metafield_definition_store_key(owner_type, namespace, key),
                            definition.clone(),
                        )),
                        [namespace, key] => {
                            let owner_type = definition
                                .get("ownerType")
                                .and_then(Value::as_str)
                                .unwrap_or("PRODUCT");
                            Some((
                                metafield_definition_store_key(owner_type, namespace, key),
                                definition.clone(),
                            ))
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn metafield_definition_key_set_from_json(
    value: Option<&Value>,
) -> BTreeSet<MetafieldDefinitionKey> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| {
                let owner_type = value.get("ownerType").and_then(Value::as_str)?;
                let namespace = value.get("namespace").and_then(Value::as_str)?;
                let key = value.get("key").and_then(Value::as_str)?;
                Some(metafield_definition_store_key(owner_type, namespace, key))
            })
            .collect(),
        Some(Value::Object(values)) => values
            .keys()
            .filter_map(|encoded_key| {
                let parts = encoded_key.split('\u{1f}').collect::<Vec<_>>();
                match parts.as_slice() {
                    [owner_type, namespace, key] => {
                        Some(metafield_definition_store_key(owner_type, namespace, key))
                    }
                    _ => None,
                }
            })
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn metafield_definition_namespace_set_from_json(
    value: Option<&Value>,
) -> BTreeSet<(String, String)> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let owner_type = value.get("ownerType").and_then(Value::as_str)?;
                    let namespace = value.get("namespace").and_then(Value::as_str)?;
                    Some((owner_type.to_string(), namespace.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn counter_from_json_with_floor(staged_state: &Value, key: &str, floor: u64) -> u64 {
    staged_state
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(floor)
        .max(floor)
}

fn string_map_from_json(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_array_map_from_json(value: Option<&Value>) -> BTreeMap<String, Vec<String>> {
    value
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| (key.clone(), string_array_from_json(value)))
                .collect()
        })
        .unwrap_or_default()
}

fn string_map_map_from_json(value: Option<&Value>) -> BTreeMap<String, BTreeMap<String, String>> {
    value
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(key, value)| (key.clone(), string_map_from_json(Some(value))))
                .collect()
        })
        .unwrap_or_default()
}

fn replace_staged_value_records(
    target: &mut StagedRecords<Value>,
    staged_state: &Value,
    records_key: &str,
    order_key: Option<&str>,
    tombstones_key: Option<&str>,
) {
    let records = value_map_from_json(staged_state.get(records_key));
    let order = order_key
        .and_then(|key| staged_state.get(key))
        .map(string_array_from_json)
        .unwrap_or_default();
    let tombstones = tombstones_key
        .map(|key| string_set_from_json(staged_state.get(key)))
        .unwrap_or_default();
    target.replace_with_order_and_tombstones(records, order, tombstones);
}

fn customer_payment_method_index_from_records(
    records: &BTreeMap<String, Value>,
) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, record) in records {
        if let Some(customer_id) = record["customer"]["id"].as_str() {
            index
                .entry(customer_id.to_string())
                .or_default()
                .push(id.clone());
        }
    }
    index
}

fn advance_counter_past_value_id(counter: &mut u64, value: &Value) {
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        advance_counter_past_gid_tail(counter, id);
    }
}

fn advance_counter_past_gid_tail(counter: &mut u64, id: &str) {
    if let Ok(numeric) = resource_id_tail(id).parse::<u64>() {
        *counter = (*counter).max(numeric.saturating_add(1));
    }
}

fn advance_order_number_past_order_name(counter: &mut u64, name: &str) {
    let Some(number) = name
        .strip_prefix('#')
        .and_then(|suffix| suffix.parse::<u64>().ok())
    else {
        return;
    };
    *counter = (*counter).max(number.saturating_add(1));
}

fn json_records(value: &Value) -> Vec<&Value> {
    let mut records = Vec::new();
    if let Some(array) = value.as_array() {
        records.extend(array.iter());
    }
    if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
        records.extend(nodes.iter());
    }
    if let Some(edges) = value.get("edges").and_then(Value::as_array) {
        records.extend(edges.iter().filter_map(|edge| edge.get("node")));
    }
    if records.is_empty() && value.get("id").and_then(Value::as_str).is_some() {
        records.push(value);
    }
    records
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

fn inventory_level_order_from_json(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let inventory_item_id = entry.get("inventoryItemId")?.as_str()?.to_string();
            let location_id = entry.get("locationId")?.as_str()?.to_string();
            Some((inventory_item_id, location_id))
        })
        .collect()
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

fn inventory_levels_from_json(value: &Value) -> BTreeMap<(String, String), BTreeMap<String, i64>> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let inventory_item_id = entry.get("inventoryItemId")?.as_str()?.to_string();
            let location_id = entry.get("locationId")?.as_str()?.to_string();
            let quantities = entry
                .get("quantities")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .filter_map(|(name, quantity)| {
                            quantity.as_i64().map(|quantity| (name.clone(), quantity))
                        })
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            Some(((inventory_item_id, location_id), quantities))
        })
        .collect()
}

fn inventory_level_ids_from_json(value: &Value) -> BTreeMap<(String, String), String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                (
                    entry.get("inventoryItemId")?.as_str()?.to_string(),
                    entry.get("locationId")?.as_str()?.to_string(),
                ),
                entry.get("id")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn inactive_inventory_levels_from_json(value: &Value) -> BTreeSet<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                entry.get("inventoryItemId")?.as_str()?.to_string(),
                entry.get("locationId")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn inventory_quantity_updated_at_from_json(
    value: &Value,
) -> BTreeMap<(String, String, String), String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                (
                    entry.get("inventoryItemId")?.as_str()?.to_string(),
                    entry.get("locationId")?.as_str()?.to_string(),
                    entry.get("name")?.as_str()?.to_string(),
                ),
                entry.get("updatedAt")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

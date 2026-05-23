use super::*;

impl DraftProxy {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            log_entries: Vec::new(),
            registry: default_registry(),
            base_products: BTreeMap::new(),
            staged_products: BTreeMap::new(),
            staged_product_search_tags: BTreeMap::new(),
            staged_deleted_product_ids: BTreeSet::new(),
            staged_saved_searches: BTreeMap::new(),
            staged_deleted_saved_search_ids: BTreeSet::new(),
            staged_shipping_packages: BTreeMap::new(),
            staged_deleted_shipping_package_ids: BTreeSet::new(),
            staged_customers: BTreeMap::new(),
            staged_deleted_customer_ids: BTreeSet::new(),
            staged_customer_orders: BTreeMap::new(),
            staged_carrier_services: BTreeMap::new(),
            staged_deleted_carrier_service_ids: BTreeSet::new(),
            staged_app_subscriptions: BTreeMap::new(),
            staged_app_one_time_purchases: BTreeMap::new(),
            revoked_app_access_scopes: BTreeSet::new(),
            app_uninstalled: false,
            staged_delegate_access_tokens: BTreeMap::new(),
            staged_customer_segment_member_queries: BTreeMap::new(),
            staged_fulfillment_services: BTreeMap::new(),
            staged_fulfillment_service_locations: BTreeMap::new(),
            staged_deleted_fulfillment_service_ids: BTreeSet::new(),
            staged_deleted_fulfillment_service_location_ids: BTreeSet::new(),
            staged_segments: BTreeMap::new(),
            staged_collections: BTreeMap::new(),
            staged_fulfillment_order_deadlines: BTreeMap::new(),
            staged_bulk_operations: BTreeMap::new(),
            staged_timestamp_discounts: BTreeMap::new(),
            staged_gift_cards: BTreeMap::new(),
            staged_markets: BTreeMap::new(),
            staged_catalogs: BTreeMap::new(),
            staged_price_lists: BTreeMap::new(),
            staged_web_presences: BTreeMap::new(),
            staged_shop_locales: BTreeMap::new(),
            staged_localization_translations: Vec::new(),
            staged_marketing_activities: BTreeMap::new(),
            staged_deleted_marketing_activity_ids: BTreeSet::new(),
            staged_marketing_delete_all_external: false,
            staged_webhook_subscriptions: BTreeMap::new(),
            staged_b2b_companies: BTreeMap::new(),
            staged_b2b_locations: BTreeMap::new(),
            next_b2b_company_id: 1,
            staged_inventory_levels: BTreeMap::new(),
            staged_metaobjects: BTreeMap::new(),
            staged_deleted_metaobject_ids: BTreeSet::new(),
            staged_app_metafields: BTreeMap::new(),
            staged_owner_metafields: BTreeMap::new(),
            staged_metafield_definitions: BTreeMap::new(),
            staged_media_files: BTreeMap::new(),
            staged_deleted_media_file_ids: BTreeSet::new(),
            staged_online_store_integrations: BTreeMap::new(),
            staged_product_set_updated: false,
            staged_product_option_fixture: None,
            staged_product_metafields_fixture: None,
            staged_product_delete_operations: BTreeMap::new(),
            staged_selling_plan_group_downstream_step: 0,
            staged_return_status: None,
            staged_recorded_return_statuses: BTreeMap::new(),
            staged_mandate_payment_keys: BTreeSet::new(),
            staged_payment_terms_ids: BTreeSet::new(),
            staged_payment_reminder_schedule_ids: BTreeSet::new(),
            staged_payment_customizations: BTreeMap::new(),
            staged_draft_orders: BTreeMap::new(),
            next_draft_order_id: 1,
            staged_draft_order_tags: BTreeMap::new(),
            next_draft_order_bulk_tag_job_id: 1,
            staged_draft_order_complete_gateway_create_count: 0,
            staged_order_customer_orders: BTreeMap::new(),
            staged_order_customer_cancelled_ids: BTreeSet::new(),
            staged_order_customer_b2b_order_ids: BTreeSet::new(),
            staged_order_customer_contact_customer_ids: BTreeSet::new(),
            next_order_customer_order_id: 1,
            staged_order_payment_transaction_state: None,
            staged_order_edit_existing_mode: None,
            staged_function_validation: None,
            staged_function_cart_transform: None,
            staged_code_basic_lifecycle_status: None,
            staged_free_shipping_code_status: None,
            staged_free_shipping_automatic_status: None,
            staged_redeem_code_bulk_live_added: false,
            staged_redeem_code_bulk_live_deleted_seed: false,
            backup_region: backup_region_country("CA"),
            next_synthetic_id: 1,
            commit_transport: Arc::new(default_commit_transport),
            upstream_transport: Arc::new(default_upstream_transport),
        }
    }

    pub fn with_registry(mut self, registry: Vec<OperationRegistryEntry>) -> Self {
        self.registry = registry;
        self
    }

    pub fn with_base_products(mut self, products: Vec<ProductRecord>) -> Self {
        self.base_products = products
            .into_iter()
            .map(|product| (product.id.clone(), product))
            .collect();
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
        self.upstream_transport = Arc::new(transport);
        self
    }

    pub fn process_request(&mut self, request: Request) -> Response {
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
                self.staged_products.clear();
                self.staged_product_search_tags.clear();
                self.staged_deleted_product_ids.clear();
                self.staged_saved_searches.clear();
                self.staged_deleted_saved_search_ids.clear();
                self.staged_shipping_packages.clear();
                self.staged_deleted_shipping_package_ids.clear();
                self.staged_customers.clear();
                self.staged_deleted_customer_ids.clear();
                self.staged_customer_orders.clear();
                self.staged_carrier_services.clear();
                self.staged_deleted_carrier_service_ids.clear();
                self.staged_app_subscriptions.clear();
                self.staged_app_one_time_purchases.clear();
                self.revoked_app_access_scopes.clear();
                self.app_uninstalled = false;
                self.staged_delegate_access_tokens.clear();
                self.staged_customer_segment_member_queries.clear();
                self.staged_fulfillment_services.clear();
                self.staged_fulfillment_service_locations.clear();
                self.staged_deleted_fulfillment_service_ids.clear();
                self.staged_deleted_fulfillment_service_location_ids.clear();
                self.staged_segments.clear();
                self.staged_collections.clear();
                self.staged_fulfillment_order_deadlines.clear();
                self.staged_bulk_operations.clear();
                self.staged_timestamp_discounts.clear();
                self.staged_gift_cards.clear();
                self.staged_markets.clear();
                self.staged_catalogs.clear();
                self.staged_price_lists.clear();
                self.staged_web_presences.clear();
                self.staged_shop_locales.clear();
                self.staged_localization_translations.clear();
                self.staged_marketing_activities.clear();
                self.staged_deleted_marketing_activity_ids.clear();
                self.staged_marketing_delete_all_external = false;
                self.staged_webhook_subscriptions.clear();
                self.staged_b2b_companies.clear();
                self.staged_b2b_locations.clear();
                self.next_b2b_company_id = 1;
                self.staged_inventory_levels.clear();
                self.staged_metaobjects.clear();
                self.staged_deleted_metaobject_ids.clear();
                self.staged_app_metafields.clear();
                self.staged_owner_metafields.clear();
                self.staged_metafield_definitions.clear();
                self.staged_media_files.clear();
                self.staged_deleted_media_file_ids.clear();
                self.staged_product_set_updated = false;
                self.staged_product_option_fixture = None;
                self.staged_product_delete_operations.clear();
                self.staged_selling_plan_group_downstream_step = 0;
                self.staged_return_status = None;
                self.staged_recorded_return_statuses.clear();
                self.staged_mandate_payment_keys.clear();
                self.staged_payment_terms_ids.clear();
                self.staged_payment_reminder_schedule_ids.clear();
                self.staged_payment_customizations.clear();
                self.staged_draft_orders.clear();
                self.next_draft_order_id = 1;
                self.staged_draft_order_tags.clear();
                self.next_draft_order_bulk_tag_job_id = 1;
                self.staged_draft_order_complete_gateway_create_count = 0;
                self.staged_order_customer_orders.clear();
                self.staged_order_customer_cancelled_ids.clear();
                self.staged_order_customer_b2b_order_ids.clear();
                self.staged_order_customer_contact_customer_ids.clear();
                self.next_order_customer_order_id = 1;
                self.staged_order_payment_transaction_state = None;
                self.staged_order_edit_existing_mode = None;
                self.staged_function_validation = None;
                self.staged_function_cart_transform = None;
                self.staged_code_basic_lifecycle_status = None;
                self.staged_free_shipping_code_status = None;
                self.staged_free_shipping_automatic_status = None;
                self.staged_redeem_code_bulk_live_added = false;
                self.staged_redeem_code_bulk_live_deleted_seed = false;
                self.backup_region = backup_region_country("CA");
                self.next_synthetic_id = 1;
                ok_json(json!({ "ok": true, "message": "state reset" }))
            }
            Route::MetaDump => self.dump_state(&request),
            Route::MetaRestore => self.restore_state(&request),
            Route::MetaCommit => self.commit_staged_mutations(&request),
            Route::Graphql => self.dispatch_graphql(&request),
            Route::NotFound => json_error(404, "Not found"),
            Route::MethodNotAllowed => json_error(405, "Method not allowed"),
        }
    }

    pub fn get_config_snapshot(&self) -> Value {
        self.config_snapshot()
    }

    pub fn get_log_snapshot(&self) -> Value {
        json!({ "entries": self.log_entries })
    }

    pub fn get_state_snapshot(&self) -> Value {
        self.state_snapshot()
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
        let mut snapshot = json!({
            "baseState": {
                "products": product_state_map_json(&self.base_products),
                "savedSearches": {}
            },
            "stagedState": {
                "products": product_state_map_json(&self.staged_products),
                "deletedProductIds": self.staged_deleted_product_ids.iter().cloned().collect::<Vec<_>>(),
                "savedSearches": saved_search_state_map_json(&self.staged_saved_searches),
                "shippingPackages": self.staged_shipping_packages.clone(),
                "deletedShippingPackageIds": self.staged_deleted_shipping_package_ids.iter().map(|id| (id.clone(), json!(true))).collect::<serde_json::Map<_, _>>(),
                "delegatedAccessTokens": self.staged_delegate_access_tokens.clone(),
                "customers": self.staged_customers.clone(),
                "deletedCustomerIds": self.staged_deleted_customer_ids.iter().cloned().collect::<Vec<_>>(),
                "customerOrders": self.staged_customer_orders.clone()
            }
        });
        if !self.staged_draft_orders.is_empty() {
            snapshot["stagedState"]["draftOrders"] = Value::Object(
                self.staged_draft_orders
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
                json!(self.staged_draft_orders.keys().cloned().collect::<Vec<_>>());
        }
        snapshot
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
            "state.baseState.savedSearches",
            "state.stagedState",
            "state.stagedState.products",
            "state.stagedState.deletedProductIds",
            "state.stagedState.savedSearches",
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

        self.base_products = product_state_map_from_json(&state["baseState"]["products"]);
        self.staged_products = product_state_map_from_json(&state["stagedState"]["products"]);
        self.staged_deleted_product_ids = state["stagedState"]["deletedProductIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.staged_saved_searches =
            saved_search_state_map_from_json(&state["stagedState"]["savedSearches"]);
        self.staged_shipping_packages = state["stagedState"]["shippingPackages"]
            .as_object()
            .map(|packages| {
                packages
                    .iter()
                    .map(|(id, package)| (id.clone(), package.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.staged_deleted_shipping_package_ids = state["stagedState"]
            ["deletedShippingPackageIds"]
            .as_object()
            .map(|ids| ids.keys().cloned().collect())
            .unwrap_or_default();
        self.staged_customers = state["stagedState"]["customers"]
            .as_object()
            .map(|customers| {
                customers
                    .iter()
                    .map(|(id, customer)| (id.clone(), customer.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.staged_deleted_customer_ids = state["stagedState"]["deletedCustomerIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.staged_customer_orders = state["stagedState"]["customerOrders"]
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
        self.log_entries = dump["log"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.next_synthetic_id = next_synthetic_id;

        ok_json(json!({ "ok": true, "message": "state restored" }))
    }

    pub(in crate::proxy) fn commit_staged_mutations(
        &mut self,
        commit_request: &Request,
    ) -> Response {
        let transport = Arc::clone(&self.commit_transport);
        let mut committed = 0usize;
        let mut failed = 0usize;

        for index in 0..self.log_entries.len() {
            if self.log_entries[index].get("status") != Some(&json!("staged")) {
                continue;
            }
            let log_id = self.log_entries[index]
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let path = self.log_entries[index]
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("/admin/api/2026-04/graphql.json")
                .to_string();
            let query = self.log_entries[index]
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let variables = self.log_entries[index]
                .get("variables")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let replay = Request {
                method: "POST".to_string(),
                path,
                headers: commit_request.headers.clone(),
                body: json!({ "query": query, "variables": variables }).to_string(),
            };
            let outcome = transport(replay);
            if outcome.status >= 400 || outcome.body.get("errors").is_some() {
                failed += 1;
                set_log_status(&mut self.log_entries[index], "failed");
                return Response {
                    status: 502,
                    headers: BTreeMap::new(),
                    body: json!({
                        "ok": false,
                        "committed": committed,
                        "failed": failed,
                        "error": format!("Upstream commit failed for {log_id} with status {}", outcome.status)
                    }),
                };
            }
            committed += 1;
            set_log_status(&mut self.log_entries[index], "committed");
        }

        ok_json(json!({ "ok": true, "committed": committed, "failed": failed }))
    }
}

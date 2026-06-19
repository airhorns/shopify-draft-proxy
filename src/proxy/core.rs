use super::*;

impl DraftProxy {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            log_entries: Vec::new(),
            registry: default_registry(),
            store: Store::with_default_baseline(),
            next_synthetic_id: 1,
            shop_sells_subscriptions: None,
            commit_transport: Arc::new(default_commit_transport),
            upstream_transport: Arc::new(default_upstream_transport),
        }
    }

    pub fn with_registry(mut self, registry: Vec<OperationRegistryEntry>) -> Self {
        self.registry = registry;
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
                self.store.clear_staged();
                self.next_synthetic_id = 1;
                self.shop_sells_subscriptions = None;
                ok_json(json!({ "ok": true, "message": "state reset" }))
            }
            Route::MetaSeed => self.seed_state(&request),
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
            .deleted_shipping_package_ids
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
        let mut snapshot = json!({
            "baseState": {
                "products": product_state_map_json(&self.store.base.products.records),
                "productOrder": self.store.base.products.order,
                "productVariants": product_variant_state_map_json(&self.store.base.product_variants.records),
                "productVariantOrder": self.store.base.product_variants.order,
                "savedSearches": saved_search_state_map_json(&self.store.base.saved_searches.records),
                "savedSearchOrder": self.store.base.saved_searches.order,
                "shopPolicies": shop_policy_state_map_json(&self.store.base.shop_policies.records),
                "shopPolicyOrder": self.store.base.shop_policies.order,
                "shop": self.store.base.shop.clone(),
                "publicationIds": self.store.base.publication_ids.iter().cloned().collect::<Vec<_>>(),
                "publicationCount": self.store.base.publication_count,
                "availableLocales": available_locales,
                "shopLocales": self.store.base.shop_locales.clone(),
                "localizationProductIds": self.store.base.localization_product_ids.iter().cloned().collect::<Vec<_>>()
            },
            "stagedState": {
                "products": product_state_map_json(&self.store.staged.products.records),
                "productOrder": self.store.staged.products.order,
                "deletedProductIds": self.store.staged.products.tombstones.iter().cloned().collect::<Vec<_>>(),
                "productVariants": product_variant_state_map_json(&self.store.staged.product_variants.records),
                "productVariantOrder": self.store.staged.product_variants.order,
                "deletedProductVariantIds": self.store.staged.product_variants.tombstones.iter().cloned().collect::<Vec<_>>(),
                "collections": self.store.staged.collections.clone(),
                "deletedCollectionIds": self.store.staged.deleted_collection_ids.iter().cloned().collect::<Vec<_>>(),
                "collectionJobs": self.store.staged.collection_jobs.clone(),
                "savedSearches": saved_search_state_map_json(&self.store.staged.saved_searches.records),
                "savedSearchOrder": self.store.staged.saved_searches.order,
                "deletedSavedSearchIds": self.store.staged.saved_searches.tombstones.iter().cloned().collect::<Vec<_>>(),
                "shopPolicies": shop_policy_state_map_json(&self.store.staged.shop_policies.records),
                "shopPolicyOrder": self.store.staged.shop_policies.order,
                "deletedShopPolicyIds": self.store.staged.shop_policies.tombstones.iter().cloned().collect::<Vec<_>>(),
                "shippingPackages": self.store.staged.shipping_packages.clone(),
                "deletedShippingPackageIds": deleted_shipping_package_ids,
                "delegatedAccessTokens": self.store.staged.delegate_access_tokens.clone(),
                "customers": self.store.staged.customers.clone(),
                "deletedCustomerIds": self.store.staged.deleted_customer_ids.iter().cloned().collect::<Vec<_>>(),
                "customerAddresses": self.store.staged.customer_addresses.clone(),
                "customerAddressOrder": self.store.staged.customer_address_order.clone(),
                "customerAddressOwners": self.store.staged.customer_address_owners.clone(),
                "customerOrders": self.store.staged.customer_orders.clone(),
                "mergedCustomerIds": self.store.staged.merged_customer_ids.clone(),
                "customerMergeRequests": self.store.staged.customer_merge_requests.clone(),
                "customerDataErasureRequests": self.store.staged.customer_data_erasure_requests.clone(),
                "customersCountBase": self.store.staged.customers_count_base,
                "storeCreditAccounts": self.store.staged.store_credit_accounts.clone(),
                "storeCreditAccountOrder": self.store.staged.store_credit_account_order.clone(),
                "storeCreditTransactions": self.store.staged.store_credit_transactions.clone(),
                "storeCreditTransactionOrder": self.store.staged.store_credit_transaction_order.clone(),
                "nextStoreCreditAccountId": self.store.staged.next_store_credit_account_id,
                "nextStoreCreditTransactionId": self.store.staged.next_store_credit_transaction_id,
                "giftCards": self.store.staged.gift_cards.clone(),
                "taggableResources": self.store.staged.taggable_resources.clone(),
                "orders": self.store.staged.orders.clone(),
                "deletedOrderIds": self.store.staged.deleted_order_ids.iter().cloned().collect::<Vec<_>>(),
                "nextDraftOrderId": self.store.staged.next_draft_order_id,
                "draftOrderTags": self.store.staged.draft_order_tags.clone(),
                "returns": self.store.staged.returns.clone(),
                "returnsByOrder": self.store.staged.returns_by_order.clone(),
               "reverseDeliveries": self.store.staged.reverse_deliveries.clone(),
                "reverseFulfillmentOrders": self.store.staged.reverse_fulfillment_orders.clone(),
                "observedShippingLocations": self.store.staged.observed_shipping_locations.clone(),
                "observedShippingLocationOrder": self.store.staged.observed_shipping_location_order.clone(),
                "locations": self.store.staged.locations.clone(),
                "locationOrder": self.store.staged.location_order.clone(),
                "publicationIds": self.store.staged.publication_ids.iter().cloned().collect::<Vec<_>>(),
                "createdPublicationIds": self.store.staged.created_publication_ids.iter().cloned().collect::<Vec<_>>(),
                "publications": self.store.staged.publications.clone(),
                "resourcePublications": self.store.staged.resource_publications.iter().map(|(resource, pubs)| {
                    (resource.clone(), pubs.iter().cloned().collect::<Vec<String>>())
                }).collect::<std::collections::BTreeMap<String, Vec<String>>>(),
                "locationLimitReached": self.store.staged.location_limit_reached,
                "discounts": self.store.staged.discounts.clone(),
                "discountCodeIndex": self.store.staged.discount_code_index.clone(),
                "deletedDiscountIds": self.store.staged.deleted_discount_ids.iter().cloned().collect::<Vec<_>>(),
                "discountRedeemCodeBulkCreations": self.store.staged.discount_redeem_code_bulk_creations.clone(),
                "ownerMetafields": self.store.staged.owner_metafields.clone(),
                "deletedOwnerMetafields": deleted_owner_metafields
            }
        });
        if !self.store.staged.product_operations.is_empty() {
            snapshot["stagedState"]["productOperations"] =
                json!(self.store.staged.product_operations);
        }
        if self.has_staged_b2b_state() {
            snapshot["stagedState"]["b2bCompanies"] =
                json!(self.store.staged.b2b_companies.clone());
            snapshot["stagedState"]["b2bLocations"] =
                json!(self.store.staged.b2b_locations.clone());
            snapshot["stagedState"]["b2bLocationOrder"] =
                json!(self.store.staged.b2b_location_order.clone());
            snapshot["stagedState"]["b2bContacts"] = json!(self.store.staged.b2b_contacts.clone());
            snapshot["stagedState"]["b2bContactRoles"] =
                json!(self.store.staged.b2b_contact_roles.clone());
            snapshot["stagedState"]["b2bRoleAssignments"] =
                json!(self.store.staged.b2b_role_assignments.clone());
            snapshot["stagedState"]["b2bStaffAssignments"] =
                json!(self.store.staged.b2b_staff_assignments.clone());
            snapshot["stagedState"]["deletedB2bContactRoleAssignmentIds"] = json!(self
                .store
                .staged
                .deleted_b2b_contact_role_assignment_ids
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
        if !self.store.staged.inventory_level_cursors.is_empty() {
            snapshot["stagedState"]["inventoryLevelCursors"] =
                serde_json::to_value(&self.store.staged.inventory_level_cursors)
                    .unwrap_or_default();
        }
        if !self.store.staged.inactive_inventory_levels.is_empty() {
            snapshot["stagedState"]["inactiveInventoryLevels"] =
                inactive_inventory_levels_json(&self.store.staged.inactive_inventory_levels);
        }
        if !self.store.staged.inventory_transfers.is_empty() {
            snapshot["stagedState"]["inventoryTransfers"] =
                serde_json::to_value(&self.store.staged.inventory_transfers).unwrap_or_default();
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
        if !self.store.staged.metaobject_definitions.is_empty() {
            snapshot["stagedState"]["metaobjectDefinitions"] =
                json!(self.store.staged.metaobject_definitions);
        }
        if !self
            .store
            .staged
            .deleted_metaobject_definition_ids
            .is_empty()
        {
            snapshot["stagedState"]["deletedMetaobjectDefinitionIds"] = json!(self
                .store
                .staged
                .deleted_metaobject_definition_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
        }
        if !self.store.staged.metaobjects.is_empty() {
            snapshot["stagedState"]["metaobjects"] = json!(self.store.staged.metaobjects);
        }
        if !self.store.staged.deleted_metaobject_ids.is_empty() {
            snapshot["stagedState"]["deletedMetaobjectIds"] = json!(self
                .store
                .staged
                .deleted_metaobject_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>());
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
                    .map(|((namespace, key), definition)| {
                        (format!("{namespace}\u{1f}{key}"), definition.clone())
                    })
                    .collect::<serde_json::Map<_, _>>(),
            );
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
            snapshot["stagedState"]["draftOrderOrder"] = json!(self
                .store
                .staged
                .draft_orders
                .keys()
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
        if !self.store.staged.catalogs.is_empty() {
            snapshot["stagedState"]["catalogs"] = json!(self.store.staged.catalogs.clone());
        }
        if !self.store.staged.price_lists.is_empty() {
            snapshot["stagedState"]["priceLists"] = json!(self.store.staged.price_lists.clone());
        }
        if !self.store.staged.web_presences.is_empty() {
            snapshot["stagedState"]["webPresences"] =
                json!(self.store.staged.web_presences.clone());
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
        if self.store.staged.functions_dirty {
            snapshot["stagedState"]["functionsDirty"] = json!(true);
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
            || !self.store.staged.b2b_locations.is_empty()
            || !self.store.staged.b2b_location_order.is_empty()
            || !self.store.staged.b2b_contacts.is_empty()
            || !self.store.staged.b2b_contact_roles.is_empty()
            || !self.store.staged.b2b_role_assignments.is_empty()
            || !self.store.staged.b2b_staff_assignments.is_empty()
    }

    /// Seed pre-existing entity records that a scenario's captured setup created
    /// before the requests under test. The parity harness replays a fixture's
    /// `seedCustomers` / `seedSegments` / `seedProducts` / `seedProductVariants` /
    /// `seedCollections` / `seedDiscounts` declarations here so local replay can
    /// resolve references the live store already held (buyer-context display names,
    /// segment names, and item-entitlement / duplicate-code referential integrity)
    /// the same way live Shopify resolves them from already-existing entities. Each
    /// record is upserted by its captured (live) id so later inputs that reference
    /// those ids match. Seeding products/variants/collections also makes the proxy
    /// *authoritative* for those entity types, which is what lets discount item-ref
    /// validation reject ids the store has never seen.
    pub(in crate::proxy) fn seed_state(&mut self, request: &Request) -> Response {
        let Ok(body) = serde_json::from_str::<Value>(&request.body) else {
            return json_error(400, "Invalid seed payload JSON");
        };
        let mut seeded_customers = 0_usize;
        let mut seeded_segments = 0_usize;
        let mut seeded_products = 0_usize;
        let mut seeded_variants = 0_usize;
        let mut seeded_collections = 0_usize;
        let mut seeded_discounts = 0_usize;
        if let Some(customers) = body.get("customers").and_then(Value::as_array) {
            for customer in customers {
                if let Some(id) = customer.get("id").and_then(Value::as_str) {
                    self.store
                        .staged
                        .customers
                        .insert(id.to_string(), customer.clone());
                    seeded_customers += 1;
                }
            }
        }
        // A scenario can seed the live shop's total customer count so the
        // `customersCount` read root reports the store-specific baseline (which
        // can't be reconstructed from the staged customers) and tracks deletions
        // generically as `base - deletions`.
        if let Some(count) = body.get("customersCount").and_then(Value::as_u64) {
            self.store.staged.customers_count_base = Some(count);
        }
        if let Some(segments) = body.get("segments").and_then(Value::as_array) {
            for segment in segments {
                if let Some(id) = segment.get("id").and_then(Value::as_str) {
                    self.store
                        .staged
                        .segments
                        .insert(id.to_string(), segment.clone());
                    seeded_segments += 1;
                }
            }
        }
        // A scenario can seed recorded segment-catalog connection baselines keyed by
        // root field name (`segments` / `segmentsCount` / `segmentFilters` /
        // `segmentFilterSuggestions` / `segmentValueSuggestions` /
        // `segmentMigrations`). The read resolver projects the requested selection
        // over them; this preserves the live cursors/pageInfo that the catalog roots
        // can't synthesize from arbitrary store state.
        if let Some(catalog) = body.get("segmentCatalog").and_then(Value::as_object) {
            for (key, value) in catalog {
                self.store
                    .staged
                    .segment_catalog
                    .insert(key.clone(), value.clone());
            }
        }
        // A scenario can seed recorded top-level `collections(query:, sortKey:)`
        // connection baselines keyed by GraphQL alias (response key). The catalog
        // read resolver projects the requested selection over them; this preserves
        // the live search-index cursors/pageInfo (title case folding, SQL-datetime
        // sort keys) that the collections list root cannot synthesize from store
        // state. Mirrors `segmentCatalog`.
        if let Some(catalog) = body.get("collectionCatalog").and_then(Value::as_object) {
            for (key, value) in catalog {
                self.store
                    .staged
                    .collection_catalog
                    .insert(key.clone(), value.clone());
            }
        }
        if let Some(products) = body.get("products").and_then(Value::as_array) {
            for product in products {
                if product.get("id").and_then(Value::as_str).is_some() {
                    self.store.stage_observed_product_json(product);
                    seeded_products += 1;
                }
            }
        }
        if let Some(variants) = body.get("productVariants").and_then(Value::as_array) {
            for variant in variants {
                // A variant state record requires an `inventoryItem` with an id; a
                // seed declaration only needs to assert existence, so synthesize a
                // deterministic inventory-item id from the variant id when absent.
                let mut variant = variant.clone();
                if variant.get("inventoryItem").is_none() {
                    if let Some(id) = variant.get("id").and_then(Value::as_str) {
                        let numeric = id.rsplit('/').next().unwrap_or(id);
                        variant["inventoryItem"] = json!({
                            "id": format!("gid://shopify/InventoryItem/{numeric}")
                        });
                    }
                }
                if let Some(record) = product_variant_state_from_json(&variant) {
                    self.store.stage_product_variant(record);
                    seeded_variants += 1;
                }
            }
        }
        if let Some(collections) = body.get("collections").and_then(Value::as_array) {
            for collection in collections {
                if collection.get("id").and_then(Value::as_str).is_some() {
                    self.store.stage_collection(collection.clone());
                    seeded_collections += 1;
                }
            }
        }
        if let Some(discounts) = body.get("discounts").and_then(Value::as_array) {
            for discount in discounts {
                if self.seed_discount_record(discount) {
                    seeded_discounts += 1;
                }
            }
        }
        // A scenario can seed the store's base/default publications (and any other
        // pre-existing ones) as `seedPublications`, plus per-resource publication
        // membership via `publicationIds` on the seeded products/collections. This
        // drives the local publication/channel roots and the
        // publishedOnPublication/count fields without replaying the upstream
        // hydration. Empty for every scenario that does not seed publications,
        // leaving the existing passthrough behavior for those roots untouched.
        if let Some(publications) = body.get("publications").and_then(Value::as_array) {
            for publication in publications {
                let Some(id) = publication.get("id").and_then(Value::as_str) else {
                    continue;
                };
                // Accept a full recorded record (carrying `channel`) verbatim, or
                // synthesize the canonical record from id/name/autoPublish.
                let record = if publication.get("channel").is_some() {
                    publication.clone()
                } else {
                    let name = publication.get("name").and_then(Value::as_str).unwrap_or("");
                    let auto_publish = publication
                        .get("autoPublish")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    publication_record_json(id, name, auto_publish)
                };
                self.store.staged.publications.insert(id.to_string(), record);
            }
        }
        // Seed per-resource publication membership from the `publicationIds`
        // declared on the seeded products/collections.
        for key in ["products", "collections"] {
            if let Some(resources) = body.get(key).and_then(Value::as_array) {
                for resource in resources {
                    let Some(resource_id) = resource.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(ids) = resource.get("publicationIds").and_then(Value::as_array) {
                        let set = self
                            .store
                            .staged
                            .resource_publications
                            .entry(resource_id.to_string())
                            .or_default();
                        for id in ids {
                            if let Some(id) = id.as_str() {
                                set.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
        // Seed precondition orders captured by a scenario's setup so lifecycle
        // mutations (e.g. draftOrderCreateFromOrder) and order reads can resolve
        // entities the recorded setup created before the request under test.
        let mut seeded_orders = 0_usize;
        if let Some(orders) = body.get("orders").and_then(Value::as_array) {
            for order in orders {
                if let Some(id) = order.get("id").and_then(Value::as_str) {
                    self.store
                        .staged
                        .orders
                        .insert(id.to_string(), order.clone());
                    seeded_orders += 1;
                }
            }
        }
        // A scenario can seed per-customer order connections (customer id -> recorded
        // order nodes, each optionally carrying an opaque `__cursor`) so customer
        // reads and order-transferring merges resolve a customer's orders from store
        // state. The recorded cursors cannot be reconstructed locally, so seeding
        // them lets downstream reads observe the live connection cursors verbatim.
        if let Some(customer_orders) = body.get("customerOrders").and_then(Value::as_object) {
            for (customer_id, orders) in customer_orders {
                if let Some(orders) = orders.as_array() {
                    self.store
                        .staged
                        .customer_orders
                        .entry(customer_id.clone())
                        .or_default()
                        .extend(orders.iter().cloned());
                }
            }
        }
        // Seed precondition draft orders (full recorded projections). The draft
        // record carries its own GraphQL line-item connection; mirror it into the
        // internal `__draftProxyLineItems` shadow the create/update path maintains
        // so totals recalculation and subsequent mutations operate on a complete
        // record, exactly as a locally-created draft would.
        let mut seeded_draft_orders = 0_usize;
        if let Some(draft_orders) = body.get("draftOrders").and_then(Value::as_array) {
            for draft in draft_orders {
                let Some(id) = draft.get("id").and_then(Value::as_str).map(str::to_string) else {
                    continue;
                };
                let mut record = draft.clone();
                if record.get("__draftProxyLineItems").is_none() {
                    if let Some(nodes) = record["lineItems"]["nodes"].as_array() {
                        record["__draftProxyLineItems"] = Value::Array(nodes.clone());
                    }
                }
                self.store
                    .staged
                    .draft_orders
                    .insert(id.clone(), record);
                self.sync_draft_order_tags(&id);
                // Advance the synthetic draft sequence past any seeded numeric id so a
                // later create in the same scenario cannot collide with a precondition.
                if let Some(numeric) = id
                    .rsplit('/')
                    .next()
                    .and_then(|tail| tail.split('?').next())
                    .and_then(|tail| tail.parse::<u64>().ok())
                {
                    if numeric >= self.store.staged.next_draft_order_id {
                        self.store.staged.next_draft_order_id = numeric + 1;
                    }
                }
                seeded_draft_orders += 1;
            }
        }
        // Order-edit variant catalog: the store-state product variants an
        // `orderEditAddVariant` resolves against. Stored as a map keyed by
        // variant id so the edit engine can build an added calculated line item
        // (title / sku / unit price) from store state rather than echoing the
        // recorded response.
        let mut seeded_order_edit_variants = 0_usize;
        if let Some(variants) = body.get("orderEditVariants").and_then(Value::as_array) {
            let catalog = self
                .store
                .staged
                .order_edit_variant_catalog
                .as_object_mut()
                .expect("order_edit_variant_catalog is always an object");
            for variant in variants {
                if let Some(id) = variant.get("id").and_then(Value::as_str) {
                    catalog.insert(id.to_string(), variant.clone());
                    seeded_order_edit_variants += 1;
                }
            }
        }
        if let Some(author) = body.get("orderEditAuthor").and_then(Value::as_str) {
            self.store.staged.order_edit_author = Some(author.to_string());
        }
        // Opaque InventoryLevel connection cursors (level gid -> cursor) recovered
        // from a capture's recorded inventory-level edges. These Relay pagination
        // tokens encode Shopify's internal row ids and can't be synthesized from
        // store state, so seeding them lets the inventory-level connection renderer
        // replay the live cursors verbatim when projecting edges/pageInfo.
        if let Some(cursors) = body.get("inventoryLevelCursors").and_then(Value::as_object) {
            for (level_id, cursor) in cursors {
                if let Some(cursor) = cursor.as_str() {
                    self.store
                        .staged
                        .inventory_level_cursors
                        .insert(level_id.clone(), cursor.to_string());
                }
            }
        }
        ok_json(json!({
            "ok": true,
            "seededCustomers": seeded_customers,
            "seededSegments": seeded_segments,
            "seededProducts": seeded_products,
            "seededProductVariants": seeded_variants,
            "seededCollections": seeded_collections,
            "seededDiscounts": seeded_discounts,
            "seededOrders": seeded_orders,
            "seededDraftOrders": seeded_draft_orders,
            "seededOrderEditVariants": seeded_order_edit_variants
        }))
    }

    /// Stage a pre-existing discount from a node-shaped seed declaration. Only the
    /// id and redeem codes are needed for referential checks (duplicate-code TAKEN),
    /// so this normalizes the captured node — whether codes live under
    /// `discount.codes.nodes[]`, a flat `codes[]`, or a single `code` — into the
    /// proxy's internal `{ id, codes: [{ code }] }` record shape and registers each
    /// code in the uppercase code index. Returns false when the seed lacks an id.
    fn seed_discount_record(&mut self, seed: &Value) -> bool {
        let Some(id) = seed.get("id").and_then(Value::as_str) else {
            return false;
        };
        let mut codes: Vec<Value> = Vec::new();
        let mut push_code = |code: &str, redeem_id: Option<&str>| {
            let mut entry = serde_json::Map::new();
            entry.insert("code".to_string(), json!(code));
            if let Some(redeem_id) = redeem_id {
                entry.insert("id".to_string(), json!(redeem_id));
            }
            codes.push(Value::Object(entry));
        };
        if let Some(nodes) = seed
            .pointer("/discount/codes/nodes")
            .and_then(Value::as_array)
        {
            for node in nodes {
                if let Some(code) = node.get("code").and_then(Value::as_str) {
                    push_code(code, node.get("id").and_then(Value::as_str));
                }
            }
        } else if let Some(nodes) = seed.pointer("/codes/nodes").and_then(Value::as_array) {
            for node in nodes {
                if let Some(code) = node.get("code").and_then(Value::as_str) {
                    push_code(code, node.get("id").and_then(Value::as_str));
                }
            }
        } else if let Some(list) = seed.get("codes").and_then(Value::as_array) {
            for node in list {
                if let Some(code) = node.get("code").and_then(Value::as_str) {
                    push_code(code, node.get("id").and_then(Value::as_str));
                }
            }
        } else if let Some(code) = seed.get("code").and_then(Value::as_str) {
            push_code(code, None);
        }
        for entry in &codes {
            if let Some(code) = entry.get("code").and_then(Value::as_str) {
                self.store
                    .staged
                    .discount_code_index
                    .insert(code.to_ascii_uppercase(), id.to_string());
            }
        }
        self.store.staged.deleted_discount_ids.remove(id);
        // Carry a curated set of discount fields through from the seed when present
        // so seeded discounts read back and filter by status / kind exactly like
        // discounts staged through mutations. Absent those, a bare id+codes record
        // is preserved (the historical seed shape).
        let mut record = serde_json::Map::new();
        for key in [
            "kind",
            "typename",
            "status",
            "title",
            "summary",
            "startsAt",
            "endsAt",
            "createdAt",
            "updatedAt",
            "asyncUsageCount",
            "usageLimit",
            "combinesWith",
            "appDiscountType",
            "recurringCycleLimit",
            "codesCount",
        ] {
            if let Some(value) = seed.get(key) {
                record.insert(key.to_string(), value.clone());
            }
        }
        record.insert("id".to_string(), json!(id));
        record.insert("codes".to_string(), Value::Array(codes.clone()));
        if !record.contains_key("codesCount") {
            record.insert(
                "codesCount".to_string(),
                json!({ "count": codes.len(), "precision": "EXACT" }),
            );
        }
        self.store
            .staged
            .discounts
            .insert(id.to_string(), Value::Object(record));
        true
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

        self.store.replace_base_products_map_with_order(
            product_state_map_from_json(&state["baseState"]["products"]),
            string_array_from_json(&state["baseState"]["productOrder"]),
        );
        self.store.replace_base_product_variants_map_with_order(
            product_variant_state_map_from_json(&state["baseState"]["productVariants"]),
            string_array_from_json(&state["baseState"]["productVariantOrder"]),
        );
        self.store.replace_staged_products_map_with_order(
            product_state_map_from_json(&state["stagedState"]["products"]),
            string_array_from_json(&state["stagedState"]["productOrder"]),
        );
        self.store.replace_staged_product_variants_map_with_order(
            product_variant_state_map_from_json(&state["stagedState"]["productVariants"]),
            string_array_from_json(&state["stagedState"]["productVariantOrder"]),
        );
        self.store.replace_product_tombstones(
            string_array_from_json(&state["stagedState"]["deletedProductIds"])
                .into_iter()
                .collect(),
        );
        self.store.replace_product_variant_tombstones(
            string_array_from_json(&state["stagedState"]["deletedProductVariantIds"])
                .into_iter()
                .collect(),
        );
        self.store.staged.collections = state["stagedState"]
            .get("collections")
            .and_then(Value::as_object)
            .map(|collections| {
                collections
                    .iter()
                    .map(|(id, collection)| (id.clone(), collection.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_collection_ids = state["stagedState"]
            .get("deletedCollectionIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.collection_jobs = state["stagedState"]
            .get("collectionJobs")
            .and_then(Value::as_object)
            .map(|jobs| {
                jobs.iter()
                    .map(|(id, job)| (id.clone(), job.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.replace_base_saved_searches_map_with_order(
            saved_search_state_map_from_json(&state["baseState"]["savedSearches"]),
            string_array_from_json(&state["baseState"]["savedSearchOrder"]),
        );
        let base_shop = state["baseState"]
            .get("shop")
            .filter(|shop| shop.is_object())
            .cloned()
            .unwrap_or_else(default_shop_json);
        let mut base_shop_policies =
            shop_policy_state_map_from_json(&state["baseState"]["shopPolicies"]);
        let mut base_shop_policy_order =
            string_array_from_json(&state["baseState"]["shopPolicyOrder"]);
        if base_shop_policies.is_empty() {
            (base_shop_policies, base_shop_policy_order) = shop_policy_state_from_shop(&base_shop);
        }
        self.store
            .replace_base_shop_policies_map_with_order(base_shop_policies, base_shop_policy_order);
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
                        "marketWebPresences": [{
                            "id": "gid://shopify/MarketWebPresence/62842765618",
                            "subfolderSuffix": null
                        }]
                    }),
                )])
            });
        self.store.base.localization_product_ids = state["baseState"]
            .get("localizationProductIds")
            .map(string_array_from_json)
            .unwrap_or_else(|| vec![LOCALIZATION_BASELINE_PRODUCT_ID.to_string()])
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
        self.store.staged.publications = state["stagedState"]["publications"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(id, record)| (id.clone(), record.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.resource_publications = state["stagedState"]["resourcePublications"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(resource, pubs)| {
                        (resource.clone(), string_array_from_json(pubs).into_iter().collect())
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store.replace_staged_saved_searches_map_with_order(
            saved_search_state_map_from_json(&state["stagedState"]["savedSearches"]),
            string_array_from_json(&state["stagedState"]["savedSearchOrder"]),
        );
        self.store.replace_saved_search_tombstones(
            string_array_from_json(&state["stagedState"]["deletedSavedSearchIds"])
                .into_iter()
                .collect(),
        );
        self.store.replace_staged_shop_policies_map_with_order(
            shop_policy_state_map_from_json(&state["stagedState"]["shopPolicies"]),
            string_array_from_json(&state["stagedState"]["shopPolicyOrder"]),
        );
        self.store.replace_shop_policy_tombstones(
            string_array_from_json(&state["stagedState"]["deletedShopPolicyIds"])
                .into_iter()
                .collect(),
        );
        self.store.staged.shipping_packages = state["stagedState"]["shippingPackages"]
            .as_object()
            .map(|packages| {
                packages
                    .iter()
                    .map(|(id, package)| (id.clone(), package.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_shipping_package_ids = state["stagedState"]
            ["deletedShippingPackageIds"]
            .as_object()
            .map(|ids| ids.keys().cloned().collect())
            .unwrap_or_default();
        self.store.staged.customers = state["stagedState"]["customers"]
            .as_object()
            .map(|customers| {
                customers
                    .iter()
                    .map(|(id, customer)| (id.clone(), customer.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_customer_ids = state["stagedState"]["deletedCustomerIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.store.staged.customer_addresses = state["stagedState"]["customerAddresses"]
            .as_object()
            .map(|addresses| {
                addresses
                    .iter()
                    .map(|(id, address)| (id.clone(), address.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.customer_merge_requests = state["stagedState"]["customerMergeRequests"]
            .as_object()
            .map(|requests| {
                requests
                    .iter()
                    .map(|(id, request)| (id.clone(), request.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.customer_data_erasure_requests = state["stagedState"]
            ["customerDataErasureRequests"]
            .as_object()
            .map(|requests| {
                requests
                    .iter()
                    .map(|(id, request)| (id.clone(), request.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.customers_count_base =
            state["stagedState"]["customersCountBase"].as_u64();
        self.store.staged.store_credit_accounts = state["stagedState"]["storeCreditAccounts"]
            .as_object()
            .map(|accounts| {
                accounts
                    .iter()
                    .map(|(id, account)| (id.clone(), account.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.store_credit_account_order = state["stagedState"]
            .get("storeCreditAccountOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| {
                self.store
                    .staged
                    .store_credit_accounts
                    .keys()
                    .cloned()
                    .collect()
            });
        self.store.staged.store_credit_transactions = state["stagedState"]["storeCreditTransactions"]
            .as_object()
            .map(|transactions| {
                transactions
                    .iter()
                    .map(|(id, transaction)| (id.clone(), transaction.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.next_store_credit_account_id = state["stagedState"]
            .get("nextStoreCreditAccountId")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0)
            .unwrap_or(1);
        self.store.staged.next_store_credit_transaction_id = state["stagedState"]
            .get("nextStoreCreditTransactionId")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0)
            .unwrap_or(1);
        self.store.staged.gift_cards = state["stagedState"]["giftCards"]
            .as_object()
            .map(|cards| {
                cards
                    .iter()
                    .map(|(id, card)| (id.clone(), card.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.taggable_resources = state["stagedState"]["taggableResources"]
            .as_object()
            .map(|resources| {
                resources
                    .iter()
                    .map(|(id, resource)| (id.clone(), resource.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.orders = state["stagedState"]["orders"]
            .as_object()
            .map(|orders| {
                orders
                    .iter()
                    .map(|(id, order)| (id.clone(), order.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_order_ids = state["stagedState"]["deletedOrderIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        // Draft orders are dumped in the cursor-wrapped overlay format
        // ({ "id", "cursor", "data" }); unwrap `data` back to the staged record.
        // These MUST round-trip because the parity runner restores mainState
        // before every downstream target, so read-after-write across targets
        // (draftOrder reads, duplicate/delete chains) would otherwise lose state.
        self.store.staged.draft_orders = state["stagedState"]["draftOrders"]
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
        self.store.staged.next_draft_order_id = state["stagedState"]["nextDraftOrderId"]
            .as_u64()
            .unwrap_or(1);
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
        self.store.staged.returns = state["stagedState"]["returns"]
            .as_object()
            .map(|returns| {
                returns
                    .iter()
                    .map(|(id, return_record)| (id.clone(), return_record.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.reverse_deliveries = state["stagedState"]["reverseDeliveries"]
            .as_object()
            .map(|deliveries| {
                deliveries
                    .iter()
                    .map(|(id, delivery)| (id.clone(), delivery.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.reverse_fulfillment_orders = state["stagedState"]
            ["reverseFulfillmentOrders"]
            .as_object()
            .map(|orders| {
                orders
                    .iter()
                    .map(|(id, order)| (id.clone(), order.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.observed_shipping_locations = state["stagedState"]
            .get("observedShippingLocations")
            .and_then(Value::as_object)
            .map(|locations| {
                locations
                    .iter()
                    .map(|(id, location)| (id.clone(), location.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.locations = state["stagedState"]
            .get("locations")
            .and_then(Value::as_object)
            .map(|locations| {
                locations
                    .iter()
                    .map(|(id, location)| (id.clone(), location.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.location_order = state["stagedState"]
            .get("locationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| self.store.staged.locations.keys().cloned().collect());
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
        self.store.staged.inventory_transfers = state["stagedState"]
            .get("inventoryTransfers")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        self.store.staged.inventory_quantity_updated_at = inventory_quantity_updated_at_from_json(
            &state["stagedState"]["inventoryQuantityUpdatedAt"],
        );
        self.store.staged.next_inventory_quantity_timestamp = state["stagedState"]
            .get("nextInventoryQuantityTimestamp")
            .and_then(Value::as_u64)
            .unwrap_or_default();
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
        self.store.staged.b2b_companies = state["stagedState"]
            .get("b2bCompanies")
            .and_then(Value::as_object)
            .map(|companies| {
                companies
                    .iter()
                    .map(|(id, company)| (id.clone(), company.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_locations = state["stagedState"]
            .get("b2bLocations")
            .and_then(Value::as_object)
            .map(|locations| {
                locations
                    .iter()
                    .map(|(id, location)| (id.clone(), location.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_location_order = state["stagedState"]
            .get("b2bLocationOrder")
            .map(string_array_from_json)
            .unwrap_or_else(|| self.store.staged.b2b_locations.keys().cloned().collect());
        self.store.staged.b2b_contacts = state["stagedState"]
            .get("b2bContacts")
            .and_then(Value::as_object)
            .map(|contacts| {
                contacts
                    .iter()
                    .map(|(id, contact)| (id.clone(), contact.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_contact_roles = state["stagedState"]
            .get("b2bContactRoles")
            .and_then(Value::as_object)
            .map(|roles| {
                roles
                    .iter()
                    .map(|(id, role)| (id.clone(), role.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_role_assignments = state["stagedState"]
            .get("b2bRoleAssignments")
            .and_then(Value::as_object)
            .map(|assignments| {
                assignments
                    .iter()
                    .map(|(id, assignment)| (id.clone(), assignment.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_staff_assignments = state["stagedState"]
            .get("b2bStaffAssignments")
            .and_then(Value::as_object)
            .map(|assignments| {
                assignments
                    .iter()
                    .map(|(id, assignment)| (id.clone(), assignment.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.metaobject_definitions = state["stagedState"]
            .get("metaobjectDefinitions")
            .and_then(Value::as_object)
            .map(|definitions| {
                definitions
                    .iter()
                    .map(|(id, definition)| (id.clone(), definition.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_metaobject_definition_ids = state["stagedState"]
            .get("deletedMetaobjectDefinitionIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.metaobjects = state["stagedState"]
            .get("metaobjects")
            .and_then(Value::as_object)
            .map(|metaobjects| {
                metaobjects
                    .iter()
                    .map(|(id, metaobject)| (id.clone(), metaobject.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_metaobject_ids = state["stagedState"]
            .get("deletedMetaobjectIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.linked_product_option_metaobject_sets = state["stagedState"]
            .get("linkedProductOptionMetaobjectSets")
            .and_then(Value::as_array)
            .map(|sets| {
                sets.iter()
                    .map(|set| string_array_from_json(set).into_iter().collect())
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.metafield_definitions = state["stagedState"]
            .get("metafieldDefinitions")
            .and_then(Value::as_object)
            .map(|definitions| {
                definitions
                    .iter()
                    .filter_map(|(encoded_key, definition)| {
                        encoded_key.split_once('\u{1f}').map(|(namespace, key)| {
                            ((namespace.to_string(), key.to_string()), definition.clone())
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.discounts = state["stagedState"]["discounts"]
            .as_object()
            .map(|discounts| {
                discounts
                    .iter()
                    .map(|(id, discount)| (id.clone(), discount.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.discount_code_index = state["stagedState"]["discountCodeIndex"]
            .as_object()
            .map(|index| {
                index
                    .iter()
                    .filter_map(|(code, id)| id.as_str().map(|id| (code.clone(), id.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_discount_ids = state["stagedState"]["deletedDiscountIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.store.staged.discount_redeem_code_bulk_creations = state["stagedState"]
            ["discountRedeemCodeBulkCreations"]
            .as_object()
            .map(|creations| {
                creations
                    .iter()
                    .map(|(id, creation)| (id.clone(), creation.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_companies = state["stagedState"]
            .get("b2bCompanies")
            .and_then(Value::as_object)
            .map(|companies| {
                companies
                    .iter()
                    .map(|(id, company)| (id.clone(), company.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_locations = state["stagedState"]
            .get("b2bLocations")
            .and_then(Value::as_object)
            .map(|locations| {
                locations
                    .iter()
                    .map(|(id, location)| (id.clone(), location.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.b2b_contacts = state["stagedState"]
            .get("b2bContacts")
            .and_then(Value::as_object)
            .map(|contacts| {
                contacts
                    .iter()
                    .map(|(id, contact)| (id.clone(), contact.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.deleted_b2b_contact_ids = state["stagedState"]
            .get("deletedB2bContactIds")
            .map(string_array_from_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.store.staged.b2b_contact_roles = state["stagedState"]
            .get("b2bContactRoles")
            .and_then(Value::as_object)
            .map(|roles| {
                roles
                    .iter()
                    .map(|(id, role)| (id.clone(), role.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.next_b2b_company_id = state["stagedState"]
            .get("nextB2bCompanyId")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        self.store.staged.next_b2b_contact_id = state["stagedState"]
            .get("nextB2bContactId")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        self.store.staged.next_b2b_contact_role_assignment_id = state["stagedState"]
            .get("nextB2bContactRoleAssignmentId")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        // Markets-domain staged maps — symmetric with the conditional emit in
        // state_snapshot. Missing keys restore to empty (the default).
        self.store.staged.markets = state["stagedState"]
            .get("markets")
            .and_then(Value::as_object)
            .map(|markets| {
                markets
                    .iter()
                    .map(|(id, market)| (id.clone(), market.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.catalogs = state["stagedState"]
            .get("catalogs")
            .and_then(Value::as_object)
            .map(|catalogs| {
                catalogs
                    .iter()
                    .map(|(id, catalog)| (id.clone(), catalog.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.price_lists = state["stagedState"]
            .get("priceLists")
            .and_then(Value::as_object)
            .map(|price_lists| {
                price_lists
                    .iter()
                    .map(|(id, price_list)| (id.clone(), price_list.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.store.staged.web_presences = state["stagedState"]
            .get("webPresences")
            .and_then(Value::as_object)
            .map(|presences| {
                presences
                    .iter()
                    .map(|(id, presence)| (id.clone(), presence.clone()))
                    .collect()
            })
            .unwrap_or_default();
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
        self.store.staged.functions_dirty = state["stagedState"]["functionsDirty"]
            .as_bool()
            .unwrap_or(false);
        self.log_entries = dump["log"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.next_synthetic_id = next_synthetic_id;

        ok_json(json!({ "ok": true, "message": "state restored" }))
    }
}

fn string_array_from_json(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
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

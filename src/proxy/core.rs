use super::*;

impl DraftProxy {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            log_entries: Vec::new(),
            registry: default_registry(),
            store: Store::with_default_baseline(),
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
                "taggableResources": self.store.staged.taggable_resources.clone(),
                "orders": self.store.staged.orders.clone(),
                "deletedOrderIds": self.store.staged.deleted_order_ids.iter().cloned().collect::<Vec<_>>(),
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
                "locationLimitReached": self.store.staged.location_limit_reached,
                "discounts": self.store.staged.discounts.clone(),
                "discountCodeIndex": self.store.staged.discount_code_index.clone(),
                "deletedDiscountIds": self.store.staged.deleted_discount_ids.iter().cloned().collect::<Vec<_>>(),
                "discountRedeemCodeBulkCreations": self.store.staged.discount_redeem_code_bulk_creations.clone(),
                "ownerMetafields": self.store.staged.owner_metafields.clone(),
                "deletedOwnerMetafields": deleted_owner_metafields
            }
        });
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
        if self.store.staged.localization_dirty {
            snapshot["stagedState"]["localizationDirty"] = json!(true);
        }
        if self.store.staged.functions_dirty {
            snapshot["stagedState"]["functionsDirty"] = json!(true);
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
        self.store.staged.location_limit_reached = state["stagedState"]
            .get("locationLimitReached")
            .and_then(Value::as_bool)
            .unwrap_or(false);
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

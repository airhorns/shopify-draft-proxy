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
        let mut snapshot = json!({
            "baseState": {
                "products": product_state_map_json(&self.store.base.products.records),
                "productOrder": self.store.base.products.order,
                "productVariants": product_variant_state_map_json(&self.store.base.product_variants.records),
                "productVariantOrder": self.store.base.product_variants.order,
                "savedSearches": saved_search_state_map_json(&self.store.base.saved_searches.records),
                "savedSearchOrder": self.store.base.saved_searches.order,
                "shop": self.store.base.shop.clone(),
                "publicationIds": self.store.base.publication_ids.iter().cloned().collect::<Vec<_>>(),
                "publicationCount": self.store.base.publication_count,
                "availableLocales": self.store.base.available_locales.iter().map(|(locale, name)| (locale.clone(), json!(name))).collect::<serde_json::Map<_, _>>(),
                "shopLocales": self.store.base.shop_locales.clone()
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
                "shippingPackages": self.store.staged.shipping_packages.clone(),
                "deletedShippingPackageIds": self.store.staged.deleted_shipping_package_ids.iter().map(|id| (id.clone(), json!(true))).collect::<serde_json::Map<_, _>>(),
                "delegatedAccessTokens": self.store.staged.delegate_access_tokens.clone(),
                "customers": self.store.staged.customers.clone(),
                "deletedCustomerIds": self.store.staged.deleted_customer_ids.iter().cloned().collect::<Vec<_>>(),
                "customerOrders": self.store.staged.customer_orders.clone(),
                "taggableResources": self.store.staged.taggable_resources.clone(),
                "orders": self.store.staged.orders.clone(),
                "returns": self.store.staged.returns.clone(),
                "returnsByOrder": self.store.staged.returns_by_order.clone(),
               "reverseDeliveries": self.store.staged.reverse_deliveries.clone(),
                "reverseFulfillmentOrders": self.store.staged.reverse_fulfillment_orders.clone(),
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
                "deletedOwnerMetafields": self.store.staged.deleted_owner_metafields.iter().map(|(owner_id, namespace, key)| json!({"ownerId": owner_id, "namespace": namespace, "key": key})).collect::<Vec<_>>()
            }
        });
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
        self.store.base.shop = state["baseState"]
            .get("shop")
            .filter(|shop| shop.is_object())
            .cloned()
            .unwrap_or_else(default_shop_json);
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

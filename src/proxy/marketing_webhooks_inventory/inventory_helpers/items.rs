use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn effective_inventory_level(
        &self,
        key: &(String, String),
    ) -> Option<&BTreeMap<String, i64>> {
        self.store
            .staged
            .inventory_levels
            .get(key)
            .or_else(|| self.store.base.inventory_levels.get(key))
    }

    pub(in crate::proxy) fn stage_inventory_level_for_write(&mut self, key: &(String, String)) {
        if self.store.staged.inventory_levels.contains_key(key) {
            return;
        }
        if let Some(base) = self.store.base.inventory_levels.get(key).cloned() {
            self.store.staged.inventory_levels.insert(key.clone(), base);
        }
    }

    fn effective_inventory_level_id(&self, key: &(String, String)) -> Option<&String> {
        self.store
            .staged
            .inventory_level_ids
            .get(key)
            .or_else(|| self.store.base.inventory_level_ids.get(key))
    }

    pub(in crate::proxy) fn inventory_level_is_active(&self, key: &(String, String)) -> bool {
        if self.store.staged.active_inventory_levels.contains(key) {
            return true;
        }
        if self.store.staged.inactive_inventory_levels.contains(key) {
            return false;
        }
        !self.store.base.inactive_inventory_levels.contains(key)
    }

    fn effective_inventory_quantity_updated_at(
        &self,
        key: &(String, String, String),
    ) -> Option<&String> {
        self.store
            .staged
            .inventory_quantity_updated_at
            .get(key)
            .or_else(|| self.store.base.inventory_quantity_updated_at.get(key))
    }

    pub(in crate::proxy) fn inventory_item_is_tombstoned(&self, inventory_item_id: &str) -> bool {
        self.store
            .product_variants
            .base
            .records
            .values()
            .chain(self.store.product_variants.staged.records.values())
            .filter(|variant| variant.inventory_item.id == inventory_item_id)
            .any(|variant| {
                self.store
                    .product_variants
                    .staged
                    .is_tombstoned(&variant.id)
                    || self
                        .store
                        .products
                        .staged
                        .is_tombstoned(&variant.product_id)
            })
    }

    pub(in crate::proxy) fn inventory_item_has_staged_overlay(
        &self,
        inventory_item_id: &str,
    ) -> bool {
        self.inventory_item_is_tombstoned(inventory_item_id)
            || self
                .store
                .product_variants
                .staged
                .records
                .values()
                .any(|variant| variant.inventory_item.id == inventory_item_id)
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(item_id, _)| item_id == inventory_item_id)
            || self
                .store
                .staged
                .inactive_inventory_levels
                .iter()
                .chain(self.store.staged.active_inventory_levels.iter())
                .any(|(item_id, _)| item_id == inventory_item_id)
            || self
                .store
                .staged
                .inventory_transfers
                .records
                .values()
                .any(|transfer| {
                    transfer
                        .line_items
                        .iter()
                        .any(|line_item| line_item.inventory_item_id == inventory_item_id)
                })
    }

    pub(in crate::proxy) fn inventory_item_has_authoritative_base(
        &self,
        inventory_item_id: &str,
    ) -> bool {
        self.store
            .product_variants
            .base
            .records
            .values()
            .any(|variant| variant.inventory_item.id == inventory_item_id)
            || self
                .store
                .base
                .inventory_levels
                .keys()
                .any(|(item_id, _)| item_id == inventory_item_id)
    }

    pub(in crate::proxy) fn inventory_catalog_has_staged_overlay(&self) -> bool {
        !self.store.product_variants.staged.records.is_empty()
            || !self.store.product_variants.staged.tombstones.is_empty()
            || !self.store.products.staged.tombstones.is_empty()
            || !self.store.staged.inventory_levels.is_empty()
            || !self.store.staged.inactive_inventory_levels.is_empty()
            || !self.store.staged.active_inventory_levels.is_empty()
    }

    pub(in crate::proxy) fn inventory_level_has_staged_overlay(&self, id: &str) -> bool {
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(id)
        else {
            return false;
        };
        let key = (inventory_item_id, location_id);
        self.inventory_item_is_tombstoned(&key.0)
            || self.store.staged.inventory_levels.contains_key(&key)
            || self.store.staged.inventory_level_ids.contains_key(&key)
            || self.store.staged.inactive_inventory_levels.contains(&key)
            || self.store.staged.active_inventory_levels.contains(&key)
    }

    pub(in crate::proxy) fn observe_inventory_items_connection(&mut self, connection: &Value) {
        let mut seen_ids = BTreeSet::new();
        for edge in connection
            .get("edges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(node) = edge.get("node") else {
                continue;
            };
            if let (Some(id), Some(cursor)) = (
                node.get("id").and_then(Value::as_str),
                edge.get("cursor").and_then(Value::as_str),
            ) {
                self.store
                    .base
                    .inventory_item_cursors
                    .insert(id.to_string(), cursor.to_string());
                seen_ids.insert(id.to_string());
            }
            self.observe_inventory_item_node(node);
        }
        for node in connection
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if node
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| seen_ids.contains(id))
            {
                continue;
            }
            self.observe_inventory_item_node(node);
        }
    }

    pub(in crate::proxy) fn hydrate_inventory_items_catalog(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.base.inventory_items_catalog_hydrated
        {
            return;
        }
        let mut after = Value::Null;
        let mut seen_cursors = BTreeSet::new();
        loop {
            let response = self.upstream_post(
                request,
                json!({
                    "query": INVENTORY_ITEMS_CATALOG_HYDRATE_QUERY,
                    "operationName": "InventoryItemsCatalogHydrate",
                    "variables": { "first": 250, "after": after }
                }),
            );
            if !(200..300).contains(&response.status)
                || response
                    .body
                    .get("errors")
                    .and_then(Value::as_array)
                    .is_some_and(|errors| !errors.is_empty())
            {
                return;
            }
            let Some(connection) = response.body.pointer("/data/inventoryItems") else {
                return;
            };
            self.observe_inventory_items_connection(&connection.clone());
            let has_next_page = connection
                .pointer("/pageInfo/hasNextPage")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !has_next_page {
                self.store.base.inventory_items_catalog_hydrated = true;
                return;
            }
            let Some(end_cursor) = connection
                .pointer("/pageInfo/endCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            if !seen_cursors.insert(end_cursor.clone()) {
                return;
            }
            after = json!(end_cursor);
        }
    }

    pub(super) fn inventory_items_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        staged_connection_value_with_args(
            self.inventory_item_ids_for_connection(),
            arguments,
            |inventory_item_id, query| {
                self.inventory_item_search_decision(inventory_item_id, query)
            },
            |inventory_item_id, sort_key| inventory_item_sort_key(inventory_item_id, sort_key),
            |inventory_item_id| self.inventory_item_canonical_value(inventory_item_id),
            |inventory_item_id| {
                self.store
                    .base
                    .inventory_item_cursors
                    .get(inventory_item_id)
                    .cloned()
                    .unwrap_or_else(|| inventory_item_id.clone())
            },
        )
    }

    fn inventory_item_ids_for_connection(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut item_ids = Vec::new();

        for variant in effective_records(
            &self.store.product_variants.base,
            &self.store.product_variants.staged,
        ) {
            let inventory_item_id = variant.inventory_item.id;
            if seen.insert(inventory_item_id.clone()) {
                item_ids.push(inventory_item_id);
            }
        }

        for (inventory_item_id, _) in &self.store.staged.inventory_level_order {
            if seen.insert(inventory_item_id.clone()) {
                item_ids.push(inventory_item_id.clone());
            }
        }
        for (inventory_item_id, _) in &self.store.base.inventory_level_order {
            if seen.insert(inventory_item_id.clone()) {
                item_ids.push(inventory_item_id.clone());
            }
        }
        for (inventory_item_id, _) in self.store.base.inventory_levels.keys() {
            if seen.insert(inventory_item_id.clone()) {
                item_ids.push(inventory_item_id.clone());
            }
        }
        for (inventory_item_id, _) in self.store.staged.inventory_levels.keys() {
            if seen.insert(inventory_item_id.clone()) {
                item_ids.push(inventory_item_id.clone());
            }
        }

        item_ids
            .into_iter()
            .filter(|id| !self.inventory_item_is_tombstoned(id))
            .collect()
    }

    fn inventory_item_search_decision(
        &self,
        inventory_item_id: &str,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return StagedSearchDecision::Match;
        };
        let mut saw_supported_term = false;
        for term in inventory_search_terms(query) {
            match self.inventory_item_matches_search_term(inventory_item_id, &term) {
                Some(true) => saw_supported_term = true,
                Some(false) => return StagedSearchDecision::NoMatch,
                None => return StagedSearchDecision::Unsupported,
            }
        }
        StagedSearchDecision::from_bool(saw_supported_term)
    }

    fn inventory_item_matches_search_term(
        &self,
        inventory_item_id: &str,
        term: &str,
    ) -> Option<bool> {
        let term = term.trim();
        if term.is_empty() {
            return Some(true);
        }
        let Some((field, raw_value)) = term.split_once(':') else {
            return Some(self.inventory_item_matches_default_query(inventory_item_id, term));
        };
        let field = field.trim().to_ascii_lowercase();
        match field.as_str() {
            "id" => Some(inventory_id_matches_query(inventory_item_id, raw_value)),
            "sku" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(
                    self.store
                        .product_variant_by_inventory_item_id(inventory_item_id)
                        .map(|variant| variant.sku.eq_ignore_ascii_case(&value))
                        .unwrap_or(false),
                )
            }
            "tracked" => {
                let value = inventory_unquoted_query_value(raw_value);
                match value.to_ascii_lowercase().as_str() {
                    "true" => Some(self.inventory_item_tracked(inventory_item_id)),
                    "false" => Some(!self.inventory_item_tracked(inventory_item_id)),
                    _ => None,
                }
            }
            "created_at" => Some(inventory_datetime_matches_query(
                self.inventory_item_query_timestamp(inventory_item_id, "createdAt")
                    .as_deref(),
                raw_value,
            )),
            "updated_at" => Some(inventory_datetime_matches_query(
                self.inventory_item_query_timestamp(inventory_item_id, "updatedAt")
                    .as_deref(),
                raw_value,
            )),
            _ => None,
        }
    }

    fn inventory_item_matches_default_query(&self, inventory_item_id: &str, term: &str) -> bool {
        let value = inventory_unquoted_query_value(term);
        if value.is_empty() {
            return false;
        }
        inventory_item_id.eq_ignore_ascii_case(&value)
            || resource_id_tail(inventory_item_id).eq_ignore_ascii_case(&value)
            || self
                .store
                .product_variant_by_inventory_item_id(inventory_item_id)
                .map(|variant| inventory_search_string_matches(&variant.sku, &value))
                .unwrap_or(false)
    }

    fn inventory_item_query_timestamp(
        &self,
        inventory_item_id: &str,
        graph_key: &str,
    ) -> Option<String> {
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)?;
        variant
            .inventory_item
            .extra_fields
            .get(graph_key)
            .and_then(Value::as_str)
            .or_else(|| variant.extra_fields.get(graph_key).and_then(Value::as_str))
            .map(str::to_string)
            .or_else(|| {
                self.store
                    .product_by_id(&variant.product_id)
                    .map(|product| {
                        if graph_key == "createdAt" {
                            product.created_at.clone()
                        } else {
                            product.updated_at.clone()
                        }
                    })
            })
    }

    fn inventory_item_tracked(&self, inventory_item_id: &str) -> bool {
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| variant.inventory_item.tracked)
            .unwrap_or(true)
    }

    pub(in crate::proxy) fn observe_inventory_item_node(&mut self, node: &Value) {
        let Some(inventory_item_id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        if let Some(variant) = node.get("variant") {
            self.stage_inventory_item_observed_variant(inventory_item_id, node, variant);
        }
        if let Some(levels) = node
            .get("inventoryLevels")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
        {
            for level in levels {
                self.observe_inventory_level_node_for_item(level, Some(inventory_item_id));
            }
        }
    }

    pub(in crate::proxy) fn observe_inventory_level_node(&mut self, node: &Value) {
        self.observe_inventory_level_node_for_item(node, None);
    }

    fn observe_inventory_level_node_for_item(
        &mut self,
        node: &Value,
        inventory_item_hint: Option<&str>,
    ) {
        let Some(level_id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        let parsed_parts = self.inventory_level_parts_from_id_or_fallback(level_id);
        let inventory_item_id = node
            .get("item")
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| inventory_item_hint.map(str::to_string))
            .or_else(|| {
                inventory_level_id_tail_and_query(level_id).map(|(_, query)| {
                    if is_shopify_gid_of_type(query, "InventoryItem") {
                        query.to_string()
                    } else {
                        shopify_gid("InventoryItem", query)
                    }
                })
            })
            .or_else(|| parsed_parts.as_ref().map(|(item_id, _)| item_id.clone()));
        let location_id = node
            .get("location")
            .and_then(|location| location.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| parsed_parts.map(|(_, location_id)| location_id));
        let (Some(inventory_item_id), Some(location_id)) = (inventory_item_id, location_id) else {
            return;
        };
        let key = (inventory_item_id.clone(), location_id.clone());
        if let Some(rows) = node.get("quantities").and_then(Value::as_array) {
            let observed = inventory_quantities_from_observed_rows(rows);
            let quantities = self
                .store
                .base
                .inventory_levels
                .entry(key.clone())
                .or_insert_with(empty_inventory_quantities);
            for row in rows {
                let Some(name) = row.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(quantity) = observed.get(name) {
                    quantities.insert(name.to_string(), *quantity);
                }
            }
        } else {
            self.store
                .base
                .inventory_levels
                .entry(key.clone())
                .or_default();
        }
        self.store
            .base
            .inventory_level_ids
            .insert(key.clone(), level_id.to_string());
        if !self.store.base.inventory_level_order.contains(&key) {
            self.store.base.inventory_level_order.push(key.clone());
        }
        if let Some(rows) = node.get("quantities").and_then(Value::as_array) {
            for row in rows {
                let Some(name) = row.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let timestamp_key = (
                    inventory_item_id.clone(),
                    location_id.clone(),
                    name.to_string(),
                );
                if let Some(updated_at) = row.get("updatedAt").and_then(Value::as_str) {
                    self.store
                        .base
                        .inventory_quantity_updated_at
                        .insert(timestamp_key, updated_at.to_string());
                } else {
                    self.store
                        .base
                        .inventory_quantity_updated_at
                        .remove(&timestamp_key);
                }
            }
        }
        if let Some(is_active) = node.get("isActive").and_then(Value::as_bool) {
            if is_active {
                self.store.base.inactive_inventory_levels.remove(&key);
            } else {
                self.store
                    .base
                    .inactive_inventory_levels
                    .insert(key.clone());
            }
        }
        if let Some(location) = node.get("location") {
            self.observe_base_inventory_location(location);
        }
        if let Some(item) = node.get("item") {
            if let Some(variant) = item.get("variant") {
                self.stage_inventory_item_observed_variant(&inventory_item_id, item, variant);
            }
            if let Some(levels) = item
                .get("inventoryLevels")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
            {
                for nested_level in levels {
                    self.observe_inventory_level_node_for_item(
                        nested_level,
                        Some(&inventory_item_id),
                    );
                }
            }
        }
    }

    fn stage_inventory_item_observed_variant(
        &mut self,
        inventory_item_id: &str,
        inventory_item: &Value,
        variant: &Value,
    ) {
        let Some(variant_id) = variant.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(product_id) = variant
            .get("product")
            .and_then(|product| product.get("id"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if let Some(product) = variant.get("product").and_then(product_state_from_json) {
            self.store.observe_base_product(product);
        }
        let selected_options = variant
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect();
        let inventory_item_extra = product_variant_state_extra_fields(
            inventory_item,
            &[
                "id",
                "tracked",
                "requiresShipping",
                "inventoryLevels",
                "variant",
            ],
        );
        let mut variant_record = ProductVariantRecord {
            id: variant_id.to_string(),
            product_id: product_id.to_string(),
            title: variant
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            sku: variant
                .get("sku")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            barcode: variant
                .get("barcode")
                .and_then(Value::as_str)
                .map(str::to_string),
            price: variant
                .get("price")
                .and_then(Value::as_str)
                .unwrap_or("0.00")
                .to_string(),
            compare_at_price: variant
                .get("compareAtPrice")
                .and_then(Value::as_str)
                .map(str::to_string),
            taxable: variant
                .get("taxable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            inventory_policy: variant
                .get("inventoryPolicy")
                .and_then(Value::as_str)
                .unwrap_or("DENY")
                .to_string(),
            inventory_quantity: variant
                .get("inventoryQuantity")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            selected_options,
            media_ids: Vec::new(),
            inventory_item: ProductVariantInventoryItem {
                id: inventory_item_id.to_string(),
                tracked: inventory_item
                    .get("tracked")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                requires_shipping: inventory_item
                    .get("requiresShipping")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                extra_fields: inventory_item_extra,
            },
            extra_fields: product_variant_state_extra_fields(
                variant,
                &[
                    "id",
                    "productId",
                    "title",
                    "sku",
                    "barcode",
                    "price",
                    "compareAtPrice",
                    "taxable",
                    "inventoryPolicy",
                    "inventoryQuantity",
                    "selectedOptions",
                    "inventoryItem",
                ],
            ),
        };
        if let Some(mut existing) = self.store.product_variants.base.get(variant_id).cloned() {
            existing.product_id = variant_record.product_id.clone();
            if variant.get("title").is_some() {
                existing.title = variant_record.title.clone();
            }
            if variant.get("sku").is_some() {
                existing.sku = variant_record.sku.clone();
            }
            if variant.get("barcode").is_some() {
                existing.barcode = variant_record.barcode.clone();
            }
            if variant.get("price").is_some() {
                existing.price = variant_record.price.clone();
            }
            if variant.get("compareAtPrice").is_some() {
                existing.compare_at_price = variant_record.compare_at_price.clone();
            }
            if variant.get("taxable").is_some() {
                existing.taxable = variant_record.taxable;
            }
            if variant.get("inventoryPolicy").is_some() {
                existing.inventory_policy = variant_record.inventory_policy.clone();
            }
            if variant.get("inventoryQuantity").is_some() {
                existing.inventory_quantity = variant_record.inventory_quantity;
            }
            if variant.get("selectedOptions").is_some() {
                existing.selected_options = variant_record.selected_options.clone();
            }
            existing
                .extra_fields
                .extend(variant_record.extra_fields.clone());
            existing.inventory_item.id = inventory_item_id.to_string();
            if inventory_item.get("tracked").is_some() {
                existing.inventory_item.tracked = variant_record.inventory_item.tracked;
            }
            if inventory_item.get("requiresShipping").is_some() {
                existing.inventory_item.requires_shipping =
                    variant_record.inventory_item.requires_shipping;
            }
            existing
                .inventory_item
                .extra_fields
                .extend(variant_record.inventory_item.extra_fields.clone());
            variant_record = existing;
        }
        self.store.observe_base_product_variant(variant_record);
    }

    pub(in crate::proxy) fn observe_base_inventory_location(&mut self, location: &Value) {
        let Some(id) = location.get("id").and_then(Value::as_str) else {
            return;
        };
        if self.store.staged.locations.is_tombstoned(id) {
            return;
        }
        let mut record = self
            .store
            .base
            .locations
            .get(id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(object) = location.as_object() {
            for (key, value) in object {
                record.insert(key.clone(), value.clone());
            }
        }
        record
            .entry("__typename".to_string())
            .or_insert_with(|| json!("Location"));
        record
            .entry("isActive".to_string())
            .or_insert_with(|| json!(true));
        self.store
            .base
            .locations
            .insert(id.to_string(), Value::Object(record));
    }

    pub(in crate::proxy) fn merge_staged_location(
        &mut self,
        location: &Value,
        defaults: &[(&str, Value)],
    ) {
        let Some(id) = location.get("id").and_then(Value::as_str) else {
            return;
        };
        let mut record = self
            .store
            .staged
            .locations
            .get(id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(object) = location.as_object() {
            for (key, value) in object {
                record.insert(key.clone(), value.clone());
            }
        }
        for (key, value) in defaults {
            record
                .entry((*key).to_string())
                .or_insert_with(|| value.clone());
        }
        self.store
            .staged
            .locations
            .insert(id.to_string(), Value::Object(record));
    }

    pub(super) fn inventory_quantity_value_by_id(&self, id: &str) -> Value {
        let Some((inventory_item_id, location_id, name)) = inventory_quantity_parts_from_id(id)
        else {
            return Value::Null;
        };
        if self.inventory_item_is_tombstoned(&inventory_item_id) {
            return Value::Null;
        }
        let level_key = (inventory_item_id.clone(), location_id.clone());
        let Some(quantities) = self.effective_inventory_level(&level_key) else {
            return Value::Null;
        };
        json!({
            "__typename": "InventoryQuantity",
            "id": inventory_quantity_id(&inventory_item_id, &location_id, &name),
            "name": name,
            "quantity": quantities.get(&name).copied().unwrap_or(0),
            "updatedAt": self.effective_inventory_quantity_updated_at(&(
                inventory_item_id,
                location_id,
                name,
            )),
        })
    }

    pub(super) fn inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        if self.inventory_item_is_tombstoned(inventory_item_id) {
            return Vec::new();
        }
        // Levels created via local mutations (e.g. inventoryActivate) are surfaced in
        // their creation order, tracked by `inventory_level_order`. Any remaining
        // levels (observed/hydrated from upstream) fall back to the BTreeMap's stable
        // sorted-by-location-id order, which the inventory lifecycle specs depend on.
        let mut levels = Vec::new();
        let mut seen = BTreeSet::new();
        for (item_id, location_id) in self
            .store
            .base
            .inventory_level_order
            .iter()
            .chain(self.store.staged.inventory_level_order.iter())
        {
            if item_id != inventory_item_id || seen.contains(location_id) {
                continue;
            }
            if let Some(quantities) =
                self.effective_inventory_level(&(item_id.clone(), location_id.clone()))
            {
                seen.insert(location_id.clone());
                levels.push((location_id.clone(), quantities.clone()));
            }
        }
        for ((item_id, location_id), _) in self
            .store
            .base
            .inventory_levels
            .iter()
            .chain(self.store.staged.inventory_levels.iter())
        {
            if item_id != inventory_item_id || !seen.insert(location_id.clone()) {
                continue;
            }
            if let Some(quantities) =
                self.effective_inventory_level(&(item_id.clone(), location_id.clone()))
            {
                levels.push((location_id.clone(), quantities.clone()));
            }
        }
        levels
    }

    pub(in crate::proxy) fn inventory_item_canonical_value(
        &self,
        inventory_item_id: &str,
    ) -> Value {
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id);
        let mut object = variant
            .map(|variant| {
                variant
                    .inventory_item
                    .extra_fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect::<serde_json::Map<_, _>>()
            })
            .unwrap_or_default();
        object.insert("__typename".to_string(), json!("InventoryItem"));
        object.insert("id".to_string(), json!(inventory_item_id));
        object.insert(
            "legacyResourceId".to_string(),
            json!(resource_id_tail(inventory_item_id)),
        );
        object.insert(
            "tracked".to_string(),
            json!(variant
                .map(|variant| variant.inventory_item.tracked)
                .unwrap_or(true)),
        );
        object.insert(
            "requiresShipping".to_string(),
            json!(variant
                .map(|variant| variant.inventory_item.requires_shipping)
                .unwrap_or(true)),
        );
        object.insert(
            "sku".to_string(),
            variant
                .map(|variant| {
                    if variant.sku.is_empty() {
                        Value::Null
                    } else {
                        json!(variant.sku)
                    }
                })
                .unwrap_or(Value::Null),
        );
        Value::Object(object)
    }

    pub(super) fn inventory_item_variant_value(&self, inventory_item_id: &str) -> Value {
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| self.inventory_item_effective_variant_value(variant))
            .unwrap_or(Value::Null)
    }

    fn inventory_item_effective_variant_value(&self, variant: &ProductVariantRecord) -> Value {
        let mut variant = variant.clone();
        if !self
            .inventory_levels_for_item(&variant.inventory_item.id)
            .is_empty()
        {
            variant.inventory_quantity =
                self.inventory_total(&variant.inventory_item.id, "available");
        }
        self.product_variant_canonical_value(&variant)
    }

    pub(super) fn inventory_item_variants_connection_value(
        &self,
        inventory_item_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let variants = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| vec![self.inventory_item_effective_variant_value(variant)])
            .unwrap_or_default();
        connection_value_with_args(variants, arguments, |variant| {
            variant
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
    }

    pub(super) fn inventory_item_country_codes_value(
        &self,
        inventory_item_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let records = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .and_then(|variant| {
                variant
                    .inventory_item
                    .extra_fields
                    .get("countryHarmonizedSystemCodes")
            })
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        connection_value_with_args(records, arguments, |record| {
            record
                .get("countryCode")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
    }

    pub(super) fn inventory_item_level_value(
        &self,
        inventory_item_id: &str,
        location_id: &str,
        include_inactive: bool,
    ) -> Value {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        if !include_inactive && !self.inventory_level_is_active(&key) {
            return Value::Null;
        }
        self.effective_inventory_level(&key)
            .map(|quantities| {
                self.inventory_level_canonical_value(inventory_item_id, location_id, quantities)
            })
            .unwrap_or(Value::Null)
    }

    pub(super) fn inventory_item_levels_connection_value(
        &self,
        inventory_item_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let include_inactive = resolved_bool_field(arguments, "includeInactive").unwrap_or(false);
        staged_connection_value_with_args(
            self.inventory_level_records_for_item(inventory_item_id, include_inactive),
            arguments,
            |level, query| self.inventory_item_level_search_decision(level, query),
            inventory_location_level_sort_key,
            |level| {
                self.inventory_level_canonical_value(
                    &level.inventory_item_id,
                    &level.location_id,
                    &level.quantities,
                )
            },
            |level| self.inventory_location_level_cursor(level),
        )
    }

    pub(in crate::proxy) fn inventory_level_value_by_id(&self, id: &str) -> Value {
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(id)
        else {
            return Value::Null;
        };
        if self.inventory_item_is_tombstoned(&inventory_item_id) {
            return Value::Null;
        }
        let Some(quantities) =
            self.effective_inventory_level(&(inventory_item_id.clone(), location_id.clone()))
        else {
            return Value::Null;
        };
        self.inventory_level_canonical_value(&inventory_item_id, &location_id, quantities)
    }

    fn inventory_level_canonical_value(
        &self,
        inventory_item_id: &str,
        location_id: &str,
        quantities: &BTreeMap<String, i64>,
    ) -> Value {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        let rows = quantities
            .iter()
            .map(|(name, quantity)| {
                json!({
                    "__typename": "InventoryQuantity",
                    "id": inventory_quantity_id(inventory_item_id, location_id, name),
                    "name": name,
                    "quantity": quantity,
                    "updatedAt": self.effective_inventory_quantity_updated_at(&(
                        inventory_item_id.to_string(),
                        location_id.to_string(),
                        name.clone(),
                    )),
                })
            })
            .collect::<Vec<_>>();
        json!({
            "__typename": "InventoryLevel",
            "id": self
                .store
                .staged
                .inventory_level_ids
                .get(&key)
                .or_else(|| self.store.base.inventory_level_ids.get(&key))
                .cloned()
                .unwrap_or_else(|| inventory_level_id(inventory_item_id, location_id)),
            "inventoryItemId": inventory_item_id,
            "isActive": self.inventory_level_is_active(&key),
            "item": { "id": inventory_item_id },
            "location": inventory_level_location_for_view(
                location_id,
                &self.inventory_level_view_state(),
            ),
            "quantities": rows,
        })
    }

    fn inventory_level_records_for_item(
        &self,
        inventory_item_id: &str,
        include_inactive: bool,
    ) -> Vec<InventoryLocationLevelRecord> {
        self.inventory_levels_for_item(inventory_item_id)
            .into_iter()
            .filter_map(|(location_id, quantities)| {
                let key = (inventory_item_id.to_string(), location_id.clone());
                if !include_inactive && !self.inventory_level_is_active(&key) {
                    return None;
                }
                Some(InventoryLocationLevelRecord {
                    inventory_item_id: inventory_item_id.to_string(),
                    level_id: self.effective_inventory_level_id(&key).cloned(),
                    location_id,
                    quantities,
                })
            })
            .collect()
    }

    fn inventory_item_level_search_decision(
        &self,
        level: &InventoryLocationLevelRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return StagedSearchDecision::Match;
        };
        for term in inventory_search_terms(query) {
            let term = term.trim();
            let Some((field, raw_value)) = term.split_once(':') else {
                continue;
            };
            match field.trim().to_ascii_lowercase().as_str() {
                "inventory_item_id" => {
                    if !inventory_id_matches_query(&level.inventory_item_id, raw_value) {
                        return StagedSearchDecision::NoMatch;
                    }
                }
                "id" => return StagedSearchDecision::NoMatch,
                _ => {}
            }
        }
        StagedSearchDecision::Match
    }

    pub(in crate::proxy) fn location_inventory_levels_connection_value(
        &self,
        location_id: &str,
        location: Option<&Value>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let include_inactive = resolved_bool_field(arguments, "includeInactive").unwrap_or(false);
        staged_connection_value_with_args(
            self.inventory_levels_for_location(location_id, location, include_inactive),
            arguments,
            |level, query| self.inventory_location_level_search_decision(level, query),
            inventory_location_level_sort_key,
            |level| {
                self.inventory_level_canonical_value(
                    &level.inventory_item_id,
                    &level.location_id,
                    &level.quantities,
                )
            },
            |level| self.inventory_location_level_cursor(level),
        )
    }

    fn inventory_levels_for_location(
        &self,
        location_id: &str,
        location: Option<&Value>,
        include_inactive: bool,
    ) -> Vec<InventoryLocationLevelRecord> {
        let mut levels = Vec::new();
        let mut seen = BTreeSet::new();
        for (inventory_item_id, staged_location_id) in self
            .store
            .base
            .inventory_level_order
            .iter()
            .chain(self.store.staged.inventory_level_order.iter())
        {
            if staged_location_id != location_id
                || self.inventory_item_is_tombstoned(inventory_item_id)
                || seen.contains(&(inventory_item_id.clone(), staged_location_id.clone()))
            {
                continue;
            }
            let key = (inventory_item_id.clone(), staged_location_id.clone());
            seen.insert(key.clone());
            if !include_inactive && !self.inventory_level_is_active(&key) {
                continue;
            }
            if let Some(quantities) = self.effective_inventory_level(&key) {
                levels.push(InventoryLocationLevelRecord {
                    inventory_item_id: inventory_item_id.clone(),
                    location_id: staged_location_id.clone(),
                    level_id: self.effective_inventory_level_id(&key).cloned(),
                    quantities: quantities.clone(),
                });
            }
        }
        for ((inventory_item_id, staged_location_id), quantities) in self
            .store
            .base
            .inventory_levels
            .iter()
            .chain(self.store.staged.inventory_levels.iter())
        {
            if staged_location_id != location_id {
                continue;
            }
            if self.inventory_item_is_tombstoned(inventory_item_id) {
                continue;
            }
            let key = (inventory_item_id.clone(), staged_location_id.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key.clone());
            if !include_inactive && !self.inventory_level_is_active(&key) {
                continue;
            }
            let quantities = self.effective_inventory_level(&key).unwrap_or(quantities);
            levels.push(InventoryLocationLevelRecord {
                inventory_item_id: inventory_item_id.clone(),
                location_id: staged_location_id.clone(),
                level_id: self.effective_inventory_level_id(&key).cloned(),
                quantities: quantities.clone(),
            });
        }
        if let Some(location) = location {
            if let Some(nodes) = location
                .get("inventoryLevels")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
            {
                for node in nodes {
                    let level_id = node.get("id").and_then(Value::as_str).map(str::to_string);
                    let Some(inventory_item_id) = node
                        .get("item")
                        .and_then(|item| item.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            level_id
                                .as_deref()
                                .and_then(|id| self.inventory_level_parts_from_id_or_fallback(id))
                                .map(|(inventory_item_id, _)| inventory_item_id)
                        })
                    else {
                        continue;
                    };
                    let node_location_id = node
                        .get("location")
                        .and_then(|location| location.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            level_id
                                .as_deref()
                                .and_then(|id| self.inventory_level_parts_from_id_or_fallback(id))
                                .map(|(_, location_id)| location_id)
                        })
                        .unwrap_or_else(|| location_id.to_string());
                    if node_location_id != location_id {
                        continue;
                    }
                    let key = (inventory_item_id.clone(), node_location_id.clone());
                    if self.inventory_item_is_tombstoned(&inventory_item_id) {
                        continue;
                    }
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.insert(key);
                    if !include_inactive
                        && node.get("isActive").and_then(Value::as_bool) == Some(false)
                    {
                        continue;
                    }
                    let quantities = node
                        .get("quantities")
                        .and_then(Value::as_array)
                        .map(|rows| inventory_quantities_from_observed_rows(rows))
                        .unwrap_or_else(empty_inventory_quantities);
                    levels.push(InventoryLocationLevelRecord {
                        inventory_item_id,
                        location_id: node_location_id,
                        level_id,
                        quantities,
                    });
                }
            }
        }
        levels
    }

    fn inventory_location_level_cursor(&self, level: &InventoryLocationLevelRecord) -> String {
        level.level_id.clone().unwrap_or_else(|| {
            self.store
                .staged
                .inventory_level_ids
                .get(&(level.inventory_item_id.clone(), level.location_id.clone()))
                .or_else(|| {
                    self.store
                        .base
                        .inventory_level_ids
                        .get(&(level.inventory_item_id.clone(), level.location_id.clone()))
                })
                .cloned()
                .unwrap_or_else(|| inventory_level_id(&level.inventory_item_id, &level.location_id))
        })
    }

    fn inventory_location_level_search_decision(
        &self,
        level: &InventoryLocationLevelRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return StagedSearchDecision::Match;
        };
        let mut saw_supported_term = false;
        for term in inventory_search_terms(query) {
            match self.inventory_location_level_matches_search_term(level, &term) {
                Some(true) => saw_supported_term = true,
                Some(false) => return StagedSearchDecision::NoMatch,
                None => return StagedSearchDecision::Unsupported,
            }
        }
        StagedSearchDecision::from_bool(saw_supported_term)
    }

    fn inventory_location_level_matches_search_term(
        &self,
        level: &InventoryLocationLevelRecord,
        term: &str,
    ) -> Option<bool> {
        let term = term.trim();
        if term.is_empty() {
            return Some(true);
        }
        let Some((field, raw_value)) = term.split_once(':') else {
            let level_id = self.inventory_location_level_cursor(level);
            let value = inventory_unquoted_query_value(term);
            return Some(
                inventory_id_matches_query(&level_id, &value)
                    || inventory_id_matches_query(&level.inventory_item_id, &value)
                    || inventory_id_matches_query(&level.location_id, &value)
                    || self
                        .store
                        .product_variant_by_inventory_item_id(&level.inventory_item_id)
                        .map(|variant| inventory_search_string_matches(&variant.sku, &value))
                        .unwrap_or(false),
            );
        };
        let field = field.trim().to_ascii_lowercase();
        match field.as_str() {
            "id" => Some(inventory_id_matches_query(
                &self.inventory_location_level_cursor(level),
                raw_value,
            )),
            "inventory_item_id" => Some(inventory_id_matches_query(
                &level.inventory_item_id,
                raw_value,
            )),
            "location_id" => Some(inventory_id_matches_query(&level.location_id, raw_value)),
            "sku" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(
                    self.store
                        .product_variant_by_inventory_item_id(&level.inventory_item_id)
                        .map(|variant| variant.sku.eq_ignore_ascii_case(&value))
                        .unwrap_or(false),
                )
            }
            _ => None,
        }
    }

    /// Build a fully-materialized `inventoryLevels` connection value for an inventory
    /// item from effective base-plus-staged level state (ids, locations, quantities, updatedAt timestamps,
    /// and the opaque seeded edge cursors). The result carries `edges`, `nodes`, and
    /// `pageInfo` with every canonical quantity name, so the GraphQL executor can
    /// render whatever shape an `inventoryItem.inventoryLevels(...)` selection asks
    /// for. Returns `None` when the item has no known levels, leaving
    /// the field absent exactly as before. The overlay product/variant/inventory-item
    /// read paths inject this onto the variant's inventory item before projection so a
    /// variant-backed `inventoryItem` resolves its levels rather than dropping them.
    pub(in crate::proxy) fn materialized_inventory_levels_value(
        &self,
        inventory_item_id: &str,
    ) -> Option<Value> {
        let levels = self.inventory_levels_for_item(inventory_item_id);
        if levels.is_empty() {
            return None;
        }
        let view = self.inventory_level_view_state();
        const CANONICAL: [&str; 8] = [
            "available",
            "on_hand",
            "committed",
            "incoming",
            "reserved",
            "damaged",
            "quality_control",
            "safety_stock",
        ];
        let mut edges = Vec::new();
        let mut nodes = Vec::new();
        for (location_id, quantities) in &levels {
            let key = (inventory_item_id.to_string(), location_id.clone());
            let level_id = view
                .level_id(&key)
                .cloned()
                .unwrap_or_else(|| inventory_level_id(inventory_item_id, location_id));
            let is_active = view.is_active(&key);
            let location = inventory_level_location_for_view(location_id, &view);
            let quantities_value: Vec<Value> = CANONICAL
                .iter()
                .map(|name| {
                    let updated_at = view
                        .quantity_updated_at(&(
                            inventory_item_id.to_string(),
                            location_id.clone(),
                            (*name).to_string(),
                        ))
                        .map_or(Value::Null, |value| json!(value));
                    json!({
                        "name": name,
                        "quantity": quantities.get(*name).copied().unwrap_or(0),
                        "updatedAt": updated_at
                    })
                })
                .collect();
            let cursor = self
                .store
                .staged
                .inventory_level_cursors
                .get(&level_id)
                .or_else(|| self.store.base.inventory_level_cursors.get(&level_id))
                .cloned();
            let node = json!({
                "id": level_id,
                "isActive": is_active,
                "item": { "id": inventory_item_id },
                "location": location,
                "quantities": quantities_value
            });
            match cursor {
                Some(cursor) => edges.push(json!({ "cursor": cursor, "node": node.clone() })),
                None => edges.push(json!({ "node": node.clone() })),
            }
            nodes.push(node);
        }
        let start_cursor = edges
            .first()
            .and_then(|edge| edge.get("cursor"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let end_cursor = edges
            .last()
            .and_then(|edge| edge.get("cursor"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(json!({
            "edges": edges,
            "nodes": nodes,
            "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
        }))
    }

    /// Clone a variant record and inject its materialized `inventoryLevels` connection
    /// onto the inventory item's extra fields, so overlay reads that project
    /// `inventoryItem.inventoryLevels` resolve from staged level state. A no-op clone
    /// when the item has no staged levels.
    pub(in crate::proxy) fn variant_with_inventory_levels(
        &self,
        variant: &ProductVariantRecord,
    ) -> ProductVariantRecord {
        let mut variant = variant.clone();
        if let Some(levels) = self.materialized_inventory_levels_value(&variant.inventory_item.id) {
            variant
                .inventory_item
                .extra_fields
                .insert("inventoryLevels".to_string(), levels);
        }
        variant
    }

    pub(in crate::proxy) fn active_inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        self.inventory_levels_for_item(inventory_item_id)
            .into_iter()
            .filter(|(location_id, _)| {
                self.inventory_level_is_active(&(
                    inventory_item_id.to_string(),
                    location_id.clone(),
                ))
            })
            .collect()
    }

    pub(in crate::proxy) fn inventory_total(&self, inventory_item_id: &str, name: &str) -> i64 {
        self.inventory_levels_for_item(inventory_item_id)
            .into_iter()
            .filter(|(location_id, _)| {
                self.inventory_level_is_active(&(
                    inventory_item_id.to_string(),
                    location_id.clone(),
                ))
            })
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    /// After an `available` inventory mutation, keep the owning variant's
    /// denormalized `inventoryQuantity` in lockstep with the summed available
    /// level so direct product/variant overlay reads reflect the new stock.
    /// Mirrors the sync `inventoryItemUpdate` and inventory-level item payloads
    /// already perform. No-op for non-`available` names (those don't feed
    /// `ProductVariant.inventoryQuantity`).
    fn sync_variant_available_quantity(
        &mut self,
        inventory_item_id: &str,
        name: &str,
        sync_product_aggregate: bool,
    ) {
        if name != "available" {
            return;
        }
        let Some(mut variant) = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .cloned()
        else {
            return;
        };
        variant.inventory_quantity = self.inventory_total(inventory_item_id, "available");
        let product_id = variant.product_id.clone();
        self.store.stage_product_variant(variant);
        if sync_product_aggregate {
            self.sync_product_inventory_aggregates(&product_id);
        }
    }

    pub(in crate::proxy) fn next_inventory_quantity_timestamp(&mut self) -> String {
        let sequence = self.store.staged.next_inventory_quantity_timestamp;
        self.store.staged.next_inventory_quantity_timestamp += 1;
        inventory_sequence_timestamp(sequence)
    }

    pub(super) fn stamp_inventory_quantity(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        name: &str,
        updated_at: &str,
    ) {
        self.store.staged.inventory_quantity_updated_at.insert(
            (
                inventory_item_id.to_string(),
                location_id.to_string(),
                name.to_string(),
            ),
            updated_at.to_string(),
        );
    }

    pub(in crate::proxy) fn decrement_inventory_item_available(
        &mut self,
        inventory_item_id: &str,
        quantity: i64,
    ) {
        if quantity <= 0 {
            return;
        }
        let location_id = self
            .active_inventory_levels_for_item(inventory_item_id)
            .first()
            .map(|(location_id, _)| location_id.clone())
            .or_else(|| {
                self.store
                    .staged
                    .inventory_levels
                    .keys()
                    .find(|(item_id, _)| item_id == inventory_item_id)
                    .map(|(_, location_id)| location_id.clone())
            })
            .or_else(|| self.default_inventory_location_id());
        let Some(location_id) = location_id else {
            return;
        };
        let updated_at = self.next_inventory_quantity_timestamp();
        let key = (inventory_item_id.to_string(), location_id.clone());
        self.stage_inventory_level_for_write(&key);
        {
            let level = self.store.staged.inventory_levels.entry(key).or_default();
            *level.entry("available".to_string()).or_insert(0) -= quantity;
            level.entry("on_hand".to_string()).or_insert(0);
            level.entry("damaged".to_string()).or_insert(0);
        }
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "available", &updated_at);
    }

    fn hydrate_inventory_quantity_rows(
        &mut self,
        request: &Request,
        rows: &[BTreeMap<String, ResolvedValue>],
        item_field: &str,
        location_field: &str,
    ) {
        let mut ids = Vec::new();
        for row in rows {
            for field in [item_field, location_field] {
                let id = resolved_string_field(row, field).unwrap_or_default();
                if !id.is_empty() && !ids.iter().any(|candidate| candidate == &id) {
                    ids.push(id);
                }
            }
        }
        self.hydrate_inventory_reference_ids(request, ids);
    }

    fn hydrate_inventory_move_rows(
        &mut self,
        request: &Request,
        changes: &[BTreeMap<String, ResolvedValue>],
    ) {
        let mut ids = Vec::new();
        for change in changes {
            let item_id = resolved_string_field(change, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty() && !ids.iter().any(|candidate| candidate == &item_id) {
                ids.push(item_id);
            }
            for container in ["from", "to"] {
                let object = resolved_object_field(change, container).unwrap_or_default();
                let location_id = resolved_string_field(&object, "locationId").unwrap_or_default();
                if !location_id.is_empty() && !ids.iter().any(|candidate| candidate == &location_id)
                {
                    ids.push(location_id);
                }
            }
        }
        self.hydrate_inventory_reference_ids(request, ids);
    }

    fn hydrate_inventory_reference_ids(&mut self, request: &Request, ids: Vec<String>) {
        if self.config.read_mode == ReadMode::Snapshot || ids.is_empty() {
            return;
        }
        let inventory_item_ids = ids
            .iter()
            .filter(|id| !is_synthetic_gid(id) && is_shopify_gid_of_type(id, "InventoryItem"))
            .cloned()
            .collect::<Vec<_>>();
        let mut item_hydration_failed = false;
        if !inventory_item_ids.is_empty() {
            let response = self.upstream_post(
                request,
                json!({
                    "query": INVENTORY_RICH_REFERENCE_HYDRATE_NODES_QUERY,
                    "variables": { "ids": inventory_item_ids }
                }),
            );
            let has_graphql_errors = response
                .body
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty());
            item_hydration_failed = response.status >= 400 || has_graphql_errors;
            if !item_hydration_failed {
                self.observe_inventory_transfer_hydration_response(&response.body);
            }
        }

        let portable_ids = ids
            .iter()
            .filter(|id| {
                !is_synthetic_gid(id)
                    && ((item_hydration_failed && is_shopify_gid_of_type(id, "InventoryItem"))
                        || (is_shopify_gid_of_type(id, "Location")
                            && !self.inventory_location_exists(id)))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !portable_ids.is_empty() {
            let response = self.upstream_post(
                request,
                json!({
                    "query": INVENTORY_TRANSFER_HYDRATE_NODES_QUERY,
                    "variables": { "ids": portable_ids }
                }),
            );
            if response.status < 400 {
                self.observe_inventory_transfer_hydration_response(&response.body);
            }
        }

        let inventory_level_ids = ids
            .into_iter()
            .filter(|id| !is_synthetic_gid(id) && is_shopify_gid_of_type(id, "InventoryLevel"))
            .collect::<Vec<_>>();
        if inventory_level_ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": INVENTORY_RICH_REFERENCE_HYDRATE_NODES_QUERY,
                "variables": { "ids": inventory_level_ids }
            }),
        );
        if response.status < 400 {
            self.observe_inventory_transfer_hydration_response(&response.body);
        }
    }

    pub(crate) fn inventory_set_quantities(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if inventory_set_requires_change_from(&invocation) && !ignore_compare {
            if let Some(error) = inventory_quantity_missing_change_from_error(
                &invocation,
                "InventoryQuantityInput",
                &quantities,
                "quantity",
            ) {
                return ResolverOutcome::value(Value::Null).with_errors(vec![error]);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(&input) {
            return ResolverOutcome::value(error_payload);
        }
        if !ignore_compare
            && quantities.iter().any(|quantity| {
                !quantity.contains_key("compareQuantity")
                    && !quantity.contains_key("changeFromQuantity")
            })
        {
            return ResolverOutcome::value(json!({
                "inventoryAdjustmentGroup": null,
                "userErrors": [user_error_omit_code(["input", "ignoreCompareQuantity"], "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.", None)]
            }));
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) = inventory_invalid_set_quantity_name_payload(&name) {
            return ResolverOutcome::value(error_payload);
        }
        if let Some(error_payload) = inventory_invalid_set_quantities_payload(&quantities, &name) {
            return ResolverOutcome::value(error_payload);
        }
        self.hydrate_inventory_quantity_rows(
            invocation.request,
            &quantities,
            "inventoryItemId",
            "locationId",
        );
        if let Some(error_payload) = self.inventory_existence_payload(&quantities, "quantities") {
            return ResolverOutcome::value(error_payload);
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for quantity in quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let new_quantity = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id.clone(), location_id.clone());
            let existed_before = self.effective_inventory_level(&key).is_some();
            self.stage_inventory_level_for_write(&key);
            let level = self
                .store
                .staged
                .inventory_levels
                .entry(key.clone())
                .or_default();
            let old = level.get(&name).copied().unwrap_or(0);
            let delta = new_quantity - old;
            level.insert(name.clone(), new_quantity);
            if name == "available" {
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                level.insert("on_hand".to_string(), old_on_hand + delta);
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                if delta != 0 {
                    on_hand_changes.push(inventory_change_json(
                        &item_id,
                        "on_hand",
                        delta,
                        None,
                        &location_id,
                        &location_name,
                    ));
                }
            }
            if !existed_before {
                self.store.staged.inventory_level_order.push(key.clone());
                self.record_inventory_level_id(&item_id, &location_id);
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &name, false);
            if delta != 0 {
                changes.push(inventory_change_json(
                    &item_id,
                    &name,
                    delta,
                    None,
                    &location_id,
                    &location_name,
                ));
            }
        }
        changes.extend(on_hand_changes);
        ResolverOutcome::value(
            self.inventory_adjustment_group_payload(updated_at, reason, reference, changes),
        )
        .with_log_draft(LogDraft::staged(
            "inventorySetQuantities",
            "products",
            Vec::new(),
        ))
    }

    pub(crate) fn inventory_set_on_hand_quantities(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if inventory_requires_idempotency(&invocation) && !invocation.has_directive("idempotent") {
            return ResolverOutcome::value(Value::Null)
                .with_errors(vec![inventory_idempotency_required_error(&invocation)]);
        }

        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let set_quantities = resolved_object_list_field(&input, "setQuantities");
        if inventory_set_requires_change_from(&invocation) {
            if let Some(error) = inventory_quantity_missing_change_from_error(
                &invocation,
                "InventorySetQuantityInput",
                &set_quantities,
                "quantity",
            ) {
                return ResolverOutcome::value(Value::Null).with_errors(vec![error]);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(&input) {
            return ResolverOutcome::value(error_payload);
        }
        if let Some(error_payload) =
            inventory_invalid_set_on_hand_quantities_payload(&set_quantities)
        {
            return ResolverOutcome::value(error_payload);
        }
        self.hydrate_inventory_quantity_rows(
            invocation.request,
            &set_quantities,
            "inventoryItemId",
            "locationId",
        );
        if let Some(error_payload) =
            self.inventory_existence_payload(&set_quantities, "setQuantities")
        {
            return ResolverOutcome::value(error_payload);
        }

        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for quantity in set_quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let new_on_hand = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id.clone(), location_id.clone());
            let existed_before = self.effective_inventory_level(&key).is_some();
            self.stage_inventory_level_for_write(&key);
            let delta = {
                let level = self
                    .store
                    .staged
                    .inventory_levels
                    .entry(key.clone())
                    .or_default();
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                let delta = new_on_hand - old_on_hand;
                let old_available = level.get("available").copied().unwrap_or(0);
                let available_after_change = old_available + delta;
                level.insert("available".to_string(), available_after_change);
                level.insert("on_hand".to_string(), new_on_hand);
                level.entry("damaged".to_string()).or_insert(0);
                delta
            };
            if !existed_before {
                self.store.staged.inventory_level_order.push(key.clone());
                self.record_inventory_level_id(&item_id, &location_id);
            }
            self.stamp_inventory_quantity(&item_id, &location_id, "available", &updated_at);
            self.store.staged.inventory_quantity_updated_at.remove(&(
                item_id.clone(),
                location_id.clone(),
                "on_hand".to_string(),
            ));
            self.sync_variant_available_quantity(&item_id, "available", true);
            changes.push(inventory_set_on_hand_change_json(
                &item_id,
                "available",
                delta,
                None,
                &location_id,
                &location_name,
            ));
            changes.push(inventory_change_json(
                &item_id,
                "on_hand",
                delta,
                None,
                &location_id,
                &location_name,
            ));
        }

        ResolverOutcome::value(
            self.inventory_adjustment_group_payload(updated_at, reason, reference, changes),
        )
        .with_log_draft(LogDraft::staged(
            "inventorySetOnHandQuantities",
            "products",
            Vec::new(),
        ))
    }

    pub(crate) fn inventory_adjust_quantities(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if inventory_adjust_requires_change_from(&invocation) {
            if let Some(error) = inventory_quantity_missing_change_from_error(
                &invocation,
                "InventoryChangeInput",
                &changes_input,
                "delta",
            ) {
                return ResolverOutcome::value(Value::Null).with_errors(vec![error]);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(&input) {
            return ResolverOutcome::value(error_payload);
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) =
            inventory_invalid_public_quantity_name_payload(&name, json!(["input", "name"]))
        {
            return ResolverOutcome::value(error_payload);
        }
        self.hydrate_inventory_quantity_rows(
            invocation.request,
            &changes_input,
            "inventoryItemId",
            "locationId",
        );
        if let Some(error_payload) =
            inventory_invalid_adjust_ledger_document_payload(&changes_input, &name)
        {
            return ResolverOutcome::value(error_payload);
        }
        if let Some(error_payload) = self.inventory_existence_payload(&changes_input, "changes") {
            return ResolverOutcome::value(error_payload);
        }
        if changes_input
            .iter()
            .all(|change| resolved_int_field(change, "delta").unwrap_or(0) == 0)
        {
            return ResolverOutcome::value(json!({
                "inventoryAdjustmentGroup": null,
                "userErrors": []
            }));
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for change in changes_input {
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&change, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let ledger = resolved_string_field(&change, "ledgerDocumentUri");
            let delta = resolved_int_field(&change, "delta").unwrap_or(0);
            if delta == 0 {
                continue;
            }
            self.record_inventory_level_id(&item_id, &location_id);
            self.stage_inventory_level_for_write(&(item_id.clone(), location_id.clone()));
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((item_id.clone(), location_id.clone()))
                .or_default();
            {
                let quantity = level.entry(name.clone()).or_insert(0);
                *quantity += delta;
            }
            if inventory_adjust_name_mirrors_on_hand(&name) {
                {
                    let on_hand = level.entry("on_hand".to_string()).or_insert(0);
                    *on_hand += delta;
                }
                if name == "available" {
                    level.entry("damaged".to_string()).or_insert(0);
                    self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                }
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    None,
                    &location_id,
                    &location_name,
                ));
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &name, false);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                ledger.as_deref(),
                &location_id,
                &location_name,
            ));
        }
        changes.extend(on_hand_changes);
        ResolverOutcome::value(
            self.inventory_adjustment_group_payload(updated_at, reason, reference, changes),
        )
        .with_log_draft(LogDraft::staged(
            "inventoryAdjustQuantities",
            "products",
            Vec::new(),
        ))
    }

    pub(crate) fn inventory_move_quantities(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if let Some(error_payload) = inventory_invalid_reason_payload(&input) {
            return ResolverOutcome::value(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                &from_name,
                json!(["input", "changes", index.to_string(), "from", "name"]),
            ) {
                return ResolverOutcome::value(error_payload);
            }
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                &to_name,
                json!(["input", "changes", index.to_string(), "to", "name"]),
            ) {
                return ResolverOutcome::value(error_payload);
            }
        }
        self.hydrate_inventory_move_rows(invocation.request, &changes_input);
        if let Some(error_payload) = self.inventory_move_existence_payload(&changes_input) {
            return ResolverOutcome::value(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return ResolverOutcome::value(json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [user_error_omit_code(json!(["input", "changes", index.to_string()]), "The quantities can't be moved between different locations.", None)]
                }));
            }
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut created_at = None;
        for change in changes_input {
            let updated_at = self.next_inventory_quantity_timestamp();
            if created_at.is_none() {
                created_at = Some(updated_at.clone());
            }
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(&change, "quantity").unwrap_or(0);
            let from = resolved_object_field(&change, "from").unwrap_or_default();
            let to = resolved_object_field(&change, "to").unwrap_or_default();
            let location_id = resolved_string_field(&from, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            let ledger = resolved_string_field(&to, "ledgerDocumentUri");
            {
                self.record_inventory_level_id(&item_id, &location_id);
                self.stage_inventory_level_for_write(&(item_id.clone(), location_id.clone()));
                let level = self
                    .store
                    .staged
                    .inventory_levels
                    .entry((item_id.clone(), location_id.clone()))
                    .or_default();
                {
                    let from_quantity = level.entry(from_name.clone()).or_insert(0);
                    *from_quantity -= quantity;
                }
                {
                    let to_quantity = level.entry(to_name.clone()).or_insert(0);
                    *to_quantity += quantity;
                }
                level.entry("on_hand".to_string()).or_insert(0);
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &from_name, &updated_at);
            self.stamp_inventory_quantity(&item_id, &location_id, &to_name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &from_name, true);
            self.sync_variant_available_quantity(&item_id, &to_name, true);
            changes.push(inventory_change_json(
                &item_id,
                &from_name,
                -quantity,
                None,
                &location_id,
                &location_name,
            ));
            changes.push(inventory_change_json(
                &item_id,
                &to_name,
                quantity,
                ledger.as_deref(),
                &location_id,
                &location_name,
            ));
        }
        let created_at = created_at.unwrap_or_else(|| self.next_inventory_quantity_timestamp());
        ResolverOutcome::value(
            self.inventory_adjustment_group_payload(created_at, reason, reference, changes),
        )
        .with_log_draft(LogDraft::staged(
            "inventoryMoveQuantities",
            "products",
            Vec::new(),
        ))
    }

    fn inventory_adjustment_group_payload(
        &mut self,
        created_at: String,
        reason: String,
        reference: String,
        changes: Vec<Value>,
    ) -> Value {
        let id = self.next_proxy_synthetic_gid("InventoryAdjustmentGroup");
        let group = json!({
            "__typename": "InventoryAdjustmentGroup",
            "id": id.clone(),
            "createdAt": created_at,
            "reason": reason,
            "referenceDocumentUri": reference,
            "changes": changes
        });
        self.store
            .staged
            .inventory_adjustment_groups
            .insert(id, group.clone());
        json!({
            "inventoryAdjustmentGroup": group,
            "userErrors": []
        })
    }

    fn inventory_existence_payload(
        &self,
        rows: &[BTreeMap<String, ResolvedValue>],
        list_key: &str,
    ) -> Option<Value> {
        let mut errors = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            self.push_inventory_item_existence_error(&mut errors, row, list_key, index);
            let location_id = resolved_string_field(row, "locationId").unwrap_or_default();
            self.push_inventory_location_existence_error(
                &mut errors,
                &location_id,
                list_key,
                index,
                &["locationId"],
            );
        }
        inventory_existence_error_payload(errors)
    }

    fn inventory_move_existence_payload(
        &self,
        changes: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Value> {
        let mut errors = Vec::new();
        for (index, change) in changes.iter().enumerate() {
            self.push_inventory_item_existence_error(&mut errors, change, "changes", index);
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let from_location_id = resolved_string_field(&from, "locationId").unwrap_or_default();
            self.push_inventory_location_existence_error(
                &mut errors,
                &from_location_id,
                "changes",
                index,
                &["from", "locationId"],
            );
            let to = resolved_object_field(change, "to").unwrap_or_default();
            let to_location_id = resolved_string_field(&to, "locationId").unwrap_or_default();
            self.push_inventory_location_existence_error(
                &mut errors,
                &to_location_id,
                "changes",
                index,
                &["to", "locationId"],
            );
        }
        inventory_existence_error_payload(errors)
    }

    fn push_inventory_item_existence_error(
        &self,
        errors: &mut Vec<Value>,
        row: &BTreeMap<String, ResolvedValue>,
        list_key: &str,
        index: usize,
    ) {
        let item_id = resolved_string_field(row, "inventoryItemId").unwrap_or_default();
        if !self.inventory_item_exists(&item_id) {
            errors.push(inventory_unknown_inventory_item_error(
                inventory_input_path(list_key, index, &["inventoryItemId"]),
            ));
        }
    }

    fn push_inventory_location_existence_error(
        &self,
        errors: &mut Vec<Value>,
        location_id: &str,
        list_key: &str,
        index: usize,
        field_path: &[&str],
    ) {
        if !self.inventory_location_exists(location_id) {
            errors.push(inventory_unknown_location_error(inventory_input_path(
                list_key, index, field_path,
            )));
        }
    }

    pub(crate) fn inventory_activate(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let inventory_item_id =
            resolved_string_field(&arguments, "inventoryItemId").unwrap_or_default();
        let location_id = resolved_string_field(&arguments, "locationId").unwrap_or_default();
        let has_available = arguments.contains_key("available");
        let available = resolved_int_field(&arguments, "available");
        let has_on_hand = arguments.contains_key("onHand");
        let on_hand = resolved_int_field(&arguments, "onHand");
        let mut user_errors = Vec::new();

        self.hydrate_inventory_reference_ids(
            invocation.request,
            vec![inventory_item_id.clone(), location_id.clone()],
        );

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(user_error_omit_code(
                vec!["inventoryItemId"],
                "The product couldn't be stocked because it wasn't found.",
                None,
            ));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }
        if available.is_some_and(|value| value < 0) {
            user_errors.push(user_error_omit_code(
                vec!["available"],
                "Available must be greater than or equal to 0",
                None,
            ));
        }
        let on_hand_out_of_range = on_hand.is_some_and(|value| {
            !(-INVENTORY_SET_QUANTITY_MAX..=INVENTORY_SET_QUANTITY_MAX).contains(&value)
        });
        if !self.inventory_location_exists(&location_id) {
            user_errors.push(user_error_omit_code(
                vec!["locationId"],
                "The product couldn't be stocked because the location wasn't found.",
                None,
            ));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }
        if !self.inventory_location_is_active(&location_id) {
            user_errors.push(user_error_omit_code(
                vec!["locationId"],
                "The product couldn't be stocked because the location is not active.",
                None,
            ));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }
        let location_name = self.inventory_location_display_name(&location_id);
        if has_available && has_on_hand {
            let message = format!(
                "The product couldn't be stocked at {location_name} because not allowed to set available and on_hand quantities at the same time."
            );
            user_errors.push(inventory_activate_user_error(vec!["available"], &message));
            user_errors.push(inventory_activate_user_error(vec!["onHand"], &message));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }
        if on_hand_out_of_range {
            let message = format!(
                "The product couldn't be stocked at {location_name} because the quantity needs to be between -1 billion and 1 billion."
            );
            user_errors.push(inventory_activate_user_error(vec!["onHand"], &message));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }

        let key = (inventory_item_id.clone(), location_id.clone());
        // The "already active" decision must be based on the level's state *before*
        // this call. A fresh activation (a brand-new level, or reactivating an
        // inactive one) is allowed to seed `available`; only a level that was
        // already active rejects it. Computing this up-front avoids the earlier bug
        // where pre-creating a default level flipped the flag and spuriously errored.
        let existed_before = self.effective_inventory_level(&key).is_some();
        let was_active = existed_before && self.inventory_level_is_active(&key);
        if was_active && has_available {
            user_errors.push(user_error_omit_code(
                vec!["available"],
                "Not allowed to set available quantity when the item is already active at the location.",
                None,
            ));
            let level = self.inventory_level_for_payload(&inventory_item_id, &location_id);
            return ResolverOutcome::value(self.inventory_activate_payload(level, user_errors));
        }
        if was_active && has_on_hand {
            user_errors.push(inventory_activate_user_error(
                vec!["onHand"],
                "Not allowed to set an on_hand quantity when the item is already active at the location.",
            ));
            let level = self.inventory_level_for_payload(&inventory_item_id, &location_id);
            return ResolverOutcome::value(self.inventory_activate_payload(level, user_errors));
        }
        if !was_active
            && self
                .active_inventory_levels_for_item(&inventory_item_id)
                .len()
                >= INVENTORY_MAX_ACTIVE_LEVELS
        {
            user_errors.push(user_error_omit_code(
                vec!["locationId"],
                "The product couldn't be stocked because it has reached the maximum number of inventory locations.",
                None,
            ));
            return ResolverOutcome::value(self.inventory_activate_payload(None, user_errors));
        }

        if !was_active {
            if !existed_before {
                self.store.staged.inventory_level_order.push(key.clone());
            }
            self.activate_inventory_level(&inventory_item_id, &location_id);
            // A first-time activation with `available` seeds both available and
            // on_hand to that value. Reactivating an existing (inactive) level must
            // preserve its prior quantities, so only seed on a brand-new level.
            if !existed_before {
                let available_seed = available.filter(|value| *value >= 0);
                let on_hand_seed = on_hand.filter(|value| {
                    (-INVENTORY_SET_QUANTITY_MAX..=INVENTORY_SET_QUANTITY_MAX).contains(value)
                });
                if let Some(value) = available_seed.or(on_hand_seed) {
                    let updated_at = self.next_inventory_quantity_timestamp();
                    if let Some(level) = self.store.staged.inventory_levels.get_mut(&key) {
                        level.insert("available".to_string(), value);
                        level.insert("on_hand".to_string(), value);
                    }
                    self.stamp_inventory_quantity(
                        &inventory_item_id,
                        &location_id,
                        "available",
                        &updated_at,
                    );
                    if available_seed.is_some() {
                        self.stamp_inventory_quantity(
                            &inventory_item_id,
                            &location_id,
                            "on_hand",
                            &updated_at,
                        );
                    }
                }
            }
        }
        let level = self.inventory_level_for_payload(&inventory_item_id, &location_id);
        ResolverOutcome::value(self.inventory_activate_payload(level, user_errors)).with_log_draft(
            LogDraft::staged("inventoryActivate", "products", vec![inventory_item_id]),
        )
    }

    pub(crate) fn inventory_deactivate(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let inventory_level_id =
            resolved_string_field(&arguments, "inventoryLevelId").unwrap_or_default();
        let mut user_errors = Vec::new();
        let inventory_item_hint =
            inventory_level_id_tail_and_query(&inventory_level_id).map(|(_, query)| {
                if is_shopify_gid_of_type(query, "InventoryItem") {
                    query.to_string()
                } else {
                    shopify_gid("InventoryItem", query)
                }
            });
        self.hydrate_inventory_reference_ids(invocation.request, vec![inventory_level_id.clone()]);
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(&inventory_level_id)
        else {
            let message = if inventory_item_hint
                .as_deref()
                .is_some_and(|item_id| self.inventory_item_exists(item_id))
            {
                "The product couldn't be unstocked because the location was deleted."
            } else {
                "The product couldn't be unstocked because the product was deleted."
            };
            user_errors.push(inventory_deactivate_user_error(message));
            return ResolverOutcome::value(self.inventory_deactivate_payload(user_errors));
        };
        let key = (inventory_item_id.clone(), location_id.clone());
        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the product was deleted.",
            ));
        } else if self.effective_inventory_level(&key).is_none() {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the location was deleted.",
            ));
        }
        if user_errors.is_empty()
            && self
                .active_inventory_levels_for_item(&inventory_item_id)
                .len()
                <= 1
            && self.inventory_level_is_active(&key)
        {
            user_errors.push(inventory_deactivate_user_error(
                &format!(
                    "The product couldn't be unstocked from {} because products need to be stocked at a minimum of 1 location.",
                    self.inventory_location_display_name(&location_id)
                ),
            ));
        }
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_deactivate_payload(user_errors));
        }

        self.store.staged.active_inventory_levels.remove(&key);
        self.store.staged.inactive_inventory_levels.insert(key);
        ResolverOutcome::value(self.inventory_deactivate_payload(user_errors)).with_log_draft(
            LogDraft::staged("inventoryDeactivate", "products", vec![inventory_level_id]),
        )
    }

    pub(crate) fn inventory_bulk_toggle_activation(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let inventory_item_id =
            resolved_string_field(&arguments, "inventoryItemId").unwrap_or_default();
        let updates = resolved_object_list_field(&arguments, "inventoryItemUpdates");
        let mut changed_levels = Vec::new();
        let mut user_errors = Vec::new();

        let mut hydration_ids = vec![inventory_item_id.clone()];
        hydration_ids.extend(
            updates
                .iter()
                .filter_map(|update| resolved_string_field(update, "locationId")),
        );
        self.hydrate_inventory_reference_ids(invocation.request, hydration_ids);

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(user_error_omit_code(
                vec!["inventoryItemId".to_string()],
                "The inventory item couldn't be found.",
                Some("INVENTORY_ITEM_NOT_FOUND"),
            ));
            return ResolverOutcome::value(self.inventory_bulk_toggle_payload(
                None,
                None,
                user_errors,
            ));
        }

        for (index, update) in updates.iter().enumerate() {
            let location_id = resolved_string_field(update, "locationId").unwrap_or_default();
            let activate = resolved_bool_field(update, "activate").unwrap_or(true);
            let location_path = vec![
                "inventoryItemUpdates".to_string(),
                index.to_string(),
                "locationId".to_string(),
            ];
            if !self.inventory_location_exists(&location_id)
                || !self.inventory_location_is_active(&location_id)
            {
                user_errors.push(user_error_omit_code(
                    location_path.clone(),
                    "The quantity couldn't be updated because the location was not found.",
                    Some("LOCATION_NOT_FOUND"),
                ));
                return ResolverOutcome::value(self.inventory_bulk_toggle_payload(
                    None,
                    None,
                    user_errors,
                ));
            }
            let key = (inventory_item_id.clone(), location_id.clone());
            let is_active = self.effective_inventory_level(&key).is_some()
                && self.inventory_level_is_active(&key);
            if !is_active
                && self
                    .active_inventory_levels_for_item(&inventory_item_id)
                    .is_empty()
            {
                self.ensure_default_inventory_level(&inventory_item_id, &location_id);
            }
            let is_active = self.effective_inventory_level(&key).is_some()
                && self.inventory_level_is_active(&key);
            if activate {
                if !is_active {
                    self.activate_inventory_level(&inventory_item_id, &location_id);
                }
                if let Some(level) =
                    self.inventory_level_for_payload(&inventory_item_id, &location_id)
                {
                    changed_levels.push(level);
                }
            } else {
                if self
                    .active_inventory_levels_for_item(&inventory_item_id)
                    .len()
                    <= 1
                    && is_active
                {
                    user_errors.push(user_error_omit_code(
                        location_path.clone(),
                        &format!(
                            "The variant couldn't be unstocked from {} because products need to be stocked at a minimum of 1 location.",
                            self.inventory_location_display_name(&location_id)
                        ),
                        Some("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION"),
                    ));
                    return ResolverOutcome::value(self.inventory_bulk_toggle_payload(
                        None,
                        None,
                        user_errors,
                    ));
                }
                if is_active {
                    self.store.staged.active_inventory_levels.remove(&key);
                    self.store.staged.inactive_inventory_levels.insert(key);
                }
            }
        }

        let item = Some(self.inventory_item_canonical_value(&inventory_item_id));
        ResolverOutcome::value(self.inventory_bulk_toggle_payload(
            item,
            Some(changed_levels),
            user_errors,
        ))
        .with_log_draft(LogDraft::staged(
            "inventoryBulkToggleActivation",
            "products",
            vec![inventory_item_id],
        ))
    }

    pub(crate) fn inventory_item_update(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let user_errors = inventory_item_update_user_errors(&input);
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_item_update_payload(None, user_errors));
        }
        self.hydrate_inventory_reference_ids(invocation.request, vec![id.clone()]);
        let Some(mut variant) = self
            .store
            .product_variant_by_inventory_item_id(&id)
            .cloned()
        else {
            return ResolverOutcome::value(self.inventory_item_update_payload(
                None,
                vec![user_error_omit_code(
                    inventory_item_update_field_path(&["id"]),
                    "The product couldn't be updated because it does not exist.",
                    None,
                )],
            ));
        };

        self.apply_inventory_item_update_input(&mut variant, &input);
        let inventory_item_id = variant.inventory_item.id.clone();
        let product_id = variant.product_id.clone();
        self.stage_inventory_item_variant_update(variant);
        let inventory_item = self.inventory_item_canonical_value(&inventory_item_id);
        ResolverOutcome::value(self.inventory_item_update_payload(Some(inventory_item), Vec::new()))
            .with_log_draft(LogDraft::staged(
                "inventoryItemUpdate",
                "products",
                vec![product_id],
            ))
    }

    pub(in crate::proxy) fn inventory_item_exists(&self, inventory_item_id: &str) -> bool {
        if inventory_item_id.is_empty()
            || !is_shopify_gid_of_type(inventory_item_id, "InventoryItem")
        {
            return false;
        }
        !self.inventory_item_is_tombstoned(inventory_item_id)
            && (self
                .store
                .product_variant_by_inventory_item_id(inventory_item_id)
                .is_some()
                || self
                    .store
                    .staged
                    .inventory_levels
                    .keys()
                    .chain(self.store.base.inventory_levels.keys())
                    .any(|(item_id, _)| item_id == inventory_item_id))
    }

    fn inventory_location_exists(&self, location_id: &str) -> bool {
        if location_id.is_empty() || !is_shopify_gid_of_type(location_id, "Location") {
            return false;
        }
        self.inventory_location_has_local_state(location_id)
    }

    fn inventory_location_has_local_state(&self, location_id: &str) -> bool {
        self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .observed_shipping_locations
                .contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .chain(self.store.base.inventory_levels.keys())
                .any(|(_, staged_location_id)| staged_location_id == location_id)
            || self.store.base.locations.get(location_id).is_some()
    }

    fn inventory_location_is_active(&self, location_id: &str) -> bool {
        self.location_for_read(location_id)
            .and_then(|location| location.get("isActive").and_then(Value::as_bool))
            .unwrap_or(true)
    }

    pub(in crate::proxy) fn inventory_location_display_name(&self, location_id: &str) -> String {
        self.location_for_read(location_id)
            .and_then(|location| {
                location
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| location_id.to_string())
    }

    pub(in crate::proxy) fn inventory_level_parts_from_id_or_fallback(
        &self,
        id: &str,
    ) -> Option<(String, String)> {
        let (_, query) = inventory_level_id_tail_and_query(id)?;
        let inventory_item_id = if is_shopify_gid_of_type(query, "InventoryItem") {
            query.to_string()
        } else {
            shopify_gid("InventoryItem", query)
        };
        if let Some(((item_id, location_id), _)) = self
            .store
            .staged
            .inventory_level_ids
            .iter()
            .chain(self.store.base.inventory_level_ids.iter())
            .find(|(_, observed_id)| observed_id.as_str() == id)
        {
            return Some((item_id.clone(), location_id.clone()));
        }
        if let Some((item_id, location_id)) = self
            .store
            .staged
            .inventory_levels
            .keys()
            .chain(self.store.base.inventory_levels.keys())
            .find(|(item_id, location_id)| inventory_level_id(item_id, location_id) == id)
        {
            return Some((item_id.clone(), location_id.clone()));
        }
        if let Some((_, location_id)) = inventory_level_parts_from_id(id) {
            return Some((inventory_item_id, location_id));
        }
        None
    }

    pub(super) fn default_inventory_location_id(&self) -> Option<String> {
        self.first_active_location_from_order(
            &self.store.staged.locations.order,
            &self.store.staged.locations.records,
        )
        .or_else(|| {
            self.first_active_location_from_order(
                &self.store.base.locations.order,
                &self.store.base.locations.records,
            )
        })
        .or_else(|| {
            self.first_active_location_from_order(
                &self.store.staged.observed_shipping_location_order,
                &self.store.staged.observed_shipping_locations,
            )
        })
        .or_else(|| {
            self.first_active_location_from_order(
                &self.store.staged.fulfillment_service_locations.order,
                &self.store.staged.fulfillment_service_locations.records,
            )
        })
        .or_else(|| {
            self.store
                .staged
                .inventory_level_order
                .iter()
                .chain(self.store.base.inventory_level_order.iter())
                .map(|(_, location_id)| location_id)
                .find(|location_id| self.inventory_location_exists(location_id))
                .cloned()
        })
        .or_else(|| {
            self.store
                .staged
                .inventory_levels
                .keys()
                .chain(self.store.base.inventory_levels.keys())
                .map(|(_, location_id)| location_id)
                .find(|location_id| self.inventory_location_exists(location_id))
                .cloned()
        })
    }

    fn first_active_location_from_order(
        &self,
        order: &[String],
        records: &BTreeMap<String, Value>,
    ) -> Option<String> {
        order
            .iter()
            .filter_map(|id| records.get(id).map(|record| (id, record)))
            .find(|(_, record)| {
                record
                    .get("isActive")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            })
            .map(|(id, _)| id.clone())
            .or_else(|| {
                records
                    .iter()
                    .find(|(_, record)| {
                        record
                            .get("isActive")
                            .and_then(Value::as_bool)
                            .unwrap_or(true)
                    })
                    .map(|(id, _)| id.clone())
            })
    }

    fn ensure_default_inventory_level(
        &mut self,
        inventory_item_id: &str,
        requested_location_id: &str,
    ) {
        if !self.inventory_item_exists(inventory_item_id) {
            return;
        }
        let location_id = if self.inventory_location_exists(requested_location_id)
            && is_shopify_gid_of_type(requested_location_id, "Location")
        {
            requested_location_id.to_string()
        } else {
            let Some(location_id) = self.default_inventory_location_id() else {
                return;
            };
            location_id
        };
        let key = (inventory_item_id.to_string(), location_id);
        self.stage_inventory_level_for_write(&key);
        self.store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities);
    }

    fn record_inventory_level_id(&mut self, inventory_item_id: &str, location_id: &str) {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        let base_id = self.store.base.inventory_level_ids.get(&key).cloned();
        self.store
            .staged
            .inventory_level_ids
            .entry(key)
            .or_insert_with(|| {
                base_id.unwrap_or_else(|| inventory_level_id(inventory_item_id, location_id))
            });
    }

    fn activate_inventory_level(&mut self, inventory_item_id: &str, location_id: &str) {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        self.stage_inventory_level_for_write(&key);
        self.store.staged.inactive_inventory_levels.remove(&key);
        self.store
            .staged
            .active_inventory_levels
            .insert(key.clone());
        self.record_inventory_level_id(inventory_item_id, location_id);
        self.store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities)
            .entry("incoming".to_string())
            .or_insert(0);
        let updated_at = self.next_inventory_quantity_timestamp();
        self.stamp_inventory_quantity(inventory_item_id, location_id, "available", &updated_at);
    }

    fn inventory_level_for_payload(
        &self,
        inventory_item_id: &str,
        location_id: &str,
    ) -> Option<Value> {
        let quantities = self
            .effective_inventory_level(&(inventory_item_id.to_string(), location_id.to_string()))?;
        Some(self.inventory_level_canonical_value(inventory_item_id, location_id, quantities))
    }

    pub(in crate::proxy) fn inventory_node_value_by_id(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id)? {
            "InventoryItem" => Some(if self.inventory_item_exists(id) {
                self.inventory_item_canonical_value(id)
            } else {
                Value::Null
            }),
            "InventoryLevel" => Some(self.inventory_level_value_by_id(id)),
            "InventoryQuantity" => Some(self.inventory_quantity_value_by_id(id)),
            "InventoryAdjustmentGroup" => Some(
                self.store
                    .staged
                    .inventory_adjustment_groups
                    .get(id)
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            "InventoryTransfer" => Some(self.inventory_transfer_value_by_id(id)),
            "InventoryTransferLineItem" => Some(self.inventory_transfer_line_item_value_by_id(id)),
            "InventoryShipment" => Some(self.inventory_shipment_value_by_id(id)),
            "InventoryShipmentLineItem" => Some(self.inventory_shipment_line_item_value_by_id(id)),
            _ => None,
        }
    }

    fn inventory_activate_payload(
        &self,
        inventory_level: Option<Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "inventoryLevel": inventory_level.unwrap_or(Value::Null),
            "userErrors": user_errors,
        })
    }

    fn inventory_deactivate_payload(&self, user_errors: Vec<Value>) -> Value {
        json!({ "userErrors": user_errors })
    }

    fn inventory_bulk_toggle_payload(
        &self,
        inventory_item: Option<Value>,
        inventory_levels: Option<Vec<Value>>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "inventoryItem": inventory_item.unwrap_or(Value::Null),
            "inventoryLevels": inventory_levels.map(Value::Array).unwrap_or(Value::Null),
            "userErrors": user_errors,
        })
    }

    fn inventory_item_update_payload(
        &self,
        inventory_item: Option<Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "inventoryItem": inventory_item.unwrap_or(Value::Null),
            "userErrors": user_errors,
        })
    }

    fn apply_inventory_item_update_input(
        &self,
        variant: &mut ProductVariantRecord,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        if let Some(tracked) = resolved_bool_field(input, "tracked") {
            variant.inventory_item.tracked = tracked;
        }
        if let Some(requires_shipping) = resolved_bool_field(input, "requiresShipping") {
            variant.inventory_item.requires_shipping = requires_shipping;
        }
        for field_name in ["countryCodeOfOrigin", "provinceCodeOfOrigin", "measurement"] {
            if let Some(value) = input.get(field_name) {
                variant
                    .inventory_item
                    .extra_fields
                    .insert(field_name.to_string(), resolved_value_json(value));
            }
        }
        if let Some(value) = input.get("harmonizedSystemCode") {
            variant.inventory_item.extra_fields.insert(
                "harmonizedSystemCode".to_string(),
                resolved_harmonized_system_code_json(value),
            );
        }
        if let Some(value) = input.get("cost") {
            variant
                .inventory_item
                .extra_fields
                .insert("cost".to_string(), resolved_value_json(value));
        }
        if let Some(value) = input.get("countryHarmonizedSystemCodes") {
            variant.inventory_item.extra_fields.insert(
                "countryHarmonizedSystemCodes".to_string(),
                resolved_value_json(value),
            );
        }
    }

    fn stage_inventory_item_variant_update(&mut self, mut variant: ProductVariantRecord) {
        if let Some(product) = self.store.product_by_id(&variant.product_id) {
            if variant.inventory_item.tracked
                && product.variants.is_empty()
                && product.total_inventory == 0
            {
                let mut staged_product = product.clone();
                staged_product.tracks_inventory = true;
                self.store.stage_product(staged_product);
            }
        }
        variant.inventory_quantity = self.inventory_total(&variant.inventory_item.id, "available");
        self.store.stage_product_variant(variant);
    }
}

fn inventory_location_level_sort_key(
    level: &InventoryLocationLevelRecord,
    sort_key: Option<&str>,
) -> StagedSortKey {
    match sort_key.unwrap_or("ID") {
        "INVENTORY_ITEM_ID" => inventory_gid_sort_key(&level.inventory_item_id),
        "LOCATION_ID" => inventory_gid_sort_key(&level.location_id),
        "ID" | "RELEVANCE" => level
            .level_id
            .as_deref()
            .map(inventory_gid_sort_key)
            .unwrap_or_else(|| {
                inventory_gid_sort_key(&inventory_level_id(
                    &level.inventory_item_id,
                    &level.location_id,
                ))
            }),
        _ => level
            .level_id
            .as_deref()
            .map(inventory_gid_sort_key)
            .unwrap_or_else(|| {
                inventory_gid_sort_key(&inventory_level_id(
                    &level.inventory_item_id,
                    &level.location_id,
                ))
            }),
    }
}

use super::*;

impl DraftProxy {
    pub(crate) fn inventory_transfer_create(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let ready_to_ship = invocation.root_name == "inventoryTransferCreateAsReadyToShip";
        let origin_location_id =
            resolved_string_field(&input, "originLocationId").unwrap_or_default();
        let destination_location_id =
            resolved_string_field(&input, "destinationLocationId").unwrap_or_default();
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            invocation.request,
            [&origin_location_id, &destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_transfer_user_error_payload(
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store.inventory_transfer_count().saturating_add(1)
        );
        let mut line_items = Vec::new();
        for item_input in line_item_inputs {
            line_items.push(InventoryTransferLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                inventory_item_id: resolved_string_field(&item_input, "inventoryItemId")
                    .unwrap_or_default(),
                quantity: resolved_int_field(&item_input, "quantity").unwrap_or(0),
            });
        }
        let record = InventoryTransferRecord {
            id: id.clone(),
            name,
            created_at: resolved_string_field(&input, "dateCreated").unwrap_or_else(|| {
                inventory_transfer_default_created_at(self.store.inventory_transfer_count())
            }),
            status: if ready_to_ship {
                "READY_TO_SHIP".to_string()
            } else {
                "DRAFT".to_string()
            },
            origin_location_id,
            destination_location_id,
            tags: inventory_transfer_tags_from_input(&input),
            line_items,
        };
        self.ensure_transfer_inventory_levels(&record);
        if ready_to_ship {
            self.apply_transfer_reservations(&record, 1);
        }
        let payload = self.inventory_transfer_payload_value(&record, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            invocation.root_name,
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_mark_ready(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "DRAFT" {
            self.apply_transfer_reservations(&record, 1);
        }
        record.status = "READY_TO_SHIP".to_string();
        let payload = self.inventory_transfer_payload_value(&record, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferMarkAsReadyToShip",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_set_items(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let mut record = existing;
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            invocation.request,
            [&record.origin_location_id, &record.destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &record.origin_location_id,
            &record.destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_transfer_user_error_payload(
                "inventoryTransfer",
                &["updatedLineItems"],
                user_errors,
            ));
        }
        let mut updated = Vec::new();
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(&item_input, "inventoryItemId").unwrap_or_default();
            let new_quantity = resolved_int_field(&item_input, "quantity").unwrap_or(0);
            let mut old_quantity = 0;
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.inventory_item_id == item_id)
            {
                old_quantity = line_item.quantity;
                line_item.quantity = new_quantity;
            } else {
                record.line_items.push(InventoryTransferLineItemRecord {
                    id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                    inventory_item_id: item_id.clone(),
                    quantity: new_quantity,
                });
            }
            let delta = new_quantity - old_quantity;
            if record.status == "READY_TO_SHIP" {
                self.apply_inventory_reservation(&item_id, &record.origin_location_id, delta);
            }
            updated.push(json!({
                "inventoryItemId": item_id,
                "newQuantity": new_quantity,
                "deltaQuantity": delta
            }));
        }
        let payload = json!({
            "inventoryTransfer": self.inventory_transfer_full_json(&record),
            "updatedLineItems": updated,
            "userErrors": []
        });
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferSetItems",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_edit(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let origin_location_id = resolved_string_field(&input, "originId")
            .unwrap_or_else(|| existing.origin_location_id.clone());
        let destination_location_id = resolved_string_field(&input, "destinationId")
            .unwrap_or_else(|| existing.destination_location_id.clone());
        let line_item_inputs = existing
            .line_items
            .iter()
            .map(|line_item| {
                BTreeMap::from([
                    (
                        "inventoryItemId".to_string(),
                        ResolvedValue::String(line_item.inventory_item_id.clone()),
                    ),
                    (
                        "quantity".to_string(),
                        ResolvedValue::Int(line_item.quantity),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        self.hydrate_inventory_transfer_references(
            invocation.request,
            [&origin_location_id, &destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_transfer_user_error_payload(
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }

        let was_ready = existing.status == "READY_TO_SHIP";
        if was_ready {
            self.apply_transfer_reservations(&existing, -1);
        }
        let mut record = existing;
        record.origin_location_id = origin_location_id;
        record.destination_location_id = destination_location_id;
        if let Some(date_created) = resolved_string_field(&input, "dateCreated") {
            record.created_at = date_created;
        }
        if input.contains_key("tags") {
            record.tags = inventory_transfer_tags_from_input(&input);
        }
        self.ensure_transfer_inventory_levels(&record);
        if was_ready {
            self.apply_transfer_reservations(&record, 1);
        }

        let payload = self.inventory_transfer_payload_value(&record, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferEdit",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_duplicate(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let line_item_inputs = existing
            .line_items
            .iter()
            .map(|line_item| {
                BTreeMap::from([
                    (
                        "inventoryItemId".to_string(),
                        ResolvedValue::String(line_item.inventory_item_id.clone()),
                    ),
                    (
                        "quantity".to_string(),
                        ResolvedValue::Int(line_item.quantity),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        let user_errors = self.inventory_transfer_validate(
            &existing.origin_location_id,
            &existing.destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.inventory_transfer_user_error_payload(
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }

        let new_id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store.inventory_transfer_count().saturating_add(1)
        );
        let record = InventoryTransferRecord {
            id: new_id.clone(),
            name,
            created_at: inventory_transfer_default_created_at(
                self.store.inventory_transfer_count(),
            ),
            status: "DRAFT".to_string(),
            origin_location_id: existing.origin_location_id,
            destination_location_id: existing.destination_location_id,
            tags: existing.tags,
            line_items: existing
                .line_items
                .into_iter()
                .map(|line_item| InventoryTransferLineItemRecord {
                    id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                    inventory_item_id: line_item.inventory_item_id,
                    quantity: line_item.quantity,
                })
                .collect(),
        };
        self.ensure_transfer_inventory_levels(&record);
        let payload = self.inventory_transfer_payload_value(&record, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(new_id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferDuplicate",
            "products",
            vec![new_id],
        ))
    }

    pub(crate) fn inventory_transfer_remove_items(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let mut record = existing;
        let remove_ids = resolved_string_list_field(&input, "transferLineItemIds");
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for line_item in record.line_items {
            if remove_ids.iter().any(|id| id == &line_item.id) {
                if record.status == "READY_TO_SHIP" {
                    self.apply_inventory_reservation(
                        &line_item.inventory_item_id,
                        &record.origin_location_id,
                        -line_item.quantity,
                    );
                }
                removed.push(json!({
                    "inventoryItemId": line_item.inventory_item_id,
                    "newQuantity": 0,
                    "deltaQuantity": -line_item.quantity
                }));
            } else {
                kept.push(line_item);
            }
        }
        record.line_items = kept;
        let payload = json!({
            "inventoryTransfer": self.inventory_transfer_full_json(&record),
            "removedQuantities": removed,
            "userErrors": []
        });
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferRemoveItems",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_cancel(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(existing) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_transfer_missing_payload("inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "READY_TO_SHIP" {
            self.apply_transfer_reservations(&record, -1);
        }
        record.status = "CANCELED".to_string();
        let payload = self.inventory_transfer_payload_value(&record, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryTransferCancel",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_transfer_delete(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if let Err(outcome) =
            self.hydrate_inventory_transfer_mutation_target(invocation.request, &id)
        {
            return outcome;
        }
        let Some(record) = self.store.inventory_transfer_by_id(&id).cloned() else {
            return ResolverOutcome::value(self.inventory_transfer_missing_payload("deletedId"));
        };
        if record.status != "DRAFT" {
            return ResolverOutcome::value(json!({
                "deletedId": Value::Null,
                "userErrors": [user_error_omit_code(["id"], "Can't delete the transfer if it's not in the draft status.", None)]
            }));
        }
        self.store.staged.inventory_transfers.remove(&id);
        self.store.staged.inventory_transfers.tombstone(id.clone());
        ResolverOutcome::value(json!({ "deletedId": id, "userErrors": [] })).with_log_draft(
            LogDraft::staged("inventoryTransferDelete", "products", Vec::new()),
        )
    }

    fn inventory_transfer_payload_value(
        &self,
        record: &InventoryTransferRecord,
        transfer_field: &str,
    ) -> Value {
        json!({
            transfer_field: self.inventory_transfer_full_json(record),
            "userErrors": []
        })
    }

    fn inventory_transfer_user_error_payload(
        &self,
        transfer_field: &str,
        extra_null_fields: &[&str],
        user_errors: Vec<Value>,
    ) -> Value {
        let mut payload = serde_json::Map::new();
        payload.insert(transfer_field.to_string(), Value::Null);
        for field in extra_null_fields {
            payload.insert((*field).to_string(), Value::Null);
        }
        payload.insert("userErrors".to_string(), Value::Array(user_errors));
        Value::Object(payload)
    }

    fn inventory_transfer_missing_payload(&self, transfer_field: &str) -> Value {
        json!({
            transfer_field: Value::Null,
            "userErrors": [user_error(
                ["id"],
                "Inventory transfer not found.",
                Some("TRANSFER_NOT_FOUND")
            )]
        })
    }

    pub(super) fn inventory_transfer_value_by_id(&self, id: &str) -> Value {
        self.store
            .inventory_transfer_by_id(id)
            .map(|record| self.inventory_transfer_full_json(record))
            .unwrap_or(Value::Null)
    }

    pub(super) fn inventory_transfers_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        staged_connection_value_with_args(
            self.store.inventory_transfers(),
            arguments,
            |record, query| self.inventory_transfer_search_decision(record, query),
            |record, sort_key| self.inventory_transfer_sort_key(record, sort_key),
            |record| self.inventory_transfer_full_json(record),
            |record| record.id.clone(),
        )
    }

    pub(super) fn inventory_transfer_line_item_value_by_id(&self, id: &str) -> Value {
        self.store
            .inventory_transfers()
            .iter()
            .find_map(|record| {
                record
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == id)
                    .map(|line_item| self.inventory_transfer_line_item_full_json(record, line_item))
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn observe_inventory_transfer_read_response(&mut self, body: &Value) {
        self.observe_inventory_transfer_value(body);
    }

    pub(super) fn observe_inventory_transfer_value(&mut self, value: &Value) {
        if let Some(record) = inventory_transfer_record_from_json(value) {
            let transfer_id = record.id.clone();
            self.store.observe_base_inventory_transfer(record);
            if let Some(location) = value.get("origin") {
                let location = location.get("location").unwrap_or(location);
                self.merge_staged_location(
                    location,
                    &[("__typename", json!("Location")), ("isActive", json!(true))],
                );
            }
            if let Some(location) = value.get("destination") {
                let location = location.get("location").unwrap_or(location);
                self.merge_staged_location(
                    location,
                    &[("__typename", json!("Location")), ("isActive", json!(true))],
                );
            }
            for line_item in connection_node_values(value.get("lineItems")) {
                if let Some(inventory_item) = line_item.get("inventoryItem") {
                    self.observe_inventory_item_node(inventory_item);
                }
            }
            self.observe_inventory_shipments_for_transfer(value, &transfer_id);
        }
        match value {
            Value::Array(items) => {
                for item in items {
                    self.observe_inventory_transfer_value(item);
                }
            }
            Value::Object(object) => {
                for child in object.values() {
                    self.observe_inventory_transfer_value(child);
                }
            }
            _ => {}
        }
    }
    fn inventory_transfer_full_json(&self, record: &InventoryTransferRecord) -> Value {
        let status = self.inventory_transfer_effective_status(record);
        let origin_location = self
            .location_for_read(&record.origin_location_id)
            .unwrap_or_else(|| {
                json!({
                    "__typename": "Location",
                    "id": record.origin_location_id,
                    "name": self.inventory_location_display_name(&record.origin_location_id),
                    "isActive": true
                })
            });
        let destination_location = self
            .location_for_read(&record.destination_location_id)
            .unwrap_or_else(|| {
                json!({
                    "__typename": "Location",
                    "id": record.destination_location_id,
                    "name": self.inventory_location_display_name(&record.destination_location_id),
                    "isActive": true
                })
            });
        let origin_name = origin_location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&record.origin_location_id);
        let destination_name = destination_location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&record.destination_location_id);
        let nodes = record
            .line_items
            .iter()
            .map(|line_item| self.inventory_transfer_line_item_full_json(record, line_item))
            .collect::<Vec<_>>();
        let shipments = self
            .store
            .inventory_shipments()
            .into_iter()
            .filter(|shipment| shipment.transfer_id.as_deref() == Some(record.id.as_str()))
            .map(|shipment| self.inventory_shipment_full_json(&shipment))
            .collect::<Vec<_>>();
        json!({
            "__typename": "InventoryTransfer",
            "id": record.id,
            "name": record.name,
            "dateCreated": record.created_at,
            "status": status,
            "origin": {
                "name": origin_name,
                "location": origin_location
            },
            "destination": {
                "name": destination_name,
                "location": destination_location
            },
            "tags": record.tags,
            "totalQuantity": record.line_items.iter().map(|line_item| line_item.quantity).sum::<i64>(),
            "lineItemsCount": count_object(record.line_items.len()),
            "lineItems": connection_json(nodes),
            "shipments": connection_json(shipments)
        })
    }

    fn inventory_transfer_effective_status(&self, record: &InventoryTransferRecord) -> String {
        let has_shipped_line = record.line_items.iter().any(|line_item| {
            let (shipped, _) = self.transfer_line_shipment_quantities(&record.id, line_item, None);
            shipped > 0
        });
        if record.status == "READY_TO_SHIP" && has_shipped_line {
            "IN_PROGRESS".to_string()
        } else {
            record.status.clone()
        }
    }

    fn inventory_transfer_line_item_full_json(
        &self,
        record: &InventoryTransferRecord,
        line_item: &InventoryTransferLineItemRecord,
    ) -> Value {
        let status = self.inventory_transfer_effective_status(record);
        let (shipped, picked) = self.transfer_line_shipment_quantities(&record.id, line_item, None);
        let remaining = self
            .remaining_transfer_record_line_quantity(&record.id, line_item, None)
            .max(0);
        let shippable = if matches!(status.as_str(), "READY_TO_SHIP" | "IN_PROGRESS") {
            remaining
        } else {
            0
        };
        let variant = self
            .store
            .product_variant_by_inventory_item_id(&line_item.inventory_item_id);
        let title = variant
            .and_then(|variant| self.store.product_by_id(&variant.product_id))
            .map(|product| product.title.clone());
        let sku = variant
            .map(|variant| variant.sku.clone())
            .filter(|sku| !sku.is_empty());
        let tracked = variant
            .map(|variant| variant.inventory_item.tracked)
            .unwrap_or(true);
        let requires_shipping = variant
            .map(|variant| variant.inventory_item.requires_shipping)
            .unwrap_or(true);
        json!({
            "__typename": "InventoryTransferLineItem",
            "id": line_item.id,
            "title": title,
            "inventoryItem": {
                "id": line_item.inventory_item_id,
                "sku": sku,
                "tracked": tracked,
                "requiresShipping": requires_shipping
            },
            "totalQuantity": line_item.quantity,
            "shippableQuantity": shippable,
            "shippedQuantity": shipped,
            "processableQuantity": remaining,
            "pickedForShipmentQuantity": picked
        })
    }

    fn inventory_transfer_search_decision(
        &self,
        record: &InventoryTransferRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return StagedSearchDecision::Match;
        };
        let mut saw_supported_term = false;
        for term in inventory_search_terms(query) {
            match self.inventory_transfer_matches_search_term(record, &term) {
                Some(true) => saw_supported_term = true,
                Some(false) => return StagedSearchDecision::NoMatch,
                None => return StagedSearchDecision::Unsupported,
            }
        }
        StagedSearchDecision::from_bool(saw_supported_term)
    }

    fn inventory_transfer_matches_search_term(
        &self,
        record: &InventoryTransferRecord,
        term: &str,
    ) -> Option<bool> {
        let term = term.trim();
        if term.is_empty() {
            return Some(true);
        }
        let Some((field, raw_value)) = term.split_once(':') else {
            let value = inventory_unquoted_query_value(term);
            return Some(
                inventory_id_matches_query(&record.id, &value)
                    || inventory_search_string_matches(&record.name, &value)
                    || inventory_search_string_matches(&record.status, &value)
                    || record
                        .tags
                        .iter()
                        .any(|tag| inventory_search_string_matches(tag, &value)),
            );
        };
        let field = field.trim().to_ascii_lowercase();
        match field.as_str() {
            "id" => Some(inventory_id_matches_query(&record.id, raw_value)),
            "name" | "reference_name" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(inventory_search_string_matches(&record.name, &value))
            }
            "status" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(record.status.eq_ignore_ascii_case(&value))
            }
            "tag" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(
                    record
                        .tags
                        .iter()
                        .any(|tag| tag.eq_ignore_ascii_case(&value)),
                )
            }
            "tag_not" => {
                let value = inventory_unquoted_query_value(raw_value);
                Some(
                    !record
                        .tags
                        .iter()
                        .any(|tag| tag.eq_ignore_ascii_case(&value)),
                )
            }
            "created_at" | "date_created" => Some(inventory_datetime_matches_query(
                Some(record.created_at.as_str()),
                raw_value,
            )),
            "origin_id" | "source_id" => Some(inventory_id_matches_query(
                &record.origin_location_id,
                raw_value,
            )),
            "destination_id" => Some(inventory_id_matches_query(
                &record.destination_location_id,
                raw_value,
            )),
            "product_id" => Some(self.inventory_transfer_has_product(record, raw_value)),
            "product_variant_id" => Some(self.inventory_transfer_has_variant(record, raw_value)),
            "inventory_item_id" => Some(record.line_items.iter().any(|line_item| {
                inventory_id_matches_query(&line_item.inventory_item_id, raw_value)
            })),
            _ => None,
        }
    }

    fn inventory_transfer_has_product(
        &self,
        record: &InventoryTransferRecord,
        product_id: &str,
    ) -> bool {
        record.line_items.iter().any(|line_item| {
            self.store
                .product_variant_by_inventory_item_id(&line_item.inventory_item_id)
                .is_some_and(|variant| inventory_id_matches_query(&variant.product_id, product_id))
        })
    }

    fn inventory_transfer_has_variant(
        &self,
        record: &InventoryTransferRecord,
        variant_id: &str,
    ) -> bool {
        record.line_items.iter().any(|line_item| {
            self.store
                .product_variant_by_inventory_item_id(&line_item.inventory_item_id)
                .is_some_and(|variant| inventory_id_matches_query(&variant.id, variant_id))
        })
    }

    fn inventory_transfer_sort_key(
        &self,
        record: &InventoryTransferRecord,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        match sort_key.unwrap_or("ID") {
            "CREATED_AT" | "DATE_CREATED" => {
                vec![StagedSortValue::String(record.created_at.clone())]
            }
            "DESTINATION_NAME" => vec![StagedSortValue::String(
                self.inventory_location_display_name(&record.destination_location_id)
                    .to_ascii_lowercase(),
            )],
            "ID" => inventory_gid_sort_key(&record.id),
            "NAME" | "REFERENCE_NAME" => {
                vec![StagedSortValue::String(record.name.to_ascii_lowercase())]
            }
            "ORIGIN_NAME" | "SOURCE_NAME" => vec![StagedSortValue::String(
                self.inventory_location_display_name(&record.origin_location_id)
                    .to_ascii_lowercase(),
            )],
            "STATUS" => vec![StagedSortValue::String(record.status.to_ascii_lowercase())],
            "RELEVANCE" => vec![StagedSortValue::Null],
            _ => inventory_gid_sort_key(&record.id),
        }
    }

    fn ensure_transfer_inventory_levels(&mut self, record: &InventoryTransferRecord) {
        let default_location_id = self.default_inventory_location_id();
        for line_item in &record.line_items {
            if let Some(default_location_id) =
                default_location_id.as_deref().filter(|location_id| {
                    record.origin_location_id != *location_id
                        && record.destination_location_id != *location_id
                })
            {
                self.stage_inventory_level_for_write(&(
                    line_item.inventory_item_id.clone(),
                    default_location_id.to_string(),
                ));
                self.store
                    .staged
                    .inventory_levels
                    .entry((
                        line_item.inventory_item_id.clone(),
                        default_location_id.to_string(),
                    ))
                    .or_insert_with(empty_inventory_quantities);
            }
            self.stage_inventory_level_for_write(&(
                line_item.inventory_item_id.clone(),
                record.origin_location_id.clone(),
            ));
            let origin = self
                .store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.origin_location_id.clone(),
                ))
                .or_insert_with(empty_inventory_quantities);
            if origin.is_empty() {
                *origin = empty_inventory_quantities();
            }
            self.stage_inventory_level_for_write(&(
                line_item.inventory_item_id.clone(),
                record.destination_location_id.clone(),
            ));
            self.store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.destination_location_id.clone(),
                ))
                .or_insert_with(empty_inventory_quantities);
        }
    }

    fn inventory_transfer_validate(
        &self,
        origin_location_id: &str,
        destination_location_id: &str,
        line_item_inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let mut user_errors = Vec::new();
        let origin_is_active = self.inventory_transfer_location_is_active(origin_location_id);
        let destination_is_active =
            self.inventory_transfer_location_is_active(destination_location_id);
        if !origin_is_active {
            user_errors.push(user_error(
                ["input", "originLocationId"],
                "The location selected can't be found.",
                Some("LOCATION_NOT_FOUND"),
            ));
        }
        if !destination_is_active {
            user_errors.push(user_error(
                ["input", "destinationLocationId"],
                "The location selected can't be found.",
                Some("LOCATION_NOT_FOUND"),
            ));
        }
        if !origin_location_id.is_empty()
            && origin_location_id == destination_location_id
            && origin_is_active
        {
            user_errors.push(user_error(
                ["input", "destinationLocationId"],
                "The origin location cannot be the same as the destination location.",
                Some("TRANSFER_ORIGIN_CANNOT_BE_THE_SAME_AS_DESTINATION"),
            ));
        }

        let mut item_counts: BTreeMap<String, usize> = BTreeMap::new();
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty() {
                *item_counts.entry(item_id).or_insert(0) += 1;
            }
        }

        for (index, item_input) in line_item_inputs.iter().enumerate() {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(item_input, "quantity").unwrap_or(0);
            if item_counts.get(&item_id).copied().unwrap_or(0) > 1 {
                user_errors.push(user_error(
                    json!(["input", "lineItems", index.to_string(), "inventoryItemId"]),
                    "The inventory item is already present in the list. Each item must be unique.",
                    Some("DUPLICATE_ITEM"),
                ));
            }
            if origin_is_active
                && !self.inventory_transfer_item_is_stocked_at_origin(&item_id, origin_location_id)
            {
                user_errors.push(user_error(
                    json!(["input", "lineItems", index.to_string(), "inventoryItemId"]),
                    "The inventory item could not be found.",
                    Some("ITEM_NOT_FOUND"),
                ));
            }
            if quantity < 0 {
                user_errors.push(user_error(
                    json!(["input", "lineItems", index.to_string(), "quantity"]),
                    "The quantity can't be negative.",
                    Some("INVALID_QUANTITY"),
                ));
            }
        }
        user_errors
    }

    fn inventory_transfer_location_is_active(&self, location_id: &str) -> bool {
        if location_id.is_empty() {
            return false;
        }
        self.location_for_read(location_id)
            .as_ref()
            .and_then(|location| location.get("isActive"))
            .and_then(Value::as_bool)
            == Some(true)
    }

    fn inventory_transfer_item_is_stocked_at_origin(
        &self,
        inventory_item_id: &str,
        origin_location_id: &str,
    ) -> bool {
        if inventory_item_id.is_empty() || origin_location_id.is_empty() {
            return false;
        }
        if self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| variant.inventory_item.tracked)
            == Some(false)
        {
            return false;
        }
        let key = (
            inventory_item_id.to_string(),
            origin_location_id.to_string(),
        );
        self.effective_inventory_level(&key).is_some() && self.inventory_level_is_active(&key)
    }

    fn hydrate_inventory_transfer_references<'a>(
        &mut self,
        request: &Request,
        location_ids: impl IntoIterator<Item = &'a String>,
        line_item_inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let mut ids = Vec::new();
        for location_id in location_ids {
            if !location_id.is_empty()
                && !self.inventory_location_exists(location_id)
                && !ids.iter().any(|id| id == location_id)
            {
                ids.push(location_id.clone());
            }
        }
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty()
                && !self.inventory_item_exists(&item_id)
                && !ids.iter().any(|id| id == &item_id)
            {
                ids.push(item_id);
            }
        }
        if ids.is_empty() {
            return;
        }
        let mut response = self.upstream_post(
            request,
            json!({
                "query": INVENTORY_LIFECYCLE_REFERENCE_HYDRATE_NODES_QUERY,
                "variables": { "ids": ids }
            }),
        );
        // Older captures contain the same read with legacy formatting. Retry that
        // exact query only after a server failure; confirmed 4xx responses remain
        // authoritative and supported mutations are never forwarded.
        if response.status >= 500 {
            response = self.upstream_post(
                request,
                json!({
                    "query": INVENTORY_TRANSFER_HYDRATE_NODES_QUERY,
                    "variables": { "ids": ids }
                }),
            );
        }
        if response.status >= 400 {
            return;
        }
        self.observe_inventory_transfer_hydration_response(&response.body);
    }

    pub(super) fn observe_inventory_transfer_hydration_response(&mut self, body: &Value) {
        let nodes = body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        for node in nodes {
            let node_type = node
                .get("__typename")
                .and_then(Value::as_str)
                .or_else(|| {
                    node.get("id")
                        .and_then(Value::as_str)
                        .and_then(shopify_gid_resource_type)
                })
                .or_else(|| {
                    node.get("inventoryLevels")
                        .is_some()
                        .then_some("InventoryItem")
                });
            match node_type {
                Some("Location") => self.observe_base_inventory_location(&node),
                Some("InventoryItem") => self.observe_inventory_item_node(&node),
                Some("InventoryLevel") => self.observe_inventory_level_node(&node),
                _ => {}
            }
        }
    }

    fn apply_transfer_reservations(&mut self, record: &InventoryTransferRecord, direction: i64) {
        for line_item in &record.line_items {
            self.apply_inventory_reservation(
                &line_item.inventory_item_id,
                &record.origin_location_id,
                direction * line_item.quantity,
            );
        }
    }

    fn apply_inventory_reservation(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        reserved_delta: i64,
    ) {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        self.stage_inventory_level_for_write(&key);
        let level = self
            .store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities);
        *level.entry("available".to_string()).or_insert(0) -= reserved_delta;
        *level.entry("reserved".to_string()).or_insert(0) += reserved_delta;
        let available = level.get("available").copied().unwrap_or(0);
        let reserved = level.get("reserved").copied().unwrap_or(0);
        level
            .entry("on_hand".to_string())
            .or_insert(available + reserved);
    }
}

fn inventory_transfer_record_from_json(value: &Value) -> Option<InventoryTransferRecord> {
    let id = value.get("id").and_then(Value::as_str)?;
    if !is_shopify_gid_of_type(id, "InventoryTransfer") {
        return None;
    }
    let line_items = connection_node_values(value.get("lineItems"))
        .into_iter()
        .filter_map(inventory_transfer_line_item_record_from_json)
        .collect::<Vec<_>>();
    Some(InventoryTransferRecord {
        id: id.to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_at: value
            .get("dateCreated")
            .or_else(|| value.get("createdAt"))
            .or_else(|| value.get("created_at"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        origin_location_id: value
            .get("origin")
            .and_then(|origin| origin.get("id").or_else(|| origin.pointer("/location/id")))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        destination_location_id: value
            .get("destination")
            .and_then(|destination| {
                destination
                    .get("id")
                    .or_else(|| destination.pointer("/location/id"))
            })
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        tags: value
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_string))
            .collect(),
        line_items,
    })
}

fn inventory_transfer_tags_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut tags = list_string_field(input, "tags");
    tags.sort_by_key(|tag| tag.to_ascii_lowercase());
    tags
}

fn inventory_transfer_line_item_record_from_json(
    value: &Value,
) -> Option<InventoryTransferLineItemRecord> {
    let id = value.get("id").and_then(Value::as_str)?.to_string();
    Some(InventoryTransferLineItemRecord {
        id,
        inventory_item_id: value
            .get("inventoryItem")
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        quantity: value
            .get("totalQuantity")
            .or_else(|| value.get("quantity"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    })
}

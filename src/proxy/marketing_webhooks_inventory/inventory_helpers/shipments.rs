use super::*;

impl DraftProxy {
    pub(crate) fn inventory_shipment_create(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let in_transit = invocation.root_name == "inventoryShipmentCreateInTransit";
        let transfer_id = resolved_string_field(&input, "inventoryTransferId")
            .or_else(|| resolved_string_field(&input, "transferId"));
        let movement_id = resolved_string_field(&input, "movementId");
        let line_inputs = resolved_object_list_field(&input, "lineItems");
        let tracking = inventory_shipment_tracking_from_input(&input);
        let status = if in_transit { "IN_TRANSIT" } else { "DRAFT" };

        if let Some(errors) = self.inventory_shipment_create_validation_errors(
            &input,
            transfer_id.as_deref(),
            &line_inputs,
        ) {
            return ResolverOutcome::value(
                self.inventory_shipment_payload_with_errors("inventoryShipment", errors),
            );
        }

        let id = self.next_proxy_synthetic_gid("InventoryShipment");
        let mut line_items = Vec::new();
        for line_input in line_inputs {
            line_items.push(InventoryShipmentLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryShipmentLineItem"),
                inventory_item_id: resolved_string_field(&line_input, "inventoryItemId")
                    .unwrap_or_default(),
                transfer_line_item_id: resolved_string_field(
                    &line_input,
                    "inventoryTransferLineItemId",
                ),
                quantity: resolved_int_field(&line_input, "quantity").unwrap_or(0),
                accepted_quantity: 0,
                rejected_quantity: 0,
            });
        }
        let record = InventoryShipmentRecord {
            id: id.clone(),
            name: format!(
                "#S{}",
                self.store
                    .staged
                    .inventory_shipments
                    .len()
                    .saturating_add(1)
            ),
            status: status.to_string(),
            transfer_id,
            movement_id,
            tracking,
            line_items,
        };
        self.ensure_shipment_inventory_levels(&record);
        if in_transit {
            self.apply_shipment_incoming_delta(&record, record.unreceived_quantity());
        }
        let payload = self.inventory_shipment_payload_value(&record, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            invocation.root_name,
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_add_items(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(self.inventory_shipment_missing_mutation_payload(
                "inventoryShipment",
                &[("addedItems", json!([]))],
            ));
        };
        let line_inputs = resolved_object_list_field(&arguments, "lineItems");
        if let Some(errors) =
            self.inventory_shipment_line_validation_errors(&record, &line_inputs, "lineItems")
        {
            return ResolverOutcome::value(self.inventory_shipment_payload_with_errors_and_extra(
                "inventoryShipment",
                errors,
                &[("addedItems", json!([]))],
            ));
        }
        let was_in_transit = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut added_items = Vec::new();
        for line_input in line_inputs {
            let line_item = InventoryShipmentLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryShipmentLineItem"),
                inventory_item_id: resolved_string_field(&line_input, "inventoryItemId")
                    .unwrap_or_default(),
                transfer_line_item_id: resolved_string_field(
                    &line_input,
                    "inventoryTransferLineItemId",
                ),
                quantity: resolved_int_field(&line_input, "quantity").unwrap_or(0),
                accepted_quantity: 0,
                rejected_quantity: 0,
            };
            if let (true, Some(destination_location_id)) =
                (was_in_transit, destination_location_id.as_deref())
            {
                self.apply_inventory_quantity_delta(
                    &line_item.inventory_item_id,
                    destination_location_id,
                    "incoming",
                    line_item.unreceived_quantity(),
                );
            }
            added_items.push(self.inventory_shipment_line_item_full_json(&line_item));
            record.line_items.push(line_item);
        }
        let payload = json!({
            "inventoryShipment": self.inventory_shipment_full_json(&record),
            "addedItems": added_items,
            "userErrors": []
        });
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentAddItems",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_remove_items(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(self.inventory_shipment_missing_mutation_payload(
                "inventoryShipment",
                &[("removedLineItemIds", json!([]))],
            ));
        };
        let remove_ids = resolved_string_list_arg(&arguments, "shipmentLineItemIds");
        let was_in_transit = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut kept = Vec::new();
        let mut removed_ids = Vec::new();
        for line_item in record.line_items {
            if remove_ids
                .iter()
                .any(|candidate| candidate == &line_item.id)
            {
                if let (true, Some(destination_location_id)) =
                    (was_in_transit, destination_location_id.as_deref())
                {
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        destination_location_id,
                        "incoming",
                        -line_item.unreceived_quantity(),
                    );
                }
                removed_ids.push(json!(line_item.id));
            } else {
                kept.push(line_item);
            }
        }
        record.line_items = kept;
        let payload = json!({
            "inventoryShipment": self.inventory_shipment_full_json(&record),
            "removedLineItemIds": removed_ids,
            "userErrors": []
        });
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentRemoveItems",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_update_item_quantities(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(self.inventory_shipment_missing_mutation_payload(
                "shipment",
                &[("updatedLineItems", json!([]))],
            ));
        };
        let items = resolved_object_list_field(&arguments, "items");
        let mut proposed_quantities_by_line_id = BTreeMap::new();
        for (index, item) in items.iter().enumerate() {
            let line_item_id =
                resolved_string_field(item, "shipmentLineItemId").unwrap_or_default();
            let Some(line_item) = record
                .line_items
                .iter()
                .find(|line_item| line_item.id == line_item_id)
            else {
                return ResolverOutcome::value(
                    self.inventory_shipment_payload_with_errors_and_extra(
                        "shipment",
                        vec![inventory_shipment_user_error(
                            vec!["items", &index.to_string(), "shipmentLineItemId"],
                            "The specified inventory shipment line item could not be found.",
                            "NOT_FOUND",
                        )],
                        &[("updatedLineItems", json!([]))],
                    ),
                );
            };
            let new_quantity = resolved_int_field(item, "quantity").unwrap_or(0);
            proposed_quantities_by_line_id.insert(
                line_item.id.clone(),
                new_quantity.max(line_item.received_quantity()),
            );
            if let (Some(transfer_id), Some(transfer_line_item_id)) = (
                record.transfer_id.as_deref(),
                line_item.transfer_line_item_id.as_deref(),
            ) {
                let proposed_total = record
                    .line_items
                    .iter()
                    .filter(|candidate| {
                        candidate.transfer_line_item_id.as_deref() == Some(transfer_line_item_id)
                    })
                    .map(|candidate| {
                        proposed_quantities_by_line_id
                            .get(&candidate.id)
                            .copied()
                            .unwrap_or(candidate.quantity)
                    })
                    .sum::<i64>();
                if proposed_total
                    > self.remaining_transfer_line_quantity(
                        transfer_id,
                        transfer_line_item_id,
                        Some(&record.id),
                    )
                {
                    return ResolverOutcome::value(
                        self.inventory_shipment_payload_with_errors_and_extra(
                            "shipment",
                            vec![inventory_shipment_user_error(
                                vec!["items", &index.to_string(), "quantity"],
                                "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                                "QUANTITY_EXCEEDS_REMAINING",
                            )],
                            &[("updatedLineItems", json!([]))],
                        ),
                    );
                }
            }
        }

        let has_incoming = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut updated = Vec::new();
        for item in items {
            let line_item_id =
                resolved_string_field(&item, "shipmentLineItemId").unwrap_or_default();
            let new_quantity = resolved_int_field(&item, "quantity").unwrap_or(0);
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.id == line_item_id)
            {
                let old_unreceived = line_item.unreceived_quantity();
                line_item.quantity = new_quantity.max(line_item.received_quantity());
                let new_unreceived = line_item.unreceived_quantity();
                if let (true, Some(destination_location_id)) =
                    (has_incoming, destination_location_id.as_deref())
                {
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        destination_location_id,
                        "incoming",
                        new_unreceived - old_unreceived,
                    );
                }
                updated.push(self.inventory_shipment_line_item_full_json(line_item));
            }
        }
        let payload = json!({
            "shipment": self.inventory_shipment_full_json(&record),
            "updatedLineItems": updated,
            "userErrors": []
        });
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentUpdateItemQuantities",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_set_tracking(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_shipment_missing_mutation_payload("inventoryShipment", &[]),
            );
        };
        let input = resolved_object_field(&arguments, "trackingInput")
            .or_else(|| resolved_object_field(&arguments, "tracking"))
            .unwrap_or_default();
        let errors = inventory_shipment_tracking_errors(&input);
        if !errors.is_empty() {
            return ResolverOutcome::value(
                self.inventory_shipment_payload_with_errors("inventoryShipment", errors),
            );
        }
        record.tracking = inventory_shipment_tracking_from_input(&input);
        let payload = self.inventory_shipment_payload_value(&record, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentSetTracking",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_mark_in_transit(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_shipment_missing_mutation_payload("inventoryShipment", &[]),
            );
        };
        if record.status != "DRAFT" {
            return ResolverOutcome::value(self.inventory_shipment_payload_with_errors(
                "inventoryShipment",
                vec![inventory_shipment_user_error(
                    vec!["id"],
                    "Only draft shipments can be marked in transit.",
                    "INVALID_STATE",
                )],
            ));
        }
        record.status = "IN_TRANSIT".to_string();
        self.apply_shipment_incoming_delta(&record, record.unreceived_quantity());
        let payload = self.inventory_shipment_payload_value(&record, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentMarkInTransit",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_receive(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return ResolverOutcome::value(
                self.inventory_shipment_missing_mutation_payload("inventoryShipment", &[]),
            );
        };
        if !matches!(record.status.as_str(), "IN_TRANSIT" | "PARTIALLY_RECEIVED") {
            return ResolverOutcome::value(self.inventory_shipment_payload_with_errors(
                "inventoryShipment",
                vec![inventory_shipment_user_error(
                    vec!["id"],
                    "Only in-transit shipments can be received.",
                    "INVALID_STATE",
                )],
            ));
        }
        let receive_items = resolved_object_list_field(&arguments, "lineItems");
        let destination_location_id = self.shipment_destination_location_id(&record);
        for receive_item in receive_items {
            let line_item_id =
                resolved_string_field(&receive_item, "shipmentLineItemId").unwrap_or_default();
            let quantity = resolved_int_field(&receive_item, "quantity").unwrap_or(0);
            let reason = resolved_string_field(&receive_item, "reason")
                .unwrap_or_else(|| "ACCEPTED".to_string());
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.id == line_item_id)
            {
                let applied = quantity.min(line_item.unreceived_quantity()).max(0);
                if applied == 0 {
                    continue;
                }
                if let Some(destination_location_id) = destination_location_id.as_deref() {
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        destination_location_id,
                        "incoming",
                        -applied,
                    );
                }
                if reason == "REJECTED" {
                    line_item.rejected_quantity += applied;
                } else {
                    line_item.accepted_quantity += applied;
                    if let Some(destination_location_id) = destination_location_id.as_deref() {
                        self.apply_inventory_quantity_delta(
                            &line_item.inventory_item_id,
                            destination_location_id,
                            "available",
                            applied,
                        );
                        self.apply_inventory_quantity_delta(
                            &line_item.inventory_item_id,
                            destination_location_id,
                            "on_hand",
                            applied,
                        );
                    }
                }
            }
        }
        record.status = if record.unreceived_quantity() == 0 {
            "RECEIVED".to_string()
        } else {
            "PARTIALLY_RECEIVED".to_string()
        };
        let payload = self.inventory_shipment_payload_value(&record, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "inventoryShipmentReceive",
            "products",
            vec![id],
        ))
    }

    pub(crate) fn inventory_shipment_delete(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_shipments.remove(&id) else {
            return ResolverOutcome::value(json!({
                "id": Value::Null,
                "userErrors": [inventory_shipment_user_error(
                    vec!["id"],
                    "The specified inventory shipment could not be found.",
                    "NOT_FOUND",
                )]
            }));
        };
        if inventory_shipment_has_incoming(&record) {
            self.apply_shipment_incoming_delta(&record, -record.unreceived_quantity());
        }
        let deleted_id = record.id.clone();
        ResolverOutcome::value(json!({
            "id": id,
            "userErrors": []
        }))
        .with_log_draft(LogDraft::staged(
            "inventoryShipmentDelete",
            "products",
            vec![deleted_id],
        ))
    }

    fn inventory_shipment_create_validation_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        transfer_id: Option<&str>,
        line_inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Vec<Value>> {
        let tracking_errors = inventory_shipment_tracking_errors(
            &resolved_object_field(input, "trackingInput").unwrap_or_default(),
        );
        if !tracking_errors.is_empty() {
            return Some(tracking_errors);
        }
        let transfer_id = transfer_id?;
        let Some(transfer) = self.store.staged.inventory_transfers.get(transfer_id) else {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "The specified inventory transfer could not be found.",
                "NOT_FOUND",
            )]);
        };
        if !matches!(transfer.status.as_str(), "DRAFT" | "READY_TO_SHIP") {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "Inventory shipments can only be created for open or ready to ship transfers.",
                "INVALID_STATE",
            )]);
        }
        let mut proposed_quantities_by_transfer_line = BTreeMap::new();
        for (index, line_input) in line_inputs.iter().enumerate() {
            let transfer_line_item_id =
                resolved_string_field(line_input, "inventoryTransferLineItemId");
            let matching_line = transfer_line_item_id.as_ref().and_then(|id| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == *id)
            });
            if transfer_line_item_id.is_some() && matching_line.is_none() {
                return Some(vec![inventory_shipment_user_error(
                    vec![
                        "lineItems",
                        &index.to_string(),
                        "inventoryTransferLineItemId",
                    ],
                    "The specified inventory transfer line item could not be found.",
                    "NOT_FOUND",
                )]);
            }
            let quantity = resolved_int_field(line_input, "quantity").unwrap_or(0);
            if let Some(transfer_line) = matching_line {
                let proposed_quantity = proposed_quantities_by_transfer_line
                    .entry(transfer_line.id.clone())
                    .or_insert(0);
                *proposed_quantity += quantity;
                if *proposed_quantity
                    > self.remaining_transfer_line_quantity(transfer_id, &transfer_line.id, None)
                {
                    return Some(vec![inventory_shipment_user_error(
                        vec!["lineItems", &index.to_string(), "quantity"],
                        "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                        "QUANTITY_EXCEEDS_REMAINING",
                    )]);
                }
            }
        }
        None
    }

    fn inventory_shipment_line_validation_errors(
        &self,
        record: &InventoryShipmentRecord,
        line_inputs: &[BTreeMap<String, ResolvedValue>],
        field_name: &'static str,
    ) -> Option<Vec<Value>> {
        let transfer_id = record.transfer_id.as_deref()?;
        let Some(transfer) = self.store.staged.inventory_transfers.get(transfer_id) else {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "The specified inventory transfer could not be found.",
                "NOT_FOUND",
            )]);
        };
        let mut proposed_quantities_by_transfer_line = BTreeMap::new();
        for (index, line_input) in line_inputs.iter().enumerate() {
            let transfer_line_item_id =
                resolved_string_field(line_input, "inventoryTransferLineItemId");
            let matching_line = transfer_line_item_id.as_ref().and_then(|id| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == *id)
            });
            if transfer_line_item_id.is_some() && matching_line.is_none() {
                return Some(vec![inventory_shipment_user_error(
                    vec![
                        field_name,
                        &index.to_string(),
                        "inventoryTransferLineItemId",
                    ],
                    "The specified inventory transfer line item could not be found.",
                    "NOT_FOUND",
                )]);
            }
            if let Some(transfer_line) = matching_line {
                let quantity = resolved_int_field(line_input, "quantity").unwrap_or(0);
                let current_shipment_quantity = record
                    .line_items
                    .iter()
                    .filter(|line_item| {
                        line_item.transfer_line_item_id.as_deref()
                            == Some(transfer_line.id.as_str())
                    })
                    .map(|line_item| line_item.quantity)
                    .sum::<i64>();
                let remaining_for_add = self.remaining_transfer_line_quantity(
                    transfer_id,
                    &transfer_line.id,
                    Some(&record.id),
                ) - current_shipment_quantity;
                let proposed_quantity = proposed_quantities_by_transfer_line
                    .entry(transfer_line.id.clone())
                    .or_insert(0);
                *proposed_quantity += quantity;
                if *proposed_quantity > remaining_for_add {
                    return Some(vec![inventory_shipment_user_error(
                        vec![field_name, &index.to_string(), "quantity"],
                        "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                        "QUANTITY_EXCEEDS_REMAINING",
                    )]);
                }
            }
        }
        None
    }

    pub(super) fn remaining_transfer_line_quantity(
        &self,
        transfer_id: &str,
        transfer_line_item_id: &str,
        excluding_shipment_id: Option<&str>,
    ) -> i64 {
        self.store
            .staged
            .inventory_transfers
            .get(transfer_id)
            .and_then(|transfer| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == transfer_line_item_id)
                    .map(|line_item| {
                        self.remaining_transfer_record_line_quantity(
                            transfer_id,
                            line_item,
                            excluding_shipment_id,
                        )
                    })
            })
            .unwrap_or(0)
    }

    pub(super) fn remaining_transfer_record_line_quantity(
        &self,
        transfer_id: &str,
        transfer_line: &InventoryTransferLineItemRecord,
        excluding_shipment_id: Option<&str>,
    ) -> i64 {
        transfer_line.quantity
            - self.transfer_line_shipment_consumed_quantity(
                transfer_id,
                transfer_line,
                excluding_shipment_id,
            )
    }

    fn shipment_belongs_to_transfer(
        &self,
        shipment: &InventoryShipmentRecord,
        transfer_id: &str,
    ) -> bool {
        shipment.transfer_id.as_deref() == Some(transfer_id)
            || shipment.movement_id.as_deref() == Some(transfer_id)
    }

    fn shipment_line_matches_transfer_line(
        &self,
        shipment_line: &InventoryShipmentLineItemRecord,
        transfer_line: &InventoryTransferLineItemRecord,
    ) -> bool {
        shipment_line.transfer_line_item_id.as_deref() == Some(transfer_line.id.as_str())
            || (shipment_line.transfer_line_item_id.is_none()
                && shipment_line.inventory_item_id == transfer_line.inventory_item_id)
    }

    pub(super) fn transfer_line_shipment_quantities(
        &self,
        transfer_id: &str,
        transfer_line: &InventoryTransferLineItemRecord,
        excluding_shipment_id: Option<&str>,
    ) -> (i64, i64) {
        let mut shipped = 0;
        let mut picked = 0;
        for shipment in self
            .store
            .staged
            .inventory_shipments
            .values()
            .filter(|shipment| excluding_shipment_id != Some(shipment.id.as_str()))
            .filter(|shipment| self.shipment_belongs_to_transfer(shipment, transfer_id))
        {
            let quantity = shipment
                .line_items
                .iter()
                .filter(|shipment_line| {
                    self.shipment_line_matches_transfer_line(shipment_line, transfer_line)
                })
                .map(|shipment_line| shipment_line.quantity)
                .sum::<i64>();
            if shipment.status == "DRAFT" {
                picked += quantity;
            } else {
                shipped += quantity;
            }
        }
        (shipped, picked)
    }

    fn transfer_line_shipment_consumed_quantity(
        &self,
        transfer_id: &str,
        transfer_line: &InventoryTransferLineItemRecord,
        excluding_shipment_id: Option<&str>,
    ) -> i64 {
        let (shipped, picked) = self.transfer_line_shipment_quantities(
            transfer_id,
            transfer_line,
            excluding_shipment_id,
        );
        shipped + picked
    }

    fn inventory_shipment_payload_value(
        &self,
        record: &InventoryShipmentRecord,
        shipment_field: &str,
    ) -> Value {
        json!({
            shipment_field: self.inventory_shipment_full_json(record),
            "userErrors": []
        })
    }

    fn inventory_shipment_payload_with_errors(
        &self,
        shipment_field: &str,
        errors: Vec<Value>,
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(shipment_field, errors, &[])
    }

    fn inventory_shipment_payload_with_errors_and_extra(
        &self,
        shipment_field: &str,
        errors: Vec<Value>,
        extra: &[(&str, Value)],
    ) -> Value {
        let mut payload = serde_json::Map::from_iter([
            (shipment_field.to_string(), Value::Null),
            ("userErrors".to_string(), Value::Array(errors)),
        ]);
        for (name, value) in extra {
            payload.insert((*name).to_string(), value.clone());
        }
        Value::Object(payload)
    }

    fn inventory_shipment_missing_mutation_payload(
        &self,
        shipment_field: &str,
        extra: &[(&str, Value)],
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(
            shipment_field,
            vec![inventory_shipment_user_error(
                vec!["id"],
                "The specified inventory shipment could not be found.",
                "NOT_FOUND",
            )],
            extra,
        )
    }

    pub(super) fn inventory_shipment_value_by_id(&self, id: &str) -> Value {
        self.store
            .staged
            .inventory_shipments
            .get(id)
            .map(|record| self.inventory_shipment_full_json(record))
            .unwrap_or(Value::Null)
    }

    pub(super) fn inventory_shipment_line_item_value_by_id(&self, id: &str) -> Value {
        self.store
            .staged
            .inventory_shipments
            .values()
            .find_map(|record| {
                record
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == id)
                    .map(|line_item| self.inventory_shipment_line_item_full_json(line_item))
            })
            .unwrap_or(Value::Null)
    }

    fn inventory_shipment_full_json(&self, record: &InventoryShipmentRecord) -> Value {
        let line_items = record
            .line_items
            .iter()
            .map(|line_item| self.inventory_shipment_line_item_full_json(line_item))
            .collect::<Vec<_>>();
        json!({
            "__typename": "InventoryShipment",
            "id": record.id,
            "name": record.name,
            "movementId": record.movement_id,
            "status": record.status,
            "lineItemsCount": count_object(record.line_items.len()),
            "lineItemTotalQuantity": record.line_item_total_quantity(),
            "totalAcceptedQuantity": record.total_accepted_quantity(),
            "totalReceivedQuantity": record.total_received_quantity(),
            "totalRejectedQuantity": record.total_rejected_quantity(),
            "tracking": record.tracking.as_ref().map(|tracking| json!({
                "trackingNumber": tracking.tracking_number,
                "company": tracking.company,
                "trackingUrl": tracking.tracking_url,
                "arrivesAt": tracking.arrives_at
            })),
            "lineItems": connection_json(line_items)
        })
    }

    fn inventory_shipment_line_item_full_json(
        &self,
        line_item: &InventoryShipmentLineItemRecord,
    ) -> Value {
        // sku/tracked come from the inventory item's hydrated/staged variant
        // (populated by the ProductsHydrateNodes read-through cache), never derived
        // from the id — the proxy emulates an arbitrary backend, not a fixture.
        let variant = self
            .store
            .product_variant_by_inventory_item_id(&line_item.inventory_item_id);
        let sku = variant
            .map(|variant| variant.sku.clone())
            .filter(|sku| !sku.is_empty());
        let tracked = variant
            .map(|variant| variant.inventory_item.tracked)
            .unwrap_or(true);
        json!({
            "__typename": "InventoryShipmentLineItem",
            "id": line_item.id,
            "quantity": line_item.quantity,
            "acceptedQuantity": line_item.accepted_quantity,
            "rejectedQuantity": line_item.rejected_quantity,
            "unreceivedQuantity": line_item.unreceived_quantity(),
            "inventoryItem": {
                "id": line_item.inventory_item_id,
                "sku": sku,
                "tracked": tracked
            }
        })
    }

    fn ensure_shipment_inventory_levels(&mut self, record: &InventoryShipmentRecord) {
        let Some(location_id) = self.shipment_destination_location_id(record) else {
            return;
        };
        for line_item in &record.line_items {
            let key = (line_item.inventory_item_id.clone(), location_id.clone());
            if self.effective_inventory_level(&key).is_some() {
                self.stage_inventory_level_for_write(&key);
                continue;
            }
            // Seed a destination level only for product-backed movement shipments that
            // have no recorded level yet. available/on_hand mirror the hydrated variant's
            // current inventory quantity (committed defaults to 0, so on_hand ==
            // available) — the relationship Shopify reports for a freshly stocked
            // single-location item before the shipment's incoming delta is applied.
            let on_hand = if record.transfer_id.is_none() {
                self.store
                    .product_variant_by_inventory_item_id(&line_item.inventory_item_id)
                    .map(|variant| variant.inventory_quantity)
                    .unwrap_or(0)
            } else {
                0
            };
            self.store.staged.inventory_levels.insert(
                key,
                BTreeMap::from([
                    ("available".to_string(), on_hand),
                    ("reserved".to_string(), 0),
                    ("on_hand".to_string(), on_hand),
                    ("incoming".to_string(), 0),
                ]),
            );
        }
    }

    fn apply_shipment_incoming_delta(&mut self, record: &InventoryShipmentRecord, delta: i64) {
        if delta == 0 {
            return;
        }
        let Some(location_id) = self.shipment_destination_location_id(record) else {
            return;
        };
        for line_item in &record.line_items {
            let line_delta = if delta < 0 {
                -line_item.unreceived_quantity()
            } else {
                line_item.unreceived_quantity()
            };
            self.apply_inventory_quantity_delta(
                &line_item.inventory_item_id,
                &location_id,
                "incoming",
                line_delta,
            );
        }
    }

    fn apply_inventory_quantity_delta(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        name: &str,
        delta: i64,
    ) {
        if delta == 0 {
            return;
        }
        let updated_at = self.next_inventory_quantity_timestamp();
        let key = (inventory_item_id.to_string(), location_id.to_string());
        self.stage_inventory_level_for_write(&key);
        let level = self
            .store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities);
        *level.entry(name.to_string()).or_insert(0) += delta;
        self.stamp_inventory_quantity(inventory_item_id, location_id, name, &updated_at);
    }

    fn shipment_destination_location_id(&self, record: &InventoryShipmentRecord) -> Option<String> {
        record
            .transfer_id
            .as_deref()
            .and_then(|transfer_id| {
                self.store
                    .staged
                    .inventory_transfers
                    .get(transfer_id)
                    .map(|transfer| transfer.destination_location_id.clone())
            })
            .or_else(|| self.default_inventory_location_id())
    }
}
